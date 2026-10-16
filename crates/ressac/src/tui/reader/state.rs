//! The reader's state and keys (spec §5). Pure: no terminal, no database, no clock. Keys and
//! events come in, `Command`s go out to the event loop (phase 5b), which runs them and answers
//! with `Event`s; the same pattern as the garden's `GardenState`.

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ressac_core::model::{CadenceGroup, FeedId};
use ressac_core::store::{
    EntryDetail, EntryPage, EntryRow, JournalView, ListCursor, ListFilter, Source, SourcesSnapshot,
};

use crate::output::printable;

use super::html::{RenderedArticle, is_web_url, render_html};
use super::layout::{Column, LayoutClass, Look, columns, layout_class, text_height, text_width};
use super::settings::TerminalSettings;
use super::sidebar::{self, SidebarRow};

/// Lines of the reader column taken by the article header (title; feed, author, date; state).
pub const READER_HEADER_LINES: usize = 3;

pub const END_OF_EDITION: &str = "That's the edition.";
pub const NO_MORE_UNREAD: &str = "No more unread entries here.";
pub const NO_ENTRY: &str = "No entry selected.";
pub const NO_LINK: &str = "This entry has no link.";
pub const NO_FEED: &str = "No feed selected.";
pub const FETCHING_FULL: &str = "Fetching the full article…";
pub const OFFLINE: &str = "Offline: no edition yet";
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
    #[allow(dead_code, reason = "phase 6 asks for the images")]
    FetchImage(String),
    #[allow(dead_code, reason = "phase 7 builds the edition when the reader opens")]
    BuildEdition,
    Refresh(Option<Source>),
    OpenUrl(String),
    SetCadence(FeedId, Option<CadenceGroup>),
    /// A garden command from the garden screen (phase 8).
    #[allow(dead_code, reason = "phase 8 runs the garden screen's commands")]
    Garden(crate::tui::state::Command),
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
    /// The answer to a `Command::LoadEntries` or `Command::Search`: `request` is echoed from the
    /// command, so a late answer to a list the reader has left is recognised and dropped.
    Entries {
        source: Source,
        page: EntryPage,
        appended: bool,
        request: u64,
    },
    Journal(Option<JournalView>),
    /// Boxed: far larger than the other variants (`clippy::large_enum_variant`).
    Entry(Box<EntryDetail>),
    #[allow(dead_code, reason = "phase 6 answers the image commands")]
    ImageReady {
        url: String,
    },
    #[allow(dead_code, reason = "phase 6 answers the image commands")]
    ImageFailed {
        url: String,
    },
    Progress {
        label: String,
        done: usize,
        total: usize,
    },
    JobDone(JobOutcome),
    DataChanged,
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
    /// returns NotDue in both cases (phase 7).
    EditionNotDue,
    Offline,
    Failed(String),
    Extracted {
        entry_id: i64,
        error: Option<String>,
    },
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
    pub sidebar: Vec<SidebarRow>,
    pub sidebar_index: usize,
    pub entries: Vec<EntryRow>,
    /// Rows and unread rows of the whole list, loaded or not.
    pub total: usize,
    pub unread: usize,
    pub next: Option<ListCursor>,
    pub journal: Option<JournalView>,
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
    #[allow(dead_code, reason = "read by the Journal look (phase 7)")]
    pub sources_overlay: bool,
    pub now_local: DateTime<FixedOffset>,
    #[allow(dead_code, reason = "read by the Journal look (phase 7)")]
    pub source: Source,
    pub source_title: String,
    pub source_lines: Vec<SourceLine>,
    pub filter: ListFilter,
    pub search: Option<&'a str>,
    pub entries: &'a [EntryRow],
    pub entry_cursor: Option<usize>,
    #[allow(dead_code, reason = "read by the Journal look (phase 7)")]
    pub total: usize,
    pub unread: usize,
    #[allow(dead_code, reason = "read by the Journal look (phase 7)")]
    pub journal: Option<&'a JournalView>,
    pub entry: Option<&'a EntryDetail>,
    pub article: Option<&'a RenderedArticle>,
    pub show_fulltext: bool,
    pub reader_scroll: usize,
    #[allow(dead_code, reason = "read by the image drawing (phase 6)")]
    pub images: bool,
    pub progress: Option<(&'a str, usize, usize)>,
    pub message: Option<&'a str>,
    pub help: bool,
    /// The search text or the link number is being typed: the bottom line shows how to end it.
    pub editing: bool,
    pub rhythm: Option<(usize, usize)>,
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
            sidebar: Vec::new(),
            sidebar_index: 0,
            entries: Vec::new(),
            total: 0,
            unread: 0,
            next: None,
            journal: None,
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

    /// The first commands: the sources, the Journal and its entries. Stores the clock; phase 7
    /// extends this body with the edition build (same signature, see the plan index).
    pub fn start(&mut self, now: DateTime<Utc>, local: DateTime<FixedOffset>) -> Vec<Command> {
        self.now = Some(now);
        self.local = Some(local);
        vec![Command::LoadSources, Command::LoadJournal, self.reload()]
    }

    /// Entry points for other screens (Antiope page jumps, phase 9).
    #[allow(dead_code, reason = "used by Antiope page jumps (phase 9)")]
    pub fn go_to(&mut self, source: Source) -> Vec<Command> {
        self.switch_source(source)
    }

    #[allow(dead_code, reason = "used by Antiope page jumps (phase 9)")]
    pub fn select_entry(&mut self, entry_id: i64) -> Vec<Command> {
        match self.entries.iter().position(|row| row.entry_id == entry_id) {
            Some(index) => self.select(index, true),
            None => Vec::new(),
        }
    }

    #[allow(dead_code, reason = "used by the garden screen (phase 8)")]
    pub fn set_focus(&mut self, column: Column) {
        self.focus = column;
    }

    pub fn look(&self) -> Look {
        self.settings.look
    }

    #[allow(dead_code, reason = "used by the garden screen (phase 8)")]
    pub fn focus(&self) -> Column {
        self.focus
    }

    #[allow(dead_code, reason = "used by the garden screen (phase 8)")]
    pub fn source(&self) -> Source {
        self.source
    }

    #[allow(dead_code, reason = "used by the image and Antiope phases (6 and 9)")]
    pub fn selected_entry(&self) -> Option<i64> {
        self.selected
    }

    #[allow(dead_code, reason = "used by the image and garden phases (6 and 8)")]
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
            .find(|row| row.source() == Some(self.source))
            .map(|row| source_line(row, false).label)
            .unwrap_or_else(|| {
                match self.source {
                    Source::Journal => "Journal",
                    Source::Unread => "Unread",
                    Source::Today => "Today",
                    Source::Starred => "Starred",
                    _ => "",
                }
                .to_string()
            });
        ReaderView {
            look: self.look(),
            layout: self.class,
            focus: self.focus,
            sources_overlay: self.class == LayoutClass::Two && self.focus == Column::Sources,
            now_local: self.local.unwrap_or(DateTime::UNIX_EPOCH.fixed_offset()),
            source: self.source,
            source_title,
            source_lines,
            filter: self.filter,
            search: self.search.as_deref(),
            entries: &self.entries,
            entry_cursor: self.index(),
            total: self.total,
            unread: self.unread,
            journal: self.journal.as_ref(),
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
        }
    }

    /// The renderer reports the article's height in rows and the reader's visible rows (below
    /// the header); Space uses them.
    pub fn set_reader_metrics(&mut self, total_rows: usize, viewport_rows: u16) {
        self.metrics = Some((total_rows, viewport_rows));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Command::Quit];
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
        self.message = None;
        match key.code {
            KeyCode::Char('q') => vec![Command::Quit],
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                Vec::new()
            }
            KeyCode::Char('t') => {
                self.settings.look = match self.settings.look {
                    Look::Native => Look::Antiope,
                    Look::Antiope => Look::Native,
                };
                self.render_article();
                vec![Command::SaveSettings]
            }
            KeyCode::Char('i') => {
                self.settings.images = !self.settings.images;
                vec![Command::SaveSettings]
            }
            KeyCode::Char('j') => self.move_entry(1),
            KeyCode::Char('k') => self.move_entry(-1),
            KeyCode::Down if self.focus == Column::Sources => self.move_source(1),
            KeyCode::Up if self.focus == Column::Sources => self.move_source(-1),
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
            KeyCode::Char('r') => vec![Command::Refresh(Some(self.source))],
            KeyCode::Char('R') => vec![Command::Refresh(None)],
            KeyCode::Char('c') => self.rhythm_menu(),
            _ => Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::Sources(snapshot) => {
                self.sidebar = sidebar::rows(&snapshot);
                let fallback = self.sidebar_index.min(self.sidebar.len().saturating_sub(1));
                self.sidebar_index =
                    sidebar::index_of(&self.sidebar, self.source).unwrap_or(fallback);
                self.sources = Some(snapshot);
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
                Vec::new()
            }
            Event::Entry(detail) => {
                self.entry_loaded(*detail);
                Vec::new()
            }
            Event::ImageReady { .. } | Event::ImageFailed { .. } => Vec::new(),
            Event::Progress { label, done, total } => {
                self.progress = Some((label, done, total));
                Vec::new()
            }
            Event::JobDone(outcome) => self.job_done(outcome),
            Event::DataChanged => self.data_changed(),
            Event::Tick { now, local } => {
                self.now = Some(now);
                self.local = Some(local);
                Vec::new()
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

    /// Selects row `index`; opening it by hand (`mark_read`) marks it read, as the app does.
    fn select(&mut self, index: usize, mark_read: bool) -> Vec<Command> {
        let Some(row) = self.entries.get(index) else {
            return Vec::new();
        };
        let (id, was_read) = (row.entry_id, row.is_read);
        let mut commands = Vec::new();
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
        let from = self.index().map_or(0, |i| i + 1).min(self.entries.len());
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
        let end = if self.source == Source::Journal {
            END_OF_EDITION
        } else {
            NO_MORE_UNREAD
        };
        self.message = Some(end.to_string());
        Vec::new()
    }

    fn focus_right(&mut self, wrap: bool) -> Vec<Command> {
        match self.focus {
            Column::Sources => self.open_sidebar_row(),
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
            Column::Entries => self.focus_right(false),
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
        match self
            .sidebar
            .get(self.sidebar_index)
            .and_then(SidebarRow::source)
        {
            Some(source) => self.switch_source(source),
            None => Vec::new(),
        }
    }

    fn move_source(&mut self, delta: isize) -> Vec<Command> {
        if self.sidebar.is_empty() {
            return Vec::new();
        }
        self.sidebar_index = sidebar::step(&self.sidebar, self.sidebar_index, delta);
        match self.sidebar[self.sidebar_index].source() {
            Some(source) => self.switch_source(source),
            None => Vec::new(),
        }
    }

    fn switch_source(&mut self, source: Source) -> Vec<Command> {
        if let Some(index) = sidebar::index_of(&self.sidebar, source) {
            self.sidebar_index = index;
        }
        if source == self.source {
            return Vec::new();
        }
        self.source = source;
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

    /// `c`: the rhythm menu for the feed in view (a feed source, else the entry's feed).
    fn rhythm_menu(&mut self) -> Vec<Command> {
        let feed = match self.source {
            Source::Feed(id) => Some(id),
            _ => self.index().map(|i| self.entries[i].feed_id),
        };
        match feed {
            Some(feed_id) => self.overlay = Overlay::Rhythm { feed_id, choice: 0 },
            None => self.message = Some(NO_FEED.into()),
        }
        Vec::new()
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
        if self.selected.is_none() && !self.entries.is_empty() && self.reader_visible() {
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
            JobOutcome::EditionBuilt | JobOutcome::EditionNotDue => self.data_changed(),
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
        let mut commands = vec![Command::LoadSources];
        if self.source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands.push(self.reload_in_place());
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
                Command::LoadJournal,
                Command::LoadEntries {
                    source: Source::Journal,
                    filter: ListFilter::All,
                    after: None,
                    request: 1
                },
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
        assert!(!v.sources_overlay);
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
}
