//! Images in the reader: protocol detection, guarded fetch, bounded decoding, a small cache,
//! and the eight-colour quantisation Antiope draws with.

use std::collections::VecDeque;
use std::io::{Cursor, Write};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

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
/// Images are scaled down to this width before the protocol encodes them (memory).
const MAX_KEPT_WIDTH: u32 = 1600;
/// Antiope's half-blocks need few pixels: the teletext copy is a thumbnail.
const TELETEXT_WIDTH: u32 = 320;
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

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
        teletext: Box<StatefulProtocol>,
    },
    Failed,
}

/// What a fetch thread sends back.
struct Fetched {
    url: String,
    result: Result<DynamicImage, String>,
}

pub struct ImageStore {
    /// `false` for `disabled()`: nothing is fetched and every image stays its alt text.
    enabled: bool,
    picker: Picker,
    teletext: Picker,
    /// Least recently used first; only `Ready` slots count towards `CACHE_SIZE`.
    slots: Vec<(String, Slot)>,
    wanted: Vec<String>,
    queue: VecDeque<String>,
    in_flight: usize,
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

    pub fn state_of(&self, url: &str) -> ImageState {
        match self.position(url).map(|i| &self.slots[i].1) {
            None => ImageState::Absent,
            Some(Slot::Loading) => ImageState::Loading,
            Some(Slot::Ready { .. }) => ImageState::Ready,
            Some(Slot::Failed) => ImageState::Failed,
        }
    }

    /// The renderer's request for an image near the viewport; absent images only, once.
    pub fn want(&mut self, url: &str) {
        if self.enabled
            && self.state_of(url) == ImageState::Absent
            && !self.wanted.iter().any(|w| w == url)
            && !self.queue.iter().any(|q| q == url)
        {
            self.wanted.push(url.to_owned());
        }
    }

    pub fn take_wanted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.wanted)
    }

    pub fn mark_loading(&mut self, url: &str) {
        if self.position(url).is_none() {
            self.slots.push((url.to_owned(), Slot::Loading));
        }
    }

    #[cfg(test)]
    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Starts the wanted fetches, at most `MAX_IN_FLIGHT` at a time; the rest wait in a queue.
    pub fn start_wanted(&mut self, db: &Path) {
        let wanted = self.take_wanted();
        self.queue.extend(wanted);
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

    /// One fetch in its own thread and connection (`Ressac::fetch_image`), decoded there. Not a
    /// long job: it never touches the loop's job queue.
    pub fn fetch(&mut self, db: &Path, url: String) {
        if !self.enabled || self.state_of(&url) != ImageState::Absent {
            return;
        }
        self.mark_loading(&url);
        self.in_flight += 1;
        let tx = self.tx.clone();
        let db: PathBuf = db.to_path_buf();
        std::thread::spawn(move || {
            // A decoder panic on a hostile file must still free the slot.
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| fetch_and_decode(&db, &url)))
                .unwrap_or_else(|_| Err("the image could not be decoded".into()));
            let _ = tx.send(Fetched { url, result });
        });
    }

    /// The fetches that ended since the last call, as state events; the queue moves on.
    pub fn drain(&mut self, db: &Path) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(Fetched { url, result }) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match result {
                Ok(image) => {
                    self.ingest(&url, image);
                    events.push(Event::ImageReady { url });
                }
                Err(e) => {
                    tracing::debug!("image {url}: {e}");
                    self.fail(&url);
                    events.push(Event::ImageFailed { url });
                }
            }
        }
        self.start_queued(db);
        events
    }

    pub fn ingest(&mut self, url: &str, image: DynamicImage) {
        let kept = if image.width() > MAX_KEPT_WIDTH {
            image.thumbnail(MAX_KEPT_WIDTH, MAX_KEPT_WIDTH * 4)
        } else {
            image
        };
        let small = kept.thumbnail(TELETEXT_WIDTH, TELETEXT_WIDTH * 4);
        let teletext = self.teletext.new_resize_protocol(quantise_teletext(&small));
        let native = self.picker.new_resize_protocol(kept);
        if let Some(i) = self.position(url) {
            self.slots.remove(i);
        }
        self.slots.push((
            url.to_owned(),
            Slot::Ready {
                native: Box::new(native),
                teletext: Box::new(teletext),
            },
        ));
        self.evict();
    }

    pub fn fail(&mut self, url: &str) {
        if let Some(i) = self.position(url) {
            self.slots.remove(i);
        }
        self.slots.push((url.to_owned(), Slot::Failed));
    }

    fn evict(&mut self) {
        let ready = |s: &(String, Slot)| matches!(s.1, Slot::Ready { .. });
        while self.slots.iter().filter(|s| ready(s)).count() > CACHE_SIZE {
            let oldest = self
                .slots
                .iter()
                .position(ready)
                .expect("over the cap, so one is ready");
            self.slots.remove(oldest);
        }
    }

    /// The protocol to draw; touching it makes the image the most recently used.
    pub fn protocol(&mut self, url: &str, variant: Variant) -> Option<&mut StatefulProtocol> {
        let i = self.position(url)?;
        let slot = self.slots.remove(i);
        self.slots.push(slot);
        match &mut self.slots.last_mut().expect("just pushed").1 {
            Slot::Ready { native, teletext } => Some(match variant {
                Variant::Native => native,
                Variant::Teletext => teletext,
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
        match &self.slots[i].1 {
            Slot::Ready { native, teletext } => {
                let proto = match variant {
                    Variant::Native => native,
                    Variant::Teletext => teletext,
                };
                proto
                    .size_for(Resize::Fit(None), Size::new(width, max_rows.max(1)))
                    .height
                    .clamp(1, max_rows.max(1))
            }
            _ => 1,
        }
    }
}

fn fetch_and_decode(db: &Path, url: &str) -> Result<DynamicImage, String> {
    let app = crate::commands::open(db).map_err(|e| format!("{e:#}"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = runtime
        .block_on(app.fetch_image(url))
        .map_err(|e| e.to_string())?;
    decode_limited(&bytes)
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
}
