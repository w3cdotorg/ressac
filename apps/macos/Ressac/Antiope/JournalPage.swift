import SwiftUI

/// P104 JOURNAL as teletext rows (spec §8, Antiope): pure functions over the edition, so that
/// grouping, colours and widths are tested here and `JournalEntryPage` only draws what they return.
enum JournalPage {
    struct Line: Identifiable, Equatable, Sendable {
        enum Role: Equatable, Sendable {
            case header, message, blank, end
            case heading(EditionSection)
            /// An edition entry, clickable; every row of a wrapped title carries it.
            case entry(Int64)
            /// `+12 MORE IN …`, clickable: the feed it opens, and that feed's page.
            case overflow(feedId: Int64, page: Int?)
        }

        var id: String
        var role: Role
        /// The cells as drawn. An empty `accessibilityText` hides the row from VoiceOver (a
        /// wrapped title is read once, from its first row).
        var cells: CellLine
        /// `FIN DE L'EDITION`, drawn over two rows by `DoubleHeight`.
        var isDoubleHeight = false
        /// Reverse video of the selected entry.
        var isSelected = false

        /// The characters drawn, `?` replacements included.
        var text: String { cells.spans.map { $0.cells.runs.map(\.text).joined() }.joined() }
    }

    /// Section headings (spec §8): FRONT PAGE yellow, THIS WEEK cyan, THE WIRE green, on black.
    static func colour(for section: EditionSection) -> TeletextColor {
        switch section {
        case .front: .yellow
        case .week: .cyan
        case .wire: .green
        }
    }

    /// The entry column of P104 (spec §8, §9). While the edition builds, only its progress; offline
    /// or failed, why there is none; else the header, then each section with entries (heading,
    /// entries in rank order, each feed's overflow row under its last entry), then
    /// `FIN DE L'EDITION` once every entry is read. Every row is exactly `cells` wide.
    @MainActor static func lines(edition: Edition?, phase: JournalModel.Phase, selected: Int64?, cells: Int,
                      timeZone: TimeZone = .current) -> [Line] {
        switch phase {
        case let .building(done, total):
            return message(progressText(done: done, total: total),
                           spoken: "Building this morning's edition…" + (total > 0 ? " \(done)/\(total)" : ""),
                           colour: .yellow, cells: cells)
        case .offline:
            return message("OFFLINE: NO EDITION YET", spoken: "Offline: no edition yet", colour: .red, reversed: true,
                           cells: cells)
                + [Line(id: "retry", role: .message,
                        cells: .centred("RÉPÉTITION: RETRY", width: max(0, cells), foreground: .cyan))]
        case let .failed(reason):
            return message(reason.uppercased(), spoken: reason, colour: .red, reversed: true, cells: cells)
        case .idle, .due:
            break
        }
        guard let edition else {
            return message(progressText(done: 0, total: 0), spoken: "Building this morning's edition…", colour: .yellow,
                           cells: cells)
        }
        var lines = headerLines(edition, cells: cells, timeZone: timeZone)
        guard !edition.items.isEmpty else {
            return lines + [blankLine(id: "blank-empty", cells: cells),
                            Line(id: "empty", role: .message,
                                 cells: EntryPageColumn.emptyLine("Nothing new this morning.", width: max(0, cells)))]
        }
        let items = edition.items.sorted { $0.rank < $1.rank }
        for section in EditionSection.allCases {
            let entries = items.filter { $0.section == section }
            let overflow = edition.overflow.filter { $0.section == section }
            guard !entries.isEmpty || !overflow.isEmpty else { continue }
            lines.append(blankLine(id: "blank-\(section.rawValue)", cells: cells))
            lines.append(headingLine(section, count: entries.count, cells: cells))
            var placed = Set<Int64>()
            for (index, entry) in entries.enumerated() {
                lines += entryLines(entry, selected: entry.entryId == selected, cells: cells)
                let lastOfFeed = index + 1 == entries.count || entries[index + 1].feedId != entry.feedId
                if lastOfFeed, let more = overflow.first(where: { $0.feedId == entry.feedId }) {
                    placed.insert(more.feedId)
                    lines.append(overflowLine(more, cells: cells))
                }
            }
            // A cap of 0 (`edition build --cap-wire 0`) leaves overflow with no entry above it.
            for more in overflow where !placed.contains(more.feedId) {
                lines.append(overflowLine(more, cells: cells))
            }
        }
        if edition.isFinished {
            lines += [blankLine(id: "blank-end", cells: cells), endLine(cells: cells)]
        }
        return lines
    }

    /// `FRI 16 OCT` in cyan, `12/43 READ` in yellow at the right edge; a second row
    /// `3 FEEDS UNREACHABLE` in red when some feeds failed (spec §9). VoiceOver reads the native
    /// header (`Friday 16 October · 43 entries · 12 read`).
    static func headerLines(_ edition: Edition, cells: Int, timeZone: TimeZone) -> [Line] {
        let width = max(0, cells)
        let built = edition.info.builtAt
        let day = TeletextDateFormat.string(from: built, pattern: "EEE dd MMM", timeZone: timeZone).uppercased()
        let count = " \(edition.readCount)/\(edition.items.count) READ"
        let countCells = min(count.count, width)
        let entries = edition.items.count == 1 ? "1 entry" : "\(edition.items.count) entries"
        let spoken = TeletextDateFormat.string(from: built, pattern: "EEEE d MMMM", timeZone: timeZone)
            + " · \(entries) · \(edition.readCount) read"
        var lines = [Line(id: "header", role: .header, cells: CellLine([
            CellSpan(" " + day, cells: width - countCells, foreground: .cyan),
            CellSpan(count, cells: countCells, foreground: .yellow),
        ], accessibilityText: spoken))]
        let unreachable = edition.info.unreachable
        if unreachable > 0 {
            let text = unreachable == 1 ? "1 feed unreachable" : "\(unreachable) feeds unreachable"
            lines.append(Line(id: "unreachable", role: .header, cells: CellLine([
                CellSpan(" " + text.uppercased(), cells: width, foreground: .red),
            ], accessibilityText: text)))
        }
        return lines
    }

    /// ` FRONT PAGE … 2`, in the section's colour on black.
    static func headingLine(_ section: EditionSection, count: Int, cells: Int) -> Line {
        let width = max(0, cells)
        let colour = colour(for: section)
        let number = " \(count)"
        let numberCells = min(number.count, width)
        return Line(id: "heading-\(section.rawValue)", role: .heading(section), cells: CellLine([
            CellSpan(" " + section.antiopeTitle, cells: width - numberCells, foreground: colour),
            CellSpan(number, cells: numberCells, foreground: colour),
        ], accessibilityText: count == 1 ? "\(section.title), 1 entry" : "\(section.title), \(count) entries"))
    }

    /// As `EntryPageColumn.lines(for:width:isSelected:)`: a yellow `●` when unread, the title over
    /// two rows at most, a white `*` when starred, markers on the first row only; black on white
    /// when selected. VoiceOver reads "Title, Feed, unread, starred" on the first row.
    @MainActor static func entryLines(_ item: EditionItem, selected: Bool, cells: Int) -> [Line] {
        let width = max(0, cells)
        let title = item.title?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "Untitled"
        let markerCells = min(2, width)
        let titleCells = max(0, width - 4)
        let starCells = max(0, width - markerCells - titleCells)
        let spoken = [title, item.feedTitle, item.isRead ? nil : "unread", item.isStarred ? "starred" : nil]
            .compactMap { $0 }.joined(separator: ", ")
        return EntryPageColumn.wrap(title, width: titleCells).enumerated().map { index, text in
            let line = CellLine([
                CellSpan(index == 0 && !item.isRead ? "●" : "", cells: markerCells, foreground: .yellow),
                CellSpan(text, cells: titleCells, foreground: .white),
                CellSpan(index == 0 && item.isStarred ? " *" : "", cells: starCells, foreground: .white),
            ], accessibilityText: index == 0 ? spoken : "")
            return Line(id: "entry-\(item.entryId)-\(index)", role: .entry(item.entryId),
                        cells: selected ? line.reversed(.black, on: .white) : line, isSelected: selected)
        }
    }

    /// `  +12 MORE IN ACTUALITTÉ`: white on blue (blue never colours text, as the Fossil heading),
    /// two cells in, under the entry titles. VoiceOver reads the native `+12 more in Actualitté`.
    static func overflowLine(_ overflow: EditionOverflowItem, cells: Int) -> Line {
        let width = max(0, cells)
        let indent = min(2, width)
        return Line(id: "overflow-\(overflow.section.rawValue)-\(overflow.feedId)",
                    role: .overflow(feedId: overflow.feedId, page: overflow.page),
                    cells: CellLine([
                        CellSpan("", cells: indent, foreground: .white),
                        CellSpan("+\(overflow.hiddenCount) MORE IN \(overflow.feedTitle.uppercased())",
                                 cells: width - indent, foreground: .white, background: .blue),
                    ], accessibilityText: "+\(overflow.hiddenCount) more in \(overflow.feedTitle)"))
    }

    /// The Minitel exception (spec §2): `FIN DE L'EDITION`, yellow, centred, drawn in double height.
    static func endLine(cells: Int) -> Line {
        var line = CellLine.centred("FIN DE L'EDITION", width: max(0, cells), foreground: .yellow)
        line.accessibilityText = "That's the edition."
        return Line(id: "end", role: .end, cells: line, isDoubleHeight: true)
    }

    /// P104's progress row and the Minitel bar's status during the build.
    static func progressText(done: Int, total: Int) -> String {
        total > 0 ? "EDITION IN PROGRESS \(done)/\(total)" : "EDITION IN PROGRESS"
    }

    /// The edition's last entry by rank: SUITE past it lands on FIN DE L'EDITION.
    static func lastEntryId(of edition: Edition?) -> Int64? {
        edition?.items.max { $0.rank < $1.rank }?.entryId
    }

    static func blankLine(id: String, cells: Int) -> Line {
        Line(id: id, role: .blank, cells: .blank(max(0, cells)))
    }

    /// A blank row, then one centred row: `colour` on black, or black on `colour` when `reversed`.
    private static func message(_ text: String, spoken: String, colour: TeletextColor, reversed: Bool = false,
                                cells: Int) -> [Line] {
        var line = CellLine.centred(text, width: max(0, cells), foreground: colour)
        if reversed { line = line.reversed(.black, on: colour) }
        line.accessibilityText = spoken
        return [blankLine(id: "blank-message", cells: cells), Line(id: "message", role: .message, cells: line)]
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
