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

/// Where a long entry list stops: the `published_at` text and the id of its oldest loaded row.
/// Rows at or after it (the lists run newest first) are loaded. The raw text, so that SQLite
/// compares it the way it sorts the column.
struct ListFloor: Comparable, Sendable {
    var publishedAt: String
    var entryId: Int64

    /// Below every row: the whole list, loaded to its end on purpose.
    static let oldest = ListFloor(publishedAt: "", entryId: .min)

    static func < (a: ListFloor, b: ListFloor) -> Bool {
        (a.publishedAt, a.entryId) < (b.publishedAt, b.entryId)
    }

    /// Whether `row` sorts at or after the floor.
    func admits(_ row: EntryRow) -> Bool {
        guard let date = Timestamp.date(from: publishedAt) else { return true }
        return (row.publishedAt, row.entryId) >= (date, entryId)
    }
}

/// The loaded rows of an entry list, newest first, and the size of the whole list.
struct EntryPage: Equatable, Sendable {
    var rows: [EntryRow] = []
    /// Rows of the whole list, loaded or not.
    var total = 0
    /// Unread rows of the whole list, loaded or not.
    var unread = 0
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
                    SELECT id AS folder_id, parent_id, name, position, page FROM folder ORDER BY position, id
                    """),
                feeds: try FeedItem.fetchAll(db, sql: """
                    SELECT f.id AS feed_id, f.title, f.folder_id, f.position, COALESCE(u.unread, 0) AS unread,
                           h.status, f.prefer_full_text, f.html_url, f.page
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

    /// The entry list of a source, newest first, down to `floor` (every row when nil). Garden
    /// sources have no entries.
    static func entries(source: Source, unreadOnly: Bool, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: unreadOnly, search: nil, now: now), floor: floor)
    }

    /// Full-text search within a source (spec §7, Search), down to `floor` like `entries`. Every
    /// word must appear (see `ftsQuery`).
    static func search(_ text: String, in source: Source, floor: ListFloor? = nil, now: Date = Date()) -> LiveQuery<EntryPage> {
        page(of: condition(source: source, unreadOnly: false, search: text, now: now), floor: floor)
    }

    /// The floor that loads `rows` more rows of a list after `below` (from its newest row when
    /// nil), lowered to entry `keeping` when that entry sorts older. Nil when fewer rows are
    /// left: they all load. `search` nil is the plain list.
    static func floor(source: Source, unreadOnly: Bool, search: String?, below: ListFloor?, rows: Int,
                      keeping entryId: Int64? = nil, now: Date = Date(), in db: Database) throws -> ListFloor? {
        guard let list = condition(source: source, unreadOnly: unreadOnly, search: search, now: now) else { return nil }
        let rest = bounded(list, by: below, "<")
        guard let row = try Row.fetchOne(db, sql: """
            SELECT published_at, entry_id FROM v_entry WHERE \(rest.sql)
            ORDER BY published_at DESC, entry_id DESC LIMIT 1 OFFSET ?
            """, arguments: rest.arguments + [max(rows, 1) - 1])
        else { return nil }
        let floor = ListFloor(publishedAt: row["published_at"], entryId: row["entry_id"])
        guard let entryId, let kept = try Row.fetchOne(db, sql: "SELECT published_at, id FROM entry WHERE id = ?", arguments: [entryId])
        else { return floor }
        return min(floor, ListFloor(publishedAt: kept["published_at"], entryId: kept["id"]))
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
        // `v_unread_count` covers active feeds only, so the Cemetery counts on `entry` itself. The
        // other scopes leave `entry` out: reading or fetching entries does not refetch them.
        let unread = scope == .cemetery
            ? "(SELECT COUNT(*) FROM entry e WHERE e.feed_id = v_garden_row.feed_id AND e.read_at IS NULL)"
            : "0"
        return ValueObservation.tracking { db in
            try GardenRow.fetchAll(db, sql: """
                SELECT feed_id, title, folder_path, state, status, reason, clue, evidence,
                       COALESCE(remarks, '[]') AS remarks, proposed_action, needs_triage, last_decision,
                       wayback_url, html_url, xml_url, \(unread) AS unread
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

    /// The first unread row of a list after entry `entryId` (older, in the list's order; from its
    /// newest row when nil), as the floor that loads it. Nil when none is left.
    static func nextUnread(source: Source, unreadOnly: Bool, search: String?, after entryId: Int64?,
                           now: Date = Date(), in db: Database) throws -> ListFloor? {
        guard let list = condition(source: source, unreadOnly: unreadOnly, search: search, now: now) else { return nil }
        var rest = Predicate(sql: list.sql + " AND NOT is_read", arguments: list.arguments)
        if let entryId, let from = try Row.fetchOne(db, sql: "SELECT published_at, id FROM entry WHERE id = ?", arguments: [entryId]) {
            rest = bounded(rest, by: ListFloor(publishedAt: from["published_at"], entryId: from["id"]), "<")
        }
        return try Row.fetchOne(db, sql: """
            SELECT published_at, entry_id FROM v_entry WHERE \(rest.sql)
            ORDER BY published_at DESC, entry_id DESC LIMIT 1
            """, arguments: rest.arguments).map { ListFloor(publishedAt: $0["published_at"], entryId: $0["entry_id"]) }
    }

    /// The `v_entry` rows of a list: a source's, unread only or not, or those matching `search`
    /// (which ignores the unread filter). Nil when the list is empty by definition: a garden
    /// source, a search without words.
    private static func condition(source: Source, unreadOnly: Bool, search: String?, now: Date) -> Predicate? {
        guard let filter = predicate(for: source, now: now) else { return nil }
        guard let search else {
            return unreadOnly ? Predicate(sql: filter.sql + " AND NOT is_read", arguments: filter.arguments) : filter
        }
        guard let match = ftsQuery(search) else { return nil }
        return Predicate(sql: "entry_id IN (SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?) AND \(filter.sql)",
                         arguments: [match] + filter.arguments)
    }

    /// `list` cut at `floor` with `comparison` (`>=`: the rows at or after it; `<`: the older
    /// ones); `list` itself when there is no floor.
    private static func bounded(_ list: Predicate, by floor: ListFloor?, _ comparison: String) -> Predicate {
        guard let floor else { return list }
        return Predicate(sql: list.sql + " AND (published_at, entry_id) \(comparison) (?, ?)",
                         arguments: list.arguments + [floor.publishedAt, floor.entryId])
    }

    /// The rows of `list` down to `floor`, and the counts of the whole list.
    private static func page(of list: Predicate?, floor: ListFloor?) -> LiveQuery<EntryPage> {
        ValueObservation.tracking { db in
            guard let list else { return EntryPage() }
            let loaded = bounded(list, by: floor, ">=")
            let rows = try EntryRow.fetchAll(db, sql: """
                SELECT \(listColumns) FROM v_entry WHERE \(loaded.sql)
                ORDER BY published_at DESC, entry_id DESC
                """, arguments: loaded.arguments)
            guard let counts = try Row.fetchOne(db, sql: """
                SELECT COUNT(*) AS total, COALESCE(SUM(NOT is_read), 0) AS unread FROM v_entry WHERE \(list.sql)
                """, arguments: list.arguments)
            else { return EntryPage(rows: rows, total: rows.count) }
            return EntryPage(rows: rows, total: counts["total"], unread: counts["unread"])
        }
        .removeDuplicates()
    }

    /// `"word1" "word2"`: every token quoted, so FTS5 operators typed by the user stay literal.
    static func ftsQuery(_ text: String) -> String? {
        let tokens = text.split(whereSeparator: \.isWhitespace).map {
            "\"" + $0.replacing("\"", with: "\"\"") + "\""
        }
        return tokens.isEmpty ? nil : tokens.joined(separator: " ")
    }
}
