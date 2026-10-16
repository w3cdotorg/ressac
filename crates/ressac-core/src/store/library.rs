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
}
