//! The entries pane's logic (the Mac's `AppState` list rules and `EntryRow`): the list loaded
//! down to a floor, rows retained while read, the filter and search, the next-turn page
//! scheduling, the row texts and the optimistic local edits. Nothing here writes to the
//! database; the caller does, then settles or reverts the local edit.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use ressac_core::model::{FeedId, parse_ts};
use ressac_core::store::{ListCursor, ListFilter, ListQuery, ListedEntry, Source, Store};

/// Rows per page of a long list.
pub const PAGE_SIZE: usize = ressac_core::store::PAGE_SIZE;
/// A page is scheduled when a row within this many of the end appears.
pub const LOAD_MORE_MARGIN: usize = 20;
pub const EXCERPT_CHARS: usize = 280;
pub const UNTITLED: &str = "Untitled";
pub const NO_RESULTS: &str = "No results";
pub const NO_UNREAD: &str = "No unread entries";
pub const NO_ENTRIES: &str = "No entries";

/// One row of the entries pane.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub title: String,
    pub excerpt: String,
    pub published_at: DateTime<Utc>,
    pub is_read: bool,
    pub is_starred: bool,
    pub has_fulltext: bool,
    pub link: Option<String>,
    pub rank: Option<u32>,
}

impl ListRow {
    pub fn new(listed: &ListedEntry) -> ListRow {
        let row = &listed.row;
        ListRow {
            entry_id: row.entry_id,
            feed_id: row.feed_id,
            feed_title: row.feed_title.clone(),
            title: display_title(row.title.as_deref()),
            excerpt: excerpt(listed.snippet.as_deref().unwrap_or(""), EXCERPT_CHARS),
            published_at: row.published_at,
            is_read: row.is_read,
            is_starred: row.is_starred,
            has_fulltext: row.has_fulltext,
            link: web_link(row.url.as_deref()),
            rank: listed.rank,
        }
    }
}

/// The trimmed title, or "Untitled".
pub fn display_title(title: Option<&str>) -> String {
    match title.map(str::trim) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => UNTITLED.to_string(),
    }
}

/// The text of an HTML body for a list row (the Mac's `HTMLText.plain`): tags become spaces,
/// the common entities are decoded (`&amp;` last), whitespace collapses, and more than
/// `limit` characters are cut with an ellipsis.
pub fn excerpt(html: &str, limit: usize) -> String {
    let mut stripped = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        stripped.push_str(&rest[..open]);
        stripped.push(' ');
        match rest[open..].find('>') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    stripped.push_str(rest);
    let decoded = [
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&amp;", "&"),
    ]
    .iter()
    .fold(stripped, |text, (entity, plain)| {
        text.replace(entity, plain)
    });
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &collapsed[..cut]),
        None => collapsed,
    }
}

/// `url` when it is a web address (`sanitize::is_web_url`, the reader's link rule).
pub fn web_link(url: Option<&str>) -> Option<String> {
    url.filter(|url| crate::sanitize::is_web_url(url))
        .map(str::to_string)
}

/// "3 hours ago", "yesterday", "in 2 days": the largest whole unit, with fixed spans
/// (month 30 days, year 365).
pub fn relative_date(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - at).num_seconds();
    let (n, past) = (secs.unsigned_abs(), secs >= 0);
    // largest unit first; the one-unit name when it has one
    /// Seconds, unit, and the names of one such unit in the past and the future.
    type Unit = (u64, &'static str, Option<(&'static str, &'static str)>);
    const UNITS: [Unit; 7] = [
        (365 * 86_400, "year", Some(("last year", "next year"))),
        (30 * 86_400, "month", Some(("last month", "next month"))),
        (7 * 86_400, "week", Some(("last week", "next week"))),
        (86_400, "day", Some(("yesterday", "tomorrow"))),
        (3_600, "hour", None),
        (60, "minute", None),
        (1, "second", None),
    ];
    for (span, unit, named) in UNITS {
        let k = n / span;
        if k == 0 {
            continue;
        }
        if let (1, Some((last, next))) = (k, named) {
            return if past { last } else { next }.into();
        }
        let plural = if k == 1 { "" } else { "s" };
        return if past {
            format!("{k} {unit}{plural} ago")
        } else {
            format!("in {k} {unit}{plural}")
        };
    }
    "now".into()
}

#[derive(Debug, Clone, PartialEq)]
enum Change {
    Read { ids: Vec<i64>, value: bool },
    Starred { id: i64, value: bool },
}

/// An optimistic change in flight: it stays applied over every reload until `settle` (the
/// write landed) or `revert` (it failed), so a poll during a 5 s busy wait never flickers the
/// row back.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalEdit {
    token: u64,
    change: Change,
    /// The flag of each row the edit changed, as it was.
    previous: Vec<(i64, bool)>,
}

/// The entries pane's list: its rows, counts and paging.
#[derive(Debug)]
pub struct EntryList {
    query: ListQuery,
    page_size: usize,
    rows: Vec<ListRow>,
    total: usize,
    unread: usize,
    has_more: bool,
    floor: Option<ListCursor>,
    floor_failed: bool,
    generation: u64,
    /// Rows read in this visit stay listed (and in place) when the list is unread-only.
    retained: HashSet<i64>,
    page_scheduled: bool,
    edits: Vec<LocalEdit>,
    next_token: u64,
}

impl EntryList {
    /// Empty, generation 0, pages of [`PAGE_SIZE`].
    pub fn new(query: ListQuery) -> EntryList {
        EntryList {
            query,
            page_size: PAGE_SIZE,
            rows: Vec::new(),
            total: 0,
            unread: 0,
            has_more: false,
            floor: None,
            floor_failed: false,
            generation: 0,
            retained: HashSet::new(),
            page_scheduled: false,
            edits: Vec::new(),
            next_token: 0,
        }
    }

    /// Pages of `rows` rows (tests).
    pub fn with_page_size(mut self, rows: usize) -> EntryList {
        self.page_size = rows;
        self
    }

    pub fn query(&self) -> &ListQuery {
        &self.query
    }

    pub fn rows(&self) -> &[ListRow] {
        &self.rows
    }

    pub fn index_of(&self, entry_id: i64) -> Option<usize> {
        self.rows.iter().position(|r| r.entry_id == entry_id)
    }

    /// Rows of the whole list, loaded or not.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Unread rows of the whole list, loaded or not.
    pub fn unread(&self) -> usize {
        self.unread
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }

    /// Bumped by every restart: a scheduled answer for an older one is dropped.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The list starts over: a new floor one page down (lowered to keep `keeping` loaded),
    /// then the rows. When the floor cannot be read the whole list loads and the error is
    /// returned.
    pub fn restart(
        &mut self,
        store: &Store,
        keeping: Option<i64>,
        today_start: DateTime<Utc>,
    ) -> ressac_core::Result<()> {
        self.generation += 1;
        let failure =
            match store.list_floor(&self.query, None, self.page_size, keeping, today_start) {
                Ok(floor) => {
                    self.floor = floor;
                    self.floor_failed = false;
                    None
                }
                Err(error) => {
                    self.floor = None;
                    self.floor_failed = true;
                    Some(error)
                }
            };
        let reloaded = self.reload(store, today_start);
        match failure {
            Some(error) => Err(error),
            None => reloaded.map(|_| ()),
        }
    }

    /// A new query; a different source forgets the rows retained by the last visit.
    pub fn change(
        &mut self,
        query: ListQuery,
        store: &Store,
        keeping: Option<i64>,
        today_start: DateTime<Utc>,
    ) -> ressac_core::Result<()> {
        if query.source != self.query.source {
            self.retained.clear();
        }
        self.query = query;
        self.restart(store, keeping, today_start)
    }

    /// Reads the rows again down to the floor (the observation's refetch). `true` when a
    /// restart is due on the next turn: a whole list grew past a page.
    pub fn reload(
        &mut self,
        store: &Store,
        today_start: DateTime<Utc>,
    ) -> ressac_core::Result<bool> {
        let page = store.list_rows(&self.query, self.floor.as_ref(), today_start)?;
        let fresh: Vec<ListRow> = page.rows.iter().map(ListRow::new).collect();
        let fresh_count = fresh.len();
        let fresh_ids: HashSet<i64> = fresh.iter().map(|r| r.entry_id).collect();
        let kept: Vec<ListRow> = self
            .rows
            .iter()
            .filter(|r| {
                self.retained.contains(&r.entry_id)
                    && !fresh_ids.contains(&r.entry_id)
                    && self.admits(r)
            })
            .cloned()
            .map(|mut r| {
                r.is_read = true;
                r
            })
            .collect();
        let merged_count = kept.len();
        let mut rows = fresh;
        rows.extend(kept);
        if self.query.source == Source::Journal {
            rows.sort_by_key(|r| (r.rank.unwrap_or(u32::MAX), r.entry_id));
        } else {
            rows.sort_by_key(|r| std::cmp::Reverse((r.published_at, r.entry_id)));
        }
        self.rows = rows;
        self.total = page.total + merged_count;
        self.unread = page.unread;
        self.has_more = fresh_count < page.total;
        self.apply_edits();
        Ok(self.restart_wanted())
    }

    fn admits(&self, row: &ListRow) -> bool {
        match self.floor.as_ref() {
            None => true,
            Some(floor) => match parse_ts(&floor.published_at) {
                None => true,
                Some(published) => (row.published_at, row.entry_id) >= (published, floor.entry_id),
            },
        }
    }

    fn restart_wanted(&self) -> bool {
        self.floor.is_none()
            && !self.floor_failed
            && self.query.source != Source::Journal
            && self.rows.len() > self.page_size
    }

    /// The restart [`reload`](Self::reload) asked for is still wanted by the list it was for.
    pub fn restart_due(&self, generation: u64) -> bool {
        generation == self.generation && self.restart_wanted()
    }

    /// One more page: the floor moves one page down (to the oldest possible when the rest
    /// fits), then the rows.
    pub fn load_more(
        &mut self,
        store: &Store,
        today_start: DateTime<Utc>,
    ) -> ressac_core::Result<()> {
        let Some(floor) = self.floor.clone().filter(|_| self.has_more) else {
            return Ok(());
        };
        let next =
            store.list_floor(&self.query, Some(&floor), self.page_size, None, today_start)?;
        self.floor = Some(next.unwrap_or(ListCursor {
            published_at: String::new(),
            entry_id: i64::MIN,
        }));
        self.reload(store, today_start).map(|_| ())
    }

    /// A row appeared on screen: `Some(generation)` when it is near the end and no page is
    /// scheduled yet, meaning "load a page on the next turn".
    pub fn row_appeared(&mut self, entry_id: i64) -> Option<u64> {
        if self.page_scheduled || !self.has_more {
            return None;
        }
        let first_near_end = self.rows.len().saturating_sub(LOAD_MORE_MARGIN);
        if self.index_of(entry_id)? < first_near_end {
            return None;
        }
        self.page_scheduled = true;
        Some(self.generation)
    }

    /// The scheduled turn came: the mark clears; `true` when the list is still the same.
    pub fn scheduled_page_due(&mut self, generation: u64) -> bool {
        self.page_scheduled = false;
        generation == self.generation
    }

    pub fn can_mark_all_read(&self) -> bool {
        self.query.search.is_none()
            && (self.rows.iter().any(|r| !r.is_read) || (self.has_more && self.unread > 0))
    }

    /// Why the list is empty, `None` with rows.
    pub fn empty_text(&self) -> Option<&'static str> {
        if !self.rows.is_empty() {
            None
        } else if self.query.search.is_some() {
            Some(NO_RESULTS)
        } else if self.query.source == Source::Unread || self.query.filter == ListFilter::Unread {
            Some(NO_UNREAD)
        } else {
            Some(NO_ENTRIES)
        }
    }

    fn start(&mut self, change: Change) -> LocalEdit {
        let mut previous = Vec::new();
        let (ids, flag): (Vec<i64>, bool) = match &change {
            Change::Read { ids, value } => (ids.clone(), *value),
            Change::Starred { id, value } => (vec![*id], *value),
        };
        for row in self.rows.iter_mut().filter(|r| ids.contains(&r.entry_id)) {
            let slot = match change {
                Change::Read { .. } => &mut row.is_read,
                Change::Starred { .. } => &mut row.is_starred,
            };
            if *slot != flag {
                previous.push((row.entry_id, *slot));
                *slot = flag;
            }
        }
        self.next_token += 1;
        let edit = LocalEdit {
            token: self.next_token,
            change,
            previous,
        };
        self.edits.push(edit.clone());
        edit
    }

    /// Shows `ids` read (or unread) at once. Reading also retains them in the list.
    pub fn set_read_local(&mut self, ids: &[i64], read: bool) -> LocalEdit {
        if read {
            self.retained.extend(ids.iter().copied());
        }
        self.start(Change::Read {
            ids: ids.to_vec(),
            value: read,
        })
    }

    pub fn set_starred_local(&mut self, id: i64, starred: bool) -> LocalEdit {
        self.start(Change::Starred { id, value: starred })
    }

    /// Shows every loaded unread row read at once.
    pub fn mark_all_read_local(&mut self) -> LocalEdit {
        let ids: Vec<i64> = self
            .rows
            .iter()
            .filter(|r| !r.is_read)
            .map(|r| r.entry_id)
            .collect();
        self.set_read_local(&ids, true)
    }

    fn apply_edits(&mut self) {
        for edit in &self.edits {
            let (ids, flag, read): (Vec<i64>, bool, bool) = match &edit.change {
                Change::Read { ids, value } => (ids.clone(), *value, true),
                Change::Starred { id, value } => (vec![*id], *value, false),
            };
            for row in self.rows.iter_mut().filter(|r| ids.contains(&r.entry_id)) {
                if read {
                    row.is_read = flag;
                } else {
                    row.is_starred = flag;
                }
            }
        }
    }

    /// The write landed: the overlay goes, the database agrees.
    pub fn settle(&mut self, edit: LocalEdit) {
        self.edits.retain(|e| e.token != edit.token);
    }

    /// The write failed: the flags the edit changed come back (a retained row stays
    /// retained, as on the Mac).
    pub fn revert(&mut self, edit: LocalEdit) {
        self.edits.retain(|e| e.token != edit.token);
        for (id, before) in &edit.previous {
            if let Some(row) = self.rows.iter_mut().find(|r| r.entry_id == *id) {
                match edit.change {
                    Change::Read { .. } => row.is_read = *before,
                    Change::Starred { .. } => row.is_starred = *before,
                }
            }
        }
        self.apply_edits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::*;

    fn q(source: Source) -> ListQuery {
        ListQuery {
            source,
            filter: ListFilter::All,
            search: None,
        }
    }

    fn row_ids(l: &EntryList) -> Vec<i64> {
        l.rows().iter().map(|r| r.entry_id).collect()
    }

    fn opened(db: &TestDb, query: ListQuery, page: usize) -> (Store, EntryList) {
        let store = db.store();
        let mut list = EntryList::new(query).with_page_size(page);
        list.restart(&store, None, today()).unwrap();
        (store, list)
    }

    #[test]
    fn untitled_entries_and_excerpts_read_like_the_mac_list() {
        assert_eq!(
            [
                display_title(None),
                display_title(Some(" \n ")),
                display_title(Some("  Tides "))
            ],
            ["Untitled", "Untitled", "Tides"]
        );
        assert_eq!(
            excerpt(
                "<p>a&nbsp;b &lt;tag&gt; &quot;q&quot; &#39;s&apos;</p>\n\n<div>end</div>",
                280
            ),
            "a b <tag> \"q\" 's' end"
        );
        assert_eq!(
            excerpt("<p>Caf&eacute; &amp; cr&egrave;me</p>", 280),
            "Caf&eacute; & cr&egrave;me"
        );
        assert_eq!(excerpt("&amp;lt;", 280), "&lt;", "&amp; is decoded last");
        assert_eq!(excerpt("before <a href=\"x\" never closed", 280), "before");
        assert_eq!(
            excerpt(&"é".repeat(300), 280),
            format!("{}…", "é".repeat(280))
        );
        assert_eq!(excerpt(&"é".repeat(280), 280), "é".repeat(280));
    }

    #[test]
    fn only_web_links_are_links() {
        assert_eq!(
            web_link(Some("https://e.org/a")).as_deref(),
            Some("https://e.org/a")
        );
        assert_eq!(
            web_link(Some("HTTP://e.org/a")).as_deref(),
            Some("HTTP://e.org/a")
        );
        for bad in [
            Some("javascript:alert(1)"),
            Some("mailto:a@b.org"),
            Some("https://"),
            Some(""),
            None,
        ] {
            assert_eq!(web_link(bad), None, "{bad:?}");
        }
        assert_eq!(web_link(Some("https:///nohost")), None);
    }

    #[test]
    fn relative_dates_name_the_largest_unit_like_the_mac_list() {
        let now = at("2026-10-16T12:00:00Z");
        let ago = |s: i64| relative_date(now - chrono::Duration::seconds(s), now);
        assert_eq!(
            [ago(0), ago(1), ago(45), ago(60), ago(7_200)],
            [
                "now",
                "1 second ago",
                "45 seconds ago",
                "1 minute ago",
                "2 hours ago"
            ]
        );
        assert_eq!(
            [
                ago(86_400),
                ago(3 * 86_400),
                ago(7 * 86_400),
                ago(15 * 86_400)
            ],
            ["yesterday", "3 days ago", "last week", "2 weeks ago"]
        );
        assert_eq!(
            [
                ago(31 * 86_400),
                ago(90 * 86_400),
                ago(400 * 86_400),
                ago(800 * 86_400)
            ],
            ["last month", "3 months ago", "last year", "2 years ago"]
        );
        assert_eq!([ago(-3 * 3_600), ago(-86_400)], ["in 3 hours", "tomorrow"]);
    }

    /// Mac DataTests.entries_follow_source_and_unread_filter.
    #[test]
    fn source_and_filter_drive_the_list() {
        let db = TestDb::new();
        let a = db.entry(1, "2026-10-01T08:00:00.000Z", false);
        let b = db.entry_with(NewEntry {
            feed: 2,
            published: "2026-10-03T08:00:00.000Z",
            read: true,
            ..Default::default()
        });
        let c = db.entry_with(NewEntry {
            feed: 8,
            published: "2026-10-02T08:00:00.000Z",
            starred: true,
            ..Default::default()
        });
        let ids = |query| row_ids(&opened(&db, query, PAGE_SIZE).1);
        assert_eq!(ids(q(Source::Folder(1))), [b, a]);
        assert_eq!(
            ids(ListQuery {
                filter: ListFilter::Unread,
                ..q(Source::Folder(1))
            }),
            [a]
        );
        assert_eq!(
            [
                ids(q(Source::Unread)),
                ids(q(Source::Starred)),
                ids(q(Source::Feed(8)))
            ],
            [vec![c, a], vec![c], vec![c]]
        );
        let (_, l) = opened(&db, q(Source::Feed(1)), PAGE_SIZE);
        assert_eq!(
            (
                l.rows()[0].title.as_str(),
                l.rows()[0].excerpt.as_str(),
                l.rows()[0].link.as_deref()
            ),
            ("Entry", "Summary", Some("https://example.org/a"))
        );
        assert_eq!(l.rows()[0].feed_title, "xkcd.com");
    }

    #[test]
    fn a_long_list_loads_one_page_and_counts_the_whole_list() {
        let db = TestDb::new();
        let ids = db.days(5, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert_eq!(row_ids(&l), ids[..2]);
        assert_eq!((l.total(), l.unread(), l.has_more()), (5, 5, true));
        l.load_more(&store, today()).unwrap();
        assert_eq!(row_ids(&l), ids[..4]);
        l.load_more(&store, today()).unwrap();
        assert_eq!((row_ids(&l), l.has_more()), (ids, false));
    }

    #[test]
    fn rows_appearing_near_the_end_schedule_one_page() {
        let db = TestDb::new();
        let ids = db.days(26, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 24);
        assert_eq!(
            l.row_appeared(ids[3]),
            None,
            "21 rows from the end of 24: not near it"
        );
        let g = l.row_appeared(ids[4]).expect("20 rows from the end");
        assert_eq!(
            l.row_appeared(ids[5]),
            None,
            "one page per turn, however many rows appear"
        );
        assert!(l.scheduled_page_due(g));
        l.load_more(&store, today()).unwrap();
        assert_eq!((l.rows().len(), l.has_more()), (26, false));
        assert_eq!(l.row_appeared(ids[25]), None, "nothing left to load");
    }

    #[test]
    fn a_scheduled_page_is_dropped_when_the_list_changed() {
        let db = TestDb::new();
        let first = db.days(30, 1, |_| false);
        db.days(30, 8, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 24);
        let g = l.row_appeared(first[4]).unwrap();
        l.change(q(Source::Feed(8)), &store, None, today()).unwrap();
        assert!(!l.scheduled_page_due(g));
        assert_eq!((l.rows().len(), l.has_more()), (24, true));
    }

    #[test]
    fn new_entries_never_push_the_loaded_rows_out() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        let newer: Vec<i64> = (20..=23)
            .map(|d| db.entry(1, &format!("2026-10-{d}T08:00:00.000Z"), false))
            .collect();
        assert!(
            !l.reload(&store, today()).unwrap(),
            "the list has a floor: no restart"
        );
        let mut want: Vec<i64> = newer.into_iter().rev().collect();
        want.extend(&ids[..2]);
        assert_eq!((row_ids(&l), l.total(), l.has_more()), (want, 7, true));
    }

    /// The first run: the list was empty, so whole, when the first refresh filled it.
    #[test]
    fn a_short_list_made_long_by_a_refresh_is_paged_again() {
        let db = TestDb::new();
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert!(l.rows().is_empty() && !l.has_more());
        let ids = db.days(5, 1, |_| false);
        assert!(
            l.reload(&store, today()).unwrap(),
            "a whole list grew past a page"
        );
        assert!(l.restart_due(l.generation()));
        l.restart(&store, None, today()).unwrap();
        assert_eq!(
            (row_ids(&l), l.total(), l.has_more()),
            (ids[..2].to_vec(), 5, true)
        );
        assert!(!l.restart_due(l.generation()));
    }

    #[test]
    fn the_selection_stays_loaded_when_the_list_restarts() {
        let db = TestDb::new();
        let ids = db.days(4, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        l.load_more(&store, today()).unwrap();
        l.change(
            ListQuery {
                filter: ListFilter::Unread,
                ..q(Source::Feed(1))
            },
            &store,
            Some(ids[3]),
            today(),
        )
        .unwrap();
        assert_eq!((row_ids(&l), l.has_more()), (ids, false));
    }

    #[test]
    fn mark_all_read_counts_the_rows_not_loaded() {
        let db = TestDb::new();
        db.days(3, 1, |day| day != 1); // only the oldest, not loaded, is unread
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert!(l.rows().iter().all(|r| r.is_read) && l.can_mark_all_read());
        db.execute("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z'");
        l.reload(&store, today()).unwrap();
        assert!(!l.can_mark_all_read());
    }

    /// Mac `AppState.merge` and ListPageTests.reading_through_a_long_unread_list_keeps_the_selection_and_the_counts.
    #[test]
    fn read_rows_stay_listed_in_an_unread_list_until_the_source_changes() {
        let db = TestDb::new();
        let ids = db.days(6, 1, |_| false);
        let unread = ListQuery {
            filter: ListFilter::Unread,
            ..q(Source::Feed(1))
        };
        let (store, mut l) = opened(&db, unread.clone(), 2);
        let _ = l.set_read_local(&[ids[1]], true);
        db.execute(&format!(
            "UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}",
            ids[1]
        ));
        l.reload(&store, today()).unwrap();
        assert_eq!(row_ids(&l), ids[..2]);
        assert!(l.rows()[1].is_read);
        assert_eq!((l.unread(), l.total(), l.has_more()), (5, 6, true));
        l.load_more(&store, today()).unwrap();
        assert_eq!(row_ids(&l)[..2], ids[..2]);
        l.change(q(Source::Feed(8)), &store, None, today()).unwrap();
        l.change(unread, &store, None, today()).unwrap();
        assert!(
            !row_ids(&l).contains(&ids[1]),
            "a new visit forgets what the last one read"
        );
    }

    #[test]
    fn search_replaces_the_list_and_ignores_the_unread_filter() {
        let db = TestDb::new();
        let a = db.entry_with(NewEntry {
            title: Some("Café crème"),
            body: Some("le petit déjeuner"),
            published: "2026-10-02T08:00:00.000Z",
            ..Default::default()
        });
        let b = db.entry_with(NewEntry {
            title: Some("Cafe noir"),
            body: Some("le soir"),
            read: true,
            ..Default::default()
        });
        let search = |text: &str| ListQuery {
            source: Source::Feed(1),
            filter: ListFilter::Unread,
            search: Some(text.into()),
        };
        let (_, l) = opened(&db, search("cafe"), PAGE_SIZE);
        assert_eq!(row_ids(&l), [a, b]);
        assert!(!l.can_mark_all_read(), "never during a search");
        assert_eq!(
            row_ids(&opened(&db, search("cafe dejeuner"), PAGE_SIZE).1),
            [a]
        );
        assert_eq!(
            opened(&db, search("zzz"), PAGE_SIZE).1.empty_text(),
            Some(NO_RESULTS)
        );
    }

    #[test]
    fn empty_lists_say_why() {
        let db = TestDb::new();
        let text = |query| opened(&db, query, PAGE_SIZE).1.empty_text();
        assert_eq!(text(q(Source::Feed(1))), Some(NO_ENTRIES));
        assert_eq!(text(q(Source::Unread)), Some(NO_UNREAD));
        assert_eq!(
            text(ListQuery {
                filter: ListFilter::Unread,
                ..q(Source::Starred)
            }),
            Some(NO_UNREAD)
        );
        db.entry(1, "2026-10-01T08:00:00.000Z", false);
        assert_eq!(text(q(Source::Feed(1))), None);
        assert_eq!(
            [NO_RESULTS, NO_UNREAD, NO_ENTRIES],
            ["No results", "No unread entries", "No entries"]
        );
    }

    #[test]
    fn local_changes_show_at_once_and_revert_exactly() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |day| day == 2);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), PAGE_SIZE);
        let read = |l: &EntryList| l.rows().iter().map(|r| r.is_read).collect::<Vec<_>>();
        assert_eq!(read(&l), [false, true, false]);
        let r = l.set_read_local(&[ids[0]], true);
        assert_eq!(read(&l), [true, true, false]);
        l.reload(&store, today()).unwrap();
        assert_eq!(
            read(&l),
            [true, true, false],
            "a reload while the write is pending keeps the mark"
        );
        l.revert(r);
        assert_eq!(read(&l), [false, true, false]);
        let r = l.set_read_local(&[ids[0]], true);
        db.execute(&format!(
            "UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}",
            ids[0]
        ));
        l.settle(r);
        l.reload(&store, today()).unwrap();
        assert_eq!(
            read(&l),
            [true, true, false],
            "settled: the database says so now"
        );
        db.execute(&format!(
            "UPDATE entry SET read_at = NULL WHERE id = {}",
            ids[0]
        ));
        l.reload(&store, today()).unwrap();
        assert_eq!(
            read(&l),
            [false, true, false],
            "no overlay left after settle"
        );
        let r = l.set_starred_local(ids[2], true);
        assert!(l.rows()[2].is_starred);
        l.revert(r);
        assert!(!l.rows()[2].is_starred);
        let r = l.mark_all_read_local();
        assert_eq!(read(&l), [true, true, true]);
        l.revert(r);
        assert_eq!(
            read(&l),
            [false, true, false],
            "the row read before stays read"
        );
        assert_eq!(
            (db.read_at(ids[2]), db.starred(ids[2])),
            (None, false),
            "local edits never write"
        );
    }

    #[test]
    fn the_journal_loads_whole_in_rank_order_and_keeps_read_rows_at_their_rank() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |_| false);
        db.edition(&[(ids[2], "front"), (ids[0], "wire"), (ids[1], "wire")]);
        let (store, mut l) = opened(
            &db,
            ListQuery {
                filter: ListFilter::Unread,
                ..q(Source::Journal)
            },
            1,
        );
        assert_eq!(
            (row_ids(&l), l.has_more()),
            (vec![ids[2], ids[0], ids[1]], false)
        );
        let _ = l.set_read_local(&[ids[0]], true);
        db.execute(&format!(
            "UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}",
            ids[0]
        ));
        assert!(!l.reload(&store, today()).unwrap());
        assert_eq!(row_ids(&l), [ids[2], ids[0], ids[1]]);
    }
}
