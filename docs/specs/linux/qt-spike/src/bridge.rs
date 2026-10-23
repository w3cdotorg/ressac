//! Rust-side QObjects exposed to QML: a backend with a property + invokables, and a
//! QAbstractListModel of feeds read through ressac-core.
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

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
    }

    #[qenum(FeedModel)]
    enum FeedRoles {
        Title,
        Url,
        State,
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(i32, clicks)]
        type Backend = super::BackendRust;

        /// Called from QML (and, through QML, from the WebEngine page).
        #[qinvokable]
        #[cxx_name = "nodeClicked"]
        fn node_clicked(self: Pin<&mut Backend>, name: &QString) -> QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, count)]
        #[qproperty(QString, store_info, cxx_name = "storeInfo")]
        type FeedModel = super::FeedModelRust;

        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FeedModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &FeedModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FeedModel) -> QHash_i32_QByteArray;

        /// Opens a ressac-core Store on a temp DB, imports a small OPML, lists feeds.
        #[qinvokable]
        #[cxx_name = "loadDemo"]
        fn load_demo(self: Pin<&mut FeedModel>);

        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut FeedModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut FeedModel>);
    }
}

#[derive(Default)]
pub struct BackendRust {
    status: QString,
    clicks: i32,
}

impl qobject::Backend {
    fn node_clicked(mut self: Pin<&mut Self>, name: &QString) -> QString {
        let n = *self.clicks() + 1;
        self.as_mut().set_clicks(n);
        let msg = QString::from(&format!("Rust saw click #{n} on « {name} »"));
        self.as_mut().set_status(msg.clone());
        eprintln!("[spike] {msg}");
        msg
    }
}

#[derive(Default)]
pub struct FeedModelRust {
    count: i32,
    store_info: QString,
    rows: Vec<(String, String, String)>,
}

const OPML: &str = r#"<?xml version="1.0"?>
<opml version="2.0"><head><title>spike</title></head><body>
  <outline text="Tech">
    <outline type="rss" text="LWN.net" xmlUrl="https://lwn.net/headlines/rss"/>
    <outline type="rss" text="KDE Planet" xmlUrl="https://planet.kde.org/global/atom.xml"/>
    <outline type="rss" text="Rust Blog" xmlUrl="https://blog.rust-lang.org/feed.xml"/>
  </outline>
  <outline type="rss" text="Le Monde" xmlUrl="https://www.lemonde.fr/rss/une.xml"/>
  <outline type="rss" text="Fedora Magazine" xmlUrl="https://fedoramagazine.org/feed/"/>
</body></opml>"#;

fn demo_rows() -> Result<(String, Vec<(String, String, String)>), ressac_core::Error> {
    use ressac_core::store::Store;
    let dir = std::env::temp_dir().join("ressac-qt-spike");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("ressac.sqlite");
    let store = Store::open(&path)?;
    let outlines = ressac_core::opml::parse(OPML)?;
    let report = store.import_outlines(&outlines, false, chrono::Utc::now())?;
    let rows = store
        .feeds()?
        .into_iter()
        .map(|f| (f.title, f.xml_url, f.state))
        .collect();
    let info = format!(
        "Store {} · schema v{} · import: {:?}",
        path.display(),
        store.schema_version()?,
        report
    );
    Ok((info, rows))
}

impl qobject::FeedModel {
    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let role = qobject::FeedRoles { repr: role };
        let Some((title, url, state)) = self.rows.get(index.row() as usize) else {
            return QVariant::default();
        };
        let s = match role {
            qobject::FeedRoles::Title => title,
            qobject::FeedRoles::Url => url,
            qobject::FeedRoles::State => state,
            _ => return QVariant::default(),
        };
        QVariant::from(&QString::from(s.as_str()))
    }

    fn role_names(&self) -> qobject::QHash_i32_QByteArray {
        let mut h = QHash::<QHashPair_i32_QByteArray>::default();
        h.insert(qobject::FeedRoles::Title.repr, QByteArray::from("title"));
        h.insert(qobject::FeedRoles::Url.repr, QByteArray::from("url"));
        h.insert(qobject::FeedRoles::State.repr, QByteArray::from("feedState"));
        h
    }

    fn load_demo(mut self: Pin<&mut Self>) {
        let (info, rows) = match demo_rows() {
            Ok(v) => v,
            Err(e) => (format!("ressac-core error: {e}"), Vec::new()),
        };
        eprintln!("[spike] {info}");
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        let n = self.rows.len() as i32;
        self.as_mut().set_count(n);
        self.as_mut().set_store_info(QString::from(info.as_str()));
    }
}
