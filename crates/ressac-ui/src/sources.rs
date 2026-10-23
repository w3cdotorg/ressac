//! The sources pane's logic (the Mac's `MainView` sidebar and `GardenStatusStyle`): the
//! selection and its keys, the flattened sidebar with its collapsible folders, badges, status
//! dots and Breeze icon names.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use ressac_core::Result;
use ressac_core::model::{CadenceGroup, FeedId, FolderId, Status};
use ressac_core::store::{FeedFacts, Source, SourceNode, SourceNodeKind, SourcesSnapshot, Store};

/// The sidebar's section titles, in order.
pub const HEADINGS: [&str; 4] = ["Smart Lists", "Feeds", "Rhythm", "Garden"];

/// The key prefix of a heading row (`heading:Feeds`); a heading selects nothing.
pub const HEADING_KEY_PREFIX: &str = "heading:";
/// The key prefix of a folder row (`folder:7`), shared with [`Selection::key`].
pub const FOLDER_KEY_PREFIX: &str = "folder:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GardenScope {
    NeedsTriage,
    All,
    Cemetery,
}

impl GardenScope {
    pub const ALL: [GardenScope; 3] = [
        GardenScope::NeedsTriage,
        GardenScope::All,
        GardenScope::Cemetery,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GardenScope::NeedsTriage => "Needs Triage",
            GardenScope::All => "All Feeds",
            GardenScope::Cemetery => "Cemetery",
        }
    }

    fn key(self) -> &'static str {
        match self {
            GardenScope::NeedsTriage => "garden:needs-triage",
            GardenScope::All => "garden:all",
            GardenScope::Cemetery => "garden:cemetery",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            GardenScope::NeedsTriage => "games-hint",
            GardenScope::All => "format-list-unordered",
            GardenScope::Cemetery => "box",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selection {
    Source(Source),
    Garden(GardenScope),
}

impl Default for Selection {
    fn default() -> Self {
        Selection::Source(Source::Unread)
    }
}

impl Selection {
    /// The stable text of a selection, used by the models and QML.
    pub fn key(&self) -> String {
        match self {
            Selection::Source(Source::Journal) => "journal".into(),
            Selection::Source(Source::Unread) => "unread".into(),
            Selection::Source(Source::Today) => "today".into(),
            Selection::Source(Source::Starred) => "starred".into(),
            Selection::Source(Source::Folder(id)) => format!("{FOLDER_KEY_PREFIX}{id}"),
            Selection::Source(Source::Feed(id)) => format!("feed:{id}"),
            Selection::Source(Source::Cadence(group)) => format!("rhythm:{}", group.as_str()),
            Selection::Garden(scope) => scope.key().to_string(),
        }
    }

    pub fn parse_key(key: &str) -> Option<Selection> {
        let source = |s| Some(Selection::Source(s));
        match key {
            "journal" => return source(Source::Journal),
            "unread" => return source(Source::Unread),
            "today" => return source(Source::Today),
            "starred" => return source(Source::Starred),
            _ => {}
        }
        if let Some(scope) = GardenScope::ALL.into_iter().find(|s| s.key() == key) {
            return Some(Selection::Garden(scope));
        }
        let (kind, rest) = key.split_once(':')?;
        match kind {
            "folder" => source(Source::Folder(rest.parse().ok()?)),
            "feed" => source(Source::Feed(rest.parse().ok()?)),
            "rhythm" => source(Source::Cadence(CadenceGroup::parse(rest)?)),
            _ => None,
        }
    }

    pub fn source(&self) -> Option<Source> {
        match self {
            Selection::Source(source) => Some(*source),
            Selection::Garden(_) => None,
        }
    }

    pub fn is_garden(&self) -> bool {
        matches!(self, Selection::Garden(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Heading,
    Smart,
    Folder,
    Feed,
    Rhythm,
    Garden,
}

impl RowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RowKind::Heading => "heading",
            RowKind::Smart => "smart",
            RowKind::Folder => "folder",
            RowKind::Feed => "feed",
            RowKind::Rhythm => "rhythm",
            RowKind::Garden => "garden",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SidebarRow {
    pub kind: RowKind,
    pub key: String,
    pub title: String,
    /// A Breeze icon name; `""` for headings and feeds.
    pub icon: &'static str,
    pub badge: usize,
    pub depth: usize,
    pub expandable: bool,
    pub expanded: bool,
    pub status: Option<Status>,
    pub prefer_full_text: bool,
    pub site: Option<String>,
    pub page: Option<u16>,
}

impl SidebarRow {
    fn new(kind: RowKind, key: String, title: &str, icon: &'static str, badge: usize) -> Self {
        SidebarRow {
            kind,
            key,
            title: title.to_string(),
            icon,
            badge,
            depth: 0,
            expandable: false,
            expanded: false,
            status: None,
            prefer_full_text: false,
            site: None,
            page: None,
        }
    }

    fn heading(title: &str) -> Self {
        Self::new(
            RowKind::Heading,
            format!("{HEADING_KEY_PREFIX}{title}"),
            title,
            "",
            0,
        )
    }

    /// What the row selects; headings select nothing.
    pub fn selection(&self) -> Option<Selection> {
        match self.kind {
            RowKind::Heading => None,
            _ => Selection::parse_key(&self.key),
        }
    }

    /// The colour of a feed's sidebar dot (sick statuses only).
    pub fn dot(&self) -> Option<&'static str> {
        match self.kind {
            RowKind::Feed => sidebar_dot(self.status),
            _ => None,
        }
    }
}

pub fn rhythm_title(group: CadenceGroup) -> &'static str {
    match group {
        CadenceGroup::Daily => "Daily",
        CadenceGroup::Weekly => "Weekly",
        CadenceGroup::Monthly => "Monthly",
        CadenceGroup::Rare => "Rare",
        CadenceGroup::Silent => "Silent",
    }
}

fn rhythm_icon(group: CadenceGroup) -> &'static str {
    match group {
        CadenceGroup::Daily => "flash",
        CadenceGroup::Weekly => "view-calendar-week",
        CadenceGroup::Monthly => "weather-clear-night",
        CadenceGroup::Rare => "star-shape",
        CadenceGroup::Silent => "system-suspend-symbolic",
    }
}

/// The one status to colour map (the garden reuses it); `""` when there is no status.
pub fn status_color(status: Option<Status>) -> &'static str {
    match status {
        Some(Status::Dead) => "red",
        Some(Status::Hijacked) => "purple",
        Some(Status::Moved) => "blue",
        Some(Status::Ailing) => "orange",
        Some(Status::Duplicate) => "teal",
        Some(Status::Fossil) => "brown",
        Some(Status::Dormant) => "indigo",
        Some(Status::Alive) => "green",
        Some(Status::Unchecked) | None => "",
    }
}

/// The sidebar shows a dot for sick feeds only.
pub fn sidebar_dot(status: Option<Status>) -> Option<&'static str> {
    match status {
        Some(
            Status::Ailing | Status::Dead | Status::Moved | Status::Hijacked | Status::Duplicate,
        ) => Some(status_color(status)),
        _ => None,
    }
}

pub fn index_of(rows: &[SidebarRow], selection: &Selection) -> Option<usize> {
    let key = selection.key();
    rows.iter().position(|row| row.key == key)
}

/// The sidebar's data and which folders are open.
#[derive(Debug, Clone, Default)]
pub struct SidebarTree {
    snapshot: Option<SourcesSnapshot>,
    facts: HashMap<FeedId, FeedFacts>,
    expanded: HashSet<FolderId>,
}

impl SidebarTree {
    pub fn load(&mut self, store: &Store, today_start: DateTime<Utc>) -> Result<()> {
        let snapshot = store.sources(today_start)?;
        let facts = store.feed_facts()?;
        self.set(snapshot, facts);
        Ok(())
    }

    pub fn set(&mut self, snapshot: SourcesSnapshot, facts: HashMap<FeedId, FeedFacts>) {
        self.snapshot = Some(snapshot);
        self.facts = facts;
    }

    /// The Feeds section's tree (empty until a snapshot is loaded).
    pub fn nodes(&self) -> &[SourceNode] {
        self.snapshot.as_ref().map_or(&[], |s| s.tree.as_slice())
    }

    /// The first-run screen's predicate: a snapshot is loaded and no active feed is in it.
    /// Never true before the first load, so the screen does not flash at launch.
    pub fn is_empty_library(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| !has_feed(&s.tree))
    }

    /// The folder above a feed in the sidebar: `Some(None)` at the top level, `None` when
    /// the feed is not in the sidebar (buried, unknown).
    pub fn folder_of(&self, feed: FeedId) -> Option<Option<FolderId>> {
        folder_above(self.nodes(), None, feed)
    }

    pub fn rows(&self) -> Vec<SidebarRow> {
        let empty = SourcesSnapshot {
            counts: Default::default(),
            tree: Vec::new(),
        };
        let snapshot = self.snapshot.as_ref().unwrap_or(&empty);
        let counts = &snapshot.counts;
        let mut rows = vec![SidebarRow::heading(HEADINGS[0])];
        for (key, title, icon, badge) in [
            ("journal", "Journal", "view-pim-news", counts.journal),
            ("unread", "Unread", "mail-unread", counts.unread),
            ("today", "Today", "brightness-high", counts.today),
            ("starred", "Starred", "starred-symbolic", counts.starred),
        ] {
            rows.push(SidebarRow::new(
                RowKind::Smart,
                key.into(),
                title,
                icon,
                badge,
            ));
        }
        rows.push(SidebarRow::heading(HEADINGS[1]));
        self.push_nodes(&mut rows, &snapshot.tree, 0);
        rows.push(SidebarRow::heading(HEADINGS[2]));
        for group in CadenceGroup::ALL {
            let badge = counts.cadence.get(&group).copied().unwrap_or(0);
            let key = Selection::Source(Source::Cadence(group)).key();
            rows.push(SidebarRow::new(
                RowKind::Rhythm,
                key,
                rhythm_title(group),
                rhythm_icon(group),
                badge,
            ));
        }
        rows.push(SidebarRow::heading(HEADINGS[3]));
        for scope in GardenScope::ALL {
            let badge = if scope == GardenScope::NeedsTriage {
                counts.needs_triage
            } else {
                0
            };
            rows.push(SidebarRow::new(
                RowKind::Garden,
                scope.key().into(),
                scope.title(),
                scope.icon(),
                badge,
            ));
        }
        rows
    }

    fn push_nodes(&self, rows: &mut Vec<SidebarRow>, nodes: &[SourceNode], depth: usize) {
        for node in nodes {
            match &node.kind {
                SourceNodeKind::Folder { id, name } => {
                    let key = Selection::Source(Source::Folder(*id)).key();
                    let mut row =
                        SidebarRow::new(RowKind::Folder, key, name, "folder", node.unread);
                    row.depth = depth;
                    row.page = node.page;
                    row.expandable = !node.children.is_empty();
                    row.expanded = self.expanded.contains(id);
                    let open = row.expanded;
                    rows.push(row);
                    if open {
                        self.push_nodes(rows, &node.children, depth + 1);
                    }
                }
                SourceNodeKind::Feed { id, title } => {
                    let key = Selection::Source(Source::Feed(*id)).key();
                    let mut row = SidebarRow::new(RowKind::Feed, key, title, "", node.unread);
                    row.depth = depth;
                    row.page = node.page;
                    if let Some(facts) = self.facts.get(id) {
                        row.status = facts.status;
                        row.prefer_full_text = facts.prefer_full_text;
                        row.site = facts.html_url.clone();
                    }
                    rows.push(row);
                }
            }
        }
    }

    /// Opens or closes a folder; returns the new state.
    pub fn toggle(&mut self, folder: FolderId) -> bool {
        if self.expanded.remove(&folder) {
            false
        } else {
            self.expanded.insert(folder);
            true
        }
    }

    /// Opens every folder above a feed or folder selection.
    pub fn reveal(&mut self, selection: &Selection) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let target = match selection {
            Selection::Source(Source::Feed(id)) => (false, *id),
            Selection::Source(Source::Folder(id)) => (true, *id),
            _ => return,
        };
        let mut path = Vec::new();
        if find_path(&snapshot.tree, target, &mut path) {
            self.expanded.extend(path);
        }
    }

    /// Sets a feed's full-text preference in the loaded facts; returns the previous value.
    pub fn set_prefer_full_text_local(&mut self, feed: FeedId, on: bool) -> bool {
        match self.facts.get_mut(&feed) {
            Some(facts) => std::mem::replace(&mut facts.prefer_full_text, on),
            None => false,
        }
    }
}

fn has_feed(nodes: &[SourceNode]) -> bool {
    nodes.iter().any(|node| match node.kind {
        SourceNodeKind::Feed { .. } => true,
        SourceNodeKind::Folder { .. } => has_feed(&node.children),
    })
}

fn folder_above(
    nodes: &[SourceNode],
    folder: Option<FolderId>,
    feed: FeedId,
) -> Option<Option<FolderId>> {
    nodes.iter().find_map(|node| match &node.kind {
        SourceNodeKind::Feed { id, .. } => (*id == feed).then_some(folder),
        SourceNodeKind::Folder { id, .. } => folder_above(&node.children, Some(*id), feed),
    })
}

/// Collects the folders above `target` (`(is_folder, id)`); true when it was found.
fn find_path(nodes: &[SourceNode], target: (bool, i64), path: &mut Vec<FolderId>) -> bool {
    for node in nodes {
        match &node.kind {
            SourceNodeKind::Folder { id, .. } => {
                if target == (true, *id) {
                    return true;
                }
                path.push(*id);
                if find_path(&node.children, target, path) {
                    return true;
                }
                path.pop();
            }
            SourceNodeKind::Feed { id, .. } => {
                if target == (false, *id) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::*;
    fn loaded(db: &TestDb) -> SidebarTree {
        let mut tree = SidebarTree::default();
        tree.load(&db.store(), today()).unwrap();
        tree
    }

    fn titles(rows: &[SidebarRow]) -> Vec<&str> {
        rows.iter().map(|r| r.title.as_str()).collect()
    }

    fn row<'a>(rows: &'a [SidebarRow], key: &str) -> &'a SidebarRow {
        rows.iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("no row {key}"))
    }

    #[test]
    fn the_feed_s_folder_comes_from_the_sidebar_tree() {
        let db = TestDb::new();
        db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
        let store = db.store();
        let mut tree = SidebarTree::default();
        assert!(
            !tree.is_empty_library(),
            "not loaded yet: never the first-run screen"
        );
        assert!(tree.nodes().is_empty());
        tree.load(&store, today()).unwrap();
        assert!(!tree.is_empty_library());
        assert_eq!(
            tree.folder_of(2),
            Some(Some(2)),
            "Orange Drenka sits in potes"
        );
        assert_eq!(tree.folder_of(1), Some(Some(1)));
        assert_eq!(
            tree.folder_of(4),
            None,
            "a buried feed is not in the sidebar"
        );
        assert_eq!(tree.folder_of(999), None);
    }

    #[test]
    fn a_library_without_active_feeds_is_empty() {
        let db = TestDb::new();
        db.execute("UPDATE feed SET state = 'buried'");
        let store = db.store();
        let mut tree = SidebarTree::default();
        tree.load(&store, today()).unwrap();
        assert!(tree.is_empty_library());
    }

    #[test]
    fn row_keys_use_the_shared_prefixes() {
        assert_eq!(
            SidebarRow::heading(HEADINGS[1]).key,
            format!("{HEADING_KEY_PREFIX}{}", HEADINGS[1])
        );
        assert_eq!(
            Selection::Source(Source::Folder(7)).key(),
            format!("{FOLDER_KEY_PREFIX}7")
        );
    }

    #[test]
    fn selection_keys_round_trip() {
        let all = [
            Selection::Source(Source::Journal),
            Selection::Source(Source::Unread),
            Selection::Source(Source::Today),
            Selection::Source(Source::Starred),
            Selection::Source(Source::Folder(7)),
            Selection::Source(Source::Feed(12)),
            Selection::Source(Source::Cadence(CadenceGroup::Weekly)),
            Selection::Garden(GardenScope::NeedsTriage),
            Selection::Garden(GardenScope::All),
            Selection::Garden(GardenScope::Cemetery),
        ];
        assert_eq!(
            all.map(|s| s.key()),
            [
                "journal",
                "unread",
                "today",
                "starred",
                "folder:7",
                "feed:12",
                "rhythm:weekly",
                "garden:needs-triage",
                "garden:all",
                "garden:cemetery"
            ]
        );
        for s in all {
            assert_eq!(Selection::parse_key(&s.key()), Some(s));
        }
        for bad in [
            "",
            "feed:",
            "feed:x",
            "folder:-1x",
            "rhythm:hourly",
            "garden:compost",
            "heading:Feeds",
            "Unread",
        ] {
            assert_eq!(Selection::parse_key(bad), None, "{bad}");
        }
        assert_eq!(Selection::default(), Selection::Source(Source::Unread));
        assert!(
            Selection::Garden(GardenScope::All).is_garden()
                && Selection::Garden(GardenScope::All).source().is_none()
        );
        assert_eq!(
            GardenScope::ALL.map(GardenScope::title),
            ["Needs Triage", "All Feeds", "Cemetery"]
        );
    }

    #[test]
    fn the_sidebar_lists_the_mac_sections_with_folders_closed() {
        let db = TestDb::new();
        let rows = loaded(&db).rows();
        assert_eq!(
            titles(&rows),
            [
                "Smart Lists",
                "Journal",
                "Unread",
                "Today",
                "Starred",
                "Feeds",
                "Blogs",
                "Biblis",
                "Infos",
                "Jon",
                "Rhythm",
                "Daily",
                "Weekly",
                "Monthly",
                "Rare",
                "Silent",
                "Garden",
                "Needs Triage",
                "All Feeds",
                "Cemetery"
            ]
        );
        let icons: Vec<&str> = rows
            .iter()
            .filter(|r| r.kind != RowKind::Heading)
            .map(|r| r.icon)
            .collect();
        assert_eq!(
            icons,
            [
                "view-pim-news",
                "mail-unread",
                "brightness-high",
                "starred-symbolic",
                "folder",
                "folder",
                "folder",
                "folder",
                "flash",
                "view-calendar-week",
                "weather-clear-night",
                "star-shape",
                "system-suspend-symbolic",
                "games-hint",
                "format-list-unordered",
                "box"
            ]
        );
        let blogs = row(&rows, "folder:1");
        assert_eq!(
            (blogs.kind, blogs.depth, blogs.expandable, blogs.expanded),
            (RowKind::Folder, 0, true, false)
        );
        assert_eq!(HEADINGS, ["Smart Lists", "Feeds", "Rhythm", "Garden"]);
        assert_eq!(row(&rows, "heading:Feeds").selection(), None);
        assert_eq!(index_of(&rows, &Selection::default()), Some(2));
    }

    #[test]
    fn opening_a_folder_shows_its_children_one_level_deeper() {
        let db = TestDb::new();
        let mut tree = loaded(&db);
        assert!(tree.toggle(1));
        let rows = tree.rows();
        let at = index_of(&rows, &Selection::Source(Source::Folder(1))).unwrap();
        assert_eq!(
            titles(&rows[at + 1..at + 4]),
            ["xkcd.com", "potes", "Girl on the Couch"]
        );
        assert!(rows[at + 1..at + 4].iter().all(|r| r.depth == 1));
        assert!(!tree.toggle(1));
        tree.reveal(&Selection::Source(Source::Feed(11)));
        let rows = tree.rows();
        assert_eq!(
            row(&rows, "feed:11").depth,
            2,
            "Jon > YouTube > Center for Humane Technology (1)"
        );
    }

    #[test]
    fn badges_follow_the_counts_and_folders_sum_their_subtree() {
        let db = TestDb::new();
        db.entry(2, "2026-10-16T08:00:00.000Z", false); // potes, inside Blogs; today
        db.entry_with(NewEntry {
            feed: 2,
            read: true,
            starred: true,
            ..Default::default()
        });
        db.entry(8, "2026-10-01T08:00:00.000Z", false); // Infos
        db.health(8, "Alive", "weekly", false);
        db.health(3, "Dead", "daily", true);
        let rows = loaded(&db).rows();
        let badge = |key| row(&rows, key).badge;
        assert_eq!(
            ["journal", "unread", "today", "starred"].map(badge),
            [0, 2, 1, 1]
        );
        assert_eq!(["folder:1", "folder:4", "folder:3"].map(badge), [1, 1, 0]);
        assert_eq!(
            ["rhythm:daily", "rhythm:weekly", "rhythm:silent"].map(badge),
            [0, 1, 0]
        );
        assert_eq!(
            ["garden:needs-triage", "garden:all", "garden:cemetery"].map(badge),
            [1, 0, 0]
        );
    }

    #[test]
    fn sick_feeds_get_a_coloured_dot_and_the_others_none() {
        assert_eq!(
            Status::ALL.map(|s| sidebar_dot(Some(s))),
            [
                Some("teal"),
                Some("purple"),
                Some("blue"),
                Some("red"),
                Some("orange"),
                None,
                None,
                None,
                None
            ]
        );
        assert_eq!(
            Status::ALL.map(|s| status_color(Some(s))),
            [
                "teal", "purple", "blue", "red", "orange", "brown", "indigo", "green", ""
            ]
        );
        assert_eq!((status_color(None), sidebar_dot(None)), ("", None));
        let db = TestDb::new();
        db.health(1, "Ailing", "daily", false);
        db.execute(
            "UPDATE feed SET prefer_full_text = 1, html_url = 'https://xkcd.com/' WHERE id = 1",
        );
        let mut tree = loaded(&db);
        tree.toggle(1);
        let rows = tree.rows();
        let xkcd = row(&rows, "feed:1");
        assert_eq!(
            (
                xkcd.dot(),
                xkcd.status.map(Status::as_str),
                xkcd.prefer_full_text,
                xkcd.site.as_deref()
            ),
            (
                Some("orange"),
                Some("Ailing"),
                true,
                Some("https://xkcd.com/")
            )
        );
        assert!(tree.set_prefer_full_text_local(1, false));
        assert!(!row(&tree.rows(), "feed:1").prefer_full_text);
    }

    /// Mac FixWaveTests.todayCountsEveryEntryOfTheDayAndRestartsAtMidnight.
    #[test]
    fn today_counts_every_entry_of_the_day_and_restarts_at_midnight() {
        let db = TestDb::new();
        db.entry(1, "2026-10-10T08:00:00.000Z", true);
        db.entry(1, "2026-10-10T08:30:00.000Z", false);
        db.entry(1, "2026-10-08T08:30:00.000Z", false);
        let store = db.store();
        let mut tree = SidebarTree::default();
        tree.load(&store, at("2026-10-10T00:00:00Z")).unwrap();
        assert_eq!(row(&tree.rows(), "today").badge, 2, "read and unread");
        tree.load(&store, at("2026-10-11T00:00:00Z")).unwrap();
        assert_eq!(row(&tree.rows(), "today").badge, 0);
    }
}
