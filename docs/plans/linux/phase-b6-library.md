# Phase B6: add feed, OPML, first run, Settings, drag and drop (B6a–B6e)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE app adds feeds (Ctrl+N dialog with clipboard prefill, Theme and Folder pickers, refusals in the dialog, the 6-second note), imports OPML (drop on the window, Ctrl+O, first-run button; dry-run preview, confirmation, import then fetch), exports OPML (Ctrl+Shift+E), shows "No Feeds Yet" when the library is empty, has a Settings window (Reading: font, size, line width, preview; Data: database path with Choose…, New…, Use Default, the relaunch rule, Show Logs), moves feeds by dragging them onto a folder or the "Feeds" heading, and lets an entry be dragged out as its link, all with the Mac app's sentences.

**Runs after:** B5d (garden; `garden::{theme_names, theme_choice}`), B4b (`runs::submit_long`, `runs::submit_short`, `RunEnd`, `Recette.walk`, `Main.qml`'s `steps` chooser), B3 (`ressac:` scheme and `ressac_scheme_serve`, `sanitize::is_web_url`, `MainMenu.qml` with the disabled `addFeed`, `importOpml`, `exportOpml`, `settings` actions, the key filter), B2 (`SidebarTree`, `SourcesModel`, `RunRequest::{Add, Import, Export, MoveFeed}` with their refusal classes, `ReadingPage.reloadAll()`), B1 (`AppSettings`, `QSettingsStore`, `launch::{database_location, location_problem, NEWER_DATABASE, NOT_A_DATABASE}`, `Recette.grab`, `tests/common`). Master plan `docs/plans/2026-10-23-linux.md` rows B6a, B6b; Interfaces (`application/x-ressac-feed`, settings keys); Global Constraints (Runs, Language, Shortcuts, Settings keys, Never, Commits, Gates). Spec §7.7 (and §7.1 runs, §7.2 shortcuts, §7.3 drag and drop, §7.11 errors). Inventory §2.9, §7.1, §7.2, §7.3, §10.1 (AddFeedTests, LogRedactionTests, FirstRunTests, DatabaseChoiceTests, MoveFeedTests): strings binding.

**Architecture:** every decision is Rust in `ressac-ui`, tested on the Mac: a new module `library` (add-feed form and note, folder choices, clipboard rule, OPML flow and texts, the preview / import / export jobs, drag payloads and drop decisions), additions to `sources` (empty library, a feed's folder), `settings` (font, width titles and families, preview text), `launch` (the Data page's choose rule and notes) and `runs` (redacted log labels, the fetch job shared with the import). The core gains the CLI's address redaction. `ressac-kde` adds one QML singleton `Library`, grows `SourcesModel` (empty library, `dropFeed`) and `AppSettings` (Data page), and its C++ shim reads the clipboard, registers the bundled fonts and grabs the focused window. QML: `AddFeedDialog.qml`, `OpmlDialogs.qml`, `FirstRun.qml`, `SettingsWindow.qml`, the drop target and the note bar in `Main.qml`, drag sources in the panes.

**Tech Stack:** Rust 2024 / 1.99; `ressac-core` (`Ressac::{add_feed, import_opml}`, `opml::export`, `Store::{sources, themes, move_feed, probe}`), `url` (file URLs); cxx-qt / cxx-qt-lib 0.10.0 (`QStringList`, `QMap_QString_QVariant`); Qt 6.11.2 Quick, QuickControls2, QtQuick.Dialogs (`FileDialog`, XDG portal in a Plasma session), QtCore (`StandardPaths`), `QFontDatabase`, `QClipboard`; Kirigami 6.31 (`PlaceholderMessage`, `FormLayout`); Fedora 44 container.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Five sub-phases, not two** (5-file cap; QML and C++ count). **B6a** redaction and the `library` rules (5 files), **B6b** sidebar, settings and Data-page rules with their two bridges (5), **B6c** the `Library` singleton and the shim (5), **B6d** add feed, OPML and first run on screen (4), **B6e** Settings window and drag and drop on screen (5). Master B6a = B6a + B6c + B6d; master B6b = B6b + B6e. Ledger rows B6a–B6e. Every master name is kept.
2. **In-process engine, no exit codes.** `Ressac::add_feed` returns the `AddedFeed` (its `like` is `ThemeSource::Sibling { title }`), so "ressac did not report the added feed." cannot happen and is not ported. A refusal is B2's `RunRequest::is_refusal` (the CLI's exit-2 classes); its sentence shows without Show Log. The preview and the import read the file themselves with the CLI's sentence `cannot read <path>: <io error>`.
3. **Log redaction is a core function.** The CLI's private `redact_address` moves to `ressac_core::logging` (with `redact_text` for addresses inside sentences) and the CLI calls it there. Since B2 the app logs `{request:?}`, which would write a typed address with its `user:pass@` or `?token=` into the log file; every run log line now uses `RunRequest::log_label` and `redact_text`. The query marker is the CLI's `?...` (the Mac writes `?…`): log text, not UI.
4. **The import preview is a short job outside the coordinator** (the Mac runs it as a separate process outside the RunCoordinator). It opens a write transaction that is rolled back, so it never runs on the GUI thread: it goes on `Session.worker`'s FIFO short lane and may wait out a terminal's write lock without freezing the window.
5. **Feed drags are internal Qt Quick drags.** `Drag.dragType: Drag.Internal` with `Drag.keys: ["application/x-ressac-feed"]` (the private MIME type is the drag key). Internal drags carry no MIME data (validated: `formats` empty, `getDataAsString` `""`), so the drop reads the payload (`"<feed id>"`, digits) from `drop.source.payload`. A text field never receives a feed (no `text/plain`), which fixes inventory §11's paste gap. Dropping onto a feed row still does nothing (Mac parity). The entry drag-out is an automatic (platform) drag with `text/uri-list`, `text/plain` and `text/x-moz-url`; offscreen it starts without blocking but cannot complete, so its gesture joins the next VM checkpoint's checklist (B8e).
6. **Settings, Linux wording:** the font titles are "System Sans", "System Serif", "Newsreader", "Atkinson Hyperlegible Next"; the hint names KDE keys: "Ctrl++ and Ctrl+− change the text size while you read; Ctrl+0 restores it."; the logs row is "Logs:" with **Show Logs** (spec §7.7) instead of "Reveal in Finder". The Appearance row (and the font picker's Antiope rule "Bedstead in Antiope", the Antiope size slider) comes with B8, the CRT toggle with B9: no control for a look that does not exist yet.
7. **New Settings ▸ Data texts for the file dialogs** are the Mac panels' messages used as dialog titles ("Choose a Ressac database.", "Ressac creates the database here on its next launch."); filters `OPML files (*.opml)` and `SQLite databases (*.sqlite)`.
8. **Relaunch Now starts a new `ressac-kde` process** (`std::env::current_exe()`, same environment) and quits, as the Mac opens a new instance; B1's in-process `Launcher.relaunch()` stays reserved for the launch-failure screens.
9. **`Recette.grab` grabs the focused window** when it is a visible `QQuickWindow`, else the first visible one (B1's rule), so the Settings and Sea Chart windows can be shot; `tests/common` gains `assert_drawn_min(png, width, height)` for windows smaller than 900×500.
10. **Bundled fonts are registered with `QFontDatabase`** at start (bytes from `ressac_scheme_serve("ressac:asset/<file>")`, B3), so the font menu and the preview draw in their faces (Mac `BundledFontTests`).
11. **Menu wiring from `Main.qml`.** `MainMenu.qml` keeps B3's items; `Main.qml` binds the four File actions' `enabled` (`Binding`) and connects their `triggered` (`Connections`), so `tests/menus.rs` is untouched.
12. **Mac tests that become walk checks:** selection kept across a move (`MoveFeedTests.selection_stays_on_the_moved_feed`, `a_source_picked_during_the_move_is_kept`: B2's selection is a key that a move does not change), the dialogs and first-run screen (`AddFeedSheet`, `FirstRunView`, `ImportConfirmationSheet`), font licences (the files are B3's, read in place).

## Validated in a throwaway container (`ressac-b67-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A C++ Qt 6.11.2 program loading QML files offscreen (`QApplication`, `QTest` mouse events), all checks passed:
- **(a) Feed drag.** A 1×1 proxy `Item` inside a `ListView` delegate, `Drag.dragType: Drag.Internal`, `Drag.keys: ["application/x-ressac-feed"]`, moved by a `MouseArea { drag.target: proxy; preventStealing: true }` placed at the press point (`onPressed: proxy.x = mouse.x; proxy.y = mouse.y`): real `QTest::mousePress/mouseMove/mouseRelease` from row 2 to row 1 entered row 1's `DropArea { keys: ["application/x-ressac-feed"] }` and `proxy.Drag.drop()` in `onReleased` returned `Qt.MoveAction` (2) after `drop.accept(Qt.MoveAction)`; `drop.source.feedId` was 2. **Gotchas:** `Drag.active: area.drag.active` (a binding) cancels the drag on release instead of dropping, so set `proxy.Drag.active = true` in `drag.onActiveChanged` and call `Drag.drop()` in `onReleased`; a proxy left at its delegate's origin puts the hot spot away from the pointer. Driven from QML (`proxy.Drag.active = true`, move the proxy over the target's centre, `proxy.Drag.drop()`) the same `DropArea` received the drop (`Qt.MoveAction`). Internal drags: `drop.formats` empty, `getDataAsString(...)` `""`.
- **(b) External drops** (`QDragEnterEvent`/`QDragMoveEvent`/`QDropEvent` sent to the `QQuickWindow`): the private type's data reached the folder `DropArea` (`getDataAsString` `"1"`, `drop.source` null); a `TextField` refused it (enter not accepted, text unchanged); `file:///tmp/My%20Feeds.opml` plus a `.txt` reached a window-level `DropArea { keys: ["text/uri-list"] }` declared first (under everything else): `drop.urls` → `file:///tmp/My Feeds.opml`, `QUrl::toLocalFile` → `/tmp/My Feeds.opml`; the folder `DropArea`s above it let the URL drop through.
- **(c) Automatic drag** with `Drag.mimeData` `{"text/uri-list": "<url>\r\n", "text/plain": "<url>", "text/x-moz-url": "<url>\n<title>"}`: `Drag.active = true` offscreen returns at once (no platform drag), the map reads back.
- **(d) `FileDialog`** (QtQuick.Dialogs) offscreen: Qt's own non-native dialog opens (`visible` true) in a window of its own, absent from the main window's grab; in `SaveFile` mode `selectedFile` on a missing file is refused with a QML warning ("Cannot set … because it doesn't exist"), and a programmatic `accept()` leaves `selectedFile` empty. So the app's logic takes URLs through invokables, walks call those, and `selectedFile` is set just before `open()` (never at creation).
- **(e) Clipboard:** after a hidden `TextEdit`'s `selectAll(); copy()`, `QGuiApplication::clipboard()->text()` returns the text (`"  https://example.org/feed.xml\n"`).
- **(f) Focus:** a second top-level `ApplicationWindow` after `show(); raise(); requestActivate()` is `QGuiApplication::focusWindow()` and `grabWindow()` of it saves its content; closing it leaves no focus window until the main one `requestActivate()`s.
- Core behaviour checked with the built `ressac` on copies of `tests/fixtures/contract.sqlite`: the walk OPML below previews `{"feeds_added":3,"folders_added":1,"merged":[["Alpha again","Alpha"]],"skipped_existing":0}` on an empty database and imports folder 1 "Walk", feeds 1 Alpha and 2 Beta in it, 3 Gamma at the top level; `add http://127.0.0.1:9/feed` → "Ressac only follows public web addresses." (exit 2), a followed address → "Already followed: xkcd.com.", a buried one → "Already in the Cemetery: Photos from bouquiniste.", blank → "Enter a web address."; a missing OPML → `cannot read <path>: No such file or directory (os error 2)`; `<opml><body><outline text="x"></body>` → `cannot read OPML at line 1, column 31: …`; `feed move 2 root` moves Orange Drenka (folder 2) to the top level. Bundled font families (fontconfig): "Newsreader", "Atkinson Hyperlegible Next".

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-core/src/logging.rs` | B6a T1 | `redact_address` (moved from the CLI), `redact_text` |
| `crates/ressac/src/commands/network.rs` | B6a T1 | calls `ressac_core::logging::redact_address`; its test moves to the core |
| `crates/ressac-ui/src/runs.rs` | B6a T1, T2 | `RunRequest::log_label` (and its use in `admit`'s log line); `fetch` job |
| `crates/ressac-ui/src/library.rs` (new) | B6a T2 | add form, note, folder choices, clipboard rule, OPML flow and texts, preview / import / export / add jobs, drag rules |
| `crates/ressac-ui/src/lib.rs` | B6a T2 | `pub mod library;` |
| `crates/ressac-ui/src/sources.rs` | B6b T3 | `SidebarTree::{nodes, is_empty_library, folder_of}` |
| `crates/ressac-ui/src/settings.rs` | B6b T3 | font and width titles, font families, `PREVIEW_TEXT`, `SIZE_HINT` |
| `crates/ressac-ui/src/launch.rs` | B6b T3 | Data page: `choose_database`, `DatabaseChoice`, notes, dialog texts |
| `apps/linux/src/sources.rs` | B6b T4 | `SourcesModel.emptyLibrary`, `SourcesModel.dropFeed` |
| `apps/linux/src/settings.rs` | B6b T4 | `AppSettings` font and width lists, Data page properties and invokables, `relaunch` |
| `apps/linux/src/library.rs` (new) | B6c T5 | QML singleton `Library` |
| `apps/linux/src/lib.rs` | B6c T5 | `mod library;`, shim declarations |
| `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp` | B6c T5 | `ressac_clipboard_text`, font registration, focused-window grab |
| `apps/linux/src/runs.rs` | B6c T5 | redacted log lines; the fetch path calls `ressac_ui::runs::fetch` |
| `apps/linux/qml/AddFeedDialog.qml` (new) | B6d T6 | the Add a Feed dialog |
| `apps/linux/qml/OpmlDialogs.qml` (new) | B6d T6 | import file dialog, confirmation, failure alert, export file dialog |
| `apps/linux/qml/FirstRun.qml` (new) | B6d T6 | "No Feeds Yet" |
| `apps/linux/qml/Main.qml` | B6d T6; B6e T7 | first-run swap, window drop target, note bar, menu wiring, `openExternally`, walks |
| `apps/linux/qml/SettingsWindow.qml` (new) | B6e T7 | Reading and Data pages |
| `apps/linux/qml/SourcesPane.qml` | B6e T7 | feed drag proxy, folder and "Feeds" drop targets, `dragFeed` |
| `apps/linux/qml/EntriesPane.qml` | B6e T7 | entry drag-out |
| `apps/linux/qml/ReadingPage.qml` | B6e T7 | `dragFeed` passthrough |
| tests (not counted) | | `crates/ressac-ui/src/test_db.rs` (unchanged helpers), `apps/linux/tests/common/mod.rs` (`assert_drawn_min`), `apps/linux/tests/library.rs` (new) |

Files per sub-phase: B6a 5, B6b 5, B6c 5, B6d 4, B6e 5.

## Interfaces produced (B7 onwards rely on these names)

**Core** (`ressac_core::logging`, Task 1):

```rust
/// An address as a log may show it: scheme, host (port) and path. The userinfo, the query and the
/// fragment are dropped; `?...` marks a query that was there. (Moved from the CLI, unchanged.)
pub fn redact_address(address: &str) -> String;
/// `text` with each whitespace-separated word that contains `http://` or `https://` (any case)
/// redacted from that point with `redact_address`; other words unchanged, spacing kept.
pub fn redact_text(text: &str) -> String;
```

**`ressac_ui::runs`** additions (Tasks 1–2):

```rust
impl RunRequest {
    /// The request as a log line shows it: its `Debug` form with `Add`'s address through `redact_address`.
    pub fn log_label(&self) -> String;
}
/// The job of a fetch request (FetchAll, FetchFeeds, FetchFolder): `fetch_options` then `Ressac::refresh`;
/// the bridges run it with `cx.block_on(runs::fetch(cx.app, &request, sink, stop))`.
pub async fn fetch(app: &Ressac, request: &RunRequest, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> ressac_core::Result<()>;
```

**`ressac_ui::library`** (Task 2):

```rust
pub const FEED_MIME: &str = "application/x-ressac-feed";
pub const ADD_TITLE: &str = "Add a Feed";
pub const ADDRESS_PLACEHOLDER: &str = "Feed or site address";
pub const NO_THEME: &str = "None";            // = garden::theme_names' first item
pub const TOP_LEVEL: &str = "Feeds";
pub const ADD_REFUSED: &str = "A refresh is running. Try again when it ends.";
pub const NOTE_SECONDS: u32 = 6;
pub const CANT_IMPORT: &str = "This OPML file can't be imported";
pub const IMPORT_NOTE: &str = "Ressac fetches every feed right after the import.";
pub const IMPORT_BLOCKED: &str = "A refresh is running; try again when it ends.";
pub const NO_FEEDS_TITLE: &str = "No Feeds Yet";
pub const NO_FEEDS_TEXT: &str = "Drop an OPML file here, or choose one…";
pub const CHOOSE_OPML: &str = "Choose OPML File…";
pub const EXPORT_NAME: &str = "Ressac.opml";
pub const OPML_FILTER: &str = "OPML files (*.opml)";

/// The clipboard's text when it is, on its own (trimmed, no inner whitespace), a web address
/// (`sanitize::is_web_url`); else None.
pub fn clipboard_address(text: Option<&str>) -> Option<String>;
/// "In <theme>, like <sibling>." / "In <theme>." / "Added to Uncharted waters." (the sibling: `ThemeSource::Sibling`).
pub fn added_note(added: &AddedFeed) -> String;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChoice { pub folder: Option<FolderId>, pub name: String, pub depth: usize }
impl FolderChoice { pub fn title(&self) -> String; }     // one U+2003 per level of depth, then the name
/// "Feeds" (None, depth 0), then every folder of `tree` in sidebar order, root folders at depth 1.
pub fn folder_choices(tree: &[SourceNode]) -> Vec<FolderChoice>;
#[derive(Debug, Clone, Default)]
pub struct AddFeedForm { pub address: String, pub theme: Option<String>, pub folder: Option<FolderId>, /* running, error, shows_log */ }
impl AddFeedForm {
    pub fn new(clipboard: Option<&str>) -> AddFeedForm;          // address = clipboard_address or ""
    pub fn can_add(&self) -> bool;                              // not running and a non-blank address
    pub fn start(&mut self) -> Option<RunRequest>;              // can_add → running, error cleared, Add { trimmed address, theme, folder }
    pub fn finished(&mut self, end: RunEnd<AddedFeed>) -> Option<AddedFeed>; // Done → Some; Refused → ADD_REFUSED; Failed → its message
    pub fn keep_folder(&mut self, choices: &[FolderChoice]);    // a folder gone from the choices → the top level
    pub fn is_running(&self) -> bool;
    pub fn error(&self) -> Option<&str>;
    pub fn shows_log(&self) -> bool;                             // a failure that is not a refusal
}
/// The add job: `Ressac::add_feed`, the folder passed as its id (`resolve_folder` takes ids first);
/// run with `cx.block_on(library::add_feed(cx.app, …))`.
pub async fn add_feed(app: &Ressac, address: &str, theme: Option<&str>, folder: Option<FolderId>,
    sink: &dyn EventSink, stop: Arc<AtomicBool>) -> ressac_core::Result<AddedFeed>;

pub fn import_title(path: &Path) -> String;                     // "Import “walk.opml”?"
pub fn import_summary(report: &ImportReport) -> Vec<String>;    // the Mac's OPMLFlow.summary lines
pub fn local_path(url: &str) -> Option<PathBuf>;                // a file: URL, percent-decoded; anything else None
pub fn first_opml(urls: &[String]) -> Option<PathBuf>;         // the first local file whose extension is opml (any case)
pub fn read_opml(path: &Path) -> ressac_core::Result<String>;   // Error::Invalid("cannot read <path>: <io error>")
pub fn preview(app: &Ressac, path: &Path) -> ressac_core::Result<ImportReport>;  // import_opml(xml, true)
pub fn import(app: &Ressac, path: &Path) -> ressac_core::Result<ImportReport>;   // import_opml(xml, false)
pub fn export(store: &Store, path: &Path) -> ressac_core::Result<()>;           // opml::export(store, false) written to path
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OpmlStep { #[default] Idle, Previewing(PathBuf), Confirming(PathBuf, ImportReport), Importing(PathBuf, ImportReport), Failed(String) }
impl OpmlStep { pub fn as_str(&self) -> &'static str; }       // "idle" "previewing" "confirming" "importing" "failed"
#[derive(Debug, Clone, Default)]
pub struct OpmlFlow { /* step */ }
impl OpmlFlow {
    pub fn step(&self) -> &OpmlStep;
    pub fn is_busy(&self) -> bool;                                // Previewing or Importing
    pub fn open(&mut self, urls: &[String]) -> Option<PathBuf>;   // not busy and an .opml among them → Previewing(path); the path to preview
    pub fn previewed(&mut self, path: &Path, result: Result<ImportReport, String>); // only while Previewing(path)
    pub fn can_import(&self, blocked: bool) -> bool;              // Confirming, feeds_added > 0, not blocked
    pub fn confirm(&mut self, blocked: bool) -> Option<RunRequest>; // can_import → Importing; Some(Import(path))
    pub fn imported(&mut self, end: &RunEnd<ImportReport>) -> Option<RunRequest>; // Done → Some(FetchAll), stays Importing; Refused → Confirming again; Failed → Idle
    pub fn fetched(&mut self);                                    // Importing → Idle
    pub fn cancel(&mut self);                                     // not busy → Idle
}

pub fn feed_payload(feed: FeedId) -> String;                  // "42"
pub fn feed_from_payload(text: &str) -> Option<FeedId>;      // ASCII digits only, no sign, fits in i64
pub fn drop_target(key: &str) -> Option<Option<FolderId>>;   // "folder:<id>" → Some(Some(id)); "heading:Feeds" → Some(None); else None
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedDrop { Ignore, Move(Option<FolderId>) }
pub fn decide_drop(current: Option<FolderId>, target: Option<FolderId>) -> FeedDrop; // same place → Ignore
/// An entry dragged out: [("text/uri-list", "<link>\r\n"), ("text/plain", "<link>"), ("text/x-moz-url", "<link>\n<title>")],
/// empty when `link` is not a web address.
pub fn entry_drag_data(link: &str, title: &str) -> Vec<(&'static str, String)>;
```

**`ressac_ui::sources`** additions (Task 3):

```rust
impl SidebarTree {
    pub fn nodes(&self) -> &[SourceNode];                              // empty before the first load
    pub fn is_empty_library(&self) -> bool;                            // loaded, and no active feed in the tree
    pub fn folder_of(&self, feed: FeedId) -> Option<Option<FolderId>>; // None: not an active feed of the tree
}
```

**`ressac_ui::settings`** additions (Task 3):

```rust
impl ReaderFont {
    pub fn title(self) -> &'static str;   // "System Sans", "System Serif", "Newsreader", "Atkinson Hyperlegible Next"
    pub fn family(self) -> &'static str;  // "" (the system default), "serif", "Newsreader", "Atkinson Hyperlegible Next"
}
impl LineWidth { pub fn title(self) -> &'static str; }  // "Narrow", "Medium", "Wide", "40 columns"
pub const PREVIEW_TEXT: &str = "The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse.";
pub const SIZE_HINT: &str = "Ctrl++ and Ctrl+− change the text size while you read; Ctrl+0 restores it.";
```

**`ressac_ui::launch`** additions (Task 3):

```rust
pub const SHARED_NOTE: &str = "Shared with the ressac command in Terminal.";
pub const OWN_PATH_NOTE: &str = "In Terminal, pass --db with this path to use the same database.";
pub const RESSAC_DB_NOTE: &str = "RESSAC_DB is set, so this launch uses it instead.";
pub const RELAUNCH_NOTE: &str = "Ressac opens the new database after a relaunch.";
pub const RELAUNCH_NOW: &str = "Relaunch Now";
pub const CHOOSE_TITLE: &str = "Choose a Ressac database.";
pub const NEW_TITLE: &str = "Ressac creates the database here on its next launch.";
pub const NEW_NAME: &str = "ressac.sqlite";
pub const SQLITE_FILTER: &str = "SQLite databases (*.sqlite)";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseChoice { Unchanged, Refused(&'static str), Stored }
/// Settings ▸ Data (the Mac's `DataSettings.update`): `path` equal to the stored one (or blank with
/// none stored) → Unchanged; blank → the setting removed (Use Default), Stored; else `probe(path)`
/// through `location_problem` → Refused(sentence) with the setting untouched, or stored, Stored.
pub fn choose_database(settings: &mut Settings, store: &mut dyn SettingsStore, path: &str,
    probe: impl FnOnce(&Path) -> Result<SchemaState, ressac_core::Error>) -> DatabaseChoice;
pub fn database_note(stored: Option<&str>) -> &'static str;   // SHARED_NOTE without a setting, else OWN_PATH_NOTE
```

**`ressac-kde`** (Tasks 4–7):

- **`SourcesModel`** additions: property `emptyLibrary: bool` (`READ`, `NOTIFY = library_changed`, emitted when it flips after a reload); invokable `dropFeed(payload: QString, targetKey: QString) -> bool` (true when a move was submitted).
- **`AppSettings`** additions (`CONSTANT`): `fontValues`, `fontTitles`, `fontFamilies`, `widthValues`, `widthTitles` (`QStringList`, in `ALL` order), `previewText`, `sizeHint`, `ressacDbSet: bool`, `ressacDbNote`, `relaunchNote`, `relaunchNow`, `chooseTitle`, `newTitle`, `newName`, `sqliteFilter`; (`READ`, `NOTIFY = data_changed`) `resolvedDatabasePath`, `databaseNote`, `databaseRefusal` (`""` = none), `needsRelaunch: bool`; invokables `chooseDatabase(url: QString)`, `useDefaultLocation()`, `relaunch() -> bool`. `databasePath`'s notify also emits `data_changed`.
- **`Library`** (QML singleton, `impl cxx_qt::Threading`): add dialog — `addOpen: bool`, `address: QString` (`WRITE`), `themeNames: QStringList`, `themeIndex: i32` (`WRITE`), `folderTitles: QStringList`, `folderIndex: i32` (`WRITE`), `adding: bool`, `canAdd: bool`, `addError: QString`, `addShowsLog: bool` (all `NOTIFY = add_changed`); note — `note: QString` (`NOTIFY = note_changed`), `noteSeconds: i32` (`CONSTANT`, 6); OPML — `opmlStep: QString`, `busy: bool`, `importTitle`, `importLines: QStringList`, `importNote` (`""` when nothing is new), `canImport: bool` (for an unblocked confirm), `failure: QString` (all `NOTIFY = opml_changed`); `CONSTANT` texts `addTitle`, `addressPlaceholder`, `cantImport`, `importBlocked`, `noFeedsTitle`, `noFeedsText`, `chooseOpml`, `exportName`, `opmlFilter`, `feedMime`. Invokables `beginAdd()`, `add()`, `cancelAdd()`, `reloadChoices()`, `dismissNote()`, `openUrls(urls: QStringList) -> bool`, `confirmImport(blocked: bool)`, `cancelImport()`, `exportTo(url: QString) -> bool`, `logsUrl() -> QString` (creates the folder), `entryDragData(link: QString, title: QString) -> QMap_QString_QVariant`. Signals `addChanged()`, `noteChanged()`, `opmlChanged()`, `added(key: QString)` (`"feed:<id>"`).
- **Shim:** `ressac_clipboard_text() -> String`; `ressac_create_application()` also registers `Newsreader-Variable.ttf`, `Newsreader-Italic-Variable.ttf`, `AtkinsonHyperlegibleNext-Variable.ttf`, `AtkinsonHyperlegibleNext-Italic-Variable.ttf` with `QFontDatabase::addApplicationFontFromData` (bytes through `ressac_scheme_serve("ressac:asset/<file>", …)`; a failure is a `qWarning`); `ressac_grab_window(path)` grabs `QGuiApplication::focusWindow()` when it is a visible `QQuickWindow`, else the first visible one.
- **QML:** `Main.qml` functions `addFeed()`, `importOpml()`, `exportOpml()`, `openSettings()`, `openExternally(url)` (recette: `console.warn("open: " + url)` instead of `Qt.openUrlExternally`), `readonly property var librarySteps`, the chooser gains `Recette.walk === "library" ? librarySteps : …`; `AddFeedDialog.qml` (`function accept()`), `OpmlDialogs.qml` (`function chooseImport()`, `function chooseExport()`, `function confirmImport()`), `FirstRun.qml` (`signal chooseRequested()`), `SettingsWindow.qml` (`function showPage(name)` with `"reading" | "data"`, `function relaunchNow()`, `function showLogs()`), `SourcesPane.qml` and `ReadingPage.qml` `function dragFeed(feedKey, targetKey) -> bool`.

**Consumed:** core `Ressac::{add_feed, import_opml, open}`, `opml::export`, `Store::{sources, themes, move_feed, probe}`, `AddedFeed`, `ThemeSource`, `ImportReport`, `SourceNode`, `SourceNodeKind`, `SchemaState`; B5 `garden::{theme_names, theme_choice}`; B4 `runs::{submit_long, submit_short}`, `RunEnd`, `Recette.walk`; B3 `sanitize::is_web_url`, `ressac_scheme_serve`, `MainMenu.qml` action aliases, `ReaderPane.openExternally`; B2 `SidebarTree`, `RunRequest`, `fetch_options`, `Worker::short`, `JobContext`, `with_session`, `Session.worker`, `ReadingPage.reloadAll()`, `Runs.{longRunActive, progressFraction, logDir}`; B1 `Settings`, `SettingsStore`, `QSettingsStore`, `launch::{database_location, location_problem}`, `Recette.grab`, `tests/common`.

**Decisions this phase pins:**

- **Runs.** Add = a long run (`RunRequest::Add`, refused during another long run with `ADD_REFUSED`); import = a long run, then, when it is `Done`, a forced fetch of everything (`RunRequest::FetchAll`, `runs::fetch`) as another long run; export and move = short runs; the preview = a plain short job on the session worker, outside the coordinator. Failures of import, fetch, export and move are the coordinator's banners (Show Log unless a refusal); add failures stay in the dialog.
- **Add success:** the dialog closes, `Library.note` = `added_note`, `added("feed:<id>")` → `SourcesModel.selectKey` (on a later turn, after the run's reload). An add that ends after Cancel still notes and selects (Mac behaviour, inventory §7.1).
- **First run:** the window shows `FirstRun.qml` instead of the reading page when `SourcesModel.emptyLibrary` (false until the first sidebar load, so a library never flashes it); `MainMenu.page` is null meanwhile.
- **Drops:** the window-level `DropArea { keys: ["text/uri-list"] }` lies under everything (declared first); an OPML among the URLs starts the preview when `Library.busy` is false; a dashed accent overlay (`Kirigami.Theme.highlightColor`, 3 px, dash 8/6, radius 10, 6 px inset, 5 % fill) while `containsDrag`.
- **Settings window:** a separate top-level window (`title: "Settings"`, `width: 560`, `minimumWidth: 540`, `height: 460`), two tabs "Reading" and "Data" over `Kirigami.FormLayout`s; every value read and written through `AppSettings`.
- **Clock:** `Library.reloadChoices()` passes `today_start(chrono::Local::now())` to `Store::sources`.

**Step 0:** not triggered: `ressac-ui/src/runs.rs` (981 lines), `sources.rs` (666), `settings.rs` (738), `launch.rs` (372), `apps/linux/src/settings.rs` (331), `sources.rs` (352) and `runs.rs` (435) only gain items or change two log lines; `network.rs` (286) loses one function. `Main.qml` and `ReadingPage.qml` only gain items.

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

- **Runs:** one long run at a time (fetch, import, edition build, add), a second refused with the Mac's sentence, never queued; short runs FIFO on one worker, allowed during a long run.
- **Language:** UI strings in English and identical to the Mac app's (inventory §7.1–§7.3 quote them), except deviations 6–7.
- **Shortcuts:** Ctrl+N Add Feed…, Ctrl+O Import OPML…, Ctrl+Shift+E Export OPML…, Ctrl+Shift+, Settings… (B3's actions).
- **Settings keys:** `reader.font` (`systemSans` default, `systemSerif`, `newsreader`, `atkinson`), `reader.size` (17, 12…28), `reader.width` (`narrow`, `medium` default, `wide`, `forty`), `databasePath`; `AppSettings` is the only writer of `ressac-kde.conf`.
- **Private drag type:** `application/x-ressac-feed`.
- **Never:** the real database; the network in tests (feeds and adds point at closed loopback ports); push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **An address carrying a secret** (`https://user:pass@host.example/feed?token=abc`) typed into Add Feed and failing → the dialog shows the core's sentence; the log file never holds `pass` or `token=abc`. Tests: Task 1 `redact_text_redacts_every_web_address_in_a_sentence`, `log_label_redacts_the_address_add_takes`; Task 5 uses them at every run log line.
2. **A big or broken OPML dropped while a refresh runs, or dropped twice** → the window never blocks (the preview waits on the worker), a second drop during a preview is ignored, Import is disabled with "A refresh is running; try again when it ends.", a parse error shows "This OPML file can't be imported" with the parser's sentence and writes nothing. Tests: Task 2 `the_preview_reads_the_dry_run_counts_and_writes_nothing`, `preview_failures_read_like_the_cli`, `import_is_blocked_with_a_message_while_a_refresh_runs`, `a_drop_during_a_preview_is_ignored`.
3. **A feed dropped where it already is, a stale or foreign payload, or a drag over the search field** → nothing runs, nothing is pasted. Tests: Task 2 `drop_on_own_folder_is_ignored`, `payload_round_trips_and_foreign_strings_are_ignored`; Task 3 `the_feed_s_folder_comes_from_the_sidebar_tree`; Task 7 walk `21-drag-root` (`ignored=false own=false notFeed=false`, feed 3 still at the top level until its drop on Walk).
4. **Settings ▸ Data pointed at a junk file, a newer database, the same path, or a new place** → the refusal sentence and the setting untouched, nothing for the same path, Relaunch offered only after a stored change; `RESSAC_DB` noted. Tests: Task 3 `a_non_sqlite_file_is_refused_and_the_setting_stays`, `a_newer_schema_is_refused`, `a_v1_file_a_missing_path_and_an_empty_file_are_accepted`, `the_same_path_changes_nothing`; Task 7 walk `29-junk`…`31-default`.
5. **Add pressed twice, or Cancel during an add** → one run; after Cancel the add still completes, selects and notes. Tests: Task 2 `add_cannot_start_twice`, `a_finished_add_after_cancel_still_reports`; Task 6 walk `12-add-refused`…`17-add-known`.

---

## B6a: redaction, the `library` rules

### Task 1 (B6a): the core redacts addresses, run log lines use it

**Files:**
- Modify: `crates/ressac-core/src/logging.rs` (add `redact_address`, `redact_text`, a `#[cfg(test)] mod tests`)
- Modify: `crates/ressac/src/commands/network.rs` (delete the private `redact_address` and its test; `use ressac_core::logging::redact_address;`)
- Modify: `crates/ressac-ui/src/runs.rs` (`RunRequest::log_label`; `admit`'s `tracing::info!("refused {}: a long run is active", request.log_label())`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `ressac_core::logging::{redact_address, redact_text}`, `RunRequest::log_label` (header).

- [ ] **Step 1: Write the failing tests.** In `logging.rs`'s new `mod tests` (the CLI test moved, plus the Mac's `LogRedactionTests` cases in the CLI's `?...` form):

```rust
#[test]
fn redact_address_drops_userinfo_query_and_fragment() {
    assert_eq!(redact_address("https://user:pass@host.example:8080/feed.xml?token=abc#top"), "https://host.example:8080/feed.xml?...");
    assert_eq!(redact_address("https://u:p@host.example?token=abc"), "https://host.example?...");
    assert_eq!(redact_address("https://host.example/a/b.xml"), "https://host.example/a/b.xml");
    assert_eq!(redact_address("https://host.example/@me/feed"), "https://host.example/@me/feed");
    assert_eq!(redact_address("host.example/feed#frag"), "host.example/feed");
    assert_eq!(redact_address("u:p@host.example/feed?k=v"), "host.example/feed?...");
    assert_eq!(redact_address("https://u:pa?ss@host.example/f"), "https://host.example/f");
    assert_eq!(redact_address("u:pa?ss@host/f"), "host/f");
    assert_eq!(redact_address("intranet?token=abc"), "intranet?...");
    assert_eq!(redact_address("u:p@[::1]:8080/feed"), "[::1]:8080/feed");
}

#[test]
fn redact_text_redacts_every_web_address_in_a_sentence() {
    assert_eq!(redact_text("Could not reach https://host.example/f?k=v: timed out."), "Could not reach https://host.example/f?...");
    assert_eq!(redact_text("Could not reach https://u:p@host.example/feed: refused."), "Could not reach https://host.example/feed: refused.");
    assert_eq!(redact_text("from HTTPS://u:p@h.example/x to http://h.example/y?z"), "from HTTPS://h.example/x to http://h.example/y?...");
    for plain in ["Already followed: xkcd.com.", "No folder #3.", "Enter a web address.", "feed theme 7 ai-labs a@b?c", ""] {
        assert_eq!(redact_text(plain), plain);
    }
}
```

In `crates/ressac-ui/src/runs.rs`'s tests:

```rust
#[test]
fn log_label_redacts_the_address_add_takes() {
    let add = RunRequest::Add { address: "https://u:p@host.example/f?token=abc".into(), theme: Some("tech".into()), folder: Some(3) };
    assert_eq!(add.log_label(), r#"Add { address: "https://host.example/f?...", theme: Some("tech"), folder: Some(3) }"#);
    let bare = RunRequest::Add { address: "intranet#k".into(), theme: None, folder: None };
    assert_eq!(bare.log_label(), r#"Add { address: "intranet", theme: None, folder: None }"#);
    assert_eq!(RunRequest::MoveFeed(12, None).log_label(), "MoveFeed(12, None)");
    assert_eq!(RunRequest::FetchAll.log_label(), "FetchAll");
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-core logging 2>&1 | tail -3` → `error[E0425]: cannot find function `redact_address``; `cargo test -p ressac-ui log_label 2>&1 | tail -3` → `no method named `log_label``.
- [ ] **Step 3: Implement.** `redact_address`: the CLI body, moved verbatim (the `ressac` binary's behaviour does not change). `redact_text`: split on whitespace keeping the separators (`split_inclusive(char::is_whitespace)`), and in each word find the first `http://` / `https://` (ASCII case-insensitive): keep the text before it, `redact_address` the rest of the word (its own trailing whitespace kept). `log_label`: clone, replace `Add`'s address with `redact_address(address)`, `format!("{:?}")`.
- [ ] **Step 4: Verify** — `cargo test -p ressac-core logging && cargo test -p ressac-ui log_label && cargo test -p ressac --lib network` → every `ok.` (`add_refusals_are_usage_errors_and_a_failed_reach_stays_fatal` unchanged). `rg -n 'fn redact_address' crates` → only `crates/ressac-core/src/logging.rs`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/logging.rs crates/ressac/src/commands/network.rs crates/ressac-ui/src/runs.rs
git commit -m "Core: address redaction for logs (moved from the CLI); run log lines redact add's address

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B6a): `ressac_ui::library` (add, OPML, first run, drag rules) and `runs::fetch`

**Files:**
- Create: `crates/ressac-ui/src/library.rs` (tests in its `mod tests`, using `crate::test_db`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod library;`)
- Modify: `crates/ressac-ui/src/runs.rs` (`pub async fn fetch`, the kde bridge's inline fetch job made shared: `let options = fetch_options(app.store(), request)?; app.refresh(options, sink, stop).await.map(drop)`)

**Interfaces:**
- Consumes: B2 `RunRequest`, `fetch_options`, `sources::*`, `test_db::{TestDb, today}`; B4 `RunEnd`, `RunFailure`; B3 `sanitize::is_web_url`; core `AddedFeed`, `ThemeSource`, `ImportReport`, `SourceNode`, `SourceNodeKind`, `opml::export`, `Ressac::{add_feed, import_opml}`.
- Produces: `ressac_ui::library` and `runs::fetch` (header).

Values: `import_summary` = the Mac's `OPMLFlow.summary` (`count(n, noun)` = `"<n> <noun>"` plus `s` unless `n == 1`): `feeds_added == 0` → `"No new feeds in this file."`, else `"<count(feeds, "feed")> in <count(folders, "new folder")>."`; merged not empty → `"<count(merged, "duplicate")> inside the file will be merged:"` then one `"“<dropped>” into “<kept>”"` per pair; `skipped_existing > 0` → `"<count(skipped, "feed")> already in Ressac will be skipped."`. `import_title` = `"Import “<file name>”?"`. `folder_choices` walks only `SourceNodeKind::Folder` nodes, depth first, children right after their parent. `export` writes with `std::fs::write` (an I/O error is `Error::Io`). `OpmlFlow::open` returns None (and keeps the step) when busy or when no URL is an `.opml` file.

- [ ] **Step 1: Write the failing tests** in `library.rs` (helpers: `fn added(theme: Option<&str>, like: Option<&str>) -> AddedFeed` building an `AddedFeed` with `theme_source: like.map_or(ThemeSource::None, |t| ThemeSource::Sibling { id: 9, title: t.into() })`; `fn report(feeds, folders, merged: &[(&str, &str)], skipped) -> ImportReport`; `fn node_folder(id, name, children) -> SourceNode`; `fn walk_opml(dir: &Path) -> PathBuf` writing the OPML of the "Validated" section with feeds `http://127.0.0.21:9/alpha.xml` (twice: "Alpha", "Alpha again") and `http://127.0.0.22:9/beta.xml` ("Beta") in folder "Walk", and "Gamma" `http://127.0.0.23:9/gamma.xml` at the top level):

```rust
// AddFeedTests
#[test]
fn add_request_carries_the_trimmed_address_theme_and_folder() {
    let mut form = AddFeedForm::new(None);
    form.address = "  df.example \n".into();
    form.theme = Some("tech".into());
    form.folder = Some(7);
    let request = form.start().unwrap();
    assert_eq!(request, RunRequest::Add { address: "df.example".into(), theme: Some("tech".into()), folder: Some(7) });
    assert_eq!(request.kind(), RunKind::Long);
    assert!(!request.shows_failure_banner(), "errors stay in the dialog");
    assert!(request.is_refusal(&ressac_core::Error::FeedExists { id: 1, title: "Daring Fireball".into(), buried: false }));
    assert!(!request.is_refusal(&ressac_core::Error::Unreachable { url: "https://df.example".into(), reason: "timed out".into() }));
}

#[test]
fn the_folder_pop_up_lists_the_top_level_then_every_folder_in_sidebar_order() {
    let tree = [node_folder(4, "Blogs", vec![]),
                node_folder(1, "News", vec![node_folder(2, "Sub", vec![node_folder(3, "Deep", vec![])])])];
    let choices = folder_choices(&tree);
    assert_eq!(choices.iter().map(|c| c.folder).collect::<Vec<_>>(), [None, Some(4), Some(1), Some(2), Some(3)]);
    assert_eq!(choices.iter().map(|c| c.depth).collect::<Vec<_>>(), [0, 1, 1, 2, 3]);
    assert_eq!(choices.iter().map(FolderChoice::title).collect::<Vec<_>>(),
        ["Feeds", "\u{2003}Blogs", "\u{2003}News", "\u{2003}\u{2003}Sub", "\u{2003}\u{2003}\u{2003}Deep"]);
    assert_eq!(AddFeedForm::new(None).folder, None, "the top level is the default");
    assert_eq!(folder_choices(&[]).iter().map(FolderChoice::title).collect::<Vec<_>>(), ["Feeds"]);
}

#[test]
fn a_folder_that_disappears_resets_the_choice_to_the_top_level() {
    let mut form = AddFeedForm::new(None);
    let choices = folder_choices(&[node_folder(3, "A", vec![])]);
    form.folder = Some(3);
    form.keep_folder(&choices);
    assert_eq!(form.folder, Some(3));
    form.keep_folder(&choices[..1]);
    assert_eq!(form.folder, None);
}

#[test]
fn an_empty_address_disables_add() {
    let mut form = AddFeedForm::new(None);
    assert!(!form.can_add());
    form.address = "   \n".into();
    assert!(!form.can_add() && form.start().is_none());
    form.address = " example.com ".into();
    assert!(form.can_add());
}

#[test]
fn add_cannot_start_twice() {
    let mut form = AddFeedForm::new(Some("https://example.com/feed.xml"));
    assert!(form.start().is_some());
    assert!(form.is_running() && !form.can_add() && form.start().is_none());
}

#[test]
fn clipboard_prefill_only_for_web_addresses() {
    assert_eq!(clipboard_address(Some("  https://example.com/feed.xml\n")).as_deref(), Some("https://example.com/feed.xml"));
    assert_eq!(clipboard_address(Some("http://example.com")).as_deref(), Some("http://example.com"));
    for refused in [Some("example.com"), Some("ftp://example.com/x"), Some("some words https://example.com"), Some("https://"), Some(""), None] {
        assert_eq!(clipboard_address(refused), None, "{refused:?}");
    }
    assert_eq!(AddFeedForm::new(Some("  https://example.com/feed.xml\n")).address, "https://example.com/feed.xml");
    assert_eq!(AddFeedForm::new(Some("not an address")).address, "");
}

#[test]
fn a_refusal_stays_in_the_dialog_without_the_log_button() {
    let mut form = AddFeedForm::new(Some("https://df.example"));
    form.start();
    let refusal = RunFailure { message: "Already followed: Daring Fireball.".into(), refusal: true };
    assert_eq!(form.finished(RunEnd::Failed(refusal)), None);
    assert_eq!((form.error(), form.shows_log(), form.is_running()), (Some("Already followed: Daring Fireball."), false, false));
}

#[test]
fn a_busy_refusal_shows_its_sentence_without_the_log_button() {
    let mut form = AddFeedForm::new(Some("https://df.example"));
    form.start();
    assert_eq!(form.finished(RunEnd::Refused), None);
    assert_eq!((form.error(), form.shows_log()), (Some("A refresh is running. Try again when it ends."), false));
}

#[test]
fn a_failure_keeps_the_dialog_and_offers_the_log() {
    let mut form = AddFeedForm::new(Some("https://df.example"));
    form.start();
    let failure = RunFailure { message: "Could not reach https://df.example: timed out.".into(), refusal: false };
    assert_eq!(form.finished(RunEnd::Failed(failure)), None);
    assert_eq!((form.error(), form.shows_log()), (Some("Could not reach https://df.example: timed out."), true));
    assert!(form.start().is_some(), "a new attempt clears the error");
    assert_eq!(form.error(), None);
}

#[test]
fn success_closes_the_dialog_and_reports_the_theme() {
    let mut form = AddFeedForm::new(Some("https://df.example"));
    form.start();
    let done = form.finished(RunEnd::Done(added(Some("Tech, Apple and geeks"), Some("Six Colors")))).unwrap();
    assert_eq!(added_note(&done), "In Tech, Apple and geeks, like Six Colors.");
    assert_eq!((form.error(), form.is_running()), (None, false));
}

#[test]
fn a_finished_add_after_cancel_still_reports() {
    // The bridge drops the form on Cancel; the run's end is still a Done with the feed to select.
    let end: RunEnd<AddedFeed> = RunEnd::Done(added(None, None));
    let RunEnd::Done(feed) = end else { unreachable!() };
    assert_eq!(added_note(&feed), "Added to Uncharted waters.");
}

#[test]
fn note_wording() {
    assert_eq!(added_note(&added(Some("Tech"), Some("B"))), "In Tech, like B.");
    assert_eq!(added_note(&added(Some("Tech"), None)), "In Tech.");
    assert_eq!(added_note(&added(None, None)), "Added to Uncharted waters.");
    assert_eq!(added_note(&added(None, Some("B"))), "Added to Uncharted waters.");
}

#[test]
fn the_add_job_refuses_private_known_buried_and_blank_addresses() {
    let db = TestDb::new();
    db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
    let app = Ressac::open(&db.path).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let try_add = |address: &str| runtime.block_on(add_feed(&app, address, None, None, &NullSink, Arc::new(AtomicBool::new(false))))
        .unwrap_err().to_string();
    assert_eq!(try_add("http://127.0.0.1:9/feed"), "Ressac only follows public web addresses.");
    assert_eq!(try_add("https://www.xkcd.com/rss.xml"), "Already followed: xkcd.com.");
    assert_eq!(try_add("http://api.flickr.com/services/feeds/photos_public.gne?id=48609153@N00&lang=en-us&format=rss_200"),
        "Already in the Cemetery: Photos from bouquiniste.");
    assert_eq!(try_add(" "), "Enter a web address.");
    let folder_gone = runtime.block_on(add_feed(&app, "http://127.0.0.1:9/feed", None, Some(99), &NullSink, Arc::new(AtomicBool::new(false)))).unwrap_err();
    assert_eq!(folder_gone.to_string(), "No folder #99.");
}

// FirstRunTests
#[test]
fn the_summary_describes_the_dry_run() {
    assert_eq!(import_summary(&report(186, 16, &[("Daring Fireball (1)", "Daring Fireball")], 3)), [
        "186 feeds in 16 new folders.",
        "1 duplicate inside the file will be merged:",
        "“Daring Fireball (1)” into “Daring Fireball”",
        "3 feeds already in Ressac will be skipped.",
    ]);
    assert_eq!(import_summary(&report(0, 0, &[], 1)), ["No new feeds in this file.", "1 feed already in Ressac will be skipped."]);
    assert_eq!(import_summary(&report(1, 1, &[], 0)), ["1 feed in 1 new folder."]);
    assert_eq!(import_title(Path::new("/tmp/My Feeds.opml")), "Import “My Feeds.opml”?");
}

#[test]
fn the_preview_reads_the_dry_run_counts_and_writes_nothing() {
    let db = TestDb::new();
    let app = Ressac::open(&db.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let before = app.store().feed_count().unwrap();
    let got = preview(&app, &walk_opml(dir.path())).unwrap();
    assert_eq!(got, report(3, 1, &[("Alpha again", "Alpha")], 0));
    assert_eq!(app.store().feed_count().unwrap(), before);
    let imported = import(&app, &walk_opml(dir.path())).unwrap();
    assert_eq!(imported.feeds_added, 3);
    assert_eq!(app.store().feed_count().unwrap(), before + 3);
}

#[test]
fn preview_failures_read_like_the_cli() {
    let db = TestDb::new();
    let app = Ressac::open(&db.path).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.opml");
    assert_eq!(preview(&app, &missing).unwrap_err().to_string(),
        format!("cannot read {}: No such file or directory (os error 2)", missing.display()));
    let bad = dir.path().join("bad.opml");
    std::fs::write(&bad, r#"<opml><body><outline text="x"></body>"#).unwrap();
    assert!(preview(&app, &bad).unwrap_err().to_string().starts_with("cannot read OPML at line 1, column 31: "));
}

#[test]
fn only_opml_files_are_accepted() {
    assert_eq!(first_opml(&["file:///tmp/feeds.xml".into(), "https://example.org/a.opml".into()]), None);
    assert_eq!(first_opml(&["file:///tmp/notes.txt".into(), "file:///tmp/My%20Feeds.OPML".into()]), Some(PathBuf::from("/tmp/My Feeds.OPML")));
    assert_eq!(first_opml(&["file:///tmp/My Feeds.opml".into()]), Some(PathBuf::from("/tmp/My Feeds.opml")));
    assert_eq!(local_path("file:///tmp/a%20b/Ressac.opml"), Some(PathBuf::from("/tmp/a b/Ressac.opml")));
    assert_eq!(local_path("ressac:asset/x"), None);
}

#[test]
fn confirming_imports_then_fetches_everything() {
    let mut flow = OpmlFlow::default();
    let path = flow.open(&["file:///tmp/notes.txt".into(), "file:///tmp/vienna.opml".into()]).unwrap();
    assert_eq!(path, PathBuf::from("/tmp/vienna.opml"));
    assert!(flow.is_busy());
    flow.previewed(&path, Ok(report(186, 16, &[], 3)));
    assert_eq!(flow.step().as_str(), "confirming");
    assert!(flow.can_import(false));
    assert_eq!(flow.confirm(false), Some(RunRequest::Import(path.clone())));
    assert!(flow.is_busy());
    assert_eq!(flow.imported(&RunEnd::Done(report(186, 16, &[], 3))), Some(RunRequest::FetchAll));
    assert!(flow.is_busy(), "busy until the fetch ends, as the Mac's spinner");
    flow.fetched();
    assert_eq!(flow.step(), &OpmlStep::Idle);
}

#[test]
fn import_is_blocked_with_a_message_while_a_refresh_runs() {
    let mut flow = OpmlFlow::default();
    let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
    flow.previewed(&path, Ok(report(2, 0, &[], 0)));
    assert!(!flow.can_import(true));
    assert_eq!(flow.confirm(true), None);
    assert_eq!(flow.step().as_str(), "confirming");
    assert_eq!(IMPORT_BLOCKED, "A refresh is running; try again when it ends.");
    assert_eq!(flow.confirm(false), Some(RunRequest::Import(path.clone())));
    assert_eq!(flow.imported(&RunEnd::Refused), None, "turned away anyway: back to the confirmation");
    assert_eq!(flow.step().as_str(), "confirming");
}

#[test]
fn a_file_with_no_new_feeds_cannot_be_imported() {
    let mut flow = OpmlFlow::default();
    let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
    flow.previewed(&path, Ok(report(0, 0, &[], 12)));
    assert!(!flow.can_import(false));
    assert_eq!(flow.confirm(false), None);
}

#[test]
fn a_failed_import_does_not_fetch() {
    let mut flow = OpmlFlow::default();
    let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
    flow.previewed(&path, Ok(report(2, 0, &[], 0)));
    flow.confirm(false);
    assert_eq!(flow.imported(&RunEnd::Failed(RunFailure { message: "database error: x".into(), refusal: false })), None);
    assert_eq!(flow.step(), &OpmlStep::Idle);
}

#[test]
fn preview_failures_are_shown_and_nothing_is_written() {
    let mut flow = OpmlFlow::default();
    let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
    flow.previewed(&path, Err("invalid OPML".into()));
    assert_eq!(flow.step(), &OpmlStep::Failed("invalid OPML".into()));
    assert!(!flow.is_busy());
    flow.cancel();
    assert_eq!(flow.step(), &OpmlStep::Idle);
}

#[test]
fn a_drop_during_a_preview_is_ignored() {
    let mut flow = OpmlFlow::default();
    let first = flow.open(&["file:///tmp/a.opml".into()]).unwrap();
    assert_eq!(flow.open(&["file:///tmp/b.opml".into()]), None);
    flow.previewed(Path::new("/tmp/b.opml"), Ok(report(9, 0, &[], 0)));   // not the one being previewed
    assert_eq!(flow.step(), &OpmlStep::Previewing(first));
    flow.cancel();
    assert!(flow.is_busy(), "a preview under way cannot be cancelled");
}

#[test]
fn export_writes_the_active_feeds() {
    let db = TestDb::new();
    db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
    let store = db.store();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join(EXPORT_NAME);
    export(&store, &target).unwrap();
    let xml = std::fs::read_to_string(&target).unwrap();
    assert!(xml.contains(r#"xmlUrl="https://www.xkcd.com/rss.xml""#));
    assert!(!xml.contains("Photos from bouquiniste"), "the buried feed is not exported");
    let request = RunRequest::Export { path: target, cemetery: false };
    assert_eq!(request.kind(), RunKind::Short);
    assert!(export(&store, &dir.path().join("no/such/dir/x.opml")).is_err());
}

// MoveFeedTests
#[test]
fn move_request_is_short_and_its_refusals_are_sentences() {
    assert_eq!(RunRequest::MoveFeed(7, Some(3)).kind(), RunKind::Short);
    assert!(RunRequest::MoveFeed(7, Some(3)).is_refusal(&ressac_core::Error::UnknownFolder(3)));
}

#[test]
fn drop_on_own_folder_is_ignored() {
    assert_eq!(decide_drop(Some(3), Some(3)), FeedDrop::Ignore);
    assert_eq!(decide_drop(None, None), FeedDrop::Ignore, "a top-level feed dropped on the Feeds heading stays");
}

#[test]
fn drop_maps_a_feed_onto_a_folder() {
    assert_eq!(decide_drop(None, Some(3)), FeedDrop::Move(Some(3)));
    assert_eq!(decide_drop(Some(2), Some(3)), FeedDrop::Move(Some(3)));
    assert_eq!(decide_drop(Some(2), None), FeedDrop::Move(None), "the Feeds heading is the top level");
}

#[test]
fn payload_round_trips_and_foreign_strings_are_ignored() {
    assert_eq!(FEED_MIME, "application/x-ressac-feed");
    assert_eq!(feed_payload(42), "42");
    assert_eq!(feed_from_payload("42"), Some(42));
    for foreign in ["", "x", "1 2", "-1", "+3", " 4", "4 ", "ressac-feed:4", "４２", "99999999999999999999"] {
        assert_eq!(feed_from_payload(foreign), None, "{foreign:?}");
    }
}

#[test]
fn drop_targets_are_folders_and_the_feeds_heading() {
    assert_eq!(drop_target("folder:7"), Some(Some(7)));
    assert_eq!(drop_target("heading:Feeds"), Some(None));
    for other in ["feed:7", "heading:Rhythm", "unread", "garden:all", "folder:", "folder:x"] {
        assert_eq!(drop_target(other), None, "{other}");
    }
}

#[test]
fn an_entry_drags_out_as_its_link_and_title() {
    assert_eq!(entry_drag_data("https://example.org/a", "A title"), [
        ("text/uri-list", "https://example.org/a\r\n".to_string()),
        ("text/plain", "https://example.org/a".to_string()),
        ("text/x-moz-url", "https://example.org/a\nA title".to_string()),
    ]);
    assert!(entry_drag_data("javascript:alert(1)", "x").is_empty());
    assert!(entry_drag_data("", "x").is_empty());
}
```

(`TestDb::new()` is B2's copy of `tests/fixtures/contract.sqlite` with every feed active: the tests that need the buried feed 4 bury it again.)

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui library 2>&1 | tail -3` → `error[E0432]: unresolved import` (`library`).
- [ ] **Step 3: Implement** `library.rs`, `lib.rs`, `runs::fetch` as in the header and the values above.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; `cargo clippy -p ressac-ui --all-targets -- -D warnings` → `Finished`. Then the full gate (B6a ends here; the kde fetch path still has its own job until Task 5).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/library.rs crates/ressac-ui/src/lib.rs crates/ressac-ui/src/runs.rs
git commit -m "ressac-ui: library rules (add form and note, folder choices, OPML flow, preview/import/export jobs, drag rules)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B6b: sidebar, settings and Data-page rules, their bridges

### Task 3 (B6b): `sources`, `settings`, `launch` additions

**Files:**
- Modify: `crates/ressac-ui/src/sources.rs`, `crates/ressac-ui/src/settings.rs`, `crates/ressac-ui/src/launch.rs` (tests in each `mod tests`)

**Interfaces:**
- Consumes: B1 `Settings`, `SettingsStore`, `location_problem`; B2 `SidebarTree`; core `Store::probe`, `SchemaState`.
- Produces: the three additions (header).

- [ ] **Step 1: Write the failing tests.**

`sources.rs`:

```rust
#[test]
fn the_feed_s_folder_comes_from_the_sidebar_tree() {
    let db = TestDb::new();
    db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
    let store = db.store();
    let mut tree = SidebarTree::default();
    assert!(!tree.is_empty_library(), "not loaded yet: never the first-run screen");
    assert!(tree.nodes().is_empty());
    tree.load(&store, today()).unwrap();
    assert!(!tree.is_empty_library());
    assert_eq!(tree.folder_of(2), Some(Some(2)), "Orange Drenka sits in potes");
    assert_eq!(tree.folder_of(1), Some(Some(1)));
    assert_eq!(tree.folder_of(4), None, "a buried feed is not in the sidebar");
    assert_eq!(tree.folder_of(999), None);
}

#[test]
fn a_library_without_active_feeds_is_empty() {
    let db = TestDb::new();
    db.execute("UPDATE feed SET state = 'buried'");
    let store = db.store();
    let mut tree = SidebarTree::default();
    tree.load(&store, today()).unwrap();
    assert!(tree.is_empty_library());
}
```

`settings.rs`:

```rust
#[test]
fn fonts_and_widths_have_their_settings_titles() {
    assert_eq!(ReaderFont::ALL.map(ReaderFont::title), ["System Sans", "System Serif", "Newsreader", "Atkinson Hyperlegible Next"]);
    assert_eq!(ReaderFont::ALL.map(ReaderFont::family), ["", "serif", "Newsreader", "Atkinson Hyperlegible Next"]);
    assert_eq!(LineWidth::ALL.map(LineWidth::title), ["Narrow", "Medium", "Wide", "40 columns"]);
    assert_eq!(PREVIEW_TEXT, "The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse.");
    assert_eq!(SIZE_HINT, "Ctrl++ and Ctrl+− change the text size while you read; Ctrl+0 restores it.");
}
```

`launch.rs` (the Mac's `DatabaseChoiceTests`; helpers `fn probe(path: &Path) -> Result<SchemaState, ressac_core::Error> { Store::probe(path) }` and a v1 / v99 file written with `rusqlite`: `CREATE TABLE schema_meta (version INTEGER NOT NULL); INSERT INTO schema_meta VALUES (1)`):

```rust
#[test]
fn a_non_sqlite_file_is_refused_and_the_setting_stays() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.sqlite");
    std::fs::write(&junk, "this is not sqlite, just some text padding").unwrap();
    let mut store = BTreeMap::from([("databasePath".to_string(), "/old/path.sqlite".to_string())]);
    let mut settings = Settings::load(&store);
    assert_eq!(choose_database(&mut settings, &mut store, junk.to_str().unwrap(), probe), DatabaseChoice::Refused(NOT_A_DATABASE));
    assert_eq!(store["databasePath"], "/old/path.sqlite");
    assert_eq!(settings.database_path.as_deref(), Some("/old/path.sqlite"));
}

#[test]
fn a_v1_file_a_missing_path_and_an_empty_file_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let v1 = schema_file(dir.path(), "v1.sqlite", 1);
    let mut store = BTreeMap::new();
    let mut settings = Settings::load(&store);
    assert_eq!(choose_database(&mut settings, &mut store, v1.to_str().unwrap(), probe), DatabaseChoice::Stored);
    assert_eq!(store["databasePath"], v1.to_str().unwrap());
    let fresh = dir.path().join("new.sqlite");
    assert_eq!(choose_database(&mut settings, &mut store, fresh.to_str().unwrap(), probe), DatabaseChoice::Stored);
    assert!(!fresh.exists(), "created by the next launch, not by Settings");
    let empty = dir.path().join("empty.sqlite");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(choose_database(&mut settings, &mut store, empty.to_str().unwrap(), probe), DatabaseChoice::Stored);
}

#[test]
fn a_newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let future = schema_file(dir.path(), "future.sqlite", 99);
    let mut store = BTreeMap::new();
    let mut settings = Settings::load(&store);
    assert_eq!(choose_database(&mut settings, &mut store, future.to_str().unwrap(), probe), DatabaseChoice::Refused(NEWER_DATABASE));
    assert!(!store.contains_key("databasePath"));
}

#[test]
fn the_same_path_changes_nothing_and_blank_is_use_default() {
    let mut store = BTreeMap::from([("databasePath".to_string(), "/a/r.sqlite".to_string())]);
    let mut settings = Settings::load(&store);
    let never = |_: &Path| -> Result<SchemaState, ressac_core::Error> { panic!("not probed") };
    assert_eq!(choose_database(&mut settings, &mut store, "/a/r.sqlite", never), DatabaseChoice::Unchanged);
    assert_eq!(choose_database(&mut settings, &mut store, "", never), DatabaseChoice::Stored);
    assert!(!store.contains_key("databasePath") && settings.database_path.is_none());
    assert_eq!(choose_database(&mut settings, &mut store, "", never), DatabaseChoice::Unchanged);
}

#[test]
fn the_data_notes_follow_the_setting() {
    assert_eq!(database_note(None), "Shared with the ressac command in Terminal.");
    assert_eq!(database_note(Some("/a/r.sqlite")), "In Terminal, pass --db with this path to use the same database.");
    assert_eq!(RESSAC_DB_NOTE, "RESSAC_DB is set, so this launch uses it instead.");
    assert_eq!(RELAUNCH_NOTE, "Ressac opens the new database after a relaunch.");
    assert_eq!((CHOOSE_TITLE, NEW_TITLE, NEW_NAME), ("Choose a Ressac database.", "Ressac creates the database here on its next launch.", "ressac.sqlite"));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui 2>&1 | grep -E "^error" | head -3` → `no method named `folder_of``, `no function `title``, `cannot find function `choose_database``.
- [ ] **Step 3: Implement** the three additions (`choose_database` writes through `Settings::set_database_path`; `is_empty_library` = a snapshot is loaded and no `SourceNodeKind::Feed` anywhere in the tree; `folder_of` walks the tree keeping the folder above each feed).
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/sources.rs crates/ressac-ui/src/settings.rs crates/ressac-ui/src/launch.rs
git commit -m "ressac-ui: empty library and a feed's folder, settings titles, Data page choose rule and notes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 4 (B6b): `SourcesModel.emptyLibrary` and `dropFeed`, `AppSettings` Data page

**Files:**
- Modify: `apps/linux/src/sources.rs` (`#[qproperty(bool, empty_library, cxx_name = "emptyLibrary", READ, NOTIFY = library_changed)]`, `#[qsignal] fn library_changed`, `#[qinvokable] fn drop_feed(self: Pin<&mut SourcesModel>, payload: &QString, target_key: &QString) -> bool`)
- Modify: `apps/linux/src/settings.rs` (the `AppSettings` additions of the header)

**Interfaces:**
- Consumes: Tasks 2–3; B4 `runs::submit_short`; B1 `QSettingsStore`, `database_location`, `Paths`.
- Produces: `SourcesModel.{emptyLibrary, dropFeed}`, `AppSettings` additions (header).

Behaviour:
- `emptyLibrary` = `tree.is_empty_library()`; after each reload, `library_changed` when it flipped.
- `dropFeed(payload, key)`: `feed_from_payload` → `drop_target(key)` → `tree.folder_of(feed)` (None → false) → `decide_drop`; `Ignore` → false; `Move(target)` → `runs::submit_short(RunRequest::MoveFeed(feed, target), move |cx| cx.app.store().move_feed(feed, target).map(drop), |_| {})`, true. The reload comes from `Runs.databaseChanged`; the selection is a key and stays.
- `AppSettings`: lists from `ReaderFont::ALL` / `LineWidth::ALL` (`as_str`, `title`, `family`); `resolvedDatabasePath` = `database_location(None, settings.database_path.as_deref(), home, Paths::from_env().ok().map(|p| p.database()).as_deref())` displayed with `display()` (`""` on error); `databaseNote` = `database_note(settings.database_path.as_deref())`; `ressacDbSet` = `RESSAC_DB` set and non-empty at start; `chooseDatabase(url)` = `library::local_path` (not a file URL → nothing) then `choose_database(…, Store::probe)`: `Refused(s)` → `databaseRefusal = s`; `Stored` → refusal cleared, `needsRelaunch = true`, `database_path_changed`; `Unchanged` → refusal cleared; always `data_changed`. `useDefaultLocation()` = the same with `""`. `relaunch()` = `std::process::Command::new(std::env::current_exe()?).spawn()` → true (QML then quits), an error → `tracing::warn!` and false.

- [ ] **Step 1: Implement** the two files (no unit test here: the rules are Tasks 2–3's; Task 7's walk drives them).
- [ ] **Step 2: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.`. Then the full gate (B6b ends here).
- [ ] **Step 3: Commit**

```bash
git add apps/linux/src/sources.rs apps/linux/src/settings.rs
git commit -m "ressac-kde: SourcesModel empty library and feed drops, AppSettings Data page and font lists

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B6c: the `Library` singleton and the shim

### Task 5 (B6c): `Library`, clipboard, fonts, focused-window grab, redacted run logs

**Files:**
- Create: `apps/linux/src/library.rs`
- Modify: `apps/linux/src/lib.rs` (`mod library;`; in the `shim` bridge `fn ressac_clipboard_text() -> String;`)
- Modify: `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp` (`<QtGui/QClipboard>`, `<QtGui/QFontDatabase>`; `rust::String ressac_clipboard_text()`; the fonts in `ressac_create_application()`; the focused-window rule in `ressac_grab_window`)
- Modify: `apps/linux/src/runs.rs` (the fetch path's job becomes `move |cx, sink, stop| cx.block_on(ressac_ui::runs::fetch(cx.app, &request, sink, stop))`; the refusal / failure log lines become `tracing::info!("{} refused: {}", request.log_label(), redact_text(&failure.message))` and `tracing::warn!("{} failed: {}", …)`)
- Test (not counted): `apps/linux/tests/common/mod.rs` (`pub fn assert_drawn_min(png: &Path, width: u32, height: u32)`; `assert_drawn(png)` = `assert_drawn_min(png, 900, 500)`)

**Interfaces:**
- Consumes: Tasks 1–4; B5 `garden::{theme_names, theme_choice}`; B4 `submit_long`, `submit_short`, `RunEnd`; B2 `with_session`, `Session.worker`, `today_start`.
- Produces: `Library`, `ressac_clipboard_text`, the grab rule (header).

Behaviour (`LibraryRust { form: Option<AddFeedForm>, themes: Vec<ThemeDef>, folders: Vec<FolderChoice>, note: QString, flow: OpmlFlow }`):
- `beginAdd()`: `form = Some(AddFeedForm::new(Some(&shim::ressac_clipboard_text())))`, `reloadChoices()`, `add_changed`. `addOpen` = `form.is_some()`. Getters read the form: `themeIndex` = `theme_index(themes, form.theme)`, `folderIndex` = the index of `form.folder` in `folders`; writes: `address` sets `form.address`; `themeIndex` → `theme_choice` → `form.theme`; `folderIndex` → `folders[i].folder`.
- `reloadChoices()` (while a form exists): `with_session(|s| (s.store.themes(), s.store.sources(today_start(Local::now()))))` → `themes`, `folders = folder_choices(&snapshot.tree)`, `form.keep_folder(&folders)`; an error keeps the old lists (`tracing::warn!`).
- `add()`: `form.start()` → `Some(RunRequest::Add { address, theme, folder })` → `submit_long(request, move |cx, sink, stop| cx.block_on(library::add_feed(cx.app, &address, theme.as_deref(), folder, sink, stop)), |_| None, |_| {}, end)` where `end` queues `add_finished(end)` onto `Library`; `add_changed`.
- `add_finished(end)`: `let feed = match &mut form { Some(f) => f.finished(end), None => match end { RunEnd::Done(feed) => Some(feed), _ => None } }`; with a feed: `form = None`, `note = added_note(&feed)`, `note_changed`, `added(format!("feed:{}", feed.id))`; `add_changed`.
- `cancelAdd()`: `form = None`, `add_changed` (a running add continues).
- `openUrls(urls)`: `flow.open(&urls)` → `Some(path)` → `with_session(|s| s.worker.short(move |cx| library::preview(cx.app, &path), done))` (no session → `flow.previewed(&path, Err("The database is not open.".into()))`), `done` queues `preview_done(path, result.map_err(|f| f.message()))` → `flow.previewed`; `opml_changed`; returns whether a preview started.
- `confirmImport(blocked)`: `flow.confirm(blocked)` → `submit_long(Import(path), move |cx, _, _| library::import(cx.app, &path), |_| None, |_| {}, end → import_done)`. `import_done(end)`: `flow.imported(&end)` → `Some(FetchAll)` → `submit_long(FetchAll, move |cx, sink, stop| cx.block_on(runs::fetch(cx.app, &RunRequest::FetchAll, sink, stop)), |_| None, |_| {}, end → fetch_done)`; `fetch_done(_)` → `flow.fetched()`. Each → `opml_changed`.
- `cancelImport()` → `flow.cancel()`. Getters: `opmlStep` = `as_str`; `busy`; `importTitle`/`importLines`/`importNote` from `Confirming` (else empty; `importNote` = `IMPORT_NOTE` when `feeds_added > 0`); `canImport` = `flow.can_import(false)`; `failure` from `Failed`.
- `exportTo(url)`: `local_path(url)` → `submit_short(RunRequest::Export { path, cemetery: false }, move |cx| library::export(cx.app.store(), &path), |_| {})` → true; else false.
- `logsUrl()`: `Paths::from_env()` → `log_dir()`, `create_dir_all`, `format!("file://{}", dir.display())` (via `url::Url::from_directory_path`); `""` on error.
- `entryDragData(link, title)`: the `entry_drag_data` pairs as a `QMap_QString_QVariant` of `QString` values.
- Shim: `ressac_clipboard_text()` = `QGuiApplication::clipboard()->text()`; fonts: for each of the four files, `rust::String mime; rust::Vec<uint8_t> body; if (ressac_scheme_serve(rust::Str("ressac:asset/" + file), mime, body))` then `QFontDatabase::addApplicationFontFromData(QByteArray(reinterpret_cast<const char *>(body.data()), body.size()))`, `-1` → `qWarning("ressac-kde: cannot register %s", file)`; grab: `if (auto *w = qobject_cast<QQuickWindow *>(QGuiApplication::focusWindow()); w && w->isVisible()) return w->grabWindow().save(...)`, else B1's loop.

- [ ] **Step 1: Verify the current behaviour is pinned** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.` (the fetch path and the walks before the change).
- [ ] **Step 2: Implement** the files as above.
- [ ] **Step 3: Verify** — the same command → every `ok.` (every walk's main-window shots are unchanged: the main window is the focused one); `cargo clippy` as in Task 4 → `Finished`. Then the full gate (B6c ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/library.rs apps/linux/src/lib.rs apps/linux/cpp/shim.h apps/linux/cpp/shim.cpp apps/linux/src/runs.rs apps/linux/tests/common/mod.rs
git commit -m "ressac-kde: Library singleton (add, OPML preview/import/export, logs), clipboard, bundled fonts, focused-window grab

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B6d: add feed, OPML and first run on screen

### Task 6 (B6d): dialogs, first run, window drop, note, menu wiring, library walk (part 1)

**Files:**
- Create: `apps/linux/qml/AddFeedDialog.qml`, `apps/linux/qml/OpmlDialogs.qml`, `apps/linux/qml/FirstRun.qml`
- Modify: `apps/linux/qml/Main.qml`
- Test (not counted): `apps/linux/tests/library.rs` (new)

**Interfaces:**
- Consumes: Task 5; B4's `steps` chooser; B3's `mainMenu` action aliases; B2's `ReadingPage`.
- Produces: the QML items of the header (`Main.qml`'s `openSettings()` comes in Task 7).

`AddFeedDialog.qml` (inventory §7.1; root `QQC2.Dialog`, `modal: true`, `anchors.centerIn: parent`, `width: 420`, `title: Library.addTitle`, `closePolicy: QQC2.Popup.CloseOnEscape`), visible while `Library.addOpen` (`Connections { target: Library; function onAddChanged() { if (Library.addOpen && !dialog.visible) dialog.open(); if (!Library.addOpen && dialog.visible) dialog.close() } }`, `onClosed: if (Library.addOpen) Library.cancelAdd()`):
- `QQC2.TextField { id: address; placeholderText: Library.addressPlaceholder; text: Library.address; onTextEdited: Library.address = text; enabled: !Library.adding; onAccepted: dialog.accept(); focus: true }` (focused on open);
- under it, when `Library.addError !== ""`: a `RowLayout` with `QQC2.Label { text: Library.addError; color: Kirigami.Theme.negativeTextColor; textFormat: Text.PlainText; wrapMode: Text.Wrap }` and, when `Library.addShowsLog`, `QQC2.Button { text: "Show Log"; onClicked: root.openExternally(Library.logsUrl()) }` (small);
- `Kirigami.FormLayout` rows "Theme" (`QQC2.ComboBox { model: Library.themeNames; currentIndex: Library.themeIndex; onActivated: (i) => Library.themeIndex = i; enabled: !Library.adding }`) and "Folder" (`QQC2.ComboBox { model: Library.folderTitles; currentIndex: Library.folderIndex; onActivated: … }`);
- `footer`: a `RowLayout` with, while `Library.adding`, `QQC2.ProgressBar { width: 120; indeterminate: Runs.progressFraction < 0; value: Math.max(0, Runs.progressFraction) }`, a filler, `QQC2.Button { text: "Cancel"; onClicked: Library.cancelAdd() }`, `QQC2.Button { text: "Add"; highlighted: true; enabled: Library.canAdd; onClicked: dialog.accept() }`;
- `function accept() { if (Library.canAdd) Library.add() }` (the dialog stays open; `Library` closes it on success).

`OpmlDialogs.qml` (root `Item`; inventory §7.2):
- `FileDialog { id: importFile; nameFilters: [Library.opmlFilter]; onAccepted: Library.openUrls([String(selectedFile)]) }`; `function chooseImport() { if (!Library.busy) importFile.open() }`.
- `FileDialog { id: exportFile; fileMode: FileDialog.SaveFile; defaultSuffix: "opml"; nameFilters: [Library.opmlFilter]; onAccepted: Library.exportTo(String(selectedFile)) }`; `function chooseExport() { exportFile.currentFolder = StandardPaths.writableLocation(StandardPaths.DocumentsLocation); exportFile.selectedFile = exportFile.currentFolder + "/" + Library.exportName; exportFile.open() }` (`import QtCore`; the name set just before opening, validated finding (d)).
- The confirmation: `QQC2.Dialog { id: confirm; modal: true; anchors.centerIn: parent; width: 420; title: Library.importTitle }` opened when `Library.opmlStep === "confirming"` (and closed otherwise, by `Connections` on `opmlChanged`); body: the `Library.importLines` as `QQC2.Label`s (`textFormat: Text.PlainText`, wrapping), `Library.importNote` (small, `disabledTextColor`, hidden when empty), `Library.importBlocked` (small, `disabledTextColor`, `visible: Runs.longRunActive`); footer `QQC2.DialogButtonBox` with `QQC2.Button { text: "Cancel"; DialogButtonBox.buttonRole: RejectRole }` and `QQC2.Button { text: "Import"; DialogButtonBox.buttonRole: AcceptRole; enabled: Library.canImport && !Runs.longRunActive }`; `onAccepted: Library.confirmImport(Runs.longRunActive)`; `onRejected: Library.cancelImport()`. `function confirmImport() { confirm.accept() }`.
- The failure: `QQC2.Dialog { title: Library.cantImport }` with `QQC2.Label { text: Library.failure; textFormat: Text.PlainText; wrapMode: Text.Wrap }` and one `QQC2.Button { text: "OK" }` (`AcceptRole`), opened when `Library.opmlStep === "failed"`, `onClosed: Library.cancelImport()`.

`FirstRun.qml` (root `Item`; `signal chooseRequested()`): `Kirigami.PlaceholderMessage { anchors.centerIn: parent; icon.name: "view-pim-news"; text: Library.noFeedsTitle; explanation: Library.noFeedsText; helpfulAction: Kirigami.Action { text: Library.chooseOpml; enabled: !Library.busy; onTriggered: chooseRequested() } }` and `QQC2.BusyIndicator { visible: Library.busy; running: visible }` under it.

`Main.qml`:
- The ready content: `ReadingPage`'s `Loader` gets `visible: !SourcesModel.emptyLibrary`; a `FirstRun { anchors.fill: parent; visible: Launcher.phase === Launcher.Ready && SourcesModel.emptyLibrary; onChooseRequested: root.importOpml() }`; `menuBar: MainMenu { page: SourcesModel.emptyLibrary ? null : reading.item }`.
- `DropArea { id: fileDrop; anchors.fill: parent; keys: ["text/uri-list"]; enabled: Launcher.phase === Launcher.Ready; onDropped: (drop) => { if (!Library.busy && Library.openUrls(drop.urls.map(String))) drop.accept(Qt.CopyAction) } }` declared before the content (under it), plus the dashed overlay `Rectangle` (decision above) `visible: fileDrop.containsDrag`, `enabled: false`.
- The note bar as the window `footer` (`visible: Library.note !== ""`): `Kirigami.Icon { source: "emblem-ok-symbolic"; color: Kirigami.Theme.positiveTextColor }`, `QQC2.Label { text: Library.note; maximumLineCount: 2; wrapMode: Text.Wrap; textFormat: Text.PlainText }`, a filler, `QQC2.ToolButton { icon.name: "window-close"; display: QQC2.AbstractButton.IconOnly; text: "Dismiss"; onClicked: Library.dismissNote() }`; `Timer { interval: Library.noteSeconds * 1000; running: Library.note !== "" }` restarted on `noteChanged`, `onTriggered: Library.dismissNote()`.
- `Connections { target: Library; function onAdded(key) { SourcesModel.selectKey(key) } }`; `AddFeedDialog { id: addFeedDialog }`; `OpmlDialogs { id: opml }`; `TextEdit { id: clipboardProbe; visible: false }` (walk only: puts text on the clipboard).
- `function addFeed() { Library.beginAdd() }`, `function importOpml() { opml.chooseImport() }`, `function exportOpml() { opml.chooseExport() }`, `function openExternally(url) { if (Recette.active) console.warn("open: " + url); else Qt.openUrlExternally(url) }`.
- Menu wiring (deviation 11): `Binding { target: mainMenu.addFeed; property: "enabled"; value: Launcher.phase === Launcher.Ready }`, the same for `importOpml` (`… && !Library.busy`) and `exportOpml`; `Connections { target: mainMenu.addFeed; function onTriggered() { root.addFeed() } }`, likewise `importOpml` → `importOpml()`, `exportOpml` → `exportOpml()`.
- `reloadAll` reach: `Connections { target: Runs; function onDatabaseChanged() { Library.reloadChoices() } }`.
- The chooser gains `Recette.walk === "library" ? librarySteps : …`, and (`const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))`, the test's files under it):

```qml
readonly property var librarySteps: [
    { name: "02-first-run", enter: function () { console.warn("walk: 02-first-run empty=" + SourcesModel.emptyLibrary
        + " add=" + mainMenu.addFeed.enabled + " import=" + mainMenu.importOpml.enabled + " page=" + (mainMenu.page === null)) } },
    { name: "03-drop", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
        console.warn("walk: 03-drop started=" + Library.openUrls([home + "/notes.txt", home + "/walk.opml"]) + " again="
            + Library.openUrls([home + "/walk.opml"]) + " busy=" + Library.busy) } },
    { name: "04-wait", enter: function () {} },
    { name: "05-confirm", enter: function () { console.warn("walk: 05-confirm step=" + Library.opmlStep + " title=" + Library.importTitle
        + " lines=" + Library.importLines.join("|") + " note=" + Library.importNote + " canImport=" + Library.canImport) } },
    { name: "06-import", enter: function () { opml.confirmImport() } },
    { name: "07-wait", enter: function () {} },
    { name: "08-wait", enter: function () {} },
    { name: "09-wait", enter: function () {} },
    { name: "10-imported", enter: function () { console.warn("walk: 10-imported empty=" + SourcesModel.emptyLibrary + " step=" + Library.opmlStep
        + " walk=" + (SourcesModel.rowOfKey("folder:1") >= 0) + " gamma=" + (SourcesModel.rowOfKey("feed:3") >= 0) + " longRun=" + Runs.longRunActive) } },
    { name: "11-add-dialog", enter: function () { clipboardProbe.text = "  https://example.org/feed.xml\n"; clipboardProbe.selectAll(); clipboardProbe.copy()
        root.addFeed(); console.warn("walk: 11-add-dialog open=" + Library.addOpen + " address=" + Library.address + " themes=" + Library.themeNames.join("|")
            + " folders=" + Library.folderTitles.join("|") + " canAdd=" + Library.canAdd) } },
    { name: "12-add-refused", enter: function () { Library.address = "http://127.0.0.1:9/feed"; addFeedDialog.accept() } },
    { name: "13-wait", enter: function () {} },
    { name: "14-add-refusal", enter: function () { console.warn("walk: 14-add-refusal error=" + Library.addError + " showLog=" + Library.addShowsLog
        + " open=" + Library.addOpen + " adding=" + Library.adding) } },
    { name: "15-add-known", enter: function () { Library.address = "http://127.0.0.21:9/alpha.xml"; Library.folderIndex = 1; addFeedDialog.accept() } },
    { name: "16-wait", enter: function () {} },
    { name: "17-add-known", enter: function () { console.warn("walk: 17-add-known error=" + Library.addError + " showLog=" + Library.addShowsLog)
        Library.cancelAdd(); console.warn("walk: 17-closed open=" + Library.addOpen) } },
    { name: "18-export", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
        console.warn("walk: 18-export started=" + Library.exportTo(home + "/" + Library.exportName) + " refused=" + Library.exportTo("https://example.org/x.opml")) } },
    { name: "19-wait", enter: function () {} },
]
```

- [ ] **Step 1: Write the failing test** `apps/linux/tests/library.rs`:

```rust
//! Add feed, OPML, first run, Settings and drag and drop, offscreen (spec §7.7; inventory §2.9, §7).
#![cfg(target_os = "linux")]

mod common;
use common::*;

const WALK_OPML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><head><title>Walk</title></head><body>
<outline text="Walk">
<outline text="Alpha" type="rss" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline text="Alpha again" type="rss" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline text="Beta" type="rss" xmlUrl="http://127.0.0.22:9/beta.xml"/>
</outline>
<outline text="Gamma" type="rss" xmlUrl="http://127.0.0.23:9/gamma.xml"/>
</body></opml>"#;

/// An empty library (RESSAC_DB names a missing file: the launch creates it) and the walk's files.
fn prepare(run: &Run) -> std::path::PathBuf {
    std::fs::write(run.home.join("walk.opml"), WALK_OPML).unwrap();
    std::fs::write(run.home.join("notes.txt"), "not a feed list").unwrap();
    std::fs::write(run.home.join("junk.sqlite"), "this is not sqlite, just some text padding").unwrap();
    run.home.join("library.sqlite")
}

#[test]
fn first_run_imports_an_opml_then_adds_moves_exports_and_sets_the_database() {
    let run = Run::new();
    let db = prepare(&run);
    let out = run.launch_with(Some(&db), &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "library")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-first-run empty=true add=true import=true page=true",
        "walk: 03-drop started=true again=false busy=true",
        "walk: 05-confirm step=confirming title=Import “walk.opml”? lines=3 feeds in 1 new folder.|1 duplicate inside the file will be merged:|“Alpha again” into “Alpha” note=Ressac fetches every feed right after the import. canImport=true",
        "walk: 10-imported empty=false step=idle walk=true gamma=true longRun=false",
        "walk: 11-add-dialog open=true address=https://example.org/feed.xml themes=None folders=Feeds|\u{2003}Walk canAdd=true",
        "walk: 14-add-refusal error=Ressac only follows public web addresses. showLog=false open=true adding=false",
        "walk: 17-add-known error=Already followed: Alpha. showLog=false",
        "walk: 17-closed open=false",
        "walk: 18-export started=true refused=false",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    for shot in ["01-ready.png", "05-confirm.png", "11-add-dialog.png", "14-add-refusal.png", "10-imported.png"] {
        assert_drawn(&run.shots.join(shot));
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let folders: Vec<(i64, String)> = conn.prepare("SELECT id, name FROM folder ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(folders, [(1, "Walk".to_string())]);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM feed", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 3, "the refused adds wrote nothing");
    let attempts: i64 = conn.query_row("SELECT COUNT(*) FROM fetch_log", [], |r| r.get(0)).unwrap();
    assert!(attempts >= 3, "the import was followed by a fetch of every feed");
    let exported = std::fs::read_to_string(run.home.join("Ressac.opml")).unwrap();
    assert!(exported.contains(r#"xmlUrl="http://127.0.0.22:9/beta.xml""#), "{exported}");
}
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test library 2>&1 | grep -E "panicked|test result"'` → FAIL (`walk: 02-first-run … missing`).
- [ ] **Step 3: Implement** the four QML files.
- [ ] **Step 4: Verify and look** — the same command → `ok.`; `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.` (the other walks unchanged: their library is not empty). If a `*-wait` is short for the import and its fetch, add one more empty wait step (never shorten the walk's 400 ms). Open `01-ready.png` ("No Feeds Yet", the button), `05-confirm.png` (the confirmation over the first-run screen), `11-add-dialog.png` (address prefilled, Theme "None", Folder "Feeds"), `14-add-refusal.png` (the red sentence, no Show Log); `rm -rf target-shots` if you ran by hand. Then the full gate (B6d ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/AddFeedDialog.qml apps/linux/qml/OpmlDialogs.qml apps/linux/qml/FirstRun.qml apps/linux/qml/Main.qml apps/linux/tests/library.rs
git commit -m "ressac-kde: Add a Feed dialog, OPML drop/preview/import/export, No Feeds Yet, the add note

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B6e: Settings window and drag and drop on screen

### Task 7 (B6e): `SettingsWindow.qml`, feed and entry drags, library walk (part 2)

**Files:**
- Create: `apps/linux/qml/SettingsWindow.qml`
- Modify: `apps/linux/qml/SourcesPane.qml`, `apps/linux/qml/EntriesPane.qml`, `apps/linux/qml/ReadingPage.qml`, `apps/linux/qml/Main.qml`
- Test (not counted): `apps/linux/tests/library.rs` (more lines in the same test)

**Interfaces:**
- Consumes: Tasks 4–6.
- Produces: `SettingsWindow.qml`, `dragFeed`, `Main.qml`'s `openSettings()` (header).

`SettingsWindow.qml` (root `QQC2.ApplicationWindow { id: settings; title: "Settings"; width: 560; minimumWidth: 540; height: 460; visible: false }`, inventory §7.3):
- `header: QQC2.TabBar { id: tabs }` with `QQC2.TabButton { text: "Reading" }` and `QQC2.TabButton { text: "Data" }`; a `StackLayout { currentIndex: tabs.currentIndex }` of two `Kirigami.FormLayout`s (margins `Kirigami.Units.largeSpacing`).
- Reading: "Font:" `QQC2.ComboBox { model: AppSettings.fontTitles; currentIndex: AppSettings.fontValues.indexOf(AppSettings.readerFont); onActivated: (i) => AppSettings.readerFont = AppSettings.fontValues[i] }` whose `delegate` is a `QQC2.ItemDelegate` with `font.family: AppSettings.fontFamilies[index] || Kirigami.Theme.defaultFont.family`; "Text size:" a `RowLayout` of `Kirigami.Icon { source: "format-font-size-less" }`, `QQC2.Slider { from: 12; to: 28; stepSize: 1; snapMode: QQC2.Slider.SnapAlways; value: AppSettings.readerSize; onMoved: AppSettings.readerSize = value }`, `Kirigami.Icon { source: "format-font-size-more" }`, `QQC2.Label { text: Math.round(AppSettings.readerSize) + " pt"; font.features: { "tnum": 1 }; color: Kirigami.Theme.disabledTextColor }`; "Line width:" checkable `QQC2.ToolButton`s from `AppSettings.widthTitles` in a `QQC2.ButtonGroup`, checked when `AppSettings.widthValues[i] === AppSettings.readerWidth`, `onClicked: AppSettings.readerWidth = AppSettings.widthValues[i]`; "Preview:" `QQC2.Label { text: AppSettings.previewText; font.family: <the chosen family or the default>; font.pixelSize: Math.round(AppSettings.readerSize); wrapMode: Text.Wrap; maximumLineCount: 3; Layout.fillWidth: true }`; then `QQC2.Label { text: AppSettings.sizeHint; font: Kirigami.Theme.smallFont; color: Kirigami.Theme.disabledTextColor; wrapMode: Text.Wrap }`.
- Data: "Database:" a `ColumnLayout` of `QQC2.TextField { readOnly: true; text: AppSettings.resolvedDatabasePath; QQC2.ToolTip.text: text }`, a `RowLayout` of `QQC2.Button { text: "Choose…"; onClicked: chooseDb.open() }`, `QQC2.Button { text: "New…"; onClicked: { newDb.currentFolder = <folder of the resolved path>; newDb.selectedFile = newDb.currentFolder + "/" + AppSettings.newName; newDb.open() } }`, `QQC2.Button { text: "Use Default"; enabled: AppSettings.databasePath !== ""; onClicked: AppSettings.useDefaultLocation() }`; `AppSettings.databaseNote` (small, secondary); `AppSettings.ressacDbNote` (`visible: AppSettings.ressacDbSet`); the refusal (`Kirigami.Icon { source: "dialog-warning" }` + `QQC2.Label { text: AppSettings.databaseRefusal; color: Kirigami.Theme.negativeTextColor }`, `visible: AppSettings.databaseRefusal !== ""`); and, when `AppSettings.needsRelaunch`, `QQC2.Label { text: AppSettings.relaunchNote }` + `QQC2.Button { text: AppSettings.relaunchNow; onClicked: settings.relaunchNow() }`. "Logs:" `QQC2.Button { text: "Show Logs"; onClicked: settings.showLogs() }`.
- `FileDialog { id: chooseDb; title: AppSettings.chooseTitle; nameFilters: [AppSettings.sqliteFilter]; onAccepted: AppSettings.chooseDatabase(String(selectedFile)) }` and `FileDialog { id: newDb; title: AppSettings.newTitle; fileMode: FileDialog.SaveFile; defaultSuffix: "sqlite"; nameFilters: [AppSettings.sqliteFilter]; onAccepted: AppSettings.chooseDatabase(String(selectedFile)) }`.
- `function showPage(name) { tabs.currentIndex = name === "data" ? 1 : 0 }`; `function relaunchNow() { if (Recette.active) { console.warn("settings: relaunch"); return } if (AppSettings.relaunch()) Qt.quit() }`; `function showLogs() { root.openExternally(Library.logsUrl()) }` (`root` = `Main.qml`'s window, reached through a `property var chrome` set by `Main.qml`).

`SourcesPane.qml` (deviation 5; validated finding (a)):
- In the delegate, for feed rows: `Item { id: proxy; readonly property string payload: String(delegate.model.feedId); width: 1; height: 1; Drag.dragType: Drag.Internal; Drag.keys: [Library.feedMime]; Drag.supportedActions: Qt.MoveAction; Drag.proposedAction: Qt.MoveAction }` holding a `QQC2.Label` chip with the feed title (`visible: proxy.Drag.active`, `Kirigami.Theme.backgroundColor` capsule); `MouseArea { anchors.fill: parent; enabled: delegate.model.kind === "feed"; acceptedButtons: Qt.LeftButton; preventStealing: true; drag.target: proxy; drag.threshold: Kirigami.Units.gridUnit / 2; onPressed: (mouse) => { proxy.x = mouse.x; proxy.y = mouse.y }; drag.onActiveChanged: if (drag.active) proxy.Drag.active = true; onReleased: { if (proxy.Drag.active) proxy.Drag.drop(); proxy.x = 0; proxy.y = 0 }; onClicked: SourcesModel.select(delegate.index) }` (right clicks still reach the context menu's `TapHandler`).
- For folder rows and the `heading:Feeds` row: `DropArea { id: dropArea; anchors.fill: parent; enabled: delegate.model.kind === "folder" || delegate.model.key === "heading:Feeds"; keys: [Library.feedMime]; onDropped: (drop) => { const payload = drop.source && drop.source.payload !== undefined ? drop.source.payload : drop.getDataAsString(Library.feedMime); if (SourcesModel.dropFeed(payload, delegate.model.key)) drop.accept(Qt.MoveAction) } }`; the row highlighted (`Kirigami.Theme.hoverColor` background) while `dropArea.containsDrag`.
- `readonly property alias dragProxy: proxy` and `readonly property alias dropArea: dropArea` on the delegate; `function dragFeed(feedKey, targetKey)` (recette; the drop path of a pointer drag): `const from = list.itemAtIndex(SourcesModel.rowOfKey(feedKey)), to = list.itemAtIndex(SourcesModel.rowOfKey(targetKey)); if (!from || !to) return false; const proxy = from.dragProxy, area = to.dropArea; proxy.Drag.active = true; const p = proxy.parent.mapFromItem(area, area.width / 2, area.height / 2); proxy.x = p.x; proxy.y = p.y; const action = proxy.Drag.drop(); proxy.x = 0; proxy.y = 0; return action === Qt.MoveAction`.

`EntriesPane.qml`: entry rows with a web link drag out (automatic drag, validated finding (c)): on the delegate `Drag.dragType: Drag.Automatic; Drag.supportedActions: Qt.CopyAction; Drag.mimeData: delegate.model.link !== "" ? Library.entryDragData(delegate.model.link, delegate.model.title) : ({})`; `Item { id: dragAnchor }` and `MouseArea { anchors.fill: parent; enabled: delegate.model.link !== ""; acceptedButtons: Qt.LeftButton; preventStealing: true; drag.target: dragAnchor; drag.threshold: Kirigami.Units.gridUnit / 2; drag.onActiveChanged: if (drag.active) delegate.Drag.active = true; onReleased: dragAnchor.x = dragAnchor.y = 0; onClicked: EntriesModel.select(delegate.index) }`.

`ReadingPage.qml`: `function dragFeed(feedKey, targetKey) { return sourcesPane.dragFeed(feedKey, targetKey) }`.

`Main.qml`: `SettingsWindow { id: settingsWindow; chrome: root }`; `function openSettings() { settingsWindow.show(); settingsWindow.raise(); settingsWindow.requestActivate() }`; `Binding { target: mainMenu.settings; property: "enabled"; value: true }` and `Connections { target: mainMenu.settings; function onTriggered() { root.openSettings() } }`; `librarySteps` continues:

```qml
    { name: "20-drag", enter: function () { SourcesModel.toggleExpanded(SourcesModel.rowOfKey("folder:1")); SourcesModel.selectKey("feed:2") } },
    { name: "21-drag-root", enter: function () { console.warn("walk: 21-drag moved=" + reading.item.dragFeed("feed:2", "heading:Feeds")
        + " ignored=" + reading.item.dragFeed("feed:3", "heading:Feeds") + " own=" + reading.item.dragFeed("feed:1", "folder:1")
        + " notFeed=" + reading.item.dragFeed("folder:1", "heading:Feeds")) } },
    { name: "22-wait", enter: function () {} },
    { name: "23-dragged", enter: function () { console.warn("walk: 23-dragged selected=" + SourcesModel.selectedKey)
        console.warn("walk: 23-drag-in moved=" + reading.item.dragFeed("feed:3", "folder:1")) } },
    { name: "24-wait", enter: function () {} },
    { name: "25-entry-drag", enter: function () { const data = Library.entryDragData("https://example.org/a", "A title")
        console.warn("walk: 25-entry-drag plain=" + data["text/plain"] + " moz=" + JSON.stringify(data["text/x-moz-url"])
            + " none=" + Object.keys(Library.entryDragData("javascript:alert(1)", "x")).length) } },
    { name: "26-settings", enter: function () { root.openSettings()
        console.warn("walk: 26-settings fonts=" + AppSettings.fontTitles.join("|") + " widths=" + AppSettings.widthTitles.join("|")
            + " registered=" + ["Newsreader", "Atkinson Hyperlegible Next"].every(f => Qt.fontFamilies().includes(f))) } },
    { name: "27-data", enter: function () { settingsWindow.showPage("data")
        console.warn("walk: 27-data path=" + AppSettings.resolvedDatabasePath + " note=" + AppSettings.databaseNote + " ressacDb=" + AppSettings.ressacDbSet) } },
    { name: "28-junk", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
        AppSettings.chooseDatabase(home + "/junk.sqlite")
        console.warn("walk: 28-junk refusal=" + AppSettings.databaseRefusal + " relaunch=" + AppSettings.needsRelaunch) } },
    { name: "29-new", enter: function () { const home = String(StandardPaths.writableLocation(StandardPaths.HomeLocation))
        AppSettings.chooseDatabase(home + "/new.sqlite")
        console.warn("walk: 29-new refusal=" + AppSettings.databaseRefusal + " relaunch=" + AppSettings.needsRelaunch
            + " stored=" + AppSettings.databasePath.endsWith("/new.sqlite") + " note=" + AppSettings.databaseNote)
        settingsWindow.relaunchNow() } },
    { name: "30-default", enter: function () { AppSettings.useDefaultLocation()
        console.warn("walk: 30-default stored=" + AppSettings.databasePath + " note=" + AppSettings.databaseNote); settingsWindow.showLogs() } },
    { name: "31-back", enter: function () { settingsWindow.close(); root.requestActivate() } },
```

- [ ] **Step 1: Write the failing assertions** — append to the test of Task 6 (before the DB checks):

```rust
    for line in [
        "walk: 21-drag moved=true ignored=false own=false notFeed=false",
        "walk: 23-dragged selected=feed:2",
        "walk: 23-drag-in moved=true",
        "walk: 25-entry-drag plain=https://example.org/a moz=\"https://example.org/a\\nA title\" none=0",
        "walk: 26-settings fonts=System Sans|System Serif|Newsreader|Atkinson Hyperlegible Next widths=Narrow|Medium|Wide|40 columns registered=true",
        "note=Shared with the ressac command in Terminal. ressacDb=true",
        "walk: 28-junk refusal=This file is not a usable Ressac database. relaunch=false",
        "walk: 29-new refusal= relaunch=true stored=true note=In Terminal, pass --db with this path to use the same database.",
        "settings: relaunch",
        "walk: 30-default stored= note=Shared with the ressac command in Terminal.",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    let data_line = err.lines().find(|l| l.starts_with("walk: 27-data path=")).unwrap();
    assert!(data_line.contains(&format!("path={}", run.home.join("data/ressac/ressac.sqlite").display())), "{data_line}");
    assert!(err.contains(&format!("open: file://{}", run.home.join("state/ressac/logs").display())), "{err}");
    assert!(run.home.join("state/ressac/logs").is_dir());
    assert_drawn_min(&run.shots.join("26-settings.png"), 540, 400);
    assert_drawn_min(&run.shots.join("28-junk.png"), 540, 400);
    let conf = std::fs::read_to_string(run.home.join("config/ressac/ressac-kde.conf")).unwrap_or_default();
    assert!(!conf.contains("databasePath"), "Use Default removed the key: {conf}");
```

and after the folder check:

```rust
    let placed: Vec<(i64, Option<i64>)> = conn.prepare("SELECT id, folder_id FROM feed ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(placed, [(1, Some(1)), (2, None), (3, Some(1))], "Beta to the top level, Gamma into Walk");
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test library 2>&1 | grep -E "panicked|test result"'` → FAIL (`walk: 21-drag … missing`).
- [ ] **Step 3: Implement** the five QML files.
- [ ] **Step 4: Verify and look** — the same command → `ok.`; then every suite (`cargo test --locked -p ressac-kde`) → every `ok.` (B2's `reading` walk still selects by click: feed rows now take clicks through their `MouseArea`). Open `21-drag-root.png`, `23-dragged.png` (Beta at the top level, Gamma under Walk next), `26-settings.png` (the Reading tab: the font menu in its face, the slider "17 pt", the width buttons, the preview), `29-new.png` (the Data tab: path, notes, "Ressac opens the new database after a relaunch." and Relaunch Now). Add to the ledger's B8e checklist (VM checkpoint 2) the gestures offscreen cannot do: drag a feed onto a folder and onto "Feeds" with the mouse; drag an entry into Firefox's address bar and into Dolphin; drop an `.opml` from Dolphin on the window (dashed outline); Ctrl+N with a feed address copied in Firefox; the portal file dialogs (Ctrl+O, Ctrl+Shift+E with "Ressac.opml" prefilled, Settings ▸ Data Choose… / New… with "ressac.sqlite"). Then the full gate (B6e and B6 end here). Ledger rows B6a–B6e: commits, test counts, deviations 1–12.
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/SettingsWindow.qml apps/linux/qml/SourcesPane.qml apps/linux/qml/EntriesPane.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/Main.qml apps/linux/tests/library.rs
git commit -m "ressac-kde: Settings window (Reading, Data), feed drag onto folders, entry drag-out

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
