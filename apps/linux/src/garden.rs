//! The `Garden` QML singleton: the garden's table as a list model (one row per feed, the
//! section header as a role), the header, the commands and the inspector's properties. The
//! rules are `ressac_ui::garden`'s (`GardenVisit`, `Inspector`, keys, labels); this object
//! applies the rows by feed id (never a reset: the selection and the scroll position survive
//! every reload) and sends decisions, undo, safe repairs, rhythm and theme through the
//! short door of `Runs`. The inspector's getters are in `garden_inspector.rs`.

use std::collections::HashMap;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList, QVariant};
use ressac_core::model::{Action, Decision, FeedId};
use ressac_core::store::AttemptRecord;
use ressac_ui::chrono::{self, Utc};
use ressac_ui::garden::{
    ARCHIVED_COPY, BURIED_NOTE, COULD_NOT_READ_GARDEN, Command, GardenVisit, Grouping,
    HISTORY_TITLE, Inspector, LOG_LINES, NO_FEED_TEXT, NO_FEED_TITLE, NOT_FETCHED, RHYTHM_CHOICES,
    apply_button_title, apply_confirm_label, apply_question, chip, load, rhythm_choice,
    rhythm_label, safe_repair_preview, status_glyph, theme_choice, theme_names, tooltip,
    unread_label,
};
use ressac_ui::live::plan_rows;
use ressac_ui::runs::{RunEnd, RunRequest};
use ressac_ui::sources::{GardenScope, Selection};

use crate::launcher::with_session;
use crate::rows::{list_model_rows, to_i32, variant_string};
use crate::runs::submit_short;

/// The role names QML sees, in role order (`FIRST_ROLE + index`).
const ROLES: [&str; 10] = [
    "feedId", "glyph", "title", "clue", "rhythm", "unread", "chip", "dimmed", "tooltip", "section",
];

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // The header and the dialog.
        #[qproperty(bool, active, READ = active, NOTIFY = state_changed)]
        #[qproperty(QString, scope_title, READ = scope_title, NOTIFY = state_changed)]
        #[qproperty(QString, count_label, READ = count_label, NOTIFY = state_changed)]
        #[qproperty(bool, shows_grouping, READ = shows_grouping, NOTIFY = state_changed)]
        #[qproperty(QString, grouping, READ = grouping, WRITE = set_grouping, NOTIFY = state_changed)]
        #[qproperty(bool, shows_apply, READ = shows_apply, NOTIFY = state_changed)]
        #[qproperty(i32, safe_repair_count, READ = safe_repair_count, NOTIFY = state_changed)]
        #[qproperty(QString, apply_title, READ = apply_title, NOTIFY = state_changed)]
        #[qproperty(QString, apply_question, READ = apply_question, NOTIFY = state_changed)]
        #[qproperty(QString, apply_preview, READ = apply_preview, NOTIFY = state_changed)]
        #[qproperty(QString, apply_confirm_label, READ = apply_confirm_label, NOTIFY = state_changed)]
        #[qproperty(bool, confirming, READ = confirming, WRITE = set_confirming, NOTIFY = state_changed)]
        #[qproperty(QString, message, READ = message, NOTIFY = state_changed)]
        #[qproperty(QString, empty_text, READ = empty_text, NOTIFY = state_changed)]
        #[qproperty(QStringList, section_titles, READ = section_titles, NOTIFY = state_changed)]
        // The selection.
        #[qproperty(i32, selected_row, READ = selected_row, NOTIFY = selection_changed)]
        #[qproperty(i64, selected_feed_id, READ = selected_feed_id, NOTIFY = selection_changed)]
        // The inspector (`garden_inspector.rs`).
        #[qproperty(bool, has_selection, READ = has_selection, NOTIFY = inspector_changed)]
        #[qproperty(QString, title, READ = title, NOTIFY = inspector_changed)]
        #[qproperty(QString, folder_path, READ = folder_path, NOTIFY = inspector_changed)]
        #[qproperty(QString, status_line, READ = status_line, NOTIFY = inspector_changed)]
        #[qproperty(QString, status_color, READ = status_color, NOTIFY = inspector_changed)]
        #[qproperty(bool, buried, READ = buried, NOTIFY = inspector_changed)]
        #[qproperty(QString, reason, READ = reason, NOTIFY = inspector_changed)]
        #[qproperty(QString, rhythm_line, READ = rhythm_line, NOTIFY = inspector_changed)]
        #[qproperty(i32, rhythm_index, READ = rhythm_index, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, theme_names, READ = theme_names, NOTIFY = themes_changed)]
        #[qproperty(i32, theme_index, READ = theme_index, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, evidence, READ = evidence, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, remarks, READ = remarks, NOTIFY = inspector_changed)]
        #[qproperty(QString, proposed, READ = proposed, NOTIFY = inspector_changed)]
        #[qproperty(QString, decision, READ = decision, NOTIFY = inspector_changed)]
        #[qproperty(QString, feed_url, READ = feed_url, NOTIFY = inspector_changed)]
        #[qproperty(QString, site, READ = site, NOTIFY = inspector_changed)]
        #[qproperty(QString, wayback, READ = wayback, NOTIFY = inspector_changed)]
        #[qproperty(QString, keep_title, READ = keep_title, NOTIFY = inspector_changed)]
        #[qproperty(bool, can_repair, READ = can_repair, NOTIFY = inspector_changed)]
        #[qproperty(bool, can_bury, READ = can_bury, NOTIFY = inspector_changed)]
        #[qproperty(bool, bury_prominent, READ = bury_prominent, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, redirects, READ = redirects, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, log_dates, READ = log_dates, NOTIFY = inspector_changed)]
        #[qproperty(QStringList, log_outcomes, READ = log_outcomes, NOTIFY = inspector_changed)]
        // Fixed strings.
        #[qproperty(QStringList, grouping_titles, READ = grouping_titles, CONSTANT)]
        #[qproperty(QStringList, rhythm_choices, READ = rhythm_choices, CONSTANT)]
        #[qproperty(QString, no_feed_title, READ = no_feed_title, CONSTANT)]
        #[qproperty(QString, no_feed_text, READ = no_feed_text, CONSTANT)]
        #[qproperty(QString, buried_note, READ = buried_note, CONSTANT)]
        #[qproperty(QString, history_title, READ = history_title, CONSTANT)]
        #[qproperty(QString, not_fetched, READ = not_fetched, CONSTANT)]
        #[qproperty(QString, archived_copy, READ = archived_copy, CONSTANT)]
        type Garden = super::GardenRust;

        fn active(self: &Garden) -> bool;
        fn scope_title(self: &Garden) -> QString;
        fn count_label(self: &Garden) -> QString;
        fn shows_grouping(self: &Garden) -> bool;
        fn grouping(self: &Garden) -> QString;
        fn set_grouping(self: Pin<&mut Garden>, grouping: QString);
        fn shows_apply(self: &Garden) -> bool;
        fn safe_repair_count(self: &Garden) -> i32;
        fn apply_title(self: &Garden) -> QString;
        fn apply_question(self: &Garden) -> QString;
        fn apply_preview(self: &Garden) -> QString;
        fn apply_confirm_label(self: &Garden) -> QString;
        fn confirming(self: &Garden) -> bool;
        fn set_confirming(self: Pin<&mut Garden>, on: bool);
        fn message(self: &Garden) -> QString;
        fn empty_text(self: &Garden) -> QString;
        fn section_titles(self: &Garden) -> QStringList;
        fn selected_row(self: &Garden) -> i32;
        fn selected_feed_id(self: &Garden) -> i64;

        fn has_selection(self: &Garden) -> bool;
        fn title(self: &Garden) -> QString;
        fn folder_path(self: &Garden) -> QString;
        fn status_line(self: &Garden) -> QString;
        fn status_color(self: &Garden) -> QString;
        fn buried(self: &Garden) -> bool;
        fn reason(self: &Garden) -> QString;
        fn rhythm_line(self: &Garden) -> QString;
        fn rhythm_index(self: &Garden) -> i32;
        fn theme_names(self: &Garden) -> QStringList;
        fn theme_index(self: &Garden) -> i32;
        fn evidence(self: &Garden) -> QStringList;
        fn remarks(self: &Garden) -> QStringList;
        fn proposed(self: &Garden) -> QString;
        fn decision(self: &Garden) -> QString;
        fn feed_url(self: &Garden) -> QString;
        fn site(self: &Garden) -> QString;
        fn wayback(self: &Garden) -> QString;
        fn keep_title(self: &Garden) -> QString;
        fn can_repair(self: &Garden) -> bool;
        fn can_bury(self: &Garden) -> bool;
        fn bury_prominent(self: &Garden) -> bool;
        fn redirects(self: &Garden) -> QStringList;
        fn log_dates(self: &Garden) -> QStringList;
        fn log_outcomes(self: &Garden) -> QStringList;

        fn grouping_titles(self: &Garden) -> QStringList;
        fn rhythm_choices(self: &Garden) -> QStringList;
        fn no_feed_title(self: &Garden) -> QString;
        fn no_feed_text(self: &Garden) -> QString;
        fn buried_note(self: &Garden) -> QString;
        fn history_title(self: &Garden) -> QString;
        fn not_fetched(self: &Garden) -> QString;
        fn archived_copy(self: &Garden) -> QString;

        #[cxx_override]
        fn data(self: &Garden, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn row_count(self: &Garden, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn role_names(self: &Garden) -> QHash_i32_QByteArray;

        #[inherit]
        fn begin_insert_rows(self: Pin<&mut Garden>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        fn end_insert_rows(self: Pin<&mut Garden>);
        #[inherit]
        fn begin_remove_rows(self: Pin<&mut Garden>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        fn end_remove_rows(self: Pin<&mut Garden>);
        #[inherit]
        fn index(self: &Garden, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        /// The selection moved: a garden scope is shown (`garden:…` keys) or not.
        #[qinvokable]
        fn show(self: Pin<&mut Garden>, key: &QString);

        /// Reads the garden again and applies the difference.
        #[qinvokable]
        fn reload(self: Pin<&mut Garden>);

        #[qinvokable]
        fn select(self: Pin<&mut Garden>, row: i32);

        /// Selects a listed feed; whether it is listed.
        #[qinvokable]
        fn select_feed(self: Pin<&mut Garden>, feed_id: i64) -> bool;

        #[qinvokable]
        fn row_of_feed(self: &Garden, feed_id: i64) -> i32;

        /// A key the table received (`text`, Qt's modifier mask); whether it was the garden's.
        #[qinvokable]
        fn key(self: Pin<&mut Garden>, text: &QString, modifiers: i32) -> bool;

        /// `"keep"`, `"repair"` or `"bury"` for the selected feed.
        #[qinvokable]
        fn decide(self: Pin<&mut Garden>, decision: &QString);

        #[qinvokable]
        fn undo(self: Pin<&mut Garden>);

        #[qinvokable]
        fn apply_safe(self: Pin<&mut Garden>);

        /// The Rhythm picker's item: 0 is Auto, then Daily to Silent.
        #[qinvokable]
        fn set_cadence(self: Pin<&mut Garden>, index: i32);

        /// The Theme picker's item: 0 is None, then the themes by position.
        #[qinvokable]
        fn set_theme(self: Pin<&mut Garden>, index: i32);

        /// The header, the dialog or the message changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Garden>);

        /// The selected feed or its row changed.
        #[qsignal]
        fn selection_changed(self: Pin<&mut Garden>);

        /// The Theme picker's items changed.
        #[qsignal]
        fn themes_changed(self: Pin<&mut Garden>);

        /// The inspector's content changed.
        #[qsignal]
        fn inspector_changed(self: Pin<&mut Garden>);

        /// `o` or `w` asked to open a web address.
        #[qsignal]
        fn open_url(self: Pin<&mut Garden>, url: &QString);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        fn data_changed(
            self: Pin<&mut Garden>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    impl cxx_qt::Threading for Garden {}
}

/// One line of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    feed_id: FeedId,
    glyph: &'static str,
    title: String,
    clue: String,
    rhythm: String,
    unread: String,
    chip: &'static str,
    dimmed: bool,
    tooltip: String,
    section: String,
}

#[derive(Default)]
pub struct GardenRust {
    pub(crate) visit: GardenVisit,
    rows: Vec<TableRow>,
    /// Each table row's position, by feed.
    row_index: HashMap<FeedId, usize>,
    pub(crate) inspector: Option<Inspector>,
    /// The inspector's fetch log and the feed it was read for: read again only when the
    /// selected feed changes or a decision touched it.
    log: Vec<AttemptRecord>,
    log_feed: Option<FeedId>,
    /// The visit's safe repairs, as of the last sync (five properties read them).
    repairs: Vec<(FeedId, Action)>,
    /// The Theme picker's items, as of the last sync.
    pub(crate) theme_names: Vec<String>,
}

/// The value of `role` (a name of [`ROLES`]) for a row.
fn role_value(row: &TableRow, role: &str) -> QVariant {
    match role {
        "feedId" => QVariant::from(&row.feed_id),
        "glyph" => variant_string(row.glyph),
        "title" => variant_string(&row.title),
        "clue" => variant_string(&row.clue),
        "rhythm" => variant_string(&row.rhythm),
        "unread" => variant_string(&row.unread),
        "chip" => variant_string(row.chip),
        "dimmed" => QVariant::from(&row.dimmed),
        "tooltip" => variant_string(&row.tooltip),
        "section" => variant_string(&row.section),
        _ => QVariant::default(),
    }
}

/// A list of strings for QML.
pub(crate) fn string_list<'a>(items: impl IntoIterator<Item = &'a str>) -> QStringList {
    items.into_iter().map(QString::from).collect()
}

impl qobject::Garden {
    fn active(&self) -> bool {
        self.rust().visit.scope().is_some()
    }

    fn scope_title(&self) -> QString {
        QString::from(self.rust().visit.scope().map_or("", GardenScope::title))
    }

    fn count_label(&self) -> QString {
        QString::from(self.rust().visit.count_label().as_str())
    }

    fn shows_grouping(&self) -> bool {
        self.rust().visit.scope() == Some(GardenScope::All)
    }

    fn grouping(&self) -> QString {
        QString::from(self.rust().visit.grouping().as_str())
    }

    fn set_grouping(mut self: Pin<&mut Self>, grouping: QString) {
        let Some(grouping) = Grouping::parse(&grouping.to_string()) else {
            return;
        };
        self.as_mut()
            .rust_mut()
            .get_mut()
            .visit
            .set_grouping(grouping);
        self.sync();
    }

    fn shows_apply(&self) -> bool {
        self.rust()
            .visit
            .scope()
            .is_some_and(|scope| scope != GardenScope::Cemetery)
    }

    fn repair_count(&self) -> usize {
        self.rust().repairs.len()
    }

    fn safe_repair_count(&self) -> i32 {
        to_i32(self.repair_count())
    }

    fn apply_title(&self) -> QString {
        QString::from(apply_button_title(self.repair_count()).as_str())
    }

    fn apply_question(&self) -> QString {
        QString::from(apply_question(self.repair_count()).as_str())
    }

    fn apply_preview(&self) -> QString {
        let rust = self.rust();
        QString::from(safe_repair_preview(&rust.visit.snapshot().rows, &rust.repairs).as_str())
    }

    fn apply_confirm_label(&self) -> QString {
        QString::from(apply_confirm_label(self.repair_count()))
    }

    fn confirming(&self) -> bool {
        self.rust().visit.confirming()
    }

    fn set_confirming(mut self: Pin<&mut Self>, on: bool) {
        self.as_mut().rust_mut().get_mut().visit.set_confirming(on);
        self.state_changed();
    }

    fn message(&self) -> QString {
        QString::from(self.rust().visit.message().unwrap_or(""))
    }

    fn empty_text(&self) -> QString {
        QString::from(self.rust().visit.empty_text().unwrap_or(""))
    }

    fn section_titles(&self) -> QStringList {
        self.rust()
            .visit
            .sections()
            .iter()
            .map(|section| QString::from(section.header().as_str()))
            .collect()
    }

    fn selected_feed(&self) -> Option<FeedId> {
        self.rust().visit.selected()
    }

    fn selected_row(&self) -> i32 {
        self.selected_feed()
            .map_or(-1, |feed| self.row_of_feed(feed))
    }

    fn selected_feed_id(&self) -> i64 {
        self.selected_feed().unwrap_or(-1)
    }

    fn grouping_titles(&self) -> QStringList {
        string_list(Grouping::ALL.map(Grouping::title))
    }

    fn rhythm_choices(&self) -> QStringList {
        string_list(RHYTHM_CHOICES)
    }

    fn no_feed_title(&self) -> QString {
        QString::from(NO_FEED_TITLE)
    }

    fn no_feed_text(&self) -> QString {
        QString::from(NO_FEED_TEXT)
    }

    fn buried_note(&self) -> QString {
        QString::from(BURIED_NOTE)
    }

    fn history_title(&self) -> QString {
        QString::from(HISTORY_TITLE)
    }

    fn not_fetched(&self) -> QString {
        QString::from(NOT_FETCHED)
    }

    fn archived_copy(&self) -> QString {
        QString::from(ARCHIVED_COPY)
    }

    fn row_of_feed(&self, feed_id: i64) -> i32 {
        self.rust()
            .row_index
            .get(&feed_id)
            .map_or(-1, |at| to_i32(*at))
    }

    /// The table the visit lists now, in section order.
    fn table_rows(&self) -> Vec<TableRow> {
        let visit = &self.rust().visit;
        let snapshot = visit.snapshot();
        let in_all = visit.scope() == Some(GardenScope::All);
        let mut table = Vec::new();
        for section in visit.sections() {
            let header = section.header();
            for feed in section.rows {
                let Some(row) = visit.row(feed) else { continue };
                let unread = snapshot.unread.get(&feed).copied().unwrap_or(0);
                table.push(TableRow {
                    feed_id: feed,
                    glyph: status_glyph(row.status()),
                    title: row.title.clone(),
                    clue: row
                        .diagnosis
                        .as_ref()
                        .map_or_else(String::new, |d| d.evidence.clue.clone()),
                    rhythm: if in_all {
                        rhythm_label(snapshot.cadence.get(&feed)).unwrap_or_default()
                    } else {
                        String::new()
                    },
                    unread: unread_label(row, unread).unwrap_or_default(),
                    chip: chip(row).unwrap_or(""),
                    dimmed: visit.is_dimmed(feed),
                    tooltip: tooltip(row),
                    section: header.clone(),
                });
            }
        }
        table
    }

    /// Applies the visit's table, the inspector and the signals for what moved.
    fn sync(mut self: Pin<&mut Self>) {
        let new = self.table_rows();
        for op in plan_rows(&self.rust().rows, &new, |row| row.feed_id) {
            self.as_mut().apply(op);
        }
        self.as_mut().rust_mut().get_mut().row_index = self
            .rust()
            .rows
            .iter()
            .enumerate()
            .map(|(at, row)| (row.feed_id, at))
            .collect();
        let repairs = self.rust().visit.safe_repairs();
        self.as_mut().rust_mut().get_mut().repairs = repairs;
        let names = theme_names(&self.rust().visit.snapshot().themes);
        if names != self.rust().theme_names {
            self.as_mut().rust_mut().get_mut().theme_names = names;
            self.as_mut().themes_changed();
        }
        self.as_mut().refresh_inspector();
        self.as_mut().state_changed();
        // Always: a decision moves the selection before the sync, and the rows move in it, so
        // the selected row is not comparable with an earlier one. The table's follower is idempotent.
        self.selection_changed();
    }

    /// The selected feed's inspector, with its fetch log.
    fn refresh_inspector(mut self: Pin<&mut Self>) {
        let feed = self.selected_feed();
        if self.rust().log_feed != feed {
            let log = feed
                .and_then(|feed| with_session(|session| session.store.attempts(feed, LOG_LINES)))
                .and_then(|read| {
                    read.inspect_err(|error| tracing::warn!("cannot read the fetch log: {error}"))
                        .ok()
                })
                .unwrap_or_default();
            let rust = self.as_mut().rust_mut().get_mut();
            rust.log = log;
            rust.log_feed = feed;
        }
        let inspector = self
            .rust()
            .visit
            .inspector(&self.rust().log, Utc::now(), &chrono::Local);
        if inspector != self.rust().inspector {
            self.as_mut().rust_mut().get_mut().inspector = inspector;
            self.inspector_changed();
        }
    }

    fn show(mut self: Pin<&mut Self>, key: &QString) {
        match Selection::parse_key(&key.to_string()) {
            Some(Selection::Garden(scope)) => {
                self.as_mut().rust_mut().get_mut().visit.enter(scope);
                self.reload();
            }
            _ => {
                self.as_mut().rust_mut().get_mut().visit.leave();
                self.sync();
            }
        }
    }

    fn reload(mut self: Pin<&mut Self>) {
        if self.rust().visit.scope().is_none() {
            return;
        }
        // Whatever ran (a refresh, a decision in another visit) may have added fetch attempts.
        self.as_mut().rust_mut().get_mut().log_feed = None;
        match with_session(|session| load(&session.store)) {
            Some(Ok(snapshot)) => self.as_mut().rust_mut().get_mut().visit.update(snapshot),
            Some(Err(error)) => {
                tracing::warn!("cannot read the garden: {error}");
                self.as_mut()
                    .rust_mut()
                    .get_mut()
                    .visit
                    .set_message(Some(COULD_NOT_READ_GARDEN));
            }
            None => {}
        }
        self.sync();
    }

    fn select(self: Pin<&mut Self>, row: i32) {
        let feed = usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().rows.get(row))
            .map(|row| row.feed_id);
        if let Some(feed) = feed {
            self.select_feed(feed);
        }
    }

    fn select_feed(mut self: Pin<&mut Self>, feed_id: i64) -> bool {
        let listed = self.row_of_feed(feed_id) >= 0;
        if self.as_mut().rust_mut().get_mut().visit.select(feed_id) {
            self.as_mut().selection_changed();
            self.refresh_inspector();
        }
        listed
    }

    fn key(mut self: Pin<&mut Self>, text: &QString, modifiers: i32) -> bool {
        let modifiers = u32::try_from(modifiers).unwrap_or(0);
        let found = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .visit
            .key(&text.to_string(), modifiers);
        let Some(command) = found else {
            return false;
        };
        match command {
            Command::Decide(decision) => self.as_mut().run_decision(decision),
            Command::Undo => self.as_mut().run_undo(),
            Command::Open(url) => self.as_mut().open_url(&QString::from(url.as_str())),
            Command::ConfirmSafeRepairs | Command::Message(_) => {}
        }
        self.state_changed();
        true
    }

    fn decide(self: Pin<&mut Self>, decision: &QString) {
        if let Some(decision) = Decision::parse(&decision.to_string()) {
            self.run_decision(decision);
        }
    }

    /// The selected feed, when a scope is shown.
    fn target(&self) -> Option<FeedId> {
        self.rust().visit.scope().and(self.selected_feed())
    }

    fn run_decision(self: Pin<&mut Self>, decision: Decision) {
        let Some(feed) = self.target() else { return };
        let qt = self.qt_thread();
        let epoch = self.rust().visit.epoch();
        submit_short(
            RunRequest::Decide(feed, decision),
            move |cx| ressac_ui::garden::decide(cx.app.store(), feed, decision, Utc::now()),
            move |end| {
                let queued = qt.queue(move |garden| garden.decision_ended(epoch, feed, end));
                if queued.is_err() {
                    tracing::debug!("the app quit before a decision finished");
                }
            },
        );
    }

    /// A decision run ended, on the GUI thread: a recorded one stays listed, dimmed. (A
    /// failure or a refusal is the banner's, from `Runs`; the reload is `databaseChanged`'s.)
    /// A run that ended in another visit (the scope changed meanwhile) only reloads: the
    /// decision did land, but its memory belongs to the visit that asked.
    fn decision_ended(mut self: Pin<&mut Self>, epoch: u64, feed: FeedId, end: RunEnd<()>) {
        if let RunEnd::Done(()) = end {
            if self.rust().visit.epoch() == epoch {
                let rust = self.as_mut().rust_mut().get_mut();
                rust.visit.decided(feed);
                rust.log_feed = None;
                self.sync();
            } else {
                self.reload();
            }
        }
    }

    fn undo(self: Pin<&mut Self>) {
        self.run_undo();
    }

    fn run_undo(self: Pin<&mut Self>) {
        if self.rust().visit.scope().is_none() {
            return;
        }
        let qt = self.qt_thread();
        let epoch = self.rust().visit.epoch();
        submit_short(
            RunRequest::Undo,
            |cx| ressac_core::garden::undo(cx.app.store()),
            move |end| {
                let queued = qt.queue(move |garden| garden.undo_ended(epoch, end));
                if queued.is_err() {
                    tracing::debug!("the app quit before an undo finished");
                }
            },
        );
    }

    fn undo_ended(mut self: Pin<&mut Self>, epoch: u64, end: RunEnd<Option<FeedId>>) {
        if let RunEnd::Done(undone) = end {
            if self.rust().visit.epoch() == epoch {
                let rust = self.as_mut().rust_mut().get_mut();
                rust.visit.undone(undone);
                rust.log_feed = None;
                self.sync();
            } else {
                self.reload();
            }
        }
    }

    fn apply_safe(mut self: Pin<&mut Self>) {
        if self.rust().visit.scope().is_none() {
            return;
        }
        self.as_mut()
            .rust_mut()
            .get_mut()
            .visit
            .set_confirming(false);
        self.as_mut().state_changed();
        submit_short(
            RunRequest::ApplySafe,
            |cx| ressac_core::garden::apply_safe_repairs(cx.app.store(), Utc::now()).map(drop),
            |_| {},
        );
    }

    fn set_cadence(self: Pin<&mut Self>, index: i32) {
        let Some(feed) = self.target() else { return };
        let Some(group) = usize::try_from(index).ok().and_then(rhythm_choice) else {
            return;
        };
        submit_short(
            RunRequest::SetCadence(feed, group),
            move |cx| cx.app.set_cadence(feed, group).map(drop),
            |_| {},
        );
    }

    fn set_theme(self: Pin<&mut Self>, index: i32) {
        let Some(feed) = self.target() else { return };
        let choice = usize::try_from(index)
            .ok()
            .and_then(|index| theme_choice(&self.rust().visit.snapshot().themes, index));
        let Some(key) = choice else { return };
        submit_short(
            RunRequest::SetTheme(feed, key.clone()),
            move |cx| cx.app.store().set_feed_theme(feed, key.as_deref()),
            |_| {},
        );
    }
}

list_model_rows!(qobject::Garden, rows: TableRow, ROLES, role_value);
