# Phase B2: the data layer (B2a, B2b, B2c, B2d)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE window shows the Mac app's sources and entries panes over live data: the sidebar (sections, folder tree, badges, status dots), entry lists paged by the Mac's floor rule with retained rows, unread filter and search, optimistic read/star/mark-all writes reverted with the Mac's banner sentences, one long run at a time with its progress pill and banners, and reloads on every own write, at most once a second during a run, at the end of every run, and when `PRAGMA data_version` moves (polled every 3 s while a window is active).

**Runs after:** B1d (`ressac-ui` with `settings` and `launch`; `ressac-kde` lib + bin, `Launcher`/`AppSettings`/`Recette`, `Session` and `with_session`, the `--screenshot` walk). Master plan `docs/plans/2026-10-23-linux.md` (rows B2a–B2c; Interfaces; Global Constraints: Runs, Language, Time, Gates; Review Focus 3). Spec §7.1 (threads, run rules, live data, optimistic writes), §7.3 (Native reading: sources and entries panes; the reader is B3). Inventory §1.3, §2.1–§2.5, §2.8, §8.4, §8.5 (strings and numbers binding), §10.1 (AppStateTests, DataTests, RunCoordinatorTests, FixWaveTests, JournalListTests' progress texts). Audit §3 (core read API, threading). B0 (`check_subset`, `clock::today_start`, `EventSink: Send + Sync`), B1 (build.rs discovery, bridge conventions, test layout).

**Architecture:** every rule is Rust in `ressac-ui`, tested on the Mac: `entries` (the Mac's floor paging, retained rows, filter, search, row texts, optimistic local edits), `sources` (selection keys, flattened sidebar with collapsible folders), `live` (invalidation rules, row diffs by id), `worker` (new: the job threads, Qt-free) and `runs` (the RunCoordinator, requests, banners, direct writes, the fetch job). The core gains the Mac's floor queries (`Store::list_floor`, `Store::list_rows`) and the sidebar's feed facts. `ressac-kde` adds four QObjects that only translate: `SourcesModel` and `EntriesModel` (QML singletons, `QAbstractListModel`, rows applied as `beginInsertRows`/`beginRemoveRows`/`dataChanged` from `live::plan_rows`, never a reset), `Runs` and `Live` (singletons); worker callbacks come back through `qt_thread().queue`. QML: a reading page with toolbar, banner and a three-pane `SplitView`.

**Tech Stack:** Rust 2024 / 1.99; `ressac-core`, `chrono`, `tokio` (current-thread runtimes), `rusqlite` (dev, raw fixture writes); cxx-qt / cxx-qt-lib 0.10.0; Qt 6.11.2 QtQuick Controls `SplitView`; Kirigami 6.31 (`InlineMessage`, `SearchField`, `PlaceholderMessage`); Breeze icons; Fedora 44 container.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers) and `docs/specs/linux/portability-audit.md` §3.

## Deviations from the master plan

1. **Four sub-phases, not three** (5-file cap). **B2a** core lists + `entries` (5 files), **B2b** `sources`, `live`, `worker`, `runs` (5), **B2c** the Rust bridges (5), **B2d** the QML (4). `sources` moves from B2a to B2b (B2a's fifth slot is `store/mod.rs`, which re-exports the new core types); the optimistic local edits are written with `entries` in B2a, their execution and banners in B2b. Names unchanged; the ledger gets a B2d row.
2. **The four direct writes run on the short-run worker thread (its own connection), not on the GUI thread** (spec §7.1 says the GUI thread owns them). Review Focus 3 requires that a busy wait never freezes the window; the GUI thread keeps one `Store` for reads only. Validated on the Mac (throwaway, scratch copy of HEAD): while a second connection holds `BEGIN IMMEDIATE`, GUI-connection `sources` + `entries` take 2 ms; the worker's `set_read` waits 1.56 s when the lock is released after 1.5 s, and fails after 5.31 s with `is_lock_contention() == true` when held 6.5 s.
3. **New module `ressac_ui::worker`** (the master plan puts "run worker threads" in B2c): the threads, each job's own `Ressac`, the current-thread runtime and the FIFO are Qt-free, so the run rules and Review Focus 3 are tested on the Mac. The bridges only wrap the done-callbacks with `qt_thread().queue`.
4. **Core additions** (B2a Task 1): `ListQuery`, `Store::list_floor`, `Store::list_rows` (`ListedEntry`, `ListRows`, `LIST_SNIPPET_CHARS`), `Store::feed_facts` (`FeedFacts`). The core's `entries` pages with a strict cursor and the TUI's reload replaces the list with page one; the Mac loads every row at or above a floor, so new entries never push loaded rows out (inventory §2.4). List rows also need the Mac's 1200-character snippet (excerpts) and the sidebar needs status, full-text preference and site per feed. The TUI's API is untouched.
5. **Nothing moved from the terminal reader.** `tui/reader/state.rs` (4274 lines) is terminal-bound; its `sidebar.rs` has no "Smart Lists" heading, no collapsible folders, no status dots and a TUI-only `GardenView`; its reload has no floor. Its patterns are kept: a generation/request number that drops late answers, the pinned row (here: retained rows), `jobs.rs`'s thread-per-job with its own connection and `catch_unwind`, `ProgressSink`, the "The job stopped unexpectedly." sentence.
6. **`RunRequest` lists every request of inventory §8.2 now** (kind, banner rule, refusal classes copied from the CLI's `decision_error`, `edition_error`, `refusal_error`, `add_error`); only the fetch job is written in B2, later phases add theirs. A failure's text is the core error's `Display` (no `error: ` prefix, no CLI context); a refusal's is the CLI's sentence.
7. **Left to later phases:** 1 s auto-read, `j`/`k`/Space, menus (B3c); the reader pane (B3, an empty pane here); the Journal shows as a plain rank-ordered list until B4; garden sources show an empty content pane until B5b, and the feed menu's **Show in Garden** item comes with B5b; feed drag and drop, the "Feeds" header drop (B6b).
8. **`ressac_ui` re-exports `chrono`** (`pub use chrono;`) so the bridges read the clock (`chrono::Local::now()`) without a sixth B2c file (`apps/linux/Cargo.toml`). No crate enters the workspace (`chrono`, `tokio`, `rusqlite` are pinned already), so no `notes.md` line.
9. **Relative dates** approximate Foundation's named relative format with fixed spans (minute 60 s, hour, day, week 7 d, month 30 d, year 365 d); excerpts cut at 280 `char`s (Swift counts grapheme clusters).

## Validated in a throwaway container (`ressac-b2-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A cxx-qt 0.10 spike (`QAbstractListModel` QML singleton, Kirigami window, `QQC2.SplitView`), offscreen, all checks passed:
- rows applied from `plan_rows` (the algorithm below; 2000 pseudo-random reloads rebuild the list exactly): two rows inserted on top, one removed, one changed → `rowsInserted` ×1, `rowsRemoved` ×1, `dataChanged` ×1, `modelReset` ×0, the changed row's delegate redrawn, and `ListView.currentIndex` follows the selected id (row 4 → 5); a row moved to the top keeps the selection; the selected row removed → `selectedRow` −1;
- selection both ways: `ListView.currentIndex = n` reaches the model through `onCurrentIndexChanged`, the model's `selectedRowChanged` reaches the view through `Connections` (no binding to break);
- `qt_thread().queue` from a worker thread runs the closure on the GUI thread; from the GUI thread itself it runs on a later turn (the Mac's "deferred one turn"); `CxxQtThread<T>` is `Send + Sync + Clone`;
- a `Timer { interval: 3000 }` ticks twice in 7.2 s;
- `SplitView` with 180/220/320, 260/320/480 and a filling pane: widths 220, 320, 552 in a 1100-wide window. **Gotcha:** with `import QtQuick.Controls as QQC2` the attached properties are `QQC2.SplitView.minimumWidth` (unqualified: "Non-existent attached object").
- cxx-qt's build script parses the whole bridge file: `match qobject::Role { repr: role } {` is a parse error; write `let role = qobject::Role { repr: role }; match role {`.
- Breeze icon names present in `breeze-icon-theme` 6.31 (used below): `view-pim-news`, `mail-unread`, `brightness-high`, `starred-symbolic`, `folder`, `flash`, `view-calendar-week`, `weather-clear-night`, `star-shape`, `system-suspend-symbolic`, `games-hint`, `format-list-unordered`, `box`, `view-refresh`, `window-close`.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-core/src/store/library.rs` | B2a T1 | `ListQuery`, `list_floor`, `list_rows`, `feed_facts` (+ tests in its `mod tests`) |
| `crates/ressac-core/src/store/mod.rs` | B2a T1 | re-export the new types |
| `crates/ressac-ui/Cargo.toml` | B2a T2 | `chrono`, `tokio`; dev `rusqlite` |
| `crates/ressac-ui/src/lib.rs` | B2a T2, B2b T3–T6 | `pub mod entries; live; runs; sources; worker;`, `pub use chrono;`, `#[cfg(test)] mod test_db;` |
| `crates/ressac-ui/src/entries.rs` (new) | B2a T2 | row texts, `EntryList` (floor, retained rows, filter, search, schedule, local edits) |
| `crates/ressac-ui/src/test_db.rs` (new, `cfg(test)`, not counted) | B2a T2 | the Mac's `TestDatabase` |
| `crates/ressac-ui/src/sources.rs` (new) | B2b T3 | `Selection`, `GardenScope`, `SidebarTree`, `SidebarRow`, icons, status dots |
| `crates/ressac-ui/src/live.rs` (new) | B2b T4 | `Invalidation`, `should_poll`, `RowOp`, `plan_rows`, `apply_rows` |
| `crates/ressac-ui/src/worker.rs` (new) | B2b T5 | `Worker`, `JobContext`, `JobFailure` |
| `crates/ressac-ui/src/runs.rs` (new) | B2b T6 | `RunRequest`, `RunCoordinator`, progress texts, banners, `DirectWrite`, `fetch_options` (+ Review Focus 3 tests) |
| `apps/linux/src/lib.rs` | B2c T7, T8 | `mod entries; mod runs; mod sources;` |
| `apps/linux/src/launcher.rs` | B2c T7 | `Session` gains its `Worker`; the `#[expect(dead_code)]` goes |
| `apps/linux/src/runs.rs` (new) | B2c T7 | QML singletons `Runs`, `Live` |
| `apps/linux/src/sources.rs` (new) | B2c T8 | QML singleton list model `SourcesModel` |
| `apps/linux/src/entries.rs` (new) | B2c T8 | QML singleton list model `EntriesModel` |
| `apps/linux/qml/Main.qml` | B2d T9 | Ready → `ReadingPage`; walk steps |
| `apps/linux/qml/ReadingPage.qml` (new) | B2d T9 | toolbar, banner, `SplitView`, poll and minute timers, reload wiring |
| `apps/linux/qml/SourcesPane.qml` (new) | B2d T9 | sidebar list, folder disclosure, badges, dots, feed context menu |
| `apps/linux/qml/EntriesPane.qml` (new) | B2d T9 | entry rows, empty text, page trigger, selection |
| `apps/linux/tests/common/mod.rs`, `apps/linux/tests/reading.rs` (not counted) | B2d T9 | B1's `Run` helpers moved to `common`; the reading walk |

## Interfaces produced (B3 onwards rely on these names)

**Core** (`ressac_core::store`, Task 1):

```rust
pub const LIST_SNIPPET_CHARS: usize = 1200;
/// A list as the app shows it. With `search` (words, see `fts_query`) the `filter` is ignored, as in the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListQuery { pub source: Source, pub filter: ListFilter, pub search: Option<String> }
#[derive(Debug, Clone, PartialEq)]
pub struct ListedEntry { pub row: EntryRow, pub snippet: Option<String>, pub rank: Option<u32> }
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListRows { pub rows: Vec<ListedEntry>, pub total: usize, pub unread: usize }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedFacts { pub status: Option<Status>, pub prefer_full_text: bool, pub html_url: Option<String> }
impl Store {
    pub fn list_floor(&self, list: &ListQuery, below: Option<&ListCursor>, rows: usize, keeping: Option<i64>, today_start: DateTime<Utc>) -> Result<Option<ListCursor>>;
    pub fn list_rows(&self, list: &ListQuery, floor: Option<&ListCursor>, today_start: DateTime<Utc>) -> Result<ListRows>;
    pub fn feed_facts(&self) -> Result<HashMap<FeedId, FeedFacts>>;
}
```

**`ressac_ui::entries`** (Task 2):

```rust
pub const PAGE_SIZE: usize = ressac_core::store::PAGE_SIZE;   // 500
pub const LOAD_MORE_MARGIN: usize = 20;
pub const EXCERPT_CHARS: usize = 280;
pub const UNTITLED: &str = "Untitled";
pub const NO_RESULTS: &str = "No results";
pub const NO_UNREAD: &str = "No unread entries";
pub const NO_ENTRIES: &str = "No entries";
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub entry_id: i64, pub feed_id: FeedId, pub feed_title: String, pub title: String, pub excerpt: String,
    pub published_at: DateTime<Utc>, pub is_read: bool, pub is_starred: bool, pub has_fulltext: bool,
    pub link: Option<String>, pub rank: Option<u32>,
}
impl ListRow { pub fn new(listed: &ListedEntry) -> ListRow; }
pub fn display_title(title: Option<&str>) -> String;
pub fn excerpt(html: &str, limit: usize) -> String;
pub fn web_link(url: Option<&str>) -> Option<String>;
pub fn relative_date(at: DateTime<Utc>, now: DateTime<Utc>) -> String;
/// An optimistic change in flight: it stays applied over every reload until `settle` (the write
/// landed) or `revert` (it failed), so a poll during a 5 s busy wait never flickers the row back.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalEdit { /* private: id, the ids and the flag values to put back */ }
pub struct EntryList { /* private */ }
impl EntryList {
    pub fn new(query: ListQuery) -> EntryList;               // empty, generation 0, page PAGE_SIZE
    pub fn with_page_size(self, rows: usize) -> EntryList;   // tests
    pub fn query(&self) -> &ListQuery;
    pub fn rows(&self) -> &[ListRow];
    pub fn index_of(&self, entry_id: i64) -> Option<usize>;
    pub fn total(&self) -> usize;  pub fn unread(&self) -> usize;  pub fn has_more(&self) -> bool;
    pub fn generation(&self) -> u64;
    pub fn restart(&mut self, store: &Store, keeping: Option<i64>, today_start: DateTime<Utc>) -> ressac_core::Result<()>;
    pub fn change(&mut self, query: ListQuery, store: &Store, keeping: Option<i64>, today_start: DateTime<Utc>) -> ressac_core::Result<()>;
    pub fn reload(&mut self, store: &Store, today_start: DateTime<Utc>) -> ressac_core::Result<bool>; // true: restart on the next turn
    pub fn restart_due(&self, generation: u64) -> bool;
    pub fn load_more(&mut self, store: &Store, today_start: DateTime<Utc>) -> ressac_core::Result<()>;
    pub fn row_appeared(&mut self, entry_id: i64) -> Option<u64>;  // Some(generation): load a page on the next turn
    pub fn scheduled_page_due(&mut self, generation: u64) -> bool;
    pub fn can_mark_all_read(&self) -> bool;
    pub fn empty_text(&self) -> Option<&'static str>;
    pub fn set_read_local(&mut self, ids: &[i64], read: bool) -> LocalEdit;
    pub fn set_starred_local(&mut self, id: i64, starred: bool) -> LocalEdit;
    pub fn mark_all_read_local(&mut self) -> LocalEdit;
    pub fn settle(&mut self, edit: LocalEdit);   // the write landed: the overlay goes, the database agrees
    pub fn revert(&mut self, edit: LocalEdit);   // the write failed: the flags it changed come back
}
```

**`ressac_ui::sources`** (Task 3):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GardenScope { NeedsTriage, All, Cemetery }        // ALL; title(): "Needs Triage", "All Feeds", "Cemetery"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selection { Source(Source), Garden(GardenScope) }
impl Default for Selection { /* Source(Source::Unread) */ }
impl Selection {
    pub fn key(&self) -> String;                    // "journal" "unread" "today" "starred" "folder:<id>" "feed:<id>"
    pub fn parse_key(key: &str) -> Option<Selection>; // "rhythm:<group>" "garden:needs-triage|all|cemetery"
    pub fn source(&self) -> Option<Source>;
    pub fn is_garden(&self) -> bool;
}
pub const HEADINGS: [&str; 4] = ["Smart Lists", "Feeds", "Rhythm", "Garden"];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind { Heading, Smart, Folder, Feed, Rhythm, Garden }  // as_str: "heading" "smart" "folder" "feed" "rhythm" "garden"
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarRow {
    pub kind: RowKind, pub key: String, pub title: String, pub icon: &'static str, pub badge: usize,
    pub depth: usize, pub expandable: bool, pub expanded: bool, pub status: Option<Status>,
    pub prefer_full_text: bool, pub site: Option<String>, pub page: Option<u16>,
}
impl SidebarRow { pub fn selection(&self) -> Option<Selection>; pub fn dot(&self) -> Option<&'static str>; }
pub fn rhythm_title(group: CadenceGroup) -> &'static str;      // "Daily" … "Silent"
pub fn status_color(status: Option<Status>) -> &'static str;   // the one status → colour map (garden reuses it)
pub fn sidebar_dot(status: Option<Status>) -> Option<&'static str>; // sick statuses only
pub fn index_of(rows: &[SidebarRow], selection: &Selection) -> Option<usize>;
#[derive(Debug, Clone, Default)]
pub struct SidebarTree { /* snapshot, facts, expanded folders */ }
impl SidebarTree {
    pub fn load(&mut self, store: &Store, today_start: DateTime<Utc>) -> ressac_core::Result<()>;
    pub fn set(&mut self, snapshot: SourcesSnapshot, facts: HashMap<FeedId, FeedFacts>);
    pub fn rows(&self) -> Vec<SidebarRow>;
    pub fn toggle(&mut self, folder: FolderId) -> bool;          // new state
    pub fn reveal(&mut self, selection: &Selection);             // opens the folders above it
    pub fn set_prefer_full_text_local(&mut self, feed: FeedId, on: bool) -> bool; // the previous value
}
```

**`ressac_ui::live`** (Task 4):

```rust
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);
pub const PROGRESS_RELOAD_GAP: Duration = Duration::from_secs(1);
pub fn should_poll(window_active: bool, chart_on_screen: bool) -> bool;
#[derive(Debug, Clone, Default)]
pub struct Invalidation { /* last data_version, last run notice, last today_start */ }
impl Invalidation {
    pub fn version_seen(&mut self, version: i64) -> bool;  // first call: baseline, false
    pub fn activated(&mut self, version: i64) -> bool;     // window became active: new baseline, always true
    pub fn run_progress(&mut self, now: Instant) -> bool;  // at most once per PROGRESS_RELOAD_GAP
    pub fn run_finished(&mut self, now: Instant) -> bool;  // always true, counts as a notice
    pub fn day_seen(&mut self, today_start: DateTime<Utc>) -> bool; // first call: baseline, false
}
#[derive(Debug, Clone, PartialEq)]
pub enum RowOp<T> { Remove { first: usize, last: usize }, Insert { at: usize, rows: Vec<T> }, Change { first: usize, rows: Vec<T> } }
pub fn plan_rows<T: Clone + PartialEq, K: Eq + Hash>(old: &[T], new: &[T], key: impl Fn(&T) -> K) -> Vec<RowOp<T>>;
pub fn apply_rows<T: Clone>(rows: &mut Vec<T>, ops: &[RowOp<T>]);  // the reference semantics the bridges follow
```

**`ressac_ui::worker`** (Task 5):

```rust
#[derive(Debug)]
pub enum JobFailure { Core(ressac_core::Error), Panicked }
impl JobFailure { pub fn is_busy(&self) -> bool; pub fn message(&self) -> String; } // Panicked: "The job stopped unexpectedly."
pub type JobResult<T> = Result<T, JobFailure>;
pub struct JobContext<'a> { pub app: &'a Ressac, /* runtime */ }
impl JobContext<'_> { pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output; }
pub struct Worker { /* db, short-run channel, stop flag */ }
impl Worker {
    pub fn new(db: &Path) -> Worker;
    pub fn db(&self) -> &Path;
    pub fn short<T: Send + 'static>(&self,
        job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
        done: impl FnOnce(JobResult<T>) + Send + 'static);
    pub fn long<T: Send + 'static>(&self,
        job: impl FnOnce(&JobContext<'_>, &dyn EventSink, Arc<AtomicBool>) -> ressac_core::Result<T> + Send + 'static,
        events: impl Fn(&Event) + Send + Sync + 'static,
        done: impl FnOnce(JobResult<T>) + Send + 'static);
    pub fn stop(&self);
}
impl Drop for Worker { /* stop(); the short thread ends when its queue closes */ }
```

**`ressac_ui::runs`** (Task 6):

```rust
pub const EDITION_LABEL: &str = "Building this morning's edition…";
pub const FETCHING_FEEDS: &str = "Fetching feeds";
pub const FETCHING_FULL_ARTICLES: &str = "Fetching full articles";
pub const BUSY: &str = "The database is busy (is `ressac` running in a terminal?)";
pub const COULD_NOT_SAVE: &str = "Could not save the change: ";
pub const COULD_NOT_READ: &str = "Could not read the database: ";
pub const PROGRESS_INSET: u32 = 10;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum RunKind { Long, Short }
#[derive(Debug, Clone, PartialEq)]
pub enum RunRequest {
    FetchAll, FetchFeeds(Vec<FeedId>), FetchFolder(FolderId), Import(PathBuf), BuildEdition { rebuild: bool },
    Add { address: String, theme: Option<String>, folder: Option<FolderId> },                     // long
    Export { path: PathBuf, cemetery: bool }, Extract(i64), Decide(FeedId, Decision), Undo, ApplySafe,
    SetCadence(FeedId, Option<CadenceGroup>), SetTheme(FeedId, Option<String>), MoveFeed(FeedId, Option<FolderId>), // short
}
impl RunRequest {
    pub fn refresh(selection: &Selection) -> RunRequest;
    pub fn kind(&self) -> RunKind;
    pub fn shows_failure_banner(&self) -> bool;          // false: Extract, Add
    pub fn is_refusal(&self, error: &ressac_core::Error) -> bool;
    pub fn failure(&self, failure: &JobFailure) -> RunFailure;
}
#[derive(Debug, Clone, PartialEq, Eq)] pub struct RunProgress { pub done: usize, pub total: usize, pub label: String }
impl RunProgress { pub fn fraction(&self) -> Option<f64>; }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Banner { pub message: String, pub show_log: bool }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct RunFailure { pub message: String, pub refusal: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Admission { Start, Refused }
#[derive(Debug, Clone, Default)]
pub struct RunCoordinator { /* long active, progress, banner, banner set during the long run */ }
impl RunCoordinator {
    pub fn admit(&mut self, request: &RunRequest) -> Admission;
    pub fn event(&mut self, request: &RunRequest, event: &Event) -> bool; // true: the event changed the database
    pub fn finish(&mut self, request: &RunRequest, outcome: Result<(), &RunFailure>);
    pub fn long_run_active(&self) -> bool;  pub fn can_refresh(&self) -> bool;
    pub fn progress(&self) -> Option<&RunProgress>;  pub fn banner(&self) -> Option<&Banner>;
    pub fn show_banner(&mut self, banner: Banner);  pub fn dismiss_banner(&mut self);
}
pub fn progress_text(progress: &RunProgress) -> String;
pub fn compact_progress_text(progress: &RunProgress) -> String;
pub fn progress_width(progress: &RunProgress) -> u32;              // 260 edition, 140 otherwise
pub fn network_unreachable(failed: usize, attempted: usize) -> String;
pub fn write_failure(failure: &JobFailure) -> Banner;              // show_log false
pub fn read_failure(error: &ressac_core::Error) -> Banner;         // show_log true
#[derive(Debug, Clone, PartialEq)]
pub enum DirectWrite {
    SetRead { ids: Vec<i64>, read: bool }, SetStarred { id: i64, starred: bool },
    MarkAllRead { source: Source, today_start: DateTime<Utc> }, PreferFullText { feed: FeedId, on: bool },
}
impl DirectWrite { pub fn apply(&self, store: &Store, now: DateTime<Utc>) -> ressac_core::Result<()>; }
pub fn fetch_options(store: &Store, request: &RunRequest) -> ressac_core::Result<RefreshOptions>;
```

**`ressac-kde`** (Tasks 7–9). QML singletons (`#[qml_element] #[qml_singleton]`, `#[auto_cxx_name]`, custom `READ = …` getters over the Rust state, `impl cxx_qt::Threading`):

- `Runs`: properties (all `READ`, `NOTIFY = state_changed`) `longRunActive: bool`, `canRefresh: bool`, `progressVisible: bool`, `progressText: QString`, `progressCompactText: QString`, `progressFraction: f64` (−1 = indeterminate), `progressWidth: i32`, `bannerVisible: bool`, `bannerMessage: QString`, `bannerShowLog: bool`; `logDir: QString` (`CONSTANT`, `Paths::log_dir()`); invokables `refreshAll()`, `refreshSource(key: QString)`, `refreshFeed(feedId: i64)`, `showBanner(message: QString, showLog: bool)`, `dismissBanner()`; signals `stateChanged()`, `databaseChanged()` (throttled by `Invalidation`).
- `Live`: `windowActive: bool` and `chartOnScreen: bool` (`READ`/`WRITE`/`NOTIFY`; B7 writes `chartOnScreen`), `polling: bool` (`READ`, `NOTIFY = polling_changed`), `pollInterval: i32` (`CONSTANT`, 3000); invokables `poll()`, `minuteTick()`; signal `invalidated()`.
- `SourcesModel` (`#[base = QAbstractListModel]`): roles `kind`, `key`, `title`, `icon`, `badge`, `depth`, `expandable`, `expanded`, `statusColor` (`""` = none), `statusText`, `feedId` (0 = none), `preferFullText`, `site`, `selectable`; properties `selectedKey: QString`, `selectedRow: i32`, `isGarden: bool` (`READ`, `NOTIFY = selection_changed`); invokables `reload()`, `select(row)`, `selectKey(key)`, `rowOfKey(key) -> i32`, `toggleExpanded(row)`, `setPreferFullText(feedId: i64, on: bool)`; signals `selectionChanged()`, `banner(message, showLog)`, `wrote()`.
- `EntriesModel` (`#[base = QAbstractListModel]`): roles `entryId` (i64), `feedTitle`, `title`, `excerpt`, `date`, `read`, `starred`, `link` (`""` = none); properties `unreadOnly: bool` and `searchText: QString` (`READ`/`WRITE`/`NOTIFY`), `selectedRow: i32`, `selectedEntryId: i64` (−1 = none) (`NOTIFY = selection_changed`), `active: bool`, `emptyText: QString`, `canMarkAllRead: bool` (`NOTIFY = list_changed`); invokables `showSource(key)`, `reload()`, `rowAppeared(row)`, `select(row)`, `toggleRead(row)`, `toggleStarred(row)`, `markAllRead()`, `markAllReadIn(key)`; signals `selectionChanged()`, `listChanged()`, `banner(message, showLog)`, `wrote()`.
- Crate-private: `crate::launcher::Session { pub db: PathBuf, pub store: Store, pub worker: Worker }`.
- QML: `ReadingPage.qml` (root `Item`) is what `Main.qml` shows when `Launcher.phase === Launcher.Ready`; it exposes `function reloadAll()`. B3 puts the reader in its third pane; B4/B5b replace the content pane for the Journal and the garden.

**Consumed:** `Store::{sources, data_version, set_read, set_starred, mark_all_read, set_prefer_full_text, open_existing}`, `fts_query`, `SourcesSnapshot`, `EntryRow`, `ListCursor`, `PAGE_SIZE` (core); `service::{check_subset, Ressac, RefreshOptions, Event, EventSink, Summary}` (B0a); `clock::today_start` (B0c); `paths::Paths::log_dir` (A1); B1's `with_session`, `Recette`, `walkSteps`, `tests/launch.rs` helpers.

**Decisions this phase pins:**

- **Threads.** The GUI thread owns `Session.store` and only reads. `Session.worker` owns one short-run thread (FIFO, one `Ressac` opened on its first job and kept, a current-thread runtime) and spawns one thread per long run (its own `Ressac::open`, its own runtime). Direct writes are short jobs. Every callback runs on the worker thread; the bridges hand it to Qt with `qt_thread().queue` (an `Err` from `queue` is dropped with `tracing::debug!`).
- **Reload wiring.** One QML function `reloadAll()` (`SourcesModel.reload(); EntriesModel.reload()`) runs on `Live.invalidated` (poll saw another commit, the day changed, the window became active), on `Runs.databaseChanged` (progress at most once a second, every run end), and on `wrote()` from either model (an own write succeeded or was reverted). Reloads diff by key and never reset a model, so selection and scroll survive.
- **Next turn.** A page asked by `rowAppeared` and the restart of a short list that grew loads on the next event-loop turn (`qt_thread().queue` from the GUI thread, generation checked), never inside a delegate's creation.
- **Selection.** By id in Rust (`Selection` for the sidebar, entry id for the list); `selectedRow` is derived after every row change; QML follows it with `Connections { function onSelectionChanged() { list.currentIndex = Model.selectedRow } }` and reports clicks with `select(index)`.
- **Clock.** Bridges pass `chrono::Local::now()` / `today_start(chrono::Local::now())` and `std::time::Instant::now()` into `ressac-ui` at each call.

**Step 0:** not triggered. `library.rs` (1322 lines) and `store/mod.rs` (1149) only gain items; the existing `page_of`, `entries`, `search_entries` are not restructured. `launcher.rs` loses one attribute.

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

From B2b on `cargo test -p ressac-ui` takes about 8 s more (Review Focus 3 waits for the real 5 s busy timeout once and 1.5 s once).

## Review Focus

1. **`ressac fetch` in Konsole holds the write lock while the app marks entries read** (master Review Focus 3) → the window never blocks, the mark shows at once, the write waits up to the 5 s busy timeout on the worker, a timeout reverts the row with "The database is busy (is `ressac` running in a terminal?)". Tests: Task 6 `a_read_mark_waits_on_the_worker_while_a_terminal_writes_and_the_list_never_blocks`, `a_read_mark_still_blocked_after_the_busy_timeout_reverts_with_the_busy_sentence`.
2. **A refresh lands hundreds of entries above the row being read** → loaded rows stay, the selection keeps its entry and the view its place, no model reset. Tests: Task 2 `new_entries_never_push_the_loaded_rows_out`; Task 4 `new_rows_on_top_a_gone_row_and_a_changed_row_are_three_operations`, `any_reload_plans_rows_that_rebuild_it_exactly` (the container spike showed the view keeping its current row through such a plan, with no `modelReset`).
3. **Refresh pressed again (F5, toolbar, a feed's menu) while a fetch runs** → refused, not queued; the triggers are disabled. Tests: Task 6 `long_run_is_refused_while_another_is_active`.
4. **The window left in the background or minimised** → no 3 s wake-ups; coming back reloads once at once. Tests: Task 4 `polling_runs_while_a_window_is_active_or_the_chart_is_on_screen`, `becoming_active_reloads_once_and_takes_a_fresh_baseline`.
5. **Hostile or broken list text** (an unclosed `<a href=`, entities, a 5000-character body, a blank title, a `javascript:` link) → a clean excerpt cut at 280 + "…", "Untitled", no link. Tests: Task 2 `untitled_entries_and_excerpts_read_like_the_mac_list`, `only_web_links_are_links`; Task 1 `list_rows_carry_a_short_snippet_and_the_journal_rank`.

---

## B2a: the Mac's list queries, `ressac_ui::entries`

### Task 1 (B2a): core lists by floor, feed facts

**Files:**
- Modify: `crates/ressac-core/src/store/library.rs` (new items after `search_entries`; tests in its `mod tests`)
- Modify: `crates/ressac-core/src/store/mod.rs:33` (re-export `ListQuery, ListedEntry, ListRows, FeedFacts, LIST_SNIPPET_CHARS`)

**Interfaces:**
- Consumes: `predicate`, `ROW_COLUMNS`, `entry_row`, `EDITION_RANK`, `fts_query` (same file); `Status::parse` (`model`).
- Produces: the core block of the header.

Rules (the Mac's `Queries.floor`, `Queries.page`, `Queries.sidebar`, `Queries.swift:146-164, 353-379`):
- The list's condition is `predicate(source)` plus `AND NOT is_read` under `ListFilter::Unread`; with `search: Some(text)`, `entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?) AND (<predicate>)` and the filter ignored; a search whose `fts_query` is `None` is the empty list (`list_rows` → `ListRows::default()`, `list_floor` → `None`).
- `list_floor`: `None` for `Source::Journal`; else the rows strictly below `below` (`(published_at, entry_id) < (?, ?)`), newest first, `LIMIT 1 OFFSET max(rows, 1) - 1`; no row → `None`; with `keeping: Some(id)`, the result is the smaller (by `(published_at text, id)`) of that floor and `SELECT published_at, id FROM entry WHERE id = ?` (unknown id: the floor unchanged).
- `list_rows`: the rows with `(published_at, entry_id) >= floor` (every row when `None`), ordered `EDITION_RANK, entry_id` for the Journal (`rank` filled from `EDITION_RANK`, else `None`) and `published_at DESC, entry_id DESC` otherwise; extra column `substr(COALESCE(summary_html, content_html), 1, 1200)` as `snippet`; `total` and `unread` from `COUNT(*), COALESCE(SUM(NOT is_read), 0)` over the whole list.
- `feed_facts`: `SELECT f.id, h.status, f.prefer_full_text, f.html_url FROM feed f LEFT JOIN feed_health h ON h.feed_id = f.id WHERE f.state = 'active'`; an unknown status text reads as `None`.

- [ ] **Step 1: Write the failing tests** in `library.rs`'s `mod tests` (they use `library()`, `add_edition`, `today()`, `at`; helper `fn listed_ids(rows: &ListRows) -> Vec<i64>`; `fn query(source: Source) -> ListQuery { ListQuery { source, filter: ListFilter::All, search: None } }`):

```rust
    /// Mac DataTests.entry_pages_stop_at_the_floor_and_count_the_whole_list. Folder 1 lists
    /// entries 1 (10-16), 3 (10-15), 2 (10-14); 5 is a buried feed's.
    #[test]
    fn list_rows_stop_at_the_floor_and_count_the_whole_list() {
        let s = library();
        let infos = query(Source::Folder(1));
        let first = s.list_floor(&infos, None, 2, None, today()).unwrap().expect("a third row remains");
        assert_eq!((first.published_at.as_str(), first.entry_id), ("2026-10-15T09:00:00.000Z", 3));
        let page = s.list_rows(&infos, Some(&first), today()).unwrap();
        assert_eq!(listed_ids(&page), [1, 3]);
        assert_eq!((page.total, page.unread), (3, 2));
        assert_eq!(s.list_floor(&infos, Some(&first), 2, None, today()).unwrap(), None, "one row left: it fits");
        assert_eq!(s.list_floor(&infos, Some(&first), 1, None, today()).unwrap().map(|f| f.entry_id), Some(2));
        assert_eq!(listed_ids(&s.list_rows(&infos, None, today()).unwrap()), [1, 3, 2], "no floor: the whole list");
        assert_eq!(s.list_floor(&infos, None, 1, Some(2), today()).unwrap().map(|f| f.entry_id), Some(2), "kept older entry lowers it");
        assert_eq!(s.list_floor(&infos, None, 2, Some(1), today()).unwrap(), Some(first), "a newer one changes nothing");
        assert_eq!(s.list_floor(&query(Source::Journal), None, 1, None, today()).unwrap(), None, "the Journal loads whole");
    }

    #[test]
    fn list_rows_carry_a_short_snippet_and_the_journal_rank() {
        let s = library();
        s.conn().execute("UPDATE entry SET summary_html = NULL, content_html = ?1 WHERE id = 1", ["x".repeat(5000)]).unwrap();
        let unread = s.list_rows(&query(Source::Unread), None, today()).unwrap();
        let snippet = |id| unread.rows.iter().find(|e| e.row.entry_id == id).unwrap().snippet.clone();
        assert_eq!(snippet(1).map(|s| s.len()), Some(LIST_SNIPPET_CHARS));
        assert_eq!(snippet(4).as_deref(), Some("<p>Rare bird</p>"));
        assert!(unread.rows.iter().all(|e| e.rank.is_none()));
        add_edition(&s, 1, &[(3, "front", 1), (4, "wire", 2), (1, "wire", 3)], &[]);
        let journal = s.list_rows(&query(Source::Journal), None, today()).unwrap();
        assert_eq!(listed_ids(&journal), [3, 4, 1]);
        assert_eq!(journal.rows.iter().map(|e| e.rank).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn a_search_list_ignores_the_unread_filter_and_matches_every_word() {
        let s = library();
        let search = |source, filter, text: &str| ListQuery { source, filter, search: Some(text.to_string()) };
        let old = s.list_rows(&search(Source::Folder(1), ListFilter::Unread, "old news"), None, today()).unwrap();
        assert_eq!((listed_ids(&old), old.total), (vec![2], 1), "a read entry is found under the unread filter");
        assert_eq!(listed_ids(&s.list_rows(&search(Source::Folder(1), ListFilter::All, "tides"), None, today()).unwrap()), [1],
                   "the buried feed's \"Buried tides\" stays out");
        assert_eq!(s.list_rows(&search(Source::Unread, ListFilter::All, "   "), None, today()).unwrap(), ListRows::default());
        assert_eq!(s.list_floor(&search(Source::Unread, ListFilter::All, "   "), None, 1, None, today()).unwrap(), None);
        assert_eq!(s.list_floor(&search(Source::Folder(1), ListFilter::All, "tides"), None, 1, None, today()).unwrap().map(|f| f.entry_id), Some(1));
    }

    #[test]
    fn feed_facts_give_the_sidebar_status_preference_and_site() {
        let s = library();
        s.conn().execute_batch(
            "UPDATE feed_health SET status = 'Dead' WHERE feed_id = 2; DELETE FROM feed_health WHERE feed_id = 3; \
             UPDATE feed SET prefer_full_text = 1, html_url = 'https://f1.example.org/' WHERE id = 1;").unwrap();
        let facts = s.feed_facts().unwrap();
        let mut ids: Vec<FeedId> = facts.keys().copied().collect();
        ids.sort();
        assert_eq!(ids, [1, 2, 3], "active feeds only");
        assert_eq!(facts[&1], FeedFacts { status: Some(Status::Alive), prefer_full_text: true, html_url: Some("https://f1.example.org/".into()) });
        assert_eq!((facts[&2].status, facts[&3].status), (Some(Status::Dead), None));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac-core store::library::tests::list_ store::library::tests::a_search_list store::library::tests::feed_facts`
Expected: compile errors (`cannot find type ListQuery`, `no method named list_floor`).

- [ ] **Step 3: Implement** the four items as the rules above (one private helper builds the condition and its arguments from a `ListQuery`; `list_rows` reuses `entry_row` with the snippet at column 10 and the rank at 11). Re-export in `store/mod.rs`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac-core && cargo clippy -p ressac-core --all-targets -- -D warnings`
Expected: every `test result: ok.` (4 more in the lib); clippy `Finished`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/library.rs crates/ressac-core/src/store/mod.rs
git commit -m "Core: list_floor, list_rows (the app's floor paging, snippets, ranks), feed_facts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B2a): `ressac_ui::entries`

**Files:**
- Modify: `crates/ressac-ui/Cargo.toml` (`[dependencies]` `chrono.workspace = true`, `tokio.workspace = true`; `[dev-dependencies]` `rusqlite.workspace = true`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod entries;`, `pub use chrono;` with the doc line "so the KDE bridges pass the clock in with the same chrono", `#[cfg(test)] mod test_db;`)
- Create: `crates/ressac-ui/src/entries.rs` (tests in its `mod tests`)
- Create (not counted): `crates/ressac-ui/src/test_db.rs`

**Interfaces:**
- Consumes: Task 1 (`ListQuery`, `list_floor`, `list_rows`, `ListedEntry`), `ListCursor`, `model::parse_ts`.
- Produces: `ressac_ui::entries` of the header; `test_db` (crate-private, `cfg(test)`):

```rust
pub const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/contract.sqlite");
pub fn at(s: &str) -> DateTime<Utc>;
pub fn today() -> DateTime<Utc>;                  // at("2026-10-16T00:00:00Z")
pub struct NewEntry<'a> { pub feed: i64, pub title: Option<&'a str>, pub url: Option<&'a str>, pub published: &'a str,
    pub summary: Option<&'a str>, pub content: Option<&'a str>, pub read: bool, pub starred: bool, pub body: Option<&'a str> }
impl Default for NewEntry<'_> { /* feed 1, Some("Entry"), Some("https://example.org/a"), "2026-10-01T08:00:00.000Z", Some("<p>Summary</p>"), None, false, false, None */ }
pub struct TestDb { pub path: PathBuf, /* temp dir, raw rusqlite connection */ }
impl TestDb {
    pub fn new() -> TestDb;   // copy of FIXTURE, then the Mac TestDatabase batch below
    pub fn store(&self) -> Store;                                    // Store::open_existing
    pub fn entry(&self, feed: i64, published: &str, read: bool) -> i64;
    pub fn entry_with(&self, entry: NewEntry<'_>) -> i64;            // + its entry_fts row (rowid, title, '', body or title)
    pub fn days(&self, count: usize, feed: i64, read: impl Fn(usize) -> bool) -> Vec<i64>; // day d at 2026-10-%02dT08:00:00.000Z; ids newest first
    pub fn health(&self, feed: i64, status: &str, group: &str, needs_triage: bool);
    pub fn edition(&self, entries: &[(i64, &str)]) -> i64;           // ranks 1… in order
    pub fn execute(&self, sql: &str);
    pub fn read_at(&self, id: i64) -> Option<String>;
    pub fn starred(&self, id: i64) -> bool;
    /// BEGIN IMMEDIATE + `UPDATE feed SET title = 'Renamed by the terminal' WHERE id = 1` on a third
    /// connection, held `hold` on its own thread, then COMMIT; returns once the lock is held.
    pub fn hold_write_lock(&self, hold: Duration) -> std::thread::JoinHandle<()>;
}
```

`TestDb::new` runs (the Mac's `TestDatabase.init`): `DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition; DELETE FROM entry_fts; DELETE FROM entry_fulltext; DELETE FROM entry; DELETE FROM feed_health; DELETE FROM fetch_log; DELETE FROM garden_decision; UPDATE feed SET state = 'active', merged_into_id = NULL, prefer_full_text = 0, cadence_override = NULL;`. The fixture then holds 7 folders and 11 active feeds: Blogs (1) > potes (2); Biblis (3); Infos (4) > Science (5); Jon (6) > YouTube (7); feed 1 `xkcd.com` and 5 in Blogs, 2–4 in potes, 8–9 in Infos.

Rules (Mac `AppState.swift:403-430, 698-864`):
- `restart(store, keeping, today)` (`listDidChange`): generation + 1; floor = `list_floor(query, None, page_size, keeping)`; on error: floor `None`, floor failed, error returned; then `reload`.
- `change(query, …)`: a different `source` clears the retained ids; then `restart`.
- `reload` (the observation's refetch): `list_rows(query, floor)` → rows `ListRow::new`; merged with the previous rows whose id is retained, not in the fresh rows and admitted by the floor (`(published_at, entry_id) >= (parse_ts(floor.published_at), floor.entry_id)`; an unparsable floor admits), those marked read; sorted by `(rank or u32::MAX, entry_id)` when the query is the Journal, else `(published_at, entry_id)` descending. `total = page.total + merged − fresh`, `unread = page.unread`, `has_more = fresh < page.total`. Returns `true` when the floor is `None`, not failed, the source is not the Journal and the fresh rows exceed the page size; `restart_due(g)` is that condition again with `g == generation`.
- `load_more`: when `has_more` and a floor exists, floor = `list_floor(query, Some(floor), page_size, None)` or the oldest possible (`ListCursor { published_at: "", entry_id: i64::MIN }`); then `reload`.
- `row_appeared(id)`: `Some(generation)` only when no page is scheduled, `has_more`, and `id` is among the last `LOAD_MORE_MARGIN` rows (it then marks a page scheduled); `scheduled_page_due(g)` clears the mark and is `g == generation`.
- `can_mark_all_read`: no search and (an unread row, or `has_more` and `unread > 0`).
- `empty_text`: `None` with rows; `NO_RESULTS` with a search; `NO_UNREAD` for `Source::Unread` or the unread filter; else `NO_ENTRIES`.
- Local edits change the rows only (never the database) and stay as an overlay re-applied after every `reload` until `settle` or `revert`; `set_read_local(ids, true)` also retains `ids`; `mark_all_read_local` marks the loaded unread rows read and retains them; `revert` restores exactly the flags it changed (a retained id stays retained, as on the Mac).
- `excerpt` (Mac `HTMLText.plain`): each `<…>` (or `<` to the end of the text when unclosed) becomes a space; then, in this order, `&nbsp;`→space, `&lt;`, `&gt;`, `&quot;`, `&#39;`, `&apos;`, `&amp;`; whitespace runs collapse to one space, trimmed; more than `limit` chars → the first `limit` chars + `…`. `ListRow::new` uses `excerpt(snippet or "", EXCERPT_CHARS)`, `display_title`, `web_link(url)`.
- `web_link`: `http://` or `https://` (ASCII case-insensitive) and something after it; else `None`.
- `relative_date` (not determined by the tests alone):

```rust
pub fn relative_date(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - at).num_seconds();
    let (n, past) = (secs.unsigned_abs(), secs >= 0);
    // largest unit first; the one-unit name when it has one
    const UNITS: [(u64, &str, Option<(&str, &str)>); 7] = [
        (365 * 86_400, "year", Some(("last year", "next year"))), (30 * 86_400, "month", Some(("last month", "next month"))),
        (7 * 86_400, "week", Some(("last week", "next week"))), (86_400, "day", Some(("yesterday", "tomorrow"))),
        (3_600, "hour", None), (60, "minute", None), (1, "second", None)];
    for (span, unit, named) in UNITS {
        let k = n / span;
        if k == 0 { continue; }
        if let (1, Some((last, next))) = (k, named) { return if past { last } else { next }.into(); }
        let plural = if k == 1 { "" } else { "s" };
        return if past { format!("{k} {unit}{plural} ago") } else { format!("in {k} {unit}{plural}") };
    }
    "now".into()
}
```

- [ ] **Step 1: Write the failing tests** in `entries.rs` (`use crate::test_db::*;` helpers `fn q(source: Source) -> ListQuery` (filter `All`, no search), `fn row_ids(l: &EntryList) -> Vec<i64>`, `fn opened(db: &TestDb, query: ListQuery, page: usize) -> (Store, EntryList)` that restarts with `keeping: None` at `today()`):

```rust
    #[test]
    fn untitled_entries_and_excerpts_read_like_the_mac_list() {
        assert_eq!([display_title(None), display_title(Some(" \n ")), display_title(Some("  Tides "))], ["Untitled", "Untitled", "Tides"]);
        assert_eq!(excerpt("<p>a&nbsp;b &lt;tag&gt; &quot;q&quot; &#39;s&apos;</p>\n\n<div>end</div>", 280), "a b <tag> \"q\" 's' end");
        assert_eq!(excerpt("<p>Caf&eacute; &amp; cr&egrave;me</p>", 280), "Caf&eacute; & cr&egrave;me");
        assert_eq!(excerpt("&amp;lt;", 280), "&lt;", "&amp; is decoded last");
        assert_eq!(excerpt("before <a href=\"x\" never closed", 280), "before");
        assert_eq!(excerpt(&"é".repeat(300), 280), format!("{}…", "é".repeat(280)));
        assert_eq!(excerpt(&"é".repeat(280), 280), "é".repeat(280));
    }

    #[test]
    fn only_web_links_are_links() {
        assert_eq!(web_link(Some("https://e.org/a")).as_deref(), Some("https://e.org/a"));
        assert_eq!(web_link(Some("HTTP://e.org/a")).as_deref(), Some("HTTP://e.org/a"));
        for bad in [Some("javascript:alert(1)"), Some("mailto:a@b.org"), Some("https://"), Some(""), None] {
            assert_eq!(web_link(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn relative_dates_name_the_largest_unit_like_the_mac_list() {
        let now = at("2026-10-16T12:00:00Z");
        let ago = |s: i64| relative_date(now - chrono::Duration::seconds(s), now);
        assert_eq!([ago(0), ago(1), ago(45), ago(60), ago(7_200)], ["now", "1 second ago", "45 seconds ago", "1 minute ago", "2 hours ago"]);
        assert_eq!([ago(86_400), ago(3 * 86_400), ago(7 * 86_400), ago(15 * 86_400)], ["yesterday", "3 days ago", "last week", "2 weeks ago"]);
        assert_eq!([ago(31 * 86_400), ago(90 * 86_400), ago(400 * 86_400), ago(800 * 86_400)], ["last month", "3 months ago", "last year", "2 years ago"]);
        assert_eq!([ago(-3 * 3_600), ago(-86_400)], ["in 3 hours", "tomorrow"]);
    }

    /// Mac DataTests.entries_follow_source_and_unread_filter.
    #[test]
    fn source_and_filter_drive_the_list() {
        let db = TestDb::new();
        let a = db.entry(1, "2026-10-01T08:00:00.000Z", false);
        let b = db.entry_with(NewEntry { feed: 2, published: "2026-10-03T08:00:00.000Z", read: true, ..Default::default() });
        let c = db.entry_with(NewEntry { feed: 8, published: "2026-10-02T08:00:00.000Z", starred: true, ..Default::default() });
        let ids = |query| row_ids(&opened(&db, query, PAGE_SIZE).1);
        assert_eq!(ids(q(Source::Folder(1))), [b, a]);
        assert_eq!(ids(ListQuery { filter: ListFilter::Unread, ..q(Source::Folder(1)) }), [a]);
        assert_eq!([ids(q(Source::Unread)), ids(q(Source::Starred)), ids(q(Source::Feed(8)))], [vec![c, a], vec![c], vec![c]]);
        let (_, l) = opened(&db, q(Source::Feed(1)), PAGE_SIZE);
        assert_eq!((l.rows()[0].title.as_str(), l.rows()[0].excerpt.as_str(), l.rows()[0].link.as_deref()), ("Entry", "Summary", Some("https://example.org/a")));
        assert_eq!(l.rows()[0].feed_title, "xkcd.com");
    }

    #[test]
    fn a_long_list_loads_one_page_and_counts_the_whole_list() {
        let db = TestDb::new();
        let ids = db.days(5, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert_eq!(row_ids(&l), ids[..2]);
        assert_eq!((l.total(), l.unread(), l.has_more()), (5, 5, true));
        l.load_more(&store, today()).unwrap();
        assert_eq!(row_ids(&l), ids[..4]);
        l.load_more(&store, today()).unwrap();
        assert_eq!((row_ids(&l), l.has_more()), (ids, false));
    }

    #[test]
    fn rows_appearing_near_the_end_schedule_one_page() {
        let db = TestDb::new();
        let ids = db.days(26, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 24);
        assert_eq!(l.row_appeared(ids[3]), None, "21 rows from the end of 24: not near it");
        let g = l.row_appeared(ids[4]).expect("20 rows from the end");
        assert_eq!(l.row_appeared(ids[5]), None, "one page per turn, however many rows appear");
        assert!(l.scheduled_page_due(g));
        l.load_more(&store, today()).unwrap();
        assert_eq!((l.rows().len(), l.has_more()), (26, false));
        assert_eq!(l.row_appeared(ids[25]), None, "nothing left to load");
    }

    #[test]
    fn a_scheduled_page_is_dropped_when_the_list_changed() {
        let db = TestDb::new();
        let first = db.days(30, 1, |_| false);
        db.days(30, 8, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 24);
        let g = l.row_appeared(first[4]).unwrap();
        l.change(q(Source::Feed(8)), &store, None, today()).unwrap();
        assert!(!l.scheduled_page_due(g));
        assert_eq!((l.rows().len(), l.has_more()), (24, true));
    }

    #[test]
    fn new_entries_never_push_the_loaded_rows_out() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        let newer: Vec<i64> = (20..=23).map(|d| db.entry(1, &format!("2026-10-{d}T08:00:00.000Z"), false)).collect();
        assert!(!l.reload(&store, today()).unwrap(), "the list has a floor: no restart");
        let mut want: Vec<i64> = newer.into_iter().rev().collect();
        want.extend(&ids[..2]);
        assert_eq!((row_ids(&l), l.total(), l.has_more()), (want, 7, true));
    }

    /// The first run: the list was empty, so whole, when the first refresh filled it.
    #[test]
    fn a_short_list_made_long_by_a_refresh_is_paged_again() {
        let db = TestDb::new();
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert!(l.rows().is_empty() && !l.has_more());
        let ids = db.days(5, 1, |_| false);
        assert!(l.reload(&store, today()).unwrap(), "a whole list grew past a page");
        assert!(l.restart_due(l.generation()));
        l.restart(&store, None, today()).unwrap();
        assert_eq!((row_ids(&l), l.total(), l.has_more()), (ids[..2].to_vec(), 5, true));
        assert!(!l.restart_due(l.generation()));
    }

    #[test]
    fn the_selection_stays_loaded_when_the_list_restarts() {
        let db = TestDb::new();
        let ids = db.days(4, 1, |_| false);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        l.load_more(&store, today()).unwrap();
        l.change(ListQuery { filter: ListFilter::Unread, ..q(Source::Feed(1)) }, &store, Some(ids[3]), today()).unwrap();
        assert_eq!((row_ids(&l), l.has_more()), (ids, false));
    }

    #[test]
    fn mark_all_read_counts_the_rows_not_loaded() {
        let db = TestDb::new();
        db.days(3, 1, |day| day != 1); // only the oldest, not loaded, is unread
        let (store, mut l) = opened(&db, q(Source::Feed(1)), 2);
        assert!(l.rows().iter().all(|r| r.is_read) && l.can_mark_all_read());
        db.execute("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z'");
        l.reload(&store, today()).unwrap();
        assert!(!l.can_mark_all_read());
    }

    /// Mac `AppState.merge` and ListPageTests.reading_through_a_long_unread_list_keeps_the_selection_and_the_counts.
    #[test]
    fn read_rows_stay_listed_in_an_unread_list_until_the_source_changes() {
        let db = TestDb::new();
        let ids = db.days(6, 1, |_| false);
        let unread = ListQuery { filter: ListFilter::Unread, ..q(Source::Feed(1)) };
        let (store, mut l) = opened(&db, unread.clone(), 2);
        let _ = l.set_read_local(&[ids[1]], true);
        db.execute(&format!("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}", ids[1]));
        l.reload(&store, today()).unwrap();
        assert_eq!(row_ids(&l), ids[..2]);
        assert!(l.rows()[1].is_read);
        assert_eq!((l.unread(), l.total(), l.has_more()), (5, 6, true));
        l.load_more(&store, today()).unwrap();
        assert_eq!(row_ids(&l)[..2], ids[..2]);
        l.change(q(Source::Feed(8)), &store, None, today()).unwrap();
        l.change(unread, &store, None, today()).unwrap();
        assert!(!row_ids(&l).contains(&ids[1]), "a new visit forgets what the last one read");
    }

    #[test]
    fn search_replaces_the_list_and_ignores_the_unread_filter() {
        let db = TestDb::new();
        let a = db.entry_with(NewEntry { title: Some("Café crème"), body: Some("le petit déjeuner"), published: "2026-10-02T08:00:00.000Z", ..Default::default() });
        let b = db.entry_with(NewEntry { title: Some("Cafe noir"), body: Some("le soir"), read: true, ..Default::default() });
        let search = |text: &str| ListQuery { source: Source::Feed(1), filter: ListFilter::Unread, search: Some(text.into()) };
        let (_, l) = opened(&db, search("cafe"), PAGE_SIZE);
        assert_eq!(row_ids(&l), [a, b]);
        assert!(!l.can_mark_all_read(), "never during a search");
        assert_eq!(row_ids(&opened(&db, search("cafe dejeuner"), PAGE_SIZE).1), [a]);
        assert_eq!(opened(&db, search("zzz"), PAGE_SIZE).1.empty_text(), Some(NO_RESULTS));
    }

    #[test]
    fn empty_lists_say_why() {
        let db = TestDb::new();
        let text = |query| opened(&db, query, PAGE_SIZE).1.empty_text();
        assert_eq!(text(q(Source::Feed(1))), Some(NO_ENTRIES));
        assert_eq!(text(q(Source::Unread)), Some(NO_UNREAD));
        assert_eq!(text(ListQuery { filter: ListFilter::Unread, ..q(Source::Starred) }), Some(NO_UNREAD));
        db.entry(1, "2026-10-01T08:00:00.000Z", false);
        assert_eq!(text(q(Source::Feed(1))), None);
        assert_eq!([NO_RESULTS, NO_UNREAD, NO_ENTRIES], ["No results", "No unread entries", "No entries"]);
    }

    #[test]
    fn local_changes_show_at_once_and_revert_exactly() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |day| day == 2);
        let (store, mut l) = opened(&db, q(Source::Feed(1)), PAGE_SIZE);
        let read = |l: &EntryList| l.rows().iter().map(|r| r.is_read).collect::<Vec<_>>();
        assert_eq!(read(&l), [false, true, false]);
        let r = l.set_read_local(&[ids[0]], true);
        assert_eq!(read(&l), [true, true, false]);
        l.reload(&store, today()).unwrap();
        assert_eq!(read(&l), [true, true, false], "a reload while the write is pending keeps the mark");
        l.revert(r);
        assert_eq!(read(&l), [false, true, false]);
        let r = l.set_read_local(&[ids[0]], true);
        db.execute(&format!("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}", ids[0]));
        l.settle(r);
        l.reload(&store, today()).unwrap();
        assert_eq!(read(&l), [true, true, false], "settled: the database says so now");
        db.execute(&format!("UPDATE entry SET read_at = NULL WHERE id = {}", ids[0]));
        l.reload(&store, today()).unwrap();
        assert_eq!(read(&l), [false, true, false], "no overlay left after settle");
        let r = l.set_starred_local(ids[2], true);
        assert!(l.rows()[2].is_starred);
        l.revert(r);
        assert!(!l.rows()[2].is_starred);
        let r = l.mark_all_read_local();
        assert_eq!(read(&l), [true, true, true]);
        l.revert(r);
        assert_eq!(read(&l), [false, true, false], "the row read before stays read");
        assert_eq!((db.read_at(ids[2]), db.starred(ids[2])), (None, false), "local edits never write");
    }

    #[test]
    fn the_journal_loads_whole_in_rank_order_and_keeps_read_rows_at_their_rank() {
        let db = TestDb::new();
        let ids = db.days(3, 1, |_| false);
        db.edition(&[(ids[2], "front"), (ids[0], "wire"), (ids[1], "wire")]);
        let (store, mut l) = opened(&db, ListQuery { filter: ListFilter::Unread, ..q(Source::Journal) }, 1);
        assert_eq!((row_ids(&l), l.has_more()), (vec![ids[2], ids[0], ids[1]], false));
        let _ = l.set_read_local(&[ids[0]], true);
        db.execute(&format!("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z' WHERE id = {}", ids[0]));
        assert!(!l.reload(&store, today()).unwrap());
        assert_eq!(row_ids(&l), [ids[2], ids[0], ids[1]]);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac-ui entries::`
Expected: compile errors (`unresolved import crate::test_db`, `cannot find function display_title`) until the module skeleton exists, then failures.

- [ ] **Step 3: Implement** `test_db.rs`, then `entries.rs` as the header and rules above.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac-ui && cargo clippy -p ressac-ui --all-targets -- -D warnings`
Expected: every `test result: ok.` (16 new in `entries`; B1's `settings` and `launch` tests unchanged); clippy `Finished`. Then the full gate (B2a ends here).

- [ ] **Step 5: Commit**

```bash
git add Cargo.lock crates/ressac-ui/Cargo.toml crates/ressac-ui/src/lib.rs crates/ressac-ui/src/entries.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: entry lists (floor paging, retained rows, filter, search, row texts, local edits)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B2b: sidebar, live data, worker, runs

### Task 3 (B2b): `ressac_ui::sources`

**Files:**
- Create: `crates/ressac-ui/src/sources.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod sources;`)

**Interfaces:**
- Consumes: `Store::sources` (`SourcesSnapshot`, `SourceNode`), `Store::feed_facts` (Task 1), `CadenceGroup::ALL`, `Status`.
- Produces: `ressac_ui::sources` of the header.

Rules (inventory §2.2, §2.3; `MainView.swift:98-168`; `GardenStatusStyle.swift`):
- `rows()`: heading "Smart Lists"; Journal (`view-pim-news`, badge `counts.journal`), Unread (`mail-unread`, `counts.unread`), Today (`brightness-high`, `counts.today`), Starred (`starred-symbolic`, `counts.starred`); heading "Feeds" (always, even with no feed); the tree: a folder row (`folder`, badge = subtree unread, `expandable` when it has children, `expanded` from the set) followed, when expanded, by its children one `depth` deeper; a feed row (no icon, badge = unread, `status`, `prefer_full_text`, `site` from `feed_facts`); heading "Rhythm"; Daily `flash`, Weekly `view-calendar-week`, Monthly `weather-clear-night`, Rare `star-shape`, Silent `system-suspend-symbolic` (badge `counts.cadence`, Silent always listed); heading "Garden"; Needs Triage (`games-hint`, `counts.needs_triage`), All Feeds (`format-list-unordered`, 0), Cemetery (`box`, 0). Headings: `key = "heading:<title>"`, no selection, icon `""`. Folders start closed.
- `status_color`: Dead `red`, Hijacked `purple`, Moved `blue`, Ailing `orange`, Duplicate `teal`, Fossil `brown`, Dormant `indigo`, Alive `green`, else `""`. `sidebar_dot`: `Some(status_color)` for Ailing, Dead, Moved, Hijacked, Duplicate only. `SidebarRow::dot()` = `sidebar_dot(self.status)` for feed rows.
- `reveal`: a feed or folder selection opens every folder above it (the snapshot's tree gives the path).

- [ ] **Step 1: Write the failing tests** (`use crate::test_db::*;` helper `fn loaded(db: &TestDb) -> SidebarTree`, `fn titles(rows: &[SidebarRow]) -> Vec<&str>`, `fn row<'a>(rows: &'a [SidebarRow], key: &str) -> &'a SidebarRow`):

```rust
    #[test]
    fn selection_keys_round_trip() {
        let all = [Selection::Source(Source::Journal), Selection::Source(Source::Unread), Selection::Source(Source::Today),
            Selection::Source(Source::Starred), Selection::Source(Source::Folder(7)), Selection::Source(Source::Feed(12)),
            Selection::Source(Source::Cadence(CadenceGroup::Weekly)), Selection::Garden(GardenScope::NeedsTriage),
            Selection::Garden(GardenScope::All), Selection::Garden(GardenScope::Cemetery)];
        assert_eq!(all.map(|s| s.key()), ["journal", "unread", "today", "starred", "folder:7", "feed:12", "rhythm:weekly",
            "garden:needs-triage", "garden:all", "garden:cemetery"]);
        for s in all {
            assert_eq!(Selection::parse_key(&s.key()), Some(s));
        }
        for bad in ["", "feed:", "feed:x", "folder:-1x", "rhythm:hourly", "garden:compost", "heading:Feeds", "Unread"] {
            assert_eq!(Selection::parse_key(bad), None, "{bad}");
        }
        assert_eq!(Selection::default(), Selection::Source(Source::Unread));
        assert!(Selection::Garden(GardenScope::All).is_garden() && Selection::Garden(GardenScope::All).source().is_none());
        assert_eq!(GardenScope::ALL.map(GardenScope::title), ["Needs Triage", "All Feeds", "Cemetery"]);
    }

    #[test]
    fn the_sidebar_lists_the_mac_sections_with_folders_closed() {
        let db = TestDb::new();
        let rows = loaded(&db).rows();
        assert_eq!(titles(&rows), ["Smart Lists", "Journal", "Unread", "Today", "Starred", "Feeds", "Blogs", "Biblis", "Infos", "Jon",
            "Rhythm", "Daily", "Weekly", "Monthly", "Rare", "Silent", "Garden", "Needs Triage", "All Feeds", "Cemetery"]);
        let icons: Vec<&str> = rows.iter().filter(|r| r.kind != RowKind::Heading).map(|r| r.icon).collect();
        assert_eq!(icons, ["view-pim-news", "mail-unread", "brightness-high", "starred-symbolic", "folder", "folder", "folder", "folder",
            "flash", "view-calendar-week", "weather-clear-night", "star-shape", "system-suspend-symbolic", "games-hint", "format-list-unordered", "box"]);
        let blogs = row(&rows, "folder:1");
        assert_eq!((blogs.kind, blogs.depth, blogs.expandable, blogs.expanded), (RowKind::Folder, 0, true, false));
        assert_eq!(HEADINGS, ["Smart Lists", "Feeds", "Rhythm", "Garden"]);
        assert_eq!(row(&rows, "heading:Feeds").selection(), None);
        assert_eq!(index_of(&rows, &Selection::default()), Some(2));
    }

    #[test]
    fn opening_a_folder_shows_its_children_one_level_deeper() {
        let db = TestDb::new();
        let mut tree = loaded(&db);
        assert!(tree.toggle(1));
        let rows = tree.rows();
        let at = index_of(&rows, &Selection::Source(Source::Folder(1))).unwrap();
        assert_eq!(titles(&rows[at + 1..at + 4]), ["xkcd.com", "potes", "Girl on the Couch"]);
        assert!(rows[at + 1..at + 4].iter().all(|r| r.depth == 1));
        assert!(!tree.toggle(1));
        tree.reveal(&Selection::Source(Source::Feed(11)));
        let rows = tree.rows();
        assert_eq!(row(&rows, "feed:11").depth, 2, "Jon > YouTube > Center for Humane Technology (1)");
    }

    #[test]
    fn badges_follow_the_counts_and_folders_sum_their_subtree() {
        let db = TestDb::new();
        db.entry(2, "2026-10-16T08:00:00.000Z", false);            // potes, inside Blogs; today
        db.entry_with(NewEntry { feed: 2, read: true, starred: true, ..Default::default() });
        db.entry(8, "2026-10-01T08:00:00.000Z", false);            // Infos
        db.health(8, "Alive", "weekly", false);
        db.health(3, "Dead", "daily", true);
        let rows = loaded(&db).rows();
        let badge = |key| row(&rows, key).badge;
        assert_eq!(["journal", "unread", "today", "starred"].map(badge), [0, 2, 1, 1]);
        assert_eq!(["folder:1", "folder:4", "folder:3"].map(badge), [1, 1, 0]);
        assert_eq!(["rhythm:daily", "rhythm:weekly", "rhythm:silent"].map(badge), [0, 1, 0]);
        assert_eq!(["garden:needs-triage", "garden:all", "garden:cemetery"].map(badge), [1, 0, 0]);
    }

    #[test]
    fn sick_feeds_get_a_coloured_dot_and_the_others_none() {
        assert_eq!(Status::ALL.map(|s| sidebar_dot(Some(s))),
            [Some("teal"), Some("purple"), Some("blue"), Some("red"), Some("orange"), None, None, None, None]);
        assert_eq!(Status::ALL.map(|s| status_color(Some(s))), ["teal", "purple", "blue", "red", "orange", "brown", "indigo", "green", ""]);
        assert_eq!((status_color(None), sidebar_dot(None)), ("", None));
        let db = TestDb::new();
        db.health(1, "Ailing", "daily", false);
        db.execute("UPDATE feed SET prefer_full_text = 1, html_url = 'https://xkcd.com/' WHERE id = 1");
        let mut tree = loaded(&db);
        tree.toggle(1);
        let rows = tree.rows();
        let xkcd = row(&rows, "feed:1");
        assert_eq!((xkcd.dot(), xkcd.status.map(Status::as_str), xkcd.prefer_full_text, xkcd.site.as_deref()),
                   (Some("orange"), Some("Ailing"), true, Some("https://xkcd.com/")));
        assert!(tree.set_prefer_full_text_local(1, false));
        assert!(!row(&tree.rows(), "feed:1").prefer_full_text);
    }

    /// Mac FixWaveTests.todayCountsEveryEntryOfTheDayAndRestartsAtMidnight.
    #[test]
    fn today_counts_every_entry_of_the_day_and_restarts_at_midnight() {
        let db = TestDb::new();
        db.entry(1, "2026-10-10T08:00:00.000Z", true);
        db.entry(1, "2026-10-10T08:30:00.000Z", false);
        db.entry(1, "2026-10-08T08:30:00.000Z", false);
        let store = db.store();
        let mut tree = SidebarTree::default();
        tree.load(&store, at("2026-10-10T00:00:00Z")).unwrap();
        assert_eq!(row(&tree.rows(), "today").badge, 2, "read and unread");
        tree.load(&store, at("2026-10-11T00:00:00Z")).unwrap();
        assert_eq!(row(&tree.rows(), "today").badge, 0);
    }
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui sources::` → compile errors.
- [ ] **Step 3: Implement** `sources.rs` as the header and rules above.
- [ ] **Step 4: Run them to verify they pass** — `cargo test -p ressac-ui sources:: && cargo clippy -p ressac-ui --all-targets -- -D warnings` → `test result: ok. 6 passed`; clippy `Finished`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/sources.rs crates/ressac-ui/src/lib.rs
git commit -m "ressac-ui: sidebar (selection keys, sections, folder tree, badges, status dots, icons)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 4 (B2b): `ressac_ui::live`

**Files:**
- Create: `crates/ressac-ui/src/live.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod live;`)

**Interfaces:**
- Consumes: `Store::data_version` (tests).
- Produces: `ressac_ui::live` of the header.

`plan_rows` (validated in the container, 2000 random reloads): kept rows are a longest run already in order, so a moved row is one removal and one insertion; removals bottom-up in contiguous runs (old indices), then insertions top-down in contiguous runs (final indices), then changes of kept rows whose value differs, in contiguous runs (final indices).

```rust
pub fn plan_rows<T: Clone + PartialEq, K: Eq + Hash>(old: &[T], new: &[T], key: impl Fn(&T) -> K) -> Vec<RowOp<T>> {
    let at_new: HashMap<K, usize> = new.iter().enumerate().map(|(i, r)| (key(r), i)).collect();
    let seq: Vec<Option<usize>> = old.iter().map(|r| at_new.get(&key(r)).copied()).collect();
    let kept_old: HashSet<usize> = longest_increasing(&seq).into_iter().collect(); // old indices, patience sorting
    let kept_new: HashSet<usize> = kept_old.iter().map(|&o| seq[o].unwrap()).collect();
    // 1. Remove runs of old indices not kept, from the bottom: Remove { first, last }.
    // 2. Insert runs of new indices not kept, from the top: Insert { at, rows: new[at..end] }.
    // 3. Change runs of new indices j = seq[o] (o kept) where old[o] != new[j]: Change { first, rows }.
}
```

- [ ] **Step 1: Write the failing tests:**

```rust
    #[test]
    fn the_watcher_sees_only_other_connections_commits() {
        let db = crate::test_db::TestDb::new();
        let gui = db.store();
        let mut live = Invalidation::default();
        assert!(!live.version_seen(gui.data_version().unwrap()), "the first poll is the baseline");
        db.entry(1, "2026-10-01T08:00:00.000Z", false);
        assert!(live.version_seen(gui.data_version().unwrap()));
        assert!(!live.version_seen(gui.data_version().unwrap()));
        db.execute("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z'");
        assert!(live.version_seen(gui.data_version().unwrap()));
    }

    #[test]
    fn polling_runs_while_a_window_is_active_or_the_chart_is_on_screen() {
        assert_eq!([should_poll(true, false), should_poll(false, true), should_poll(true, true), should_poll(false, false)],
                   [true, true, true, false]);
        assert_eq!((POLL_INTERVAL, PROGRESS_RELOAD_GAP), (Duration::from_secs(3), Duration::from_secs(1)));
    }

    #[test]
    fn becoming_active_reloads_once_and_takes_a_fresh_baseline() {
        let mut live = Invalidation::default();
        live.version_seen(5);
        assert!(live.activated(9));
        assert!(!live.version_seen(9));
        assert!(live.version_seen(10));
    }

    #[test]
    fn run_progress_reloads_at_most_once_a_second_and_every_run_end_reloads() {
        let mut live = Invalidation::default();
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        assert!(live.run_progress(t0));
        assert!(!live.run_progress(ms(999)));
        assert!(live.run_progress(ms(1000)));
        assert!(live.run_finished(ms(1100)));
        assert!(!live.run_progress(ms(1500)), "the end counts as a notice");
        assert!(live.run_progress(ms(2100)));
    }

    #[test]
    fn the_day_changes_at_local_midnight() {
        let mut live = Invalidation::default();
        let paris_midnight = |d: &str| crate::test_db::at(&format!("{d}T22:00:00Z"));
        assert!(!live.day_seen(paris_midnight("2026-10-15")), "baseline");
        assert!(!live.day_seen(paris_midnight("2026-10-15")));
        assert!(live.day_seen(paris_midnight("2026-10-16")));
    }

    fn rows(ids: &[u32]) -> Vec<(u32, u32)> { ids.iter().map(|&i| (i, 0)).collect() }
    fn rebuilds(old: &[(u32, u32)], new: &[(u32, u32)]) -> Vec<RowOp<(u32, u32)>> {
        let ops = plan_rows(old, new, |r| r.0);
        let mut got = old.to_vec();
        apply_rows(&mut got, &ops);
        assert_eq!(got, new, "{ops:?}");
        ops
    }

    #[test]
    fn a_reload_that_changes_nothing_plans_nothing() {
        let a = rows(&[1, 2, 3]);
        assert!(rebuilds(&a, &a).is_empty());
    }

    #[test]
    fn new_rows_on_top_a_gone_row_and_a_changed_row_are_three_operations() {
        let old = rows(&(1..=10).collect::<Vec<_>>());
        let mut new = rows(&[21, 20, 1, 3, 4, 5, 6, 7, 8, 9, 10]);
        new[7].1 = 1; // entry 7 changed (read, starred, title…)
        assert_eq!(rebuilds(&old, &new), [
            RowOp::Remove { first: 1, last: 1 },
            RowOp::Insert { at: 0, rows: rows(&[21, 20]) },
            RowOp::Change { first: 7, rows: vec![(7, 1)] },
        ]);
    }

    #[test]
    fn a_moved_row_is_one_removal_and_one_insertion() {
        let old = rows(&[1, 2, 3, 4]);
        assert_eq!(rebuilds(&old, &rows(&[3, 1, 2, 4])), [RowOp::Remove { first: 2, last: 2 }, RowOp::Insert { at: 0, rows: rows(&[3]) }]);
        rebuilds(&old, &rows(&[4, 3, 2, 1]));
        rebuilds(&old, &[]);
        rebuilds(&[], &old);
    }

    #[test]
    fn any_reload_plans_rows_that_rebuild_it_exactly() {
        let mut seed: u64 = 20_261_023;
        let mut next = move || { seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407); (seed >> 33) as u32 };
        for _ in 0..2000 {
            // plain loops: two closures cannot both borrow `next` mutably
            let mut old: Vec<(u32, u32)> = Vec::new();
            for i in 0..next() % 30 { if next() % 3 != 0 { old.push((i, next() % 2)); } }
            let mut new: Vec<(u32, u32)> = Vec::new();
            for r in &old { if next() % 4 != 0 { new.push((r.0, next() % 2)); } }
            for i in 0..next() % 5 { let at = next() as usize % (new.len() + 1); new.insert(at, (100 + i, 0)); }
            if new.len() > 2 && next() % 2 == 0 { let r = new.remove(0); new.push(r); }
            rebuilds(&old, &new);
        }
    }
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui live::` → compile errors.
- [ ] **Step 3: Implement** `live.rs` (header; `plan_rows` as above; `Invalidation` fields `version: Option<i64>`, `notice: Option<Instant>`, `today: Option<DateTime<Utc>>`).
- [ ] **Step 4: Run them to verify they pass** — `cargo test -p ressac-ui live:: && cargo clippy -p ressac-ui --all-targets -- -D warnings` → `ok. 9 passed`; `Finished`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/live.rs crates/ressac-ui/src/lib.rs
git commit -m "ressac-ui: live data rules (data_version, 3 s poll, run reloads, midnight) and row diffs by id

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 5 (B2b): `ressac_ui::worker`

**Files:**
- Create: `crates/ressac-ui/src/worker.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod worker;`)

**Interfaces:**
- Consumes: `Ressac::open`, `Ressac::store`, `EventSink`, `Event` (core); `tokio::runtime::Builder::new_current_thread().enable_all()`.
- Produces: `ressac_ui::worker` of the header.

Rules (audit §3.3, `tui/reader/jobs.rs`): `Worker::new` spawns the short thread at once (it owns a `Receiver` of boxed jobs, a runtime, and an `Option<Ressac>` opened with `Ressac::open(db)` on the first job and again after a failed open); jobs run in submission order; each job and each long run is wrapped in `catch_unwind(AssertUnwindSafe(…))`: a panic is `JobFailure::Panicked` and the short thread carries on. `long` spawns a thread: `Ressac::open`, its own runtime, a private `ForwardSink` (an `EventSink` calling `events`), `stop` = the worker's flag; `done` runs on that thread after the job. An open failure is `JobFailure::Core(e)`. `stop()` sets the flag (`Ordering::Relaxed`); `Drop` calls it. `JobFailure::is_busy` = `Core(e)` with `e.is_lock_contention()`; `message()` = the error's `Display`, or "The job stopped unexpectedly.".

- [ ] **Step 1: Write the failing tests** (`use crate::test_db::TestDb; use std::sync::mpsc;` and `const WAIT: Duration = Duration::from_secs(10);`):

```rust
    /// Mac RunCoordinatorTests.short_runs_run_one_at_a_time_in_order.
    #[test]
    fn short_jobs_run_one_at_a_time_in_order() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let log = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        for n in [3, 1, 2] {
            let (log, tx) = (log.clone(), tx.clone());
            worker.short(move |_| {
                log.lock().unwrap().push(format!("start {n}"));
                std::thread::sleep(Duration::from_millis(30));
                log.lock().unwrap().push(format!("end {n}"));
                Ok(n)
            }, move |r| tx.send(r.unwrap()).unwrap());
        }
        let done: Vec<i32> = (0..3).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
        assert_eq!(done, [3, 1, 2]);
        assert_eq!(*log.lock().unwrap(), ["start 3", "end 3", "start 1", "end 1", "start 2", "end 2"]);
    }

    #[test]
    fn short_jobs_run_while_a_long_job_runs() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (release, wait) = mpsc::channel::<()>();
        let (tx, rx) = mpsc::channel();
        let long_tx = tx.clone();
        worker.long(move |_, _, _| { wait.recv_timeout(WAIT).unwrap(); Ok("long") }, |_| {}, move |r| long_tx.send(r.unwrap()).unwrap());
        worker.short(|cx| Ok(cx.app.store().feed_count()?), move |r| tx.send(if r.unwrap() == 11 { "short" } else { "?" }).unwrap());
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "short", "the long job is still waiting");
        release.send(()).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "long");
    }

    #[test]
    fn a_long_job_s_events_arrive_before_its_end_and_stop_reaches_it() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        let event_tx = Mutex::new(tx.clone());
        let (asked, stopped) = mpsc::channel::<()>();
        worker.long(move |_, sink, stop| {
            sink.emit(&Event::Started { total: 2 });
            stopped.recv_timeout(WAIT).unwrap();
            Ok(stop.load(Ordering::Relaxed))
        }, move |e| event_tx.lock().unwrap().send(format!("{e:?}")).unwrap(), move |r| tx.send(format!("done {}", r.unwrap())).unwrap());
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "Started { total: 2 }");
        worker.stop();
        asked.send(()).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "done true");
    }

    #[test]
    fn a_database_that_cannot_be_opened_fails_the_job_and_the_next_one_still_runs() {
        let worker = Worker::new(Path::new("/nonexistent/dir/r.sqlite"));
        let (tx, rx) = mpsc::channel();
        worker.short(|_| Ok(()), move |r| tx.send(r.map_err(|f| f.message())).unwrap());
        assert!(rx.recv_timeout(WAIT).unwrap().is_err());
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        worker.short(|_| -> ressac_core::Result<()> { panic!("boom") }, move |r| tx.send(r.map_err(|f| f.message())).unwrap());
        worker.short(|_| Ok(()), move |r| tx2.send(r.map_err(|f| f.message())).unwrap());
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Err("The job stopped unexpectedly.".to_string()));
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Ok(()));
    }

    #[test]
    fn a_job_writes_on_its_own_connection_and_the_gui_connection_sees_it() {
        let db = TestDb::new();
        let id = db.entry(1, "2026-10-01T08:00:00.000Z", false);
        let gui = db.store();
        let before = gui.data_version().unwrap();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        worker.short(move |cx| cx.app.store().set_read(id, true, chrono::Utc::now()), move |r| tx.send(r.is_ok()).unwrap());
        assert!(rx.recv_timeout(WAIT).unwrap());
        assert_ne!(gui.data_version().unwrap(), before);
        assert!(db.read_at(id).is_some());
    }
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui worker::` → compile errors. (As root the `/nonexistent` open succeeds: run as a normal user, as the gate does.)
- [ ] **Step 3: Implement** `worker.rs` as the header and rules above.
- [ ] **Step 4: Run them to verify they pass** — `cargo test -p ressac-ui worker:: && cargo clippy -p ressac-ui --all-targets -- -D warnings` → `ok. 5 passed`; `Finished`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/worker.rs crates/ressac-ui/src/lib.rs
git commit -m "ressac-ui: worker threads (FIFO short runs, one thread per long run, own connections)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 6 (B2b): `ressac_ui::runs` (and Review Focus 3)

**Files:**
- Create: `crates/ressac-ui/src/runs.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod runs;`)

**Interfaces:**
- Consumes: `Selection` (Task 3), `Worker`, `JobFailure`, `JobContext` (Task 5), `EntryList` (Task 2, tests), `check_subset`, `RefreshOptions`, `Event`, `Store` writes (core).
- Produces: `ressac_ui::runs` of the header. B3–B6 add their jobs next to `fetch_options` and use `RunRequest`'s existing variants.

Rules (inventory §8.2, §8.4; `RunCoordinator.swift`; `JournalList.swift:83-103`):
- `kind`: Long = FetchAll, FetchFeeds, FetchFolder, Import, BuildEdition, Add; the others Short. `shows_failure_banner`: false for Extract and Add. `refresh(selection)`: Feed(id) → `FetchFeeds(vec![id])`, Folder(id) → `FetchFolder(id)`, everything else → `FetchAll`.
- `is_refusal(error)`: Decide, Undo → `Invalid | UnknownFeed`; BuildEdition → `EditionNotDue`; SetCadence, SetTheme, MoveFeed → `UnknownFeed | FeedBuried | UnknownFolder | UnknownFolderName | AmbiguousFolder | UnknownTheme`; Add → `InvalidAddress | NoFeedFound | FeedExists | UnknownTheme | UnknownFolder | UnknownFolderName | AmbiguousFolder | UnknownFeed | Invalid`; others never. `failure(f)` = `RunFailure { message: f.message(), refusal: matches!(f, Core(e) if self.is_refusal(e)) }`.
- `admit`: a Long request while one is active → `Refused` (logged `tracing::info!`); a Long → active, `banner_set_during_run = false`; Short → `Start`.
- `event` (Long requests only change state): `Started{total}` → progress `(0, total, EDITION_LABEL if BuildEdition else FETCHING_FEEDS)`; `FeedDone`, `Extracted` → `done += 1`; `Probing{what, total}` → `(0, total, FETCHING_FULL_ARTICLES if what == "full articles" else "Checking <what>")`; `NetworkUnreachable{failed, attempted}` → banner `network_unreachable` (show_log false), set during the run. Returns true for `FeedDone`, `Extracted`, `Decided`, `Undone`, `Finished`, `Edition` (any kind).
- `finish`: Long → inactive, progress `None`. `Ok` → banner cleared unless a Long run set one during itself. `Err(f)` → when `shows_failure_banner`, `Banner { message, show_log: !refusal }`.
- `progress_text`: the edition label with `total > 0` → `"<label> <done>/<total>"`, else the label. `compact_progress_text`: edition → `"Edition… <done>/<total>"` (`"Edition…"` when total 0), else the label. `progress_width`: 260 edition, 140 otherwise. `fraction`: `Some(min(1, done/total))` only when both > 0.
- `network_unreachable(f, a)` = `"The network looks unreachable: {f} of {a} feeds could not connect."`. `write_failure`: busy → `BUSY`, else `COULD_NOT_SAVE + message`, `show_log: false`. `read_failure` = `COULD_NOT_READ + error`, `show_log: true`.
- `DirectWrite::apply`: `SetRead` → `set_read` per id; `SetStarred` → `set_starred`; `MarkAllRead` → `mark_all_read(source, today_start, now)`; `PreferFullText` → `set_prefer_full_text`.
- `fetch_options`: FetchAll → `{ force: true, only: None }`; FetchFeeds(ids) / FetchFolder(id) → `check_subset(store, ids, folder)` and `only: None` when it returns empty for an empty `ids`, else `Some(list)`, `force: true`; any other request → `Err(Error::Invalid("not a fetch request".into()))`. The B2c job is `cx.block_on(cx.app.refresh(fetch_options(cx.app.store(), &request)?, sink, stop)).map(drop)`.

- [ ] **Step 1: Write the failing tests** (`use crate::test_db::*; use ressac_core::Error;` helpers `fn busy() -> Error { Error::Db(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(5), Some("database is locked".into()))) }`, `fn failed(e: Error) -> JobFailure { JobFailure::Core(e) }`):

```rust
    #[test]
    fn long_run_is_refused_while_another_is_active() {
        let mut runs = RunCoordinator::default();
        assert_eq!(runs.admit(&RunRequest::FetchAll), Admission::Start);
        assert!(runs.long_run_active() && !runs.can_refresh());
        assert_eq!(runs.admit(&RunRequest::FetchFolder(1)), Admission::Refused);
        assert_eq!(runs.admit(&RunRequest::Decide(1, Decision::Keep)), Admission::Start, "short runs overlap a long run");
        runs.finish(&RunRequest::FetchAll, Ok(()));
        assert!(!runs.long_run_active() && runs.can_refresh());
        assert_eq!(runs.admit(&RunRequest::FetchFeeds(vec![1])), Admission::Start);
    }

    #[test]
    fn progress_follows_the_events() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        runs.admit(&fetch);
        assert!(!runs.event(&fetch, &Event::Started { total: 2 }));
        assert_eq!(runs.progress(), Some(&RunProgress { done: 0, total: 2, label: "Fetching feeds".into() }));
        assert_eq!(runs.progress().unwrap().fraction(), None, "indeterminate until something is done");
        assert!(runs.event(&fetch, &Event::FeedDone { feed_id: 1, title: "A".into(), error: None }));
        assert_eq!(runs.progress().unwrap().fraction(), Some(0.5));
        runs.event(&fetch, &Event::Probing { what: "full articles".into(), total: 3 });
        assert_eq!(runs.progress(), Some(&RunProgress { done: 0, total: 3, label: "Fetching full articles".into() }));
        runs.event(&fetch, &Event::Probing { what: "https".into(), total: 4 });
        assert_eq!(runs.progress().unwrap().label, "Checking https");
        runs.finish(&fetch, Ok(()));
        assert_eq!(runs.progress(), None);
    }

    #[test]
    fn the_toolbar_progress_counts_feeds_during_an_edition_build_and_shortens_when_narrow() {
        let edition = |done, total| RunProgress { done, total, label: EDITION_LABEL.into() };
        let fetching = RunProgress { done: 3, total: 10, label: "Fetching feeds".into() };
        assert_eq!(progress_text(&edition(34, 156)), "Building this morning's edition… 34/156");
        assert_eq!(progress_text(&edition(0, 0)), "Building this morning's edition…");
        assert_eq!(progress_text(&fetching), "Fetching feeds");
        assert_eq!([compact_progress_text(&edition(152, 156)), compact_progress_text(&edition(0, 0)), compact_progress_text(&fetching)],
                   ["Edition… 152/156", "Edition…", "Fetching feeds"]);
        assert_eq!((PROGRESS_INSET, progress_width(&edition(1, 2)), progress_width(&fetching)), (10, 260, 140));
        let mut runs = RunCoordinator::default();
        let build = RunRequest::BuildEdition { rebuild: false };
        runs.admit(&build);
        runs.event(&build, &Event::Started { total: 156 });
        assert_eq!(runs.progress().unwrap().label, EDITION_LABEL);
    }

    #[test]
    fn failure_shows_a_banner_and_the_next_success_clears_it() {
        let mut runs = RunCoordinator::default();
        let undo = RunRequest::Undo;
        let failure = undo.failure(&failed(Error::Io(std::io::Error::other("disk I/O error"))));
        runs.finish(&undo, Err(&failure));
        assert_eq!(runs.banner(), Some(&Banner { message: "I/O error: disk I/O error".into(), show_log: true }));
        runs.finish(&RunRequest::ApplySafe, Ok(()));
        assert_eq!(runs.banner(), None);
    }

    /// The CLI's exit-2 cases show their sentence without Show Log; the same error from a request
    /// the CLI never refuses keeps the log button.
    #[test]
    fn a_refused_run_shows_its_sentence_without_the_log_button() {
        let mut runs = RunCoordinator::default();
        for (request, error, sentence) in [
            (RunRequest::Decide(3, Decision::Repair), Error::Invalid("Nothing to repair for feed #3.".into()), "Nothing to repair for feed #3."),
            (RunRequest::SetCadence(11, Some(CadenceGroup::Daily)), Error::FeedBuried(11), "Feed #11 is buried."),
            (RunRequest::BuildEdition { rebuild: false }, Error::EditionNotDue, "An edition was already built this morning."),
        ] {
            runs.finish(&request, Err(&request.failure(&failed(error))));
            assert_eq!(runs.banner(), Some(&Banner { message: sentence.into(), show_log: false }), "{request:?}");
        }
        let fetch = RunRequest::FetchFeeds(vec![9]);
        runs.finish(&fetch, Err(&fetch.failure(&failed(Error::UnknownFeed(9)))));
        assert_eq!(runs.banner(), Some(&Banner { message: "No feed #9.".into(), show_log: true }));
    }

    #[test]
    fn only_garden_edition_feed_and_add_runs_can_be_refused() {
        let refusals: Vec<bool> = [RunRequest::Decide(9, Decision::Keep), RunRequest::Undo, RunRequest::BuildEdition { rebuild: true },
            RunRequest::SetCadence(9, None), RunRequest::SetTheme(9, None), RunRequest::MoveFeed(9, None),
            RunRequest::Add { address: "x".into(), theme: None, folder: None }, RunRequest::ApplySafe, RunRequest::FetchFeeds(vec![9]),
            RunRequest::FetchAll, RunRequest::Extract(9), RunRequest::Export { path: "/tmp/x.opml".into(), cemetery: false }]
            .iter().map(|r| r.is_refusal(&Error::UnknownFeed(9))).collect();
        assert_eq!(refusals, [true, true, false, true, true, true, true, false, false, false, false, false]);
        assert!(RunRequest::BuildEdition { rebuild: false }.is_refusal(&Error::EditionNotDue));
        assert!(!RunRequest::Decide(9, Decision::Keep).is_refusal(&busy()));
        assert!(RunRequest::Add { address: "x".into(), theme: None, folder: None }.is_refusal(&Error::InvalidAddress));
        assert!(!RunRequest::Add { address: "x".into(), theme: None, folder: None }.is_refusal(&Error::Unreachable { url: "u".into(), reason: "r".into() }));
    }

    #[test]
    fn request_kinds_and_banner_rules_follow_the_mac_table() {
        use RunKind::{Long, Short};
        let all = [RunRequest::FetchAll, RunRequest::FetchFeeds(vec![1]), RunRequest::FetchFolder(1), RunRequest::Import("/tmp/a.opml".into()),
            RunRequest::BuildEdition { rebuild: false }, RunRequest::Add { address: "x".into(), theme: None, folder: None },
            RunRequest::Export { path: "/tmp/x.opml".into(), cemetery: false }, RunRequest::Extract(1), RunRequest::Decide(1, Decision::Bury),
            RunRequest::Undo, RunRequest::ApplySafe, RunRequest::SetCadence(1, None), RunRequest::SetTheme(1, None), RunRequest::MoveFeed(1, None)];
        assert_eq!(all.iter().map(RunRequest::kind).collect::<Vec<_>>(),
                   [Long, Long, Long, Long, Long, Long, Short, Short, Short, Short, Short, Short, Short, Short]);
        assert_eq!(all.iter().filter(|r| !r.shows_failure_banner()).cloned().collect::<Vec<_>>(),
                   [RunRequest::Add { address: "x".into(), theme: None, folder: None }, RunRequest::Extract(1)]);
    }

    #[test]
    fn refresh_targets_the_current_source() {
        assert_eq!(RunRequest::refresh(&Selection::Source(Source::Feed(4))), RunRequest::FetchFeeds(vec![4]));
        assert_eq!(RunRequest::refresh(&Selection::Source(Source::Folder(2))), RunRequest::FetchFolder(2));
        for s in [Selection::Source(Source::Unread), Selection::Source(Source::Journal), Selection::Source(Source::Cadence(CadenceGroup::Rare)), Selection::Garden(GardenScope::All)] {
            assert_eq!(RunRequest::refresh(&s), RunRequest::FetchAll, "{s:?}");
        }
    }

    #[test]
    fn network_warning_survives_its_own_successful_run_and_extraction_failures_stay_out() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        runs.admit(&fetch);
        runs.event(&fetch, &Event::NetworkUnreachable { failed: 4, attempted: 5 });
        runs.finish(&fetch, Ok(()));
        assert_eq!(runs.banner(), Some(&Banner { message: "The network looks unreachable: 4 of 5 feeds could not connect.".into(), show_log: false }));
        runs.dismiss_banner();
        let extract = RunRequest::Extract(9);
        runs.finish(&extract, Err(&extract.failure(&failed(Error::Invalid("HTTP 404.".into())))));
        assert_eq!(runs.banner(), None);
    }

    #[test]
    fn database_changes_are_told_for_the_events_that_write() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        let told: Vec<bool> = [Event::Started { total: 1 }, Event::Probing { what: "https".into(), total: 1 }, Event::WaybackDone { found: 0 },
            Event::FeedDone { feed_id: 1, title: "A".into(), error: None }, Event::Extracted { entry_id: 1, ok: true, error: None },
            Event::Decided { feed_id: 1, decision: "keep".into() }, Event::Undone { feed_id: None }]
            .iter().map(|e| runs.event(&fetch, e)).collect();
        assert_eq!(told, [false, false, false, true, true, true, true]);
    }

    #[test]
    fn direct_writes_round_trip_and_failures_name_the_busy_terminal() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let store = db.store();
        let now = at("2026-10-16T09:00:00Z");
        DirectWrite::SetRead { ids: ids.clone(), read: true }.apply(&store, now).unwrap();
        assert_eq!(db.read_at(ids[0]).as_deref(), Some("2026-10-16T09:00:00.000Z"));
        DirectWrite::SetStarred { id: ids[1], starred: true }.apply(&store, now).unwrap();
        assert!(db.starred(ids[1]));
        DirectWrite::SetRead { ids: vec![ids[0]], read: false }.apply(&store, now).unwrap();
        DirectWrite::MarkAllRead { source: Source::Feed(1), today_start: today() }.apply(&store, now).unwrap();
        assert!(db.read_at(ids[0]).is_some());
        DirectWrite::PreferFullText { feed: 1, on: true }.apply(&store, now).unwrap();
        assert!(store.prefer_full_text(1).unwrap());
        assert_eq!(write_failure(&failed(busy())), Banner { message: BUSY.into(), show_log: false });
        assert_eq!(BUSY, "The database is busy (is `ressac` running in a terminal?)");
        assert_eq!(write_failure(&failed(Error::Invalid("disk full".into()))), Banner { message: "Could not save the change: disk full".into(), show_log: false });
        assert_eq!(read_failure(&Error::Invalid("gone".into())), Banner { message: "Could not read the database: gone".into(), show_log: true });
    }

    #[test]
    fn fetch_checks_its_subset_before_the_network() {
        let db = TestDb::new();
        let store = db.store();
        let only = |r| fetch_options(&store, &r).map(|o| (o.force, o.only));
        assert_eq!(only(RunRequest::FetchAll).unwrap(), (true, None));
        assert_eq!(only(RunRequest::FetchFeeds(vec![8, 1])).unwrap(), (true, Some(vec![8, 1])));
        assert_eq!(only(RunRequest::FetchFolder(1)).unwrap(), (true, Some(vec![1, 2, 3, 4, 5])), "subfolders included");
        assert_eq!(only(RunRequest::FetchFeeds(vec![99])).unwrap_err().to_string(), "No feed #99.");
        assert_eq!(only(RunRequest::FetchFolder(99)).unwrap_err().to_string(), "No folder #99.");
        assert!(only(RunRequest::Undo).is_err());
    }

    /// Review Focus 3 (master plan): `ressac fetch` in Konsole holds the write lock while the app marks
    /// an entry read. The mark shows at once, submitting never waits, the GUI connection keeps reading,
    /// and the write lands when the terminal commits (here after 1.5 s).
    #[test]
    fn a_read_mark_waits_on_the_worker_while_a_terminal_writes_and_the_list_never_blocks() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let gui = db.store();
        let mut list = EntryList::new(ListQuery { source: Source::Unread, filter: ListFilter::All, search: None });
        list.restart(&gui, None, today()).unwrap();
        let worker = Worker::new(&db.path);
        let terminal = db.hold_write_lock(Duration::from_millis(1500));
        let edit = list.set_read_local(&[ids[0]], true);
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let write = DirectWrite::SetRead { ids: vec![ids[0]], read: true };
        worker.short(move |cx| write.apply(cx.app.store(), chrono::Utc::now()), move |r| tx.send((r.map_err(|f| f.message()), start.elapsed())).unwrap());
        assert!(start.elapsed() < Duration::from_millis(50), "submitting never waits");
        let read = Instant::now();
        list.reload(&gui, today()).unwrap();
        assert!(read.elapsed() < Duration::from_millis(200), "the GUI connection reads while the terminal writes");
        assert!(list.rows()[0].is_read, "the optimistic mark holds");
        let (result, waited) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(result, Ok(()));
        assert!(waited >= Duration::from_secs(1), "{waited:?}");
        list.settle(edit);
        terminal.join().unwrap();
        assert!(db.read_at(ids[0]).is_some());
        list.reload(&gui, today()).unwrap();
        assert!(list.rows()[0].is_read);
    }

    /// Review Focus 3: still locked after the 5 s busy timeout → the row reverts and the banner names the terminal.
    #[test]
    fn a_read_mark_still_blocked_after_the_busy_timeout_reverts_with_the_busy_sentence() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let gui = db.store();
        let mut list = EntryList::new(ListQuery { source: Source::Unread, filter: ListFilter::All, search: None });
        list.restart(&gui, None, today()).unwrap();
        let worker = Worker::new(&db.path);
        let terminal = db.hold_write_lock(Duration::from_millis(6500));
        let edit = list.set_read_local(&[ids[0]], true);
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let write = DirectWrite::SetRead { ids: vec![ids[0]], read: true };
        worker.short(move |cx| write.apply(cx.app.store(), chrono::Utc::now()), move |r| tx.send((r, start.elapsed())).unwrap());
        list.reload(&gui, today()).unwrap();
        assert!(list.rows()[0].is_read, "a poll's reload during the wait keeps the mark");
        let (result, waited) = rx.recv_timeout(Duration::from_secs(15)).unwrap();
        let failure = result.unwrap_err();
        assert!(failure.is_busy(), "{}", failure.message());
        assert!(waited >= Duration::from_millis(4500), "the busy timeout was waited: {waited:?}");
        list.revert(edit);
        assert!(!list.rows()[0].is_read);
        let mut runs = RunCoordinator::default();
        runs.show_banner(write_failure(&failure));
        assert_eq!(runs.banner(), Some(&Banner { message: "The database is busy (is `ressac` running in a terminal?)".into(), show_log: false }));
        terminal.join().unwrap();
        assert_eq!(db.read_at(ids[0]), None);
    }
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui runs::` → compile errors.
- [ ] **Step 3: Implement** `runs.rs` as the header and rules above.
- [ ] **Step 4: Run them to verify they pass** — `cargo test -p ressac-ui && cargo clippy -p ressac-ui --all-targets -- -D warnings` → every `test result: ok.` (14 in `runs`; the two Review Focus tests take about 1.5 s and 6.5 s); clippy `Finished`. Then the full gate (B2b ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/runs.rs crates/ressac-ui/src/lib.rs
git commit -m "ressac-ui: run coordinator (one long run, progress texts, banners, refusals), direct writes, fetch subset

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B2c: the bridges

Every bridge follows B1's conventions (`#[auto_cxx_name]` on the `extern "RustQt"` block, custom `READ = …` getters declared in the block, `impl cxx_qt::Threading for X {}`), the spike's list-model block (`include!(<QtCore/QAbstractListModel>)`, `#[cxx_override] rowCount / data / roleNames`, `#[inherit] beginInsertRows / endInsertRows / beginRemoveRows / endRemoveRows / index`, `#[inherit] #[qsignal] dataChanged(top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32)` with `type QList_i32 = cxx_qt_lib::QList<i32>` from `cxx-qt-lib/qlist.h`), and the two gotchas of the validation section. Rows are applied only through `live::plan_rows`: `Remove` → `begin_remove_rows(&QModelIndex::default(), first, last)`, `drain(first..=last)`, `end_remove_rows`; `Insert` → `begin_insert_rows(at, at + n − 1)`, `splice(at..at, rows)`, `end_insert_rows`; `Change` → replace, then `data_changed(index(first, 0), index(first + n − 1, 0), &QList::default())`; then the derived properties' signals. Never `beginResetModel`.

### Task 7 (B2c): `Runs`, `Live`, the session's worker

**Files:**
- Create: `apps/linux/src/runs.rs` (one `#[cxx_qt::bridge]` declaring `Runs` and `Live`)
- Modify: `apps/linux/src/launcher.rs` (`Session { db, store, worker: Worker }`, built with `Worker::new(&db)` where the session is installed; remove `#[expect(dead_code, reason = "read by B2c's models")]` from `Session` and `with_session`)
- Modify: `apps/linux/src/lib.rs` (`mod runs;`)

**Interfaces:**
- Consumes: `ressac_ui::{runs::*, live::*, worker::Worker, sources::Selection}`, `ressac_ui::chrono`, `ressac_core::{clock::today_start, paths::Paths}`, `with_session`.
- Produces: the `Runs` and `Live` singletons of the header; `crate::launcher::Session::worker`.

Behaviour:
- `RunsRust { coordinator: RunCoordinator, invalidation: Invalidation, current: Option<RunRequest>, log_dir: String }` (`log_dir` from `Paths::from_env()` at `Default`, `""` when it fails). `refreshAll` / `refreshSource(key)` (`Selection::parse_key`, an unknown key does nothing) / `refreshFeed(id)` build the request (`RunRequest::refresh`, `FetchFeeds(vec![id])`), then `admit`; `Refused` stops there; `Start` stores `current`, emits `state_changed`, and calls `with_session(|s| s.worker.long(job, events, done))` with `job = |cx, sink, stop| cx.block_on(cx.app.refresh(fetch_options(cx.app.store(), &request)?, sink, stop)).map(drop)`, `events` queueing `run_event(event.clone())`, `done` queueing `run_finished(result)` (one `self.qt_thread()` per closure; the handle is `Send + Sync + Clone`). No session → the run is finished at once with nothing shown.
- `run_event`: `coordinator.event(current, &e)`; when it returns true and `invalidation.run_progress(Instant::now())` → `database_changed`; then `state_changed`.
- `run_finished(result)`: `coordinator.finish(&current, outcome)` where a failure becomes `current.failure(&f)`; `current = None`; `invalidation.run_finished(now)` → `database_changed`; `state_changed`. A failure is logged `tracing::warn!` (a refusal `info!`).
- `showBanner(message, showLog)` / `dismissBanner()` → coordinator, `state_changed`.
- Getters: `progressText` / `progressCompactText` / `progressWidth` from `progress_text` / `compact_progress_text` / `progress_width` (`""`, `""`, 140 without progress); `progressFraction` = `fraction()` or −1.
- `LiveRust { invalidation: Invalidation, window_active: bool, chart_on_screen: bool }`. `set_window_active(true)` from false → `invalidation.activated(data_version)` → `invalidated`; any change of either flag emits `polling_changed` when `should_poll` changes. `poll()`: only when `should_poll`; `with_session(|s| s.store.data_version())`; `Ok(v)` and `version_seen(v)` → `invalidated`; `Err` → `tracing::warn!("cannot read data_version: {e}")` (transient). `minuteTick()`: `day_seen(today_start(chrono::Local::now()))` → `invalidated`.

- [ ] **Step 1: Write the failing check** — `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^(error|warning)" | head -3'` after adding `worker` to `Session` but before removing the `expect` attributes. Expected: `error: this lint expectation is unfulfilled` (B1's marker: the session is now read).
- [ ] **Step 2: Implement** `runs.rs`, the `launcher.rs` and `lib.rs` changes as above.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `test result: ok.` (B1's `launch` suite unchanged: the singletons are registered, not yet used). `grep -rn "expect(dead_code" apps/linux/src` → nothing.
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/runs.rs apps/linux/src/launcher.rs apps/linux/src/lib.rs
git commit -m "ressac-kde: Runs and Live singletons, the session's worker

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 8 (B2c): `SourcesModel`, `EntriesModel`

**Files:**
- Create: `apps/linux/src/sources.rs`, `apps/linux/src/entries.rs`
- Modify: `apps/linux/src/lib.rs` (`mod entries; mod sources;`)

**Interfaces:**
- Consumes: `ressac_ui::{sources::*, entries::*, live::plan_rows, runs::{DirectWrite, write_failure, read_failure}}`, `with_session` (Task 7's `Session.worker`).
- Produces: the `SourcesModel` and `EntriesModel` singletons of the header.

Behaviour (`SourcesModel`; `SourcesModelRust { tree: SidebarTree, rows: Vec<SidebarRow>, selection: Selection }`, default selection Unread):
- `reload()`: `tree.load(store, today_start(Local::now()))`; error → `banner(read_failure(e))`; else apply `plan_rows(&rows, &tree.rows(), |r| r.key.clone())`; `selection_changed` when `selectedRow` moved.
- `select(row)`: a row with a selection different from the current one → store it, `selection_changed`. `selectKey(key)`: `parse_key`, `tree.reveal`, re-plan the rows, select. `rowOfKey(key)` → row or −1. `toggleExpanded(row)`: folder rows only, `tree.toggle`, re-plan.
- `setPreferFullText(feedId, on)`: `set_prefer_full_text_local`, re-plan, then `worker.short(DirectWrite::PreferFullText { feed, on }.apply(…))`; `done` queued back: `Ok` → `wrote`; `Err(f)` → put back the previous value, re-plan, `banner(write_failure(&f))`, `wrote`.
- Roles from `SidebarRow`: `kind` = `RowKind::as_str`, `statusColor` = `dot()` or `""`, `statusText` = status `as_str` when a dot shows, `feedId` = the feed id or 0, `site` or `""`, `selectable` = `selection().is_some()`; `isGarden` = `selection.is_garden()`.

Behaviour (`EntriesModel`; `EntriesModelRust { list: Option<EntryList>, shown: Vec<Shown>, selected: Option<i64>, unread_only: bool, search: String }` with private `struct Shown { row: ListRow, date: String }` (`PartialEq`), keyed by `entry_id`):
- `showSource(key)`: a garden key or an unknown one → `list = None`, rows planned to empty, `active` false; else `EntryList::new` (different source: selection cleared) or `change` (same source) with `ListQuery { source, filter: Unread if unread_only else All, search: trimmed non-empty search }`, `keeping = selected`; then sync. `set_unread_only` / `set_search_text` (an unchanged value does nothing, so the search field's two-way wiring cannot loop) → the same with the current source, keeping the selection.
- sync = `Shown` rows with `relative_date(row.published_at, Utc::now())`, `plan_rows`, apply; `selected` dropped when its row is gone; `selection_changed` and `list_changed` when their values moved.
- `reload()`: `list.reload(store, today)`; `Ok(true)` → queue `restart_if_due(generation)` (`restart_due` → `restart(store, selected, today)` + sync); errors → `banner(read_failure)`.
- `rowAppeared(row)`: `row_appeared(id)` → `Some(g)` → queue `load_page(g)` (`scheduled_page_due(g)` → `load_more` + sync).
- `select(row)`, `toggleRead(row)` (`set_read_local([id], !is_read)`), `toggleStarred(row)`, `markAllRead()` (`can_mark_all_read` only; `mark_all_read_local` + `DirectWrite::MarkAllRead { source, today_start }`), `markAllReadIn(key)` (another source: the write only; the current one: as `markAllRead`): local edit, sync, `worker.short(write)`, `done` queued back with the `LocalEdit`: `Ok` → `settle`, `wrote`; `Err(f)` → `revert`, sync, `banner(write_failure(&f))`, `wrote`. A done arriving after the list changed (another source) only emits its banner and `wrote`.
- Every invokable taking a row ignores a row out of range (an empty list, a stale index from QML); `SourcesModel` does the same.

- [ ] **Step 1: Implement** both files and the `lib.rs` line as above (no unit tests: the logic is `ressac-ui`'s; Task 9's walk drives these objects offscreen).
- [ ] **Step 2: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `test result: ok.`. Then the full gate (B2c ends here).
- [ ] **Step 3: Commit**

```bash
git add apps/linux/src/sources.rs apps/linux/src/entries.rs apps/linux/src/lib.rs
git commit -m "ressac-kde: SourcesModel and EntriesModel (rows by id, optimistic writes on the worker)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B2d: the reading page

### Task 9 (B2d): QML sources and entries panes, toolbar, banner, live reloads

**Files:**
- Modify: `apps/linux/qml/Main.qml` (the `Ready` content: `Loader { anchors.fill: parent; active: Launcher.phase === Launcher.Ready; source: Qt.resolvedUrl("ReadingPage.qml") }` in place of B1's placeholder; the walk steps below)
- Create: `apps/linux/qml/ReadingPage.qml`, `apps/linux/qml/SourcesPane.qml`, `apps/linux/qml/EntriesPane.qml`
- Test (not counted): `apps/linux/tests/common/mod.rs` (B1's `Run`, `assert_drawn`, `assert_clean`, `shots` moved out of `launch.rs`, which now starts with `mod common; use common::*;`; its two ready cases now assert `shots(&run)[0] == "01-ready.png"` and six shots, since a ready launch walks the reading page), `apps/linux/tests/reading.rs`

**Interfaces:**
- Consumes: Tasks 7–8 (`Runs`, `Live`, `SourcesModel`, `EntriesModel`), B1's `walkSteps` and `Recette`.
- Produces: `ReadingPage.qml` with `function reloadAll()` and the panes' ids `sourcesPane`, `entriesPane`, `readerPane` (an empty `Item`, `SplitView.fillWidth: true`, `QQC2.SplitView.minimumWidth: 300`) for B3.

`ReadingPage.qml` (root `Item`; imports `QtQuick`, `QtQuick.Layouts`, `QtQuick.Controls as QQC2`, `org.kde.kirigami as Kirigami`, `org.w3cdotorg.ressac`):
- A `QQC2.ToolBar` whose row ends with: `Kirigami.SearchField { placeholderText: "Search"; text: EntriesModel.searchText; onTextChanged: EntriesModel.searchText = text }` (Esc clears it); a segmented pair of checkable `QQC2.ToolButton`s "All" / "Unread" in a `QQC2.ButtonGroup` bound to `EntriesModel.unreadOnly`, `enabled: !SourcesModel.isGarden`; when `!Runs.progressVisible` a `QQC2.ToolButton { icon.name: "view-refresh"; text: "Refresh"; display: QQC2.AbstractButton.IconOnly; enabled: Runs.canRefresh; onClicked: Runs.refreshSource(SourcesModel.selectedKey) }`, else the pill: `QQC2.ProgressBar { width: Runs.progressWidth; indeterminate: Runs.progressFraction < 0; value: Math.max(0, Runs.progressFraction) }` with a label inset 10 px (`PROGRESS_INSET`) showing `Runs.progressText` when a `TextMetrics` of it fits `progressWidth`, else `Runs.progressCompactText`, tooltip `Runs.progressText`.
- Under it, `Kirigami.InlineMessage { type: Kirigami.MessageType.Warning; visible: Runs.bannerVisible; text: Runs.bannerMessage; showCloseButton: false; actions: [ Kirigami.Action { text: "Show Log"; visible: Runs.bannerShowLog; onTriggered: Qt.openUrlExternally("file://" + encodeURI(Runs.logDir)) }, Kirigami.Action { text: "Dismiss"; icon.name: "window-close"; displayHint: Kirigami.DisplayHint.IconOnly; onTriggered: Runs.dismissBanner() } ] }` (the message wraps to 2 lines).
- `QQC2.SplitView` (horizontal): `SourcesPane { id: sourcesPane; QQC2.SplitView.minimumWidth: 180; QQC2.SplitView.preferredWidth: 220; QQC2.SplitView.maximumWidth: 320 }`, `EntriesPane { id: entriesPane; … 260 / 320 / 480 }`, `readerPane`.
- `Timer { interval: Live.pollInterval; repeat: true; running: Live.polling; onTriggered: Live.poll() }`, `Timer { interval: 60000; repeat: true; running: true; onTriggered: Live.minuteTick() }`, `Binding { target: Live; property: "windowActive"; value: page.Window.window !== null && page.Window.window.active && page.Window.window.visibility !== Window.Minimized }`.
- `Connections`: `Live.onInvalidated`, `Runs.onDatabaseChanged`, `SourcesModel.onWrote`, `EntriesModel.onWrote` → `reloadAll()`; `SourcesModel.onBanner(m, l)`, `EntriesModel.onBanner(m, l)` → `Runs.showBanner(m, l)`; `SourcesModel.onSelectionChanged` → `EntriesModel.showSource(SourcesModel.selectedKey)`.
- `Component.onCompleted: { SourcesModel.reload(); EntriesModel.showSource(SourcesModel.selectedKey) }`.

`SourcesPane.qml`: a `ListView` on `SourcesModel`; one delegate (`QQC2.ItemDelegate`, `width: ListView.view.width`, `highlighted: ListView.isCurrentItem`, `enabled: model.selectable || model.kind === "heading"`) whose content is a heading label (`kind === "heading"`, bold, small caps off, no hover) or a `RowLayout` indented `depth * Kirigami.Units.gridUnit`: a disclosure `QQC2.ToolButton` (`icon.name: model.expanded ? "arrow-down" : "arrow-right"`, `visible: model.expandable`, `onClicked: SourcesModel.toggleExpanded(index)`), `Kirigami.Icon { source: model.icon; visible: model.icon !== "" }`, the title (`elide: Text.ElideRight`), the 7 px dot (`Rectangle { width: 7; height: 7; radius: 3.5; color: model.statusColor; visible: model.statusColor !== "" }` with `QQC2.ToolTip.text: model.statusText`), and the badge (`QQC2.Label { text: model.badge; visible: model.badge > 0 }`). `onClicked: SourcesModel.select(index)`. Selection: `Connections { target: SourcesModel; function onSelectionChanged() { list.currentIndex = SourcesModel.selectedRow } }`. Feed rows: a right-click `TapHandler` opens a `QQC2.Menu` with "Refresh" (`enabled: Runs.canRefresh`, `Runs.refreshFeed(model.feedId)`), "Always Fetch Full Article" (checkable, `checked: model.preferFullText`, `SourcesModel.setPreferFullText(model.feedId, checked)`), "Mark All as Read" (`enabled: model.badge > 0`, `EntriesModel.markAllReadIn(model.key)`), "Open Site" (`enabled: model.site !== ""`, `Qt.openUrlExternally(model.site)`).

`EntriesPane.qml`: a `ListView` on `EntriesModel` (`visible: EntriesModel.active`); delegate `QQC2.ItemDelegate` with: an 8 px dot (`color: model.read ? "transparent" : Kirigami.Theme.highlightColor`); a caption row (`model.feedTitle`, elided; a `Kirigami.Icon { source: "starred-symbolic"; visible: model.starred }`; `model.date`) in `Kirigami.Theme.smallFont` and `disabledTextColor`; the title (`maximumLineCount: 2`, `wrapMode: Text.Wrap`, `font.weight: model.read ? Font.Normal : Font.DemiBold`, `color: model.read ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.textColor`); the excerpt (2 lines, small, `disabledTextColor`). `Component.onCompleted: EntriesModel.rowAppeared(index)`; `onClicked: EntriesModel.select(index)`. `Connections { target: EntriesModel; function onSelectionChanged() { list.currentIndex = EntriesModel.selectedRow; if (list.currentIndex >= 0) list.positionViewAtIndex(list.currentIndex, ListView.Contain) } }`. `Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: EntriesModel.active && list.count === 0 && EntriesModel.emptyText !== ""; text: EntriesModel.emptyText }`.

Walk steps appended to `Main.qml`'s `walkSteps` (each `enter` ends with `console.warn("walk: <name> …")` so the test reads the state):

```qml
{ name: "02-folder-blogs", enter: function () { SourcesModel.selectKey("folder:1"); SourcesModel.toggleExpanded(SourcesModel.rowOfKey("folder:1"));
    console.warn("walk: 02-folder-blogs entries=" + EntriesModel.rowCount() + " expanded=" + (SourcesModel.rowOfKey("feed:1") >= 0)) } },
{ name: "03-starred", enter: function () { EntriesModel.select(0); EntriesModel.toggleStarred(0);
    console.warn("walk: 03-starred selected=" + EntriesModel.selectedEntryId) } },
{ name: "04-search", enter: function () { EntriesModel.searchText = "comic"; console.warn("walk: 04-search entries=" + EntriesModel.rowCount()) } },
{ name: "05-banner", enter: function () { Runs.showBanner("The database is busy (is `ressac` running in a terminal?)", false);
    console.warn("walk: 05-banner visible=" + Runs.bannerVisible) } },
{ name: "06-garden", enter: function () { EntriesModel.searchText = ""; SourcesModel.selectKey("garden:needs-triage");
    console.warn("walk: 06-garden active=" + EntriesModel.active + " unreadEnabled=" + !SourcesModel.isGarden) } },
```

- [ ] **Step 1: Write the failing test** `apps/linux/tests/reading.rs`:

```rust
//! The reading page, offscreen, through `--screenshot` (spec §7.3; inventory §1.3, §2.3, §2.4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

#[test]
fn the_reading_page_walks_sources_entries_search_banner_and_garden() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-ready.png", "02-folder-blogs.png", "03-starred.png", "04-search.png", "05-banner.png", "06-garden.png"]);
    for shot in shots(&run) {
        assert_drawn(&run.shots.join(&shot));
    }
    assert_ne!(std::fs::read(run.shots.join("02-folder-blogs.png")).unwrap(), std::fs::read(run.shots.join("04-search.png")).unwrap());
    for line in ["walk: 02-folder-blogs entries=4 expanded=true", "walk: 03-starred selected=1", "walk: 04-search entries=3",
                 "walk: 05-banner visible=true", "walk: 06-garden active=false unreadEnabled=false"] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(!err.contains("Could not read the database"), "{err}");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let starred: Vec<i64> = conn.prepare("SELECT id FROM entry WHERE starred_at IS NOT NULL ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(starred, [1, 4], "the walk starred Blogs' newest entry (1); 4 was starred already");
}
```

(The fixture: Blogs' subtree lists entries 1, 3, 4, 2 newest first; "comic" matches 1, 2, 4; the Unread list on `01-ready` is 5, 1, 6, 3, 4.)

- [ ] **Step 2: Run it to verify it fails**

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test reading 2>&1 | grep -E "panicked|test result"'`
Expected: FAIL, `shots` holds only `01-ready.png` (no walk steps yet).

- [ ] **Step 3: Implement** the three QML files, the `Main.qml` changes, and move B1's helpers into `tests/common/mod.rs`.

- [ ] **Step 4: Run it to verify it passes, and look at the screens**

Run: `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"; cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1'`
Expected: every `test result: ok.` (`launch` 5, `reading` 1); clippy `Finished`.
Run (shots in the bind mount, removed afterwards):
```bash
apps/linux/dev/dev.sh sh 'h=$(mktemp -d); cp tests/fixtures/contract.sqlite $h/r.sqlite; HOME=$h XDG_CONFIG_HOME=$h/c XDG_CACHE_HOME=$h/k XDG_STATE_HOME=$h/s RESSAC_DB=$h/r.sqlite QT_QPA_PLATFORM=offscreen "$CARGO_TARGET_DIR/debug/ressac-kde" --screenshot /work/ressac/target-shots 2>&1 | grep -c "walk:"; echo "exit=$?"'
```
Expected: `5`, `exit=0`; open `target-shots/01-ready.png` … `06-garden.png` on the Mac: three panes at 220 / 320 / rest, Breeze icons in the sidebar, badges, unread dots and bold titles, the busy banner with its dismiss button, the garden selection with an empty middle pane and "All | Unread" disabled. Then `rm -rf target-shots`.

- [ ] **Step 5: Gate, then commit.** The full gate (B2d and B2 end here). Ledger rows B2a–B2d: commits, test counts (`ressac-core` lib, `ressac-ui`, `ressac-kde` `launch` and `reading`), the Linux gate's wall time.

```bash
git add apps/linux/qml/Main.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/SourcesPane.qml apps/linux/qml/EntriesPane.qml apps/linux/tests/common/mod.rs apps/linux/tests/launch.rs apps/linux/tests/reading.rs
git commit -m "ressac-kde: reading page (sidebar, entry list, toolbar, banner, live reloads)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
