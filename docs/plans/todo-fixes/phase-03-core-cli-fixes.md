# Phase 3: CLI refusals, `apply-safe` counts, spam hint, entry authors and a lean triage

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 2 (the CLI split), itself after Phase 1: Phase 3a edits `crates/ressac/src/commands/garden.rs`, which Phase 2 creates. Phase 1 also edits `crates/ressac-core/src/service.rs`, two lines away from Task 3.6's first edit (its lines 257 and 266-267 around our 259-260), so **match on the quoted texts, not on the line numbers**, which are those of `855cd2c` (or of Phase 2's files for `commands/garden.rs`); checked on 15/10/2026 with Phases 1 and 2 applied first, every quoted text matches. Phase 3c touches the app (`RessacCLI.swift`, `RunCoordinator.swift`, `RunCoordinatorTests.swift`), which no earlier phase edits; Phase 7 later cites line 277 of the app spec, which 3c's one-line edit of line 276 leaves in place. Re-read each file before each group of at most three edits (CLAUDE.md rules 6 and 9).

**Goal:** close four of the five items of `TODO.md` « Reporté ▸ Cœur et CLI » (the fifth, the Cemetery's unread counts, is Phase 4), and apply Clément's decision on the spam hint (« Reporté ▸ Appli et outillage »):
1. `garden decide <id> keep|repair` towards an address another active feed already uses exits 2 with the bare message, like the other refusals, instead of 1 with `error: `.
2. `garden apply-safe --json` says how many repairs it applied and which it skipped, and why.
3. The "Possible spam signs" hint is no longer added to Alive feeds (decision of 15/10/2026).
4. A change of an entry's author reaches `entry.author` and the search index.
5. `recompute_triage` reads one light row per feed instead of `load_rows` (every feed with its folder path, three entries and five attempts), and a refresh computes the flags once instead of twice.
6. The Mac app treats that new exit 2 of `garden decide` and `garden undo` as the store's refusal, not as a bug in its own call (Phase 3c, decision of 15/10/2026).

**Tech stack:** Rust 2024 (rust-version 1.99), rusqlite 0.40.2, clap 4.6.7, serde_json.

## Two sub-phases

Thirteen files in all, so three sub-phases of at most five files, each with its own gate and its own approval stop (3c, the app side of Task 3.1, was added on 15/10/2026 at Clément's request):

**Phase 3a: CLI and spam hint (5 files)**
- Modify: `crates/ressac/src/commands/garden.rs` (95 lines after Phase 2): `decision_error`, the `apply-safe` JSON line, one unit test.
- Modify: `crates/ressac/tests/cli.rs` (629 lines): `seed_diagnoses` and two CLI tests.
- Modify: `crates/ressac-core/src/health/mod.rs` (1058 lines): the hint's status filter, two tests.
- Modify: `docs/specs/2026-10-10-core-and-garden-design.md` (line 182, the evidence note).
- Modify: `docs/specs/2026-10-10-mac-app-design.md` (lines 147-148, garden subcommands).

**Phase 3b: entry authors and triage (5 files)**
- Modify: `crates/ressac-core/src/store/fetch.rs` (457 lines): the entry `UPDATE`, one test.
- Modify: `crates/ressac-core/src/store/health.rs` (292 lines): `TriageFacts`, `Store::triage_facts`.
- Modify: `crates/ressac-core/src/store/mod.rs` (line 19): re-export `TriageFacts`.
- Modify: `crates/ressac-core/src/garden.rs` (513 lines): one `verdict` function for the rows and for `recompute_triage`, one test.
- Modify: `crates/ressac-core/src/service.rs` (1355 lines before Phase 1): `save_fresh_diagnoses`, one test.

**Phase 3c: the app shows an address clash as a refusal (4 files)**
- Modify: `apps/macos/Ressac/Engine/RessacCLI.swift`, `apps/macos/Ressac/App/RunCoordinator.swift`, `apps/macos/RessacTests/RunCoordinatorTests.swift`, `docs/specs/2026-10-10-mac-app-design.md` (line 276). Details under « Phase 3c » below.

**Step 0 rule:** every change is a local edit or an addition. No file over 300 lines is restructured: `GardenRow::baseline` is replaced by a free function of the same few lines, and `diagnose_all`'s body moves into a private function under it, unchanged except its last two lines. So no cleanup commit. Check the counts with `wc -l` when each sub-phase starts; if a file has been restructured since, apply the rule.

**`TODO.md`** is updated in the plan's final docs phase, not here (see Notes).

## Decisions

- **Item 1: refusals are matched on `Error::Invalid` in the CLI, not with a new error variant.** On the paths of `garden::decide` and `garden::undo`, `Error::Invalid` comes only from `Store::record_decision` and `Store::undo_last_decision`, and every one of their `Invalid`s is a refusal: "Feed #N already uses …; cannot restore." (keep on a buried feed), "…; merge into it instead." (repair), "Cannot merge into #N: it is not an active feed.", "Nothing to repair for this feed.", and "…; cannot undo.". The other `Invalid`s of the crate (`wayback.rs:36`, `store/pages.rs:304`) are not reachable from these two calls. A typed variant would touch `error.rs`, `store/decisions.rs` and `garden.rs` (whose `apply_safe_repairs` matches `Invalid` to skip), for the same behaviour. `undo` gets the same mapping: its refusal is the same address clash.
- **Item 2: the `finished` line keeps its shape and gains two keys,** `applied` (a number) and `skipped` (one `"#id: reason"` string per skipped repair, as the text output prints them). The Mac app decodes `finished` with `decodeIfPresent(Summary.self, forKey: .summary)` and ignores unknown keys (`apps/macos/Ressac/Engine/Events.swift:61-62`), so it needs no change. The line is built from `Event::Finished` with `serde_json::to_value`, so the tag stays the core's. A separate event would have made `apply-safe` print two lines where every `garden` subcommand prints one.
- **Item 3: the filter is on the status, in the core.** The TUI (`crates/ressac/src/tui/ui.rs:174-186`) and the app (`GardenInspector.swift:59-60`, `Antiope/GardenPage.swift:207-210`) only draw `evidence.bullets`, so they follow without a change. Stored diagnoses change at the next diagnosis: every `ressac fetch` or `check` and every app refresh runs `diagnose_all` on all active feeds, even when no feed is due. `hijack_score` is still computed and stored for Alive feeds (the TUI shows the score only for Hijacked ones).
- **Item 5: one rule, two readers.** `GardenRow::needs_triage` (the TUI) and `recompute_triage` both call one `verdict(&TriageFacts)`. The rows build their facts from what `load_rows` already read; `recompute_triage` reads them with one `SELECT` (feed, health status, latest decision by `max(id)` as `latest_decisions` does, newest attempt by `at DESC, id DESC` as `attempts` does). A test compares the two readings through decisions, a burial, later fetches and an undiagnosed feed.
- **Item 5, once per refresh:** the refresh's first diagnosis (before autodiscovery) now stores the diagnoses without the flags; the final `diagnose_all` computes them. If autodiscovery or the full-article pass fails in between, the refresh returns that error and the flags stay those of the previous diagnosis until the next diagnosis, decision or undo. Both passes only fail on a database error, which the final recompute would hit too.

## Review focus

1. **A refused decision writes nothing.** `record_decision` returns before its `INSERT`, so no decision is recorded and stdout stays empty: `decisions_on_a_taken_address_exit_2` asserts the empty stdout, and `apply_safe_json_counts_applied_and_skipped_repairs` sees feed #3, refused, still undecided.
2. **The app now sees exit 2 for an address clash.** Between 3a and 3c it shows the same banner, now without the `error: ` prefix, and logs it at fault level (`RunCoordinator.swift:113-114`, "usage error from ressac"). Phase 3c (Task 3.7) makes `garden decide` and `garden undo` exits 2 a refusal: error-level log, and a banner with the sentence alone (no Show log). Exit 2 from every other call stays a fault.
3. **A feed that drops an entry's author clears it,** as a dropped title already does: `a_changed_author_reaches_the_entry_and_the_search_index` pins both the change and the clearing.
4. **The stored flags must equal the rows' verdicts in every state,** including a repaired feed fetched again (Alive baseline) and a feed with two decisions: `stored_triage_facts_match_the_rows` plus the existing `triage_flags_follow_decisions_undo_and_safe_repairs` and `a_repaired_feed_comes_back_on_any_problem_found_after_the_repair`.
5. **The hint stays on the statuses that are not Alive:** `dormant_and_fossil_feeds_keep_the_hint` and the existing `spam_hint_below_the_threshold_is_shown_in_the_evidence` (Dead).

Start by checking that the tree is green: `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"` then `cargo test --workspace` (expected after Phases 1 and 2: `ressac` unit tests 34, `cli` 17, core lib `255 passed; 0 failed; 1 ignored`, `article_proxy` 1, `contract` 8, `end_to_end` 4).

---

# Phase 3a: CLI refusals, `apply-safe` counts, spam hint

### Task 3.1: A decision on a taken address exits 2

**Files:**
- Modify: `crates/ressac/tests/cli.rs:1-8` (imports) and before `:467` (new helper and test)
- Modify: `crates/ressac/src/commands/garden.rs:53`, `:65`, `:73`, and the end of the file (unit test)
- Modify: `docs/specs/2026-10-10-mac-app-design.md:148`

**Interfaces:**
- Consumes: `crate::commands::{usage, UsageError}` (Phase 2); `ressac_core::store::Store::{open, save_diagnoses}`; `ressac_core::model::{Action, Cadence, Diagnosis, Evidence, FeedId, Status}`.
- Produces: `fn decision_error(e: ressac_core::Error) -> anyhow::Error` (private to `commands/garden.rs`); the test helper `fn seed_diagnoses(home: &Path, diagnoses: &[(FeedId, Status, Action)])` in `tests/cli.rs`, used again by Task 3.2.

Feed ids of `tests/fixtures/vienna-like.opml` on a fresh database (from `ressac page list`): #2 Orange Drenka (`https://orange.example.org/feed/`), #3 Orange-Drenka, #5 Girl on the Couch (`http://girl.example.org/rss`), #9 Six Colors (`https://sixcolors.com/feed.json`), #10 Center for Humane Technology; 11 feeds in all.

- [ ] **Step 1: Write the failing tests**

In `crates/ressac/tests/cli.rs`, replace lines 1-8:

```rust
//! Runs the real binary.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
```

with:

```rust
//! Runs the real binary.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

use ressac_core::model::{Action, Cadence, Diagnosis, Evidence, FeedId, Status};
use ressac_core::store::Store;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
```

Then insert, just before

```rust
#[test]
fn migrate_reports_the_schema_version() {
```

(line 467 before the edit above, 470 after it), the helper and the test:

```rust
/// Stores diagnoses as a refresh would, so that the garden has repairs to propose.
fn seed_diagnoses(home: &Path, diagnoses: &[(FeedId, Status, Action)]) {
    let store = Store::open(&home.join("db.sqlite")).unwrap();
    let diagnoses: HashMap<FeedId, Diagnosis> = diagnoses
        .iter()
        .map(|(id, status, action)| {
            let diagnosis = Diagnosis {
                status: *status,
                reason: format!("{status:?}"),
                evidence: Evidence::default(),
                remarks: vec![],
                action: Some(action.clone()),
                hijack_score: 0.0,
                cadence: Cadence::default(),
            };
            (*id, diagnosis)
        })
        .collect();
    store
        .save_diagnoses(&diagnoses, chrono::Utc::now())
        .unwrap();
}

/// A repair or a restore onto an address another active feed already uses is refused like
/// the other refusals: exit 2, the bare message, nothing on stdout.
#[test]
fn decisions_on_a_taken_address_exit_2() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    // Feed #2 is Orange Drenka (https://orange.example.org/feed/), #3 Orange-Drenka,
    // #9 Six Colors (https://sixcolors.com/feed.json), #10 Center for Humane Technology.
    seed_diagnoses(
        home.path(),
        &[
            (
                3,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://orange.example.org/feed/".into(),
                },
            ),
            (
                10,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://sixcolors.com/feed.json".into(),
                },
            ),
        ],
    );

    let repair = run(&["--json", "garden", "decide", "3", "repair"]);
    assert_eq!(repair.status.code(), Some(2), "{repair:?}");
    assert_eq!(
        stderr(&repair),
        "Feed #2 already uses https://orange.example.org/feed/; merge into it instead.\n"
    );
    assert!(repair.stdout.is_empty());

    // Six Colors buried, its address goes to #10; restoring Six Colors is then refused.
    assert!(run(&["garden", "decide", "9", "bury"]).status.success());
    assert!(run(&["garden", "decide", "10", "repair"]).status.success());
    let keep = run(&["garden", "decide", "9", "keep"]);
    assert_eq!(keep.status.code(), Some(2), "{keep:?}");
    assert_eq!(
        stderr(&keep),
        "Feed #10 already uses https://sixcolors.com/feed.json; cannot restore.\n"
    );
    assert!(keep.stdout.is_empty());
}

```

At the end of `crates/ressac/src/commands/garden.rs` (after line 95, the closing `}` of `garden_command`), add the unit test:

```rust

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::UsageError;

    #[test]
    fn decision_refusals_are_usage_errors_and_others_stay_fatal() {
        let text = "Feed #2 already uses https://e.org/feed; merge into it instead.";
        let refusal = decision_error(ressac_core::Error::Invalid(text.into()));
        assert_eq!(
            refusal
                .downcast_ref::<UsageError>()
                .map(ToString::to_string),
            Some(text.to_string())
        );
        let fatal = decision_error(ressac_core::Error::Io(std::io::Error::other("disk")));
        assert!(fatal.downcast_ref::<UsageError>().is_none());
    }
}
```

- [ ] **Step 2: Run them to see them fail**

```bash
cargo test -p ressac --test cli decisions_on_a_taken_address_exit_2
cargo test -p ressac --bin ressac decision_refusals
```

Expected: the CLI test fails at its first exit-code assertion: `left: Some(1)`, `right: Some(2)`, the `Output` showing `stderr: "error: Feed #2 already uses https://orange.example.org/feed/; merge into it instead.\n"`. The unit test does not compile: ``cannot find function `decision_error` in this scope``.

- [ ] **Step 3: Map the refusals to usage errors**

In `crates/ressac/src/commands/garden.rs`, replace line 53:

```rust
/// `ressac garden decide | undo | apply-safe`: the TUI's keys, one result line each.
```

with:

```rust
/// The refusals of `garden::decide` and `garden::undo` (an address another active feed
/// already uses, a merge target that is not active) are usage errors (exit 2), as
/// `check_decision`'s are; any other error stays fatal (exit 1).
fn decision_error(e: ressac_core::Error) -> anyhow::Error {
    match e {
        ressac_core::Error::Invalid(message) => usage(message),
        other => other.into(),
    }
}

/// `ressac garden decide | undo | apply-safe`: the TUI's keys, one result line each.
```

Replace line 65 (75 after the insertion above):

```rust
            garden::decide(store, feed_id, decision, Utc::now())?;
```

with:

```rust
            garden::decide(store, feed_id, decision, Utc::now()).map_err(decision_error)?;
```

Replace line 73 (83 after the insertion above):

```rust
            let feed_id = garden::undo(store)?;
```

with:

```rust
            let feed_id = garden::undo(store).map_err(decision_error)?;
```

In `docs/specs/2026-10-10-mac-app-design.md`, replace line 148:

```markdown
- A decision on a buried or unknown feed exits `2` with a message.
```

with:

```markdown
- A decision on a buried or unknown feed, a repair with nothing to repair, and a decision or an undo the store refuses (an address another active feed already uses, a merge target that is not active) exit `2` with a message.
```

Line 276 (what the app does with exit 2) is Phase 3c's: Task 3.7 rewrites it with the app change.

- [ ] **Step 4: Run them to see them pass**

```bash
cargo test -p ressac --test cli decisions_on_a_taken_address_exit_2
cargo test -p ressac --bin ressac decision_refusals
cargo test -p ressac
```

Expected: both pass; then `ressac` unit tests 35 and `cli` 18, `0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/commands/garden.rs crates/ressac/tests/cli.rs docs/specs/2026-10-10-mac-app-design.md
git commit -F - <<'EOF'
CLI: a decision on an address another feed uses exits 2

keep and repair onto an address another active feed already uses, and an
undo that would clash the same way, were fatal errors (exit 1, "error: "
prefix). They are refusals like the others: exit 2, the bare message.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3.2: `apply-safe --json` counts what it applied and skipped

**Files:**
- Modify: `crates/ressac/tests/cli.rs` (one test, after Task 3.1's)
- Modify: `crates/ressac/src/commands/garden.rs:81-92` (Phase 2 numbering; `:91-102` after Task 3.1)
- Modify: `docs/specs/2026-10-10-mac-app-design.md:147`

**Interfaces:**
- Consumes: `seed_diagnoses` (Task 3.1); `garden::apply_safe_repairs(&Store, DateTime<Utc>) -> Result<(usize, Vec<String>)>` (unchanged: the skipped strings are `"#id: reason"`).
- Produces: the JSON line `{"event":"finished","summary":{…},"applied":N,"skipped":["#id: reason", …]}`.

- [ ] **Step 1: Write the failing test**

In `crates/ressac/tests/cli.rs`, insert just before `#[test]\nfn migrate_reports_the_schema_version() {` (after Task 3.1's test):

```rust
/// `apply-safe --json` prints the `finished` event with what it did: the number of repairs
/// applied and why the others were skipped. The text output says the same.
#[test]
fn apply_safe_json_counts_applied_and_skipped_repairs() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    // #3's redirect target is #2's address, so it is skipped; #5's https upgrade applies.
    seed_diagnoses(
        home.path(),
        &[
            (
                3,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://orange.example.org/feed/".into(),
                },
            ),
            (
                5,
                Status::Alive,
                Action::UpgradeHttps {
                    url: "https://girl.example.org/rss".into(),
                },
            ),
        ],
    );
    let skipped =
        "#3: Feed #2 already uses https://orange.example.org/feed/; merge into it instead.";

    let applied = run(&["--json", "garden", "apply-safe"]);
    assert!(applied.status.success(), "{applied:?}");
    let lines = json_lines(&applied);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 11);
    assert_eq!(lines[0]["applied"], 1);
    assert_eq!(lines[0]["skipped"], json!([skipped]));
    assert!(stdout(&run(&["export"])).contains("https://girl.example.org/rss"));

    // #3 is still undecided, so a second run skips it again; #5 is decided now.
    assert_eq!(
        stdout(&run(&["garden", "apply-safe"])),
        format!("Applied 0 safe repairs. Skipped 1: {skipped}\n")
    );
}

```

- [ ] **Step 2: Run it to see it fail**

```bash
cargo test -p ressac --test cli apply_safe_json_counts_applied_and_skipped_repairs
```

Expected: FAIL at `assert_eq!(lines[0]["applied"], 1)` with `left: Null`, `right: 1`.

- [ ] **Step 3: Add the counts to the `finished` line**

In `crates/ressac/src/commands/garden.rs`, replace:

```rust
        GardenCommand::ApplySafe => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            let event = Event::Finished {
                summary: app.summary()?,
            };
            output::write_result(
                out,
                json,
                &event,
                &output::apply_safe_text(applied, &skipped),
            )?;
        }
```

with:

```rust
        GardenCommand::ApplySafe => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            if json {
                // The `finished` event, plus what this command did: the app reads the
                // summary and ignores the extra keys.
                let mut line = serde_json::to_value(Event::Finished {
                    summary: app.summary()?,
                })?;
                line["applied"] = applied.into();
                line["skipped"] = skipped.into();
                writeln!(out, "{line}")?;
            } else {
                writeln!(out, "{}", output::apply_safe_text(applied, &skipped))?;
            }
        }
```

The text output is unchanged; it no longer computes a summary it does not print.

In `docs/specs/2026-10-10-mac-app-design.md`, replace line 147:

```markdown
- They emit `Decided { feed_id, decision }`, `Undone { feed_id }` and `Finished { summary }`.
```

with:

```markdown
- They emit `Decided { feed_id, decision }`, `Undone { feed_id }` and `Finished { summary }`. The `finished` line of `apply-safe` also has `applied` (the number of repairs applied) and `skipped` (one `"#id: reason"` string per repair skipped).
```

- [ ] **Step 4: Run the CLI tests**

```bash
cargo test -p ressac
```

Expected: `ressac` unit tests 35, `cli` 19, `0 failed`. The older `garden_subcommands_decide_undo_and_apply_safe` still passes: it reads `event` and `summary` only.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/commands/garden.rs crates/ressac/tests/cli.rs docs/specs/2026-10-10-mac-app-design.md
git commit -F - <<'EOF'
CLI: apply-safe --json gives the repairs applied and skipped

The finished line keeps its summary and gains "applied" (a number) and
"skipped" (one "#id: reason" per skipped repair), as the text output says.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3.3: No spam hint on Alive feeds

**Files:**
- Modify: `crates/ressac-core/src/health/mod.rs:79-83` and tests before `:875`
- Modify: `docs/specs/2026-10-10-core-and-garden-design.md:182`

**Interfaces:** none new. `diagnose(&FeedEvidence) -> Diagnosis` keeps its signature; only `evidence.bullets` changes for Alive feeds.

- [ ] **Step 1: Write the tests**

In `crates/ressac-core/src/health/mod.rs`, insert just before

```rust
    #[test]
    fn clean_feed_gets_no_spam_hint() {
```

(line 875):

```rust
    /// Decision of 15/10/2026: the hint is for feeds that are not alive. On the real run,
    /// ReadWriteWeb (Alive) showed it for betting words in a tech site.
    #[test]
    fn alive_feed_with_spam_words_gets_no_hint() {
        let mut e = ev(vec![ok(0)], vec![entry(3)]);
        e.description = Some("Les meilleurs casino en ligne pour les joueurs français.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Alive);
        assert!(d.hijack_score > 0.0, "the score itself is still stored");
        assert!(
            d.evidence
                .bullets
                .iter()
                .all(|b| !b.contains("spam") && !b.contains("casino")),
            "{:?}",
            d.evidence.bullets
        );
    }

    #[test]
    fn dormant_and_fossil_feeds_keep_the_hint() {
        for (days, status) in [(200, Status::Dormant), (800, Status::Fossil)] {
            let mut e = ev(vec![ok(0)], vec![entry(days)]);
            e.description = Some("Les meilleurs casino en ligne pour les joueurs français.".into());
            let d = diagnose(&e);
            assert_eq!(d.status, status);
            assert!(
                d.evidence
                    .bullets
                    .iter()
                    .any(|b| b.starts_with("Possible spam signs (score 0.4")),
                "{status:?}: {:?}",
                d.evidence.bullets
            );
        }
    }

```

- [ ] **Step 2: Run them**

```bash
cargo test -p ressac-core --lib health::tests
```

Expected: `alive_feed_with_spam_words_gets_no_hint` FAILS and prints the bullets: `["Last entry on 2026-10-07.", "About 0.1 entries per month over the last year.", "Possible spam signs (score 0.4, below the 0.6 threshold):", "Spam vocabulary: \"casino\"."]`. `dormant_and_fossil_feeds_keep_the_hint` passes already: it pins what must not change.

- [ ] **Step 3: Restrict the hint**

Replace lines 79-83:

```rust
    let assessed = !matches!(
        v.status,
        Status::Hijacked | Status::Duplicate | Status::Unchecked
    );
    if assessed && hijack.score > 0.0 {
```

with:

```rust
    // The hint is for feeds already in doubt: never on an Alive feed, where a few
    // spam-like words are noise (decision of 15/10/2026), nor on a verdict that is not an
    // assessment of the feed itself.
    let hinted = !matches!(
        v.status,
        Status::Alive | Status::Hijacked | Status::Duplicate | Status::Unchecked
    );
    if hinted && hijack.score > 0.0 {
```

In `docs/specs/2026-10-10-core-and-garden-design.md`, replace line 182:

```markdown
**Evidence note:** when `hijack_score > 0` but the feed is not Hijacked (and is neither Duplicate nor Unchecked), the evidence bullets end with `Possible spam signs (score 0.4, below the 0.6 threshold):` followed by the hijack bullets. The reason and clue are unchanged, and the language hint stays Hijacked-only (a Dead feed with a 0.4 score is shown as Dead, with the hint underneath).
```

with:

```markdown
**Evidence note:** when `hijack_score > 0` but the feed is not Hijacked (and is neither Alive, Duplicate nor Unchecked), the evidence bullets end with `Possible spam signs (score 0.4, below the 0.6 threshold):` followed by the hijack bullets. The reason and clue are unchanged, and the language hint stays Hijacked-only (a Dead feed with a 0.4 score is shown as Dead, with the hint underneath). Alive feeds never get the hint (decision of 15/10/2026: on the real run, ReadWriteWeb showed it for betting words in a tech site); their score is still stored.
```

- [ ] **Step 4: Run them to see them pass**

```bash
cargo test -p ressac-core --lib health::tests
```

Expected: every `health::tests` test passes, `0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/health/mod.rs docs/specs/2026-10-10-core-and-garden-design.md
git commit -F - <<'EOF'
Health: no spam hint on Alive feeds

Clément's decision of 15/10/2026: the "Possible spam signs" bullets are for
feeds that are not alive (Dormant, Fossil, Ailing, Dead, Moved). Five alive
feeds of the real run, ReadWriteWeb among them, showed them for noise.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Phase 3a verification gate

- [ ] **Step 1: Run the Rust gate**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
```

Expected: `fmt` prints nothing; clippy ends with `Finished` and no warning; every `test result:` line says `0 failed`: `ressac` unit tests 35, `cli` 19, core lib 257 passed and 1 ignored, `article_proxy` 1, `contract` 8, `end_to_end` 4.

No Swift file changed: the app's `finished` decoding is unchanged and ignores the new keys, so the Xcode gate is not needed here.

- [ ] **Step 2: Stop.** Report the gate output and wait for Clément's approval before Phase 3b.

---

# Phase 3b: entry authors and a lean triage

### Task 3.4: A changed author reaches the entry and the search index

**Files:**
- Modify: `crates/ressac-core/src/store/fetch.rs:98-103`, `:127-143` and a test after `:456`

**Interfaces:** none new. `Store::record_attempt(&self, run_id: &str, a: &FetchAttempt) -> Result<usize>` keeps its signature and its count of new entries.

- [ ] **Step 1: Write the failing test**

In `crates/ressac-core/src/store/fetch.rs`, at the end of `mod tests`, after `entries_are_indexed_and_reindexed_only_when_they_change` (whose last lines are `store.record_attempt("r3", &third).unwrap();`, `assert_eq!(store.search_ids("gamma").unwrap(), [1]);`, `}`, lines 454-456), insert before the module's closing `}` (line 457):

```rust

    /// A new author alone is a change: the row and its search row follow the feed.
    #[test]
    fn a_changed_author_reaches_the_entry_and_the_search_index() {
        let store = store_with_feeds(1);
        let mut e = entry("a");
        e.author = Some("Ada Lovelace".into());
        let mut first = attempt(1, now(), None, &[]);
        first.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r1", &first).unwrap();
        assert_eq!(store.search_ids("author:ada").unwrap(), [1]);

        e.author = Some("Grace Hopper".into());
        let mut second = attempt(1, now() + Duration::hours(1), None, &[]);
        second.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r2", &second).unwrap();
        let author: Option<String> = store
            .conn()
            .query_row("SELECT author FROM entry WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(author.as_deref(), Some("Grace Hopper"));
        assert_eq!(store.search_ids("author:grace").unwrap(), [1]);
        assert!(store.search_ids("author:ada").unwrap().is_empty());

        // A feed that drops the author clears it too.
        e.author = None;
        let mut third = attempt(1, now() + Duration::hours(2), None, &[]);
        third.feed.as_mut().unwrap().entries = vec![e];
        store.record_attempt("r3", &third).unwrap();
        assert!(store.search_ids("author:grace").unwrap().is_empty());
        assert_eq!(store.search_ids("title:A").unwrap(), [1]);
    }
```

- [ ] **Step 2: Run it to see it fail**

```bash
cargo test -p ressac-core --lib a_changed_author_reaches_the_entry_and_the_search_index
```

Expected: FAIL with `left: Some("Ada Lovelace")`, `right: Some("Grace Hopper")`: the entry is not even rewritten, because the author is neither set nor compared.

- [ ] **Step 3: Set and compare the author**

Replace lines 98-103:

```rust
            // Only a real change rewrites the row, and only then is the search row redone.
            let mut update = tx.prepare(
                "UPDATE entry SET url = ?3, title = ?4, updated_at = ?5, summary_html = ?6, content_html = ?7 \
                 WHERE feed_id = ?1 AND guid = ?2 AND (url IS NOT ?3 OR title IS NOT ?4 \
                 OR updated_at IS NOT ?5 OR summary_html IS NOT ?6 OR content_html IS NOT ?7) \
                 RETURNING id, author",
            )?;
```

with:

```rust
            // Only a real change rewrites the row, and only then is the search row redone.
            let mut update = tx.prepare(
                "UPDATE entry SET url = ?3, title = ?4, author = ?5, updated_at = ?6, summary_html = ?7, \
                 content_html = ?8 WHERE feed_id = ?1 AND guid = ?2 AND (url IS NOT ?3 OR title IS NOT ?4 \
                 OR author IS NOT ?5 OR updated_at IS NOT ?6 OR summary_html IS NOT ?7 OR content_html IS NOT ?8) \
                 RETURNING id",
            )?;
```

Replace lines 127-143:

```rust
                    let changed: Option<(i64, Option<String>)> = update
                        .query_row(
                            params![
                                a.feed_id,
                                e.guid,
                                e.url,
                                e.title,
                                e.updated_at.map(ts),
                                e.summary_html,
                                e.content_html
                            ],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    if let Some((id, author)) = changed {
                        index_entry(&tx, id, e.title.as_deref(), author.as_deref(), html)?;
                    }
```

with:

```rust
                    let changed: Option<i64> = update
                        .query_row(
                            params![
                                a.feed_id,
                                e.guid,
                                e.url,
                                e.title,
                                e.author,
                                e.updated_at.map(ts),
                                e.summary_html,
                                e.content_html
                            ],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if let Some(id) = changed {
                        index_entry(&tx, id, e.title.as_deref(), e.author.as_deref(), html)?;
                    }
```

`index_entry` already writes the author into `entry_fts`, also for an entry with an extracted article (it updates title and author and keeps the article body).

- [ ] **Step 4: Run the store tests**

```bash
cargo test -p ressac-core --lib store::
```

Expected: all pass, `a_changed_author_reaches_the_entry_and_the_search_index` and `entries_are_indexed_and_reindexed_only_when_they_change` included.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/fetch.rs
git commit -F - <<'EOF'
Store: a changed author updates the entry and its search row

The entry UPDATE neither set nor compared the author (inherited from v1),
so a new author reached neither entry.author nor entry_fts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3.5: `recompute_triage` reads one light row per feed

**Files:**
- Modify: `crates/ressac-core/src/store/health.rs:8-10`, before `:52`, before `:122`
- Modify: `crates/ressac-core/src/store/mod.rs:19`
- Modify: `crates/ressac-core/src/garden.rs:7`, `:42-67`, `:121-128`, and a test before `:462`

**Interfaces:**
- Consumes: `store::fetch::bad_timestamp(col: usize) -> rusqlite::Error` (`pub(super)`, visible to its sibling `health`); `Decision::parse`, `Status::parse`, `parse_ts`.
- Produces: `pub struct ressac_core::store::TriageFacts { pub feed_id: FeedId, pub buried: bool, pub status: Status, pub decision: Option<(Decision, Status, DateTime<Utc>)>, pub last_attempt_at: Option<DateTime<Utc>> }` (derives `Debug, Clone, PartialEq`); `pub fn Store::triage_facts(&self) -> Result<Vec<TriageFacts>>`, ordered by feed id; in `garden.rs`, the private `fn verdict(f: &TriageFacts) -> bool` and `fn GardenRow::triage_facts(&self) -> TriageFacts`. `pub fn garden::needs_triage(Status, Option<Status>) -> bool` and `pub fn GardenRow::needs_triage(&self) -> bool` keep their signatures.

- [ ] **Step 1: Write the failing test**

In `crates/ressac-core/src/garden.rs`, insert just before

```rust
    #[test]
    fn apply_safe_repairs_recomputes_triage_even_when_it_fails() {
```

(line 462):

```rust
    /// `recompute_triage` reads `Store::triage_facts`, the TUI reads rows: both must agree,
    /// through decisions, a burial, later fetches and a feed never diagnosed.
    #[test]
    fn stored_triage_facts_match_the_rows() {
        let s = store();
        s.import_outlines(
            &opml::parse(r#"<opml><body><outline text="New" xmlUrl="https://n.example.org/feed"/></body></opml>"#)
                .unwrap(),
            false,
            now(),
        )
        .unwrap();
        let check = |s: &Store| {
            let mut from_rows: Vec<TriageFacts> = load_rows(s)
                .unwrap()
                .iter()
                .map(GardenRow::triage_facts)
                .collect();
            from_rows.sort_by_key(|f| f.feed_id);
            assert_eq!(s.triage_facts().unwrap(), from_rows);
        };
        check(&s);
        assert_eq!(s.triage_facts().unwrap()[4].status, Status::Unchecked);
        decide(&s, 3, Decision::Keep, now()).unwrap();
        decide(&s, 3, Decision::Bury, now()).unwrap();
        decide(&s, 4, Decision::Repair, now()).unwrap();
        for hours in [1, 2] {
            s.record_attempt(
                "r",
                &failed_attempt(4, now() + chrono::Duration::hours(hours)),
            )
            .unwrap();
        }
        s.save_diagnoses(&HashMap::from([(4, diag(Status::Dead, None))]), now())
            .unwrap();
        check(&s);
        let facts = s.triage_facts().unwrap();
        assert_eq!(
            facts[2].decision.map(|(d, _, _)| d),
            Some(Decision::Bury),
            "the latest decision"
        );
        assert!(facts[2].buried);
        assert_eq!(
            facts[3].last_attempt_at,
            Some(now() + chrono::Duration::hours(2))
        );
        recompute_triage(&s).unwrap();
        assert_eq!(
            stored_flags(&s)[3],
            (4, true),
            "Dead after a repair and a fetch"
        );
        assert_flags_match_rows(&s);
    }

```

Feeds of the test store: 1 Dead (Zed), 2 Alive (alpha), 3 Fossil (Old), 4 Moved (Mover), and 5 (New), never diagnosed.

- [ ] **Step 2: Run it to see it fail**

```bash
cargo test -p ressac-core --lib stored_triage_facts_match_the_rows
```

Expected: does not compile: ``cannot find type `TriageFacts` in this scope``, and no `triage_facts` on `Store` nor on `GardenRow`.

- [ ] **Step 3: Add `TriageFacts` and `Store::triage_facts`**

In `crates/ressac-core/src/store/health.rs`, replace lines 8-10:

```rust
use super::Store;
use crate::error::Result;
use crate::model::{Cadence, Diagnosis, FeedId, Status, parse_ts, ts};
```

with:

```rust
use super::Store;
use super::fetch::bad_timestamp;
use crate::error::Result;
use crate::model::{Cadence, Decision, Diagnosis, FeedId, Status, parse_ts, ts};
```

Insert just before `fn write_triage(conn: &rusqlite::Connection, flags: &HashMap<FeedId, bool>) -> Result<()> {` (line 52):

```rust
/// What the garden's triage rule reads for one feed (`garden::recompute_triage`).
#[derive(Debug, Clone, PartialEq)]
pub struct TriageFacts {
    pub feed_id: FeedId,
    pub buried: bool,
    /// `Unchecked` while the feed has no diagnosis.
    pub status: Status,
    /// The latest decision: what was decided, the status at the time, and when.
    pub decision: Option<(Decision, Status, DateTime<Utc>)>,
    /// When the newest fetch attempt ran.
    pub last_attempt_at: Option<DateTime<Utc>>,
}

/// A status or decision name `parse` does not know, as a column conversion error.
fn unknown(col: usize, what: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        format!("unknown {what} {value:?}").into(),
    )
}

fn triage_row(r: &Row<'_>) -> rusqlite::Result<TriageFacts> {
    let status: Option<String> = r.get(2)?;
    let decision: Option<String> = r.get(3)?;
    let decision = match decision {
        None => None,
        Some(name) => {
            let at_decision: String = r.get(4)?;
            let decided_at: String = r.get(5)?;
            Some((
                Decision::parse(&name).ok_or_else(|| unknown(3, "decision", &name))?,
                Status::parse(&at_decision).ok_or_else(|| unknown(4, "status", &at_decision))?,
                parse_ts(&decided_at).ok_or_else(|| bad_timestamp(5))?,
            ))
        }
    };
    let last_attempt: Option<String> = r.get(6)?;
    Ok(TriageFacts {
        feed_id: r.get(0)?,
        buried: r.get(1)?,
        status: match status {
            None => Status::Unchecked,
            Some(name) => Status::parse(&name).ok_or_else(|| unknown(2, "status", &name))?,
        },
        decision,
        last_attempt_at: last_attempt
            .map(|at| parse_ts(&at).ok_or_else(|| bad_timestamp(6)))
            .transpose()?,
    })
}

```

Insert just before `    pub fn diagnosis(&self, feed_id: FeedId) -> Result<Option<Diagnosis>> {` (line 122):

```rust
    /// The triage facts of every feed, by id, in one query: no entries, no folder paths,
    /// and only the newest fetch attempt's time.
    pub fn triage_facts(&self) -> Result<Vec<TriageFacts>> {
        let mut stmt = self.conn().prepare(
            "SELECT f.id, f.state = 'buried', h.status, d.decision, d.status_at_decision, d.decided_at, \
             (SELECT l.at FROM fetch_log l WHERE l.feed_id = f.id ORDER BY l.at DESC, l.id DESC LIMIT 1) \
             FROM feed f \
             LEFT JOIN feed_health h ON h.feed_id = f.id \
             LEFT JOIN garden_decision d \
               ON d.id = (SELECT max(g.id) FROM garden_decision g WHERE g.feed_id = f.id) \
             ORDER BY f.id",
        )?;
        let rows = stmt
            .query_map([], triage_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

```

In `crates/ressac-core/src/store/mod.rs`, replace line 19:

```rust
pub use fetch::{AttemptRecord, BACKLOG_DAYS, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
```

with:

```rust
pub use fetch::{AttemptRecord, BACKLOG_DAYS, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
pub use health::TriageFacts;
```

- [ ] **Step 4: One verdict for the rows and for `recompute_triage`**

In `crates/ressac-core/src/garden.rs`, replace line 7:

```rust
use crate::store::{AttemptRecord, DecisionRecord, Store};
```

with:

```rust
use crate::store::{AttemptRecord, DecisionRecord, Store, TriageFacts};
```

Replace lines 42-67:

```rust
    pub fn needs_triage(&self) -> bool {
        !self.buried
            && needs_triage(
                self.status(),
                self.decision.as_ref().map(|d| self.baseline(d)),
            )
    }

    /// The status a decision is measured against. A repair is meant to leave the feed
    /// healthy, so once the feed was fetched again after it, any problem status brings it
    /// back (Alive baseline). Until then its diagnosis still describes the old address.
    fn baseline(&self, d: &DecisionRecord) -> Status {
        let fetched_since = self.attempts.first().is_some_and(|a| a.at > d.decided_at);
        if d.decision == Decision::Repair && fetched_since {
            Status::Alive
        } else {
            d.status_at_decision
        }
    }
}

/// A feed needs triage when its status is a problem and it is undecided, or it got
/// strictly more urgent than its decision baseline (spec §9).
pub fn needs_triage(status: Status, decided_at: Option<Status>) -> bool {
    status.needs_triage() && decided_at.is_none_or(|prev| status.urgency() < prev.urgency())
}
```

with:

```rust
    pub fn needs_triage(&self) -> bool {
        verdict(&self.triage_facts())
    }

    /// The facts `recompute_triage` reads from the store, taken from this row.
    fn triage_facts(&self) -> TriageFacts {
        TriageFacts {
            feed_id: self.feed_id,
            buried: self.buried,
            status: self.status(),
            decision: self
                .decision
                .as_ref()
                .map(|d| (d.decision, d.status_at_decision, d.decided_at)),
            last_attempt_at: self.attempts.first().map(|a| a.at),
        }
    }
}

/// A feed needs triage when its status is a problem and it is undecided, or it got
/// strictly more urgent than its decision baseline (spec §9).
pub fn needs_triage(status: Status, decided_at: Option<Status>) -> bool {
    status.needs_triage() && decided_at.is_none_or(|prev| status.urgency() < prev.urgency())
}

/// One feed's triage verdict. A buried feed never needs triage. A decision is measured
/// against the status it was taken on, except a repair: it is meant to leave the feed
/// healthy, so once the feed was fetched again after it, any problem status brings it
/// back (Alive baseline). Until then its diagnosis still describes the old address.
fn verdict(f: &TriageFacts) -> bool {
    let baseline = f.decision.map(|(decision, at_decision, decided_at)| {
        let fetched_since = f.last_attempt_at.is_some_and(|at| at > decided_at);
        if decision == Decision::Repair && fetched_since {
            Status::Alive
        } else {
            at_decision
        }
    });
    !f.buried && needs_triage(f.status, baseline)
}
```

Replace lines 121-128:

```rust
pub fn recompute_triage(store: &Store) -> Result<()> {
    store.update_triage(|| {
        Ok(load_rows(store)?
            .iter()
            .map(|r| (r.feed_id, r.needs_triage()))
            .collect())
    })
}
```

with:

```rust
pub fn recompute_triage(store: &Store) -> Result<()> {
    store.update_triage(|| {
        Ok(store
            .triage_facts()?
            .iter()
            .map(|f| (f.feed_id, verdict(f)))
            .collect())
    })
}
```

The doc comment above it (lines 119-120) stays: it still holds, and `update_triage` still reads inside its immediate transaction (`recompute_triage_holds_the_write_lock_while_it_reads`).

- [ ] **Step 5: Run the garden and store tests**

```bash
cargo test -p ressac-core --lib
```

Expected: `0 failed`, `stored_triage_facts_match_the_rows`, `triage_flags_follow_decisions_undo_and_safe_repairs`, `a_repaired_feed_comes_back_on_any_problem_found_after_the_repair`, `apply_safe_repairs_recomputes_triage_even_when_it_fails` and the `store::health` tests included.

- [ ] **Step 6: Commit**

```bash
git add crates/ressac-core/src/store/health.rs crates/ressac-core/src/store/mod.rs crates/ressac-core/src/garden.rs
git commit -F - <<'EOF'
Garden: recompute_triage reads one light row per feed

It loaded every garden row (folder paths, three entries and five attempts
per feed) to read five facts. Store::triage_facts reads them in one query,
and the rows and the stored flags share one verdict function.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3.6: A refresh computes the triage flags once

**Files:**
- Modify: `crates/ressac-core/src/service.rs:259`, `:424-478`, and a test before `:960`

**Interfaces:**
- Produces: `fn Ressac::save_fresh_diagnoses(&self, now: DateTime<Utc>) -> Result<()>`, private. `pub fn Ressac::diagnose_all(&self, now: DateTime<Utc>) -> Result<Summary>` keeps its signature and behaviour.

Line numbers are those of `855cd2c`; Phase 1 edits lines 257 and 266-267 of `refresh` and adds code elsewhere in the file, so find each place by its quoted text.

- [ ] **Step 1: Write the failing test**

In the `tests` module of `crates/ressac-core/src/service.rs`, insert just before

```rust
    #[test]
    fn reading_and_garden_events_have_stable_json() {
```

(lines 960-961 at `855cd2c`):

```rust
    /// A refresh diagnoses twice; only its last diagnosis, `diagnose_all`, writes the triage
    /// flags, so the first one leaves them alone.
    #[test]
    fn only_diagnose_all_writes_the_triage_flags() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            false,
        )
        .unwrap();
        let flag = |app: &Ressac| -> bool {
            app.store()
                .conn()
                .query_row(
                    "SELECT needs_triage FROM v_garden_row WHERE feed_id = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        app.save_fresh_diagnoses(Utc::now()).unwrap();
        // A stale flag on the Unchecked feed shows who writes it.
        app.store()
            .conn()
            .execute(
                "UPDATE feed_health SET needs_triage = 1 WHERE feed_id = 1",
                [],
            )
            .unwrap();
        app.save_fresh_diagnoses(Utc::now()).unwrap();
        assert!(flag(&app), "the first diagnosis leaves the flag alone");
        app.diagnose_all(Utc::now()).unwrap();
        assert!(!flag(&app), "diagnose_all recomputes it");
    }

```

- [ ] **Step 2: Run it to see it fail**

```bash
cargo test -p ressac-core --lib only_diagnose_all_writes_the_triage_flags
```

Expected: does not compile: ``no method named `save_fresh_diagnoses` found for struct `Ressac` ``.

- [ ] **Step 3: Split `diagnose_all`**

Replace the doc comment and the first lines of `diagnose_all` (lines 424-426):

```rust
    /// Diagnoses every active feed, stores the results and the garden's triage flags.
    pub fn diagnose_all(&self, now: DateTime<Utc>) -> Result<Summary> {
        let feeds: Vec<FeedRow> = self
```

with:

```rust
    /// Diagnoses every active feed, stores the results and the garden's triage flags.
    pub fn diagnose_all(&self, now: DateTime<Utc>) -> Result<Summary> {
        self.save_fresh_diagnoses(now)?;
        garden::recompute_triage(&self.store)?;
        self.summary()
    }

    /// Diagnoses every active feed and stores the results, leaving the triage flags alone.
    fn save_fresh_diagnoses(&self, now: DateTime<Utc>) -> Result<()> {
        let feeds: Vec<FeedRow> = self
```

and its last lines (lines 474-478):

```rust
        apply_similar_titles(&inputs, &mut diags);
        self.store.save_diagnoses(&diags, now)?;
        garden::recompute_triage(&self.store)?;
        self.summary()
    }
```

with:

```rust
        apply_similar_titles(&inputs, &mut diags);
        self.store.save_diagnoses(&diags, now)
    }
```

- [ ] **Step 4: Use it for the refresh's first diagnosis**

In `refresh`, replace (lines 259-260):

```rust
                self.diagnose_all(Utc::now())?;
                self.discover_replacements(sink, Utc::now(), &stop).await?;
```

with:

```rust
                // Triage waits for the final diagnosis below.
                self.save_fresh_diagnoses(Utc::now())?;
                self.discover_replacements(sink, Utc::now(), &stop).await?;
```

The final `let mut summary = self.diagnose_all(Utc::now())?;` of `refresh` (line 269) is unchanged: it runs on every path that does not return an error, and computes the flags from the final diagnoses.

- [ ] **Step 5: Run the service tests**

```bash
cargo test -p ressac-core --lib service::
```

Expected: all pass, `only_diagnose_all_writes_the_triage_flags` and `a_subset_refresh_fetches_only_the_named_active_feeds_and_diagnoses_all` (which checks the flag a refresh stores) included.

- [ ] **Step 6: Commit**

```bash
git add crates/ressac-core/src/service.rs
git commit -F - <<'EOF'
Service: a refresh computes the triage flags once

The first diagnosis of a refresh (before autodiscovery) recomputed the
flags that the final diagnose_all recomputes anyway. It now only stores
the diagnoses.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Phase 3b verification gate

- [ ] **Step 1: Run the Rust gate**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
```

Expected: `fmt` prints nothing; clippy ends with `Finished` and no warning; every `test result:` line says `0 failed`: `ressac` unit tests 35, `cli` 19, core lib 260 passed and 1 ignored, `article_proxy` 1, `contract` 8, `end_to_end` 4.

No Swift file changed and the schema is unchanged (v3), so the Xcode gate waits for Phase 3c.

- [ ] **Step 2: Stop.** Report the gate output and wait for Clément's approval before Phase 3c.

---

# Phase 3c: the app shows an address clash as a refusal

Clément's decision of 15/10/2026: Task 3.1 makes `garden decide` and `garden undo` exit 2 on an address another active feed already uses, and the app must not treat that as a bug in its own call.

**Files (4)**
- Modify: `apps/macos/Ressac/Engine/RessacCLI.swift` (217 lines): `RunRequest.isRefusal(_:)`.
- Modify: `apps/macos/Ressac/App/RunCoordinator.swift` (153 lines): log level and banner of a refusal.
- Modify: `apps/macos/RessacTests/RunCoordinatorTests.swift` (118 lines): two tests.
- Modify: `docs/specs/2026-10-10-mac-app-design.md` (line 276, exit 2; one line, so Phase 7's citation of line 277 stays right).

**Decision: classify exit 2 per call, and show the sentence in the banner.**
- **Per call, not per message.** `RunRequest` knows its command line. Only `garden decide` and `garden undo` reach the store's refusals (Decisions, item 1); exit 2 from any other call (`fetch --feed`, `extract`, `page set`, `migrate`, `apply-safe`) still means the app built a wrong command line, and keeps its fault-level log. Matching the stderr text would tie the app to the core's wording.
- **The banner, not `GardenStore.message`.** Every garden run, whatever started it (a key through `GardenStore.perform`, the Native inspector's Keep, Repair and Bury buttons, which call `AppState.gardenDecide` directly, the header's Undo, ⌘Z in Antiope), ends in `RunCoordinator.execute`, and both looks draw its banner (`BannerView` in `MainView.swift`, `AntiopeBannerRow` in `AntiopeRootView.swift`). `GardenStore.message` is only fed by key commands, so the inspector's buttons would lose the sentence, and routing them through the store would mean moving the store out of `GardenTable` (`MainView.swift`, which Phases 6 and 7 edit). A refusal's banner has no **Show log** button: the sentence says all there is, and the log holds nothing more. The next successful short run clears it, as for any failure.

### Task 3.7: Exit 2 from a garden decision is a refusal, not a bug

**Interfaces:**
- Consumes: `Stub` (`EngineTests.swift:47`; the subcommand starts at `$4`), `RunFailure`, `Banner`, `RunRequest.{decide, undo, applySafe, fetch, migrate}`.
- Produces: `func RunRequest.isRefusal(_ failure: RunFailure) -> Bool`.

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/RunCoordinatorTests.swift`, insert before `    @Test func extraction_failure_stays_out_of_the_banner() async throws {`:

```swift
    /// An address clash refuses a garden decision or an undo with exit 2: the banner shows
    /// the CLI's sentence, without Show log. Exit 2 from a call the app builds alone is still
    /// a bug, with the log button.
    @Test func a_refused_garden_decision_shows_its_sentence_without_the_log_button() async throws {
        let stub = try Stub("""
            case "$4 $5" in
              "garden decide") echo 'Feed #2 already uses https://e.org/feed; merge into it instead.' >&2 ;;
              "garden undo") echo 'Feed #10 already uses https://e.org/feed; cannot undo.' >&2 ;;
              *) echo 'No feed #9.' >&2 ;;
            esac
            exit 2
            """)
        let runs = RunCoordinator(cli: stub.cli)
        let clash = "Feed #2 already uses https://e.org/feed; merge into it instead."
        #expect(await runs.perform(.decide(feedId: 3, .repair)) == .failed(RunFailure(exitCode: 2, lastStderrLine: clash)))
        #expect(runs.banner == Banner(message: clash, showLog: false))
        let undo = "Feed #10 already uses https://e.org/feed; cannot undo."
        #expect(await runs.perform(.undo()) == .failed(RunFailure(exitCode: 2, lastStderrLine: undo)))
        #expect(runs.banner == Banner(message: undo, showLog: false))
        #expect(await runs.perform(.fetch(feeds: [9])) == .failed(RunFailure(exitCode: 2, lastStderrLine: "No feed #9.")))
        #expect(runs.banner == Banner(message: "No feed #9.", showLog: true))
    }

    @Test func only_garden_decisions_and_undo_can_be_refused() {
        let exitTwo = RunFailure(exitCode: 2, lastStderrLine: "Feed #10 already uses https://e.org/feed; cannot restore.")
        #expect(RunRequest.decide(feedId: 9, .keep).isRefusal(exitTwo))
        #expect(RunRequest.undo().isRefusal(exitTwo))
        #expect(!RunRequest.applySafe().isRefusal(exitTwo))
        #expect(!RunRequest.fetch(feeds: [9]).isRefusal(exitTwo))
        #expect(!RunRequest.migrate().isRefusal(exitTwo))
        #expect(!RunRequest.decide(feedId: 9, .keep).isRefusal(RunFailure(exitCode: 1, lastStderrLine: "error: disk I/O error")))
    }

```

- [ ] **Step 2: Run them to see them fail**

```bash
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/RunCoordinatorTests 2>&1 | grep -E 'error:|Test run with|\*\* TEST'
```

Expected: the test target does not compile: `value of type 'RunRequest' has no member 'isRefusal'` (six times), then `** TEST FAILED **`.

- [ ] **Step 3: Classify and show the refusal**

In `apps/macos/Ressac/Engine/RessacCLI.swift`, replace:

```swift
    /// Extraction failures are shown inline in the reader, never as a banner (spec §8).
    var showsFailureBanner: Bool { arguments.first != "extract" }
}
```

with:

```swift
    /// Extraction failures are shown inline in the reader, never as a banner (spec §8).
    var showsFailureBanner: Bool { arguments.first != "extract" }

    /// True when `failure` is the store refusing a garden decision or an undo (exit 2, e.g. an
    /// address another active feed already uses), which the app cannot foresee. From any other
    /// call, exit 2 means the app built a wrong command line (spec §8).
    func isRefusal(_ failure: RunFailure) -> Bool {
        failure.exitCode == 2
            && (arguments.starts(with: ["garden", "decide"]) || arguments == ["garden", "undo"])
    }
}
```

In `apps/macos/Ressac/App/RunCoordinator.swift`, replace:

```swift
        case let .failed(failure):
            if failure.exitCode == 2 {
                Self.log.fault("usage error from ressac \(request.arguments.joined(separator: " "), privacy: .public)")
            }
            if request.showsFailureBanner {
                banner = Banner(message: failure.message, showLog: true)
            }
```

with:

```swift
        case let .failed(failure):
            let refused = request.isRefusal(failure)
            if refused {
                Self.log.error("ressac refused \(request.arguments.joined(separator: " "), privacy: .public): \(failure.message, privacy: .public)")
            } else if failure.exitCode == 2 {
                Self.log.fault("usage error from ressac \(request.arguments.joined(separator: " "), privacy: .public)")
            }
            if request.showsFailureBanner {
                // A refusal's sentence says it all: the log holds nothing more.
                banner = Banner(message: failure.message, showLog: !refused)
            }
```

In `docs/specs/2026-10-10-mac-app-design.md`, replace line 276:

```markdown
- **Subprocess exit `2`:** this is a bug in the app's call. It is logged and shown in the same banner.
```

with:

```markdown
- **Subprocess exit `2`:** a bug in the app's call, logged as a fault and shown in the same banner. Except from `garden decide` and `garden undo`, which exit `2` when the store refuses the decision (an address another active feed already uses): the app cannot foresee that, so it logs it as an error and the banner shows the CLI's sentence without **Show log**.
```

- [ ] **Step 4: Run them to see them pass**

Run the Step 2 command again.

Expected: `✔ Test run with 10 tests in 1 suite passed`, `** TEST SUCCEEDED **`, no `warning:` line.

- [ ] **Step 5: Commit**

```bash
git add apps/macos/Ressac/Engine/RessacCLI.swift apps/macos/Ressac/App/RunCoordinator.swift apps/macos/RessacTests/RunCoordinatorTests.swift docs/specs/2026-10-10-mac-app-design.md
git commit -F - <<'EOF'
App: an address clash from the garden is a refusal, not a bug

garden decide and garden undo now exit 2 when another active feed already
uses the address. The app logged every exit 2 as a fault and offered Show
log. For those two calls it is the store's refusal: logged as an error, and
the banner shows the CLI's sentence alone. Other calls keep the fault log.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Phase 3c verification gate

- [ ] **Step 1: Run the Swift gate (it includes the Rust gate)**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-3c.log" | grep -E 'Test run with|\*\* TEST'
grep -cE '\.swift:[0-9]+:[0-9]+: (warning|error)' "$TMPDIR/ressac-3c.log"
cd ../..
```

Expected: the Rust counts of the Phase 3b gate; then `✔ Test run with 320 tests in 79 suites passed`, `** TEST SUCCEEDED **`; the `grep -c` prints `0`.

- [ ] **Step 2: Stop.** Report the gate output and wait for Clément's approval before Phase 4.

---

## Notes for the executor

- **Validated in a scratch copy** of `855cd2c` plus Phase 2's split (without Phase 1): every edit above was applied, each "expected failure" was observed as written, and the Rust gate passed after each sub-phase. Re-checked on 15/10/2026 with Phases 1 and 2 applied first, in order: every quoted text matched and the counts above are those of that run. Phase 3c was written and checked the same day on that tree.
- **`TODO.md`** (for the plan's final docs phase), « Reporté ▸ Cœur et CLI »: tick the lines "`keep` ou `repair` vers une adresse déjà utilisée…", "`garden apply-safe --json` ne donne que le résumé…", "Un changement d'auteur d'un article…" and "`recompute_triage` relit toutes les lignes…"; « Reporté ▸ Appli et outillage »: tick "Indice de spam affiché sur 5 flux…" (restricted to non-alive statuses, decision of 15/10/2026). The `v_entry` / `v_unread_count` line is Phase 4's.
- **Out of scope, on purpose:**
  - **A typed refusal error in the core** (see Decisions, item 1).
  - **The TUI's `apply-safe`** (key `R`) already shows the applied and skipped counts through `output::apply_safe_text`.
  - **Re-diagnosing the stored hints at migration.** Alive feeds keep their old hint bullets until the next refresh, which re-diagnoses every active feed.
