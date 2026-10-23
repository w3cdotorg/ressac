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
    // No usable HOME and no database named: refuse rather than write relative to the working
    // directory. The tmux ticker (both forms) is decided here, before `run`, so that no ticker
    // path ever reaches a default database: the line form prints an empty line, `open` does
    // nothing, and both exit 0 silently since tmux calls them.
    if paths.is_none() && cli.db.is_none() {
        match &cli.command {
            Some(Command::Ticker { action, .. }) => {
                if action.is_none() {
                    println!();
                }
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("error: no home directory: set HOME or pass --db");
                return ExitCode::from(1);
            }
        }
    }
    let _log_guard = paths
        .as_ref()
        .and_then(|p| ressac_core::logging::init(&p.log_dir(), cli.verbose));
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

/// On Linux, a database left at the macOS location (copied over from a Mac) is never read
/// or moved; this names it, and the one in use, unless `db` is that very file.
fn legacy_database_warning(paths: &ressac_core::paths::Paths, db: &Path) -> Option<String> {
    let legacy = paths.legacy_macos_database()?;
    if let (Ok(a), Ok(b)) = (db.canonicalize(), legacy.canonicalize())
        && a == b
    {
        return None;
    }
    Some(format!(
        "An older Ressac database is still at {}, unused: this run uses {}. \
         Move it there, or pass --db, to keep using it.",
        legacy.display(),
        db.display()
    ))
}

/// The commands that read `Globals::stop`. Only these catch SIGTERM: the readers and the
/// garden TUI never look at the flag, so a handled SIGTERM would leave them running.
fn honours_stop(command: &Command) -> bool {
    matches!(
        command,
        Command::Add { .. }
            | Command::Fetch { .. }
            | Command::Check { .. }
            | Command::Edition { .. }
    )
}

/// Ctrl-C, and SIGTERM when asked for (Unix only), as one stream of interrupts.
struct Interrupts {
    #[cfg(unix)]
    term: Option<tokio::signal::unix::Signal>,
}

impl Interrupts {
    /// Installs the listeners now (before dispatch); `with_term` adds SIGTERM.
    fn new(with_term: bool) -> Self {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let term = if with_term {
                signal(SignalKind::terminate()).ok()
            } else {
                None
            };
            Self { term }
        }
        #[cfg(not(unix))]
        {
            let _ = with_term;
            Self {}
        }
    }

    /// Waits for the next interrupt; false when no listener could be installed.
    async fn next(&mut self) -> bool {
        #[cfg(unix)]
        if let Some(term) = self.term.as_mut() {
            return tokio::select! {
                r = tokio::signal::ctrl_c() => r.is_ok(),
                t = term.recv() => t.is_some(),
            };
        }
        tokio::signal::ctrl_c().await.is_ok()
    }
}

async fn run(cli: Cli, paths: Option<&ressac_core::paths::Paths>) -> anyhow::Result<ExitCode> {
    let db = cli
        .db
        .clone()
        .unwrap_or_else(ressac_core::store::default_path);
    if let Some(text) = paths.and_then(|p| legacy_database_warning(p, &db)) {
        // The tmux ticker runs every 15 s: it must not fill the log with this.
        if matches!(cli.command, Some(Command::Ticker { .. })) {
            tracing::debug!("{text}");
        } else {
            tracing::warn!("{text}");
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        // First Ctrl-C (or SIGTERM, for the commands that watch the flag): stop scheduling
        // and finish cleanly. Second one: leave now.
        let stop = stop.clone();
        let mut interrupts = Interrupts::new(cli.command.as_ref().is_some_and(honours_stop));
        tokio::spawn(async move {
            if interrupts.next().await {
                stop.store(true, Ordering::Relaxed);
                if interrupts.next().await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ressac_core::paths::{Layout, Paths};

    fn paths(layout: Layout, home: &Path) -> Paths {
        let home = home.as_os_str().to_owned();
        Paths::resolve(layout, &move |k: &str| (k == "HOME").then(|| home.clone())).unwrap()
    }

    fn make_legacy(home: &Path) -> PathBuf {
        let legacy = home.join("Library/Application Support/Ressac/ressac.sqlite");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"").unwrap();
        legacy
    }

    #[test]
    fn a_legacy_database_on_linux_is_named_with_the_one_in_use() {
        let home = tempfile::tempdir().unwrap();
        let paths = paths(Layout::Xdg, home.path());
        let db = paths.database();
        assert_eq!(legacy_database_warning(&paths, &db), None);
        let legacy = make_legacy(home.path());
        assert_eq!(
            legacy_database_warning(&paths, &db),
            Some(format!(
                "An older Ressac database is still at {}, unused: this run uses {}. \
                 Move it there, or pass --db, to keep using it.",
                legacy.display(),
                db.display()
            ))
        );
        assert_eq!(
            legacy_database_warning(&paths, &legacy),
            None,
            "--db on the old file"
        );
    }

    #[test]
    fn the_mac_layout_never_warns() {
        let home = tempfile::tempdir().unwrap();
        let paths = paths(Layout::MacOs, home.path());
        make_legacy(home.path());
        assert_eq!(
            legacy_database_warning(&paths, &home.path().join("db.sqlite")),
            None
        );
    }

    fn parse(args: &[&str]) -> Command {
        Cli::parse_from(std::iter::once("ressac").chain(args.iter().copied()))
            .command
            .unwrap()
    }

    #[test]
    fn only_the_commands_that_watch_the_stop_flag_catch_sigterm() {
        for args in [
            &["add", "http://e.org/feed"][..],
            &["fetch"],
            &["check"],
            &["edition", "build"],
        ] {
            assert!(honours_stop(&parse(args)), "{args:?}");
        }
        // The readers and the garden TUI die at once on SIGTERM, as before.
        for args in [
            &["read"][..],
            &["garden"],
            &["export"],
            &["import", "x.opml"],
        ] {
            assert!(!honours_stop(&parse(args)), "{args:?}");
        }
    }
}
