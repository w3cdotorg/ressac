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

    /// An address clash refuses a garden decision or an undo with exit 2: the banner shows
    /// the CLI's sentence, without Show log. Exit 2 from a call the app builds alone is still
    /// a bug, with the log button.
    @Test func a_refused_garden_decision_shows_its_sentence_without_the_log_button() async throws {
        let stub = try Stub("""
            case "$4 $5" in
              "garden decide") echo 'Feed #2 already uses https://e.org/feed; merge into it instead.' >&2 ;;
              "garden undo") echo 'Feed #10 already uses https://e.org/feed; cannot undo.' >&2 ;;
              *) echo 'No feed #9.' >&2 ;;
            esac
            exit 2
            """)
        let runs = RunCoordinator(cli: stub.cli)
        let clash = "Feed #2 already uses https://e.org/feed; merge into it instead."
        #expect(await runs.perform(.decide(feedId: 3, .repair)) == .failed(RunFailure(exitCode: 2, lastStderrLine: clash)))
        #expect(runs.banner == Banner(message: clash, showLog: false))
        let undo = "Feed #10 already uses https://e.org/feed; cannot undo."
        #expect(await runs.perform(.undo()) == .failed(RunFailure(exitCode: 2, lastStderrLine: undo)))
        #expect(runs.banner == Banner(message: undo, showLog: false))
        #expect(await runs.perform(.fetch(feeds: [9])) == .failed(RunFailure(exitCode: 2, lastStderrLine: "No feed #9.")))
        #expect(runs.banner == Banner(message: "No feed #9.", showLog: true))
    }

    @Test func only_garden_edition_and_cadence_runs_can_be_refused() {
        let exitTwo = RunFailure(exitCode: 2, lastStderrLine: "Feed #10 already uses https://e.org/feed; cannot restore.")
        #expect(RunRequest.decide(feedId: 9, .keep).isRefusal(exitTwo))
        #expect(RunRequest.undo().isRefusal(exitTwo))
        #expect(RunRequest.buildEdition().isRefusal(exitTwo))
        #expect(RunRequest.buildEdition(rebuild: true).isRefusal(exitTwo))
        #expect(RunRequest.setCadence(feedId: 9, .rare).isRefusal(exitTwo))
        #expect(RunRequest.setCadence(feedId: 9, nil).isRefusal(exitTwo))
        #expect(!RunRequest.applySafe().isRefusal(exitTwo))
        #expect(!RunRequest.fetch(feeds: [9]).isRefusal(exitTwo))
        #expect(!RunRequest.migrate().isRefusal(exitTwo))
        #expect(!RunRequest.decide(feedId: 9, .keep).isRefusal(RunFailure(exitCode: 1, lastStderrLine: "error: disk I/O error")))
        #expect(!RunRequest.buildEdition().isRefusal(RunFailure(exitCode: 1, lastStderrLine: "error: disk I/O error")))
    }

    /// Spec §9: offline and "already built" refusals of `edition build`, and an override on a
    /// buried feed, show the CLI's sentence without Show log.
    @Test func edition_and_cadence_refusals_show_their_sentence_without_the_log_button() async throws {
        let stub = try Stub("""
            case "$4 $5" in
              "edition build")
                echo '{"event":"started","total":2}'
                echo '{"event":"network_unreachable","failed":2,"attempted":2}'
                echo 'The network is unreachable: no edition built.' >&2 ;;
              "feed cadence") echo 'Feed #11 is buried.' >&2 ;;
            esac
            exit 2
            """)
        let runs = RunCoordinator(cli: stub.cli)
        let offline = "The network is unreachable: no edition built."
        #expect(await runs.perform(.buildEdition()) == .failed(RunFailure(exitCode: 2, lastStderrLine: offline)))
        #expect(runs.banner == Banner(message: offline, showLog: false))
        #expect(!runs.isLongRunActive)
        let buried = "Feed #11 is buried."
        #expect(await runs.perform(.setCadence(feedId: 11, .daily)) == .failed(RunFailure(exitCode: 2, lastStderrLine: buried)))
        #expect(runs.banner == Banner(message: buried, showLog: false))
    }

    /// The edition event reaches the caller; the run counts feeds like a fetch and ends cleanly.
    @Test func an_edition_build_reports_its_edition_and_tells_the_observations() async throws {
        let stub = try Stub("""
            echo '{"event":"started","total":1}'
            echo '{"event":"feed_done","feed_id":1,"title":"A","error":null}'
            echo '{"event":"finished"}'
            echo '{"event":"edition","id":4,"entries":2,"front":1,"week":0,"wire":1,"overflow":5,"unreachable":0}'
            """)
        let runs = RunCoordinator(cli: stub.cli)
        var notices = 0
        runs.onDatabaseChange = { notices += 1 }
        var events: [EngineEvent] = []
        #expect(await runs.perform(.buildEdition()) { events.append($0) } == .succeeded)
        #expect(events.last == .edition(id: 4, entries: 2, overflow: 5, unreachable: 0))
        #expect(runs.progress == nil && runs.banner == nil && !runs.isLongRunActive)
        #expect(notices >= 1)
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
