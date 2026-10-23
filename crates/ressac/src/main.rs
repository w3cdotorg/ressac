//! `ressac`: tidy your RSS subscriptions, then read them calmly.

mod cli;
mod commands;
mod output;
mod ticker;
mod tui;

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use clap::Parser;

use cli::{Cli, Command, TickerCommand};
use commands::{Globals, INTERRUPTED, USAGE, UsageError};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = ressac_core::paths::Paths::from_env().ok();
    let _log_guard = paths
        .as_ref()
        .and_then(|p| init_logging(&p.log_dir(), cli.verbose));
    match run(cli, paths.as_ref()).await {
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

async fn run(cli: Cli, paths: Option<&ressac_core::paths::Paths>) -> anyhow::Result<ExitCode> {
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
    let Some(command) = cli.command else {
        return commands::read(&g);
    };
    match command {
        Command::Import { file, dry_run } => commands::import(&g, &file, dry_run, &mut out),
        Command::Chart { action } => commands::chart::chart(&g, action, &mut out),
        Command::Add {
            address,
            theme,
            folder,
        } => {
            commands::network::add(&g, &address, theme.as_deref(), folder.as_deref(), &mut out)
                .await
        }
        Command::Fetch {
            force,
            feeds,
            folder,
        } => commands::network::fetch(&g, force, feeds, folder, &mut out).await,
        Command::Check { force } => commands::network::check(&g, force, &mut out).await,
        Command::Extract { entry_id } => commands::network::extract(&g, entry_id, &mut out).await,
        Command::Garden { action } => commands::garden::garden(&g, action, &mut out),
        Command::Read => commands::read(&g),
        Command::Export { output, cemetery } => commands::export(&g, output, cemetery, &mut out),
        Command::Migrate => commands::migrate(&g, &mut out),
        Command::Page { action } => commands::page(&g, action, &mut out),
        Command::Edition { action } => commands::edition::edition(&g, action, &mut out).await,
        Command::Feed { action } => commands::feed::feed(&g, action, &mut out),
        Command::Ticker {
            tmux,
            width,
            action: None,
        } => match paths {
            Some(p) => commands::ticker::show(
                &g,
                &p.ticker_state(&g.db),
                tmux,
                width,
                Utc::now(),
                &mut out,
            ),
            None => {
                // No usable HOME: nothing to show, like every other ticker failure.
                tracing::debug!("ticker: no state directory (no home), printing nothing");
                writeln!(out)?;
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Ticker {
            action: Some(TickerCommand::Open { token }),
            ..
        } => commands::ticker::open(&g, &token, Utc::now(), open_in_browser),
    }
}

/// Opens a URL in the default browser without waiting for the launcher (`xdg-open` may run
/// the browser in the foreground); `RESSAC_NO_BROWSER=1` only logs it (tests).
fn open_in_browser(url: &str) -> std::io::Result<()> {
    if std::env::var_os("RESSAC_NO_BROWSER").is_some_and(|v| v == "1") {
        tracing::info!("RESSAC_NO_BROWSER: not opening {url}");
        return Ok(());
    }
    open::that_detached(url)
}

/// Daily-rotated log file in `dir` (`Paths::log_dir`: `~/Library/Logs/Ressac` on macOS,
/// `$XDG_STATE_HOME/ressac/logs` on Linux), mirrored to stderr with `-v`.
fn init_logging(dir: &Path, verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    std::fs::create_dir_all(dir).ok()?;
    // `rolling::daily` panics when the directory is not writable; a status-bar helper must
    // never print that, so no file logging in that case.
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ressac.log")
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
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
