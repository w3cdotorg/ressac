//! The `SourcesModel` QML singleton: the sidebar's rows as a list model. The rules (sections,
//! folder tree, badges, selection keys, row diffs) are `ressac_ui`'s; this object applies the
//! diff by id with `beginInsertRows` / `beginRemoveRows` / `dataChanged`, never a reset, so the
//! view keeps its place and the selection survives every reload.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QVariant};
use ressac_core::clock::today_start;
use ressac_core::store::Source;
use ressac_ui::chrono;
use ressac_ui::live::plan_rows;
use ressac_ui::runs::{DirectWrite, read_failure, write_failure};
use ressac_ui::sources::{RowKind, Selection, SidebarRow, SidebarTree, index_of};
use ressac_ui::worker::JobResult;

use crate::launcher::with_session;
use crate::rows::{list_model_rows, to_i32, variant_string};

/// The role names QML sees, in role order (`FIRST_ROLE + index`).
const ROLES: [&str; 14] = [
    "kind",
    "key",
    "title",
    "icon",
    "badge",
    "depth",
    "expandable",
    "expanded",
    "statusColor",
    "statusText",
    "feedId",
    "preferFullText",
    "site",
    "selectable",
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
        #[qproperty(QString, selected_key, READ = selected_key, NOTIFY = selection_changed)]
        #[qproperty(i32, selected_row, READ = selected_row, NOTIFY = selection_changed)]
        #[qproperty(bool, is_garden, READ = is_garden, NOTIFY = selection_changed)]
        type SourcesModel = super::SourcesModelRust;

        fn selected_key(self: &SourcesModel) -> QString;
        fn selected_row(self: &SourcesModel) -> i32;
        fn is_garden(self: &SourcesModel) -> bool;

        #[cxx_override]
        fn data(self: &SourcesModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn row_count(self: &SourcesModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn role_names(self: &SourcesModel) -> QHash_i32_QByteArray;

        #[inherit]
        fn begin_insert_rows(
            self: Pin<&mut SourcesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_insert_rows(self: Pin<&mut SourcesModel>);
        #[inherit]
        fn begin_remove_rows(
            self: Pin<&mut SourcesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_remove_rows(self: Pin<&mut SourcesModel>);
        #[inherit]
        fn index(self: &SourcesModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        /// Reads the sidebar again and applies the difference.
        #[qinvokable]
        fn reload(self: Pin<&mut SourcesModel>);

        #[qinvokable]
        fn select(self: Pin<&mut SourcesModel>, row: i32);

        #[qinvokable]
        fn select_key(self: Pin<&mut SourcesModel>, key: &QString);

        #[qinvokable]
        fn row_of_key(self: &SourcesModel, key: &QString) -> i32;

        #[qinvokable]
        fn toggle_expanded(self: Pin<&mut SourcesModel>, row: i32);

        #[qinvokable]
        fn set_prefer_full_text(self: Pin<&mut SourcesModel>, feed_id: i64, on: bool);

        #[qsignal]
        fn selection_changed(self: Pin<&mut SourcesModel>);

        /// A failed read or write: for the window's banner.
        #[qsignal]
        fn banner(self: Pin<&mut SourcesModel>, message: &QString, show_log: bool);

        /// An own write ended (landed or reverted): the window reloads.
        #[qsignal]
        fn wrote(self: Pin<&mut SourcesModel>);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        fn data_changed(
            self: Pin<&mut SourcesModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    impl cxx_qt::Threading for SourcesModel {}
}

#[derive(Default)]
pub struct SourcesModelRust {
    tree: SidebarTree,
    rows: Vec<SidebarRow>,
    selection: Selection,
}

fn variant_count(count: usize) -> QVariant {
    QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX))
}

/// The value of `role` (an index into [`ROLES`]) for a row.
fn role_value(row: &SidebarRow, role: &str) -> QVariant {
    match role {
        "kind" => variant_string(row.kind.as_str()),
        "key" => variant_string(&row.key),
        "title" => variant_string(&row.title),
        "icon" => variant_string(row.icon),
        "badge" => variant_count(row.badge),
        "depth" => variant_count(row.depth),
        "expandable" => QVariant::from(&row.expandable),
        "expanded" => QVariant::from(&row.expanded),
        "statusColor" => variant_string(row.dot().unwrap_or("")),
        "statusText" => variant_string(
            row.dot()
                .and(row.status)
                .map_or("", |status| status.as_str()),
        ),
        "feedId" => QVariant::from(&feed_id(row).unwrap_or(0)),
        "preferFullText" => QVariant::from(&row.prefer_full_text),
        "site" => variant_string(row.site.as_deref().unwrap_or("")),
        "selectable" => QVariant::from(&row.selection().is_some()),
        _ => QVariant::default(),
    }
}

/// The feed a feed row stands for.
fn feed_id(row: &SidebarRow) -> Option<i64> {
    match (row.kind, row.selection()) {
        (RowKind::Feed, Some(Selection::Source(Source::Feed(id)))) => Some(id),
        _ => None,
    }
}

impl qobject::SourcesModel {
    fn selected_key(&self) -> QString {
        QString::from(self.rust().selection.key().as_str())
    }

    fn selected_row(&self) -> i32 {
        let rust = self.rust();
        index_of(&rust.rows, &rust.selection)
            .and_then(|row| i32::try_from(row).ok())
            .unwrap_or(-1)
    }

    fn is_garden(&self) -> bool {
        self.rust().selection.is_garden()
    }

    /// A row index from QML; `None` when out of range (a stale index).
    fn row_at(&self, row: i32) -> Option<&SidebarRow> {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().rows.get(row))
    }

    /// Applies the tree's current rows to the model, by key.
    fn replan(mut self: Pin<&mut Self>) {
        let before = (self.selected_row(), self.selected_key());
        let new = self.rust().tree.rows();
        let ops = plan_rows(&self.rust().rows, &new, |row| row.key.clone());
        for op in ops {
            self.as_mut().apply(op);
        }
        if (self.selected_row(), self.selected_key()) != before {
            self.selection_changed();
        }
    }

    fn reload(mut self: Pin<&mut Self>) {
        let today = today_start(chrono::Local::now());
        let loaded = with_session(|session| {
            self.as_mut()
                .rust_mut()
                .get_mut()
                .tree
                .load(&session.store, today)
        });
        match loaded {
            Some(Ok(())) => self.replan(),
            Some(Err(error)) => {
                tracing::warn!("cannot read the sidebar: {error}");
                let banner = read_failure(&error);
                self.banner(&QString::from(banner.message.as_str()), banner.show_log);
            }
            None => {}
        }
    }

    /// Stores `selection` and tells QML when it differs from the current one.
    fn choose(mut self: Pin<&mut Self>, selection: Selection) {
        if self.rust().selection == selection {
            return;
        }
        self.as_mut().rust_mut().get_mut().selection = selection;
        self.selection_changed();
    }

    fn select(self: Pin<&mut Self>, row: i32) {
        let Some(selection) = self.row_at(row).and_then(SidebarRow::selection) else {
            return;
        };
        self.choose(selection);
    }

    fn select_key(mut self: Pin<&mut Self>, key: &QString) {
        let Some(selection) = Selection::parse_key(&key.to_string()) else {
            return;
        };
        self.as_mut().rust_mut().get_mut().tree.reveal(&selection);
        self.as_mut().replan();
        self.choose(selection);
    }

    fn row_of_key(&self, key: &QString) -> i32 {
        let key = key.to_string();
        self.rust()
            .rows
            .iter()
            .position(|row| row.key == key)
            .map_or(-1, to_i32)
    }

    fn toggle_expanded(mut self: Pin<&mut Self>, row: i32) {
        let folder = self
            .row_at(row)
            .filter(|row| row.kind == RowKind::Folder)
            .and_then(SidebarRow::selection);
        let Some(Selection::Source(Source::Folder(folder))) = folder else {
            return;
        };
        self.as_mut().rust_mut().get_mut().tree.toggle(folder);
        self.replan();
    }

    /// The optimistic switch, then the write on the worker's direct lane.
    fn set_prefer_full_text(mut self: Pin<&mut Self>, feed_id: i64, on: bool) {
        let previous = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .tree
            .set_prefer_full_text_local(feed_id, on);
        self.as_mut().replan();
        let qt = self.qt_thread();
        let write = DirectWrite::PreferFullText { feed: feed_id, on };
        let queued = with_session(|session| {
            session.worker.direct(
                move |cx| write.apply(cx.app.store(), chrono::Utc::now()),
                move |result| {
                    let queued =
                        qt.queue(move |model| model.preference_written(feed_id, previous, result));
                    if queued.is_err() {
                        tracing::debug!("the app quit before a write finished");
                    }
                },
            );
        });
        if queued.is_none() {
            // No session (cannot happen once the window is up): take the change back.
            self.as_mut()
                .rust_mut()
                .get_mut()
                .tree
                .set_prefer_full_text_local(feed_id, previous);
            self.replan();
        }
    }

    /// The write's result, back on the GUI thread.
    fn preference_written(
        mut self: Pin<&mut Self>,
        feed_id: i64,
        previous: bool,
        result: JobResult<()>,
    ) {
        if let Err(failure) = result {
            tracing::warn!(
                "cannot save the full-text preference: {}",
                failure.message()
            );
            self.as_mut()
                .rust_mut()
                .get_mut()
                .tree
                .set_prefer_full_text_local(feed_id, previous);
            self.as_mut().replan();
            let banner = write_failure(&failure);
            self.as_mut()
                .banner(&QString::from(banner.message.as_str()), banner.show_log);
        }
        self.wrote();
    }
}

list_model_rows!(qobject::SourcesModel, rows: SidebarRow, ROLES, role_value);
