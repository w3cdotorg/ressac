# Phase 6: Images in the reader

**Runs after:** phase 5b.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §7 "Images" (picker, fallbacks, guarded fetch through `Ressac::fetch_image`, decode limits, lazy loading, at most half the reader's height, a cache of 32) for the Native look, plus the teletext quantisation phase 9 needs. Review Focus 4 (the decode side: a 20 000 × 20 000 image and an HTML body are refused, nothing hangs) belongs here; the network side (redirect to a private address, 50 MB, timeout) is phase 2's.

**Goal of the phase:** in Ghostty under tmux, an article's photos show inline (Kitty protocol through `allow-passthrough`); without a graphics protocol they show as coloured half-blocks; while loading, `[loading image…]`; when they cannot load, or with `i` (images off), `[image: alt text]`. The keyboard never freezes at start (clappuccino's picker fixes).

**Builds on:**
- Phase 2: `Ressac::fetch_image(&self, url: &str) -> ressac_core::Result<Vec<u8>>` (http/https only, private addresses refused, 10 MB, 15 s), `IMAGE_MAX_BYTES`, `IMAGE_TIMEOUT`, `Error::ImageRefused(String)`.
- Phase 3: `RenderedArticle { lines, links, images }`, `ImageRef { url, alt, line }`; at `ImageRef.line`, `render_html` emitted the line `[image: alt text]`.
- Phase 5: `native::{render, RenderCache, ReaderMetrics}` (`render(frame, state, cache) -> ReaderMetrics`, the reader drawn as a scrolled `Paragraph`), `ReaderView.images` (the `i` toggle, from `TerminalSettings.images`), `ReaderView.reader_scroll`, `tui::reader::run(app, db, settings_path)` with `ratatui::try_init()` then the loop, `Command::FetchImage(String)` (executor arm empty since 5b), `Event::ImageReady { url }`, `Event::ImageFailed { url }`, `commands::open(db)` (`pub(crate)`).

**Reference (read before coding):** `~/Sites/LearningShow16-TUI/crates/wooclap-cli/src/preview.rs` (`PreviewState::new`: the query, the Warp override, the DA1 decoy; `ingest`; `render_key` with `StatefulImage::<StatefulProtocol>::default()`), and the comment above `ratatui::init()` in `app.rs` (why the picker comes after init).

**ratatui-image 11.1.0 API used** (checked in the crate source): `ratatui_image::picker::{Picker, ProtocolType}` (`Picker::from_query_stdio() -> Result<Picker>`, `Picker::halfblocks()`, `protocol_type()`, `set_protocol_type()`, `font_size() -> FontSize { width, height }`, `new_resize_protocol(DynamicImage) -> StatefulProtocol`; `Picker: Clone`), `ratatui_image::protocol::StatefulProtocol` (`size_for(Resize, Size) -> Size`), `ratatui_image::{StatefulImage, Resize}` (`StatefulImage::<StatefulProtocol>::default()` fits with `Resize::Fit(None)`). `Picker::from_fontsize` is deprecated: tests use `Picker::halfblocks()`, which never queries the terminal.

**image 0.25.10 API used:** `image::ImageReader::new(Cursor)`, `.with_guessed_format()`, `.limits(Limits)`, `.decode()`; `image::Limits { max_image_width, max_image_height, max_alloc }` (fields are public; start from `Limits::default()`), `DynamicImage::thumbnail(w, h)`, `to_rgba8()`.

---

**Files (5):**
- Modify: `Cargo.toml` (workspace dependencies)
- Modify: `crates/ressac/Cargo.toml`
- Create: `crates/ressac/src/tui/reader/images.rs` (picker, decode limits, `ImageStore`, teletext quantisation, fetch threads; tests inline)
- Modify: `crates/ressac/src/tui/reader/native.rs` (the reader drawn as blocks of text lines and images; new snapshot)
- Modify: `crates/ressac/src/tui/reader/mod.rs` (`pub mod images;`, picker after init, `FetchImage`, draining image results)

The image results travel on `ImageStore`'s own channel, so `jobs.rs` (and its `Msg`) stay untouched.

**Produces** (phase 9 relies on these):

```rust
pub const TELETEXT: [[u8; 3]; 8]; // black, red, green, yellow, blue, magenta, cyan, white
pub const MAX_DECODED_SIDE: u32 = 4096;
pub const CACHE_SIZE: usize = 32;
pub const MAX_IN_FLIGHT: usize = 4;
pub fn quantise_teletext(img: &image::DynamicImage) -> image::DynamicImage;
pub fn decode_limited(bytes: &[u8]) -> Result<image::DynamicImage, String>;
pub fn detect_picker() -> Picker; // only after ratatui::try_init()
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Variant { Native, Teletext }
pub struct ImageStore { /* picker, teletext picker, LRU slots, wanted, queue, channel */ }
impl ImageStore {
    pub fn new(picker: Picker) -> Self;
    pub fn has_graphics_protocol(&self) -> bool;
    pub fn want(&mut self, url: &str);                       // the renderer asks for an image near the viewport
    pub fn start_wanted(&mut self, db: &Path);                // the loop starts fetches (at most MAX_IN_FLIGHT)
    pub fn fetch(&mut self, db: &Path, url: String);          // Command::FetchImage
    pub fn drain(&mut self, db: &Path) -> Vec<crate::tui::reader::state::Event>; // ImageReady / ImageFailed, then queued fetches start
    pub fn ingest(&mut self, url: &str, image: image::DynamicImage);
    pub fn fail(&mut self, url: &str);
    pub fn rows_for(&self, url: &str, variant: Variant, width: u16, max_rows: u16) -> u16;
    pub fn protocol(&mut self, url: &str, variant: Variant) -> Option<&mut StatefulProtocol>;
    pub fn state_of(&self, url: &str) -> ImageState;          // Absent, Loading, Ready, Failed
}
// native.rs
pub fn render(frame: &mut Frame, state: &ReaderState, cache: &mut RenderCache, images: &mut ImageStore) -> ReaderMetrics;
```

**Reconciled with the other phases (controller, 2026-10-16; these rules override the code below where they differ):**
- Phase 5's `RenderCache` is dropped (the article is `state.article`): the final signature is `native::render(frame: &mut Frame, state: &ReaderState, images: &mut ImageStore) -> ReaderMetrics`; remove the `cache` parameter wherever this file passes it.
- `Variant { Native, Teletext }` is the one image-variant name for the whole plan; the cache is keyed by `(url, Variant)`. Phase 9 does not add an `ImageMode`: Antiope uses `Variant::Teletext` by default and `Variant::Native` while its `i` toggle shows the real image.
- This phase also provides `ImageStore::disabled() -> ImageStore` (no picker, no fetch; every image stays `[image: alt text]`), used by phase 9's rendering tests; add it with a one-line test.
- Dates and clocks come from `state.local` (phase 4b), never `chrono::Local`.

### Task 6.1: dependencies, decoding and quantisation

- [ ] **Step 1: Add the dependencies.** Root `Cargo.toml`, in `[workspace.dependencies]`, alphabetical order:

```toml
image = "0.25.10"
ratatui-image = { version = "11.1.0", default-features = false, features = ["crossterm", "image-defaults"] }
```

`crates/ressac/Cargo.toml`, in `[dependencies]`:

```toml
image.workspace = true
ratatui-image.workspace = true
```

Run: `cargo tree -p ressac -i ratatui | head -5`
Expected: one `ratatui v0.30.2` (ratatui-image must not pull a second ratatui). If it does, stop and report: the versions in the Global Constraints are the ones clappuccino runs together.

- [ ] **Step 2: Write the failing tests** in a new `crates/ressac/src/tui/reader/images.rs` (add `pub mod images;` to `tui/reader/mod.rs`).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
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
        src.put_pixel(0, 0, Rgb([250, 10, 10]));   // red
        src.put_pixel(1, 0, Rgb([20, 30, 200]));   // blue
        src.put_pixel(2, 0, Rgb([240, 240, 30]));  // yellow
        src.put_pixel(3, 0, Rgb([128, 128, 128])); // grey → the nearest of the eight
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
}
```

- [ ] **Step 3: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::images`
Expected: FAIL to compile, `cannot find function decode_limited` / `quantise_teletext` / `TELETEXT`.

- [ ] **Step 4: Implement** the top of `images.rs`.

```rust
//! Images in the reader: protocol detection, guarded fetch, bounded decoding, a small cache,
//! and the eight-colour quantisation Antiope draws with.

use std::collections::VecDeque;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use image::{DynamicImage, ImageReader, Limits, Rgb, RgbImage};
use ratatui::layout::Size;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::Resize;

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
pub fn quantise_teletext(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let [r, g, b, a] = p.0;
        let px = if a < 128 { [0, 0, 0] } else { nearest_teletext([r, g, b]) };
        out.put_pixel(x, y, Rgb(px));
    }
    DynamicImage::ImageRgb8(out)
}

fn nearest_teletext(c: [u8; 3]) -> [u8; 3] {
    let dist = |t: &[u8; 3]| -> u32 {
        t.iter().zip(c).map(|(a, b)| (i32::from(*a) - i32::from(b)).pow(2) as u32).sum()
    };
    *TELETEXT.iter().min_by_key(|t| dist(t)).expect("eight colours")
}
```

The error text of a limit refusal comes from `image` (`ImageError::Limits`, displayed as "The image ... limit ..."); if its wording does not contain "limit", match the variant instead: make the test assert on `decode_limited` returning `Err` and on the error starting the way `image::error::LimitError`'s `Display` does, and say so in the report.

- [ ] **Step 5: Run the tests.**

Run: `cargo test -p ressac --bin ressac tui::reader::images`
Expected: PASS, 5 tests.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml Cargo.lock crates/ressac/Cargo.toml crates/ressac/src/tui/reader/images.rs crates/ressac/src/tui/reader/mod.rs
git commit -m "Reader images: bounded decoding and the eight teletext colours

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6.2: the picker and the image store

- [ ] **Step 1: Write the failing tests** (same `mod tests`). They never query the terminal: `Picker::halfblocks()` only reads environment variables.

```rust
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
        assert_eq!(s.take_wanted(), vec!["https://example.com/a.jpg".to_string()], "asked once");
        s.mark_loading("https://example.com/a.jpg");
        assert_eq!(s.state_of("https://example.com/a.jpg"), ImageState::Loading);
        s.ingest("https://example.com/a.jpg", photo());
        assert_eq!(s.state_of("https://example.com/a.jpg"), ImageState::Ready);
        assert!(s.protocol("https://example.com/a.jpg", Variant::Native).is_some());
        assert!(s.protocol("https://example.com/a.jpg", Variant::Teletext).is_some());
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
            s.ingest(&format!("https://example.com/{i}.png"), DynamicImage::new_rgb8(8, 8));
        }
        assert_eq!(s.state_of("https://example.com/0.png"), ImageState::Absent, "evicted");
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
        assert_eq!(s.rows_for("https://example.com/none.jpg", Variant::Native, 60, 10), 1, "placeholder line");
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
```

The last test opens real threads that fail fast (port 9 refused, or refused by the private-address guard): it checks only the counters right after `start_wanted`, before any result arrives, so it does not wait on the network.

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::images`
Expected: FAIL to compile, `cannot find struct ImageStore`.

- [ ] **Step 3: Implement** below the quantisation.

```rust
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
    Ready { native: StatefulProtocol, teletext: StatefulProtocol },
    Failed,
}

/// What a fetch thread sends back.
struct Fetched {
    url: String,
    result: Result<DynamicImage, String>,
}

pub struct ImageStore {
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

    pub fn has_graphics_protocol(&self) -> bool {
        matches!(self.picker.protocol_type(), ProtocolType::Kitty | ProtocolType::Iterm2 | ProtocolType::Sixel)
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
        if self.state_of(url) == ImageState::Absent && !self.wanted.iter().any(|w| w == url) && !self.queue.iter().any(|q| q == url) {
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

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Starts the wanted fetches, at most `MAX_IN_FLIGHT` at a time; the rest wait in a queue.
    pub fn start_wanted(&mut self, db: &Path) {
        let wanted = self.take_wanted();
        self.queue.extend(wanted);
        while self.in_flight < MAX_IN_FLIGHT {
            let Some(url) = self.queue.pop_front() else { break };
            self.fetch(db, url);
        }
    }

    /// One fetch in its own thread and connection (`Ressac::fetch_image`), decoded there.
    pub fn fetch(&mut self, db: &Path, url: String) {
        if self.state_of(&url) != ImageState::Absent {
            return;
        }
        self.mark_loading(&url);
        self.in_flight += 1;
        let tx = self.tx.clone();
        let db: PathBuf = db.to_path_buf();
        std::thread::spawn(move || {
            let result = fetch_and_decode(&db, &url);
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
                Err(_) => {
                    self.fail(&url);
                    events.push(Event::ImageFailed { url });
                }
            }
        }
        while self.in_flight < MAX_IN_FLIGHT {
            let Some(url) = self.queue.pop_front() else { break };
            self.fetch(db, url);
        }
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
        self.slots.push((url.to_owned(), Slot::Ready { native, teletext }));
        self.evict();
    }

    pub fn fail(&mut self, url: &str) {
        if let Some(i) = self.position(url) {
            self.slots.remove(i);
        }
        self.slots.push((url.to_owned(), Slot::Failed));
    }

    fn evict(&mut self) {
        while self.slots.iter().filter(|(_, s)| matches!(s, Slot::Ready { .. })).count() > CACHE_SIZE {
            let oldest = self.slots.iter().position(|(_, s)| matches!(s, Slot::Ready { .. })).expect("over the cap");
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
        let Some(i) = self.position(url) else { return 1 };
        match &self.slots[i].1 {
            Slot::Ready { native, teletext } => {
                let proto = match variant {
                    Variant::Native => native,
                    Variant::Teletext => teletext,
                };
                proto.size_for(Resize::Fit(None), Size::new(width, max_rows.max(1))).height.max(1)
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
    let bytes = runtime.block_on(app.fetch_image(url)).map_err(|e| e.to_string())?;
    decode_limited(&bytes)
}
```

`drain` takes the database path to start the queued fetches.

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ressac --bin ressac tui::reader::images`
Expected: PASS, 11 tests.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/images.rs
git commit -m "Reader images: the picker after init, a cache of 32, four fetches at a time

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6.3: images in the Native reader and in the loop

The reader stops being one scrolled `Paragraph`: it becomes a list of blocks, each one text line or one image of some rows; only the blocks in the viewport are drawn. An image is drawn only when it fits entirely in the viewport (a protocol image cannot be cut); partly visible, its rows stay blank. Images within one viewport above and two below the visible rows are wanted (lazy loading).

- [ ] **Step 1: Write the failing tests** in `native.rs`'s `mod tests` (reuse its `state`, `row` and `draw` helpers; `draw` now builds an `ImageStore::new(Picker::halfblocks())`).

```rust
    use crate::tui::reader::images::{ImageState, ImageStore, Variant};
    use image::{DynamicImage, Rgb, RgbImage};
    use ratatui_image::picker::Picker;

    const WITH_IMAGE: &str = "<p>Avant.</p><img src=\"https://example.com/photo.jpg\" alt=\"Le port\"><p>Après.</p>";

    fn image_state(width: u16, height: u16, images_on: bool) -> ReaderState {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: images_on }, width, height);
        s.on_event(Event::Sources(snapshot()));
        let r = row(21, "Photos", false);
        s.on_event(Event::Entries {
            source: Source::Unread,
            page: EntryPage { total: 1, unread: 1, rows: vec![r.clone()], next: None },
            appended: false,
        });
        s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        s.on_event(Event::Entry(EntryDetail {
            row: r,
            summary_html: None,
            content_html: Some(WITH_IMAGE.into()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        }));
        s
    }

    fn draw_with(s: &ReaderState, images: &mut ImageStore, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut cache = RenderCache::default();
        terminal.draw(|f| { render(f, s, &mut cache, images); }).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn an_image_not_loaded_yet_is_wanted_and_shows_loading() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        let out = draw_with(&s, &mut images, 140, 30);
        assert!(out.contains("[loading image…]"), "{out}");
        assert_eq!(images.take_wanted(), vec!["https://example.com/photo.jpg".to_string()]);
    }

    #[test]
    fn a_ready_image_is_drawn_in_half_blocks_without_a_protocol() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.ingest(
            "https://example.com/photo.jpg",
            DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([200, 120, 40]))),
        );
        let out = draw_with(&s, &mut images, 140, 30);
        assert!(out.contains('▀') || out.contains('▄'), "half-blocks drawn: {out}");
        assert!(!out.contains("[loading image…]"));
        assert!(images.rows_for("https://example.com/photo.jpg", Variant::Native, 72, 13) <= 13);
    }

    #[test]
    fn a_failed_image_or_images_off_show_the_alt_text() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.fail("https://example.com/photo.jpg");
        assert!(draw_with(&s, &mut images, 140, 30).contains("[image: Le port]"));

        let off = image_state(140, 30, false);
        let mut fresh = ImageStore::new(Picker::halfblocks());
        let out = draw_with(&off, &mut fresh, 140, 30);
        assert!(out.contains("[image: Le port]"));
        assert!(fresh.take_wanted().is_empty(), "images off: nothing is fetched");
        assert_eq!(fresh.state_of("https://example.com/photo.jpg"), ImageState::Absent);
    }

    #[test]
    fn native_reader_with_an_image() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.ingest(
            "https://example.com/photo.jpg",
            DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([200, 120, 40]))),
        );
        insta::assert_snapshot!(draw_with(&s, &mut images, 140, 30));
    }
```

Update the five 5a tests' `draw` helper to pass `&mut ImageStore::new(Picker::halfblocks())`: their snapshots must not change (the articles of 5a have no image), which is the regression check of the block renderer.

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::native`
Expected: FAIL to compile, `render` takes 3 arguments but 4 were supplied.

- [ ] **Step 3: Implement in `native.rs`.** Change `render`'s signature and replace `render_reader`'s `Paragraph` with blocks:

```rust
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;
use super::images::{ImageState, ImageStore, Variant};

pub const LOADING_IMAGE: &str = "[loading image…]";

enum Piece {
    Text(Line<'static>),
    Image { url: String, alt: String, rows: u16 },
}

pub fn render(
    frame: &mut Frame,
    state: &ReaderState,
    cache: &mut RenderCache,
    images: &mut ImageStore,
) -> ReaderMetrics {
    // … unchanged until the reader column …
    match rects.reader {
        Some(r) => render_reader(frame, &view, cache, images, r, view.focus == Column::Reader),
        None => ReaderMetrics::default(),
    }
}

fn render_reader(
    frame: &mut Frame,
    view: &ReaderView<'_>,
    cache: &mut RenderCache,
    images: &mut ImageStore,
    area: Rect,
    focused: bool,
) -> ReaderMetrics {
    let block = frame_block("Article", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(entry) = view.entry else {
        frame.render_widget(Paragraph::new(Span::styled("No entry selected", DIM)), inner);
        return ReaderMetrics { total_rows: 0, viewport_rows: inner.height };
    };
    let width = inner.width.min(MAX_TEXT_WIDTH);
    let max_image_rows = (inner.height / 2).max(1);
    let mut pieces: Vec<Piece> = header_lines(entry, view).into_iter().map(Piece::Text).collect();
    let article = cache.article(entry, view.show_fulltext, width);
    for (i, line) in article.lines.iter().enumerate() {
        match article.images.iter().find(|img| img.line == i) {
            Some(img) if view.images && images.state_of(&img.url) != ImageState::Failed => {
                let rows = images.rows_for(&img.url, Variant::Native, width, max_image_rows);
                pieces.push(Piece::Image { url: img.url.clone(), alt: img.alt.clone(), rows });
            }
            _ => pieces.push(Piece::Text(line.clone())),
        }
    }
    if !article.links.is_empty() {
        pieces.push(Piece::Text(Line::raw("")));
        pieces.push(Piece::Text(Line::styled("Links", Style::new().add_modifier(Modifier::BOLD))));
        for (i, link) in article.links.iter().enumerate() {
            pieces.push(Piece::Text(Line::styled(format!("[{}] {}", i + 1, printable(link)), DIM)));
        }
    }
    let viewport = usize::from(inner.height);
    let scroll = view.reader_scroll;
    let mut top = 0usize; // first row of the current piece
    for piece in pieces.iter() {
        let rows = match piece {
            Piece::Text(_) => 1,
            Piece::Image { rows, .. } => usize::from(*rows),
        };
        if let Piece::Image { url, .. } = piece {
            // Wanted from one viewport above to two below the visible rows.
            let near = top + rows + viewport >= scroll && top <= scroll + 2 * viewport;
            if near && images.state_of(url) == ImageState::Absent {
                images.want(url);
            }
        }
        let visible = top >= scroll && top + rows <= scroll + viewport;
        if visible {
            let y = inner.y + u16::try_from(top - scroll).unwrap_or(u16::MAX);
            let height = u16::try_from(rows).unwrap_or(u16::MAX);
            let place = Rect { x: inner.x, y, width, height };
            match piece {
                Piece::Text(line) => frame.render_widget(Paragraph::new(line.clone()), place),
                Piece::Image { url, alt, .. } => match images.state_of(url) {
                    ImageState::Ready => {
                        if let Some(proto) = images.protocol(url, Variant::Native) {
                            frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::default(), place, proto);
                        }
                    }
                    ImageState::Failed => frame.render_widget(
                        Paragraph::new(Span::styled(format!("[image: {}]", printable(alt)), DIM)),
                        place,
                    ),
                    _ => frame.render_widget(Paragraph::new(Span::styled(LOADING_IMAGE, DIM)), place),
                },
            }
        }
        top += rows;
    }
    ReaderMetrics { total_rows: top, viewport_rows: inner.height }
}
```

When images are off, the line `render_html` emitted at `ImageRef.line` (`[image: alt text]`) is drawn as text: that is the off state. If phase 3 emits a different fallback line, keep its text for the off state and use `[image: alt]` only for failed images.

- [ ] **Step 4: Wire the loop** (`tui/reader/mod.rs`): `pub mod images;` (done in 6.1), then in `run`, right after `ratatui::try_init()` and **never before it**:

```rust
    let mut images = images::ImageStore::new(images::detect_picker());
```

pass `&mut images` to `event_loop` and to `native::render`, and in the loop:

```rust
        // after terminal.draw(...)
        images.start_wanted(db);
        for event in images.drain(db) {
            pending.extend(state.on_event(event));
        }
```

and in the command dispatch, before `exec.run(other, …)`:

```rust
                Command::FetchImage(url) => images.fetch(db, url),
```

(the executor's empty `FetchImage` arm stays for completeness; the loop handles it first). `Event::ImageReady` makes the state request a redraw (it already redraws every turn; no state change is needed).

- [ ] **Step 5: Run the tests, review and accept the new snapshot.**

Run: `cargo test -p ressac --bin ressac tui::reader`, then review `native_reader_with_an_image` (half-blocks under `Avant.`, `Après.` after them, the image at most 13 rows high in a 30-row pane).
Expected: PASS; the 5a snapshots unchanged.

- [ ] **Step 6: Gate, look once, commit.**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
SCRATCH=$(mktemp -d); sqlite3 -readonly "$HOME/Library/Application Support/Ressac/ressac.sqlite" ".backup '$SCRATCH/r.sqlite'"
cargo run -q -p ressac -- --db "$SCRATCH/r.sqlite" read
```

In Ghostty inside tmux (`allow-passthrough on`): open an article with photos; they show; `i` hides them; the first key after start is not swallowed. In Terminal.app: half-blocks. Report what you saw (no scripted keystrokes).

```bash
git add crates/ressac/src/tui/reader/native.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader: images inline, lazy, at most half the reader's height, alt text when they fail or are off

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

## Deviations from the plan index (to reconcile)

1. `native::render(frame, state, cache, images: &mut ImageStore)` (the index has `images: &ImageStore`): drawing a `StatefulImage` needs `&mut`.
2. `ImageStore::drain(&mut self, db: &Path)` takes the database path to start queued fetches; image results travel on the store's own channel, not on `jobs::Msg`.
3. Fetches are started by the renderer's `want` list (lazy loading needs the layout); `Command::FetchImage` is still honoured by the loop.
4. New public names: `Variant`, `ImageState`, `MAX_IN_FLIGHT`, `LOADING_IMAGE`, `take_wanted`, `mark_loading`, `in_flight`, `queued`, `state_of`.
