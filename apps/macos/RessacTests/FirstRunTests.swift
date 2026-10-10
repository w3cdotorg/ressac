import AppKit
import Foundation
import Testing
@testable import Ressac

@Suite struct BundledFontTests {
    @Test func bundledFontsShipWithTheirLicences() {
        #expect(NSFont(name: "Newsreader", size: 12) != nil, "ATSApplicationFontsPath did not register Newsreader")
        #expect(NSFont(name: "Atkinson Hyperlegible Next", size: 12) != nil, "ATSApplicationFontsPath did not register Atkinson")
        for licence in ["Newsreader-OFL", "AtkinsonHyperlegibleNext-OFL"] {
            #expect(Bundle.main.url(forResource: licence, withExtension: "txt", subdirectory: "Fonts") != nil, "missing \(licence).txt")
        }
        let newsreader = ArticleTemplate.fontFaces(for: .newsreader)
        #expect(newsreader.components(separatedBy: "@font-face").count == 3)
        #expect(newsreader.contains("font-style: italic"))
        #expect(newsreader.contains("data:font/ttf;base64,"))
        #expect(ArticleTemplate.fontFaces(for: .atkinson).contains(#"font-family: "Atkinson Hyperlegible Next""#))
        #expect(ArticleTemplate.fontFaces(for: .charter).isEmpty)
    }
}

