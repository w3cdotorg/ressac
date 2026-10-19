import Foundation
import Observation
import os

/// Produces the sea chart document: one JSON object (`ressac chart --json`).
protocol ChartRunner: Sendable {
    func chartJSON() async throws -> Data
}

/// The bundled helper (spec §8): a plain run that captures stdout, not the event stream.
struct CLIChartRunner: ChartRunner {
    let cli: RessacCLI

    func chartJSON() async throws -> Data {
        try await cli.capture(["chart"])
    }
}

/// Runs `ressac chart`, checks the document's `version`, and hands the raw JSON to the page
/// (spec §8). Reruns on window open and on database changes, at most once per `debounce`.
@MainActor @Observable final class ChartModel {
    enum Phase: Equatable {
        case loading
        case ready(json: String)
        case failed(String)
    }

    /// The only document version this build draws.
    static let supportedVersion = 1

    private(set) var phase: Phase = .loading
    /// Why the last run failed, whatever the phase: a failed refresh keeps the last chart on
    /// screen, so this is where the failure stays visible. Nil after a good run.
    private(set) var lastRefreshError: String?

    @ObservationIgnored private let runner: any ChartRunner
    @ObservationIgnored private let debounce: Duration
    @ObservationIgnored private var run: Task<Void, Never>?
    @ObservationIgnored private var scheduled: Task<Void, Never>?
    /// A refresh was asked for while one was running: it reruns once that one ends.
    @ObservationIgnored private var rerunAfterRun = false

    private static let log = Logger(subsystem: "org.w3cdotorg.ressac", category: "chart")

    init(runner: any ChartRunner, debounce: Duration = .seconds(2)) {
        self.runner = runner
        self.debounce = debounce
    }

    /// Runs now (window open, Retry). Asked during a run, it follows that run.
    func refresh() {
        guard run == nil else {
            rerunAfterRun = true
            return
        }
        run = Task {
            await load()
            guard !Task.isCancelled else { return }
            run = nil
            if rerunAfterRun {
                rerunAfterRun = false
                refresh()
            }
        }
    }

    /// The database changed. Changes within one `debounce` window share a single run, started
    /// at the end of the window: a steady trickle of writes (a fetch) still draws once per window.
    func noteChange() {
        guard scheduled == nil else { return }
        scheduled = Task {
            try? await Task.sleep(for: debounce)
            guard !Task.isCancelled else { return }
            scheduled = nil
            refresh()
        }
    }

    /// The window is open (spec §8): draws now, then notes each later change of `changes` (the
    /// value at the call is not a change) until the calling task is cancelled, which happens when
    /// the window closes; the scheduled and running work is then cancelled too.
    func follow(_ changes: @escaping @MainActor @Sendable () -> Int) async {
        var seen = changes()
        refresh()
        for await tick in Observations(changes) where tick != seen {
            seen = tick
            noteChange()
        }
        cancel()
    }

    /// Drops the scheduled run and the running one; their results never reach `phase`.
    func cancel() {
        scheduled?.cancel()
        scheduled = nil
        run?.cancel()
        run = nil
        rerunAfterRun = false
    }

    private func load() async {
        do {
            let data = try await runner.chartJSON()
            guard !Task.isCancelled else { return }
            let version = try JSONDecoder().decode(Versioned.self, from: data).version
            guard version == Self.supportedVersion else {
                throw ChartError.unsupportedVersion(version)
            }
            lastRefreshError = nil
            phase = .ready(json: String(decoding: data, as: UTF8.self))
        } catch {
            guard !Task.isCancelled else { return }
            let message = Self.message(for: error)
            Self.log.error("chart refresh failed: \(message, privacy: .public)")
            lastRefreshError = message
            // A failed refresh keeps the last chart on screen.
            if case .ready = phase { return }
            phase = .failed(message)
        }
    }

    private struct Versioned: Decodable {
        let version: Int
    }

    private enum ChartError: Error {
        case unsupportedVersion(Int)
    }

    private static func message(for error: any Error) -> String {
        switch error {
        case let failure as RunFailure: failure.message
        case ChartError.unsupportedVersion(let version):
            "This build draws chart version \(supportedVersion), but ressac produced version \(version)."
        case is DecodingError: "ressac printed a chart this app cannot read."
        default: error.localizedDescription
        }
    }
}
