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

/// The CRT screen in a walk's shots: the Antiope view's place and size in the window (logical
/// pixels, from a `walk: crt-origin x=… y=… width=… height=… size=…` line), its text size and
/// the device pixel ratio.
#[derive(Debug, Clone, Copy)]
pub struct CrtScreen {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub scale: f64,
}

impl CrtScreen {
    pub fn from_line(line: &str, scale: f64) -> CrtScreen {
        let value = |key: &str| -> f64 {
            line.split(&format!(" {key}="))
                .nth(1)
                .unwrap_or_else(|| panic!("{key} missing in {line}"))
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        CrtScreen {
            x: value("x"),
            y: value("y"),
            width: value("width"),
            height: value("height"),
            size: value("size"),
            scale,
        }
    }

    /// A logical length or position in device pixels.
    pub fn device(&self, logical: f64) -> u32 {
        (logical * self.scale).round() as u32
    }

    /// The view's origin in device pixels.
    pub fn origin(&self) -> (u32, u32) {
        (self.device(self.x), self.device(self.y))
    }

    /// Cells `first .. first + cells` of the view's row `row`, in device pixels in the window.
    pub fn cells(&self, row: u32, first: u32, cells: u32) -> Rect {
        let cell = 0.6 * self.size;
        let left = self.device(self.x + f64::from(first) * cell);
        let top = self.device(self.y + f64::from(row) * self.size);
        Rect {
            x: left,
            y: top,
            width: self.device(self.x + f64::from(first + cells) * cell) - left,
            height: self.device(self.y + f64::from(row + 1) * self.size) - top,
        }
    }

    /// The window's device rows a scanline band covers (`teletext::crt::scanline_bands`, the
    /// geometry the overlay paints).
    pub fn band_rows(&self) -> std::collections::HashSet<u32> {
        ressac_ui::teletext::crt::scanline_bands(self.height, self.size / 10.0, self.scale)
            .into_iter()
            .flat_map(|(top, bottom)| self.device(self.y + top)..self.device(self.y + bottom))
            .collect()
    }

    /// The vignette's black opacity at device pixel `(x, y)`: 0 at the centre, 0.35 at half the
    /// diagonal, linear in between.
    pub fn vignette(&self, x: u32, y: u32) -> f64 {
        let lx = (f64::from(x) + 0.5) / self.scale - self.x - self.width / 2.0;
        let ly = (f64::from(y) + 0.5) / self.scale - self.y - self.height / 2.0;
        let radius = self.width.hypot(self.height) / 2.0;
        0.35 * (lx.hypot(ly) / radius).min(1.0)
    }
}

/// The scanlines over white text: in `rect`, every pixel white in `off` (CRT off) reads, in `on`,
/// 255 − vignette (± 4) between the bands, and on a band row 183 ± 4 (255 × 0.72) once the
/// vignette of the white pixel just above or below it (between bands) is divided out. Returns
/// the band and between pixels checked.
pub fn darkened_band_rows(
    off: &image::RgbImage,
    on: &image::RgbImage,
    screen: &CrtScreen,
    rect: Rect,
) -> (usize, usize) {
    const WHITE: [u8; 3] = [255, 255, 255];
    let bands = screen.band_rows();
    let (mut band_pixels, mut between_pixels) = (0, 0);
    for (x, y) in rect.pixels() {
        if off.get_pixel(x, y).0 != WHITE {
            continue;
        }
        let shown = on.get_pixel(x, y).0;
        if bands.contains(&y) {
            let neighbour = [y.wrapping_sub(1), y + 1].into_iter().find(|&ny| {
                !bands.contains(&ny) && ny < off.height() && off.get_pixel(x, ny).0 == WHITE
            });
            let Some(ny) = neighbour else { continue };
            let between = on.get_pixel(x, ny).0;
            for c in 0..3 {
                let undimmed = f64::from(shown[c]) / f64::from(between[c]) * 255.0;
                assert!(
                    (undimmed - 183.0).abs() <= 4.0,
                    "band row {y} at x {x}: {shown:?} under {between:?} = {undimmed:.1}"
                );
            }
            band_pixels += 1;
        } else {
            let expected = 255.0 * (1.0 - screen.vignette(x, y));
            for value in shown {
                assert!(
                    (f64::from(value) - expected).abs() <= 4.0,
                    "between bands at ({x}, {y}): {shown:?}, expected {expected:.1}"
                );
            }
            between_pixels += 1;
        }
    }
    (band_pixels, between_pixels)
}

/// The colour bleed beside text of colour `text` drawn on `ground`: the edges are found in
/// `reference` (CRT off), the bleed read in `shot`. Returns (right edges, reddish pixels just
/// right of them, left edges, bluish pixels just left of them); a pixel is reddish (bluish) when
/// its red (blue) channel beats both others by more than 30.
pub fn bleed_beside_edges(
    reference: &image::RgbImage,
    shot: &image::RgbImage,
    rect: Rect,
    text: [u8; 3],
    ground: [u8; 3],
) -> (usize, usize, usize, usize) {
    let reddish = |p: [u8; 3]| i32::from(p[0]) > i32::from(p[1].max(p[2])) + 30;
    let bluish = |p: [u8; 3]| i32::from(p[2]) > i32::from(p[0].max(p[1])) + 30;
    let (mut right, mut red, mut left, mut blue) = (0, 0, 0, 0);
    for (x, y) in rect.pixels() {
        if x == 0 || x + 1 >= reference.width() || reference.get_pixel(x, y).0 != text {
            continue;
        }
        if reference.get_pixel(x + 1, y).0 == ground {
            right += 1;
            red += usize::from(reddish(shot.get_pixel(x + 1, y).0));
        }
        if reference.get_pixel(x - 1, y).0 == ground {
            left += 1;
            blue += usize::from(bluish(shot.get_pixel(x - 1, y).0));
        }
    }
    (right, red, left, blue)
}

/// The number after ` key=` in `frames: crt=<crt> step=<step> …` (the `crt-scroll` walk).
pub fn frames_field(err: &str, crt: &str, step: u32, key: &str) -> f64 {
    let line = walk_line(err, &format!("frames: crt={crt} step={step} "));
    line.split(&format!(" {key}="))
        .nth(1)
        .unwrap_or_else(|| panic!("{key} missing in {line}"))
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
