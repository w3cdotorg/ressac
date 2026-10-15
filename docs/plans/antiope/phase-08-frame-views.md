# Phase 8: the Antiope frame (root view, header, columns, Minitel bar)

**Runs after:** phase 7b.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`): Global Constraints (Teletext palette, Font geometry, Columns, Keys), Review Focus #1 (this phase owns its second half), and "Interfaces between phases" (Phases 3, 4, 7, 8). Spec §5 (Window, Focus and keys), §6 (Page header, Minitel bar), §8 (Accessibility; Banners belong to phase 12).

**Files (5):**
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (phase 4's stub, replaced whole: the root view, `PageHeader`, `ColumnSeparator`, `AntiopeWindowChrome`)
- Create: `apps/macos/Ressac/Antiope/CellLine.swift`
- Create: `apps/macos/Ressac/Antiope/PageColumns.swift` (`PageIndexColumn`, `EntryPageColumn`)
- Create: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`MinitelKey`, `MinitelBar`)
- Create: `apps/macos/RessacTests/AntiopeFrameTests.swift`

**Prerequisites** (by their pinned names):
- Phase 3: `TeletextColor` (`.color`), `TeletextFont` (`font(size:)`, `cell(forSize:)`, `snapped(_:)`, `defaultSize`, `isAvailable`), `\.teletextSize`, `CellText.fit`, `FittedCells`, `CellRun`, `ReaderStyle.Keys.antiopeSize` (Antiope's own stored text size, decision of 2026-10-14).
- Phase 4: `AntiopeRootView(state:)` is what `LaunchView` shows for Antiope; this phase replaces the stub's body and keeps that initialiser.
- Phase 7: `AntiopeState` (`column`, `entry`, `notFound`, `readerPage`, `directory`, `currentPage`, `jump(to:)`, `handle(_:)`, `closeSearchPrompt(clearing:)`), `PageDirectory`, `ReservedPage`, `ReadingKeyRouter(state:focus:antiope:)`, `ReadingWindowReader(makeRouter:)`.

**Test command** (narrow it with one `-only-testing:` per suite):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/CellLineTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**UI is not snapshot-tested.** The test suites are `@MainActor`: the row-model functions are static members of `View` types, which Swift 6 isolates to the main actor. Every row any view draws comes from a pure function returning a `CellLine` (`PageIndexColumn.line`, `EntryPageColumn.lines`, `PageHeader.line`, `MinitelKey.line`, `MinitelBar.hintLine`); the tests pin those models: exact widths (Review Focus #1), colours, reverse video, and that blue never colours text. The views only lay the models out. The tests that read a row's text assume Bedstead is registered in the test host, as phase 3's `CellText` tests do; widths hold either way.

**What later phases plug into** (keep these names; they are new public names for the index table):
- `CellSpan`, `CellLine` (`init(_:accessibilityText:)`, `width`, `blank(_:)`, `reversed(_:on:)`) and `CellLineView(line:)`: every Antiope text is a `Text` inside a `CellLineView`, which phase 13's root-level `TextRenderer` reaches. Phase 12's garden rows are `CellLine`s too (exact width, reverse video, replacements and accessibility are handled here), with no second span model.
- `PageIndexColumn.line(for:width:isRootFolder:isSelected:highlightsReserved:)` and `PageIndexColumn.isRootFolder(_:in:)`: phase 9's P100 reuses them.
- `EntryPageColumn.cells` (28), `PageIndexColumn.cells` (22), `EntryPageColumn.lines(for:width:isSelected:)`, `EntryPageColumn.wrap(_:width:)`.
- `PageHeader.line(label:date:width:timeZone:)`, `HeaderLabel`, `PageHeader.rollingPage(at:)`.
- `MinitelKey`, `MinitelHint`, `MinitelBar.hint(column:isGarden:searchText:)`, `MinitelBar.hintLine(_:width:)`, `MinitelBar.keysCells`, in `Antiope/MinitelBar.swift`. Phase 9 puts the search prompt where the hint line is (it keeps priority); phase 12 adds `MinitelBar.status(progress:message:)` (`REFRESHING 42/155` during runs, the garden's refusals), drawn through `hintLine` in place of the hints.
- `AntiopeRootView`: the private `entriesColumn` and `readerColumn` view builders are where phase 9 (P100, P899) and phase 12 (P800–P802, the inspector page) swap content in; `readerColumn` is a `switch antiope.readerPage` whose `case .article:` branch holds the Garden test. Until phase 12 the Garden keeps its native `GardenTable` and `GardenInspector` there, and the reader column hosts the existing `ReaderView` until phase 10 themes it. The banner row goes between `PageHeader` and the columns (a comment marks the place; phase 12 owns banners). `AntiopeWindowChrome` dresses the window and puts it back. `PageHeader` and `HeaderLabel` live in `AntiopeRootView.swift`.

**Layout decisions** (spec §5, Window):
- The window has no title, a transparent title bar, full-size content, a black background and a dark appearance. The traffic lights sit in the black strip above the header: the header and everything under it stay inside the safe area, so no text ever hides under them, and the black reaches the top.
- Row height is one cell (`TeletextFont.cell(forSize:).height`); every span is framed to exactly its cells. The header and the Minitel bar span `floor(window width / cell width)` cells; the leftover fraction of a cell stays black at the right.
- `ColumnSeparator` is a shape filling the right half of a one-cell-wide strip in blue, the full height of the columns: Bedstead's `▐` is exactly that block on every row, and a shape cannot be bent by a glyph.
- The window's minimum width keeps 24 cells for the reader (22 + 1 + 28 + 1 + 24 = 76 cells: 912 pt at 20 points, 1368 pt at 30).
- Selection: black on cyan in the index, black on white in the entries; the Minitel keys black on cyan.

---

### Task 8.1: `CellSpan`, `CellLine` and `CellLineView`

**Files:**
- Create: `apps/macos/Ressac/Antiope/CellLine.swift`
- Test: `apps/macos/RessacTests/AntiopeFrameTests.swift`

**Interfaces:**
- Consumes: `CellText.fit(_:cells:supports:)`, `FittedCells`, `CellRun`, `TeletextColor`, `TeletextFont`, `\.teletextSize`.
- Produces: `struct CellSpan: Equatable, Sendable` (`init(_ text: String, cells: Int, foreground: TeletextColor, background: TeletextColor = .black)`, `cells`, `foreground`, `background`, `cellCount`, `attributedText`); `struct CellLine: Equatable, Sendable` (`init(_ spans: [CellSpan], accessibilityText: String? = nil)`, `spans`, `accessibilityText`, `width`, `static func blank(_:)`, `func reversed(_:on:)`); `struct CellLineView: View` (`init(line:)`).

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/AntiopeFrameTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/CellLineTests`. Expected: `error: cannot find 'CellLine' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/CellLine.swift`:

```swift
import SwiftUI

/// One coloured stretch of a cell row: its text already fitted to a number of cells.
struct CellSpan: Equatable, Sendable {
    var cells: FittedCells
    var foreground: TeletextColor
    var background: TeletextColor

    /// Fits `text` into exactly `cells` cells (`CellText.fit`: truncated by grapheme, padded with
    /// spaces, missing glyphs as `?`); a negative count is taken as 0.
    init(_ text: String, cells: Int, foreground: TeletextColor, background: TeletextColor = .black) {
        self.cells = CellText.fit(text, cells: max(0, cells))
        self.foreground = foreground
        self.background = background
    }

    /// The cells this span takes: one per character of its runs.
    var cellCount: Int { cells.runs.reduce(0) { $0 + $1.text.count } }

    /// The span's text in its colour; a replaced grapheme stays a magenta `?` (spec §5, `CellText`).
    var attributedText: AttributedString {
        cells.runs.reduce(into: AttributedString()) { text, run in
            var part = AttributedString(run.text)
            part.foregroundColor = (run.isReplacement ? TeletextColor.magenta : foreground).color
            text += part
        }
    }
}

/// A row of the teletext grid (spec §5, Columns): spans side by side, `width` cells in all. Every
/// text an Antiope view draws is one, so no glyph can push a column or a separator.
struct CellLine: Equatable, Sendable {
    var spans: [CellSpan]
    /// What VoiceOver reads: the original strings, never the fitted cells (spec §8, Accessibility).
    var accessibilityText: String

    /// Without `accessibilityText`, VoiceOver reads the spans' original strings, trimmed and
    /// joined by spaces.
    init(_ spans: [CellSpan], accessibilityText: String? = nil) {
        self.spans = spans
        self.accessibilityText = accessibilityText ?? spans
            .map { $0.cells.accessibilityText.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
    }

    var width: Int { spans.reduce(0) { $0 + $1.cellCount } }

    /// `width` black cells, with nothing to read.
    static func blank(_ width: Int) -> CellLine {
        CellLine([CellSpan("", cells: width, foreground: .white)], accessibilityText: "")
    }

    /// Reverse video, as teletext shows a selection or a key: every span in `foreground` on `background`.
    func reversed(_ foreground: TeletextColor, on background: TeletextColor) -> CellLine {
        CellLine(spans.map { span in
            var span = span
            span.foreground = foreground
            span.background = background
            return span
        }, accessibilityText: accessibilityText)
    }
}

/// Draws a `CellLine`, each span in a frame of exactly its cells, in Bedstead at `\.teletextSize`.
struct CellLineView: View {
    let line: CellLine
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        HStack(spacing: 0) {
            ForEach(Array(line.spans.enumerated()), id: \.offset) { _, span in
                Text(span.attributedText)
                    .font(TeletextFont.font(size: size))
                    .lineLimit(1)
                    .fixedSize()
                    .frame(width: CGFloat(span.cellCount) * cell.width, height: cell.height, alignment: .leading)
                    .background(span.background.color)
            }
        }
        .frame(width: CGFloat(line.width) * cell.width, height: cell.height, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(line.accessibilityText)
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/CellLineTests`. Expected: `✔ Test run with 4 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/CellLine.swift apps/macos/RessacTests/AntiopeFrameTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): cell rows of exact width, drawn span by span

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 8.2: the index and entry columns

**Files:**
- Create: `apps/macos/Ressac/Antiope/PageColumns.swift`
- Test: `apps/macos/RessacTests/AntiopeFrameTests.swift` (append two suites)

**Interfaces:**
- Consumes: `CellSpan`, `CellLine`, `CellLineView` (8.1); `AntiopeState` (`directory`, `currentPage`, `jump(to:)`, `column`, `readerPage`), `PageDirectory.Row`, `ReservedPage.isGarden` (phase 7); `AppState` (`entries`, `selectedEntryId`); `EntryRow` (`displayTitle`, `feedTitle`, `isRead`, `isStarred`).
- Produces:
  - `struct PageIndexColumn: View` with `static let cells = 22`, `static func line(for: PageDirectory.Row, width: Int, isRootFolder: Bool, isSelected: Bool, highlightsReserved: Bool = false) -> CellLine`, `static func isRootFolder(_: PageDirectory.Row, in: PageDirectory) -> Bool`;
  - `struct EntryPageColumn: View` with `static let cells = 28`, `static func lines(for: EntryRow, width: Int, isSelected: Bool) -> [CellLine]`, `static func wrap(_ title: String, width: Int) -> [String]`, `static func accessibilityLabel(for: EntryRow) -> String`.

**Rules (spec §5, Window):**
- Index row `101 Unread         1083`: the number and a space in cyan (4 cells), the title indented one cell per level (three at most), the count right-aligned in yellow after a space, nothing when it is 0, capped at 99999. Root folders in green capitals, the Garden's pages in magenta with P800's triage count in red, other titles white; P100 (phase 9) passes `highlightsReserved` to colour the other reserved titles yellow. Selected: black on cyan.
- Entry rows: a yellow `●` and a space when unread, the title on 24 cells wrapped over two rows at most, a space and a white `*` when starred; the second row has no marker. Selected: black on white. A click selects the entry, focuses the entry column and brings the article back into the reader.
- Both columns are `ScrollView` + `LazyVStack` of cell rows (no `List`), scroll to the selection when it changes, and hide their scroll indicators.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeFrameTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/IndexColumnTests -only-testing:RessacTests/EntryColumnTests`. Expected: `error: cannot find 'PageIndexColumn' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/PageColumns.swift`:

```swift
import SwiftUI

/// The index column (spec §5, Window): every page, 22 cells wide. A click goes to the page.
struct PageIndexColumn: View {
    static let cells = 22

    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        let directory = antiope.directory
        let current = antiope.currentPage
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(directory.rows, id: \.number) { row in
                        let selected = row.number == current
                        let line = Self.line(for: row, width: Self.cells,
                                             isRootFolder: Self.isRootFolder(row, in: directory), isSelected: selected)
                        Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
                            .buttonStyle(.plain)
                            .accessibilityLabel(line.accessibilityText)
                            .accessibilityAddTraits(selected ? .isSelected : [])
                            .id(row.number)
                    }
                }
            }
            .scrollIndicators(.hidden)
            .onChange(of: current) { _, page in
                if let page { proxy.scrollTo(page) }
            }
        }
    }

    /// `101 Unread         1083`: the number in cyan, the title indented by depth, the count in
    /// yellow at the right edge. Root folders in green capitals, the Garden in magenta with P800's
    /// triage count in red; P100 colours the other reserved titles yellow (`highlightsReserved`).
    /// Selected: black on cyan. Always exactly `width` cells.
    static func line(for row: PageDirectory.Row, width: Int, isRootFolder: Bool, isSelected: Bool,
                     highlightsReserved: Bool = false) -> CellLine {
        let reserved = row.isReserved ? ReservedPage(rawValue: row.number) : nil
        let count = row.unread > 0 ? String(min(row.unread, 99_999)) : ""
        let numberCells = min(4, max(0, width))
        let countCells = min(count.isEmpty ? 0 : count.count + 1, max(0, width - numberCells))
        let titleCells = max(0, width - numberCells - countCells)
        let indent = String(repeating: " ", count: min(row.depth, 3))
        let title = isRootFolder ? row.title.uppercased() : row.title
        let titleColour: TeletextColor =
            if reserved?.isGarden == true { .magenta }
            else if reserved != nil, highlightsReserved { .yellow }
            else if isRootFolder { .green }
            else { .white }
        let line = CellLine([
            CellSpan("\(row.number) ", cells: numberCells, foreground: .cyan),
            CellSpan(indent + title, cells: titleCells, foreground: titleColour),
            CellSpan(count.isEmpty ? "" : " " + count, cells: countCells, foreground: reserved == .needsTriage ? .red : .yellow),
        ], accessibilityText: "\(row.title), page \(row.number)" + (row.unread > 0 ? ", \(row.unread)" : ""))
        return isSelected ? line.reversed(.black, on: .cyan) : line
    }

    /// A folder at the top of the tree (green capitals in the index).
    static func isRootFolder(_ row: PageDirectory.Row, in directory: PageDirectory) -> Bool {
        guard row.depth == 0, case .source(.folder(_))? = directory.destination(for: row.number) else { return false }
        return true
    }
}

/// The entry column (spec §5, Window): the entries of the selected page, 28 cells wide.
struct EntryPageColumn: View {
    static let cells = 28

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(app.entries) { entry in
                        let selected = entry.entryId == app.selectedEntryId
                        Button { open(entry) } label: {
                            VStack(alignment: .leading, spacing: 0) {
                                ForEach(Array(Self.lines(for: entry, width: Self.cells, isSelected: selected).enumerated()),
                                        id: \.offset) { _, line in
                                    CellLineView(line: line)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(Self.accessibilityLabel(for: entry))
                        .accessibilityAddTraits(selected ? .isSelected : [])
                        .id(entry.entryId)
                    }
                }
            }
            .scrollIndicators(.hidden)
            // `j`, `k` and Space select from the keyboard: the column follows the selection.
            .onChange(of: app.selectedEntryId) { _, id in
                if let id { proxy.scrollTo(id) }
            }
        }
    }

    private func open(_ entry: EntryRow) {
        app.selectedEntryId = entry.entryId
        antiope.column = .entries
        antiope.readerPage = .article
    }

    /// One or two rows: a yellow `●` when unread, the title wrapped over two rows at most, a white
    /// `*` when starred, the markers on the first row only. Selected: black on white. Every row is
    /// exactly `width` cells.
    static func lines(for entry: EntryRow, width: Int, isSelected: Bool) -> [CellLine] {
        let markerCells = min(2, max(0, width))
        let titleCells = max(0, width - 4)
        let starCells = max(0, width - markerCells - titleCells)
        let lines = wrap(entry.displayTitle, width: titleCells).enumerated().map { index, text in
            CellLine([
                CellSpan(index == 0 && !entry.isRead ? "●" : "", cells: markerCells, foreground: .yellow),
                CellSpan(text, cells: titleCells, foreground: .white),
                CellSpan(index == 0 && entry.isStarred ? " *" : "", cells: starCells, foreground: .white),
            ], accessibilityText: entry.displayTitle)
        }
        return isSelected ? lines.map { $0.reversed(.black, on: .white) } : lines
    }

    /// The title over two rows at most, words separated by single spaces: the first row ends at
    /// the last word that fits (a first word longer than the row is cut), the second takes the
    /// rest, which `CellText.fit` truncates.
    static func wrap(_ title: String, width: Int) -> [String] {
        let words = title.split(whereSeparator: \.isWhitespace).map(String.init)
        let text = words.joined(separator: " ")
        guard width > 0, text.count > width else { return [text] }
        var first = ""
        var used = 0
        for word in words {
            let candidate = first.isEmpty ? word : first + " " + word
            guard candidate.count <= width else { break }
            first = candidate
            used += 1
        }
        guard !first.isEmpty else { return [String(text.prefix(width)), String(text.dropFirst(width))] }
        return [first, words[used...].joined(separator: " ")]
    }

    /// "Title, Feed, unread, starred": what VoiceOver reads for an entry.
    static func accessibilityLabel(for entry: EntryRow) -> String {
        var parts = [entry.displayTitle, entry.feedTitle]
        if !entry.isRead { parts.append("unread") }
        if entry.isStarred { parts.append("starred") }
        return parts.joined(separator: ", ")
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/IndexColumnTests -only-testing:RessacTests/EntryColumnTests`. Expected: `✔ Test run with 6 tests in 2 suites passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/PageColumns.swift apps/macos/RessacTests/AntiopeFrameTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): index and entry columns as fixed-width cell rows

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 8.3: the Minitel bar

**Files:**
- Create: `apps/macos/Ressac/Antiope/MinitelBar.swift`
- Test: `apps/macos/RessacTests/AntiopeFrameTests.swift` (append a suite)

**Interfaces:**
- Consumes: `CellSpan`, `CellLine`, `CellLineView` (8.1); `AntiopeState` (`column`, `handle(_:)`), `AntiopeColumn`, `ReadingCommand` (phase 7); `AppState` (`selection.isGarden`, `searchText`, `perform(_:isRepeat:)`).
- Produces: `enum MinitelKey: CaseIterable, Sendable` (`label`, `accessibilityLabel`, `command`, `line`); `struct MinitelHint: Equatable, Sendable { var text: String; var colour: TeletextColor }`; `struct MinitelBar: View` (`init(width:)`), `static let keysCells` (49), `static func hint(column:isGarden:searchText:) -> MinitelHint`, `static func hintLine(_:width:) -> CellLine`.

**Rules (spec §6, Minitel bar):**
- Keys, left to right: ` SOMMAIRE `, ` SUITE `, ` RETOUR `, ` GUIDE `, ` RÉPÉTITION `, black on cyan, one black cell after each. A click sends the same command as the key: `.sommaire`, `.advance` (Space), `.previousEntry` (`k`), `.guide`, `.repetition`, first to `AntiopeState.handle(_:)`, then to `AppState.perform(_:isRepeat:)`. VoiceOver reads "Sommaire, go to page 100" and the like.
- The rest of the bar, after one blank cell: the search in force, `SEARCH: tidal` (cyan); else, in white, the Garden's keys `k keep · r repair · b bury · u undo` in a Garden scope, or by column `type a page number · ? guide` (index), `j next · k previous · m read` (entries), `m read · s star · i images` (reader).
- During a run the hints give way to `REFRESHING 42/155`: phase 12 adds that (`MinitelBar.status`), with the garden's refusals, drawn through `hintLine` in place of the hints.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeFrameTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/MinitelBarTests`. Expected: `error: cannot find 'MinitelKey' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/MinitelBar.swift`:

```swift
import SwiftUI

/// A key of the Minitel bar (spec §6): its French label, what VoiceOver reads, what it does.
enum MinitelKey: CaseIterable, Sendable {
    case sommaire, suite, retour, guide, repetition

    /// The device's own key names, the one exception to English in the interface.
    var label: String {
        switch self {
        case .sommaire: "SOMMAIRE"
        case .suite: "SUITE"
        case .retour: "RETOUR"
        case .guide: "GUIDE"
        case .repetition: "RÉPÉTITION"
        }
    }

    var accessibilityLabel: String {
        switch self {
        case .sommaire: "Sommaire, go to page 100"
        case .suite: "Suite, next page"
        case .retour: "Retour, previous page"
        case .guide: "Guide, key help on page 899"
        case .repetition: "Répétition, refresh"
        }
    }

    /// The keyboard key's command, so that a click and a key do one thing.
    var command: ReadingCommand {
        switch self {
        case .sommaire: .sommaire
        case .suite: .advance
        case .retour: .previousEntry
        case .guide: .guide
        case .repetition: .repetition
        }
    }

    /// ` SOMMAIRE `, black on cyan.
    var line: CellLine {
        CellLine([CellSpan(" \(label) ", cells: label.count + 2, foreground: .black, background: .cyan)],
                 accessibilityText: accessibilityLabel)
    }
}

/// The text right of the keys and its colour.
struct MinitelHint: Equatable, Sendable {
    var text: String
    var colour: TeletextColor
}

/// The bottom row: the Minitel keys, then hints that follow the focus (spec §6, Minitel bar).
struct MinitelBar: View {
    let width: Int

    /// Each key with its two padding cells and the black cell after it.
    static let keysCells = MinitelKey.allCases.reduce(0) { $0 + $1.label.count + 3 }

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        HStack(spacing: 0) {
            ForEach(MinitelKey.allCases, id: \.self) { key in
                Button { press(key) } label: { CellLineView(line: key.line) }
                    .buttonStyle(.plain)
                    .accessibilityLabel(key.accessibilityLabel)
                CellLineView(line: .blank(1))
            }
            CellLineView(line: Self.hintLine(
                Self.hint(column: antiope.column, isGarden: app.selection.isGarden, searchText: app.searchText),
                width: max(0, width - Self.keysCells)))
        }
    }

    private func press(_ key: MinitelKey) {
        if !antiope.handle(key.command) { app.perform(key.command) }
    }

    /// The search in force first, then the keys of the Garden or of the focused column.
    static func hint(column: AntiopeColumn, isGarden: Bool, searchText: String) -> MinitelHint {
        if !searchText.isEmpty { return MinitelHint(text: "SEARCH: \(searchText)", colour: .cyan) }
        if isGarden { return MinitelHint(text: "k keep · r repair · b bury · u undo", colour: .white) }
        switch column {
        case .index: return MinitelHint(text: "type a page number · ? guide", colour: .white)
        case .entries: return MinitelHint(text: "j next · k previous · m read", colour: .white)
        case .reader: return MinitelHint(text: "m read · s star · i images", colour: .white)
        }
    }

    /// The hint after one blank cell, exactly `width` cells.
    static func hintLine(_ hint: MinitelHint, width: Int) -> CellLine {
        CellLine([CellSpan(" " + hint.text, cells: width, foreground: hint.colour)], accessibilityText: hint.text)
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/MinitelBarTests`. Expected: `✔ Test run with 3 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/MinitelBar.swift apps/macos/RessacTests/AntiopeFrameTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): Minitel key bar with focus hints

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 8.4: `AntiopeRootView`, the page header, separators and window chrome

**Files:**
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (replace the phase 4 stub whole)
- Test: `apps/macos/RessacTests/AntiopeFrameTests.swift` (append two suites)

**Interfaces:**
- Consumes: everything above; `AntiopeState` (`entry`, `notFound`, `currentPage`, `readerPage`, `closeSearchPrompt(clearing:)`), `PageEntry`, `ReadingWindowReader(makeRouter:)`, `ReadingKeyRouter(state:focus:antiope:)` (phase 7); `AppState.runs.isLongRunActive`; `ReaderView(state:)`, `GardenTable(state:scope:)`, `GardenInspector(state:scope:)`; `ReaderStyle.Keys.antiopeSize`, `TeletextFont.defaultSize` (phase 3).
- Produces: `struct AntiopeRootView: View` (`init(state:)`, `static let minimumReaderCells = 24`, `static let minimumCells`); `enum HeaderLabel: Equatable, Sendable { case page(Int?), typing(String), notFound(Int), searching(Int) }`; `struct PageHeader: View` (`init(width:)`), `static func label(entry:notFound:current:rolling:) -> HeaderLabel`, `static func line(label:date:width:timeZone:) -> CellLine`, `static func rollingPage(at:) -> Int`; `struct ColumnSeparator: View`; `struct AntiopeWindowChrome: NSViewRepresentable` with `ChromeView`.

**Rules:**
- `AntiopeRootView` owns one `AntiopeState` for its lifetime (`@State`), puts `AppState`, `AntiopeState` and `\.teletextSize` (`TeletextFont.snapped` of Antiope's own stored size, `ReaderStyle.Keys.antiopeSize`, default 20; the Native size `reader.size` is never read here) in the environment, and routes the window's keys with `ReadingKeyRouter(state:focus: antiope, antiope: antiope)`. It closes the search prompt, keeping its text, when it appears and when it goes away (Review Focus #5; phase 9 tests the prompt side).
- Header (spec §6, Page header): `P412` in yellow, `RESSAC` in white, then at the right edge `WED 14 OCT ` in cyan and `09:47:12` in yellow, in English (POSIX locale, Gregorian calendar, the Mac's time zone), ticking every second through `TimelineView`. While digits are typed: `P4__`. An unknown page: `P437 NOT FOUND` in red without `RESSAC`, for two seconds (`AntiopeState.notFound`). During a long run the page number counts 100…899 ten times a second, unless Reduce Motion is on. With no page (a folder not numbered yet): `P---`. Precedence: typing, then not found, then counting, then the page.
- The banner row goes between the header and the columns; phase 12 adds it (spec §8, Banners).

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeFrameTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/PageHeaderTests -only-testing:RessacTests/AntiopeRootTests`. Expected: `error: cannot find 'PageHeader' in scope` and `cannot find 'AntiopeWindowChrome' in scope`.

- [ ] **Step 3: Implement.** Replace `apps/macos/Ressac/Antiope/AntiopeRootView.swift` with:

```swift
import AppKit
import SwiftUI

/// The Antiope look of the reading window (spec §5, Window): the page header, the index, entry and
/// reader columns between straight separators, and the Minitel bar, on black. It shares `AppState`
/// with `MainView`; `AntiopeState` holds what only this look needs, for as long as it is on screen.
struct AntiopeRootView: View {
    /// The narrowest reader column, in cells, so that the window never squeezes it away.
    static let minimumReaderCells = 24
    /// Index, separator, entries, separator, reader.
    static let minimumCells = PageIndexColumn.cells + 1 + EntryPageColumn.cells + 1 + minimumReaderCells

    let state: AppState
    @State private var antiope: AntiopeState
    /// Antiope's own text size (decision of 2026-10-14), separate from the Native `reader.size`.
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var storedSize = TeletextFont.defaultSize

    init(state: AppState) {
        self.state = state
        _antiope = State(initialValue: AntiopeState(app: state))
    }

    var body: some View {
        let size = TeletextFont.snapped(storedSize)
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            VStack(alignment: .leading, spacing: 0) {
                PageHeader(width: width)
                // Phase 12 puts the banner row here (spec §8, Banners).
                HStack(alignment: .top, spacing: 0) {
                    PageIndexColumn()
                        .frame(width: CGFloat(PageIndexColumn.cells) * cell.width)
                    ColumnSeparator()
                    entriesColumn
                        .frame(width: CGFloat(EntryPageColumn.cells) * cell.width)
                    ColumnSeparator()
                    readerColumn
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity, alignment: .top)
                MinitelBar(width: width)
            }
        }
        .frame(minWidth: CGFloat(Self.minimumCells) * cell.width)
        .background(TeletextColor.black.color.ignoresSafeArea())
        .toolbar(.hidden, for: .windowToolbar)
        .background(ReadingWindowReader { [state, antiope] in
            ReadingKeyRouter(state: state, focus: antiope, antiope: antiope)
        })
        .background(AntiopeWindowChrome())
        .environment(state)
        .environment(antiope)
        .environment(\.teletextSize, size)
        // Neither look inherits an open search prompt; the search text stays (Review Focus #5).
        .onAppear { antiope.closeSearchPrompt(clearing: false) }
        .onDisappear { antiope.closeSearchPrompt(clearing: false) }
    }

    /// The entry list. In a Garden scope the native Garden table stays here until phase 12 draws
    /// P800–P802.
    @ViewBuilder private var entriesColumn: some View {
        if case let .garden(scope) = state.selection {
            GardenTable(state: state, scope: scope)
        } else {
            EntryPageColumn()
        }
    }

    /// The selected entry (`ReaderView`, themed by phase 10) or the Garden's inspector (redrawn by
    /// phase 12). Phase 9 draws P100 and P899 for `.index` and `.guide`; until then the article stays.
    @ViewBuilder private var readerColumn: some View {
        switch antiope.readerPage {
        case .article:
            if case let .garden(scope) = state.selection {
                GardenInspector(state: state, scope: scope)
            } else {
                ReaderView(state: state)
            }
        case .index, .guide:
            ReaderView(state: state)
        }
    }
}

/// What the header shows in place of the current page (spec §6, Page header).
enum HeaderLabel: Equatable, Sendable {
    /// The page on screen; nil while the selection has no number yet (`P---`).
    case page(Int?)
    /// The digits being typed, `"4__"`.
    case typing(String)
    /// An unknown page, in red for two seconds.
    case notFound(Int)
    /// The counter rolling during a refresh, as a TV searches for a page.
    case searching(Int)
}

/// The top row: `P412 RESSAC` … `WED 14 OCT 09:47:12`, ticking every second.
struct PageHeader: View {
    let width: Int

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // The counter is decorative and stays off with Reduce Motion (spec §8, Accessibility).
        let rolling = app.runs.isLongRunActive && !reduceMotion
        TimelineView(.periodic(from: .now, by: rolling ? 0.1 : 1)) { context in
            CellLineView(line: Self.line(
                label: Self.label(entry: antiope.entry, notFound: antiope.notFound, current: antiope.currentPage,
                                  rolling: rolling ? Self.rollingPage(at: context.date) : nil),
                date: context.date, width: width))
        }
    }

    /// Typing first, then `NOT FOUND`, then the rolling counter, then the page.
    static func label(entry: PageEntry, notFound: Int?, current: Int?, rolling: Int?) -> HeaderLabel {
        if entry.isActive { return .typing(entry.display) }
        if let notFound { return .notFound(notFound) }
        if let rolling { return .searching(rolling) }
        return .page(current)
    }

    /// 100…899, one step every tenth of a second.
    static func rollingPage(at date: Date) -> Int {
        let tenths = Int((date.timeIntervalSinceReferenceDate * 10).rounded(.down))
        return 100 + ((tenths % 800) + 800) % 800
    }

    /// The page in yellow (red when not found) and `RESSAC` in white on the left; the date in cyan
    /// and the time in yellow at the right edge. Exactly `width` cells: when the window is narrow,
    /// the time keeps its cells first, then the date, then the page.
    static func line(label: HeaderLabel, date: Date, width: Int, timeZone: TimeZone = .current) -> CellLine {
        let day = format(date, "EEE dd MMM", timeZone: timeZone).uppercased() + " "
        let time = format(date, "HH:mm:ss", timeZone: timeZone)
        let (page, colour, showsName): (String, TeletextColor, Bool) = switch label {
        case let .page(number): ("P" + (number.map { String($0) } ?? "---"), .yellow, true)
        case let .typing(digits): ("P" + digits, .yellow, true)
        case let .searching(number): ("P\(number)", .yellow, true)
        case let .notFound(number): ("P\(number) NOT FOUND", .red, false)
        }
        var remaining = max(0, width)
        func take(_ wanted: Int) -> Int {
            let cells = min(wanted, remaining)
            remaining -= cells
            return cells
        }
        let timeCells = take(time.count)
        let dayCells = take(day.count)
        let pageCells = take(page.count + 1)
        let nameCells = take(remaining)
        return CellLine([
            CellSpan(page, cells: pageCells, foreground: colour),
            CellSpan(showsName ? "RESSAC" : "", cells: nameCells, foreground: .white),
            CellSpan(day, cells: dayCells, foreground: .cyan),
            CellSpan(time, cells: timeCells, foreground: .yellow),
        ])
    }

    private static func format(_ date: Date, _ pattern: String, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = timeZone
        formatter.dateFormat = pattern
        return formatter.string(from: date)
    }
}

/// One cell drawing a blue `▐` on every row (spec §5): the right half of the cell in blue, the full
/// height of the columns. A shape, never text, so no glyph can bend it.
struct ColumnSeparator: View {
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        TeletextColor.blue.color
            .frame(width: cell.width / 2)
            .frame(width: cell.width, alignment: .trailing)
            .frame(maxHeight: .infinity)
            .accessibilityHidden(true)
    }
}

/// Dresses the window for Antiope while the root view is in it: no title, a transparent title bar
/// over full-size black content, dark controls. It puts everything back when it leaves, so a live
/// switch to Native finds the window as it was.
struct AntiopeWindowChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> ChromeView { ChromeView() }

    func updateNSView(_ view: ChromeView, context: Context) {}

    final class ChromeView: NSView {
        private struct Saved {
            var titleVisibility: NSWindow.TitleVisibility
            var titlebarAppearsTransparent: Bool
            var fullSizeContent: Bool
            var backgroundColor: NSColor
            var appearance: NSAppearance?
        }

        private var saved: Saved?
        private weak var dressed: NSWindow?

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            if let dressed, newWindow !== dressed { undress(dressed) }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window, dressed !== window { dress(window) }
        }

        private func dress(_ window: NSWindow) {
            saved = Saved(titleVisibility: window.titleVisibility,
                          titlebarAppearsTransparent: window.titlebarAppearsTransparent,
                          fullSizeContent: window.styleMask.contains(.fullSizeContentView),
                          backgroundColor: window.backgroundColor, appearance: window.appearance)
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.styleMask.insert(.fullSizeContentView)
            window.backgroundColor = .black
            window.appearance = NSAppearance(named: .darkAqua)
            dressed = window
        }

        private func undress(_ window: NSWindow) {
            defer {
                saved = nil
                dressed = nil
            }
            guard let saved else { return }
            window.titleVisibility = saved.titleVisibility
            window.titlebarAppearsTransparent = saved.titlebarAppearsTransparent
            if !saved.fullSizeContent { window.styleMask.remove(.fullSizeContentView) }
            window.backgroundColor = saved.backgroundColor
            window.appearance = saved.appearance
        }
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/PageHeaderTests -only-testing:RessacTests/AntiopeRootTests`, then the whole suite. Expected: `✔ Test run with 6 tests in 2 suites passed`, then `** TEST SUCCEEDED **`.

- [ ] **Step 5: Smoke launch in Antiope on a temporary database.** `-appearance antiope` sets the setting for this launch only (the argument domain), so Clément's own defaults are untouched. Nothing is typed into the app.

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
APP="$(xcodebuild -project apps/macos/Ressac.xcodeproj -scheme Ressac -showBuildSettings 2>/dev/null | awk '/ BUILT_PRODUCTS_DIR /{print $3}')/Ressac.app"
SMOKE="$(mktemp -d)"; mkdir -p "$SMOKE/home"
cp tests/fixtures/contract.sqlite "$SMOKE/ressac.sqlite"
RESSAC_DB="$SMOKE/ressac.sqlite" HOME="$SMOKE/home" "$APP/Contents/MacOS/Ressac" -appearance antiope >"$SMOKE/app.log" 2>&1 &
PID=$!
sleep 6
ps -p "$PID" -o stat= && lsof -p "$PID" | grep -c "$SMOKE/ressac.sqlite"
kill "$PID"
```

Expected: a state such as `SN` and a count of at least `1`. If a window appears, it is black with the header `P101 RESSAC` and a running clock, the index column with `101 Unread`, blue separators, and the Minitel keys at the bottom.

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/RessacTests/AntiopeFrameTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): root view with page header, three columns, separators and window chrome

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Verification gate

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "${TMPDIR:-/tmp}/ressac-xcodebuild.log" | tail -3
grep -E '^/.+/apps/macos/.+: (warning|error):' "${TMPDIR:-/tmp}/ressac-xcodebuild.log" || echo "no warnings in our sources"
```

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` (19 more Swift tests than after phase 7), `no warnings in our sources`. Push.

**Manual checks carried to phase 14** (a person at the keyboard; never simulated keystrokes):
1. Switch Appearance to Antiope and back without relaunching: the title bar, the toolbar and the light or dark look come back in Native.
2. The separators stay straight on feeds with emoji or CJK titles, at 15, 20, 25 and 30 points.
3. Typing `412` jumps; an unknown number shows `NOT FOUND` in red for two seconds; ⌫ and Esc edit the number in the header.
4. The clock ticks; during ⌘R the page number rolls, and stays still with Reduce Motion on.
5. Each Minitel key works on a click; VoiceOver reads "Sommaire, go to page 100".

### Notes for the executor

- **`toolbar(.hidden, for: .windowToolbar)`** hides the toolbar items `ReaderView` still declares; phase 10 decides what the reader shows in Antiope.
- **The title bar strip.** With `fullSizeContentView` the content's top safe-area inset is the title bar's height; the background ignores it (black to the top), the rows respect it (nothing under the traffic lights).
- **`TimelineView(.periodic(by:))`** re-renders only the header; the columns do not tick.
- **Plain buttons and hit testing.** `CellLineView` paints a background on every span (black included), so a whole row is clickable, not only its glyphs.
- **`AttributedString.foregroundColor`** with a SwiftUI `Color` picks the SwiftUI attribute scope; `Text` draws it.
- **Clicks during a run.** RÉPÉTITION is declined while a long run is active (`AppState.canRefresh`), the same as ⌘R.
- **No banner and no run status yet.** Spec §8's banner row and §6's `REFRESHING 42/155` belong to phase 12 (plan index), which adds both to the places named above; until then a run failure shows nowhere in Antiope, and the rolling page counter is the only sign of a run.
