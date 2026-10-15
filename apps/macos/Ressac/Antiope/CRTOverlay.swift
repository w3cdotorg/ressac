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
