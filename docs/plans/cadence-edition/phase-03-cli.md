# Phase 3: `ressac edition build|show` and `ressac feed cadence`

**Runs after:** phase 2b (3a), then 3a (3b). Phase 4b runs after 3b.

Read the plan index (`docs/plans/2026-10-16-cadence-edition.md`) first. Its Global Constraints apply. This phase implements spec §7 (CLI), the CLI half of the error table in §9 (offline, two builds, refusals), and the CLI items of §10 ("CLI: exit codes of `edition build`, `edition show`, `feed cadence`, `page set 104`"). None of the plan index's five Review Focus items belongs to this phase (they are pinned in phases 2a, 2b, 4a, 5a and 6b).

**Two sub-phases, each at most 5 files:**

- **3a:** `ressac edition build` and `ressac edition show`.
- **3b:** `ressac feed cadence`, the `cadence` event, and the CLI tests of `page list` / `page set` on 104–109.

**Step 0 check:** `crates/ressac/src/main.rs` (103 lines), `crates/ressac/src/cli.rs` (219) and `crates/ressac/src/commands/mod.rs` (230) are under 300 lines: no Step 0. `crates/ressac/src/output.rs` (436) is **not touched** by this phase (see "Where the text helpers live" below), so it needs no Step 0 either. `crates/ressac/tests/cli.rs` (751) only gains tests, which is not a restructuring. Re-check the counts with `wc -l` when the phase starts; if any file the phase modifies is over 300 lines by then, do its Step 0 commit first.

**Where the text helpers live.** The existing text helpers (`page_text`, `decided_text`, …) are in `output.rs`. Putting the new ones there would make 3a touch six files (cli, main, commands/mod, commands/edition, output, tests/cli), over the project's limit. They go in `commands/edition.rs` and `commands/feed.rs` instead, with their unit tests inline, next to the only code that calls them.

## Builds on (phases 1–2, exact names from the plan index)

- `ressac_core::model::CadenceGroup` (`Daily`, `Weekly`, `Monthly`, `Rare`, `Silent`), `CadenceGroup::as_str(self) -> &'static str` (`"daily"` …).
- `ressac_core::edition::{Caps, Section}`: `Caps { week: usize, wire: usize }` with `Caps::default()` = `{ week: 3, wire: 1 }`; `Section::{Front, Week, Wire}`, `Section::as_str(self)` (`"front"`, `"week"`, `"wire"`).
- `ressac_core::service::{EditionOptions, EditionOutcome, EditionReport, EditionView}`:
  - `EditionOptions { caps: Caps, rebuild: bool }`;
  - `EditionOutcome::{Built(EditionReport), NotDue, Offline, Interrupted}`;
  - `EditionReport { id: i64, entries: usize, front: usize, week: usize, wire: usize, overflow: u32, unreachable: u32 }`;
  - `EditionView { edition: EditionRecord, entries: Vec<_>, overflow: Vec<_> }`.
- `Ressac::build_edition(&self, opts: EditionOptions, now: DateTime<Utc>, tz: &impl chrono::TimeZone, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<EditionOutcome>`. It emits the refresh events and, on success, `Event::Edition { id, entries, front, week, wire, overflow, unreachable }` to `sink`; the `--json` sink prints it as `{"event":"edition",…}` (the enum is `#[serde(tag = "event", rename_all = "snake_case")]`).
- `Ressac::edition_today(&self) -> Result<Option<EditionView>>`: `None` when no edition exists; `Some` with empty `entries` for an empty edition.
- `ressac_core::store::EditionRecord { id, built_at, window_start, window_end, unreachable: u32 }` (all times `DateTime<Utc>`). The module `store::edition` is private; `EditionRecord`, `EditionView`, `EditionEntryRow` and `EditionOverflowRow` are re-exported from `ressac_core::store` (and the three view types also from `ressac_core::service`). The store's reader is `Store::edition_view`, exposed as `Ressac::edition_today`.
- `Ressac::set_cadence(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<CadenceRow>`; `CadenceRow { feed_id, effective_group, computed_group, is_override }`.
- `ressac_core::Error::{UnknownFeed(FeedId), FeedBuried(FeedId), EditionNotDue}`, displaying exactly `No feed #N.`, `Feed #N is buried.`, `An edition was already built this morning.`
- `pages::list` returns 14 reserved rows (100–109, 800, 801, 802, 899) and `pages::set` refuses 104–109 with `Page N is reserved.` (checked before the range test).

**Field names this phase reads on the edition rows** (the plan index says they "mirror the columns of `v_edition_today` / `v_edition_overflow`"; these are the names this phase relies on, and phase 2b must use them):

- each `EditionView.entries[i]`: `entry_id: i64`, `section: Section`, `rank: u32`, `feed_id: FeedId`, `feed_title: String`, `title: Option<String>`, `published_at: DateTime<Utc>`, `read_at: Option<DateTime<Utc>>`, `starred_at: Option<DateTime<Utc>>`;
- each `EditionView.overflow[i]`: `section: Section`, `feed_id: FeedId`, `feed_title: String`, `page: Option<u16>`, `hidden_count: u32`.

**Already done by earlier phases (the workspace gate forces it, this phase relies on it):**

- Phase 1b updated `migrate_reports_the_schema_version` (`Schema version 4.`), and Phase 1d the two page tests that 14 reserved pages break: `page_list_prints_reserved_pages_then_every_folder_and_feed` expects the 14 reserved lines and `lines.len() == 32` (`lines[17]` for Orange Drenka); in `page_set_refusals_exit_2_and_write_nothing`, page `109` now answers `Page 109 is reserved.` This phase does not edit those assertions again; Task 3b.3 only adds a new test (JSON row 104, `folder` kind, every page 104–109).
- Phase 2b added `| Event::Edition { .. }` to the no-op arm of the progress display in `output.rs` (`Sink::emit`), since that `match` is exhaustive; Phase 1c declared `Error::EditionNotDue`.

If either is missing when 3a starts, stop and fix the owning phase first: this phase does not touch `output.rs`, and its gate would not compile.

## Existing CLI machinery used as is

- `UsageError` / `usage(…)` in `commands/mod.rs`: downcast in `main`, exit `2`, bare message on stderr.
- `open_with_feeds(db)`: first-run hint `No feeds yet: run "ressac import <file.opml>" first.` on stderr and exit `0` on a missing or empty database.
- `output::sink(json)`, `Sink::finish()`, `Sink::not_fetched()`, `output::interrupted_message(n)`, `output::summary_text(&Summary, ascii)`, `output::write_result(out, json, &Event, text)`.
- `INTERRUPTED` (130) and `Globals { db, json, ascii, stop }`.
- `Ressac::pages()` (to show a page number next to each edition entry) and `Ressac::summary()` (the text summary after a build, as `check` does).
- Test helpers in `crates/ressac/tests/cli.rs`: `ressac`, `ressac_async`, `stdout`, `stderr`, `json_lines`, `channel`, `FIXTURE`, and wiremock's `MockServer` (the CLI tests already fetch feeds from it).

## CLI contract produced by this phase

`ressac [--db P] [--json] …`:

| Command | `--json` stdout | Text stdout | Exit `2` (message on stderr) |
|---|---|---|---|
| `edition build [--cap-week N] [--cap-wire N] [--rebuild]` | the refresh events (`started`, `feed_done`, `network_unreachable`, `finished`, …), then `{"event":"edition","id":…,"entries":…,"front":…,"week":…,"wire":…,"overflow":…,"unreachable":…}` | `summary_text` of the refresh, then `Edition #12: 43 entries (front 2, week 7, wire 34), 61 more left in their feeds.` | `An edition was already built this morning.` (nothing on stdout); `The network is unreachable: no edition built.` (the refresh events may already be on stdout) |
| `edition show` | one `{"event":"edition_entry",…}` per entry in rank order, then one `{"event":"edition_overflow",…}` per overflow | section headings, `rank page title` lines, `+N more in Feed` lines, then the header line | none; no edition yet → text `No edition yet.`, JSON nothing; exit `0` |
| `feed cadence ID auto\|daily\|weekly\|monthly\|rare\|silent` | `{"event":"cadence","feed_id":12,"group":"daily","computed":"weekly","override":true}` | `#12 Daily (set by you)` or `#12 Weekly (auto)` | `No feed #N.`, `Feed #N is buried.` (nothing on stdout) |
| `page list`, `page set` | unchanged shapes | unchanged | `page set feed|folder ID 104`…`109` → `Page N is reserved.` |

Clap's own errors (`edition` alone, `feed` alone, a cap that is not a number, an unknown group) exit `2`. On a database with no feeds, `edition build`, `edition show` and `feed cadence` print the first-run hint and exit `0`, like `garden`. A database error exits `1` with `error: …`. `edition build` stopped by Ctrl-C prints `Interrupted: N feeds were not fetched.` and exits `130`, writing no edition.

**Text formats** (all in `commands/edition.rs` / `commands/feed.rs`):

- Build line: `Edition #{id}: {entries} entries (front {f}, week {w}, wire {r})`, then `, {n} more left in their feeds.` when `overflow > 1`, `, 1 more left in its feed.` when `overflow == 1`, `.` when `0`; then ` {n} feeds unreachable.` / ` 1 feed unreachable.` when `unreachable > 0`. `1 entry` in the singular.
- Show, per section that has entries: the heading (`Front Page`, `This Week`, `The Wire`), then one line per entry `{rank} {page} {title}` (`-` when the feed has no page, `(untitled)` when the entry has no title), then that section's overflow lines `+{n} more in {feed title}`, sorted by page then feed id. An empty edition prints `Nothing new this morning.` instead of the sections. The last line is the header: `{Weekday} {day} {Month} · {n} entries · {r} read`, plus ` · {u} feeds unreachable` (` · 1 feed unreachable`) when `unreachable > 0`; `built_at` in local time, English names (`%A %-d %B`).
- Feed cadence: `#{id} {Group}` then ` (set by you)` or ` (auto)`, the group title capitalised (`Daily` … `Silent`).

---

# Sub-phase 3a: `ressac edition build` and `ressac edition show`

**Files (5):**
- Modify: `crates/ressac/src/cli.rs` (`Command::Edition`, `EditionCommand`)
- Modify: `crates/ressac/src/main.rs` (dispatch)
- Modify: `crates/ressac/src/commands/mod.rs` (`pub mod edition;`)
- Create: `crates/ressac/src/commands/edition.rs` (handlers, text helpers, unit tests inline)
- Modify: `crates/ressac/tests/cli.rs`

### Task 3a.1: Text helpers for the edition

**Files:**
- Create: `crates/ressac/src/commands/edition.rs`
- Modify: `crates/ressac/src/commands/mod.rs`

- [ ] **Step 1: Declare the module**

In `crates/ressac/src/commands/mod.rs`, next to the two existing declarations:

```rust
pub mod edition;
pub mod garden;
pub mod network;
```

- [ ] **Step 2: Write the failing unit tests**

Create `crates/ressac/src/commands/edition.rs` with only the module doc and the tests:

```rust
//! `ressac edition build | show`: the morning edition from the command line.

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use ressac_core::edition::Section;
    use ressac_core::service::EditionReport;

    fn report(entries: usize, overflow: u32, unreachable: u32) -> EditionReport {
        EditionReport {
            id: 12,
            entries,
            front: 2,
            week: 7,
            wire: 34,
            overflow,
            unreachable,
        }
    }

    #[test]
    fn built_text_counts_sections_overflow_and_failures() {
        assert_eq!(
            built_text(&report(43, 61, 0)),
            "Edition #12: 43 entries (front 2, week 7, wire 34), 61 more left in their feeds."
        );
        assert_eq!(
            built_text(&report(43, 1, 0)),
            "Edition #12: 43 entries (front 2, week 7, wire 34), 1 more left in its feed."
        );
        assert_eq!(
            built_text(&report(43, 0, 3)),
            "Edition #12: 43 entries (front 2, week 7, wire 34). 3 feeds unreachable."
        );
        assert_eq!(
            built_text(&report(1, 0, 1)),
            "Edition #12: 1 entry (front 2, week 7, wire 34). 1 feed unreachable."
        );
    }

    #[test]
    fn section_titles_are_the_apps_headings() {
        assert_eq!(section_title(Section::Front), "Front Page");
        assert_eq!(section_title(Section::Week), "This Week");
        assert_eq!(section_title(Section::Wire), "The Wire");
    }

    #[test]
    fn entry_and_overflow_lines() {
        assert_eq!(entry_line(3, Some(412), Some("Tides")), "3 412 Tides");
        assert_eq!(entry_line(4, None, None), "4 - (untitled)");
        assert_eq!(overflow_line(12, "Actualitté"), "+12 more in Actualitté");
    }

    #[test]
    fn header_line_names_the_day_counts_and_failures() {
        let built = Local.with_ymd_and_hms(2026, 10, 16, 6, 30, 0).unwrap();
        assert_eq!(
            header_line(built, 43, 12, 0),
            "Friday 16 October · 43 entries · 12 read"
        );
        assert_eq!(
            header_line(built, 1, 0, 1),
            "Friday 16 October · 1 entry · 0 read · 1 feed unreachable"
        );
        assert_eq!(
            header_line(built, 43, 12, 3),
            "Friday 16 October · 43 entries · 12 read · 3 feeds unreachable"
        );
    }
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p ressac --bin ressac commands::edition`
Expected: compile errors, `cannot find function built_text` (and `section_title`, `entry_line`, `overflow_line`, `header_line`).

- [ ] **Step 4: Write the helpers**

Above the `#[cfg(test)]` module of `commands/edition.rs`:

```rust
use chrono::{DateTime, TimeZone};
use ressac_core::edition::Section;
use ressac_core::service::EditionReport;

/// The line after `summary_text` once an edition is written.
pub fn built_text(r: &EditionReport) -> String {
    let mut text = format!(
        "Edition #{}: {} (front {}, week {}, wire {})",
        r.id,
        count(r.entries, "entry", "entries"),
        r.front,
        r.week,
        r.wire,
    );
    match r.overflow {
        0 => text.push('.'),
        1 => text.push_str(", 1 more left in its feed."),
        n => text.push_str(&format!(", {n} more left in their feeds.")),
    }
    if r.unreachable > 0 {
        text.push_str(&format!(
            " {} unreachable.",
            count(r.unreachable as usize, "feed", "feeds")
        ));
    }
    text
}

/// The app's section headings (spec §8), shared by `edition show`.
pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Front => "Front Page",
        Section::Week => "This Week",
        Section::Wire => "The Wire",
    }
}

/// `rank page title`, with `-` for a feed without a page and `(untitled)` for an entry
/// without a title.
pub fn entry_line(rank: u32, page: Option<u16>, title: Option<&str>) -> String {
    let page = page.map_or("-".to_string(), |p| p.to_string());
    format!("{rank} {page} {}", title.unwrap_or("(untitled)"))
}

pub fn overflow_line(hidden: u32, feed_title: &str) -> String {
    format!("+{hidden} more in {feed_title}")
}

/// The Journal's header (spec §8): `Friday 16 October · 43 entries · 12 read`.
pub fn header_line<Tz: TimeZone>(
    built_at: DateTime<Tz>,
    entries: usize,
    read: usize,
    unreachable: u32,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut text = format!(
        "{} · {} · {read} read",
        built_at.format("%A %-d %B"),
        count(entries, "entry", "entries")
    );
    if unreachable > 0 {
        text.push_str(&format!(
            " · {} unreachable",
            count(unreachable as usize, "feed", "feeds")
        ));
    }
    text
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
```

Until Task 3a.2 calls them, these `pub fn` of a binary crate are dead code: `cargo test` warns, which is expected at this step. The gate only runs at the end of 3a.

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p ressac --bin ressac commands::edition`
Expected: `4 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/ressac/src/commands/mod.rs crates/ressac/src/commands/edition.rs
git commit -m "CLI: text helpers of the morning edition

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3a.2: `ressac edition build`

**Files:**
- Modify: `crates/ressac/src/cli.rs`
- Modify: `crates/ressac/src/main.rs`
- Modify: `crates/ressac/src/commands/edition.rs`
- Modify: `crates/ressac/tests/cli.rs`

- [ ] **Step 1: Write the failing CLI test and its fixture helper**

At the end of `crates/ressac/tests/cli.rs`, add the helper and the test. The helper serves three feeds whose rhythm the classification of phase 1a decides without doubt, imported at the top level (feed ids 1, 2, 3; pages 110, 111, 112):

- **Rare** (id 1): an entry 20 hours ago and one 100 days ago: median interval ≈ 99 days → `rare` → Front Page. The old entry arrives read (the fetcher marks a new feed's entries older than 7 days as read).
- **Weekly** (id 2): entries 1, 6, 11 and 16 days ago: median 5 days → `weekly` → This Week. The two oldest arrive read.
- **Wire** (id 3): entries 1, 3 and 5 hours ago: median 2 hours → `daily` → The Wire, capped to 1, overflow 2.

```rust
/// One RSS item dated `ago` before now.
fn dated_item(guid: &str, title: &str, ago: chrono::Duration) -> String {
    let date = (chrono::Utc::now() - ago).to_rfc2822();
    format!("<item><guid>{guid}</guid><title>{title}</title><pubDate>{date}</pubDate></item>")
}

/// Three top-level feeds with a known rhythm, imported but not fetched: Rare (#1, page 110),
/// Weekly (#2, 111) and Wire (#3, 112). See Task 3a.2 for the edition they give.
async fn edition_home(server: &MockServer) -> tempfile::TempDir {
    use chrono::Duration;
    let feeds = [
        (
            "Rare",
            "/rare.xml",
            dated_item("r1", "Rare news", Duration::hours(20))
                + &dated_item("r0", "Rare old", Duration::days(100)),
        ),
        (
            "Weekly",
            "/weekly.xml",
            [1, 6, 11, 16]
                .iter()
                .map(|d| dated_item(&format!("w{d}"), &format!("Weekly {d}d"), Duration::days(*d)))
                .collect::<String>(),
        ),
        (
            "Wire",
            "/wire.xml",
            [1, 3, 5]
                .iter()
                .map(|h| dated_item(&format!("h{h}"), &format!("Wire {h}h"), Duration::hours(*h)))
                .collect::<String>(),
        ),
    ];
    let base = server.uri();
    let mut outlines = String::new();
    for (title, at, items) in &feeds {
        Mock::given(method("GET"))
            .and(path(*at))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(channel(title, items), "application/rss+xml"),
            )
            .mount(server)
            .await;
        outlines.push_str(&format!(r#"<outline text="{title}" xmlUrl="{base}{at}"/>"#));
    }
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("edition.opml");
    std::fs::write(&opml, format!("<opml><body>{outlines}</body></opml>")).unwrap();
    let import = ressac_async(home.path(), &["import", opml.to_str().unwrap()]).await;
    assert!(import.status.success(), "{import:?}");
    home
}

#[tokio::test]
async fn edition_build_composes_once_a_morning() {
    let server = MockServer::start().await;
    let home = edition_home(&server).await;

    let built = ressac_async(home.path(), &["--json", "edition", "build"]).await;
    assert!(built.status.success(), "{built:?}");
    let lines = json_lines(&built);
    assert_eq!(lines.first().unwrap()["event"], "started");
    assert!(lines.iter().any(|l| l["event"] == "feed_done"));
    assert_eq!(
        lines.last().unwrap(),
        &json!({"event": "edition", "id": 1, "entries": 4, "front": 1, "week": 2,
                "wire": 1, "overflow": 2, "unreachable": 0})
    );

    let again = ressac_async(home.path(), &["edition", "build"]).await;
    assert_eq!(again.status.code(), Some(2), "{again:?}");
    assert_eq!(
        stderr(&again),
        "An edition was already built this morning.\n"
    );
    assert!(again.stdout.is_empty(), "refused before any refresh");

    // Same window, other caps: the wire keeps all three entries of Wire.
    let rebuilt = ressac_async(
        home.path(),
        &["edition", "build", "--rebuild", "--cap-wire", "3"],
    )
    .await;
    assert!(rebuilt.status.success(), "{rebuilt:?}");
    let text = stdout(&rebuilt);
    assert!(
        text.contains(": 6 entries (front 1, week 2, wire 3)."),
        "{text}"
    );
    assert!(
        text.contains("run `ressac garden` to triage"),
        "the refresh summary comes first: {text}"
    );
}
```

The time of day matters only through the 04:00 rule: two runs a few seconds apart could straddle 04:00 local time, which would make the second build due. The window for that is a few seconds a day; the test accepts it rather than adding a clock hook to a release binary.

- [ ] **Step 2: Run the test to see it fail**

Run: `cargo test -p ressac --test cli edition_build_composes_once_a_morning`
Expected: FAIL, the first build exits `2` with clap's `unrecognized subcommand 'edition'`.

- [ ] **Step 3: Add the subcommand to `cli.rs`**

In `crates/ressac/src/cli.rs`, add the variant at the end of `enum Command`:

```rust
    /// Build or show the morning edition
    Edition {
        #[command(subcommand)]
        action: EditionCommand,
    },
```

and, after `enum PageKindArg`:

```rust
#[derive(Subcommand)]
pub enum EditionCommand {
    /// Refresh every feed, then compose this morning's edition (once a day, from 04:00)
    Build {
        /// Entries per weekly feed [default: 3]
        #[arg(long, value_name = "N")]
        cap_week: Option<usize>,
        /// Entries per daily feed [default: 1]
        #[arg(long, value_name = "N")]
        cap_wire: Option<usize>,
        /// Replace this morning's edition, over the same window
        #[arg(long)]
        rebuild: bool,
    },
    /// Print this morning's edition by section
    Show,
}
```

- [ ] **Step 4: Dispatch it in `main.rs`**

In `crates/ressac/src/main.rs`, add the arm at the end of `match cli.command`:

```rust
        Command::Edition { action } => commands::edition::edition(&g, action, &mut out).await,
```

- [ ] **Step 5: Write the handler**

In `crates/ressac/src/commands/edition.rs`, extend the imports and add the handler above `built_text`:

```rust
use std::io::Write;
use std::process::ExitCode;

use chrono::{DateTime, Local, TimeZone, Utc};
use ressac_core::edition::{Caps, Section};
use ressac_core::service::{EditionOptions, EditionOutcome, EditionReport, Ressac};

use super::{Globals, INTERRUPTED, open_with_feeds, usage};
use crate::cli::EditionCommand;
use crate::output;

/// `ressac edition build` when every fetch failed for network reasons (spec §9).
const OFFLINE: &str = "The network is unreachable: no edition built.";

/// `ressac edition build | show`.
pub async fn edition(
    g: &Globals,
    action: EditionCommand,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        EditionCommand::Build {
            cap_week,
            cap_wire,
            rebuild,
        } => build(g, &app, cap_week, cap_wire, rebuild, out).await,
        EditionCommand::Show => show(g, &app, out),
    }
}

/// The refusal of a second build the same morning is a usage error (exit 2), whether the
/// service saw it before the refresh (`NotDue`) or inside the write (`EditionNotDue`).
fn edition_error(e: ressac_core::Error) -> anyhow::Error {
    match e {
        ressac_core::Error::EditionNotDue => usage(e.to_string()),
        other => other.into(),
    }
}

async fn build(
    g: &Globals,
    app: &Ressac,
    cap_week: Option<usize>,
    cap_wire: Option<usize>,
    rebuild: bool,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let defaults = Caps::default();
    let opts = EditionOptions {
        caps: Caps {
            week: cap_week.unwrap_or(defaults.week),
            wire: cap_wire.unwrap_or(defaults.wire),
        },
        rebuild,
    };
    let sink = output::sink(g.json);
    let outcome = app
        .build_edition(opts, Utc::now(), &Local, &sink, g.stop.clone())
        .await;
    sink.finish();
    match outcome.map_err(edition_error)? {
        EditionOutcome::NotDue => Err(edition_error(ressac_core::Error::EditionNotDue)),
        EditionOutcome::Offline => Err(usage(OFFLINE)),
        EditionOutcome::Interrupted => {
            eprintln!("{}", output::interrupted_message(sink.not_fetched()));
            Ok(ExitCode::from(INTERRUPTED))
        }
        EditionOutcome::Built(report) => {
            // With --json the sink already printed the `edition` event.
            if !g.json {
                writeln!(out, "{}", output::summary_text(&app.summary()?, g.ascii))?;
                writeln!(out, "{}", built_text(&report))?;
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
```

`show` is written in Task 3a.3; until then, add a stub so the crate compiles:

```rust
fn show(_g: &Globals, _app: &Ressac, _out: &mut impl Write) -> anyhow::Result<ExitCode> {
    Ok(ExitCode::SUCCESS)
}
```

Remove the now-duplicate `use chrono::{DateTime, TimeZone};`, `use ressac_core::edition::Section;` and `use ressac_core::service::EditionReport;` lines that Task 3a.1 put at the top: the block above replaces them.

- [ ] **Step 6: Run the test to see it pass**

Run: `cargo test -p ressac --test cli edition_build_composes_once_a_morning`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/ressac/src/cli.rs crates/ressac/src/main.rs crates/ressac/src/commands/edition.rs crates/ressac/tests/cli.rs
git commit -m "CLI: ressac edition build, once a morning, with --rebuild and caps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3a.3: `ressac edition show`

**Files:**
- Modify: `crates/ressac/src/commands/edition.rs`
- Modify: `crates/ressac/tests/cli.rs`

- [ ] **Step 1: Write the failing CLI tests**

Append to `crates/ressac/tests/cli.rs`:

```rust
#[tokio::test]
async fn edition_show_prints_sections_overflow_and_header() {
    let server = MockServer::start().await;
    let home = edition_home(&server).await;
    let built = ressac_async(home.path(), &["edition", "build"]).await;
    assert!(built.status.success(), "{built:?}");

    let text = ressac_async(home.path(), &["edition", "show"]).await;
    assert!(text.status.success(), "{text:?}");
    let out = stdout(&text);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[..lines.len() - 1],
        [
            "Front Page",
            "1 110 Rare news",
            "This Week",
            "2 111 Weekly 1d",
            "3 111 Weekly 6d",
            "The Wire",
            "4 112 Wire 1h",
            "+2 more in Wire",
        ]
    );
    assert!(
        lines.last().unwrap().ends_with(" · 4 entries · 0 read"),
        "{out}"
    );

    let json = ressac_async(home.path(), &["--json", "edition", "show"]).await;
    assert!(json.status.success(), "{json:?}");
    let rows = json_lines(&json);
    assert_eq!(rows.len(), 5, "4 entries, then 1 overflow");
    assert_eq!(rows[0]["event"], "edition_entry");
    assert_eq!(rows[0]["edition_id"], 1);
    assert_eq!(rows[0]["section"], "front");
    assert_eq!(rows[0]["rank"], 1);
    assert_eq!(rows[0]["feed_id"], 1);
    assert_eq!(rows[0]["feed_title"], "Rare");
    assert_eq!(rows[0]["title"], "Rare news");
    assert_eq!(rows[0]["read_at"], Value::Null);
    assert_eq!(rows[0]["unreachable"], 0);
    for key in ["built_at", "window_start", "window_end", "published_at", "entry_id"] {
        assert!(!rows[0][key].is_null(), "{key}");
    }
    assert_eq!(rows[3]["section"], "wire");
    assert_eq!(rows[3]["title"], "Wire 1h");
    assert_eq!(
        rows[4],
        json!({"event": "edition_overflow", "edition_id": 1, "section": "wire",
               "feed_id": 3, "feed_title": "Wire", "page": 112, "hidden_count": 2})
    );
}

#[test]
fn edition_show_before_any_edition() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let text = ressac(home.path(), &["edition", "show"]);
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    assert_eq!(stdout(&text), "No edition yet.\n");
    let json = ressac(home.path(), &["--json", "edition", "show"]);
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    assert!(json.stdout.is_empty(), "an empty NDJSON stream");
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p ressac --test cli edition_show`
Expected: FAIL in both: the stub prints nothing (`lines[..lines.len() - 1]` panics on an empty output; `No edition yet.` is missing).

- [ ] **Step 3: Write `show`**

In `crates/ressac/src/commands/edition.rs`, replace the stub with:

```rust
/// Printed by `edition show` before the first edition (text only; `--json` prints nothing).
const NO_EDITION: &str = "No edition yet.";

/// Printed by `edition show` instead of the sections when the edition is empty.
const NOTHING_NEW: &str = "Nothing new this morning.";

fn show(g: &Globals, app: &Ressac, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(view) = app.edition_today()? else {
        if !g.json {
            writeln!(out, "{NO_EDITION}")?;
        }
        return Ok(ExitCode::SUCCESS);
    };
    let ed = &view.edition;
    if g.json {
        for e in &view.entries {
            let row = serde_json::json!({
                "event": "edition_entry",
                "edition_id": ed.id,
                "built_at": ed.built_at,
                "window_start": ed.window_start,
                "window_end": ed.window_end,
                "unreachable": ed.unreachable,
                "entry_id": e.entry_id,
                "section": e.section,
                "rank": e.rank,
                "feed_id": e.feed_id,
                "feed_title": e.feed_title,
                "title": e.title,
                "published_at": e.published_at,
                "read_at": e.read_at,
                "starred_at": e.starred_at,
            });
            writeln!(out, "{row}")?;
        }
        for o in &view.overflow {
            let row = serde_json::json!({
                "event": "edition_overflow",
                "edition_id": ed.id,
                "section": o.section,
                "feed_id": o.feed_id,
                "feed_title": o.feed_title,
                "page": o.page,
                "hidden_count": o.hidden_count,
            });
            writeln!(out, "{row}")?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    let pages: std::collections::HashMap<i64, u16> = app
        .pages()?
        .into_iter()
        .filter(|r| r.kind == ressac_core::store::pages::PageKind::Feed)
        .filter_map(|r| r.id.map(|id| (id, r.page)))
        .collect();
    if view.entries.is_empty() {
        writeln!(out, "{NOTHING_NEW}")?;
    }
    for section in [Section::Front, Section::Week, Section::Wire] {
        let mut entries: Vec<_> = view.entries.iter().filter(|e| e.section == section).collect();
        if entries.is_empty() {
            continue;
        }
        entries.sort_by_key(|e| e.rank);
        writeln!(out, "{}", section_title(section))?;
        for e in entries {
            let page = pages.get(&e.feed_id).copied();
            writeln!(out, "{}", entry_line(e.rank, page, e.title.as_deref()))?;
        }
        let mut overflow: Vec<_> = view.overflow.iter().filter(|o| o.section == section).collect();
        overflow.sort_by_key(|o| (o.page.is_none(), o.page, o.feed_id));
        for o in overflow {
            writeln!(out, "{}", overflow_line(o.hidden_count, &o.feed_title))?;
        }
    }
    let read = view.entries.iter().filter(|e| e.read_at.is_some()).count();
    let built = ed.built_at.with_timezone(&Local);
    writeln!(out, "{}", header_line(built, view.entries.len(), read, ed.unreachable))?;
    Ok(ExitCode::SUCCESS)
}
```

`Section` must derive `PartialEq` and `Serialize` (it does: plan index, phase 2a), so `e.section == section` compiles and `json!` writes `"front"` / `"week"` / `"wire"`. The JSON times are chrono's serde form (`2026-10-16T06:30:00.123Z`); the test only checks they are present.

- [ ] **Step 4: Unit test the empty edition text**

Add to the `tests` module of `commands/edition.rs`:

```rust
    #[test]
    fn empty_and_missing_edition_texts() {
        assert_eq!(NO_EDITION, "No edition yet.");
        assert_eq!(NOTHING_NEW, "Nothing new this morning.");
    }
```

(An empty edition cannot be produced through the binary without controlling the clock; phase 2b tests the empty composition, this test pins the two strings the spec names.)

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p ressac --test cli edition_show && cargo test -p ressac --bin ressac commands::edition`
Expected: `2 passed`, then `5 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/ressac/src/commands/edition.rs crates/ressac/tests/cli.rs
git commit -m "CLI: ressac edition show, by section, in text and NDJSON

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3a.4: Refusals, first-run hint and database errors of `edition`

**Files:**
- Modify: `crates/ressac/tests/cli.rs`

These paths are already implemented by Tasks 3a.2 and 3a.3 (through `open_with_feeds`, `usage` and clap). This task pins them; each assertion below should pass on the first run. If one fails, fix the handler, not the test.

- [ ] **Step 1: Extend the empty-store and usage tests**

In `commands_on_an_empty_store_hint_at_import`, add to the `for args in [ … ]` list:

```rust
        &["edition", "build"],
        &["--json", "edition", "build"],
        &["edition", "show"],
```

In `usage_errors_exit_2`, add:

```rust
        &["edition"],
        &["edition", "frobnicate"],
        &["edition", "build", "--cap-wire", "x"],
        &["edition", "build", "--cap-week", "-1"],
```

- [ ] **Step 2: Add the offline and database-error tests**

Append to `crates/ressac/tests/cli.rs`:

```rust
/// Four feeds on a port nobody listens to: every fetch fails to connect, so the refresh's
/// network guard (4 attempts at least, more than half network failures) calls the network
/// unreachable, and no edition is written (spec §9).
#[test]
fn edition_build_offline_writes_nothing() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let home = tempfile::tempdir().unwrap();
    let outlines: String = (1..=4)
        .map(|i| format!(r#"<outline text="F{i}" xmlUrl="http://127.0.0.1:{port}/f{i}.xml"/>"#))
        .collect();
    let opml = home.path().join("closed.opml");
    std::fs::write(&opml, format!("<opml><body>{outlines}</body></opml>")).unwrap();
    assert!(
        ressac(home.path(), &["import", opml.to_str().unwrap()])
            .status
            .success()
    );

    let out = ressac(home.path(), &["edition", "build"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        stderr(&out).ends_with("The network is unreachable: no edition built.\n"),
        "{out:?}"
    );
    assert_eq!(stdout(&ressac(home.path(), &["edition", "show"])), "No edition yet.\n");

    let json = ressac(home.path(), &["--json", "edition", "build"]);
    assert_eq!(json.status.code(), Some(2), "still due, still offline: {json:?}");
    assert!(
        json_lines(&json)
            .iter()
            .any(|l| l["event"] == "network_unreachable"),
        "{json:?}"
    );
    assert!(!json_lines(&json).iter().any(|l| l["event"] == "edition"));
}

#[test]
fn edition_commands_exit_1_on_a_database_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("db.sqlite"), b"this is not a database").unwrap();
    for args in [&["edition", "show"][..], &["edition", "build"]] {
        let out = ressac(home.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(
            stderr(&out).starts_with("error: cannot open database"),
            "{out:?}"
        );
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p ressac --test cli -- edition commands_on_an_empty_store usage_errors`
Expected: all pass. If `edition_build_offline_writes_nothing` exits `0`, check that the refresh classifies a refused connection as `ErrorKind::Connect` (it does since sub-project 1; `is_network()` covers `Dns` and `Connect`) and that `build_edition` returns `Offline` when `summary.network_unreachable` is set (phase 2b).

- [ ] **Step 4: Commit**

```bash
git add crates/ressac/tests/cli.rs
git commit -m "CLI tests: edition refusals, first-run hint, offline and database errors

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 3a

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no diff, no warning, every test green. The Swift suite is not run (no Swift change). Stop here for Clément's approval before 3b (project rule).

---

# Sub-phase 3b: `ressac feed cadence` and the reserved pages 104–109

**Files (5):**
- Modify: `crates/ressac/src/cli.rs` (`Command::Feed`, `FeedCommand`, `CadenceArg`)
- Modify: `crates/ressac/src/main.rs` (dispatch)
- Modify: `crates/ressac/src/commands/mod.rs` (`pub mod feed;`)
- Create: `crates/ressac/src/commands/feed.rs` (handler, text and JSON helpers, unit tests inline)
- Modify: `crates/ressac/tests/cli.rs`

**Also builds on (phase 1c):** `CadenceRow` is re-exported as `ressac_core::store::CadenceRow` (next to `FeedRow` in `store/mod.rs`'s `pub use feeds::{…}`), with the fields of the plan index: `feed_id: FeedId`, `effective_group: CadenceGroup`, `computed_group: CadenceGroup`, `is_override: bool`.

**Why the CLI writes the `cadence` JSON itself.** The event's keys (`group`, `computed`, `override`) are shorter than `CadenceRow`'s field names, and the core has no `Event::Cadence` (the plan index defines none). Like `import --json`, the handler builds the object with `serde_json::json!`. Only phase 4b reads it.

### Task 3b.1: Text and JSON of a rhythm change

**Files:**
- Create: `crates/ressac/src/commands/feed.rs`
- Modify: `crates/ressac/src/commands/mod.rs`
- Modify: `crates/ressac/src/cli.rs`

- [ ] **Step 1: Declare the module and the argument type**

In `crates/ressac/src/commands/mod.rs`:

```rust
pub mod edition;
pub mod feed;
pub mod garden;
pub mod network;
```

In `crates/ressac/src/cli.rs`, add to the imports:

```rust
use ressac_core::model::CadenceGroup;
```

and, after `enum EditionCommand`:

```rust
#[derive(Subcommand)]
pub enum FeedCommand {
    /// Force a feed's rhythm group, or give it back to the automatic one
    Cadence {
        /// The feed's id
        feed_id: FeedId,
        #[arg(value_enum)]
        group: CadenceArg,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum CadenceArg {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Rare,
    Silent,
}

impl CadenceArg {
    /// `None` for `auto`: the override is cleared.
    pub fn group(self) -> Option<CadenceGroup> {
        match self {
            CadenceArg::Auto => None,
            CadenceArg::Daily => Some(CadenceGroup::Daily),
            CadenceArg::Weekly => Some(CadenceGroup::Weekly),
            CadenceArg::Monthly => Some(CadenceGroup::Monthly),
            CadenceArg::Rare => Some(CadenceGroup::Rare),
            CadenceArg::Silent => Some(CadenceGroup::Silent),
        }
    }
}
```

- [ ] **Step 2: Write the failing unit tests**

Create `crates/ressac/src/commands/feed.rs`:

```rust
//! `ressac feed cadence`: force a feed's rhythm group, or give it back to the core.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CadenceArg;
    use ressac_core::model::CadenceGroup;
    use ressac_core::store::CadenceRow;
    use serde_json::json;

    fn row(effective: CadenceGroup, computed: CadenceGroup, is_override: bool) -> CadenceRow {
        CadenceRow {
            feed_id: 12,
            effective_group: effective,
            computed_group: computed,
            is_override,
        }
    }

    #[test]
    fn cadence_text_names_the_group_and_who_chose_it() {
        assert_eq!(
            cadence_text(&row(CadenceGroup::Daily, CadenceGroup::Weekly, true)),
            "#12 Daily (set by you)"
        );
        assert_eq!(
            cadence_text(&row(CadenceGroup::Weekly, CadenceGroup::Weekly, false)),
            "#12 Weekly (auto)"
        );
    }

    #[test]
    fn cadence_json_is_the_cadence_event() {
        assert_eq!(
            cadence_json(&row(CadenceGroup::Daily, CadenceGroup::Weekly, true)),
            json!({"event": "cadence", "feed_id": 12, "group": "daily",
                   "computed": "weekly", "override": true})
        );
    }

    #[test]
    fn group_titles_are_the_sidebar_names() {
        let titles: Vec<&str> = CadenceGroup::ALL.iter().map(|g| group_title(*g)).collect();
        assert_eq!(titles, ["Daily", "Weekly", "Monthly", "Rare", "Silent"]);
    }

    #[test]
    fn auto_clears_and_the_others_force() {
        assert_eq!(CadenceArg::Auto.group(), None);
        assert_eq!(CadenceArg::Rare.group(), Some(CadenceGroup::Rare));
        assert_eq!(CadenceArg::Silent.group(), Some(CadenceGroup::Silent));
    }

    #[test]
    fn cadence_refusals_are_usage_errors_and_others_stay_fatal() {
        use ressac_core::Error;
        for refusal in [Error::UnknownFeed(99), Error::FeedBuried(2)] {
            let text = refusal.to_string();
            let e = cadence_error(refusal);
            assert_eq!(
                e.downcast_ref::<super::super::UsageError>()
                    .map(ToString::to_string),
                Some(text)
            );
        }
        let fatal = cadence_error(Error::Invalid("disk".into()));
        assert!(fatal.downcast_ref::<super::super::UsageError>().is_none());
    }
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p ressac --bin ressac commands::feed`
Expected: compile errors, `cannot find function cadence_text` (and `cadence_json`, `group_title`, `cadence_error`).

- [ ] **Step 4: Write the helpers**

Above the `tests` module of `commands/feed.rs`:

```rust
use ressac_core::model::CadenceGroup;
use ressac_core::store::CadenceRow;

use super::usage;

/// The sidebar's names for the five groups (spec §8).
pub fn group_title(group: CadenceGroup) -> &'static str {
    match group {
        CadenceGroup::Daily => "Daily",
        CadenceGroup::Weekly => "Weekly",
        CadenceGroup::Monthly => "Monthly",
        CadenceGroup::Rare => "Rare",
        CadenceGroup::Silent => "Silent",
    }
}

/// `#12 Daily (set by you)` or `#12 Weekly (auto)`.
pub fn cadence_text(row: &CadenceRow) -> String {
    let who = if row.is_override { "set by you" } else { "auto" };
    format!("#{} {} ({who})", row.feed_id, group_title(row.effective_group))
}

/// The `cadence` event: the effective group, the computed one, and whether it is forced.
pub fn cadence_json(row: &CadenceRow) -> serde_json::Value {
    serde_json::json!({
        "event": "cadence",
        "feed_id": row.feed_id,
        "group": row.effective_group.as_str(),
        "computed": row.computed_group.as_str(),
        "override": row.is_override,
    })
}

/// An unknown or buried feed is a usage error (exit 2); anything else stays fatal (exit 1).
fn cadence_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    match e {
        Error::UnknownFeed(_) | Error::FeedBuried(_) => usage(e.to_string()),
        other => other.into(),
    }
}
```

`UsageError` is private to `commands/mod.rs` but its parent module's items are visible to the child module `feed`, so `super::super::UsageError` resolves from `feed::tests`. If the compiler disagrees, use `crate::commands::UsageError`, which is `pub`.

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p ressac --bin ressac commands::feed`
Expected: `5 passed` (warnings about unused `pub fn` until Task 3b.2 are expected).

- [ ] **Step 6: Commit**

```bash
git add crates/ressac/src/commands/mod.rs crates/ressac/src/commands/feed.rs crates/ressac/src/cli.rs
git commit -m "CLI: text and JSON of a feed's rhythm group

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3b.2: `ressac feed cadence`, and its effect on the edition

**Files:**
- Modify: `crates/ressac/src/cli.rs`
- Modify: `crates/ressac/src/main.rs`
- Modify: `crates/ressac/src/commands/feed.rs`
- Modify: `crates/ressac/tests/cli.rs`

- [ ] **Step 1: Write the failing CLI test**

Append to `crates/ressac/tests/cli.rs`. It reuses `edition_home` (Task 3a.2): Rare #1 (page 110), Weekly #2 (111), Wire #3 (112). A `fetch` first gives every feed its health row and computed group; the edition built afterwards reuses those entries (they are in its first 24-hour window).

```rust
#[tokio::test]
async fn feed_cadence_forces_a_group_and_the_edition_follows() {
    let server = MockServer::start().await;
    let home = edition_home(&server).await;
    let fetched = ressac_async(home.path(), &["fetch"]).await;
    assert!(fetched.status.success(), "{fetched:?}");

    let forced = ressac_async(home.path(), &["feed", "cadence", "3", "weekly"]).await;
    assert!(forced.status.success(), "{forced:?}");
    assert_eq!(stdout(&forced), "#3 Weekly (set by you)\n");

    let auto = ressac_async(home.path(), &["--json", "feed", "cadence", "3", "auto"]).await;
    assert!(auto.status.success(), "{auto:?}");
    assert_eq!(
        json_lines(&auto),
        [json!({"event": "cadence", "feed_id": 3, "group": "daily",
                "computed": "daily", "override": false})]
    );

    let rare_to_daily =
        ressac_async(home.path(), &["--json", "feed", "cadence", "1", "daily"]).await;
    assert_eq!(
        json_lines(&rare_to_daily),
        [json!({"event": "cadence", "feed_id": 1, "group": "daily",
                "computed": "rare", "override": true})]
    );
    assert_eq!(
        stdout(&ressac_async(home.path(), &["feed", "cadence", "2", "auto"]).await),
        "#2 Weekly (auto)\n"
    );

    // Rare forced to Daily leaves the front page and joins the wire, capped to one entry.
    let built = ressac_async(home.path(), &["--json", "edition", "build"]).await;
    assert!(built.status.success(), "{built:?}");
    assert_eq!(
        json_lines(&built).last().unwrap(),
        &json!({"event": "edition", "id": 1, "entries": 4, "front": 0, "week": 2,
                "wire": 2, "overflow": 2, "unreachable": 0})
    );
    let shown = stdout(&ressac_async(home.path(), &["edition", "show"]).await);
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines[..lines.len() - 1],
        [
            "This Week",
            "1 111 Weekly 1d",
            "2 111 Weekly 6d",
            "The Wire",
            "3 110 Rare news",
            "4 112 Wire 1h",
            "+2 more in Wire",
        ]
    );

    for (args, message) in [
        (&["feed", "cadence", "99", "daily"][..], "No feed #99.\n"),
        (&["--json", "feed", "cadence", "99", "auto"], "No feed #99.\n"),
    ] {
        let out = ressac_async(home.path(), args).await;
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert_eq!(stderr(&out), message, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let buried = ressac_async(home.path(), &["garden", "decide", "2", "bury"]).await;
    assert!(buried.status.success(), "{buried:?}");
    let refused = ressac_async(home.path(), &["feed", "cadence", "2", "daily"]).await;
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert_eq!(stderr(&refused), "Feed #2 is buried.\n");
    assert!(refused.stdout.is_empty());
}
```

- [ ] **Step 2: Run the test to see it fail**

Run: `cargo test -p ressac --test cli feed_cadence_forces_a_group_and_the_edition_follows`
Expected: FAIL, `feed cadence 3 weekly` exits `2` with clap's `unrecognized subcommand 'feed'`.

- [ ] **Step 3: Add the subcommand**

In `crates/ressac/src/cli.rs`, add the variant at the end of `enum Command`:

```rust
    /// Change one feed's settings
    Feed {
        #[command(subcommand)]
        action: FeedCommand,
    },
```

In `crates/ressac/src/main.rs`, add the arm at the end of `match cli.command`:

```rust
        Command::Feed { action } => commands::feed::feed(&g, action, &mut out),
```

- [ ] **Step 4: Write the handler**

In `crates/ressac/src/commands/feed.rs`, extend the imports and add the handler above `group_title`:

```rust
use std::io::Write;
use std::process::ExitCode;

use ressac_core::model::CadenceGroup;
use ressac_core::store::CadenceRow;

use super::{Globals, open_with_feeds, usage};
use crate::cli::FeedCommand;

/// `ressac feed cadence ID GROUP`: one line, or one `cadence` event.
pub fn feed(g: &Globals, action: FeedCommand, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        FeedCommand::Cadence { feed_id, group } => {
            let row = app
                .set_cadence(feed_id, group.group())
                .map_err(cadence_error)?;
            if g.json {
                writeln!(out, "{}", cadence_json(&row))?;
            } else {
                writeln!(out, "{}", cadence_text(&row))?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
```

(The three `use` lines from Task 3b.1 are replaced by this block.)

- [ ] **Step 5: Run the test to see it pass**

Run: `cargo test -p ressac --test cli feed_cadence_forces_a_group_and_the_edition_follows`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ressac/src/cli.rs crates/ressac/src/main.rs crates/ressac/src/commands/feed.rs crates/ressac/tests/cli.rs
git commit -m "CLI: ressac feed cadence, forced or automatic rhythm group

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3b.3: Refusals of `feed`, and pages 104–109 from the command line

**Files:**
- Modify: `crates/ressac/tests/cli.rs`

As in Task 3a.4, the behaviour is already there (clap, `open_with_feeds`, `pages::set` with 14 reserved pages from phase 1d); these tests pin it and should pass on the first run.

- [ ] **Step 1: Extend the empty-store and usage tests**

In `commands_on_an_empty_store_hint_at_import`, add:

```rust
        &["feed", "cadence", "1", "daily"],
        &["--json", "feed", "cadence", "1", "auto"],
```

In `usage_errors_exit_2`, add:

```rust
        &["feed"],
        &["feed", "cadence", "1"],
        &["feed", "cadence", "1", "sometimes"],
        &["feed", "cadence", "x", "daily"],
```

- [ ] **Step 2: Add the database-error and page tests**

Append to `crates/ressac/tests/cli.rs`:

```rust
#[test]
fn feed_cadence_exits_1_on_a_database_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("db.sqlite"), b"this is not a database").unwrap();
    let out = ressac(home.path(), &["feed", "cadence", "1", "daily"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        stderr(&out).starts_with("error: cannot open database"),
        "{out:?}"
    );
}

#[test]
fn page_commands_know_the_journal_and_rhythm_pages() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);

    let list = stdout(&run(&["page", "list"]));
    let reserved: Vec<&str> = list.lines().take(14).collect();
    assert_eq!(
        reserved,
        [
            "100 reserved - Index",
            "101 reserved - Unread",
            "102 reserved - Today",
            "103 reserved - Starred",
            "104 reserved - Journal",
            "105 reserved - Daily",
            "106 reserved - Weekly",
            "107 reserved - Monthly",
            "108 reserved - Rare",
            "109 reserved - Silent",
            "800 reserved - Needs triage",
            "801 reserved - All feeds",
            "802 reserved - Cemetery",
            "899 reserved - Guide",
        ]
    );
    let json = json_lines(&run(&["--json", "page", "list"]));
    assert_eq!(
        json[4],
        json!({"event": "page", "page": 104, "kind": "reserved", "id": null,
               "title": "Journal", "parent_page": null})
    );

    for page in 104..=109 {
        for kind in ["feed", "folder"] {
            let page_arg = page.to_string();
            let out = run(&["page", "set", kind, "2", &page_arg]);
            assert_eq!(out.status.code(), Some(2), "{kind} {page}: {out:?}");
            assert_eq!(stderr(&out), format!("Page {page} is reserved.\n"));
            assert!(out.stdout.is_empty());
        }
    }
    assert!(
        stdout(&run(&["page", "list"])).contains("203 feed 2 Orange Drenka\n"),
        "unchanged"
    );
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p ressac --test cli -- feed_cadence page_commands commands_on_an_empty_store usage_errors`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ressac/tests/cli.rs
git commit -m "CLI tests: feed cadence refusals, pages 104-109 reserved

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 3b

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: no diff, no warning, every test green. Then a manual look, on a throwaway database only (never `~/Library/...`):

```bash
TMP=$(mktemp -d)
HOME=$TMP cargo run -q -p ressac -- --db "$TMP/db.sqlite" import tests/fixtures/vienna-like.opml
HOME=$TMP cargo run -q -p ressac -- --db "$TMP/db.sqlite" edition --help
HOME=$TMP cargo run -q -p ressac -- --db "$TMP/db.sqlite" feed cadence --help
HOME=$TMP cargo run -q -p ressac -- --db "$TMP/db.sqlite" edition show
```

Expected: both help texts list the options of the contract table; `edition show` prints `No edition yet.` Do not run `edition build` here: the fixture's feeds point at real hosts.

Stop for Clément's approval before phase 4b (project rule).
