//! `CellRowsModel`: the rows of an Antiope list (`pages::Line::encode`d strings) as a list
//! model. A new `rows` list is parsed and applied by id with `live::plan_rows`, never a reset:
//! the view keeps its delegates, its scroll position and its current item.

use std::collections::HashMap;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList, QVariant};
use ressac_ui::live::plan_rows;

use crate::rows::{list_model_rows, to_i32, variant_string};

/// Between the fields of an encoded row (`pages::Line::encode`); never in fitted text.
const FIELD_SEPARATOR: char = '\u{1d}';

/// The role names QML sees, in role order (`FIRST_ROLE + index`).
const ROLES: [&str; 5] = ["rowId", "line", "doubleHeight", "selected", "action"];

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
        #[base = QAbstractListModel]
        #[qproperty(QStringList, rows, READ, WRITE = set_rows, NOTIFY)]
        type CellRowsModel = super::CellRowsModelRust;

        fn set_rows(self: Pin<&mut CellRowsModel>, rows: QStringList);

        #[cxx_override]
        fn data(self: &CellRowsModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn row_count(self: &CellRowsModel, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn role_names(self: &CellRowsModel) -> QHash_i32_QByteArray;

        #[inherit]
        fn begin_insert_rows(
            self: Pin<&mut CellRowsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_insert_rows(self: Pin<&mut CellRowsModel>);
        #[inherit]
        fn begin_remove_rows(
            self: Pin<&mut CellRowsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        fn end_remove_rows(self: Pin<&mut CellRowsModel>);
        #[inherit]
        fn index(self: &CellRowsModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        /// The row of `id`, −1 when none.
        #[qinvokable]
        fn row_of_id(self: &CellRowsModel, id: &QString) -> i32;

        /// The first selected row, −1 when none.
        #[qinvokable]
        fn row_of_selected(self: &CellRowsModel) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        fn data_changed(
            self: Pin<&mut CellRowsModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }
}

/// One parsed row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CellRowData {
    /// What the row plan follows: the id, made unique when a list repeats one (the n-th
    /// repetition gets `\u{1d}n`), so a malformed list can never confuse the plan.
    key: String,
    id: String,
    line: String,
    double_height: bool,
    selected: bool,
    action: String,
}

/// `id \u{1d} flags \u{1d} action \u{1d} line`; a missing field is empty, unknown flags are
/// ignored. The keys of repeated ids are told apart in list order.
fn parse_rows<'a>(encoded: impl IntoIterator<Item = &'a str>) -> Vec<CellRowData> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    encoded
        .into_iter()
        .map(|row| {
            let mut fields = row.splitn(4, FIELD_SEPARATOR);
            let mut next = || fields.next().unwrap_or("").to_string();
            let (id, flags, action, line) = (next(), next(), next(), next());
            let repeats = seen.entry(id.clone()).or_insert(0);
            let key = if *repeats == 0 {
                id.clone()
            } else {
                format!("{id}{FIELD_SEPARATOR}{repeats}")
            };
            *repeats += 1;
            CellRowData {
                key,
                id,
                line,
                double_height: flags.contains('d'),
                selected: flags.contains('s'),
                action,
            }
        })
        .collect()
}

#[derive(Default)]
pub struct CellRowsModelRust {
    rows: QStringList,
    lines: Vec<CellRowData>,
}

fn role_value(row: &CellRowData, role: &str) -> QVariant {
    match role {
        "rowId" => variant_string(&row.id),
        "line" => variant_string(&row.line),
        "doubleHeight" => QVariant::from(&row.double_height),
        "selected" => QVariant::from(&row.selected),
        "action" => variant_string(&row.action),
        _ => QVariant::default(),
    }
}

impl qobject::CellRowsModel {
    fn set_rows(mut self: Pin<&mut Self>, rows: QStringList) {
        if self.rust().rows == rows {
            return;
        }
        let texts: Vec<String> = rows.iter().map(ToString::to_string).collect();
        let new = parse_rows(texts.iter().map(String::as_str));
        let ops = plan_rows(&self.rust().lines, &new, |row| row.key.clone());
        for op in ops {
            self.as_mut().apply(op);
        }
        self.as_mut().rust_mut().get_mut().rows = rows;
        self.rows_changed();
    }

    fn row_of_id(&self, id: &QString) -> i32 {
        let id = id.to_string();
        self.rust()
            .lines
            .iter()
            .position(|row| row.id == id)
            .map_or(-1, to_i32)
    }

    fn row_of_selected(&self) -> i32 {
        self.rust()
            .lines
            .iter()
            .position(|row| row.selected)
            .map_or(-1, to_i32)
    }
}

list_model_rows!(qobject::CellRowsModel, lines: CellRowData, ROLES, role_value);

#[cfg(test)]
mod tests {
    use super::*;

    use ressac_ui::pages::{Line, LineRole, encode_rows};
    use ressac_ui::teletext::{CellLine, Color};

    #[test]
    fn an_encoded_line_parses_back_into_its_fields() {
        let mut line = Line::new(
            "page-101",
            LineRole::Jump(101),
            CellLine::centred("UNREAD", 22, Color::Yellow),
        );
        line.selected = true;
        line.double_height = true;
        let plain = Line::new("blank", LineRole::Blank, CellLine::blank(5));
        let encoded = encode_rows(&[line.clone(), plain.clone()]);
        let rows = parse_rows(encoded.iter().map(String::as_str));
        assert_eq!(
            rows,
            vec![
                CellRowData {
                    key: "page-101".into(),
                    id: "page-101".into(),
                    line: line.cells.encode(),
                    double_height: true,
                    selected: true,
                    action: "jump:101".into(),
                },
                CellRowData {
                    key: "blank".into(),
                    id: "blank".into(),
                    line: plain.cells.encode(),
                    double_height: false,
                    selected: false,
                    action: String::new(),
                },
            ]
        );
    }

    #[test]
    fn missing_fields_are_empty_and_repeated_ids_get_their_own_keys() {
        let rows = parse_rows(["", "a\u{1d}s", "a", "a"]);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["", "a", "a\u{1d}1", "a\u{1d}2"]);
        assert!(
            rows.iter()
                .all(|r| r.line.is_empty() && r.action.is_empty())
        );
        assert_eq!(
            rows.iter().map(|r| r.selected).collect::<Vec<_>>(),
            [false, true, false, false]
        );
        assert!(rows.iter().filter(|r| r.id == "a").count() == 3);
    }
}
