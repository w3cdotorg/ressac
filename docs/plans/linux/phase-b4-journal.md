# Phase B4: the Journal (B4a–B4c)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE window has the Mac app's Journal: the morning edition built in-process as a long run at launch, when the app becomes active and when a once-a-minute wall-clock check sees the 04:00 day move (04:00 passing, a resume from suspend, a time-zone change), with the Mac's phases, the 15-minute wait after a failure and an immediate Retry; the list shows the edition by section (Front Page, This Week, The Wire) with "+N more in Feed" rows after their feed, the header line, the placeholders and "That's the edition.".

**Runs after:** B3h (VM checkpoint 1, read by Clément). Master plan `docs/plans/2026-10-23-linux.md` (row B4; Interfaces; Global Constraints: Runs, Language, Time, Gates; Review Focus 5). Spec §7.5 (and §7.1 run rules). Inventory §3 (binding strings and numbers), §8.4, §10.1 (JournalModelTests, JournalListTests). Mac sources: `apps/macos/Ressac/App/JournalModel.swift`, `Reading/JournalList.swift`, `Data/Edition.swift`, `RessacTests/JournalModelTests.swift`, `RessacTests/JournalListTests.swift`. Terminal patterns: `crates/ressac/src/tui/reader/journal.rs` (layout over `JournalView`), `tui/reader/jobs.rs::edition_outcome`.

**Architecture:** the rules are Rust in `ressac_ui::journal`, look-agnostic so Antiope's P104 (B8) reuses them: the layout (`sections`, `header`, `placeholder`, `overflow_label`, `is_finished`), the phase machine `JournalModel` (triggers, the 15-minute wait, progress, outcomes) and the clock check (`ClockCheck`, `due_after`). `ressac_ui::runs` gains `RunEnd<T>` and `RunCoordinator::conclude`. In `ressac-kde`, `Runs` becomes the one door for every run (a `submit_long` / `submit_short` any bridge calls, through a GUI-thread handle registered when `Runs` is created); a new `Journal` QML singleton holds the phase machine and the edition's header; `EntriesModel` lays the Journal's rows out by section with overflow rows; `EntriesPane.qml` draws sections, overflow rows, header, placeholders and end line; `ReadingPage.qml` fires the triggers.

**Tech Stack:** Rust 2024 / 1.99; `chrono` 0.4.45 (`Local` re-reads `TZ` and `/etc/localtime` at most once a second per thread when the symlink's mtime changes: `offset/local/unix.rs`), cxx-qt / cxx-qt-lib 0.10.0 (`cxx_qt::Initialize`, `CxxQtThread::queue`), Qt 6.11.2 QtQuick `ListView` sections, Kirigami 6.31 (`ListSectionHeader`, `PlaceholderMessage`), Fedora 44 container.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Three sub-phases** (5-file cap): **B4a** `ressac_ui::journal` and `RunEnd` (3 files), **B4b** the bridges (5), **B4c** the QML (3). The ledger gets rows B4a–B4c.
2. **No sleep until 04:00.** The Mac's `scheduleRollover` / `nextRollover` (a task sleeping on the continuous clock) becomes spec §7.5's once-a-minute check of the wall clock: `JournalModel::clock_tick` compares the 04:00 day start of now, in the current zone, with the one recorded at the last check. The Mac's two rollover tests become `the_day_turns_at_four_local_time_across_dst` and `the_minute_tick_turns_once_at_four`. The tick rides on B2's 60 s `Timer` that already drives `Live.minuteTick()` (Today's midnight), so both use the same check.
3. **Outcomes instead of exit codes** (`Ressac::build_edition` returns `EditionOutcome`): `Built` → Idle; `NotDue` (built elsewhere meanwhile) → Idle with no banner (the Mac shows "An edition was already built this morning." then removes it: same end state); `Offline` → Offline, banner "The network is unreachable: no edition built." without Show Log (the CLI's exit-2 sentence, `commands/edition.rs::OFFLINE`); `Interrupted` (the stop flag, only at quit) → Failed("Interrupted."), as the terminal reader's `edition_outcome`. A job failure → Failed(its message). The Mac's `sawNetworkUnreachable` flag is not needed.
4. **`Runs` gets a generic door** (`runs::submit_long`, `runs::submit_short`): the Journal (here) and the garden (B5) start their runs through the one `RunCoordinator`, so one long run at a time, banners and reloads stay the Mac's. The GUI-thread handle is registered in `Runs`'s `cxx_qt::Initialize` (validated below). B2's fetch path is rewritten on top of it.
5. **The Journal list is `EntriesModel`**, not a second list model: in the Journal its rows become slots (an entry, or an overflow row) carrying a `section` role, and `EntriesPane.qml` turns `ListView` sections on. Selection, `j`/`k`/Space, auto-read and the reader keep working on entries (overflow rows are never selected). The `Journal` singleton holds the phase and its own copy of the edition for the header, placeholder and end line; `EntriesModel` reads the edition for the sections. Both reload in `reloadAll()` (the Mac also observes the edition and the list separately and merges them in the view).
6. **Recette:** in `--screenshot` mode the automatic edition triggers are off (the fixture's feeds are real hosts; tests never touch the network), except in the `journal` walk chosen by the new `RESSAC_RECETTE_WALK` variable (`Recette.walk`), whose test points every active feed at a closed loopback port. `Main.qml` walks `journalSteps` for it, `walkSteps` otherwise.
7. **Focus:** as on the Mac (inventory §3, acceptance fix), the list takes the keyboard when the Journal is selected.
8. **Mirrored elsewhere, not repeated:** JournalListTests' progress texts, compact text and inset (B2 `runs` tests), refresh scope (B2 `RunRequest::refresh`), rank order and rows kept after reading (B2 `entries`), rhythm lists and counts (B2 `sources`).

## Validated in a throwaway container (`ressac-b45-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A cxx-qt 0.10.0 crate (two `#[qml_singleton]` objects, offscreen), all checks passed:
- `impl cxx_qt::Initialize for Hub {}` in the bridge plus `impl cxx_qt::Initialize for qobject::Hub { fn initialize(self: Pin<&mut Self>) { … } }` runs when QML first touches the singleton; storing `self.qt_thread()` in a `thread_local! { static HUB: RefCell<Option<CxxQtThread<qobject::Hub>>> }` works;
- an invokable of a second singleton calling a free function that does `HUB.with(|h| h.borrow().as_ref().map(|t| t.queue(move |hub| hub.start(job, done))))` reaches the hub on the next turn; `start` is a **generic** method of `impl qobject::Hub` (`fn start<T: Send + 'static>(self: Pin<&mut Self>, job: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static)`), its thread result came back through the hub's own `qt_thread().queue` and then the client's (log: `hub finished on GUI thread runs=1`, client phase `done 42`);
- `ListView { section.property: journal ? "section" : "" }` draws headers from a string role and turns them off when bound to `""`; a `QQC2.Dialog` with a custom `DialogButtonBox` opened offscreen with the focus inside it (used by B5).

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-ui/src/journal.rs` (new) | B4a T1, T2 | layout, texts, `Phase`, `JournalModel`, `ClockCheck`, `due_after`, `is_due`, `edition_failure` |
| `crates/ressac-ui/src/lib.rs` | B4a T1 | `pub mod journal;` |
| `crates/ressac-ui/src/runs.rs` | B4a T2 | `RunEnd<T>`, `RunCoordinator::conclude` |
| `apps/linux/src/runs.rs` | B4b T3 | `Initialize` registration, `submit_long`, `submit_short`, generic `start_long` / `start_short`; fetch rewritten on them |
| `apps/linux/src/journal.rs` (new) | B4b T4 | QML singleton `Journal` |
| `apps/linux/src/lib.rs` | B4b T4 | `mod journal;` |
| `apps/linux/src/recette.rs` | B4b T4 | `Recette.walk` (`RESSAC_RECETTE_WALK`) |
| `apps/linux/src/entries.rs` | B4b T5 | Journal slots, roles `kind`, `section`, `overflowFeed`, property `journal` |
| `apps/linux/qml/EntriesPane.qml` | B4c T6 | sections, overflow rows, header, placeholders, end line |
| `apps/linux/qml/ReadingPage.qml` | B4c T6 | triggers (launch, activation, minute), `Journal.reload()` in `reloadAll()`, focus on the Journal |
| `apps/linux/qml/Main.qml` | B4c T6 | `steps` chooser, `journalSteps`, step `23-journal` |
| tests (not counted) | | `crates/ressac-ui/src/test_db.rs` (`Paris`, `TestDb::built`), `apps/linux/tests/journal.rs` |

Files per sub-phase: B4a 3, B4b 5, B4c 3.

## Interfaces produced (B5 and B8 rely on these names)

**`ressac_ui::runs`** additions (Task 2):

```rust
/// How a run ended for the one who asked (the Mac's `RunOutcome`).
#[derive(Debug, Clone, PartialEq)]
pub enum RunEnd<T> { Done(T), Refused, Failed(RunFailure) }
impl RunCoordinator {
    /// `finish` for a job's own result: a job failure, or a value `failure_of` reads as one
    /// (an offline edition), sets the banner as `finish` does; the value goes back as `Done`.
    pub fn conclude<T>(&mut self, request: &RunRequest, result: JobResult<T>,
        failure_of: impl FnOnce(&T) -> Option<RunFailure>) -> RunEnd<T>;
}
```

**`ressac_ui::journal`** (Tasks 1–2):

```rust
pub const BUILDING: &str = runs::EDITION_LABEL;                    // "Building this morning's edition…"
pub const END_LINE: &str = "That's the edition.";
pub const EMPTY_LINE: &str = "Nothing new this morning.";
pub const NO_EDITION: &str = "No edition yet";
pub const OFFLINE: &str = "Offline: no edition yet";
pub const RETRY: &str = "Retry";
pub const OFFLINE_REFUSAL: &str = "The network is unreachable: no edition built.";
pub const INTERRUPTED: &str = "Interrupted.";
pub const AUTOMATIC_RETRY_DELAY: TimeDelta = TimeDelta::minutes(15);
pub const SECTIONS: [Section; 3] = [Section::Front, Section::Week, Section::Wire];
pub fn section_title(section: Section) -> &'static str;           // "Front Page", "This Week", "The Wire"
#[derive(Debug, Clone, PartialEq)]
pub enum JournalRow { Entry(i64), Overflow(EditionOverflowRow) }
impl JournalRow { pub fn id(&self) -> String; }                  // "entry-<id>", "overflow-<section as_str>-<feed>"
#[derive(Debug, Clone, PartialEq)]
pub struct JournalSection { pub section: Section, pub rows: Vec<JournalRow> }
/// `listed`: (entry id, feed id) in list order (the Native list's rows, or P104's edition items).
pub fn sections(listed: &[(i64, FeedId)], view: Option<&JournalView>) -> Vec<JournalSection>;
pub fn header<Tz: TimeZone>(view: &JournalView, tz: &Tz) -> String where Tz::Offset: Display;
pub fn read_count(view: &JournalView) -> usize;
pub fn is_finished(view: &JournalView) -> bool;                   // ≥ 1 item and all read
pub fn overflow_label(row: &EditionOverflowRow) -> String;        // "+12 more in Actualitté"
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase { #[default] Idle, Due, Building { done: usize, total: usize }, Offline, Failed(String) }
impl Phase { pub fn as_str(&self) -> &'static str; pub fn offers_retry(&self) -> bool; } // "idle" "due" "building" "offline" "failed"
pub fn placeholder(phase: &Phase, view: Option<&JournalView>) -> Option<String>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockCheck { pub day_start: DateTime<Utc> }           // the 04:00 day start seen at a check
impl ClockCheck { pub fn at<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> ClockCheck; }
pub fn due_after<Tz: TimeZone>(last_check: &ClockCheck, now: DateTime<Utc>, tz: &Tz) -> bool;
pub fn is_due<Tz: TimeZone>(store: &Store, now: DateTime<Utc>, tz: &Tz) -> ressac_core::Result<bool>;
pub fn edition_failure(outcome: &EditionOutcome) -> Option<RunFailure>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Trigger { Automatic, Retry }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Step { Nothing, Build }
#[derive(Debug, Clone, Default)]
pub struct JournalModel { /* phase, busy, not_before: Option<DateTime<Utc>>, clock: Option<ClockCheck> */ }
impl JournalModel {
    pub fn phase(&self) -> &Phase;
    pub fn is_busy(&self) -> bool;
    pub fn begin<Tz: TimeZone>(&mut self, trigger: Trigger, now: DateTime<Utc>, tz: &Tz) -> bool;
    pub fn checked(&mut self, due: ressac_core::Result<bool>) -> Step;
    pub fn event(&mut self, event: &Event);
    pub fn finished(&mut self, end: RunEnd<EditionOutcome>, now: DateTime<Utc>);
    pub fn clock_tick<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> bool;
}
```

**`ressac-kde`** (Tasks 3–6):

- **`crate::runs`** (Rust, GUI thread): `pub(crate) fn submit_long<T: Send + 'static>(request: RunRequest, job: impl FnOnce(&JobContext<'_>, &dyn EventSink, Arc<AtomicBool>) -> ressac_core::Result<T> + Send + 'static, failure_of: fn(&T) -> Option<RunFailure>, events: impl Fn(&Event) + Send + Sync + 'static, end: impl FnOnce(RunEnd<T>) + Send + 'static)` and `pub(crate) fn submit_short<T: Send + 'static>(request: RunRequest, job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static, end: impl FnOnce(RunEnd<T>) + Send + 'static)`. `events` runs on the worker thread; `end` runs on the GUI thread inside `Runs`'s turn (it must `queue` onto its own object, never touch `Runs`). No `Runs` yet → `end(RunEnd::Refused)` at once and `tracing::error!`.
- **`Journal`** (QML singleton): properties (`READ`, `NOTIFY = state_changed`) `phase: QString` (`Phase::as_str`), `building: bool`, `placeholder: QString` (`""` = show the list), `offersRetry: bool`, `header: QString` (`""` without an edition), `finished: bool`; `endLine: QString` and `retryTitle: QString` (`CONSTANT`); invokables `reload()`, `trigger()`, `minuteTick()`, `retry()`; signals `stateChanged()`, `banner(message: QString, showLog: bool)`.
- **`Recette.walk: QString`** (`CONSTANT`; `RESSAC_RECETTE_WALK` when `--screenshot` is given, else `""`).
- **`EntriesModel`** additions: roles `kind` (`"entry"` | `"overflow"`), `section` (the section title in the Journal, else `""`), `overflowFeed` (`i64`, 0 for entries; an overflow row's `title` role is its label); property `journal: bool` (`NOTIFY = list_changed`).
- **QML:** `EntriesPane.qml` draws the Journal when `EntriesModel.journal`; `ReadingPage.qml` has `readonly property bool editionTriggers`; `Main.qml` has `readonly property var journalSteps` and `readonly property var steps` (the walker reads `steps`; B5 adds `gardenSteps` to the chooser).

**Consumed:** B2's `RunCoordinator`, `RunRequest::BuildEdition`, `RunFailure`, `Banner`, `EDITION_LABEL`, `COULD_NOT_READ`, `read_failure`, `DirectWrite::MarkAllRead`, `live::Invalidation::day_seen`, `entries::EntryList`, `worker::{Worker, JobContext, JobResult, JobFailure}`, `Runs` / `Live` bridges, `SourcesModel.selectKey`, `ReadingPage.reloadAll()`, the 60 s `Timer`, `tests/common` (`Run`, `copy_fixture`, `launch`, `launch_with`, `assert_clean`, `assert_drawn`, `shots`); B3's `EntryList::next_unread`, `ReadingPage.focusName()`, `entriesPane.focusList()`, `EntriesModel` row invokables (`select`, `toggleRead`, `toggleStarred`, `rowAppeared`, `selectedRow`), walk steps up to `22-text-size`; core `Store::{journal, edition_due, sources}`, `JournalView`, `JournalItem`, `EditionRecord`, `EditionOverflowRow`, `edition::{Section, day_start}`, `clock::today_start`, `service::{EditionOptions, EditionOutcome, EditionReport, Event, EventSink, Ressac::build_edition}`.

**Decisions this phase pins:**

- **Triggers.** Launch (`ReadingPage`'s `Component.onCompleted`), the application becoming active (`Qt.application.state === Qt.ApplicationActive`) and the minute tick (`Journal.minuteTick()` → `JournalModel::clock_tick(Utc::now(), &Local)`) all call the same `begin(Trigger::Automatic, …)`; Retry calls `begin(Trigger::Retry, …)`. Only one check or build at a time (`busy` from `begin` until `checked` says Nothing or `finished`). Every `begin` records the clock check, so the minute tick fires once per 04:00 day.
- **Due check on the GUI thread** (`is_due` on `Session.store`, one indexed query, as the Mac's `database.reader.read`), then the build as a long run: `Ressac::build_edition(EditionOptions::default(), Utc::now(), &chrono::Local, sink, stop)`. A refused admission (a fetch is running) → Due, never queued.
- **15-minute wait:** after Offline or Failed, automatic triggers before `not_before` do nothing; a `not_before` more than 15 minutes ahead means the clock went back and is ignored; Retry never waits; finding the edition not due, or a build that ends any other way, clears the wait. In memory only.
- **Time:** every bridge call passes `Utc::now()` and `chrono::Local` (never a cached offset), so a zone change is seen at the next tick.
- **Network in tests:** none. Recette disables the automatic triggers outside the `journal` walk; that walk's feeds point at `http://127.0.0.<id>:9/feed` (distinct hosts, so the 1 s per-host gap never applies; port 9 refuses at once), which makes the build Offline with no traffic leaving the container.

**Step 0:** `apps/linux/src/runs.rs` (B2c, 432 lines) and `apps/linux/src/entries.rs` (B2c + B3e, over 300) are restructured in Tasks 3 and 5: each task starts by removing that file's dead code (unused items, `dbg!`, stray `eprintln!`, commented-out code) in its own commit, or by recording "no dead code" in the ledger when `cargo clippy` and a read find none. `ressac-ui/src/runs.rs` (981 lines) only gains items.

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

- **Runs:** one long run at a time (fetch, import, edition build, add), a second refused with the Mac's sentence, never queued; short runs FIFO; progress reloads at most once per second.
- **Language:** UI strings in English and identical to the Mac app's (inventory §3 quotes them: "Building this morning's edition… 34/156", "Offline: no edition yet", "No edition yet", "Nothing new this morning.", "That's the edition.", "+12 more in Actualitté", "Retry", header `Friday 16 October · 43 entries · 12 read · 3 feeds unreachable` with singulars).
- **Time:** functions depending on now or the zone take them as parameters; the Journal day starts at 04:00 local (`edition::day_start`).
- **Never:** the real database; the network in tests; push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **Suspend across 04:00, or a time-zone change while running** (master Review Focus 5) → the minute check builds at the next tick after resume and after a zone change; "Today" moves to the new midnight. Tests: Task 2 `due_after_notices_a_resume_across_four_and_a_zone_change`, `a_zone_change_while_running_makes_the_edition_due_at_the_next_tick`, `today_moves_to_the_new_midnight_after_a_zone_change`.
2. **Launch, activation and the minute tick firing together, or Retry pressed during a build** → one due check, one build. Test: Task 2 `a_second_trigger_during_a_build_does_nothing`.
3. **The edition comes due while a fetch runs** (Ctrl+R just before 04:00, or a launch during a refresh started elsewhere) → refused, Due with Retry, never queued; the next trigger builds. Tests: Task 2 `a_build_turned_away_by_another_long_run_stays_due`; the coordinator's refusal is B2's `long_run_is_refused_while_another_is_active`.
4. **Offline at launch** (laptop without Wi-Fi) → Offline with Retry and the CLI's banner, no edition written, no build every minute or every activation for 15 minutes, a clock set back expires the wait. Tests: Task 2 `automatic_triggers_wait_fifteen_minutes_after_a_failure`, `a_clock_moved_backwards_expires_the_window`, `an_offline_refusal_keeps_its_banner`; Task 6 `an_offline_build_shows_retry_and_the_banner_and_writes_nothing`.
5. **A feed buried after the build, the unread filter or a search while in the Journal** → its entries leave every Journal surface, overflow rows stay right after their feed's last listed entry (or close the section), the header still counts the whole edition, an edition whose every feed was buried reads "Nothing new this morning.". Tests: Task 1 `entries_go_under_their_section_and_overflow_follows_its_feed`, `a_feed_buried_after_the_build_leaves_every_journal_surface`, `an_edition_whose_every_feed_was_buried_reads_as_empty`.

---

## B4a: `ressac_ui::journal`

### Task 1 (B4a): layout and texts

**Files:**
- Create: `crates/ressac-ui/src/journal.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod journal;`)
- Test helper (not counted): `crates/ressac-ui/src/test_db.rs` gains `pub struct Paris` (an exact copy of core `edition.rs`'s test zone: CEST from 2026-03-29 01:00 UTC to 2026-10-25 01:00 UTC, CET otherwise, `impl TimeZone`), and `TestDb::built(&self, built_at: &str) -> i64` (a new latest edition with that `built_at`, `window_start`/`window_end` as `edition()` writes them, no entries) and `TestDb::overflow(&self, edition: i64, feed: i64, section: &str, hidden: u32)`.

**Interfaces:**
- Consumes: core `JournalView`, `JournalItem`, `EditionRecord`, `EditionOverflowRow`, `EntryRow`, `Section`; B2 `runs::EDITION_LABEL`, `DirectWrite::MarkAllRead`, `entries::EntryList`; B3 `EntryList::next_unread`.
- Produces: the layout and text items of the header (`SECTIONS`, `section_title`, `JournalRow`, `JournalSection`, `sections`, `header`, `read_count`, `is_finished`, `overflow_label`, `Phase`, `placeholder`, the text constants).

Rules (`JournalList.swift:24-75`, `tui/reader/journal.rs:58-141`): `sections` keeps, for each of `SECTIONS`, the `listed` entries whose id is in `view.items` with that section, in `listed` order; then each overflow row of that section (in `view.overflow` order) goes right after the last listed entry of its feed, at the section's end when none is listed; sections without rows are omitted; no view → empty. `header`: `built_at` in `tz` formatted `"%A %-d %B"`, then `"1 entry"` / `"N entries"`, `"N read"`, and `"1 feed unreachable"` / `"N feeds unreachable"` when `unreachable > 0`, joined by `" · "`. `placeholder`: Building → `BUILDING` or `"{BUILDING} {done}/{total}"` when `total > 0`; Offline → `OFFLINE`; Failed(m) → m; Idle and Due → `NO_EDITION` without a view, `EMPTY_LINE` with an empty one, else `None`. `offers_retry`: Offline, Failed, Due.

- [ ] **Step 1: Write the failing tests** in `journal.rs` (`fn at(s: &str) -> DateTime<Utc>`; `fn view(items: &[(i64, Section, u32, FeedId)], overflow: &[(Section, FeedId, u32, &str)], read: &[i64], unreachable: u32) -> JournalView` building a `JournalView` built at `2026-10-16T05:12:00Z` whose items' `EntryRow`s have title `"Entry <id>"`, feed title `"Feed <feed>"`, `is_read` from `read`):

```rust
use Section::{Front, Week, Wire};

#[test]
fn entries_go_under_their_section_and_overflow_follows_its_feed() {
    let v = view(&[(1, Front, 1, 5), (2, Week, 2, 3), (3, Week, 3, 3), (4, Week, 4, 4), (5, Wire, 5, 1)],
                 &[(Week, 3, 2, "Feed 3"), (Wire, 1, 12, "Feed 1"), (Wire, 9, 4, "Feed 9")], &[], 0);
    let s = sections(&[(1, 5), (2, 3), (3, 3), (4, 4), (5, 1)], Some(&v));
    assert_eq!(s.iter().map(|s| s.section).collect::<Vec<_>>(), [Front, Week, Wire]);
    let ids: Vec<Vec<String>> = s.iter().map(|s| s.rows.iter().map(JournalRow::id).collect()).collect();
    assert_eq!(ids, [vec!["entry-1"], vec!["entry-2", "entry-3", "overflow-week-3", "entry-4"],
        vec!["entry-5", "overflow-wire-1", "overflow-wire-9"]]);           // feed 9 has no row: its overflow closes the section
    // A filtered list (entry 3 hidden by the unread filter): the overflow follows feed 3's last listed entry.
    let filtered = sections(&[(1, 5), (2, 3), (4, 4), (5, 1)], Some(&v));
    assert_eq!(filtered[1].rows.iter().map(JournalRow::id).collect::<Vec<_>>(), ["entry-2", "overflow-week-3", "entry-4"]);
}

#[test]
fn a_section_without_rows_is_omitted() {
    let v = view(&[(2, Week, 1, 3)], &[], &[], 0);
    assert_eq!(sections(&[(2, 3)], Some(&v)).iter().map(|s| s.section).collect::<Vec<_>>(), [Week]);
    assert!(sections(&[(2, 3)], None).is_empty());
    assert!(sections(&[(7, 3)], Some(&v)).is_empty(), "an entry outside the edition is not the Journal's");
    assert_eq!(SECTIONS.map(section_title), ["Front Page", "This Week", "The Wire"]);
}

#[test]
fn retry_is_offered_whenever_the_edition_could_not_be_built() {
    for phase in [Phase::Offline, Phase::Failed("Database is locked.".into()), Phase::Due] { assert!(phase.offers_retry(), "{phase:?}"); }
    for phase in [Phase::Idle, Phase::Building { done: 1, total: 2 }] { assert!(!phase.offers_retry(), "{phase:?}"); }
    assert_eq!([Phase::Idle, Phase::Due, Phase::Building { done: 0, total: 0 }, Phase::Offline, Phase::Failed(String::new())].map(|p| p.as_str()),
               ["idle", "due", "building", "offline", "failed"]);
}

#[test]
fn the_header_counts_entries_read_entries_and_unreachable_feeds() {
    let paris = FixedOffset::east_opt(2 * 3600).unwrap();
    let items = [(1, Front, 1, 1), (2, Wire, 2, 2), (3, Wire, 3, 3)];
    assert_eq!(header(&view(&items, &[], &[2], 0), &paris), "Friday 16 October · 3 entries · 1 read");
    assert_eq!(header(&view(&items, &[], &[], 3), &paris), "Friday 16 October · 3 entries · 0 read · 3 feeds unreachable");
    assert_eq!(header(&view(&[(1, Front, 1, 1)], &[], &[], 1), &paris), "Friday 16 October · 1 entry · 0 read · 1 feed unreachable");
}

#[test]
fn placeholders_say_what_the_journal_is_waiting_for() {
    let full = view(&[(1, Front, 1, 1)], &[], &[], 0);
    let empty = view(&[], &[], &[], 0);
    assert_eq!(placeholder(&Phase::Building { done: 0, total: 0 }, Some(&full)).as_deref(), Some("Building this morning's edition…"));
    assert_eq!(placeholder(&Phase::Building { done: 34, total: 156 }, None).as_deref(), Some("Building this morning's edition… 34/156"));
    assert_eq!(placeholder(&Phase::Offline, Some(&full)).as_deref(), Some("Offline: no edition yet"));
    assert_eq!(placeholder(&Phase::Failed("error: disk full".into()), None).as_deref(), Some("error: disk full"));
    assert_eq!(placeholder(&Phase::Idle, Some(&empty)).as_deref(), Some("Nothing new this morning."));
    assert_eq!(placeholder(&Phase::Idle, None).as_deref(), Some("No edition yet"));
    assert_eq!(placeholder(&Phase::Idle, Some(&full)), None);
    assert_eq!(placeholder(&Phase::Due, Some(&full)), None);
    assert_eq!(END_LINE, "That's the edition.");
    assert!(!is_finished(&empty), "an empty edition is never finished");
    assert!(is_finished(&view(&[(1, Front, 1, 1)], &[], &[1], 0)));
}

#[test]
fn overflow_rows_name_the_feed() {
    let row = |hidden, title: &str| EditionOverflowRow { section: Wire, feed_id: 4, feed_title: title.into(), page: None, hidden_count: hidden };
    assert_eq!(overflow_label(&row(12, "Actualitté")), "+12 more in Actualitté");
    assert_eq!(overflow_label(&row(1, "Fabula")), "+1 more in Fabula");
}

#[test]
fn a_feed_buried_after_the_build_leaves_every_journal_surface() {
    let db = TestDb::new();
    let buried = db.entry(1, "2026-10-15T08:00:00.000Z", false);
    let first = db.entry(2, "2026-10-15T08:00:00.000Z", false);
    let second = db.entry(8, "2026-10-15T08:00:00.000Z", false);
    db.edition(&[(buried, "front"), (first, "wire"), (second, "wire")]);
    db.execute("UPDATE feed SET state = 'buried' WHERE id = 1");
    let store = db.store();
    let v = store.journal().unwrap().unwrap();
    assert_eq!(v.items.iter().map(|i| i.row.entry_id).collect::<Vec<_>>(), [first, second]);
    assert_eq!(store.sources(today()).unwrap().counts.journal, 2);
    assert!(!is_finished(&v) && read_count(&v) == 0);
    DirectWrite::MarkAllRead { source: Source::Journal, today_start: today() }.apply(&store, at("2026-10-16T09:00:00Z")).unwrap();
    assert!(is_finished(&store.journal().unwrap().unwrap()));
    assert_eq!(db.read_at(buried), None, "the buried feed's entry is left alone");
}

#[test]
fn an_edition_whose_every_feed_was_buried_reads_as_empty() {
    let db = TestDb::new();
    let only = db.entry(1, "2026-10-15T08:00:00.000Z", false);
    db.edition(&[(only, "wire")]);
    db.execute("UPDATE feed SET state = 'buried' WHERE id = 1");
    let v = db.store().journal().unwrap().unwrap();
    assert!(v.items.is_empty() && !is_finished(&v));
    assert_eq!(placeholder(&Phase::Idle, Some(&v)).as_deref(), Some("Nothing new this morning."));
}

#[test]
fn mark_all_read_in_the_journal_leaves_the_overflow_unread() {
    let db = TestDb::new();
    let first = db.entry(1, "2026-10-15T08:00:00.000Z", false);
    let second = db.entry(2, "2026-10-15T08:00:00.000Z", false);
    let left_out = db.entry(1, "2026-10-14T08:00:00.000Z", false);
    let edition = db.edition(&[(first, "wire"), (second, "wire")]);
    db.overflow(edition, 1, "wire", 1);
    DirectWrite::MarkAllRead { source: Source::Journal, today_start: today() }.apply(&db.store(), at("2026-10-16T09:00:00Z")).unwrap();
    assert!(db.read_at(first).is_some() && db.read_at(second).is_some());
    assert_eq!(db.read_at(left_out), None);
}

#[test]
fn space_stays_inside_the_edition_in_rank_order() {
    let db = TestDb::new();
    let first = db.entry(1, "2026-10-15T06:00:00.000Z", false);
    let second = db.entry(2, "2026-10-15T09:00:00.000Z", false);
    db.entry(8, "2026-10-15T12:00:00.000Z", false);                    // newer, unread, not in the edition
    db.edition(&[(first, "front"), (second, "wire")]);
    let store = db.store();
    let mut list = EntryList::new(ListQuery { source: Source::Journal, filter: ListFilter::All, search: None });
    list.restart(&store, None, today()).unwrap();
    assert_eq!(list.next_unread(&store, Some(first), today()).unwrap(), Some(second));
    assert_eq!(list.next_unread(&store, Some(second), today()).unwrap(), None);
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui journal 2>&1 | tail -3` → compile error (`journal` missing).
- [ ] **Step 3: Implement** the layout and text items of `journal.rs` and the `lib.rs` line.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui journal 2>&1 | grep "test result"` → `ok.`; `cargo clippy -p ressac-ui --all-targets -- -D warnings 2>&1 | tail -1` → `Finished`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/journal.rs crates/ressac-ui/src/lib.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: Journal layout (sections, overflow rows, header, placeholders)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B4a): phases, the 15-minute wait, the clock check

**Files:**
- Modify: `crates/ressac-ui/src/journal.rs` (`ClockCheck`, `due_after`, `is_due`, `edition_failure`, `Trigger`, `Step`, `JournalModel`)
- Modify: `crates/ressac-ui/src/runs.rs` (`RunEnd<T>`, `RunCoordinator::conclude`, with tests in its `mod tests`)

**Interfaces:**
- Consumes: Task 1; B2 `RunCoordinator::{admit, event, finish, banner}`, `RunRequest::failure`, `RunFailure`, `Banner`, `COULD_NOT_READ`, `worker::{JobFailure, JobResult}`, `live::Invalidation`; core `day_start`, `clock::today_start`, `Store::edition_due`, `EditionOutcome`, `EditionReport`, `Event`.
- Produces: the phase and clock items of the header; `RunEnd`, `conclude`.

Rules (`JournalModel.swift:51-157`):
- `ClockCheck::at(now, tz)` = `day_start(now.with_timezone(tz))`; `due_after(last, now, tz)` = `ClockCheck::at(now, tz) != *last`; `clock_tick(now, tz)` = no check recorded yet, or `due_after`.
- `begin(trigger, now, tz)`: records `clock = ClockCheck::at(now, tz)`; busy → false; `Automatic` with `not_before = Some(nb)`, `now < nb` and `nb - now <= AUTOMATIC_RETRY_DELAY` → false; else busy, true.
- `checked`: `Ok(false)` → Idle, `not_before` cleared, not busy, `Nothing`; `Ok(true)` → `Building { 0, 0 }`, `Build`; `Err(e)` → `Failed(format!("{COULD_NOT_READ}{e}"))`, not busy, `Nothing`.
- `event` (only while Building): `Started { total }` → `Building { 0, total }`; `FeedDone` → `done + 1`.
- `finished(end, now)`: not busy; `Done(Built(_))` and `Done(NotDue)` → Idle; `Done(Offline)` → Offline; `Done(Interrupted)` → `Failed(INTERRUPTED)`; `Refused` → Due; `Failed(f)` → `Failed(f.message)`; then `not_before = Some(now + AUTOMATIC_RETRY_DELAY)` when Offline or Failed, else `None`.
- `edition_failure`: `Offline` → `RunFailure { message: OFFLINE_REFUSAL, refusal: true }`; `Interrupted` → `RunFailure { message: INTERRUPTED, refusal: false }`; `Built`, `NotDue` → `None`.
- `conclude`: `Err(f)` → `failure = request.failure(&f)`, `finish(request, Err(&failure))`, `Failed(failure)`; `Ok(v)` → `finish(request, failure_of(&v).as_ref().map_or(Ok(()), Err))`, `Done(v)`.

- [ ] **Step 1: Write the failing tests.** In `journal.rs` (`use crate::test_db::{Paris, TestDb, today}`; `fn built() -> EditionOutcome` = `Built(EditionReport { id: 1, entries: 0, front: 0, week: 0, wire: 0, overflow: 0, unreachable: 0 })`; `fn failed(m: &str) -> RunEnd<EditionOutcome>` = `RunEnd::Failed(RunFailure { message: m.into(), refusal: false })`; `fn mins(n: i64) -> TimeDelta`; `const T0: &str = "2026-10-16T07:30:00Z"`):

```rust
/// Master Review Focus 5.
#[test]
fn due_after_notices_a_resume_across_four_and_a_zone_change() {
    let night = ClockCheck::at(at("2026-10-15T23:30:00Z"), &Paris);            // 01:30 Paris
    assert!(!due_after(&night, at("2026-10-16T01:59:00Z"), &Paris));           // 03:59
    assert!(due_after(&night, at("2026-10-16T05:30:00Z"), &Paris));            // asleep six hours, awake at 07:30
    let morning = ClockCheck::at(at("2026-10-16T03:00:00Z"), &Paris);          // 05:00 Paris
    assert!(!due_after(&morning, at("2026-10-16T09:00:00Z"), &Paris));         // six hours that do not cross 04:00
    let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
    assert!(due_after(&morning, at("2026-10-16T03:01:00Z"), &tokyo));          // the zone changed: 12:01 in Tokyo, another day start
    assert!(!due_after(&ClockCheck::at(at("2026-10-16T03:01:00Z"), &tokyo), at("2026-10-16T03:02:00Z"), &tokyo));
}

#[test]
fn a_zone_change_while_running_makes_the_edition_due_at_the_next_tick() {
    let db = TestDb::new();
    db.built("2026-10-15T20:30:00.000Z");                                   // 05:30 in Tokyo, Friday's edition there
    let store = db.store();
    let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
    let mut m = JournalModel::default();
    assert!(m.begin(Trigger::Automatic, at("2026-10-16T05:59:00Z"), &tokyo));
    assert_eq!(m.checked(is_due(&store, at("2026-10-16T05:59:00Z"), &tokyo)), Step::Nothing);
    assert!(m.clock_tick(at("2026-10-16T06:00:00Z"), &Paris), "moved to Paris: 08:00 there, another 04:00");
    assert!(m.begin(Trigger::Automatic, at("2026-10-16T06:00:00Z"), &Paris));
    assert_eq!(m.checked(is_due(&store, at("2026-10-16T06:00:00Z"), &Paris)), Step::Build);
}

#[test]
fn today_moves_to_the_new_midnight_after_a_zone_change() {
    let mut seen = Invalidation::default();
    assert!(!seen.day_seen(today_start(at("2026-10-16T06:00:00Z").with_timezone(&Paris))));
    assert!(!seen.day_seen(today_start(at("2026-10-16T06:01:00Z").with_timezone(&Paris))));
    let los_angeles = FixedOffset::west_opt(7 * 3600).unwrap();                // still 15 October there
    assert!(seen.day_seen(today_start(at("2026-10-16T06:02:00Z").with_timezone(&los_angeles))));
}

#[test]
fn the_day_turns_at_four_local_time_across_dst() {
    let tick = |last: &str, now: &str| due_after(&ClockCheck::at(at(last), &Paris), at(now), &Paris);
    assert!(tick("2026-10-25T02:59:00Z", "2026-10-25T03:00:00Z"));            // 25 October: 04:00 Paris is 03:00 UTC
    assert!(!tick("2026-10-25T02:58:00Z", "2026-10-25T02:59:00Z"));
    assert!(tick("2026-03-29T01:59:00Z", "2026-03-29T02:00:00Z"));            // 29 March: 04:00 Paris is 02:00 UTC
    assert!(!tick("2026-03-29T01:58:00Z", "2026-03-29T01:59:00Z"));
    assert!(tick("2026-10-17T01:59:00Z", "2026-10-17T02:00:00Z"));            // summer time: 02:00 UTC
}

#[test]
fn the_minute_tick_turns_once_at_four() {
    let mut m = JournalModel::default();
    let mut now = at(T0);                                                    // 09:30 Paris
    assert!(m.begin(Trigger::Automatic, now, &Paris));
    m.checked(Ok(false));
    let mut turned = Vec::new();
    while now < at("2026-10-17T07:30:00Z") {
        now += mins(1);
        if m.clock_tick(now, &Paris) {
            turned.push(now);
            assert!(m.begin(Trigger::Automatic, now, &Paris));
            m.checked(Ok(false));
        }
    }
    assert_eq!(turned, [at("2026-10-17T02:00:00Z")]);
}

#[test]
fn an_edition_built_after_four_is_not_due_and_one_built_before_belongs_to_yesterday() {
    let db = TestDb::new();
    let store = db.store();
    assert!(is_due(&store, at(T0), &Paris).unwrap(), "no edition at all");
    db.built("2026-10-16T01:30:00.000Z");                                   // 03:30 Paris: yesterday's
    assert!(is_due(&store, at(T0), &Paris).unwrap());
    db.built("2026-10-16T02:30:00.000Z");                                   // 04:30 Paris
    assert!(!is_due(&store, at(T0), &Paris).unwrap());
}

#[test]
fn before_four_the_day_is_still_yesterday() {
    let db = TestDb::new();
    db.built("2026-10-15T05:00:00.000Z");                                   // yesterday 07:00
    assert!(!is_due(&db.store(), at("2026-10-16T01:30:00Z"), &Paris).unwrap()); // 03:30 Paris
}

#[test]
fn a_due_edition_is_built_and_shown() {
    let mut m = JournalModel::default();
    assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
    assert_eq!(m.checked(Ok(true)), Step::Build);
    assert_eq!(m.phase(), &Phase::Building { done: 0, total: 0 });
    m.finished(RunEnd::Done(built()), at(T0));
    assert_eq!(m.phase(), &Phase::Idle);
    assert!(!m.is_busy());
}

#[test]
fn building_reports_its_progress() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.event(&Event::Started { total: 3 });
    for feed_id in [1, 2] { m.event(&Event::FeedDone { feed_id, title: "A".into(), error: None }); }
    assert_eq!(m.phase(), &Phase::Building { done: 2, total: 3 });
}

#[test]
fn offline_keeps_the_edition_due_and_retry_builds_it() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(RunEnd::Done(EditionOutcome::Offline), at(T0));
    assert_eq!(m.phase(), &Phase::Offline);
    assert!(m.begin(Trigger::Retry, at(T0), &Paris), "Retry never waits");
    assert_eq!(m.checked(Ok(true)), Step::Build);
}

#[test]
fn an_edition_built_meanwhile_elsewhere_is_simply_shown() {
    assert_eq!(edition_failure(&EditionOutcome::NotDue), None);
    let mut runs = RunCoordinator::default();
    let request = RunRequest::BuildEdition { rebuild: false };
    runs.admit(&request);
    assert_eq!(runs.conclude(&request, Ok(EditionOutcome::NotDue), edition_failure), RunEnd::Done(EditionOutcome::NotDue));
    assert_eq!(runs.banner(), None);
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(RunEnd::Done(EditionOutcome::NotDue), at(T0));
    assert_eq!(m.phase(), &Phase::Idle);
}

#[test]
fn an_offline_refusal_keeps_its_banner() {
    let mut runs = RunCoordinator::default();
    let request = RunRequest::BuildEdition { rebuild: false };
    runs.admit(&request);
    runs.event(&request, &Event::NetworkUnreachable { failed: 11, attempted: 11 });
    assert_eq!(runs.conclude(&request, Ok(EditionOutcome::Offline), edition_failure), RunEnd::Done(EditionOutcome::Offline));
    assert_eq!(runs.banner(), Some(&Banner { message: "The network is unreachable: no edition built.".into(), show_log: false }));
}

#[test]
fn a_failed_or_stopped_build_leaves_the_edition_due() {
    for (end, message) in [(failed("disk full"), "disk full"), (failed("The job stopped unexpectedly."), "The job stopped unexpectedly."),
                           (RunEnd::Done(EditionOutcome::Interrupted), "Interrupted.")] {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(end, at(T0));
        assert_eq!(m.phase(), &Phase::Failed(message.into()));
        let mut relaunched = JournalModel::default();                       // the wait is in memory only
        assert!(relaunched.begin(Trigger::Automatic, at(T0) + mins(1), &Paris));
    }
    assert_eq!(edition_failure(&EditionOutcome::Interrupted), Some(RunFailure { message: "Interrupted.".into(), refusal: false }));
}

#[test]
fn automatic_triggers_wait_fifteen_minutes_after_a_failure() {
    for end in [failed("disk full"), RunEnd::Done(EditionOutcome::Offline)] {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(end, at(T0));
        assert!(!m.begin(Trigger::Automatic, at(T0) + mins(5), &Paris));
        assert!(m.begin(Trigger::Automatic, at(T0) + mins(16), &Paris));
    }
}

#[test]
fn retry_builds_during_the_window_and_a_manual_failure_restarts_it() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(failed("disk full"), at(T0));
    assert!(m.begin(Trigger::Retry, at(T0) + mins(10), &Paris));
    m.checked(Ok(true));
    m.finished(failed("disk full"), at(T0) + mins(10));
    assert!(!m.begin(Trigger::Automatic, at(T0) + mins(20), &Paris), "10 minutes after the manual failure");
    assert!(m.begin(Trigger::Automatic, at(T0) + mins(26), &Paris));
}

#[test]
fn a_success_clears_the_window() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(failed("disk full"), at(T0));
    m.begin(Trigger::Retry, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(RunEnd::Done(built()), at(T0));
    assert!(m.begin(Trigger::Automatic, at(T0) + mins(1), &Paris));
}

#[test]
fn finding_the_edition_not_due_clears_the_window() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(failed("disk full"), at(T0));
    assert!(m.begin(Trigger::Retry, at(T0) + mins(2), &Paris));
    assert_eq!(m.checked(Ok(false)), Step::Nothing);                         // built in a terminal meanwhile
    assert_eq!(m.phase(), &Phase::Idle);
    assert!(m.begin(Trigger::Automatic, at(T0) + mins(3), &Paris));
}

#[test]
fn a_clock_moved_backwards_expires_the_window() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(failed("disk full"), at(T0));
    assert!(m.begin(Trigger::Automatic, at(T0) - mins(60), &Paris));
}

#[test]
fn a_build_turned_away_by_another_long_run_stays_due() {
    let mut runs = RunCoordinator::default();
    runs.admit(&RunRequest::FetchAll);
    assert_eq!(runs.admit(&RunRequest::BuildEdition { rebuild: false }), Admission::Refused);
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    m.checked(Ok(true));
    m.finished(RunEnd::Refused, at(T0));
    assert_eq!(m.phase(), &Phase::Due);
    assert!(m.phase().offers_retry());
    assert!(m.begin(Trigger::Automatic, at(T0) + mins(1), &Paris), "no wait after a refusal");
}

#[test]
fn a_second_trigger_during_a_build_does_nothing() {
    let mut m = JournalModel::default();
    assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
    assert!(!m.begin(Trigger::Automatic, at(T0), &Paris));
    m.checked(Ok(true));
    assert!(!m.begin(Trigger::Retry, at(T0), &Paris));
    m.finished(RunEnd::Done(built()), at(T0));
    assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
}

#[test]
fn a_database_error_while_checking_fails_with_its_sentence() {
    let mut m = JournalModel::default();
    m.begin(Trigger::Automatic, at(T0), &Paris);
    assert_eq!(m.checked(Err(ressac_core::Error::Invalid("disk I/O error".into()))), Step::Nothing);
    assert_eq!(m.phase(), &Phase::Failed("Could not read the database: disk I/O error".into()));
    assert!(!m.is_busy());
}
```

In `runs.rs`'s tests: `conclude_hands_a_job_failure_back_and_shows_its_banner` — `admit(&FetchAll)`, `conclude::<()>(&FetchAll, Err(JobFailure::Panicked), |_| None)` → `RunEnd::Failed(RunFailure { message: "The job stopped unexpectedly.".into(), refusal: false })`, banner shown with `show_log: true`, `long_run_active()` false; `conclude(&FetchAll, Ok(()), |_| None)` after a fresh `admit` → `Done(())` and no banner.

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui journal 2>&1 | tail -3` → compile errors (`ClockCheck`, `RunEnd` missing).
- [ ] **Step 3: Implement** the items of the header in `journal.rs` and `runs.rs`.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B4a ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/journal.rs crates/ressac-ui/src/runs.rs
git commit -m "ressac-ui: Journal phases, the 15-minute wait and the 04:00 clock check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B4b: the bridges

### Task 3 (B4b): one door for every run

**Files:**
- Modify: `apps/linux/src/runs.rs`

**Interfaces:**
- Consumes: Task 2 (`RunEnd`, `conclude`); B2's `Worker::{long, short}`, `fetch_options`, `Invalidation`.
- Produces: `crate::runs::{submit_long, submit_short}` (header).

Behaviour:
- **Step 0 first** (see the header): dead code out of `apps/linux/src/runs.rs` in its own commit, or "none" in the ledger.
- Bridge: `impl cxx_qt::Initialize for Runs {}`; Rust: `impl cxx_qt::Initialize for qobject::Runs` stores `self.qt_thread()` in `thread_local! { static RUNS: RefCell<Option<CxxQtThread<qobject::Runs>>> }`.
- `submit_long` / `submit_short` queue `start_long` / `start_short` onto `Runs` (a failed `queue` or no handle → `tracing::error!` and `end(RunEnd::Refused)`).
- `start_long<T>(self, request, job, failure_of, events, end)`: `admit` → `Refused` → `end(Refused)`; else `current = Some(request)`, `state_changed`, `worker.long(job, |e| { queue run_event(e); events(e) }, |result| queue long_finished(result, failure_of, end))`. No session → `end(Refused)` and nothing shown. `long_finished`: `let run_end = coordinator.conclude(&current, result, failure_of)`, the existing logging (a refusal `info!`, a failure `warn!`), `current = None`, `invalidation.run_finished` → `database_changed`, `state_changed`, then `end(run_end)`.
- `start_short<T>(self, request, job, end)`: `admit` (always `Start`), `worker.short(job, |result| queue short_finished(request, result, end))`; `short_finished`: `conclude(&request, result, |_| None)`, logging, `invalidation.run_finished` → `database_changed`, `state_changed`, `end(run_end)`.
- `refresh_all` / `refresh_source` / `refresh_feed` call `start_long(request, <the fetch job>, |_: &()| None, |_| {}, |_| {})`; `run_event` and the old `run_finished` keep their effects (progress, `database_changed` at most once a second).

- [ ] **Step 1: Verify the current behaviour is pinned** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test reading 2>&1 | grep "test result"'` → `ok.` (the fetch path's walk, before the rewrite).
- [ ] **Step 2: Implement** as above.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.` (`launch`, `reading`, `reader`, `keys`, `menus`).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/runs.rs
git commit -m "ressac-kde: one coordinator door for every run (long and short, from any bridge)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 4 (B4b): the `Journal` singleton, `Recette.walk`

**Files:**
- Create: `apps/linux/src/journal.rs`
- Modify: `apps/linux/src/lib.rs` (`mod journal;`)
- Modify: `apps/linux/src/recette.rs` (`#[qproperty(QString, walk, READ, CONSTANT)]`, `const WALK_ENV: &str = "RESSAC_RECETTE_WALK";`, read in `Default` like `press_default`, only when `--screenshot` was given)

**Interfaces:**
- Consumes: Task 2 (`JournalModel`, `Trigger`, `Step`, `is_due`, `header`, `placeholder`, `is_finished`, `edition_failure`, `END_LINE`, `RETRY`), Task 3 (`submit_long`); `with_session`, `runs::read_failure`.
- Produces: `Journal` and `Recette.walk` (header).

Behaviour (`JournalRust { model: JournalModel, view: Option<JournalView> }`, `impl cxx_qt::Threading for Journal {}`):
- `reload()`: `with_session(|s| s.store.journal())`; `Ok(v)` → `view = v`; `Err(e)` → `banner(read_failure(&e))`; `state_changed`.
- `trigger()` → `begin(Trigger::Automatic)`; `retry()` → `begin(Trigger::Retry)`; `minuteTick()` → `if model.clock_tick(Utc::now(), &Local) { begin(Trigger::Automatic) }`.
- `begin(trigger)` (private): `now = Utc::now()`; `model.begin(trigger, now, &Local)` false → return; `due = with_session(|s| is_due(&s.store, now, &Local)).unwrap_or(Ok(false))`; `model.checked(due)`; `state_changed`; on `Step::Build`: `submit_long(RunRequest::BuildEdition { rebuild: false }, |cx, sink, stop| cx.block_on(cx.app.build_edition(EditionOptions::default(), Utc::now(), &Local, sink, stop)), edition_failure, events, end)` with `events` queueing `edition_event(event.clone())` and `end` queueing `edition_finished(run_end)` onto the `Journal` (`self.qt_thread()`, one handle per closure; a failed `queue` is `tracing::debug!`).
- `edition_event(e)`: `model.event(&e)`, `state_changed`. `edition_finished(end)`: `model.finished(end, Utc::now())`, `reload()` (emits `state_changed`).
- Getters: `phase` = `model.phase().as_str()`; `building` = phase is Building; `placeholder` = `placeholder(model.phase(), view.as_ref())` or `""`; `offersRetry`; `header` = `view.map(|v| header(v, &Local))` or `""`; `finished` = `view.is_some_and(is_finished)`; `endLine` = `END_LINE`; `retryTitle` = `RETRY`.

- [ ] **Step 1: Implement** the three files as above (no unit test here: the rules are Task 2's, Task 6's walk drives the object).
- [ ] **Step 2: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.`.
- [ ] **Step 3: Commit**

```bash
git add apps/linux/src/journal.rs apps/linux/src/lib.rs apps/linux/src/recette.rs
git commit -m "ressac-kde: Journal singleton (launch, activation and minute triggers, Retry), Recette.walk

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 5 (B4b): the entry list lays the Journal out

**Files:**
- Modify: `apps/linux/src/entries.rs`

**Interfaces:**
- Consumes: Task 1 (`sections`, `JournalRow`, `section_title`, `overflow_label`); core `Store::journal`.
- Produces: `EntriesModel` roles `kind`, `section`, `overflowFeed` and property `journal` (header).

Behaviour:
- **Step 0 first** (see the header) for `apps/linux/src/entries.rs`.
- `EntriesModelRust` gains `edition: Option<JournalView>`, read with `store.journal()` in `showSource` and `reload` when the list's source is `Source::Journal` (else `None`; an error → `banner(read_failure)`); local edits never re-read it.
- The shown rows become slots: `enum Slot { Entry { shown: Shown, section: &'static str }, Overflow { id: String, section: &'static str, label: String, feed_id: FeedId } }`, keyed `"entry-<id>"` / `JournalRow::id()`. In the Journal, sync builds them from `sections(&rows.map(|r| (r.entry_id, r.feed_id)), edition.as_ref())` flattened in order; elsewhere every row is an `Entry` with section `""`. `plan_rows` keys by that string.
- Every invokable that takes a QML row (`select`, `toggleRead`, `toggleStarred`, `rowAppeared`, and B3's row-based ones) resolves it through the slot: an overflow row or a stale index does nothing. `selectedRow` = the index of the selected entry's slot. `canNavigate`, `next`/`previous`/`next unread` keep working on entries (B3's `EntryList` methods), so `j`, `k`, Space and the arrows never land on an overflow row.
- Roles: `kind` (`"entry"` / `"overflow"`), `section` (the title or `""`), `overflowFeed` (the overflow's feed id, 0 for entries); an overflow slot answers `title` = its label, `entryId` −1, the others empty or false. `journal` = the list's source is the Journal (emitted with `list_changed`).

- [ ] **Step 1: Verify the current behaviour is pinned** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.` (`reading` and `keys` walk the entry list, selection and keys).
- [ ] **Step 2: Implement** as above.
- [ ] **Step 3: Verify** — the same command → every `ok.`; `cargo clippy` as in Task 4 → `Finished`. Then the full gate (B4b ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/entries.rs
git commit -m "ressac-kde: the entry list lays the Journal out (sections, overflow rows)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B4c: the Journal on screen

### Task 6 (B4c): sections, placeholders, triggers and walks

**Files:**
- Modify: `apps/linux/qml/EntriesPane.qml`
- Modify: `apps/linux/qml/ReadingPage.qml`
- Modify: `apps/linux/qml/Main.qml`
- Test (not counted): `apps/linux/tests/journal.rs`

**Interfaces:**
- Consumes: Tasks 4–5; B2's panes, `reloadAll()`, the 60 s `Timer`, `Runs`; B3's `focusName()`, `entriesPane.focusList()`.
- Produces: the QML items of the header.

`EntriesPane.qml` (inventory §3 "Native Journal screen"):
- `readonly property bool journal: EntriesModel.journal`.
- Above the list, when `journal && Journal.placeholder === "" && Journal.header !== ""`: the header line (`Kirigami.Theme.smallFont`, `disabledTextColor`, horizontal padding 12, vertical 6, `font.features: { "tnum": 1 }`) and a `Kirigami.Separator`.
- When `journal && Journal.placeholder !== ""`, instead of the list: a centered column with `QQC2.BusyIndicator { visible: Journal.building }`, the text (`Journal.placeholder`, `disabledTextColor`, centered, wrapping) and a `QQC2.Button { text: Journal.retryTitle; visible: Journal.offersRetry; onClicked: Journal.retry() }`. B2's empty-list placeholder stays hidden then.
- The `ListView`: `section.property: journal ? "section" : ""`, `section.delegate: Kirigami.ListSectionHeader { required property string section; text: section; width: ListView.view.width }`; one delegate whose content is the B2 entry layout when `model.kind === "entry"`, or, for `"overflow"`, a single line `model.title` in `Kirigami.Theme.smallFont` and `disabledTextColor` that is never highlighted, `focusPolicy: Qt.NoFocus`, and whose click runs `SourcesModel.selectKey("feed:" + model.overflowFeed)`. `footer`: when `journal && Journal.finished`, `Journal.endLine` centered, `disabledTextColor`, padding 8 (height 0 otherwise).

`ReadingPage.qml`:
- `readonly property bool editionTriggers: !Recette.active || Recette.walk === "journal"`.
- `Component.onCompleted` additionally runs `Journal.reload(); if (editionTriggers) Journal.trigger()`.
- `Connections { target: Qt.application; function onStateChanged() { if (Qt.application.state === Qt.ApplicationActive && page.editionTriggers) Journal.trigger() } }`.
- The 60 s timer's `onTriggered: { Live.minuteTick(); if (page.editionTriggers) Journal.minuteTick() }`.
- `reloadAll()` also calls `Journal.reload()`; `Connections { target: Journal; function onBanner(message, showLog) { Runs.showBanner(message, showLog) } }`.
- The existing `SourcesModel.onSelectionChanged` handler, after `EntriesModel.showSource(…)`, calls `entriesPane.focusList()` when `SourcesModel.selectedKey === "journal"`.

`Main.qml`: `readonly property var steps: Recette.walk === "journal" ? journalSteps : walkSteps`; the walker reads `root.steps` where it read `root.walkSteps`. Appended to `walkSteps` (fixture: edition 1 built 2026-10-10T08:00Z, 1 unreachable feed; This Week 5, 6 (Daring Fireball), The Wire 1 (xkcd.com) then "+2 more in xkcd.com"; entry 1 read by step 13):

```qml
{ name: "23-journal", enter: function () { SourcesModel.selectKey("journal")
    console.warn("walk: 23-journal rows=" + EntriesModel.rowCount() + " phase=" + Journal.phase + " placeholder=" + Journal.placeholder
        + " header=" + Journal.header + " focus=" + reading.item.focusName()) } },
```

`journalSteps` (the test points every active feed at a closed loopback port; the launch trigger builds at once):

```qml
readonly property var journalSteps: [
    { name: "02-journal", enter: function () { SourcesModel.selectKey("journal"); console.warn("walk: 02-journal phase=" + Journal.phase) } },
    { name: "03-wait", enter: function () {} },
    { name: "04-wait", enter: function () {} },
    { name: "05-offline", enter: function () { console.warn("walk: 05-offline phase=" + Journal.phase + " placeholder=" + Journal.placeholder
        + " retry=" + Journal.offersRetry + " banner=" + Runs.bannerMessage) } },
    { name: "06-retry", enter: function () { Journal.retry(); console.warn("walk: 06-retry phase=" + Journal.phase) } },
    { name: "07-wait", enter: function () {} },
    { name: "08-retried", enter: function () { const after = Journal.phase; Journal.trigger()
        console.warn("walk: 08-retried phase=" + after + " after-trigger=" + Journal.phase) } },
]
```

- [ ] **Step 1: Write the failing test** `apps/linux/tests/journal.rs`:

```rust
//! The Journal, offscreen, through `--screenshot` (spec §7.5; inventory §3).
#![cfg(target_os = "linux")]

mod common;
use common::*;

fn editions(db: &std::path::Path) -> i64 {
    rusqlite::Connection::open(db).unwrap().query_row("SELECT COUNT(*) FROM edition", [], |r| r.get(0)).unwrap()
}

#[test]
fn the_journal_lists_the_edition_by_section_and_builds_nothing_in_recette() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    let line = "walk: 23-journal rows=4 phase=idle placeholder= header=Saturday 10 October · 3 entries · 1 read · 1 feed unreachable focus=entries";
    assert!(err.contains(line), "{line} missing in {err}");
    assert_drawn(&run.shots.join("23-journal.png"));
    assert_eq!(editions(&db), 1, "no automatic build in recette");
}

#[test]
fn an_offline_build_shows_retry_and_the_banner_and_writes_nothing() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db).unwrap()
        .execute_batch("UPDATE feed SET xml_url = 'http://127.0.0.' || id || ':9/feed' WHERE state = 'active'").unwrap();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "journal")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in ["walk: 05-offline phase=offline placeholder=Offline: no edition yet retry=true banner=The network is unreachable: no edition built.",
                 "walk: 06-retry phase=building", "walk: 08-retried phase=offline after-trigger=offline"] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert_drawn(&run.shots.join("05-offline.png"));
    assert_eq!(editions(&db), 1, "offline: nothing written");
}
```

(`02-journal` may log `building` or `offline` depending on speed; it is not asserted. `08-retried`: the second failure set a new 15-minute window, so the automatic trigger does nothing.)

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test journal 2>&1 | grep -E "panicked|test result"'` → FAIL: `walk: 23-journal … missing`.
- [ ] **Step 3: Implement** the three QML files.
- [ ] **Step 4: Verify and look** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.` (`launch`, `reading`, `reader`, `keys`, `menus`, `journal`). If `05-offline` is flaky because the build has not ended, add one more empty wait step before it (never shorten the walk's 400 ms). Look at the screens: run B2d Task 9 Step 4's `--screenshot` command (it writes `target-shots/` in the bind mount), once as is and once with `RESSAC_RECETTE_WALK=journal` on a copy whose feeds point at `127.0.0.<id>:9`, open `23-journal.png` and `05-offline.png` on the Mac, then `rm -rf target-shots`: section headers "This Week" and "The Wire", "+2 more in xkcd.com" in small grey under "Recent comic", the header line above the list; the offline placeholder with its Retry button and the warning banner. Then the full gate (B4c and B4 end here). Ledger rows B4a–B4c: commits, `ressac-ui` and `ressac-kde` test counts, deviations 1–8.
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/EntriesPane.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/Main.qml apps/linux/tests/journal.rs
git commit -m "ressac-kde: Journal list (sections, overflow rows, placeholders), edition triggers, walks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
