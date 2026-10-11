import AppKit
import Foundation
import GRDB
import Testing
import WebKit

@testable import Ressac

/// The entry whose page the reader has loaded.
@MainActor final class FakeReader {
    var loaded: Int64?
    init(loaded: Int64?) { self.loaded = loaded }
}

/// Acceptance fix, review round 1.
@MainActor @Suite struct AdvanceTests {
    /// Three unread entries, newest first; the first is selected.
    func makeList() async throws -> (AppState, TestDatabase, [Int64]) {
        let (state, test, _) = try makeState()
        let a = try test.entry(feed: 1, published: "2026-10-03T08:00:00.000Z")
        let b = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        let c = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 3 })
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = a
        return (state, test, [a, b, c])
    }

    func readAt(_ test: TestDatabase, _ id: Int64) throws -> String? {
        try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [id])
    }

    /// Important 1: the second Space lands while the first one runs, then while B's page loads.
    @Test func twoSpacesAtTheBottomMoveOneEntryAndMarkOneRead() async throws {
        let (state, test, ids) = try await makeList()
        let reader = FakeReader(loaded: ids[0])
        // At the bottom of the loaded page; a page still loading spends the key.
        state.readerPageDown = { id in reader.loaded != id }
        let first = try #require(state.advance())
        #expect(state.advance() == nil)  // in flight: ignored
        await first.value
        #expect(state.selectedEntryId == ids[1])
        await state.advance()?.value  // B's page is not loaded yet: nothing moves
        #expect(state.selectedEntryId == ids[1])
        #expect(await eventually { (try? readAt(test, ids[0])) != nil })
        try await Task.sleep(for: .milliseconds(100))
        #expect(try readAt(test, ids[1]) == nil)
        #expect(try readAt(test, ids[2]) == nil)
        reader.loaded = ids[1]
        await state.advance()?.value
        #expect(state.selectedEntryId == ids[2])
    }

    @Test func anAutoRepeatedSpaceScrollsButNeverMovesOn() async throws {
        let (state, test, ids) = try await makeList()
        var pages = 1
        state.readerPageDown = { _ in
            pages -= 1
            return pages >= 0
        }
        await state.advance(isRepeat: true)?.value  // scrolls
        await state.advance(isRepeat: true)?.value  // at the bottom: stays
        await state.advance(isRepeat: true)?.value
        #expect(state.selectedEntryId == ids[0])
        #expect(try readAt(test, ids[0]) == nil)
        #expect(state.perform(.advance, isRepeat: true))  // the key is still spent
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.selectedEntryId == ids[0])
        await state.advance()?.value  // a fresh press moves on
        #expect(state.selectedEntryId == ids[1])
    }

    /// Minor 3: holding `m` or `s` toggles once.
    @Test func repeatedToggleKeysAreSpentWithoutToggling() async throws {
        let (state, test, ids) = try await makeList()
        #expect(await eventually { state.selectedEntry != nil })
        #expect(state.perform(.toggleRead, isRepeat: true))
        #expect(state.perform(.toggleStar, isRepeat: true))
        try await Task.sleep(for: .milliseconds(100))
        #expect(try readAt(test, ids[0]) == nil)
        #expect(state.selectedEntry?.isStarred == false)
        state.selection = .garden(.needsTriage)
        #expect(!state.perform(.toggleRead, isRepeat: true))  // nothing to toggle: the key goes on
    }
}

/// The reader's half of Important 1: Space acts only on the page of the entry it is asked about.
@MainActor @Suite struct ReaderControllerTests {
    @Test func pageDownWaitsForTheSelectedEntrysPage() async throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let webView = WKWebView(frame: window.contentLayoutRect)
        window.contentView = webView
        let controller = ReaderController()
        controller.webView = webView
        let coordinator = ArticleWebView.Coordinator(controller: controller)
        webView.navigationDelegate = coordinator

        controller.load(entryId: 1, html: "<p>Short.</p>")
        #expect(await controller.pageDown(entryId: 1))  // still loading: the key is spent
        #expect(await eventually { controller.loadedEntryId == 1 })
        #expect(await !controller.pageDown(entryId: 1))  // loaded, at the bottom: move on
        #expect(await controller.pageDown(entryId: 2))  // another entry's page: wait for it

        let long = String(repeating: "<p>Line of a long article.</p>", count: 400)
        controller.load(entryId: 2, html: long)
        #expect(controller.loadedEntryId == nil)
        #expect(await eventually { controller.loadedEntryId == 2 })
        #expect(await controller.pageDown(entryId: 2))  // scrolls
    }
}

/// Important 2 and Minor 4: the button is on only while the full article is shown.
@MainActor @Suite struct FullArticleStateTests {
    @Test func aFailedExtractionLeavesTheButtonOffAndTheNoticeUp() async throws {
        let (state, test, _) = try makeState(stub: """
            echo '{"event":"extracted","entry_id":'"$5"',"ok":false,"error":"HTTP 403."}'
            """)
        let id = try test.entry(feed: 1, url: "https://example.org/paywalled")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.selectedEntryId = id
        let run = try #require(state.requestFullText())
        #expect(state.isExtracting && !state.isShowingFullText)
        await run.value
        // The core stores the error row too.
        try test.execute(
            "INSERT INTO entry_fulltext (entry_id, fetched_at, error) VALUES (?, '2026-10-10T09:00:00.000Z', 'HTTP 403.')", [id])
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.selectedFulltext?.error == "HTTP 403." })
        #expect(!state.isShowingFullText)
        #expect(state.readerShowsFullText)  // the page keeps its notice
        #expect(state.fullTextFailure == "Full article unavailable: HTTP 403.")
        #expect(ReaderView.fullArticleHelp(for: state.selectedEntry, isShowingFullText: state.isShowingFullText,
                                           isExtracting: state.isExtracting, failure: state.fullTextFailure)
            == "Full article unavailable: HTTP 403. Click to try again")
        state.selectedEntryId = nil
        state.selectedEntryId = id  // the notice lasts until the selection changes
        #expect(!state.readerShowsFullText && state.fullTextFailure == nil)
    }

    @Test func theButtonIsOnOnlyWithTheFullArticleShown() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1, url: "https://example.org/a")
        try test.execute(
            "INSERT INTO entry_fulltext (entry_id, fetched_at, html) VALUES (?, '2026-10-10T09:00:00.000Z', '<p>Full</p>')", [id])
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.selectedEntryId = id
        #expect(state.isShowingFullText && state.fullTextFailure == nil)
        #expect(state.requestFullText() == nil)
        #expect(!state.isShowingFullText)
        #expect(state.requestFullText() == nil)
        #expect(state.isShowingFullText)
    }

    @Test func anExtractionBusiesOnlyItsOwnEntry() async throws {
        let (state, test, _) = try makeState(stub: "sleep 0.3")
        let a = try test.entry(feed: 1, url: "https://example.org/a", published: "2026-10-02T08:00:00.000Z")
        let b = try test.entry(feed: 1, url: "https://example.org/b", published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.selectedEntryId = a
        let run = try #require(state.requestFullText())
        #expect(state.isExtracting && !state.canRequestFullText)
        state.selectedEntryId = b
        #expect(!state.isExtracting && state.canRequestFullText)
        #expect(ReaderView.fullArticleHelp(for: state.selectedEntry, isShowingFullText: false,
                                           isExtracting: state.isExtracting, failure: nil)
            == "Show the full article, fetched from the site")
        state.selectedEntryId = a
        #expect(state.isExtracting)
        await run.value
        #expect(!state.isExtracting)
    }
}

/// Minor 1: each window registers and unregisters itself.
@MainActor @Suite struct ReadingWindowRegistrationTests {
    @Test func unregisteringNeverTurnsTheLettersBackOn() throws {
        let state = try makeState().0
        let first = NSWindow()
        let second = NSWindow()
        let panel = NSPanel()
        state.registerReadingWindow(first)
        state.registerReadingWindow(second)
        state.unregisterReadingWindow(first)  // closing one window keeps the other's registration
        state.windowDidChangeKey(second, isKey: true)
        #expect(state.letterShortcutsEnabled)
        state.windowDidChangeKey(panel, isKey: true)
        #expect(!state.letterShortcutsEnabled)
        state.windowDidChangeKey(second, isKey: true)
        state.unregisterReadingWindow(first)  // not registered any more: nothing changes
        #expect(state.letterShortcutsEnabled)
        state.unregisterReadingWindow(second)
        #expect(!state.letterShortcutsEnabled)  // no reading window: no letters
    }
}

/// Acceptance finding A: `ressac migrate` leaves a WAL database with no `-wal` or `-shm` file,
/// which a read-only connection cannot open (SQLITE_CANTOPEN, "unable to open database file").
@MainActor @Suite struct FirstRunWALTests {
    /// A schema v2 database in WAL mode, closed so that its `-wal` and `-shm` files are gone.
    func makeClosedWAL(in dir: URL) throws -> URL {
        let url = dir.appending(path: "template.sqlite")
        try FileManager.default.copyItem(at: TestDatabase.fixture, to: url)
        var queue: DatabaseQueue? = try DatabaseQueue(path: url.path)
        let mode = try queue?.writeWithoutTransaction { db in try String.fetchOne(db, sql: "PRAGMA journal_mode = WAL") }
        #expect(mode == "wal")
        queue = nil  // the last connection closes: SQLite removes -wal and -shm, as after `ressac migrate`
        #expect(!FileManager.default.fileExists(atPath: url.path + "-shm"))
        #expect(!FileManager.default.fileExists(atPath: url.path + "-wal"))
        return url
    }

    @Test func aClosedWALDatabaseChecksAndOpens() async throws {
        let test = try TestDatabase()
        let template = try makeClosedWAL(in: test.dir)
        #expect(try AppDatabase.check(at: template) == .ok)
        let watcher = try DataVersionWatcher(url: template)
        try watcher.prime()
    }

    @Test func theFirstRunOpensWhatMigrateCreated() async throws {
        let test = try TestDatabase()
        let template = try makeClosedWAL(in: test.dir)
        let fresh = test.dir.appending(path: "empty.sqlite")
        let stub = try Stub("""
            [ "$4" = migrate ] || exit 2
            cp "\(template.path)" "$2"
            echo '{"event":"finished"}'
            """)
        let launcher = Launcher()
        await launcher.launch(database: fresh, executable: stub.executable)
        if case let .failed(message) = launcher.phase { Issue.record("launch failed: \(message)") }
        #expect(launcher.state != nil)
    }
}

extension FirstRunWALTests {
    @Test func theReadingConnectionsRefuseWrites() throws {
        let test = try TestDatabase()
        let queue = try DatabaseQueue(path: test.url.path, configuration: AppDatabase.readingConfiguration(label: "test"))
        #expect(throws: (any Error).self) {
            try queue.write { try $0.execute(sql: "UPDATE schema_meta SET version = 9") }
        }
        #expect(try AppDatabase.check(at: test.url) == .ok)
    }
}
