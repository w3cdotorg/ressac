# Phase 6: Antiope, P104 JOURNAL and the rhythm pages P105 to P109

**Runs after:** phase 5c (6a, then 6b, then 6c).

Read the plan index first (`docs/plans/2026-10-16-cadence-edition.md`): Global Constraints (Language: `FIN DE L'EDITION` is the only French string besides the Minitel keys; Exact strings; Reserved pages), Review Focus #5 (this phase owns it), and "Interfaces between phases" (Phases 4a, 5a, 5b, 6). Spec §8 (Antiope), §9 (network unreachable: RÉPÉTITION retries), §10 (Swift: Antiope). The Antiope spec (`docs/specs/2026-10-14-antiope-design.md`) §5 to §8 still applies: every row is exactly its column's width, blue never colours text.

**Sub-phases:**

| Sub-phase | Deliverable | Files |
|---|---|---|
| 6a | `ReservedPage` 104 to 109, their destinations, titles and counts; SOMMAIRE and the index list them | 3 |
| 6b | P104 entry column (header, coloured headings, entries, overflow rows, end line, progress and offline rows), `FIN DE L'EDITION` page reached by SUITE, RÉPÉTITION retries, `EDITION IN PROGRESS n/N` in the Minitel bar | 5 |
| 6c | Sub-pages `104 n/N` in the reader, the rhythm line and the override rows in the Antiope inspector | 5 |

The plan index lists 6a and 6b only; 6b is split in two (6b, 6c) to keep each sub-phase at five files. Nothing else changes.

**Builds on (exact names from the plan index; another phase owns each):**
- 4a: `CadenceGroup` (`title`, `page`, `rawValue`, `allCases`), `EditionSection` (`title`, `antiopeTitle`, `rawValue`, `allCases` in the order front, week, wire), `Edition` (`info`, `items`, `overflow`, `readCount`, `isFinished`), `EditionInfo`, `EditionItem`, `EditionOverflowItem`, `CadenceRow`, `Queries.cadenceRows()`; the v4 contract fixture (tables `edition`, `edition_entry`, `edition_overflow`).
- 4b: `RunRequest.buildEdition(rebuild:)` (kind `.long`), `RunRequest.setCadence(feedId:_:)`.
- 5a: `AppState.journal: JournalModel` with `phase: JournalModel.Phase` (`.idle`, `.due`, `.building(done:total:)`, `.offline`, `.failed(String)`), `edition: Edition?` (the latest edition, observed live), `retry() async`.
- 5b: `Source.journal`, `Source.cadence(CadenceGroup)`; `SidebarSnapshot.journalUnread: Int` and `SidebarSnapshot.cadenceUnread: [CadenceGroup: Int]` (both defaulted, so `pagedSnapshot()` keeps compiling); **for `.journal`, `AppState.entries` holds the edition's entries in `rank` order**, so `j`, `k` and Space walk the edition in order (spec §8: "SUITE and RETOUR walk the edition in order"). Task 6b.3 checks this contract; if 5b ordered them otherwise, stop and report before changing 5b's code.
- 5c: `GardenModel.rhythmText(_ row: CadenceRow, now: Date) -> String`, the native inspector line without its `Rhythm: ` prefix (`Daily · median 0.4 d · last entry 2 h ago`, plus ` (set by you)` when overridden). Antiope reuses it so both looks say the same thing.
- Existing Antiope code (sub-project 4): `PageDirectory`, `ReservedPage`, `PageDestination`, `AntiopeState` (`readerPage`, `currentPage`, `jump(to:)`, `handle(_:)`), `ReaderPage`, `PageIndexColumn.line(...)`, `EntryPageColumn` (`cells`, `wrap(_:width:)`, `emptyLine(_:width:)`), `SommairePage`, `CellLine` (`centred`, `blank`, `reversed`), `CellSpan`, `CellLineView`, `DoubleHeight`, `TeletextDateFormat`, `MinitelBar.status(progress:message:)`, `GardenPage` (`Line`, `inspectorLines`), `GardenInspectorPage`, `SubpageLabel`; test helpers `text(of:)` (AntiopeFrameTests), `pagedSnapshot()` (PageDirectoryTests), `makeState()`, `eventually`, `TestDatabase.entry/execute`.

**Test command** (narrow it with one `-only-testing:` per suite):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/PageDirectoryTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

**Decisions:**
- **Overflow rows are white on blue**, as the Fossil heading of P800 (`GardenPage.headingStyle`): blue never colours text in Antiope, so "blue rows" means a blue background. They start two cells in, under the entry titles.
- **Feed titles keep their accents** in capitals (`+12 MORE IN ACTUALITTÉ`): Antiope has no accent folding and Bedstead draws every French capital. The plan index's `ACTUALITTE` was written without the accent; the code follows `String.uppercased()`, as every other Antiope title does.
- **`FIN DE L'EDITION`** shows twice, in double height and yellow: at the end of the P104 entry column once every entry is read (as the native `That's the edition.`), and as a page of its own in the reader column, `ReaderPage.endOfEdition`, which SUITE reaches from the edition's last entry. `j` lands there at once; Space first scrolls the article, as everywhere. RETOUR (`k`) brings the last entry back; SUITE on that page does nothing. VoiceOver reads `That's the edition.`
- **Overflow rows open by click** (they jump to the feed's page, or select the feed when it has no number yet). The spec's `↩` is not wired: rows of the entry column are not keyboard-selectable one by one, only entries are, and `⌘↩` already opens the article in the browser. Reported in the hand-off.
- **While the edition builds**, P104's entry column shows only `EDITION IN PROGRESS 34/156`, and the Minitel bar shows the same text in place of `REFRESHING 34/156`. **Offline**, P104 shows `OFFLINE: NO EDITION YET` (black on red) and `RÉPÉTITION: RETRY`; RÉPÉTITION on P104 calls `journal.retry()` instead of refreshing. A failed build shows its message the same way.
- **The searching page number** during the build needs no code: `PageHeader` rolls it for every long run, and `RunRequest.buildEdition` is a long run (4b). A test pins that dependency.
- **The inspector override** is six clickable rows under the rhythm line (`AUTO`, `DAILY`, `WEEKLY`, `MONTHLY`, `RARE`, `SILENT`), the choice in force in reverse video, black on cyan: a SwiftUI `Menu` would draw native chrome inside the teletext page. A click runs `ressac feed cadence` through `RunCoordinator.start`; a refusal shows in the existing red banner row.

---

## Sub-phase 6a: reserved pages 104 to 109

**Files (3):**
- Modify: `apps/macos/Ressac/Antiope/PageDirectory.swift` (`ReservedPage`)
- Modify: `apps/macos/RessacTests/PageDirectoryTests.swift`
- Modify: `apps/macos/RessacTests/AntiopePagesTests.swift`

### Task 6a.1: `ReservedPage` gains the Journal and the five rhythm groups

**Interfaces:**
- Consumes: `Source.journal`, `Source.cadence(_:)`, `CadenceGroup`, `SidebarSnapshot.journalUnread`, `SidebarSnapshot.cadenceUnread`.
- Produces: `ReservedPage.journal`, `.daily`, `.weekly`, `.monthly`, `.rare`, `.silent` (raw values 104 to 109); `ReservedPage.cadenceGroup: CadenceGroup?`. `PageDirectory` needs no change: it already lists every `ReservedPage` first and maps each `.source` destination back to its number, so `directory.number(for: .journal) == 104` and `AntiopeState.currentPage` follow.

- [ ] **Step 1: Write the failing tests.** In `apps/macos/RessacTests/PageDirectoryTests.swift`, replace the two tests `reservedPagesComeFirstWithTheirCounts` and `foldersAndFeedsFollowInDisplayOrderWithTheirDepth` by:

```swift
    @Test func reservedPagesComeFirstWithTheirCounts() {
        var snapshot = pagedSnapshot()
        snapshot.journalUnread = 43
        snapshot.cadenceUnread = [.daily: 34, .weekly: 7, .monthly: 2]
        let rows = PageDirectory(snapshot: snapshot).rows
        #expect(rows.prefix(14).map(\.number) == [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899])
        #expect(rows.prefix(14).map(\.title) == [
            "Index", "Unread", "Today", "Starred", "Journal", "Daily", "Weekly", "Monthly", "Rare", "Silent",
            "Needs triage", "All feeds", "Cemetery", "Guide",
        ])
        #expect(rows.prefix(14).map(\.unread) == [0, 10, 1, 4, 43, 34, 7, 2, 0, 0, 7, 0, 0, 0])
        #expect(rows.prefix(14).allSatisfy { $0.isReserved && $0.depth == 0 })
    }

    @Test func foldersAndFeedsFollowInDisplayOrderWithTheirDepth() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows.dropFirst(14)
        #expect(rows.map(\.number) == [200, 201, 400, 401, 402, 403, 110])
        #expect(rows.map(\.title) == [
            "Blogs", "xkcd.com", "Infos", "Daring Fireball", "Science", "Center for Humane Technology", "Loose feed",
        ])
        #expect(rows.map(\.depth) == [0, 1, 0, 1, 1, 2, 0])
        #expect(rows.map(\.unread) == [3, 3, 3, 2, 1, 1, 0])  // folders sum their subtree
        #expect(rows.allSatisfy { !$0.isReserved })
    }

    /// The app and `ressac page list` agree (core `RESERVED_PAGES`, schema v4), and each rhythm
    /// page is its group's (`CadenceGroup.page`, phase 4a).
    @Test func reservedPagesMatchTheCoreAndTheRhythmGroups() {
        #expect(ReservedPage.allCases.map(\.rawValue) == [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899])
        for group in CadenceGroup.allCases {
            let page = ReservedPage(rawValue: group.page)
            #expect(page?.cadenceGroup == group)
            #expect(page?.title == group.title)
            #expect(page?.destination == .source(.cadence(group)))
            #expect(page?.isGarden == false)
        }
        #expect(ReservedPage.journal.cadenceGroup == nil && ReservedPage.unread.cadenceGroup == nil)
        #expect(ReservedPage.journal.destination == .source(.journal) && !ReservedPage.journal.isGarden)
    }
```

In `numbersLeadToTheirDestinations`, after the line `#expect(directory.destination(for: 103) == .source(.starred))`, add:

```swift
        #expect(directory.destination(for: 104) == .source(.journal))
        #expect(directory.destination(for: 105) == .source(.cadence(.daily)))
        #expect(directory.destination(for: 106) == .source(.cadence(.weekly)))
        #expect(directory.destination(for: 107) == .source(.cadence(.monthly)))
        #expect(directory.destination(for: 108) == .source(.cadence(.rare)))
        #expect(directory.destination(for: 109) == .source(.cadence(.silent)))
```

In `sourcesAndFeedsFindTheirNumbers`, after `#expect(directory.number(for: .unread) == 101)`, add:

```swift
        #expect(directory.number(for: .journal) == 104)
        #expect(directory.number(for: .cadence(.rare)) == 108)
```

In `aNumberCollidingWithAReservedPageIsIgnored`, before the closing brace, add:

```swift
        snapshot.feeds[2].page = 104  // a corrupt database again: P104 stays the Journal
        let again = PageDirectory(snapshot: snapshot)
        #expect(again.destination(for: 104) == .source(.journal))
        #expect(again.number(for: .feed(10)) == nil)
```

At the end of the file, add a suite that types the new pages:

```swift
@MainActor @Suite struct ReservedRhythmPageTests {
    @Test func typingTheNewPagesSelectsTheirSources() throws {
        let (state, _, _) = try makeState()
        let antiope = AntiopeState(app: state)
        for (digits, source) in [("104", Source.journal), ("105", .cadence(.daily)), ("109", .cadence(.silent))] {
            for digit in digits.compactMap(\.wholeNumberValue) {
                #expect(antiope.handle(.pageDigit(digit)))
            }
            #expect(state.selection == source && antiope.readerPage == .article)
            #expect(antiope.currentPage == Int(digits))
        }
    }
}
```

In `apps/macos/RessacTests/AntiopePagesTests.swift`, replace the first line of expectations of `pagesRunDownTheLeftColumnThenTheRight` (14 reserved pages and 7 numbered ones: 11 rows on the left) by:

```swift
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows  // 14 reserved pages, 7 numbered
        let pairs = SommairePage.pairs(rows)
        #expect(pairs.map(\.left.number) == [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800])
        #expect(pairs.map { $0.right?.number } == [801, 802, 899, 200, 201, 400, 401, 402, 403, 110, nil])
```

(keep its last two expectations as they are), and replace `reservedPagesAreInColour` by:

```swift
    @Test func reservedPagesAreInColour() throws {
        var snapshot = pagedSnapshot()
        snapshot.journalUnread = 43
        let directory = PageDirectory(snapshot: snapshot)
        func line(_ number: Int) throws -> CellLine {
            SommairePage.line(for: try #require(directory.rows.first { $0.number == number }), width: 20, in: directory)
        }
        #expect(try line(101).spans[1].foreground == .yellow)
        let journal = try line(104)
        #expect(journal.spans[1].foreground == .yellow && text(of: journal.spans[2]) == " 43")
        for rhythm in 105...109 {
            #expect(try line(rhythm).spans[1].foreground == .yellow)
        }
        let triage = try line(800)
        #expect(triage.spans[1].foreground == .magenta && triage.spans[2].foreground == .red)
        #expect(try line(200).spans[1].foreground == .green)
        #expect(try line(201).spans[1].foreground == .white)
    }
```

- [ ] **Step 2: Run red.** The test command with `-only-testing:RessacTests/PageDirectoryTests -only-testing:RessacTests/ReservedRhythmPageTests -only-testing:RessacTests/SommairePageTests`. Expected: compile errors `type 'ReservedPage' has no member 'journal'` and `value of type 'ReservedPage' has no member 'cadenceGroup'`.

- [ ] **Step 3: Implement.** Re-read `apps/macos/Ressac/Antiope/PageDirectory.swift`, then replace the whole `enum ReservedPage` (from its doc comment to its closing brace, before `/// Where a page number leads`) by:

```swift
/// Page numbers fixed in code (spec §4, Reserved pages; sub-project 3 adds 104 to 109): never
/// stored, refused by `ressac page set`.
enum ReservedPage: Int, CaseIterable, Sendable {
    case index = 100, unread = 101, today = 102, starred = 103
    case journal = 104, daily = 105, weekly = 106, monthly = 107, rare = 108, silent = 109
    case needsTriage = 800, allFeeds = 801, cemetery = 802, guide = 899

    /// The core's titles (`pages::list`), so that the app and `ressac page list` agree.
    var title: String {
        switch self {
        case .index: "Index"
        case .unread: "Unread"
        case .today: "Today"
        case .starred: "Starred"
        case .journal: "Journal"
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        case .rare: "Rare"
        case .silent: "Silent"
        case .needsTriage: "Needs triage"
        case .allFeeds: "All feeds"
        case .cemetery: "Cemetery"
        case .guide: "Guide"
        }
    }

    /// The rhythm group a page 105 to 109 lists; nil for the other pages.
    var cadenceGroup: CadenceGroup? {
        switch self {
        case .daily: .daily
        case .weekly: .weekly
        case .monthly: .monthly
        case .rare: .rare
        case .silent: .silent
        case .index, .unread, .today, .starred, .journal, .needsTriage, .allFeeds, .cemetery, .guide: nil
        }
    }

    var destination: PageDestination {
        switch self {
        case .index: .index
        case .unread: .source(.unread)
        case .today: .source(.today)
        case .starred: .source(.starred)
        case .journal: .source(.journal)
        case .daily: .source(.cadence(.daily))
        case .weekly: .source(.cadence(.weekly))
        case .monthly: .source(.cadence(.monthly))
        case .rare: .source(.cadence(.rare))
        case .silent: .source(.cadence(.silent))
        case .needsTriage: .source(.garden(.needsTriage))
        case .allFeeds: .source(.garden(.all))
        case .cemetery: .source(.garden(.cemetery))
        case .guide: .guide
        }
    }

    /// The count shown beside the page: unread entries for the smart lists, the edition and the
    /// rhythm groups, feeds to triage for P800.
    func count(in snapshot: SidebarSnapshot) -> Int {
        switch self {
        case .unread: snapshot.unread
        case .today: snapshot.today
        case .starred: snapshot.starred
        case .journal: snapshot.journalUnread
        case .daily, .weekly, .monthly, .rare, .silent:
            cadenceGroup.map { snapshot.cadenceUnread[$0, default: 0] } ?? 0
        case .needsTriage: snapshot.needsTriage
        case .index, .allFeeds, .cemetery, .guide: 0
        }
    }

    /// The Garden's three pages, drawn in magenta in the index (spec §5).
    var isGarden: Bool {
        switch self {
        case .needsTriage, .allFeeds, .cemetery: true
        case .index, .unread, .today, .starred, .journal, .daily, .weekly, .monthly, .rare, .silent, .guide: false
        }
    }
}
```

`PageIndexColumn.line` already colours every non-Garden reserved title yellow on P100 (`highlightsReserved`) and white in the index column, and right-aligns the count in yellow: the new rows need nothing else.

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: the three suites pass, no warning. Then run the whole `RessacTests` target once (any other test that counted 8 reserved pages fails here; none is known: `grep -n "prefix(8)\|dropFirst(8)" apps/macos/RessacTests/*.swift` prints nothing).

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/PageDirectory.swift apps/macos/RessacTests/PageDirectoryTests.swift \
  apps/macos/RessacTests/AntiopePagesTests.swift
git commit -m "Antiope: reserved pages 104 Journal and 105-109 rhythm groups, in the index and SOMMAIRE" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6a.2: Gate

- [ ] **Step 1:** the Swift gate of the plan index (Rust gate, then the whole `xcodebuild … test`), zero warnings. `git show --stat HEAD` lists the three files of this sub-phase.

---

## Sub-phase 6b: P104 JOURNAL and FIN DE L'EDITION

**Files (5):**
- Create: `apps/macos/Ressac/Antiope/JournalPage.swift` (`JournalPage` rows, `JournalEntryPage`, `EndOfEditionPage`)
- Modify: `apps/macos/Ressac/Antiope/AntiopeState.swift` (`ReaderPage.endOfEdition`, SUITE / RETOUR / RÉPÉTITION on P104)
- Modify: `apps/macos/Ressac/Antiope/AntiopeRootView.swift` (`entriesColumn`, `readerColumn`)
- Modify: `apps/macos/Ressac/Antiope/MinitelBar.swift` (`status(progress:message:journal:)`)
- Create: `apps/macos/RessacTests/AntiopeJournalTests.swift`

`AntiopeState.swift` (215 lines) and `AntiopeRootView.swift` (308) are edited in place, not restructured: no Step 0.

### Task 6b.1: P104 rows (pure)

**Interfaces:**
- Consumes: `Edition`, `EditionItem`, `EditionOverflowItem`, `EditionSection`, `JournalModel.Phase`, `EntryPageColumn.wrap(_:width:)`, `EntryPageColumn.emptyLine(_:width:)`, `CellLine.centred`, `TeletextDateFormat`.
- Produces (used by 6b.2, 6b.3 and 6c):

```swift
enum JournalPage {
    struct Line: Identifiable, Equatable, Sendable {
        enum Role: Equatable, Sendable {
            case header, message, blank, end
            case heading(EditionSection)
            case entry(Int64)
            case overflow(feedId: Int64, page: Int?)
        }
        var id: String; var role: Role; var cells: CellLine
        var isDoubleHeight = false; var isSelected = false
        var text: String { get }
    }
    static func lines(edition: Edition?, phase: JournalModel.Phase, selected: Int64?, cells: Int,
                      timeZone: TimeZone = .current) -> [Line]
    static func colour(for section: EditionSection) -> TeletextColor
    static func headerLines(_ edition: Edition, cells: Int, timeZone: TimeZone) -> [Line]
    static func headingLine(_ section: EditionSection, count: Int, cells: Int) -> Line
    static func entryLines(_ item: EditionItem, selected: Bool, cells: Int) -> [Line]
    static func overflowLine(_ overflow: EditionOverflowItem, cells: Int) -> Line
    static func endLine(cells: Int) -> Line
    static func progressText(done: Int, total: Int) -> String     // "EDITION IN PROGRESS 34/156"
    static func lastEntryId(of edition: Edition?) -> Int64?
}
```

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/AntiopeJournalTests.swift`:

```swift
import Foundation
import Testing

@testable import Ressac

private let paris = TimeZone(identifier: "Europe/Paris")!

private func item(_ id: Int64, _ section: EditionSection, rank: Int, feed: Int64, feedTitle: String,
                  title: String? = "Entry", read: Bool = false, starred: Bool = false) -> EditionItem {
    EditionItem(entryId: id, section: section, rank: rank, feedId: feed, feedTitle: feedTitle, title: title,
                publishedAt: Timestamp.date(from: "2026-10-16T05:00:00.000Z")!, isRead: read, isStarred: starred)
}

/// Built on Friday 16 October 2026 at 09:12 in Paris: one front-page entry, two weekly entries of
/// one feed, one wire entry with 12 more left in Actualitté (page 412).
private func sampleEdition(read: Set<Int64> = [], unreachable: Int = 0, items: [EditionItem]? = nil) -> Edition {
    let built = Timestamp.date(from: "2026-10-16T07:12:00.000Z")!
    let all = items ?? [
        item(1, .front, rank: 1, feed: 30, feedTitle: "Bibliobsession", title: "A rare post"),
        item(2, .week, rank: 2, feed: 20, feedTitle: "Six Colors", title: "Weekly one", starred: true),
        item(3, .week, rank: 3, feed: 20, feedTitle: "Six Colors", title: "Weekly two"),
        item(4, .wire, rank: 4, feed: 10, feedTitle: "Actualitté", title: "Latest news"),
    ]
    return Edition(
        info: EditionInfo(id: 12, builtAt: built, windowStart: built.addingTimeInterval(-86_400), windowEnd: built,
                          unreachable: unreachable),
        items: all.map { entry in
            var entry = entry
            entry.isRead = entry.isRead || read.contains(entry.entryId)
            return entry
        },
        overflow: [EditionOverflowItem(section: .wire, feedId: 10, feedTitle: "Actualitté", page: 412, hiddenCount: 12)])
}

private func trimmed(_ text: String) -> String {
    text.trimmingCharacters(in: .whitespaces)
}

/// The rows are phase 8's `CellLine`s, read with Bedstead registered in the test host
/// (`?` stands for a glyph Bedstead lacks), as the garden suites do.
@MainActor @Suite struct JournalPageTests {
    let cells = EntryPageColumn.cells

    @Test func sectionsComeInOrderWithTheirEntriesAndOverflow() {
        let lines = JournalPage.lines(edition: sampleEdition(), phase: .idle, selected: nil, cells: cells, timeZone: paris)
        #expect(lines.map(\.role) == [
            .header,
            .blank, .heading(.front), .entry(1),
            .blank, .heading(.week), .entry(2), .entry(3),
            .blank, .heading(.wire), .entry(4), .overflow(feedId: 10, page: 412),
        ])
        #expect(Set(lines.map(\.id)).count == lines.count)  // ForEach needs unique ids
        let header = lines[0].cells
        #expect(trimmed(text(of: header.spans[0])) == "FRI 16 OCT" && header.spans[0].foreground == .cyan)
        #expect(text(of: header.spans[1]) == " 0/4 READ" && header.spans[1].foreground == .yellow)
        #expect(header.accessibilityText == "Friday 16 October · 4 entries · 0 read")
        #expect(trimmed(text(of: lines[2].cells.spans[0])) == "FRONT PAGE" && text(of: lines[2].cells.spans[1]) == " 1")
        #expect(trimmed(text(of: lines[5].cells.spans[0])) == "THIS WEEK" && text(of: lines[5].cells.spans[1]) == " 2")
        #expect(trimmed(text(of: lines[9].cells.spans[0])) == "THE WIRE" && text(of: lines[9].cells.spans[1]) == " 1")
        #expect(lines[2].cells.accessibilityText == "Front Page, 1 entry")
        #expect(lines[5].cells.accessibilityText == "This Week, 2 entries")
    }

    @Test func headingsAreColouredAndBlueNeverColoursText() throws {
        let lines = JournalPage.lines(edition: sampleEdition(), phase: .idle, selected: nil, cells: cells, timeZone: paris)
        #expect(JournalPage.colour(for: .front) == .yellow)
        #expect(JournalPage.colour(for: .week) == .cyan)
        #expect(JournalPage.colour(for: .wire) == .green)
        for line in lines {
            if case let .heading(section) = line.role {
                #expect(line.cells.spans.allSatisfy { $0.foreground == JournalPage.colour(for: section) && $0.background == .black })
            }
        }
        let overflow = try #require(lines.last)
        #expect(overflow.cells.spans[1].foreground == .white && overflow.cells.spans[1].background == .blue)
        #expect(!lines.flatMap(\.cells.spans).contains { $0.foreground == .blue })
    }

    @Test func entriesShowUnreadAndStarredMarkersAndTheSelection() throws {
        let lines = JournalPage.lines(edition: sampleEdition(read: [3]), phase: .idle, selected: 2, cells: cells,
                                      timeZone: paris)
        let two = try #require(lines.first { $0.role == .entry(2) })
        #expect(two.isSelected && two.cells.spans.allSatisfy { $0.foreground == .black && $0.background == .white })
        #expect(text(of: two.cells.spans[0]) == "● " && text(of: two.cells.spans[2]) == " *")
        #expect(two.cells.accessibilityText == "Weekly one, Six Colors, unread, starred")
        let three = try #require(lines.first { $0.role == .entry(3) })
        #expect(!three.isSelected && text(of: three.cells.spans[0]) == "  ")
        #expect(three.cells.accessibilityText == "Weekly two, Six Colors")
    }

    /// Review Focus #5: whatever the feed's title, an overflow row is exactly the column's 28
    /// cells, so the separator stays straight; VoiceOver reads the real title.
    @Test func anOverflowRowIsExactlyTheColumnWidth() {
        let awkward = EditionOverflowItem(section: .wire, feedId: 5, feedTitle: "Café ☕ Pédago", page: nil, hiddenCount: 3)
        let line = JournalPage.overflowLine(awkward, cells: cells)
        #expect(line.cells.width == cells)
        #expect(line.text == "  +3 MORE IN CAFÉ ? PÉDAGO  ")
        #expect(line.cells.spans[1].cells.runs.contains { $0.isReplacement && $0.text == "?" })
        #expect(line.cells.accessibilityText == "+3 more in Café ☕ Pédago")
        #expect(line.role == .overflow(feedId: 5, page: nil))
        let long = EditionOverflowItem(section: .week, feedId: 6, feedTitle: String(repeating: "東京", count: 30),
                                       page: 300, hiddenCount: 120)
        #expect(JournalPage.overflowLine(long, cells: cells).cells.width == cells)
    }

    @Test func everyRowIsExactlyTheWidthAsked() {
        var awkward = sampleEdition(unreachable: 3)
        awkward.items[0].title = "Café 🎉 東京 e\u{301}cole, a title far longer than two rows of the column"
        awkward.items[1].title = nil
        for width in [0, 1, 2, 9, 28, 40] {
            for phase in [JournalModel.Phase.idle, .building(done: 34, total: 156), .offline, .failed("Database is locked.")] {
                for line in JournalPage.lines(edition: awkward, phase: phase, selected: 1, cells: width, timeZone: paris) {
                    #expect(line.cells.width == width)
                }
            }
        }
    }

    @Test func theEndLineComesOnceEveryEntryIsRead() throws {
        let reading = JournalPage.lines(edition: sampleEdition(read: [1, 2, 3]), phase: .idle, selected: nil, cells: cells,
                                        timeZone: paris)
        #expect(!reading.contains { $0.role == .end })
        let done = JournalPage.lines(edition: sampleEdition(read: [1, 2, 3, 4]), phase: .idle, selected: nil, cells: cells,
                                     timeZone: paris)
        let end = try #require(done.last)
        #expect(end.role == .end && end.isDoubleHeight)
        #expect(trimmed(end.text) == "FIN DE L'EDITION" && end.cells.spans[0].foreground == .yellow)
        #expect(end.cells.accessibilityText == "That's the edition.")
        #expect(text(of: done[0].cells.spans[1]) == " 4/4 READ")
    }

    @Test func emptyBuildingOfflineAndUnreachable() {
        let empty = JournalPage.lines(edition: sampleEdition(items: []), phase: .idle, selected: nil, cells: cells,
                                      timeZone: paris)
        #expect(empty.map(\.role) == [.header, .blank, .message])
        #expect(trimmed(empty[2].text) == "NOTHING NEW THIS MORNING.")
        #expect(!empty.contains { $0.role == .end })  // an empty edition is not "finished"

        // While building, only the progress, even over yesterday's edition (spec §8).
        let building = JournalPage.lines(edition: sampleEdition(), phase: .building(done: 34, total: 156), selected: nil,
                                         cells: cells, timeZone: paris)
        #expect(building.map(\.role) == [.blank, .message])
        #expect(trimmed(building[1].text) == "EDITION IN PROGRESS 34/156")
        #expect(building[1].cells.accessibilityText == "Building this morning's edition… 34/156")
        #expect(JournalPage.progressText(done: 0, total: 0) == "EDITION IN PROGRESS")
        let none = JournalPage.lines(edition: nil, phase: .due, selected: nil, cells: cells, timeZone: paris)
        #expect(trimmed(none[1].text) == "EDITION IN PROGRESS")

        let offline = JournalPage.lines(edition: nil, phase: .offline, selected: nil, cells: cells, timeZone: paris)
        #expect(offline.map(\.role) == [.blank, .message, .message])
        #expect(trimmed(offline[1].text) == "OFFLINE: NO EDITION YET")
        #expect(offline[1].cells.spans.allSatisfy { $0.foreground == .black && $0.background == .red })
        #expect(offline[1].cells.accessibilityText == "Offline: no edition yet")
        #expect(trimmed(offline[2].text) == "RÉPÉTITION: RETRY" && offline[2].cells.spans[0].foreground == .cyan)
        let failed = JournalPage.lines(edition: nil, phase: .failed("Database is locked."), selected: nil, cells: cells,
                                       timeZone: paris)
        #expect(trimmed(failed[1].text) == "DATABASE IS LOCKED.")

        let three = JournalPage.headerLines(sampleEdition(unreachable: 3), cells: cells, timeZone: paris)
        #expect(three.count == 2 && trimmed(three[1].text) == "3 FEEDS UNREACHABLE")
        #expect(three[1].cells.spans[0].foreground == .red && three[1].cells.accessibilityText == "3 feeds unreachable")
        let one = JournalPage.headerLines(sampleEdition(unreachable: 1), cells: cells, timeZone: paris)
        #expect(trimmed(one[1].text) == "1 FEED UNREACHABLE")
        #expect(JournalPage.headerLines(sampleEdition(), cells: cells, timeZone: paris).count == 1)
    }

    @Test func overflowWithoutAPlacedEntryClosesItsSection() throws {
        var edition = sampleEdition()
        edition.overflow.append(EditionOverflowItem(section: .week, feedId: 99, feedTitle: "Capped out", page: nil,
                                                    hiddenCount: 4))
        let roles = JournalPage.lines(edition: edition, phase: .idle, selected: nil, cells: cells, timeZone: paris).map(\.role)
        let week = try #require(roles.firstIndex(of: .heading(.week)))
        #expect(Array(roles[week...].prefix(4)) == [.heading(.week), .entry(2), .entry(3), .overflow(feedId: 99, page: nil)])
        #expect(JournalPage.lastEntryId(of: edition) == 4 && JournalPage.lastEntryId(of: nil) == nil)
    }
}
```

- [ ] **Step 2: Run red.** The test command with `-only-testing:RessacTests/JournalPageTests`. Expected: `cannot find 'JournalPage' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/JournalPage.swift` (the views come in Task 6b.2; this task writes the rows only):

```swift
import SwiftUI

/// P104 JOURNAL as teletext rows (spec §8, Antiope): pure functions over the edition, so that
/// grouping, colours and widths are tested here and `JournalEntryPage` only draws what they return.
enum JournalPage {
    struct Line: Identifiable, Equatable, Sendable {
        enum Role: Equatable, Sendable {
            case header, message, blank, end
            case heading(EditionSection)
            /// An edition entry, clickable; every row of a wrapped title carries it.
            case entry(Int64)
            /// `+12 MORE IN …`, clickable: the feed it opens, and that feed's page.
            case overflow(feedId: Int64, page: Int?)
        }

        var id: String
        var role: Role
        /// The cells as drawn. An empty `accessibilityText` hides the row from VoiceOver (a
        /// wrapped title is read once, from its first row).
        var cells: CellLine
        /// `FIN DE L'EDITION`, drawn over two rows by `DoubleHeight`.
        var isDoubleHeight = false
        /// Reverse video of the selected entry.
        var isSelected = false

        /// The characters drawn, `?` replacements included.
        var text: String { cells.spans.map { $0.cells.runs.map(\.text).joined() }.joined() }
    }

    /// Section headings (spec §8): FRONT PAGE yellow, THIS WEEK cyan, THE WIRE green, on black.
    static func colour(for section: EditionSection) -> TeletextColor {
        switch section {
        case .front: .yellow
        case .week: .cyan
        case .wire: .green
        }
    }

    /// The entry column of P104 (spec §8, §9). While the edition builds, only its progress; offline
    /// or failed, why there is none; else the header, then each section with entries (heading,
    /// entries in rank order, each feed's overflow row under its last entry), then
    /// `FIN DE L'EDITION` once every entry is read. Every row is exactly `cells` wide.
    static func lines(edition: Edition?, phase: JournalModel.Phase, selected: Int64?, cells: Int,
                      timeZone: TimeZone = .current) -> [Line] {
        switch phase {
        case let .building(done, total):
            return message(progressText(done: done, total: total),
                           spoken: "Building this morning's edition…" + (total > 0 ? " \(done)/\(total)" : ""),
                           colour: .yellow, cells: cells)
        case .offline:
            return message("OFFLINE: NO EDITION YET", spoken: "Offline: no edition yet", colour: .red, reversed: true,
                           cells: cells)
                + [Line(id: "retry", role: .message,
                        cells: .centred("RÉPÉTITION: RETRY", width: max(0, cells), foreground: .cyan))]
        case let .failed(reason):
            return message(reason.uppercased(), spoken: reason, colour: .red, reversed: true, cells: cells)
        case .idle, .due:
            break
        }
        guard let edition else {
            return message(progressText(done: 0, total: 0), spoken: "Building this morning's edition…", colour: .yellow,
                           cells: cells)
        }
        var lines = headerLines(edition, cells: cells, timeZone: timeZone)
        guard !edition.items.isEmpty else {
            return lines + [blankLine(id: "blank-empty", cells: cells),
                            Line(id: "empty", role: .message,
                                 cells: EntryPageColumn.emptyLine("Nothing new this morning.", width: max(0, cells)))]
        }
        let items = edition.items.sorted { $0.rank < $1.rank }
        for section in EditionSection.allCases {
            let entries = items.filter { $0.section == section }
            let overflow = edition.overflow.filter { $0.section == section }
            guard !entries.isEmpty || !overflow.isEmpty else { continue }
            lines.append(blankLine(id: "blank-\(section.rawValue)", cells: cells))
            lines.append(headingLine(section, count: entries.count, cells: cells))
            var placed = Set<Int64>()
            for (index, entry) in entries.enumerated() {
                lines += entryLines(entry, selected: entry.entryId == selected, cells: cells)
                let lastOfFeed = index + 1 == entries.count || entries[index + 1].feedId != entry.feedId
                if lastOfFeed, let more = overflow.first(where: { $0.feedId == entry.feedId }) {
                    placed.insert(more.feedId)
                    lines.append(overflowLine(more, cells: cells))
                }
            }
            // A cap of 0 (`edition build --cap-wire 0`) leaves overflow with no entry above it.
            for more in overflow where !placed.contains(more.feedId) {
                lines.append(overflowLine(more, cells: cells))
            }
        }
        if edition.isFinished {
            lines += [blankLine(id: "blank-end", cells: cells), endLine(cells: cells)]
        }
        return lines
    }

    /// `FRI 16 OCT` in cyan, `12/43 READ` in yellow at the right edge; a second row
    /// `3 FEEDS UNREACHABLE` in red when some feeds failed (spec §9). VoiceOver reads the native
    /// header (`Friday 16 October · 43 entries · 12 read`).
    static func headerLines(_ edition: Edition, cells: Int, timeZone: TimeZone) -> [Line] {
        let width = max(0, cells)
        let built = edition.info.builtAt
        let day = TeletextDateFormat.string(from: built, pattern: "EEE dd MMM", timeZone: timeZone).uppercased()
        let count = " \(edition.readCount)/\(edition.items.count) READ"
        let countCells = min(count.count, width)
        let entries = edition.items.count == 1 ? "1 entry" : "\(edition.items.count) entries"
        let spoken = TeletextDateFormat.string(from: built, pattern: "EEEE d MMMM", timeZone: timeZone)
            + " · \(entries) · \(edition.readCount) read"
        var lines = [Line(id: "header", role: .header, cells: CellLine([
            CellSpan(" " + day, cells: width - countCells, foreground: .cyan),
            CellSpan(count, cells: countCells, foreground: .yellow),
        ], accessibilityText: spoken))]
        let unreachable = edition.info.unreachable
        if unreachable > 0 {
            let text = unreachable == 1 ? "1 feed unreachable" : "\(unreachable) feeds unreachable"
            lines.append(Line(id: "unreachable", role: .header, cells: CellLine([
                CellSpan(" " + text.uppercased(), cells: width, foreground: .red),
            ], accessibilityText: text)))
        }
        return lines
    }

    /// ` FRONT PAGE … 2`, in the section's colour on black.
    static func headingLine(_ section: EditionSection, count: Int, cells: Int) -> Line {
        let width = max(0, cells)
        let colour = colour(for: section)
        let number = " \(count)"
        let numberCells = min(number.count, width)
        return Line(id: "heading-\(section.rawValue)", role: .heading(section), cells: CellLine([
            CellSpan(" " + section.antiopeTitle, cells: width - numberCells, foreground: colour),
            CellSpan(number, cells: numberCells, foreground: colour),
        ], accessibilityText: count == 1 ? "\(section.title), 1 entry" : "\(section.title), \(count) entries"))
    }

    /// As `EntryPageColumn.lines(for:width:isSelected:)`: a yellow `●` when unread, the title over
    /// two rows at most, a white `*` when starred, markers on the first row only; black on white
    /// when selected. VoiceOver reads "Title, Feed, unread, starred" on the first row.
    static func entryLines(_ item: EditionItem, selected: Bool, cells: Int) -> [Line] {
        let width = max(0, cells)
        let title = item.title?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "Untitled"
        let markerCells = min(2, width)
        let titleCells = max(0, width - 4)
        let starCells = max(0, width - markerCells - titleCells)
        let spoken = [title, item.feedTitle, item.isRead ? nil : "unread", item.isStarred ? "starred" : nil]
            .compactMap { $0 }.joined(separator: ", ")
        return EntryPageColumn.wrap(title, width: titleCells).enumerated().map { index, text in
            let line = CellLine([
                CellSpan(index == 0 && !item.isRead ? "●" : "", cells: markerCells, foreground: .yellow),
                CellSpan(text, cells: titleCells, foreground: .white),
                CellSpan(index == 0 && item.isStarred ? " *" : "", cells: starCells, foreground: .white),
            ], accessibilityText: index == 0 ? spoken : "")
            return Line(id: "entry-\(item.entryId)-\(index)", role: .entry(item.entryId),
                        cells: selected ? line.reversed(.black, on: .white) : line, isSelected: selected)
        }
    }

    /// `  +12 MORE IN ACTUALITTÉ`: white on blue (blue never colours text, as the Fossil heading),
    /// two cells in, under the entry titles. VoiceOver reads the native `+12 more in Actualitté`.
    static func overflowLine(_ overflow: EditionOverflowItem, cells: Int) -> Line {
        let width = max(0, cells)
        let indent = min(2, width)
        return Line(id: "overflow-\(overflow.section.rawValue)-\(overflow.feedId)",
                    role: .overflow(feedId: overflow.feedId, page: overflow.page),
                    cells: CellLine([
                        CellSpan("", cells: indent, foreground: .white),
                        CellSpan("+\(overflow.hiddenCount) MORE IN \(overflow.feedTitle.uppercased())",
                                 cells: width - indent, foreground: .white, background: .blue),
                    ], accessibilityText: "+\(overflow.hiddenCount) more in \(overflow.feedTitle)"))
    }

    /// The Minitel exception (spec §2): `FIN DE L'EDITION`, yellow, centred, drawn in double height.
    static func endLine(cells: Int) -> Line {
        var line = CellLine.centred("FIN DE L'EDITION", width: max(0, cells), foreground: .yellow)
        line.accessibilityText = "That's the edition."
        return Line(id: "end", role: .end, cells: line, isDoubleHeight: true)
    }

    /// P104's progress row and the Minitel bar's status during the build.
    static func progressText(done: Int, total: Int) -> String {
        total > 0 ? "EDITION IN PROGRESS \(done)/\(total)" : "EDITION IN PROGRESS"
    }

    /// The edition's last entry by rank: SUITE past it lands on FIN DE L'EDITION.
    static func lastEntryId(of edition: Edition?) -> Int64? {
        edition?.items.max { $0.rank < $1.rank }?.entryId
    }

    static func blankLine(id: String, cells: Int) -> Line {
        Line(id: id, role: .blank, cells: .blank(max(0, cells)))
    }

    /// A blank row, then one centred row: `colour` on black, or black on `colour` when `reversed`.
    private static func message(_ text: String, spoken: String, colour: TeletextColor, reversed: Bool = false,
                                cells: Int) -> [Line] {
        var line = CellLine.centred(text, width: max(0, cells), foreground: colour)
        if reversed { line = line.reversed(.black, on: colour) }
        line.accessibilityText = spoken
        return [blankLine(id: "blank-message", cells: cells), Line(id: "message", role: .message, cells: line)]
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
```

Check before running: `grep -rn "var nilIfEmpty" apps/macos/Ressac` must print only this file; if another file already declares it at module level, delete the `private extension` above and use that one.

- [ ] **Step 4: Run green.** Same command as Step 2. Expected: `JournalPageTests` passes, 8 tests, no warning.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Antiope/JournalPage.swift apps/macos/RessacTests/AntiopeJournalTests.swift
git commit -m "Antiope: P104 rows, sections, overflow, FIN DE L'EDITION, progress and offline rows" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6b.2: SUITE, RETOUR and RÉPÉTITION on P104; the Minitel status

**Interfaces:**
- Consumes: `AppState.journal` (`edition`, `phase`, `retry()`), `AppState.selection`, `selectedEntryId`, `selectedEntry`, `readerPageDown`, `toggleRead()`, `JournalPage.lastEntryId(of:)`, `JournalPage.progressText(done:total:)`.
- Produces: `ReaderPage.endOfEdition`; `AntiopeState.retriesJournal(_ phase: JournalModel.Phase) -> Bool` (static); `MinitelBar.status(progress:message:journal:)` with `journal` defaulting to `.idle`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeJournalTests.swift`:

```swift
@MainActor @Suite struct JournalNavigationTests {
    /// An edition of two entries, the first ranked one published *before* the second, so that rank
    /// order and date order differ; P104 selected.
    func makeEdition() async throws -> (AntiopeState, AppState, first: Int64, last: Int64) {
        let (state, test, _) = try makeState()
        let first = try test.entry(feed: 1, title: "Ranked first", published: "2026-10-09T06:00:00.000Z")
        let last = try test.entry(feed: 8, title: "Ranked last", published: "2026-10-09T09:00:00.000Z")
        try test.execute("DELETE FROM edition")  // whatever the contract fixture holds
        try test.execute("""
            INSERT INTO edition (id, built_at, window_start, window_end, unreachable)
            VALUES (1, '2026-10-10T07:00:00.000Z', '2026-10-09T07:00:00.000Z', '2026-10-10T07:00:00.000Z', 0)
            """)
        try test.execute("INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?, 'front', 1)", [first])
        try test.execute("INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?, 'wire', 2)", [last])
        try await state.database.notifyExternalChanges()
        state.autoReadDelay = .seconds(60)
        state.selection = .journal
        #expect(await eventually { state.journal.edition?.items.count == 2 && state.entries.count == 2 })
        // Phase 5b's contract: on P104 the list runs in rank order, not by date.
        #expect(state.entries.map(\.entryId) == [first, last])
        return (AntiopeState(app: state), state, first, last)
    }

    @Test func suiteFromTheLastEntryLandsOnTheEndOfTheEdition() async throws {
        let (antiope, state, first, last) = try await makeEdition()
        state.selectedEntryId = first
        #expect(await eventually { state.selectedEntry != nil })
        #expect(!antiope.handle(.nextEntry))  // not the last entry: AppState moves on
        #expect(state.perform(.nextEntry) && state.selectedEntryId == last)
        #expect(antiope.handle(.nextEntry))   // the last one: FIN DE L'EDITION
        #expect(antiope.readerPage == .endOfEdition && antiope.currentPage == 104)
        #expect(await eventually { state.entries.first { $0.entryId == last }?.isRead == true })  // read on leaving
        #expect(antiope.handle(.nextEntry) && antiope.handle(.advance))  // nothing after the end
        #expect(antiope.readerPage == .endOfEdition && state.selectedEntryId == last)
        #expect(antiope.handle(.previousEntry))  // RETOUR brings the last entry back
        #expect(antiope.readerPage == .article && state.selectedEntryId == last)
        // Space at the bottom of the last entry (no reader in the test: nothing left to scroll).
        #expect(antiope.handle(.advance))
        #expect(await eventually { antiope.readerPage == .endOfEdition })
    }

    @Test func elsewhereTheLastEntryStaysAppStates() async throws {
        let (antiope, state, _, _) = try await makeEdition()
        state.selection = .unread
        #expect(await eventually { state.entries.count == 2 })
        state.selectedEntryId = state.entries.last?.entryId
        #expect(!antiope.handle(.nextEntry) && !antiope.handle(.advance))
        #expect(antiope.readerPage == .article)
    }

    @Test func aJumpOrAClickLeavesTheEndPage() async throws {
        let (antiope, state, _, last) = try await makeEdition()
        state.selectedEntryId = last
        #expect(await eventually { state.selectedEntry != nil })
        #expect(antiope.handle(.nextEntry) && antiope.readerPage == .endOfEdition)
        antiope.jump(to: 101)
        #expect(antiope.readerPage == .article && state.selection == .unread)
    }

    @Test func repetitionRetriesOnlyAnEditionThatCouldNotBeBuilt() {
        #expect(AntiopeState.retriesJournal(.offline))
        #expect(AntiopeState.retriesJournal(.failed("Database is locked.")))
        for phase in [JournalModel.Phase.idle, .due, .building(done: 1, total: 2)] {
            #expect(!AntiopeState.retriesJournal(phase))
        }
    }

    @Test func theMinitelBarShowsTheBuild() {
        let progress = RunProgress(done: 34, total: 156, label: "")
        #expect(MinitelBar.status(progress: progress, message: nil, journal: .building(done: 34, total: 156))?.text
            == "EDITION IN PROGRESS 34/156")
        #expect(MinitelBar.status(progress: progress, message: nil, journal: .building(done: 34, total: 156))?.colour
            == .yellow)
        #expect(MinitelBar.status(progress: progress, message: nil)?.text == "REFRESHING 34/156")
        #expect(MinitelBar.status(progress: nil, message: "Feed #3 is buried.", journal: .building(done: 1, total: 2))?.text
            == "Feed #3 is buried.")
        // The header's page number "searches" during every long run; the build is one (phase 4b).
        #expect(RunRequest.buildEdition().kind == .long)
    }
}
```

- [ ] **Step 2: Run red.** The test command with `-only-testing:RessacTests/JournalNavigationTests`. Expected: `type 'ReaderPage' has no member 'endOfEdition'`, `extra argument 'journal' in call`.

- [ ] **Step 3: `AntiopeState`.** Re-read `apps/macos/Ressac/Antiope/AntiopeState.swift`, then make four edits.

(a) Replace the `ReaderPage` declaration and its comment by:

```swift
/// What the reader column shows: the selected entry, P100 SOMMAIRE, P899 GUIDE, or the end of
/// the morning edition that SUITE reaches from P104's last entry.
enum ReaderPage: Equatable, Sendable {
    case article, index, guide, endOfEdition
}
```

(b) Below the line `@ObservationIgnored private(set) var directoryBuilds = 0`, add:

```swift
    /// A Space on the edition's last entry is scrolling the article before landing on the end.
    @ObservationIgnored private var isAdvancingToEnd = false
```

(c) In `currentPage`, replace `case .guide: ReservedPage.guide.rawValue` by:

```swift
        case .guide: ReservedPage.guide.rawValue
        case .endOfEdition: ReservedPage.journal.rawValue
```

(d) In `handle(_:)`, replace the `.repetition` case by:

```swift
        case .repetition:
            // Offline or failed on P104: RÉPÉTITION retries the edition (spec §9).
            if app.selection == .journal, Self.retriesJournal(app.journal.phase) {
                Task { await app.journal.retry() }
                return true
            }
            guard app.canRefresh else { return false }
            app.refreshCurrent()
            return true
```

and replace the `.nextEntry, .previousEntry, .advance` case (comment included) by:

```swift
        case .nextEntry, .previousEntry, .advance:
            if readerPage == .endOfEdition {
                // FIN DE L'EDITION: nothing comes after it; RETOUR brings the last entry back.
                if command == .previousEntry { readerPage = .article }
                return true
            }
            if readerPage == .article {
                return command != .previousEntry && suiteFromTheLastEntry(advancing: command == .advance)
            }
            // SUITE and RETOUR are today's commands and `AppState` performs them (Space still
            // scrolls the article first). Leaving P100 or P899 brings the article back, and that
            // first press is all it does: `AppState` would otherwise move on in the same press,
            // with no reader on screen to scroll, and skip the entry unseen.
            guard app.canNavigateEntries else { return false }
            readerPage = .article
            return app.selectedEntryId != nil
```

Then, in the `// MARK: Private` section, add before `private func move(to:)`:

```swift
    /// Whether RÉPÉTITION on P104 retries the edition rather than refreshing: offline or failed.
    static func retriesJournal(_ phase: JournalModel.Phase) -> Bool {
        switch phase {
        case .offline, .failed: true
        case .idle, .due, .building: false
        }
    }

    /// SUITE on P104's last entry (spec §8): `j` lands on FIN DE L'EDITION at once; Space scrolls
    /// the article first, as everywhere, and lands there at its bottom. False anywhere else, so that
    /// `AppState` moves on as usual.
    private func suiteFromTheLastEntry(advancing: Bool) -> Bool {
        guard app.selection == .journal, let id = app.selectedEntryId,
              id == JournalPage.lastEntryId(of: app.journal.edition) else { return false }
        guard advancing else {
            landOnEndOfEdition()
            return true
        }
        guard !isAdvancingToEnd else { return true }
        isAdvancingToEnd = true
        Task { [weak self] in
            guard let self else { return }
            defer { isAdvancingToEnd = false }
            if let pageDown = app.readerPageDown, await pageDown(id) { return }
            // Not when a click or a jump moved on meanwhile.
            guard app.selectedEntryId == id, app.selection == .journal, readerPage == .article else { return }
            landOnEndOfEdition()
        }
        return true
    }

    /// Moving past the last entry marks it read, as `j` and Space do between entries (app spec §7).
    private func landOnEndOfEdition() {
        if app.selectedEntry?.isRead == false { app.toggleRead() }
        readerPage = .endOfEdition
    }
```

`jump(to:)` already sets `readerPage = .article`, and a click on an entry row does too (`EntryPageColumn.open`, `JournalEntryPage.open`), so the end page never outlives a move elsewhere.

- [ ] **Step 4: `MinitelBar`.** Re-read `apps/macos/Ressac/Antiope/MinitelBar.swift`. Replace `status(progress:message:)` and its doc comment by:

```swift
    /// What replaces the hints (spec §6): the garden's last refusal ("Nothing to repair for this
    /// feed."), else `EDITION IN PROGRESS 34/156` while the morning edition builds, else
    /// `REFRESHING 42/155` while a run is going, all in yellow. Nil keeps the hints.
    static func status(progress: RunProgress?, message: String?, journal: JournalModel.Phase = .idle) -> MinitelHint? {
        if let message { return MinitelHint(text: message, colour: .yellow) }
        if case let .building(done, total) = journal {
            return MinitelHint(text: JournalPage.progressText(done: done, total: total), colour: .yellow)
        }
        guard let progress else { return nil }
        let text = progress.total > 0 ? "REFRESHING \(progress.done)/\(progress.total)" : "REFRESHING"
        return MinitelHint(text: text, colour: .yellow)
    }
```

and in `body`, replace

```swift
                    Self.status(progress: app.runs.progress, message: app.selection.isGarden ? garden.message : nil)
```

by

```swift
                    Self.status(progress: app.runs.progress, message: app.selection.isGarden ? garden.message : nil,
                                journal: app.journal.phase)
```

- [ ] **Step 5: Run green.** Same command as Step 2, plus `-only-testing:RessacTests/AntiopeStateTests` (sub-project 4's `suiteFromAReservedPageOnlyBringsTheArticleBack` must still pass: P100 and P899 keep their behaviour). Expected: both suites pass, no warning. If `makeEdition`'s rank-order expectation fails, stop: phase 5b's list for `.journal` is not in rank order; report to Clément before touching 5b's code.

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Antiope/AntiopeState.swift apps/macos/Ressac/Antiope/MinitelBar.swift \
  apps/macos/RessacTests/AntiopeJournalTests.swift
git commit -m "Antiope: SUITE past the edition's last entry lands on FIN DE L'EDITION; RÉPÉTITION retries it" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6b.3: The P104 views in the window

**Interfaces:**
- Consumes: `JournalPage`, `AntiopeState` (`column`, `readerPage`, `jump(to:)`), `DoubleHeight`, `CellLineView`.
- Produces: `struct JournalEntryPage: View`, `struct EndOfEditionPage: View`.

- [ ] **Step 1: The views.** Append to `apps/macos/Ressac/Antiope/JournalPage.swift`:

```swift
/// The entry column of P104 (spec §8): the rows of `JournalPage.lines`, entries and overflow rows
/// clickable, the selection followed as `EntryPageColumn` does.
struct JournalEntryPage: View {
    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        let lines = JournalPage.lines(edition: app.journal.edition, phase: app.journal.phase,
                                      selected: app.selectedEntryId, cells: EntryPageColumn.cells)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        row(line)
                    }
                }
            }
            .scrollIndicators(.never)
            // `j`, `k` and Space select from the keyboard: the column follows the selection.
            .onChange(of: app.selectedEntryId) { _, id in
                if let id { proxy.scrollTo("entry-\(id)-0") }
            }
        }
    }

    @ViewBuilder private func row(_ line: JournalPage.Line) -> some View {
        switch line.role {
        case let .entry(entryId):
            Button { open(entryId) } label: { CellLineView(line: line.cells) }
                .buttonStyle(.plain)
                .focusable(false)  // Full Keyboard Access would give Space to the button, not to SUITE
                .accessibilityLabel(line.cells.accessibilityText)
                .accessibilityHidden(line.cells.accessibilityText.isEmpty)
                .accessibilityAddTraits(line.isSelected ? .isSelected : [])
                .id(line.id)
        case let .overflow(feedId, page):
            Button { openFeed(feedId, page: page) } label: { CellLineView(line: line.cells) }
                .buttonStyle(.plain)
                .focusable(false)
                .accessibilityLabel(line.cells.accessibilityText)
                .accessibilityAddTraits(.isLink)
        case .header, .message, .blank, .end, .heading:
            Group {
                if line.isDoubleHeight {
                    DoubleHeight(line: line.cells)
                } else {
                    CellLineView(line: line.cells)
                }
            }
            .accessibilityHidden(line.cells.accessibilityText.isEmpty)
        }
    }

    private func open(_ entryId: Int64) {
        app.selectedEntryId = entryId
        antiope.column = .entries
        antiope.readerPage = .article
    }

    /// The feed's page, as typing its number would; a feed without a number yet is selected directly.
    private func openFeed(_ feedId: Int64, page: Int?) {
        if let page {
            antiope.jump(to: page)
        } else {
            app.selection = .feed(feedId)
            antiope.readerPage = .article
            antiope.column = .index
        }
    }
}

/// The reader column after P104's last entry: `FIN DE L'EDITION` in double height, centred.
struct EndOfEditionPage: View {
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            DoubleHeight(line: JournalPage.endLine(cells: max(0, Int(proxy.size.width / cell.width))).cells)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(TeletextColor.black.color)
    }
}
```

- [ ] **Step 2: The root view.** Re-read `apps/macos/Ressac/Antiope/AntiopeRootView.swift`. Replace the `entriesColumn` property by:

```swift
    /// The entry list, P104's edition, or P800–P802 in a Garden scope.
    @ViewBuilder private var entriesColumn: some View {
        if case let .garden(scope) = state.selection {
            GardenEntryPage(scope: scope)
        } else if state.selection == .journal {
            JournalEntryPage()
        } else {
            EntryPageColumn()
        }
    }
```

and in `readerColumn`, after

```swift
        case .guide:
            GuidePage()
```

add

```swift
        case .endOfEdition:
            EndOfEditionPage()
```

- [ ] **Step 3: Build and run the Antiope suites.** The test command with `-only-testing:RessacTests/JournalPageTests -only-testing:RessacTests/JournalNavigationTests -only-testing:RessacTests/AntiopeFrameTests -only-testing:RessacTests/SommairePageTests`. Expected: all pass, `** TEST SUCCEEDED **`, no warning (an unhandled `ReaderPage` case would be a compile error here).

- [ ] **Step 4: Commit.**

```bash
git add apps/macos/Ressac/Antiope/JournalPage.swift apps/macos/Ressac/Antiope/AntiopeRootView.swift
git commit -m "Antiope: P104 entry column and the FIN DE L'EDITION page in the window" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6b.4: Gate

- [ ] **Step 1:** the Swift gate of the plan index, zero warnings.
- [ ] **Step 2:** `git show --stat HEAD~2..HEAD` lists only the five files of this sub-phase; `git diff main...HEAD -- apps/macos/Ressac/App apps/macos/Ressac/Reading` shows nothing from 6b (AppState and the native views are untouched).

---

## Sub-phase 6c: sub-pages `104 n/N` and the rhythm in the Antiope inspector

**Files (5):**
- Modify: `apps/macos/Ressac/Reading/ReaderView.swift` (`SubpageLabel.ofEdition`, `subpage(for:theme:)`)
- Modify: `apps/macos/Ressac/Antiope/GardenPage.swift` (`Line.Role.rhythm`, `inspectorLines(…cadence:now:)`, `CadenceChoice`, `cadenceChoices`)
- Modify: `apps/macos/Ressac/Antiope/GardenPageViews.swift` (`GardenInspectorPage`)
- Modify: `apps/macos/RessacTests/AntiopeGardenTests.swift`
- Modify: `apps/macos/RessacTests/AntiopeJournalTests.swift`

`ReaderView.swift` (395 lines) gets two small additions, not a restructuring: no Step 0.

### Task 6c.1: An edition entry is a sub-page of P104

**Interfaces:**
- Consumes: `Edition`, `ReservedPage.journal`, `AppState.journal.edition`, `AppState.selection`.
- Produces: `SubpageLabel.ofEdition(entryId: Int64, in edition: Edition?) -> SubpageLabel?`.

- [ ] **Step 1: Write the failing test.** Append to `apps/macos/RessacTests/AntiopeJournalTests.swift`:

```swift
@Suite struct EditionSubpageTests {
    @Test func anEditionEntryIsASubPageOf104ByItsRank() {
        let edition = sampleEdition()
        #expect(SubpageLabel.ofEdition(entryId: 3, in: edition)?.text == "104 3/4")
        #expect(SubpageLabel.ofEdition(entryId: 1, in: edition)?.text == "104 1/4")
        #expect(SubpageLabel.ofEdition(entryId: 99, in: edition) == nil)  // not in the edition
        #expect(SubpageLabel.ofEdition(entryId: 1, in: nil) == nil)
    }
}
```

- [ ] **Step 2: Run red.** The test command with `-only-testing:RessacTests/EditionSubpageTests`. Expected: `type 'SubpageLabel' has no member 'ofEdition'`.

- [ ] **Step 3: Implement.** Re-read `apps/macos/Ressac/Reading/ReaderView.swift`. In `struct SubpageLabel`, after `static func of(…)`, add:

```swift
    /// "104 3/43": on P104 an entry is a sub-page of the Journal by its rank in the edition, out of
    /// the edition's size (spec §8, Antiope). Nil when the entry is not in it.
    static func ofEdition(entryId: Int64, in edition: Edition?) -> SubpageLabel? {
        guard let edition, let item = edition.items.first(where: { $0.entryId == entryId }) else { return nil }
        return SubpageLabel(page: ReservedPage.journal.rawValue, index: item.rank, count: edition.items.count)
    }
```

and in `subpage(for:theme:)`, replace its last line

```swift
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, total: state.entryTotal, page: directory.page(forFeed:))
```

by

```swift
        if state.selection == .journal {
            return SubpageLabel.ofEdition(entryId: entry.entryId, in: state.journal.edition)
        }
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, total: state.entryTotal, page: directory.page(forFeed:))
```

`ArticlePage` leaves the sub-page out of its equality and `ArticleWebView` rewrites that line in place, so reading an entry of the edition does not reload the page.

- [ ] **Step 4: Run green.** Same command as Step 2, plus `-only-testing:RessacTests/ReaderTests`. Expected: both pass.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/AntiopeJournalTests.swift
git commit -m "Antiope: an edition entry is sub-page n/N of P104" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6c.2: The rhythm line and the override rows in the Antiope inspector

**Interfaces:**
- Consumes: `CadenceRow`, `CadenceGroup`, `GardenModel.rhythmText(_:now:)` (5c), `Queries.cadenceRows()`, `RunRequest.setCadence(feedId:_:)`, `RunCoordinator.start(_:onEvent:)`.
- Produces: `GardenPage.Line.Role.rhythm`; `GardenPage.inspectorLines(_:log:cells:timeZone:cadence:now:)` (the two new parameters defaulted, so existing calls compile); `GardenPage.CadenceChoice` (`group: CadenceGroup?`, `line: Line`, `id`); `GardenPage.cadenceChoices(_ cadence: CadenceRow, cells: Int) -> [CadenceChoice]`.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/AntiopeGardenTests.swift`:

```swift
private func cadenceRow(_ id: Int64, effective: CadenceGroup = .weekly, computed: CadenceGroup = .weekly,
                        override: Bool = false) -> CadenceRow {
    CadenceRow(feedId: id, effective: effective, computed: computed, isOverride: override, medianIntervalDays: 6.5,
               entriesPerMonth: 4.2, lastEntryAt: Timestamp.date(from: "2026-10-15T08:00:00.000Z"), unread: 3)
}

@MainActor @Suite struct GardenRhythmTests {
    let now = Timestamp.date(from: "2026-10-16T08:00:00.000Z")!

    @Test func theInspectorShowsTheRhythmUnderTheStatus() throws {
        let row = gardenRow(7, "Six Colors", status: "Alive", reason: "Last entry on 2026-10-15.")
        let cadence = cadenceRow(7)
        let lines = GardenPage.inspectorLines(row, log: [], cells: 80, timeZone: .gmt, cadence: cadence, now: now)
        let rhythm = try #require(lines.firstIndex { $0.text.hasPrefix("RHYTHM") })
        #expect(trailingTrimmed(lines[rhythm].text) == "RHYTHM    " + GardenModel.rhythmText(cadence, now: now))
        #expect(lines[rhythm].cells.spans[0].foreground == .cyan && lines[rhythm].cells.spans[1].foreground == .white)
        #expect(lines[rhythm + 1].role == .rhythm)  // the anchor of the override rows
        let reason = try #require(lines.firstIndex { $0.text.hasPrefix("Last entry") })
        let evidence = lines.firstIndex { $0.text.hasPrefix("LINKS") }
        #expect(reason < rhythm && rhythm < (evidence ?? .max))
        for width in [0, 1, 24, 40] {
            #expect(GardenPage.inspectorLines(row, log: [], cells: width, cadence: cadence, now: now)
                .allSatisfy { $0.cells.width == width })
        }
    }

    @Test func noRhythmForABuriedFeedOrWithoutCadence() {
        let alive = gardenRow(7, "Six Colors", status: "Alive")
        #expect(!GardenPage.inspectorLines(alive, log: [], cells: 40).contains { $0.role == .rhythm })
        let buried = gardenRow(8, "Gone", status: "Dead", state: "buried", decision: "bury")
        #expect(!GardenPage.inspectorLines(buried, log: [], cells: 40, cadence: cadenceRow(8), now: now)
            .contains { $0.role == .rhythm || $0.text.hasPrefix("RHYTHM") })
    }

    @Test func theOverrideRowsShowTheChoiceInForce() {
        let auto = GardenPage.cadenceChoices(cadenceRow(7), cells: 24)
        #expect(auto.map(\.group) == [nil, .daily, .weekly, .monthly, .rare, .silent])
        #expect(auto.map { trailingTrimmed($0.line.text) } == [" AUTO", " DAILY", " WEEKLY", " MONTHLY", " RARE", " SILENT"])
        #expect(auto[0].line.isSelected && auto[0].line.cells.spans.allSatisfy { $0.foreground == .black && $0.background == .cyan })
        #expect(auto.dropFirst().allSatisfy { !$0.line.isSelected && $0.line.cells.spans[0].foreground == .cyan })
        #expect(auto[0].line.cells.accessibilityText == "Auto, current" && auto[1].line.cells.accessibilityText == "Daily")
        let forced = GardenPage.cadenceChoices(cadenceRow(7, effective: .daily, override: true), cells: 24)
        #expect(forced.filter(\.line.isSelected).map(\.group) == [.daily])
        #expect(Set(forced.map(\.id)).count == 6)
        for width in [0, 1, 8, 24, 40] {
            #expect(GardenPage.cadenceChoices(cadenceRow(7), cells: width).allSatisfy { $0.line.cells.width == width })
        }
    }
}
```

- [ ] **Step 2: Run red.** The test command with `-only-testing:RessacTests/GardenRhythmTests`. Expected: `extra arguments 'cadence', 'now' in call`, `type 'GardenPage' has no member 'cadenceChoices'`.

- [ ] **Step 3: `GardenPage`.** Re-read `apps/macos/Ressac/Antiope/GardenPage.swift`. Four edits.

(a) In `Line.Role`, after `case text, blank`, add:

```swift
            /// Under the rhythm line: where `GardenInspectorPage` draws the override rows.
            case rhythm
```

(b) Change the signature of `inspectorLines` to:

```swift
    static func inspectorLines(_ row: GardenRow?, log: [FetchLogRow], cells: Int,
                               timeZone: TimeZone = .current, cadence: CadenceRow? = nil, now: Date = Date()) -> [Line] {
```

(c) In its body, right after

```swift
        if let reason = row.reason {
            paragraph(reason, color: .white)
        }
```

add

```swift
        // The rhythm (sub-project 3): the native inspector's line, then the override rows'
        // anchor. Not for a buried feed, which has no rhythm group.
        if let cadence, !buried {
            blank()
            labeled("Rhythm", GardenModel.rhythmText(cadence, now: now))
            lines.append(Line(id: "inspector-rhythm", role: .rhythm, cells: .blank(cells)))
        }
```

(d) After the `inspectorLines` function (before `static func wrap`), add:

```swift
    /// One row of the rhythm override (spec §8, Inspector): `nil` is Auto.
    struct CadenceChoice: Identifiable, Equatable, Sendable {
        var group: CadenceGroup?
        var line: Line
        var id: String { line.id }
    }

    /// `AUTO`, then the five groups, one row each, cyan; the choice in force (Auto unless the feed
    /// is overridden) in reverse video, black on cyan. Every row is exactly `cells` wide.
    static func cadenceChoices(_ cadence: CadenceRow, cells: Int) -> [CadenceChoice] {
        let width = max(0, cells)
        let choices: [CadenceGroup?] = [nil] + CadenceGroup.allCases.map { Optional($0) }
        return choices.map { group in
            let title = group?.title ?? "Auto"
            let current = cadence.isOverride ? group == cadence.effective : group == nil
            let line = CellLine([CellSpan(" " + title.uppercased(), cells: width, foreground: .cyan)],
                                accessibilityText: current ? "\(title), current" : title)
            return CadenceChoice(group: group, line: Line(id: "cadence-\(group?.rawValue ?? "auto")", role: .text,
                                                          cells: current ? line.reversed(.black, on: .cyan) : line,
                                                          isSelected: current))
        }
    }
```

- [ ] **Step 4: `GardenInspectorPage`.** Re-read `apps/macos/Ressac/Antiope/GardenPageViews.swift`. Replace the whole `struct GardenInspectorPage` by:

```swift
/// The reader column of the garden pages: the selected feed's inspector as a teletext page, with
/// its rhythm and the six override rows (sub-project 3).
struct GardenInspectorPage: View {
    @Environment(AppState.self) private var state
    @Environment(GardenStore.self) private var store
    @Environment(\.teletextSize) private var size
    @State private var cadence: CadenceRow?

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let cells = Int(proxy.size.width / cell.width)
            let lines = GardenPage.inspectorLines(store.selectedRow(state), log: store.log, cells: cells,
                                                  cadence: cadence, now: Date())
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        GardenLineView(line: line)
                        if line.role == .rhythm, let cadence {
                            ForEach(GardenPage.cadenceChoices(cadence, cells: cells)) { choice in
                                Button { choose(choice.group, for: cadence.feedId) } label: {
                                    GardenLineView(line: choice.line)
                                }
                                .buttonStyle(.plain)
                                .focusable(false)
                                .accessibilityLabel(choice.line.cells.accessibilityText)
                            }
                        }
                    }
                }
            }
            .scrollIndicators(.never)
        }
        .background(TeletextColor.black.color)
        .task(id: state.selectedFeedId) {
            await store.observeLog(feedId: state.selectedFeedId, app: state)
        }
        .task(id: state.selectedFeedId) {
            await observeCadence(feedId: state.selectedFeedId)
        }
    }

    /// `ressac feed cadence`; a refusal (unknown or buried feed) shows in the red banner row.
    private func choose(_ group: CadenceGroup?, for feedId: Int64) {
        state.runs.start(.setCadence(feedId: feedId, group))
    }

    /// The selected feed's row of `v_cadence`, live; nil for a buried feed (the view lists active
    /// feeds only) or when nothing is selected.
    private func observeCadence(feedId: Int64?) async {
        cadence = nil
        guard let feedId else { return }
        do {
            for try await rows in Queries.cadenceRows().values(in: state.database.reader) {
                cadence = rows.first { $0.feedId == feedId }
            }
        } catch {
            cadence = nil
        }
    }
}
```

The error is dropped as `GardenStore.observeLog` drops it: the rows disappear and the rest of the inspector stays.

- [ ] **Step 5: Run green.** The test command with `-only-testing:RessacTests/GardenRhythmTests -only-testing:RessacTests/GardenPageTests`. Expected: both suites pass (the garden suites of sub-project 4 call `inspectorLines` without the new parameters and see no rhythm), no warning.

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Antiope/GardenPage.swift apps/macos/Ressac/Antiope/GardenPageViews.swift \
  apps/macos/RessacTests/AntiopeGardenTests.swift
git commit -m "Antiope: rhythm line and override rows in the garden inspector" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6c.3: Gate

- [ ] **Step 1:** the Swift gate of the plan index (Rust gate, then the whole `xcodebuild … test`), zero warnings.
- [ ] **Step 2:** `git show --stat HEAD~1..HEAD` lists only the five files of this sub-phase; `grep -rn "FIN DE L'EDITION\|RÉPÉTITION: RETRY" apps/macos/Ressac` prints only `JournalPage.swift` (the French strings stay where the plan allows them).

## Notes for the executor

- **No simulated keystrokes**: everything above is tested in process (`AntiopeState.handle`, pure row builders). The visual check of P104 is phase 7's checklist, done by Clément.
- **If a name from phases 4a, 5a, 5b or 5c differs** from the plan index (for instance `journal` on `AppState`, `cadenceUnread`, `rhythmText`), use the real one, keep the behaviour, and record the difference in the sub-phase's hand-off; do not rename the other phase's code.
- **`GardenModel.rhythmText(_:now:)`** comes from 5c (listed in the plan index's Phase 5c interfaces); `rhythmLine` is `"Rhythm: " + rhythmText`. Never write a second formatter for the same sentence.
