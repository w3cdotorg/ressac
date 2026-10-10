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
            phase = .ready(AppState(database: try AppDatabase.open(at: url), runs: runs))
        } catch {
            phase = .failed("Cannot open \(url.path): \(error.localizedDescription)")
        }
    }
}
