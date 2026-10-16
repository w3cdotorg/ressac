# Ressac Cadence and Morning Edition (Sub-project 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every feed a rhythm group (Daily, Weekly, Monthly, Rare, Silent) and give Ressac.app a finished morning edition, composed and stored by the core on the first launch of the day, readable in Native and in Antiope.

**Architecture:**

- **Phases 1a–1d (Rust core rhythm).** `CadenceGroup` and its classification from the median interval; the `entries_per_month` fix; schema v4 (`cadence_group`, `cadence_override`, edition tables, contract views); the per-feed override in the store; the 14 reserved pages (1d).
- **Phases 2a–2b (Rust edition).** A pure `edition` module composes sections, caps and ranks from candidate rows; `store/edition.rs` selects candidates, checks the 04:00 due rule and writes the edition in one transaction; `Ressac::build_edition` chains refresh and composition.
- **Phases 3a–3b (CLI).** `ressac edition build|show`, `ressac feed cadence`, the `edition` and `cadence` NDJSON events.
- **Phases 4a–4b (Swift data and engine).** `knownSchemaVersion = 4`, records and live queries over the v4 views; `RunRequest.buildEdition`, `RunRequest.setCadence`, the `edition` event.
- **Phases 5a–5c (Swift Native).** Step 0 on `AppState.swift`, `JournalModel` (due check, build on launch, foreground and 04:00 rollover, offline state); sidebar Journal and Rhythm, the Journal list; inspector rhythm line and override menu, Garden › All feeds Rhythm column.
- **Phases 6a–6c (Swift Antiope).** Reserved pages 104–109 and SOMMAIRE; P104 with sections, overflow rows and `FIN DE L'EDITION`, build progress; sub-pages `104 n/N` and the Antiope inspector line (6c).
- **Phase 7.** Manual acceptance on the real database, docs, push and PR (not merged).

**Tech Stack:**

- **Rust:** as in sub-projects 1, 2a and 4 (edition 2024, rusqlite 0.40.2 bundled, chrono, clap, serde). No new crate.
- **Swift:** Swift 6.4, SwiftUI, macOS 27, Xcode 27, `xcodegen`; GRDB.swift `7.11.1`; Swift Testing.

**Spec:** `docs/specs/2026-10-16-cadence-and-edition-design.md` (*the spec*). Earlier specs: `docs/specs/2026-10-10-core-and-garden-design.md` (*core spec*), `docs/specs/2026-10-10-mac-app-design.md` (*app spec*), `docs/specs/2026-10-14-antiope-design.md` (*Antiope spec*). Crate and API findings are in `notes.md`.

## Phase files

Each phase touches **at most 5 files, test files included** (project rule). The regenerated `tests/fixtures/contract.sqlite` does not count.

Execute the (sub-)phases **in the order of the table below**, not in file-number order. Each (sub-)phase ends with its verification gate.

- **Phase 4a runs right after phase 1b.** From 1b the contract fixture is at v4; the app only accepts v4 from 4a, so the Swift suite is red between 1b and 4a (the Rust-only gate of 1b does not run it).
- **Stops between phases:** not decided yet. Until Clément says otherwise, the project rule applies: wait for his explicit approval after each (sub-)phase.

| Order | Phase | File | Deliverable |
|---|---|---|---|
| 1 | 1a | `cadence-edition/phase-01-cadence-core.md` | `CadenceGroup`, `classify`, `Cadence.group`, `entries_per_month` over the covered span; `health/mod.rs` re-exports |
| 2 | 1b | `cadence-edition/phase-01-cadence-core.md` | `0004_edition.sql`, migration to v4 with the `cadence_group` backfill and the 104–109 move, `save_diagnoses` stores the group, contract fixture (rhythm rows and one seeded edition) and drift test at v4, CLI `Schema version 4.` |
| 3 | 4a | `cadence-edition/phase-04-swift-data-engine.md` | `knownSchemaVersion = 4`, `Data/Edition.swift` (`CadenceGroup`, `EditionSection`, edition and cadence records, `EditionClock`), `Queries.edition`, `Queries.cadenceRows`, `Queries.isEditionDue` |
| 4 | 1c | `cadence-edition/phase-01-cadence-core.md` | `Store::set_cadence_override`, `Store::cadence_row`, `CadenceRow`, `Error::UnknownFeed`, `Error::FeedBuried`, `Error::EditionNotDue`, `Ressac::set_cadence` |
| 5 | 1d | `cadence-edition/phase-01-cadence-core.md` | `RESERVED_PAGES` 14 pages with their titles; `page list` / `page set` CLI expectations |
| 6 | 2a | `cadence-edition/phase-02-edition-core.md` | `edition.rs`: `Section`, `Caps`, `Candidate`, `compose`, `day_start` |
| 7 | 2b | `cadence-edition/phase-02-edition-core.md` | `store/edition.rs` (latest, due, candidates, write, rebuild, `edition_view`), `Ressac::build_edition`, `Ressac::edition_today`, `Event::Edition` (and its arm in `output.rs`) |
| 8 | 3a | `cadence-edition/phase-03-cli.md` | `ressac edition build` and `ressac edition show`, text and `--json`, exit codes |
| 9 | 3b | `cadence-edition/phase-03-cli.md` | `ressac feed cadence`, `cadence` JSON line, CLI tests of pages 104–109 |
| 10 | 4b | `cadence-edition/phase-04-swift-data-engine.md` | `RunRequest.buildEdition`, `RunRequest.setCadence`, `EngineEvent.edition` / `.cadence`, refusal mapping for both, `RunCoordinator` switches |
| 11 | 5a | `cadence-edition/phase-05-native.md` | Step 0 on `AppState.swift`; `JournalModel` with live edition, due check, launch build, foreground check, 04:00 rollover, offline, failed and "built elsewhere" states; `RunCoordinator.editionProgressLabel` |
| 12 | 5b | `cadence-edition/phase-05-native.md` | `Source.journal`, `Source.cadence(_)`, sidebar Journal and Rhythm, Journal list in rank order with sections, overflow rows, header, end line, `⇧⌘A` and `Space` inside the edition |
| 13 | 5c | `cadence-edition/phase-05-native.md` | Inspector rhythm line and override menu, All Feeds rhythm label and `Status \| Rhythm` grouping, `AppState.setCadence` |
| 14 | 6a | `cadence-edition/phase-06-antiope.md` | `ReservedPage` 104–109, `PageDestination`s, SOMMAIRE rows with counts |
| 15 | 6b | `cadence-edition/phase-06-antiope.md` | P104 column (headings, overflow rows, end line, progress and offline rows), `FIN DE L'EDITION` page reached by SUITE, RÉPÉTITION retries, `EDITION IN PROGRESS n/N` |
| 16 | 6c | `cadence-edition/phase-06-antiope.md` | Sub-pages `104 n/N` in the reader, Antiope inspector rhythm line and override rows |
| 17 | 7 | `cadence-edition/phase-07-acceptance-docs.md` | Migration of the real database, manual check with Clément, README, `notes.md`, `TODO.md`, push and PR (not merged) |

## Global Constraints

- **Inherited:**
  - Rust edition `2024`, `rust-version = "1.99"`, pins in the root `Cargo.toml` (no new crate).
  - Swift 6.4 language mode with complete strict concurrency; deployment target macOS 27.0.
  - No third-party Swift dependency other than GRDB.swift `7.11.1`.
  - `SWIFT_TREAT_WARNINGS_AS_ERRORS = YES`.
- **Language:** every user-facing string is in English. **Exceptions:** the Minitel key labels (`SOMMAIRE`, `SUITE`, `RETOUR`, `GUIDE`, `RÉPÉTITION`) and, in Antiope only, the end-of-edition line `FIN DE L'EDITION`. Code and docs in English, `notes.md` in French (no em dash in French prose).
- **Exact strings:**
  - Native sections `Front Page`, `This Week`, `The Wire`; Antiope `FRONT PAGE` (yellow), `THIS WEEK` (cyan), `THE WIRE` (green).
  - Header `Friday 16 October · 43 entries · 12 read`, suffix ` · 3 feeds unreachable` when `unreachable > 0` (`1 feed unreachable` singular).
  - Overflow `+12 more in Actualitté` (Native), `+12 MORE IN ACTUALITTÉ` (Antiope: white on a blue background, since blue never colours text; accents kept in capitals, Bedstead draws them; opened by a click only, the column's rows are not selectable one by one).
  - End `That's the edition.`; empty `Nothing new this morning.`; progress `Building this morning's edition… 34/156` and Antiope `EDITION IN PROGRESS 34/156`; offline `Offline: no edition yet` with a `Retry` button.
  - Inspector `Rhythm: Daily · median 0.4 d · last entry 2 h ago`, suffix ` (set by you)`; menu items `Auto`, `Daily`, `Weekly`, `Monthly`, `Rare`, `Silent`.
  - CLI refusals (exit `2`, stderr): `An edition was already built this morning.`, `The network is unreachable: no edition built.`, `No feed #N.`, `Feed #N is buried.`
- **Rhythm thresholds** (`health/cadence.rs`): `SILENT_AFTER_DAYS = 365`, `DAILY_BELOW_DAYS = 1.5`, `WEEKLY_BELOW_DAYS = 10.0`, `MONTHLY_BELOW_DAYS = 45.0`; at most the 21 most recent dated entries (existing `INTERVAL_WINDOW`).
- **Edition constants** (`edition.rs`): `CAP_WEEK = 3`, `CAP_WIRE = 1`, `BACKLOG_GUARD_DAYS = 7`, `DAY_STARTS_AT_HOUR = 4` (local time), first-edition window `24` hours.
- **Schema:**
  - `schema_meta.version` is **4** after phase 1b; core `SCHEMA_VERSION = 4`; app `AppDatabase.knownSchemaVersion = 4` from phase 4a;
  - the migration takes a backup first (`ressac.sqlite.bak-v3`) and runs in one transaction, like v2 and v3.
- **Client-writable columns:** unchanged (`entry.read_at`, `entry.starred_at`, `feed.prefer_full_text`). `feed.cadence_override` and every edition table are written by the core only; the app goes through `ressac feed cadence` and `ressac edition build`.
- **Reserved pages:** `100` Index, `101` Unread, `102` Today, `103` Starred, `104` Journal, `105` Daily, `106` Weekly, `107` Monthly, `108` Rare, `109` Silent, `800` Needs triage, `801` All feeds, `802` Cemetery, `899` Guide. Assignable range `110...799`, minus the reserved pages.
- **Verification gates:**
  - **Rust phases:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. The plan's code is not guaranteed to be in rustfmt's exact layout: running `cargo fmt --all` before `--check` is allowed.
  - **Swift phases:** the Rust gate, plus `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test`, zero warnings.
  - There is no `tsc`/`eslint` in this project; the gates above replace them.
- **Step 0 rule:** before restructuring a file over 300 lines, first remove its dead code, unused imports and debug logs in a separate commit. Concerned here: `AppState.swift` (823, Step 0 in 5a), `ArticleTemplate.swift` (695, only if touched), `Queries.swift` (282, grows past 300 in 4a: no Step 0 needed), `service.rs` and `store/mod.rs` (checked in 1b and 2b), `output.rs` (436, one additive match arm in 2b, no restructuring). Check line counts when the phase starts.
- **Shell setup:** run `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"` before any command.
- **Commits:**
  - one per task, on branch `cadence-edition`;
  - messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Push:** `git -c credential.helper= -c credential.helper='!gh auth git-credential' push`, only in phase 7.
- **Never commit:** a real `.opml`, `.superpowers/`, or the generated `apps/macos/Ressac.xcodeproj`.
- **Tests and the real database:** never run `ressac` against `~/Library/...` from a test. Use `--db` in a temp dir and `HOME` set to a temp dir. Every test that depends on the time of day takes an injected clock and timezone, never `Utc::now()` / `Date()` directly.
- **No simulated keystrokes** (System Events, `CGEvent.post*`) towards any app other than the test host. In-process tests only.

## Review Focus

Inputs and conditions the spec implies but no feature test naturally hits. Each one has its test in the owning phase.

1. **A day boundary across a DST change** (the night of 2026-10-25 in Europe/Paris, 03:00 → 02:00): `day_start` gives 04:00 local on both sides, and an edition built at 03:30 local on that night still counts for the previous day. Phase 2a: `day_start` tests in `Europe/Paris` on 2026-10-25 at 03:30 and 04:30 local, and on 2026-03-29.
2. **The user reads part of the edition, then a manual `⇧⌘R` during the day fetches more entries:** the Journal does not change, and those entries are in tomorrow's edition (their `fetched_at` is after `window_end`). Phase 2b store test: write an edition, insert entries with a later `fetched_at`, assert `edition_today` unchanged and the next `edition_candidates` includes them.
3. **An entry whose feed is buried after the edition was built:** it stays in today's edition (the edition is frozen) but the rhythm lists drop the feed at once. Phase 2b test on `v_edition_today` after a bury; phase 4a test that `Queries.cadenceRows` excludes the buried feed.
4. **The app is quit, or `ressac` is killed, in the middle of `edition build`:** no edition row exists, the next launch builds one with the same `window_start`. Phase 2b test with the stop flag set during refresh (`EditionOutcome::Interrupted`, nothing written); phase 5a test that a failed or cancelled run leaves `JournalModel` in `.due` and a relaunch starts a build.
5. **A feed title with an accent or emoji in an overflow row, in Antiope** (`+3 MORE IN CAFÉ ☕ PÉDAGO`): the row is exactly the column's cell width and the separator stays straight. Phase 6b test on the overflow row through `CellText`, like the existing row-width test in `EntryPageColumn`.

## Interfaces between phases

Phase files must use these names and signatures exactly. A phase may add private helpers; a new **public** name that a later phase needs is a plan change and goes back to this section first.

### Phase 1a (Rust, `model.rs` and `health/cadence.rs`)

```rust
// model.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CadenceGroup { Daily, Weekly, Monthly, Rare, #[default] Silent }

impl CadenceGroup {
    pub const ALL: [CadenceGroup; 5] = [Self::Daily, Self::Weekly, Self::Monthly, Self::Rare, Self::Silent];
    pub fn as_str(self) -> &'static str;          // "daily" … "silent"
    pub fn parse(s: &str) -> Option<Self>;        // inverse of as_str
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cadence {
    pub last_entry_at: Option<DateTime<Utc>>,
    pub median_interval_days: Option<f64>,
    pub entries_per_month: f64,
    pub group: CadenceGroup,                      // new
}

// health/cadence.rs
pub const SILENT_AFTER_DAYS: i64 = 365;
pub const DAILY_BELOW_DAYS: f64 = 1.5;
pub const WEEKLY_BELOW_DAYS: f64 = 10.0;
pub const MONTHLY_BELOW_DAYS: f64 = 45.0;
/// First match wins: Silent (no entry, or latest older than SILENT_AFTER_DAYS), Daily, Weekly,
/// Monthly by median, else Rare (median ≥ 45 d, or no median: a single entry within the year).
pub fn classify(last_entry_at: Option<DateTime<Utc>>, median_interval_days: Option<f64>, now: DateTime<Utc>) -> CadenceGroup;
```

`cadence(entries, now)` keeps its signature and fills `group` with `classify`. `entries_per_month` = entries of the last 365 days ÷ months spanned (oldest of them → `now`, in days ÷ 30.4375, at least `1.0`). The health verdict keeps reading `entries_per_month` for its bullet text only.

### Phase 1b (schema v4, store)

`schema/0004_edition.sql` is exactly the SQL of spec §5, plus the three views below, plus `UPDATE schema_meta SET version = 4;`.

```sql
CREATE VIEW v_cadence AS
  SELECT d.id AS feed_id, d.title, d.page, p.path AS folder_path,
         COALESCE(d.cadence_override, h.cadence_group) AS effective_group,
         h.cadence_group AS computed_group,
         d.cadence_override IS NOT NULL AS is_override,
         h.median_interval_days, h.entries_per_month, h.last_entry_at,
         (SELECT COUNT(*) FROM entry e WHERE e.feed_id = d.id AND e.read_at IS NULL) AS unread
  FROM feed d JOIN feed_health h ON h.feed_id = d.id
  LEFT JOIN v_folder_path p ON p.folder_id = d.folder_id
  WHERE d.state = 'active';

CREATE VIEW v_edition_today AS
  SELECT ed.id AS edition_id, ed.built_at, ed.window_start, ed.window_end, ed.unreachable,
         ee.entry_id, ee.section, ee.rank, e.feed_id, d.title AS feed_title,
         e.title, e.published_at, e.read_at, e.starred_at
  FROM edition ed
  JOIN edition_entry ee ON ee.edition_id = ed.id
  JOIN entry e ON e.id = ee.entry_id
  JOIN feed d ON d.id = e.feed_id
  WHERE ed.id = (SELECT MAX(id) FROM edition);

CREATE VIEW v_edition_overflow AS
  SELECT o.edition_id, o.section, o.feed_id, d.title AS feed_title, d.page, o.hidden_count
  FROM edition_overflow o JOIN feed d ON d.id = o.feed_id
  WHERE o.edition_id = (SELECT MAX(id) FROM edition);
```

An edition with no entries has no row in `v_edition_today`; clients read the edition header from `edition` directly (`SELECT … FROM edition ORDER BY id DESC LIMIT 1`).

- `store/pages.rs`: `RESERVED_PAGES: [u16; 14] = [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899]`, titles `"Journal"`, `"Daily"`, `"Weekly"`, `"Monthly"`, `"Rare"`, `"Silent"` for 104–109.
- Migration v3 → v4 (`store/mod.rs`): after the SQL, set `page = NULL` on any folder or feed holding 104–109, run `pages::assign_missing`, then reclassify every feed: `UPDATE feed_health SET cadence_group = ?` from `classify(last_entry_at, median_interval_days, now)` (no refetch).
- `Store::save_diagnoses` writes `cadence_group = d.cadence.group.as_str()`; reading diagnoses back fills `Cadence.group`.

### Phase 1c (override)

```rust
// error.rs — new variants (EditionNotDue is declared here for Phase 2b, whose five files are taken)
#[error("No feed #{0}.")]       UnknownFeed(FeedId),
#[error("Feed #{0} is buried.")] FeedBuried(FeedId),
#[error("An edition was already built this morning.")] EditionNotDue,

// store/feeds.rs
impl Store {
    /// `None` clears the override. Refuses an unknown or buried feed.
    pub fn set_cadence_override(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<()>;
    /// `feed` LEFT JOIN `feed_health` (not `v_cadence`): a feed never diagnosed reads computed `Silent`.
    /// Refuses an unknown feed; reads a buried one.
    pub fn cadence_row(&self, feed_id: FeedId) -> Result<CadenceRow>;
}

// service.rs
impl Ressac {
    pub fn set_cadence(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<CadenceRow>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CadenceRow {            // in store/feeds.rs, re-exported as ressac_core::store::CadenceRow
    pub feed_id: FeedId,
    pub effective_group: CadenceGroup,
    pub computed_group: CadenceGroup,
    pub is_override: bool,
}
```

### Phase 1d (reserved pages)

`store/pages.rs`: `RESERVED_PAGES: [u16; 14]` and `RESERVED_TITLES` as listed under Phase 1b; the `page list` and `page set … 109` CLI tests are updated here (the 1b migration already frees 104–109 with plain SQL).

### Phase 2a (Rust, `crates/ressac-core/src/edition.rs`, pure)

```rust
pub const CAP_WEEK: usize = 3;
pub const CAP_WIRE: usize = 1;
pub const BACKLOG_GUARD_DAYS: i64 = 7;
pub const DAY_STARTS_AT_HOUR: u32 = 4;
pub const FIRST_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section { Front, Week, Wire }
impl Section {
    pub fn for_group(group: CadenceGroup) -> Option<Section>; // Monthly|Rare → Front, Weekly → Week, Daily → Wire, Silent → None
    pub fn as_str(self) -> &'static str;                      // "front" | "week" | "wire"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps { pub week: usize, pub wire: usize }
impl Default for Caps { /* CAP_WEEK, CAP_WIRE */ }

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_page: Option<u16>,
    pub group: CadenceGroup,          // effective group
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placed { pub entry_id: i64, pub section: Section, pub rank: u32 }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overflow { pub feed_id: FeedId, pub section: Section, pub hidden_count: u32 }
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composition { pub entries: Vec<Placed>, pub overflow: Vec<Overflow> }

/// Sections front → week → wire; inside a section feeds by page (None last, then feed_id),
/// entries by published_at desc then entry_id desc; caps per feed per edition; rank from 1.
pub fn compose(candidates: &[Candidate], caps: Caps) -> Composition;

/// 04:00 local of the day `now` belongs to (yesterday's 04:00 before 04:00), in UTC.
pub fn day_start<Tz: chrono::TimeZone>(now: chrono::DateTime<Tz>) -> DateTime<Utc>;
```

`lib.rs` gains `pub mod edition;`.

### Phase 2b (store and service)

```rust
// store/edition.rs
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionRecord {
    pub id: i64,
    pub built_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub unreachable: u32,
}

pub struct EditionParams {
    pub day_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub built_at: DateTime<Utc>,
    pub caps: Caps,
    pub unreachable: u32,
    pub rebuild: bool,
}

impl Store {
    pub fn latest_edition(&self) -> Result<Option<EditionRecord>>;
    /// No edition has built_at >= day_start.
    pub fn edition_due(&self, day_start: DateTime<Utc>) -> Result<bool>;
    /// Active feeds only, fetched_at in (start, end], unread, published_at >= start − BACKLOG_GUARD_DAYS,
    /// effective group not Silent.
    pub fn edition_candidates(&self, window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> Result<Vec<Candidate>>;
    /// One write_tx: re-check due (or, with rebuild, delete today's edition and reuse its window_start),
    /// select candidates, compose, insert edition + edition_entry + edition_overflow.
    /// Not due and not rebuild → Err(Error::EditionNotDue).
    pub fn write_edition(&self, p: &EditionParams) -> Result<(EditionRecord, Composition)>;
    /// The latest edition through the contract views; None before the first one.
    pub fn edition_view(&self) -> Result<Option<EditionView>>;
}
// store/edition.rs is a private module: EditionRecord, EditionParams, EditionView,
// EditionEntryRow and EditionOverflowRow are re-exported from ressac_core::store,
// and the three view types also from ressac_core::service.

// service.rs
pub struct EditionOptions { pub caps: Caps, pub rebuild: bool }

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionReport {
    pub id: i64, pub entries: usize, pub front: usize, pub week: usize, pub wire: usize,
    pub overflow: u32,      // sum of hidden_count
    pub unreachable: u32,   // feed_done events with an error during the refresh
}

pub enum EditionOutcome { Built(EditionReport), NotDue, Offline, Interrupted }

impl Ressac {
    /// Due check (skip if !rebuild and not due → NotDue), refresh (force = false, every feed),
    /// Offline if summary.network_unreachable, Interrupted if `stop` was set, else write_edition
    /// with day_start(Local::now()) — injected through `now` for tests — and emit Event::Edition.
    pub async fn build_edition(&self, opts: EditionOptions, now: DateTime<Utc>, tz: &impl chrono::TimeZone,
                               sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<EditionOutcome>;
    pub fn edition_today(&self) -> Result<Option<EditionView>>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionView { pub edition: EditionRecord, pub entries: Vec<EditionEntryRow>, pub overflow: Vec<EditionOverflowRow> }
pub struct EditionEntryRow { pub entry_id: i64, pub section: Section, pub rank: u32, pub feed_id: FeedId, pub feed_title: String,
    pub title: Option<String>, pub published_at: DateTime<Utc>, pub read_at: Option<DateTime<Utc>>, pub starred_at: Option<DateTime<Utc>> }
pub struct EditionOverflowRow { pub section: Section, pub feed_id: FeedId, pub feed_title: String, pub page: Option<u16>, pub hidden_count: u32 }

// Event (service.rs)
Edition { id: i64, entries: usize, front: usize, week: usize, wire: usize, overflow: u32, unreachable: u32 },
```

Rule for `window_start`: the latest edition's `window_end`; with `rebuild`, the deleted edition's `window_start`; with no edition at all, `window_end − FIRST_WINDOW_HOURS`.

`window_end` and `built_at` are `max(Utc::now(), now)` taken after the refresh: the refresh stamps `fetched_at` with the real clock, so a window ending on an injected `now` could leave the build's own entries out. `unreachable` counts every `feed_done` that carries an error during the refresh (HTTP 404 and 410 included). `Event::Edition` joins the no-op arm of the CLI progress display's exhaustive `match` in `crates/ressac/src/output.rs` in this sub-phase (the workspace gate compiles that crate).

### Phases 3a–3b (CLI)

| Command | `--json` stdout | Text stdout | Exit `2` |
|---|---|---|---|
| `edition build [--cap-week N] [--cap-wire N] [--rebuild]` | the `fetch` events (`started`, `feed_done`, `network_unreachable`, `finished`), then `{"event":"edition","id":…,"entries":…,"front":…,"week":…,"wire":…,"overflow":…,"unreachable":…}` | the `fetch` summary, then `Edition #12: 43 entries (front 2, week 7, wire 34), 61 more left in their feeds.` | `An edition was already built this morning.`; `The network is unreachable: no edition built.` |
| `edition show` | one `{"event":"edition_entry",…}` per `v_edition_today` row, then one `{"event":"edition_overflow",…}` per overflow, keys = view columns | `Front Page` / `This Week` / `The Wire` headings, `rank page title` lines, `+12 more in Actualitté` lines, then the summary line | none; no edition → `No edition yet.` on stdout, exit `0` |
| `feed cadence ID auto\|daily\|weekly\|monthly\|rare\|silent` | `{"event":"cadence","feed_id":12,"group":"daily","computed":"weekly","override":true}` | `#12 Daily (set by you)` or `#12 Weekly (auto)` | `No feed #N.`, `Feed #N is buried.` |
| `page list`, `page set` | unchanged shapes | unchanged | `page set … 104`…`109` → `Page N is reserved.` |

Interrupted (`Ctrl-C`) → exit `130` like `fetch`, no edition. Database errors exit `1`.

- The text helpers live in `crates/ressac/src/commands/edition.rs` and `commands/feed.rs` (not `output.rs`), to keep 3a at five files.
- The `cadence` JSON line is built by the CLI with `json!`; the core has no `Event::Cadence`.
- `edition show --json` with no edition prints nothing and exits `0`; each `edition_entry` line also repeats the edition's own fields; the page of each overflow and entry comes from `Ressac::pages()` (no `page` column in `v_edition_today`).

### Phase 4a (Swift data)

```swift
enum CadenceGroup: String, CaseIterable, Sendable, Codable, Hashable {
    case daily, weekly, monthly, rare, silent
    var title: String { get }        // "Daily" … "Silent"
    var page: Int { get }            // 105 … 109
}

enum EditionSection: String, CaseIterable, Sendable, Codable {
    case front, week, wire
    var title: String { get }        // "Front Page", "This Week", "The Wire"
    var antiopeTitle: String { get } // "FRONT PAGE", "THIS WEEK", "THE WIRE"
}

struct EditionInfo: Equatable, Sendable { var id: Int64; var builtAt: Date; var windowStart: Date; var windowEnd: Date; var unreachable: Int }
struct EditionItem: Equatable, Sendable, Identifiable {
    var entryId: Int64; var section: EditionSection; var rank: Int
    var feedId: Int64; var feedTitle: String; var title: String?
    var publishedAt: Date; var isRead: Bool; var isStarred: Bool
    var id: Int64 { entryId }
}
struct EditionOverflowItem: Equatable, Sendable { var section: EditionSection; var feedId: Int64; var feedTitle: String; var page: Int?; var hiddenCount: Int }
struct Edition: Equatable, Sendable {
    var info: EditionInfo; var items: [EditionItem]; var overflow: [EditionOverflowItem]
    var readCount: Int { get }; var isFinished: Bool { get }   // every item read (false when empty)
}
struct CadenceRow: Equatable, Sendable, Identifiable {
    var feedId: Int64; var effective: CadenceGroup; var computed: CadenceGroup; var isOverride: Bool
    var medianIntervalDays: Double?; var entriesPerMonth: Double; var lastEntryAt: Date?; var unread: Int
    var id: Int64 { feedId }
}

extension Queries {
    static func edition() -> LiveQuery<Edition?>              // latest edition, nil when none
    static func cadenceRows() -> LiveQuery<[CadenceRow]>      // v_cadence
    static func isEditionDue(dayStart: Date) -> @Sendable (Database) throws -> Bool
}

enum EditionClock {
    /// 04:00 local of the day `now` belongs to; mirrors edition::day_start.
    static func dayStart(for now: Date, calendar: Calendar = .current) -> Date
}
```

`AppDatabase.knownSchemaVersion = 4`. All the types above live in the new file `apps/macos/Ressac/Data/Edition.swift`; the record types conform to `RessacRecord` and keep their memberwise initialisers (any `init(row:)` goes in an extension). The query functions are added to `struct Queries` itself.

### Phase 4b (Swift engine)

```swift
extension RunRequest {
    static func buildEdition(rebuild: Bool = false) -> RunRequest   // ["edition", "build"] (+ "--rebuild"), kind .long
    static func setCadence(feedId: Int64, _ group: CadenceGroup?) -> RunRequest  // ["feed", "cadence", id, group?.rawValue ?? "auto"], kind .short
}
// EngineEvent gains:
case edition(id: Int64, entries: Int, overflow: Int, unreachable: Int)   // "edition"
case cadence(feedId: Int64, group: String, isOverride: Bool)            // "cadence"
```

`RunFailure` exit `2` from `edition build` and from `feed cadence` is a refusal (`isRefusal` true): its stderr line is the banner text, without Show Log. `RunCoordinator`'s two exhaustive `switch event` list the two new cases.

### Phase 5a (Swift, `App/JournalModel.swift`)

```swift
@MainActor @Observable
final class JournalModel {
    enum Phase: Equatable { case idle, due, building(done: Int, total: Int), offline, failed(String) }
    private(set) var phase: Phase
    private(set) var edition: Edition?
    init(database: AppDatabase, coordinator: RunCoordinator, clock: @escaping @Sendable () -> Date = Date.init, calendar: Calendar = .current)
    func checkAndBuild() async          // launch, foreground, rollover; no-op while building
    func retry() async
    func scheduleRollover()             // next EditionClock.dayStart after now; replaces the midnight-only path for the Journal
    func cancelRollover()
    static func nextRollover(after: Date, calendar: Calendar) -> Date
    static let builtElsewhere = "An edition was already built this morning."
    var sleep: @Sendable (Duration) async throws -> Void   // injectable for tests
}
// RunCoordinator.editionProgressLabel: String = "Building this morning's edition…" (RunProgress.label of an edition run)
```

`AppState` owns one `JournalModel` (`appState.journal`). `RessacApp`/`AppState` call `checkAndBuild()` at launch, on `NSApplication.didBecomeActiveNotification`, and at the rollover (none of the three while `Launcher.isRunningTests`). The existing `dayDidChange()` for Today stays as is.

- `JournalModel.edition` follows the database live through `Queries.edition()`.
- A failed or killed run gives `.failed(message)`; `.due` means the coordinator turned the run away. Either way the next trigger builds.
- Exit `2` after a `network_unreachable` event gives `.offline` (banner kept); exit `2` with `builtElsewhere` gives `.idle` and takes the coordinator's banner down (spec §9).

### Phase 5b (Swift, sidebar and list)

```swift
enum Source { …; case journal; case cadence(CadenceGroup) }
// Queries.predicate(for: .journal)  → entries of the latest edition (for nextUnread and mark-all-read)
// Queries.predicate(for: .cadence(g)) → feed_id IN (SELECT feed_id FROM v_cadence WHERE effective_group = g)
// SidebarSnapshot gains: var journalUnread = 0; var cadenceUnread: [CadenceGroup: Int] = [:]
```

For `.journal`, the entry list is ordered by the entry's `rank` in the latest edition in SQL and always loads whole (`Queries.floor` and `Queries.nextUnread` return nil for it), so `AppState.entries` holds the edition in rank order and `j`, `k`, Space, `⇧⌘A` work unchanged; `AppState.merge(fresh:previous:retained:floor:rank:)` keeps that order. The Journal view lays these rows out with the sections, overflow rows, header and end line taken from `appState.journal.edition`. Every exhaustive `switch` over `Source` (grep `case .starred` and `case .today` in `apps/macos/Ressac`) gets the two new cases.

### Phase 5c (Swift, inspector and garden)

Garden › All feeds is a sectioned `List`, not a table: "a sortable Rhythm column" becomes a trailing rhythm label on each row (`Daily`, `Daily*` when set by hand) and a `Status | Rhythm` segmented control (`GardenGrouping`) that regroups the rows by rhythm. Both join `Queries.cadenceRows()` to the garden rows by `feedId`; `v_garden_row` does not change. The inspector reads the same `CadenceRow` and runs `RunRequest.setCadence` as a short run through `AppState.setCadence(feedId:_:)`.

```swift
extension GardenModel {
    static func rhythmText(_ cadence: CadenceRow, now: Date) -> String   // "Daily · median 0.4 d · last entry 2 h ago" (+ " (set by you)"); Antiope reuses it
    static func rhythmLine(_ cadence: CadenceRow, now: Date) -> String   // "Rhythm: " + rhythmText
    static func rhythmLabel(_ cadence: CadenceRow?) -> String?
    static func ago(_ date: Date, now: Date) -> String
}
```

### Phase 6 (Swift Antiope)

```swift
enum ReservedPage: Int { …; case journal = 104, daily = 105, weekly = 106, monthly = 107, rare = 108, silent = 109 }
// destinations: .journal → .source(.journal); .daily … .silent → .source(.cadence(.daily …))
// titles mirror pages::list: "Journal", "Daily", "Weekly", "Monthly", "Rare", "Silent"
```

Sub-pages of P104 are numbered by `EditionItem.rank` over `edition.items.count` (6c). Overflow rows are white on blue and open on a click only; `FIN DE L'EDITION` shows at the end of the column when everything is read and as the reader page that SUITE reaches after the last entry; RÉPÉTITION on P104 retries a failed or offline build; with no edition yet and while building, P104 shows `EDITION IN PROGRESS`.

## After this plan

Sub-project 2b (terminal reading) reads `v_cadence`, `v_edition_today` and `v_edition_overflow`; nothing in this plan may depend on the Mac app for the edition to exist.
