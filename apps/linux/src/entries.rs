//! The `EntriesModel` QML singleton: the entries pane's list as a list model. Paging by the
//! Mac's floor rule, retained rows, the unread filter, search and the optimistic edits are
//! `ressac_ui::entries`'; this object applies the rows by id (no reset: the selection and the
//! scroll position survive every reload) and runs the writes on the worker's direct lane.

use std::collections::HashMap;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QVariant};
use ressac_core::clock::today_start;
use ressac_core::model::FeedId;
use ressac_core::store::{JournalView, ListFilter, ListQuery, Source, Store};
use ressac_ui::chrono::{self, DateTime, Utc};
use ressac_ui::entries::{AUTO_READ_DELAY, EntryList, ListRow, LocalEdit, relative_date};
use ressac_ui::journal::{JournalRow, overflow_label, section_title, sections};
use ressac_ui::live::plan_rows;
use ressac_ui::runs::{DirectWrite, read_failure, write_failure};
use ressac_ui::sources::Selection;
use ressac_ui::worker::JobResult;

use crate::launcher::with_session;
use crate::rows::{list_model_rows, to_i32, variant_string};

/// The role names QML sees, in role order (`FIRST_ROLE + index`).
const ROLES: [&str; 11] = [
    "entryId",
    "feedTitle",
    "title",
    "excerpt",
    "date",
    "read",
    "starred",
    "link",
    "kind",
    "section",
    "overflowFeed",
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
        #[qproperty(bool, unread_only, READ, WRITE = set_unread_only, NOTIFY)]
        #[qproperty(QString, search_text, READ = search_text, WRITE = set_search_text, NOTIFY)]
        #[qproperty(i32, selected_row, READ = selected_row, NOTIFY = selection_changed)]
        #[qproperty(i64, selected_entry_id, READ = selected_entry_id, NOTIFY = selection_changed)]
        #[qproperty(bool, active, READ = active, NOTIFY = list_changed)]
        #[qproperty(bool, journal, READ = journal, NOTIFY = list_changed)]
        #[qproperty(QString, empty_text, READ = empty_text, NOTIFY = list_changed)]
        #[qproperty(bool, can_mark_all_read, READ = can_mark_all_read, NOTIFY = list_changed)]
        #[qproperty(bool, can_navigate, READ = can_navigate, NOTIFY = list_changed)]
        #[qproperty(bool, can_toggle, READ = can_toggle, NOTIFY = selected_state_changed)]
        #[qproperty(bool, selected_read, READ = selected_read, NOTIFY = selected_state_changed)]
        #[qproperty(bool, selected_starred, READ = selected_starred, NOTIFY = selected_state_changed)]
        #[qproperty(i32, auto_read_delay, READ = auto_read_delay, CONSTANT)]
        type EntriesModel = super::EntriesModelRust;

        fn search_text(self: &EntriesModel) -> QString;
        fn selected_row(self: &EntriesModel) -> i32;
        fn selected_entry_id(self: &EntriesModel) -> i64;
        fn active(self: &EntriesModel) -> bool;
        fn journal(self: &EntriesModel) -> bool;
        fn empty_text(self: &EntriesModel) -> QString;
        fn can_mark_all_read(self: &EntriesModel) -> bool;
        fn can_navigate(self: &EntriesModel) -> bool;
        fn can_toggle(self: &EntriesModel) -> bool;
        fn selected_read(self: &EntriesModel) -> bool;
        fn selected_starred(self: &EntriesModel) -> bool;
        fn auto_read_delay(self: &EntriesModel) -> i32;

        fn set_unread_only(self: Pin<&mut EntriesModel>, unread_only: bool);
        fn set_search_text(self: Pin<&mut EntriesModel>, text: &QString);

        #[cxx_override]
        fn data(self: &EntriesModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn row_count(self: &EntriesModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn role_names(self: &EntriesModel) -> QHash_i32_QByteArray;

        #[inherit]
        fn begin_insert_rows(
            self: Pin<&mut EntriesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_insert_rows(self: Pin<&mut EntriesModel>);
        #[inherit]
        fn begin_remove_rows(
            self: Pin<&mut EntriesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_remove_rows(self: Pin<&mut EntriesModel>);
        #[inherit]
        fn index(self: &EntriesModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        /// Lists `key`'s entries (a garden or unknown key shows no list).
        #[qinvokable]
        fn show_source(self: Pin<&mut EntriesModel>, key: &QString);

        /// Reads the rows again down to the floor and applies the difference.
        #[qinvokable]
        fn reload(self: Pin<&mut EntriesModel>);

        /// A delegate was created: near the end of the loaded rows, one more page loads.
        #[qinvokable]
        fn row_appeared(self: Pin<&mut EntriesModel>, row: i32);

        #[qinvokable]
        fn select(self: Pin<&mut EntriesModel>, row: i32);

        /// Selects the loaded row of `entry_id`; nothing for an unknown one.
        #[qinvokable]
        fn select_entry(self: Pin<&mut EntriesModel>, entry_id: i64);

        /// Selects row 0 when nothing is selected.
        #[qinvokable]
        fn select_first_if_none(self: Pin<&mut EntriesModel>);

        /// `j` and `↓`: selects the next row, marking the unread one it leaves as read; false at
        /// the end of the list (nothing moved).
        #[qinvokable]
        fn select_next(self: Pin<&mut EntriesModel>) -> bool;

        /// `k` and `↑`: selects the previous row, no read mark; false at the top.
        #[qinvokable]
        fn select_previous(self: Pin<&mut EntriesModel>) -> bool;

        /// Space at the end of a page: the next unread row, marking the unread one it leaves.
        #[qinvokable]
        fn select_next_unread(self: Pin<&mut EntriesModel>) -> bool;

        /// The selection stayed on `entry_id` for `autoReadDelay`: marks it read when it still
        /// is the selected, loaded, unread row.
        #[qinvokable]
        fn auto_read(self: Pin<&mut EntriesModel>, entry_id: i64);

        #[qinvokable]
        fn toggle_read(self: Pin<&mut EntriesModel>, row: i32);

        #[qinvokable]
        fn toggle_starred(self: Pin<&mut EntriesModel>, row: i32);

        #[qinvokable]
        fn mark_all_read(self: Pin<&mut EntriesModel>);

        /// Mark All as Read from a feed's menu: `key` may be another source than the list's.
        #[qinvokable]
        fn mark_all_read_in(self: Pin<&mut EntriesModel>, key: &QString);

        #[qsignal]
        fn selection_changed(self: Pin<&mut EntriesModel>);

        #[qsignal]
        fn list_changed(self: Pin<&mut EntriesModel>);

        /// Whether the selected row exists, is read or is starred changed.
        #[qsignal]
        fn selected_state_changed(self: Pin<&mut EntriesModel>);

        /// A failed read or write: for the window's banner.
        #[qsignal]
        fn banner(self: Pin<&mut EntriesModel>, message: &QString, show_log: bool);

        /// An own write ended (landed or reverted): the window reloads.
        #[qsignal]
        fn wrote(self: Pin<&mut EntriesModel>);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        fn data_changed(
            self: Pin<&mut EntriesModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    impl cxx_qt::Threading for EntriesModel {}
}

/// A shown row: the list row and its date text, which moves with the clock.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    row: ListRow,
    date: String,
}

/// A row of the model: an entry, or (in the Journal) the "+N more in Feed" row after its
/// feed. `section` is the Journal section's title, `""` outside the Journal.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Entry {
        shown: Shown,
        section: &'static str,
    },
    Overflow {
        id: String,
        section: &'static str,
        label: String,
        feed_id: FeedId,
    },
}

impl Slot {
    /// The identity the row plan follows: `entry-<id>` or the overflow row's own.
    fn key(&self) -> String {
        match self {
            Slot::Entry { shown, .. } => format!("entry-{}", shown.row.entry_id),
            Slot::Overflow { id, .. } => id.clone(),
        }
    }

    /// The entry of an entry slot; `None` for an overflow row.
    fn entry(&self) -> Option<&Shown> {
        match self {
            Slot::Entry { shown, .. } => Some(shown),
            Slot::Overflow { .. } => None,
        }
    }
}

#[derive(Default)]
pub struct EntriesModelRust {
    list: Option<EntryList>,
    /// The Journal's latest edition, read with the list's source (never by a local edit).
    edition: Option<JournalView>,
    slots: Vec<Slot>,
    selected: Option<i64>,
    unread_only: bool,
    search: String,
    /// Bumped whenever the list is replaced: a write's answer for an older list only reports.
    epoch: u64,
}

/// What the properties show, to tell QML only what moved.
#[derive(Debug, PartialEq)]
struct View {
    active: bool,
    journal: bool,
    empty_text: &'static str,
    can_mark_all_read: bool,
    can_navigate: bool,
    selected_row: i32,
    selected_entry_id: i64,
    can_toggle: bool,
    selected_read: bool,
    selected_starred: bool,
}

fn role_value(slot: &Slot, role: &str) -> QVariant {
    match (slot, role) {
        (Slot::Entry { .. }, "kind") => variant_string("entry"),
        (Slot::Overflow { .. }, "kind") => variant_string("overflow"),
        (Slot::Entry { section, .. } | Slot::Overflow { section, .. }, "section") => {
            variant_string(section)
        }
        (Slot::Overflow { feed_id, .. }, "overflowFeed") => QVariant::from(feed_id),
        (Slot::Entry { .. }, "overflowFeed") => QVariant::from(&0_i64),
        (Slot::Overflow { label, .. }, "title") => variant_string(label),
        (Slot::Overflow { .. }, "entryId") => QVariant::from(&-1_i64),
        (Slot::Overflow { .. }, "read" | "starred") => QVariant::from(&false),
        (Slot::Overflow { .. }, "feedTitle" | "excerpt" | "date" | "link") => variant_string(""),
        (Slot::Entry { shown, .. }, role) => entry_value(shown, role),
        _ => QVariant::default(),
    }
}

fn entry_value(shown: &Shown, role: &str) -> QVariant {
    let row = &shown.row;
    match role {
        "entryId" => QVariant::from(&row.entry_id),
        "feedTitle" => variant_string(&row.feed_title),
        "title" => variant_string(&row.title),
        "excerpt" => variant_string(&row.excerpt),
        "date" => variant_string(&shown.date),
        "read" => QVariant::from(&row.is_read),
        "starred" => QVariant::from(&row.is_starred),
        "link" => variant_string(row.link.as_deref().unwrap_or("")),
        _ => QVariant::default(),
    }
}

/// The list's query for `source` under the pane's filter and search box.
fn query_for(source: Source, unread_only: bool, search: &str) -> ListQuery {
    let search = search.trim();
    ListQuery {
        source,
        filter: if unread_only {
            ListFilter::Unread
        } else {
            ListFilter::All
        },
        search: (!search.is_empty()).then(|| search.to_string()),
    }
}

impl qobject::EntriesModel {
    fn search_text(&self) -> QString {
        QString::from(self.rust().search.as_str())
    }

    fn selected_row(&self) -> i32 {
        let rust = self.rust();
        let selected = rust.selected;
        rust.slots
            .iter()
            .position(|slot| {
                slot.entry()
                    .is_some_and(|s| Some(s.row.entry_id) == selected)
            })
            .map_or(-1, to_i32)
    }

    fn selected_entry_id(&self) -> i64 {
        self.rust().selected.unwrap_or(-1)
    }

    fn active(&self) -> bool {
        self.rust().list.is_some()
    }

    /// The list's source is the Journal: its rows carry sections.
    fn journal(&self) -> bool {
        self.rust()
            .list
            .as_ref()
            .is_some_and(|list| list.query().source == Source::Journal)
    }

    fn empty_text(&self) -> QString {
        let text = self.rust().list.as_ref().and_then(EntryList::empty_text);
        QString::from(text.unwrap_or(""))
    }

    fn can_mark_all_read(&self) -> bool {
        self.rust()
            .list
            .as_ref()
            .is_some_and(EntryList::can_mark_all_read)
    }

    /// Whether the keys can move through the list: a list is shown and has rows.
    fn can_navigate(&self) -> bool {
        self.rust()
            .list
            .as_ref()
            .is_some_and(|list| !list.rows().is_empty())
    }

    fn can_toggle(&self) -> bool {
        self.rust().list.is_some() && self.selected_shown().is_some()
    }

    fn selected_read(&self) -> bool {
        self.selected_shown().is_some_and(|shown| shown.row.is_read)
    }

    fn selected_starred(&self) -> bool {
        self.selected_shown()
            .is_some_and(|shown| shown.row.is_starred)
    }

    fn auto_read_delay(&self) -> i32 {
        i32::try_from(AUTO_READ_DELAY.as_millis()).unwrap_or(i32::MAX)
    }

    fn selected_shown(&self) -> Option<&Shown> {
        let selected = self.rust().selected?;
        self.rust()
            .slots
            .iter()
            .filter_map(Slot::entry)
            .find(|shown| shown.row.entry_id == selected)
    }

    fn view(&self) -> View {
        let rust = self.rust();
        View {
            active: rust.list.is_some(),
            journal: self.journal(),
            empty_text: rust
                .list
                .as_ref()
                .and_then(EntryList::empty_text)
                .unwrap_or(""),
            can_mark_all_read: self.can_mark_all_read(),
            can_navigate: self.can_navigate(),
            selected_row: self.selected_row(),
            selected_entry_id: self.selected_entry_id(),
            can_toggle: self.can_toggle(),
            selected_read: self.selected_read(),
            selected_starred: self.selected_starred(),
        }
    }

    /// The rows of the list as shown now, with their relative dates: in the Journal laid out
    /// by section with the overflow rows, elsewhere one entry slot per row.
    fn shown_slots(&self) -> Vec<Slot> {
        let now = Utc::now();
        let rust = self.rust();
        let Some(list) = rust.list.as_ref() else {
            return Vec::new();
        };
        let shown = list.rows().iter().map(|row| Shown {
            row: row.clone(),
            date: relative_date(row.published_at, now),
        });
        if list.query().source != Source::Journal {
            return shown
                .map(|shown| Slot::Entry { shown, section: "" })
                .collect();
        }
        let listed: Vec<(i64, FeedId)> = list
            .rows()
            .iter()
            .map(|row| (row.entry_id, row.feed_id))
            .collect();
        let mut by_id: HashMap<i64, Shown> = shown.map(|s| (s.row.entry_id, s)).collect();
        let mut slots = Vec::new();
        for section in sections(&listed, rust.edition.as_ref()) {
            let title = section_title(section.section);
            for row in section.rows {
                match row {
                    JournalRow::Entry(id) => {
                        if let Some(shown) = by_id.remove(&id) {
                            slots.push(Slot::Entry {
                                shown,
                                section: title,
                            });
                        }
                    }
                    JournalRow::Overflow(overflow) => slots.push(Slot::Overflow {
                        id: JournalRow::Overflow(overflow.clone()).id(),
                        section: title,
                        label: overflow_label(&overflow),
                        feed_id: overflow.feed_id,
                    }),
                }
            }
        }
        slots
    }

    /// Reads the Journal's edition when the list's source is the Journal (else forgets it);
    /// an unreadable one is a banner, the sections stay as they were.
    fn read_edition(mut self: Pin<&mut Self>) {
        if !self.as_ref().journal() {
            self.as_mut().rust_mut().get_mut().edition = None;
            return;
        }
        match with_session(|session| session.store.journal()) {
            Some(Ok(edition)) => self.as_mut().rust_mut().get_mut().edition = edition,
            Some(Err(error)) => self.report_read(&error),
            None => {}
        }
    }

    /// Applies the list's rows to the model by id, drops a selection whose row is gone, and
    /// tells QML what moved since `before` (a [`View`] taken before the change).
    fn sync(mut self: Pin<&mut Self>, before: &View) {
        let new = self.shown_slots();
        let ops = plan_rows(&self.rust().slots, &new, Slot::key);
        for op in ops {
            self.as_mut().apply(op);
        }
        let selected = self.rust().selected;
        if selected.is_some_and(|id| {
            !new.iter()
                .filter_map(Slot::entry)
                .any(|shown| shown.row.entry_id == id)
        }) {
            self.as_mut().rust_mut().get_mut().selected = None;
        }
        let after = self.view();
        if (after.selected_row, after.selected_entry_id)
            != (before.selected_row, before.selected_entry_id)
        {
            self.as_mut().selection_changed();
        }
        if (
            after.active,
            after.journal,
            after.empty_text,
            after.can_mark_all_read,
            after.can_navigate,
        ) != (
            before.active,
            before.journal,
            before.empty_text,
            before.can_mark_all_read,
            before.can_navigate,
        ) {
            self.as_mut().list_changed();
        }
        if (
            after.can_toggle,
            after.selected_read,
            after.selected_starred,
        ) != (
            before.can_toggle,
            before.selected_read,
            before.selected_starred,
        ) {
            self.selected_state_changed();
        }
    }

    fn show_banner_for(self: Pin<&mut Self>, banner: ressac_ui::runs::Banner) {
        self.banner(&QString::from(banner.message.as_str()), banner.show_log);
    }

    /// An unreadable list is a banner, not a crash: the rows stay as they were.
    fn report_read(self: Pin<&mut Self>, error: &ressac_core::Error) {
        tracing::warn!("cannot read the entries: {error}");
        self.show_banner_for(read_failure(error));
    }

    /// The entry at `row`: `None` when out of range (an empty list, a stale index from QML)
    /// and for an overflow row, which no action touches.
    fn shown_at(&self, row: i32) -> Option<&Shown> {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().slots.get(row))
            .and_then(Slot::entry)
    }

    fn show_source(mut self: Pin<&mut Self>, key: &QString) {
        let before = self.view();
        let source =
            Selection::parse_key(&key.to_string()).and_then(|selection| selection.source());
        let Some(source) = source else {
            // A garden (B5b) or unknown key: no list.
            let rust = self.as_mut().rust_mut().get_mut();
            rust.list = None;
            rust.selected = None;
            rust.epoch += 1;
            self.as_mut().read_edition();
            self.sync(&before);
            return;
        };
        let query = query_for(source, self.rust().unread_only, &self.rust().search);
        let keeping = self.rust().selected;
        let today = today_start(chrono::Local::now());
        let same_source = self
            .rust()
            .list
            .as_ref()
            .is_some_and(|list| list.query().source == source);
        let loaded = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            if same_source && let Some(list) = rust.list.as_mut() {
                return list.change(query, &session.store, keeping, today);
            }
            // Another source: a fresh list, and the selection of the last one is forgotten.
            rust.selected = None;
            rust.epoch += 1;
            rust.list
                .insert(EntryList::new(query))
                .restart(&session.store, None, today)
        });
        if let Some(Err(error)) = &loaded {
            self.as_mut().report_read(error);
        }
        self.as_mut().read_edition();
        self.sync(&before);
    }

    /// The same source under the current filter and search, keeping the selected row loaded.
    fn requery(mut self: Pin<&mut Self>) {
        let before = self.view();
        let Some(source) = self.rust().list.as_ref().map(|list| list.query().source) else {
            return;
        };
        let query = query_for(source, self.rust().unread_only, &self.rust().search);
        let keeping = self.rust().selected;
        let today = today_start(chrono::Local::now());
        let loaded = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            rust.list
                .as_mut()
                .map(|list| list.change(query, &session.store, keeping, today))
        });
        if let Some(Some(Err(error))) = &loaded {
            self.as_mut().report_read(error);
        }
        self.sync(&before);
    }

    fn set_unread_only(mut self: Pin<&mut Self>, unread_only: bool) {
        if self.rust().unread_only == unread_only {
            return;
        }
        self.as_mut().rust_mut().get_mut().unread_only = unread_only;
        self.as_mut().unread_only_changed();
        self.requery();
    }

    fn set_search_text(mut self: Pin<&mut Self>, text: &QString) {
        let text = text.to_string();
        // The search field's two-way wiring echoes the value back: an unchanged one is a no-op.
        if self.rust().search == text {
            return;
        }
        self.as_mut().rust_mut().get_mut().search = text;
        self.as_mut().search_text_changed();
        self.requery();
    }

    fn reload(mut self: Pin<&mut Self>) {
        let before = self.view();
        let today = today_start(chrono::Local::now());
        let reloaded = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            rust.list.as_mut().map(|list| {
                list.reload(&session.store, today)
                    .map(|restart| restart.then(|| list.generation()))
            })
        });
        match reloaded {
            Some(Some(Ok(Some(generation)))) => {
                // The list outgrew a page: restart it on the next turn.
                let epoch = self.rust().epoch;
                let qt = self.qt_thread();
                if qt
                    .queue(move |model| model.restart_if_due(epoch, generation))
                    .is_err()
                {
                    tracing::debug!("the app quit before a restart");
                }
            }
            Some(Some(Err(error))) => self.as_mut().report_read(&error),
            _ => {}
        }
        self.as_mut().read_edition();
        self.sync(&before);
    }

    /// The restart a reload asked for, if its list is still the one that wanted it.
    fn restart_if_due(mut self: Pin<&mut Self>, epoch: u64, generation: u64) {
        if self.rust().epoch != epoch {
            return;
        }
        let before = self.view();
        let keeping = self.rust().selected;
        let today = today_start(chrono::Local::now());
        let restarted = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            rust.list
                .as_mut()
                .filter(|list| list.restart_due(generation))
                .map(|list| list.restart(&session.store, keeping, today))
        });
        if let Some(Some(Err(error))) = &restarted {
            self.as_mut().report_read(error);
        }
        self.sync(&before);
    }

    fn row_appeared(mut self: Pin<&mut Self>, row: i32) {
        let Some(id) = self.shown_at(row).map(|shown| shown.row.entry_id) else {
            return;
        };
        let scheduled = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .list
            .as_mut()
            .and_then(|list| list.row_appeared(id));
        if let Some(generation) = scheduled {
            // Never inside a delegate's creation: the page loads on the next turn.
            let epoch = self.rust().epoch;
            let qt = self.qt_thread();
            if qt
                .queue(move |model| model.load_page(epoch, generation))
                .is_err()
            {
                tracing::debug!("the app quit before a page loaded");
            }
        }
    }

    fn load_page(mut self: Pin<&mut Self>, epoch: u64, generation: u64) {
        if self.rust().epoch != epoch {
            return;
        }
        let before = self.view();
        let today = today_start(chrono::Local::now());
        let loaded = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            let list = rust.list.as_mut()?;
            // The mark clears either way; the page loads only for the list that asked.
            list.scheduled_page_due(generation)
                .then(|| list.load_more(&session.store, today))
        });
        if let Some(Some(Err(error))) = &loaded {
            self.as_mut().report_read(error);
        }
        self.sync(&before);
    }

    fn select(mut self: Pin<&mut Self>, row: i32) {
        let Some(id) = self.shown_at(row).map(|shown| shown.row.entry_id) else {
            return;
        };
        let before = self.view();
        self.as_mut().rust_mut().get_mut().selected = Some(id);
        self.sync(&before);
    }

    fn select_entry(mut self: Pin<&mut Self>, entry_id: i64) {
        if self.rust().selected == Some(entry_id)
            || !self
                .rust()
                .slots
                .iter()
                .filter_map(Slot::entry)
                .any(|shown| shown.row.entry_id == entry_id)
        {
            return;
        }
        let before = self.view();
        self.as_mut().rust_mut().get_mut().selected = Some(entry_id);
        self.sync(&before);
    }

    fn select_first_if_none(self: Pin<&mut Self>) {
        if self.rust().selected.is_some() {
            return;
        }
        let first = self
            .rust()
            .slots
            .iter()
            .find_map(Slot::entry)
            .map(|shown| shown.row.entry_id);
        if let Some(id) = first {
            self.select_entry(id);
        }
    }

    /// Moves the selection to what `find` answers; `leaving` marks the unread row left behind
    /// as read (the optimistic write of `toggleRead`). True when the selection moved.
    fn step(
        mut self: Pin<&mut Self>,
        leaving: bool,
        find: impl FnOnce(
            &mut EntryList,
            &Store,
            Option<i64>,
            DateTime<Utc>,
        ) -> ressac_core::Result<Option<i64>>,
    ) -> bool {
        let before = self.view();
        let selected = self.rust().selected;
        let today = today_start(chrono::Local::now());
        let found = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            rust.list
                .as_mut()
                .map(|list| find(list, &session.store, selected, today))
        });
        // The search may have loaded rows.
        self.as_mut().sync(&before);
        let target = match found {
            Some(Some(Ok(target))) => target,
            Some(Some(Err(error))) => {
                self.report_read(&error);
                return false;
            }
            _ => return false,
        };
        let Some(target) = target.filter(|id| Some(*id) != selected) else {
            return false;
        };
        if leaving && let Some(id) = selected.filter(|_| !self.selected_read()) {
            self.as_mut().set_read(id, true);
        }
        self.select_entry(target);
        true
    }

    fn select_next(self: Pin<&mut Self>) -> bool {
        self.step(true, EntryList::next_entry)
    }

    fn select_previous(self: Pin<&mut Self>) -> bool {
        self.step(false, |list, _, selected, _| {
            Ok(list.previous_entry(selected))
        })
    }

    fn select_next_unread(self: Pin<&mut Self>) -> bool {
        self.step(true, EntryList::next_unread)
    }

    fn auto_read(self: Pin<&mut Self>, entry_id: i64) {
        let due = self
            .rust()
            .list
            .as_ref()
            .is_some_and(|list| list.auto_read_due(self.rust().selected, entry_id));
        if due {
            self.set_read(entry_id, true);
        }
    }

    fn toggle_read(self: Pin<&mut Self>, row: i32) {
        let Some((id, read)) = self
            .shown_at(row)
            .map(|shown| (shown.row.entry_id, !shown.row.is_read))
        else {
            return;
        };
        self.set_read(id, read);
    }

    /// The optimistic read write: the row changes at once, the worker's answer settles or
    /// reverts it (with a banner).
    fn set_read(mut self: Pin<&mut Self>, id: i64, read: bool) {
        let before = self.view();
        let edit = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .list
            .as_mut()
            .map(|list| list.set_read_local(&[id], read));
        self.as_mut().sync(&before);
        self.write(
            edit,
            DirectWrite::SetRead {
                ids: vec![id],
                read,
            },
        );
    }

    fn toggle_starred(mut self: Pin<&mut Self>, row: i32) {
        let Some((id, starred)) = self
            .shown_at(row)
            .map(|shown| (shown.row.entry_id, !shown.row.is_starred))
        else {
            return;
        };
        let before = self.view();
        let edit = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .list
            .as_mut()
            .map(|list| list.set_starred_local(id, starred));
        self.as_mut().sync(&before);
        self.write(edit, DirectWrite::SetStarred { id, starred });
    }

    fn mark_all_read(mut self: Pin<&mut Self>) {
        let Some(source) = self
            .rust()
            .list
            .as_ref()
            .filter(|list| list.can_mark_all_read())
            .map(|list| list.query().source)
        else {
            return;
        };
        let before = self.view();
        let edit = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .list
            .as_mut()
            .map(EntryList::mark_all_read_local);
        self.as_mut().sync(&before);
        let today_start = today_start(chrono::Local::now());
        self.write(
            edit,
            DirectWrite::MarkAllRead {
                source,
                today_start,
            },
        );
    }

    fn mark_all_read_in(self: Pin<&mut Self>, key: &QString) {
        let Some(source) = Selection::parse_key(&key.to_string()).and_then(|s| s.source()) else {
            return;
        };
        let current = self.rust().list.as_ref().map(|list| list.query().source);
        if current == Some(source) {
            self.mark_all_read();
            return;
        }
        // Another source than the list's: only the write (the sidebar's badge reloads).
        let today_start = today_start(chrono::Local::now());
        self.write(
            None,
            DirectWrite::MarkAllRead {
                source,
                today_start,
            },
        );
    }

    /// Runs `write` on the worker's direct lane; its answer settles or reverts `edit`.
    fn write(mut self: Pin<&mut Self>, edit: Option<LocalEdit>, write: DirectWrite) {
        let epoch = self.rust().epoch;
        let qt = self.qt_thread();
        let for_revert = edit.clone();
        let queued = with_session(|session| {
            session.worker.direct(
                move |cx| write.apply(cx.app.store(), Utc::now()),
                move |result| {
                    let queued = qt.queue(move |model| model.write_done(epoch, edit, result));
                    if queued.is_err() {
                        tracing::debug!("the app quit before a write finished");
                    }
                },
            );
        });
        if queued.is_none() {
            // No session (cannot happen once the window is up): take the edit back.
            let before = self.view();
            if let Some(edit) = for_revert
                && let Some(list) = self.as_mut().rust_mut().get_mut().list.as_mut()
            {
                list.revert(edit);
            }
            self.sync(&before);
        }
    }

    /// The write's answer, back on the GUI thread. An answer for another list (the source
    /// changed meanwhile) only reports.
    fn write_done(
        mut self: Pin<&mut Self>,
        epoch: u64,
        edit: Option<LocalEdit>,
        result: JobResult<()>,
    ) {
        let before = self.view();
        let same_list = self.rust().epoch == epoch;
        if let Some(edit) = edit.filter(|_| same_list)
            && let Some(list) = self.as_mut().rust_mut().get_mut().list.as_mut()
        {
            match &result {
                Ok(()) => list.settle(edit),
                Err(_) => list.revert(edit),
            }
        }
        self.as_mut().sync(&before);
        if let Err(failure) = result {
            tracing::warn!("a write failed: {}", failure.message());
            self.as_mut().show_banner_for(write_failure(&failure));
        }
        self.wrote();
    }
}

list_model_rows!(qobject::EntriesModel, slots: Slot, ROLES, role_value);
