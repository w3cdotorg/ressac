import Foundation
import GRDB

/// The core's `CadenceGroup`: a feed's rhythm, computed (`feed_health.cadence_group`) or set by
/// Clément (`feed.cadence_override`). Spec §4.
enum CadenceGroup: String, CaseIterable, Sendable, Codable, Hashable {
    case daily, weekly, monthly, rare, silent

    var title: String {
        switch self {
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        case .rare: "Rare"
        case .silent: "Silent"
        }
    }

    /// The group's reserved Antiope page (spec §5).
    var page: Int {
        switch self {
        case .daily: 105
        case .weekly: 106
        case .monthly: 107
        case .rare: 108
        case .silent: 109
        }
    }
}

/// The core's `edition::Section`, in reading order.
enum EditionSection: String, CaseIterable, Sendable, Codable {
    case front, week, wire

    var title: String {
        switch self {
        case .front: "Front Page"
        case .week: "This Week"
        case .wire: "The Wire"
        }
    }

    var antiopeTitle: String { title.uppercased() }
}

/// The header of an edition (`edition` table).
struct EditionInfo: RessacRecord {
    var id: Int64
    var builtAt: Date
    var windowStart: Date
    var windowEnd: Date
    /// Feeds whose fetch failed while the edition was built.
    var unreachable: Int
}

/// One entry of an edition (`v_edition_today`), with its live read and starred state.
struct EditionItem: RessacRecord, Identifiable {
    var entryId: Int64
    var section: EditionSection
    /// 1-based across the whole edition: the Antiope sub-page number.
    var rank: Int
    var feedId: Int64
    var feedTitle: String
    var title: String?
    var publishedAt: Date
    var isRead: Bool
    var isStarred: Bool

    var id: Int64 { entryId }
}

/// "+12 more in Actualitté": entries a cap left unread in their feed (`v_edition_overflow`).
struct EditionOverflowItem: RessacRecord {
    var section: EditionSection
    var feedId: Int64
    var feedTitle: String
    var page: Int?
    var hiddenCount: Int
}

/// The latest edition: items by rank, overflow by section then page.
struct Edition: Equatable, Sendable {
    var info: EditionInfo
    var items: [EditionItem]
    var overflow: [EditionOverflowItem]

    var readCount: Int { items.count(where: \.isRead) }

    /// Every entry read. An empty edition is never "finished": it says "Nothing new this morning."
    var isFinished: Bool { !items.isEmpty && readCount == items.count }
}

/// One active, diagnosed feed's rhythm (`v_cadence`).
struct CadenceRow: RessacRecord, Identifiable {
    var feedId: Int64
    var effective: CadenceGroup
    var computed: CadenceGroup
    var isOverride: Bool
    var medianIntervalDays: Double?
    var entriesPerMonth: Double
    var lastEntryAt: Date?
    var unread: Int

    var id: Int64 { feedId }
}

/// When a morning edition is due: the day starts at 04:00 local time (spec §6, "When").
enum EditionClock {
    static let hour = 4

    /// 04:00 local of the day `now` belongs to: before 04:00, the previous day's 04:00.
    /// Mirrors the core's `edition::day_start`, DST changes included.
    static func dayStart(for now: Date, calendar: Calendar = .autoupdatingCurrent) -> Date {
        let today = fourOClock(on: now, calendar)
        if now >= today { return today }
        let yesterday = calendar.date(byAdding: .day, value: -1, to: now) ?? now.addingTimeInterval(-86_400)
        return fourOClock(on: yesterday, calendar)
    }

    private static func fourOClock(on day: Date, _ calendar: Calendar) -> Date {
        calendar.date(bySettingHour: hour, minute: 0, second: 0, of: day)
            ?? calendar.startOfDay(for: day).addingTimeInterval(TimeInterval(hour * 3_600))
    }
}
