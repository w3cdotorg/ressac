import Foundation
import SwiftUI
import Testing
import WebKit
@testable import Ressac

private let articleURL = "https://blog.example.org/2026/10/post.html"

private func entry(id: Int64 = 1, title: String? = "A title", url: String? = articleURL,
                   summary: String? = "<p>Feed summary</p>", content: String? = nil) -> EntryRow {
    EntryRow(entryId: id, feedId: 7, feedTitle: "Example Blog", folderId: nil, title: title, author: nil, url: url,
             publishedAt: Date(timeIntervalSince1970: 1_791_619_200), isRead: false, isStarred: false,
             hasFulltext: false, summaryHtml: summary, contentHtml: content)
}

@Suite struct ReaderStyleTests {
    @Test func defaultsMatchTheSpec() {
        let style = ReaderStyle()
        #expect(style.font == .sfPro)
        #expect(style.size == 17)
        #expect(style.width == .medium)
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80])
        #expect(ReaderFont.allCases.map(\.displayName) == ["SF Pro", "New York", "Charter", "Newsreader", "Atkinson Hyperlegible Next"])
    }

    @Test func textSizeStepsAndClamps() {
        #expect(ReaderStyle.bigger(17) == 18)
        #expect(ReaderStyle.smaller(17) == 16)
        #expect(ReaderStyle.bigger(28) == 28)
        #expect(ReaderStyle.smaller(12) == 12)
        #expect(ReaderStyle.clampedSize(40) == 28)
        #expect(ReaderStyle.clampedSize(3) == 12)
        #expect(ReaderStyle.clampedSize(16.6) == 17)
        #expect(ReaderStyle.clampedSize(.nan) == 17)
    }

    @Test func styleLoadsFromDefaultsAndIgnoresGarbage() throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle())
        defaults.set("newsreader", forKey: ReaderStyle.Keys.font)
        defaults.set(21.0, forKey: ReaderStyle.Keys.size)
        defaults.set("wide", forKey: ReaderStyle.Keys.width)
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle(font: .newsreader, size: 21, width: .wide))
        defaults.set("comic-sans", forKey: ReaderStyle.Keys.font)
        defaults.set(99.0, forKey: ReaderStyle.Keys.size)
        defaults.set("huge", forKey: ReaderStyle.Keys.width)
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle(font: .sfPro, size: 28, width: .medium))
    }

    @Test func cssFamiliesPutTheChosenFaceFirst() {
        #expect(ReaderFont.sfPro.cssFamily.hasPrefix("-apple-system"))
        #expect(ReaderFont.newYork.cssFamily.hasPrefix("ui-serif"))
        #expect(ReaderFont.charter.cssFamily.hasPrefix("Charter"))
        #expect(ReaderFont.newsreader.cssFamily.hasPrefix("\"Newsreader\""))
        #expect(ReaderFont.atkinson.cssFamily.hasPrefix("\"Atkinson Hyperlegible Next\""))
        #expect(ReaderFont.sfPro.bundledFaces.isEmpty)
        #expect(ReaderFont.newsreader.bundledFaces.map(\.italic) == [false, true])
    }
}
