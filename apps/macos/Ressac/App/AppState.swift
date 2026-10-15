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
    /// The entry list, or the search results while `searchText` is not empty.
    private(set) var entries: [EntryRow] = []
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
    var canMarkAllRead: Bool { !selection.isGarden && searchText.isEmpty && entries.contains { !$0.isRead } }
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

    private func listDidChange() {
        let observation = searchText.trimmingCharacters(in: .whitespaces).isEmpty
            ? Queries.entries(source: selection, unreadOnly: unreadOnly, now: now())
            : Queries.search(searchText, in: selection, now: now())
        listObservation = observation.start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] in self?.report($0) },
            onChange: { [weak self] fresh in
                guard let self else { return }
                entries = Self.merge(fresh: fresh.rows, previous: entries, retained: retained)
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
        DatabaseLocation.resetToDefault(defaults: defaults)
        phase = .starting
        await launch(database: DatabaseLocation.current(environment: environment, defaults: defaults), executable: executable)
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
            let state = AppState(database: try AppDatabase.open(at: url), runs: runs)
            state.startSystemObservers()
            phase = .ready(state)
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }
}
