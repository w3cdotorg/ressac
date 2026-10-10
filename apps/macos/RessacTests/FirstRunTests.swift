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


@Suite struct DatabaseLocationTests {
    @Test func environmentThenSettingThenTheSharedDefault() throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let shared = URL.homeDirectory.appending(path: "Library/Application Support/Ressac/ressac.sqlite")
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("  ", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == shared)
        defaults.set("~/Feeds/other.sqlite", forKey: DatabaseLocation.defaultsKey)
        #expect(DatabaseLocation.current(environment: [:], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": "/tmp/copy.sqlite"], defaults: defaults) == URL(filePath: "/tmp/copy.sqlite"))
        #expect(DatabaseLocation.current(environment: ["RESSAC_DB": ""], defaults: defaults) == URL.homeDirectory.appending(path: "Feeds/other.sqlite"))
    }
}
