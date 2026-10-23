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
        type Recette = super::RecetteRust;

        /// Saves the first visible window to `<dir>/<name>.png`; false when inactive or failed.
        #[qinvokable]
        fn grab(self: &Recette, name: &QString) -> bool;
    }
}

/// Active when `--screenshot` was given.
pub struct RecetteRust {
    active: bool,
}

impl Default for RecetteRust {
    fn default() -> Self {
        RecetteRust {
            active: DIR.get().is_some(),
        }
    }
}

impl qobject::Recette {
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
