import Foundation
import GRDB
import SwiftUI
import Testing

@testable import Ressac

private func gardenRow(_ id: Int64, _ title: String, status: String?, folder: String? = "Blogs", state: String = "active",
                       reason: String? = nil, evidence: [String]? = nil, remarks: [Remark] = [],
                       action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                       site: String? = nil, wayback: String? = nil, unread: Int = 0) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: reason, clue: nil,
              evidence: evidence.map { GardenEvidence(bullets: $0, clue: "", language: nil) }, remarks: remarks,
              proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)", unread: unread)
}

private func spaces(_ count: Int) -> String { String(repeating: " ", count: count) }

private func trailingTrimmed(_ text: String) -> String {
    String(text.reversed().drop { $0 == " " }.reversed())
}

/// The rows are phase 8's `CellLine`s: these suites read their text with Bedstead registered in the
/// test host, as phases 8 and 9 do (`?` stands for a glyph Bedstead lacks).
@MainActor @Suite struct GardenPageTests {
    let cells = EntryPageColumn.cells

    /// Review Focus #1 for the garden: whatever the title, every row is exactly the entry column's
    /// 28 cells, so the column separators stay straight.
    @Test func everyLineIsExactlyTheColumnWidth() {
        let rows = [
            gardenRow(1, "Café 🎉 東京", status: "Dead", action: .bury, needsTriage: true),
            gardenRow(2, "A title far longer than the twenty-eight cells of the column", status: "Moved",
                      action: .followRedirect(url: "https://m.example.org/feed"), needsTriage: true),
            gardenRow(3, "e\u{301}cole", status: "Fossil", needsTriage: false, decision: "keep"),
            gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury"),
            gardenRow(5, "", status: nil),
        ]
        for scope in [GardenScope.needsTriage, .all, .cemetery] {
            for selected in [nil, Int64(1), 2] {
                let lines = GardenPage.entryLines(rows, scope: scope, retained: [1, 2, 3, 4], selected: selected, cells: cells)
                #expect(!lines.isEmpty)
                for line in lines {
                    #expect(line.cells.width == 28, "\(scope) \(line.id): «\(line.text)»")
                    #expect(line.text.count == 28, "\(scope) \(line.id): «\(line.text)»")
                    #expect(!line.isDoubleHeight)
                }
            }
        }
    }

    @Test func feedsAreGroupedByStatusInUrgencyOrder() {
        let statuses: [String?] = ["Alive", "Dead", "Fossil", "Duplicate", "Hijacked", "Moved", "Ailing", "Dormant", nil]
        let rows = statuses.enumerated().map { gardenRow(Int64($0.offset + 1), "Feed \($0.offset + 1)", status: $0.element) }
        let roles = GardenPage.entryLines(rows, scope: .all, retained: [], selected: nil, cells: cells).map(\.role)
        #expect(roles == [
            .title,
            .heading("Duplicate"), .feed(4), .heading("Hijacked"), .feed(5), .heading("Moved"), .feed(6),
            .heading("Dead"), .feed(2), .heading("Ailing"), .feed(7), .heading("Fossil"), .feed(3),
            .heading("Dormant"), .feed(8), .heading("Alive"), .feed(1), .heading("Unchecked"), .feed(9),
        ])
        #expect(GardenPage.orderedRows(rows, scope: .all, retained: []).map(\.feedId) == [4, 5, 6, 2, 7, 3, 8, 1, 9])
    }

    @Test func headingsTakeTheirTeletextColourAndFossilIsReverseVideo() {
        let dead = GardenModel.Section(id: "Dead", glyph: "🪦",
                                       rows: [gardenRow(1, "A", status: "Dead"), gardenRow(2, "B", status: "Dead")])
        let line = GardenPage.headingLine(dead, cells: cells)
        #expect(line.text == " DEAD" + spaces(22) + "2")
        #expect(line.cells.spans.allSatisfy { $0.foreground == .red && $0.background == .black })
        #expect(line.cells.accessibilityText == "Dead, 2 feeds")

        let fossil = GardenPage.headingLine(GardenModel.Section(id: "Fossil", glyph: "🦴", rows: [gardenRow(3, "C", status: "Fossil")]),
                                            cells: cells)
        #expect(fossil.text == " FOSSIL" + spaces(20) + "1")
        #expect(fossil.cells.spans.allSatisfy { $0.foreground == .white && $0.background == .blue })
        #expect(fossil.cells.accessibilityText == "Fossil, 1 feed")

        for status in GardenModel.urgencyOrder + ["Buried"] {
            let style = GardenPage.headingStyle(for: status)
            let colour = GardenStatusStyle.teletextColor(for: status)
            #expect(style.foreground != .blue, "\(status): blue never colours text")
            if colour == .blue {
                #expect(style.foreground == .white && style.background == .blue, "\(status)")
            } else {
                #expect(style.foreground == colour && style.background == .black, "\(status)")
            }
        }
    }

    @Test func titleRowCountsWhatIsLeftOrListed() {
        let rows = [gardenRow(1, "A", status: "Dead", needsTriage: true),
                    gardenRow(2, "B", status: "Fossil", needsTriage: false, decision: "keep")]
        let queue = GardenPage.titleLine(rows, scope: .needsTriage, visibleCount: 2, cells: cells)
        #expect(queue.text == "NEEDS TRIAGE" + spaces(10) + "1 LEFT")
        #expect(queue.cells.spans.last == CellSpan(" 1 LEFT", cells: 7, foreground: .yellow))
        #expect(queue.cells.accessibilityText == "Needs Triage, 1 left")
        #expect(GardenPage.titleLine(rows, scope: .all, visibleCount: 186, cells: cells).text
            == "ALL FEEDS" + spaces(10) + "186 FEEDS")
        #expect(GardenPage.titleLine(rows, scope: .cemetery, visibleCount: 1, cells: cells).text
            == "CEMETERY" + spaces(14) + "1 FEED")
    }

    /// The mosaic gauge: `▐`, done cells in green, the rest in blue shade, then done/total, always
    /// exactly 28 cells, from an empty garden to a five-digit one.
    @Test func progressRowIsExactlyTheColumnWidth() {
        let cases: [(left: Int, total: Int, expected: String)] = [
            (68, 80, "▐" + String(repeating: "█", count: 3) + String(repeating: "░", count: 18) + " 12/80"),
            (0, 0, "▐" + String(repeating: "░", count: 23) + " 0/0"),
            (0, 80, "▐" + String(repeating: "█", count: 21) + " 80/80"),
            (2, 3, "▐" + String(repeating: "█", count: 7) + String(repeating: "░", count: 16) + " 1/3"),
            (0, 12345, "▐" + String(repeating: "█", count: 15) + " 12345/12345"),
            (90, 80, "▐" + String(repeating: "░", count: 22) + " 0/80"),
        ]
        for (left, total, expected) in cases {
            let line = GardenPage.progressLine(GardenModel.Progress(left: left, total: total), cells: cells)
            #expect(line.text == expected, "\(left)/\(total)")
            #expect(line.cells.width == 28, "\(left)/\(total)")
            #expect(line.role == .progress)
        }
        let line = GardenPage.progressLine(GardenModel.Progress(left: 68, total: 80), cells: cells)
        #expect(line.cells.spans == [
            CellSpan("▐", cells: 1, foreground: .cyan),
            CellSpan("███", cells: 3, foreground: .green),
            CellSpan(String(repeating: "░", count: 18), cells: 18, foreground: .blue),
            CellSpan(" 12/80", cells: 6, foreground: .yellow),
        ])
        #expect(line.cells.accessibilityText == "12 of 80 triaged")
        #expect(GardenPage.entryLines([], scope: .all, retained: [], selected: nil, cells: cells)
            .allSatisfy { $0.role != .progress }, "the gauge belongs to P800 only")
    }

    @Test func feedRowsShowTheMarkerTheTitleAndTheChip() {
        let waiting = GardenPage.feedLine(gardenRow(1, "Zed", status: "Dead", action: .bury, needsTriage: true),
                                          selected: false, cells: cells)
        #expect(waiting.text == "● Zed" + spaces(19) + "bury")
        #expect(waiting.cells.spans.first == CellSpan("●", cells: 2, foreground: .yellow))
        #expect(waiting.cells.spans.last == CellSpan(" bury", cells: 5, foreground: .cyan))
        #expect(waiting.cells.spans.allSatisfy { $0.background == .black })
        #expect(!waiting.isSelected)
        #expect(waiting.role == .feed(1))
        #expect(waiting.cells.accessibilityText == "Zed, Dead, bury")

        let kept = GardenPage.feedLine(gardenRow(2, "Old", status: "Fossil", needsTriage: false, decision: "keep"),
                                       selected: false, cells: cells)
        #expect(kept.text == "  Old" + spaces(19) + "kept")
        #expect(kept.cells.spans.last == CellSpan(" kept", cells: 5, foreground: .green))

        let selected = GardenPage.feedLine(gardenRow(1, "Zed", status: "Dead", action: .bury, needsTriage: true),
                                           selected: true, cells: cells)
        #expect(selected.isSelected)
        #expect(selected.cells.spans.allSatisfy { $0.foreground == .black && $0.background == .white })
        #expect(selected.text == waiting.text)
    }

    /// A buried feed's unread count sits before its chip, in yellow like the index's counts, and
    /// VoiceOver reads it; without unread entries the row is unchanged.
    @Test func buriedFeedRowsShowTheirUnreadCount() {
        let counted = GardenPage.feedLine(gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury", unread: 12),
                                          selected: false, cells: cells)
        #expect(counted.text == "  Gone" + spaces(12) + " 12 buried")
        #expect(counted.cells.width == 28)
        #expect(counted.cells.spans[2] == CellSpan(" 12", cells: 3, foreground: .yellow))
        #expect(counted.cells.spans.last == CellSpan(" buried", cells: 7, foreground: .green))
        #expect(counted.cells.accessibilityText == "Gone, Dead, 12 unread, buried")

        let none = GardenPage.feedLine(gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury"),
                                       selected: false, cells: cells)
        #expect(none.text == "  Gone" + spaces(15) + " buried")
        #expect(none.cells.spans.count == 3)
        #expect(none.cells.accessibilityText == "Gone, Dead, buried")
    }

    /// VoiceOver reads the original title; the cells show a `?` for what Bedstead lacks, which
    /// `CellSpan` draws in magenta even in reverse video.
    @Test func missingGlyphsAreReplacedAndVoiceOverReadsTheOriginal() {
        let line = GardenPage.feedLine(gardenRow(3, "Café 🎉", status: "Dead", action: .bury, needsTriage: true),
                                       selected: true, cells: cells)
        #expect(line.text.hasPrefix("● Café ?"))
        #expect(line.cells.width == 28)
        #expect(line.cells.spans[1].cells.runs.contains(CellRun(text: "?", isReplacement: true)))
        #expect(line.cells.accessibilityText == "Café 🎉, Dead, bury")
    }

    @Test func emptyPagesShowOneCentredCyanLine() {
        let queue = GardenPage.entryLines([], scope: .needsTriage, retained: [], selected: nil, cells: cells)
        #expect(queue.map(\.role) == [.title, .progress, .blank, .text])
        #expect(queue.last?.text == spaces(5) + "NOTHING TO TRIAGE" + spaces(6))
        #expect(queue.last?.cells.spans.allSatisfy { $0.foreground == .cyan } == true)
        #expect(queue.last?.cells.accessibilityText == "Nothing to triage")
        let cemetery = GardenPage.entryLines([gardenRow(1, "Alive", status: "Alive")], scope: .cemetery, retained: [],
                                             selected: nil, cells: cells)
        #expect(cemetery.last?.text == spaces(3) + "THE CEMETERY IS EMPTY" + spaces(4))
    }

    @Test func arrowsMoveTheSelectionAndStopAtTheEnds() {
        let ordered = [gardenRow(1, "A", status: "Dead"), gardenRow(2, "B", status: "Dead"), gardenRow(3, "C", status: "Dead")]
        #expect(GardenPage.move(nil, in: ordered, by: 1) == 1)
        #expect(GardenPage.move(nil, in: ordered, by: -1) == 3)
        #expect(GardenPage.move(1, in: ordered, by: 1) == 2)
        #expect(GardenPage.move(1, in: ordered, by: -1) == 1)
        #expect(GardenPage.move(3, in: ordered, by: 1) == 3)
        #expect(GardenPage.move(9, in: ordered, by: 1) == 1)
        #expect(GardenPage.move(9, in: [], by: 1) == 9)
        #expect(!GardenPage.takesArrows(column: .index))
        #expect(GardenPage.takesArrows(column: .entries))
        #expect(GardenPage.takesArrows(column: .reader))
    }

    /// The queue keeps a feed decided during the visit, without its dot and with its decision.
    @Test func theQueueKeepsDecidedFeedsUntilThePageIsLeft() {
        let decided = gardenRow(2, "Decided", status: "Dead", needsTriage: false, decision: "keep")
        let rows = [gardenRow(1, "Waiting", status: "Dead", needsTriage: true), decided]
        let kept = GardenPage.entryLines(rows, scope: .needsTriage, retained: [1, 2], selected: nil, cells: cells)
        #expect(kept.map(\.role).contains(.feed(2)))
        #expect(kept.first { $0.role == .feed(2) }?.text.hasPrefix("  Decided") == true)
        let fresh = GardenPage.entryLines(rows, scope: .needsTriage, retained: [1], selected: nil, cells: cells)
        #expect(!fresh.map(\.role).contains(.feed(2)))
    }
}

@MainActor @Suite struct GardenInspectorPageTests {
    private let log = [FetchLogRow(id: 1, at: Date(timeIntervalSince1970: 1_791_615_600), httpStatus: 410, errorKind: nil,
                                   redirectChain: [Hop(status: 301, url: "https://www.unphotographable.com/index.rdf")])]

    @Test func theInspectorIsATeletextPage() throws {
        let row = gardenRow(7, "Unphotographable", status: "Ailing", folder: "Blogs / Photo",
                            reason: "The feed is not valid XML.", evidence: ["Parse error at line 3."], remarks: [.httpOnly],
                            needsTriage: true, site: "http://www.unphotographable.com/")
        let lines = GardenPage.inspectorLines(row, log: log, cells: 40, timeZone: .gmt)
        #expect(lines.map { trailingTrimmed($0.text) } == [
            "Unphotographable",
            "Blogs / Photo",
            "",
            " AILING",
            "The feed is not valid XML.",
            "",
            "EVIDENCE",
            "- Parse error at line 3.",
            "",
            "REMARKS",
            "- Only served over http://.",
            "",
            "LINKS",
            "FEED      https://feeds.example.org/7",
            "SITE      http://www.unphotographable.co",
            "          m/",
            "",
            "REDIRECTS",
            "301 https://www.unphotographable.com/ind",
            "    ex.rdf",
            "",
            "FETCH LOG",
            "10 OCT 26 07:00  HTTP 410",
        ])
        for line in lines {
            #expect(line.cells.width == 40, "\(line.id): «\(line.text)»")
        }
        let status = try #require(lines.first { $0.isDoubleHeight })
        #expect(status.role == .heading("Ailing"))
        #expect(status.cells.spans.allSatisfy { $0.foreground == .yellow })
        #expect(status.cells.accessibilityText == "Ailing")
        #expect(lines[13].cells.spans.first == CellSpan("FEED      ", cells: 10, foreground: .cyan))
        #expect(lines[13].cells.accessibilityText == "Feed: https://feeds.example.org/7")
        #expect(lines[15].cells.accessibilityText.isEmpty, "a wrapped value is read once")
        #expect(lines[18].cells.accessibilityText == "301 to https://www.unphotographable.com/index.rdf")
        #expect(lines[22].cells.accessibilityText == "10 OCT 26 07:00, HTTP 410")
    }

    @Test func aBuriedFossilShowsHowToRestoreIt() throws {
        let fossil = gardenRow(8, "Old blog", status: "Fossil", state: "buried", action: .bury, decision: "bury")
        let page = GardenPage.inspectorLines(fossil, log: [], cells: 40, timeZone: .gmt)
        let status = try #require(page.first { $0.isDoubleHeight })
        #expect(status.cells.spans.allSatisfy { $0.foreground == .white && $0.background == .blue })
        let texts = page.map { trailingTrimmed($0.text) }
        #expect(texts.contains("Buried. Press k to restore it."))
        #expect(!texts.contains { $0.hasPrefix("PROPOSED") }, "nothing is proposed for a buried feed")
        #expect(texts.contains("DECISION  Bury"))
        #expect(texts.contains("Not fetched yet."))
    }

    @Test func withoutASelectionTheReaderSaysSo() {
        let none = GardenPage.inspectorLines(nil, log: [], cells: 40)
        #expect(none.map(\.text) == [spaces(40), spaces(12) + "NO FEED SELECTED" + spaces(12)])
        #expect(none.last?.cells.spans.allSatisfy { $0.foreground == .cyan } == true)
        #expect(none.last?.cells.accessibilityText == "No feed selected")
    }

    /// Spec §5: blue is for separators, backgrounds and mosaics, never for text.
    @Test func blueNeverColoursText() {
        let rows = (GardenModel.urgencyOrder + ["Buried"]).enumerated().map {
            gardenRow(Int64($0.offset + 1), $0.element, status: $0.element, state: $0.element == "Buried" ? "buried" : "active",
                      needsTriage: true)
        }
        var lines = GardenPage.entryLines(rows, scope: .all, retained: [], selected: 3, cells: EntryPageColumn.cells)
        for row in rows {
            lines += GardenPage.inspectorLines(row, log: log, cells: 40)
        }
        for line in lines where line.role != .progress {
            #expect(!line.cells.spans.contains { $0.foreground == .blue }, "\(line.id): «\(line.text)»")
        }
    }

    @Test func wrapBreaksOnWordsAndSplitsLongWords() {
        #expect(GardenPage.wrap("The quick brown fox jumps", cells: 10) == ["The quick", "brown fox", "jumps"])
        #expect(GardenPage.wrap("abcdefghijkl xy", cells: 5) == ["abcde", "fghij", "kl xy"])
        #expect(GardenPage.wrap("a\nb", cells: 10) == ["a b"])
        #expect(GardenPage.wrap("", cells: 10).isEmpty)
        #expect(GardenPage.wrap("anything", cells: 0).isEmpty)
    }

    @Test func logDatesAreTeletextCapitals() {
        #expect(GardenPage.logDate(Date(timeIntervalSince1970: 1_791_615_600), timeZone: .gmt) == "10 OCT 26 07:00")
    }
}

@MainActor @Suite struct AntiopeBannerTests {
    @Test func theBannerRowFillsTheWindowWidth() {
        let failure = Banner(message: "ressac exited with code 1: the database is locked.", showLog: true)
        let row = GardenPage.bannerRow(failure, cells: 60)
        #expect(row.width == 60)
        #expect(row.log?.accessibilityText == "Show Log" && row.dismiss.accessibilityText == "Dismiss")
        #expect(row.message.spans.allSatisfy { $0.foreground == .white && $0.background == .red })
        #expect([row.log, row.dismiss].compactMap { $0 }.allSatisfy {
            $0.spans.allSatisfy { $0.foreground == .yellow && $0.background == .red }
        })
        #expect(text(of: row.message.spans[0]).hasPrefix(" ressac exited with code 1"))
        #expect(row.message.accessibilityText == failure.message)

        let network = Banner(message: "The network looks unreachable: 12 of 186 feeds could not connect.", showLog: false)
        let short = GardenPage.bannerRow(network, cells: 40)
        #expect(short.log == nil)
        #expect(short.width == 40)
    }

    /// The search prompt keeps its place ahead of all this (phase 9); then the garden's last
    /// refusal, then the run, then the hints.
    @Test func theMinitelBarShowsRefusalsThenRunsThenItsHints() {
        let run = RunProgress(done: 42, total: 155, label: "Fetching feeds")
        #expect(MinitelBar.status(progress: run, message: nil) == MinitelHint(text: "REFRESHING 42/155", colour: .yellow))
        #expect(MinitelBar.status(progress: RunProgress(done: 0, total: 0, label: "Fetching feeds"), message: nil)?.text
            == "REFRESHING")
        #expect(MinitelBar.status(progress: run, message: "Nothing to repair for this feed.")?.text
            == "Nothing to repair for this feed.")
        #expect(MinitelBar.status(progress: nil, message: nil) == nil)
        #expect(MinitelBar.hintLine(MinitelBar.status(progress: run, message: nil)!, width: 30).width == 30)
    }
}

@MainActor @Suite struct GardenStoreTests {
    /// Two feeds needing triage in the contract database: Moved (feed 3) comes before Dead (feed 2).
    private func diagnose(_ test: TestDatabase) async throws {
        try await test.raw.write { db in
            try db.execute(sql: """
                INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
                VALUES (2, 'Dead', 'Gone.', '{"bullets":[],"clue":"","language":null}', '[]', '{"type":"bury"}',
                        '2026-10-10T08:00:00.000Z', 1),
                       (3, 'Moved', 'Moved.', '{"bullets":[],"clue":"301","language":null}', '[]',
                        '{"type":"follow_redirect","url":"https://moved.example.org/feed"}', '2026-10-10T08:00:00.000Z', 1);
                """)
        }
    }

    @Test func enteringTheQueueSelectsTheFirstFeedToTriage() async throws {
        let (state, test, _) = try makeState()
        try await diagnose(test)
        state.selection = .garden(.needsTriage)
        let store = GardenStore()
        let observation = Task { await store.observe(page: .needsTriage, app: state) }
        defer { observation.cancel() }
        #expect(await eventually { store.rows.contains { $0.feedId == 3 } })
        #expect(await eventually { state.selectedFeedId == 3 })
        #expect(store.selectedRow(state)?.status == "Moved")
        #expect(store.message == nil)
    }

    @Test func gardenCommandsActAsInTheNativeTable() async throws {
        let (state, test, stub) = try makeState()
        try await diagnose(test)
        var opened: [URL] = []
        state.openURL = { opened.append($0) }
        state.selection = .garden(.needsTriage)
        state.selectedFeedId = 3
        let store = GardenStore()

        store.perform(.message("Nothing to repair for this feed."), app: state)
        #expect(store.message == "Nothing to repair for this feed.")
        store.perform(.confirmSafeRepairs, app: state)
        #expect(store.confirmingSafeRepairs)
        #expect(store.message == nil, "a new key clears the last refusal")
        store.perform(.open(URL(string: "https://example.org/")!), app: state)
        #expect(opened == [URL(string: "https://example.org/")!])
        store.perform(.decide(.keep), app: state)
        #expect(await eventually { (try? calls(stub))?.contains { $0.hasSuffix("garden decide 3 keep") } == true })
        store.perform(.undo, app: state)
        #expect(await eventually { (try? calls(stub))?.contains { $0.hasSuffix("garden undo") } == true })
    }

    /// The garden keys reach `GardenEntryPage` only if neither Antiope nor `AppState` takes them
    /// first: `k` is Keep here, not RETOUR (phase 7's `AntiopeState.handle` must leave it).
    @Test func readingCommandsFallThroughInTheGarden() throws {
        let (state, _, _) = try makeState()
        let antiope = AntiopeState(app: state)
        state.selection = .garden(.needsTriage)
        for command in [ReadingCommand.previousEntry, .nextEntry, .advance, .toggleRead, .toggleStar] {
            #expect(!antiope.handle(command), "\(command)")
            #expect(!state.perform(command), "\(command)")
        }
    }

    /// The glyphs the garden pages draw outside `CellText` must exist in Bedstead.
    @Test(.enabled(if: TeletextFont.isAvailable)) func bedsteadDrawsTheGardenGlyphs() {
        for glyph: Character in ["▐", "█", "░", "●"] {
            #expect(TeletextFont.supports(glyph), "\(glyph)")
        }
    }
}
