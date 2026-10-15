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

    var id: Self { self }

    /// Maximum line length, in `ch` units of the article font.
    var maxCh: Int {
        switch self {
        case .narrow: 58
        case .medium: 68
        case .wide: 80
        }
    }

    var displayName: String {
        switch self {
        case .narrow: "Narrow"
        case .medium: "Medium"
        case .wide: "Wide"
        }
    }
}

/// How articles look. Persisted with `@AppStorage` under `Keys`; Phase 9's View menu
/// (Bigger Text ⌘+, Smaller Text ⌘−, Actual Size ⌘0) writes `Keys.size` directly.
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
