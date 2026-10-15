import SwiftUI

/// One coloured stretch of a cell row: its text already fitted to a number of cells.
struct CellSpan: Equatable, Sendable {
    var cells: FittedCells
    var foreground: TeletextColor
    var background: TeletextColor

    /// Fits `text` into exactly `cells` cells (`CellText.fit`: truncated by grapheme, padded with
    /// spaces, missing glyphs as `?`); a negative count is taken as 0.
    init(_ text: String, cells: Int, foreground: TeletextColor, background: TeletextColor = .black) {
        self.cells = CellText.fit(text, cells: max(0, cells))
        self.foreground = foreground
        self.background = background
    }

    /// The cells this span takes: one per character of its runs.
    var cellCount: Int { cells.runs.reduce(0) { $0 + $1.text.count } }

    /// The span's text in its colour; a replaced grapheme stays a magenta `?` (spec §5, `CellText`).
    var attributedText: AttributedString {
        cells.runs.reduce(into: AttributedString()) { text, run in
            var part = AttributedString(run.text)
            part.foregroundColor = (run.isReplacement ? TeletextColor.magenta : foreground).color
            text += part
        }
    }
}

/// A row of the teletext grid (spec §5, Columns): spans side by side, `width` cells in all. Every
/// text an Antiope view draws is one, so no glyph can push a column or a separator.
struct CellLine: Equatable, Sendable {
    var spans: [CellSpan]
    /// What VoiceOver reads: the original strings, never the fitted cells (spec §8, Accessibility).
    var accessibilityText: String

    /// Without `accessibilityText`, VoiceOver reads the spans' original strings, trimmed and
    /// joined by spaces.
    init(_ spans: [CellSpan], accessibilityText: String? = nil) {
        self.spans = spans
        self.accessibilityText = accessibilityText ?? spans
            .map { $0.cells.accessibilityText.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
    }

    var width: Int { spans.reduce(0) { $0 + $1.cellCount } }

    /// `width` black cells, with nothing to read.
    static func blank(_ width: Int) -> CellLine {
        CellLine([CellSpan("", cells: width, foreground: .white)], accessibilityText: "")
    }

    /// Reverse video, as teletext shows a selection or a key: every span in `foreground` on `background`.
    func reversed(_ foreground: TeletextColor, on background: TeletextColor) -> CellLine {
        CellLine(spans.map { span in
            var span = span
            span.foreground = foreground
            span.background = background
            return span
        }, accessibilityText: accessibilityText)
    }
}

/// Draws a `CellLine`, each span in a frame of exactly its cells, in Bedstead at `\.teletextSize`.
struct CellLineView: View {
    let line: CellLine
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        HStack(spacing: 0) {
            ForEach(Array(line.spans.enumerated()), id: \.offset) { _, span in
                Text(span.attributedText)
                    .font(TeletextFont.font(size: size))
                    .lineLimit(1)
                    .fixedSize()
                    .frame(width: CGFloat(span.cellCount) * cell.width, height: cell.height, alignment: .leading)
                    .background(span.background.color)
            }
        }
        .frame(width: CGFloat(line.width) * cell.width, height: cell.height, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(line.accessibilityText)
    }
}
