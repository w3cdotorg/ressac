# Phase B1: the `ressac-ui` crate and the `ressac-kde` skeleton (B1a, B1b, B1c, B1d)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a Qt-free `crates/ressac-ui` (settings model, launch decisions, tested on the Mac) and an `apps/linux` crate `ressac-kde` that builds everywhere (a stub off Linux) and, on Linux, opens a Kirigami window through the launch states of the Mac app, reads and writes its settings in `QSettings`, walks `--screenshot` offscreen, and installs on marghe with `install.sh`.

**Runs after:** B0c (`Store::probe`, `SchemaState`, `logging::init` exist). Master plan `docs/plans/2026-10-23-linux.md` (rows B1a, B1b; "Interfaces fixed by this plan": `ressac-ui`, `ressac-kde`; Global Constraints: Toolchain, Language, Settings keys, Fedora, Gates). Spec §7.1, §7.2 (launch states), §7.7 (settings), §7.11, §8.1, §8.2, §8.4. Inventory §1.1 (launch phases and sentences), §7.3 (keys, values, Data tab rules), §8.6 (schema check). Spike: `docs/specs/linux/qt-spike-notes.md` and `docs/specs/linux/qt-spike/` (cargo-only build, lld, QML module, shim).

**Architecture:** `ressac-ui` holds every decision (key names, defaults, clamping and snapping, database location, launch step after a probe, command line), with Rust tests that run on the Mac. `ressac-kde` is a library (`src/lib.rs`, the whole Qt layer, compiled only on Linux) plus a binary (`src/main.rs`: `ressac_kde::run()` on Linux, the stub elsewhere), built by `build.rs` with `cxx-qt-build` (QML module `org.w3cdotorg.ressac`), with a small C++ shim (`cpp/shim.{h,cpp}`) for what cxx-qt-lib does not wrap. Three Rust QObjects are QML singletons: `Launcher`, `AppSettings`, `Recette`.

**Tech Stack:** Rust 2024 / 1.99; cxx 1.0.205, cxx-qt / cxx-qt-lib / cxx-qt-build 0.10.0 (Linux only); Qt 6.11.2 (Core, Gui, Qml, Quick, QuickControls2, Widgets, WebEngineQuick), Kirigami 6.31 and qqc2-desktop-style (runtime QML imports), lld; Fedora 44 aarch64 container `ressac-fedora`.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Four sub-phases, not two.** B1 creates 15 files (4 in `crates/ressac-ui`, 11 in `apps/linux`) and edits the root `Cargo.toml` twice. With the 5-file cap: **B1a** `ressac-ui` (5), **B1b** the crate, the stub and a first window (5), **B1c** the C++ shim, the lib/bin split and the settings bridge (5), **B1d** launch flow, `--screenshot`, `install.sh` (5). The ledger gets B1c and B1d rows. Every name of the master plan is kept.
2. **`ressac-ui` modules arrive with the phase that fills them** (no empty files): B1 adds `settings` (master plan) and **`launch`** (new: database location, launch steps, the binary's command line). `sources`, `entries`, … are added by B2 onwards.
3. **`ressac-kde` is a lib + bin crate** from B1c on. Validated in a throwaway Fedora 44 container: cxx-qt-build's link lines apply to every target of the package, so any `tests/*.rs` of a bin-only crate fails to link (`ld.lld: error: undefined symbol: cxx_qt_init_crate_cxx_qt`); with the bridges in a library the integration tests link and run. B1b (no tests directory yet) stays bin-only.
4. **Every dependency of `ressac-kde` is Linux-only**, `ressac-core`, `ressac-ui` and `tracing` included: the stub uses only `std`. Validated on the Mac: `cargo test --workspace` and `cargo clippy --workspace --all-targets` build the stub and compile no `cxx*` crate (`Cargo.lock` still lists them; commit it).
5. **The `.desktop` file is written by `install.sh`** (absolute `Exec` path, optional software rendering for the VM checkpoint), not checked in; the icon is `apps/macos/Ressac/Resources/AppIcon.svg` reused in place (spec §3.8).
6. **`QSettings` is opened per call on an explicit file** (`QSettings(file, QSettings::IniFormat)`), the file being `QSettings("ressac", "ressac-kde").fileName()` = `$XDG_CONFIG_HOME/ressac/ressac-kde.conf` (validated). Unit tests then use a temp file and never touch the environment.
7. **`--screenshot` refuses to run without `RESSAC_DB`**, so a recette never opens the real database.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `Cargo.toml` (root) | B1a T1, B1b T3 | members `crates/ressac-ui`, `apps/linux`; `default-members` without `apps/linux`; pins `cxx`, `cxx-qt`, `cxx-qt-lib`, `cxx-qt-build` |
| `crates/ressac-ui/Cargo.toml` (new) | B1a T1 | `ressac-core` path dependency; dev `tempfile` |
| `crates/ressac-ui/src/lib.rs` (new) | B1a T1, T2 | `pub mod launch; pub mod settings;` and the crate rule (no Qt, no I/O but through `ressac-core`) |
| `crates/ressac-ui/src/settings.rs` (new) | B1a T1 | keys, values, defaults, clamping, snapping, text-size commands, `SettingsStore`, `Settings` |
| `crates/ressac-ui/src/launch.rs` (new) | B1a T2 | `LaunchPhase`, `LaunchStep`, `after_probe`, `after_migration`, `database_location`, `location_problem`, `parse_args` |
| `apps/linux/Cargo.toml` (new) | B1b T3 | package `ressac-kde`, every dependency under `cfg(target_os = "linux")` |
| `apps/linux/build.rs` (new) | B1b T3 | Linux only: QML module from `qml/*.qml`, bridges found in `src/*.rs`, `cpp/*.cpp`, Qt modules |
| `apps/linux/src/main.rs` (new) | B1b T3, B1c T4 | the stub off Linux; `ressac_kde::run()` on Linux |
| `apps/linux/qml/Main.qml` (new) | B1b T3, B1d T6 | the window, the launch states, the screenshot walk |
| `apps/linux/src/lib.rs` (new) | B1c T4, T5, B1d T6 | `run()`, the `#[cxx::bridge] mod shim` declarations, module list |
| `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp` (new) | B1c T4 | WebEngine init, `QApplication`, names, style, icons, window check, exec, window grab, `QSettings` access |
| `apps/linux/src/settings.rs` (new) | B1c T5 | `QSettingsStore` (a `SettingsStore`), the `AppSettings` singleton |
| `apps/linux/src/launcher.rs` (new) | B1d T6 | the `Launcher` singleton, the migration worker, the GUI thread's `Session` |
| `apps/linux/src/recette.rs` (new) | B1d T6 | `--screenshot` directory, the `Recette` singleton (`grab`) |
| `apps/linux/install.sh` (new) | B1d T7 | package check, release build, install / `--uninstall` |
| `apps/linux/tests/stub.rs`, `apps/linux/tests/launch.rs` (new, not counted) | B1c T4, B1d T6 | the stub on the Mac; the launch states offscreen on Linux |

## Interfaces produced (B2 onwards rely on these names)

**`ressac_ui::settings`** (Task 1):

```rust
pub mod keys {
    pub const APPEARANCE: &str = "appearance";
    pub const CRT: &str = "antiope.crt";
    pub const READER_FONT: &str = "reader.font";
    pub const READER_SIZE: &str = "reader.size";
    pub const READER_SIZE_ANTIOPE: &str = "reader.size.antiope";
    pub const READER_WIDTH: &str = "reader.width";
    pub const DATABASE_PATH: &str = "databasePath";
    pub const CHART_VIEW: &str = "chart.view";
    pub const CHART_INK: &str = "chart.ink";
    pub const CHART_ANIMATE: &str = "chart.animate";
}
pub const READER_SIZE_DEFAULT: f64 = 17.0;
pub const READER_SIZE_MIN: f64 = 12.0;
pub const READER_SIZE_MAX: f64 = 28.0;
pub const ANTIOPE_SIZES: [f64; 4] = [15.0, 20.0, 25.0, 30.0];
pub const ANTIOPE_SIZE_DEFAULT: f64 = 20.0;

// Each enum: #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)], `pub const ALL: [Self; N]`,
// `pub fn as_str(self) -> &'static str`, `pub fn parse(s: &str) -> Option<Self>` (exact spelling).
pub enum Appearance { #[default] Native, Antiope }            // "native", "antiope"
pub enum ReaderFont { #[default] SystemSans, SystemSerif, Newsreader, Atkinson } // "systemSans", "systemSerif", "newsreader", "atkinson"
pub enum LineWidth { Narrow, #[default] Medium, Wide, Forty } // "narrow", "medium", "wide", "forty"
pub enum ChartView { #[default] Harbour, Themes }             // "harbour", "themes"
pub enum ChartInk { #[default] Bleu, Nuit }                   // "bleu", "nuit"
impl LineWidth { pub fn max_ch(self) -> u32 }                 // 58, 68, 80, 40
impl Appearance { pub fn effective(stored: Appearance, bedstead_available: bool) -> Appearance }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSizeCommand { Bigger, Smaller, ActualSize }

pub fn clamped_size(size: f64) -> f64;               // non-finite → 17; rounded; 12…28
pub fn snapped_antiope(size: f64) -> f64;            // non-finite → 20; nearest step, ties go up
pub fn step_antiope(size: f64, delta: i32) -> f64;   // from the snapped step, clamped to 15…30
pub fn effective_size(size: f64, look: Appearance) -> f64;
pub fn actual_size(look: Appearance) -> f64;         // 17 / 20
pub fn size_key(look: Appearance) -> &'static str;   // keys::READER_SIZE / keys::READER_SIZE_ANTIOPE

pub trait SettingsStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn remove(&mut self, key: &str);
}
impl SettingsStore for std::collections::BTreeMap<String, String> { /* the test store */ }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPrefs { pub view: ChartView, pub ink: ChartInk, pub animate: bool } // Default: harbour, bleu, true
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub appearance: Appearance, pub crt: bool, pub reader_font: ReaderFont,
    pub reader_size: f64, pub reader_size_antiope: f64, pub reader_width: LineWidth,
    pub database_path: Option<String>, pub chart: ChartPrefs,
}
impl Default for Settings { /* native, false, systemSans, 17, 20, medium, None, ChartPrefs::default() */ }
impl Settings {
    pub fn load(store: &dyn SettingsStore) -> Settings;
    // Setters: normalise, write the normalised value, return true only when it changed.
    // An unparsable value returns false and writes nothing.
    pub fn set_appearance(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;
    pub fn set_crt(&mut self, store: &mut dyn SettingsStore, value: bool) -> bool;
    pub fn set_reader_font(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;
    pub fn set_reader_size(&mut self, store: &mut dyn SettingsStore, value: f64) -> bool;          // clamped
    pub fn set_reader_size_antiope(&mut self, store: &mut dyn SettingsStore, value: f64) -> bool;  // snapped
    pub fn set_reader_width(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;
    pub fn set_database_path(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;       // blank → key removed, None
    pub fn set_chart_view(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;
    pub fn set_chart_ink(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool;
    pub fn set_chart_animate(&mut self, store: &mut dyn SettingsStore, value: bool) -> bool;
    /// View ▸ Bigger / Smaller / Actual Size on the look shown: writes only that look's key, returns its new size.
    pub fn resize(&mut self, store: &mut dyn SettingsStore, shown: Appearance, command: TextSizeCommand) -> f64;
}
```

Stored spelling: enums by `as_str`, booleans `true` / `false`, sizes by `f64`'s `Display` (`17`, `25`). `load` falls back to the default for a missing or unparsable value, clamps `reader.size`, snaps `reader.size.antiope`, reads a retired ink (`pop`, `pirate`) as `bleu`, and keeps `databasePath` raw (tilde expansion is `launch`'s job) unless blank.

**`ressac_ui::launch`** (Task 2):

```rust
pub const UPDATING: &str = "Updating the database…";
pub const TOO_NEW: &str = "This database was created by a newer Ressac";
pub const NOT_MIGRATED: &str = "The database could not be migrated.";
pub const NEWER_DATABASE: &str = "This database was created by a newer Ressac.";   // Settings ▸ Data (B6b)
pub const NOT_A_DATABASE: &str = "This file is not a usable Ressac database.";     // Settings ▸ Data (B6b)
pub const SCREENSHOT_NEEDS_DB: &str =
    "ressac-kde: --screenshot needs RESSAC_DB (a copy of a fixture database, never the real one)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPhase { Starting, Migrating, TooNew(u32), Failed(String), Ready }
impl LaunchPhase {
    pub fn message(&self) -> Option<String>;          // None, UPDATING, TOO_NEW, the failure, None
    pub fn offers_default_database(&self) -> bool;    // TooNew and Failed only
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchStep { Open, Migrate, Show(LaunchPhase) }
pub fn after_probe(probe: Result<SchemaState, ressac_core::Error>, db: &Path) -> LaunchStep;
pub fn after_migration(outcome: Result<SchemaState, ressac_core::Error>, db: &Path) -> LaunchStep;
pub fn cannot_open(db: &Path, error: &ressac_core::Error) -> LaunchPhase; // Failed("Cannot open <db>: <error>")
pub fn database_location(
    env_db: Option<&OsStr>, stored: Option<&str>, home: Option<&Path>, default_db: Option<&Path>,
) -> Result<PathBuf, ressac_core::paths::PathsError>;
pub fn location_problem(probe: &Result<SchemaState, ressac_core::Error>) -> Option<&'static str>;
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options { pub screenshot: Option<PathBuf> }
pub fn parse_args(args: impl IntoIterator<Item = OsString>, ressac_db: Option<&OsStr>) -> Result<Options, String>;
```

**`ressac-kde`** (Tasks 3–7):

- QML module URI `org.w3cdotorg.ressac`; entry `qrc:/qt/qml/org/w3cdotorg/ressac/qml/Main.qml`. New QML files go in `apps/linux/qml/` (flat) and are picked up by `build.rs` with no edit; QML loads siblings with `Qt.resolvedUrl("Other.qml")` (spike gotcha 3: the qrc prefix is `/qt/qml/org/w3cdotorg/ressac/`).
- **Bridges:** any `apps/linux/src/*.rs` holding a line that starts with `#[cxx_qt::bridge` or `#[cxx::bridge` is compiled as a bridge by `build.rs`; `cpp/*.cpp` are compiled with `cpp/` on the include path. Adding a QRC resource (B3) or a Qt module (`WebChannel`, B7) is a `build.rs` edit.
- **QML singletons** (`#[qml_element] #[qml_singleton]`, `#[auto_cxx_name]` on the `extern "RustQt"` block, custom `READ = …` getters over one Rust value so no state is duplicated):
  - `Launcher`: enum `Launcher.Starting | Migrating | TooNew | Failed | Ready` (`#[qenum(Launcher)] enum Phase`); properties `phase` (`Phase`), `message` (`QString`, `LaunchPhase::message` or empty), `databasePath` (`QString`, empty before resolution); invokables `launch()` (once, from `Starting`), `relaunch()` (only when `offers_default_database`).
  - `AppSettings`: `appearance` (`QString`), `crt` (`bool`), `readerFont` (`QString`), `readerSize` (`f64`), `readerSizeAntiope` (`f64`), `readerWidth` (`QString`), `databasePath` (`QString`, empty = default), `chartView`, `chartInk` (`QString`), `chartAnimate` (`bool`); every one `READ`/`WRITE`/`NOTIFY`, writes through `Settings::set_*` (a refused or unchanged value emits nothing). It is the only writer of the settings file.
  - `Recette`: `active` (`bool`, `CONSTANT`: `--screenshot` was given); invokable `grab(name: QString) -> bool` saves the first visible `QQuickWindow` (WebEngine content included, validated) to `<dir>/<name>.png`.
- **Screenshot walk:** `Main.qml` grabs `01-<phase>` (`ready`, `too-new`, `failed`) once the launch settles, then, only when ready, each entry of `readonly property var walkSteps` (B2 onwards append `{ name: "02-…", enter: function () { … } }`), 400 ms apart, then `Qt.exit(0)`; a failed grab exits 1; a 60 s watchdog exits 1 with `ressac-kde: the screenshot walk timed out`.
- **Rust, crate-private:** `crate::launcher::Session { pub db: PathBuf, pub store: Store }` and `crate::launcher::with_session<R>(f: impl FnOnce(&Session) -> R) -> Option<R>` (the GUI thread's own `Store`, opened with `Store::open_existing` when the launch is ready; B2's models read through it); `crate::settings::QSettingsStore` (`default_file()`, `at(file: &Path)`, `file()`, implements `SettingsStore`); `crate::shim` (the `#[cxx::bridge]` module inside `lib.rs`: every C++ function is declared there, by the task that first calls it from Rust).
- **Worker threads hand results to Qt** with cxx-qt's `Threading`: the QObject's bridge has `impl cxx_qt::Threading for X {}`, the worker gets `let qt = self.qt_thread();`, computes a plain `Send` value (never a Qt type, never the GUI `Store`; it opens its own `Store`), then `qt.queue(move |mut obj| obj.as_mut().apply_<what>(value))`; an `Err` from `queue` (the object is gone, the app is quitting) is dropped with `tracing::debug!`. B1's only worker is the migration (`apply_migration`).
- **C++ shim** (`cpp/shim.h`): `ressac_init_webengine()`, `ressac_create_application()`, `ressac_has_window() -> bool` (a top-level `QQuickWindow` exists), `ressac_exec() -> int`, `ressac_grab_window(rust::Str path) -> bool`, `ressac_settings_default_file() -> rust::String`, `ressac_settings_get(rust::Str file, rust::Str key, rust::String &value) -> bool`, `ressac_settings_set(rust::Str file, rust::Str key, rust::Str value) -> bool`, `ressac_settings_remove(rust::Str file, rust::Str key) -> bool`.
- **Tests that start the app** live in `apps/linux/tests/*.rs` behind `#![cfg(target_os = "linux")]`, and every child process gets `QT_QPA_PLATFORM=offscreen`, `QT_FORCE_STDERR_LOGGING=1`, and `HOME` plus the four `XDG_*` inside a `tempfile::tempdir()` (Qt writes its QML cache to `$XDG_CACHE_HOME/ressac/ressac-kde/qmlcache`, settings to `$XDG_CONFIG_HOME/ressac/`, our logs to `$XDG_STATE_HOME/ressac/logs/`): the gate fails a test that writes into its `HOME`.
- `install.sh`: `apps/linux/install.sh [--software-rendering]` and `apps/linux/install.sh --uninstall`; installs `~/.local/bin/ressac`, `~/.local/bin/ressac-kde`, `$DATA/applications/org.w3cdotorg.ressac.desktop`, `$DATA/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg` (`$DATA` = `$XDG_DATA_HOME` when absolute, else `~/.local/share`).

**Consumed:** `ressac_core::store::{Store, SchemaState}` with `Store::probe`, `Store::open`, `Store::open_existing` (B0c); `ressac_core::logging::init` (B0b); `ressac_core::paths::{Paths, PathsError}` (A1); `apps/linux/dev/dev.sh check`, `scripts/check-linux.sh` and `apps/linux/dev/packages.txt` (A4, unchanged: the gate already uses `--workspace` and exports `QT_QPA_PLATFORM=offscreen`, so `ressac-kde` is built, linted and tested there from B1b on).

**Decisions this phase pins:**

- **Launch = the Mac's `Launcher`, in-process.** Resolve the database (`database_location`: `RESSAC_DB` non-empty, else the `databasePath` setting tilde-expanded, else `Paths::database()`; no `HOME` gives `Failed("HOME is not set to an absolute path")`), then `Store::probe` on the GUI thread (query-only, 250 ms). `Current` → open; `Missing` or `Older(_)` → phase `Migrating`, a worker runs `Store::open` (creates or migrates, the core backs up first) then `Store::probe` again → `after_migration`; `Newer(v)` → `TooNew(v)`; an error → `Failed("Cannot open <db>: <error>")`. Open = `Store::open_existing` on the GUI thread, kept in the `Session`; its failure → `cannot_open`. No question is asked before migrating (the Mac does not ask). "Use Default Database" = `AppSettings.databasePath = ""` (removes the key) then `Launcher.relaunch()`; `RESSAC_DB` still wins.
- **App identity** (shim, before anything else): organisation `ressac`, application `ressac-kde`, desktop file name `org.w3cdotorg.ressac` (Wayland app id), window icon `QIcon::fromTheme("org.w3cdotorg.ressac")`, `QApplication` (the `org.kde.desktop` style paints with QStyle), style `org.kde.desktop` unless `QT_QUICK_CONTROLS_STYLE` is set, icon theme `breeze` when none (or `hicolor`) is set, fallback `breeze`. `QtWebEngineQuick::initialize()` with `AA_ShareOpenGLContexts` runs before the application object (B3's reader needs it there; costs nothing until a view exists). Qt gets no command-line arguments (`argv = {"ressac-kde"}`); use environment variables (`QT_QPA_PLATFORM=…`).
- **Exit codes:** 0 normal and after a completed walk; 1 when the walk fails or times out, or when `Main.qml` did not produce a window (`ressac-kde: the main window did not load (see the QML errors above)`); 2 for a refused command line (`parse_args`'s message on stderr). Off Linux: `ressac-kde runs on Linux only.` on stderr, exit 1.
- **Logs:** `run()` calls `ressac_core::logging::init(&paths.log_dir(), false)` when `Paths::from_env()` succeeds and keeps the guard for the process lifetime; the launch logs its database and its outcome at `info`.
- **Placeholder:** the `Ready` screen in B1 is a `Kirigami.PlaceholderMessage` with `text: "Ressac"` and `explanation: Launcher.databasePath`, replaced by the reading view in B2c.

**Step 0:** not triggered (new files only; the root `Cargo.toml` is 40 lines).

**Gate (end of each sub-phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

From B1b on, `dev.sh check` builds, lints and tests `ressac-kde` (first cold build of the Qt crates about 1–2 min more); from B1d on its tests start the app offscreen four times (about 3 s each). gcc 16 prints `-Wsfinae-incomplete` build-script warnings for Qt headers next to `rust/cxx.h` (spike gotcha 7): they are not rustc warnings and do not fail `-D warnings`.

## Review Focus

1. **A `databasePath` setting that points at a newer or broken file, or nowhere** → the window offers Use Default Database (and Quit on failure) instead of crashing or migrating something it should not; a missing file is created; `RESSAC_DB` still wins. Tests: Task 2 (`each_probe_result_leads_to_its_step`, `the_database_location_follows_ressac_db_then_the_setting_then_the_default`), Task 6 (`a_newer_database_is_refused_and_left_alone`, `a_file_that_is_not_a_database_fails_and_is_left_alone`).
2. **A hand-edited or older settings file** (`reader.size=huge`, `reader.font=comic-sans`, `chart.ink=pop`, `reader.size.antiope=17`) → defaults or the nearest legal value, never a crash or an illegal size. Tests: Task 1 (`load_reads_the_store_and_ignores_garbage`), Task 5 (`load_through_qsettings_matches_the_memory_store`).
3. **A recette or test run near the real data** → `--screenshot` without `RESSAC_DB` is refused before Qt starts; every test child gets its own `HOME`/`XDG_*`. Tests: Task 2 (`the_command_line_takes_only_a_hidden_screenshot_flag`), Task 6 (`the_screenshot_flag_needs_ressac_db`), and the gate's leftover check.
4. **The Mac build pulling Qt in** → `cargo test --workspace` on the Mac builds the stub only, which says so and exits 1. Tests: Task 3 Step 4 (`cargo tree`), Task 4 (`off_linux_the_binary_says_so_and_exits_1`).
5. **`install.sh --uninstall` on marghe** → removes exactly the four installed files, never the database, the settings or the logs. Test: Task 7 Step 3.

---

## B1a: `ressac-ui` (settings and launch decisions)

### Task 1 (B1a): the `ressac-ui` crate and `ressac_ui::settings`

**Files:**
- Modify: `Cargo.toml` (`members = ["crates/ressac-core", "crates/ressac", "crates/ressac-ui"]`)
- Create: `crates/ressac-ui/Cargo.toml` (`description = "View models and pure logic of the Ressac desktop app (no Qt)"`, workspace `version`/`edition`/`rust-version`/`license`; `[dependencies] ressac-core = { path = "../ressac-core" }`; `[dev-dependencies] tempfile.workspace = true`)
- Create: `crates/ressac-ui/src/lib.rs` (crate doc: "The Ressac desktop app's logic, without Qt: every model, state machine and pure renderer the KDE app (`apps/linux`) draws. No I/O except through `ressac-core`; time and time zone are always passed in." and `pub mod settings;`)
- Create: `crates/ressac-ui/src/settings.rs` (tests in its `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `ressac_ui::settings` as in the header.

- [ ] **Step 1: Write the failing tests** in `crates/ressac-ui/src/settings.rs` (`use std::collections::BTreeMap;` and a helper `fn store(pairs: &[(&str, &str)]) -> BTreeMap<String, String>`):

```rust
    #[test]
    fn keys_are_the_mac_app_s() {
        assert_eq!(
            [keys::APPEARANCE, keys::CRT, keys::READER_FONT, keys::READER_SIZE, keys::READER_SIZE_ANTIOPE,
             keys::READER_WIDTH, keys::DATABASE_PATH, keys::CHART_VIEW, keys::CHART_INK, keys::CHART_ANIMATE],
            ["appearance", "antiope.crt", "reader.font", "reader.size", "reader.size.antiope",
             "reader.width", "databasePath", "chart.view", "chart.ink", "chart.animate"]
        );
    }

    #[test]
    fn values_are_spelled_once_and_parse_back() {
        assert_eq!(Appearance::ALL.map(Appearance::as_str), ["native", "antiope"]);
        assert_eq!(ReaderFont::ALL.map(ReaderFont::as_str), ["systemSans", "systemSerif", "newsreader", "atkinson"]);
        assert_eq!(LineWidth::ALL.map(LineWidth::as_str), ["narrow", "medium", "wide", "forty"]);
        assert_eq!(LineWidth::ALL.map(LineWidth::max_ch), [58, 68, 80, 40]);
        assert_eq!(ChartView::ALL.map(ChartView::as_str), ["harbour", "themes"]);
        assert_eq!(ChartInk::ALL.map(ChartInk::as_str), ["bleu", "nuit"]);
        for font in ReaderFont::ALL {
            assert_eq!(ReaderFont::parse(font.as_str()), Some(font));
        }
        assert_eq!(Appearance::parse("teletext"), None);
        assert_eq!(Appearance::parse("Antiope"), None); // exact spelling only
    }

    #[test]
    fn an_empty_store_gives_the_defaults() {
        let s = Settings::load(&BTreeMap::<String, String>::new());
        assert_eq!(s, Settings::default());
        assert_eq!((s.appearance, s.crt, s.reader_font), (Appearance::Native, false, ReaderFont::SystemSans));
        assert_eq!((s.reader_size, s.reader_size_antiope, s.reader_width), (17.0, 20.0, LineWidth::Medium));
        assert_eq!(s.database_path, None);
        assert_eq!(s.chart, ChartPrefs { view: ChartView::Harbour, ink: ChartInk::Bleu, animate: true });
    }

    #[test]
    fn load_reads_the_store_and_ignores_garbage() {
        let good = store(&[("appearance", "antiope"), ("antiope.crt", "true"), ("reader.font", "newsreader"),
            ("reader.size", "21"), ("reader.size.antiope", "25"), ("reader.width", "wide"),
            ("databasePath", "~/Feeds/other.sqlite"), ("chart.view", "themes"), ("chart.ink", "nuit"),
            ("chart.animate", "false")]);
        let s = Settings::load(&good);
        assert_eq!((s.appearance, s.crt, s.reader_font, s.reader_size), (Appearance::Antiope, true, ReaderFont::Newsreader, 21.0));
        assert_eq!((s.reader_size_antiope, s.reader_width), (25.0, LineWidth::Wide));
        assert_eq!(s.database_path.as_deref(), Some("~/Feeds/other.sqlite")); // raw: launch expands it
        assert_eq!(s.chart, ChartPrefs { view: ChartView::Themes, ink: ChartInk::Nuit, animate: false });

        let bad = store(&[("appearance", "teletext"), ("antiope.crt", "yes"), ("reader.font", "comic-sans"),
            ("reader.size", "99"), ("reader.size.antiope", "17"), ("reader.width", "huge"),
            ("databasePath", "   "), ("chart.view", "map"), ("chart.ink", "pop"), ("chart.animate", "0")]);
        let s = Settings::load(&bad);
        assert_eq!((s.appearance, s.crt, s.reader_font), (Appearance::Native, false, ReaderFont::SystemSans));
        assert_eq!((s.reader_size, s.reader_size_antiope, s.reader_width), (28.0, 15.0, LineWidth::Medium));
        assert_eq!(s.database_path, None);
        assert_eq!(s.chart, ChartPrefs::default()); // "pop" is a retired ink: bleu
        assert_eq!(Settings::load(&store(&[("reader.size", "nan"), ("reader.size.antiope", "inf")])).reader_size, 17.0);
        assert_eq!(Settings::load(&store(&[("reader.size.antiope", "inf")])).reader_size_antiope, 20.0);
    }

    #[test]
    fn text_size_steps_and_clamps() {
        assert_eq!([clamped_size(40.0), clamped_size(3.0), clamped_size(16.6), clamped_size(f64::NAN)], [28.0, 12.0, 17.0, 17.0]);
    }

    #[test]
    fn antiope_sizes_snap_to_the_nearest_step_and_ties_go_up() {
        let cases = [(12.0, 15.0), (15.0, 15.0), (17.0, 15.0), (17.5, 20.0), (18.0, 20.0), (20.0, 20.0), (22.0, 20.0),
            (22.5, 25.0), (23.0, 25.0), (27.0, 25.0), (27.5, 30.0), (28.0, 30.0), (40.0, 30.0), (-3.0, 15.0),
            (f64::NAN, 20.0), (f64::INFINITY, 20.0)];
        for (size, expected) in cases {
            assert_eq!(snapped_antiope(size), expected, "{size}");
        }
    }

    #[test]
    fn antiope_steps_move_from_the_snapped_size_and_stop_at_the_ends() {
        assert_eq!(step_antiope(17.0, 1), 20.0); // 17 draws at 15
        assert_eq!(step_antiope(18.0, 1), 25.0); // 18 draws at 20
        assert_eq!([step_antiope(20.0, -1), step_antiope(30.0, 1), step_antiope(15.0, -1), step_antiope(15.0, 7), step_antiope(25.0, 0)],
                   [15.0, 30.0, 15.0, 30.0, 25.0]);
        assert_eq!([effective_size(17.0, Appearance::Antiope), effective_size(17.0, Appearance::Native)], [15.0, 17.0]);
        assert_eq!([effective_size(f64::NAN, Appearance::Antiope), effective_size(40.0, Appearance::Native)], [20.0, 28.0]);
        assert_eq!([actual_size(Appearance::Native), actual_size(Appearance::Antiope)], [17.0, 20.0]);
        assert_eq!([size_key(Appearance::Native), size_key(Appearance::Antiope)], ["reader.size", "reader.size.antiope"]);
    }

    /// Decision of 2026-10-14 (Mac `TextSizePerLookTests`): each look keeps its own size.
    #[test]
    fn resizing_acts_on_the_look_shown_and_never_on_the_other() {
        let mut st: BTreeMap<String, String> = BTreeMap::new();
        let mut s = Settings::load(&st);
        assert_eq!(s.resize(&mut st, Appearance::Antiope, TextSizeCommand::Bigger), 25.0);
        assert_eq!(st.get("reader.size.antiope").map(String::as_str), Some("25"));
        assert_eq!(st.get("reader.size"), None);
        assert_eq!(s.resize(&mut st, Appearance::Native, TextSizeCommand::Smaller), 16.0);
        assert_eq!((st["reader.size"].as_str(), st["reader.size.antiope"].as_str()), ("16", "25"));
        assert_eq!(s.resize(&mut st, Appearance::Native, TextSizeCommand::Bigger), 17.0);
        assert_eq!(s.resize(&mut st, Appearance::Antiope, TextSizeCommand::ActualSize), 20.0);
        assert_eq!(st["reader.size"].as_str(), "17");
        for stored in ["17", "0", "-3", "22.5", "99"] {
            for command in [TextSizeCommand::Bigger, TextSizeCommand::Smaller, TextSizeCommand::ActualSize] {
                let mut st = store(&[("reader.size.antiope", stored)]);
                let mut s = Settings::load(&st);
                let size = s.resize(&mut st, Appearance::Antiope, command);
                assert!(ANTIOPE_SIZES.contains(&size), "{stored} {command:?}");
                assert_eq!(st["reader.size.antiope"], size.to_string());
            }
        }
    }

    #[test]
    fn setters_normalise_write_and_report_changes() {
        let mut st: BTreeMap<String, String> = BTreeMap::new();
        let mut s = Settings::load(&st);
        assert!(s.set_reader_size(&mut st, 40.0));
        assert_eq!((s.reader_size, st["reader.size"].as_str()), (28.0, "28"));
        assert!(!s.set_reader_size(&mut st, 28.4)); // same normalised value: no change
        assert!(s.set_reader_size_antiope(&mut st, 23.0));
        assert_eq!(st["reader.size.antiope"].as_str(), "25");
        assert!(!s.set_appearance(&mut st, "teletext"));
        assert_eq!(st.get("appearance"), None);
        assert!(s.set_appearance(&mut st, "antiope") && s.set_crt(&mut st, true) && s.set_reader_font(&mut st, "atkinson"));
        assert!(s.set_reader_width(&mut st, "forty") && s.set_chart_view(&mut st, "themes") && s.set_chart_ink(&mut st, "nuit"));
        assert!(s.set_chart_animate(&mut st, false));
        assert_eq!(Settings::load(&st), s); // what is stored reads back the same
        assert!(s.set_database_path(&mut st, "/data/r.sqlite"));
        assert_eq!(st["databasePath"].as_str(), "/data/r.sqlite");
        assert!(s.set_database_path(&mut st, "  "));
        assert_eq!((st.get("databasePath"), s.database_path.as_deref()), (None, None));
        assert!(!s.set_database_path(&mut st, ""));
    }

    /// Mac `antiopeFallsBackToNativeWithoutBedstead`: never Antiope in a fallback font.
    #[test]
    fn antiope_only_with_bedstead() {
        assert_eq!(Appearance::effective(Appearance::Antiope, true), Appearance::Antiope);
        assert_eq!(Appearance::effective(Appearance::Antiope, false), Appearance::Native);
        assert_eq!(Appearance::effective(Appearance::Native, true), Appearance::Native);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac-ui settings::`
Expected: compile errors (`cannot find … in this scope`) once the module skeleton exists; before that, `error: package(s) ressac-ui not found` until `Cargo.toml` and `lib.rs` are in place.

- [ ] **Step 3: Implement** `settings.rs` as the header's block; `Settings::resize` = `effective_size` of the size `shown` stores, then `clamped_size(±1)` (Native) or `step_antiope(±1)` (Antiope) or `actual_size`, kept in `self` and **always** written under `size_key(shown)` (unlike the setters: a hand-edited `0` that loads as 15 is rewritten `15`, Mac `theAntiopeSizeIsAlwaysAStep`).

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac-ui && cargo clippy -p ressac-ui --all-targets -- -D warnings`
Expected: `test result: ok.` (9 passed); clippy `Finished`, no warning.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ressac-ui
git commit -m "ressac-ui: new Qt-free crate, settings keys, defaults, sizes per look

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B1a): `ressac_ui::launch`

**Files:**
- Create: `crates/ressac-ui/src/launch.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod launch;`)

**Interfaces:**
- Consumes: `ressac_core::store::{Store, SchemaState}`, `Store::probe` (B0c), `ressac_core::Error`, `ressac_core::paths::PathsError`.
- Produces: `ressac_ui::launch` as in the header.

Rules: `database_location`: `env_db` non-empty → it, as given; else `stored` trimmed non-empty → `"~"` is `home`, `"~/x"` is `home/x` (no `home` → `Err(PathsError::NoHome)`), any other spelling (`"~bob/x"`, relative, absolute) as given; else `default_db` or `Err(PathsError::NoHome)`. `parse_args` takes the arguments after the program name: `--screenshot <dir>` or `--screenshot=<dir>` (once); `--screenshot` with no value → `Err("ressac-kde: --screenshot needs a directory")`; a screenshot with `ressac_db` absent or empty → `Err(SCREENSHOT_NEEDS_DB)`; anything else → `Err("ressac-kde: unknown argument <arg>")` (lossy UTF-8). `location_problem`: `Ok(Newer(_))` → `NEWER_DATABASE`, `Err(_)` → `NOT_A_DATABASE`, otherwise `None`.

- [ ] **Step 1: Write the failing tests** (`use std::ffi::{OsStr, OsString}; use std::path::{Path, PathBuf}; use ressac_core::Error; use ressac_core::store::{SchemaState, Store};`):

```rust
    fn invalid(s: &str) -> Error { Error::Invalid(s.to_string()) }
    const DB: &str = "/d/r.sqlite";

    #[test]
    fn the_database_location_follows_ressac_db_then_the_setting_then_the_default() {
        let home = Path::new("/home/u");
        let default = PathBuf::from("/home/u/.local/share/ressac/ressac.sqlite");
        let at = |env: Option<&str>, stored: Option<&str>| {
            database_location(env.map(OsStr::new), stored, Some(home), Some(&default)).unwrap()
        };
        assert_eq!(at(None, None), default);
        assert_eq!(at(None, Some("  ")), default);
        assert_eq!(at(None, Some("~/Feeds/other.sqlite")), PathBuf::from("/home/u/Feeds/other.sqlite"));
        assert_eq!(at(None, Some("~")), PathBuf::from("/home/u"));
        assert_eq!(at(None, Some("~bob/r.sqlite")), PathBuf::from("~bob/r.sqlite"));
        assert_eq!(at(None, Some("/data/r.sqlite")), PathBuf::from("/data/r.sqlite"));
        assert_eq!(at(Some("/tmp/copy.sqlite"), Some("~/Feeds/other.sqlite")), PathBuf::from("/tmp/copy.sqlite"));
        assert_eq!(at(Some(""), Some("~/Feeds/other.sqlite")), PathBuf::from("/home/u/Feeds/other.sqlite"));
        // No HOME: only an explicit path works, as in the CLI.
        assert_eq!(database_location(Some(OsStr::new("/tmp/c.sqlite")), None, None, None).unwrap(), PathBuf::from("/tmp/c.sqlite"));
        assert_eq!(database_location(None, Some("/abs/r.sqlite"), None, None).unwrap(), PathBuf::from("/abs/r.sqlite"));
        assert_eq!(database_location(None, Some("~/r.sqlite"), None, None), Err(ressac_core::paths::PathsError::NoHome));
        assert_eq!(database_location(None, None, None, None), Err(ressac_core::paths::PathsError::NoHome));
    }

    #[test]
    fn each_probe_result_leads_to_its_step() {
        let db = Path::new(DB);
        assert_eq!(after_probe(Ok(SchemaState::Current), db), LaunchStep::Open);
        for state in [SchemaState::Missing, SchemaState::Older(0), SchemaState::Older(3)] {
            assert_eq!(after_probe(Ok(state), db), LaunchStep::Migrate, "{state:?}");
        }
        assert_eq!(after_probe(Ok(SchemaState::Newer(7)), db), LaunchStep::Show(LaunchPhase::TooNew(7)));
        assert_eq!(after_probe(Err(invalid("file is not a database")), db),
                   LaunchStep::Show(LaunchPhase::Failed("Cannot open /d/r.sqlite: file is not a database".into())));
    }

    #[test]
    fn a_migration_must_end_current() {
        let db = Path::new(DB);
        assert_eq!(after_migration(Ok(SchemaState::Current), db), LaunchStep::Open);
        for state in [SchemaState::Missing, SchemaState::Older(5), SchemaState::Newer(9)] {
            assert_eq!(after_migration(Ok(state), db), LaunchStep::Show(LaunchPhase::Failed(NOT_MIGRATED.into())), "{state:?}");
        }
        assert_eq!(after_migration(Err(invalid("disk full")), db),
                   LaunchStep::Show(LaunchPhase::Failed("Cannot open /d/r.sqlite: disk full".into())));
        assert_eq!(cannot_open(db, &invalid("locked")), LaunchPhase::Failed("Cannot open /d/r.sqlite: locked".into()));
    }

    #[test]
    fn phases_show_the_mac_app_s_sentences() {
        assert_eq!(LaunchPhase::Starting.message(), None);
        assert_eq!(LaunchPhase::Migrating.message().as_deref(), Some("Updating the database…"));
        assert_eq!(LaunchPhase::TooNew(7).message().as_deref(), Some("This database was created by a newer Ressac"));
        assert_eq!(LaunchPhase::Failed("x".into()).message().as_deref(), Some("x"));
        assert_eq!(LaunchPhase::Ready.message(), None);
        let offers: Vec<bool> = [LaunchPhase::Starting, LaunchPhase::Migrating, LaunchPhase::TooNew(7),
            LaunchPhase::Failed("x".into()), LaunchPhase::Ready].iter().map(LaunchPhase::offers_default_database).collect();
        assert_eq!(offers, [false, false, true, true, false]);
    }

    #[test]
    fn a_chosen_database_is_refused_only_when_newer_or_unreadable() {
        for state in [SchemaState::Missing, SchemaState::Current, SchemaState::Older(2)] {
            assert_eq!(location_problem(&Ok(state)), None);
        }
        assert_eq!(location_problem(&Ok(SchemaState::Newer(9))), Some("This database was created by a newer Ressac."));
        assert_eq!(location_problem(&Err(invalid("x"))), Some("This file is not a usable Ressac database."));
    }

    #[test]
    fn probing_real_files_drives_the_launch() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sub/ressac.sqlite");
        assert_eq!(after_probe(Store::probe(&db), &db), LaunchStep::Migrate);
        drop(Store::open(&db).unwrap());
        assert_eq!(after_probe(Store::probe(&db), &db), LaunchStep::Open);
        let text = dir.path().join("notes.txt");
        std::fs::write(&text, "not a database, just words long enough to fill a page header").unwrap();
        match after_probe(Store::probe(&text), &text) {
            LaunchStep::Show(LaunchPhase::Failed(m)) => assert!(m.starts_with(&format!("Cannot open {}: ", text.display())), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_command_line_takes_only_a_hidden_screenshot_flag() {
        let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
        let db = Some(OsStr::new("/tmp/copy.sqlite"));
        assert_eq!(parse_args(args(&[]), None), Ok(Options::default()));
        let shots = Options { screenshot: Some(PathBuf::from("/tmp/s")) };
        assert_eq!(parse_args(args(&["--screenshot", "/tmp/s"]), db), Ok(shots.clone()));
        assert_eq!(parse_args(args(&["--screenshot=/tmp/s"]), db), Ok(shots));
        assert_eq!(parse_args(args(&["--screenshot"]), db), Err("ressac-kde: --screenshot needs a directory".into()));
        for none in [None, Some(OsStr::new(""))] {
            assert_eq!(parse_args(args(&["--screenshot", "/tmp/s"]), none), Err(SCREENSHOT_NEEDS_DB.into()));
        }
        assert_eq!(parse_args(args(&["--help"]), db), Err("ressac-kde: unknown argument --help".into()));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac-ui launch::`
Expected: compile errors (`cannot find function after_probe`, …).

- [ ] **Step 3: Implement** `launch.rs` per the header and the rules above (`cannot_open` formats with `db.display()` and the error's `Display`).

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac-ui && cargo clippy -p ressac-ui --all-targets -- -D warnings`
Expected: `test result: ok.` (16 passed); clippy clean. Then the full gate of the header (B1a ends here).

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/launch.rs crates/ressac-ui/src/lib.rs
git commit -m "ressac-ui: launch decisions (database location, probe, migration, command line)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B1b: the `ressac-kde` crate, the stub, a first window

### Task 3 (B1b): workspace member, stub off Linux, Kirigami window on Linux

**Files:**
- Modify: `Cargo.toml` (root)
- Create: `apps/linux/Cargo.toml`, `apps/linux/build.rs`, `apps/linux/src/main.rs`, `apps/linux/qml/Main.qml`

**Interfaces:**
- Consumes: nothing from earlier tasks (the dependencies are declared now for B1c and B1d).
- Produces: package `ressac-kde`, binary `ressac-kde`; the `build.rs` discovery rules of the header; QML module `org.w3cdotorg.ressac`.

Root `Cargo.toml`: `members = ["crates/ressac-core", "crates/ressac", "crates/ressac-ui", "apps/linux"]`, `default-members = ["crates/ressac-core", "crates/ressac", "crates/ressac-ui"]` (spec §7.1: plain `cargo build` and `cargo run` stay Qt-free and unambiguous; the gates use `--workspace`); in `[workspace.dependencies]`, alphabetically: `cxx = "1.0.205"`, `cxx-qt = "0.10.0"`, `cxx-qt-build = "0.10.0"`, `cxx-qt-lib = { version = "0.10.0", features = ["qt_gui", "qt_qml"] }`.

`apps/linux/Cargo.toml`: `name = "ressac-kde"`, `description = "Ressac for KDE Plasma (Qt 6, QML, Kirigami)"`, workspace `version`/`edition`/`rust-version`/`license`, `publish = false`; **no** `[dependencies]` table; `[target.'cfg(target_os = "linux")'.dependencies]`: `cxx`, `cxx-qt`, `cxx-qt-lib`, `tracing` (all `.workspace = true`), `ressac-core = { path = "../../crates/ressac-core" }`, `ressac-ui = { path = "../../crates/ressac-ui" }`; `[target.'cfg(target_os = "linux")'.build-dependencies]`: `cxx-qt-build.workspace = true`; `[target.'cfg(target_os = "linux")'.dev-dependencies]`: `image`, `rusqlite`, `tempfile` (`.workspace = true`). (Target-specific build dependencies follow the host; every build here is native.)

`apps/linux/build.rs` (the only algorithm worth fixing here):

```rust
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
    fn sources(dir: &str, ext: &str) -> Vec<PathBuf> { /* read_dir, filter by extension, sort */ }

    /// A Rust file is a bridge when a line starts (after indentation) with `#[cxx_qt::bridge` or `#[cxx::bridge`.
    fn is_bridge(text: &str) -> bool { /* … */ }

    pub fn build() {
        for dir in ["src", "qml", "cpp"] {
            println!("cargo::rerun-if-changed={dir}"); // new files are picked up
        }
        let bridges = sources("src", "rs").into_iter().filter(|p| is_bridge(&std::fs::read_to_string(p).unwrap()));
        cxx_qt_build::CxxQtBuilder::new_qml_module(
            cxx_qt_build::QmlModule::new("org.w3cdotorg.ressac").qml_files(sources("qml", "qml")).depend("QtQuick"),
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
```

`src/main.rs` (B1b; rewritten in Task 4): crate doc "ressac-kde: Ressac for KDE Plasma. It builds everywhere so `cargo test --workspace` needs no Qt; off Linux it is a stub."; `#[cfg(not(target_os = "linux"))] fn main() -> ExitCode` printing `ressac-kde runs on Linux only.` with `eprintln!` and returning `ExitCode::from(1)`; `#[cfg(target_os = "linux")] fn main() -> ExitCode` creating `cxx_qt_lib::QGuiApplication::new()`, a `QQmlApplicationEngine`, loading `qrc:/qt/qml/org/w3cdotorg/ressac/qml/Main.qml`, returning `exec()`'s code (`u8::try_from(code).unwrap_or(1)`). **No `tests/` directory in B1b**: a bin-only cxx-qt crate cannot link integration tests (deviation 3); Task 4 adds the library and the first test.

`qml/Main.qml`: `import QtQuick`, `import QtQuick.Controls as QQC2`, `import org.kde.kirigami as Kirigami`; `Kirigami.ApplicationWindow` with `title: "Ressac"`, `width: 1100`, `height: 720`, `minimumWidth: 900`, `minimumHeight: 500` (inventory §1.1), `visible: true`, `pageStack.initialPage: Kirigami.Page` with `globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None` holding a centred running `QQC2.BusyIndicator` (the `Starting` look).

- [ ] **Step 1: Write the failing checks**

```bash
cargo run -q -p ressac-kde; echo "exit=$?"
```
Expected now: `error: package(s) ressac-kde not found in workspace`.

- [ ] **Step 2: Implement** the five files as above.

- [ ] **Step 3: The stub on the Mac**

Run: `cargo run -q -p ressac-kde; echo "exit=$?"`
Expected: `ressac-kde runs on Linux only.` then `exit=1`.
Run: `cargo build -v 2>&1 | grep -c 'ressac-kde'`
Expected: `0` (not a default member).

- [ ] **Step 4: No Qt crate on the Mac**

Run: `cargo tree -p ressac-kde -e normal,build | grep -cE 'cxx|ressac-core'`
Expected: `0` (all Linux-only). `grep -c 'name = "cxx-qt' Cargo.lock` is `4` or more (the lock file lists them for Linux; commit it).

- [ ] **Step 5: The window on Linux**

Run:
```bash
apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" ; h=$(mktemp -d); HOME=$h XDG_CACHE_HOME=$h/cache XDG_CONFIG_HOME=$h/config timeout 10 "$CARGO_TARGET_DIR/debug/ressac-kde" > $h/log 2>&1; echo "exit=$?"; grep -E "failed to load component|is not installed|Error:|TypeError|ReferenceError" $h/log; echo "errors=$?"'
```
Expected: no `error` line, `exit=124` (still running when `timeout` stops it: the window is up), `errors=1` (no QML error line).

- [ ] **Step 6: Gate, then commit.** The full gate of the header (B1b ends here).

```bash
git add Cargo.toml Cargo.lock apps/linux/Cargo.toml apps/linux/build.rs apps/linux/src/main.rs apps/linux/qml/Main.qml
git commit -m "ressac-kde: new workspace member (cxx-qt, Kirigami), a stub off Linux

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B1c: the C++ shim, the library, the settings bridge

### Task 4 (B1c): `lib.rs` + C++ shim (QApplication, Breeze, identity)

**Files:**
- Modify: `apps/linux/src/main.rs` (Linux `main` = `ressac_kde::run()`; the stub unchanged)
- Create: `apps/linux/src/lib.rs`, `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp`
- Test: `apps/linux/tests/stub.rs` (not counted)

**Interfaces:**
- Consumes: Task 3's crate.
- Produces: `pub fn run() -> ExitCode` in `lib.rs` (crate attribute `#![cfg(target_os = "linux")]`: the library is empty elsewhere); `#[cxx::bridge] mod shim` in `lib.rs` declaring `ressac_init_webengine`, `ressac_create_application`, `ressac_has_window`, `ressac_exec`; the full C++ shim of the header (all nine functions defined now, so B1d touches no C++: `ressac_grab_window` saves `qw->grabWindow()` of the first visible `QQuickWindow`, false when there is none or the save fails; the `QSettings` functions open `QSettings(file, QSettings::IniFormat)` per call, `set`/`remove` end with `sync()` and return `status() == QSettings::NoError`; `default_file` returns `QSettings(u"ressac"_s, u"ressac-kde"_s).fileName()`).

`run()` order: `shim::ressac_init_webengine()`; `shim::ressac_create_application()` (the header's "App identity", in that order: names, then `new QApplication(argc, argv)` with static `argc = 1`, `argv = {"ressac-kde", nullptr}`, then desktop file name, window icon, style via `QQuickStyle::setStyle` unless `QT_QUICK_CONTROLS_STYLE` is set, icon theme); a `QQmlApplicationEngine` loading `Main.qml`; if `!shim::ressac_has_window()` → `eprintln!("ressac-kde: the main window did not load (see the QML errors above)")`, `ExitCode::from(1)`; else `shim::ressac_exec()` mapped as in Task 3. `shim.cpp` includes `<QtQuickControls2/QQuickStyle>` (hence the `QuickControls2` module), `<QtWebEngineQuick/QtWebEngineQuick>`, `<QtWidgets/QApplication>`.

- [ ] **Step 1: Write the failing test** `apps/linux/tests/stub.rs`:

```rust
//! Off Linux, ressac-kde is a stub (master plan, "ressac-kde").
#![cfg(not(target_os = "linux"))]

#[test]
fn off_linux_the_binary_says_so_and_exits_1() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ressac-kde")).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "ressac-kde runs on Linux only.\n");
    assert!(out.stdout.is_empty());
}
```

- [ ] **Step 2: Run it** — `cargo test -p ressac-kde --test stub` passes already on the Mac (the stub exists); the Linux side is what changes: run `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep -E "undefined symbol|test result"'` with `tests/stub.rs` present and before `lib.rs` exists.
Expected: `ld.lld: error: undefined symbol: cxx_qt_init_crate_cxx_qt` (deviation 3).

- [ ] **Step 3: Implement** `lib.rs`, `shim.h`, `shim.cpp` and the new `main.rs` as above.

- [ ] **Step 4: Run it to verify it passes, and the window still loads**

Run (Mac): `cargo test -p ressac-kde` → `test result: ok. 1 passed` for `stub`.
Run (Linux): `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every line `ok`.
Run (Linux): the Task 3 Step 5 command again → `exit=124`, `errors=1`, and `grep -c "breeze-internal" $h/log` is `0` (the icon theme is set; spike gotcha 6).

- [ ] **Step 5: Commit**

```bash
git add apps/linux/src/main.rs apps/linux/src/lib.rs apps/linux/cpp/shim.h apps/linux/cpp/shim.cpp apps/linux/tests/stub.rs
git commit -m "ressac-kde: Qt layer as a library, C++ shim (QApplication, Breeze, app identity)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 5 (B1c): the settings bridge (`QSettingsStore`, `AppSettings`)

**Files:**
- Create: `apps/linux/src/settings.rs` (Linux-only unit tests in its `mod tests`)
- Modify: `apps/linux/src/lib.rs` (`mod settings;`; the four `ressac_settings_*` declarations in `mod shim`)

**Interfaces:**
- Consumes: `ressac_ui::settings::{Settings, SettingsStore, keys}` (Task 1); the shim (Task 4).
- Produces: `QSettingsStore { file: PathBuf }` with `pub fn default_file() -> QSettingsStore`, `pub fn at(file: &Path) -> QSettingsStore`, `pub fn file(&self) -> &Path`, `impl SettingsStore` (a failed `set`/`remove` logs `tracing::warn!("cannot write {key} in {file}")`); the `AppSettings` singleton of the header, struct `AppSettingsRust { store: QSettingsStore, settings: Settings }` (`Default` = `default_file()` and `Settings::load`), each property `READ = <name>` and `WRITE = set_<name>` declared in the bridge (validated syntax: `#[qproperty(f64, reader_size, READ = reader_size, WRITE = set_reader_size, NOTIFY)]` with `fn reader_size(self: &AppSettings) -> f64;` and `fn set_reader_size(self: Pin<&mut AppSettings>, value: f64);` under `#[auto_cxx_name]`); a setter calls the matching `Settings::set_*` and emits `<name>_changed` only when it returns true.

- [ ] **Step 1: Write the failing tests** in `apps/linux/src/settings.rs`:

```rust
    #[test]
    fn a_value_round_trips_through_the_ini_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = QSettingsStore::at(&dir.path().join("ressac-kde.conf"));
        assert_eq!(store.get(keys::READER_SIZE), None);
        store.set(keys::READER_SIZE, "21");
        store.set(keys::DATABASE_PATH, "~/Flux é/r.sqlite");
        assert_eq!(store.get(keys::READER_SIZE).as_deref(), Some("21"));
        assert_eq!(store.get(keys::DATABASE_PATH).as_deref(), Some("~/Flux é/r.sqlite"));
        store.remove(keys::READER_SIZE);
        assert_eq!(store.get(keys::READER_SIZE), None);
        let text = std::fs::read_to_string(store.file()).unwrap();
        assert!(text.starts_with("[General]"), "{text}");
    }

    #[test]
    fn load_through_qsettings_matches_the_memory_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut q = QSettingsStore::at(&dir.path().join("ressac-kde.conf"));
        let mut m: std::collections::BTreeMap<String, String> = Default::default();
        for (key, value) in [(keys::READER_FONT, "comic-sans"), (keys::READER_SIZE, "99"),
                             (keys::READER_SIZE_ANTIOPE, "17"), (keys::CHART_INK, "pop"), (keys::CRT, "true")] {
            q.set(key, value);
            m.set(key, value);
        }
        assert_eq!(Settings::load(&q), Settings::load(&m));
        let mut s = Settings::load(&q);
        assert!(s.set_reader_size_antiope(&mut q, 23.0));
        assert_eq!(Settings::load(&QSettingsStore::at(q.file())).reader_size_antiope, 25.0);
    }

    #[test]
    fn the_default_file_is_ressac_kde_conf_in_the_config_directory() {
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".config"));
        assert_eq!(QSettingsStore::default_file().file(), config.join("ressac/ressac-kde.conf"));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --lib settings:: 2>&1 | grep -E "^error|test result"'`
Expected: `error[E0433]`/`E0425` (`QSettingsStore` not found).

- [ ] **Step 3: Implement** `settings.rs` and the `lib.rs` additions as above. No QML uses `AppSettings` yet; it is registered with the module and exercised from QML in Task 6.

- [ ] **Step 4: Run them to verify they pass**

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"; cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1'`
Expected: every `test result: ok.` (3 in the lib); clippy `Finished`. Then the full gate (B1c ends here; the gate's leftover check proves the tests wrote nothing in `HOME`).

- [ ] **Step 5: Commit**

```bash
git add apps/linux/src/settings.rs apps/linux/src/lib.rs
git commit -m "ressac-kde: settings in QSettings (ressac-kde.conf) with the Mac app's keys

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B1d: launch flow, `--screenshot`, `install.sh`

### Task 6 (B1d): `Launcher`, `Recette`, the launch states and the screenshot walk

**Files:**
- Create: `apps/linux/src/launcher.rs`, `apps/linux/src/recette.rs`
- Modify: `apps/linux/src/lib.rs` (modules; `parse_args`; logging; `recette::set_dir`; `ressac_grab_window` in `mod shim`)
- Modify: `apps/linux/qml/Main.qml`
- Test: `apps/linux/tests/launch.rs` (not counted)

**Interfaces:**
- Consumes: `ressac_ui::launch::*` (Task 2), `QSettingsStore` (Task 5, read-only here), `Store::probe` / `open` / `open_existing`, `Paths::from_env`, `logging::init`, the shim.
- Produces: `Launcher`, `Recette`, `Session` / `with_session`, the walk, the qt_thread convention (header).

`lib.rs` `run()`, before any Qt call: `parse_args(std::env::args_os().skip(1), std::env::var_os("RESSAC_DB").as_deref())`; `Err(m)` → `eprintln!("{m}")`, exit 2; a screenshot directory is created (`create_dir_all`; failure → `ressac-kde: cannot create <dir>: <error>`, exit 2) and handed to `recette::set_dir`; then logging (header); then Task 4's sequence.

`launcher.rs`: struct `LauncherRust { phase: LaunchPhase, db: Option<PathBuf>, launching: bool }` (`Default`: `Starting`, `None`, `false`); the bridge's `Phase` qenum and `fn phase(&self) -> Phase` mapping every `LaunchPhase` variant; `fn message(&self) -> QString`, `fn database_path(&self) -> QString`; a private `set_phase` emitting `phase_changed` and `message_changed`. `launch()`: returns unless `Starting` and not `launching`; resolves (`Paths::from_env`, `HOME` from the environment when absolute, `QSettingsStore::default_file().get(keys::DATABASE_PATH)`), sets `db` (emits `database_path_changed`), probes, then follows `LaunchStep` (header decision); `Migrate` sets `launching`, `Migrating`, and spawns the worker (`Store::open(&db).map(drop).and_then(|()| Store::probe(&db))`) whose result comes back through `apply_migration(outcome)`, which clears `launching` and follows `after_migration`. `Open` = `Store::open_existing(&db)` → `install_session(Session { db, store })` and `Ready`, or `cannot_open`. `relaunch()`: returns unless `offers_default_database()` and not `launching`; drops any session, `Starting`, `launch()`. `Session` and `with_session` carry `#[expect(dead_code, reason = "read by B2c's models")]` until B2c reads them (the attribute then fails the build and must go).

`recette.rs`: `static DIR: OnceLock<PathBuf>`; `pub(crate) fn set_dir(dir: PathBuf)`; `RecetteRust { active: bool }` (`Default` = `DIR.get().is_some()`); `grab(name)` → `false` when inactive, else `shim::ressac_grab_window(<dir>/<name>.png)`, logging `ressac-kde: screenshot <path>` (or `… failed`) on stderr.

`Main.qml` (strings exact, inventory §1.1):
- `import org.w3cdotorg.ressac`; `Component.onCompleted: { Launcher.launch(); root.startWalkIfSettled() }`.
- One `Kirigami.Page` (no global toolbar) whose content follows `Launcher.phase`: `Starting` → running `QQC2.BusyIndicator`; `Migrating` → busy indicator and `QQC2.Label { text: Launcher.message }` ("Updating the database…"); `TooNew` → `Kirigami.Heading { level: 3; text: Launcher.message }` and `QQC2.Button { text: "Use Default Database" }`; `Failed` → `QQC2.Label { text: Launcher.message; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }` and a row of `"Use Default Database"` and `"Quit Ressac"` (`Qt.quit()`); `Ready` → the placeholder of the header. "Use Default Database" calls `function useDefaultDatabase() { AppSettings.databasePath = ""; Launcher.relaunch() }`.
- The walk (the algorithm, fixed here):

```qml
    // --screenshot (spec §8.2): grab the settled launch screen, then each walk step (ready only), then quit.
    readonly property var walkSteps: []          // B2 onwards: { name: "02-…", enter: function () { … } }
    property int walkIndex: -2                   // -2 not started, -1 launch screen, 0… walkSteps
    function settled() { return Launcher.phase === Launcher.Ready || Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed }
    function slug() { return Launcher.phase === Launcher.Ready ? "ready" : Launcher.phase === Launcher.TooNew ? "too-new" : "failed" }
    function startWalkIfSettled() { if (Recette.active && walkIndex === -2 && settled()) { walkIndex = -1; walkTimer.start() } }
    Connections { target: Launcher; function onPhaseChanged() { root.startWalkIfSettled() } }
    Timer {
        id: walkTimer; interval: 400
        onTriggered: {
            const name = root.walkIndex < 0 ? "01-" + root.slug() : root.walkSteps[root.walkIndex].name
            if (!Recette.grab(name)) { Qt.exit(1); return }
            root.walkIndex += 1
            if (Launcher.phase !== Launcher.Ready || root.walkIndex >= root.walkSteps.length) { Qt.exit(0); return }
            root.walkSteps[root.walkIndex].enter()
            walkTimer.start()
        }
    }
    Timer { interval: 60000; running: Recette.active; onTriggered: { console.error("ressac-kde: the screenshot walk timed out"); Qt.exit(1) } }
```

- [ ] **Step 1: Write the failing tests** `apps/linux/tests/launch.rs`:

```rust
//! The launch states, offscreen, through `--screenshot` (spec §7.2, §8.2; inventory §1.1).
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use ressac_core::store::{SchemaState, Store};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/contract.sqlite");

struct Run { _home: tempfile::TempDir, home: PathBuf, shots: PathBuf }

impl Run {
    fn new() -> Run { /* tempdir; shots = home/shots */ }
    /// `ressac-kde --screenshot <shots>` with RESSAC_DB = `db` (None: unset), offscreen, HOME and
    /// XDG_{CONFIG,DATA,STATE,CACHE}_HOME under the temp dir, RUST_LOG unset.
    fn launch(&self, db: Option<&Path>) -> Output { /* … */ }
    fn copy_fixture(&self) -> PathBuf { /* copies FIXTURE to home/r.sqlite */ }
}

/// Not blank: at least the window's minimum size, and more than 1000 pixels unlike the top-left one.
fn assert_drawn(png: &Path) { /* image::open(png).to_rgba8() … */ }

/// No QML error in the run's stderr.
fn assert_clean(out: &Output) {
    let err = String::from_utf8_lossy(&out.stderr);
    for bad in ["failed to load component", "is not installed", "ReferenceError", "TypeError", "Unable to assign"] {
        assert!(!err.contains(bad), "{bad}: {err}");
    }
}

fn shots(run: &Run) -> Vec<String> { /* sorted file names in run.shots */ }

#[test]
fn a_current_database_opens_ready() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-ready.png"]);
    assert_drawn(&run.shots.join("01-ready.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current);
    assert!(run.home.join("state/ressac/logs").is_dir());              // logging::init under XDG_STATE_HOME
    assert!(!run.home.join("config/ressac/ressac-kde.conf").exists()); // the launch only reads settings
}

#[test]
fn a_missing_or_empty_database_is_created_before_opening() {
    let run = Run::new();
    let missing = run.home.join("data/new/ressac.sqlite");
    let empty = run.home.join("empty.sqlite");
    std::fs::write(&empty, b"").unwrap();
    for db in [missing, empty] {
        let _ = std::fs::remove_dir_all(&run.shots);
        let out = run.launch(Some(&db));
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(shots(&run), ["01-ready.png"], "{db:?}");
        assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current, "{db:?}");
    }
}

#[test]
fn a_newer_database_is_refused_and_left_alone() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db).unwrap().execute("UPDATE schema_meta SET version = 99", []).unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-too-new.png"]);
    assert_drawn(&run.shots.join("01-too-new.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Newer(99));
}

#[test]
fn a_file_that_is_not_a_database_fails_and_is_left_alone() {
    let run = Run::new();
    let db = run.home.join("notes.sqlite");
    let words = "not a database, only words, long enough to fill the first page of a SQLite header";
    std::fs::write(&db, words).unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(shots(&run), ["01-failed.png"]);
    assert_drawn(&run.shots.join("01-failed.png"));
    assert_eq!(std::fs::read_to_string(&db).unwrap(), words);
}

#[test]
fn the_screenshot_flag_needs_ressac_db() {
    let run = Run::new();
    let out = run.launch(None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&out.stderr), format!("{}\n", ressac_ui::launch::SCREENSHOT_NEEDS_DB));
    assert!(!run.shots.exists());
}
```

- [ ] **Step 2: Run them to verify they fail**

`Run::launch` runs the binary under `timeout 90` (`Command::new("timeout").arg("90").arg(env!("CARGO_BIN_EXE_ressac-kde"))…`), so a hung app fails its test instead of hanging the gate.

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test launch the_screenshot_flag_needs_ressac_db 2>&1 | grep -E "panicked|test result"'`
Expected: FAIL after about 90 s, `left: Some(124)` (the B1c app ignores its arguments and runs until `timeout` stops it). The four other tests fail the same way; run them first in Step 4.

- [ ] **Step 3: Implement** `launcher.rs`, `recette.rs`, the `lib.rs` and `Main.qml` changes as above.

- [ ] **Step 4: Run them to verify they pass, and look at one screen**

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"; cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1'`
Expected: every `test result: ok.` (5 in `launch`); clippy `Finished`.
Run (a copy, shots in the bind mount, removed afterwards):
```bash
apps/linux/dev/dev.sh sh 'h=$(mktemp -d); cp tests/fixtures/contract.sqlite $h/r.sqlite; HOME=$h XDG_CONFIG_HOME=$h/c XDG_CACHE_HOME=$h/k XDG_STATE_HOME=$h/s RESSAC_DB=$h/r.sqlite "$CARGO_TARGET_DIR/debug/ressac-kde" --screenshot /work/ressac/target-shots; echo "exit=$?"'
```
Expected: `exit=0`; open `target-shots/01-ready.png` on the Mac: Breeze style, "Ressac" and the database path centred. Then `rm -rf target-shots`.

- [ ] **Step 5: Live check over VNC (spec §8.2)**

Run:
```bash
apps/linux/dev/dev.sh sh 'h=$(mktemp -d); cp tests/fixtures/contract.sqlite $h/r.sqlite; HOME=$h XDG_CACHE_HOME=$h/k XDG_CONFIG_HOME=$h/c XDG_STATE_HOME=$h/s RESSAC_DB=$h/r.sqlite QT_QPA_PLATFORM=vnc:size=1100x720:port=5900 setsid nohup timeout 20 "$CARGO_TARGET_DIR/debug/ressac-kde" >/dev/null 2>&1 < /dev/null &'
sleep 5; nc -w 3 ressac-fedora.orb.local 5900 < /dev/null | head -c 12; echo
```
Expected: `RFB 003.003` (Clément may connect a VNC viewer to `ressac-fedora.orb.local:5900` instead).

- [ ] **Step 6: Commit**

```bash
git add apps/linux/src/launcher.rs apps/linux/src/recette.rs apps/linux/src/lib.rs apps/linux/qml/Main.qml apps/linux/tests/launch.rs
git commit -m "ressac-kde: launch states (migrate, too new, failed), hidden --screenshot walk

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 7 (B1d): `apps/linux/install.sh`

**Files:**
- Create: `apps/linux/install.sh` (`chmod +x`)

**Interfaces:**
- Consumes: `apps/linux/dev/packages.txt` (A4), the binaries `ressac` and `ressac-kde`, `apps/macos/Ressac/Resources/AppIcon.svg`.
- Produces: the installed files of the header; usage `usage: apps/linux/install.sh [--software-rendering] | --uninstall` (anything else: that line on stderr, exit 2).

Behaviour (`set -euo pipefail`; repo root from the script's directory; `die()` = `install.sh: <reason>` on stderr, exit 2):
1. Guards: `uname -s` is `Linux` (`run me on Linux (Fedora)`); not root (`run me as your user, not root`); `HOME` absolute (`HOME is not an absolute path`); `$HOME/.local/bin` and `$DATA` free of `"`, `` ` ``, `$`, `\` (`<path> holds a character a .desktop Exec line cannot carry`).
2. Packages (install only): every name of `packages.txt` (`sed 's/#.*//' | xargs -n1`) except the container-only ones `rustfmt clippy nodejs cmake shadow-utils util-linux-core procps-ng findutils diffutils tar gzip git-core`, checked with `rpm -q --quiet`; any missing → `install.sh: missing Fedora packages; install them with:` then `  sudo dnf install <names…>`, exit 1. The script never runs `dnf` itself.
3. Build: `cargo build --release --locked -p ressac -p ressac-kde` from the repo root; binaries from `${CARGO_TARGET_DIR:-$repo/target}/release/`.
4. Install with `install -Dm755` (binaries) / `install -Dm644` (icon, desktop file), printing `installed <path>` for each:
   - `$HOME/.local/bin/ressac`, `$HOME/.local/bin/ressac-kde`;
   - `$DATA/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg` from `AppIcon.svg`;
   - `$DATA/applications/org.w3cdotorg.ressac.desktop`, exactly (validated with `desktop-file-validate`, both variants):

```ini
[Desktop Entry]
Type=Application
Name=Ressac
GenericName=Feed Reader
Comment=Tidy your RSS subscriptions, then read them calmly
Exec="/home/willow/.local/bin/ressac-kde"
Icon=org.w3cdotorg.ressac
Terminal=false
Categories=Network;Feed;News;Qt;KDE;
StartupWMClass=ressac-kde
```
   (`Exec` holds the real absolute path; with `--software-rendering`: `Exec=env QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu "<path>"`, the switch spec §8.4 leaves to the VM checkpoint.)
5. When `$HOME/.local/bin` is not in `PATH`: `install.sh: <dir> is not on PATH; add it to use ressac in Konsole.` Last line: `install.sh: installed ressac and ressac-kde in <dir>`.
6. `--uninstall`: `rm -f` of exactly those four paths, `removed <path>` for each that existed; never the database, `ressac-kde.conf`, logs or caches. Last line: `install.sh: uninstalled (database, settings and logs kept)`.

- [ ] **Step 1: Write the failing checks**

```bash
apps/linux/install.sh --nope; echo "exit=$?"
```
Expected now: `No such file or directory`, `exit=127`.

- [ ] **Step 2: Implement** `apps/linux/install.sh` as above.

- [ ] **Step 3: Lint, refuse, install, uninstall (in the container, a fake HOME)**

Run: `shellcheck apps/linux/install.sh` → no output.
Run: `apps/linux/install.sh; echo "exit=$?"` (Mac) → `install.sh: run me on Linux (Fedora)`, `exit=2`.
Run:
```bash
apps/linux/dev/dev.sh sh 'h=$(mktemp -d); mkdir -p $h/.local/share/ressac && echo keep > $h/.local/share/ressac/ressac.sqlite
env -u XDG_DATA_HOME HOME=$h apps/linux/install.sh 2>&1 | tail -3
grep -c "^Exec=\"$h/.local/bin/ressac-kde\"$" $h/.local/share/applications/org.w3cdotorg.ressac.desktop
test -x $h/.local/bin/ressac-kde && test -x $h/.local/bin/ressac && test -f $h/.local/share/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg && echo files-ok
env -u XDG_DATA_HOME HOME=$h apps/linux/install.sh --software-rendering >/dev/null 2>&1; grep -c "^Exec=env QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu " $h/.local/share/applications/org.w3cdotorg.ressac.desktop
env -u XDG_DATA_HOME HOME=$h apps/linux/install.sh --uninstall | tail -1
find $h -type f | sort'
```
Expected: the install ends with `install.sh: installed ressac and ressac-kde in <h>/.local/bin` (after the `not on PATH` note); `1`; `files-ok`; `1`; `install.sh: uninstalled (database, settings and logs kept)`; `find` lists only `<h>/.local/share/ressac/ressac.sqlite`.

- [ ] **Step 4: Gate, then commit.** The full gate of the header (B1d and B1 end here). Ledger rows B1a–B1d: commits, test counts (`ressac-ui`, `ressac-kde` lib and `launch`), the Linux gate's wall time with `ressac-kde`.

```bash
git add apps/linux/install.sh
git commit -m "Linux: install.sh (package check, release build, ~/.local, desktop entry, --uninstall)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
