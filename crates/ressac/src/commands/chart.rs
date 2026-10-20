//! `ressac chart`: the sea chart's data for the apps, and the import and export of its themes.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use chrono::Utc;
use ressac_core::service::{PRODUCTION_WAYBACK, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::{ChartImport, ChartImportReport, Store};

use super::{Globals, open, usage};
use crate::cli::ChartCommand;
use crate::output;

pub fn chart(
    g: &Globals,
    action: Option<ChartCommand>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    match action {
        None => snapshot(g, out),
        Some(ChartCommand::Import { file, dry_run }) => import(g, &file, dry_run, out),
        Some(ChartCommand::Export { output }) => export(g, output.as_deref(), out),
    }
}

/// An app on a database that does not exist yet, and a dry run, get a throwaway one: a
/// read must not create files, and nothing may be written.
fn open_or_throwaway(g: &Globals, throwaway: bool) -> anyhow::Result<Ressac> {
    if throwaway {
        return Ok(Ressac::with_parts(
            Store::open_in_memory()?,
            FetchLimits::default(),
            PRODUCTION_WAYBACK,
        )?);
    }
    open(&g.db)
}

/// `ressac chart`: the snapshot as one JSON line, or a short summary.
fn snapshot(g: &Globals, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let app = open_or_throwaway(g, !g.db.exists())?;
    let snap = app.store().chart_snapshot(Utc::now())?;
    if g.json {
        writeln!(out, "{}", serde_json::to_string(&snap)?)?;
    } else {
        write!(out, "{}", output::chart_text(&snap))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `ressac chart import FILE`: the file is read and parsed before the database is opened.
fn import(
    g: &Globals,
    file: &Path,
    dry_run: bool,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let parsed: ChartImport = serde_json::from_str(&text).map_err(|e| {
        usage(format!(
            "{} is not a chart import file: {e}",
            file.display()
        ))
    })?;
    let app = open_or_throwaway(g, dry_run && !g.db.exists())?;
    let report = app
        .store()
        .import_chart(&parsed, dry_run)
        .map_err(|e| match e {
            ressac_core::Error::Invalid(_) => usage(e.to_string()),
            other => other.into(),
        })?;
    if g.json {
        writeln!(out, "{}", report_json(&report, dry_run))?;
    } else {
        writeln!(out, "{}", output::chart_import_text(&report, dry_run))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `ressac chart export [--output FILE]`: the import file, from the database as it is now.
/// Read-only, and never creates a database: with none there is nothing to export. The JSON
/// is serde's, so feed-supplied strings are escaped; the messages go through `printable`.
fn export(g: &Globals, output: Option<&Path>, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let store = Store::open_existing_query_only(&g.db)?;
    let export = store.export_chart()?;
    let mut text = serde_json::to_string_pretty(&export.chart)?;
    text.push('\n');
    match output {
        Some(file) => {
            std::fs::write(file, &text)
                .with_context(|| format!("cannot write {}", file.display()))?;
            writeln!(out, "{}", output::chart_export_text(&export.chart, file))?;
        }
        None => write!(out, "{text}")?,
    }
    if !export.unaddressable.is_empty() {
        eprintln!(
            "{}",
            output::chart_export_unaddressable(&export.unaddressable)
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The report's counts and lists, with whether it was a dry run.
fn report_json(report: &ChartImportReport, dry_run: bool) -> serde_json::Value {
    let mut v = serde_json::to_value(report).expect("the report serializes");
    v["dry_run"] = dry_run.into();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_json_carries_the_dry_run_flag() {
        let v = report_json(&ChartImportReport::default(), true);
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["themes"], 0);
        assert_eq!(v["unmatched_urls"], serde_json::json!([]));
    }
}
