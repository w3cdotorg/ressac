import Foundation
import GRDB
import Testing

@testable import Ressac

extension Calendar {
    /// The tests' timezone: Europe/Paris, with its DST changes on 2026-03-29 and 2026-10-25.
    static let paris: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Paris")!
        return calendar
    }()
}

/// A stand-in for `ressac edition build`, steered by flag files next to it: `offline` (every
/// fetch failed, exit 2), `built` (an edition was written meanwhile, exit 2), `fail` (exit 1),
/// `kill` (killed by a signal), `slow` (one second before anything). Otherwise it emits a short
/// refresh, writes an edition built at 09:30:05 Paris time with sqlite3, and emits `edition`.
/// `fetch` sleeps one second, so that a test can hold the long-run slot.
let editionStub = #"""
    d="$(dirname "$0")"
    echo "$*" >> "$d/calls"
    case "$4" in
      fetch) sleep 1; exit 0 ;;
      edition) ;;
      *) exit 0 ;;
    esac
    [ -f "$d/slow" ] && sleep 1
    [ -f "$d/fail" ] && { echo 'error: disk full' >&2; exit 1; }
    [ -f "$d/kill" ] && kill -TERM $$
    [ -f "$d/built" ] && { echo 'An edition was already built this morning.' >&2; exit 2; }
    if [ -f "$d/offline" ]; then
      echo '{"event":"network_unreachable","failed":11,"attempted":11}'
      echo 'The network is unreachable: no edition built.' >&2
      exit 2
    fi
    echo '{"event":"started","total":3}'
    echo '{"event":"feed_done","feed_id":1,"title":"A","error":null}'
    echo '{"event":"feed_done","feed_id":2,"title":"B","error":null}'
    [ -f "$d/hold" ] && sleep 1
    /usr/bin/sqlite3 -cmd ".timeout 5000" "$2" "INSERT INTO edition (built_at, window_start, window_end, unreachable) VALUES ('2026-10-16T07:30:05.000Z', '2026-10-15T07:30:00.000Z', '2026-10-16T07:30:05.000Z', 0);"
    echo '{"event":"edition","id":1,"entries":0,"front":0,"week":0,"wire":0,"overflow":0,"unreachable":0}'
    """#

func flag(_ stub: Stub, _ name: String) throws {
    try Data().write(to: stub.file(name))
}

func unflag(_ stub: Stub, _ name: String) throws {
    try FileManager.default.removeItem(at: stub.file(name))
}

/// How many times the stub was asked to build an edition.
func builds(_ stub: Stub) throws -> Int {
    guard FileManager.default.fileExists(atPath: stub.file("calls").path) else { return 0 }
    return try calls(stub).filter { $0.hasSuffix("edition build") }.count
}

/// Inserts an edition the way the core writes it. `items`: (entry id, section, rank).
func seedEdition(_ test: TestDatabase, id: Int64 = 1, builtAt: String = "2026-10-16T05:00:00.000Z",
                 items: [(Int64, String, Int)] = [], overflow: [(feed: Int64, section: String, hidden: Int)] = [],
                 unreachable: Int = 0) throws {
    try test.execute("""
        INSERT INTO edition (id, built_at, window_start, window_end, unreachable)
        VALUES (?, ?, '2026-10-15T05:00:00.000Z', ?, ?)
        """, [id, builtAt, builtAt, unreachable])
    for (entry, section, rank) in items {
        try test.execute("INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (?, ?, ?, ?)",
                         [id, entry, section, rank])
    }
    for item in overflow {
        try test.execute("INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (?, ?, ?, ?)",
                         [id, item.feed, item.section, item.hidden])
    }
}

/// A `JournalModel` over a fixture copy whose edition tables are empty, at `instant` (UTC),
/// in Paris time, with `editionStub` as `ressac`. The `AppState` wires the change notices.
@MainActor
func makeJournal(at instant: String = "2026-10-16T07:30:00.000Z") throws -> (JournalModel, AppState, TestDatabase, Stub) {
    let (state, test, stub) = try makeState(stub: editionStub)
    try test.execute("DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition;")
    let now = Timestamp.date(from: instant)!
    let journal = JournalModel(database: state.database, coordinator: state.runs, clock: { now }, calendar: .paris)
    return (journal, state, test, stub)
}

/// Records the rollover waits; every wait after the first lasts a minute (the test cancels it).
actor SleepLog {
    private(set) var waits: [Duration] = []

    func wait(_ duration: Duration) async throws {
        waits.append(duration)
        if waits.count > 1 { try await Task.sleep(for: .seconds(60)) }
    }
}

@MainActor @Suite struct JournalModelTests {
    @Test func a_due_edition_is_built_and_shown() async throws {
        let (journal, _, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        #expect(journal.edition == nil)
        await journal.checkAndBuild()
        #expect(try builds(stub) == 1)
        #expect(journal.phase == .idle)
        #expect(await eventually { journal.edition?.info.builtAt == Timestamp.date(from: "2026-10-16T07:30:05.000Z") })
    }

    @Test func an_edition_built_after_four_is_not_due() async throws {
        let (journal, _, test, stub) = try makeJournal()
        try seedEdition(test, builtAt: "2026-10-16T02:30:00.000Z")  // 04:30 in Paris
        await journal.checkAndBuild()
        #expect(try builds(stub) == 0)
        #expect(journal.phase == .idle)
    }

    @Test func an_edition_built_before_four_belongs_to_yesterday() async throws {
        let (journal, _, test, stub) = try makeJournal()
        try seedEdition(test, builtAt: "2026-10-16T01:30:00.000Z")  // 03:30 in Paris
        await journal.checkAndBuild()
        #expect(try builds(stub) == 1)
    }

    @Test func before_four_the_day_is_still_yesterday() async throws {
        // 03:30 in Paris: yesterday's 07:00 edition is today's.
        let (journal, _, test, stub) = try makeJournal(at: "2026-10-16T01:30:00.000Z")
        try seedEdition(test, builtAt: "2026-10-15T05:00:00.000Z")
        await journal.checkAndBuild()
        #expect(try builds(stub) == 0)
    }

    @Test func building_reports_its_progress() async throws {
        let (journal, state, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        try flag(stub, "hold")
        let run = Task { await journal.checkAndBuild() }
        #expect(await eventually { journal.phase == .building(done: 2, total: 3) })
        #expect(state.runs.progress?.label == RunCoordinator.editionProgressLabel)
        #expect(RunCoordinator.editionProgressLabel == "Building this morning's edition…")
        await run.value
        #expect(journal.phase == .idle)
        #expect(state.runs.progress == nil)
    }

    @Test func offline_keeps_the_edition_due_and_retry_builds_it() async throws {
        let (journal, _, test, stub) = try makeJournal()
        try flag(stub, "offline")
        await journal.checkAndBuild()
        #expect(journal.phase == .offline)
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM edition") == 0)
        try unflag(stub, "offline")
        await journal.retry()
        #expect(journal.phase == .idle)
        #expect(try builds(stub) == 2)
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM edition") == 1)
    }

    @Test func an_edition_built_meanwhile_elsewhere_is_simply_shown() async throws {
        let (journal, state, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        try flag(stub, "built")
        await journal.checkAndBuild()
        #expect(journal.phase == .idle)
        // Spec §9: the Journal simply reloads; no banner for a refusal that is not news.
        #expect(state.runs.banner == nil)
    }

    @Test func an_offline_refusal_keeps_its_banner() async throws {
        let (journal, state, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        try flag(stub, "offline")
        await journal.checkAndBuild()
        #expect(journal.phase == .offline)
        #expect(state.runs.banner == Banner(message: "The network is unreachable: no edition built.", showLog: false))
    }

    /// Review Focus 4: a failed or killed build writes nothing; the edition stays due, and the
    /// next trigger (or a relaunch) builds it.
    @Test(arguments: ["fail", "kill"])
    func a_failed_or_killed_build_leaves_the_edition_due(_ failure: String) async throws {
        let (journal, state, test, stub) = try makeJournal()
        try flag(stub, failure)
        await journal.checkAndBuild()
        guard case let .failed(message) = journal.phase else {
            Issue.record("expected a failure, got \(journal.phase)")
            return
        }
        #expect(failure == "kill" ? message.contains("signal") : message == "error: disk full")
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM edition") == 0)

        try unflag(stub, failure)
        let now = Timestamp.date(from: "2026-10-16T07:31:00.000Z")!
        let relaunched = JournalModel(database: state.database, coordinator: state.runs, clock: { now }, calendar: .paris)
        await relaunched.checkAndBuild()
        #expect(relaunched.phase == .idle)
        #expect(try builds(stub) == 2)
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM edition") == 1)
    }

    @Test func a_build_turned_away_by_another_long_run_stays_due() async throws {
        let (journal, state, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        let fetch = state.runs.start(.fetch(feeds: []))
        await journal.checkAndBuild()
        #expect(journal.phase == .due)
        #expect(try builds(stub) == 0)
        _ = await fetch.value
        await journal.checkAndBuild()
        #expect(journal.phase == .idle)
        #expect(try builds(stub) == 1)
    }

    @Test func a_second_trigger_during_a_build_does_nothing() async throws {
        let (journal, _, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        try flag(stub, "slow")
        async let first: Void = journal.checkAndBuild()
        async let second: Void = journal.checkAndBuild()
        _ = await (first, second)
        await journal.checkAndBuild()  // after the build: not due any more
        #expect(try builds(stub) == 1)
    }

    @Test func the_rollover_is_four_in_the_morning_local_time_across_dst() {
        func next(_ utc: String) -> Date {
            JournalModel.nextRollover(after: Timestamp.date(from: utc)!, calendar: .paris)
        }
        // Summer time: 04:00 Paris is 02:00 UTC.
        #expect(next("2026-10-16T07:30:00.000Z") == Timestamp.date(from: "2026-10-17T02:00:00.000Z"))
        #expect(next("2026-10-16T01:30:00.000Z") == Timestamp.date(from: "2026-10-16T02:00:00.000Z"))
        // The night of 25 October (03:00 → 02:00): the next 04:00 is in winter time, 03:00 UTC.
        #expect(next("2026-10-24T22:00:00.000Z") == Timestamp.date(from: "2026-10-25T03:00:00.000Z"))
        // The night of 29 March (02:00 → 03:00): 04:00 Paris is 02:00 UTC again.
        #expect(next("2026-03-28T23:30:00.000Z") == Timestamp.date(from: "2026-03-29T02:00:00.000Z"))
    }

    @Test func the_rollover_waits_until_four_then_builds() async throws {
        let (journal, _, test, stub) = try makeJournal()
        defer { withExtendedLifetime(test) {} }  // the temporary database goes with it
        let log = SleepLog()
        journal.sleep = { try await log.wait($0) }
        journal.scheduleRollover()
        #expect(await eventually { (try? builds(stub)) == 1 })
        journal.cancelRollover()
        let waits = await log.waits
        // 09:30 Paris → 04:00 the next day: 18 h 30.
        let expected: Duration = .seconds(66_600)
        #expect(waits.first == expected)
    }
}
