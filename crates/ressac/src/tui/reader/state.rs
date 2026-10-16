//! The reader's state and keys (spec §5). Pure: no terminal, no database, no clock. Keys and
//! events come in, `Command`s go out to the event loop (phase 5b), which runs them and answers
//! with `Event`s; the same pattern as the garden's `GardenState`.

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ressac_core::garden::GardenRow;
use ressac_core::model::{CadenceGroup, FeedId};
use ressac_core::store::pages::PageRow;
use ressac_core::store::{
    EntryDetail, EntryPage, EntryRow, JournalView, ListCursor, ListFilter, Source, SourcesSnapshot,
};

use crate::output::printable;
use crate::tui::state::{self as garden_state, GardenState};

use super::html::{RenderedArticle, is_web_url, render_html};
use super::journal::{self, END_LINE, JournalLine, JournalPhase, OFFLINE};
use super::layout::{Column, LayoutClass, Look, columns, layout_class, text_height, text_width};
use super::pages::{
    AntiopeNav, NavContext, NavOutcome, PageDestination, PageDirectory, ReaderPage,
};
use super::rhythm::{self, RhythmInfo};
use super::settings::TerminalSettings;
use super::sidebar::{self, SidebarRow};

/// Lines of the reader column taken by the article header (title; feed, author, date; state).
pub const READER_HEADER_LINES: usize = 3;

pub const NO_MORE_UNREAD: &str = "No more unread entries here.";
pub const NO_ENTRY: &str = "No entry selected.";
pub const NO_LINK: &str = "This entry has no link.";
pub const NO_FEED: &str = "No feed selected.";
pub const FETCHING_FULL: &str = "Fetching the full article…";
pub const OPEN_PROMPT: &str = "Open: Enter for the article, 1-9 for a link, Esc to cancel.";

/// The rhythm menu, in the app's order; `None` is Auto.
pub const RHYTHM_CHOICES: [Option<CadenceGroup>; 6] = [
    None,
    Some(CadenceGroup::Daily),
    Some(CadenceGroup::Weekly),
    Some(CadenceGroup::Monthly),
    Some(CadenceGroup::Rare),
    Some(CadenceGroup::Silent),
];

/// What the event loop must do (phase 5b runs them).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadSources,
    /// The teletext page numbers (`Ressac::pages`): the Antiope index and its destinations.
    LoadPages,
    /// A page of a list. `after: None` is a fresh list (it replaces the rows), `Some` the next
    /// page (it is appended). The runner echoes `request` in the `Event::Entries` it answers
    /// with; the state ignores answers older than its current request.
    LoadEntries {
        source: Source,
        filter: ListFilter,
        after: Option<ListCursor>,
        request: u64,
    },
    LoadJournal,
    /// A search in `source`. The runner applies `filter` (only the unread hits under
    /// `ListFilter::Unread`), passes `after` to `Store::search_entries` (`Some` is the next page
    /// of the hits, appended) and echoes `request` in its `Event::Entries`.
    Search {
        source: Source,
        text: String,
        filter: ListFilter,
        after: Option<ListCursor>,
        request: u64,
    },
    LoadEntry(i64),
    SetRead(i64, bool),
    SetStarred(i64, bool),
    MarkAllRead(Source),
    Extract(i64),
    BuildEdition,
    Refresh(Option<Source>),
    OpenUrl(String),
    SetCadence(FeedId, Option<CadenceGroup>),
    /// The feed's rhythm line (`Event::Cadence`): asked whenever the selected garden row or the
    /// feed source in view changes.
    LoadCadence(FeedId),
    /// The garden's rows (a garden page is showing or about to).
    LoadGarden,
    /// A garden command from a garden page; the runner answers with the fresh rows, the message
    /// to show and the sources.
    Garden(garden_state::Command),
    /// Persist `state.settings` with `TerminalSettings::save`. The caller ignores the
    /// `io::Result` (spec §5: writing never blocks or fails reading): an unwritable file only
    /// means the choice is forgotten at the next start.
    SaveSettings,
    Quit,
}

/// What the event loop reports back.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Sources(SourcesSnapshot),
    Pages(Vec<PageRow>),
    /// The answer to a `Command::LoadEntries` or `Command::Search`: `request` is echoed from the
    /// command, so a late answer to a list the reader has left is recognised and dropped.
    Entries {
        source: Source,
        page: EntryPage,
        appended: bool,
        request: u64,
    },
    Journal(Option<JournalView>),
    GardenRows(Vec<GardenRow>),
    /// The line a garden command leaves on the garden page (`None` clears it).
    GardenMessage(Option<String>),
    /// A feed's rhythm, from `v_cadence`; kept when it is the selected feed's.
    Cadence(RhythmInfo),
    /// Boxed: far larger than the other variants (`clippy::large_enum_variant`).
    Entry(Box<EntryDetail>),
    ImageReady {
        url: String,
    },
    ImageFailed {
        url: String,
    },
    Progress {
        label: String,
        done: usize,
        total: usize,
    },
    JobDone(JobOutcome),
    /// Another process changed the database (`data_version`), or a bulk write of ours did.
    DataChanged,
    /// This reader's own single-entry write (`SetRead`, `SetStarred`): the row was updated
    /// locally, only the counts follow.
    OwnWrite,
    Tick {
        now: DateTime<Utc>,
        local: DateTime<FixedOffset>,
    },
    Resize {
        width: u16,
        height: u16,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobOutcome {
    Refreshed,
    EditionBuilt,
    /// Also when another process built today's edition meanwhile: the core's due re-check
    /// returns NotDue in both cases.
    EditionNotDue,
    /// The edition job's own offline and failure: the Journal's phase follows these two only,
    /// never the `Offline` of a refresh or the `Failed` of a quick command (the executor tags the
    /// long job that ran).
    EditionOffline,
    EditionFailed(String),
    Offline,
    Failed(String),
    Extracted {
        entry_id: i64,
        error: Option<String>,
    },
}

/// The reader's garden destinations (spec §4): Needs triage, All feeds, Cemetery. Not core
/// `Source`s: the core has no garden variant, the reader shows the garden screen instead of an
/// entries list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GardenView {
    NeedsTriage,
    AllFeeds,
    Cemetery,
}

impl GardenView {
    pub const ALL: [GardenView; 3] = [
        GardenView::NeedsTriage,
        GardenView::AllFeeds,
        GardenView::Cemetery,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GardenView::NeedsTriage => "Needs triage",
            GardenView::AllFeeds => "All feeds",
            GardenView::Cemetery => "Cemetery",
        }
    }

    pub(super) fn filter(self) -> garden_state::Filter {
        match self {
            GardenView::NeedsTriage => garden_state::Filter::Queue,
            GardenView::AllFeeds => garden_state::Filter::All,
            GardenView::Cemetery => garden_state::Filter::Cemetery,
        }
    }
}

/// A modal layer over the columns; it takes the next key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    /// After `o`: Enter opens the article, a digit a numbered link.
    LinkPrompt,
    Rhythm {
        feed_id: FeedId,
        choice: usize,
    },
}

pub struct ReaderState {
    pub settings: TerminalSettings,
    pub width: u16,
    pub height: u16,
    pub class: LayoutClass,
    pub focus: Column,
    pub source: Source,
    pub filter: ListFilter,
    pub sources: Option<SourcesSnapshot>,
    /// Every teletext page (reserved, then folders and feeds), empty until the first load.
    pub pages: Vec<PageRow>,
    /// The page directory (`pages` and the counts of `sources`), rebuilt when either arrives.
    pub directory: PageDirectory,
    /// The Antiope look's navigation: the page number being typed, `NOT FOUND`, the page shown
    /// in the reader column (P100, P899, `FIN DE L'EDITION` or the entry).
    pub antiope: AntiopeNav,
    /// First row of P100 or P899 in the reader column.
    pub page_scroll: usize,
    /// Antiope draws the real images instead of the teletext mosaics (`i`).
    pub antiope_real_images: bool,
    pub sidebar: Vec<SidebarRow>,
    pub sidebar_index: usize,
    pub entries: Vec<EntryRow>,
    /// Rows and unread rows of the whole list, loaded or not.
    pub total: usize,
    pub unread: usize,
    pub next: Option<ListCursor>,
    pub journal: Option<JournalView>,
    /// The garden page shown instead of the entries and reader columns, if any; `source` keeps
    /// the last reading source meanwhile. `garden` is built when the rows arrive.
    garden_view: Option<GardenView>,
    garden: Option<GardenState>,
    journal_phase: JournalPhase,
    /// True from `BuildEdition` until the edition job's outcome: no second build meanwhile.
    edition_job: bool,
    /// The 04:00 day of the last tick or of `start`; a change is the rollover.
    last_day: Option<DateTime<Utc>>,
    /// The Journal's cursor is on this feed's overflow row (no entry is selected then).
    journal_overflow: Option<FeedId>,
    /// The rhythm of the feed in view (the selected garden row, a feed source) or of the feed
    /// `c` was pressed on; the line is drawn only when it is the feed in view's.
    pub rhythm: Option<RhythmInfo>,
    /// The feed whose rhythm was last asked for: a change of the feed in view asks again.
    rhythm_feed: Option<FeedId>,
    pub selected: Option<i64>,
    pub detail: Option<EntryDetail>,
    pub article: Option<RenderedArticle>,
    pub show_fulltext: bool,
    /// First article line shown.
    pub scroll: usize,
    pub search: Option<String>,
    pub search_editing: bool,
    pub overlay: Overlay,
    pub message: Option<String>,
    pub progress: Option<(String, usize, usize)>,
    pub now: Option<DateTime<Utc>>,
    /// The local time of the last `Tick` (or `start`): every date shown uses it, never `chrono::Local`.
    pub local: Option<DateTime<FixedOffset>>,
    /// Space reached the end of the loaded rows: carry on once the next page arrives.
    pending_advance: bool,
    /// The generation of the list being shown: bumped by every fresh list load (source, filter,
    /// search, reload), echoed by the runner; older answers are ignored.
    pub request: u64,
    /// A next-page request is out: no second one until it lands or the list changes.
    loading_more: bool,
    /// The row open in the reader and its position when a reload was asked: put back if the
    /// reloaded list no longer holds it (read under an unread filter), so the reader stays put.
    pinned: Option<(usize, EntryRow)>,
    /// The entry whose full-text extraction is running.
    extracting: Option<i64>,
    /// What the last draw measured (the article's height in rows, the visible rows): images and
    /// the links list change both. `None` until a draw, and again whenever the article changes.
    metrics: Option<(usize, u16)>,
}

/// One line of the sources column, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLine {
    pub label: String,
    pub unread: usize,
    pub depth: u16,
    pub selected: bool,
    pub heading: bool,
}

/// The state as the renderers see it: borrowed, read-only, every feed string already through
/// `output::printable` where it is built here (the renderers do the rest).
#[derive(Debug, Clone)]
pub struct ReaderView<'a> {
    pub look: Look,
    pub layout: LayoutClass,
    pub focus: Column,
    pub now_local: DateTime<FixedOffset>,
    pub source: Source,
    pub source_title: String,
    /// A garden page is showing instead of the entries and reader columns.
    pub garden: bool,
    pub source_lines: Vec<SourceLine>,
    pub filter: ListFilter,
    pub search: Option<&'a str>,
    pub entries: &'a [EntryRow],
    pub entry_cursor: Option<usize>,
    pub total: usize,
    pub unread: usize,
    /// The Journal column is showing (the Journal, unfiltered, no search): its rows or, while
    /// there is no edition to show, its placeholder.
    pub journal_mode: bool,
    pub journal: Option<&'a JournalView>,
    pub journal_phase: &'a JournalPhase,
    /// The Journal's rows (empty unless `journal_mode` and an edition is loaded) and the one
    /// under the cursor.
    pub journal_lines: Vec<JournalLine>,
    pub journal_cursor: Option<usize>,
    pub entry: Option<&'a EntryDetail>,
    pub article: Option<&'a RenderedArticle>,
    pub show_fulltext: bool,
    pub reader_scroll: usize,
    pub images: bool,
    pub progress: Option<(&'a str, usize, usize)>,
    pub message: Option<&'a str>,
    pub help: bool,
    /// The search text or the link number is being typed: the bottom line shows how to end it.
    pub editing: bool,
    pub rhythm: Option<(usize, usize)>,
    /// The menu's item that is the feed's current setting (`Auto` when not overridden): marked.
    pub rhythm_current: Option<usize>,
    /// "Rhythm: Daily · median 0.4 d · last entry 2 h ago" for the feed in view: the selected
    /// garden row (not a buried one) or the feed source; `None` until the info arrives.
    pub rhythm_line: Option<String>,
}

/// `Daily`, `Weekly`...: the group's name as the sources column and the menu show it.
pub fn group_label(group: CadenceGroup) -> String {
    let name = group.as_str();
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl ReaderState {
    /// The reader opens on the Journal (spec §6), the entry list focused.
    pub fn new(settings: TerminalSettings, width: u16, height: u16) -> Self {
        ReaderState {
            settings,
            width,
            height,
            class: layout_class(width, height),
            focus: Column::Entries,
            source: Source::Journal,
            filter: ListFilter::All,
            sources: None,
            pages: Vec::new(),
            directory: PageDirectory::default(),
            antiope: AntiopeNav::default(),
            page_scroll: 0,
            antiope_real_images: false,
            sidebar: Vec::new(),
            sidebar_index: 0,
            entries: Vec::new(),
            total: 0,
            unread: 0,
            next: None,
            journal: None,
            garden_view: None,
            garden: None,
            journal_phase: JournalPhase::Idle,
            edition_job: false,
            last_day: None,
            journal_overflow: None,
            rhythm: None,
            rhythm_feed: None,
            selected: None,
            detail: None,
            article: None,
            show_fulltext: false,
            scroll: 0,
            search: None,
            search_editing: false,
            overlay: Overlay::None,
            message: None,
            progress: None,
            now: None,
            local: None,
            pending_advance: false,
            request: 0,
            loading_more: false,
            pinned: None,
            extracting: None,
            metrics: None,
        }
    }

    /// The first commands: the sources, the Journal and its entries, then the edition build (the
    /// core turns it away at once when today's edition exists, so nothing shows as building).
    /// Stores the clock.
    pub fn start(&mut self, now: DateTime<Utc>, local: DateTime<FixedOffset>) -> Vec<Command> {
        self.now = Some(now);
        self.local = Some(local);
        self.last_day = Some(journal::day_of(local));
        let mut commands = vec![
            Command::LoadSources,
            Command::LoadPages,
            Command::LoadJournal,
            self.reload(),
        ];
        commands.extend(self.build_edition());
        commands.extend(self.sync_rhythm());
        commands
    }
    #[cfg(test)]
    pub fn journal_phase(&self) -> &JournalPhase {
        &self.journal_phase
    }
    pub fn journal(&self) -> Option<&JournalView> {
        self.journal.as_ref()
    }

    /// The rhythm of the feed in view, once it has arrived.
    #[cfg(test)]
    pub fn rhythm(&self) -> Option<&RhythmInfo> {
        self.rhythm.as_ref()
    }

    /// The garden page shown instead of the entries and reader columns, if any.
    pub fn garden_view(&self) -> Option<GardenView> {
        self.garden_view
    }

    /// The garden screen of that page, once its rows have arrived.
    pub fn garden(&self) -> Option<&GardenState> {
        self.garden.as_ref()
    }

    /// Choosing a garden page in the sources column: the garden takes the entries and reader
    /// columns and the focus moves to it.
    pub fn open_garden(&mut self, view: GardenView) -> Vec<Command> {
        let mut commands = self.show_garden(view);
        if commands.is_empty() {
            // Already showing it: fresh rows all the same.
            commands.push(Command::LoadGarden);
        }
        self.focus = Column::Entries;
        commands.extend(self.sync_rhythm());
        commands
    }

    /// The garden page in the columns, the focus left where it is (arrowing down the sources
    /// column shows each page as it passes, like a reading source).
    fn show_garden(&mut self, view: GardenView) -> Vec<Command> {
        if let Some(index) = sidebar::index_of_garden(&self.sidebar, view) {
            self.sidebar_index = index;
        }
        self.search = None;
        self.search_editing = false;
        if self.garden_view == Some(view) {
            return Vec::new();
        }
        self.garden_view = Some(view);
        self.garden = None;
        vec![Command::LoadGarden]
    }

    pub fn go_to(&mut self, source: Source) -> Vec<Command> {
        let mut commands = self.switch_source(source);
        commands.extend(self.sync_rhythm());
        commands
    }
    pub fn select_entry(&mut self, entry_id: i64) -> Vec<Command> {
        match self.entries.iter().position(|row| row.entry_id == entry_id) {
            Some(index) => self.select(index, true),
            None => Vec::new(),
        }
    }
    pub fn set_focus(&mut self, column: Column) {
        self.focus = column;
    }

    pub fn look(&self) -> Look {
        self.settings.look
    }
    pub fn focus(&self) -> Column {
        self.focus
    }
    pub fn source(&self) -> Source {
        self.source
    }

    pub fn selected_entry(&self) -> Option<i64> {
        self.selected
    }

    #[cfg(test)]
    pub fn class(&self) -> LayoutClass {
        self.class
    }

    /// The renderers' borrowed projection of the state.
    pub fn view(&self) -> ReaderView<'_> {
        let source_lines: Vec<SourceLine> = self
            .sidebar
            .iter()
            .enumerate()
            .map(|(i, row)| source_line(row, i == self.sidebar_index))
            .collect();
        let source_title = self
            .sidebar
            .iter()
            .find(|row| match self.garden_view {
                Some(view) => row.garden_view() == Some(view),
                None => row.source() == Some(self.source),
            })
            .map(|row| source_line(row, false).label)
            .unwrap_or_else(|| {
                if let Some(view) = self.garden_view {
                    return view.title().to_string();
                }
                match self.source {
                    Source::Journal => "Journal",
                    Source::Unread => "Unread",
                    Source::Today => "Today",
                    Source::Starred => "Starred",
                    _ => "",
                }
                .to_string()
            });
        let journal_lines = if self.journal_active() {
            self.journal
                .as_ref()
                .map(journal::journal_lines)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let journal_cursor = self.journal_cursor_line(&journal_lines);
        ReaderView {
            look: self.look(),
            layout: self.class,
            focus: self.focus,
            now_local: self.local.unwrap_or(DateTime::UNIX_EPOCH.fixed_offset()),
            source: self.source,
            source_title,
            garden: self.garden_view.is_some(),
            source_lines,
            filter: self.filter,
            search: self.search.as_deref(),
            entries: &self.entries,
            entry_cursor: self.index(),
            total: self.total,
            unread: self.unread,
            journal_mode: self.journal_mode(),
            journal: self.journal.as_ref(),
            journal_phase: &self.journal_phase,
            journal_lines,
            journal_cursor,
            entry: self.detail.as_ref(),
            article: self.article.as_ref(),
            show_fulltext: self.show_fulltext,
            reader_scroll: self.scroll,
            images: self.settings.images,
            progress: self
                .progress
                .as_ref()
                .map(|(label, done, total)| (label.as_str(), *done, *total)),
            message: self.message.as_deref(),
            help: self.overlay == Overlay::Help,
            editing: self.search_editing || self.overlay == Overlay::LinkPrompt,
            rhythm: match self.overlay {
                Overlay::Rhythm { choice, .. } => Some((choice, RHYTHM_CHOICES.len())),
                _ => None,
            },
            rhythm_current: match self.overlay {
                Overlay::Rhythm { feed_id, .. } => Some(self.rhythm_choice(feed_id)),
                _ => None,
            },
            rhythm_line: self.rhythm_line(),
        }
    }

    /// The renderer reports the article's height in rows and the reader's visible rows (below
    /// the header); Space uses them.
    pub fn set_reader_metrics(&mut self, total_rows: usize, viewport_rows: u16) {
        self.metrics = Some((total_rows, viewport_rows));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let before = (self.source, self.garden_view, self.selected);
        let page = self.antiope.reader_page();
        let mut commands = self.handle_key(key);
        // A key that moves to another list or entry, without choosing a page, brings the entry
        // back over P100, P899 or `FIN DE L'EDITION`.
        if (self.source, self.garden_view, self.selected) != before {
            if self.antiope.reader_page() == page {
                self.antiope.show(ReaderPage::Article);
            }
            self.page_scroll = 0;
        }
        commands.extend(self.sync_rhythm());
        commands
    }

    fn rebuild_directory(&mut self) {
        self.directory = PageDirectory::new(&self.pages, self.sources.as_ref());
    }

    /// The Antiope page keys (spec §5, §6): digits and `⌫`, `Esc`, `S`, `?`, SUITE past the
    /// edition, and the scrolling of P100 and P899. `None` lets the key through to the rest.
    fn on_antiope_key(&mut self, key: KeyEvent) -> Option<Vec<Command>> {
        let in_journal =
            self.source() == Source::Journal && self.garden_view.is_none() && self.journal_mode();
        let rank_of = |id: i64| {
            self.journal()
                .and_then(|view| view.items.iter().find(|item| item.row.entry_id == id))
                .map(|item| item.rank)
        };
        let ctx = NavContext {
            dir: &self.directory,
            now: self.now.unwrap_or(DateTime::UNIX_EPOCH),
            in_text_input: self.search_editing,
            journal_last: self.last_journal_entry().map(|(_, rank)| rank),
            on_journal_entry: self.selected.filter(|_| in_journal).and_then(rank_of),
        };
        match self.antiope.on_key(key, &ctx) {
            NavOutcome::Unhandled => self.scroll_page(key.code).then(Vec::new),
            NavOutcome::Consumed | NavOutcome::BackToLastEntry => Some(Vec::new()),
            NavOutcome::EndOfEdition => Some(self.land_on_end()),
            NavOutcome::Jump(destination) => Some(self.jump(destination)),
        }
    }

    /// Typing a page number (spec §6): the source or garden page opens with the focus on its
    /// list; P100 and P899 open in the reader. A garden page is left for them, so that the
    /// garden's keys never act on a row that is no longer on screen.
    fn jump(&mut self, destination: PageDestination) -> Vec<Command> {
        self.page_scroll = 0;
        match destination {
            PageDestination::Source(source) => {
                let commands = self.go_to(source);
                self.set_focus(Column::Entries);
                commands
            }
            PageDestination::Garden(view) => self.open_garden(view),
            PageDestination::Sommaire | PageDestination::Guide => {
                let commands = if self.garden_view.is_some() {
                    self.switch_source(self.source)
                } else {
                    Vec::new()
                };
                self.set_focus(Column::Reader);
                commands
            }
        }
    }

    /// SUITE past P104's last entry: the entry is read, as `j` and Space do between entries,
    /// and `FIN DE L'EDITION` shows.
    fn land_on_end(&mut self) -> Vec<Command> {
        if !self.entries_visible() {
            return Vec::new();
        }
        self.antiope.show(ReaderPage::EndOfEdition);
        match self.selected_entry() {
            Some(id) => self.select_entry(id),
            None => Vec::new(),
        }
    }

    /// `i` in Antiope: the article's images swap between mosaics and the originals, as in the
    /// app, on an open article only.
    fn toggle_real_images(&mut self) -> Vec<Command> {
        if self.antiope.reader_page() != ReaderPage::Article || self.selected.is_none() {
            return Vec::new();
        }
        // Images are off in the settings: `i` turns them on, as mosaics.
        if !self.settings.images {
            self.settings.images = true;
            self.antiope_real_images = false;
            return vec![Command::SaveSettings];
        }
        self.antiope_real_images = !self.antiope_real_images;
        Vec::new()
    }

    /// The keys that scroll P100 and P899 while one is in the reader: one row for `j` and `k`
    /// (and the arrows outside the sources column), a page for Space. SUITE at the bottom leaves
    /// the page, as in the app. The bound comes from the last draw (the page's rows and the
    /// rows visible).
    fn scroll_page(&mut self, code: KeyCode) -> bool {
        let chosen = matches!(
            self.antiope.reader_page(),
            ReaderPage::Sommaire | ReaderPage::Guide
        );
        // With no entry open the reader shows P100 as well: it scrolls too, but its bottom and
        // top hand the keys back (`j` then opens the first entry).
        let fallback = self.antiope.reader_page() == ReaderPage::Article
            && self.selected.is_none()
            && self.garden_view.is_none();
        if !chosen && !fallback {
            return false;
        }
        let arrows = self.focus() != Column::Sources;
        let (total, viewport) = self.metrics.unwrap_or((0, 1));
        let max = total.saturating_sub(usize::from(viewport));
        let page = usize::from(viewport).saturating_sub(1).max(1);
        match code {
            KeyCode::Char('j') | KeyCode::Char(' ') | KeyCode::Down
                if code != KeyCode::Down || arrows =>
            {
                if self.page_scroll >= max {
                    if fallback {
                        return false;
                    }
                    self.antiope.show(ReaderPage::Article);
                    self.page_scroll = 0;
                } else {
                    let step = if code == KeyCode::Char(' ') { page } else { 1 };
                    self.page_scroll = (self.page_scroll + step).min(max);
                }
            }
            KeyCode::Char('k') | KeyCode::Up if code != KeyCode::Up || arrows => {
                if fallback && self.page_scroll == 0 {
                    return false;
                }
                self.page_scroll = self.page_scroll.saturating_sub(1);
            }
            _ => return false,
        }
        true
    }

    /// `t`: the other look. Each look starts from the entry: no page number half typed, no
    /// special page, mosaics.
    fn toggle_look(&mut self) -> Vec<Command> {
        self.settings.look = match self.settings.look {
            Look::Native => Look::Antiope,
            Look::Antiope => Look::Native,
        };
        self.antiope = AntiopeNav::default();
        self.page_scroll = 0;
        self.antiope_real_images = false;
        self.render_article();
        vec![Command::SaveSettings]
    }

    /// The entries list, or the reader showing its selected entry, is on screen: only then may
    /// a key read, mark or open an entry. Not behind a garden page, not while the sources fill
    /// the one-column window, not under a Journal placeholder (Building, Offline, Failed).
    fn entries_visible(&self) -> bool {
        let area = Rect::new(0, 0, self.width, self.height);
        let rects = columns(area, self.class, self.focus, self.look());
        self.garden_view.is_none()
            && (rects.entries.is_some() || rects.reader.is_some())
            && !(self.journal_mode() && self.journal_phase != JournalPhase::Idle)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Command::Quit];
        }
        // A window too small to draw anything: nothing is on screen to act on.
        if self.class == LayoutClass::TooSmall {
            return match key.code {
                KeyCode::Char('q') => vec![Command::Quit],
                KeyCode::Char('t') => self.toggle_look(),
                _ => Vec::new(),
            };
        }
        if self.search_editing {
            return self.on_search_key(key);
        }
        match self.overlay {
            Overlay::Help => {
                self.overlay = Overlay::None;
                return Vec::new();
            }
            Overlay::LinkPrompt => return self.on_link_key(key.code),
            Overlay::Rhythm { .. } => return self.on_rhythm_key(key.code),
            Overlay::None => {}
        }
        if self.look() == Look::Antiope
            && let Some(commands) = self.on_antiope_key(key)
        {
            return commands;
        }
        if self.garden_view.is_some()
            && self.focus != Column::Sources
            && let Some(commands) = self.on_garden_key(key)
        {
            return commands;
        }
        self.message = None;
        // In the sources column `j` and `k` walk the sources like the arrows (vim habit).
        let key = match (self.focus, key.code) {
            (Column::Sources, KeyCode::Char('j')) => KeyEvent::new(KeyCode::Down, key.modifiers),
            (Column::Sources, KeyCode::Char('k')) => KeyEvent::new(KeyCode::Up, key.modifiers),
            _ => key,
        };
        // What acts on the selected entry or on the list waits until one is drawn.
        if !self.entries_visible()
            && matches!(
                key.code,
                KeyCode::Char('j' | 'k' | ' ' | 'm' | 's' | 'A' | 'u' | 'f' | 'o')
                    | KeyCode::Down
                    | KeyCode::Up
            )
            && !(self.focus == Column::Sources && matches!(key.code, KeyCode::Down | KeyCode::Up))
        {
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('q') => vec![Command::Quit],
            // Antiope's `?` is the GUIDE page (`on_antiope_key`); this is Native's help.
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                Vec::new()
            }
            KeyCode::Char('t') => self.toggle_look(),
            KeyCode::Char('i') if self.look() == Look::Antiope => self.toggle_real_images(),
            KeyCode::Char('i') => {
                self.settings.images = !self.settings.images;
                vec![Command::SaveSettings]
            }
            KeyCode::Char('j') if self.journal_active() => self.move_journal(1, false),
            KeyCode::Char('k') if self.journal_active() => self.move_journal(-1, false),
            KeyCode::Char('j') => self.move_entry(1),
            KeyCode::Char('k') => self.move_entry(-1),
            KeyCode::Down if self.focus == Column::Sources => self.move_source(1),
            KeyCode::Up if self.focus == Column::Sources => self.move_source(-1),
            KeyCode::Down if self.journal_active() => self.move_journal(1, true),
            KeyCode::Up if self.journal_active() => self.move_journal(-1, true),
            KeyCode::Down => self.move_entry(1),
            KeyCode::Up => self.move_entry(-1),
            KeyCode::Char(' ') => self.advance(),
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = match self.focus {
                    Column::Reader => Column::Entries,
                    _ => Column::Sources,
                };
                Vec::new()
            }
            KeyCode::Char('l') | KeyCode::Right => self.focus_right(false),
            KeyCode::Tab => self.focus_right(true),
            KeyCode::Enter => self.enter(),
            KeyCode::Esc => self.escape(),
            KeyCode::Char('m') => self.toggle_read(),
            KeyCode::Char('s') => self.toggle_star(),
            KeyCode::Char('A') => vec![Command::MarkAllRead(self.source)],
            KeyCode::Char('u') => {
                self.filter = match self.filter {
                    ListFilter::All => ListFilter::Unread,
                    ListFilter::Unread => ListFilter::All,
                };
                vec![self.reload()]
            }
            KeyCode::Char('/') => {
                self.search_editing = true;
                self.search.get_or_insert_with(String::new);
                Vec::new()
            }
            KeyCode::Char('f') => self.full_text(),
            KeyCode::Char('o') => self.open_prompt(),
            // The Journal's Retry (spec §6): offline or failed, `r` builds the edition again.
            KeyCode::Char('r')
                if self.source == Source::Journal && journal::offers_retry(&self.journal_phase) =>
            {
                self.build_edition().into_iter().collect()
            }
            KeyCode::Char('r') => vec![Command::Refresh(Some(self.source))],
            KeyCode::Char('R') => vec![Command::Refresh(None)],
            KeyCode::Char('c') => self.rhythm_menu(),
            _ => Vec::new(),
        }
    }

    /// The garden's keys win while a garden page has the focus (spec §5); `None` lets the
    /// reader's own key through (`t`, the look). The page's `←`, `h`, Tab and Esc go back to the
    /// sources column, `q` quits; the garden's detail, help and confirmation take their own Esc.
    fn on_garden_key(&mut self, key: KeyEvent) -> Option<Vec<Command>> {
        use garden_state::{Command as G, Mode};
        let at_list = self
            .garden
            .as_ref()
            .is_none_or(|garden| garden.mode == Mode::Queue);
        match key.code {
            KeyCode::Char('t') => return None,
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Tab if at_list => {
                self.focus = Column::Sources;
                return Some(Vec::new());
            }
            KeyCode::Char('q') if at_list => return Some(vec![Command::Quit]),
            KeyCode::Char('c') if at_list => return Some(self.garden_rhythm_menu()),
            _ => {}
        }
        // The rows are not here yet: nothing else to press.
        Some(
            match self.garden.as_mut().map(|garden| garden.on_key(key)) {
                None | Some(G::None) => Vec::new(),
                Some(G::Quit) => vec![Command::Quit],
                Some(command) => vec![Command::Garden(command)],
            },
        )
    }

    pub fn on_event(&mut self, event: Event) -> Vec<Command> {
        // Another process may have changed the rhythm: ask again on every data change.
        if matches!(event, Event::DataChanged) {
            self.rhythm_feed = None;
        }
        let mut commands = self.handle_event(event);
        commands.extend(self.sync_rhythm());
        commands
    }

    fn handle_event(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::Sources(snapshot) => {
                self.sidebar = sidebar::rows(&snapshot);
                let fallback = self.sidebar_index.min(self.sidebar.len().saturating_sub(1));
                self.sidebar_index = match self.garden_view {
                    Some(view) => sidebar::index_of_garden(&self.sidebar, view),
                    None => sidebar::index_of(&self.sidebar, self.source),
                }
                .unwrap_or(fallback);
                self.sources = Some(snapshot);
                self.rebuild_directory();
                Vec::new()
            }
            Event::Pages(pages) => {
                self.pages = pages;
                self.rebuild_directory();
                Vec::new()
            }
            Event::GardenRows(rows) => {
                match (self.garden.as_mut(), self.garden_view) {
                    (Some(garden), _) => garden.reload(rows),
                    (None, Some(view)) => {
                        self.garden = Some(GardenState::with_filter(rows, false, view.filter()));
                    }
                    // Late rows of a page the reader has left.
                    (None, None) => {}
                }
                Vec::new()
            }
            Event::GardenMessage(message) => {
                if let Some(garden) = self.garden.as_mut() {
                    garden.message = message;
                }
                Vec::new()
            }
            Event::Cadence(info) => {
                self.cadence_arrived(info);
                Vec::new()
            }
            Event::Entries {
                source,
                page,
                appended,
                request,
            } => self.entries_loaded(source, page, appended, request),
            Event::Journal(view) => {
                self.journal = view;
                self.settle_journal_cursor()
            }
            Event::Entry(detail) => {
                self.entry_loaded(*detail);
                Vec::new()
            }
            Event::ImageReady { .. } | Event::ImageFailed { .. } => Vec::new(),
            Event::Progress { label, done, total } => {
                if label == journal::BUILDING {
                    self.journal_phase = JournalPhase::Building { done, total };
                }
                self.progress = Some((label, done, total));
                Vec::new()
            }
            Event::JobDone(outcome) => self.job_done(outcome),
            Event::DataChanged => self.data_changed(),
            Event::OwnWrite => self.own_write(),
            Event::Tick { now, local } => {
                self.now = Some(now);
                self.local = Some(local);
                self.on_tick_journal(local)
            }
            Event::Resize { width, height } => {
                self.resize(width, height);
                Vec::new()
            }
        }
    }

    /// A fresh load of the current list (the search when one is active, else the source) under
    /// a new request: whatever was in flight (a next page, an older list) is now stale.
    fn reload(&mut self) -> Command {
        self.request += 1;
        self.next = None;
        self.pending_advance = false;
        self.loading_more = false;
        self.pinned = None;
        self.list_command(None)
    }

    /// The load of the list in view: the search when one is active, else the source.
    fn list_command(&self, after: Option<ListCursor>) -> Command {
        match &self.search {
            Some(text) if !text.is_empty() && !self.search_editing => Command::Search {
                source: self.source,
                text: text.clone(),
                filter: self.filter,
                after,
                request: self.request,
            },
            _ => Command::LoadEntries {
                source: self.source,
                filter: self.filter,
                after,
                request: self.request,
            },
        }
    }

    /// The same list again after a change of the data: the open row is remembered so that a
    /// reload which no longer holds it (read under an unread filter) leaves the reader alone.
    fn reload_in_place(&mut self) -> Command {
        let pinned = self.index().map(|i| (i, self.entries[i].clone()));
        let command = self.reload();
        self.pinned = pinned;
        command
    }

    /// The next page of the list, once: `None` while one is already out or there is none.
    fn load_more(&mut self) -> Option<Command> {
        if self.loading_more {
            return None;
        }
        let after = self.next.clone()?;
        self.loading_more = true;
        Some(self.list_command(Some(after)))
    }

    fn index(&self) -> Option<usize> {
        let id = self.selected?;
        self.entries.iter().position(|row| row.entry_id == id)
    }

    /// The Journal column is what the entries column shows: the Journal itself, not filtered nor
    /// searched (those are plain lists of its entries).
    fn journal_mode(&self) -> bool {
        self.source == Source::Journal && self.filter == ListFilter::All && self.search.is_none()
    }

    /// The Journal column has an edition to walk.
    fn journal_active(&self) -> bool {
        self.journal_mode() && self.journal.is_some()
    }

    /// The line under the Journal's cursor: the overflow row it rests on, else the selected entry.
    fn journal_cursor_line(&self, lines: &[JournalLine]) -> Option<usize> {
        let view = self.journal.as_ref()?;
        if let Some(feed) = self.journal_overflow {
            return lines.iter().position(
                |line| matches!(line, JournalLine::Overflow(o) if view.overflow[*o].feed_id == feed),
            );
        }
        let id = self.selected?;
        lines.iter().position(
            |line| matches!(line, JournalLine::Entry(i) if view.items[*i].row.entry_id == id),
        )
    }

    /// `↓`/`↑` (`over_overflow`) and `j`/`k` along the Journal's lines: headings are crossed; an
    /// overflow row is stopped on by the arrows, which then select nothing, and passed by `j`/`k`.
    fn move_journal(&mut self, delta: isize, over_overflow: bool) -> Vec<Command> {
        enum Target {
            Entry(i64),
            Overflow(FeedId),
        }
        let Some(view) = self.journal.as_ref() else {
            return Vec::new();
        };
        let lines = journal::journal_lines(view);
        let step = delta.signum();
        let mut at = match self.journal_cursor_line(&lines) {
            Some(line) => line as isize,
            None if step > 0 => -1,
            None => lines.len() as isize,
        };
        let target = loop {
            at += step;
            let Some(line) = usize::try_from(at).ok().and_then(|i| lines.get(i)) else {
                return Vec::new();
            };
            match *line {
                JournalLine::Heading(_) => {}
                JournalLine::Overflow(o) if over_overflow => {
                    break Target::Overflow(view.overflow[o].feed_id);
                }
                JournalLine::Overflow(_) => {}
                JournalLine::Entry(i) => {
                    let id = view.items[i].row.entry_id;
                    // An entry the list does not hold (yet) cannot be selected: skip it.
                    if self.entries.iter().any(|row| row.entry_id == id) {
                        break Target::Entry(id);
                    }
                }
            }
        };
        match target {
            Target::Entry(id) => match self.entries.iter().position(|row| row.entry_id == id) {
                Some(index) => self.select(index, true),
                None => Vec::new(),
            },
            Target::Overflow(feed) => {
                self.rest_on_overflow(feed);
                Vec::new()
            }
        }
    }

    /// The cursor goes onto a feed's overflow row; the reader has no entry then.
    fn rest_on_overflow(&mut self, feed: FeedId) {
        self.journal_overflow = Some(feed);
        self.selected = None;
        self.detail = None;
        self.article = None;
        self.scroll = 0;
        self.show_fulltext = false;
        self.metrics = None;
    }

    /// A new edition may no longer hold the overflow row the cursor rested on: back onto an entry.
    fn settle_journal_cursor(&mut self) -> Vec<Command> {
        let Some(feed) = self.journal_overflow else {
            return Vec::new();
        };
        let still_there = self
            .journal
            .as_ref()
            .is_some_and(|view| view.overflow.iter().any(|m| m.feed_id == feed));
        if still_there {
            return Vec::new();
        }
        self.journal_overflow = None;
        if self.selected.is_none() && !self.entries.is_empty() && self.reader_visible() {
            return self.select(0, false);
        }
        Vec::new()
    }

    /// Where Space looks for the next unread entry from: after the selected entry, or after the
    /// entry that precedes the overflow row the cursor rests on.
    fn advance_from(&self) -> usize {
        let Some(feed) = self.journal_overflow else {
            return self.index().map_or(0, |i| i + 1);
        };
        let Some(view) = self.journal.as_ref() else {
            return 0;
        };
        let lines = journal::journal_lines(view);
        let Some(at) = lines.iter().position(
            |line| matches!(line, JournalLine::Overflow(o) if view.overflow[*o].feed_id == feed),
        ) else {
            return 0;
        };
        lines[..at]
            .iter()
            .rev()
            .find_map(|line| match line {
                JournalLine::Entry(i) => Some(view.items[*i].row.entry_id),
                _ => None,
            })
            .and_then(|id| self.entries.iter().position(|row| row.entry_id == id))
            .map_or(0, |i| i + 1)
    }

    /// An edition build, unless one is already out (or queued behind a refresh).
    fn build_edition(&mut self) -> Option<Command> {
        if self.edition_job {
            return None;
        }
        self.edition_job = true;
        Some(Command::BuildEdition)
    }

    /// The rollover: the first tick of a new 04:00 day (in the clock's own zone) asks for a build.
    fn on_tick_journal(&mut self, local: DateTime<FixedOffset>) -> Vec<Command> {
        let day = journal::day_of(local);
        let rolled = self.last_day.is_some_and(|last| last != day);
        self.last_day = Some(day);
        if rolled {
            self.build_edition().into_iter().collect()
        } else {
            Vec::new()
        }
    }

    /// Selects row `index`; opening it by hand (`mark_read`) marks it read, as the app does.
    fn select(&mut self, index: usize, mark_read: bool) -> Vec<Command> {
        let Some(row) = self.entries.get(index) else {
            return Vec::new();
        };
        let (id, was_read) = (row.entry_id, row.is_read);
        let mut commands = Vec::new();
        self.journal_overflow = None;
        if self.selected != Some(id) {
            self.selected = Some(id);
            self.detail = None;
            self.article = None;
            self.scroll = 0;
            self.show_fulltext = false;
            self.metrics = None;
            commands.push(Command::LoadEntry(id));
        }
        if mark_read && !was_read {
            self.entries[index].is_read = true;
            self.unread = self.unread.saturating_sub(1);
            commands.push(Command::SetRead(id, true));
        }
        commands
    }

    fn move_entry(&mut self, delta: isize) -> Vec<Command> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let target = self.index().map_or(0, |i| i.saturating_add_signed(delta));
        if target >= self.entries.len() {
            return if delta > 0 {
                self.load_more().into_iter().collect()
            } else {
                Vec::new()
            };
        }
        if Some(target) == self.index() {
            return Vec::new();
        }
        self.select(target, true)
    }

    fn reader_rect(&self) -> Option<Rect> {
        let area = Rect::new(0, 0, self.width, self.height);
        columns(area, self.class, Column::Reader, self.look()).reader
    }

    /// Article lines visible at once.
    fn page_height(&self) -> usize {
        if let Some((_, viewport)) = self.metrics {
            return usize::from(viewport).max(1);
        }
        self.reader_rect().map_or(1, |rect| {
            usize::from(text_height(rect, self.look()))
                .saturating_sub(READER_HEADER_LINES)
                .max(1)
        })
    }

    fn reader_text_width(&self) -> u16 {
        self.reader_rect()
            .map_or(self.width, |rect| text_width(rect, self.look()))
    }

    fn max_scroll(&self) -> usize {
        let rows = self
            .metrics
            .map(|(total, _)| total)
            .or_else(|| self.article.as_ref().map(|a| a.lines.len()))
            .unwrap_or(0);
        rows.saturating_sub(self.page_height())
    }

    /// Space (spec §5): scroll the article, then the next unread entry of the source.
    fn advance(&mut self) -> Vec<Command> {
        let max = self.max_scroll();
        if self.article.is_some() && self.scroll < max {
            let step = self.page_height().saturating_sub(1).max(1);
            self.scroll = (self.scroll + step).min(max);
            return Vec::new();
        }
        let from = self.advance_from().min(self.entries.len());
        if let Some(offset) = self.entries[from..].iter().position(|row| !row.is_read) {
            if self.class == LayoutClass::One {
                self.focus = Column::Reader;
            }
            return self.select(from + offset, true);
        }
        if self.next.is_some() {
            self.pending_advance = true;
            return self.load_more().into_iter().collect();
        }
        if self.look() == Look::Antiope && self.on_last_journal_entry() {
            return self.land_on_end();
        }
        let end = if self.source == Source::Journal {
            END_LINE
        } else {
            NO_MORE_UNREAD
        };
        self.message = Some(end.to_string());
        Vec::new()
    }

    /// The Journal's last entry by rank (its id and rank), while the Journal is the page in view.
    fn last_journal_entry(&self) -> Option<(i64, u32)> {
        if self.source != Source::Journal || self.garden_view.is_some() {
            return None;
        }
        let last = self.journal()?.items.iter().max_by_key(|item| item.rank)?;
        Some((last.row.entry_id, last.rank))
    }

    /// The Journal's last entry by rank is the one open in the reader.
    fn on_last_journal_entry(&self) -> bool {
        self.last_journal_entry()
            .is_some_and(|(id, _)| self.selected == Some(id))
    }

    fn focus_right(&mut self, wrap: bool) -> Vec<Command> {
        match self.focus {
            Column::Sources => self.open_sidebar_row(),
            // An overflow row has no article: Enter opens its feed, the other keys wait.
            Column::Entries if self.journal_overflow.is_some() || !self.entries_visible() => {
                Vec::new()
            }
            Column::Entries => {
                self.focus = Column::Reader;
                if self.selected.is_none() && !self.entries.is_empty() {
                    return self.select(0, true);
                }
                Vec::new()
            }
            Column::Reader => {
                if wrap {
                    self.focus = Column::Sources;
                }
                Vec::new()
            }
        }
    }

    fn enter(&mut self) -> Vec<Command> {
        match self.focus {
            Column::Sources => self.open_sidebar_row(),
            Column::Entries => match self.journal_overflow {
                Some(feed) => self.switch_source(Source::Feed(feed)),
                None => self.focus_right(false),
            },
            Column::Reader => Vec::new(),
        }
    }

    fn escape(&mut self) -> Vec<Command> {
        if self.search.is_some() {
            self.search = None;
            return vec![self.reload()];
        }
        self.focus = match (self.class, self.focus) {
            (LayoutClass::One, Column::Reader) => Column::Entries,
            (LayoutClass::One, Column::Entries) => Column::Sources,
            (LayoutClass::Two, Column::Sources) => Column::Entries,
            (_, focus) => focus,
        };
        Vec::new()
    }

    fn open_sidebar_row(&mut self) -> Vec<Command> {
        self.focus = Column::Entries;
        let Some(row) = self.sidebar.get(self.sidebar_index) else {
            return Vec::new();
        };
        match (row.garden_view(), row.source()) {
            (Some(view), _) => self.open_garden(view),
            (None, Some(source)) => self.switch_source(source),
            (None, None) => Vec::new(),
        }
    }

    fn move_source(&mut self, delta: isize) -> Vec<Command> {
        if self.sidebar.is_empty() {
            return Vec::new();
        }
        self.sidebar_index = sidebar::step(&self.sidebar, self.sidebar_index, delta);
        let row = &self.sidebar[self.sidebar_index];
        match (row.garden_view(), row.source()) {
            (Some(view), _) => self.show_garden(view),
            (None, Some(source)) => self.switch_source(source),
            (None, None) => Vec::new(),
        }
    }

    fn switch_source(&mut self, source: Source) -> Vec<Command> {
        if let Some(index) = sidebar::index_of(&self.sidebar, source) {
            self.sidebar_index = index;
        }
        // Leaving a garden page for the reading source it interrupted: the list is reloaded, it
        // may have changed meanwhile (a buried feed's entries).
        let from_garden = self.garden_view.take().is_some();
        self.garden = None;
        if source == self.source && !from_garden {
            return Vec::new();
        }
        self.source = source;
        self.journal_overflow = None;
        self.entries.clear();
        self.total = 0;
        self.unread = 0;
        self.next = None;
        self.selected = None;
        self.detail = None;
        self.article = None;
        self.scroll = 0;
        self.search = None;
        let mut commands = Vec::new();
        if source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands.push(self.reload());
        commands
    }

    fn toggle_read(&mut self) -> Vec<Command> {
        let Some(index) = self.index() else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let row = &mut self.entries[index];
        row.is_read = !row.is_read;
        let (id, read) = (row.entry_id, row.is_read);
        if read {
            self.unread = self.unread.saturating_sub(1);
        } else {
            self.unread += 1;
        }
        if let Some(detail) = self.detail.as_mut().filter(|d| d.row.entry_id == id) {
            detail.row.is_read = read;
        }
        vec![Command::SetRead(id, read)]
    }

    fn toggle_star(&mut self) -> Vec<Command> {
        let Some(index) = self.index() else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let row = &mut self.entries[index];
        row.is_starred = !row.is_starred;
        let (id, starred) = (row.entry_id, row.is_starred);
        if let Some(detail) = self.detail.as_mut().filter(|d| d.row.entry_id == id) {
            detail.row.is_starred = starred;
        }
        vec![Command::SetStarred(id, starred)]
    }

    /// `f`: the stored full text, the feed's content again, or an extraction.
    fn full_text(&mut self) -> Vec<Command> {
        let Some(detail) = &self.detail else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let id = detail.row.entry_id;
        let has_fulltext = detail.fulltext_html.is_some();
        if self.show_fulltext || has_fulltext {
            self.show_fulltext = !self.show_fulltext;
            self.render_article();
            return Vec::new();
        }
        if self.extracting == Some(id) {
            return Vec::new();
        }
        self.extracting = Some(id);
        self.message = Some(FETCHING_FULL.into());
        vec![Command::Extract(id)]
    }

    /// The article for the reader's current text width.
    fn render_article(&mut self) {
        let Some(detail) = &self.detail else {
            self.article = None;
            return;
        };
        let html = if self.show_fulltext {
            detail.fulltext_html.as_deref()
        } else {
            detail
                .content_html
                .as_deref()
                .or(detail.summary_html.as_deref())
        };
        let article = render_html(
            html.unwrap_or_default(),
            self.reader_text_width(),
            detail.row.url.as_deref(),
        );
        self.article = Some(article);
        self.metrics = None;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn open_prompt(&mut self) -> Vec<Command> {
        if self.detail.is_none() {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        }
        self.overlay = Overlay::LinkPrompt;
        self.message = Some(OPEN_PROMPT.into());
        Vec::new()
    }

    fn on_link_key(&mut self, code: KeyCode) -> Vec<Command> {
        self.overlay = Overlay::None;
        self.message = None;
        match code {
            KeyCode::Enter => {
                let url = self.detail.as_ref().and_then(|d| d.row.url.clone());
                match url.filter(|u| is_web_url(u)) {
                    Some(url) => vec![Command::OpenUrl(url)],
                    None => {
                        self.message = Some(NO_LINK.into());
                        Vec::new()
                    }
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                let number = c.to_digit(10).map_or(0, |d| d as usize);
                match self.article.as_ref().and_then(|a| a.links.get(number - 1)) {
                    Some(url) => vec![Command::OpenUrl(url.clone())],
                    None => {
                        self.message = Some(format!("No link [{number}]."));
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    fn on_search_key(&mut self, key: KeyEvent) -> Vec<Command> {
        match key.code {
            KeyCode::Char(_)
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Vec::new()
            }
            KeyCode::Char(c) => {
                self.search.get_or_insert_with(String::new).push(c);
                Vec::new()
            }
            KeyCode::Backspace => {
                if let Some(text) = self.search.as_mut() {
                    text.pop();
                }
                Vec::new()
            }
            KeyCode::Enter => {
                self.search_editing = false;
                if self.search.as_deref().is_some_and(str::is_empty) {
                    self.search = None;
                }
                vec![self.reload()]
            }
            KeyCode::Esc => {
                self.search_editing = false;
                self.search = None;
                vec![self.reload()]
            }
            _ => Vec::new(),
        }
    }

    /// `c` on a reading page: the rhythm menu for the feed in view (a feed source, else the
    /// entry's feed).
    fn rhythm_menu(&mut self) -> Vec<Command> {
        let feed = match self.source {
            Source::Feed(id) => Some(id),
            _ => self.index().map(|i| self.entries[i].feed_id),
        };
        match feed {
            Some(feed_id) => self.open_rhythm_menu(feed_id),
            None => {
                self.message = Some(NO_FEED.into());
                Vec::new()
            }
        }
    }

    /// `c` on a garden page: the selected row's feed, unless it is buried (a buried feed has no
    /// rhythm to set).
    fn garden_rhythm_menu(&mut self) -> Vec<Command> {
        let Some(garden) = self.garden.as_mut() else {
            return Vec::new();
        };
        let Some((feed_id, buried)) = garden.selected_row().map(|r| (r.feed_id, r.buried)) else {
            return Vec::new();
        };
        if buried {
            garden.message = Some(format!("Feed #{feed_id} is buried."));
            return Vec::new();
        }
        self.open_rhythm_menu(feed_id)
    }

    /// The menu opens on the feed's current setting: its forced group, else `Auto`. A feed whose
    /// rhythm is not known yet (a list's entry, not the feed in view) is asked for, and the menu
    /// moves onto the answer if it has not been touched.
    fn open_rhythm_menu(&mut self, feed_id: FeedId) -> Vec<Command> {
        self.overlay = Overlay::Rhythm {
            feed_id,
            choice: self.rhythm_choice(feed_id),
        };
        // The feed in view is kept fresh (every data change asks again); any other is asked.
        match self.rhythm.as_ref() {
            Some(info) if info.feed_id == feed_id && self.feed_in_view() == Some(feed_id) => {
                Vec::new()
            }
            _ => vec![Command::LoadCadence(feed_id)],
        }
    }

    /// The menu item that is the feed's current setting.
    fn rhythm_choice(&self, feed_id: FeedId) -> usize {
        self.rhythm
            .as_ref()
            .filter(|info| info.feed_id == feed_id && info.is_override)
            .and_then(|info| {
                RHYTHM_CHOICES
                    .iter()
                    .position(|choice| *choice == Some(info.effective))
            })
            .unwrap_or(0)
    }

    /// The feed whose rhythm the page shows: the selected garden row (never a buried one) or the
    /// feed source.
    fn feed_in_view(&self) -> Option<FeedId> {
        if self.garden_view.is_some() {
            return self
                .garden
                .as_ref()
                .and_then(GardenState::selected_row)
                .filter(|row| !row.buried)
                .map(|row| row.feed_id);
        }
        match self.source {
            Source::Feed(id) => Some(id),
            _ => None,
        }
    }

    /// Asks for the rhythm of the feed in view when it is not the one last asked for; drops a
    /// rhythm that belongs to another feed.
    fn sync_rhythm(&mut self) -> Option<Command> {
        let want = self.feed_in_view();
        if want == self.rhythm_feed {
            return None;
        }
        self.rhythm_feed = want;
        if self.rhythm.as_ref().map(|info| info.feed_id) != want {
            self.rhythm = None;
        }
        want.map(Command::LoadCadence)
    }

    /// An answer is kept when it is for the feed in view or the one the menu is open on; a menu
    /// still on its first item moves onto the feed's current setting.
    fn cadence_arrived(&mut self, info: RhythmInfo) {
        let menu_feed = match self.overlay {
            Overlay::Rhythm { feed_id, .. } => Some(feed_id),
            _ => None,
        };
        if Some(info.feed_id) != self.feed_in_view() && Some(info.feed_id) != menu_feed {
            return;
        }
        let feed_id = info.feed_id;
        self.rhythm = Some(info);
        if let Overlay::Rhythm { choice: 0, .. } = self.overlay
            && menu_feed == Some(feed_id)
        {
            self.overlay = Overlay::Rhythm {
                feed_id,
                choice: self.rhythm_choice(feed_id),
            };
        }
    }

    /// The rhythm line of the feed in view, worded as the app's; the time is the state's clock.
    fn rhythm_line(&self) -> Option<String> {
        let want = self.feed_in_view()?;
        let info = self.rhythm.as_ref().filter(|info| info.feed_id == want)?;
        let now = self.local?.with_timezone(&Utc);
        Some(rhythm::rhythm_line(info, now))
    }

    fn on_rhythm_key(&mut self, code: KeyCode) -> Vec<Command> {
        let Overlay::Rhythm { feed_id, choice } = self.overlay else {
            return Vec::new();
        };
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                let choice = (choice + 1).min(RHYTHM_CHOICES.len() - 1);
                self.overlay = Overlay::Rhythm { feed_id, choice };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.overlay = Overlay::Rhythm {
                    feed_id,
                    choice: choice.saturating_sub(1),
                };
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                return vec![Command::SetCadence(feed_id, RHYTHM_CHOICES[choice])];
            }
            _ => self.overlay = Overlay::None,
        }
        Vec::new()
    }

    fn reader_visible(&self) -> bool {
        self.class != LayoutClass::One || self.focus == Column::Reader
    }

    fn entries_loaded(
        &mut self,
        source: Source,
        page: EntryPage,
        appended: bool,
        request: u64,
    ) -> Vec<Command> {
        if source != self.source || request < self.request {
            return Vec::new();
        }
        if appended {
            self.loading_more = false;
            let known: HashSet<i64> = self.entries.iter().map(|row| row.entry_id).collect();
            self.entries.extend(
                page.rows
                    .into_iter()
                    .filter(|row| !known.contains(&row.entry_id)),
            );
        } else {
            self.entries = page.rows;
            if let Some((at, row)) = self.pinned.take()
                && !self.entries.iter().any(|r| r.entry_id == row.entry_id)
            {
                self.entries.insert(at.min(self.entries.len()), row);
            }
        }
        self.total = page.total;
        self.unread = page.unread;
        self.next = page.next;
        if self.index().is_none() {
            self.selected = None;
            self.detail = None;
            self.article = None;
        }
        if std::mem::take(&mut self.pending_advance) {
            return self.advance();
        }
        if self.selected.is_none()
            && self.journal_overflow.is_none()
            && !self.entries.is_empty()
            && self.reader_visible()
        {
            return self.select(0, false);
        }
        Vec::new()
    }

    fn entry_loaded(&mut self, detail: EntryDetail) {
        if self.selected != Some(detail.row.entry_id) {
            return;
        }
        let same = self
            .detail
            .as_ref()
            .is_some_and(|d| d.row.entry_id == detail.row.entry_id);
        if !same {
            self.scroll = 0;
            self.show_fulltext = detail.prefer_full_text && detail.fulltext_html.is_some();
        } else if self.show_fulltext && detail.fulltext_html.is_none() {
            self.show_fulltext = false;
        }
        self.detail = Some(detail);
        self.render_article();
    }

    fn job_done(&mut self, outcome: JobOutcome) -> Vec<Command> {
        self.progress = None;
        match outcome {
            JobOutcome::Refreshed => vec![Command::LoadSources, self.reload_in_place()],
            JobOutcome::EditionBuilt | JobOutcome::EditionNotDue => {
                self.edition_job = false;
                self.journal_phase = JournalPhase::Idle;
                self.data_changed()
            }
            JobOutcome::EditionOffline => {
                self.edition_job = false;
                self.journal_phase = JournalPhase::Offline;
                Vec::new()
            }
            JobOutcome::EditionFailed(sentence) => {
                self.edition_job = false;
                self.journal_phase = JournalPhase::Failed(sentence);
                Vec::new()
            }
            JobOutcome::Offline => {
                self.message = Some(OFFLINE.into());
                Vec::new()
            }
            JobOutcome::Failed(sentence) => {
                // A list load that failed answers with this too: the next page may be asked again.
                self.loading_more = false;
                self.extracting = None;
                self.message = Some(sentence);
                Vec::new()
            }
            JobOutcome::Extracted { entry_id, error } => {
                if self.extracting == Some(entry_id) {
                    self.extracting = None;
                }
                if self.selected != Some(entry_id) {
                    return Vec::new();
                }
                self.message = None;
                if error.is_none() {
                    self.show_fulltext = true;
                }
                vec![Command::LoadEntry(entry_id)]
            }
        }
    }

    /// Another process (or this one) changed the database: reload, keeping the selection.
    fn data_changed(&mut self) -> Vec<Command> {
        let mut commands = vec![Command::LoadSources, Command::LoadPages];
        if self.garden_view.is_some() {
            commands.push(Command::LoadGarden);
        }
        if self.source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands.push(self.reload_in_place());
        commands
    }

    /// A `j`, `m` or `s` of this reader: the list in view and the open article already show the
    /// change, so only the sources' counts (and the Journal's header) are asked for again.
    fn own_write(&mut self) -> Vec<Command> {
        let mut commands = vec![Command::LoadSources];
        if self.source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands
    }

    /// Review Focus 3: the selection and the scroll survive; a narrow pane keeps the article.
    fn resize(&mut self, width: u16, height: u16) {
        let before = self.class;
        self.width = width;
        self.height = height;
        self.metrics = None;
        self.class = layout_class(width, height);
        if self.class == LayoutClass::One
            && before != LayoutClass::One
            && self.detail.is_some()
            && self.focus != Column::Sources
        {
            self.focus = Column::Reader;
        }
        // A too-small window has no reader column to measure: keep the article and the scroll
        // as they are, they are re-fitted when the window grows back.
        if self.class != LayoutClass::TooSmall {
            self.render_article();
        }
    }
}

/// A sidebar row as a drawable line: titles from the feeds go through `printable`.
fn source_line(row: &SidebarRow, selected: bool) -> SourceLine {
    let line = |label: &str, unread: usize, depth: usize| SourceLine {
        label: printable(label).into_owned(),
        unread,
        depth: u16::try_from(depth).unwrap_or(u16::MAX),
        selected,
        heading: false,
    };
    match row {
        SidebarRow::Heading(label) => SourceLine {
            heading: true,
            selected: false,
            ..line(label, 0, 0)
        },
        SidebarRow::Smart { title, unread, .. } => line(title, *unread, 0),
        SidebarRow::Folder {
            name,
            depth,
            unread,
            ..
        } => line(name, *unread, *depth),
        SidebarRow::Feed {
            title,
            depth,
            unread,
            ..
        } => line(title, *unread, *depth),
        SidebarRow::Rhythm { group, unread } => line(&group_label(*group), *unread, 0),
        SidebarRow::Garden { view, count } => line(view.title(), *count, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::sidebar::sample_snapshot;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn row(id: i64, feed: FeedId, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: feed,
            feed_title: format!("Feed {feed}"),
            title: Some(format!("Entry {id}")),
            author: None,
            url: Some(format!("https://e.test/{id}")),
            published_at: "2026-10-16T08:00:00Z".parse().unwrap(),
            is_read: read,
            is_starred: false,
            has_fulltext: false,
        }
    }

    fn page(rows: Vec<EntryRow>, next: Option<ListCursor>) -> EntryPage {
        let unread = rows.iter().filter(|r| !r.is_read).count();
        EntryPage {
            total: rows.len(),
            unread,
            rows,
            next,
        }
    }

    fn cursor() -> ListCursor {
        ListCursor {
            published_at: "2026-10-15T08:00:00.000Z".into(),
            entry_id: 1,
        }
    }

    fn detail_with(id: i64, html: &str) -> Box<EntryDetail> {
        Box::new(EntryDetail {
            row: row(id, 7, false),
            summary_html: None,
            content_html: Some(html.to_string()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        })
    }

    /// `paragraphs` one-line paragraphs: 2 × paragraphs − 1 lines once rendered.
    fn detail(id: i64, paragraphs: usize) -> Box<EntryDetail> {
        let html: String = (0..paragraphs)
            .map(|i| format!("<p>Paragraph {i}</p>"))
            .collect();
        detail_with(id, &html)
    }

    fn key(state: &mut ReaderState, code: KeyCode) -> Vec<Command> {
        state.on_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn char_key(state: &mut ReaderState, c: char) -> Vec<Command> {
        key(state, KeyCode::Char(c))
    }

    fn article_text(state: &ReaderState) -> String {
        state
            .article
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .map(|line| line.spans.iter().map(|s| &*s.content).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn entries(
        state: &ReaderState,
        source: Source,
        rows: Vec<EntryRow>,
        next: Option<ListCursor>,
    ) -> Event {
        Event::Entries {
            source,
            page: page(rows, next),
            appended: false,
            request: state.request,
        }
    }

    /// 140 × 40 (three columns) on the Journal, entries 1–4, entries 2 and 4 already read,
    /// entry 1 shown (not marked read: nobody asked for it yet).
    /// A fixed clock: 16 October 2026, 06:00 UTC, 08:00 in a +02:00 zone.
    fn test_clock() -> (
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::FixedOffset>,
    ) {
        use chrono::TimeZone;
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 16, 6, 0, 0).unwrap();
        (
            now,
            now.with_timezone(&chrono::FixedOffset::east_opt(7200).unwrap()),
        )
    }

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        let (now, local) = test_clock();
        state.start(now, local);
        state.on_event(Event::Sources(sample_snapshot()));
        let rows = vec![
            row(1, 7, false),
            row(2, 7, true),
            row(3, 8, false),
            row(4, 8, true),
        ];
        state.on_event(entries(&state, Source::Journal, rows, None));
        state
    }

    #[test]
    fn the_page_list_follows_the_sources_and_is_kept() {
        use ressac_core::store::pages::{PageKind, PageRow};
        let mut state = reader();
        let page = PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: None,
        };
        assert!(state.pages.is_empty());
        assert!(state.on_event(Event::Pages(vec![page.clone()])).is_empty());
        assert_eq!(state.pages, [page]);
        // Another process may renumber: every data change asks again.
        assert!(
            state
                .on_event(Event::DataChanged)
                .contains(&Command::LoadPages)
        );
    }

    #[test]
    fn the_reader_opens_on_the_journal() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        assert_eq!(state.source(), Source::Journal);
        assert_eq!(state.focus(), Column::Entries);
        assert_eq!(state.class(), LayoutClass::Three);
        assert_eq!(state.look(), Look::Native);
        let (now, local) = test_clock();
        assert_eq!(
            state.start(now, local),
            [
                Command::LoadSources,
                Command::LoadPages,
                Command::LoadJournal,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request: 1
                },
                Command::BuildEdition,
            ]
        );
    }

    #[test]
    fn the_first_entry_is_shown_without_being_marked_read() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        let commands = state.on_event(entries(
            &state,
            Source::Journal,
            vec![row(1, 7, false)],
            None,
        ));
        assert_eq!(commands, [Command::LoadEntry(1)]);
        assert_eq!(state.selected_entry(), Some(1));
        assert!(!state.entries[0].is_read);
    }

    #[test]
    fn one_column_waits_for_enter_before_showing_an_entry() {
        let mut state = ReaderState::new(TerminalSettings::default(), 70, 40);
        assert_eq!(state.class(), LayoutClass::One);
        let commands = state.on_event(entries(
            &state,
            Source::Journal,
            vec![row(1, 7, false)],
            None,
        ));
        assert!(commands.is_empty());
        assert_eq!(state.selected_entry(), None);
    }

    #[test]
    fn entries_of_another_source_are_ignored() {
        let mut state = reader();
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(9, 7, false)],
            None,
        ));
        assert_eq!(state.entries.len(), 4);
    }

    #[test]
    fn j_and_k_move_and_mark_what_they_open_as_read() {
        let mut state = reader();
        assert_eq!(state.unread, 2);
        assert_eq!(char_key(&mut state, 'j'), [Command::LoadEntry(2)]);
        assert_eq!(
            char_key(&mut state, 'j'),
            [Command::LoadEntry(3), Command::SetRead(3, true)]
        );
        assert_eq!(state.unread, 1);
        assert_eq!(key(&mut state, KeyCode::Up), [Command::LoadEntry(2)]);
        assert_eq!(
            char_key(&mut state, 'k'),
            [Command::LoadEntry(1), Command::SetRead(1, true)]
        );
        assert_eq!(state.unread, 0);
        assert!(char_key(&mut state, 'k').is_empty());
    }

    #[test]
    fn j_past_the_loaded_rows_asks_for_the_next_page() {
        let mut state = reader();
        for _ in 0..3 {
            char_key(&mut state, 'j');
        }
        assert!(char_key(&mut state, 'j').is_empty());
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.on_event(entries(&state, Source::Unread, vec![], None));
        state.source = Source::Unread;
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(1, 7, false)],
            Some(cursor()),
        ));
        assert_eq!(
            char_key(&mut state, 'j'),
            [Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: Some(cursor()),
                request: 0,
            }]
        );
    }

    #[test]
    fn space_scrolls_the_article_then_moves_to_the_next_unread() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        assert_eq!(state.article.as_ref().unwrap().lines.len(), 79);
        // Reader text height at 140 × 40: 38 − 2 (border) − 3 (header) = 33; max scroll 46.
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.scroll, 32);
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.scroll, 46);
        assert_eq!(
            char_key(&mut state, ' '),
            [Command::LoadEntry(3), Command::SetRead(3, true)]
        );
        assert_eq!(state.scroll, 0);
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.message.as_deref(), Some("That's the edition."));
    }

    #[test]
    fn space_waits_for_the_next_page_then_carries_on() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Unread;
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(1, 7, true)],
            Some(cursor()),
        ));
        assert_eq!(
            char_key(&mut state, ' '),
            [Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: Some(cursor()),
                request: 0,
            }]
        );
        let more = Event::Entries {
            source: Source::Unread,
            page: page(vec![row(5, 7, false)], None),
            appended: true,
            request: state.request,
        };
        assert_eq!(
            state.on_event(more),
            [Command::LoadEntry(5), Command::SetRead(5, true)]
        );
    }

    #[test]
    fn a_failed_next_page_can_be_asked_again() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Unread;
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(1, 7, true)],
            Some(cursor()),
        ));
        assert_eq!(char_key(&mut state, ' ').len(), 1, "the page is asked");
        assert!(state.loading_more);
        state.on_event(Event::JobDone(JobOutcome::Failed("busy".into())));
        assert!(
            !state.loading_more,
            "the failed load no longer blocks the next"
        );
        assert_eq!(state.message.as_deref(), Some("busy"));
        assert_eq!(char_key(&mut state, ' ').len(), 1, "asked again");
    }

    #[test]
    fn space_at_the_end_of_a_list_says_so() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Starred;
        state.on_event(entries(
            &state,
            Source::Starred,
            vec![row(1, 7, true)],
            None,
        ));
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(
            state.message.as_deref(),
            Some("No more unread entries here.")
        );
    }

    #[test]
    fn arrows_in_the_sources_column_switch_the_list() {
        let mut state = reader();
        key(&mut state, KeyCode::Left);
        assert_eq!(state.focus(), Column::Sources);
        assert_eq!(
            key(&mut state, KeyCode::Down),
            [Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: None,
                request: 2
            }]
        );
        assert_eq!(state.source(), Source::Unread);
        assert!(state.entries.is_empty());
        assert_eq!(state.selected_entry(), None);
        key(&mut state, KeyCode::Down);
        key(&mut state, KeyCode::Down);
        assert_eq!(
            key(&mut state, KeyCode::Down),
            [Command::LoadEntries {
                source: Source::Folder(1),
                filter: ListFilter::All,
                after: None,
                request: 5
            }]
        );
        assert_eq!(state.sidebar_index, 5);
        key(&mut state, KeyCode::Up);
        key(&mut state, KeyCode::Up);
        key(&mut state, KeyCode::Up);
        assert_eq!(
            key(&mut state, KeyCode::Up),
            [
                Command::LoadJournal,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request: 9
                },
            ]
        );
    }

    #[test]
    fn columns_in_three_column_mode() {
        let mut state = reader();
        char_key(&mut state, 'l');
        assert_eq!(state.focus(), Column::Reader);
        char_key(&mut state, 'h');
        assert_eq!(state.focus(), Column::Entries);
        char_key(&mut state, 'h');
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Right);
        assert_eq!(state.focus(), Column::Entries);
        key(&mut state, KeyCode::Tab);
        key(&mut state, KeyCode::Tab);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Sources);
    }

    #[test]
    fn two_column_mode_opens_the_sources_over_the_list() {
        let mut state = reader();
        state.on_event(Event::Resize {
            width: 100,
            height: 40,
        });
        assert_eq!(state.class(), LayoutClass::Two);
        key(&mut state, KeyCode::Left);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Entries);
    }

    #[test]
    fn one_column_mode_walks_in_with_enter_and_out_with_esc() {
        let mut state = ReaderState::new(TerminalSettings::default(), 70, 40);
        state.on_event(Event::Sources(sample_snapshot()));
        state.on_event(entries(
            &state,
            Source::Journal,
            vec![row(1, 7, false)],
            None,
        ));
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::LoadEntry(1), Command::SetRead(1, true)]
        );
        assert_eq!(state.focus(), Column::Reader);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Entries);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Enter);
        assert_eq!(state.focus(), Column::Entries);
    }

    #[test]
    fn m_s_shift_a_and_u() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 'm'), [Command::SetRead(1, true)]);
        assert_eq!(state.unread, 1);
        assert_eq!(char_key(&mut state, 'm'), [Command::SetRead(1, false)]);
        assert_eq!(state.unread, 2);
        assert_eq!(char_key(&mut state, 's'), [Command::SetStarred(1, true)]);
        assert!(state.entries[0].is_starred);
        let shifted = state.on_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(shifted, [Command::MarkAllRead(Source::Journal)]);
        assert_eq!(
            char_key(&mut state, 'u'),
            [Command::LoadEntries {
                source: Source::Journal,
                filter: ListFilter::Unread,
                after: None,
                request: 2
            }]
        );
    }

    #[test]
    fn search_is_typed_then_run_then_cleared() {
        let mut state = reader();
        assert!(char_key(&mut state, '/').is_empty());
        for c in ['r', 'u', 'x'] {
            assert!(char_key(&mut state, c).is_empty());
        }
        key(&mut state, KeyCode::Backspace);
        char_key(&mut state, 's');
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::Search {
                source: Source::Journal,
                text: "rus".into(),
                filter: ListFilter::All,
                after: None,
                request: 2
            }]
        );
        assert_eq!(
            key(&mut state, KeyCode::Esc),
            [Command::LoadEntries {
                source: Source::Journal,
                filter: ListFilter::All,
                after: None,
                request: 3
            }]
        );
        assert_eq!(state.search, None);
    }

    #[test]
    fn full_text_is_extracted_once_then_toggled() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 3)));
        assert_eq!(char_key(&mut state, 'f'), [Command::Extract(1)]);
        assert_eq!(state.message.as_deref(), Some("Fetching the full article…"));
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::Extracted {
                entry_id: 1,
                error: None
            })),
            [Command::LoadEntry(1)]
        );
        let mut full = detail(1, 3);
        full.fulltext_html = Some("<p>Full article</p>".into());
        state.on_event(Event::Entry(full));
        assert!(article_text(&state).contains("Full article"));
        assert!(char_key(&mut state, 'f').is_empty());
        assert!(article_text(&state).contains("Paragraph 0"));
        assert!(char_key(&mut state, 'f').is_empty());
        assert!(article_text(&state).contains("Full article"));
    }

    #[test]
    fn o_opens_the_article_or_a_numbered_link() {
        let mut state = reader();
        assert!(char_key(&mut state, 'o').is_empty());
        assert_eq!(state.message.as_deref(), Some("No entry selected."));
        state.on_event(Event::Entry(detail_with(
            1,
            "<p><a href='https://e.test/a'>a</a> <a href='https://e.test/b'>b</a></p>",
        )));
        char_key(&mut state, 'o');
        assert_eq!(state.overlay, Overlay::LinkPrompt);
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::OpenUrl("https://e.test/1".into())]
        );
        char_key(&mut state, 'o');
        assert_eq!(
            char_key(&mut state, '2'),
            [Command::OpenUrl("https://e.test/b".into())]
        );
        char_key(&mut state, 'o');
        assert!(char_key(&mut state, '9').is_empty());
        assert_eq!(state.message.as_deref(), Some("No link [9]."));
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn c_overrides_the_rhythm_of_the_entry_s_feed() {
        let mut state = reader();
        char_key(&mut state, 'c');
        assert_eq!(
            state.overlay,
            Overlay::Rhythm {
                feed_id: 7,
                choice: 0
            }
        );
        key(&mut state, KeyCode::Down);
        char_key(&mut state, 'j');
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::SetCadence(7, Some(CadenceGroup::Weekly))]
        );
        assert_eq!(state.overlay, Overlay::None);
        char_key(&mut state, 'c');
        assert!(key(&mut state, KeyCode::Esc).is_empty());
        assert_eq!(state.overlay, Overlay::None);
        // A forced rhythm: the menu asks for it and moves onto it, not left on Auto.
        assert_eq!(char_key(&mut state, 'c'), [Command::LoadCadence(7)]);
        state.on_event(Event::Cadence(RhythmInfo {
            feed_id: 7,
            effective: CadenceGroup::Monthly,
            computed: CadenceGroup::Daily,
            is_override: true,
            median_days: None,
            last_entry_at: None,
        }));
        assert_eq!(
            state.overlay,
            Overlay::Rhythm {
                feed_id: 7,
                choice: 3
            }
        );
        key(&mut state, KeyCode::Esc);
        let mut empty = ReaderState::new(TerminalSettings::default(), 140, 40);
        char_key(&mut empty, 'c');
        assert_eq!(empty.message.as_deref(), Some("No feed selected."));
    }

    #[test]
    fn t_and_i_change_the_settings() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 't'), [Command::SaveSettings]);
        assert_eq!(state.look(), Look::Antiope);
        char_key(&mut state, 't');
        assert_eq!(state.look(), Look::Native);
        assert_eq!(char_key(&mut state, 'i'), [Command::SaveSettings]);
        assert!(!state.settings.images);
    }

    #[test]
    fn q_ctrl_c_and_help() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 'q'), [Command::Quit]);
        let ctrl_c = state.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(ctrl_c, [Command::Quit]);
        assert!(char_key(&mut state, '?').is_empty());
        assert_eq!(state.overlay, Overlay::Help);
        assert!(char_key(&mut state, 'j').is_empty());
        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.selected_entry(), Some(1));
    }

    /// Review Focus 2: the app marks entries read; the reader reloads and keeps its place.
    #[test]
    fn a_reload_keeps_the_selected_entry() {
        let mut state = reader();
        char_key(&mut state, 'j');
        assert_eq!(
            state.on_event(Event::DataChanged),
            [
                Command::LoadSources,
                Command::LoadPages,
                Command::LoadJournal,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request: 2
                },
            ]
        );
        let reloaded = vec![row(3, 8, false), row(2, 7, true), row(1, 7, true)];
        assert!(
            state
                .on_event(entries(&state, Source::Journal, reloaded, None))
                .is_empty()
        );
        assert_eq!(state.selected_entry(), Some(2));
        assert_eq!(state.unread, 1);
    }

    /// Review Focus 3: 140 → 70 → 140 with an article open keeps the entry and the scroll.
    #[test]
    fn resizing_keeps_the_entry_and_the_scroll() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        char_key(&mut state, ' ');
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize {
            width: 70,
            height: 40,
        });
        assert_eq!(state.class(), LayoutClass::One);
        assert_eq!(state.focus(), Column::Reader);
        assert_eq!(state.selected_entry(), Some(1));
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize {
            width: 140,
            height: 40,
        });
        assert_eq!(state.class(), LayoutClass::Three);
        assert_eq!(state.focus(), Column::Reader);
        assert_eq!(state.scroll, 32);
    }

    #[test]
    fn a_shorter_pane_clamps_the_scroll() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        char_key(&mut state, ' ');
        char_key(&mut state, ' ');
        assert_eq!(state.scroll, 46);
        state.on_event(Event::Resize {
            width: 140,
            height: 80,
        });
        // 78 − 2 − 3 = 73 visible lines: 79 − 73 = 6 at most.
        assert_eq!(state.scroll, 6);
    }

    #[test]
    fn ticks_and_the_journal_are_stored() {
        let mut state = reader();
        let (now, local) = test_clock();
        let later = now + chrono::Duration::seconds(2);
        assert!(
            state
                .on_event(Event::Tick {
                    now: later,
                    local: local + chrono::Duration::seconds(2)
                })
                .is_empty()
        );
        assert_eq!(state.now, Some(later));
        assert_eq!(state.local, Some(local + chrono::Duration::seconds(2)));
        assert!(state.on_event(Event::Journal(None)).is_empty());
        assert!(state.journal.is_none());
        assert!(
            state
                .on_event(Event::ImageReady {
                    url: "https://e.test/i.png".into()
                })
                .is_empty()
        );
        assert!(
            state
                .on_event(Event::ImageFailed {
                    url: "https://e.test/i.png".into()
                })
                .is_empty()
        );
    }

    #[test]
    fn an_edition_outcome_reloads_the_journal() {
        let mut state = reader();
        let reload = |request| {
            [
                Command::LoadSources,
                Command::LoadPages,
                Command::LoadJournal,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request,
                },
            ]
        };
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::EditionBuilt)),
            reload(2)
        );
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::EditionNotDue)),
            reload(3)
        );
        assert_eq!(state.message, None);
    }

    #[test]
    fn other_screens_drive_the_state() {
        let mut state = reader();
        assert_eq!(
            state.go_to(Source::Starred),
            [Command::LoadEntries {
                source: Source::Starred,
                filter: ListFilter::All,
                after: None,
                request: 2
            }]
        );
        assert_eq!(state.source(), Source::Starred);
        assert!(state.go_to(Source::Starred).is_empty());
        state.on_event(entries(
            &state,
            Source::Starred,
            vec![row(5, 7, true), row(6, 7, false)],
            None,
        ));
        assert_eq!(
            state.select_entry(6),
            [Command::LoadEntry(6), Command::SetRead(6, true)]
        );
        assert_eq!(state.selected_entry(), Some(6));
        assert!(state.select_entry(99).is_empty());
        state.set_focus(Column::Sources);
        assert_eq!(state.focus(), Column::Sources);
    }

    /// A window that shrinks below the minimum and grows back keeps the entry and the scroll.
    #[test]
    fn a_too_small_window_keeps_the_scroll() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        char_key(&mut state, ' ');
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize {
            width: 30,
            height: 8,
        });
        assert_eq!(state.class(), LayoutClass::TooSmall);
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize {
            width: 140,
            height: 40,
        });
        assert_eq!(state.class(), LayoutClass::Three);
        assert_eq!(state.selected_entry(), Some(1));
        assert_eq!(state.scroll, 32);
    }

    /// The Two class shows the entries and the reader side by side: nothing to move.
    #[test]
    fn the_two_class_keeps_the_focus() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 3)));
        state.on_event(Event::Resize {
            width: 100,
            height: 40,
        });
        assert_eq!(state.focus(), Column::Entries);
        key(&mut state, KeyCode::Left);
        state.on_event(Event::Resize {
            width: 70,
            height: 40,
        });
        assert_eq!(state.focus(), Column::Sources);
    }

    /// Review Focus 2 under an unread filter: reading a row makes the next reload drop it from
    /// the list, the reader keeps showing it.
    #[test]
    fn a_reload_under_the_unread_filter_keeps_the_open_article() {
        let mut state = reader();
        char_key(&mut state, 'u');
        assert_eq!(state.filter, ListFilter::Unread);
        let unread = vec![row(1, 7, false), row(3, 8, false)];
        state.on_event(entries(&state, Source::Journal, unread, None));
        char_key(&mut state, 'j');
        assert_eq!(state.selected_entry(), Some(3));
        state.on_event(Event::Entry(detail(3, 3)));
        assert!(state.on_event(Event::DataChanged).len() >= 2);
        // The app (or this reader) read entry 3: the reloaded unread list no longer has it.
        let reloaded = vec![row(1, 7, false)];
        assert!(
            state
                .on_event(entries(&state, Source::Journal, reloaded, None))
                .is_empty()
        );
        assert_eq!(state.selected_entry(), Some(3));
        assert!(state.article.is_some());
        assert_eq!(state.detail.as_ref().map(|d| d.row.entry_id), Some(3));
        let ids: Vec<i64> = state.entries.iter().map(|r| r.entry_id).collect();
        assert_eq!(ids, [1, 3]);
        // Navigation carries on from the kept row; once away, the next reload drops it.
        assert_eq!(
            char_key(&mut state, 'k'),
            [Command::LoadEntry(1), Command::SetRead(1, true)]
        );
        state.on_event(Event::DataChanged);
        state.on_event(entries(
            &state,
            Source::Journal,
            vec![row(1, 7, false)],
            None,
        ));
        assert_eq!(state.entries.len(), 1);
    }

    #[test]
    fn the_next_page_is_asked_once_and_never_duplicated() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Unread;
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(1, 7, false)],
            Some(cursor()),
        ));
        let ask = Command::LoadEntries {
            source: Source::Unread,
            filter: ListFilter::All,
            after: Some(cursor()),
            request: state.request,
        };
        assert_eq!(char_key(&mut state, 'j'), std::slice::from_ref(&ask));
        assert!(char_key(&mut state, 'j').is_empty());
        assert!(char_key(&mut state, ' ').is_empty());
        let landed = |state: &ReaderState| Event::Entries {
            source: Source::Unread,
            page: page(vec![row(1, 7, false), row(2, 7, false)], Some(cursor())),
            appended: true,
            request: state.request,
        };
        // Space was waiting for the page: it carries on to the new unread row.
        assert_eq!(
            state.on_event(landed(&state)),
            [Command::LoadEntry(2), Command::SetRead(2, true)]
        );
        let ids: Vec<i64> = state.entries.iter().map(|r| r.entry_id).collect();
        assert_eq!(ids, [1, 2]);
        // The same page twice appends nothing; the flag is free again.
        assert!(state.on_event(landed(&state)).is_empty());
        assert_eq!(state.entries.len(), 2);
        assert_eq!(char_key(&mut state, 'j'), [ask]);
    }

    #[test]
    fn a_late_page_for_an_older_request_is_ignored() {
        let mut state = reader();
        key(&mut state, KeyCode::Left);
        let commands = key(&mut state, KeyCode::Down);
        let Some(Command::LoadEntries { request, .. }) = commands.first() else {
            panic!("expected a list load, got {commands:?}");
        };
        let request = *request;
        let stale = Event::Entries {
            source: Source::Unread,
            page: page(vec![row(9, 7, false)], None),
            appended: false,
            request: request - 1,
        };
        assert!(state.on_event(stale).is_empty());
        assert!(state.entries.is_empty());
        let fresh = Event::Entries {
            source: Source::Unread,
            page: page(vec![row(8, 7, false)], None),
            appended: false,
            request,
        };
        state.on_event(fresh);
        assert_eq!(state.entries[0].entry_id, 8);
        // A reload (data changed) supersedes a page request still in flight.
        let before = state.request;
        state.on_event(Event::DataChanged);
        assert!(state.request > before);
    }

    #[test]
    fn a_pending_advance_does_not_survive_a_new_list() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Unread;
        state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(1, 7, true)],
            Some(cursor()),
        ));
        char_key(&mut state, ' ');
        char_key(&mut state, 'u');
        let commands = state.on_event(entries(
            &state,
            Source::Unread,
            vec![row(5, 7, false)],
            None,
        ));
        assert_eq!(commands, [Command::LoadEntry(5)]);
    }

    #[test]
    fn the_search_ignores_chords() {
        let mut state = reader();
        char_key(&mut state, '/');
        state.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        state.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT));
        state.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT));
        char_key(&mut state, 'u');
        assert_eq!(state.search.as_deref(), Some("Ru"));
    }

    #[test]
    fn f_does_not_extract_twice() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 3)));
        assert_eq!(char_key(&mut state, 'f'), [Command::Extract(1)]);
        assert!(char_key(&mut state, 'f').is_empty());
        state.on_event(Event::JobDone(JobOutcome::Extracted {
            entry_id: 1,
            error: Some("No article found.".into()),
        }));
        assert_eq!(char_key(&mut state, 'f'), [Command::Extract(1)]);
    }

    #[test]
    fn u_applies_the_filter_to_the_search() {
        let mut state = reader();
        char_key(&mut state, '/');
        char_key(&mut state, 'r');
        key(&mut state, KeyCode::Enter);
        let commands = char_key(&mut state, 'u');
        assert_eq!(
            commands,
            [Command::Search {
                source: Source::Journal,
                text: "r".into(),
                filter: ListFilter::Unread,
                after: None,
                request: state.request,
            }]
        );
    }

    #[test]
    fn paging_inside_a_search_asks_the_search() {
        let mut state = reader();
        char_key(&mut state, '/');
        char_key(&mut state, 'r');
        key(&mut state, KeyCode::Enter);
        let hits = vec![row(1, 7, false), row(3, 8, false)];
        state.on_event(entries(
            &state,
            Source::Journal,
            hits.clone(),
            Some(cursor()),
        ));
        char_key(&mut state, 'j');
        let ask = Command::Search {
            source: Source::Journal,
            text: "r".into(),
            filter: ListFilter::All,
            after: Some(cursor()),
            request: state.request,
        };
        assert_eq!(char_key(&mut state, 'j'), std::slice::from_ref(&ask));
        assert!(char_key(&mut state, 'j').is_empty());
        let more = Event::Entries {
            source: Source::Journal,
            page: page(vec![row(3, 8, false), row(4, 8, false)], None),
            appended: true,
            request: state.request,
        };
        state.on_event(more);
        let ids: Vec<i64> = state.entries.iter().map(|r| r.entry_id).collect();
        assert_eq!(ids, [1, 3, 4]);
    }

    #[test]
    fn job_outcomes() {
        let mut state = reader();
        state.on_event(Event::Progress {
            label: "Fetching feeds".into(),
            done: 3,
            total: 9,
        });
        assert_eq!(state.progress, Some(("Fetching feeds".into(), 3, 9)));
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::Refreshed)),
            [
                Command::LoadSources,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request: 2
                },
            ]
        );
        assert_eq!(state.progress, None);
        assert!(
            state
                .on_event(Event::JobDone(JobOutcome::Offline))
                .is_empty()
        );
        assert_eq!(state.message.as_deref(), Some("Offline: no edition yet"));
        state.on_event(Event::JobDone(JobOutcome::Failed(
            "The database is busy.".into(),
        )));
        assert_eq!(state.message.as_deref(), Some("The database is busy."));
    }

    #[test]
    fn the_view_shows_what_the_state_holds() {
        let state = reader();
        let v = state.view();
        assert_eq!(v.look, Look::Native);
        assert_eq!(v.source, Source::Journal);
        assert_eq!(v.source_title, "Journal");
        assert_eq!(v.entries.len(), 4);
        assert_eq!(v.filter, ListFilter::All);
        assert!(
            v.source_lines
                .iter()
                .any(|l| l.heading && l.label == "Feeds")
        );
        assert!(
            v.source_lines
                .iter()
                .any(|l| l.label == "Daring Fireball" && l.depth == 1)
        );
        assert!(
            v.source_lines
                .iter()
                .any(|l| l.label == "Daily" && l.unread == 3)
        );
        assert_eq!(v.source_lines.iter().filter(|l| l.selected).count(), 1);
        assert_eq!(v.now_local.offset().local_minus_utc(), 7200);
    }

    #[test]
    fn the_view_neutralises_feed_titles() {
        let mut state = reader();
        let mut snapshot = sample_snapshot();
        if let ressac_core::store::SourceNodeKind::Folder { name, .. } = &mut snapshot.tree[0].kind
        {
            *name = "In\u{1b}]0;x\u{7}fos".into();
        }
        state.on_event(Event::Sources(snapshot));
        let v = state.view();
        assert!(
            v.source_lines
                .iter()
                .all(|l| !l.label.chars().any(char::is_control))
        );
    }

    #[test]
    fn reader_metrics_drive_space_scrolling() {
        let mut state = reader();
        char_key(&mut state, 'l');
        state.on_event(Event::Entry(detail(1, 3)));
        assert_eq!(state.view().reader_scroll, 0);
        // The renderer measured 120 rows (images make the article taller than its text lines).
        state.set_reader_metrics(120, 30);
        char_key(&mut state, ' ');
        assert_eq!(state.view().reader_scroll, 29);
        for _ in 0..3 {
            char_key(&mut state, ' ');
        }
        assert_eq!(state.view().reader_scroll, 90, "never past the last page");
        // At the end of the article Space moves on to the next unread entry.
        char_key(&mut state, ' ');
        assert_eq!(state.selected_entry(), Some(3));
        assert_eq!(state.view().reader_scroll, 0);
    }

    #[test]
    fn a_late_article_for_another_entry_is_ignored() {
        let mut state = reader();
        char_key(&mut state, 'l');
        assert_eq!(state.selected_entry(), Some(1));
        state.on_event(Event::Entry(detail(1, 3)));
        char_key(&mut state, 'j');
        assert_eq!(state.selected_entry(), Some(2));
        // The answer for entry 1 arrives after the reader moved on.
        state.on_event(Event::Entry(detail(1, 5)));
        assert!(state.detail.is_none(), "entry 2 is still loading");
        state.on_event(Event::Entry(detail(2, 3)));
        state.on_event(Event::Entry(detail(1, 5)));
        assert_eq!(state.detail.as_ref().map(|d| d.row.entry_id), Some(2));
    }

    /// Commands that read, mark or open an entry: none may come out of a key that acts on a
    /// list nobody can see.
    fn touches_entries(commands: &[Command]) -> bool {
        commands.iter().any(|c| {
            matches!(
                c,
                Command::SetRead(..)
                    | Command::SetStarred(..)
                    | Command::MarkAllRead(_)
                    | Command::LoadEntry(_)
            )
        })
    }

    /// Every key that acts on the selected entry or the list.
    const ENTRY_KEYS: [char; 9] = ['j', 'k', ' ', 'm', 's', 'A', 'u', 'f', 'o'];

    #[test]
    fn an_own_write_reloads_the_counts_and_not_the_list() {
        let mut state = reader();
        // On the Journal: its counts and the sources.
        assert_eq!(
            state.on_event(Event::OwnWrite),
            vec![Command::LoadSources, Command::LoadJournal]
        );
        state.go_to(Source::Unread);
        assert_eq!(state.on_event(Event::OwnWrite), vec![Command::LoadSources]);
        // The same change from outside still reloads everything and keeps the selection.
        assert!(state.on_event(Event::DataChanged).len() >= 3);
    }

    #[test]
    fn keys_do_not_act_on_the_list_behind_a_garden_page() {
        let mut state = reader();
        state.open_garden(GardenView::NeedsTriage);
        state.set_focus(Column::Sources);
        for c in ENTRY_KEYS {
            let commands = char_key(&mut state, c);
            assert!(!touches_entries(&commands), "{c}: {commands:?}");
        }
        assert!(state.entries.iter().filter(|r| r.is_read).count() == 2);
    }

    #[test]
    fn keys_do_not_act_on_the_list_when_one_column_shows_the_sources() {
        let mut state = reader();
        state.on_event(Event::Resize {
            width: 70,
            height: 40,
        });
        state.set_focus(Column::Sources);
        for c in ENTRY_KEYS {
            let commands = char_key(&mut state, c);
            assert!(!touches_entries(&commands), "{c}: {commands:?}");
        }
    }

    #[test]
    fn j_and_k_move_between_sources_in_the_sources_column() {
        let mut state = reader();
        state.set_focus(Column::Sources);
        let start = state.sidebar_index;
        char_key(&mut state, 'j');
        assert_eq!(state.sidebar_index, start + 1);
        char_key(&mut state, 'k');
        assert_eq!(state.sidebar_index, start);
    }

    #[test]
    fn a_too_small_window_keeps_only_q_ctrl_c_and_t() {
        let mut state = reader();
        state.on_event(Event::Resize {
            width: 30,
            height: 8,
        });
        for c in ENTRY_KEYS {
            assert_eq!(char_key(&mut state, c), Vec::new(), "{c}");
        }
        assert_eq!(key(&mut state, KeyCode::Down), Vec::new());
        assert_eq!(key(&mut state, KeyCode::Enter), Vec::new());
        assert_eq!(state.look(), Look::Native);
        assert_eq!(char_key(&mut state, 't'), vec![Command::SaveSettings]);
        assert_eq!(state.look(), Look::Antiope);
        assert_eq!(char_key(&mut state, 'q'), vec![Command::Quit]);
    }

    mod journal_tests {
        use super::*;
        use crate::tui::reader::journal::{BUILDING, JournalLine, JournalPhase};
        use chrono::TimeZone;
        use ressac_core::edition::Section;
        use ressac_core::store::{EditionOverflowRow, EditionRecord, JournalItem};

        fn paris() -> FixedOffset {
            FixedOffset::east_opt(2 * 3600).unwrap()
        }

        fn local(s: &str) -> DateTime<FixedOffset> {
            paris().from_local_datetime(&s.parse().unwrap()).unwrap()
        }

        fn tick(s: &str) -> Event {
            let l = local(s);
            Event::Tick {
                now: l.with_timezone(&Utc),
                local: l,
            }
        }

        fn started(at: &str) -> ReaderState {
            let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
            state.start(local(at).with_timezone(&Utc), local(at));
            state
        }

        /// Week: entries 10 and 11 of feed 7 (12 more hidden), entry 12 of feed 8;
        /// Wire: entry 13 of feed 9. Entries 10 and 13 are read when `first_read`.
        fn journal(first_read: bool) -> JournalView {
            let item = |id: i64, feed: i64, section, read| JournalItem {
                row: row(id, feed, read),
                section,
                rank: id as u32,
            };
            JournalView {
                edition: EditionRecord {
                    id: 1,
                    built_at: "2026-10-16T06:00:00Z".parse().unwrap(),
                    window_start: "2026-10-15T06:00:00Z".parse().unwrap(),
                    window_end: "2026-10-16T06:00:00Z".parse().unwrap(),
                    unreachable: 0,
                },
                items: vec![
                    item(10, 7, Section::Week, first_read),
                    item(11, 7, Section::Week, false),
                    item(12, 8, Section::Week, false),
                    item(13, 9, Section::Wire, first_read),
                ],
                overflow: vec![EditionOverflowRow {
                    section: Section::Week,
                    feed_id: 7,
                    feed_title: "Actualitté".into(),
                    page: Some(403),
                    hidden_count: 12,
                }],
            }
        }

        /// Started, with the Journal and its entries loaded; entry 10 selected.
        fn loaded(first_read: bool) -> ReaderState {
            let mut state = started("2026-10-16T08:00:00");
            let view = journal(first_read);
            let rows = view.items.iter().map(|i| i.row.clone()).collect();
            state.on_event(Event::Journal(Some(view)));
            state.on_event(entries(&state, Source::Journal, rows, None));
            state
        }

        #[test]
        fn a_placeholder_over_an_older_edition_blocks_the_reading_keys() {
            for phase in [
                Event::Progress {
                    label: BUILDING.into(),
                    done: 3,
                    total: 9,
                },
                Event::JobDone(JobOutcome::EditionOffline),
            ] {
                let mut state = loaded(false);
                state.on_event(phase);
                assert!(!matches!(state.journal_phase(), JournalPhase::Idle));
                for c in ENTRY_KEYS {
                    let commands = char_key(&mut state, c);
                    assert!(!touches_entries(&commands), "{c}: {commands:?}");
                }
                assert!(
                    state
                        .journal()
                        .unwrap()
                        .items
                        .iter()
                        .all(|i| !i.row.is_read)
                );
            }
        }

        fn loads_entries_of(commands: &[Command], wanted: Source) -> bool {
            commands
                .iter()
                .any(|c| matches!(c, Command::LoadEntries { source, .. } if *source == wanted))
        }

        #[test]
        fn a_session_starts_with_sources_journal_and_a_build() {
            let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
            let at = "2026-10-16T08:00:00";
            let commands = state.start(local(at).with_timezone(&Utc), local(at));
            assert_eq!(commands.first(), Some(&Command::LoadSources));
            assert!(commands.contains(&Command::LoadJournal));
            assert_eq!(commands.last(), Some(&Command::BuildEdition));
            assert_eq!(state.source(), Source::Journal);
            // Not shown as building until the refresh reports: a not-due build returns at once.
            assert_eq!(state.journal_phase(), &JournalPhase::Idle);
        }

        #[test]
        fn progress_shows_the_build_and_no_second_build_starts() {
            let mut state = started("2026-10-16T08:00:00");
            state.on_event(Event::Progress {
                label: BUILDING.into(),
                done: 3,
                total: 156,
            });
            assert_eq!(
                state.journal_phase(),
                &JournalPhase::Building {
                    done: 3,
                    total: 156
                }
            );
            // The rollover while the first build runs asks for nothing.
            assert!(
                !state
                    .on_event(tick("2026-10-17T04:00:01"))
                    .contains(&Command::BuildEdition)
            );
            // Other jobs' progress is not the Journal's.
            let mut other = started("2026-10-16T08:00:00");
            other.on_event(Event::Progress {
                label: "Fetching feeds".into(),
                done: 1,
                total: 2,
            });
            assert_eq!(other.journal_phase(), &JournalPhase::Idle);
        }

        /// Review Focus 5: the reader stays open across 04:00, the app builds first, the reader's
        /// build is turned away by the core's due re-check, the Journal reloads, no error shows.
        #[test]
        fn across_four_the_reader_reloads_quietly_when_the_app_built_first() {
            let mut state = started("2026-10-16T22:00:00");
            state.on_event(Event::JobDone(JobOutcome::EditionNotDue));
            assert!(
                state.on_event(tick("2026-10-17T03:59:59")).is_empty(),
                "the 04:00 day has not changed"
            );
            let at_four = state.on_event(tick("2026-10-17T04:00:00"));
            assert_eq!(at_four, [Command::BuildEdition]);
            // The same morning's later ticks start nothing.
            assert!(state.on_event(tick("2026-10-17T04:00:01")).is_empty());
            state.on_event(Event::Progress {
                label: BUILDING.into(),
                done: 0,
                total: 156,
            });
            let done = state.on_event(Event::JobDone(JobOutcome::EditionNotDue));
            assert!(done.contains(&Command::LoadJournal));
            assert!(done.contains(&Command::LoadSources));
            assert_eq!(state.journal_phase(), &JournalPhase::Idle);
            assert_eq!(state.message, None);
            assert_eq!(state.progress, None);
        }

        #[test]
        fn the_rollover_is_the_zone_s_04_00_not_midnight() {
            let mut state = started("2026-10-16T22:00:00");
            state.on_event(Event::JobDone(JobOutcome::EditionNotDue));
            assert!(state.on_event(tick("2026-10-17T00:00:00")).is_empty());
            assert!(state.on_event(tick("2026-10-17T02:30:00")).is_empty());
            assert_eq!(
                state.on_event(tick("2026-10-17T04:00:00")),
                [Command::BuildEdition]
            );
        }

        #[test]
        fn offline_and_failed_offer_r_as_retry() {
            let mut state = started("2026-10-16T08:00:00");
            state.on_event(Event::JobDone(JobOutcome::EditionOffline));
            assert_eq!(state.journal_phase(), &JournalPhase::Offline);
            assert_eq!(
                state.message, None,
                "the placeholder says it, not the top line"
            );
            assert_eq!(char_key(&mut state, 'r'), [Command::BuildEdition]);
            // Asked once: a second `r` while the build is out does nothing.
            assert!(char_key(&mut state, 'r').is_empty());
            state.on_event(Event::JobDone(JobOutcome::EditionFailed(
                "disk full".into(),
            )));
            assert_eq!(
                state.journal_phase(),
                &JournalPhase::Failed("disk full".into())
            );
            assert_eq!(char_key(&mut state, 'r'), [Command::BuildEdition]);
            state.on_event(Event::JobDone(JobOutcome::EditionBuilt));
            assert_eq!(state.journal_phase(), &JournalPhase::Idle);
            // Idle: `r` is the ordinary refresh again.
            assert_eq!(
                char_key(&mut state, 'r'),
                [Command::Refresh(Some(Source::Journal))]
            );
        }

        #[test]
        fn r_elsewhere_is_a_refresh_even_when_the_journal_is_offline() {
            let mut state = started("2026-10-16T08:00:00");
            state.on_event(Event::JobDone(JobOutcome::EditionOffline));
            state.go_to(Source::Unread);
            assert_eq!(
                char_key(&mut state, 'r'),
                [Command::Refresh(Some(Source::Unread))]
            );
        }

        /// A quick command's failure (a list that could not load, a write that failed) and a
        /// refresh's outcome are not the edition's.
        #[test]
        fn only_the_edition_job_s_outcome_moves_the_phase() {
            let mut state = started("2026-10-16T08:00:00");
            state.on_event(Event::Progress {
                label: BUILDING.into(),
                done: 1,
                total: 4,
            });
            state.on_event(Event::JobDone(JobOutcome::Failed(
                "The database is busy.".into(),
            )));
            state.on_event(Event::JobDone(JobOutcome::Offline));
            assert_eq!(
                state.journal_phase(),
                &JournalPhase::Building { done: 1, total: 4 }
            );
            // The edition is still out: no second build, then its own failure lands.
            assert!(
                char_key(&mut state, 'r')
                    .iter()
                    .all(|c| *c != Command::BuildEdition)
            );
            state.on_event(Event::JobDone(JobOutcome::EditionFailed("boom".into())));
            assert_eq!(state.journal_phase(), &JournalPhase::Failed("boom".into()));
        }

        #[test]
        fn a_built_edition_reloads_the_journal_and_the_counts() {
            let mut state = started("2026-10-16T08:00:00");
            state.on_event(Event::Progress {
                label: BUILDING.into(),
                done: 2,
                total: 2,
            });
            let commands = state.on_event(Event::JobDone(JobOutcome::EditionBuilt));
            assert!(commands.contains(&Command::LoadJournal));
            assert!(commands.contains(&Command::LoadSources));
            assert!(loads_entries_of(&commands, Source::Journal));
            assert_eq!(state.journal_phase(), &JournalPhase::Idle);
        }

        #[test]
        fn the_journal_lines_follow_the_view() {
            let mut state = loaded(false);
            let v = state.view();
            assert_eq!(
                v.journal_lines,
                [
                    JournalLine::Heading(Section::Week),
                    JournalLine::Entry(0),
                    JournalLine::Entry(1),
                    JournalLine::Overflow(0),
                    JournalLine::Entry(2),
                    JournalLine::Heading(Section::Wire),
                    JournalLine::Entry(3),
                ]
            );
            assert_eq!(v.journal_cursor, Some(1));
            // Another source, a filter or a search: the plain list, no Journal lines.
            char_key(&mut state, 'u');
            assert!(state.view().journal_lines.is_empty());
            char_key(&mut state, 'u');
            assert!(!state.view().journal_lines.is_empty());
        }

        #[test]
        fn down_and_up_walk_the_lines_and_stop_on_overflow_rows() {
            let mut state = loaded(false);
            assert_eq!(state.selected_entry(), Some(10));
            key(&mut state, KeyCode::Down);
            assert_eq!(state.selected_entry(), Some(11));
            // The overflow row selects nothing: the reader has no entry.
            let commands = key(&mut state, KeyCode::Down);
            assert!(commands.is_empty());
            assert_eq!(state.selected_entry(), None);
            assert_eq!(state.view().journal_cursor, Some(3));
            // Down again: over the next entry; its heading is skipped when crossed.
            key(&mut state, KeyCode::Down);
            assert_eq!(state.selected_entry(), Some(12));
            key(&mut state, KeyCode::Down);
            assert_eq!(state.selected_entry(), Some(13));
            assert_eq!(state.view().journal_cursor, Some(6));
            key(&mut state, KeyCode::Up);
            key(&mut state, KeyCode::Up);
            assert_eq!(state.view().journal_cursor, Some(3));
            key(&mut state, KeyCode::Up);
            assert_eq!(state.selected_entry(), Some(11));
        }

        #[test]
        fn j_and_k_skip_the_overflow_rows() {
            let mut state = loaded(false);
            char_key(&mut state, 'j');
            assert_eq!(state.selected_entry(), Some(11));
            char_key(&mut state, 'j');
            assert_eq!(state.selected_entry(), Some(12), "over the overflow row");
            char_key(&mut state, 'k');
            assert_eq!(state.selected_entry(), Some(11));
            // From the overflow row: j goes on after it, k back to the entry before it.
            key(&mut state, KeyCode::Down);
            assert_eq!(state.selected_entry(), None);
            char_key(&mut state, 'k');
            assert_eq!(state.selected_entry(), Some(11));
            key(&mut state, KeyCode::Down);
            char_key(&mut state, 'j');
            assert_eq!(state.selected_entry(), Some(12));
            // The ends do not wrap.
            char_key(&mut state, 'j');
            assert_eq!(state.selected_entry(), Some(13));
            assert!(char_key(&mut state, 'j').is_empty());
            assert_eq!(state.selected_entry(), Some(13));
        }

        #[test]
        fn enter_on_an_overflow_row_opens_its_feed() {
            let mut state = loaded(false);
            key(&mut state, KeyCode::Down);
            key(&mut state, KeyCode::Down);
            let commands = key(&mut state, KeyCode::Enter);
            assert_eq!(state.source(), Source::Feed(7));
            assert!(loads_entries_of(&commands, Source::Feed(7)));
            assert_eq!(state.view().journal_cursor, None);
        }

        #[test]
        fn enter_on_an_entry_still_opens_the_reader() {
            let mut state = loaded(false);
            key(&mut state, KeyCode::Enter);
            assert_eq!(state.source(), Source::Journal);
            assert_eq!(state.focus(), Column::Reader);
        }

        #[test]
        fn the_overflow_cursor_survives_a_reload_and_goes_with_its_row() {
            let mut state = loaded(false);
            key(&mut state, KeyCode::Down);
            key(&mut state, KeyCode::Down);
            let view = journal(false);
            let rows: Vec<_> = view.items.iter().map(|i| i.row.clone()).collect();
            state.on_event(Event::Journal(Some(view.clone())));
            state.on_event(entries(&state, Source::Journal, rows.clone(), None));
            assert_eq!(
                state.view().journal_cursor,
                Some(3),
                "still on the overflow row"
            );
            assert_eq!(
                state.selected_entry(),
                None,
                "and nothing selected behind it"
            );
            // The row is gone from the next edition: the cursor falls back to an entry.
            let mut next = view;
            next.overflow.clear();
            state.on_event(Event::Journal(Some(next)));
            state.on_event(entries(&state, Source::Journal, rows, None));
            assert_eq!(state.selected_entry(), Some(10));
            assert_eq!(state.view().journal_cursor, Some(1));
        }

        #[test]
        fn space_after_an_overflow_row_goes_to_the_next_unread_entry() {
            let mut state = loaded(true);
            // Entries 10 and 13 are read: from the overflow row, the next unread is 12.
            key(&mut state, KeyCode::Down);
            key(&mut state, KeyCode::Down);
            assert_eq!(state.selected_entry(), None);
            char_key(&mut state, ' ');
            assert_eq!(state.selected_entry(), Some(12));
        }

        #[test]
        fn the_finished_edition_says_so_on_space() {
            let mut state = loaded(false);
            for e in &mut state.entries {
                e.is_read = true;
            }
            state.selected = Some(13);
            char_key(&mut state, ' ');
            assert_eq!(state.message.as_deref(), Some("That's the edition."));
        }

        #[test]
        fn leaving_the_journal_forgets_the_overflow_cursor() {
            let mut state = loaded(false);
            key(&mut state, KeyCode::Down);
            key(&mut state, KeyCode::Down);
            state.go_to(Source::Unread);
            state.go_to(Source::Journal);
            assert_eq!(state.view().journal_cursor, None);
        }

        // Antiope's page navigation (spec §5, §6).

        fn antiope() -> ReaderState {
            let mut state = loaded(false);
            state.on_event(Event::Pages(
                crate::tui::reader::pages::tests::sample_pages(),
            ));
            state.settings.look = Look::Antiope;
            state
        }

        fn type_page(state: &mut ReaderState, digits: &str) -> Vec<Command> {
            let mut commands = Vec::new();
            for c in digits.chars() {
                commands = char_key(state, c);
            }
            commands
        }

        #[test]
        fn typing_412_in_antiope_opens_the_feed() {
            let mut state = antiope();
            let commands = type_page(&mut state, "412");
            assert_eq!(state.source(), Source::Feed(12));
            assert!(
                loads_entries_of(&commands, Source::Feed(12)),
                "{commands:?}"
            );
            assert_eq!(state.focus(), Column::Entries);
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
        }

        #[test]
        fn digits_do_nothing_in_native() {
            let mut state = loaded(false);
            state.on_event(Event::Pages(
                crate::tui::reader::pages::tests::sample_pages(),
            ));
            assert!(type_page(&mut state, "412").is_empty());
            assert_eq!(state.source(), Source::Journal);
            assert_eq!(state.antiope.typing(), None);
        }

        #[test]
        fn the_rhythm_pages_and_the_garden_pages_open_by_number() {
            let mut state = antiope();
            type_page(&mut state, "106");
            assert_eq!(state.source(), Source::Cadence(CadenceGroup::Weekly));
            let commands = type_page(&mut state, "801");
            assert_eq!(state.garden_view(), Some(GardenView::AllFeeds));
            assert!(commands.contains(&Command::LoadGarden), "{commands:?}");
            assert_eq!(state.focus(), Column::Entries);
            // A reading page takes the garden down again.
            type_page(&mut state, "101");
            assert_eq!(state.garden_view(), None);
            assert_eq!(state.source(), Source::Unread);
        }

        #[test]
        fn sommaire_and_guide_open_in_the_reader_and_leave_the_garden() {
            let mut state = antiope();
            type_page(&mut state, "100");
            assert_eq!(state.antiope.reader_page(), ReaderPage::Sommaire);
            assert_eq!(state.focus(), Column::Reader);
            type_page(&mut state, "800");
            assert_eq!(state.garden_view(), Some(GardenView::NeedsTriage));
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
            // `?` over a garden page: the GUIDE shows and the garden's keys stop.
            char_key(&mut state, '?');
            assert_eq!(state.antiope.reader_page(), ReaderPage::Guide);
            assert_eq!(state.garden_view(), None);
            assert_eq!(state.overlay, Overlay::None);
            char_key(&mut state, 'S');
            assert_eq!(state.antiope.reader_page(), ReaderPage::Sommaire);
        }

        #[test]
        fn native_help_is_untouched_by_antiope() {
            let mut state = loaded(false);
            char_key(&mut state, '?');
            assert_eq!(state.overlay, Overlay::Help);
            let mut state = antiope();
            char_key(&mut state, '?');
            assert_eq!(state.overlay, Overlay::None);
        }

        #[test]
        fn backspace_and_escape_correct_and_cancel_the_number() {
            let mut state = antiope();
            type_page(&mut state, "41");
            assert_eq!(state.antiope.typing(), Some("41_".into()));
            key(&mut state, KeyCode::Backspace);
            assert_eq!(state.antiope.typing(), Some("4__".into()));
            key(&mut state, KeyCode::Esc);
            assert_eq!(state.antiope.typing(), None);
            assert_eq!(state.source(), Source::Journal, "nothing was jumped to");
        }

        #[test]
        fn an_unknown_page_changes_nothing_but_the_header() {
            let mut state = antiope();
            let before = state.selected_entry();
            assert!(type_page(&mut state, "437").is_empty());
            assert_eq!(state.source(), Source::Journal);
            assert_eq!(state.selected_entry(), before);
            let at = state.now.unwrap_or(DateTime::UNIX_EPOCH);
            assert_eq!(state.antiope.not_found(at), Some(437));
        }

        #[test]
        fn the_search_and_the_link_prompt_keep_their_digits() {
            let mut state = antiope();
            char_key(&mut state, '/');
            type_page(&mut state, "412");
            assert_eq!(state.search.as_deref(), Some("412"));
            assert_eq!(state.antiope.typing(), None);
            assert_eq!(state.source(), Source::Journal);
            key(&mut state, KeyCode::Esc);
            state.on_event(Event::Entry(detail(10, 3)));
            char_key(&mut state, 'o');
            char_key(&mut state, '1');
            assert_eq!(
                state.antiope.typing(),
                None,
                "1 is link 1, not a page digit"
            );
        }

        #[test]
        fn suite_walks_the_journal_by_rank_then_reaches_fin_de_l_edition() {
            let mut state = antiope();
            assert_eq!(state.selected_entry(), Some(10));
            for expected in [11, 12, 13] {
                char_key(&mut state, 'j');
                assert_eq!(state.selected_entry(), Some(expected));
                assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
            }
            let commands = char_key(&mut state, 'j');
            assert!(commands.is_empty(), "13 was read on arrival: {commands:?}");
            assert_eq!(state.antiope.reader_page(), ReaderPage::EndOfEdition);
            assert!(char_key(&mut state, 'j').is_empty());
            assert_eq!(state.antiope.reader_page(), ReaderPage::EndOfEdition);
            char_key(&mut state, 'k');
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
            assert_eq!(state.selected_entry(), Some(13));
        }

        #[test]
        fn space_on_the_last_entry_scrolls_then_lands_on_the_end_and_marks_it_read() {
            let mut state = antiope();
            for e in &mut state.entries {
                e.is_read = true;
            }
            state.entries[3].is_read = false;
            state.selected = Some(13);
            state.on_event(Event::Entry(detail(13, 40)));
            state.set_reader_metrics(79, 20);
            char_key(&mut state, ' ');
            assert_eq!(
                state.antiope.reader_page(),
                ReaderPage::Article,
                "scrolled first"
            );
            assert!(state.scroll > 0);
            state.scroll = state.max_scroll();
            let commands = char_key(&mut state, ' ');
            assert_eq!(state.antiope.reader_page(), ReaderPage::EndOfEdition);
            assert_eq!(commands, [Command::SetRead(13, true)]);
            // Native keeps its own end: a message.
            let mut native = loaded(false);
            for e in &mut native.entries {
                e.is_read = true;
            }
            native.selected = Some(13);
            char_key(&mut native, ' ');
            assert_eq!(native.antiope.reader_page(), ReaderPage::Article);
        }

        #[test]
        fn another_entry_or_list_brings_the_article_back() {
            let mut state = antiope();
            state.on_event(Event::Sources(sample_snapshot()));
            type_page(&mut state, "100");
            assert_eq!(state.antiope.reader_page(), ReaderPage::Sommaire);
            // Choosing a reading source from the index column leaves the page.
            state.set_focus(Column::Sources);
            key(&mut state, KeyCode::Down);
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
        }

        #[test]
        fn j_k_and_space_scroll_the_special_pages_and_suite_at_the_bottom_leaves_them() {
            let mut state = antiope();
            type_page(&mut state, "899");
            state.set_reader_metrics(30, 10);
            char_key(&mut state, 'j');
            assert_eq!(state.page_scroll, 1);
            char_key(&mut state, ' ');
            assert_eq!(
                state.page_scroll, 10,
                "a page less the overlap, then the bound"
            );
            char_key(&mut state, 'k');
            assert_eq!(state.page_scroll, 9);
            state.page_scroll = 20;
            char_key(&mut state, 'j');
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
            assert_eq!(state.page_scroll, 0);
            assert_eq!(state.selected_entry(), Some(10), "the entry did not move");
        }

        #[test]
        fn the_look_switch_keeps_the_selection_and_forgets_the_page() {
            let mut state = antiope();
            type_page(&mut state, "100");
            state.page_scroll = 3;
            let selected = state.selected_entry();
            char_key(&mut state, 't');
            assert_eq!(state.look(), Look::Native);
            assert_eq!(state.selected_entry(), selected);
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
            assert_eq!(state.page_scroll, 0);
            char_key(&mut state, 't');
            assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
        }

        #[test]
        fn r_retries_an_offline_edition_in_antiope() {
            let mut state = antiope();
            state.edition_job = false;
            state.on_event(Event::JobDone(JobOutcome::EditionOffline));
            assert_eq!(state.journal_phase(), &JournalPhase::Offline);
            assert!(char_key(&mut state, 'r').contains(&Command::BuildEdition));
        }

        #[test]
        fn i_toggles_the_real_images_on_an_article_only_and_not_the_setting() {
            let mut state = antiope();
            state.on_event(Event::Entry(detail(10, 3)));
            let images = state.settings.images;
            assert!(char_key(&mut state, 'i').is_empty());
            assert!(state.antiope_real_images);
            assert_eq!(state.settings.images, images);
            char_key(&mut state, 'i');
            assert!(!state.antiope_real_images);
            type_page(&mut state, "100");
            char_key(&mut state, 'i');
            assert!(!state.antiope_real_images);
            // Native's `i` is still the setting.
            let mut native = loaded(false);
            assert_eq!(char_key(&mut native, 'i'), [Command::SaveSettings]);
        }

        #[test]
        fn the_directory_is_rebuilt_when_the_pages_or_the_sources_arrive() {
            let mut state = loaded(false);
            assert!(state.directory.rows().is_empty());
            state.on_event(Event::Pages(
                crate::tui::reader::pages::tests::sample_pages(),
            ));
            let rows = state.directory.rows().len();
            assert!(rows > 14);
            let mut snapshot = crate::tui::reader::sidebar::sample_snapshot();
            snapshot.counts.unread = 77;
            state.on_event(Event::Sources(snapshot));
            let unread = state
                .directory
                .rows()
                .iter()
                .find(|r| r.page == 101)
                .and_then(|r| r.count);
            assert_eq!(unread, Some(77));
            // Another event leaves it as it is.
            let kept = state.directory.clone();
            state.on_event(Event::Tick {
                now: state.now.unwrap(),
                local: state.local.unwrap(),
            });
            assert_eq!(state.directory, kept);
        }
    }
}

#[cfg(test)]
mod garden_tests {
    use super::*;
    use crate::tui::reader::sidebar::sample_snapshot;
    use crate::tui::state::{Mode, sample_rows, sample_rows_with_buried};
    use ressac_core::model::Decision;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn fresh() -> ReaderState {
        ReaderState::new(
            TerminalSettings {
                look: Look::Native,
                images: true,
            },
            140,
            40,
        )
    }

    fn on_needs_triage() -> ReaderState {
        let mut s = fresh();
        let commands = s.open_garden(GardenView::NeedsTriage);
        assert!(commands.contains(&Command::LoadGarden));
        s.on_event(Event::GardenRows(sample_rows()));
        s
    }

    #[test]
    fn a_garden_destination_shows_its_filter() {
        let s = on_needs_triage();
        assert_eq!(s.garden_view(), Some(GardenView::NeedsTriage));
        assert_eq!(s.garden().unwrap().filter, garden_state::Filter::Queue);
        for (view, filter) in [
            (GardenView::AllFeeds, garden_state::Filter::All),
            (GardenView::Cemetery, garden_state::Filter::Cemetery),
        ] {
            let mut s = fresh();
            s.open_garden(view);
            s.on_event(Event::GardenRows(sample_rows_with_buried()));
            assert_eq!(s.garden().unwrap().filter, filter);
            assert_eq!(s.view().source_title, view.title());
        }
    }

    #[test]
    fn the_garden_s_keys_win_on_a_garden_page() {
        let mut s = on_needs_triage();
        // k keeps (not "previous entry"), u undoes (not the unread filter).
        let keep = s.on_key(key(KeyCode::Char('k')));
        assert!(matches!(
            keep.as_slice(),
            [Command::Garden(garden_state::Command::Decide(
                _,
                Decision::Keep
            ))]
        ));
        assert_eq!(
            s.on_key(key(KeyCode::Char('u'))),
            vec![Command::Garden(garden_state::Command::Undo)]
        );
        assert_eq!(
            s.filter,
            ListFilter::All,
            "the reader's filter is untouched"
        );
        let before = s.garden().unwrap().selected;
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.garden().unwrap().selected, before + 1);
    }

    #[test]
    fn q_quits_the_reader_and_esc_goes_back_to_the_sources() {
        let mut s = on_needs_triage();
        assert_eq!(s.on_key(key(KeyCode::Esc)), Vec::<Command>::new());
        assert_eq!(s.focus(), Column::Sources);
        let mut s = on_needs_triage();
        assert_eq!(s.on_key(key(KeyCode::Char('q'))), vec![Command::Quit]);
    }

    #[test]
    fn esc_closes_the_garden_s_detail_first() {
        let mut s = on_needs_triage();
        s.on_key(key(KeyCode::Enter));
        assert_eq!(s.garden().unwrap().mode, Mode::Detail);
        s.on_key(key(KeyCode::Esc));
        assert_eq!(s.garden().unwrap().mode, Mode::Queue);
        assert_ne!(s.focus(), Column::Sources);
        // In the detail, q still quits and Left does not leave the page.
        s.on_key(key(KeyCode::Enter));
        s.on_key(key(KeyCode::Left));
        assert_ne!(s.focus(), Column::Sources);
    }

    #[test]
    fn rows_reload_after_a_decision_keep_the_position() {
        let mut s = on_needs_triage();
        s.on_key(key(KeyCode::Down));
        let selected = s.garden().unwrap().selected;
        s.on_event(Event::GardenRows(sample_rows()));
        assert_eq!(s.garden().unwrap().selected, selected);
        s.on_event(Event::GardenMessage(Some("Restored Re/code.".into())));
        assert_eq!(
            s.garden().unwrap().message.as_deref(),
            Some("Restored Re/code.")
        );
        s.on_event(Event::GardenMessage(None));
        assert_eq!(s.garden().unwrap().message, None);
    }

    #[test]
    fn t_and_left_still_belong_to_the_reader() {
        let mut s = on_needs_triage();
        assert!(
            s.on_key(key(KeyCode::Char('t')))
                .contains(&Command::SaveSettings)
        );
        assert_eq!(s.look(), Look::Antiope);
        s.on_key(key(KeyCode::Left));
        assert_eq!(s.focus(), Column::Sources);
    }

    #[test]
    fn until_the_rows_arrive_only_the_way_out_works() {
        let mut s = fresh();
        s.open_garden(GardenView::Cemetery);
        assert!(s.garden().is_none());
        assert!(s.on_key(key(KeyCode::Char('j'))).is_empty());
        assert!(s.on_key(key(KeyCode::Char('m'))).is_empty());
        assert_eq!(s.on_key(key(KeyCode::Char('q'))), vec![Command::Quit]);
        s.on_key(key(KeyCode::Esc));
        assert_eq!(s.focus(), Column::Sources);
    }

    #[test]
    fn the_sources_column_walks_into_the_garden_pages() {
        let mut s = fresh();
        s.on_event(Event::Sources(sample_snapshot()));
        s.on_key(key(KeyCode::Left));
        // Arrowing onto a garden page shows it, the focus stays on the sources column.
        for _ in 0..10 {
            s.on_key(key(KeyCode::Down));
        }
        assert_eq!(s.sidebar_index, 12);
        assert_eq!(s.on_key(key(KeyCode::Down)), vec![Command::LoadGarden]);
        assert_eq!(s.sidebar_index, 14);
        assert_eq!(s.garden_view(), Some(GardenView::NeedsTriage));
        assert_eq!(s.focus(), Column::Sources);
        s.on_event(Event::GardenRows(sample_rows()));
        // The sources reload (a decision changed the counts) keeps the cursor on the page.
        s.on_event(Event::Sources(sample_snapshot()));
        assert_eq!(s.sidebar_index, 14);
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.garden_view(), Some(GardenView::AllFeeds));
        assert!(s.garden().is_none(), "the next page waits for its own rows");
        // Enter moves into it.
        s.on_key(key(KeyCode::Enter));
        assert_eq!(s.focus(), Column::Entries);
    }

    #[test]
    fn choosing_a_reading_source_leaves_the_garden() {
        let mut s = on_needs_triage();
        s.on_event(Event::Sources(sample_snapshot()));
        s.on_key(key(KeyCode::Left));
        let commands = s.on_key(key(KeyCode::Up));
        assert_eq!(s.garden_view(), None);
        assert!(s.garden().is_none());
        assert!(!commands.is_empty());
        assert!(matches!(
            s.on_event(Event::GardenRows(sample_rows())).as_slice(),
            []
        ));
        assert!(s.garden().is_none(), "late rows of a left page are dropped");
    }

    #[test]
    fn coming_back_to_the_interrupted_source_reloads_its_list() {
        let mut s = fresh();
        s.on_event(Event::Sources(sample_snapshot()));
        let before = s.source();
        s.open_garden(GardenView::Cemetery);
        let commands = s.go_to(before);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::LoadEntries { .. }))
        );
        assert_eq!(s.garden_view(), None);
    }

    #[test]
    fn a_data_change_reloads_the_garden_too() {
        let mut s = on_needs_triage();
        assert!(
            s.on_event(Event::DataChanged)
                .contains(&Command::LoadGarden)
        );
        let mut r = fresh();
        assert!(
            !r.on_event(Event::DataChanged)
                .contains(&Command::LoadGarden)
        );
    }
}

#[cfg(test)]
mod rhythm_tests {
    use super::*;
    use crate::tui::state::{sample_rows, sample_rows_with_buried};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn fresh() -> ReaderState {
        ReaderState::new(
            TerminalSettings {
                look: Look::Native,
                images: true,
            },
            140,
            40,
        )
    }

    fn on_needs_triage() -> ReaderState {
        let mut s = fresh();
        s.open_garden(GardenView::NeedsTriage);
        s.on_event(Event::GardenRows(sample_rows()));
        s
    }

    fn info(feed_id: FeedId, effective: CadenceGroup, is_override: bool) -> RhythmInfo {
        RhythmInfo {
            feed_id,
            effective,
            computed: CadenceGroup::Weekly,
            is_override,
            median_days: Some(1.5),
            last_entry_at: None,
        }
    }

    fn selected_feed(s: &ReaderState) -> FeedId {
        s.garden().unwrap().selected_row().unwrap().feed_id
    }

    #[test]
    fn moving_in_the_garden_loads_the_selected_feed_s_rhythm() {
        let mut s = fresh();
        s.open_garden(GardenView::NeedsTriage);
        // The rows arrive: the first feed's rhythm is asked for.
        let commands = s.on_event(Event::GardenRows(sample_rows()));
        let first = selected_feed(&s);
        assert!(commands.contains(&Command::LoadCadence(first)));
        // A fresh load of the same rows asks nothing more.
        let again = s.on_event(Event::GardenRows(sample_rows()));
        assert!(!again.iter().any(|c| matches!(c, Command::LoadCadence(_))));
        let commands = s.on_key(key(KeyCode::Down));
        let second = selected_feed(&s);
        assert_ne!(first, second);
        assert!(commands.contains(&Command::LoadCadence(second)));
    }

    #[test]
    fn a_rhythm_of_another_feed_is_dropped_and_not_drawn() {
        let mut s = on_needs_triage();
        let first = selected_feed(&s);
        s.on_event(Event::Cadence(info(first, CadenceGroup::Weekly, false)));
        assert!(s.rhythm().is_some());
        s.on_key(key(KeyCode::Down));
        assert!(s.rhythm().is_none());
        // A late answer for the feed left behind is ignored.
        s.on_event(Event::Cadence(info(first, CadenceGroup::Weekly, false)));
        assert!(s.rhythm().is_none());
    }

    #[test]
    fn c_opens_the_menu_and_enter_sets_the_rhythm() {
        let mut s = on_needs_triage();
        let feed = selected_feed(&s);
        s.on_event(Event::Cadence(info(feed, CadenceGroup::Weekly, false)));
        assert!(s.on_key(key(KeyCode::Char('c'))).is_empty());
        assert_eq!(
            s.overlay,
            Overlay::Rhythm {
                feed_id: feed,
                choice: 0
            }
        );
        // The menu has the keys now: down then Enter forces Daily.
        s.on_key(key(KeyCode::Down));
        assert_eq!(
            s.on_key(key(KeyCode::Enter)),
            vec![Command::SetCadence(feed, Some(CadenceGroup::Daily))]
        );
        assert_eq!(s.overlay, Overlay::None);
    }

    #[test]
    fn the_menu_opens_on_the_feed_s_current_setting() {
        let mut s = on_needs_triage();
        let feed = selected_feed(&s);
        s.on_event(Event::Cadence(info(feed, CadenceGroup::Rare, true)));
        s.on_key(key(KeyCode::Char('c')));
        let rare = RHYTHM_CHOICES
            .iter()
            .position(|c| *c == Some(CadenceGroup::Rare))
            .unwrap();
        assert_eq!(
            s.overlay,
            Overlay::Rhythm {
                feed_id: feed,
                choice: rare
            }
        );
        assert_eq!(s.view().rhythm_current, Some(rare));
        // Enter without moving sets what is already set: harmless.
        assert_eq!(
            s.on_key(key(KeyCode::Enter)),
            vec![Command::SetCadence(feed, Some(CadenceGroup::Rare))]
        );
        // Not overridden (the computed group shows, but the setting is Auto): opens on Auto.
        s.on_event(Event::Cadence(info(feed, CadenceGroup::Rare, false)));
        s.on_key(key(KeyCode::Char('c')));
        assert_eq!(
            s.overlay,
            Overlay::Rhythm {
                feed_id: feed,
                choice: 0
            }
        );
        assert_eq!(s.view().rhythm_current, Some(0));
    }

    #[test]
    fn a_menu_opened_before_the_answer_moves_onto_it() {
        let mut s = on_needs_triage();
        let feed = selected_feed(&s);
        // The rhythm has not arrived: Auto for now.
        s.on_key(key(KeyCode::Char('c')));
        assert_eq!(
            s.overlay,
            Overlay::Rhythm {
                feed_id: feed,
                choice: 0
            }
        );
        s.on_event(Event::Cadence(info(feed, CadenceGroup::Monthly, true)));
        let monthly = RHYTHM_CHOICES
            .iter()
            .position(|c| *c == Some(CadenceGroup::Monthly))
            .unwrap();
        assert_eq!(
            s.overlay,
            Overlay::Rhythm {
                feed_id: feed,
                choice: monthly
            }
        );
    }

    #[test]
    fn c_on_a_buried_feed_says_so() {
        let mut s = fresh();
        s.open_garden(GardenView::Cemetery);
        let commands = s.on_event(Event::GardenRows(sample_rows_with_buried()));
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::LoadCadence(_))),
            "a buried feed has no rhythm to load"
        );
        assert!(s.on_key(key(KeyCode::Char('c'))).is_empty());
        assert_eq!(s.overlay, Overlay::None);
        assert_eq!(
            s.garden().unwrap().message.as_deref(),
            Some("Feed #3 is buried.")
        );
    }

    #[test]
    fn c_on_a_feed_source_loads_and_opens_the_menu_too() {
        let mut s = fresh();
        let commands = s.go_to(Source::Feed(12));
        assert!(commands.contains(&Command::LoadCadence(12)));
        s.on_event(Event::Cadence(info(12, CadenceGroup::Rare, false)));
        let tick = "2026-10-16T18:00:00Z".parse::<DateTime<Utc>>().unwrap();
        s.on_event(Event::Tick {
            now: tick,
            local: tick.fixed_offset(),
        });
        assert_eq!(
            s.view().rhythm_line.as_deref(),
            Some("Rhythm: Rare · median 1.5 d")
        );
        s.on_key(key(KeyCode::Char('c')));
        assert!(matches!(s.overlay, Overlay::Rhythm { feed_id: 12, .. }));
        // Leaving the feed drops the line.
        s.on_key(key(KeyCode::Esc));
        s.go_to(Source::Unread);
        assert!(s.rhythm().is_none());
        assert!(s.view().rhythm_line.is_none());
    }

    #[test]
    fn a_data_change_asks_for_the_rhythm_again() {
        let mut s = fresh();
        s.go_to(Source::Feed(12));
        let commands = s.on_event(Event::DataChanged);
        assert!(commands.contains(&Command::LoadCadence(12)));
    }
}
