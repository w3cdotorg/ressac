import Foundation

/// The garden pages P800, P801 and P802 as teletext rows (spec §8). Pure functions over
/// `GardenModel` that build phase 8's `CellLine`s (exact widths, reverse video, magenta `?` for
/// what Bedstead lacks, VoiceOver text): the views only draw what these return, so grouping,
/// colours and widths are tested here.
enum GardenPage {
    /// A row of a garden page: a `CellLine`, plus what the page needs on top of it.
    struct Line: Identifiable, Equatable, Sendable {
        enum Role: Equatable, Sendable {
            case title, progress
            /// A status heading; the associated value is the status ("Dead", "Unchecked", "Buried").
            case heading(String)
            /// A feed row, clickable.
            case feed(Int64)
            case text, blank
        }

        var id: String
        var role: Role
        /// The cells as drawn. An empty `accessibilityText` hides the row from VoiceOver (a
        /// wrapped paragraph is read once, from its first row).
        var cells: CellLine
        /// The inspector's status, drawn over two rows by `DoubleHeight` (phase 9).
        var isDoubleHeight = false
        /// Reverse video of the selected feed row.
        var isSelected = false

        /// The characters drawn, `?` replacements included.
        var text: String { cells.spans.map { $0.cells.runs.map(\.text).joined() }.joined() }
    }

    // MARK: Entry column

    /// The title row, the progress row (P800 only), then one heading per status in urgency order,
    /// each followed by its feeds, as `GardenTable` groups them. `retained` is what the queue
    /// keeps after a decision (`seenInQueue ∪ decidedThisVisit`). Every row is `cells` wide
    /// (`EntryPageColumn.cells` in the window).
    static func entryLines(_ rows: [GardenRow], scope: GardenScope, retained: Set<Int64>, selected: Int64?,
                           cells: Int) -> [Line] {
        let visible = GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained)
        let sections = GardenModel.sections(visible, scope: scope)
        var lines = [titleLine(rows, scope: scope, visibleCount: visible.count, cells: cells)]
        if scope == .needsTriage {
            lines.append(progressLine(GardenModel.progress(rows), cells: cells))
        }
        guard !sections.isEmpty else {
            let message = GardenModel.emptyMessage(for: scope)
            var empty = CellLine.centred(message.uppercased(), width: cells, foreground: .cyan)
            empty.accessibilityText = message
            lines.append(blankLine(id: "blank", cells: cells))
            lines.append(Line(id: "empty", role: .text, cells: empty))
            return lines
        }
        for section in sections {
            lines.append(headingLine(section, cells: cells))
            for row in section.rows {
                lines.append(feedLine(row, selected: row.feedId == selected, cells: cells))
            }
        }
        return lines
    }

    /// The feeds in the order the page lists them: what the arrows walk and the selection follows.
    static func orderedRows(_ rows: [GardenRow], scope: GardenScope, retained: Set<Int64>) -> [GardenRow] {
        GardenModel.sections(GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained), scope: scope)
            .flatMap(\.rows)
    }

    /// "NEEDS TRIAGE … 68 LEFT", "ALL FEEDS … 186 FEEDS", "CEMETERY … 1 FEED".
    static func titleLine(_ rows: [GardenRow], scope: GardenScope, visibleCount: Int, cells: Int) -> Line {
        let count: String
        switch scope {
        case .needsTriage: count = "\(GardenModel.progress(rows).left) LEFT"
        case .all, .cemetery: count = visibleCount == 1 ? "1 FEED" : "\(visibleCount) FEEDS"
        }
        let title = GardenModel.title(for: scope)
        let countCells = min(count.count + 1, max(0, cells))
        return Line(id: "title", role: .title, cells: CellLine([
            CellSpan(title.uppercased(), cells: cells - countCells, foreground: .white),
            CellSpan(" " + count, cells: countCells, foreground: .yellow),
        ], accessibilityText: "\(title), \(count.lowercased())"))
    }

    /// P800's mosaic gauge, replacing the native capsule: "▐███░░░ 12/80", feeds triaged over feeds
    /// with a problem status, exactly `cells` wide. Blue is allowed here: the shade is a mosaic.
    static func progressLine(_ progress: GardenModel.Progress, cells: Int) -> Line {
        let done = max(0, progress.total - progress.left)
        let label = " \(done)/\(progress.total)"
        let labelCells = min(label.count, max(0, cells - 1))
        let bar = max(0, cells - 1 - labelCells)
        let filled = progress.total > 0 ? min(bar, bar * done / progress.total) : 0
        let spans = [
            CellSpan("▐", cells: min(1, max(0, cells)), foreground: .cyan),
            CellSpan(String(repeating: "█", count: filled), cells: filled, foreground: .green),
            CellSpan(String(repeating: "░", count: bar - filled), cells: bar - filled, foreground: .blue),
            CellSpan(label, cells: labelCells, foreground: .yellow),
        ]
        return Line(id: "progress", role: .progress,
                    cells: CellLine(spans.filter { $0.cellCount > 0 }, accessibilityText: "\(done) of \(progress.total) triaged"))
    }

    /// A status heading in its teletext colour. Fossil's colour is blue, and blue never colours
    /// text, so its heading is reverse video: white on blue.
    static func headingStyle(for status: String) -> (foreground: TeletextColor, background: TeletextColor) {
        let color = GardenStatusStyle.teletextColor(for: status)
        return color == .blue ? (.white, .blue) : (color, .black)
    }

    /// " DEAD … 8".
    static func headingLine(_ section: GardenModel.Section, cells: Int) -> Line {
        let style = headingStyle(for: section.id)
        let count = " \(section.rows.count)"
        let countCells = min(count.count, max(0, cells))
        return Line(id: "heading-\(section.id)", role: .heading(section.id), cells: CellLine([
            CellSpan(" " + section.id.uppercased(), cells: cells - countCells, foreground: style.foreground,
                     background: style.background),
            CellSpan(count, cells: countCells, foreground: style.foreground, background: style.background),
        ], accessibilityText: section.rows.count == 1 ? "\(section.id), 1 feed" : "\(section.id), \(section.rows.count) feeds"))
    }

    /// "● Title … bury": a yellow dot while the feed needs triage, the title, then the chip (the
    /// proposed action in cyan, or the decision once made in green), as `EntryPageColumn` lays out
    /// an entry. Black on white when selected; a `?` replacement stays magenta (`CellSpan`).
    static func feedLine(_ row: GardenRow, selected: Bool, cells: Int) -> Line {
        let decided = row.lastDecision != nil && !row.needsTriage
        let chip = GardenModel.chip(for: row).map { " " + $0 } ?? ""
        let markerCells = min(2, max(0, cells))
        let chipCells = min(chip.count, max(0, cells - markerCells))
        let spoken = [row.title, GardenModel.status(of: row), GardenModel.chip(for: row)].compactMap { $0 }
        let line = CellLine([
            CellSpan(row.needsTriage ? "●" : "", cells: markerCells, foreground: .yellow),
            CellSpan(row.title, cells: cells - markerCells - chipCells, foreground: .white),
            CellSpan(chip, cells: chipCells, foreground: decided ? .green : .cyan),
        ], accessibilityText: spoken.joined(separator: ", "))
        return Line(id: "feed-\(row.feedId)", role: .feed(row.feedId),
                    cells: selected ? line.reversed(.black, on: .white) : line, isSelected: selected)
    }

    static func blankLine(id: String, cells: Int) -> Line {
        Line(id: id, role: .blank, cells: .blank(max(0, cells)))
    }

    // MARK: Keys

    /// `↑` and `↓` walk the garden list unless the index column is active: its arrows pick pages.
    static func takesArrows(column: AntiopeColumn) -> Bool {
        column != .index
    }

    /// The next (`delta` 1) or previous (-1) feed, stopping at the ends like the native list.
    /// Without a selection, the first (or last) feed.
    static func move(_ selected: Int64?, in ordered: [GardenRow], by delta: Int) -> Int64? {
        guard !ordered.isEmpty else { return selected }
        guard let selected, let index = ordered.firstIndex(where: { $0.feedId == selected }) else {
            return (delta >= 0 ? ordered.first : ordered.last)?.feedId
        }
        return ordered[min(max(index + delta, 0), ordered.count - 1)].feedId
    }
}
