#pragma once
#include <rust/cxx.h>

// What cxx-qt-lib does not wrap. Every function is declared to Rust in the `shim` bridge of
// src/lib.rs.

// Must run before the application object is constructed (WebEngine needs it there).
void ressac_init_webengine();
// Names, a QApplication (the org.kde.desktop style paints with QStyle), desktop file name,
// window icon, Quick Controls style, icon theme. Argument-free: Qt gets no command line.
void ressac_create_application();
// Deletes the application object (and with it the WebEngine profile); call after the QML engine
// is gone and the worker threads are joined.
void ressac_destroy_application();
// True when a top-level QQuickWindow exists (Main.qml produced its window).
bool ressac_has_window();
// Runs the event loop of the application object.
int ressac_exec();
// Saves the first visible QQuickWindow (WebEngine content included) to `path`; false when there
// is none or the save fails.
bool ressac_grab_window(rust::Str path);

// QSettings on an explicit INI file, opened per call. `set` and `remove` sync and report
// whether the file was written.
rust::String ressac_settings_default_file();
bool ressac_settings_get(rust::Str file, rust::Str key, rust::String &value);
bool ressac_settings_set(rust::Str file, rust::Str key, rust::Str value);
bool ressac_settings_remove(rust::Str file, rust::Str key);
