# Ressac Linux GUI spike: Rust + Qt 6 + QML + Kirigami + QtWebEngine on Fedora aarch64

Date: 2026-10-23. Throwaway spike, not product code. Host: macOS arm64 with OrbStack Docker;
container `ressac-qt-spike` from `fedora:latest` (arm64, 8 CPUs, 8 GB), removed at the end.
The ressac repo was copied in with tar (no `target/`, `apps/macos/build`, `.git`); it was not modified.

## 1. Versions (Fedora 44, aarch64, all from the main repos, no COPR)

| Package | Version |
|---|---|
| Fedora | 44 (Forty Four) |
| qt6-qtbase-devel / qtbase-gui | 6.11.2-3.fc44 |
| qt6-qtdeclarative-devel | 6.11.2-2.fc44 |
| qt6-qtwebengine(-devel) | 6.11.2-2.fc44 (Chromium 140 user agent) |
| qt6-qtwebchannel-devel | 6.11.2-1.fc44 |
| kf6-kirigami (+ -devel) | 6.31.0-1.fc44 |
| kf6-qqc2-desktop-style | 6.31.0-1.fc44 |
| kf6-kcoreaddons-devel, kf6-ki18n-devel, kf6-kiconthemes | 6.31.0-1.fc44 |
| breeze-icon-theme | 6.31.0-1.fc44 |
| rust / cargo (dnf) | 1.99.0-1.fc44, exactly the workspace `rust-version = "1.99"`; rustup not needed |
| gcc / clang / lld | 16.2.1 / 22.1.8 / 22.1.8 |
| cmake | 4.3.0 (installed, not used) |
| cxx-qt / cxx-qt-lib / cxx-qt-build | 0.10.0 (crates.io, 2026-08-24) |
| cxx | 1.0.205 |

QtWebEngine is packaged for aarch64 on Fedora 44.
Available QPA plugins: offscreen, vnc, minimal, minimalegl, eglfs, linuxfb, wayland, xcb, vkkhrdisplay. **No `webgl` plugin** (removed upstream in Qt 6).

dnf line used (about 2 min):
`rust cargo cmake gcc gcc-c++ clang lld make pkgconf-pkg-config qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtwebengine-devel qt6-qtwebchannel-devel kf6-kirigami kf6-kirigami-devel kf6-qqc2-desktop-style kf6-kcoreaddons-devel kf6-ki18n-devel breeze-icon-theme mesa-dri-drivers mesa-libGL mesa-libEGL dejavu-sans-fonts tmux sqlite openssl-devel`
(`lld` was added after the first link failure, see below.)

## 2. `cargo test --workspace` (ressac, unchanged) on Linux

The run took 1 min 37 s, including a cold build of 1 min 13 s. Results:

- **992 passed, 1 failed, 1 ignored.** The ignored test is `extract::spike::real_pages`, which needs the network and is ignored by design.
- Binary counts: ressac bin 449/1 fail, cli 64, ressac-core lib 444 (+1 ignored), article_proxy 1, contract 12, edition 11, end_to_end 4, fetch_image 7.
- The only failure is `tui::reader::jobs::tests::a_job_that_cannot_open_the_database_reports_a_failure`.
  It is **a root-in-container artifact, not a Linux bug**. The test expects `Store::open("/nonexistent/dir/r.sqlite")`
  to fail, but as root `create_dir_all` succeeds. As the non-root user `spike`, after deleting the `/nonexistent`
  directory the root run had created, it **passes**. So the suite is green on Linux when it runs as non-root.
- tmux tests (`ticker::line::*tmux*`, `tui::reader::images::*tmux*`) pass because they are pure string logic.
  `~/Library/Application Support` is only a default path string, so nothing breaks, but a Linux GUI spec must
  define an XDG path (`$XDG_DATA_HOME/ressac/ressac.sqlite`) in `ressac_core::store::default_path()`.
- Nothing WebKit- or macOS-specific is compiled into the Rust workspace. The Swift app lives in `apps/macos`, outside the workspace.

## 3. cxx-qt spike app (`app/`)

The build is **cargo only** (`build.rs` with `CxxQtBuilder::new_qml_module`), with no CMake.

- `src/bridge.rs` contains two Rust QObjects:
  - `Backend`: `#[qproperty] status: QString`, `clicks: i32`, and a `#[qinvokable] nodeClicked(name) -> QString`.
  - `FeedModel`: `#[base = QAbstractListModel]` with `rowCount`/`data`/`roleNames` as `#[cxx_override]` and
    `beginResetModel`/`endResetModel` as `#[inherit]`, plus a `#[qenum]` for the roles. **cxx-qt 0.10 supports
    list models cleanly.** It follows the upstream `todo_app` example and needs no C++.
  - `FeedModel.loadDemo()` **calls ressac-core**, used as a path dependency on the workspace member, which works
    and inherits `workspace.package` fields. It opens `Store::open(tmp/ressac.sqlite)`, runs `opml::parse` and
    `import_outlines` on a 5-feed OPML, then lists `store.feeds()`. The screenshot shows schema v6, 1 folder and 5 feeds.
- `qml/Main.qml` is a `Kirigami.ApplicationWindow` with a GlobalDrawer and a `pageStack`:
  `FeedsPage` (a `Kirigami.ScrollablePage` holding a ListView on the Rust model) and `GraphPage` (pushed at 3 s).
- `GraphPage.qml` has a `WebEngineView` that loads `web/graph.html`, embedded with `.qrc_resources()`. The page
  holds a hand-written SVG force layout in the style of d3. The page talks to Rust and QML in both directions:
  - JS calls the **Rust invokable directly over WebChannel**: `backend.nodeClicked(name, cb)`. The Rust QObject
    is registered on the `WebChannel` with `WebChannel.id: "backend"`.
  - JS calls a QML function: `qmlBridge.hello()` returns its value.
  - JS gets a Rust property's change signal: `backend.statusChanged.connect(...)`.
  - QML calls JS with `web.runJavaScript("simulateClick('Rust Blog'); ...", cb)`, and the result comes back into QML.
  - The log in the screenshot shows the whole chain working.
- `cpp/shim.{h,cpp}` with `src/shim.rs` (a plain `cxx::bridge`) is the only C++. It holds
  `QtWebEngineQuick::initialize()`, which must run before the app is constructed and which cxx-qt-lib does not
  wrap, plus a `QQuickWindow::grabWindow()` timer used for the screenshots, an optional `QApplication`, and the
  icon theme.

### Build times (aarch64 container, 8 cores)

These times include ressac-core and its dependencies, which bring aws-lc-sys, rusqlite bundled, scraper and others.

| Build | Wall time | CPU (user) |
|---|---|---|
| Clean debug | 1 min 16 s | 7 min 14 s |
| Clean release | 1 min 43 s | 10 min 7 s |
| Incremental after editing a `.rs` bridge file | 5.4 s | |
| Incremental after editing a `.qml` file | 5.1 s | |

The debug binary is 173 MB. The release binary is 7.4 MB and links 327 shared libraries. Qt is linked directly
(Core/Gui/Qml/Quick/Widgets/WebEngineQuick/WebChannel), but **Kirigami is not linked at all**: it is a runtime QML
import (`/usr/lib64/qt6/qml/org/kde/kirigami`), so the `kf6-*-devel` packages are not needed to build.
qmlcachegen and qmltyperegistrar run automatically: `plugin.qmltypes` and `qmlcache_loader.cpp` are generated.

### Friction and gotchas met

1. **Link failure with GNU ld.bfd.** The symptom is undefined `QtWebEngineQuick::initialize`,
   `QQuickWindow::grabWindow` and `QQuickWindow::staticMetaObject`. With `--as-needed`, the Qt libraries come
   before the static `-cxxqt-generated.a`. cxx-qt-build warns about this: "Neither mold, lld, nor gold linkers were
   found". **Fix: `dnf install lld`.** cxx-qt-build then picks lld on its own.
2. qproperty renaming syntax: use `#[qproperty(QString, store_info, cxx_name = "storeInfo")]`. The second argument
   is the Rust field name. Inside `impl` blocks, refer to `qobject::QHash_i32_QByteArray` by path.
3. `qrc_resources([...])` in a QML-module build gets the prefix `/qt/qml/<uri as dirs>/`, which makes the URL
   `qrc:/qt/qml/org/ressac/spike/web/graph.html`. Use `Qt.resolvedUrl("../web/graph.html")` from QML.
   `qrc:/web/...` returns ERR_FILE_NOT_FOUND, which Chromium renders as its own error page.
4. **Fedora sends Qt logs to journald and drops QML `console.log`.** To see logs in a container or CI, set
   `QT_FORCE_STDERR_LOGGING=1` and `QT_LOGGING_RULES="qml.debug=true;js.debug=true"`.
5. `WebEngineView.onJavaScriptConsoleMessage` relays page JS errors to QML. Without it, JS errors are silent.
6. **Kirigami looks bare outside Plasma.** There is no Breeze style and no icons, plus a warning:
   `kf.iconthemes: Icon theme "breeze-internal" not found`.
   - Icons need `QIcon::setThemeName("breeze")`.
   - The `org.kde.desktop` style from qqc2-desktop-style needs a **`QApplication`**, because it paints with
     QStyle. cxx-qt-lib only gives `QGuiApplication`; `QApplication` lives in cxx-qt-lib-extras, or 5 lines of
     C++ in the shim, as done here. Link the Widgets module and set `QT_QUICK_CONTROLS_STYLE=org.kde.desktop`
     (or `QQuickStyle::setStyle`).
   - With both in place, `shot-3-qapplication-desktop-style.png` shows Breeze icons in the drawer.
   - A real session would rather have plasma-integration or `KIconTheme::initTheme()` from KF6 do this.
7. gcc 16 prints `-Wsfinae-incomplete` noise for Qt headers included next to `rust/cxx.h`. It is harmless.
8. This is a mistake in the spike's own JS, not a Qt problem: a page pushed onto the pageStack can report a
   meaningless `innerWidth` during load. Use an SVG `viewBox` or a ResizeObserver, never sizes read at load time.

## Runtime findings: QtWebEngine headless in a container

- **It runs offscreen out of the box** with `QT_QPA_PLATFORM=offscreen` and needs no extra Chromium flags.
  - The offscreen platform has no GL, so Qt Quick falls back to the **software** scene graph
    (`qt.scenegraph.general: Loading backend software`).
  - WebEngine reports `Chromium GL Backend: disabled`, `disable-gpu`, `in-process-gpu`, `use-gl disabled`, and
    renders with software compositing.
  - The log noise is `QRhiGles2: Failed to create context` and the dbus `system_bus_socket` error. Both are harmless.
  - You do not need mesa or llvmpipe for this path.
- **As root**, Chromium refuses to run sandboxed. Set `QTWEBENGINE_DISABLE_SANDBOX=1`, or
  `QTWEBENGINE_CHROMIUM_FLAGS=--no-sandbox`.
- **As a normal user (`spike`)**, the default sandbox works in the OrbStack container: the namespace sandbox,
  with `disable-setuid-sandbox` added automatically. This is the recommended mode, with no sandbox flags.
- Screenshots work with `QQuickWindow::grabWindow()`, triggered by a C++ QTimer, and they **include the
  WebEngine content**. `grabToImage` was not needed.

## 4. Visual recette from the Mac

- **Static screenshots (works):** run with `QT_QPA_PLATFORM=offscreen` and a grab hook
  (`SPIKE_SHOTS=/dir` here; `ressac_schedule_grab(ms, path)`) on the bind-mounted folder, then open the PNG on the Mac.
  For a recette script, expose grab and quit as an env var or CLI flag (for example
  `--screenshot page=graph out=x.png`) and drive navigation with QML timers or invokables.
- **Live, interactive (works):** use `QT_QPA_PLATFORM=vnc:size=1100x720:port=5900`.
  - The VNC plugin is in Fedora's Qt, and WebEngine renders under it through the software path.
  - **OrbStack exposes the container's IP and `ressac-qt-spike.orb.local` to the Mac with no `-p`.** The Mac
    received the banner `RFB 003.003` (Qt for Embedded Linux VNC Server, no auth).
  - `tools/vnc_grab.py`, a 50-line RFB client, pulled a full frame from the Mac, saved as `vnc-from-mac.png`.
  - For live clicking, connect a VNC viewer (TigerVNC, RealVNC) to `ressac-qt-spike.orb.local:5900`.
  - **Not checked:** whether macOS Screen Sharing.app accepts a server with RFB 3.3 and security "None".
  - The VNC display uses another DPI than offscreen (fonts larger), so pin `QT_FONT_DPI` for comparable shots.
- **WebGL QPA:** not available, because it was removed in Qt 6.
- **Other options, not tested:** a Wayland compositor in the container (weston/cage headless with
  `--backend=vnc` or RDP), or Xvfb with x11vnc. These would be the way to exercise real GL or the xcb/wayland
  plugins, which is closer to a desktop than offscreen or vnc.

## Recommendation for the spec

- **Build: cargo only, with cxx-qt-build.** It covered the QML module, qmlcachegen, qrc and the extra C++ shim,
  with fast incremental builds and no CMake. Move to CMake (`cxx_qt_import_crate` + ECM) only if packaging needs
  KDE conventions: ki18n `.po` catalogs, `.desktop`/AppStream/icon install, or Flatpak with the KDE runtime.
  Otherwise put those in an `xtask` or a Makefile `install` target.
- Use dnf's rust 1.99 on Fedora 44. Require **`lld`**, or mold, in the build dependencies.
- Keep ressac-core as a path or workspace dependency. Add a `crates/ressac-qt` (or `apps/linux`) member and
  make `store::default_path()` XDG-aware on Linux.
- Keep about 50 lines of C++ for things cxx-qt-lib does not wrap: `QtWebEngineQuick::initialize`,
  `QApplication`, the icon theme and window grabs. Or check cxx-qt-lib-extras for `QApplication`.
- Use WebChannel and register the Rust QObjects directly, so no QML relay is needed. Keep `runJavaScript` for
  QML-to-page pushes.
- Run the test and recette pipeline as a **non-root** user, so the Chromium sandbox stays on and the ressac
  test that needs a failing directory creation passes. Set `QT_FORCE_STDERR_LOGGING=1`.

## Files

- `app/` holds the spike sources: Cargo.toml, build.rs, src/, qml/, cpp/, web/.
- `shot-1-feeds.png` is the Kirigami list page, with feeds from ressac-core (offscreen, non-root, sandbox on).
- `shot-2-graph.png` shows the list and the WebEngine graph page side by side, with the WebChannel round-trip log.
- `shot-3-qapplication-desktop-style.png` shows QApplication with org.kde.desktop and Breeze icons.
- `vnc-from-mac.png` is a frame grabbed from the Mac over VNC.
- `tools/vnc_grab.py` is the minimal RFB 3.3 frame grabber.
- `*.log` are the dnf, cargo test, build and run logs.
