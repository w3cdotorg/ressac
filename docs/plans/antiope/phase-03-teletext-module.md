# Phase 3: The `Teletext/` module (Bedstead, palette, font metrics, cell text)

**Runs after:** phase 2.

Read the plan index first. Global Constraints apply. Spec §2 (Font, Layout, Language), §5 (`Teletext/` module, Settings), §8 (Accessibility), §10 (Bedstead missing) and §11 (`CellText` tests). Review Focus #1 is pinned here.

**Execution order (read before starting).** Phase 1 (1a, then 1b) regenerates `tests/fixtures/contract.sqlite` at schema v3, while the app still has `AppDatabase.knownSchemaVersion = 2` until phase 6. From the end of phase 1a, every Swift test that checks or launches a copy of the fixture gets `.tooNew(3)`: `DatabaseTests.check_reports_missing_and_current_databases`, `LauncherTests.v1_database_is_migrated_before_opening`, `LauncherTests.missing_database_is_created_by_migrate`, `FirstRunWALTests.aClosedWALDatabaseChecksAndOpens`, `FirstRunWALTests.theFirstRunOpensWhatMigrateCreated` and `FirstRunWALTests.theReadingConnectionsRefuseWrites`. Phase 6 depends only on phase 1b and fixes them, so the order is **1a, 1b, 6, 2, then phases 3, 4 and 5**. This phase's verification gate cannot be green before phase 6 has landed.

**Files (5):**
- Create: `apps/macos/Ressac/Teletext/TeletextColor.swift`
- Create: `apps/macos/Ressac/Teletext/TeletextFont.swift`
- Create: `apps/macos/Ressac/Teletext/CellText.swift`
- Create: `apps/macos/RessacTests/TeletextTests.swift`
- Modify: `apps/macos/Ressac/Settings/ReaderStyle.swift` (one key, `ReaderStyle.Keys.antiopeSize`, Task 3.3)
- Binary assets (not counted): `apps/macos/Ressac/Resources/Fonts/bedstead.otf`, `apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt`

**No `project.yml` change.** `Teletext/` sits under the `Ressac` source path, which XcodeGen walks with `createIntermediateGroups`. `Resources/Fonts` is already a folder reference copied to `Ressac.app/Contents/Resources/Fonts/`, and `ATSApplicationFontsPath = Fonts` (mac-app phase 11) registers every font file in it for the app process: a new `.otf` dropped in the folder is bundled and registered with nothing else to declare. The `.txt` next to it is ignored by the font loader, like the two `*-OFL.txt`.

**Tests see the font.** `RessacTests` runs inside `Ressac.app` (XcodeGen sets `TEST_HOST` from the target dependency), so `ATSApplicationFontsPath` applies to the test process too; `BundledFontTests` already relies on it for Newsreader.

**Step 0:** nothing to clean. Every Swift file of this phase is new except `ReaderStyle.swift` (129 lines, one key added), and no file over 300 lines is touched.

**Test command** (narrow with the suite of the task):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/BedsteadBundleTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**Interface notes.** The names and signatures are exactly those of the plan index (Phase 3). Two behaviours the index states in one line are made precise here, without changing any signature:
- `TeletextFont.supports(_:)` tests the grapheme's **composed form (NFC)**: Bedstead has `é` (U+00E9) but not the combining acute U+0301 alone, so a decomposed `e` + U+0301 is supported as `é`.
- `CellText.fit` writes that composed form into its runs, draws any whitespace (tab, line break, no-break space) as one space, and drops graphemes made only of invisible format or control characters (zero-width space, BOM, soft hyphen). Every other grapheme takes exactly one cell, so every cell holds exactly one Unicode scalar.

### Task 3.1: Bundle Bedstead and its CC0 dedication

**Interfaces:**
- Consumes: `ATSApplicationFontsPath = Fonts` and the `Resources/Fonts` folder reference (mac-app phase 11).
- Produces: `Ressac.app/Contents/Resources/Fonts/bedstead.otf` (Bedstead 3.261, family and PostScript name `Bedstead`) and `Bedstead-CC0.txt`, the upstream `COPYING` renamed so it cannot collide with another licence in the bundle.

Bedstead is by Ben Harris, Simon Tatham, Marnanel Thurman and Neil Williamson. The header of its source, `bedstead.c`, dedicates the program and the typeface to the public domain under CC0 1.0 and points to `COPYING`, the CC0 legal code. Both files come from the author's site; the hashes pin version 3.261 (name table: `Version 3.261`, built 7 February 2026).

- [ ] **Step 1: Write the failing test.** Create `apps/macos/RessacTests/TeletextTests.swift`:

```swift
import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Ressac

/// Bedstead's design: one advance of 600/1000 em, ascent 800, descent 200, no line gap.
@Suite struct BedsteadBundleTests {
    @Test func bedsteadShipsWithItsDedication() throws {
        let font = try #require(NSFont(name: "Bedstead", size: 20), "ATSApplicationFontsPath did not register Bedstead")
        #expect(font.familyName == "Bedstead")
        #expect(font.isFixedPitch)
        #expect(Bundle.main.url(forResource: "bedstead", withExtension: "otf", subdirectory: "Fonts") != nil)
        #expect(Bundle.main.url(forResource: "Bedstead-CC0", withExtension: "txt", subdirectory: "Fonts") != nil)
    }

    @Test(arguments: [15.0, 20.0, 25.0, 30.0])
    func everyGlyphTakesSixTenthsOfAnEm(size: Double) throws {
        let font = try #require(NSFont(name: "Bedstead", size: size))
        let row = "MMMMMiiiii«é€▐🬀"  // 15 characters, mosaics included
        // As Double: inside #expect, comparing a CGFloat with this Double failed (see Notes).
        let width = Double((row as NSString).size(withAttributes: [.font: font]).width)
        #expect(width == size * 6 / 10 * 15)
        #expect(Double(NSLayoutManager().defaultLineHeight(for: font)) == size)
        #expect(abs(font.ascender - size * 0.8) < 0.01)
        #expect(abs(font.descender + size * 0.2) < 0.01)
        #expect(font.leading == 0)
    }
}
```

- [ ] **Step 2: Run red.** The test command above (`-only-testing:RessacTests/BedsteadBundleTests`). Expected: `✘` on both tests, with `ATSApplicationFontsPath did not register Bedstead`, then `** TEST FAILED **`. (If Bedstead happens to be installed system-wide on the Mac, `NSFont` finds it and only the two `Bundle.main.url` expectations fail; that still proves the bundle lacks the files.)

- [ ] **Step 3: Download the font and its dedication, and check them.** From the repository root:

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
FONTS=apps/macos/Ressac/Resources/Fonts
curl -fsSL --retry 4 --retry-all-errors --connect-timeout 30 -o "$FONTS/bedstead.otf" https://bjh21.me.uk/bedstead/bedstead.otf
curl -fsSL --retry 4 --retry-all-errors --connect-timeout 30 -o "$FONTS/Bedstead-CC0.txt" https://bjh21.me.uk/bedstead/COPYING
(cd "$FONTS" && shasum -a 256 -c <<'EOF'
9473c5671c703d26e728a8187f8d30ca7a19497ee17cc5b076bd0fb6e2986007  bedstead.otf
a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499  Bedstead-CC0.txt
EOF
)
ls -l "$FONTS/bedstead.otf" "$FONTS/Bedstead-CC0.txt"
head -3 "$FONTS/Bedstead-CC0.txt"
```

Expected: `bedstead.otf: OK` and `Bedstead-CC0.txt: OK`; sizes 218 300 and 7 048 bytes; the text starts with `Creative Commons Legal Code`, a blank line, then `CC0 1.0 Universal`. Both URLs answered `200` on 2026-10-14 (`Content-Type: font/otf; outlines=cff`, `Last-Modified: Sat, 07 Feb 2026`), but the server once timed out on a `GET` that day: `--retry` covers that. On this Mac, `.superpowers/brainstorm/53559-1791963931/content/bedstead.otf` (not committed) is the same file, same SHA-256, and can be copied instead if the site stays down. **If a hash does not match, stop:** upstream has published a new version; do not update the hash without Clément's approval.

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: `✔ Test run with 2 tests in 1 suite passed` and `** TEST SUCCEEDED **`. Then check the product:

```bash
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
ls "$APP/Contents/Resources/Fonts"
```

Expected: eight files, `AtkinsonHyperlegibleNext-Italic-Variable.ttf`, `AtkinsonHyperlegibleNext-OFL.txt`, `AtkinsonHyperlegibleNext-Variable.ttf`, `Bedstead-CC0.txt`, `Newsreader-Italic-Variable.ttf`, `Newsreader-OFL.txt`, `Newsreader-Variable.ttf`, `bedstead.otf`. Xcode does not always re-copy a folder reference whose contents changed on an incremental build: if `bedstead.otf` is missing, run `xcodebuild -project Ressac.xcodeproj -scheme Ressac clean` once in `apps/macos`, then Step 4 again.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Resources/Fonts/bedstead.otf apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt apps/macos/RessacTests/TeletextTests.swift
git commit -F - <<'EOF'
macOS app: bundle the Bedstead teletext font (CC0) with its dedication

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3.2: `TeletextColor`, the eight teletext colours

**Interfaces:**
- Consumes: nothing.
- Produces (`Teletext/TeletextColor.swift`): `enum TeletextColor: String, CaseIterable, Sendable` with the cases `black, red, green, yellow, blue, magenta, cyan, white` in that order, `var rgb: (r: UInt8, g: UInt8, b: UInt8)`, `var color: Color` (SwiftUI, sRGB), `var nsColor: NSColor` (sRGB) and `var css: String` (`"#FF0000"`, upper case).

- [ ] **Step 1: Write the failing test.** Append to `apps/macos/RessacTests/TeletextTests.swift`:

```swift
@Suite struct TeletextColorTests {
    @Test func theEightColoursArePure() {
        #expect(TeletextColor.allCases == [.black, .red, .green, .yellow, .blue, .magenta, .cyan, .white])
        #expect(TeletextColor.allCases.map(\.css) == [
            "#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF",
        ])
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            #expect([r, g, b].allSatisfy { $0 == 0 || $0 == 0xFF }, "\(colour)")
        }
        #expect(TeletextColor(rawValue: "magenta") == .magenta)
    }

    @Test func swiftUIAndAppKitColoursMatchTheTriplet() throws {
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            let expected = [Double(r), Double(g), Double(b)].map { $0 / 255 }
            let ns = try #require(colour.nsColor.usingColorSpace(.sRGB))
            #expect([ns.redComponent, ns.greenComponent, ns.blueComponent].map(Double.init) == expected, "\(colour)")
            #expect(ns.alphaComponent == 1)
            let resolved = colour.color.resolve(in: EnvironmentValues())
            #expect([resolved.red, resolved.green, resolved.blue].map(Double.init) == expected, "\(colour)")
            #expect(resolved.opacity == 1)
        }
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/TeletextColorTests`. Expected: `error: cannot find 'TeletextColor' in scope` and `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Teletext/TeletextColor.swift`:

```swift
import AppKit
import SwiftUI

/// The eight teletext colours (spec §5, `TeletextPalette`). No other colour appears in an
/// Antiope view, and blue never colours text: it is for separators, backgrounds and mosaics.
enum TeletextColor: String, CaseIterable, Sendable {
    case black, red, green, yellow, blue, magenta, cyan, white

    var rgb: (r: UInt8, g: UInt8, b: UInt8) {
        switch self {
        case .black: (0x00, 0x00, 0x00)
        case .red: (0xFF, 0x00, 0x00)
        case .green: (0x00, 0xFF, 0x00)
        case .yellow: (0xFF, 0xFF, 0x00)
        case .blue: (0x00, 0x00, 0xFF)
        case .magenta: (0xFF, 0x00, 0xFF)
        case .cyan: (0x00, 0xFF, 0xFF)
        case .white: (0xFF, 0xFF, 0xFF)
        }
    }

    /// In sRGB, so that the pure primaries stay pure on any display.
    var color: Color {
        Color(.sRGB, red: Self.unit(rgb.r), green: Self.unit(rgb.g), blue: Self.unit(rgb.b), opacity: 1)
    }

    var nsColor: NSColor {
        NSColor(srgbRed: Self.unit(rgb.r), green: Self.unit(rgb.g), blue: Self.unit(rgb.b), alpha: 1)
    }

    /// `#RRGGBB`, upper case, for `reader.css` (phase 10).
    var css: String {
        String(format: "#%02X%02X%02X", rgb.r, rgb.g, rgb.b)
    }

    private static func unit(_ component: UInt8) -> Double { Double(component) / 255 }
}
```

- [ ] **Step 4: Run green**, same filter. Expected: `✔ Test run with 2 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Teletext/TeletextColor.swift apps/macos/RessacTests/TeletextTests.swift
git commit -F - <<'EOF'
macOS app: the eight teletext colours

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3.3: `TeletextFont`, metrics, size steps and glyph coverage

**Interfaces:**
- Consumes: Bedstead registered for the process (Task 3.1).
- Produces (`Teletext/TeletextFont.swift`), exactly as the plan index:
  - `enum TeletextFont` with `static let familyName = "Bedstead"`, `static let fileName = "bedstead"`, `static let sizeSteps: [Double] = [15, 20, 25, 30]`, `static let defaultSize = 20.0`;
  - `static var isAvailable: Bool` (registered and resolvable through `NSFont`; logged once when not);
  - `static func font(size: Double) -> Font` (`.custom("Bedstead", fixedSize:)`);
  - `static func cell(forSize size: Double) -> CGSize` (`0.6 × size` by `size`), `static func pixel(forSize size: Double) -> Double` (`size / 10`);
  - `static func snapped(_ size: Double) -> Double` (nearest step, ties go up, non-finite gives `defaultSize`);
  - `static func step(_ size: Double, by delta: Int) -> Double` (from the snapped step, clamped to 15...30);
  - `static func supports(_ grapheme: Character) -> Bool` (NFC form, `CTFontCopyCharacterSet`);
  - `extension EnvironmentValues { @Entry var teletextSize: Double = TeletextFont.defaultSize }`, in the same file;
  - `ReaderStyle.Keys.antiopeSize = "reader.size.antiope"` (`Settings/ReaderStyle.swift`): Antiope's own stored text size, separate from the Native `reader.size` (decision of 2026-10-14). Its default is `TeletextFont.defaultSize` (20) and it always holds one of `sizeSteps` once the app wrote it; phase 4's stub and Settings preview, phase 8's root view, phase 10's menu, slider and reader and phase 13's CRT overlay read it.

- [ ] **Step 1: Write the failing test.** Append to `TeletextTests.swift`:

```swift
@Suite struct TeletextFontTests {
    @Test func bedsteadIsAvailableInTheApp() {
        #expect(TeletextFont.isAvailable)
        #expect(TeletextFont.familyName == "Bedstead")
        #expect(Bundle.main.url(forResource: TeletextFont.fileName, withExtension: "otf", subdirectory: "Fonts") != nil)
        #expect(TeletextFont.font(size: 20) == Font.custom("Bedstead", fixedSize: 20))
    }

    @Test func cellsAndPixelsFollowTheSize() {
        #expect(TeletextFont.sizeSteps.map(TeletextFont.cell(forSize:)) == [
            CGSize(width: 9, height: 15), CGSize(width: 12, height: 20),
            CGSize(width: 15, height: 25), CGSize(width: 18, height: 30),
        ])
        #expect(TeletextFont.sizeSteps.map(TeletextFont.pixel(forSize:)) == [1.5, 2, 2.5, 3])
    }

    @Test func sizesSnapToTheNearestStepAndTiesGoUp() {
        let cases: [(Double, Double)] = [
            (12, 15), (15, 15), (17, 15), (17.5, 20), (18, 20), (20, 20), (22, 20), (22.5, 25),
            (23, 25), (27, 25), (27.5, 30), (28, 30), (40, 30), (-3, 15), (.nan, 20), (.infinity, 20),
        ]
        for (size, expected) in cases {
            #expect(TeletextFont.snapped(size) == expected, "\(size)")
        }
        #expect(TeletextFont.defaultSize == 20)
        #expect(EnvironmentValues().teletextSize == TeletextFont.defaultSize)
        // Antiope keeps its own size, never the Native `reader.size` (decision of 2026-10-14).
        #expect(ReaderStyle.Keys.antiopeSize == "reader.size.antiope" && ReaderStyle.Keys.size == "reader.size")
    }

    @Test func stepsMoveFromTheSnappedSizeAndStopAtTheEnds() {
        #expect(TeletextFont.step(17, by: 1) == 20)  // a stored 17 draws at 15, so ⌘+ gives 20
        #expect(TeletextFont.step(18, by: 1) == 25)  // 18 draws at 20
        #expect(TeletextFont.step(20, by: -1) == 15)
        #expect(TeletextFont.step(30, by: 1) == 30)
        #expect(TeletextFont.step(15, by: -1) == 15)
        #expect(TeletextFont.step(15, by: 7) == 30)
        #expect(TeletextFont.step(25, by: 0) == 25)
    }

    /// The French repertoire and the glyphs Antiope draws (spec §2, Font), and what Bedstead lacks.
    @Test func supportsReadsBedsteadsCharacterSet() {
        for character in "éèêëàâîïôùûüÿçœæÉÈÊËÀÂÎÏÔÙÛÜŸÇŒÆ«»’“”…–—€°▐█░●*?" {
            #expect(TeletextFont.supports(character), "\(character)")
        }
        #expect(TeletextFont.supports("\u{1FB00}") && TeletextFont.supports("\u{1FB3B}"))  // sextant mosaics
        #expect(TeletextFont.supports("e\u{301}"))  // composed to é first
        for character: Character in ["🎉", "東", "❤️", "🇫🇷", "q\u{301}", "\u{301}"] {
            #expect(!TeletextFont.supports(character), "\(character)")
        }
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/TeletextFontTests`. Expected: `error: cannot find 'TeletextFont' in scope` (and `value of type 'EnvironmentValues' has no member 'teletextSize'`), then `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Teletext/TeletextFont.swift`:

```swift
import AppKit
import CoreText
import os
import SwiftUI

/// Bedstead 3.261 (Ben Harris and others, CC0), modelled on the SAA5050 teletext character
/// generator, bundled in `Resources/Fonts` and registered for the app process by
/// `ATSApplicationFontsPath`, like Newsreader.
///
/// Its geometry fixes every metric here: one advance of 600/1000 em, ascent 800, descent 200,
/// no line gap. A glyph is drawn on a 6 × 10 pixel cell, so one font pixel is a tenth of the size.
enum TeletextFont {
    static let familyName = "Bedstead"
    /// `Resources/Fonts/bedstead.otf`.
    static let fileName = "bedstead"
    /// Sizes at which one font pixel lands on whole Retina pixels (spec §5, Settings).
    static let sizeSteps: [Double] = [15, 20, 25, 30]
    static let defaultSize = 20.0

    /// Bedstead is registered and resolvable through `NSFont`. When it is not, the app logs it
    /// once and stays in Native (spec §5, Switch).
    static var isAvailable: Bool { characterSet != nil }

    /// `fixedSize`: Antiope sizes are exact, never scaled by the text-size accessibility setting.
    static func font(size: Double) -> Font {
        .custom(familyName, fixedSize: size)
    }

    /// One character cell: 0.6 em wide, 1 em tall. `size * 6 / 10` is exact for whole sizes.
    static func cell(forSize size: Double) -> CGSize {
        CGSize(width: size * 6 / 10, height: size)
    }

    static func pixel(forSize size: Double) -> Double {
        size / 10
    }

    /// The nearest of `sizeSteps`; a size halfway between two steps takes the bigger one.
    static func snapped(_ size: Double) -> Double {
        guard size.isFinite else { return defaultSize }
        let nearest = sizeSteps.min { a, b in
            let (da, db) = (abs(a - size), abs(b - size))
            return da < db || (da == db && a > b)
        }
        return nearest ?? defaultSize
    }

    /// `delta` steps from the step `size` snaps to, clamped to the first and last steps.
    static func step(_ size: Double, by delta: Int) -> Double {
        let index = sizeSteps.firstIndex(of: snapped(size)) ?? 0
        return sizeSteps[min(max(index + delta, 0), sizeSteps.count - 1)]
    }

    /// True when every scalar of the grapheme's composed form (NFC) is in Bedstead's character
    /// set: "e" + U+0301 counts as "é", which Bedstead has, although it lacks U+0301 alone.
    static func supports(_ grapheme: Character) -> Bool {
        guard let characterSet else { return false }
        let composed = String(grapheme).precomposedStringWithCanonicalMapping
        return composed.unicodeScalars.allSatisfy { characterSet.contains($0) }
    }

    /// Bedstead's character map, read once; nil (and logged) when the font is not registered.
    private static let characterSet: CharacterSet? = {
        guard let font = NSFont(name: familyName, size: defaultSize), font.familyName == familyName else {
            Logger(subsystem: "org.w3cdotorg.ressac", category: "teletext")
                .error("Bedstead is not registered: the Antiope appearance stays off")
            return nil
        }
        return CTFontCopyCharacterSet(font as CTFont) as CharacterSet
    }()
}

extension EnvironmentValues {
    /// The Antiope text size, already snapped to a step; `AntiopeRootView` sets it.
    @Entry var teletextSize: Double = TeletextFont.defaultSize
}
```

Then, in `apps/macos/Ressac/Settings/ReaderStyle.swift` (re-read it first), replace:

```swift
        static let size = "reader.size"
```

with:

```swift
        static let size = "reader.size"
        /// Antiope's own text size (decision of 2026-10-14), one of `TeletextFont.sizeSteps`,
        /// `TeletextFont.defaultSize` until changed. `size` stays the Native size: switching looks
        /// never changes the other one.
        static let antiopeSize = "reader.size.antiope"
```

- [ ] **Step 4: Run green**, same filter. Expected: `✔ Test run with 5 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Teletext/TeletextFont.swift apps/macos/Ressac/Settings/ReaderStyle.swift apps/macos/RessacTests/TeletextTests.swift
git commit -F - <<'EOF'
macOS app: Bedstead metrics, Antiope size steps and glyph coverage

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3.4: `CellText`, strings fitted to an exact number of cells

**Interfaces:**
- Consumes: `TeletextFont.supports(_:)` (Task 3.3), as the default coverage test.
- Produces (`Teletext/CellText.swift`): `struct CellRun: Equatable, Sendable { var text: String; var isReplacement: Bool }`, `struct FittedCells: Equatable, Sendable { var runs: [CellRun]; var accessibilityText: String }` and `enum CellText { static func fit(_ text: String, cells: Int, supports: (Character) -> Bool = TeletextFont.supports) -> FittedCells }`. Phase 8 builds every column row on `fit`, draws replacement runs in magenta and gives `accessibilityText` to VoiceOver; phase 12 does the same for the garden pages.

The runs hold exactly `cells` characters, one Unicode scalar per cell; adjacent runs of the same kind are merged; `cells <= 0` gives no run.

- [ ] **Step 1: Write the failing test.** Append to `TeletextTests.swift`. `emojiCJKAndCombiningAccentsTakeOneCellEach` pins Review Focus #1:

```swift
@Suite struct CellTextTests {
    func cellCount(_ fitted: FittedCells) -> Int {
        fitted.runs.reduce(0) { $0 + $1.text.count }
    }

    /// Review Focus #1: emoji, CJK and a combining accent each take exactly one cell.
    @Test func emojiCJKAndCombiningAccentsTakeOneCellEach() {
        let fitted = CellText.fit("Café 🎉 東京", cells: 12)
        #expect(fitted.runs == [
            CellRun(text: "Café ", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
            CellRun(text: " ", isReplacement: false),
            CellRun(text: "??", isReplacement: true),
            CellRun(text: "   ", isReplacement: false),
        ])
        #expect(cellCount(fitted) == 12)
        #expect(fitted.runs.reduce(0) { $0 + $1.text.unicodeScalars.count } == 12)  // one scalar per cell
        #expect(fitted.accessibilityText == "Café 🎉 東京")

        let decomposed = CellText.fit("Cafe\u{301} noir", cells: 6)
        #expect(decomposed.runs == [CellRun(text: "Café n", isReplacement: false)])
        #expect(decomposed.runs[0].text.unicodeScalars.map(\.value) == [0x43, 0x61, 0x66, 0xE9, 0x20, 0x6E])
        #expect(decomposed.accessibilityText == "Cafe\u{301} noir")
    }

    @Test func longTextIsCutWithoutEllipsisAndShortTextIsPadded() {
        #expect(CellText.fit("Courrier international", cells: 8).runs == [CellRun(text: "Courrier", isReplacement: false)])
        #expect(CellText.fit("Unread", cells: 9).runs == [CellRun(text: "Unread   ", isReplacement: false)])
        #expect(CellText.fit("", cells: 3).runs == [CellRun(text: "   ", isReplacement: false)])
        #expect(CellText.fit("xkcd", cells: 0).runs.isEmpty)
        #expect(CellText.fit("xkcd", cells: -2).runs.isEmpty)
        #expect(CellText.fit("xkcd", cells: 0).accessibilityText == "xkcd")
    }

    @Test func clustersWhitespaceAndInvisiblesNeverMoveTheGrid() {
        let family = CellText.fit("👩‍👩‍👧🇫🇷x", cells: 4)  // a ZWJ sequence and a flag: one cell each
        #expect(family.runs == [CellRun(text: "??", isReplacement: true), CellRun(text: "x ", isReplacement: false)])
        let lines = CellText.fit("Line\nbreak\ttab\r\nend", cells: 20)
        #expect(lines.runs == [CellRun(text: "Line break tab end  ", isReplacement: false)])
        let invisible = CellText.fit("a\u{200B}b\u{FEFF}c\u{00AD}d", cells: 5)
        #expect(invisible.runs == [CellRun(text: "abcd ", isReplacement: false)])
        for fitted in [family, lines, invisible] {
            #expect(fitted.runs.reduce(0) { $0 + $1.text.unicodeScalars.count } == cellCount(fitted))
        }
    }

    @Test func theCoverageTestIsInjectable() {
        let fitted = CellText.fit("abcb", cells: 4, supports: { $0 != "b" })
        #expect(fitted.runs == [
            CellRun(text: "a", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
            CellRun(text: "c", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
        ])
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/CellTextTests`. Expected: `error: cannot find 'CellText' in scope` (and the same for `FittedCells` and `CellRun`), then `** TEST FAILED **`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Teletext/CellText.swift`:

```swift
/// A run of cells: text Bedstead draws, or `?` standing for graphemes it lacks, which the
/// views draw in magenta (spec §5, `CellText`).
struct CellRun: Equatable, Sendable {
    var text: String
    var isReplacement: Bool
}

struct FittedCells: Equatable, Sendable {
    /// Exactly `cells` characters in total, one per cell, replacements as "?".
    var runs: [CellRun]
    /// The original string, untouched: VoiceOver reads this, never the cells.
    var accessibilityText: String
}

/// Fits a string into an exact number of character cells, so that no row can push a column
/// separator (spec §2, Layout).
enum CellText {
    /// One grapheme per cell. Truncation cuts between graphemes, without an ellipsis (teletext
    /// had none); short text is padded with spaces; adjacent runs of the same kind are merged.
    static func fit(_ text: String, cells: Int, supports: (Character) -> Bool = TeletextFont.supports) -> FittedCells {
        let width = max(cells, 0)
        var runs: [CellRun] = []
        var used = 0
        func add(_ piece: String, replacement: Bool) {
            if runs.last?.isReplacement == replacement {
                runs[runs.count - 1].text += piece
            } else {
                runs.append(CellRun(text: piece, isReplacement: replacement))
            }
        }
        for grapheme in text {
            guard used < width else { break }
            guard let cell = cell(for: grapheme, supports: supports) else { continue }
            add(String(cell.character), replacement: cell.isReplacement)
            used += 1
        }
        if used < width {
            add(String(repeating: " ", count: width - used), replacement: false)
        }
        return FittedCells(runs: runs, accessibilityText: text)
    }

    /// What one grapheme draws: a space for any whitespace (tabs and line breaks included),
    /// nothing for invisible format or control characters (zero-width space, BOM, soft hyphen),
    /// the composed form (NFC) when Bedstead has it, else a replacement "?".
    private static func cell(
        for grapheme: Character, supports: (Character) -> Bool
    ) -> (character: Character, isReplacement: Bool)? {
        if grapheme.isWhitespace { return (" ", false) }
        if grapheme.unicodeScalars.allSatisfy({ invisible.contains($0.properties.generalCategory) }) { return nil }
        let composed = String(grapheme).precomposedStringWithCanonicalMapping
        guard composed.count == 1, let character = composed.first, supports(character) else { return ("?", true) }
        return (character, false)
    }

    private static let invisible: Set<Unicode.GeneralCategory> = [.format, .control]
}
```

- [ ] **Step 4: Run green**, same filter. Expected: `✔ Test run with 4 tests in 1 suite passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Teletext/CellText.swift apps/macos/RessacTests/TeletextTests.swift
git commit -F - <<'EOF'
macOS app: fit strings to teletext cells, one grapheme per cell

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: the Rust checks pass unchanged; `** TEST SUCCEEDED **` with the 4 new suites (13 tests) on top of the existing ones; `no warnings in our sources`. A failure in `DatabaseTests`, `LauncherTests` or `FirstRunWALTests` with `.tooNew(3)` means phase 6 has not landed yet (see Execution order). Push:

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

### Notes for the executor

- **Verified ahead of time.** The three source files and `TeletextTests.swift` of this phase were built and run in a scratch copy of the repository with Xcode 27 and Swift 6.4 (Swift 6 mode, complete strict concurrency, warnings as errors), the two downloaded files in `Resources/Fonts`, and no `project.yml` change.
- **Bedstead 3.261, measured on 2026-10-14** (fontTools and AppKit): family, full and PostScript name `Bedstead`; 1000 units per em, `hhea` ascent 800, descent −200, line gap 0; 2 603 mapped characters, every one 600 units wide. The French repertoire of the spec, `« » ’ “ ” … – — € °`, `▐ █ ░ ●` and the sextant mosaics are mapped. U+0301 (combining acute), emoji, CJK, U+FE0F and U+200B are not, hence the NFC step and the invisible-character rule.
- **Measured, not `maximumAdvancement`.** `NSFont.maximumAdvancement` reports 13.24 at 20 pt because `hhea.advanceWidthMax` counts unmapped variant glyphs; the tests measure a 15-character string instead, which lays out at exactly 135, 180, 225 and 270 pt for 15, 20, 25 and 30 pt, with `defaultLineHeight` equal to the size. `ascender` comes back as 16.000032 at 20 pt, so it is compared within 0.01.
- **`CGFloat` against `Double` inside `#expect`.** `#expect(nsSize.width == size * 6 / 10 * 15)` and `#expect(layoutManager.defaultLineHeight(for: font) == size)` failed in the test host although both sides printed `180.0` and `20.0` and the same comparison on local variables outside the macro was true. The tests therefore convert to `Double` before comparing. Keep that pattern in later phases (cell geometry, CRT bands).
- **String equality in Swift is canonical:** `"Cafe\u{301}" == "Café"` is true. The NFC test therefore checks the unicode scalars (`0xE9`), not the string.
- **`characterSet` is computed once**, on first use, from `NSFont(name:size:)`. `ATSApplicationFontsPath` registers the bundle's fonts before `main`, so the first access already sees Bedstead in the app and in the test host. When the font is missing, the `teletext` log category records it once; `Appearance.effective` (phase 4) then keeps Native.
- **Antiope's own size.** Decision of 2026-10-14: Antiope stores its size under `ReaderStyle.Keys.antiopeSize` (default `TeletextFont.defaultSize`, 20), separate from the Native `reader.size` (default 17), so a never-changed Antiope draws at 20 and switching looks changes neither size. This phase only adds the key; phase 10a's menu and Settings write it.
