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
    /// An action type newer than this app (a newer core wrote it): shown, never applied.
    case unknown(String)

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
        case let other: self = .unknown(other)
        }
    }

    func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .unknown(type): try c.encode(type, forKey: .type)
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
    var isRepair: Bool {
        switch self {
        case .bury, .unknown: false
        default: true
        }
    }

    /// Same wording as the core's `Action::label`.
    var label: String {
        switch self {
        case let .upgradeHttps(url): "switch to \(url)"
        case let .followRedirect(url): "follow redirect to \(url)"
        case let .adoptDiscovered(url): "adopt discovered feed \(url)"
        case let .mergeInto(feedId): "merge into #\(feedId)"
        case .bury: "bury → cemetery"
        case let .unknown(type): "\(type) (newer than this app)"
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
