import AppKit
import Observation

/// The three columns of the Antiope window, left to right (spec §5, Focus and keys).
enum AntiopeColumn: Sendable {
    case index, entries, reader

    var next: AntiopeColumn? {
        switch self {
        case .index: .entries
        case .entries: .reader
        case .reader: nil
        }
    }

    var previous: AntiopeColumn? {
        switch self {
        case .index: nil
        case .entries: .index
        case .reader: .entries
        }
    }
}

/// What the reader column shows: the selected entry, P100 SOMMAIRE or P899 GUIDE.
enum ReaderPage: Equatable, Sendable {
    case article, index, guide
}

/// What only the Antiope look needs, beside the shared `AppState` (spec §5, approach A): the
/// focused column, the page number being typed, `NOT FOUND`, the page in the reader column. It is
/// also that window's `KeyFocusProvider`, and `ReadingKeyRouter` offers it every command first.
@MainActor @Observable final class AntiopeState {
    /// How long an unknown page shows `NOT FOUND` in the header (spec §6, §10).
    static let notFoundDuration: Duration = .seconds(2)

    var column: AntiopeColumn = .index
    var entry = PageEntry()
    /// The unknown page just typed, shown in red in the header until `notFoundDuration` passes.
    var notFound: Int?
    var readerPage: ReaderPage = .article
    /// The `SEARCH?` prompt of the Minitel bar. It is `AppState.isSearchFocused`, so ⌘F (the
    /// Search menu item) opens it and the letter shortcuts stay off while it is open.
    var isSearchPromptOpen: Bool {
        get { app.isSearchFocused }
        set { app.isSearchFocused = newValue }
    }
    /// Set by the Antiope reader (phase 11): swaps every image of the article between its
    /// mosaic and the original. `i` does nothing while it is nil.
    @ObservationIgnored var imageToggle: (@MainActor () -> Void)?

    let app: AppState
    private let clock: any Clock<Duration>
    @ObservationIgnored private var notFoundReset: Task<Void, Never>?
    @ObservationIgnored private var directoryCache: (snapshot: SidebarSnapshot, directory: PageDirectory)?
    /// How many times `directory` was rebuilt, for the tests.
    @ObservationIgnored private(set) var directoryBuilds = 0

    init(app: AppState, clock: any Clock<Duration> = ContinuousClock()) {
        self.app = app
        self.clock = clock
    }

    /// Every page of the library, from the live sidebar snapshot. It is rebuilt only when the
    /// snapshot changed: the header, both columns and the reader read it on every body evaluation.
    /// Reading `app.sidebar` keeps the observation tracking.
    var directory: PageDirectory {
        let snapshot = app.sidebar
        if let cached = directoryCache, cached.snapshot == snapshot { return cached.directory }
        let directory = PageDirectory(snapshot: snapshot)
        directoryCache = (snapshot, directory)
        directoryBuilds += 1
        return directory
    }

    /// The page on screen: P100 or P899 in the reader column, else the selection's page (nil
    /// while the selected folder or feed has no number yet).
    var currentPage: Int? {
        switch readerPage {
        case .index: ReservedPage.index.rawValue
        case .guide: ReservedPage.guide.rawValue
        case .article: directory.number(for: app.selection)
        }
    }

    /// Goes to a page (spec §6): its row is selected in the index, the entry list follows, the
    /// index column takes the focus; P100 and P899 open in the reader. An unknown number shows
    /// `NOT FOUND` and changes nothing else.
    func jump(to number: Int) {
        entry.cancel()
        guard let destination = directory.destination(for: number) else {
            showNotFound(number)
            return
        }
        clearNotFound()
        switch destination {
        case let .source(source):
            app.selection = source
            readerPage = .article
        case .index:
            readerPage = .index
        case .guide:
            readerPage = .guide
        }
        column = .index
    }

    /// True when Antiope used the command; false hands it to `AppState.perform(_:isRepeat:)`,
    /// then to the focused view.
    func handle(_ command: ReadingCommand) -> Bool {
        switch command {
        case let .pageDigit(digit):
            clearNotFound()
            if case let .complete(number) = entry.type(digit) { jump(to: number) }
            return true
        case .pageCorrect:
            guard entry.isActive else { return false }
            entry.correct()
            return true
        case .pageCancel:
            guard entry.isActive else { return false }
            entry.cancel()
            return true
        case .sommaire:
            jump(to: ReservedPage.index.rawValue)
            return true
        case .guide:
            jump(to: ReservedPage.guide.rawValue)
            return true
        case .repetition:
            guard app.canRefresh else { return false }
            app.refreshCurrent()
            return true
        case .toggleImages:
            guard readerPage == .article, app.selectedEntry != nil, let imageToggle else { return false }
            imageToggle()
            return true
        case .nextColumn:
            return move(to: column.next)
        case .previousColumn:
            return move(to: column.previous)
        case .nextIndexRow:
            return moveInIndex(by: 1)
        case .previousIndexRow:
            return moveInIndex(by: -1)
        case .nextEntry, .previousEntry, .advance:
            // SUITE and RETOUR are today's commands and `AppState` performs them (Space still
            // scrolls the article first). Leaving P100 or P899 brings the article back, and that
            // first press is all it does: `AppState` would otherwise move on in the same press,
            // with no reader on screen to scroll, and skip the entry unseen.
            guard readerPage != .article, app.canNavigateEntries else { return false }
            readerPage = .article
            return app.selectedEntryId != nil
        case .toggleRead, .toggleStar, .focusEntryList, .focusSidebar:
            return false
        }
    }

    /// Return keeps the search, Esc clears it (spec §5, Search); both close the prompt. The root
    /// view also closes it when Antiope appears and goes away, so that neither look inherits an
    /// open prompt (Review Focus #5).
    func closeSearchPrompt(clearing: Bool) {
        if clearing { app.searchText = "" }
        isSearchPromptOpen = false
    }

    // MARK: Private

    private func move(to target: AntiopeColumn?) -> Bool {
        guard let target else { return false }
        if target == .entries {
            // As `→` from the native sidebar: the list takes the keyboard on its first entry.
            app.perform(.focusEntryList)
        }
        column = target
        return true
    }

    private func moveInIndex(by offset: Int) -> Bool {
        let rows = directory.rows
        let current = currentPage.flatMap { page in rows.firstIndex { $0.number == page } }
        let target = current.map { $0 + offset } ?? 0
        if rows.indices.contains(target) { jump(to: rows[target].number) }
        return true
    }

    private func showNotFound(_ number: Int) {
        notFoundReset?.cancel()
        notFound = number
        notFoundReset = Task { [weak self, clock] in
            try? await clock.sleep(for: Self.notFoundDuration)
            guard !Task.isCancelled else { return }
            self?.notFound = nil
        }
    }

    private func clearNotFound() {
        notFoundReset?.cancel()
        notFoundReset = nil
        notFound = nil
    }
}

extension AntiopeState: KeyFocusProvider {
    /// The focused column, or `.textInput` while the search prompt or any text view has the
    /// keyboard: digits typed there never start a page number (Review Focus #2).
    func keyFocus(in window: NSWindow?) -> KeyFocus {
        if isSearchPromptOpen || ResponderFocusProvider.isTextInput(window?.firstResponder) { return .textInput }
        switch column {
        case .index: return .sidebar
        case .entries: return .entryList
        case .reader: return .reader
        }
    }
}
