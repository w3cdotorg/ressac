//! The hidden `--screenshot <dir>` run (spec §8.2): the directory, and the `Recette` QML
//! singleton the walk in `Main.qml` grabs windows through.

use std::path::PathBuf;
use std::sync::OnceLock;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::shim;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Records the screenshot directory, before the application starts. Only the first call counts.
pub(crate) fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
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
        #[qproperty(bool, active, READ, CONSTANT)]
        #[qproperty(QString, press_default, READ, CONSTANT)]
        type Recette = super::RecetteRust;

        /// Saves the first visible window to `<dir>/<name>.png`; false when inactive or failed.
        #[qinvokable]
        fn grab(self: &Recette, name: &QString) -> bool;

        /// Test hook of the walk: sends a key (a `Qt::Key`, its text, `Qt::KeyboardModifier` bits)
        /// to the window as the keyboard would, through its key filter; true when it was consumed.
        #[qinvokable]
        fn key(self: &Recette, key: i32, text: &QString, modifiers: i32) -> bool;

        /// Test hook of the walk: with `press_default` set to `drop-db`, forgets `RESSAC_DB` so the
        /// relaunch after "Use Default Database" reaches the default location.
        #[qinvokable]
        fn prepare_default(self: &Recette);
    }
}

/// Active when `--screenshot` was given.
pub struct RecetteRust {
    active: bool,
    /// `RESSAC_RECETTE_PRESS_DEFAULT` (`keep-db` or `drop-db`): the walk presses "Use Default
    /// Database" once after the first screen. Empty: it does not.
    press_default: QString,
}

const PRESS_DEFAULT_ENV: &str = "RESSAC_RECETTE_PRESS_DEFAULT";

impl Default for RecetteRust {
    fn default() -> Self {
        RecetteRust {
            active: DIR.get().is_some(),
            press_default: QString::from(
                std::env::var(PRESS_DEFAULT_ENV)
                    .ok()
                    .filter(|_| DIR.get().is_some())
                    .unwrap_or_default()
                    .as_str(),
            ),
        }
    }
}

impl qobject::Recette {
    fn prepare_default(&self) {
        if self.rust().active && self.rust().press_default.to_string() == "drop-db" {
            // SAFETY: a test hook, run from QML on the GUI thread; no other thread reads the
            // environment at that point (the migration worker only touches the database).
            unsafe { std::env::remove_var("RESSAC_DB") };
        }
    }

    fn key(&self, key: i32, text: &QString, modifiers: i32) -> bool {
        self.rust().active
            && shim::ressac_send_key(
                key,
                u32::try_from(modifiers).unwrap_or(0),
                &text.to_string(),
            )
    }

    fn grab(&self, name: &QString) -> bool {
        let Some(dir) = DIR.get().filter(|_| self.rust().active) else {
            return false;
        };
        let path = dir.join(format!("{name}.png"));
        let saved = shim::ressac_grab_window(&path.to_string_lossy());
        if saved {
            eprintln!("ressac-kde: screenshot {}", path.display());
        } else {
            eprintln!("ressac-kde: screenshot {} failed", path.display());
        }
        saved
    }
}
