import Foundation
import GRDB

/// The shared SQLite database, opened as a GRDB pool (spec §4, Concurrency).
///
/// The schema belongs to the Rust core: the app never migrates it itself, it runs
/// `ressac migrate` when `check(at:)` says so.
final class AppDatabase: Sendable {
    /// Schema version this build understands (`schema_meta.version`): v3 adds page numbers,
    /// v4 the rhythm groups and the morning edition.
    static let knownSchemaVersion = 5

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
    /// The database file, for connections of our own (see `DataVersionWatcher`).
    let url: URL

    private init(writer: any DatabaseWriter, url: URL) {
        self.writer = writer
        self.url = url
    }

    /// Opens an existing database. `busyTimeout` is 5 s in the app; tests shorten it.
    static func open(at url: URL, busyTimeout: TimeInterval = 5) throws -> AppDatabase {
        var config = Configuration()
        config.busyMode = .timeout(busyTimeout)
        config.label = "Ressac"
        return AppDatabase(writer: try DatabasePool(path: url.path, configuration: config), url: url)
    }

    /// For a connection of ours that only reads. Not `readonly`: a read-only connection cannot
    /// open a WAL database whose `-shm` file is gone, which is how `ressac migrate` leaves a new
    /// database (SQLITE_CANTOPEN, "unable to open database file"). The file is opened read-write,
    /// so SQLite can recreate `-shm`, and `query_only` refuses any write.
    static func readingConfiguration(label: String) -> Configuration {
        var config = Configuration()
        config.label = label
        config.prepareDatabase { db in try db.execute(sql: "PRAGMA query_only = ON") }
        return config
    }

    /// `schema_meta.version`, or nil when the file does not exist.
    static func schemaVersion(at url: URL) throws -> Int? {
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        // An empty file (a fresh "New…" target) is treated as missing: `migrate` fills it.
        if let size = try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int, size == 0 { return nil }
        let queue = try DatabaseQueue(path: url.path, configuration: readingConfiguration(label: "Ressac schema check"))
        return try queue.read { db in
            try Int.fetchOne(db, sql: "SELECT version FROM schema_meta")
        }
    }

    static func check(at url: URL) throws -> SchemaCheck {
        try check(at: url, known: knownSchemaVersion)
    }

    /// `check(at:)` against any known version, so that a test can play an older app
    /// opening a newer database (Review Focus #3).
    static func check(at url: URL, known: Int) throws -> SchemaCheck {
        guard let version = try schemaVersion(at: url) else { return .missing }
        if version < known { return .needsMigration(version) }
        if version > known { return .tooNew(version) }
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
