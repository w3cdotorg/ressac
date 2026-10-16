//! Reading for the terminal reader (spec §4): the sources, the entry lists, the Journal and
//! search, over the contract views. Every rule mirrors the Mac app's `Queries.swift`, so the
//! terminal and the app show the same lists.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{Row, params_from_iter};

use super::Store;
use super::edition::{EditionOverflowRow, EditionRecord};
use super::fetch::bad_timestamp;
use crate::edition::Section;
use crate::error::Result;
use crate::model::{CadenceGroup, FeedId, FolderId, parse_ts, ts};

/// What the reader lists: a smart list, a folder (its subfolders included), a feed, or a
/// rhythm group. The garden is not a source of entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Journal,
    Unread,
    Today,
    Starred,
    Folder(FolderId),
    Feed(FeedId),
    Cadence(CadenceGroup),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListFilter {
    #[default]
    All,
    Unread,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceCounts {
    pub journal: usize,
    pub unread: usize,
    pub today: usize,
    pub starred: usize,
    pub needs_triage: usize,
    /// Unread entries per rhythm group; all five groups are present.
    pub cadence: BTreeMap<CadenceGroup, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceNodeKind {
    Folder { id: FolderId, name: String },
    Feed { id: FeedId, title: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceNode {
    pub kind: SourceNodeKind,
    pub page: Option<u16>,
    /// A feed's unread entries; a folder's: the sum over its subtree.
    pub unread: usize,
    pub children: Vec<SourceNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourcesSnapshot {
    pub counts: SourceCounts,
    /// Root folders and feeds without a folder, in the app's sidebar order.
    pub tree: Vec<SourceNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryRow {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub url: Option<String>,
    pub published_at: DateTime<Utc>,
    pub is_read: bool,
    pub is_starred: bool,
    pub has_fulltext: bool,
}

/// Where the next page starts: the stored `published_at` text and the id of the last row
/// loaded (like the app's `ListFloor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCursor {
    pub published_at: String,
    pub entry_id: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryPage {
    pub rows: Vec<EntryRow>,
    /// Rows of the whole list, loaded or not.
    pub total: usize,
    /// Unread rows of the whole list, loaded or not.
    pub unread: usize,
    /// Set when rows older than the last one remain.
    pub next: Option<ListCursor>,
}

/// Rows per page of a list (the app's long-list step).
pub const PAGE_SIZE: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct JournalItem {
    pub row: EntryRow,
    pub section: Section,
    pub rank: u32,
}

/// The latest edition with its entries in rank order and its overflow, both restricted to
/// active feeds (a feed buried since the build drops out, as in the app).
#[derive(Debug, Clone, PartialEq)]
pub struct JournalView {
    pub edition: EditionRecord,
    pub items: Vec<JournalItem>,
    pub overflow: Vec<EditionOverflowRow>,
}

/// The `v_entry` rows of the latest edition.
const LATEST_EDITION_ENTRIES: &str = "entry_id IN (SELECT entry_id FROM edition_entry WHERE edition_id = (SELECT MAX(id) FROM edition))";

/// An entry's rank in the latest edition: the Journal's order.
const EDITION_RANK: &str = "(SELECT ee.rank FROM edition_entry ee \
     WHERE ee.entry_id = v_entry.entry_id AND ee.edition_id = (SELECT MAX(id) FROM edition))";

/// The `v_entry` columns `entry_row` reads, in its order.
const ROW_COLUMNS: &str = "entry_id, feed_id, feed_title, title, author, url, published_at, \
     is_read, is_starred, has_fulltext";

/// The `v_entry` rows of `source` and their arguments (the app's `Queries.predicate`).
fn predicate(source: Source, today_start: DateTime<Utc>) -> (String, Vec<Value>) {
    match source {
        Source::Journal => (LATEST_EDITION_ENTRIES.to_string(), vec![]),
        Source::Unread => ("NOT is_read".to_string(), vec![]),
        Source::Today => (
            "published_at >= ?".to_string(),
            vec![Value::Text(ts(today_start))],
        ),
        Source::Starred => ("is_starred".to_string(), vec![]),
        Source::Cadence(group) => (
            "feed_id IN (SELECT feed_id FROM v_cadence WHERE effective_group = ?)".to_string(),
            vec![Value::Text(group.as_str().to_string())],
        ),
        Source::Feed(id) => ("feed_id = ?".to_string(), vec![Value::Integer(id)]),
        Source::Folder(id) => (
            "folder_id IN (WITH RECURSIVE sub(id) AS \
             (SELECT ? UNION ALL SELECT f.id FROM folder f JOIN sub ON f.parent_id = sub.id) \
             SELECT id FROM sub)"
                .to_string(),
            vec![Value::Integer(id)],
        ),
    }
}

/// A folder line of the sidebar tree.
struct FolderLine {
    id: FolderId,
    parent: Option<FolderId>,
    name: String,
    position: i64,
    page: Option<u16>,
}

/// An active feed line of the sidebar tree.
struct FeedLine {
    id: FeedId,
    folder: Option<FolderId>,
    position: i64,
    title: String,
    page: Option<u16>,
    unread: usize,
}

/// The children of `parent` (the root when `None`): folders (rank 0) and feeds (rank 1)
/// sorted by `(position, rank, id)`, a folder's unread being the sum over its subtree.
fn children(
    parent: Option<FolderId>,
    folders: &[FolderLine],
    feeds: &[FeedLine],
) -> Vec<SourceNode> {
    let mut items: Vec<(i64, u8, i64, SourceNode)> = Vec::new();
    for f in folders.iter().filter(|f| f.parent == parent) {
        let nested = children(Some(f.id), folders, feeds);
        let unread = nested.iter().map(|n| n.unread).sum();
        items.push((
            f.position,
            0,
            f.id,
            SourceNode {
                kind: SourceNodeKind::Folder {
                    id: f.id,
                    name: f.name.clone(),
                },
                page: f.page,
                unread,
                children: nested,
            },
        ));
    }
    for d in feeds.iter().filter(|d| d.folder == parent) {
        items.push((
            d.position,
            1,
            d.id,
            SourceNode {
                kind: SourceNodeKind::Feed {
                    id: d.id,
                    title: d.title.clone(),
                },
                page: d.page,
                unread: d.unread,
                children: Vec::new(),
            },
        ));
    }
    items.sort_by_key(|(position, rank, id, _)| (*position, *rank, *id));
    items.into_iter().map(|(_, _, _, node)| node).collect()
}

/// One `v_entry` row read with `ROW_COLUMNS`, with its raw `published_at` text (the cursor).
fn entry_row(r: &Row<'_>) -> rusqlite::Result<(EntryRow, String)> {
    let published: String = r.get(6)?;
    let row = EntryRow {
        entry_id: r.get(0)?,
        feed_id: r.get(1)?,
        feed_title: r.get(2)?,
        title: r.get(3)?,
        author: r.get(4)?,
        url: r.get(5)?,
        published_at: parse_ts(&published).ok_or_else(|| bad_timestamp(6))?,
        is_read: r.get(7)?,
        is_starred: r.get(8)?,
        has_fulltext: r.get(9)?,
    };
    Ok((row, published))
}

fn section_at(r: &Row<'_>, col: usize) -> rusqlite::Result<Section> {
    let text: String = r.get(col)?;
    Section::parse(&text).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(col, "section".into(), rusqlite::types::Type::Text)
    })
}

fn group_at(r: &Row<'_>, col: usize) -> rusqlite::Result<CadenceGroup> {
    let text: String = r.get(col)?;
    CadenceGroup::parse(&text).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(col, "cadence group".into(), rusqlite::types::Type::Text)
    })
}

/// Every word of `text`, quoted for FTS5 (`"a" "b"` matches rows with both); `None` without
/// words. The app's `Queries.ftsQuery`.
pub fn fts_query(text: &str) -> Option<String> {
    let tokens: Vec<String> = text
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

/// One entry for the reader (spec §4, Entry content): its row, every body the reader may
/// show, the stored extraction and the feed's full-text preference.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryDetail {
    pub row: EntryRow,
    pub summary_html: Option<String>,
    pub content_html: Option<String>,
    pub fulltext_html: Option<String>,
    /// The stored failure sentence of the last extraction.
    pub fulltext_error: Option<String>,
    pub prefer_full_text: bool,
}

/// One active, diagnosed feed's rhythm, as `v_cadence` gives it: what the rhythm line and
/// the override menu need.
#[derive(Debug, Clone, PartialEq)]
pub struct CadenceDetail {
    pub feed_id: FeedId,
    pub effective_group: CadenceGroup,
    pub computed_group: CadenceGroup,
    pub is_override: bool,
    pub median_interval_days: Option<f64>,
    pub entries_per_month: f64,
    pub last_entry_at: Option<DateTime<Utc>>,
    pub unread: usize,
}

impl Store {
    /// The reader's sources column (spec §4, Sources). `today_start` is local midnight in UTC,
    /// computed by the caller from its zone.
    pub fn sources(&self, today_start: DateTime<Utc>) -> Result<SourcesSnapshot> {
        let conn = self.conn();
        let count = |sql: &str, args: Vec<Value>| -> Result<usize> {
            let n: i64 = conn.query_row(sql, params_from_iter(args), |r| r.get(0))?;
            Ok(n as usize)
        };
        let mut cadence: BTreeMap<CadenceGroup, usize> =
            CadenceGroup::ALL.iter().map(|g| (*g, 0)).collect();
        let mut stmt = conn.prepare(
            "SELECT effective_group, COALESCE(SUM(unread), 0) FROM v_cadence GROUP BY effective_group",
        )?;
        let groups = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for group in groups {
            let (name, unread) = group?;
            if let Some(g) = CadenceGroup::parse(&name) {
                cadence.insert(g, unread as usize);
            }
        }
        let counts = SourceCounts {
            journal: count(
                &format!(
                    "SELECT COUNT(*) FROM v_entry WHERE NOT is_read AND {LATEST_EDITION_ENTRIES}"
                ),
                vec![],
            )?,
            unread: count("SELECT COUNT(*) FROM v_entry WHERE NOT is_read", vec![])?,
            today: count(
                "SELECT COUNT(*) FROM v_entry WHERE published_at >= ?",
                vec![Value::Text(ts(today_start))],
            )?,
            starred: count("SELECT COUNT(*) FROM v_entry WHERE is_starred", vec![])?,
            needs_triage: count(
                "SELECT COUNT(*) FROM v_garden_row WHERE needs_triage AND state = 'active'",
                vec![],
            )?,
            cadence,
        };
        let folders = conn
            .prepare("SELECT id, parent_id, name, position, page FROM folder")?
            .query_map([], |r| {
                Ok(FolderLine {
                    id: r.get(0)?,
                    parent: r.get(1)?,
                    name: r.get(2)?,
                    position: r.get(3)?,
                    page: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let feeds = conn
            .prepare(
                "SELECT f.id, f.folder_id, f.position, f.title, f.page, COALESCE(u.unread, 0) \
                 FROM feed f LEFT JOIN v_unread_count u ON u.feed_id = f.id WHERE f.state = 'active'",
            )?
            .query_map([], |r| {
                Ok(FeedLine {
                    id: r.get(0)?,
                    folder: r.get(1)?,
                    position: r.get(2)?,
                    title: r.get(3)?,
                    page: r.get(4)?,
                    unread: r.get::<_, i64>(5)? as usize,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(SourcesSnapshot {
            counts,
            tree: children(None, &folders, &feeds),
        })
    }

    /// The entries of `source` (spec §4, Entry lists): newest first, pages of `PAGE_SIZE`
    /// strictly older than `after`; the Journal in rank order and whole.
    pub fn entries(
        &self,
        source: Source,
        filter: ListFilter,
        after: Option<&ListCursor>,
        today_start: DateTime<Utc>,
    ) -> Result<EntryPage> {
        let (mut list, args) = predicate(source, today_start);
        if filter == ListFilter::Unread {
            list.push_str(" AND NOT is_read");
        }
        self.page_of(&list, args, after, source == Source::Journal)
    }

    /// The rows of `list` (a `v_entry` condition and its arguments) and the counts of the
    /// whole list. `ranked`: the latest edition's order, whole; else newest first, paged.
    fn page_of(
        &self,
        list: &str,
        args: Vec<Value>,
        after: Option<&ListCursor>,
        ranked: bool,
    ) -> Result<EntryPage> {
        let conn = self.conn();
        let (total, unread): (i64, i64) = conn.query_row(
            &format!("SELECT COUNT(*), COALESCE(SUM(NOT is_read), 0) FROM v_entry WHERE {list}"),
            params_from_iter(args.iter()),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut bounded = list.to_string();
        let mut bound_args = args;
        let (order, limit) = if ranked {
            (format!("{EDITION_RANK}, entry_id"), String::new())
        } else {
            if let Some(cursor) = after {
                bounded.push_str(" AND (published_at, entry_id) < (?, ?)");
                bound_args.push(Value::Text(cursor.published_at.clone()));
                bound_args.push(Value::Integer(cursor.entry_id));
            }
            (
                "published_at DESC, entry_id DESC".to_string(),
                format!(" LIMIT {}", PAGE_SIZE + 1),
            )
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT {ROW_COLUMNS} FROM v_entry WHERE {bounded} ORDER BY {order}{limit}"
        ))?;
        let mut rows = stmt
            .query_map(params_from_iter(bound_args.iter()), entry_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let next = if !ranked && rows.len() > PAGE_SIZE {
            rows.truncate(PAGE_SIZE);
            rows.last().map(|(row, published)| ListCursor {
                published_at: published.clone(),
                entry_id: row.entry_id,
            })
        } else {
            None
        };
        Ok(EntryPage {
            rows: rows.into_iter().map(|(row, _)| row).collect(),
            total: total as usize,
            unread: unread as usize,
            next,
        })
    }

    /// The latest edition for the reader (spec §6, The Journal), `None` before the first one.
    pub fn journal(&self) -> Result<Option<JournalView>> {
        let Some(edition) = self.latest_edition()? else {
            return Ok(None);
        };
        let conn = self.conn();
        let items = conn
            .prepare(
                "SELECT v.entry_id, v.feed_id, v.feed_title, v.title, v.author, v.url, v.published_at, \
                 v.is_read, v.is_starred, v.has_fulltext, ee.section, ee.rank \
                 FROM edition_entry ee JOIN v_entry v ON v.entry_id = ee.entry_id \
                 WHERE ee.edition_id = ?1 ORDER BY ee.rank, ee.entry_id",
            )?
            .query_map([edition.id], |r| {
                Ok(JournalItem {
                    row: entry_row(r)?.0,
                    section: section_at(r, 10)?,
                    rank: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let overflow = conn
            .prepare(
                "SELECT section, feed_id, feed_title, page, hidden_count FROM v_edition_overflow \
                 WHERE feed_id IN (SELECT id FROM feed WHERE state = 'active') \
                 ORDER BY CASE section WHEN 'front' THEN 0 WHEN 'week' THEN 1 ELSE 2 END, \
                 page IS NULL, page, feed_id",
            )?
            .query_map([], |r| {
                Ok(EditionOverflowRow {
                    section: section_at(r, 0)?,
                    feed_id: r.get(1)?,
                    feed_title: r.get(2)?,
                    page: r.get(3)?,
                    hidden_count: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(JournalView {
            edition,
            items,
            overflow,
        }))
    }

    /// Full-text search within `source` (spec §4): every word must appear; the read state is
    /// ignored, as in the app; the Journal keeps its rank order.
    pub fn search_entries(
        &self,
        source: Source,
        text: &str,
        after: Option<&ListCursor>,
        today_start: DateTime<Utc>,
    ) -> Result<EntryPage> {
        let Some(query) = fts_query(text) else {
            return Ok(EntryPage::default());
        };
        let (filter, args) = predicate(source, today_start);
        let list = format!(
            "entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?) AND ({filter})"
        );
        let mut all = vec![Value::Text(query)];
        all.extend(args);
        self.page_of(&list, all, after, source == Source::Journal)
    }

    /// The entry for the reader, `None` when it is unknown or its feed is buried (`v_entry`).
    pub fn entry_detail(&self, entry_id: i64) -> Result<Option<EntryDetail>> {
        use rusqlite::OptionalExtension;
        let found = self
            .conn()
            .query_row(
                &format!(
                    "SELECT {ROW_COLUMNS}, summary_html, content_html FROM v_entry WHERE entry_id = ?1"
                ),
                [entry_id],
                |r| {
                    Ok((
                        entry_row(r)?.0,
                        r.get::<_, Option<String>>(10)?,
                        r.get::<_, Option<String>>(11)?,
                    ))
                },
            )
            .optional()?;
        let Some((row, summary_html, content_html)) = found else {
            return Ok(None);
        };
        let fulltext = self.fulltext(entry_id)?;
        let prefer_full_text = self.prefer_full_text(row.feed_id)?;
        Ok(Some(EntryDetail {
            row,
            summary_html,
            content_html,
            fulltext_html: fulltext.as_ref().and_then(|f| f.html.clone()),
            fulltext_error: fulltext.and_then(|f| f.error),
            prefer_full_text,
        }))
    }

    /// Every active, diagnosed feed's rhythm (`v_cadence`), by feed id.
    pub fn cadence_details(&self) -> Result<Vec<CadenceDetail>> {
        let rows = self
            .conn()
            .prepare(
                "SELECT feed_id, effective_group, computed_group, is_override, median_interval_days, \
                 entries_per_month, last_entry_at, unread FROM v_cadence ORDER BY feed_id",
            )?
            .query_map([], |r| {
                let last: Option<String> = r.get(6)?;
                Ok(CadenceDetail {
                    feed_id: r.get(0)?,
                    effective_group: group_at(r, 1)?,
                    computed_group: group_at(r, 2)?,
                    is_override: r.get(3)?,
                    median_interval_days: r.get(4)?,
                    entries_per_month: r.get(5)?,
                    last_entry_at: last
                        .map(|t| parse_ts(&t).ok_or_else(|| bad_timestamp(6)))
                        .transpose()?,
                    unread: r.get::<_, i64>(7)? as usize,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Marks one entry read (keeping its first read time) or unread. An unknown entry is a
    /// no-op, as in the app.
    pub fn set_read(&self, entry_id: i64, read: bool, now: DateTime<Utc>) -> Result<()> {
        if read {
            self.conn().execute(
                "UPDATE entry SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL",
                rusqlite::params![entry_id, ts(now)],
            )?;
        } else {
            self.conn()
                .execute("UPDATE entry SET read_at = NULL WHERE id = ?1", [entry_id])?;
        }
        Ok(())
    }

    pub fn set_starred(&self, entry_id: i64, starred: bool, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "UPDATE entry SET starred_at = ?2 WHERE id = ?1",
            rusqlite::params![entry_id, starred.then(|| ts(now))],
        )?;
        Ok(())
    }

    /// Marks every unread entry of `source` read and returns how many changed. The Journal
    /// marks its own entries only: overflow entries are not in the edition.
    pub fn mark_all_read(
        &self,
        source: Source,
        today_start: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<usize> {
        let (filter, args) = predicate(source, today_start);
        let mut all = vec![Value::Text(ts(now))];
        all.extend(args);
        let changed = self.conn().execute(
            &format!(
                "UPDATE entry SET read_at = ? WHERE read_at IS NULL \
                 AND id IN (SELECT entry_id FROM v_entry WHERE {filter})"
            ),
            params_from_iter(all.iter()),
        )?;
        Ok(changed)
    }

    /// SQLite's `PRAGMA data_version` of this connection: it changes when another connection
    /// (Ressac.app, another `ressac`) commits, so the reader knows when to reload.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    pub(super) fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// Local midnight of the tests' "today", in UTC.
    pub(super) fn today() -> DateTime<Utc> {
        at("2026-10-16T00:00:00Z")
    }

    pub(super) fn add_folder(
        s: &Store,
        id: i64,
        parent: Option<i64>,
        position: i64,
        page: Option<u16>,
    ) {
        s.conn()
            .execute(
                "INSERT INTO folder (id, parent_id, name, position, page) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, parent, format!("Folder {id}"), position, page],
            )
            .unwrap();
    }

    /// A feed with its folder, position, page, state and stored rhythm group.
    pub(super) fn add_feed(
        s: &Store,
        id: i64,
        folder: Option<i64>,
        position: i64,
        page: Option<u16>,
        state: &str,
        group: &str,
    ) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, \
                 state, added_at, page) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5, ?6, '2026-01-01T00:00:00.000Z', ?7)",
                params![
                    id,
                    folder,
                    position,
                    format!("Feed {id}"),
                    format!("https://f{id}.example.org/feed"),
                    state,
                    page
                ],
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

    /// An entry with its title, publication time, read and starred state, indexed for search.
    pub(super) fn add_entry(
        s: &Store,
        id: i64,
        feed: i64,
        title: &str,
        published: &str,
        read: bool,
        starred: bool,
    ) {
        let stamp = "2026-10-16T07:00:00.000Z";
        s.conn()
            .execute(
                "INSERT INTO entry (id, feed_id, guid, title, published_at, fetched_at, read_at, starred_at, \
                 summary_html) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    feed,
                    format!("g{id}"),
                    title,
                    ts(at(published)),
                    read.then_some(stamp),
                    starred.then_some(stamp),
                    format!("<p>{title}</p>")
                ],
            )
            .unwrap();
        super::super::reading::index_entry(
            s.conn(),
            id,
            Some(title),
            None,
            Some(&format!("<p>{title}</p>")),
        )
        .unwrap();
    }

    /// The latest edition: `items` as (entry, section, rank), `overflow` as (feed, section, hidden).
    pub(super) fn add_edition(
        s: &Store,
        id: i64,
        items: &[(i64, &str, u32)],
        overflow: &[(i64, &str, u32)],
    ) {
        s.conn()
            .execute(
                "INSERT INTO edition (id, built_at, window_start, window_end, unreachable) \
                 VALUES (?1, '2026-10-16T05:00:00.000Z', '2026-10-15T05:00:00.000Z', '2026-10-16T05:00:00.000Z', 0)",
                [id],
            )
            .unwrap();
        for (entry, section, rank) in items {
            s.conn()
                .execute(
                    "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (?1, ?2, ?3, ?4)",
                    params![id, entry, section, rank],
                )
                .unwrap();
        }
        for (feed, section, hidden) in overflow {
            s.conn()
                .execute(
                    "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (?1, ?2, ?3, ?4)",
                    params![id, feed, section, hidden],
                )
                .unwrap();
        }
    }

    pub(super) fn set_triage(s: &Store, feed: i64) {
        s.conn()
            .execute(
                "UPDATE feed_health SET needs_triage = 1 WHERE feed_id = ?1",
                [feed],
            )
            .unwrap();
    }

    /// Infos (1, page 400) > Sub (2, page 410) > feed 2; Infos > feed 1; feed 3 at the root;
    /// feed 4 buried in Infos.
    pub(super) fn library() -> Store {
        let s = Store::open_in_memory().unwrap();
        add_folder(&s, 1, None, 0, Some(400));
        add_folder(&s, 2, Some(1), 1, Some(410));
        add_feed(&s, 1, Some(1), 0, Some(401), "active", "daily");
        add_feed(&s, 2, Some(2), 0, Some(411), "active", "weekly");
        add_feed(&s, 3, None, 2, Some(110), "active", "rare");
        add_feed(&s, 4, Some(1), 2, Some(402), "buried", "daily");
        add_entry(
            &s,
            1,
            1,
            "Tides of March",
            "2026-10-16T06:00:00Z",
            false,
            false,
        );
        add_entry(&s, 2, 1, "Old news", "2026-10-14T06:00:00Z", true, true);
        add_entry(
            &s,
            3,
            2,
            "Weekly letter",
            "2026-10-15T09:00:00Z",
            false,
            true,
        );
        add_entry(&s, 4, 3, "Rare bird", "2026-10-16T01:00:00Z", false, false);
        add_entry(
            &s,
            5,
            4,
            "Buried tides",
            "2026-10-16T02:00:00Z",
            false,
            false,
        );
        s
    }

    #[test]
    fn counts_follow_the_app_rules() {
        let s = library();
        set_triage(&s, 3);
        add_edition(
            &s,
            1,
            &[(4, "front", 1), (1, "wire", 2), (5, "wire", 3)],
            &[],
        );
        let snap = s.sources(today()).unwrap();
        let c = &snap.counts;
        assert_eq!(
            c.unread, 3,
            "entries 1, 3, 4: the buried feed's entry 5 is not counted"
        );
        assert_eq!(c.today, 2, "entries 1 and 4 since local midnight");
        assert_eq!(c.starred, 2, "entries 2 and 3");
        assert_eq!(
            c.journal, 2,
            "entries 4 and 1; entry 5 belongs to a buried feed"
        );
        assert_eq!(c.needs_triage, 1);
        assert_eq!(c.cadence.len(), 5, "every group present");
        assert_eq!(c.cadence[&CadenceGroup::Daily], 1);
        assert_eq!(c.cadence[&CadenceGroup::Weekly], 1);
        assert_eq!(c.cadence[&CadenceGroup::Rare], 1);
        assert_eq!(c.cadence[&CadenceGroup::Monthly], 0);
        assert_eq!(c.cadence[&CadenceGroup::Silent], 0);
    }

    #[test]
    fn the_tree_is_the_sidebar_order_with_subtree_counts() {
        let snap = library().sources(today()).unwrap();
        let names: Vec<String> = snap
            .tree
            .iter()
            .map(|n| match &n.kind {
                SourceNodeKind::Folder { name, .. } => name.clone(),
                SourceNodeKind::Feed { title, .. } => title.clone(),
            })
            .collect();
        assert_eq!(
            names,
            ["Folder 1", "Feed 3"],
            "position 0 before position 2"
        );
        let infos = &snap.tree[0];
        assert_eq!(infos.page, Some(400));
        assert_eq!(
            infos.unread, 2,
            "feed 1 (1 unread) + Sub (feed 2, 1 unread); buried feed 4 left out"
        );
        let kinds: Vec<&SourceNodeKind> = infos.children.iter().map(|n| &n.kind).collect();
        assert_eq!(
            kinds,
            [
                &SourceNodeKind::Feed {
                    id: 1,
                    title: "Feed 1".into()
                },
                &SourceNodeKind::Folder {
                    id: 2,
                    name: "Folder 2".into()
                },
            ],
            "same position 0 would put the folder first; here feed 1 is at 0 and Sub at 1"
        );
        assert_eq!(infos.children[1].children[0].page, Some(411));
        assert_eq!(snap.tree[1].unread, 1);
    }

    #[test]
    fn a_folder_comes_before_a_feed_at_the_same_position() {
        let s = Store::open_in_memory().unwrap();
        add_folder(&s, 7, None, 0, None);
        add_feed(&s, 9, None, 0, None, "active", "daily");
        let snap = s.sources(today()).unwrap();
        assert!(matches!(
            snap.tree[0].kind,
            SourceNodeKind::Folder { id: 7, .. }
        ));
        assert!(matches!(
            snap.tree[1].kind,
            SourceNodeKind::Feed { id: 9, .. }
        ));
        assert_eq!(snap.tree[0].unread, 0);
    }

    fn ids(page: &EntryPage) -> Vec<i64> {
        page.rows.iter().map(|r| r.entry_id).collect()
    }

    #[test]
    fn lists_are_newest_first_and_follow_their_source() {
        let s = library();
        let all = |src| ids(&s.entries(src, ListFilter::All, None, today()).unwrap());
        assert_eq!(
            all(Source::Unread),
            [1, 4, 3],
            "newest first; buried feed's entry 5 absent"
        );
        assert_eq!(all(Source::Today), [1, 4]);
        assert_eq!(all(Source::Starred), [3, 2]);
        assert_eq!(all(Source::Feed(1)), [1, 2]);
        assert_eq!(
            all(Source::Folder(1)),
            [1, 3, 2],
            "Infos and its subfolder Sub"
        );
        assert_eq!(all(Source::Cadence(CadenceGroup::Weekly)), [3]);
        let unread = s
            .entries(Source::Feed(1), ListFilter::Unread, None, today())
            .unwrap();
        assert_eq!(ids(&unread), [1]);
        assert_eq!((unread.total, unread.unread), (1, 1));
        let feed = s
            .entries(Source::Feed(1), ListFilter::All, None, today())
            .unwrap();
        assert_eq!((feed.total, feed.unread, feed.next.clone()), (2, 1, None));
        let row = &feed.rows[1];
        assert_eq!(row.title.as_deref(), Some("Old news"));
        assert!(row.is_read && row.is_starred && !row.has_fulltext);
        assert_eq!(row.feed_title, "Feed 1");
        assert_eq!(row.published_at, at("2026-10-14T06:00:00Z"));
    }

    #[test]
    fn a_long_list_comes_in_pages_of_500() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, None, 0, None, "active", "daily");
        for i in 0..(PAGE_SIZE as i64 + 2) {
            let published =
                (at("2026-10-01T00:00:00Z") + chrono::Duration::minutes(i)).to_rfc3339();
            add_entry(&s, i + 1, 1, &format!("E{i}"), &published, false, false);
        }
        let first = s
            .entries(Source::Feed(1), ListFilter::All, None, today())
            .unwrap();
        assert_eq!(first.rows.len(), PAGE_SIZE);
        assert_eq!(first.total, PAGE_SIZE + 2);
        assert_eq!(first.rows[0].entry_id, PAGE_SIZE as i64 + 2, "newest first");
        let cursor = first.next.clone().expect("more rows remain");
        assert_eq!(cursor.entry_id, 3);
        let second = s
            .entries(Source::Feed(1), ListFilter::All, Some(&cursor), today())
            .unwrap();
        assert_eq!(ids(&second), [2, 1]);
        assert_eq!(second.next, None);
        assert_eq!(second.total, PAGE_SIZE + 2, "counts cover the whole list");
    }

    #[test]
    fn entries_published_at_the_same_time_page_by_id() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, None, 0, None, "active", "daily");
        for i in 1..=(PAGE_SIZE as i64 + 1) {
            add_entry(&s, i, 1, "Same", "2026-10-10T00:00:00Z", false, false);
        }
        let first = s
            .entries(Source::Feed(1), ListFilter::All, None, today())
            .unwrap();
        let second = s
            .entries(
                Source::Feed(1),
                ListFilter::All,
                first.next.as_ref(),
                today(),
            )
            .unwrap();
        assert_eq!(
            ids(&second),
            [1],
            "no row lost or repeated across the page boundary"
        );
    }

    #[test]
    fn the_journal_is_in_rank_order_and_leaves_buried_feeds_out() {
        let s = library();
        // Ranks differ from dates: 3 (2026-10-15) first, then 4, then 1; 5 is a buried feed's.
        add_edition(
            &s,
            1,
            &[
                (3, "week", 1),
                (5, "wire", 2),
                (4, "front", 3),
                (1, "wire", 4),
            ],
            &[(1, "wire", 12), (4, "wire", 3), (3, "front", 1)],
        );
        let list = s
            .entries(Source::Journal, ListFilter::All, None, today())
            .unwrap();
        assert_eq!(ids(&list), [3, 4, 1]);
        assert_eq!(list.next, None, "always whole");
        let view = s.journal().unwrap().expect("an edition exists");
        assert_eq!(view.edition.id, 1);
        let items: Vec<(i64, Section, u32)> = view
            .items
            .iter()
            .map(|i| (i.row.entry_id, i.section, i.rank))
            .collect();
        assert_eq!(
            items,
            [
                (3, Section::Week, 1),
                (4, Section::Front, 3),
                (1, Section::Wire, 4)
            ]
        );
        let overflow: Vec<(i64, Section)> = view
            .overflow
            .iter()
            .map(|o| (o.feed_id, o.section))
            .collect();
        assert_eq!(
            overflow,
            [(3, Section::Front), (1, Section::Wire)],
            "front first; buried feed 4 dropped"
        );
        assert_eq!(view.overflow[1].hidden_count, 12);
        assert_eq!(view.overflow[1].page, Some(401));
    }

    #[test]
    fn no_edition_means_no_journal() {
        let s = library();
        assert_eq!(s.journal().unwrap(), None);
        assert!(
            s.entries(Source::Journal, ListFilter::All, None, today())
                .unwrap()
                .rows
                .is_empty()
        );
    }

    #[test]
    fn search_matches_every_word_inside_the_source() {
        let s = library();
        let found = |src, text: &str| ids(&s.search_entries(src, text, None, today()).unwrap());
        assert_eq!(
            found(Source::Unread, "tides"),
            [1],
            "entry 5 matches but its feed is buried"
        );
        assert_eq!(found(Source::Feed(1), "tides march"), [1]);
        assert_eq!(
            found(Source::Feed(1), "tides weekly"),
            Vec::<i64>::new(),
            "every word must match"
        );
        assert_eq!(
            found(Source::Starred, "news"),
            [2],
            "search ignores the read state"
        );
        assert_eq!(found(Source::Feed(2), "tides"), Vec::<i64>::new());
    }

    #[test]
    fn blank_or_quoted_searches_are_safe() {
        let s = library();
        assert_eq!(
            s.search_entries(Source::Unread, "   ", None, today())
                .unwrap(),
            EntryPage::default()
        );
        assert!(
            s.search_entries(Source::Unread, "a\"b OR", None, today())
                .is_ok()
        );
        assert_eq!(fts_query("tides  of"), Some("\"tides\" \"of\"".to_string()));
        assert_eq!(fts_query("a\"b"), Some("\"a\"\"b\"".to_string()));
        assert_eq!(fts_query(" "), None);
    }

    #[test]
    fn search_ids_is_public_for_the_terminal() {
        let s = library();
        assert_eq!(
            s.search_ids("\"tides\"").unwrap(),
            [1, 5],
            "the raw FTS index, buried feeds included"
        );
    }

    fn add_fulltext(s: &Store, entry: i64, html: Option<&str>, error: Option<&str>) {
        s.conn()
            .execute(
                "INSERT INTO entry_fulltext (entry_id, fetched_at, html, error) \
                 VALUES (?1, '2026-10-16T07:00:00.000Z', ?2, ?3)",
                params![entry, html, error],
            )
            .unwrap();
    }

    #[test]
    fn entry_detail_carries_every_body_and_the_feed_preference() {
        let s = library();
        s.conn()
            .execute("UPDATE entry SET content_html = '<p>Full feed body</p>', author = 'Ann', url = 'https://f1.example.org/a' WHERE id = 1", [])
            .unwrap();
        add_fulltext(&s, 1, Some("<article>Extracted</article>"), None);
        s.set_prefer_full_text(1, true).unwrap();
        let d = s.entry_detail(1).unwrap().expect("active entry");
        assert_eq!(d.row.entry_id, 1);
        assert!(d.row.has_fulltext);
        assert_eq!(d.row.author.as_deref(), Some("Ann"));
        assert_eq!(d.summary_html.as_deref(), Some("<p>Tides of March</p>"));
        assert_eq!(d.content_html.as_deref(), Some("<p>Full feed body</p>"));
        assert_eq!(
            d.fulltext_html.as_deref(),
            Some("<article>Extracted</article>")
        );
        assert_eq!(d.fulltext_error, None);
        assert!(d.prefer_full_text);
    }

    #[test]
    fn a_failed_extraction_and_unknown_or_buried_entries() {
        let s = library();
        add_fulltext(&s, 3, None, Some("HTTP 404."));
        let d = s.entry_detail(3).unwrap().unwrap();
        assert_eq!(
            (d.fulltext_html, d.fulltext_error.as_deref()),
            (None, Some("HTTP 404."))
        );
        assert!(!d.prefer_full_text);
        assert_eq!(s.entry_detail(5).unwrap(), None, "entry of a buried feed");
        assert_eq!(s.entry_detail(999).unwrap(), None);
    }

    #[test]
    fn cadence_details_feed_the_rhythm_line() {
        let s = library();
        s.conn()
            .execute(
                "UPDATE feed_health SET median_interval_days = 0.4, last_entry_at = '2026-10-16T06:00:00.000Z', \
                 entries_per_month = 30.5 WHERE feed_id = 1",
                [],
            )
            .unwrap();
        s.conn()
            .execute(
                "UPDATE feed SET cadence_override = 'monthly' WHERE id = 3",
                [],
            )
            .unwrap();
        let rows = s.cadence_details().unwrap();
        let one = rows.iter().find(|r| r.feed_id == 1).unwrap();
        assert_eq!(one.effective_group, CadenceGroup::Daily);
        assert!(!one.is_override);
        assert_eq!(one.median_interval_days, Some(0.4));
        assert_eq!(one.last_entry_at, Some(at("2026-10-16T06:00:00Z")));
        assert_eq!(one.unread, 1);
        let three = rows.iter().find(|r| r.feed_id == 3).unwrap();
        assert_eq!(
            (
                three.effective_group,
                three.computed_group,
                three.is_override
            ),
            (CadenceGroup::Monthly, CadenceGroup::Rare, true)
        );
        assert!(
            rows.iter().all(|r| r.feed_id != 4),
            "buried feeds are not in v_cadence"
        );
    }

    fn read_at(s: &Store, id: i64) -> Option<String> {
        s.conn()
            .query_row("SELECT read_at FROM entry WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    #[test]
    fn reading_keeps_the_first_read_time_and_unreading_clears_it() {
        let s = library();
        s.set_read(1, true, at("2026-10-16T08:00:00Z")).unwrap();
        s.set_read(1, true, at("2026-10-16T09:00:00Z")).unwrap();
        assert_eq!(read_at(&s, 1).as_deref(), Some("2026-10-16T08:00:00.000Z"));
        s.set_read(1, false, at("2026-10-16T10:00:00Z")).unwrap();
        assert_eq!(read_at(&s, 1), None);
        s.set_read(999, true, at("2026-10-16T10:00:00Z")).unwrap(); // unknown: a no-op
    }

    #[test]
    fn starring_sets_and_clears() {
        let s = library();
        s.set_starred(1, true, at("2026-10-16T08:00:00Z")).unwrap();
        assert!(s.entry_detail(1).unwrap().unwrap().row.is_starred);
        s.set_starred(1, false, at("2026-10-16T08:00:00Z")).unwrap();
        assert!(!s.entry_detail(1).unwrap().unwrap().row.is_starred);
    }

    #[test]
    fn mark_all_read_covers_its_source_only() {
        let s = library();
        let now = at("2026-10-16T08:00:00Z");
        assert_eq!(s.mark_all_read(Source::Feed(2), today(), now).unwrap(), 1);
        assert_eq!(
            s.sources(today()).unwrap().counts.unread,
            2,
            "entries 1 and 4 still unread"
        );
        assert_eq!(
            s.mark_all_read(Source::Feed(2), today(), now).unwrap(),
            0,
            "nothing left"
        );
    }

    #[test]
    fn mark_all_read_on_the_journal_leaves_the_overflow_unread() {
        let s = library();
        add_entry(&s, 6, 1, "Overflowed", "2026-10-16T05:00:00Z", false, false);
        // Entry 6 is in feed 1's overflow, not in the edition.
        add_edition(
            &s,
            1,
            &[(4, "front", 1), (1, "wire", 2), (5, "wire", 3)],
            &[(1, "wire", 1)],
        );
        let changed = s
            .mark_all_read(Source::Journal, today(), at("2026-10-16T08:00:00Z"))
            .unwrap();
        assert_eq!(changed, 2, "entries 4 and 1; entry 5's feed is buried");
        assert_eq!(
            read_at(&s, 6),
            None,
            "the overflow stays unread in its feed"
        );
        assert_eq!(read_at(&s, 5), None, "a buried feed's entry is not touched");
        assert_eq!(s.sources(today()).unwrap().counts.journal, 0);
    }

    #[test]
    fn another_connection_s_write_changes_data_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        let reader = Store::open(&path).unwrap();
        let writer = Store::open(&path).unwrap();
        add_feed(&writer, 1, None, 0, None, "active", "daily");
        add_entry(&writer, 1, 1, "Hello", "2026-10-16T06:00:00Z", false, false);
        let before = reader.data_version().unwrap();
        assert_eq!(
            reader.data_version().unwrap(),
            before,
            "stable while nobody writes"
        );
        writer
            .set_read(1, true, at("2026-10-16T08:00:00Z"))
            .unwrap();
        let after = reader.data_version().unwrap();
        assert_ne!(after, before, "the app's write is seen by the reader");
        assert_eq!(reader.sources(today()).unwrap().counts.unread, 0);
    }
}
