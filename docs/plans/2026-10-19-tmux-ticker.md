# Ressac tmux Ticker (Sub-project 5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ressac ticker --tmux` prints, for tmux's status bar, one recent unread entry that no Journal edition showed, rotating every 15 s across feeds; `ressac ticker open rs<id>` (bound to a click) marks it read and opens it in the browser.

**Architecture:** The core gains two store constructors that never create or migrate (`Store::open_existing`, `Store::open_existing_query_only`) and a pool query (`store/ticker.rs`). The binary gains a `ticker` module: a pure, seeded selection with 15-second slots, line formatting for tmux, and a locked JSON state file; `commands/ticker.rs` wires them to `ressac ticker` and `ressac ticker open`, which never write to stderr and always exit 0. No schema change, no Swift change.

**Tech Stack:** Rust edition 2024 (rustc 1.99), rusqlite (existing), `std::fs::File::try_lock` (std, no crate), `unicode-width = "0.2.2"` (already in `Cargo.lock` through ratatui; becomes a direct dependency of `crates/ressac`), `open` (existing), serde/serde_json (existing), tempfile (existing, tests).

**Spec:** `docs/specs/2026-10-19-tmux-ticker-design.md` (*the spec*). Earlier specs: core, edition, terminal (see the spec header).

## Global Constraints

- **Schema:** unchanged, v4. The only write is `entry.read_at`, through `Store::set_read`.
- **Pool rule (spec §4):** active feed (`v_entry`), `read_at IS NULL`, `published_at >= now - 7 days`, in no `edition_entry` row of any edition, URL scheme `http` or `https`, title not blank.
- **Pace:** slot = `unix_seconds.div_euclid(15)`; one entry per slot for every caller.
- **Output (`--tmux`):** `#[range=user|rs<entry_id>]#[bold]<feed>#[nobold] · <title>#[norange]`; `#` doubled, control characters removed; no colour; `--width` default `70`, display columns, `…` on cuts; feed title cut when wider than `width / 2`.
- **Plain output (no `--tmux`):** `<feed> · <title>`, same width rule, no escaping of `#`.
- **Token:** `rs` + decimal entry id, at most 15 bytes in total.
- **State file:** `~/Library/Application Support/Ressac/ticker.json` (next to `terminal.json`), lock `ticker.json.lock`, lock wait 200 ms; written via `ticker.json.tmp` + rename.
- **Store opens:** `open_existing*` never create the file or its directory, never migrate or back up, require `schema_version() == SCHEMA_VERSION`; the query-only one sets `PRAGMA query_only = ON` and a 250 ms busy timeout and must not use `SQLITE_OPEN_READ_ONLY`.
- **Silence:** both ticker commands exit 0 in every case, never write to stderr, log failures with `tracing::warn!` (the log file `~/Library/Logs/Ressac/` is set up by `main`). `ressac ticker` prints exactly one line (possibly empty); `ressac ticker open` prints nothing.
- **Browser in tests:** `RESSAC_NO_BROWSER=1` makes `ticker open` log the URL instead of opening it.
- **Language:** user-facing strings in English; `notes.md` in French, no em dash in French prose.
- **Verification gate (every task):** `cargo fmt --all` then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `INSTA_UPDATE=no cargo test --workspace`.
- **Step 0 rule:** before adding to a file over 300 lines (`store/mod.rs` 597, `cli.rs` 305, `commands/mod.rs` 251 is under), look for dead code; remove it in its own commit, or note "none found" in the task report.
- **Shell:** `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"`.
- **Never** run anything against `~/Library/Application Support/Ressac/ressac.sqlite`; tests use temporary directories.
- **Commits:** one per task on branch `tmux-ticker`, messages ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **A title or feed name containing `#[`, `#(` or `#{`** must show literally and never restyle the bar, open a range or run a command: covered in Task 4 (`escapes_tmux_formats`).
2. **The database being written by Ressac.app or `ressac fetch` during a status redraw** must give an empty line or the current entry within ~250 ms, never a hang: Task 1 (`query_only_open_times_out_quickly_when_locked`) and Task 5 (`ticker_reprints_current_when_the_database_is_busy`).
3. **A WAL database whose `-shm` file is gone** (no other connection open) must still be readable: Task 1 (`query_only_open_reads_a_wal_database_without_shm`).
4. **Two tmux clients calling within the same slot** must show the same entry: Task 3 (`same_slot_reprints_current`).
5. **The pool shrinking under the cycle** (entries read in the app, an edition built at 04:00 taking some of them) must not show a now-ineligible entry or crash: Task 3 (`current_left_the_pool_picks_at_once`, `shown_is_pruned`).

---

### Task 1: Store opens that never create or migrate

**Files:**
- Modify: `crates/ressac-core/src/store/mod.rs` (beside `Store::open`, ~line 79; tests in its `mod tests`)
- Modify: `crates/ressac-core/src/error.rs`

**Interfaces:**
- Produces: `Store::open_existing(path: &Path) -> Result<Store>`, `Store::open_existing_query_only(path: &Path) -> Result<Store>`; `Error::DatabaseMissing(PathBuf)` (`"No database at {0}."`, via `.display()`), `Error::SchemaMismatch { found: i64, expected: i64 }` (`"Database schema v{found} does not match this ressac (v{expected})."`).

- [ ] **Step 0:** dead-code pass on `store/mod.rs` (Step 0 rule).
- [ ] **Step 1: Failing tests** in `store::tests`:
  - `open_existing_refuses_a_missing_file`: path in a fresh tempdir under a missing subdirectory → `Err(Error::DatabaseMissing(_))`; neither the file nor the subdirectory exists afterwards.
  - `open_existing_refuses_another_schema_version`: create with `Store::open`, then `UPDATE schema_meta SET version = 3` through a raw `rusqlite::Connection` → `Err(Error::SchemaMismatch { found: 3, expected: SCHEMA_VERSION })`; version still 3 and no `*.bak-v3` file next to it.
  - `open_existing_can_write`: `open_existing` on a v4 database, `set_read` an entry inserted by SQL → `read_at` set.
  - `query_only_open_cannot_write`: `open_existing_query_only` then an `UPDATE entry …` → `Err`.
  - `query_only_open_reads_a_wal_database_without_shm`: create with `Store::open`, insert a row, drop the store; assert the `-shm` file is absent (remove it if SQLite left it); `open_existing_query_only` then `SELECT count(*) FROM entry` → 1.
  - `query_only_open_times_out_quickly_when_locked`: a raw connection holds `BEGIN EXCLUSIVE`; a query through `open_existing_query_only` returns `Err` in under 1 s (measure with `Instant`).
- [ ] **Step 2:** run `cargo test -p ressac-core store::tests::open_existing` and `…query_only` → FAIL (not defined).
- [ ] **Step 3:** implement. Shared private `fn open_existing_with(path, query_only: bool) -> Result<Store>`: `path.exists()` else `DatabaseMissing`; `Connection::open_with_flags(path, SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_NO_MUTEX | SQLITE_OPEN_URI)` (never `CREATE`); `configure()`; then for query-only `PRAGMA query_only = ON` and `busy_timeout(250 ms)`; compare `schema_version()` with `SCHEMA_VERSION` → `SchemaMismatch`. `Store::open` is unchanged.
- [ ] **Step 4:** run the six tests → PASS; full gate.
- [ ] **Step 5:** commit `Core: store opens that never create or migrate (ticker)`.

### Task 2: The ticker pool

**Files:**
- Create: `crates/ressac-core/src/store/ticker.rs`
- Modify: `crates/ressac-core/src/store/mod.rs` (`mod ticker;` and `pub use ticker::TickerEntry;`)
- Modify: `crates/ressac-core/src/store/library.rs` (only the visibility of its test helpers: `mod tests` → `pub(super) mod tests`, `add_feed`/`add_entry`/`add_edition` → `pub(in crate::store)`)

**Interfaces:**
- Consumes: Task 1 nothing; existing `Store::conn()`, `model::{ts, parse_ts, FeedId}`.
- Produces:
  ```rust
  pub const TICKER_WINDOW_DAYS: i64 = 7;
  #[derive(Debug, Clone, PartialEq)]
  pub struct TickerEntry { pub entry_id: i64, pub feed_id: FeedId, pub feed_title: String, pub title: String, pub url: String }
  impl Store {
      pub fn ticker_pool(&self, now: DateTime<Utc>) -> Result<Vec<TickerEntry>>;   // newest first
      pub fn ticker_entry(&self, entry_id: i64) -> Result<Option<TickerEntry>>;     // same rules except "unread" and "recent", for `ticker open`
  }
  ```

- [ ] **Step 1: Failing tests** in `store::ticker::tests`, with `now = 2026-10-19T12:00:00Z`, one active feed 1, one buried feed 2, entries given a URL by `UPDATE entry SET url = …` after `add_entry`:
  - `pool_keeps_only_eligible_entries`: entries 1 (eligible, published 2026-10-18), 2 (read), 3 (published 2026-10-12T11:59:59Z, just over 7 days), 4 (in an old edition id 1), 5 (in the latest edition id 2), 6 (feed 2, buried), 7 (`mailto:x@example.org`), 8 (title `"   "`), 9 (eligible, published 2026-10-12T12:00:00Z, exactly 7 days) → `ticker_pool(now)` ids `[1, 9]` in that order.
  - `pool_rows_carry_feed_and_url`: entry 1's `TickerEntry` has `feed_title == "Feed 1"`, its URL and title.
  - `ticker_entry_finds_a_read_entry_but_not_a_buried_one`: entry 2 → `Some`; entry 6 → `None`; entry 7 → `None`; id 999 → `None`.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement both with one SQL each over `v_entry` joined to `entry` (for `read_at`), `NOT EXISTS (SELECT 1 FROM edition_entry ee WHERE ee.entry_id = …)`, `published_at >= ts(now - 7 days)`, `(url LIKE 'http://%' OR url LIKE 'https://%')`, `trim(title) <> ''`; `ORDER BY published_at DESC, entry_id DESC LIMIT 2000`. Read rows with the existing timestamp helpers.
- [ ] **Step 4:** run → PASS; full gate.
- [ ] **Step 5:** commit `Core: the ticker pool`.

### Task 3: Selection and slots (pure)

**Files:**
- Create: `crates/ressac/src/ticker/mod.rs`, `crates/ressac/src/ticker/pick.rs`
- Modify: `crates/ressac/src/main.rs` (`mod ticker;`)

**Interfaces:**
- Consumes: `ressac_core::store::TickerEntry`.
- Produces (in `ticker::pick`, re-exported from `ticker`):
  ```rust
  pub const SLOT_SECONDS: i64 = 15;
  pub fn slot(now: DateTime<Utc>) -> i64;                       // now.timestamp().div_euclid(15)
  pub struct Rng(u64);                                          // SplitMix64
  impl Rng { pub fn new(seed: u64) -> Rng; pub fn below(&mut self, n: usize) -> usize; }
  #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
  pub struct TickerState { pub shown: Vec<i64>, pub last_feed: Option<i64>, pub current: Option<i64>, pub slot: Option<i64> }
  pub fn next(pool: &[TickerEntry], state: &TickerState, now_slot: i64, rng: &mut Rng) -> (Option<TickerEntry>, TickerState);
  ```
  `next` is the whole spec §6 rule: prune `shown` to ids in `pool`; if `state.slot == Some(now_slot)` and `current` is in the pool, return it unchanged; otherwise pick (candidates = pool − shown, new cycle when empty; feed uniformly among candidate feeds other than `last_feed` when one exists; entry uniformly within it), push to `shown`, set `last_feed`, `current`, `slot = Some(now_slot)`. Empty pool → `(None, TickerState::default())`.

- [ ] **Step 1: Failing tests** (`ticker::pick::tests`, pool built from a helper `entry(id, feed)`):
  - `empty_pool_gives_nothing_and_clears_state`.
  - `no_repeat_within_a_cycle`: pool of 6 over 3 feeds, 6 calls on successive slots → 6 distinct ids; 7th call starts a new cycle (`shown.len() == 1`).
  - `feeds_alternate_when_possible`: feed A with 5 entries, feed B with 1, feed C with 1 → over the first 3 picks no two consecutive picks share a feed.
  - `a_big_feed_does_not_dominate`: feed A 40 entries, feed B 1 → with `Rng::new(7)`, over 10 successive slots B appears at least once in the first 2 picks (feed-first choice), and A never appears twice in a row while B has a candidate.
  - `same_slot_reprints_current`: two calls with the same `now_slot` → same entry, state unchanged.
  - `current_left_the_pool_picks_at_once`: same slot, `current` removed from the pool → a different entry.
  - `shown_is_pruned`: `shown` holds ids absent from the pool → they are gone from the returned state.
  - `slot_boundaries`: `slot(2026-10-19T12:00:14Z) == slot(2026-10-19T12:00:00Z)`, `slot(…12:00:15Z)` is one more.
  - `rng_is_deterministic`: two `Rng::new(42)` give the same 10 `below(100)` values, all `< 100`.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement as specified (iterate feeds in ascending id order before drawing so results depend only on the seed).
- [ ] **Step 4:** run → PASS; full gate.
- [ ] **Step 5:** commit `Ticker: rotation by feed and 15-second slot`.

### Task 4: Line formatting and tokens (pure)

**Files:**
- Create: `crates/ressac/src/ticker/line.rs`
- Modify: `crates/ressac/src/ticker/mod.rs`, `Cargo.toml` (`unicode-width = "0.2.2"` in `[workspace.dependencies]`), `crates/ressac/Cargo.toml` (`unicode-width.workspace = true`)

**Interfaces:**
- Produces (in `ticker::line`, re-exported):
  ```rust
  pub const DEFAULT_WIDTH: usize = 70;
  pub fn line(entry: &TickerEntry, width: usize, tmux: bool) -> String;
  pub fn token(entry_id: i64) -> String;              // "rs{id}"
  pub fn parse_token(token: &str) -> Option<i64>;     // "rs" + ASCII digits, total ≤ 15 bytes, id > 0
  ```

- [ ] **Step 1: Failing tests** (`ticker::line::tests`):
  - `plain_line`: feed `Libération`, title `Le budget 2027 adopté` → `Libération · Le budget 2027 adopté`.
  - `tmux_line_wraps_in_a_range`: id 4521 → `#[range=user|rs4521]#[bold]Libération#[nobold] · Le budget 2027 adopté#[norange]`.
  - `escapes_tmux_formats`: title `50#[fg=red] #(rm -rf ~) #{pane_id}` → tmux line contains `50##[fg=red] ##(rm -rf ~) ##{pane_id}`; no single `#` outside the four markup sequences (check by removing the markup and asserting every remaining `#` is doubled).
  - `controls_are_removed`: title `a\u{1b}[31mb\nc\td` → visible text `a[31mbcd`.
  - `cuts_by_display_width`: title of 100 ASCII characters, width 70 → visible text width exactly 70, ends with `…`; a title of CJK characters (`日本語のニュース` repeated) and one with an emoji are cut on a character boundary with width ≤ 70.
  - `long_feed_title_is_cut_to_half`: feed title of 50 characters, width 70 → feed part width ≤ 35, ends with `…`.
  - `token_round_trip`: `parse_token(&token(4521)) == Some(4521)`; `None` for `"rs"`, `"rs-1"`, `"rs0"`, `"rx12"`, `"rs12a"`, `"rs1234567890123"` (16 bytes); `token(9_999_999_999_999).len() == 15`.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement; visible text measured with `unicode_width::UnicodeWidthStr`; markup added after cutting, escaping applied to the cut text (so `##` never counts as two columns).
- [ ] **Step 4:** run → PASS; full gate.
- [ ] **Step 5:** commit `Ticker: tmux line and click token`.

### Task 5: State file, `ressac ticker` and `ressac ticker open`

**Files:**
- Create: `crates/ressac/src/ticker/state_file.rs`, `crates/ressac/src/commands/ticker.rs`
- Modify: `crates/ressac/src/cli.rs` (Step 0 first), `crates/ressac/src/commands/mod.rs` (`pub mod ticker;`), `crates/ressac/src/main.rs` (dispatch)
- Test: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  ```rust
  // ticker::state_file
  pub fn default_path() -> PathBuf;                                        // store::default_path().with_file_name("ticker.json")
  pub fn load(path: &Path) -> TickerState;                                 // missing or corrupt → default
  pub fn save(path: &Path, state: &TickerState) -> std::io::Result<()>;    // .tmp + rename
  pub fn lock(path: &Path, wait: Duration) -> Option<std::fs::File>;       // ticker.json.lock, try_lock polled every 20 ms
  // cli.rs
  Command::Ticker { #[arg(long)] tmux: bool, #[arg(long, default_value_t = 70)] width: usize, #[command(subcommand)] action: Option<TickerCommand> }
  enum TickerCommand { /// Mark the clicked entry read and open it in the browser
                       Open { token: String } }
  // commands::ticker
  pub fn show(g: &Globals, state_path: &Path, tmux: bool, width: usize, now: DateTime<Utc>, out: &mut impl Write) -> anyhow::Result<ExitCode>;
  pub fn open(g: &Globals, token: &str, now: DateTime<Utc>, opener: impl FnOnce(&str) -> std::io::Result<()>) -> anyhow::Result<ExitCode>;
  ```
  `show`: take the lock (200 ms); open `open_existing_query_only`; `ticker_pool(now)`; `next(…, slot(now), &mut Rng::new(seed from now's nanoseconds))`; save; print `line(…)` or an empty line. If the lock cannot be taken, reprint `current` from `load()` when it is still in the pool (or an empty line if the pool cannot be read), without saving. Any error → `tracing::warn!` + empty line. Always `Ok(ExitCode::SUCCESS)`.
  `open`: `parse_token`; `open_existing`; `ticker_entry`; `set_read(id, true, now)`; then `opener(url)`; every failure → `tracing::warn!`, nothing printed, `Ok(ExitCode::SUCCESS)`. `main` passes `open::that` unless `RESSAC_NO_BROWSER=1`, in which case a closure that logs the URL. `--json` with `ticker` is ignored (one line is the output).

- [ ] **Step 0:** dead-code pass on `cli.rs`.
- [ ] **Step 1: Failing tests.**
  - `state_file` unit tests: `round_trip`; `corrupt_file_means_default` (`"{not json"`); `missing_file_means_default`; `lock_is_exclusive` (second `lock` with 50 ms wait → `None` while the first guard lives, `Some` after drop).
  - `commands::ticker` unit tests on a tempdir database built with `Store::open` plus SQL rows (two feeds, three eligible entries with URLs): `two_calls_in_the_same_slot_print_the_same_line`; `a_call_in_the_next_slot_prints_another_feed`; `ticker_reprints_current_when_the_database_is_busy` (a raw connection holds `BEGIN EXCLUSIVE` after a first call → second call, same slot, prints the same line in under 1 s); `ticker_reprints_current_when_the_lock_is_held` (hold `lock()` → same line, state file unchanged).
  - `commands::ticker::open` unit tests: `open_marks_read_then_opens` (opener records the URL; `read_at` set; recorded URL equals the entry's); `open_with_a_bad_token_changes_nothing` (`"rsx"`, opener not called); `open_on_a_missing_database_creates_nothing`.
  - `tests/cli.rs`: `ticker_on_a_missing_database_prints_an_empty_line` (`ressac ticker --tmux` → exit 0, stdout `"\n"`, stderr empty, no database file created); `ticker_open_is_silent` (`RESSAC_NO_BROWSER=1 ressac ticker open rs1` on a missing database → exit 0, stdout and stderr empty).
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement as specified; `main` dispatch: `Command::Ticker { tmux, width, action: None } => commands::ticker::show(&g, &ticker::state_file::default_path(), tmux, width, Utc::now(), &mut out)`, `Some(TickerCommand::Open { token }) => commands::ticker::open(&g, &token, Utc::now(), opener)`.
- [ ] **Step 4:** run → PASS; full gate.
- [ ] **Step 5:** commit `Ticker: ressac ticker and ressac ticker open`.

### Task 6: Docs, acceptance, merge

**Files:**
- Modify: `README.md` (a "tmux ticker" section after "The terminal reader"), `notes.md` (acceptance record), `TODO.md` (sub-project 5 line and anything deferred)

- [ ] **Step 1:** README: what the ticker shows (spec §1, §4), the three tmux lines of spec §7 with `/Applications/Ressac.app/Contents/Helpers/ressac` and a note to use the local build's path, "before your plugin manager's `run` line", how to remove it.
- [ ] **Step 2:** Release build of the app from the branch (README "The Mac app" commands, derived data in the scratchpad); quit Ressac before replacing `apps/macos/build/Ressac.app` — **only with Clément's go** (it is the app he runs).
- [ ] **Step 3:** acceptance with Clément (spec §9): item 1 on a copy of the real database (`--db`), items 2–5 live. Claude shows the `~/.tmux.conf` diff and edits it **only after Clément's go**, then `tmux source-file ~/.tmux.conf` and checks `tmux show -gv status-right` starts with `#(…continuum_save.sh)`.
- [ ] **Step 4:** `notes.md` record (French), `TODO.md`; full gate plus `xcodebuild … test` once (no Swift change expected; it guards the helper embedding).
- [ ] **Step 5:** push, PR, merge **only with Clément's go**.
