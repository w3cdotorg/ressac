import Foundation
import GRDB
import Observation

/// The morning edition on the app side (spec §8, Build on launch): whether today's edition is
/// due, the `ressac edition build` run that makes it, and the latest edition for the Journal.
/// The core decides everything else; this object only asks, runs and shows.
@MainActor @Observable final class JournalModel {
    enum Phase: Equatable, Sendable {
        /// Nothing to do: today's edition exists, or no check has run yet.
        case idle
        /// Today's edition is missing and the last attempt was turned away (another long run held
        /// the engine). The next trigger tries again.
        case due
        /// `total` is 0 until the refresh says how many feeds it fetches.
        case building(done: Int, total: Int)
        /// Every feed failed to connect: no edition was written and the window did not move.
        case offline
        /// The run failed; the edition is still due, so the next trigger builds it.
        case failed(String)
    }

    private(set) var phase: Phase = .idle
    /// The latest edition, today's or an older one; nil before the first build.
    private(set) var edition: Edition?

    /// Waits before the next rollover. Tests replace it.
    @ObservationIgnored var sleep: @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }

    @ObservationIgnored private let database: AppDatabase
    @ObservationIgnored private let coordinator: RunCoordinator
    @ObservationIgnored private let clock: @Sendable () -> Date
    @ObservationIgnored let calendar: Calendar
    @ObservationIgnored private var editionObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var rolloverTask: Task<Void, Never>?
    /// A check or a build is under way: launch, activation and the rollover can fire together.
    @ObservationIgnored private var isBusy = false
    @ObservationIgnored private var sawNetworkUnreachable = false
    /// Automatic triggers do not build before this instant: set when a build ends offline or
    /// failed, cleared by a success. In memory only: a relaunch tries again.
    @ObservationIgnored private var automaticRetryNotBefore: Date?

    /// How long automatic triggers leave a failed or offline edition alone.
    static let automaticRetryDelay: TimeInterval = 15 * 60

    init(database: AppDatabase, coordinator: RunCoordinator,
         clock: @escaping @Sendable () -> Date = { Date() }, calendar: Calendar = .autoupdatingCurrent) {
        self.database = database
        self.coordinator = coordinator
        self.clock = clock
        self.calendar = calendar
        editionObservation = Queries.edition().start(
            in: database.reader, scheduling: .immediate,
            onError: { [weak self] error in
                self?.coordinator.banner = Banner(
                    message: "Could not read the database: \(error.localizedDescription)", showLog: true)
            },
            onChange: { [weak self] in self?.edition = $0 })
    }

    /// Builds today's edition when it is due (spec §6, When). Called at launch, when the app
    /// becomes active and at the 04:00 rollover; a call while another one runs does nothing.
    /// After a failed or offline build it waits 15 minutes; `retry()` does not.
    func checkAndBuild() async {
        if let notBefore = automaticRetryNotBefore, clock() < notBefore { return }
        await checkAndBuildNow()
    }

    private func checkAndBuildNow() async {
        guard !isBusy else { return }
        isBusy = true
        defer { isBusy = false }
        let dayStart = EditionClock.dayStart(for: clock(), calendar: calendar)
        do {
            guard try await database.reader.read(Queries.isEditionDue(dayStart: dayStart)) else {
                phase = .idle
                return
            }
        } catch {
            phase = .failed("Could not read the database: \(error.localizedDescription)")
            return
        }
        await build()
    }

    /// The Retry button of the offline and failed states: the same check, so that an edition
    /// built meanwhile in a terminal is not built twice.
    func retry() async {
        await checkAndBuildNow()
    }

    /// The next 04:00 local after `now`. Through the calendar, so that a DST night keeps 04:00.
    static func nextRollover(after now: Date, calendar: Calendar) -> Date {
        let start = EditionClock.dayStart(for: now, calendar: calendar)
        return calendar.date(byAdding: .day, value: 1, to: start) ?? start.addingTimeInterval(86_400)
    }

    /// Waits for the next 04:00, checks, and schedules the one after. `Task.sleep` counts on the
    /// continuous clock, so a Mac asleep at 04:00 checks as soon as it wakes.
    func scheduleRollover() {
        rolloverTask?.cancel()
        let now = clock()
        let wait = max(1, Self.nextRollover(after: now, calendar: calendar).timeIntervalSince(now))
        rolloverTask = Task { [weak self, sleep] in
            do {
                try await sleep(.seconds(wait))
            } catch {
                return
            }
            guard let self, !Task.isCancelled else { return }
            await checkAndBuild()
            scheduleRollover()
        }
    }

    func cancelRollover() {
        rolloverTask?.cancel()
        rolloverTask = nil
    }

    private func build() async {
        phase = .building(done: 0, total: 0)
        sawNetworkUnreachable = false
        let outcome = await coordinator.perform(.buildEdition()) { [weak self] event in
            self?.note(event)
        }
        defer {
            switch phase {
            case .offline, .failed: automaticRetryNotBefore = clock().addingTimeInterval(Self.automaticRetryDelay)
            default: automaticRetryNotBefore = nil
            }
        }
        switch outcome {
        case .succeeded:
            phase = .idle
        case .refused:
            phase = .due
        case let .failed(failure) where failure.exitCode == 2:
            // A refusal: every fetch failed, or another process wrote today's edition meanwhile
            // (spec §9), which the live query now shows. The latter is not news: the Journal
            // simply reloads, so the coordinator's refusal banner is taken down.
            if sawNetworkUnreachable {
                phase = .offline
            } else {
                phase = .idle
                if failure.message == Self.builtElsewhere, coordinator.banner?.message == Self.builtElsewhere {
                    coordinator.banner = nil
                }
            }
        case let .failed(failure):
            phase = .failed(failure.message)
        }
    }

    /// `ressac edition build`'s refusal when an edition already exists since 04:00 (core `Error::EditionNotDue`).
    static let builtElsewhere = "An edition was already built this morning."

    private func note(_ event: EngineEvent) {
        switch event {
        case let .started(total):
            phase = .building(done: 0, total: total)
        case .feedDone:
            if case let .building(done, total) = phase { phase = .building(done: done + 1, total: total) }
        case .networkUnreachable:
            sawNetworkUnreachable = true
        default:
            break
        }
    }
}
