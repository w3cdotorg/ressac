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
