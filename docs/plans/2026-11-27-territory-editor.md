# The territory editor in the Sea Chart: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** in both apps' Sea Chart, a `✒ Survey` mode edits territories, continents and seas directly (drag, freehand loop, Cartographer's Log), each edit one transaction in the database, undoable with ⌘Z / Ctrl+Z; the same edits exist as `ressac chart` subcommands.

**Architecture:** the core gets one entry point, `Store::apply_chart_edit(&ChartEdit)`, which compiles each edit into a *patch* (a list of changes, each with its exact before and after), checks every `before` against the database, applies it in one transaction and returns the inverse patch. Undo and redo are just more patches, so another writer's changes are never reverted and a stale undo is refused (spec rule 3, rule 4). The undo stack lives in the shared page (one implementation for both apps, scoped to the open window); the apps only carry `{type:"edit"}` messages to the core (Mac: `ressac --json chart apply <EDIT>`; KDE: in-process on the short lane) and hand the result back with `Chart.edited(result)`. The snapshot gains ids and a `ledger` (every sea and land, empty ones included) without changing `version` 1.

**Tech Stack:** Rust (rusqlite, serde, clap, insta), plain JavaScript in the chart page (d3, `node --test`), Swift/SwiftUI/WebKit (Swift Testing, xcodegen), cxx-qt and QML (QtWebEngine over QWebChannel), the Fedora dev container.

**Spec:** `docs/specs/2026-11-27-territory-editor-design.md` (approved by Clément 27/11/2026; its « Rules » are binding).

## Global Constraints

- UI strings, exactly: `✒ Survey` (toolbar, next to `Key`), `Cartographer's Log`, `Name this land`, `Make a continent`, `Leave Land`, `New Land…`, `New Sea…`, `Lands`, `Seas`, `Sea:`, `Land:`. Undo titles read `Undo <label>` / `Redo <label>`, e.g. `Undo Join APPLE PRESS`.
- Messages, exactly: `The chart changed; nothing was done.` · `A land with that name already exists.` · `A sea with that name already exists.` · `Part of the continent <NAME>: move it to another sea, or turn the continent back into open water.` · `Delete the sea SILICON SHORES? Its 12 feeds will be uncharted, and its continent removed.`
- Names: territory, continent and sea names are trimmed, 1 to 40 characters, shown in small capitals, unique among lands and among seas, compared without case.
- A new sea's key: "the name in lower case with runs of non-alphanumerics turned to `-`, trimmed of `-`; if that key is taken, empty, `none` or starts with `-`, a number is appended (`comics-2`)". Theme keys follow the import rules (not empty, not `none`, not starting with `-`).
- Undo: ⌘Z / ⇧⌘Z on the Mac, Ctrl+Z / Ctrl+Shift+Z in the KDE app, as many steps as were made while the chart window is open.
- Rule 1: every edit keeps a feed in at most one territory; imports still accept what they accept today. Rule 2: continents follow the sea (a continental island only changes sea; `Leave Land` disabled with the hint). A sea has at most one continent.
- Anti-goals: no edit touches a feed, theme or territory it was not aimed at; nothing moves or changes unless Survey mode is on (undo and redo included); feed titles and names stay text everywhere (`textContent`, canvas text, `<option>` text; never `innerHTML`); every gesture has a keyboard path in the log.
- No schema change: the chart tables of `schema/0005_chart.sql` stay as they are. Snapshot `version` stays 1 (fields are only added); the import/export format does not change.
- Reduce Motion (macOS) and Plasma's instant animations: no animated ink, spring-back or drawing flourish.
- `chart-core.js` stays pure (no DOM). No new font face (the WebKit test pins 8 faces). No network in tests, never the real database.
- Copy this plan decides (not in the spec): `Log` (toolbar), `Island:`, `With:`, `Rename`, `Delete`, `Back to Open Water`, `Cancel`, `A name has 1 to 40 characters.`, `A new land needs at least two feeds.`, and the edit labels of the table below.
- Plans and UI in English; `TODO.md` and `notes.md` lines in French, no em dash.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb`. Never push, open a PR, merge or tag.
- Gates: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; `node --test 'apps/macos/ChartTests/*.test.cjs'`; `apps/linux/dev/dev.sh check` ending `check-linux: OK (fmt, clippy, test, chart, layout smoke)`; for Swift tasks `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' -derivedDataPath "$TMPDIR/ressac-dd" test` → `** TEST SUCCEEDED **` (about 10 minutes; run it once per Swift task).

## Names decided here

**Core** (`crates/ressac-core/src/store/chart_edit.rs`, re-exported from `store`):
- `LandRef { id: i64, name: String }`, `SeaRef { id: i64, key: String, sea: String }`: what the page saw; the core checks every field (rule 4).
- `ChartEdit`: serde `#[serde(tag = "op", rename_all = "snake_case")]`, variants below.
- `Change`: serde `#[serde(tag = "kind", rename_all = "snake_case")]`: `Sea { before: Option<SeaRow>, after: Option<SeaRow> }`, `Land { before: Option<LandRow>, after: Option<LandRow> }`, `Member { feed: FeedId, land: i64, before: Option<i64>, after: Option<i64> }` (positions), `FeedSeas { feeds: Vec<FeedId>, before: Option<i64>, after: Option<i64> }` (theme ids; grouped so a large sea's undo stays small). `SeaRow { id, key, name, sea, position }`, `LandRow { id, name, continent: Option<i64> }` (a theme id makes it a continent).
- `ChartEditDone { label: String, undo: ChartEdit }` (always `ChartEdit::Patch`), with `fn event(&self) -> serde_json::Value` = `{"event":"chart_edit","label":…,"undo":…}`, the one shape the CLI prints and both apps hand to the page.
- `Store::apply_chart_edit(&self, edit: &ChartEdit) -> Result<ChartEditDone>`.
- `Error` variants: `ChartChanged`, `LandNameTaken`, `SeaNameTaken`, `NameLength`, `PartOfContinent { name: String }`, with the texts of Global Constraints.
- `MAX_EDIT: usize = 64 * 1024`: the most bytes of edit JSON any path accepts (page, both bridges, CLI).

| `op` | fields | label | refused with |
|---|---|---|---|
| `join` | `feed`, `land: LandRef` | `Join <LAND>` | `ChartChanged` (feed or land gone, land renamed or a continent, feed already in it), `PartOfContinent` |
| `leave` | `feed`, `land` | `Leave <LAND>` | `ChartChanged` (feed not in it), `PartOfContinent` |
| `found` | `name`, `feeds: [id]` (chart order) | `New Land <NAME>` | `NameLength`, `LandNameTaken`, `PartOfContinent`, `ChartChanged` (a feed gone), `Invalid("A new land needs at least two feeds.")` |
| `rename_land` | `land`, `name` | `Rename <NAME>` | `NameLength`, `LandNameTaken`, `ChartChanged` |
| `delete_land` | `land` | `Delete <NAME>` | `ChartChanged` |
| `make_continent` | `sea: SeaRef`, `name` | `Make Continent <NAME>` | `NameLength`, `LandNameTaken`, `ChartChanged` (sea gone, renamed, or already has one) |
| `set_sea` | `feed`, `from: SeaRef?`, `to: SeaRef?` | `Move to <SEA>` (`UNCHARTED WATERS` for null) | `ChartChanged` (feed's theme is not `from`, `to` gone, `from` = `to`) |
| `new_sea` | `name` | `New Sea <NAME>` | `NameLength`, `SeaNameTaken` |
| `rename_sea` | `sea`, `name` | `Rename <NAME>` | `NameLength`, `SeaNameTaken`, `ChartChanged` |
| `order_seas` | `seas: [theme id]`, all of them, new order | `Reorder Seas` | `ChartChanged` (not exactly the stored ids) |
| `delete_sea` | `sea` | `Delete <SEA>` | `ChartChanged` |
| `patch` | `label`, `changes: [Change]` | its `label` | `ChartChanged` (any `before` differs), the name refusals |

Decisions inside the table: a moved feed leaves every territory it was in, and a territory left with fewer than two member rows is deleted (rule 1, rule 6); `join` appends at the end; `set_sea` never touches memberships (an island dropped in another sea keeps its land); `rename_sea` changes `sea_name` only; `new_sea` stores the trimmed name as both `name` and `sea_name`, at the last position; a sea name clashes with any other theme's `sea_name` or `name`; `make_continent` leaves the sea's territories alone; `delete_sea` unthemes every feed of the sea (buried ones too) and deletes its continent; an empty derived key becomes `sea` before numbering (`sea`, `sea-2`).

**Snapshot** (still `version` 1): `themes[].id`, `territories[].id` (grouped by id, no longer by name), `continents[].id`, and `ledger: {seas: [{id, key, name, sea, position, feeds}], lands: [{id, name, continent, feeds}]}`: every theme in position order with its feed count (any state), every territory and continent in id order (`continent` = the theme key or null; `feeds` = member ids in position order).

**CLI:** `ressac chart apply <EDIT>` (one `ChartEdit` as JSON: the apps' path); `ressac chart land new <NAME> <FEED> <FEED>…`, `land rename <LAND> <NAME>`, `land delete <LAND>`, `land add <LAND> <FEED>`, `land remove <LAND> <FEED>`; `ressac chart continent make <THEME> <NAME>`, `continent unmake <THEME>`; `ressac chart theme new <NAME>`, `theme rename <THEME> <NAME>`, `theme move <THEME> <POSITION>` (1-based), `theme delete <THEME>`. `<LAND>` is an id or a name (any case); `<THEME>` a key or name (`resolve_theme`). Each prints the label, or with `--json` the `chart_edit` event; refusals exit 2 with the message.

**Page ↔ app:** the page posts `{type: "edit", seq: <whole number>, edit: "<ChartEdit JSON text>"}`; the app answers `Chart.edited(resultText)` with `{"seq":n,"ok":true,"done":<event>}` or `{"seq":n,"ok":false,"refused":<bool>,"error":"<text>"}`; the apps' shortcuts call `Chart.undo()` / `Chart.redo()`. `Chart.load`'s prefs gain `reduceMotion` (bool). After every edit (done or not) the app surveys at once; the page keeps the edit pending until the next `Chart.update` or `PENDING_MS = 5000`.

**Page history** (pure, `chart-core.js`): `{undo: Record[], redo: Record[]}`, `Record = {label, edit}` where `edit` is the patch to send.

## Review Focus

1. **A stale snapshot**: the page edits a land or sea that another writer renamed or deleted since the last survey. Expected: nothing written, `The chart changed; nothing was done.`, the chart redrawn from a fresh survey. Pinned in Task 2 (`stale_refs_are_refused_and_write_nothing`), Task 5 (`chart_apply_refuses_a_stale_edit_with_exit_2`) and Task 9 (`a_changed_refusal_shows_the_message_and_keeps_history`); both apps survey after every edit, refused or not (Tasks 13 and 15), and recette line 9 checks the redraw.
2. **Undo after another writer**: undoing a join after the feed was moved elsewhere, or after an unrelated write (a theme set from the garden, a feed added). Expected: the conflicting undo is refused and writes nothing; the unrelated write survives the undo. Pinned in Task 4 (`undo_keeps_an_unrelated_write`, `undo_after_a_conflicting_write_is_refused`).
3. **Deleting a sea that has a continent**: expected: its feeds unthemed (buried ones too), its continent removed, its territories and every other row untouched, the confirmation naming both; undo restores the sea (same id, key, position), the continent (same id, name) and every feed's theme. Pinned in Task 3 (`deleting_a_sea_removes_its_continent_and_undo_restores_all`) and Task 7 (`delete_sea_question_names_feeds_and_continent`).
4. **A name clash under a case difference**: `apple press` against `APPLE PRESS` (lands), `sea of kites` against `SEA OF KITES` (seas); SQLite's `UNIQUE` is case-sensitive, so only the core's check catches it. Expected: refused with the land or sea message and the field stays open; renaming a land to a case variant of its own name is allowed. Pinned in Task 2 (`names_clash_without_case`) and Task 7 (`check_name_clashes_without_case`).
5. **A hostile feed title in the log** (`<img src=x onerror=…>`, `</option>`, control characters): expected: shown as text in `Island:` options, the island heading and every label; no element is built from it. Pinned in Task 9 (`log_feed_titles_are_text`), with the fake document that throws on `innerHTML`.

Also pinned, as the spec implies: **Reduce Motion** (Task 8 `edit_motion_is_off_when_motion_is_reduced`, Task 12 `page_argument_carries_reduce_motion`, Task 14 `page_prefs_carry_reduce_motion`); **a loop drawn through overlapping labels** (Task 8 `a_press_that_moves_on_a_name_draws`, `loop_selects_by_centre_only`, `cartouche_falls_back_when_every_spot_is_taken`).

---

## Phase 1: the core

### Task 1: snapshot ids and the ledger

**Files:**
- Modify: `crates/ressac-core/src/store/chart_snapshot.rs` (types at 23-81, territories at 241-275, `json_shape` test)
- Modify: `crates/ressac-core/src/store/mod.rs:23` (re-export `ChartLedger`, `LedgerSea`, `LedgerLand`)
- Modify: `crates/ressac/src/output.rs` (the `chart_text_neutralises_controls` literal gains the new fields)
- Modify: `crates/ressac/tests/snapshots/cli__chart_json_snapshot.snap`

**Interfaces:**
- Produces: `ChartTheme.id: i64`, `ChartTerritory.id: i64`, `ChartContinent.id: i64`, `ChartSnapshot.ledger: ChartLedger { seas: Vec<LedgerSea>, lands: Vec<LedgerLand> }`, `LedgerSea { id, key, name, sea, position: i64, feeds: usize }`, `LedgerLand { id, name, continent: Option<String>, feeds: Vec<FeedId> }`. Field order as in Names decided here.

- [ ] **Step 1: Tests first** in `chart_snapshot.rs`:
  - `json_shape`: top-level keys gain `"ledger"`; `themes[0]` keys `["count","id","key","name","sea"]`; `territories[0]` keys `["feeds","id","name"]`; `ledger` keys `["lands","seas"]`; `ledger.seas[0]` keys `["feeds","id","key","name","position","sea"]`; `ledger.lands[0]` keys `["continent","feeds","id","name"]`.
  - `ledger_lists_empty_seas_and_hidden_lands`: themes `a` (2 feeds, one buried) and `empty` (none), a territory with one chart feed and one buried feed, a continent on `a`: `themes` has only `a`; `ledger.seas` = `[a (feeds 2), empty (feeds 0)]` in position order; `ledger.lands` has the territory with both member ids in position order and `continent: null`, then the continent with `continent: "a"` and `feeds: []`; `territories` is empty (fewer than two chart members).
  - `territories_are_grouped_by_id`: two territories inserted with interleaved member rows come out as two entries with their own ids (closes the TODO « territoires regroupés par nom et non par id »).
- [ ] **Step 2:** `cargo test -p ressac-core chart_snapshot` → the three fail.
- [ ] **Step 3: Implement.** Theme and continent ids from the same queries; territories `ORDER BY t.id, tf.position` grouped on `t.id`; the ledger from two extra queries (`theme` with `(SELECT count(*) FROM feed WHERE theme_id = theme.id)`; `territory` left-joined to `theme` for the key, members from `territory_feed`). Update `output.rs`'s test literal.
- [ ] **Step 4:** `cargo test --workspace`; `chart_json_snapshot` fails on the new fields: run `INSTA_UPDATE=always cargo test -p ressac --test cli chart_json_snapshot`, read the `.snap` diff (only ids and `ledger` added), keep it.
- [ ] **Step 5:** fmt, clippy, `cargo test --workspace` green. Commit `Core: ids and the ledger in the chart snapshot`.

### Task 2: the edit vocabulary, the patch engine, names and keys

**Files:**
- Create: `crates/ressac-core/src/store/chart_edit.rs`
- Modify: `crates/ressac-core/src/store/mod.rs` (`mod chart_edit;` and re-exports)
- Modify: `crates/ressac-core/src/error.rs` (five variants)

**Interfaces:**
- Consumes: `chart::check_theme_key` (make it `pub(super)`), `Store::conn`.
- Produces: the types of Names decided here; `pub(super) fn clean_name(raw: &str) -> Result<String>` (trim, 1 to 40 `char`s, else `NameLength`); `pub(super) fn same_name(a: &str, b: &str) -> bool` (`trim().to_lowercase()` equality); `pub(super) fn derive_key(name: &str, taken: impl Fn(&str) -> bool) -> String`; `pub(super) fn apply_patch(conn: &Connection, changes: &[Change]) -> Result<()>`; `pub(super) fn invert(changes: &[Change]) -> Vec<Change>`; `Store::apply_chart_edit` handling `ChartEdit::Patch` (other ops: Tasks 3 and 4; until then their arms return `Err(Error::Invalid(format!("{op} is not implemented yet")))`).

- [ ] **Step 1: Tests first** (in `chart_edit.rs`):
  - `edits_round_trip_through_json`: one value per op serializes to the field names of the table (`{"op":"set_sea","feed":3,"from":null,"to":{"id":1,"key":"a","sea":"SEA A"}}`) and back; an unknown `op` or a missing field fails to parse.
  - `clean_name_trims_and_counts_characters`: `"  Apple  "` → `"Apple"`; `""`, `"   "` and 41 `é` → `NameLength`; 40 `é` passes; `NameLength.to_string()` = `A name has 1 to 40 characters.`
  - `derive_key_follows_the_spec`: `"Sea of Kites!!"` → `sea-of-kites`; `"Comics"` with `comics` taken → `comics-2` (and `comics-3` when both taken); `"None"` → `none-2`; `"!!!"` → `sea`; `"--Été & Mer--"` → `été-mer`; every result passes `check_theme_key`.
  - `names_clash_without_case` (Review Focus 4): a patch creating land `apple press` beside `APPLE PRESS` → `LandNameTaken`; a sea patch `sea of kites` beside a theme whose `sea_name` is `SEA OF KITES` → `SeaNameTaken`, and beside a theme whose `name` is `Sea of kites` → `SeaNameTaken`; renaming `APPLE PRESS` to `Apple Press` passes; the error texts equal Global Constraints.
  - `stale_refs_are_refused_and_write_nothing` (Review Focus 1): a patch whose `Land.before` name, `Member.before` position, `FeedSeas.before` or `Sea.before` row differs from the database → `ChartChanged`, and a full `export_chart()` plus `count(territory_feed)` are unchanged.
  - `a_patch_and_its_inverse_restore_the_rows`: create a land with two members and a sea, apply, apply `invert`, and `export_chart()` equals the start; restored rows keep their ids.
  - `deleting_a_land_with_unlisted_members_is_refused`: a `Land` deletion whose member rows are not all removed by the same patch → `ChartChanged` (no silent `ON DELETE CASCADE`); same for a `Sea` deletion with a feed or continent still on it.
  - `a_feed_ends_in_one_territory`: a patch that would leave a touched feed in two territories → `ChartChanged`.
- [ ] **Step 2:** `cargo test -p ressac-core chart_edit` → fails to compile.
- [ ] **Step 3: Implement.** `apply_patch` checks every `before` first, then applies in this order: sea upserts, land upserts, `FeedSeas`, member removals, member inserts and moves, land deletions (refuse when members remain), sea deletions (refuse when a feed or a continent still points at it); then checks invariants on what it touched: `clean_name` and case-free uniqueness for changed land and sea names, `check_theme_key` and key uniqueness for sea rows, at most one continent per theme, each touched feed in at most one `kind='territory'` row. Rows are inserted with their explicit ids (an id in use → `ChartChanged`). `apply_chart_edit` runs compile + `apply_patch` in one `unchecked_transaction` and commits only on success. The four `Error` texts and `PartOfContinent`'s go in `error.rs`.
- [ ] **Step 4:** `cargo test -p ressac-core chart_edit` → pass; fmt, clippy.
- [ ] **Step 5:** Commit `Core: chart edits as checked, invertible patches`.

### Task 3: sea edits

**Files:**
- Create: `crates/ressac-core/src/store/chart_ops.rs` (compiling ops into `(label, Vec<Change>)`; `mod chart_ops;` in `store/mod.rs`)
- Modify: `crates/ressac-core/src/store/chart_edit.rs` (dispatch to `chart_ops`)

**Interfaces:**
- Consumes: Task 2's engine and helpers.
- Produces: `pub(super) fn compile(conn: &Connection, edit: &ChartEdit) -> Result<(String, Vec<Change>)>`, complete for `new_sea`, `rename_sea`, `order_seas`, `delete_sea`, `set_sea`, `patch` (land ops still stubbed until Task 4).

- [ ] **Step 1: Tests first** (in `chart_ops.rs`, on the `charted_store` pattern of `chart.rs` tests):
  - `new_sea_takes_a_derived_key_at_the_end`: `new_sea "Comics"` with `comics` taken → key `comics-2`, `name` = `sea_name` = `Comics`, last position; label `New Sea Comics`; `undo` deletes it.
  - `rename_sea_changes_the_sea_name_only`: `sea_name` changes, `name` and `key` do not; label `Rename <NAME>`.
  - `order_seas_needs_every_id`: a permutation reorders `themes()`; a list missing or adding an id → `ChartChanged`; undo restores the order.
  - `deleting_a_sea_removes_its_continent_and_undo_restores_all` (Review Focus 3): sea `alpha` with 3 feeds (one buried), a continent and a territory whose members are alpha feeds; `delete_sea` → those feeds unthemed, continent gone, the territory and the other seas and feed themes byte-equal to before (compare `export_chart()` minus `alpha`); label `Delete <SEA>`; applying `undo` gives the original `export_chart()` and the continent's id.
  - `a_large_sea_undo_fits_the_message_cap`: a sea of 2000 feeds; `serde_json::to_string(&done.undo).len() < MAX_EDIT`.
  - `set_sea_checks_from_and_keeps_lands`: `from` mismatch → `ChartChanged`; `to: null` unthemes; the feed's territory membership is unchanged; label `Move to SEA OF X` / `Move to UNCHARTED WATERS`.
- [ ] **Step 2:** run → fail. **Step 3:** implement each op per the table and its decisions. **Step 4:** `cargo test -p ressac-core` → pass; fmt, clippy.
- [ ] **Step 5:** Commit `Core: sea edits (new, rename, order, delete, set a feed's sea)`.

### Task 4: land edits

**Files:**
- Modify: `crates/ressac-core/src/store/chart_ops.rs`

**Interfaces:**
- Produces: `compile` complete for `join`, `leave`, `found`, `rename_land`, `delete_land`, `make_continent` (no stub left).

- [ ] **Step 1: Tests first:**
  - `join_moves_a_feed_out_of_its_old_land`: feed 3 in `PAIR` (with feed 4) joins `TRIO` → appended last in `TRIO`, `PAIR` deleted (one member left, its row removed too); label `Join TRIO`; undo restores `PAIR` with its id and both positions.
  - `leave_deletes_a_land_left_with_one_feed`; `leave` of a feed not in the land → `ChartChanged`.
  - `found_keeps_the_given_order_and_empties_old_lands` (rule 6): `found "NEW" [5, 3, 4]` → positions 0, 1, 2 in that order; old lands left with fewer than two rows deleted; one feed → `A new land needs at least two feeds.`
  - `continental_feeds_cannot_join_or_leave` (rule 2): a feed whose theme has continent `ALL ALPHA` → `join`, `leave` and `found` refused with `Part of the continent ALL ALPHA: move it to another sea, or turn the continent back into open water.`
  - `make_continent_once_per_sea`: creates a `continent` row on the theme, label `Make Continent <NAME>`; a second one on that sea → `ChartChanged`; the sea's territories are untouched.
  - `delete_land_handles_both_kinds`: a territory's member rows go with it; a continent goes back to open water; undo restores either.
  - `undo_keeps_an_unrelated_write` (Review Focus 2): join, then `set_feed_theme` on another feed and a new feed added with `add_feed`; applying the undo restores the membership and keeps both writes.
  - `undo_after_a_conflicting_write_is_refused` (Review Focus 2): join feed 3 to `TRIO`, then `import_chart` (or a direct `DELETE FROM territory WHERE name='TRIO'`) elsewhere; the undo → `ChartChanged`, and `export_chart()` is unchanged by the attempt.
  - `rename_land_to_its_own_name_in_another_case_is_allowed`.
- [ ] **Step 2:** run → fail. **Step 3:** implement. **Step 4:** `cargo test -p ressac-core` → pass.
- [ ] **Step 5: Phase gate:** fmt, clippy, `cargo test --workspace`. Commit `Core: land edits (join, leave, new land, rename, delete, continents)`.

## Phase 2: the CLI

### Task 5: `ressac chart apply`

**Files:**
- Modify: `crates/ressac/src/cli.rs` (`ChartCommand::Apply { edit: String }`)
- Modify: `crates/ressac/src/commands/chart.rs`
- Modify: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: `Store::apply_chart_edit`, `ChartEditDone::event`, `MAX_EDIT`.
- Produces: `ressac [--json] chart apply <EDIT>`: prints the label (human) or the event as one JSON line; exit 2 (usage, message as is) for `ChartChanged`, the name errors, `PartOfContinent`, `Invalid`, a JSON that is not a `ChartEdit` (`The edit is not a chart edit: <serde message>`) and an edit over `MAX_EDIT` bytes (`The edit is too large.`); needs an existing database (exit 1 otherwise, nothing created).

- [ ] **Step 1: Tests first** (`cli.rs`, on `chart_home()`):
  - `chart_apply_prints_the_event_and_its_undo_reverts_it`: `--json chart apply '{"op":"new_sea","name":"Comics"}'` → one line, `event` = `chart_edit`, `label` = `New Sea Comics`; applying `undo` (re-serialized) with a second `chart apply` removes the theme (`chart export` shows it gone).
  - `chart_apply_refuses_a_stale_edit_with_exit_2`: a `rename_land` with a wrong `name` → exit 2, stderr last line `The chart changed; nothing was done.`, export unchanged.
  - `chart_apply_rejects_malformed_and_oversized_edits`: `{"op":"sail"}` and a 70 000-byte string → exit 2, nothing written.
  - `chart_apply_needs_a_database`: missing database → exit 1, no file created.
- [ ] **Step 2:** `cargo test -p ressac --test cli chart_apply` → fail. **Step 3:** implement in `commands/chart.rs` with a refusal mapper like `feed.rs`'s `refusal_error` (texts through `output::printable`). **Step 4:** pass.
- [ ] **Step 5:** Commit `CLI: ressac chart apply`.

### Task 6: `ressac chart land|continent|theme …`

**Files:**
- Modify: `crates/ressac/src/cli.rs` (`ChartCommand::Land { action: LandCommand }`, `Continent { action: ContinentCommand }`, `Theme { action: ThemeCommand }`)
- Create: `crates/ressac/src/commands/chart_edit.rs` (argument resolution and the subcommands; `mod chart_edit;` in `commands/mod.rs`)
- Modify: `crates/ressac/src/commands/chart.rs` (dispatch)
- Modify: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: Task 5's printing and refusal mapper (`pub(super)`).
- Produces: the verbs of Names decided here. `<LAND>`: all digits → that id, else a name compared without case; `land add`/`land remove` match territories only. Unknown → exit 2 `No land "<given>".`; `continent unmake` on a sea without one → exit 2 `Theme "<key>" has no continent.`; `theme move` outside `1..=n` → exit 2 `Position <p> is not between 1 and <n>.`

- [ ] **Step 1: Tests first:** `chart_land_commands_round_trip` (new with two feed ids, add, remove, rename, delete; each line is the label; `chart export` matches after each); `chart_land_name_clash_ignores_case` (exit 2 `A land with that name already exists.`); `chart_theme_commands_round_trip` (new `Comics` twice → keys `comics`, `comics-2`; rename; move 2 → 1; delete; `--json` prints events); `chart_continent_make_and_unmake`; `chart_land_unknown_name_exits_2`.
- [ ] **Step 2:** run → fail. **Step 3:** implement; each verb builds the `ChartEdit` from the database's current rows (so a CLI edit is never stale) and calls `apply_chart_edit`. Help strings name what each does in the README's words. **Step 4:** pass.
- [ ] **Step 5: Phase gate:** fmt, clippy, `cargo test --workspace`. Commit `CLI: chart land, continent and theme subcommands`.

## Phase 3: the page

### Task 7: fixture, edit texts, names and history in `chart-core.js`

**Files:**
- Modify: `apps/macos/ChartTests/make-fixture.cjs`, then regenerate `apps/macos/ChartTests/fixture.json`
- Modify: `apps/macos/Ressac/Resources/Chart/chart-core.js`
- Create: `apps/macos/ChartTests/chart-edit.test.cjs`

**Interfaces:**
- Consumes: the snapshot shape of Task 1, the wire table of Names decided here.
- Produces on `ChartCore`: `EDIT_TEXT` (`changed`, `landTaken`, `seaTaken`, `nameLength`, `newLandTwo`, `continentHint(name)`), `MAX_EDIT = 65536`, `PENDING_MS = 5000`; `checkName(raw, kind /* "land"|"sea" */, ledger, selfId) → {name} | {error}`; `deleteSeaQuestion(sea, count, hasContinent) → string`; `ledgerIndex(data) → {landOf(feedId), continentOf(themeKey), seaById(id), landById(id), seaRef(key|null), landRef(id)}`; `edits` with one builder per op returning the wire object (`edits.join(feed, landRef)` …); `history: {empty(), record(h, done), step(h, which) → {record, rest} | null, land(h, which, record, done), titles(h) → {undo, redo}}` (pure, return new objects); `editFailure(result) → "name" | "continent" | "changed" | "failed"`.
- Fixture: every theme, territory and continent gets an id and the document a `ledger` (one empty sea `UNSAILED SEA`, one land hidden off the chart); the tricky title gains `<img src=x onerror=alert(1)>` and `</option>`.

- [ ] **Step 1: Tests first** (`chart-edit.test.cjs`):
  - `edit_texts_match_the_core`: reads `crates/ressac-core/src/error.rs` and asserts each `EDIT_TEXT` string (and the continent hint with `{name}`) appears there verbatim.
  - `check_name_clashes_without_case` (Review Focus 4): `" apple press "` against land `APPLE PRESS` → `{error: EDIT_TEXT.landTaken}`; same with `selfId` of that land → `{name: "apple press"}`; `"sea of kites"` → `seaTaken` (fixture sea `SEA OF KITES`); `""` and 41 code points → `nameLength`; 40 astral code points pass.
  - `delete_sea_question_names_feeds_and_continent` (Review Focus 3): `("SILICON SHORES", 12, true)` → the Global Constraints sentence exactly; `(…, 12, false)` → `Delete the sea SILICON SHORES? Its 12 feeds will be uncharted.`; count 1 → `Its 1 feed will be uncharted`; count 0 → `Delete the sea SILICON SHORES?`.
  - `history_records_undoes_and_redoes`: record two dones → `titles` = `{undo: "Undo Join TRIO", redo: null}`; `step(h,"undo")` returns the last record; `land(h,"undo",record,done)` puts `{label, edit: done.undo}` on `redo`; a new `record` clears `redo`.
  - `edit_failure_classifies_results`: the three name texts → `name`; a `Part of the continent …` text → `continent`; refused otherwise → `changed`; `refused: false` → `failed`.
  - `builders_write_the_wire_format`: each builder's output deep-equals the table's example, and `ledgerIndex` finds the fixture's hidden land for its member.
- [ ] **Step 2:** `node --test 'apps/macos/ChartTests/*.test.cjs'` → the new file fails; the old ones still pass on the new fixture.
- [ ] **Step 3:** implement (code points via `[...s].length`; case via `toLowerCase()`). **Step 4:** all Node tests pass.
- [ ] **Step 5:** Commit `Chart page: edit texts, names and history (pure)`.

### Task 8: gesture geometry in `chart-core.js`

**Files:**
- Modify: `apps/macos/Ressac/Resources/Chart/chart-core.js`
- Create: `apps/macos/ChartTests/chart-gestures.test.cjs`

**Interfaces:**
- Consumes: `place`, `territories`, `field`, `labels`, `seas`, Task 7's `ledgerIndex`.
- Produces: `classifyPress(points /* screen px */) → "click" | "stroke"` (stroke once any point is 4 px or more from the first); `closedLoop(points /* chart units */) → boolean` (path length ≥ 120 and end within 40 of start); `loopSelection(nodes, loop, data) → {islands, continentSea}` (centres inside `d3.polygonContains`, continental islands left out of `islands`; `continentSea` = the sea key when the loop holds every island of exactly one sea, at least two, and that sea has no continent, else null); `nameAt(placements, x, y) → placement | null` (kinds `territory`, `continent`, `sea` only); `dropTarget(scene, data, dragged, x, y)`, where `scene = {view, nodes, territories, ground}` (what `buildScene` returns from Task 11 on; tests build it from `place`, `territories` and `field`), `→ {kind: "join", land} | {kind: "pair", with} | {kind: "sea", key} | {kind: "leave", land} | {kind: "refuse", hint} | {kind: "none"}`; `cartouchePlace(box, placements, size) → {x, y}`; `editMotion(prefs, reducedMatches) → boolean`.
- `dropTarget` precedence: an island within `r + 12` (a territory's → `join`; a lone non-continental one → `pair`; else `none`); a territory link within 24 → `join`; in `themes` view only, the nearest sea cell other than the island's own → `sea` (`key` null for the uncharted waters); open water (`ground.at < .5`) for an island in a territory → `leave`; else `none`. Then rule 2: a continental dragged island turns `join`, `pair` and `leave` into `refuse` with `EDIT_TEXT.continentHint(name)`. `cartouchePlace` tries right, left, below, above the loop's box (inside the 40-unit margin, meeting no placement box), then falls back to `{x: 40 + size.w / 2, y: 40 + size.h / 2}`.

- [ ] **Step 1: Tests first:**
  - `a_press_that_moves_on_a_name_draws`: 3 px of jitter → `click`; a 5 px move → `stroke` (whatever is under the press).
  - `loop_selects_by_centre_only`: a loop that covers an island's label and edge but not its centre does not select it; a loop round two lone islands selects both, in the order of `data.feeds`.
  - `closed_loop_needs_length_and_closure`; `loop_offers_a_continent_for_one_whole_sea` (all of a sea without a continent → `continentSea`; one island missing, one from another sea, or a sea that has a continent → null).
  - `drop_targets_follow_the_precedence`: on a territory's island → `join`; on a lone island → `pair`; in another sea's cell (themes view) → `sea`; the same spot in harbour view → not `sea`; open water for a territorial island → `leave`; on itself → `none`.
  - `continental_islands_only_change_sea` (rule 2): a continental island on a territory → `refuse` with the exact hint; into another sea → `sea`.
  - `cartouche_avoids_labels_and_falls_back_when_every_spot_is_taken`: a free spot never meets a placement box; with placements covering all four spots the fallback is returned (and the function returns in one pass).
  - `edit_motion_is_off_when_motion_is_reduced`: `({reduceMotion: true}, false)` and `({reduceMotion: false}, true)` → false; both false → true.
- [ ] **Step 2:** run → fail. **Step 3:** implement. **Step 4:** all Node tests pass.
- [ ] **Step 5:** Commit `Chart page: hit-testing, loops, drop targets and the cartouche spot (pure)`.

### Task 9: `chart-survey.js`, the edit session and the Cartographer's Log

**Files:**
- Create: `apps/macos/Ressac/Resources/Chart/chart-survey.js` (UMD like `chart-draw.js`; builders take the document, as `popoverContent` does)
- Create: `apps/macos/ChartTests/chart-survey.test.cjs` (with a copy of `chart-draw.test.cjs`'s `fakeDocument`, which throws on `innerHTML`, plus `dispatch` for `keydown`)
- Modify: `apps/macos/Ressac/Resources/Chart/index.html` (script after `chart-core.js`)
- Modify: `crates/ressac-ui/src/chart.rs` (`FILES` gains `chart-survey.js`, 14 entries; `the_chart_files_are_the_mac_files` checks it is referenced)
- Modify: `apps/linux/src/chart.rs` (`file_bytes` list)

**Interfaces:**
- Consumes: Task 7's `history`, `editFailure`, `MAX_EDIT`, `PENDING_MS`, `checkName`, `deleteSeaQuestion`, `ledgerIndex`.
- Produces: `ChartSurvey.createSession({post, onChange}) → {send(edit) → boolean, edited(text) → {kind, message} | null, undo() → boolean, redo() → boolean, titles(), pending() → boolean, updated()}`: one edit in flight (`send` returns false while pending or when `JSON.stringify(edit).length > MAX_EDIT`); `seq` counts up; a result with another `seq` is ignored (`null`); done → `history.record` (or `land` for undo/redo); a refused undo or redo drops its record, a failed one keeps it; pending ends at `updated()` after an ok result, or `PENDING_MS` after it; `onChange()` after every state change.
- Produces: `ChartSurvey.logContent(doc, data, state, on) → Node[]` with `state = {selected, busy, message}` and handlers `on.{select, setSea, setLand, leave, newLand, renameLand, deleteLand, makeContinent, renameSea, moveSea, newSea, deleteSea, undo, redo}`; `ChartSurvey.nameForm(doc, {title, value, withChoices}, on) → Node[]` (the cartouche and `New Land…`/`New Sea…`/rename forms: `<form>` with `<input maxlength=40>`, Enter submits, Esc calls `on.cancel`, an error line under it). Classes: `.log`, `.log-island`, `.log-lands`, `.log-seas`, `.log-undo`, `.log-redo`, `.log-status`, `.name-form`.
- Log layout: `<aside aria-labelledby>` titled `Cartographer's Log`; island block (`Island:` select of every chart feed by title, the title as a heading, `Sea:` select of `ledger.seas` plus `Uncharted waters`, `Land:` select of territories, `Leave Land`, `New Land…`); `Lands` (each: name, feed count, `Rename`, `Delete` or, for a continent, `Back to Open Water`); `Seas` (each: `≡` handle, sea name, count, `Rename`, `Make a continent` when it has none, `Delete`; Alt+↑/↓ on a row calls `on.moveSea(id, ∓1)`); `New Sea…`; `Undo …`/`Redo …` buttons showing `titles()`, disabled when null; a `role="status"` line. `Leave Land` is disabled with `title` = the continent hint on a continental island; every control is disabled while `busy`.

- [ ] **Step 1: Tests first:**
  - `log_feed_titles_are_text` (Review Focus 5): the island select and heading for the fixture's tricky feed carry its exact title as text nodes; no element under the log has the tag `IMG` or `SCRIPT`; the fake document's `innerHTML` trap never fires.
  - `log_lists_every_sea_and_land_from_the_ledger`: `Seas` has the empty `UNSAILED SEA`, in ledger order, with counts; `Lands` has the hidden land; continents show `Back to Open Water`.
  - `leave_land_is_disabled_on_a_continental_island` with the hint as `title`.
  - `alt_arrows_move_a_sea`: dispatching `keydown {key: "ArrowUp", altKey: true}` on the second sea row calls `on.moveSea(id, -1)`.
  - `session_sends_one_edit_at_a_time`: `send` posts `{type: "edit", seq: 1, edit: "<json>"}`, a second `send` returns false until `edited`; an oversized edit is not posted.
  - `a_changed_refusal_shows_the_message_and_keeps_history` (Review Focus 1): after one done edit, a refused result with `EDIT_TEXT.changed` → `{kind: "changed", message: EDIT_TEXT.changed}`, history still has its record.
  - `session_undo_redo_round_trip`: undo posts the record's patch; its done result moves it to `redo` with the new patch; a refused undo drops the record; a result with a stale `seq` is ignored.
- [ ] **Step 2:** `node --test …` → fail. **Step 3:** implement; update `index.html`, `FILES` and `file_bytes`. **Step 4:** Node tests pass; `cargo test -p ressac-ui chart` passes; `apps/linux/dev/dev.sh check` → `check-linux: OK …` (the KDE app serves the new file).
- [ ] **Step 5:** Commit `Chart page: the edit session and the Cartographer's Log`.

### Task 10: Survey mode in the page

**Files:**
- Modify: `apps/macos/Ressac/Resources/Chart/chart-draw.js` (`createPage`, the `Chart` object, `normalisePrefs`)
- Modify: `apps/macos/Ressac/Resources/Chart/chart.css`
- Modify: `apps/macos/ChartTests/chart-draw.test.cjs`

**Interfaces:**
- Consumes: Task 9's session and builders, Task 8's `nameAt`, `editMotion`.
- Produces: `Chart = {load, update, edited, undo, redo}`; `normalisePrefs` keeps a boolean `reduceMotion` (default false); `prefsMessage` unchanged.
- Behaviour: `✒ Survey` (after `Key`, `aria-pressed`) shows only when a bridge exists and the document has a `ledger`; on, it adds `body.survey` (faint survey grid drawn on the overlay canvas, the quill cursor from `chart.css`), opens the log and shows the `Log` button that toggles it; `fit()` keeps the chart clear of the open drawer (320 px). In Survey mode a click on an island selects it in the log (no popover); a click on a territory, continent or sea name opens the inline rename form over that label (Cormorant SC, `font-variant: small-caps`; Enter confirms, Esc cancels; an unchanged name cancels; a `name` failure keeps it open with its message). `Chart.undo()`/`Chart.redo()` act only in Survey mode with no name form open. `Chart.update` calls `session.updated()`. Survey mode always starts off.

- [ ] **Step 1: Tests first:** `chart_exposes_edited_undo_and_redo` (the `Chart` object keys); `prefs_keep_reduce_motion` (`normalisePrefs({reduceMotion: true}).reduceMotion === true`, a string → false); `prefs_message_never_carries_reduce_motion`.
- [ ] **Step 2:** run → fail. **Step 3:** implement. **Step 4:** Node tests pass; then the preview check of Task 11 Step 4 covers the DOM by eye.
- [ ] **Step 5:** Commit `Chart page: Survey mode, the log drawer and renaming in place`.

### Task 11: gestures on the chart

**Files:**
- Modify: `apps/macos/Ressac/Resources/Chart/chart-draw.js` (`buildScene` keeps `territories`; pointer handlers)
- Modify: `apps/macos/Ressac/Resources/Chart/chart.css`
- Modify: `apps/macos/ChartTests/chart-draw.test.cjs`
- Modify: `apps/macos/ChartTests/preview.html` (`&survey=1`: a stub bridge that answers each edit after 300 ms with a done result whose `undo` is `{op: "patch", label, changes: []}`, and `&refuse=1` with `EDIT_TEXT.changed`)

**Interfaces:**
- Consumes: Task 8's `classifyPress`, `closedLoop`, `loopSelection`, `dropTarget`, `cartouchePlace`, `editMotion`; Task 9's session and `nameForm`.
- Behaviour: in Survey mode, a press on an island drags it (the island follows in ink on the overlay); the drop runs `dropTarget`: `join` → `edits.join`; `pair` → the cartouche `Name this land` then `found` with both ids in `data.feeds` order (the chart order of rule 6); `sea` → `set_sea`; `leave` → `leave`; `refuse` → spring back and the hint in the log's status line; `none` → spring back. A press elsewhere that becomes a stroke draws ink; on release `closedLoop` + `loopSelection`: fewer than two islands → the line fades; `continentSea` → the cartouche titled `Make a continent` (Enter makes it); else `Name this land` then `found` with `islands` (already in `data.feeds` order). The ink line stays until the new chart arrives (or the cartouche is cancelled), then the invented coastline replaces it. The cartouche sits at `cartouchePlace`. While pending, the moved island stays at its drop point and gestures are ignored. With `editMotion` false: no spring-back tween, no ink fade, the line simply disappears.

- [ ] **Step 1: Tests first:** `scene_keeps_the_territories` (`buildScene(...).territories` equals `core.territories` for the fixture); `reduced_motion_schedules_no_frames_for_a_spring_back` (with a stub `win.requestAnimationFrame` counter and `reduceMotion: true`, the exported `springBack(scene, node, win, motion)` helper returns without scheduling).
- [ ] **Step 2:** run → fail. **Step 3:** implement. **Step 4:** Node tests pass. Preview check in a browser (Playwright, Firefox): serve the repository, open `apps/macos/ChartTests/preview.html?view=themes&survey=1`, turn on Survey, drag a lone island onto another (cartouche appears clear of labels), name it, check `Undo New Land …` in the log; reload with `&refuse=1`, rename a sea, check the status line shows `The chart changed; nothing was done.`; screenshot both into the scratchpad. Emulate `prefers-reduced-motion: reduce` once and check a refused drop snaps back.
- [ ] **Step 5: Phase gate:** Node tests, `cargo test --workspace`, `apps/linux/dev/dev.sh check`. Commit `Chart page: drag, freehand coastlines and the cartouche`.

## Phase 4: the Mac bridge

### Task 12: edit messages, replies and Reduce Motion in `ChartWebView.swift`

**Files:**
- Modify: `apps/macos/Ressac/Chart/ChartWebView.swift`
- Modify: `apps/macos/RessacTests/ChartBridgeTests.swift`

**Interfaces:**
- Produces: `ChartAction.edit(seq: Int64, json: String)`; `ChartBridge.editOps: Set<String>` (the 12 ops) and `ChartBridge.maxEdit = 65_536`; `enum ChartEditReply: Equatable, Sendable { case done(String /* event line */), refused(String), failed(String) }`; `ChartPage.init(prefs:onAction:onEdit:)` with `onEdit: @MainActor (String) async -> ChartEditReply` (default answers `.failed("Editing is not available.")`); `ChartPage.edited(seq:reply:)` runs `Chart.edited(result)` with the result as a call argument; `ChartPage.undo()`/`redo()` run `Chart.undo()`/`Chart.redo()`; `ChartPageLink` (`@MainActor final class`, `weak var page: ChartPage?`) handed to `ChartWebView(json:prefs:link:onAction:onEdit:)`, which sets it in `makeCoordinator`; `ChartPrefs.pageArgument(reduceMotion:)` adds `"reduceMotion": reduceMotion`.
- `.edit` messages go to `onEdit`, never to `onAction`; only from the main frame, as today.

- [ ] **Step 1: Tests first:**
  - `bridge_accepts_an_edit_message`: `["type": "edit", "seq": NSNumber(value: 3.0), "edit": #"{"op":"new_sea","name":"Comics"}"#]` → `.edit(seq: 3, json: …)`.
  - `bridge_refuses_malformed_edits`: unknown op, `seq` boolean or 1.5, `edit` not a string, not a JSON object, or over 65 536 UTF-8 bytes → nil.
  - `page_argument_carries_reduce_motion`: `pageArgument(reduceMotion: true)["reduceMotion"] as? Bool == true`, and false when off.
  - WebKit (same `.disabled(if: RESSAC_HEADLESS)` guard): `an_edit_goes_to_the_app_and_its_reply_comes_back`: the harness's `onEdit` records the JSON and answers `.done(<event line for New Sea Comics>)`; the test clicks `✒ Survey`, then `New Sea…`, sets the field to `Comics` and submits; expects one recorded JSON with `"op":"new_sea"` and the page's `.log-undo` text `Undo New Sea Comics`.
- [ ] **Step 2:** xcodebuild test → fails to compile. **Step 3:** implement. **Step 4:** xcodebuild test → `** TEST SUCCEEDED **`.
- [ ] **Step 5:** Commit `Mac: chart edit messages and replies in the page bridge`.

### Task 13: the helper path, the window and ⌘Z

**Files:**
- Modify: `apps/macos/Ressac/Engine/RessacCLI.swift` (`RunRequest.chartEdit(json:)` = `["chart", "apply", json]`, short; `["chart", "apply"]` in `refusable`; no banner for it)
- Modify: `apps/macos/Ressac/Engine/Events.swift` (`case chartEdit(json: String)`, the raw line, when `event` is `chart_edit`)
- Modify: `apps/macos/Ressac/App/AppState.swift` (`func chartEdit(json: String) async -> ChartEditReply`)
- Modify: `apps/macos/Ressac/Chart/ChartWindow.swift` (`ChartScreen`: `onEdit` runs `state.chartEdit`, then `model.refresh()`; hidden `Undo Chart Edit` (⌘Z) and `Redo Chart Edit` (⇧⌘Z) buttons calling the link's page, like `GardenPageViews.undoShortcut`)
- Modify: `apps/macos/RessacTests/AppStateTests.swift`

**Interfaces:**
- Consumes: Task 12's types; Task 5's CLI.
- `chartEdit`: `.done(line)` when the run succeeds with a `chart_edit` event; `.refused(failure.message)` when `isRefusal`; `.failed(message)` otherwise (an exit 0 without the event is `.failed("ressac printed no edit.")`).

- [ ] **Step 1: Tests first** (stub `ressac` scripts, `makeState`): `chart_edit_runs_chart_apply` (the stub's `calls` line ends `chart apply {"op":"new_sea","name":"Comics"}`); `chart_edit_returns_the_event_line`; `chart_edit_refusal_is_returned_not_bannered` (stub exits 2 with `The chart changed; nothing was done.` → `.refused` with that text, `state.runs.banner == nil`); `chart_edit_failure_is_failed`; `engine_event_decodes_chart_edit` (`EngineEvent.decode(line:)` of a `chart_edit` line → `.chartEdit(json:)` holding the trimmed line).
- [ ] **Step 2:** xcodebuild test → fails. **Step 3:** implement. **Step 4:** `** TEST SUCCEEDED **`.
- [ ] **Step 5: Phase gate:** also `cargo test --workspace`. Commit `Mac: chart edits through ressac chart apply, with ⌘Z and ⇧⌘Z`.

## Phase 5: the KDE bridge

### Task 14: `ressac-ui`: the edit action, the result script, prefs and the run request

**Files:**
- Modify: `crates/ressac-ui/src/chart.rs`
- Modify: `crates/ressac-ui/src/runs.rs`

**Interfaces:**
- Produces: `ChartAction::Edit { seq: i64, edit: String }` from `handle` (whole-number `seq`; `edit` a string of at most `MAX_EDIT` bytes that parses as `ressac_core::store::ChartEdit`); `pub fn edit_result(seq: i64, end: &RunEnd<ChartEditDone>) -> String` (the result JSON of Names decided here; `Refused` reads as failed with `ressac is busy.`); `pub fn edited_script(result: &str) -> String` = `Chart.edited(<js_string>)`; `page_prefs` writes `"reduceMotion":<bool>`; `RunRequest::ChartEdit(String)`: short, `shows_failure_banner` false, `is_refusal` true for `ChartChanged`, `LandNameTaken`, `SeaNameTaken`, `NameLength`, `PartOfContinent`, `Invalid`.

- [ ] **Step 1: Tests first:** `bridge_accepts_an_edit` and `bridge_refuses_malformed_edits` (as the Swift ones); `edit_results_are_the_page_format` (done → `ok:true` and `done.event == "chart_edit"`; a refused failure → `ok:false, refused:true, error` exact); `edited_script_never_splices` (a hostile error text stays inside one `js_string` literal, as `scripts_never_splice_the_document`); `page_prefs_carry_reduce_motion`; `chart_edit_is_short_and_bannerless`.
- [ ] **Step 2:** `cargo test -p ressac-ui` → fail. **Step 3:** implement (adjust `reduce_motion_starts_still_and_keeps_the_stored_choice` for the new key). **Step 4:** pass.
- [ ] **Step 5:** Commit `KDE core: chart edit messages, results and Reduce Motion in the prefs`.

### Task 15: the `Chart` object, shortcuts and the `chart-edit` walk

**Files:**
- Modify: `apps/linux/src/chart.rs` (`receive` routes `Edit`; `MAX_MESSAGE = 256 * 1024`; `fn start_edit(seq, edit)` and `fn edit_ended(seq, end)`)
- Modify: `apps/linux/qml/SeaChartWindow.qml` (`Shortcut { sequences: [StandardKey.Undo]; onActivated: web.runJavaScript("Chart.undo()") }`, `Shortcut { sequences: ["Ctrl+Shift+Z"]; onActivated: web.runJavaScript("Chart.redo()") }`)
- Modify: `apps/linux/qml/Main.qml` (`RESSAC_RECETTE_WALK=chart-edit` steps)
- Modify: `apps/linux/tests/chart.rs`

**Interfaces:**
- Consumes: Task 14; `submit_short` (`apps/linux/src/runs.rs:510`) as `garden.rs::set_theme` uses it.
- `start_edit` submits `RunRequest::ChartEdit(edit)` with the job `cx.app.store().apply_chart_edit(&parsed)` and an `end` that queues `edit_ended` on the GUI thread (`qt.queue`, as `run_undo` does); `edit_ended` emits `run_script(edited_script(edit_result(seq, &end)))` and starts a survey through `model.refresh()`.

- [ ] **Step 1: Tests first:** in `apps/linux/src/chart.rs` tests, `an_edit_message_under_the_new_cap_parses`; in `apps/linux/tests/chart.rs`, `the_sea_chart_edits_and_undoes_with_ctrl_z`: the walk opens the chart on a copy of the contract fixture, clicks `✒ Survey`, `New Sea…`, submits `Sea of Tests`, waits, prints `walk: 20-made seas=<n> undo=<.log-undo text>`; activates the chart window, sends Ctrl+Z (`Recette.key(Qt.Key_Z, "z", Qt.ControlModifier)`), prints `walk: 24-undone seas=<n> redo=<.log-redo text>`; sends Ctrl+Shift+Z, prints `walk: 28-redone seas=<n>`. Asserts `seas=1 undo=Undo New Sea Sea of Tests`, `seas=0 redo=Redo New Sea Sea of Tests`, `seas=1`, and after exit the database (rusqlite) has one theme, key `sea-of-tests`.
- [ ] **Step 2:** `apps/linux/dev/dev.sh sh 'cargo test -p ressac-kde --test chart'` → fails. **Step 3:** implement. **Step 4:** passes.
- [ ] **Step 5: Phase gate:** `apps/linux/dev/dev.sh check` → `check-linux: OK (fmt, clippy, test, chart, layout smoke)`. Commit `KDE: chart edits on the short lane, with Ctrl+Z and Ctrl+Shift+Z`.

## Phase 6: documentation and recette

### Task 16: README and TODO

**Files:**
- Modify: `README.md` (Sea Chart section, lines 243-290)
- Modify: `TODO.md` (lines 98, 108, 109)

- [ ] **Step 1: README.** A **Survey** bullet after « Themes and territories »: `✒ Survey` in the toolbar; drag an island onto a land, onto a lone island, into open water or (By theme) into another sea; click a name to rename it; draw a closed loop round islands to make a land, or round a whole sea for `Make a continent`; the Cartographer's Log does all of it from the keyboard (seas reordered with `≡` or Alt+↑/↓); ⌘Z / ⇧⌘Z (Ctrl+Z / Ctrl+Shift+Z in the KDE app) undo and redo while the window is open; an edit made on a chart another writer changed meanwhile is refused with `The chart changed; nothing was done.`; continents follow their sea. The « Themes and territories » bullet's last sentence becomes « Territories and the list of themes are edited in Survey mode, with `ressac chart land|continent|theme …`, or imported. ». A paragraph after the export paragraph lists the subcommands of Names decided here and `ressac chart apply`. « The database is canonical » adds that the editor and the subcommands write the database directly, so `ressac chart export` captures them.
- [ ] **Step 2: TODO.md** (French, no em dash): line 98 ticked with « Fait le <date> sur la branche `territory-editor` (spec et plan du 27/11/2026) ; recette en attente. »; line 108 drops « territoires regroupés par nom et non par id » (the snapshot groups by id now); line 109 adds « l'éditeur garde un seul territoire par flux ; une importation peut encore en mettre deux ».
- [ ] **Step 3:** Commit `Docs: the territory editor in the README and TODO`.

### Task 17: recette on both apps (stops for Clément)

**Files:** `notes.md` (a « Recette de l'éditeur de territoires (<date>) » paragraph in French, no em dash), committed alone.

- [ ] **Step 1:** build a Release Mac app and the KDE app (marghe, as for earlier recettes); copy Clément's database to a scratch path and open both apps on the copy with `RESSAC_DB` (never the real database).
- [ ] **Step 2: ⏸ STOP, hand Clément this checklist**, each line run on the Mac and on the KDE app:
  1. Survey off: nothing drags, no name opens, ⌘Z / Ctrl+Z does nothing to the chart.
  2. `✒ Survey` on: grid and quill; the log opens; `Log` closes and reopens it.
  3. Drag an island onto a land; onto a lone island (cartouche `Name this land`, clear of labels); into open water; in By theme, into another sea.
  4. A continental island dropped on a land springs back with the continent hint; `Leave Land` is disabled with it.
  5. A loop round three islands of two seas makes a land; a loop round every island of a sea without a continent offers `Make a continent`.
  6. Rename a land, a continent and a sea in place; a name differing only in case from another is refused, the field stays open.
  7. In the log, keyboard only: select an island, change its sea and land, `New Land…`, `New Sea…`, reorder with Alt+↑/↓, delete a sea with a continent (the confirmation names both).
  8. Undo every step back to the start, redo two; `ressac chart export` before and after the full undo are identical.
  9. Stale chart: rename a land with `ressac chart land rename` in a terminal, then rename it in the page before the chart redraws: `The chart changed; nothing was done.`, then the redraw.
  10. Reduce Motion on (macOS) / Plasma animation speed instant: no ink fade, no spring-back tween.
- [ ] **Step 3:** record his answers and every finding in `notes.md`; findings become `TODO.md` lines; commit `Notes: recette of the territory editor`.
