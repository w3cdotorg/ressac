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
