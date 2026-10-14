# Phase 11: Mosaics (11a: renderer and scheme handler; 11b: the reader shows and swaps them)

**Runs after:** phase 10b (11a, then 11b).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`); its Global Constraints apply to every task, in particular the mosaic limits (http/https only, `10 * 1024 * 1024` bytes, 15 s, 40 cells, 2 × 3 sub-pixels). Spec: §7 (Mosaics: loading, rendering, swapping, cache and failures), §11 (scheme handler and renderer tests), §13 (second download accepted). Phase 10 (both halves) must be merged first: this phase builds on `ReaderTheme`, the Antiope page and `reader.css`'s Antiope section.

**Goal:** In Antiope, every web image of an article appears as a two-colour teletext block mosaic; a click on it, or `i` for the whole article, swaps in the original and back; an image that cannot be fetched or decoded shows a magenta `IMAGE` line with its alt text.

**Architecture:** A pure `MosaicRenderer` turns a `CGImage` into cells (best pair of teletext colours per 2 × 3 cell) and a PNG at one pixel per font pixel. A `MosaicSchemeHandler` serves `ressac-mosaic:` addresses to the reader's `WKWebView`: it downloads the original with a streaming byte cap and a 15 s timeout, decodes a thumbnail with ImageIO and renders the mosaic off the main actor, caches the PNG, and never calls back a task WebKit has stopped. The sanitiser, in Antiope, wraps each http(s) image in a numbered box linking to `ressac-toggle:<n>`; the navigation delegate catches that link and the controller swaps the image's `src` through app JavaScript (page JavaScript stays off).

**Tech Stack:** Swift 6.4 (Swift 6 mode, complete strict concurrency, warnings as errors), WebKit (`WKURLSchemeHandler`, `callAsyncJavaScript`), ImageIO, Core Graphics, `URLSession`, `Synchronization.Mutex`, Swift Testing. No new dependency.

**Spec:** `docs/specs/2026-10-14-antiope-design.md`.

## This phase is split in two

Six files in all (`MosaicRenderer.swift`, `MosaicSchemeHandler.swift`, `MosaicTests.swift`, `ArticleTemplate.swift`, `ReaderView.swift`, `reader.css`), so two phases, each with its gate and Clément's approval. The index table gets rows **11a** and **11b** in place of row 11 (a plan change, reported with this file).

| Phase | Files (5 at most, tests included) | Deliverable |
|---|---|---|
| 11a | Create `Teletext/MosaicRenderer.swift`, `Reading/MosaicSchemeHandler.swift`, `RessacTests/MosaicTests.swift` | The renderer; mosaic addresses, the capped fetcher, the decoder, the scheme handler; Review Focus #4 |
| 11b | `Reading/ArticleTemplate.swift`, `Resources/reader.css`, `Reading/ReaderView.swift`, `RessacTests/MosaicTests.swift` | Mosaic boxes in the sanitiser, CSP and font pixel; the mosaic stylesheet; handler registration, click and `i` swaps; an end-to-end web view test |

**Where the files go.** The spec puts `MosaicRenderer` in the `Teletext/` module (§5): it is pure teletext geometry with no WebKit, next to `TeletextColor`. The handler is reader plumbing and goes to `Reading/`, next to `ArticleTemplate` (there is no `Reader/` folder in the app).

## Interfaces

**Consumes:**
- Phase 3: `TeletextColor` (`allCases` in declaration order black, red, green, yellow, blue, magenta, cyan, white; `rgb`).
- Phase 7: `ReadingCommand.toggleImages` (the key `i`, Antiope only) and **`AntiopeState.imageToggle: (@MainActor () -> Void)?`**, the hook phase 7 added for this phase: its `handle(.toggleImages)` calls it when an article is selected in the reader page and returns `false` otherwise, and its `AntiopeStateTests.imagesToggleOnlyOnAnArticleWithAReader` pins that. The index table does not list `imageToggle` yet (reported). Phase 8 puts the `AntiopeState` in the environment of the Antiope window (`.environment(antiope)`).
- Phase 10: `ReaderTheme`, `ArticleTemplate.page(…theme:subpage:)`, `LinkPolicy`, `ReaderController` (`webView`, `loadedEntryId`, `load(entryId:html:)`), `ArticleWebView.Coordinator(controller:)`, `reader.css`'s `Antiope begin` … `Antiope end` section and its `--row`, `--cell`, `--tt-*` properties.
- Test helpers already in the target: `eventually(timeout:_:)` (`RunCoordinatorTests.swift`), `HangingServer` (`ReadingFixRound1Tests.swift`), `makeState()` (`AppStateTests.swift`).

**Produces (pinned names in bold):**
- `Teletext/MosaicRenderer.swift`: **`MosaicCell`**, **`MosaicImage`** (as pinned); **`MosaicRenderer.mosaic(from:maxColumns:)`**; `MosaicRenderer.png(_:scale:)` (see the first deviation below); `MosaicRenderer.columns(forPixelWidth:maxColumns:)`, `grid(width:height:columns:)`, `cell(for:)`; constants `cellWidth` 6, `cellHeight` 10, `columnEdges` `[0, 3, 6]`, `rowEdges` `[0, 3, 7, 10]`, `maxColumns` 40, `maxRows` 100, `pixelsPerColumn` 12.
- `Reading/MosaicSchemeHandler.swift`: **`MosaicSchemeHandler`** (`@MainActor final class`, `NSObject`, `WKURLSchemeHandler`; `scheme` `"ressac-mosaic"`, `maxBytes`, `timeout`, `failurePNG`, `CachedMosaic`, `init(fetcher:cache:)`, `runningCount`, `mosaic(for:fetcher:)`); `MosaicRequest` (`url(for:columns:)`, `init?(_ url: URL?)`, `original`, `columns`, `cacheKey`); `MosaicFetcher` (`shared`, `init(configuration:maxBytes:timeout:)`, `fetch(_:) async throws(MosaicFailure) -> Data`); `MosaicDecoder` (`maxSourcePixels`, `thumbnail(of:maxColumns:maxPixels:)`); `MosaicFailure` (`refused`, `status(Int)`, `tooLarge`, `unreadable`, `network`).
- `Reading/ArticleTemplate.swift`: **`ArticleTemplate.sanitize(_:baseURL:budget:mosaics:)`** (`mosaics` defaults to `false`); the CSP's `img-src` gains `ressac-mosaic:`; the Antiope page sets `--font-pixel`; `LinkPolicy.Decision.toggleImage(Int)`, **`LinkPolicy.toggleScheme`** (`"ressac-toggle"`), `LinkPolicy.toggleIndex(_:)`.
- `Reading/ReaderView.swift`: `ArticleWebView.configuration(mosaics:)`; `ReaderController.toggleImage(_:) async -> Bool` and `toggleAllImages() async -> Int`.

**Deviations from the index, with the reason:**
1. **`png(_:subpixel:)` becomes `png(_:scale:)`, one PNG pixel per font pixel × `scale`.** A teletext sub-pixel is not square: the cell is 6 × 10 font pixels, and the SAA5050 (which Bedstead copies) draws sextant rows of 3, 4 and 3 font pixels over columns of 3 and 3. Square sub-pixels would stretch every mosaic by 10/9 and miss the 3–4–3 rows. The page then zooms the PNG by the font pixel (`zoom: var(--font-pixel)`, 1.5 to 3 CSS pixels) with `image-rendering: pixelated`: whole device pixels on Retina at every Antiope size, and PNGs of at most 240 pixels wide.
2. **`MosaicSchemeHandler` is `@MainActor`.** The SDK marks `WKURLSchemeHandler` `WK_SWIFT_UI_ACTOR`; the work runs off the main actor in a `@concurrent` function.
3. **The CSP must name the scheme.** With today's `img-src http: https: data:`, WebKit blocks `ressac-mosaic:` subresources outright (measured: the handler is never called). With the scheme added, they load from the `about:blank` page that `loadHTMLString(_:baseURL: nil)` gives: no special base URL is needed. (This probably also explains the mac-app phase 10 finding that a custom-scheme `@font-face` did not load: `font-src` only allowed `data:`.)

## Verified ahead of time (14/10/2026, this Mac, Xcode 27, macOS 27 SDK)

- **Code and tests.** Both new files and all edits were compiled, and every test below run, in a SwiftPM harness (Swift 6.4, Swift 6 mode, complete strict concurrency, warnings as errors) with the real `ArticleTemplate.swift`, `ReaderStyle.swift` and `ReaderView.swift` after phase 10 and stand-ins for the phase 3, 4 and 7 types: 31 tests green, and ten runs in a row before the last trims. The checks that need the bundled `reader.css` (computed styles in the end-to-end test) passed with the composed stylesheet passed in by hand. The edit blocks were generated from those files and re-applied by script.
- **WebKit, measured in a `WKWebView` with JavaScript off:** custom-scheme images load from `about:blank` once the CSP lists the scheme (blocked otherwise); `callAsyncJavaScript` in the `.page` world with `arguments:` swaps an image's `src`; an app-JavaScript `click()` on a `ressac-toggle:0` link reaches `decidePolicyFor` as `.linkActivated`; `zoom` on an `img` scales its PNG by whole pixels and still honours `max-width: 100%` (a 240-pixel mosaic zoomed by 2 in a 300-pixel column is drawn 300 wide, aspect kept).
- **Failure display, measured:** when a scheme task fails (`didFailWithError`), WebKit draws its own blue "?" icon and **no** alt text, whatever the CSS. So a pure CSS fallback is impossible, and a placeholder PNG cannot carry the alt text as real text. **Chosen:** on any failure the handler answers a one-cell black PNG (no "?" icon) and marks the image's box `data-failed` through app JavaScript; the stylesheet then hides the image and shows a magenta line `IMAGE <alt text>` that is real, selectable text, read by VoiceOver. The original stays one click away. Rendered and inspected with Bedstead.
- **Decompression bomb, measured:** 187 KB of valid PNG can announce 8000 × 8000 pixels, and ImageIO reports those dimensions before decoding. The decoder refuses anything over 40 megapixels before asking for a thumbnail (a PNG is decoded in full before it is downsampled).

## Review Focus (this phase)

1. **Index #4: an `<img>` whose server answers HTML, 0 bytes, or 50 MB.** The `IMAGE` line shows, nothing hangs, memory stays bounded (the body is cut at 10 MB as it streams). `MosaicFetchTests.htmlEmptyAndSVGBodiesAreUnreadable`, `aBodyOverTenMegabytesIsCutOffWithoutHanging` (50 chunks of 1 MB, no `Content-Length`, under 5 s; and an announced 50 MB refused before the body), and the failure half of `MosaicWebViewTests`.
2. **WebKit stopping a load** (the article changes while images download): calling a stopped task throws an Objective-C exception and kills the app. `MosaicSchemeHandlerTests.aStoppedTaskIsNeverCalledBack`.
3. **A server that accepts and never answers:** the 15 s timeout ends it (0.5 s in the test). `aServerThatNeverAnswersTimesOut`.
4. **An image inside a link** (`<a href="big.jpg"><img src="small.jpg"></a>`, very common): anchors cannot nest, so the mosaic box is a `span` without its own toggle link; the click keeps opening the feed's link, `i` still swaps it. `MosaicMarkupTests.anImageInsideALinkKeepsTheLink`.
5. **A tracking pixel or an icon:** a 1 × 1 image becomes one cell, not a 40-column block; a 100-pixel picture gets 9 columns. `MosaicRendererTests.widthAndHeightStayWithinTheLimits`.

---

# Phase 11a

### Task 11a.1: `MosaicRenderer`

**Files:**
- Create: `apps/macos/Ressac/Teletext/MosaicRenderer.swift`
- Create: `apps/macos/RessacTests/MosaicTests.swift`

**Interfaces:**
- Consumes: `TeletextColor` (`allCases`, `rgb`).
- Produces: `MosaicCell`, `MosaicImage`, `MosaicRenderer` (`mosaic(from:maxColumns:)`, `png(_:scale:)`, `columns(forPixelWidth:maxColumns:)`, `grid(width:height:columns:)`, `cell(for:)`, the constants listed above).

**Rules (spec §7, pinned by the tests):**
- **Grid.** A cell is 0.6 em × 1 em, so for an image `w × h` drawn `c` columns wide, `rows = round(0.6 × c × h / w)`, at least 1. Over `maxRows` (100) the columns shrink to keep the shape.
- **Columns from the picture's width.** `columns(forPixelWidth:)` = ⌈width / 12⌉ clamped to 1…40: one cell per 12 image pixels, the width of a cell at the default 20 pt. The handler applies it to the original's width; `mosaic(from:maxColumns:)` itself draws exactly `maxColumns` columns.
- **Sampling.** The image is drawn over black (the page's paper, so transparency reads as black) into a `6c × 10r` sRGB bitmap with high-quality interpolation; each sub-pixel is the mean of its font pixels (rows 0–2, 3–6, 7–9; columns 0–2, 3–5 of each cell).
- **Colours.** For each cell, every unordered pair of the eight colours (28 pairs) and every single colour (8 uniform cells, tried first so they win ties) is scored by the sum over the six sub-pixels of the squared RGB distance to the nearer colour of the pair; the lowest score wins and each sub-pixel takes its nearer colour.
- **Canonical form.** Uniform cell: `foreground == background`, `mask == 0`. Otherwise the background is the colour of more sub-pixels; on a 3–3 tie, the one listed first in `TeletextColor`. Flat red is therefore `MosaicCell(foreground: .red, background: .red, mask: 0)` in every cell.
- **PNG.** Opaque sRGB, `6c·scale × 10r·scale` pixels, each pixel exactly one of the eight colours (no interpolation).

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/MosaicTests.swift` (the fixtures at the top serve every mosaic test of both phases):

```swift
import CoreGraphics
import Foundation
import ImageIO
import Synchronization
import Testing
import UniformTypeIdentifiers
import WebKit

@testable import Ressac

// MARK: Fixtures

/// An sRGB image filled pixel by pixel.
private func image(width: Int, height: Int, alpha: UInt8 = 255, _ color: (Int, Int) -> (UInt8, UInt8, UInt8)) -> CGImage {
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    for y in 0..<height {
        for x in 0..<width {
            let c = color(x, y)
            let i = (y * width + x) * 4
            bytes[i] = UInt8(Int(c.0) * Int(alpha) / 255)
            bytes[i + 1] = UInt8(Int(c.1) * Int(alpha) / 255)
            bytes[i + 2] = UInt8(Int(c.2) * Int(alpha) / 255)
            bytes[i + 3] = alpha
        }
    }
    let provider = CGDataProvider(data: Data(bytes) as CFData)!
    return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                   space: CGColorSpace(name: CGColorSpace.sRGB)!,
                   bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                   provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
}

private func flat(_ r: UInt8, _ g: UInt8, _ b: UInt8, width: Int = 60, height: Int = 100) -> CGImage {
    image(width: width, height: height) { _, _ in (r, g, b) }
}

/// `images` encoded as one file of `type` (several frames for a GIF).
private func encoded(_ images: [CGImage], as type: UTType) -> Data {
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, type.identifier as CFString, images.count, nil)!
    for image in images { CGImageDestinationAddImage(destination, image, nil) }
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
}

/// Decodes a PNG to (width, height, RGB at a pixel).
private func decodePNG(_ data: Data) -> (width: Int, height: Int, rgb: (Int, Int) -> (UInt8, UInt8, UInt8))? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
    let width = image.width, height = image.height
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    bytes.withUnsafeMutableBytes { buffer in
        let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    let pixels = bytes
    return (width, height, { x, y in
        let i = (y * width + x) * 4
        return (pixels[i], pixels[i + 1], pixels[i + 2])
    })
}

private func uniform(_ color: TeletextColor) -> MosaicCell {
    MosaicCell(foreground: color, background: color, mask: 0)
}

// MARK: Renderer

@Suite struct MosaicRendererTests {
    @Test func aFlatColourGivesUniformCellsInCanonicalForm() {
        let mosaic = MosaicRenderer.mosaic(from: flat(255, 0, 0), maxColumns: 10)
        #expect(mosaic.columns == 10)
        #expect(mosaic.rows == 10)  // 0.6 × 10 × 100 / 60
        #expect(mosaic.cells == Array(repeating: uniform(.red), count: 100))
    }

    @Test func aHalfBlackHalfWhiteCellTakesBlackAsBackground() {
        // One cell, 6 × 10 pixels: the left sub-pixel column white, the right one black. A 3–3 tie
        // puts the colour listed first (black) in the background.
        let leftWhite = image(width: 6, height: 10) { x, _ in x < 3 ? (255, 255, 255) : (0, 0, 0) }
        let mosaic = MosaicRenderer.mosaic(from: leftWhite, maxColumns: 1)
        #expect(mosaic.columns == 1 && mosaic.rows == 1)
        #expect(mosaic.cells == [MosaicCell(foreground: .white, background: .black, mask: 0b01_0101)])
        // The top row white (2 of 6): bits 0 and 1.
        let topWhite = image(width: 6, height: 10) { _, y in y < 3 ? (255, 255, 255) : (0, 0, 0) }
        #expect(MosaicRenderer.mosaic(from: topWhite, maxColumns: 1).cells
            == [MosaicCell(foreground: .white, background: .black, mask: 0b00_0011)])
        // Four white of six: white is the background, the two black sub-pixels the foreground.
        let mostlyWhite = image(width: 6, height: 10) { _, y in y < 7 ? (255, 255, 255) : (0, 0, 0) }
        #expect(MosaicRenderer.mosaic(from: mostlyWhite, maxColumns: 1).cells
            == [MosaicCell(foreground: .black, background: .white, mask: 0b11_0000)])
    }

    @Test func coloursQuantiseToTheNearestTeletextColour() {
        #expect(MosaicRenderer.mosaic(from: flat(30, 220, 235), maxColumns: 4).cells.allSatisfy { $0 == uniform(.cyan) })
        #expect(MosaicRenderer.mosaic(from: flat(200, 30, 40), maxColumns: 4).cells.allSatisfy { $0 == uniform(.red) })
        #expect(MosaicRenderer.mosaic(from: flat(20, 20, 20), maxColumns: 4).cells.allSatisfy { $0 == uniform(.black) })
        let samples = Array(repeating: SIMD3<Double>(250, 240, 20), count: 6)
        #expect(MosaicRenderer.cell(for: samples) == uniform(.yellow))
    }

    @Test func transparencyReadsAsTheBlackPaper() {
        let clear = image(width: 12, height: 20, alpha: 0) { _, _ in (255, 255, 255) }
        #expect(MosaicRenderer.mosaic(from: clear, maxColumns: 2).cells.allSatisfy { $0 == uniform(.black) })
    }

    @Test func widthAndHeightStayWithinTheLimits() {
        let wide = MosaicRenderer.mosaic(from: flat(0, 0, 255, width: 1000, height: 500), maxColumns: 40)
        #expect(wide.columns == 40)
        #expect(wide.rows == 12)  // 0.6 × 40 × 500 / 1000
        #expect(MosaicRenderer.columns(forPixelWidth: 100) == 9)
        #expect(MosaicRenderer.columns(forPixelWidth: 5000) == 40)
        #expect(MosaicRenderer.columns(forPixelWidth: 1) == 1)
        #expect(MosaicRenderer.columns(forPixelWidth: 480, maxColumns: 20) == 20)
        // A tall strip keeps its shape under the row limit.
        let strip = MosaicRenderer.grid(width: 100, height: 10_000, columns: 40)
        #expect(strip.rows <= MosaicRenderer.maxRows)
        #expect(strip.columns == 1)
    }

    @Test func thePNGHasOnePixelPerFontPixelWithSextantRowsOfThreeFourThree() throws {
        // Two cells: the left one has only its middle-left sub-pixel (bit 2) in green on blue,
        // the right one is uniform magenta.
        let mosaic = MosaicImage(columns: 2, rows: 1, cells: [
            MosaicCell(foreground: .green, background: .blue, mask: 0b00_0100), uniform(.magenta),
        ])
        let png = try #require(decodePNG(MosaicRenderer.png(mosaic)))
        #expect(png.width == 12 && png.height == 10)
        #expect(png.rgb(1, 2) == (0, 0, 255))  // top row: background
        #expect(png.rgb(1, 3) == (0, 255, 0))  // middle row starts at font pixel 3
        #expect(png.rgb(2, 6) == (0, 255, 0))  // and is 4 pixels tall
        #expect(png.rgb(1, 7) == (0, 0, 255))  // bottom row
        #expect(png.rgb(3, 4) == (0, 0, 255))  // right column of the cell
        #expect(png.rgb(6, 0) == (255, 0, 255))
        let doubled = try #require(decodePNG(MosaicRenderer.png(mosaic, scale: 2)))
        #expect(doubled.width == 24 && doubled.height == 20)
        #expect(MosaicRenderer.png(MosaicImage(columns: 2, rows: 1, cells: [])).isEmpty)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/MosaicRendererTests`. Expected: build failure, `cannot find 'MosaicRenderer' in scope` and `cannot find type 'MosaicCell' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Teletext/MosaicRenderer.swift`:

```swift
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// One teletext mosaic cell: 2 × 3 sub-pixels, each in the foreground or the background colour.
struct MosaicCell: Equatable, Sendable {
    var foreground: TeletextColor
    var background: TeletextColor
    /// 6 bits, bit 0 = top-left, row-major (1 top-right, 2 middle-left, 3 middle-right,
    /// 4 bottom-left, 5 bottom-right). A set bit is a foreground sub-pixel.
    var mask: UInt8
}

/// A picture as teletext mosaics: `columns × rows` cells, row-major.
struct MosaicImage: Equatable, Sendable {
    var columns: Int
    var rows: Int
    var cells: [MosaicCell]
}

/// Turns an image into two-colour block mosaics (spec §7). Pure: no I/O, no global state.
///
/// Geometry follows Bedstead and the SAA5050 it copies: a cell is 6 × 10 font pixels
/// (0.6 em × 1 em), split into sub-pixel columns of 3 + 3 and rows of 3 + 4 + 3 font pixels.
/// Each cell takes the pair of teletext colours that best fits its six sub-pixels, by least
/// squared RGB error, and each sub-pixel takes the nearer of the two.
///
/// Canonical form, so equal pictures give equal cells: a cell whose six sub-pixels share one
/// colour has `foreground == background` and `mask == 0`; otherwise the background is the
/// colour of more sub-pixels, and on a 3–3 tie the one listed first in `TeletextColor`.
enum MosaicRenderer {
    static let cellWidth = 6
    static let cellHeight = 10
    /// Sub-pixel boundaries inside a cell, in font pixels.
    static let columnEdges = [0, 3, 6]
    static let rowEdges = [0, 3, 7, 10]
    /// Widest mosaic, in cells (spec §7).
    static let maxColumns = 40
    /// Tallest mosaic, in cells: a strip-shaped image must not make the page endless.
    static let maxRows = 100
    /// Image pixels per cell when sizing a picture: one cell at the default 20 pt is 12 pt wide,
    /// so a 120-pixel picture gets 10 columns and a 1-pixel tracker one.
    static let pixelsPerColumn = 12.0

    /// How many columns an image `width` pixels wide gets, at most `maxColumns`.
    static func columns(forPixelWidth width: Int, maxColumns: Int = maxColumns) -> Int {
        let wanted = Int((Double(max(width, 1)) / pixelsPerColumn).rounded(.up))
        return min(max(wanted, 1), max(maxColumns, 1))
    }

    /// The cell grid for a `width × height` image `columns` wide. Cells are 0.6 em by 1 em, so
    /// `rows = 0.6 × columns × height / width`; past `maxRows` the columns shrink to keep the shape.
    static func grid(width: Int, height: Int, columns: Int) -> (columns: Int, rows: Int) {
        let aspect = Double(max(height, 1)) / Double(max(width, 1))
        func rows(for columns: Int) -> Int { max(1, Int((0.6 * Double(columns) * aspect).rounded())) }
        var columns = max(columns, 1)
        if rows(for: columns) > maxRows {
            columns = max(1, Int(Double(maxRows) / (0.6 * aspect)))
        }
        return (columns, min(rows(for: columns), maxRows))
    }

    static func mosaic(from image: CGImage, maxColumns: Int) -> MosaicImage {
        let (columns, rows) = grid(width: image.width, height: image.height, columns: maxColumns)
        let width = columns * cellWidth
        let pixels = fontPixels(of: image, width: width, height: rows * cellHeight)
        var cells: [MosaicCell] = []
        cells.reserveCapacity(columns * rows)
        for row in 0..<rows {
            for column in 0..<columns {
                let samples = (0..<6).map { bit in
                    let x = column * cellWidth, y = row * cellHeight
                    return average(pixels, stride: width,
                                   x: (x + columnEdges[bit % 2])..<(x + columnEdges[bit % 2 + 1]),
                                   y: (y + rowEdges[bit / 2])..<(y + rowEdges[bit / 2 + 1]))
                }
                cells.append(cell(for: samples))
            }
        }
        return MosaicImage(columns: columns, rows: rows, cells: cells)
    }

    /// The best cell for six sub-pixel colours (0...255 per channel), in canonical form.
    static func cell(for samples: [SIMD3<Double>]) -> MosaicCell {
        let distances = samples.map { sample in
            palette.map { colour in ((sample - colour.rgb) * (sample - colour.rgb)).sum() }
        }
        // a == b is the uniform cell; listed first, it wins any tie with a pair that adds nothing.
        var best = (a: 0, b: 0, error: Double.infinity)
        for a in palette.indices {
            for b in a..<palette.count {
                let error = distances.reduce(0) { $0 + min($1[a], $1[b]) }
                if error < best.error { best = (a, b, error) }
            }
        }
        var maskOfB: UInt8 = 0
        for (bit, distance) in distances.enumerated() where distance[best.b] < distance[best.a] {
            maskOfB |= 1 << UInt8(bit)
        }
        let a = palette[best.a].color, b = palette[best.b].color
        switch maskOfB.nonzeroBitCount {
        case 0: return MosaicCell(foreground: a, background: a, mask: 0)
        case 6: return MosaicCell(foreground: b, background: b, mask: 0)
        case 4, 5: return MosaicCell(foreground: a, background: b, mask: ~maskOfB & 0b11_1111)
        default: return MosaicCell(foreground: b, background: a, mask: maskOfB)  // a comes first on a tie
        }
    }

    /// A PNG with one pixel per font pixel × `scale`: a cell is `6·scale × 10·scale` pixels.
    /// The page shows it with `image-rendering: pixelated` and zooms it by the font pixel.
    static func png(_ mosaic: MosaicImage, scale: Int = 1) -> Data {
        let scale = max(scale, 1)
        guard mosaic.columns > 0, mosaic.rows > 0, mosaic.cells.count == mosaic.columns * mosaic.rows else { return Data() }
        let width = mosaic.columns * cellWidth * scale, height = mosaic.rows * cellHeight * scale
        var bytes = [UInt8](repeating: 255, count: width * height * 4)
        for y in 0..<height {
            let fontY = y / scale
            let row = fontY / cellHeight
            let subRow = rowEdges.lastIndex { $0 <= fontY % cellHeight } ?? 0
            for x in 0..<width {
                let fontX = x / scale
                let cell = mosaic.cells[row * mosaic.columns + fontX / cellWidth]
                let bit = UInt8(subRow * 2 + (fontX % cellWidth < columnEdges[1] ? 0 : 1))
                let rgb = (cell.mask & (1 << bit) != 0 ? cell.foreground : cell.background).rgb
                let i = (y * width + x) * 4
                bytes[i] = rgb.r
                bytes[i + 1] = rgb.g
                bytes[i + 2] = rgb.b
            }
        }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                                  space: sRGB, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        else { return Data() }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return Data() }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : Data()
    }

    // MARK: Private

    private static let sRGB = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()

    private static let palette: [(color: TeletextColor, rgb: SIMD3<Double>)] = TeletextColor.allCases.map {
        ($0, SIMD3(Double($0.rgb.r), Double($0.rgb.g), Double($0.rgb.b)))
    }

    /// The image drawn over black (the page's paper, so transparency reads as black) at
    /// `width × height`, as RGBA bytes, top row first.
    private static func fontPixels(of image: CGImage, width: Int, height: Int) -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        bytes.withUnsafeMutableBytes { buffer in
            guard let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                          bytesPerRow: width * 4, space: sRGB,
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return }
            let frame = CGRect(x: 0, y: 0, width: width, height: height)
            context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 1))
            context.fill(frame)
            context.interpolationQuality = .high
            context.draw(image, in: frame)
        }
        return bytes
    }

    private static func average(_ pixels: [UInt8], stride: Int, x: Range<Int>, y: Range<Int>) -> SIMD3<Double> {
        var sum = SIMD3<Double>(0, 0, 0)
        for row in y {
            for column in x {
                let i = (row * stride + column) * 4
                sum += SIMD3(Double(pixels[i]), Double(pixels[i + 1]), Double(pixels[i + 2]))
            }
        }
        return sum / Double(x.count * y.count)
    }
}
```

- [ ] **Step 4: Run green.** Same command. Expected: `MosaicRendererTests`, 6 tests pass.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Teletext/MosaicRenderer.swift apps/macos/RessacTests/MosaicTests.swift
git commit -m "Mosaic renderer: teletext sextant cells and a font-pixel PNG

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 11a.2: Mosaic addresses, the capped fetcher, the decoder and the scheme handler

**Files:**
- Create: `apps/macos/Ressac/Reading/MosaicSchemeHandler.swift`
- Test: `apps/macos/RessacTests/MosaicTests.swift`

**Interfaces:**
- Consumes: `MosaicRenderer` (11a.1); `LinkPolicy.webURL(_:)` (http(s) with a host, phase 10 of the mac app).
- Produces: `MosaicFailure`, `MosaicRequest`, `MosaicFetcher`, `MosaicDecoder`, `MosaicSchemeHandler` as listed above.

**Design:**
- **Address.** `ressac-mosaic:image?cols=40&src=<original>`, the original percent-encoded except unreserved ASCII (`A–Z a–z 0–9 - . _ ~`), so `&`, `?`, `#` and non-ASCII survive the round trip and WebKit's URL canonicalisation leaves the string unchanged. `MosaicRequest(_:)` refuses anything but `cols` in 1…40 and an http(s) original with a host (`file:`, `javascript:`, `data:`, `ftp:` → `nil` → `MosaicFailure.refused`).
- **Fetcher.** One `URLSession` per fetcher (ephemeral, no cookies, no URL cache), `timeoutIntervalForRequest` and `timeoutIntervalForResource` both 15 s. A delegate sees the response first: a status outside 200…299 is `status(n)`, an announced `Content-Length` over 10 MB is `tooLarge`, both cancel before the body. Then it counts bytes as they arrive and cancels the transfer the moment the total would pass 10 MB (`tooLarge`): memory holds at most 10 MB plus one chunk. The first outcome wins; a slot is prepared before the task can be cancelled, so a cancellation that lands before the continuation exists is kept, never lost (no hang). `fetch` uses typed throws.
- **Decoder.** `CGImageSourceCreateWithData` without caching; no type, no frame, or no dimensions → `unreadable` (HTML, an empty body, SVG: ImageIO has no SVG decoder). Over 40 megapixels → `tooLarge`, before decoding. Otherwise a thumbnail of frame 0 (a GIF's first frame) with `kCGImageSourceCreateThumbnailFromImageAlways`, the EXIF orientation applied, and a longest side of about two image pixels per font pixel of the mosaic.
- **Handler.** WebKit calls `start` and `stop` on the main actor. A cache hit answers at once. Otherwise a `Task` runs `mosaic(for:fetcher:)` (`@concurrent`, so download, decoding and rendering leave the main actor) and comes back to the main actor to answer, **only if** its token is still the one registered for that task: `stop` removes the entry and cancels the work, and WebKit throws an exception at any call on a stopped task. Every answer is `200 image/png`: the mosaic, or `failurePNG` (one black cell) plus app JavaScript that sets `data-failed` on every `.mosaic` box showing that address, which the stylesheet (11b) turns into the `IMAGE` line. Results are cached in an `NSCache` (16 MB of PNG, shared by every reader web view, keyed by columns and original); network failures are not cached, so a later visit retries.

- [ ] **Step 1: Write the failing tests.** Append to `MosaicTests.swift`:

```swift
// MARK: Addresses

@Suite struct MosaicRequestTests {
    @Test func theOriginalSurvivesTheRoundTrip() throws {
        let original = try #require(URL(string: "https://img.example.org/a b/photo.jpg?w=800&h=600#x"))
        let address = MosaicRequest.url(for: original, columns: 40)
        #expect(address.hasPrefix("ressac-mosaic:image?cols=40&src=https%3A%2F%2Fimg.example.org"))
        let request = try #require(MosaicRequest(URL(string: address)))
        #expect(request.original == original)
        #expect(request.columns == 40)
    }

    @Test func otherSchemesAndBadColumnsAreRefused() {
        for source in ["file:///etc/passwd", "javascript:alert(1)", "data:image/png;base64,AAAA", "ftp://example.org/a.png", "https:///nohost"] {
            let address = MosaicRequest.url(for: URL(string: source)!, columns: 40)
            #expect(MosaicRequest(URL(string: address)) == nil, "\(source)")
        }
        let good = "https%3A%2F%2Fexample.org%2Fa.png"
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?cols=0&src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?cols=41&src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "ressac-mosaic:image?src=\(good)")) == nil)
        #expect(MosaicRequest(URL(string: "https://example.org/a.png")) == nil)
        #expect(MosaicRequest(nil) == nil)
    }
}

// MARK: Fetching (Review Focus #4)

/// Serves canned answers to `MosaicFetcher` in process: a status, headers and body chunks.
final class StubProtocol: URLProtocol, @unchecked Sendable {
    struct Answer: Sendable {
        var status = 200
        var headers: [String: String] = [:]
        var chunks: [Data] = []
        /// Seconds before the response, to keep a load in flight.
        var delay: TimeInterval = 0
    }

    static let answers = Mutex<[String: Answer]>([:])
    private let stopped = Mutex(false)

    static func serve(_ path: String, _ answer: Answer) {
        answers.withLock { $0[path] = answer }
    }

    static func configuration() -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [StubProtocol.self]
        return configuration
    }

    override class func canInit(with request: URLRequest) -> Bool { request.url?.host() == "stub.ressac.test" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let url = request.url!
        guard let answer = Self.answers.withLock({ $0[url.path()] }) else {
            client?.urlProtocol(self, didFailWithError: URLError(.cannotFindHost))
            return
        }
        Thread.detachNewThread { [self] in
            Thread.sleep(forTimeInterval: answer.delay)
            let response = HTTPURLResponse(url: url, statusCode: answer.status, httpVersion: "HTTP/1.1", headerFields: answer.headers)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            for chunk in answer.chunks {
                if stopped.withLock({ $0 }) { return }
                client?.urlProtocol(self, didLoad: chunk)
            }
            if !stopped.withLock({ $0 }) { client?.urlProtocolDidFinishLoading(self) }
        }
    }

    override func stopLoading() {
        stopped.withLock { $0 = true }
    }
}

private func stub(_ path: String) -> MosaicRequest {
    MosaicRequest(URL(string: MosaicRequest.url(for: URL(string: "https://stub.ressac.test\(path)")!, columns: 40)))!
}

@Suite(.serialized) struct MosaicFetchTests {
    let fetcher = MosaicFetcher(configuration: StubProtocol.configuration())

    @Test func anImageBecomesAMosaicPNG() async throws {
        StubProtocol.serve("/red.png", .init(headers: ["Content-Type": "image/png"],
                                             chunks: [encoded([flat(255, 0, 0, width: 120, height: 200)], as: .png)]))
        let png = try await MosaicSchemeHandler.mosaic(for: stub("/red.png"), fetcher: fetcher).get()
        let decoded = try #require(decodePNG(png))
        #expect(decoded.width == 10 * 6)  // 120 pixels: 10 columns of 6 font pixels
        #expect(decoded.height == 10 * 10)  // 0.6 × 10 × 200 / 120 = 10 rows of 10 font pixels
        #expect(decoded.rgb(5, 5) == (255, 0, 0))
    }

    @Test func aGIFUsesItsFirstFrame() async throws {
        let gif = encoded([flat(255, 0, 0, width: 24, height: 40), flat(0, 0, 255, width: 24, height: 40)], as: .gif)
        StubProtocol.serve("/anim.gif", .init(chunks: [gif]))
        let png = try await MosaicSchemeHandler.mosaic(for: stub("/anim.gif"), fetcher: fetcher).get()
        #expect(try #require(decodePNG(png)).rgb(0, 0) == (255, 0, 0))
    }

    @Test func htmlEmptyAndSVGBodiesAreUnreadable() async {
        StubProtocol.serve("/page.html", .init(headers: ["Content-Type": "text/html"], chunks: [Data("<html><body>Not an image</body></html>".utf8)]))
        StubProtocol.serve("/empty.png", .init(chunks: []))
        StubProtocol.serve("/logo.svg", .init(chunks: [Data(#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#.utf8)]))
        for path in ["/page.html", "/empty.png", "/logo.svg"] {
            #expect(await MosaicSchemeHandler.mosaic(for: stub(path), fetcher: fetcher) == .failure(.unreadable), "\(path)")
        }
    }

    @Test func anHTTPErrorIsAFailure() async {
        StubProtocol.serve("/gone.png", .init(status: 404, chunks: [Data("Not found".utf8)]))
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/gone.png"), fetcher: fetcher) == .failure(.status(404)))
    }

    @Test func aBodyOverTenMegabytesIsCutOffWithoutHanging() async {
        let megabyte = Data(repeating: 0x41, count: 1 << 20)
        // No Content-Length: the cap is enforced while the body streams in.
        StubProtocol.serve("/huge.jpg", .init(chunks: Array(repeating: megabyte, count: 50)))
        let start = ContinuousClock.now
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/huge.jpg"), fetcher: fetcher) == .failure(.tooLarge))
        #expect(ContinuousClock.now - start < .seconds(5))
        // An announced length over the cap is refused before the body.
        StubProtocol.serve("/announced.jpg", .init(headers: ["Content-Length": "\(50 << 20)"], chunks: [megabyte]))
        #expect(await MosaicSchemeHandler.mosaic(for: stub("/announced.jpg"), fetcher: fetcher) == .failure(.tooLarge))
    }

    /// 190 KB of PNG can announce 8000 × 8000 pixels, which ImageIO would decode in full: the
    /// size is checked from the header first. A small real PNG under a small limit stands for it.
    @Test func tooManyPixelsAreRefusedBeforeDecoding() {
        let png = encoded([flat(255, 255, 255, width: 100, height: 100)], as: .png)
        guard case .failure(let failure) = MosaicDecoder.thumbnail(of: png, maxColumns: 40, maxPixels: 9_999) else {
            Issue.record("decoded an image over the pixel limit")
            return
        }
        #expect(failure == .tooLarge)
        guard case .success(let thumbnail) = MosaicDecoder.thumbnail(of: png, maxColumns: 40, maxPixels: 10_000) else {
            Issue.record("refused an image at the pixel limit")
            return
        }
        #expect(thumbnail.columns == 9)
    }

    @Test func aServerThatNeverAnswersTimesOut() async throws {
        let server = try HangingServer()
        let quick = MosaicFetcher(timeout: 0.5)
        let request = try #require(MosaicRequest(URL(string: MosaicRequest.url(
            for: URL(string: "http://127.0.0.1:\(server.port)/slow.png")!, columns: 40))))
        let start = ContinuousClock.now
        #expect(await MosaicSchemeHandler.mosaic(for: request, fetcher: quick) == .failure(.network))
        #expect(ContinuousClock.now - start < .seconds(3))
    }
}

// MARK: Handler

/// Records what the handler sends to one WebKit task.
final class FakeSchemeTask: NSObject, WKURLSchemeTask {
    let request: URLRequest
    private(set) var events: [String] = []
    private(set) var body = Data()
    private(set) var response: HTTPURLResponse?

    init(_ url: String) {
        request = URLRequest(url: URL(string: url)!)
    }

    func didReceive(_ response: URLResponse) {
        self.response = response as? HTTPURLResponse
        events.append("response")
    }

    func didReceive(_ data: Data) {
        body.append(data)
        events.append("data")
    }

    func didFinish() { events.append("finish") }
    func didFailWithError(_ error: any Error) { events.append("fail") }
}

@MainActor @Suite(.serialized) struct MosaicSchemeHandlerTests {
    let webView = WKWebView()

    func handler() -> MosaicSchemeHandler {
        MosaicSchemeHandler(fetcher: MosaicFetcher(configuration: StubProtocol.configuration()), cache: NSCache())
    }

    @Test func aMosaicIsAnsweredAsAPNG() async throws {
        StubProtocol.serve("/blue.png", .init(chunks: [encoded([flat(0, 0, 255, width: 60, height: 60)], as: .png)]))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/blue.png")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(await eventually { task.events == ["response", "data", "finish"] })
        #expect(task.response?.statusCode == 200)
        #expect(task.response?.value(forHTTPHeaderField: "Content-Type") == "image/png")
        #expect(try #require(decodePNG(task.body)).width == 5 * 6)
        #expect(handler.runningCount == 0)
    }

    /// A cached mosaic is answered at once, without a download. (A real `NSCache` may evict at
    /// any time, even right after `setObject`, under the memory pressure of the full suite: the
    /// test hands the handler a cache that keeps what it is given.)
    @Test func aCachedMosaicIsAnsweredAtOnce() throws {
        let cache = KeepingCache()
        let handler = MosaicSchemeHandler(fetcher: MosaicFetcher(configuration: StubProtocol.configuration()), cache: cache)
        let address = MosaicRequest.url(for: URL(string: "https://stub.ressac.test/never-fetched.png")!, columns: 40)
        let request = try #require(MosaicRequest(URL(string: address)))
        cache.setObject(MosaicSchemeHandler.CachedMosaic(.success(Data("cached".utf8))), forKey: request.cacheKey)
        let task = FakeSchemeTask(address)
        handler.webView(webView, start: task)
        #expect(task.events == ["response", "data", "finish"])
        #expect(task.body == Data("cached".utf8))
        #expect(handler.runningCount == 0)
    }

    @Test func aFailureAnswersTheBlackPlaceholder() async {
        StubProtocol.serve("/not-an-image.html", .init(chunks: [Data("<html></html>".utf8)]))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/not-an-image.html")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(await eventually { task.events == ["response", "data", "finish"] })
        #expect(task.body == MosaicSchemeHandler.failurePNG)
        let refused = FakeSchemeTask("ressac-mosaic:image?cols=40&src=file%3A%2F%2F%2Fetc%2Fpasswd")
        handler.webView(webView, start: refused)
        #expect(refused.events == ["response", "data", "finish"])
        #expect(refused.body == MosaicSchemeHandler.failurePNG)
    }

    /// WebKit throws if a stopped task is called back: nothing may reach it after `stop`.
    @Test func aStoppedTaskIsNeverCalledBack() async throws {
        StubProtocol.serve("/slow.png", .init(chunks: [encoded([flat(0, 255, 0)], as: .png)], delay: 0.3))
        let handler = handler()
        let task = FakeSchemeTask(MosaicRequest.url(for: URL(string: "https://stub.ressac.test/slow.png")!, columns: 40))
        handler.webView(webView, start: task)
        #expect(handler.runningCount == 1)
        handler.webView(webView, stop: task)
        #expect(handler.runningCount == 0)
        try await Task.sleep(for: .milliseconds(800))
        #expect(task.events.isEmpty)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/MosaicRequestTests -only-testing:RessacTests/MosaicFetchTests -only-testing:RessacTests/MosaicSchemeHandlerTests`. Expected: build failure, `cannot find 'MosaicRequest' in scope`, `cannot find 'MosaicFetcher' in scope`, `cannot find 'MosaicSchemeHandler' in scope`, `cannot find 'MosaicDecoder' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Reading/MosaicSchemeHandler.swift`:

```swift
import Foundation
import ImageIO
import Synchronization
import WebKit

/// Why an image has no mosaic. Every case shows the magenta IMAGE block (spec §7).
enum MosaicFailure: Error, Equatable, Sendable {
    /// Not a mosaic address, or an original that is not http(s).
    case refused
    case status(Int)
    /// Over `MosaicSchemeHandler.maxBytes`, or too many pixels to decode safely.
    case tooLarge
    /// Not an image ImageIO decodes: HTML, an empty body, SVG…
    case unreadable
    /// Timeout, DNS, connection, TLS.
    case network
}

/// A `ressac-mosaic:` address: `ressac-mosaic:image?cols=40&src=<original, percent-encoded>`.
struct MosaicRequest: Equatable, Sendable {
    var original: URL
    var columns: Int

    /// Everything but unreserved ASCII is percent-encoded, so the original survives intact.
    private static let unreserved = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")

    static func url(for original: URL, columns: Int) -> String {
        let source = original.absoluteString.addingPercentEncoding(withAllowedCharacters: unreserved) ?? ""
        return "\(MosaicSchemeHandler.scheme):image?cols=\(columns)&src=\(source)"
    }

    /// Nil unless `url` is a well-formed mosaic address for an http(s) original.
    init?(_ url: URL?) {
        guard let url, url.scheme?.lowercased() == MosaicSchemeHandler.scheme,
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let columns = items.first(where: { $0.name == "cols" })?.value.flatMap(Int.init),
              (1...MosaicRenderer.maxColumns).contains(columns),
              let original = LinkPolicy.webURL(items.first(where: { $0.name == "src" })?.value)
        else { return nil }
        self.original = original
        self.columns = columns
    }

    var cacheKey: NSString { "\(columns) \(original.absoluteString)" as NSString }
}

/// Downloads originals with the mosaic limits: a byte cap enforced while the body streams in
/// (the transfer is cancelled as soon as it is passed) and one timeout for the whole transfer.
final class MosaicFetcher: Sendable {
    static let shared = MosaicFetcher()

    private let session: URLSession
    private let delegate: Delegate

    init(configuration: URLSessionConfiguration = .ephemeral, maxBytes: Int = MosaicSchemeHandler.maxBytes,
         timeout: TimeInterval = MosaicSchemeHandler.timeout) {
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        configuration.httpCookieAcceptPolicy = .never
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        delegate = Delegate(maxBytes: maxBytes)
        session = URLSession(configuration: configuration, delegate: delegate, delegateQueue: nil)
    }

    deinit {
        session.invalidateAndCancel()
    }

    func fetch(_ url: URL) async throws(MosaicFailure) -> Data {
        let task = session.dataTask(with: URLRequest(url: url))
        let delegate = delegate
        // The slot exists before the task can be cancelled, so an early completion is kept for
        // the continuation instead of being lost.
        delegate.prepare(task)
        let result: Result<Data, MosaicFailure> = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                delegate.attach(continuation, to: task)
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
        return try result.get()
    }

    private final class Delegate: NSObject, URLSessionDataDelegate, Sendable {
        private typealias Continuation = CheckedContinuation<Result<Data, MosaicFailure>, Never>

        /// One transfer: its body so far, then its outcome or the caller waiting for it.
        private struct Load {
            var data = Data()
            var continuation: Continuation?
            var result: Result<Data, MosaicFailure>?
        }

        private let maxBytes: Int
        private let loads = Mutex<[Int: Load]>([:])

        init(maxBytes: Int) {
            self.maxBytes = maxBytes
        }

        func prepare(_ task: URLSessionTask) {
            loads.withLock { $0[task.taskIdentifier] = Load() }
        }

        func attach(_ continuation: CheckedContinuation<Result<Data, MosaicFailure>, Never>, to task: URLSessionTask) {
            let early = loads.withLock { loads -> Result<Data, MosaicFailure>? in
                guard var load = loads[task.taskIdentifier] else { return .failure(.network) }
                if let result = load.result {
                    loads[task.taskIdentifier] = nil
                    return result
                }
                load.continuation = continuation
                loads[task.taskIdentifier] = load
                return nil
            }
            if let early { continuation.resume(returning: early) }
        }

        /// The first outcome wins (the cap, the status check or the completion); later ones are ignored.
        private func finish(_ task: URLSessionTask, _ result: Result<Data, MosaicFailure>) {
            let waiting = loads.withLock { loads -> Continuation? in
                guard var load = loads[task.taskIdentifier], load.result == nil else { return nil }
                guard let continuation = load.continuation else {
                    load.result = result
                    loads[task.taskIdentifier] = load
                    return nil
                }
                loads[task.taskIdentifier] = nil
                return continuation
            }
            waiting?.resume(returning: result)
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                        completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void) {
            let status = (response as? HTTPURLResponse)?.statusCode ?? 200
            if !(200...299).contains(status) {
                finish(dataTask, .failure(.status(status)))
                completionHandler(.cancel)
            } else if response.expectedContentLength > Int64(maxBytes) {
                finish(dataTask, .failure(.tooLarge))
                completionHandler(.cancel)
            } else {
                completionHandler(.allow)
            }
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
            let overflow = loads.withLock { loads -> Bool in
                guard var load = loads[dataTask.taskIdentifier], load.result == nil else { return false }
                guard load.data.count + data.count <= maxBytes else { return true }
                load.data.append(data)
                loads[dataTask.taskIdentifier] = load
                return false
            }
            if overflow {
                finish(dataTask, .failure(.tooLarge))
                dataTask.cancel()
            }
        }

        func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
            let data = loads.withLock { $0[task.taskIdentifier]?.data } ?? Data()
            finish(task, error == nil ? .success(data) : .failure(.network))
        }
    }
}

/// Decodes a downloaded original into a small image for `MosaicRenderer` (spec §7).
enum MosaicDecoder {
    /// Images with more pixels than this are refused unread: ImageIO decodes a PNG in full
    /// before it downsamples, and 190 KB of PNG can announce 8000 × 8000 pixels (checked).
    static let maxSourcePixels = 40_000_000

    /// The first frame (a GIF's too), downsampled by ImageIO to about two image pixels per font
    /// pixel of the mosaic, with the columns that the original's width gives (`maxColumns` at most).
    static func thumbnail(of data: Data, maxColumns: Int,
                          maxPixels: Int = maxSourcePixels) -> Result<(image: CGImage, columns: Int), MosaicFailure> {
        guard !data.isEmpty,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetType(source) != nil, CGImageSourceGetCount(source) > 0,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int, width > 0,
              let height = properties[kCGImagePropertyPixelHeight] as? Int, height > 0
        else { return .failure(.unreadable) }
        let pixels = width.multipliedReportingOverflow(by: height)
        guard !pixels.overflow, pixels.partialValue <= maxPixels else { return .failure(.tooLarge) }
        let columns = MosaicRenderer.columns(forPixelWidth: width, maxColumns: maxColumns)
        let grid = MosaicRenderer.grid(width: width, height: height, columns: columns)
        let longSide = 2 * max(grid.columns * MosaicRenderer.cellWidth, grid.rows * MosaicRenderer.cellHeight)
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: longSide,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return .failure(.unreadable) }
        return .success((image, columns))
    }
}

/// Serves `ressac-mosaic:` images to the reader (spec §7): fetches the original, decodes a
/// thumbnail, renders a mosaic PNG. WebKit calls it on the main thread; the download, decoding
/// and rendering run off it. A failure answers a small black PNG and marks the image's box
/// `data-failed`, so the stylesheet shows the magenta IMAGE block with the alt text: an image
/// request that fails makes WebKit draw its own blue "?" icon and no alt text (checked on
/// macOS 27), so the fallback cannot come from WebKit itself.
@MainActor final class MosaicSchemeHandler: NSObject, WKURLSchemeHandler {
    nonisolated static let scheme = "ressac-mosaic"
    nonisolated static let maxBytes = 10 * 1024 * 1024
    nonisolated static let timeout: TimeInterval = 15

    /// Mosaics survive a page reload and a switch of look; keyed by original and columns.
    private static let sharedCache: NSCache<NSString, CachedMosaic> = {
        let cache = NSCache<NSString, CachedMosaic>()
        cache.totalCostLimit = 16 << 20
        return cache
    }()

    /// What a failure answers: one black cell, hidden by the stylesheet once the box is marked.
    nonisolated static let failurePNG = MosaicRenderer.png(
        MosaicImage(columns: 1, rows: 1, cells: [MosaicCell(foreground: .black, background: .black, mask: 0)]))

    /// An `NSObject`, so that a test can hand the handler an `NSCache` subclass that never evicts.
    final class CachedMosaic: NSObject {
        let result: Result<Data, MosaicFailure>
        init(_ result: Result<Data, MosaicFailure>) { self.result = result }
    }

    private struct Running {
        let token: Int
        let work: Task<Void, Never>
    }

    private let fetcher: MosaicFetcher
    private let cache: NSCache<NSString, CachedMosaic>
    private var running: [ObjectIdentifier: Running] = [:]
    private var nextToken = 0

    init(fetcher: MosaicFetcher = .shared, cache: NSCache<NSString, CachedMosaic>? = nil) {
        self.fetcher = fetcher
        self.cache = cache ?? Self.sharedCache
    }

    /// How many loads are in flight (tests).
    var runningCount: Int { running.count }

    func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(URLError(.badURL))
            return
        }
        guard let request = MosaicRequest(url) else {
            return respond(.failure(.refused), url: url, to: urlSchemeTask, in: webView)
        }
        if let cached = cache.object(forKey: request.cacheKey) {
            return respond(cached.result, url: url, to: urlSchemeTask, in: webView)
        }
        let key = ObjectIdentifier(urlSchemeTask)
        nextToken += 1
        let token = nextToken
        let fetcher = fetcher
        let work = Task {
            let result = await Self.mosaic(for: request, fetcher: fetcher)
            // Stopped (or replaced): WebKit throws if a stopped task is called back.
            guard running[key]?.token == token else { return }
            running[key] = nil
            if result != .failure(.network) {
                cache.setObject(CachedMosaic(result), forKey: request.cacheKey, cost: (try? result.get().count) ?? 64)
            }
            respond(result, url: url, to: urlSchemeTask, in: webView)
        }
        running[key] = Running(token: token, work: work)
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) {
        running.removeValue(forKey: ObjectIdentifier(urlSchemeTask))?.work.cancel()
    }

    /// The mosaic PNG for `request`, computed off the main actor.
    @concurrent nonisolated static func mosaic(for request: MosaicRequest, fetcher: MosaicFetcher) async -> Result<Data, MosaicFailure> {
        let data: Data
        do {
            data = try await fetcher.fetch(request.original)
        } catch {
            return .failure(error)
        }
        return MosaicDecoder.thumbnail(of: data, maxColumns: request.columns).map { thumbnail in
            MosaicRenderer.png(MosaicRenderer.mosaic(from: thumbnail.image, maxColumns: thumbnail.columns))
        }
    }

    private func respond(_ result: Result<Data, MosaicFailure>, url: URL, to task: any WKURLSchemeTask, in webView: WKWebView) {
        let body: Data
        switch result {
        case .success(let png):
            body = png
        case .failure:
            body = Self.failurePNG
            markFailed(url, in: webView)
        }
        let headers = ["Content-Type": "image/png", "Content-Length": String(body.count), "Cache-Control": "no-store"]
        task.didReceive(HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers)
            ?? URLResponse(url: url, mimeType: "image/png", expectedContentLength: body.count, textEncodingName: nil))
        task.didReceive(body)
        task.didFinish()
    }

    /// Flags every box showing `url`, through app JavaScript (the page's own stays off).
    private func markFailed(_ url: URL, in webView: WKWebView) {
        let script = """
        for (const box of document.querySelectorAll(".mosaic")) {
          const image = box.querySelector("img");
          if (box.dataset.mosaic === source || (image && image.src === source)) { box.dataset.failed = ""; }
        }
        """
        Task { _ = try? await webView.callAsyncJavaScript(script, arguments: ["source": url.absoluteString], contentWorld: .page) }
    }
}

/// An `NSCache` that never evicts, for the one test that needs an entry to stay.
final class KeepingCache: NSCache<NSString, MosaicSchemeHandler.CachedMosaic>, @unchecked Sendable {
    private var kept: [NSString: MosaicSchemeHandler.CachedMosaic] = [:]

    override func object(forKey key: NSString) -> MosaicSchemeHandler.CachedMosaic? { kept[key] }
    override func setObject(_ obj: MosaicSchemeHandler.CachedMosaic, forKey key: NSString) { kept[key] = obj }
    override func setObject(_ obj: MosaicSchemeHandler.CachedMosaic, forKey key: NSString, cost g: Int) { kept[key] = obj }
}
```

- [ ] **Step 4: Run green.** Same command. Expected: `MosaicRequestTests` (2), `MosaicFetchTests` (7) and `MosaicSchemeHandlerTests` (4) pass, in about 2 s.

- [ ] **Step 5: Verification gate (phase 11a).**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: every command succeeds, `** TEST SUCCEEDED **`, zero warnings.

- [ ] **Step 6: Commit and push.**

```bash
git add apps/macos/Ressac/Reading/MosaicSchemeHandler.swift apps/macos/RessacTests/MosaicTests.swift
git commit -m "Mosaic scheme handler: capped fetch, ImageIO thumbnail, cache, safe stop

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Stop here for Clément's approval before phase 11b.**

---

# Phase 11b

### Task 11b.1: Mosaic boxes in the sanitiser, the CSP, the font pixel and the toggle link

**Files:**
- Modify: `apps/macos/Ressac/Reading/ArticleTemplate.swift`
- Test: `apps/macos/RessacTests/MosaicTests.swift`

**Interfaces:**
- Consumes: `MosaicRequest.url(for:columns:)`, `MosaicRenderer.maxColumns`, `MosaicSchemeHandler.scheme` (11a); `ReaderTheme` (10a).
- Produces: `sanitize(_:baseURL:budget:mosaics:)`; `LinkPolicy.Decision.toggleImage(Int)`, `LinkPolicy.toggleScheme`, `LinkPolicy.toggleIndex(_:)`; in Antiope pages, `--font-pixel` and mosaic boxes.

**Markup of image `n` (Antiope only):**

```html
<a class="mosaic" href="ressac-toggle:n" id="mn" data-original="https://…/a.png" data-mosaic="ressac-mosaic:image?cols=40&amp;src=…"><img src="ressac-mosaic:image?cols=40&amp;src=…" alt="…" loading="lazy"><span class="mosaic-alt">IMAGE …</span></a>
```

- Only http(s) images become mosaics; `data:` images stay as they are (the handler fetches http and https only). `width` and `height` are dropped from mosaics (the PNG has its own size).
- Inside a feed link the box is a `span` with the same `id` and data attributes and no toggle link.
- Images are numbered from 0 in document order by a small `Renderer` value that replaces the recursive `render` function (the same rules, plus the image counter and whether the walk is inside an `a`). The sanitiser's memo is keyed by `mosaics` too. Ids, classes and `data-` attributes from feeds never survive the sanitiser, so `m<n>`, `mosaic` and the data attributes are always ours.
- The page: `sanitize(…, mosaics: theme is Antiope)`; the CSP's `img-src` gains `ressac-mosaic:` (in both looks; harmless in Native, where no such address is written); Antiope's inline `:root` gains `--font-pixel: <size / 10>` (`1.5`, `2`, `2.5`, `3`), formatted with `%g`, which ignores the user's locale.
- `LinkPolicy.decide`: a `.linkActivated` navigation to `ressac-toggle:<n>` (n ≥ 0) is `.toggleImage(n)`; any other use of the scheme is cancelled.

- [ ] **Step 1: Write the failing tests.** Append to `MosaicTests.swift`:

```swift
// MARK: Reader wiring (phase 11b)

private func mosaicEntry(_ html: String) -> EntryRow {
    EntryRow(entryId: 1, feedId: 7, feedTitle: "Example Blog", folderId: nil, title: "Pictures", author: nil,
             url: "https://blog.example.org/post.html", publishedAt: Date(timeIntervalSince1970: 1_791_619_200),
             isRead: false, isStarred: false, hasFulltext: false, summaryHtml: nil, contentHtml: html)
}

@Suite struct MosaicMarkupTests {
    @Test func imagesBecomeNumberedMosaicBoxes() {
        let html = #"<p><img src="https://img.example.org/a.png" alt="A &quot;cat&quot;" width="800"></p><img src="/b.jpg"><img src="data:image/png;base64,AAAA">"#
        let clean = ArticleTemplate.sanitize(html, baseURL: URL(string: "https://blog.example.org/post.html"), mosaics: true)
        let a = MosaicRequest.url(for: URL(string: "https://img.example.org/a.png")!, columns: 40).replacingOccurrences(of: "&", with: "&amp;")
        let b = MosaicRequest.url(for: URL(string: "https://blog.example.org/b.jpg")!, columns: 40).replacingOccurrences(of: "&", with: "&amp;")
        #expect(clean.contains(#"<a class="mosaic" href="ressac-toggle:0" id="m0" data-original="https://img.example.org/a.png" data-mosaic="\#(a)"><img src="\#(a)" alt="A &quot;cat&quot;" loading="lazy"><span class="mosaic-alt">IMAGE A "cat"</span></a>"#))
        #expect(clean.contains(#"<a class="mosaic" href="ressac-toggle:1" id="m1" data-original="https://blog.example.org/b.jpg" data-mosaic="\#(b)"><img src="\#(b)" alt="" loading="lazy"><span class="mosaic-alt">IMAGE</span></a>"#))
        #expect(clean.contains(#"<img src="data:image/png;base64,AAAA" loading="lazy">"#))  // inline data stays as it is
        #expect(!clean.contains("width="))
        // Without mosaics (Native) the same input keeps its plain images.
        #expect(ArticleTemplate.sanitize(html, baseURL: URL(string: "https://blog.example.org/post.html"))
            .contains(#"<img src="https://img.example.org/a.png" alt="A &quot;cat&quot;" width="800" loading="lazy">"#))
    }

    @Test func anImageInsideALinkKeepsTheLink() {
        let clean = ArticleTemplate.sanitize(#"<a href="https://example.org/big.jpg"><img src="https://example.org/small.jpg"></a>"#, mosaics: true)
        #expect(clean.hasPrefix(#"<a href="https://example.org/big.jpg"><span class="mosaic" id="m0" data-original="https://example.org/small.jpg""#))
        #expect(!clean.contains("ressac-toggle"))
    }

    @Test func theToggleLinkAndThePolicy() {
        #expect(LinkPolicy.decide(type: .linkActivated, url: URL(string: "ressac-toggle:3")) == .toggleImage(3))
        #expect(LinkPolicy.decide(type: .linkActivated, url: URL(string: "ressac-toggle:x")) == .cancel)
        #expect(LinkPolicy.decide(type: .linkActivated, url: URL(string: "ressac-toggle:-1")) == .cancel)
        #expect(LinkPolicy.decide(type: .other, url: URL(string: "ressac-toggle:3")) == .cancel)
        let page = ArticleTemplate.page(for: mosaicEntry("<p>x</p>"), fulltext: nil, showFull: false, style: ReaderStyle(),
                                        colorScheme: .dark, theme: ReaderTheme(appearance: .antiope, crt: false))
        #expect(page.contains("img-src http: https: data: ressac-mosaic:;"))
        #expect(page.contains("--font-pixel: 1.5;"))  // 17 pt is drawn at 15
        let large = ArticleTemplate.page(for: mosaicEntry("<p>x</p>"), fulltext: nil, showFull: false, style: ReaderStyle(size: 25),
                                         colorScheme: .dark, theme: ReaderTheme(appearance: .antiope, crt: false))
        #expect(large.contains("--font-pixel: 2.5;"))
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/MosaicMarkupTests`. Expected: build failure, `extra argument 'mosaics' in call` and `type 'LinkPolicy.Decision' has no member 'toggleImage'`.

- [ ] **Step 3: Implement.** In `ArticleTemplate.swift` (re-read the file first: phase 10b changed it):

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 1.** Replace:

```swift
    /// address), since `<base>` is removed; links and images keep only http(s) addresses
    /// (images may also be `data:image/…`, except SVG).
    static func sanitize(_ html: String, baseURL: URL? = nil, budget: TimeInterval = parseBudget) -> String {
        let key = CacheKey(html: html, baseURL: baseURL)
        if let cached = cache.withLock({ $0.lookup(key) }) { return cached }
        let (result, timedOut) = sanitizeUncached(html, baseURL: baseURL, budget: budget)
        // A timed-out parse is a busy moment, not a property of the article: do not remember it.
        if html.utf8.count <= maxParsedBytes, !timedOut { cache.withLock { $0.store(key, result) } }
        return result
    }

    private static func sanitizeUncached(_ html: String, baseURL: URL?, budget: TimeInterval) -> (html: String, timedOut: Bool) {
```

with:

```swift
    /// address), since `<base>` is removed; links and images keep only http(s) addresses
    /// (images may also be `data:image/…`, except SVG). With `mosaics` (Antiope, spec §7), each
    /// http(s) image becomes a numbered mosaic box: a `ressac-mosaic:` image that a click on its
    /// `ressac-toggle:<n>` link, or `i`, swaps for the original.
    static func sanitize(_ html: String, baseURL: URL? = nil, budget: TimeInterval = parseBudget, mosaics: Bool = false) -> String {
        let key = CacheKey(html: html, baseURL: baseURL, mosaics: mosaics)
        if let cached = cache.withLock({ $0.lookup(key) }) { return cached }
        let (result, timedOut) = sanitizeUncached(html, baseURL: baseURL, budget: budget, mosaics: mosaics)
        // A timed-out parse is a busy moment, not a property of the article: do not remember it.
        if html.utf8.count <= maxParsedBytes, !timedOut { cache.withLock { $0.store(key, result) } }
        return result
    }

    private static func sanitizeUncached(_ html: String, baseURL: URL?, budget: TimeInterval, mosaics: Bool) -> (html: String, timedOut: Bool) {
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 2.** Replace:

```swift
        }
        if let rendered = onLargeStack(timeout: budget, { parseAndRender(html, baseURL: baseURL) }) {
```

with:

```swift
        }
        if let rendered = onLargeStack(timeout: budget, { parseAndRender(html, baseURL: baseURL, mosaics: mosaics) }) {
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 3.** Replace:

```swift
        let baseURL: URL?
```

with:

```swift
        let baseURL: URL?
        let mosaics: Bool
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 4.** Replace:

```swift

    private static func parseAndRender(_ html: String, baseURL: URL?) -> String {
        let wrapped = "<html><body>\(html)</body></html>"
        guard let document = try? XMLDocument(xmlString: wrapped, options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever]),
              let body = document.rootElement()?.children?.first(where: { name(of: $0) == "body" })
        else {
            return "<p>\(escapeText(html))</p>"
        }
        var out = ""
        for child in body.children ?? [] {
            render(child, baseURL: baseURL, depth: 1, into: &out)
        }
        return out
    }
```

with:

```swift

    private static func parseAndRender(_ html: String, baseURL: URL?, mosaics: Bool) -> String {
        let wrapped = "<html><body>\(html)</body></html>"
        guard let document = try? XMLDocument(xmlString: wrapped, options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever]),
              let body = document.rootElement()?.children?.first(where: { name(of: $0) == "body" })
        else {
            return "<p>\(escapeText(html))</p>"
        }
        var renderer = Renderer(baseURL: baseURL, mosaics: mosaics)
        for child in body.children ?? [] {
            renderer.render(child, depth: 1, inLink: false)
        }
        return renderer.out
    }
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 5.** Replace:

```swift

    private static func render(_ node: XMLNode, baseURL: URL?, depth: Int, into out: inout String) {
        if depth > maxNestingDepth { return }
        switch node.kind {
        case .text:
            out += escapeText(node.stringValue ?? "")
        case .element:
            guard let element = node as? XMLElement else { return }
            let tag = name(of: element)
            if droppedElements.contains(tag) { return }
            guard allowedElements.contains(tag) else {
                for child in element.children ?? [] { render(child, baseURL: baseURL, depth: depth + 1, into: &out) }
                return
            }
            let attributes = safeAttributes(of: element, tag: tag, baseURL: baseURL)
            if tag == "img" && !attributes.contains(where: { $0.name == "src" }) { return }
            out += "<\(tag)"
            for attribute in attributes {
                out += " \(attribute.name)=\"\(escapeAttribute(attribute.value))\""
            }
            // Images below the fold wait until they are scrolled near.
            if tag == "img" { out += #" loading="lazy""# }
            out += ">"
            guard !voidTags.contains(tag) else { return }
            for child in element.children ?? [] { render(child, baseURL: baseURL, depth: depth + 1, into: &out) }
            out += "</\(tag)>"
        default:
            // Comments, processing instructions, DTD nodes: dropped.
            return
        }
```

with:

```swift

    /// Writes the allowed tree back out with our own serialiser, numbering images as it goes.
    private struct Renderer {
        let baseURL: URL?
        let mosaics: Bool
        var out = ""
        private var images = 0

        init(baseURL: URL?, mosaics: Bool) {
            self.baseURL = baseURL
            self.mosaics = mosaics
        }

        /// `inLink`: inside an `a`, where a mosaic cannot carry its own toggle link (anchors do not nest).
        mutating func render(_ node: XMLNode, depth: Int, inLink: Bool) {
            if depth > maxNestingDepth { return }
            switch node.kind {
            case .text:
                out += escapeText(node.stringValue ?? "")
            case .element:
                guard let element = node as? XMLElement else { return }
                let tag = name(of: element)
                if droppedElements.contains(tag) { return }
                guard allowedElements.contains(tag) else {
                    for child in element.children ?? [] { render(child, depth: depth + 1, inLink: inLink) }
                    return
                }
                let attributes = safeAttributes(of: element, tag: tag, baseURL: baseURL)
                if tag == "img" {
                    guard let source = attributes.first(where: { $0.name == "src" })?.value else { return }
                    if mosaics, let original = LinkPolicy.webURL(source) {
                        let alt = attributes.first(where: { $0.name == "alt" })?.value ?? ""
                        return mosaic(original: original, alt: alt, inLink: inLink)
                    }
                }
                out += "<\(tag)"
                for attribute in attributes {
                    out += " \(attribute.name)=\"\(escapeAttribute(attribute.value))\""
                }
                // Images below the fold wait until they are scrolled near.
                if tag == "img" { out += #" loading="lazy""# }
                out += ">"
                guard !voidTags.contains(tag) else { return }
                for child in element.children ?? [] { render(child, depth: depth + 1, inLink: inLink || tag == "a") }
                out += "</\(tag)>"
            default:
                // Comments, processing instructions, DTD nodes: dropped.
                return
            }
        }

        /// Image `n` as a mosaic box. The box keeps both addresses for the swap and an IMAGE line
        /// with the alt text, shown by the stylesheet when the mosaic fails (`data-failed`).
        private mutating func mosaic(original: URL, alt: String, inLink: Bool) {
            let index = images
            images += 1
            let source = escapeAttribute(MosaicRequest.url(for: original, columns: MosaicRenderer.maxColumns))
            let box = #"id="m\#(index)" data-original="\#(escapeAttribute(original.absoluteString))" data-mosaic="\#(source)""#
            let label = escapeText(alt.isEmpty ? "IMAGE" : "IMAGE \(alt)")
            let content = #"<img src="\#(source)" alt="\#(escapeAttribute(alt))" loading="lazy">"#
                + #"<span class="mosaic-alt">\#(label)</span>"#
            out += inLink
                ? #"<span class="mosaic" \#(box)>\#(content)</span>"#
                : #"<a class="mosaic" href="\#(LinkPolicy.toggleScheme):\#(index)" \#(box)>\#(content)</a>"#
        }
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 6.** Replace:

```swift
    // MARK: Page

    /// No scripts, frames, objects, forms or base URL; images from the web; fonts and styles inline.
    private static let contentSecurityPolicy =
        "default-src 'none'; img-src http: https: data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"

```

with:

```swift
    // MARK: Page

    /// No scripts, frames, objects, forms or base URL; images from the web and the mosaic handler;
    /// fonts and styles inline.
    private static let contentSecurityPolicy =
        "default-src 'none'; img-src http: https: data: \(MosaicSchemeHandler.scheme):; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"

```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 7.** Replace:

```swift
            .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? ""
        let body = sanitize(source, baseURL: base)
```

with:

```swift
            .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? ""
        let body = sanitize(source, baseURL: base, mosaics: antiope)
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 8.** Replace:

```swift
        \(readerCSS)
        :root { --reader-font: \(style.font.cssFamily); --reader-size: \(Int(size))px; --reader-measure: \(style.width.maxCh)ch;\(antiope ? teletextVariables : "") }
```

with:

```swift
        \(readerCSS)
        :root { --reader-font: \(style.font.cssFamily); --reader-size: \(Int(size))px; --reader-measure: \(style.width.maxCh)ch;\(antiope ? teletextVariables(size: size) : "") }
```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 9.** Replace:

```swift
    }

    /// Antiope's extra custom properties: the teletext font for every role of `reader.css`.
    private static let teletextVariables = #" --teletext-font: "\#(TeletextFont.familyName)", monospace;"#

```

with:

```swift
    }

    /// Antiope's extra custom properties: the teletext font for every role of `reader.css`, and
    /// the font pixel in CSS pixels, by which mosaics (one PNG pixel per font pixel) are zoomed.
    private static func teletextVariables(size: Double) -> String {
        #" --teletext-font: "\#(TeletextFont.familyName)", monospace; --font-pixel: \#(String(format: "%g", size / 10));"#
    }

```

**`apps/macos/Ressac/Reading/ArticleTemplate.swift`, edit 10.** Replace:

```swift
/// it is loaded without a base URL) is shown; a clicked http(s) link opens in the default
/// browser; everything else (redirects, form posts, other schemes) is cancelled.
enum LinkPolicy {
    enum Decision: Equatable {
        case allow
        case cancel
        case openExternally(URL)
    }

    static func decide(type: WKNavigationType, url: URL?) -> Decision {
        switch type {
        case .other where url?.absoluteString == "about:blank":
            return .allow
        case .linkActivated:
            return webURL(url?.absoluteString).map(Decision.openExternally) ?? .cancel
        default:
            return .cancel
        }
```

with:

```swift
/// it is loaded without a base URL) is shown; a clicked http(s) link opens in the default
/// browser; a clicked mosaic (`ressac-toggle:<n>`) swaps image `n`; everything else (redirects,
/// form posts, other schemes) is cancelled.
enum LinkPolicy {
    enum Decision: Equatable {
        case allow
        case cancel
        case openExternally(URL)
        case toggleImage(Int)
    }

    static let toggleScheme = "ressac-toggle"

    static func decide(type: WKNavigationType, url: URL?) -> Decision {
        switch type {
        case .other where url?.absoluteString == "about:blank":
            return .allow
        case .linkActivated:
            if let index = toggleIndex(url) { return .toggleImage(index) }
            return webURL(url?.absoluteString).map(Decision.openExternally) ?? .cancel
        default:
            return .cancel
        }
    }

    /// `n` of `ressac-toggle:<n>`, or nil for any other address.
    static func toggleIndex(_ url: URL?) -> Int? {
        guard let url, url.scheme?.lowercased() == toggleScheme,
              let index = Int(url.absoluteString.dropFirst(toggleScheme.count + 1)), index >= 0
        else { return nil }
        return index
```

- [ ] **Step 4: Run green.** Same command. Expected: `MosaicMarkupTests`, 3 tests pass. Then `-only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/LinkPolicyTests -only-testing:RessacTests/ArticlePageTests -only-testing:RessacTests/AntiopePageTests -only-testing:RessacTests/FixWaveTests`: all pass unchanged.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/MosaicTests.swift
git commit -m "Sanitiser: numbered mosaic boxes in Antiope, ressac-mosaic in the CSP, toggle links

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 11b.2: The reader shows mosaics and swaps them

**Files:**
- Modify: `apps/macos/Ressac/Resources/reader.css`
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift`
- Test: `apps/macos/RessacTests/MosaicTests.swift`

**Interfaces:**
- Consumes: `MosaicSchemeHandler` (11a); the markup and `LinkPolicy.Decision.toggleImage` (11b.1); `AntiopeState.imageToggle` (phase 7).
- Produces: `ArticleWebView.configuration(mosaics:)` (the web view configuration, shared by `makeNSView` and the tests); `ReaderController.toggleImage(_:)` and `toggleAllImages()`.

**Stylesheet (inside the Antiope section, so the palette test covers it):** a box is a block as wide as its image (`fit-content`, at most the column), one row of space below; the mosaic image is zoomed by `--font-pixel` and drawn `pixelated`, on a line height of exactly 1 em (spec §7); a box with class `original` shows the original at its own size; a `data-failed` box that is not showing the original hides its image and shows the `IMAGE` line, black on magenta, half a cell of padding each side.

**Reader:**
- `makeNSView` uses `ArticleWebView.configuration()`, which adds `setURLSchemeHandler(MosaicSchemeHandler(), forURLScheme: "ressac-mosaic")` to today's settings (JavaScript off, non-persistent data, media on user action). Every handler shares the process-wide cache and fetcher.
- The coordinator answers `.toggleImage(n)` by cancelling the navigation and calling `controller.toggleImage(n)`.
- `toggleImage(n)` flips class `original` on box `m<n>` and sets its image's `src` to `data-original` or `data-mosaic`. `toggleAllImages()` (`i`) shows every original while any mosaic is showing, else every mosaic. Both are app JavaScript in the `.page` world; the article's own JavaScript stays off.
- `ReaderView.onAppear` sets `antiope?.imageToggle` to `controller.toggleAllImages()`, `onDisappear` clears it; `antiope` is the optional `AntiopeState` from the environment (absent in the Native window, so `nil` there). Phase 7's router already offers `i` to `AntiopeState.handle`, which calls the hook.

- [ ] **Step 1: Write the failing test.** Append to `MosaicTests.swift`:

```swift
/// End to end in a real web view: mosaics load through the handler, a failure shows its box,
/// a click on a mosaic and `i` swap the images (spec §7, manual check 4 in process).
@MainActor @Suite(.serialized) struct MosaicWebViewTests {
    @Test func mosaicsLoadFailAndSwap() async throws {
        StubProtocol.serve("/photo.png", .init(chunks: [encoded([flat(255, 0, 0, width: 120, height: 120)], as: .png)]))
        StubProtocol.serve("/broken.png", .init(chunks: [Data("<html>gone</html>".utf8)]))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 800, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
        let handler = MosaicSchemeHandler(fetcher: MosaicFetcher(configuration: StubProtocol.configuration()), cache: NSCache())
        let webView = WKWebView(frame: window.contentLayoutRect, configuration: ArticleWebView.configuration(mosaics: handler))
        window.contentView = webView
        let controller = ReaderController()
        controller.webView = webView
        let coordinator = ArticleWebView.Coordinator(controller: controller)
        webView.navigationDelegate = coordinator
        let html = #"<p><img src="https://stub.ressac.test/photo.png" alt="Photo"></p><p><img src="https://stub.ressac.test/broken.png" alt="Gone"></p>"#
        let page = ArticleTemplate.page(for: mosaicEntry(html), fulltext: nil, showFull: false, style: ReaderStyle(),
                                        colorScheme: .dark, theme: ReaderTheme(appearance: .antiope, crt: false))
        controller.load(entryId: 1, html: page)
        #expect(await eventually { controller.loadedEntryId == 1 })

        func js(_ script: String) async -> Any? { try? await webView.callAsyncJavaScript(script, contentWorld: .page) }
        #expect(await eventually(timeout: .seconds(5)) { handler.runningCount == 0 })
        var loaded: Int?
        for _ in 0..<100 where loaded != 60 {
            loaded = await js(#"return document.getElementById("m0").querySelector("img").naturalWidth"#) as? Int
            try await Task.sleep(for: .milliseconds(50))
        }
        #expect(loaded == 60)  // 10 columns of 6 font pixels
        var failed = false
        for _ in 0..<100 where !failed {
            failed = await js(#"return document.getElementById("m1").hasAttribute("data-failed")"#) as? Bool ?? false
            try await Task.sleep(for: .milliseconds(50))
        }
        #expect(failed)
        #expect(await js(#"return document.getElementById("m0").hasAttribute("data-failed")"#) as? Bool == false)
        // reader.css hides the placeholder image and shows the magenta IMAGE line instead.
        #expect(await js(#"return getComputedStyle(document.getElementById("m1").querySelector(".mosaic-alt")).display"#) as? String == "block")
        #expect(await js(#"return getComputedStyle(document.getElementById("m1").querySelector("img")).display"#) as? String == "none")
        #expect(await js(#"return getComputedStyle(document.getElementById("m0").querySelector(".mosaic-alt")).display"#) as? String == "none")

        // A click on the first mosaic goes through the navigation delegate (.linkActivated).
        _ = await js(#"document.getElementById("m0").click()"#)
        var source: String?
        for _ in 0..<100 where source != "https://stub.ressac.test/photo.png" {
            source = await js(#"return document.getElementById("m0").querySelector("img").getAttribute("src")"#) as? String
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(source == "https://stub.ressac.test/photo.png")
        #expect(await js(#"return document.getElementById("m0").classList.contains("original")"#) as? Bool == true)

        // `i`: one mosaic still shows, so every image goes to its original; then all back.
        #expect(await controller.toggleAllImages() == 2)
        #expect(await js(#"return document.querySelectorAll(".mosaic.original").length"#) as? Int == 2)
        #expect(await controller.toggleAllImages() == 2)
        #expect(await js(#"return document.querySelectorAll(".mosaic.original").length"#) as? Int == 0)
        #expect(await js(#"return document.getElementById("m1").querySelector("img").getAttribute("src")"#) as? String
            == MosaicRequest.url(for: URL(string: "https://stub.ressac.test/broken.png")!, columns: 40))
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/MosaicWebViewTests`. Expected: build failure, `type 'ArticleWebView' has no member 'configuration'` and `value of type 'ReaderController' has no member 'toggleAllImages'`.

- [ ] **Step 3: The stylesheet.** In `reader.css`, insert the mosaic rules just before the `Antiope end` comment:

**`apps/macos/Ressac/Resources/reader.css`, edit 1.** Replace:

```css
/* Antiope end. */
```

with:

```css
/*
 * Mosaics (spec §7). A box per image: the mosaic PNG has one pixel per font pixel and is zoomed
 * by --font-pixel (CSS pixels per font pixel), pixelated, on rows of exactly 1 em. A box marked
 * data-failed shows the magenta IMAGE line instead; .original shows the original image.
 */
:root[data-theme="antiope"] .mosaic {
  display: block;
  width: fit-content;
  max-width: 100%;
  margin: 0 0 var(--row);
  line-height: 1em;
}

:root[data-theme="antiope"] .mosaic img {
  display: block;
  max-width: 100%;
  height: auto;
  margin: 0;
  zoom: var(--font-pixel);
  image-rendering: pixelated;
}

:root[data-theme="antiope"] .mosaic.original img {
  zoom: 1;
  image-rendering: auto;
}

:root[data-theme="antiope"] .mosaic-alt {
  display: none;
}

:root[data-theme="antiope"] .mosaic[data-failed]:not(.original) img {
  display: none;
}

:root[data-theme="antiope"] .mosaic[data-failed]:not(.original) .mosaic-alt {
  display: block;
  padding: 0 var(--cell);
  background: var(--tt-magenta);
  color: var(--tt-black);
  line-height: var(--row);
}

/* Antiope end. */
```

- [ ] **Step 4: The reader.** In `ReaderView.swift` (re-read it first):

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 1.** Replace:

```swift
    @AppStorage(Appearance.Keys.crt) private var crt = false
```

with:

```swift
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @Environment(AntiopeState.self) private var antiope: AntiopeState?
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 2.** Replace:

```swift
        .toolbar { toolbarContent }
        .onAppear { state.readerPageDown = { [controller] id in await controller.pageDown(entryId: id) } }
        .onDisappear { state.readerPageDown = nil }
```

with:

```swift
        .toolbar { toolbarContent }
        .onAppear {
            state.readerPageDown = { [controller] id in await controller.pageDown(entryId: id) }
            antiope?.imageToggle = { [controller] in Task { await controller.toggleAllImages() } }
        }
        .onDisappear {
            state.readerPageDown = nil
            antiope?.imageToggle = nil
        }
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 3.** Replace:

```swift

    /// Rewrites the Antiope sub-page line of the page on screen, keeping its scroll position.
```

with:

```swift

    /// Swaps mosaic `index` and its original (a click on the mosaic, spec §7). Returns whether the
    /// original is now shown. App JavaScript; the page's own stays off.
    @discardableResult
    func toggleImage(_ index: Int) async -> Bool {
        guard let webView else { return false }
        let script = """
        const box = document.getElementById("m" + index);
        if (!box) { return false; }
        const original = !box.classList.contains("original");
        box.classList.toggle("original", original);
        box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic;
        return original;
        """
        let result = try? await webView.callAsyncJavaScript(script, arguments: ["index": index], contentWorld: .page)
        return (result as? Bool) ?? false
    }

    /// `i`: every image of the article both ways, originals while any mosaic shows, else mosaics.
    /// Returns how many images it swapped.
    @discardableResult
    func toggleAllImages() async -> Int {
        guard let webView else { return 0 }
        let script = """
        const boxes = Array.from(document.querySelectorAll(".mosaic"));
        const original = boxes.some((box) => !box.classList.contains("original"));
        for (const box of boxes) {
          box.classList.toggle("original", original);
          box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic;
        }
        return boxes.length;
        """
        let result = try? await webView.callAsyncJavaScript(script, contentWorld: .page)
        return (result as? Int) ?? 0
    }

    /// Rewrites the Antiope sub-page line of the page on screen, keeping its scroll position.
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 4.** Replace:

```swift
    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.websiteDataStore = .nonPersistent()
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        let webView = WKWebView(frame: .zero, configuration: configuration)
```

with:

```swift
    func makeNSView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero, configuration: Self.configuration())
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 5.** Replace:

```swift
        controller.load(entryId: page.entry.entryId, html: page.html())
    }
```

with:

```swift
        controller.load(entryId: page.entry.entryId, html: page.html())
    }

    /// JavaScript off, no persistent website data, and the mosaic handler (spec §7).
    static func configuration(mosaics: MosaicSchemeHandler = MosaicSchemeHandler()) -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.websiteDataStore = .nonPersistent()
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        configuration.setURLSchemeHandler(mosaics, forURLScheme: MosaicSchemeHandler.scheme)
        return configuration
    }
```

**`apps/macos/Ressac/Reading/ReaderView.swift`, edit 6.** Replace:

```swift
                NSWorkspace.shared.open(url)
```

with:

```swift
                NSWorkspace.shared.open(url)
            case .toggleImage(let index):
                decisionHandler(.cancel)
                Task { await controller.toggleImage(index) }
```

- [ ] **Step 5: Run green.** Same command. Expected: `MosaicWebViewTests` passes (about 1 s). Then the reader suites: `-only-testing:RessacTests/ReaderControllerTests -only-testing:RessacTests/ReaderSlowImageTests -only-testing:RessacTests/SubpageLineTests -only-testing:RessacTests/AntiopeStylesheetTests`: all pass (the palette test now also covers the mosaic rules).

- [ ] **Step 6: Verification gate (phase 11b).**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: every command succeeds, `** TEST SUCCEEDED **`, zero warnings.

- [ ] **Step 7: Commit and push.**

```bash
git add apps/macos/Ressac/Resources/reader.css apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/MosaicTests.swift
git commit -m "Reader: mosaics in Antiope, a click or i swaps them with the originals

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Stop here for Clément's approval before phase 12.**

## Notes for the executor

- **No simulated keystrokes.** `MosaicWebViewTests` drives the page with app JavaScript (`click()` on the box, the controller's toggles) inside the test process, which the project rule allows. Swapping on the real database is manual check 4 of spec §11, in phase 14.
- **Edit blocks** are applied in the order given, top to bottom. Project rule 9: re-read the file after every three edits to it, and before the first (ten blocks for `ArticleTemplate.swift` in 11b.1).
- **Line counts after this phase:** `ArticleTemplate.swift` about 545 lines (read it in chunks), `ReaderView.swift` about 350, `reader.css` about 530, `MosaicSchemeHandler.swift` about 320, `MosaicRenderer.swift` about 180.
- **Teletext colours only.** The mosaic has no dithering: a photograph becomes broad flat areas of the eight colours, as on a real set. Dithering would be a later, separate choice.
- **Second download.** Showing the original makes WebKit fetch it again (spec §13, accepted); the mosaic fetch goes through `URLSession` with no cookies and no cache, so it reveals no more to the image host than the native reader already does.
- **Lazy loading.** Mosaic images keep `loading="lazy"`: a long article only fetches the mosaics near the screen.
- **`NSCache` may evict at any moment** (memory pressure, other tests), even right after `setObject`: never write a test that expects an entry of a real `NSCache` to survive. `aCachedMosaicIsAnsweredAtOnce` hands the handler a `KeepingCache` (an `NSCache` subclass that never evicts, hence `CachedMosaic: NSObject`) for that reason: filling a real cache by hand still failed one full-suite run in three during the reconciliation of 14/10.
- **A failure and a page change.** If the article changes between a fetch's start and its failure, the `data-failed` mark runs on the new page and finds nothing: harmless. The same image in the new page is served from the cache and marked again.
- **`@concurrent`** needs a Swift 6.2 or later compiler (6.4 here); it keeps `mosaic(for:fetcher:)` off the main actor whatever the project's default isolation becomes.
