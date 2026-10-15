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
