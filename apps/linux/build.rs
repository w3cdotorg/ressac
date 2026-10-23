//! Linux: compiles the QML module, the Rust bridges and the C++ shim with cxx-qt-build (cargo
//! only, no CMake). Elsewhere it does nothing: the crate is a stub (src/main.rs).
fn main() {
    #[cfg(target_os = "linux")]
    qt::build();
}

#[cfg(target_os = "linux")]
mod qt {
    use std::path::PathBuf;

    /// `dir/*.ext`, sorted; a missing directory is empty.
    fn sources(dir: &str, ext: &str) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut found: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == ext))
            .collect();
        found.sort();
        found
    }

    /// A Rust file is a bridge when a line starts (after indentation) with `#[cxx_qt::bridge`
    /// or `#[cxx::bridge`.
    fn is_bridge(text: &str) -> bool {
        text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("#[cxx_qt::bridge") || line.starts_with("#[cxx::bridge")
        })
    }

    pub fn build() {
        for dir in ["src", "qml", "cpp"] {
            println!("cargo::rerun-if-changed={dir}"); // new files are picked up
        }
        let bridges = sources("src", "rs").into_iter().filter(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            is_bridge(&text)
        });
        cxx_qt_build::CxxQtBuilder::new_qml_module(
            cxx_qt_build::QmlModule::new("org.w3cdotorg.ressac")
                .qml_files(sources("qml", "qml"))
                .depend("QtQuick"),
        )
        .files(bridges)
        .include_dir("cpp")
        .cpp_files(sources("cpp", "cpp"))
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("Widgets")
        .qt_module("WebEngineQuick")
        .build();
    }
}
