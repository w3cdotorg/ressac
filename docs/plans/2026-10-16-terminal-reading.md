# Ressac Terminal Reading (Sub-project 2b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A quick keyboard reader in the `ressac` binary (ratatui) for a tmux pane: the Journal first, the rhythm groups, the garden as a destination, real images, Native and Antiope looks, adaptive columns, on the same database and rules as Ressac.app.

**Architecture:**

- **Phases 1–2 (core).** A reading API in `store/library.rs` on the contract views (sources, lists, Journal, search, entry content, read/star/mark-all, `data_version`), and `Ressac::fetch_image` through the guarded client. No schema change (v4).
- **Phases 3–5 (terminal foundations).** `tui/reader/html.rs` (HTML → styled text), `tui/reader/state.rs` + `layout.rs` (pure state machine, adaptive columns), `tui/reader/native.rs` + `mod.rs` (Native rendering, event loop, background jobs), `ressac read`.
- **Phases 6–9 (features).** Images (`images.rs`, `ratatui-image`), the Journal and the edition build, the garden as a reader screen and `ressac` without subcommand, Antiope (`antiope.rs`).
- **Phase 10.** Acceptance with Clément in Ghostty under tmux, docs, push, PR, merge.

**Tech Stack:** Rust edition 2024, ratatui 0.30.2 (already), crossterm through ratatui, `ratatui-image` 11.1.0 (`default-features = false`, features `crossterm`, `image-defaults`), `image` 0.25.10, `scraper` 0.27.0 (already), `insta` 1.49.0 (already), wiremock (already). No Swift change.

**Spec:** `docs/specs/2026-10-16-terminal-reading-design.md` (*the spec*). Earlier specs: core, app, Antiope, edition (see the spec header). Clappuccino reference: `~/Sites/LearningShow16-TUI/crates/wooclap-cli` (`preview.rs` `PreviewState::new`, `app.rs` near `ratatui::init()`).

## Phase files

Each (sub-)phase touches **at most 5 files, test files and snapshot files excluded** (insta `.snap` files do not count). Execute in the order of the table. Each (sub-)phase ends with the Rust gate.

| Order | Phase | File | Deliverable |
|---|---|---|---|
| 1 | 1a | `terminal-reading/phase-01-core-library.md` | `store/library.rs`: `Source`, `ListFilter`, `SourcesSnapshot`, `EntryRow`, `EntryPage`, `ListCursor`, `JournalView`, `Store::sources`, `Store::entries`, `Store::journal`, `Store::search_entries` (`search_ids` public) |
| 2 | 1b | `terminal-reading/phase-01-core-library.md` | `Store::entry_detail`, `set_read`, `set_starred`, `mark_all_read`, `data_version` |
| 3 | 2 | `terminal-reading/phase-02-fetch-image.md` | `Ressac::fetch_image`, limits, errors |
| 4 | 3 | `terminal-reading/phase-03-html.md` | `tui/reader/html.rs`: `render_html` → `RenderedArticle`, insta snapshots |
| 5 | 4a | `terminal-reading/phase-04-state-layout.md` | `tui/reader/layout.rs` (`LayoutClass`, `columns`), `tui/reader/settings.rs` (`TerminalSettings`) |
| 6 | 4b | `terminal-reading/phase-04-state-layout.md` | `tui/reader/state.rs`: `ReaderState`, `Command`, `Event`, Native keys |
| 7 | 5a | `terminal-reading/phase-05-native-loop.md` | `tui/reader/native.rs`: Native rendering, `TestBackend` snapshots at 140/100/70 |
| 8 | 5b | `terminal-reading/phase-05-native-loop.md` | `tui/reader/mod.rs`: event loop, jobs, `data_version` polling, `ressac read` |
| 9 | 6 | `terminal-reading/phase-06-images.md` | `tui/reader/images.rs`: picker after init, cache, half-blocks, teletext quantisation, dependencies |
| 10 | 7 | `terminal-reading/phase-07-journal.md` | Journal view, edition build at launch and at 04:00, offline/refused/failed states |
| 11 | 8a | `terminal-reading/phase-08-garden-entry.md` | Step 0 on `tui/state.rs` and `tui/ui.rs`; garden filters `All`, `Cemetery`; `ui::render_in`; `tui::apply_garden_command` |
| 12 | 8b | `terminal-reading/phase-08-garden-entry.md` | `GardenView` destinations in the reader, garden keys winning, garden lines in the sources column |
| 13 | 8c | `terminal-reading/phase-08-garden-entry.md` | `rhythm.rs` (`RhythmInfo` from `CadenceDetail`, `rhythm_text`), `c` binds 4b's rhythm overlay on garden pages |
| 14 | 8d | `terminal-reading/phase-08-garden-entry.md` | `Cli.command: Option<Command>`, bare `ressac` opens the reader |
| 15 | 9a | `terminal-reading/phase-09-antiope.md` | `tui/reader/antiope.rs` (palette, header clock, Minitel bar, columns, P100/P899), `tui/reader/pages.rs` (page directory), the page list loaded (`Command::LoadPages`, `Event::Pages`) |
| 16 | 9b | `terminal-reading/phase-09-antiope.md` | Page entry, P104 sub-pages, FIN DE L'EDITION, P105–P109, P800–P802, Antiope images |
| 17 | 10 | `terminal-reading/phase-10-acceptance-docs.md` | Manual check (spec §10), README, `notes.md`, `TODO.md`, push, PR, merge |

## Global Constraints

- **Inherited:** Rust edition `2024`, `rust-version = "1.99"`, pins in the root `Cargo.toml`. New crates allowed in this plan: exactly `ratatui-image = { version = "11.1.0", default-features = false, features = ["crossterm", "image-defaults"] }` and `image = "0.25.10"`, added to `[workspace.dependencies]` and used by `crates/ressac` only.
- **Schema:** unchanged, v4. Client-writable columns unchanged: `entry.read_at`, `entry.starred_at` (plus `feed.prefer_full_text`). The terminal writes only through core functions.
- **Language:** every user-facing string in English; exceptions: the Minitel key labels (`SOMMAIRE`, `SUITE`, `RETOUR`, `GUIDE`, `RÉPÉTITION`) and `FIN DE L'EDITION` in Antiope. `notes.md` in French, no em dash in French prose.
- **Exact strings (Native):** Journal header `Friday 16 October · 43 entries · 12 read`, ` · 3 feeds unreachable` / ` · 1 feed unreachable`; sections `Front Page`, `This Week`, `The Wire`; `+12 more in Actualitté`; `That's the edition.`; `Nothing new this morning.`; `No edition yet`; `Offline: no edition yet`; progress `Building this morning's edition… 34/156`, short `Edition… 34/156`; `The database is busy (is Ressac.app writing?)`; `Window too small`; `[image: alt text]`; `[loading image…]`; rhythm line `Rhythm: Daily · median 0.4 d · last entry 2 h ago`, ` (set by you)`; override menu `Auto`, `Daily`, `Weekly`, `Monthly`, `Rare`, `Silent`.
- **Exact strings (Antiope):** sections `FRONT PAGE` yellow, `THIS WEEK` cyan, `THE WIRE` green; `+12 MORE IN ACTUALITTÉ` white on blue; sub-pages `104 3/43`; `FIN DE L'EDITION`; `P437 NOT FOUND` red for 2 s; header `P412 RESSAC … FRI 16 OCT 18:33:12`.
- **Teletext palette:** black `#000000`, red `#FF0000`, green `#00FF00`, yellow `#FFFF00`, blue `#0000FF`, magenta `#FF00FF`, cyan `#00FFFF`, white `#FFFFFF`; blue never colours text; Antiope columns index 22, entries 28 cells, `▐` blue separators.
- **Reserved pages:** 100 Index, 101 Unread, 102 Today, 103 Starred, 104 Journal, 105–109 Daily…Silent, 800 Needs triage, 801 All feeds, 802 Cemetery, 899 Guide (from `store::pages`).
- **Layout classes:** width ≥ 120 → three columns (sources 26, entries 40, reader rest); 80–119 → two (entries 40, reader; sources overlay); < 80 → one; below 40 × 10 → `Window too small`.
- **Text:** reader text wrapped to the column, at most 72 characters; every feed-supplied string through `output::printable` before reaching the terminal.
- **Images:** `Ressac::fetch_image` http/https only, private addresses refused (the core's guarded client), at most `10 * 1024 * 1024` bytes and 15 s; decode limited to 4096 × 4096; at most half the reader height; cache of 32; picker created **after** `ratatui::init()`, Warp forced to iTerm2, an unwrapped DA1 emitted after the query (clappuccino's fixes).
- **Settings:** `~/Library/Application Support/Ressac/terminal.json` `{"look":"native"|"antiope","images":true}`; Native and images on by default; unreadable → defaults; writing never blocks.
- **Idle polling:** `PRAGMA data_version` every 2 s; busy timeout 5 s.
- **Time:** every function depending on "now" or the local zone takes it as a parameter (tests inject a fixed clock and `chrono::FixedOffset`/a test zone); the edition rule is `edition::day_start`.
- **Verification gate (every phase):** `cargo fmt --all` then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (insta: `INSTA_UPDATE=no`; new snapshots reviewed and committed with the code). The Swift suite is untouched by this plan; phase 10 runs it once.
- **Step 0 rule:** before restructuring a file over 300 lines, remove its dead code in a separate commit. Concerned: `crates/ressac/src/tui/state.rs` (616), `tui/ui.rs` (402), `output.rs`, `service.rs`, `store/mod.rs`, `main.rs`, `cli.rs`.
- **Shell:** `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"`.
- **Commits:** one per task on branch `terminal-reading`, messages ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Push:** `git -c credential.helper= -c credential.helper='!gh auth git-credential' push` (phase 10).
- **Tests and the real database:** never run against `~/Library/...`; temp `--db` and `HOME`. Terminal tests use ratatui's `TestBackend` and never touch the real terminal; no simulated keystrokes towards other apps.

## Review Focus

1. **A title or HTML with escape sequences (ESC, OSC, BEL, U+009B) and an emoji/CJK title in a narrow column:** nothing reaches the terminal raw, and rows stay within their column. Phase 3 (`html.rs` snapshots) and phase 5a (`TestBackend` row with `Café ☕ 東京 \u{1b}]0;x\u{7}`).
2. **Ressac.app marks entries read while the reader shows the same list:** within one poll (2 s) the reader's list and counts follow, and the selection does not jump. Phase 1b (`data_version` changes after a write from another connection) and phase 5b (state test: `DataChanged` reload keeps the selected entry id).
3. **The pane is resized from 140 to 70 columns while an article is open, then back:** the reader stays on the same entry and scroll position, the column focus moves sensibly (reader stays the visible column). Phase 4a/4b (layout and state tests).
4. **An image URL that redirects to a private address, answers HTML, is 50 MB, or decodes to 20 000 × 20 000:** refused or placeholder, no hang, bounded memory. Phase 2 (wiremock) and phase 6 (decode limit).
5. **The reader is open across 04:00 and the app builds the edition first:** the reader's build is refused by the core's due re-check and it simply reloads the Journal, no error shown. Phase 7 (state test: `JobDone(EditionNotDue)` while building → Journal reloaded, phase idle, no message).

## Interfaces between phases

Phase files must use these names and signatures; a phase may add private helpers. A deviation forced by the real code is reported in the phase's report and reconciled before the next phase that consumes it.

### Phase 1 (core, `crates/ressac-core/src/store/library.rs`, re-exported from `store`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source { Journal, Unread, Today, Starred, Folder(FolderId), Feed(FeedId), Cadence(CadenceGroup) }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListFilter { #[default] All, Unread }

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceCounts {
    pub journal: usize, pub unread: usize, pub today: usize, pub starred: usize,
    pub needs_triage: usize, pub cadence: BTreeMap<CadenceGroup, usize>, // all five keys present
}
#[derive(Debug, Clone, PartialEq)]
pub enum SourceNodeKind { Folder { id: FolderId, name: String }, Feed { id: FeedId, title: String } }
#[derive(Debug, Clone, PartialEq)]
pub struct SourceNode { pub kind: SourceNodeKind, pub page: Option<u16>, pub unread: usize, pub children: Vec<SourceNode> }
#[derive(Debug, Clone, PartialEq)]
pub struct SourcesSnapshot { pub counts: SourceCounts, pub tree: Vec<SourceNode> } // tree in the app's display order

#[derive(Debug, Clone, PartialEq)]
pub struct EntryRow {
    pub entry_id: i64, pub feed_id: FeedId, pub feed_title: String,
    pub title: Option<String>, pub author: Option<String>, pub url: Option<String>,
    pub published_at: DateTime<Utc>, pub is_read: bool, pub is_starred: bool, pub has_fulltext: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCursor { pub published_at: String, pub entry_id: i64 } // raw stored text, like the app's ListFloor
#[derive(Debug, Clone, PartialEq)]
pub struct EntryPage { pub rows: Vec<EntryRow>, pub total: usize, pub unread: usize, pub next: Option<ListCursor> }
pub const PAGE_SIZE: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct JournalItem { pub row: EntryRow, pub section: Section, pub rank: u32 }
#[derive(Debug, Clone, PartialEq)]
pub struct JournalView { pub edition: EditionRecord, pub items: Vec<JournalItem>, pub overflow: Vec<EditionOverflowRow> }
// items and overflow restricted to active feeds (the app's ruling of 2026-10-16)

#[derive(Debug, Clone, PartialEq)]
pub struct EntryDetail {
    pub row: EntryRow, pub summary_html: Option<String>, pub content_html: Option<String>,
    pub fulltext_html: Option<String>, pub fulltext_error: Option<String>, pub prefer_full_text: bool,
}

impl Store {
    /// `today_start` = local midnight in UTC (caller computes it from its zone).
    pub fn sources(&self, today_start: DateTime<Utc>) -> Result<SourcesSnapshot>;
    pub fn entries(&self, source: Source, filter: ListFilter, after: Option<&ListCursor>, today_start: DateTime<Utc>) -> Result<EntryPage>;
    /// `Source::Journal` in `entries` returns the Journal's rows in rank order (whole, `next` None).
    pub fn journal(&self) -> Result<Option<JournalView>>;
    pub fn search_entries(&self, source: Source, text: &str, after: Option<&ListCursor>, today_start: DateTime<Utc>) -> Result<EntryPage>;
    pub fn entry_detail(&self, entry_id: i64) -> Result<Option<EntryDetail>>;
    pub fn set_read(&self, entry_id: i64, read: bool, now: DateTime<Utc>) -> Result<()>;
    pub fn set_starred(&self, entry_id: i64, starred: bool, now: DateTime<Utc>) -> Result<()>;
    /// Journal: its entries only, never the overflow. Returns how many entries changed.
    pub fn mark_all_read(&self, source: Source, today_start: DateTime<Utc>, now: DateTime<Utc>) -> Result<usize>;
    /// `PRAGMA data_version` of this connection.
    pub fn data_version(&self) -> Result<i64>;
    pub fn search_ids(&self, query: &str) -> Result<Vec<i64>>; // was pub(crate)
}
```

### Phase 2 (core, `service.rs`)

```rust
pub const IMAGE_MAX_BYTES: usize = 10 * 1024 * 1024;
pub const IMAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
impl Ressac { pub async fn fetch_image(&self, url: &str) -> Result<Vec<u8>>; }
// error.rs: #[error("Image refused: {0}")] ImageRefused(String)  // scheme, private address, too large, not an image content-type
```

### Phase 3 (`crates/ressac/src/tui/reader/html.rs`)

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef { pub url: String, pub alt: String, pub line: usize } // line index in `lines` where the image goes
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedArticle { pub lines: Vec<Line<'static>>, pub links: Vec<String>, pub images: Vec<ImageRef> }
pub const MAX_TEXT_WIDTH: u16 = 72;
pub fn render_html(html: &str, width: u16, base_url: Option<&str>) -> RenderedArticle;
```

### Phase 4 (`tui/reader/layout.rs`, `settings.rs`, `state.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Look { Native, Antiope }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Column { Sources, Entries, Reader }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum LayoutClass { Three, Two, One, TooSmall }
pub fn layout_class(width: u16, height: u16) -> LayoutClass;
pub struct ColumnRects { pub sources: Option<Rect>, pub entries: Option<Rect>, pub reader: Option<Rect>, pub top: Rect, pub bottom: Rect }
pub fn columns(area: Rect, class: LayoutClass, focus: Column, look: Look) -> ColumnRects;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TerminalSettings { pub look: Look, pub images: bool } // Look serialised "native"/"antiope"
impl TerminalSettings { pub fn load(path: &Path) -> Self; pub fn save(&self, path: &Path) -> std::io::Result<()>; pub fn default_path() -> PathBuf; }

pub enum Command {
    LoadSources, LoadEntries { source: Source, filter: ListFilter, after: Option<ListCursor> }, LoadJournal,
    Search { source: Source, text: String }, LoadEntry(i64),
    SetRead(i64, bool), SetStarred(i64, bool), MarkAllRead(Source), Extract(i64),
    FetchImage(String), BuildEdition, Refresh(Option<Source>), OpenUrl(String),
    SetCadence(FeedId, Option<CadenceGroup>), Garden(crate::tui::state::Command), SaveSettings, Quit,
}
pub enum Event {
    Sources(SourcesSnapshot), Entries { source: Source, page: EntryPage, appended: bool }, Journal(Option<JournalView>),
    Entry(EntryDetail), ImageReady { url: String }, ImageFailed { url: String },
    Progress { label: String, done: usize, total: usize }, JobDone(JobOutcome),
    DataChanged, Tick { now: DateTime<Utc>, local: DateTime<FixedOffset> }, Resize { width: u16, height: u16 },
}
pub enum JobOutcome { Refreshed, EditionBuilt, EditionNotDue, Offline, Failed(String), Extracted { entry_id: i64, error: Option<String> } }
// EditionNotDue also covers "built by another process meanwhile": the core cannot tell them apart.
pub struct ReaderState { /* private */ }
impl ReaderState {
    pub fn new(settings: TerminalSettings, width: u16, height: u16) -> Self; // opens on Source::Journal
    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command>;
    pub fn on_event(&mut self, event: Event) -> Vec<Command>;
    pub fn look(&self) -> Look; pub fn focus(&self) -> Column; pub fn source(&self) -> Source;
    pub fn selected_entry(&self) -> Option<i64>;
}
```

### Phases 5–9

- `native.rs`: `pub fn render(frame: &mut Frame, state: &ReaderState) -> ReaderMetrics` in 5a, `images: &mut ImageStore` added as third parameter in phase 6; `antiope.rs`: same final signature; `ReaderMetrics { total_rows, viewport_rows }` fed back through `ReaderState::set_reader_metrics`; no render cache (the article is `state.article`). `mod.rs`: `pub fn run(app: &Ressac, db: &Path, settings_path: &Path) -> anyhow::Result<()>`, synchronous; jobs (`jobs.rs`: `spawn`, `Msg`, `ProgressSink`) each run in their own thread with their own `Ressac::open(db)` and current-thread runtime, so `commands::read` calls `run` directly from `main`, like the garden.
- `images.rs`: `pub struct ImageStore` (picker, cache of 32 keyed by `(url, Variant)`, states Loading/Ready/Failed), `ImageStore::disabled()`, `pub enum Variant { Native, Teletext }`, `pub fn quantise_teletext(img: &image::DynamicImage) -> image::DynamicImage`, `pub const TELETEXT: [[u8; 3]; 8]`.
- CLI: `Command::Read` and `commands::read` (5b.3); `Cli.command: Option<Command>` with `None` routed to `commands::read` (8d); `Command::Garden { action: None }` keeps opening the garden alone.
- Rhythm line: `tui/reader/rhythm.rs` (8c): `RhythmInfo` filled from phase 1b's `CadenceDetail`, `rhythm_text(&RhythmInfo, now) -> String`, `rhythm_line`, `ago`; `Command::LoadCadence`, `Event::Cadence(RhythmInfo)`. The override menu is phase 4b's `Overlay::Rhythm` (`RHYTHM_CHOICES`).
- Journal (7): `tui/reader/journal.rs` (`JournalPhase`, `JournalLine`, `journal_lines`, `journal_header`, `placeholder`, `progress_text`, `offers_retry`, `outcome_of`, `EditionProgress`, constants `BUILDING`, `END_LINE`, `EMPTY_LINE`, `NO_EDITION`, `OFFLINE`); `ReaderState::journal_phase()`, `journal()`; phase 7 replaces the body of 4b's `start(now, local)` (same signature) to append `Command::BuildEdition`.
- Garden (8): `GardenView { NeedsTriage, AllFeeds, Cemetery }`, `ReaderState::{garden_view, garden, open_garden}`, `Command::LoadGarden`, `Event::{GardenRows, GardenMessage}`, `tui::state::Filter::{All, Cemetery}`, `GardenState::with_filter`, `tui::ui::render_in`, `tui::apply_garden_command`.
- Antiope (9): `tui/reader/pages.rs` (`PageDirectory`, `PageDestination`, `ReaderPage`, `AntiopeNav`, `NavContext`, `NavOutcome`, `subpage_label`), `ReaderState.pages` + `Command::LoadPages` + `Event::Pages` (9a), `ReaderState.antiope_real_images` (9b).

### Reconciled `ReaderState` API (one rule for every phase)

- **Fields are public** (as `GardenState`): phase 4b declares `settings, width, height, class, focus, source, filter, sources, sidebar, sidebar_index, entries, total, unread, next, journal, selected, detail, article, show_fulltext, scroll, search, search_editing, overlay, message, progress, now, local`. Later phases add their own fields once: 5a the reader metrics, 7 `journal_phase`, `edition_job`, `last_day`, `journal_cursor`, 8 `garden_view`, `garden`, `rhythm`, 9a `pages`, 9b the Antiope navigation and `antiope_real_images`. Nobody redeclares an earlier field.
- **Methods:** 4b `new`, `start(now, local)`, `on_key`, `on_event`, `look`, `focus`, `source`, `selected_entry`, `class`, `go_to`, `select_entry`, `set_focus`; 5a `view()` (the renderers' borrowed `ReaderView`, its `source_lines` built from `sidebar`) and `set_reader_metrics`; 7 `journal_phase`, `journal`; 8 `garden_view`, `garden`, `open_garden`, `rhythm`.
- **Time:** dates and clocks shown come from `state.local` (set by `start` and every `Tick`), never `chrono::Local` in rendering or state code.
- **Other phase-1 names added by the writers:** `CadenceDetail` + `Store::cadence_details()` (over `v_cadence`), `fts_query(&str) -> Option<String>` (public), `EntryPage: Default`; `fetch_image_within` private in `service.rs`; `ImageRefused` reasons are fixed sentences (phase 2).

### Tooling notes

- `cargo-insta` is not installed: create snapshots with `INSTA_UPDATE=always`, review each `.snap` file, commit it; the gate runs with `INSTA_UPDATE=no`.
- No `rusqlite` dev-dependency in the binary: tests that need a second connection open it with `ressac_core::service::Ressac::open(&db)`.
- `tui/reader/mod.rs` carries `#![allow(dead_code)]` from phase 3/4 until 5b wires the loop; 5b removes it.

## After this plan

Possible later: a separate crate for the reader if compile times hurt; separated mosaics; syncing the look with the app.
