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
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows
        #expect(rows.prefix(8).map(\.number) == [100, 101, 102, 103, 800, 801, 802, 899])
        #expect(rows.prefix(8).map(\.title) == [
            "Index", "Unread", "Today", "Starred", "Needs triage", "All feeds", "Cemetery", "Guide",
        ])
        #expect(rows.prefix(8).map(\.unread) == [0, 10, 1, 4, 7, 0, 0, 0])
        #expect(rows.prefix(8).allSatisfy { $0.isReserved && $0.depth == 0 })
    }

    @Test func foldersAndFeedsFollowInDisplayOrderWithTheirDepth() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows.dropFirst(8)
        #expect(rows.map(\.number) == [200, 201, 400, 401, 402, 403, 110])
        #expect(rows.map(\.title) == [
            "Blogs", "xkcd.com", "Infos", "Daring Fireball", "Science", "Center for Humane Technology", "Loose feed",
        ])
        #expect(rows.map(\.depth) == [0, 1, 0, 1, 1, 2, 0])
        #expect(rows.map(\.unread) == [3, 3, 3, 2, 1, 1, 0])  // folders sum their subtree
        #expect(rows.allSatisfy { !$0.isReserved })
    }

    @Test func numbersLeadToTheirDestinations() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        #expect(directory.destination(for: 100) == .index)
        #expect(directory.destination(for: 899) == .guide)
        #expect(directory.destination(for: 101) == .source(.unread))
        #expect(directory.destination(for: 102) == .source(.today))
        #expect(directory.destination(for: 103) == .source(.starred))
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
