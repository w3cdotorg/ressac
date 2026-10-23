//! Antiope's pages (the Mac's `PageDirectory`, `PageEntry` and `AntiopeState`): the reserved
//! pages and the directory of every page number, the three-digit entry, and `AntiopeNav`, the
//! state machine that answers a command with a handled flag and the [`Effect`]s the QML
//! applies across the singletons. No Qt, no clock of its own: `now` comes in with the context.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ressac_core::model::{CadenceGroup, FeedId};
use ressac_core::store::{Source, SourceCounts, SourceNode, SourceNodeKind, SourcesSnapshot};

use crate::journal;
use crate::keys::Command;
use crate::sources::{GardenScope, Selection, rhythm_title};
use crate::teletext::CellLine;

pub mod frame;
pub mod special;

/// The index column's width, in cells.
pub const INDEX_CELLS: usize = 22;
/// The entries column's width, in cells.
pub const ENTRY_CELLS: usize = 28;
/// The narrowest reader column, in cells.
pub const MIN_READER_CELLS: usize = 24;
/// 22 + 1 + 28 + 1 + 24: the three columns and their two separators.
pub const MIN_CELLS: usize = 76;
/// How long an unknown page shows `NOT FOUND` in the header.
pub const NOT_FOUND_FOR: Duration = Duration::from_secs(2);

/// Page numbers fixed in code: never stored, refused by `ressac page set`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservedPage {
    Index,
    Unread,
    Today,
    Starred,
    Journal,
    Daily,
    Weekly,
    Monthly,
    Rare,
    Silent,
    NeedsTriage,
    AllFeeds,
    Cemetery,
    Guide,
}

impl ReservedPage {
    /// In the core's order (`store::pages::RESERVED_PAGES`).
    pub const ALL: [ReservedPage; 14] = [
        ReservedPage::Index,
        ReservedPage::Unread,
        ReservedPage::Today,
        ReservedPage::Starred,
        ReservedPage::Journal,
        ReservedPage::Daily,
        ReservedPage::Weekly,
        ReservedPage::Monthly,
        ReservedPage::Rare,
        ReservedPage::Silent,
        ReservedPage::NeedsTriage,
        ReservedPage::AllFeeds,
        ReservedPage::Cemetery,
        ReservedPage::Guide,
    ];

    pub fn number(self) -> u16 {
        match self {
            ReservedPage::Index => 100,
            ReservedPage::Unread => 101,
            ReservedPage::Today => 102,
            ReservedPage::Starred => 103,
            ReservedPage::Journal => 104,
            ReservedPage::Daily => 105,
            ReservedPage::Weekly => 106,
            ReservedPage::Monthly => 107,
            ReservedPage::Rare => 108,
            ReservedPage::Silent => 109,
            ReservedPage::NeedsTriage => 800,
            ReservedPage::AllFeeds => 801,
            ReservedPage::Cemetery => 802,
            ReservedPage::Guide => 899,
        }
    }

    pub fn from_number(number: u16) -> Option<ReservedPage> {
        ReservedPage::ALL.into_iter().find(|p| p.number() == number)
    }

    /// The core's titles, so that the app and `ressac page list` agree.
    pub fn title(self) -> &'static str {
        match self {
            ReservedPage::Index => "Index",
            ReservedPage::Unread => "Unread",
            ReservedPage::Today => "Today",
            ReservedPage::Starred => "Starred",
            ReservedPage::Journal => "Journal",
            ReservedPage::Daily => rhythm_title(CadenceGroup::Daily),
            ReservedPage::Weekly => rhythm_title(CadenceGroup::Weekly),
            ReservedPage::Monthly => rhythm_title(CadenceGroup::Monthly),
            ReservedPage::Rare => rhythm_title(CadenceGroup::Rare),
            ReservedPage::Silent => rhythm_title(CadenceGroup::Silent),
            ReservedPage::NeedsTriage => "Needs triage",
            ReservedPage::AllFeeds => "All feeds",
            ReservedPage::Cemetery => "Cemetery",
            ReservedPage::Guide => "Guide",
        }
    }

    /// The rhythm group a page 105 to 109 lists.
    pub fn cadence_group(self) -> Option<CadenceGroup> {
        match self {
            ReservedPage::Daily => Some(CadenceGroup::Daily),
            ReservedPage::Weekly => Some(CadenceGroup::Weekly),
            ReservedPage::Monthly => Some(CadenceGroup::Monthly),
            ReservedPage::Rare => Some(CadenceGroup::Rare),
            ReservedPage::Silent => Some(CadenceGroup::Silent),
            _ => None,
        }
    }

    pub fn destination(self) -> PageDestination {
        let source = |source| PageDestination::Source(Selection::Source(source));
        let garden = |scope| PageDestination::Source(Selection::Garden(scope));
        match self {
            ReservedPage::Index => PageDestination::Index,
            ReservedPage::Unread => source(Source::Unread),
            ReservedPage::Today => source(Source::Today),
            ReservedPage::Starred => source(Source::Starred),
            ReservedPage::Journal => source(Source::Journal),
            ReservedPage::Daily => source(Source::Cadence(CadenceGroup::Daily)),
            ReservedPage::Weekly => source(Source::Cadence(CadenceGroup::Weekly)),
            ReservedPage::Monthly => source(Source::Cadence(CadenceGroup::Monthly)),
            ReservedPage::Rare => source(Source::Cadence(CadenceGroup::Rare)),
            ReservedPage::Silent => source(Source::Cadence(CadenceGroup::Silent)),
            ReservedPage::NeedsTriage => garden(GardenScope::NeedsTriage),
            ReservedPage::AllFeeds => garden(GardenScope::All),
            ReservedPage::Cemetery => garden(GardenScope::Cemetery),
            ReservedPage::Guide => PageDestination::Guide,
        }
    }

    /// The count shown beside the page: unread entries for the smart lists, the edition and
    /// the rhythm groups, feeds to triage for P800.
    pub fn count(self, counts: &SourceCounts) -> usize {
        match self {
            ReservedPage::Unread => counts.unread,
            ReservedPage::Today => counts.today,
            ReservedPage::Starred => counts.starred,
            ReservedPage::Journal => counts.journal,
            ReservedPage::NeedsTriage => counts.needs_triage,
            _ => self
                .cadence_group()
                .and_then(|group| counts.cadence.get(&group).copied())
                .unwrap_or(0),
        }
    }

    /// The garden's three pages, drawn in magenta in the index.
    pub fn is_garden(self) -> bool {
        matches!(
            self,
            ReservedPage::NeedsTriage | ReservedPage::AllFeeds | ReservedPage::Cemetery
        )
    }
}

/// Where a page number leads: a source of the window, or one of the two pages drawn in the
/// reader column (P100 SOMMAIRE, P899 GUIDE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDestination {
    Source(Selection),
    Index,
    Guide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRow {
    pub number: u16,
    pub title: String,
    /// Unread entries (a folder sums its subtree), or the reserved page's count.
    pub unread: usize,
    pub reserved: bool,
    /// 0 for reserved pages and root items, one more per folder level.
    pub depth: usize,
}

/// Every page of the library: the reserved pages, then the folders and feeds the core
/// numbered, in the sidebar's display order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageDirectory {
    rows: Vec<DirRow>,
    destinations: HashMap<u16, PageDestination>,
    numbers: HashMap<Selection, u16>,
    feed_pages: HashMap<FeedId, u16>,
}

impl PageDirectory {
    pub fn new(snapshot: &SourcesSnapshot) -> PageDirectory {
        let mut directory = PageDirectory::default();
        for page in ReservedPage::ALL {
            let number = page.number();
            directory.rows.push(DirRow {
                number,
                title: page.title().to_string(),
                unread: page.count(&snapshot.counts),
                reserved: true,
                depth: 0,
            });
            let destination = page.destination();
            directory.destinations.insert(number, destination);
            if let PageDestination::Source(selection) = destination {
                directory.numbers.insert(selection, number);
            }
        }
        directory.walk(&snapshot.tree, 0);
        directory
    }

    fn walk(&mut self, nodes: &[SourceNode], depth: usize) {
        for node in nodes {
            // A node not numbered yet is skipped; a reserved number can only come from a
            // corrupt database and never hides the reserved page.
            if let Some(number) = node
                .page
                .filter(|n| ReservedPage::from_number(*n).is_none())
            {
                let (title, source) = match &node.kind {
                    SourceNodeKind::Folder { id, name } => (name, Source::Folder(*id)),
                    SourceNodeKind::Feed { id, title } => (title, Source::Feed(*id)),
                };
                self.rows.push(DirRow {
                    number,
                    title: title.clone(),
                    unread: node.unread,
                    reserved: false,
                    depth,
                });
                let selection = Selection::Source(source);
                self.destinations
                    .insert(number, PageDestination::Source(selection));
                self.numbers.insert(selection, number);
                if let Source::Feed(feed) = source {
                    self.feed_pages.insert(feed, number);
                }
            }
            self.walk(&node.children, depth + 1);
        }
    }

    /// Reserved pages, then folders and feeds depth-first in display order.
    pub fn rows(&self) -> &[DirRow] {
        &self.rows
    }

    /// `None` for a number no page holds: the header then shows `NOT FOUND`.
    pub fn destination(&self, number: u16) -> Option<PageDestination> {
        self.destinations.get(&number).copied()
    }

    /// The page of a selection, reserved ones included; `None` while a folder or feed has no
    /// number.
    pub fn number_of(&self, selection: &Selection) -> Option<u16> {
        self.numbers.get(selection).copied()
    }

    /// A feed's page, for the reader's sub-page line.
    pub fn page_of_feed(&self, feed: FeedId) -> Option<u16> {
        self.feed_pages.get(&feed).copied()
    }

    /// A folder at the top of the tree (green capitals in the index).
    pub fn is_root_folder(&self, row: &DirRow) -> bool {
        row.depth == 0
            && matches!(
                self.destination(row.number),
                Some(PageDestination::Source(Selection::Source(Source::Folder(
                    _
                ))))
            )
    }
}

/// The directory, rebuilt only when the snapshot differs.
#[derive(Debug, Default)]
pub struct DirectoryCache {
    snapshot: Option<SourcesSnapshot>,
    directory: PageDirectory,
    builds: usize,
}

impl DirectoryCache {
    pub fn get(&mut self, snapshot: &SourcesSnapshot) -> &PageDirectory {
        if self.snapshot.as_ref() != Some(snapshot) {
            self.builds += 1;
            self.directory = PageDirectory::new(snapshot);
            self.snapshot = Some(snapshot.clone());
        }
        &self.directory
    }

    /// How many times the directory was built, for the tests.
    pub fn builds(&self) -> usize {
        self.builds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOutcome {
    Pending,
    Complete(u16),
}

/// The three-digit page number being typed: a digit starts it, the third completes it;
/// CORRECTION erases the last digit, ANNULATION cancels it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageEntry {
    digits: String,
}

impl PageEntry {
    pub const LENGTH: usize = 3;

    /// The digits typed so far, padded with `_` (`4__`, `41_`); empty when idle.
    pub fn display(&self) -> String {
        if self.digits.is_empty() {
            return String::new();
        }
        format!("{:_<width$}", self.digits, width = Self::LENGTH)
    }

    pub fn digits(&self) -> &str {
        &self.digits
    }

    pub fn is_active(&self) -> bool {
        !self.digits.is_empty()
    }

    /// Adds `digit` (0 to 9; any other value is ignored). The third digit completes the
    /// number and leaves the entry idle again.
    pub fn type_digit(&mut self, digit: u8) -> EntryOutcome {
        if digit > 9 {
            return EntryOutcome::Pending;
        }
        self.digits.push(char::from(b'0' + digit));
        if self.digits.len() < Self::LENGTH {
            return EntryOutcome::Pending;
        }
        let number = self
            .digits
            .bytes()
            .fold(0, |n, b| n * 10 + u16::from(b - b'0'));
        self.digits.clear();
        EntryOutcome::Complete(number)
    }

    pub fn correct(&mut self) {
        self.digits.pop();
    }

    pub fn cancel(&mut self) {
        self.digits.clear();
    }
}

/// The three columns of the Antiope window, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntiopeColumn {
    #[default]
    Index,
    Entries,
    Reader,
}

impl AntiopeColumn {
    pub fn as_str(self) -> &'static str {
        match self {
            AntiopeColumn::Index => "index",
            AntiopeColumn::Entries => "entries",
            AntiopeColumn::Reader => "reader",
        }
    }

    pub fn parse(text: &str) -> Option<AntiopeColumn> {
        [
            AntiopeColumn::Index,
            AntiopeColumn::Entries,
            AntiopeColumn::Reader,
        ]
        .into_iter()
        .find(|c| c.as_str() == text)
    }

    pub fn next(self) -> Option<AntiopeColumn> {
        match self {
            AntiopeColumn::Index => Some(AntiopeColumn::Entries),
            AntiopeColumn::Entries => Some(AntiopeColumn::Reader),
            AntiopeColumn::Reader => None,
        }
    }

    pub fn previous(self) -> Option<AntiopeColumn> {
        match self {
            AntiopeColumn::Index => None,
            AntiopeColumn::Entries => Some(AntiopeColumn::Index),
            AntiopeColumn::Reader => Some(AntiopeColumn::Entries),
        }
    }
}

/// What the reader column shows: the selected entry, P100 SOMMAIRE, P899 GUIDE, or the end of
/// the morning edition that SUITE reaches from P104's last entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderPage {
    #[default]
    Article,
    Index,
    Guide,
    EndOfEdition,
}

impl ReaderPage {
    pub fn as_str(self) -> &'static str {
        match self {
            ReaderPage::Article => "article",
            ReaderPage::Index => "index",
            ReaderPage::Guide => "guide",
            ReaderPage::EndOfEdition => "end",
        }
    }
}

/// What the QML does after a command, in order, across the singletons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Select(Selection),
    RetryEdition,
    Refresh,
    ToggleImages,
    FocusEntries,
    MarkRead(i64),
    PageDownThenEnd(i64),
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// False hands the command to the Native handling, then to the focused view.
    pub handled: bool,
    pub effects: Vec<Effect>,
}

impl Outcome {
    /// `""` when not handled; else `handled` then `;select:<selection key>`, `;retry`,
    /// `;refresh`, `;images`, `;focusEntries`, `;markRead:<id>`, `;pageDownThenEnd:<id>` and
    /// `;notFound`, in order.
    pub fn encode(&self) -> String {
        if !self.handled {
            return String::new();
        }
        let mut out = String::from("handled");
        for effect in &self.effects {
            match effect {
                Effect::Select(selection) => {
                    out.push_str(";select:");
                    out.push_str(&selection.key());
                }
                Effect::RetryEdition => out.push_str(";retry"),
                Effect::Refresh => out.push_str(";refresh"),
                Effect::ToggleImages => out.push_str(";images"),
                Effect::FocusEntries => out.push_str(";focusEntries"),
                Effect::MarkRead(id) => out.push_str(&format!(";markRead:{id}")),
                Effect::PageDownThenEnd(id) => out.push_str(&format!(";pageDownThenEnd:{id}")),
                Effect::NotFound => out.push_str(";notFound"),
            }
        }
        out
    }

    fn handled(effects: Vec<Effect>) -> Outcome {
        Outcome {
            handled: true,
            effects,
        }
    }

    fn not_handled() -> Outcome {
        Outcome::default()
    }
}

/// What `AntiopeNav` needs to know about the window, at the moment of a command.
pub struct NavContext<'a> {
    pub directory: &'a PageDirectory,
    pub selection: Selection,
    /// The selected entry's id and whether it is read.
    pub selected_entry: Option<(i64, bool)>,
    /// The edition's last entry by rank.
    pub last_edition_entry: Option<i64>,
    /// `retries_journal(phase)`.
    pub journal_retries: bool,
    pub can_refresh: bool,
    pub can_navigate_entries: bool,
    pub now: Instant,
}

/// Whether RÉPÉTITION on P104 retries the edition rather than refreshing: offline, failed, or
/// turned away by another long run.
pub fn retries_journal(phase: &journal::Phase) -> bool {
    matches!(
        phase,
        journal::Phase::Offline | journal::Phase::Failed(_) | journal::Phase::Due
    )
}

/// What only the Antiope look needs: the focused column, the page number being typed,
/// `NOT FOUND`, the page in the reader column.
#[derive(Debug, Clone, Default)]
pub struct AntiopeNav {
    column: AntiopeColumn,
    entry: PageEntry,
    not_found: Option<(u16, Instant)>,
    reader_page: ReaderPage,
    advancing_to_end: bool,
}

impl AntiopeNav {
    pub fn column(&self) -> AntiopeColumn {
        self.column
    }

    pub fn set_column(&mut self, column: AntiopeColumn) {
        self.column = column;
    }

    pub fn reader_page(&self) -> ReaderPage {
        self.reader_page
    }

    /// A click on an entry or a jump brings the article back.
    pub fn show_article(&mut self) {
        self.reader_page = ReaderPage::Article;
    }

    pub fn entry(&self) -> &PageEntry {
        &self.entry
    }

    /// The unknown page just typed, until `NOT_FOUND_FOR` has passed.
    pub fn not_found(&self, now: Instant) -> Option<u16> {
        self.not_found
            .filter(|(_, until)| now < *until)
            .map(|(number, _)| number)
    }

    /// The page on screen: P100 or P899 in the reader column, else the selection's page
    /// (`None` while the selected folder or feed has no number yet).
    pub fn current_page(&self, directory: &PageDirectory, selection: &Selection) -> Option<u16> {
        match self.reader_page {
            ReaderPage::Index => Some(ReservedPage::Index.number()),
            ReaderPage::Guide => Some(ReservedPage::Guide.number()),
            ReaderPage::EndOfEdition => Some(ReservedPage::Journal.number()),
            ReaderPage::Article => directory.number_of(selection),
        }
    }

    /// Goes to a page: its source is selected (the QML applies the effect), the index column
    /// takes the focus; P100 and P899 open in the reader. An unknown number shows `NOT FOUND`
    /// and changes nothing else.
    pub fn jump(&mut self, number: u16, ctx: &NavContext<'_>) -> Outcome {
        self.entry.cancel();
        let Some(destination) = ctx.directory.destination(number) else {
            self.not_found = Some((number, ctx.now + NOT_FOUND_FOR));
            return Outcome::handled(vec![Effect::NotFound]);
        };
        self.not_found = None;
        let mut effects = Vec::new();
        match destination {
            PageDestination::Source(selection) => {
                effects.push(Effect::Select(selection));
                self.reader_page = ReaderPage::Article;
            }
            PageDestination::Index => self.reader_page = ReaderPage::Index,
            PageDestination::Guide => self.reader_page = ReaderPage::Guide,
        }
        self.column = AntiopeColumn::Index;
        Outcome::handled(effects)
    }

    /// Antiope's answer to a command: `handled` false hands it to the Native handling.
    pub fn handle(&mut self, command: Command, ctx: &NavContext<'_>) -> Outcome {
        match command {
            Command::PageDigit(digit) => {
                self.not_found = None;
                match self.entry.type_digit(digit) {
                    EntryOutcome::Complete(number) => self.jump(number, ctx),
                    EntryOutcome::Pending => Outcome::handled(Vec::new()),
                }
            }
            Command::PageCorrect if self.entry.is_active() => {
                self.entry.correct();
                Outcome::handled(Vec::new())
            }
            Command::PageCancel if self.entry.is_active() => {
                self.entry.cancel();
                Outcome::handled(Vec::new())
            }
            Command::PageCorrect | Command::PageCancel => Outcome::not_handled(),
            Command::Sommaire => self.jump(ReservedPage::Index.number(), ctx),
            Command::Guide => self.jump(ReservedPage::Guide.number(), ctx),
            Command::Repetition => {
                let on_journal = ctx.selection == Selection::Source(Source::Journal);
                if on_journal && ctx.journal_retries {
                    Outcome::handled(vec![Effect::RetryEdition])
                } else if ctx.can_refresh {
                    Outcome::handled(vec![Effect::Refresh])
                } else {
                    Outcome::not_handled()
                }
            }
            Command::ToggleImages => {
                if self.reader_page == ReaderPage::Article && ctx.selected_entry.is_some() {
                    Outcome::handled(vec![Effect::ToggleImages])
                } else {
                    Outcome::not_handled()
                }
            }
            Command::NextColumn => self.move_to(self.column.next()),
            Command::PreviousColumn => self.move_to(self.column.previous()),
            Command::NextIndexRow => self.move_in_index(1, ctx),
            Command::PreviousIndexRow => self.move_in_index(-1, ctx),
            Command::NextEntry | Command::PreviousEntry | Command::Advance => {
                self.suite_or_retour(command, ctx)
            }
            Command::ToggleRead
            | Command::ToggleStar
            | Command::FocusEntryList
            | Command::FocusSidebar => Outcome::not_handled(),
        }
    }

    /// Moving past the last entry of the edition marks it read, as `j` and Space do between
    /// entries, and shows FIN DE L'EDITION.
    pub fn land_on_end(&mut self, selected_entry: Option<(i64, bool)>) -> Outcome {
        self.reader_page = ReaderPage::EndOfEdition;
        match selected_entry {
            Some((id, false)) => Outcome::handled(vec![Effect::MarkRead(id)]),
            _ => Outcome::handled(Vec::new()),
        }
    }

    /// The page-down that `PageDownThenEnd` asked for is over (the QML then lands on the
    /// end, unless the window moved on).
    pub fn advance_finished(&mut self) {
        self.advancing_to_end = false;
    }

    fn move_to(&mut self, target: Option<AntiopeColumn>) -> Outcome {
        let Some(target) = target else {
            return Outcome::not_handled();
        };
        self.column = target;
        // As `→` from the native sidebar: the list takes the keyboard on its first entry.
        let effects = if target == AntiopeColumn::Entries {
            vec![Effect::FocusEntries]
        } else {
            Vec::new()
        };
        Outcome::handled(effects)
    }

    /// Always handled (the ends are spent): the neighbour of the current page, or the first
    /// row when the selection has no page.
    fn move_in_index(&mut self, offset: isize, ctx: &NavContext<'_>) -> Outcome {
        let rows = ctx.directory.rows();
        let current = self
            .current_page(ctx.directory, &ctx.selection)
            .and_then(|page| rows.iter().position(|row| row.number == page));
        let target = current.map_or(Some(0), |at| at.checked_add_signed(offset));
        match target.and_then(|at| rows.get(at)) {
            Some(row) => self.jump(row.number, ctx),
            None => Outcome::handled(Vec::new()),
        }
    }

    fn suite_or_retour(&mut self, command: Command, ctx: &NavContext<'_>) -> Outcome {
        match self.reader_page {
            // FIN DE L'EDITION: nothing comes after it; RETOUR brings the last entry back.
            ReaderPage::EndOfEdition => {
                if command == Command::PreviousEntry {
                    self.reader_page = ReaderPage::Article;
                }
                Outcome::handled(Vec::new())
            }
            ReaderPage::Article => {
                if command == Command::PreviousEntry {
                    Outcome::not_handled()
                } else {
                    self.suite_from_the_last_entry(command == Command::Advance, ctx)
                }
            }
            // Leaving P100 or P899 brings the article back, and that first press is all it
            // does: the lists would otherwise move on in the same press, with no reader on
            // screen to scroll, and skip the entry unseen.
            ReaderPage::Index | ReaderPage::Guide => {
                if !ctx.can_navigate_entries {
                    return Outcome::not_handled();
                }
                self.reader_page = ReaderPage::Article;
                Outcome {
                    handled: ctx.selected_entry.is_some(),
                    effects: Vec::new(),
                }
            }
        }
    }

    /// SUITE on P104's last entry: `j` lands on FIN DE L'EDITION at once; Space scrolls the
    /// article first, as everywhere, and lands there at its bottom. Not handled anywhere
    /// else, so the lists move on as usual.
    fn suite_from_the_last_entry(&mut self, advancing: bool, ctx: &NavContext<'_>) -> Outcome {
        let last = match (ctx.selected_entry, ctx.last_edition_entry) {
            (Some((id, _)), Some(last))
                if id == last && ctx.selection == Selection::Source(Source::Journal) =>
            {
                id
            }
            _ => return Outcome::not_handled(),
        };
        if !advancing {
            return self.land_on_end(ctx.selected_entry);
        }
        if self.advancing_to_end {
            return Outcome::handled(Vec::new());
        }
        self.advancing_to_end = true;
        Outcome::handled(vec![Effect::PageDownThenEnd(last)])
    }
}

/// What a line of a page does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineRole {
    Blank,
    Text,
    Header,
    Message,
    End,
    Title,
    Progress,
    Heading(String),
    Entry(i64),
    Overflow {
        feed: FeedId,
        page: Option<u16>,
    },
    Feed(FeedId),
    Jump(u16),
    Pair {
        left: u16,
        right: Option<u16>,
        half: usize,
    },
    Cadence(usize),
    Theme(usize),
}

/// A row of a page: its cells, and what a click on it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub id: String,
    pub role: LineRole,
    pub cells: CellLine,
    pub double_height: bool,
    pub selected: bool,
}

impl Line {
    pub fn new(id: impl Into<String>, role: LineRole, cells: CellLine) -> Line {
        Line {
            id: id.into(),
            role,
            cells,
            double_height: false,
            selected: false,
        }
    }

    pub fn text(&self) -> String {
        self.cells.text()
    }

    /// `entry:<id>`, `overflow:<feed>:<page|->`, `feed:<id>`, `jump:<n>`,
    /// `pair:<left>:<right|->:<half>`, `cadence:<i>`, `theme:<i>`, else `""`.
    pub fn action(&self) -> String {
        let page = |page: Option<u16>| page.map_or("-".to_string(), |p| p.to_string());
        match &self.role {
            LineRole::Entry(id) => format!("entry:{id}"),
            LineRole::Overflow { feed, page: p } => format!("overflow:{feed}:{}", page(*p)),
            LineRole::Feed(id) => format!("feed:{id}"),
            LineRole::Jump(number) => format!("jump:{number}"),
            LineRole::Pair { left, right, half } => {
                format!("pair:{left}:{}:{half}", page(*right))
            }
            LineRole::Cadence(index) => format!("cadence:{index}"),
            LineRole::Theme(index) => format!("theme:{index}"),
            LineRole::Blank
            | LineRole::Text
            | LineRole::Header
            | LineRole::Message
            | LineRole::End
            | LineRole::Title
            | LineRole::Progress
            | LineRole::Heading(_) => String::new(),
        }
    }

    /// `id`, flags (`d` double height, `s` selected), action and the encoded cells, joined by
    /// U+001D (never in fitted text).
    pub fn encode(&self) -> String {
        let flags = format!(
            "{}{}",
            if self.double_height { "d" } else { "" },
            if self.selected { "s" } else { "" }
        );
        [
            self.id.as_str(),
            &flags,
            &self.action(),
            &self.cells.encode(),
        ]
        .join("\u{1d}")
    }
}

pub fn encode_rows(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::encode).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::sources::{GardenScope, rhythm_title};
    use crate::teletext::{CellSpan, Color};
    use crate::test_db::{feed, paged_snapshot};
    use ressac_core::store::pages::RESERVED_PAGES;

    fn src(source: Source) -> Selection {
        Selection::Source(source)
    }

    #[test]
    fn reserved_pages_come_first_with_their_counts() {
        let mut snapshot = paged_snapshot();
        snapshot.counts.journal = 43;
        snapshot.counts.cadence = BTreeMap::from([
            (CadenceGroup::Daily, 34),
            (CadenceGroup::Weekly, 7),
            (CadenceGroup::Monthly, 2),
        ]);
        let directory = PageDirectory::new(&snapshot);
        let rows = &directory.rows()[..14];
        assert_eq!(
            rows.iter().map(|r| r.number).collect::<Vec<_>>(),
            [
                100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899
            ]
        );
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            [
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
                "Guide"
            ]
        );
        assert_eq!(
            rows.iter().map(|r| r.unread).collect::<Vec<_>>(),
            [0, 10, 1, 4, 43, 34, 7, 2, 0, 0, 7, 0, 0, 0]
        );
        assert!(rows.iter().all(|r| r.reserved && r.depth == 0));
    }

    #[test]
    fn folders_and_feeds_follow_in_display_order_with_their_depth() {
        let directory = PageDirectory::new(&paged_snapshot());
        let rows = &directory.rows()[14..];
        assert_eq!(
            rows.iter().map(|r| r.number).collect::<Vec<_>>(),
            [200, 201, 400, 401, 402, 403, 110]
        );
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            [
                "Blogs",
                "xkcd.com",
                "Infos",
                "Daring Fireball",
                "Science",
                "Center for Humane Technology",
                "Loose feed"
            ]
        );
        assert_eq!(
            rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
            [0, 1, 0, 1, 1, 2, 0]
        );
        assert_eq!(
            rows.iter().map(|r| r.unread).collect::<Vec<_>>(),
            [3, 3, 3, 2, 1, 1, 0]
        ); // folders sum their subtree
        assert!(rows.iter().all(|r| !r.reserved));
    }

    /// The app and `ressac page list` agree (core `RESERVED_PAGES`), and each rhythm page is
    /// its group's.
    #[test]
    fn reserved_pages_match_the_core_and_the_rhythm_groups() {
        assert_eq!(ReservedPage::ALL.map(ReservedPage::number), RESERVED_PAGES);
        for (i, group) in CadenceGroup::ALL.into_iter().enumerate() {
            let page = ReservedPage::from_number(105 + i as u16).unwrap();
            assert_eq!(page.cadence_group(), Some(group));
            assert_eq!(page.title(), rhythm_title(group));
            assert_eq!(
                page.destination(),
                PageDestination::Source(src(Source::Cadence(group)))
            );
            assert!(!page.is_garden());
        }
        assert!(ReservedPage::Journal.cadence_group().is_none());
        assert!(ReservedPage::Unread.cadence_group().is_none());
        assert_eq!(
            ReservedPage::Journal.destination(),
            PageDestination::Source(src(Source::Journal))
        );
        assert!(!ReservedPage::Journal.is_garden());
        assert_eq!(
            ReservedPage::ALL.iter().filter(|p| p.is_garden()).count(),
            3
        );
        assert_eq!(ReservedPage::from_number(437), None);
    }

    #[test]
    fn numbers_lead_to_their_destinations() {
        let d = PageDirectory::new(&paged_snapshot());
        use PageDestination::{Guide, Index};
        assert_eq!(d.destination(100), Some(Index));
        assert_eq!(d.destination(899), Some(Guide));
        for (number, selection) in [
            (101, src(Source::Unread)),
            (102, src(Source::Today)),
            (103, src(Source::Starred)),
            (104, src(Source::Journal)),
            (105, src(Source::Cadence(CadenceGroup::Daily))),
            (106, src(Source::Cadence(CadenceGroup::Weekly))),
            (107, src(Source::Cadence(CadenceGroup::Monthly))),
            (108, src(Source::Cadence(CadenceGroup::Rare))),
            (109, src(Source::Cadence(CadenceGroup::Silent))),
            (800, Selection::Garden(GardenScope::NeedsTriage)),
            (801, Selection::Garden(GardenScope::All)),
            (802, Selection::Garden(GardenScope::Cemetery)),
            (401, src(Source::Feed(8))),
            (402, src(Source::Folder(5))),
            (110, src(Source::Feed(13))),
        ] {
            assert_eq!(
                d.destination(number),
                Some(PageDestination::Source(selection)),
                "{number}"
            );
        }
        for unknown in [0, 7, 99, 437, 799, 900, 999] {
            assert_eq!(d.destination(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn sources_and_feeds_find_their_numbers() {
        let d = PageDirectory::new(&paged_snapshot());
        assert_eq!(d.number_of(&src(Source::Unread)), Some(101));
        assert_eq!(d.number_of(&src(Source::Journal)), Some(104));
        assert_eq!(
            d.number_of(&src(Source::Cadence(CadenceGroup::Rare))),
            Some(108)
        );
        assert_eq!(
            d.number_of(&Selection::Garden(GardenScope::Cemetery)),
            Some(802)
        );
        assert_eq!(d.number_of(&src(Source::Folder(4))), Some(400));
        assert_eq!(d.number_of(&src(Source::Feed(10))), Some(403));
        assert_eq!(d.number_of(&src(Source::Feed(12))), None); // not numbered yet
        assert_eq!(d.page_of_feed(8), Some(401));
        assert_eq!(d.page_of_feed(12), None);
        assert_eq!(d.page_of_feed(999), None);
        assert_eq!(d, PageDirectory::new(&paged_snapshot()));
    }

    #[test]
    fn a_number_colliding_with_a_reserved_page_is_ignored() {
        let mut snapshot = paged_snapshot();
        snapshot.tree[2] = feed(13, "Loose feed", Some(101), 0); // only a corrupt database could hold this
        let d = PageDirectory::new(&snapshot);
        assert_eq!(
            d.destination(101),
            Some(PageDestination::Source(src(Source::Unread)))
        );
        assert_eq!(d.rows().iter().filter(|r| r.number == 101).count(), 1);
        assert_eq!(d.number_of(&src(Source::Feed(13))), None);
        // A corrupt database again: P104 stays the Journal.
        let SourceNodeKind::Folder { .. } = snapshot.tree[1].kind else {
            unreachable!()
        };
        snapshot.tree[1].children[1].children[0].page = Some(104);
        let again = PageDirectory::new(&snapshot);
        assert_eq!(
            again.destination(104),
            Some(PageDestination::Source(src(Source::Journal)))
        );
        assert_eq!(again.number_of(&src(Source::Feed(10))), None);
    }

    #[test]
    fn is_root_folder() {
        let d = PageDirectory::new(&paged_snapshot());
        let row = |n: u16| d.rows().iter().find(|r| r.number == n).unwrap().clone();
        assert!(d.is_root_folder(&row(400)));
        assert!(d.is_root_folder(&row(200)));
        assert!(!d.is_root_folder(&row(402))); // a subfolder
        assert!(!d.is_root_folder(&row(201))); // a feed
        assert!(!d.is_root_folder(&row(110))); // a root feed
        assert!(!d.is_root_folder(&row(100)));
    }

    #[test]
    fn the_directory_is_rebuilt_only_when_the_snapshot_changes() {
        let mut cache = DirectoryCache::default();
        let snapshot = paged_snapshot();
        let first = cache.get(&snapshot).clone();
        assert_eq!(cache.get(&snapshot), &first);
        assert_eq!(cache.builds(), 1);
        let mut more = paged_snapshot();
        more.counts.unread += 1;
        let second = cache.get(&more).clone();
        assert_eq!(cache.builds(), 2);
        assert_ne!(second, first);
        assert_eq!(cache.get(&more), &second);
        assert_eq!(cache.builds(), 2);
    }

    #[test]
    fn the_third_digit_completes_the_number() {
        let mut entry = PageEntry::default();
        assert!(!entry.is_active() && entry.display().is_empty());
        assert_eq!(entry.type_digit(4), EntryOutcome::Pending);
        assert!(entry.is_active() && entry.display() == "4__");
        assert_eq!(entry.type_digit(1), EntryOutcome::Pending);
        assert_eq!(entry.display(), "41_");
        assert_eq!(entry.type_digit(2), EntryOutcome::Complete(412));
        assert!(!entry.is_active() && entry.display().is_empty() && entry.digits().is_empty());
        assert_eq!(entry.type_digit(8), EntryOutcome::Pending); // the next digit starts a new number
        assert_eq!(entry.display(), "8__");
    }

    #[test]
    fn correction_erases_the_last_digit() {
        let mut entry = PageEntry::default();
        entry.correct(); // nothing typed: nothing happens
        assert!(!entry.is_active());
        entry.type_digit(4);
        entry.type_digit(1);
        entry.correct();
        assert_eq!(entry.display(), "4__");
        entry.correct();
        assert!(!entry.is_active());
        entry.type_digit(8);
        entry.type_digit(0);
        assert_eq!(entry.type_digit(1), EntryOutcome::Complete(801));
    }

    #[test]
    fn cancel_empties_the_entry() {
        let mut entry = PageEntry::default();
        entry.type_digit(4);
        entry.type_digit(1);
        entry.cancel();
        assert_eq!(entry, PageEntry::default());
        assert_eq!(entry.type_digit(1), EntryOutcome::Pending);
    }

    #[test]
    fn leading_zeros_and_stray_values() {
        let mut entry = PageEntry::default();
        assert_eq!(entry.type_digit(12), EntryOutcome::Pending); // not a digit: ignored
        assert!(!entry.is_active());
        entry.type_digit(0);
        entry.type_digit(4);
        assert_eq!(entry.display(), "04_");
        assert_eq!(entry.type_digit(2), EntryOutcome::Complete(42)); // no page has it: NOT FOUND
    }

    // ----- AntiopeNav -----

    const EDITION_LAST: i64 = 77;

    fn ctx(directory: &PageDirectory, selection: Selection, now: Instant) -> NavContext<'_> {
        NavContext {
            directory,
            selection,
            selected_entry: None,
            last_edition_entry: None,
            journal_retries: false,
            can_refresh: true,
            can_navigate_entries: true,
            now,
        }
    }

    fn dial(nav: &mut AntiopeNav, number: &str, c: &NavContext<'_>) -> Outcome {
        let mut last = Outcome::default();
        for d in number.bytes().map(|b| b - b'0') {
            last = nav.handle(Command::PageDigit(d), c);
            assert!(last.handled);
        }
        last
    }

    fn handled(effects: Vec<Effect>) -> Outcome {
        Outcome {
            handled: true,
            effects,
        }
    }

    const NOT_HANDLED: Outcome = Outcome {
        handled: false,
        effects: Vec::new(),
    };

    #[test]
    fn a_typed_page_selects_its_source_and_focuses_the_index() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        nav.set_column(AntiopeColumn::Reader);
        assert_eq!(dial(&mut nav, "40", &c), handled(vec![]));
        assert_eq!(nav.entry().display(), "40_");
        assert_eq!(nav.column(), AntiopeColumn::Reader); // nothing happens before the third digit
        assert_eq!(
            nav.handle(Command::PageDigit(1), &c),
            handled(vec![Effect::Select(src(Source::Feed(8)))])
        );
        assert_eq!(nav.column(), AntiopeColumn::Index);
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        let after = ctx(&d, src(Source::Feed(8)), now);
        assert_eq!(nav.current_page(&d, &after.selection), Some(401));
        assert!(!nav.entry().is_active() && nav.not_found(now).is_none());
    }

    #[test]
    fn correction_and_cancel_edit_the_entry() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        assert_eq!(nav.handle(Command::PageCorrect, &c), NOT_HANDLED); // nothing typed: the key goes on
        assert_eq!(nav.handle(Command::PageCancel, &c), NOT_HANDLED);
        dial(&mut nav, "41", &c);
        assert_eq!(nav.handle(Command::PageCorrect, &c), handled(vec![]));
        assert_eq!(nav.entry().display(), "4__");
        assert_eq!(nav.handle(Command::PageCancel, &c), handled(vec![]));
        assert!(!nav.entry().is_active() && nav.entry().display().is_empty());
    }

    /// Review Focus 3.
    #[test]
    fn an_unknown_page_shows_not_found_for_two_seconds() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        assert_eq!(d.destination(437), None);
        assert_eq!(dial(&mut nav, "437", &c), handled(vec![Effect::NotFound]));
        assert_eq!(nav.not_found(now), Some(437));
        assert_eq!(nav.current_page(&d, &c.selection), Some(101)); // selection unchanged
        assert!(!nav.entry().is_active());
        assert_eq!(nav.not_found(now + Duration::from_millis(1999)), Some(437));
        assert_eq!(nav.not_found(now + Duration::from_secs(2)), None);
    }

    #[test]
    fn a_new_digit_clears_not_found_at_once() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        dial(&mut nav, "437", &c);
        assert_eq!(nav.not_found(now), Some(437));
        dial(&mut nav, "1", &c);
        assert!(nav.not_found(now).is_none() && nav.entry().display() == "1__");
        dial(&mut nav, "99", &c);
        assert_eq!(nav.not_found(now), Some(199));
        assert_eq!(nav.not_found(now + Duration::from_millis(1999)), Some(199));
        assert_eq!(nav.not_found(now + NOT_FOUND_FOR), None);
    }

    #[test]
    fn reserved_pages_open_their_destination() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let today = nav.jump(102, &ctx(&d, src(Source::Unread), now));
        assert_eq!(today, handled(vec![Effect::Select(src(Source::Today))]));
        let triage = Selection::Garden(GardenScope::NeedsTriage);
        assert_eq!(
            nav.jump(800, &ctx(&d, src(Source::Today), now)),
            handled(vec![Effect::Select(triage)])
        );
        let c = ctx(&d, triage, now);
        assert_eq!(nav.jump(100, &c), handled(vec![])); // P100 keeps the selection
        assert_eq!(nav.reader_page(), ReaderPage::Index);
        assert_eq!(nav.current_page(&d, &triage), Some(100));
        assert_eq!(nav.handle(Command::Guide, &c), handled(vec![]));
        assert_eq!(
            (nav.reader_page(), nav.current_page(&d, &triage)),
            (ReaderPage::Guide, Some(899))
        );
        assert_eq!(nav.handle(Command::Sommaire, &c), handled(vec![]));
        assert_eq!(nav.reader_page(), ReaderPage::Index);
        assert_eq!(
            nav.jump(103, &c),
            handled(vec![Effect::Select(src(Source::Starred))])
        );
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn typing_the_rhythm_pages_selects_their_sources() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        for (digits, source) in [
            ("104", Source::Journal),
            ("105", Source::Cadence(CadenceGroup::Daily)),
            ("109", Source::Cadence(CadenceGroup::Silent)),
        ] {
            let out = dial(&mut nav, digits, &ctx(&d, src(Source::Unread), now));
            assert_eq!(out, handled(vec![Effect::Select(src(source))]));
            assert_eq!(nav.reader_page(), ReaderPage::Article);
            assert_eq!(nav.current_page(&d, &src(source)), digits.parse().ok());
        }
    }

    #[test]
    fn suite_and_retour_are_the_lists_on_an_article() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut c = ctx(&d, src(Source::Unread), now);
        c.selected_entry = Some((5, false));
        let mut nav = AntiopeNav::default();
        for command in [Command::NextEntry, Command::Advance, Command::PreviousEntry] {
            assert_eq!(nav.handle(command, &c), NOT_HANDLED, "{command:?}");
        }
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn suite_from_a_reserved_page_only_brings_the_article_back() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut c = ctx(&d, src(Source::Unread), now);
        c.selected_entry = Some((5, false));
        let mut nav = AntiopeNav::default();
        for page in [ReaderPage::Guide, ReaderPage::Index] {
            for command in [Command::Advance, Command::NextEntry, Command::PreviousEntry] {
                nav.jump(if page == ReaderPage::Guide { 899 } else { 100 }, &c);
                assert_eq!(nav.reader_page(), page);
                assert_eq!(nav.handle(command, &c), handled(vec![]), "{command:?}");
                assert_eq!(nav.reader_page(), ReaderPage::Article);
                assert_eq!(nav.handle(command, &c), NOT_HANDLED); // the article is back
            }
        }
        // Without an entry the press still brings the article back, but the key goes on.
        nav.jump(899, &ctx(&d, src(Source::Unread), now));
        let bare = ctx(&d, src(Source::Unread), now);
        assert_eq!(nav.handle(Command::NextEntry, &bare), NOT_HANDLED);
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        // And when the lists cannot move at all, nothing changes.
        nav.jump(899, &bare);
        let mut stuck = ctx(&d, src(Source::Unread), now);
        stuck.can_navigate_entries = false;
        assert_eq!(nav.handle(Command::NextEntry, &stuck), NOT_HANDLED);
        assert_eq!(nav.reader_page(), ReaderPage::Guide);
    }

    #[test]
    fn repetition_refreshes_or_retries_the_edition() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let feed3 = ctx(&d, src(Source::Feed(3)), now);
        assert_eq!(
            nav.handle(Command::Repetition, &feed3),
            handled(vec![Effect::Refresh])
        );
        let mut cannot = ctx(&d, src(Source::Feed(3)), now);
        cannot.can_refresh = false;
        assert_eq!(nav.handle(Command::Repetition, &cannot), NOT_HANDLED);
        let mut journal = ctx(&d, src(Source::Journal), now);
        journal.journal_retries = true;
        assert_eq!(
            nav.handle(Command::Repetition, &journal),
            handled(vec![Effect::RetryEdition])
        );
        journal.can_refresh = false; // a retry never needs the run to be free
        assert_eq!(
            nav.handle(Command::Repetition, &journal),
            handled(vec![Effect::RetryEdition])
        );
        let mut other = ctx(&d, src(Source::Unread), now);
        other.journal_retries = true; // only P104 retries
        assert_eq!(
            nav.handle(Command::Repetition, &other),
            handled(vec![Effect::Refresh])
        );
        let healthy = ctx(&d, src(Source::Journal), now);
        assert_eq!(
            nav.handle(Command::Repetition, &healthy),
            handled(vec![Effect::Refresh])
        );
    }

    #[test]
    fn images_toggle_only_on_an_article_with_an_entry() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let mut c = ctx(&d, src(Source::Unread), now);
        assert_eq!(nav.handle(Command::ToggleImages, &c), NOT_HANDLED); // no entry selected
        c.selected_entry = Some((5, true));
        assert_eq!(
            nav.handle(Command::ToggleImages, &c),
            handled(vec![Effect::ToggleImages])
        );
        nav.handle(Command::Guide, &c);
        assert_eq!(nav.handle(Command::ToggleImages, &c), NOT_HANDLED); // P899 has no images
    }

    #[test]
    fn the_index_arrows_walk_the_pages() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let at_101 = ctx(&d, src(Source::Unread), now);
        assert_eq!(
            nav.handle(Command::PreviousIndexRow, &at_101),
            handled(vec![])
        ); // 101 to 100
        assert_eq!(
            (nav.reader_page(), nav.current_page(&d, &at_101.selection)),
            (ReaderPage::Index, Some(100))
        );
        assert_eq!(
            nav.handle(Command::PreviousIndexRow, &at_101),
            handled(vec![])
        ); // the first page: spent
        assert_eq!(nav.current_page(&d, &at_101.selection), Some(100));
        assert_eq!(
            nav.handle(Command::NextIndexRow, &at_101),
            handled(vec![Effect::Select(src(Source::Unread))])
        );
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        assert_eq!(
            nav.handle(Command::NextIndexRow, &at_101),
            handled(vec![Effect::Select(src(Source::Today))])
        );
        assert_eq!(nav.column(), AntiopeColumn::Index);
        // The last page is spent too, and a selection without a page starts at the top.
        let last = ctx(&d, src(Source::Folder(4)), now);
        let mut tail = AntiopeNav::default();
        tail.jump(110, &last);
        assert_eq!(
            tail.handle(Command::NextIndexRow, &ctx(&d, src(Source::Feed(13)), now)),
            handled(vec![])
        );
        let nameless = ctx(&d, src(Source::Feed(12)), now);
        assert_eq!(
            AntiopeNav::default().handle(Command::NextIndexRow, &nameless),
            handled(vec![]) // first row is P100: the reader shows it, no Select
        );
    }

    #[test]
    fn columns_move_left_and_right() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        assert_eq!(nav.handle(Command::PreviousColumn, &c), NOT_HANDLED); // the index is the first column
        assert_eq!(
            nav.handle(Command::NextColumn, &c),
            handled(vec![Effect::FocusEntries])
        );
        assert_eq!(nav.column(), AntiopeColumn::Entries);
        assert_eq!(nav.handle(Command::NextColumn, &c), handled(vec![]));
        assert_eq!(nav.column(), AntiopeColumn::Reader);
        assert_eq!(nav.handle(Command::NextColumn, &c), NOT_HANDLED); // the reader is the last
        assert_eq!(
            nav.handle(Command::PreviousColumn, &c),
            handled(vec![Effect::FocusEntries])
        );
        assert_eq!(nav.column(), AntiopeColumn::Entries);
        assert_eq!(
            AntiopeColumn::parse("entries"),
            Some(AntiopeColumn::Entries)
        );
        assert_eq!(AntiopeColumn::parse("nope"), None);
        assert_eq!(
            [
                AntiopeColumn::Index,
                AntiopeColumn::Entries,
                AntiopeColumn::Reader
            ]
            .map(AntiopeColumn::as_str),
            ["index", "entries", "reader"]
        );
        assert_eq!(
            [
                ReaderPage::Article,
                ReaderPage::Index,
                ReaderPage::Guide,
                ReaderPage::EndOfEdition
            ]
            .map(ReaderPage::as_str),
            ["article", "index", "guide", "end"]
        );
    }

    fn on_the_last_entry<'a>(d: &'a PageDirectory, now: Instant, read: bool) -> NavContext<'a> {
        let mut c = ctx(d, src(Source::Journal), now);
        c.selected_entry = Some((EDITION_LAST, read));
        c.last_edition_entry = Some(EDITION_LAST);
        c
    }

    #[test]
    fn suite_from_the_last_edition_entry_lands_on_the_end_page() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let mut before = on_the_last_entry(&d, now, false);
        before.selected_entry = Some((EDITION_LAST - 1, false)); // not the last entry: the list moves on
        assert_eq!(nav.handle(Command::NextEntry, &before), NOT_HANDLED);
        let c = on_the_last_entry(&d, now, false);
        assert_eq!(
            nav.handle(Command::NextEntry, &c),
            handled(vec![Effect::MarkRead(EDITION_LAST)])
        );
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        assert_eq!(nav.current_page(&d, &c.selection), Some(104));
        assert_eq!(nav.handle(Command::NextEntry, &c), handled(vec![])); // nothing after the end
        assert_eq!(nav.handle(Command::Advance, &c), handled(vec![]));
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        assert_eq!(nav.handle(Command::PreviousEntry, &c), handled(vec![])); // RETOUR
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        // Space at the bottom: page down first, once, then the end.
        assert_eq!(
            nav.handle(Command::Advance, &c),
            handled(vec![Effect::PageDownThenEnd(EDITION_LAST)])
        );
        assert_eq!(nav.handle(Command::Advance, &c), handled(vec![]));
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        nav.advance_finished();
        assert_eq!(
            nav.handle(Command::Advance, &c),
            handled(vec![Effect::PageDownThenEnd(EDITION_LAST)])
        );
        nav.advance_finished();
        assert_eq!(nav.land_on_end(Some((EDITION_LAST, true))), handled(vec![])); // already read
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        let mut fresh = AntiopeNav::default();
        assert_eq!(
            fresh.land_on_end(Some((EDITION_LAST, false))),
            handled(vec![Effect::MarkRead(EDITION_LAST)])
        );
        assert_eq!(fresh.land_on_end(None), handled(vec![]));
    }

    #[test]
    fn elsewhere_the_last_entry_is_the_lists() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let mut c = on_the_last_entry(&d, now, false);
        c.selection = src(Source::Unread);
        assert_eq!(nav.handle(Command::NextEntry, &c), NOT_HANDLED);
        assert_eq!(nav.handle(Command::Advance, &c), NOT_HANDLED);
        assert_eq!(nav.reader_page(), ReaderPage::Article);
        let mut no_edition = on_the_last_entry(&d, now, false);
        no_edition.last_edition_entry = None;
        assert_eq!(nav.handle(Command::NextEntry, &no_edition), NOT_HANDLED);
    }

    #[test]
    fn a_jump_leaves_the_end_page() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let mut nav = AntiopeNav::default();
        let c = on_the_last_entry(&d, now, true);
        assert!(nav.handle(Command::NextEntry, &c).handled);
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        assert_eq!(
            nav.jump(101, &c),
            handled(vec![Effect::Select(src(Source::Unread))])
        );
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn other_commands_are_never_antiopes() {
        let (d, now) = (PageDirectory::new(&paged_snapshot()), Instant::now());
        let c = ctx(&d, src(Source::Unread), now);
        let mut nav = AntiopeNav::default();
        for command in [
            Command::ToggleRead,
            Command::ToggleStar,
            Command::FocusEntryList,
            Command::FocusSidebar,
        ] {
            assert_eq!(nav.handle(command, &c), NOT_HANDLED, "{command:?}");
        }
    }

    #[test]
    fn repetition_retries_only_an_edition_that_could_not_be_built() {
        use journal::Phase::*;
        for (phase, retries) in [
            (Offline, true),
            (Failed("boom".into()), true),
            (Due, true),
            (Idle, false),
            (Building { done: 1, total: 4 }, false),
        ] {
            assert_eq!(retries_journal(&phase), retries, "{phase:?}");
        }
    }

    #[test]
    fn outcomes_encode_for_qml() {
        let out = Outcome {
            handled: true,
            effects: vec![Effect::Select(src(Source::Feed(8))), Effect::NotFound],
        };
        assert_eq!(out.encode(), "handled;select:feed:8;notFound");
        assert_eq!(NOT_HANDLED.encode(), "");
        assert_eq!(handled(vec![]).encode(), "handled");
        assert_eq!(
            handled(vec![
                Effect::RetryEdition,
                Effect::Refresh,
                Effect::ToggleImages,
                Effect::FocusEntries,
                Effect::MarkRead(9),
                Effect::PageDownThenEnd(10),
            ])
            .encode(),
            "handled;retry;refresh;images;focusEntries;markRead:9;pageDownThenEnd:10"
        );
    }

    #[test]
    fn lines_carry_their_action_and_encode_for_the_painter() {
        let cells = CellLine::new(vec![CellSpan::new("hi", 2, Color::Cyan)]);
        let line = |role| Line::new("r", role, cells.clone());
        assert_eq!(
            [
                LineRole::Blank,
                LineRole::Text,
                LineRole::Entry(7),
                LineRole::Overflow {
                    feed: 3,
                    page: Some(401)
                },
                LineRole::Overflow {
                    feed: 3,
                    page: None
                },
                LineRole::Feed(8),
                LineRole::Jump(104),
                LineRole::Pair {
                    left: 105,
                    right: Some(106),
                    half: 1
                },
                LineRole::Pair {
                    left: 107,
                    right: None,
                    half: 0
                },
                LineRole::Cadence(2),
                LineRole::Theme(0),
                LineRole::Heading("RHYTHM".into()),
            ]
            .map(|role| line(role).action()),
            [
                "",
                "",
                "entry:7",
                "overflow:3:401",
                "overflow:3:-",
                "feed:8",
                "jump:104",
                "pair:105:106:1",
                "pair:107:-:0",
                "cadence:2",
                "theme:0",
                ""
            ]
        );
        let mut plain = line(LineRole::Entry(7));
        assert_eq!(plain.text(), "hi");
        assert_eq!(plain.encode(), "r\u{1d}\u{1d}entry:7\u{1d}60hi");
        plain.double_height = true;
        plain.selected = true;
        assert_eq!(plain.encode(), "r\u{1d}ds\u{1d}entry:7\u{1d}60hi");
        assert_eq!(encode_rows(&[plain.clone(), plain]).len(), 2);
    }
}
