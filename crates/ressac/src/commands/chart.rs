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
            write_replacing(file, &text).with_context(|| {
                format!(
                    "cannot write {}",
                    output::printable(&file.display().to_string())
                )
            })?;
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

/// Writes `text` to a temporary file beside `file`, then renames it over `file`: a failed
/// write leaves the old file whole instead of a truncated backup. The temporary file is
/// created exclusively (`create_new` refuses an existing path, a planted symlink included)
/// under a name that differs on every try, and removed if anything fails.
fn write_replacing(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let name = file
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "not a file name"))?;
    let mut attempt = 0u32;
    let (tmp, mut handle) = loop {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let mut tmp_name = std::ffi::OsString::from(".");
        tmp_name.push(name);
        tmp_name.push(format!(".{}-{nanos:x}-{attempt}.tmp", std::process::id()));
        let tmp = file.with_file_name(tmp_name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(handle) => break (tmp, handle),
            Err(e) if e.kind() == ErrorKind::AlreadyExists && attempt < 8 => attempt += 1,
            Err(e) => return Err(e),
        }
    };
    let result = handle
        .write_all(text.as_bytes())
        .and_then(|()| handle.sync_all())
        .and_then(|()| std::fs::rename(&tmp, file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
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
    fn write_replacing_replaces_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("seas.json");
        std::fs::write(&file, "old").unwrap();
        write_replacing(&file, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file left"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_replacing_ignores_a_planted_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "precious").unwrap();
        let file = dir.path().join("seas.json");
        let planted = dir
            .path()
            .join(format!(".seas.json.{}.tmp", std::process::id()));
        std::os::unix::fs::symlink(&victim, planted).unwrap();
        write_replacing(&file, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn write_replacing_failure_keeps_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("missing-dir").join("seas.json");
        assert!(write_replacing(&file, "x").is_err());
        assert!(!file.exists());
    }

    #[test]
    fn report_json_carries_the_dry_run_flag() {
        let v = report_json(&ChartImportReport::default(), true);
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["themes"], 0);
        assert_eq!(v["unmatched_urls"], serde_json::json!([]));
    }
}
