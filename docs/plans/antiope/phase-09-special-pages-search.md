# Phase 9: P100 SOMMAIRE, P899 GUIDE, the search prompt and empty states

**Runs after:** phase 8.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`): Global Constraints (Language: the Minitel labels are the only French strings; Keys), Review Focus #5 (this phase owns its second half), and "Interfaces between phases" (Phases 7 and 8). Spec §5 (Search, Window: empty states), §6 (P100 SOMMAIRE and P899 GUIDE).

**Files (5):**
- Create: `apps/macos/Ressac/Antiope/SpecialPages.swift` (`CellLine.centred`, `DoubleHeight`, `SommairePage`, `GuidePage`)
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (`readerColumn`)
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`MinitelBar.body`, new `SearchPrompt`)
- Modify: `apps/macos/Ressac/Antiope/PageColumns.swift` (`EntryPageColumn`: empty state)
- Create: `apps/macos/RessacTests/AntiopePagesTests.swift`

**Prerequisites** (by their names in phases 7 and 8): `AntiopeState` (`directory`, `readerPage`, `jump(to:)`, `isSearchPromptOpen`, `closeSearchPrompt(clearing:)`, `keyFocus(in:)`), `PageDirectory`, `ReadingKeyRouter`; `CellSpan`, `CellLine`, `CellLineView`, `PageIndexColumn.line(for:width:isRootFolder:isSelected:highlightsReserved:)`, `PageIndexColumn.isRootFolder(_:in:)`, `MinitelBar` (`keysCells`, `hint`, `hintLine`), `EntryPageColumn.cells`; the test helpers `text(of:)` (AntiopeFrameTests), `pagedSnapshot()` (PageDirectoryTests), `makeState()`, `eventually`, `keyEvent`.

**Test command** (narrow it with one `-only-testing:` per suite):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/SommairePageTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

As in phase 8, the views are not tested; their row models are (widths, colours, contents), and the search prompt's behaviour is tested on `AntiopeState` and `AppState`, which the prompt only calls. The suites are `@MainActor` because the models are static members of `View` types.

**What later phases plug into** (new public names for the index table):
- `CellLine.centred(_:width:foreground:)` and `DoubleHeight(line:)`: phase 12's inspector page draws the status in double height with `DoubleHeight`.
- `SommairePage`, `GuidePage` (`sections`, `lines(width:)`): when a later phase adds a key (phase 11's `i` is already listed), it adds its line to `GuidePage.sections`.
- `SearchPrompt` (`label`).
- `EntryPageColumn.emptyLine(_:width:)`.

**Decisions:**
- **Empty states come from `AppState.emptyListMessage`**, in capitals: "No unread entries" gives `NO UNREAD ENTRIES`, "No results" gives `NO RESULTS`, "No entries" gives `NO ENTRIES`. One cyan line, centred in the 28-cell column; VoiceOver reads the sentence as written. The Garden's empty pages stay with `GardenTable` until phase 12.
- **P100**: the double-height `RESSAC` is yellow; the wave is two rows of half-block mosaics (`▄`, `▀`) in cyan, each column holding exactly one block, so it reads as one unbroken line; the pages follow in two columns, column-major (100 to 899 down the left, then the folders and feeds), formatted by `PageIndexColumn.line` with `highlightsReserved` (reserved titles yellow, the Garden magenta, root folders green). A click goes to the page.
- **P899** lists every key of the window in five sections, PAGES, READING, COLUMNS, GARDEN, MENUS, each heading black on green, the keys in cyan on a 12-cell column, the actions in white. Key names are spelled in capitals (`OPT S`, `SHIFT CMD R`, `DEL`, `ESC`, `LEFT RIGHT`): Bedstead has no `⌘`, `⌥`, `⇧` or `⌫`, and a magenta `?` in a help page would help no one. Actions are English; `SOMMAIRE`, `SUITE`, `RETOUR`, `GUIDE` and `RÉPÉTITION` appear as the key names they are.
- **The search prompt** replaces the hint line while it is open; the Minitel keys stay. Phase 12's run status takes the same place during runs; when phase 12 edits this `if`, the prompt keeps priority (typing must stay possible during a refresh). Return keeps the search (the hint line then shows `SEARCH: tidal`), Esc clears it and the full list comes back, a click elsewhere closes it and keeps the text. The prompt focuses its field when it appears.

---

### Task 9.1: P100 SOMMAIRE

**Files:**
- Create: `apps/macos/Ressac/Antiope/SpecialPages.swift`
- Test: `apps/macos/RessacTests/AntiopePagesTests.swift`

**Interfaces:**
- Consumes: `PageDirectory`, `AntiopeState.directory`, `AntiopeState.jump(to:)`, `PageIndexColumn.line(...)`, `PageIndexColumn.isRootFolder(_:in:)`, `CellLine`, `CellSpan`, `CellLineView`, `TeletextFont`, `\.teletextSize`.
- Produces: `extension CellLine { static func centred(_ text: String, width: Int, foreground: TeletextColor) -> CellLine }`; `struct DoubleHeight: View` (`init(line:)`); `struct SommairePage: View` with `Pair`, `static func pairs(_:) -> [Pair]`, `static func halfWidth(_:) -> Int`, `static func line(for:width:in:) -> CellLine`, `static func titleLine(width:) -> CellLine`, `static func logo(width:) -> [CellLine]`.

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/AntiopePagesTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/SommairePageTests`. Expected: `error: cannot find 'SommairePage' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/SpecialPages.swift`:

```swift
import SwiftUI

extension CellLine {
    /// `text` centred in `width` cells (one cell more on the right when the margin is odd).
    static func centred(_ text: String, width: Int, foreground: TeletextColor) -> CellLine {
        let margin = max(0, (width - text.count) / 2)
        return CellLine([CellSpan(String(repeating: " ", count: margin) + text, cells: width, foreground: foreground)],
                        accessibilityText: text)
    }
}

/// A cell row drawn twice as tall over two rows, as teletext's double height.
struct DoubleHeight: View {
    let line: CellLine
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        CellLineView(line: line)
            .scaleEffect(x: 1, y: 2, anchor: .top)
            .frame(height: cell.height * 2, alignment: .top)
    }
}

/// P100 SOMMAIRE (spec §6), drawn in the reader column: a double-height `RESSAC` over a mosaic
/// wave, then every page in two columns with its count, reserved pages in colour. A click goes to
/// the page; typing its number works as everywhere.
struct SommairePage: View {
    /// One row of the two-column list.
    struct Pair: Equatable, Sendable {
        var left: PageDirectory.Row
        var right: PageDirectory.Row?
    }

    /// The wave of the app icon in half-block mosaics, repeated across the page.
    static let wave: (top: [Character], bottom: [Character]) = (Array(" ▄▀▀▄   "), Array("▀    ▀▄▄"))

    @Environment(AntiopeState.self) private var antiope
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        let directory = antiope.directory
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            let half = Self.halfWidth(width)
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    DoubleHeight(line: Self.titleLine(width: width))
                    ForEach(Array(Self.logo(width: width).enumerated()), id: \.offset) { _, line in
                        CellLineView(line: line)
                    }
                    .accessibilityHidden(true)
                    CellLineView(line: .blank(width))
                    ForEach(Array(Self.pairs(directory.rows).enumerated()), id: \.offset) { _, pair in
                        HStack(spacing: 0) {
                            pageButton(pair.left, width: half, in: directory)
                            CellLineView(line: .blank(width - 2 * half))
                            if let right = pair.right {
                                pageButton(right, width: half, in: directory)
                            }
                        }
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
    }

    private func pageButton(_ row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> some View {
        let line = Self.line(for: row, width: width, in: directory)
        return Button { antiope.jump(to: row.number) } label: { CellLineView(line: line) }
            .buttonStyle(.plain)
            .accessibilityLabel(line.accessibilityText)
    }

    /// Column-major, as a printed index: the first half of the pages down the left, the rest
    /// down the right.
    static func pairs(_ rows: [PageDirectory.Row]) -> [Pair] {
        let leftCount = (rows.count + 1) / 2
        return (0..<leftCount).map { index in
            Pair(left: rows[index], right: index + leftCount < rows.count ? rows[index + leftCount] : nil)
        }
    }

    /// Each column's width; the one or two cells left over separate them.
    static func halfWidth(_ width: Int) -> Int {
        max(0, (width - 1) / 2)
    }

    /// An index row with the reserved titles in colour.
    static func line(for row: PageDirectory.Row, width: Int, in directory: PageDirectory) -> CellLine {
        PageIndexColumn.line(for: row, width: width, isRootFolder: PageIndexColumn.isRootFolder(row, in: directory),
                             isSelected: false, highlightsReserved: true)
    }

    static func titleLine(width: Int) -> CellLine {
        .centred("RESSAC", width: width, foreground: .yellow)
    }

    /// Two rows of the wave in cyan; decorative, so VoiceOver skips them.
    static func logo(width: Int) -> [CellLine] {
        [wave.top, wave.bottom].map { pattern in
            let row = String((0..<max(0, width)).map { pattern[$0 % pattern.count] })
            return CellLine([CellSpan(row, cells: width, foreground: .cyan)], accessibilityText: "")
        }
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/SommairePageTests`. Expected: `✔ Test run with 4 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/SpecialPages.swift apps/macos/RessacTests/AntiopePagesTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): P100 SOMMAIRE with double-height title, mosaic wave and two-column index

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 9.2: P899 GUIDE, and both pages in the reader column

**Files:**
- Modify: `apps/macos/Ressac/Antiope/SpecialPages.swift` (append `GuideKey`, `GuideSection`, `GuidePage`)
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (`readerColumn`)
- Test: `apps/macos/RessacTests/AntiopePagesTests.swift` (append a suite)

**Interfaces:**
- Consumes: `CellLine.centred`, `DoubleHeight` (9.1), `AntiopeState.readerPage`.
- Produces: `struct GuideKey: Equatable, Sendable { var keys: String; var action: String }`; `struct GuideSection: Equatable, Sendable { var title: String; var keys: [GuideKey] }`; `struct GuidePage: View` with `static let keyCells = 12`, `static let sections: [GuideSection]`, `static func lines(width:) -> [CellLine]`. `AntiopeRootView` shows `SommairePage()` for `.index` and `GuidePage()` for `.guide`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopePagesTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/GuidePageTests`. Expected: `error: cannot find 'GuidePage' in scope`.

- [ ] **Step 3: Implement the page.** Append to `apps/macos/Ressac/Antiope/SpecialPages.swift`:

```swift
/// One line of P899: the keys, spelled out, and what they do.
struct GuideKey: Equatable, Sendable {
    var keys: String
    var action: String
}

struct GuideSection: Equatable, Sendable {
    var title: String
    var keys: [GuideKey]
}

/// P899 GUIDE (spec §6), drawn in the reader column: every key of the window, grouped as the
/// GUIDE page of a Minitel service. Keys are spelled in capitals: Bedstead has no ⌘, ⌥, ⇧ or ⌫.
struct GuidePage: View {
    /// The key column, wide enough for ` SHIFT CMD R`.
    static let keyCells = 12

    static let sections: [GuideSection] = [
        GuideSection(title: "PAGES", keys: [
            GuideKey(keys: "0-9", action: "type a page number"),
            GuideKey(keys: "DEL", action: "erase the last digit"),
            GuideKey(keys: "ESC", action: "cancel the number"),
            GuideKey(keys: "OPT S", action: "SOMMAIRE: every page"),
            GuideKey(keys: "OPT G  ?", action: "GUIDE: this page"),
        ]),
        GuideSection(title: "READING", keys: [
            GuideKey(keys: "SPACE  J", action: "SUITE: next page"),
            GuideKey(keys: "K", action: "RETOUR: previous page"),
            GuideKey(keys: "UP DOWN", action: "previous or next entry"),
            GuideKey(keys: "M", action: "mark read or unread"),
            GuideKey(keys: "S", action: "star or unstar"),
            GuideKey(keys: "I", action: "mosaics or original images"),
            GuideKey(keys: "OPT R", action: "RÉPÉTITION: refresh"),
            GuideKey(keys: "CMD RETURN", action: "open in the browser"),
        ]),
        GuideSection(title: "COLUMNS", keys: [
            GuideKey(keys: "LEFT RIGHT", action: "change column"),
            GuideKey(keys: "UP DOWN", action: "in the index: previous or next page"),
        ]),
        GuideSection(title: "GARDEN", keys: [
            GuideKey(keys: "K", action: "keep"),
            GuideKey(keys: "R", action: "repair"),
            GuideKey(keys: "B", action: "bury"),
            GuideKey(keys: "U", action: "undo"),
            GuideKey(keys: "O", action: "open the site"),
            GuideKey(keys: "W", action: "open the Wayback snapshot"),
            GuideKey(keys: "SHIFT R", action: "apply the safe repairs"),
        ]),
        GuideSection(title: "MENUS", keys: [
            GuideKey(keys: "CMD F", action: "search: RETURN keeps it, ESC clears it"),
            GuideKey(keys: "CMD R", action: "refresh the selection"),
            GuideKey(keys: "SHIFT CMD R", action: "refresh every feed"),
            GuideKey(keys: "SHIFT CMD A", action: "mark all as read"),
            GuideKey(keys: "CMD + -", action: "bigger or smaller text"),
            GuideKey(keys: "CMD O", action: "import OPML"),
            GuideKey(keys: "SHIFT CMD E", action: "export OPML"),
        ]),
    ]

    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let width = max(0, Int(proxy.size.width / cell.width))
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    DoubleHeight(line: .centred("GUIDE", width: width, foreground: .yellow))
                    ForEach(Array(Self.lines(width: width).enumerated()), id: \.offset) { _, line in
                        CellLineView(line: line)
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
    }

    /// For each section: a blank row, the heading black on green, then one row per key, the keys
    /// in cyan and the action in white. Every row is exactly `width` cells.
    static func lines(width: Int) -> [CellLine] {
        let keyCells = min(Self.keyCells, max(0, width))
        var lines: [CellLine] = []
        for section in sections {
            lines.append(.blank(width))
            lines.append(CellLine([CellSpan(" " + section.title, cells: width, foreground: .black, background: .green)],
                                  accessibilityText: section.title))
            for key in section.keys {
                lines.append(CellLine([
                    CellSpan(" " + key.keys, cells: keyCells, foreground: .cyan),
                    CellSpan(key.action, cells: width - keyCells, foreground: .white),
                ], accessibilityText: "\(key.keys): \(key.action)"))
            }
        }
        return lines
    }
}
```

- [ ] **Step 4: Show both pages in the reader column.** In `apps/macos/Ressac/Antiope/AntiopeRootView.swift`, replace:

```swift
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
```

with:

```swift
    /// P100 SOMMAIRE, P899 GUIDE, or else the selected entry (`ReaderView`, themed by phase 10) or
    /// the Garden's inspector (redrawn by phase 12).
    @ViewBuilder private var readerColumn: some View {
        switch antiope.readerPage {
        case .article:
            if case let .garden(scope) = state.selection {
                GardenInspector(state: state, scope: scope)
            } else {
                ReaderView(state: state)
            }
        case .index:
            SommairePage()
        case .guide:
            GuidePage()
        }
    }
```

- [ ] **Step 5: Run** with `-only-testing:RessacTests/SommairePageTests -only-testing:RessacTests/GuidePageTests`. Expected: `✔ Test run with 8 tests in 2 suites passed`. If `thePagesDrawWithoutAMissingGlyph` fails on `#require`, Bedstead is not registered in the test host: fix that (phase 3) rather than the test.

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/Antiope/SpecialPages.swift apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/RessacTests/AntiopePagesTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): P899 GUIDE with every key, P100 and P899 in the reader column

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 9.3: the `SEARCH?` prompt in the Minitel bar

**Files:**
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`MinitelBar.body`; append `SearchPrompt`)
- Test: `apps/macos/RessacTests/AntiopePagesTests.swift` (append a suite)

**Interfaces:**
- Consumes: `AntiopeState.isSearchPromptOpen` (which is `AppState.isSearchFocused`, set by the existing ⌘F menu item), `AntiopeState.closeSearchPrompt(clearing:)`, `AppState.searchText`, `MinitelBar.keysCells`.
- Produces: `struct SearchPrompt: View` (`init(width:)`, `static let label = "SEARCH? "`).

**Rules (spec §5, Search):**
- ⌘F opens the prompt (no new menu item: `SearchMenuItem` already sets `isSearchFocused`); it focuses its field.
- While it is open, `AntiopeState.keyFocus(in:)` is `.textInput`: every key, digits included, is typing (Review Focus #2; tested in phase 7).
- Return: `closeSearchPrompt(clearing: false)`. Esc: `closeSearchPrompt(clearing: true)`. Focus leaving the field: `closeSearchPrompt(clearing: false)`.
- Switching Appearance with the prompt open: `AntiopeRootView` closes it on disappear and on appear, keeping the text (Review Focus #5).

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopePagesTests.swift`:

```swift
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
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/SearchPromptTests`. Expected: `error: cannot find 'SearchPrompt' in scope`.

- [ ] **Step 3: Implement.** In `apps/macos/Ressac/Antiope/MinitelBar.swift`, replace `MinitelBar`'s `body`:

```swift
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
```

with:

```swift
    var body: some View {
        HStack(spacing: 0) {
            ForEach(MinitelKey.allCases, id: \.self) { key in
                Button { press(key) } label: { CellLineView(line: key.line) }
                    .buttonStyle(.plain)
                    .accessibilityLabel(key.accessibilityLabel)
                CellLineView(line: .blank(1))
            }
            if antiope.isSearchPromptOpen {
                SearchPrompt(width: max(0, width - Self.keysCells))
            } else {
                CellLineView(line: Self.hintLine(
                    Self.hint(column: antiope.column, isGarden: app.selection.isGarden, searchText: app.searchText),
                    width: max(0, width - Self.keysCells)))
            }
        }
    }
```

Then append at the end of the file:

```swift
/// `SEARCH? ____` in the Minitel bar (spec §5, Search), opened by ⌘F: a field bound to
/// `AppState.searchText`, the same text as the native search field. Return keeps the search, Esc
/// clears it; both close the prompt, and so does a click elsewhere (the search stays).
struct SearchPrompt: View {
    static let label = "SEARCH? "

    let width: Int

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope
    @Environment(\.teletextSize) private var size
    @FocusState private var focused: Bool

    var body: some View {
        @Bindable var app = app
        let cell = TeletextFont.cell(forSize: size)
        let labelCells = min(Self.label.count + 1, width)
        HStack(spacing: 0) {
            CellLineView(line: CellLine([CellSpan(" " + Self.label, cells: labelCells, foreground: .cyan)],
                                        accessibilityText: ""))
            TextField("Search", text: $app.searchText, prompt: Text("____").foregroundStyle(TeletextColor.yellow.color))
                .textFieldStyle(.plain)
                .font(TeletextFont.font(size: size))
                .foregroundStyle(TeletextColor.yellow.color)
                .tint(TeletextColor.white.color)
                .focused($focused)
                .onSubmit { antiope.closeSearchPrompt(clearing: false) }
                .onExitCommand { antiope.closeSearchPrompt(clearing: true) }
                .frame(width: CGFloat(max(0, width - labelCells)) * cell.width, height: cell.height, alignment: .leading)
                .background(TeletextColor.black.color)
        }
        // After the first layout, so that the field exists when it takes the focus.
        .task { focused = true }
        .onChange(of: focused) { _, isFocused in
            if !isFocused { antiope.closeSearchPrompt(clearing: false) }
        }
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/SearchPromptTests`. Expected: `✔ Test run with 3 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/MinitelBar.swift apps/macos/RessacTests/AntiopePagesTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): SEARCH? prompt in the Minitel bar, bound to the shared search text

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 9.4: empty states

**Files:**
- Modify: `apps/macos/Ressac/Antiope/PageColumns.swift` (`EntryPageColumn`)
- Test: `apps/macos/RessacTests/AntiopePagesTests.swift` (append a suite)

**Interfaces:**
- Consumes: `CellLine.centred` (9.1), `AppState.entries`, `AppState.emptyListMessage`.
- Produces: `static func emptyLine(_ message: String, width: Int) -> CellLine` on `EntryPageColumn`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopePagesTests.swift`:

```swift
@MainActor @Suite struct EmptyStateTests {
    @Test func theMessageIsOneCentredCyanLine() {
        let line = EntryPageColumn.emptyLine("No unread entries", width: 28)
        #expect(line.width == 28)
        #expect(text(of: line.spans[0]) == "     NO UNREAD ENTRIES      ")
        #expect(line.spans[0].foreground == .cyan)
        #expect(line.accessibilityText == "No unread entries")  // VoiceOver reads the sentence
        #expect(EntryPageColumn.emptyLine("A message far longer than the twenty-eight cells", width: 28).width == 28)
    }

    @Test func theMessagesComeFromAppState() async throws {
        let (state, test, _) = try makeState()
        try test.entry(feed: 8, read: true)
        try await state.database.notifyExternalChanges()
        state.selection = .feed(8)
        #expect(!state.entries.isEmpty)
        func shown() -> String { text(of: EntryPageColumn.emptyLine(state.emptyListMessage, width: 28).spans[0]) }
        state.unreadOnly = true
        #expect(state.entries.isEmpty && shown().contains("NO UNREAD ENTRIES"))
        state.unreadOnly = false
        state.searchText = "zzz"
        #expect(state.entries.isEmpty && shown().contains("NO RESULTS"))
        state.searchText = ""
        state.selection = .starred
        #expect(state.entries.isEmpty && shown().contains("NO ENTRIES"))
    }
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/EmptyStateTests`. Expected: `error: type 'EntryPageColumn' has no member 'emptyLine'`.

- [ ] **Step 3: Implement.** In `apps/macos/Ressac/Antiope/PageColumns.swift`, in `EntryPageColumn.body`, replace:

```swift
            .scrollIndicators(.hidden)
            // `j`, `k` and Space select from the keyboard: the column follows the selection.
```

with:

```swift
            .scrollIndicators(.hidden)
            .overlay {
                if app.entries.isEmpty {
                    CellLineView(line: Self.emptyLine(app.emptyListMessage, width: Self.cells))
                }
            }
            // `j`, `k` and Space select from the keyboard: the column follows the selection.
```

and, at the end of `accessibilityLabel(for:)` inside `EntryPageColumn`, replace:

```swift
        if entry.isStarred { parts.append("starred") }
        return parts.joined(separator: ", ")
    }
```

with:

```swift
        if entry.isStarred { parts.append("starred") }
        return parts.joined(separator: ", ")
    }

    /// `NO UNREAD ENTRIES`: `AppState.emptyListMessage` in capitals, one cyan line, centred
    /// (spec §5); VoiceOver reads the sentence as written.
    static func emptyLine(_ message: String, width: Int) -> CellLine {
        var line = CellLine.centred(message.uppercased(), width: width, foreground: .cyan)
        line.accessibilityText = message
        return line
    }
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/EmptyStateTests`, then the whole suite. Expected: `✔ Test run with 2 tests in 1 suite passed`, then `** TEST SUCCEEDED **`.

- [ ] **Step 5: Smoke launch in Antiope on a temporary database** (nothing is typed into the app):

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

Expected: a state such as `SN` and a count of at least `1`. If a window appears, the entry column shows `NO UNREAD ENTRIES` in cyan (the fixture has no entries).

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/Antiope/PageColumns.swift apps/macos/RessacTests/AntiopePagesTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): empty entry pages show one centred cyan line

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

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` (13 more Swift tests than after phase 8), `no warnings in our sources`. Push.

**Manual checks carried to phase 14** (a person at the keyboard; never simulated keystrokes):
1. ⌥S shows P100: double-height `RESSAC`, the wave, every page in two columns; a click on a page and typing its number both go there.
2. `?` and ⌥G show P899; every key it lists does what it says.
3. ⌘F opens `SEARCH? ____` with the cursor in it; typing `412` searches for "412" and jumps nowhere; Return keeps the search and the bar shows `SEARCH: 412`; ⌘F then Esc clears it.
4. With the prompt open, switch Appearance to Native: the native search field holds the text and `j` moves in the list.
5. An empty Unread list shows `NO UNREAD ENTRIES` centred in cyan.

### Notes for the executor

- **`Text.foregroundStyle(_:)`** has a `Text`-returning overload (macOS 14+), which a `TextField` prompt needs; `foregroundColor` is deprecated and would warn.
- **`onExitCommand`** is macOS's Esc (`cancelOperation:`), which the field editor sends up the responder chain; the router leaves Esc alone while the prompt is open (`.textInput`).
- **`@Bindable var app = app`** inside `body` turns the environment's `AppState` into a binding source for `$app.searchText`.
- **If the field does not take the focus** (a window not yet key), a click in it does; the prompt stays open until Return, Esc or a click elsewhere.
- **Double height** is a vertical scale of one row into a two-row frame, as `scaleY(2)` in the reader (phase 10); Bedstead's pixels stay square in width and become 2:1, as on a teletext set.
