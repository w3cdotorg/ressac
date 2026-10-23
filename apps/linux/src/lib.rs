//! The Qt layer of ressac-kde: the C++ shim declarations and `run()`. Compiled only on Linux; the
//! library is empty elsewhere so `cargo test --workspace` needs no Qt.
#![cfg(target_os = "linux")]

mod entries;
mod launcher;
mod reader;
mod recette;
mod rows;
mod runs;
mod settings;
mod sources;

use std::process::ExitCode;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};
use ressac_core::logging;
use ressac_core::paths::Paths;
use ressac_ui::launch::parse_args;

// Named by the `shim` bridge's `extern "Rust"` block (cxx looks it up in this scope).
use reader::ressac_scheme_serve;

/// What cxx-qt-lib does not wrap (`cpp/shim.h`).
#[cxx::bridge]
mod shim {
    unsafe extern "C++" {
        include!("shim.h");

        pub fn ressac_init_webengine();
        pub fn ressac_create_application();
        pub fn ressac_has_window() -> bool;
        pub fn ressac_exec() -> i32;
        pub fn ressac_grab_window(path: &str) -> bool;

        pub fn ressac_settings_default_file() -> String;
        pub fn ressac_settings_get(file: &str, key: &str, value: &mut String) -> bool;
        pub fn ressac_settings_set(file: &str, key: &str, value: &str) -> bool;
        pub fn ressac_settings_remove(file: &str, key: &str) -> bool;
    }

    extern "Rust" {
        /// The `ressac:` scheme handler's lookup (`reader.rs`).
        fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool;
    }
}

const MAIN_QML: &str = "qrc:/qt/qml/org/w3cdotorg/ressac/qml/Main.qml";

/// Starts the application and runs its event loop; the process exit code.
pub fn run() -> ExitCode {
    let options = match parse_args(
        std::env::args_os().skip(1),
        std::env::var_os("RESSAC_DB").as_deref(),
    ) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if let Some(dir) = options.screenshot {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            eprintln!("ressac-kde: cannot create {}: {error}", dir.display());
            return ExitCode::from(2);
        }
        recette::set_dir(dir);
    }
    // Kept for the whole run: dropping the guard flushes the log file.
    let _log_guard = Paths::from_env()
        .ok()
        .and_then(|paths| logging::init(&paths.log_dir(), false));

    shim::ressac_init_webengine();
    shim::ressac_create_application();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(MAIN_QML));
    }
    if !shim::ressac_has_window() {
        eprintln!("ressac-kde: the main window did not load (see the QML errors above)");
        return ExitCode::from(1);
    }
    ExitCode::from(u8::try_from(shim::ressac_exec()).unwrap_or(1))
}
