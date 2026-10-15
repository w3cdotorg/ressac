# Phase 2: Split `crates/ressac/src/main.rs` into modules

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 1. No file in common: Phase 1 touches `crates/ressac-core` only, and leaves the `ressac` crate alone, so `main.rs` is still the file of `855cd2c` (667 lines). Phase 3 builds on this split: its CLI fixes edit `commands/garden.rs`. At the end, run the verification gate and stop: wait for Clément's approval before Phase 3.

**Goal:** close the `main.rs` part of `TODO.md` « Reporté ▸ Antiope » (« `main.rs` à découper avant le sous-projet 2b »). Sub-project 2b will add reading commands to the CLI; today one 667-line file holds the clap definitions, eight command bodies inside one `run` function, their helpers and their tests. After this phase, `main.rs` only parses, sets up logging and Ctrl-C, and dispatches; the clap types live in `cli.rs`; each command is a function in `commands/`. **This is a pure move:** no behaviour, message or exit code changes. The moved functions are checked byte for byte against the original, and the crate's existing tests (34 unit tests, 17 CLI tests) are the safety net.

**Tech stack:** Rust 2024 (rust-version 1.99), clap 4.6.7 (derive), anyhow, tokio.

## Files (5)

- Modify: `crates/ressac/src/main.rs` (667 → 103 lines): `main`, `is_broken_pipe`, `run` (Ctrl-C, `Globals`, the dispatch), `init_logging`.
- Create: `crates/ressac/src/cli.rs` (219 lines): the clap types, their `From` conversions and the four parse tests.
- Create: `crates/ressac/src/commands/mod.rs` (230 lines): what every command shares (exit codes, `UsageError`, `Globals`, `open`, `open_with_feeds`), the one-shot commands `import`, `export`, `migrate`, `page`, and the `page_error` test.
- Create: `crates/ressac/src/commands/network.rs` (134 lines): `fetch`, `check`, `extract`, with `subset` and `interrupted`.
- Create: `crates/ressac/src/commands/garden.rs` (95 lines): `garden` (the TUI or one subcommand), `check_decision`, `garden_command`.

`output.rs` (436 lines), `tui/` and `tests/cli.rs` are not touched.

## Where everything goes

| Old `main.rs` lines | Item | New home |
|---|---|---|
| 1-24 | crate doc, `mod`, `use` | `main.rs` (rewritten imports) and each new file's own imports |
| 26-162 | `Cli`, `Command`, `GardenCommand`, `DecisionArg`, `PageCommand`, `PageKindArg`, both `From` impls | `cli.rs` 9-145, verbatim except `pub` on the six types and on `Cli`'s five fields |
| 164-191 | `INTERRUPTED`, `USAGE`, `NO_FEEDS`, `UsageError`, `usage` | `commands/mod.rs` 23-55, with `pub` on `INTERRUPTED`, `USAGE`, `UsageError`, `usage` |
| 193-219 | `main`, `is_broken_pipe` | `main.rs` 18-44, verbatim |
| 221-242 | `open`, `open_with_feeds` | `commands/mod.rs` 57-78, verbatim |
| 244-275 | `interrupted`, `subset` | `commands/network.rs` 18-49, verbatim |
| 277-340 | `check_decision`, `garden_command` | `commands/garden.rs` 32-95, verbatim |
| 342-382 | `page_error`, `page_command` | `commands/mod.rs` 162-202, verbatim |
| 384-552 | `run`: Ctrl-C setup, then one `match` arm per command | `main.rs` `run` keeps the setup and dispatches; each arm's body becomes a function: `commands::import`, `export`, `migrate`, `page`, `commands::network::{fetch, check, extract}`, `commands::garden::garden` |
| 554-570 | `init_logging` | `main.rs` 87-103, verbatim |
| 572-644 | tests `cli_definition_is_consistent`, `bare_garden_still_opens_the_tui`, `feed_is_repeatable_and_exclusive_with_folder`, `page_set_parses_its_kind_id_and_page` | `cli.rs` tests, verbatim |
| 645-667 | test `page_refusals_are_usage_errors_and_others_stay_fatal` | `commands/mod.rs` tests, verbatim |

**Design choices:**
- **`Globals`** (`db`, `json`, `ascii`, `stop`) carries what every command reads, so each handler takes `&Globals` plus its own arguments, instead of up to seven parameters.
- **Every handler returns `anyhow::Result<ExitCode>`.** `fetch` and `check` return the Ctrl-C code (130) themselves, as they did with `return Ok(interrupted(..))`; the others return `ExitCode::SUCCESS`, also after the empty-database hint. So `run` is one `match` expression.
- **`network`, not `fetch`,** for the module of `fetch`, `check` and `extract`: `extract` is no fetch, and a `fetch` module holding a `fetch` function reads badly.
- **No re-exports.** `main.rs` calls `commands::network::fetch` and `commands::garden::garden` by their paths.
- **`commands/mod.rs` does not import `ressac_core::garden`:** that name would clash with its `garden` submodule. `migrate` calls `ressac_core::garden::recompute_triage` by path.
- **`pub` inside private modules,** as `output.rs` already does. In a binary crate, an unused `pub` item still warns, so clippy keeps catching dead code.
- **Not renamed:** Phase 1's notes suggest renaming `allow_private_articles` and `RESSAC_ALLOW_PRIVATE_ARTICLES` "along with the split". That rename touches `ressac-core` and the CLI tests, beyond these five files, and a move is easier to review without it. It stays out of this phase.

**Builds on (unchanged):** `output::{sink, Sink, write_result, summary_text, import_text, migrated_text, page_text, decided_text, undo_text, apply_safe_text, extracted_text, interrupted_message}`, `tui::run(&Ressac, bool)`, and the `ressac_core` API the old `main.rs` used.

**Produces (for Phase 3):** `crates/ressac/src/commands/garden.rs` with `fn garden_command(app: &Ressac, action: GardenCommand, json: bool, out: &mut impl Write) -> anyhow::Result<()>` at lines 53-95, and `crate::commands::{usage, UsageError}`.

## Review focus

1. **Ctrl-C still works before any command runs.** The handler is spawned in `run` before the dispatch, and `fetch` and `check` read the same flag through `g.stop`. The code is moved, not changed; the verbatim check in Task 2.2 Step 7 covers `interrupted`, and `run` keeps the setup lines as they were.
2. **`check` still finishes the progress sink before it returns a refresh error.** The `match refreshed { Err(e) => { sink.finish(); return Err(e.into()); } }` block moves as is.
3. **`import --dry-run` without a database still creates none.** Pinned by `dry_run_import_reports_and_writes_nothing`.
4. **Usage errors still exit 2 with the bare message,** because `UsageError` is downcast in `main` and created in `commands`. Pinned by `usage_errors_exit_2`, `fetch_subsets_reject_unknown_ids_and_skip_buried_feeds` and `page_set_refusals_exit_2_and_write_nothing`.
5. **A closed stdout still exits 0.** Pinned by `closed_stdout_does_not_panic`.

---

### Task 2.1: Step 0, dead code in `main.rs`

`main.rs` is over 300 lines and is restructured, so the Step 0 rule applies: dead code goes first, in its own commit.

- [ ] **Step 1: Look for dead code, debug output and lint escapes**

```bash
cd /Users/willow/Sites/ressac
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
wc -l crates/ressac/src/main.rs
grep -nE 'dbg!|(^|[^e])println!|todo!|unimplemented!|#\[allow|#!\[allow' crates/ressac/src/main.rs
grep -n 'eprintln!' crates/ressac/src/main.rs
cargo clippy -p ressac --all-targets -- -D warnings
```

Expected (as at `855cd2c`): `667 crates/ressac/src/main.rs`; the first `grep` prints nothing; the second prints four lines (202, 206, 240, 247), which are user-facing messages (the usage error, the fatal error, the first-run hint, the Ctrl-C summary), not debug logs; clippy ends with `Finished` and no warning. In a binary crate rustc already reports every unused function, import, constant and field, so nothing is dead.

- [ ] **Step 2: Decide**

If the output matches, there is nothing to remove: write "Step 0: nothing to remove" in the phase report, make no commit, and go to Task 2.2. If `main.rs` changed since `855cd2c` and something turns up, remove it, run `cargo test -p ressac`, and commit it alone before Task 2.2:

```bash
git add crates/ressac/src/main.rs
git commit -F - <<'EOF'
CLI: remove dead code from main.rs before the split

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

In that case the line numbers below are off: match on the quoted texts.

---

### Task 2.2: Move the clap types and the command handlers out of `main.rs`

**Files:**
- Create: `crates/ressac/src/cli.rs`
- Create: `crates/ressac/src/commands/mod.rs`
- Create: `crates/ressac/src/commands/network.rs`
- Create: `crates/ressac/src/commands/garden.rs`
- Modify: `crates/ressac/src/main.rs` (replaced whole)

**Interfaces:**
- Consumes: the old `main.rs` items, unchanged.
- Produces: `cli::{Cli, Command, GardenCommand, DecisionArg, PageCommand, PageKindArg}`; `commands::{Globals, INTERRUPTED, USAGE, UsageError, usage, import, export, migrate, page}`; `commands::network::{fetch, check, extract}`; `commands::garden::garden`.

This task moves code; it adds no test. The test-first steps become: run the existing tests before the move (Step 1), check the move is verbatim (Step 7), and run the same tests after it (Step 8).

- [ ] **Step 1: Record the baseline**

```bash
cargo test -p ressac 2>&1 | grep -E '^test result'
```

Expected: `test result: ok. 34 passed` (unit tests of `src/main.rs`), then `test result: ok. 17 passed` (`tests/cli.rs`).

- [ ] **Step 2: Create `crates/ressac/src/cli.rs`**

Lines 9-145 are old `main.rs` lines 26-162 with `pub` added; the tests are old lines 577-643.

```rust
//! The command line: clap's definitions and their conversions to core types.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use ressac_core::model::{Decision, FeedId, FolderId};
use ressac_core::store::pages::PageKind;

#[derive(Parser)]
#[command(
    name = "ressac",
    version,
    about = "Tidy your RSS subscriptions, then read them calmly"
)]
pub struct Cli {
    /// Database file [default: ~/Library/Application Support/Ressac/ressac.sqlite]
    #[arg(long, env = "RESSAC_DB", global = true)]
    pub db: Option<PathBuf>,
    /// Print one JSON object per line instead of human-readable output
    #[arg(long, global = true)]
    pub json: bool,
    /// Use ASCII status glyphs instead of emoji
    #[arg(long, global = true)]
    pub ascii: bool,
    /// Also write logs to stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
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
    /// List the teletext page numbers, or give a folder or a feed another one
    Page {
        #[command(subcommand)]
        action: PageCommand,
    },
}

#[derive(Subcommand)]
pub enum GardenCommand {
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
pub enum DecisionArg {
    Keep,
    Repair,
    Bury,
}

#[derive(Subcommand)]
pub enum PageCommand {
    /// Every page, reserved ones first: page, kind, id, title
    List,
    /// Give a folder or a feed another page (110 to 799, not reserved, not taken)
    Set {
        #[arg(value_enum)]
        kind: PageKindArg,
        /// The folder's or the feed's id
        id: i64,
        page: u16,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PageKindArg {
    Feed,
    Folder,
}

impl From<PageKindArg> for PageKind {
    fn from(k: PageKindArg) -> Self {
        match k {
            PageKindArg::Feed => PageKind::Feed,
            PageKindArg::Folder => PageKind::Folder,
        }
    }
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
        let decide =
            Cli::try_parse_from(["ressac", "--json", "garden", "decide", "3", "bury"]).unwrap();
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

    #[test]
    fn page_set_parses_its_kind_id_and_page() {
        let set = Cli::try_parse_from(["ressac", "page", "set", "folder", "4", "450"]).unwrap();
        assert!(matches!(
            set.command,
            Command::Page {
                action: PageCommand::Set {
                    kind: PageKindArg::Folder,
                    id: 4,
                    page: 450
                }
            }
        ));
        for bad in [
            &["ressac", "page"][..],
            &["ressac", "page", "set", "entry", "1", "300"],
            &["ressac", "page", "set", "feed", "1", "70000"],
            &["ressac", "page", "set", "feed", "1", "-3"],
        ] {
            assert!(Cli::try_parse_from(bad).is_err(), "{bad:?}");
        }
    }
}
```

- [ ] **Step 3: Create `crates/ressac/src/commands/mod.rs`**

`import`, `export` and `migrate` are the bodies of the old `run` arms (old lines 404-435, 521-531, 532-543), with `cli.json` → `g.json` and `db` → `g.db`. `page` is the old arm at lines 544-549.

```rust
//! The command handlers. Each one runs a subcommand and returns the process's exit code;
//! `main` only parses the command line and dispatches here.

pub mod garden;
pub mod network;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::Context;
use ressac_core::opml;
use ressac_core::service::{Event, PRODUCTION_WAYBACK, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;
use ressac_core::store::pages::PageKind;

use crate::cli::PageCommand;
use crate::output;

/// Exit code of a run stopped by Ctrl-C (128 + SIGINT), as shells expect.
pub const INTERRUPTED: u8 = 130;

/// Exit code of a usage error: clap's own, and requests on ids the database cannot satisfy.
pub const USAGE: u8 = 2;

/// Printed instead of running a command that needs feeds on an empty database.
const NO_FEEDS: &str = "No feeds yet: run \"ressac import <file.opml>\" first.";

/// What every command reads: the database path, the global flags and the run's Ctrl-C flag.
pub struct Globals {
    pub db: PathBuf,
    pub json: bool,
    pub ascii: bool,
    pub stop: Arc<AtomicBool>,
}

/// A request the database cannot satisfy (unknown id, nothing to repair). It exits 2 like
/// clap's usage errors, with the bare message on stderr.
#[derive(Debug)]
pub struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

pub fn usage(message: impl Into<String>) -> anyhow::Error {
    UsageError(message.into()).into()
}

fn open(db: &Path) -> anyhow::Result<Ressac> {
    let app = Ressac::open(db).with_context(|| format!("cannot open database {}", db.display()))?;
    // Test hook: the CLI tests serve article pages from a local mock server. Debug builds
    // only: a release binary never lifts the private-address guard.
    if cfg!(debug_assertions) && std::env::var_os("RESSAC_ALLOW_PRIVATE_ARTICLES").is_some() {
        return Ok(app.allow_private_articles()?);
    }
    Ok(app)
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

/// `ressac import`: reads and parses the file before touching the database, so a bad
/// input never creates one.
pub fn import(
    g: &Globals,
    file: &Path,
    dry_run: bool,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let xml =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    opml::parse(&xml)?;
    let app = if dry_run && !g.db.exists() {
        // Nothing to compare against and nothing may be written: use a throwaway database.
        Ressac::with_parts(
            Store::open_in_memory()?,
            FetchLimits::default(),
            PRODUCTION_WAYBACK,
        )?
    } else {
        open(&g.db)?
    };
    let report = app.import_opml(&xml, dry_run)?;
    if g.json {
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
    Ok(ExitCode::SUCCESS)
}

/// `ressac export`: the active feeds, or the cemetery, as OPML on stdout or in a file.
pub fn export(
    g: &Globals,
    output: Option<PathBuf>,
    cemetery: bool,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let xml = opml::export(app.store(), cemetery)?;
    match output {
        Some(path) => std::fs::write(&path, xml)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => write!(out, "{xml}")?,
    }
    Ok(ExitCode::SUCCESS)
}

/// `ressac migrate`: opening runs the pending migrations (with a backup of the old file).
pub fn migrate(g: &Globals, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let app = open(&g.db)?;
    // A v1 → v2 migration leaves every `needs_triage` at 0 until the next
    // diagnosis or decision: compute the flags now, so the app's Garden is right.
    ressac_core::garden::recompute_triage(app.store())?;
    let event = Event::Finished {
        summary: app.summary()?,
    };
    let text = output::migrated_text(app.store().schema_version()?);
    output::write_result(out, g.json, &event, &text)?;
    Ok(ExitCode::SUCCESS)
}

/// `ressac page list | set`.
pub fn page(g: &Globals, action: PageCommand, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    page_command(&app, action, g.json, out)?;
    Ok(ExitCode::SUCCESS)
}

/// The refusals of `page set` (reserved, out of range, taken, unknown id) are usage errors
/// (exit 2); any other error stays fatal (exit 1).
fn page_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    match e {
        Error::PageReserved(_)
        | Error::PageOutOfRange(_)
        | Error::PageTaken { .. }
        | Error::UnknownPageTarget { .. } => usage(e.to_string()),
        other => other.into(),
    }
}

/// `ressac page list | set`: one line (or one `page` event) per page.
fn page_command(
    app: &Ressac,
    action: PageCommand,
    json: bool,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    match action {
        PageCommand::List => {
            for row in app.pages()? {
                let text = output::page_text(&row);
                output::write_result(out, json, &Event::Page(row), &text)?;
            }
        }
        PageCommand::Set { kind, id, page } => {
            let kind = PageKind::from(kind);
            app.set_page(kind, id, page).map_err(page_error)?;
            let row = app
                .pages()?
                .into_iter()
                .find(|r| r.kind == kind && r.id == Some(id))
                .context("the renumbered page is missing from v_page")?;
            let text = output::page_text(&row);
            output::write_result(out, json, &Event::Page(row), &text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_refusals_are_usage_errors_and_others_stay_fatal() {
        use ressac_core::Error;
        for refusal in [
            Error::PageReserved(800),
            Error::PageOutOfRange(900),
            Error::PageTaken { page: 201 },
            Error::UnknownPageTarget {
                kind: "feed",
                id: 99,
            },
        ] {
            let text = refusal.to_string();
            let e = page_error(refusal);
            assert_eq!(
                e.downcast_ref::<UsageError>().map(ToString::to_string),
                Some(text)
            );
        }
        let fatal = page_error(Error::Invalid("disk".into()));
        assert!(fatal.downcast_ref::<UsageError>().is_none());
    }
}
```

- [ ] **Step 4: Create `crates/ressac/src/commands/network.rs`**

`fetch`, `check` and `extract` are the old arms at lines 436-458, 459-489 and 490-511, with `cli.json` → `g.json`, `cli.ascii` → `g.ascii`, `stop` → `g.stop`, and `return Ok(interrupted(&stop, &sink))` → `Ok(interrupted(&g.stop, &sink))`.

```rust
//! The commands that go to the network: `fetch`, `check` and `extract`.

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use ressac_core::model::{FeedId, FolderId};
use ressac_core::service::{ExtractOutcome, NullSink, RefreshOptions};
use ressac_core::store::Store;

use super::{Globals, INTERRUPTED, open_with_feeds, usage};
use crate::output;

/// `ressac extract` on an entry without a link (spec Review Focus 1).
const NO_LINK: &str = "This entry has no link.";

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

/// `ressac fetch`: every due feed, or only those named by `--feed` or `--folder`.
pub async fn fetch(
    g: &Globals,
    force: bool,
    feeds: Vec<FeedId>,
    folder: Option<FolderId>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let only = subset(app.store(), feeds, folder)?;
    // A manual refresh of some feeds ignores the 30-minute freshness rule (spec §2).
    let opts = RefreshOptions {
        force: force || only.is_some(),
        only,
    };
    let sink = output::sink(g.json);
    let result = app.refresh(opts, &sink, g.stop.clone()).await;
    sink.finish();
    let summary = result?;
    if !g.json {
        writeln!(out, "{}", output::summary_text(&summary, g.ascii))?;
    }
    Ok(interrupted(&g.stop, &sink))
}

/// `ressac check`: a refresh, then the Wayback lookups, then the health report.
pub async fn check(g: &Globals, force: bool, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let sink = output::sink(g.json);
    let refreshed = app
        .refresh(RefreshOptions { force, only: None }, &sink, g.stop.clone())
        .await;
    let refreshed = match refreshed {
        Ok(s) => s,
        Err(e) => {
            sink.finish();
            return Err(e.into());
        }
    };
    let looked_up = app.lookup_wayback(&sink, g.stop.clone()).await;
    sink.finish();
    looked_up?;
    let mut summary = app.summary()?;
    summary.network_unreachable |= refreshed.network_unreachable;
    if g.json {
        writeln!(
            out,
            "{}",
            serde_json::json!({ "event": "report", "summary": summary })
        )?;
    } else {
        writeln!(out, "{}", output::summary_text(&summary, g.ascii))?;
    }
    Ok(interrupted(&g.stop, &sink))
}

/// `ressac extract`: one entry's full article. With `--json` the sink prints the
/// `extracted` line; otherwise the text below says it.
pub async fn extract(g: &Globals, entry_id: i64, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let outcome = if g.json {
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
    if !g.json {
        writeln!(out, "{text}")?;
    }
    Ok(ExitCode::SUCCESS)
}
```

- [ ] **Step 5: Create `crates/ressac/src/commands/garden.rs`**

`garden` is the old arm at lines 512-520.

```rust
//! `ressac garden`: the full-screen garden, or one decision without it.

use std::io::Write;
use std::process::ExitCode;

use chrono::Utc;
use ressac_core::garden;
use ressac_core::model::{Decision, FeedId};
use ressac_core::service::{Event, Ressac};
use ressac_core::store::Store;

use super::{Globals, open_with_feeds, usage};
use crate::cli::GardenCommand;
use crate::{output, tui};

/// `ressac garden` alone opens the TUI; with a subcommand it prints one result line.
pub fn garden(
    g: &Globals,
    action: Option<GardenCommand>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        None => tui::run(&app, g.ascii)?,
        Some(action) => garden_command(&app, action, g.json, out)?,
    }
    Ok(ExitCode::SUCCESS)
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
```

- [ ] **Step 6: Replace `crates/ressac/src/main.rs` whole**

```rust
//! `ressac`: tidy your RSS subscriptions, then read them calmly.

mod cli;
mod commands;
mod output;
mod tui;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;

use cli::{Cli, Command};
use commands::{Globals, INTERRUPTED, USAGE, UsageError};

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
    let g = Globals {
        db,
        json: cli.json,
        ascii: cli.ascii,
        stop,
    };
    let mut out = std::io::stdout().lock();
    match cli.command {
        Command::Import { file, dry_run } => commands::import(&g, &file, dry_run, &mut out),
        Command::Fetch {
            force,
            feeds,
            folder,
        } => commands::network::fetch(&g, force, feeds, folder, &mut out).await,
        Command::Check { force } => commands::network::check(&g, force, &mut out).await,
        Command::Extract { entry_id } => commands::network::extract(&g, entry_id, &mut out).await,
        Command::Garden { action } => commands::garden::garden(&g, action, &mut out),
        Command::Export { output, cemetery } => commands::export(&g, output, cemetery, &mut out),
        Command::Migrate => commands::migrate(&g, &mut out),
        Command::Page { action } => commands::page(&g, action, &mut out),
    }
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
```

- [ ] **Step 7: Check that the moved code is verbatim**

```bash
git show HEAD:crates/ressac/src/main.rs > "$TMPDIR/main.orig.rs"
O="$TMPDIR/main.orig.rs"
diff <(sed -n 26,162p "$O") <(sed -n 9,145p crates/ressac/src/cli.rs | sed -E 's/^pub (struct|enum) /\1 /; s/^    pub (db|json|ascii|verbose|command):/    \1:/') && echo cli-ok
diff <(sed -n 577,643p "$O") <(sed -n 152,218p crates/ressac/src/cli.rs) && echo cli-tests-ok
diff <(sed -n 342,382p "$O") <(sed -n 162,202p crates/ressac/src/commands/mod.rs) && echo page-ok
diff <(sed -n 645,666p "$O") <(sed -n 208,229p crates/ressac/src/commands/mod.rs) && echo page-test-ok
diff <(sed -n 244,275p "$O") <(sed -n 18,49p crates/ressac/src/commands/network.rs) && echo network-ok
diff <(sed -n 277,340p "$O") <(sed -n 32,95p crates/ressac/src/commands/garden.rs) && echo garden-ok
diff <(sed -n 193,219p "$O") <(sed -n 18,44p crates/ressac/src/main.rs) && echo main-ok
diff <(sed -n 554,570p "$O") <(sed -n 87,103p crates/ressac/src/main.rs) && echo logging-ok
```

Expected: the eight `-ok` lines and no diff output. A diff means a typo in the copy: fix the new file, not the check.

- [ ] **Step 8: Run the gate**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
```

Expected: `fmt` prints nothing; clippy ends with `Finished` and no warning; every `test result:` line says `0 failed`, with the same counts as after Phase 1: `ressac` unit tests 34 (now `cli::tests` 4, `commands::tests` 1, `output` and `tui` 29), `cli.rs` 17, the core lib 255 passed and 1 ignored, `article_proxy` 1, `contract` 8, `end_to_end` 4.

- [ ] **Step 9: Commit**

```bash
git add crates/ressac/src/main.rs crates/ressac/src/cli.rs crates/ressac/src/commands
git commit -F - <<'EOF'
CLI: split main.rs into cli.rs and commands/ before sub-project 2b

main.rs keeps parsing, logging, Ctrl-C and the dispatch. The clap types
move to cli.rs; each command becomes a function in commands/ (mod.rs for
import, export, migrate and page, network.rs for fetch, check and
extract, garden.rs for the garden). Pure move: no behaviour change.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Step 10: Stop.** Report the gate output (and the Step 0 result) and wait for Clément's approval before Phase 3.

---

## Notes for the executor

- **Validated in a scratch copy.** The five files above were written in a copy of `855cd2c`; the eight verbatim checks print `-ok`, and the Rust gate passed (fmt, clippy `-D warnings`, `cargo test --workspace`: `ressac` 34 + 17, unchanged). Phase 1's changes do not touch these files.
- **The line numbers in the table are those of `855cd2c`,** which Phase 1 does not change for `main.rs`.
- **Out of scope:** `output.rs` (436 lines) stays as it is; it is not restructured here, so Step 0 does not apply to it. Splitting it is for sub-project 2b if its reading commands need new output.
- **`TODO.md`:** the `main.rs` mention in « Reporté ▸ Antiope » (the last bullet) is updated in the plan's final docs phase, with the other items of that bullet.
