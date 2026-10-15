import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Ressac

/// Bedstead's design: one advance of 600/1000 em, ascent 800, descent 200, no line gap.
@Suite struct BedsteadBundleTests {
    @Test func bedsteadShipsWithItsDedication() throws {
        let font = try #require(NSFont(name: "Bedstead", size: 20), "ATSApplicationFontsPath did not register Bedstead")
        #expect(font.familyName == "Bedstead")
        #expect(font.isFixedPitch)
        #expect(Bundle.main.url(forResource: "bedstead", withExtension: "otf", subdirectory: "Fonts") != nil)
        #expect(Bundle.main.url(forResource: "Bedstead-CC0", withExtension: "txt", subdirectory: "Fonts") != nil)
    }

    @Test(arguments: [15.0, 20.0, 25.0, 30.0])
    func everyGlyphTakesSixTenthsOfAnEm(size: Double) throws {
        let font = try #require(NSFont(name: "Bedstead", size: size))
        let row = "MMMMMiiiii«é€▐🬀"  // 15 characters, mosaics included
        // As Double: inside #expect, comparing a CGFloat with this Double failed (see Notes).
        let width = Double((row as NSString).size(withAttributes: [.font: font]).width)
        #expect(width == size * 6 / 10 * 15)
        #expect(Double(NSLayoutManager().defaultLineHeight(for: font)) == size)
        #expect(abs(font.ascender - size * 0.8) < 0.01)
        #expect(abs(font.descender + size * 0.2) < 0.01)
        #expect(font.leading == 0)
    }
}

@Suite struct TeletextColorTests {
    @Test func theEightColoursArePure() {
        #expect(TeletextColor.allCases == [.black, .red, .green, .yellow, .blue, .magenta, .cyan, .white])
        #expect(TeletextColor.allCases.map(\.css) == [
            "#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF",
        ])
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            #expect([r, g, b].allSatisfy { $0 == 0 || $0 == 0xFF }, "\(colour)")
        }
        #expect(TeletextColor(rawValue: "magenta") == .magenta)
    }

    @Test func swiftUIAndAppKitColoursMatchTheTriplet() throws {
        for colour in TeletextColor.allCases {
            let (r, g, b) = colour.rgb
            let expected = [Double(r), Double(g), Double(b)].map { $0 / 255 }
            let ns = try #require(colour.nsColor.usingColorSpace(.sRGB))
            #expect([ns.redComponent, ns.greenComponent, ns.blueComponent].map(Double.init) == expected, "\(colour)")
            #expect(ns.alphaComponent == 1)
            let resolved = colour.color.resolve(in: EnvironmentValues())
            #expect([resolved.red, resolved.green, resolved.blue].map(Double.init) == expected, "\(colour)")
            #expect(resolved.opacity == 1)
        }
    }
}

@Suite struct TeletextFontTests {
    @Test func bedsteadIsAvailableInTheApp() {
        #expect(TeletextFont.isAvailable)
        #expect(TeletextFont.familyName == "Bedstead")
        #expect(Bundle.main.url(forResource: TeletextFont.fileName, withExtension: "otf", subdirectory: "Fonts") != nil)
        #expect(TeletextFont.font(size: 20) == Font.custom("Bedstead", fixedSize: 20))
    }

    @Test func cellsAndPixelsFollowTheSize() {
        #expect(TeletextFont.sizeSteps.map(TeletextFont.cell(forSize:)) == [
            CGSize(width: 9, height: 15), CGSize(width: 12, height: 20),
            CGSize(width: 15, height: 25), CGSize(width: 18, height: 30),
        ])
        #expect(TeletextFont.sizeSteps.map(TeletextFont.pixel(forSize:)) == [1.5, 2, 2.5, 3])
    }

    @Test func sizesSnapToTheNearestStepAndTiesGoUp() {
        let cases: [(Double, Double)] = [
            (12, 15), (15, 15), (17, 15), (17.5, 20), (18, 20), (20, 20), (22, 20), (22.5, 25),
            (23, 25), (27, 25), (27.5, 30), (28, 30), (40, 30), (-3, 15), (.nan, 20), (.infinity, 20),
        ]
        for (size, expected) in cases {
            #expect(TeletextFont.snapped(size) == expected, "\(size)")
        }
        #expect(TeletextFont.defaultSize == 20)
        #expect(EnvironmentValues().teletextSize == TeletextFont.defaultSize)
        // Antiope keeps its own size, never the Native `reader.size` (decision of 2026-10-14).
        #expect(ReaderStyle.Keys.antiopeSize == "reader.size.antiope" && ReaderStyle.Keys.size == "reader.size")
    }

    @Test func stepsMoveFromTheSnappedSizeAndStopAtTheEnds() {
        #expect(TeletextFont.step(17, by: 1) == 20)  // a stored 17 draws at 15, so ⌘+ gives 20
        #expect(TeletextFont.step(18, by: 1) == 25)  // 18 draws at 20
        #expect(TeletextFont.step(20, by: -1) == 15)
        #expect(TeletextFont.step(30, by: 1) == 30)
        #expect(TeletextFont.step(15, by: -1) == 15)
        #expect(TeletextFont.step(15, by: 7) == 30)
        #expect(TeletextFont.step(25, by: 0) == 25)
    }

    /// The French repertoire and the glyphs Antiope draws (spec §2, Font), and what Bedstead lacks.
    @Test func supportsReadsBedsteadsCharacterSet() {
        for character in "éèêëàâîïôùûüÿçœæÉÈÊËÀÂÎÏÔÙÛÜŸÇŒÆ«»’“”…–—€°▐█░●*?" {
            #expect(TeletextFont.supports(character), "\(character)")
        }
        #expect(TeletextFont.supports("\u{1FB00}") && TeletextFont.supports("\u{1FB3B}"))  // sextant mosaics
        #expect(TeletextFont.supports("e\u{301}"))  // composed to é first
        for character: Character in ["🎉", "東", "❤️", "🇫🇷", "q\u{301}", "\u{301}"] {
            #expect(!TeletextFont.supports(character), "\(character)")
        }
    }
}

@Suite struct CellTextTests {
    func cellCount(_ fitted: FittedCells) -> Int {
        fitted.runs.reduce(0) { $0 + $1.text.count }
    }

    /// Review Focus #1: emoji, CJK and a combining accent each take exactly one cell.
    @Test func emojiCJKAndCombiningAccentsTakeOneCellEach() {
        let fitted = CellText.fit("Café 🎉 東京", cells: 12)
        #expect(fitted.runs == [
            CellRun(text: "Café ", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
            CellRun(text: " ", isReplacement: false),
            CellRun(text: "??", isReplacement: true),
            CellRun(text: "   ", isReplacement: false),
        ])
        #expect(cellCount(fitted) == 12)
        #expect(fitted.runs.reduce(0) { $0 + $1.text.unicodeScalars.count } == 12)  // one scalar per cell
        #expect(fitted.accessibilityText == "Café 🎉 東京")

        let decomposed = CellText.fit("Cafe\u{301} noir", cells: 6)
        #expect(decomposed.runs == [CellRun(text: "Café n", isReplacement: false)])
        #expect(decomposed.runs[0].text.unicodeScalars.map(\.value) == [0x43, 0x61, 0x66, 0xE9, 0x20, 0x6E])
        #expect(decomposed.accessibilityText == "Cafe\u{301} noir")
    }

    @Test func longTextIsCutWithoutEllipsisAndShortTextIsPadded() {
        #expect(CellText.fit("Courrier international", cells: 8).runs == [CellRun(text: "Courrier", isReplacement: false)])
        #expect(CellText.fit("Unread", cells: 9).runs == [CellRun(text: "Unread   ", isReplacement: false)])
        #expect(CellText.fit("", cells: 3).runs == [CellRun(text: "   ", isReplacement: false)])
        #expect(CellText.fit("xkcd", cells: 0).runs.isEmpty)
        #expect(CellText.fit("xkcd", cells: -2).runs.isEmpty)
        #expect(CellText.fit("xkcd", cells: 0).accessibilityText == "xkcd")
    }

    @Test func clustersWhitespaceAndInvisiblesNeverMoveTheGrid() {
        let family = CellText.fit("👩‍👩‍👧🇫🇷x", cells: 4)  // a ZWJ sequence and a flag: one cell each
        #expect(family.runs == [CellRun(text: "??", isReplacement: true), CellRun(text: "x ", isReplacement: false)])
        let lines = CellText.fit("Line\nbreak\ttab\r\nend", cells: 20)
        #expect(lines.runs == [CellRun(text: "Line break tab end  ", isReplacement: false)])
        let invisible = CellText.fit("a\u{200B}b\u{FEFF}c\u{00AD}d", cells: 5)
        #expect(invisible.runs == [CellRun(text: "abcd ", isReplacement: false)])
        for fitted in [family, lines, invisible] {
            #expect(fitted.runs.reduce(0) { $0 + $1.text.unicodeScalars.count } == cellCount(fitted))
        }
    }

    @Test func theCoverageTestIsInjectable() {
        let fitted = CellText.fit("abcb", cells: 4, supports: { $0 != "b" })
        #expect(fitted.runs == [
            CellRun(text: "a", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
            CellRun(text: "c", isReplacement: false),
            CellRun(text: "?", isReplacement: true),
        ])
    }
}
