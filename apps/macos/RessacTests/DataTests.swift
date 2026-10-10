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
