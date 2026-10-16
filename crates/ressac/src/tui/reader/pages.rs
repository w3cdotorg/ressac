//! The teletext page directory of the terminal reader (Antiope spec §4, §6): every reserved page
//! and every numbered folder and feed, where each one leads, and its count.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ressac_core::model::CadenceGroup;
use ressac_core::store::pages::{PageKind, PageRow};
use ressac_core::store::{Source, SourceNode, SourceNodeKind, SourcesSnapshot};

use super::state::GardenView;

/// Where typing a page number leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDestination {
    Sommaire,
    Guide,
    Source(Source),
    Garden(GardenView),
}

/// What the reader column of the Antiope look shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderPage {
    /// The selected entry (or P100 while none is selected).
    #[default]
    Article,
    Sommaire,
    Guide,
    /// After P104's last entry: `FIN DE L'EDITION`.
    EndOfEdition,
}

/// One row of the index column and of P100.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRow {
    pub page: u16,
    pub title: String,
    pub kind: PageKind,
    pub depth: usize,
    pub is_root_folder: bool,
    /// Unread entries (feeds, folders, smart lists, rhythm groups) or feeds to triage (P800).
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageDirectory {
    rows: Vec<DirRow>,
    destinations: HashMap<u16, PageDestination>,
    pages_of: HashMap<Source, u16>,
}

fn reserved_destination(page: u16) -> Option<PageDestination> {
    use PageDestination::{Garden, Guide, Sommaire};
    let source = |s| Some(PageDestination::Source(s));
    match page {
        100 => Some(Sommaire),
        101 => source(Source::Unread),
        102 => source(Source::Today),
        103 => source(Source::Starred),
        104 => source(Source::Journal),
        105 => source(Source::Cadence(CadenceGroup::Daily)),
        106 => source(Source::Cadence(CadenceGroup::Weekly)),
        107 => source(Source::Cadence(CadenceGroup::Monthly)),
        108 => source(Source::Cadence(CadenceGroup::Rare)),
        109 => source(Source::Cadence(CadenceGroup::Silent)),
        800 => Some(Garden(GardenView::NeedsTriage)),
        801 => Some(Garden(GardenView::AllFeeds)),
        802 => Some(Garden(GardenView::Cemetery)),
        899 => Some(Guide),
        _ => None,
    }
}

impl PageDirectory {
    /// `pages` as `Ressac::pages()` returns them (reserved first, then by page); `sources` gives
    /// the counts, absent until the first load.
    pub fn new(pages: &[PageRow], sources: Option<&SourcesSnapshot>) -> Self {
        let counts = sources.map(unread_by_node).unwrap_or_default();
        let by_page: HashMap<u16, &PageRow> = pages.iter().map(|r| (r.page, r)).collect();
        let mut destinations = HashMap::new();
        let mut pages_of = HashMap::new();
        let mut rows = Vec::with_capacity(pages.len());
        for row in pages {
            let destination = match row.kind {
                PageKind::Reserved => reserved_destination(row.page),
                PageKind::Folder => row.id.map(|id| PageDestination::Source(Source::Folder(id))),
                PageKind::Feed => row.id.map(|id| PageDestination::Source(Source::Feed(id))),
            };
            if let Some(destination) = destination {
                destinations.insert(row.page, destination);
                if let PageDestination::Source(source) = destination {
                    pages_of.insert(source, row.page);
                }
            }
            // A parent chain cannot loop (pages are a tree), but never trust the data: stop at
            // the number of rows.
            let mut depth = 0;
            let mut parent = row.parent_page;
            while let Some(p) = parent {
                depth += 1;
                if depth > pages.len() {
                    break;
                }
                parent = by_page.get(&p).and_then(|r| r.parent_page);
            }
            let count = match (row.kind, sources) {
                (PageKind::Reserved, Some(s)) => reserved_count(row.page, s),
                (PageKind::Reserved, None) => None,
                (PageKind::Folder, _) => row.id.and_then(|id| counts.folders.get(&id).copied()),
                (PageKind::Feed, _) => row.id.and_then(|id| counts.feeds.get(&id).copied()),
            };
            rows.push(DirRow {
                page: row.page,
                title: row.title.clone(),
                kind: row.kind,
                depth,
                is_root_folder: row.kind == PageKind::Folder && row.parent_page.is_none(),
                count,
            });
        }
        PageDirectory {
            rows,
            destinations,
            pages_of,
        }
    }

    pub fn rows(&self) -> &[DirRow] {
        &self.rows
    }

    pub fn destination(&self, page: u16) -> Option<PageDestination> {
        self.destinations.get(&page).copied()
    }

    /// The page a source sits on, for the header and the index column's selection.
    pub fn page_of(&self, source: Source) -> Option<u16> {
        self.pages_of.get(&source).copied()
    }

    pub fn page_of_garden(&self, view: GardenView) -> u16 {
        match view {
            GardenView::NeedsTriage => 800,
            GardenView::AllFeeds => 801,
            GardenView::Cemetery => 802,
        }
    }
}

/// How long an unknown page shows `NOT FOUND` in the header (Antiope spec §6).
pub const NOT_FOUND_FOR: Duration = Duration::seconds(2);

/// What the state tells the navigation about one key.
pub struct NavContext<'a> {
    pub dir: &'a PageDirectory,
    pub now: DateTime<Utc>,
    /// The search prompt has the keyboard: every key is typing there.
    pub in_text_input: bool,
    /// The rank of the Journal's last entry, when the Journal's edition is shown.
    pub journal_last: Option<u32>,
    /// The rank of the Journal entry being read, when the reader shows one.
    pub on_journal_entry: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavOutcome {
    /// Not a page key: the rest of the key handling goes on.
    Unhandled,
    /// Handled, nothing else to do.
    Consumed,
    Jump(PageDestination),
    /// SUITE past the edition's last entry: `FIN DE L'EDITION`.
    EndOfEdition,
    /// RETOUR from `FIN DE L'EDITION`: the last entry is back.
    BackToLastEntry,
}

/// The Antiope look's own navigation state (spec §5, §6): the page number being typed, the
/// unknown page just typed, the page shown in the reader column. Pure: the clock comes in the
/// `NavContext`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AntiopeNav {
    digits: String,
    not_found: Option<(u16, DateTime<Utc>)>,
    reader_page: ReaderPage,
}

impl AntiopeNav {
    /// The digits typed so far, padded with `_` (`4__`); `None` when idle.
    pub fn typing(&self) -> Option<String> {
        (!self.digits.is_empty()).then(|| format!("{:_<3}", self.digits))
    }

    /// The unknown page typed less than `NOT_FOUND_FOR` ago.
    pub fn not_found(&self, now: DateTime<Utc>) -> Option<u16> {
        self.not_found
            .filter(|(_, at)| now - *at < NOT_FOUND_FOR)
            .map(|(page, _)| page)
    }

    pub fn reader_page(&self) -> ReaderPage {
        self.reader_page
    }

    pub fn show(&mut self, page: ReaderPage) {
        self.reader_page = page;
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &NavContext) -> NavOutcome {
        if ctx.in_text_input {
            return NavOutcome::Unhandled;
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() && plain => {
                self.not_found = None;
                self.digits.push(c);
                if self.digits.len() < 3 {
                    return NavOutcome::Consumed;
                }
                let page: u16 = std::mem::take(&mut self.digits).parse().unwrap_or(0);
                match ctx.dir.destination(page) {
                    Some(destination) => {
                        self.reader_page = match destination {
                            PageDestination::Sommaire => ReaderPage::Sommaire,
                            PageDestination::Guide => ReaderPage::Guide,
                            PageDestination::Source(_) | PageDestination::Garden(_) => {
                                ReaderPage::Article
                            }
                        };
                        NavOutcome::Jump(destination)
                    }
                    None => {
                        self.not_found = Some((page, ctx.now));
                        NavOutcome::Consumed
                    }
                }
            }
            KeyCode::Backspace if !self.digits.is_empty() => {
                self.digits.pop();
                NavOutcome::Consumed
            }
            KeyCode::Esc if !self.digits.is_empty() => {
                self.digits.clear();
                NavOutcome::Consumed
            }
            KeyCode::Char('S') if plain => self.jump_to(ReaderPage::Sommaire),
            KeyCode::Char('?') if plain => self.jump_to(ReaderPage::Guide),
            // Nothing comes after `FIN DE L'EDITION`; RETOUR brings the last entry back.
            KeyCode::Char('j' | ' ') if self.reader_page == ReaderPage::EndOfEdition => {
                NavOutcome::Consumed
            }
            KeyCode::Char('k') if self.reader_page == ReaderPage::EndOfEdition => {
                self.reader_page = ReaderPage::Article;
                NavOutcome::BackToLastEntry
            }
            KeyCode::Char('j')
                if ctx.on_journal_entry.is_some() && ctx.on_journal_entry == ctx.journal_last =>
            {
                self.reader_page = ReaderPage::EndOfEdition;
                NavOutcome::EndOfEdition
            }
            _ => NavOutcome::Unhandled,
        }
    }

    fn jump_to(&mut self, page: ReaderPage) -> NavOutcome {
        self.digits.clear();
        self.not_found = None;
        self.reader_page = page;
        NavOutcome::Jump(match page {
            ReaderPage::Guide => PageDestination::Guide,
            _ => PageDestination::Sommaire,
        })
    }
}

/// `104 3/43`: an entry is a sub-page of its source's page (Antiope spec §6).
pub fn subpage_label(page: u16, position: usize, total: usize) -> String {
    format!("{page} {position}/{total}")
}

fn reserved_count(page: u16, s: &SourcesSnapshot) -> Option<usize> {
    let c = &s.counts;
    let group = |g| c.cadence.get(&g).copied().unwrap_or(0);
    Some(match page {
        101 => c.unread,
        102 => c.today,
        103 => c.starred,
        104 => c.journal,
        105 => group(CadenceGroup::Daily),
        106 => group(CadenceGroup::Weekly),
        107 => group(CadenceGroup::Monthly),
        108 => group(CadenceGroup::Rare),
        109 => group(CadenceGroup::Silent),
        800 => c.needs_triage,
        _ => return None,
    })
}

/// Unread entries per folder and per feed (`PageKind` has no `Hash`, so two maps).
#[derive(Default)]
struct NodeCounts {
    folders: HashMap<i64, usize>,
    feeds: HashMap<i64, usize>,
}

fn unread_by_node(s: &SourcesSnapshot) -> NodeCounts {
    fn walk(nodes: &[SourceNode], out: &mut NodeCounts) {
        for n in nodes {
            match &n.kind {
                SourceNodeKind::Folder { id, .. } => out.folders.insert(*id, n.unread),
                SourceNodeKind::Feed { id, .. } => out.feeds.insert(*id, n.unread),
            };
            walk(&n.children, out);
        }
    }
    let mut out = NodeCounts::default();
    walk(&s.tree, &mut out);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn reserved(page: u16, title: &str) -> PageRow {
        PageRow {
            page,
            kind: PageKind::Reserved,
            id: None,
            title: title.into(),
            parent_page: None,
        }
    }

    pub(crate) fn sample_pages() -> Vec<PageRow> {
        let mut rows: Vec<PageRow> = ressac_core::store::pages::RESERVED_PAGES
            .iter()
            .zip([
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
            ])
            .map(|(p, t)| reserved(*p, t))
            .collect();
        rows.push(PageRow {
            page: 400,
            kind: PageKind::Folder,
            id: Some(4),
            title: "Infos".into(),
            parent_page: None,
        });
        rows.push(PageRow {
            page: 403,
            kind: PageKind::Feed,
            id: Some(31),
            title: "Actualitté".into(),
            parent_page: Some(400),
        });
        rows.push(PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: Some(400),
        });
        rows.push(PageRow {
            page: 110,
            kind: PageKind::Feed,
            id: Some(2),
            title: "Daring Fireball".into(),
            parent_page: None,
        });
        rows.sort_by_key(|r| (r.kind != PageKind::Reserved, r.page));
        rows
    }

    #[test]
    fn reserved_pages_go_where_the_app_sends_them() {
        let dir = PageDirectory::new(&sample_pages(), None);
        assert_eq!(dir.destination(100), Some(PageDestination::Sommaire));
        assert_eq!(
            dir.destination(101),
            Some(PageDestination::Source(Source::Unread))
        );
        assert_eq!(
            dir.destination(102),
            Some(PageDestination::Source(Source::Today))
        );
        assert_eq!(
            dir.destination(103),
            Some(PageDestination::Source(Source::Starred))
        );
        assert_eq!(
            dir.destination(104),
            Some(PageDestination::Source(Source::Journal))
        );
        assert_eq!(
            dir.destination(105),
            Some(PageDestination::Source(Source::Cadence(
                CadenceGroup::Daily
            )))
        );
        assert_eq!(
            dir.destination(109),
            Some(PageDestination::Source(Source::Cadence(
                CadenceGroup::Silent
            )))
        );
        assert_eq!(
            dir.destination(800),
            Some(PageDestination::Garden(GardenView::NeedsTriage))
        );
        assert_eq!(
            dir.destination(801),
            Some(PageDestination::Garden(GardenView::AllFeeds))
        );
        assert_eq!(
            dir.destination(802),
            Some(PageDestination::Garden(GardenView::Cemetery))
        );
        assert_eq!(dir.destination(899), Some(PageDestination::Guide));
    }

    #[test]
    fn folders_and_feeds_go_to_their_source() {
        let dir = PageDirectory::new(&sample_pages(), None);
        assert_eq!(
            dir.destination(400),
            Some(PageDestination::Source(Source::Folder(4)))
        );
        assert_eq!(
            dir.destination(412),
            Some(PageDestination::Source(Source::Feed(12)))
        );
        assert_eq!(dir.destination(437), None);
        assert_eq!(dir.page_of(Source::Feed(12)), Some(412));
        assert_eq!(dir.page_of(Source::Journal), Some(104));
        assert_eq!(dir.page_of(Source::Feed(99)), None);
    }

    #[test]
    fn rows_have_depth_and_root_folders_are_marked() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let infos = dir.rows().iter().find(|r| r.page == 400).unwrap();
        let actualitte = dir.rows().iter().find(|r| r.page == 403).unwrap();
        assert_eq!((infos.depth, infos.is_root_folder), (0, true));
        assert_eq!((actualitte.depth, actualitte.is_root_folder), (1, false));
    }

    #[test]
    fn counts_come_from_the_sources() {
        use ressac_core::store::SourceCounts;
        let snapshot = SourcesSnapshot {
            counts: SourceCounts {
                unread: 1083,
                needs_triage: 7,
                ..SourceCounts::default()
            },
            tree: vec![SourceNode {
                kind: SourceNodeKind::Folder {
                    id: 4,
                    name: "Infos".into(),
                },
                page: Some(400),
                unread: 12,
                children: vec![SourceNode {
                    kind: SourceNodeKind::Feed {
                        id: 12,
                        title: "Courrier international".into(),
                    },
                    page: Some(412),
                    unread: 5,
                    children: Vec::new(),
                }],
            }],
        };
        let dir = PageDirectory::new(&sample_pages(), Some(&snapshot));
        let count = |page| dir.rows().iter().find(|r| r.page == page).unwrap().count;
        assert_eq!(count(101), Some(1083));
        assert_eq!(count(800), Some(7));
        assert_eq!(count(400), Some(12));
        assert_eq!(count(412), Some(5));
        assert_eq!(count(100), None);
        assert_eq!(count(110), None);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn t0() -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 10, 16, 16, 0, 0).unwrap()
    }

    fn ctx(dir: &PageDirectory) -> NavContext<'_> {
        NavContext {
            dir,
            now: t0(),
            in_text_input: false,
            journal_last: Some(43),
            on_journal_entry: None,
        }
    }

    fn type_digits(nav: &mut AntiopeNav, dir: &PageDirectory, digits: &str) -> NavOutcome {
        let mut outcome = NavOutcome::Unhandled;
        for c in digits.chars() {
            outcome = nav.on_key(key(KeyCode::Char(c)), &ctx(dir));
        }
        outcome
    }

    #[test]
    fn three_digits_jump_and_show_while_typing() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        assert_eq!(
            nav.on_key(key(KeyCode::Char('4')), &ctx(&dir)),
            NavOutcome::Consumed
        );
        assert_eq!(nav.typing(), Some("4__".to_string()));
        nav.on_key(key(KeyCode::Char('1')), &ctx(&dir));
        assert_eq!(nav.typing(), Some("41_".to_string()));
        assert_eq!(
            nav.on_key(key(KeyCode::Char('2')), &ctx(&dir)),
            NavOutcome::Jump(PageDestination::Source(Source::Feed(12)))
        );
        assert_eq!(nav.typing(), None);
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn backspace_corrects_and_escape_cancels() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        type_digits(&mut nav, &dir, "43");
        assert_eq!(nav.typing(), Some("43_".to_string()));
        nav.on_key(key(KeyCode::Backspace), &ctx(&dir));
        assert_eq!(nav.typing(), Some("4__".to_string()));
        nav.on_key(key(KeyCode::Esc), &ctx(&dir));
        assert_eq!(nav.typing(), None);
        // With nothing typed, both keys belong to the rest of the reader.
        assert_eq!(
            nav.on_key(key(KeyCode::Backspace), &ctx(&dir)),
            NavOutcome::Unhandled
        );
        assert_eq!(
            nav.on_key(key(KeyCode::Esc), &ctx(&dir)),
            NavOutcome::Unhandled
        );
    }

    #[test]
    fn an_unknown_page_is_not_found_for_two_seconds() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        assert_eq!(type_digits(&mut nav, &dir, "437"), NavOutcome::Consumed);
        assert_eq!(nav.typing(), None);
        assert_eq!(
            nav.not_found(t0() + Duration::milliseconds(1999)),
            Some(437)
        );
        assert_eq!(nav.not_found(t0() + Duration::seconds(2)), None);
        // The next digit clears it at once.
        nav.on_key(key(KeyCode::Char('1')), &ctx(&dir));
        assert_eq!(nav.not_found(t0()), None);
    }

    #[test]
    fn digits_in_a_text_input_are_typing_and_chords_are_not_digits() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        let mut c = ctx(&dir);
        c.in_text_input = true;
        assert_eq!(
            nav.on_key(key(KeyCode::Char('4')), &c),
            NavOutcome::Unhandled
        );
        assert_eq!(nav.typing(), None);
        let alt = KeyEvent::new(KeyCode::Char('4'), KeyModifiers::ALT);
        assert_eq!(nav.on_key(alt, &ctx(&dir)), NavOutcome::Unhandled);
    }

    #[test]
    fn sommaire_and_guide_keys_and_numbers() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        assert_eq!(
            nav.on_key(
                KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT),
                &ctx(&dir)
            ),
            NavOutcome::Jump(PageDestination::Sommaire)
        );
        assert_eq!(nav.reader_page(), ReaderPage::Sommaire);
        assert_eq!(
            nav.on_key(key(KeyCode::Char('?')), &ctx(&dir)),
            NavOutcome::Jump(PageDestination::Guide)
        );
        assert_eq!(nav.reader_page(), ReaderPage::Guide);
        assert_eq!(
            type_digits(&mut nav, &dir, "104"),
            NavOutcome::Jump(PageDestination::Source(Source::Journal))
        );
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        assert_eq!(
            type_digits(&mut nav, &dir, "801"),
            NavOutcome::Jump(PageDestination::Garden(GardenView::AllFeeds))
        );
        assert_eq!(
            type_digits(&mut nav, &dir, "100"),
            NavOutcome::Jump(PageDestination::Sommaire)
        );
        assert_eq!(nav.reader_page(), ReaderPage::Sommaire);
    }

    #[test]
    fn suite_past_the_editions_last_entry_lands_on_the_end_page() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        let mut c = ctx(&dir);
        // Not the last entry: the reader's own `j` moves on.
        c.on_journal_entry = Some(42);
        assert_eq!(
            nav.on_key(key(KeyCode::Char('j')), &c),
            NavOutcome::Unhandled
        );
        c.on_journal_entry = Some(43);
        assert_eq!(
            nav.on_key(key(KeyCode::Char('j')), &c),
            NavOutcome::EndOfEdition
        );
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        assert_eq!(
            nav.on_key(key(KeyCode::Char('j')), &c),
            NavOutcome::Consumed
        );
        assert_eq!(
            nav.on_key(key(KeyCode::Char(' ')), &c),
            NavOutcome::Consumed
        );
        assert_eq!(
            nav.on_key(key(KeyCode::Char('k')), &c),
            NavOutcome::BackToLastEntry
        );
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn the_sub_page_label_counts_by_rank() {
        assert_eq!(subpage_label(104, 3, 43), "104 3/43");
        assert_eq!(subpage_label(412, 1, 20), "412 1/20");
    }
}
