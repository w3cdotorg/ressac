import Foundation

/// Page numbers fixed in code (spec §4, Reserved pages; sub-project 3 adds 104 to 109): never
/// stored, refused by `ressac page set`.
enum ReservedPage: Int, CaseIterable, Sendable {
    case index = 100, unread = 101, today = 102, starred = 103
    case journal = 104, daily = 105, weekly = 106, monthly = 107, rare = 108, silent = 109
    case needsTriage = 800, allFeeds = 801, cemetery = 802, guide = 899

    /// The core's titles (`pages::list`), so that the app and `ressac page list` agree.
    var title: String {
        switch self {
        case .index: "Index"
        case .unread: "Unread"
        case .today: "Today"
        case .starred: "Starred"
        case .journal: "Journal"
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        case .rare: "Rare"
        case .silent: "Silent"
        case .needsTriage: "Needs triage"
        case .allFeeds: "All feeds"
        case .cemetery: "Cemetery"
        case .guide: "Guide"
        }
    }

    /// The rhythm group a page 105 to 109 lists; nil for the other pages.
    var cadenceGroup: CadenceGroup? {
        switch self {
        case .daily: .daily
        case .weekly: .weekly
        case .monthly: .monthly
        case .rare: .rare
        case .silent: .silent
        case .index, .unread, .today, .starred, .journal, .needsTriage, .allFeeds, .cemetery, .guide: nil
        }
    }

    var destination: PageDestination {
        switch self {
        case .index: .index
        case .unread: .source(.unread)
        case .today: .source(.today)
        case .starred: .source(.starred)
        case .journal: .source(.journal)
        case .daily: .source(.cadence(.daily))
        case .weekly: .source(.cadence(.weekly))
        case .monthly: .source(.cadence(.monthly))
        case .rare: .source(.cadence(.rare))
        case .silent: .source(.cadence(.silent))
        case .needsTriage: .source(.garden(.needsTriage))
        case .allFeeds: .source(.garden(.all))
        case .cemetery: .source(.garden(.cemetery))
        case .guide: .guide
        }
    }

    /// The count shown beside the page: unread entries for the smart lists, the edition and the
    /// rhythm groups, feeds to triage for P800.
    func count(in snapshot: SidebarSnapshot) -> Int {
        switch self {
        case .unread: snapshot.unread
        case .today: snapshot.today
        case .starred: snapshot.starred
        case .journal: snapshot.journalUnread
        case .daily, .weekly, .monthly, .rare, .silent:
            cadenceGroup.map { snapshot.cadenceUnread[$0, default: 0] } ?? 0
        case .needsTriage: snapshot.needsTriage
        case .index, .allFeeds, .cemetery, .guide: 0
        }
    }

    /// The Garden's three pages, drawn in magenta in the index (spec §5).
    var isGarden: Bool {
        switch self {
        case .needsTriage, .allFeeds, .cemetery: true
        case .index, .unread, .today, .starred, .journal, .daily, .weekly, .monthly, .rare, .silent, .guide: false
        }
    }
}

/// Where a page number leads: a source of the window, or one of the two pages drawn in the
/// reader column (P100 SOMMAIRE, P899 GUIDE).
enum PageDestination: Equatable, Sendable {
    case source(Source)
    case index
    case guide
}

/// Every page of the library (spec §6, Page numbers): the reserved pages, then the folders and
/// feeds the core numbered (`folder.page`, `feed.page`), in the sidebar's display order.
struct PageDirectory: Equatable, Sendable {
    struct Row: Equatable, Sendable {
        var number: Int
        var title: String
        /// Unread entries (a folder sums its subtree), or the reserved page's `count(in:)`.
        var unread: Int
        var isReserved: Bool
        /// 0 for reserved pages and root items, one more per folder level.
        var depth: Int
    }

    /// Reserved pages, then folders and feeds depth-first in display order.
    let rows: [Row]
    private let destinations: [Int: PageDestination]
    private let numbers: [Source: Int]
    private let feedPages: [Int64: Int]

    init(snapshot: SidebarSnapshot) {
        var rows: [Row] = []
        var destinations: [Int: PageDestination] = [:]
        var numbers: [Source: Int] = [:]
        var feedPages: [Int64: Int] = [:]
        for page in ReservedPage.allCases {
            rows.append(Row(number: page.rawValue, title: page.title, unread: page.count(in: snapshot),
                            isReserved: true, depth: 0))
            destinations[page.rawValue] = page.destination
            if case let .source(source) = page.destination { numbers[source] = page.rawValue }
        }
        func walk(_ nodes: [SidebarNode], depth: Int) {
            for node in nodes {
                // A node not numbered yet is skipped; a reserved number can only come from a
                // corrupt database and never hides the reserved page.
                if let number = node.page, ReservedPage(rawValue: number) == nil {
                    rows.append(Row(number: number, title: node.title, unread: node.unread, isReserved: false, depth: depth))
                    destinations[number] = .source(node.source)
                    numbers[node.source] = number
                    if case let .feed(feed) = node.kind { feedPages[feed.feedId] = number }
                }
                walk(node.children ?? [], depth: depth + 1)
            }
        }
        walk(SidebarNode.tree(from: snapshot), depth: 0)
        self.rows = rows
        self.destinations = destinations
        self.numbers = numbers
        self.feedPages = feedPages
    }

    /// Nil for a number no page holds: the header then shows `NOT FOUND`.
    func destination(for number: Int) -> PageDestination? {
        destinations[number]
    }

    /// The page of a source, reserved ones included; nil while a folder or feed has no number.
    func number(for source: Source) -> Int? {
        numbers[source]
    }

    /// A feed's page, for the reader's sub-page line (`412 3/20`, phase 10).
    func page(forFeed feedId: Int64) -> Int? {
        feedPages[feedId]
    }
}

private extension SidebarNode {
    var page: Int? {
        switch kind {
        case let .folder(folder): folder.page
        case let .feed(feed): feed.page
        }
    }
}
