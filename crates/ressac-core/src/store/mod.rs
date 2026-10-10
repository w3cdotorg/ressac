//! SQLite storage. Every SQL statement of the crate lives under this module.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{Error, Result};

mod decisions;
mod feeds;
mod fetch;
mod health;
mod reading;

pub use decisions::{DecisionRecord, FeedSnapshot};
pub use feeds::{FeedRow, FolderRow, ImportReport};
pub use fetch::{AttemptRecord, BACKLOG_DAYS, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
pub use reading::plain_text;

/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 2;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../schema/0001_init.sql")),
    (2, include_str!("../../../../schema/0002_reading.sql")),
];

/// The migration after which existing entries get their search rows.
const FTS_MIGRATION: i64 = 2;

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
        let has_meta: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
            [],
            |r| r.get(0),
        )?;
        if has_meta == 0 {
            return Ok(0);
        }
        Ok(self
            .conn
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew {
                found: current,
                known: SCHEMA_VERSION,
            });
        }
        let pending: Vec<&(i64, &str)> = MIGRATIONS.iter().filter(|(v, _)| *v > current).collect();
        if pending.is_empty() {
            return Ok(());
        }
        if current > 0 {
            self.backup(current)?;
        }
        for (version, sql) in pending {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            if *version == FTS_MIGRATION {
                reading::backfill_fts(&tx)?;
            }
            tx.execute("UPDATE schema_meta SET version = ?1", [version])?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Copies the database to `ressac.sqlite.bak-v<version>` before migrating it.
    fn backup(&self, version: i64) -> Result<()> {
        if let Some(path) = &self.path {
            let dest = path.with_extension(format!("sqlite.bak-v{version}"));
            self.conn
                .execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
        }
        Ok(())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
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
    fn fresh_store_has_schema_v2() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        let names = object_names(&store);
        for expected in [
            "entry",
            "entry_fts",
            "entry_fulltext",
            "feed",
            "feed_health",
            "fetch_log",
            "folder",
            "garden_decision",
            "schema_meta",
            "v_entry",
            "v_feed_overview",
            "v_folder_path",
            "v_garden_row",
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
        assert_eq!(store.schema_version().unwrap(), 2);
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
                 '<p>Lighthouse &amp; pier</p>', '2026-01-05T00:00:00.000Z', '2026-01-01T00:00:00.000Z');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
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
            [None, Some("2026-01-05T00:00:00.000Z".to_string())],
            "no backlog rule on migration"
        );
        let indexed: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM entry_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 2);
        assert_eq!(store.search_ids("harbour").unwrap(), [1]);
        assert_eq!(store.search_ids("lighthouse").unwrap(), [2]);
        assert_eq!(store.search_ids("author:ada").unwrap(), [2]);
        assert!(
            store.search_ids("short").unwrap().is_empty(),
            "content_html wins over summary_html"
        );
    }
}
