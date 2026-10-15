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
            garden::recompute_triage(app.store())?;
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
}
