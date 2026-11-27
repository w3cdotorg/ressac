//! The Qt layer of ressac-kde: the C++ shim declarations and `run()`. Compiled only on Linux; the
//! library is empty elsewhere so `cargo test --workspace` needs no Qt.
#![cfg(target_os = "linux")]

mod antiope;
mod cellrow;
mod cellrows;
mod chart;
mod entries;
mod garden;
mod garden_inspector;
mod journal;
mod launcher;
mod library;
mod mosaic;
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
use ressac_ui::keys;
use ressac_ui::launch::parse_args;
use ressac_ui::settings::Appearance;

// Named by the `shim` bridge's `extern "Rust"` block (cxx looks them up in this scope).
use mosaic::{ressac_mosaic_cancel, ressac_mosaic_start};
use reader::ressac_scheme_serve;

/// What cxx-qt-lib does not wrap (`cpp/shim.h`).
#[cxx::bridge]
mod shim {
    unsafe extern "C++" {
        include!("shim.h");

        pub fn ressac_init_webengine();
        pub fn ressac_create_application();
        pub fn ressac_destroy_application();
        pub fn ressac_has_window() -> bool;
        pub fn ressac_exec() -> i32;
        pub fn ressac_grab_window(path: &str) -> bool;
        pub fn ressac_install_key_filter();
        pub fn ressac_clipboard_text() -> String;
        pub fn ressac_send_key(key: i32, modifiers: u32, text: &str) -> bool;
        pub fn ressac_send_key_to(title: &str, key: i32, modifiers: u32, text: &str) -> bool;

        /// The family Qt gave the registered `bedstead.otf` (empty when registration failed).
        pub fn ressac_bedstead_family() -> String;
        /// Starts recording the frame intervals of the first visible window.
        pub fn ressac_frame_log_start();
        /// Stops recording; the one-line summary (`n=0` when no frame came).
        pub fn ressac_frame_log_stop() -> String;
        /// With `RESSAC_FRAME_LOG=1`: records, and logs a summary every ten seconds.
        pub fn ressac_frame_log_auto();

        pub fn ressac_settings_default_file() -> String;
        pub fn ressac_settings_get(file: &str, key: &str, value: &mut String) -> bool;
        pub fn ressac_settings_set(file: &str, key: &str, value: &str) -> bool;
        pub fn ressac_settings_remove(file: &str, key: &str) -> bool;

        /// Answers the `ressac-mosaic:` request `token` with `body` (GUI thread only); a
        /// request the page cancelled is dropped without an answer.
        pub fn ressac_mosaic_reply(token: u64, mime: &str, body: &[u8]);
    }

    extern "Rust" {
        /// The `ressac:` scheme handler's lookup (`reader.rs`).
        fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool;

        /// The `ressac-mosaic:` handler's request `token` for `url` (`mosaic.rs`): answered
        /// later through `ressac_mosaic_reply`, at once when cached or unparsable.
        fn ressac_mosaic_start(token: u64, url: &str);

        /// The page cancelled the `ressac-mosaic:` request `token` before its answer: it
        /// leaves the mosaic queue and its download stops (`mosaic.rs`).
        fn ressac_mosaic_cancel(token: u64);

        /// The window key filter's question (`ressac_ui::keys::route`): fills `verb` (empty
        /// when the key maps to no command) and returns whether the key's `ShortcutOverride`
        /// is claimed.
        fn ressac_route_key(
            key: i32,
            text: &str,
            modifiers: u32,
            repeat: bool,
            chain: &str,
            look: &str,
            verb: &mut String,
        ) -> bool;
    }
}

/// The window key filter's question (`cpp/shim.cpp`), answered by `ressac_ui::keys`: the tokens
/// of the focused chain arrive joined with `|`, the verb goes back empty when the key maps to
/// no command, and the result says whether the `ShortcutOverride` is claimed.
fn ressac_route_key(
    key: i32,
    text: &str,
    modifiers: u32,
    repeat: bool,
    chain: &str,
    look: &str,
    verb: &mut String,
) -> bool {
    let chain: Vec<&str> = chain.split('|').filter(|t| !t.is_empty()).collect();
    let route = keys::route(
        key,
        text,
        modifiers,
        repeat,
        &chain,
        Appearance::parse(look).unwrap_or_default(),
    );
    *verb = route.verb.unwrap_or("").into();
    route.claims_override
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
    let code = if shim::ressac_has_window() {
        shim::ressac_install_key_filter();
        shim::ressac_frame_log_auto();
        u8::try_from(shim::ressac_exec()).unwrap_or(1)
    } else {
        eprintln!("ressac-kde: the main window did not load (see the QML errors above)");
        1
    };
    // Teardown in order, before `main` returns: the QML engine (and its singletons, so a
    // late answer cannot queue onto them), the mosaic thread and the session's worker threads
    // joined within a bounded wait, then the application and Chromium.
    drop(engine);
    mosaic::shutdown();
    launcher::shutdown_session();
    shim::ressac_destroy_application();
    ExitCode::from(code)
}
