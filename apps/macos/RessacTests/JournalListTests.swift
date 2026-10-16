import Foundation
import GRDB
import Testing

@testable import Ressac

/// An `AppState` over a fixture copy with empty edition tables (the stub only records its calls).
@MainActor
func makeJournalState() throws -> (AppState, TestDatabase, Stub) {
    let (state, test, stub) = try makeState()
    try test.execute("DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition;")
    return (state, test, stub)
}

/// A diagnosis row with a rhythm group, as `diagnose` stores it.
func health(_ test: TestDatabase, feed: Int64, group: String) throws {
    try test.execute("""
        INSERT INTO feed_health (feed_id, status, reason, computed_at, cadence_group)
        VALUES (?, 'Alive', 'Last entry on 2026-10-15.', '2026-10-16T05:00:00.000Z', ?)
        """, [feed, group])
}

private func listRow(_ id: Int64, feed: Int64, published: String = "2026-10-15T08:00:00.000Z", read: Bool = false) -> EntryRow {
    EntryRow(entryId: id, feedId: feed, feedTitle: "Feed \(feed)", folderId: nil, title: "Entry \(id)", author: nil,
             url: nil, publishedAt: Timestamp.date(from: published)!, isRead: read, isStarred: false,
             hasFulltext: false, summaryHtml: nil, contentHtml: nil)
}

/// An edition built at 07:12 Paris time on Friday 16 October 2026. `items`: (entry, section, rank, feed).
private func sampleEdition(_ items: [(Int64, EditionSection, Int, Int64)], overflow: [EditionOverflowItem] = [],
                           read: Set<Int64> = [], unreachable: Int = 0) -> Edition {
    let built = Timestamp.date(from: "2026-10-16T05:12:00.000Z")!
    return Edition(
        info: EditionInfo(id: 1, builtAt: built, windowStart: built.addingTimeInterval(-86_400), windowEnd: built,
                          unreachable: unreachable),
        items: items.map {
            EditionItem(entryId: $0.0, section: $0.1, rank: $0.2, feedId: $0.3, feedTitle: "Feed \($0.3)",
                        title: "Entry \($0.0)", publishedAt: built, isRead: read.contains($0.0), isStarred: false)
        },
        overflow: overflow)
}

private func overflowItem(_ section: EditionSection, feed: Int64, _ hidden: Int, title: String? = nil) -> EditionOverflowItem {
    EditionOverflowItem(section: section, feedId: feed, feedTitle: title ?? "Feed \(feed)", page: nil, hiddenCount: hidden)
}

@MainActor @Suite struct JournalSourceTests {
    @Test func the_journal_lists_the_edition_in_rank_order() async throws {
        let (state, test, _) = try makeJournalState()
        let early = try test.entry(feed: 1, published: "2026-10-15T06:00:00.000Z")
        let late = try test.entry(feed: 2, published: "2026-10-15T09:00:00.000Z")
        let middle = try test.entry(feed: 8, published: "2026-10-15T07:00:00.000Z")
        _ = try test.entry(feed: 1, published: "2026-10-15T10:00:00.000Z")  // not in the edition
        try seedEdition(test, items: [(middle, "front", 1), (early, "week", 2), (late, "wire", 3)])
        try await state.database.notifyExternalChanges()
        state.selection = .journal
        #expect(state.entries.map(\.entryId) == [middle, early, late])
        #expect(!state.hasMoreEntries)
        #expect(await eventually { state.journal.edition?.items.count == 3 })
        let sections = JournalLayout.sections(entries: state.entries, edition: state.journal.edition)
        #expect(sections.map(\.section) == [.front, .week, .wire])
    }

    @Test func a_feed_buried_after_the_build_leaves_every_journal_surface() async throws {
        let (state, test, _) = try makeJournalState()
        let buried = try test.entry(feed: 1, title: "From the buried feed")
        let first = try test.entry(feed: 2)
        let second = try test.entry(feed: 8)
        try seedEdition(test, items: [(buried, "front", 1), (first, "wire", 2), (second, "wire", 3)])
        try test.execute("UPDATE feed SET state = 'buried' WHERE id = 1")
        state.selection = .journal
        try await state.database.notifyExternalChanges()

        let snapshot = try await value(of: Queries.sidebar(), in: state.database) { $0.journalUnread == 2 }
        #expect(snapshot.journalUnread == 2)
        #expect(await eventually { state.journal.edition?.items.count == 2 })
        let edition = try #require(state.journal.edition)
        #expect(edition.items.map(\.entryId) == [first, second])
        #expect(edition.readCount == 0 && !edition.isFinished)
        #expect(state.entries.map(\.entryId) == [first, second])
        let page = JournalPage.lines(edition: edition, phase: .idle, selected: nil, cells: 40)
        #expect(!page.contains { $0.cells.accessibilityText.contains("buried feed") || $0.text.contains("buried feed") })

        await state.markAllRead()?.value
        #expect(await eventually { state.journal.edition?.isFinished == true })
        let finished = JournalPage.lines(edition: state.journal.edition, phase: .idle, selected: nil, cells: 40)
        #expect(finished.contains { $0.role == .end })
    }

    @Test func an_edition_whose_every_feed_was_buried_reads_as_empty() async throws {
        let (state, test, _) = try makeJournalState()
        let only = try test.entry(feed: 1)
        try seedEdition(test, items: [(only, "wire", 1)])
        try test.execute("UPDATE feed SET state = 'buried' WHERE id = 1")
        try await state.database.notifyExternalChanges()
        let edition = try await value(of: Queries.edition(), in: state.database) { $0 != nil }
        #expect(edition?.items.isEmpty == true && edition?.isFinished == false)
    }

    @Test func mark_all_read_in_the_journal_leaves_the_overflow_unread() async throws {
        let (state, test, _) = try makeJournalState()
        let first = try test.entry(feed: 1)
        let second = try test.entry(feed: 2)
        let leftOut = try test.entry(feed: 1, published: "2026-10-14T08:00:00.000Z")
        try seedEdition(test, items: [(first, "wire", 1), (second, "wire", 2)], overflow: [(feed: 1, section: "wire", hidden: 1)])
        state.selection = .journal
        #expect(state.canMarkAllRead)
        await state.markAllRead()?.value
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") == 1)
        #expect(try test.one(Bool.self, "SELECT read_at IS NULL FROM entry WHERE id = ?", [leftOut]) == true)
    }

    @Test func space_stays_inside_the_edition_in_rank_order() async throws {
        let (state, test, _) = try makeJournalState()
        let first = try test.entry(feed: 1, published: "2026-10-15T06:00:00.000Z")
        let second = try test.entry(feed: 2, published: "2026-10-15T09:00:00.000Z")
        _ = try test.entry(feed: 8, published: "2026-10-15T12:00:00.000Z")  // newer, unread, not in the edition
        try seedEdition(test, items: [(first, "front", 1), (second, "wire", 2)])
        state.selection = .journal
        state.selectedEntryId = first
        state.selectNextUnread()
        #expect(state.selectedEntryId == second)
        state.selectNextUnread()
        #expect(state.selectedEntryId == second)
    }

    @Test func rhythm_groups_list_their_feeds_and_the_sidebar_counts_them() async throws {
        let (state, test, _) = try makeJournalState()
        try health(test, feed: 1, group: "daily")
        try health(test, feed: 2, group: "weekly")
        try health(test, feed: 8, group: "weekly")
        try test.execute("UPDATE feed SET cadence_override = 'rare' WHERE id = 8")
        let daily = try test.entry(feed: 1)
        _ = try test.entry(feed: 1)
        let weekly = try test.entry(feed: 2)
        let rare = try test.entry(feed: 8)
        try seedEdition(test, items: [(daily, "wire", 1)])

        let snapshot = try await value(of: Queries.sidebar(), in: state.database) { $0.journalUnread == 1 }
        #expect(snapshot.cadenceUnread == [.daily: 2, .weekly: 1, .rare: 1])

        state.selection = .cadence(.weekly)
        #expect(state.entries.map(\.entryId) == [weekly])
        state.selection = .cadence(.rare)
        #expect(state.entries.map(\.entryId) == [rare])
        state.selection = .cadence(.silent)
        #expect(state.entries.isEmpty)
    }

    @Test func refresh_from_the_journal_or_a_rhythm_group_fetches_every_feed() async throws {
        let (state, _, stub) = try makeJournalState()
        state.selection = .journal
        _ = await state.refreshCurrent().value
        state.selection = .cadence(.daily)
        _ = await state.refreshCurrent().value
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == [
            "fetch --force", "fetch --force",
        ])
    }

    @Test func rows_kept_after_reading_follow_the_edition_order() {
        let one = listRow(1, feed: 1, published: "2026-10-15T09:00:00.000Z")
        let two = listRow(2, feed: 2, published: "2026-10-15T06:00:00.000Z")
        let three = listRow(3, feed: 3, published: "2026-10-15T07:00:00.000Z")
        // Unread only: entry 1 was read during the visit and is kept; the edition ranks 3, 1, 2.
        let merged = AppState.merge(fresh: [two, three], previous: [three, one, two], retained: [1], floor: nil,
                                    rank: [3: 1, 1: 2, 2: 3])
        #expect(merged.map(\.entryId) == [3, 1, 2])
        #expect(merged[1].isRead)
        // Without ranks, newest first as before.
        #expect(AppState.merge(fresh: [two, three], previous: [three, one, two], retained: [1], floor: nil)
            .map(\.entryId) == [1, 3, 2])
    }
}

/// `@MainActor`: `JournalModel.Phase` is declared inside a main-actor class.
@MainActor @Suite struct JournalLayoutTests {
    @Test func entries_go_under_their_section_and_overflow_follows_its_feed() {
        let entries = [listRow(1, feed: 5), listRow(2, feed: 3), listRow(3, feed: 3), listRow(4, feed: 4), listRow(5, feed: 1)]
        let edition = sampleEdition(
            [(1, .front, 1, 5), (2, .week, 2, 3), (3, .week, 3, 3), (4, .week, 4, 4), (5, .wire, 5, 1)],
            overflow: [overflowItem(.week, feed: 3, 2), overflowItem(.wire, feed: 1, 12), overflowItem(.wire, feed: 9, 4)])
        let sections = JournalLayout.sections(entries: entries, edition: edition)
        #expect(sections.map(\.section) == [.front, .week, .wire])
        #expect(sections.map { $0.rows.map(\.id) } == [
            ["entry-1"],
            ["entry-2", "entry-3", "overflow-week-3", "entry-4"],
            // Feed 9 has no row in the list: its overflow closes the section.
            ["entry-5", "overflow-wire-1", "overflow-wire-9"],
        ])
    }

    @Test func a_section_without_rows_is_omitted() {
        let edition = sampleEdition([(2, .week, 1, 3)])
        #expect(JournalLayout.sections(entries: [listRow(2, feed: 3)], edition: edition).map(\.section) == [.week])
        #expect(JournalLayout.sections(entries: [listRow(2, feed: 3)], edition: nil).isEmpty)
    }

    @Test func retry_is_offered_whenever_the_edition_could_not_be_built() {
        for phase in [JournalModel.Phase.offline, .failed("Database is locked."), .due] {
            #expect(JournalListView.offersRetry(phase))
        }
        for phase in [JournalModel.Phase.idle, .building(done: 1, total: 2)] {
            #expect(!JournalListView.offersRetry(phase))
        }
    }

    @Test func the_header_counts_entries_read_entries_and_unreachable_feeds() {
        let items: [(Int64, EditionSection, Int, Int64)] = [(1, .front, 1, 1), (2, .wire, 2, 2), (3, .wire, 3, 3)]
        #expect(JournalLayout.header(sampleEdition(items, read: [2]), calendar: .paris)
            == "Friday 16 October · 3 entries · 1 read")
        #expect(JournalLayout.header(sampleEdition(items, unreachable: 3), calendar: .paris)
            == "Friday 16 October · 3 entries · 0 read · 3 feeds unreachable")
        #expect(JournalLayout.header(sampleEdition([(1, .front, 1, 1)], unreachable: 1), calendar: .paris)
            == "Friday 16 October · 1 entry · 0 read · 1 feed unreachable")
    }

    @Test func placeholders_say_what_the_journal_is_waiting_for() {
        let full = sampleEdition([(1, .front, 1, 1)])
        let empty = sampleEdition([])
        #expect(JournalLayout.placeholder(phase: .building(done: 0, total: 0), edition: full) == "Building this morning's edition…")
        #expect(JournalLayout.placeholder(phase: .building(done: 34, total: 156), edition: nil)
            == "Building this morning's edition… 34/156")
        #expect(JournalLayout.placeholder(phase: .offline, edition: full) == "Offline: no edition yet")
        #expect(JournalLayout.placeholder(phase: .failed("error: disk full"), edition: nil) == "error: disk full")
        #expect(JournalLayout.placeholder(phase: .idle, edition: empty) == "Nothing new this morning.")
        #expect(JournalLayout.placeholder(phase: .idle, edition: nil) == "No edition yet")
        #expect(JournalLayout.placeholder(phase: .idle, edition: full) == nil)
        #expect(JournalLayout.placeholder(phase: .due, edition: full) == nil)
        #expect(JournalLayout.endLine == "That's the edition.")
    }

    @Test func overflow_rows_name_the_feed() {
        #expect(JournalLayout.overflowLabel(overflowItem(.wire, feed: 4, 12, title: "Actualitté")) == "+12 more in Actualitté")
        #expect(JournalLayout.overflowLabel(overflowItem(.week, feed: 4, 1, title: "Fabula")) == "+1 more in Fabula")
    }

    @Test func the_toolbar_progress_counts_feeds_during_an_edition_build() {
        let label = RunCoordinator.editionProgressLabel
        #expect(JournalLayout.progressText(RunProgress(done: 34, total: 156, label: label))
            == "Building this morning's edition… 34/156")
        #expect(JournalLayout.progressText(RunProgress(done: 0, total: 0, label: label)) == label)
        #expect(JournalLayout.progressText(RunProgress(done: 3, total: 10, label: "Fetching feeds")) == "Fetching feeds")
    }

    /// The pill is 260 pt wide: when the full text does not fit, the build shows the short one.
    @Test func a_narrow_toolbar_shortens_the_edition_progress() {
        let label = RunCoordinator.editionProgressLabel
        #expect(JournalLayout.compactProgressText(RunProgress(done: 152, total: 156, label: label)) == "Edition… 152/156")
        #expect(JournalLayout.compactProgressText(RunProgress(done: 0, total: 0, label: label)) == "Edition…")
        #expect(JournalLayout.compactProgressText(RunProgress(done: 3, total: 10, label: "Fetching feeds")) == "Fetching feeds")
    }
}
