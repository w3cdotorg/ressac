# Phase 5: The Native app (Journal, Rhythm, inspector and garden)

**Runs after:** phase 4b (5a), then 5b, then 5c, in that order.

Read the plan index first (`docs/plans/2026-10-16-cadence-edition.md`). Global Constraints apply. This phase implements spec §8 "Native" and "Data layer" (the `JournalModel` part), the app rows of §9 (offline, two builds at once, app quit during the build), and the Swift items of §10 for the Journal model, the sidebar, the edition due check, the launch flow and the inspector menu. Review Focus 4 belongs to 5a.

**Goal of the phase:** at launch, on activation and at 04:00, the app builds the morning edition when it is due and shows its progress; the sidebar gains Journal (top) and a Rhythm section; the Journal lists the edition by section with overflow rows, a header, an end line and an empty line, and `⇧⌘A` and Space stay inside it; the garden inspector shows a rhythm line with an override menu and Garden › All feeds can be grouped by rhythm.

**Builds on (phase 4, exact names from the plan index):**
- 4a: `CadenceGroup` (`rawValue` `daily`…`silent`, `title`, `page`, `CaseIterable`, `Hashable`), `EditionSection` (`front`/`week`/`wire`, `title`, `antiopeTitle`, `CaseIterable`), `EditionInfo`, `EditionItem`, `EditionOverflowItem`, `Edition` (`info`, `items`, `overflow`, `readCount`, `isFinished`), `CadenceRow` (`feedId`, `effective`, `computed`, `isOverride`, `medianIntervalDays`, `entriesPerMonth`, `lastEntryAt`, `unread`), `Queries.edition() -> LiveQuery<Edition?>`, `Queries.cadenceRows() -> LiveQuery<[CadenceRow]>`, `Queries.isEditionDue(dayStart:) -> @Sendable (Database) throws -> Bool`, `EditionClock.dayStart(for:calendar:)`, `AppDatabase.knownSchemaVersion == 4`. The four edition structs and `CadenceRow` are plain structs with their memberwise initialisers (the tests below build them directly).
- 4b: `RunRequest.buildEdition(rebuild:)` (`["edition", "build"]`, `.long`), `RunRequest.setCadence(feedId:_:)` (`["feed", "cadence", id, group?.rawValue ?? "auto"]`, `.short`), `EngineEvent.edition(...)`, `EngineEvent.cadence(...)`; exit `2` from `edition build` is a refusal (banner without Show Log).
- Schema v4 (phase 1b): tables `edition`, `edition_entry`, `edition_overflow`, columns `feed.cadence_override`, `feed_health.cadence_group`, view `v_cadence`.

**Existing code used as is:** `AppState` (`selection`, `entries`, `selectedEntryId`, `markAllRead()`, `selectNextUnread()`, `refreshCurrent()`, `runs`, `database`, `now`, `report`), `RunCoordinator` (`perform(_:onEvent:)`, `start(_:onEvent:)`, `progress`, `banner`, `onDatabaseChange`), `RunOutcome`, `RunFailure` (`exitCode`, `message`), `Banner`, `EngineEvent` (`started`, `feedDone`, `networkUnreachable`), `Queries` (`predicate(for:now:)`, `sidebar(now:)`, `entries`, `search`, `floor`, `nextUnread`), `Writes.markAllRead(in:at:)` (goes through `Queries.predicate`, so it needs no change), `EntryRowView` and `EntryListView` (`MainView.swift`), `GardenModel`, `GardenRow`, `GardenInspector`, `GardenTable`. Test helpers: `makeState(stub:)`, `calls(_:)`, `Stub` (`file(_:)`, `$2` is the database path, the subcommand starts at `$4`), `TestDatabase` (`entry(feed:…)`, `execute`, `one`), `value(of:in:where:)`, `eventually(_:)`, `Timestamp.date(from:)`.

**Antiope in this phase:** nothing to change. There is no exhaustive `switch` over `Source` under `Antiope/` (checked: `AntiopeState.jump` switches over `PageDestination`, `PageDirectory` over `ReservedPage`; `PageDirectory.number(for:)` is a dictionary lookup). Until phase 6a, selecting Journal or a rhythm group in Antiope shows the header `P---` (no page number yet) and the entry column lists the rows, in rank order for the Journal, because Antiope draws `AppState.entries`. The only exhaustive `switch` over `Source` in the app is `AppState.refreshCurrent()` and `Queries.predicate(for:now:)`; both get the two new cases in 5b.

---

## Sub-phase 5a: Step 0 on `AppState.swift`, `JournalModel`

**Files (4):**
- Modify: `apps/macos/Ressac/App/AppState.swift` (Step 0 commit, then `journal`, activation and rollover hooks)
- Create: `apps/macos/Ressac/App/JournalModel.swift`
- Modify: `apps/macos/Ressac/App/RunCoordinator.swift` (the edition run's progress label)
- Create: `apps/macos/RessacTests/JournalModelTests.swift`

**Produces** (later sub-phases and phase 6 rely on these):
- `@MainActor @Observable final class JournalModel` with `Phase` (`idle`, `due`, `building(done:total:)`, `offline`, `failed(String)`), `phase`, `edition`, `init(database:coordinator:clock:calendar:)`, `checkAndBuild() async`, `retry() async`, `scheduleRollover()`, `cancelRollover()`, `static func nextRollover(after:calendar:) -> Date`, `sleep` (injectable).
- `AppState.journal: JournalModel`.
- `RunCoordinator.editionProgressLabel` = `"Building this morning's edition…"`: the `RunProgress.label` of an `edition build` run (phase 6b reads it to show `EDITION IN PROGRESS n/N`).

### Task 5a.0: Step 0, dead-code pass on `AppState.swift`

`AppState.swift` has 823 lines and this phase adds to it, so the Step 0 rule applies. Checked on 2026-10-16:

- **No unused member.** Every `func`, `var` and `let` declared in the file is referenced (script: for each declared name, `grep -rwn` over `apps/macos/Ressac` and `apps/macos/RessacTests`; the names only used inside the file are `canGardenDecide`, `loadMoreMargin`, `scheduleAutoRead`, `runFatalAlert` and locals, all called). Only `scheduleAutoRead(entryId:)` is wider than it needs to be: its one caller is `entryDidChange()`.
- **No debug log** (`print`, `NSLog`, `Logger`, `debugPrint`: none), **no unused import** (`AppKit`: `NSWorkspace`, `NSWindow`, `NSAlert`; `GRDB`: `AnyDatabaseCancellable`; `Observation`: `@Observable`).
- **Dead comments:** four references to the build phases of sub-project 2a, which mean nothing to a reader today, and one doc comment split in two by `@discardableResult`.

- [ ] **Step 1: Apply the cleanup.** Five replacements, nothing else:

```swift
// line 59, before
/// Import and export panels, presented by the window (phase 11).
// after
/// Import and export panels, presented by the window.
```

```swift
// line 89, before
    /// The feed selected in the Garden table (phase 12); garden commands act on it.
// after
    /// The feed selected in the Garden table; garden commands act on it.
```

```swift
// line 127, before
    /// Set by the reader (phase 10): scrolls one page of the given entry and returns false at
// after
    /// Set by the reader: scrolls one page of the given entry and returns false at
```

```swift
// line 541, before
    // MARK: Garden (UI in phase 12)
// after
    // MARK: Garden
```

```swift
// lines 793-794, before
                // A missing file is created by `migrate` too: the first-run screen then
                // deals with "no feeds" only (phase 11).
// after
                // A missing file is created by `migrate` too: the first-run screen then
                // deals with "no feeds" only.
```

```swift
// requestFullText, before
    /// The reader's Full article toggle (spec §7, Reader).
    @discardableResult
    /// On: back to the feed's text. Off with a stored article: show it. Otherwise fetch it,
    /// which after a failure is a retry.
    func requestFullText() -> Task<Void, Never>? {
// after
    /// The reader's Full article toggle (spec §7, Reader). On: back to the feed's text. Off with a
    /// stored article: show it. Otherwise fetch it, which after a failure is a retry.
    @discardableResult
    func requestFullText() -> Task<Void, Never>? {
```

```swift
// before
    func scheduleAutoRead(entryId: Int64) {
// after
    private func scheduleAutoRead(entryId: Int64) {
```

- [ ] **Step 2: Gate (Swift only, nothing else changed).**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tail -n 3
```

Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 3: Commit.**

```bash
git add apps/macos/Ressac/App/AppState.swift
git commit -m "AppState: Step 0, stale phase references and a split doc comment; scheduleAutoRead is private" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5a.1: `JournalModel` and the edition run's progress label

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/JournalModelTests.swift`:

```swift
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
        let (journal, _, _, stub) = try makeJournal()
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
        let (journal, state, _, stub) = try makeJournal()
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
        let (journal, state, _, stub) = try makeJournal()
        try flag(stub, "built")
        await journal.checkAndBuild()
        #expect(journal.phase == .idle)
        // Spec §9: the Journal simply reloads; no banner for a refusal that is not news.
        #expect(state.runs.banner == nil)
    }

    @Test func an_offline_refusal_keeps_its_banner() async throws {
        let (journal, state, _, stub) = try makeJournal()
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
        let (journal, state, _, stub) = try makeJournal()
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
        let (journal, _, _, stub) = try makeJournal()
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
        let (journal, _, _, stub) = try makeJournal()
        let log = SleepLog()
        journal.sleep = { try await log.wait($0) }
        journal.scheduleRollover()
        #expect(await eventually { (try? builds(stub)) == 1 })
        journal.cancelRollover()
        let waits = await log.waits
        // 09:30 Paris → 04:00 the next day: 18 h 30.
        #expect(waits.first == .seconds(18 * 3600 + 30 * 60))
    }
}
```

- [ ] **Step 2: Run them to see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/JournalModelTests 2>&1 | tail -n 5
```

Expected: build failure, `cannot find 'JournalModel' in scope` and `type 'RunCoordinator' has no member 'editionProgressLabel'`.

- [ ] **Step 3: Write `JournalModel`.** Create `apps/macos/Ressac/App/JournalModel.swift`:

```swift
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
    @ObservationIgnored private let calendar: Calendar
    @ObservationIgnored private var editionObservation: AnyDatabaseCancellable?
    @ObservationIgnored private var rolloverTask: Task<Void, Never>?
    /// A check or a build is under way: launch, activation and the rollover can fire together.
    @ObservationIgnored private var isBusy = false
    @ObservationIgnored private var sawNetworkUnreachable = false

    init(database: AppDatabase, coordinator: RunCoordinator,
         clock: @escaping @Sendable () -> Date = { Date() }, calendar: Calendar = .current) {
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
    func checkAndBuild() async {
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
        await checkAndBuild()
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
```

- [ ] **Step 4: The edition run's progress label.** In `apps/macos/Ressac/App/RunCoordinator.swift`, inside `final class RunCoordinator`, after `private static let log = …`:

```swift
    /// The progress label of `ressac edition build` (spec §8): the refresh it starts with is the
    /// morning edition being made.
    static let editionProgressLabel = "Building this morning's edition…"
```

In `execute(_:onEvent:)`, replace

```swift
                if request.kind == .long {
                    apply(event)
                }
```

with

```swift
                if request.kind == .long {
                    apply(event, request: request)
                }
```

and change the signature and the `.started` arm of `apply`:

```swift
    private func apply(_ event: EngineEvent, request: RunRequest) {
        switch event {
        case let .started(total):
            let isEdition = request.arguments.starts(with: ["edition", "build"])
            progress = RunProgress(done: 0, total: total, label: isEdition ? Self.editionProgressLabel : "Fetching feeds")
```

(the other arms are unchanged, including the `.edition` and `.cadence` cases phase 4b added to them).

- [ ] **Step 5: Wire it into `AppState`.** In `apps/macos/Ressac/App/AppState.swift`:

After `let runs: RunCoordinator`:

```swift
    /// Today's edition: the due check, the build run and the latest edition (spec §8).
    let journal: JournalModel
```

In `init(database:runs:)`, after `self.runs = runs`:

```swift
        self.journal = JournalModel(database: database, coordinator: runs)
```

At the end of `startSystemObservers()`, after the `systemObservers = [ … ]` array:

```swift
        // In the unit-test host, launched windows run stubs that do not know `edition build`, and a
        // build would take the long-run slot the tests' own refreshes expect (as
        // `AppearanceSwitchTests` does). `JournalModelTests` drive the model directly.
        guard !Launcher.isRunningTests else { return }
        systemObservers.append(
            center.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.applicationDidBecomeActive() }
            })
        journal.scheduleRollover()
        Task { await journal.checkAndBuild() }
```

Update its doc comment's first sentence to: `/// Starts what the running app needs from the system: refreshing on activation and polling for terminal writes (`ExternalChangeMonitor`), the midnight restart of the date-based queries, the morning edition (at launch, on activation and at 04:00), and tracking which window has the keyboard. Not started in unit tests.` (keep the line breaks at 100 columns).

After `dayDidChange()`:

```swift
    /// The app came to the front: the edition may have become due while it was in the background.
    func applicationDidBecomeActive() {
        Task { await journal.checkAndBuild() }
    }
```

`dayDidChange()` (Today's midnight restart) is unchanged.

- [ ] **Step 6: Run the tests.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/JournalModelTests -only-testing:RessacTests/RunCoordinatorTests -only-testing:RessacTests/LauncherTests 2>&1 | tail -n 5
```

Expected: `** TEST SUCCEEDED **`. `LauncherTests` and `AppearanceSwitchTests` launch real `AppState`s: the `isRunningTests` guard keeps them from starting a build (at launch, on activation or at 04:00), so their call lists and their long-run checks are unchanged.

- [ ] **Step 7: Commit.**

```bash
git add apps/macos/Ressac/App/JournalModel.swift apps/macos/Ressac/App/RunCoordinator.swift \
        apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/JournalModelTests.swift
git commit -m "JournalModel: build the morning edition at launch, on activation and at 04:00" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5a.2: Verification gate

- [ ] **Step 1: Gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-phase5a.log" | tail -n 5
grep -E '^/.+/apps/macos/.+: (warning|error):' "$TMPDIR/ressac-phase5a.log" || echo "no warnings in our sources"
```

Expected: fmt and clippy silent, every Rust test passes, `** TEST SUCCEEDED **`, `no warnings in our sources`. New suite: `JournalModelTests` (13 cases with the two arguments).

- [ ] **Step 2: Check the scope.** `git show --stat HEAD~1..HEAD` lists the four files of 5a, and `HEAD~1` is the Step 0 commit (one file).

Then stop: report to Clément and wait for his approval before 5b.

---
## Sub-phase 5b: Journal and Rhythm in the sidebar, the Journal list

**Files (5):**
- Modify: `apps/macos/Ressac/Data/Queries.swift` (`Source.journal`, `Source.cadence(_)`, sidebar counts, predicates, the Journal's rank order)
- Modify: `apps/macos/Ressac/App/AppState.swift` (`refreshCurrent`, the Journal's order when rows are retained)
- Modify: `apps/macos/Ressac/Reading/MainView.swift` (sidebar Journal and Rhythm, the Journal in the list column)
- Create: `apps/macos/Ressac/Reading/JournalList.swift` (`JournalLayout`, pure; `JournalListView`)
- Create: `apps/macos/RessacTests/JournalListTests.swift`

`AppState.swift` had its Step 0 in 5a. `Writes.swift` needs no change: `markAllRead(in:at:)` goes through `Queries.predicate(for:now:)`.

**Design.** The Journal is an ordinary list source, so selection, `j`/`k`, Space, auto-read, `m`, `s`, search and `⇧⌘A` keep working through `AppState` unchanged:
- its predicate is "the entries of the latest edition", so `⇧⌘A` marks the edition and never the overflow, and Space never leaves the edition;
- its rows come in `rank` order (`ORDER BY` the entry's rank in the latest edition), and the list always loads whole (`Queries.floor` and `Queries.nextUnread` return nil for it: an edition holds at most a few hundred rows, one per Daily feed, three per Weekly feed, plus the front page);
- the list column draws it with `JournalListView`, which groups `AppState.entries` under the three section titles with `JournalModel.edition` and inserts the overflow rows.
A rhythm group is an ordinary list source too, filtered by `v_cadence.effective_group`.

**Produces:**
- `Source.journal`, `Source.cadence(CadenceGroup)`; `SidebarSnapshot.journalUnread: Int`, `SidebarSnapshot.cadenceUnread: [CadenceGroup: Int]` (phase 6a reads both for the SOMMAIRE counts).
- `JournalLayout` (`sections(entries:edition:)`, `header(_:calendar:)`, `placeholder(phase:edition:)`, `overflowLabel(_:)`, `building`, `endLine`, `emptyLine`), `JournalSection`, `JournalRow` (phase 6b reuses `sections` and the strings for P104).
- `AppState.merge(fresh:previous:retained:floor:rank:)`.

### Task 5b.1: Sources, sidebar counts and the Journal's order

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/JournalListTests.swift`:

```swift
import Foundation
import GRDB
import Testing

@testable import Ressac

/// An `AppState` over a fixture copy with empty edition tables (the stub only records its calls).
@MainActor
func makeJournalState() throws -> (AppState, TestDatabase, Stub) {
    let (state, test, stub) = try makeState()
    try test.execute("DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition;")
    return (state, test, stub)
}

/// A diagnosis row with a rhythm group, as `diagnose` stores it.
func health(_ test: TestDatabase, feed: Int64, group: String) throws {
    try test.execute("""
        INSERT INTO feed_health (feed_id, status, reason, computed_at, cadence_group)
        VALUES (?, 'Alive', 'Last entry on 2026-10-15.', '2026-10-16T05:00:00.000Z', ?)
        """, [feed, group])
}

private func listRow(_ id: Int64, feed: Int64, published: String = "2026-10-15T08:00:00.000Z", read: Bool = false) -> EntryRow {
    EntryRow(entryId: id, feedId: feed, feedTitle: "Feed \(feed)", folderId: nil, title: "Entry \(id)", author: nil,
             url: nil, publishedAt: Timestamp.date(from: published)!, isRead: read, isStarred: false,
             hasFulltext: false, summaryHtml: nil, contentHtml: nil)
}

/// An edition built at 07:12 Paris time on Friday 16 October 2026. `items`: (entry, section, rank, feed).
private func sampleEdition(_ items: [(Int64, EditionSection, Int, Int64)], overflow: [EditionOverflowItem] = [],
                           read: Set<Int64> = [], unreachable: Int = 0) -> Edition {
    let built = Timestamp.date(from: "2026-10-16T05:12:00.000Z")!
    return Edition(
        info: EditionInfo(id: 1, builtAt: built, windowStart: built.addingTimeInterval(-86_400), windowEnd: built,
                          unreachable: unreachable),
        items: items.map {
            EditionItem(entryId: $0.0, section: $0.1, rank: $0.2, feedId: $0.3, feedTitle: "Feed \($0.3)",
                        title: "Entry \($0.0)", publishedAt: built, isRead: read.contains($0.0), isStarred: false)
        },
        overflow: overflow)
}

private func overflowItem(_ section: EditionSection, feed: Int64, _ hidden: Int, title: String? = nil) -> EditionOverflowItem {
    EditionOverflowItem(section: section, feedId: feed, feedTitle: title ?? "Feed \(feed)", page: nil, hiddenCount: hidden)
}

@MainActor @Suite struct JournalSourceTests {
    @Test func the_journal_lists_the_edition_in_rank_order() async throws {
        let (state, test, _) = try makeJournalState()
        let early = try test.entry(feed: 1, published: "2026-10-15T06:00:00.000Z")
        let late = try test.entry(feed: 2, published: "2026-10-15T09:00:00.000Z")
        let middle = try test.entry(feed: 8, published: "2026-10-15T07:00:00.000Z")
        _ = try test.entry(feed: 1, published: "2026-10-15T10:00:00.000Z")  // not in the edition
        try seedEdition(test, items: [(middle, "front", 1), (early, "week", 2), (late, "wire", 3)])
        try await state.database.notifyExternalChanges()
        state.selection = .journal
        #expect(state.entries.map(\.entryId) == [middle, early, late])
        #expect(!state.hasMoreEntries)
        #expect(await eventually { state.journal.edition?.items.count == 3 })
        let sections = JournalLayout.sections(entries: state.entries, edition: state.journal.edition)
        #expect(sections.map(\.section) == [.front, .week, .wire])
    }

    @Test func mark_all_read_in_the_journal_leaves_the_overflow_unread() async throws {
        let (state, test, _) = try makeJournalState()
        let first = try test.entry(feed: 1)
        let second = try test.entry(feed: 2)
        let leftOut = try test.entry(feed: 1, published: "2026-10-14T08:00:00.000Z")
        try seedEdition(test, items: [(first, "wire", 1), (second, "wire", 2)], overflow: [(feed: 1, section: "wire", hidden: 1)])
        state.selection = .journal
        #expect(state.canMarkAllRead)
        await state.markAllRead()?.value
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL") == 1)
        #expect(try test.one(Bool.self, "SELECT read_at IS NULL FROM entry WHERE id = ?", [leftOut]) == true)
    }

    @Test func space_stays_inside_the_edition_in_rank_order() async throws {
        let (state, test, _) = try makeJournalState()
        let first = try test.entry(feed: 1, published: "2026-10-15T06:00:00.000Z")
        let second = try test.entry(feed: 2, published: "2026-10-15T09:00:00.000Z")
        _ = try test.entry(feed: 8, published: "2026-10-15T12:00:00.000Z")  // newer, unread, not in the edition
        try seedEdition(test, items: [(first, "front", 1), (second, "wire", 2)])
        state.selection = .journal
        state.selectedEntryId = first
        state.selectNextUnread()
        #expect(state.selectedEntryId == second)
        state.selectNextUnread()
        #expect(state.selectedEntryId == second)
    }

    @Test func rhythm_groups_list_their_feeds_and_the_sidebar_counts_them() async throws {
        let (state, test, _) = try makeJournalState()
        try health(test, feed: 1, group: "daily")
        try health(test, feed: 2, group: "weekly")
        try health(test, feed: 8, group: "weekly")
        try test.execute("UPDATE feed SET cadence_override = 'rare' WHERE id = 8")
        let daily = try test.entry(feed: 1)
        _ = try test.entry(feed: 1)
        let weekly = try test.entry(feed: 2)
        let rare = try test.entry(feed: 8)
        try seedEdition(test, items: [(daily, "wire", 1)])

        let snapshot = try await value(of: Queries.sidebar(), in: state.database) { $0.journalUnread == 1 }
        #expect(snapshot.cadenceUnread == [.daily: 2, .weekly: 1, .rare: 1])

        state.selection = .cadence(.weekly)
        #expect(state.entries.map(\.entryId) == [weekly])
        state.selection = .cadence(.rare)
        #expect(state.entries.map(\.entryId) == [rare])
        state.selection = .cadence(.silent)
        #expect(state.entries.isEmpty)
    }

    @Test func refresh_from_the_journal_or_a_rhythm_group_fetches_every_feed() async throws {
        let (state, _, stub) = try makeJournalState()
        state.selection = .journal
        _ = await state.refreshCurrent().value
        state.selection = .cadence(.daily)
        _ = await state.refreshCurrent().value
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == [
            "fetch --force", "fetch --force",
        ])
    }

    @Test func rows_kept_after_reading_follow_the_edition_order() {
        let one = listRow(1, feed: 1, published: "2026-10-15T09:00:00.000Z")
        let two = listRow(2, feed: 2, published: "2026-10-15T06:00:00.000Z")
        let three = listRow(3, feed: 3, published: "2026-10-15T07:00:00.000Z")
        // Unread only: entry 1 was read during the visit and is kept; the edition ranks 3, 1, 2.
        let merged = AppState.merge(fresh: [two, three], previous: [three, one, two], retained: [1], floor: nil,
                                    rank: [3: 1, 1: 2, 2: 3])
        #expect(merged.map(\.entryId) == [3, 1, 2])
        #expect(merged[1].isRead)
        // Without ranks, newest first as before.
        #expect(AppState.merge(fresh: [two, three], previous: [three, one, two], retained: [1], floor: nil)
            .map(\.entryId) == [1, 3, 2])
    }
}

/// `@MainActor`: `JournalModel.Phase` is declared inside a main-actor class.
@MainActor @Suite struct JournalLayoutTests {
    @Test func entries_go_under_their_section_and_overflow_follows_its_feed() {
        let entries = [listRow(1, feed: 5), listRow(2, feed: 3), listRow(3, feed: 3), listRow(4, feed: 4), listRow(5, feed: 1)]
        let edition = sampleEdition(
            [(1, .front, 1, 5), (2, .week, 2, 3), (3, .week, 3, 3), (4, .week, 4, 4), (5, .wire, 5, 1)],
            overflow: [overflowItem(.week, feed: 3, 2), overflowItem(.wire, feed: 1, 12), overflowItem(.wire, feed: 9, 4)])
        let sections = JournalLayout.sections(entries: entries, edition: edition)
        #expect(sections.map(\.section) == [.front, .week, .wire])
        #expect(sections.map { $0.rows.map(\.id) } == [
            ["entry-1"],
            ["entry-2", "entry-3", "overflow-week-3", "entry-4"],
            // Feed 9 has no row in the list: its overflow closes the section.
            ["entry-5", "overflow-wire-1", "overflow-wire-9"],
        ])
    }

    @Test func a_section_without_rows_is_omitted() {
        let edition = sampleEdition([(2, .week, 1, 3)])
        #expect(JournalLayout.sections(entries: [listRow(2, feed: 3)], edition: edition).map(\.section) == [.week])
        #expect(JournalLayout.sections(entries: [listRow(2, feed: 3)], edition: nil).isEmpty)
    }

    @Test func the_header_counts_entries_read_entries_and_unreachable_feeds() {
        let items: [(Int64, EditionSection, Int, Int64)] = [(1, .front, 1, 1), (2, .wire, 2, 2), (3, .wire, 3, 3)]
        #expect(JournalLayout.header(sampleEdition(items, read: [2]), calendar: .paris)
            == "Friday 16 October · 3 entries · 1 read")
        #expect(JournalLayout.header(sampleEdition(items, unreachable: 3), calendar: .paris)
            == "Friday 16 October · 3 entries · 0 read · 3 feeds unreachable")
        #expect(JournalLayout.header(sampleEdition([(1, .front, 1, 1)], unreachable: 1), calendar: .paris)
            == "Friday 16 October · 1 entry · 0 read · 1 feed unreachable")
    }

    @Test func placeholders_say_what_the_journal_is_waiting_for() {
        let full = sampleEdition([(1, .front, 1, 1)])
        let empty = sampleEdition([])
        #expect(JournalLayout.placeholder(phase: .building(done: 0, total: 0), edition: full) == "Building this morning's edition…")
        #expect(JournalLayout.placeholder(phase: .building(done: 34, total: 156), edition: nil)
            == "Building this morning's edition… 34/156")
        #expect(JournalLayout.placeholder(phase: .offline, edition: full) == "Offline: no edition yet")
        #expect(JournalLayout.placeholder(phase: .failed("error: disk full"), edition: nil) == "error: disk full")
        #expect(JournalLayout.placeholder(phase: .idle, edition: empty) == "Nothing new this morning.")
        #expect(JournalLayout.placeholder(phase: .idle, edition: nil) == "No edition yet")
        #expect(JournalLayout.placeholder(phase: .idle, edition: full) == nil)
        #expect(JournalLayout.placeholder(phase: .due, edition: full) == nil)
        #expect(JournalLayout.endLine == "That's the edition.")
    }

    @Test func overflow_rows_name_the_feed() {
        #expect(JournalLayout.overflowLabel(overflowItem(.wire, feed: 4, 12, title: "Actualitté")) == "+12 more in Actualitté")
        #expect(JournalLayout.overflowLabel(overflowItem(.week, feed: 4, 1, title: "Fabula")) == "+1 more in Fabula")
    }
}
```

`seedEdition`, `Calendar.paris` and `eventually` come from `JournalModelTests.swift` and `RunCoordinatorTests.swift` (same test target).

- [ ] **Step 2: Run them to see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/JournalSourceTests -only-testing:RessacTests/JournalLayoutTests 2>&1 | tail -n 5
```

Expected: build failure, `type 'Source' has no member 'journal'` and `cannot find 'JournalLayout' in scope`.

- [ ] **Step 3: The sources and the sidebar counts.** In `apps/macos/Ressac/Data/Queries.swift`:

Replace the `Source` enum's cases:

```swift
enum Source: Hashable, Sendable {
    case unread, today, starred
    /// The latest morning edition, in rank order (spec §8, Journal screen).
    case journal
    /// The feeds of one rhythm group, by effective group (spec §4).
    case cadence(CadenceGroup)
    case folder(Int64)
    case feed(Int64)
    case garden(GardenScope)
```

(`isGarden` is unchanged.)

In `SidebarSnapshot`, after `var needsTriage = 0`:

```swift
    /// Unread entries of the latest edition.
    var journalUnread = 0
    /// Unread entries per rhythm group (effective group, active feeds); a group with none is absent.
    var cadenceUnread: [CadenceGroup: Int] = [:]
```

In `sidebar(now:)`, after the `needsTriage:` argument (add a comma after its closing `?? 0`):

```swift
                needsTriage: try Int.fetchOne(
                    db, sql: "SELECT COUNT(*) FROM v_garden_row WHERE needs_triage AND state = 'active'") ?? 0,
                journalUnread: try Int.fetchOne(db, sql: """
                    SELECT COUNT(*) FROM edition_entry ee JOIN entry e ON e.id = ee.entry_id
                    WHERE ee.edition_id = (SELECT MAX(id) FROM edition) AND e.read_at IS NULL
                    """) ?? 0,
                cadenceUnread: try Dictionary(
                    Row.fetchAll(db, sql: """
                        SELECT effective_group, SUM(unread) AS unread FROM v_cadence
                        GROUP BY effective_group HAVING SUM(unread) > 0
                        """)
                    .compactMap { row -> (CadenceGroup, Int)? in
                        guard let group = (row["effective_group"] as String?).flatMap(CadenceGroup.init(rawValue:)) else { return nil }
                        return (group, row["unread"])
                    },
                    uniquingKeysWith: { first, _ in first })
```

In `predicate(for:now:)`, after the `.starred` case:

```swift
        case .journal:
            Predicate(sql: "entry_id IN (SELECT entry_id FROM edition_entry WHERE edition_id = (SELECT MAX(id) FROM edition))")
        case let .cadence(group):
            Predicate(sql: "feed_id IN (SELECT feed_id FROM v_cadence WHERE effective_group = ?)", arguments: [group.rawValue])
```

- [ ] **Step 4: The Journal's rank order, loaded whole.** Still in `Queries.swift`:

After `private static let listColumns = …`:

```swift
    /// An entry's rank in the latest edition: the Journal's order (spec §6, Order).
    private static let editionRank = """
        (SELECT ee.rank FROM edition_entry ee
         WHERE ee.entry_id = v_entry.entry_id AND ee.edition_id = (SELECT MAX(id) FROM edition))
        """
```

Replace `entries` and `search`:

```swift
    /// The entry list of a source, newest first (the Journal: in rank order), down to `floor`
    /// (every row when nil). Garden sources have no entries.
    static func entries(source: Source, unreadOnly: Bool, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: unreadOnly, search: nil, now: now), floor: floor,
             ranked: source == .journal)
    }

    /// Full-text search within a source (spec §7, Search), down to `floor` like `entries`. Every
    /// word must appear (see `ftsQuery`).
    static func search(_ text: String, in source: Source, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: false, search: text, now: now), floor: floor,
             ranked: source == .journal)
    }
```

In `floor(source:…)`, replace the first line of the body:

```swift
        // The Journal always loads whole: its order is the edition's, not the floor's dates.
        guard source != .journal,
              let list = condition(source: source, unreadOnly: unreadOnly, search: search, now: now) else { return nil }
```

In `nextUnread(source:…)`, the same:

```swift
        guard source != .journal,
              let list = condition(source: source, unreadOnly: unreadOnly, search: search, now: now) else { return nil }
```

Replace `page(of:floor:)`:

```swift
    /// The rows of `list` down to `floor`, and the counts of the whole list. `ranked`: in the
    /// latest edition's order (the Journal), else newest first.
    private static func page(of list: Predicate?, floor: ListFloor?, ranked: Bool = false) -> LiveQuery<EntryPage> {
        let order = ranked ? "\(editionRank), entry_id" : "published_at DESC, entry_id DESC"
        return ValueObservation.tracking { db in
            guard let list else { return EntryPage() }
            let loaded = bounded(list, by: floor, ">=")
            let rows = try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry WHERE \(loaded.sql)
                ORDER BY \(order)
                """, arguments: loaded.arguments)
            guard let counts = try Row.fetchOne(db, sql: """
                SELECT COUNT(*) AS total, COALESCE(SUM(NOT is_read), 0) AS unread FROM v_entry WHERE \(list.sql)
                """, arguments: list.arguments)
            else { return EntryPage(rows: rows, total: rows.count) }
            return EntryPage(rows: rows, total: counts["total"], unread: counts["unread"])
        }
        .removeDuplicates()
    }
```

- [ ] **Step 5: `AppState`.** In `apps/macos/Ressac/App/AppState.swift`:

In `refreshCurrent()`:

```swift
        case .unread, .today, .starred, .journal, .cadence, .garden: runs.start(.fetch(feeds: []))
```

(⌘R refreshes; it never rebuilds the edition, which is the core's once-a-day job.)

In `listDidLoad(_:)`, replace the first line and the re-paging condition:

```swift
        let rank = selection == .journal ? journalRanks : [:]
        let merged = Self.merge(fresh: page.rows, previous: entries, retained: retained, floor: listFloor, rank: rank)
```

```swift
        if listFloor == nil, !listFloorFailed, selection != .journal, page.rows.count > listPageSize {
```

After `private var selectedIndex: Int? { … }`:

```swift
    /// Each entry's place in the latest edition, for the Journal's order (spec §6, Order).
    private var journalRanks: [Int64: Int] {
        Dictionary((journal.edition?.items ?? []).map { ($0.entryId, $0.rank) }, uniquingKeysWith: { first, _ in first })
    }
```

Replace `merge`:

```swift
    /// Fresh rows, plus the entries read during this visit that an unread-only query
    /// dropped: newest first, or by `rank` when given (the Journal). Only rows at or after
    /// `floor` are kept: below it, a row of the list is merely not loaded.
    static func merge(fresh: [EntryRow], previous: [EntryRow], retained: Set<Int64>, floor: ListFloor?,
                      rank: [Int64: Int] = [:]) -> [EntryRow] {
        let freshIds = Set(fresh.map(\.entryId))
        let kept = previous.filter {
            retained.contains($0.entryId) && !freshIds.contains($0.entryId) && floor?.admits($0) != false
        }
        guard !kept.isEmpty else { return fresh }
        let rows = fresh + kept.map { row in
            var row = row
            row.isRead = true
            return row
        }
        guard rank.isEmpty else {
            return rows.sorted { (rank[$0.entryId] ?? .max, $0.entryId) < (rank[$1.entryId] ?? .max, $1.entryId) }
        }
        return rows.sorted { ($0.publishedAt, $0.entryId) > ($1.publishedAt, $1.entryId) }
    }
```

- [ ] **Step 6: `JournalLayout` and `JournalListView`.** Create `apps/macos/Ressac/Reading/JournalList.swift`:

```swift
import SwiftUI

/// One row of the Journal list: an entry, or the "+12 more in …" line of a capped feed.
enum JournalRow: Identifiable, Equatable {
    case entry(EntryRow)
    case overflow(EditionOverflowItem)

    var id: String {
        switch self {
        case let .entry(entry): "entry-\(entry.entryId)"
        case let .overflow(item): "overflow-\(item.section.rawValue)-\(item.feedId)"
        }
    }
}

/// One section of the Journal (Front Page, This Week, The Wire) and its rows.
struct JournalSection: Identifiable, Equatable {
    let section: EditionSection
    let rows: [JournalRow]

    var id: EditionSection { section }
}

/// The Journal screen's text and layout, pure (spec §8, Journal screen). `AppState.entries` gives
/// the rows (rank order, live read state, search); the edition gives their sections and the
/// overflow.
enum JournalLayout {
    static let building = "Building this morning's edition…"
    static let endLine = "That's the edition."
    static let emptyLine = "Nothing new this morning."

    /// The edition's sections in order, each with its listed entries and, right after the last
    /// entry of a capped feed, that feed's overflow row (at the section's end when none of its
    /// entries is listed). A section with no row is omitted.
    static func sections(entries: [EntryRow], edition: Edition?) -> [JournalSection] {
        guard let edition else { return [] }
        let sectionOf = Dictionary(edition.items.map { ($0.entryId, $0.section) }, uniquingKeysWith: { first, _ in first })
        return EditionSection.allCases.compactMap { section in
            var rows = entries.filter { sectionOf[$0.entryId] == section }.map(JournalRow.entry)
            for item in edition.overflow where item.section == section {
                let last = rows.lastIndex { row in
                    if case let .entry(entry) = row { entry.feedId == item.feedId } else { false }
                }
                rows.insert(.overflow(item), at: last.map { $0 + 1 } ?? rows.endIndex)
            }
            return rows.isEmpty ? nil : JournalSection(section: section, rows: rows)
        }
    }

    /// "Friday 16 October · 43 entries · 12 read", plus " · 3 feeds unreachable".
    static func header(_ edition: Edition, calendar: Calendar = .current) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "EEEE d MMMM"
        let count = edition.items.count
        var parts = [formatter.string(from: edition.info.builtAt), count == 1 ? "1 entry" : "\(count) entries",
                     "\(edition.readCount) read"]
        let unreachable = edition.info.unreachable
        if unreachable > 0 {
            parts.append(unreachable == 1 ? "1 feed unreachable" : "\(unreachable) feeds unreachable")
        }
        return parts.joined(separator: " · ")
    }

    /// What the Journal shows instead of the list, or nil to show the edition.
    static func placeholder(phase: JournalModel.Phase, edition: Edition?) -> String? {
        switch phase {
        case let .building(done, total):
            return total > 0 ? "\(building) \(done)/\(total)" : building
        case .offline:
            return "Offline: no edition yet"
        case let .failed(message):
            return message
        case .idle, .due:
            guard let edition else { return "No edition yet" }
            return edition.items.isEmpty ? emptyLine : nil
        }
    }

    /// "+12 more in Actualitté".
    static func overflowLabel(_ item: EditionOverflowItem) -> String {
        "+\(item.hiddenCount) more in \(item.feedTitle)"
    }
}

/// The list column while the Journal is selected (spec §8).
struct JournalListView: View {
    @Bindable var state: AppState

    var body: some View {
        let journal = state.journal
        if let message = JournalLayout.placeholder(phase: journal.phase, edition: journal.edition) {
            VStack(spacing: 10) {
                if case .building = journal.phase {
                    ProgressView().controlSize(.small)
                }
                Text(message)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .monospacedDigit()
                if Self.offersRetry(journal.phase) {
                    Button("Retry") { Task { await journal.retry() } }
                }
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let edition = journal.edition {
            VStack(spacing: 0) {
                Text(JournalLayout.header(edition))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                Divider()
                list(edition)
            }
        }
    }

    private static func offersRetry(_ phase: JournalModel.Phase) -> Bool {
        switch phase {
        case .offline, .failed: true
        case .idle, .due, .building: false
        }
    }

    private func list(_ edition: Edition) -> some View {
        ScrollViewReader { proxy in
            List(selection: $state.selectedEntryId) {
                ForEach(JournalLayout.sections(entries: state.entries, edition: edition)) { section in
                    Section(section.section.title) {
                        ForEach(section.rows) { row in
                            switch row {
                            case let .entry(entry):
                                EntryRowView(entry: entry).tag(entry.entryId)
                            case let .overflow(item):
                                // Not selectable: a click opens the feed in the sidebar.
                                Button(JournalLayout.overflowLabel(item)) { state.selection = .feed(item.feedId) }
                                    .buttonStyle(.plain)
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if edition.isFinished {
                    Text(JournalLayout.endLine)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                }
            }
            // As in `EntryListView`: the keyboard moves the selection, the list follows on the next turn.
            .onChange(of: state.selectedEntryId) { _, id in
                guard let id else { return }
                Task { @MainActor in proxy.scrollTo("entry-\(id)") }
            }
        }
    }
}
```

- [ ] **Step 7: The sidebar and the list column.** In `apps/macos/Ressac/Reading/MainView.swift`:

In `MainView.body`, replace the `content:` group:

```swift
            Group {
                if case let .garden(scope) = state.selection {
                    GardenTable(state: state, scope: scope)
                } else if state.selection == .journal {
                    JournalListView(state: state)
                } else {
                    EntryListView(state: state)
                }
            }
```

In `SidebarView.body`, replace the `Smart Lists` section and add the `Rhythm` section between `Feeds` and `Garden`:

```swift
            Section("Smart Lists") {
                Label("Journal", systemImage: "newspaper").badge(state.sidebar.journalUnread).tag(Source.journal)
                Label("Unread", systemImage: "circle.inset.filled").badge(state.sidebar.unread).tag(Source.unread)
                Label("Today", systemImage: "sun.max").badge(state.sidebar.today).tag(Source.today)
                Label("Starred", systemImage: "star").badge(state.sidebar.starred).tag(Source.starred)
            }
            Section("Feeds") {
                OutlineGroup(state.sidebarTree, children: \.children) { node in
                    SidebarRow(node: node, state: state).tag(node.source)
                }
            }
            Section("Rhythm") {
                // Silent stays listed at zero: it is where feeds that stopped publishing end up.
                ForEach(CadenceGroup.allCases, id: \.self) { group in
                    Label(group.title, systemImage: group.symbol)
                        .badge(state.sidebar.cadenceUnread[group] ?? 0)
                        .tag(Source.cadence(group))
                }
            }
```

At the end of the file:

```swift
private extension CadenceGroup {
    var symbol: String {
        switch self {
        case .daily: "bolt"
        case .weekly: "calendar"
        case .monthly: "moon"
        case .rare: "sparkles"
        case .silent: "zzz"
        }
    }
}
```

- [ ] **Step 8: Run the tests.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/JournalSourceTests -only-testing:RessacTests/JournalLayoutTests -only-testing:RessacTests/AppStateTests -only-testing:RessacTests/ListPageTests -only-testing:RessacTests/QueryTests 2>&1 | tail -n 5
```

Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 9: Commit.**

```bash
git add apps/macos/Ressac/Data/Queries.swift apps/macos/Ressac/App/AppState.swift \
        apps/macos/Ressac/Reading/MainView.swift apps/macos/Ressac/Reading/JournalList.swift \
        apps/macos/RessacTests/JournalListTests.swift
git commit -m "Journal and Rhythm in the sidebar; the Journal lists the edition by section with its overflow" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5b.2: Verification gate

- [ ] **Step 1: Gate.** The command block of Task 5a.2, with the log named `ressac-phase5b.log`. Expected: as in 5a, new suites `JournalSourceTests` (6) and `JournalLayoutTests` (5).

- [ ] **Step 2: Look at it once.** Build and run the app on a copy of the real database (Settings → Database, or `RESSAC_DB=$TMPDIR/ressac-copy.sqlite open apps/macos/build/…/Ressac.app`), after `ressac edition build --db $TMPDIR/ressac-copy.sqlite` from phase 3a: Journal first in the sidebar, the three section titles, overflow rows under their feed, the header line, `That's the edition.` after `⇧⌘A`, the Rhythm section with Silent at zero. Never on `~/Library/…`.

Then stop: report to Clément and wait for his approval before 5c.

---
## Sub-phase 5c: Rhythm in the inspector and in Garden › All Feeds

**Files (5):**
- Modify: `apps/macos/Ressac/Garden/GardenModel.swift` (`GardenGrouping`, rhythm line, label, glyphs, sections, ages; pure)
- Modify: `apps/macos/Ressac/Garden/GardenInspector.swift` (rhythm line and override menu)
- Modify: `apps/macos/Ressac/Garden/GardenTable.swift` (Rhythm label on each All Feeds row, Status / Rhythm grouping)
- Modify: `apps/macos/Ressac/App/AppState.swift` (`setCadence(feedId:_:)`)
- Modify: `apps/macos/RessacTests/GardenTests.swift`

**Design.** In the Native window, a feed's inspector is the Garden's detail column, so the rhythm line and its menu go there, under the status. The garden list is a sectioned `List`, not a table with sortable headers; "a sortable Rhythm column" (spec §8) becomes, in All Feeds only, a trailing rhythm label on every row and a `Status | Rhythm` segmented control that regroups the rows by rhythm (Daily … Silent, then Unchecked). `v_garden_row` does not change: both views join `Queries.cadenceRows()` by `feedId` (plan index, Phase 5c). A buried feed has no `v_cadence` row, so the Cemetery shows neither the line nor the menu. The menu goes through `ressac feed cadence` (a short run), never a direct write: `feed.cadence_override` is written by the core only (Global Constraints).

**Produces:** `GardenGrouping`, `GardenModel.rhythmLine(_:now:)`, `GardenModel.rhythmText(_:now:)` (the line without `Rhythm: `, used by phase 6c), `rhythmLabel(_:)`, `rhythmGlyph(_:)`, `rhythmSections(_:cadence:)`, `ago(_:now:)`; `AppState.setCadence(feedId:_:)` (phase 6b's Antiope inspector calls it too).

### Task 5c.1: Rhythm model, inspector, All Feeds

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/GardenTests.swift` (the file's private `row(…)` helper is reused):

```swift
private func cadence(_ id: Int64, _ group: CadenceGroup, override: Bool = false, median: Double? = 0.4,
                     last: Date? = nil) -> CadenceRow {
    CadenceRow(feedId: id, effective: group, computed: override ? .weekly : group, isOverride: override,
               medianIntervalDays: median, entriesPerMonth: 12, lastEntryAt: last, unread: 0)
}

@MainActor @Suite struct GardenRhythmTests {
    let now = Timestamp.date(from: "2026-10-16T09:00:00.000Z")!

    @Test func theRhythmLineReadsLikeTheSpec() {
        let twoHours = now.addingTimeInterval(-2 * 3600)
        #expect(GardenModel.rhythmLine(cadence(1, .daily, last: twoHours), now: now)
            == "Rhythm: Daily · median 0.4 d · last entry 2 h ago")
        #expect(GardenModel.rhythmLine(cadence(1, .daily, override: true, last: twoHours), now: now)
            == "Rhythm: Daily · median 0.4 d · last entry 2 h ago (set by you)")
        #expect(GardenModel.rhythmLine(cadence(2, .monthly, median: 12.3, last: now.addingTimeInterval(-3 * 86_400)), now: now)
            == "Rhythm: Monthly · median 12 d · last entry 3 d ago")
        #expect(GardenModel.rhythmLine(cadence(3, .silent, median: nil), now: now) == "Rhythm: Silent")
        #expect(GardenModel.rhythmText(cadence(1, .daily, override: true, last: twoHours), now: now)
            == "Daily · median 0.4 d · last entry 2 h ago (set by you)")
    }

    @Test func agesCountMinutesHoursDaysThenMonths() {
        #expect(GardenModel.ago(now.addingTimeInterval(-45 * 60), now: now) == "45 min ago")
        #expect(GardenModel.ago(now.addingTimeInterval(-47 * 3600), now: now) == "47 h ago")
        #expect(GardenModel.ago(now.addingTimeInterval(-59 * 86_400), now: now) == "59 d ago")
        #expect(GardenModel.ago(now.addingTimeInterval(-400 * 86_400), now: now) == "13 mo ago")
        #expect(GardenModel.ago(now.addingTimeInterval(60), now: now) == "0 min ago")  // a clock ahead of the feed
    }

    @Test func theLabelMarksAGroupSetByHand() {
        #expect(GardenModel.rhythmLabel(cadence(1, .daily)) == "Daily")
        #expect(GardenModel.rhythmLabel(cadence(1, .rare, override: true)) == "Rare*")
        #expect(GardenModel.rhythmLabel(nil) == nil)
    }

    @Test func allFeedsGroupsByRhythmThenUnchecked() {
        let rows = [row(1, "Zeta", status: "Alive"), row(2, "Alpha", status: "Dead"),
                    row(3, "Mid", status: "Alive"), row(4, "New", status: nil)]
        let groups = [1: cadence(1, .daily), 2: cadence(2, .daily), 3: cadence(3, .rare, override: true)]
        let sections = GardenModel.rhythmSections(rows, cadence: groups)
        #expect(sections.map(\.id) == ["Daily", "Rare", "Unchecked"])
        #expect(sections.map { $0.rows.map(\.feedId) } == [[2, 1], [3], [4]])
        #expect(sections.map(\.header) == ["⚡ DAILY · 2", "✨ RARE · 1", "❔ UNCHECKED · 1"])
        #expect(GardenGrouping.allCases.map(\.title) == ["Status", "Rhythm"])
    }

    @Test func theInspectorMenuRunsFeedCadence() async throws {
        let (state, _, stub) = try makeState()
        _ = await state.setCadence(feedId: 3, .daily).value
        _ = await state.setCadence(feedId: 3, nil).value
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == [
            "feed cadence 3 daily", "feed cadence 3 auto",
        ])
    }
}
```

- [ ] **Step 2: Run them to see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenRhythmTests 2>&1 | tail -n 5
```

Expected: build failure, `type 'GardenModel' has no member 'rhythmLine'`.

- [ ] **Step 3: The model.** At the end of `apps/macos/Ressac/Garden/GardenModel.swift`:

```swift
/// How Garden › All Feeds groups its rows (spec §8): by status, as everywhere in the garden, or
/// by publishing rhythm.
enum GardenGrouping: String, CaseIterable, Sendable {
    case status, rhythm

    var title: String {
        switch self {
        case .status: "Status"
        case .rhythm: "Rhythm"
        }
    }
}

// MARK: - Rhythm (spec §4, §8)

extension GardenModel {
    /// Section glyphs of the rhythm grouping; `nil` is a feed without a diagnosis yet.
    static func rhythmGlyph(_ group: CadenceGroup?) -> String {
        switch group {
        case .daily: "⚡"
        case .weekly: "📅"
        case .monthly: "🌗"
        case .rare: "✨"
        case .silent: "💤"
        case nil: "❔"
        }
    }

    /// The All Feeds row label: "Daily", or "Daily*" when the group was set by hand. Nil for a
    /// feed with no rhythm yet (not diagnosed, or buried).
    static func rhythmLabel(_ cadence: CadenceRow?) -> String? {
        guard let cadence else { return nil }
        return cadence.isOverride ? "\(cadence.effective.title)*" : cadence.effective.title
    }

    /// "Rhythm: Daily · median 0.4 d · last entry 2 h ago", ending in " (set by you)" when the
    /// group was set by hand.
    static func rhythmLine(_ cadence: CadenceRow, now: Date) -> String {
        "Rhythm: " + rhythmText(cadence, now: now)
    }

    /// The line without its "Rhythm: " label, shared with the Antiope inspector (phase 6c) so
    /// that both looks say the same thing. A missing median or last entry leaves its part out.
    static func rhythmText(_ cadence: CadenceRow, now: Date) -> String {
        var parts = [cadence.effective.title]
        if let median = cadence.medianIntervalDays {
            parts.append(median < 10 ? String(format: "median %.1f d", median) : "median \(Int(median.rounded())) d")
        }
        if let last = cadence.lastEntryAt {
            parts.append("last entry \(ago(last, now: now))")
        }
        return parts.joined(separator: " · ") + (cadence.isOverride ? " (set by you)" : "")
    }

    /// "45 min ago", "2 h ago" (under two days), "3 d ago" (under 60 days), then "13 mo ago".
    static func ago(_ date: Date, now: Date) -> String {
        let minutes = max(0, Int(now.timeIntervalSince(date) / 60))
        if minutes < 60 { return "\(minutes) min ago" }
        let hours = minutes / 60
        if hours < 48 { return "\(hours) h ago" }
        let days = hours / 24
        if days < 60 { return "\(days) d ago" }
        return "\(days / 30) mo ago"
    }

    /// All Feeds grouped by effective rhythm, Daily to Silent, then the feeds without one
    /// ("Unchecked"); inside a group, the status sections' order (top folder, then title).
    static func rhythmSections(_ rows: [GardenRow], cadence: [Int64: CadenceRow]) -> [Section] {
        let groups: [CadenceGroup?] = CadenceGroup.allCases.map(Optional.some) + [nil]
        return groups.compactMap { group in
            let members = rows.filter { cadence[$0.feedId]?.effective == group }.sorted(by: rowOrder)
            guard !members.isEmpty else { return nil }
            return Section(id: group?.title ?? "Unchecked", glyph: rhythmGlyph(group), rows: members)
        }
    }
}
```

(`rowOrder` is `private static` in `GardenModel`; an extension in the same file may call it.)

- [ ] **Step 4: `AppState.setCadence`.** In `apps/macos/Ressac/App/AppState.swift`, under `// MARK: Feeds`, after `setPreferFullText(feedId:on:)`:

```swift
    /// The inspector's Rhythm menu (spec §8): a group, or nil for Auto. The core writes it.
    @discardableResult
    func setCadence(feedId: Int64, _ group: CadenceGroup?) -> Task<RunOutcome, Never> {
        runs.start(.setCadence(feedId: feedId, group))
    }
```

- [ ] **Step 5: The inspector.** In `apps/macos/Ressac/Garden/GardenInspector.swift`:

After `@State private var showsHistory = false`:

```swift
    /// `v_cadence` by feed: active feeds only, so a buried feed shows no rhythm.
    @State private var cadence: [Int64: CadenceRow] = [:]
```

After the two `.task(id: scope)` modifiers of `body`:

```swift
        .task { await observeCadence() }
```

In `details(_:)`, right after the status `VStack` (the one ending with `if let reason = row.reason { … }`) and before `if let bullets = …`:

```swift
            if let rhythm = cadence[row.feedId], !buried {
                rhythmControl(rhythm, feedId: row.feedId)
            }
```

After `actions(_:buried:)`:

```swift
    /// "Rhythm: Daily · median 0.4 d · last entry 2 h ago" and the Auto / group menu.
    private func rhythmControl(_ rhythm: CadenceRow, feedId: Int64) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(GardenModel.rhythmLine(rhythm, now: state.now()))
                .fixedSize(horizontal: false, vertical: true)
            Picker("Rhythm", selection: Binding(
                get: { rhythm.isOverride ? Optional(rhythm.effective) : nil },
                set: { state.setCadence(feedId: feedId, $0) })) {
                Text("Auto").tag(CadenceGroup?.none)
                ForEach(CadenceGroup.allCases, id: \.self) { group in
                    Text(group.title).tag(Optional(group))
                }
            }
            .pickerStyle(.menu)
            .labelsHidden()
            .fixedSize()
            .controlSize(.small)
            .help("Auto follows how often the feed publishes; a group chosen here stays until you choose Auto.")
        }
        .font(.callout)
    }
```

After `observeLog(of:)`:

```swift
    private func observeCadence() async {
        do {
            for try await rows in Queries.cadenceRows().values(in: state.database.reader) {
                cadence = Dictionary(rows.map { ($0.feedId, $0) }, uniquingKeysWith: { first, _ in first })
            }
        } catch {
            cadence = [:]
        }
    }
```

- [ ] **Step 6: All Feeds.** In `apps/macos/Ressac/Garden/GardenTable.swift`:

After `@FocusState private var listFocused: Bool`:

```swift
    @State private var grouping = GardenGrouping.status
    /// `v_cadence` by feed, for the Rhythm label and grouping of All Feeds.
    @State private var cadence: [Int64: CadenceRow] = [:]
```

In `body`, replace `let sections = GardenModel.sections(visible, scope: scope)` with:

```swift
        let sections = scope == .all && grouping == .rhythm
            ? GardenModel.rhythmSections(visible, cadence: cadence)
            : GardenModel.sections(visible, scope: scope)
```

and the row:

```swift
                                GardenRowView(row: row, dimmed: GardenModel.isDimmed(row, scope: scope, seenInQueue: retained),
                                              rhythm: scope == .all ? GardenModel.rhythmLabel(cadence[row.feedId]) : nil)
                                    .tag(row.feedId)
```

After the `.task(id: scope) { … }` modifier:

```swift
        .task { await observeCadence() }
```

In `header(visibleCount:safeRepairCount:)`, right after the count `Text(…)` capsule:

```swift
            if scope == .all {
                Picker("Group by", selection: $grouping) {
                    ForEach(GardenGrouping.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()
                .help("Group the feeds by status or by publishing rhythm")
            }
```

After `handle(_:modifiers:safeRepairCount:)`:

```swift
    private func observeCadence() async {
        do {
            for try await rows in Queries.cadenceRows().values(in: state.database.reader) {
                cadence = Dictionary(rows.map { ($0.feedId, $0) }, uniquingKeysWith: { first, _ in first })
            }
        } catch {
            cadence = [:]
        }
    }
```

In `GardenRowView`, add the property and its label, after the `clue` text:

```swift
    let row: GardenRow
    let dimmed: Bool
    /// "Daily" or "Daily*" in All Feeds, nil elsewhere.
    let rhythm: String?
```

```swift
            if let rhythm {
                Text(rhythm)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .help(rhythm.hasSuffix("*") ? "Rhythm set by you" : "Rhythm")
            }
```

The garden keys, the selection rule after a decision and the Antiope garden pages are unchanged: in All Feeds a decided row stays in place (dimmed), whichever grouping is shown.

- [ ] **Step 7: Run the tests.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenRhythmTests -only-testing:RessacTests/GardenModelTests -only-testing:RessacTests/AppStateTests 2>&1 | tail -n 5
```

Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 8: Commit.**

```bash
git add apps/macos/Ressac/Garden/GardenModel.swift apps/macos/Ressac/Garden/GardenInspector.swift \
        apps/macos/Ressac/Garden/GardenTable.swift apps/macos/Ressac/App/AppState.swift \
        apps/macos/RessacTests/GardenTests.swift
git commit -m "Garden: rhythm line and override menu in the inspector, rhythm label and grouping in All Feeds" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5c.2: Verification gate

- [ ] **Step 1: Gate.** The command block of Task 5a.2, with the log named `ressac-phase5c.log`. Expected: as in 5a, new suite `GardenRhythmTests` (5); `GardenModelTests` and `GardenTests` unchanged and green.

- [ ] **Step 2: Look at it once** on the database copy of 5b: Garden › All Feeds, switch to Rhythm, open a Daily feed, choose Weekly in the menu (the line ends with `(set by you)`, the row reads `Weekly*`, the sidebar's Weekly count grows), then Auto. Never on `~/Library/…`.

Then stop: report to Clément and wait for his approval before phase 6a.

## Notes for the executor

- **Not built ahead of time.** Unlike the Antiope phases, the blocks of this phase were written against the source as it stood on 2026-10-16 (`main` at `17fbbc6`) and the plan index's phase 4 interfaces, without a scratch build. Where phase 4 landed a different spelling (an initialiser label, an `Edition` property), follow phase 4 and keep the behaviour asserted here.
- **`TestDatabase` and the edition tables.** The helpers here empty `edition_overflow`, `edition_entry` and `edition` themselves (`makeJournal`, `makeJournalState`), so they do not depend on whether phase 4a made `TestDatabase.init` clear them.
- **External writes and live queries.** GRDB only sees its own writes: after `TestDatabase` (a second connection) seeds an edition, `JournalModel.edition` changes only after `AppDatabase.notifyExternalChanges()` (called by hand in `the_journal_lists_the_edition_in_rank_order`, and by `RunCoordinator.onDatabaseChange` after every run). Lists restarted by a selection change read the file afresh and need no notice.
- **`sqlite3` in the stub** is `/usr/bin/sqlite3`, present on every macOS; `-cmd ".timeout 5000"` waits for the app's connections instead of failing on `SQLITE_BUSY`.
- **The banner of an exit `2`.** Phase 4b makes exit `2` from `edition build` a refusal: `RunCoordinator` shows its stderr line without Show Log. The offline case shows `The network is unreachable: no edition built.` above the Journal's `Offline: no edition yet`, which is intended; the rare "built meanwhile in a terminal" case shows no banner: `JournalModel` takes the coordinator's refusal banner down when its text is exactly `JournalModel.builtElsewhere` (spec §9, "the app simply reloads the Journal").
- **⌘R in the Journal** refreshes every feed and never rebuilds the edition: the edition stays frozen until tomorrow (spec §2), and what ⌘R fetches goes into tomorrow's edition (Review Focus 2).
- **Antiope until phase 6.** Journal and the rhythm groups have no page number yet (`P---`); the entry column lists their rows (the Journal in rank order) and the reader works. Phase 6a gives them 104–109.
