# Phase 12: The garden in Antiope (P800, P801, P802), banners and refresh progress

**Runs after:** phase 11b.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`). Global Constraints apply. Spec §8 (garden pages, `GardenStatusStyle`, banners, what stays native, accessibility) and §6 (Minitel bar hints, `REFRESHING 42/155`).

**Goal of the phase:** in Antiope, the three garden pages list the feeds by status under coloured headings with a mosaic progress row, the reader column shows the inspector as a teletext page, every garden key works exactly as in the native table, run failures show as a red reverse-video row under the header, and the Minitel bar shows `REFRESHING n/m` during runs.

**Files (5):**
- Create: `apps/macos/Ressac/Antiope/GardenPage.swift` (pure row model: entry column, inspector page, banner row)
- Create: `apps/macos/Ressac/Antiope/GardenPageViews.swift` (`AntiopeGardenStore`, `GardenEntryPage`, `GardenInspectorPage`, `AntiopeBannerRow`, `GardenLineView`)
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (garden pages in the entry and reader columns, banner row, store in the environment)
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`MinitelBar.status`, and the status in place of the hints during runs and after a garden refusal)
- Create: `apps/macos/RessacTests/AntiopeGardenTests.swift`

No other file changes. `AppState.swift` is **not** touched, so no Step-0 cleanup is needed in this phase. `AntiopeRootView.swift` and `MinitelBar.swift` only get small replacements; nothing is restructured.

**Consumes** (names as phases 3 to 9 define them; every edit below was applied to the text those phases leave and built):
- Phase 3: `TeletextColor` (`.color`), `TeletextFont.cell(forSize:)`, `TeletextFont.supports(_:)`, `TeletextFont.isAvailable`, `EnvironmentValues.teletextSize`, `CellRun`.
- Phase 5: `GardenStatusStyle.teletextColor(for: String?) -> TeletextColor` (Dead red, Hijacked magenta, Moved cyan, Ailing yellow, Duplicate white, Fossil blue, Dormant green, Alive green, nil/unknown white).
- Phase 7: `AntiopeColumn` (`index`, `entries`, `reader`), `AntiopeState` (`column`, `readerPage`, `isSearchPromptOpen` (a view of `AppState.isSearchFocused`), `init(app:clock:)`, `handle(_:) -> Bool`, which returns false for `.nextEntry`, `.previousEntry`, `.advance`, `.toggleRead`, `.toggleStar`, and consumes `↑`/`↓` in the index column as `.nextIndexRow`/`.previousIndexRow`), `ReadingCommand`; `ReadingKeyRouter` offers each command to `AntiopeState.handle`, then to `AppState.perform`, then lets the key go on to the focused view; `r`, `b`, `u`, `o`, `w` stay unbound.
- Phase 8: `CellSpan` (`init(_:cells:foreground:background:)`, `cells`, `foreground`, `background`, `cellCount`), `CellLine` (`init(_:accessibilityText:)`, `spans`, `accessibilityText`, `width`, `blank(_:)`, `reversed(_:on:)`), `CellLineView(line:)`, `EntryPageColumn.cells` (28), `MinitelHint`, `MinitelBar.hint(column:isGarden:searchText:)`, `MinitelBar.hintLine(_:width:)`; `AntiopeRootView` with `@State private var antiope`, the banner marker comment under `PageHeader(width:)`, and the private builders `entriesColumn` and `readerColumn`; the test helper `text(of:)` (`AntiopeFrameTests.swift`).
- Phase 9: `CellLine.centred(_:width:foreground:)`, `DoubleHeight(line:)`; `readerColumn` switches on `antiope.readerPage` with the Garden test inside `case .article:`; `MinitelBar.body` shows `SearchPrompt` while `antiope.isSearchPromptOpen`, else the hint line.
- Existing code, unchanged: `GardenModel` (`sections`, `visibleRows`, `progress`, `safeRepairs`, `applyQuestion`, `safeRepairPreview`, `selectionAfterUpdate`, `command(for:modifiers:row:safeRepairCount:)`, `chip(for:)`, `remarkSentence`, `outcome(of:)`, `status(of:)`, `isBuried`, `title(for:)`, `emptyMessage(for:)`, `urgencyOrder`), `GardenCommand`, `GardenRow`, `GardenEvidence`, `Remark`, `ProposedAction.label`, `FetchLogRow`, `Hop`, `GardenScope`, `Source.isGarden`, `Queries.garden(scope:)`, `Queries.fetchLog(feedId:)`, `AppState` (`selection`, `selectedFeedId`, `decidedThisVisit`, `gardenDecide(_:)`, `gardenUndo()`, `gardenApplySafe()`, `openURL`, `database`, `runs`), `RunCoordinator` (`progress`, `banner`), `RunProgress`, `Banner`, `LinkPolicy.webURL(_:)`, `DatabaseLocation.logsURL`. Test helpers: `makeState()`, `calls(_:)`, `eventually(_:)`, `TestDatabase.raw`.

**Produces** (new names; none is needed by a later phase except through `AntiopeRootView`):
- `enum GardenPage` with `Line` (`id`, `role`, `cells: CellLine`, `isDoubleHeight`, `isSelected`, `text`), `Line.Role` (`title`, `progress`, `heading(String)`, `feed(Int64)`, `text`, `blank`), `entryLines(_:scope:retained:selected:cells:)`, `orderedRows(_:scope:retained:)`, `titleLine`, `progressLine`, `headingStyle(for:)`, `headingLine`, `feedLine`, `blankLine(id:cells:)`, `takesArrows(column:)`, `move(_:in:by:)`, `inspectorLines(_:log:cells:timeZone:)`, `wrap(_:cells:)`, `logDate(_:timeZone:)`, `BannerRow` (`message`, `log`, `dismiss`, `width`), `bannerRow(_:cells:)`.
- `MinitelBar.status(progress:message:) -> MinitelHint?`.
- `@MainActor @Observable final class AntiopeGardenStore`; views `GardenEntryPage(scope:)`, `GardenInspectorPage()`, `AntiopeBannerRow(banner:dismiss:)`, `GardenLineView(line:)`.

**Design, in short:**
- **One row model for every Antiope view.** A garden row is phase 8's `CellLine` (exact width, per-span colours and backgrounds, reverse video, magenta `?` for what Bedstead lacks, the original text for VoiceOver), drawn by `CellLineView`, or by phase 9's `DoubleHeight` for the inspector's status; centred messages use phase 9's `CellLine.centred`. `GardenPage.Line` only adds what a garden page needs on top of a `CellLine`: an identity for `ForEach` and scrolling, a role for clicks, the double-height flag and the selection trait. There is no second span type and no second row view.
- Rows are built by pure functions over `GardenModel`, so every rule (grouping, colours, widths, reverse video, wrapping) is unit-tested; the views only draw them. As in phases 8 and 9 the tests read the rows with Bedstead registered in the test host (phase 3).
- One `AntiopeGardenStore` replaces the two separate observations that the native `GardenTable` and `GardenInspector` each run: the entry page drives it, the inspector page and the Minitel bar read it. Its selection logic and its command dispatch are the native table's, reusing `GardenModel` unchanged.
- Garden keys: `ReadingKeyRouter` (phase 7) offers each key to `AntiopeState.handle` then to `AppState.perform`; in a garden scope both return false for `j`, `k`, Space, `m` and `s`, so the key reaches the focused view, which is `GardenEntryPage` (focusable, `onKeyPress`), as the native `GardenTable` does. Task 12.3 pins this with a test. `↑`/`↓` move the garden selection unless the index column is active.
- Teletext has no dimming and no other colours, so a decided row is shown by its content instead of 45 % opacity: no yellow `●` (the "needs triage" mark, like the unread dot of entry rows) and the decision chip (`kept`, `repaired`, `buried`) in green.
- The Minitel bar's priority: the open `SEARCH?` prompt first (phase 9: typing must stay possible during a refresh), then the garden's last refusal, then `REFRESHING n/m` during a run, then the hints of phase 8.

---

### Task 12.1: Garden entry column rows (pure)

**Files:**
- Create: `apps/macos/Ressac/Antiope/GardenPage.swift`
- Create: `apps/macos/RessacTests/AntiopeGardenTests.swift`

**Interfaces:**
- Consumes: `GardenModel`, `GardenRow`, `GardenScope`, `GardenStatusStyle.teletextColor(for:)`, `TeletextColor`, `CellSpan`, `CellLine`, `CellLine.centred`, `EntryPageColumn.cells`, `AntiopeColumn`.
- Produces: `GardenPage.Line`, `Line.Role`, `entryLines(_:scope:retained:selected:cells:)`, `orderedRows(_:scope:retained:)`, `titleLine(_:scope:visibleCount:cells:)`, `progressLine(_:cells:)`, `headingStyle(for:)`, `headingLine(_:cells:)`, `feedLine(_:selected:cells:)`, `blankLine(id:cells:)`, `takesArrows(column:)`, `move(_:in:by:)`.

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/AntiopeGardenTests.swift`:

```swift
import Foundation
import GRDB
import SwiftUI
import Testing

@testable import Ressac

private func gardenRow(_ id: Int64, _ title: String, status: String?, folder: String? = "Blogs", state: String = "active",
                       reason: String? = nil, evidence: [String]? = nil, remarks: [Remark] = [],
                       action: ProposedAction? = nil, needsTriage: Bool = false, decision: String? = nil,
                       site: String? = nil, wayback: String? = nil) -> GardenRow {
    GardenRow(feedId: id, title: title, folderPath: folder, state: state, status: status, reason: reason, clue: nil,
              evidence: evidence.map { GardenEvidence(bullets: $0, clue: "", language: nil) }, remarks: remarks,
              proposedAction: action, needsTriage: needsTriage, lastDecision: decision,
              waybackUrl: wayback, htmlUrl: site, xmlUrl: "https://feeds.example.org/\(id)")
}

private func spaces(_ count: Int) -> String { String(repeating: " ", count: count) }

private func trailingTrimmed(_ text: String) -> String {
    String(text.reversed().drop { $0 == " " }.reversed())
}

/// The rows are phase 8's `CellLine`s: these suites read their text with Bedstead registered in the
/// test host, as phases 8 and 9 do (`?` stands for a glyph Bedstead lacks).
@MainActor @Suite struct GardenPageTests {
    let cells = EntryPageColumn.cells

    /// Review Focus #1 for the garden: whatever the title, every row is exactly the entry column's
    /// 28 cells, so the column separators stay straight.
    @Test func everyLineIsExactlyTheColumnWidth() {
        let rows = [
            gardenRow(1, "Café 🎉 東京", status: "Dead", action: .bury, needsTriage: true),
            gardenRow(2, "A title far longer than the twenty-eight cells of the column", status: "Moved",
                      action: .followRedirect(url: "https://m.example.org/feed"), needsTriage: true),
            gardenRow(3, "e\u{301}cole", status: "Fossil", needsTriage: false, decision: "keep"),
            gardenRow(4, "Gone", status: "Dead", state: "buried", decision: "bury"),
            gardenRow(5, "", status: nil),
        ]
        for scope in [GardenScope.needsTriage, .all, .cemetery] {
            for selected in [nil, Int64(1), 2] {
                let lines = GardenPage.entryLines(rows, scope: scope, retained: [1, 2, 3, 4], selected: selected, cells: cells)
                #expect(!lines.isEmpty)
                for line in lines {
                    #expect(line.cells.width == 28, "\(scope) \(line.id): «\(line.text)»")
                    #expect(line.text.count == 28, "\(scope) \(line.id): «\(line.text)»")
                    #expect(!line.isDoubleHeight)
                }
            }
        }
    }

    @Test func feedsAreGroupedByStatusInUrgencyOrder() {
        let statuses: [String?] = ["Alive", "Dead", "Fossil", "Duplicate", "Hijacked", "Moved", "Ailing", "Dormant", nil]
        let rows = statuses.enumerated().map { gardenRow(Int64($0.offset + 1), "Feed \($0.offset + 1)", status: $0.element) }
        let roles = GardenPage.entryLines(rows, scope: .all, retained: [], selected: nil, cells: cells).map(\.role)
        #expect(roles == [
            .title,
            .heading("Duplicate"), .feed(4), .heading("Hijacked"), .feed(5), .heading("Moved"), .feed(6),
            .heading("Dead"), .feed(2), .heading("Ailing"), .feed(7), .heading("Fossil"), .feed(3),
            .heading("Dormant"), .feed(8), .heading("Alive"), .feed(1), .heading("Unchecked"), .feed(9),
        ])
        #expect(GardenPage.orderedRows(rows, scope: .all, retained: []).map(\.feedId) == [4, 5, 6, 2, 7, 3, 8, 1, 9])
    }

    @Test func headingsTakeTheirTeletextColourAndFossilIsReverseVideo() {
        let dead = GardenModel.Section(id: "Dead", glyph: "🪦",
                                       rows: [gardenRow(1, "A", status: "Dead"), gardenRow(2, "B", status: "Dead")])
        let line = GardenPage.headingLine(dead, cells: cells)
        #expect(line.text == " DEAD" + spaces(22) + "2")
        #expect(line.cells.spans.allSatisfy { $0.foreground == .red && $0.background == .black })
        #expect(line.cells.accessibilityText == "Dead, 2 feeds")

        let fossil = GardenPage.headingLine(GardenModel.Section(id: "Fossil", glyph: "🦴", rows: [gardenRow(3, "C", status: "Fossil")]),
                                            cells: cells)
        #expect(fossil.text == " FOSSIL" + spaces(20) + "1")
        #expect(fossil.cells.spans.allSatisfy { $0.foreground == .white && $0.background == .blue })
        #expect(fossil.cells.accessibilityText == "Fossil, 1 feed")

        for status in GardenModel.urgencyOrder + ["Buried"] {
            let style = GardenPage.headingStyle(for: status)
            let colour = GardenStatusStyle.teletextColor(for: status)
            #expect(style.foreground != .blue, "\(status): blue never colours text")
            if colour == .blue {
                #expect(style.foreground == .white && style.background == .blue, "\(status)")
            } else {
                #expect(style.foreground == colour && style.background == .black, "\(status)")
            }
        }
    }

    @Test func titleRowCountsWhatIsLeftOrListed() {
        let rows = [gardenRow(1, "A", status: "Dead", needsTriage: true),
                    gardenRow(2, "B", status: "Fossil", needsTriage: false, decision: "keep")]
        let queue = GardenPage.titleLine(rows, scope: .needsTriage, visibleCount: 2, cells: cells)
        #expect(queue.text == "NEEDS TRIAGE" + spaces(10) + "1 LEFT")
        #expect(queue.cells.spans.last == CellSpan(" 1 LEFT", cells: 7, foreground: .yellow))
        #expect(queue.cells.accessibilityText == "Needs Triage, 1 left")
        #expect(GardenPage.titleLine(rows, scope: .all, visibleCount: 186, cells: cells).text
            == "ALL FEEDS" + spaces(10) + "186 FEEDS")
        #expect(GardenPage.titleLine(rows, scope: .cemetery, visibleCount: 1, cells: cells).text
            == "CEMETERY" + spaces(14) + "1 FEED")
    }

    /// The mosaic gauge: `▐`, done cells in green, the rest in blue shade, then done/total, always
    /// exactly 28 cells, from an empty garden to a five-digit one.
    @Test func progressRowIsExactlyTheColumnWidth() {
        let cases: [(left: Int, total: Int, expected: String)] = [
            (68, 80, "▐" + String(repeating: "█", count: 3) + String(repeating: "░", count: 18) + " 12/80"),
            (0, 0, "▐" + String(repeating: "░", count: 23) + " 0/0"),
            (0, 80, "▐" + String(repeating: "█", count: 21) + " 80/80"),
            (2, 3, "▐" + String(repeating: "█", count: 7) + String(repeating: "░", count: 16) + " 1/3"),
            (0, 12345, "▐" + String(repeating: "█", count: 15) + " 12345/12345"),
            (90, 80, "▐" + String(repeating: "░", count: 22) + " 0/80"),
        ]
        for (left, total, expected) in cases {
            let line = GardenPage.progressLine(GardenModel.Progress(left: left, total: total), cells: cells)
            #expect(line.text == expected, "\(left)/\(total)")
            #expect(line.cells.width == 28, "\(left)/\(total)")
            #expect(line.role == .progress)
        }
        let line = GardenPage.progressLine(GardenModel.Progress(left: 68, total: 80), cells: cells)
        #expect(line.cells.spans == [
            CellSpan("▐", cells: 1, foreground: .cyan),
            CellSpan("███", cells: 3, foreground: .green),
            CellSpan(String(repeating: "░", count: 18), cells: 18, foreground: .blue),
            CellSpan(" 12/80", cells: 6, foreground: .yellow),
        ])
        #expect(line.cells.accessibilityText == "12 of 80 triaged")
        #expect(GardenPage.entryLines([], scope: .all, retained: [], selected: nil, cells: cells)
            .allSatisfy { $0.role != .progress }, "the gauge belongs to P800 only")
    }

    @Test func feedRowsShowTheMarkerTheTitleAndTheChip() {
        let waiting = GardenPage.feedLine(gardenRow(1, "Zed", status: "Dead", action: .bury, needsTriage: true),
                                          selected: false, cells: cells)
        #expect(waiting.text == "● Zed" + spaces(19) + "bury")
        #expect(waiting.cells.spans.first == CellSpan("●", cells: 2, foreground: .yellow))
        #expect(waiting.cells.spans.last == CellSpan(" bury", cells: 5, foreground: .cyan))
        #expect(waiting.cells.spans.allSatisfy { $0.background == .black })
        #expect(!waiting.isSelected)
        #expect(waiting.role == .feed(1))
        #expect(waiting.cells.accessibilityText == "Zed, Dead, bury")

        let kept = GardenPage.feedLine(gardenRow(2, "Old", status: "Fossil", needsTriage: false, decision: "keep"),
                                       selected: false, cells: cells)
        #expect(kept.text == "  Old" + spaces(19) + "kept")
        #expect(kept.cells.spans.last == CellSpan(" kept", cells: 5, foreground: .green))

        let selected = GardenPage.feedLine(gardenRow(1, "Zed", status: "Dead", action: .bury, needsTriage: true),
                                           selected: true, cells: cells)
        #expect(selected.isSelected)
        #expect(selected.cells.spans.allSatisfy { $0.foreground == .black && $0.background == .white })
        #expect(selected.text == waiting.text)
    }

    /// VoiceOver reads the original title; the cells show a `?` for what Bedstead lacks, which
    /// `CellSpan` draws in magenta even in reverse video.
    @Test func missingGlyphsAreReplacedAndVoiceOverReadsTheOriginal() {
        let line = GardenPage.feedLine(gardenRow(3, "Café 🎉", status: "Dead", action: .bury, needsTriage: true),
                                       selected: true, cells: cells)
        #expect(line.text.hasPrefix("● Café ?"))
        #expect(line.cells.width == 28)
        #expect(line.cells.spans[1].cells.runs.contains(CellRun(text: "?", isReplacement: true)))
        #expect(line.cells.accessibilityText == "Café 🎉, Dead, bury")
    }

    @Test func emptyPagesShowOneCentredCyanLine() {
        let queue = GardenPage.entryLines([], scope: .needsTriage, retained: [], selected: nil, cells: cells)
        #expect(queue.map(\.role) == [.title, .progress, .blank, .text])
        #expect(queue.last?.text == spaces(5) + "NOTHING TO TRIAGE" + spaces(6))
        #expect(queue.last?.cells.spans.allSatisfy { $0.foreground == .cyan } == true)
        #expect(queue.last?.cells.accessibilityText == "Nothing to triage")
        let cemetery = GardenPage.entryLines([gardenRow(1, "Alive", status: "Alive")], scope: .cemetery, retained: [],
                                             selected: nil, cells: cells)
        #expect(cemetery.last?.text == spaces(3) + "THE CEMETERY IS EMPTY" + spaces(4))
    }

    @Test func arrowsMoveTheSelectionAndStopAtTheEnds() {
        let ordered = [gardenRow(1, "A", status: "Dead"), gardenRow(2, "B", status: "Dead"), gardenRow(3, "C", status: "Dead")]
        #expect(GardenPage.move(nil, in: ordered, by: 1) == 1)
        #expect(GardenPage.move(nil, in: ordered, by: -1) == 3)
        #expect(GardenPage.move(1, in: ordered, by: 1) == 2)
        #expect(GardenPage.move(1, in: ordered, by: -1) == 1)
        #expect(GardenPage.move(3, in: ordered, by: 1) == 3)
        #expect(GardenPage.move(9, in: ordered, by: 1) == 1)
        #expect(GardenPage.move(9, in: [], by: 1) == 9)
        #expect(!GardenPage.takesArrows(column: .index))
        #expect(GardenPage.takesArrows(column: .entries))
        #expect(GardenPage.takesArrows(column: .reader))
    }

    /// The queue keeps a feed decided during the visit, without its dot and with its decision.
    @Test func theQueueKeepsDecidedFeedsUntilThePageIsLeft() {
        let decided = gardenRow(2, "Decided", status: "Dead", needsTriage: false, decision: "keep")
        let rows = [gardenRow(1, "Waiting", status: "Dead", needsTriage: true), decided]
        let kept = GardenPage.entryLines(rows, scope: .needsTriage, retained: [1, 2], selected: nil, cells: cells)
        #expect(kept.map(\.role).contains(.feed(2)))
        #expect(kept.first { $0.role == .feed(2) }?.text.hasPrefix("  Decided") == true)
        let fresh = GardenPage.entryLines(rows, scope: .needsTriage, retained: [1], selected: nil, cells: cells)
        #expect(!fresh.map(\.role).contains(.feed(2)))
    }
}
```

- [ ] **Step 2: Run red.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenPageTests
```

Expected: build failure, `cannot find 'GardenPage' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/GardenPage.swift`:

```swift
import Foundation

/// The garden pages P800, P801 and P802 as teletext rows (spec §8). Pure functions over
/// `GardenModel` that build phase 8's `CellLine`s (exact widths, reverse video, magenta `?` for
/// what Bedstead lacks, VoiceOver text): the views only draw what these return, so grouping,
/// colours and widths are tested here.
enum GardenPage {
    /// A row of a garden page: a `CellLine`, plus what the page needs on top of it.
    struct Line: Identifiable, Equatable, Sendable {
        enum Role: Equatable, Sendable {
            case title, progress
            /// A status heading; the associated value is the status ("Dead", "Unchecked", "Buried").
            case heading(String)
            /// A feed row, clickable.
            case feed(Int64)
            case text, blank
        }

        var id: String
        var role: Role
        /// The cells as drawn. An empty `accessibilityText` hides the row from VoiceOver (a
        /// wrapped paragraph is read once, from its first row).
        var cells: CellLine
        /// The inspector's status, drawn over two rows by `DoubleHeight` (phase 9).
        var isDoubleHeight = false
        /// Reverse video of the selected feed row.
        var isSelected = false

        /// The characters drawn, `?` replacements included.
        var text: String { cells.spans.map { $0.cells.runs.map(\.text).joined() }.joined() }
    }

    // MARK: Entry column

    /// The title row, the progress row (P800 only), then one heading per status in urgency order,
    /// each followed by its feeds, as `GardenTable` groups them. `retained` is what the queue
    /// keeps after a decision (`seenInQueue ∪ decidedThisVisit`). Every row is `cells` wide
    /// (`EntryPageColumn.cells` in the window).
    static func entryLines(_ rows: [GardenRow], scope: GardenScope, retained: Set<Int64>, selected: Int64?,
                           cells: Int) -> [Line] {
        let visible = GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained)
        let sections = GardenModel.sections(visible, scope: scope)
        var lines = [titleLine(rows, scope: scope, visibleCount: visible.count, cells: cells)]
        if scope == .needsTriage {
            lines.append(progressLine(GardenModel.progress(rows), cells: cells))
        }
        guard !sections.isEmpty else {
            let message = GardenModel.emptyMessage(for: scope)
            var empty = CellLine.centred(message.uppercased(), width: cells, foreground: .cyan)
            empty.accessibilityText = message
            lines.append(blankLine(id: "blank", cells: cells))
            lines.append(Line(id: "empty", role: .text, cells: empty))
            return lines
        }
        for section in sections {
            lines.append(headingLine(section, cells: cells))
            for row in section.rows {
                lines.append(feedLine(row, selected: row.feedId == selected, cells: cells))
            }
        }
        return lines
    }

    /// The feeds in the order the page lists them: what the arrows walk and the selection follows.
    static func orderedRows(_ rows: [GardenRow], scope: GardenScope, retained: Set<Int64>) -> [GardenRow] {
        GardenModel.sections(GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained), scope: scope)
            .flatMap(\.rows)
    }

    /// "NEEDS TRIAGE … 68 LEFT", "ALL FEEDS … 186 FEEDS", "CEMETERY … 1 FEED".
    static func titleLine(_ rows: [GardenRow], scope: GardenScope, visibleCount: Int, cells: Int) -> Line {
        let count: String
        switch scope {
        case .needsTriage: count = "\(GardenModel.progress(rows).left) LEFT"
        case .all, .cemetery: count = visibleCount == 1 ? "1 FEED" : "\(visibleCount) FEEDS"
        }
        let title = GardenModel.title(for: scope)
        let countCells = min(count.count + 1, max(0, cells))
        return Line(id: "title", role: .title, cells: CellLine([
            CellSpan(title.uppercased(), cells: cells - countCells, foreground: .white),
            CellSpan(" " + count, cells: countCells, foreground: .yellow),
        ], accessibilityText: "\(title), \(count.lowercased())"))
    }

    /// P800's mosaic gauge, replacing the native capsule: "▐███░░░ 12/80", feeds triaged over feeds
    /// with a problem status, exactly `cells` wide. Blue is allowed here: the shade is a mosaic.
    static func progressLine(_ progress: GardenModel.Progress, cells: Int) -> Line {
        let done = max(0, progress.total - progress.left)
        let label = " \(done)/\(progress.total)"
        let labelCells = min(label.count, max(0, cells - 1))
        let bar = max(0, cells - 1 - labelCells)
        let filled = progress.total > 0 ? min(bar, bar * done / progress.total) : 0
        let spans = [
            CellSpan("▐", cells: min(1, max(0, cells)), foreground: .cyan),
            CellSpan(String(repeating: "█", count: filled), cells: filled, foreground: .green),
            CellSpan(String(repeating: "░", count: bar - filled), cells: bar - filled, foreground: .blue),
            CellSpan(label, cells: labelCells, foreground: .yellow),
        ]
        return Line(id: "progress", role: .progress,
                    cells: CellLine(spans.filter { $0.cellCount > 0 }, accessibilityText: "\(done) of \(progress.total) triaged"))
    }

    /// A status heading in its teletext colour. Fossil's colour is blue, and blue never colours
    /// text, so its heading is reverse video: white on blue.
    static func headingStyle(for status: String) -> (foreground: TeletextColor, background: TeletextColor) {
        let color = GardenStatusStyle.teletextColor(for: status)
        return color == .blue ? (.white, .blue) : (color, .black)
    }

    /// " DEAD … 8".
    static func headingLine(_ section: GardenModel.Section, cells: Int) -> Line {
        let style = headingStyle(for: section.id)
        let count = " \(section.rows.count)"
        let countCells = min(count.count, max(0, cells))
        return Line(id: "heading-\(section.id)", role: .heading(section.id), cells: CellLine([
            CellSpan(" " + section.id.uppercased(), cells: cells - countCells, foreground: style.foreground,
                     background: style.background),
            CellSpan(count, cells: countCells, foreground: style.foreground, background: style.background),
        ], accessibilityText: section.rows.count == 1 ? "\(section.id), 1 feed" : "\(section.id), \(section.rows.count) feeds"))
    }

    /// "● Title … bury": a yellow dot while the feed needs triage, the title, then the chip (the
    /// proposed action in cyan, or the decision once made in green), as `EntryPageColumn` lays out
    /// an entry. Black on white when selected; a `?` replacement stays magenta (`CellSpan`).
    static func feedLine(_ row: GardenRow, selected: Bool, cells: Int) -> Line {
        let decided = row.lastDecision != nil && !row.needsTriage
        let chip = GardenModel.chip(for: row).map { " " + $0 } ?? ""
        let markerCells = min(2, max(0, cells))
        let chipCells = min(chip.count, max(0, cells - markerCells))
        let spoken = [row.title, GardenModel.status(of: row), GardenModel.chip(for: row)].compactMap { $0 }
        let line = CellLine([
            CellSpan(row.needsTriage ? "●" : "", cells: markerCells, foreground: .yellow),
            CellSpan(row.title, cells: cells - markerCells - chipCells, foreground: .white),
            CellSpan(chip, cells: chipCells, foreground: decided ? .green : .cyan),
        ], accessibilityText: spoken.joined(separator: ", "))
        return Line(id: "feed-\(row.feedId)", role: .feed(row.feedId),
                    cells: selected ? line.reversed(.black, on: .white) : line, isSelected: selected)
    }

    static func blankLine(id: String, cells: Int) -> Line {
        Line(id: id, role: .blank, cells: .blank(max(0, cells)))
    }

    // MARK: Keys

    /// `↑` and `↓` walk the garden list unless the index column is active: its arrows pick pages.
    static func takesArrows(column: AntiopeColumn) -> Bool {
        column != .index
    }

    /// The next (`delta` 1) or previous (-1) feed, stopping at the ends like the native list.
    /// Without a selection, the first (or last) feed.
    static func move(_ selected: Int64?, in ordered: [GardenRow], by delta: Int) -> Int64? {
        guard !ordered.isEmpty else { return selected }
        guard let selected, let index = ordered.firstIndex(where: { $0.feedId == selected }) else {
            return (delta >= 0 ? ordered.first : ordered.last)?.feedId
        }
        return ordered[min(max(index + delta, 0), ordered.count - 1)].feedId
    }
}
```

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: `GardenPageTests` passes, 10 tests, no warning.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/GardenPage.swift apps/macos/RessacTests/AntiopeGardenTests.swift
git commit -m "Antiope garden: entry column rows by status, mosaic progress, reverse-video selection" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 12.2: Inspector page, banner row and Minitel status (pure)

**Files:**
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift`
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`MinitelBar.status`)
- Modify: `apps/macos/RessacTests/AntiopeGardenTests.swift`

**Interfaces:**
- Consumes: Task 12.1's `GardenPage`; `GardenModel.remarkSentence`, `GardenModel.outcome(of:)`, `ProposedAction.label`, `LinkPolicy.webURL(_:)`, `FetchLogRow`, `Hop`, `Banner`, `RunProgress`; `MinitelHint`, `MinitelBar.hintLine(_:width:)` (phase 8).
- Produces: `GardenPage.inspectorLines(_:log:cells:timeZone:)`, `GardenPage.wrap(_:cells:)`, `GardenPage.logDate(_:timeZone:)`, `GardenPage.BannerRow { message; log; dismiss; width }`, `GardenPage.bannerRow(_:cells:)`, `MinitelBar.status(progress:message:)`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeGardenTests.swift`:

```swift
@MainActor @Suite struct GardenInspectorPageTests {
    private let log = [FetchLogRow(id: 1, at: Date(timeIntervalSince1970: 1_791_615_600), httpStatus: 410, errorKind: nil,
                                   redirectChain: [Hop(status: 301, url: "https://www.unphotographable.com/index.rdf")])]

    @Test func theInspectorIsATeletextPage() throws {
        let row = gardenRow(7, "Unphotographable", status: "Ailing", folder: "Blogs / Photo",
                            reason: "The feed is not valid XML.", evidence: ["Parse error at line 3."], remarks: [.httpOnly],
                            needsTriage: true, site: "http://www.unphotographable.com/")
        let lines = GardenPage.inspectorLines(row, log: log, cells: 40, timeZone: .gmt)
        #expect(lines.map { trailingTrimmed($0.text) } == [
            "Unphotographable",
            "Blogs / Photo",
            "",
            " AILING",
            "The feed is not valid XML.",
            "",
            "EVIDENCE",
            "- Parse error at line 3.",
            "",
            "REMARKS",
            "- Only served over http://.",
            "",
            "LINKS",
            "FEED      https://feeds.example.org/7",
            "SITE      http://www.unphotographable.co",
            "          m/",
            "",
            "REDIRECTS",
            "301 https://www.unphotographable.com/ind",
            "    ex.rdf",
            "",
            "FETCH LOG",
            "10 OCT 26 07:00  HTTP 410",
        ])
        for line in lines {
            #expect(line.cells.width == 40, "\(line.id): «\(line.text)»")
        }
        let status = try #require(lines.first { $0.isDoubleHeight })
        #expect(status.role == .heading("Ailing"))
        #expect(status.cells.spans.allSatisfy { $0.foreground == .yellow })
        #expect(status.cells.accessibilityText == "Ailing")
        #expect(lines[13].cells.spans.first == CellSpan("FEED      ", cells: 10, foreground: .cyan))
        #expect(lines[13].cells.accessibilityText == "Feed: https://feeds.example.org/7")
        #expect(lines[15].cells.accessibilityText.isEmpty, "a wrapped value is read once")
        #expect(lines[18].cells.accessibilityText == "301 to https://www.unphotographable.com/index.rdf")
        #expect(lines[22].cells.accessibilityText == "10 OCT 26 07:00, HTTP 410")
    }

    @Test func aBuriedFossilShowsHowToRestoreIt() throws {
        let fossil = gardenRow(8, "Old blog", status: "Fossil", state: "buried", action: .bury, decision: "bury")
        let page = GardenPage.inspectorLines(fossil, log: [], cells: 40, timeZone: .gmt)
        let status = try #require(page.first { $0.isDoubleHeight })
        #expect(status.cells.spans.allSatisfy { $0.foreground == .white && $0.background == .blue })
        let texts = page.map { trailingTrimmed($0.text) }
        #expect(texts.contains("Buried. Press k to restore it."))
        #expect(!texts.contains { $0.hasPrefix("PROPOSED") }, "nothing is proposed for a buried feed")
        #expect(texts.contains("DECISION  Bury"))
        #expect(texts.contains("Not fetched yet."))
    }

    @Test func withoutASelectionTheReaderSaysSo() {
        let none = GardenPage.inspectorLines(nil, log: [], cells: 40)
        #expect(none.map(\.text) == [spaces(40), spaces(12) + "NO FEED SELECTED" + spaces(12)])
        #expect(none.last?.cells.spans.allSatisfy { $0.foreground == .cyan } == true)
        #expect(none.last?.cells.accessibilityText == "No feed selected")
    }

    /// Spec §5: blue is for separators, backgrounds and mosaics, never for text.
    @Test func blueNeverColoursText() {
        let rows = (GardenModel.urgencyOrder + ["Buried"]).enumerated().map {
            gardenRow(Int64($0.offset + 1), $0.element, status: $0.element, state: $0.element == "Buried" ? "buried" : "active",
                      needsTriage: true)
        }
        var lines = GardenPage.entryLines(rows, scope: .all, retained: [], selected: 3, cells: EntryPageColumn.cells)
        for row in rows {
            lines += GardenPage.inspectorLines(row, log: log, cells: 40)
        }
        for line in lines where line.role != .progress {
            #expect(!line.cells.spans.contains { $0.foreground == .blue }, "\(line.id): «\(line.text)»")
        }
    }

    @Test func wrapBreaksOnWordsAndSplitsLongWords() {
        #expect(GardenPage.wrap("The quick brown fox jumps", cells: 10) == ["The quick", "brown fox", "jumps"])
        #expect(GardenPage.wrap("abcdefghijkl xy", cells: 5) == ["abcde", "fghij", "kl xy"])
        #expect(GardenPage.wrap("a\nb", cells: 10) == ["a b"])
        #expect(GardenPage.wrap("", cells: 10).isEmpty)
        #expect(GardenPage.wrap("anything", cells: 0).isEmpty)
    }

    @Test func logDatesAreTeletextCapitals() {
        #expect(GardenPage.logDate(Date(timeIntervalSince1970: 1_791_615_600), timeZone: .gmt) == "10 OCT 26 07:00")
    }
}

@MainActor @Suite struct AntiopeBannerTests {
    @Test func theBannerRowFillsTheWindowWidth() {
        let failure = Banner(message: "ressac exited with code 1: the database is locked.", showLog: true)
        let row = GardenPage.bannerRow(failure, cells: 60)
        #expect(row.width == 60)
        #expect(row.log?.accessibilityText == "Show Log" && row.dismiss.accessibilityText == "Dismiss")
        #expect(row.message.spans.allSatisfy { $0.foreground == .white && $0.background == .red })
        #expect([row.log, row.dismiss].compactMap { $0 }.allSatisfy {
            $0.spans.allSatisfy { $0.foreground == .yellow && $0.background == .red }
        })
        #expect(text(of: row.message.spans[0]).hasPrefix(" ressac exited with code 1"))
        #expect(row.message.accessibilityText == failure.message)

        let network = Banner(message: "The network looks unreachable: 12 of 186 feeds could not connect.", showLog: false)
        let short = GardenPage.bannerRow(network, cells: 40)
        #expect(short.log == nil)
        #expect(short.width == 40)
    }

    /// The search prompt keeps its place ahead of all this (phase 9); then the garden's last
    /// refusal, then the run, then the hints.
    @Test func theMinitelBarShowsRefusalsThenRunsThenItsHints() {
        let run = RunProgress(done: 42, total: 155, label: "Fetching feeds")
        #expect(MinitelBar.status(progress: run, message: nil) == MinitelHint(text: "REFRESHING 42/155", colour: .yellow))
        #expect(MinitelBar.status(progress: RunProgress(done: 0, total: 0, label: "Fetching feeds"), message: nil)?.text
            == "REFRESHING")
        #expect(MinitelBar.status(progress: run, message: "Nothing to repair for this feed.")?.text
            == "Nothing to repair for this feed.")
        #expect(MinitelBar.status(progress: nil, message: nil) == nil)
        #expect(MinitelBar.hintLine(MinitelBar.status(progress: run, message: nil)!, width: 30).width == 30)
    }
}
```

- [ ] **Step 2: Run red.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenInspectorPageTests -only-testing:RessacTests/AntiopeBannerTests
```

Expected: build failure, `type 'GardenPage' has no member 'inspectorLines'` (and `type 'MinitelBar' has no member 'status'`).

- [ ] **Step 3: Implement.** Re-read both files first.

**`apps/macos/Ressac/Antiope/GardenPage.swift`, edit 1.** Replace:

```swift
    // MARK: Keys
```

with:

```swift
    // MARK: Inspector

    /// The garden inspector as a teletext page `cells` wide (spec §8): name and folder path, the
    /// status in double height, then evidence, remarks, links, redirects and the fetch log.
    static func inspectorLines(_ row: GardenRow?, log: [FetchLogRow], cells: Int,
                               timeZone: TimeZone = .current) -> [Line] {
        guard cells > 0 else { return [] }
        var lines: [Line] = []

        func add(_ spans: [CellSpan], role: Line.Role = .text, doubleHeight: Bool = false, spoken: String) {
            lines.append(Line(id: "inspector-\(lines.count)", role: role,
                              cells: CellLine(spans, accessibilityText: spoken), isDoubleHeight: doubleHeight))
        }
        func blank() {
            lines.append(blankLine(id: "inspector-\(lines.count)", cells: cells))
        }
        func heading(_ title: String, _ color: TeletextColor) {
            add([CellSpan(title.uppercased(), cells: cells, foreground: color)], spoken: title)
        }
        /// `text` wrapped on words; `lead` (a label, a date, a dash) starts the first row and the
        /// following rows are indented by its width. Only the first row is read by VoiceOver.
        func paragraph(_ text: String, color: TeletextColor, lead: (text: String, color: TeletextColor)? = nil,
                       spokenLead: String = "") {
            let indent = min(lead?.text.count ?? 0, cells)
            for (index, piece) in wrap(text, cells: cells - indent).enumerated() {
                var spans: [CellSpan] = []
                if let lead {
                    spans.append(CellSpan(index == 0 ? lead.text : "", cells: indent, foreground: lead.color))
                }
                spans.append(CellSpan(piece, cells: cells - indent, foreground: color))
                add(spans, spoken: index == 0 ? spokenLead + text : "")
            }
        }
        func labeled(_ label: String, _ value: String) {
            paragraph(value, color: .white,
                      lead: (label.uppercased().padding(toLength: 10, withPad: " ", startingAt: 0), .cyan),
                      spokenLead: "\(label): ")
        }

        guard let row else {
            blank()
            var none = CellLine.centred("NO FEED SELECTED", width: cells, foreground: .cyan)
            none.accessibilityText = "No feed selected"
            lines.append(Line(id: "inspector-\(lines.count)", role: .text, cells: none))
            return lines
        }

        let status = GardenModel.status(of: row)
        let buried = GardenModel.isBuried(row)
        paragraph(row.title, color: .white)
        if let path = row.folderPath, !path.isEmpty {
            paragraph(path, color: .cyan)
        }
        blank()
        let style = headingStyle(for: status)
        add([CellSpan(" " + status.uppercased(), cells: cells, foreground: style.foreground, background: style.background)],
            role: .heading(status), doubleHeight: true, spoken: status)
        if buried {
            paragraph("Buried. Press k to restore it.", color: .yellow)
        }
        if let reason = row.reason {
            paragraph(reason, color: .white)
        }
        if let bullets = row.evidence?.bullets, !bullets.isEmpty {
            blank()
            heading("Evidence", .green)
            for bullet in bullets {
                paragraph(bullet, color: .white, lead: ("- ", .green))
            }
        }
        if !row.remarks.isEmpty {
            blank()
            heading("Remarks", .yellow)
            for remark in row.remarks {
                paragraph(GardenModel.remarkSentence(remark), color: .white, lead: ("- ", .yellow))
            }
        }
        blank()
        heading("Links", .cyan)
        if let action = row.proposedAction, !buried {
            labeled("Proposed", action.label)
        }
        if let decision = row.lastDecision {
            labeled("Decision", decision.capitalized)
        }
        labeled("Feed", row.xmlUrl)
        if let site = LinkPolicy.webURL(row.htmlUrl) {
            labeled("Site", site.absoluteString)
        }
        if let wayback = LinkPolicy.webURL(row.waybackUrl) {
            labeled("Wayback", wayback.absoluteString)
        }
        if let chain = log.first?.redirectChain, !chain.isEmpty {
            blank()
            heading("Redirects", .magenta)
            for hop in chain {
                paragraph(hop.url, color: .white, lead: ("\(hop.status) ", .magenta), spokenLead: "\(hop.status) to ")
            }
        }
        blank()
        heading("Fetch log", .green)
        if log.isEmpty {
            paragraph("Not fetched yet.", color: .white)
        }
        for entry in log {
            let date = logDate(entry.at, timeZone: timeZone)
            paragraph(GardenModel.outcome(of: entry), color: .white, lead: (date + "  ", .cyan), spokenLead: "\(date), ")
        }
        return lines
    }

    /// Greedy word wrap into rows of at most `cells` characters; a word longer than a row (a URL)
    /// is cut into full rows.
    static func wrap(_ text: String, cells: Int) -> [String] {
        guard cells > 0 else { return [] }
        var rows: [String] = []
        var current = ""
        for piece in text.split(whereSeparator: \.isWhitespace) {
            var word = String(piece)
            while word.count > cells {
                if !current.isEmpty {
                    rows.append(current)
                    current = ""
                }
                rows.append(String(word.prefix(cells)))
                word = String(word.dropFirst(cells))
            }
            if current.isEmpty {
                current = word
            } else if current.count + 1 + word.count <= cells {
                current += " " + word
            } else {
                rows.append(current)
                current = word
            }
        }
        if !current.isEmpty {
            rows.append(current)
        }
        return rows
    }

    /// "10 OCT 26 07:00": fetch log dates, English month in capitals, in `timeZone`.
    static func logDate(_ date: Date, timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "dd MMM yy HH:mm"
        return formatter.string(from: date).uppercased()
    }

    // MARK: Banner

    /// `RunCoordinator`'s banner as one red reverse-video row (spec §8): the message in white,
    /// then the clickable keys in yellow, all on red, exactly the window's width.
    struct BannerRow: Equatable, Sendable {
        var message: CellLine
        /// ` LOG `, when the log explains the failure.
        var log: CellLine?
        /// ` X `, to dismiss.
        var dismiss: CellLine

        var width: Int { message.width + (log?.width ?? 0) + dismiss.width }
    }

    static func bannerRow(_ banner: Banner, cells: Int) -> BannerRow {
        func key(_ label: String, spoken: String) -> CellLine {
            CellLine([CellSpan(label, cells: label.count, foreground: .yellow, background: .red)], accessibilityText: spoken)
        }
        let log = banner.showLog ? key(" LOG ", spoken: "Show Log") : nil
        let dismiss = key(" X ", spoken: "Dismiss")
        let messageCells = max(0, cells - (log?.width ?? 0) - dismiss.width)
        return BannerRow(message: CellLine([CellSpan(" " + banner.message, cells: messageCells, foreground: .white,
                                                     background: .red)],
                                           accessibilityText: banner.message),
                         log: log, dismiss: dismiss)
    }

    // MARK: Keys
```

**`apps/macos/Ressac/Antiope/MinitelBar.swift`, edit 1.** Replace:

```swift
    /// The hint after one blank cell, exactly `width` cells.
    static func hintLine(_ hint: MinitelHint, width: Int) -> CellLine {
```

with:

```swift
    /// What replaces the hints (spec §6): the garden's last refusal ("Nothing to repair for this
    /// feed."), else "REFRESHING 42/155" while a run is going, in yellow. Nil keeps the hints.
    static func status(progress: RunProgress?, message: String?) -> MinitelHint? {
        if let message { return MinitelHint(text: message, colour: .yellow) }
        guard let progress else { return nil }
        let text = progress.total > 0 ? "REFRESHING \(progress.done)/\(progress.total)" : "REFRESHING"
        return MinitelHint(text: text, colour: .yellow)
    }

    /// The hint after one blank cell, exactly `width` cells.
    static func hintLine(_ hint: MinitelHint, width: Int) -> CellLine {
```

- [ ] **Step 4: Run green.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/GardenPageTests -only-testing:RessacTests/GardenInspectorPageTests -only-testing:RessacTests/AntiopeBannerTests -only-testing:RessacTests/MinitelBarTests
```

Expected: 21 tests pass (10 + 6 + 2 + phase 8's 3), no warning.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/GardenPage.swift apps/macos/Ressac/Antiope/MinitelBar.swift apps/macos/RessacTests/AntiopeGardenTests.swift
git commit -m "Antiope garden: inspector page, banner row and Minitel status rows" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 12.3: Garden store and page views

**Files:**
- Create: `apps/macos/Ressac/Antiope/GardenPageViews.swift`
- Modify: `apps/macos/RessacTests/AntiopeGardenTests.swift`

**Interfaces:**
- Consumes: `GardenPage`, `GardenModel`, `GardenCommand`, `AppState`, `AntiopeState` (`column`, `isSearchPromptOpen`, `handle(_:)`), `Queries.garden(scope:)`, `Queries.fetchLog(feedId:)`, `TeletextFont`, `\.teletextSize`, `CellLineView`, `DoubleHeight`, `EntryPageColumn.cells`, `DatabaseLocation.logsURL`.
- Produces: `AntiopeGardenStore` (`rows`, `log`, `message`, `confirmingSafeRepairs`, `retained(_:)`, `selectedRow(_:)`, `observe(page:app:)`, `observeLog(feedId:app:)`, `perform(_:app:)`), `GardenEntryPage(scope:)`, `GardenInspectorPage()`, `AntiopeBannerRow(banner:dismiss:)`, `GardenLineView(line:)`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeGardenTests.swift`:

```swift
@MainActor @Suite struct AntiopeGardenStoreTests {
    /// Two feeds needing triage in the contract database: Moved (feed 3) comes before Dead (feed 2).
    private func diagnose(_ test: TestDatabase) async throws {
        try await test.raw.write { db in
            try db.execute(sql: """
                INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, computed_at, needs_triage)
                VALUES (2, 'Dead', 'Gone.', '{"bullets":[],"clue":"","language":null}', '[]', '{"type":"bury"}',
                        '2026-10-10T08:00:00.000Z', 1),
                       (3, 'Moved', 'Moved.', '{"bullets":[],"clue":"301","language":null}', '[]',
                        '{"type":"follow_redirect","url":"https://moved.example.org/feed"}', '2026-10-10T08:00:00.000Z', 1);
                """)
        }
    }

    @Test func enteringTheQueueSelectsTheFirstFeedToTriage() async throws {
        let (state, test, _) = try makeState()
        try await diagnose(test)
        state.selection = .garden(.needsTriage)
        let store = AntiopeGardenStore()
        let observation = Task { await store.observe(page: .needsTriage, app: state) }
        defer { observation.cancel() }
        #expect(await eventually { store.rows.contains { $0.feedId == 3 } })
        #expect(await eventually { state.selectedFeedId == 3 })
        #expect(store.selectedRow(state)?.status == "Moved")
        #expect(store.message == nil)
    }

    @Test func gardenCommandsActAsInTheNativeTable() async throws {
        let (state, test, stub) = try makeState()
        try await diagnose(test)
        var opened: [URL] = []
        state.openURL = { opened.append($0) }
        state.selection = .garden(.needsTriage)
        state.selectedFeedId = 3
        let store = AntiopeGardenStore()

        store.perform(.message("Nothing to repair for this feed."), app: state)
        #expect(store.message == "Nothing to repair for this feed.")
        store.perform(.confirmSafeRepairs, app: state)
        #expect(store.confirmingSafeRepairs)
        #expect(store.message == nil, "a new key clears the last refusal")
        store.perform(.open(URL(string: "https://example.org/")!), app: state)
        #expect(opened == [URL(string: "https://example.org/")!])
        store.perform(.decide(.keep), app: state)
        #expect(await eventually { (try? calls(stub))?.contains { $0.hasSuffix("garden decide 3 keep") } == true })
        store.perform(.undo, app: state)
        #expect(await eventually { (try? calls(stub))?.contains { $0.hasSuffix("garden undo") } == true })
    }

    /// The garden keys reach `GardenEntryPage` only if neither Antiope nor `AppState` takes them
    /// first: `k` is Keep here, not RETOUR (phase 7's `AntiopeState.handle` must leave it).
    @Test func readingCommandsFallThroughInTheGarden() throws {
        let (state, _, _) = try makeState()
        let antiope = AntiopeState(app: state)
        state.selection = .garden(.needsTriage)
        for command in [ReadingCommand.previousEntry, .nextEntry, .advance, .toggleRead, .toggleStar] {
            #expect(!antiope.handle(command), "\(command)")
            #expect(!state.perform(command), "\(command)")
        }
    }

    /// The glyphs the garden pages draw outside `CellText` must exist in Bedstead.
    @Test(.enabled(if: TeletextFont.isAvailable)) func bedsteadDrawsTheGardenGlyphs() {
        for glyph: Character in ["▐", "█", "░", "●"] {
            #expect(TeletextFont.supports(glyph), "\(glyph)")
        }
    }
}
```

- [ ] **Step 2: Run red.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/AntiopeGardenStoreTests
```

Expected: build failure, `cannot find 'AntiopeGardenStore' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/GardenPageViews.swift`:

```swift
import GRDB
import SwiftUI

/// The garden pages' shared state (spec §8): the rows both columns draw, the queue's memory, the
/// fetch log of the selected feed and the last refusal. `AntiopeRootView` owns one;
/// `GardenEntryPage` drives the observations, `GardenInspectorPage` and `MinitelBar` read it.
/// The selection logic and the command dispatch are `GardenTable`'s, over `GardenModel`.
@MainActor @Observable final class AntiopeGardenStore {
    /// `Queries.garden(scope: .all)` (active feeds) and `.cemetery` (buried ones).
    private(set) var active: [GardenRow] = []
    private(set) var buried: [GardenRow] = []
    /// Feeds seen needing triage during this visit: once decided (here, by ⇧R or in the
    /// terminal) they stay listed until the page is left.
    private(set) var seenInQueue: Set<Int64> = []
    private(set) var log: [FetchLogRow] = []
    /// The last refusal ("Nothing to repair for this feed."), shown in the Minitel bar.
    var message: String?
    var confirmingSafeRepairs = false

    var rows: [GardenRow] { active + buried }

    func retained(_ app: AppState) -> Set<Int64> {
        seenInQueue.union(app.decidedThisVisit)
    }

    func selectedRow(_ app: AppState) -> GardenRow? {
        rows.first { $0.feedId == app.selectedFeedId }
    }

    /// Starts a visit of the page `scope` and observes the lists it needs until the task is
    /// cancelled (the page changes or disappears).
    func observe(page scope: GardenScope, app: AppState) async {
        active = []
        buried = []
        seenInQueue = []
        message = nil
        await withDiscardingTaskGroup { group in
            if scope != .cemetery {
                group.addTask { await self.follow(.all, page: scope, app: app) }
            }
            group.addTask { await self.follow(.cemetery, page: scope, app: app) }
        }
    }

    func observeLog(feedId: Int64?, app: AppState) async {
        log = []
        guard let feedId else { return }
        do {
            for try await fresh in Queries.fetchLog(feedId: feedId).values(in: app.database.reader) {
                log = fresh
            }
        } catch {
            log = []
        }
    }

    /// Carries out a key's command exactly as `GardenTable.handle` does.
    func perform(_ command: GardenCommand, app: AppState) {
        message = nil
        switch command {
        case .decide(let decision): app.gardenDecide(decision)
        case .undo: app.gardenUndo()
        case .open(let url): app.openURL(url)
        case .confirmSafeRepairs: confirmingSafeRepairs = true
        case .message(let text): message = text
        }
    }

    private func follow(_ query: GardenScope, page scope: GardenScope, app: AppState) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: app.database.reader) {
                let previous = rows
                if query == .cemetery {
                    buried = fresh
                } else {
                    active = fresh
                }
                rowsDidChange(previous: previous, scope: scope, app: app)
            }
        } catch {
            if !Task.isCancelled {
                message = "The garden could not be read."
            }
        }
    }

    /// Keeps the queue's memory and moves the selection on when the selected feed was decided
    /// (`GardenTable.rowsDidChange`).
    private func rowsDidChange(previous: [GardenRow], scope: GardenScope, app: AppState) {
        seenInQueue.formUnion(rows.filter(\.needsTriage).map(\.feedId))
        let ordered = GardenPage.orderedRows(rows, scope: scope, retained: retained(app))
        if scope == .needsTriage {
            app.selectedFeedId = GardenModel.selectionAfterUpdate(
                selected: app.selectedFeedId, previous: previous, current: rows, ordered: ordered)
        }
        if !ordered.contains(where: { $0.feedId == app.selectedFeedId }) {
            app.selectedFeedId = ordered.first(where: \.needsTriage)?.feedId ?? ordered.first?.feedId
        }
    }
}

/// The entry column of P800, P801 and P802 (spec §8): the feeds by status under coloured headings,
/// the mosaic progress row, and the terminal garden's keys.
struct GardenEntryPage: View {
    let scope: GardenScope

    @Environment(AppState.self) private var state
    @Environment(AntiopeState.self) private var antiope
    @Environment(AntiopeGardenStore.self) private var store
    @Environment(\.teletextSize) private var size
    @FocusState private var focused: Bool

    var body: some View {
        @Bindable var store = store
        let cell = TeletextFont.cell(forSize: size)
        let lines = GardenPage.entryLines(store.rows, scope: scope, retained: store.retained(state),
                                          selected: state.selectedFeedId, cells: EntryPageColumn.cells)
        let safeRepairs = GardenModel.safeRepairs(store.rows)

        ScrollViewReader { proxy in
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        GardenLineView(line: line)
                            .contentShape(Rectangle())
                            .onTapGesture { select(line) }
                    }
                }
            }
            .scrollIndicators(.never)
            .onChange(of: state.selectedFeedId) { _, feedId in
                guard let feedId else { return }
                proxy.scrollTo("feed-\(feedId)")
            }
        }
        .frame(width: Double(EntryPageColumn.cells) * cell.width)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(TeletextColor.black.color)
        .focusable(interactions: .edit)
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(phases: .down) { press in
            handle(press, safeRepairCount: safeRepairs.count)
        }
        .background { undoShortcut }
        .confirmationDialog(GardenModel.applyQuestion(safeRepairs.count), isPresented: $store.confirmingSafeRepairs,
                            titleVisibility: .visible) {
            Button(safeRepairs.count == 1 ? "Apply" : "Apply All") { state.gardenApplySafe() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(GardenModel.safeRepairPreview(safeRepairs))
        }
        .task(id: scope) {
            focused = !antiope.isSearchPromptOpen
            await store.observe(page: scope, app: state)
        }
        .onChange(of: antiope.isSearchPromptOpen) { _, open in
            focused = !open
        }
        .onDisappear {
            store.message = nil
        }
    }

    /// ⌘Z undoes the last garden decision, like the native table's Undo button.
    private var undoShortcut: some View {
        Button("Undo Garden Decision") { store.perform(.undo, app: state) }
            .keyboardShortcut("z", modifiers: .command)
            .frame(width: 0, height: 0)
            .opacity(0)
            .accessibilityHidden(true)
    }

    /// The terminal garden's keys through `GardenModel.command`, as `GardenTable.handle`; `↑`/`↓`
    /// walk the list unless the index column is active.
    private func handle(_ press: KeyPress, safeRepairCount: Int) -> KeyPress.Result {
        if press.key == .downArrow || press.key == .upArrow {
            guard GardenPage.takesArrows(column: antiope.column) else { return .ignored }
            let ordered = GardenPage.orderedRows(store.rows, scope: scope, retained: store.retained(state))
            state.selectedFeedId = GardenPage.move(state.selectedFeedId, in: ordered, by: press.key == .downArrow ? 1 : -1)
            return .handled
        }
        guard let command = GardenModel.command(for: press.characters, modifiers: press.modifiers,
                                                row: store.selectedRow(state), safeRepairCount: safeRepairCount)
        else { return .ignored }
        store.perform(command, app: state)
        return .handled
    }

    private func select(_ line: GardenPage.Line) {
        guard case let .feed(feedId) = line.role else { return }
        state.selectedFeedId = feedId
        antiope.column = .entries
        focused = true
    }
}

/// The reader column of the garden pages: the selected feed's inspector as a teletext page.
struct GardenInspectorPage: View {
    @Environment(AppState.self) private var state
    @Environment(AntiopeGardenStore.self) private var store
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let lines = GardenPage.inspectorLines(store.selectedRow(state), log: store.log,
                                                  cells: Int(proxy.size.width / cell.width))
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        GardenLineView(line: line)
                    }
                }
            }
            .scrollIndicators(.never)
        }
        .background(TeletextColor.black.color)
        .task(id: state.selectedFeedId) {
            await store.observeLog(feedId: state.selectedFeedId, app: state)
        }
    }
}

/// `RunCoordinator`'s banner as a red reverse-video row under the page header (spec §8).
struct AntiopeBannerRow: View {
    let banner: Banner
    let dismiss: () -> Void

    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let row = GardenPage.bannerRow(banner, cells: Int(proxy.size.width / cell.width))
            HStack(spacing: 0) {
                CellLineView(line: row.message)
                if let log = row.log {
                    Button {
                        NSWorkspace.shared.open(DatabaseLocation.logsURL)
                    } label: {
                        CellLineView(line: log)
                    }
                    .buttonStyle(.plain)
                }
                Button(action: dismiss) {
                    CellLineView(line: row.dismiss)
                }
                .buttonStyle(.plain)
            }
        }
        .frame(height: cell.height)
        .background(TeletextColor.red.color)
        .help(banner.message)
    }
}

/// One row of a garden page: phase 8's `CellLineView`, or phase 9's `DoubleHeight` for the
/// inspector's status. A row with nothing to read is hidden from VoiceOver.
struct GardenLineView: View {
    let line: GardenPage.Line

    var body: some View {
        Group {
            if line.isDoubleHeight {
                DoubleHeight(line: line.cells)
            } else {
                CellLineView(line: line.cells)
            }
        }
        .accessibilityHidden(line.cells.accessibilityText.isEmpty)
        .accessibilityAddTraits(line.isSelected ? .isSelected : [])
    }
}
```

- [ ] **Step 4: Run green.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/AntiopeGardenStoreTests
```

Expected: 4 tests pass (the Bedstead test runs because the test host registers the font; if it reports "skipped", phase 3's registration is broken: stop and report). If `readingCommandsFallThroughInTheGarden` fails on `antiope.handle(.previousEntry)`, phase 7's `AntiopeState.handle` consumes RETOUR in the garden: stop and report it to Clément; the fix belongs to phase 7's file (return false for `.nextEntry`, `.previousEntry`, `.advance`, `.toggleRead`, `.toggleStar` when `app.selection.isGarden`), not to this phase.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/GardenPageViews.swift apps/macos/RessacTests/AntiopeGardenTests.swift
git commit -m "Antiope garden: shared garden store, entry and inspector pages, banner row" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 12.4: Wire the garden pages, the banner and the run status into the Antiope window

**Files:**
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift`
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift`

**Interfaces:**
- Consumes: `AntiopeGardenStore`, `GardenEntryPage(scope:)`, `GardenInspectorPage()`, `AntiopeBannerRow(banner:dismiss:)`, `MinitelBar.status(progress:message:)`; `AntiopeRootView` as phases 8 and 9 leave it, `MinitelBar` as phase 9 leaves it (with Task 12.2's `status`).
- Produces: the finished garden pages in the window; no new name.

- [ ] **Step 1: Re-read both files** (Edit Safety). Each block below is the exact text phases 8 and 9 leave; phases 10 and 11 do not touch these two files.

- [ ] **Step 2: The root view.** Five replacements in `AntiopeRootView.swift`: the store, the banner row in the place phase 8 marked for it (spec §5: header, optional banner row, columns, Minitel bar), the garden pages in the entry and reader columns (P100 and P899 keep showing over the garden, as over any page), and the store in the environment.

**`apps/macos/Ressac/Antiope/AntiopeRootView.swift`, edit 1.** Replace:

```swift
    @State private var antiope: AntiopeState
```

with:

```swift
    @State private var antiope: AntiopeState
    /// Shared by the garden pages and the Minitel bar (phase 12).
    @State private var garden = AntiopeGardenStore()
```

**`apps/macos/Ressac/Antiope/AntiopeRootView.swift`, edit 2.** Replace:

```swift
                PageHeader(width: width)
                // Phase 12 puts the banner row here (spec §8, Banners).
```

with:

```swift
                PageHeader(width: width)
                if let banner = state.runs.banner {
                    AntiopeBannerRow(banner: banner) { state.runs.banner = nil }
                }
```

**`apps/macos/Ressac/Antiope/AntiopeRootView.swift`, edit 3.** Replace:

```swift
    /// The entry list. In a Garden scope the native Garden table stays here until phase 12 draws
    /// P800–P802.
    @ViewBuilder private var entriesColumn: some View {
        if case let .garden(scope) = state.selection {
            GardenTable(state: state, scope: scope)
        } else {
```

with:

```swift
    /// The entry list, or P800–P802 in a Garden scope.
    @ViewBuilder private var entriesColumn: some View {
        if case let .garden(scope) = state.selection {
            GardenEntryPage(scope: scope)
        } else {
```

**`apps/macos/Ressac/Antiope/AntiopeRootView.swift`, edit 4.** Replace:

```swift
    /// P100 SOMMAIRE, P899 GUIDE, or else the selected entry (`ReaderView`, themed by phase 10) or
    /// the Garden's inspector (redrawn by phase 12).
    @ViewBuilder private var readerColumn: some View {
        switch antiope.readerPage {
        case .article:
            if case let .garden(scope) = state.selection {
                GardenInspector(state: state, scope: scope)
            } else {
```

with:

```swift
    /// P100 SOMMAIRE, P899 GUIDE, or else the selected entry (`ReaderView`, themed by phase 10) or,
    /// in a Garden scope, the selected feed's inspector page.
    @ViewBuilder private var readerColumn: some View {
        switch antiope.readerPage {
        case .article:
            if state.selection.isGarden {
                GardenInspectorPage()
            } else {
```

**`apps/macos/Ressac/Antiope/AntiopeRootView.swift`, edit 5.** Replace:

```swift
        .environment(antiope)
```

with:

```swift
        .environment(antiope)
        .environment(garden)
```

- [ ] **Step 3: The Minitel bar.** The search prompt keeps its place ahead of everything (phase 9); in the hint line, the status of Task 12.2 replaces the hints while it has something to say.

**`apps/macos/Ressac/Antiope/MinitelBar.swift`, edit 2.** Replace:

```swift
    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        HStack(spacing: 0) {
```

with:

```swift
    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope
    @Environment(AntiopeGardenStore.self) private var garden

    var body: some View {
        HStack(spacing: 0) {
```

**`apps/macos/Ressac/Antiope/MinitelBar.swift`, edit 3.** Replace:

```swift
            } else {
                CellLineView(line: Self.hintLine(
                    Self.hint(column: antiope.column, isGarden: app.selection.isGarden, searchText: app.searchText),
                    width: max(0, width - Self.keysCells)))
            }
```

with:

```swift
            } else {
                CellLineView(line: Self.hintLine(
                    Self.status(progress: app.runs.progress, message: app.selection.isGarden ? garden.message : nil)
                        ?? Self.hint(column: antiope.column, isGarden: app.selection.isGarden, searchText: app.searchText),
                    width: max(0, width - Self.keysCells)))
            }
```

- [ ] **Step 4: Build.**

```bash
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' build
```

Expected: `BUILD SUCCEEDED`, no warning in our sources. A runtime trap "No Observable object of type AntiopeGardenStore found" means a view reading the store sits outside the subtree that received `.environment(garden)`; edit 5 puts it on the outermost view of the body, next to `.environment(antiope)`, which covers the header, the banner row, the columns and the Minitel bar.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/AntiopeRootView.swift apps/macos/Ressac/Antiope/MinitelBar.swift
git commit -m "Antiope: garden pages in the window, banner row, REFRESHING in the Minitel bar" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 12.5: Verification gate and push

- [ ] **Step 1: Gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | tee "$TMPDIR/ressac-phase12.log" | tail -n 5
grep -E '^/.+/apps/macos/.+: (warning|error):' "$TMPDIR/ressac-phase12.log" || echo "no warnings in our sources"
```

Expected: fmt and clippy silent, every Rust test passes, `** TEST SUCCEEDED **`, and `no warnings in our sources`. The Swift suites added here: `GardenPageTests` (10), `GardenInspectorPageTests` (6), `AntiopeBannerTests` (2), `AntiopeGardenStoreTests` (4). The native `GardenTests` still pass unchanged: this phase reuses `GardenModel` without modifying it.

- [ ] **Step 2: Check the scope.** `git diff --stat main...HEAD -- apps/macos/Ressac/Garden apps/macos/Ressac/App` shows nothing from this phase (the native garden and `AppState` are untouched), and `git show --stat HEAD~3..HEAD` lists only the five files of this phase.

- [ ] **Step 3: Push.**

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push -u origin antiope
```

Then stop: report to Clément and wait for his approval before phase 13.

## Notes for the executor

- **Verified ahead of time (14/10/2026).** Every block of this phase was applied, in order, to a scratch copy with phases 6, 3, 4, 5, 7 to 11 applied from their files, built with warnings as errors, and its suites run.
- **Why the garden page holds SwiftUI focus.** In Antiope the router decides with `AntiopeState.column`, not with the responder, so the native trick (the Garden table is an `NSTableView` with the keyboard) does not exist. The page takes `.focusable(interactions: .edit)` focus, which works whatever the Keyboard Navigation setting, and gives it back while the search prompt is open. Letters work in every column, like the native window. In the index column phase 7's router already turns `↑`/`↓` into `.nextIndexRow`/`.previousIndexRow`, so they never reach the page; `GardenPage.takesArrows` is the page's own guard for the same rule.
- **`k` in the garden** is Keep, never RETOUR, because in a garden scope `AppState.perform(.previousEntry)` returns false (unchanged since phase 12 of the app) and phase 7 does not consume it.
- **Dimming:** teletext forbids the native 45 % opacity (no colour outside the palette), so decided rows lose the yellow `●` and show their decision chip in green. The native rule (rows stay until the page is left, selection moves to the next feed needing triage) is unchanged, through `GardenModel` and `AntiopeGardenStore.rowsDidChange`.
- **Double height** is phase 9's `DoubleHeight`, a vertical scale of one `CellLineView` row into a two-row frame, which stays sharp on a vector glyph; phase 14's manual check item 7 looks at it at every size.
- **Bedstead coverage** of `▐ █ ░ ●` is asserted by `bedsteadDrawsTheGardenGlyphs`. If a glyph is missing, `CellSpan` would draw it as a magenta `?`: replace it in `GardenPage.progressLine`/`feedLine` by a sextant mosaic from U+1FB00–1FB3B (spec §2 lists them as present), and update the expected strings of `progressRowIsExactlyTheColumnWidth` and `feedRowsShowTheMarkerTheTitleAndTheChip`.
- **The gauge's blue shade** is a mosaic, not text: `blueNeverColoursText` skips the `.progress` row on purpose.
