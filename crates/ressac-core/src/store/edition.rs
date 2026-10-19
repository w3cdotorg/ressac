//! Editions: candidate entries, the 04:00 due rule, and writing one edition in one transaction.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::Store;
use super::fetch::bad_timestamp;
use crate::edition::{
    BACKLOG_GUARD_DAYS, Candidate, Caps, Composition, FIRST_WINDOW_HOURS, Section, compose,
};
use crate::error::{Error, Result};
use crate::model::{CadenceGroup, FeedId, parse_ts, ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionRecord {
    pub id: i64,
    pub built_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub unreachable: u32,
}

pub struct EditionParams {
    /// 04:00 local of the day being built, in UTC (`edition::day_start`).
    pub day_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub built_at: DateTime<Utc>,
    pub caps: Caps,
    /// Feeds whose fetch failed during the build's refresh.
    pub unreachable: u32,
    /// Replace today's edition, reusing its `window_start`.
    pub rebuild: bool,
}

/// One row of `v_edition_today`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionEntryRow {
    pub entry_id: i64,
    pub section: Section,
    pub rank: u32,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub title: Option<String>,
    pub published_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub starred_at: Option<DateTime<Utc>>,
}

/// One row of `v_edition_overflow`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionOverflowRow {
    pub section: Section,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub page: Option<u16>,
    pub hidden_count: u32,
}

/// The latest edition with its entries (by rank) and its overflow (by section, then page).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionView {
    pub edition: EditionRecord,
    pub entries: Vec<EditionEntryRow>,
    pub overflow: Vec<EditionOverflowRow>,
}

const EDITION_COLUMNS: &str = "id, built_at, window_start, window_end, unreachable";

fn time(row: &Row<'_>, col: usize) -> rusqlite::Result<DateTime<Utc>> {
    let text: String = row.get(col)?;
    parse_ts(&text).ok_or_else(|| bad_timestamp(col))
}

fn optional_time(row: &Row<'_>, col: usize) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let text: Option<String> = row.get(col)?;
    text.map(|t| parse_ts(&t).ok_or_else(|| bad_timestamp(col)))
        .transpose()
}

fn section(row: &Row<'_>, col: usize) -> rusqlite::Result<Section> {
    let text: String = row.get(col)?;
    Section::parse(&text).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(col, "section".into(), rusqlite::types::Type::Text)
    })
}

fn edition_record(row: &Row<'_>) -> rusqlite::Result<EditionRecord> {
    Ok(EditionRecord {
        id: row.get(0)?,
        built_at: time(row, 1)?,
        window_start: time(row, 2)?,
        window_end: time(row, 3)?,
        unreachable: row.get(4)?,
    })
}

fn latest(conn: &Connection) -> Result<Option<EditionRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {EDITION_COLUMNS} FROM edition ORDER BY id DESC LIMIT 1"),
            [],
            edition_record,
        )
        .optional()?)
}

/// The eligible entries of a window (spec §6, Eligibility).
fn candidates(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Candidate>> {
    let guard = start - Duration::days(BACKLOG_GUARD_DAYS);
    let mut stmt = conn.prepare(
        "SELECT e.id, e.feed_id, d.page, COALESCE(d.cadence_override, h.cadence_group), e.published_at \
         FROM entry e JOIN feed d ON d.id = e.feed_id JOIN feed_health h ON h.feed_id = d.id \
         WHERE d.state = 'active' AND e.read_at IS NULL \
           AND e.fetched_at > ?1 AND e.fetched_at <= ?2 AND e.published_at >= ?3 \
           AND COALESCE(d.cadence_override, h.cadence_group) <> 'silent'",
    )?;
    let rows = stmt
        .query_map(params![ts(start), ts(end), ts(guard)], |r| {
            let group: String = r.get(3)?;
            Ok(Candidate {
                entry_id: r.get(0)?,
                feed_id: r.get(1)?,
                feed_page: r.get(2)?,
                group: CadenceGroup::parse(&group).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(
                        3,
                        "cadence group".into(),
                        rusqlite::types::Type::Text,
                    )
                })?,
                published_at: time(r, 4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

impl Store {
    pub fn latest_edition(&self) -> Result<Option<EditionRecord>> {
        latest(self.conn())
    }

    /// No edition has `built_at >= day_start`.
    pub fn edition_due(&self, day_start: DateTime<Utc>) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM edition WHERE built_at >= ?1)",
            [ts(day_start)],
            |r| r.get(0),
        )?)
    }

    /// The latest `fetched_at` stamped at or after `since`: what a refresh that began then wrote.
    pub fn latest_fetch_since(&self, since: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
        let latest: Option<String> = self.conn().query_row(
            "SELECT MAX(fetched_at) FROM entry WHERE fetched_at >= ?1",
            [ts(since)],
            |r| r.get(0),
        )?;
        Ok(latest
            .map(|text| parse_ts(&text).ok_or_else(|| bad_timestamp(0)))
            .transpose()?)
    }

    /// Active feeds only, `fetched_at` in `(window_start, window_end]`, unread,
    /// `published_at >= window_start − BACKLOG_GUARD_DAYS`, effective group not Silent.
    pub fn edition_candidates(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Candidate>> {
        candidates(self.conn(), window_start, window_end)
    }

    /// In one write transaction: re-checks the due rule (with `rebuild`, deletes today's edition
    /// and reuses its `window_start`), selects the candidates, composes them and inserts the
    /// edition with its entries and overflow. Not due and not `rebuild` → `Error::EditionNotDue`.
    pub fn write_edition(&self, p: &EditionParams) -> Result<(EditionRecord, Composition)> {
        let tx = self.write_tx()?;
        let today: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, window_start FROM edition WHERE built_at >= ?1 ORDER BY id DESC LIMIT 1",
                [ts(p.day_start)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let window_start = match today {
            Some(_) if !p.rebuild => return Err(Error::EditionNotDue),
            Some((id, start)) => {
                tx.execute("DELETE FROM edition WHERE id = ?1", [id])?;
                parse_ts(&start).ok_or_else(|| bad_timestamp(1))?
            }
            None => match latest(&tx)? {
                Some(previous) => previous.window_end,
                None => p.window_end - Duration::hours(FIRST_WINDOW_HOURS),
            },
        };
        let composition = compose(&candidates(&tx, window_start, p.window_end)?, p.caps);
        tx.execute(
            "INSERT INTO edition (built_at, window_start, window_end, unreachable) VALUES (?1, ?2, ?3, ?4)",
            params![ts(p.built_at), ts(window_start), ts(p.window_end), p.unreachable],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut entry = tx.prepare(
                "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for e in &composition.entries {
                entry.execute(params![id, e.entry_id, e.section.as_str(), e.rank])?;
            }
            let mut overflow = tx.prepare(
                "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for o in &composition.overflow {
                overflow.execute(params![id, o.feed_id, o.section.as_str(), o.hidden_count])?;
            }
        }
        tx.commit()?;
        let record = EditionRecord {
            id,
            built_at: p.built_at,
            window_start,
            window_end: p.window_end,
            unreachable: p.unreachable,
        };
        Ok((record, composition))
    }

    /// The latest edition read through the contract views; `None` before the first edition.
    pub fn edition_view(&self) -> Result<Option<EditionView>> {
        let Some(edition) = latest(self.conn())? else {
            return Ok(None);
        };
        let entries = self
            .conn()
            .prepare(
                "SELECT entry_id, section, rank, feed_id, feed_title, title, published_at, read_at, starred_at \
                 FROM v_edition_today ORDER BY rank",
            )?
            .query_map([], |r| {
                Ok(EditionEntryRow {
                    entry_id: r.get(0)?,
                    section: section(r, 1)?,
                    rank: r.get(2)?,
                    feed_id: r.get(3)?,
                    feed_title: r.get(4)?,
                    title: r.get(5)?,
                    published_at: time(r, 6)?,
                    read_at: optional_time(r, 7)?,
                    starred_at: optional_time(r, 8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let overflow = self
            .conn()
            .prepare(
                "SELECT section, feed_id, feed_title, page, hidden_count FROM v_edition_overflow \
                 ORDER BY CASE section WHEN 'front' THEN 0 WHEN 'week' THEN 1 ELSE 2 END, \
                 page IS NULL, page, feed_id",
            )?
            .query_map([], |r| {
                Ok(EditionOverflowRow {
                    section: section(r, 0)?,
                    feed_id: r.get(1)?,
                    feed_title: r.get(2)?,
                    page: r.get(3)?,
                    hidden_count: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(EditionView {
            edition,
            entries,
            overflow,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edition::{Caps, Section};
    use rusqlite::params;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// An active feed with its page and stored group (`daily`, `weekly`, `monthly`, `rare`, `silent`).
    fn add_feed(s: &Store, id: i64, page: Option<u16>, group: &str) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, position, title, xml_url, xml_url_original, url_key, added_at, page) \
                 VALUES (?1, ?1, ?2, ?3, ?3, ?3, '2026-01-01T00:00:00.000Z', ?4)",
                params![id, format!("Feed {id}"), format!("https://f{id}.example.org/feed"), page],
            )
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO feed_health (feed_id, status, reason, computed_at, cadence_group) \
                 VALUES (?1, 'Alive', '', '2026-01-01T00:00:00.000Z', ?2)",
                params![id, group],
            )
            .unwrap();
    }

    fn add_entry(s: &Store, id: i64, feed: i64, published: &str, fetched: &str) {
        s.conn()
            .execute(
                "INSERT INTO entry (id, feed_id, guid, title, published_at, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    feed,
                    format!("g{id}"),
                    format!("Entry {id}"),
                    ts(at(published)),
                    ts(at(fetched))
                ],
            )
            .unwrap();
    }

    fn mark_read(s: &Store, id: i64) {
        s.conn()
            .execute(
                "UPDATE entry SET read_at = '2026-10-16T07:00:00.000Z' WHERE id = ?1",
                [id],
            )
            .unwrap();
    }

    /// Params for a build at `end` on the day starting at `day`.
    fn params_at(day: &str, end: &str) -> EditionParams {
        EditionParams {
            day_start: at(day),
            window_end: at(end),
            built_at: at(end),
            caps: Caps::default(),
            unreachable: 0,
            rebuild: false,
        }
    }

    fn ids(s: &Store) -> Vec<i64> {
        s.edition_view()
            .unwrap()
            .map(|v| v.entries.iter().map(|e| e.entry_id).collect())
            .unwrap_or_default()
    }

    fn edition_count(s: &Store) -> i64 {
        s.conn()
            .query_row("SELECT count(*) FROM edition", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn the_first_edition_covers_the_last_24_hours() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-15T06:00:00Z", "2026-10-15T06:00:00Z"); // in
        add_entry(&s, 2, 1, "2026-10-15T04:00:00Z", "2026-10-15T04:00:00Z"); // 25 h before: out
        let (rec, comp) = s
            .write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        assert_eq!(rec.window_start, at("2026-10-15T05:00:00Z"));
        assert_eq!(rec.window_end, at("2026-10-16T05:00:00Z"));
        assert_eq!(comp.entries.len(), 1);
        assert_eq!(ids(&s), [1]);
        assert_eq!(s.latest_edition().unwrap(), Some(rec));
    }

    /// Review Focus 2: entries fetched after the build leave today's edition alone and open tomorrow's.
    #[test]
    fn entries_fetched_after_the_build_go_to_the_next_edition() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T03:00:00Z", "2026-10-16T03:00:00Z");
        let (first, _) = s
            .write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        add_entry(&s, 2, 1, "2026-10-16T11:00:00Z", "2026-10-16T11:30:00Z"); // a manual refresh at noon
        assert_eq!(ids(&s), [1], "today's edition is frozen");
        let (second, _) = s
            .write_edition(&params_at("2026-10-17T04:00:00Z", "2026-10-17T05:00:00Z"))
            .unwrap();
        assert_eq!(second.window_start, first.window_end);
        assert_eq!(ids(&s), [2]);
    }

    #[test]
    fn a_monday_edition_spans_the_weekend_with_the_same_caps() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap(); // Friday
        add_entry(&s, 1, 1, "2026-10-17T08:00:00Z", "2026-10-17T09:00:00Z"); // Saturday
        add_entry(&s, 2, 1, "2026-10-18T08:00:00Z", "2026-10-18T09:00:00Z"); // Sunday
        add_entry(&s, 3, 1, "2026-10-19T03:00:00Z", "2026-10-19T04:30:00Z"); // Monday morning
        let (monday, comp) = s
            .write_edition(&params_at("2026-10-19T04:00:00Z", "2026-10-19T05:00:00Z"))
            .unwrap();
        assert_eq!(monday.window_start, at("2026-10-16T05:00:00Z"));
        assert_eq!(ids(&s), [3], "one entry per daily feed");
        assert_eq!(comp.overflow.len(), 1);
        assert_eq!(comp.overflow[0].hidden_count, 2);
    }

    #[test]
    fn only_unread_entries_of_active_non_silent_feeds_within_the_guard() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_feed(&s, 2, Some(402), "silent");
        add_feed(&s, 3, Some(403), "weekly");
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 3", [])
            .unwrap();
        let fetched = "2026-10-16T03:00:00Z";
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", fetched); // in
        add_entry(&s, 2, 1, "2026-10-16T01:00:00Z", fetched); // read: out
        mark_read(&s, 2);
        add_entry(&s, 3, 2, "2026-10-16T02:00:00Z", fetched); // silent feed: out
        add_entry(&s, 4, 3, "2026-10-16T02:00:00Z", fetched); // buried feed: out
        // The window opens 2026-10-15T05:00Z, so the guard is 2026-10-08T05:00Z.
        add_entry(&s, 5, 1, "2026-10-08T04:59:00Z", fetched); // republished archive: out
        add_entry(&s, 6, 1, "2026-10-08T05:00:00Z", fetched); // exactly at the guard: in
        let c = s
            .edition_candidates(at("2026-10-15T05:00:00Z"), at("2026-10-16T05:00:00Z"))
            .unwrap();
        let mut got: Vec<i64> = c.iter().map(|c| c.entry_id).collect();
        got.sort_unstable();
        assert_eq!(got, [1, 6]);
        assert_eq!(c[0].feed_page, Some(401));
        assert_eq!(c[0].group, CadenceGroup::Weekly);
    }

    #[test]
    fn an_override_wins_over_the_computed_group() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        s.conn()
            .execute("UPDATE feed SET cadence_override = 'rare' WHERE id = 1", [])
            .unwrap();
        add_feed(&s, 2, Some(402), "rare");
        s.conn()
            .execute(
                "UPDATE feed SET cadence_override = 'silent' WHERE id = 2",
                [],
            )
            .unwrap();
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 2, 1, "2026-10-16T01:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 3, 2, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        let (_, comp) = s
            .write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        // Feed 1 forced to Rare: front page, uncapped. Feed 2 forced to Silent: absent.
        assert_eq!(
            comp.entries
                .iter()
                .map(|p| (p.entry_id, p.section))
                .collect::<Vec<_>>(),
            [(1, Section::Front), (2, Section::Front)]
        );
    }

    #[test]
    fn the_due_rule_follows_the_day_start() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.edition_due(at("2026-10-16T04:00:00Z")).unwrap());
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        assert!(!s.edition_due(at("2026-10-16T04:00:00Z")).unwrap());
        assert!(s.edition_due(at("2026-10-17T04:00:00Z")).unwrap());
    }

    /// Two builds the same morning (the app and the CLI): the second is refused inside its
    /// transaction and writes nothing.
    #[test]
    fn a_second_build_the_same_morning_is_refused() {
        let s = Store::open_in_memory().unwrap();
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        let again = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T06:00:00Z"));
        assert!(matches!(again, Err(Error::EditionNotDue)));
        assert_eq!(edition_count(&s), 1);
    }

    #[test]
    fn a_rebuild_replaces_today_and_keeps_its_window_start() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        add_entry(&s, 1, 1, "2026-10-16T01:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 2, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        let (first, _) = s
            .write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        assert_eq!(ids(&s), [2]);
        mark_read(&s, 2);
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T09:00:00Z");
        p.rebuild = true;
        p.caps = Caps { week: 3, wire: 5 };
        let (second, _) = s.write_edition(&p).unwrap();
        // No AUTOINCREMENT on `edition`: SQLite may reuse the freed rowid, so compare built_at.
        assert_ne!(second.built_at, first.built_at);
        assert_eq!(second.window_start, first.window_start);
        assert_eq!(edition_count(&s), 1, "today's edition was replaced");
        assert_eq!(
            ids(&s),
            [1],
            "an entry read since then is no longer eligible"
        );
    }

    #[test]
    fn a_rebuild_without_an_edition_today_builds_normally() {
        let s = Store::open_in_memory().unwrap();
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z");
        p.rebuild = true;
        let (rec, _) = s.write_edition(&p).unwrap();
        assert_eq!(rec.window_start, at("2026-10-15T05:00:00Z"));
    }

    #[test]
    fn an_empty_edition_is_stored_and_advances_the_window() {
        let s = Store::open_in_memory().unwrap();
        let (empty, comp) = s
            .write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        assert_eq!(comp, Composition::default());
        let view = s
            .edition_view()
            .unwrap()
            .expect("an empty edition is still an edition");
        assert_eq!(view.edition, empty);
        assert!(view.entries.is_empty() && view.overflow.is_empty());
        let (next, _) = s
            .write_edition(&params_at("2026-10-17T04:00:00Z", "2026-10-17T05:00:00Z"))
            .unwrap();
        assert_eq!(next.window_start, empty.window_end);
    }

    #[test]
    fn the_view_lists_entries_by_rank_and_overflow_by_section() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        add_feed(&s, 2, Some(402), "weekly");
        for i in 0..3 {
            add_entry(
                &s,
                10 + i,
                1,
                &format!("2026-10-16T0{i}:00:00Z"),
                "2026-10-16T03:00:00Z",
            );
        }
        for i in 0..5 {
            add_entry(
                &s,
                20 + i,
                2,
                &format!("2026-10-1{i}T08:00:00Z"),
                "2026-10-16T03:00:00Z",
            );
        }
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z");
        p.unreachable = 3;
        s.write_edition(&p).unwrap();
        let v = s.edition_view().unwrap().unwrap();
        assert_eq!(v.edition.unreachable, 3);
        assert_eq!(
            v.entries
                .iter()
                .map(|e| (e.rank, e.entry_id, e.section))
                .collect::<Vec<_>>(),
            [
                (1, 24, Section::Week),
                (2, 23, Section::Week),
                (3, 22, Section::Week),
                (4, 12, Section::Wire)
            ]
        );
        assert_eq!(v.entries[0].feed_title, "Feed 2");
        assert!(v.entries[0].read_at.is_none());
        assert_eq!(
            v.overflow
                .iter()
                .map(|o| (o.section, o.feed_id, o.hidden_count))
                .collect::<Vec<_>>(),
            [(Section::Week, 2, 2), (Section::Wire, 1, 2)]
        );
        assert_eq!(v.overflow[0].page, Some(402));
    }

    /// Review Focus 3: burying a feed after the build leaves today's edition as it was.
    #[test]
    fn a_feed_buried_after_the_build_stays_in_today_s_edition() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 1", [])
            .unwrap();
        assert_eq!(ids(&s), [1]);
    }

    #[test]
    fn read_state_in_the_view_is_live() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z"))
            .unwrap();
        mark_read(&s, 1);
        let v = s.edition_view().unwrap().unwrap();
        assert_eq!(v.entries[0].read_at, Some(at("2026-10-16T07:00:00Z")));
    }

    #[test]
    fn no_edition_at_all() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.latest_edition().unwrap(), None);
        assert_eq!(s.edition_view().unwrap(), None);
    }
}
