//! SQLite storage. Every SQL statement of the crate lives under this module.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use crate::error::{Error, Result};

mod decisions;
mod feeds;
mod fetch;
mod health;
pub mod pages;
mod reading;

pub use decisions::{DecisionRecord, FeedSnapshot};
pub use feeds::{CadenceRow, FeedRow, FolderRow, ImportReport};
pub use fetch::{AttemptRecord, BACKLOG_DAYS, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
pub use health::TriageFacts;
pub use reading::{FulltextRow, plain_text};

/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 4;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../schema/0001_init.sql")),
    (2, include_str!("../../../../schema/0002_reading.sql")),
    (3, include_str!("../../../../schema/0003_pages.sql")),
    (4, include_str!("../../../../schema/0004_edition.sql")),
];

/// The migration after which existing entries get their search rows.
const FTS_MIGRATION: i64 = 2;

/// The migration after which existing folders and feeds get their page numbers.
const PAGES_MIGRATION: i64 = 3;

/// The migration after which pages 104 to 109 are reserved and stored diagnoses get their
/// rhythm group.
const CADENCE_MIGRATION: i64 = 4;

/// `schema_meta.version`, or 0 for a database without it.
fn read_version(conn: &Connection) -> Result<i64> {
    let has_meta: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
        [],
        |r| r.get(0),
    )?;
    if has_meta == 0 {
        return Ok(0);
    }
    Ok(conn.query_row("SELECT version FROM schema_meta", [], |r| r.get(0))?)
}

pub struct Store {
    conn: Connection,
    path: Option<PathBuf>,
}

/// `$HOME/Library/Application Support/Ressac/ressac.sqlite`.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/Ressac/ressac.sqlite")
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        let mut store = Store {
            conn,
            path: Some(path.to_path_buf()),
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut store = Store {
            conn: Connection::open_in_memory()?,
            path: None,
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    fn configure(&self) -> Result<()> {
        // journal_mode returns a row, so it is read rather than executed.
        self.conn
            .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        self.conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        read_version(&self.conn)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew {
                found: current,
                known: SCHEMA_VERSION,
            });
        }
        if MIGRATIONS.iter().all(|(v, _)| *v <= current) {
            return Ok(());
        }
        if current > 0 {
            self.backup(current)?;
        }
        for (version, sql) in MIGRATIONS {
            // The write lock is taken first and the version re-read under it, so a second
            // process migrating at the same time finds the work done instead of failing.
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if read_version(&tx)? >= *version {
                continue;
            }
            tx.execute_batch(sql)?;
            if *version == FTS_MIGRATION {
                reading::backfill_fts(&tx)?;
            }
            if *version == PAGES_MIGRATION {
                pages::assign_missing(&tx)?;
            }
            if *version == CADENCE_MIGRATION {
                // Pages 104–109 become reserved (Journal and the rhythm groups). Nothing can
                // hold them (`ASSIGNABLE` starts at 110); a hand-edited database still gets
                // its squatter renumbered.
                tx.execute_batch(
                    "UPDATE folder SET page = NULL WHERE page BETWEEN 104 AND 109;
                     UPDATE feed SET page = NULL WHERE page BETWEEN 104 AND 109;",
                )?;
                pages::assign_missing(&tx)?;
                health::backfill_cadence_groups(&tx, chrono::Utc::now())?;
            }
            tx.execute("UPDATE schema_meta SET version = ?1", [version])?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Copies the database to `ressac.sqlite.bak-v<version>` before migrating it. An
    /// existing copy is kept: it is a snapshot of the same version from an earlier attempt.
    fn backup(&self, version: i64) -> Result<()> {
        if let Some(path) = &self.path {
            let dest = path.with_extension(format!("sqlite.bak-v{version}"));
            if dest.exists() {
                tracing::info!(backup = %dest.display(), "backup already exists, keeping it");
                return Ok(());
            }
            self.conn
                .execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
        }
        Ok(())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Starts a write transaction holding the write lock from the first statement
    /// (IMMEDIATE), so a transaction that reads before it writes never meets
    /// `SQLITE_BUSY_SNAPSHOT` when another process commits in between: it waits instead.
    pub(crate) fn write_tx(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(rusqlite::Transaction::new_unchecked(
            self.conn(),
            TransactionBehavior::Immediate,
        )?)
    }

    /// Entry ids whose search row matches an FTS5 query, in id order.
    #[cfg(test)]
    pub(crate) fn search_ids(&self, query: &str) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?1 ORDER BY rowid")?;
        let ids = stmt
            .query_map([query], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_names(store: &Store) -> Vec<String> {
        let mut stmt = store
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn fresh_store_has_schema_v4() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let names = object_names(&store);
        for expected in [
            "edition",
            "edition_entry",
            "edition_overflow",
            "entry",
            "entry_fts",
            "entry_fulltext",
            "feed",
            "feed_health",
            "fetch_log",
            "folder",
            "garden_decision",
            "schema_meta",
            "v_cadence",
            "v_edition_overflow",
            "v_edition_today",
            "v_entry",
            "v_feed_overview",
            "v_folder_path",
            "v_garden_row",
            "v_page",
            "v_unread_count",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn reopening_a_file_is_idempotent_and_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/ressac.sqlite");
        drop(Store::open(&path).unwrap());
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let mode: String = store
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        let store = Store::open(&path).unwrap();
        store
            .conn()
            .execute("UPDATE schema_meta SET version = 99", [])
            .unwrap();
        drop(store);
        match Store::open(&path) {
            Err(Error::SchemaTooNew {
                found: 99,
                known: SCHEMA_VERSION,
            }) => {}
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("newer schema was accepted"),
        }
    }

    #[test]
    fn default_path_is_under_application_support() {
        let p = default_path();
        assert!(
            p.ends_with("Library/Application Support/Ressac/ressac.sqlite"),
            "{p:?}"
        );
    }

    /// Review Focus 4: a sub-project 1 database opens at v2 with every entry searchable.
    #[test]
    fn migration_from_v1_backfills_fts_and_keeps_read_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v1 = Connection::open(&path).unwrap();
            v1.execute_batch(MIGRATIONS[0].1).unwrap();
            v1.execute_batch(
                "INSERT INTO feed (id, position, title, xml_url, xml_url_original, url_key, added_at) \
                 VALUES (1, 0, 'F', 'https://f.example.org/feed', 'https://f.example.org/feed', \
                 'f.example.org/feed', '2026-01-01T00:00:00.000Z');
                 INSERT INTO entry (id, feed_id, guid, title, author, published_at, summary_html, \
                 content_html, read_at, fetched_at) VALUES \
                 (1, 1, 'a', 'Tides', NULL, '2020-01-01T00:00:00.000Z', '<p>harbour</p>', NULL, NULL, \
                 '2026-01-01T00:00:00.000Z'), \
                 (2, 1, 'b', NULL, 'Ada', '2020-01-02T00:00:00.000Z', '<p>short</p>', \
                 '<p>Lighthouse &amp; pier</p>', '2026-01-05T00:00:00.000Z', '2026-01-01T00:00:00.000Z'), \
                 (3, 1, 'c', NULL, NULL, '2020-01-03T00:00:00.000Z', NULL, NULL, NULL, \
                 '2026-01-01T00:00:00.000Z'), \
                 (4, 1, 'd', 'Menu', NULL, '2020-01-04T00:00:00.000Z', '<p>un caf\u{e9}</p>', NULL, NULL, \
                 '2026-01-01T00:00:00.000Z');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(path.with_extension("sqlite.bak-v1").exists());
        let read: Vec<Option<String>> = store
            .conn()
            .prepare("SELECT read_at FROM entry ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            read,
            [
                None,
                Some("2026-01-05T00:00:00.000Z".to_string()),
                None,
                None
            ],
            "no backlog rule on migration"
        );
        let indexed: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM entry_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            indexed, 4,
            "NULL title, summary and content still get a row"
        );
        assert_eq!(store.search_ids("cafe").unwrap(), [4], "remove_diacritics");
        assert_eq!(store.search_ids("harbour").unwrap(), [1]);
        assert_eq!(store.search_ids("lighthouse").unwrap(), [2]);
        assert_eq!(store.search_ids("author:ada").unwrap(), [2]);
        assert!(
            store.search_ids("short").unwrap().is_empty(),
            "content_html wins over summary_html"
        );
    }

    #[test]
    fn migration_survives_an_existing_backup_and_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v1 = Connection::open(&path).unwrap();
            v1.execute_batch(MIGRATIONS[0].1).unwrap();
        }
        let bak = path.with_extension("sqlite.bak-v1");
        std::fs::write(&bak, b"earlier attempt").unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(std::fs::read(&bak).unwrap(), b"earlier attempt");
        drop(store);
        assert_eq!(
            Store::open(&path).unwrap().schema_version().unwrap(),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn a_failing_migration_leaves_v1_untouched_and_is_retryable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v1 = Connection::open(&path).unwrap();
            v1.execute_batch(MIGRATIONS[0].1).unwrap();
            // Poison: 0002 creates this table, after its ALTERs have already run.
            v1.execute_batch("CREATE TABLE entry_fulltext (x)").unwrap();
        }
        assert!(Store::open(&path).is_err());
        let raw = Connection::open(&path).unwrap();
        let version: i64 = raw
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let leftovers: i64 = raw
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name IN ('entry_fts', 'v_entry', 'entry_unread')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftovers, 0, "no partial v2 objects");
        let columns: i64 = raw
            .query_row(
                "SELECT count(*) FROM pragma_table_info('feed') WHERE name = 'prefer_full_text'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 0, "ALTER rolled back");
        raw.execute_batch("DROP TABLE entry_fulltext").unwrap();
        drop(raw);
        assert_eq!(
            Store::open(&path).unwrap().schema_version().unwrap(),
            SCHEMA_VERSION
        );
    }

    /// A v2 database: two root items, a subfolder and a buried feed, no page column yet.
    fn v2_database(path: &Path) -> Connection {
        let v2 = Connection::open(path).unwrap();
        v2.execute_batch(MIGRATIONS[0].1).unwrap();
        v2.execute_batch(MIGRATIONS[1].1).unwrap();
        v2.execute_batch(
            "INSERT INTO folder (id, parent_id, name, position) VALUES (1, NULL, 'Infos', 0), (2, 1, 'Sub', 1);
             INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, state, added_at) VALUES \
             (1, NULL, 1, 'Root', 'https://r.example.org/feed', 'https://r.example.org/feed', 'r.example.org/feed', 'active', '2026-01-01T00:00:00.000Z'), \
             (2, 1, 0, 'A', 'https://a.example.org/feed', 'https://a.example.org/feed', 'a.example.org/feed', 'active', '2026-01-01T00:00:00.000Z'), \
             (3, 2, 0, 'B', 'https://b.example.org/feed', 'https://b.example.org/feed', 'b.example.org/feed', 'buried', '2026-01-01T00:00:00.000Z');",
        )
        .unwrap();
        v2
    }

    #[test]
    fn migration_from_v2_numbers_every_folder_and_feed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        drop(v2_database(&path));
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(path.with_extension("sqlite.bak-v2").exists());
        let missing: i64 = store
            .conn()
            .query_row(
                "SELECT (SELECT count(*) FROM folder WHERE page IS NULL) + (SELECT count(*) FROM feed WHERE page IS NULL)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing, 0);
        let pages: Vec<(u16, String, i64)> = store
            .conn()
            .prepare("SELECT page, kind, ref_id FROM v_page ORDER BY page")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            pages,
            [
                (110, "feed".to_string(), 1),
                (200, "folder".to_string(), 1),
                (201, "feed".to_string(), 2),
                (202, "folder".to_string(), 2),
                (203, "feed".to_string(), 3),
            ],
            "the buried feed is numbered and listed too"
        );
    }

    #[test]
    fn a_failing_v3_migration_leaves_v2_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v2 = v2_database(&path);
            // Poison: 0003 creates this view after its ALTERs and indexes have already run.
            v2.execute_batch("CREATE VIEW v_page AS SELECT 1 AS x")
                .unwrap();
        }
        assert!(Store::open(&path).is_err());
        let raw = Connection::open(&path).unwrap();
        let version: i64 = raw
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let columns: i64 = raw
            .query_row(
                "SELECT (SELECT count(*) FROM pragma_table_info('feed') WHERE name = 'page') \
                 + (SELECT count(*) FROM pragma_table_info('folder') WHERE name = 'page')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 0, "ALTERs rolled back");
        assert!(path.with_extension("sqlite.bak-v2").exists());
        raw.execute_batch("DROP VIEW v_page").unwrap();
        drop(raw);
        assert_eq!(
            Store::open(&path).unwrap().schema_version().unwrap(),
            SCHEMA_VERSION
        );
    }

    /// A v3 database: `v2_database` numbered, a feed squatting page 105 (now reserved), two
    /// stored diagnoses (a daily feed, a fossil) and the buried feed without one.
    fn v3_database(path: &Path) -> Connection {
        let v3 = v2_database(path);
        v3.execute_batch(MIGRATIONS[2].1).unwrap();
        v3.execute_batch(&format!(
            "UPDATE folder SET page = 200 WHERE id = 1; UPDATE folder SET page = 202 WHERE id = 2;
             UPDATE feed SET page = 105 WHERE id = 1; UPDATE feed SET page = 201 WHERE id = 2;
             UPDATE feed SET page = 203 WHERE id = 3;
             INSERT INTO feed_health (feed_id, status, reason, last_entry_at, median_interval_days, computed_at) VALUES \
             (1, 'Alive', 'r', '{recent}', 0.5, '2026-10-14T08:00:00.000Z'), \
             (2, 'Fossil', 'r', '2019-01-01T00:00:00.000Z', 3.0, '2026-10-14T08:00:00.000Z');",
            recent = crate::model::ts(chrono::Utc::now() - chrono::Duration::days(1)),
        ))
        .unwrap();
        v3
    }

    #[test]
    fn migration_from_v3_frees_pages_104_to_109_and_classifies_every_feed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        drop(v3_database(&path));
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 4);
        assert!(path.with_extension("sqlite.bak-v3").exists());
        let page: u16 = store
            .conn()
            .query_row("SELECT page FROM feed WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            page, 110,
            "the root feed leaves 105 for the first root-feed number"
        );
        let groups: Vec<(i64, String)> = store
            .conn()
            .prepare("SELECT feed_id, cadence_group FROM feed_health ORDER BY feed_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            groups,
            [(1, "daily".to_string()), (2, "silent".to_string())]
        );
        let overrides: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM feed WHERE cadence_override IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(overrides, 0);
    }

    #[test]
    fn a_failing_v4_migration_leaves_v3_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v3 = v3_database(&path);
            // Poison: 0004 creates this table after its ALTERs have already run.
            v3.execute_batch("CREATE TABLE edition (x)").unwrap();
        }
        assert!(Store::open(&path).is_err());
        let raw = Connection::open(&path).unwrap();
        let version: i64 = raw
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
        let columns: i64 = raw
            .query_row(
                "SELECT (SELECT count(*) FROM pragma_table_info('feed') WHERE name = 'cadence_override') \
                 + (SELECT count(*) FROM pragma_table_info('feed_health') WHERE name = 'cadence_group')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 0, "ALTERs rolled back");
        let page: u16 = raw
            .query_row("SELECT page FROM feed WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(page, 105, "the page move rolled back too");
        assert!(path.with_extension("sqlite.bak-v3").exists());
        raw.execute_batch("DROP TABLE edition").unwrap();
        drop(raw);
        assert_eq!(Store::open(&path).unwrap().schema_version().unwrap(), 4);
    }
}
