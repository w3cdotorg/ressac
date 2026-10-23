//! The Mac's `TestDatabase`: a scratch copy of the contract fixture, emptied of entries, with
//! raw writes for the tests (another connection, as the terminal would be).

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use ressac_core::store::Store;
use rusqlite::{Connection, params};
use tempfile::TempDir;

pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contract.sqlite"
);

pub fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// Local midnight of the tests' "today", in UTC.
pub fn today() -> DateTime<Utc> {
    at("2026-10-16T00:00:00Z")
}

/// An entry to insert; the default is a plain unread one in feed 1.
pub struct NewEntry<'a> {
    pub feed: i64,
    pub title: Option<&'a str>,
    pub url: Option<&'a str>,
    pub published: &'a str,
    pub summary: Option<&'a str>,
    pub content: Option<&'a str>,
    pub read: bool,
    pub starred: bool,
    /// The search body; the title when `None`.
    pub body: Option<&'a str>,
}

impl Default for NewEntry<'_> {
    fn default() -> Self {
        NewEntry {
            feed: 1,
            title: Some("Entry"),
            url: Some("https://example.org/a"),
            published: "2026-10-01T08:00:00.000Z",
            summary: Some("<p>Summary</p>"),
            content: None,
            read: false,
            starred: false,
            body: None,
        }
    }
}

const STAMP: &str = "2026-10-10T08:00:00.000Z";

pub struct TestDb {
    pub path: PathBuf,
    _dir: TempDir,
    conn: Connection,
}

impl TestDb {
    /// A copy of the fixture with no entries, editions or health rows and every feed active.
    pub fn new() -> TestDb {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        std::fs::copy(FIXTURE, &path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.busy_timeout(Duration::from_secs(10)).unwrap();
        conn.execute_batch(
            "DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition; \
             DELETE FROM entry_fts; DELETE FROM entry_fulltext; DELETE FROM entry; \
             DELETE FROM feed_health; DELETE FROM fetch_log; DELETE FROM garden_decision; \
             UPDATE feed SET state = 'active', merged_into_id = NULL, prefer_full_text = 0, \
             cadence_override = NULL;",
        )
        .unwrap();
        TestDb {
            path,
            _dir: dir,
            conn,
        }
    }

    pub fn store(&self) -> Store {
        Store::open_existing(&self.path).unwrap()
    }

    pub fn entry(&self, feed: i64, published: &str, read: bool) -> i64 {
        self.entry_with(NewEntry {
            feed,
            published,
            read,
            ..Default::default()
        })
    }

    /// The entry and its search row.
    pub fn entry_with(&self, entry: NewEntry<'_>) -> i64 {
        let id: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM entry", [], |r| {
                r.get(0)
            })
            .unwrap();
        self.conn
            .execute(
                "INSERT INTO entry (id, feed_id, guid, url, title, published_at, summary_html, \
                 content_html, read_at, starred_at, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?6)",
                params![
                    id,
                    entry.feed,
                    format!("guid-{id}"),
                    entry.url,
                    entry.title,
                    entry.published,
                    entry.summary,
                    entry.content,
                    entry.read.then_some(STAMP),
                    entry.starred.then_some(STAMP),
                ],
            )
            .unwrap();
        self.conn
            .execute(
                "INSERT INTO entry_fts (rowid, title, author, body) VALUES (?1, ?2, '', ?3)",
                params![id, entry.title, entry.body.or(entry.title)],
            )
            .unwrap();
        id
    }

    /// `count` entries of `feed`, one per day (day 1 the oldest), read as `read(day)` says;
    /// the ids newest first.
    pub fn days(&self, count: usize, feed: i64, read: impl Fn(usize) -> bool) -> Vec<i64> {
        let mut ids: Vec<i64> = (1..=count)
            .map(|day| self.entry(feed, &format!("2026-10-{day:02}T08:00:00.000Z"), read(day)))
            .collect();
        ids.reverse();
        ids
    }

    #[allow(dead_code, reason = "used by the sources and runs tests (B2b)")]
    pub fn health(&self, feed: i64, status: &str, group: &str, needs_triage: bool) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO feed_health (feed_id, status, reason, computed_at, \
                 cadence_group, needs_triage) VALUES (?1, ?2, '', '2026-10-10T08:00:00.000Z', ?3, ?4)",
                params![feed, status, group, needs_triage],
            )
            .unwrap();
    }

    /// A new latest edition holding `entries` as (entry, section), ranked 1… in order.
    pub fn edition(&self, entries: &[(i64, &str)]) -> i64 {
        let id: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM edition", [], |r| {
                r.get(0)
            })
            .unwrap();
        self.conn
            .execute(
                "INSERT INTO edition (id, built_at, window_start, window_end, unreachable) \
                 VALUES (?1, '2026-10-16T05:00:00.000Z', '2026-10-15T05:00:00.000Z', \
                 '2026-10-16T05:00:00.000Z', 0)",
                [id],
            )
            .unwrap();
        for (rank, (entry, section)) in entries.iter().enumerate() {
            self.conn
                .execute(
                    "INSERT INTO edition_entry (edition_id, entry_id, section, rank) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, entry, section, rank as i64 + 1],
                )
                .unwrap();
        }
        id
    }

    pub fn execute(&self, sql: &str) {
        self.conn.execute_batch(sql).unwrap();
    }

    pub fn read_at(&self, id: i64) -> Option<String> {
        self.conn
            .query_row("SELECT read_at FROM entry WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    pub fn starred(&self, id: i64) -> bool {
        self.conn
            .query_row(
                "SELECT starred_at IS NOT NULL FROM entry WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// `BEGIN IMMEDIATE` and a feed rename on a third connection, held `hold` on its own
    /// thread, then committed; returns once the lock is held.
    #[allow(dead_code, reason = "used by the runs tests (B2b)")]
    pub fn hold_write_lock(&self, hold: Duration) -> std::thread::JoinHandle<()> {
        let conn = Connection::open(&self.path).unwrap();
        conn.busy_timeout(Duration::from_secs(10)).unwrap();
        conn.execute_batch(
            "BEGIN IMMEDIATE; UPDATE feed SET title = 'Renamed by the terminal' WHERE id = 1;",
        )
        .unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(hold);
            conn.execute_batch("COMMIT").unwrap();
        })
    }
}
