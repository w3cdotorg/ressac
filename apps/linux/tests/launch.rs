//! The launch states, offscreen, through `--screenshot` (spec §7.2, §8.2; inventory §1.1).
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ressac_core::store::{SchemaState, Store};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contract.sqlite"
);

struct Run {
    _home: tempfile::TempDir,
    home: PathBuf,
    shots: PathBuf,
}

impl Run {
    fn new() -> Run {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let shots = home.join("shots");
        Run {
            _home: dir,
            home,
            shots,
        }
    }

    /// `ressac-kde --screenshot <shots>` with RESSAC_DB = `db` (None: unset), offscreen, HOME and
    /// XDG_{CONFIG,DATA,STATE,CACHE}_HOME under the temp dir, RUST_LOG unset.
    fn launch(&self, db: Option<&Path>) -> Output {
        let mut cmd = Command::new("timeout");
        cmd.arg("90")
            .arg(env!("CARGO_BIN_EXE_ressac-kde"))
            .arg("--screenshot")
            .arg(&self.shots)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env_remove("RUST_LOG")
            .env_remove("RESSAC_DB");
        if let Some(db) = db {
            cmd.env("RESSAC_DB", db);
        }
        cmd.output().unwrap()
    }

    fn copy_fixture(&self) -> PathBuf {
        let db = self.home.join("r.sqlite");
        std::fs::copy(FIXTURE, &db).unwrap();
        db
    }
}

/// Not blank: at least the window's minimum size, and more than 1000 pixels unlike the top-left one.
fn assert_drawn(png: &Path) {
    let image = image::open(png).unwrap().to_rgba8();
    assert!(
        image.width() >= 900 && image.height() >= 500,
        "{}x{}",
        image.width(),
        image.height()
    );
    let corner = *image.get_pixel(0, 0);
    let different = image.pixels().filter(|p| **p != corner).count();
    assert!(different > 1000, "{png:?} looks blank ({different} pixels)");
}

/// No QML error in the run's stderr.
fn assert_clean(out: &Output) {
    let err = String::from_utf8_lossy(&out.stderr);
    for bad in [
        "failed to load component",
        "is not installed",
        "ReferenceError",
        "TypeError",
        "Unable to assign",
    ] {
        assert!(!err.contains(bad), "{bad}: {err}");
    }
}

fn shots(run: &Run) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&run.shots)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_current_database_opens_ready() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-ready.png"]);
    assert_drawn(&run.shots.join("01-ready.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current);
    assert!(run.home.join("state/ressac/logs").is_dir()); // logging::init under XDG_STATE_HOME
    assert!(!run.home.join("config/ressac/ressac-kde.conf").exists()); // the launch only reads settings
}

#[test]
fn a_missing_or_empty_database_is_created_before_opening() {
    let run = Run::new();
    let missing = run.home.join("data/new/ressac.sqlite");
    let empty = run.home.join("empty.sqlite");
    std::fs::write(&empty, b"").unwrap();
    for db in [missing, empty] {
        let _ = std::fs::remove_dir_all(&run.shots);
        let out = run.launch(Some(&db));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(shots(&run), ["01-ready.png"], "{db:?}");
        assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current, "{db:?}");
    }
}

#[test]
fn a_newer_database_is_refused_and_left_alone() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute("UPDATE schema_meta SET version = 99", [])
        .unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-too-new.png"]);
    assert_drawn(&run.shots.join("01-too-new.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Newer(99));
}

#[test]
fn a_file_that_is_not_a_database_fails_and_is_left_alone() {
    let run = Run::new();
    let db = run.home.join("notes.sqlite");
    let words = "not a database, only words, long enough to fill the first page of a SQLite header";
    std::fs::write(&db, words).unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(shots(&run), ["01-failed.png"]);
    assert_drawn(&run.shots.join("01-failed.png"));
    assert_eq!(std::fs::read_to_string(&db).unwrap(), words);
}

#[test]
fn the_screenshot_flag_needs_ressac_db() {
    let run = Run::new();
    let out = run.launch(None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        format!("{}\n", ressac_ui::launch::SCREENSHOT_NEEDS_DB)
    );
    assert!(!run.shots.exists());
}
