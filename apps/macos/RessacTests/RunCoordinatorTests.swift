import Foundation
import Testing

@testable import Ressac

/// Polls a main-actor condition (observations and runs finish asynchronously).
@MainActor
func eventually(timeout: Duration = .seconds(3), _ condition: () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
        if ContinuousClock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return true
}

@MainActor @Suite struct RunCoordinatorTests {
    /// `fetch` sleeps so that it stays active; `extract ID` logs its start and end; `undo` fails.
    let script = """
        log="$(dirname "$0")/log"
        case "$4" in
          fetch)
            echo '{"event":"started","total":2}'
            echo '{"event":"feed_done","feed_id":1,"title":"A","error":null}'
            sleep 1
            echo '{"event":"feed_done","feed_id":2,"title":"B","error":null}'
            echo '{"event":"finished"}' ;;
          extract)
            echo "start $5" >> "$log"; sleep 0.2; echo "end $5" >> "$log"
            echo '{"event":"extracted","entry_id":'"$5"',"ok":false,"error":"HTTP 404."}'
            exit 1 ;;
          garden)
            [ "$5" = undo ] && { echo 'error: nothing to undo here' >&2; exit 1; }
            echo '{"event":"undone","feed_id":null}' ;;
        esac
        """

    @Test func long_run_is_refused_while_another_is_active() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let first = runs.start(.fetch(feeds: []))
        #expect(runs.isLongRunActive)
        #expect(await runs.perform(.fetch(folder: 1)) == .refused)
        #expect(await first.value == .succeeded)
        #expect(!runs.isLongRunActive)
        #expect(await runs.submit(.fetch(feeds: [1])))
    }

    @Test func progress_follows_the_events() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let run = runs.start(.fetch(feeds: []))
        #expect(await eventually { runs.progress == RunProgress(done: 1, total: 2, label: "Fetching feeds") })
        #expect(runs.progress?.fraction == 0.5)
        _ = await run.value
        #expect(runs.progress == nil)
    }

    @Test func short_runs_run_one_at_a_time_in_order() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let tasks = [3, 1, 2].map { runs.start(.extract(entryId: Int64($0))) }
        for task in tasks { _ = await task.value }
        let log = try String(contentsOf: stub.file("log"), encoding: .utf8)
        #expect(log == "start 3\nend 3\nstart 1\nend 1\nstart 2\nend 2\n")
    }

    @Test func short_runs_overlap_a_long_run() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        let fetch = runs.start(.fetch(feeds: []))
        var events: [EngineEvent] = []
        let outcome = await runs.perform(.decide(feedId: 1, .keep)) { events.append($0) }
        #expect(outcome == .succeeded && events == [.undone(feedId: nil)])
        #expect(runs.isLongRunActive)  // the fetch is still sleeping
        _ = await fetch.value
    }

    @Test func failure_shows_a_banner_and_the_next_success_clears_it() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        #expect(await runs.perform(.undo()) == .failed(RunFailure(exitCode: 1, lastStderrLine: "error: nothing to undo here")))
        #expect(runs.banner == Banner(message: "error: nothing to undo here", showLog: true))
        #expect(await runs.submit(.applySafe()))
        #expect(runs.banner == nil)
    }

    @Test func extraction_failure_stays_out_of_the_banner() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        var events: [EngineEvent] = []
        let outcome = await runs.perform(.extract(entryId: 9)) { events.append($0) }
        #expect(outcome == .failed(RunFailure(exitCode: 1, lastStderrLine: nil)))
        #expect(events == [.extracted(entryId: 9, ok: false, error: "HTTP 404.")])
        #expect(runs.banner == nil)
    }

    @Test func network_warning_survives_its_own_successful_run() async throws {
        let stub = try Stub("""
            echo '{"event":"started","total":5}'
            echo '{"event":"network_unreachable","failed":4,"attempted":5}'
            echo '{"event":"finished"}'
            """)
        let runs = RunCoordinator(cli: stub.cli)
        #expect(await runs.submit(.fetch(feeds: [])))
        #expect(runs.banner?.message == "The network looks unreachable: 4 of 5 feeds could not connect.")
    }

    @Test func observations_are_told_after_each_run() async throws {
        let stub = try Stub(script)
        let runs = RunCoordinator(cli: stub.cli)
        var notices = 0
        runs.onDatabaseChange = { notices += 1 }
        _ = await runs.submit(.applySafe())
        _ = await runs.submit(.undo())
        #expect(notices == 2)
    }
}
