//! Teletext page numbers (schema v3). The core gives every folder and feed a number once
//! and never changes it on its own; the apps read them through `v_page`.

use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;

use crate::error::{Error, Result};

/// Pages fixed in code and never stored: Index, Unread, Today, Starred, the Journal and the
/// five rhythm groups, the three garden pages and the Guide.
pub const RESERVED_PAGES: [u16; 14] = [
    100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899,
];
/// The pages a folder or a feed may hold.
pub const ASSIGNABLE: RangeInclusive<u16> = 110..=799;
/// Titles of `RESERVED_PAGES`, in the same order.
const RESERVED_TITLES: [&str; 14] = [
    "Index",
    "Unread",
    "Today",
    "Starred",
    "Journal",
    "Daily",
    "Weekly",
    "Monthly",
    "Rare",
    "Silent",
    "Needs triage",
    "All feeds",
    "Cemetery",
    "Guide",
];

/// Feeds without a folder.
const ROOT_FEEDS: RangeInclusive<u16> = 110..=199;
/// Root folders that get a hundred of their own (200 to 700).
const ROOT_HUNDREDS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Folder,
    Feed,
    Reserved,
}

impl PageKind {
    /// The `kind` of `v_page` and of the CLI's lines: "folder", "feed" or "reserved".
    pub fn as_str(self) -> &'static str {
        match self {
            PageKind::Folder => "folder",
            PageKind::Feed => "feed",
            PageKind::Reserved => "reserved",
        }
    }
}

/// One line of `ressac page list`: a reserved page (no id), or a numbered folder or feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageRow {
    pub page: u16,
    pub kind: PageKind,
    pub id: Option<i64>,
    pub title: String,
    pub parent_page: Option<u16>,
}

/// A folder or a feed as the assignment sees it.
#[derive(Debug, Clone, Copy)]
struct Node {
    kind: PageKind,
    id: i64,
    position: i64,
    page: Option<u16>,
}

impl Node {
    /// Display order inside a parent: position, then folders before feeds, then id
    /// (the app's `SidebarNode.tree`).
    fn order(&self) -> (i64, u8, i64) {
        let rank = if self.kind == PageKind::Folder { 0 } else { 1 };
        (self.position, rank, self.id)
    }
}

/// Every folder and feed, grouped by parent folder (`None` = top level), in display order.
struct Tree {
    children: HashMap<Option<i64>, Vec<Node>>,
}

impl Tree {
    fn load(conn: &Connection) -> Result<Self> {
        let mut children: HashMap<Option<i64>, Vec<Node>> = HashMap::new();
        for (kind, sql) in [
            (
                PageKind::Folder,
                "SELECT id, parent_id, position, page FROM folder",
            ),
            (
                PageKind::Feed,
                "SELECT id, folder_id, position, page FROM feed",
            ),
        ] {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, Option<i64>>(1)?,
                        Node {
                            kind,
                            id: r.get(0)?,
                            position: r.get(2)?,
                            page: r.get(3)?,
                        },
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (parent, node) in rows {
                children.entry(parent).or_default().push(node);
            }
        }
        for nodes in children.values_mut() {
            nodes.sort_by_key(Node::order);
        }
        Ok(Tree { children })
    }

    fn children(&self, parent: Option<i64>) -> &[Node] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// The contents of a folder, depth-first, each folder before its own contents.
    fn descendants(&self, folder: i64, out: &mut Vec<Node>) {
        for node in self.children(Some(folder)) {
            out.push(*node);
            if node.kind == PageKind::Folder {
                self.descendants(node.id, out);
            }
        }
    }

    fn used(&self) -> BTreeSet<u16> {
        self.children
            .values()
            .flatten()
            .filter_map(|n| n.page)
            .collect()
    }
}

/// The lowest number of `range` that nobody holds.
fn lowest_free(used: &BTreeSet<u16>, range: RangeInclusive<u16>) -> Option<u16> {
    range.into_iter().find(|n| !used.contains(n))
}

/// The overflow pool: the lowest free assignable number that is neither reserved nor a
/// hundred.
fn pool_next(used: &BTreeSet<u16>) -> Option<u16> {
    ASSIGNABLE
        .into_iter()
        .find(|n| !n.is_multiple_of(100) && !RESERVED_PAGES.contains(n) && !used.contains(n))
}

/// 200, 300, … 700: a root folder's own hundred.
fn is_hundred(page: u16) -> bool {
    page.is_multiple_of(100) && (200..=700).contains(&page)
}

/// What one pass of `assign_missing` has decided so far.
struct Assignment {
    used: BTreeSet<u16>,
    assigned: Vec<(Node, u16)>,
    /// Items with no home range or a full one, in display order, for the pool.
    overflow: Vec<Node>,
}

impl Assignment {
    /// Returns the node's page: the one it holds, else the lowest free number of `home`.
    /// `None` means it waits for the overflow pool.
    fn place(&mut self, node: Node, home: Option<RangeInclusive<u16>>) -> Option<u16> {
        if node.page.is_some() {
            return node.page;
        }
        match home.and_then(|range| lowest_free(&self.used, range)) {
            Some(page) => {
                self.used.insert(page);
                self.assigned.push((node, page));
                Some(page)
            }
            None => {
                self.overflow.push(node);
                None
            }
        }
    }

    /// Second pass: every overflowed item takes the lowest free number of the pool. With
    /// none left (more than 684 items), the item keeps no page.
    fn drain_overflow(&mut self) {
        for node in std::mem::take(&mut self.overflow) {
            match pool_next(&self.used) {
                Some(page) => {
                    self.used.insert(page);
                    self.assigned.push((node, page));
                }
                None => {
                    tracing::warn!(id = node.id, kind = node.kind.as_str(), "no free page left")
                }
            }
        }
    }
}

/// Gives a number to every folder and feed whose `page` is NULL; returns how many were
/// assigned. Numbers already given never change. Items with a home range (a root folder's
/// hundred, its contents, the root feeds' 110–199) are placed first, in display order;
/// the others then take the lowest free numbers of the overflow pool, in the same order.
pub fn assign_missing(conn: &Connection) -> Result<usize> {
    let tree = Tree::load(conn)?;
    let mut pass = Assignment {
        used: tree.used(),
        assigned: Vec::new(),
        overflow: Vec::new(),
    };
    let mut root_folders = 0;
    for &node in tree.children(None) {
        match node.kind {
            PageKind::Folder => {
                let hundred =
                    (root_folders < ROOT_HUNDREDS).then(|| (root_folders as u16 + 2) * 100);
                root_folders += 1;
                let page = pass.place(node, hundred.map(|h| h..=h));
                let home = page.filter(|p| is_hundred(*p)).map(|h| h + 1..=h + 99);
                let mut inside = Vec::new();
                tree.descendants(node.id, &mut inside);
                for child in inside {
                    pass.place(child, home.clone());
                }
            }
            PageKind::Feed => {
                pass.place(node, Some(ROOT_FEEDS));
            }
            PageKind::Reserved => {}
        }
    }
    pass.drain_overflow();
    for (node, page) in &pass.assigned {
        let sql = match node.kind {
            PageKind::Folder => "UPDATE folder SET page = ?2 WHERE id = ?1",
            _ => "UPDATE feed SET page = ?2 WHERE id = ?1",
        };
        conn.execute(sql, params![node.id, page])?;
    }
    Ok(pass.assigned.len())
}

/// Reserved pages first (in `RESERVED_PAGES` order), then every row of `v_page` by page.
pub fn list(conn: &Connection) -> Result<Vec<PageRow>> {
    let mut rows: Vec<PageRow> = RESERVED_PAGES
        .iter()
        .zip(RESERVED_TITLES)
        .map(|(&page, title)| PageRow {
            page,
            kind: PageKind::Reserved,
            id: None,
            title: title.to_string(),
            parent_page: None,
        })
        .collect();
    let mut stmt = conn.prepare(
        "SELECT page, kind, ref_id, title, parent_page FROM v_page ORDER BY page, kind, ref_id",
    )?;
    let stored = stmt
        .query_map([], |r| {
            let kind: String = r.get(1)?;
            let kind = match kind.as_str() {
                "folder" => PageKind::Folder,
                "feed" => PageKind::Feed,
                _ => {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        format!("invalid kind {kind:?} in v_page").into(),
                    ));
                }
            };
            Ok(PageRow {
                page: r.get(0)?,
                kind,
                id: Some(r.get(2)?),
                title: r.get(3)?,
                parent_page: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.extend(stored);
    Ok(rows)
}

/// Renumbers one folder or feed; `kind` must be Folder or Feed. Refuses a reserved page,
/// a page outside `ASSIGNABLE`, an unknown id and a page held by any other folder or feed;
/// nothing is written then. Giving an item the page it already holds changes nothing.
/// Opens its own write transaction, so it must not run inside another one.
pub fn set(conn: &Connection, kind: PageKind, id: i64, page: u16) -> Result<()> {
    let table = match kind {
        PageKind::Folder => "folder",
        PageKind::Feed => "feed",
        PageKind::Reserved => {
            return Err(Error::Invalid(
                "Only a folder or a feed can be renumbered.".into(),
            ));
        }
    };
    if RESERVED_PAGES.contains(&page) {
        return Err(Error::PageReserved(page));
    }
    if !ASSIGNABLE.contains(&page) {
        return Err(Error::PageOutOfRange(page));
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let current: Option<Option<u16>> = tx
        .query_row(
            &format!("SELECT page FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(current) = current else {
        return Err(Error::UnknownPageTarget {
            kind: kind.as_str(),
            id,
        });
    };
    if current == Some(page) {
        return Ok(());
    }
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM folder WHERE page = ?1) OR EXISTS (SELECT 1 FROM feed WHERE page = ?1)",
        [page],
        |r| r.get(0),
    )?;
    if taken {
        return Err(Error::PageTaken { page });
    }
    tx.execute(
        &format!("UPDATE {table} SET page = ?2 WHERE id = ?1"),
        params![id, page],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::json;

    use super::*;
    use crate::model::{Action, Decision, Status};
    use crate::store::Store;

    fn now() -> DateTime<Utc> {
        "2026-10-14T08:00:00Z".parse().unwrap()
    }

    /// A folder named "Folder <id>".
    fn folder(s: &Store, id: i64, parent: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO folder (id, parent_id, name, position) VALUES (?1, ?2, ?3, ?4)",
                params![id, parent, format!("Folder {id}"), position],
            )
            .unwrap();
    }

    /// An active feed titled "Feed <id>", at https://f<id>.example.org/feed.
    fn feed(s: &Store, id: i64, folder: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, '2026-10-14T08:00:00.000Z')",
                params![
                    id,
                    folder,
                    position,
                    format!("Feed {id}"),
                    format!("https://f{id}.example.org/feed"),
                    format!("f{id}.example.org/feed")
                ],
            )
            .unwrap();
    }

    fn folder_page(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM folder WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn feed_page(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM feed WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn assign(s: &Store) -> usize {
        assign_missing(s.conn()).unwrap()
    }

    #[test]
    fn root_folders_take_hundreds_in_display_order() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 2);
        folder(&s, 2, None, 0);
        feed(&s, 10, None, 0); // same position as folder 2: folders come first
        folder(&s, 3, None, 1);
        folder(&s, 4, None, 1); // same position as folder 3: lower id first
        assert_eq!(assign(&s), 5);
        assert_eq!(
            [2, 3, 4, 1].map(|id| folder_page(&s, id)),
            [Some(200), Some(300), Some(400), Some(500)]
        );
        assert_eq!(
            feed_page(&s, 10),
            Some(110),
            "a root feed is not a root folder"
        );
    }

    #[test]
    fn contents_fill_their_hundred_depth_first_in_display_order() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0); // Infos
        feed(&s, 10, Some(1), 0); // A
        folder(&s, 2, Some(1), 1); // Sub, before E at the same position
        feed(&s, 11, Some(2), 0); // B
        feed(&s, 12, Some(2), 1); // C
        feed(&s, 14, Some(1), 1); // E
        feed(&s, 13, Some(1), 2); // D
        assign(&s);
        assert_eq!(folder_page(&s, 1), Some(200));
        assert_eq!(
            [
                feed_page(&s, 10),
                folder_page(&s, 2),
                feed_page(&s, 11),
                feed_page(&s, 12)
            ],
            [Some(201), Some(202), Some(203), Some(204)],
            "pre-order: a folder before its contents"
        );
        assert_eq!(
            [feed_page(&s, 14), feed_page(&s, 13)],
            [Some(205), Some(206)]
        );
    }

    #[test]
    fn root_feeds_take_110_to_199() {
        let s = Store::open_in_memory().unwrap();
        feed(&s, 1, None, 2);
        feed(&s, 2, None, 0);
        feed(&s, 3, None, 1);
        folder(&s, 4, None, 3);
        assign(&s);
        assert_eq!(
            [2, 3, 1].map(|id| feed_page(&s, id)),
            [Some(110), Some(111), Some(112)]
        );
        assert_eq!(folder_page(&s, 4), Some(200));
    }

    #[test]
    fn a_full_hundred_overflows_after_every_item_with_a_home() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        for id in 1..=100 {
            feed(&s, id, Some(1), id);
        }
        feed(&s, 101, None, 1); // a root feed shown after the full folder
        assert_eq!(assign(&s), 102);
        assert_eq!(feed_page(&s, 1), Some(201));
        assert_eq!(feed_page(&s, 99), Some(299));
        assert_eq!(
            feed_page(&s, 101),
            Some(110),
            "homed items are placed first"
        );
        assert_eq!(
            feed_page(&s, 100),
            Some(111),
            "then the overflow takes the lowest free"
        );
    }

    #[test]
    fn a_seventh_root_folder_and_its_contents_take_the_lowest_free_pages() {
        let s = Store::open_in_memory().unwrap();
        for i in 1..=7 {
            folder(&s, i, None, i);
            feed(&s, 10 + i, Some(i), 0);
        }
        feed(&s, 20, None, 8);
        assign(&s);
        assert_eq!(
            (1..=6).map(|i| folder_page(&s, i)).collect::<Vec<_>>(),
            [200, 300, 400, 500, 600, 700].map(Some)
        );
        assert_eq!(feed_page(&s, 16), Some(701));
        assert_eq!(feed_page(&s, 20), Some(110));
        assert_eq!(folder_page(&s, 7), Some(111));
        assert_eq!(
            feed_page(&s, 17),
            Some(112),
            "no hundred, so its contents overflow too"
        );
    }

    #[test]
    fn the_pool_never_yields_a_reserved_page_or_a_hundred() {
        let mut used = BTreeSet::new();
        while let Some(page) = pool_next(&used) {
            assert!(ASSIGNABLE.contains(&page), "{page}");
            assert!(!RESERVED_PAGES.contains(&page), "{page}");
            assert!(!page.is_multiple_of(100), "{page}");
            used.insert(page);
        }
        assert_eq!(used.len(), 690 - 6, "110…799 minus 200, 300, … 700");
        assert_eq!(used.first(), Some(&110));
        assert_eq!(used.last(), Some(&799));
    }

    #[test]
    fn overflow_skips_a_free_hundred() {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=91 {
            feed(&s, id, None, id);
        }
        assign(&s);
        assert_eq!(feed_page(&s, 90), Some(199));
        assert_eq!(feed_page(&s, 91), Some(201), "200 is free but a hundred");
    }

    #[test]
    fn an_exhausted_pool_leaves_the_last_items_unnumbered() {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=685 {
            feed(&s, id, None, id);
        }
        assert_eq!(
            assign(&s),
            684,
            "110…799 holds 684 numbers outside the hundreds"
        );
        assert_eq!(feed_page(&s, 684), Some(799));
        assert_eq!(feed_page(&s, 685), None);
    }

    #[test]
    fn existing_numbers_are_never_changed() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        feed(&s, 11, Some(1), 1);
        assert_eq!(assign(&s), 3);
        s.conn()
            .execute("UPDATE feed SET page = 150 WHERE id = 10", [])
            .unwrap();
        feed(&s, 12, Some(1), 2);
        feed(&s, 13, Some(1), -1);
        assert_eq!(assign(&s), 2, "only the NULL pages");
        assert_eq!(feed_page(&s, 10), Some(150));
        assert_eq!(feed_page(&s, 11), Some(202));
        assert_eq!(
            feed_page(&s, 13),
            Some(201),
            "the lowest free number of the hundred"
        );
        assert_eq!(feed_page(&s, 12), Some(203));
        assert_eq!(assign(&s), 0, "idempotent");
    }

    #[test]
    fn buried_and_merged_feeds_keep_their_numbers() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        for id in [10, 11, 12] {
            feed(&s, id, Some(1), id);
        }
        assign(&s);
        s.record_decision(10, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        s.record_decision(
            11,
            Decision::Repair,
            Status::Duplicate,
            Some(&Action::MergeInto { feed_id: 12 }),
            now(),
        )
        .unwrap();
        assert_eq!(assign(&s), 0);
        assert_eq!(
            [10, 11, 12].map(|id| feed_page(&s, id)),
            [Some(201), Some(202), Some(203)]
        );
        feed(&s, 13, Some(1), 13);
        assign(&s);
        assert_eq!(feed_page(&s, 13), Some(204), "201 and 202 are still held");
    }

    #[test]
    fn folders_and_feeds_share_one_number_space() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        folder(&s, 2, Some(1), 1);
        feed(&s, 10, Some(1), 0);
        folder(&s, 3, None, 1);
        feed(&s, 20, None, 2);
        s.conn()
            .execute_batch(
                "UPDATE folder SET page = 201 WHERE id = 2; UPDATE feed SET page = 300 WHERE id = 20;",
            )
            .unwrap();
        assign(&s);
        assert_eq!(feed_page(&s, 10), Some(202), "201 is a folder's");
        assert_eq!(
            folder_page(&s, 3),
            Some(110),
            "300 is a feed's, so the second root folder overflows"
        );
    }

    #[test]
    fn list_gives_reserved_pages_then_v_page_by_page() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        folder(&s, 2, Some(1), 1);
        feed(&s, 11, None, 1);
        assign(&s);
        s.record_decision(10, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        let rows = list(s.conn()).unwrap();
        let reserved: Vec<(u16, &str)> = rows[..14]
            .iter()
            .map(|r| (r.page, r.title.as_str()))
            .collect();
        assert_eq!(
            reserved,
            [
                (100, "Index"),
                (101, "Unread"),
                (102, "Today"),
                (103, "Starred"),
                (104, "Journal"),
                (105, "Daily"),
                (106, "Weekly"),
                (107, "Monthly"),
                (108, "Rare"),
                (109, "Silent"),
                (800, "Needs triage"),
                (801, "All feeds"),
                (802, "Cemetery"),
                (899, "Guide")
            ]
        );
        assert!(
            rows[..14]
                .iter()
                .all(|r| r.kind == PageKind::Reserved && r.id.is_none())
        );
        let stored: Vec<(u16, PageKind, Option<i64>, Option<u16>)> = rows[14..]
            .iter()
            .map(|r| (r.page, r.kind, r.id, r.parent_page))
            .collect();
        assert_eq!(
            stored,
            [
                (110, PageKind::Feed, Some(11), None),
                (200, PageKind::Folder, Some(1), None),
                (201, PageKind::Feed, Some(10), Some(200)),
                (202, PageKind::Folder, Some(2), Some(200)),
            ],
            "the buried feed 10 stays listed"
        );
        assert_eq!(
            serde_json::to_value(&rows[16]).unwrap(),
            json!({"page": 201, "kind": "feed", "id": 10, "title": "Feed 10", "parent_page": 200})
        );
        assert_eq!(
            serde_json::to_value(&rows[0]).unwrap(),
            json!({"page": 100, "kind": "reserved", "id": null, "title": "Index", "parent_page": null})
        );
    }

    /// Root folder 1 (200) holding feeds 10 (201) and 11 (202), subfolder 2 (203).
    fn numbered() -> Store {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        feed(&s, 11, Some(1), 1);
        folder(&s, 2, Some(1), 2);
        assign(&s);
        s
    }

    #[test]
    fn set_renumbers_a_feed_or_a_folder() {
        let s = numbered();
        set(s.conn(), PageKind::Feed, 10, 412).unwrap();
        set(s.conn(), PageKind::Folder, 2, 150).unwrap();
        assert_eq!(feed_page(&s, 10), Some(412));
        assert_eq!(folder_page(&s, 2), Some(150));
        set(s.conn(), PageKind::Feed, 10, 412).unwrap(); // its own page: nothing to do
        set(s.conn(), PageKind::Feed, 11, 201).unwrap(); // 201 was freed above
        assert_eq!(feed_page(&s, 11), Some(201));
        assert_eq!(assign(&s), 0, "a renumbered item is never renumbered again");
    }

    #[test]
    fn set_refuses_reserved_pages() {
        let s = numbered();
        for page in RESERVED_PAGES {
            let err = set(s.conn(), PageKind::Feed, 10, page).unwrap_err();
            assert!(
                matches!(err, Error::PageReserved(p) if p == page),
                "{err:?}"
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 800)
                .unwrap_err()
                .to_string(),
            "Page 800 is reserved."
        );
        assert_eq!(feed_page(&s, 10), Some(201));
    }

    #[test]
    fn set_refuses_pages_outside_110_to_799() {
        let s = numbered();
        for page in [0, 99, 900, u16::MAX] {
            let err = set(s.conn(), PageKind::Feed, 10, page).unwrap_err();
            assert!(
                matches!(err, Error::PageOutOfRange(p) if p == page),
                "{err:?}"
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 900)
                .unwrap_err()
                .to_string(),
            "Page 900 is not between 110 and 799."
        );
        assert_eq!(feed_page(&s, 10), Some(201));
    }

    #[test]
    fn set_refuses_a_page_taken_by_a_feed_or_a_folder() {
        let s = numbered();
        let taken = |kind, id, page| {
            let err = set(s.conn(), kind, id, page).unwrap_err();
            assert!(
                matches!(err, Error::PageTaken { page: p } if p == page),
                "{err:?}"
            );
            err.to_string()
        };
        assert_eq!(taken(PageKind::Feed, 10, 202), "Page 202 is already taken.");
        taken(PageKind::Feed, 10, 203); // a folder's page
        taken(PageKind::Folder, 2, 201); // a feed's page
        taken(PageKind::Folder, 2, 200); // another folder's page
        assert_eq!(feed_page(&s, 10), Some(201));
        assert_eq!(folder_page(&s, 2), Some(203));
    }

    #[test]
    fn set_refuses_an_unknown_id_and_the_reserved_kind() {
        let s = numbered();
        let err = set(s.conn(), PageKind::Feed, 99, 300).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownPageTarget {
                    kind: "feed",
                    id: 99
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "No feed #99.");
        let err = set(s.conn(), PageKind::Folder, 10, 300).unwrap_err();
        assert_eq!(err.to_string(), "No folder #10.", "ids are per table");
        assert!(matches!(
            set(s.conn(), PageKind::Reserved, 1, 300),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn journal_and_rhythm_pages_are_reserved_not_out_of_range() {
        let s = numbered();
        for page in 104..=109 {
            assert_eq!(
                set(s.conn(), PageKind::Feed, 10, page)
                    .unwrap_err()
                    .to_string(),
                format!("Page {page} is reserved.")
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 103)
                .unwrap_err()
                .to_string(),
            "Page 103 is reserved."
        );
        set(s.conn(), PageKind::Feed, 10, 110).unwrap();
        assert_eq!(
            feed_page(&s, 10),
            Some(110),
            "110 stays the first assignable page"
        );
    }
}
