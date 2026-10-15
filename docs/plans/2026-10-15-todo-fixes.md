# Ressac TODO Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the items deferred in `TODO.md` (« À décider » and « Reporté », 15/10/2026), except sub-projects 3 and 2b and the items Clément kept for later.

**Architecture:** These are targeted fixes, each in the layer that owns the behaviour.

- **Rust core:** network guards, pacing, triage, the Cemetery query.
- **CLI:** the module split, exit codes, JSON output.
- **SwiftUI app:** paged entry lists, sanitiser bounds, image sizes, launch, refusals shown in the garden.
- **CI:** Xcode 27 image, checkout on Node 24, test-count check.

No schema change: the database stays at v3.

**Tech Stack:**

- **Rust:** edition 2024, rusqlite 0.40.2, reqwest, clap. No new crate.
- **Swift:** Swift 6.4, SwiftUI, GRDB 7.11.1, WKWebView, xcodegen.
- **CI:** GitHub Actions.

**Spec:** this plan has no spec of its own. The requirements are the items of `TODO.md`, plus Clément's decisions below. Where a fix touches specified behaviour, the governing specs are:

- `docs/specs/2026-10-10-core-and-garden-design.md` (core spec);
- `docs/specs/2026-10-10-mac-app-design.md` (app spec);
- `docs/specs/2026-10-14-antiope-design.md` (Antiope spec).

## Decisions (Clément, 15/10/2026)

- **Tailscale CGNAT (100.64/10):** still refused for full articles. The TODO line goes, and no code changes.
- **Spam hint:** shown only on feeds that are not Alive (Task 3.3).
- **CI:**
  - add `|| true` to the two Xcode-detection lines;
  - move the macOS job to `runs-on: xcode-27`, the preview image with macOS 27 and Xcode 27.0, so the app is really built and tested;
  - rollback: one line back to `macos-latest` (Task 7.5).
- **Cemetery unread count:** yes, with no schema change (Phase 04).
- **Exit 2 on an address clash:** the app shows the CLI's sentence in the garden banner and logs at error level. Other exit-2 cases stay fault-level bugs (Task 3.7).
- **Out of this plan:**
  - Antiope scroll hitches on large folders (postponed);
  - the "reentrant NSTableView delegate" warning (needs a trace first);
  - the optional `ressac` link in the `PATH`.

## Phase files

Execute the (sub-)phases **in this order**. Each ends with its verification gate. The project rule "wait for Clément's approval after each phase" applies unless he waives it.

| Order | Phase | File | Files | Deliverable |
|---|---|---|---|---|
| 1 | 01 | `todo-fixes/phase-01-network.md` | 4 | `discover_one` uses the guarded client (no blind SSRF); the guarded client ignores proxy variables (`.no_proxy()`); `HostPacer` remembers every host and counts feed fetches |
| 2 | 02 | `todo-fixes/phase-02-cli-split.md` | 5 | Step 0 check, then `main.rs` (667 → ~103 lines) split into `cli.rs` and `commands/{mod,network,garden}.rs`; moved code byte-identical |
| 3 | 3a | `todo-fixes/phase-03-core-cli-fixes.md` | 5 | Keep, repair or undo onto a taken address exits 2; `apply-safe --json` adds `applied` and `skipped`; no spam hint on Alive feeds |
| 4 | 3b | same | 5 | A changed author reaches `entry` and the search index; `recompute_triage` reads one light row per feed; triage computed once per refresh |
| 5 | 3c | same | 4 | App: exit 2 from `garden decide` or `garden undo` is a refusal (CLI sentence in the banner, error-level log) |
| 6 | 4a | `todo-fixes/phase-04-cemetery-unread.md` | 4 | The Cemetery query counts unread entries on `entry`; `GardenRow.unread`; contract test |
| 7 | 4b | same | 5 | "12 unread" on buried rows (native), a yellow count on P802 (Antiope) |
| 8 | 05 | `todo-fixes/phase-05-entry-list-pages.md` | 5 | `EntryRow` decoded by hand; list queries paged at 500 rows, with whole-list totals; `AppState` loads one page at a time (a read mark costs ~5 ms instead of ~213 ms on 5,907 rows) |
| 9 | 06 | `todo-fixes/phase-06-reader-bounds-and-image-sizes.md` | 5 | Sub-page line over the whole list ("412 3/5907"); next page as the last rows appear (both looks); depth guard by libxml's end-tag rule; attribute bound; one parse worker at a time; cap 256 → 512 KB; feed-given image sizes kept (both looks) |
| 10 | 07 | `todo-fixes/phase-07-launch-and-ci.md` | 5 | Missing helper → fatal `NSAlert`; one launch at a time; `onDatabaseChange` test; native banner and `GardenInspector` through `AppState.openURL`; CI on `xcode-27`, `|| true`, `actions/checkout` on Node 24, declared-vs-ran test count |
| 11 | 08 | `todo-fixes/phase-08-antiope-minors.md` | 5 | One `DateFormatter` per pattern and time zone; P100 rows and the banner keys out of the Full Keyboard Access loop |
| 12 | 09 | `todo-fixes/phase-09-close-todo.md` | 3 | `TODO.md` ticked or cleaned, a dated note in `notes.md`, README (exit codes, `apply-safe --json`, paged lists) |

**Checked on 15/10/2026.** Every phase was applied in this order to a scratch copy of `main` at `855cd2c`, and every TDD step's fail/pass expectation was observed.

- **Swift test counts** (zero warnings throughout):

  | After | Tests |
  |---|---|
  | 3c | 320 |
  | 05 | 333 |
  | 06 | 343 |
  | 07 | 345 |
  | 08 | 347 (84 suites) |

- **Rust, final:** core 260 (+1 ignored), `article_proxy` 1, `contract` 9, `end_to_end` 4, `ressac` 35 + 19.
- **CI count step:** a local dry run printed "347 declared, 347 ran".

## Global Constraints

- **Rust:**
  - edition `2024`, `rust-version = "1.99"`, versions pinned in the root `Cargo.toml`, no new crate;
  - gate: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- **Swift:**
  - Swift 6.4 with complete strict concurrency, macOS 27.0, `SWIFT_TREAT_WARNINGS_AS_ERRORS = YES`, no third-party dependency other than GRDB `7.11.1`;
  - gate: the Rust gate plus `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test`, with zero warnings;
  - there is no `tsc` or `eslint` in this project.
- **Schema:** stays v3. No migration, and the client-writable columns are unchanged.
- **Phase size:** at most 5 files per (sub-)phase, test files included.
- **Step 0:** before restructuring a file over 300 lines, remove its dead code in a separate commit. This concerns `main.rs`, `AppState.swift` and `ArticleTemplate.swift`; each phase checks the line count.
- **Language:** English for code, docs and user-facing strings, except the Antiope Minitel labels. `notes.md` and `TODO.md` are in French, with no em dash in French prose.
- **Commits:**
  - one per task, on branch `todo-fixes`;
  - messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`;
  - never commit a real `.opml`, `.superpowers/` or `apps/macos/Ressac.xcodeproj`.
- **Push:** `git -c credential.helper= -c credential.helper='!gh auth git-credential' push`, only when Clément asks.
- **Tests:**
  - never touch the real database under `~/Library` (temp dirs, `HOME` set to a temp dir);
  - no network: use local mock servers and `URLProtocol` stubs;
  - no simulated keystrokes (`CGEvent.post*`, System Events): in-process tests only.
- **Shell:** run `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"` before any command.

## Review Focus

Conditions no task's tests exercise directly, most likely first. Each has its check in the owning phase.

1. **A folder of several thousand entries in "Unread only" while entries are being marked read.**
   - Expected: the selection stays put, the sub-page line ("3/5907") and the sidebar counts stay right, and the list never jumps back to page one.
   - Owner: Phase 05 Task 5.3 tests, then Phase 06 Task 6.1.
2. **A feed or page whose server sits behind a proxy, or announces a feed on `localhost` or a private address.**
   - Expected: autodiscovery finds nothing rather than fetching it, and feeds still use the proxy.
   - Owner: Phase 01 Tasks 1.1 and 1.2.
3. **The first push on the `xcode-27` preview image.**
   - Expected: `brew install xcodegen`, the WebKit and font tests and the count step pass. If the image fails, revert to `macos-latest` in one line.
   - Owner: Phase 07 Task 7.5 notes.
4. **A garden decision refused because another active feed already uses the address.**
   - Expected: the banner shows the CLI's sentence in both looks, with no fault log and no generic alert. Other exit-2 calls still log a fault.
   - Owner: Task 3.7.
5. **An article mixing unclosed inline tags inside block tags (`<b><div>…`) near the cap.**
   - Expected: rendered or downgraded to text in well under a second, with no overlapping parse threads.
   - Owner: Phase 06 Tasks 6.3 and 6.5.

## Notes

- **Phase 01** renames nothing. The names `articles`, `for_articles`, `allow_private_articles` and `RESSAC_ALLOW_PRIVATE_ARTICLES` now also cover autodiscovery; a rename belongs with sub-project 2b.
- **Between Phases 05 and 06:**
  - the native list and Antiope's entry column go past the first 500 rows only by keyboard;
  - Antiope's sub-page line counts only the loaded rows.

  Phase 06 closes both gaps.
- **Out of scope**, recorded in the phase notes:
  - two remaining direct `NSWorkspace` calls (`ReaderView.swift`, `SettingsView.swift`);
  - the hidden ⌘Z Undo button's focusability (`GardenPageViews.swift`);
  - the https probe right after its own feed fetch;
  - redirect hosts in the pacer;
  - on-demand `extract_entry`.
