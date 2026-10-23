//! The Qt layer of ressac-kde: the C++ shim declarations and `run()`. Compiled only on Linux; the
//! library is empty elsewhere so `cargo test --workspace` needs no Qt.
#![cfg(target_os = "linux")]

use std::process::ExitCode;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};

/// What cxx-qt-lib does not wrap (`cpp/shim.h`).
#[cxx::bridge]
mod shim {
    unsafe extern "C++" {
        include!("shim.h");

        fn ressac_init_webengine();
        fn ressac_create_application();
        fn ressac_has_window() -> bool;
        fn ressac_exec() -> i32;
    }
}

const MAIN_QML: &str = "qrc:/qt/qml/org/w3cdotorg/ressac/qml/Main.qml";

/// Starts the application and runs its event loop; the process exit code.
pub fn run() -> ExitCode {
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
