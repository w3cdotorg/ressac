# Phase 9: `AppState`, sidebar, entry list, menus and shortcuts, search

Read the plan index first. Global Constraints apply (Keyboard). Spec §7 (Window and navigation, Search, Marking read, Keyboard; the Reader, Garden and Settings sections only through the hooks named below), §8 (Schema version, Binary missing, Database busy, Read-state write failure) and §9 (keyboard-command routing in `AppState`).

**Files (5):**
- Create: `apps/macos/Ressac/App/AppState.swift`
- Create: `apps/macos/Ressac/App/Commands.swift`
- Create: `apps/macos/Ressac/Reading/MainView.swift`
- Modify: `apps/macos/Ressac/App/RessacApp.swift`
- Create: `apps/macos/RessacTests/AppStateTests.swift`

**Test command:** as in phase 6, narrowed with `-only-testing:RessacTests/<Suite>`:

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/AppStateTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**What later phases replace or plug into** (keep these names):
- **Everywhere:** `LaunchView` injects the state into the window with `.environment(state)`, so any view can read `@Environment(AppState.self)`.
- **Phase 10 (reader):** the detail column of a reading source shows `ReaderPlaceholder(entry: state.selectedEntry)`, a plain-text stand-in defined at the end of `Reading/MainView.swift`. Phase 10 swaps that one line for its reader and deletes `ReaderPlaceholder`. The reader must render `state.selectedEntry`, the complete row with `contentHtml`; list rows carry no content (phase 7). `AppState` also offers `selectedFulltext`, `readerShowsFullText`, `isExtracting`, `fullTextNotice` and `requestFullText()`. The reader sets `state.readerPageDown` to a closure that scrolls one page and returns `false` at the bottom: Space then moves to the next unread entry. The text-size menu items write `@AppStorage("reader.size")` (points, 12...28, default 17), which is phase 10's `ReaderStyle.Keys.size`.
- **Phase 11 (settings, first run):** a missing database is created by `ressac migrate` at launch, like an old one is migrated, so there is always an `AppState`; the first-run screen is the "no feeds" case (`state.sidebar.feeds.isEmpty`). ⌘O and ⇧⌘E set `state.pendingPanel` to `.importOPML` / `.exportOPML`; phase 11 either presents the panels from that property or replaces `FileMenuItems`' two import/export buttons with its own commands, but never keeps both (two items with one shortcut). It also owns the `Settings` scene (⌘,). The database URL is `AppDatabase.defaultURL()` (`RESSAC_DB`, else the core's default); phase 11 inserts the Settings location between the two.
- **Phase 12 (garden):** for garden sources the content column shows `Text("Garden")` and the detail column `Text("No feed selected")`, both in `MainView`; phase 12 swaps them for its table and inspector. `AppState` already has `selectedFeedId` (the Garden's selection, cleared when the source changes), `decidedThisVisit`, `gardenDecide(_:)`, `gardenUndo()`, `gardenApplySafe()`, `canGardenDecide` and `showInGarden(feedId:)`. In the Garden, `canNavigateEntries` is false, so the menus bind no `j`/`k` there and a view-level `onKeyPress` receives `k` (Keep). A Garden *menu* item for `k` must likewise bind its key only in the Garden (see the notes).

### Task 9.1: `AppState`, the window's state and commands

**Interfaces:**
- Consumes: `AppDatabase` (`reader`, `notifyExternalChanges()`, `isBusy(_:)`), `Writes`, `Queries` (`sidebar`, `entries`, `search`, `entry(id:)`, `fulltext(entryId:)`), `EntryRow`, `FeedItem`, `FolderItem`, `FulltextRow`, `SidebarSnapshot`, `Source`, `RunCoordinator` (`start`, `banner`, `isLongRunActive`, `onDatabaseChange`), `RunRequest`, `RunOutcome`, `GardenDecision`; GRDB `ValueObservation.start(in:scheduling: .immediate, onError:onChange:)` (its `@MainActor` overload) and `AnyDatabaseCancellable`.
- Produces (`App/AppState.swift`):
  - `struct SidebarNode: Identifiable, Equatable, Sendable` (`kind: .folder(FolderItem) | .feed(FeedItem)`, `unread`, `children: [SidebarNode]?`, `id`, `source`, `title`, `static func tree(from: SidebarSnapshot) -> [SidebarNode]`);
  - `enum PendingPanel { case importOPML, exportOPML }`;
  - `@MainActor @Observable final class AppState`, `init(database: AppDatabase, runs: RunCoordinator)`:
    - state from the master: `selection: Source` (default `.unread`), `selectedEntryId: Int64?`, `unreadOnly: Bool`, `searchText: String`, `readerShowsFullText: Bool`; dependencies `database`, `writes`, `runs`;
    - more state: `isSearchFocused`, `selectedFeedId: Int64?`, `pendingPanel: PendingPanel?`, and read-only `sidebar`, `entries` (the list, or the search results while `searchText` is not blank), `selectedEntry`, `selectedFulltext`, `isExtracting`, `fullTextNotice`, `decidedThisVisit`, `sidebarTree`, `emptyListMessage`;
    - availability, read by the menus: `letterShortcutsEnabled`, `canNavigateEntries`, `canToggleEntry`, `canMarkAllRead`, `canRefresh`, `canOpenInBrowser`, `canRequestFullText`, `canGardenDecide`;
    - the master's methods `refreshCurrent()`, `refreshAll()`, `toggleRead()`, `toggleStar()`, `markAllRead()`, `selectNext()`, `selectPrevious()`, `requestFullText()`, `scheduleAutoRead(entryId:)`, `gardenDecide(_:)`, `gardenUndo()`, `gardenApplySafe()`. Those that start work return the work's `Task` with `@discardableResult`, so `state.toggleRead()` still reads as in the master and tests can await it;
    - more methods: `selectNextUnread()`, `advance() async` (Space), `markAllRead(in:)`, `openInBrowser()`, `refresh(feedId:)`, `setPreferFullText(feedId:on:)`, `openSite(feedId:)`, `showInGarden(feedId:)`, `static func merge(fresh:previous:retained:) -> [EntryRow]`;
    - injection points: `autoReadDelay: Duration` (1 s), `now`, `openURL`, `readerPageDown`.

**Rules implemented here:**
- An entry is marked read after it has been selected for `autoReadDelay`, and at once when `j`, `selectNextUnread()` or Space moves past it. `m` on a read entry cancels the pending auto-read.
- Entries read during a visit stay in an unread-only list, dimmed, until the source changes (`merge`), so the selection never disappears under the reader.
- Writes are optimistic. On failure the change is reverted and the banner reads "The database is busy (is `ressac` running in a terminal?)" for SQLITE_BUSY (spec §8).
- Single-letter commands are off while the search field has focus (`letterShortcutsEnabled`).
- ⌘R refreshes the selected feed (`fetch --feed`), the selected folder (`fetch --folder`) or, on a smart list or in the Garden, every feed (`fetch --force`).
- `requestFullText()` toggles the full article when one is cached; otherwise it runs `ressac extract` and reports a failure only in `fullTextNotice` ("Full article unavailable: …"), never as a banner.
- `init` wires `runs.onDatabaseChange` to `database.notifyExternalChanges()`, so the lists follow what `ressac` writes.

- [ ] **Step 1: Write the failing tests** `apps/macos/RessacTests/AppStateTests.swift`. `untitled_entry_shows_placeholder_title` pins the Swift part of Review Focus #1; `failed_read_write_reverts_and_shows_a_banner` is the app side of Review Focus #2.

```swift
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
        state.readerPageDown = {
            pages -= 1
            return pages >= 0
        }
        await state.advance()
        await state.advance()
        #expect(state.selectedEntryId == first)
        await state.advance()
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
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/AppStateTests`. Expected: `error: cannot find 'AppState' in scope` (and `SidebarNode`).

- [ ] **Step 3: Implement** `apps/macos/Ressac/App/AppState.swift`:

```swift
import AppKit
import GRDB
import Observation

/// A node of the sidebar's folder tree: folders and feeds, ordered by OPML position.
struct SidebarNode: Identifiable, Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        case folder(FolderItem)
        case feed(FeedItem)
    }

    let kind: Kind
    /// Sum of the subtree's unread entries for a folder.
    let unread: Int
    /// Nil for feeds, so `OutlineGroup` shows no disclosure triangle.
    let children: [SidebarNode]?

    var id: String {
        switch kind {
        case let .folder(folder): "folder-\(folder.folderId)"
        case let .feed(feed): "feed-\(feed.feedId)"
        }
    }

    var source: Source {
        switch kind {
        case let .folder(folder): .folder(folder.folderId)
        case let .feed(feed): .feed(feed.feedId)
        }
    }

    var title: String {
        switch kind {
        case let .folder(folder): folder.name
        case let .feed(feed): feed.title
        }
    }

    static func tree(from snapshot: SidebarSnapshot) -> [SidebarNode] {
        let folders = Dictionary(grouping: snapshot.folders, by: \.parentId)
        let feeds = Dictionary(grouping: snapshot.feeds, by: \.folderId)
        func children(of parent: Int64?) -> [SidebarNode] {
            let subfolders = (folders[parent] ?? []).map { folder in
                let nested = children(of: folder.folderId)
                return (position: folder.position, rank: 0, node: SidebarNode(
                    kind: .folder(folder), unread: nested.reduce(0) { $0 + $1.unread }, children: nested))
            }
            let leaves = (feeds[parent] ?? []).map { feed in
                (position: feed.position, rank: 1, node: SidebarNode(kind: .feed(feed), unread: feed.unread, children: nil))
            }
            return (subfolders + leaves)
                .sorted { ($0.position, $0.rank, $0.node.id) < ($1.position, $1.rank, $1.node.id) }
                .map(\.node)
        }
        return children(of: nil)
    }
}

/// Import and export panels, presented by the window (phase 11).
enum PendingPanel: Equatable, Sendable {
    case importOPML, exportOPML
}

/// The window's state and every command the menus and keys trigger (spec §7). Views
/// stay thin: they read this object and call its methods.
@MainActor @Observable final class AppState {
    // MARK: Navigation

    var selection: Source = .unread {
        didSet { if selection != oldValue { sourceDidChange() } }
    }
    var selectedEntryId: Int64? {
        didSet { if selectedEntryId != oldValue { entryDidChange() } }
    }
    var unreadOnly = false {
        didSet { if unreadOnly != oldValue { listDidChange() } }
    }
    var searchText = "" {
        didSet { if searchText != oldValue { listDidChange() } }
    }
    /// Mirrors the search field's focus: single-letter shortcuts are off while typing.
    var isSearchFocused = false
    var readerShowsFullText = false
    /// The feed selected in the Garden table (phase 12); garden commands act on it.
    var selectedFeedId: Int64?
    var pendingPanel: PendingPanel?

    // MARK: Data

    private(set) var sidebar = SidebarSnapshot()
    /// The entry list, or the search results while `searchText` is not empty.
    private(set) var entries: [EntryRow] = []
    /// The selected entry with its full content, for the reader.
    private(set) var selectedEntry: EntryRow?
    private(set) var selectedFulltext: FulltextRow?
    private(set) var isExtracting = false
    /// "Full article unavailable: …" for the selected entry (shown inline, spec §8).
    private(set) var fullTextNotice: String?
    /// Garden rows decided since the scope was entered, dimmed by the table.
    private(set) var decidedThisVisit: Set<Int64> = []

    let database: AppDatabase
    let writes: Writes
    let runs: RunCoordinator

    @ObservationIgnored var autoReadDelay: Duration = .seconds(1)
    @ObservationIgnored var now: @MainActor () -> Date = { Date() }
    @ObservationIgnored var openURL: @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) }
    /// Set by the reader (phase 10): scrolls one page and returns false at the bottom.
    @ObservationIgnored var readerPageDown: (@MainActor () async -> Bool)?

    @ObservationIgnored private var sidebarObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var listObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var entryObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var fulltextObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var autoReadTask: Task<Void, Never>?
    /// Entries read during this visit of the source: they stay listed in unread-only
    /// lists until the source changes, so the selection never vanishes.
    @ObservationIgnored private var retained: Set<Int64> = []

    init(database: AppDatabase, runs: RunCoordinator) {
        self.database = database
        self.writes = Writes(db: database)
        self.runs = runs
        runs.onDatabaseChange = { [database] in
            Task { try? await database.notifyExternalChanges() }
        }
        sidebarObservation = Queries.sidebar().start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] in self?.sidebar = $0 })
        listDidChange()
    }

    var sidebarTree: [SidebarNode] { SidebarNode.tree(from: sidebar) }

    var emptyListMessage: String {
        if !searchText.isEmpty { return "No results" }
        if selection == .unread || unreadOnly { return "No unread entries" }
        return "No entries"
    }

    // MARK: Command availability (menus disable what does nothing, spec §7)

    var letterShortcutsEnabled: Bool { !isSearchFocused }
    var canNavigateEntries: Bool { letterShortcutsEnabled && !selection.isGarden && !entries.isEmpty }
    var canToggleEntry: Bool { letterShortcutsEnabled && !selection.isGarden && selectedEntry != nil }
    var canMarkAllRead: Bool { !selection.isGarden && searchText.isEmpty && entries.contains { !$0.isRead } }
    var canRefresh: Bool { !runs.isLongRunActive }
    var canOpenInBrowser: Bool { selectedEntry?.link != nil }
    var canRequestFullText: Bool { selectedEntry?.link != nil && !isExtracting }
    var canGardenDecide: Bool { selection.isGarden && selectedFeedId != nil }

    // MARK: Reading commands

    func selectNext() {
        guard canNavigateEntries else { return }
        let index = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
        let next = index.map { $0 + 1 } ?? 0
        guard next < entries.count else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = entries[next].entryId
    }

    func selectPrevious() {
        guard canNavigateEntries else { return }
        let index = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
        let previous = index.map { $0 - 1 } ?? 0
        guard previous >= 0, previous < entries.count else { return }
        selectedEntryId = entries[previous].entryId
    }

    /// The next unread entry after the selection (spec §7, Space at the bottom).
    func selectNextUnread() {
        guard canNavigateEntries else { return }
        let start = selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }.map { $0 + 1 } ?? 0
        guard let next = entries[start...].first(where: { !$0.isRead && $0.entryId != selectedEntryId }) else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = next.entryId
    }

    /// Space: scroll the reader; at the bottom, next unread.
    func advance() async {
        if let readerPageDown, await readerPageDown() { return }
        selectNextUnread()
    }

    @discardableResult
    func toggleRead() -> Task<Void, Never>? {
        guard canToggleEntry, let entry = selectedEntry else { return nil }
        if entry.isRead {
            autoReadTask?.cancel()
        }
        return Task { await setRead([entry.entryId], read: !entry.isRead) }
    }

    @discardableResult
    func toggleStar() -> Task<Void, Never>? {
        guard canToggleEntry, let entry = selectedEntry else { return nil }
        let starred = !entry.isStarred
        update([entry.entryId]) { $0.isStarred = starred }
        return Task {
            do {
                try await writes.setStarred(entry.entryId, starred: starred, at: now())
            } catch {
                update([entry.entryId]) { $0.isStarred = !starred }
                reportWrite(error)
            }
        }
    }

    @discardableResult
    func markAllRead() -> Task<Void, Never>? {
        guard canMarkAllRead else { return nil }
        return markAllRead(in: selection)
    }

    /// Feed context menu and ⇧⌘A.
    @discardableResult
    func markAllRead(in source: Source) -> Task<Void, Never> {
        let ids = source == selection ? entries.filter { !$0.isRead }.map(\.entryId) : []
        retained.formUnion(ids)
        update(ids) { $0.isRead = true }
        return Task {
            do {
                _ = try await writes.markAllRead(in: source, at: now())
            } catch {
                update(ids) { $0.isRead = false }
                reportWrite(error)
            }
        }
    }

    func openInBrowser() {
        if let link = selectedEntry?.link { openURL(link) }
    }

    func scheduleAutoRead(entryId: Int64) {
        autoReadTask?.cancel()
        autoReadTask = Task { [weak self, autoReadDelay] in
            try? await Task.sleep(for: autoReadDelay)
            guard !Task.isCancelled, let self, self.selectedEntryId == entryId,
                self.selectedEntry?.isRead == false
            else { return }
            await self.setRead([entryId], read: true)
        }
    }

    /// The reader's Full article toggle (spec §7, Reader).
    @discardableResult
    func requestFullText() -> Task<Void, Never>? {
        guard let entry = selectedEntry, entry.link != nil else { return nil }
        if readerShowsFullText || entry.hasFulltext {
            readerShowsFullText.toggle()
            return nil
        }
        guard !isExtracting else { return nil }
        readerShowsFullText = true
        isExtracting = true
        fullTextNotice = nil
        let entryId = entry.entryId
        let run = runs.start(.extract(entryId: entryId)) { [weak self] event in
            if case let .extracted(id, false, error) = event, id == entryId {
                self?.fullTextNotice = "Full article unavailable: \(error ?? "unknown error")"
            }
        }
        return Task {
            let outcome = await run.value
            isExtracting = false
            if case let .failed(failure) = outcome, fullTextNotice == nil, selectedEntryId == entryId {
                fullTextNotice = "Full article unavailable: \(failure.message)"
            }
        }
    }

    // MARK: Feeds

    /// ⌘R: the selected feed, every feed of the selected folder, or all feeds.
    @discardableResult
    func refreshCurrent() -> Task<RunOutcome, Never> {
        switch selection {
        case let .feed(id): runs.start(.fetch(feeds: [id]))
        case let .folder(id): runs.start(.fetch(folder: id))
        case .unread, .today, .starred, .garden: runs.start(.fetch(feeds: []))
        }
    }

    @discardableResult
    func refreshAll() -> Task<RunOutcome, Never> {
        runs.start(.fetch(feeds: []))
    }

    @discardableResult
    func refresh(feedId: Int64) -> Task<RunOutcome, Never> {
        runs.start(.fetch(feeds: [feedId]))
    }

    @discardableResult
    func setPreferFullText(feedId: Int64, on: Bool) -> Task<Void, Never> {
        Task {
            do {
                try await writes.setPreferFullText(feedId: feedId, on: on)
            } catch {
                reportWrite(error)
            }
        }
    }

    func openSite(feedId: Int64) {
        let site = sidebar.feeds.first { $0.feedId == feedId }?.htmlUrl
        if let site, let url = URL(string: site) { openURL(url) }
    }

    func showInGarden(feedId: Int64) {
        selection = .garden(.all)
        selectedFeedId = feedId
    }

    // MARK: Garden (UI in phase 12)

    @discardableResult
    func gardenDecide(_ decision: GardenDecision) -> Task<RunOutcome, Never>? {
        guard canGardenDecide, let feedId = selectedFeedId else { return nil }
        let run = runs.start(.decide(feedId: feedId, decision))
        return Task {
            let outcome = await run.value
            if outcome == .succeeded { decidedThisVisit.insert(feedId) }
            return outcome
        }
    }

    @discardableResult
    func gardenUndo() -> Task<RunOutcome, Never> {
        runs.start(.undo()) { [weak self] event in
            if case let .undone(feedId?) = event { self?.decidedThisVisit.remove(feedId) }
        }
    }

    @discardableResult
    func gardenApplySafe() -> Task<RunOutcome, Never> {
        runs.start(.applySafe())
    }

    // MARK: Private

    private func sourceDidChange() {
        retained = []
        decidedThisVisit = []
        selectedFeedId = nil
        selectedEntryId = nil
        listDidChange()
    }

    private func listDidChange() {
        let observation = searchText.trimmingCharacters(in: .whitespaces).isEmpty
            ? Queries.entries(source: selection, unreadOnly: unreadOnly, now: now())
            : Queries.search(searchText, in: selection, now: now())
        listObservation = observation.start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] fresh in
                guard let self else { return }
                entries = Self.merge(fresh: fresh, previous: entries, retained: retained)
            })
    }

    private func entryDidChange() {
        autoReadTask?.cancel()
        fullTextNotice = nil
        selectedEntry = nil
        selectedFulltext = nil
        guard let id = selectedEntryId else {
            entryObservation = nil
            fulltextObservation = nil
            readerShowsFullText = false
            return
        }
        entryObservation = Queries.entry(id: id).start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] in self?.selectedEntry = $0 })
        fulltextObservation = Queries.fulltext(entryId: id).start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] in self?.selectedFulltext = $0 })
        readerShowsFullText = selectedEntry?.hasFulltext ?? false
        scheduleAutoRead(entryId: id)
    }

    /// Moving past an entry with `j` or Space marks it read at once (spec §7).
    private func markSelectedReadOnLeaving() {
        guard let entry = selectedEntry, !entry.isRead else { return }
        autoReadTask?.cancel()
        Task { await setRead([entry.entryId], read: true) }
    }

    /// Optimistic: the UI changes first and is reverted if the write fails (spec §8).
    private func setRead(_ ids: [Int64], read: Bool) async {
        if read { retained.formUnion(ids) }
        update(ids) { $0.isRead = read }
        do {
            try await writes.setRead(ids, read: read, at: now())
        } catch {
            update(ids) { $0.isRead = !read }
            reportWrite(error)
        }
    }

    private func update(_ ids: [Int64], _ change: (inout EntryRow) -> Void) {
        let ids = Set(ids)
        for index in entries.indices where ids.contains(entries[index].entryId) {
            change(&entries[index])
        }
        if var entry = selectedEntry, ids.contains(entry.entryId) {
            change(&entry)
            selectedEntry = entry
        }
    }

    private func reportWrite(_ error: any Error) {
        runs.banner = Banner(
            message: AppDatabase.isBusy(error)
                ? "The database is busy (is `ressac` running in a terminal?)"
                : "Could not save the change: \(error.localizedDescription)",
            showLog: false)
    }

    private func report(_ error: any Error) {
        runs.banner = Banner(message: "Could not read the database: \(error.localizedDescription)", showLog: true)
    }

    /// Fresh rows, plus the entries read during this visit that an unread-only query
    /// dropped, newest first.
    static func merge(fresh: [EntryRow], previous: [EntryRow], retained: Set<Int64>) -> [EntryRow] {
        let freshIds = Set(fresh.map(\.entryId))
        let kept = previous.filter { retained.contains($0.entryId) && !freshIds.contains($0.entryId) }
        guard !kept.isEmpty else { return fresh }
        return (fresh + kept.map { row in
            var row = row
            row.isRead = true
            return row
        }).sorted { ($0.publishedAt, $0.entryId) > ($1.publishedAt, $1.entryId) }
    }
}
```

- [ ] **Step 4: Run.** Expected: `✔ Test run with 15 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/AppStateTests.swift
git commit -m "macOS app: AppState with live lists, read marks, search and command routing"
```

### Task 9.2: Launch: helper check, schema check, `ressac migrate`

**Interfaces:**
- Consumes: `AppDatabase.check(at:)`, `AppDatabase.open(at:)`, `RessacCLI`, `RunCoordinator.submit(_:)`, `RunRequest.migrate()`, `AppState.init`.
- Produces (appended to `App/AppState.swift`): `@MainActor @Observable final class Launcher` with `enum Phase { case starting, migrating, tooNew(Int), failed(String), ready(AppState) }`, `private(set) var phase`, `var state: AppState?`, `static var isRunningTests: Bool` and `func launch(database url: URL, executable: URL?) async`.

**Behaviour (spec §8):** no helper → `.failed("The ressac helper is missing from the app bundle. Rebuild the app.")`; schema newer than 2 → `.tooNew`; older, or no file yet → `.migrating`, then `ressac migrate` (phase 5 accepts a missing file and creates it), then a fresh check that must say `.ok`; any failure → `.failed(message)` with the CLI's last stderr line; otherwise `.ready(AppState)`.

- [ ] **Step 1: Write the failing tests.** Append to `AppStateTests.swift`. `v1_database_is_migrated_before_opening` is the app side of Review Focus #4: the stub "migrates" by copying the v2 fixture over the `--db` file.

```swift
@MainActor @Suite struct LauncherTests {
    /// Review Focus #4 (app side): a v1 database goes through `ressac migrate`, then opens.
    @Test func v1_database_is_migrated_before_opening() async throws {
        let test = try TestDatabase()
        let v1 = test.dir.appending(path: "v1.sqlite")
        try TestDatabase.makeV1(at: v1)
        // The stub "migrates" by copying the v2 fixture over the --db file.
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
        try test.execute("UPDATE schema_meta SET version = 3")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub("exit 0").executable)
        guard case .tooNew(3) = launcher.phase else {
            Issue.record("expected tooNew(3), got \(launcher.phase)")
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

    @Test func missing_helper_is_fatal() async throws {
        let test = try TestDatabase()
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: nil)
        guard case let .failed(message) = launcher.phase else {
            Issue.record("expected a failure, got \(launcher.phase)")
            return
        }
        #expect(message.hasPrefix("The ressac helper is missing"))
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/LauncherTests`. Expected: `error: cannot find 'Launcher' in scope`.

- [ ] **Step 3: Implement.** Append to `apps/macos/Ressac/App/AppState.swift`:

```swift
/// Opens the database the way spec §8 asks: helper present, schema checked, `ressac
/// migrate` first when the schema is older or the file does not exist yet.
@MainActor @Observable final class Launcher {
    enum Phase {
        case starting
        /// Creating (no file yet) or upgrading the database with `ressac migrate`.
        case migrating
        case tooNew(Int)
        case failed(String)
        case ready(AppState)
    }

    private(set) var phase = Phase.starting

    var state: AppState? {
        if case let .ready(state) = phase { state } else { nil }
    }

    /// True inside the unit-test host, where the app must not open the real database.
    static var isRunningTests: Bool {
        ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
    }

    func launch(database url: URL, executable: URL?) async {
        guard let executable else {
            phase = .failed("The ressac helper is missing from the app bundle. Rebuild the app.")
            return
        }
        let runs = RunCoordinator(cli: RessacCLI(executable: executable, database: url))
        do {
            switch try AppDatabase.check(at: url) {
            case let .tooNew(version):
                phase = .tooNew(version)
                return
            case .missing, .needsMigration:
                // A missing file is created by `migrate` too: the first-run screen then
                // deals with "no feeds" only (phase 11).
                phase = .migrating
                guard await runs.submit(.migrate()), try AppDatabase.check(at: url) == .ok else {
                    phase = .failed(runs.banner?.message ?? "The database could not be migrated.")
                    return
                }
            case .ok:
                break
            }
            phase = .ready(AppState(database: try AppDatabase.open(at: url), runs: runs))
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }
}
```

- [ ] **Step 4: Run.** Expected: `✔ Test run with 5 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/AppStateTests.swift
git commit -m "macOS app: launch sequence with helper check, schema check and ressac migrate"
```

### Task 9.3: The window, the sidebar, the entry list and the menus

**Interfaces:**
- Consumes: `AppState`, `Launcher`, `RunCoordinator.progress`/`banner`, `SidebarNode`, `EntryRow`, `HTMLText.plain`, `AppDatabase.defaultURL()`, `RessacCLI.bundledExecutable()`.
- Produces:
  - `App/Commands.swift`: `struct RessacCommands: Commands { let state: AppState? }`. File: Import OPML… ⌘O, Export OPML… ⇧⌘E, Refresh ⌘R, Refresh All Feeds ⇧⌘R. Edit: Search ⌘F (replaces the text-editing group). View: Bigger Text ⌘+, Smaller Text ⌘−, Actual Size ⌘0. Go: Next Entry `j`, Previous Entry `k`, Scroll Down or Next Unread Space. Article: Mark as Read/Unread `m`, Star/Unstar `s`, Mark All as Read ⇧⌘A, Full Article, Open in Browser ⌘↩. Every item is disabled when it cannot act.
  - `Reading/MainView.swift`: `MainView`, `BannerView`, `SidebarView`, `SidebarRow`, `EntryListView`, `EntryRowView`, `ReaderPlaceholder`.
  - `App/RessacApp.swift`: `RessacApp` (a `WindowGroup` driven by a `Launcher`, plus the commands) and `LaunchView` (which injects `.environment(state)`).
- Arrow keys are not menu commands: the `List`s handle ↓/↑ themselves, through the `selection` bindings.

There is no unit test in this task: the logic the views call is tested in Tasks 9.1 and 9.2, and the spec excludes UI snapshot tests. Step 5 is a launch smoke test on a temporary database.

- [ ] **Step 1: Write the menus** `apps/macos/Ressac/App/Commands.swift`:

```swift
import SwiftUI

/// Menu bar commands with every shortcut of spec §7. The items live in small views so
/// that SwiftUI tracks the `AppState` properties they read.
struct RessacCommands: Commands {
    let state: AppState?

    var body: some Commands {
        CommandGroup(replacing: .newItem) { FileMenuItems(state: state) }
        CommandGroup(replacing: .textEditing) { SearchMenuItem(state: state) }
        CommandGroup(after: .toolbar) { TextSizeMenuItems() }
        CommandMenu("Go") { GoMenuItems(state: state) }
        CommandMenu("Article") { ArticleMenuItems(state: state) }
    }
}

/// A modifier-free shortcut, or none when the command cannot act. AppKit swallows a menu
/// key equivalent even when its item is disabled, so a bound `j` would never reach the
/// search field, and of two items sharing a key only the first is ever tried.
private func letter(_ key: KeyEquivalent, when enabled: Bool?) -> KeyboardShortcut? {
    enabled == true ? KeyboardShortcut(key, modifiers: []) : nil
}

private struct FileMenuItems: View {
    let state: AppState?

    var body: some View {
        Button("Import OPML…") { state?.pendingPanel = .importOPML }
            .keyboardShortcut("o")
            .disabled(state == nil)
        Button("Export OPML…") { state?.pendingPanel = .exportOPML }
            .keyboardShortcut("e", modifiers: [.command, .shift])
            .disabled(state == nil)
        Divider()
        Button("Refresh") { state?.refreshCurrent() }
            .keyboardShortcut("r")
            .disabled(state?.canRefresh != true)
        Button("Refresh All Feeds") { state?.refreshAll() }
            .keyboardShortcut("r", modifiers: [.command, .shift])
            .disabled(state?.canRefresh != true)
    }
}

private struct SearchMenuItem: View {
    let state: AppState?

    var body: some View {
        Button("Search") { state?.isSearchFocused = true }
            .keyboardShortcut("f")
            .disabled(state == nil)
    }
}

/// Reader text size in points (12...28, default 17). The key is phase 10's `ReaderStyle.Keys.size`.
private struct TextSizeMenuItems: View {
    @AppStorage("reader.size") private var textSize = 17.0

    var body: some View {
        Button("Bigger Text") { textSize = min(28, textSize + 1) }
            .keyboardShortcut("+")
        Button("Smaller Text") { textSize = max(12, textSize - 1) }
            .keyboardShortcut("-")
        Button("Actual Size") { textSize = 17 }
            .keyboardShortcut("0")
        Divider()
    }
}

private struct GoMenuItems: View {
    let state: AppState?

    var body: some View {
        Button("Next Entry") { state?.selectNext() }
            .keyboardShortcut(letter("j", when: state?.canNavigateEntries))
            .disabled(state?.canNavigateEntries != true)
        Button("Previous Entry") { state?.selectPrevious() }
            .keyboardShortcut(letter("k", when: state?.canNavigateEntries))
            .disabled(state?.canNavigateEntries != true)
        Button("Scroll Down or Next Unread") {
            if let state { Task { await state.advance() } }
        }
        .keyboardShortcut(letter(.space, when: state?.canNavigateEntries))
        .disabled(state?.canNavigateEntries != true)
    }
}

private struct ArticleMenuItems: View {
    let state: AppState?

    var body: some View {
        Button(state?.selectedEntry?.isRead == true ? "Mark as Unread" : "Mark as Read") { state?.toggleRead() }
            .keyboardShortcut(letter("m", when: state?.canToggleEntry))
            .disabled(state?.canToggleEntry != true)
        Button(state?.selectedEntry?.isStarred == true ? "Unstar" : "Star") { state?.toggleStar() }
            .keyboardShortcut(letter("s", when: state?.canToggleEntry))
            .disabled(state?.canToggleEntry != true)
        Button("Mark All as Read") { state?.markAllRead() }
            .keyboardShortcut("a", modifiers: [.command, .shift])
            .disabled(state?.canMarkAllRead != true)
        Divider()
        Button("Full Article") { state?.requestFullText() }
            .disabled(state?.canRequestFullText != true)
        Button("Open in Browser") { state?.openInBrowser() }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(state?.canOpenInBrowser != true)
    }
}
```

- [ ] **Step 2: Write the window** `apps/macos/Ressac/Reading/MainView.swift`:

```swift
import SwiftUI

/// The three-column window (spec §7): sidebar 220 pt, list 320 pt, reader.
struct MainView: View {
    @Bindable var state: AppState
    @FocusState private var searchFocused: Bool

    var body: some View {
        NavigationSplitView {
            SidebarView(state: state)
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } content: {
            Group {
                if state.selection.isGarden {
                    // Phase 12 replaces this with the Garden table.
                    Text("Garden").foregroundStyle(.secondary)
                } else {
                    EntryListView(state: state)
                }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 480)
        } detail: {
            if state.selection.isGarden {
                // Phase 12 replaces this with the Garden inspector.
                Text("No feed selected").foregroundStyle(.secondary)
            } else {
                // Phase 10 replaces this with the WKWebView reader. Pass the complete
                // row (`selectedEntry`), never a list row: list rows carry no content.
                ReaderPlaceholder(entry: state.selectedEntry)
            }
        }
        .searchable(text: $state.searchText, placement: .toolbar, prompt: "Search")
        .searchFocused($searchFocused)
        .onChange(of: searchFocused) { _, focused in state.isSearchFocused = focused }
        .onChange(of: state.isSearchFocused) { _, focused in searchFocused = focused }
        .toolbar {
            ToolbarItem {
                Picker("Show", selection: $state.unreadOnly) {
                    Text("All").tag(false)
                    Text("Unread").tag(true)
                }
                .pickerStyle(.segmented)
                .disabled(state.selection.isGarden)
            }
            ToolbarItem {
                if let progress = state.runs.progress {
                    ProgressView(value: progress.fraction) { Text(progress.label) }
                        .progressViewStyle(.linear)
                        .frame(width: 140)
                        .help(progress.label)
                } else {
                    Button("Refresh", systemImage: "arrow.clockwise") { state.refreshCurrent() }
                        .disabled(!state.canRefresh)
                }
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if let banner = state.runs.banner {
                BannerView(banner: banner) { state.runs.banner = nil }
            }
        }
    }
}

struct BannerView: View {
    let banner: Banner
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(banner.message).lineLimit(2).textSelection(.enabled)
            Spacer()
            if banner.showLog {
                Button("Show Log") {
                    NSWorkspace.shared.open(URL.homeDirectory.appending(path: "Library/Logs/Ressac"))
                }
            }
            Button("Dismiss", systemImage: "xmark", action: dismiss).labelStyle(.iconOnly).buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

struct SidebarView: View {
    @Bindable var state: AppState

    var body: some View {
        List(selection: $state.selection) {
            Section("Smart Lists") {
                Label("Unread", systemImage: "circle.inset.filled").badge(state.sidebar.unread).tag(Source.unread)
                Label("Today", systemImage: "sun.max").badge(state.sidebar.today).tag(Source.today)
                Label("Starred", systemImage: "star").badge(state.sidebar.starred).tag(Source.starred)
            }
            Section("Feeds") {
                OutlineGroup(state.sidebarTree, children: \.children) { node in
                    SidebarRow(node: node, state: state).tag(node.source)
                }
            }
            Section("Garden") {
                Label("Needs Triage", systemImage: "leaf").badge(state.sidebar.needsTriage)
                    .tag(Source.garden(.needsTriage))
                Label("All Feeds", systemImage: "list.bullet").tag(Source.garden(.all))
                Label("Cemetery", systemImage: "archivebox").tag(Source.garden(.cemetery))
            }
        }
        .listStyle(.sidebar)
    }
}

struct SidebarRow: View {
    let node: SidebarNode
    let state: AppState

    var body: some View {
        switch node.kind {
        case .folder:
            Label(node.title, systemImage: "folder").badge(node.unread)
        case let .feed(feed):
            HStack(spacing: 6) {
                Text(feed.title).lineLimit(1)
                if feed.isSick, let status = feed.status {
                    Circle().fill(Self.color(for: status)).frame(width: 7, height: 7).help(status)
                }
            }
            .badge(feed.unread)
            .contextMenu {
                Button("Refresh") { state.refresh(feedId: feed.feedId) }.disabled(!state.canRefresh)
                Toggle("Always Fetch Full Article", isOn: Binding(
                    get: { feed.preferFullText },
                    set: { state.setPreferFullText(feedId: feed.feedId, on: $0) }))
                Button("Mark All as Read") { state.markAllRead(in: .feed(feed.feedId)) }.disabled(feed.unread == 0)
                Button("Open Site") { state.openSite(feedId: feed.feedId) }.disabled(feed.htmlUrl == nil)
                Button("Show in Garden") { state.showInGarden(feedId: feed.feedId) }
            }
        }
    }

    static func color(for status: String) -> Color {
        switch status {
        case "Dead": .red
        case "Hijacked": .purple
        case "Moved": .blue
        case "Duplicate": .teal
        default: .orange
        }
    }
}

struct EntryListView: View {
    @Bindable var state: AppState

    var body: some View {
        List(state.entries, selection: $state.selectedEntryId) { entry in
            EntryRowView(entry: entry)
                .tag(entry.entryId)
                .onDrag {
                    guard let link = entry.link else { return NSItemProvider() }
                    let provider = NSItemProvider(object: link as NSURL)
                    provider.suggestedName = entry.displayTitle
                    return provider
                }
        }
        .overlay {
            if state.entries.isEmpty {
                Text(state.emptyListMessage).foregroundStyle(.secondary)
            }
        }
    }
}

struct EntryRowView: View {
    let entry: EntryRow

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Circle().fill(entry.isRead ? .clear : Color.accentColor).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(entry.feedTitle).lineLimit(1)
                    Spacer()
                    if entry.isStarred { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                    Text(entry.publishedAt, format: .relative(presentation: .named))
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                Text(entry.displayTitle)
                    .fontWeight(entry.isRead ? .regular : .semibold)
                    .foregroundStyle(entry.isRead ? .secondary : .primary)
                    .lineLimit(2)
                Text(entry.excerpt).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.vertical, 3)
    }
}

/// Stand-in for the reader column until phase 10.
struct ReaderPlaceholder: View {
    let entry: EntryRow?

    var body: some View {
        if let entry {
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    Text(entry.displayTitle).font(.title)
                    Text(entry.feedTitle).foregroundStyle(.secondary)
                    Text(HTMLText.plain(entry.contentHtml ?? entry.summaryHtml ?? "", limit: 20_000))
                }
                .padding(24)
                .frame(maxWidth: 680, alignment: .leading)
            }
        } else {
            Text("No entry selected").foregroundStyle(.secondary)
        }
    }
}
```

- [ ] **Step 3: Replace** `apps/macos/Ressac/App/RessacApp.swift` with:

```swift
import SwiftUI

@main
struct RessacApp: App {
    @State private var launcher = Launcher()

    var body: some Scene {
        WindowGroup("Ressac") {
            LaunchView(launcher: launcher)
                .frame(minWidth: 900, minHeight: 500)
                .task {
                    // The unit-test host must not open the real database.
                    guard !Launcher.isRunningTests else { return }
                    await launcher.launch(database: AppDatabase.defaultURL(), executable: RessacCLI.bundledExecutable())
                }
        }
        .commands { RessacCommands(state: launcher.state) }
    }
}

/// What the window shows for each launch phase (spec §8, Schema version).
struct LaunchView: View {
    let launcher: Launcher

    var body: some View {
        switch launcher.phase {
        case .starting:
            ProgressView()
        case .migrating:
            ProgressView("Updating the database…")
        case .tooNew:
            Text("This database was created by a newer Ressac").font(.title3)
        case let .failed(message):
            VStack(spacing: 12) {
                Text(message).multilineTextAlignment(.center)
                Button("Quit Ressac") { NSApplication.shared.terminate(nil) }
            }
            .padding(40)
        case let .ready(state):
            MainView(state: state)
                .environment(state)
        }
    }
}
```

- [ ] **Step 4: Run** the whole suite. Expected: `✔ Test run with 62 tests in 11 suites passed`.

- [ ] **Step 5: Smoke launch on a temporary database.** `RESSAC_DB` points the app at the copy; `HOME` keeps the helper's logs out of `~/Library`. Never launch the app without `RESSAC_DB` before phase 13.

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
cp tests/fixtures/contract.sqlite "$SMOKE/ressac.sqlite"
sqlite3 "$SMOKE/ressac.sqlite" "INSERT INTO entry (feed_id, guid, url, title, published_at, summary_html, fetched_at)
  VALUES (1, 'smoke-1', 'https://xkcd.com/1', 'Hello', '2026-10-09T08:00:00.000Z', '<p>hi there</p>', '2026-10-10T08:00:00.000Z');
  INSERT INTO entry_fts (rowid, title, author, body) VALUES (last_insert_rowid(), 'Hello', '', 'hi there');"
RESSAC_DB="$SMOKE/ressac.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" >"$SMOKE/app.log" 2>&1 &
PID=$!
sleep 6
ps -p "$PID" -o stat= && lsof -p "$PID" | grep -c "$SMOKE/ressac.sqlite"
kill "$PID"
```

Expected: a state such as `SN` (the app is still running after 6 s) and a count of at least `1` (the app holds the temporary database open). If a window appears, it shows the sidebar with Blogs, Biblis, Infos and Jon, and "Hello" in the Unread list.

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/App/Commands.swift apps/macos/Ressac/Reading/MainView.swift apps/macos/Ressac/App/RessacApp.swift
git commit -m "macOS app: three-column window, sidebar, entry list, menus and shortcuts"
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` with 62 Swift tests, `no warnings in our sources`. Push.

**Manual checks carried to phase 13's acceptance run** (they need a person at the keyboard):
1. Typing `j`, `k`, `m`, `s` and a space in the search field inserts them; with the focus in the list they act.
2. ⌘F focuses the search field; Esc clears it and the list comes back.
3. Every menu item shows its shortcut, and ⇧⌘A, ⌘↩ and Full Article are greyed out when nothing applies.

### Notes for the executor

- **AppKit swallows a menu key equivalent even when its item is disabled,** and of two items bound to the same key it only ever tries the first. Checked with an `NSMenu` probe: a disabled `k` item before an enabled `k` item made `performKeyEquivalent` return `true` and fire nothing. So a plain-letter shortcut must not exist while the letter has to reach a text field. `Commands.swift` attaches `j`, `k`, `m`, `s` and Space with `keyboardShortcut(_ shortcut: KeyboardShortcut?)` (nil removes the key equivalent) only while the command can act. The menu contents are small `View`s, so SwiftUI tracks the `AppState` properties they read; this refresh could not be exercised headless (manual check 1).
- **`.immediate` scheduling** delivers an observation's first value synchronously inside `start(in:scheduling:onError:onChange:)` when called on the main actor. That is why the tests can assert `state.entries` right after changing `selection`. Later values arrive asynchronously, hence `eventually`.
- **`@Observable` keeps `didSet`.** The observers on `selection`, `selectedEntryId`, `unreadOnly` and `searchText` restart the observations; they compare with `oldValue` so that writing the same value does nothing.
- **The test host never opens the real database.** `RessacApp` skips `launch` when `XCTestConfigurationFilePath` is set (`Launcher.isRunningTests`), which Xcode sets in the hosted app.
- **Test timing.** `makeState` sets `autoReadDelay` to 100 ms. The busy test opens the database with a 0.2 s timeout while `holdWriteLock` keeps the core's lock for 1 s.
- **`List(selection:)` with a non-optional `Source`** uses the macOS 13+ initialiser `init(selection: Binding<SelectionValue>, content:)`. `.searchable(text:placement:prompt:)` and `.searchFocused(_:)` exist on macOS 15+; the deployment target is 27.0.
