//! The hidden `--screenshot <dir>` run (spec §8.2): the directory, and the `Recette` QML
//! singleton the walk in `Main.qml` grabs windows through.

use std::path::PathBuf;
use std::sync::OnceLock;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use ressac_ui::teletext::{self, CellLine, CellSpan, Color};

use crate::shim;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Records the screenshot directory, before the application starts. Only the first call counts.
pub(crate) fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// Whether this is the recette run (`--screenshot`): what `Recette.active` reads.
pub(crate) fn active() -> bool {
    DIR.get().is_some()
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
        #[qproperty(QString, walk, READ, CONSTANT)]
        #[qproperty(QString, grid_variant, READ, CONSTANT)]
        #[qproperty(f64, grid_size, READ, CONSTANT)]
        type Recette = super::RecetteRust;

        /// Saves the first visible window to `<dir>/<name>.png`; false when inactive or failed.
        #[qinvokable]
        fn grab(self: &Recette, name: &QString) -> bool;

        /// Test hook of the walk: sends a key (a `Qt::Key`, its text, `Qt::KeyboardModifier` bits)
        /// to the window as the keyboard would, through its key filter; the result of the KeyPress's sendEvent (handled or accepted, not necessarily by the filter).
        #[qinvokable]
        fn key(self: &Recette, key: i32, text: &QString, modifiers: i32) -> bool;

        /// Test hook of the walk: `key`, aimed at the visible window titled `title` (the Sea Chart's
        /// `Chart.windowTitle`); false when no visible window has that title.
        #[qinvokable]
        fn key_to(
            self: &Recette,
            title: &QString,
            key: i32,
            text: &QString,
            modifiers: i32,
        ) -> bool;

        /// Test hook of the walk: with `press_default` set to `drop-db`, forgets `RESSAC_DB` so the
        /// relaunch after "Use Default Database" reaches the default location.
        #[qinvokable]
        fn prepare_default(self: &Recette);

        /// Test hook of the grid measurement: starts the frame-time log of the window; no-op
        /// when inactive.
        #[qinvokable]
        fn frame_log_start(self: &Recette);

        /// Stops the frame-time log: `n=… mean=… p50=… p95=… max=… over16=…` in ms; empty when
        /// inactive.
        #[qinvokable]
        fn frame_log_stop(self: &Recette) -> QString;

        /// The synthetic teletext entry `index` of the grid measurement, encoded for `CellRow`
        /// (two rows); empty when inactive.
        #[qinvokable]
        fn grid_line(self: &Recette, index: i32) -> QString;
    }
}

/// Active when `--screenshot` was given.
pub struct RecetteRust {
    active: bool,
    /// `RESSAC_RECETTE_PRESS_DEFAULT` (`keep-db` or `drop-db`): the walk presses "Use Default
    /// Database" once after the first screen. Empty: it does not.
    press_default: QString,
    /// `RESSAC_RECETTE_WALK` (`journal`): the walk `Main.qml` runs, with that walk's own
    /// automatic edition triggers. Empty: the usual walk, with every automatic trigger off.
    walk: QString,
    /// `RESSAC_RECETTE_GRID` (`painted` or `text`): which grid the measurement builds. Empty: none.
    grid_variant: QString,
    /// `RESSAC_RECETTE_GRID_SIZE`: the pixel size of the measured rows (20 when unset).
    grid_size: f64,
}

const PRESS_DEFAULT_ENV: &str = "RESSAC_RECETTE_PRESS_DEFAULT";
const WALK_ENV: &str = "RESSAC_RECETTE_WALK";
const GRID_VARIANT_ENV: &str = "RESSAC_RECETTE_GRID";
const GRID_SIZE_ENV: &str = "RESSAC_RECETTE_GRID_SIZE";
const DEFAULT_GRID_SIZE: f64 = 20.0;

const TITLE_WORDS: [&str; 12] = [
    "Ressac",
    "teletext",
    "Antiope",
    "Minitel",
    "dimanche",
    "edition",
    "jardin",
    "PAGE",
    "nouvelles",
    "BBC",
    "Ceefax",
    "Oracle",
];

/// Cells of a title row.
const TITLE_CELLS: i64 = 24;

/// The synthetic entry `index` of the grid measurement: two rows of a 2-cell marker, a 24-cell
/// title and a 2-cell flag. Entry 7 is reversed.
fn grid_entry(index: i32) -> String {
    let index = i64::from(index.max(0));
    let mut title = String::new();
    let mut word = 0;
    while title.chars().count() < 2 * TITLE_CELLS as usize {
        // `% 12` keeps the index inside `TITLE_WORDS`.
        let pick = ((index * 7 + word * 3) % 12) as usize;
        title.push_str(TITLE_WORDS[pick]);
        title.push(' ');
        word += 1;
    }
    let cut = title
        .char_indices()
        .nth(TITLE_CELLS as usize)
        .map_or(title.len(), |(at, _)| at);
    let (first, second) = title.split_at(cut);
    let marker = if index % 3 == 0 {
        CellSpan::new("\u{25cf} ", 2, Color::Yellow)
    } else {
        CellSpan::new("  ", 2, Color::White)
    };
    let flag = if index % 5 == 0 {
        CellSpan::new(" *", 2, Color::White)
    } else {
        CellSpan::new("", 2, Color::White)
    };
    let mut lines = vec![
        CellLine::new(vec![
            marker,
            CellSpan::new(first, TITLE_CELLS, Color::White),
            flag,
        ]),
        CellLine::new(vec![
            CellSpan::new("", 2, Color::White),
            CellSpan::new(second, TITLE_CELLS, Color::White),
            CellSpan::new("", 2, Color::White),
        ]),
    ];
    if index == 7 {
        lines = lines
            .iter()
            .map(|line| line.reversed(Color::Black, Color::White))
            .collect();
    }
    teletext::encode_lines(&lines)
}

/// The variable's value, only when `--screenshot` was given.
fn recette_env(name: &str) -> QString {
    QString::from(
        std::env::var(name)
            .ok()
            .filter(|_| DIR.get().is_some())
            .unwrap_or_default()
            .as_str(),
    )
}

impl Default for RecetteRust {
    fn default() -> Self {
        RecetteRust {
            active: active(),
            press_default: recette_env(PRESS_DEFAULT_ENV),
            walk: recette_env(WALK_ENV),
            grid_variant: recette_env(GRID_VARIANT_ENV),
            grid_size: recette_env(GRID_SIZE_ENV)
                .to_string()
                .parse()
                .ok()
                .filter(|size: &f64| size.is_finite() && *size >= 1.0)
                .unwrap_or(DEFAULT_GRID_SIZE),
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

    fn frame_log_start(&self) {
        if self.rust().active {
            shim::ressac_frame_log_start();
        }
    }

    fn frame_log_stop(&self) -> QString {
        if self.rust().active {
            QString::from(shim::ressac_frame_log_stop().as_str())
        } else {
            QString::default()
        }
    }

    fn grid_line(&self, index: i32) -> QString {
        if self.rust().active {
            QString::from(grid_entry(index).as_str())
        } else {
            QString::default()
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

    fn key_to(&self, title: &QString, key: i32, text: &QString, modifiers: i32) -> bool {
        self.rust().active
            && shim::ressac_send_key_to(
                &title.to_string(),
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
