# Phase 7: GRDB database, records, observations, client writes

Read the plan index first. Global Constraints apply. Spec §4 (Data flow, Concurrency), §5 (schema v2 and its views), §7 (Sidebar sections, Search), §8 (Schema version, Database busy, Read-state write failure) and §9 (records and observations against a copy of the contract fixture).

**Files (5):**
- Create: `apps/macos/Ressac/Data/Database.swift`
- Create: `apps/macos/Ressac/Data/Records.swift`
- Create: `apps/macos/Ressac/Data/Queries.swift`
- Create: `apps/macos/Ressac/Data/Writes.swift`
- Create: `apps/macos/RessacTests/DataTests.swift`

**Test command:** as in phase 6, narrowed with `-only-testing:RessacTests/<Suite>`:

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/DatabaseTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**Fixture.** The tests copy `tests/fixtures/contract.sqlite` (regenerated at schema v2 by phase 5) into a temp directory. `TestDatabase` then reduces the copy to its schema plus the 7 folders and 11 feeds of `tests/fixtures/vienna-like.opml`: it empties `entry`, `entry_fts`, `entry_fulltext`, `feed_health`, `fetch_log` and `garden_decision`, and sets every feed back to `active` with `prefer_full_text = 0`. Each test then seeds exactly what it asserts on, through a second connection that stands in for the core. The phase 5 rows (entries, full texts, diagnoses, decisions, the buried feed) stay available to tests that open the fixture directly. Feed and folder ids follow the OPML import order. Folders: 1 Blogs (root), 2 potes (in 1), 3 Biblis, 4 Infos, 5 Science (in 4), 6 Jon, 7 YouTube (in 6). Feeds: 1 xkcd.com and 5 Girl on the Couch in folder 1; 2 Orange Drenka, 3 Orange-Drenka and 4 Photos from bouquiniste in folder 2; 6 and 7 in folder 3; 8 Daring Fireball and 9 in folder 4; 10 in folder 5; 11 in folder 7.

**Interface additions to the master** (all additive; the master's names are unchanged):
- `AppDatabase.writer` (the pool; `reader` returns it), `open(at:busyTimeout:)` with a 5 s default, `defaultURL(environment:)`, `notifyExternalChanges()` and `isBusy(_:)`.
- `FeedItem.htmlUrl` (for the feed context menu's Open Site) and `FetchLogRow.id`.
- `Queries.entry(id:)`. List rows from `entries`/`search` carry `contentHtml = nil` and at most 1,200 characters of `summary_html` (else `content_html`); the reader takes the complete row from `entry(id:)`. A large backlog would otherwise hold every article body in memory twice.
- `LiveQuery<T>`, `Queries.predicate(for:now:)` (shared with `Writes.markAllRead`), `Queries.ftsQuery(_:)`, `Source.isGarden`, `SidebarSnapshot`'s counts `unread`, `today`, `starred`, `needsTriage`.
- `Timestamp` formats with `Date.ISO8601FormatStyle` (a `Sendable` value) instead of `ISO8601DateFormatter`; the output is the same `2026-10-10T08:00:00.000Z`.

### Task 7.1: Open and check the shared database

**Interfaces:**
- Consumes: GRDB `DatabasePool`, `DatabaseQueue`, `Configuration.busyMode = .timeout(_:)`, `Configuration.readonly`, `Database.notifyChanges(in: .fullDatabase)`, `DatabaseError.resultCode`.
- Produces (`Data/Database.swift`):
  - `final class AppDatabase: Sendable` with `static let knownSchemaVersion = 2`, `let writer: any DatabaseWriter`, `var reader: any DatabaseReader`;
  - `enum SchemaCheck: Equatable, Sendable { case missing, needsMigration(Int), ok, tooNew(Int) }` (nested in `AppDatabase`);
  - `static func defaultURL(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL`;
  - `static func open(at url: URL, busyTimeout: TimeInterval = 5) throws -> AppDatabase`;
  - `static func schemaVersion(at url: URL) throws -> Int?`, `static func check(at url: URL) throws -> SchemaCheck`;
  - `func notifyExternalChanges() async throws`, `static func isBusy(_ error: any Error) -> Bool`.
  - Test helper `final class TestDatabase: Sendable` (`dir`, `url`, `raw`, `execute(_:_:)`, `one(_:_:_:)`, `holdWriteLock(for:sql:whenLocked:)`, `static makeV1(at:)`, `open(busyTimeout:)`, `entry(feed:title:url:published:summary:content:read:starred:body:)`), used again by phases 9 to 12.

- [ ] **Step 1: Write the failing tests** `apps/macos/RessacTests/DataTests.swift`. `opening_v1_database_requests_migration` pins Review Focus #4 on the Swift side:

```swift
import Foundation
import GRDB
import Testing

@testable import Ressac

// MARK: - Test database

/// A private copy of `tests/fixtures/contract.sqlite` reduced to its schema (v2) and the
/// 7 folders and 11 feeds of `vienna-like.opml`, all active: entries, full texts,
/// diagnoses, fetch logs and decisions are cleared, so each test seeds what it asserts on.
final class TestDatabase: Sendable {
    static let repoRoot = URL(filePath: #filePath)
        .deletingLastPathComponent()  // RessacTests
        .deletingLastPathComponent()  // apps/macos
        .deletingLastPathComponent()  // apps
        .deletingLastPathComponent()  // repo root
    static let fixture = repoRoot.appending(path: "tests/fixtures/contract.sqlite")

    let dir: URL
    let url: URL
    let raw: DatabaseQueue

    init() throws {
        dir = FileManager.default.temporaryDirectory.appending(path: "ressac-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        url = dir.appending(path: "ressac.sqlite")
        try FileManager.default.copyItem(at: Self.fixture, to: url)
        raw = try DatabaseQueue(path: url.path)
        try raw.write { db in
            try db.execute(sql: """
                DELETE FROM entry_fts; DELETE FROM entry_fulltext; DELETE FROM entry;
                DELETE FROM feed_health; DELETE FROM fetch_log; DELETE FROM garden_decision;
                UPDATE feed SET state = 'active', merged_into_id = NULL, prefer_full_text = 0;
                """)
        }
    }

    deinit {
        try? FileManager.default.removeItem(at: dir)
    }

    /// Raw SQL on a second connection, standing in for the core.
    func execute(_ sql: String, _ arguments: StatementArguments = []) throws {
        try raw.write { try $0.execute(sql: sql, arguments: arguments) }
    }

    func one<T: DatabaseValueConvertible>(_ type: T.Type, _ sql: String, _ arguments: StatementArguments = []) throws -> T? {
        try raw.read { try T.fetchOne($0, sql: sql, arguments: arguments) }
    }

    /// Holds a write transaction on the second connection, as the core does while it stores a feed.
    func holdWriteLock(for seconds: TimeInterval, sql: String = "SELECT 1", whenLocked: @Sendable () -> Void) throws {
        try raw.write { db in
            try db.execute(sql: sql)
            whenLocked()
            Thread.sleep(forTimeInterval: seconds)
        }
    }

    /// A database as sub-project 1 left it: `schema/0001_init.sql` only.
    static func makeV1(at url: URL) throws {
        let sql = try String(contentsOf: repoRoot.appending(path: "schema/0001_init.sql"), encoding: .utf8)
        try DatabaseQueue(path: url.path).write { try $0.execute(sql: sql) }
    }

    func open(busyTimeout: TimeInterval = 5) throws -> AppDatabase {
        try AppDatabase.open(at: url, busyTimeout: busyTimeout)
    }

    /// Inserts an entry and its FTS row (the core does both in one transaction).
    @discardableResult
    func entry(
        feed: Int64, title: String? = "Entry", url: String? = "https://example.org/a",
        published: String = "2026-10-01T08:00:00.000Z", summary: String? = "<p>Summary</p>",
        content: String? = nil, read: Bool = false, starred: Bool = false, body: String? = nil
    ) throws -> Int64 {
        try raw.write { db in
            try db.execute(
                sql: """
                    INSERT INTO entry (feed_id, guid, url, title, published_at, summary_html, content_html,
                                       read_at, starred_at, fetched_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '2026-10-10T08:00:00.000Z')
                    """,
                arguments: [
                    feed, UUID().uuidString, url, title, published, summary, content,
                    read ? "2026-10-10T08:00:00.000Z" : nil, starred ? "2026-10-10T08:00:00.000Z" : nil,
                ])
            let id = db.lastInsertedRowID
            try db.execute(
                sql: "INSERT INTO entry_fts (rowid, title, author, body) VALUES (?, ?, '', ?)",
                arguments: [id, title ?? "", body ?? content ?? summary ?? ""])
            return id
        }
    }
}

// MARK: - Database

@Suite struct DatabaseTests {
    @Test func check_reports_missing_and_current_databases() throws {
        let test = try TestDatabase()
        #expect(try AppDatabase.check(at: test.dir.appending(path: "nope.sqlite")) == .missing)
        #expect(try AppDatabase.schemaVersion(at: test.dir.appending(path: "nope.sqlite")) == nil)
        #expect(try AppDatabase.check(at: test.url) == .ok)
        #expect(!FileManager.default.fileExists(atPath: test.dir.appending(path: "nope.sqlite").path))
    }

    /// Review Focus #4: a sub-project 1 database must go through `ressac migrate`.
    @Test func opening_v1_database_requests_migration() throws {
        let test = try TestDatabase()
        let v1 = test.dir.appending(path: "v1.sqlite")
        try TestDatabase.makeV1(at: v1)
        #expect(try AppDatabase.check(at: v1) == .needsMigration(1))
    }

    @Test func newer_database_is_refused() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 3")
        #expect(try AppDatabase.check(at: test.url) == .tooNew(3))
    }

    @Test func default_url_follows_ressac_db_like_the_cli() {
        #expect(AppDatabase.defaultURL(environment: ["RESSAC_DB": "/tmp/x/r.sqlite"]) == URL(filePath: "/tmp/x/r.sqlite"))
        #expect(AppDatabase.defaultURL(environment: [:]).path.hasSuffix("Library/Application Support/Ressac/ressac.sqlite"))
    }

    @Test func open_uses_wal() throws {
        let test = try TestDatabase()
        let db = try test.open()
        let mode = try db.reader.read { try String.fetchOne($0, sql: "PRAGMA journal_mode") }
        #expect(mode == "wal")
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/DatabaseTests`. Expected: `error: cannot find 'AppDatabase' in scope` and `** TEST FAILED **`.

- [ ] **Step 3: Implement** `apps/macos/Ressac/Data/Database.swift`:

```swift
import Foundation
import GRDB

/// The shared SQLite database, opened as a GRDB pool (spec §4, Concurrency).
///
/// The schema belongs to the Rust core: the app never migrates it itself, it runs
/// `ressac migrate` when `check(at:)` says so.
final class AppDatabase: Sendable {
    /// Schema version this build understands (`schema_meta.version`).
    static let knownSchemaVersion = 2

    enum SchemaCheck: Equatable, Sendable {
        case missing
        case needsMigration(Int)
        case ok
        case tooNew(Int)
    }

    /// `RESSAC_DB` when set (as the CLI reads it), else the core's default location,
    /// shared with the terminal.
    static func defaultURL(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let path = environment["RESSAC_DB"], !path.isEmpty {
            return URL(filePath: path)
        }
        return URL.homeDirectory.appending(path: "Library/Application Support/Ressac/ressac.sqlite")
    }

    let writer: any DatabaseWriter
    var reader: any DatabaseReader { writer }

    private init(writer: any DatabaseWriter) {
        self.writer = writer
    }

    /// Opens an existing database. `busyTimeout` is 5 s in the app; tests shorten it.
    static func open(at url: URL, busyTimeout: TimeInterval = 5) throws -> AppDatabase {
        var config = Configuration()
        config.busyMode = .timeout(busyTimeout)
        config.label = "Ressac"
        return AppDatabase(writer: try DatabasePool(path: url.path, configuration: config))
    }

    /// `schema_meta.version`, or nil when the file does not exist.
    static func schemaVersion(at url: URL) throws -> Int? {
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        var config = Configuration()
        config.readonly = true
        let queue = try DatabaseQueue(path: url.path, configuration: config)
        return try queue.read { db in
            try Int.fetchOne(db, sql: "SELECT version FROM schema_meta")
        }
    }

    static func check(at url: URL) throws -> SchemaCheck {
        guard let version = try schemaVersion(at: url) else { return .missing }
        if version < knownSchemaVersion { return .needsMigration(version) }
        if version > knownSchemaVersion { return .tooNew(version) }
        return .ok
    }

    /// GRDB only sees its own writes. After `ressac` (or anything else) wrote to the file,
    /// this makes every live observation fetch again. The deferred transaction takes no lock.
    func notifyExternalChanges() async throws {
        try await writer.writeWithoutTransaction { db in
            try db.inTransaction(.deferred) {
                try db.notifyChanges(in: .fullDatabase)
                return .commit
            }
        }
    }

    /// True for SQLITE_BUSY after the busy timeout ran out.
    static func isBusy(_ error: any Error) -> Bool {
        (error as? DatabaseError)?.resultCode == .SQLITE_BUSY
    }
}
```

- [ ] **Step 4: Run.** Expected: `✔ Test run with 5 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Data/Database.swift apps/macos/RessacTests/DataTests.swift
git commit -m "macOS app: open the shared database with GRDB and check its schema version"
```

### Task 7.2: Records and live queries over the contract views

**Interfaces:**
- Consumes: Task 7.1; the v2 views `v_entry`, `v_unread_count`, `v_garden_row`, `v_folder_path`, the tables `folder`, `feed`, `feed_health`, `entry_fts`, `entry_fulltext`, `fetch_log`; GRDB `FetchableRecord` with `databaseColumnDecodingStrategy` and `databaseDateDecodingStrategy(for:)`, `ValueObservation.tracking(_:)`, `.removeDuplicates()`, `values(in:)`.
- Produces:
  - `Data/Records.swift`: `enum Timestamp { static func string(from: Date) -> String; static func date(from: String) -> Date? }`; `protocol RessacRecord: Codable, FetchableRecord, Sendable, Equatable`; `EntryRow` (`displayTitle`, `excerpt`, `link: URL?`), `FeedItem` (`isSick`), `FolderItem`, `GardenEvidence`, `Remark`, `ProposedAction` (`isSafe`, `isRepair`, `label`), `GardenRow`, `FulltextRow`, `Hop`, `FetchLogRow`; `enum HTMLText { static func plain(_ html: String, limit: Int) -> String }`;
  - `Data/Queries.swift`: `enum GardenScope`, `enum Source` (`isGarden`), `struct SidebarSnapshot`, `typealias LiveQuery<Value>`, and `struct Queries` with `sidebar(now:)`, `entries(source:unreadOnly:now:)`, `search(_:in:now:)`, `entry(id:)`, `garden(scope:)`, `fulltext(entryId:)`, `fetchLog(feedId:)`, `predicate(for:now:) -> Predicate?`, `ftsQuery(_:) -> String?`, `listSummaryLength = 1200`, `urgencyOrder`.

- [ ] **Step 1: Write the failing tests.** Append to `DataTests.swift`:

```swift
// MARK: - Records and queries

/// The first value of an observation that satisfies `predicate`, or a timeout error.
func value<T: Equatable & Sendable>(
    of observation: LiveQuery<T>, in database: AppDatabase, timeout: Duration = .seconds(3),
    where predicate: @escaping @Sendable (T) -> Bool = { _ in true }
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            for try await value in observation.values(in: database.reader) where predicate(value) {
                return value
            }
            throw CancellationError()
        }
        group.addTask {
            try await Task.sleep(for: timeout)
            throw TimeoutError()
        }
        let first = try await group.next()!
        group.cancelAll()
        return first
    }
}

struct TimeoutError: Error {}

@Suite struct QueryTests {
    @Test func timestamps_use_the_core_format() throws {
        let date = try #require(Timestamp.date(from: "2026-10-10T08:00:00.000Z"))
        #expect(Timestamp.string(from: date) == "2026-10-10T08:00:00.000Z")
        #expect(Timestamp.date(from: "2026-10-10T08:00:00Z") == date)
        #expect(Timestamp.string(from: date.addingTimeInterval(0.25)) == "2026-10-10T08:00:00.250Z")
    }

    @Test func entry_rows_decode_from_v_entry() async throws {
        let test = try TestDatabase()
        let id = try test.entry(feed: 1, title: "  ", url: nil, summary: "<p>Only a <b>summary</b> &amp; more</p>")
        let rows = try await value(of: Queries.entries(source: .feed(1), unreadOnly: false), in: test.open())
        let row = try #require(rows.first)
        #expect(row.entryId == id)
        #expect(row.feedTitle == "xkcd.com")
        #expect(row.folderId == 1)
        #expect(row.displayTitle == "Untitled")
        #expect(row.link == nil)
        #expect(row.excerpt == "Only a summary & more")
        #expect(row.publishedAt == Timestamp.date(from: "2026-10-01T08:00:00.000Z"))
        #expect(!row.isRead && !row.isStarred && !row.hasFulltext)
        #expect(row.contentHtml == nil)
    }

    @Test func list_rows_are_shortened_but_entry_is_complete() async throws {
        let test = try TestDatabase()
        let long = String(repeating: "x", count: 5000)
        let id = try test.entry(feed: 1, summary: nil, content: long)
        let db = try test.open()
        let listed = try await value(of: Queries.entries(source: .unread, unreadOnly: false), in: db)
        #expect(listed.first?.summaryHtml?.count == Queries.listSummaryLength)
        let full = try await value(of: Queries.entry(id: id), in: db)
        #expect(full?.contentHtml == long)
        #expect(full?.summaryHtml == nil)
    }

    @Test func sidebar_snapshot_lists_folders_feeds_and_counts() async throws {
        let test = try TestDatabase()
        let today = Timestamp.string(from: Date())
        try test.entry(feed: 2, published: today)
        try test.entry(feed: 2, read: true, starred: true)
        try test.entry(feed: 8)
        try test.execute("""
            INSERT INTO feed_health (feed_id, status, reason, computed_at, needs_triage)
            VALUES (3, 'Dead', 'Gone.', '2026-10-10T08:00:00.000Z', 1)
            """)
        let snapshot = try await value(of: Queries.sidebar(), in: test.open())
        #expect(snapshot.folders.map(\.name) == ["Blogs", "YouTube", "potes", "Biblis", "Infos", "Science", "Jon"])
        #expect(snapshot.feeds.count == 11)
        #expect(snapshot.feeds.first { $0.feedId == 2 }?.unread == 1)
        #expect(snapshot.feeds.first { $0.feedId == 3 }?.status == "Dead")
        #expect(snapshot.feeds.first { $0.feedId == 3 }?.isSick == true)
        #expect((snapshot.unread, snapshot.today, snapshot.starred, snapshot.needsTriage) == (2, 1, 1, 1))
    }

    @Test func entries_follow_source_and_unread_filter() async throws {
        let test = try TestDatabase()
        let a = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        let b = try test.entry(feed: 2, published: "2026-10-03T08:00:00.000Z", read: true)  // folder 2, inside 1
        let c = try test.entry(feed: 8, published: "2026-10-02T08:00:00.000Z", starred: true)
        let db = try test.open()
        func ids(_ source: Source, unreadOnly: Bool = false) async throws -> [Int64] {
            try await value(of: Queries.entries(source: source, unreadOnly: unreadOnly), in: db).map(\.entryId)
        }
        #expect(try await ids(.folder(1)) == [b, a])
        #expect(try await ids(.folder(1), unreadOnly: true) == [a])
        #expect(try await ids(.unread) == [c, a])
        #expect(try await ids(.starred) == [c])
        #expect(try await ids(.feed(8)) == [c])
        #expect(try await ids(.garden(.all)) == [])
    }

    @Test func search_matches_every_word_within_the_source() async throws {
        let test = try TestDatabase()
        let a = try test.entry(feed: 1, title: "Café crème", body: "le petit déjeuner")
        try test.entry(feed: 8, title: "Cafe noir", body: "le soir")
        let db = try test.open()
        func ids(_ text: String, _ source: Source = .unread) async throws -> Set<Int64> {
            Set(try await value(of: Queries.search(text, in: source), in: db).map(\.entryId))
        }
        #expect(try await ids("cafe").count == 2)  // remove_diacritics 2
        #expect(try await ids("cafe dejeuner") == [a])
        #expect(try await ids("cafe", .folder(1)) == [a])
        #expect(try await ids("\"cafe OR noir") == [])  // quotes and operators stay literal
        #expect(try await ids("   ") == [])
        #expect(Queries.ftsQuery("a \"b") == "\"a\" \"\"\"b\"")
    }

    @Test func garden_rows_decode_the_json_columns() async throws {
        let test = try TestDatabase()
        try test.execute("""
            INSERT INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
            VALUES (2, 'Moved', 'Redirects.', '{"bullets":["b1"],"clue":"301","language":null}',
                    '[{"type":"http_only"},{"type":"similar_title","feed_id":3}]',
                    '{"type":"follow_redirect","url":"https://new.example.org/feed"}', '2026-10-10T08:00:00.000Z', 1);
            INSERT INTO fetch_log (feed_id, run_id, at, http_status, error_kind, redirect_chain)
            VALUES (2, 'r1', '2026-10-09T08:00:00.000Z', 404, '{"Http":404}', '[]'),
                   (2, 'r2', '2026-10-10T08:00:00.000Z', 200, NULL, '[{"status":301,"url":"https://new.example.org/feed"}]');
            INSERT INTO garden_decision (feed_id, decision, status_at_decision, decided_at)
            VALUES (1, 'keep', 'Unchecked', '2026-10-10T08:00:00.000Z');
            UPDATE feed SET state = 'buried' WHERE id = 11;
            """)
        let db = try test.open()
        let triage = try await value(of: Queries.garden(scope: .needsTriage), in: db)
        let row = try #require(triage.first)
        #expect(triage.count == 1)
        #expect(row.feedId == 2 && row.status == "Moved" && row.clue == "301" && row.needsTriage)
        #expect(row.folderPath == "Blogs / potes")
        #expect(row.evidence == GardenEvidence(bullets: ["b1"], clue: "301", language: nil))
        #expect(row.remarks == [.httpOnly, .similarTitle(feedId: 3)])
        #expect(row.proposedAction == .followRedirect(url: "https://new.example.org/feed"))
        #expect(row.proposedAction?.isSafe == true)
        let all = try await value(of: Queries.garden(scope: .all), in: db)
        #expect(all.count == 10)
        #expect(all.first?.feedId == 2)  // Moved sorts before unchecked feeds
        #expect(all.first { $0.feedId == 1 }?.lastDecision == "keep")
        #expect(all.first { $0.feedId == 1 }?.remarks == [])
        #expect(try await value(of: Queries.garden(scope: .cemetery), in: db).map(\.feedId) == [11])
        let log = try await value(of: Queries.fetchLog(feedId: 2), in: db)
        #expect(log.map(\.httpStatus) == [200, 404])
        #expect(log.first?.redirectChain == [Hop(status: 301, url: "https://new.example.org/feed")])
        #expect(log.last?.errorKind == "{\"Http\":404}")
    }

    @Test func fulltext_row_is_observed() async throws {
        let test = try TestDatabase()
        let id = try test.entry(feed: 1)
        try test.execute(
            "INSERT INTO entry_fulltext (entry_id, fetched_at, error) VALUES (?, '2026-10-10T08:00:00.000Z', 'HTTP 404.')",
            [id])
        let row = try await value(of: Queries.fulltext(entryId: id), in: test.open())
        #expect(row == FulltextRow(entryId: id, html: nil, title: nil, byline: nil, error: "HTTP 404."))
    }

    /// GRDB does not see another process's writes; `notifyExternalChanges` makes it refetch.
    @Test func external_writes_show_after_notify() async throws {
        let test = try TestDatabase()
        let db = try test.open()
        let observation = Queries.entries(source: .unread, unreadOnly: false)
        #expect(try await value(of: observation, in: db).isEmpty)
        let seen = Task { try await value(of: observation, in: db, where: { $0.count == 1 }) }
        try await Task.sleep(for: .milliseconds(100))
        try test.entry(feed: 1)  // another connection, like `ressac fetch`
        try await db.notifyExternalChanges()
        #expect(try await seen.value.count == 1)
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/QueryTests`. Expected: `error: cannot find type 'LiveQuery' in scope` (and similar for `Timestamp`, `Queries`), `** TEST FAILED **`.

- [ ] **Step 3: Implement the records** `apps/macos/Ressac/Data/Records.swift`:

```swift
import Foundation
import GRDB

/// RFC 3339 UTC timestamps with milliseconds and `Z`, as the core writes them (`model::ts`).
enum Timestamp {
    static func string(from date: Date) -> String {
        date.formatted(withFractions)
    }

    static func date(from text: String) -> Date? {
        (try? withFractions.parse(text)) ?? (try? withoutFractions.parse(text))
    }

    private static let withFractions = Date.ISO8601FormatStyle(includingFractionalSeconds: true, timeZone: .gmt)
    private static let withoutFractions = Date.ISO8601FormatStyle(timeZone: .gmt)
}

/// Shared decoding rules: snake_case columns, RFC 3339 dates.
protocol RessacRecord: Codable, FetchableRecord, Sendable, Equatable {}

extension RessacRecord {
    static var databaseColumnDecodingStrategy: DatabaseColumnDecodingStrategy { .convertFromSnakeCase }

    static func databaseDateDecodingStrategy(for column: String) -> DatabaseDateDecodingStrategy {
        .custom { value in String.fromDatabaseValue(value).flatMap(Timestamp.date(from:)) }
    }
}

/// One row of `v_entry`. In list results `contentHtml` is nil and `summaryHtml` is
/// shortened (see `Queries.entries`); `Queries.entry(id:)` returns the full row.
struct EntryRow: RessacRecord, Identifiable {
    var entryId: Int64
    var feedId: Int64
    var feedTitle: String
    var folderId: Int64?
    var title: String?
    var author: String?
    var url: String?
    var publishedAt: Date
    var isRead: Bool
    var isStarred: Bool
    var hasFulltext: Bool
    var summaryHtml: String?
    var contentHtml: String?

    var id: Int64 { entryId }

    /// The title, or "Untitled" when the feed gave none (Review Focus #1).
    var displayTitle: String {
        let trimmed = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? "Untitled" : trimmed
    }

    /// Plain text for the list's two summary lines.
    var excerpt: String {
        HTMLText.plain(summaryHtml ?? contentHtml ?? "", limit: 280)
    }

    var link: URL? {
        url.flatMap(URL.init(string:)).flatMap { $0.scheme == "http" || $0.scheme == "https" ? $0 : nil }
    }
}

/// Crude tag stripping for list excerpts. The reader never uses it.
enum HTMLText {
    static func plain(_ html: String, limit: Int) -> String {
        var text = html.replacing(/<[^>]*>?/, with: " ")
        for (entity, char) in entities {
            text = text.replacing(entity, with: char)
        }
        let collapsed = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return collapsed.count > limit ? String(collapsed.prefix(limit)) + "…" : collapsed
    }

    private static let entities: [(String, String)] = [
        ("&nbsp;", " "), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'"), ("&amp;", "&"),
    ]
}

struct FeedItem: RessacRecord, Identifiable {
    var feedId: Int64
    var title: String
    var folderId: Int64?
    var position: Int
    var unread: Int
    var status: String?
    var preferFullText: Bool
    var htmlUrl: String?

    var id: Int64 { feedId }

    /// Statuses that get a coloured dot in the sidebar (spec §7).
    var isSick: Bool {
        ["Ailing", "Dead", "Moved", "Hijacked", "Duplicate"].contains(status ?? "")
    }
}

struct FolderItem: RessacRecord, Identifiable {
    var folderId: Int64
    var parentId: Int64?
    var name: String
    var position: Int

    var id: Int64 { folderId }
}

/// `feed_health.evidence`, serialised by the core's `model::Evidence`.
struct GardenEvidence: Codable, Sendable, Equatable {
    var bullets: [String]
    var clue: String
    var language: String?
}

/// The core's `model::Remark` (`#[serde(tag = "type", rename_all = "snake_case")]`).
enum Remark: Codable, Sendable, Equatable {
    case httpsAvailable
    case httpOnly
    case similarTitle(feedId: Int64)
    case dateFallback
    case blocked
    case unknown(String)

    private enum CodingKeys: String, CodingKey {
        case type
        case feedId = "feed_id"
    }

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "https_available": self = .httpsAvailable
        case "http_only": self = .httpOnly
        case "similar_title": self = .similarTitle(feedId: try c.decode(Int64.self, forKey: .feedId))
        case "date_fallback": self = .dateFallback
        case "blocked": self = .blocked
        default: self = .unknown(type)
        }
    }

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .httpsAvailable: try c.encode("https_available", forKey: .type)
        case .httpOnly: try c.encode("http_only", forKey: .type)
        case let .similarTitle(feedId):
            try c.encode("similar_title", forKey: .type)
            try c.encode(feedId, forKey: .feedId)
        case .dateFallback: try c.encode("date_fallback", forKey: .type)
        case .blocked: try c.encode("blocked", forKey: .type)
        case let .unknown(type): try c.encode(type, forKey: .type)
        }
    }
}

/// The core's `model::Action` (`#[serde(tag = "type", rename_all = "snake_case")]`).
enum ProposedAction: Codable, Sendable, Equatable {
    case upgradeHttps(url: String)
    case followRedirect(url: String)
    case adoptDiscovered(url: String)
    case mergeInto(feedId: Int64)
    case bury

    private enum CodingKeys: String, CodingKey {
        case type, url
        case feedId = "feed_id"
    }

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "upgrade_https": self = .upgradeHttps(url: try c.decode(String.self, forKey: .url))
        case "follow_redirect": self = .followRedirect(url: try c.decode(String.self, forKey: .url))
        case "adopt_discovered": self = .adoptDiscovered(url: try c.decode(String.self, forKey: .url))
        case "merge_into": self = .mergeInto(feedId: try c.decode(Int64.self, forKey: .feedId))
        case "bury": self = .bury
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown action \(other)")
        }
    }

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .upgradeHttps(url):
            try c.encode("upgrade_https", forKey: .type)
            try c.encode(url, forKey: .url)
        case let .followRedirect(url):
            try c.encode("follow_redirect", forKey: .type)
            try c.encode(url, forKey: .url)
        case let .adoptDiscovered(url):
            try c.encode("adopt_discovered", forKey: .type)
            try c.encode(url, forKey: .url)
        case let .mergeInto(feedId):
            try c.encode("merge_into", forKey: .type)
            try c.encode(feedId, forKey: .feedId)
        case .bury:
            try c.encode("bury", forKey: .type)
        }
    }

    /// Repairs `apply-safe` applies in bulk (core `Action::is_safe`).
    var isSafe: Bool {
        switch self {
        case .upgradeHttps, .followRedirect: true
        default: false
        }
    }

    /// Everything except burying (core `Action::is_repair`).
    var isRepair: Bool { self != .bury }

    /// Same wording as the core's `Action::label`.
    var label: String {
        switch self {
        case let .upgradeHttps(url): "switch to \(url)"
        case let .followRedirect(url): "follow redirect to \(url)"
        case let .adoptDiscovered(url): "adopt discovered feed \(url)"
        case let .mergeInto(feedId): "merge into #\(feedId)"
        case .bury: "bury → cemetery"
        }
    }
}

/// One row of `v_garden_row`. Fetch it through `Queries.garden`, which turns a missing
/// `remarks` (feed never diagnosed) into `[]`.
struct GardenRow: RessacRecord, Identifiable {
    var feedId: Int64
    var title: String
    var folderPath: String?
    var state: String
    var status: String?
    var reason: String?
    var clue: String?
    var evidence: GardenEvidence?
    var remarks: [Remark]
    var proposedAction: ProposedAction?
    var needsTriage: Bool
    var lastDecision: String?
    var waybackUrl: String?
    var htmlUrl: String?
    var xmlUrl: String

    var id: Int64 { feedId }
}

/// One row of `entry_fulltext`: exactly one of `html` and `error` is set.
struct FulltextRow: RessacRecord, Identifiable {
    var entryId: Int64
    var html: String?
    var title: String?
    var byline: String?
    var error: String?

    var id: Int64 { entryId }
}

/// The core's `model::Hop`.
struct Hop: Codable, Sendable, Equatable {
    var status: Int
    var url: String
}

struct FetchLogRow: RessacRecord, Identifiable {
    var id: Int64
    var at: Date
    var httpStatus: Int?
    /// Raw JSON of the core's `ErrorKind`, for example `"Dns"` or `{"Http":404}`.
    var errorKind: String?
    var redirectChain: [Hop]
}
```

- [ ] **Step 4: Implement the queries** `apps/macos/Ressac/Data/Queries.swift`:

```swift
import Foundation
import GRDB

enum GardenScope: Hashable, Sendable {
    case needsTriage, all, cemetery
}

/// What the sidebar selects (spec §7, Sidebar sections).
enum Source: Hashable, Sendable {
    case unread, today, starred
    case folder(Int64)
    case feed(Int64)
    case garden(GardenScope)

    var isGarden: Bool {
        if case .garden = self { true } else { false }
    }
}

struct SidebarSnapshot: Equatable, Sendable {
    var folders: [FolderItem] = []
    var feeds: [FeedItem] = []
    var unread = 0
    var today = 0
    var starred = 0
    var needsTriage = 0
}

/// A live query: refetched when GRDB sees a change, duplicates dropped.
typealias LiveQuery<Value: Equatable & Sendable> = ValueObservation<ValueReducers.RemoveDuplicates<ValueReducers.Fetch<Value>>>

/// Pure `ValueObservation` factories over the contract views (spec §5).
struct Queries {
    /// Characters of `summary_html` (else `content_html`) kept in list rows.
    static let listSummaryLength = 1200

    private static let listColumns = """
        entry_id, feed_id, feed_title, folder_id, title, author, url, published_at,
        is_read, is_starred, has_fulltext,
        substr(COALESCE(summary_html, content_html), 1, \(listSummaryLength)) AS summary_html,
        NULL AS content_html
        """

    static func sidebar(now: Date = Date()) -> LiveQuery<SidebarSnapshot> {
        let midnight = Timestamp.string(from: Calendar.current.startOfDay(for: now))
        return ValueObservation.tracking { db in
            SidebarSnapshot(
                folders: try FolderItem.fetchAll(db, sql: """
                    SELECT id AS folder_id, parent_id, name, position FROM folder ORDER BY position, id
                    """),
                feeds: try FeedItem.fetchAll(db, sql: """
                    SELECT f.id AS feed_id, f.title, f.folder_id, f.position, COALESCE(u.unread, 0) AS unread,
                           h.status, f.prefer_full_text, f.html_url
                    FROM feed f
                    LEFT JOIN v_unread_count u ON u.feed_id = f.id
                    LEFT JOIN feed_health h ON h.feed_id = f.id
                    WHERE f.state = 'active'
                    ORDER BY f.position, f.id
                    """),
                unread: try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM v_entry WHERE NOT is_read") ?? 0,
                today: try Int.fetchOne(
                    db, sql: "SELECT COUNT(*) FROM v_entry WHERE NOT is_read AND published_at >= ?",
                    arguments: [midnight]) ?? 0,
                starred: try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM v_entry WHERE is_starred") ?? 0,
                needsTriage: try Int.fetchOne(
                    db, sql: "SELECT COUNT(*) FROM v_garden_row WHERE needs_triage AND state = 'active'") ?? 0
            )
        }
        .removeDuplicates()
    }

    /// The entry list of a source, newest first. Garden sources have no entries.
    static func entries(source: Source, unreadOnly: Bool, now: Date = Date()) -> LiveQuery<[EntryRow]> {
        ValueObservation.tracking { db in
            guard let filter = predicate(for: source, now: now) else { return [] }
            let unread = unreadOnly ? " AND NOT is_read" : ""
            return try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry
                WHERE \(filter.sql)\(unread)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: filter.arguments)
        }
        .removeDuplicates()
    }

    /// Full-text search within a source (spec §7, Search). Every word must appear (see `ftsQuery`).
    static func search(_ text: String, in source: Source, now: Date = Date()) -> LiveQuery<[EntryRow]> {
        ValueObservation.tracking { db in
            guard let match = ftsQuery(text), let filter = predicate(for: source, now: now) else { return [] }
            return try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry
                WHERE entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?)
                  AND \(filter.sql)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: [match] + filter.arguments)
        }
        .removeDuplicates()
    }

    /// One complete entry (with `content_html`), for the reader.
    static func entry(id: Int64) -> LiveQuery<EntryRow?> {
        ValueObservation.tracking { db in
            try EntryRow.fetchOne(db, sql: """
                SELECT entry_id, feed_id, feed_title, folder_id, title, author, url, published_at,
                       is_read, is_starred, has_fulltext, summary_html, content_html
                FROM v_entry WHERE entry_id = ?
                """, arguments: [id])
        }
        .removeDuplicates()
    }

    static func garden(scope: GardenScope) -> LiveQuery<[GardenRow]> {
        let filter = switch scope {
        case .needsTriage: "needs_triage AND state = 'active'"
        case .all: "state = 'active'"
        case .cemetery: "state = 'buried'"
        }
        return ValueObservation.tracking { db in
            try GardenRow.fetchAll(db, sql: """
                SELECT feed_id, title, folder_path, state, status, reason, clue, evidence,
                       COALESCE(remarks, '[]') AS remarks, proposed_action, needs_triage, last_decision,
                       wayback_url, html_url, xml_url
                FROM v_garden_row WHERE \(filter)
                ORDER BY \(urgencyOrder), title COLLATE NOCASE, feed_id
                """)
        }
        .removeDuplicates()
    }

    static func fulltext(entryId: Int64) -> LiveQuery<FulltextRow?> {
        ValueObservation.tracking { db in
            try FulltextRow.fetchOne(db, sql: """
                SELECT entry_id, html, title, byline, error FROM entry_fulltext WHERE entry_id = ?
                """, arguments: [entryId])
        }
        .removeDuplicates()
    }

    /// The last 20 fetch attempts of a feed, newest first.
    static func fetchLog(feedId: Int64) -> LiveQuery<[FetchLogRow]> {
        ValueObservation.tracking { db in
            try FetchLogRow.fetchAll(db, sql: """
                SELECT id, at, http_status, error_kind, redirect_chain FROM fetch_log
                WHERE feed_id = ? ORDER BY at DESC, id DESC LIMIT 20
                """, arguments: [feedId])
        }
        .removeDuplicates()
    }

    /// The core's `Status::ALL` order (urgency), unknown statuses last.
    static let urgencyOrder = """
        CASE status WHEN 'Duplicate' THEN 0 WHEN 'Hijacked' THEN 1 WHEN 'Moved' THEN 2 WHEN 'Dead' THEN 3 \
        WHEN 'Ailing' THEN 4 WHEN 'Fossil' THEN 5 WHEN 'Dormant' THEN 6 WHEN 'Alive' THEN 7 ELSE 8 END
        """

    struct Predicate {
        var sql: String
        var arguments: StatementArguments = []
    }

    /// The `v_entry` rows of a source, shared with `Writes.markAllRead`. Nil for garden sources.
    static func predicate(for source: Source, now: Date) -> Predicate? {
        switch source {
        case .unread:
            Predicate(sql: "NOT is_read")
        case .today:
            Predicate(sql: "published_at >= ?", arguments: [Timestamp.string(from: Calendar.current.startOfDay(for: now))])
        case .starred:
            Predicate(sql: "is_starred")
        case let .feed(id):
            Predicate(sql: "feed_id = ?", arguments: [id])
        case let .folder(id):
            Predicate(
                sql: """
                    folder_id IN (WITH RECURSIVE sub(id) AS \
                    (SELECT ? UNION ALL SELECT f.id FROM folder f JOIN sub ON f.parent_id = sub.id) SELECT id FROM sub)
                    """,
                arguments: [id])
        case .garden:
            nil
        }
    }

    /// `"word1" "word2"`: every token quoted, so FTS5 operators typed by the user stay literal.
    static func ftsQuery(_ text: String) -> String? {
        let tokens = text.split(whereSeparator: \.isWhitespace).map {
            "\"" + $0.replacing("\"", with: "\"\"") + "\""
        }
        return tokens.isEmpty ? nil : tokens.joined(separator: " ")
    }
}
```

- [ ] **Step 5: Run.** Expected: `✔ Test run with 9 tests in 1 suite passed`.

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/Data/Records.swift apps/macos/Ressac/Data/Queries.swift apps/macos/RessacTests/DataTests.swift
git commit -m "macOS app: records and live queries over the v2 contract views"
```

### Task 7.3: Client writes

**Interfaces:**
- Consumes: `AppDatabase.writer`, `Queries.predicate(for:now:)`, `Timestamp.string(from:)`, `databaseQuestionMarks(count:)` (GRDB).
- Produces (`Data/Writes.swift`): `struct Writes: Sendable { let db: AppDatabase }` with `setRead(_ ids: [Int64], read: Bool, at: Date) async throws` (keeps the first `read_at` of an entry already read), `setStarred(_ id: Int64, starred: Bool, at: Date) async throws`, `markAllRead(in source: Source, at: Date) async throws -> Int` and `setPreferFullText(feedId: Int64, on: Bool) async throws`; `Array.chunked(by:)`.

- [ ] **Step 1: Write the failing tests.** Append to `DataTests.swift`. `concurrent_writer_does_not_lose_read_marks` pins Review Focus #2: a second connection holds a write transaction for 1 s while the app marks 50 entries read.

```swift
// MARK: - Writes

@Suite struct WriteTests {
    let now = Timestamp.date(from: "2026-10-10T09:00:00.000Z")!

    func readAt(_ test: TestDatabase, _ id: Int64) throws -> String? {
        try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [id])
    }

    @Test func read_and_star_marks_round_trip() async throws {
        let test = try TestDatabase()
        let a = try test.entry(feed: 1)
        let b = try test.entry(feed: 1, read: true)
        let writes = Writes(db: try test.open())
        try await writes.setRead([a, b], read: true, at: now)
        #expect(try readAt(test, a) == "2026-10-10T09:00:00.000Z")
        #expect(try readAt(test, b) == "2026-10-10T08:00:00.000Z")  // first read time kept
        try await writes.setRead([a], read: false, at: now)
        #expect(try readAt(test, a) == nil)
        try await writes.setStarred(a, starred: true, at: now)
        let starred = try test.one(String.self, "SELECT starred_at FROM entry WHERE id = ?", [a])
        #expect(starred == "2026-10-10T09:00:00.000Z")
        try await writes.setStarred(a, starred: false, at: now)
        #expect(try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE starred_at IS NOT NULL") == 0)
    }

    @Test func mark_all_read_is_scoped_to_the_source() async throws {
        let test = try TestDatabase()
        let inFolder = try test.entry(feed: 2)  // folder 2, a subfolder of 1
        let alsoInFolder = try test.entry(feed: 5)
        let elsewhere = try test.entry(feed: 8)
        let writes = Writes(db: try test.open())
        #expect(try await writes.markAllRead(in: .folder(1), at: now) == 2)
        #expect(try readAt(test, inFolder) != nil && readAt(test, alsoInFolder) != nil)
        #expect(try readAt(test, elsewhere) == nil)
        #expect(try await writes.markAllRead(in: .folder(1), at: now) == 0)
        #expect(try await writes.markAllRead(in: .garden(.all), at: now) == 0)
        #expect(try await writes.markAllRead(in: .unread, at: now) == 1)
    }

    @Test func prefer_full_text_is_written() async throws {
        let test = try TestDatabase()
        let writes = Writes(db: try test.open())
        try await writes.setPreferFullText(feedId: 4, on: true)
        #expect(try test.one(Bool.self, "SELECT prefer_full_text FROM feed WHERE id = 4") == true)
    }

    /// Review Focus #2: the core holds a write transaction for 1 s while the app marks entries read.
    @Test func concurrent_writer_does_not_lose_read_marks() async throws {
        let test = try TestDatabase()
        let ids = try (0..<50).map { _ in try test.entry(feed: 1) }
        let writes = Writes(db: try test.open())
        let (locked, lockedSignal) = AsyncStream<Void>.makeStream()
        let core = Task.detached {
            try test.holdWriteLock(for: 1, sql: "UPDATE feed SET title = 'Renamed by the core' WHERE id = 1") {
                lockedSignal.yield()
            }
        }
        for await _ in locked { break }
        let start = ContinuousClock.now
        try await writes.setRead(ids, read: true, at: now)
        #expect(ContinuousClock.now - start > .milliseconds(500))  // it waited for the lock
        try await core.value
        let unread = try test.one(Int.self, "SELECT COUNT(*) FROM entry WHERE read_at IS NULL")
        let title = try test.one(String.self, "SELECT title FROM feed WHERE id = 1")
        #expect(unread == 0)
        #expect(title == "Renamed by the core")
    }

    @Test func busy_timeout_surfaces_as_a_busy_error() async throws {
        let test = try TestDatabase()
        let id = try test.entry(feed: 1)
        let writes = Writes(db: try test.open(busyTimeout: 0.2))
        let (locked, lockedSignal) = AsyncStream<Void>.makeStream()
        let core = Task.detached {
            try test.holdWriteLock(for: 1) { lockedSignal.yield() }
        }
        for await _ in locked { break }
        let error = await #expect(throws: DatabaseError.self) {
            try await writes.setRead([id], read: true, at: now)
        }
        #expect(error.map(AppDatabase.isBusy) == true)
        try await core.value
        #expect(try readAt(test, id) == nil)
    }
}
```

- [ ] **Step 2: Run** `-only-testing:RessacTests/WriteTests`. Expected: `error: cannot find 'Writes' in scope`.

- [ ] **Step 3: Implement** `apps/macos/Ressac/Data/Writes.swift`:

```swift
import Foundation
import GRDB

/// The only SQL writes the app makes: `entry.read_at`, `entry.starred_at` and
/// `feed.prefer_full_text` (Global Constraints, client-writable columns).
struct Writes: Sendable {
    let db: AppDatabase

    func setRead(_ ids: [Int64], read: Bool, at: Date) async throws {
        guard !ids.isEmpty else { return }
        let stamp: String? = read ? Timestamp.string(from: at) : nil
        try await db.writer.write { db in
            for chunk in ids.chunked(by: 500) {
                let marks = databaseQuestionMarks(count: chunk.count)
                // Keep the first read time when an entry is already read.
                let keep = read ? " AND read_at IS NULL" : ""
                try db.execute(
                    sql: "UPDATE entry SET read_at = ? WHERE id IN (\(marks))\(keep)",
                    arguments: StatementArguments([stamp] + chunk.map { $0 as (any DatabaseValueConvertible)? }))
            }
        }
    }

    func setStarred(_ id: Int64, starred: Bool, at: Date) async throws {
        let stamp: String? = starred ? Timestamp.string(from: at) : nil
        try await db.writer.write { db in
            try db.execute(sql: "UPDATE entry SET starred_at = ? WHERE id = ?", arguments: [stamp, id])
        }
    }

    /// Marks every unread entry of the source read; returns how many changed.
    func markAllRead(in source: Source, at: Date) async throws -> Int {
        guard let filter = Queries.predicate(for: source, now: at) else { return 0 }
        let stamp = Timestamp.string(from: at)
        return try await db.writer.write { db in
            try db.execute(
                sql: """
                    UPDATE entry SET read_at = ?
                    WHERE read_at IS NULL AND id IN (SELECT entry_id FROM v_entry WHERE \(filter.sql))
                    """,
                arguments: [stamp] + filter.arguments)
            return db.changesCount
        }
    }

    func setPreferFullText(feedId: Int64, on: Bool) async throws {
        try await db.writer.write { db in
            try db.execute(sql: "UPDATE feed SET prefer_full_text = ? WHERE id = ?", arguments: [on, feedId])
        }
    }
}

extension Array {
    func chunked(by size: Int) -> [ArraySlice<Element>] {
        stride(from: 0, to: count, by: size).map { self[$0..<Swift.min($0 + size, count)] }
    }
}
```

- [ ] **Step 4: Run** the whole suite (no `-only-testing`). Expected: `✔ Test run with 21 tests in 4 suites passed` (2 smoke tests and 19 data tests), in about 3 s.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Data/Writes.swift apps/macos/RessacTests/DataTests.swift
git commit -m "macOS app: write read, starred and prefer-full-text state"
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` with 21 Swift tests, `no warnings in our sources`. Push.

### Notes for the executor

- **GRDB does not see another process's writes.** A `ValueObservation` only refetches after commits made through its own pool. After `ressac` writes, call `AppDatabase.notifyExternalChanges()` (phase 8's `RunCoordinator` does it after each run). `external_writes_show_after_notify` fails with a 3 s `TimeoutError` if the notify line is removed: that was checked.
- **`notifyExternalChanges` uses a deferred transaction.** GRDB's default write transaction is `IMMEDIATE`, which would wait for the core's write lock just to send a notification.
- **Synchronous GRDB calls in async tests.** Inside an `async` function, `queue.write { }` resolves to GRDB's async overload and needs `await`. The tests therefore go through `TestDatabase`'s synchronous helpers (`execute`, `one`, `holdWriteLock`), which are plain functions.
- **`Remark`, `ProposedAction` and `GardenEvidence` are decoded from the JSON text columns** by GRDB's Codable support (non-primitive properties are read as JSON). `v_garden_row.remarks` is NULL for a feed never diagnosed, so `Queries.garden` selects `COALESCE(remarks, '[]')`; always fetch `GardenRow` through it.
- **`Bool` columns** (`is_read`, `needs_triage`, `prefer_full_text`) decode from SQLite integers; `has_fulltext` comes from `EXISTS`.
- **Search** quotes every word (`"cafe" "dejeuner"`): FTS5 operators typed by the user stay literal, and `remove_diacritics 2` makes "cafe" match "Café". The folder filter is a recursive CTE inside the `IN (…)` subquery, so the `?` placeholders stay in reading order for `markAllRead`'s `UPDATE`.
- **Do not name a type `Observation`.** It shadows the Observation module and breaks every `@Observable` macro expansion in phase 8; hence `LiveQuery`.
- **Fixture independence.** The tests were also run against a fixture shaped like phase 5's (entries, a full text, two diagnoses, a buried feed, two decisions, `prefer_full_text` on Daring Fireball): all pass, thanks to `TestDatabase`'s reset. They only rely on the folder and feed ids above. If phase 5 changed the OPML the fixture imports, update the names and counts in `sidebar_snapshot_lists_folders_feeds_and_counts`, `garden_rows_decode_the_json_columns` and phase 9's `sidebar_tree_nests_folders_and_feeds_in_opml_order`.
