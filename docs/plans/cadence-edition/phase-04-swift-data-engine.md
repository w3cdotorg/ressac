# Phase 4: The app reads schema v4 (4a) and drives the edition and the override (4b)

Read the plan index (`docs/plans/2026-10-16-cadence-edition.md`) first; its Global Constraints apply. This phase implements the app's data layer of spec §8 ("Data layer"): the records and live queries over the v4 contract views (4a), and the run requests and events for `ressac edition build` and `ressac feed cadence` (4b). Review Focus #3 (a buried feed leaves the rhythm lists at once) is pinned in 4a.

**Changes from the plan index** (names unchanged, placement made explicit):
- The new types (`CadenceGroup`, `EditionSection`, `EditionInfo`, `EditionItem`, `EditionOverflowItem`, `Edition`, `CadenceRow`, `EditionClock`) go in a **new file `Data/Edition.swift`**, not in `Records.swift` (301 lines already). The three record types also conform to `RessacRecord` (which implies the `Equatable, Sendable` the index asks for), so GRDB decodes them like every other row.
- `Queries.edition()`, `Queries.cadenceRows()` and `Queries.isEditionDue(dayStart:)` are added **inside `struct Queries`** in `Queries.swift`, next to `fetchLog`, not in an `extension`: same names and signatures.
- The `RunRequest` factories are added **inside `struct RunRequest`**, after `migrate()`: same names and signatures.
- 4b also edits `RunCoordinator.swift`: its two `switch event` statements are exhaustive over `EngineEvent`, so the two new cases must be listed there (otherwise the build fails).

---

## Sub-phase 4a: records and live queries over schema v4

**Runs after:** phase 1b (the v4 contract fixture). Run it **right after 1b**, before 1c: from 1b the fixture is at v4 while the app still accepts only v3, so every Swift test that opens a copy of the fixture fails with `.tooNew(4)` until this sub-phase lands.

**Files (5):**
- Modify: `apps/macos/Ressac/Data/Database.swift` (`knownSchemaVersion = 4`, its doc comment)
- Create: `apps/macos/Ressac/Data/Edition.swift`
- Modify: `apps/macos/Ressac/Data/Queries.swift` (three new static functions)
- Modify: `apps/macos/RessacTests/DataTests.swift` (`TestDatabase` reset, version tests, helpers, `EditionQueryTests`, `EditionClockTests`)
- Modify: `apps/macos/RessacTests/AppStateTests.swift` (`LauncherTests.newer_database_is_refused`: a "newer" database is now v5)
- Fixture (not counted, not modified here): `tests/fixtures/contract.sqlite`, regenerated at v4 by phase 1b.

**Step 0:** nothing to clean. `Database.swift` (90 lines) and `Queries.swift` (282) are under 300; `DataTests.swift` and `AppStateTests.swift` are over 300 but this sub-phase only adds tests and changes literals, it restructures nothing.

**Builds on (phase 1b, exact):**
- `schema_meta.version = 4`; tables `edition(id, built_at, window_start, window_end, unreachable)`, `edition_entry(edition_id, entry_id, section, rank)`, `edition_overflow(edition_id, feed_id, section, hidden_count)`; columns `feed.cadence_override`, `feed_health.cadence_group` (`NOT NULL DEFAULT 'silent'`).
- `v_cadence(feed_id, title, page, folder_path, effective_group, computed_group, is_override, median_interval_days, entries_per_month, last_entry_at, unread)`, active feeds joined to `feed_health` (a feed with no diagnosis has no row).
- `v_edition_today(edition_id, built_at, window_start, window_end, unreachable, entry_id, section, rank, feed_id, feed_title, title, published_at, read_at, starred_at)` and `v_edition_overflow(edition_id, section, feed_id, feed_title, page, hidden_count)`, both for `MAX(edition.id)`.
- Timestamps written by the core in the `Timestamp` format (RFC 3339 UTC with milliseconds and `Z`), so text comparisons on `built_at` order like dates.

**Test command** (the `-only-testing` list changes per step, as written in the step):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/DatabaseTests -only-testing:RessacTests/LauncherTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

### Task 4a.1: The app accepts schema v4

- [ ] **Step 1: Check the fixture phase 1b left**

```sh
cd "$(git rev-parse --show-toplevel)"
sqlite3 tests/fixtures/contract.sqlite "SELECT version FROM schema_meta;
  SELECT name FROM sqlite_master WHERE name IN ('edition','edition_entry','edition_overflow','v_cadence','v_edition_today','v_edition_overflow') ORDER BY name;
  SELECT group_concat(name, ',') FROM pragma_table_info('v_cadence');
  SELECT group_concat(name, ',') FROM pragma_table_info('v_edition_today');"
```

Expected: `4`, then `edition`, `edition_entry`, `edition_overflow`, `v_cadence`, `v_edition_overflow`, `v_edition_today`, then `feed_id,title,page,folder_path,effective_group,computed_group,is_override,median_interval_days,entries_per_month,last_entry_at,unread`, then `edition_id,built_at,window_start,window_end,unreachable,entry_id,section,rank,feed_id,feed_title,title,published_at,read_at,starred_at`. **If the version is 3 or a column differs, stop:** phase 1b disagrees with the plan index, and the fix belongs there.

- [ ] **Step 2: Write the failing tests**

In `apps/macos/RessacTests/DataTests.swift`, replace the doc comment and the reset SQL of `TestDatabase.init` (lines 9–12 and the `try raw.write { … }` block) with:

```swift
/// A private copy of `tests/fixtures/contract.sqlite` reduced to its schema (v4) and the
/// 7 folders and 11 feeds of `vienna-like.opml`, all active, with the page numbers the core
/// gave them: entries, full texts, diagnoses, fetch logs, decisions, editions and rhythm
/// overrides are cleared, so each test seeds what it asserts on.
```

```swift
        try raw.write { db in
            try db.execute(sql: """
                DELETE FROM edition_overflow; DELETE FROM edition_entry; DELETE FROM edition;
                DELETE FROM entry_fts; DELETE FROM entry_fulltext; DELETE FROM entry;
                DELETE FROM feed_health; DELETE FROM fetch_log; DELETE FROM garden_decision;
                UPDATE feed SET state = 'active', merged_into_id = NULL, prefer_full_text = 0, cadence_override = NULL;
                """)
        }
```

In `DatabaseTests`, replace `newer_database_is_refused` and `an_older_app_refuses_a_v3_database` with the three tests below (keep `a_v2_database_requests_migration` as it is):

```swift
    @Test func newer_database_is_refused() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 5")
        #expect(try AppDatabase.check(at: test.url) == .tooNew(5))
    }

    /// Review Focus #3 of sub-project 4, carried to v4: a database the v4 core migrated, opened
    /// by an app that knows v3, gets "This database was created by a newer Ressac".
    @Test func an_older_app_refuses_a_v4_database() throws {
        let test = try TestDatabase()
        #expect(AppDatabase.knownSchemaVersion == 4)
        #expect(try AppDatabase.schemaVersion(at: test.url) == 4)
        #expect(try AppDatabase.check(at: test.url, known: 3) == .tooNew(4))
        #expect(try AppDatabase.check(at: test.url, known: 4) == .ok)
        #expect(try AppDatabase.check(at: test.url, known: 5) == .needsMigration(4))
    }

    /// Every database made before this sub-project is at v3: the app runs `ressac migrate` first.
    @Test func a_v3_database_requests_migration() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 3")
        #expect(try AppDatabase.check(at: test.url) == .needsMigration(3))
    }
```

In `apps/macos/RessacTests/AppStateTests.swift`, `LauncherTests.newer_database_is_refused` (around line 303) becomes:

```swift
    @Test func newer_database_is_refused() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 5")
        let launcher = Launcher()
        await launcher.launch(database: test.url, executable: try Stub("exit 0").executable)
        guard case .tooNew(5) = launcher.phase else {
            Issue.record("expected tooNew(5), got \(launcher.phase)")
            return
        }
    }
```

- [ ] **Step 3: Run red** (test command above, `-only-testing:RessacTests/DatabaseTests -only-testing:RessacTests/LauncherTests`). Expected, with `knownSchemaVersion` still 3 and the fixture at v4: `an_older_app_refuses_a_v4_database` fails on `(AppDatabase.knownSchemaVersion → 3) == 4`, `check_reports_missing_and_current_databases` fails with `.tooNew(4)` instead of `.ok`, `a_v3_database_requests_migration` fails with `.ok` instead of `.needsMigration(3)`, then `** TEST FAILED **`.

- [ ] **Step 4: Implement**

In `apps/macos/Ressac/Data/Database.swift`:

```swift
    /// Schema version this build understands (`schema_meta.version`): v3 adds page numbers,
    /// v4 the rhythm groups and the morning edition.
    static let knownSchemaVersion = 4
```

- [ ] **Step 5: Run green**, same command. Expected: `✔ Test run with … tests in 2 suites passed`, `** TEST SUCCEEDED **` (`DatabaseTests` has 8 tests). Then run the other suites that open a copy of the fixture, `-only-testing:RessacTests/QueryTests -only-testing:RessacTests/WriteTests -only-testing:RessacTests/FirstRunWALTests`: all pass (they failed with `.tooNew(4)` since phase 1b).

- [ ] **Step 6: Commit**

```bash
cd "$(git rev-parse --show-toplevel)"
git add apps/macos/Ressac/Data/Database.swift apps/macos/RessacTests/DataTests.swift apps/macos/RessacTests/AppStateTests.swift
git commit -F - <<'EOF'
App: accept schema v4 (rhythm groups and the morning edition)

The test database also clears editions and rhythm overrides, so each test
seeds what it asserts on.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4a.2: Rhythm and edition records, live queries, the 04:00 clock

- [ ] **Step 1: Write the failing tests**

Append to `apps/macos/RessacTests/DataTests.swift` (after the `// MARK: - Records and queries` helpers, at the end of the file):

```swift
// MARK: - Rhythm and edition (schema v4)

extension TestDatabase {
    /// A diagnosis row with a rhythm group, as `save_diagnoses` writes it.
    func health(feed: Int64, group: CadenceGroup, median: Double? = nil, lastEntry: String? = nil) throws {
        try execute("""
            INSERT INTO feed_health (feed_id, status, reason, computed_at, cadence_group,
                                     median_interval_days, entries_per_month, last_entry_at)
            VALUES (?, 'Alive', 'Fine.', '2026-10-16T02:00:00.000Z', ?, ?, 0, ?)
            """, [feed, group.rawValue, median, lastEntry])
    }

    /// An edition and its rows as `write_edition` stores them: `rank` follows the order of
    /// `entries`, `window_end` is `builtAt`.
    @discardableResult
    func edition(
        builtAt: String, windowStart: String = "2026-10-15T02:00:00.000Z", unreachable: Int = 0,
        entries: [(Int64, EditionSection)] = [], overflow: [(feed: Int64, section: EditionSection, hidden: Int)] = []
    ) throws -> Int64 {
        try raw.write { db in
            try db.execute(
                sql: "INSERT INTO edition (built_at, window_start, window_end, unreachable) VALUES (?, ?, ?, ?)",
                arguments: [builtAt, windowStart, builtAt, unreachable])
            let id = db.lastInsertedRowID
            for (index, (entry, section)) in entries.enumerated() {
                try db.execute(
                    sql: "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (?, ?, ?, ?)",
                    arguments: [id, entry, section.rawValue, index + 1])
            }
            for item in overflow {
                try db.execute(
                    sql: "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (?, ?, ?, ?)",
                    arguments: [id, item.feed, item.section.rawValue, item.hidden])
            }
            return id
        }
    }
}

@Suite struct EditionQueryTests {
    @Test func rhythm_groups_and_sections_have_their_titles_and_pages() {
        #expect(CadenceGroup.allCases.map(\.rawValue) == ["daily", "weekly", "monthly", "rare", "silent"])
        #expect(CadenceGroup.allCases.map(\.title) == ["Daily", "Weekly", "Monthly", "Rare", "Silent"])
        #expect(CadenceGroup.allCases.map(\.page) == [105, 106, 107, 108, 109])
        #expect(EditionSection.allCases.map(\.rawValue) == ["front", "week", "wire"])
        #expect(EditionSection.allCases.map(\.title) == ["Front Page", "This Week", "The Wire"])
        #expect(EditionSection.allCases.map(\.antiopeTitle) == ["FRONT PAGE", "THIS WEEK", "THE WIRE"])
    }

    @Test func cadence_rows_carry_the_effective_and_the_computed_group() async throws {
        let test = try TestDatabase()
        try test.health(feed: 1, group: .daily, median: 0.4, lastEntry: "2026-10-16T06:00:00.000Z")
        try test.health(feed: 2, group: .weekly)
        try test.execute("UPDATE feed SET cadence_override = 'rare' WHERE id = 2")
        try test.entry(feed: 2)
        try test.entry(feed: 2, read: true)
        let rows = try await value(of: Queries.cadenceRows(), in: test.open())
        // Only diagnosed feeds have a rhythm: `v_cadence` joins `feed_health`.
        #expect(Set(rows.map(\.feedId)) == [1, 2])
        let one = try #require(rows.first { $0.feedId == 1 })
        #expect(one.effective == .daily && one.computed == .daily && !one.isOverride)
        #expect(one.medianIntervalDays == 0.4)
        #expect(one.lastEntryAt == Timestamp.date(from: "2026-10-16T06:00:00.000Z"))
        #expect(one.entriesPerMonth == 0 && one.unread == 0)
        let two = try #require(rows.first { $0.feedId == 2 })
        #expect(two.effective == .rare && two.computed == .weekly && two.isOverride)
        #expect(two.medianIntervalDays == nil && two.lastEntryAt == nil)
        #expect(two.unread == 1)
    }

    /// Review Focus #3: burying a feed takes it out of the rhythm lists at once.
    @Test func cadence_rows_drop_a_feed_as_soon_as_it_is_buried() async throws {
        let test = try TestDatabase()
        try test.health(feed: 1, group: .daily)
        try test.health(feed: 2, group: .weekly)
        let db = try test.open()
        #expect(try await value(of: Queries.cadenceRows(), in: db).map(\.feedId).sorted() == [1, 2])
        try test.execute("UPDATE feed SET state = 'buried' WHERE id = 2")
        try await db.notifyExternalChanges()
        let rows = try await value(of: Queries.cadenceRows(), in: db) { $0.count == 1 }
        #expect(rows.map(\.feedId) == [1])
    }

    @Test func no_edition_reads_as_nil() async throws {
        let test = try TestDatabase()
        #expect(try await value(of: Queries.edition(), in: test.open()) == nil)
    }

    @Test func the_latest_edition_reads_in_rank_order_with_its_overflow() async throws {
        let test = try TestDatabase()
        let a = try test.entry(feed: 1, title: "A", published: "2026-10-15T20:00:00.000Z")
        let b = try test.entry(feed: 3, title: "B", read: true)
        let c = try test.entry(feed: 8, title: "C", starred: true)
        let old = try test.entry(feed: 9, title: "Old")
        try test.edition(builtAt: "2026-10-15T06:00:00.000Z", entries: [(old, .wire)])
        // Ranks run against the entry ids, so only ORDER BY rank gives c, a, b.
        try test.edition(
            builtAt: "2026-10-16T06:12:00.000Z", windowStart: "2026-10-15T06:00:00.000Z", unreachable: 3,
            entries: [(c, .front), (a, .week), (b, .wire)],
            overflow: [(feed: 8, section: .wire, hidden: 12), (feed: 2, section: .front, hidden: 1),
                       (feed: 3, section: .wire, hidden: 4)])
        let edition = try #require(try await value(of: Queries.edition(), in: test.open()))
        #expect(edition.info.unreachable == 3)
        #expect(edition.info.builtAt == Timestamp.date(from: "2026-10-16T06:12:00.000Z"))
        #expect(edition.info.windowStart == Timestamp.date(from: "2026-10-15T06:00:00.000Z"))
        #expect(edition.info.windowEnd == edition.info.builtAt)
        #expect(edition.items.map(\.entryId) == [c, a, b])
        #expect(edition.items.map(\.rank) == [1, 2, 3])
        #expect(edition.items.map(\.section) == [.front, .week, .wire])
        #expect(edition.items.map(\.title) == ["C", "A", "B"])
        #expect(edition.items.map(\.feedId) == [8, 1, 3])
        #expect(edition.items[0].isStarred && !edition.items[0].isRead)
        #expect(edition.items[2].isRead)
        #expect(edition.items[1].publishedAt == Timestamp.date(from: "2026-10-15T20:00:00.000Z"))
        #expect(!edition.items[1].feedTitle.isEmpty)
        // Overflow: front before wire, then by page (feed 2 → 203, 3 → 204, 8 → 401).
        #expect(edition.overflow.map(\.feedId) == [2, 3, 8])
        #expect(edition.overflow.map(\.section) == [.front, .wire, .wire])
        #expect(edition.overflow.map(\.page) == [203, 204, 401])
        #expect(edition.overflow.map(\.hiddenCount) == [1, 4, 12])
        #expect(edition.readCount == 1 && !edition.isFinished)
    }

    @Test func an_edition_is_finished_when_every_entry_is_read_and_never_when_empty() async throws {
        let test = try TestDatabase()
        let a = try test.entry(feed: 1)
        let b = try test.entry(feed: 2)
        try test.edition(builtAt: "2026-10-16T06:00:00.000Z", entries: [(a, .front), (b, .wire)])
        let db = try test.open()
        #expect(try await value(of: Queries.edition(), in: db)?.isFinished == false)
        try test.execute("UPDATE entry SET read_at = '2026-10-16T07:00:00.000Z'")
        try await db.notifyExternalChanges()
        let read = try await value(of: Queries.edition(), in: db) { $0?.isFinished == true }
        #expect(read?.readCount == 2)
        try test.edition(builtAt: "2026-10-17T06:00:00.000Z")
        try await db.notifyExternalChanges()
        let empty = try #require(try await value(of: Queries.edition(), in: db) { $0?.items.isEmpty == true })
        #expect(empty.overflow.isEmpty && empty.readCount == 0 && !empty.isFinished)
    }

    @Test func an_edition_is_due_until_one_is_built_after_the_day_start() async throws {
        let test = try TestDatabase()
        let db = try test.open()
        let dayStart = try #require(Timestamp.date(from: "2026-10-16T02:00:00.000Z"))
        #expect(try await db.reader.read(Queries.isEditionDue(dayStart: dayStart)) == true)
        try test.edition(builtAt: "2026-10-16T01:59:59.999Z")
        #expect(try await db.reader.read(Queries.isEditionDue(dayStart: dayStart)) == true)
        try test.edition(builtAt: "2026-10-16T02:00:00.000Z")
        #expect(try await db.reader.read(Queries.isEditionDue(dayStart: dayStart)) == false)
        let tomorrow = try #require(Timestamp.date(from: "2026-10-17T02:00:00.000Z"))
        #expect(try await db.reader.read(Queries.isEditionDue(dayStart: tomorrow)) == true)
    }
}

/// `EditionClock.dayStart` mirrors the core's `edition::day_start` (Review Focus #1 of the
/// plan index pins the Rust side; these pin the same instants on the Swift side).
@Suite struct EditionClockTests {
    let paris: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Paris")!
        return calendar
    }()

    @Test func the_day_starts_at_four_local_time_across_both_dst_changes() throws {
        // (now, expected day start), all in UTC.
        let cases: [(String, String)] = [
            ("2026-10-16T01:30:00Z", "2026-10-15T02:00:00Z"),  // 03:30 CEST: still yesterday
            ("2026-10-16T02:00:00Z", "2026-10-16T02:00:00Z"),  // 04:00 CEST exactly
            ("2026-10-16T08:00:00Z", "2026-10-16T02:00:00Z"),  // 10:00 CEST
            ("2026-10-25T02:30:00Z", "2026-10-24T02:00:00Z"),  // 03:30 CET, after 03:00 CEST → 02:00 CET
            ("2026-10-25T03:30:00Z", "2026-10-25T03:00:00Z"),  // 04:30 CET
            ("2026-03-29T01:30:00Z", "2026-03-28T03:00:00Z"),  // 03:30 CEST, after 02:00 CET → 03:00 CEST
            ("2026-03-29T02:30:00Z", "2026-03-29T02:00:00Z"),  // 04:30 CEST
        ]
        for (now, expected) in cases {
            let start = EditionClock.dayStart(for: try #require(Timestamp.date(from: now)), calendar: paris)
            #expect(start == Timestamp.date(from: expected), "\(now)")
        }
    }
}
```

- [ ] **Step 2: Run red** with `-only-testing:RessacTests/EditionQueryTests -only-testing:RessacTests/EditionClockTests`. Expected: `error: cannot find type 'CadenceGroup' in scope` (and `EditionSection`, `Queries` has no member `cadenceRows`, `EditionClock`), then `** TEST FAILED **`.

- [ ] **Step 3: Create `apps/macos/Ressac/Data/Edition.swift`**

```swift
import Foundation
import GRDB

/// The core's `CadenceGroup`: a feed's rhythm, computed (`feed_health.cadence_group`) or set by
/// Clément (`feed.cadence_override`). Spec §4.
enum CadenceGroup: String, CaseIterable, Sendable, Codable, Hashable {
    case daily, weekly, monthly, rare, silent

    var title: String {
        switch self {
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        case .rare: "Rare"
        case .silent: "Silent"
        }
    }

    /// The group's reserved Antiope page (spec §5).
    var page: Int {
        switch self {
        case .daily: 105
        case .weekly: 106
        case .monthly: 107
        case .rare: 108
        case .silent: 109
        }
    }
}

/// The core's `edition::Section`, in reading order.
enum EditionSection: String, CaseIterable, Sendable, Codable {
    case front, week, wire

    var title: String {
        switch self {
        case .front: "Front Page"
        case .week: "This Week"
        case .wire: "The Wire"
        }
    }

    var antiopeTitle: String { title.uppercased() }
}

/// The header of an edition (`edition` table).
struct EditionInfo: RessacRecord {
    var id: Int64
    var builtAt: Date
    var windowStart: Date
    var windowEnd: Date
    /// Feeds whose fetch failed while the edition was built.
    var unreachable: Int
}

/// One entry of an edition (`v_edition_today`), with its live read and starred state.
struct EditionItem: RessacRecord, Identifiable {
    var entryId: Int64
    var section: EditionSection
    /// 1-based across the whole edition: the Antiope sub-page number.
    var rank: Int
    var feedId: Int64
    var feedTitle: String
    var title: String?
    var publishedAt: Date
    var isRead: Bool
    var isStarred: Bool

    var id: Int64 { entryId }
}

/// "+12 more in Actualitté": entries a cap left unread in their feed (`v_edition_overflow`).
struct EditionOverflowItem: RessacRecord {
    var section: EditionSection
    var feedId: Int64
    var feedTitle: String
    var page: Int?
    var hiddenCount: Int
}

/// The latest edition: items by rank, overflow by section then page.
struct Edition: Equatable, Sendable {
    var info: EditionInfo
    var items: [EditionItem]
    var overflow: [EditionOverflowItem]

    var readCount: Int { items.count(where: \.isRead) }

    /// Every entry read. An empty edition is never "finished": it says "Nothing new this morning."
    var isFinished: Bool { !items.isEmpty && readCount == items.count }
}

/// One active, diagnosed feed's rhythm (`v_cadence`).
struct CadenceRow: RessacRecord, Identifiable {
    var feedId: Int64
    var effective: CadenceGroup
    var computed: CadenceGroup
    var isOverride: Bool
    var medianIntervalDays: Double?
    var entriesPerMonth: Double
    var lastEntryAt: Date?
    var unread: Int

    var id: Int64 { feedId }
}

/// When a morning edition is due: the day starts at 04:00 local time (spec §6, "When").
enum EditionClock {
    static let hour = 4

    /// 04:00 local of the day `now` belongs to: before 04:00, the previous day's 04:00.
    /// Mirrors the core's `edition::day_start`, DST changes included.
    static func dayStart(for now: Date, calendar: Calendar = .current) -> Date {
        let today = fourOClock(on: now, calendar)
        if now >= today { return today }
        let yesterday = calendar.date(byAdding: .day, value: -1, to: now) ?? now.addingTimeInterval(-86_400)
        return fourOClock(on: yesterday, calendar)
    }

    private static func fourOClock(on day: Date, _ calendar: Calendar) -> Date {
        calendar.date(bySettingHour: hour, minute: 0, second: 0, of: day)
            ?? calendar.startOfDay(for: day).addingTimeInterval(TimeInterval(hour * 3_600))
    }
}
```

- [ ] **Step 4: Add the queries** inside `struct Queries` in `apps/macos/Ressac/Data/Queries.swift`, right after `fetchLog(feedId:)`:

```swift
    /// The latest edition (spec §8, Journal screen), nil before the first one. Its header comes
    /// from `edition` itself: an empty edition has no row in `v_edition_today`.
    static func edition() -> LiveQuery<Edition?> {
        ValueObservation.tracking { db -> Edition? in
            guard let info = try EditionInfo.fetchOne(db, sql: """
                SELECT id, built_at, window_start, window_end, unreachable FROM edition ORDER BY id DESC LIMIT 1
                """)
            else { return nil }
            let items = try EditionItem.fetchAll(db, sql: """
                SELECT entry_id, section, rank, feed_id, feed_title, title, published_at,
                       read_at IS NOT NULL AS is_read, starred_at IS NOT NULL AS is_starred
                FROM v_edition_today ORDER BY rank
                """)
            let overflow = try EditionOverflowItem.fetchAll(db, sql: """
                SELECT section, feed_id, feed_title, page, hidden_count FROM v_edition_overflow
                ORDER BY CASE section WHEN 'front' THEN 0 WHEN 'week' THEN 1 ELSE 2 END, page IS NULL, page, feed_id
                """)
            return Edition(info: info, items: items, overflow: overflow)
        }
        .removeDuplicates()
    }

    /// Every active, diagnosed feed's rhythm, by title (spec §8, Rhythm section and inspector).
    static func cadenceRows() -> LiveQuery<[CadenceRow]> {
        ValueObservation.tracking { db in
            try CadenceRow.fetchAll(db, sql: """
                SELECT feed_id, effective_group AS effective, computed_group AS computed, is_override,
                       median_interval_days, entries_per_month, last_entry_at, unread
                FROM v_cadence ORDER BY title COLLATE NOCASE, feed_id
                """)
        }
        .removeDuplicates()
    }

    /// True while no edition was built at or after `dayStart` (see `EditionClock.dayStart`).
    /// The core repeats this check inside its write transaction.
    static func isEditionDue(dayStart: Date) -> @Sendable (Database) throws -> Bool {
        let start = Timestamp.string(from: dayStart)
        return { db in
            try Bool.fetchOne(db, sql: "SELECT NOT EXISTS (SELECT 1 FROM edition WHERE built_at >= ?)",
                              arguments: [start]) ?? true
        }
    }
```

- [ ] **Step 5: Run green** with `-only-testing:RessacTests/EditionQueryTests -only-testing:RessacTests/EditionClockTests`. Expected: `✔ Test run with 8 tests in 2 suites passed`, `** TEST SUCCEEDED **`. Then `-only-testing:RessacTests/QueryTests -only-testing:RessacTests/DatabaseTests`: all pass.

If `cadence_rows_carry_the_effective_and_the_computed_group` fails on `lastEntryAt` for the NULL row with a decoding error, GRDB applied the custom date strategy to NULL: decode `last_entry_at` by hand in a `CadenceRow.init(row:)` (as `EntryRow` does), keeping the same properties. Declare that initialiser in an `extension CadenceRow`, never in the struct body: phases 5 and 6 build `CadenceRow`, `EditionItem`, `EditionInfo` and `EditionOverflowItem` with their memberwise initialisers, which an initialiser in the body would remove.

- [ ] **Step 6: Commit**

```bash
cd "$(git rev-parse --show-toplevel)"
git add apps/macos/Ressac/Data/Edition.swift apps/macos/Ressac/Data/Queries.swift apps/macos/RessacTests/DataTests.swift
git commit -F - <<'EOF'
App: read rhythm groups and the latest edition from the v4 views

CadenceGroup, EditionSection, the edition records, Queries.edition,
Queries.cadenceRows, Queries.isEditionDue and EditionClock (04:00 local,
DST included). A buried feed leaves the rhythm rows at once.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### 4a verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd apps/macos && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | \
  grep -E 'warning:|error:|✘|Test run|\*\* TEST'
```

Expected: Rust unchanged and green; `** TEST SUCCEEDED **` for the whole Swift suite (every test that failed with `.tooNew(4)` since phase 1b passes again), no `warning:` line from `apps/macos/Ressac` or `apps/macos/RessacTests`. Stop here for Clément's approval before phase 1c.

---

## Sub-phase 4b: run requests and events for the edition and the override

**Runs after:** phase 3b (`ressac edition build`, `ressac feed cadence` and their NDJSON events exist in the bundled binary) and 4a (`CadenceGroup`).

**Files (5):**
- Modify: `apps/macos/Ressac/Engine/RessacCLI.swift` (`RunRequest.buildEdition`, `RunRequest.setCadence`, `isRefusal`)
- Modify: `apps/macos/Ressac/Engine/Events.swift` (`EngineEvent.edition`, `EngineEvent.cadence`)
- Modify: `apps/macos/Ressac/App/RunCoordinator.swift` (the two exhaustive `switch event`)
- Modify: `apps/macos/RessacTests/EngineTests.swift`
- Modify: `apps/macos/RessacTests/RunCoordinatorTests.swift`

**Step 0:** nothing to clean (225, 92, 170 lines; the test files gain tests only).

**Builds on (phases 3a and 3b, exact):**
- `ressac --db P --json edition build [--rebuild]` prints the `fetch` events, then `{"event":"edition","id":12,"entries":43,"front":2,"week":7,"wire":34,"overflow":61,"unreachable":3}`; exit `2` with `An edition was already built this morning.` or `The network is unreachable: no edition built.` on stderr.
- `ressac --db P --json feed cadence ID auto|daily|weekly|monthly|rare|silent` prints `{"event":"cadence","feed_id":12,"group":"daily","computed":"weekly","override":true}`; exit `2` with `No feed #N.` or `Feed #N is buried.`
- `EngineEvent.decode(line:)` keeps an unknown object as `.unknown(line)`; `RunCoordinator.execute` shows `failure.message` in the banner with `showLog: !request.isRefusal(failure)`.

**Test command:** as in 4a with `-only-testing:RessacTests/RunRequestTests -only-testing:RessacTests/EventTests -only-testing:RessacTests/RunCoordinatorTests`.

### Task 4b.1: `buildEdition`, `setCadence` and their refusals

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/EngineTests.swift`, add at the end of `RunRequestTests.factories_build_the_cli_arguments`:

```swift
        #expect(RunRequest.buildEdition() == RunRequest(arguments: ["edition", "build"], kind: .long))
        #expect(RunRequest.buildEdition(rebuild: true) == RunRequest(arguments: ["edition", "build", "--rebuild"], kind: .long))
        #expect(RunRequest.setCadence(feedId: 12, .monthly) == RunRequest(arguments: ["feed", "cadence", "12", "monthly"], kind: .short))
        #expect(RunRequest.setCadence(feedId: 12, nil) == RunRequest(arguments: ["feed", "cadence", "12", "auto"], kind: .short))
        #expect(RunRequest.buildEdition().showsFailureBanner && RunRequest.setCadence(feedId: 1, nil).showsFailureBanner)
```

In `apps/macos/RessacTests/RunCoordinatorTests.swift`, replace `only_garden_decisions_and_undo_can_be_refused` with:

```swift
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
```

- [ ] **Step 2: Run red** (`-only-testing:RessacTests/RunRequestTests -only-testing:RessacTests/RunCoordinatorTests`). Expected: `error: type 'RunRequest' has no member 'buildEdition'` (and `setCadence`), `** TEST FAILED **`.

- [ ] **Step 3: Implement** in `apps/macos/Ressac/Engine/RessacCLI.swift`, inside `struct RunRequest`, after `migrate()`:

```swift
    /// Refresh, then compose today's edition (spec §6). Long: it fetches every due feed.
    static func buildEdition(rebuild: Bool = false) -> RunRequest {
        RunRequest(arguments: ["edition", "build"] + (rebuild ? ["--rebuild"] : []), kind: .long)
    }

    /// Forces a feed's rhythm group, or gives it back to the core with `nil` ("auto").
    static func setCadence(feedId: Int64, _ group: CadenceGroup?) -> RunRequest {
        RunRequest(arguments: ["feed", "cadence", String(feedId), group?.rawValue ?? "auto"], kind: .short)
    }
```

and replace `isRefusal` (and its doc comment) with:

```swift
    /// True when `failure` is the store refusing something the app cannot foresee (exit 2): a
    /// garden decision or an undo (an address another active feed already uses), an edition
    /// that is not due or cannot be built offline, an override on a feed buried meanwhile.
    /// From any other call, exit 2 means the app built a wrong command line (spec §8).
    func isRefusal(_ failure: RunFailure) -> Bool {
        failure.exitCode == 2 && Self.refusable.contains { arguments.starts(with: $0) }
    }

    private static let refusable: [[String]] = [
        ["garden", "decide"], ["garden", "undo"], ["edition", "build"], ["feed", "cadence"],
    ]
```

- [ ] **Step 4: Run green**, same command. Expected: `✔ Test run with … tests in 2 suites passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
cd "$(git rev-parse --show-toplevel)"
git add apps/macos/Ressac/Engine/RessacCLI.swift apps/macos/RessacTests/EngineTests.swift apps/macos/RessacTests/RunCoordinatorTests.swift
git commit -F - <<'EOF'
App: run requests for the morning edition and the rhythm override

edition build and feed cadence refusals (exit 2) show the CLI's sentence
without Show log.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4b.2: The `edition` and `cadence` events

- [ ] **Step 1: Write the failing tests**

In `apps/macos/RessacTests/EngineTests.swift`, add to the `cases` array of `EventTests.every_core_event_decodes`:

```swift
            (#"{"event":"edition","id":12,"entries":43,"front":2,"week":7,"wire":34,"overflow":61,"unreachable":3}"#,
             .edition(id: 12, entries: 43, overflow: 61, unreachable: 3)),
            (#"{"event":"edition","id":13,"entries":0,"front":0,"week":0,"wire":0,"overflow":0,"unreachable":0}"#,
             .edition(id: 13, entries: 0, overflow: 0, unreachable: 0)),
            (#"{"event":"cadence","feed_id":12,"group":"daily","computed":"weekly","override":true}"#,
             .cadence(feedId: 12, group: "daily", isOverride: true)),
            (#"{"event":"cadence","feed_id":12,"group":"weekly","computed":"weekly","override":false}"#,
             .cadence(feedId: 12, group: "weekly", isOverride: false)),
```

and to `unknown_objects_are_kept_and_garbage_is_dropped`:

```swift
        // An edition line without its id is not an edition this build can use.
        #expect(EngineEvent.decode(line: #"{"event":"edition","entries":3}"#) == .unknown(#"{"event":"edition","entries":3}"#))
```

In `apps/macos/RessacTests/RunCoordinatorTests.swift`, add:

```swift
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
```

- [ ] **Step 2: Run red** (`-only-testing:RessacTests/EventTests -only-testing:RessacTests/RunCoordinatorTests`). Expected: `error: type 'EngineEvent' has no member 'edition'` (and `cadence`), `** TEST FAILED **`.

- [ ] **Step 3: Implement** in `apps/macos/Ressac/Engine/Events.swift`. Add the cases after `undone`:

```swift
    /// The edition `edition build` wrote (the core's `Event::Edition`); `front`, `week` and
    /// `wire` are read from the database by the Journal, not from here.
    case edition(id: Int64, entries: Int, overflow: Int, unreachable: Int)
    /// `feed cadence`: the feed's effective group and whether Clément set it.
    case cadence(feedId: Int64, group: String, isOverride: Bool)
```

Extend `CodingKeys`:

```swift
    private enum CodingKeys: String, CodingKey {
        case event, total, title, error, failed, attempted, what, found, summary, ok, decision
        case id, entries, overflow, unreachable, group
        case feedId = "feed_id"
        case entryId = "entry_id"
        case isOverride = "override"
    }
```

and add, before `default:` in `init(from:)`:

```swift
        case "edition":
            self = .edition(
                id: try c.decode(Int64.self, forKey: .id), entries: try c.decode(Int.self, forKey: .entries),
                overflow: try c.decode(Int.self, forKey: .overflow), unreachable: try c.decode(Int.self, forKey: .unreachable))
        case "cadence":
            self = .cadence(
                feedId: try c.decode(Int64.self, forKey: .feedId), group: try c.decode(String.self, forKey: .group),
                isOverride: try c.decode(Bool.self, forKey: .isOverride))
```

In `apps/macos/Ressac/App/RunCoordinator.swift`, list the two cases in both switches:

```swift
        case .waybackDone, .finished, .decided, .undone, .edition, .cadence, .unknown:
            break
```

```swift
        case .feedDone, .extracted, .decided, .undone, .finished, .edition, .cadence:
```

Then check no other exhaustive switch over `EngineEvent` exists: `grep -rn "case .undone\|case let .undone\|case .waybackDone" apps/macos/Ressac`. Expected: only `RunCoordinator.swift` and `Events.swift`.

- [ ] **Step 4: Run green**, same command. Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 5: Commit**

```bash
cd "$(git rev-parse --show-toplevel)"
git add apps/macos/Ressac/Engine/Events.swift apps/macos/Ressac/App/RunCoordinator.swift \
        apps/macos/RessacTests/EngineTests.swift apps/macos/RessacTests/RunCoordinatorTests.swift
git commit -F - <<'EOF'
App: decode the edition and cadence events

Both tell the observations to refetch, like the other events that write.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### 4b verification gate

The 4a gate commands (Rust gate, then the whole Swift suite). Expected: `** TEST SUCCEEDED **`, no warnings from our sources. `SmokeTests.bundled_helper_is_embedded_and_runs` proves the bundled `ressac` is the phase 3b core. Stop for Clément's approval before phase 5a.

**Manual check on a throwaway database** (no keystroke is sent to any app):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
SMOKE="$(mktemp -d)"; cp tests/fixtures/contract.sqlite "$SMOKE/r.sqlite"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
"$APP/Contents/Helpers/ressac" --db "$SMOKE/r.sqlite" --json feed cadence 1 rare
"$APP/Contents/Helpers/ressac" --db "$SMOKE/r.sqlite" --json feed cadence 1 auto
rm -rf "$SMOKE"
```

Expected: two `cadence` lines, `"override":true` then `"override":false`, in the shape listed under "Builds on". If a key differs, the core and the app disagree: fix `Events.swift` to the core's shape and its test with it.

### Notes for the executor

- **Not verified ahead of time.** Unlike the Antiope phases, this file was written without building: the Swift above follows the existing patterns (`RessacRecord`, `Timestamp`, `value(of:in:where:)`, `Stub`) read from the sources on 2026-10-16. Expect small compile fixes; keep names and test expectations.
- **`v_cadence` lists diagnosed feeds only** (inner join on `feed_health`). After the v4 migration every active feed is diagnosed, so in the app every feed has a row; in tests, seed `feed_health` with `TestDatabase.health`.
- **Why `isEditionDue` returns a closure.** `JournalModel` (5a) runs it with `database.reader.read(…)` at launch, on foreground and at the rollover, with `EditionClock.dayStart(for: clock(), calendar:)`; it never observes it live.
- **The "already built" refusal.** When another process built the edition first, `edition build` exits `2` with `An edition was already built this morning.` and 4b shows that sentence in the banner like any refusal. Spec §9 wants the app to "simply reload the Journal" in that case; that belongs to `JournalModel` (5a), which can clear `coordinator.banner` when the failure's `lastStderrLine` is that sentence. 4b exposes no helper for it, so the index needs no change.
- **The progress label.** A long run's progress reads `Fetching feeds` (from `started`); the Journal's `Building this morning's edition… 34/156` is 5a's to compose from `RunCoordinator.progress`.
