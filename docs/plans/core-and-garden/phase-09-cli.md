# Phase 9: The `ressac` binary (import, fetch, check, export)

Read the plan index first. Global Constraints apply. Spec §3 (commands, global flags), §6 (output), §8 (exit codes, logging).

**Files (5):**
- Modify: `Cargo.toml` (workspace members → `["crates/ressac-core", "crates/ressac"]`)
- Create: `crates/ressac/Cargo.toml`
- Create: `crates/ressac/src/main.rs`
- Create: `crates/ressac/src/output.rs` (tests inline)
- Create: `crates/ressac/tests/cli.rs`

### Task 9.1: Output helpers

**Interfaces:**
- Consumes: `ressac_core::model::Status`, `ressac_core::service::{Event, EventSink, Summary}`, `ressac_core::store::ImportReport`.
- Produces (`output` module of the binary):
  - `glyph(status: Status, ascii: bool) -> &'static str`
  - `enum Sink { Progress(ProgressBar), Json }` implementing `EventSink`
  - `sink(json: bool) -> Sink`, `Sink::finish(&self)`
  - `import_text(&ImportReport, dry_run: bool) -> String`
  - `summary_text(&Summary, ascii: bool) -> String`

- [ ] **Step 1: Create** `crates/ressac/Cargo.toml` and add the crate to the workspace members:

```toml
[package]
name = "ressac"
description = "Tidy your RSS subscriptions, then read them calmly"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
clap.workspace = true
indicatif.workspace = true
open.workspace = true
ratatui.workspace = true
ressac-core = { path = "../ressac-core" }
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-appender.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
insta.workspace = true
tempfile.workspace = true
tokio.workspace = true
wiremock.workspace = true
```

- [ ] **Step 2: Write the failing tests** in `crates/ressac/src/output.rs`

```rust
//! Human and JSON output for the CLI commands.

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn glyphs_have_an_ascii_fallback() {
        assert_eq!(glyph(Status::Dead, false), "🪦");
        assert_eq!(glyph(Status::Dead, true), "x");
        for s in Status::ALL {
            assert_eq!(glyph(s, true).chars().count(), 1);
        }
    }

    #[test]
    fn import_text_mentions_counts_merges_and_dry_run() {
        let report = ImportReport {
            folders_added: 7,
            feeds_added: 11,
            merged: vec![("Daring Fireball (1)".into(), "Daring Fireball".into())],
            skipped_existing: 0,
        };
        let text = import_text(&report, true);
        assert!(text.starts_with("Dry run: nothing written."));
        assert!(text.contains("Imported 11 feeds in 7 new folders."));
        assert!(text.contains("\"Daring Fireball (1)\" → \"Daring Fireball\""));
    }

    #[test]
    fn summary_text_lists_statuses_repairs_and_next_step() {
        let summary = Summary {
            total: 6,
            counts: BTreeMap::from([("Alive".into(), 3), ("Dead".into(), 2), ("Moved".into(), 1)]),
            https_repairs: 2,
            redirect_repairs: 1,
            network_unreachable: false,
        };
        let text = summary_text(&summary, true);
        assert!(text.contains("+ Alive"), "{text}");
        assert!(text.contains("x Dead"), "{text}");
        assert!(text.contains("3 safe repairs available (2 × https, 1 × redirect)"), "{text}");
        assert!(text.contains("run `ressac garden` to triage"), "{text}");
        assert!(!text.contains("Unchecked"), "zero Unchecked is hidden");
    }
}
```

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use indicatif::{ProgressBar, ProgressStyle};
use ressac_core::model::Status;
use ressac_core::service::{Event, EventSink, Summary};
use ressac_core::store::ImportReport;

/// Order of the `check` summary grid (three per line).
const SUMMARY_ORDER: [Status; 9] = [
    Status::Alive,
    Status::Fossil,
    Status::Moved,
    Status::Dormant,
    Status::Ailing,
    Status::Dead,
    Status::Hijacked,
    Status::Duplicate,
    Status::Unchecked,
];

pub fn glyph(status: Status, ascii: bool) -> &'static str {
    match (status, ascii) {
        (Status::Duplicate, false) => "👯",
        (Status::Hijacked, false) => "🦠",
        (Status::Moved, false) => "🚚",
        (Status::Dead, false) => "🪦",
        (Status::Ailing, false) => "🤒",
        (Status::Fossil, false) => "🦴",
        (Status::Dormant, false) => "🌙",
        (Status::Alive, false) => "🌿",
        (Status::Unchecked, false) => "❔",
        (Status::Duplicate, true) => "=",
        (Status::Hijacked, true) => "!",
        (Status::Moved, true) => ">",
        (Status::Dead, true) => "x",
        (Status::Ailing, true) => "~",
        (Status::Fossil, true) => "#",
        (Status::Dormant, true) => "z",
        (Status::Alive, true) => "+",
        (Status::Unchecked, true) => "?",
    }
}

pub enum Sink {
    /// Progress bar on stderr (hidden automatically when stderr is not a terminal).
    Progress(ProgressBar),
    /// One JSON object per line on stdout.
    Json,
}

pub fn sink(json: bool) -> Sink {
    if json {
        return Sink::Json;
    }
    let bar = ProgressBar::new(0);
    let style = ProgressStyle::with_template("Checking {pos}/{len} feeds {bar:20} {elapsed}  {wide_msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    bar.set_style(style);
    Sink::Progress(bar)
}

impl Sink {
    pub fn finish(&self) {
        if let Sink::Progress(bar) = self {
            bar.finish_and_clear();
        }
    }
}

impl EventSink for Sink {
    fn emit(&self, event: &Event) {
        match self {
            Sink::Json => println!("{}", serde_json::to_string(event).unwrap_or_default()),
            Sink::Progress(bar) => match event {
                Event::Started { total } => bar.set_length(*total as u64),
                Event::FeedDone { title, .. } => {
                    bar.inc(1);
                    bar.set_message(title.clone());
                }
                Event::NetworkUnreachable { failed, attempted } => bar.println(format!(
                    "Network looks unreachable ({failed}/{attempted} feeds could not connect) — results not recorded as failures."
                )),
                Event::Probing { what, total } => bar.set_message(format!("probing {what} for {total} feeds…")),
                Event::WaybackDone { .. } | Event::Finished { .. } => {}
            },
        }
    }
}

pub fn import_text(r: &ImportReport, dry_run: bool) -> String {
    let mut out = String::new();
    if dry_run {
        out.push_str("Dry run: nothing written.\n");
    }
    out.push_str(&format!("Imported {} feeds in {} new folders.", r.feeds_added, r.folders_added));
    if !r.merged.is_empty() {
        out.push_str(&format!("\nMerged {} duplicate(s):", r.merged.len()));
        for (dropped, kept) in &r.merged {
            out.push_str(&format!("\n  \"{dropped}\" → \"{kept}\""));
        }
    }
    if r.skipped_existing > 0 {
        out.push_str(&format!("\n{} feed(s) already present.", r.skipped_existing));
    }
    out
}

pub fn summary_text(s: &Summary, ascii: bool) -> String {
    let cells: Vec<String> = SUMMARY_ORDER
        .iter()
        .filter(|st| **st != Status::Unchecked || s.count(**st) > 0)
        .map(|st| format!("{} {:<10}{:>4}", glyph(*st, ascii), st.as_str(), s.count(*st)))
        .collect();
    let mut out = String::new();
    for row in cells.chunks(3) {
        out.push_str("  ");
        out.push_str(&row.join("    "));
        out.push('\n');
    }
    if s.network_unreachable {
        out.push_str("\n  Network looked unreachable: failures were not counted.\n");
    }
    if s.safe_repairs() > 0 {
        out.push_str(&format!(
            "\n  {} safe repairs available ({} × https, {} × redirect)\n",
            s.safe_repairs(),
            s.https_repairs,
            s.redirect_repairs
        ));
    }
    out.push_str("  → run `ressac garden` to triage");
    out
}
```

- [ ] **Step 4:** Create a minimal `main.rs` containing `mod output; fn main() {}` so the crate compiles. Run `cargo test -p ressac output` → `3 passed`.

- [ ] **Step 5: Commit** ("ressac binary: output helpers", both attribution lines).

### Task 9.2: Commands, logging, exit codes

**Interfaces:**
- Consumes: `ressac_core::service::{Ressac, RefreshOptions}`, `ressac_core::opml::export`, `ressac_core::store::default_path`, `output::*`.
- Produces: the CLI.
  - Global flags: `--db` (env `RESSAC_DB`), `--json`, `--ascii`, `-v/--verbose`.
  - Subcommands: `import <file> [--dry-run]`, `fetch [--force]`, `check [--force]`, `export [-o file] [--cemetery]`.
  - Exit codes: 0 success, 1 fatal (`error: …` on stderr), 2 usage.

- [ ] **Step 1: Write the failing CLI tests** `crates/ressac/tests/cli.rs`

```rust
//! Runs the real binary.

use std::path::Path;
use std::process::{Command, Output};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/vienna-like.opml");

fn ressac(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ressac"))
        .args(["--db", home.join("db.sqlite").to_str().unwrap()])
        .args(args)
        .env("HOME", home)
        .output()
        .expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn dry_run_import_reports_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let out = ressac(home.path(), &["import", FIXTURE, "--dry-run"]);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("Imported 11 feeds in 7 new folders."));
    let export = ressac(home.path(), &["export"]);
    assert!(!stdout(&export).contains("xkcd"));
}

#[test]
fn import_then_export_round_trips() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let out_file = home.path().join("clean.opml");
    let export = ressac(home.path(), &["export", "-o", out_file.to_str().unwrap()]);
    assert!(export.status.success(), "{export:?}");
    let xml = std::fs::read_to_string(out_file).unwrap();
    assert!(xml.contains("<opml version=\"2.0\">"));
    assert!(xml.contains("xkcd.com"));
}

#[test]
fn json_import_is_machine_readable() {
    let home = tempfile::tempdir().unwrap();
    let out = ressac(home.path(), &["--json", "import", FIXTURE]);
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["feeds_added"], 11);
}

#[test]
fn fatal_errors_exit_1_with_a_message() {
    let home = tempfile::tempdir().unwrap();
    let missing = ressac(home.path(), &["import", "/nonexistent.opml"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).starts_with("error: cannot read /nonexistent.opml"));

    let bad = home.path().join("bad.opml");
    std::fs::write(&bad, "<opml>\n<body>\n<outline text=\"x\">\n</body></opml>").unwrap();
    let malformed = ressac(home.path(), &["import", bad.to_str().unwrap()]);
    assert_eq!(malformed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("cannot read OPML at line"));
}

#[test]
fn usage_errors_exit_2() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(ressac(home.path(), &["frobnicate"]).status.code(), Some(2));
}

#[tokio::test]
async fn fetch_json_streams_events() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://e.org/</link><description>d</description></channel></rss>"#,
            "application/rss+xml",
        ))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("one.opml");
    std::fs::write(&opml, format!(r#"<opml><body><outline text="T" xmlUrl="{}/feed"/></body></opml>"#, server.uri())).unwrap();
    let home_path = home.path().to_path_buf();
    let out = tokio::task::spawn_blocking(move || {
        assert!(ressac(&home_path, &["import", opml.to_str().unwrap()]).status.success());
        ressac(&home_path, &["--json", "fetch"])
    })
    .await
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    let lines: Vec<serde_json::Value> = stdout(&out).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.first().unwrap()["event"], "started");
    assert_eq!(lines.last().unwrap()["event"], "finished");
    assert!(lines.iter().any(|l| l["event"] == "feed_done"));
}
```

- [ ] **Step 2: Run** `cargo test -p ressac --test cli`. Expected: failures (no subcommands yet).

- [ ] **Step 3: Implement** `crates/ressac/src/main.rs`

```rust
//! `ressac`: tidy your RSS subscriptions, then read them calmly.

mod output;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use clap::{Parser, Subcommand};
use ressac_core::opml;
use ressac_core::service::{RefreshOptions, Ressac};

#[derive(Parser)]
#[command(name = "ressac", version, about = "Tidy your RSS subscriptions, then read them calmly")]
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
    /// Fetch every feed and update its health
    Fetch {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
    },
    /// Fetch, look up archives of dead feeds, and print a health report
    Check {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
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
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let _log_guard = init_logging(cli.verbose);
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    if let Command::Import { file, .. } = &cli.command {
        // Read before opening the database so a typo does not create an empty one.
        std::fs::metadata(file).with_context(|| format!("cannot read {}", file.display()))?;
    }
    let db = cli.db.clone().unwrap_or_else(ressac_core::store::default_path);
    let app = Ressac::open(&db).with_context(|| format!("cannot open database {}", db.display()))?;
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                stop.store(true, Ordering::Relaxed);
            }
        });
    }
    match cli.command {
        Command::Import { file, dry_run } => {
            let xml = std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
            let report = app.import_opml(&xml, dry_run)?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "dry_run": dry_run,
                        "folders_added": report.folders_added,
                        "feeds_added": report.feeds_added,
                        "merged": report.merged,
                        "skipped_existing": report.skipped_existing,
                    })
                );
            } else {
                println!("{}", output::import_text(&report, dry_run));
            }
        }
        Command::Fetch { force } => {
            let sink = output::sink(cli.json);
            let summary = app.refresh(RefreshOptions { force }, &sink, stop).await?;
            sink.finish();
            if !cli.json {
                println!("{}", output::summary_text(&summary, cli.ascii));
            }
        }
        Command::Check { force } => {
            let sink = output::sink(cli.json);
            app.refresh(RefreshOptions { force }, &sink, stop.clone()).await?;
            app.lookup_wayback(&sink, stop).await?;
            sink.finish();
            let summary = app.summary()?;
            if cli.json {
                println!("{}", serde_json::json!({ "event": "report", "summary": summary }));
            } else {
                println!("{}", output::summary_text(&summary, cli.ascii));
            }
        }
        Command::Export { output, cemetery } => {
            let xml = opml::export(app.store(), cemetery)?;
            match output {
                Some(path) => std::fs::write(&path, xml).with_context(|| format!("cannot write {}", path.display()))?,
                None => print!("{xml}"),
            }
        }
    }
    Ok(())
}

/// Daily-rotated log file in ~/Library/Logs/Ressac, mirrored to stderr with `-v`.
fn init_logging(verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    let dir = PathBuf::from(std::env::var_os("HOME")?).join("Library/Logs/Ressac");
    std::fs::create_dir_all(&dir).ok()?;
    let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(dir, "ressac.log"));
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

The `ImportReport.merged` field is `Vec<(String, String)>`, which `serde_json::json!` serializes as arrays of two strings.

- [ ] **Step 4: Run.** `cargo test -p ressac` → `3` output tests + `6` CLI tests pass.

- [ ] **Step 5: Verification gate**, then commit and push ("ressac CLI: import, fetch, check, export with logging and exit codes", both attribution lines).
