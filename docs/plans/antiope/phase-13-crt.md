# Phase 13: The CRT screen

**Runs after:** phase 12.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`). Global Constraints apply, in particular the **CRT values** (scanline band = lower 40 % of each font-pixel row, black at `0.28`; vignette black `0.35` at the corners; corner radius 3 % of the window's shorter side; bleed red `+1`, blue `−1`, opacity `0.45`, glow `2`). Spec §9 (the CRT screen), §5 (Settings: the checkbox), §11 (CRT tests), §13 (risk: bleed and scrolling smoothness).

**Goal of the phase:** with Appearance on Antiope and Settings ▸ CRT screen checked, the whole Antiope window gets scanlines on the font-pixel grid, a vignette and rounded tube corners from one overlay that never takes a click, and every SwiftUI text and the reader get a colour bleed that steps aside for Increase contrast. Nothing animates; nothing is drawn in Native.

**Files (5):**
- Create: `apps/macos/Ressac/Antiope/CRTOverlay.swift` (`CRTGeometry`, `CRTOverlay`, `CRTBleedRenderer`, `crtBleed(_:)`, `CRTScreen`, `crtScreen()`)
- Create: `apps/macos/RessacTests/CRTTests.swift`
- Modify: `apps/macos/Ressac/Settings/SettingsView.swift` (the CRT screen checkbox under the Appearance picker)
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (one modifier: `.crtScreen()`)
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift` (the `crt` of `ReaderTheme`)

None of these files is restructured, so the Step-0 rule does not apply even if one is over 300 lines (`ReaderView.swift` may be after phases 10 and 11; check with `wc -l` and note it).

**Consumes** (pinned names; anything marked *assumed* is listed again in "Notes for the executor"):
- Phase 3: `TeletextColor` (`.color`), `TeletextFont.sizeSteps` (`[15, 20, 25, 30]`), `TeletextFont.pixel(forSize:)` (size / 10), `TeletextFont.snapped(_:)`, `TeletextFont.isAvailable`.
- Phase 4: `Appearance` (`.native`, `.antiope`), `Appearance.Keys.appearance`, `Appearance.Keys.crt` (`"antiope.crt"`), `Appearance.effective(stored:fontAvailable:)`; the Appearance `Picker` that phase 4 put first in `private struct ReadingSettings` (`SettingsView.swift`), with `@AppStorage(Appearance.Keys.appearance) private var appearance` and the computed effective look `shown` (phase 10a leaves both; checked against `phase-04-appearance.md` and `phase-10-reader.md`).
- Phase 8: `AntiopeRootView` (`Antiope/AntiopeRootView.swift`), drawn with `\.teletextSize = TeletextFont.snapped` of Antiope's own stored size (`ReaderStyle.Keys.antiopeSize`, phase 3, default `TeletextFont.defaultSize`); phases 9 and 12 leave its last modifiers (`.onAppear`, `.onDisappear`) as they are.
- Phase 10: `ReaderTheme(appearance:crt:)` built in one place, the private `theme` property of `ReaderView` (`Reading/ReaderView.swift`, phase 10b), `ArticleTemplate` writing `data-crt` when `theme.showsCRT`, and the `reader.css` rule that gives text the bleed `text-shadow` under `data-crt`.
- Phase 3: `ReaderStyle.Keys.antiopeSize`, `TeletextFont.defaultSize`.

**Produces** (pinned in the plan index: `CRTOverlay(teletextSize:)`, `CRTGeometry.scanlineBands(height:pixel:)`, `CRTGeometry.cornerRadius(for:)`, `View.crtBleed(_:)`):
- `enum CRTGeometry` with the constants `scanlineOpacity`, `scanlineFraction`, `vignetteOpacity`, `cornerPercent`, `bleedOffset`, `bleedOpacity`, `glowRadius`, `drawsTintedCopies`, and `isActive(appearance:crt:)`, `bleedIsActive(_:contrast:)`, `scanlineBands(height:pixel:scale:)` (`scale` defaults to 2, so the pinned two-argument call is valid), `cornerRadius(for:)`.
- `struct CRTOverlay: View, Equatable` (`init(teletextSize:)`, `allowsHits`).
- `struct CRTBleedRenderer: TextRenderer` (`isActive`, `displayPadding`).
- `extension View { func crtBleed(_ active: Bool) -> some View; func crtScreen() -> some View }`.

**Design, in short:**
- **One overlay, one `Canvas`.** `CRTOverlay` draws the scanline bands, the radial vignette and the black outside of a rounded rectangle in one `Canvas`, with `allowsHitTesting(false)`. It is `Equatable` on the text size and wrapped in `.equatable()`, so its body runs again only for a new text size, a new size of the window (the `Canvas` itself) or a new display scale. It is applied on the root view, so its origin is the cell grid's origin and the bands stay on font-pixel rows; above the reader's `WKWebView` it stays fixed while the article scrolls under it.
- **Bands on whole device pixels.** 40 % of a font pixel is not a whole number of Retina pixels at every size (at 15 pt a font pixel is 3 device pixels, 40 % of it 1.2). `scanlineBands` rounds the band to whole device pixels, at least one, and ends it on the font-pixel row's bottom edge, which is always on a device pixel because the sizes are snapped (spec §5). At 2× the bands are 0.5 pt at 15 pt and 1 pt at 20, 25 and 30 pt. This keeps "lines stay crisp at every size" (spec §11, manual item 9) at the cost of a band of 33 % to 50 % instead of exactly 40 %.
- **The bleed is a `TextRenderer`.** An overlay cannot read the pixels under it (spec §9), and duplicating whole columns would duplicate their `ScrollView`s. SwiftUI's `textRenderer(_:)` reaches every `Text` below the view it is applied to, inside scroll views and lazy stacks included, so `crtBleed(_:)` is applied once on the root and draws, for each line of text, a 2 pt blurred copy (the glow, in the text's own colour), a red copy at +1 pt and a blue copy at −1 pt at 0.45, then the line itself. No file of phases 8, 9 or 12 changes. The renderer is always installed and draws the plain line when inactive, so toggling the box or Increase contrast never changes the view tree (scroll positions survive).
- **Reader:** `ReaderTheme.crt` is true exactly when the bleed applies (Antiope, box checked, contrast not increased); phase 10's stylesheet does the rest. The scanlines and vignette over the reader come from the overlay.

---

### Task 13.1: CRT geometry (pure)

**Files:**
- Create: `apps/macos/Ressac/Antiope/CRTOverlay.swift`
- Create: `apps/macos/RessacTests/CRTTests.swift`

**Interfaces:**
- Consumes: `Appearance`, `TeletextFont.sizeSteps`, `TeletextFont.pixel(forSize:)`, SwiftUI `ColorSchemeContrast`.
- Produces: `CRTGeometry` (constants, `isActive(appearance:crt:)`, `bleedIsActive(_:contrast:)`, `scanlineBands(height:pixel:scale:)`, `cornerRadius(for:)`).

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/CRTTests.swift`:

```swift
import SwiftUI
import Testing

@testable import Ressac

@Suite struct CRTGeometryTests {
    @Test func theScreenIsOnlyForAntiopeWithTheBoxChecked() {
        #expect(CRTGeometry.isActive(appearance: .antiope, crt: true))
        #expect(!CRTGeometry.isActive(appearance: .antiope, crt: false))
        #expect(!CRTGeometry.isActive(appearance: .native, crt: true))
        #expect(!CRTGeometry.isActive(appearance: .native, crt: false))
    }

    /// Spec §9: Increase contrast turns the bleed off and keeps the scanlines and vignette.
    @Test func increaseContrastTurnsTheBleedOff() {
        #expect(CRTGeometry.bleedIsActive(true, contrast: .standard))
        #expect(!CRTGeometry.bleedIsActive(true, contrast: .increased))
        #expect(!CRTGeometry.bleedIsActive(false, contrast: .standard))
        #expect(!CRTGeometry.bleedIsActive(false, contrast: .increased))
    }

    /// Spec §11: the scanline bands land on font-pixel rows for each text size. Each band ends on
    /// its row's bottom edge, covers the lower part of the row rounded to whole Retina pixels,
    /// and every edge sits on a device pixel.
    @Test func bandsLandOnFontPixelRowsAtEveryTextSize() {
        let expectedBand: [Double: Double] = [15: 0.5, 20: 1.0, 25: 1.0, 30: 1.0]
        #expect(TeletextFont.sizeSteps == [15, 20, 25, 30])
        for size in TeletextFont.sizeSteps {
            let pixel = TeletextFont.pixel(forSize: size)
            let bands = CRTGeometry.scanlineBands(height: 600, pixel: pixel)
            #expect(bands.count == Int(600 / pixel), "\(size) pt")
            for (row, band) in bands.enumerated() {
                #expect(band.upperBound == Double(row + 1) * pixel, "\(size) pt, row \(row)")
                #expect(band.upperBound - band.lowerBound == expectedBand[size], "\(size) pt, row \(row)")
                #expect((band.lowerBound * 2).rounded() == band.lowerBound * 2, "\(size) pt, row \(row): crisp at 2x")
                #expect(band.upperBound - band.lowerBound <= pixel / 2, "\(size) pt: a band never covers more than half a row")
            }
        }
    }

    @Test func theLastBandStopsAtTheWindowEdge() {
        let bands = CRTGeometry.scanlineBands(height: 101, pixel: 2)
        #expect(bands.count == 50)
        #expect(bands.last == 99...100)
        let taller = CRTGeometry.scanlineBands(height: 101.5, pixel: 2)
        #expect(taller.count == 51)
        #expect(taller.last == 101...101.5)
        #expect(CRTGeometry.scanlineBands(height: 0, pixel: 2).isEmpty)
        #expect(CRTGeometry.scanlineBands(height: 100, pixel: 0).isEmpty)
        #expect(CRTGeometry.scanlineBands(height: 10, pixel: 2, scale: 1).first == 1...2, "one whole pixel at 1x")
    }

    @Test func tubeCornersAreThreePercentOfTheShorterSide() {
        #expect(CRTGeometry.cornerRadius(for: CGSize(width: 900, height: 500)) == 15)
        #expect(CRTGeometry.cornerRadius(for: CGSize(width: 1440, height: 900)) == 27)
        #expect(CRTGeometry.cornerRadius(for: .zero) == 0)
    }

    @Test func theValuesAreLeLionsAndTheSpecs() {
        #expect(CRTGeometry.scanlineOpacity == 0.28)
        #expect(CRTGeometry.scanlineFraction == 0.4)
        #expect(CRTGeometry.vignetteOpacity == 0.35)
        #expect(CRTGeometry.cornerPercent == 3)
        #expect(CRTGeometry.bleedOffset == 1)
        #expect(CRTGeometry.bleedOpacity == 0.45)
        #expect(CRTGeometry.glowRadius == 2)
    }
}
```

- [ ] **Step 2: Run red.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/CRTGeometryTests
```

Expected: build failure, `cannot find 'CRTGeometry' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/CRTOverlay.swift`:

```swift
import SwiftUI

/// The CRT screen's values and pure geometry (spec §9, after LeLion-web's `Shaders/Crt.gdshader`).
enum CRTGeometry {
    /// Scanlines: black at this opacity over the lower part of every font-pixel row.
    static let scanlineOpacity = 0.28
    /// The part of a font-pixel row a band covers, from the row's bottom (before rounding to
    /// whole device pixels).
    static let scanlineFraction = 0.4
    /// Vignette: black at this opacity in the corners, clear in the centre.
    static let vignetteOpacity = 0.35
    /// Tube corners: radius in percent of the window's shorter side.
    static let cornerPercent = 3.0
    /// Colour bleed: a red copy at +offset and a blue copy at -offset, at this opacity, over a
    /// glow of the text's own colour.
    static let bleedOffset = 1.0
    static let bleedOpacity = 0.45
    static let glowRadius = 2.0
    /// False draws the glow alone: the spec §13 fallback if the tinted copies make scrolling
    /// stutter (decided by Task 13.4's measurement).
    static let drawsTintedCopies = true

    /// The CRT screen is drawn only in Antiope with the box checked.
    static func isActive(appearance: Appearance, crt: Bool) -> Bool {
        appearance == .antiope && crt
    }

    /// Increase contrast (`colorSchemeContrast == .increased`) turns the bleed off; the
    /// scanlines and the vignette stay.
    static func bleedIsActive(_ active: Bool, contrast: ColorSchemeContrast) -> Bool {
        active && contrast != .increased
    }

    /// One band per font-pixel row, counted from the top of the window: the lower part of the
    /// row, `scanlineFraction` of it rounded to whole device pixels (at least one), ending on the
    /// row's bottom edge, so that every edge falls on a device pixel at the snapped text sizes.
    /// The last band is clipped to `height`.
    static func scanlineBands(height: Double, pixel: Double, scale: Double = 2) -> [ClosedRange<Double>] {
        guard height > 0, pixel > 0, scale > 0 else { return [] }
        let band = max(1, (pixel * scanlineFraction * scale).rounded()) / scale
        var bands: [ClosedRange<Double>] = []
        var row = 0
        while true {
            let bottom = Double(row + 1) * pixel
            let top = bottom - band
            guard top < height else { break }
            bands.append(top...min(bottom, height))
            row += 1
        }
        return bands
    }

    /// 3 % of the shorter side.
    static func cornerRadius(for size: CGSize) -> Double {
        Double(min(size.width, size.height)) * cornerPercent / 100
    }
}
```

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: `CRTGeometryTests` passes, 6 tests.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/CRTOverlay.swift apps/macos/RessacTests/CRTTests.swift
git commit -m "CRT screen: geometry of the scanlines, corners and when it applies" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 13.2: The overlay, the colour bleed and the screen modifier

**Files:**
- Modify: `apps/macos/Ressac/Antiope/CRTOverlay.swift`
- Modify: `apps/macos/RessacTests/CRTTests.swift`

**Interfaces:**
- Consumes: `CRTGeometry`, `TeletextColor`, `TeletextFont.pixel(forSize:)`, `TeletextFont.snapped(_:)`, `TeletextFont.isAvailable`, `TeletextFont.defaultSize`, `Appearance.effective(stored:fontAvailable:)`, `Appearance.Keys`, `ReaderStyle.Keys.antiopeSize`.
- Produces: `CRTOverlay(teletextSize:)` (`View`, `Equatable`, `allowsHits`), `CRTBleedRenderer(isActive:)`, `View.crtBleed(_:)`, `View.crtScreen()`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/CRTTests.swift`:

```swift
@MainActor @Suite struct CRTOverlayTests {
    /// Spec §11: the overlay never takes a hit test, so every click and scroll reaches the views
    /// under it. `CRTOverlay.body` applies `.allowsHitTesting(CRTOverlay.allowsHits)`.
    @Test func theOverlayNeverTakesAHitTest() {
        #expect(!CRTOverlay.allowsHits)
    }

    /// The overlay is redrawn only for a new text size (and, through its `Canvas`, a new window
    /// size or display scale): equal sizes compare equal, so `.equatable()` skips the body.
    @Test func theOverlayRedrawsOnlyForANewTextSize() {
        #expect(CRTOverlay(teletextSize: 20) == CRTOverlay(teletextSize: 20))
        #expect(CRTOverlay(teletextSize: 20) != CRTOverlay(teletextSize: 25))
    }

    /// The bleed draws around the glyphs, so the text's drawing area grows by the offset and the
    /// glow when it is on, and not at all when it is off.
    @Test func theBleedDrawsBeyondTheGlyphsOnlyWhenActive() {
        #expect(CRTBleedRenderer(isActive: false).displayPadding == EdgeInsets())
        let reach = CRTGeometry.bleedOffset + CRTGeometry.glowRadius
        #expect(CRTBleedRenderer(isActive: true).displayPadding
            == EdgeInsets(top: reach, leading: reach, bottom: reach, trailing: reach))
    }
}
```

- [ ] **Step 2: Run red.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/CRTOverlayTests
```

Expected: build failure, `cannot find 'CRTOverlay' in scope`.

- [ ] **Step 3: Implement.** Append to `apps/macos/Ressac/Antiope/CRTOverlay.swift`:

```swift

/// Scanlines, vignette and tube corners over the whole Antiope window (spec §9). Decoration
/// only: it never takes a click or a scroll, and nothing in it animates.
struct CRTOverlay: View, Equatable {
    /// `allowsHitTesting` of the overlay: every event goes through to the views under it.
    nonisolated static let allowsHits = false

    let teletextSize: Double

    @Environment(\.displayScale) private var displayScale

    nonisolated static func == (a: CRTOverlay, b: CRTOverlay) -> Bool {
        a.teletextSize == b.teletextSize
    }

    var body: some View {
        let pixel = TeletextFont.pixel(forSize: teletextSize)
        let scale = displayScale
        Canvas(opaque: false, rendersAsynchronously: false) { context, size in
            Self.draw(in: &context, size: size, pixel: pixel, scale: scale)
        }
        .allowsHitTesting(Self.allowsHits)
        .accessibilityHidden(true)
    }

    private static func draw(in context: inout GraphicsContext, size: CGSize, pixel: Double, scale: Double) {
        let bounds = CGRect(origin: .zero, size: size)
        let black = TeletextColor.black.color

        var bands = Path()
        for band in CRTGeometry.scanlineBands(height: size.height, pixel: pixel, scale: scale) {
            bands.addRect(CGRect(x: 0, y: band.lowerBound, width: size.width, height: band.upperBound - band.lowerBound))
        }
        context.fill(bands, with: .color(black.opacity(CRTGeometry.scanlineOpacity)))

        context.fill(Path(bounds), with: .radialGradient(
            Gradient(colors: [black.opacity(0), black.opacity(CRTGeometry.vignetteOpacity)]),
            center: CGPoint(x: bounds.midX, y: bounds.midY),
            startRadius: 0,
            endRadius: hypot(size.width, size.height) / 2))

        let radius = CRTGeometry.cornerRadius(for: size)
        var corners = Path(bounds)
        corners.addRoundedRect(in: bounds, cornerSize: CGSize(width: radius, height: radius), style: .continuous)
        context.fill(corners, with: .color(black), style: FillStyle(eoFill: true))
    }
}

/// Draws every line of text with a glow of its own colour, a red copy at +1 pt and a blue copy
/// at −1 pt at 0.45, then the line itself (spec §9). Inactive, it draws the line unchanged.
struct CRTBleedRenderer: TextRenderer {
    let isActive: Bool

    var displayPadding: EdgeInsets {
        guard isActive else { return EdgeInsets() }
        let reach = CRTGeometry.bleedOffset + CRTGeometry.glowRadius
        return EdgeInsets(top: reach, leading: reach, bottom: reach, trailing: reach)
    }

    func draw(layout: Text.Layout, in context: inout GraphicsContext) {
        for line in layout {
            if isActive {
                var glow = context
                glow.addFilter(.blur(radius: CRTGeometry.glowRadius))
                glow.draw(line)
                if CRTGeometry.drawsTintedCopies {
                    Self.drawTinted(line, in: context, color: TeletextColor.red.color, dx: CRTGeometry.bleedOffset)
                    Self.drawTinted(line, in: context, color: TeletextColor.blue.color, dx: -CRTGeometry.bleedOffset)
                }
            }
            context.draw(line)
        }
    }

    /// The line's glyph shapes filled with `color`, shifted by `dx`: drawn in a layer, then
    /// recoloured with `.sourceIn`, which keeps the colour only where the glyphs are.
    private static func drawTinted(_ line: Text.Layout.Line, in context: GraphicsContext, color: Color, dx: Double) {
        var tinted = context
        tinted.opacity = CRTGeometry.bleedOpacity
        tinted.translateBy(x: dx, y: 0)
        tinted.drawLayer { layer in
            layer.draw(line)
            layer.blendMode = .sourceIn
            layer.fill(Path(line.typographicBounds.rect.insetBy(dx: -1, dy: -1)), with: .color(color))
        }
    }
}

private struct CRTBleed: ViewModifier {
    let active: Bool

    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        content.textRenderer(CRTBleedRenderer(isActive: CRTGeometry.bleedIsActive(active, contrast: contrast)))
    }
}

/// Everything the CRT screen adds to `AntiopeRootView`: the bleed on every SwiftUI text below it
/// and the overlay over all of it, both only when the screen is on (Antiope, box checked).
private struct CRTScreen: ViewModifier {
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
    /// Antiope's own text size (decision of 2026-10-14), the one `AntiopeRootView` draws at.
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var size = TeletextFont.defaultSize

    func body(content: Content) -> some View {
        let active = CRTGeometry.isActive(
            appearance: Appearance.effective(stored: appearance, fontAvailable: TeletextFont.isAvailable), crt: crt)
        content
            .crtBleed(active)
            .overlay {
                if active {
                    CRTOverlay(teletextSize: TeletextFont.snapped(size)).equatable()
                }
            }
    }
}

extension View {
    /// The colour bleed of the CRT screen on every `Text` below this view. Always installed (it
    /// draws plain text when inactive), so switching it never rebuilds the view tree.
    func crtBleed(_ active: Bool) -> some View {
        modifier(CRTBleed(active: active))
    }

    /// The CRT screen of the Antiope window: apply once, as the last modifier of its root view.
    func crtScreen() -> some View {
        modifier(CRTScreen())
    }
}
```

- [ ] **Step 4: Run green.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/CRTGeometryTests -only-testing:RessacTests/CRTOverlayTests
```

Expected: 9 tests pass, no warning. If the compiler rejects `nonisolated static func ==` (a newer toolchain may infer the conformance differently), declare the conformance as `struct CRTOverlay: View, @MainActor Equatable` and drop the two `nonisolated`; the tests run on the main actor either way.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/CRTOverlay.swift apps/macos/RessacTests/CRTTests.swift
git commit -m "CRT screen: overlay (scanlines, vignette, tube corners) and colour bleed text renderer" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 13.3: The checkbox, the window and the reader

**Files:**
- Modify: `apps/macos/Ressac/Settings/SettingsView.swift`
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift`
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift`

**Interfaces:**
- Consumes: `CRTGeometry.isActive(appearance:crt:)`, `CRTGeometry.bleedIsActive(_:contrast:)`, `View.crtScreen()`, `Appearance.Keys.crt`, `Appearance.effective(stored:fontAvailable:)`, `TeletextFont.isAvailable`, phase 10's `ReaderTheme(appearance:crt:)`.
- Produces: the finished CRT screen; no new name.

- [ ] **Step 1: Re-read the three files** (Edit Safety) and check the line counts: `wc -l Ressac/Settings/SettingsView.swift Ressac/Antiope/AntiopeRootView.swift Ressac/Reading/ReaderView.swift` from `apps/macos`. Each block below is the text as phases 4, 8 to 12 leave it (`SettingsView.swift`: phase 4, then phase 10a; `AntiopeRootView.swift`: phases 8, 9 and 12; `ReaderView.swift`: phases 10b and 11b). `grep -rn "ReaderTheme(" Ressac/` finds one construction, in `ReaderView.swift`.

- [ ] **Step 2: The checkbox.** Phase 4 put the Appearance picker first in `private struct ReadingSettings`, with `@AppStorage(Appearance.Keys.appearance) private var appearance` and the computed effective look `shown` (Native whenever Bedstead failed to load); phase 10a left both. In `apps/macos/Ressac/Settings/SettingsView.swift`, replace:

```swift
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
```

with:

```swift
    @AppStorage(Appearance.Keys.appearance) private var appearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
```

and replace the Appearance picker's note:

```swift
            if !TeletextFont.isAvailable {
                Text("Antiope needs the Bedstead font, which did not load.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
```

with:

```swift
            if !TeletextFont.isAvailable {
                Text("Antiope needs the Bedstead font, which did not load.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Toggle("CRT screen", isOn: $crt)
                .disabled(!CRTGeometry.isActive(appearance: shown, crt: true))
            Text("Scanlines, a vignette and color bleed over the Antiope window. Increase Contrast turns the bleed off.")
                .font(.callout)
                .foregroundStyle(.secondary)
```

In Native, or when Bedstead is missing (Antiope then falls back to Native), the box is greyed and keeps its value.

- [ ] **Step 3: The window.** In `apps/macos/Ressac/Antiope/AntiopeRootView.swift`, `.crtScreen()` becomes the **last** modifier of the body's outermost view. Replace:

```swift
        .onAppear { antiope.closeSearchPrompt(clearing: false) }
        .onDisappear { antiope.closeSearchPrompt(clearing: false) }
    }
```

with:

```swift
        .onAppear { antiope.closeSearchPrompt(clearing: false) }
        .onDisappear { antiope.closeSearchPrompt(clearing: false) }
        .crtScreen()
    }
```

Being last, the overlay covers the whole window, page header (under the traffic lights), banner row and Minitel bar included, with the same origin as the cell grid; the bleed reaches every `Text` of the header, the columns, P100, P899, the garden pages and the Minitel bar.

- [ ] **Step 4: The reader.** Phase 10b declared `crt` and built the theme in one `ReaderTheme(` call; phase 11b added the optional `AntiopeState` after `crt`. In `apps/macos/Ressac/Reading/ReaderView.swift`, replace:

```swift
    @AppStorage(Appearance.Keys.crt) private var crt = false
```

with:

```swift
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @Environment(\.colorSchemeContrast) private var contrast
```

and replace:

```swift
    /// Antiope only when Bedstead is there (spec §5). `crt` is the CRT screen box; phase 13 also
    /// turns it off under Increase contrast, in this call (spec §9).
    private var theme: ReaderTheme {
        ReaderTheme(appearance: Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable),
                    crt: crt)
    }
```

with:

```swift
    /// Antiope only when Bedstead is there (spec §5). `crt` is true exactly when the CRT bleed
    /// applies: Antiope, the CRT screen box checked, Increase contrast off (spec §9).
    private var theme: ReaderTheme {
        let appearance = Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
        return ReaderTheme(appearance: appearance,
                           crt: CRTGeometry.bleedIsActive(CRTGeometry.isActive(appearance: appearance, crt: crt),
                                                          contrast: contrast))
    }
```

`data-crt`, and with it the reader's `text-shadow` bleed, is then present exactly when the SwiftUI bleed is: Antiope, box checked, contrast not increased.

- [ ] **Step 5: Build and run the suites.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: `** TEST SUCCEEDED **`, no warning. Phase 10's test that `data-crt` is in the page exactly when the theme says so still passes (it builds `ReaderTheme` directly).

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Settings/SettingsView.swift apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/Ressac/Reading/ReaderView.swift
git commit -m "CRT screen: Settings checkbox, overlay and bleed on the Antiope window, bleed in the reader" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 13.4: Risk check, scrolling smoothness with the bleed (spec §13)

The bleed draws each text line four times. The spec asks for a check on the largest folder, with the glow alone as the fallback. The measure is objective: Instruments' **Animation Hitches** template records the time the app spent in late frames; the hitch time ratio (ms of hitches per second of scrolling) is under **5 ms/s** for Apple's "good" range. Criterion: **with the CRT screen on, the ratio is at most 5 ms/s.**

Clément does the scrolling and the clicks; nothing here posts a keystroke or a mouse event (rule since 11/10). Claude builds, prepares the copy, starts and reads the recordings.

- [ ] **Step 1: Build a Release app and a copy of the real database.** The real file is only read, through SQLite's online backup.

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
CRT=$(mktemp -d -t ressac-crt); echo "$CRT"; mkdir -p "$CRT/home"
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$CRT/dd" build)
APP="$CRT/dd/Build/Products/Release/Ressac.app"
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
sqlite3 -readonly "$REAL" ".backup '$CRT/ressac.sqlite'"
HOME="$CRT/home" "$APP/Contents/Helpers/ressac" --db "$CRT/ressac.sqlite" migrate
sqlite3 "$CRT/ressac.sqlite" "
WITH RECURSIVE tree(root, id) AS (
  SELECT id, id FROM folder WHERE parent_id IS NULL
  UNION ALL SELECT t.root, f.id FROM folder f JOIN tree t ON f.parent_id = t.id)
SELECT r.page, r.name, count(e.id) AS entries
FROM tree t JOIN folder r ON r.id = t.root JOIN feed d ON d.folder_id = t.id JOIN entry e ON e.feed_id = d.id
GROUP BY t.root ORDER BY entries DESC LIMIT 3;"
```

Expected: `BUILD SUCCEEDED`, the migration to v3 on the copy, and three lines `page|name|entries`; the first is the largest folder (by the `notes.md` figures, several thousand entries). Write its page down.

- [ ] **Step 2: Launch the build on the copy and hand over.** Ask Clément to quit his own Ressac first (both builds share the bundle identifier and its settings), then:

```bash
pgrep -x Ressac && echo "Ressac is still running: wait for Clément" || \
  open -n --env RESSAC_DB="$CRT/ressac.sqlite" --env HOME="$CRT/home" "$APP"
```

Message to Clément: « La version de test tourne sur une copie de ta base. Dans Settings, mets Appearance sur Antiope et coche CRT screen, puis tape `<page du plus gros dossier>`. Dis-moi quand c'est prêt : je lance un enregistrement de 20 s pendant lequel tu fais défiler la colonne des articles puis l'article, en continu, au trackpad. »

- [ ] **Step 3: Record with the CRT screen on, then off.** When Clément is ready:

```bash
PID=$(pgrep -n -x Ressac); echo "$PID"
xcrun xctrace record --template 'Animation Hitches' --attach "$PID" --time-limit 20s --output "$CRT/crt-on.trace"
```

Then ask him to uncheck CRT screen and scroll the same way, and record `"$CRT/crt-off.trace"` with the same command. Expected for each: `Recording completed` and a `.trace` bundle. If `xctrace` refuses the template for this process, use `--instrument Hitches` instead of `--template 'Animation Hitches'`.

- [ ] **Step 4: Compute the two ratios.**

```bash
cat > "$CRT/hitch-ratio.py" <<'EOF'
"""Sums the hitch durations of an `xctrace export` table; prints the hitch time ratio."""
import sys
import xml.etree.ElementTree as ET

root = ET.parse(sys.argv[1]).getroot()
seconds = float(sys.argv[2])
by_id = {element.get("id"): element for element in root.iter() if element.get("id")}
total_ns = 0
count = 0
for row in root.iter("row"):
    for cell in row:
        if cell.tag == "duration":
            node = by_id.get(cell.get("ref"), cell) if cell.get("ref") else cell
            total_ns += int(node.text)
            count += 1
            break
print(f"{count} hitches, {total_ns / 1e6:.1f} ms in total, {total_ns / 1e6 / seconds:.2f} ms/s")
EOF
xcrun xctrace export --input "$CRT/crt-on.trace" --toc | grep -o 'schema="[^"]*hitch[^"]*"' | sort -u
for t in crt-on crt-off; do
  xcrun xctrace export --input "$CRT/$t.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="hitches"]' > "$CRT/$t-hitches.xml"
  printf '%s: ' "$t"; python3 -I "$CRT/hitch-ratio.py" "$CRT/$t-hitches.xml" 20
done
```

Expected: the table of contents lists `schema="hitches"` (if the name differs, use the one printed in the `--xpath`), then two lines such as `crt-on: 3 hitches, 41.7 ms in total, 2.08 ms/s`. If the export gives no rows, open the trace (`open "$CRT/crt-on.trace"`) and read the total hitch duration of the Hitches track over the 20 s instead.

- [ ] **Step 5: Decide.**
  - **CRT on ≤ 5 ms/s:** the full bleed stays. Nothing to commit.
  - **CRT on > 5 ms/s and CRT off ≤ 5 ms/s:** the bleed is the cause. Apply the spec's fallback, the glow alone: in `apps/macos/Ressac/Antiope/CRTOverlay.swift`, change `static let drawsTintedCopies = true` to `static let drawsTintedCopies = false`, rebuild (Step 1's `xcodebuild … build` line), relaunch (Step 2), record and compute again (Steps 3 and 4). Then commit:

    ```bash
    git add apps/macos/Ressac/Antiope/CRTOverlay.swift
    git commit -m "CRT screen: glow only, the tinted copies made scrolling stutter (<ratio on> ms/s, <ratio off> ms/s)" \
      -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
    ```

  - **Both > 5 ms/s:** scrolling stutters without the CRT screen too; the bleed is not the cause. Keep it, and tell Clément with both figures.

- [ ] **Step 6: Look, with Clément.** With the CRT screen on, ask him to check, and note his answers: the lines are regular and sharp at 15, 20, 25 and 30 pt; a click on an entry row, on a mosaic and on a Minitel key lands where it should; the reader scrolls under fixed lines; with System Settings ▸ Accessibility ▸ Display ▸ Increase contrast on, the coloured fringes disappear and the lines and vignette stay; in Native the box is greyed. Phase 14's checklist repeats item 9 on the real database; this step catches a gross problem before it.

- [ ] **Step 7: Clean up and report.** Ask Clément to quit the test build, then `rm -rf "$CRT"`. Report the two ratios, the decision and the answers of Step 6 in the phase hand-off: phase 14 copies them into `notes.md`.

### Task 13.5: Verification gate and push

- [ ] **Step 1: Gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-phase13.log" | tail -n 5
grep -c "warning:" "$TMPDIR/ressac-phase13.log"
```

Expected: all green, `** TEST SUCCEEDED **`, `0` warnings. New suites: `CRTGeometryTests` (6) and `CRTOverlayTests` (3).

- [ ] **Step 2: Push.**

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push -u origin antiope
```

Then stop: report to Clément (with Task 13.4's figures) and wait for his approval before phase 14.

## Notes for the executor

- **Edits checked against the producing phases:** every block of Task 13.3 matches the text phases 4 and 10a (`SettingsView.swift`), 8, 9 and 12 (`AntiopeRootView.swift`), 10b and 11b (`ReaderView.swift`) leave, applied in order in a scratch copy and built.
- **Extension of a pinned signature:** `CRTGeometry.scanlineBands(height:pixel:)` gains a defaulted `scale: Double = 2`, so the pinned call compiles unchanged and the overlay passes the real display scale. Reason: crisp bands need whole device pixels (see Design).
- **Why a `TextRenderer` for the bleed** (spec §9 says "one `crtBleed()` modifier on `CellText` views"): the modifier is still `crtBleed(_:)`, but applied once on the root instead of on every cell view, because SwiftUI propagates a text renderer to every `Text` below it. It avoids editing phases 8, 9 and 12's row views and avoids duplicating scroll views.
- **`colorSchemeContrast`** is `.increased` on macOS when System Settings ▸ Accessibility ▸ Display ▸ Increase contrast is on; Task 13.4 Step 6 checks it live, since no unit test can flip the system setting.
- **`open --env`** needs macOS 13 or later (README already uses it). `HOME` is redirected so the test build's logs stay in `$CRT`; its settings are Clément's, shared through the bundle identifier, which is why he quits his own Ressac first.
