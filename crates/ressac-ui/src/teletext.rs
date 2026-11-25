//! Teletext, as Antiope draws it (the Mac's `TeletextColor`, `TeletextFont`, `CellText` and
//! `CellLine`): the eight colours, Bedstead's metrics and character map read from the bundled
//! font, text fitted to an exact number of cells, cell lines and the wire format `CellRow`
//! paints, teletext dates, and the geometry of the Mac's `CRTOverlay` (not drawn here). No Qt.

use std::fmt::Display;
use std::sync::OnceLock;

use chrono::{DateTime, TimeZone, Utc};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::settings::Appearance;

pub const FAMILY: &str = "Bedstead";
pub const FONT_FILE: &str = "bedstead.otf";
pub const BEDSTEAD: &[u8] =
    include_bytes!("../../../apps/macos/Ressac/Resources/Fonts/bedstead.otf");
pub const LICENCE: &str =
    include_str!("../../../apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt");

/// The eight teletext colours. No other colour appears in an Antiope view, and blue never
/// colours text: it is for separators, backgrounds and mosaics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Color {
    pub const ALL: [Color; 8] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::White,
    ];

    /// Pure primaries, sRGB.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Color::Black => [0x00, 0x00, 0x00],
            Color::Red => [0xFF, 0x00, 0x00],
            Color::Green => [0x00, 0xFF, 0x00],
            Color::Yellow => [0xFF, 0xFF, 0x00],
            Color::Blue => [0x00, 0x00, 0xFF],
            Color::Magenta => [0xFF, 0x00, 0xFF],
            Color::Cyan => [0x00, 0xFF, 0xFF],
            Color::White => [0xFF, 0xFF, 0xFF],
        }
    }

    /// `#RRGGBB`, upper case.
    pub fn css(self) -> &'static str {
        match self {
            Color::Black => "#000000",
            Color::Red => "#FF0000",
            Color::Green => "#00FF00",
            Color::Yellow => "#FFFF00",
            Color::Blue => "#0000FF",
            Color::Magenta => "#FF00FF",
            Color::Cyan => "#00FFFF",
            Color::White => "#FFFFFF",
        }
    }

    /// 0 to 7, in [`Color::ALL`] order.
    pub fn index(self) -> u8 {
        match self {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::White => 7,
        }
    }

    pub fn from_index(index: u8) -> Option<Color> {
        Color::ALL.get(usize::from(index)).copied()
    }
}

/// What Bedstead's `head` and `hhea` tables say, in font units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metrics {
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
}

struct Font {
    metrics: Metrics,
    coverage: Vec<(u32, u32)>,
}

/// Big-endian reads that fail instead of panicking on a bad offset.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        Some(u16::from_be_bytes(self.0.get(at..at + 2)?.try_into().ok()?))
    }

    fn i16(&self, at: usize) -> Option<i16> {
        Some(i16::from_be_bytes(self.0.get(at..at + 2)?.try_into().ok()?))
    }

    fn u32(&self, at: usize) -> Option<u32> {
        Some(u32::from_be_bytes(self.0.get(at..at + 4)?.try_into().ok()?))
    }

    /// The offset of the table named `tag`, from the sfnt's table records.
    fn table(&self, tag: &[u8; 4]) -> Option<usize> {
        let count = usize::from(self.u16(4)?);
        (0..count).find_map(|i| {
            let record = 12 + 16 * i;
            (self.0.get(record..record + 4)? == tag)
                .then(|| self.u32(record + 8).map(|o| o as usize))?
        })
    }
}

fn parse_font(data: &[u8]) -> Option<Font> {
    let r = Reader(data);
    let head = r.table(b"head")?;
    let hhea = r.table(b"hhea")?;
    let cmap = r.table(b"cmap")?;
    let metrics = Metrics {
        units_per_em: r.u16(head + 18)?,
        ascender: r.i16(hhea + 4)?,
        descender: r.i16(hhea + 6)?,
        line_gap: r.i16(hhea + 8)?,
        advance_width_max: r.u16(hhea + 10)?,
    };
    let tables = usize::from(r.u16(cmap + 2)?);
    let mut found: [Option<usize>; 2] = [None, None]; // (3, 10), then (0, 4)
    for j in 0..tables {
        let record = cmap + 4 + 8 * j;
        let (platform, encoding) = (r.u16(record)?, r.u16(record + 2)?);
        let offset = r.u32(record + 4)? as usize;
        match (platform, encoding) {
            (3, 10) => found[0] = Some(offset),
            (0, 4) => found[1] = Some(offset),
            _ => {}
        }
    }
    let subtable = cmap + found[0].or(found[1])?;
    if r.u16(subtable)? != 12 {
        return None;
    }
    let groups = r.u32(subtable + 12)? as usize;
    let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(groups.min(data.len() / 12));
    for k in 0..groups {
        let group = subtable + 16 + 12 * k;
        let (start, end, glyph) = (r.u32(group)?, r.u32(group + 4)?, r.u32(group + 8)?);
        // A code point mapped to glyph 0 (`.notdef`, here U+F1FF) is a missing glyph, not a
        // supported one: a group starting at glyph 0 loses its first code point.
        let start = if glyph == 0 {
            start.saturating_add(1)
        } else {
            start
        };
        if start <= end {
            ranges.push((start, end));
        }
    }
    ranges.sort_unstable();
    let mut coverage: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match coverage.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => coverage.push((start, end)),
        }
    }
    Some(Font { metrics, coverage })
}

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        parse_font(BEDSTEAD).unwrap_or_else(|| {
            tracing::error!("Bedstead's tables are unreadable: no character is supported");
            Font {
                metrics: Metrics::default(),
                coverage: Vec::new(),
            }
        })
    })
}

/// From `head` and `hhea`; zero when the font is unreadable.
pub fn metrics() -> Metrics {
    font().metrics
}

/// `(size * 6 / 10, size)`: every glyph takes six tenths of an em.
pub fn cell_size(size: f64) -> (f64, f64) {
    (size * 6.0 / 10.0, size)
}

/// One pixel of Bedstead's 6 x 10 design grid.
pub fn font_pixel(size: f64) -> f64 {
    size / 10.0
}

/// The code points of the `(3, 10)` character map, as sorted, merged, closed ranges.
pub fn coverage() -> &'static [(u32, u32)] {
    &font().coverage
}

fn covers(scalar: char) -> bool {
    let code = u32::from(scalar);
    let ranges = coverage();
    let after = ranges.partition_point(|&(_, end)| end < code);
    ranges.get(after).is_some_and(|&(start, _)| start <= code)
}

/// The composed (NFC) form of the grapheme, when it is one scalar.
fn single_scalar(grapheme: &str) -> Option<char> {
    let mut composed = grapheme.nfc();
    let first = composed.next()?;
    composed.next().is_none().then_some(first)
}

/// Whether Bedstead draws the grapheme: its NFC form is one scalar of the character map.
pub fn supports(grapheme: &str) -> bool {
    single_scalar(grapheme).is_some_and(covers)
}

/// General category Cf, Unicode 16.
const FORMAT: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

fn is_invisible(c: char) -> bool {
    let code = u32::from(c);
    c.is_control() || FORMAT.iter().any(|&(a, b)| (a..=b).contains(&code))
}

/// A stretch of cells: text Bedstead draws, or `?` standing for graphemes it lacks, which the
/// views draw in magenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRun {
    pub text: String,
    pub replacement: bool,
}

/// [`fit_with`] with Bedstead's character map.
pub fn fit(text: &str, cells: i64) -> Vec<CellRun> {
    fit_with(text, cells, supports)
}

/// Fits `text` into exactly `cells` cells, one grapheme per cell: cut between graphemes
/// without an ellipsis, padded with spaces; any whitespace is a space, control and format
/// characters take no cell, a grapheme `supports` rejects (or that does not compose to one
/// scalar) is a `?` replacement. Adjacent runs of one kind merge; `cells <= 0` gives no runs.
pub fn fit_with(text: &str, cells: i64, supports: impl Fn(&str) -> bool) -> Vec<CellRun> {
    let width = usize::try_from(cells).unwrap_or(0);
    let mut runs: Vec<CellRun> = Vec::new();
    let mut used = 0;
    let mut add = |piece: char, replacement: bool| match runs.last_mut() {
        Some(last) if last.replacement == replacement => last.text.push(piece),
        _ => runs.push(CellRun {
            text: piece.to_string(),
            replacement,
        }),
    };
    for grapheme in text.graphemes(true) {
        if used >= width {
            break;
        }
        if grapheme.chars().all(char::is_whitespace) {
            add(' ', false);
        } else if grapheme.chars().all(is_invisible) {
            continue;
        } else {
            match single_scalar(grapheme) {
                Some(c) if supports(&c.to_string()) => add(c, false),
                _ => add('?', true),
            }
        }
        used += 1;
    }
    for _ in used..width {
        add(' ', false);
    }
    runs
}

/// One coloured stretch of a cell row, its text already fitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellSpan {
    pub runs: Vec<CellRun>,
    pub fg: Color,
    pub bg: Color,
}

impl CellSpan {
    /// Fits `text` into `cells` cells (a negative count is 0), on black.
    pub fn new(text: &str, cells: i64, fg: Color) -> CellSpan {
        CellSpan {
            runs: fit(text, cells.max(0)),
            fg,
            bg: Color::Black,
        }
    }

    pub fn on(mut self, bg: Color) -> CellSpan {
        self.bg = bg;
        self
    }

    /// One cell per character of the runs.
    pub fn cells(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// A row of the teletext grid: spans side by side.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellLine {
    pub spans: Vec<CellSpan>,
}

/// Between the runs of an encoded line.
pub const RUN_SEPARATOR: char = '\u{1f}';
/// Between the lines of an encoded row.
pub const LINE_SEPARATOR: char = '\u{1e}';

impl CellLine {
    pub fn new(spans: Vec<CellSpan>) -> CellLine {
        CellLine { spans }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(CellSpan::cells).sum()
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(CellSpan::text).collect()
    }

    /// `width` black cells.
    pub fn blank(width: usize) -> CellLine {
        CellLine::new(vec![CellSpan::new("", width as i64, Color::White)])
    }

    /// `text` centred in `width` cells (one cell more on the right when the margin is odd).
    pub fn centred(text: &str, width: usize, fg: Color) -> CellLine {
        let margin = width.saturating_sub(text.graphemes(true).count()) / 2;
        let padded = format!("{}{text}", " ".repeat(margin));
        CellLine::new(vec![CellSpan::new(&padded, width as i64, fg)])
    }

    /// Reverse video: every span in `fg` on `bg`.
    pub fn reversed(&self, fg: Color, bg: Color) -> CellLine {
        CellLine::new(
            self.spans
                .iter()
                .map(|s| CellSpan {
                    runs: s.runs.clone(),
                    fg,
                    bg,
                })
                .collect(),
        )
    }

    /// The runs joined by [`RUN_SEPARATOR`]; a run is its foreground (`'0'` to `'7'`, `'?'`
    /// for a replacement), its background, then its text. Fitted text never holds U+001E or
    /// U+001F (control characters are dropped), so the separators are safe.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            for run in &span.runs {
                if !out.is_empty() {
                    out.push(RUN_SEPARATOR);
                }
                out.push(if run.replacement {
                    '?'
                } else {
                    digit(span.fg.index())
                });
                out.push(digit(span.bg.index()));
                out.push_str(&run.text);
            }
        }
        out
    }
}

fn digit(index: u8) -> char {
    char::from(b'0' + index)
}

/// A run as `CellRow` paints it: no foreground is a replacement, always magenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintRun {
    pub fg: Option<Color>,
    pub bg: Color,
    pub text: String,
}

/// The inverse of [`CellLine::encode`]; a malformed run is skipped.
pub fn decode(encoded: &str) -> Vec<PaintRun> {
    if encoded.is_empty() {
        return Vec::new();
    }
    encoded
        .split(RUN_SEPARATOR)
        .filter_map(|run| {
            let mut chars = run.chars();
            let fg = chars.next()?;
            let bg = Color::from_index(u8::try_from(chars.next()?.to_digit(8)?).ok()?)?;
            let fg = match fg {
                '?' => None,
                d => Some(Color::from_index(u8::try_from(d.to_digit(8)?).ok()?)?),
            };
            Some(PaintRun {
                fg,
                bg,
                text: chars.as_str().to_string(),
            })
        })
        .collect()
}

/// Lines joined by [`LINE_SEPARATOR`].
pub fn encode_lines(lines: &[CellLine]) -> String {
    lines
        .iter()
        .map(CellLine::encode)
        .collect::<Vec<_>>()
        .join(&LINE_SEPARATOR.to_string())
}

pub fn decode_lines(encoded: &str) -> Vec<Vec<PaintRun>> {
    if encoded.is_empty() {
        return Vec::new();
    }
    encoded.split(LINE_SEPARATOR).map(decode).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePattern {
    /// `WED 14 OCT`
    Day,
    /// `09:47:12`
    Time,
    /// `10 OCT 26 07:00`
    LogDate,
}

/// English capitals whatever the system language, in the given zone.
pub fn date<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz, pattern: DatePattern) -> String
where
    Tz::Offset: Display,
{
    let local = at.with_timezone(tz);
    match pattern {
        DatePattern::Day => local.format("%a %d %b").to_string().to_uppercase(),
        DatePattern::Time => local.format("%H:%M:%S").to_string(),
        DatePattern::LogDate => local.format("%d %b %y %H:%M").to_string().to_uppercase(),
    }
}

/// The CRT screen (scanlines, vignette, corners, bleed): its numbers and geometry. The KDE app
/// does not draw it (B9 was cut on its frame budget); kept for a later attempt, see TODO.md,
/// « Linux (sous-projet 8) », the CRT screen line. `pages::frame` reads its margins.
pub mod crt {
    use super::Appearance;

    pub const SCANLINE_OPACITY: f64 = 0.28;
    pub const SCANLINE_FRACTION: f64 = 0.4;
    pub const VIGNETTE_OPACITY: f64 = 0.35;
    pub const CORNER_PERCENT: f64 = 3.0;
    pub const BLEED_OFFSET: f64 = 1.0;
    pub const BLEED_OPACITY: f64 = 0.45;
    pub const GLOW_RADIUS: f64 = 2.0;

    /// The screen shows only in Antiope with its box checked.
    pub fn is_active(look: Appearance, crt: bool) -> bool {
        look == Appearance::Antiope && crt
    }

    /// One dark band per font-pixel row, `(top, bottom)`, the band a whole number of device
    /// pixels thick at `scale`, the last one cut at the window's edge.
    pub fn scanline_bands(height: f64, pixel: f64, scale: f64) -> Vec<(f64, f64)> {
        if height <= 0.0 || pixel <= 0.0 || scale <= 0.0 {
            return Vec::new();
        }
        let band = (pixel * SCANLINE_FRACTION * scale).round().max(1.0) / scale;
        let mut bands = Vec::new();
        for row in 0.. {
            let bottom = (row + 1) as f64 * pixel;
            let top = bottom - band;
            if top >= height {
                break;
            }
            bands.push((top, bottom.min(height)));
        }
        bands
    }

    pub fn corner_radius(width: f64, height: f64) -> f64 {
        width.min(height) * CORNER_PERCENT / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Utc};

    use crate::settings::Appearance;

    fn runs(f: &[CellRun]) -> Vec<(&str, bool)> {
        f.iter().map(|r| (r.text.as_str(), r.replacement)).collect()
    }

    #[test]
    fn bedstead_ships_with_its_dedication_and_its_metrics() {
        assert!(
            LICENCE.contains("CC0") || LICENCE.contains("Creative Commons"),
            "{LICENCE}"
        );
        assert_eq!(
            metrics(),
            Metrics {
                units_per_em: 1000,
                ascender: 800,
                descender: -200,
                line_gap: 0,
                advance_width_max: 600
            }
        );
        assert_eq!(
            [15.0, 20.0, 25.0, 30.0].map(cell_size),
            [(9.0, 15.0), (12.0, 20.0), (15.0, 25.0), (18.0, 30.0)]
        );
        assert_eq!(
            [15.0, 20.0, 25.0, 30.0].map(font_pixel),
            [1.5, 2.0, 2.5, 3.0]
        );
    }

    #[test]
    fn the_character_map_is_bedsteads() {
        let total: u32 = coverage().iter().map(|(a, b)| b - a + 1).sum();
        assert_eq!(total, 2603); // fontTools: 2603 code points in the (3,10) table
        assert_eq!(coverage().first().map(|r| r.0), Some(0x20));
        for g in [
            "é",
            "è",
            "ê",
            "ë",
            "à",
            "â",
            "î",
            "ï",
            "ô",
            "ù",
            "û",
            "ü",
            "ÿ",
            "ç",
            "œ",
            "æ",
            "É",
            "Œ",
            "«",
            "»",
            "’",
            "“",
            "”",
            "…",
            "–",
            "—",
            "€",
            "°",
            "▐",
            "█",
            "░",
            "●",
            "*",
            "?",
            "\u{1FB00}",
            "\u{1FB3B}",
            "e\u{301}",
        ] {
            assert!(supports(g), "{g:?}");
        }
        for g in ["🎉", "東", "❤️", "🇫🇷", "q\u{301}", "\u{301}"] {
            assert!(!supports(g), "{g:?}");
        }
    }

    #[test]
    fn a_damaged_font_reads_as_nothing_instead_of_panicking() {
        assert!(parse_font(&[]).is_none());
        assert!(parse_font(&BEDSTEAD[..1000]).is_none());
    }

    #[test]
    fn the_eight_colours_are_pure() {
        assert_eq!(
            Color::ALL.map(Color::css),
            [
                "#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF",
                "#FFFFFF"
            ]
        );
        for c in Color::ALL {
            assert!(c.rgb().iter().all(|v| *v == 0 || *v == 0xFF), "{c:?}");
            assert_eq!(Color::from_index(c.index()), Some(c));
        }
        assert_eq!(Color::from_index(8), None);
    }

    /// Review Focus 1: emoji, CJK and a combining accent each take exactly one cell.
    #[test]
    fn emoji_cjk_and_combining_accents_take_one_cell_each() {
        let f = fit("Café 🎉 東京", 12);
        assert_eq!(
            runs(&f),
            [
                ("Café ", false),
                ("?", true),
                (" ", false),
                ("??", true),
                ("   ", false)
            ]
        );
        assert_eq!(f.iter().map(|r| r.text.chars().count()).sum::<usize>(), 12); // one scalar per cell
        let d = fit("Cafe\u{301} noir", 6);
        assert_eq!(runs(&d), [("Café n", false)]);
        assert_eq!(
            d[0].text.chars().map(u32::from).collect::<Vec<_>>(),
            [0x43, 0x61, 0x66, 0xE9, 0x20, 0x6E]
        );
    }

    #[test]
    fn long_text_is_cut_without_ellipsis_and_short_text_is_padded() {
        assert_eq!(
            runs(&fit("Courrier international", 8)),
            [("Courrier", false)]
        );
        assert_eq!(runs(&fit("Unread", 9)), [("Unread   ", false)]);
        assert_eq!(runs(&fit("", 3)), [("   ", false)]);
        assert!(fit("xkcd", 0).is_empty() && fit("xkcd", -2).is_empty());
    }

    #[test]
    fn clusters_whitespace_and_invisibles_never_move_the_grid() {
        assert_eq!(runs(&fit("👩‍👩‍👧🇫🇷x", 4)), [("??", true), ("x ", false)]);
        assert_eq!(
            runs(&fit("Line\nbreak\ttab\r\nend", 20)),
            [("Line break tab end  ", false)]
        );
        assert_eq!(
            runs(&fit("a\u{200B}b\u{FEFF}c\u{00AD}d", 5)),
            [("abcd ", false)]
        );
    }

    #[test]
    fn the_coverage_test_is_injectable() {
        assert_eq!(
            runs(&fit_with("abcb", 4, |g| g != "b")),
            [("a", false), ("?", true), ("c", false), ("?", true)]
        );
    }

    #[test]
    fn spans_add_up_to_the_line_width() {
        let line = CellLine::new(vec![
            CellSpan::new("101 ", 4, Color::Cyan),
            CellSpan::new("Unread", 13, Color::White),
            CellSpan::new(" 1083", 5, Color::Yellow),
        ]);
        assert_eq!(
            (
                line.width(),
                line.spans.iter().map(CellSpan::cells).collect::<Vec<_>>()
            ),
            (22, vec![4, 13, 5])
        );
        assert_eq!(
            CellSpan::new("far too long for the span", 3, Color::White).cells(),
            3
        );
        assert_eq!(CellSpan::new("negative", -2, Color::White).cells(), 0);
    }

    #[test]
    fn reverse_video_keeps_the_text_and_replacements_stay_magenta_on_the_wire() {
        let line = CellLine::new(vec![
            CellSpan::new("A🎉", 2, Color::Yellow),
            CellSpan::new("x", 1, Color::White).on(Color::Blue),
        ]);
        assert_eq!(line.encode(), "30A\u{1f}?0?\u{1f}74x");
        let rev = line.reversed(Color::Black, Color::Cyan);
        assert!(
            rev.spans
                .iter()
                .all(|s| s.fg == Color::Black && s.bg == Color::Cyan)
        );
        assert_eq!(rev.text(), line.text());
        assert_eq!(
            decode(&rev.encode()),
            [
                PaintRun {
                    fg: Some(Color::Black),
                    bg: Color::Cyan,
                    text: "A".into()
                },
                PaintRun {
                    fg: None,
                    bg: Color::Cyan,
                    text: "?".into()
                },
                PaintRun {
                    fg: Some(Color::Black),
                    bg: Color::Cyan,
                    text: "x".into()
                }
            ]
        );
        assert_eq!(
            decode_lines(&encode_lines(&[line.clone(), CellLine::blank(2)])).len(),
            2
        );
    }

    #[test]
    fn blank_and_centred_lines() {
        assert_eq!(
            (CellLine::blank(5).width(), CellLine::blank(5).text()),
            (5, "     ".into())
        );
        assert!(
            CellLine::blank(5)
                .spans
                .iter()
                .all(|s| s.bg == Color::Black)
        );
        assert_eq!(CellLine::blank(0).width(), 0);
        assert_eq!(
            CellLine::centred("RESSAC", 20, Color::Yellow).text(),
            "       RESSAC       "
        );
        assert_eq!(
            CellLine::centred("A message far longer than ten", 10, Color::Cyan).width(),
            10
        );
    }

    #[test]
    fn teletext_dates_are_english_capitals_in_any_zone() {
        let at = Utc.timestamp_opt(1_791_964_032, 0).unwrap(); // 2026-10-14 07:47:12 UTC
        assert_eq!(
            date(at, &crate::test_db::Paris, DatePattern::Day),
            "WED 14 OCT"
        );
        assert_eq!(
            date(at, &crate::test_db::Paris, DatePattern::Time),
            "09:47:12"
        );
        assert_eq!(
            date(
                Utc.timestamp_opt(1_791_615_600, 0).unwrap(),
                &Utc,
                DatePattern::LogDate
            ),
            "10 OCT 26 07:00"
        );
        let kolkata = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        assert_eq!(date(at, &kolkata, DatePattern::Time), "13:17:12");
    }

    #[test]
    fn the_screen_is_only_for_antiope_with_the_box_checked() {
        assert!(crt::is_active(Appearance::Antiope, true));
        assert!(
            ![
                (Appearance::Antiope, false),
                (Appearance::Native, true),
                (Appearance::Native, false)
            ]
            .iter()
            .any(|(l, c)| crt::is_active(*l, *c))
        );
    }

    #[test]
    fn bands_land_on_font_pixel_rows_at_every_text_size() {
        for (size, band) in [(15.0, 0.5), (20.0, 1.0), (25.0, 1.0), (30.0, 1.0)] {
            let pixel = font_pixel(size);
            let bands = crt::scanline_bands(600.0, pixel, 2.0);
            assert_eq!(bands.len(), (600.0 / pixel) as usize, "{size}");
            for (row, (top, bottom)) in bands.iter().enumerate() {
                assert_eq!(*bottom, (row + 1) as f64 * pixel);
                assert_eq!(bottom - top, band);
                assert_eq!((top * 2.0).round(), top * 2.0, "crisp at 2x");
                assert!(bottom - top <= pixel / 2.0);
            }
        }
    }

    #[test]
    fn the_last_band_stops_at_the_window_edge() {
        let b = crt::scanline_bands(101.0, 2.0, 2.0);
        assert_eq!((b.len(), *b.last().unwrap()), (50, (99.0, 100.0)));
        let t = crt::scanline_bands(101.5, 2.0, 2.0);
        assert_eq!((t.len(), *t.last().unwrap()), (51, (101.0, 101.5)));
        assert!(
            crt::scanline_bands(0.0, 2.0, 2.0).is_empty()
                && crt::scanline_bands(100.0, 0.0, 2.0).is_empty()
        );
        assert_eq!(crt::scanline_bands(10.0, 2.0, 1.0)[0], (1.0, 2.0));
    }

    #[test]
    fn tube_corners_and_the_values() {
        assert_eq!(
            [
                crt::corner_radius(900.0, 500.0),
                crt::corner_radius(1440.0, 900.0),
                crt::corner_radius(0.0, 0.0)
            ],
            [15.0, 27.0, 0.0]
        );
        assert_eq!(
            [
                crt::SCANLINE_OPACITY,
                crt::SCANLINE_FRACTION,
                crt::VIGNETTE_OPACITY,
                crt::CORNER_PERCENT,
                crt::BLEED_OFFSET,
                crt::BLEED_OPACITY,
                crt::GLOW_RADIUS
            ],
            [0.28, 0.4, 0.35, 3.0, 1.0, 0.45, 2.0]
        );
    }
}
