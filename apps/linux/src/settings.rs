//! The settings of the app in `QSettings` (`$XDG_CONFIG_HOME/ressac/ressac-kde.conf`), under the
//! keys of the Mac app, and the `AppSettings` QML singleton over them. The decisions (defaults,
//! clamping, snapping, spelling) are `ressac_ui::settings`'s.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use ressac_ui::settings::{Settings, SettingsStore};

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
    }
}

/// The one settings value (no copy in the QObject) and the file it is written to.
pub struct AppSettingsRust {
    store: QSettingsStore,
    settings: Settings,
}

impl Default for AppSettingsRust {
    fn default() -> Self {
        let store = QSettingsStore::default_file();
        let settings = Settings::load(&store);
        AppSettingsRust { store, settings }
    }
}

impl qobject::AppSettings {
    /// Runs one `Settings::set_*` against the file; true when the value changed.
    fn write(
        self: Pin<&mut Self>,
        set: impl FnOnce(&mut Settings, &mut QSettingsStore) -> bool,
    ) -> bool {
        let AppSettingsRust { store, settings } = self.rust_mut().get_mut();
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
