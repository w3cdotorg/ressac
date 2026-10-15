import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Ressac

/// The characters a span draws (replacements included), for assertions on a row's text.
func text(of span: CellSpan) -> String {
    span.cells.runs.map(\.text).joined()
}

@MainActor @Suite struct CellLineTests {
    @Test func spansAddUpToTheLineWidth() {
        let line = CellLine([
            CellSpan("101 ", cells: 4, foreground: .cyan),
            CellSpan("Unread", cells: 13, foreground: .white),
            CellSpan(" 1083", cells: 5, foreground: .yellow),
        ])
        #expect(line.width == 22)
        #expect(line.spans.map(\.cellCount) == [4, 13, 5])
        #expect(line.accessibilityText == "101 Unread 1083")  // the original strings, trimmed
        #expect(CellSpan("far too long for the span", cells: 3, foreground: .white).cellCount == 3)
        #expect(CellSpan("negative", cells: -2, foreground: .white).cellCount == 0)
    }

    @Test func replacementsAreMagentaWhateverTheSpanColour() {
        var span = CellSpan("", cells: 0, foreground: .yellow)
        span.cells = FittedCells(runs: [CellRun(text: "A", isReplacement: false), CellRun(text: "?", isReplacement: true)],
                                 accessibilityText: "A🎉")
        let colours: [Color?] = span.attributedText.runs.map { $0.foregroundColor }  // the SwiftUI attribute
        #expect(colours == [TeletextColor.yellow.color, TeletextColor.magenta.color])
        #expect(String(span.attributedText.characters) == "A?")
    }

    @Test func reverseVideoKeepsTheTextAndTheLabel() {
        let line = CellLine([CellSpan("101 ", cells: 4, foreground: .cyan), CellSpan("Unread", cells: 8, foreground: .white)],
                            accessibilityText: "Unread, page 101")
        let reversed = line.reversed(.black, on: .cyan)
        #expect(reversed.spans.allSatisfy { $0.foreground == .black && $0.background == .cyan })
        #expect(reversed.spans.map(text(of:)) == line.spans.map(text(of:)))
        #expect(reversed.accessibilityText == "Unread, page 101" && reversed.width == 12)
    }

    @Test func blankIsBlackCellsWithNothingToRead() {
        let blank = CellLine.blank(5)
        #expect(blank.width == 5 && blank.accessibilityText.isEmpty)
        #expect(blank.spans.allSatisfy { $0.background == .black })
        #expect(CellLine.blank(0).width == 0)
    }
}

/// Titles that used to bend columns: emoji, CJK, combining accents, very long, empty.
let awkwardTitles = [
    "Café 🎉 東京", "e\u{301}cole de\u{301}ja\u{300} la\u{300}", "👩🏽‍💻👨‍👩‍👧‍👦🇫🇷🏳️‍🌈",
    String(repeating: "Long title with words ", count: 12), "Supercalifragilisticexpialidociously-long-hyphenated",
    "", "   ", "Tab\tand\nnewline", "مرحبا بالعالم", "Ελληνικά και Русский",
]

func listEntry(title: String?, isRead: Bool = false, isStarred: Bool = false) -> EntryRow {
    EntryRow(entryId: 1, feedId: 8, feedTitle: "Daring Fireball", folderId: 4, title: title, author: nil, url: nil,
             publishedAt: Date(timeIntervalSince1970: 1_791_619_200), isRead: isRead, isStarred: isStarred,
             hasFulltext: false, summaryHtml: nil, contentHtml: nil)
}

@MainActor @Suite struct IndexColumnTests {
    func row(_ number: Int, _ title: String, unread: Int = 0, reserved: Bool = false, depth: Int = 0) -> PageDirectory.Row {
        PageDirectory.Row(number: number, title: title, unread: unread, isReserved: reserved, depth: depth)
    }

    /// Review Focus #1: every index row is exactly 22 cells, whatever the title or the count.
    @Test func everyRowIsExactly22Cells() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        var rows = directory.rows
        rows += awkwardTitles.map { row(412, $0, unread: 3, depth: 1) }
        rows += [row(413, "Huge", unread: 1_234_567), row(414, "Deep", depth: 9)]
        for row in rows {
            for selected in [false, true] {
                for highlights in [false, true] {
                    let line = PageIndexColumn.line(for: row, width: PageIndexColumn.cells,
                                                    isRootFolder: PageIndexColumn.isRootFolder(row, in: directory),
                                                    isSelected: selected, highlightsReserved: highlights)
                    #expect(line.width == 22, "\(row.title)")
                }
            }
        }
        for width in [0, 1, 3, 4, 8, 11, 30] {
            #expect(PageIndexColumn.line(for: row(101, "Unread", unread: 1083, reserved: true), width: width,
                                         isRootFolder: false, isSelected: false).width == width)
        }
    }

    @Test func coloursFollowTheSpec() throws {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        let unread = PageIndexColumn.line(for: row(101, "Unread", unread: 1083, reserved: true), width: 22,
                                          isRootFolder: false, isSelected: false)
        #expect(unread.spans.map(text(of:)) == ["101 ", "Unread       ", " 1083"])
        #expect(unread.spans.map(\.foreground) == [.cyan, .white, .yellow])
        #expect(unread.accessibilityText == "Unread, page 101, 1083")

        let infos = try #require(directory.rows.first { $0.number == 400 })
        #expect(PageIndexColumn.isRootFolder(infos, in: directory))
        let folder = PageIndexColumn.line(for: infos, width: 22, isRootFolder: true, isSelected: false)
        #expect(text(of: folder.spans[1]).hasPrefix("INFOS") && folder.spans[1].foreground == .green)

        let science = try #require(directory.rows.first { $0.number == 402 })
        #expect(!PageIndexColumn.isRootFolder(science, in: directory))  // a subfolder
        let nested = PageIndexColumn.line(for: science, width: 22, isRootFolder: false, isSelected: false)
        #expect(text(of: nested.spans[1]).hasPrefix(" Science") && nested.spans[1].foreground == .white)

        let garden = PageIndexColumn.line(for: row(800, "Needs triage", unread: 7, reserved: true), width: 22,
                                          isRootFolder: false, isSelected: false)
        #expect(garden.spans.map(\.foreground) == [.cyan, .magenta, .red])
        let cemetery = PageIndexColumn.line(for: row(802, "Cemetery", reserved: true), width: 22,
                                            isRootFolder: false, isSelected: false)
        #expect(cemetery.spans[1].foreground == .magenta && cemetery.spans[2].cellCount == 0)  // no count

        let selected = PageIndexColumn.line(for: row(101, "Unread", unread: 1083, reserved: true), width: 22,
                                            isRootFolder: false, isSelected: true)
        #expect(selected.spans.allSatisfy { $0.foreground == .black && $0.background == .cyan })
    }

    @Test func theSommaireColoursTheOtherReservedTitles() {
        let today = PageIndexColumn.line(for: row(102, "Today", unread: 4, reserved: true), width: 20,
                                         isRootFolder: false, isSelected: false, highlightsReserved: true)
        #expect(today.spans[1].foreground == .yellow)
        let triage = PageIndexColumn.line(for: row(800, "Needs triage", unread: 7, reserved: true), width: 20,
                                          isRootFolder: false, isSelected: false, highlightsReserved: true)
        #expect(triage.spans[1].foreground == .magenta)  // the Garden stays magenta
        let feed = PageIndexColumn.line(for: row(401, "Daring Fireball", unread: 2, depth: 1), width: 20,
                                        isRootFolder: false, isSelected: false, highlightsReserved: true)
        #expect(feed.spans[1].foreground == .white)
    }
}

@MainActor @Suite struct EntryColumnTests {
    /// Review Focus #1: every entry row is exactly 28 cells, one or two rows per entry.
    @Test func everyRowIsExactly28Cells() {
        for title in awkwardTitles + [nil] {
            for isRead in [false, true] {
                for isStarred in [false, true] {
                    for selected in [false, true] {
                        let lines = EntryPageColumn.lines(for: listEntry(title: title, isRead: isRead, isStarred: isStarred),
                                                          width: EntryPageColumn.cells, isSelected: selected)
                        #expect((1...2).contains(lines.count), "\(String(describing: title))")
                        #expect(lines.allSatisfy { $0.width == 28 }, "\(String(describing: title))")
                    }
                }
            }
        }
        for width in [0, 1, 3, 4, 10] {
            #expect(EntryPageColumn.lines(for: listEntry(title: "Café 🎉 東京"), width: width, isSelected: false)
                .allSatisfy { $0.width == width })
        }
    }

    @Test func markersColoursAndReverseVideo() {
        let title = "Apple announces something rather long today"
        let unread = EntryPageColumn.lines(for: listEntry(title: title, isStarred: true), width: 28, isSelected: false)
        #expect(unread.count == 2)
        #expect(text(of: unread[0].spans[0]) == "● " && unread[0].spans[0].foreground == .yellow)
        #expect(text(of: unread[0].spans[1]) == "Apple announces " + "        ")
        #expect(text(of: unread[0].spans[2]) == " *" && unread[0].spans[2].foreground == .white)
        #expect(text(of: unread[1].spans[0]) == "  " && text(of: unread[1].spans[2]) == "  ")  // markers on the first row only
        #expect(text(of: unread[1].spans[1]).hasPrefix("something rather long"))
        #expect(unread[0].accessibilityText == title)

        let read = EntryPageColumn.lines(for: listEntry(title: "Short", isRead: true), width: 28, isSelected: false)
        #expect(read.count == 1 && text(of: read[0].spans[0]) == "  " && text(of: read[0].spans[2]) == "  ")

        let selected = EntryPageColumn.lines(for: listEntry(title: title), width: 28, isSelected: true)
        #expect(selected.allSatisfy { $0.spans.allSatisfy { $0.foreground == .black && $0.background == .white } })

        #expect(EntryPageColumn.accessibilityLabel(for: listEntry(title: "Short", isStarred: true))
            == "Short, Daring Fireball, unread, starred")
        #expect(EntryPageColumn.accessibilityLabel(for: listEntry(title: nil, isRead: true)) == "Untitled, Daring Fireball")
    }

    @Test func titlesWrapOnWordsOverTwoRowsAtMost() {
        #expect(EntryPageColumn.wrap("Short title", width: 24) == ["Short title"])
        #expect(EntryPageColumn.wrap("  spaced   out  ", width: 24) == ["spaced out"])
        #expect(EntryPageColumn.wrap("", width: 24) == [""])
        #expect(EntryPageColumn.wrap("The quick brown fox jumps over the lazy dog", width: 24)
            == ["The quick brown fox", "jumps over the lazy dog"])
        #expect(EntryPageColumn.wrap("Supercalifragilisticexpialidocious and more", width: 24)
            == ["Supercalifragilisticexpi", "alidocious and more"])  // a word longer than a row is cut
        #expect(EntryPageColumn.wrap("Café 🎉 東京 ensemble", width: 10) == ["Café 🎉 東京", "ensemble"])  // graphemes, not bytes
        #expect(EntryPageColumn.wrap("anything", width: 0) == ["anything"])
    }
}

@MainActor @Suite struct ColumnTitleTests {
    static let columns: [AntiopeColumn] = [.index, .entries, .reader]

    @Test func everyTitleIsExactlyAsWideAsItsColumn() {
        for column in Self.columns {
            for active in [false, true] {
                for width in [0, 1, 5, 22, 28, 31, 120] {
                    #expect(ColumnTitle.line(for: column, isActive: active, width: width).width == width)
                }
            }
        }
        #expect(Self.columns.map(ColumnTitle.label(for:)) == ["INDEX", "ENTRIES", "READER"])
    }

    @Test func onlyTheActiveColumnIsInReverseVideo() {
        for column in Self.columns {
            let active = ColumnTitle.line(for: column, isActive: true, width: 22)
            #expect(active.spans.allSatisfy { $0.foreground == .black && $0.background == .cyan })
            let inactive = ColumnTitle.line(for: column, isActive: false, width: 22)
            #expect(inactive.spans.allSatisfy { $0.foreground == .cyan && $0.background == .black })
            #expect(text(of: inactive.spans[0]).hasPrefix(" " + ColumnTitle.label(for: column)))
            #expect(active.accessibilityText == ColumnTitle.label(for: column))
        }
    }

    @Test func blueNeverColoursATitle() {
        for column in Self.columns {
            for active in [false, true] {
                let line = ColumnTitle.line(for: column, isActive: active, width: 22)
                #expect(line.spans.allSatisfy { $0.foreground != .blue })
            }
        }
    }
}

@MainActor @Suite struct MinitelBarTests {
    @Test func theKeysAreTheMinitelsInReverseVideo() {
        #expect(MinitelKey.allCases.map(\.label) == ["SOMMAIRE", "SUITE", "RETOUR", "GUIDE", "RÉPÉTITION"])
        #expect(MinitelKey.allCases.map(\.command) == [.sommaire, .advance, .previousEntry, .guide, .repetition])
        #expect(MinitelKey.sommaire.accessibilityLabel == "Sommaire, go to page 100")
        for key in MinitelKey.allCases {
            #expect(key.line.width == key.label.count + 2)
            #expect(text(of: key.line.spans[0]) == " \(key.label) ")
            #expect(key.line.spans.allSatisfy { $0.foreground == .black && $0.background == .cyan })
            #expect(key.line.accessibilityText == key.accessibilityLabel)
        }
        #expect(MinitelBar.keysCells == 49)
    }

    @Test func hintsFollowTheSearchTheGardenAndTheFocus() {
        #expect(MinitelBar.hint(column: .entries, isGarden: true, searchText: "tidal")
            == MinitelHint(text: "SEARCH: tidal", colour: .cyan))
        #expect(MinitelBar.hint(column: .entries, isGarden: true, searchText: "")
            == MinitelHint(text: "k keep · r repair · b bury · u undo", colour: .white))
        #expect(MinitelBar.hint(column: .index, isGarden: false, searchText: "").text == "type a page number · ? guide")
        #expect(MinitelBar.hint(column: .entries, isGarden: false, searchText: "").text == "j next · k previous · m read")
        #expect(MinitelBar.hint(column: .reader, isGarden: false, searchText: "").text == "m read · s star · i images")
    }

    @Test func theHintLineFillsTheRestOfTheBar() {
        let hint = MinitelHint(text: "m read · s star · i images", colour: .white)
        for width in [0, 10, 26, 80] {
            #expect(MinitelBar.hintLine(hint, width: width).width == width)
        }
        #expect(text(of: MinitelBar.hintLine(hint, width: 30).spans[0]).hasPrefix(" m read"))
        #expect(MinitelBar.hintLine(hint, width: 30).accessibilityText == "m read · s star · i images")
    }
}

@MainActor @Suite struct PageHeaderTests {
    /// 2026-10-14 07:47:12 UTC, 09:47:12 in Paris.
    let date = Date(timeIntervalSince1970: 1_791_964_032)
    let paris = TimeZone(identifier: "Europe/Paris")!

    @Test func labelPrecedence() {
        var typing = PageEntry()
        _ = typing.type(4)
        #expect(PageHeader.label(entry: typing, notFound: 437, current: 412, rolling: 300) == .typing("4__"))
        #expect(PageHeader.label(entry: PageEntry(), notFound: 437, current: 412, rolling: 300) == .notFound(437))
        #expect(PageHeader.label(entry: PageEntry(), notFound: nil, current: 412, rolling: 300) == .searching(300))
        #expect(PageHeader.label(entry: PageEntry(), notFound: nil, current: 412, rolling: nil) == .page(412))
        #expect(PageHeader.label(entry: PageEntry(), notFound: nil, current: nil, rolling: nil) == .page(nil))
    }

    @Test func theHeaderFillsTheWidthWithPageNameDateAndTime() {
        let line = PageHeader.line(label: .page(412), date: date, width: 40, timeZone: paris)
        #expect(line.width == 40)
        #expect(line.spans.map(text(of:)) == ["P412 ", "RESSAC          ", "WED 14 OCT ", "09:47:12"])
        #expect(line.spans.map(\.foreground) == [.yellow, .white, .cyan, .yellow])
        for width in [0, 5, 8, 19, 24, 75, 160] {
            #expect(PageHeader.line(label: .notFound(437), date: date, width: width, timeZone: paris).width == width)
            #expect(PageHeader.line(label: .page(412), date: date, width: width, timeZone: paris).width == width)
        }
        #expect(text(of: PageHeader.line(label: .page(412), date: date, width: 8, timeZone: paris).spans[3]) == "09:47:12")
    }

    @Test func typingNotFoundAndNoPage() {
        let typing = PageHeader.line(label: .typing("41_"), date: date, width: 40, timeZone: paris)
        #expect(text(of: typing.spans[0]) == "P41_ " && typing.spans[0].foreground == .yellow)
        let notFound = PageHeader.line(label: .notFound(437), date: date, width: 40, timeZone: paris)
        #expect(text(of: notFound.spans[0]) == "P437 NOT FOUND " && notFound.spans[0].foreground == .red)
        #expect(text(of: notFound.spans[1]).allSatisfy { $0 == " " })  // no RESSAC beside NOT FOUND
        let none = PageHeader.line(label: .page(nil), date: date, width: 40, timeZone: paris)
        #expect(text(of: none.spans[0]) == "P--- ")
    }

    @Test func theCounterRollsThroughEveryPage() {
        let start = Date(timeIntervalSinceReferenceDate: 1_000_000.05)  // away from a tenth's edge
        let first = PageHeader.rollingPage(at: start)
        #expect(PageHeader.rollingPage(at: start.addingTimeInterval(0.1)) == (first - 100 + 1) % 800 + 100)
        for step in 0..<1_600 {
            #expect((100...899).contains(PageHeader.rollingPage(at: start.addingTimeInterval(Double(step) / 10))))
        }
        #expect((100...899).contains(PageHeader.rollingPage(at: Date(timeIntervalSinceReferenceDate: -12.34))))
    }
}

@MainActor @Suite struct AntiopeRootTests {
    /// A live switch back to Native must find the window as it was.
    @Test func theWindowIsDressedForAntiopeAndPutBack() {
        #expect(AntiopeRootView.minimumCells == 76)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
                              styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.contentView = NSView()
        let background = window.backgroundColor
        let chrome = AntiopeWindowChrome.ChromeView()
        window.contentView?.addSubview(chrome)
        #expect(window.titlebarAppearsTransparent && window.titleVisibility == .hidden)
        #expect(window.styleMask.contains(.fullSizeContentView))
        #expect(window.backgroundColor == .black)
        #expect(window.appearance?.name == .darkAqua)
        chrome.removeFromSuperview()
        #expect(!window.titlebarAppearsTransparent && window.titleVisibility == .visible)
        #expect(!window.styleMask.contains(.fullSizeContentView))
        #expect(window.backgroundColor == background && window.appearance == nil)
    }

    /// Spec §5: blue is for separators, backgrounds and mosaics, never for text.
    @Test func blueNeverColoursText() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        let date = Date(timeIntervalSince1970: 1_791_964_032)
        var lines: [CellLine] = []
        for row in directory.rows {
            for selected in [false, true] {
                for highlights in [false, true] {
                    lines.append(PageIndexColumn.line(for: row, width: 22, isRootFolder: PageIndexColumn.isRootFolder(row, in: directory),
                                                      isSelected: selected, highlightsReserved: highlights))
                }
            }
        }
        for isRead in [false, true] {
            for selected in [false, true] {
                lines += EntryPageColumn.lines(for: listEntry(title: "A title", isRead: isRead, isStarred: true),
                                               width: 28, isSelected: selected)
            }
        }
        for label in [HeaderLabel.page(412), .page(nil), .typing("4__"), .notFound(437), .searching(300)] {
            lines.append(PageHeader.line(label: label, date: date, width: 60))
        }
        lines += MinitelKey.allCases.map(\.line)
        for column in [AntiopeColumn.index, .entries, .reader] {
            for isGarden in [false, true] {
                for search in ["", "tidal"] {
                    lines.append(MinitelBar.hintLine(MinitelBar.hint(column: column, isGarden: isGarden, searchText: search),
                                                     width: 30))
                }
            }
        }
        #expect(lines.allSatisfy { $0.spans.allSatisfy { $0.foreground != .blue } })
    }
}
