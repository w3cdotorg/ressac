# Phase 5: CLI subcommands and contract v2

Read the plan index first. Global Constraints apply. Spec §3 (core table), §5 ("Contract test"), §6 (subset fetch, `ressac extract`, garden subcommands, exit codes) and §9 (CLI tests, contract at v2).

**Files (5):**
- Modify: `crates/ressac/src/output.rs` (tests inline)
- Modify: `crates/ressac/src/tui/mod.rs` (uses the shared garden messages)
- Modify: `crates/ressac/src/main.rs` (tests inline)
- Modify: `crates/ressac/tests/cli.rs`
- Modify: `crates/ressac-core/tests/contract.rs`
- Regenerated binary asset (not counted): `tests/fixtures/contract.sqlite`

**Builds on (fixed by earlier phases):**
- Phase 1, `extract::{Article, ExtractError}`.
- Phase 2:
  - `SCHEMA_VERSION = 2`;
  - `Store::{save_fulltext, set_prefer_full_text}`;
  - the backlog rule and FTS upkeep inside `Store::record_attempt`.
- Phase 3:
  - `RefreshOptions { force, only: Option<Vec<FeedId>> }`; `refresh` skips buried feeds even when they are named in `only`;
  - `Store::feeds_in_folder(folder_id) -> Result<Vec<FeedId>>`, which is recursive and returns active feeds only;
  - `garden::recompute_triage(&Store)`. `garden::{decide, undo, apply_safe_repairs}` call it before returning.
- Phase 4:
  - `Ressac::extract_entry(entry_id, &dyn EventSink) -> Result<ExtractOutcome>`, where `ExtractOutcome` is `Saved`, `Failed(String)`, `NoUrl` or `UnknownEntry`;
  - the events `Event::{Extracted, Decided, Undone}`;
  - `FULL_ARTICLES`.

**CLI contract produced by this phase** (`ressac [--db P] [--json] …`; stdout carries NDJSON events tagged by `"event"` under `--json`, stderr carries free text):

| Command | `--json` stdout | Text stdout | Exit `2` (message on stderr, nothing on stdout) |
|---|---|---|---|
| `fetch [--force] [--feed ID]… [--folder ID]` | unchanged event stream | unchanged summary | `No feed #ID.`, `No folder #ID.`; `--feed` with `--folder` (clap) |
| `extract ENTRY_ID` | one `extracted` line; a failed extraction still exits `0` | `Saved the full article of entry #N.` or `Full article unavailable: <sentence>` | `No entry #N.`, `This entry has no link.` |
| `garden` | none (opens the TUI, unchanged) | none (opens the TUI, unchanged) | none |
| `garden decide FEED_ID keep\|repair\|bury` | one `decided` line | `Kept feed #N.` / `Repaired feed #N.` / `Buried feed #N.` | `No feed #N.`, `Feed #N is buried; only keep restores it.`, `Nothing to repair for feed #N.` |
| `garden undo` | one `undone` line (`feed_id` is `null` when nothing is left) | `Undid the last decision (#N).` / `Nothing to undo.` | none |
| `garden apply-safe` | one `finished` line with the summary | `Applied N safe repairs.` (+ ` Skipped K: …`) | none |
| `migrate` | one `finished` line with the summary | `Schema version 2.` | none |

On a database with no feeds, `extract` and the `garden` subcommands print the existing first-run hint and exit `0`, as `garden` does today. `migrate` works on a missing or empty database.

**Deviation from the master table:** `garden decide` on a buried feed exits `2` for `repair` and `bury`. It accepts `keep`, which restores the feed, because the app's Cemetery `k` (spec §7) is `ressac garden decide <id> keep`, as in the TUI.

Start by checking that the tree is green: `cargo test --workspace`.

### Task 5.1: Result texts and the JSON result line

**Interfaces:**
- Consumes: `model::{Decision, FeedId}` and `service::Event`.
- Produces (in `crates/ressac/src/output.rs`, binary crate):
  - `pub fn write_result(out: &mut impl Write, json: bool, event: &Event, text: &str) -> std::io::Result<()>`;
  - `pub fn decided_text(feed_id: FeedId, decision: Decision) -> String`;
  - `pub fn undo_text(undone: Option<FeedId>) -> String`;
  - `pub fn apply_safe_text(applied: usize, skipped: &[String]) -> String`;
  - `pub fn extracted_text(entry_id: i64, error: Option<&str>) -> String`;
  - `pub fn migrated_text(version: i64) -> String`;
  - a private `fn event_json(event: &Event) -> String`, shared by `write_result` and the JSON sink.
- The TUI (`tui/mod.rs`) uses `undo_text` and `apply_safe_text`, so the terminal garden and `ressac garden …` say the same thing.

- [ ] **Step 1: Write the failing tests.** In `output.rs`, inside `mod tests`, append:

```rust
    #[test]
    fn garden_texts_are_shared_with_the_tui() {
        assert_eq!(decided_text(3, Decision::Keep), "Kept feed #3.");
        assert_eq!(decided_text(3, Decision::Repair), "Repaired feed #3.");
        assert_eq!(decided_text(3, Decision::Bury), "Buried feed #3.");
        assert_eq!(undo_text(Some(4)), "Undid the last decision (#4).");
        assert_eq!(undo_text(None), "Nothing to undo.");
        assert_eq!(apply_safe_text(2, &[]), "Applied 2 safe repairs.");
        assert_eq!(
            apply_safe_text(1, &["#7: Feed #2 already uses x; merge into it instead.".into()]),
            "Applied 1 safe repairs. Skipped 1: #7: Feed #2 already uses x; merge into it instead."
        );
    }

    #[test]
    fn extraction_and_migration_texts() {
        assert_eq!(extracted_text(5, None), "Saved the full article of entry #5.");
        assert_eq!(
            extracted_text(5, Some("HTTP 404.")),
            "Full article unavailable: HTTP 404."
        );
        assert_eq!(migrated_text(2), "Schema version 2.");
    }

    #[test]
    fn write_result_prints_the_event_or_the_text() {
        let event = Event::Undone { feed_id: Some(4) };
        let mut json = Vec::new();
        write_result(&mut json, true, &event, "ignored").unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"event\":\"undone\",\"feed_id\":4}\n"
        );
        let mut text = Vec::new();
        write_result(&mut text, false, &event, "Undid the last decision (#4).").unwrap();
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "Undid the last decision (#4).\n"
        );
    }
```

- [ ] **Step 2: Run red.** `cargo test -p ressac --bin ressac output::tests`. Expected: compile errors, ``cannot find function `decided_text` `` and the like.

- [ ] **Step 3: Implement in `output.rs`.**

Replace the import line `use ressac_core::model::Status;` with:

```rust
use ressac_core::model::{Decision, FeedId, Status};
```

In `impl EventSink for Sink`, replace the `Display::Json` arm:

```rust
            Display::Json => {
                // A closed stdout (BrokenPipe) must not panic; the final write reports it.
                let _ = writeln!(
                    std::io::stdout().lock(),
                    "{}",
                    serde_json::to_string(event).unwrap_or_default()
                );
            }
```

with:

```rust
            Display::Json => {
                // A closed stdout (BrokenPipe) must not panic; the final write reports it.
                let _ = writeln!(std::io::stdout().lock(), "{}", event_json(event));
            }
```

Below `fn probing_message` (Phase 4), add:

```rust
fn event_json(event: &Event) -> String {
    serde_json::to_string(event).unwrap_or_default()
}

/// Writes a one-shot command's result: the event as one JSON line with `--json`, else `text`.
pub fn write_result(
    out: &mut impl Write,
    json: bool,
    event: &Event,
    text: &str,
) -> std::io::Result<()> {
    if json {
        writeln!(out, "{}", event_json(event))
    } else {
        writeln!(out, "{text}")
    }
}

pub fn decided_text(feed_id: FeedId, decision: Decision) -> String {
    let verb = match decision {
        Decision::Keep => "Kept",
        Decision::Repair => "Repaired",
        Decision::Bury => "Buried",
    };
    format!("{verb} feed #{feed_id}.")
}

/// Also shown by the full-screen garden.
pub fn undo_text(undone: Option<FeedId>) -> String {
    undone.map_or("Nothing to undo.".into(), |id| {
        format!("Undid the last decision (#{id}).")
    })
}

/// Also shown by the full-screen garden.
pub fn apply_safe_text(applied: usize, skipped: &[String]) -> String {
    let mut msg = format!("Applied {applied} safe repairs.");
    if !skipped.is_empty() {
        msg.push_str(&format!(
            " Skipped {}: {}",
            skipped.len(),
            skipped.join("; ")
        ));
    }
    msg
}

/// `error` is the stored failure sentence; the wording matches the app's inline notice.
pub fn extracted_text(entry_id: i64, error: Option<&str>) -> String {
    match error {
        None => format!("Saved the full article of entry #{entry_id}."),
        Some(e) => format!("Full article unavailable: {e}"),
    }
}

pub fn migrated_text(version: i64) -> String {
    format!("Schema version {version}.")
}
```

- [ ] **Step 4: Use the shared texts in the TUI.** In `crates/ressac/src/tui/mod.rs`, inside `event_loop`, replace the `Command::Undo` and `Command::ApplySafeRepairs` arms (the inline `"Nothing to undo."`, `"Undid the last decision (#{id})."` and `"Applied {applied} safe repairs."` builders) with:

```rust
            Command::Undo => garden::undo(store)
                .map(|undone| Some(crate::output::undo_text(undone)))
                .map_err(Into::into),
            Command::ApplySafeRepairs => garden::apply_safe_repairs(store, Utc::now())
                .map(|(applied, skipped)| Some(crate::output::apply_safe_text(applied, &skipped)))
                .map_err(Into::into),
```

If Phase 3 changed these arms (for example to re-read rows), keep its additions and replace only the message construction.

- [ ] **Step 5: Run green.** `cargo test -p ressac --bin ressac`. Expected: all pass, including the TUI snapshot tests, which are unchanged because the strings are identical.

- [ ] **Step 6: Commit.**

```bash
git add crates/ressac/src/output.rs crates/ressac/src/tui/mod.rs
git commit -m "Result texts and JSON result lines for one-shot commands, shared with the TUI"
```

### Task 5.2: `fetch --feed/--folder`, `extract`, `garden decide/undo/apply-safe`, `migrate`

**Interfaces:**
- Consumes:
  - `Ressac::{open, refresh, extract_entry, summary, store}`;
  - `Store::{feeds, folders, feeds_in_folder, diagnosis, schema_version, feed_count}`;
  - `garden::{decide, undo, apply_safe_repairs}`;
  - `service::{Event, ExtractOutcome, NullSink, RefreshOptions}`;
  - the output helpers of Task 5.1.
- Produces (binary `ressac`, `main.rs`):
  - clap: `Command::Fetch { force, feeds: Vec<FeedId> /* --feed, repeatable */, folder: Option<FolderId> /* conflicts with --feed */ }`, `Command::Extract { entry_id: i64 }`, `Command::Garden { action: Option<GardenCommand> }`, `Command::Migrate`; `enum GardenCommand { Decide { feed_id, decision: DecisionArg }, Undo, ApplySafe }`; `#[derive(ValueEnum)] enum DecisionArg { Keep, Repair, Bury }` with `From<DecisionArg> for Decision`;
  - `struct UsageError(String)`, which `main` maps to exit `2` with the bare message on stderr, and `fn usage(message: impl Into<String>) -> anyhow::Error`;
  - `fn subset(store: &Store, feeds: Vec<FeedId>, folder: Option<FolderId>) -> anyhow::Result<Option<Vec<FeedId>>>`;
  - `fn check_decision(store: &Store, feed_id: FeedId, decision: Decision) -> anyhow::Result<()>`;
  - `fn garden_command(app: &Ressac, action: GardenCommand, json: bool, out: &mut impl Write) -> anyhow::Result<()>`.

Review Focus 1 (CLI side): `extract` on an entry without a link exits `2` with "This entry has no link." (test `extract_stores_articles_and_refuses_unusable_entries`).

- [ ] **Step 1: Write the failing CLI tests** in `crates/ressac/tests/cli.rs`.

Replace the imports at the top:

```rust
use std::path::Path;
use std::process::{Command, Output};
```

with:

```rust
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
```

and delete the two now-redundant lines inside `fetch_json_streams_events`:

```rust
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

Below `fn stdout`, add the helpers:

```rust
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn json_lines(o: &Output) -> Vec<Value> {
    stdout(o)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Runs the binary off the async runtime, so a wiremock server on it keeps answering.
async fn ressac_async(home: &Path, args: &[&str]) -> Output {
    let home = home.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        ressac(&home, &args)
    })
    .await
    .unwrap()
}

const PARA: &str = "The tide comes in twice a day, pulled by the moon and the sun, and the shore keeps a careful record of every visit in sand, shells and drifting weed.";

fn article_page() -> String {
    let body: String = (0..8)
        .map(|i| format!("<p>Paragraph {i}. {PARA} {PARA}</p>"))
        .collect();
    format!(
        "<!DOCTYPE html><html><head><title>Tides</title></head><body><article><h1>Tides</h1>{body}</article></body></html>"
    )
}

fn channel(title: &str, items: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://e.org/</link><description>d</description>{items}</channel></rss>"#
    )
}

/// Serves four feeds and two pages, and imports them as News { A, B, Sub { C } } then D at
/// the top level: feed ids 1–4 in that order, folder ids News = 1 and Sub = 2 (ids follow
/// the OPML's depth-first order on a fresh database). Feed A holds, in order: an entry
/// linking to /article, one linking to /missing (404), and one with no link and no title.
async fn reading_home(server: &MockServer) -> tempfile::TempDir {
    let base = server.uri();
    let a_items = format!(
        "<item><guid>linked</guid><title>Tides</title><link>{base}/article</link></item>\
         <item><guid>broken</guid><title>Gone</title><link>{base}/missing</link></item>\
         <item><guid>bare</guid><description>Only a summary.</description></item>"
    );
    for (at, body) in [
        ("/a.xml", channel("A", &a_items)),
        ("/b.xml", channel("B", "")),
        ("/c.xml", channel("C", "")),
        ("/d.xml", channel("D", "")),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml"))
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(article_page(), "text/html"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("reading.opml");
    std::fs::write(
        &opml,
        format!(
            r#"<opml><body><outline text="News"><outline text="A" xmlUrl="{base}/a.xml"/><outline text="B" xmlUrl="{base}/b.xml"/><outline text="Sub"><outline text="C" xmlUrl="{base}/c.xml"/></outline></outline><outline text="D" xmlUrl="{base}/d.xml"/></body></opml>"#
        ),
    )
    .unwrap();
    let import = ressac_async(home.path(), &["import", opml.to_str().unwrap()]).await;
    assert!(import.status.success(), "{import:?}");
    home
}
```

In `commands_on_an_empty_store_hint_at_import`, extend the `for args in [ … ]` list with:

```rust
        &["fetch", "--feed", "1"],
        &["extract", "1"],
        &["garden", "decide", "1", "keep"],
        &["garden", "undo"],
        &["garden", "apply-safe"],
```

Replace `usage_errors_exit_2` with:

```rust
#[test]
fn usage_errors_exit_2() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        &["frobnicate"][..],
        &["garden", "frobnicate"],
        &["garden", "decide", "1", "frobnicate"],
        &["extract", "x"],
        &["fetch", "--feed", "1", "--folder", "1"],
    ] {
        assert_eq!(ressac(home.path(), args).status.code(), Some(2), "{args:?}");
    }
}
```

Append the new tests:

```rust
#[tokio::test]
async fn fetch_subsets_imply_force_and_include_subfolders() {
    let server = MockServer::start().await;
    let home = reading_home(&server).await;
    let started = |o: &Output| json_lines(o)[0].clone();

    let two = ressac_async(home.path(), &["--json", "fetch", "--feed", "1", "--feed", "4"]).await;
    assert!(two.status.success(), "{two:?}");
    assert_eq!(started(&two), json!({"event": "started", "total": 2}));
    let again = ressac_async(home.path(), &["--json", "fetch", "--feed", "1"]).await;
    assert_eq!(
        started(&again),
        json!({"event": "started", "total": 1}),
        "fetched seconds ago, fetched again: --feed implies --force"
    );
    let news = ressac_async(home.path(), &["--json", "fetch", "--folder", "1"]).await;
    assert_eq!(
        started(&news),
        json!({"event": "started", "total": 3}),
        "News holds A, B and Sub/C"
    );
    let sub = ressac_async(home.path(), &["--json", "fetch", "--folder", "2"]).await;
    assert_eq!(started(&sub), json!({"event": "started", "total": 1}));
    assert_eq!(json_lines(&sub).last().unwrap()["event"], "finished");
}

#[test]
fn fetch_subsets_reject_unknown_ids_and_skip_buried_feeds() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let feed = ressac(home.path(), &["fetch", "--feed", "99"]);
    assert_eq!(feed.status.code(), Some(2));
    assert_eq!(stderr(&feed), "No feed #99.\n");
    let folder = ressac(home.path(), &["fetch", "--folder", "99"]);
    assert_eq!(folder.status.code(), Some(2));
    assert_eq!(stderr(&folder), "No folder #99.\n");

    // Nothing touches the network: the only named feed is buried.
    assert!(ressac(home.path(), &["garden", "decide", "1", "bury"]).status.success());
    let buried = ressac(home.path(), &["--json", "fetch", "--feed", "1"]);
    assert!(buried.status.success(), "{buried:?}");
    let lines = json_lines(&buried);
    assert_eq!(
        lines[0],
        json!({"event": "started", "total": 0}),
        "a buried feed is skipped even when named"
    );
    assert_eq!(lines.last().unwrap()["event"], "finished");
}

#[tokio::test]
async fn extract_stores_articles_and_refuses_unusable_entries() {
    let server = MockServer::start().await;
    let home = reading_home(&server).await;
    let fetched = ressac_async(home.path(), &["fetch", "--feed", "1"]).await;
    assert!(fetched.status.success(), "{fetched:?}");

    let saved = ressac_async(home.path(), &["--json", "extract", "1"]).await;
    assert!(saved.status.success(), "{saved:?}");
    assert_eq!(
        json_lines(&saved),
        [json!({"event": "extracted", "entry_id": 1, "ok": true, "error": null})]
    );
    let failed = ressac_async(home.path(), &["--json", "extract", "2"]).await;
    assert_eq!(
        failed.status.code(),
        Some(0),
        "a failed extraction is data, not a fatal error"
    );
    assert_eq!(
        json_lines(&failed),
        [json!({"event": "extracted", "entry_id": 2, "ok": false, "error": "HTTP 404."})]
    );
    assert_eq!(
        stdout(&ressac_async(home.path(), &["extract", "1"]).await),
        "Saved the full article of entry #1.\n"
    );
    assert_eq!(
        stdout(&ressac_async(home.path(), &["extract", "2"]).await),
        "Full article unavailable: HTTP 404.\n"
    );

    // Review Focus 1: no link, no title, only a summary.
    let no_link = ressac_async(home.path(), &["--json", "extract", "3"]).await;
    assert_eq!(no_link.status.code(), Some(2));
    assert_eq!(stderr(&no_link), "This entry has no link.\n");
    assert!(no_link.stdout.is_empty());
    let unknown = ressac_async(home.path(), &["extract", "999"]).await;
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(stderr(&unknown), "No entry #999.\n");
}

#[test]
fn garden_subcommands_decide_undo_and_apply_safe() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);

    // Feed #2 is "Orange Drenka" (https://orange.example.org/feed/).
    let buried = run(&["--json", "garden", "decide", "2", "bury"]);
    assert!(buried.status.success(), "{buried:?}");
    assert_eq!(
        json_lines(&buried),
        [json!({"event": "decided", "feed_id": 2, "decision": "bury"})]
    );
    assert!(!stdout(&run(&["export"])).contains("orange.example.org/feed/"));
    for decision in ["bury", "repair"] {
        let refused = run(&["garden", "decide", "2", decision]);
        assert_eq!(refused.status.code(), Some(2), "{decision}");
        assert_eq!(stderr(&refused), "Feed #2 is buried; only keep restores it.\n");
        assert!(refused.stdout.is_empty());
    }
    let restored = run(&["--json", "garden", "decide", "2", "keep"]);
    assert_eq!(
        json_lines(&restored),
        [json!({"event": "decided", "feed_id": 2, "decision": "keep"})]
    );
    assert!(stdout(&run(&["export"])).contains("orange.example.org/feed/"));

    let nothing = run(&["garden", "decide", "1", "repair"]);
    assert_eq!(nothing.status.code(), Some(2));
    assert_eq!(stderr(&nothing), "Nothing to repair for feed #1.\n");
    let unknown = run(&["garden", "decide", "99", "keep"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(stderr(&unknown), "No feed #99.\n");

    // One shared stack: the keep, then the bury, then nothing left.
    for expected in [json!(2), json!(2), Value::Null] {
        let undo = run(&["--json", "garden", "undo"]);
        assert!(undo.status.success(), "{undo:?}");
        assert_eq!(
            json_lines(&undo),
            [json!({"event": "undone", "feed_id": expected})]
        );
    }
    assert_eq!(stdout(&run(&["garden", "undo"])), "Nothing to undo.\n");
    assert_eq!(
        stdout(&run(&["garden", "decide", "5", "keep"])),
        "Kept feed #5.\n"
    );

    let applied = run(&["--json", "garden", "apply-safe"]);
    assert!(applied.status.success(), "{applied:?}");
    let lines = json_lines(&applied);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 11);
    assert_eq!(
        stdout(&run(&["garden", "apply-safe"])),
        "Applied 0 safe repairs.\n"
    );
}

#[test]
fn migrate_reports_the_schema_version() {
    let home = tempfile::tempdir().unwrap();
    let text = ressac(home.path(), &["migrate"]);
    assert!(text.status.success(), "{text:?}");
    assert_eq!(stdout(&text), "Schema version 2.\n");
    assert!(home.path().join("db.sqlite").exists());
    let json = ressac(home.path(), &["--json", "migrate"]);
    assert!(json.status.success(), "{json:?}");
    let lines = json_lines(&json);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 0);
}
```

- [ ] **Step 2: Run red.** `cargo test -p ressac --test cli`. Expected failures:
  - every new test fails, because clap rejects `--feed`, `extract`, `decide` and `migrate` with exit `2` (for example `assertion failed: two.status.success()`);
  - `commands_on_an_empty_store_hint_at_import` fails on `["fetch", "--feed", "1"]` with exit `2` against the expected `0`;
  - `usage_errors_exit_2` passes already, since every case is still an error.

- [ ] **Step 3: Implement.** Replace `crates/ressac/src/main.rs` with the file below. It keeps every existing behaviour, and it already contains Phase 3's `RefreshOptions { force, only: None }` for `check`. If Phase 3 added anything else to `main.rs`, carry it over.

```rust
//! `ressac`: tidy your RSS subscriptions, then read them calmly.

mod output;
mod tui;

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use ressac_core::garden;
use ressac_core::model::{Decision, FeedId, FolderId};
use ressac_core::opml;
use ressac_core::service::{
    Event, ExtractOutcome, NullSink, PRODUCTION_WAYBACK, RefreshOptions, Ressac,
};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;

#[derive(Parser)]
#[command(
    name = "ressac",
    version,
    about = "Tidy your RSS subscriptions, then read them calmly"
)]
struct Cli {
    /// Database file [default: ~/Library/Application Support/Ressac/ressac.sqlite]
    #[arg(long, env = "RESSAC_DB", global = true)]
    db: Option<PathBuf>,
    /// Print one JSON object per line instead of human-readable output
    #[arg(long, global = true)]
    json: bool,
    /// Use ASCII status glyphs instead of emoji
    #[arg(long, global = true)]
    ascii: bool,
    /// Also write logs to stderr
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import feeds and folders from an OPML file
    Import {
        file: PathBuf,
        /// Show what would be imported without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Fetch every feed (or only some) and update its health
    Fetch {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
        /// Only fetch this feed (repeatable); implies --force
        #[arg(long = "feed", value_name = "ID")]
        feeds: Vec<FeedId>,
        /// Only fetch the feeds of this folder and its subfolders; implies --force
        #[arg(long, value_name = "ID", conflicts_with = "feeds")]
        folder: Option<FolderId>,
    },
    /// Fetch, look up archives of dead feeds, and print a health report
    Check {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
    },
    /// Fetch an entry's web page and store its full article
    Extract {
        /// The entry's id
        entry_id: i64,
    },
    /// Triage feeds in a full-screen garden, or decide without it
    Garden {
        #[command(subcommand)]
        action: Option<GardenCommand>,
    },
    /// Export active feeds (or the cemetery) as OPML
    Export {
        /// Write to this file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Export buried feeds, with their Wayback links
        #[arg(long)]
        cemetery: bool,
    },
    /// Open the database and apply pending schema migrations, nothing else
    Migrate,
}

#[derive(Subcommand)]
enum GardenCommand {
    /// Keep, repair or bury one feed (keep restores a buried feed)
    Decide {
        feed_id: FeedId,
        #[arg(value_enum)]
        decision: DecisionArg,
    },
    /// Undo the last decision (the stack is shared with the full-screen garden)
    Undo,
    /// Apply every safe repair (https upgrades and permanent redirects)
    ApplySafe,
}

#[derive(Clone, Copy, ValueEnum)]
enum DecisionArg {
    Keep,
    Repair,
    Bury,
}

impl From<DecisionArg> for Decision {
    fn from(d: DecisionArg) -> Self {
        match d {
            DecisionArg::Keep => Decision::Keep,
            DecisionArg::Repair => Decision::Repair,
            DecisionArg::Bury => Decision::Bury,
        }
    }
}

/// Exit code of a run stopped by Ctrl-C (128 + SIGINT), as shells expect.
const INTERRUPTED: u8 = 130;

/// Exit code of a usage error: clap's own, and requests on ids the database cannot satisfy.
const USAGE: u8 = 2;

/// Printed instead of running a command that needs feeds on an empty database.
const NO_FEEDS: &str = "No feeds yet: run \"ressac import <file.opml>\" first.";

/// `ressac extract` on an entry without a link (spec Review Focus 1).
const NO_LINK: &str = "This entry has no link.";

/// A request the database cannot satisfy (unknown id, nothing to repair). It exits 2 like
/// clap's usage errors, with the bare message on stderr.
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn usage(message: impl Into<String>) -> anyhow::Error {
    UsageError(message.into()).into()
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let _log_guard = init_logging(cli.verbose);
    match run(cli).await {
        Ok(code) => code,
        Err(e) if is_broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => match e.downcast_ref::<UsageError>() {
            Some(usage) => {
                eprintln!("{usage}");
                ExitCode::from(USAGE)
            }
            None => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
    }
}

/// True when an io `BrokenPipe` (closed stdout, e.g. `| head -1`) sits anywhere in the chain.
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

fn open(db: &Path) -> anyhow::Result<Ressac> {
    Ressac::open(db).with_context(|| format!("cannot open database {}", db.display()))
}

/// Opens the database for a command that needs feeds. With none (or no database yet),
/// prints the first-run hint instead, without creating anything.
fn open_with_feeds(db: &Path) -> anyhow::Result<Option<Ressac>> {
    if db.exists() {
        let app = open(db)?;
        if app.store().feed_count()? > 0 {
            return Ok(Some(app));
        }
    }
    eprintln!("{NO_FEEDS}");
    Ok(None)
}

/// After a fetch stopped by Ctrl-C: say how much was left undone, exit 130.
fn interrupted(stop: &AtomicBool, sink: &output::Sink) -> ExitCode {
    if stop.load(Ordering::Relaxed) {
        eprintln!("{}", output::interrupted_message(sink.not_fetched()));
        ExitCode::from(INTERRUPTED)
    } else {
        ExitCode::SUCCESS
    }
}

/// The feeds named by `--feed` or `--folder` (`None`: no restriction). Unknown ids are
/// usage errors. Buried feeds stay in the list, and `refresh` skips them.
fn subset(
    store: &Store,
    feeds: Vec<FeedId>,
    folder: Option<FolderId>,
) -> anyhow::Result<Option<Vec<FeedId>>> {
    if let Some(folder) = folder {
        if !store.folders()?.iter().any(|f| f.id == folder) {
            return Err(usage(format!("No folder #{folder}.")));
        }
        return Ok(Some(store.feeds_in_folder(folder)?));
    }
    if feeds.is_empty() {
        return Ok(None);
    }
    let known: HashSet<FeedId> = store.feeds()?.into_iter().map(|f| f.id).collect();
    if let Some(missing) = feeds.iter().find(|id| !known.contains(id)) {
        return Err(usage(format!("No feed #{missing}.")));
    }
    Ok(Some(feeds))
}

/// Refuses, as usage errors, the decisions the garden cannot apply: an unknown feed, a
/// buried feed (only keep, which restores it, applies), or a repair with no proposed repair.
fn check_decision(store: &Store, feed_id: FeedId, decision: Decision) -> anyhow::Result<()> {
    let Some(feed) = store.feeds()?.into_iter().find(|f| f.id == feed_id) else {
        return Err(usage(format!("No feed #{feed_id}.")));
    };
    if feed.state == "buried" && decision != Decision::Keep {
        return Err(usage(format!(
            "Feed #{feed_id} is buried; only keep restores it."
        )));
    }
    let repairable = store
        .diagnosis(feed_id)?
        .and_then(|d| d.action)
        .is_some_and(|a| a.is_repair());
    if decision == Decision::Repair && !repairable {
        return Err(usage(format!("Nothing to repair for feed #{feed_id}.")));
    }
    Ok(())
}

/// `ressac garden decide | undo | apply-safe`: the TUI's keys, one result line each.
fn garden_command(
    app: &Ressac,
    action: GardenCommand,
    json: bool,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let store = app.store();
    match action {
        GardenCommand::Decide { feed_id, decision } => {
            let decision = Decision::from(decision);
            check_decision(store, feed_id, decision)?;
            garden::decide(store, feed_id, decision, Utc::now())?;
            let event = Event::Decided {
                feed_id,
                decision: decision.as_str().to_string(),
            };
            output::write_result(out, json, &event, &output::decided_text(feed_id, decision))?;
        }
        GardenCommand::Undo => {
            let feed_id = garden::undo(store)?;
            output::write_result(
                out,
                json,
                &Event::Undone { feed_id },
                &output::undo_text(feed_id),
            )?;
        }
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
    }
    Ok(())
}

async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let db = cli
        .db
        .clone()
        .unwrap_or_else(ressac_core::store::default_path);
    let stop = Arc::new(AtomicBool::new(false));
    {
        // First Ctrl-C: stop scheduling and finish cleanly. Second one: leave now.
        let stop = stop.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                stop.store(true, Ordering::Relaxed);
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(INTERRUPTED.into());
                }
            }
        });
    }
    let mut out = std::io::stdout().lock();
    match cli.command {
        Command::Import { file, dry_run } => {
            // Read and parse before touching the database, so a bad input never creates one.
            let xml = std::fs::read_to_string(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            opml::parse(&xml)?;
            let app = if dry_run && !db.exists() {
                // Nothing to compare against and nothing may be written: use a throwaway database.
                Ressac::with_parts(
                    Store::open_in_memory()?,
                    FetchLimits::default(),
                    PRODUCTION_WAYBACK,
                )?
            } else {
                open(&db)?
            };
            let report = app.import_opml(&xml, dry_run)?;
            if cli.json {
                writeln!(
                    out,
                    "{}",
                    serde_json::json!({
                        "dry_run": dry_run,
                        "folders_added": report.folders_added,
                        "feeds_added": report.feeds_added,
                        "merged": report.merged,
                        "skipped_existing": report.skipped_existing,
                    })
                )?;
            } else {
                writeln!(out, "{}", output::import_text(&report, dry_run))?;
            }
        }
        Command::Fetch {
            force,
            feeds,
            folder,
        } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            let only = subset(app.store(), feeds, folder)?;
            // A manual refresh of some feeds ignores the 30-minute freshness rule (spec §2).
            let opts = RefreshOptions {
                force: force || only.is_some(),
                only,
            };
            let sink = output::sink(cli.json);
            let result = app.refresh(opts, &sink, stop.clone()).await;
            sink.finish();
            let summary = result?;
            if !cli.json {
                writeln!(out, "{}", output::summary_text(&summary, cli.ascii))?;
            }
            return Ok(interrupted(&stop, &sink));
        }
        Command::Check { force } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            let sink = output::sink(cli.json);
            let refreshed = app
                .refresh(RefreshOptions { force, only: None }, &sink, stop.clone())
                .await;
            let refreshed = match refreshed {
                Ok(s) => s,
                Err(e) => {
                    sink.finish();
                    return Err(e.into());
                }
            };
            let looked_up = app.lookup_wayback(&sink, stop.clone()).await;
            sink.finish();
            looked_up?;
            let mut summary = app.summary()?;
            summary.network_unreachable |= refreshed.network_unreachable;
            if cli.json {
                writeln!(
                    out,
                    "{}",
                    serde_json::json!({ "event": "report", "summary": summary })
                )?;
            } else {
                writeln!(out, "{}", output::summary_text(&summary, cli.ascii))?;
            }
            return Ok(interrupted(&stop, &sink));
        }
        Command::Extract { entry_id } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            // With --json the sink prints the `extracted` line; otherwise the text below says it.
            let outcome = if cli.json {
                app.extract_entry(entry_id, &output::sink(true)).await?
            } else {
                app.extract_entry(entry_id, &NullSink).await?
            };
            let text = match outcome {
                ExtractOutcome::UnknownEntry => {
                    return Err(usage(format!("No entry #{entry_id}.")));
                }
                ExtractOutcome::NoUrl => return Err(usage(NO_LINK)),
                ExtractOutcome::Saved => output::extracted_text(entry_id, None),
                ExtractOutcome::Failed(error) => output::extracted_text(entry_id, Some(&error)),
            };
            if !cli.json {
                writeln!(out, "{text}")?;
            }
        }
        Command::Garden { action } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            match action {
                None => tui::run(&app, cli.ascii)?,
                Some(action) => garden_command(&app, action, cli.json, &mut out)?,
            }
        }
        Command::Export { output, cemetery } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            let xml = opml::export(app.store(), cemetery)?;
            match output {
                Some(path) => std::fs::write(&path, xml)
                    .with_context(|| format!("cannot write {}", path.display()))?,
                None => write!(out, "{xml}")?,
            }
        }
        Command::Migrate => {
            // Opening runs the pending migrations (with a backup of the old file).
            let app = open(&db)?;
            // A v1 → v2 migration leaves every `needs_triage` at 0 until the next
            // diagnosis or decision: compute the flags now, so the app's Garden is right.
            ressac_core::garden::recompute_triage(app.store())?;
            let event = Event::Finished {
                summary: app.summary()?,
            };
            let text = output::migrated_text(app.store().schema_version()?);
            output::write_result(&mut out, cli.json, &event, &text)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Daily-rotated log file in ~/Library/Logs/Ressac, mirrored to stderr with `-v`.
fn init_logging(verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    let dir = PathBuf::from(std::env::var_os("HOME")?).join("Library/Logs/Ressac");
    std::fs::create_dir_all(&dir).ok()?;
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(dir, "ressac.log"));
    let file_layer = fmt::layer().with_writer(writer).with_ansi(false);
    let stderr_layer = verbose.then(|| fmt::layer().with_writer(std::io::stderr));
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .ok()?;
    Some(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    /// `ressac garden` alone still opens the TUI. It cannot run under `cargo test`
    /// (crossterm falls back to /dev/tty), so the parse is checked instead.
    #[test]
    fn bare_garden_still_opens_the_tui() {
        let bare = Cli::try_parse_from(["ressac", "garden"]).unwrap();
        assert!(matches!(bare.command, Command::Garden { action: None }));
        let decide = Cli::try_parse_from(["ressac", "--json", "garden", "decide", "3", "bury"]).unwrap();
        assert!(decide.json);
        assert!(matches!(
            decide.command,
            Command::Garden {
                action: Some(GardenCommand::Decide {
                    feed_id: 3,
                    decision: DecisionArg::Bury
                })
            }
        ));
        let safe = Cli::try_parse_from(["ressac", "garden", "apply-safe"]).unwrap();
        assert!(matches!(
            safe.command,
            Command::Garden {
                action: Some(GardenCommand::ApplySafe)
            }
        ));
    }

    #[test]
    fn feed_is_repeatable_and_exclusive_with_folder() {
        let cli = Cli::try_parse_from(["ressac", "fetch", "--feed", "1", "--feed", "4"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Fetch { ref feeds, folder: None, force: false } if *feeds == vec![1, 4]
        ));
        let conflict = Cli::try_parse_from(["ressac", "fetch", "--feed", "1", "--folder", "2"])
            .err()
            .unwrap();
        assert_eq!(conflict.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
}
```

- [ ] **Step 4: Run green.**
  - `cargo test -p ressac --bin ressac`: the three new parse tests pass, along with the existing ones.
  - `cargo test -p ressac --test cli`: everything passes. `fetch_subsets_imply_force_and_include_subfolders` takes a few seconds, because the binary keeps its 1 s per-host gap between feeds on the mock host.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/main.rs crates/ressac/tests/cli.rs
git commit -m "CLI: fetch --feed/--folder, extract, garden decide/undo/apply-safe, migrate"
```

### Task 5.3: Contract v2 and the regenerated fixture

**Interfaces:**
- Consumes: `Store::{open, import_outlines, feeds, feed, record_attempt, save_fulltext, set_prefer_full_text, save_diagnoses, set_wayback}`, `garden::{decide, recompute_triage}`, `extract::{Article, ExtractError}`, `parse::{ParsedEntry, ParsedFeed}`, `source::FetchAttempt` and `model::*`.
- Produces:
  - in `crates/ressac-core/tests/contract.rs`, the tests `reading_columns_and_views_are_stable` (live store) and `contract_fixture_covers_the_reading_cases` (committed fixture), plus the helper `fn fill_contract_store(path: &Path)` used by `write_contract_fixture_when_asked`;
  - `tests/fixtures/contract.sqlite` at schema v2, with this content. The Swift phases (7, 9, 10, 12) read it and look rows up by guid or title, never by id:

| Kind | Content |
|---|---|
| Feeds | the 11 feeds and 7 folders of `vienna-like.opml` |
| `xkcd.com` entries | first fill at `2026-10-10T08:00:00.000Z`: `x-recent` (1 day old, has an author), `x-old` (30 days, **read** by the backlog rule), `x-untitled` (no url, no title, only a summary containing "link"), `x-starred` (title "Café comic", `content_html`, **starred**). Unread count 3 |
| `Daring Fireball` entries | `df-extracted` (full text OK; the FTS body contains "whole"), `df-walled` (full text error "The page is behind a login."). `prefer_full_text = 1` |
| Diagnoses | `xkcd.com` and `Daring Fireball` Alive; `Girl on the Couch` Moved (FollowRedirect, remark `http_only`); `Bibliobsession 2.0` Hijacked (clue `vie ⚠`, action Bury); `Orange Drenka` Fossil (clue `2014`); `Unphotographable` Dead (clue `410`, Wayback URL, one fetch-log row with a 301 hop and HTTP 410) |
| Decisions | `Unphotographable` kept; `Photos from bouquiniste` buried |
| `needs_triage` | 3 rows: Girl on the Couch, Bibliobsession 2.0, Orange Drenka |
| Fetch log | 3 rows |

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/tests/contract.rs`.

Replace the imports at the top:

```rust
use std::path::Path;

use ressac_core::model::{Decision, Status};
use ressac_core::store::Store;
use rusqlite::{Connection, OpenFlags};
```

with:

```rust
use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use ressac_core::extract::{Article, ExtractError};
use ressac_core::garden;
use ressac_core::model::{
    Action, Cadence, Decision, Diagnosis, ErrorKind, Evidence, FeedId, Hop, Remark, Status, ts,
};
use ressac_core::parse::{ParsedEntry, ParsedFeed};
use ressac_core::source::FetchAttempt;
use ressac_core::store::Store;
use rusqlite::{Connection, OpenFlags};
```

Below `fn migrations`, add the helpers:

```rust
fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .unwrap();
    stmt.query_map([table], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// A store freshly created by this binary, reopened as a plain connection.
fn live_store() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let conn = Connection::open(&path).unwrap();
    (dir, conn)
}

fn fixture() -> Connection {
    Connection::open_with_flags(
        Path::new(ROOT).join("tests/fixtures/contract.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
```

Append the two tests:

```rust
#[test]
fn reading_columns_and_views_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_entry"),
        [
            "entry_id",
            "feed_id",
            "feed_title",
            "folder_id",
            "title",
            "author",
            "url",
            "published_at",
            "is_read",
            "is_starred",
            "has_fulltext",
            "summary_html",
            "content_html"
        ]
    );
    assert_eq!(columns(&conn, "v_unread_count"), ["feed_id", "unread"]);
    assert_eq!(
        columns(&conn, "v_garden_row"),
        [
            "feed_id",
            "title",
            "folder_path",
            "state",
            "status",
            "reason",
            "clue",
            "evidence",
            "remarks",
            "proposed_action",
            "needs_triage",
            "last_decision",
            "wayback_url",
            "html_url",
            "xml_url"
        ]
    );
    assert_eq!(
        columns(&conn, "entry_fulltext"),
        ["entry_id", "fetched_at", "html", "title", "byline", "error"]
    );
    assert_eq!(columns(&conn, "entry_fts"), ["title", "author", "body"]);
    assert!(columns(&conn, "feed").contains(&"prefer_full_text".to_string()));
    assert!(columns(&conn, "feed_health").contains(&"needs_triage".to_string()));
    let version: i64 = conn
        .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    assert_eq!(version, ressac_core::store::SCHEMA_VERSION);
}

/// What the Swift tests rely on in the committed fixture (rows found by guid or title).
#[test]
fn contract_fixture_covers_the_reading_cases() {
    let db = fixture();
    let int = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    let text = |sql: &str| -> String { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(int("SELECT version FROM schema_meta"), 2);

    // One FTS row per entry; the backlog rule on xkcd's first fill.
    assert_eq!(int("SELECT count(*) FROM entry"), 6);
    assert_eq!(
        int("SELECT count(*) FROM entry WHERE id NOT IN (SELECT rowid FROM entry_fts)"),
        0
    );
    assert_eq!(int("SELECT count(*) FROM entry WHERE read_at IS NOT NULL"), 1);
    assert_eq!(int("SELECT read_at IS NOT NULL FROM entry WHERE guid = 'x-old'"), 1);
    assert_eq!(
        int("SELECT unread FROM v_unread_count WHERE feed_id = (SELECT id FROM feed WHERE title = 'xkcd.com')"),
        3
    );

    // Review Focus 1: no link, no title, only a summary; listed and searchable.
    assert_eq!(
        int("SELECT count(*) FROM v_entry WHERE title IS NULL AND url IS NULL AND summary_html IS NOT NULL"),
        1
    );
    assert_eq!(
        int("SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'link' AND rowid = (SELECT id FROM entry WHERE guid = 'x-untitled')"),
        1
    );
    // Search folds diacritics; one starred entry.
    assert_eq!(
        int("SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'cafe' AND rowid = (SELECT id FROM entry WHERE guid = 'x-starred')"),
        1
    );
    assert_eq!(int("SELECT count(*) FROM v_entry WHERE is_starred"), 1);

    // Full text: one article (its text replaced the FTS body), one stored failure.
    assert_eq!(int("SELECT count(*) FROM v_entry WHERE has_fulltext"), 1);
    assert_eq!(
        int("SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'whole' AND rowid = (SELECT id FROM entry WHERE guid = 'df-extracted')"),
        1
    );
    assert_eq!(
        text("SELECT error FROM entry_fulltext WHERE error IS NOT NULL"),
        "The page is behind a login."
    );
    assert_eq!(int("SELECT count(*) FROM feed WHERE prefer_full_text = 1"), 1);

    // Garden.
    assert_eq!(int("SELECT count(*) FROM v_garden_row WHERE needs_triage = 1"), 3);
    assert_eq!(int("SELECT count(*) FROM v_garden_row WHERE state = 'buried'"), 1);
    assert_eq!(
        text("SELECT last_decision FROM v_garden_row WHERE title = 'Unphotographable'"),
        "keep"
    );
    assert_eq!(
        text("SELECT clue FROM v_garden_row WHERE title = 'Unphotographable'"),
        "410"
    );
    assert_eq!(int("SELECT count(*) FROM v_garden_row WHERE wayback_url IS NOT NULL"), 1);
    assert_eq!(int("SELECT count(*) FROM fetch_log"), 3);
}
```

Refactor the existing tests onto the helpers, without changing what they check:
- `contract_fixture_schema_matches_the_live_store` opens the fixture with `fixture()`;
- `overview_view_columns_are_stable` uses `live_store()` and `columns(&conn, "v_feed_overview")`.

If Phase 2 already added a test with the same purpose as `reading_columns_and_views_are_stable`, keep only one of the two.

- [ ] **Step 2: Run red.** `cargo test -p ressac-core --test contract`. Expected:
  - `reading_columns_and_views_are_stable` passes, since Phase 2 created the schema;
  - `contract_fixture_covers_the_reading_cases` fails on `assert_eq!(int("SELECT count(*) FROM entry"), 6)` (left `0`), because the committed fixture has no entries;
  - if Phase 2 did not regenerate the fixture, it fails earlier on `version` (left `1`), and `contract_fixture_schema_matches_the_live_store` fails too. Step 4 fixes both.

- [ ] **Step 3: Implement the generator.** Replace `write_contract_fixture_when_asked` with the code below. It does not touch the network, and every timestamp derives from `now()`, so a regeneration is reproducible.

```rust
/// Regenerates the fixture shared with the Swift tests:
/// `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract`.
#[test]
fn write_contract_fixture_when_asked() {
    if std::env::var_os("RESSAC_WRITE_CONTRACT").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("contract.sqlite");
    fill_contract_store(&work);
    let target = Path::new(ROOT).join("tests/fixtures/contract.sqlite");
    let _ = std::fs::remove_file(&target);
    Connection::open(&work)
        .unwrap()
        .execute("VACUUM INTO ?1", [target.to_str().unwrap()])
        .unwrap();
}

fn now() -> DateTime<Utc> {
    "2026-10-10T08:00:00Z".parse().unwrap()
}

/// A bare entry (no link, no title, no author): add fields with struct update syntax.
fn parsed(guid: &str, days_ago: i64, summary: &str) -> ParsedEntry {
    ParsedEntry {
        guid: guid.into(),
        url: None,
        title: None,
        author: None,
        published_at: now() - Duration::days(days_ago),
        published_at_is_fallback: false,
        updated_at: None,
        summary_html: Some(summary.into()),
        content_html: None,
    }
}

/// A successful fetch of the feed's own address at `now()`.
fn fetched(store: &Store, feed_id: FeedId, entries: Vec<ParsedEntry>) -> FetchAttempt {
    let url = store.feed(feed_id).unwrap().xml_url;
    FetchAttempt {
        feed_id,
        at: now(),
        requested_url: url.clone(),
        final_url: url,
        http_status: Some(200),
        error: None,
        redirect_chain: vec![],
        content_type: Some("application/rss+xml".into()),
        bytes: 2048,
        duration_ms: 120,
        not_modified: false,
        etag: None,
        last_modified: None,
        feed: Some(ParsedFeed {
            title: None,
            description: None,
            site_url: None,
            entries,
        }),
    }
}

/// A fetch an hour ago that hops to https (301) and ends on HTTP 410.
fn gone(store: &Store, feed_id: FeedId) -> FetchAttempt {
    let url = store.feed(feed_id).unwrap().xml_url;
    let https = url.replacen("http://", "https://", 1);
    FetchAttempt {
        feed_id,
        at: now() - Duration::hours(1),
        requested_url: url,
        final_url: https.clone(),
        http_status: Some(410),
        error: Some(ErrorKind::Http(410)),
        redirect_chain: vec![Hop {
            status: 301,
            url: https,
        }],
        content_type: Some("text/html".into()),
        bytes: 0,
        duration_ms: 80,
        not_modified: false,
        etag: None,
        last_modified: None,
        feed: None,
    }
}

fn diagnosis(status: Status, clue: &str, action: Option<Action>) -> Diagnosis {
    Diagnosis {
        status,
        reason: format!("{} in the contract fixture.", status.as_str()),
        evidence: Evidence {
            bullets: vec![format!("Clue: {clue}.")],
            clue: clue.into(),
            language: None,
        },
        remarks: vec![],
        action,
        hijack_score: 0.0,
        cadence: Cadence::default(),
    }
}

/// The fixture's content, described in `contract_fixture_covers_the_reading_cases`.
fn fill_contract_store(path: &Path) {
    let store = Store::open(path).unwrap();
    let opml =
        std::fs::read_to_string(Path::new(ROOT).join("tests/fixtures/vienna-like.opml")).unwrap();
    store
        .import_outlines(&ressac_core::opml::parse(&opml).unwrap(), false, now())
        .unwrap();
    let feeds = store.feeds().unwrap();
    let id = |title: &str| feeds.iter().find(|f| f.title == title).unwrap().id;
    let (xkcd, fireball, orange, photos) = (
        id("xkcd.com"),
        id("Daring Fireball"),
        id("Orange Drenka"),
        id("Photos from bouquiniste"),
    );
    let (girl, biblio, unphoto) = (
        id("Girl on the Couch"),
        id("Bibliobsession 2.0"),
        id("Unphotographable"),
    );

    // Reading: xkcd's first fill (the backlog rule marks x-old read) and two truncated
    // Daring Fireball entries.
    store
        .record_attempt(
            "contract",
            &fetched(
                &store,
                xkcd,
                vec![
                    ParsedEntry {
                        url: Some("https://xkcd.com/3001/".into()),
                        title: Some("Recent comic".into()),
                        author: Some("Randall Munroe".into()),
                        ..parsed("x-recent", 1, "<p>A recent <b>comic</b> about tides.</p>")
                    },
                    ParsedEntry {
                        url: Some("https://xkcd.com/2900/".into()),
                        title: Some("Old comic".into()),
                        ..parsed("x-old", 30, "<p>An old comic from the backlog.</p>")
                    },
                    parsed("x-untitled", 2, "<p>Only a summary, with no link and no title.</p>"),
                    ParsedEntry {
                        url: Some("https://xkcd.com/3000/".into()),
                        title: Some("Café comic".into()),
                        content_html: Some("<p>Full content about a <i>café</i> by the sea.</p>".into()),
                        ..parsed("x-starred", 3, "<p>Short.</p>")
                    },
                ],
            ),
        )
        .unwrap();
    store
        .record_attempt(
            "contract",
            &fetched(
                &store,
                fireball,
                vec![
                    ParsedEntry {
                        url: Some("https://daringfireball.net/2026/10/tides".into()),
                        title: Some("Linked: Tides".into()),
                        author: Some("John Gruber".into()),
                        ..parsed("df-extracted", 1, "<p>Truncated…</p>")
                    },
                    ParsedEntry {
                        url: Some("https://daringfireball.net/2026/10/walled".into()),
                        title: Some("Walled".into()),
                        ..parsed("df-walled", 2, "<p>Truncated too…</p>")
                    },
                ],
            ),
        )
        .unwrap();
    store.record_attempt("contract", &gone(&store, unphoto)).unwrap();

    // Full text: one article, one stored failure, and the feed that prefers full text.
    let conn = Connection::open(path).unwrap();
    let entry = |guid: &str| -> i64 {
        conn.query_row("SELECT id FROM entry WHERE guid = ?1", [guid], |r| r.get(0))
            .unwrap()
    };
    store.set_prefer_full_text(fireball, true).unwrap();
    let article = Article {
        html: "<article><p>The whole article about tides.</p></article>".into(),
        title: Some("Tides".into()),
        byline: Some("John Gruber".into()),
        text: "The whole article about tides.".into(),
    };
    store
        .save_fulltext(entry("df-extracted"), &Ok(article), now())
        .unwrap();
    store
        .save_fulltext(entry("df-walled"), &Err(ExtractError::LoginWall), now())
        .unwrap();
    // `starred_at` is client-written (the app's column), so the fixture sets it directly.
    conn.execute(
        "UPDATE entry SET starred_at = ?1 WHERE guid = 'x-starred'",
        [ts(now())],
    )
    .unwrap();

    // Garden: three feeds need triage; one Dead feed is kept, one feed is buried.
    store
        .save_diagnoses(
            &HashMap::from([
                (xkcd, diagnosis(Status::Alive, "", None)),
                (fireball, diagnosis(Status::Alive, "", None)),
                (
                    girl,
                    Diagnosis {
                        remarks: vec![Remark::HttpOnly],
                        ..diagnosis(
                            Status::Moved,
                            "301",
                            Some(Action::FollowRedirect {
                                url: "https://girl.example.org/feed".into(),
                            }),
                        )
                    },
                ),
                (biblio, diagnosis(Status::Hijacked, "vie ⚠", Some(Action::Bury))),
                (orange, diagnosis(Status::Fossil, "2014", None)),
                (unphoto, diagnosis(Status::Dead, "410", Some(Action::Bury))),
            ]),
            now(),
        )
        .unwrap();
    store
        .set_wayback(unphoto, "http://web.archive.org/web/2019/http://www.unphotographable.com/")
        .unwrap();
    garden::decide(&store, unphoto, Decision::Keep, now()).unwrap();
    garden::decide(&store, photos, Decision::Bury, now()).unwrap();
    garden::recompute_triage(&store).unwrap();
}
```

- [ ] **Step 4: Regenerate the fixture and run green.**

```bash
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked
cargo test -p ressac-core --test contract
```

Expected: the first command passes (`1 passed`) and rewrites `tests/fixtures/contract.sqlite`; the second passes every contract test. Check the fixture by hand:

```bash
sqlite3 tests/fixtures/contract.sqlite "SELECT version FROM schema_meta; SELECT count(*) FROM entry; SELECT title, needs_triage FROM v_garden_row WHERE needs_triage = 1;"
```

Expected output: `2`, `6`, then the three rows Girl on the Couch, Bibliobsession 2.0 and Orange Drenka, each with `1`. The rows may come in any order.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac-core/tests/contract.rs tests/fixtures/contract.sqlite
git commit -m "Contract v2: reading views and columns, fixture with entries, full text and triage"
```

### Verification gate

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. If `cargo fmt` reports anything, run `cargo fmt --all` and commit `"cargo fmt"`. Then do a smoke check in a temp dir. Never use the real `~/Library` paths:

```bash
cargo build -q -p ressac
T=$(mktemp -d)
r() { HOME="$T" target/debug/ressac --db "$T/db.sqlite" "$@"; }   # HOME only for the binary, never for cargo
r migrate                                  # Schema version 2.
r import tests/fixtures/vienna-like.opml
r --json garden decide 2 bury              # {"event":"decided","feed_id":2,"decision":"bury"}
r --json garden undo                       # {"event":"undone","feed_id":2}
r garden --help                            # lists decide, undo, apply-safe
```

Then push (Global Constraints).

### Notes for the executor
- **`migrate` recomputes the triage flags** (controller reconciliation, 2026-10-10): after a v1 → v2 migration every `needs_triage` is 0 until the next diagnosis or decision, so `Command::Migrate` calls `garden::recompute_triage`. Check it by hand at the end of the phase on a v1 copy: `sqlite3 <copy> 'select count(*) from feed_health where needs_triage=1'` is non-zero after `ressac --db <copy> migrate` when the copy has sick feeds (the calibration scratch database is a v1 database with several).

- **clap 4.6.7 behaviour, checked offline** with a scratch crate using the same derive layout:
  - `garden` gives `action: None`, and `garden apply-safe` maps to `ApplySafe`, because subcommand names are kebab-case by default;
  - `garden decide 3 frob` exits `2` (`InvalidValue`), and so do `garden frob` (`InvalidSubcommand`) and `extract x` (`ValueValidation`);
  - `--feed 1 --folder 2` exits `2` (`ArgumentConflict`). `conflicts_with = "feeds"` names the field, because an argument's id is its field name even when `long = "feed"`;
  - global flags such as `--json` are accepted after the subcommand (`garden undo --json`).
- **Never run a bare `ressac garden` in a test.** crossterm's raw mode falls back to `/dev/tty` when stdin is not a terminal, so from an interactive shell the TUI would start and hang the test. The parse test `bare_garden_still_opens_the_tui` covers it instead, and the existing empty-store test still runs `garden` on a database without feeds, where it prints the hint and returns before the TUI.
- **Usage errors flow through `anyhow`.** `UsageError` is downcast in `main`, so `?` works everywhere and exit `2` never needs a special return path. The message goes to stderr bare, without `error: `, so the app's banner shows the sentence itself (spec §8: last stderr line).
- **No re-diagnosis after decisions in the CLI.** The CLI does not call `diagnose_all` after `decide`, `undo` or `apply-safe`; it relies on Phase 3's `recompute_triage` inside the `garden` functions, which is what the TUI does too. If Phase 3 introduced a facade method that re-diagnoses after decisions, call it in `garden_command` instead of the bare `garden::…` functions.
- **The binary keeps the production politeness.** `FetchLimits::default()` means a 1 s gap per host, and all wiremock feeds share one host, so the subset test runs for about 5 s. Do not lower the limits for tests through a hidden flag.
- **Fixture ids are not stable.** Entry and feed ids in `reading_home` rely on a fresh database, where SQLite rowids start at 1 and follow insertion order (import is depth-first in file order). The contract fixture is looked up by guid and title only, so the Swift phases must do the same.
- **The fixture is binary but committed.** `tests/fixtures/contract.sqlite` is not counted in the 5-file budget (master plan). Regenerate it only with the command in Step 4, and commit it together with `contract.rs`.
