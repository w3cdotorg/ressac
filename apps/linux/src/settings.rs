//! The settings of the app in `QSettings` (`$XDG_CONFIG_HOME/ressac/ressac-kde.conf`), under the
//! keys of the Mac app, and the `AppSettings` QML singleton over them. The decisions (defaults,
//! clamping, snapping, spelling) are `ressac_ui::settings`'s.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use ressac_core::paths::Paths;
use ressac_core::store::Store;
use ressac_ui::chart::save_prefs;
use ressac_ui::launch::{DatabaseChoice, choose_database, database_location, database_note};
use ressac_ui::library::local_path;
use ressac_ui::settings::{
    Appearance, CHART_WINDOW_MIN, ChartInk, ChartPrefs, ChartView, LineWidth, ReaderFont, Settings,
    SettingsStore, TextSizeCommand,
};

use crate::shim;

/// A `SettingsStore` over one INI file, opened per call so a write is on disk when it returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QSettingsStore {
    file: PathBuf,
}

impl QSettingsStore {
    /// `QSettings("ressac", "ressac-kde")`'s file.
    pub fn default_file() -> QSettingsStore {
        QSettingsStore::at(Path::new(&shim::ressac_settings_default_file()))
    }

    pub fn at(file: &Path) -> QSettingsStore {
        QSettingsStore {
            file: file.to_path_buf(),
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    fn file_name(&self) -> String {
        self.file().to_string_lossy().into_owned()
    }
}

impl SettingsStore for QSettingsStore {
    fn get(&self, key: &str) -> Option<String> {
        let mut value = String::new();
        shim::ressac_settings_get(&self.file_name(), key, &mut value).then_some(value)
    }

    fn set(&mut self, key: &str, value: &str) {
        if !shim::ressac_settings_set(&self.file_name(), key, value) {
            tracing::warn!("cannot write {key} in {}", self.file().display());
        }
    }

    fn remove(&mut self, key: &str) {
        if !shim::ressac_settings_remove(&self.file_name(), key) {
            tracing::warn!("cannot write {key} in {}", self.file().display());
        }
    }
}

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
        #[qproperty(QString, appearance, READ = appearance, WRITE = set_appearance, NOTIFY)]
        #[qproperty(bool, crt, READ = crt, WRITE = set_crt, NOTIFY)]
        #[qproperty(QString, reader_font, READ = reader_font, WRITE = set_reader_font, NOTIFY)]
        #[qproperty(f64, reader_size, READ = reader_size, WRITE = set_reader_size, NOTIFY)]
        #[qproperty(f64, reader_size_antiope, READ = reader_size_antiope, WRITE = set_reader_size_antiope, NOTIFY)]
        #[qproperty(QString, reader_width, READ = reader_width, WRITE = set_reader_width, NOTIFY)]
        #[qproperty(QString, database_path, READ = database_path, WRITE = set_database_path, NOTIFY)]
        #[qproperty(QString, chart_view, READ = chart_view, WRITE = set_chart_view, NOTIFY)]
        #[qproperty(QString, chart_ink, READ = chart_ink, WRITE = set_chart_ink, NOTIFY)]
        #[qproperty(bool, chart_animate, READ = chart_animate, WRITE = set_chart_animate, NOTIFY)]
        #[qproperty(i32, chart_window_width, READ = chart_window_width, WRITE = set_chart_window_width, NOTIFY)]
        #[qproperty(i32, chart_window_height, READ = chart_window_height, WRITE = set_chart_window_height, NOTIFY)]
        #[qproperty(QStringList, font_ids, READ, CONSTANT)]
        #[qproperty(QStringList, font_titles, READ, CONSTANT)]
        #[qproperty(QStringList, font_families, READ, CONSTANT)]
        #[qproperty(QStringList, width_ids, READ, CONSTANT)]
        #[qproperty(QStringList, width_titles, READ, CONSTANT)]
        #[qproperty(QString, resolved_database_path, READ = resolved_database_path, NOTIFY = data_changed)]
        #[qproperty(QString, database_note, READ = database_note, NOTIFY = data_changed)]
        #[qproperty(bool, ressac_db_set, READ = ressac_db_set, CONSTANT)]
        #[qproperty(QString, database_refusal, READ = database_refusal, NOTIFY = data_changed)]
        #[qproperty(bool, needs_relaunch, READ = needs_relaunch, NOTIFY = data_changed)]
        type AppSettings = super::AppSettingsRust;

        fn appearance(self: &AppSettings) -> QString;
        fn set_appearance(self: Pin<&mut AppSettings>, value: &QString);
        fn crt(self: &AppSettings) -> bool;
        fn set_crt(self: Pin<&mut AppSettings>, value: bool);
        fn reader_font(self: &AppSettings) -> QString;
        fn set_reader_font(self: Pin<&mut AppSettings>, value: &QString);
        fn reader_size(self: &AppSettings) -> f64;
        fn set_reader_size(self: Pin<&mut AppSettings>, value: f64);
        fn reader_size_antiope(self: &AppSettings) -> f64;
        fn set_reader_size_antiope(self: Pin<&mut AppSettings>, value: f64);
        fn reader_width(self: &AppSettings) -> QString;
        fn set_reader_width(self: Pin<&mut AppSettings>, value: &QString);
        fn database_path(self: &AppSettings) -> QString;
        fn set_database_path(self: Pin<&mut AppSettings>, value: &QString);
        fn chart_view(self: &AppSettings) -> QString;
        fn set_chart_view(self: Pin<&mut AppSettings>, value: &QString);
        fn chart_ink(self: &AppSettings) -> QString;
        fn set_chart_ink(self: Pin<&mut AppSettings>, value: &QString);
        fn chart_animate(self: &AppSettings) -> bool;
        fn set_chart_animate(self: Pin<&mut AppSettings>, value: bool);
        fn chart_window_width(self: &AppSettings) -> i32;
        fn set_chart_window_width(self: Pin<&mut AppSettings>, value: i32);
        fn chart_window_height(self: &AppSettings) -> i32;
        fn set_chart_window_height(self: Pin<&mut AppSettings>, value: i32);

        fn resolved_database_path(self: &AppSettings) -> QString;
        fn database_note(self: &AppSettings) -> QString;
        fn ressac_db_set(self: &AppSettings) -> bool;
        fn database_refusal(self: &AppSettings) -> QString;
        fn needs_relaunch(self: &AppSettings) -> bool;

        /// Settings ▸ Data: a database chosen in a file dialog (a `file:` URL). A refusal
        /// shows in `databaseRefusal`; a change is stored and `needsRelaunch` turns on.
        #[qinvokable]
        fn choose_database(self: Pin<&mut AppSettings>, url: &QString);

        /// Back to the default location.
        #[qinvokable]
        fn use_default_location(self: Pin<&mut AppSettings>);

        /// Starts a new `ressac-kde` process; true when it started (QML then quits).
        #[qinvokable]
        fn relaunch(self: &AppSettings) -> bool;

        /// The Data page's texts changed (path, note, refusal, relaunch rule).
        #[qsignal]
        fn data_changed(self: Pin<&mut AppSettings>);

        /// What the Sea Chart page chose (`Chart.prefsReported`): view and ink always, animate
        /// not under Reduce Motion. An unknown view or ink word stores nothing.
        #[qinvokable]
        fn save_chart_prefs(
            self: Pin<&mut AppSettings>,
            view: &QString,
            ink: &QString,
            animate: bool,
            reduce: bool,
        );

        /// View ▸ Bigger / Smaller / Actual Size on `look` (`native` or `antiope`): `command` is
        /// `bigger`, `smaller` or `actualSize`. Returns the size now shown; an unknown command
        /// writes nothing.
        #[qinvokable]
        fn resize(self: Pin<&mut AppSettings>, look: &QString, command: &QString) -> f64;
    }
}

/// The one settings value (no copy in the QObject) and the file it is written to.
pub struct AppSettingsRust {
    store: QSettingsStore,
    settings: Settings,
    /// `RESSAC_DB` set and not empty at start.
    ressac_db_set: bool,
    database_refusal: &'static str,
    needs_relaunch: bool,
    font_ids: QStringList,
    font_titles: QStringList,
    font_families: QStringList,
    width_ids: QStringList,
    width_titles: QStringList,
}

impl Default for AppSettingsRust {
    fn default() -> Self {
        let store = QSettingsStore::default_file();
        let settings = Settings::load(&store);
        AppSettingsRust {
            store,
            settings,
            ressac_db_set: std::env::var_os("RESSAC_DB").is_some_and(|v| !v.is_empty()),
            database_refusal: "",
            needs_relaunch: false,
            font_ids: string_list(ReaderFont::ALL.map(ReaderFont::as_str)),
            font_titles: string_list(ReaderFont::ALL.map(ReaderFont::title)),
            font_families: string_list(ReaderFont::ALL.map(ReaderFont::family)),
            width_ids: string_list(LineWidth::ALL.map(LineWidth::as_str)),
            width_titles: string_list(LineWidth::ALL.map(LineWidth::title)),
        }
    }
}

impl qobject::AppSettings {
    /// Runs one `Settings::set_*` against the file; true when the value changed.
    fn write(
        self: Pin<&mut Self>,
        set: impl FnOnce(&mut Settings, &mut QSettingsStore) -> bool,
    ) -> bool {
        let AppSettingsRust {
            store, settings, ..
        } = self.rust_mut().get_mut();
        set(settings, store)
    }

    fn appearance(&self) -> QString {
        QString::from(self.rust().settings.appearance.as_str())
    }

    fn set_appearance(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_appearance(store, &value))
        {
            self.as_mut().appearance_changed();
        }
    }

    fn crt(&self) -> bool {
        self.rust().settings.crt
    }

    fn set_crt(mut self: Pin<&mut Self>, value: bool) {
        if self.as_mut().write(|s, store| s.set_crt(store, value)) {
            self.as_mut().crt_changed();
        }
    }

    fn reader_font(&self) -> QString {
        QString::from(self.rust().settings.reader_font.as_str())
    }

    fn set_reader_font(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_reader_font(store, &value))
        {
            self.as_mut().reader_font_changed();
        }
    }

    fn reader_size(&self) -> f64 {
        self.rust().settings.reader_size
    }

    fn set_reader_size(mut self: Pin<&mut Self>, value: f64) {
        if self
            .as_mut()
            .write(|s, store| s.set_reader_size(store, value))
        {
            self.as_mut().reader_size_changed();
        }
    }

    fn reader_size_antiope(&self) -> f64 {
        self.rust().settings.reader_size_antiope
    }

    fn set_reader_size_antiope(mut self: Pin<&mut Self>, value: f64) {
        if self
            .as_mut()
            .write(|s, store| s.set_reader_size_antiope(store, value))
        {
            self.as_mut().reader_size_antiope_changed();
        }
    }

    fn reader_width(&self) -> QString {
        QString::from(self.rust().settings.reader_width.as_str())
    }

    fn set_reader_width(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_reader_width(store, &value))
        {
            self.as_mut().reader_width_changed();
        }
    }

    /// Empty means the default location.
    fn database_path(&self) -> QString {
        QString::from(
            self.rust()
                .settings
                .database_path
                .as_deref()
                .unwrap_or_default(),
        )
    }

    fn set_database_path(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_database_path(store, &value))
        {
            self.as_mut().database_path_changed();
        }
    }

    /// Where the database is (or will be created), as the launch resolves it; `""` when no
    /// location can be worked out.
    fn resolved_database_path(&self) -> QString {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute());
        let default_db = Paths::from_env().ok().map(|paths| paths.database());
        let path = database_location(
            None,
            self.rust().settings.database_path.as_deref(),
            home.as_deref(),
            default_db.as_deref(),
        );
        QString::from(
            path.map(|path| path.display().to_string())
                .unwrap_or_default()
                .as_str(),
        )
    }

    fn database_note(&self) -> QString {
        QString::from(database_note(self.rust().settings.database_path.as_deref()))
    }

    fn ressac_db_set(&self) -> bool {
        self.rust().ressac_db_set
    }

    fn database_refusal(&self) -> QString {
        QString::from(self.rust().database_refusal)
    }

    fn needs_relaunch(&self) -> bool {
        self.rust().needs_relaunch
    }

    /// Applies a chosen path (blank: the default location) and tells QML what changed.
    fn apply_database_choice(mut self: Pin<&mut Self>, path: &str) {
        let AppSettingsRust {
            store,
            settings,
            database_refusal,
            needs_relaunch,
            ..
        } = self.as_mut().rust_mut().get_mut();
        match choose_database(settings, store, path, Store::probe) {
            DatabaseChoice::Refused(sentence) => *database_refusal = sentence,
            DatabaseChoice::Stored => {
                *database_refusal = "";
                *needs_relaunch = true;
                self.as_mut().database_path_changed();
            }
            DatabaseChoice::Unchanged => *database_refusal = "",
        }
        self.data_changed();
    }

    fn choose_database(self: Pin<&mut Self>, url: &QString) {
        // Anything but a file URL (a stale or empty dialog result) is ignored.
        let Some(path) = local_path(&url.to_string()) else {
            return;
        };
        self.apply_database_choice(&path.to_string_lossy());
    }

    fn use_default_location(self: Pin<&mut Self>) {
        self.apply_database_choice("");
    }

    fn relaunch(&self) -> bool {
        let started =
            std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn());
        match started {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!("cannot start a new Ressac: {error}");
                false
            }
        }
    }

    fn resize(mut self: Pin<&mut Self>, look: &QString, command: &QString) -> f64 {
        let look = Appearance::parse(&look.to_string()).unwrap_or_default();
        let command = match command.to_string().as_str() {
            "bigger" => TextSizeCommand::Bigger,
            "smaller" => TextSizeCommand::Smaller,
            "actualSize" => TextSizeCommand::ActualSize,
            _ => return self.rust().settings.shown_size(look),
        };
        let mut size = 0.0;
        let changed = self.as_mut().write(|settings, store| {
            let (new_size, changed) = settings.resize(store, look, command);
            size = new_size;
            changed
        });
        if changed {
            match look {
                Appearance::Native => self.reader_size_changed(),
                Appearance::Antiope => self.reader_size_antiope_changed(),
            }
        }
        size
    }

    fn chart_view(&self) -> QString {
        QString::from(self.rust().settings.chart.view.as_str())
    }

    fn set_chart_view(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_chart_view(store, &value))
        {
            self.as_mut().chart_view_changed();
        }
    }

    fn chart_ink(&self) -> QString {
        QString::from(self.rust().settings.chart.ink.as_str())
    }

    fn set_chart_ink(mut self: Pin<&mut Self>, value: &QString) {
        let value = value.to_string();
        if self
            .as_mut()
            .write(|s, store| s.set_chart_ink(store, &value))
        {
            self.as_mut().chart_ink_changed();
        }
    }

    fn chart_animate(&self) -> bool {
        self.rust().settings.chart.animate
    }

    fn set_chart_animate(mut self: Pin<&mut Self>, value: bool) {
        if self
            .as_mut()
            .write(|s, store| s.set_chart_animate(store, value))
        {
            self.as_mut().chart_animate_changed();
        }
    }

    fn chart_window_width(&self) -> i32 {
        chart_extent(self.rust().settings.chart_window.0, CHART_WINDOW_MIN.0)
    }

    fn set_chart_window_width(mut self: Pin<&mut Self>, value: i32) {
        let width = chart_extent(u32::try_from(value).unwrap_or(0), CHART_WINDOW_MIN.0);
        let height = self.chart_window_height();
        if self.as_mut().write_chart_window(width, height) {
            self.as_mut().chart_window_width_changed();
        }
    }

    fn chart_window_height(&self) -> i32 {
        chart_extent(self.rust().settings.chart_window.1, CHART_WINDOW_MIN.1)
    }

    fn set_chart_window_height(mut self: Pin<&mut Self>, value: i32) {
        let width = self.chart_window_width();
        let height = chart_extent(u32::try_from(value).unwrap_or(0), CHART_WINDOW_MIN.1);
        if self.as_mut().write_chart_window(width, height) {
            self.as_mut().chart_window_height_changed();
        }
    }

    /// Stores both dimensions (each setter keeps the other); true when the size changed.
    fn write_chart_window(self: Pin<&mut Self>, width: i32, height: i32) -> bool {
        let (width, height) = (
            u32::try_from(width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
        );
        self.write(|s, store| s.set_chart_window(store, width, height))
    }

    fn save_chart_prefs(
        mut self: Pin<&mut Self>,
        view: &QString,
        ink: &QString,
        animate: bool,
        reduce: bool,
    ) {
        let (Some(view), Some(ink)) = (
            ChartView::parse(&view.to_string()),
            ChartInk::parse(&ink.to_string()),
        ) else {
            return;
        };
        let AppSettingsRust {
            store, settings, ..
        } = self.as_mut().rust_mut().get_mut();
        let before = settings.chart;
        save_prefs(settings, store, &ChartPrefs { view, ink, animate }, reduce);
        let after = settings.chart;
        if before.view != after.view {
            self.as_mut().chart_view_changed();
        }
        if before.ink != after.ink {
            self.as_mut().chart_ink_changed();
        }
        if before.animate != after.animate {
            self.chart_animate_changed();
        }
    }
}

/// The Sea Chart window's size as the window may use it: never below the minimum, never above
/// what a corrupt settings file could ask a window to be.
fn chart_extent(value: u32, minimum: u32) -> i32 {
    i32::try_from(value.clamp(minimum, CHART_WINDOW_MAX)).unwrap_or(i32::MAX)
}

const CHART_WINDOW_MAX: u32 = 16384;

fn string_list<const N: usize>(items: [&str; N]) -> QStringList {
    let mut list = QStringList::default();
    for item in items {
        list.append(QString::from(item));
    }
    list
}

#[cfg(test)]
mod tests {
    use ressac_ui::settings::keys;

    use super::*;

    #[test]
    fn a_value_round_trips_through_the_ini_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = QSettingsStore::at(&dir.path().join("ressac-kde.conf"));
        assert_eq!(store.get(keys::READER_SIZE), None);
        store.set(keys::READER_SIZE, "21");
        store.set(keys::DATABASE_PATH, "~/Flux é/r.sqlite");
        assert_eq!(store.get(keys::READER_SIZE).as_deref(), Some("21"));
        assert_eq!(
            store.get(keys::DATABASE_PATH).as_deref(),
            Some("~/Flux é/r.sqlite")
        );
        store.remove(keys::READER_SIZE);
        assert_eq!(store.get(keys::READER_SIZE), None);
        let text = std::fs::read_to_string(store.file()).unwrap();
        assert!(text.starts_with("[General]"), "{text}");
    }

    #[test]
    fn load_through_qsettings_matches_the_memory_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut q = QSettingsStore::at(&dir.path().join("ressac-kde.conf"));
        let mut m: std::collections::BTreeMap<String, String> = Default::default();
        for (key, value) in [
            (keys::READER_FONT, "comic-sans"),
            (keys::READER_SIZE, "99"),
            (keys::READER_SIZE_ANTIOPE, "17"),
            (keys::CHART_INK, "pop"),
            (keys::CRT, "true"),
        ] {
            q.set(key, value);
            m.set(key, value);
        }
        assert_eq!(Settings::load(&q), Settings::load(&m));
        let mut s = Settings::load(&q);
        assert!(s.set_reader_size_antiope(&mut q, 23.0));
        assert_eq!(
            Settings::load(&QSettingsStore::at(q.file())).reader_size_antiope,
            25.0
        );
    }

    #[test]
    fn a_stored_chart_window_size_is_clamped_to_sane_bounds() {
        assert_eq!(chart_extent(1100, CHART_WINDOW_MIN.0), 1100);
        assert_eq!(chart_extent(10, CHART_WINDOW_MIN.0), 640);
        assert_eq!(chart_extent(0, CHART_WINDOW_MIN.1), 360);
        assert_eq!(chart_extent(u32::MAX, CHART_WINDOW_MIN.0), 16384);
        assert_eq!(chart_extent(16384, CHART_WINDOW_MIN.1), 16384);
    }

    #[test]
    fn the_default_file_is_ressac_kde_conf_in_the_config_directory() {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".config"));
        assert_eq!(
            QSettingsStore::default_file().file(),
            config.join("ressac/ressac-kde.conf")
        );
    }
}
