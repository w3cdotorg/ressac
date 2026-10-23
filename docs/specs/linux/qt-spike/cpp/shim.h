#pragma once
#include <rust/cxx.h>

// Must run before QGuiApplication is constructed.
void ressac_init_webengine();
// Grab the first QQuickWindow after `ms` and save it to `path`.
void ressac_schedule_grab(int ms, rust::Str path);
// Quit the application after `ms`.
void ressac_schedule_quit(int ms);
// Creates a QApplication (Widgets) instead of a QGuiApplication: required by the
// org.kde.desktop (qqc2-desktop-style) Qt Quick Controls style, which paints with QStyle.
void ressac_create_qapplication();
// Sets the Breeze icon theme (outside a Plasma session nothing sets it).
void ressac_setup_icons();
// Runs the event loop of whatever application object exists.
int ressac_exec();
