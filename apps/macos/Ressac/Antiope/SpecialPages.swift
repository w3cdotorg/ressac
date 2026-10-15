import SwiftUI

extension CellLine {
    /// `text` centred in `width` cells (one cell more on the right when the margin is odd).
    static func centred(_ text: String, width: Int, foreground: TeletextColor) -> CellLine {
        let margin = max(0, (width - text.count) / 2)
        return CellLine([CellSpan(String(repeating: " ", count: margin) + text, cells: width, foreground: foreground)],
                        accessibilityText: text)
    }
}

/// A cell row drawn twice as tall over two rows, as teletext's double height.
struct DoubleHeight: View {
    let line: CellLine
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        CellLineView(line: line)
            .scaleEffect(x: 1, y: 2, anchor: .top)
            .frame(height: cell.height * 2, alignment: .top)
    }
}

/// P100 SOMMAIRE (spec §6), drawn in the reader column: a double-height `RESSAC` over a mosaic
/// wave, then every page in two columns with its count, reserved pages in colour. A click goes to
/// the page; typing its number works as everywhere.
struct SommairePage: View {
    /// One row of the two-column list.
    struct Pair: Equatable, Sendable {
        var left: PageDirectory.Row
        var right: PageDirectory.Row?
    }

    /// The wave of the app icon in half-block mosaics, repeated across the page.
    static let wave: (top: [Character], bottom: [Character]) = (Array(" ▄▀▀▄   "), Array("▀    ▀▄▄"))

    @Environment(AntiopeState.self) private var antiope
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        let directory = antiope.directory
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            let half = Self.halfWidth(width)
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    DoubleHeight(line: Self.titleLine(width: width))
                    ForEach(Array(Self.logo(width: width).enumerated()), id: \.offset) { _, line in
                        CellLineView(line: line)
                    }
                    .accessibilityHidden(true)
                    CellLineView(line: .blank(width))
                    ForEach(Array(Self.pairs(directory.rows).enumerated()), id: \.offset) { _, pair in
                        HStack(spacing: 0) {
                            pageButton(pair.left, width: half, in: directory)
                            CellLineView(line: .blank(width - 2 * half))
                            if let right = pair.right {
                                pageButton(right, width: half, in: directory)
                            }
                        }
                    }
                }
            }
            .scrollIndicators(.never)
        }
    }

    private func pageButton(_ row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> some View {
        let line = Self.line(for: row, width: width, in: directory)
        return Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
            .buttonStyle(.plain)
            .accessibilityLabel(line.accessibilityText)
    }

    /// Column-major, as a printed index: the first half of the pages down the left, the rest
    /// down the right.
    static func pairs(_ rows: [PageDirectory.Row]) -> [Pair] {
        let leftCount = (rows.count + 1) / 2
        return (0..<leftCount).map { index in
            Pair(left: rows[index], right: index + leftCount < rows.count ? rows[index + leftCount] : nil)
        }
    }

    /// Each column's width; the one or two cells left over separate them.
    static func halfWidth(_ width: Int) -> Int {
        max(0, (width - 1) / 2)
    }

    /// An index row with the reserved titles in colour.
    static func line(for row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> CellLine {
        PageIndexColumn.line(for: row, width: width, isRootFolder: PageIndexColumn.isRootFolder(row, in: directory),
                             isSelected: false, highlightsReserved: true)
    }

    static func titleLine(width: Int) -> CellLine {
        .centred("RESSAC", width: width, foreground: .yellow)
    }

    /// Two rows of the wave in cyan; decorative, so VoiceOver skips them.
    static func logo(width: Int) -> [CellLine] {
        [wave.top, wave.bottom].map { pattern in
            let row = String((0..<max(0, width)).map { pattern[$0 % pattern.count] })
            return CellLine([CellSpan(row, cells: width, foreground: .cyan)], accessibilityText: "")
        }
    }
}

/// One line of P899: the keys, spelled out, and what they do.
struct GuideKey: Equatable, Sendable {
    var keys: String
    var action: String
}

struct GuideSection: Equatable, Sendable {
    var title: String
    var keys: [GuideKey]
}

/// P899 GUIDE (spec §6), drawn in the reader column: every key of the window, grouped as the
/// GUIDE page of a Minitel service. Keys are spelled in capitals: Bedstead has no ⌘, ⌥, ⇧ or ⌫.
struct GuidePage: View {
    /// The key column, wide enough for ` SHIFT CMD R`.
    static let keyCells = 12

    static let sections: [GuideSection] = [
        GuideSection(title: "PAGES", keys: [
            GuideKey(keys: "0-9", action: "type a page number"),
            GuideKey(keys: "DEL", action: "erase the last digit"),
            GuideKey(keys: "ESC", action: "cancel the number"),
            GuideKey(keys: "OPT S", action: "SOMMAIRE: every page"),
            GuideKey(keys: "OPT G  ?", action: "GUIDE: this page"),
        ]),
        GuideSection(title: "READING", keys: [
            GuideKey(keys: "SPACE  J", action: "SUITE: next page"),
            GuideKey(keys: "K", action: "RETOUR: previous page"),
            GuideKey(keys: "UP DOWN", action: "previous or next entry"),
            GuideKey(keys: "M", action: "mark read or unread"),
            GuideKey(keys: "S", action: "star or unstar"),
            GuideKey(keys: "I", action: "mosaics or original images"),
            GuideKey(keys: "OPT R", action: "RÉPÉTITION: refresh"),
            GuideKey(keys: "CMD RETURN", action: "open in the browser"),
        ]),
        GuideSection(title: "COLUMNS", keys: [
            GuideKey(keys: "LEFT RIGHT", action: "change column"),
            GuideKey(keys: "UP DOWN", action: "in the index: previous or next page"),
        ]),
        GuideSection(title: "GARDEN", keys: [
            GuideKey(keys: "K", action: "keep"),
            GuideKey(keys: "R", action: "repair"),
            GuideKey(keys: "B", action: "bury"),
            GuideKey(keys: "U", action: "undo"),
            GuideKey(keys: "O", action: "open the site"),
            GuideKey(keys: "W", action: "open the Wayback snapshot"),
            GuideKey(keys: "SHIFT R", action: "apply the safe repairs"),
        ]),
        GuideSection(title: "MENUS", keys: [
            GuideKey(keys: "CMD F", action: "search: RETURN keeps it, ESC clears it"),
            GuideKey(keys: "CMD R", action: "refresh the selection"),
            GuideKey(keys: "SHIFT CMD R", action: "refresh every feed"),
            GuideKey(keys: "SHIFT CMD A", action: "mark all as read"),
            GuideKey(keys: "CMD + -", action: "bigger or smaller text"),
            GuideKey(keys: "CMD O", action: "import OPML"),
            GuideKey(keys: "SHIFT CMD E", action: "export OPML"),
        ]),
    ]

    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    DoubleHeight(line: .centred("GUIDE", width: width, foreground: .yellow))
                    ForEach(Array(Self.lines(width: width).enumerated()), id: \.offset) { _, line in
                        CellLineView(line: line)
                    }
                }
            }
            .scrollIndicators(.never)
        }
    }

    /// For each section: a blank row, the heading black on green, then one row per key, the keys
    /// in cyan and the action in white. Every row is exactly `width` cells.
    static func lines(width: Int) -> [CellLine] {
        let keyCells = min(Self.keyCells, max(0, width))
        var lines: [CellLine] = []
        for section in sections {
            lines.append(.blank(width))
            lines.append(CellLine([CellSpan(" " + section.title, cells: width, foreground: .black, background: .green)],
                                  accessibilityText: section.title))
            for key in section.keys {
                lines.append(CellLine([
                    CellSpan(" " + key.keys, cells: keyCells, foreground: .cyan),
                    CellSpan(key.action, cells: width - keyCells, foreground: .white),
                ], accessibilityText: "\(key.keys): \(key.action)"))
            }
        }
        return lines
    }
}
