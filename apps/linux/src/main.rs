//! ressac-kde: Ressac for KDE Plasma. It builds everywhere so `cargo test --workspace` needs no
//! Qt; off Linux it is a stub.

use std::process::ExitCode;

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("ressac-kde runs on Linux only.");
    ExitCode::from(1)
}

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/w3cdotorg/ressac/qml/Main.qml"));
    }
    let code = app.as_mut().map_or(1, |app| app.exec());
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
