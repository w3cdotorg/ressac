import SwiftUI

/// Article fonts offered in Settings (spec §7). The interface itself always stays in SF Pro.
enum ReaderFont: String, CaseIterable, Identifiable, Sendable {
    case sfPro, newYork, charter, newsreader, atkinson

    var id: Self { self }

    var displayName: String {
        switch self {
        case .sfPro: "SF Pro"
        case .newYork: "New York"
        case .charter: "Charter"
        case .newsreader: "Newsreader"
        case .atkinson: "Atkinson Hyperlegible Next"
        }
    }

    /// CSS `font-family` for the article page. Bundled fonts come first under the family
    /// name their `@font-face` rules declare (see `ArticleTemplate`).
    var cssFamily: String {
        switch self {
        case .sfPro: #"-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif"#
        case .newYork: #"ui-serif, "New York", Georgia, serif"#
        case .charter: #"Charter, "Charter BT", Georgia, serif"#
        case .newsreader: #""Newsreader", ui-serif, Georgia, serif"#
        case .atkinson: #""Atkinson Hyperlegible Next", -apple-system, sans-serif"#
        }
    }

    /// Font files shipped in `Resources/Fonts` (Phase 11) and embedded in the article page,
    /// because the web content process cannot see fonts registered by the app.
    var bundledFaces: [BundledFace] {
        switch self {
        case .newsreader:
            [BundledFace(file: "Newsreader-Variable", italic: false, weights: 200...800),
             BundledFace(file: "Newsreader-Italic-Variable", italic: true, weights: 200...800)]
        case .atkinson:
            [BundledFace(file: "AtkinsonHyperlegibleNext-Variable", italic: false, weights: 200...800),
             BundledFace(file: "AtkinsonHyperlegibleNext-Italic-Variable", italic: true, weights: 200...800)]
        case .sfPro, .newYork, .charter:
            []
        }
    }

    /// The same face for SwiftUI previews (Settings). Bundled fonts are registered for the app
    /// process through `ATSApplicationFontsPath` (Phase 11).
    func swiftUIFont(size: Double) -> Font {
        switch self {
        case .sfPro: .system(size: size)
        case .newYork: .system(size: size, design: .serif)
        case .charter: .custom("Charter", size: size)
        case .newsreader: .custom("Newsreader", size: size)
        case .atkinson: .custom("Atkinson Hyperlegible Next", size: size)
        }
    }
}

struct BundledFace: Equatable, Sendable {
    /// File name in `Resources/Fonts`, without the `.ttf` extension.
    let file: String
    let italic: Bool
    /// The `wght` axis range of the variable font.
    let weights: ClosedRange<Int>
}

enum LineWidth: String, CaseIterable, Identifiable, Sendable {
    case narrow, medium, wide
    /// Exactly 40 cells in Antiope, where `1ch` is one Bedstead cell (spec §5); offered in both looks.
    case forty

    var id: Self { self }

    /// Maximum line length, in `ch` units of the article font.
    var maxCh: Int {
        switch self {
        case .narrow: 58
        case .medium: 68
        case .wide: 80
        case .forty: 40
        }
    }

    var displayName: String {
        switch self {
        case .narrow: "Narrow"
        case .medium: "Medium"
        case .wide: "Wide"
        case .forty: "40 columns"
        }
    }
}

/// The look the reader draws in (spec §5 and §9).
struct ReaderTheme: Equatable, Sendable {
    var appearance: Appearance
    /// The CRT screen box (`Appearance.Keys.crt`); phase 13 also passes it off under Increase contrast.
    var crt: Bool

    static let native = ReaderTheme(appearance: .native, crt: false)

    /// The colour bleed applies only to Antiope with the box checked (spec §9).
    var showsCRT: Bool { appearance == .antiope && crt }
}

/// How articles look. Persisted with `@AppStorage` under `Keys`; the View menu (Bigger Text ⌘+,
/// Smaller Text ⌘−, Actual Size ⌘0) changes the size of the look shown, `resize(_:appearance:in:)`.
struct ReaderStyle: Equatable, Sendable {
    var font: ReaderFont = .sfPro
    var size: Double = ReaderStyle.defaultSize
    var width: LineWidth = .medium

    static let defaultSize = 17.0
    static let sizeRange = 12.0...28.0
    static let sizeStep = 1.0

    enum Keys {
        static let font = "reader.font"
        static let size = "reader.size"
        /// Antiope's own text size (decision of 2026-10-14), one of `TeletextFont.sizeSteps`,
        /// `TeletextFont.defaultSize` until changed. `size` stays the Native size: switching looks
        /// never changes the other one.
        static let antiopeSize = "reader.size.antiope"
        static let width = "reader.width"
    }

    static func clampedSize(_ size: Double) -> Double {
        guard size.isFinite else { return defaultSize }
        return min(max(size.rounded(), sizeRange.lowerBound), sizeRange.upperBound)
    }

    static func bigger(_ size: Double) -> Double { clampedSize(size + sizeStep) }
    static func smaller(_ size: Double) -> Double { clampedSize(size - sizeStep) }

    /// Reads the persisted style; unknown or missing values fall back to the defaults.
    static func load(from defaults: UserDefaults) -> ReaderStyle {
        var style = ReaderStyle()
        if let raw = defaults.string(forKey: Keys.font), let font = ReaderFont(rawValue: raw) {
            style.font = font
        }
        if defaults.object(forKey: Keys.size) != nil {
            style.size = clampedSize(defaults.double(forKey: Keys.size))
        }
        if let raw = defaults.string(forKey: Keys.width), let width = LineWidth(rawValue: raw) {
            style.width = width
        }
        return style
    }
}

/// What View ▸ Bigger Text (⌘+), Smaller Text (⌘−) and Actual Size (⌘0) ask for.
enum TextSizeCommand: Sendable {
    case bigger, smaller, actualSize
}

/// Text size per look (spec §5): Native moves by 1 pt in 12...28; Antiope draws only Bedstead's
/// steps (15, 20, 25, 30), so one font pixel lands on whole Retina pixels. Each look has its own
/// stored size (decision of 2026-10-14); Antiope snaps its size when drawing and steps from what
/// it draws.
extension ReaderStyle {
    static func effectiveSize(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: clampedSize(size)
        case .antiope: TeletextFont.snapped(size.isFinite ? size : TeletextFont.defaultSize)
        }
    }

    static func bigger(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: bigger(size)
        case .antiope: TeletextFont.step(effectiveSize(size, appearance: .antiope), by: 1)
        }
    }

    static func smaller(_ size: Double, appearance: Appearance) -> Double {
        switch appearance {
        case .native: smaller(size)
        case .antiope: TeletextFont.step(effectiveSize(size, appearance: .antiope), by: -1)
        }
    }

    /// What View ▸ Actual Size (⌘0) restores, and each look's size before any change.
    static func actualSize(for appearance: Appearance) -> Double {
        switch appearance {
        case .native: defaultSize
        case .antiope: TeletextFont.defaultSize
        }
    }

    /// Where each look stores its size: switching looks never changes the other one.
    static func sizeKey(for appearance: Appearance) -> String {
        switch appearance {
        case .native: Keys.size
        case .antiope: Keys.antiopeSize
        }
    }

    /// The size `appearance` stores, or its default while none is stored.
    static func storedSize(for appearance: Appearance, in defaults: UserDefaults) -> Double {
        let key = sizeKey(for: appearance)
        return defaults.object(forKey: key) == nil ? actualSize(for: appearance) : defaults.double(forKey: key)
    }

    /// ⌘+, ⌘− or ⌘0 on the size of the look shown, and only on it.
    static func resize(_ command: TextSizeCommand, appearance: Appearance, in defaults: UserDefaults) {
        let size = storedSize(for: appearance, in: defaults)
        let resized = switch command {
        case .bigger: bigger(size, appearance: appearance)
        case .smaller: smaller(size, appearance: appearance)
        case .actualSize: actualSize(for: appearance)
        }
        defaults.set(resized, forKey: sizeKey(for: appearance))
    }
}
