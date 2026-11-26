//! What the list models of the window share: the role numbering, and the model methods that
//! turn a `ressac_ui::live::RowOp` plan into the Qt calls the views depend on (begin/end
//! ordering, `dataChanged` for kept rows). A model declares `data`, `row_count`, `role_names`,
//! `begin_*`/`end_*_rows`, `index` and `data_changed` in its bridge and calls
//! [`list_model_rows!`] for their bodies.

use cxx_qt_lib::{QString, QVariant};

/// `Qt::UserRole`: the first custom role.
pub(crate) const FIRST_ROLE: i32 = 256;

pub(crate) fn variant_string(text: &str) -> QVariant {
    QVariant::from(&QString::from(text))
}

pub(crate) fn to_i32(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// Generates `data`, `row_count`, `role_names` and `apply` for `$model` (the bridge's type),
/// over its Rust field `$rows: Vec<$row>`. `$roles` lists the role names in role order
/// (`FIRST_ROLE + index`); `$value(&row, name)` gives a role's `QVariant`.
macro_rules! list_model_rows {
    ($model:ty, $rows:ident: $row:ty, $roles:expr, $value:path) => {
        impl $model {
            fn data(&self, index: &cxx_qt_lib::QModelIndex, role: i32) -> cxx_qt_lib::QVariant {
                let row = usize::try_from(index.row()).ok();
                let name = usize::try_from(role - $crate::rows::FIRST_ROLE)
                    .ok()
                    .and_then(|role| $roles.get(role));
                match (row.and_then(|row| self.rust().$rows.get(row)), name) {
                    (Some(row), Some(name)) => $value(row, name),
                    _ => cxx_qt_lib::QVariant::default(),
                }
            }

            fn row_count(&self, parent: &cxx_qt_lib::QModelIndex) -> i32 {
                if parent.is_valid() {
                    return 0;
                }
                $crate::rows::to_i32(self.rust().$rows.len())
            }

            fn role_names(&self) -> cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray> {
                let mut names =
                    cxx_qt_lib::QHash::<cxx_qt_lib::QHashPair_i32_QByteArray>::default();
                for (offset, name) in ($crate::rows::FIRST_ROLE..).zip($roles) {
                    names.insert(offset, cxx_qt_lib::QByteArray::from(name));
                }
                names
            }

            /// One operation of a row plan, as the Qt calls the views follow.
            fn apply(mut self: std::pin::Pin<&mut Self>, op: ressac_ui::live::RowOp<$row>) {
                use ressac_ui::live::RowOp;
                let root = cxx_qt_lib::QModelIndex::default();
                let to_i32 = $crate::rows::to_i32;
                match op {
                    RowOp::Remove { first, last } => {
                        self.as_mut()
                            .begin_remove_rows(&root, to_i32(first), to_i32(last));
                        self.as_mut().rust_mut().get_mut().$rows.drain(first..=last);
                        self.as_mut().end_remove_rows();
                    }
                    RowOp::Insert { at, rows } => {
                        let last = at + rows.len() - 1;
                        self.as_mut()
                            .begin_insert_rows(&root, to_i32(at), to_i32(last));
                        self.as_mut()
                            .rust_mut()
                            .get_mut()
                            .$rows
                            .splice(at..at, rows);
                        self.as_mut().end_insert_rows();
                    }
                    RowOp::Change { first, rows } => {
                        let last = first + rows.len() - 1;
                        self.as_mut()
                            .rust_mut()
                            .get_mut()
                            .$rows
                            .splice(first..=last, rows);
                        let top = self.index(to_i32(first), 0, &root);
                        let bottom = self.index(to_i32(last), 0, &root);
                        // No roles listed: every role may have changed.
                        self.data_changed(&top, &bottom, &cxx_qt_lib::QList::default());
                    }
                }
            }
        }
    };
}

pub(crate) use list_model_rows;
