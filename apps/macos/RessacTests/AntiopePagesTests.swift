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

@MainActor @Suite struct GuidePageTests {
    @Test func everyLineFillsTheWidth() {
        for width in [0, 5, 12, 30, 60] {
            let lines = GuidePage.lines(width: width)
            #expect(lines.count == GuidePage.sections.reduce(0) { $0 + 2 + $1.keys.count })
            #expect(lines.allSatisfy { $0.width == width })
        }
    }

    @Test func everyKeyOfTheWindowIsListed() {
        let keys = Set(GuidePage.sections.flatMap { $0.keys.map(\.keys) })
        for key in ["0-9", "DEL", "ESC", "OPT S", "OPT G  ?", "OPT R", "I", "SPACE  J", "K", "M", "S",
                    "LEFT RIGHT", "UP DOWN", "R", "B", "U", "O", "W", "SHIFT R", "CMD F", "CMD R"] {
            #expect(keys.contains(key), "\(key)")
        }
        #expect(GuidePage.sections.map(\.title) == ["PAGES", "READING", "COLUMNS", "GARDEN", "MENUS"])
        #expect(GuidePage.sections.flatMap(\.keys).allSatisfy { $0.keys.count + 1 <= GuidePage.keyCells })
    }

    @Test func headingsKeysAndActionsAreColoured() {
        let lines = GuidePage.lines(width: 40)
        #expect(lines[0] == .blank(40))
        #expect(text(of: lines[1].spans[0]).hasPrefix(" PAGES"))
        #expect(lines[1].spans[0].foreground == .black && lines[1].spans[0].background == .green)
        #expect(text(of: lines[2].spans[0]) == " 0-9        " && lines[2].spans[0].foreground == .cyan)
        #expect(lines[2].spans[1].foreground == .white)
        #expect(lines[2].accessibilityText == "0-9: type a page number")
        #expect(lines.allSatisfy { $0.spans.allSatisfy { $0.foreground != .blue } })
    }

    /// Bedstead draws every character of P100 and P899: no magenta `?` on a help page.
    @Test func thePagesDrawWithoutAMissingGlyph() throws {
        try #require(TeletextFont.isAvailable)
        let lines = GuidePage.lines(width: 60) + SommairePage.logo(width: 16) + [SommairePage.titleLine(width: 20)]
        #expect(lines.allSatisfy { $0.spans.allSatisfy { $0.cells.runs.allSatisfy { !$0.isReplacement } } })
    }
}

@MainActor @Suite struct SearchPromptTests {
    @Test func thePromptAsksAsATerminalWould() {
        #expect(SearchPrompt.label == "SEARCH? ")
    }

    /// Review Focus #5: switching Appearance with the prompt open closes it and keeps the search.
    @Test func switchingLooksClosesThePromptAndKeepsTheSearch() async throws {
        let (state, test, _) = try makeState()
        let match = try test.entry(feed: 1, title: "Tidal bore", body: "ressac")
        try test.entry(feed: 1, title: "Other", body: "calm")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        let antiope = AntiopeState(app: state)
        state.isSearchFocused = true  // ⌘F
        #expect(antiope.isSearchPromptOpen && antiope.keyFocus(in: nil) == .textInput)
        state.searchText = "tidal"
        #expect(state.entries.map(\.entryId) == [match])
        antiope.closeSearchPrompt(clearing: false)  // AntiopeRootView.onDisappear
        #expect(!state.isSearchFocused && state.letterShortcutsEnabled)  // Native's letters work at once
        #expect(state.searchText == "tidal" && state.entries.map(\.entryId) == [match])
        let back = AntiopeState(app: state)  // the look comes back: a fresh state, the prompt shut
        #expect(!back.isSearchPromptOpen && back.keyFocus(in: nil) == .sidebar)
        #expect(MinitelBar.hint(column: back.column, isGarden: false, searchText: state.searchText).text == "SEARCH: tidal")
    }

    @Test func escapeClearsTheSearchAndTheListComesBack() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 1, title: "Tidal bore", body: "ressac")
        try test.entry(feed: 1, title: "Other", body: "calm")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        let antiope = AntiopeState(app: state)
        antiope.isSearchPromptOpen = true
        state.searchText = "zzz"
        #expect(state.entries.isEmpty)
        antiope.closeSearchPrompt(clearing: true)
        #expect(state.searchText.isEmpty && !antiope.isSearchPromptOpen)
        #expect(state.entries.count == 2)
    }
}
