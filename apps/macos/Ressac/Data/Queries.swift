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
                    db, sql: "SELECT COUNT(*) FROM v_entry WHERE published_at >= ?",
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
