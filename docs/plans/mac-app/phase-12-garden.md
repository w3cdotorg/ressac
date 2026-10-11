# Phase 12: Garden table, inspector, garden actions

Read the plan index first. Global Constraints apply. Spec §7 (Garden: table, header, inspector, behaviour; Keyboard: `k` is Keep in the Garden; "Nothing to triage"; and, for the reader wiring, Reader), core spec §9 (garden rules), mockup `garden-placement.html` option A.

**Files (5):**
- Create: `apps/macos/Ressac/Garden/GardenModel.swift`
- Create: `apps/macos/Ressac/Garden/GardenTable.swift`
- Create: `apps/macos/Ressac/Garden/GardenInspector.swift`
- Modify: `apps/macos/Ressac/Reading/MainView.swift` (swaps Phase 9's three placeholders: the Garden table, the Garden inspector and, for Phase 10, the reader)
- Create: `apps/macos/RessacTests/GardenTests.swift`

**Consumes from Phases 6–11** (names as their phase files define them):
- Records (Phase 7): `GardenRow` with its memberwise initialiser (`GardenRow(feedId:title:folderPath:state:status:reason:clue:evidence:remarks:proposedAction:needsTriage:lastDecision:waybackUrl:htmlUrl:xmlUrl:)`, `id == feedId`); `GardenEvidence { bullets; clue; language }`; `enum Remark { httpsAvailable, httpOnly, similarTitle(feedId:), dateFallback, blocked, unknown(String) }`; `enum ProposedAction { upgradeHttps(url:), followRedirect(url:), adoptDiscovered(url:), mergeInto(feedId:), bury }` with `isSafe`, `isRepair` and `label` (core `Action::label`), which this phase reuses; `FetchLogRow { id; at; httpStatus; errorKind; redirectChain: [Hop] }` where `errorKind` is the raw `fetch_log.error_kind` JSON (`"Dns"`, `{"Http":404}`); `Hop { status: Int; url }`. They mirror `crates/ressac-core/src/model.rs` (`#[serde(tag = "type", rename_all = "snake_case")]`); Task 12.2 pins the decoding on the Phase 5 fixture.
- Queries (Phase 7): `garden(scope:)` with `.needsTriage` = `needs_triage AND state = 'active'`, `.all` = active feeds, `.cemetery` = buried feeds; `fetchLog(feedId:)`, newest first, 20 rows. `AppDatabase.reader`.
- `AppState` (Phase 9): `selection`, `selectedFeedId` (the Garden's selection, cleared on a source change, set by `showInGarden(feedId:)`), `decidedThisVisit` (feeds decided with success since the scope was entered), `canGardenDecide`, `gardenDecide(_:)`, `gardenUndo()`, `gardenApplySafe()`, `selectedEntry`, `database`. `GardenDecision` lives in `Engine/RessacCLI.swift`. `RunCoordinator` already refreshes the observations after every `ressac` run (`notifyExternalChanges()`), so the table follows decisions with no extra mechanism. In the Garden `canNavigateEntries` is false, so no menu binds `j`/`k` and the table's `onKeyPress` receives `k`.
- `Reading/MainView.swift` (Phase 9): the content column shows `Text("Garden")` and the detail column `Text("No feed selected")` for garden sources, and the detail column of reading sources `ReaderPlaceholder(entry: state.selectedEntry)`; views take the state as `@Bindable var state: AppState`.
- `ReaderView(state:)` (Phase 10), `LinkPolicy.webURL(_:)` (Phase 10).
- `ressac garden decide <id> keep` restores a buried feed (Phase 5's deviation, as in the TUI), so `k` on the Cemetery restores.

**Behaviour, from the TUI** (`crates/ressac/src/tui/state.rs`, `ui.rs`) and the spec:
- Sections by status in urgency order (`Duplicate, Hijacked, Moved, Dead, Ailing, Fossil, Dormant, Alive, Unchecked`), then by top folder and title, case-insensitive (core `load_rows`). The Cemetery is one "Buried" section.
- The queue observes both `.all` and `.cemetery`: "N of M left" then counts as the TUI does (M = feeds whose status needs triage, buried ones counting as done; N = those with `needs_triage`), and a feed buried during the visit stays visible.
- Decided rows are dimmed until the list is left. The table dims `decidedThisVisit` (Phase 9) together with every feed it has *seen* needing triage during this visit that no longer does: the second set adds what `decidedThisVisit` cannot know, the feeds settled by Apply Safe Repairs or by a decision taken in the terminal. When the selected feed leaves the queue, the selection moves to the next feed that still needs triage, like the TUI's cursor; it moves only after the database shows the decision, so it never races with `gardenDecide`, which reads `selectedFeedId` when called.
- Keys on the focused table: `k` keep (restore on a buried feed), `r` repair (refused without a repair action), `b` bury (refused on a buried feed), `u` undo, `o` open site, `w` Wayback (http(s) only), `⇧R` apply safe repairs after a confirmation listing the first eight. Arrows navigate. `⌘Z` is the header's Undo button. Messages are the TUI's.

### Task 12.1: Garden model

**Interfaces:**
- Consumes: `GardenRow`, `ProposedAction` (`isSafe`, `isRepair`, `label`), `Remark`, `FetchLogRow`, `GardenScope`, `GardenDecision`, `LinkPolicy.webURL`.
- Produces (`Garden/GardenModel.swift`):
  - `enum GardenCommand: Equatable { case decide(GardenDecision), undo, open(URL), confirmSafeRepairs, message(String) }`;
  - `enum GardenModel` with `urgencyOrder`, `triageStatuses`, `struct Section { id; glyph; rows; header }`, `struct Progress { left; total }`, and the functions `status(of:)`, `isBuried(_:)`, `glyph(_:)`, `title(for:)`, `emptyMessage(for:)`, `visibleRows(_:scope:seenInQueue:)`, `isDimmed(_:scope:seenInQueue:)`, `sections(_:scope:)`, `progress(_:)`, `safeRepairs(_:)`, `applyButtonTitle(_:)`, `applyQuestion(_:)`, `safeRepairPreview(_:)`, `selectionAfterUpdate(selected:previous:current:ordered:)`, `command(for:row:safeRepairCount:)`, `chip(for:)`, `remarkSentence(_:)`, `errorLabel(_:)`, `outcome(of:)`. Labels match `ErrorKind::label` and the TUI's messages; action labels and safety are `ProposedAction`'s own (Phase 7).

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/GardenTests.swift`:

```swift
import Foundation
import GRDB
import Testing
@testable import Ressac

private func row(_ id: Int64, _ title: String, status: String?, folder: String? = "Blogs", state: String = "active",
                 action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                 site: String? = nil, wayback: String? = nil) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: nil, clue: nil,
              evidence: nil, remarks: [], proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)")
}

@Suite struct GardenModelTests {
    @Test func sectionsFollowUrgencyThenFolderThenTitle() {
        let rows = [
            row(1, "Zed", status: "Dead"),
            row(2, "alpha", status: "Dead"),
            row(3, "Mover", status: "Moved", folder: "Infos"),
            row(4, "Fine", status: "Alive"),
            row(5, "Gone", status: "Dead", state: "buried"),
            row(6, "New", status: nil, folder: nil),
            row(7, "Early", status: "Dead", folder: "Biblis / Sub"),
        ]
        let all = GardenModel.sections(rows, scope: .all)
        #expect(all.map(\.id) == ["Moved", "Dead", "Alive", "Unchecked", "Buried"])
        #expect(all[1].rows.map(\.title) == ["Early", "alpha", "Zed"])
        #expect(all[1].header == "🪦 DEAD · 3")
        #expect(all.last?.rows.map(\.feedId) == [5])
        let queue = GardenModel.sections(rows, scope: .needsTriage)
        #expect(queue.first { $0.id == "Dead" }?.rows.map(\.feedId) == [7, 2, 5, 1])
        #expect(!queue.contains { $0.id == "Buried" })
    }

    @Test func theQueueKeepsFeedsDecidedDuringTheVisitDimmed() {
        let waiting = row(1, "Waiting", status: "Dead", needsTriage: true)
        let decided = row(2, "Decided", status: "Dead", needsTriage: false, decision: "keep")
        let old = row(3, "Decided last week", status: "Fossil", needsTriage: false, decision: "keep")
        let rows = [waiting, decided, old]
        #expect(GardenModel.visibleRows(rows, scope: .needsTriage, seenInQueue: [1, 2]).map(\.feedId) == [1, 2])
        #expect(GardenModel.visibleRows(rows, scope: .needsTriage, seenInQueue: [1]).map(\.feedId) == [1])
        #expect(GardenModel.isDimmed(decided, scope: .needsTriage, seenInQueue: [1, 2]))
        #expect(!GardenModel.isDimmed(waiting, scope: .needsTriage, seenInQueue: [1, 2]))
        #expect(!GardenModel.isDimmed(decided, scope: .all, seenInQueue: [1, 2]))
        #expect(GardenModel.visibleRows(rows + [row(4, "Gone", status: "Dead", state: "buried")], scope: .cemetery, seenInQueue: []).map(\.feedId) == [4])
    }

    @Test func progressCountsLikeTheTerminalGarden() {
        let rows = [
            row(1, "Dead", status: "Dead", needsTriage: true),
            row(2, "Fossil kept", status: "Fossil", needsTriage: false, decision: "keep"),
            row(3, "Buried", status: "Dead", state: "buried", needsTriage: false, decision: "bury"),
            row(4, "Alive", status: "Alive"),
            row(5, "Unchecked", status: nil),
        ]
        #expect(GardenModel.progress(rows) == GardenModel.Progress(left: 1, total: 3))
    }

    @Test func safeRepairsAreUndecidedActiveSafeActions() {
        let https = ProposedAction.upgradeHttps(url: "https://a.example.org/feed")
        let rows = [
            row(1, "A", status: "Alive", action: https),
            row(2, "B", status: "Moved", action: .followRedirect(url: "https://b.example.org/feed"), needsTriage: true),
            row(3, "C", status: "Moved", action: .followRedirect(url: "https://c.example.org/feed"), decision: "keep"),
            row(4, "D", status: "Alive", state: "buried", action: https),
            row(5, "E", status: "Duplicate", action: .mergeInto(feedId: 1), needsTriage: true),
            row(6, "F", status: "Dead", action: .bury, needsTriage: true),
        ]
        #expect(GardenModel.safeRepairs(rows).map(\.feedId) == [1, 2])
        #expect(GardenModel.safeRepairPreview(GardenModel.safeRepairs(rows))
            == "A: switch to https://a.example.org/feed\nB: follow redirect to https://b.example.org/feed")
        let many = (1...10).map { row(Int64($0), "Feed \($0)", status: "Alive", action: https) }
        let preview = GardenModel.safeRepairPreview(many).components(separatedBy: "\n")
        #expect(preview.count == 9)
        #expect(preview.last == "and 2 more.")
        #expect(GardenModel.applyButtonTitle(1) == "Apply 1 Safe Repair")
        #expect(GardenModel.applyButtonTitle(87) == "Apply 87 Safe Repairs")
        #expect(GardenModel.applyQuestion(87) == "Apply 87 safe repairs?")
    }

    @Test func selectionMovesOnWhenTheSelectedFeedLeavesTheQueue() {
        let before = [row(1, "A", status: "Dead", needsTriage: true), row(2, "B", status: "Dead", needsTriage: false),
                      row(3, "C", status: "Dead", needsTriage: true)]
        var after = before
        after[0].needsTriage = false
        #expect(GardenModel.selectionAfterUpdate(selected: 1, previous: before, current: after, ordered: after) == 3)
        var lastDecided = before
        lastDecided[2].needsTriage = false
        #expect(GardenModel.selectionAfterUpdate(selected: 3, previous: before, current: lastDecided, ordered: lastDecided) == 1)
        #expect(GardenModel.selectionAfterUpdate(selected: 3, previous: before, current: before, ordered: before) == 3)
        let done = before.map { var r = $0; r.needsTriage = false; return r }
        #expect(GardenModel.selectionAfterUpdate(selected: 1, previous: before, current: done, ordered: done) == 1)
        #expect(GardenModel.selectionAfterUpdate(selected: nil, previous: before, current: after, ordered: after) == nil)
    }

    @Test func keysMapLikeTheTerminalGarden() {
        let dead = row(1, "Dead", status: "Dead", action: .bury, needsTriage: true,
                       site: "https://dead.example.org/", wayback: "http://web.archive.org/web/2021/https://dead.example.org/")
        let fossil = row(2, "Fossil", status: "Fossil", needsTriage: true, site: "javascript:alert(1)")
        let buried = row(3, "Buried", status: "Dead", state: "buried", decision: "bury")
        let moved = row(4, "Moved", status: "Moved", action: .followRedirect(url: "https://m.example.org/feed"), needsTriage: true)

        #expect(GardenModel.command(for: "k", row: dead, safeRepairCount: 0) == .decide(.keep))
        #expect(GardenModel.command(for: "b", row: dead, safeRepairCount: 0) == .decide(.bury))
        #expect(GardenModel.command(for: "r", row: moved, safeRepairCount: 0) == .decide(.repair))
        #expect(GardenModel.command(for: "r", row: dead, safeRepairCount: 0) == .message("Nothing to repair for this feed."))
        #expect(GardenModel.command(for: "r", row: fossil, safeRepairCount: 0) == .message("Nothing to repair for this feed."))
        #expect(GardenModel.command(for: "k", row: buried, safeRepairCount: 0) == .decide(.keep))
        #expect(GardenModel.command(for: "b", row: buried, safeRepairCount: 0) == .message("This feed is buried; press k to restore it first."))
        #expect(GardenModel.command(for: "r", row: buried, safeRepairCount: 0) == .message("This feed is buried; press k to restore it first."))
        #expect(GardenModel.command(for: "o", row: dead, safeRepairCount: 0) == .open(URL(string: "https://dead.example.org/")!))
        #expect(GardenModel.command(for: "w", row: dead, safeRepairCount: 0) == .open(URL(string: "http://web.archive.org/web/2021/https://dead.example.org/")!))
        #expect(GardenModel.command(for: "o", row: fossil, safeRepairCount: 0) == .message("Refusing to open a non-web address."))
        #expect(GardenModel.command(for: "o", row: moved, safeRepairCount: 0) == .message("This feed has no site address."))
        #expect(GardenModel.command(for: "w", row: moved, safeRepairCount: 0) == .message("No Wayback snapshot yet (run `ressac check`)."))
        #expect(GardenModel.command(for: "u", row: nil, safeRepairCount: 0) == .undo)
        #expect(GardenModel.command(for: "R", row: nil, safeRepairCount: 3) == .confirmSafeRepairs)
        #expect(GardenModel.command(for: "R", row: nil, safeRepairCount: 0) == .message("No safe repairs pending."))
        #expect(GardenModel.command(for: "k", row: nil, safeRepairCount: 0) == .message("Select a feed first."))
        #expect(GardenModel.command(for: "j", row: dead, safeRepairCount: 0) == nil)
    }

    @Test func labelsMatchTheCore() {
        #expect(GardenModel.chip(for: row(1, "A", status: "Duplicate", action: .mergeInto(feedId: 2), needsTriage: true)) == "merge")
        #expect(GardenModel.chip(for: row(1, "A", status: "Ailing", needsTriage: true)) == "wait")
        #expect(GardenModel.chip(for: row(1, "A", status: "Alive")) == nil)
        #expect(GardenModel.chip(for: row(1, "A", status: "Moved", action: .followRedirect(url: "x"), decision: "repair")) == "repaired")

        #expect(GardenModel.remarkSentence(.similarTitle(feedId: 12)) == "A feed in the same folder has a similar title (#12).")
        #expect(GardenModel.remarkSentence(.unknown("paywall")) == "Remark “paywall” (newer than this app).")
        #expect(GardenModel.errorLabel(#""Dns""#) == "Dns")
        #expect(GardenModel.errorLabel(#"{"Http":404}"#) == "HTTP 404")
        #expect(GardenModel.errorLabel(#""HtmlInsteadOfFeed""#) == "HTML page instead of a feed")
        #expect(GardenModel.errorLabel(#""Challenge""#) == "anti-bot challenge page")
        #expect(GardenModel.errorLabel(#""TooLarge""#) == "response too large")
        #expect(GardenModel.errorLabel("Timeout") == "Timeout")
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenModelTests`. Expected: build failure, `cannot find 'GardenModel' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Garden/GardenModel.swift`:

```swift
import Foundation

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
        case .all: rows
        case .cemetery: rows.filter(isBuried)
        }
    }

    static func isDimmed(_ row: GardenRow, scope: GardenScope, seenInQueue: Set<Int64>) -> Bool {
        scope == .needsTriage && !row.needsTriage && seenInQueue.contains(row.feedId)
    }

    /// One section per status in urgency order; buried feeds last (in the queue, a feed buried
    /// during the visit stays dimmed in its status section instead).
    static func sections(_ rows: [GardenRow], scope: GardenScope) -> [Section] {
        let splitBuried = scope != .needsTriage
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

    /// Keys of the terminal garden. `nil` means the key is not the Garden's.
    static func command(for key: String, row: GardenRow?, safeRepairCount: Int) -> GardenCommand? {
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
        case nil: return triageStatuses.contains(status(of: row)) ? "wait" : nil
        }
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
```

- [ ] **Step 4: Run green.** Same command. Expected: `GardenModelTests` passes, 7 tests.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Garden/GardenModel.swift apps/macos/RessacTests/GardenTests.swift
git commit -m "Garden model: sections, progress, safe repairs, keys and labels as in the TUI"
```

### Task 12.2: Pin the core JSON shapes

**Interfaces:**
- Consumes: `AppDatabase.open(at:)`, `Queries.garden(scope:)`, `Queries.fetchLog(feedId:)`, `GardenModel.outcome(of:)`, `TestDatabase.fixture` (Phase 7's path to the fixture; `TestDatabase` itself clears the diagnoses this test needs, so the test copies the file), GRDB `DatabaseQueue` (test setup only), `tests/fixtures/contract.sqlite` at v2 (Phase 5: the 11 feeds of `vienna-like.opml`; Girl on the Couch Moved with `follow_redirect` and `http_only`; Bibliobsession 2.0 Hijacked, clue `vie ⚠`, `bury`; Orange Drenka Fossil; Unphotographable Dead, kept, with a Wayback URL and one fetch at `2026-10-10T07:00:00.000Z`: a 301 hop to `https://www.unphotographable.com/index.rdf`, then HTTP 410; Photos from bouquiniste buried).
- Produces: the test `gardenRowsDecodeTheCoreJSONShapes`. It adds, on a private copy, the shapes the fixture lacks (`upgrade_https`, `merge_into`, `adopt_discovered`, `https_available`, `date_fallback`, `blocked`, `similar_title`, an evidence `language`), looking feeds up by title, never by id.

This is a contract test on Phase 7's decoding and query scopes, written against real rows. It is expected to pass at once (Phase 7's `Remark` and `ProposedAction` decode the `type` tag and `feed_id` by hand); it is red only if a JSON column and `model.rs` disagree. In that case stop and report: the fix belongs in Phase 7's file, outside this phase's budget.

- [ ] **Step 1: Write the test.** Append to `GardenTests.swift`:

```swift
@Suite struct GardenDecodingTests {
    /// A private copy of the **full** contract fixture (Phase 7's `TestDatabase` clears the
    /// diagnoses and decisions this test needs), so the test may add rows.
    private func fixtureCopy() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appending(path: "ressac-garden-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let copy = dir.appending(path: "ressac.sqlite")
        try FileManager.default.copyItem(at: TestDatabase.fixture, to: copy)
        return copy
    }

    private func firstValue<Reducer: ValueReducer>(_ observation: ValueObservation<Reducer>,
                                                   in reader: any DatabaseReader) async throws -> Reducer.Value
        where Reducer.Value: Sendable {
        for try await value in observation.values(in: reader) {
            return value
        }
        throw CancellationError()
    }

    /// The JSON written by core `model.rs` (evidence, remarks, proposed_action, fetch_log) must
    /// decode into the Swift records. The fixture covers follow_redirect, bury, http_only and a
    /// fetch log with a hop; the other shapes are added here on feeds the fixture leaves undiagnosed.
    @Test func gardenRowsDecodeTheCoreJSONShapes() async throws {
        let url = try fixtureCopy()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        do {
            let queue = try DatabaseQueue(path: url.path(percentEncoded: false))
            try await queue.write { db in
                try db.execute(sql: """
                    INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
                    SELECT id, 'Alive', 'Fine.', '{"bullets":["Feed language: eng."],"clue":"","language":"eng"}',
                           '[{"type":"https_available"},{"type":"date_fallback"},{"type":"blocked"}]',
                           '{"type":"upgrade_https","url":"https://sixcolors.example/feed.json"}', '2026-10-10T08:00:00.000Z', 0
                    FROM feed WHERE title = 'Six Colors';
                    INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
                    SELECT f.id, 'Duplicate', 'Same entries as another feed.', '{"bullets":[],"clue":"= #' || k.id || '","language":null}', '[]',
                           '{"type":"merge_into","feed_id":' || k.id || '}', '2026-10-10T08:00:00.000Z', 1
                    FROM feed f, feed k WHERE f.title = 'Center for Humane Technology (1)' AND k.title = 'Center for Humane Technology';
                    INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
                    SELECT f.id, 'Fossil', 'Nothing new since 2013.', '{"bullets":[],"clue":"2013","language":null}',
                           '[{"type":"similar_title","feed_id":' || k.id || '}]',
                           '{"type":"adopt_discovered","url":"https://orange.example.org/feed"}', '2026-10-10T08:00:00.000Z', 1
                    FROM feed f, feed k WHERE f.title = 'Orange-Drenka' AND k.title = 'Orange Drenka';
                    """)
            }
        }
        let database = try AppDatabase.open(at: url)
        let rows = try await firstValue(Queries.garden(scope: .all), in: database.reader)
        let byTitle = Dictionary(uniqueKeysWithValues: rows.map { ($0.title, $0) })
        let id = { (title: String) in byTitle[title]?.feedId ?? -1 }

        let girl = try #require(byTitle["Girl on the Couch"])
        #expect(girl.status == "Moved")
        #expect(girl.clue == "301")
        #expect(girl.evidence?.bullets == ["Clue: 301."])
        #expect(girl.remarks == [.httpOnly])
        #expect(girl.proposedAction == .followRedirect(url: "https://girl.example.org/feed"))
        #expect(girl.needsTriage)

        let biblio = try #require(byTitle["Bibliobsession 2.0"])
        #expect(biblio.status == "Hijacked")
        #expect(biblio.clue == "vie ⚠")
        #expect(biblio.proposedAction == .bury)

        let unphoto = try #require(byTitle["Unphotographable"])
        #expect(unphoto.lastDecision == "keep")
        #expect(!unphoto.needsTriage)
        #expect(unphoto.waybackUrl == "http://web.archive.org/web/2019/http://www.unphotographable.com/")

        #expect(byTitle["Photos from bouquiniste"] == nil, "All feeds lists active feeds only")

        let undiagnosed = try #require(byTitle["Center for Humane Technology"])
        #expect(undiagnosed.status == nil)
        #expect(undiagnosed.evidence == nil)
        #expect(undiagnosed.remarks.isEmpty)
        #expect(undiagnosed.proposedAction == nil)

        let six = try #require(byTitle["Six Colors"])
        #expect(six.evidence?.language == "eng")
        #expect(six.remarks == [.httpsAvailable, .dateFallback, .blocked])
        #expect(six.proposedAction == .upgradeHttps(url: "https://sixcolors.example/feed.json"))
        #expect(byTitle["Center for Humane Technology (1)"]?.proposedAction == .mergeInto(feedId: id("Center for Humane Technology")))
        #expect(byTitle["Orange-Drenka"]?.remarks == [.similarTitle(feedId: id("Orange Drenka"))])
        #expect(byTitle["Orange-Drenka"]?.proposedAction == .adoptDiscovered(url: "https://orange.example.org/feed"))

        let queue = try await firstValue(Queries.garden(scope: .needsTriage), in: database.reader)
        #expect(Set(queue.map(\.title)) == ["Girl on the Couch", "Bibliobsession 2.0", "Orange Drenka",
                                             "Center for Humane Technology (1)", "Orange-Drenka"])
        let cemetery = try await firstValue(Queries.garden(scope: .cemetery), in: database.reader)
        #expect(cemetery.map(\.title) == ["Photos from bouquiniste"])
        #expect(cemetery.first?.state == "buried")
        #expect(cemetery.first?.lastDecision == "bury")

        let log = try await firstValue(Queries.fetchLog(feedId: unphoto.feedId), in: database.reader)
        let latest = try #require(log.first)
        #expect(latest.at == Date(timeIntervalSince1970: 1_791_615_600))
        #expect(latest.httpStatus == 410)
        #expect(GardenModel.outcome(of: latest) == "HTTP 410")
        #expect(latest.redirectChain.map(\.status) == [301])
        #expect(latest.redirectChain.map(\.url) == ["https://www.unphotographable.com/index.rdf"])
    }
}
```

- [ ] **Step 2: Run.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenDecodingTests`. Expected: 1 test passes.

- [ ] **Step 3: Commit.**

```bash
git add apps/macos/RessacTests/GardenTests.swift
git commit -m "Garden: pin the decoding of evidence, remarks, actions and fetch log"
```

### Task 12.3: The Garden table

**Interfaces:**
- Consumes: `GardenModel`, `AppState` (`selectedFeedId`, `decidedThisVisit`, `gardenDecide`, `gardenUndo`, `gardenApplySafe`, `database`), `Queries.garden(scope:)`.
- Produces (`Garden/GardenTable.swift`): `struct GardenTable: View { @Bindable var state: AppState; let scope: GardenScope }`. Header: scope title, "N of M left" (queue) or "N feeds", the last refusal message, **Apply N Safe Repairs** (prominent, hidden on the Cemetery), **Undo** (`⌘Z`). List: one `Section` per status with "🪦 DEAD · 8" headers; rows with glyph, title, clue (tabular digits) and a chip (proposed action, or the decision once made: kept, repaired, buried); dimmed rows at 45 % opacity. Empty: "Nothing to triage", "No feeds" or "The cemetery is empty".

- [ ] **Step 1: Implement.** Create `apps/macos/Ressac/Garden/GardenTable.swift`:

```swift
import GRDB
import SwiftUI

/// The Garden's list column (spec §7, mockup option A): one section per status, a header
/// with progress, safe repairs and undo, and the terminal garden's keys.
struct GardenTable: View {
    @Bindable var state: AppState
    let scope: GardenScope

    /// `Queries.garden(scope: .all)` (active feeds) and `.cemetery` (buried ones).
    @State private var active: [GardenRow] = []
    @State private var buried: [GardenRow] = []
    /// Feeds seen needing triage during this visit: once decided (here, by ⇧R or in the
    /// terminal) they stay, dimmed, until the list is left.
    @State private var seenInQueue: Set<Int64> = []
    @State private var message: String?
    @State private var confirmingSafeRepairs = false
    @FocusState private var listFocused: Bool

    private var rows: [GardenRow] { active + buried }
    private var retained: Set<Int64> { seenInQueue.union(state.decidedThisVisit) }

    var body: some View {
        let visible = GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained)
        let sections = GardenModel.sections(visible, scope: scope)
        let safeRepairs = GardenModel.safeRepairs(rows)

        VStack(spacing: 0) {
            header(visibleCount: visible.count, safeRepairCount: safeRepairs.count)
            Divider()
            if sections.isEmpty {
                ContentUnavailableView(GardenModel.emptyMessage(for: scope), systemImage: "leaf")
            } else {
                List(selection: $state.selectedFeedId) {
                    ForEach(sections) { section in
                        Section(section.header) {
                            ForEach(section.rows) { row in
                                GardenRowView(row: row, dimmed: GardenModel.isDimmed(row, scope: scope, seenInQueue: retained))
                                    .tag(row.feedId)
                            }
                        }
                    }
                }
                .focused($listFocused)
                .onKeyPress(phases: .down) { press in
                    handle(press.characters, safeRepairCount: safeRepairs.count)
                }
            }
        }
        .confirmationDialog(GardenModel.applyQuestion(safeRepairs.count), isPresented: $confirmingSafeRepairs, titleVisibility: .visible) {
            Button(safeRepairs.count == 1 ? "Apply" : "Apply All") { state.gardenApplySafe() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(GardenModel.safeRepairPreview(safeRepairs))
        }
        .task(id: scope) {
            seenInQueue = []
            message = nil
            active = []
            listFocused = true
            guard scope != .cemetery else { return }
            await observe(.all) { active = $0 }
        }
        .task(id: scope) {
            buried = []
            guard scope != .all else { return }
            await observe(.cemetery) { buried = $0 }
        }
    }

    private func header(visibleCount: Int, safeRepairCount: Int) -> some View {
        HStack(spacing: 8) {
            Text(GardenModel.title(for: scope))
                .font(.headline)
            Text(countLabel(visibleCount: visibleCount))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(.quaternary, in: Capsule())
            Spacer(minLength: 8)
            if let message {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .transition(.opacity)
            }
            if scope != .cemetery {
                Button(GardenModel.applyButtonTitle(safeRepairCount)) { confirmingSafeRepairs = true }
                    .buttonStyle(.borderedProminent)
                    .disabled(safeRepairCount == 0)
                    .help("Apply every safe repair: https upgrades and permanent redirects (⇧R)")
            }
            Button("Undo") { state.gardenUndo() }
                .keyboardShortcut("z", modifiers: .command)
                .help("Undo the last decision, here or in the terminal garden (U or ⌘Z)")
        }
        .controlSize(.small)
        .padding(.horizontal, 12)
        .frame(height: 44)
        .animation(.easeOut(duration: 0.15), value: message)
    }

    private func countLabel(visibleCount: Int) -> String {
        switch scope {
        case .needsTriage:
            let progress = GardenModel.progress(rows)
            return "\(progress.left) of \(progress.total) left"
        case .all, .cemetery:
            return visibleCount == 1 ? "1 feed" : "\(visibleCount) feeds"
        }
    }

    private func handle(_ key: String, safeRepairCount: Int) -> KeyPress.Result {
        let row = rows.first { $0.feedId == state.selectedFeedId }
        guard let command = GardenModel.command(for: key, row: row, safeRepairCount: safeRepairCount) else {
            return .ignored
        }
        message = nil
        switch command {
        case .decide(let decision): state.gardenDecide(decision)
        case .undo: state.gardenUndo()
        case .open(let url): NSWorkspace.shared.open(url)
        case .confirmSafeRepairs: confirmingSafeRepairs = true
        case .message(let text): message = text
        }
        return .handled
    }

    private func observe(_ query: GardenScope, into store: ([GardenRow]) -> Void) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: state.database.reader) {
                let previous = rows
                store(fresh)
                rowsDidChange(previous: previous)
            }
        } catch {
            message = "The garden could not be read."
        }
    }

    /// Keeps the queue's memory and moves the selection on when the selected feed was decided.
    private func rowsDidChange(previous: [GardenRow]) {
        seenInQueue.formUnion(rows.filter(\.needsTriage).map(\.feedId))
        let ordered = GardenModel.sections(GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained), scope: scope)
            .flatMap(\.rows)
        if scope == .needsTriage {
            state.selectedFeedId = GardenModel.selectionAfterUpdate(
                selected: state.selectedFeedId, previous: previous, current: rows, ordered: ordered)
        }
        if !ordered.contains(where: { $0.feedId == state.selectedFeedId }) {
            state.selectedFeedId = ordered.first(where: \.needsTriage)?.feedId ?? ordered.first?.feedId
        }
    }
}

private struct GardenRowView: View {
    let row: GardenRow
    let dimmed: Bool

    var body: some View {
        HStack(spacing: 8) {
            Text(GardenModel.glyph(GardenModel.status(of: row)))
                .frame(width: 22)
            Text(row.title)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            if let clue = row.clue, !clue.isEmpty {
                Text(clue)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let chip = GardenModel.chip(for: row) {
                Text(chip)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(.quaternary, in: Capsule())
            }
        }
        .opacity(dimmed ? 0.45 : 1)
        .animation(.easeOut(duration: 0.2), value: dimmed)
        .help([row.folderPath, row.reason].compactMap { $0 }.joined(separator: "\n"))
    }
}
```

- [ ] **Step 2: Build.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' build`. Expected: `BUILD SUCCEEDED`, no warning in our sources.

- [ ] **Step 3: Commit.**

```bash
git add apps/macos/Ressac/Garden/GardenTable.swift
git commit -m "Garden table: sections by status, progress, safe repairs, undo and TUI keys"
```

### Task 12.4: The Garden inspector

**Interfaces:**
- Consumes: `GardenModel`, `AppState` (`selectedFeedId`, `gardenDecide`, `database`), `Queries.garden(scope:)`, `Queries.fetchLog(feedId:)`, `ProposedAction.label`/`isRepair`, `LinkPolicy.webURL`.
- Produces (`Garden/GardenInspector.swift`): `struct GardenInspector: View { @Bindable var state: AppState; let scope: GardenScope }`: title and folder; status with its glyph and colour, "Buried. Press k to restore it." on the Cemetery; reason; Evidence and Remarks bullets; Proposed action, Decision, Feed, Site and Wayback rows; buttons Keep (Restore when buried) `k`, Repair `r` (only with a repair action, prominent), Bury `b` (prominent when burying is proposed), Open Site `o`, Wayback `w`; a "Redirects and fetch log" disclosure with the latest redirect chain and the recent fetch log. The key hints are lowercase, as in the terminal, because `⇧R` is a different command.

- [ ] **Step 1: Implement.** Create `apps/macos/Ressac/Garden/GardenInspector.swift`:

```swift
import GRDB
import SwiftUI

/// The Garden's detail column: why a feed is here, and what to do with it (spec §7).
struct GardenInspector: View {
    @Bindable var state: AppState
    let scope: GardenScope

    @State private var active: [GardenRow] = []
    @State private var buried: [GardenRow] = []
    @State private var log: [FetchLogRow] = []
    @State private var showsHistory = false

    var body: some View {
        Group {
            if let row = (active + buried).first(where: { $0.feedId == state.selectedFeedId }) {
                ScrollView {
                    details(row)
                        .padding(20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .task(id: row.feedId) { await observeLog(of: row.feedId) }
            } else {
                ContentUnavailableView("No Feed Selected", systemImage: "leaf",
                                       description: Text("Select a feed to see why it is in the garden."))
            }
        }
        // Active feeds and the Cemetery: a feed buried during the visit stays inspectable.
        .task(id: scope) { await observe(.all) { active = $0 } }
        .task(id: scope) { await observe(.cemetery) { buried = $0 } }
    }

    @ViewBuilder
    private func details(_ row: GardenRow) -> some View {
        let status = GardenModel.status(of: row)
        let buried = GardenModel.isBuried(row)
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.title3.weight(.semibold))
                    .textSelection(.enabled)
                if let path = row.folderPath, !path.isEmpty {
                    Text(path).font(.callout).foregroundStyle(.secondary)
                }
            }

            VStack(alignment: .leading, spacing: 4) {
                Text("\(GardenModel.glyph(status)) \(status)")
                    .font(.headline)
                    .foregroundStyle(statusColor(status))
                if buried {
                    Text("Buried. Press k to restore it.").font(.callout.weight(.medium))
                }
                if let reason = row.reason {
                    Text(reason).fixedSize(horizontal: false, vertical: true)
                }
            }

            if let bullets = row.evidence?.bullets, !bullets.isEmpty {
                section("Evidence", lines: bullets)
            }
            if !row.remarks.isEmpty {
                section("Remarks", lines: row.remarks.map(GardenModel.remarkSentence))
            }

            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 10, verticalSpacing: 4) {
                if let action = row.proposedAction, !buried {
                    labeled("Proposed", Text(action.label))
                }
                if let decision = row.lastDecision {
                    labeled("Decision", Text(decision.capitalized))
                }
                labeled("Feed", Text(row.xmlUrl).textSelection(.enabled))
                if let site = LinkPolicy.webURL(row.htmlUrl) {
                    labeled("Site", Link(site.absoluteString, destination: site))
                }
                if let wayback = LinkPolicy.webURL(row.waybackUrl) {
                    labeled("Wayback", Link("Archived copy", destination: wayback))
                }
            }
            .font(.callout)

            actions(row, buried: buried)

            DisclosureGroup("Redirects and fetch log", isExpanded: $showsHistory) {
                history
            }
            .font(.callout)
        }
    }

    private func actions(_ row: GardenRow, buried: Bool) -> some View {
        let action = row.proposedAction
        return HStack(spacing: 6) {
            KeyHintButton(buried ? "Restore" : "Keep", key: "k", prominent: false) { state.gardenDecide(.keep) }
            if let action, action.isRepair, !buried {
                KeyHintButton("Repair", key: "r", prominent: true) { state.gardenDecide(.repair) }
            }
            if !buried {
                KeyHintButton("Bury", key: "b", prominent: action == .bury) { state.gardenDecide(.bury) }
            }
            KeyHintButton("Open Site", key: "o", prominent: false) {
                LinkPolicy.webURL(row.htmlUrl).map { _ = NSWorkspace.shared.open($0) }
            }
            .disabled(LinkPolicy.webURL(row.htmlUrl) == nil)
            KeyHintButton("Wayback", key: "w", prominent: false) {
                LinkPolicy.webURL(row.waybackUrl).map { _ = NSWorkspace.shared.open($0) }
            }
            .disabled(LinkPolicy.webURL(row.waybackUrl) == nil)
        }
        .controlSize(.small)
    }

    @ViewBuilder
    private var history: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let chain = log.first?.redirectChain, !chain.isEmpty {
                Text("Redirects").font(.callout.weight(.semibold))
                ForEach(Array(chain.enumerated()), id: \.offset) { _, hop in
                    Text("\(hop.status) → \(hop.url)")
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            if log.isEmpty {
                Text("Not fetched yet.").foregroundStyle(.secondary)
            } else {
                Text("Fetch log").font(.callout.weight(.semibold))
                ForEach(log) { entry in
                    HStack(alignment: .firstTextBaseline) {
                        Text(entry.at.formatted(date: .abbreviated, time: .shortened))
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                        Text(GardenModel.outcome(of: entry))
                    }
                    .font(.caption)
                }
            }
        }
        .padding(.top, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func section(_ title: String, lines: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.callout.weight(.semibold))
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("•").foregroundStyle(.secondary)
                    Text(line).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                }
            }
        }
    }

    private func labeled(_ label: String, _ value: some View) -> some View {
        GridRow {
            Text(label).foregroundStyle(.secondary).gridColumnAlignment(.trailing)
            value
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "Dead", "Hijacked": .red
        case "Ailing": .orange
        case "Moved": .blue
        case "Duplicate": .purple
        case "Fossil": .brown
        case "Dormant": .indigo
        case "Alive": .green
        default: .secondary
        }
    }

    private func observe(_ query: GardenScope, into store: ([GardenRow]) -> Void) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: state.database.reader) {
                store(fresh)
            }
        } catch {
            store([])
        }
    }

    private func observeLog(of feedId: Int64) async {
        log = []
        do {
            for try await fresh in Queries.fetchLog(feedId: feedId).values(in: state.database.reader) {
                log = fresh
            }
        } catch {
            log = []
        }
    }
}

/// A button with its single-key shortcut shown in a lighter weight, as in the mockup ("Keep k").
private struct KeyHintButton: View {
    let title: String
    let key: String
    let prominent: Bool
    let action: () -> Void

    init(_ title: String, key: String, prominent: Bool, action: @escaping () -> Void) {
        self.title = title
        self.key = key
        self.prominent = prominent
        self.action = action
    }

    var body: some View {
        if prominent {
            Button(action: action) { label }.buttonStyle(.borderedProminent)
        } else {
            Button(action: action) { label }.buttonStyle(.bordered)
        }
    }

    private var label: some View {
        HStack(spacing: 4) {
            Text(title)
            Text(key).font(.caption2).opacity(0.6)
        }
    }
}
```

- [ ] **Step 2: Build.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' build`. Expected: `BUILD SUCCEEDED`, no warning in our sources.

- [ ] **Step 3: Commit.**

```bash
git add apps/macos/Ressac/Garden/GardenInspector.swift
git commit -m "Garden inspector: evidence, remarks, links, actions, redirects and fetch log"
```

### Task 12.5: Plug the Garden and the reader into the window

**Interfaces:**
- Consumes: `GardenTable(state:scope:)`, `GardenInspector(state:scope:)`, `ReaderView(state:)` (Phase 10), Phase 9's `MainView`.
- Produces: `MainView` showing the Garden table and inspector for `.garden(scope)` and the reader for reading sources; `ReaderPlaceholder` deleted.

- [ ] **Step 1: Edit `apps/macos/Ressac/Reading/MainView.swift`.** In the `content:` column, replace

```swift
                if state.selection.isGarden {
                    // Phase 12 replaces this with the Garden table.
                    Text("Garden").foregroundStyle(.secondary)
                } else {
```

with

```swift
                if case let .garden(scope) = state.selection {
                    GardenTable(state: state, scope: scope)
                } else {
```

In the `detail:` column, replace

```swift
            if state.selection.isGarden {
                // Phase 12 replaces this with the Garden inspector.
                Text("No feed selected").foregroundStyle(.secondary)
            } else {
                // Phase 10 replaces this with the WKWebView reader. Pass the complete
                // row (`selectedEntry`), never a list row: list rows carry no content (phase 7).
                ReaderPlaceholder(entry: state.selectedEntry)
            }
```

with

```swift
            if case let .garden(scope) = state.selection {
                GardenInspector(state: state, scope: scope)
            } else {
                ReaderView(state: state)
            }
```

Then delete the whole `ReaderPlaceholder` struct and its doc comment ("Stand-in for the reader column until phase 10.") at the end of the file. Match the placeholder lines on their code, not on the comments, if Phase 9's executor reworded them.

- [ ] **Step 2: Build and test.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test`. Expected: all tests pass, zero warnings in our sources; `grep -n ReaderPlaceholder apps/macos/Ressac -r` prints nothing.

- [ ] **Step 3: Manual check of the reader and the Garden on a copy of the fixture** (Phase 9's pattern: `RESSAC_DB` on a copy, `HOME` in the temporary directory; the garden subcommands write to the copy only):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
cp tests/fixtures/contract.sqlite "$SMOKE/ressac.sqlite"
RESSAC_DB="$SMOKE/ressac.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/app.log" 2>&1 &
```

Reader (Phase 10), in light then dark mode (System Settings ▸ Appearance):
1. "Café comic" (xkcd.com) renders its `content_html` ("Full content about a café by the sea."), not the short summary; the title links to the comic and a click opens the default browser while the reader stays put.
2. "Linked: Tides" (Daring Fireball) with Full Article on shows "The whole article about tides." and the "full article" badge; "Walled" with Full Article on shows "Full article unavailable: The page is behind a login." above the feed text.
3. The untitled xkcd entry shows "Untitled" and its summary; Full Article is disabled for it.
4. `m` on the open entry does not move the scroll position; Space scrolls a long article, then moves to the next unread entry at the bottom.
5. In Settings, each of the five fonts re-renders the open article at once (Newsreader and Atkinson Hyperlegible Next included), as do `⌘+`, `⌘−`, `⌘0` and the three line widths; the light and dark palettes differ (dark text is softer, images slightly dimmed), not a plain inversion.

Garden, against the mockup (option A):
6. Garden ▸ Needs triage shows "3 of 4 left", the sections "🦠 HIJACKED · 1", "🚚 MOVED · 1", "🦴 FOSSIL · 1", and "Apply 1 Safe Repair". The inspector of Bibliobsession 2.0 shows "🦠 Hijacked" in red, the reason, the evidence bullet "Clue: vie ⚠." and a prominent Bury.
7. With the table focused, `k` on Bibliobsession 2.0: the row dims with the chip "kept", the header says "2 of 4 left", the selection moves to Girl on the Couch. `u` brings it back undimmed; after another decision, `⌘Z` does the same.
8. `r` on Orange Drenka shows "Nothing to repair for this feed." in the header; `w` on it shows "No Wayback snapshot yet (run `ressac check`)."
9. `⇧R` asks "Apply 1 safe repair?" with "Girl on the Couch: follow redirect to https://girl.example.org/feed"; Apply dims the row with "repaired".
10. Garden ▸ All feeds shows every status section of the active feeds. Unphotographable's inspector: "Redirects and fetch log" lists "301 → https://www.unphotographable.com/index.rdf" and "HTTP 410"; its Wayback button opens the archive in the browser.
11. Garden ▸ Cemetery: Photos from bouquiniste, "Buried. Press k to restore it.", Restore `k`; `b` and `r` say "This feed is buried; press k to restore it first."; `k` restores it and it leaves the Cemetery. "Nothing to triage" shows once the queue is empty.
12. In Terminal, `HOME="$SMOKE/home" "$APP/Contents/Helpers/ressac" --db "$SMOKE/ressac.sqlite" garden` shows the same decisions (quit with `q`); `sqlite3 "$SMOKE/ressac.sqlite" "SELECT feed_id, decision FROM garden_decision ORDER BY id"` lists them in order.

Quit the app, then `rm -rf "$SMOKE"`.

- [ ] **Step 4: Verification gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

- [ ] **Step 5: Commit and push.**

```bash
git add apps/macos/Ressac/Reading/MainView.swift
git commit -m "Window: Garden table and inspector, WKWebView reader in place of the placeholders"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

## Notes for the executor

- **Verified ahead of time.** The three Swift files were type-checked with Swift 6.4 in Swift 6 mode with warnings as errors, against stand-ins for the Phase 7–9 types with the shapes of their phase files; `GardenModelTests` ran green there. `GardenDecodingTests` compiles there but needs the Phase 5 fixture and Phase 7's real decoding to run.
- **Two `.task(id: scope)` modifiers, one per query,** run the `.all` and `.cemetery` observations side by side on the main actor. Starting both from one task with `async let` or a task group was rejected by Swift 6.4's region-based isolation checker ("pattern that the region-based isolation checker does not understand").
- **`k` in the Garden is Keep.** The table's `onKeyPress` handles it while the list has focus and returns `.handled`, so Phase 9's window-level `j`/`k` never sees it. Arrow keys stay with the `List`.
- **`⌘Z`** is attached to the header's Undo button. AppKit offers a key equivalent to the key window's views before the main menu, and the Edit ▸ Undo item is disabled when nothing in the responder chain can undo. If the system Undo item still wins on this Xcode, move the binding to a `CommandGroup(replacing: .undoRedo)` enabled only while `selection` is a Garden scope, and report it.
- **Selection after a decision** moves only when the observation shows the selected feed leaving the queue, so `gardenDecide` reads the right `selectedFeedId` whatever its internal timing.
- **The table and the inspector each observe the garden rows.** Two observations of one view on a `DatabasePool` are cheap, and it keeps the two columns independent.
- **Fixture counts used in the manual check** come from Phase 5's generator: triage statuses on Girl on the Couch (Moved), Bibliobsession 2.0 (Hijacked), Orange Drenka (Fossil) and Unphotographable (Dead, kept), so "3 of 4 left"; Photos from bouquiniste has no diagnosis (Unchecked) and is buried.
