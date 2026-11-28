#pragma once
#include <rust/cxx.h>

#include <cstdint>

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
// Saves the focused window when it is a visible QQuickWindow, else the first visible one (WebEngine
// content included), to `path`; false when there is none or the save fails.
bool ressac_grab_window(rust::Str path);

// The text on the clipboard ("" when it holds none).
rust::String ressac_clipboard_text();

// Installs the reading window's key filter on the first QQuickWindow: it sees ShortcutOverride and
// KeyPress before the focused item (WebEngine included), asks Rust (`ressac_route_key`) and calls
// the QML `readingPage.perform(verb)`. Once per process; call after the window exists.
void ressac_install_key_filter();
// Test hook of the walk: sends ShortcutOverride, KeyPress and KeyRelease to the first visible
// QQuickWindow (`modifiers` are Qt::KeyboardModifier bits); the result of the KeyPress's sendEvent (the event was handled by the filter or delivered and accepted), not proof that the filter consumed it.
bool ressac_send_key(std::int32_t key, std::uint32_t modifiers, rust::Str text);

// The same, to the visible QQuickWindow whose title() equals `title` (the Sea Chart's, not the first
// visible window's); false when there is none.
bool ressac_send_key_to(rust::Str title, std::int32_t key, std::uint32_t modifiers, rust::Str text);

// QSettings on an explicit INI file, opened per call. `set` and `remove` sync and report
// whether the file was written.
rust::String ressac_settings_default_file();
bool ressac_settings_get(rust::Str file, rust::Str key, rust::String &value);
bool ressac_settings_set(rust::Str file, rust::Str key, rust::Str value);
bool ressac_settings_remove(rust::Str file, rust::Str key);

// The family Qt gave the bundled bedstead.otf ("" when registration failed).
rust::String ressac_bedstead_family();

// Frame-time log of the first visible QQuickWindow: `start` clears and records the interval
// between frameSwapped signals; `stop` ends it and returns "n=%d mean=%.2f p50=%.2f p95=%.2f
// max=%.2f over16=%d" in ms ("n=0" when no interval came). `auto` (RESSAC_FRAME_LOG=1) records
// and logs a summary every 10 s.
void ressac_frame_log_start();
rust::String ressac_frame_log_stop();
void ressac_frame_log_auto();

// Answers the ressac-mosaic: request `token` (the MosaicHandler's, handed to Rust's
// `ressac_mosaic_start`) with `body` of type `mime`, `Cache-Control: no-store`. GUI thread only.
// A request the page cancelled (its job is gone) is dropped without an answer and logged at
// debug level as "mosaic: dropped <token>" (category ressac.mosaic).
void ressac_mosaic_reply(std::uint64_t token, rust::Str mime, rust::Slice<const std::uint8_t> body);
