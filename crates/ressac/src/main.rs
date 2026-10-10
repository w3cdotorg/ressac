//! `ressac`: tidy your RSS subscriptions, then read them calmly.

mod output;
mod tui;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use clap::{Parser, Subcommand};
use ressac_core::opml;
use ressac_core::service::{PRODUCTION_WAYBACK, RefreshOptions, Ressac};
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
    /// Triage feeds in a full-screen garden
    Garden,
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
        Err(e) if is_broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// True when an io `BrokenPipe` (closed stdout, e.g. `| head -1`) sits anywhere in the chain.
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let db = cli
        .db
        .clone()
        .unwrap_or_else(ressac_core::store::default_path);
    let open =
        || Ressac::open(&db).with_context(|| format!("cannot open database {}", db.display()));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                stop.store(true, Ordering::Relaxed);
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
                open()?
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
        Command::Fetch { force } => {
            let app = open()?;
            let sink = output::sink(cli.json);
            let result = app.refresh(RefreshOptions { force }, &sink, stop).await;
            sink.finish();
            let summary = result?;
            if !cli.json {
                writeln!(out, "{}", output::summary_text(&summary, cli.ascii))?;
            }
        }
        Command::Check { force } => {
            let app = open()?;
            let sink = output::sink(cli.json);
            let refreshed = app
                .refresh(RefreshOptions { force }, &sink, stop.clone())
                .await;
            let refreshed = match refreshed {
                Ok(s) => s,
                Err(e) => {
                    sink.finish();
                    return Err(e.into());
                }
            };
            let looked_up = app.lookup_wayback(&sink, stop).await;
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
        }
        Command::Garden => {
            let app = open()?;
            tui::run(&app, cli.ascii)?;
        }
        Command::Export { output, cemetery } => {
            let app = open()?;
            let xml = opml::export(app.store(), cemetery)?;
            match output {
                Some(path) => std::fs::write(&path, xml)
                    .with_context(|| format!("cannot write {}", path.display()))?,
                None => write!(out, "{xml}")?,
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
