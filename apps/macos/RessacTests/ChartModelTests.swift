import Foundation
import Synchronization
import Testing

@testable import Ressac

/// A chart runner that replays scripted results and counts its runs.
final class ScriptedChartRunner: ChartRunner {
    private struct State {
        var results: [Result<Data, any Error>]
        var runs = 0
    }
    private let state: Mutex<State>

    /// The last result repeats once the script is used up.
    init(_ results: [Result<Data, any Error>]) { state = Mutex(State(results: results)) }

    var runs: Int { state.withLock { $0.runs } }

    func chartJSON() async throws -> Data {
        let result = state.withLock { state -> Result<Data, any Error> in
            state.runs += 1
            return state.results.count > 1 ? state.results.removeFirst() : state.results[0]
        }
        return try result.get()
    }
}

private func chart(version: Int) -> Result<Data, any Error> {
    .success(Data(#"{"version":\#(version),"feeds":[]}"#.utf8))
}

@MainActor @Suite struct ChartModelTests {
    @Test func decodes_version_1() async {
        let model = ChartModel(runner: ScriptedChartRunner([chart(version: 1)]), debounce: .milliseconds(50))
        #expect(model.phase == .loading)
        model.refresh()
        #expect(await eventually { model.phase == .ready(json: #"{"version":1,"feeds":[]}"#) })
    }

    @Test func refuses_other_versions() async {
        let model = ChartModel(runner: ScriptedChartRunner([chart(version: 2)]), debounce: .milliseconds(50))
        model.refresh()
        #expect(await eventually { if case .failed = model.phase { true } else { false } })
        guard case let .failed(message) = model.phase else { return }
        #expect(message.contains("2"))
    }

    @Test func rejects_unreadable_output() async {
        let model = ChartModel(runner: ScriptedChartRunner([.success(Data("nope".utf8))]), debounce: .milliseconds(50))
        model.refresh()
        #expect(await eventually { if case .failed = model.phase { true } else { false } })
    }

    @Test func a_failed_run_shows_the_cli_message() async {
        let failure = RunFailure(exitCode: 1, lastStderrLine: "database is locked")
        let model = ChartModel(runner: ScriptedChartRunner([.failure(failure)]), debounce: .milliseconds(50))
        model.refresh()
        #expect(await eventually { model.phase == .failed("database is locked") })
    }

    @Test func debounces_bursts() async throws {
        let runner = ScriptedChartRunner([chart(version: 1)])
        let model = ChartModel(runner: runner, debounce: .milliseconds(300))
        for _ in 0..<5 {
            model.noteChange()
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(await eventually { runner.runs == 1 })
        try await Task.sleep(for: .milliseconds(500))
        #expect(runner.runs == 1)
        // A later change starts a new window.
        model.noteChange()
        #expect(await eventually { runner.runs == 2 })
    }

    @Test func a_change_during_a_run_is_not_lost() async throws {
        let runner = ScriptedChartRunner([chart(version: 1)])
        let model = ChartModel(runner: runner, debounce: .milliseconds(20))
        model.refresh()
        model.noteChange()
        #expect(await eventually { runner.runs == 2 })
    }

    @Test func failed_refresh_keeps_last_chart() async {
        let failure = RunFailure(exitCode: 1, lastStderrLine: "boom")
        let runner = ScriptedChartRunner([chart(version: 1), .failure(failure)])
        let model = ChartModel(runner: runner, debounce: .milliseconds(50))
        model.refresh()
        #expect(await eventually { model.phase == .ready(json: #"{"version":1,"feeds":[]}"#) })
        model.refresh()
        #expect(await eventually { model.lastRefreshError == "boom" })
        #expect(model.phase == .ready(json: #"{"version":1,"feeds":[]}"#))
    }
}

@Suite struct RessacCLICaptureTests {
    @Test func capture_returns_stdout_and_passes_the_prefix() async throws {
        let stub = try Stub(#"printf '{"version":1,"args":"%s"}' "$*""#)
        let data = try await stub.cli.capture(["chart"])
        let text = String(decoding: data, as: UTF8.self)
        #expect(text == #"{"version":1,"args":"--db \#(stub.database.path) --json chart"}"#)
    }

    @Test func capture_throws_run_failure_with_last_stderr_line() async throws {
        let stub = try Stub("echo first >&2; echo 'no chart today' >&2; exit 3")
        do {
            _ = try await stub.cli.capture(["chart"])
            Issue.record("expected a failure")
        } catch let failure as RunFailure {
            #expect(failure == RunFailure(exitCode: 3, lastStderrLine: "no chart today"))
        }
    }

    @Test func capture_reads_output_larger_than_a_pipe_buffer() async throws {
        let stub = try Stub("head -c 300000 /dev/zero | tr '\\0' 'x'")
        let data = try await stub.cli.capture(["chart"])
        #expect(data.count == 300_000)
    }
}

@MainActor @Suite struct ChangeTickTests {
    @Test func external_changes_bump_the_tick() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 1)
        let before = state.changeTick
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.changeTick > before })
    }

    @Test func the_apps_own_writes_bump_the_tick() async throws {
        let (state, test, _) = try makeState()
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        let before = state.changeTick
        try await state.writes.setRead([id], read: true, at: Date())
        #expect(await eventually { state.changeTick > before })
    }

    @Test func a_finished_run_bumps_the_tick() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        try await Task.sleep(for: .milliseconds(100))
        let before = state.changeTick
        _ = await state.refresh(feedId: 1).value
        #expect(await eventually { state.changeTick > before })
    }
}
