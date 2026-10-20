import AppKit
import Foundation
import GRDB
import Testing

@testable import Ressac

@MainActor @Suite struct ExternalChangeTests {
    @Test func theWatcherSeesOnlyOtherConnectionsCommits() throws {
        let test = try TestDatabase()
        let watcher = try DataVersionWatcher(url: test.url)
        try watcher.prime()
        #expect(try !watcher.hasChanged())
        try test.entry(feed: 1)  // the second connection, standing in for a terminal ressac
        #expect(try watcher.hasChanged())
        #expect(try !watcher.hasChanged())
        try test.execute("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z'")
        #expect(try watcher.hasChanged())
    }

    @Test func aPollRefreshesTheLiveQueriesAfterATerminalWrite() async throws {
        let test = try TestDatabase()
        let database = try test.open()
        let runs = RunCoordinator(cli: RessacCLI(executable: try Stub("exit 0").executable, database: test.url))
        let state = AppState(database: database, runs: runs)
        let monitor = ExternalChangeMonitor(database: database, shouldPoll: { true })
        try await Task.sleep(for: .milliseconds(50))
        #expect(await !monitor.poll())  // first poll: baseline only
        #expect(state.sidebar.unread == 0)
        try test.entry(feed: 1)
        #expect(await monitor.poll())
        #expect(await eventually { state.sidebar.unread == 1 })
        #expect(await !monitor.poll())
    }

    /// The Sea Chart on a second display stays live while another app is in front (spec §8):
    /// in the background, the app's monitor polls only while a chart window is on screen.
    @Test func inTheBackgroundTheMonitorPollsOnlyWhileAChartIsOnScreen() async throws {
        let (state, test, _) = try makeState()
        try await Task.sleep(for: .milliseconds(100))  // let GRDB's observation start before the insert
        state.isAppActive = { false }
        let monitor = state.makeExternalChangeMonitor(interval: .milliseconds(40))
        monitor.start()
        defer { monitor.stop() }
        try test.entry(feed: 1)
        try await Task.sleep(for: .milliseconds(300))
        #expect(state.sidebar.unread == 0)  // in the background with no chart: not polled
        state.isChartOnScreen = true
        #expect(await eventually { state.sidebar.unread == 1 })
        state.isChartOnScreen = false
        try test.entry(feed: 1, url: "https://example.org/second")
        try await Task.sleep(for: .milliseconds(300))
        #expect(state.sidebar.unread == 1)
        state.isAppActive = { true }
        #expect(await eventually { state.sidebar.unread == 2 })
    }
}

@MainActor @Suite struct FixWaveStateTests {
    @Test func theFullTextNoticeNeverLandsOnAnotherEntry() async throws {
        let (state, test, _) = try makeState(stub: """
            sleep 0.4
            echo '{"event":"extracted","entry_id":'"$5"',"ok":false,"error":"Behind a login."}'
            """)
        let first = try test.entry(feed: 1, url: "https://example.org/a", published: "2026-10-02T08:00:00.000Z")
        let second = try test.entry(feed: 1, url: "https://example.org/b", published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.selectedEntryId = first
        let run = try #require(state.requestFullText())
        state.selectedEntryId = second
        await run.value
        #expect(state.fullTextNotice == nil)
    }

    @Test func todayCountsEveryEntryOfTheDayAndRestartsAtMidnight() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 1, published: "2026-10-10T08:00:00.000Z", read: true)
        try test.entry(feed: 1, published: "2026-10-10T08:30:00.000Z")
        try test.entry(feed: 1, published: "2026-10-08T08:30:00.000Z")
        state.dayDidChange()  // the sidebar started before the test pinned `now`
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.sidebar.today == 2 })  // read and unread, from 2026-10-10 (local midnight)
        var calendar = Calendar.current
        calendar.timeZone = .current
        state.now = { calendar.date(byAdding: .day, value: 1, to: Timestamp.date(from: "2026-10-10T09:00:00.000Z")!)! }
        state.dayDidChange()
        #expect(await eventually { state.sidebar.today == 0 })
    }

    @Test func letterShortcutsFollowTheKeyWindow() {
        let state = try? makeState().0
        let reading = NSWindow()
        let panel = NSPanel()
        guard let state else { Issue.record("no state"); return }
        #expect(state.letterShortcutsEnabled)
        state.registerReadingWindow(reading)
        state.windowDidChangeKey(panel, isKey: true)
        #expect(!state.letterShortcutsEnabled)
        state.windowDidChangeKey(reading, isKey: true)
        #expect(state.letterShortcutsEnabled)
        state.windowDidChangeKey(reading, isKey: false)
        #expect(!state.letterShortcutsEnabled)
    }
}

@Suite struct SanitiseTimeoutTests {
    @Test func aTimedOutSanitiseIsNotMemoised() {
        let html = String(repeating: "<p>budget \(UUID().uuidString) <b>bold</b></p>", count: 3000)
        let rushed = ArticleTemplate.sanitize(html, budget: 0)
        #expect(!rushed.contains("<b>bold</b>"))  // the plain-text fallback
        let before = ArticleTemplate.cacheStats()
        let patient = ArticleTemplate.sanitize(html)
        #expect(patient.contains("<b>bold</b>"))  // parsed for real: the fallback was not remembered
        #expect(ArticleTemplate.cacheStats().misses >= before.misses + 1)
        #expect(ArticleTemplate.sanitize(html) == patient)  // and a real result is
    }
}
