# Phase B7: the Sea Chart window (B7a–B7c)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl+Shift+M opens a Sea Chart window that shows the Mac's **unchanged** chart page (`index.html`, `chart-core.js`, `chart-draw.js`, `d3.min.js`, `chart.css`, fonts) in a `WebEngineView`, fed in-process with `Store::chart_snapshot` (version 1), refreshed on open, 2 s after a change (trailing), every 5 minutes and once more after a request made during a survey, answering the page's `openFeed`, `openURL` and `prefs` messages through a `window.webkit.messageHandlers.ressac` adapter over `QWebChannel`, with the prefs in `QSettings` and Plasma's instant animations acting as Reduce Motion.

**Runs after:** B6e (`Main.qml`'s `openExternally`, the focused-window grab, `assert_drawn_min`, the `Binding`/`Connections` menu wiring), B3 (`ressac:` scheme, `ressac_scheme_serve` in `reader.rs`, `article::js_string`, `sanitize::is_web_url`, `MainMenu.qml`'s disabled `seaChart` action), B2 (`Live.chartOnScreen`, `ReadingPage.reloadAll()`, `SourcesModel.selectKey`, `with_session`, `Session.db`), B1 (`settings::{ChartPrefs, ChartView, ChartInk, Settings}`, `QSettingsStore`, `AppSettings.chart*`, `Recette`, `tests/common`). Master plan row B7; Interfaces (`ressac:` scheme, settings keys `chart.view`, `chart.ink`, `chart.animate`); Global Constraints (Runs: `data_version` polled while the Sea Chart window is open and not minimised; Gates). Spec §7.8 (and §7.1 live data, §8.3 "one WebEngine test that loads the page with `apps/macos/ChartTests/fixture.json` and checks it drew"). Inventory §6.1–§6.5, §10.1 ChartModelTests (20), ChartBridgeTests (13), §10.2 (the Node tests run unchanged).

**Architecture:** `ressac_ui::chart` (new, Qt-free, tested on the Mac) holds the refresh rules as a state machine over `Instant`s (`ChartModel`), the version check, the in-process survey (its own query-only connection on a thread), the page bridge's message validation (`handle`), the prefs argument and save rule, the navigation rule, the page's document queue (`ChartPage`: `Chart.load` first, `Chart.update` after, scripts built with `js_string`), the list of served files and the adapter script. `ressac-kde` adds a `Chart` QML singleton that only translates, serves the Mac's files byte for byte under `ressac:chart/` through B3's scheme handler, links `WebChannel`, and remembers the window size through `AppSettings`. QML: `SeaChartWindow.qml` (a separate top-level window with the `WebEngineView`, the two injected scripts and a `QtObject` registered on a `WebChannel`), opened from the View menu.

**Tech Stack:** Rust 2024 / 1.99; `ressac-core` (`Store::{chart_snapshot, open_existing_query_only}`), `serde_json` (pinned, newly used by `ressac-ui`), `chrono`; cxx-qt 0.10.0; Qt 6.11.2 WebEngineQuick (`WebEngineView`, `WebEngine.script()`, `userScripts`, `runJavaScript`, `navigationRequested`), WebChannel (`QtWebChannel` QML module, `qrc:/qtwebchannel/qwebchannel.js`); Kirigami 6.31 (`Units.longDuration`); Fedora 44 container.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Three sub-phases** (5-file cap): **B7a** `ressac_ui::chart` and the window-size keys (4 files), **B7b** the bridge, the scheme branch, `WebChannel`, `AppSettings` (5), **B7c** the window and its wiring (3). Ledger rows B7a–B7c.
2. **No Linux copy of `index.html`.** The page is served unchanged at `ressac:chart/index.html` (relative `chart.css`, scripts and `fonts/…` resolve under it), and the adapter plus `qwebchannel.js` are injected as `WebEngineScript`s in the main world at `DocumentReady`: the page's CSP (`script-src 'self' file:`) still refuses inline scripts and `eval` (validated) but does not apply to injected user scripts. The JS files are never forked.
3. **The survey is in-process** (`Store::chart_snapshot` → `serde_json::to_string`, the same JSON as `ressac chart --json`) on a thread of its own with a query-only connection: not the GUI thread (a big database), not the short lane (a network extraction can sit there for seconds). The Mac's capture tests (prefix, 300 KB output, stderr last line) become `survey_reads_the_store_like_the_cli`; the version check and "ressac printed a chart this app cannot read." stay for a malformed document.
4. **Timers:** a QML `Timer` of 250 ms runs while the window is open and calls `ChartModel::tick(now)`; debounce and resurvey are exact in the model and within 250 ms on screen.
5. **The change signal is B2's reload funnel:** `ReadingPage.reloadAll()` (own writes, run progress and ends, `data_version` moves, activation) also calls `Chart.noteChange()`. The extra debounced survey after an activation is harmless. The Mac's `changeTick` tests become the `chart-live` walk.
6. **Reduce Motion = `Kirigami.Units.longDuration <= 1`**, not `== 0` (spec §7.8): with `AnimationDurationFactor=0` in `kdeglobals` and the `org.kde.desktop` style, Kirigami reports `longDuration` 1 and `shortDuration` 0 (validated).
7. **Navigation lock:** a load of `about:blank` never reaches `navigationRequested` and commits (validated), so a load that **succeeds** on any URL other than the page sends the view back to the page and the documents are sent again; refused loads (link clicks, `location.href` to a chart file) leave the page in place.
8. **Window size remembered** under two new keys `chart.window.width` / `chart.window.height` (default 1100×720, minimum 640×360) through `AppSettings`, the only writer of the settings file. The Mac uses AppKit's frame autosave, not a listed defaults key.
9. **Prefs are saved through `AppSettings.saveChartPrefs`** (same rule: view and ink always, animate not under Reduce Motion); the `Chart` object reads the stored prefs at each page load (`Settings::load` on `QSettingsStore::default_file()`), as the Mac reads its defaults at each load.
10. **Recette hooks:** `Chart.showFile(url)` hands a document file to the page as a finished survey would (the spec's WebEngine test draws `apps/macos/ChartTests/fixture.json`), `Chart.probeScript` returns the Mac test's canvas sample with the font, ink, view and location facts on one line; walks `chart` and `chart-live` (B4's `RESSAC_RECETTE_WALK`).
11. **"Not on screen"** is `!visible || visibility === Window.Minimized` written to `Live.chartOnScreen` (spec §7.1; Wayland gives no occlusion).
12. **openFeed** selects the feed and brings the main window back (`show()`, `showNormal()` when minimised, `raise()`, `requestActivate()`): closing the main window only hides it while the chart window is open.

## Validated in a throwaway container (`ressac-b67-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A C++ Qt 6.11.2 program (`QApplication`, `QtWebEngineQuick::initialize()`, the `ressac` scheme registered with `Syntax::Path` and no flags before it, a `QWebEngineUrlSchemeHandler` on the default profile serving the Mac's `Resources/Chart` files, a QML `WebEngineView`), offscreen, as user `ressac`, all checks passed for the three page URLs `qrc:/chart/index.html`, `ressac:chart/index.html` and `ressac:/chart/index.html`:
- **The unchanged page loads and draws.** Relative URLs resolve under `ressac:chart/` (requests `ressac:chart/chart.css`, `…/d3.min.js`, `…/chart-core.js`, `…/chart-draw.js`, `…/fonts/*.woff2`, initiator `ressac:`); `location.origin` is `ressac:`. After `runJavaScript("Chart.load(" + <JSON string literal of fixture.json> + ", {\"view\":\"themes\",\"ink\":\"nuit\",\"animate\":false})")` in the main world (QML `runJavaScript(script, callback)` default world): 817 distinct colours in the Mac test's canvas sample, the 8 faces `loaded`, `dataset.ink` `nuit`, `.bar` `position: fixed`. `Chart.update(<3 feeds>)` redrew (329 colours, another fingerprint). `Chart.load('{"version":2}', {})` throws `TypeError: unsupported chart version: 2` in the page (so the app checks the version before handing a document in).
- **The adapter over `QWebChannel`.** Two `WebEngine.script()` objects inserted in `userScripts` (`sourceUrl: "qrc:/qtwebchannel/qwebchannel.js"`, then `sourceCode: <adapter>`; both `injectionPoint: WebEngineScript.DocumentReady`, `worldId: WebEngineScript.MainWorld`) with `webChannel: channel` on the view and `WebChannel { registeredObjects: [relay] }`, `relay` = `QtObject { WebChannel.id: "ressac"; function post(text) { … } }`: in the page `typeof QWebChannel` is `function`, `qt.webChannelTransport` an object, the channel came up with object `ressac` before the first `Chart.load`; `window.webkit` (defined with `Object.defineProperty`, frozen) is an object. The page's own toolbar click on "From the harbour" posted `{"type":"prefs","view":"harbour","ink":"nuit","animate":false}`; `postMessage({type: 'openFeed', id: 12})` arrived as `{"type":"openFeed","id":12}`.
- **CSP:** an inline `<script>` appended by script was refused ("Refused to execute inline script because it violates … "script-src 'self' file:"") and `eval('1+1')` too ("Refused to evaluate a string as JavaScript because 'unsafe-eval' …"), while the injected scripts ran: no Linux `index.html`.
- **Navigation:** clicked links to `ressac:chart/LICENSE-d3.txt` and `https://example.invalid/` arrived as `navigationRequested` (`LinkClickedNavigation`, main frame) and `reject()` kept the page; `location.href = 'ressac:chart/chart.css'` likewise; `window.open(…)` returned `null` with `settings.javascriptCanOpenWindows: false`; **`about:blank` (clicked or assigned) never reached `navigationRequested` and committed**; setting `url` back to the page on that `LoadSucceededStatus` reloaded it and a new `Chart.load` drew again (`typeof Chart` `object`, `location.href` the page).
- `matchMedia("(prefers-reduced-motion: reduce)")` is `false` in the container (Reduce Motion reaches the page through the prefs only); `WebEngineView.grabToImage` never called back offscreen (shots go through `grabWindow`, B6c's focused-window grab); a second top-level window is grabbed when it holds the focus.
- **Kirigami units:** with `$XDG_CONFIG_HOME/kdeglobals` `[KDE] AnimationDurationFactor=0` and `QT_QUICK_CONTROLS_STYLE=org.kde.desktop`, `Kirigami.Units.longDuration` is 1 and `shortDuration` 0; without the file 200 and 100.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-ui/Cargo.toml` | B7a T1 | `serde_json.workspace = true` |
| `crates/ressac-ui/src/lib.rs` | B7a T1 | `pub mod chart;` |
| `crates/ressac-ui/src/chart.rs` (new) | B7a T1 | texts, files, adapter, probe, `check_document`, `survey`, `ChartModel`, `handle`, prefs, navigation, `ChartPage`, scripts |
| `crates/ressac-ui/src/settings.rs` | B7a T1 | `chart.window.width` / `chart.window.height` |
| `apps/linux/src/chart.rs` (new) | B7b T2 | QML singleton `Chart`, the embedded files (`include_bytes!`), survey threads |
| `apps/linux/src/lib.rs` | B7b T2 | `mod chart;` |
| `apps/linux/src/reader.rs` | B7b T2 | `ressac_scheme_serve` serves `ressac:chart/<file>` |
| `apps/linux/build.rs` | B7b T2 | `.qt_module("WebChannel")` |
| `apps/linux/src/settings.rs` | B7b T2 | `AppSettings.chartWindowWidth/Height`, `saveChartPrefs` |
| `apps/linux/qml/SeaChartWindow.qml` (new) | B7c T3 | the window, the view, the scripts, the channel, states, timer |
| `apps/linux/qml/Main.qml` | B7c T3 | `openChart()`, the loader, `openFeed` / `openUrl` handling, menu wiring, walks |
| `apps/linux/qml/ReadingPage.qml` | B7c T3 | `reloadAll()` also calls `Chart.noteChange()` |
| tests (not counted) | | `apps/linux/tests/chart.rs` (new) |

Files per sub-phase: B7a 4, B7b 5, B7c 3.

## Interfaces produced (B8 onwards rely on these names)

**`ressac_ui::chart`** (Task 1):

```rust
pub const PREFIX: &str = "ressac:chart/";
pub const PAGE_URL: &str = "ressac:chart/index.html";
pub const SUPPORTED_VERSION: u64 = 1;
pub const DEBOUNCE: Duration = Duration::from_secs(2);
pub const RESURVEY: Duration = Duration::from_secs(300);
pub const TICK_MS: i32 = 250;
pub const WINDOW_TITLE: &str = "Sea Chart";
pub const NOT_OPEN: &str = "The database is not open. The main window says why.";
pub const NOT_DRAWN: &str = "The chart could not be drawn";
pub const RETRY: &str = "Retry";
pub const UNREADABLE: &str = "ressac printed a chart this app cannot read.";
pub fn unsupported(version: u64) -> String;   // "This build draws chart version 1, but ressac produced version <n>."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartFile { pub path: &'static str, pub mime: &'static str }
/// index.html, chart.css, d3.min.js, chart-core.js, chart-draw.js, fonts/ (8 woff2), as in apps/macos/Ressac/Resources/Chart.
pub const FILES: [ChartFile; 13];
pub fn file(path: &str) -> Option<&'static ChartFile>;     // exact match on the part after PREFIX
pub const ADAPTER_SCRIPT: &str = /* exact copy, Task 1 */;
pub const PROBE_SCRIPT: &str = /* exact copy, Task 1 */;   // recette: "colours=<n> print=<p> faces=<n> ink=<ink> byTheme=<b> home=<typeof Chart> <href>"
pub fn check_document(json: &str) -> Result<(), String>;    // UNREADABLE, unsupported(n)
/// The chart as `ressac chart --json` prints it, read with its own query-only connection.
pub fn survey(db: &Path, now: DateTime<Utc>) -> Result<String, String>;
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase { #[default] Loading, Ready(String), Failed(String) }
impl Phase { pub fn as_str(&self) -> &'static str; }       // "loading" "ready" "failed"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Survey(pub u64);
#[derive(Debug, Clone)]
pub struct ChartModel { /* phase, last_error, open, running: Option<Survey>, next ticket, rerun, scheduled, next_survey, debounce, resurvey */ }
impl Default for ChartModel { /* DEBOUNCE, RESURVEY */ }
impl ChartModel {
    pub fn with_timing(debounce: Duration, resurvey: Duration) -> ChartModel;
    pub fn open(&mut self, now: Instant) -> Option<Survey>;   // closed → open, next survey at now + resurvey, refresh(); already open → None
    pub fn close(&mut self);                                  // forget the running survey, the schedule and the rerun; phase Loading, no error
    pub fn refresh(&mut self) -> Option<Survey>;              // open and idle → a new survey; during one → rerun once after it; closed → None
    pub fn note_change(&mut self, now: Instant);              // open and nothing scheduled → a survey at now + debounce
    pub fn tick(&mut self, now: Instant) -> Option<Survey>;   // the scheduled time or the resurvey time reached → refresh()
    pub fn finished(&mut self, survey: Survey, result: Result<String, String>) -> Option<Survey>; // a stale survey → None, ignored
    /// Recette (`Chart.showFile`): a document handed in as a successful survey would be, while open.
    pub fn show_document(&mut self, json: String) -> Result<(), String>;   // check_document; closed → Err(NOT_OPEN)
    pub fn phase(&self) -> &Phase;
    pub fn last_error(&self) -> Option<&str>;                 // the last survey's failure, kept while a chart is shown
    pub fn is_open(&self) -> bool;
}
#[derive(Debug, Clone, PartialEq)]
pub enum ChartAction { OpenFeed(FeedId), OpenUrl(String), Prefs(ChartPrefs) }
/// A page message (the adapter's JSON text). Only the three known types with well-formed fields.
pub fn handle(message: &str) -> Option<ChartAction>;
pub fn page_prefs(prefs: &ChartPrefs, reduce_motion: bool) -> String;  // {"view":"harbour","ink":"bleu","animate":true}
pub fn save_prefs(settings: &mut Settings, store: &mut dyn SettingsStore, prefs: &ChartPrefs, reduce_motion: bool);
pub fn allows_navigation(url: &str, main_frame: bool) -> bool;          // main frame and exactly PAGE_URL
pub fn load_script(json: &str, prefs: &str) -> String;                  // "Chart.load(" + js_string(json) + ", " + prefs + ")"
pub fn update_script(json: &str) -> String;                             // "Chart.update(" + js_string(json) + ")"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageStep { Run(String), GoHome, Nothing }
#[derive(Debug, Clone, Default)]
pub struct ChartPage { /* latest, shown, loaded */ }
impl ChartPage {
    pub fn show(&mut self, json: &str, prefs: &str) -> Option<String>;  // the script when the page is loaded and `json` is not the one shown
    pub fn load_started(&mut self);                                     // not loaded, nothing shown
    pub fn load_finished(&mut self, url: &str, succeeded: bool, prefs: &str) -> PageStep;
}
```

**`ressac_ui::settings`** additions (Task 1):

```rust
pub mod keys { pub const CHART_WINDOW_WIDTH: &str = "chart.window.width"; pub const CHART_WINDOW_HEIGHT: &str = "chart.window.height"; }
pub const CHART_WINDOW_DEFAULT: (u32, u32) = (1100, 720);
pub const CHART_WINDOW_MIN: (u32, u32) = (640, 360);
pub struct Settings { /* …, */ pub chart_window: (u32, u32) }   // load: missing or unparsable → default, raised to the minimum
impl Settings { pub fn set_chart_window(&mut self, store: &mut dyn SettingsStore, width: u32, height: u32) -> bool; } // raised to the minimum; both keys written when either changed
```

**`ressac-kde`** (Tasks 2–3):

- **`Chart`** (QML singleton, `impl cxx_qt::Threading`): properties (`READ`, `NOTIFY = state_changed`) `phase: QString`, `message: QString` (the failure shown with `NOT_DRAWN`), `refreshError: QString` (`""` = none; shown over a chart), `open: bool`, `surveys: i32` (successful surveys since start; walks read it); `reduceMotion: bool` (`READ`/`WRITE`/`NOTIFY`); `CONSTANT`: `pageUrl`, `adapterScript`, `probeScript`, `tickInterval` (250), `windowTitle`, `notOpenText`, `notDrawnText`, `retryText`. Invokables `openWindow()`, `closeWindow()`, `noteChange()`, `tick()`, `retry()`, `pageLoadStarted()`, `pageLoaded(url: QString, succeeded: bool)`, `allowsNavigation(url: QString, mainFrame: bool) -> bool`, `receive(message: QString)`, `showFile(url: QString) -> bool`. Signals `stateChanged()`, `runScript(script: QString)`, `goHome()`, `openFeed(key: QString)` (`"feed:<id>"`), `openUrl(url: QString)`, `prefsReported(view: QString, ink: QString, animate: bool)`.
- **`AppSettings`** additions: `chartWindowWidth: i32`, `chartWindowHeight: i32` (`READ`/`WRITE`/`NOTIFY`, through `set_chart_window`); invokable `saveChartPrefs(view: QString, ink: QString, animate: bool, reduceMotion: bool)` (through `chart::save_prefs`, emitting the changed `chart*` notifies).
- **Scheme:** `ressac_scheme_serve` answers `ressac:chart/<path>` for the 13 `FILES` with the Mac's bytes and their mime; anything else under `PREFIX` is refused.
- **QML:** `SeaChartWindow.qml` (`property var chrome`, `function probe(tag)`, `function runPage(script)`); `Main.qml` `function openChart()`, `function chartClosed()`, `Loader { id: chartLoader }`, `readonly property var chartSteps`, `readonly property var chartLiveSteps`.

**Consumed:** core `Store::{chart_snapshot, open_existing_query_only}`; B6 `Main.qml`'s `openExternally`, `Recette.grab`'s focused-window rule, `tests/common::assert_drawn_min`; B4 `Recette.walk`, `steps` chooser; B3 `article::js_string`, `sanitize::is_web_url`, `ressac_scheme_serve`, `MainMenu.qml`'s `seaChart` action alias; B2 `Live.chartOnScreen`, `ReadingPage.reloadAll()`, `SourcesModel.selectKey`, `EntriesModel.{select, toggleStarred}`, `with_session`, `Session.db`; B1 `settings::{ChartPrefs, ChartView, ChartInk, Settings, SettingsStore}`, `QSettingsStore::default_file`, `Launcher.phase`, `AppSettings.chart{View,Ink,Animate}`.

**Decisions this phase pins:**

- **One `Chart` for the app, one window at a time.** `openWindow()` when already open does nothing new (the QML raises the window). Closing the window calls `closeWindow()` (everything stops; a survey under way is forgotten) and destroys the window (`Loader.active = false` on the next turn), so the view and its page go too; reopening draws afresh.
- **Survey threads:** `std::thread::spawn` per survey with `Session.db`; the result comes back through `qt_thread().queue` with its `Survey` ticket, and the model drops stale ones. No session (launch not ready) → `finished(survey, Err(NOT_OPEN))`.
- **Documents:** after each successful survey (or `showFile`), `ChartPage::show(json, prefs)`; a returned script goes out as `runScript`, which the window runs with `runJavaScript(script)` (main world). Each page load: `pageLoadStarted()` on `LoadStartedStatus`, `pageLoaded(url, succeeded)` on `LoadSucceededStatus` / `LoadFailedStatus`; `GoHome` → `goHome()` → the view's `url = Chart.pageUrl`. A crashed render process → `web.reload()`.
- **Messages:** the adapter posts `JSON.stringify(message)`; `relay.post(text)` → `Chart.receive(text)` → `handle` → `openFeed` (QML: `SourcesModel.selectKey(key)` and the main window to front), `openUrl` (QML: `root.openExternally(url)`), `prefsReported` (QML: `AppSettings.saveChartPrefs(view, ink, animate, Chart.reduceMotion)`).
- **Launch states in the window:** `Launcher.Ready` → the chart (busy indicator while `phase === "loading"`, the failure view with Retry when `"failed"`, the bottom capsule "message + Retry" when `refreshError` over a chart); `TooNew` / `Failed` → `NOT_OPEN` (secondary text); otherwise a busy indicator. A launch that becomes ready while the window is open (Use Default Database) → `closeWindow(); openWindow()`.

**Step 0:** not triggered. `settings.rs` (ui 738 lines, kde 331), `reader.rs` and `Main.qml` / `ReadingPage.qml` only gain items.

**Gate (end of each sub-phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0" (28 tests, unchanged)
git diff --exit-code "$(git merge-base HEAD main)" -- apps/macos/Ressac/Resources/Chart apps/macos/ChartTests && echo chart-unchanged   # "chart-unchanged"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

## Global Constraints

- **The chart page is the Mac's, unchanged:** no edit under `apps/macos/Ressac/Resources/Chart` or `apps/macos/ChartTests`; the Node tests pass as they are.
- **Runs:** `data_version` polled every 3 s while a Ressac window is active or the Sea Chart window is open and not minimised.
- **Settings keys:** `chart.view` (`harbour` default, `themes`), `chart.ink` (`bleu` default, `nuit`; `pop`, `pirate` and anything else read back as `bleu`), `chart.animate` (true); `AppSettings` is the only writer.
- **Shortcut:** Ctrl+Shift+M Sea Chart (B3's View menu action).
- **Language:** the inventory's chart strings (§6.1).
- **Never:** the real database; the network in tests (`https://example.invalid/x` only reaches the recette's `open:` log); push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **A burst of writes while the chart is open** (a fetch landing hundreds of entries, `ressac fetch` in Konsole) → one survey per 2-second window, never one per write; a change during a survey reruns once after it. Tests: Task 1 `debounces_bursts`, `a_change_during_a_survey_is_not_lost`; Task 3 walk `chart-live` (a write redraws within the window).
2. **A malformed or hostile page message** (an id as a string, `1.5`, `true`, `1e300`; `file:`, `javascript:`, `data:` URLs; unknown types; a missing or non-boolean `animate`; ink `"pop"`) → ignored, nothing opens, the settings unchanged. Tests: Task 1 `bridge_refuses_file_and_javascript_urls`, `bridge_ignores_unknown_types`, `a_retired_or_unknown_stored_ink_reads_back_as_blue`; Task 3 walk `chart` (only `open: https://example.invalid/x`).
3. **The window closed during a survey, or reopened at once** → the old survey never reaches the new window, nothing keeps running, no 3 s polling for the chart. Tests: Task 1 `closing_drops_a_running_survey_and_the_schedule`; Task 3 walks (`open=false onScreen=false`, `surveys` unchanged after a change made once the window is closed).
4. **The page asked to go elsewhere** (a link in the popover, `location.href`, `about:blank`, `window.open`) → the page stays or comes back and draws again; only http(s) addresses reach the browser, through `openURL`. Tests: Task 1 `only_the_chart_page_is_navigable`, `documents_wait_for_the_page_then_load_then_update`; Task 3 walk step `23-home`.
5. **Plasma animations off** (`AnimationDurationFactor=0`) → the page starts still, and a prefs message from the page does not overwrite the stored animate choice. Tests: Task 1 `reduce_motion_starts_still_and_keeps_the_stored_choice`; Task 3 walk `chart` (`reduce=true`, `animate=true` kept).

---

## B7a: `ressac_ui::chart`

### Task 1 (B7a): refresh rules, bridge, page queue, files, window size

**Files:**
- Modify: `crates/ressac-ui/Cargo.toml` (`serde_json.workspace = true`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod chart;`)
- Create: `crates/ressac-ui/src/chart.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/settings.rs` (window size keys, field, setter, tests)

**Interfaces:**
- Consumes: B3 `article::js_string`, `sanitize::is_web_url`; B1 `settings::{ChartPrefs, ChartView, ChartInk, Settings, SettingsStore}`; core `Store::{open_existing_query_only, chart_snapshot}`.
- Produces: `ressac_ui::chart` and the settings additions (header).

Exact copies:

```rust
pub const ADAPTER_SCRIPT: &str = r#"(function () {
  "use strict";
  var pending = [], target = null;
  function send(message) {
    var text = JSON.stringify(message);
    if (target) target.post(text); else pending.push(text);
  }
  var ressac = Object.freeze({postMessage: send});
  Object.defineProperty(window, "webkit", {value: Object.freeze({messageHandlers: Object.freeze({ressac: ressac})})});
  new QWebChannel(qt.webChannelTransport, function (channel) {
    target = channel.objects.ressac;
    pending.splice(0).forEach(function (text) { target.post(text); });
  });
})();
"#;

pub const PROBE_SCRIPT: &str = r#"(function () {
  const c = document.querySelector("canvas");
  let colours = 0, print = "";
  if (c && c.width && c.height) {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    const seen = new Set(); let p = 0;
    for (let i = 0; i < d.length; i += 4 * 61) { const v = (d[i] << 16) | (d[i + 1] << 8) | d[i + 2]; seen.add(v); p = (p * 31 + v) >>> 0; }
    colours = seen.size; print = String(p);
  }
  const theme = [...document.querySelectorAll(".bar button")].find(b => b.textContent === "By theme");
  const faces = [...document.fonts].filter(f => f.status === "loaded").length;
  return "colours=" + colours + " print=" + print + " faces=" + faces + " ink=" + (document.documentElement.dataset.ink || "")
    + " byTheme=" + (theme ? theme.getAttribute("aria-pressed") : "") + " home=" + typeof Chart + " " + location.href;
})()"#;
```

Values: `FILES` = `index.html` (`text/html; charset=utf-8`), `chart.css` (`text/css`), `d3.min.js`, `chart-core.js`, `chart-draw.js` (`text/javascript`), `fonts/CormorantGaramond-Medium.woff2`, `fonts/CormorantGaramond-SemiBold.woff2`, `fonts/CormorantGaramond-Italic.woff2`, `fonts/CormorantGaramond-MediumItalic.woff2`, `fonts/CormorantSC-Medium.woff2`, `fonts/CormorantSC-SemiBold.woff2`, `fonts/IBMPlexMono-Regular.woff2`, `fonts/IBMPlexMono-Medium.woff2` (`font/woff2`). `handle`: a JSON object whose `"type"` is exactly `openFeed` (an `id` that is a JSON number holding a whole value representable in `i64`, also written `7.0`; never a boolean or a string), `openURL` (a string `url` that `is_web_url` accepts) or `prefs` (`view` and `ink` that `ChartView::parse` / `ChartInk::parse` accept and an `animate` that is a JSON boolean). `page_prefs` = `format!(r#"{{"view":"{}","ink":"{}","animate":{}}}"#, view, ink, animate && !reduce_motion)`. `save_prefs` = `set_chart_view`, `set_chart_ink`, and `set_chart_animate` only when `!reduce_motion`. `ChartPage::load_finished`: `succeeded && url == PAGE_URL` → loaded, nothing shown, `Run(load_script(latest, prefs))` when a document waits else `Nothing`; `succeeded` elsewhere → `GoHome`; a failure → `Nothing`. `show` records `latest`, and when loaded and `json` differs from the one shown returns `load_script` for the first document of this load, `update_script` after.

- [ ] **Step 1: Write the failing tests** in `chart.rs` (helpers: `fn doc(version: u64) -> String { format!(r#"{{"version":{version},"feeds":[]}}"#) }`, `fn ms(n: u64) -> Duration`, `let t0 = Instant::now();`):

```rust
// ChartModelTests
#[test]
fn decodes_version_1() {
    let mut model = ChartModel::default();
    assert_eq!(model.phase(), &Phase::Loading);
    let s = model.open(Instant::now()).unwrap();
    assert_eq!(model.finished(s, Ok(doc(1))), None);
    assert_eq!(model.phase(), &Phase::Ready(doc(1)));
}

#[test]
fn refuses_other_versions_and_unreadable_output() {
    let mut model = ChartModel::default();
    let s = model.open(Instant::now()).unwrap();
    model.finished(s, Ok(doc(2)));
    assert_eq!(model.phase(), &Phase::Failed("This build draws chart version 1, but ressac produced version 2.".into()));
    let s = model.refresh().unwrap();
    model.finished(s, Ok("nope".into()));
    assert_eq!(model.phase(), &Phase::Failed("ressac printed a chart this app cannot read.".into()));
    assert_eq!(check_document(r#"{"feeds":[]}"#), Err(UNREADABLE.to_string()));
}

#[test]
fn a_failed_survey_shows_its_message() {
    let mut model = ChartModel::default();
    let s = model.open(Instant::now()).unwrap();
    model.finished(s, Err("database is locked".into()));
    assert_eq!(model.phase(), &Phase::Failed("database is locked".into()));
    assert_eq!(model.last_error(), Some("database is locked"));
}

#[test]
fn debounces_bursts() {
    let t0 = Instant::now();
    let mut model = ChartModel::with_timing(ms(300), RESURVEY);
    let s = model.open(t0).unwrap();
    model.finished(s, Ok(doc(1)));
    for i in 0..5 { model.note_change(t0 + ms(20 * i)); }
    assert_eq!(model.tick(t0 + ms(299)), None);
    let s = model.tick(t0 + ms(300)).expect("one survey for the burst");
    model.finished(s, Ok(doc(1)));
    assert_eq!(model.tick(t0 + ms(800)), None, "the burst is spent");
    model.note_change(t0 + ms(900));   // a later change starts a new window
    assert_eq!(model.tick(t0 + ms(1199)), None);
    assert!(model.tick(t0 + ms(1200)).is_some());
}

#[test]
fn a_change_during_a_survey_is_not_lost() {
    let t0 = Instant::now();
    let mut model = ChartModel::with_timing(ms(20), RESURVEY);
    let first = model.open(t0).unwrap();
    model.note_change(t0);
    assert_eq!(model.tick(t0 + ms(20)), None, "a survey is running: it reruns after");
    let second = model.finished(first, Ok(doc(1))).expect("the rerun");
    assert_ne!(second, first);
    assert_eq!(model.finished(second, Ok(doc(1))), None, "only once");
}

#[test]
fn failed_refresh_keeps_last_chart() {
    let mut model = ChartModel::default();
    let s = model.open(Instant::now()).unwrap();
    model.finished(s, Ok(doc(1)));
    let s = model.refresh().unwrap();
    model.finished(s, Err("boom".into()));
    assert_eq!((model.phase(), model.last_error()), (&Phase::Ready(doc(1)), Some("boom")));
    let s = model.refresh().unwrap();
    model.finished(s, Ok(doc(1)));
    assert_eq!(model.last_error(), None);
}

#[test]
fn following_draws_on_open_then_on_later_changes() {
    let t0 = Instant::now();
    let mut model = ChartModel::with_timing(ms(50), RESURVEY);
    let s = model.open(t0).expect("drawn on open");
    model.finished(s, Ok(doc(1)));
    assert_eq!(model.open(t0 + ms(10)), None, "already open");
    assert_eq!(model.tick(t0 + ms(300)), None, "no change, no survey");
    model.note_change(t0 + ms(300));
    assert!(model.tick(t0 + ms(350)).is_some());
}

#[test]
fn closing_drops_a_running_survey_and_the_schedule() {
    let t0 = Instant::now();
    let mut model = ChartModel::with_timing(ms(150), ms(1000));
    let s = model.open(t0).unwrap();
    model.note_change(t0 + ms(10));
    model.close();
    assert_eq!(model.finished(s, Ok(doc(1))), None);
    assert_eq!((model.phase(), model.is_open()), (&Phase::Loading, false), "the result never reaches the closed chart");
    model.note_change(t0 + ms(200));
    assert_eq!(model.tick(t0 + ms(5000)), None);
    assert_eq!(model.refresh(), None);
}

#[test]
fn following_resurveys_periodically_until_closed() {
    let t0 = Instant::now();
    let mut model = ChartModel::with_timing(ms(50), ms(100));
    let s = model.open(t0).unwrap();
    model.finished(s, Ok(doc(1)));
    let s = model.tick(t0 + ms(100)).expect("ages move with the clock");
    model.finished(s, Ok(doc(1)));
    assert_eq!(model.tick(t0 + ms(150)), None);
    assert!(model.tick(t0 + ms(200)).is_some());
    model.close();
    assert_eq!(model.tick(t0 + ms(1000)), None);
    assert_eq!(RESURVEY, Duration::from_secs(300));
    assert_eq!(DEBOUNCE, Duration::from_secs(2));
}

#[test]
fn a_document_file_is_shown_like_a_survey() {
    let mut model = ChartModel::default();
    assert_eq!(model.show_document(doc(1)), Err(NOT_OPEN.to_string()), "closed: nothing to show it in");
    let s = model.open(Instant::now()).unwrap();
    model.finished(s, Err("boom".into()));
    assert_eq!(model.show_document(doc(2)), Err(unsupported(2)));
    assert_eq!(model.show_document(doc(1)), Ok(()));
    assert_eq!((model.phase(), model.last_error()), (&Phase::Ready(doc(1)), None));
}

#[test]
fn survey_reads_the_store_like_the_cli() {
    let db = TestDb::new();
    let now = at("2026-10-12T09:00:00.000Z");
    let json = survey(&db.path, now).unwrap();
    assert_eq!(json, serde_json::to_string(&db.store().chart_snapshot(now).unwrap()).unwrap());
    assert_eq!(check_document(&json), Ok(()));
    let missing = db.path.with_file_name("missing.sqlite");
    assert!(survey(&missing, now).is_err());
    assert!(!missing.exists(), "a survey never creates a database");
}

// ChartBridgeTests
#[test]
fn bridge_accepts_known_messages() {
    assert_eq!(handle(r#"{"type":"openFeed","id":42}"#), Some(ChartAction::OpenFeed(42)));
    assert_eq!(handle(r#"{"type":"openFeed","id":7.0}"#), Some(ChartAction::OpenFeed(7)));
    assert_eq!(handle(r#"{"type":"openURL","url":"https://example.org/a?b=1"}"#), Some(ChartAction::OpenUrl("https://example.org/a?b=1".into())));
    assert_eq!(handle(r#"{"type":"openURL","url":"http://example.org/"}"#), Some(ChartAction::OpenUrl("http://example.org/".into())));
    assert_eq!(handle(r#"{"type":"prefs","view":"themes","ink":"nuit","animate":false}"#),
        Some(ChartAction::Prefs(ChartPrefs { view: ChartView::Themes, ink: ChartInk::Nuit, animate: false })));
}

#[test]
fn bridge_refuses_file_and_javascript_urls() {
    for url in ["file:///etc/passwd", "javascript:alert(1)", "data:text/html,<p>x</p>", "ftp://example.org/", "ressac-toggle:1",
                "http://", "https:no-host", "not a url", ""] {
        assert_eq!(handle(&serde_json::json!({"type": "openURL", "url": url}).to_string()), None, "{url}");
    }
    assert_eq!(handle(r#"{"type":"openURL","url":1}"#), None);
    assert_eq!(handle(r#"{"type":"openURL"}"#), None);
}

#[test]
fn bridge_ignores_unknown_types() {
    for message in [
        r#"{"type":"navigate","url":"https://example.org/"}"#, r#"{"type":"OPENFEED","id":1}"#, r#"{"id":1}"#,
        r#""openFeed""#, r#"["openFeed",1]"#, "null", "not json", "", "undefined",
        r#"{"type":"openFeed","id":"42"}"#, r#"{"type":"openFeed","id":1.5}"#, r#"{"type":"openFeed","id":true}"#,
        r#"{"type":"openFeed","id":1e300}"#,
        r#"{"type":"prefs","view":"sideways","ink":"bleu","animate":true}"#, r#"{"type":"prefs","view":"harbour","ink":"green","animate":true}"#,
        r#"{"type":"prefs","view":"harbour","ink":"bleu","animate":1}"#, r#"{"type":"prefs","view":"harbour","ink":"bleu"}"#,
    ] {
        assert_eq!(handle(message), None, "{message}");
    }
}

#[test]
fn prefs_persist() {
    let mut store: BTreeMap<String, String> = BTreeMap::new();
    let mut settings = Settings::load(&store);
    assert_eq!(settings.chart, ChartPrefs { view: ChartView::Harbour, ink: ChartInk::Bleu, animate: true });
    save_prefs(&mut settings, &mut store, &ChartPrefs { view: ChartView::Themes, ink: ChartInk::Nuit, animate: false }, false);
    assert_eq!((store["chart.view"].as_str(), store["chart.ink"].as_str(), store["chart.animate"].as_str()), ("themes", "nuit", "false"));
    assert_eq!(Settings::load(&store).chart, ChartPrefs { view: ChartView::Themes, ink: ChartInk::Nuit, animate: false });
    store.insert("chart.view".into(), "sideways".into());
    assert_eq!(Settings::load(&store).chart, ChartPrefs { view: ChartView::Harbour, ink: ChartInk::Nuit, animate: false });
}

#[test]
fn reduce_motion_starts_still_and_keeps_the_stored_choice() {
    let prefs = ChartPrefs { view: ChartView::Themes, ink: ChartInk::Nuit, animate: true };
    assert_eq!(page_prefs(&prefs, false), r#"{"view":"themes","ink":"nuit","animate":true}"#);
    assert_eq!(page_prefs(&prefs, true), r#"{"view":"themes","ink":"nuit","animate":false}"#);
    let mut store: BTreeMap<String, String> = BTreeMap::new();
    let mut settings = Settings::load(&store);
    save_prefs(&mut settings, &mut store, &prefs, false);
    save_prefs(&mut settings, &mut store, &ChartPrefs { view: ChartView::Harbour, ink: ChartInk::Nuit, animate: false }, true);
    assert_eq!(Settings::load(&store).chart, ChartPrefs { view: ChartView::Harbour, ink: ChartInk::Nuit, animate: true });
}

#[test]
fn a_retired_or_unknown_stored_ink_reads_back_as_blue() {
    assert_eq!(ChartInk::ALL, [ChartInk::Bleu, ChartInk::Nuit]);
    for stored in ["pop", "pirate", "rouge", ""] {
        let store = BTreeMap::from([("chart.ink".to_string(), stored.to_string()), ("chart.view".to_string(), "themes".to_string())]);
        assert_eq!(Settings::load(&store).chart, ChartPrefs { view: ChartView::Themes, ink: ChartInk::Bleu, animate: true }, "{stored}");
        let message = serde_json::json!({"type": "prefs", "view": "themes", "ink": stored, "animate": true}).to_string();
        assert_eq!(handle(&message), None, "{stored}");
    }
}

#[test]
fn only_the_chart_page_is_navigable() {
    assert!(allows_navigation(PAGE_URL, true));
    assert!(!allows_navigation(PAGE_URL, false));
    for url in ["https://example.org/", "http://example.org/index.html", "ressac:chart/chart.css", "ressac:chart/LICENSE-d3.txt",
                "file:///etc/passwd", "about:blank", "data:text/html,x", "ressac:chart/index.html?x=1", "qrc:/chart/index.html",
                "ressac:page/1-1", ""] {
        assert!(!allows_navigation(url, true), "{url}");
    }
}

#[test]
fn documents_wait_for_the_page_then_load_then_update() {
    let prefs = r#"{"view":"harbour","ink":"bleu","animate":true}"#;
    let mut page = ChartPage::default();
    assert_eq!(page.show(&doc(1), prefs), None, "the page has not loaded");
    page.load_started();
    assert_eq!(page.load_finished(PAGE_URL, true, prefs), PageStep::Run(load_script(&doc(1), prefs)));
    assert_eq!(page.show(&doc(1), prefs), None, "the same document is not sent twice");
    let next = r#"{"version":1,"feeds":[{"id":1}]}"#;
    assert_eq!(page.show(next, prefs), Some(update_script(next)));
    page.load_started();   // a reload: the page starts empty
    let still = r#"{"view":"harbour","ink":"bleu","animate":false}"#;
    assert_eq!(page.load_finished(PAGE_URL, true, still), PageStep::Run(load_script(next, still)));
    assert_eq!(page.load_finished("about:blank", true, prefs), PageStep::GoHome);
    assert_eq!(page.load_finished("ressac:chart/chart.css", false, prefs), PageStep::Nothing);
    assert_eq!(ChartPage::default().load_finished(PAGE_URL, true, prefs), PageStep::Nothing, "nothing to draw yet");
}

#[test]
fn scripts_never_splice_the_document() {
    let hostile = r#"{"version":1,"feeds":[{"title":"</script><script>alert(1)</script> \"x"}]}"#;
    let prefs = r#"{"view":"harbour","ink":"bleu","animate":true}"#;
    assert_eq!(load_script(hostile, prefs), format!("Chart.load({}, {prefs})", js_string(hostile)));
    assert_eq!(update_script(hostile), format!("Chart.update({})", js_string(hostile)));
    assert!(!load_script(hostile, prefs).contains("</script>"));
}

#[test]
fn the_chart_files_are_the_mac_files() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/macos/Ressac/Resources/Chart");
    for f in FILES {
        assert!(dir.join(f.path).is_file(), "{}", f.path);
        assert_eq!(file(f.path), Some(&f));
    }
    let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
    for referenced in ["chart.css", "d3.min.js", "chart-core.js", "chart-draw.js"] {
        assert!(html.contains(&format!("\"{referenced}\"")) && file(referenced).is_some(), "{referenced}");
    }
    let woff2 = std::fs::read_dir(dir.join("fonts")).unwrap().filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "woff2")).count();
    assert_eq!(FILES.iter().filter(|f| f.mime == "font/woff2").count(), woff2);
    for refused in ["LICENSE-d3.txt", "fonts/IBMPlexMono-OFL.txt", "../reader.css", "/index.html", "index.html?x", ""] {
        assert_eq!(file(refused), None, "{refused}");
    }
    assert_eq!(unsupported(2), "This build draws chart version 1, but ressac produced version 2.");
    assert_eq!((NOT_OPEN, NOT_DRAWN, RETRY, WINDOW_TITLE),
        ("The database is not open. The main window says why.", "The chart could not be drawn", "Retry", "Sea Chart"));
}

#[test]
fn the_adapter_defines_the_webkit_handler_over_the_channel() {
    for part in ["window, \"webkit\"", "messageHandlers", "ressac: ressac", "new QWebChannel(qt.webChannelTransport",
                 "channel.objects.ressac", "JSON.stringify(message)"] {
        assert!(ADAPTER_SCRIPT.contains(part), "{part}");
    }
}
```

In `settings.rs`:

```rust
#[test]
fn the_chart_window_size_is_remembered_and_never_below_the_minimum() {
    let mut store: BTreeMap<String, String> = BTreeMap::new();
    let mut s = Settings::load(&store);
    assert_eq!(s.chart_window, (1100, 720));
    assert!(s.set_chart_window(&mut store, 1300, 800));
    assert_eq!((store["chart.window.width"].as_str(), store["chart.window.height"].as_str()), ("1300", "800"));
    assert!(!s.set_chart_window(&mut store, 1300, 800), "unchanged");
    assert!(s.set_chart_window(&mut store, 100, 50));
    assert_eq!(Settings::load(&store).chart_window, (640, 360));
    let garbage = BTreeMap::from([("chart.window.width".to_string(), "wide".to_string()), ("chart.window.height".to_string(), "-3".to_string())]);
    assert_eq!(Settings::load(&garbage).chart_window, (1100, 720));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui chart 2>&1 | tail -3` → `error[E0432]: unresolved import` (`chart`).
- [ ] **Step 3: Implement** the four files as above.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; `cargo clippy -p ressac-ui --all-targets -- -D warnings` → `Finished`. Then the full gate (B7a ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/Cargo.toml crates/ressac-ui/src/lib.rs crates/ressac-ui/src/chart.rs crates/ressac-ui/src/settings.rs
git commit -m "ressac-ui: Sea Chart rules (refresh model, page bridge, document queue, served files, adapter), window size keys

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B7b: the bridge

### Task 2 (B7b): `Chart` singleton, `ressac:chart/` files, `WebChannel`, `AppSettings`

**Files:**
- Create: `apps/linux/src/chart.rs` (the bridge; `pub(crate) fn file_bytes(path: &str) -> Option<&'static [u8]>` over `include_bytes!("../../macos/Ressac/Resources/Chart/<path>")` for each `FILES` entry; unit tests)
- Modify: `apps/linux/src/lib.rs` (`mod chart;`)
- Modify: `apps/linux/src/reader.rs` (`ressac_scheme_serve`: `url.strip_prefix(chart::PREFIX)` → `chart::file(path)` and `crate::chart::file_bytes(path)` → mime and bytes; before the page and asset branches)
- Modify: `apps/linux/build.rs` (`.qt_module("WebChannel")` after `WebEngineCore`)
- Modify: `apps/linux/src/settings.rs` (`chartWindowWidth`, `chartWindowHeight`, `saveChartPrefs`)

**Interfaces:**
- Consumes: Task 1; B2 `with_session`, `Session.db`; B1 `QSettingsStore::default_file`, `Settings::load`; B6 `library::local_path`.
- Produces: `Chart`, the scheme branch, the `AppSettings` additions (header).

Behaviour (`ChartRust { model: ChartModel, page: ChartPage, reduce_motion: bool, surveys: i32 }`):
- `openWindow()`: `model.open(Instant::now())` → `start(survey)`; `state_changed`. `closeWindow()`: `model.close()`, `page = ChartPage::default()`, `state_changed`. `noteChange()`: `model.note_change(Instant::now())`. `tick()`: `model.tick(Instant::now())` → `start`. `retry()`: `model.refresh()` → `start`.
- `start(survey)`: `with_session(|s| s.db.clone())`; `None` → `survey_done(survey, Err(NOT_OPEN.into()))`; else `let qt = self.qt_thread(); std::thread::spawn(move || { let result = chart::survey(&db, Utc::now()); if qt.queue(move |o| o.survey_done(survey, result)).is_err() { tracing::debug!("the app quit during a survey") } })`.
- `survey_done(survey, result)`: `let ok = result.is_ok(); let next = model.finished(survey, result)`; if `ok` and the phase is `Ready` → `surveys += 1`; `show_current()`; `next` → `start`; failures logged with `tracing::warn!("chart survey failed: {}", redact_text(&message))`; `state_changed`.
- `show_current()`: on `Phase::Ready(json)`, `page.show(&json, &prefs())` → `Some(script)` → `run_script(script)`. `prefs()` = `page_prefs(&Settings::load(&QSettingsStore::default_file()).chart, reduce_motion)`.
- `pageLoadStarted()` → `page.load_started()`. `pageLoaded(url, ok)` → `page.load_finished(&url, ok, &prefs())`: `Run(s)` → `run_script(s)`; `GoHome` → `go_home()`; `Nothing`.
- `allowsNavigation(url, mainFrame)` → `chart::allows_navigation`. `receive(text)` → `handle(&text)`: `OpenFeed(id)` → `open_feed(format!("feed:{id}"))`; `OpenUrl(u)` → `open_url(u)`; `Prefs(p)` → `prefs_reported(p.view.as_str(), p.ink.as_str(), p.animate)`; `None` → `tracing::debug!("chart: ignored a page message")`.
- `showFile(url)`: `library::local_path` → `std::fs::read_to_string` → `model.show_document(json)` → `Ok` → `show_current()`, `state_changed`, true; any failure → `tracing::warn!` and false (`surveys` is not counted).
- Getters: `phase` = `as_str`; `message` = the `Failed` text or `""`; `refreshError` = `last_error()` when the phase is `Ready`, else `""`; `open` = `is_open()`.
- `AppSettings`: `chartWindowWidth/Height` read `settings.chart_window` and write through `set_chart_window` (each setter keeps the other dimension); `saveChartPrefs(view, ink, animate, reduce)`: `ChartView::parse` / `ChartInk::parse` (an unknown word does nothing) → `chart::save_prefs`, then the `chart_view` / `chart_ink` / `chart_animate` notifies that changed.

- [ ] **Step 1: Write the failing test** in `chart.rs`'s `#[cfg(test)] mod tests`:

```rust
#[test]
fn chart_files_are_served_byte_for_byte() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../macos/Ressac/Resources/Chart");
    for f in ressac_ui::chart::FILES {
        let (mut mime, mut body) = (String::new(), Vec::new());
        assert!(crate::reader::ressac_scheme_serve(&format!("ressac:chart/{}", f.path), &mut mime, &mut body), "{}", f.path);
        assert_eq!(mime, f.mime);
        assert_eq!(body, std::fs::read(dir.join(f.path)).unwrap(), "{} differs from the Mac file", f.path);
    }
    for refused in ["ressac:chart/LICENSE-d3.txt", "ressac:chart/../reader.css", "ressac:chart/", "ressac:chart/index.html?x=1"] {
        assert!(!crate::reader::ressac_scheme_serve(refused, &mut String::new(), &mut Vec::new()), "{refused}");
    }
}
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --lib chart 2>&1 | grep -E "^error|test result" | head -3'` → `error[E0432]` (`chart` module missing).
- [ ] **Step 3: Implement** the five files as above.
- [ ] **Step 4: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.` (the singleton is registered, not yet used). Then the full gate (B7b ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/src/chart.rs apps/linux/src/lib.rs apps/linux/src/reader.rs apps/linux/build.rs apps/linux/src/settings.rs
git commit -m "ressac-kde: Chart singleton (surveys, page documents, bridge messages), the Mac chart files under ressac:chart/

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B7c: the window

### Task 3 (B7c): `SeaChartWindow.qml`, opening, change feed, walks

**Files:**
- Create: `apps/linux/qml/SeaChartWindow.qml`
- Modify: `apps/linux/qml/Main.qml`, `apps/linux/qml/ReadingPage.qml`
- Test (not counted): `apps/linux/tests/chart.rs` (new)

**Interfaces:**
- Consumes: Task 2; B6 `openExternally`, the focused-window grab, `assert_drawn_min`; B4 `steps` chooser; B2 `Live`, `SourcesModel`, `EntriesModel`.
- Produces: the QML items of the header.

`SeaChartWindow.qml` (`import QtWebEngine`, `import QtWebChannel`; root `QQC2.ApplicationWindow { id: chartWindow; title: Chart.windowTitle; minimumWidth: 640; minimumHeight: 360; width: AppSettings.chartWindowWidth; height: AppSettings.chartWindowHeight; color: Kirigami.Theme.backgroundColor; property var chrome }`):
- `Binding { target: Chart; property: "reduceMotion"; value: Kirigami.Units.longDuration <= 1 }`; `Binding { target: Live; property: "chartOnScreen"; value: chartWindow.visible && chartWindow.visibility !== Window.Minimized }`; `Timer { interval: Chart.tickInterval; repeat: true; running: Chart.open; onTriggered: Chart.tick() }`.
- `onClosing: { AppSettings.chartWindowWidth = width; AppSettings.chartWindowHeight = height; Chart.closeWindow(); Live.chartOnScreen = false; Qt.callLater(chrome.chartClosed) }`.
- `QtObject { id: relay; WebChannel.id: "ressac"; function post(text) { Chart.receive(String(text)) } }` and `WebChannel { id: channel; registeredObjects: [relay] }`.
- `WebEngineView { id: web; anchors.fill: parent; visible: Launcher.phase === Launcher.Ready && Chart.phase === "ready"; webChannel: channel; settings.javascriptCanOpenWindows: false; settings.localContentCanAccessRemoteUrls: false }`; `Component.onCompleted`: two scripts from `WebEngine.script()` inserted in this order — `name: "qwebchannel"`, `sourceUrl: "qrc:/qtwebchannel/qwebchannel.js"`, then `name: "ressac-adapter"`, `sourceCode: Chart.adapterScript`; both `injectionPoint: WebEngineScript.DocumentReady`, `worldId: WebEngineScript.MainWorld` — then `web.url = Chart.pageUrl`. `onNavigationRequested: (request) => { if (Chart.allowsNavigation(String(request.url), request.isMainFrame)) request.accept(); else request.reject() }`; `onNewWindowRequested: {}` (nothing opens); `onLoadingChanged: (info) => { if (info.status === WebEngineView.LoadStartedStatus) Chart.pageLoadStarted(); else Chart.pageLoaded(String(info.url), info.status === WebEngineView.LoadSucceededStatus) }`; `onRenderProcessTerminated: web.reload()`; `onJavaScriptConsoleMessage: (level, message, line, source) => { if (level === WebEngineView.ErrorMessageLevel) console.warn("chart page: " + message) }` (a page `TypeError` then fails `assert_clean`).
- `Connections { target: Chart; function onRunScript(script) { web.runJavaScript(script) } function onGoHome() { web.url = Chart.pageUrl } function onPrefsReported(view, ink, animate) { AppSettings.saveChartPrefs(view, ink, animate, Chart.reduceMotion) } }`; `Connections { target: Launcher; function onPhaseChanged() { if (Launcher.phase === Launcher.Ready && Chart.open) { Chart.closeWindow(); Chart.openWindow() } } }`.
- Overlays: `QQC2.BusyIndicator { anchors.centerIn: parent; visible: (Launcher.phase === Launcher.Ready && Chart.phase === "loading") || Launcher.phase === Launcher.Starting || Launcher.phase === Launcher.Migrating }`; `QQC2.Label { anchors.centerIn: parent; visible: Launcher.phase === Launcher.TooNew || Launcher.phase === Launcher.Failed; text: Chart.notOpenText; color: Kirigami.Theme.disabledTextColor; padding: 40 }`; the failure view (`visible: Launcher.phase === Launcher.Ready && Chart.phase === "failed"`): `Kirigami.Heading { level: 3; text: Chart.notDrawnText }`, `Kirigami.SelectableLabel { text: Chart.message; color: Kirigami.Theme.disabledTextColor; horizontalAlignment: Text.AlignHCenter }`, `QQC2.Button { text: Chart.retryText; onClicked: Chart.retry() }`; the capsule (`visible: web.visible && Chart.refreshError !== ""`, anchored bottom-centre with 16 px margin, `Kirigami.Theme.backgroundColor` at 0.9, radius half its height): `QQC2.Label { text: Chart.refreshError; maximumLineCount: 2; textFormat: Text.PlainText }` and `QQC2.Button { text: Chart.retryText; onClicked: Chart.retry() }`.
- `function probe(tag) { web.runJavaScript(Chart.probeScript, function (result) { console.warn("walk: " + tag + " " + result) }) }`; `function runPage(script) { web.runJavaScript(script) }`.

`Main.qml`:
- `Loader { id: chartLoader; active: false; sourceComponent: Component { SeaChartWindow { chrome: root } } }`; `function openChart() { chartLoader.active = true; const w = chartLoader.item; w.show(); w.raise(); w.requestActivate(); Chart.openWindow() }`; `function chartClosed() { chartLoader.active = false }`.
- `Binding { target: mainMenu.seaChart; property: "enabled"; value: true }`; `Connections { target: mainMenu.seaChart; function onTriggered() { root.openChart() } }`.
- `Connections { target: Chart; function onOpenFeed(key) { SourcesModel.selectKey(key); if (root.visibility === Window.Minimized) root.showNormal(); root.show(); root.raise(); root.requestActivate() } function onOpenUrl(url) { root.openExternally(url) } }`.
- The chooser gains `Recette.walk === "chart" ? chartSteps : Recette.walk === "chart-live" ? chartLiveSteps : …`, with `function waits(n, from) { … }` not needed: the wait steps are written out. (`const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))`.)

```qml
readonly property var chartSteps: [
    { name: "02-open", enter: function () { root.openChart()
        console.warn("walk: 02-open open=" + Chart.open + " reduce=" + Chart.reduceMotion + " onScreen=" + Live.chartOnScreen) } },
    { name: "03-wait", enter: function () {} }, { name: "04-wait", enter: function () {} }, { name: "05-wait", enter: function () {} },
    { name: "06-wait", enter: function () {} }, { name: "07-wait", enter: function () {} }, { name: "08-wait", enter: function () {} },
    { name: "09-wait", enter: function () {} },
    { name: "10-first", enter: function () { chartLoader.item.probe("10-first") } },
    { name: "11-fixture", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
        console.warn("walk: 11-fixture shown=" + Chart.showFile(home + "/fixture.json")) } },
    { name: "12-wait", enter: function () {} }, { name: "13-wait", enter: function () {} }, { name: "14-wait", enter: function () {} },
    { name: "15-drawn", enter: function () { chartLoader.item.probe("15-drawn") } },
    { name: "16-messages", enter: function () { chartLoader.item.runPage(
        "const h = window.webkit.messageHandlers.ressac;"
        + "h.postMessage({type: 'openFeed', id: 8}); h.postMessage({type: 'openURL', url: 'file:///etc/passwd'});"
        + "h.postMessage({type: 'openURL', url: 'javascript:alert(1)'}); h.postMessage({type: 'openFeed', id: '9'});"
        + "h.postMessage({type: 'openURL', url: 'https://example.invalid/x'});"
        + "[...document.querySelectorAll('.bar button')].find(b => b.textContent === 'From the harbour').click();") } },
    { name: "17-wait", enter: function () {} },
    { name: "18-answered", enter: function () { console.warn("walk: 18-answered selected=" + SourcesModel.selectedKey + " view=" + AppSettings.chartView
        + " ink=" + AppSettings.chartInk + " animate=" + AppSettings.chartAnimate); chartLoader.item.requestActivate() } },
    { name: "19-navigate", enter: function () { chartLoader.item.runPage(
        "for (const t of ['LICENSE-d3.txt', 'https://example.invalid/']) { const a = document.createElement('a'); a.href = t; document.body.append(a); a.click() }"
        + "setTimeout(() => { location.href = 'about:blank' }, 100);") } },
    { name: "20-wait", enter: function () {} }, { name: "21-wait", enter: function () {} }, { name: "22-wait", enter: function () {} },
    { name: "23-home", enter: function () { chartLoader.item.probe("23-home") } },
    { name: "24-close", enter: function () { chartLoader.item.width = 1200; chartLoader.item.close() } },
    { name: "25-closed", enter: function () { console.warn("walk: 25-closed open=" + Chart.open + " onScreen=" + Live.chartOnScreen
        + " window=" + (chartLoader.item !== null)) } },
]
readonly property var chartLiveSteps: [
    { name: "02-open", enter: function () { root.openChart() } },
    { name: "03-wait", enter: function () {} }, { name: "04-wait", enter: function () {} }, { name: "05-wait", enter: function () {} },
    { name: "06-first", enter: function () { console.warn("walk: 06-first surveys=" + Chart.surveys + " phase=" + Chart.phase) } },
    { name: "07-write", enter: function () { root.requestActivate(); SourcesModel.selectKey("unread"); EntriesModel.select(0); EntriesModel.toggleStarred(0) } },
    { name: "08-wait", enter: function () {} }, { name: "09-wait", enter: function () {} }, { name: "10-wait", enter: function () {} },
    { name: "11-wait", enter: function () {} }, { name: "12-wait", enter: function () {} }, { name: "13-wait", enter: function () {} },
    { name: "14-wait", enter: function () {} },
    { name: "15-second", enter: function () { console.warn("walk: 15-second surveys=" + Chart.surveys) } },
    { name: "16-retry", enter: function () { Chart.retry() } },
    { name: "17-wait", enter: function () {} },
    { name: "18-third", enter: function () { console.warn("walk: 18-third surveys=" + Chart.surveys) } },
    { name: "19-close", enter: function () { chartLoader.item.close() } },
    { name: "20-note", enter: function () { console.warn("walk: 20-note surveys=" + Chart.surveys); Chart.noteChange(); EntriesModel.toggleStarred(0) } },
    { name: "21-wait", enter: function () {} }, { name: "22-wait", enter: function () {} }, { name: "23-wait", enter: function () {} },
    { name: "24-wait", enter: function () {} }, { name: "25-wait", enter: function () {} }, { name: "26-wait", enter: function () {} },
    { name: "27-wait", enter: function () {} },
    { name: "28-after", enter: function () { console.warn("walk: 28-after surveys=" + Chart.surveys + " open=" + Chart.open
        + " onScreen=" + Live.chartOnScreen + " polling=" + Live.polling) } },
]
```

`ReadingPage.qml`: `reloadAll()` gains `Chart.noteChange()` (the model ignores it while the window is closed).

- [ ] **Step 1: Write the failing tests** `apps/linux/tests/chart.rs`:

```rust
//! The Sea Chart window, offscreen (spec §7.8, §8.3; inventory §6).
#![cfg(target_os = "linux")]

mod common;
use common::*;

/// The facts a `chart` walk step printed after `walk: <tag> ` (`PROBE_SCRIPT`'s line), by key.
fn probe(err: &str, tag: &str) -> std::collections::HashMap<String, String> {
    let prefix = format!("walk: {tag} ");
    let line = err.lines().find_map(|l| l.split_once(&prefix).map(|(_, rest)| rest.to_string()))
        .unwrap_or_else(|| panic!("{tag} missing in {err}"));
    let (facts, home) = line.split_once(" home=").unwrap();
    let mut map: std::collections::HashMap<String, String> = facts.split(' ')
        .filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    map.insert("home".into(), home.to_string());
    map
}
fn number(facts: &std::collections::HashMap<String, String>, key: &str) -> u64 {
    facts[key].parse().unwrap()
}

#[test]
fn the_sea_chart_draws_the_fixture_with_the_stored_prefs_and_answers_the_page() {
    let run = Run::new();
    let db = run.copy_fixture();
    std::fs::create_dir_all(run.home.join("config/ressac")).unwrap();
    std::fs::write(run.home.join("config/ressac/ressac-kde.conf"), "[General]\nchart.view=themes\nchart.ink=nuit\nchart.animate=true\n").unwrap();
    std::fs::write(run.home.join("config/kdeglobals"), "[KDE]\nAnimationDurationFactor=0\n").unwrap();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../macos/ChartTests/fixture.json"), run.home.join("fixture.json")).unwrap();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "chart")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(err.contains("walk: 02-open open=true reduce=true onScreen=true"), "{err}");
    assert!(err.contains("walk: 11-fixture shown=true"), "{err}");
    let first = probe(&err, "10-first");
    let drawn = probe(&err, "15-drawn");
    assert!(number(&first, "colours") > 8, "the survey's chart stayed blank: {first:?}");
    assert!(number(&drawn, "colours") > 8, "the fixture did not draw: {drawn:?}");
    assert_ne!(first["print"], drawn["print"], "Chart.update did not redraw");
    assert_eq!(number(&drawn, "faces"), 8, "every face loaded under the page's CSP: {drawn:?}");
    assert_eq!((drawn["ink"].as_str(), drawn["byTheme"].as_str()), ("nuit", "true"), "the stored prefs reached Chart.load");
    assert!(err.contains("walk: 18-answered selected=feed:8 view=harbour ink=nuit animate=true"), "{err}");
    assert!(err.contains("open: https://example.invalid/x"), "{err}");
    assert!(!err.contains("open: file:") && !err.contains("open: javascript:"), "{err}");
    let home = probe(&err, "23-home");
    assert_eq!(home["home"], "object ressac:chart/index.html", "the page came back: {home:?}");
    assert!(number(&home, "colours") > 8, "and drew again: {home:?}");
    assert!(err.contains("walk: 25-closed open=false onScreen=false window=false"), "{err}");
    assert_drawn_min(&run.shots.join("15-drawn.png"), 640, 360);
    let conf = std::fs::read_to_string(run.home.join("config/ressac/ressac-kde.conf")).unwrap();
    assert!(conf.contains("chart.view=harbour") && conf.contains("chart.animate=true"), "the page's choice, the stored animate kept: {conf}");
    assert!(conf.contains("chart.window.width=1200") && conf.contains("chart.window.height=720"), "the size is remembered: {conf}");
}

#[test]
fn the_sea_chart_follows_the_database_and_stops_when_closed() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "chart-live")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    // The 3 s data_version poll may note the same write once more: counts are compared, not pinned.
    let count = |tag: &str| -> u64 {
        let prefix = format!("walk: {tag} surveys=");
        err.lines().find_map(|l| l.split_once(&prefix)).map(|(_, rest)| rest.split(' ').next().unwrap().parse().unwrap())
            .unwrap_or_else(|| panic!("{tag} missing in {err}"))
    };
    assert!(err.contains("walk: 06-first surveys=1 phase=ready"), "drawn once on open: {err}");
    let second = count("15-second");
    assert!(second >= 2, "a write redrew the chart within the 2 s window: {err}");
    assert!(count("18-third") > second, "Retry surveys again: {err}");
    assert_eq!(count("28-after"), count("20-note"), "nothing runs once the window is closed: {err}");
    assert!(err.contains("open=false onScreen=false"), "{err}");
}
```

- [ ] **Step 2: Run them to verify they fail** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test chart 2>&1 | grep -E "panicked|test result"'` → FAIL (`walk: 02-open … missing`).
- [ ] **Step 3: Implement** the three QML files.
- [ ] **Step 4: Verify and look** — the same command → `ok.`; `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.`; `node --test 'apps/macos/ChartTests/*.test.cjs'` → `# fail 0`. If a wait is short (the survey, the page and its fonts take up to 3 s), add empty wait steps; never shorten the walk's 400 ms. Open `10-first.png` (the contract database's two islands on the sea, in "nuit"), `15-drawn.png` (the fixture's archipelagos "By theme"), `23-home.png` (the page back after `about:blank`). Add to the ledger's B8e checklist (VM checkpoint 2): open the Sea Chart with Ctrl+Shift+M at scale 2; hover and click an island (popover, "Click the name…" opens the feed in the main window); a link in the popover opens Firefox; the toolbar's View / Ink / Animation choices survive a reopen; Plasma's "Animation speed" at Instant starts the chart still; minimise the chart and check that the 3 s poll stops (`Live.polling`). Then the full gate (B7c and B7 end here). Ledger rows B7a–B7c: commits, test counts, deviations 1–12.
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/SeaChartWindow.qml apps/linux/qml/Main.qml apps/linux/qml/ReadingPage.qml apps/linux/tests/chart.rs
git commit -m "ressac-kde: Sea Chart window (unchanged page, webkit adapter over QWebChannel, refresh rules, prefs)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
