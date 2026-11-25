//! The `Antiope` QML singleton: the Antiope look's navigation (`pages::AntiopeNav`), its page
//! directory, and the frame's rows (header, column titles, index, Minitel bar) encoded for
//! `CellRow`. The rules are `ressac_ui::pages`'; this object holds the inputs QML binds, asks
//! the nav, and tells QML (`stateChanged`) only when what it shows moved.

use std::pin::Pin;
use std::time::Instant;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use ressac_core::clock::today_start;
use ressac_core::store::{JournalView, Source, SourcesSnapshot};
use ressac_ui::article::SubpageLabel;
use ressac_ui::chrono::{DateTime, Local, Utc};
use ressac_ui::journal::Phase;
use ressac_ui::keys::Command;
use ressac_ui::pages::frame::{
    self, KEYS_CELLS, MinitelKey, column_title, header_label, header_line, hint, hint_line,
    index_rows, right_margin_cells, rolling_page, search_label_line, status,
};
use ressac_ui::pages::special::last_entry_id;
use ressac_ui::pages::{
    AntiopeColumn, AntiopeNav, DirectoryCache, ENTRY_CELLS, INDEX_CELLS, MIN_CELLS,
    MIN_READER_CELLS, NavContext, PageDirectory, ReaderPage, encode_rows, retries_journal,
};
use ressac_ui::runs::RunProgress;
use ressac_ui::sources::Selection;
use ressac_ui::teletext::Color;

use crate::launcher::with_session;
use crate::rows::to_i32;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Inputs, bound in QML (write-only in practice: no notify signal).
        #[qproperty(QString, selection_key, READ, WRITE = set_selection_key)]
        #[qproperty(QString, search_text, READ, WRITE = set_search_text)]
        #[qproperty(QString, garden_message, READ, WRITE = set_garden_message)]
        #[qproperty(QString, journal_phase, READ, WRITE = set_journal_phase)]
        #[qproperty(QString, journal_failure, READ, WRITE = set_journal_failure)]
        #[qproperty(bool, is_garden, READ, WRITE = set_is_garden)]
        #[qproperty(bool, unread_only, READ, WRITE = set_unread_only)]
        #[qproperty(bool, selected_read, READ, WRITE = set_selected_read)]
        #[qproperty(bool, can_refresh, READ, WRITE = set_can_refresh)]
        #[qproperty(bool, can_navigate, READ, WRITE = set_can_navigate)]
        #[qproperty(bool, run_active, READ, WRITE = set_run_active)]
        #[qproperty(bool, reduce_motion, READ, WRITE = set_reduce_motion)]
        #[qproperty(bool, crt, READ, WRITE = set_crt)]
        #[qproperty(i64, selected_entry_id, READ, WRITE = set_selected_entry_id)]
        #[qproperty(i64, selected_feed_id, READ, WRITE = set_selected_feed_id)]
        #[qproperty(i32, entry_position, READ, WRITE = set_entry_position)]
        #[qproperty(i32, entry_total, READ, WRITE = set_entry_total)]
        #[qproperty(i32, run_done, READ, WRITE = set_run_done)]
        #[qproperty(i32, run_total, READ, WRITE = set_run_total)]
        #[qproperty(i32, journal_done, READ, WRITE = set_journal_done)]
        #[qproperty(i32, journal_total, READ, WRITE = set_journal_total)]
        #[qproperty(i32, window_cells, READ, WRITE = set_window_cells)]
        #[qproperty(i32, reader_cells, READ, WRITE = set_reader_cells)]
        #[qproperty(f64, window_width, READ, WRITE = set_window_width)]
        #[qproperty(f64, window_height, READ, WRITE = set_window_height)]
        #[qproperty(f64, cell_width, READ, WRITE = set_cell_width)]
        // Outputs.
        #[qproperty(QString, column, READ = column, WRITE = set_column, NOTIFY = state_changed)]
        #[qproperty(QString, reader_page, READ = reader_page, NOTIFY = state_changed)]
        #[qproperty(i32, current_page, READ = current_page, NOTIFY = state_changed)]
        #[qproperty(QString, header, READ = header, NOTIFY = state_changed)]
        #[qproperty(QString, index_title, READ = index_title, NOTIFY = state_changed)]
        #[qproperty(QString, entries_title, READ = entries_title, NOTIFY = state_changed)]
        #[qproperty(QString, reader_title, READ = reader_title, NOTIFY = state_changed)]
        #[qproperty(QString, minitel, READ = minitel, NOTIFY = state_changed)]
        #[qproperty(QStringList, index_rows, READ = index_rows, NOTIFY = state_changed)]
        #[qproperty(i32, subpage_page, READ = subpage_page, NOTIFY = state_changed)]
        #[qproperty(i32, subpage_index, READ = subpage_index, NOTIFY = state_changed)]
        #[qproperty(i32, subpage_count, READ = subpage_count, NOTIFY = state_changed)]
        // Constants.
        #[qproperty(QStringList, palette, READ = palette, CONSTANT)]
        #[qproperty(QStringList, key_lines, READ = key_lines, CONSTANT)]
        #[qproperty(QStringList, key_verbs, READ = key_verbs, CONSTANT)]
        #[qproperty(i32, minimum_cells, READ = minimum_cells, CONSTANT)]
        #[qproperty(i32, index_cells, READ = index_cells, CONSTANT)]
        #[qproperty(i32, entry_cells, READ = entry_cells, CONSTANT)]
        #[qproperty(i32, keys_cells, READ = keys_cells, CONSTANT)]
        type Antiope = super::AntiopeRust;

        fn set_selection_key(self: Pin<&mut Antiope>, value: QString);
        fn set_search_text(self: Pin<&mut Antiope>, value: QString);
        fn set_garden_message(self: Pin<&mut Antiope>, value: QString);
        fn set_journal_phase(self: Pin<&mut Antiope>, value: QString);
        fn set_journal_failure(self: Pin<&mut Antiope>, value: QString);
        fn set_is_garden(self: Pin<&mut Antiope>, value: bool);
        fn set_unread_only(self: Pin<&mut Antiope>, value: bool);
        fn set_selected_read(self: Pin<&mut Antiope>, value: bool);
        fn set_can_refresh(self: Pin<&mut Antiope>, value: bool);
        fn set_can_navigate(self: Pin<&mut Antiope>, value: bool);
        fn set_run_active(self: Pin<&mut Antiope>, value: bool);
        fn set_reduce_motion(self: Pin<&mut Antiope>, value: bool);
        fn set_crt(self: Pin<&mut Antiope>, value: bool);
        fn set_selected_entry_id(self: Pin<&mut Antiope>, value: i64);
        fn set_selected_feed_id(self: Pin<&mut Antiope>, value: i64);
        fn set_entry_position(self: Pin<&mut Antiope>, value: i32);
        fn set_entry_total(self: Pin<&mut Antiope>, value: i32);
        fn set_run_done(self: Pin<&mut Antiope>, value: i32);
        fn set_run_total(self: Pin<&mut Antiope>, value: i32);
        fn set_journal_done(self: Pin<&mut Antiope>, value: i32);
        fn set_journal_total(self: Pin<&mut Antiope>, value: i32);
        fn set_window_cells(self: Pin<&mut Antiope>, value: i32);
        fn set_reader_cells(self: Pin<&mut Antiope>, value: i32);
        fn set_window_width(self: Pin<&mut Antiope>, value: f64);
        fn set_window_height(self: Pin<&mut Antiope>, value: f64);
        fn set_cell_width(self: Pin<&mut Antiope>, value: f64);
        fn set_column(self: Pin<&mut Antiope>, value: QString);

        fn column(self: &Antiope) -> QString;
        fn reader_page(self: &Antiope) -> QString;
        fn current_page(self: &Antiope) -> i32;
        fn header(self: &Antiope) -> QString;
        fn index_title(self: &Antiope) -> QString;
        fn entries_title(self: &Antiope) -> QString;
        fn reader_title(self: &Antiope) -> QString;
        fn minitel(self: &Antiope) -> QString;
        fn index_rows(self: &Antiope) -> QStringList;
        fn subpage_page(self: &Antiope) -> i32;
        fn subpage_index(self: &Antiope) -> i32;
        fn subpage_count(self: &Antiope) -> i32;
        fn palette(self: &Antiope) -> QStringList;
        fn key_lines(self: &Antiope) -> QStringList;
        fn key_verbs(self: &Antiope) -> QStringList;
        fn minimum_cells(self: &Antiope) -> i32;
        fn index_cells(self: &Antiope) -> i32;
        fn entry_cells(self: &Antiope) -> i32;
        fn keys_cells(self: &Antiope) -> i32;

        /// Reads the sources (the directory) and the latest edition again, then everything.
        #[qinvokable]
        fn reload(self: Pin<&mut Antiope>);

        /// The clock moved: the header's time, the rolling counter, `NOT FOUND` expiring.
        #[qinvokable]
        fn tick(self: Pin<&mut Antiope>);

        /// A verb of `ReadingPage.perform`: `Outcome::encode()`, `""` when not handled.
        #[qinvokable]
        fn perform(self: Pin<&mut Antiope>, verb: &QString) -> QString;

        /// Goes to page `number` (an index row, an overflow row): `Outcome::encode()`.
        #[qinvokable]
        fn jump(self: Pin<&mut Antiope>, number: i32) -> QString;

        /// The page-down past P104's last entry reached the bottom: FIN DE L'EDITION.
        #[qinvokable]
        fn land_on_end(self: Pin<&mut Antiope>) -> QString;

        /// The page-down a `pageDownThenEnd` effect asked for is over, whatever came of it.
        #[qinvokable]
        fn advance_finished(self: Pin<&mut Antiope>);

        /// A click on an entry: the reader column shows the article again.
        #[qinvokable]
        fn show_article(self: Pin<&mut Antiope>);

        /// The entries column's empty message, centred in capitals.
        #[qinvokable]
        fn empty_line(self: &Antiope, message: &QString) -> QString;

        /// ` SEARCH? ` for the Minitel bar's prompt, at most `width` cells.
        #[qinvokable]
        fn search_label(self: &Antiope, width: i32) -> QString;

        /// An output changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Antiope>);
    }
}

/// What the properties show, to tell QML only what moved.
#[derive(Debug, Clone, PartialEq, Default)]
struct Outputs {
    column: &'static str,
    reader_page: &'static str,
    current_page: i32,
    header: String,
    index_title: String,
    entries_title: String,
    reader_title: String,
    minitel: String,
    index_rows: Vec<String>,
    /// Page (−1 none), index (0 no label) and count of the selected entry.
    subpage: (i32, i32, i32),
}

pub struct AntiopeRust {
    selection_key: QString,
    search_text: QString,
    garden_message: QString,
    journal_phase: QString,
    journal_failure: QString,
    is_garden: bool,
    unread_only: bool,
    selected_read: bool,
    can_refresh: bool,
    can_navigate: bool,
    run_active: bool,
    reduce_motion: bool,
    crt: bool,
    selected_entry_id: i64,
    selected_feed_id: i64,
    entry_position: i32,
    entry_total: i32,
    run_done: i32,
    run_total: i32,
    journal_done: i32,
    journal_total: i32,
    window_cells: i32,
    reader_cells: i32,
    window_width: f64,
    window_height: f64,
    cell_width: f64,

    nav: AntiopeNav,
    cache: DirectoryCache,
    /// The cache's directory, copied when it was rebuilt (the cache only lends it out for a
    /// snapshot).
    directory: PageDirectory,
    /// The latest edition, read at `reload`: P104's sub-pages and its last entry.
    journal: Option<JournalView>,
    /// The (current page, directory build) `shown.index_rows` was encoded for.
    index_for: Option<(Option<u16>, usize)>,
    shown: Outputs,
}

impl Default for AntiopeRust {
    fn default() -> Self {
        let mut rust = AntiopeRust {
            selection_key: QString::from(Selection::default().key().as_str()),
            search_text: QString::default(),
            garden_message: QString::default(),
            journal_phase: QString::from(Phase::Idle.as_str()),
            journal_failure: QString::default(),
            is_garden: false,
            unread_only: false,
            selected_read: false,
            can_refresh: false,
            can_navigate: false,
            run_active: false,
            reduce_motion: false,
            crt: false,
            selected_entry_id: -1,
            selected_feed_id: -1,
            entry_position: 0,
            entry_total: 0,
            run_done: 0,
            run_total: 0,
            journal_done: 0,
            journal_total: 0,
            window_cells: to_i32(MIN_CELLS),
            reader_cells: to_i32(MIN_READER_CELLS),
            window_width: 0.0,
            window_height: 0.0,
            cell_width: 0.0,
            nav: AntiopeNav::default(),
            cache: DirectoryCache::default(),
            directory: PageDirectory::default(),
            journal: None,
            index_for: None,
            shown: Outputs::default(),
        };
        // QML may read before the first reload: the frame of an empty directory.
        rust.update(Utc::now(), Instant::now());
        rust
    }
}

/// A cell count from QML, negative as 0.
fn cells(count: i32) -> usize {
    usize::try_from(count).unwrap_or(0)
}

/// The command of a verb Antiope answers (`keys::verb`'s names); `None` for the verbs it
/// leaves to the Native handling (`toggleRead`, `spent`…).
fn command_of(verb: &str) -> Option<Command> {
    if let Some(digit) = verb.strip_prefix("pageDigit") {
        let digit = match digit.as_bytes() {
            [d @ b'0'..=b'9'] => d - b'0',
            _ => return None,
        };
        return Some(Command::PageDigit(digit));
    }
    Some(match verb {
        "pageCorrect" => Command::PageCorrect,
        "pageCancel" => Command::PageCancel,
        "sommaire" => Command::Sommaire,
        "guide" => Command::Guide,
        "repetition" => Command::Repetition,
        "toggleImages" => Command::ToggleImages,
        "nextColumn" => Command::NextColumn,
        "previousColumn" => Command::PreviousColumn,
        "nextIndexRow" => Command::NextIndexRow,
        "previousIndexRow" => Command::PreviousIndexRow,
        "nextEntry" => Command::NextEntry,
        "previousEntry" => Command::PreviousEntry,
        "advance" | "advanceHeld" => Command::Advance,
        _ => return None,
    })
}

/// `Journal.phase` (`Phase::as_str`) with its counts and failure message, back as a phase.
fn phase_of(name: &str, done: i32, total: i32, failure: &str) -> Phase {
    match name {
        "due" => Phase::Due,
        "building" => Phase::Building {
            done: cells(done),
            total: cells(total),
        },
        "offline" => Phase::Offline,
        "failed" => Phase::Failed(failure.to_string()),
        _ => Phase::Idle,
    }
}

impl AntiopeRust {
    fn selection(&self) -> Selection {
        Selection::parse_key(&self.selection_key.to_string()).unwrap_or_default()
    }

    fn selected_entry(&self) -> Option<(i64, bool)> {
        (self.selected_entry_id >= 0).then_some((self.selected_entry_id, self.selected_read))
    }

    fn phase(&self) -> Phase {
        phase_of(
            &self.journal_phase.to_string(),
            self.journal_done,
            self.journal_total,
            &self.journal_failure.to_string(),
        )
    }

    fn context(&self, now: Instant) -> NavContext<'_> {
        NavContext {
            directory: &self.directory,
            selection: self.selection(),
            selected_entry: self.selected_entry(),
            last_edition_entry: last_entry_id(self.journal.as_ref()),
            journal_retries: retries_journal(&self.phase()),
            can_refresh: self.can_refresh,
            can_navigate_entries: self.can_navigate,
            now,
        }
    }

    /// The directory for `snapshot`, copied only when the cache rebuilt it.
    fn take_snapshot(&mut self, snapshot: &SourcesSnapshot) {
        let builds = self.cache.builds();
        self.cache.get(snapshot);
        if self.cache.builds() != builds {
            // The same snapshot again: no second build, only the loan.
            self.directory = self.cache.get(snapshot).clone();
        }
    }

    /// The nav's answer to `command`, with the window as it is now.
    fn handle(&mut self, command: Command, now: Instant) -> String {
        let mut nav = std::mem::take(&mut self.nav);
        let outcome = nav.handle(command, &self.context(now));
        self.nav = nav;
        outcome.encode()
    }

    fn jump_to(&mut self, number: u16, now: Instant) -> String {
        let mut nav = std::mem::take(&mut self.nav);
        let outcome = nav.jump(number, &self.context(now));
        self.nav = nav;
        outcome.encode()
    }

    /// `selectedEntryId` under its feed's page, `entryPosition` of `entryTotal`; on P104 by
    /// its rank in the edition.
    fn subpage(&self) -> (i32, i32, i32) {
        const NONE: (i32, i32, i32) = (-1, 0, 0);
        let id = self.selected_entry_id;
        if id < 0 {
            return NONE;
        }
        let label = if self.selection() == Selection::Source(Source::Journal) {
            SubpageLabel::of_edition(id, self.journal.as_ref())
        } else {
            SubpageLabel::of(
                id,
                &[(id, self.selected_feed_id)],
                Some(cells(self.entry_total)),
                |feed| self.directory.page_of_feed(feed),
            )
            .map(|label| SubpageLabel {
                index: cells(self.entry_position),
                ..label
            })
        };
        match label {
            Some(label) if label.index > 0 => (
                label.page.map_or(-1, i32::from),
                to_i32(label.index),
                to_i32(label.count),
            ),
            _ => NONE,
        }
    }

    /// Every output for the inputs, the nav and the clock. The index rows are encoded again
    /// only when the current page or the directory changed.
    fn outputs(&mut self, now: DateTime<Utc>, instant: Instant) -> Outputs {
        let selection = self.selection();
        let current = self.nav.current_page(&self.directory, &selection);
        let column = self.nav.column();
        let reader_page = self.nav.reader_page();

        let index_for = (current, self.cache.builds());
        let rows = if self.index_for == Some(index_for) {
            self.shown.index_rows.clone()
        } else {
            self.index_for = Some(index_for);
            encode_rows(&index_rows(&self.directory, current))
        };

        let rolling = (self.run_active && !self.reduce_motion).then(|| rolling_page(now));
        let label = header_label(
            self.nav.entry(),
            self.nav.not_found(instant),
            current,
            rolling,
        );
        let margin = right_margin_cells(
            self.window_width,
            self.window_height,
            self.cell_width,
            self.crt,
        );
        let header = header_line(&label, now, &Local, cells(self.window_cells), margin);

        let title = |of: AntiopeColumn, width: usize, unread: Option<bool>, blank: bool| {
            column_title(of, column == of, width, unread, blank).encode()
        };

        let progress = self.run_active.then(|| RunProgress {
            done: cells(self.run_done),
            total: cells(self.run_total),
            label: String::new(),
        });
        let message = self.garden_message.to_string();
        let message = (!message.is_empty()).then_some(message.as_str());
        let search = self.search_text.to_string();
        let shown_hint = status(progress.as_ref(), message, &self.phase())
            .unwrap_or_else(|| hint(column, self.is_garden, &search));
        let minitel = hint_line(
            &shown_hint,
            cells(self.window_cells).saturating_sub(KEYS_CELLS),
        );

        Outputs {
            column: column.as_str(),
            reader_page: reader_page.as_str(),
            current_page: current.map_or(-1, i32::from),
            header: header.encode(),
            index_title: title(AntiopeColumn::Index, INDEX_CELLS, None, false),
            entries_title: title(
                AntiopeColumn::Entries,
                ENTRY_CELLS,
                (!self.is_garden).then_some(self.unread_only),
                false,
            ),
            reader_title: title(
                AntiopeColumn::Reader,
                cells(self.reader_cells),
                None,
                reader_page != ReaderPage::Article,
            ),
            minitel: minitel.encode(),
            index_rows: rows,
            subpage: self.subpage(),
        }
    }

    /// Recomputes the outputs; true when one changed.
    fn update(&mut self, now: DateTime<Utc>, instant: Instant) -> bool {
        let outputs = self.outputs(now, instant);
        if outputs == self.shown {
            return false;
        }
        self.shown = outputs;
        true
    }
}

/// A setter per input: an unchanged value does nothing, a new one recomputes the outputs.
macro_rules! input_setters {
    ($($setter:ident => $field:ident: $ty:ty),* $(,)?) => {
        impl qobject::Antiope {
            $(
                fn $setter(mut self: Pin<&mut Self>, value: $ty) {
                    if self.rust().$field == value {
                        return;
                    }
                    self.as_mut().rust_mut().get_mut().$field = value;
                    self.refresh();
                }
            )*
        }
    };
}

input_setters! {
    set_selection_key => selection_key: QString,
    set_search_text => search_text: QString,
    set_garden_message => garden_message: QString,
    set_journal_phase => journal_phase: QString,
    set_journal_failure => journal_failure: QString,
    set_is_garden => is_garden: bool,
    set_unread_only => unread_only: bool,
    set_selected_read => selected_read: bool,
    set_can_refresh => can_refresh: bool,
    set_can_navigate => can_navigate: bool,
    set_run_active => run_active: bool,
    set_reduce_motion => reduce_motion: bool,
    set_crt => crt: bool,
    set_selected_entry_id => selected_entry_id: i64,
    set_selected_feed_id => selected_feed_id: i64,
    set_entry_position => entry_position: i32,
    set_entry_total => entry_total: i32,
    set_run_done => run_done: i32,
    set_run_total => run_total: i32,
    set_journal_done => journal_done: i32,
    set_journal_total => journal_total: i32,
    set_window_cells => window_cells: i32,
    set_reader_cells => reader_cells: i32,
    set_window_width => window_width: f64,
    set_window_height => window_height: f64,
    set_cell_width => cell_width: f64,
}

fn string_list(items: impl IntoIterator<Item = String>) -> QStringList {
    items
        .into_iter()
        .map(|item| QString::from(item.as_str()))
        .collect()
}

impl qobject::Antiope {
    /// Recomputes the outputs and emits `stateChanged` when one moved.
    fn refresh(mut self: Pin<&mut Self>) {
        let changed = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .update(Utc::now(), Instant::now());
        if changed {
            self.state_changed();
        }
    }

    fn set_column(mut self: Pin<&mut Self>, value: QString) {
        let Some(column) = AntiopeColumn::parse(&value.to_string()) else {
            return;
        };
        if self.rust().nav.column() == column {
            return;
        }
        self.as_mut().rust_mut().get_mut().nav.set_column(column);
        self.refresh();
    }

    fn column(&self) -> QString {
        QString::from(self.rust().shown.column)
    }

    fn reader_page(&self) -> QString {
        QString::from(self.rust().shown.reader_page)
    }

    fn current_page(&self) -> i32 {
        self.rust().shown.current_page
    }

    fn header(&self) -> QString {
        QString::from(self.rust().shown.header.as_str())
    }

    fn index_title(&self) -> QString {
        QString::from(self.rust().shown.index_title.as_str())
    }

    fn entries_title(&self) -> QString {
        QString::from(self.rust().shown.entries_title.as_str())
    }

    fn reader_title(&self) -> QString {
        QString::from(self.rust().shown.reader_title.as_str())
    }

    fn minitel(&self) -> QString {
        QString::from(self.rust().shown.minitel.as_str())
    }

    fn index_rows(&self) -> QStringList {
        string_list(self.rust().shown.index_rows.iter().cloned())
    }

    fn subpage_page(&self) -> i32 {
        self.rust().shown.subpage.0
    }

    fn subpage_index(&self) -> i32 {
        self.rust().shown.subpage.1
    }

    fn subpage_count(&self) -> i32 {
        self.rust().shown.subpage.2
    }

    fn palette(&self) -> QStringList {
        string_list(Color::ALL.iter().map(|color| color.css().to_string()))
    }

    fn key_lines(&self) -> QStringList {
        string_list(MinitelKey::ALL.iter().map(|key| key.line().encode()))
    }

    fn key_verbs(&self) -> QStringList {
        string_list(MinitelKey::ALL.iter().map(|key| key.verb().to_string()))
    }

    fn minimum_cells(&self) -> i32 {
        to_i32(MIN_CELLS)
    }

    fn index_cells(&self) -> i32 {
        to_i32(INDEX_CELLS)
    }

    fn entry_cells(&self) -> i32 {
        to_i32(ENTRY_CELLS)
    }

    fn keys_cells(&self) -> i32 {
        to_i32(KEYS_CELLS)
    }

    fn reload(mut self: Pin<&mut Self>) {
        let today = today_start(Local::now());
        let read = with_session(|session| (session.store.sources(today), session.store.journal()));
        if let Some((sources, journal)) = read {
            let rust = self.as_mut().rust_mut().get_mut();
            match sources {
                Ok(snapshot) => rust.take_snapshot(&snapshot),
                // The last directory stays: the pages keep their numbers until a read works.
                Err(error) => tracing::warn!("cannot read the sources for the pages: {error}"),
            }
            match journal {
                Ok(view) => rust.journal = view,
                Err(error) => tracing::warn!("cannot read the edition for the pages: {error}"),
            }
        }
        self.as_mut()
            .rust_mut()
            .get_mut()
            .update(Utc::now(), Instant::now());
        self.state_changed();
    }

    fn tick(self: Pin<&mut Self>) {
        self.refresh();
    }

    fn perform(mut self: Pin<&mut Self>, verb: &QString) -> QString {
        let Some(command) = command_of(&verb.to_string()) else {
            return QString::default();
        };
        let encoded = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .handle(command, Instant::now());
        self.refresh();
        QString::from(encoded.as_str())
    }

    fn jump(mut self: Pin<&mut Self>, number: i32) -> QString {
        let Ok(number) = u16::try_from(number) else {
            return QString::default();
        };
        let encoded = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .jump_to(number, Instant::now());
        self.refresh();
        QString::from(encoded.as_str())
    }

    fn land_on_end(mut self: Pin<&mut Self>) -> QString {
        let rust = self.as_mut().rust_mut().get_mut();
        let selected = rust.selected_entry();
        let encoded = rust.nav.land_on_end(selected).encode();
        self.refresh();
        QString::from(encoded.as_str())
    }

    fn advance_finished(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().nav.advance_finished();
        self.refresh();
    }

    fn show_article(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().nav.show_article();
        self.refresh();
    }

    fn empty_line(&self, message: &QString) -> QString {
        QString::from(
            frame::empty_line(&message.to_string(), ENTRY_CELLS)
                .encode()
                .as_str(),
        )
    }

    fn search_label(&self, width: i32) -> QString {
        QString::from(search_label_line(cells(width)).encode().as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    use ressac_ui::pages::NOT_FOUND_FOR;
    use ressac_ui::teletext::decode;

    /// The text of an encoded row.
    fn text(encoded: &str) -> String {
        decode(encoded).into_iter().map(|run| run.text).collect()
    }

    fn header(rust: &AntiopeRust) -> String {
        text(&rust.shown.header)
    }

    #[test]
    fn verbs_map_to_the_commands_antiope_answers_and_no_others() {
        for digit in 0..=9u8 {
            assert_eq!(
                command_of(&format!("pageDigit{digit}")),
                Some(Command::PageDigit(digit))
            );
        }
        assert_eq!(command_of("advanceHeld"), Some(Command::Advance));
        assert_eq!(command_of("previousEntry"), Some(Command::PreviousEntry));
        assert_eq!(command_of("nextIndexRow"), Some(Command::NextIndexRow));
        for other in [
            "pageDigit",
            "pageDigit10",
            "pageDigit+5",
            "toggleRead",
            "toggleStar",
            "toggleReadHeld",
            "focusEntryList",
            "spent",
            "",
        ] {
            assert_eq!(command_of(other), None, "{other}");
        }
    }

    #[test]
    fn the_journal_phase_is_rebuilt_from_its_name_counts_and_failure() {
        assert_eq!(phase_of("idle", 3, 9, "x"), Phase::Idle);
        assert_eq!(phase_of("due", 0, 0, ""), Phase::Due);
        assert_eq!(
            phase_of("building", 34, 156, ""),
            Phase::Building {
                done: 34,
                total: 156
            }
        );
        assert_eq!(phase_of("offline", 0, 0, ""), Phase::Offline);
        assert_eq!(
            phase_of("failed", 0, 0, "No network"),
            Phase::Failed("No network".into())
        );
        assert_eq!(phase_of("", -1, -1, ""), Phase::Idle);
    }

    #[test]
    fn typing_a_page_shows_in_the_header_and_only_antiope_verbs_are_handled() {
        let mut rust = AntiopeRust::default();
        let now = Instant::now();
        assert_eq!(rust.handle(Command::PageDigit(4), now), "handled");
        assert!(rust.update(Utc::now(), now));
        assert!(header(&rust).starts_with("P4__"), "{}", header(&rust));
        assert_eq!(rust.handle(Command::PageCancel, now), "handled");
        // Nothing typed: ANNULATION falls through to the Native handling.
        assert_eq!(rust.handle(Command::PageCancel, now), "");
        assert_eq!(rust.handle(Command::PageCorrect, now), "");
    }

    #[test]
    fn an_unknown_page_is_not_found_until_it_expires() {
        let mut rust = AntiopeRust::default();
        let now = Instant::now();
        assert_eq!(rust.jump_to(555, now), "handled;notFound");
        rust.update(Utc::now(), now);
        assert!(
            header(&rust).starts_with("P555 NOT FOUND"),
            "{}",
            header(&rust)
        );
        let later = now + NOT_FOUND_FOR + Duration::from_millis(1);
        assert!(rust.update(Utc::now(), later));
        assert!(!header(&rust).contains("NOT FOUND"), "{}", header(&rust));
    }

    #[test]
    fn an_input_no_output_depends_on_changes_nothing() {
        let mut rust = AntiopeRust::default();
        let (now, instant) = (Utc::now(), Instant::now());
        rust.update(now, instant);
        assert!(!rust.update(now, instant));
        rust.selected_read = true;
        rust.can_refresh = true;
        assert!(!rust.update(now, instant));
        rust.unread_only = true;
        assert!(rust.update(now, instant));
        assert_eq!(
            text(&rust.shown.entries_title).trim_end(),
            " ENTRIES  UNREAD"
        );
    }

    #[test]
    fn the_minitel_bar_shows_the_garden_message_then_the_run_then_the_hint() {
        let mut rust = AntiopeRust::default();
        let (now, instant) = (Utc::now(), Instant::now());
        rust.update(now, instant);
        // 76 cells leave 27 for the hint: it is cut, never wider.
        assert_eq!(
            text(&rust.shown.minitel).chars().count(),
            MIN_CELLS - KEYS_CELLS
        );
        rust.window_cells = 100;
        rust.update(now, instant);
        assert_eq!(
            text(&rust.shown.minitel).trim(),
            "type a page number · ? guide"
        );
        rust.run_active = true;
        rust.run_done = 42;
        rust.run_total = 155;
        rust.update(now, instant);
        assert_eq!(text(&rust.shown.minitel).trim(), "REFRESHING 42/155");
        rust.garden_message = QString::from("Nothing to undo");
        rust.update(now, instant);
        assert_eq!(text(&rust.shown.minitel).trim(), "Nothing to undo");
        assert_eq!(text(&rust.shown.minitel).chars().count(), 100 - KEYS_CELLS);
    }

    #[test]
    fn the_subpage_is_the_entry_position_out_of_the_list_total() {
        let mut rust = AntiopeRust::default();
        assert_eq!(rust.subpage(), (-1, 0, 0));
        rust.selected_entry_id = 7;
        rust.selected_feed_id = 3;
        rust.entry_position = 2;
        rust.entry_total = 20;
        // No directory yet: the feed has no page.
        assert_eq!(rust.subpage(), (-1, 2, 20));
        rust.entry_position = 0;
        assert_eq!(rust.subpage(), (-1, 0, 0));
        // P104 without an edition read: no label.
        rust.entry_position = 2;
        rust.selection_key = QString::from("journal");
        assert_eq!(rust.subpage(), (-1, 0, 0));
    }
}
