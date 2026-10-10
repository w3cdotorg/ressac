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
