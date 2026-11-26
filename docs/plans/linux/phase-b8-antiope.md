# Phase B8: Antiope (B8a–B8e)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE window has the Antiope look of the Mac app: a pixel-exact Bedstead grid in the eight teletext colours (header with clock and rolling counter, index 22 cells, entries 28, reader ≥ 24, Minitel bar), page numbers typed on the keyboard, P100 SOMMAIRE, P899 GUIDE, P104 JOURNAL and its end page, the garden pages P800–P802 with their inspector page (Theme picker included), the search prompt, the banner row, SAA5050 mosaics instead of images with `i` to swap, then the second VM checkpoint on marghe, which stops for Clément.

**Runs after:** B7c (Sea Chart). Master plan `docs/plans/2026-10-23-linux.md` (rows B8a–B8e; Interfaces; Global Constraints: Antiope, Language, Shortcuts, Settings keys, Time, Never, Gates). Spec §7.9, §7.10 (the header's tube-corner margin), §8.2, §8.3, §8.5 (checkpoint 2), §9, §10. Inventory §5 (all of it except §5.9: no Orca parity, spec §2), §1.4, §7.3 (Appearance row), §10.1 (AntiopeFrameTests, AntiopePagesTests, AntiopeJournalTests, AntiopeGardenTests, PageDirectoryTests, TeletextTests, MosaicTests, the Antiope parts of KeyboardTests): strings and numbers binding. Mac sources: `apps/macos/Ressac/Antiope/*.swift`, `Teletext/*.swift`, `Reading/MosaicSchemeHandler.swift`, `Reading/ReadingKeyRouter.swift:1-200`, `Reading/ReaderView.swift:148-290`, `Reading/ArticleTemplate.swift:410-470`, `Resources/reader.css:281-533` (reused in place, never edited), and their tests in `apps/macos/RessacTests/`.

**Architecture:** every rule is Rust in `ressac-ui`, tested on the Mac: `teletext` (palette, Bedstead's metrics and character map read from the bundled OTF, cell fitting, cell lines and their wire encoding, teletext dates, the CRT geometry), `pages` (reserved pages, the page directory, page entry, the `AntiopeNav` state machine that returns effects, and the pure line builders of every page: `pages::frame`, `pages::special`, `pages::garden`), `keys` (the Antiope map), `mosaic` (the SAA5050 renderer, requests, decoding, cache, fetch failures), `sanitize` (mosaic boxes) and `article` (sub-page labels and app-world scripts). `ressac-kde` draws every teletext row with one cxx-qt `CellRow` (`#[base = QQuickPaintedItem]`) painting Rust-computed runs through cxx-qt-lib's `QPainter` (the measured decision below), adds an `Antiope` singleton (navigation, directory, header, index, P100/P899/P104 rows, Minitel bar, sub-page), a generic `CellRowsModel` list model (rows diffed by id), a `Mosaics` singleton (fetch threads, cache, replies) behind a C++ `QWebEngineUrlSchemeHandler` for `ressac-mosaic:`, and QML: `AntiopeView.qml`, `MinitelBar.qml`, `CellList.qml`, plus `GridProbe.qml` for the measurement.

**Tech Stack:** Rust 2024 / 1.99; `unicode-segmentation` 1.13.3 (already in `Cargo.lock`) and `unicode-normalization` 0.1.26 (new; `tinyvec` it needs is already locked), `image` 0.25.10 (workspace, new to `ressac-ui`), `wiremock` 0.6.5 (dev); cxx 1.0.205 / cxx-qt / cxx-qt-lib 0.10.0 (`QPainter`, `QFont`, `QPen`, `QColor`, `QRectF`, `QPoint`); Qt 6.11.2 Quick, WebEngineQuick, WebEngineCore (`QWebEngineUrlSchemeHandler`, `QWebEngineUrlRequestJob::setAdditionalResponseHeaders`); Kirigami 6.31; Fedora 44 container; marghe's Fedora 44 VM (`ssh willow@marghe.local`, `prlctl exec "Fedora Linux"`).

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Sixteen sub-phases, not five** (5-file cap; QML, C++ and `notes.md` count; tests, `test_db.rs`, `Cargo.lock` and this file do not). Master rows map as: B8a → **B8a1** (teletext) + **B8a2** (pages, keys); B8b → **B8b1–B8b2** (the grid decision, first) + **B8b3–B8b6** (header, Minitel bar, index and entries columns); B8c → **B8c1–B8c4**; B8d → **B8d1–B8d3**; B8e unchanged. Ledger rows B8a1…B8e. Every master name is kept (`ressac_ui::teletext`, `ressac_ui::pages`, `ressac_ui::mosaic`, scheme `ressac-mosaic:`).
2. **Two Unicode crates** (`notes.md` line, B8a1): grapheme clusters (`unicode-segmentation`, already locked through ratatui) and NFC composition (`unicode-normalization`). No font crate: Bedstead's `cmap` (format 12) and metrics (`head`, `hhea`) are read from the bundled file by ~60 lines of Rust, tested against the numbers fontTools gives.
3. **No VoiceOver texts** (spec §2: no Orca parity): `CellLine` has no accessibility text, and the Mac assertions on labels ("Unread, page 101, 1083", "Sommaire, go to page 100"…) are not ported. Everything drawn is.
4. **Teletext dates through chrono** (`%a %d %b`, `%H:%M:%S`, `%d %b %y %H:%M`, English by construction, then upper case): no formatter cache, so `TeletextDateFormatTests` becomes one output test over four zones.
5. **P899 lists KDE keys** (spec §3 call 6, §7.2): `ALT S`, `ALT G  ?`, `ALT R`, `BACKSPACE`, `CTRL RETURN`, `CTRL F`, `CTRL R  F5`, `CTRL SHIFT R`, `CTRL SHIFT A`, `CTRL + -`, `CTRL O`, `CTRL SHIFT E`; the key column is 13 cells (the Mac's rule `keys + 1 ≤ key column` with the longest ` CTRL SHIFT R`).
6. **The Antiope state is a pure machine returning effects** (`pages::AntiopeNav::handle → Outcome { handled, effects }`): selecting a source, retrying the edition, refreshing, toggling images, focusing the entries, marking read, paging down before the end page. QML applies them across the singletons (`SourcesModel`, `EntriesModel`, `Journal`, `Runs`, `Garden`), so no singleton calls another from Rust.
7. **The page directory is built by the `Antiope` bridge from `Store::sources`** (one extra indexed read per reload, only while Antiope shows) and rebuilt only when the snapshot differs (`pages::DirectoryCache`), the Mac's rule.
8. **The grid is one painted item per row** (measured below), not QML `Text` per span; double height through `QPainter::setWindow`/`setViewport` (validated exact at scales 1 and 2).
9. **Mosaics are fetched by the core's `Ressac::fetch_image`** (http(s) only, private addresses refused at every hop and after DNS, 10 MB, 15 s, `image/*` answers), on one mosaic thread with at most 4 fetches in flight, instead of a second HTTP client. The Mac's `MosaicFetcher` tests map to the core's own fetch tests plus `ressac-ui` tests of the failure classes against `wiremock` (`allow_private_articles`). In recette mode only (`--screenshot`), `RESSAC_ALLOW_PRIVATE_ARTICLES=1` (the CLI's existing hook) lets the walk serve images from a loopback server.
10. **A failed mosaic is marked by the `Mosaics.failed(url)` signal**, which the reader pane turns into an app-world script; the C++ handler has no page to script.
11. **The CRT geometry lives in `teletext::crt` from B8a1** because the header's right margin (B8b3) needs it; B9 draws the screen. Until B9 binds it, `Antiope.crt` is bound to `false`.
12. **The Theme picker on the inspector page** (spec §3 call 7) is a list of clickable rows (` NONE`, then the themes in capitals, the current one black on cyan), placed after the rhythm rows, like them; none for a buried feed.
13. **Sizes are logical pixels** (`QFont::setPixelSize` 15/20/25/30): at KWin scale 2 they draw on 30/40/50/60 device pixels, as the Mac's points do on Retina.
14. **Mac-only checks become walk checks or nothing:** the legacy-scroller test (QML lists reserve no scrollbar width), the window dressing (`AntiopeRootTests`: here the native toolbar is hidden and the page background black, checked on a screenshot), `everyGlyphTakesSixTenthsOfAnEm` (here the font's tables plus the pixel check of B8b2).

## Validated in a throwaway container (`ressac-b8-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A C++ Qt 6.11.2 program (QML scenes, `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`, `QT_QPA_UPDATE_IDLE_TIME=0`, 1280 × 800 logical, Bedstead 20 px, an entries column of 28 cells holding 2,000 entries of 2 rows, the list scrolled from `QQuickWindow::frameSwapped`, frame time = interval between two `frameSwapped`, 1,000 frames, two runs each) and a cxx-qt 0.10 crate:

| Variant | Scale | 1 entry per frame: mean / p95 / max (ms) | 10 entries per frame: mean / p95 / max (ms) |
|---|---|---|---|
| Plain rectangles (baseline) | 1 | 0.06 / 0.08 / 0.16 | – |
| | 2 | 0.18 / 0.22 / 0.59 | – |
| QML `Text` per span, `NativeRendering`, `antialiasing: false`, `PreferFullHinting` | 1 | 1.61–1.71 / 1.73–2.01 / 4.84 | 4.10 / 4.60 / 6.80 |
| | 2 | 2.04–2.83 / 2.27–6.47 / 13.10 | 4.47 / 5.03 / 8.12 |
| QML `Text` per span, `QtRendering` | 1 | 1.70–1.74 / 1.95–2.06 / 8.47 | – |
| | 2 | 1.95–2.14 / 2.12–3.01 / 6.91 | – |
| **One `QQuickPaintedItem` per entry (2 rows), runs painted by `QPainter`** | 1 | **0.09–0.10 / 0.10–0.11 / 1.67** | **0.27 / 0.28 / 0.56** |
| | 2 | **0.29 / 0.44–0.59 / 1.48** | **0.54 / 0.78 / 1.35** |
| One painted item for the whole column (visible rows repainted on scroll) | 1 | 0.39–0.45 / 0.42–0.68 / 2.10 | 0.36 / 0.38 / 0.46 |
| | 2 | 0.84–0.88 / 1.03–1.19 / 4.28 | 0.78 / 0.83 / 0.96 |
| Painted rows + CRT `Canvas` overlay (scanlines, vignette, corners) | 1 | 0.23 / 0.26–0.29 / 3.46 | 0.41 / 0.44 / 3.19 |
| | 2 | 0.61 / 0.67 / 7.77 | 0.92 / 1.00 / 7.33 |

- **Recommendation, adopted:** one painted item per row (per entry in the entries column), 10 to 15 times cheaper than `Text` per span and exact by construction; the per-column item is close but repaints every visible row on each scroll step and complicates clicks and lazy paging. The numbers are offscreen software frames on the main Mac's CPU (Apple silicon, the same family as marghe's); B8b2 measures again inside the app, B8e on the VM with virgl.
- **Pixel exactness** (pattern rows: `█ █ …` in one span, 28 one-cell spans of alternating colours, `ABC…` and `mmm…` on blue): at 15/20/25/30 px and scales 1 and 2, all three paths gave only the 8 palette colours and runs of exactly 9/12/15/18 device pixels at scale 1 and 18/24/30/36 at scale 2, every run starting on its cell. One flaw: at 15 px, scale 2, a painted block glyph overhangs its row by one device pixel; painting all backgrounds first and clipping text to its row rectangle (not per span) removes it (Task 4's rule). (`QtRendering` showed no grey only because the container's fontconfig gives the offscreen path no antialiasing; in Plasma and on GL it would. Not used.)
- **cxx-qt 0.10 can subclass `QQuickPaintedItem`:** `#[base = QQuickPaintedItem]` with `type QQuickPaintedItem;` declared (include `<QtQuick/QQuickPaintedItem>`), `#[cxx_override] unsafe fn paint(self: Pin<&mut CellRow>, painter: *mut QPainter);`, `#[inherit] fn update(self: Pin<&mut CellRow>);`, and **a declared constructor `impl cxx_qt::Constructor<()> for CellRow {}` with `BaseArguments = ()`** (the generated default constructor passes a `QObject*` to `QQuickPaintedItem(QQuickItem*)` and fails to compile). Bedstead drawn through cxx-qt-lib (`QFont::set_family`, `set_pixel_size`, `set_style_strategy(QFontStyleStrategy::NoAntialias)`, `set_hinting_preference(PreferFullHinting)`, `set_kerning(false)`; `QPainter::set_font`, `set_pen`, `fill_rect`, `draw_text(&QPoint, &QString)`) came out in three colours only; a `WRITE` setter calling `update()` repaints (checked by changing the property from a QML `Timer`).
- **Double height:** `painter.setWindow(0, 0, w, size)` then `setViewport(0, 0, w, 2 * size)` drew a block 40 px tall at scale 1 and 80 at scale 2, palette colours only.
- **CRT feasibility (B9):** with the software backend, `ShaderEffect` and `QtQuick.Effects.MultiEffect` draw **nothing** (no warning); a QML `Canvas` overlay (one band per font-pixel row, black 0.28 over the bottom 40 % rounded to whole device pixels, radial vignette 0 → 0.35, rounded-rect corners filled with `fillRule = Qt.OddEvenFill`) draws correctly and costs +0.15 ms (scale 1) and +0.4 ms (scale 2) per scrolling frame. The container cannot run the RHI OpenGL or Vulkan backends offscreen (it falls back to software even with `QSG_RHI_BACKEND`), and `qsb` (qt6-qtshadertools) is not in the package list: B9 uses `Canvas` everywhere, which also works on the VM's virgl.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `Cargo.toml` (root) | B8a1 T1 | `unicode-segmentation = "1.13.3"`, `unicode-normalization = "0.1.26"` |
| `crates/ressac-ui/Cargo.toml` | B8a1 T1; B8d1 T14 | the two Unicode crates; `image`, dev `wiremock` |
| `crates/ressac-ui/src/lib.rs` | B8a1 T1; B8a2 T2; B8d1 T14 | `pub mod teletext; pub mod pages; pub mod mosaic;` |
| `crates/ressac-ui/src/teletext.rs` (new) | B8a1 T1 | palette, metrics, character map, `fit`, `CellSpan`, `CellLine`, encoding, dates, `crt` |
| `notes.md` | B8a1 T1; B8b2 T5; B8e T18 | crates line; grid decision with numbers; VM checkpoint 2 |
| `crates/ressac-ui/src/pages/mod.rs` (new) | B8a2 T2; B8b3 T6; B8c1 T9, T10 | reserved pages, directory, page entry, `AntiopeNav`, `Line`; `mod frame; mod special; mod garden;` |
| `crates/ressac-ui/src/keys.rs` | B8a2 T3 | Antiope commands, verbs, focus tokens |
| `apps/linux/src/cellrow.rs` (new) | B8b1 T4 | `CellRow` painted item |
| `apps/linux/src/lib.rs` | B8b1 T4; B8b4 T7; B8d2 T16 | `mod cellrow; mod antiope; mod cellrows; mod mosaic;`, shim declarations, `extern "Rust"` |
| `apps/linux/cpp/shim.h`, `shim.cpp` | B8b1 T4; B8d2 T16 | Bedstead registration, frame log; `ressac-mosaic` scheme and handler |
| `apps/linux/src/recette.rs` | B8b1 T4 | `frameLogStart`, `frameLogStop`, `gridLine`, `gridSize` |
| `apps/linux/qml/GridProbe.qml` (new) | B8b2 T5 | the measurement scene |
| `apps/linux/qml/Main.qml` | B8b2 T5; B8b6 T8b; B8c4 T13; B8d3 T17 | `grid` walk; Antiope chrome and walks |
| `crates/ressac-ui/src/pages/frame.rs` (new) | B8b3 T6 | header, column titles, index and entry rows, Minitel bar, prompt label, banner, empty rows |
| `crates/ressac-ui/src/article.rs` | B8b3 T6; B8d1 T15 | `SubpageLabel::of`, `of_edition`, `subpage_script`; mosaics in Antiope pages, image scripts |
| `apps/linux/src/antiope.rs` (new) | B8b4 T7; B8c2 T11 | `Antiope` singleton |
| `apps/linux/src/cellrows.rs` (new) | B8b4 T7 | `CellRowsModel` |
| `apps/linux/src/entries.rs` | B8b4 T7 | role `antiopeLines`, properties `selectedFeedId`, `total` |
| `apps/linux/src/runs.rs` | B8b4 T7 | properties `progressDone`, `progressTotal` |
| `apps/linux/src/reader.rs` | B8b5 T8a; B8d2 T16 | `subpagePage/Index/Count`; image script invokables |
| `apps/linux/src/settings.rs` | B8b5 T8a | `bedsteadAvailable`, `effectiveAppearance` |
| `apps/linux/qml/AntiopeView.qml` (new) | B8b5 T8a; B8c3 T12; B8d3 T17 | the Antiope page |
| `apps/linux/qml/MinitelBar.qml` (new) | B8b5 T8a; B8c3 T12 | keys, hints, search prompt |
| `apps/linux/qml/CellList.qml` (new) | B8b5 T8a; B8c3 T12 | a list of `CellRowsModel` rows with click actions |
| `apps/linux/qml/ReadingPage.qml` | B8b6 T8b; B8c3 T12 | look switch, `perform` delegation, `activeReader` |
| `apps/linux/qml/ReaderPane.qml` | B8b6 T8b; B8d3 T17 | `active`, sub-page rewrite; toggles, failed marks |
| `crates/ressac-ui/src/pages/special.rs` (new) | B8c1 T9 | P100, P899, end page, P104 |
| `crates/ressac-ui/src/pages/garden.rs` (new) | B8c1 T10 | P800–P802, inspector page, rhythm and theme rows |
| `apps/linux/src/journal.rs` | B8c2 T11 | `progressDone`, `progressTotal`, `failure` |
| `apps/linux/src/garden.rs`, `garden_inspector.rs` | B8c2 T11 | `antiopeRows`, `antiopeMove`, `antiopeCells`, `antiopeInspector` |
| `apps/linux/qml/SettingsWindow.qml` | B8c3 T12 | Appearance row, font rule, Antiope size slider |
| `crates/ressac-ui/src/mosaic.rs` (new) | B8d1 T14 | renderer, PNG, request, decoder, cache, failures, fetch |
| `crates/ressac-ui/src/sanitize/mod.rs` | B8d1 T15 | `Images::Mosaics` boxes |
| `apps/linux/src/mosaic.rs` (new) | B8d2 T16 | `Mosaics` singleton, `ressac_mosaic_start` |
| `apps/linux/dev/marghe.sh`, `TODO.md` | B8e T18 | `--look`, `--walk`, `--big`, `--frame-log`; deferred anomalies |
| tests (not counted) | | `crates/ressac-ui/src/test_db.rs` (`garden_row`, `journal_view`), `apps/linux/tests/grid.rs`, `antiope.rs`, `antiope_pages.rs`, `mosaic.rs` (new) |

Files per sub-phase: B8a1 5, B8a2 3, B8b1 5, B8b2 3, B8b3 3, B8b4 5, B8b5 5, B8b6 3, B8c1 3, B8c2 4, B8c3 5, B8c4 1, B8d1 5, B8d2 5, B8d3 3, B8e 3.

## Interfaces produced (B9, B10 rely on these names)

**`ressac_ui::teletext`** (Task 1):

```rust
pub const FAMILY: &str = "Bedstead";
pub const FONT_FILE: &str = "bedstead.otf";
pub const BEDSTEAD: &[u8] = include_bytes!("../../../apps/macos/Ressac/Resources/Fonts/bedstead.otf");
pub const LICENCE: &str = include_str!("../../../apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt");
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color { Black, Red, Green, Yellow, Blue, Magenta, Cyan, White }
impl Color {
    pub const ALL: [Color; 8];
    pub fn rgb(self) -> [u8; 3];          // pure primaries, sRGB
    pub fn css(self) -> &'static str;     // "#000000" … "#FFFFFF", upper case
    pub fn index(self) -> u8;             // 0…7 in ALL order
    pub fn from_index(index: u8) -> Option<Color>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics { pub units_per_em: u16, pub ascender: i16, pub descender: i16, pub line_gap: i16, pub advance_width_max: u16 }
pub fn metrics() -> Metrics;                      // from `head` and `hhea`
pub fn cell_size(size: f64) -> (f64, f64);        // (size * 6 / 10, size)
pub fn font_pixel(size: f64) -> f64;              // size / 10
pub fn coverage() -> &'static [(u32, u32)];       // the (3,10) format-12 cmap, merged ranges
pub fn supports(grapheme: &str) -> bool;          // NFC composed is one scalar Bedstead maps
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRun { pub text: String, pub replacement: bool }
pub fn fit(text: &str, cells: i64) -> Vec<CellRun>;
pub fn fit_with(text: &str, cells: i64, supports: impl Fn(&str) -> bool) -> Vec<CellRun>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellSpan { pub runs: Vec<CellRun>, pub fg: Color, pub bg: Color }
impl CellSpan {
    pub fn new(text: &str, cells: i64, fg: Color) -> CellSpan;   // bg Black; negative cells → 0
    pub fn on(self, bg: Color) -> CellSpan;
    pub fn cells(&self) -> usize;
    pub fn text(&self) -> String;
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellLine { pub spans: Vec<CellSpan> }
impl CellLine {
    pub fn new(spans: Vec<CellSpan>) -> CellLine;
    pub fn width(&self) -> usize;
    pub fn text(&self) -> String;
    pub fn blank(width: usize) -> CellLine;                          // one white-on-black span of spaces
    pub fn centred(text: &str, width: usize, fg: Color) -> CellLine; // (width - len) / 2 spaces first
    pub fn reversed(&self, fg: Color, bg: Color) -> CellLine;
    pub fn encode(&self) -> String;
}
pub const RUN_SEPARATOR: char = '\u{1f}';
pub const LINE_SEPARATOR: char = '\u{1e}';
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintRun { pub fg: Option<Color>, pub bg: Color, pub text: String } // fg None: a replacement, always magenta
pub fn decode(encoded: &str) -> Vec<PaintRun>;
pub fn encode_lines(lines: &[CellLine]) -> String;                // joined by LINE_SEPARATOR
pub fn decode_lines(encoded: &str) -> Vec<Vec<PaintRun>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePattern { Day, Time, LogDate }                       // "WED 14 OCT", "09:47:12", "10 OCT 26 07:00"
pub fn date<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz, pattern: DatePattern) -> String where Tz::Offset: Display;
pub mod crt {
    pub const SCANLINE_OPACITY: f64 = 0.28;  pub const SCANLINE_FRACTION: f64 = 0.4;
    pub const VIGNETTE_OPACITY: f64 = 0.35;  pub const CORNER_PERCENT: f64 = 3.0;
    pub const BLEED_OFFSET: f64 = 1.0;       pub const BLEED_OPACITY: f64 = 0.45;  pub const GLOW_RADIUS: f64 = 2.0;
    pub fn is_active(look: Appearance, crt: bool) -> bool;
    pub fn scanline_bands(height: f64, pixel: f64, scale: f64) -> Vec<(f64, f64)>;  // (top, bottom)
    pub fn corner_radius(width: f64, height: f64) -> f64;
}
```

Encoding (the only format `CellRow` reads): a line is its runs joined by `RUN_SEPARATOR`; a run is two header characters then its text: the foreground (`'0'`…`'7'` = `Color::index`, or `'?'` for a replacement) and the background (`'0'`…`'7'`). Fitted text never holds U+001E or U+001F (control characters are dropped), so the separators are safe.

**`ressac_ui::pages`** (Tasks 2, 6, 9, 10):

```rust
pub const INDEX_CELLS: usize = 22;  pub const ENTRY_CELLS: usize = 28;  pub const MIN_READER_CELLS: usize = 24;
pub const MIN_CELLS: usize = 76;   // 22 + 1 + 28 + 1 + 24
pub const NOT_FOUND_FOR: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservedPage { Index, Unread, Today, Starred, Journal, Daily, Weekly, Monthly, Rare, Silent, NeedsTriage, AllFeeds, Cemetery, Guide }
impl ReservedPage {
    pub const ALL: [ReservedPage; 14];
    pub fn number(self) -> u16;  pub fn from_number(number: u16) -> Option<ReservedPage>;
    pub fn title(self) -> &'static str;               // the core's titles: "Index" … "Needs triage", "All feeds", "Cemetery", "Guide"
    pub fn cadence_group(self) -> Option<CadenceGroup>;
    pub fn destination(self) -> PageDestination;
    pub fn count(self, counts: &SourceCounts) -> usize;
    pub fn is_garden(self) -> bool;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDestination { Source(Selection), Index, Guide }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRow { pub number: u16, pub title: String, pub unread: usize, pub reserved: bool, pub depth: usize }
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageDirectory { /* rows, destinations, numbers, feed pages */ }
impl PageDirectory {
    pub fn new(snapshot: &SourcesSnapshot) -> PageDirectory;
    pub fn rows(&self) -> &[DirRow];
    pub fn destination(&self, number: u16) -> Option<PageDestination>;
    pub fn number_of(&self, selection: &Selection) -> Option<u16>;
    pub fn page_of_feed(&self, feed: FeedId) -> Option<u16>;
    pub fn is_root_folder(&self, row: &DirRow) -> bool;
}
#[derive(Debug, Default)]
pub struct DirectoryCache { /* last snapshot, its directory, builds */ }
impl DirectoryCache { pub fn get(&mut self, snapshot: &SourcesSnapshot) -> &PageDirectory; pub fn builds(&self) -> usize; }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum EntryOutcome { Pending, Complete(u16) }
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageEntry { /* digits */ }
impl PageEntry {
    pub const LENGTH: usize = 3;
    pub fn display(&self) -> String;   // "4__", "41_", "" when idle
    pub fn digits(&self) -> &str;  pub fn is_active(&self) -> bool;
    pub fn type_digit(&mut self, digit: u8) -> EntryOutcome;   // > 9 ignored
    pub fn correct(&mut self);  pub fn cancel(&mut self);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntiopeColumn { #[default] Index, Entries, Reader }      // as_str/parse "index" "entries" "reader"; next(), previous()
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderPage { #[default] Article, Index, Guide, EndOfEdition } // as_str "article" "index" "guide" "end"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect { Select(Selection), RetryEdition, Refresh, ToggleImages, FocusEntries, MarkRead(i64), PageDownThenEnd(i64), NotFound }
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome { pub handled: bool, pub effects: Vec<Effect> }
impl Outcome { pub fn encode(&self) -> String; }
// "" when not handled; else "handled" then ";select:<selection key>", ";retry", ";refresh", ";images",
// ";focusEntries", ";markRead:<id>", ";pageDownThenEnd:<id>", ";notFound" in order.
pub struct NavContext<'a> {
    pub directory: &'a PageDirectory, pub selection: Selection,
    pub selected_entry: Option<(i64, bool)>,   // (id, is read)
    pub last_edition_entry: Option<i64>,       // the edition's last entry by rank
    pub journal_retries: bool,                 // retries_journal(phase)
    pub can_refresh: bool, pub can_navigate_entries: bool, pub now: Instant,
}
pub fn retries_journal(phase: &journal::Phase) -> bool;   // Offline, Failed(_), Due
#[derive(Debug, Clone, Default)]
pub struct AntiopeNav { /* column, entry, not_found: Option<(u16, Instant)>, reader_page, advancing_to_end */ }
impl AntiopeNav {
    pub fn column(&self) -> AntiopeColumn;  pub fn set_column(&mut self, column: AntiopeColumn);
    pub fn reader_page(&self) -> ReaderPage;  pub fn show_article(&mut self);
    pub fn entry(&self) -> &PageEntry;
    pub fn not_found(&self, now: Instant) -> Option<u16>;
    pub fn current_page(&self, directory: &PageDirectory, selection: &Selection) -> Option<u16>;
    pub fn jump(&mut self, number: u16, ctx: &NavContext<'_>) -> Outcome;
    pub fn handle(&mut self, command: Command, ctx: &NavContext<'_>) -> Outcome;
    pub fn land_on_end(&mut self, selected_entry: Option<(i64, bool)>) -> Outcome;
    pub fn advance_finished(&mut self);
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineRole { Blank, Text, Header, Message, End, Title, Progress, Heading(String), Entry(i64),
    Overflow { feed: FeedId, page: Option<u16> }, Feed(FeedId), Jump(u16), Pair { left: u16, right: Option<u16>, half: usize },
    Cadence(usize), Theme(usize) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line { pub id: String, pub role: LineRole, pub cells: CellLine, pub double_height: bool, pub selected: bool }
impl Line {
    pub fn new(id: impl Into<String>, role: LineRole, cells: CellLine) -> Line;
    pub fn text(&self) -> String;
    pub fn action(&self) -> String;   // "entry:<id>", "overflow:<feed>:<page|->", "feed:<id>", "jump:<n>",
                                      // "pair:<left>:<right|->:<half>", "cadence:<i>", "theme:<i>", else ""
    pub fn encode(&self) -> String;   // id \u{1d} flags ("d" double height, "s" selected) \u{1d} action \u{1d} cells.encode()
}
pub fn encode_rows(lines: &[Line]) -> Vec<String>;
```

`pages::frame` (Task 6), `pages::special` (Task 9), `pages::garden` (Task 10): the signatures are in their tasks.

**`ressac_ui::keys`** additions (Task 3): `Command::{PageDigit(u8), PageCorrect, PageCancel, Sommaire, Guide, Repetition, ToggleImages, NextColumn, PreviousColumn, NextIndexRow, PreviousIndexRow}`; `focus::{ANTIOPE_INDEX = "antiopeIndex", ANTIOPE_ENTRIES = "antiopeEntries", ANTIOPE_READER = "antiopeReader"}`; `pub fn acts_once_per_press(command: Command) -> bool`. Verbs added: `pageDigit0` … `pageDigit9`, `pageCorrect`, `pageCancel`, `sommaire`, `guide`, `repetition`, `toggleImages`, `nextColumn`, `previousColumn`, `nextIndexRow`, `previousIndexRow`, and `spent` (a held once-per-press key: consumed, nothing done).

**`ressac_ui::mosaic`** (Task 14) and **`sanitize::Images::Mosaics`**, **`article`** additions (Task 15): signatures in the tasks.

**`ressac-kde`:**

- **`CellRow`** (QML element, `cellrow.rs`): properties `line: QString` (encoded, `LINE_SEPARATOR` between rows), `size: f64` (pixel size, default 20), `doubleHeight: bool`, `reversed: bool`, `reverseForeground: i32` (default 0, black), `reverseBackground: i32` (default 7, white); from B9 `bleed: bool`. Implicit width = longest row's cells × 0.6 × size; implicit height = rows × size (× 2 when `doubleHeight`).
- **Shim:** `rust::String ressac_bedstead_family()` (`""` when registration failed); `void ressac_frame_log_start()`, `rust::String ressac_frame_log_stop()` (`"n=<frames> mean=<ms> p50=<ms> p95=<ms> max=<ms> over16=<count>"`, two decimals), `void ressac_frame_log_auto()` (`RESSAC_FRAME_LOG=1`: one summary every 10 s as `ressac-kde: frames <summary>`); from B8d2 the `ressac-mosaic` scheme, `void ressac_mosaic_reply(std::uint64_t token, rust::Str mime, rust::Slice<const std::uint8_t> body)`; `extern "Rust"`: `fn ressac_mosaic_start(token: u64, url: &str)`.
- **`Recette`** additions: `frameLogStart()`, `frameLogStop() -> QString`, `gridLine(index: i32) -> QString`, `gridSize: f64` (`CONSTANT`, `RESSAC_RECETTE_GRID_SIZE` or 20), `gridVariant: QString` (`CONSTANT`, `RESSAC_RECETTE_GRID` or `""`).
- **`Antiope`** (QML singleton, `antiope.rs`): `WRITE` inputs (bound in QML): `selectionKey`, `searchText`, `gardenMessage`, `journalPhase`, `journalFailure: QString`; `isGarden`, `unreadOnly`, `selectedRead`, `canRefresh`, `canNavigate`, `runActive`, `reduceMotion`, `crt: bool`; `selectedEntryId`, `selectedFeedId: i64`; `entryPosition`, `entryTotal`, `runDone`, `runTotal`, `journalDone`, `journalTotal`, `windowCells`, `readerCells: i32`; `windowWidth`, `windowHeight`, `cellWidth: f64`. `READ` (`NOTIFY = state_changed`): `column: QString` (also `WRITE`), `readerPage: QString`, `currentPage: i32` (−1 none), `header`, `indexTitle`, `entriesTitle`, `readerTitle`, `minitel: QString` (encoded lines), `indexRows`, `readerRows`, `journalRows: QStringList` (encoded `Line`s), `subpagePage`, `subpageIndex`, `subpageCount: i32`. `CONSTANT`: `palette: QStringList` (CSS of the 8 colours), `keyLines: QStringList`, `keyVerbs: QStringList`, `minimumCells`, `indexCells`, `entryCells`, `keysCells: i32`. Invokables `reload()`, `tick()`, `perform(verb: QString) -> QString`, `jump(number: i32) -> QString`, `landOnEnd() -> QString`, `advanceFinished()`, `showArticle()`, `emptyLine(message: QString) -> QString`, `searchLabel(width: i32) -> QString`, `bannerRow(message: QString, showLog: bool, width: i32) -> QStringList` (message, log or `""`, dismiss). From B9: `scanlineBands(height: f64, pixel: f64, scale: f64) -> QList<f64>`.
- **`CellRowsModel`** (QML element, `cellrows.rs`, `#[base = QAbstractListModel]`): `rows: QStringList` (`WRITE`: parsed and diffed by id with `live::plan_rows`, never a reset), roles `rowId`, `line`, `doubleHeight`, `selected`, `action`; invokable `rowOfId(id: QString) -> i32`, `rowOfSelected() -> i32`.
- **`EntriesModel`** additions: role `antiopeLines` (`pages::frame::entry_lines(title, read, starred, 28, false)` encoded), properties `selectedFeedId: i64` (−1), `total: i32` (`NOTIFY = list_changed`).
- **`Runs`** additions: `progressDone`, `progressTotal: i32` (`NOTIFY = state_changed`).
- **`Reader`** additions: `subpagePage`, `subpageIndex`, `subpageCount: i32` (`WRITE`; index 0 = no label; a change that keeps the page identity emits `subpageChanged()` and never reloads); invokables `subpageScript() -> QString`, from B8d2 `toggleImageScript(index: i32)`, `toggleAllScript()`, `markFailedScript(url: QString) -> QString`.
- **`AppSettings`** additions: `bedsteadAvailable: bool` (`CONSTANT`), `effectiveAppearance: QString` (`READ`, `NOTIFY = appearance_changed`).
- **`Journal`** additions: `progressDone`, `progressTotal: i32`, `failure: QString` (`NOTIFY = state_changed`).
- **`Garden`** additions: `antiopeRows: QStringList` (`NOTIFY = state_changed`, also on selection), `antiopeCells: i32` (`WRITE`), `antiopeInspector: QStringList` (`NOTIFY = inspector_changed`), invokable `antiopeMove(delta: i32) -> bool`.
- **`Mosaics`** (QML singleton, `mosaic.rs`): signal `failed(url: QString)`; property `inFlight: i32` (`NOTIFY`).
- **QML:** `AntiopeView.qml` (`objectName: "antiopeView"`): `function perform(verb) → bool`, `function apply(effects) → bool`, `function focusColumn()`, `function openPrompt()`, `function closePrompt(clearing)`, `function scrollProbe(entriesPerFrame, frames, done)` (recette), `readonly property Item reader` (its `ReaderPane`), `readonly property alias entriesList`, `readonly property bool promptOpen`. `ReadingPage.qml`: `property string look: AppSettings.effectiveAppearance`, `readonly property bool antiope`, `readonly property Item activeReader`, `readonly property alias antiopeView`. `ReaderPane.qml`: `property bool active: true`.

**Consumed:** B1 (`settings::{Appearance, ANTIOPE_SIZES, snapped_antiope}`, `AppSettings.readerSizeAntiope`, `appearance`, `crt`), B2 (`sources::{Selection, GardenScope, SidebarTree}`, `live::plan_rows`, `rows::list_model_rows!`, `runs::RunProgress`, `entries::{EntryList, UNTITLED}`, `SourcesModel`, `EntriesModel`, `Runs`, `ReadingPage.reloadAll()`, `with_session`, `Session.worker`), B3 (`keys::{decode, classify, command, claims_override, verb, route, Key, KeyFocus, Command, focus::*}`, `article::{ArticlePage, SubpageLabel, ReaderTheme, js_string, page_down_script, decide, toggle_index, LinkDecision::ToggleImage}`, `sanitize::{sanitize, is_web_url, declared_size, ImageSize, Gate}`, `Reader`, `ReaderPane.{pageDown, runAppScript, openExternally, focusWeb}`, the key filter calling `readingPage.perform(verb)`, `Recette.key`, `MainMenu`'s `search` action, `apps/linux/dev/marghe.sh`), B4 (`journal::{Phase, section_title, header, read_count, is_finished, JournalView}`, `Journal`), B5 (`garden::{Snapshot, GardenVisit, Inspector, visible_rows, sections, progress, chip, unread_label, empty_message, remark_sentence, outcome, rhythm_text, theme_names, RHYTHM_CHOICES}`, `Garden`, `GardenPane`'s dialog texts), B6 (registered fonts in `ressac_create_application`, `SettingsWindow.qml`, `Recette.grab` of the focused window), B7 (`Main.qml`'s walk chooser, reduce-motion rule `Kirigami.Units.longDuration <= 1`); core `Store::{sources, journal}`, `SourcesSnapshot`, `SourceCounts`, `SourceNode`, `store::pages::RESERVED_PAGES`, `edition::Section`, `EditionOverflowRow`, `JournalItem`, `garden::{GardenRow, Progress}`, `model::{Status, CadenceGroup, FeedId}`, `store::{CadenceDetail, AttemptRecord}`, `Ressac::{open, fetch_image, allow_private_articles}`, `Error::ImageRefused`.

**Decisions this phase pins:**

- **One `CellRow` per row everywhere** (header, titles, index rows, entries, special pages, Minitel bar, banner); the entries column of a regular page is a `ListView` over `EntriesModel` itself (role `antiopeLines`, `reversed` bound to the selection, `rowAppeared` for paging), every other list a `CellList` over a `CellRowsModel`.
- **Paint rule** (Task 4): all backgrounds of a row first, then each run's text at `x = cells before it × 0.6 × size`, baseline `y + 0.8 × size`, clipped to the row's rectangle (never per span); replacement runs magenta whatever the span or reverse colours; reverse video replaces every run's foreground (except replacements) and background.
- **Focus = column:** `AntiopeView` keeps the active focus on the item of `Antiope.column` (`antiopeIndex` list, `antiopeEntries` list, the reader's `WebEngineView` or the `antiopeReader` list); a click on a column moves both. `keys::classify` maps the three tokens to `Sidebar`, `EntryList`, `Reader`; the prompt's `TextField` is `text` → `TextInput`.
- **Keys:** `ReadingPage.perform(verb)` offers every verb to `AntiopeView.perform` first in Antiope (it calls `Antiope.perform`, applies the effects, returns `handled`), then to its own Native table, whose reader is `activeReader`. `spent` returns true. The garden page's list takes `k r b u o w` and ↑/↓ through `Keys` once `perform` declines (B3's filter rule), and `Ctrl+Z` through an `Action` of `AntiopeView` (the native header's is hidden).
- **Time:** `Antiope.tick()` (1 s, every 0.1 s while a long run rolls the counter and motion is not reduced) and every bridge call pass `Utc::now()`, `chrono::Local` and `Instant::now()`.
- **Chrome:** in Antiope the page background is black, the native toolbar and panes are hidden (not destroyed; the native `ReaderPane` stops following `Reader.pageUrl`), KWin's title bar and the menu bar stay (spec §7.9); window minimum width `76 × 0.6 × size`.

**Step 0:** `ReadingPage.qml` and `Main.qml` (over 300 lines by B7) are restructured in Tasks 8b and 12: each starts by removing that file's dead code (unused ids, properties, functions, `console.log` debugging) in its own commit, or by recording "no dead code" in the ledger. `keys.rs` (507 lines), `sanitize/mod.rs` (892), `article.rs`, `entries.rs`, `garden.rs`, `reader.rs` and `shim.cpp` only gain items.

**Gate (end of each sub-phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

When another agent holds `ressac-fedora`, gate in a temporary container on a clean worktree snapshot (ledger ruling of B3a); never `dev.sh up` over someone else's run.

## Global Constraints

- **Antiope:** Bedstead sizes 15/20/25/30 (default 20, `reader.size.antiope`); the 8 colours only, blue never on text (separators, backgrounds, mosaics and the Fossil heading's background only); index 22 cells, entries 28, reader ≥ 24; pages as inventory §5.3.
- **Language:** UI strings in English and identical to the Mac app's; the French labels kept: `SOMMAIRE`, `SUITE`, `RETOUR`, `GUIDE`, `RÉPÉTITION`, `CORRECTION`, `ANNULATION`, `FIN DE L'EDITION`. `notes.md` in French, no em dash in French prose.
- **Shortcuts:** Alt+S / Alt+G / Alt+R in Antiope; letters `j k m s Space ? i`, digits, arrows unchanged; menu mnemonics never S, G, R.
- **Settings keys:** `appearance` (`native` default, `antiope`), `antiope.crt`, `reader.size.antiope` (20).
- **Reader:** article JavaScript never runs; app scripts only through `runJavaScript(…, WebEngineScript.ApplicationWorld, …)` built in Rust with `js_string`; `reader.css` never edited.
- **Time:** functions depending on now or the zone take them as parameters.
- **Never:** the real database; the network in tests (`wiremock` and loopback servers only); `dnf install` on the VM without Clément's go-ahead; push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **Titles that bend columns** (emoji, CJK, combining accents, a ZWJ family, flags, RTL, 300 characters, empty, only spaces, tabs and newlines) in the index, entries, P104, garden and inspector rows, at every width → every row exactly its column's cells, the separators straight. Tests: Task 1 `emoji_cjk_and_combining_accents_take_one_cell_each`, Task 6 `every_index_row_is_exactly_22_cells`, `every_entry_row_is_exactly_28_cells`, Task 9 `every_journal_row_is_exactly_the_width_asked`, Task 10 `every_garden_line_is_exactly_the_column_width`; Task 8b walk step `a10-awkward` (a 2-device-pixel-wide blue stripe at the same x on every row of the grab).
2. **Digits typed while a text field or the search prompt has focus** → text, never a page number; held digits type once. Tests: Task 3 `digits_in_a_text_field_never_become_page_numbers`, `held_antiope_keys_act_once_per_press`; Task 12 walk step `c07-prompt-digits` (search text `412`, page unchanged).
3. **An unknown page** (`437`), then a digit within 2 s, then a jump from the end page → `P437 NOT FOUND` in red for 2 s, cleared at once by the next digit, nothing else changed; the end page left. Tests: Task 2 `an_unknown_page_shows_not_found_for_two_seconds`, `a_new_digit_clears_not_found_at_once`, `a_jump_leaves_the_end_page`; Task 8b step `a03-not-found`.
4. **Hostile images** (an 8000 × 8000 PNG of 190 KB, HTML served as `image/png`, SVG, an empty body, 11 MB, a 503 then a 200, a GIF, a server that never answers, a redirect to `file:`) → the magenta `IMAGE <alt>` line or the mosaic, never a hang or a crash; transient failures not cached; a request the page cancelled never answered. Tests: Task 14 `too_many_pixels_are_refused_before_decoding`, `html_empty_and_svg_bodies_are_unreadable`, `fetch_failures_keep_their_class`, `transient_failures_are_never_cached`; Task 16 `aStoppedRequestIsNeverAnswered` → `a_cancelled_request_is_never_answered`; Task 17 walk step `d02-failed`.
5. **Switching look with the prompt open, during a run, with an entry selected** → selection, search text and run kept, prompt closed, focus never on a hidden native pane, letters work at once in Native. Tests: Task 8b steps `a09-native`, Task 12 step `c08-switch-with-prompt`.

---

## B8a1: teletext (`ressac-ui`)

### Task 1 (B8a1): `ressac_ui::teletext`

**Files:**
- Modify: `Cargo.toml` (root `[workspace.dependencies]`: `unicode-normalization = "0.1.26"`, `unicode-segmentation = "1.13.3"`, alphabetical)
- Modify: `crates/ressac-ui/Cargo.toml` (`unicode-normalization.workspace = true`, `unicode-segmentation.workspace = true`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod teletext;`)
- Create: `crates/ressac-ui/src/teletext.rs` (tests in its `mod tests`)
- Modify: `notes.md` (under « Ressac sous Linux (sous-projet 8) », a « Décisions de la partie B » bullet: « Deux crates Unicode pour Antiope (B8a1) : `unicode-segmentation` 1.13.3, déjà dans `Cargo.lock` par ratatui, pour couper le texte en graphèmes comme le Mac, et `unicode-normalization` 0.1.26 pour la forme composée (NFC) qui fait de « e + accent » un « é » de Bedstead. Pas de crate de polices : la table `cmap` et les métriques de Bedstead sont lues à la main, une soixantaine de lignes. »)

**Interfaces:**
- Consumes: `settings::Appearance`.
- Produces: `ressac_ui::teletext` as in the header.

Rules (port of `TeletextColor.swift`, `TeletextFont.swift`, `CellText.swift`, `CellLine.swift`, `CRTOverlay.swift:3-60`):
- `fit_with`: graphemes with `UnicodeSegmentation::graphemes(text, true)`; stop once `cells` are used; a grapheme whose every scalar is `char::is_whitespace` → one space; a grapheme whose every scalar is a control (`char::is_control`) or a format character (General Category Cf, the table below) → nothing; otherwise its NFC form (`unicode_normalization::UnicodeNormalization::nfc`) when that form is exactly one scalar and `supports` accepts it, else `?` as a replacement. Adjacent runs of the same kind merge; spaces pad to `cells`; `cells ≤ 0` → no runs.
- Cf table (Unicode 16, closed ranges): `0x00AD, 0x0600–0x0605, 0x061C, 0x06DD, 0x070F, 0x0890–0x0891, 0x08E2, 0x180E, 0x200B–0x200F, 0x202A–0x202E, 0x2060–0x2064, 0x2066–0x206F, 0xFEFF, 0xFFF9–0xFFFB, 0x110BD, 0x110CD, 0x13430–0x1343F, 0x1BCA0–0x1BCA3, 0x1D173–0x1D17A, 0xE0001, 0xE0020–0xE007F`.
- `supports(g)`: the NFC form is one scalar inside `coverage()` (binary search).
- `coverage()` / `metrics()`, read once (`OnceLock`) from `BEDSTEAD`, big-endian (an undetermined parser, so its algorithm):

```text
sfnt: numTables = u16 @4; table record i @ 12 + 16·i = tag[4], checksum u32, offset u32, length u32.
head @off: unitsPerEm = u16 @off+18.            hhea @off: ascender i16 @+4, descender i16 @+6, lineGap i16 @+8, advanceWidthMax u16 @+10.
cmap @c: numTables = u16 @c+2; record j @ c + 4 + 8·j = platformID u16, encodingID u16, offset u32 (from c).
   pick (3,10), else (0,4); subtable @s = c + offset: format u16 @s must be 12; numGroups u32 @s+12;
   group k @ s + 16 + 12·k = startCharCode u32, endCharCode u32, startGlyphID u32.
coverage = groups' (start, end), sorted, adjacent or overlapping ranges merged.
A missing table or a bad offset (past the end) → empty coverage and zero metrics, logged once with tracing::error!.
```

- `date`: `Day` → `at.with_timezone(tz).format("%a %d %b")` upper-cased (`"WED 14 OCT"`); `Time` → `"%H:%M:%S"`; `LogDate` → `"%d %b %y %H:%M"` upper-cased (`"10 OCT 26 07:00"`).
- `crt::scanline_bands`: for row `r = 0, 1, …`: `bottom = (r + 1) × pixel`, `band = max(1, round(pixel × 0.4 × scale)) / scale`, `top = bottom − band`; stop when `top ≥ height`; the band is `(top, min(bottom, height))`. Empty when `height ≤ 0`, `pixel ≤ 0` or `scale ≤ 0`. `corner_radius = min(w, h) × 3 / 100`. `is_active = look == Antiope && crt`.

- [ ] **Step 1: Write the failing tests** (port of `TeletextTests.swift`, `CellLineTests` of `AntiopeFrameTests.swift:13-55`, `CRTGeometryTests` of `CRTTests.swift`; `fn runs(f: &[CellRun]) -> Vec<(&str, bool)>`):

```rust
#[test]
fn bedstead_ships_with_its_dedication_and_its_metrics() {
    assert!(LICENCE.contains("CC0") || LICENCE.contains("Creative Commons"), "{LICENCE}");
    assert_eq!(metrics(), Metrics { units_per_em: 1000, ascender: 800, descender: -200, line_gap: 0, advance_width_max: 600 });
    assert_eq!([15.0, 20.0, 25.0, 30.0].map(cell_size), [(9.0, 15.0), (12.0, 20.0), (15.0, 25.0), (18.0, 30.0)]);
    assert_eq!([15.0, 20.0, 25.0, 30.0].map(font_pixel), [1.5, 2.0, 2.5, 3.0]);
}

#[test]
fn the_character_map_is_bedsteads() {
    let total: u32 = coverage().iter().map(|(a, b)| b - a + 1).sum();
    assert_eq!(total, 2603);                                   // fontTools: 2603 code points in the (3,10) table
    assert_eq!(coverage().first().map(|r| r.0), Some(0x20));
    for g in ["é", "è", "ê", "ë", "à", "â", "î", "ï", "ô", "ù", "û", "ü", "ÿ", "ç", "œ", "æ", "É", "Œ", "«", "»", "’", "“", "”",
              "…", "–", "—", "€", "°", "▐", "█", "░", "●", "*", "?", "\u{1FB00}", "\u{1FB3B}", "e\u{301}"] {
        assert!(supports(g), "{g:?}");
    }
    for g in ["🎉", "東", "❤️", "🇫🇷", "q\u{301}", "\u{301}"] { assert!(!supports(g), "{g:?}"); }
}

#[test]
fn the_eight_colours_are_pure() {
    assert_eq!(Color::ALL.map(Color::css), ["#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF"]);
    for c in Color::ALL { assert!(c.rgb().iter().all(|v| *v == 0 || *v == 0xFF), "{c:?}"); assert_eq!(Color::from_index(c.index()), Some(c)); }
    assert_eq!(Color::from_index(8), None);
}

/// Review Focus 1: emoji, CJK and a combining accent each take exactly one cell.
#[test]
fn emoji_cjk_and_combining_accents_take_one_cell_each() {
    let f = fit("Café 🎉 東京", 12);
    assert_eq!(runs(&f), [("Café ", false), ("?", true), (" ", false), ("??", true), ("   ", false)]);
    assert_eq!(f.iter().map(|r| r.text.chars().count()).sum::<usize>(), 12);  // one scalar per cell
    let d = fit("Cafe\u{301} noir", 6);
    assert_eq!(runs(&d), [("Café n", false)]);
    assert_eq!(d[0].text.chars().map(u32::from).collect::<Vec<_>>(), [0x43, 0x61, 0x66, 0xE9, 0x20, 0x6E]);
}

#[test]
fn long_text_is_cut_without_ellipsis_and_short_text_is_padded() {
    assert_eq!(runs(&fit("Courrier international", 8)), [("Courrier", false)]);
    assert_eq!(runs(&fit("Unread", 9)), [("Unread   ", false)]);
    assert_eq!(runs(&fit("", 3)), [("   ", false)]);
    assert!(fit("xkcd", 0).is_empty() && fit("xkcd", -2).is_empty());
}

#[test]
fn clusters_whitespace_and_invisibles_never_move_the_grid() {
    assert_eq!(runs(&fit("👩‍👩‍👧🇫🇷x", 4)), [("??", true), ("x ", false)]);
    assert_eq!(runs(&fit("Line\nbreak\ttab\r\nend", 20)), [("Line break tab end  ", false)]);
    assert_eq!(runs(&fit("a\u{200B}b\u{FEFF}c\u{00AD}d", 5)), [("abcd ", false)]);
}

#[test]
fn the_coverage_test_is_injectable() {
    assert_eq!(runs(&fit_with("abcb", 4, |g| g != "b")), [("a", false), ("?", true), ("c", false), ("?", true)]);
}

#[test]
fn spans_add_up_to_the_line_width() {
    let line = CellLine::new(vec![CellSpan::new("101 ", 4, Color::Cyan), CellSpan::new("Unread", 13, Color::White), CellSpan::new(" 1083", 5, Color::Yellow)]);
    assert_eq!((line.width(), line.spans.iter().map(CellSpan::cells).collect::<Vec<_>>()), (22, vec![4, 13, 5]));
    assert_eq!(CellSpan::new("far too long for the span", 3, Color::White).cells(), 3);
    assert_eq!(CellSpan::new("negative", -2, Color::White).cells(), 0);
}

#[test]
fn reverse_video_keeps_the_text_and_replacements_stay_magenta_on_the_wire() {
    let line = CellLine::new(vec![CellSpan::new("A🎉", 2, Color::Yellow), CellSpan::new("x", 1, Color::White).on(Color::Blue)]);
    assert_eq!(line.encode(), "30A\u{1f}?0?\u{1f}74x");
    let rev = line.reversed(Color::Black, Color::Cyan);
    assert!(rev.spans.iter().all(|s| s.fg == Color::Black && s.bg == Color::Cyan));
    assert_eq!(rev.text(), line.text());
    assert_eq!(decode(&rev.encode()), [PaintRun { fg: Some(Color::Black), bg: Color::Cyan, text: "A".into() },
        PaintRun { fg: None, bg: Color::Cyan, text: "?".into() }, PaintRun { fg: Some(Color::Black), bg: Color::Cyan, text: "x".into() }]);
    assert_eq!(decode_lines(&encode_lines(&[line.clone(), CellLine::blank(2)])).len(), 2);
}

#[test]
fn blank_and_centred_lines() {
    assert_eq!((CellLine::blank(5).width(), CellLine::blank(5).text()), (5, "     ".into()));
    assert!(CellLine::blank(5).spans.iter().all(|s| s.bg == Color::Black));
    assert_eq!(CellLine::blank(0).width(), 0);
    assert_eq!(CellLine::centred("RESSAC", 20, Color::Yellow).text(), "       RESSAC       ");
    assert_eq!(CellLine::centred("A message far longer than ten", 10, Color::Cyan).width(), 10);
}

#[test]
fn teletext_dates_are_english_capitals_in_any_zone() {
    let at = Utc.timestamp_opt(1_791_964_032, 0).unwrap();     // 2026-10-14 07:47:12 UTC
    assert_eq!(date(at, &crate::test_db::Paris, DatePattern::Day), "WED 14 OCT");
    assert_eq!(date(at, &crate::test_db::Paris, DatePattern::Time), "09:47:12");
    assert_eq!(date(Utc.timestamp_opt(1_791_615_600, 0).unwrap(), &Utc, DatePattern::LogDate), "10 OCT 26 07:00");
    let kolkata = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
    assert_eq!(date(at, &kolkata, DatePattern::Time), "13:17:12");
}

#[test]
fn the_screen_is_only_for_antiope_with_the_box_checked() {
    assert!(crt::is_active(Appearance::Antiope, true));
    assert!(![(Appearance::Antiope, false), (Appearance::Native, true), (Appearance::Native, false)].iter().any(|(l, c)| crt::is_active(*l, *c)));
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
    assert!(crt::scanline_bands(0.0, 2.0, 2.0).is_empty() && crt::scanline_bands(100.0, 0.0, 2.0).is_empty());
    assert_eq!(crt::scanline_bands(10.0, 2.0, 1.0)[0], (1.0, 2.0));
}

#[test]
fn tube_corners_and_the_values() {
    assert_eq!([crt::corner_radius(900.0, 500.0), crt::corner_radius(1440.0, 900.0), crt::corner_radius(0.0, 0.0)], [15.0, 27.0, 0.0]);
    assert_eq!([crt::SCANLINE_OPACITY, crt::SCANLINE_FRACTION, crt::VIGNETTE_OPACITY, crt::CORNER_PERCENT, crt::BLEED_OFFSET, crt::BLEED_OPACITY, crt::GLOW_RADIUS],
               [0.28, 0.4, 0.35, 3.0, 1.0, 0.45, 2.0]);
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui teletext 2>&1 | tail -3` → compile error (`teletext` not found).
- [ ] **Step 3: Implement** the five files. `cargo build -p ressac-ui` updates `Cargo.lock` (commit it with the task).
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui teletext 2>&1 | grep "test result"` → `ok.`; then the full gate (B8a1 ends here).
- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ressac-ui/Cargo.toml crates/ressac-ui/src/lib.rs crates/ressac-ui/src/teletext.rs notes.md
git commit -m "ressac-ui: teletext palette, Bedstead's metrics and character map, cell fitting and encoding, CRT geometry

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8a2: pages and keys (`ressac-ui`)

### Task 2 (B8a2): `ressac_ui::pages` (directory, page entry, navigation)

**Files:**
- Create: `crates/ressac-ui/src/pages/mod.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod pages;`)

**Interfaces:**
- Consumes: Task 1 (`teletext::CellLine` for `Line`), B2 `sources::{Selection, GardenScope, rhythm_title}`, B4 `journal::Phase`, core `SourcesSnapshot`, `SourceCounts`, `SourceNode{Kind}`, `Source`, `CadenceGroup`, `FeedId`.
- Produces: the `pages` items of the header (`Line` and `LineRole` included, `frame`/`special`/`garden` come later).

Rules (port of `PageDirectory.swift`, `PageEntry.swift`, `AntiopeState.swift`): reserved titles and order as `store::pages::RESERVED_PAGES`; destinations: 100 → `Index`, 899 → `Guide`, 101–103 → Unread/Today/Starred, 104 → Journal, 105–109 → `Cadence(Daily…Silent)`, 800–802 → `Selection::Garden(NeedsTriage | All | Cemetery)`; counts: unread for 101, today 102, starred 103, `counts.journal` 104, `counts.cadence[group]` 105–109, `needs_triage` 800, 0 otherwise. The directory walks `snapshot.tree` depth-first in its order; a node without a page, or with a reserved number, adds no row (its children are still walked); folders sum their subtree (the core's `SourceNode::unread`). `AntiopeNav::handle` exactly as `AntiopeState.handle` (inventory §5.5; the Mac source is binding), with `Effect`s in place of the calls on `AppState`:

| Command | Not handled when | Effects / state |
|---|---|---|
| `PageDigit(d)` | never | clear NOT FOUND; a completed number → `jump` |
| `PageCorrect` / `PageCancel` | no digit typed | edit the entry |
| `Sommaire` / `Guide` | never | `jump(100)` / `jump(899)` |
| `Repetition` | not P104-retrying and `!can_refresh` | P104 and `journal_retries` → `RetryEdition`; else `Refresh` |
| `ToggleImages` | reader page not `Article`, or no entry | `ToggleImages` |
| `NextColumn` / `PreviousColumn` | no column that way | entering `Entries` → `FocusEntries`; column moves |
| `NextIndexRow` / `PreviousIndexRow` | never (the ends are spent) | `jump` to the neighbour of the current page in `rows()`, or the first row when there is none |
| `NextEntry`, `PreviousEntry`, `Advance` | see below | |
| others | always | |

On the end page: `PreviousEntry` → `Article`; all three handled, nothing else. On `Article`: `PreviousEntry` is not handled; `NextEntry`/`Advance` on P104's last edition entry (selection `Journal`, `selected_entry.0 == last_edition_entry`): `NextEntry` → `land_on_end` (`MarkRead(id)` when unread, reader page `EndOfEdition`); `Advance` → `PageDownThenEnd(id)` once (while `advancing_to_end` further presses are handled with no effect), else not handled. On P100/P899: not handled when `!can_navigate_entries`; else reader page `Article`, handled iff an entry is selected. `jump(n)`: cancel the entry; unknown → NOT FOUND until `now + NOT_FOUND_FOR`, effect `NotFound`; else clear it, `Source(sel)` → `Select(sel)` and `Article`, `Index`/`Guide` → that reader page (selection kept); column `Index`; handled.

- [ ] **Step 1: Write the failing tests** — a fixture `fn paged_snapshot() -> SourcesSnapshot` (port of `pagedSnapshot()`, `PageDirectoryTests.swift:6-32`): tree `[Blogs (folder 1, page 200, unread 3) [xkcd.com (feed 1, 201, 3)], Infos (4, 400, 3) [Daring Fireball (8, 401, 2), Science (folder 5, 402, 1) [Center for Humane Technology (10, 403, 1)]], Loose feed (13, 110, 0), Brand new (12, None, 4)]`, counts unread 10, today 1, starred 4, needs_triage 7. Port, assertions verbatim with `u16` numbers and `Selection`:
  - `reserved_pages_come_first_with_their_counts` (journal 43, cadence Daily 34, Weekly 7, Monthly 2 → counts `[0, 10, 1, 4, 43, 34, 7, 2, 0, 0, 7, 0, 0, 0]`),
  - `folders_and_feeds_follow_in_display_order_with_their_depth` (`[200, 201, 400, 401, 402, 403, 110]`, depths `[0, 1, 0, 1, 1, 2, 0]`, unread `[3, 3, 3, 2, 1, 1, 0]`),
  - `reserved_pages_match_the_core_and_the_rhythm_groups` (`ReservedPage::ALL.map(number) == RESERVED_PAGES`; each group's page title `== rhythm_title(group)`),
  - `numbers_lead_to_their_destinations` (unknown `[0, 7, 99, 437, 799, 900, 999]` → `None`),
  - `sources_and_feeds_find_their_numbers` (`number_of(&Selection::Garden(Cemetery)) == Some(802)`, feed 12 → `None`, `page_of_feed(8) == Some(401)`),
  - `a_number_colliding_with_a_reserved_page_is_ignored` (feed 13 at 101, feed 10 at 104),
  - `is_root_folder` (400 yes, 402 no, 201 no),
  - `the_directory_is_rebuilt_only_when_the_snapshot_changes` (`DirectoryCache::get` twice → `builds() == 1`; a snapshot with one more unread → 2),
  - PageEntry: `the_third_digit_completes_the_number`, `correction_erases_the_last_digit`, `cancel_empties_the_entry`, `leading_zeros_and_stray_values` (`type_digit(12)` ignored; `0 4 2` → `Complete(42)`),
  - AntiopeNav (a `ctx(&dir, selection, now)` helper; `Instant::now()` captured once, offsets added): `a_typed_page_selects_its_source_and_focuses_the_index` (column `Reader`; `4 0` pending with `display() == "40_"`; `1` → `Select(Feed(8))`, column `Index`, `Article`), `correction_and_cancel_edit_the_entry`, `an_unknown_page_shows_not_found_for_two_seconds` (`not_found(now + 1999 ms) == Some(437)`, `now + 2 s` → `None`, effects `[NotFound]`, selection effect absent), `a_new_digit_clears_not_found_at_once`, `reserved_pages_open_their_destination` (100 keeps the selection: no `Select`, reader `Index`, `current_page == Some(100)`; Guide → 899; 103 → `Select(Starred)`, `Article`), `typing_the_rhythm_pages_selects_their_sources` (104, 105, 109), `suite_and_retour_are_the_lists_on_an_article` (`NextEntry`, `Advance`, `PreviousEntry` on `Article` elsewhere → not handled), `suite_from_a_reserved_page_only_brings_the_article_back` (Guide and Index × the three commands: handled, `Article`, then not handled), `repetition_refreshes_or_retries_the_edition`, `images_toggle_only_on_an_article_with_an_entry`, `the_index_arrows_walk_the_pages` (from 101: previous → 100 `Index`; previous again spent; next → `Select(Unread)`; next → `Select(Today)`, column `Index`), `columns_move_left_and_right` (previous from Index not handled; next → `[FocusEntries]`, `Entries`; next → `Reader`; next not handled; previous → `Entries`), `suite_from_the_last_edition_entry_lands_on_the_end_page` (`NextEntry` on the last unread → `[MarkRead(id)]`, `EndOfEdition`, `current_page == Some(104)`; `NextEntry` and `Advance` there handled with no effect; `PreviousEntry` → `Article`; `Advance` → `[PageDownThenEnd(id)]`, a second `Advance` handled with no effect until `advance_finished`; `land_on_end(Some((id, true)))` → no `MarkRead`), `elsewhere_the_last_entry_is_the_lists`, `a_jump_leaves_the_end_page` (→ 101: `Article`, `Select(Unread)`), `repetition_retries_only_an_edition_that_could_not_be_built` (`retries_journal`: Offline, Failed, Due true; Idle, Building false), `outcomes_encode_for_qml` (`Outcome { handled: true, effects: vec![Effect::Select(Selection::Source(Source::Feed(8))), Effect::NotFound] }.encode() == "handled;select:feed:8;notFound"`, not handled → `""`).
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui pages 2>&1 | tail -3` → compile error.
- [ ] **Step 3: Implement** `pages/mod.rs` (`mod frame; mod special; mod garden;` come with their tasks; `Line::action`/`encode` as in the header).
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui pages 2>&1 | grep "test result"` → `ok.`
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/lib.rs crates/ressac-ui/src/pages/mod.rs
git commit -m "ressac-ui: Antiope pages (reserved pages, directory, page entry, navigation with effects)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 3 (B8a2): the Antiope key map

**Files:**
- Modify: `crates/ressac-ui/src/keys.rs` (tests in its `mod tests`)

**Interfaces:**
- Consumes: B3's `keys` (unchanged signatures).
- Produces: the `keys` additions of the header.

Rules (`ReadingKeyRouter.swift:150-185, 238-247`): in `command`, when `look == Antiope` and `focus != TextInput`, `antiope_command` is tried first: `Digit(d)` → `PageDigit(d)`; `Delete` → `PageCorrect`; `Escape` → `PageCancel`; `Option('s')` → `Sommaire`; `Option('g')` and `Question` → `Guide`; `Option('r')` → `Repetition`; `Letter('i')` → `ToggleImages`; `Right`/`Left` → `NextColumn`/`PreviousColumn` unless `focus == Control`; `Down`/`Up` → `NextIndexRow`/`PreviousIndexRow` only in `Sidebar`; else the Native map. `acts_once_per_press`: `PageDigit`, `PageCancel`, `Sommaire`, `Guide`, `Repetition`, `ToggleImages`, `NextColumn`, `PreviousColumn`. `verb(c, true)` for those → `"spent"`; `PageDigit(d)` → `"pageDigit<d>"` from a static array; the others their camelCase name. `classify`: `antiopeReader` counts as `readerPane`, `antiopeEntries` as `entriesList`, `antiopeIndex` as `sourcesList`.

- [ ] **Step 1: Write the failing tests** (port of `KeyboardTests.swift:65-147`):

```rust
#[test]
fn antiope_keys_map_to_their_commands() {
    use KeyFocus::*;
    for focus in [Sidebar, EntryList, Reader, Window, Control] {
        for d in 0..=9 { assert_eq!(command(Key::Digit(d), focus, Appearance::Antiope), Some(Command::PageDigit(d))); }
        assert_eq!(command(Key::Delete, focus, Appearance::Antiope), Some(Command::PageCorrect));
        assert_eq!(command(Key::Escape, focus, Appearance::Antiope), Some(Command::PageCancel));
        assert_eq!(command(Key::Option('s'), focus, Appearance::Antiope), Some(Command::Sommaire));
        assert_eq!(command(Key::Option('g'), focus, Appearance::Antiope), Some(Command::Guide));
        assert_eq!(command(Key::Question, focus, Appearance::Antiope), Some(Command::Guide));
        assert_eq!(command(Key::Option('r'), focus, Appearance::Antiope), Some(Command::Repetition));
        assert_eq!(command(Key::Letter('i'), focus, Appearance::Antiope), Some(Command::ToggleImages));
    }
    for focus in [Sidebar, EntryList, Reader, Window] {
        assert_eq!(command(Key::Right, focus, Appearance::Antiope), Some(Command::NextColumn));
        assert_eq!(command(Key::Left, focus, Appearance::Antiope), Some(Command::PreviousColumn));
    }
    assert_eq!(command(Key::Right, Control, Appearance::Antiope), None);
    assert_eq!(command(Key::Down, Sidebar, Appearance::Antiope), Some(Command::NextIndexRow));
    assert_eq!(command(Key::Up, Sidebar, Appearance::Antiope), Some(Command::PreviousIndexRow));
    assert_eq!(command(Key::Down, EntryList, Appearance::Antiope), Some(Command::NextEntry));
    assert_eq!(command(Key::Up, Reader, Appearance::Antiope), Some(Command::PreviousEntry));
    assert_eq!(command(Key::Option('x'), Reader, Appearance::Antiope), None);
}

/// Review Focus 2.
#[test]
fn digits_in_a_text_field_never_become_page_numbers() {
    for look in [Appearance::Native, Appearance::Antiope] {
        for key in [Key::Digit(4), Key::Delete, Key::Escape, Key::Option('s'), Key::Option('g'), Key::Option('r'), Key::Question,
                    Key::Letter('i'), Key::Left, Key::Right, Key::Up, Key::Down] {
            assert_eq!(command(key, KeyFocus::TextInput, look), None, "{key:?}");
        }
    }
    assert_eq!(route(0x34, "4", 0, false, &[focus::TEXT, "antiopePrompt", focus::READING_PAGE], Appearance::Antiope),
               Route { claims_override: false, verb: None });
}

#[test]
fn reading_and_garden_letters_keep_their_meaning_in_antiope() {
    for focus in [KeyFocus::EntryList, KeyFocus::Reader] {
        for l in ['j', 'k', 'm', 's'] { assert_eq!(command(Key::Letter(l), focus, Appearance::Antiope), command(Key::Letter(l), focus, Appearance::Native)); }
        for l in ['r', 'b', 'u', 'o', 'w'] { assert_eq!(command(Key::Letter(l), focus, Appearance::Antiope), None); }
    }
}

#[test]
fn held_antiope_keys_act_once_per_press() {
    for c in [Command::PageDigit(4), Command::PageCancel, Command::Sommaire, Command::Guide, Command::Repetition,
              Command::ToggleImages, Command::NextColumn, Command::PreviousColumn] {
        assert!(acts_once_per_press(c)); assert_eq!(verb(c, true), "spent", "{c:?}");
    }
    assert_eq!([verb(Command::PageCorrect, true), verb(Command::NextIndexRow, true)], ["pageCorrect", "nextIndexRow"]);
    assert_eq!([verb(Command::PageDigit(0), false), verb(Command::PageDigit(9), false), verb(Command::ToggleImages, false)],
               ["pageDigit0", "pageDigit9", "toggleImages"]);
}

#[test]
fn antiope_columns_classify_their_focus() {
    assert_eq!(classify(&[focus::ANTIOPE_INDEX, "antiopeView", focus::READING_PAGE]), KeyFocus::Sidebar);
    assert_eq!(classify(&[focus::ITEM, focus::ANTIOPE_ENTRIES, "antiopeView"]), KeyFocus::EntryList);
    assert_eq!(classify(&[focus::ANTIOPE_READER, "antiopeView"]), KeyFocus::Reader);
    assert_eq!(classify(&[focus::WEB, "web", focus::READER_PANE, "antiopeView"]), KeyFocus::Reader);
    assert_eq!(route(0x34, "4", 0, false, &[focus::ANTIOPE_INDEX], Appearance::Antiope), Route { claims_override: true, verb: Some("pageDigit4") });
    assert_eq!(route(0x53, "s", qt::ALT, false, &[focus::ANTIOPE_ENTRIES], Appearance::Antiope), Route { claims_override: true, verb: Some("sommaire") });
    assert_eq!(route(0x34, "4", 0, true, &[focus::ANTIOPE_INDEX], Appearance::Antiope), Route { claims_override: true, verb: Some("spent") });
}
```

  B3's `native_never_produces_an_antiope_command` and `menus_never_see_plain_reading_keys_but_keep_their_chords` stay green unchanged.
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui keys 2>&1 | tail -3` → compile errors (`PageDigit`, `acts_once_per_press`, `ANTIOPE_INDEX`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B8a2 ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/keys.rs
git commit -m "ressac-ui: Antiope keys (page digits, CORRECTION, ANNULATION, SOMMAIRE, GUIDE, RÉPÉTITION, columns, images)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8b1: grid decision (1/2) — the painted row and the frame log

### Task 4 (B8b1): `CellRow`, Bedstead registration, frame log, `Recette` hooks

**Files:**
- Create: `apps/linux/src/cellrow.rs`
- Modify: `apps/linux/src/lib.rs` (`mod cellrow;`; shim declarations `ressac_bedstead_family`, `ressac_frame_log_start`, `ressac_frame_log_stop`, `ressac_frame_log_auto`; `run()` calls `shim::ressac_frame_log_auto()` after `ressac_install_key_filter()`)
- Modify: `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp`
- Modify: `apps/linux/src/recette.rs` (`frameLogStart`, `frameLogStop`, `gridLine`, `gridSize`, `gridVariant`)

**Interfaces:**
- Consumes: Task 1 (`teletext::{decode_lines, PaintRun, Color, FAMILY, CellLine, CellSpan}`), B3 `ressac_scheme_serve` (`ressac:asset/bedstead.otf` is one of B3's `ASSETS`), B6c's font registration loop in `ressac_create_application()`.
- Produces: `CellRow`, the shim functions and `Recette` additions of the header.

`CellRow` bridge (the validated shape):

```rust
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
    }
    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QString, line, READ, WRITE = set_line, NOTIFY)]
        #[qproperty(f64, size, READ, WRITE = set_size, NOTIFY)]
        #[qproperty(bool, double_height, READ, WRITE = set_double_height, NOTIFY)]
        #[qproperty(bool, reversed, READ, WRITE = set_reversed, NOTIFY)]
        #[qproperty(i32, reverse_foreground, READ, WRITE = set_reverse_foreground, NOTIFY)]
        #[qproperty(i32, reverse_background, READ, WRITE = set_reverse_background, NOTIFY)]
        type CellRow = super::CellRowRust;
        // set_* : store, emit the notify, recompute implicit size, then `update()`
        #[cxx_override]
        unsafe fn paint(self: Pin<&mut CellRow>, painter: *mut QPainter);
    }
    unsafe extern "RustQt" {
        #[inherit] fn update(self: Pin<&mut CellRow>);
        #[inherit] fn width(self: &CellRow) -> f64;
        #[inherit] #[cxx_name = "setImplicitWidth"] fn set_implicit_width(self: Pin<&mut CellRow>, width: f64);
        #[inherit] #[cxx_name = "setImplicitHeight"] fn set_implicit_height(self: Pin<&mut CellRow>, height: f64);
        #[inherit] #[cxx_name = "setAntialiasing"] fn set_antialiasing(self: Pin<&mut CellRow>, on: bool);
        #[inherit] #[cxx_name = "setFillColor"] fn set_fill_color(self: Pin<&mut CellRow>, color: &QColor);   // black
        #[inherit] #[cxx_name = "setOpaquePainting"] fn set_opaque_painting(self: Pin<&mut CellRow>, on: bool);
    }
    impl cxx_qt::Constructor<()> for CellRow {}   // BaseArguments = (): QQuickPaintedItem's own default
}
```

The decoded rows are cached in the Rust struct on `set_line` (`teletext::decode_lines`). `Initialize` sets antialiasing off, opaque painting, fill colour black. **Paint** (per row `r`, top `y = r × size × (2 if doubleHeight)`): when `doubleHeight`, `painter.set_window(&QRect::new(0, 0, w, size))` and `set_viewport(&QRect::new(0, 0, w, 2 × size))` (validated exact); `QFont`: family `ressac_bedstead_family()` (fallback `teletext::FAMILY`), `set_pixel_size(size as i32)`, `set_style_strategy(QFontStyleStrategy::NoAntialias)`, `set_hinting_preference(QFontHintingPreference::PreferFullHinting)`, `set_kerning(false)`; pass 1: `fill_rect` each run's cells (`cells × 0.6 × size` wide, `size` tall) with its background, or with `reverseBackground` when `reversed`; pass 2: `set_clip_rect` to the row's rectangle, then each run's text with `set_pen(fg)` (`None` → magenta; `reversed` → `reverseForeground` except `None`) and `draw_text(&QPoint::new(x, y + size × 8 / 10), text)`. Colours from `Color::rgb`.

Shim:
- Bedstead: in `ressac_create_application()`'s font loop (B6c), add `bedstead.otf`; keep the family the call returns (`QFontDatabase::applicationFontFamilies(id).value(0)`) in a file-static `QString`; `rust::String ressac_bedstead_family()` returns it (empty on failure, plus `qWarning("ressac-kde: cannot register bedstead.otf")`).
- Frame log: a file-static `struct { QMetaObject::Connection conn; QElapsedTimer clock; std::mutex lock; std::vector<double> ms; bool primed; }`. `ressac_frame_log_start()`: on the first visible `QQuickWindow`, disconnect any previous, clear, connect `frameSwapped` with `Qt::DirectConnection` to a lambda that (under the lock) appends `clock.nsecsElapsed() / 1e6` since the previous frame (the first frame only starts the clock). `ressac_frame_log_stop()`: disconnect, sort, return `"n=%d mean=%.2f p50=%.2f p95=%.2f max=%.2f over16=%d"` (p50 = sorted[n/2], p95 = sorted[n × 95 / 100], `over16` = frames > 16.7 ms; `"n=0"` when empty). `ressac_frame_log_auto()`: when `RESSAC_FRAME_LOG=1`, start, and a `QTimer` every 10 s logs `qInfo("ressac-kde: frames %s", …)` of a stop-then-start.
- `Recette`: `frameLogStart()` / `frameLogStop() -> QString` call the shim when active (inactive: no-op / `""`); `gridLine(index)` = the synthetic entry `index` as `teletext::encode_lines` of two `CellLine`s (`"● "` yellow 2 cells when `index % 3 == 0`, else 2 spaces; a 24-cell white title from the words `Ressac teletext Antiope Minitel dimanche edition jardin PAGE nouvelles BBC Ceefax Oracle`, word `(index × 7 + w × 3) % 12`, cut in two rows of 24; `" *"` white when `index % 5 == 0`; entry 7 reversed black on white); `gridVariant` and `gridSize` from the environment, only when active.

- [ ] **Step 1: Write the failing check** — after adding the declarations to `lib.rs` only: `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` → an undefined `ressac_bedstead_family` (or cxx's missing-definition error).
- [ ] **Step 2: Implement** the five files.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`, every `ok.` (no QML uses `CellRow` yet; the font loop now registers five files). Then the full gate (B8b1 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/cellrow.rs apps/linux/src/lib.rs apps/linux/cpp/shim.h apps/linux/cpp/shim.cpp apps/linux/src/recette.rs
git commit -m "ressac-kde: CellRow painted teletext rows, Bedstead registered, frame-time log

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8b2: grid decision (2/2) — measured in the app, recorded

### Task 5 (B8b2): `GridProbe.qml`, the `grid` walk, the decision in `notes.md`

**Files:**
- Create: `apps/linux/qml/GridProbe.qml`
- Modify: `apps/linux/qml/Main.qml` (`readonly property var gridSteps`; the chooser: `Recette.walk === "grid" ? gridSteps : …`; a `Loader { id: gridLoader; anchors.fill: parent; z: 100; active: false; source: "GridProbe.qml" }` over the page)
- Modify: `notes.md` (« ### Antiope sous KDE : la grille (B8) »)
- Test (not counted): `apps/linux/tests/grid.rs`

**Interfaces:**
- Consumes: Task 4, B4's walk chooser, `tests/common` (`Run`, `copy_fixture`, `launch_with(env)`, `assert_clean`, `shots`).
- Produces: the measurement walk (B8e runs it on the VM).

`GridProbe.qml`: black `Rectangle`; `property string variant` (`"painted"` or `"text"`), `property real size: Recette.gridSize`; a `ListView` (`objectName: "probeList"`, `x: 23 cells`, `y: 1 row`, width 28 cells, `model: 2000`, `reuseItems: true`) whose delegate is, for `painted`, `CellRow { line: Recette.gridLine(index); size: probe.size; width: 28 cells; height: 2 rows }`, and for `text` an `Item` of two rows, each a `Repeater` over the row's runs parsed in JavaScript from the same encoded string (header char 0 = foreground or `?`, char 1 = background, the text after), each run a `Rectangle` background (when not black) and a `Text { renderType: Text.NativeRendering; antialiasing: false; font.family: "Bedstead"; font.pixelSize: size; font.hintingPreference: Font.PreferFullHinting; font.kerning: false; font.preferShaping: false }` at `x = cells before × 0.6 × size`; `function measure(entriesPerFrame, frames, done)`: `Recette.frameLogStart()`, then on each `Window.window.frameSwapped` (a `Connections`) move `contentY` by `entriesPerFrame × 2 rows` (back to 0 past the end), and after `frames` frames `done(Recette.frameLogStop())`; `function pattern()`: the model becomes 4 pattern rows (`█ █ …` white in one span; 28 one-cell `█` of colours 1…7 cycling; `ABCDEFGHIJKLMNOPQRSTUVWXYZ01` white; `m` × 28 yellow on blue), `contentY = 0`.

Walk `gridSteps` (`Recette.gridVariant` picks `painted` or `text`; empty = both, painted first):

```qml
{ name: "g01-pattern", enter: function () { gridLoader.active = true; gridLoader.item.variant = Recette.gridVariant || "painted"; gridLoader.item.pattern() } },
{ name: "g02-scroll", enter: function () { root.walkPaused = true; const probe = gridLoader.item
    probe.measure(1, 600, function (s1) { console.warn("grid: " + probe.variant + " size=" + probe.size + " step=1 " + s1)
    probe.measure(10, 600, function (s10) { console.warn("grid: " + probe.variant + " size=" + probe.size + " step=10 " + s10)
    if (Recette.gridVariant === "" && probe.variant === "painted") { probe.variant = "text"; probe.measure(1, 600, function (t1) {
        console.warn("grid: text size=" + probe.size + " step=1 " + t1); probe.measure(10, 600, function (t10) {
        console.warn("grid: text size=" + probe.size + " step=10 " + t10); root.walkPaused = false }) }) } else root.walkPaused = false }) }) } },
```

(`walkPaused`: a `Main.qml` property the walk `Timer` honours by re-arming itself without advancing; add it if B4–B7 did not.)

- [ ] **Step 1: Write the failing test** `apps/linux/tests/grid.rs`:

```rust
//! The Antiope grid in the real app: exact cells at every size and scale, and the measured frame times (spec §7.9).
#![cfg(target_os = "linux")]
mod common;
use common::*;

const PALETTE: [[u8; 3]; 8] = [[0,0,0],[255,0,0],[0,255,0],[255,255,0],[0,0,255],[255,0,255],[0,255,255],[255,255,255]];

fn mean(err: &str, variant: &str, step: u32) -> f64 {
    let line = err.lines().find(|l| l.starts_with(&format!("grid: {variant} ")) && l.contains(&format!(" step={step} ")))
        .unwrap_or_else(|| panic!("grid: {variant} step={step} missing in {err}"));
    line.split(" mean=").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap()
}

#[test]
fn the_painted_grid_is_exact_at_every_size_and_scale() {
    for scale in ["1", "2"] { for size in ["15", "20", "25", "30"] {
        let run = Run::new();
        let out = run.launch_with(Some(&run.copy_fixture()), &[("RESSAC_RECETTE_WALK", "grid"), ("RESSAC_RECETTE_GRID", "painted"),
            ("RESSAC_RECETTE_GRID_SIZE", size), ("QT_SCALE_FACTOR", scale), ("QT_QPA_UPDATE_IDLE_TIME", "0")]);
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        let img = image::open(run.shots().join("g01-pattern.png")).unwrap().to_rgb8();
        let (px, s): (u32, u32) = (size.parse().unwrap(), scale.parse().unwrap());
        let (cw, x0, y0) = (px * 6 / 10 * s, 23 * px * 6 / 10 * s, px * s);
        for y in y0..y0 + 4 * px * s { for x in x0..x0 + 28 * cw {
            assert!(PALETTE.contains(&img.get_pixel(x, y).0), "{size}px x{scale}: off-palette at {x},{y}");
        } }
        let row = |y: u32| (x0..x0 + 28 * cw).map(|x| img.get_pixel(x, y).0).collect::<Vec<_>>();
        for (i, chunk) in row(y0 + px * s / 2).chunks(cw as usize).enumerate() {         // `█ █ …`: one cell each, exact
            assert!(chunk.iter().all(|p| *p == if i % 2 == 0 { [255, 255, 255] } else { [0, 0, 0] }), "{size}px x{scale} cell {i}");
        }
    } }
}

#[test]
fn the_painted_rows_scroll_faster_than_text_per_span() {
    for scale in ["1", "2"] {
        let run = Run::new();
        let out = run.launch_with(Some(&run.copy_fixture()), &[("RESSAC_RECETTE_WALK", "grid"), ("QT_SCALE_FACTOR", scale), ("QT_QPA_UPDATE_IDLE_TIME", "0")]);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(0), "{err}");
        for step in [1, 10] { assert!(mean(&err, "painted", step) < mean(&err, "text", step), "x{scale} step {step}: {err}"); }
        for line in err.lines().filter(|l| l.starts_with("grid: ")) { println!("x{scale} {line}"); }   // for notes.md
    }
}
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test grid 2>&1 | grep -E "panicked|test result"'` → FAIL (no `g01-pattern.png`).
- [ ] **Step 3: Implement** `GridProbe.qml` and the `Main.qml` steps.
- [ ] **Step 4: Verify and decide** — the same command with `-- --nocapture` → `ok.` for both tests, and the eight `grid:` lines. **Decision rule:** keep `CellRow` when both tests pass (expected, by the planning numbers). If the exactness test fails only at one size, fix the paint rule (per-character `draw_text` at each cell's x is the fallback for an advance that drifts); if painted rows are not faster, stop and report to the orchestrator (the fallback is a `CellText.qml` of the same properties built from `Text` per span, and the B8b5+ tasks change only the delegate). Write in `notes.md` (French, no em dash): « ### Antiope sous KDE : la grille (B8) » with the decision (« une ligne peinte par `CellRow`, `QQuickPaintedItem` de cxx-qt, plutôt qu'un `Text` QML par segment »), the planning table of this file (conteneur, programme C++) and the eight lines measured in the app (scale 1 and 2, step 1 and 10), the pixel check (8 colours, cells of 9/12/15/18 px at scale 1 and 18/24/30/36 at scale 2), and the CRT finding (`ShaderEffect` and `MultiEffect` draw nothing with the software backend; `Canvas` works, +0.15 / +0.4 ms). Then the full gate (B8b2 ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/GridProbe.qml apps/linux/qml/Main.qml notes.md apps/linux/tests/grid.rs
git commit -m "ressac-kde: Antiope grid decided by measurement (painted rows, exact at 15-30 px and scales 1 and 2)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8b3: header, columns, Minitel bar (`ressac-ui`)

### Task 6 (B8b3): `pages::frame`, sub-page labels

**Files:**
- Create: `crates/ressac-ui/src/pages/frame.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/pages/mod.rs` (`pub mod frame;`)
- Modify: `crates/ressac-ui/src/article.rs` (`SubpageLabel::of`, `SubpageLabel::of_edition`, `subpage_script`)

**Interfaces:**
- Consumes: Tasks 1–2, B2 `entries::UNTITLED`, `runs::RunProgress`, B4 `journal::{Phase, JournalView}`.
- Produces:

```rust
// pages::frame
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderLabel { Page(Option<u16>), Typing(String), NotFound(u16), Searching(u16) }
pub fn header_label(entry: &PageEntry, not_found: Option<u16>, current: Option<u16>, rolling: Option<u16>) -> HeaderLabel;
pub fn rolling_page(now: DateTime<Utc>) -> u16;                 // 100 + (tenths since the Unix epoch mod 800)
pub fn header_line<Tz: TimeZone>(label: &HeaderLabel, now: DateTime<Utc>, tz: &Tz, width: usize, right_margin: usize) -> CellLine where Tz::Offset: Display;
pub fn right_margin_cells(window_width: f64, window_height: f64, cell_width: f64, crt: bool) -> usize;
pub fn column_label(column: AntiopeColumn) -> &'static str;      // "INDEX", "ENTRIES", "READER"
pub fn column_title(column: AntiopeColumn, active: bool, width: usize, unread_only: Option<bool>, blank_when_inactive: bool) -> CellLine;
pub fn index_line(row: &DirRow, width: usize, root_folder: bool, selected: bool, highlights_reserved: bool) -> CellLine;
pub fn index_rows(directory: &PageDirectory, current: Option<u16>) -> Vec<Line>;   // role Jump(n), id "page-<n>", 22 cells
pub fn wrap_title(title: &str, width: usize) -> Vec<String>;    // ≤ 2 rows, graphemes
pub fn entry_lines(title: Option<&str>, read: bool, starred: bool, width: usize, selected: bool) -> Vec<CellLine>;
pub fn empty_line(message: &str, width: usize) -> CellLine;    // upper case, centred, cyan
pub fn no_entry_line(width: usize) -> CellLine;                // "NO ENTRY SELECTED", centred, cyan
pub fn failure_notice_line(notice: &str, width: usize) -> CellLine;  // " <notice>" black on red
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinitelKey { Sommaire, Suite, Retour, Guide, Repetition }
impl MinitelKey { pub const ALL: [MinitelKey; 5]; pub fn label(self) -> &'static str; pub fn verb(self) -> &'static str; pub fn line(self) -> CellLine; }
pub const KEYS_CELLS: usize = 49;
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Hint { pub text: String, pub color: Color }
pub fn hint(column: AntiopeColumn, is_garden: bool, search_text: &str) -> Hint;
pub fn status(progress: Option<&RunProgress>, message: Option<&str>, journal: &Phase) -> Option<Hint>;
pub fn hint_line(hint: &Hint, width: usize) -> CellLine;
pub fn progress_text(done: usize, total: usize) -> String;     // "EDITION IN PROGRESS 34/156" or "EDITION IN PROGRESS"
pub const SEARCH_LABEL: &str = "SEARCH? ";
pub fn search_label_line(width: usize) -> CellLine;            // " SEARCH? " cyan, min(9, width) cells
#[derive(Debug, Clone, PartialEq, Eq)] pub struct BannerRow { pub message: CellLine, pub log: Option<CellLine>, pub dismiss: CellLine }
impl BannerRow { pub fn width(&self) -> usize; }
pub fn banner_row(message: &str, show_log: bool, cells: usize) -> BannerRow;
// article
impl SubpageLabel {
    pub fn of(entry_id: i64, listed: &[(i64, FeedId)], total: Option<usize>, page_of: impl Fn(FeedId) -> Option<u16>) -> Option<SubpageLabel>;
    pub fn of_edition(entry_id: i64, view: Option<&JournalView>) -> Option<SubpageLabel>;   // page 104, rank, items.len()
}
pub fn subpage_script(text: &str) -> String;   // app world: const line = document.getElementById("subpage"); if (line) { line.textContent = <js_string(text)>; }
```

Rules: the Mac's `PageHeader.line`, `PageIndexColumn.line`, `EntryPageColumn.{lines, wrap, emptyLine}`, `ColumnTitle.line`, `MinitelKey`, `MinitelBar.{hint, status, hintLine}`, `SearchPrompt.label`, `GardenPage.bannerRow`, `AntiopeReaderLines` (inventory §5.2–§5.4; the Swift is binding), with these Linux specifics: `right_margin_cells` = `0` without CRT, else `ceil((corner_radius + BLEED_OFFSET + GLOW_RADIUS) / cell_width)`; `MinitelKey::verb` = `"sommaire"`, `"advance"`, `"previousEntry"`, `"guide"`, `"repetition"`; `status` order: the garden message (yellow), `Phase::Building{done,total}` → `progress_text` (yellow), a run's progress → `"REFRESHING <done>/<total>"` or `"REFRESHING"` (yellow), else `None`; `index_rows` reverses the current page's row (black on cyan).

- [ ] **Step 1: Write the failing tests** (port of `AntiopeFrameTests.swift` minus `CellLineTests` and the VoiceOver labels, `EmptyStateTests.theMessageIsOneCentredCyanLine`, `AntiopeBannerTests`, `SearchPromptTests.thePromptAsksAsATerminalWould`, `SubpageLabelTests`, `EditionSubpageTests`; `const AWKWARD: [&str; 10]` = the Mac's `awkwardTitles` verbatim; `fn t(span: &CellSpan) -> String { span.text() }`; `paged_snapshot()` from Task 2 moved to `test_db.rs` as `pub fn paged_snapshot()`). Names and key assertions:
  - `every_index_row_is_exactly_22_cells` (directory rows + `AWKWARD` at depth 1 with 3 unread + `Huge` 1 234 567 unread + `Deep` depth 9, selected × highlights; widths `[0, 1, 3, 4, 8, 11, 30]`),
  - `index_colours_follow_the_spec` (`["101 ", "Unread       ", " 1083"]`, `[Cyan, White, Yellow]`; INFOS green; `" Science"` white; 800 `[Cyan, Magenta, Red]`; 802 magenta with an empty count; selected all black on cyan),
  - `the_sommaire_colours_the_other_reserved_titles`,
  - `every_entry_row_is_exactly_28_cells` (every `AWKWARD` title and `None` × read × starred × selected: 1 or 2 rows of 28; widths `[0, 1, 3, 4, 10]`),
  - `markers_colours_and_reverse_video` (`"● "` yellow; `"Apple announces " + 8 spaces`; `" *"` white; second row `"  "`/`"  "`; selected black on white),
  - `titles_wrap_on_words_over_two_rows_at_most` (the Mac's seven cases, `"Café 🎉 東京 ensemble"` at 10 → `["Café 🎉 東京", "ensemble"]`),
  - `every_title_is_exactly_as_wide_as_its_column`, `only_the_active_column_is_in_reverse_video`, `the_entries_title_shows_the_filter` (`" ENTRIES  ALL" + 15 spaces`), `the_heading_pages_blank_the_reader_title_unless_active`, `blue_never_colours_a_title`,
  - `the_keys_are_the_minitels_in_reverse_video` (labels `["SOMMAIRE", "SUITE", "RETOUR", "GUIDE", "RÉPÉTITION"]`, verbs, `" LABEL "`, `KEYS_CELLS == 49`),
  - `hints_follow_the_search_the_garden_and_the_focus` (`"SEARCH: tidal"` cyan; `"k keep · r repair · b bury · u undo"`; `"type a page number · ? guide"`; `"j next · k previous · m read"`; `"m read · s star · i images"`),
  - `the_hint_line_fills_the_rest_of_the_bar`,
  - `the_minitel_bar_shows_refusals_then_the_edition_then_runs_then_hints` (`"REFRESHING 42/155"`, `"REFRESHING"` with total 0, the message first, `"EDITION IN PROGRESS 34/156"` while building, `None`),
  - `label_precedence`, `the_header_fills_the_width_with_page_name_date_and_time` (`at = 1_791_964_032`, `test_db::Paris`, width 40 → `["P412 ", "RESSAC          ", "WED 14 OCT ", "09:47:12"]`, `[Yellow, White, Cyan, Yellow]`; widths `[0, 5, 8, 19, 24, 75, 160]`; width 8 keeps `"09:47:12"`),
  - `the_time_stays_clear_of_the_tube_corner_at_every_size` (sizes 15–30 × windows 700 × 500, 1000 × 700, 1600 × 1000 × CRT on/off, as the Mac's; `header_line(…, width 8, margin 9)` keeps the time),
  - `typing_not_found_and_no_page` (`"P41_ "`, `"P437 NOT FOUND "` red with a blank name, `"P--- "`),
  - `the_counter_rolls_through_every_page` (consecutive tenths step by one, wrapping 899 → 100; 1 600 steps in 100…899; a negative instant too),
  - `blue_never_colours_text` (every row these functions make, as `AntiopeRootTests.blueNeverColoursText`),
  - `the_empty_message_is_one_centred_cyan_line` (`"     NO UNREAD ENTRIES      "`),
  - `the_banner_row_fills_the_window_width` (60 cells with LOG, 40 without; message white on red, keys yellow on red; `" ressac exited with code 1"` prefix),
  - `the_search_label_asks_as_a_terminal_would` (`SEARCH_LABEL == "SEARCH? "`, `search_label_line(30).text() == " SEARCH? "`),
  - `index_rows_reverse_the_current_page` (`index_rows(&dir, Some(401))`: only `page-401` reversed; all 22 cells; actions `"jump:<n>"`),
  - in `article.rs`: `an_entry_is_a_sub_page_of_its_feed_by_its_position` (`of(3, &[(1, 8), (3, 8), (4, 1)], Some(20), |f| (f == 8).then_some(412))` → `"412 2/20"`; `None` total → `/3`; a feed without page → `"2/20"`; absent → `None`), `an_edition_entry_is_a_sub_page_of_104_by_its_rank` (`"104 3/4"`, `"104 1/4"`, absent and `None` view → `None`), `the_subpage_script_rewrites_the_line_in_place` (contains `getElementById("subpage")` and `js_string("412 3/20")`).
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui frame 2>&1 | tail -3` → compile error.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B8b3 ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/pages/mod.rs crates/ressac-ui/src/pages/frame.rs crates/ressac-ui/src/article.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: Antiope frame rows (header, column titles, index and entry rows, Minitel bar, banner) and sub-page labels

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8b4: the `Antiope` and `CellRowsModel` bridges

### Task 7 (B8b4): `Antiope` singleton, `CellRowsModel`, `EntriesModel` and `Runs` additions

**Files:**
- Create: `apps/linux/src/antiope.rs`, `apps/linux/src/cellrows.rs`
- Modify: `apps/linux/src/lib.rs` (`mod antiope; mod cellrows;`)
- Modify: `apps/linux/src/entries.rs` (role `antiopeLines`; `selectedFeedId`, `total`)
- Modify: `apps/linux/src/runs.rs` (`progressDone`, `progressTotal`)
**Interfaces:**
- Consumes: Tasks 1–3, 6; B2's `list_model_rows!`, `live::plan_rows`, `with_session`, `Session.store`; core `Store::{sources, journal}`, `clock::today_start`.
- Produces: `Antiope` (the B8b part of its header list: everything except `journalRows`, `readerRows`, `bannerRow`, `scanlineBands`, which Task 11 and B9 add), `CellRowsModel`, the `EntriesModel` and `Runs` additions.

Behaviour:
- `Antiope.reload()`: `with_session(|s| s.store.sources(today_start(Local::now())))` → `DirectoryCache::get`; a read error keeps the last directory and logs (`tracing::warn!`); then recompute everything and emit `stateChanged`. Every `WRITE` input recomputes what depends on it and emits `stateChanged` only when an output changed.
- Outputs: `currentPage` = `nav.current_page(directory, selection)`; `header` = `header_line(header_label(entry, not_found(now), current, rolling), Utc::now(), &Local, windowCells, right_margin_cells(windowWidth, windowHeight, cellWidth, crt))` encoded, `rolling = (runActive && !reduceMotion).then(|| rolling_page(now))`; `indexTitle`/`entriesTitle`/`readerTitle` = `column_title(…)` (entries: `unread_only = (!isGarden).then_some(unreadOnly)`; reader: `blank_when_inactive = readerPage != article`) at 22, 28 and `readerCells`; `indexRows` = `encode_rows(index_rows(…))`; `minitel` = `hint_line(status(…).unwrap_or(hint(…)), windowCells − 49)` encoded (`RunProgress { done: runDone, total: runTotal }` when `runActive`; the journal phase rebuilt from `journalPhase`/`journalDone`/`journalTotal`/`journalFailure`); `subpage*` = `of_edition` on P104 (the journal view read at reload), else `of(selectedEntryId, &[(selectedEntryId, selectedFeedId)], Some(entryTotal), page_of_feed)` with index `entryPosition` (a trio of `-1, 0, 0` when none).
- `perform(verb)`: `"pageDigit<d>"` → `Command::PageDigit(d)`, the other Antiope verbs and `nextEntry`, `previousEntry`, `advance`, `advanceHeld` (as `Advance`) → `nav.handle(command, &ctx)` → `Outcome::encode()`; any other verb → `""`. `jump(n)` likewise; `landOnEnd()` → `nav.land_on_end(…)`; `tick()` recomputes the header and expires NOT FOUND. `ctx`: the directory, `Selection::parse_key(selectionKey)`, `(selectedEntryId, selectedRead)` when ≥ 0, `last_edition_entry` from the journal view, `retries_journal(phase)`, `canRefresh`, `canNavigate`, `Instant::now()`.
- `CellRowsModel`: `WRITE rows` parses each string (`split('\u{1d}')` into id, flags, action, line) and applies `live::plan_rows` by id through `list_model_rows!`; roles in order `rowId`, `line`, `doubleHeight`, `selected`, `action`.
- `EntriesModel.antiopeLines` = `teletext::encode_lines(&entry_lines(row.title, row.read, row.starred, 28, false))`.

- [ ] **Step 1: Write the failing check** — after adding `mod antiope; mod cellrows;` to `lib.rs` only: `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` → `file not found for module`. (The bridges' behaviour is pinned by `ressac-ui`'s tests of Tasks 2, 3 and 6; their wiring by Task 8b's walk test `bridges_load_and_answer_keys`, which `Main.qml` makes possible.)
- [ ] **Step 2: Implement** the five files.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`, every `ok.` Then the full gate (B8b4 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/antiope.rs apps/linux/src/cellrows.rs apps/linux/src/lib.rs apps/linux/src/entries.rs apps/linux/src/runs.rs
git commit -m "ressac-kde: Antiope singleton (navigation, directory, header, index, Minitel bar), CellRowsModel

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8b5: reader and settings bridges, the Antiope QML

### Task 8a (B8b5): `Reader` sub-page, `AppSettings` look, `AntiopeView.qml`, `MinitelBar.qml`, `CellList.qml`

**Files:**
- Modify: `apps/linux/src/reader.rs` (`subpagePage`, `subpageIndex`, `subpageCount`, `subpageScript()`)
- Modify: `apps/linux/src/settings.rs` (`bedsteadAvailable` = `!shim::ressac_bedstead_family().is_empty()`; `effectiveAppearance` = `Appearance::effective(stored, bedsteadAvailable).as_str()`, notified with `appearance`; one `tracing::error!` when Antiope is stored and Bedstead is missing)
- Create: `apps/linux/qml/AntiopeView.qml`, `apps/linux/qml/MinitelBar.qml`, `apps/linux/qml/CellList.qml`

**Interfaces:**
- Consumes: Task 7; B3's `ReaderPane.qml` (instantiated inside `AntiopeView`), `Reader`; B2's models; B4 `Journal`; B5 `Garden.message`.
- Produces: the QML files of the header (P104, garden pages, special pages, prompt and banner come in Task 12; mosaics in Task 17).

`CellList.qml` (`ListView`, `clip: true`, `boundsBehavior: Flickable.StopAtBounds`, no scroll bar, `keyNavigationEnabled: false`): `property alias rows: rowsModel.rows`, `property real size`, `signal activated(string action, real x)`; `model: CellRowsModel { id: rowsModel }`; delegate `CellRow { line: model.line; size: list.size; doubleHeight: model.doubleHeight; width: list.width; height: (doubleHeight ? 2 : 1) * size; MouseArea { anchors.fill: parent; enabled: model.action !== ""; onClicked: (m) => list.activated(model.action, m.x) } }`; `function showSelected()` → `positionViewAtIndex(rowsModel.rowOfSelected(), ListView.Contain)` when ≥ 0, called on `rowsChanged`.

`MinitelBar.qml` (`Row`, height one row): five `CellRow { line: Antiope.keyLines[i] }` with a `MouseArea` → `readingPage.perform(Antiope.keyVerbs[i])`, each followed by one blank cell; then `CellRow { line: Antiope.minitel }` (Task 12 swaps in the prompt).

`AntiopeView.qml` (`FocusScope`, `objectName: "antiopeView"`, black `Rectangle` background): `size = AppSettings.readerSizeAntiope`, `cellWidth = 0.6 × size`; bindings into `Antiope` (`selectionKey: SourcesModel.selectedKey`, `searchText: EntriesModel.searchText`, `isGarden: SourcesModel.isGarden`, `unreadOnly: EntriesModel.unreadOnly`, `selectedEntryId`, `selectedRead`, `selectedFeedId: EntriesModel.selectedFeedId`, `entryPosition: EntriesModel.selectedRow + 1`, `entryTotal: EntriesModel.total`, `canRefresh: Runs.canRefresh`, `canNavigate: EntriesModel.canNavigate`, `runActive: Runs.longRunActive`, `runDone: Runs.progressDone`, `runTotal: Runs.progressTotal`, `reduceMotion: Kirigami.Units.longDuration <= 1`, `gardenMessage: Garden.message`, `journalPhase: Journal.phase`, `windowCells: Math.floor(width / cellWidth)`, `readerCells: windowCells − 52`, `windowWidth/Height: Window.window` size, `cellWidth`, `crt: false` (B9 binds it)) set through `Binding` elements; `Reader.subpagePage/Index/Count` bound to `Antiope.subpage*`. Layout top to bottom: header `CellRow { line: Antiope.header }`; three columns in a `Row` (index 22 cells: title row + `CellList { objectName: "antiopeIndex"; rows: Antiope.indexRows }`; a separator `Item` one cell wide holding a blue `Rectangle` of half a cell on its right, full height; entries 28 cells: title row (a `MouseArea` on it: column entries, toggles `EntriesModel.unreadOnly` when `!SourcesModel.isGarden`) + `ListView { objectName: "antiopeEntries"; model: EntriesModel; delegate: CellRow { line: model.antiopeLines; reversed: index === EntriesModel.selectedRow; reverseForeground: 0; reverseBackground: 7 } }` with `Component.onCompleted: EntriesModel.rowAppeared(index)` on the delegate and `positionViewAtIndex(EntriesModel.selectedRow, ListView.Contain)` on `selectionChanged`, and the empty line `CellRow { line: Antiope.emptyLine(EntriesModel.emptyText) }` when the list is empty; separator; reader: title row + `ReaderPane { id: antiopeReader; objectName: "readerPane" }` when `Antiope.readerPage === "article"` (P100, P899 and the end page come in Task 12)); `MinitelBar` at the bottom. Clicks: an index row → `apply(Antiope.jump(n))`; an entry → `EntriesModel.select(index)`, `Antiope.showArticle()`, `Antiope.column = "entries"`. `function perform(verb)` = `apply(Antiope.perform(verb))`; `function apply(encoded)`: `""` → false; for each `;` part after `handled`: `select:<key>` → `SourcesModel.selectKey(key)`; `retry` → `Journal.retry()`; `refresh` → `Runs.refreshSource(SourcesModel.selectedKey)`; `images` → nothing until Task 17; `focusEntries` → `if (!SourcesModel.isGarden) EntriesModel.selectFirstIfNone()`; `markRead:<id>` → when it is still the selected unread entry, `EntriesModel.toggleRead(EntriesModel.selectedRow)`; `pageDownThenEnd:<id>` → `reader.pageDown(id, (spent) => { if (!spent && EntriesModel.selectedEntryId === id && Antiope.readerPage === "article") apply(Antiope.landOnEnd()); Antiope.advanceFinished() })`; `notFound` → `notFoundTimer.restart()` (2 s, then `Antiope.tick()`); then `focusColumn()`; return true. `focusColumn()` gives active focus to the index list, the entries list, or (reader) `antiopeReader.focusWeb()` / the reader list. A `Timer { interval: Antiope.runActive && !reduceMotion ? 100 : 1000; repeat: true; running: true; onTriggered: Antiope.tick() }`. `onActiveFocusChanged` of each column item sets `Antiope.column`. `Component.onCompleted: Antiope.reload()`.

- [ ] **Step 1: Write the failing check** — `apps/linux/dev/dev.sh sh '/usr/lib64/qt6/bin/qmllint -I "$CARGO_TARGET_DIR" apps/linux/qml/AntiopeView.qml apps/linux/qml/MinitelBar.qml apps/linux/qml/CellList.qml 2>&1 | tail -3'` → file not found.
- [ ] **Step 2: Implement** the five files.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`, every `ok.` (nothing loads `AntiopeView` yet; qmllint warnings about unresolved singletons are acceptable, syntax errors are not). Then the full gate (B8b5 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/reader.rs apps/linux/src/settings.rs apps/linux/qml/AntiopeView.qml apps/linux/qml/MinitelBar.qml apps/linux/qml/CellList.qml
git commit -m "ressac-kde: Antiope view (header, index and entries columns, reader, Minitel bar), sub-page and effective look

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8b6: the look switch and its walk

### Task 8b (B8b6): `ReadingPage` switch, `ReaderPane.active`, `Main.qml` chrome and walks

**Files:**
- Modify: `apps/linux/qml/ReadingPage.qml` (Step 0 first)
- Modify: `apps/linux/qml/ReaderPane.qml`
- Modify: `apps/linux/qml/Main.qml` (Step 0 first)
- Test (not counted): `apps/linux/tests/antiope.rs`

**Interfaces:**
- Consumes: Task 8a; B3's `perform`, `advance`, `openInBrowser`, `reloadAll`, `focusName`, the key filter (finds `readingPage`, reads `look`).
- Produces: the look switch; walks `antiope` and `antiope-scroll`; `ReadingPage.{antiope, activeReader, antiopeView}`.

Changes:
- `ReadingPage.qml`: `property string look: AppSettings.effectiveAppearance`; `readonly property bool antiope: look === "antiope"`; the native content's root item gets `visible: !antiope`; `Loader { id: antiopeLoader; anchors.fill: parent; active: antiope; source: Qt.resolvedUrl("AntiopeView.qml"); onLoaded: item.focusColumn() }`; `readonly property Item antiopeView: antiopeLoader.item`; `readonly property Item activeReader: antiope && antiopeLoader.item ? antiopeLoader.item.reader : readerPane`; native `readerPane.active: !antiope`; `perform(verb)`: `if (verb === "spent") return true;` then `if (antiope && antiopeLoader.item && antiopeLoader.item.perform(verb)) return true;` then the Native table, whose `advance()` and `openInBrowser()` use `activeReader`; `reloadAll()` also `if (antiope) Antiope.reload()`; `focusName()` answers `"antiope-" + Antiope.column` in Antiope; `onAntiopeChanged: if (!antiope) focusSidebar()` (the keyboard never stays on a destroyed Antiope item, and Native's letters work at once).
- `ReaderPane.qml`: `property bool active: true`; while inactive it ignores `Reader.pageUrl` changes (no navigation), and on becoming active it loads the current `pageUrl`; `Connections { target: Reader; function onSubpageChanged() { if (pane.active) pane.runAppScript(Reader.subpageScript(), function () {}) } }`.
- `Main.qml`: the `Kirigami.Page`'s `background` black when `readingLoader.item && readingLoader.item.antiope`; `minimumWidth` = `Antiope.minimumCells × 0.6 × AppSettings.readerSizeAntiope` in Antiope, else B2's 900; walks:

```qml
readonly property var antiopeSteps: [
  { name: "a01-antiope", enter: function () { AppSettings.appearance = "antiope" } },
  { name: "a02-dial", enter: function () { const p = readingLoader.item
      console.warn("walk: a01-antiope look=" + p.look + " page=" + Antiope.currentPage + " column=" + Antiope.column + " focus=" + p.focusName())
      Recette.key(Qt.Key_2, "2", 0); Recette.key(Qt.Key_0, "0", 0); Recette.key(Qt.Key_1, "1", 0)
      console.warn("walk: a02-dial selected=" + SourcesModel.selectedKey + " page=" + Antiope.currentPage) } },
  { name: "a03-not-found", enter: function () { Recette.key(Qt.Key_4, "4", 0); Recette.key(Qt.Key_3, "3", 0); Recette.key(Qt.Key_7, "7", 0)
      console.warn("walk: a03-not-found selected=" + SourcesModel.selectedKey + " page=" + Antiope.currentPage) } },
  { name: "a04-columns", enter: function () { Recette.key(Qt.Key_Right, "", 0)
      console.warn("walk: a04-entries column=" + Antiope.column + " selected=" + EntriesModel.selectedEntryId); Recette.key(Qt.Key_Right, "", 0) } },
  { name: "a05-sommaire", enter: function () { console.warn("walk: a04-reader column=" + Antiope.column + " focus=" + readingLoader.item.focusName())
      Recette.key(Qt.Key_S, "s", Qt.AltModifier); console.warn("walk: a05-sommaire reader=" + Antiope.readerPage + " page=" + Antiope.currentPage) } },
  { name: "a06-guide", enter: function () { Recette.key(Qt.Key_Question, "?", Qt.ShiftModifier)
      console.warn("walk: a06-guide reader=" + Antiope.readerPage + " page=" + Antiope.currentPage) } },
  { name: "a07-suite", enter: function () { const before = EntriesModel.selectedEntryId; Recette.key(Qt.Key_J, "j", 0)
      console.warn("walk: a07-suite reader=" + Antiope.readerPage + " same=" + (EntriesModel.selectedEntryId === before)) } },
  { name: "a08-index-arrows", enter: function () { Recette.key(Qt.Key_Left, "", 0); Recette.key(Qt.Key_Left, "", 0); Recette.key(Qt.Key_Down, "", 0)
      console.warn("walk: a08-index-arrows column=" + Antiope.column + " page=" + Antiope.currentPage) } },
  { name: "a09-native", enter: function () { AppSettings.appearance = "native"
      console.warn("walk: a09-native look=" + readingLoader.item.look + " selected=" + SourcesModel.selectedKey + " focus=" + readingLoader.item.focusName()) } },
  { name: "a10-awkward", enter: function () { AppSettings.appearance = "antiope"; SourcesModel.selectKey("feed:1") } },
]
```

  (`a10-awkward` runs on a copy where the test renamed entry titles to the `AWKWARD` titles.) `antiopeScrollSteps`: `{ name: "s01-folder", enter: … AppSettings.appearance = "antiope"; SourcesModel.selectKey("feed:1") }`, `{ name: "s02-scroll", enter: … walkPaused = true; readingLoader.item.antiopeView.scrollProbe(1, 600, s1 => { console.warn("frames: antiope step=1 " + s1); …scrollProbe(10, 600, s10 => { console.warn("frames: antiope step=10 " + s10); walkPaused = false }) }) }` — `AntiopeView.scrollProbe` is `GridProbe.measure`'s loop applied to `antiopeEntries`. Chooser: `Recette.walk === "antiope" ? antiopeSteps : Recette.walk === "antiope-scroll" ? antiopeScrollSteps : Recette.walk === "antiope-bridges" ? bridgeSteps : …`, where `bridgeSteps` = `[{ name: "b00", enter: function () { AppSettings.appearance = "antiope" } }, { name: "b01", enter: function () { const before = Antiope.currentPage; const d = Antiope.perform("pageDigit2"); Antiope.perform("pageDigit0"); const s = Antiope.perform("pageDigit1"); readingLoader.item.antiopeView.apply(s); Antiope.tick(); console.warn("walk: b01 current=" + before + " index=" + Antiope.indexRows.length + " digit=" + d + " select=" + s + " header=" + (Antiope.header.indexOf("P201") >= 0 ? "P201" : Antiope.header)) } }]`.

- [ ] **Step 1: Write the failing test** (create `apps/linux/tests/antiope.rs`):

```rust
//! The Antiope look in the real app (spec §7.9; inventory §5).
#![cfg(target_os = "linux")]
mod common;
use common::*;

/// Task 7's wiring: the directory loads, digits route to Antiope, the header follows.
#[test]
fn bridges_load_and_answer_keys() {
    let run = Run::new();
    let out = run.launch_with(Some(&run.copy_fixture()), &[("RESSAC_RECETTE_WALK", "antiope-bridges")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    // 14 reserved rows + the fixture's numbered folders and feeds (count them once with sqlite3 and write the number here).
    assert!(err.contains("walk: b01 current=101 index=<14 + numbered> digit=handled select=handled;select:feed:1 header=P201"), "{err}");
}

#[test]
fn the_antiope_look_reads_pages_keys_and_columns() {
    let run = Run::new();
    let out = run.launch_with(Some(&run.copy_fixture()), &[("RESSAC_RECETTE_WALK", "antiope")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in ["walk: a01-antiope look=antiope page=101 column=index focus=antiope-index",
                 "walk: a02-dial selected=feed:1 page=201",
                 "walk: a03-not-found selected=feed:1 page=201",
                 "walk: a04-entries column=entries selected=1", "walk: a04-reader column=reader focus=antiope-reader",
                 "walk: a05-sommaire reader=index page=100", "walk: a06-guide reader=guide page=899",
                 "walk: a07-suite reader=article same=true", "walk: a08-index-arrows column=index page=202",
                 "walk: a09-native look=native selected=feed:1 focus=sidebar"] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    // a03: the header reads P437 NOT FOUND in red (the first 4 cells hold red pixels), RESSAC hidden.
    let shot = image::open(run.shots().join("a03-not-found.png")).unwrap().to_rgb8();
    assert!(has_colour(&shot, row_rect(0, 0, 18, 20, 1.0), [255, 0, 0]));
    // a01: black page, a blue separator half-cell right of the index column on every row of the columns.
    let a01 = image::open(run.shots().join("a01-antiope.png")).unwrap().to_rgb8();
    assert_eq!(a01.get_pixel(5, a01.height() - 5).0, [0, 0, 0]);
    for y in (40..a01.height() - 40).step_by(37) { assert_eq!(a01.get_pixel(22 * 12 + 9, y).0, [0, 0, 255], "y {y}"); }
}

/// Review Focus 1: awkward titles never bend a column.
#[test]
fn awkward_titles_keep_the_separators_straight() { /* rename entries 1..=6 of a copy to AWKWARD titles with rusqlite, run the
    walk, then on a10-awkward.png: the separator columns x = 22*12+9 and x = 51*12+9 are blue on every 20 px row of the
    columns area, and nothing but the 8 palette colours appears in the index and entries columns */ }

#[test]
fn scrolling_two_thousand_entries_is_logged() {
    let run = Run::new();
    let db = run.copy_fixture();
    insert_entries(&db, 1, 2000);                                    // helper: 2000 entries in feed 1, titles that wrap
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "antiope-scroll"), ("QT_QPA_UPDATE_IDLE_TIME", "0")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    for step in [1, 10] {
        let line = err.lines().find(|l| l.starts_with(&format!("frames: antiope step={step} n="))).expect(&err);
        let p95: f64 = line.split(" p95=").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        assert!(p95 < 16.7, "{line}");                                // smooth in the container's software renderer
        println!("{line}");
    }
}
```

  (`has_colour`, `row_rect` and `insert_entries` go in `tests/common/mod.rs`, not counted; the expected `a08` page is the directory row after 201 in the fixture (202 here: folder potes), adjust to the fixture's real next page found with `sqlite3`.)
- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test antiope 2>&1 | grep -E "panicked|test result"'` → FAIL.
- [ ] **Step 3: Implement** (Step 0 commits first for `ReadingPage.qml` and `Main.qml`).
- [ ] **Step 4: Verify and look** — the same command with `-- --nocapture` → `ok.`; every suite (`cargo test --locked -p ressac-kde`) → every `ok.` (B3's `keys` walk unchanged in Native). Open `a01-antiope.png`, `a03-not-found.png`, `a05-sommaire.png` (empty reader column until Task 12), `a09-native.png` on the Mac: Bedstead crisp, the eight colours, `P101 RESSAC … WED …` header, the Minitel keys in black on cyan. Record the two `frames:` lines in the ledger. Then the full gate (B8b6 and master B8b end here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/ReadingPage.qml apps/linux/qml/ReaderPane.qml apps/linux/qml/Main.qml apps/linux/tests/antiope.rs apps/linux/tests/common/mod.rs
git commit -m "ressac-kde: live switch to Antiope (keys first to Antiope, native panes hidden, black page), walks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8c1: special and garden pages (`ressac-ui`)

### Task 9 (B8c1): `pages::special` (P100, P899, end page, P104)

**Files:**
- Create: `crates/ressac-ui/src/pages/special.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/pages/mod.rs` (`pub mod special;`)

**Interfaces:**
- Consumes: Tasks 1, 2, 6; B4 `journal::{Phase, section_title, read_count, is_finished}`; core `JournalView`, `JournalItem`, `EditionOverflowRow`, `edition::Section`.
- Produces:

```rust
pub const WAVE_TOP: &str = " ▄▀▀▄   ";  pub const WAVE_BOTTOM: &str = "▀    ▀▄▄";
pub fn sommaire_pairs(rows: &[DirRow]) -> Vec<(usize, Option<usize>)>;   // column-major indices
pub fn half_width(width: usize) -> usize;                                 // (width − 1) / 2, 0 at 0
pub fn sommaire_line(row: &DirRow, width: usize, directory: &PageDirectory) -> CellLine;
pub fn sommaire_title(width: usize) -> CellLine;                          // "RESSAC" centred yellow
pub fn sommaire_logo(width: usize) -> [CellLine; 2];
pub fn sommaire_page(directory: &PageDirectory, width: usize) -> Vec<Line>;  // title (double), logo ×2, blank, pairs (role Pair)
pub struct GuideKey { pub keys: &'static str, pub action: &'static str }
pub struct GuideSection { pub title: &'static str, pub keys: &'static [GuideKey] }
pub const GUIDE_SECTIONS: [GuideSection; 5];
pub const GUIDE_KEY_CELLS: usize = 13;
pub fn guide_lines(width: usize) -> Vec<CellLine>;
pub fn guide_page(width: usize) -> Vec<Line>;                            // "GUIDE" double height, then guide_lines
pub fn end_line(width: usize) -> Line;                                   // "FIN DE L'EDITION", double height, yellow, id "end"
pub fn reader_page_lines(page: ReaderPage, directory: &PageDirectory, width: usize) -> Vec<Line>;  // Article → []
pub fn section_color(section: Section) -> Color;                         // Front yellow, Week cyan, Wire green
pub fn journal_page<Tz: TimeZone>(view: Option<&JournalView>, phase: &Phase, selected: Option<i64>, cells: usize, tz: &Tz) -> Vec<Line> where Tz::Offset: Display;
pub fn journal_header_lines<Tz: TimeZone>(view: &JournalView, cells: usize, tz: &Tz) -> Vec<Line> where Tz::Offset: Display;
pub fn journal_heading_line(section: Section, count: usize, cells: usize) -> Line;
pub fn journal_entry_lines(item: &JournalItem, selected: bool, cells: usize) -> Vec<Line>;
pub fn overflow_line(row: &EditionOverflowRow, cells: usize) -> Line;
pub fn last_entry_id(view: Option<&JournalView>) -> Option<i64>;
```

The Linux GUIDE (deviation 5): PAGES `0-9` type a page number · `BACKSPACE` erase the last digit · `ESC` cancel the number · `ALT S` SOMMAIRE: every page · `ALT G  ?` GUIDE: this page; READING `SPACE  J` SUITE: next page · `K` RETOUR: previous page · `UP DOWN` previous or next entry · `M` mark read or unread · `S` star or unstar · `I` mosaics or original images · `ALT R` RÉPÉTITION: refresh · `CTRL RETURN` open in the browser; COLUMNS `LEFT RIGHT` change column · `UP DOWN` in the index: previous or next page; GARDEN `K` keep · `R` repair · `B` bury · `U` undo · `O` open the site · `W` open the Wayback snapshot · `SHIFT R` apply the safe repairs; MENUS `CTRL F` search: RETURN keeps it, ESC clears it · `CTRL R  F5` refresh the selection · `CTRL SHIFT R` refresh every feed · `CTRL SHIFT A` mark all as read · `CTRL + -` bigger or smaller text · `CTRL O` import OPML · `CTRL SHIFT E` export OPML. P104 rules: `JournalPage.lines` (inventory §5.4; ids `header`, `unreachable`, `blank-<section>`, `heading-<section>`, `entry-<id>-<row>`, `overflow-<section>-<feed>`, `blank-end`, `end`, `blank-message`, `message`, `retry`, `blank-empty`, `empty`).

- [ ] **Step 1: Write the failing tests** (port of `AntiopePagesTests.swift` `SommairePageTests` and `GuidePageTests`, `AntiopeJournalTests.swift` `JournalPageTests`; the journal fixture `test_db::journal_view(read, unreachable, items)` = the Mac's `sampleEdition()` (edition 12 built 2026-10-16T07:12Z; items 1 front rank 1 Bibliobsession "A rare post", 2 week rank 2 Six Colors "Weekly one" starred, 3 week rank 3 Six Colors "Weekly two", 4 wire rank 4 Actualitté "Latest news"; overflow wire feed 10 Actualitté page 412, 12 hidden), zone `test_db::Paris`): `pages_run_down_the_left_column_then_the_right` (left `[100…109, 800]`, right `[801, 802, 899, 200, 201, 400, 401, 402, 403, 110, None]`), `every_row_fills_the_reader_width` (widths `[0, 1, 2, 9, 40, 41, 80]`), `reserved_pages_are_in_colour` (104 yellow with `" 43"`, 800 magenta + red count, 200 green, 201 white), `the_title_is_centred_and_the_wave_is_unbroken` (`"       RESSAC       "`; `" ▄▀▀▄    ▄▀▀▄   "` / `"▀    ▀▄▄▀    ▀▄▄"`; each column one block), `every_guide_line_fills_the_width` (count = Σ (2 + keys)), `every_key_of_the_window_is_listed` (`["0-9", "BACKSPACE", "ESC", "ALT S", "ALT G  ?", "ALT R", "I", "SPACE  J", "K", "M", "S", "LEFT RIGHT", "UP DOWN", "R", "B", "U", "O", "W", "SHIFT R", "CTRL F", "CTRL R  F5"]`; titles `["PAGES", "READING", "COLUMNS", "GARDEN", "MENUS"]`; `keys.len() + 1 ≤ 13`), `headings_keys_and_actions_are_coloured` (line 0 blank; `" PAGES"` black on green; `" 0-9         "` cyan, 13 cells; action white), `the_help_pages_draw_without_a_missing_glyph` (no replacement run in `guide_lines(60)`, the logo and the title), `sections_come_in_order_with_their_entries_and_overflow` (roles `[Header, Blank, Heading("front"), Entry(1), Blank, Heading("week"), Entry(2), Entry(3), Blank, Heading("wire"), Entry(4), Overflow { feed: 10, page: Some(412) }]` — `Heading` carries `Section::as_str`; header `"FRI 16 OCT"` cyan and `" 0/4 READ"` yellow; headings `"FRONT PAGE"` `" 1"`, `"THIS WEEK"` `" 2"`, `"THE WIRE"` `" 1"`; ids unique), `headings_are_coloured_and_blue_never_colours_text` (overflow white on blue), `entries_show_unread_and_starred_markers_and_the_selection`, `an_overflow_row_is_exactly_the_column_width` (`"  +3 MORE IN CAFÉ ? PÉDAGO  "`, a replacement run), `every_journal_row_is_exactly_the_width_asked` (widths `[0, 1, 2, 9, 28, 40]` × Idle, Building 34/156, Offline, Failed), `the_end_line_comes_once_every_entry_is_read` (double height, `"FIN DE L'EDITION"`, `" 4/4 READ"`), `empty_building_offline_and_unreachable` (`[Header, Blank, Message]` `"NOTHING NEW THIS MORNING."`; building `[Blank, Message]` `"EDITION IN PROGRESS 34/156"`; none + Due → `"EDITION IN PROGRESS"`; offline `"OFFLINE: NO EDITION YET"` black on red then `"RÉPÉTITION: RETRY"` cyan; failed `"DATABASE IS LOCKED."`; `"3 FEEDS UNREACHABLE"` red, `"1 FEED UNREACHABLE"`), `overflow_without_a_placed_entry_closes_its_section` (`last_entry_id == Some(4)`), `the_end_page_is_double_height` (`reader_page_lines(EndOfEdition, …)` one double-height line).
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui special 2>&1 | tail -3` → compile error.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui special 2>&1 | grep "test result"` → `ok.`
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/pages/mod.rs crates/ressac-ui/src/pages/special.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: SOMMAIRE, GUIDE (KDE keys), the end page and P104 as teletext rows

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 10 (B8c1): `pages::garden` (P800–P802, inspector page, rhythm and theme rows)

**Files:**
- Create: `crates/ressac-ui/src/pages/garden.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/pages/mod.rs` (`pub mod garden;`)

**Interfaces:**
- Consumes: Tasks 1, 2, 6; B5 `garden::{sections, progress, chip, unread_label, empty_message, Inspector, rhythm_text, outcome, RHYTHM_CHOICES}`, B2 `GardenScope::title`; core `GardenRow`, `Status::as_str`, `CadenceDetail`, `AttemptRecord`.
- Produces:

```rust
pub fn status_color(status_id: &str) -> Color;     // Dead red, Hijacked magenta, Moved cyan, Ailing yellow, Duplicate white, Fossil blue, Dormant/Alive green, else white
pub fn heading_style(status_id: &str) -> (Color, Color);   // blue → (White, Blue), else (colour, Black)
pub fn garden_page(rows: &[GardenRow], visible: &[&GardenRow], unread: &HashMap<FeedId, usize>, scope: GardenScope, selected: Option<FeedId>, cells: usize) -> Vec<Line>;
pub fn ordered(visible: &[&GardenRow], scope: GardenScope) -> Vec<FeedId>;   // status sections flattened
pub fn title_line(rows: &[GardenRow], scope: GardenScope, visible_count: usize, cells: usize) -> Line;
pub fn progress_line(progress: &Progress, cells: usize) -> Line;      // "▐" cyan, "█" green done, "░" blue left, " done/total" yellow
pub fn heading_line(section_id: &str, count: usize, cells: usize) -> Line;
pub fn feed_line(row: &GardenRow, unread: usize, selected: bool, cells: usize) -> Line;
pub fn inspector_page<Tz: TimeZone>(inspector: Option<&Inspector>, cadence: Option<&CadenceDetail>, theme_names: &[String],
    log: &[AttemptRecord], cells: usize, now: DateTime<Utc>, tz: &Tz) -> Vec<Line> where Tz::Offset: Display;
pub fn cadence_choices(cadence: &CadenceDetail, cells: usize) -> Vec<Line>;   // role Cadence(index into RHYTHM_CHOICES)
pub fn theme_choices(names: &[String], current: usize, cells: usize) -> Vec<Line>; // role Theme(index into theme_names)
pub fn wrap(text: &str, cells: usize) -> Vec<String>;
pub fn move_selection(selected: Option<FeedId>, ordered: &[FeedId], delta: i32) -> Option<FeedId>;
pub fn takes_arrows(column: AntiopeColumn) -> bool;
```

Rules: `GardenPage.swift` (inventory §5.4; binding), status order Antiope-only (no rhythm grouping), `left = needing − decided`, `done = decided`, `total = needing`. Inspector page order: title paragraph, folder path (cyan), blank, status heading (double height, `heading_style(inspector.status.as_str())`), `"Buried. Press k to restore it."` (yellow) when buried, reason; when `cadence` and not buried: blank, `RHYTHM` labelled `rhythm_text(cadence, now)`, then `cadence_choices`; when not buried (`inspector.theme` is `Some`): blank, `THEME` labelled with the current name, then `theme_choices(theme_names, index)`; EVIDENCE (green, `"- "`), REMARKS (yellow, `"- "`), blank, `LINKS` (cyan) with `PROPOSED` (not buried), `DECISION`, `FEED`, `SITE`, `WAYBACK` labelled (10 cells, cyan); REDIRECTS (magenta, `"<status> "` lead, from `log.first()`'s chain); blank, `FETCH LOG` (green), `"Not fetched yet."` or `"<LogDate>  <outcome>"` with the date cyan; no selection → blank, `"NO FEED SELECTED"` centred cyan. Labelled values wrap on words with following rows indented by the label.

- [ ] **Step 1: Write the failing tests** (port of `AntiopeGardenTests.swift` `GardenPageTests`, `GardenInspectorPageTests`, `AntiopeRhythmTests`, `bedsteadDrawsTheGardenGlyphs`; rows from `test_db::garden_row(id, title, status, …)` — B5's test helper moved to `test_db.rs` if private, not counted; inspectors from `Inspector::of`): `every_garden_line_is_exactly_the_column_width` (the Mac's five rows × three scopes × selections; 28 cells, `text().chars().count() == 28`, never double height), `feeds_are_grouped_by_status_in_urgency_order` (`ordered == [4, 5, 6, 2, 7, 3, 8, 1, 9]`), `headings_take_their_teletext_colour_and_fossil_is_reverse_video` (`" DEAD" + 22 spaces + "2"` red; `" FOSSIL" + 20 + "1"` white on blue; no blue foreground for any status), `the_title_row_counts_what_is_left_or_listed` (`"NEEDS TRIAGE" + 10 + "1 LEFT"`, last span `" 1 LEFT"` 7 cells yellow; `"ALL FEEDS … 186 FEEDS"`, `"CEMETERY … 1 FEED"`), `the_progress_row_is_exactly_the_column_width` (the Mac's table of `left/total` texts; `12/80` spans), `feed_rows_show_the_marker_the_title_and_the_chip` (`"● Zed" + 19 + "bury"`, chip cyan; decided `"  Old" + 19 + "kept"` green; selected black on white, same text), `buried_feed_rows_show_their_unread_count` (`"  Gone" + 12 + " 12 buried"`, `" 12"` yellow, `" buried"` green; none → `"  Gone" + 15 + " buried"`, 3 spans), `missing_glyphs_are_replaced` (`"● Café ?"` prefix, a replacement run), `empty_pages_show_one_centred_cyan_line` (`[Title, Progress, Blank, Text]`, `5 + "NOTHING TO TRIAGE" + 6`; `3 + "THE CEMETERY IS EMPTY" + 4`), `arrows_move_the_selection_and_stop_at_the_ends` (the Mac's eight cases, `takes_arrows`), `the_inspector_is_a_teletext_page` (the Mac's 23 trailing-trimmed lines verbatim for Unphotographable at 40 cells in UTC, `"FEED      "` lead cyan; `"301 https://www.unphotographable.com/ind"` then `"    ex.rdf"`; `"10 OCT 26 07:00  HTTP 410"`), `a_buried_fossil_shows_how_to_restore_it` (white on blue heading; `"Buried. Press k to restore it."`; no `PROPOSED`; `"DECISION  Bury"`; `"Not fetched yet."`; no THEME rows), `without_a_selection_the_reader_says_so` (`[40 spaces, 12 + "NO FEED SELECTED" + 12]`), `blue_never_colours_text_in_the_garden`, `wrap_breaks_on_words_and_splits_long_words` (the Mac's five cases), `the_inspector_shows_the_rhythm_under_the_status` (`"RHYTHM    " + rhythm_text`, then six `Cadence` rows; reason < rhythm < evidence; every width), `no_rhythm_for_a_buried_feed_or_without_cadence`, `the_override_rows_show_the_choice_in_force` (`[" AUTO", " DAILY", " WEEKLY", " MONTHLY", " RARE", " SILENT"]`; Auto reversed black on cyan unless overridden; forced Daily → only Daily; ids unique; actions `"cadence:<i>"`), `the_theme_rows_show_the_theme_in_force` (names `["None", "Tech", "Arts"]`, current 2 → `[" NONE", " TECH", " ARTS"]`, only `" ARTS"` reversed, actions `"theme:0"`…`"theme:2"`, placed after the rhythm rows and before EVIDENCE), `bedstead_draws_the_garden_glyphs` (`supports` `▐ █ ░ ●`).
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui pages::garden 2>&1 | tail -3` → compile error.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B8c1 ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/pages/mod.rs crates/ressac-ui/src/pages/garden.rs crates/ressac-ui/src/test_db.rs
git commit -m "ressac-ui: Antiope garden pages and inspector page (rhythm and Theme rows)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8c2: bridges for the pages

### Task 11 (B8c2): `Antiope` page rows, `Journal` progress, `Garden` Antiope rows

**Files:**
- Modify: `apps/linux/src/antiope.rs` (`journalRows`, `readerRows`, `bannerRow`; inputs `journalDone`, `journalTotal`, `journalFailure`, `windowCells` already there)
- Modify: `apps/linux/src/journal.rs` (`progressDone`, `progressTotal`, `failure`)
- Modify: `apps/linux/src/garden.rs` (`antiopeRows`, `antiopeMove`)
- Modify: `apps/linux/src/garden_inspector.rs` (`antiopeCells`, `antiopeInspector`)

**Interfaces:**
- Consumes: Tasks 9, 10; B4 `Journal`'s model; B5 `Garden`'s `GardenVisit`, log rows and themes.
- Produces: the B8c items of the header.

Behaviour: `Antiope.journalRows` = `encode_rows(journal_page(view, phase, selected, 28, &Local))`, the view read at `reload()` when the selection is `journal`; `Antiope.readerRows` = `encode_rows(reader_page_lines(nav.reader_page(), directory, readerCells))`, plus `no_entry_line` (role `Text`) on `Article` with no selected entry and not in the garden; `bannerRow(message, showLog, width)` = the three encoded lines of `banner_row` (`""` for a missing LOG). `Garden.antiopeRows` = `encode_rows(garden_page(snapshot.rows, visit.visible(), snapshot.unread, scope, selected, 28))` (empty outside a garden scope); `antiopeMove(delta)` = `move_selection(selected, ordered(visible, scope), delta)` then `select`; `antiopeInspector` = `encode_rows(inspector_page(visit.inspector(log, now, &Local).as_ref(), cadence, theme_names(themes), log, antiopeCells, Utc::now(), &Local))`.

- [ ] **Step 1: Write the failing check** — the walk test of Task 13 is written there; here, `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` after adding the `#[qproperty]` lines only → missing getters.
- [ ] **Step 2: Implement** the four files.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`, every `ok.` Then the full gate (B8c2 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/antiope.rs apps/linux/src/journal.rs apps/linux/src/garden.rs apps/linux/src/garden_inspector.rs
git commit -m "ressac-kde: P100, P899, P104, end page and garden rows for Antiope

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8c3: pages on screen, prompt, banner, Settings

### Task 12 (B8c3): `AntiopeView` page swaps, search prompt, banner row, Settings Appearance row

**Files:**
- Modify: `apps/linux/qml/AntiopeView.qml`, `apps/linux/qml/MinitelBar.qml`, `apps/linux/qml/CellList.qml`, `apps/linux/qml/ReadingPage.qml`, `apps/linux/qml/SettingsWindow.qml`

**Interfaces:**
- Consumes: Task 11; B5 `Garden.{key, select, selectFeed, setCadence, setTheme, undo, applySafe, confirming, applyQuestion, applyPreview, applyConfirmLabel}`; B6 `SettingsWindow.qml`'s Reading page.
- Produces: the complete Antiope page; `AntiopeView.{openPrompt, closePrompt, promptOpen}`.

Changes:
- **Entries column:** `SourcesModel.isGarden` → `CellList { objectName: "antiopeEntries"; rows: Garden.antiopeRows; Keys.onPressed: (e) => { e.accepted = Garden.key(e.text, e.modifiers) }; Keys.onUpPressed: Garden.antiopeMove(-1); Keys.onDownPressed: Garden.antiopeMove(1) }` (actions `feed:<id>` → `Garden.selectFeed(id)`, column entries); selection `journal` → `CellList { objectName: "antiopeEntries"; rows: Antiope.journalRows }` (`entry:<id>` → `EntriesModel.selectEntry(id)`, `Antiope.showArticle()`, column entries; `overflow:<feed>:<page>` → `apply(Antiope.jump(page))`, or without page `SourcesModel.selectKey("feed:" + feed)`, `Antiope.showArticle()`, column index); else the `EntriesModel` list. P104's list follows the selection with `showSelected()`.
- **Reader column:** garden → `CellList { objectName: "antiopeReader"; rows: Garden.antiopeInspector }` with `Garden.antiopeCells: Antiope.readerCells` (`cadence:<i>` → `Garden.setCadence(i)`, `theme:<i>` → `Garden.setTheme(i)`); `Antiope.readerPage !== "article"` or no entry → `CellList { objectName: "antiopeReader"; rows: Antiope.readerRows }` (`pair:<l>:<r>:<half>` → the left page when `x < half × cellWidth`, else the right; `jump:<n>`); else the `ReaderPane`, with `Reader.runFailureNotice` drawn as `CellRow` (black on red) above it when not empty.
- **Banner row:** under the header when `Runs.bannerVisible`: three `CellRow`s from `Antiope.bannerRow(Runs.bannerMessage, Runs.bannerShowLog, windowCells)`; LOG → `Qt.openUrlExternally("file://" + Runs.logDir)` (recette: `console.warn("walk: open log")`), X → `Runs.dismissBanner()`.
- **Garden extras:** `QQC2.Action { shortcut: StandardKey.Undo; enabled: SourcesModel.isGarden; onTriggered: Garden.undo() }`; a `Kirigami.PromptDialog` bound to `Garden.confirming` with B5's texts and buttons (`Garden.applySafe()`, Cancel → `Garden.confirming = false`).
- **Search prompt** (`MinitelBar.qml`): `property bool promptOpen`; open → `CellRow { line: Antiope.searchLabel(width) }` then a `QQC2.TextField { objectName: "antiopePrompt"; font.family: "Bedstead"; font.pixelSize: size; color: "#FFFF00"; placeholderText: "____"; placeholderTextColor: "#FFFF00"; background: Rectangle { color: "black" }; text: EntriesModel.searchText; onTextEdited: EntriesModel.searchText = text; onAccepted: closePrompt(false); Keys.onEscapePressed: closePrompt(true); onActiveFocusChanged: if (!activeFocus) closePrompt(false) }` focused when opened. `AntiopeView.openPrompt()` / `closePrompt(clearing)` (clearing → `EntriesModel.searchText = ""`; then `focusColumn()`). `ReadingPage.focusSearch()` → `antiopeView.openPrompt()` in Antiope. The view is destroyed on a look switch, so the prompt never survives it; the text does.
- **Settings** (`SettingsWindow.qml`, Reading page, first row): "Appearance:" two checkable `QQC2.ToolButton`s "Native" / "Antiope" in a `ButtonGroup`, checked by `AppSettings.effectiveAppearance`, `enabled: AppSettings.bedsteadAvailable`, writing `AppSettings.appearance`; under it, when `!AppSettings.bedsteadAvailable`, `"Antiope needs the Bedstead font, which did not load."` (small, disabled colour); the Font row `enabled: AppSettings.effectiveAppearance !== "antiope"` with `"Bedstead in Antiope"` under it in Antiope; the Text size row in Antiope: a 4-step `QQC2.Slider { from: 0; to: 3; stepSize: 1; snapMode: SnapAlways; value: [15, 20, 25, 30].indexOf(AppSettings.readerSizeAntiope); onMoved: AppSettings.readerSizeAntiope = [15, 20, 25, 30][value] }` and `"<size> pt"`.

- [ ] **Step 1: Write the failing check** — none at this step (the walk comes with Task 13); `qmllint` of the five files must stay free of syntax errors.
- [ ] **Step 2: Implement** (Step 0 for `ReadingPage.qml` if it grew over 300 lines since Task 8b).
- [ ] **Step 3: Verify** — every suite (`cargo test --locked -p ressac-kde`) → every `ok.` (`antiope`, `settings` walks of B6 unchanged except the new row: update B6's `26-settings` expectation only if it compares the whole page, never its texts). Then the full gate (B8c3 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/qml/AntiopeView.qml apps/linux/qml/MinitelBar.qml apps/linux/qml/CellList.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/SettingsWindow.qml
git commit -m "ressac-kde: Antiope pages on screen (SOMMAIRE, GUIDE, P104, end page, garden and inspector), search prompt, banner, Appearance setting

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8c4: the pages walk

### Task 13 (B8c4): walk `antiope-pages`

**Files:**
- Modify: `apps/linux/qml/Main.qml` (`antiopePagesSteps`, chooser `"antiope-pages"`)
- Test (not counted): `apps/linux/tests/antiope_pages.rs`

**Interfaces:**
- Consumes: Task 12.
- Produces: the walk B10's complete walk reuses.

Steps (each logs one `walk:` line; the copy is prepared by the test: an edition of entries 1 and 3 with an overflow row, feed 2 unhealthy so P800 lists it; feeds' URLs at `127.0.0.<id>:9`, no network):

| Step | Action | Log |
|---|---|---|
| `c01-sommaire` | `appearance = "antiope"`, Alt+S | `reader=index rows=<n> first=RESSAC` |
| `c02-guide` | `?` | `reader=guide` |
| `c03-journal` | dial `104` | `selected=journal rows=<journalRows.length> end=false` |
| `c04-suite-end` | select the last edition entry, `j` | `reader=end read=true` |
| `c05-retour` | `k` | `reader=article` |
| `c06-garden` | dial `800`, `→` | `selected=garden:needs-triage column=entries inspector=<antiopeInspector.length>` |
| `c06b-garden-key` | `k` | `message=` + Garden.message (the B5 keep path) |
| `c07-prompt-digits` | Ctrl+F (menu action), type `412` in the prompt, Return | `search=412 page=<page unchanged> prompt=false` |
| `c08-switch-with-prompt` | Ctrl+F, type `x`, `appearance = "native"` | `look=native search=412x focus=sidebar` |
| `c09-banner` | `Runs.showBanner("The database is busy (is `ressac` running in a terminal?)", true)`, Antiope | `banner=true` |
| `c10-settings` | open Settings | `appearance=antiope bedstead=true` |

- [ ] **Step 1: Write the failing test** `apps/linux/tests/antiope_pages.rs`: prepare the copy (rusqlite: `INSERT INTO edition …`, `edition_entry` rows, `edition_overflow` row, a `feed_health` row as B5's tests do), launch with `RESSAC_RECETTE_WALK=antiope-pages`, assert each line above with the fixture's real values, `assert_clean`, and on `c01-sommaire.png` the double-height title: a yellow run 2 rows tall in the reader column's first two rows; on `c06-garden.png` a red or yellow heading row in the entries column; `c03-journal.png` cyan pixels in the header row of the entries column.
- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test antiope_pages 2>&1 | grep -E "panicked|test result"'` → FAIL.
- [ ] **Step 3: Implement** the steps.
- [ ] **Step 4: Verify and look** — the same command → `ok.`; every suite → `ok.`. Open `c01-sommaire.png`, `c02-guide.png`, `c03-journal.png`, `c04-suite-end.png`, `c06-garden.png`, `c09-banner.png`, `c10-settings.png`. Then the full gate (B8c4 and master B8c end here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/Main.qml apps/linux/tests/antiope_pages.rs
git commit -m "ressac-kde: Antiope pages walk (SOMMAIRE, GUIDE, P104 to the end page, garden, prompt, banner, Settings)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8d1: mosaics (`ressac-ui`)

### Task 14 (B8d1): `ressac_ui::mosaic`

**Files:**
- Modify: `crates/ressac-ui/Cargo.toml` (`image.workspace = true`; dev `wiremock.workspace = true`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod mosaic;`)
- Create: `crates/ressac-ui/src/mosaic.rs` (tests in its `mod tests`)

**Interfaces:**
- Consumes: Task 1 (`Color`), B3 `sanitize::is_web_url`; core `Ressac::fetch_image`, `Error::ImageRefused`.
- Produces:

```rust
pub const SCHEME: &str = "ressac-mosaic";
pub const CELL_WIDTH: usize = 6;  pub const CELL_HEIGHT: usize = 10;
pub const COLUMN_EDGES: [usize; 3] = [0, 3, 6];  pub const ROW_EDGES: [usize; 4] = [0, 3, 7, 10];
pub const MAX_COLUMNS: usize = 40;  pub const MAX_ROWS: usize = 100;  pub const PIXELS_PER_COLUMN: f64 = 12.0;
pub const MAX_SOURCE_PIXELS: u64 = 40_000_000;  pub const CACHE_BYTES: usize = 16 << 20;  pub const MAX_IN_FLIGHT: usize = 4;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct MosaicCell { pub fg: Color, pub bg: Color, pub mask: u8 }  // bit 0 top-left … 5 bottom-right
#[derive(Debug, Clone, PartialEq, Eq)] pub struct MosaicImage { pub columns: usize, pub rows: usize, pub cells: Vec<MosaicCell> }
pub fn columns_for_width(width: u32, max_columns: usize) -> usize;   // ceil(width / 12), 1…max
pub fn grid(width: u32, height: u32, columns: usize) -> (usize, usize);
pub fn cell_for(samples: &[[f64; 3]; 6]) -> MosaicCell;
pub fn mosaic_from_rgba(image: &RgbaImage, columns: usize) -> MosaicImage;
pub fn png(mosaic: &MosaicImage, scale: usize) -> Vec<u8>;            // empty for an inconsistent mosaic
pub fn failure_png() -> Vec<u8>;                                     // one black cell, 6 × 10
#[derive(Debug, Clone, PartialEq, Eq)] pub struct MosaicRequest { pub original: String, pub columns: usize }
impl MosaicRequest { pub fn url_for(original: &str, columns: usize) -> String; pub fn parse(url: &str) -> Option<MosaicRequest>; pub fn cache_key(&self) -> String; }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum MosaicFailure { Refused, Status(u16), TooLarge, Unreadable, Network }
impl MosaicFailure { pub fn is_transient(self) -> bool; pub fn of(error: &ressac_core::Error) -> MosaicFailure; }
pub fn decode(bytes: &[u8], max_columns: usize) -> Result<(RgbaImage, usize), MosaicFailure>;   // the thumbnail and its columns
pub fn render(bytes: &[u8], max_columns: usize) -> Result<Vec<u8>, MosaicFailure>;             // decode → mosaic → png
pub async fn fetch(app: &Ressac, url: &str) -> Result<Vec<u8>, MosaicFailure>;
#[derive(Debug, Default)] pub struct MosaicCache { /* LRU by cost */ }
impl MosaicCache { pub fn get(&mut self, key: &str) -> Option<Result<Vec<u8>, MosaicFailure>>; pub fn put(&mut self, key: &str, result: Result<Vec<u8>, MosaicFailure>); pub fn cost(&self) -> usize; }
```

The renderer (undetermined; port of `MosaicRenderer.swift`, inventory §5.7):

```text
grid(w, h, cols): aspect = max(h,1)/max(w,1); rows(c) = max(1, round(0.6·c·aspect));
  cols = max(cols,1); if rows(cols) > MAX_ROWS { cols = max(1, floor(MAX_ROWS / (0.6·aspect))) }; (cols, min(rows(cols), MAX_ROWS))
mosaic_from_rgba(img, cols):
  (C, R) = grid(img.w, img.h, cols); W = 6C, H = 10R
  px = img composited over black (r·a/255, g·a/255, b·a/255) then resized to W × H (FilterType::Triangle;
       a source more than twice W or H is first reduced with `thumbnail(2W, 2H)`)
  for each cell (col, row), for bit i in 0..6: sub-column i % 2 → x in [6col + COLUMN_EDGES[i%2], 6col + COLUMN_EDGES[i%2+1]),
      sub-row i / 2 → y in [10row + ROW_EDGES[i/2], 10row + ROW_EDGES[i/2+1]); sample[i] = mean RGB over that block (f64)
  cell = cell_for(samples)
cell_for(s): d[i][c] = Σ_k (s[i][k] − PALETTE[c][k])²  for the 8 colours in enum order (black … white)
  best = (a = 0, b = 0, err = +∞); for a in 0..8 { for b in a..8 { err = Σ_i min(d[i][a], d[i][b]); if err < best.err { best = (a, b, err) } } }
  mask_b = Σ 1<<i over i where d[i][b] < d[i][a];  n = popcount(mask_b)
  n = 0 → (a, a, 0);  n = 6 → (b, b, 0);  n ∈ {4, 5} → fg a, bg b, mask = !mask_b & 0x3F;  else (1…3, a 3–3 tie included) → fg b, bg a, mask_b
png(m, scale): W = 6·C·scale, H = 10·R·scale; pixel (x, y): fx = x/scale, fy = y/scale; cell = cells[(fy/10)·C + fx/6];
  sub-row = 0 if fy%10 < 3, 1 if < 7, else 2; sub-col = 0 if fx%6 < 3 else 1; bit = 2·sub-row + sub-col;
  colour = (mask >> bit) & 1 ? fg : bg; encoded as RGB8 PNG (image::codecs::png::PngEncoder)
```

Requests: `url_for` = `"ressac-mosaic:image?cols=<n>&src=<original>"` with every byte outside `A–Z a–z 0–9 - . _ ~` percent-encoded (upper-case hex); `parse`: scheme `ressac-mosaic` (case-insensitive), path `image`, `cols` 1…40, `src` percent-decoded and `is_web_url`. Decode: `ImageReader::new(Cursor).with_guessed_format()` (no format → `Unreadable`), `into_dimensions()` on a second reader (`w·h > MAX_SOURCE_PIXELS` → `TooLarge`, before decoding), limits `max_alloc = 128 MB`, `decode()` (a GIF's first frame) errors → `Unreadable`; `columns = columns_for_width(w, max_columns)`. `MosaicFailure::of`: `ImageRefused` texts `"not a web address"`, `"local or private address"` → `Refused`; `"larger than 10 MB"`, `"response too large"` → `TooLarge`; `"not an image"` → `Unreadable`; `"HTTP <code>"` → `Status(code)`; anything else → `Network`. Transient: `Network`, `Status(429)`, `Status(500..=599)`. Cache: cost = PNG length (64 for a failure), evict least recently used past `CACHE_BYTES`; `put` ignores transient failures.

- [ ] **Step 1: Write the failing tests** (port of `MosaicTests.swift` renderer, request, decoder suites, with `image` building the fixtures; fetch classes against `wiremock` through `Ressac::open(test_db).allow_private_articles()`): `a_flat_colour_gives_uniform_cells_in_canonical_form` (60 × 100 red, 10 columns → 10 × 10 uniform red), `a_half_black_half_white_cell_takes_black_as_background` (masks `0b01_0101`, `0b00_0011`, and `fg Black / bg White, 0b11_0000`), `colours_quantise_to_the_nearest_teletext_colour` (`(30,220,235)` cyan, `(200,30,40)` red, `(20,20,20)` black; `cell_for` of six `(250,240,20)` → uniform yellow), `transparency_reads_as_the_black_paper`, `width_and_height_stay_within_the_limits` (1000 × 500 → 40 × 12; `columns_for_width(100, 40) == 9`, `(5000) == 40`, `(1) == 1`, `(480, 20) == 20`; `grid(100, 10_000, 40)` rows ≤ 100, columns 1), `the_png_has_one_pixel_per_font_pixel_with_sextant_rows_of_three_four_three` (the Mac's pixel checks; scale 2 → 24 × 20; inconsistent → empty), `the_original_survives_the_round_trip` (`"https://img.example.org/a b/photo.jpg?w=800&h=600#x"` → prefix `"ressac-mosaic:image?cols=40&src=https%3A%2F%2Fimg.example.org"`, parse back equal), `other_schemes_and_bad_columns_are_refused` (`file:`, `javascript:`, `data:`, `ftp:`, `https:///nohost`; cols 0, 41, missing; a plain `https:` URL), `a_gif_uses_its_first_frame` (red frame then blue → red cells), `html_empty_and_svg_bodies_are_unreadable`, `too_many_pixels_are_refused_before_decoding` (an 8000 × 8000 PNG header with a tiny body → `TooLarge` and no allocation: assert it returns within 50 ms), `an_image_becomes_a_mosaic_png` (120 × 120 red PNG → `render` → a 60-pixel-wide PNG), `fetch_failures_keep_their_class` (wiremock: 404 → `Status(404)` not transient; 503 → transient; `text/html` → `Unreadable`; 11 MB `image/png` → `TooLarge`; a PNG → `Ok`), `transient_failures_are_never_cached` (`put(Network)` then `get` → `None`; `put(Unreadable)` then `get` → `Some(Err(Unreadable))`; a 16 MB budget evicts the oldest), `the_failure_placeholder_is_one_black_cell`.
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui mosaic 2>&1 | tail -3` → compile error.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui mosaic 2>&1 | grep "test result"` → `ok.`
- [ ] **Step 5: Commit**

```bash
git add Cargo.lock crates/ressac-ui/Cargo.toml crates/ressac-ui/src/lib.rs crates/ressac-ui/src/mosaic.rs
git commit -m "ressac-ui: SAA5050 mosaics (renderer, PNG, requests, guarded decoding, cache, fetch failure classes)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 15 (B8d1): mosaic boxes in the sanitiser, image scripts

**Files:**
- Modify: `crates/ressac-ui/src/sanitize/mod.rs` (`Images::Mosaics`)
- Modify: `crates/ressac-ui/src/article.rs` (Antiope pages sanitise with `Images::Mosaics`; `toggle_image_script`, `toggle_all_images_script`, `mark_failed_script`)

**Interfaces:**
- Consumes: Task 14 (`MosaicRequest::url_for`, `grid`, `columns_for_width`, `MAX_COLUMNS`, `CELL_HEIGHT`), B3 `declared_size`, `js_string`.
- Produces: `Images::Mosaics`; `article::{toggle_image_script(index: u32) -> String, toggle_all_images_script() -> String, mark_failed_script(source: &str) -> String}` (returns of the scripts: the new `original` state, the number of boxes, nothing).

Rules (`ArticleTemplate.swift:410-450`): with `Images::Mosaics`, an `img` whose resolved `src` is http(s) becomes, numbered from 0 in document order, `<a class="mosaic" href="ressac-toggle:<n>" id="m<n>" data-original="<url>" data-mosaic="<mosaic url>"[ style="min-height: calc(<rows×10>px * var(--font-pixel))"]><img src="<mosaic url>" alt="<alt>" loading="lazy"><span class="mosaic-alt">IMAGE[ <alt>]</span></a>`, a `<span class="mosaic" …>` (no `href`) inside a link; the style only when `declared_size` gives both sides (rows from `grid(w, h, columns_for_width(w, 40))`); no `width`/`height` attributes on the box's `img`; `data:` images unchanged. The memo key already includes `images`. The scripts are the Mac's (`ReaderView.swift:237-270`, `MosaicSchemeHandler.swift:330-340`) with `js_string` values.

- [ ] **Step 1: Write the failing tests** (port of `MosaicMarkupTests`): `images_become_numbered_mosaic_boxes` (the Mac's two exact boxes, `&` written `&amp;` in attributes, the `data:` image kept, no `width=`; the same input with `Images::Web` keeps `width="800"`), `an_image_inside_a_link_keeps_the_link` (prefix `<a href="https://example.org/big.jpg"><span class="mosaic" id="m0" data-original="https://example.org/small.jpg"`, no `ressac-toggle`), `a_declared_size_reserves_the_mosaics_rows` (`width="120" height="120"` → `min-height: calc(100px * var(--font-pixel))`: 10 columns, 10 rows), `antiope_pages_draw_mosaics_and_native_pages_do_not` (an `ArticlePage` in Antiope contains `class="mosaic"`, in Native `<img src="https://`), `the_image_scripts_are_app_world_and_escaped` (`toggle_image_script(3)` contains `getElementById("m" + 3)`; `mark_failed_script("https://x/\"a")` contains `js_string` of it and `dataset.failed`).
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui mosaic_boxes 2>&1 | tail -3` → compile error (`Mosaics`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B8d1 ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/sanitize/mod.rs crates/ressac-ui/src/article.rs
git commit -m "ressac-ui: mosaic boxes in Antiope articles, image swap and failure scripts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8d2: the `ressac-mosaic:` scheme

### Task 16 (B8d2): `Mosaics`, the C++ handler, `Reader` scripts

**Files:**
- Create: `apps/linux/src/mosaic.rs`
- Modify: `apps/linux/src/lib.rs` (`mod mosaic;`; shim `ressac_mosaic_reply`; `extern "Rust" { fn ressac_mosaic_start(token: u64, url: &str); }` with its body `use`d from `mosaic.rs`)
- Modify: `apps/linux/cpp/shim.h`, `apps/linux/cpp/shim.cpp`
- Modify: `apps/linux/src/reader.rs` (`toggleImageScript`, `toggleAllScript`, `markFailedScript`)

**Interfaces:**
- Consumes: Tasks 14, 15; B3's scheme registration (`ressac_init_webengine`, `ressac_create_application`), `with_session` (`worker.db()`), `Recette.active`.
- Produces: the scheme, `Mosaics`, `Reader` invokables of the header.

Behaviour:
- **C++:** in `ressac_init_webengine()`, register `QWebEngineUrlScheme("ressac-mosaic")` with `Syntax::Path` (before `QtWebEngineQuick::initialize()`, as B3's); in `ressac_create_application()`, install a `MosaicHandler` on the default profile. `MosaicHandler::requestStarted(job)`: `token = ++next`; `jobs[token] = QPointer(job)`; `ressac_mosaic_start(token, job->requestUrl().toString().toStdString())`. `ressac_mosaic_reply(token, mime, body)` (GUI thread only): take the entry; a null `QPointer` (the page cancelled the request) → drop silently; else `job->setAdditionalResponseHeaders({{"Cache-Control", "no-store"}})`, a `QBuffer` parented to the job holding the bytes, `job->reply(mime, buffer)`. If an `<img>` from the `ressac:` page cannot load the scheme in Task 17's walk, add `QWebEngineUrlScheme::CorsEnabled` and say so in the ledger.
- **Rust `Mosaics`** (`#[qml_singleton]`, `impl Threading`): on `Initialize`, store its `CxxQtThread` and a `Sender<Job>` in a GUI-thread `thread_local!`, and start the mosaic thread: it opens `Ressac::open(db)` (`.allow_private_articles()` only when `Recette` is active and `RESSAC_ALLOW_PRIVATE_ARTICLES=1`), a current-thread tokio runtime, and runs jobs with at most `MAX_IN_FLIGHT` fetches at once (`FuturesUnordered`), each `fetch` then `render`; results go back by `qt_thread.queue(move |m| { ressac_mosaic_reply(token, "image/png", &body); if failed { m.failed(url) } })` after `cache.put`. `ressac_mosaic_start`: unparsable → reply `failure_png()` at once and emit `failed(url)`; cached → reply at once (`failed` for a cached failure); no `Mosaics` yet → reply the failure and `tracing::warn!`; else send the job. The cache is an `Arc<Mutex<MosaicCache>>` shared by both threads.

- [ ] **Step 1: Write the failing test** `apps/linux/tests/mosaic.rs` (scheme level; the walk comes in Task 17): `fn a_cancelled_request_is_never_answered()` is covered by the walk's log (`mosaic: dropped <token>` printed at debug level when a `QPointer` is null; Task 17 reloads a page while a slow image is pending and asserts the line and no crash). Here: `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` after the `lib.rs` declarations only → undefined `ressac_mosaic_reply`.
- [ ] **Step 2: Implement** the five files.
- [ ] **Step 3: Verify** — clippy `Finished`, every suite `ok.`; then the full gate (B8d2 ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/src/mosaic.rs apps/linux/src/lib.rs apps/linux/cpp/shim.h apps/linux/cpp/shim.cpp apps/linux/src/reader.rs
git commit -m "ressac-kde: ressac-mosaic: scheme (fetch threads through the core, cache, failure placeholder)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

## B8d3: mosaics on screen

### Task 17 (B8d3): toggles, failed marks, the mosaic walk

**Files:**
- Modify: `apps/linux/qml/ReaderPane.qml` (a `toggle:<n>` decision runs `Reader.toggleImageScript(n)`; `Connections { target: Mosaics; function onFailed(url) { pane.runAppScript(Reader.markFailedScript(url), function () {}) } }` while the look is Antiope)
- Modify: `apps/linux/qml/AntiopeView.qml` (effect `images` → `reader.runAppScript(Reader.toggleAllScript(), …)`)
- Modify: `apps/linux/qml/Main.qml` (`mosaicSteps`, chooser `"mosaic"`)
- Test (not counted): `apps/linux/tests/mosaic.rs`

**Interfaces:**
- Consumes: Tasks 15, 16; B3's `onNavigationRequested` (`toggle:` answer, left for B8d).
- Produces: walk `mosaic`.

Walk (the test serves `/photo.png` (120 × 120 red), `/broken.png` (`text/html`), `/slow.png` (answers after 30 s) from a `std::net::TcpListener` thread on `127.0.0.1`, and writes an entry whose content is `<p><img src="http://127.0.0.1:<port>/photo.png" alt="Photo"></p><p><img src="http://127.0.0.1:<port>/broken.png" alt="Gone"></p>`; the run has `RESSAC_ALLOW_PRIVATE_ARTICLES=1`): `d01-mosaic` (Antiope, select it; wait), `d02-failed` (log `walk: d02 m0=<naturalWidth> m1failed=<bool> alt=<display of .mosaic-alt in m1>` through `ReaderPane.runAppScript`), `d03-click` (click the first box via `reader.clickSelector("#m0")`, a recette hook in `ReaderPane.qml`: `document.getElementById("m0").click()` in the app world; log `src=` and `original=`), `d04-i` (`Recette.key(Qt.Key_I, "i", 0)`; log the count of `.mosaic.original`), `d05-i-again`, `d06-slow-reload` (an entry with `/slow.png`, then another entry at once; log `Mosaics.inFlight`).

- [ ] **Step 1: Write the failing test** — expected lines: `walk: d02 m0=60 m1failed=true alt=block`, `walk: d03 src=http://127.0.0.1:<port>/photo.png original=true`, `walk: d04 originals=2`, `walk: d05 originals=0`, and the run exits 0 with no crash after `d06`; `d01-mosaic.png` holds a red block made of whole cells (red pixels in a rectangle 60 × 60 font pixels × `--font-pixel`).
- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test mosaic 2>&1 | grep -E "panicked|test result"'` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify and look** — `ok.`; every suite `ok.`; open `d01-mosaic.png` (the red mosaic, then the magenta `IMAGE Gone` line), `d03-click.png` (the original photo). Then the full gate (B8d3 and master B8d end here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/ReaderPane.qml apps/linux/qml/AntiopeView.qml apps/linux/qml/Main.qml apps/linux/tests/mosaic.rs
git commit -m "ressac-kde: mosaics in the Antiope reader (click and i swap, failed images as magenta lines)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B8e: VM checkpoint 2 (spec §8.5) — stops for Clément

### Task 18 (B8e): Antiope on marghe's VM, frame times, the B6/B7 gestures

**Files:**
- Modify: `apps/linux/dev/marghe.sh`
- Modify: `notes.md` (« ### Point d'étape VM 2 (B8, <date>) »)
- Modify (only if something is deferred): `TODO.md`

**Interfaces:**
- Consumes: B3h's `marghe.sh` (`check`, `push`, `install`, `dnf`, `shots`, `manual`), walks `antiope`, `antiope-scroll`, `antiope-pages`, `grid`, `mosaic`; the ledger's B8e checklist (B6e and B7c added drags, portal dialogs, Ctrl+N, Sea Chart items).
- Produces: `marghe.sh` options reused by B9 and B10.

`marghe.sh` additions:
- `shots <label> … [--look antiope] [--walk NAME] [--big N] [--env K=V]…`: `--look antiope` writes `appearance=antiope` into the saved-and-restored settings file (as `--font`); `--walk` sets `RESSAC_RECETTE_WALK`; `--env` passes extra variables (`QT_QPA_UPDATE_IDLE_TIME=0`, `RESSAC_RECETTE_GRID=painted`); `--big N` builds the database on the Mac (`cp tests/fixtures/contract.sqlite "$tmp/big.sqlite"; sqlite3 "$tmp/big.sqlite" "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < $N) INSERT INTO entry (feed_id, guid, url, title, published_at, fetched_at, summary_html) SELECT 1, 'big-' || i, 'https://example.org/big/' || i, 'Entry ' || i || ' of a long folder, with a title that wraps over two rows', strftime('%Y-%m-%dT%H:%M:%fZ', '2026-10-01', '-' || i || ' minutes'), '2026-10-01T00:00:00.000Z', '<p>Body ' || i || '</p>' FROM n;"`) and uploads it to `/tmp/ressac-vm/<label>/r.sqlite` by the same transfer path as `push` (stdin, else base64 chunks), instead of copying the fixture on the VM.
- `manual [--look antiope] [--big N] [--frame-log]`: as B3h's, with `RESSAC_FRAME_LOG=1` when `--frame-log`, stderr to `/tmp/ressac-vm/manual/run.log`; `manual stop` also prints the `ressac-kde: frames` lines of that log.

Under the hood every VM command is `ssh willow@marghe.local "prlctl exec 'Fedora Linux' …"`; a session command is `runuser -u willow -- env HOME=<home> USER=willow LOGNAME=willow XDG_RUNTIME_DIR=… WAYLAND_DISPLAY=… DBUS_SESSION_BUS_ADDRESS=… XDG_SESSION_TYPE=… XDG_CURRENT_DESKTOP=… QT_QPA_PLATFORMTHEME=… KDE_FULL_SESSION=… DISPLAY=… <command>`, the variables read from `/proc/$(pgrep -u willow -x plasmashell)/environ` (B3h).

- [ ] **Step 1: Gate and clean tree.** Full gate green on `HEAD`; `git status --porcelain` empty.
- [ ] **Step 2: Extend `marghe.sh`.** Check: `bash -n apps/linux/dev/marghe.sh`; `shellcheck` when present; `apps/linux/dev/marghe.sh` without arguments → usage, exit 2.
- [ ] **Step 3: Reach, push, install** — `apps/linux/dev/marghe.sh check` (`Fedora release 44`, `aarch64`, `plasma session: yes|no`), `apps/linux/dev/marghe.sh push` (`pushed <sha> (Cargo.lock matches)`), `apps/linux/dev/marghe.sh install` (keep `--software-rendering` if checkpoint 1 imposed it). No new Fedora package is needed by B8 (same Qt modules); if `install.sh` still asks for one, stop and ask as in B3h Step 6.
- [ ] **Step 4: ⏸ STOP — ask Clément** when `check` says `plasma session: no` (exact message): « Point d'étape VM 2 (Antiope) : l'appli est installée sur la VM (commit `<sha>`). Peux-tu ouvrir ta session Plasma sur la VM ? Je lance ensuite les captures d'Antiope et une mesure du défilement. » Wait for his answer, then `check` again.
- [ ] **Step 5: Screenshots and frame times in his session** (virgl first):

```bash
apps/linux/dev/marghe.sh shots antiope-gpu --look antiope --walk antiope
apps/linux/dev/marghe.sh shots pages-gpu --look antiope --walk antiope-pages
apps/linux/dev/marghe.sh shots grid-gpu --walk grid --env QT_QPA_UPDATE_IDLE_TIME=0
apps/linux/dev/marghe.sh shots scroll-gpu --look antiope --walk antiope-scroll --big 2000
apps/linux/dev/marghe.sh shots grid-sw --walk grid --software-rendering --env QT_QPA_UPDATE_IDLE_TIME=0
apps/linux/dev/marghe.sh shots scroll-sw --look antiope --walk antiope-scroll --big 2000 --software-rendering
```

  Expected: `exit=0` each; the walk lines of `tests/antiope.rs` and `tests/antiope_pages.rs`; `grid:` and `frames: antiope step=1|10` lines (record all of them); PNGs at device pixel ratio 2. Look at `a01-antiope.png`, `a05-sommaire.png`, `c03-journal.png`, `c06-garden.png`, `g01-pattern.png` with the Read tool: crisp Bedstead at scale 2 (cells of 24 device pixels at 20 px), the 8 colours only, straight separators, the article text in the reader column in Bedstead (Chromium may antialias the web font: say so if its edges are grey). The `mosaic` walk needs the loopback server of its test and is not run on the VM; mosaics are checked by hand in Step 6 on real feeds.
- [ ] **Step 6: Clément's hands** — `apps/linux/dev/marghe.sh manual --look antiope --big 2000 --frame-log`, then ask (exact message, followed by the B8e checklist items the ledger holds from B6e and B7c, copied as a numbered list): « Antiope est ouvert dans ta session sur une copie de la base de test avec 2 000 articles dans xkcd. Peux-tu : taper 201 puis 437 (NOT FOUND deux secondes), Alt+S, Alt+G, ?, les flèches gauche et droite entre les colonnes, haut et bas dans l'index, Espace et j dans un article, k ; faire défiler la colonne des articles de xkcd à la molette et au clavier (est-ce fluide ?) ; ouvrir un article de xkcd et taper i, puis cliquer une image ; Ctrl+F, taper un mot, Entrée ; puis la liste ci-dessous (glisser-déposer, dialogues, carte marine). Dis-moi ce qui ne va pas. » Record his answers; `apps/linux/dev/marghe.sh manual stop` (prints the frame-log lines of his session: record them).
- [ ] **Step 7: Software rendering, only if Step 5 or 6 shows a blank, black or flickering view, or a p95 over 16.7 ms with virgl** — compare with the `-sw` runs; if software is better, `apps/linux/dev/marghe.sh install --software-rendering` and say so.
- [ ] **Step 8: Verdict in `notes.md`** (French, no em dash in prose), « ### Point d'étape VM 2 (B8, <date>) »: commit tested; rendering (virgl or software, why); the grid at scale 2 (exact cells, colours, Bedstead in the reader); frame times on the VM (the `grid:` and `frames:` lines, and Clément's own session log) beside the container's; keys (walk, then Clément's); mosaics on real feeds; the B6/B7 gestures one by one; anomalies. Each anomaly not fixed now gets a `TODO.md` line under « ### Linux (sous-projet 8) ».
- [ ] **Step 9: Commit**

```bash
git add apps/linux/dev/marghe.sh notes.md TODO.md
git commit -m "Linux: VM checkpoint 2 on marghe (Antiope at scale 2, frame times, drags and dialogs by hand)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

  (`TODO.md` only when Step 8 added a line.)
- [ ] **Step 10: ⏸ STOP for Clément.** Report the verdict (rendering, grid and frame times on the VM, keys, mosaics, the gestures, anything deferred) with the paths of the pulled screenshots, and wait. **B9 does not start before Clément has read the verdict.** Nothing is pushed.
