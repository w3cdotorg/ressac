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
