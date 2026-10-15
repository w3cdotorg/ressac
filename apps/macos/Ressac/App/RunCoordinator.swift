import Foundation
import Observation
import os

struct RunProgress: Equatable, Sendable {
    var done: Int
    var total: Int
    var label: String

    /// Nil while nothing countable has happened (an indeterminate bar).
    var fraction: Double? {
        total > 0 && done > 0 ? min(1, Double(done) / Double(total)) : nil
    }
}

/// The non-modal banner under the toolbar (spec §8).
struct Banner: Equatable, Sendable {
    let message: String
    let showLog: Bool
}

enum RunOutcome: Equatable, Sendable {
    case succeeded
    /// A long run was asked for while another one was active.
    case refused
    case failed(RunFailure)
}

/// Owns every `ressac` subprocess the app starts: at most one long run, short runs in
/// FIFO order, progress, banners, and change notices for the observations.
@MainActor @Observable final class RunCoordinator {
    private(set) var isLongRunActive = false
    private(set) var progress: RunProgress?
    var banner: Banner?

    /// Called during runs (at most once a second) and after each run, so that GRDB
    /// observations refetch what the subprocess wrote.
    @ObservationIgnored var onDatabaseChange: (@MainActor () -> Void)?

    @ObservationIgnored private let cli: RessacCLI
    @ObservationIgnored private var shortTail: Task<RunOutcome, Never>?
    @ObservationIgnored private var lastChangeNotice = ContinuousClock.now
    @ObservationIgnored private var bannerSetDuringRun = false
    private static let log = Logger(subsystem: "org.w3cdotorg.ressac", category: "runs")

    init(cli: RessacCLI) {
        self.cli = cli
    }

    /// Runs `request` and waits for it. True only when it ran and exited 0; false when it
    /// was refused (a long run is active) or failed (the banner says why).
    func submit(_ request: RunRequest) async -> Bool {
        await start(request).value == .succeeded
    }

    func perform(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)? = nil) async -> RunOutcome {
        await start(request, onEvent: onEvent).value
    }

    /// Queues `request` synchronously (so callers control the order) and returns its task.
    @discardableResult
    func start(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)? = nil) -> Task<RunOutcome, Never> {
        switch request.kind {
        case .long:
            guard !isLongRunActive else {
                Self.log.info("refused \(request.arguments.joined(separator: " "), privacy: .public): a long run is active")
                return Task { .refused }
            }
            isLongRunActive = true
            return Task {
                let outcome = await execute(request, onEvent: onEvent)
                isLongRunActive = false
                progress = nil
                return outcome
            }
        case .short:
            let previous = shortTail
            let task = Task {
                _ = await previous?.value
                return await execute(request, onEvent: onEvent)
            }
            shortTail = task
            return task
        }
    }

    private func execute(_ request: RunRequest, onEvent: (@MainActor (EngineEvent) -> Void)?) async -> RunOutcome {
        if request.kind == .long {
            bannerSetDuringRun = false
        }
        var outcome = RunOutcome.succeeded
        do {
            for try await event in await cli.run(request) {
                if request.kind == .long {
                    apply(event)
                }
                onEvent?(event)
                noteChange(after: event)
            }
        } catch let failure as RunFailure {
            outcome = .failed(failure)
        } catch {
            outcome = .failed(RunFailure(exitCode: 1, lastStderrLine: error.localizedDescription))
        }
        onDatabaseChange?()
        lastChangeNotice = .now
        switch outcome {
        case .succeeded, .refused:
            if !bannerSetDuringRun || request.kind == .short {
                banner = nil
            }
        case let .failed(failure):
            let refused = request.isRefusal(failure)
            if refused {
                Self.log.error("ressac refused \(request.arguments.joined(separator: " "), privacy: .public): \(failure.message, privacy: .public)")
            } else if failure.exitCode == 2 {
                Self.log.fault("usage error from ressac \(request.arguments.joined(separator: " "), privacy: .public)")
            }
            if request.showsFailureBanner {
                // A refusal's sentence says it all: the log holds nothing more.
                banner = Banner(message: failure.message, showLog: !refused)
            }
        }
        return outcome
    }

    private func apply(_ event: EngineEvent) {
        switch event {
        case let .started(total):
            progress = RunProgress(done: 0, total: total, label: "Fetching feeds")
        case .feedDone, .extracted:
            progress?.done += 1
        case let .probing(what, total):
            progress = RunProgress(
                done: 0, total: total, label: what == "full articles" ? "Fetching full articles" : "Checking \(what)")
        case let .networkUnreachable(failed, attempted):
            banner = Banner(
                message: "The network looks unreachable: \(failed) of \(attempted) feeds could not connect.",
                showLog: false)
            bannerSetDuringRun = true
        case .waybackDone, .finished, .decided, .undone, .unknown:
            break
        }
    }

    private func noteChange(after event: EngineEvent) {
        switch event {
        case .feedDone, .extracted, .decided, .undone, .finished:
            if ContinuousClock.now - lastChangeNotice >= .seconds(1) {
                lastChangeNotice = .now
                onDatabaseChange?()
            }
        default:
            break
        }
    }
}
