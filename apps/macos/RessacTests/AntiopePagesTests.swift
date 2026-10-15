import AppKit
import Foundation
import Testing

@testable import Ressac

@MainActor @Suite struct SommairePageTests {
    @Test func pagesRunDownTheLeftColumnThenTheRight() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows  // 8 reserved pages, 7 numbered
        let pairs = SommairePage.pairs(rows)
        #expect(pairs.map(\.left.number) == [100, 101, 102, 103, 800, 801, 802, 899])
        #expect(pairs.map { $0.right?.number } == [200, 201, 400, 401, 402, 403, 110, nil])
        #expect(SommairePage.pairs([]).isEmpty)
        #expect(SommairePage.pairs(Array(rows.prefix(2))).map { $0.right?.number } == [101])
    }

    @Test func everyRowFillsTheReaderWidth() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        for width in [0, 1, 2, 9, 40, 41, 80] {
            let half = SommairePage.halfWidth(width)
            #expect(half >= 0 && width - 2 * half >= min(width, 1))  // at least one cell between the columns
            for row in directory.rows {
                #expect(SommairePage.line(for: row, width: half, in: directory).width == half)
            }
            #expect(SommairePage.titleLine(width: width).width == width)
            #expect(SommairePage.logo(width: width).allSatisfy { $0.width == width })
        }
    }

    @Test func reservedPagesAreInColour() throws {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        let unread = SommairePage.line(for: directory.rows[1], width: 20, in: directory)
        #expect(unread.spans[1].foreground == .yellow)
        let triage = SommairePage.line(for: directory.rows[4], width: 20, in: directory)
        #expect(triage.spans[1].foreground == .magenta && triage.spans[2].foreground == .red)
        let blogs = try #require(directory.rows.first { $0.number == 200 })
        #expect(SommairePage.line(for: blogs, width: 20, in: directory).spans[1].foreground == .green)
        let xkcd = try #require(directory.rows.first { $0.number == 201 })
        #expect(SommairePage.line(for: xkcd, width: 20, in: directory).spans[1].foreground == .white)
    }

    @Test func theTitleIsCentredAndTheWaveIsUnbroken() {
        let title = SommairePage.titleLine(width: 20)
        #expect(text(of: title.spans[0]) == "       RESSAC       " && title.spans[0].foreground == .yellow)
        #expect(title.accessibilityText == "RESSAC")
        let logo = SommairePage.logo(width: 16)
        let top = Array(text(of: logo[0].spans[0]))
        let bottom = Array(text(of: logo[1].spans[0]))
        #expect(String(top) == " ▄▀▀▄    ▄▀▀▄   ")
        #expect(String(bottom) == "▀    ▀▄▄▀    ▀▄▄")
        for column in 0..<16 {
            #expect((top[column] == " ") != (bottom[column] == " "), "column \(column) holds one block")
        }
        #expect(logo.allSatisfy { $0.spans.allSatisfy { $0.foreground == .cyan } && $0.accessibilityText.isEmpty })
    }
}
