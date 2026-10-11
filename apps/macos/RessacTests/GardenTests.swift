import Foundation
import GRDB
import SwiftUI
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
        #expect(all.map(\.id) == ["Moved", "Dead", "Alive", "Unchecked"])
        #expect(all[1].rows.map(\.title) == ["Early", "alpha", "Gone", "Zed"])
        #expect(all[1].header == "🪦 DEAD · 4")
        let cemetery = GardenModel.sections(rows.filter(GardenModel.isBuried), scope: .cemetery)
        #expect(cemetery.map(\.id) == ["Buried"])
        #expect(cemetery.last?.rows.map(\.feedId) == [5])
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

    @Test func allFeedsKeepsAFeedBuriedDuringTheVisitDimmed() {
        let active = row(1, "Active", status: "Alive")
        let buriedNow = row(2, "Buried now", status: "Dead", state: "buried", needsTriage: false, decision: "bury")
        let buriedBefore = row(3, "Buried before", status: "Dead", state: "buried", needsTriage: false, decision: "bury")
        let rows = [active, buriedNow, buriedBefore]
        #expect(GardenModel.visibleRows(rows, scope: .all, seenInQueue: [2]).map(\.feedId) == [1, 2])
        #expect(GardenModel.visibleRows(rows, scope: .all, seenInQueue: []).map(\.feedId) == [1])
        #expect(GardenModel.isDimmed(buriedNow, scope: .all, seenInQueue: [2]))
        #expect(!GardenModel.isDimmed(active, scope: .all, seenInQueue: [2]))
    }

    @Test func gardenKeysIgnoreCommandControlAndOptionChords() {
        let dead = row(1, "Dead", status: "Dead")
        for chord: EventModifiers in [.command, .control, .option, [.command, .shift]] {
            for key in ["b", "k", "u", "R", "o"] {
                #expect(GardenModel.command(for: key, modifiers: chord, row: dead, safeRepairCount: 1) == nil)
            }
        }
        #expect(GardenModel.command(for: "R", modifiers: .shift, row: dead, safeRepairCount: 1) == .confirmSafeRepairs)
        #expect(GardenModel.command(for: "b", modifiers: [], row: dead, safeRepairCount: 0) == .decide(.bury))
    }

    @Test func anUnknownProposedActionDecodesAndOffersNoRepair() throws {
        let action = try JSONDecoder().decode(ProposedAction.self, from: Data(#"{"type":"teleport","to":"mars"}"#.utf8))
        #expect(action == .unknown("teleport"))
        #expect(!action.isSafe)
        #expect(!action.isRepair)
        #expect(action.label.contains("teleport"))
        let odd = row(1, "Odd", status: "Dead", action: action)
        #expect(GardenModel.safeRepairs([odd]).isEmpty)
        #expect(GardenModel.command(for: "r", row: odd, safeRepairCount: 0) == .message("Nothing to repair for this feed."))
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
