mod bridge;
mod shim;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    shim::ffi::ressac_init_webengine();
    // SPIKE_QAPP=1: QApplication (Widgets) so the org.kde.desktop style can load.
    let use_qapp = std::env::var_os("SPIKE_QAPP").is_some();
    let mut app = if use_qapp {
        shim::ffi::ressac_create_qapplication();
        None
    } else {
        Some(QGuiApplication::new())
    };
    shim::ffi::ressac_setup_icons();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/ressac/spike/qml/Main.qml"));
    }
    // Offscreen proof: grab the list page, then the graph page (QML pushes it at 3 s).
    if let Ok(dir) = std::env::var("SPIKE_SHOTS") {
        shim::ffi::ressac_schedule_grab(2500, &format!("{dir}/shot-1-feeds.png"));
        shim::ffi::ressac_schedule_grab(12000, &format!("{dir}/shot-2-graph.png"));
        shim::ffi::ressac_schedule_quit(13000);
    }
    match app.as_mut().and_then(|a| a.as_mut()) {
        Some(app) => {
            app.exec();
        }
        None => {
            shim::ffi::ressac_exec();
        }
    }
}
