use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("org.ressac.spike")
            .qml_files(["qml/Main.qml", "qml/FeedsPage.qml", "qml/GraphPage.qml"])
            .depend("QtQuick"),
    )
    .files(["src/bridge.rs", "src/shim.rs"])
    .include_dir("cpp")
    .cpp_files(["cpp/shim.cpp", "cpp/shim.h"])
    .qrc_resources(["web/graph.html"])
    .qt_module("Quick")
    .qt_module("WebEngineQuick")
    .qt_module("WebChannel")
    .qt_module("Widgets")
    .build();
}
