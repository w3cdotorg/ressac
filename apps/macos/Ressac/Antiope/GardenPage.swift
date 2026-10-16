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
            /// Under the rhythm line: where `GardenInspectorPage` draws the override rows.
            case rhythm
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

    /// "● Title … bury": a yellow dot while the feed needs triage, the title, a buried feed's unread
    /// count in yellow (as the index shows counts), then the chip (the proposed action in cyan, or
    /// the decision once made in green), as `EntryPageColumn` lays out an entry. Black on white when
    /// selected; a `?` replacement stays magenta (`CellSpan`).
    static func feedLine(_ row: GardenRow, selected: Bool, cells: Int) -> Line {
        let decided = row.lastDecision != nil && !row.needsTriage
        let chip = GardenModel.chip(for: row).map { " " + $0 } ?? ""
        let unreadLabel = GardenModel.unreadLabel(for: row)
        let count = unreadLabel == nil ? "" : " \(min(row.unread, 99_999))"
        let markerCells = min(2, max(0, cells))
        let chipCells = min(chip.count, max(0, cells - markerCells))
        let countCells = min(count.count, max(0, cells - markerCells - chipCells))
        let spoken = [row.title, GardenModel.status(of: row), unreadLabel, GardenModel.chip(for: row)].compactMap { $0 }
        var spans = [
            CellSpan(row.needsTriage ? "●" : "", cells: markerCells, foreground: .yellow),
            CellSpan(row.title, cells: cells - markerCells - countCells - chipCells, foreground: .white),
        ]
        if !count.isEmpty {
            spans.append(CellSpan(count, cells: countCells, foreground: .yellow))
        }
        spans.append(CellSpan(chip, cells: chipCells, foreground: decided ? .green : .cyan))
        let line = CellLine(spans, accessibilityText: spoken.joined(separator: ", "))
        return Line(id: "feed-\(row.feedId)", role: .feed(row.feedId),
                    cells: selected ? line.reversed(.black, on: .white) : line, isSelected: selected)
    }

    static func blankLine(id: String, cells: Int) -> Line {
        Line(id: id, role: .blank, cells: .blank(max(0, cells)))
    }

    // MARK: Inspector

    /// The garden inspector as a teletext page `cells` wide (spec §8): name and folder path, the
    /// status in double height, then evidence, remarks, links, redirects and the fetch log.
    static func inspectorLines(_ row: GardenRow?, log: [FetchLogRow], cells: Int,
                               timeZone: TimeZone = .current, cadence: CadenceRow? = nil, now: Date = Date()) -> [Line] {
        guard cells > 0 else { return [] }
        var lines: [Line] = []

        func add(_ spans: [CellSpan], role: Line.Role = .text, doubleHeight: Bool = false, spoken: String) {
            lines.append(Line(id: "inspector-\(lines.count)", role: role,
                              cells: CellLine(spans, accessibilityText: spoken), isDoubleHeight: doubleHeight))
        }
        func blank() {
            lines.append(blankLine(id: "inspector-\(lines.count)", cells: cells))
        }
        func heading(_ title: String, _ color: TeletextColor) {
            add([CellSpan(title.uppercased(), cells: cells, foreground: color)], spoken: title)
        }
        /// `text` wrapped on words; `lead` (a label, a date, a dash) starts the first row and the
        /// following rows are indented by its width. Only the first row is read by VoiceOver.
        func paragraph(_ text: String, color: TeletextColor, lead: (text: String, color: TeletextColor)? = nil,
                       spokenLead: String = "") {
            let indent = min(lead?.text.count ?? 0, cells)
            for (index, piece) in wrap(text, cells: cells - indent).enumerated() {
                var spans: [CellSpan] = []
                if let lead {
                    spans.append(CellSpan(index == 0 ? lead.text : "", cells: indent, foreground: lead.color))
                }
                spans.append(CellSpan(piece, cells: cells - indent, foreground: color))
                add(spans, spoken: index == 0 ? spokenLead + text : "")
            }
        }
        func labeled(_ label: String, _ value: String) {
            paragraph(value, color: .white,
                      lead: (label.uppercased().padding(toLength: 10, withPad: " ", startingAt: 0), .cyan),
                      spokenLead: "\(label): ")
        }

        guard let row else {
            blank()
            var none = CellLine.centred("NO FEED SELECTED", width: cells, foreground: .cyan)
            none.accessibilityText = "No feed selected"
            lines.append(Line(id: "inspector-\(lines.count)", role: .text, cells: none))
            return lines
        }

        let status = GardenModel.status(of: row)
        let buried = GardenModel.isBuried(row)
        paragraph(row.title, color: .white)
        if let path = row.folderPath, !path.isEmpty {
            paragraph(path, color: .cyan)
        }
        blank()
        let style = headingStyle(for: status)
        add([CellSpan(" " + status.uppercased(), cells: cells, foreground: style.foreground, background: style.background)],
            role: .heading(status), doubleHeight: true, spoken: status)
        if buried {
            paragraph("Buried. Press k to restore it.", color: .yellow)
        }
        if let reason = row.reason {
            paragraph(reason, color: .white)
        }
        // The rhythm (sub-project 3): the native inspector's line, then the override rows'
        // anchor. Not for a buried feed, which has no rhythm group.
        if let cadence, !buried {
            blank()
            labeled("Rhythm", GardenModel.rhythmText(cadence, now: now))
            lines.append(Line(id: "inspector-rhythm", role: .rhythm, cells: .blank(cells)))
        }
        if let bullets = row.evidence?.bullets, !bullets.isEmpty {
            blank()
            heading("Evidence", .green)
            for bullet in bullets {
                paragraph(bullet, color: .white, lead: ("- ", .green))
            }
        }
        if !row.remarks.isEmpty {
            blank()
            heading("Remarks", .yellow)
            for remark in row.remarks {
                paragraph(GardenModel.remarkSentence(remark), color: .white, lead: ("- ", .yellow))
            }
        }
        blank()
        heading("Links", .cyan)
        if let action = row.proposedAction, !buried {
            labeled("Proposed", action.label)
        }
        if let decision = row.lastDecision {
            labeled("Decision", decision.capitalized)
        }
        labeled("Feed", row.xmlUrl)
        if let site = LinkPolicy.webURL(row.htmlUrl) {
            labeled("Site", site.absoluteString)
        }
        if let wayback = LinkPolicy.webURL(row.waybackUrl) {
            labeled("Wayback", wayback.absoluteString)
        }
        if let chain = log.first?.redirectChain, !chain.isEmpty {
            blank()
            heading("Redirects", .magenta)
            for hop in chain {
                paragraph(hop.url, color: .white, lead: ("\(hop.status) ", .magenta), spokenLead: "\(hop.status) to ")
            }
        }
        blank()
        heading("Fetch log", .green)
        if log.isEmpty {
            paragraph("Not fetched yet.", color: .white)
        }
        for entry in log {
            let date = logDate(entry.at, timeZone: timeZone)
            paragraph(GardenModel.outcome(of: entry), color: .white, lead: (date + "  ", .cyan), spokenLead: "\(date), ")
        }
        return lines
    }

    /// One row of the rhythm override (spec §8, Inspector): `nil` is Auto.
    struct CadenceChoice: Identifiable, Equatable, Sendable {
        var group: CadenceGroup?
        var line: Line
        var id: String { line.id }
    }

    /// `AUTO`, then the five groups, one row each, cyan; the choice in force (Auto unless the feed
    /// is overridden) in reverse video, black on cyan. Every row is exactly `cells` wide.
    static func cadenceChoices(_ cadence: CadenceRow, cells: Int) -> [CadenceChoice] {
        let width = max(0, cells)
        let choices: [CadenceGroup?] = [nil] + CadenceGroup.allCases.map { Optional($0) }
        return choices.map { group in
            let title = group?.title ?? "Auto"
            let current = cadence.isOverride ? group == cadence.effective : group == nil
            let line = CellLine([CellSpan(" " + title.uppercased(), cells: width, foreground: .cyan)],
                                accessibilityText: current ? "\(title), current" : title)
            return CadenceChoice(group: group, line: Line(id: "cadence-\(group?.rawValue ?? "auto")", role: .text,
                                                          cells: current ? line.reversed(.black, on: .cyan) : line,
                                                          isSelected: current))
        }
    }

    /// Greedy word wrap into rows of at most `cells` characters; a word longer than a row (a URL)
    /// is cut into full rows.
    static func wrap(_ text: String, cells: Int) -> [String] {
        guard cells > 0 else { return [] }
        var rows: [String] = []
        var current = ""
        for piece in text.split(whereSeparator: \.isWhitespace) {
            var word = String(piece)
            while word.count > cells {
                if !current.isEmpty {
                    rows.append(current)
                    current = ""
                }
                rows.append(String(word.prefix(cells)))
                word = String(word.dropFirst(cells))
            }
            if current.isEmpty {
                current = word
            } else if current.count + 1 + word.count <= cells {
                current += " " + word
            } else {
                rows.append(current)
                current = word
            }
        }
        if !current.isEmpty {
            rows.append(current)
        }
        return rows
    }

    /// "10 OCT 26 07:00": fetch log dates, English month in capitals, in `timeZone`.
    static func logDate(_ date: Date, timeZone: TimeZone) -> String {
        TeletextDateFormat.string(from: date, pattern: "dd MMM yy HH:mm", timeZone: timeZone).uppercased()
    }

    // MARK: Banner

    /// `RunCoordinator`'s banner as one red reverse-video row (spec §8): the message in white,
    /// then the clickable keys in yellow, all on red, exactly the window's width.
    struct BannerRow: Equatable, Sendable {
        var message: CellLine
        /// ` LOG `, when the log explains the failure.
        var log: CellLine?
        /// ` X `, to dismiss.
        var dismiss: CellLine

        var width: Int { message.width + (log?.width ?? 0) + dismiss.width }
    }

    static func bannerRow(_ banner: Banner, cells: Int) -> BannerRow {
        func key(_ label: String, spoken: String) -> CellLine {
            CellLine([CellSpan(label, cells: label.count, foreground: .yellow, background: .red)], accessibilityText: spoken)
        }
        let log = banner.showLog ? key(" LOG ", spoken: "Show Log") : nil
        let dismiss = key(" X ", spoken: "Dismiss")
        let messageCells = max(0, cells - (log?.width ?? 0) - dismiss.width)
        return BannerRow(message: CellLine([CellSpan(" " + banner.message, cells: messageCells, foreground: .white,
                                                     background: .red)],
                                           accessibilityText: banner.message),
                         log: log, dismiss: dismiss)
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
