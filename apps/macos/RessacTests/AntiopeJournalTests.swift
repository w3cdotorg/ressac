import Foundation
import Testing

@testable import Ressac

private let paris = TimeZone(identifier: "Europe/Paris")!

private func item(_ id: Int64, _ section: EditionSection, rank: Int, feed: Int64, feedTitle: String,
                  title: String? = "Entry", read: Bool = false, starred: Bool = false) -> EditionItem {
    EditionItem(entryId: id, section: section, rank: rank, feedId: feed, feedTitle: feedTitle, title: title,
                publishedAt: Timestamp.date(from: "2026-10-16T05:00:00.000Z")!, isRead: read, isStarred: starred)
}

/// Built on Friday 16 October 2026 at 09:12 in Paris: one front-page entry, two weekly entries of
/// one feed, one wire entry with 12 more left in Actualitté (page 412).
private func sampleEdition(read: Set<Int64> = [], unreachable: Int = 0, items: [EditionItem]? = nil) -> Edition {
    let built = Timestamp.date(from: "2026-10-16T07:12:00.000Z")!
    let all = items ?? [
        item(1, .front, rank: 1, feed: 30, feedTitle: "Bibliobsession", title: "A rare post"),
        item(2, .week, rank: 2, feed: 20, feedTitle: "Six Colors", title: "Weekly one", starred: true),
        item(3, .week, rank: 3, feed: 20, feedTitle: "Six Colors", title: "Weekly two"),
        item(4, .wire, rank: 4, feed: 10, feedTitle: "Actualitté", title: "Latest news"),
    ]
    return Edition(
        info: EditionInfo(id: 12, builtAt: built, windowStart: built.addingTimeInterval(-86_400), windowEnd: built,
                          unreachable: unreachable),
        items: all.map { entry in
            var entry = entry
            entry.isRead = entry.isRead || read.contains(entry.entryId)
            return entry
        },
        overflow: [EditionOverflowItem(section: .wire, feedId: 10, feedTitle: "Actualitté", page: 412, hiddenCount: 12)])
}

private func trimmed(_ text: String) -> String {
    text.trimmingCharacters(in: .whitespaces)
}

/// The rows are phase 8's `CellLine`s, read with Bedstead registered in the test host
/// (`?` stands for a glyph Bedstead lacks), as the garden suites do.
@MainActor @Suite struct JournalPageTests {
    let cells = EntryPageColumn.cells

    @Test func sectionsComeInOrderWithTheirEntriesAndOverflow() {
        let lines = JournalPage.lines(edition: sampleEdition(), phase: .idle, selected: nil, cells: cells, timeZone: paris)
        #expect(lines.map(\.role) == [
            .header,
            .blank, .heading(.front), .entry(1),
            .blank, .heading(.week), .entry(2), .entry(3),
            .blank, .heading(.wire), .entry(4), .overflow(feedId: 10, page: 412),
        ])
        #expect(Set(lines.map(\.id)).count == lines.count)  // ForEach needs unique ids
        let header = lines[0].cells
        #expect(trimmed(text(of: header.spans[0])) == "FRI 16 OCT" && header.spans[0].foreground == .cyan)
        #expect(text(of: header.spans[1]) == " 0/4 READ" && header.spans[1].foreground == .yellow)
        #expect(header.accessibilityText == "Friday 16 October · 4 entries · 0 read")
        #expect(trimmed(text(of: lines[2].cells.spans[0])) == "FRONT PAGE" && text(of: lines[2].cells.spans[1]) == " 1")
        #expect(trimmed(text(of: lines[5].cells.spans[0])) == "THIS WEEK" && text(of: lines[5].cells.spans[1]) == " 2")
        #expect(trimmed(text(of: lines[9].cells.spans[0])) == "THE WIRE" && text(of: lines[9].cells.spans[1]) == " 1")
        #expect(lines[2].cells.accessibilityText == "Front Page, 1 entry")
        #expect(lines[5].cells.accessibilityText == "This Week, 2 entries")
    }

    @Test func headingsAreColouredAndBlueNeverColoursText() throws {
        let lines = JournalPage.lines(edition: sampleEdition(), phase: .idle, selected: nil, cells: cells, timeZone: paris)
        #expect(JournalPage.colour(for: .front) == .yellow)
        #expect(JournalPage.colour(for: .week) == .cyan)
        #expect(JournalPage.colour(for: .wire) == .green)
        for line in lines {
            if case let .heading(section) = line.role {
                #expect(line.cells.spans.allSatisfy { $0.foreground == JournalPage.colour(for: section) && $0.background == .black })
            }
        }
        let overflow = try #require(lines.last)
        #expect(overflow.cells.spans[1].foreground == .white && overflow.cells.spans[1].background == .blue)
        #expect(!lines.flatMap(\.cells.spans).contains { $0.foreground == .blue })
    }

    @Test func entriesShowUnreadAndStarredMarkersAndTheSelection() throws {
        let lines = JournalPage.lines(edition: sampleEdition(read: [3]), phase: .idle, selected: 2, cells: cells,
                                      timeZone: paris)
        let two = try #require(lines.first { $0.role == .entry(2) })
        #expect(two.isSelected && two.cells.spans.allSatisfy { $0.foreground == .black && $0.background == .white })
        #expect(text(of: two.cells.spans[0]) == "● " && text(of: two.cells.spans[2]) == " *")
        #expect(two.cells.accessibilityText == "Weekly one, Six Colors, unread, starred")
        let three = try #require(lines.first { $0.role == .entry(3) })
        #expect(!three.isSelected && text(of: three.cells.spans[0]) == "  ")
        #expect(three.cells.accessibilityText == "Weekly two, Six Colors")
    }

    /// Review Focus #5: whatever the feed's title, an overflow row is exactly the column's 28
    /// cells, so the separator stays straight; VoiceOver reads the real title.
    @Test func anOverflowRowIsExactlyTheColumnWidth() {
        let awkward = EditionOverflowItem(section: .wire, feedId: 5, feedTitle: "Café ☕ Pédago", page: nil, hiddenCount: 3)
        let line = JournalPage.overflowLine(awkward, cells: cells)
        #expect(line.cells.width == cells)
        #expect(line.text == "  +3 MORE IN CAFÉ ? PÉDAGO  ")
        #expect(line.cells.spans[1].cells.runs.contains { $0.isReplacement && $0.text == "?" })
        #expect(line.cells.accessibilityText == "+3 more in Café ☕ Pédago")
        #expect(line.role == .overflow(feedId: 5, page: nil))
        let long = EditionOverflowItem(section: .week, feedId: 6, feedTitle: String(repeating: "東京", count: 30),
                                       page: 300, hiddenCount: 120)
        #expect(JournalPage.overflowLine(long, cells: cells).cells.width == cells)
    }

    @Test func everyRowIsExactlyTheWidthAsked() {
        var awkward = sampleEdition(unreachable: 3)
        awkward.items[0].title = "Café 🎉 東京 e\u{301}cole, a title far longer than two rows of the column"
        awkward.items[1].title = nil
        for width in [0, 1, 2, 9, 28, 40] {
            for phase in [JournalModel.Phase.idle, .building(done: 34, total: 156), .offline, .failed("Database is locked.")] {
                for line in JournalPage.lines(edition: awkward, phase: phase, selected: 1, cells: width, timeZone: paris) {
                    #expect(line.cells.width == width)
                }
            }
        }
    }

    @Test func theEndLineComesOnceEveryEntryIsRead() throws {
        let reading = JournalPage.lines(edition: sampleEdition(read: [1, 2, 3]), phase: .idle, selected: nil, cells: cells,
                                        timeZone: paris)
        #expect(!reading.contains { $0.role == .end })
        let done = JournalPage.lines(edition: sampleEdition(read: [1, 2, 3, 4]), phase: .idle, selected: nil, cells: cells,
                                     timeZone: paris)
        let end = try #require(done.last)
        #expect(end.role == .end && end.isDoubleHeight)
        #expect(trimmed(end.text) == "FIN DE L'EDITION" && end.cells.spans[0].foreground == .yellow)
        #expect(end.cells.accessibilityText == "That's the edition.")
        #expect(text(of: done[0].cells.spans[1]) == " 4/4 READ")
    }

    @Test func emptyBuildingOfflineAndUnreachable() {
        let empty = JournalPage.lines(edition: sampleEdition(items: []), phase: .idle, selected: nil, cells: cells,
                                      timeZone: paris)
        #expect(empty.map(\.role) == [.header, .blank, .message])
        #expect(trimmed(empty[2].text) == "NOTHING NEW THIS MORNING.")
        #expect(!empty.contains { $0.role == .end })  // an empty edition is not "finished"

        // While building, only the progress, even over yesterday's edition (spec §8).
        let building = JournalPage.lines(edition: sampleEdition(), phase: .building(done: 34, total: 156), selected: nil,
                                         cells: cells, timeZone: paris)
        #expect(building.map(\.role) == [.blank, .message])
        #expect(trimmed(building[1].text) == "EDITION IN PROGRESS 34/156")
        #expect(building[1].cells.accessibilityText == "Building this morning's edition… 34/156")
        #expect(JournalPage.progressText(done: 0, total: 0) == "EDITION IN PROGRESS")
        let none = JournalPage.lines(edition: nil, phase: .due, selected: nil, cells: cells, timeZone: paris)
        #expect(trimmed(none[1].text) == "EDITION IN PROGRESS")

        let offline = JournalPage.lines(edition: nil, phase: .offline, selected: nil, cells: cells, timeZone: paris)
        #expect(offline.map(\.role) == [.blank, .message, .message])
        #expect(trimmed(offline[1].text) == "OFFLINE: NO EDITION YET")
        #expect(offline[1].cells.spans.allSatisfy { $0.foreground == .black && $0.background == .red })
        #expect(offline[1].cells.accessibilityText == "Offline: no edition yet")
        #expect(trimmed(offline[2].text) == "RÉPÉTITION: RETRY" && offline[2].cells.spans[0].foreground == .cyan)
        let failed = JournalPage.lines(edition: nil, phase: .failed("Database is locked."), selected: nil, cells: cells,
                                       timeZone: paris)
        #expect(trimmed(failed[1].text) == "DATABASE IS LOCKED.")

        let three = JournalPage.headerLines(sampleEdition(unreachable: 3), cells: cells, timeZone: paris)
        #expect(three.count == 2 && trimmed(three[1].text) == "3 FEEDS UNREACHABLE")
        #expect(three[1].cells.spans[0].foreground == .red && three[1].cells.accessibilityText == "3 feeds unreachable")
        let one = JournalPage.headerLines(sampleEdition(unreachable: 1), cells: cells, timeZone: paris)
        #expect(trimmed(one[1].text) == "1 FEED UNREACHABLE")
        #expect(JournalPage.headerLines(sampleEdition(), cells: cells, timeZone: paris).count == 1)
    }

    @Test func overflowWithoutAPlacedEntryClosesItsSection() throws {
        var edition = sampleEdition()
        edition.overflow.append(EditionOverflowItem(section: .week, feedId: 99, feedTitle: "Capped out", page: nil,
                                                    hiddenCount: 4))
        let roles = JournalPage.lines(edition: edition, phase: .idle, selected: nil, cells: cells, timeZone: paris).map(\.role)
        let week = try #require(roles.firstIndex(of: .heading(.week)))
        #expect(Array(roles[week...].prefix(4)) == [.heading(.week), .entry(2), .entry(3), .overflow(feedId: 99, page: nil)])
        #expect(JournalPage.lastEntryId(of: edition) == 4 && JournalPage.lastEntryId(of: nil) == nil)
    }
}

@MainActor @Suite struct JournalNavigationTests {
    /// An edition of two entries, the first ranked one published *before* the second, so that rank
    /// order and date order differ; P104 selected.
    /// `keepAlive` holds the test database and the stub: both delete their directory on deinit.
    func makeEdition() async throws -> (AntiopeState, AppState, first: Int64, last: Int64, keepAlive: (TestDatabase, Stub)) {
        let (state, test, stub) = try makeState()
        let first = try test.entry(feed: 1, title: "Ranked first", published: "2026-10-09T06:00:00.000Z")
        let last = try test.entry(feed: 8, title: "Ranked last", published: "2026-10-09T09:00:00.000Z")
        try test.execute("DELETE FROM edition")  // whatever the contract fixture holds
        try test.execute("""
            INSERT INTO edition (id, built_at, window_start, window_end, unreachable)
            VALUES (1, '2026-10-10T07:00:00.000Z', '2026-10-09T07:00:00.000Z', '2026-10-10T07:00:00.000Z', 0)
            """)
        try test.execute("INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?, 'front', 1)", [first])
        try test.execute("INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?, 'wire', 2)", [last])
        try await state.database.notifyExternalChanges()
        state.autoReadDelay = .seconds(60)
        state.selection = .journal
        #expect(await eventually { state.journal.edition?.items.count == 2 && state.entries.count == 2 })
        // Phase 5b's contract: on P104 the list runs in rank order, not by date.
        #expect(state.entries.map(\.entryId) == [first, last])
        return (AntiopeState(app: state), state, first, last, (test, stub))
    }

    @Test func suiteFromTheLastEntryLandsOnTheEndOfTheEdition() async throws {
        let (antiope, state, first, last, keepAlive) = try await makeEdition()
        defer { withExtendedLifetime(keepAlive) {} }
        state.selectedEntryId = first
        #expect(await eventually { state.selectedEntry != nil })
        #expect(!antiope.handle(.nextEntry))  // not the last entry: AppState moves on
        #expect(state.perform(.nextEntry) && state.selectedEntryId == last)
        #expect(antiope.handle(.nextEntry))   // the last one: FIN DE L'EDITION
        #expect(antiope.readerPage == .endOfEdition && antiope.currentPage == 104)
        #expect(await eventually { state.entries.first { $0.entryId == last }?.isRead == true })  // read on leaving
        #expect(antiope.handle(.nextEntry) && antiope.handle(.advance))  // nothing after the end
        #expect(antiope.readerPage == .endOfEdition && state.selectedEntryId == last)
        #expect(antiope.handle(.previousEntry))  // RETOUR brings the last entry back
        #expect(antiope.readerPage == .article && state.selectedEntryId == last)
        // Space at the bottom of the last entry (no reader in the test: nothing left to scroll).
        #expect(antiope.handle(.advance))
        #expect(await eventually { antiope.readerPage == .endOfEdition })
    }

    @Test func elsewhereTheLastEntryStaysAppStates() async throws {
        let (antiope, state, _, _, keepAlive) = try await makeEdition()
        defer { withExtendedLifetime(keepAlive) {} }
        state.selection = .unread
        #expect(await eventually { state.entries.count == 2 })
        state.selectedEntryId = state.entries.last?.entryId
        #expect(!antiope.handle(.nextEntry) && !antiope.handle(.advance))
        #expect(antiope.readerPage == .article)
    }

    @Test func aJumpOrAClickLeavesTheEndPage() async throws {
        let (antiope, state, _, last, keepAlive) = try await makeEdition()
        defer { withExtendedLifetime(keepAlive) {} }
        state.selectedEntryId = last
        #expect(await eventually { state.selectedEntry != nil })
        #expect(antiope.handle(.nextEntry) && antiope.readerPage == .endOfEdition)
        antiope.jump(to: 101)
        #expect(antiope.readerPage == .article && state.selection == .unread)
    }

    @Test func repetitionRetriesOnlyAnEditionThatCouldNotBeBuilt() {
        #expect(AntiopeState.retriesJournal(.offline))
        #expect(AntiopeState.retriesJournal(.failed("Database is locked.")))
        // Turned away by another long run: the edition is still due, RÉPÉTITION asks again.
        #expect(AntiopeState.retriesJournal(.due))
        for phase in [JournalModel.Phase.idle, .building(done: 1, total: 2)] {
            #expect(!AntiopeState.retriesJournal(phase))
        }
    }

    @Test func theMinitelBarShowsTheBuild() {
        let progress = RunProgress(done: 34, total: 156, label: "")
        #expect(MinitelBar.status(progress: progress, message: nil, journal: .building(done: 34, total: 156))?.text
            == "EDITION IN PROGRESS 34/156")
        #expect(MinitelBar.status(progress: progress, message: nil, journal: .building(done: 34, total: 156))?.colour
            == .yellow)
        #expect(MinitelBar.status(progress: progress, message: nil)?.text == "REFRESHING 34/156")
        #expect(MinitelBar.status(progress: nil, message: "Feed #3 is buried.", journal: .building(done: 1, total: 2))?.text
            == "Feed #3 is buried.")
        // The header's page number "searches" during every long run; the build is one (phase 4b).
        #expect(RunRequest.buildEdition().kind == .long)
    }
}

@Suite struct EditionSubpageTests {
    @Test func anEditionEntryIsASubPageOf104ByItsRank() {
        let edition = sampleEdition()
        #expect(SubpageLabel.ofEdition(entryId: 3, in: edition)?.text == "104 3/4")
        #expect(SubpageLabel.ofEdition(entryId: 1, in: edition)?.text == "104 1/4")
        #expect(SubpageLabel.ofEdition(entryId: 99, in: edition) == nil)  // not in the edition
        #expect(SubpageLabel.ofEdition(entryId: 1, in: nil) == nil)
    }
}
