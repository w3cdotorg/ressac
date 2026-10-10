# Ressac Core & Garden Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `ressac` binary that imports a Vienna OPML, fetches and diagnoses every feed, lets the user triage them in a terminal "garden", and exports a clean OPML.

**Architecture:**

- **Crates.** A Cargo workspace with two crates. `ressac-core` is a library with no terminal code; it covers storage, OPML, fetching, parsing, diagnosis, garden logic and the service facade. `ressac` is the binary; it covers the clap CLI, progress and JSON output, and the ratatui garden.
- **Data.** One SQLite database (WAL) is the contract with the future SwiftUI app. Diagnosis is a pure function. All SQL lives under `ressac-core/src/store/`.

**Tech Stack:** Rust 1.99 (edition 2024). Libraries: tokio, reqwest 0.13 (rustls), feed-rs 3, quick-xml 0.42, rusqlite 0.40 (bundled), whatlang, scraper, strsim, clap 4, indicatif, ratatui 0.30. Tests: wiremock, insta, tempfile.

**Spec:** `docs/specs/2026-10-10-core-and-garden-design.md`. Executors read the spec and this plan. API findings for the pinned crates are in `notes.md` § "API des crates".

## Phase files

Each phase touches at most 5 files (project rule). Execute in order. Each phase ends with the verification gate below, and with a **stop for the user's explicit approval** before the next phase (project CLAUDE.md), unless the user waives it.

| Phase | File | Deliverable |
|---|---|---|
| 1 | `core-and-garden/phase-01-workspace.md` | Workspace, `ressac-core` crate, URL keys, CI |
| 2 | `core-and-garden/phase-02-store.md` | Domain types, errors, schema v1, store open/migrate |
| 3 | `core-and-garden/phase-03-opml-import.md` | OPML parsing, fixture, idempotent import |
| 4 | `core-and-garden/phase-04-fetch.md` | Feed parsing, polite HTTP source, fetch log & entries |
| 5 | `core-and-garden/phase-05-diagnosis.md` | Pure `diagnose`, cadence, hijack scoring |
| 6 | `core-and-garden/phase-06-links-probes.md` | Duplicates, similar titles, autodiscovery, Wayback client |
| 7 | `core-and-garden/phase-07-service.md` | Health persistence, `Ressac` facade, network guard, end-to-end test |
| 8 | `core-and-garden/phase-08-garden-export.md` | Decisions with undo, garden queue logic, OPML export |
| 9 | `core-and-garden/phase-09-cli.md` | `ressac` binary: import, fetch, check, export |
| 10 | `core-and-garden/phase-10-tui.md` | `ressac garden` ratatui UI with snapshots |
| 11 | `core-and-garden/phase-11-contract-real-run.md` | Schema contract test, real Vienna run, calibration, README |

## Global Constraints

- Rust edition `2024`, `rust-version = "1.99"`. Dependency versions exactly as pinned in the root `Cargo.toml` (Phase 1).
- All user-facing strings in English. Code, comments and docs in English. `notes.md` stays in French.
- Nothing is ever deleted. Burying sets `feed.state = 'buried'`. Entries, logs and buried feeds stay in the database.
- Nothing is applied without a user decision. The only automatic step is diagnosis.
- Diagnosis thresholds are named constants in `ressac-core/src/health/mod.rs`:
  - `ALIVE_WITHIN_DAYS = 90`
  - `FOSSIL_AFTER_DAYS = 730`
  - `DEAD_FAILURE_STREAK = 5`
  - `DEAD_STREAK_MIN_DAYS = 14`
  - `DUPLICATE_OVERLAP = 0.8`
  - `HIJACK_THRESHOLD = 0.6`
  - `SIMILAR_TITLE_DISTANCE = 2`
- Status evaluation and urgency order: Duplicate, Hijacked, Moved, Dead, Ailing, Fossil, Dormant, Alive, Unchecked.
- Fetch limits:
  - concurrency: 16 in flight, 1 per host, 1 s gap between requests to the same host;
  - size and time: 30 s timeout, 10 MB body cap, 5 redirects;
  - User-Agent `Ressac/<crate version>`;
  - feeds fetched less than 30 min ago are skipped unless `--force`.
- Network guard: the run attempts ≥ 4 feeds and > 50 % fail with `Dns` or `Connect`.
- Database: default path `$HOME/Library/Application Support/Ressac/ressac.sqlite`, overridable with `--db` / `RESSAC_DB`. WAL mode. Timestamps are RFC 3339, UTC, millisecond precision, `Z` suffix (`model::ts`).
- Exit codes: `0` success (sick feeds are not a failure), `1` fatal error, `2` usage error (clap default).
- Never commit `~/Documents/vienna_20260415.opml` or any real `.opml`. Only `tests/fixtures/vienna-like.opml` (hand-written) is committed.
- Verification gate after each phase. Run all three, all must pass:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- Before any cargo command: `export PATH="/opt/homebrew/bin:$PATH"`.
- Commit after each task. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Push with `git -c credential.helper= -c credential.helper='!gh auth git-credential' push` (no global git config change).
- Log surprises and API findings in `notes.md` (French) as they happen.

## Review Focus

Input classes the spec implies but does not spell out. Each one has a test pinned in the owning task:

1. **RSS served with `Content-Type: text/html`** (misconfigured servers) must still parse, not be flagged `HtmlInsteadOfFeed`. Test: Phase 4, Task 4.1 `rss_with_html_content_type_still_parses`.
2. **Relative `Location` headers** (`Location: /new.xml`) must resolve against the current URL. Test: Phase 4, Task 4.2 `relative_redirect_is_resolved_and_recorded`.
3. **A UTF-8 BOM or whitespace before `<?xml`** must parse. Test: Phase 4, Task 4.1 `bom_and_leading_whitespace_parse`.
4. **A repair whose new URL is already used by another active feed** must fail with a clear message, without crashing or corrupting anything. Test: Phase 8, Task 8.1 `repair_onto_existing_feed_fails_cleanly`.
5. **A narrow terminal (60×20) and long titles** must render without panicking, with truncation. Test: Phase 10, Task 10.2 snapshot `garden_narrow`.

## Interfaces at a glance

These names are fixed across phases. Each phase restates the ones it uses.

- **`url_key`:** `url_key(&str) -> String`, `repair_url(&str) -> String`, `site_of(&str) -> Option<String>`, `https_variant(&str) -> Option<String>`.
- **`model`:**
  - helpers: `ts`, `parse_ts`, `FeedId`, `FolderId`;
  - enums: `Status` (`ALL`, `urgency`, `as_str`, `parse`, `needs_triage`), `ErrorKind` (`is_network`, `label`, `clue`), `Decision`, `Action` (`is_safe`, `is_repair`, `label`), `Remark`;
  - structs: `Hop`, `Evidence`, `Cadence`, `Diagnosis`.
- **`error`:** `Error`, `Result<T>`.
- **`store::Store`:**
  - lifecycle: `open`, `open_in_memory`, `schema_version`;
  - feeds and import: `import_outlines`, `feeds`, `feed`, `folders`, `folder_paths`;
  - fetching: `record_attempt`, `mark_network_suspect`, `feeds_to_fetch`, `attempts`, `entries`;
  - health and probes: `save_diagnosis`, `diagnosis`, `diagnoses`, `set_https_available`, `set_discovery`, `set_wayback`;
  - decisions: `record_decision`, `undo_last_decision`, `latest_decisions`.
- **`store` free items and records:** `default_path()`, `SCHEMA_VERSION`. Records: `FeedRow`, `FolderRow`, `ImportReport`, `AttemptRecord`, `EntryRecord`, `DecisionRecord`, `FeedSnapshot`.
- **`opml`:** `Outline`, `OpmlFeed`, `parse`, `clean_description`, `export`.
- **`parse`:** `ParsedFeed`, `ParsedEntry`, `parse_feed`, `looks_like_html`.
- **`source`:** `FeedRef`, `FetchAttempt`, `RawResponse`, `FetchLimits`, `Source` (trait), `LocalSource` (`new`, `fetch_raw`, `fetch_one`, `refresh`).
- **`health`:**
  - core: `FeedEvidence`, `diagnose`, `cadence`, `dead_reason`;
  - `hijack::{assess, has_spam}`;
  - `links::{LinkInput, find_duplicates, apply_similar_titles}`.
- **`discover`:** `alternate_feeds`, `same_feed`.
- **`wayback`:** `Wayback::{new, closest}`.
- **`service`:** `Ressac` (`open`, `with_parts`, `store`, `import_opml`, `refresh`, `diagnose_all`, `lookup_wayback`, `summary`, `export_opml`), `RefreshOptions`, `Event`, `EventSink`, `NullSink`, `Summary`.
- **`garden`:** `GardenRow`, `needs_triage`, `load_rows`, `Progress`, `progress`, `safe_repairs`, `decide`, `undo`, `apply_safe_repairs`.
