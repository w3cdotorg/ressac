import Foundation
import GRDB
import Testing

@testable import Ressac

@Suite struct ThemePickerTests {
    @Test func theme_request_arguments() {
        #expect(RunRequest.setTheme(feedId: 7, key: "ai-labs").arguments == ["feed", "theme", "7", "ai-labs"])
        #expect(RunRequest.setTheme(feedId: 7, key: nil).arguments == ["feed", "theme", "7", "none"])
        #expect(RunRequest.setTheme(feedId: 7, key: nil).kind == .short)
        // An unknown key (a theme deleted meanwhile) exits 2: a sentence, not a banner with a log.
        let exitTwo = RunFailure(exitCode: 2, lastStderrLine: "No theme 'gone'.")
        #expect(RunRequest.setTheme(feedId: 7, key: "gone").isRefusal(exitTwo))
    }

    @Test func themes_query_by_position() async throws {
        let test = try TestDatabase()
        try test.execute("""
            INSERT INTO theme (id, key, name, sea_name, position) VALUES
              (1, 'zeta', 'Zeta', 'SEA OF ZETA', 2),
              (2, 'alpha', 'Alpha', 'SEA OF ALPHA', 0),
              (3, 'mid', 'Mid', 'SEA OF MID', 1)
            """)
        let themes = try await value(of: Queries.themes(), in: test.open())
        #expect(themes.map(\.key) == ["alpha", "mid", "zeta"])
        #expect(themes.first == ThemeRow(id: 2, key: "alpha", name: "Alpha", seaName: "SEA OF ALPHA", position: 0))
    }

    @Test func feed_theme_key_query() async throws {
        let test = try TestDatabase()
        try test.execute("""
            INSERT INTO theme (id, key, name, sea_name, position) VALUES (1, 'ai', 'AI', 'SEA OF AI', 0), (2, 'web', 'Web', 'SEA OF WEB', 1);
            UPDATE feed SET theme_id = 2 WHERE id = 3
            """)
        let db = try test.open()
        let keys = try await value(of: Queries.feedThemeKeys(), in: db)
        // Unthemed feeds are absent.
        #expect(keys == [3: "web"])
        try test.execute("UPDATE feed SET theme_id = 1 WHERE id = 3; UPDATE feed SET theme_id = 2 WHERE id = 4")
        try await db.notifyExternalChanges()
        #expect(try await value(of: Queries.feedThemeKeys(), in: db) { $0.count == 2 } == [3: "ai", 4: "web"])
    }
}
