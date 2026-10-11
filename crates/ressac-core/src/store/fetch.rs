//! Fetch log, entries and fetch scheduling.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, params};

use super::Store;
use super::reading::index_entry;
use crate::error::Result;
use crate::model::{ErrorKind, FeedId, Hop, parse_ts, ts};
use crate::source::{FeedRef, FetchAttempt};

pub const FETCH_LOG_KEEP: i64 = 20;
pub const FRESH_FOR_MINUTES: i64 = 30;
/// On a feed's first fill, entries published more than this many days before the fetch
/// arrive already read.
pub const BACKLOG_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRecord {
    pub at: DateTime<Utc>,
    pub http_status: Option<u16>,
    pub error: Option<ErrorKind>,
    pub redirect_chain: Vec<Hop>,
    pub content_type: Option<String>,
    pub network_suspect: bool,
    pub not_modified: bool,
    pub entry_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryRecord {
    pub guid: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary_html: Option<String>,
    pub published_at: DateTime<Utc>,
    pub is_fallback: bool,
    pub fetched_at: DateTime<Utc>,
}

pub(super) fn bad_timestamp(col: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(col, "timestamp".into(), rusqlite::types::Type::Text)
}

impl Store {
    /// Writes one attempt in a transaction: log row (trimmed to the last 20), feed cache
    /// fields, entries and their search rows. On a feed's first fill, entries published
    /// more than `BACKLOG_DAYS` before the fetch arrive read. Returns how many entries were new.
    pub fn record_attempt(&self, run_id: &str, a: &FetchAttempt) -> Result<usize> {
        let tx = self.write_tx()?;
        let error_json = a.error.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute(
            "INSERT INTO fetch_log (feed_id, run_id, at, http_status, error_kind, redirect_chain, content_type, \
             bytes, duration_ms, entry_count, not_modified) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                a.feed_id,
                run_id,
                ts(a.at),
                a.http_status,
                error_json,
                serde_json::to_string(&a.redirect_chain)?,
                a.content_type,
                a.bytes as i64,
                a.duration_ms as i64,
                a.feed.as_ref().map(|f| f.entries.len() as i64),
                a.not_modified
            ],
        )?;
        tx.execute(
            "DELETE FROM fetch_log WHERE feed_id = ?1 AND id NOT IN \
             (SELECT id FROM fetch_log WHERE feed_id = ?1 ORDER BY at DESC, id DESC LIMIT ?2)",
            params![a.feed_id, FETCH_LOG_KEEP],
        )?;
        if a.error.is_none() {
            tx.execute(
                "UPDATE feed SET final_url = ?2, etag = COALESCE(?3, etag), \
                 last_modified = COALESCE(?4, last_modified) WHERE id = ?1",
                params![a.feed_id, a.final_url, a.etag, a.last_modified],
            )?;
        }
        let mut added = 0;
        if let Some(feed) = &a.feed {
            tx.execute(
                "UPDATE feed SET description_current = ?2 WHERE id = ?1",
                params![a.feed_id, feed.description],
            )?;
            let first_fill: bool = tx.query_row(
                "SELECT NOT EXISTS (SELECT 1 FROM entry WHERE feed_id = ?1)",
                [a.feed_id],
                |r| r.get(0),
            )?;
            let backlog_before = a.at - Duration::days(BACKLOG_DAYS);
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO entry (feed_id, guid, url, title, author, published_at, \
                 published_at_is_fallback, updated_at, summary_html, content_html, read_at, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            // Only a real change rewrites the row, and only then is the search row redone.
            let mut update = tx.prepare(
                "UPDATE entry SET url = ?3, title = ?4, updated_at = ?5, summary_html = ?6, content_html = ?7 \
                 WHERE feed_id = ?1 AND guid = ?2 AND (url IS NOT ?3 OR title IS NOT ?4 \
                 OR updated_at IS NOT ?5 OR summary_html IS NOT ?6 OR content_html IS NOT ?7) \
                 RETURNING id, author",
            )?;
            for e in &feed.entries {
                let read_at = (first_fill && e.published_at < backlog_before).then(|| ts(a.at));
                let inserted = insert.execute(params![
                    a.feed_id,
                    e.guid,
                    e.url,
                    e.title,
                    e.author,
                    ts(e.published_at),
                    e.published_at_is_fallback,
                    e.updated_at.map(ts),
                    e.summary_html,
                    e.content_html,
                    read_at,
                    ts(a.at)
                ])?;
                let html = e.content_html.as_deref().or(e.summary_html.as_deref());
                if inserted == 1 {
                    added += 1;
                    let id = tx.last_insert_rowid();
                    index_entry(&tx, id, e.title.as_deref(), e.author.as_deref(), html)?;
                } else {
                    let changed: Option<(i64, Option<String>)> = update
                        .query_row(
                            params![
                                a.feed_id,
                                e.guid,
                                e.url,
                                e.title,
                                e.updated_at.map(ts),
                                e.summary_html,
                                e.content_html
                            ],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    if let Some((id, author)) = changed {
                        index_entry(&tx, id, e.title.as_deref(), author.as_deref(), html)?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    /// Flags this run's DNS/connection failures as caused by the user's network.
    pub fn mark_network_suspect(&self, run_id: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE fetch_log SET network_suspect = 1 WHERE run_id = ?1 AND error_kind IN ('\"Dns\"', '\"Connect\"')",
            [run_id],
        )?)
    }

    /// Active feeds not fetched in the last 30 minutes (all active feeds when `force`).
    pub fn feeds_to_fetch(&self, now: DateTime<Utc>, force: bool) -> Result<Vec<FeedRef>> {
        let fresh_since = ts(now - Duration::minutes(FRESH_FOR_MINUTES));
        let mut stmt = self.conn().prepare(
            "SELECT f.id, f.xml_url, f.etag, f.last_modified FROM feed f WHERE f.state = 'active' AND \
             (?1 OR NOT EXISTS (SELECT 1 FROM fetch_log l WHERE l.feed_id = f.id AND l.at > ?2)) ORDER BY f.id",
        )?;
        let rows = stmt
            .query_map(params![force, fresh_since], |r| {
                Ok(FeedRef {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    etag: r.get(2)?,
                    last_modified: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn attempts(&self, feed_id: FeedId, limit: usize) -> Result<Vec<AttemptRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT at, http_status, error_kind, redirect_chain, content_type, network_suspect, not_modified, \
             entry_count FROM fetch_log WHERE feed_id = ?1 ORDER BY at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![feed_id, limit as i64], |r| {
                let at: String = r.get(0)?;
                let error: Option<String> = r.get(2)?;
                let chain: String = r.get(3)?;
                Ok(AttemptRecord {
                    at: parse_ts(&at).ok_or_else(|| bad_timestamp(0))?,
                    http_status: r.get(1)?,
                    error: error.and_then(|e| serde_json::from_str(&e).ok()),
                    redirect_chain: serde_json::from_str(&chain).unwrap_or_default(),
                    content_type: r.get(4)?,
                    network_suspect: r.get(5)?,
                    not_modified: r.get(6)?,
                    entry_count: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn entries(&self, feed_id: FeedId, limit: usize) -> Result<Vec<EntryRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT guid, url, title, summary_html, published_at, published_at_is_fallback, fetched_at \
             FROM entry WHERE feed_id = ?1 ORDER BY published_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![feed_id, limit as i64], |r| {
                let published: String = r.get(4)?;
                let fetched: String = r.get(6)?;
                Ok(EntryRecord {
                    guid: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    summary_html: r.get(3)?,
                    published_at: parse_ts(&published).ok_or_else(|| bad_timestamp(4))?,
                    is_fallback: r.get(5)?,
                    fetched_at: parse_ts(&fetched).ok_or_else(|| bad_timestamp(6))?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opml;
    use crate::parse::{ParsedEntry, ParsedFeed};

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn store_with_feeds(n: usize) -> Store {
        let body: String = (1..=n)
            .map(|i| format!(r#"<outline text="F{i}" xmlUrl="https://f{i}.example.org/feed"/>"#))
            .collect();
        let xml = format!("<opml><body>{body}</body></opml>");
        let store = Store::open_in_memory().unwrap();
        store
            .import_outlines(&opml::parse(&xml).unwrap(), false, now())
            .unwrap();
        store
    }

    fn entry(guid: &str) -> ParsedEntry {
        ParsedEntry {
            guid: guid.into(),
            url: Some(format!("https://e.example.org/{guid}")),
            title: Some(guid.to_uppercase()),
            author: None,
            published_at: now() - Duration::days(1),
            published_at_is_fallback: false,
            updated_at: None,
            summary_html: None,
            content_html: None,
        }
    }

    fn attempt(
        feed_id: FeedId,
        at: DateTime<Utc>,
        error: Option<ErrorKind>,
        guids: &[&str],
    ) -> FetchAttempt {
        FetchAttempt {
            feed_id,
            at,
            requested_url: "https://f1.example.org/feed".into(),
            final_url: "https://f1.example.org/feed".into(),
            http_status: Some(if error.is_some() { 503 } else { 200 }),
            error: error.clone(),
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            bytes: 100,
            duration_ms: 12,
            not_modified: false,
            etag: Some("\"e1\"".into()),
            last_modified: None,
            feed: error.is_none().then(|| ParsedFeed {
                title: Some("F".into()),
                description: Some("Desc".into()),
                site_url: None,
                entries: guids.iter().map(|g| entry(g)).collect(),
            }),
        }
    }

    #[test]
    fn records_entries_and_counts_only_new_ones() {
        let store = store_with_feeds(1);
        assert_eq!(
            store
                .record_attempt("r1", &attempt(1, now(), None, &["a", "b"]))
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .record_attempt("r2", &attempt(1, now(), None, &["b", "c"]))
                .unwrap(),
            1
        );
        let entries = store.entries(1, 10).unwrap();
        assert_eq!(entries.len(), 3);
        let feed = store.feed(1).unwrap();
        assert_eq!(feed.etag.as_deref(), Some("\"e1\""));
        assert_eq!(feed.description_current.as_deref(), Some("Desc"));
        assert_eq!(
            feed.final_url.as_deref(),
            Some("https://f1.example.org/feed")
        );
    }

    #[test]
    fn log_is_trimmed_to_last_twenty() {
        let store = store_with_feeds(1);
        for i in 0..25 {
            store
                .record_attempt("r", &attempt(1, now() + Duration::minutes(i), None, &[]))
                .unwrap();
        }
        let attempts = store.attempts(1, 100).unwrap();
        assert_eq!(attempts.len(), 20);
        assert_eq!(
            attempts[0].at,
            now() + Duration::minutes(24),
            "newest first"
        );
    }

    #[test]
    fn errors_round_trip_and_network_suspects_are_marked_per_run() {
        let store = store_with_feeds(3);
        store
            .record_attempt("r1", &attempt(1, now(), Some(ErrorKind::Dns), &[]))
            .unwrap();
        store
            .record_attempt("r1", &attempt(2, now(), Some(ErrorKind::Http(410)), &[]))
            .unwrap();
        store
            .record_attempt("r0", &attempt(3, now(), Some(ErrorKind::Connect), &[]))
            .unwrap();
        assert_eq!(store.mark_network_suspect("r1").unwrap(), 1);
        assert!(store.attempts(1, 1).unwrap()[0].network_suspect);
        assert_eq!(
            store.attempts(2, 1).unwrap()[0].error,
            Some(ErrorKind::Http(410))
        );
        assert!(
            !store.attempts(3, 1).unwrap()[0].network_suspect,
            "other runs untouched"
        );
    }

    #[test]
    fn feeds_to_fetch_skips_fresh_and_buried_feeds() {
        let store = store_with_feeds(3);
        store
            .record_attempt("r", &attempt(1, now() - Duration::minutes(10), None, &[]))
            .unwrap();
        store
            .conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 3", [])
            .unwrap();
        let ids: Vec<FeedId> = store
            .feeds_to_fetch(now(), false)
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(ids, vec![2]);
        let ids: Vec<FeedId> = store
            .feeds_to_fetch(now(), true)
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(ids, vec![1, 2]);
        let refs = store.feeds_to_fetch(now(), true).unwrap();
        assert_eq!(refs[0].etag.as_deref(), Some("\"e1\""));
    }

    fn read_at(store: &Store, guid: &str) -> Option<String> {
        store
            .conn()
            .query_row("SELECT read_at FROM entry WHERE guid = ?1", [guid], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn dated(guid: &str, days_ago: i64) -> ParsedEntry {
        ParsedEntry {
            published_at: now() - Duration::days(days_ago),
            ..entry(guid)
        }
    }

    #[test]
    fn first_fill_stores_entries_older_than_seven_days_as_read() {
        let store = store_with_feeds(1);
        let mut first = attempt(1, now(), None, &[]);
        first.feed.as_mut().unwrap().entries = vec![dated("new", 6), dated("old", 8)];
        store.record_attempt("r1", &first).unwrap();
        assert_eq!(read_at(&store, "new"), None);
        assert_eq!(
            read_at(&store, "old").as_deref(),
            Some("2026-10-10T08:00:00.000Z")
        );
        let mut second = attempt(1, now() + Duration::hours(1), None, &[]);
        second.feed.as_mut().unwrap().entries = vec![dated("older", 30)];
        store.record_attempt("r2", &second).unwrap();
        assert_eq!(
            read_at(&store, "older"),
            None,
            "only the first fill has a backlog"
        );
    }

    #[test]
    fn entries_are_indexed_and_reindexed_only_when_they_change() {
        let store = store_with_feeds(1);
        let mut first = attempt(1, now(), None, &[]);
        let mut e = entry("a");
        e.summary_html = Some("<p>alpha &amp; omega</p>".into());
        first.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r1", &first).unwrap();
        assert_eq!(store.search_ids("alpha").unwrap(), [1]);
        assert_eq!(store.search_ids("title:A").unwrap(), [1]);

        e.content_html = Some("<p>beta</p>".into());
        let mut second = attempt(1, now() + Duration::hours(1), None, &[]);
        second.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r2", &second).unwrap();
        assert_eq!(
            store.search_ids("beta").unwrap(),
            [1],
            "content wins over summary"
        );
        assert!(store.search_ids("alpha").unwrap().is_empty());

        // An identical entry leaves the row alone: a body rewritten by hand survives.
        store
            .conn()
            .execute("UPDATE entry_fts SET body = 'gamma' WHERE rowid = 1", [])
            .unwrap();
        let mut third = attempt(1, now() + Duration::hours(2), None, &[]);
        third.feed.as_mut().unwrap().entries = vec![e];
        store.record_attempt("r3", &third).unwrap();
        assert_eq!(store.search_ids("gamma").unwrap(), [1]);
    }
}
