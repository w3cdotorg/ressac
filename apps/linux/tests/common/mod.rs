//! Shared by the offscreen integration tests: the run's sandbox and the screenshot checks.
#![cfg(target_os = "linux")]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contract.sqlite"
);

pub struct Run {
    _home: tempfile::TempDir,
    pub home: PathBuf,
    pub shots: PathBuf,
}

impl Run {
    pub fn new() -> Run {
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
    pub fn launch(&self, db: Option<&Path>) -> Output {
        self.launch_with(db, &[])
    }

    pub fn launch_with(&self, db: Option<&Path>, extra_env: &[(&str, &str)]) -> Output {
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
        cmd.envs(extra_env.iter().copied());
        if let Some(db) = db {
            cmd.env("RESSAC_DB", db);
        }
        cmd.output().unwrap()
    }

    pub fn copy_fixture(&self) -> PathBuf {
        let db = self.home.join("r.sqlite");
        std::fs::copy(FIXTURE, &db).unwrap();
        // Entry 6's full-article fetch (walk step 11) must never reach the network: a closed
        // loopback port, refused by the core's address guard before any connection.
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute(
                "UPDATE entry SET url = 'http://127.0.0.1:9/walled' WHERE id = 6",
                [],
            )
            .unwrap();
        db
    }
}

/// Not blank: at least the main window's minimum size, and more than 1000 pixels unlike the
/// top-left one.
pub fn assert_drawn(png: &Path) {
    assert_drawn_min(png, 900, 500);
}

/// The same for a window of another size (Settings, the Sea Chart).
pub fn assert_drawn_min(png: &Path, width: u32, height: u32) {
    let image = image::open(png).unwrap().to_rgba8();
    assert!(
        image.width() >= width && image.height() >= height,
        "{}x{}",
        image.width(),
        image.height()
    );
    let corner = *image.get_pixel(0, 0);
    let different = image.pixels().filter(|p| **p != corner).count();
    assert!(different > 1000, "{png:?} looks blank ({different} pixels)");
}

/// No QML error in the run's stderr.
pub fn assert_clean(out: &Output) {
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

pub fn shots(run: &Run) -> Vec<String> {
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
