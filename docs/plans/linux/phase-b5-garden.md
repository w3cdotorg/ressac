# Phase B5: the garden (B5a–B5d)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE window has the Mac app's garden: Needs Triage, All Feeds (Status | Rhythm grouping) and Cemetery as a sectioned table with glyphs, clues, rhythm labels, unread counts and chips, feeds decided during the visit kept and dimmed, the selection moving to the next feed to triage, the terminal garden's keys (`u R k r b o w`, chords ignored), Undo (Ctrl+Z), the "Apply N safe repairs?" dialog, and the inspector (status, reason, rhythm line with the Auto…Silent picker, Theme picker, evidence, remarks, links, buttons, redirects and fetch log).

**Runs after:** B4c. Master plan `docs/plans/2026-10-23-linux.md` (rows B5a, B5b; Interfaces; Global Constraints: Runs, Language, Shortcuts, Time, Gates). Spec §7.6, §7.2 (Ctrl+Z). Inventory §4 (binding strings and numbers), §1.2 (garden Undo), §2.3 (Show in Garden), §10.1 (GardenTests, ThemePickerTests). Mac sources: `apps/macos/Ressac/Garden/GardenModel.swift`, `GardenStore.swift`, `GardenTable.swift`, `GardenInspector.swift`, `GardenStatusStyle.swift`, `App/AppState.swift:611-704`, `RessacTests/GardenTests.swift`, `RessacTests/ThemePickerTests.swift`. Terminal patterns: `crates/ressac/src/tui/state.rs`, `tui/reader/antiope.rs:986-1136` (chips, remarks), `tui/reader/rhythm.rs`.

**Architecture:** the rules are Rust in `ressac_ui::garden`, look-agnostic so Antiope's P800–P802 and inspector page (B8) reuse them: pure functions over the core's `garden::GardenRow` (`garden/mod.rs`: sections, retention, progress, safe repairs, keys, labels, rhythm, pickers, the data snapshot), the visit's state machine `GardenVisit` (`garden/visit.rs`, the Mac's `GardenStore`) and the inspector's content `Inspector` (`garden/inspector.rs`). The core gains two reads (`Store::buried_unread`, `Store::feed_theme_keys`). `ressac-kde` adds one `Garden` QML singleton (a list model of table rows, the header, the commands, the inspector's properties) whose decisions, undo, safe repairs, rhythm and theme go through B4's `runs::submit_short`; QML adds `GardenPane.qml` (header, table, dialog) and `GardenInspector.qml`, swapped in for the entries and reader panes while a garden scope is selected.

**Tech Stack:** Rust 2024 / 1.99; cxx-qt / cxx-qt-lib 0.10.0 (`QStringList` properties); Qt 6.11.2 QtQuick `ListView` sections, QtQuick Controls `Dialog`, `DialogButtonBox`, `ComboBox`; Kirigami 6.31 (`ListSectionHeader`, `PlaceholderMessage`); Fedora 44 container.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Four sub-phases** (5-file cap): the master's B5a (`ressac_ui::garden`) becomes **B5a** (core reads + `garden/mod.rs`, 4 files) and **B5b** (`garden/visit.rs`, `garden/inspector.rs`, 3 files); the master's B5b (QML) becomes **B5c** (the bridge, 3 files) and **B5d** (the QML, 5 files). The ledger gets rows B5a–B5d.
2. **Rows are the core's `garden::GardenRow`** (`garden::load_rows`), plus the buried feeds' unread counts, `v_cadence` rows, themes and theme keys, not `v_garden_row` decoded by the app. So the Mac's JSON-decoding cases go: an unknown proposed action or remark ("newer than this app") cannot reach an app linked to the core it was built with (a newer schema is refused at launch by B1's probe), and `GardenTests.anUnknownProposedActionDecodesAndOffersNoRepair` is not ported. The Mac's `gardenRowsDecodeTheCoreJSONShapes` becomes `load_reads_the_fixture_like_the_mac_app`.
3. **Two core reads** (B5a Task 1): `Store::buried_unread` (the Cemetery's "N unread", which `v_unread_count` leaves out) and `Store::feed_theme_keys` (the Theme picker's current value). No schema change.
4. **The inspector's buttons and pickers never take the keyboard** (`focusPolicy: Qt.NoFocus`): after a click, `k`, `r`, `b` still reach the table. The Mac's picker could take the focus (`TODO.md:68`, inventory §11); this fixes it rather than copying it.
5. **Tooltips name KDE keys**: "Apply every safe repair: https upgrades and permanent redirects (Shift+R)", "Undo the last decision, here or in the terminal garden (U or Ctrl+Z)" (the Mac says ⇧R and ⌘Z); every other string is the Mac's.
6. **The garden table and inspector sit in the entries and reader panes' slots** of B2's `SplitView` (one container per slot, the right child visible), so the pane widths and the sidebar stay put.
7. **The garden walk is its own scenario** (`RESSAC_RECETTE_WALK=garden`, B4's `Recette.walk`): its decisions, undo, rhythm and theme changes write the database, which the reading walk's assertions must not see.
8. **Colours:** the inspector's status colour is B2's `sources::status_color` (one map for the sidebar dot and the inspector, as the Mac's `GardenStatusStyle.nativeColor`, whose tests B2's `sources` tests mirror); the teletext colours and their tests are B8's.

## Validated in a throwaway container (`ressac-b45-plan`, see B4)

A `QQC2.Dialog` (`modal: true`, `anchors.centerIn: parent`) whose `footer` is a `QQC2.DialogButtonBox` with two plain `QQC2.Button`s carrying `DialogButtonBox.buttonRole: AcceptRole` / `RejectRole` opened offscreen with the focus on one of its buttons, and `accept()` emitted `accepted`. A `ListView` with `section.property` on a string role drew one header per run of equal values. The cross-singleton door this phase uses (`runs::submit_short`) is B4's, validated there.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-core/src/store/library.rs` | B5a T1 | `Store::buried_unread` |
| `crates/ressac-core/src/store/chart.rs` | B5a T1 | `Store::feed_theme_keys` |
| `crates/ressac-ui/src/garden/mod.rs` (new) | B5a T2; B5b T3 | pure rules, `Snapshot`, `load`, `decide`; `mod visit; mod inspector;` re-exports |
| `crates/ressac-ui/src/lib.rs` | B5a T2 | `pub mod garden;` |
| `crates/ressac-ui/src/garden/visit.rs` (new) | B5b T3 | `GardenVisit` |
| `crates/ressac-ui/src/garden/inspector.rs` (new) | B5b T3 | `Inspector`, `RhythmControl`, `LogLine` |
| `apps/linux/src/garden.rs` (new) | B5c T4 | QML singleton `Garden`: rows, header, commands, runs |
| `apps/linux/src/garden_inspector.rs` (new) | B5c T4 | the inspector's getters on `Garden` |
| `apps/linux/src/lib.rs` | B5c T4 | `mod garden; mod garden_inspector;` |
| `apps/linux/qml/GardenPane.qml` (new) | B5d T5 | header, table, keys, safe-repairs dialog |
| `apps/linux/qml/GardenInspector.qml` (new) | B5d T5 | the inspector |
| `apps/linux/qml/ReadingPage.qml` | B5d T5 | pane swap, `Garden.show`, focus, `reloadAll`, `openUrl`, `perform("focusEntryList")` |
| `apps/linux/qml/SourcesPane.qml` | B5d T5 | feed menu "Show in Garden" |
| `apps/linux/qml/Main.qml` | B5d T5 | `gardenSteps` |
| tests (not counted) | | `crates/ressac-ui/src/test_db.rs` (`TestDb::full`), `apps/linux/tests/garden.rs`, `apps/linux/tests/menus.rs` (one more text test) |

Files per sub-phase: B5a 4, B5b 3, B5c 3, B5d 5.

## Interfaces produced (B6 and B8 rely on these names)

**Core** (Task 1):

```rust
impl Store {
    /// Unread entries of each buried feed (absent: none). `v_unread_count` covers active feeds only.
    pub fn buried_unread(&self) -> Result<HashMap<FeedId, usize>>;
    /// Each themed feed's theme key.
    pub fn feed_theme_keys(&self) -> Result<HashMap<FeedId, String>>;
}
```

**`ressac_ui::garden`** (`garden/mod.rs`, Task 2):

```rust
pub use ressac_core::garden::{GardenRow, Progress, progress, safe_repairs};
pub const LOG_LINES: usize = 20;
pub const SELECT_FIRST: &str = "Select a feed first.";
pub const NO_SAFE_REPAIRS: &str = "No safe repairs pending.";
pub const NOTHING_TO_REPAIR: &str = "Nothing to repair for this feed.";
pub const BURIED_FIRST: &str = "This feed is buried; press k to restore it first.";
pub const NO_SITE: &str = "This feed has no site address.";
pub const NO_WAYBACK: &str = "No Wayback snapshot yet (run `ressac check`).";
pub const NON_WEB: &str = "Refusing to open a non-web address.";
pub const COULD_NOT_READ_GARDEN: &str = "The garden could not be read.";
pub const BURIED_GLYPH: &str = "⚰️";
pub const RHYTHM_CHOICES: [&str; 6] = ["Auto", "Daily", "Weekly", "Monthly", "Rare", "Silent"];
pub fn status_glyph(status: Status) -> &'static str;               // 👯 🦠 🚚 🪦 🤒 🦴 🌙 🌿 ❔ (Unchecked)
pub fn rhythm_glyph(group: Option<CadenceGroup>) -> &'static str;  // ⚡ 📅 🌗 ✨ 💤 ❔
pub fn empty_message(scope: GardenScope) -> &'static str;          // "Nothing to triage", "No feeds", "The cemetery is empty"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping { #[default] Status, Rhythm }                    // ALL, title() "Status"/"Rhythm", as_str/parse "status"/"rhythm"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section { pub id: &'static str, pub glyph: &'static str, pub rows: Vec<FeedId> }
impl Section { pub fn header(&self) -> String; }                    // "🪦 DEAD · 8"
pub fn visible_rows<'a>(rows: &'a [GardenRow], scope: GardenScope, retained: &HashSet<FeedId>) -> Vec<&'a GardenRow>;
pub fn is_dimmed(row: &GardenRow, scope: GardenScope, retained: &HashSet<FeedId>) -> bool;
pub fn sections(rows: &[&GardenRow], scope: GardenScope) -> Vec<Section>;
pub fn rhythm_sections(rows: &[&GardenRow], cadence: &HashMap<FeedId, CadenceDetail>) -> Vec<Section>;
pub fn count_label(scope: GardenScope, rows: &[GardenRow], visible: usize) -> String; // "3 of 4 left", "1 feed", "10 feeds"
pub fn safe_repair_preview(rows: &[GardenRow], repairs: &[(FeedId, Action)]) -> String;
pub fn apply_button_title(count: usize) -> String;                 // "Apply 1 Safe Repair", "Apply 87 Safe Repairs"
pub fn apply_question(count: usize) -> String;                     // "Apply 1 safe repair?", "Apply 87 safe repairs?"
pub fn apply_confirm_label(count: usize) -> &'static str;          // "Apply", "Apply All"
pub fn selection_after_update(selected: Option<FeedId>, previous: &[GardenRow], current: &[GardenRow], ordered: &[FeedId]) -> Option<FeedId>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command { Decide(Decision), Undo, Open(String), ConfirmSafeRepairs, Message(&'static str) }
/// `modifiers`: Qt's mask (`keys::qt`); any Control, Alt or Meta chord is not the garden's.
pub fn command(key: &str, modifiers: u32, row: Option<&GardenRow>, safe_repair_count: usize) -> Option<Command>;
pub fn chip(row: &GardenRow) -> Option<&'static str>;              // kept repaired buried | https follow adopt merge bury | wait
pub fn unread_label(row: &GardenRow, unread: usize) -> Option<String>; // "12 unread", buried feeds only
pub fn tooltip(row: &GardenRow) -> String;                         // folder path, newline, reason (empty parts left out)
pub fn status_line(status: Status) -> String;                      // "🪦 Dead"
pub fn remark_sentence(remark: &Remark) -> String;
pub fn outcome(attempt: &AttemptRecord) -> String;                 // error label, else "HTTP 200", else "no response"
pub fn redirect_line(hop: &Hop) -> String;                         // "301 → https://…"
pub fn rhythm_label(cadence: Option<&CadenceDetail>) -> Option<String>; // "Daily", "Daily*"
pub fn rhythm_text(cadence: &CadenceDetail, now: DateTime<Utc>) -> String;
pub fn rhythm_line(cadence: &CadenceDetail, now: DateTime<Utc>) -> String; // "Rhythm: " + text
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String;
pub fn rhythm_index(cadence: &CadenceDetail) -> usize;             // 0 Auto, else 1 + the group's index
pub fn rhythm_choice(index: usize) -> Option<Option<CadenceGroup>>; // Some(None) = Auto
pub fn theme_names(themes: &[ThemeDef]) -> Vec<String>;            // "None", then names by position
pub fn theme_index(themes: &[ThemeDef], key: Option<&str>) -> usize;
pub fn theme_choice(themes: &[ThemeDef], index: usize) -> Option<Option<String>>;
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot { pub rows: Vec<GardenRow>, pub unread: HashMap<FeedId, usize>, pub cadence: HashMap<FeedId, CadenceDetail>,
    pub themes: Vec<ThemeDef>, pub theme_keys: HashMap<FeedId, String> }
pub fn load(store: &Store) -> ressac_core::Result<Snapshot>;
/// `garden::check_decision` then `garden::decide`: the job of a decision run.
pub fn decide(store: &Store, feed: FeedId, decision: Decision, now: DateTime<Utc>) -> ressac_core::Result<()>;
```

**`ressac_ui::garden::{GardenVisit, Inspector, RhythmControl, LogLine}`** (`visit.rs`, `inspector.rs`, re-exported from `garden`; Task 3):

```rust
#[derive(Debug, Clone, Default)]
pub struct GardenVisit { /* scope, snapshot, seen_in_queue, decided_this_visit, selected, grouping, message, confirming */ }
impl GardenVisit {
    pub fn enter(&mut self, scope: GardenScope);  // another scope: a new visit (snapshot, memory, selection, message, confirmation cleared)
    pub fn leave(&mut self);
    pub fn scope(&self) -> Option<GardenScope>;
    pub fn update(&mut self, snapshot: Snapshot); // the Mac's rowsDidChange
    pub fn snapshot(&self) -> &Snapshot;
    pub fn row(&self, feed: FeedId) -> Option<&GardenRow>;
    pub fn visible(&self) -> Vec<&GardenRow>;
    pub fn sections(&self) -> Vec<Section>;       // rhythm sections in All Feeds grouped by rhythm, status sections otherwise
    pub fn ordered(&self) -> Vec<FeedId>;
    pub fn is_dimmed(&self, feed: FeedId) -> bool;
    pub fn grouping(&self) -> Grouping;
    pub fn set_grouping(&mut self, grouping: Grouping);
    pub fn selected(&self) -> Option<FeedId>;
    pub fn select(&mut self, feed: FeedId) -> bool; // visible feeds only; true when it changed
    pub fn count_label(&self) -> String;
    pub fn safe_repairs(&self) -> Vec<(FeedId, Action)>;
    pub fn key(&mut self, key: &str, modifiers: u32) -> Option<Command>;
    pub fn decided(&mut self, feed: FeedId);
    pub fn undone(&mut self, feed: Option<FeedId>);
    pub fn message(&self) -> Option<&'static str>;
    pub fn set_message(&mut self, message: Option<&'static str>);
    pub fn confirming(&self) -> bool;
    pub fn set_confirming(&mut self, on: bool);
    pub fn empty_text(&self) -> Option<&'static str>;
    pub fn inspector<Tz: TimeZone>(&self, log: &[AttemptRecord], now: DateTime<Utc>, tz: &Tz) -> Option<Inspector> where Tz::Offset: Display;
}
pub const NO_FEED_TITLE: &str = "No Feed Selected";
pub const NO_FEED_TEXT: &str = "Select a feed to see why it is in the garden.";
pub const BURIED_NOTE: &str = "Buried. Press k to restore it.";
pub const HISTORY_TITLE: &str = "Redirects and fetch log";
pub const NOT_FETCHED: &str = "Not fetched yet.";
pub const ARCHIVED_COPY: &str = "Archived copy";
#[derive(Debug, Clone, PartialEq, Eq)] pub struct RhythmControl { pub line: String, pub index: usize }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct LogLine { pub at: String, pub outcome: String }   // "Oct 10, 2026, 7:00 AM", "HTTP 410"
#[derive(Debug, Clone, PartialEq)]
pub struct Inspector {
    pub feed_id: FeedId, pub title: String, pub folder_path: String,
    pub status: Status, pub status_line: String, pub status_color: &'static str, pub buried: bool, pub reason: String,
    pub rhythm: Option<RhythmControl>,  // active feeds with a `v_cadence` row
    pub theme: Option<usize>,           // index into `theme_names`; None for a buried feed
    pub evidence: Vec<String>, pub remarks: Vec<String>,
    pub proposed: Option<String>,       // the action's label, active feeds only
    pub decision: Option<String>,       // "Keep", "Repair", "Bury"
    pub feed_url: String, pub site: Option<String>, pub wayback: Option<String>, // web addresses only
    pub keep_title: &'static str,       // "Keep" or "Restore"
    pub can_repair: bool, pub can_bury: bool, pub bury_prominent: bool,
    pub redirects: Vec<String>, pub log: Vec<LogLine>,
}
impl Inspector {
    pub fn of<Tz: TimeZone>(row: &GardenRow, cadence: Option<&CadenceDetail>, themes: &[ThemeDef], theme_key: Option<&str>,
        log: &[AttemptRecord], now: DateTime<Utc>, tz: &Tz) -> Inspector where Tz::Offset: Display;
}
```

**`ressac-kde`** (Tasks 4–5):

- **`Garden`** (QML singleton, `#[base = QAbstractListModel]`, `impl cxx_qt::Threading`): roles `feedId` (`i64`), `glyph`, `title`, `clue`, `rhythm` (`""` outside All Feeds or without a rhythm), `unread` (`""` = none), `chip` (`""` = none), `dimmed` (`bool`), `tooltip`, `section` (the section header); properties (`READ`, `NOTIFY = state_changed`) `active: bool`, `scopeTitle`, `countLabel`, `showsGrouping: bool`, `grouping: QString` (also `WRITE`), `showsApply: bool`, `safeRepairCount: i32`, `applyTitle`, `applyQuestion`, `applyPreview`, `applyConfirmLabel`, `confirming: bool` (also `WRITE`), `message` (`""` = none), `emptyText` (`""` = rows show), `sectionTitles: QStringList`; (`NOTIFY = selection_changed`) `selectedRow: i32`, `selectedFeedId: i64` (−1 none); (`NOTIFY = inspector_changed`) `hasSelection: bool`, `title`, `folderPath`, `statusLine`, `statusColor` (`""` = secondary), `buried: bool`, `reason`, `rhythmLine` (`""` = hidden), `rhythmIndex: i32` (−1 hidden), `themeNames: QStringList`, `themeIndex: i32` (−1 hidden), `evidence: QStringList`, `remarks: QStringList`, `proposed`, `decision`, `feedUrl`, `site`, `wayback` (`""` = none), `keepTitle`, `canRepair`, `canBury`, `buryProminent: bool`, `redirects: QStringList`, `logDates: QStringList`, `logOutcomes: QStringList`; `CONSTANT`: `groupingTitles`, `rhythmChoices: QStringList`, `noFeedTitle`, `noFeedText`, `buriedNote`, `historyTitle`, `notFetched`, `archivedCopy`. Invokables `show(key: QString)`, `reload()`, `select(row: i32)`, `selectFeed(feedId: i64) -> bool`, `rowOfFeed(feedId: i64) -> i32`, `key(text: QString, modifiers: i32) -> bool`, `decide(decision: QString)` (`"keep" | "repair" | "bury"`), `undo()`, `applySafe()`, `setCadence(index: i32)`, `setTheme(index: i32)`. Signals `stateChanged()`, `selectionChanged()`, `inspectorChanged()`, `openUrl(url: QString)`.
- **QML:** `GardenPane.qml` (`function focusList()`, `function acceptRepairs()`, its `ListView` `objectName: "gardenList"`), `GardenInspector.qml`; `ReadingPage.qml` gains `function focusGarden()`, `readonly property alias garden: gardenPane`, `focusName()` answers `"garden"`; `Main.qml` gains `readonly property var gardenSteps` and the chooser `Recette.walk === "garden" ? gardenSteps : …`.

**Consumed:** B2's `sources::{GardenScope, Selection, rhythm_title, status_color}`, `RunRequest::{Decide, Undo, ApplySafe, SetCadence, SetTheme}` and their refusal classes, `live::plan_rows`, `SourcesModel.selectKey`, `ReadingPage.reloadAll()`, the panes' ids; B3's `keys::qt::{SHIFT, CONTROL, ALT, META}`, `sanitize::is_web_url`, `ReaderPane.openExternally(url)` (logs `reader: open <url>` in recette), `Recette.key`, `ReadingPage.perform`, `focusName`, `focusSidebar`, the key filter (a letter whose verb `perform` declines reaches the focused item); B4's `runs::submit_short`, `RunEnd`, `Recette.walk`, `Main.qml`'s `steps`; core `garden::{load_rows, check_decision, decide, undo, apply_safe_repairs, safe_repairs, progress, GardenRow, Progress}`, `Store::{cadence_details, themes, set_feed_theme, attempts}`, `Ressac::set_cadence`, `model::{Status, Action, Decision, Remark, ErrorKind, Hop, CadenceGroup}`, `store::{AttemptRecord, CadenceDetail, DecisionRecord, ThemeDef}`.

**Decisions this phase pins:**

- **One visit, as the Mac's `GardenStore`:** entering a scope resets the queue's memory (`seen_in_queue`: every feed seen needing triage during the visit; `decided_this_visit`: decided here) and the selection; retained = their union. Needs Triage lists feeds needing triage plus retained ones (dimmed once decided); All Feeds lists active feeds plus retained buried ones (dimmed); Cemetery lists buried feeds. After each reload the selection moves on (Needs Triage) as `selection_after_update`, and falls back to the first feed needing triage, else the first row.
- **Runs:** decisions (`garden::decide` = check then decide, the CLI's refusals as banners without Show Log), undo, apply-safe, rhythm (`Ressac::set_cadence`) and theme (`Store::set_feed_theme`) are short runs through `submit_short`; the reload comes from `Runs.databaseChanged` → `reloadAll()`; `Done` of a decision adds the feed to `decided_this_visit`, `Done(Some(id))` of an undo removes it, then the rows are re-planned.
- **Data:** `garden::load` on the GUI store (core `load_rows`, `buried_unread`, `cadence_details`, `themes`, `feed_theme_keys`) only while a garden scope is shown; the fetch log (`LOG_LINES` = 20) for the selected feed only. A read error sets the header message "The garden could not be read." and keeps the last rows.
- **Keys:** the table (`objectName: "gardenList"`) takes the keyboard when a garden scope is entered (inventory §4.2); `Keys.onPressed` hands `event.text` and `event.modifiers` to `Garden.key`, which returns whether the key was the garden's; arrows stay the `ListView`'s. Ctrl+Z is the header Undo button's `QQC2.Action { shortcut: StandardKey.Undo }`.
- **Time:** the rhythm line and the log dates use `Utc::now()` and `chrono::Local` at each reload or selection change.

**Step 0:** `library.rs` (core, 1700+ lines), `chart.rs`, `SourcesPane.qml` and `Main.qml` only gain items. `ReadingPage.qml` has its pane slots restructured in Task 5: when it is over 300 lines, Task 5 starts by removing its dead code (unused ids, properties and functions, `console.log` debugging) in its own commit, or records "none" in the ledger.

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

## Global Constraints

- **Runs:** short runs FIFO on one worker, allowed during a long run; one coordinator for banners and reloads.
- **Language:** UI strings in English and identical to the Mac app's (inventory §4 quotes them), except the two tooltips of deviation 5.
- **Shortcuts:** spec §7.2: Ctrl+Z garden Undo; garden keys `u`, `R` (Shift+R), `k`, `r`, `b`, `o`, `w`, any Control, Alt or Meta chord ignored; arrows navigate.
- **Time:** functions depending on now or the zone take them as parameters.
- **Never:** the real database; the network in tests; push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **A chord on a garden key** (Ctrl+B, Alt+K, Ctrl+Shift+R, Meta+U) → nothing decided, buried or undone; Shift+R still asks for the safe repairs. Test: Task 2 `garden_keys_ignore_control_alt_and_meta_chords`.
2. **The selected feed is decided, here or by `ressac garden` in a terminal, while the queue is shown** → it stays listed and dimmed until the scope is left, the selection moves to the next feed needing triage and wraps. Tests: Task 2 `selection_moves_on_when_the_selected_feed_leaves_the_queue`; Task 3 `a_feed_decided_here_or_in_a_terminal_stays_listed_dimmed_until_the_scope_is_left`.
3. **A site or Wayback address that is not a web address** (`javascript:`, `file:`, missing) → `o`/`w` say why and open nothing, the inspector shows no link and its button is disabled. Tests: Task 2 `keys_map_like_the_terminal_garden`; Task 3 `the_inspector_shows_web_addresses_only`.
4. **A decision the store refuses** (repair or bury on a buried feed, a repair whose address another active feed already uses) → the CLI's sentence as a banner without Show Log, the row unchanged; the keys say the buried rule before any run. Tests: Task 2 `a_decision_is_checked_before_it_is_recorded`; B2's `RunRequest::is_refusal` cases.
5. **Many safe repairs** (87) → the dialog lists 8 "Title: label" lines and "and 79 more.", Apply All runs once, Cancel nothing; the Cemetery shows no Apply button. Tests: Task 2 `safe_repairs_are_undecided_active_safe_actions`; Task 5 garden walk (`06-confirm`, `09-applied`, `19-cemetery`).

---

## B5a: core reads, the garden's pure rules

### Task 1 (B5a): `Store::buried_unread`, `Store::feed_theme_keys`

**Files:**
- Modify: `crates/ressac-core/src/store/library.rs` (`buried_unread`, test in its `mod tests`)
- Modify: `crates/ressac-core/src/store/chart.rs` (`feed_theme_keys`, test in its `mod tests`)

**Interfaces:**
- Consumes: nothing new.
- Produces: the two reads (header).

SQL: `SELECT e.feed_id, COUNT(*) FROM entry e JOIN feed f ON f.id = e.feed_id WHERE f.state = 'buried' AND e.read_at IS NULL GROUP BY e.feed_id`; `SELECT f.id, t.key FROM feed f JOIN theme t ON t.id = f.theme_id`.

- [ ] **Step 1: Write the failing tests.** `library.rs`: `buried_unread_counts_the_unread_entries_of_buried_feeds_only` — with the module's helpers: `add_feed(&s, 1, None, 1, None, "active", "daily")`, `add_feed(&s, 4, None, 2, None, "buried", "daily")`; `add_entry` ids 1 (feed 1, unread), 2 and 3 (feed 4, unread), 4 (feed 4, read): `assert_eq!(s.buried_unread().unwrap(), HashMap::from([(4, 2)]))`; after `UPDATE entry SET read_at = '2026-10-16T08:00:00.000Z' WHERE feed_id = 4` → empty map. `chart.rs`: `feed_theme_keys_map_each_themed_feed_to_its_key` — `store_with_feeds(2)`, `import_chart(&import(&["a", "b"], &[], vec![]), false)`, `set_feed_theme(2, Some("b"))` → `HashMap::from([(2, "b".to_string())])`; `set_feed_theme(2, None)` → empty.
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-core buried_unread feed_theme_keys 2>&1 | tail -3` → compile errors (methods missing).
- [ ] **Step 3: Implement** both methods.
- [ ] **Step 4: Verify** — `cargo test -p ressac-core 2>&1 | grep "test result"` → every `ok.`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/library.rs crates/ressac-core/src/store/chart.rs
git commit -m "Core: unread entries of buried feeds, each feed's theme key

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B5a): `ressac_ui::garden` rules

**Files:**
- Create: `crates/ressac-ui/src/garden/mod.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod garden;`)
- Test helper (not counted): `crates/ressac-ui/src/test_db.rs` gains `TestDb::full() -> TestDb` (a copy of the contract fixture left as it is: diagnoses, decisions, fetch log, buried feed 4).

**Interfaces:**
- Consumes: Task 1; B2 `sources::{GardenScope, rhythm_title, status_color}`; B3 `keys::qt`, `sanitize::is_web_url`; core garden and store items of the header.
- Produces: `garden/mod.rs` items of the header.

Rules (`GardenModel.swift`, inventory §4.2):
- `visible_rows`: NeedsTriage → `needs_triage() || retained`; All → `!buried || retained`; Cemetery → `buried`. `is_dimmed`: NeedsTriage → `!needs_triage() && retained`; All → `buried && retained`; Cemetery → false.
- `sections`: Cemetery → one `"Buried"` section (`BURIED_GLYPH`) of the buried rows; otherwise one section per `Status::ALL` entry in that order holding the rows of that status (buried rows included), omitted when empty. Inside a section: `(top_folder, title.to_lowercase())`. `rhythm_sections`: `CadenceGroup::ALL` then `None` ("Unchecked"), by each row's `cadence.effective_group`, same order inside; ids `rhythm_title(group)` / `"Unchecked"`. `Section::header` = `"{glyph} {ID UPPERCASE} · {count}"`.
- `count_label`: NeedsTriage → `"{needing − decided} of {needing} left"` from `progress(rows)`; otherwise `"1 feed"` / `"{n} feeds"` of `visible`.
- `safe_repair_preview`: the first 8 repairs as `"{title}: {action.label()}"`, then `"and {n − 8} more."`, joined by `"\n"`.
- `selection_after_update`: when the selected feed needed triage in `previous` and does not in `current`, the first feed after it in `ordered` (wrapping) that needs triage in `current`, else unchanged.
- `command`: a chord with Control, Alt or Meta → `None`; `"u"` → Undo; `"R"` → ConfirmSafeRepairs, or `Message(NO_SAFE_REPAIRS)` at 0; `"k" "r" "b" "o" "w"` without a row → `Message(SELECT_FIRST)`; `"k"` → Decide(Keep); `"b"`/`"r"` on a buried row → `Message(BURIED_FIRST)`; `"b"` → Decide(Bury); `"r"` → Decide(Repair) when the action `is_repair()`, else `Message(NOTHING_TO_REPAIR)`; `"o"` / `"w"` → `Open(address)` when `is_web_url`, `Message(NON_WEB)` when present but not web, `Message(NO_SITE)` / `Message(NO_WAYBACK)` when missing; anything else → `None`.
- `chip`: a decision and not needing triage → `"kept"` / `"repaired"` / `"buried"`; else the action → `"https"` / `"follow"` / `"adopt"` / `"merge"` / `"bury"`; no action and a triage status → `"wait"`; else `None`.
- `remark_sentence`: "The https:// address serves the same feed.", "Only served over http://.", "A feed in the same folder has a similar title (#12).", "Most entries carry no date; Ressac used the fetch date.", "The last fetch was blocked (403 or an anti-bot page).".
- `rhythm_text`: `rhythm_title(effective)`, `"median {:.1} d"` under 10 else `"median {rounded} d"`, `"last entry {ago}"`, joined by `" · "`, plus `" (set by you)"` when overridden; `ago`: minutes (floored at 0) under 60 `"N min ago"`, hours under 48 `"N h ago"`, days under 60 `"N d ago"`, else `"{days / 30} mo ago"`.
- `load`: `load_rows`, `buried_unread`, `cadence_details` (by feed), `themes`, `feed_theme_keys`.

- [ ] **Step 1: Write the failing tests** in `garden/mod.rs`. Helper (test-only): `fn row(id: FeedId, title: &str, status: Option<Status>) -> GardenRow` (folder `"Blogs"`, active, xml `https://feeds.example.org/<id>`, a `Diagnosis` with that status and empty reason/evidence/remarks when `Some`, no decision, no attempts) and a builder trait with `.folder(Option<&str>)` (sets `folder_path` and `top_folder`), `.buried()`, `.action(Action)`, `.decided(Decision)` (a `DecisionRecord` at the row's status, decided 2026-10-10T08:00Z), `.site(&str)`, `.wayback(&str)`; `fn cadence(id, group, overridden: bool, median: Option<f64>, last: Option<DateTime<Utc>>) -> CadenceDetail`; `fn follow(url) -> Action`; `use Status::*;`.

```rust
#[test]
fn only_buried_feeds_show_their_unread_count() {
    assert_eq!(unread_label(&row(4, "Gone", Some(Dead)).buried(), 12).as_deref(), Some("12 unread"));
    assert_eq!(unread_label(&row(4, "Gone", Some(Dead)).buried(), 0), None);
    assert_eq!(unread_label(&row(1, "Live", Some(Alive)), 3), None);
}

#[test]
fn sections_follow_urgency_then_folder_then_title() {
    let rows = [row(1, "Zed", Some(Dead)), row(2, "alpha", Some(Dead)), row(3, "Mover", Some(Moved)).folder(Some("Infos")),
        row(4, "Fine", Some(Alive)), row(5, "Gone", Some(Dead)).buried(), row(6, "New", None).folder(None),
        row(7, "Early", Some(Dead)).folder(Some("Biblis / Sub"))];
    let all: Vec<&GardenRow> = rows.iter().collect();
    let s = sections(&all, GardenScope::All);
    assert_eq!(s.iter().map(|s| s.id).collect::<Vec<_>>(), ["Moved", "Dead", "Alive", "Unchecked"]);
    assert_eq!(s[1].rows, [7, 2, 5, 1]);
    assert_eq!(s[1].header(), "🪦 DEAD · 4");
    let buried: Vec<&GardenRow> = rows.iter().filter(|r| r.buried).collect();
    let cemetery = sections(&buried, GardenScope::Cemetery);
    assert_eq!((cemetery.len(), cemetery[0].id, cemetery[0].rows.as_slice()), (1, "Buried", &[5][..]));
    assert_eq!(cemetery[0].header(), "⚰️ BURIED · 1");
    let queue = sections(&all, GardenScope::NeedsTriage);
    assert_eq!(queue.iter().find(|s| s.id == "Dead").unwrap().rows, [7, 2, 5, 1]);
    assert!(!queue.iter().any(|s| s.id == "Buried"));
}

#[test]
fn the_queue_keeps_feeds_decided_during_the_visit_dimmed() {
    let rows = [row(1, "Waiting", Some(Dead)), row(2, "Decided", Some(Dead)).decided(Decision::Keep),
                row(3, "Decided last week", Some(Fossil)).decided(Decision::Keep)];
    let ids = |v: Vec<&GardenRow>| v.iter().map(|r| r.feed_id).collect::<Vec<_>>();
    assert_eq!(ids(visible_rows(&rows, GardenScope::NeedsTriage, &HashSet::from([1, 2]))), [1, 2]);
    assert_eq!(ids(visible_rows(&rows, GardenScope::NeedsTriage, &HashSet::from([1]))), [1]);
    assert!(is_dimmed(&rows[1], GardenScope::NeedsTriage, &HashSet::from([1, 2])));
    assert!(!is_dimmed(&rows[0], GardenScope::NeedsTriage, &HashSet::from([1, 2])));
    assert!(!is_dimmed(&rows[1], GardenScope::All, &HashSet::from([1, 2])));
    let with_buried = [rows[0].clone(), row(4, "Gone", Some(Dead)).buried()];
    assert_eq!(ids(visible_rows(&with_buried, GardenScope::Cemetery, &HashSet::new())), [4]);
}

#[test]
fn all_feeds_keeps_a_feed_buried_during_the_visit_dimmed() {
    let rows = [row(1, "Active", Some(Alive)), row(2, "Buried now", Some(Dead)).buried().decided(Decision::Bury),
                row(3, "Buried before", Some(Dead)).buried().decided(Decision::Bury)];
    let ids = |v: Vec<&GardenRow>| v.iter().map(|r| r.feed_id).collect::<Vec<_>>();
    assert_eq!(ids(visible_rows(&rows, GardenScope::All, &HashSet::from([2]))), [1, 2]);
    assert_eq!(ids(visible_rows(&rows, GardenScope::All, &HashSet::new())), [1]);
    assert!(is_dimmed(&rows[1], GardenScope::All, &HashSet::from([2])));
    assert!(!is_dimmed(&rows[0], GardenScope::All, &HashSet::from([2])));
}

/// Review Focus 1.
#[test]
fn garden_keys_ignore_control_alt_and_meta_chords() {
    let dead = row(1, "Dead", Some(Dead));
    for chord in [qt::CONTROL, qt::ALT, qt::META, qt::CONTROL | qt::SHIFT] {
        for key in ["b", "k", "u", "R", "o"] {
            assert_eq!(command(key, chord, Some(&dead), 1), None, "{key} {chord:#x}");
        }
    }
    assert_eq!(command("R", qt::SHIFT, Some(&dead), 1), Some(Command::ConfirmSafeRepairs));
    assert_eq!(command("b", 0, Some(&dead), 0), Some(Command::Decide(Decision::Bury)));
}

#[test]
fn progress_counts_like_the_terminal_garden() {
    let rows = [row(1, "Dead", Some(Dead)), row(2, "Fossil kept", Some(Fossil)).decided(Decision::Keep),
        row(3, "Buried", Some(Dead)).buried().decided(Decision::Bury), row(4, "Alive", Some(Alive)), row(5, "Unchecked", None)];
    assert_eq!(count_label(GardenScope::NeedsTriage, &rows, 2), "1 of 3 left");
    assert_eq!(count_label(GardenScope::All, &rows, 1), "1 feed");
    assert_eq!(count_label(GardenScope::Cemetery, &rows, 4), "4 feeds");
}

#[test]
fn safe_repairs_are_undecided_active_safe_actions() {
    let https = Action::UpgradeHttps { url: "https://a.example.org/feed".into() };
    let rows = vec![row(1, "A", Some(Alive)).action(https.clone()), row(2, "B", Some(Moved)).action(follow("https://b.example.org/feed")),
        row(3, "C", Some(Moved)).action(follow("https://c.example.org/feed")).decided(Decision::Keep),
        row(4, "D", Some(Alive)).action(https.clone()).buried(), row(5, "E", Some(Duplicate)).action(Action::MergeInto { feed_id: 1 }),
        row(6, "F", Some(Dead)).action(Action::Bury)];
    let repairs = safe_repairs(&rows);
    assert_eq!(repairs.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(safe_repair_preview(&rows, &repairs),
               "A: switch to https://a.example.org/feed\nB: follow redirect to https://b.example.org/feed");
    let many: Vec<GardenRow> = (1..=87).map(|i| row(i, &format!("Feed {i}"), Some(Alive)).action(https.clone())).collect();
    let preview = safe_repair_preview(&many, &safe_repairs(&many));
    assert_eq!(preview.lines().count(), 9);
    assert_eq!(preview.lines().last(), Some("and 79 more."));
    assert_eq!([apply_button_title(1), apply_button_title(87)], ["Apply 1 Safe Repair", "Apply 87 Safe Repairs"]);
    assert_eq!([apply_question(1), apply_question(87)], ["Apply 1 safe repair?", "Apply 87 safe repairs?"]);
    assert_eq!([apply_confirm_label(1), apply_confirm_label(87)], ["Apply", "Apply All"]);
}

#[test]
fn selection_moves_on_when_the_selected_feed_leaves_the_queue() {
    let before = [row(1, "A", Some(Dead)), row(2, "B", Some(Dead)).decided(Decision::Keep), row(3, "C", Some(Dead))];
    let order = [1, 2, 3];
    let after = [before[0].clone().decided(Decision::Keep), before[1].clone(), before[2].clone()];
    assert_eq!(selection_after_update(Some(1), &before, &after, &order), Some(3));
    let last = [before[0].clone(), before[1].clone(), before[2].clone().decided(Decision::Keep)];
    assert_eq!(selection_after_update(Some(3), &before, &last, &order), Some(1));          // wraps
    assert_eq!(selection_after_update(Some(3), &before, &before, &order), Some(3));
    let done = before.clone().map(|r| r.decided(Decision::Keep));
    assert_eq!(selection_after_update(Some(1), &before, &done, &order), Some(1));
    assert_eq!(selection_after_update(None, &before, &after, &order), None);
}

/// Review Focus 3.
#[test]
fn keys_map_like_the_terminal_garden() {
    let dead = row(1, "Dead", Some(Dead)).action(Action::Bury).site("https://dead.example.org/")
        .wayback("http://web.archive.org/web/2021/https://dead.example.org/");
    let fossil = row(2, "Fossil", Some(Fossil)).site("javascript:alert(1)").wayback("file:///etc/passwd");
    let buried = row(3, "Buried", Some(Dead)).buried().decided(Decision::Bury);
    let moved = row(4, "Moved", Some(Moved)).action(follow("https://m.example.org/feed"));
    let c = |key: &str, r: Option<&GardenRow>, n| command(key, 0, r, n);
    assert_eq!(c("k", Some(&dead), 0), Some(Command::Decide(Decision::Keep)));
    assert_eq!(c("b", Some(&dead), 0), Some(Command::Decide(Decision::Bury)));
    assert_eq!(c("r", Some(&moved), 0), Some(Command::Decide(Decision::Repair)));
    assert_eq!(c("r", Some(&dead), 0), Some(Command::Message("Nothing to repair for this feed.")));
    assert_eq!(c("r", Some(&fossil), 0), Some(Command::Message("Nothing to repair for this feed.")));
    assert_eq!(c("k", Some(&buried), 0), Some(Command::Decide(Decision::Keep)));
    assert_eq!(c("b", Some(&buried), 0), Some(Command::Message("This feed is buried; press k to restore it first.")));
    assert_eq!(c("r", Some(&buried), 0), Some(Command::Message("This feed is buried; press k to restore it first.")));
    assert_eq!(c("o", Some(&dead), 0), Some(Command::Open("https://dead.example.org/".into())));
    assert_eq!(c("w", Some(&dead), 0), Some(Command::Open("http://web.archive.org/web/2021/https://dead.example.org/".into())));
    assert_eq!(c("o", Some(&fossil), 0), Some(Command::Message("Refusing to open a non-web address.")));
    assert_eq!(c("w", Some(&fossil), 0), Some(Command::Message("Refusing to open a non-web address.")));
    assert_eq!(c("o", Some(&moved), 0), Some(Command::Message("This feed has no site address.")));
    assert_eq!(c("w", Some(&moved), 0), Some(Command::Message("No Wayback snapshot yet (run `ressac check`).")));
    assert_eq!(c("u", None, 0), Some(Command::Undo));
    assert_eq!(c("R", None, 3), Some(Command::ConfirmSafeRepairs));
    assert_eq!(c("R", None, 0), Some(Command::Message("No safe repairs pending.")));
    assert_eq!(c("k", None, 0), Some(Command::Message("Select a feed first.")));
    assert_eq!(c("j", Some(&dead), 0), None);
    assert_eq!(c("K", Some(&dead), 0), None, "Caps Lock: not the garden's k");
}

#[test]
fn labels_match_the_core() {
    assert_eq!(chip(&row(1, "A", Some(Duplicate)).action(Action::MergeInto { feed_id: 2 })), Some("merge"));
    assert_eq!(chip(&row(1, "A", Some(Ailing))), Some("wait"));
    assert_eq!(chip(&row(1, "A", Some(Alive))), None);
    assert_eq!(chip(&row(1, "A", Some(Moved)).action(follow("x")).decided(Decision::Repair)), Some("repaired"));
    assert_eq!(chip(&row(1, "A", Some(Dead)).buried().decided(Decision::Bury)), Some("buried"));
    assert_eq!(remark_sentence(&Remark::SimilarTitle { feed_id: 12 }), "A feed in the same folder has a similar title (#12).");
    assert_eq!(remark_sentence(&Remark::HttpsAvailable), "The https:// address serves the same feed.");
    let attempt = |status: Option<u16>, error: Option<ErrorKind>| AttemptRecord { at: at("2026-10-10T07:00:00Z"), http_status: status,
        error, redirect_chain: vec![], content_type: None, network_suspect: false, not_modified: false, entry_count: None };
    assert_eq!(outcome(&attempt(None, Some(ErrorKind::Dns))), "Dns");
    assert_eq!(outcome(&attempt(Some(404), Some(ErrorKind::Http(404)))), "HTTP 404");
    assert_eq!(outcome(&attempt(Some(200), Some(ErrorKind::HtmlInsteadOfFeed))), "HTML page instead of a feed");
    assert_eq!(outcome(&attempt(Some(403), Some(ErrorKind::Challenge))), "anti-bot challenge page");
    assert_eq!(outcome(&attempt(Some(200), Some(ErrorKind::TooLarge))), "response too large");
    assert_eq!(outcome(&attempt(None, Some(ErrorKind::Timeout))), "Timeout");
    assert_eq!(outcome(&attempt(Some(200), None)), "HTTP 200");
    assert_eq!(outcome(&attempt(None, None)), "no response");
    assert_eq!(redirect_line(&Hop { status: 301, url: "https://e.org/feed".into() }), "301 → https://e.org/feed");
    assert_eq!(status_line(Dead), "🪦 Dead");
    assert_eq!(tooltip(&row(1, "A", Some(Dead)).folder(Some("Blogs / potes"))), "Blogs / potes", "an empty reason is left out");
    assert_eq!([empty_message(GardenScope::NeedsTriage), empty_message(GardenScope::All), empty_message(GardenScope::Cemetery)],
               ["Nothing to triage", "No feeds", "The cemetery is empty"]);
}

#[test]
fn the_rhythm_line_reads_like_the_spec() {
    let now = at("2026-10-16T09:00:00Z");
    let two_hours = Some(now - TimeDelta::hours(2));
    assert_eq!(rhythm_line(&cadence(1, CadenceGroup::Daily, false, Some(0.4), two_hours), now),
               "Rhythm: Daily · median 0.4 d · last entry 2 h ago");
    assert_eq!(rhythm_line(&cadence(1, CadenceGroup::Daily, true, Some(0.4), two_hours), now),
               "Rhythm: Daily · median 0.4 d · last entry 2 h ago (set by you)");
    assert_eq!(rhythm_line(&cadence(2, CadenceGroup::Monthly, false, Some(12.3), Some(now - TimeDelta::days(3))), now),
               "Rhythm: Monthly · median 12 d · last entry 3 d ago");
    assert_eq!(rhythm_line(&cadence(3, CadenceGroup::Silent, false, None, None), now), "Rhythm: Silent");
    assert_eq!(rhythm_text(&cadence(1, CadenceGroup::Daily, true, Some(0.4), two_hours), now),
               "Daily · median 0.4 d · last entry 2 h ago (set by you)");
}

#[test]
fn ages_count_minutes_hours_days_then_months() {
    let now = at("2026-10-16T09:00:00Z");
    assert_eq!(ago(now - TimeDelta::minutes(45), now), "45 min ago");
    assert_eq!(ago(now - TimeDelta::hours(47), now), "47 h ago");
    assert_eq!(ago(now - TimeDelta::days(59), now), "59 d ago");
    assert_eq!(ago(now - TimeDelta::days(400), now), "13 mo ago");
    assert_eq!(ago(now + TimeDelta::minutes(1), now), "0 min ago");        // a clock ahead of the feed
}

#[test]
fn the_label_marks_a_group_set_by_hand() {
    assert_eq!(rhythm_label(Some(&cadence(1, CadenceGroup::Daily, false, None, None))).as_deref(), Some("Daily"));
    assert_eq!(rhythm_label(Some(&cadence(1, CadenceGroup::Rare, true, None, None))).as_deref(), Some("Rare*"));
    assert_eq!(rhythm_label(None), None);
}

#[test]
fn all_feeds_groups_by_rhythm_then_unchecked() {
    let rows = [row(1, "Zeta", Some(Alive)), row(2, "Alpha", Some(Dead)), row(3, "Mid", Some(Alive)), row(4, "New", None)];
    let all: Vec<&GardenRow> = rows.iter().collect();
    let groups = HashMap::from([(1, cadence(1, CadenceGroup::Daily, false, None, None)), (2, cadence(2, CadenceGroup::Daily, false, None, None)),
                                (3, cadence(3, CadenceGroup::Rare, true, None, None))]);
    let s = rhythm_sections(&all, &groups);
    assert_eq!(s.iter().map(|s| s.rows.clone()).collect::<Vec<_>>(), [vec![2, 1], vec![3], vec![4]]);
    assert_eq!(s.iter().map(Section::header).collect::<Vec<_>>(), ["⚡ DAILY · 2", "✨ RARE · 1", "❔ UNCHECKED · 1"]);
    assert_eq!(Grouping::ALL.map(Grouping::title), ["Status", "Rhythm"]);
}

#[test]
fn the_rhythm_menu_is_auto_then_the_five_groups() {
    assert_eq!(RHYTHM_CHOICES, ["Auto", "Daily", "Weekly", "Monthly", "Rare", "Silent"]);
    assert_eq!(rhythm_index(&cadence(1, CadenceGroup::Rare, true, None, None)), 4);
    assert_eq!(rhythm_index(&cadence(1, CadenceGroup::Rare, false, None, None)), 0);
    assert_eq!(rhythm_choice(0), Some(None));
    assert_eq!(rhythm_choice(1), Some(Some(CadenceGroup::Daily)));
    assert_eq!(rhythm_choice(5), Some(Some(CadenceGroup::Silent)));
    assert_eq!(rhythm_choice(6), None);
}

#[test]
fn the_theme_menu_is_none_then_the_themes_by_position() {
    let theme = |key: &str, name: &str| ThemeDef { key: key.into(), name: name.into(), sea: format!("SEA OF {name}") };
    let themes = [theme("lit", "Literature"), theme("tech", "Technology")];          // as `Store::themes` orders them
    assert_eq!(theme_names(&themes), ["None", "Literature", "Technology"]);
    assert_eq!([theme_index(&themes, Some("tech")), theme_index(&themes, None), theme_index(&themes, Some("gone"))], [2, 0, 0]);
    assert_eq!(theme_choice(&themes, 1), Some(Some("lit".to_string())));
    assert_eq!(theme_choice(&themes, 0), Some(None));
    assert_eq!(theme_choice(&themes, 3), None);
}

#[test]
fn the_theme_menu_writes_the_feed_theme() {
    let db = TestDb::full();
    db.execute("INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), \
                ('lit', 'Literature', 'SEA OF LETTERS', 1)");
    let store = db.store();
    store.set_feed_theme(8, Some("lit")).unwrap();
    let snapshot = load(&store).unwrap();
    assert_eq!(theme_names(&snapshot.themes), ["None", "Literature", "Technology"]);
    assert_eq!(snapshot.theme_keys.get(&8).map(String::as_str), Some("lit"));
    store.set_feed_theme(8, None).unwrap();
    assert!(load(&store).unwrap().theme_keys.is_empty());
}

/// Review Focus 4.
#[test]
fn a_decision_is_checked_before_it_is_recorded() {
    let db = TestDb::full();
    let store = db.store();
    let refused = decide(&store, 4, Decision::Repair, at("2026-10-16T09:00:00Z")).unwrap_err();
    assert_eq!(refused.to_string(), "Feed #4 is buried; only keep restores it.");
    assert!(RunRequest::Decide(4, Decision::Repair).failure(&JobFailure::Core(refused)).refusal);
    decide(&store, 6, Decision::Keep, at("2026-10-16T09:00:00Z")).unwrap();
    let rows = load(&store).unwrap().rows;
    let six = rows.iter().find(|r| r.feed_id == 6).unwrap();
    assert_eq!(six.decision.as_ref().map(|d| d.decision), Some(Decision::Keep));
    assert!(!six.needs_triage());
}

#[test]
fn load_reads_the_fixture_like_the_mac_app() {
    let db = TestDb::full();
    db.entry(4, "2026-10-09T08:00:00.000Z", false);
    db.entry(4, "2026-10-09T09:00:00.000Z", false);
    db.entry(4, "2026-10-09T10:00:00.000Z", true);
    let s = load(&db.store()).unwrap();
    let by_id = |id| s.rows.iter().find(|r| r.feed_id == id).unwrap();
    assert_eq!((by_id(5).status(), by_id(5).diagnosis.as_ref().unwrap().evidence.clue.as_str()), (Moved, "301"));
    assert_eq!(by_id(5).action(), Some(&Action::FollowRedirect { url: "https://girl.example.org/feed".into() }));
    assert_eq!((by_id(6).status(), by_id(6).action()), (Hijacked, Some(&Action::Bury)));
    assert_eq!(by_id(7).decision.as_ref().map(|d| d.decision), Some(Decision::Keep));
    assert_eq!(by_id(7).wayback_url.as_deref(), Some("http://web.archive.org/web/2019/http://www.unphotographable.com/"));
    assert!(by_id(4).buried);
    let mut queue: Vec<FeedId> = s.rows.iter().filter(|r| r.needs_triage()).map(|r| r.feed_id).collect();
    queue.sort();
    assert_eq!(queue, [2, 5, 6]);
    assert_eq!(s.unread, HashMap::from([(4, 2)]), "two unread, one read");
    assert_eq!(s.cadence.get(&6).map(|c| (c.effective_group, c.is_override)), Some((CadenceGroup::Weekly, true)));
    assert_eq!(unread_label(by_id(4), s.unread[&4]).as_deref(), Some("2 unread"));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui garden 2>&1 | tail -3` → compile error (`garden` missing).
- [ ] **Step 3: Implement** `garden/mod.rs` and the `lib.rs` line.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B5a ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/garden/mod.rs crates/ressac-ui/src/lib.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: garden rules (sections, retention, keys, labels, rhythm, theme and rhythm menus)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B5b: the visit and the inspector

### Task 3 (B5b): `GardenVisit`, `Inspector`

**Files:**
- Create: `crates/ressac-ui/src/garden/visit.rs` (tests in its `mod tests`)
- Create: `crates/ressac-ui/src/garden/inspector.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/garden/mod.rs` (`mod inspector; mod visit; pub use inspector::*; pub use visit::GardenVisit;`)

**Interfaces:**
- Consumes: Task 2; B2 `sources::status_color`.
- Produces: `GardenVisit`, `Inspector`, `RhythmControl`, `LogLine` and the inspector constants (header).

Rules (`GardenStore.swift`, `GardenInspector.swift`):
- `enter(scope)`: the same scope does nothing; another starts a visit (snapshot emptied, `seen_in_queue`, `decided_this_visit`, selection, message, confirmation cleared; grouping kept). `leave()`: no scope, the same clearing.
- `update(snapshot)`: `previous` = the old rows; `seen_in_queue` gains every feed needing triage; with `ordered = self.ordered()`: in Needs Triage, `selected = selection_after_update(selected, previous, rows, ordered)`; then a selection not in `ordered` becomes the first feed needing triage in `ordered`, else the first, else `None`.
- `sections()`: `rhythm_sections(visible, cadence)` when the scope is All and the grouping Rhythm, else `sections(visible, scope)`; `ordered()` = the sections' rows in order.
- `key(key, modifiers)`: `command(key, modifiers, selected row, safe_repairs().len())`; `None` → `None` (the message stays); otherwise the message is cleared, `Message(m)` sets it, `ConfirmSafeRepairs` sets `confirming`, and the command is returned (the caller runs Decide, Undo, Open).
- `empty_text()`: `Some(empty_message(scope))` when a scope is shown and no section has rows.
- `Inspector::of` (all from the row unless said): `status_line(status)`, `status_color = sources::status_color(Some(status))`; `reason` = the diagnosis's or `""`; `rhythm` = `RhythmControl { line: rhythm_line(c, now), index: rhythm_index(c) }` when a cadence row exists and the feed is not buried; `theme` = `Some(theme_index(themes, key))` unless buried; `evidence` = bullets; `remarks` = `remark_sentence`s; `proposed` = `action.label()` unless buried; `decision` = `"Keep" | "Repair" | "Bury"`; `site` / `wayback` = the address when `is_web_url`; `keep_title` = `"Restore"` when buried else `"Keep"`; `can_repair` = action `is_repair()` and not buried; `can_bury` = not buried; `bury_prominent` = action is `Bury`; `redirects` = the first log entry's hops as `redirect_line`; `log` = each attempt as `LogLine { at: <at in tz as "%b %-d, %Y, %-I:%M %p">, outcome: outcome(a) }`.
- `GardenVisit::inspector` = `Inspector::of` for the selected row with its cadence, the themes and its theme key.

- [ ] **Step 1: Write the failing tests.** `visit.rs` (`fn fixture() -> (TestDb, Store)` = `TestDb::full()` and its store; `fn visit(scope, store) -> GardenVisit` = `enter(scope)` then `update(load(store))`; `fn headers(v: &GardenVisit) -> Vec<String>`):

```rust
#[test]
fn entering_selects_the_first_feed_needing_triage() {
    let (_db, store) = fixture();
    let v = visit(GardenScope::NeedsTriage, &store);
    assert_eq!(v.ordered(), [6, 5, 2]);
    assert_eq!(headers(&v), ["🦠 HIJACKED · 1", "🚚 MOVED · 1", "🦴 FOSSIL · 1"]);
    assert_eq!(v.selected(), Some(6));
    assert_eq!(v.count_label(), "3 of 4 left");
    assert_eq!(v.safe_repairs().iter().map(|(id, _)| *id).collect::<Vec<_>>(), [5]);
    let all = visit(GardenScope::All, &store);
    assert_eq!((all.ordered().len(), all.count_label().as_str()), (10, "10 feeds"));
    assert_eq!(all.selected(), Some(6), "the first feed needing triage, not the first row");
    let cemetery = visit(GardenScope::Cemetery, &store);
    assert_eq!((cemetery.ordered(), cemetery.selected(), cemetery.count_label()), (vec![4], Some(4), "1 feed".to_string()));
}

/// Review Focus 2.
#[test]
fn a_feed_decided_here_or_in_a_terminal_stays_listed_dimmed_until_the_scope_is_left() {
    let (_db, store) = fixture();
    let mut v = visit(GardenScope::NeedsTriage, &store);
    decide(&store, 6, Decision::Keep, at("2026-10-16T09:00:00Z")).unwrap();     // here or in `ressac garden`: the same write
    v.update(load(&store).unwrap());
    assert_eq!(v.ordered(), [6, 5, 2]);
    assert!(v.is_dimmed(6) && !v.is_dimmed(5));
    assert_eq!(v.selected(), Some(5));
    assert_eq!(v.count_label(), "2 of 4 left");
    v.enter(GardenScope::All);
    v.enter(GardenScope::NeedsTriage);
    v.update(load(&store).unwrap());
    assert_eq!(v.ordered(), [5, 2]);
}

#[test]
fn all_feeds_keeps_a_feed_buried_during_the_visit() {
    let (_db, store) = fixture();
    let mut v = visit(GardenScope::All, &store);
    v.select(8);
    decide(&store, 8, Decision::Bury, at("2026-10-16T09:00:00Z")).unwrap();      // a healthy feed: never in the queue
    v.decided(8);
    v.update(load(&store).unwrap());
    assert!(v.ordered().contains(&8) && v.is_dimmed(8));
    v.undone(Some(8));
    v.update(load(&store).unwrap());
    assert!(!v.ordered().contains(&8), "buried and no longer remembered");
}

#[test]
fn keys_set_the_message_and_the_confirmation_and_hand_back_runs() {
    let (_db, store) = fixture();
    let mut v = visit(GardenScope::NeedsTriage, &store);                         // 6 selected: Hijacked, action bury
    assert_eq!(v.key("r", 0), Some(Command::Message(NOTHING_TO_REPAIR)));
    assert_eq!(v.message(), Some("Nothing to repair for this feed."));
    assert_eq!(v.key("j", 0), None);
    assert_eq!(v.message(), Some("Nothing to repair for this feed."), "not the garden's key: the message stays");
    assert_eq!(v.key("R", qt::SHIFT), Some(Command::ConfirmSafeRepairs));
    assert!(v.confirming() && v.message().is_none());
    assert_eq!(v.key("b", qt::CONTROL), None);
    assert_eq!(v.key("k", 0), Some(Command::Decide(Decision::Keep)));
}

#[test]
fn rhythm_grouping_applies_to_all_feeds_only() {
    let (_db, store) = fixture();
    let mut queue = visit(GardenScope::NeedsTriage, &store);
    queue.set_grouping(Grouping::Rhythm);
    assert_eq!(headers(&queue), ["🦠 HIJACKED · 1", "🚚 MOVED · 1", "🦴 FOSSIL · 1"]);
    let mut all = visit(GardenScope::All, &store);
    all.set_grouping(Grouping::Rhythm);
    assert_eq!(headers(&all), ["⚡ DAILY · 1", "📅 WEEKLY · 2", "🌗 MONTHLY · 1", "💤 SILENT · 2", "❔ UNCHECKED · 4"]);
}

#[test]
fn empty_scopes_say_so() {
    let (db, store) = fixture();
    db.execute("UPDATE feed SET state = 'active' WHERE id = 4");
    let cemetery = visit(GardenScope::Cemetery, &store);
    assert_eq!((cemetery.empty_text(), cemetery.selected()), (Some("The cemetery is empty"), None));
    assert_eq!(visit(GardenScope::All, &store).empty_text(), None);
}
```

`inspector.rs` (`fn fixture()` as above; themes inserted with `INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), ('lit', 'Literature', 'SEA OF LETTERS', 1)` and `UPDATE feed SET theme_id = (SELECT id FROM theme WHERE key = 'tech') WHERE id = 7`; `fn inspect(store, feed) -> Inspector` = `GardenVisit` on All Feeds (Cemetery for feed 4), `select(feed)`, `inspector(&store.attempts(feed, LOG_LINES)?, at("2026-10-16T09:00:00Z"), &Utc)`):

```rust
#[test]
fn the_inspector_reads_like_the_mac_inspector() {
    let (_db, store) = fixture();
    let i = inspect(&store, 7);
    assert_eq!((i.title.as_str(), i.folder_path.as_str(), i.status_line.as_str(), i.status_color), ("Unphotographable", "Biblis", "🪦 Dead", "red"));
    assert!(!i.buried);
    assert_eq!(i.rhythm, Some(RhythmControl { line: "Rhythm: Silent".into(), index: 0 }));
    assert_eq!(i.theme, Some(2));
    assert_eq!((i.proposed.as_deref(), i.decision.as_deref()), (Some("bury → cemetery"), Some("Keep")));
    assert_eq!(i.feed_url, "http://www.unphotographable.com/index.rdf");
    assert_eq!(i.site.as_deref(), Some("http://www.unphotographable.com/"));
    assert_eq!(i.wayback.as_deref(), Some("http://web.archive.org/web/2019/http://www.unphotographable.com/"));
    assert_eq!((i.keep_title, i.can_repair, i.can_bury, i.bury_prominent), ("Keep", false, true, true));
    assert_eq!(i.redirects, ["301 → https://www.unphotographable.com/index.rdf"]);
    assert_eq!(i.log, [LogLine { at: "Oct 10, 2026, 7:00 AM".into(), outcome: "HTTP 410".into() }]);
    let girl = inspect(&store, 5);
    assert_eq!((girl.can_repair, girl.bury_prominent, girl.status_color), (true, false, "blue"));
    assert_eq!(girl.proposed.as_deref(), Some("follow redirect to https://girl.example.org/feed"));
}

#[test]
fn a_buried_feed_offers_restore_and_nothing_else() {
    let (_db, store) = fixture();
    let i = inspect(&store, 4);
    assert!(i.buried);
    assert_eq!((i.keep_title, i.can_repair, i.can_bury), ("Restore", false, false));
    assert_eq!((i.rhythm.as_ref(), i.theme, i.proposed.as_deref()), (None, None, None));
    assert!(i.log.is_empty() && i.redirects.is_empty());
    assert_eq!([BURIED_NOTE, NOT_FETCHED, NO_FEED_TITLE, NO_FEED_TEXT],
        ["Buried. Press k to restore it.", "Not fetched yet.", "No Feed Selected", "Select a feed to see why it is in the garden."]);
}

/// Review Focus 3.
#[test]
fn the_inspector_shows_web_addresses_only() {
    let (db, store) = fixture();
    db.execute("UPDATE feed SET html_url = 'javascript:alert(1)', wayback_url = 'file:///etc/passwd' WHERE id = 7");
    let i = inspect(&store, 7);
    assert_eq!((i.site, i.wayback), (None, None));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui garden 2>&1 | tail -3` → compile errors (`GardenVisit`, `Inspector` missing).
- [ ] **Step 3: Implement** `visit.rs`, `inspector.rs` and the `mod.rs` lines.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B5b ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/garden/visit.rs crates/ressac-ui/src/garden/inspector.rs crates/ressac-ui/src/garden/mod.rs
git commit -m "ressac-ui: garden visit (queue memory, selection, keys) and inspector content

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B5c: the bridge

### Task 4 (B5c): the `Garden` singleton

**Files:**
- Create: `apps/linux/src/garden.rs` (the bridge, the list model, header getters, invokables, runs)
- Create: `apps/linux/src/garden_inspector.rs` (`impl qobject::Garden` with the inspector getters, over `GardenRust.inspector: Option<Inspector>`)
- Modify: `apps/linux/src/lib.rs` (`mod garden; mod garden_inspector;`)

**Interfaces:**
- Consumes: Tasks 2–3; B4 `runs::submit_short`, `RunEnd`; B2 `live::plan_rows`, `Selection::parse_key`, the list-model conventions of B2c (`#[base = QAbstractListModel]`, rows applied from `plan_rows`, never a reset; the two gotchas); `with_session`.
- Produces: `Garden` (header).

Behaviour (`GardenRust { visit: GardenVisit, rows: Vec<TableRow>, log: Vec<AttemptRecord>, inspector: Option<Inspector> }`; `TableRow { feed_id, glyph, title, clue, rhythm, unread, chip, dimmed, tooltip, section }`, `PartialEq`, keyed by `feed_id`):
- `show(key)`: `Selection::parse_key` → `Garden(scope)` → `visit.enter(scope)` then `reload()`; anything else → `visit.leave()`, rows planned to empty; `state_changed`, `selection_changed`, `inspector_changed`.
- `reload()`: no scope → nothing. `with_session(|s| load(&s.store))`: `Ok(snapshot)` → `visit.update(snapshot)`; `Err(e)` → `tracing::warn!`, `visit.set_message(Some(COULD_NOT_READ_GARDEN))`. Then sync: table rows from `visit.sections()` (each row's `section` = its section's `header()`; `glyph` = `status_glyph(status)`; `clue` = the evidence clue; `rhythm` = `rhythm_label` in All Feeds only; `unread` = `unread_label`; `chip`; `dimmed` = `visit.is_dimmed`; `tooltip`), `plan_rows`, apply; then the inspector (below); signals when values moved.
- Inspector refresh (after a reload or a selection change): the selected feed's log `store.attempts(feed, LOG_LINES)` (an error → empty log), `inspector = visit.inspector(&log, Utc::now(), &chrono::Local)`, `inspector_changed`.
- `select(row)` / `selectFeed(id)`: `visit.select`, then `selection_changed` and the inspector refresh when it changed; an out-of-range row does nothing. `rowOfFeed(id)` → row or −1. `selectedRow` = the selected feed's row.
- `key(text, modifiers)`: `visit.key(&text, modifiers as u32)`; `None` → false; `Decide(d)` → the decision run; `Undo` → the undo run; `Open(url)` → `open_url(url)`; then `state_changed`, true.
- Runs (each needs a scope and, except undo and apply-safe, a selected feed): decision → `submit_short(RunRequest::Decide(feed, d), move |cx| garden::decide(cx.app.store(), feed, d, Utc::now()), end)`, `end` queueing onto `Garden` a method that on `Done(())` calls `visit.decided(feed)` and re-syncs; `undo()` → `RunRequest::Undo`, job `garden::undo(cx.app.store())`, `Done(Some(id))` → `visit.undone(Some(id))` and re-sync; `applySafe()` → `visit.set_confirming(false)`, `RunRequest::ApplySafe`, job `garden::apply_safe_repairs(cx.app.store(), Utc::now()).map(drop)`; `setCadence(i)` → `rhythm_choice(i)` → `RunRequest::SetCadence(feed, group)`, job `cx.app.set_cadence(feed, group).map(drop)`; `setTheme(i)` → `theme_choice(themes, i)` → `RunRequest::SetTheme(feed, key)`, job `cx.app.store().set_feed_theme(feed, key.as_deref())`. `decide(text)`: `Decision::parse` (an unknown word does nothing). The reloads come from `Runs.databaseChanged`.
- `grouping` write: `Grouping::parse`, `visit.set_grouping`, sync. `confirming` write: `visit.set_confirming`.
- Header getters: `active` = a scope is shown; `scopeTitle` = `scope.title()`; `countLabel`; `showsGrouping` = scope All; `showsApply` = scope is not Cemetery; `safeRepairCount`, `applyTitle`, `applyQuestion`, `applyPreview` (`safe_repair_preview(rows, repairs)`), `applyConfirmLabel` from `visit.safe_repairs()`; `message`, `emptyText` (`""` for `None`); `sectionTitles` = the headers.

- [ ] **Step 1: Implement** the three files as above (no unit test here: the rules are Tasks 2–3's; Task 5's walk drives the object).
- [ ] **Step 2: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.`. Then the full gate (B5c ends here).
- [ ] **Step 3: Commit**

```bash
git add apps/linux/src/garden.rs apps/linux/src/garden_inspector.rs apps/linux/src/lib.rs
git commit -m "ressac-kde: Garden singleton (table rows, header, keys and runs, inspector)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B5d: the garden on screen

### Task 5 (B5d): table, inspector, dialog, Show in Garden, walk

**Files:**
- Create: `apps/linux/qml/GardenPane.qml`, `apps/linux/qml/GardenInspector.qml`
- Modify: `apps/linux/qml/ReadingPage.qml`, `apps/linux/qml/SourcesPane.qml`, `apps/linux/qml/Main.qml`
- Test (not counted): `apps/linux/tests/garden.rs`; `apps/linux/tests/menus.rs` (one more test)

**Interfaces:**
- Consumes: Task 4; B2's panes and `SplitView`; B3's `ReaderPane.openExternally`, `perform`, `focusName`, `Recette.key`; B4's `steps` chooser.
- Produces: the QML items of the header.

`GardenPane.qml` (inventory §4.2; root `Item`):
- Header (`RowLayout`, height 44, horizontal margins 12): `Kirigami.Heading { level: 4; text: Garden.scopeTitle }`; the count capsule (a `Rectangle` with radius half its height, `Kirigami.Theme.alternateBackgroundColor`, holding `Garden.countLabel` in `smallFont`, tabular figures); when `Garden.showsGrouping`, two checkable `QQC2.ToolButton`s from `Garden.groupingTitles` in a `QQC2.ButtonGroup` bound both ways to `Garden.grouping` (`"status"` / `"rhythm"`), tooltip "Group the feeds by status or by publishing rhythm"; a filler; the message (`Garden.message`, `smallFont`, `disabledTextColor`, elided, `opacity` 1 when not empty else 0 with a 150 ms `Behavior`); `QQC2.Button { text: Garden.applyTitle; highlighted: true; visible: Garden.showsApply; enabled: Garden.safeRepairCount > 0; onClicked: Garden.confirming = true }` with tooltip "Apply every safe repair: https upgrades and permanent redirects (Shift+R)"; `QQC2.Button { action: QQC2.Action { text: "Undo"; shortcut: StandardKey.Undo; enabled: Garden.active; onTriggered: Garden.undo() } }` with tooltip "Undo the last decision, here or in the terminal garden (U or Ctrl+Z)". Every header control `focusPolicy: Qt.NoFocus`. A `Kirigami.Separator` under it.
- `ListView { objectName: "gardenList"; model: Garden; focus: true; section.property: "section"; section.delegate: Kirigami.ListSectionHeader { required property string section; text: section } }`; delegate `QQC2.ItemDelegate` (`highlighted: ListView.isCurrentItem`, `focusPolicy: Qt.NoFocus`, `opacity: model.dimmed ? 0.45 : 1` with a 200 ms `Behavior`, `QQC2.ToolTip.text: model.tooltip`): a `RowLayout` with the glyph (22 px wide), the title (elided), a filler, the clue (`font.features: { "tnum": 1 }`, `disabledTextColor`), the rhythm (`disabledTextColor`, tooltip "Rhythm set by you" when it ends with `*`, else "Rhythm"), the unread label, the chip (a capsule like the count, `smallFont`); `onClicked: { Garden.select(index); list.forceActiveFocus() }`. `Connections { target: Garden; function onSelectionChanged() { list.currentIndex = Garden.selectedRow; if (list.currentIndex >= 0) list.positionViewAtIndex(list.currentIndex, ListView.Contain) } }`; `onCurrentIndexChanged: if (currentIndex !== Garden.selectedRow && currentIndex >= 0) Garden.select(currentIndex)` (arrow keys); `Keys.onPressed: (event) => { event.accepted = Garden.key(event.text, event.modifiers) }`.
- `Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: Garden.emptyText !== ""; text: Garden.emptyText; icon.name: "games-hint" }`.
- `QQC2.Dialog { id: repairs; modal: true; anchors.centerIn: parent; title: Garden.applyQuestion }` holding a `QQC2.Label { text: Garden.applyPreview; wrapMode: Text.Wrap }` and `footer: QQC2.DialogButtonBox` with `QQC2.Button { text: Garden.applyConfirmLabel; DialogButtonBox.buttonRole: AcceptRole }` and `QQC2.Button { text: "Cancel"; DialogButtonBox.buttonRole: RejectRole }`; `onAccepted: Garden.applySafe()`; `onClosed: Garden.confirming = false`; `Connections { target: Garden; function onStateChanged() { if (Garden.confirming && !repairs.visible) repairs.open() } }`.
- `function focusList() { list.forceActiveFocus() }`, `function acceptRepairs() { repairs.accept() }`.

`GardenInspector.qml` (inventory §4.3; root `Item`): when `!Garden.hasSelection`, `Kirigami.PlaceholderMessage { text: Garden.noFeedTitle; explanation: Garden.noFeedText; icon.name: "games-hint" }`; else a `QQC2.ScrollView` with a `ColumnLayout` (padding 20, spacing 14):
- the title (`Kirigami.SelectableLabel`, `Kirigami.Heading` size, semibold) and the folder path (`disabledTextColor`, hidden when empty);
- the status line (`Kirigami.Heading { level: 4 }`, colour `Garden.statusColor` when not empty, else `disabledTextColor`), `Garden.buriedNote` (medium weight) when `Garden.buried`, the reason (wrapping);
- when `Garden.rhythmLine !== ""`: a row with the line and `QQC2.ComboBox { model: Garden.rhythmChoices; currentIndex: Garden.rhythmIndex; onActivated: (i) => Garden.setCadence(i) }`, tooltip "Auto follows how often the feed publishes; a group chosen here stays until you choose Auto.";
- when `Garden.themeIndex >= 0`: "Theme" and `QQC2.ComboBox { model: Garden.themeNames; currentIndex: Garden.themeIndex; onActivated: (i) => Garden.setTheme(i) }`, tooltip "The sea this feed sails on in the Sea Chart. None leaves it in the uncharted waters.";
- "Evidence" and "Remarks" (semibold titles) with `•` bullet lines, each hidden when empty;
- a `GridLayout` (2 columns, labels right-aligned in `disabledTextColor`): Proposed (when set), Decision (when set), Feed (`Garden.feedUrl`, selectable), Site (a link to `Garden.site` when set), Wayback (`Garden.archivedCopy` linking to `Garden.wayback` when set); links call `reading.openExternally(url)` through a `signal openUrl(string url)` on the root, wired by `ReadingPage`;
- the buttons, each with its key shown after the title in a lighter `smallFont` (e.g. "Keep k"): `Garden.keepTitle` + `k` → `Garden.decide("keep")`; "Repair" + `r` (`visible: Garden.canRepair`, `highlighted: true`) → `decide("repair")`; "Bury" + `b` (`visible: Garden.canBury`, `highlighted: Garden.buryProminent`) → `decide("bury")`; "Open Site" + `o` (`enabled: Garden.site !== ""`) and "Wayback" + `w` (`enabled: Garden.wayback !== ""`) → `openUrl(…)`;
- a disclosure `QQC2.ToolButton { text: Garden.historyTitle; icon.name: open ? "arrow-down" : "arrow-right" }` that shows: "Redirects" (semibold) and `Garden.redirects` (monospace, small, selectable) when not empty; `Garden.notFetched` (`disabledTextColor`) when `logDates` is empty, else "Fetch log" (semibold) and one row per entry (`logDates[i]` in `disabledTextColor`, tabular figures, then `logOutcomes[i]`), small.
- Every button, combo box and the disclosure: `focusPolicy: Qt.NoFocus`.

`ReadingPage.qml`:
- The `SplitView`'s middle child becomes an `Item` (holding B2's `SplitView` attached sizes) with `EntriesPane { id: entriesPane; anchors.fill: parent; visible: !SourcesModel.isGarden }` and `GardenPane { id: gardenPane; anchors.fill: parent; visible: SourcesModel.isGarden }`; the third likewise with `ReaderPane { id: readerPane … visible: !SourcesModel.isGarden }` and `GardenInspector { id: gardenInspector; anchors.fill: parent; visible: SourcesModel.isGarden; onOpenUrl: (url) => readerPane.openExternally(url) }`. The ids are unchanged.
- The `SourcesModel.onSelectionChanged` handler also runs `Garden.show(SourcesModel.selectedKey)` and, when `SourcesModel.isGarden`, `gardenPane.focusList()`.
- `reloadAll()` also calls `Garden.reload()`; `Connections { target: Garden; function onOpenUrl(url) { readerPane.openExternally(url) } }`.
- `perform("focusEntryList")`: `SourcesModel.isGarden ? gardenPane.focusList() : <B3's behaviour>`.
- `function focusGarden() { gardenPane.focusList() }`; `focusName()` answers `"garden"` when the garden list has the active focus; `readonly property alias garden: gardenPane`.

`SourcesPane.qml`: the feed context menu gains, last, `QQC2.MenuItem { text: "Show in Garden"; onTriggered: { SourcesModel.selectKey("garden:all"); Garden.selectFeed(model.feedId) } }`.

`Main.qml`: the chooser becomes `Recette.walk === "journal" ? journalSteps : Recette.walk === "garden" ? gardenSteps : walkSteps`, and (fixture: queue Hijacked 6, Moved 5 (safe repair, follow redirect), Fossil 2; Unphotographable 7 kept; feed 4 buried; the test adds themes Literature (position 1) and Technology (2), feed 8 on Technology):

```qml
readonly property var gardenSteps: [
    { name: "02-triage", enter: function () { SourcesModel.selectKey("garden:needs-triage")
        console.warn("walk: 02-triage count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " rows=" + Garden.rowCount()
            + " apply=" + Garden.applyTitle + " focus=" + reading.item.focusName() + " sections=" + Garden.sectionTitles.join("|")) } },
    { name: "03-keep", enter: function () { Recette.key(Qt.Key_K, "k", 0) } },
    { name: "04-wait", enter: function () {} },
    { name: "05-kept", enter: function () { console.warn("walk: 05-kept count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId
        + " rows=" + Garden.rowCount() + " kept6=" + (Garden.rowOfFeed(6) >= 0)) } },
    { name: "06-confirm", enter: function () { Recette.key(Qt.Key_R, "R", Qt.ShiftModifier)
        console.warn("walk: 06-confirm confirming=" + Garden.confirming + " question=" + Garden.applyQuestion
            + " preview=" + Garden.applyPreview + " confirm=" + Garden.applyConfirmLabel) } },
    { name: "07-apply", enter: function () { reading.item.garden.acceptRepairs() } },
    { name: "08-wait", enter: function () {} },
    { name: "09-applied", enter: function () { console.warn("walk: 09-applied count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId) } },
    { name: "10-undo", enter: function () { Recette.key(Qt.Key_U, "u", 0) } },
    { name: "11-wait", enter: function () {} },
    { name: "12-undone", enter: function () { console.warn("walk: 12-undone count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId) } },
    { name: "13-all-rhythm", enter: function () { SourcesModel.selectKey("garden:all"); Garden.grouping = "rhythm"
        console.warn("walk: 13-all-rhythm count=" + Garden.countLabel + " sections=" + Garden.sectionTitles.join("|")) } },
    { name: "14-inspector", enter: function () { Garden.selectFeed(8)
        console.warn("walk: 14-inspector rhythmIndex=" + Garden.rhythmIndex + " theme=" + Garden.themeIndex
            + " themes=" + Garden.themeNames.join("|") + " rhythm=" + Garden.rhythmLine) } },
    { name: "15-set", enter: function () { Garden.setCadence(1); Garden.setTheme(1) } },
    { name: "16-wait", enter: function () {} },
    { name: "17-set-done", enter: function () { console.warn("walk: 17-set-done rhythmIndex=" + Garden.rhythmIndex + " theme=" + Garden.themeIndex
        + " rhythm=" + Garden.rhythmLine) } },
    { name: "18-open", enter: function () { reading.item.focusGarden(); Recette.key(Qt.Key_O, "o", 0) } },
    { name: "19-cemetery", enter: function () { SourcesModel.selectKey("garden:cemetery")
        console.warn("walk: 19-cemetery count=" + Garden.countLabel + " selected=" + Garden.selectedFeedId + " keep=" + Garden.keepTitle
            + " apply=" + Garden.showsApply) } },
    { name: "20-buried-key", enter: function () { Recette.key(Qt.Key_B, "b", 0); console.warn("walk: 20-buried-key message=" + Garden.message) } },
]
```

- [ ] **Step 1: Write the failing tests.** `apps/linux/tests/garden.rs`:

```rust
//! The garden, offscreen, through `--screenshot` (spec §7.6; inventory §4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

#[test]
fn the_garden_walk_triages_repairs_undoes_and_sets_rhythm_and_theme() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db).unwrap().execute_batch(
        "INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), \
         ('lit', 'Literature', 'SEA OF LETTERS', 1); \
         UPDATE feed SET theme_id = (SELECT id FROM theme WHERE key = 'tech') WHERE id = 8;").unwrap();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "garden")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-triage count=3 of 4 left selected=6 rows=3 apply=Apply 1 Safe Repair focus=garden sections=🦠 HIJACKED · 1|🚚 MOVED · 1|🦴 FOSSIL · 1",
        "walk: 05-kept count=2 of 4 left selected=5 rows=3 kept6=true",
        "walk: 06-confirm confirming=true question=Apply 1 safe repair? preview=Girl on the Couch: follow redirect to https://girl.example.org/feed confirm=Apply",
        "walk: 09-applied count=1 of 4 left selected=2",
        "walk: 12-undone count=2 of 4 left selected=2",
        "walk: 13-all-rhythm count=10 feeds sections=⚡ DAILY · 1|📅 WEEKLY · 2|🌗 MONTHLY · 1|💤 SILENT · 2|❔ UNCHECKED · 4",
        "walk: 14-inspector rhythmIndex=0 theme=2 themes=None|Literature|Technology rhythm=Rhythm: Weekly · median 4.0 d · last entry ",
        "walk: 17-set-done rhythmIndex=1 theme=1 rhythm=Rhythm: Daily · median 4.0 d · last entry ",
        "reader: open https://daringfireball.net/",
        "walk: 19-cemetery count=1 feed selected=4 keep=Restore apply=false",
        "walk: 20-buried-key message=This feed is buried; press k to restore it first.",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(err.lines().any(|l| l.starts_with("walk: 17-set-done") && l.ends_with(" (set by you)")), "{err}");
    for shot in ["02-triage.png", "06-confirm.png", "14-inspector.png", "19-cemetery.png"] {
        assert_drawn(&run.shots.join(shot));
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let decisions: Vec<(i64, String)> = conn.prepare("SELECT feed_id, decision FROM garden_decision ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(decisions, [(7, "keep".into()), (4, "bury".into()), (6, "keep".into())], "the repair of 5 was undone");
    let girl: String = conn.query_row("SELECT xml_url FROM feed WHERE id = 5", [], |r| r.get(0)).unwrap();
    assert_eq!(girl, "http://girl.example.org/rss", "undo put the address back");
    let (rhythm, theme): (Option<String>, Option<String>) = conn.query_row(
        "SELECT f.cadence_override, t.key FROM feed f LEFT JOIN theme t ON t.id = f.theme_id WHERE f.id = 8", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((rhythm.as_deref(), theme.as_deref()), (Some("daily"), Some("lit")));
}
```

`apps/linux/tests/menus.rs` gains (on every platform):

```rust
const GARDEN: &str = include_str!("../qml/GardenPane.qml");

#[test]
fn the_garden_undo_is_ctrl_z_and_its_tooltips_name_kde_keys() {
    assert!(GARDEN.contains("shortcut: StandardKey.Undo"));
    assert!(GARDEN.contains("Apply every safe repair: https upgrades and permanent redirects (Shift+R)"));
    assert!(GARDEN.contains("Undo the last decision, here or in the terminal garden (U or Ctrl+Z)"));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-kde --test menus 2>&1 | tail -3` on the Mac → compile error (`GardenPane.qml` missing); `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test garden 2>&1 | grep -E "panicked|test result"'` → FAIL.
- [ ] **Step 3: Implement** the five QML files.
- [ ] **Step 4: Verify and look** — `cargo test -p ressac-kde --test menus` on the Mac → `ok.`; `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.` (`launch`, `reading`, `reader`, `keys`, `menus`, `journal`, `garden`; B2's `06-garden` and B3's `16-garden-keys` lines unchanged). If a `*-wait` step is not enough for a short run to land, add one more empty wait step (never shorten the walk's 400 ms). Look at the screens: run B2d Task 9 Step 4's `--screenshot` command with `RESSAC_RECETTE_WALK=garden` on a copy prepared as in the test, open `02-triage.png` (section headers with glyphs, the count capsule, "Apply 1 Safe Repair", the inspector of Bibliobsession 2.0 with its "Bury b" highlighted), `05-kept.png` (6 dimmed, 5 selected), `06-confirm.png` (the dialog over the window), `14-inspector.png` (rhythm line and both pickers, the Keep / Bury / Open Site / Wayback buttons) and `19-cemetery.png` ("Restore k", "Buried. Press k to restore it."), then `rm -rf target-shots`. Then the full gate (B5d and B5 end here). Ledger rows B5a–B5d: commits, test counts, deviations 1–8.
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/GardenPane.qml apps/linux/qml/GardenInspector.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/SourcesPane.qml apps/linux/qml/Main.qml apps/linux/tests/garden.rs apps/linux/tests/menus.rs
git commit -m "ressac-kde: garden table, inspector with Theme picker, safe-repairs dialog, Show in Garden

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
