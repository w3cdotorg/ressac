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
/// created exclusively (see `open_exclusive`), takes the permissions of an existing target
/// (so a 0600 file stays 0600) and is removed if anything fails. A symlink at `file` is
/// replaced by a regular file, not followed.
fn write_replacing(file: &Path, text: &str) -> std::io::Result<()> {
    let (tmp, mut handle) = open_exclusive(temp_names(file)?)?;
    let result = (|| {
        if let Ok(existing) = std::fs::metadata(file) {
            handle.set_permissions(existing.permissions())?;
        }
        handle.write_all(text.as_bytes())?;
        handle.sync_all()?;
        std::fs::rename(&tmp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Names for the temporary file beside `file`, one per try, differing by process, time and
/// try number.
fn temp_names(file: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let name = file
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a file name"))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    Ok((0..9)
        .map(|attempt| {
            let mut tmp = std::ffi::OsString::from(".");
            tmp.push(name);
            tmp.push(format!(".{}-{nanos:x}-{attempt}.tmp", std::process::id()));
            file.with_file_name(tmp)
        })
        .collect())
}

/// Creates the first of `names` that does not exist, never following or truncating
/// anything: `create_new` fails with `AlreadyExists` on a file, a directory or a symlink
/// (dangling ones too), and the next name is tried. Every name taken is `AlreadyExists`.
fn open_exclusive(
    names: impl IntoIterator<Item = std::path::PathBuf>,
) -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    for path in names {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(handle) => return Ok((path, handle)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::ErrorKind::AlreadyExists.into())
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

    #[test]
    fn open_exclusive_refuses_an_existing_file_and_moves_to_the_next_name() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::write(&first, "mine").unwrap();
        let err = open_exclusive([first.clone()]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        let (got, _) = open_exclusive([first.clone(), second.clone()]).unwrap();
        assert_eq!(got, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "mine");
    }

    #[cfg(unix)]
    #[test]
    fn open_exclusive_never_follows_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "precious").unwrap();
        let (planted, dangling) = (dir.path().join("p"), dir.path().join("d"));
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere"), &dangling).unwrap();
        let free = dir.path().join("free");
        let (got, mut handle) =
            open_exclusive([planted.clone(), dangling.clone(), free.clone()]).unwrap();
        assert_eq!(got, free, "both symlinks were refused");
        handle.write_all(b"x").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert!(!dir.path().join("nowhere").exists());
    }

    #[test]
    fn open_exclusive_with_every_name_taken_is_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("t");
        std::fs::write(&taken, "").unwrap();
        let err = open_exclusive([taken.clone(), taken]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[cfg(unix)]
    #[test]
    fn write_replacing_keeps_the_targets_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("seas.json");
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        write_replacing(&file, "new").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn write_replacing_replaces_a_symlink_instead_of_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "precious").unwrap();
        let file = dir.path().join("seas.json");
        std::os::unix::fs::symlink(&victim, &file).unwrap();
        write_replacing(&file, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert!(!std::fs::symlink_metadata(&file).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn write_replacing_failure_keeps_the_target_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        // A non-empty directory cannot be renamed over by a file: the rename fails.
        let target = dir.path().join("seas.json");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("inside"), "kept").unwrap();
        assert!(write_replacing(&target, "x").is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("inside")).unwrap(),
            "kept"
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["seas.json"], "no .tmp left: {names:?}");
    }

    #[test]
    fn report_json_carries_the_dry_run_flag() {
        let v = report_json(&ChartImportReport::default(), true);
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["themes"], 0);
        assert_eq!(v["unmatched_urls"], serde_json::json!([]));
    }
}
