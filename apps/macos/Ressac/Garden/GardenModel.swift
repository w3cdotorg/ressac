import Foundation
import SwiftUI

/// What a key or button asks the Garden to do (mirrors `ressac garden`, crates/ressac/src/tui/state.rs).
enum GardenCommand: Equatable {
    case decide(GardenDecision)
    case undo
    case open(URL)
    case confirmSafeRepairs
    case message(String)
}

/// Pure Garden presentation logic: grouping, progress, labels and keys.
enum GardenModel {
    /// Core `Status::ALL`: the diagnosis order, which is also the garden's urgency order.
    static let urgencyOrder = ["Duplicate", "Hijacked", "Moved", "Dead", "Ailing", "Fossil", "Dormant", "Alive", "Unchecked"]
    /// Core `Status::needs_triage`.
    static let triageStatuses: Set<String> = ["Duplicate", "Hijacked", "Moved", "Dead", "Ailing", "Fossil"]

    struct Section: Identifiable, Equatable {
        let id: String
        let glyph: String
        let rows: [GardenRow]

        /// "🪦 DEAD · 8", as in the mockup.
        var header: String { "\(glyph) \(id.uppercased()) · \(rows.count)" }
    }

    struct Progress: Equatable {
        let left: Int
        let total: Int
    }

    static func status(of row: GardenRow) -> String { row.status ?? "Unchecked" }
    static func isBuried(_ row: GardenRow) -> Bool { row.state == "buried" }

    /// Same glyphs as the terminal (crates/ressac/src/output.rs).
    static func glyph(_ status: String) -> String {
        switch status {
        case "Duplicate": "👯"
        case "Hijacked": "🦠"
        case "Moved": "🚚"
        case "Dead": "🪦"
        case "Ailing": "🤒"
        case "Fossil": "🦴"
        case "Dormant": "🌙"
        case "Alive": "🌿"
        case "Buried": "⚰️"
        default: "❔"
        }
    }

    static func title(for scope: GardenScope) -> String {
        switch scope {
        case .needsTriage: "Needs Triage"
        case .all: "All Feeds"
        case .cemetery: "Cemetery"
        }
    }

    static func emptyMessage(for scope: GardenScope) -> String {
        switch scope {
        case .needsTriage: "Nothing to triage"
        case .all: "No feeds"
        case .cemetery: "The cemetery is empty"
        }
    }

    /// Rows of the current list. In the triage queue, feeds that left it during this visit
    /// (`seenInQueue`) stay, dimmed, until the list is left (spec §7).
    static func visibleRows(_ rows: [GardenRow], scope: GardenScope, seenInQueue: Set<Int64>) -> [GardenRow] {
        switch scope {
        case .needsTriage: rows.filter { $0.needsTriage || seenInQueue.contains($0.feedId) }
        // Active feeds, plus the ones buried during this visit (kept in place, dimmed).
        case .all: rows.filter { !isBuried($0) || seenInQueue.contains($0.feedId) }
        case .cemetery: rows.filter(isBuried)
        }
    }

    static func isDimmed(_ row: GardenRow, scope: GardenScope, seenInQueue: Set<Int64>) -> Bool {
        switch scope {
        case .needsTriage: !row.needsTriage && seenInQueue.contains(row.feedId)
        case .all: isBuried(row) && seenInQueue.contains(row.feedId)
        case .cemetery: false
        }
    }

    /// One section per status in urgency order. The Cemetery lists buried feeds alone; elsewhere a
    /// feed buried during the visit stays dimmed in its status section.
    static func sections(_ rows: [GardenRow], scope: GardenScope) -> [Section] {
        let splitBuried = scope == .cemetery
        let buried = splitBuried ? rows.filter(isBuried) : []
        let live = splitBuried ? rows.filter { !isBuried($0) } : rows
        let extra = Set(live.map(status(of:))).subtracting(urgencyOrder).sorted()
        var result = (urgencyOrder + extra).compactMap { name -> Section? in
            let members = live.filter { status(of: $0) == name }.sorted(by: rowOrder)
            return members.isEmpty ? nil : Section(id: name, glyph: glyph(name), rows: members)
        }
        if !buried.isEmpty {
            result.append(Section(id: "Buried", glyph: glyph("Buried"), rows: buried.sorted(by: rowOrder)))
        }
        return result
    }

    /// Core `load_rows` order inside a status: top folder, then title, case-insensitive.
    private static func rowOrder(_ a: GardenRow, _ b: GardenRow) -> Bool {
        let keyA = (topFolder(a), a.title.lowercased())
        let keyB = (topFolder(b), b.title.lowercased())
        return keyA < keyB
    }

    private static func topFolder(_ row: GardenRow) -> String {
        row.folderPath?.components(separatedBy: " / ").first ?? ""
    }

    /// "N of M left": M counts every feed with a problem status (buried ones count as done),
    /// N those still needing triage, as the terminal garden's `progress`.
    static func progress(_ rows: [GardenRow]) -> Progress {
        Progress(left: rows.filter(\.needsTriage).count,
                 total: rows.filter { triageStatuses.contains(status(of: $0)) }.count)
    }

    /// Core `safe_repairs`: undecided active feeds whose proposed action is safe.
    static func safeRepairs(_ rows: [GardenRow]) -> [GardenRow] {
        rows.filter { row in
            !isBuried(row) && row.lastDecision == nil && row.proposedAction?.isSafe == true
        }
    }

    /// "Apply 1 Safe Repair", "Apply 87 Safe Repairs" (button title case).
    static func applyButtonTitle(_ count: Int) -> String {
        count == 1 ? "Apply 1 Safe Repair" : "Apply \(count) Safe Repairs"
    }

    /// "Apply 87 safe repairs?" (confirmation title).
    static func applyQuestion(_ count: Int) -> String {
        count == 1 ? "Apply 1 safe repair?" : "Apply \(count) safe repairs?"
    }

    /// The confirmation text for ⇧R: the first eight repairs, then a count.
    static func safeRepairPreview(_ repairs: [GardenRow]) -> String {
        var lines = repairs.prefix(8).compactMap { row in
            row.proposedAction.map { "\(row.title): \($0.label)" }
        }
        if repairs.count > 8 {
            lines.append("and \(repairs.count - 8) more.")
        }
        return lines.joined(separator: "\n")
    }

    /// After the database changed: when the selected feed just left the queue, select the next
    /// feed (in list order) that still needs triage, like the terminal garden's cursor.
    static func selectionAfterUpdate(selected: Int64?, previous: [GardenRow], current: [GardenRow], ordered: [GardenRow]) -> Int64? {
        guard let selected,
              previous.first(where: { $0.feedId == selected })?.needsTriage == true,
              current.first(where: { $0.feedId == selected })?.needsTriage != true,
              let index = ordered.firstIndex(where: { $0.feedId == selected })
        else { return selected }
        let after = ordered[ordered.index(after: index)...] + ordered[..<index]
        return after.first(where: \.needsTriage)?.feedId ?? selected
    }

    /// Keys of the terminal garden. `nil` means the key is not the Garden's. Chords with ⌘, ⌃
    /// or ⌥ never are (Shift stays: ⇧R), so a stray ⌘B cannot bury a feed.
    static func command(for key: String, modifiers: EventModifiers = [], row: GardenRow?, safeRepairCount: Int) -> GardenCommand? {
        guard modifiers.isDisjoint(with: [.command, .control, .option]) else { return nil }
        switch key {
        case "u":
            return .undo
        case "R":
            return safeRepairCount > 0 ? .confirmSafeRepairs : .message("No safe repairs pending.")
        case "k", "r", "b", "o", "w":
            guard let row else { return .message("Select a feed first.") }
            return rowCommand(for: key, row: row)
        default:
            return nil
        }
    }

    private static func rowCommand(for key: String, row: GardenRow) -> GardenCommand {
        switch key {
        case "k":
            return .decide(.keep)
        case "b" where isBuried(row), "r" where isBuried(row):
            return .message("This feed is buried; press k to restore it first.")
        case "b":
            return .decide(.bury)
        case "r":
            return row.proposedAction?.isRepair == true ? .decide(.repair) : .message("Nothing to repair for this feed.")
        case "o":
            return open(row.htmlUrl, missing: "This feed has no site address.")
        default:
            return open(row.waybackUrl, missing: "No Wayback snapshot yet (run `ressac check`).")
        }
    }

    private static func open(_ address: String?, missing: String) -> GardenCommand {
        guard let address else { return .message(missing) }
        return LinkPolicy.webURL(address).map(GardenCommand.open) ?? .message("Refusing to open a non-web address.")
    }

    // MARK: Labels (core model.rs; `ProposedAction.isSafe`, `isRepair` and `label` come from Phase 7)

    /// The short chip at the end of a row: the decision once made, else the proposed action.
    static func chip(for row: GardenRow) -> String? {
        if let decision = row.lastDecision, !row.needsTriage {
            switch decision {
            case "keep": return "kept"
            case "repair": return "repaired"
            case "bury": return "buried"
            default: return decision
            }
        }
        switch row.proposedAction {
        case .upgradeHttps: return "https"
        case .followRedirect: return "follow"
        case .adoptDiscovered: return "adopt"
        case .mergeInto: return "merge"
        case .bury: return "bury"
        case .unknown: return nil
        case nil: return triageStatuses.contains(status(of: row)) ? "wait" : nil
        }
    }

    /// "12 unread" on a buried feed: the reading lists hide its entries, so the garden is the only
    /// place that tells what burying left unread. Nil for active feeds and when nothing is left.
    static func unreadLabel(for row: GardenRow) -> String? {
        isBuried(row) && row.unread > 0 ? "\(row.unread) unread" : nil
    }

    static func remarkSentence(_ remark: Remark) -> String {
        switch remark {
        case .httpsAvailable: "The https:// address serves the same feed."
        case .httpOnly: "Only served over http://."
        case .similarTitle(let feedId): "A feed in the same folder has a similar title (#\(feedId))."
        case .dateFallback: "Most entries carry no date; Ressac used the fetch date."
        case .blocked: "The last fetch was blocked (403 or an anti-bot page)."
        case .unknown(let type): "Remark “\(type)” (newer than this app)."
        }
    }

    /// `fetch_log.error_kind` holds core `ErrorKind` as JSON ("Dns", {"Http":404}); same labels as `ErrorKind::label`.
    static func errorLabel(_ raw: String) -> String {
        let parsed = try? JSONSerialization.jsonObject(with: Data(raw.utf8), options: .fragmentsAllowed)
        if let object = parsed as? [String: Any], let code = object["Http"] as? Int {
            return "HTTP \(code)"
        }
        switch (parsed as? String) ?? raw {
        case "HtmlInsteadOfFeed": return "HTML page instead of a feed"
        case "Challenge": return "anti-bot challenge page"
        case "TooLarge": return "response too large"
        case let name: return name
        }
    }

    /// One fetch log line: the error label, else the HTTP status.
    static func outcome(of entry: FetchLogRow) -> String {
        if let kind = entry.errorKind { return errorLabel(kind) }
        if let status = entry.httpStatus { return "HTTP \(status)" }
        return "no response"
    }
}
