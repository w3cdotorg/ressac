import Foundation
import GRDB
import Testing

@testable import Ressac

// MARK: - Test database

/// A private copy of `tests/fixtures/contract.sqlite` reduced to its schema (v3) and the
/// 7 folders and 11 feeds of `vienna-like.opml`, all active, with the page numbers the core
/// gave them: entries, full texts, diagnoses, fetch logs and decisions are cleared, so each
/// test seeds what it asserts on.
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
        try test.execute("UPDATE schema_meta SET version = 4")
        #expect(try AppDatabase.check(at: test.url) == .tooNew(4))
    }

    /// Review Focus #3: a database the v3 core migrated, opened by an app that knows v2,
    /// gets "This database was created by a newer Ressac" (`SchemaCheck.tooNew`).
    @Test func an_older_app_refuses_a_v3_database() throws {
        let test = try TestDatabase()
        #expect(AppDatabase.knownSchemaVersion == 3)
        #expect(try AppDatabase.schemaVersion(at: test.url) == 3)
        #expect(try AppDatabase.check(at: test.url, known: 2) == .tooNew(3))
        #expect(try AppDatabase.check(at: test.url, known: 3) == .ok)
        #expect(try AppDatabase.check(at: test.url, known: 4) == .needsMigration(3))
        #expect(try AppDatabase.check(at: test.dir.appending(path: "nope.sqlite"), known: 2) == .missing)
    }

    /// Every database made before this sub-project is at v2: the app runs `ressac migrate` first.
    @Test func a_v2_database_requests_migration() throws {
        let test = try TestDatabase()
        try test.execute("UPDATE schema_meta SET version = 2")
        #expect(try AppDatabase.check(at: test.url) == .needsMigration(2))
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

    /// `EntryRow` decodes by hand (Codable cost 36 µs a row): every column, and a clear error.
    @Test func entry_rows_decode_every_column_by_hand() async throws {
        let test = try TestDatabase()
        let id = try test.entry(feed: 2, title: "T", url: "https://example.org/t", read: true, starred: true)
        try test.execute("UPDATE entry SET author = 'Ann', content_html = '<p>C</p>' WHERE id = ?", [id])
        try test.execute(
            "INSERT INTO entry_fulltext (entry_id, html, fetched_at) VALUES (?, '<p>F</p>', '2026-10-10T08:00:00.000Z')", [id])
        let db = try test.open()
        let row = try #require(try await value(of: Queries.entry(id: id), in: db))
        #expect(row == EntryRow(entryId: id, feedId: 2, feedTitle: "Orange Drenka", folderId: 2, title: "T", author: "Ann",
                                url: "https://example.org/t", publishedAt: Timestamp.date(from: "2026-10-01T08:00:00.000Z")!,
                                isRead: true, isStarred: true, hasFulltext: true, summaryHtml: "<p>Summary</p>", contentHtml: "<p>C</p>"))
        try test.execute("UPDATE entry SET published_at = 'yesterday' WHERE id = ?", [id])
        let error = await #expect(throws: DatabaseError.self) {
            try await db.reader.read { try EntryRow.fetchOne($0, sql: "SELECT * FROM v_entry WHERE entry_id = ?", arguments: [id]) }
        }
        #expect(error?.message == "published_at is not an RFC 3339 date: yesterday")
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

    /// Schema v3: the numbers the core gave the fixture (see the table in phase 6).
    @Test func sidebar_snapshot_carries_page_numbers() async throws {
        let test = try TestDatabase()
        let db = try test.open()
        let snapshot = try await value(of: Queries.sidebar(), in: db)
        let folders = Dictionary(uniqueKeysWithValues: snapshot.folders.map { ($0.folderId, $0.page) })
        #expect(folders == [1: 200, 2: 202, 3: 300, 4: 400, 5: 403, 6: 500, 7: 501])
        let feeds = Dictionary(uniqueKeysWithValues: snapshot.feeds.map { ($0.feedId, $0.page) })
        #expect(feeds == [1: 201, 2: 203, 3: 204, 4: 205, 5: 206, 6: 301, 7: 302, 8: 401, 9: 402, 10: 404, 11: 502])
        // The view of the contract lists the same 18 numbers (through the app's pool, in WAL mode).
        let viewRows = try await db.reader.read { try Int.fetchOne($0, sql: "SELECT COUNT(*) FROM v_page") }
        #expect(viewRows == 18)
    }

    @Test func a_row_not_numbered_yet_reads_as_nil() async throws {
        let test = try TestDatabase()
        try test.execute("UPDATE feed SET page = NULL WHERE id = 8; UPDATE folder SET page = NULL WHERE id = 5")
        let snapshot = try await value(of: Queries.sidebar(), in: test.open())
        #expect(snapshot.feeds.first { $0.feedId == 8 }?.page == nil)
        #expect(snapshot.feeds.first { $0.feedId == 9 }?.page == 402)
        #expect(snapshot.folders.first { $0.folderId == 5 }?.page == nil)
        #expect(FeedItem(feedId: 1, title: "x", folderId: nil, position: 0, unread: 0, status: nil,
                         preferFullText: false, htmlUrl: nil).page == nil)
        #expect(FolderItem(folderId: 1, parentId: nil, name: "Blogs", position: 0, page: 200).page == 200)
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
