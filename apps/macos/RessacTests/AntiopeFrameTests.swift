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
