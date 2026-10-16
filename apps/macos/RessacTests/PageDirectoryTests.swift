import Testing

@testable import Ressac

/// A small library with page numbers: Blogs (200) with xkcd (201); Infos (400) with Daring
/// Fireball (401) and the subfolder Science (402) holding one feed (403); a feed without a folder
/// (110) and one the core has not numbered yet.
func pagedSnapshot() -> SidebarSnapshot {
    func folder(_ id: Int64, _ name: String, parent: Int64? = nil, position: Int, page: Int?) -> FolderItem {
        var item = FolderItem(folderId: id, parentId: parent, name: name, position: position)
        item.page = page
        return item
    }
    func feed(_ id: Int64, _ title: String, folder: Int64?, position: Int, unread: Int, page: Int?) -> FeedItem {
        var item = FeedItem(feedId: id, title: title, folderId: folder, position: position, unread: unread,
                            status: nil, preferFullText: false, htmlUrl: nil)
        item.page = page
        return item
    }
    return SidebarSnapshot(
        folders: [
            folder(1, "Blogs", position: 0, page: 200),
            folder(4, "Infos", position: 2, page: 400),
            folder(5, "Science", parent: 4, position: 3, page: 402),
        ],
        feeds: [
            feed(1, "xkcd.com", folder: 1, position: 0, unread: 3, page: 201),
            feed(8, "Daring Fireball", folder: 4, position: 0, unread: 2, page: 401),
            feed(10, "Center for Humane Technology", folder: 5, position: 0, unread: 1, page: 403),
            feed(13, "Loose feed", folder: nil, position: 5, unread: 0, page: 110),
            feed(12, "Brand new", folder: nil, position: 9, unread: 4, page: nil),
        ],
        unread: 10, today: 1, starred: 4, needsTriage: 7)
}

@Suite struct PageDirectoryTests {
    @Test func reservedPagesComeFirstWithTheirCounts() {
        var snapshot = pagedSnapshot()
        snapshot.journalUnread = 43
        snapshot.cadenceUnread = [.daily: 34, .weekly: 7, .monthly: 2]
        let rows = PageDirectory(snapshot: snapshot).rows
        #expect(rows.prefix(14).map(\.number) == [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899])
        #expect(rows.prefix(14).map(\.title) == [
            "Index", "Unread", "Today", "Starred", "Journal", "Daily", "Weekly", "Monthly", "Rare", "Silent",
            "Needs triage", "All feeds", "Cemetery", "Guide",
        ])
        #expect(rows.prefix(14).map(\.unread) == [0, 10, 1, 4, 43, 34, 7, 2, 0, 0, 7, 0, 0, 0])
        #expect(rows.prefix(14).allSatisfy { $0.isReserved && $0.depth == 0 })
    }

    @Test func foldersAndFeedsFollowInDisplayOrderWithTheirDepth() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows.dropFirst(14)
        #expect(rows.map(\.number) == [200, 201, 400, 401, 402, 403, 110])
        #expect(rows.map(\.title) == [
            "Blogs", "xkcd.com", "Infos", "Daring Fireball", "Science", "Center for Humane Technology", "Loose feed",
        ])
        #expect(rows.map(\.depth) == [0, 1, 0, 1, 1, 2, 0])
        #expect(rows.map(\.unread) == [3, 3, 3, 2, 1, 1, 0])  // folders sum their subtree
        #expect(rows.allSatisfy { !$0.isReserved })
    }

    /// The app and `ressac page list` agree (core `RESERVED_PAGES`, schema v4), and each rhythm
    /// page is its group's (`CadenceGroup.page`, phase 4a).
    @Test func reservedPagesMatchTheCoreAndTheRhythmGroups() {
        #expect(ReservedPage.allCases.map(\.rawValue) == [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899])
        for group in CadenceGroup.allCases {
            let page = ReservedPage(rawValue: group.page)
            #expect(page?.cadenceGroup == group)
            #expect(page?.title == group.title)
            #expect(page?.destination == .source(.cadence(group)))
            #expect(page?.isGarden == false)
        }
        #expect(ReservedPage.journal.cadenceGroup == nil && ReservedPage.unread.cadenceGroup == nil)
        #expect(ReservedPage.journal.destination == .source(.journal) && !ReservedPage.journal.isGarden)
    }

    @Test func numbersLeadToTheirDestinations() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        #expect(directory.destination(for: 100) == .index)
        #expect(directory.destination(for: 899) == .guide)
        #expect(directory.destination(for: 101) == .source(.unread))
        #expect(directory.destination(for: 102) == .source(.today))
        #expect(directory.destination(for: 103) == .source(.starred))
        #expect(directory.destination(for: 104) == .source(.journal))
        #expect(directory.destination(for: 105) == .source(.cadence(.daily)))
        #expect(directory.destination(for: 106) == .source(.cadence(.weekly)))
        #expect(directory.destination(for: 107) == .source(.cadence(.monthly)))
        #expect(directory.destination(for: 108) == .source(.cadence(.rare)))
        #expect(directory.destination(for: 109) == .source(.cadence(.silent)))
        #expect(directory.destination(for: 800) == .source(.garden(.needsTriage)))
        #expect(directory.destination(for: 801) == .source(.garden(.all)))
        #expect(directory.destination(for: 802) == .source(.garden(.cemetery)))
        #expect(directory.destination(for: 401) == .source(.feed(8)))
        #expect(directory.destination(for: 402) == .source(.folder(5)))
        #expect(directory.destination(for: 110) == .source(.feed(13)))
        for unknown in [0, 7, 99, 437, 799, 900, 999] {
            #expect(directory.destination(for: unknown) == nil)
        }
    }

    @Test func sourcesAndFeedsFindTheirNumbers() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        #expect(directory.number(for: .unread) == 101)
        #expect(directory.number(for: .journal) == 104)
        #expect(directory.number(for: .cadence(.rare)) == 108)
        #expect(directory.number(for: .garden(.cemetery)) == 802)
        #expect(directory.number(for: .folder(4)) == 400)
        #expect(directory.number(for: .feed(10)) == 403)
        #expect(directory.number(for: .feed(12)) == nil)  // not numbered yet
        #expect(directory.page(forFeed: 8) == 401)
        #expect(directory.page(forFeed: 12) == nil)
        #expect(directory.page(forFeed: 999) == nil)
        #expect(directory == PageDirectory(snapshot: pagedSnapshot()))
    }

    @Test func aNumberCollidingWithAReservedPageIsIgnored() {
        var snapshot = pagedSnapshot()
        snapshot.feeds[3].page = 101  // only a corrupt database could hold this
        let directory = PageDirectory(snapshot: snapshot)
        #expect(directory.destination(for: 101) == .source(.unread))
        #expect(directory.rows.filter { $0.number == 101 }.count == 1)
        #expect(directory.number(for: .feed(13)) == nil)
        snapshot.feeds[2].page = 104  // a corrupt database again: P104 stays the Journal
        let again = PageDirectory(snapshot: snapshot)
        #expect(again.destination(for: 104) == .source(.journal))
        #expect(again.number(for: .feed(10)) == nil)
    }
}

@Suite struct PageEntryTests {
    @Test func theThirdDigitCompletesTheNumber() {
        var entry = PageEntry()
        #expect(!entry.isActive && entry.display == "")
        #expect(entry.type(4) == .pending)
        #expect(entry.isActive && entry.display == "4__")
        #expect(entry.type(1) == .pending)
        #expect(entry.display == "41_")
        #expect(entry.type(2) == .complete(412))
        #expect(!entry.isActive && entry.display == "" && entry.digits == "")
        #expect(entry.type(8) == .pending)  // the next digit starts a new number
        #expect(entry.display == "8__")
    }

    @Test func correctionErasesTheLastDigit() {
        var entry = PageEntry()
        entry.correct()  // nothing typed: nothing happens
        #expect(!entry.isActive)
        _ = entry.type(4)
        _ = entry.type(1)
        entry.correct()
        #expect(entry.display == "4__")
        entry.correct()
        #expect(!entry.isActive)
        _ = entry.type(8)
        _ = entry.type(0)
        #expect(entry.type(1) == .complete(801))
    }

    @Test func cancelEmptiesTheEntry() {
        var entry = PageEntry()
        _ = entry.type(4)
        _ = entry.type(1)
        entry.cancel()
        #expect(entry == PageEntry())
        #expect(entry.type(1) == .pending)
    }

    @Test func leadingZerosAndStrayValues() {
        var entry = PageEntry()
        #expect(entry.type(12) == .pending)  // not a digit: ignored
        #expect(entry.type(-1) == .pending)
        #expect(!entry.isActive)
        _ = entry.type(0)
        _ = entry.type(4)
        #expect(entry.display == "04_")
        #expect(entry.type(2) == .complete(42))  // no page has it: the caller shows NOT FOUND
    }
}

@MainActor @Suite struct ReservedRhythmPageTests {
    @Test func typingTheNewPagesSelectsTheirSources() throws {
        let (state, _, _) = try makeState()
        let antiope = AntiopeState(app: state)
        for (digits, source) in [("104", Source.journal), ("105", .cadence(.daily)), ("109", .cadence(.silent))] {
            for digit in digits.compactMap(\.wholeNumberValue) {
                #expect(antiope.handle(.pageDigit(digit)))
            }
            #expect(state.selection == source && antiope.readerPage == .article)
            #expect(antiope.currentPage == Int(digits))
        }
    }
}
