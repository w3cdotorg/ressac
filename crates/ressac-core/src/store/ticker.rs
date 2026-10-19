//! The ticker pool: recent unread entries that no Journal edition showed.

use chrono::{DateTime, Utc};

use super::Store;
use crate::error::Result;
use crate::model::{FeedId, ts};

/// How far back an entry may have been published and still enter the pool.
pub const TICKER_WINDOW_DAYS: i64 = 7;

/// One line of the ticker: an entry with the feed it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct TickerEntry {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub title: String,
    pub url: String,
}

const SELECT: &str = "SELECT v.entry_id, v.feed_id, v.feed_title, v.title, v.url \
     FROM v_entry v JOIN entry e ON e.id = v.entry_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TickerEntry> {
    Ok(TickerEntry {
        entry_id: r.get(0)?,
        feed_id: r.get(1)?,
        feed_title: r.get(2)?,
        title: r.get(3)?,
        url: r.get(4)?,
    })
}

impl Store {
    /// Unread entries of active feeds published within the last `TICKER_WINDOW_DAYS`, in no
    /// edition, with an http(s) URL and a non-blank title. Newest first, at most 2000.
    pub fn ticker_pool(&self, now: DateTime<Utc>) -> Result<Vec<TickerEntry>> {
        let since = ts(now - chrono::Duration::days(TICKER_WINDOW_DAYS));
        let mut stmt = self.conn().prepare(&format!(
            "{SELECT} WHERE e.read_at IS NULL AND v.published_at >= ?1 \
             AND NOT EXISTS (SELECT 1 FROM edition_entry ee WHERE ee.entry_id = v.entry_id) \
             AND (v.url LIKE 'http://%' OR v.url LIKE 'https://%') AND trim(v.title) <> '' \
             ORDER BY v.published_at DESC, v.entry_id DESC LIMIT 2000"
        ))?;
        let rows = stmt.query_map([since], row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The entry a ticker click opens: its feed is active and its URL is http(s). Read state,
    /// editions and age do not matter, so a click racing the nightly edition still opens.
    pub fn ticker_entry(&self, entry_id: i64) -> Result<Option<TickerEntry>> {
        let mut stmt = self.conn().prepare(&format!(
            "{SELECT} WHERE v.entry_id = ?1 AND (v.url LIKE 'http://%' OR v.url LIKE 'https://%')"
        ))?;
        let mut rows = stmt.query_map([entry_id], row)?;
        Ok(rows.next().transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::library::tests::{add_edition, add_entry, add_feed, at};
    use super::*;

    fn now() -> DateTime<Utc> {
        at("2026-10-19T12:00:00Z")
    }

    fn set_url(s: &Store, id: i64, url: &str) {
        s.conn()
            .execute(
                "UPDATE entry SET url = ?2 WHERE id = ?1",
                rusqlite::params![id, url],
            )
            .unwrap();
    }

    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, None, 0, None, "active", "Daily");
        add_feed(&s, 2, None, 1, None, "buried", "Daily");
        let entries = [
            (1, 1, "One", "2026-10-18T08:00:00Z", false),
            (2, 1, "Two", "2026-10-18T08:00:00Z", true),
            (3, 1, "Three", "2026-10-12T11:59:59Z", false),
            (4, 1, "Four", "2026-10-18T08:00:00Z", false),
            (5, 1, "Five", "2026-10-18T08:00:00Z", false),
            (6, 2, "Six", "2026-10-18T08:00:00Z", false),
            (7, 1, "Seven", "2026-10-18T08:00:00Z", false),
            (8, 1, "   ", "2026-10-18T08:00:00Z", false),
            (9, 1, "Nine", "2026-10-12T12:00:00Z", false),
        ];
        for (id, feed, title, published, read) in entries {
            add_entry(&s, id, feed, title, published, read, false);
            set_url(&s, id, &format!("https://example.org/{id}"));
        }
        set_url(&s, 7, "mailto:x@example.org");
        add_edition(&s, 1, &[(4, "front", 1)], &[]);
        add_edition(&s, 2, &[(5, "front", 1)], &[]);
        s
    }

    #[test]
    fn pool_keeps_only_eligible_entries() {
        let ids: Vec<i64> = store()
            .ticker_pool(now())
            .unwrap()
            .iter()
            .map(|e| e.entry_id)
            .collect();
        assert_eq!(ids, [1, 9]);
    }

    #[test]
    fn pool_rows_carry_feed_and_url() {
        let pool = store().ticker_pool(now()).unwrap();
        assert_eq!(
            pool[0],
            TickerEntry {
                entry_id: 1,
                feed_id: 1,
                feed_title: "Feed 1".into(),
                title: "One".into(),
                url: "https://example.org/1".into(),
            }
        );
    }

    #[test]
    fn ticker_entry_finds_a_read_entry_but_not_a_buried_one() {
        let s = store();
        assert!(s.ticker_entry(2).unwrap().is_some());
        assert!(s.ticker_entry(4).unwrap().is_some());
        assert!(s.ticker_entry(6).unwrap().is_none());
        assert!(s.ticker_entry(7).unwrap().is_none());
        assert!(s.ticker_entry(999).unwrap().is_none());
    }
}
