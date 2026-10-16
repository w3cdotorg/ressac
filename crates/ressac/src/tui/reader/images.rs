//! Images in the reader: protocol detection, guarded fetch, bounded decoding, a small cache,
//! and the eight-colour quantisation Antiope draws with.

use std::collections::VecDeque;
use std::io::{Cursor, Write};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageReader, Limits, Rgb, RgbImage};
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use super::state::Event;

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
pub const MAX_DECODED_SIDE: u32 = 4096;
pub const CACHE_SIZE: usize = 32;
pub const MAX_IN_FLIGHT: usize = 4;
/// Images are scaled down to fit this square before anything keeps them: memory is bounded by
/// pixels, not only by the cache's count (the reader never draws more than half its height).
pub const MAX_KEPT_SIDE: u32 = 1600;
/// Antiope's half-blocks need few pixels: the teletext copy starts from a thumbnail.
const TELETEXT_SIDE: u32 = 320;
/// Failed images remembered (and refused ones waiting to be retried); oldest forgotten first.
const MAX_FAILED: usize = 256;
/// A transient failure (busy database, timeout, offline) is asked again after this long.
const RETRY_AFTER: Duration = Duration::from_secs(30);
const MAX_ALLOC: u64 = 128 * 1024 * 1024;

/// Scales down to fit `side` × `side`, keeping the aspect; never up.
fn fit_within(image: DynamicImage, side: u32) -> DynamicImage {
    if image.width() > side || image.height() > side {
        image.thumbnail(side, side)
    } else {
        image
    }
}

/// An image ready for the store, built in the fetch thread: the picture within the size cap
/// and a small copy the teletext variant is derived from, when that variant is first asked for.
pub struct Prepared {
    kept: DynamicImage,
    small: DynamicImage,
}

pub fn prepare(image: DynamicImage) -> Prepared {
    let kept = fit_within(image, MAX_KEPT_SIDE);
    let small = fit_within(kept.clone(), TELETEXT_SIDE);
    Prepared { kept, small }
}

/// Why a fetch ended without an image.
struct Failure {
    /// Permanent: asking again gives the same answer (not an image, too large, refused address,
    /// undecodable, HTTP 4xx). Otherwise it is worth another try later.
    permanent: bool,
    why: String,
}

/// `fetch_image`'s refusals are sentences (phase 2); the ones that cannot change are permanent.
fn is_permanent(error: &ressac_core::Error) -> bool {
    let ressac_core::Error::ImageRefused(why) = error else {
        return false; // the database, the runtime: nothing about the image itself
    };
    match why.as_str() {
        "not a web address"
        | "local or private address"
        | "larger than 10 MB"
        | "not an image"
        | "response too large" => true,
        other => {
            other.strip_prefix("HTTP 4").is_some() && !matches!(other, "HTTP 408" | "HTTP 429")
        }
    }
}

/// Decodes with hard limits: the header's size is checked before any pixel buffer exists.
pub fn decode_limited(bytes: &[u8]) -> Result<DynamicImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    reader.decode().map_err(|e| e.to_string())
}

/// Every pixel becomes the nearest of the eight teletext colours; transparency becomes black.
#[allow(dead_code, reason = "phase 9 draws Antiope's images with it")]
pub fn quantise_teletext(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let [r, g, b, a] = p.0;
        let px = if a < 128 {
            [0, 0, 0]
        } else {
            nearest_teletext([r, g, b])
        };
        out.put_pixel(x, y, Rgb(px));
    }
    DynamicImage::ImageRgb8(out)
}

fn nearest_teletext(c: [u8; 3]) -> [u8; 3] {
    let dist = |t: &[u8; 3]| -> i32 {
        t.iter()
            .zip(c)
            .map(|(a, b)| (i32::from(*a) - i32::from(b)).pow(2))
            .sum()
    };
    *TELETEXT
        .iter()
        .min_by_key(|t| dist(t))
        .expect("eight colours")
}

/// Asks the terminal which graphics protocol it speaks. Call it only after
/// `ratatui::try_init()`: the query leaves a thread that snapshots the terminal mode and
/// restores it when its stdin read unblocks; before init that mode is line mode, and its late
/// restore froze the keyboard under tmux (clappuccino, `app.rs`). An unwrapped DA1 request
/// then answers that thread's read so it ends on its own instead of swallowing the first key
/// (clappuccino, `preview.rs`). Warp announces Kitty but draws only iTerm2.
pub fn detect_picker() -> Picker {
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let warp = std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "WarpTerminal");
    if warp && picker.protocol_type() == ProtocolType::Kitty {
        picker.set_protocol_type(ProtocolType::Iterm2);
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[c");
    let _ = out.flush();
    picker
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Native,
    #[allow(dead_code, reason = "phase 9 draws Antiope's images with it")]
    Teletext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageState {
    Absent,
    Loading,
    Ready,
    Failed,
}

enum Slot {
    Loading,
    Ready {
        native: Box<StatefulProtocol>,
        /// The source of the teletext copy; the copy itself is built on first use.
        small: DynamicImage,
        teletext: Option<Box<StatefulProtocol>>,
    },
    Failed,
    /// A transient failure: shown as failed until `until`, then asked again.
    Retry {
        until: Instant,
    },
}

impl Slot {
    fn is_ready(&self) -> bool {
        matches!(self, Slot::Ready { .. })
    }

    fn is_failed(&self) -> bool {
        matches!(self, Slot::Failed | Slot::Retry { .. })
    }
}

/// What a fetch worker sends back.
struct Fetched {
    url: String,
    result: Result<Prepared, Failure>,
}

pub struct ImageStore {
    /// `false` for `disabled()`: nothing is fetched and every image stays its alt text.
    enabled: bool,
    picker: Picker,
    teletext: Picker,
    /// Least recently used first; only `Ready` slots count towards `CACHE_SIZE`, and the failed
    /// ones towards `MAX_FAILED`.
    slots: Vec<(String, Slot)>,
    wanted: Vec<String>,
    queue: VecDeque<String>,
    in_flight: usize,
    retry_after: Duration,
    /// The workers, started with the first fetch: each opens one connection and one runtime
    /// and keeps them for every image it fetches.
    jobs: Option<Sender<String>>,
    tx: Sender<Fetched>,
    rx: Receiver<Fetched>,
}

impl ImageStore {
    pub fn new(picker: Picker) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            enabled: true,
            picker,
            teletext: Picker::halfblocks(),
            slots: Vec::new(),
            wanted: Vec::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            retry_after: RETRY_AFTER,
            jobs: None,
            tx,
            rx,
        }
    }

    /// No picker, no fetch: every image stays `[image: alt text]` (rendering tests).
    #[allow(dead_code, reason = "phase 9's rendering tests use it")]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::new(Picker::halfblocks())
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[allow(
        dead_code,
        reason = "phase 9 tells Antiope's `i` whether a real image can be drawn"
    )]
    pub fn has_graphics_protocol(&self) -> bool {
        matches!(
            self.picker.protocol_type(),
            ProtocolType::Kitty | ProtocolType::Iterm2 | ProtocolType::Sixel
        )
    }

    fn position(&self, url: &str) -> Option<usize> {
        self.slots.iter().position(|(u, _)| u == url)
    }

    /// Replaces whatever the slot of `url` was and makes it the most recent.
    fn set_slot(&mut self, url: &str, slot: Slot) {
        if let Some(i) = self.position(url) {
            self.slots.remove(i);
        }
        self.slots.push((url.to_owned(), slot));
        self.evict();
    }

    pub fn state_of(&self, url: &str) -> ImageState {
        match self.position(url).map(|i| &self.slots[i].1) {
            None => ImageState::Absent,
            Some(Slot::Loading) => ImageState::Loading,
            Some(Slot::Ready { .. }) => ImageState::Ready,
            Some(Slot::Failed) => ImageState::Failed,
            Some(Slot::Retry { until }) if Instant::now() >= *until => ImageState::Absent,
            Some(Slot::Retry { .. }) => ImageState::Failed,
        }
    }

    /// The renderer's request for an image near the viewport, every frame: absent images only.
    pub fn want(&mut self, url: &str) {
        if self.enabled
            && self.state_of(url) == ImageState::Absent
            && !self.wanted.iter().any(|w| w == url)
        {
            self.wanted.push(url.to_owned());
        }
    }

    pub fn take_wanted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.wanted)
    }

    pub fn mark_loading(&mut self, url: &str) {
        self.set_slot(url, Slot::Loading);
    }

    #[cfg(test)]
    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Starts what the last frame wanted, at most `MAX_IN_FLIGHT` at a time. The queue is
    /// replaced by this frame's list, so images of an article the reader left do not delay the
    /// ones on screen.
    pub fn start_wanted(&mut self, db: &Path) {
        self.queue = self.take_wanted().into();
        self.start_queued(db);
    }

    fn start_queued(&mut self, db: &Path) {
        while self.in_flight < MAX_IN_FLIGHT {
            let Some(url) = self.queue.pop_front() else {
                break;
            };
            self.fetch(db, url);
        }
    }

    /// One fetch on a worker (its own connection, decoded and resized there). Not a long job:
    /// it never touches the loop's job queue. When `MAX_IN_FLIGHT` are running it waits.
    pub fn fetch(&mut self, db: &Path, url: String) {
        if !self.enabled || self.state_of(&url) != ImageState::Absent {
            return;
        }
        if self.in_flight >= MAX_IN_FLIGHT {
            if !self.queue.contains(&url) {
                self.queue.push_back(url);
            }
            return;
        }
        let sent = self
            .workers(db)
            .is_some_and(|jobs| jobs.send(url.clone()).is_ok());
        if sent {
            self.mark_loading(&url);
            self.in_flight += 1;
        } else {
            // No thread could be started: soft, and asked again later.
            tracing::warn!("image {url}: no fetch worker");
            self.retry_later(&url);
        }
    }

    /// The job channel, starting the workers on first use. `None` when no thread could start.
    fn workers(&mut self, db: &Path) -> Option<&Sender<String>> {
        if self.jobs.is_none() {
            let (jobs, queue) = mpsc::channel::<String>();
            let queue = Arc::new(Mutex::new(queue));
            let mut started = 0;
            for _ in 0..MAX_IN_FLIGHT {
                let (queue, tx, db) = (Arc::clone(&queue), self.tx.clone(), db.to_path_buf());
                let spawned = std::thread::Builder::new()
                    .name("ressac-image".into())
                    .spawn(move || worker(&db, &queue, &tx));
                started += usize::from(spawned.is_ok());
            }
            if started > 0 {
                self.jobs = Some(jobs);
            }
        }
        self.jobs.as_ref()
    }

    /// The fetches that ended since the last call, as state events; the queue moves on.
    pub fn drain(&mut self, db: &Path) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(Fetched { url, result }) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match result {
                Ok(prepared) => {
                    self.keep(&url, prepared);
                    events.push(Event::ImageReady { url });
                }
                Err(Failure { permanent, why }) => {
                    tracing::debug!("image {url}: {why} (permanent: {permanent})");
                    if permanent {
                        self.fail(&url);
                    } else {
                        self.retry_later(&url);
                    }
                    events.push(Event::ImageFailed { url });
                }
            }
        }
        self.start_queued(db);
        events
    }

    /// Keeps an image decoded elsewhere (the workers send `Prepared` ones): tests.
    #[cfg(test)]
    pub fn ingest(&mut self, url: &str, image: DynamicImage) {
        self.keep(url, prepare(image));
    }

    fn keep(&mut self, url: &str, Prepared { kept, small }: Prepared) {
        let native = Box::new(self.picker.new_resize_protocol(kept));
        self.set_slot(
            url,
            Slot::Ready {
                native,
                small,
                teletext: None,
            },
        );
    }

    pub fn fail(&mut self, url: &str) {
        self.set_slot(url, Slot::Failed);
    }

    /// A failure that may pass: the image is asked again after `retry_after`.
    fn retry_later(&mut self, url: &str) {
        let until = Instant::now() + self.retry_after;
        self.set_slot(url, Slot::Retry { until });
    }

    fn evict(&mut self) {
        for (limit, kind) in [
            (CACHE_SIZE, Slot::is_ready as fn(&Slot) -> bool),
            (MAX_FAILED, Slot::is_failed),
        ] {
            while self.slots.iter().filter(|(_, s)| kind(s)).count() > limit {
                let oldest = self
                    .slots
                    .iter()
                    .position(|(_, s)| kind(s))
                    .expect("over the cap, so one matches");
                self.slots.remove(oldest);
            }
        }
    }

    /// The protocol to draw; touching it makes the image the most recently used. The teletext
    /// copy is quantised the first time it is asked for.
    pub fn protocol(&mut self, url: &str, variant: Variant) -> Option<&mut StatefulProtocol> {
        let i = self.position(url)?;
        let slot = self.slots.remove(i);
        self.slots.push(slot);
        let Self {
            slots,
            teletext: picker,
            ..
        } = self;
        match &mut slots.last_mut().expect("just pushed").1 {
            Slot::Ready {
                native,
                small,
                teletext,
            } => Some(match variant {
                Variant::Native => native,
                Variant::Teletext => teletext.get_or_insert_with(|| {
                    Box::new(picker.new_resize_protocol(quantise_teletext(small)))
                }),
            }),
            _ => None,
        }
    }

    /// Rows the image takes in a column `width` cells wide, at most `max_rows`; 1 for the
    /// placeholder line of an image that is not ready.
    pub fn rows_for(&self, url: &str, variant: Variant, width: u16, max_rows: u16) -> u16 {
        let Some(i) = self.position(url) else {
            return 1;
        };
        let available = Size::new(width, max_rows.max(1));
        let size = match (&self.slots[i].1, variant) {
            (Slot::Ready { native, .. }, Variant::Native) => {
                native.size_for(Resize::Fit(None), available)
            }
            (
                Slot::Ready {
                    teletext: Some(teletext),
                    ..
                },
                Variant::Teletext,
            ) => teletext.size_for(Resize::Fit(None), available),
            (Slot::Ready { small, .. }, Variant::Teletext) => {
                Resize::Fit(None).size_for(small, self.teletext.font_size(), available)
            }
            _ => return 1,
        };
        size.height.clamp(1, max_rows.max(1))
    }
}

/// A fetch worker: one connection and one runtime for all its images.
fn worker(db: &Path, jobs: &Mutex<Receiver<String>>, results: &Sender<Fetched>) {
    let mut session: Option<(ressac_core::service::Ressac, tokio::runtime::Runtime)> = None;
    loop {
        let url = {
            let jobs = jobs.lock().unwrap_or_else(PoisonError::into_inner);
            match jobs.recv() {
                Ok(url) => url,
                Err(_) => return, // the store is gone
            }
        };
        // A decoder panic on a hostile file must still answer, and restart the session.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            fetch_and_decode(db, &mut session, &url)
        }))
        .unwrap_or_else(|_| {
            session = None;
            Err(Failure {
                permanent: true,
                why: "the image could not be decoded".into(),
            })
        });
        if results.send(Fetched { url, result }).is_err() {
            return;
        }
    }
}

fn fetch_and_decode(
    db: &Path,
    session: &mut Option<(ressac_core::service::Ressac, tokio::runtime::Runtime)>,
    url: &str,
) -> Result<Prepared, Failure> {
    let transient = |why: String| Failure {
        permanent: false,
        why,
    };
    if session.is_none() {
        let app = crate::commands::open(db).map_err(|e| transient(format!("{e:#}")))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| transient(e.to_string()))?;
        *session = Some((app, runtime));
    }
    let (app, runtime) = session.as_ref().expect("just opened");
    let bytes = runtime
        .block_on(app.fetch_image(url))
        .map_err(|e| Failure {
            permanent: is_permanent(&e),
            why: e.to_string(),
        })?;
    let image = decode_limited(&bytes).map_err(|why| Failure {
        permanent: true,
        why,
    })?;
    Ok(prepare(image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// A PNG whose header announces `width` × `height` and has no pixel data: a decoder that
    /// trusted the header would try to allocate it.
    fn png_header_only(width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend(width.to_be_bytes());
        ihdr.extend(height.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend(13u32.to_be_bytes());
        png.extend(&ihdr);
        png.extend(crc32(&ihdr).to_be_bytes());
        png.extend(0u32.to_be_bytes());
        png.extend(b"IEND");
        png.extend(crc32(b"IEND").to_be_bytes());
        png
    }

    fn png(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb(colour)));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_small_png_decodes() {
        let img = decode_limited(&png(40, 30, [10, 200, 30])).unwrap();
        assert_eq!((img.width(), img.height()), (40, 30));
    }

    #[test]
    fn an_image_announcing_twenty_thousand_pixels_a_side_is_refused() {
        // Review Focus 4: refused from its header, without allocating 1.6 GB.
        let err = decode_limited(&png_header_only(20_000, 20_000)).unwrap_err();
        assert!(err.to_lowercase().contains("limit"), "{err}");
    }

    #[test]
    fn an_html_body_is_not_an_image() {
        assert!(decode_limited(b"<!doctype html><title>Moved</title>").is_err());
    }

    #[test]
    fn quantisation_maps_every_pixel_to_a_teletext_colour() {
        let mut src = RgbImage::new(4, 1);
        src.put_pixel(0, 0, Rgb([250, 10, 10])); // red
        src.put_pixel(1, 0, Rgb([20, 30, 200])); // blue
        src.put_pixel(2, 0, Rgb([240, 240, 30])); // yellow
        src.put_pixel(3, 0, Rgb([128, 128, 128])); // grey: the nearest of the eight
        let out = quantise_teletext(&DynamicImage::ImageRgb8(src)).to_rgb8();
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(out.get_pixel(1, 0).0, [0, 0, 255]);
        assert_eq!(out.get_pixel(2, 0).0, [255, 255, 0]);
        assert!(TELETEXT.contains(&out.get_pixel(3, 0).0));
        for p in out.pixels() {
            assert!(TELETEXT.contains(&p.0), "{:?}", p.0);
        }
    }

    #[test]
    fn transparent_pixels_become_black() {
        let src = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 0]));
        let out = quantise_teletext(&DynamicImage::ImageRgba8(src)).to_rgb8();
        assert!(out.pixels().all(|p| p.0 == [0, 0, 0]));
    }

    fn store() -> ImageStore {
        ImageStore::new(Picker::halfblocks())
    }

    fn photo() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([200, 120, 40])))
    }

    #[test]
    fn halfblocks_is_not_a_graphics_protocol() {
        assert!(!store().has_graphics_protocol());
    }

    #[test]
    fn a_wanted_image_is_loading_then_ready() {
        let mut s = store();
        s.want("https://example.com/a.jpg");
        s.want("https://example.com/a.jpg");
        assert_eq!(
            s.take_wanted(),
            vec!["https://example.com/a.jpg".to_string()],
            "asked once"
        );
        s.mark_loading("https://example.com/a.jpg");
        assert_eq!(s.state_of("https://example.com/a.jpg"), ImageState::Loading);
        s.ingest("https://example.com/a.jpg", photo());
        assert_eq!(s.state_of("https://example.com/a.jpg"), ImageState::Ready);
        assert!(
            s.protocol("https://example.com/a.jpg", Variant::Native)
                .is_some()
        );
        assert!(
            s.protocol("https://example.com/a.jpg", Variant::Teletext)
                .is_some()
        );
    }

    #[test]
    fn a_failed_image_is_not_asked_again() {
        let mut s = store();
        s.fail("https://example.com/x.png");
        s.want("https://example.com/x.png");
        assert!(s.take_wanted().is_empty());
        assert_eq!(s.state_of("https://example.com/x.png"), ImageState::Failed);
    }

    #[test]
    fn the_cache_keeps_the_thirty_two_most_recent_images() {
        let mut s = store();
        for i in 0..40 {
            s.ingest(
                &format!("https://example.com/{i}.png"),
                DynamicImage::new_rgb8(8, 8),
            );
        }
        assert_eq!(
            s.state_of("https://example.com/0.png"),
            ImageState::Absent,
            "evicted"
        );
        assert_eq!(s.state_of("https://example.com/39.png"), ImageState::Ready);
        // Using an image keeps it.
        s.protocol("https://example.com/8.png", Variant::Native);
        s.ingest("https://example.com/40.png", DynamicImage::new_rgb8(8, 8));
        assert_eq!(s.state_of("https://example.com/8.png"), ImageState::Ready);
        assert_eq!(s.state_of("https://example.com/9.png"), ImageState::Absent);
    }

    #[test]
    fn an_image_takes_at_most_the_rows_it_is_allowed() {
        let mut s = store();
        s.ingest("https://example.com/a.jpg", photo());
        let rows = s.rows_for("https://example.com/a.jpg", Variant::Native, 60, 10);
        assert!((1..=10).contains(&rows), "{rows}");
        assert_eq!(
            s.rows_for("https://example.com/none.jpg", Variant::Native, 60, 10),
            1,
            "placeholder line"
        );
    }

    #[test]
    fn fetches_are_limited_in_flight() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        ressac_core::service::Ressac::open(&db).unwrap();
        let mut s = store();
        for i in 0..6 {
            s.want(&format!("http://127.0.0.1:9/{i}.png"));
        }
        s.start_wanted(&db);
        assert_eq!(s.in_flight(), MAX_IN_FLIGHT);
        assert_eq!(s.queued(), 2);
    }

    #[test]
    fn a_disabled_store_fetches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        let mut s = ImageStore::disabled();
        s.want("https://example.com/a.jpg");
        s.fetch(&db, "https://example.com/a.jpg".into());
        assert!(s.take_wanted().is_empty() && s.in_flight() == 0);
        assert_eq!(s.state_of("https://example.com/a.jpg"), ImageState::Absent);
    }

    fn dummy_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        ressac_core::service::Ressac::open(&db).unwrap();
        (dir, db)
    }

    #[test]
    fn a_tall_image_is_kept_within_the_size_cap() {
        let tall = DynamicImage::ImageRgb8(RgbImage::new(1600, 4000));
        let p = prepare(tall);
        assert!(p.kept.width() <= MAX_KEPT_SIDE && p.kept.height() <= MAX_KEPT_SIDE);
        assert_eq!(p.kept.height(), 1600, "scaled to fit, aspect kept");
        assert!(p.small.width() <= 320 && p.small.height() <= 320);
        let small = DynamicImage::new_rgb8(40, 30);
        assert_eq!(
            (prepare(small).kept.width(), 40),
            (40, 40),
            "never scaled up"
        );
    }

    #[test]
    fn the_teletext_copy_is_built_only_when_asked_for() {
        let mut s = store();
        s.ingest("https://example.com/a.jpg", photo());
        let url = "https://example.com/a.jpg";
        let i = s.position(url).unwrap();
        assert!(matches!(&s.slots[i].1, Slot::Ready { teletext: None, .. }));
        assert!(s.rows_for(url, Variant::Teletext, 40, 10) >= 1);
        assert!(
            matches!(&s.slots[i].1, Slot::Ready { teletext: None, .. }),
            "sizing builds nothing"
        );
        s.protocol(url, Variant::Native);
        assert!(matches!(
            &s.slots[s.position(url).unwrap()].1,
            Slot::Ready { teletext: None, .. }
        ));
        s.protocol(url, Variant::Teletext);
        assert!(matches!(
            &s.slots[s.position(url).unwrap()].1,
            Slot::Ready {
                teletext: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn only_permanent_refusals_are_final() {
        use ressac_core::Error;
        let refused = |why: &str| is_permanent(&Error::ImageRefused(why.into()));
        for why in [
            "not a web address",
            "local or private address",
            "larger than 10 MB",
            "not an image",
            "HTTP 404",
            "HTTP 410",
        ] {
            assert!(refused(why), "{why}");
        }
        for why in [
            "timed out",
            "Dns",
            "Connect",
            "HTTP 503",
            "HTTP 429",
            "HTTP 408",
        ] {
            assert!(!refused(why), "{why}");
        }
        assert!(!is_permanent(&Error::EditionNotDue), "not about the image");
    }

    fn ended(s: &mut ImageStore, url: &str, permanent: bool) {
        s.mark_loading(url);
        s.in_flight = 1;
        s.tx.send(Fetched {
            url: url.into(),
            result: Err(Failure {
                permanent,
                why: "x".into(),
            }),
        })
        .unwrap();
    }

    #[test]
    fn a_transient_failure_is_asked_again_and_a_permanent_one_is_not() {
        let (_dir, db) = dummy_db();
        let (a, b) = ("https://example.com/a.png", "https://example.com/b.png");
        let mut s = store();
        s.retry_after = Duration::ZERO;
        ended(&mut s, a, false);
        ended(&mut s, b, true);
        let events = s.drain(&db);
        assert_eq!(events.len(), 2);
        assert_eq!(s.state_of(a), ImageState::Absent, "retry due");
        assert_eq!(s.state_of(b), ImageState::Failed);
        s.want(a);
        s.want(b);
        assert_eq!(s.take_wanted(), vec![a.to_string()]);

        let mut waiting = store();
        ended(&mut waiting, a, false);
        waiting.drain(&db);
        assert_eq!(
            waiting.state_of(a),
            ImageState::Failed,
            "alt text while it waits"
        );
        waiting.want(a);
        assert!(waiting.take_wanted().is_empty(), "not before the delay");
    }

    #[test]
    fn failed_images_are_capped() {
        let mut s = store();
        for i in 0..300 {
            s.fail(&format!("https://example.com/{i}.png"));
        }
        assert_eq!(s.state_of("https://example.com/0.png"), ImageState::Absent);
        assert_eq!(
            s.state_of("https://example.com/299.png"),
            ImageState::Failed
        );
        assert_eq!(s.slots.len(), MAX_FAILED);
    }

    #[test]
    fn the_queue_follows_the_current_frame() {
        let (_dir, db) = dummy_db();
        let mut s = store();
        for i in 0..6 {
            s.want(&format!("http://127.0.0.1:9/old{i}.png"));
        }
        s.start_wanted(&db);
        assert_eq!((s.in_flight(), s.queued()), (MAX_IN_FLIGHT, 2));
        s.want("http://127.0.0.1:9/new.png");
        s.start_wanted(&db);
        assert_eq!(
            s.queued(),
            1,
            "the old queue is replaced by the new frame's list"
        );
        assert_eq!(
            s.queue.front().map(String::as_str),
            Some("http://127.0.0.1:9/new.png")
        );
    }

    #[test]
    fn fetch_waits_when_the_workers_are_all_busy() {
        let (_dir, db) = dummy_db();
        let mut s = store();
        for i in 0..5 {
            s.fetch(&db, format!("http://127.0.0.1:9/{i}.png"));
        }
        assert_eq!((s.in_flight(), s.queued()), (MAX_IN_FLIGHT, 1));
    }
}
