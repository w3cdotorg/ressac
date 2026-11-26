//! SAA5050 mosaics (the Mac's `MosaicRenderer` and `MosaicSchemeHandler`, spec §7): a picture
//! as two-colour block cells of 2 × 3 sub-pixels, drawn as a PNG with one pixel per font
//! pixel. Pure except `fetch`, which goes through `ressac-core`'s guarded image download.
//!
//! Geometry follows Bedstead and the SAA5050: a cell is 6 × 10 font pixels, split into
//! sub-pixel columns of 3 + 3 and rows of 3 + 4 + 3. Each cell takes the pair of teletext
//! colours that best fits its six sub-pixels by least squared RGB error. Canonical form: a
//! cell of one colour has `fg == bg` and `mask == 0`; otherwise the background is the colour
//! of more sub-pixels, and on a 3–3 tie the one listed first in `Color`.

use std::collections::HashMap;
use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::imageops::{self, FilterType};
use image::{
    ColorType, DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat,
    ImageReader, Limits, RgbImage, RgbaImage,
};
use ressac_core::Error;
use ressac_core::service::Ressac;

use crate::sanitize::is_web_url;
use crate::teletext::Color;

pub const SCHEME: &str = "ressac-mosaic";
pub const CELL_WIDTH: usize = 6;
pub const CELL_HEIGHT: usize = 10;
/// Sub-pixel boundaries inside a cell, in font pixels.
pub const COLUMN_EDGES: [usize; 3] = [0, 3, 6];
pub const ROW_EDGES: [usize; 4] = [0, 3, 7, 10];
/// Widest mosaic, in cells.
pub const MAX_COLUMNS: usize = 40;
/// Tallest mosaic, in cells: a strip-shaped image must not make the page endless.
pub const MAX_ROWS: usize = 100;
/// Image pixels per cell when sizing a picture: a 120-pixel picture gets 10 columns.
pub const PIXELS_PER_COLUMN: f64 = 12.0;
/// Images with more pixels are refused unread (a PNG of 190 KB can announce 8000 × 8000).
/// `decode` also checks the memory the colour type really needs against `MAX_DECODE_ALLOC`.
pub const MAX_SOURCE_PIXELS: u64 = 16_000_000;
pub const CACHE_BYTES: usize = 16 << 20;
/// Downloads at once (the scheme handler's limit).
pub const MAX_IN_FLIGHT: usize = 4;
/// The most memory a decoder may allocate.
const MAX_DECODE_ALLOC: u64 = 128 << 20;
/// What a cached failure costs.
const FAILURE_COST: usize = 64;

/// One mosaic cell. `mask` bit 0 is the top-left sub-pixel, then row-major (1 top-right,
/// 2 middle-left, 3 middle-right, 4 bottom-left, 5 bottom-right); a set bit is foreground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MosaicCell {
    pub fg: Color,
    pub bg: Color,
    pub mask: u8,
}

/// A picture as mosaics: `columns × rows` cells, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicImage {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<MosaicCell>,
}

/// How many columns an image `width` pixels wide gets: `ceil(width / 12)`, 1 to `max_columns`.
pub fn columns_for_width(width: u32, max_columns: usize) -> usize {
    let wanted = (f64::from(width.max(1)) / PIXELS_PER_COLUMN).ceil() as usize;
    wanted.clamp(1, max_columns.max(1))
}

/// The cell grid for a `width × height` image `columns` wide. Cells are 0.6 em by 1 em, so
/// `rows = 0.6 × columns × height / width`; past `MAX_ROWS` the columns shrink to keep the shape.
pub fn grid(width: u32, height: u32, columns: usize) -> (usize, usize) {
    let aspect = f64::from(height.max(1)) / f64::from(width.max(1));
    let rows_for = |columns: usize| ((0.6 * columns as f64 * aspect).round() as usize).max(1);
    let mut columns = columns.max(1);
    if rows_for(columns) > MAX_ROWS {
        columns = ((MAX_ROWS as f64 / (0.6 * aspect)).floor() as usize).max(1);
    }
    (columns, rows_for(columns).min(MAX_ROWS))
}

/// The best cell for six sub-pixel colours (0 to 255 per channel), in canonical form.
pub fn cell_for(samples: &[[f64; 3]; 6]) -> MosaicCell {
    let mut distances = [[0.0f64; 8]; 6];
    for (sample, row) in samples.iter().zip(distances.iter_mut()) {
        for (color, distance) in Color::ALL.iter().zip(row.iter_mut()) {
            let rgb = color.rgb();
            *distance = (0..3)
                .map(|k| (sample[k] - f64::from(rgb[k])).powi(2))
                .sum();
        }
    }
    // a == b is the uniform cell; listed first, it wins any tie with a pair that adds nothing.
    let mut best = (0usize, 0usize, f64::INFINITY);
    for a in 0..8 {
        for b in a..8 {
            let error: f64 = distances.iter().map(|d| d[a].min(d[b])).sum();
            if error < best.2 {
                best = (a, b, error);
            }
        }
    }
    let (a, b, _) = best;
    let mask_b = (0..6)
        .filter(|&i| distances[i][b] < distances[i][a])
        .fold(0u8, |mask, i| mask | 1 << i);
    let (ca, cb) = (Color::ALL[a], Color::ALL[b]);
    match mask_b.count_ones() {
        0 => MosaicCell {
            fg: ca,
            bg: ca,
            mask: 0,
        },
        6 => MosaicCell {
            fg: cb,
            bg: cb,
            mask: 0,
        },
        4 | 5 => MosaicCell {
            fg: ca,
            bg: cb,
            mask: !mask_b & 0x3F,
        },
        // b is the minority, or a 3–3 tie: a, listed first, is the background.
        _ => MosaicCell {
            fg: cb,
            bg: ca,
            mask: mask_b,
        },
    }
}

/// `image` over the black paper (transparency reads as black), at `columns` wide.
pub fn mosaic_from_rgba(image: &RgbaImage, columns: usize) -> MosaicImage {
    let (columns, rows) = grid(image.width(), image.height(), columns);
    let (width, height) = (columns * CELL_WIDTH, rows * CELL_HEIGHT);
    if image.width() == 0 || image.height() == 0 {
        let black = MosaicCell {
            fg: Color::Black,
            bg: Color::Black,
            mask: 0,
        };
        return MosaicImage {
            columns,
            rows,
            cells: vec![black; columns * rows],
        };
    }
    let paper = over_black(image);
    let (w, h) = (width as u32, height as u32);
    let reduced;
    let source = if paper.width() > 2 * w || paper.height() > 2 * h {
        reduced = imageops::thumbnail(&paper, paper.width().min(2 * w), paper.height().min(2 * h));
        &reduced
    } else {
        &paper
    };
    let pixels = imageops::resize(source, w, h, FilterType::Triangle);
    let mut cells = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let (x0, y0) = (column * CELL_WIDTH, row * CELL_HEIGHT);
            let mut samples = [[0.0f64; 3]; 6];
            for (bit, sample) in samples.iter_mut().enumerate() {
                let xs = x0 + COLUMN_EDGES[bit % 2]..x0 + COLUMN_EDGES[bit % 2 + 1];
                let ys = y0 + ROW_EDGES[bit / 2]..y0 + ROW_EDGES[bit / 2 + 1];
                let count = (xs.len() * ys.len()) as f64;
                for y in ys {
                    for x in xs.clone() {
                        let pixel = pixels.get_pixel(x as u32, y as u32).0;
                        for k in 0..3 {
                            sample[k] += f64::from(pixel[k]);
                        }
                    }
                }
                sample.iter_mut().for_each(|v| *v /= count);
            }
            cells.push(cell_for(&samples));
        }
    }
    MosaicImage {
        columns,
        rows,
        cells,
    }
}

/// An RGB8 PNG with one pixel per font pixel times `scale`: a cell is `6·scale × 10·scale`
/// pixels. Empty for an inconsistent mosaic.
pub fn png(mosaic: &MosaicImage, scale: usize) -> Vec<u8> {
    let scale = scale.max(1);
    let consistent = mosaic.columns > 0
        && mosaic.rows > 0
        && mosaic.columns.checked_mul(mosaic.rows) == Some(mosaic.cells.len());
    if !consistent {
        return Vec::new();
    }
    let (width, height) = (
        mosaic.columns * CELL_WIDTH * scale,
        mosaic.rows * CELL_HEIGHT * scale,
    );
    let mut bytes = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        let font_y = y / scale;
        let row = font_y / CELL_HEIGHT;
        let sub_row = match font_y % CELL_HEIGHT {
            0..=2 => 0,
            3..=6 => 1,
            _ => 2,
        };
        for x in 0..width {
            let font_x = x / scale;
            let cell = mosaic.cells[row * mosaic.columns + font_x / CELL_WIDTH];
            let sub_column = usize::from(font_x % CELL_WIDTH >= COLUMN_EDGES[1]);
            let bit = 2 * sub_row + sub_column;
            let color = if cell.mask >> bit & 1 == 1 {
                cell.fg
            } else {
                cell.bg
            };
            bytes.extend_from_slice(&color.rgb());
        }
    }
    let mut out = Vec::new();
    match PngEncoder::new(&mut out).write_image(
        &bytes,
        width as u32,
        height as u32,
        ExtendedColorType::Rgb8,
    ) {
        Ok(()) => out,
        Err(_) => Vec::new(),
    }
}

/// What a failure answers: one black cell, hidden by the stylesheet once the box is marked.
pub fn failure_png() -> Vec<u8> {
    let black = MosaicCell {
        fg: Color::Black,
        bg: Color::Black,
        mask: 0,
    };
    png(
        &MosaicImage {
            columns: 1,
            rows: 1,
            cells: vec![black],
        },
        1,
    )
}

/// A `ressac-mosaic:` address: `ressac-mosaic:image?cols=40&src=<original, percent-encoded>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicRequest {
    pub original: String,
    pub columns: usize,
}

impl MosaicRequest {
    /// Every byte outside `A–Z a–z 0–9 - . _ ~` is percent-encoded (upper-case hex), so the
    /// original survives intact.
    pub fn url_for(original: &str, columns: usize) -> String {
        let mut out = format!("{SCHEME}:image?cols={columns}&src=");
        for byte in original.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                out.push(char::from(byte));
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }

    /// None unless `url` is a well-formed mosaic address for an http(s) original.
    pub fn parse(url: &str) -> Option<MosaicRequest> {
        let (scheme, rest) = url.split_once(':')?;
        if !scheme.eq_ignore_ascii_case(SCHEME) {
            return None;
        }
        let (route, query) = rest.split_once('?')?;
        if route != "image" {
            return None;
        }
        let query = query.split('#').next().unwrap_or("");
        let value = |wanted: &str| {
            query
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .find(|(name, _)| *name == wanted)
                .map(|(_, value)| value)
        };
        let columns: usize = value("cols")?.parse().ok()?;
        if !(1..=MAX_COLUMNS).contains(&columns) {
            return None;
        }
        let original = percent_decode(value("src")?)?;
        is_web_url(&original).then_some(MosaicRequest { original, columns })
    }

    pub fn cache_key(&self) -> String {
        format!("{} {}", self.columns, self.original)
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            // Strict: `from_str_radix` alone would accept a leading '+'.
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Why an image has no mosaic. Every case shows the magenta IMAGE block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicFailure {
    /// Not a mosaic address, or an original that is not a public http(s) address.
    Refused,
    Status(u16),
    /// Over the byte cap, or too many pixels to decode safely.
    TooLarge,
    /// Not an image the decoder reads: HTML, an empty body, SVG…
    Unreadable,
    /// Timeout, DNS, connection, TLS.
    Network,
}

impl MosaicFailure {
    /// A failure that may not repeat (a network error, a server error, throttling): never remembered.
    pub fn is_transient(self) -> bool {
        match self {
            MosaicFailure::Network => true,
            MosaicFailure::Status(code) => code == 429 || (500..=599).contains(&code),
            MosaicFailure::Refused | MosaicFailure::TooLarge | MosaicFailure::Unreadable => false,
        }
    }

    /// The class of a failed download, from the texts `Ressac::fetch_image` refuses with.
    pub fn of(error: &Error) -> MosaicFailure {
        let Error::ImageRefused(text) = error else {
            return MosaicFailure::Network;
        };
        match text.as_str() {
            "not a web address" | "local or private address" => MosaicFailure::Refused,
            "larger than 10 MB" | "response too large" => MosaicFailure::TooLarge,
            "not an image" => MosaicFailure::Unreadable,
            other => other
                .strip_prefix("HTTP ")
                .and_then(|code| code.parse().ok())
                .map_or(MosaicFailure::Network, MosaicFailure::Status),
        }
    }
}

/// The only formats a feed's image may use. The decoder is picked from the bytes' magic
/// number, so without this list a hostile feed reaches every decoder `image` carries (TIFF,
/// OpenEXR, HDR, DDS, QOI, PNM…), the float ones with a weaker allocation record.
fn is_allowed_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::Gif
            | ImageFormat::WebP
            | ImageFormat::Bmp
            | ImageFormat::Ico
    )
}

/// Decodes a downloaded original into a small image for `mosaic_from_rgba`, and the columns
/// its width gives (`max_columns` at most). Only PNG, JPEG, GIF, WebP, BMP and ICO are read
/// (anything else is `Unreadable`). Before any pixel is decoded, the header gives the size and
/// the colour type: more than `MAX_SOURCE_PIXELS`, or more memory than `MAX_DECODE_ALLOC` for
/// the decoded buffer (its real bytes per pixel, 16-bit colour included) plus the RGBA8 copy
/// that the reduction needs (none when it already is RGBA8), is `TooLarge`. The decoder itself
/// is also limited to `MAX_DECODE_ALLOC`. A decoder that panics on hostile bytes, headers
/// included, is caught and reads as `Unreadable` (no cheap hostile fixture triggers one, so
/// this is not tested). A GIF gives its first frame. Peak memory is about one decoded buffer,
/// its RGBA8 form when it differs, and the thumbnail.
pub fn decode(bytes: &[u8], max_columns: usize) -> Result<(RgbaImage, usize), MosaicFailure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decode_guarded(bytes, max_columns)
    }))
    .unwrap_or(Err(MosaicFailure::Unreadable))
}

fn decode_guarded(bytes: &[u8], max_columns: usize) -> Result<(RgbaImage, usize), MosaicFailure> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .filter(|reader| reader.format().is_some_and(is_allowed_format))
        .ok_or(MosaicFailure::Unreadable)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| MosaicFailure::Unreadable)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(MosaicFailure::Unreadable);
    }
    let pixels = u64::from(width) * u64::from(height);
    let color = decoder.color_type();
    let copy = if color == ColorType::Rgba8 { 0 } else { 4 };
    if pixels > MAX_SOURCE_PIXELS
        || pixels * (u64::from(color.bytes_per_pixel()) + copy) > MAX_DECODE_ALLOC
    {
        return Err(MosaicFailure::TooLarge);
    }
    let picture = DynamicImage::from_decoder(decoder).map_err(|_| MosaicFailure::Unreadable)?;
    let columns = columns_for_width(width, max_columns);
    let (cols, rows) = grid(width, height, columns);
    // About two image pixels per font pixel, like the Mac's thumbnail.
    let long_side = (2 * (cols * CELL_WIDTH).max(rows * CELL_HEIGHT)) as u32;
    // One full-size buffer: the decoded picture as RGBA8 (no copy when it already is),
    // composited over black in place, so no transparent pixel's colour bleeds into the
    // reduction. Only then is it reduced; the thumbnail is the second, small buffer.
    let mut paper = picture.into_rgba8();
    black_paper(&mut paper);
    if paper.width().max(paper.height()) > long_side {
        let thumbnail = DynamicImage::ImageRgba8(paper).thumbnail(long_side, long_side);
        return Ok((thumbnail.into_rgba8(), columns));
    }
    Ok((paper, columns))
}

/// Composites `image` over black in place: every pixel becomes opaque.
fn black_paper(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = u32::from(pixel.0[3]);
        for channel in &mut pixel.0[..3] {
            *channel = (u32::from(*channel) * alpha / 255) as u8;
        }
        pixel.0[3] = 255;
    }
}

/// `image` over the black paper: transparency reads as black.
fn over_black(image: &RgbaImage) -> RgbImage {
    let mut paper = RgbImage::new(image.width(), image.height());
    for (source, target) in image.pixels().zip(paper.pixels_mut()) {
        let alpha = u32::from(source.0[3]);
        for (out, channel) in target.0.iter_mut().zip(source.0) {
            *out = (u32::from(channel) * alpha / 255) as u8;
        }
    }
    paper
}

/// Decode, mosaic, PNG.
pub fn render(bytes: &[u8], max_columns: usize) -> Result<Vec<u8>, MosaicFailure> {
    let (thumbnail, columns) = decode(bytes, max_columns)?;
    Ok(png(&mosaic_from_rgba(&thumbnail, columns), 1))
}

/// Downloads an original through the app's guarded image fetch (public http(s) only, 10 MB,
/// timeout, `image/*`), keeping the failure's class.
pub async fn fetch(app: &Ressac, url: &str) -> Result<Vec<u8>, MosaicFailure> {
    if !is_web_url(url) {
        return Err(MosaicFailure::Refused);
    }
    app.fetch_image(url)
        .await
        .map_err(|error| MosaicFailure::of(&error))
}

/// Finished mosaics by `MosaicRequest::cache_key`, least recently used out first once past
/// `CACHE_BYTES` (keys included). Transient failures are never remembered.
#[derive(Debug, Default)]
pub struct MosaicCache {
    entries: HashMap<String, CacheEntry>,
    clock: u64,
    cost: usize,
}

#[derive(Debug)]
struct CacheEntry {
    result: Result<Vec<u8>, MosaicFailure>,
    used: u64,
}

/// An entry's memory: its key (a full address, as long as the article allows) and its PNG,
/// or `FAILURE_COST` for a failure.
fn cost_of(key: &str, result: &Result<Vec<u8>, MosaicFailure>) -> usize {
    key.len() + result.as_ref().map_or(FAILURE_COST, Vec::len)
}

impl MosaicCache {
    pub fn get(&mut self, key: &str) -> Option<Result<Vec<u8>, MosaicFailure>> {
        self.clock += 1;
        let entry = self.entries.get_mut(key)?;
        entry.used = self.clock;
        Some(entry.result.clone())
    }

    pub fn put(&mut self, key: &str, result: Result<Vec<u8>, MosaicFailure>) {
        if matches!(&result, Err(failure) if failure.is_transient()) {
            return;
        }
        self.clock += 1;
        self.cost += cost_of(key, &result);
        let entry = CacheEntry {
            result,
            used: self.clock,
        };
        if let Some(old) = self.entries.insert(key.to_owned(), entry) {
            self.cost -= cost_of(key, &old.result);
        }
        while self.cost > CACHE_BYTES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(gone) = self.entries.remove(&oldest) {
                self.cost -= cost_of(&oldest, &gone.result);
            }
        }
    }

    pub fn cost(&self) -> usize {
        self.cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::TestDb;
    use image::{ImageFormat, Rgba};
    use ressac_core::service::Ressac;
    use std::io::Cursor;
    use std::time::{Duration, Instant};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn flat(r: u8, g: u8, b: u8, width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]))
    }

    fn encoded(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn uniform(color: Color) -> MosaicCell {
        MosaicCell {
            fg: color,
            bg: color,
            mask: 0,
        }
    }

    fn decoded_png(bytes: &[u8]) -> image::RgbImage {
        image::load_from_memory_with_format(bytes, ImageFormat::Png)
            .unwrap()
            .to_rgb8()
    }

    #[test]
    fn a_flat_colour_gives_uniform_cells_in_canonical_form() {
        let mosaic = mosaic_from_rgba(&flat(255, 0, 0, 60, 100), 10);
        assert_eq!((mosaic.columns, mosaic.rows), (10, 10));
        assert_eq!(mosaic.cells, vec![uniform(Color::Red); 100]);
    }

    #[test]
    fn a_half_black_half_white_cell_takes_black_as_background() {
        let left_white = RgbaImage::from_fn(6, 10, |x, _| {
            if x < 3 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let mosaic = mosaic_from_rgba(&left_white, 1);
        assert_eq!((mosaic.columns, mosaic.rows), (1, 1));
        assert_eq!(
            mosaic.cells,
            vec![MosaicCell {
                fg: Color::White,
                bg: Color::Black,
                mask: 0b01_0101
            }]
        );
        let top_white = RgbaImage::from_fn(6, 10, |_, y| {
            if y < 3 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        assert_eq!(
            mosaic_from_rgba(&top_white, 1).cells,
            vec![MosaicCell {
                fg: Color::White,
                bg: Color::Black,
                mask: 0b00_0011
            }]
        );
        let mostly_white = RgbaImage::from_fn(6, 10, |_, y| {
            if y < 7 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        assert_eq!(
            mosaic_from_rgba(&mostly_white, 1).cells,
            vec![MosaicCell {
                fg: Color::Black,
                bg: Color::White,
                mask: 0b11_0000
            }]
        );
    }

    #[test]
    fn colours_quantise_to_the_nearest_teletext_colour() {
        let all = |image: RgbaImage, color: Color| {
            mosaic_from_rgba(&image, 4)
                .cells
                .iter()
                .all(|cell| *cell == uniform(color))
        };
        assert!(all(flat(30, 220, 235, 60, 100), Color::Cyan));
        assert!(all(flat(200, 30, 40, 60, 100), Color::Red));
        assert!(all(flat(20, 20, 20, 60, 100), Color::Black));
        assert_eq!(cell_for(&[[250.0, 240.0, 20.0]; 6]), uniform(Color::Yellow));
    }

    #[test]
    fn transparency_reads_as_the_black_paper() {
        let clear = RgbaImage::from_pixel(12, 20, Rgba([255, 255, 255, 0]));
        assert!(
            mosaic_from_rgba(&clear, 2)
                .cells
                .iter()
                .all(|cell| *cell == uniform(Color::Black))
        );
    }

    #[test]
    fn width_and_height_stay_within_the_limits() {
        let wide = mosaic_from_rgba(&flat(0, 0, 255, 1000, 500), 40);
        assert_eq!((wide.columns, wide.rows), (40, 12));
        assert_eq!(columns_for_width(100, 40), 9);
        assert_eq!(columns_for_width(5000, 40), 40);
        assert_eq!(columns_for_width(1, 40), 1);
        assert_eq!(columns_for_width(480, 20), 20);
        let (columns, rows) = grid(100, 10_000, 40);
        assert!(rows <= MAX_ROWS);
        assert_eq!(columns, 1);
    }

    #[test]
    fn the_png_has_one_pixel_per_font_pixel_with_sextant_rows_of_three_four_three() {
        let mosaic = MosaicImage {
            columns: 2,
            rows: 1,
            cells: vec![
                MosaicCell {
                    fg: Color::Green,
                    bg: Color::Blue,
                    mask: 0b00_0100,
                },
                uniform(Color::Magenta),
            ],
        };
        let image = decoded_png(&png(&mosaic, 1));
        assert_eq!(image.dimensions(), (12, 10));
        let at = |x: u32, y: u32| image.get_pixel(x, y).0;
        assert_eq!(at(1, 2), [0, 0, 255]);
        assert_eq!(at(1, 3), [0, 255, 0]);
        assert_eq!(at(2, 6), [0, 255, 0]);
        assert_eq!(at(1, 7), [0, 0, 255]);
        assert_eq!(at(3, 4), [0, 0, 255]);
        assert_eq!(at(6, 0), [255, 0, 255]);
        assert_eq!(decoded_png(&png(&mosaic, 2)).dimensions(), (24, 20));
        let inconsistent = MosaicImage {
            columns: 2,
            rows: 1,
            cells: vec![],
        };
        assert!(png(&inconsistent, 1).is_empty());
    }

    #[test]
    fn the_original_survives_the_round_trip() {
        let original = "https://img.example.org/a b/photo.jpg?w=800&h=600#x";
        let address = MosaicRequest::url_for(original, 40);
        assert!(
            address.starts_with("ressac-mosaic:image?cols=40&src=https%3A%2F%2Fimg.example.org"),
            "{address}"
        );
        let request = MosaicRequest::parse(&address).unwrap();
        assert_eq!(request.original, original);
        assert_eq!(request.columns, 40);
        assert_eq!(request.cache_key(), format!("40 {original}"));
        assert!(MosaicRequest::parse(&address.replace("ressac-mosaic", "Ressac-Mosaic")).is_some());
    }

    #[test]
    fn other_schemes_and_bad_columns_are_refused() {
        for source in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "ftp://example.org/a.png",
            "https:///nohost",
        ] {
            let address = MosaicRequest::url_for(source, 40);
            assert_eq!(MosaicRequest::parse(&address), None, "{source}");
        }
        let good = "https%3A%2F%2Fexample.org%2Fa.png";
        for bad in [
            format!("ressac-mosaic:image?cols=0&src={good}"),
            format!("ressac-mosaic:image?cols=41&src={good}"),
            format!("ressac-mosaic:image?src={good}"),
            format!("ressac-mosaic:other?cols=4&src={good}"),
            "https://example.org/a.png".to_string(),
        ] {
            assert_eq!(MosaicRequest::parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_gif_uses_its_first_frame() {
        let mut out = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut out);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                let frame = image::Frame::new(RgbaImage::from_pixel(24, 40, Rgba(color)));
                encoder.encode_frame(frame).unwrap();
            }
        }
        let (thumbnail, columns) = decode(&out, 40).unwrap();
        assert_eq!(columns, 2);
        let mosaic = mosaic_from_rgba(&thumbnail, columns);
        assert!(mosaic.cells.iter().all(|cell| *cell == uniform(Color::Red)));
    }

    #[test]
    fn html_empty_and_svg_bodies_are_unreadable() {
        for body in [
            &b"<html><body>Not an image</body></html>"[..],
            &b""[..],
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#,
        ] {
            assert_eq!(decode(body, 40).unwrap_err(), MosaicFailure::Unreadable);
        }
    }

    #[test]
    fn only_web_image_formats_are_decoded() {
        let tiff = b"II*\0\x08\0\0\0\0\0\0\0".to_vec();
        let qoi = b"qoif\0\0\0\x01\0\0\0\x01\x03\0\0\0\0\0\0\0\0\0\0\x01".to_vec();
        let pnm = b"P6\n1 1\n255\n\x01\x02\x03".to_vec();
        let hdr = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 1\n".to_vec();
        for (name, body) in [("tiff", tiff), ("qoi", qoi), ("pnm", pnm), ("hdr", hdr)] {
            let failure = decode(&body, 40).unwrap_err();
            assert_eq!(failure, MosaicFailure::Unreadable, "{name}");
            assert!(!failure.is_transient());
        }
        // The allowed ones still read.
        for format in [ImageFormat::Png, ImageFormat::Bmp, ImageFormat::Gif] {
            let bytes = encoded(&flat(1, 2, 3, 12, 12), format);
            assert!(decode(&bytes, 40).is_ok(), "{format:?}");
        }
    }

    #[test]
    fn a_translucent_picture_reads_as_black_before_it_is_reduced() {
        let clear = RgbaImage::from_pixel(240, 240, Rgba([255, 255, 255, 0]));
        let (thumbnail, columns) = decode(&encoded(&clear, ImageFormat::Png), 40).unwrap();
        assert!(thumbnail.pixels().all(|p| p.0 == [0, 0, 0, 255]));
        assert!(
            mosaic_from_rgba(&thumbnail, columns)
                .cells
                .iter()
                .all(|c| *c == uniform(Color::Black))
        );
    }

    #[test]
    fn a_transparent_pixels_colour_does_not_bleed_into_the_reduction() {
        // Opaque blue on the left, fully transparent (but red underneath) on the right, big
        // enough to be reduced: the transparent half is the black paper, never red or purple.
        let edge = RgbaImage::from_fn(2400, 1200, |x, _| {
            if x < 1200 {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([255, 0, 0, 0])
            }
        });
        let (thumbnail, columns) = decode(&encoded(&edge, ImageFormat::Png), 40).unwrap();
        assert!(thumbnail.width() < 2400, "the picture was reduced");
        assert!(thumbnail.pixels().all(|p| p.0[0] == 0 && p.0[3] == 255));
        let mosaic = mosaic_from_rgba(&thumbnail, columns);
        assert!(mosaic.cells.iter().all(|cell| {
            [Color::Blue, Color::Black].contains(&cell.fg)
                && [Color::Blue, Color::Black].contains(&cell.bg)
        }));
    }

    #[test]
    fn percent_decoding_is_strict() {
        let good = "https%3A%2F%2Fexample.org%2Fa.png";
        assert!(MosaicRequest::parse(&format!("ressac-mosaic:image?cols=4&src={good}")).is_some());
        assert_eq!(
            MosaicRequest::parse("ressac-mosaic:image?cols=4&src=https%3A%2F%2Fexample.org%2F%+f"),
            None
        );
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in bytes {
            crc ^= u32::from(*byte);
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

    #[test]
    fn too_many_pixels_are_refused_before_decoding() {
        // A 1 x 1 PNG whose header claims 8000 x 8000, with its CRC fixed: 64 million pixels
        // that a decoder would allocate in full.
        let mut png = encoded(&flat(1, 2, 3, 1, 1), ImageFormat::Png);
        png[16..20].copy_from_slice(&8000u32.to_be_bytes());
        png[20..24].copy_from_slice(&8000u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        let start = Instant::now();
        assert_eq!(decode(&png, 40).unwrap_err(), MosaicFailure::TooLarge);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// A 1 x 1 PNG of `colour`'s type whose header claims `width` x `height`, CRC fixed.
    fn header_claiming(image: image::DynamicImage, width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        let mut png = out.into_inner();
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&height.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        png
    }

    #[test]
    fn the_cap_counts_the_real_bytes_per_pixel() {
        let rgba16 = || {
            image::DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
                1,
                1,
                Rgba([1u16, 2, 3, 4]),
            ))
        };
        // 4000 x 4000 is within the pixel cap, but 8 + 4 bytes per pixel is 192 MB.
        let big = header_claiming(rgba16(), 4000, 4000);
        assert_eq!(decode(&big, 40).unwrap_err(), MosaicFailure::TooLarge);
        // 2000 x 2000 is 48 MB: it passes the check (the one real pixel then fails to read).
        let small = header_claiming(rgba16(), 2000, 2000);
        assert_ne!(decode(&small, 40).unwrap_err(), MosaicFailure::TooLarge);
        // An RGBA8 picture needs no copy: 4000 x 4000 x 4 = 64 MB passes.
        let rgba8 = header_claiming(
            image::DynamicImage::ImageRgba8(flat(1, 2, 3, 1, 1)),
            4000,
            4000,
        );
        assert_ne!(decode(&rgba8, 40).unwrap_err(), MosaicFailure::TooLarge);
    }

    #[test]
    fn an_image_becomes_a_mosaic_png() {
        let source = encoded(&flat(255, 0, 0, 120, 120), ImageFormat::Png);
        let rendered = render(&source, 40).unwrap();
        let image = decoded_png(&rendered);
        assert_eq!(image.width(), 60);
        assert_eq!(image.height(), 60);
        assert_eq!(image.get_pixel(5, 5).0, [255, 0, 0]);
    }

    async fn app(db: &TestDb) -> Ressac {
        Ressac::open(&db.path)
            .unwrap()
            .allow_private_articles()
            .unwrap()
    }

    #[tokio::test]
    async fn fetch_failures_keep_their_class() {
        let server = MockServer::start().await;
        let serve = |route: &str, response: ResponseTemplate| {
            Mock::given(method("GET"))
                .and(path(route.to_string()))
                .respond_with(response)
        };
        serve("/gone.png", ResponseTemplate::new(404))
            .mount(&server)
            .await;
        serve("/busy.png", ResponseTemplate::new(503))
            .mount(&server)
            .await;
        serve(
            "/page.html",
            ResponseTemplate::new(200).set_body_raw("<html></html>", "text/html"),
        )
        .mount(&server)
        .await;
        serve(
            "/huge.png",
            ResponseTemplate::new(200).set_body_raw(vec![0u8; 11 << 20], "image/png"),
        )
        .mount(&server)
        .await;
        serve(
            "/ok.png",
            ResponseTemplate::new(200).set_body_raw(
                encoded(&flat(0, 255, 0, 12, 20), ImageFormat::Png),
                "image/png",
            ),
        )
        .mount(&server)
        .await;
        let db = TestDb::new();
        let app = app(&db).await;
        let url = |name: &str| format!("{}/{name}", server.uri());

        let gone = fetch(&app, &url("gone.png")).await.unwrap_err();
        assert_eq!(gone, MosaicFailure::Status(404));
        assert!(!gone.is_transient());
        let busy = fetch(&app, &url("busy.png")).await.unwrap_err();
        assert_eq!(busy, MosaicFailure::Status(503));
        assert!(busy.is_transient());
        assert_eq!(
            fetch(&app, &url("page.html")).await.unwrap_err(),
            MosaicFailure::Unreadable
        );
        assert_eq!(
            fetch(&app, &url("huge.png")).await.unwrap_err(),
            MosaicFailure::TooLarge
        );
        assert!(fetch(&app, &url("ok.png")).await.is_ok());
        assert_eq!(
            fetch(&app, "ftp://example.org/a.png").await.unwrap_err(),
            MosaicFailure::Refused
        );
    }

    #[test]
    fn failures_map_from_the_core_error_texts() {
        let of = |text: &str| MosaicFailure::of(&ressac_core::Error::ImageRefused(text.into()));
        assert_eq!(of("not a web address"), MosaicFailure::Refused);
        assert_eq!(of("local or private address"), MosaicFailure::Refused);
        assert_eq!(of("larger than 10 MB"), MosaicFailure::TooLarge);
        assert_eq!(of("response too large"), MosaicFailure::TooLarge);
        assert_eq!(of("not an image"), MosaicFailure::Unreadable);
        assert_eq!(of("HTTP 429"), MosaicFailure::Status(429));
        assert_eq!(of("timed out"), MosaicFailure::Network);
        assert_eq!(of("Dns"), MosaicFailure::Network);
        assert!(MosaicFailure::Status(429).is_transient());
        assert!(MosaicFailure::Status(500).is_transient());
        assert!(MosaicFailure::Network.is_transient());
        assert!(!MosaicFailure::Status(403).is_transient());
        assert!(!MosaicFailure::TooLarge.is_transient());
    }

    #[test]
    fn transient_failures_are_never_cached() {
        let mut cache = MosaicCache::default();
        cache.put("a", Err(MosaicFailure::Network));
        assert_eq!(cache.get("a"), None);
        cache.put("b", Err(MosaicFailure::Unreadable));
        assert_eq!(cache.get("b"), Some(Err(MosaicFailure::Unreadable)));
        assert_eq!(cache.cost(), FAILURE_COST + "b".len());

        let mut cache = MosaicCache::default();
        for key in ["one", "two", "three"] {
            cache.put(key, Ok(vec![0; 6 << 20]));
        }
        assert_eq!(cache.get("one"), None, "the oldest goes first");
        assert!(cache.get("two").is_some());
        assert!(cache.get("three").is_some());
        assert!(cache.cost() <= CACHE_BYTES);
        // "two" was read before "three": a new entry evicts "two".
        cache.put("four", Ok(vec![0; 6 << 20]));
        assert_eq!(cache.get("two"), None);
        assert!(cache.get("three").is_some());
    }

    /// A hostile feed's unique, long, failing image addresses: the keys are the memory, so
    /// they count toward the cap (each failure alone costs `FAILURE_COST`).
    #[test]
    fn long_keys_count_toward_the_cap() {
        let mut cache = MosaicCache::default();
        let key = |i: usize| format!("ressac-mosaic:image?cols=40&src={i:0>4096}");
        for i in 0..5_000 {
            cache.put(&key(i), Err(MosaicFailure::Status(404)));
        }
        let held: usize = cache.entries.keys().map(String::len).sum();
        assert!(held <= CACHE_BYTES, "{held} bytes of keys held");
        assert!(cache.cost() >= held, "{} < {held}", cache.cost());
        assert!(cache.cost() <= CACHE_BYTES);
        assert!(cache.get(&key(4_999)).is_some(), "the newest stays");
        assert_eq!(cache.get(&key(0)), None, "the oldest went first");
    }

    #[test]
    fn the_failure_placeholder_is_one_black_cell() {
        let image = decoded_png(&failure_png());
        assert_eq!(image.dimensions(), (6, 10));
        assert!(image.pixels().all(|pixel| pixel.0 == [0, 0, 0]));
    }
}
