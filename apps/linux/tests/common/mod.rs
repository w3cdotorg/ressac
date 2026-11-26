//! Shared by the offscreen integration tests: the run's sandbox and the screenshot checks.
#![cfg(target_os = "linux")]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contract.sqlite"
);

/// One app at a time per test binary. Every test here launches the whole app, and the Sea Chart's
/// WebEngine page is drawn in software: on the 4-vCPU CI runner it does not draw within the walk's
/// 15 s wait when several apps compete for the CPUs. A test that waits for the chart holds this guard
/// for its whole run. A poisoned lock (an earlier test panicked) is still a free lock.
pub fn one_app_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Run {
    _home: tempfile::TempDir,
    pub home: PathBuf,
    pub shots: PathBuf,
}

/// The fixture's path, for a test that compares a run's copy with it.
pub fn fixture_path() -> &'static Path {
    Path::new(FIXTURE)
}

/// Every feed address on a closed loopback port (`127.0.0.<id>:9`): no traffic leaves the box,
/// and the core's address guard refuses nothing it should fetch.
pub fn unreachable_feeds(db: &Path) {
    rusqlite::Connection::open(db)
        .unwrap()
        .execute_batch(
            "UPDATE feed SET xml_url = 'http://127.0.0.' || id || ':9/feed', html_url = 'http://127.0.0.' || id || ':9/'; \
             UPDATE entry SET url = 'http://127.0.0.1:9/entry/' || id;",
        )
        .unwrap();
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

    /// The run's `XDG_DATA_HOME`: where a launch with no `RESSAC_DB` puts `ressac/ressac.sqlite`.
    pub fn data_home(&self) -> PathBuf {
        self.home.join("data")
    }

    /// The working directory of every launch: a folder of its own, empty unless the app writes there.
    pub fn cwd(&self) -> PathBuf {
        self.home.join("cwd")
    }

    /// `ressac-kde --screenshot <shots>` with RESSAC_DB = `db` (None: unset), offscreen, HOME and
    /// XDG_{CONFIG,DATA,STATE,CACHE}_HOME under the temp dir, RUST_LOG unset.
    pub fn launch(&self, db: Option<&Path>) -> Output {
        self.launch_with(db, &[])
    }

    pub fn launch_with(&self, db: Option<&Path>, extra_env: &[(&str, &str)]) -> Output {
        std::fs::create_dir_all(self.cwd()).unwrap();
        let mut cmd = Command::new("timeout");
        cmd.current_dir(self.cwd());
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

/// The eight teletext colours (black, red, green, yellow, blue, magenta, cyan, white).
pub const TELETEXT: [[u8; 3]; 8] = [
    [0, 0, 0],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// A pixel rectangle of a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The same rectangle moved by `(dx, dy)` (a teletext screen's origin in the window).
    pub fn offset(self, (dx, dy): (u32, u32)) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    pub fn pixels(self) -> impl Iterator<Item = (u32, u32)> {
        (self.y..self.y + self.height)
            .flat_map(move |y| (self.x..self.x + self.width).map(move |x| (x, y)))
    }
}

/// Cells `first .. first + cells` of teletext row `row` (rows of `size` px, cells 0.6 × `size`
/// wide), in device pixels at `scale`, from the screen's origin.
pub fn row_rect(row: u32, first: u32, cells: u32, size: u32, scale: f64) -> Rect {
    let device = |logical: f64| (logical * scale).round() as u32;
    let cell = 0.6 * f64::from(size);
    Rect {
        x: device(f64::from(first) * cell),
        y: device(f64::from(row * size)),
        width: device(f64::from(cells) * cell),
        height: device(f64::from(size)),
    }
}

/// Whether any pixel of `rect` is exactly `rgb`.
pub fn has_colour(image: &image::RgbImage, rect: Rect, rgb: [u8; 3]) -> bool {
    rect.pixels().any(|(x, y)| image.get_pixel(x, y).0 == rgb)
}

/// The walk's line that starts with `prefix`, without the `qml: ` of the stderr log.
pub fn walk_line<'a>(err: &'a str, prefix: &str) -> &'a str {
    err.lines()
        .map(|l| l.strip_prefix("qml: ").unwrap_or(l))
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("{prefix} missing in {err}"))
}

/// `count` unread entries in feed `feed`, newer than the fixture's, with titles that wrap over
/// two rows of the entries column; the first 200 carry a long article (about 30 kB).
pub fn insert_entries(db: &Path, feed: i64, count: usize) {
    let mut conn = rusqlite::Connection::open(db).unwrap();
    let tx = conn.transaction().unwrap();
    let paragraph = "<p>The tide comes in over the flats twice a day, and the <a href=\"https://example.org/\">harbour</a> \
        keeps its own <em>teletext</em> pages: every line is read, sanitised and laid out again.</p>";
    let long = paragraph.repeat(150);
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO entry (feed_id, guid, url, title, published_at, content_html, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-10-10T08:00:00.000Z')",
            )
            .unwrap();
        for i in 0..count {
            // Newest first: entry i is published i minutes before 2026-11-20 00:00 (after the fixture's).
            let minutes = 60 * 24 * 20 - i as i64;
            let published = format!(
                "2026-11-{:02}T{:02}:{:02}:00.000Z",
                minutes / (60 * 24),
                minutes / 60 % 24,
                minutes % 60
            );
            insert
                .execute(rusqlite::params![
                    feed,
                    format!("bulk-{i}"),
                    format!("http://127.0.0.1:9/bulk/{i}"),
                    format!("Bulk entry {i}: a title long enough to wrap over the second row"),
                    published,
                    if i < 200 { long.as_str() } else { paragraph },
                ])
                .unwrap();
        }
    }
    tx.commit().unwrap();
}
