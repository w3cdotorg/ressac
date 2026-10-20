import Foundation
import Testing

@testable import Ressac

/// An `AppState` over a database copy and a stub `ressac` (by default the stub only appends its arguments to `calls`).
@MainActor
func makeState(
    busyTimeout: TimeInterval = 5, stub body: String = #"echo "$*" >> "$(dirname "$0")/calls""#
) throws -> (AppState, TestDatabase, Stub) {
    let test = try TestDatabase()
    let stub = try Stub(body)
    let runs = RunCoordinator(cli: RessacCLI(executable: stub.executable, database: test.url))
    let state = AppState(database: try test.open(busyTimeout: busyTimeout), runs: runs)
    state.autoReadDelay = .milliseconds(100)
    state.now = { Timestamp.date(from: "2026-10-10T09:00:00.000Z")! }
    return (state, test, stub)
}

func calls(_ stub: Stub) throws -> [String] {
    try String(contentsOf: stub.file("calls"), encoding: .utf8).split(separator: "\n").map(String.init)
}

@MainActor @Suite struct AppStateTests {
    /// Review Focus #1: no url, no title, only a summary — listed, readable, searchable.
    @Test func untitled_entry_shows_placeholder_title() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1, title: nil, url: nil, summary: "<p>Lonely albatross</p>")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.map(\.entryId) == [id] })
        #expect(state.entries.first?.displayTitle == "Untitled")
        state.selectedEntryId = id
        #expect(state.selectedEntry?.summaryHtml == "<p>Lonely albatross</p>")
        #expect(!state.canOpenInBrowser && !state.canRequestFullText)
        #expect(state.requestFullText() == nil)
        state.searchText = "albatross"
        #expect(state.entries.map(\.entryId) == [id])
        #expect(state.entries.first?.displayTitle == "Untitled")
    }

    @Test func sidebar_tree_nests_folders_and_feeds_in_opml_order() throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 2)
        let tree = SidebarNode.tree(from: state.sidebar)
        #expect(tree.map(\.title) == ["Blogs", "Biblis", "Infos", "Jon"])
        let blogs = try #require(tree.first)
        #expect(blogs.children?.map(\.title) == ["xkcd.com", "potes", "Girl on the Couch"])
        #expect(blogs.children?[1].children?.map(\.source) == [.feed(2), .feed(3), .feed(4)])
        #expect(blogs.children?[0].children == nil)
        #expect(tree[3].children?.first?.title == "YouTube")
    }

    @Test func folder_unread_counts_sum_their_subtree() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 2)
        try test.entry(feed: 5)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.sidebar.unread == 2 })
        let blogs = try #require(state.sidebarTree.first)
        #expect(blogs.unread == 2)
        #expect(blogs.children?[1].unread == 1)
    }

    @Test func selection_and_filter_drive_the_list() async throws {
        let (state, test, _) = try makeState()
        let a = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        let b = try test.entry(feed: 8, published: "2026-10-02T08:00:00.000Z", read: true)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.map(\.entryId) == [a] })
        state.selection = .feed(8)
        #expect(state.entries.map(\.entryId) == [b])
        state.unreadOnly = true
        #expect(state.entries.isEmpty)
        #expect(state.emptyListMessage == "No unread entries")
        state.selection = .garden(.needsTriage)
        #expect(state.entries.isEmpty && !state.canMarkAllRead)
    }

    @Test func next_and_previous_follow_the_list_and_mark_read_when_moving_on() async throws {
        let (state, test, _) = try makeState()
        let older = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        let newer = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.selectNext()
        #expect(state.selectedEntryId == newer)
        state.selectNext()
        #expect(state.selectedEntryId == older)
        #expect(await eventually { try! test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [newer]) != nil })
        // Still listed in the Unread list, now read.
        #expect(state.entries.map(\.entryId) == [newer, older])
        #expect(state.entries.first?.isRead == true)
        state.selectNext()
        #expect(state.selectedEntryId == older)
        state.selectPrevious()
        #expect(state.selectedEntryId == newer)
    }

    @Test func entry_is_marked_read_after_the_delay_unless_left_before() async throws {
        let (state, test, _) = try makeState()
        let a = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        let b = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.autoReadDelay = .milliseconds(300)
        state.selectedEntryId = a
        state.selectedEntryId = b  // left before the delay: a stays unread
        #expect(await eventually { state.selectedEntry?.isRead == true })
        try await Task.sleep(for: .milliseconds(400))
        #expect(try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [a]) == nil)
        #expect(try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [b]) == "2026-10-10T09:00:00.000Z")
    }

    @Test func toggle_read_and_star_write_through() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = id
        await state.toggleRead()?.value
        #expect(state.selectedEntry?.isRead == true)
        #expect(state.entries.map(\.entryId) == [id])  // kept in the Unread list
        await state.toggleRead()?.value
        #expect(try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [id]) == nil)
        await state.toggleStar()?.value
        #expect(try test.one(String.self, "SELECT starred_at FROM entry WHERE id = ?", [id]) == "2026-10-10T09:00:00.000Z")
    }

    /// Review Focus #2 (app side): a write that times out is reverted and explained.
    @Test func failed_read_write_reverts_and_shows_a_banner() async throws {
        let (state, test, _) = try makeState(busyTimeout: 0.2)
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = id
        let (locked, lockedSignal) = AsyncStream<Void>.makeStream()
        let core = Task.detached { try test.holdWriteLock(for: 1) { lockedSignal.yield() } }
        for await _ in locked { break }
        await state.toggleRead()?.value
        #expect(state.selectedEntry?.isRead == false)
        #expect(state.entries.first?.isRead == false)
        #expect(state.runs.banner?.message == "The database is busy (is `ressac` running in a terminal?)")
        try await core.value
    }

    @Test func mark_all_read_covers_the_current_source() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 1)
        try test.entry(feed: 2)
        try test.entry(feed: 8)
        state.selection = .folder(1)
        #expect(state.entries.count == 2 && state.canMarkAllRead)
        await state.markAllRead()?.value
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") == 1)
        #expect(state.entries.allSatisfy(\.isRead) && !state.canMarkAllRead)
    }

    @Test func refresh_targets_the_current_source() async throws {
        let (state, _, stub) = try makeState()
        state.selection = .feed(3)
        _ = await state.refreshCurrent().value
        state.selection = .folder(2)
        _ = await state.refreshCurrent().value
        state.selection = .starred
        _ = await state.refreshCurrent().value
        _ = await state.refreshAll().value
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == [
            "fetch --feed 3", "fetch --folder 2", "fetch --force", "fetch --force",
        ])
    }

    @Test func search_replaces_the_list_until_cleared() async throws {
        let (state, test, _) = try makeState()
        let match = try test.entry(feed: 1, title: "Tidal bore", body: "ressac")
        try test.entry(feed: 1, title: "Other", body: "calm")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.searchText = "tidal"
        #expect(state.entries.map(\.entryId) == [match])
        #expect(!state.canMarkAllRead)
        state.searchText = "zzz"
        #expect(state.entries.isEmpty && state.emptyListMessage == "No results")
        state.searchText = ""
        #expect(state.entries.count == 2)
    }

    @Test func letter_shortcuts_are_off_while_typing_a_search() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.selectedEntryId = id
        #expect(state.canNavigateEntries && state.canToggleEntry)
        state.isSearchFocused = true
        #expect(!state.letterShortcutsEnabled && !state.canNavigateEntries && !state.canToggleEntry)
        state.selectNext()
        #expect(state.selectedEntryId == id)
    }

    @Test func space_scrolls_the_reader_before_moving_on() async throws {
        let (state, test, _) = try makeState()
        let first = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        let second = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.selectedEntryId = first
        var pages = 2
        state.readerPageDown = { _ in
            pages -= 1
            return pages >= 0
        }
        await state.advance()?.value
        await state.advance()?.value
        #expect(state.selectedEntryId == first)
        await state.advance()?.value
        #expect(state.selectedEntryId == second)
    }

    @Test func full_text_failure_is_reported_inline() async throws {
        let (state, test, _) = try makeState(stub: """
            echo '{"event":"extracted","entry_id":'"$5"',"ok":false,"error":"The page is behind a login."}'
            """)
        let id = try test.entry(feed: 1, url: "https://example.org/paywalled")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.selectedEntryId = id
        let run = try #require(state.requestFullText())
        #expect(state.isExtracting && state.readerShowsFullText)
        await run.value
        #expect(!state.isExtracting)
        #expect(state.fullTextNotice == "Full article unavailable: The page is behind a login.")
        #expect(state.runs.banner == nil)
    }

    @Test func garden_commands_run_the_cli_and_track_decided_rows() async throws {
        let (state, _, stub) = try makeState(stub: """
            echo "$*" >> "$(dirname "$0")/calls"
            [ "$5" = decide ] && echo '{"event":"decided","feed_id":'"$6"',"decision":"'"$7"'"}'
            [ "$5" = undo ] && echo '{"event":"undone","feed_id":4}'
            exit 0
            """)
        #expect(state.gardenDecide(.keep) == nil)  // not in the Garden
        state.showInGarden(feedId: 4)
        #expect(state.selection == .garden(.all) && state.selectedFeedId == 4)
        _ = await state.gardenDecide(.bury)?.value
        #expect(state.decidedThisVisit == [4])
        _ = await state.gardenUndo().value
        #expect(state.decidedThisVisit.isEmpty)
        _ = await state.gardenApplySafe().value
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == [
            "garden decide 4 bury", "garden undo", "garden apply-safe",
        ])
    }
}

/// `AppState.init` wires `runs.onDatabaseChange` to `notifyExternalChanges()`. Rows written by
/// another connection (the `ressac` process, here `TestDatabase.raw`) reach the lists after a run
/// with no other nudge: nothing else notifies in a unit test (`startSystemObservers` is not called).
@MainActor @Suite struct DatabaseChangeWiringTests {
    @Test func a_finished_run_reloads_the_lists() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1, title: "Written by ressac")
        #expect(state.entries.isEmpty)
        #expect(await state.refreshAll().value == .succeeded)
        #expect(await eventually { state.entries.map(\.entryId) == [id] })
    }
}

@MainActor @Suite struct LauncherTests {
    /// Review Focus #4 (app side): a v1 database goes through `ressac migrate`, then opens.
    @Test func v1_database_is_migrated_before_opening() async throws {
        let test = try TestDatabase()
        let v1 = test.dir.appending(path: "v1.sqlite")
        try TestDatabase.makeV1(at: v1)
        // The stub "migrates" by copying the v3 fixture over the --db file.
        let stub = try Stub("""
            [ "$4" = migrate ] || exit 2
            cp "\(TestDatabase.fixture.path)" "$2"
            echo '{"event":"finished"}'
            """)
        let launcher = Launcher()
        await launcher.launch(database: v1, executable: stub.executable)
        #expect(launcher.state != nil)
        #expect(try AppDatabase.check(at: v1) == .ok)
    }

    @Test func failed_migration_is_reported() async throws {
        let test = try TestDatabase()
        let v1 = test.dir.appending(path: "v1.sqlite")
        try TestDatabase.makeV1(at: v1)
        let stub = try Stub("echo 'error: disk full' >&2; exit 1")
        let launcher = Launcher()
        await launcher.launch(database: v1, executable: stub.executable)
        guard case let .failed(message) = launcher.phase else {
            Issue.record("expected a failure, got \(launcher.phase)")
            return
        }
        #expect(message == "error: disk full")
    }

    @Test func newer_database_is_refused() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 6")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub("exit 0").executable)
        guard case .tooNew(6) = launcher.phase else {
            Issue.record("expected tooNew(6), got \(launcher.phase)")
            return
        }
    }

    @Test func missing_database_is_created_by_migrate() async throws {
        let test = try TestDatabase()
        let fresh = test.dir.appending(path: "new/ressac.sqlite")
        let stub = try Stub("""
            [ "$4" = migrate ] || exit 2
            mkdir -p "$(dirname "$2")" && cp "\(TestDatabase.fixture.path)" "$2"
            echo '{"event":"finished"}'
            """)
        let launcher = Launcher()
        await launcher.launch(database: fresh, executable: stub.executable)
        #expect(launcher.state != nil)
    }

    /// Spec §8: a missing helper is a fatal alert at launch, not only a message in the window.
    @Test func missing_helper_is_fatal() async throws {
        let test = try TestDatabase()
        let launcher = Launcher()
        var alerts: [String] = []
        launcher.presentFatalAlert = { alerts.append($0) }
        await launcher.launch(database: test.url, executable: nil)
        guard case let .failed(message) = launcher.phase else {
            Issue.record("expected a failure, got \(launcher.phase)")
            return
        }
        #expect(message.hasPrefix("The ressac helper is missing"))
        #expect(alerts == [message])
    }

    /// Two windows launch at once (each `.task` calls `launch`): one `migrate`, one `AppState`.
    /// A launch after that, from a window opened later, keeps the `AppState` already shown.
    @Test func a_second_launch_is_ignored() async throws {
        let test = try TestDatabase()
        let fresh = test.dir.appending(path: "new/ressac.sqlite")
        let stub = try Stub("""
            [ "$4" = migrate ] || exit 2
            echo migrate >> "$(dirname "$0")/calls"
            sleep 0.3
            mkdir -p "$(dirname "$2")" && cp "\(TestDatabase.fixture.path)" "$2"
            echo '{"event":"finished"}'
            """)
        let launcher = Launcher()
        async let first: Void = launcher.launch(database: fresh, executable: stub.executable)
        async let second: Void = launcher.launch(database: fresh, executable: stub.executable)
        _ = await (first, second)
        #expect(try calls(stub) == ["migrate"])
        let state = try #require(launcher.state)
        await launcher.launch(database: fresh, executable: stub.executable)
        #expect(launcher.state === state)
        #expect(try calls(stub) == ["migrate"])
    }
}

/// Long lists load one page at a time: a folder of thousands of entries in "All" is never read
/// whole, and the counts stay those of the whole list.
@MainActor @Suite struct ListPageTests {
    /// `count` entries of `feed`, one a day from 1 October; their ids, newest first.
    private func days(_ test: TestDatabase, _ count: Int, feed: Int64 = 1, read: (Int) -> Bool = { _ in false }) throws -> [Int64] {
        try (1...count).map { day in
            try test.entry(feed: feed, published: String(format: "2026-10-%02dT08:00:00.000Z", day), read: read(day))
        }.reversed()
    }

    @Test func a_long_list_loads_one_page_and_counts_the_whole_list() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 5)
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(2)))
        #expect((state.entryTotal, state.listUnread, state.hasMoreEntries) == (5, 5, true))
        state.loadMoreEntries()
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(4)))
        state.loadMoreEntries()
        #expect(state.entries.map(\.entryId) == ids)
        #expect(!state.hasMoreEntries)
    }

    /// What the list views report as rows appear: one page per turn, only near the end.
    @Test func rows_appearing_near_the_end_load_one_more_page() async throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 26)
        let short = try days(test, 6, feed: 2)
        state.listPageSize = 24
        state.selection = .feed(1)
        state.entryRowAppeared(ids[3])  // 21 rows from the end of the 24 loaded: not near it
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.entries.count == 24)
        state.entryRowAppeared(ids[4])  // 20 rows from the end
        #expect(await eventually { state.entries.count == 26 })
        #expect(!state.hasMoreEntries)
        // Rows appearing together load one page, not one each.
        state.listPageSize = 2
        state.selection = .feed(2)
        state.entryRowAppeared(short[0])
        state.entryRowAppeared(short[1])
        #expect(await eventually { state.entries.count == 4 })
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.entries.count == 4)
    }

    /// A page scheduled for one list never lands on another: the source changes before the turn.
    @Test func a_scheduled_page_is_dropped_when_the_list_changed() async throws {
        let (state, test, _) = try makeState()
        let first = try days(test, 30)
        _ = try days(test, 30, feed: 2)
        state.listPageSize = 24
        state.selection = .feed(1)
        state.entryRowAppeared(first[4])  // schedules the next page of feed 1 for the next turn
        state.selection = .feed(2)  // before that turn
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.entries.count == 24 && state.hasMoreEntries)
    }

    @Test func next_entry_past_the_loaded_rows_loads_the_next_page() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 3)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.selectNext()
        state.selectNext()
        #expect(state.selectedEntryId == ids[1] && state.entries.count == 2)
        state.selectNext()
        #expect(state.selectedEntryId == ids[2])
        #expect(state.entries.count == 3 && !state.hasMoreEntries)
    }

    @Test func space_looks_for_unread_entries_in_the_rows_not_loaded() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 5) { $0 != 1 }  // only the oldest is unread
        let allRead = try days(test, 3, feed: 2) { _ in true }
        state.listPageSize = 2
        state.autoReadDelay = .seconds(60)
        state.selection = .feed(1)
        state.selectedEntryId = ids[0]
        #expect((state.listUnread, state.entries.count) == (1, 2))
        state.selectNextUnread()
        #expect(state.selectedEntryId == ids[4])
        #expect(state.entries.count == 5)
        // Nothing unread beyond the loaded rows: no page is loaded for nothing.
        state.selection = .feed(2)
        state.selectedEntryId = allRead[0]
        state.selectNextUnread()
        #expect(state.selectedEntryId == allRead[0])
        #expect(state.entries.count == 2)
    }

    @Test func new_entries_never_push_the_loaded_rows_out() async throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 3)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = ids[1]  // the last loaded row
        let newer = try (20...23).map { try test.entry(feed: 1, published: "2026-10-\($0)T08:00:00.000Z") }
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entryTotal == 7 })
        #expect(state.entries.map(\.entryId) == Array(newer.reversed()) + Array(ids.prefix(2)))
        #expect(state.selectedEntryId == ids[1] && state.hasMoreEntries)
    }

    /// The first run: the list was empty, so whole, when the first refresh filled it.
    @Test func a_short_list_made_long_by_a_refresh_is_paged_again() async throws {
        let (state, test, _) = try makeState()
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.isEmpty && !state.hasMoreEntries)
        let ids = try days(test, 5)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        #expect(state.entries.map(\.entryId) == Array(ids.prefix(2)))
        #expect((state.entryTotal, state.hasMoreEntries) == (5, true))
    }

    @Test func the_selection_stays_loaded_when_the_list_restarts() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 4)
        state.listPageSize = 2
        state.selection = .feed(1)
        state.autoReadDelay = .seconds(60)
        state.loadMoreEntries()
        state.selectedEntryId = ids[3]  // the oldest, on the second page
        state.unreadOnly = true
        #expect(state.entries.map(\.entryId) == ids)
        #expect(!state.hasMoreEntries)
    }

    @Test func mark_all_read_counts_the_rows_not_loaded() async throws {
        let (state, test, _) = try makeState()
        _ = try days(test, 3) { $0 != 1 }  // the oldest, not loaded, is the only unread one
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.allSatisfy(\.isRead) && state.canMarkAllRead)
        await state.markAllRead()?.value
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") == 0)
        #expect(await eventually { !state.canMarkAllRead })
    }

    /// Review Focus #1: an Unread-only list longer than a page while entries are read.
    @Test func reading_through_a_long_unread_list_keeps_the_selection_and_the_counts() async throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 6)
        state.listPageSize = 2
        state.autoReadDelay = .seconds(60)
        state.unreadOnly = true
        state.selection = .feed(1)
        func unreadInDatabase() throws -> Int { try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") ?? -1 }
        state.selectedEntryId = ids[1]  // the last loaded row
        await state.toggleRead()?.value  // read: retained, so it stays listed
        #expect(state.selectedEntryId == ids[1] && state.entries.map(\.entryId) == [ids[0], ids[1]])
        #expect(await eventually { state.listUnread == 5 })
        #expect((state.listUnread, state.entryTotal, state.hasMoreEntries) == (try unreadInDatabase(), 6, true))
        // j on the last loaded row loads the next page and moves on; nothing loaded leaves.
        state.selectNext()
        #expect(state.selectedEntryId == ids[2])
        #expect(Array(state.entries.map(\.entryId).prefix(2)) == [ids[0], ids[1]])
        #expect(state.entries.count == 4 && state.hasMoreEntries)
        // Space past the loaded rows: read ids[2], then ids[3], then reach ids[4] beyond the page.
        await state.toggleRead()?.value
        await state.advance()?.value
        #expect(state.selectedEntryId == ids[3])
        await state.toggleRead()?.value
        await state.advance()?.value
        #expect(state.selectedEntryId == ids[4])
        #expect(Array(state.entries.map(\.entryId).prefix(4)) == Array(ids.prefix(4)))
        #expect(await eventually { Optional(state.listUnread) == (try? unreadInDatabase()) })
        #expect(state.entryTotal == (try unreadInDatabase()) + 3)  // plus the three read ones kept
        #expect(state.hasMoreEntries == (state.entries.count < state.entryTotal))
    }

    /// Space reaches an unread entry several pages down with one floor change, not page by page.
    @Test func space_reaches_a_deep_unread_entry_with_one_floor_change() throws {
        let (state, test, _) = try makeState()
        let ids = try days(test, 9) { day in day != 5 && day != 1 }  // unread: ids[4] and the oldest
        state.listPageSize = 2
        state.autoReadDelay = .seconds(60)
        state.selection = .feed(1)
        state.selectedEntryId = ids[0]
        state.selectNextUnread()
        #expect(state.selectedEntryId == ids[4])
        // One load down to that row (5 rows); page by page would have stopped at 6.
        #expect(state.entries.count == 5 && state.hasMoreEntries)
    }
}
