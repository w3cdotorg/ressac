//! The command handlers. Each one runs a subcommand and returns the process's exit code;
//! `main` only parses the command line and dispatches here.

pub mod edition;
pub mod feed;
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

pub(crate) fn open(db: &Path) -> anyhow::Result<Ressac> {
    let app = Ressac::open(db).with_context(|| format!("cannot open database {}", db.display()))?;
    // Test hook: the CLI tests serve article pages from a local mock server. Debug builds
    // only: a release binary never lifts the private-address guard.
    if cfg!(debug_assertions) && std::env::var_os("RESSAC_ALLOW_PRIVATE_ARTICLES").is_some() {
        return Ok(app.allow_private_articles()?);
    }
    Ok(app)
}

/// `ressac` and `ressac read`: the terminal reader on the database, until `q`. It is
/// interactive and has no machine-readable form, so `--json` is a usage error.
pub fn read(g: &Globals) -> anyhow::Result<ExitCode> {
    if g.json {
        return Err(usage(
            "The reader is interactive and has no --json output; use a command, e.g. \"ressac fetch --json\".",
        ));
    }
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    crate::tui::reader::run(
        &app,
        &g.db,
        &crate::tui::reader::settings::TerminalSettings::default_path(),
    )?;
    Ok(ExitCode::SUCCESS)
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
