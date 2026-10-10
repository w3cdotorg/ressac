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
