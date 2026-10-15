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
    /// False while another window or a panel has the keyboard (see `windowDidChangeKey`).
    private(set) var readingWindowIsKey = true
    /// The full article was asked for the selected entry. It stays true after a failed
    /// extraction, so that the page keeps its "Full article unavailable" notice until the
    /// selection changes; `isShowingFullText` says whether the article is on screen.
    var readerShowsFullText = false
    /// The feed selected in the Garden table (phase 12); garden commands act on it.
    var selectedFeedId: Int64?
    var pendingPanel: PendingPanel?

    // MARK: Data

    private(set) var sidebar = SidebarSnapshot()
    /// The entry list, or the search results while `searchText` is not empty: its loaded rows,
    /// newest first. A long list loads `listPageSize` rows at a time (`loadMoreEntries`).
    private(set) var entries: [EntryRow] = []
    /// The whole list's size, loaded or not, with the entries read during this visit that it
    /// keeps: the "/20" of the sub-page line.
    private(set) var entryTotal = 0
    /// Unread entries of the whole list, loaded or not.
    private(set) var listUnread = 0
    /// Rows of the list are not loaded yet.
    private(set) var hasMoreEntries = false
    /// The selected entry with its full content, for the reader.
    private(set) var selectedEntry: EntryRow?
    private(set) var selectedFulltext: FulltextRow?
    /// Entries whose full article is being fetched.
    private(set) var extractingEntryIds: Set<Int64> = []
    /// "Full article unavailable: …" for the selected entry (shown inline, spec §8).
    private(set) var fullTextNotice: String?
    /// Garden rows decided since the scope was entered, dimmed by the table.
    private(set) var decidedThisVisit: Set<Int64> = []

    let database: AppDatabase
    let writes: Writes
    let runs: RunCoordinator

    @ObservationIgnored var autoReadDelay: Duration = .seconds(1)
    /// Rows a list loads at a time. A list starts with one page and grows by one per `loadMoreEntries`.
    @ObservationIgnored var listPageSize = 500
    /// Rows from the end of the loaded ones whose appearance loads the next page (`entryRowAppeared`).
    static let loadMoreMargin = 20
    @ObservationIgnored var now: @MainActor () -> Date = { Date() }
    @ObservationIgnored var openURL: @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) }
    /// Set by the reader (phase 10): scrolls one page of the given entry and returns false at
    /// its bottom. True, with nothing done, while that entry's page is still loading.
    @ObservationIgnored var readerPageDown: (@MainActor (Int64) async -> Bool)?

    @ObservationIgnored private var externalChanges: ExternalChangeMonitor?
    @ObservationIgnored private var systemObservers: [any NSObjectProtocol] = []
    /// The windows showing a reading root view (`MainView` or `AntiopeRootView`), each
    /// registered by its `ReadingWindowReader`.
    @ObservationIgnored private let readingWindows = NSHashTable<NSWindow>.weakObjects()
    /// How many readers hold each of them: on a live Appearance switch the new look's reader can
    /// arrive before the old one leaves.
    @ObservationIgnored private var readingWindowHolds: [ObjectIdentifier: Int] = [:]
    /// One Space at a time: a second press while the first runs is spent without effect.
    @ObservationIgnored private var isAdvancing = false
    @ObservationIgnored private var sidebarObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var listObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var entryObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var fulltextObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var autoReadTask: Task<Void, Never>?
    /// Entries read during this visit of the source: they stay listed in unread-only
    /// lists until the source changes, so the selection never vanishes.
    @ObservationIgnored private var retained: Set<Int64> = []
    /// The list's oldest loaded row. It only moves down while the list is shown, so new entries
    /// never push a loaded row (the selection) out. Nil: the list was short enough to load whole
    /// when it started; `.oldest`: its last page was loaded.
    @ObservationIgnored private var listFloor: ListFloor?
    @ObservationIgnored private var isLoadMoreScheduled = false
    /// Counts the list restarts (`listDidChange`): a deferred page only loads into the list it was scheduled for.
    @ObservationIgnored private var listGeneration = 0
    /// The last restart could not read its floor, so the list loaded whole. Until the next restart
    /// succeeds, it is not paged again (that would fail again, and report again, every turn).
    @ObservationIgnored private var listFloorFailed = false

    init(database: AppDatabase, runs: RunCoordinator) {
        self.database = database
        self.writes = Writes(db: database)
        self.runs = runs
        runs.onDatabaseChange = { [database] in
            Task { try? await database.notifyExternalChanges() }
        }
        startSidebarObservation()
        listDidChange()
    }

    private func startSidebarObservation() {
        sidebarObservation = Queries.sidebar(now: now()).start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] in self?.sidebar = $0 })
    }

    // MARK: System events

    /// Starts what the running app needs from the system: refreshing on activation and polling
    /// for terminal writes (`ExternalChangeMonitor`), the midnight restart of the date-based
    /// queries, and tracking which window has the keyboard. Not started in unit tests.
    func startSystemObservers() {
        guard externalChanges == nil else { return }
        let monitor = ExternalChangeMonitor(database: database)
        monitor.start()
        externalChanges = monitor
        let center = NotificationCenter.default
        systemObservers = [
            center.addObserver(forName: .NSCalendarDayChanged, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.dayDidChange() }
            },
            center.addObserver(forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] note in
                let window = note.object as? NSWindow
                MainActor.assumeIsolated { self?.windowDidChangeKey(window, isKey: true) }
            },
            center.addObserver(forName: NSWindow.didResignKeyNotification, object: nil, queue: .main) { [weak self] note in
                let window = note.object as? NSWindow
                MainActor.assumeIsolated { self?.windowDidChangeKey(window, isKey: false) }
            },
        ]
    }

    /// Midnight passed: "Today" counts and lists were built around the previous day's start.
    func dayDidChange() {
        startSidebarObservation()
        listDidChange()
    }

    /// Single-letter shortcuts belong to the reading window: while a panel or another window has
    /// the keyboard (Export…, New…, Settings), typing "jams" must stay typing.
    func windowDidChangeKey(_ window: NSWindow?, isKey: Bool) {
        guard readingWindows.count > 0 else { return }
        if let window, readingWindows.contains(window) { readingWindowIsKey = isKey }
        else if isKey { readingWindowIsKey = false }
    }

    /// Called by a root view's `ReadingWindowReader` when it enters a window.
    func registerReadingWindow(_ window: NSWindow) {
        let id = ObjectIdentifier(window)
        // The table is weak: a window freed without `leave()` leaves its count behind, and a new
        // window can reuse the address. Not in the table means no live holder, so start over.
        if !readingWindows.contains(window) { readingWindowHolds[id] = 0 }
        readingWindows.add(window)
        readingWindowHolds[id, default: 0] += 1
        readingWindowIsKey = window.isKeyWindow
    }

    /// Called by a root view's reader when it leaves `window`. The window stays a reading
    /// window while another reader holds it; another window's registration stays too, and with
    /// no reading window left the letters are off.
    func unregisterReadingWindow(_ window: NSWindow) {
        let id = ObjectIdentifier(window)
        guard readingWindows.contains(window), let holds = readingWindowHolds[id] else { return }
        guard holds == 1 else {
            readingWindowHolds[id] = holds - 1
            return
        }
        readingWindowHolds[id] = nil
        readingWindows.remove(window)
        readingWindowIsKey = readingWindows.allObjects.contains { $0.isKeyWindow }
    }

    var sidebarTree: [SidebarNode] { SidebarNode.tree(from: sidebar) }

    var emptyListMessage: String {
        if !searchText.isEmpty { return "No results" }
        if selection == .unread || unreadOnly { return "No unread entries" }
        return "No entries"
    }

    // MARK: Command availability (menus disable what does nothing, spec §7)

    var letterShortcutsEnabled: Bool { !isSearchFocused && readingWindowIsKey }
    var canNavigateEntries: Bool { letterShortcutsEnabled && !selection.isGarden && !entries.isEmpty }
    var canToggleEntry: Bool { letterShortcutsEnabled && !selection.isGarden && selectedEntry != nil }
    var canMarkAllRead: Bool {
        !selection.isGarden && searchText.isEmpty && (entries.contains { !$0.isRead } || (hasMoreEntries && listUnread > 0))
    }
    var canRefresh: Bool { !runs.isLongRunActive }
    var canOpenInBrowser: Bool { selectedEntry?.link != nil }
    var canRequestFullText: Bool { selectedEntry?.link != nil && !isExtracting }
    /// The selected entry's full article is being fetched.
    var isExtracting: Bool { selectedEntryId.map(extractingEntryIds.contains) ?? false }
    /// The full article is on screen: the Full Article button is on only then.
    var isShowingFullText: Bool {
        readerShowsFullText && selectedFulltext?.html != nil && selectedFulltext?.entryId == selectedEntryId
    }
    /// Why the full article asked for is not shown, while its notice is on screen.
    var fullTextFailure: String? {
        guard readerShowsFullText, !isExtracting, !isShowingFullText else { return nil }
        if let error = selectedFulltext?.error, selectedFulltext?.entryId == selectedEntryId {
            return "Full article unavailable: \(error)"
        }
        return fullTextNotice
    }
    var canGardenDecide: Bool { selection.isGarden && selectedFeedId != nil }

    // MARK: Reading commands

    /// The next entry; past the last loaded row, the next page first.
    func selectNext() {
        guard canNavigateEntries else { return }
        if let index = selectedIndex, index + 1 == entries.count { loadMoreEntries() }
        let next = selectedIndex.map { $0 + 1 } ?? 0
        guard next < entries.count else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = entries[next].entryId
    }

    func selectPrevious() {
        guard canNavigateEntries else { return }
        let previous = selectedIndex.map { $0 - 1 } ?? 0
        guard previous >= 0, previous < entries.count else { return }
        selectedEntryId = entries[previous].entryId
    }

    /// The next unread entry after the selection (spec §7, Space at the bottom). When none is
    /// loaded, one query finds the next unread entry of the whole list and the floor goes down to
    /// it once, instead of loading pages while looking.
    func selectNextUnread() {
        guard canNavigateEntries else { return }
        func nextLoaded() -> EntryRow? {
            let start = selectedIndex.map { $0 + 1 } ?? 0
            return entries[start...].first(where: { !$0.isRead && $0.entryId != selectedEntryId })
        }
        if nextLoaded() == nil, hasMoreEntries, let current = listFloor {
            do {
                // Everything loaded was scanned above: look past the last loaded row.
                let target = try nextUnreadFloor(after: entries.last?.entryId)
                guard let target else { return }
                if target < current { listFloor = target }
                observeList()
            } catch {
                report(error)
                return
            }
        }
        guard let next = nextLoaded() else { return }
        markSelectedReadOnLeaving()
        selectedEntryId = next.entryId
    }

    /// A list view shows the row of `entryId`. Among the last `loadMoreMargin` loaded rows, the
    /// next page loads, on the next turn: a row appears while its table lays out, where changing
    /// the rows is a reentrant operation. One page per turn, however many rows appear.
    func entryRowAppeared(_ entryId: Int64) {
        guard !isLoadMoreScheduled, hasMoreEntries,
              entries.suffix(Self.loadMoreMargin).contains(where: { $0.entryId == entryId })
        else { return }
        isLoadMoreScheduled = true
        let generation = listGeneration
        Task { [weak self] in
            guard let self else { return }
            isLoadMoreScheduled = false
            guard generation == listGeneration else { return }  // the source, search or filter changed since
            loadMoreEntries()
        }
    }

    /// Loads the next `listPageSize` rows of the list. `entries` has them on return.
    func loadMoreEntries() {
        guard hasMoreEntries, let current = listFloor else { return }
        do {
            listFloor = try floor(below: current, keeping: nil) ?? .oldest
        } catch {
            report(error)
            return
        }
        observeList()
    }

    /// Space: scroll the reader; at the bottom, next unread. One at a time: nil (nothing done)
    /// while the previous one runs. A held Space (`isRepeat`) scrolls but never moves on, so
    /// that holding it through an article cannot sweep past entries unseen.
    @discardableResult
    func advance(isRepeat: Bool = false) -> Task<Void, Never>? {
        guard canNavigateEntries, !isAdvancing else { return nil }
        isAdvancing = true
        let entryId = selectedEntryId
        return Task {
            defer { isAdvancing = false }
            if let readerPageDown, let entryId, await readerPageDown(entryId) { return }
            // Not when the selection changed meanwhile (a click): that entry is the one to read.
            guard !isRepeat, selectedEntryId == entryId else { return }
            selectNextUnread()
        }
    }

    /// A key routed by the reading window (`ReadingKeyRouter`). False when the command cannot
    /// act here, so that the key goes on to the focused view (the Garden's own keys, the arrows
    /// of an empty list).
    /// `isRepeat`: the key is held. A held toggle is spent without toggling again.
    @discardableResult
    func perform(_ command: ReadingCommand, isRepeat: Bool = false) -> Bool {
        switch command {
        case .nextEntry:
            guard canNavigateEntries else { return false }
            selectNext()
        case .previousEntry:
            guard canNavigateEntries else { return false }
            selectPrevious()
        case .advance:
            guard canNavigateEntries else { return false }
            advance(isRepeat: isRepeat)
        case .toggleRead:
            return isRepeat ? canToggleEntry : toggleRead() != nil
        case .toggleStar:
            return isRepeat ? canToggleEntry : toggleStar() != nil
        case .focusEntryList:
            // In a Garden scope the list column is the Garden table: focus only.
            if !selection.isGarden, selectedEntryId == nil { selectedEntryId = entries.first?.entryId }
        case .focusSidebar:
            break
        case .pageDigit, .pageCorrect, .pageCancel, .sommaire, .guide, .repetition, .toggleImages,
             .nextColumn, .previousColumn, .nextIndexRow, .previousIndexRow:
            // Antiope's own keys: `AntiopeState.handle(_:)` had them first and declined them,
            // so the key goes on to the focused view.
            return false
        }
        return true
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
    /// On: back to the feed's text. Off with a stored article: show it. Otherwise fetch it,
    /// which after a failure is a retry.
    func requestFullText() -> Task<Void, Never>? {
        guard let entry = selectedEntry, entry.link != nil else { return nil }
        if isShowingFullText {
            readerShowsFullText = false
            return nil
        }
        if entry.hasFulltext {
            readerShowsFullText = true
            return nil
        }
        guard !isExtracting else { return nil }
        readerShowsFullText = true
        extractingEntryIds.insert(entry.entryId)
        fullTextNotice = nil
        let entryId = entry.entryId
        let run = runs.start(.extract(entryId: entryId)) { [weak self] event in
            if case let .extracted(id, false, error) = event, id == entryId, self?.selectedEntryId == entryId {
                self?.fullTextNotice = "Full article unavailable: \(error ?? "unknown error")"
            }
        }
        return Task {
            let outcome = await run.value
            extractingEntryIds.remove(entryId)
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

    /// The list restarts with one page, keeping the selection loaded.
    private func listDidChange() {
        listGeneration += 1
        do {
            listFloor = try floor(below: nil, keeping: selectedEntryId)
            listFloorFailed = false
        } catch {
            listFloor = nil
            listFloorFailed = true
            report(error)
        }
        observeList()
    }

    /// The search words, or nil for the plain list.
    private var activeSearch: String? {
        searchText.trimmingCharacters(in: .whitespaces).isEmpty ? nil : searchText
    }

    private var selectedIndex: Int? {
        selectedEntryId.flatMap { id in entries.firstIndex { $0.entryId == id } }
    }

    /// The floor one page below `current` (below the newest row when nil), lowered to keep entry
    /// `keeping` loaded; nil when the rest of the list fits in the page.
    private func floor(below current: ListFloor?, keeping entryId: Int64?) throws -> ListFloor? {
        let (source, unreadOnly, search, rows, now) = (selection, unreadOnly, activeSearch, listPageSize, now())
        return try database.reader.read { db in
            try Queries.floor(source: source, unreadOnly: unreadOnly, search: search, below: current, rows: rows,
                              keeping: entryId, now: now, in: db)
        }
    }

    /// The floor that loads the first unread entry of the list after entry `entryId`.
    private func nextUnreadFloor(after entryId: Int64?) throws -> ListFloor? {
        let (source, unreadOnly, search, now) = (selection, unreadOnly, activeSearch, now())
        return try database.reader.read { db in
            try Queries.nextUnread(source: source, unreadOnly: unreadOnly, search: search, after: entryId, now: now, in: db)
        }
    }

    /// (Re)starts the list's observation at `listFloor`. `.immediate`: `entries` is up to date on return.
    private func observeList() {
        let observation = activeSearch.map { Queries.search($0, in: selection, floor: listFloor, now: now()) }
            ?? Queries.entries(source: selection, unreadOnly: unreadOnly, floor: listFloor, now: now())
        listObservation = observation.start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] page in self?.listDidLoad(page) })
    }

    private func listDidLoad(_ page: EntryPage) {
        let merged = Self.merge(fresh: page.rows, previous: entries, retained: retained, floor: listFloor)
        // Equal after an optimistic change already made: the views are not updated twice.
        if merged != entries { entries = merged }
        entryTotal = page.total + merged.count - page.rows.count
        listUnread = page.unread
        hasMoreEntries = page.rows.count < page.total
        // A list that was whole when it started (a short one) and that a refresh made long, as on
        // the first run: one page again. On the next turn, not inside the observation's callback.
        if listFloor == nil, !listFloorFailed, page.rows.count > listPageSize {
            Task { [weak self] in
                guard let self, listFloor == nil, !listFloorFailed, entries.count > listPageSize else { return }
                listDidChange()
            }
        }
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
    /// dropped, newest first. Only rows at or after `floor` are kept: below it, a row of the
    /// list is merely not loaded.
    static func merge(fresh: [EntryRow], previous: [EntryRow], retained: Set<Int64>, floor: ListFloor?) -> [EntryRow] {
        let freshIds = Set(fresh.map(\.entryId))
        let kept = previous.filter {
            retained.contains($0.entryId) && !freshIds.contains($0.entryId) && floor?.admits($0) != false
        }
        guard !kept.isEmpty else { return fresh }
        return (fresh + kept.map { row in
            var row = row
            row.isRead = true
            return row
        }).sorted { ($0.publishedAt, $0.entryId) > ($1.publishedAt, $1.entryId) }
    }
}

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
    /// True while a launch runs. Each window's `.task` launches (a window reopened from the Dock,
    /// a new tab), and Use Default Database can be clicked twice: a second launch would run
    /// `migrate` again and replace the `AppState` already on screen.
    @ObservationIgnored private var isLaunching = false
    /// Shows the fatal alert of spec §8 (the helper is missing). Tests replace it.
    @ObservationIgnored var presentFatalAlert: @MainActor (String) -> Void = { Launcher.runFatalAlert($0) }

    var state: AppState? {
        if case let .ready(state) = phase { state } else { nil }
    }

    /// True inside the unit-test host, where the app must not open the real database.
    static var isRunningTests: Bool {
        ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
    }

    /// The launch failed or the database is unusable: Settings may hold a bad location.
    var offersDefaultDatabase: Bool {
        switch phase {
        case .failed, .tooNew: true
        case .starting, .migrating, .ready: false
        }
    }

    /// Clears the Settings location and launches again in-process (no app restart).
    /// `RESSAC_DB` still wins, as at any launch.
    func useDefaultDatabase(defaults: UserDefaults = .standard,
                            environment: [String: String] = ProcessInfo.processInfo.environment,
                            executable: URL? = RessacCLI.bundledExecutable()) async {
        guard !isLaunching else { return }
        DatabaseLocation.resetToDefault(defaults: defaults)
        phase = .starting
        await launch(database: DatabaseLocation.current(environment: environment, defaults: defaults), executable: executable)
    }

    /// Launches from `.starting` only, one launch at a time: a later call (another window's
    /// `.task`) leaves the running or finished launch alone.
    func launch(database url: URL, executable: URL?) async {
        guard !isLaunching, case .starting = phase else { return }
        isLaunching = true
        defer { isLaunching = false }
        guard let executable else {
            let message = "The ressac helper is missing from the app bundle. Rebuild the app."
            phase = .failed(message)
            presentFatalAlert(message)
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
            let state = AppState(database: try AppDatabase.open(at: url), runs: runs)
            state.startSystemObservers()
            phase = .ready(state)
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }

    /// The app cannot run a single command without its helper: a critical alert whose only way
    /// out is Quit (spec §8). Never shown in the unit-test host, where it would block the run.
    static func runFatalAlert(_ message: String) {
        guard !isRunningTests else { return }
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Ressac cannot start"
        alert.informativeText = message
        alert.addButton(withTitle: "Quit Ressac")
        _ = alert.runModal()
        NSApplication.shared.terminate(nil)
    }
}
