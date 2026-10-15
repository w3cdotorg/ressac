import SwiftUI

/// The index column (spec §5, Window): every page, 22 cells wide. A click goes to the page.
struct PageIndexColumn: View {
    static let cells = 22

    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        let directory = antiope.directory
        let current = antiope.currentPage
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(directory.rows, id: \.number) { row in
                        let selected = row.number == current
                        let line = Self.line(for: row, width: Self.cells,
                                             isRootFolder: Self.isRootFolder(row, in: directory), isSelected: selected)
                        Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
                            .buttonStyle(.plain)
                            .focusable(false)  // Full Keyboard Access would give Space to the button, not to SUITE
                            .accessibilityLabel(line.accessibilityText)
                            .accessibilityAddTraits(selected ? .isSelected : [])
                            .id(row.number)
                    }
                }
            }
            .scrollIndicators(.hidden)
            .onChange(of: current) { _, page in
                if let page { proxy.scrollTo(page) }
            }
        }
    }

    /// `101 Unread         1083`: the number in cyan, the title indented by depth, the count in
    /// yellow at the right edge. Root folders in green capitals, the Garden in magenta with P800's
    /// triage count in red; P100 colours the other reserved titles yellow (`highlightsReserved`).
    /// Selected: black on cyan. Always exactly `width` cells.
    static func line(for row: PageDirectory.Row, width: Int, isRootFolder: Bool, isSelected: Bool,
                     highlightsReserved: Bool = false) -> CellLine {
        let reserved = row.isReserved ? ReservedPage(rawValue: row.number) : nil
        let count = row.unread > 0 ? String(min(row.unread, 99_999)) : ""
        let numberCells = min(4, max(0, width))
        let countCells = min(count.isEmpty ? 0 : count.count + 1, max(0, width - numberCells))
        let titleCells = max(0, width - numberCells - countCells)
        let indent = String(repeating: " ", count: min(row.depth, 3))
        let title = isRootFolder ? row.title.uppercased() : row.title
        let titleColour: TeletextColor =
            if reserved?.isGarden == true { .magenta }
            else if reserved != nil, highlightsReserved { .yellow }
            else if isRootFolder { .green }
            else { .white }
        let line = CellLine([
            CellSpan("\(row.number) ", cells: numberCells, foreground: .cyan),
            CellSpan(indent + title, cells: titleCells, foreground: titleColour),
            CellSpan(count.isEmpty ? "" : " " + count, cells: countCells, foreground: reserved == .needsTriage ? .red : .yellow),
        ], accessibilityText: "\(row.title), page \(row.number)" + (row.unread > 0 ? ", \(row.unread)" : ""))
        return isSelected ? line.reversed(.black, on: .cyan) : line
    }

    /// A folder at the top of the tree (green capitals in the index).
    static func isRootFolder(_ row: PageDirectory.Row, in directory: PageDirectory) -> Bool {
        guard row.depth == 0, case .source(.folder(_))? = directory.destination(for: row.number) else { return false }
        return true
    }
}

/// The entry column (spec §5, Window): the entries of the selected page, 28 cells wide.
struct EntryPageColumn: View {
    static let cells = 28

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(app.entries) { entry in
                        let selected = entry.entryId == app.selectedEntryId
                        Button { open(entry) } label: {
                            VStack(alignment: .leading, spacing: 0) {
                                ForEach(Array(Self.lines(for: entry, width: Self.cells, isSelected: selected).enumerated()),
                                        id: \.offset) { _, line in
                                    CellLineView(line: line)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .focusable(false)
                        .accessibilityLabel(Self.accessibilityLabel(for: entry))
                        .accessibilityAddTraits(selected ? .isSelected : [])
                        .id(entry.entryId)
                    }
                }
            }
            .scrollIndicators(.hidden)
            .overlay {
                if app.entries.isEmpty {
                    CellLineView(line: Self.emptyLine(app.emptyListMessage, width: Self.cells))
                }
            }
            // `j`, `k` and Space select from the keyboard: the column follows the selection.
            .onChange(of: app.selectedEntryId) { _, id in
                if let id { proxy.scrollTo(id) }
            }
        }
    }

    private func open(_ entry: EntryRow) {
        app.selectedEntryId = entry.entryId
        antiope.column = .entries
        antiope.readerPage = .article
    }

    /// One or two rows: a yellow `●` when unread, the title wrapped over two rows at most, a white
    /// `*` when starred, the markers on the first row only. Selected: black on white. Every row is
    /// exactly `width` cells.
    static func lines(for entry: EntryRow, width: Int, isSelected: Bool) -> [CellLine] {
        let markerCells = min(2, max(0, width))
        let titleCells = max(0, width - 4)
        let starCells = max(0, width - markerCells - titleCells)
        let lines = wrap(entry.displayTitle, width: titleCells).enumerated().map { index, text in
            CellLine([
                CellSpan(index == 0 && !entry.isRead ? "●" : "", cells: markerCells, foreground: .yellow),
                CellSpan(text, cells: titleCells, foreground: .white),
                CellSpan(index == 0 && entry.isStarred ? " *" : "", cells: starCells, foreground: .white),
            ], accessibilityText: entry.displayTitle)
        }
        return isSelected ? lines.map { $0.reversed(.black, on: .white) } : lines
    }

    /// The title over two rows at most, words separated by single spaces: the first row ends at
    /// the last word that fits (a first word longer than the row is cut), the second takes the
    /// rest, which `CellText.fit` truncates.
    static func wrap(_ title: String, width: Int) -> [String] {
        let words = title.split(whereSeparator: \.isWhitespace).map(String.init)
        let text = words.joined(separator: " ")
        guard width > 0, text.count > width else { return [text] }
        var first = ""
        var used = 0
        for word in words {
            let candidate = first.isEmpty ? word : first + " " + word
            guard candidate.count <= width else { break }
            first = candidate
            used += 1
        }
        guard !first.isEmpty else { return [String(text.prefix(width)), String(text.dropFirst(width))] }
        return [first, words[used...].joined(separator: " ")]
    }

    /// "Title, Feed, unread, starred": what VoiceOver reads for an entry.
    static func accessibilityLabel(for entry: EntryRow) -> String {
        var parts = [entry.displayTitle, entry.feedTitle]
        if !entry.isRead { parts.append("unread") }
        if entry.isStarred { parts.append("starred") }
        return parts.joined(separator: ", ")
    }

    /// `NO UNREAD ENTRIES`: `AppState.emptyListMessage` in capitals, one cyan line, centred
    /// (spec §5); VoiceOver reads the sentence as written.
    static func emptyLine(_ message: String, width: Int) -> CellLine {
        var line = CellLine.centred(message.uppercased(), width: width, foreground: .cyan)
        line.accessibilityText = message
        return line
    }
}

/// The one-row title at the top of each column, as on a Minitel page: the focused column's in
/// reverse video (black on cyan), the others cyan on black. Exactly as wide as the column. The
/// entries title also shows the filter, `ENTRIES  ALL` or `ENTRIES  UNREAD`.
enum ColumnTitle {
    static func label(for column: AntiopeColumn) -> String {
        switch column {
        case .index: "INDEX"
        case .entries: "ENTRIES"
        case .reader: "READER"
        }
    }

    /// `unreadOnly` is nil where the filter does not apply (a Garden page, other columns).
    static func line(for column: AntiopeColumn, isActive: Bool, width: Int, unreadOnly: Bool? = nil) -> CellLine {
        var label = label(for: column)
        var spoken = label.capitalized
        if let unreadOnly {
            label += unreadOnly ? "  UNREAD" : "  ALL"
            spoken += unreadOnly ? ", unread only" : ", all entries"
        }
        let line = CellLine([CellSpan(" " + label, cells: width, foreground: .cyan)], accessibilityText: spoken)
        return isActive ? line.reversed(.black, on: .cyan) : line
    }
}

/// A column's title row, following `AntiopeState.column` live (keys and clicks alike). The entries
/// title is also the All / Unread switch (the View menu item does the same).
struct ColumnTitleView: View {
    let column: AntiopeColumn
    let width: Int

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        let isActive = antiope.column == column
        let filters = column == .entries && UnreadFilterCommand.canToggle(app)
        let line = ColumnTitle.line(for: column, isActive: isActive, width: width, unreadOnly: filters ? app.unreadOnly : nil)
        if filters {
            Button { UnreadFilterCommand.toggle(app) } label: { CellLineView(line: line) }
                .buttonStyle(.plain)
                .focusable(false)
                .accessibilityHint("Switches between all entries and unread only")
                .accessibilityAddTraits(isActive ? .isSelected : [])
        } else {
            CellLineView(line: line).accessibilityAddTraits(isActive ? .isSelected : [])
        }
    }
}
