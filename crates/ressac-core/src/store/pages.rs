//! Teletext page numbers (schema v3). The core gives every folder and feed a number once
//! and never changes it on its own; the apps read them through `v_page`.

use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::Result;

/// Pages fixed in code and never stored: Index, Unread, Today, Starred, the three garden
/// pages and the Guide.
pub const RESERVED_PAGES: [u16; 8] = [100, 101, 102, 103, 800, 801, 802, 899];
/// The pages a folder or a feed may hold.
pub const ASSIGNABLE: RangeInclusive<u16> = 110..=799;

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

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

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
}
