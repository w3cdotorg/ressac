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
