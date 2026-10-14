# Phase 7: navigation logic (pages, page entry, `AntiopeState`, keys)

**Runs after:** phase 5 (7a, then 7b).

Read the plan index first (`docs/plans/2026-10-14-antiope.md`): Global Constraints (Keys, Language, Reserved pages), Review Focus #2, and "Interfaces between phases" (Phase 7). Spec §5 (Focus and keys, Search) and §6 (Page header, Page numbers, Minitel bar, Sub-pages).

**This phase is split in two** to keep each part within the five-file rule: 7a is the pure page model, 7b the state and the keys. Run 7a's gate, wait for approval, then 7b.

| Part | Files (5 at most, tests included) |
|---|---|
| 7a | Create `apps/macos/Ressac/Antiope/PageDirectory.swift`, create `apps/macos/Ressac/Antiope/PageEntry.swift`, create `apps/macos/RessacTests/PageDirectoryTests.swift` (3 files) |
| 7b | Create `apps/macos/Ressac/Antiope/AntiopeState.swift`, modify `apps/macos/Ressac/Reading/ReadingKeyRouter.swift`, modify `apps/macos/Ressac/App/AppState.swift`, modify `apps/macos/Ressac/Reading/MainView.swift`, modify `apps/macos/RessacTests/KeyboardTests.swift` (5 files) |

**Prerequisites** (earlier phases, used here by their pinned names):
- Phase 4: `Appearance` (`.native`, `.antiope`, `CaseIterable`) in `Settings/Appearance.swift`.
- Phase 6: `FolderItem.page: Int?` and `FeedItem.page: Int?`, filled by `Queries.sidebar`; `tests/fixtures/contract.sqlite` at v3 (phase 1b), so `TestDatabase` feeds carry page numbers.

**Test command** (narrow it with one `-only-testing:` per suite):

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)/apps/macos" && xcodegen generate --quiet && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test \
    -only-testing:RessacTests/PageDirectoryTests \
  2>&1 | grep -E '^/.+: error:|✘|Test run|\*\* TEST'
```

`xcodegen` picks up the new `Antiope/` folder by itself (the target's sources are the `Ressac` directory).

**Step 0 rule.** `AppState.swift` is 647 lines, but 7b does not restructure it: it adds one `switch` arm to `perform(_:)` and a counter to the reading-window registration. No dead-code commit is needed. `ReadingKeyRouter.swift` (172 lines) and `MainView.swift` (247 lines) are under 300.

**Discrepancies with the pinned interfaces** (each is a plan change to copy into the index table when this phase is approved):

1. **`ReadingCommand` also gains `.nextIndexRow` and `.previousIndexRow`.** The native sidebar is an `NSOutlineView` that moves its own selection with `↑`/`↓`; the Antiope index column is SwiftUI rows with no AppKit table under them, so without these two commands the index could not be walked from the keyboard. They are produced only in Antiope, only when the index column has the focus.
2. **`ReadingKeys.key(for:)` accepts Shift on digits and on `?`.** On Clément's French AZERTY keyboard the top row gives `&é"'(§è!çà` and the digits need Shift; `?` is Shift-comma. The old rule (no Shift at all) would have made page numbers untypeable there. Shift is still refused on letters.
3. **`AntiopeState` adds:** `directory: PageDirectory`, `currentPage: Int?`, `closeSearchPrompt(clearing:)`, `imageToggle: (@MainActor () -> Void)?` (phase 11 sets it), a static `notFoundDuration`, and conforms to `KeyFocusProvider` itself (the "Antiope focus provider" of spec §5). `isSearchPromptOpen` is a computed property over `AppState.isSearchFocused`, so the existing ⌘F menu item opens the prompt and the letter shortcuts stay off while it is open, without a second source of truth.
4. **`ResponderFocusProvider` exposes** `focus(of:in:)` (what the router's `focus(of:)` did) and `static func isTextInput(_:)`.
5. **`ReadingWindowReader` moves** from a private struct of `MainView.swift` to an internal struct of `ReadingKeyRouter.swift`, built as `ReadingWindowReader(makeRouter:)`, so that `AntiopeRootView` (phase 8) uses the same window registration. `AppState` counts the readers holding each window: during a live Appearance switch SwiftUI may add the new root's reader before the old one leaves, and the old one's unregistration must not switch the letters off.
6. **`AntiopeState.handle(_:)` declines** `.pageCorrect`/`.pageCancel` when no number is being typed, `.repetition` while a long run is active, `.toggleImages` without an article and a reader hook, and the column moves at either end, so that the key goes on as with `AppState.perform(_:)`.

---

# Part 7a: the page model

### Task 7a.1: `ReservedPage`, `PageDestination` and `PageDirectory`

**Files:**
- Create: `apps/macos/Ressac/Antiope/PageDirectory.swift`
- Test: `apps/macos/RessacTests/PageDirectoryTests.swift`

**Interfaces:**
- Consumes: `SidebarSnapshot`, `FolderItem` (`page`), `FeedItem` (`page`), `Source`, `SidebarNode.tree(from:)` (`kind`, `unread`, `children`, `source`, `title`).
- Produces:
  - `enum ReservedPage: Int, CaseIterable, Sendable` with `title`, `destination: PageDestination`, `count(in: SidebarSnapshot) -> Int`, `isGarden: Bool`;
  - `enum PageDestination: Equatable, Sendable { case source(Source), index, guide }`;
  - `struct PageDirectory: Equatable, Sendable` with `Row`, `init(snapshot:)`, `rows`, `destination(for:)`, `number(for:)`, `page(forFeed:)`.

**Rules:**
- `rows`: the eight reserved pages in `ReservedPage.allCases` order (100, 101, 102, 103, 800, 801, 802, 899), then every folder and feed with a page number, depth-first in display order (`SidebarNode.tree`, the same order as the core's assignment). A node without a number yet is skipped, its children are still listed. A number that collides with a reserved page (a corrupt database) is ignored.
- `Row.depth`: 0 for reserved pages and root items, +1 per folder level. `Row.unread`: a folder's subtree sum (as the sidebar), a feed's unread count; for reserved pages `count(in:)` (Unread, Today, Starred counts; P800's triage count; 0 for the others).
- Buried feeds are not in the sidebar snapshot (`state = 'active'`), so they have no destination here; phase 12 lists them in P802 from the garden query.

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/PageDirectoryTests.swift`:

```swift
import Testing

@testable import Ressac

/// A small library with page numbers: Blogs (200) with xkcd (201); Infos (400) with Daring
/// Fireball (401) and the subfolder Science (402) holding one feed (403); a feed without a folder
/// (110) and one the core has not numbered yet.
func pagedSnapshot() -> SidebarSnapshot {
    func folder(_ id: Int64, _ name: String, parent: Int64? = nil, position: Int, page: Int?) -> FolderItem {
        var item = FolderItem(folderId: id, parentId: parent, name: name, position: position)
        item.page = page
        return item
    }
    func feed(_ id: Int64, _ title: String, folder: Int64?, position: Int, unread: Int, page: Int?) -> FeedItem {
        var item = FeedItem(feedId: id, title: title, folderId: folder, position: position, unread: unread,
                            status: nil, preferFullText: false, htmlUrl: nil)
        item.page = page
        return item
    }
    return SidebarSnapshot(
        folders: [
            folder(1, "Blogs", position: 0, page: 200),
            folder(4, "Infos", position: 2, page: 400),
            folder(5, "Science", parent: 4, position: 3, page: 402),
        ],
        feeds: [
            feed(1, "xkcd.com", folder: 1, position: 0, unread: 3, page: 201),
            feed(8, "Daring Fireball", folder: 4, position: 0, unread: 2, page: 401),
            feed(10, "Center for Humane Technology", folder: 5, position: 0, unread: 1, page: 403),
            feed(13, "Loose feed", folder: nil, position: 5, unread: 0, page: 110),
            feed(12, "Brand new", folder: nil, position: 9, unread: 4, page: nil),
        ],
        unread: 10, today: 1, starred: 4, needsTriage: 7)
}

@Suite struct PageDirectoryTests {
    @Test func reservedPagesComeFirstWithTheirCounts() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows
        #expect(rows.prefix(8).map(\.number) == [100, 101, 102, 103, 800, 801, 802, 899])
        #expect(rows.prefix(8).map(\.title) == [
            "Index", "Unread", "Today", "Starred", "Needs triage", "All feeds", "Cemetery", "Guide",
        ])
        #expect(rows.prefix(8).map(\.unread) == [0, 10, 1, 4, 7, 0, 0, 0])
        #expect(rows.prefix(8).allSatisfy { $0.isReserved && $0.depth == 0 })
    }

    @Test func foldersAndFeedsFollowInDisplayOrderWithTheirDepth() {
        let rows = PageDirectory(snapshot: pagedSnapshot()).rows.dropFirst(8)
        #expect(rows.map(\.number) == [200, 201, 400, 401, 402, 403, 110])
        #expect(rows.map(\.title) == [
            "Blogs", "xkcd.com", "Infos", "Daring Fireball", "Science", "Center for Humane Technology", "Loose feed",
        ])
        #expect(rows.map(\.depth) == [0, 1, 0, 1, 1, 2, 0])
        #expect(rows.map(\.unread) == [3, 3, 3, 2, 1, 1, 0])  // folders sum their subtree
        #expect(rows.allSatisfy { !$0.isReserved })
    }

    @Test func numbersLeadToTheirDestinations() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        #expect(directory.destination(for: 100) == .index)
        #expect(directory.destination(for: 899) == .guide)
        #expect(directory.destination(for: 101) == .source(.unread))
        #expect(directory.destination(for: 102) == .source(.today))
        #expect(directory.destination(for: 103) == .source(.starred))
        #expect(directory.destination(for: 800) == .source(.garden(.needsTriage)))
        #expect(directory.destination(for: 801) == .source(.garden(.all)))
        #expect(directory.destination(for: 802) == .source(.garden(.cemetery)))
        #expect(directory.destination(for: 401) == .source(.feed(8)))
        #expect(directory.destination(for: 402) == .source(.folder(5)))
        #expect(directory.destination(for: 110) == .source(.feed(13)))
        for unknown in [0, 7, 99, 437, 799, 900, 999] {
            #expect(directory.destination(for: unknown) == nil)
        }
    }

    @Test func sourcesAndFeedsFindTheirNumbers() {
        let directory = PageDirectory(snapshot: pagedSnapshot())
        #expect(directory.number(for: .unread) == 101)
        #expect(directory.number(for: .garden(.cemetery)) == 802)
        #expect(directory.number(for: .folder(4)) == 400)
        #expect(directory.number(for: .feed(10)) == 403)
        #expect(directory.number(for: .feed(12)) == nil)  // not numbered yet
        #expect(directory.page(forFeed: 8) == 401)
        #expect(directory.page(forFeed: 12) == nil)
        #expect(directory.page(forFeed: 999) == nil)
        #expect(directory == PageDirectory(snapshot: pagedSnapshot()))
    }

    @Test func aNumberCollidingWithAReservedPageIsIgnored() {
        var snapshot = pagedSnapshot()
        snapshot.feeds[3].page = 101  // only a corrupt database could hold this
        let directory = PageDirectory(snapshot: snapshot)
        #expect(directory.destination(for: 101) == .source(.unread))
        #expect(directory.rows.filter { $0.number == 101 }.count == 1)
        #expect(directory.number(for: .feed(13)) == nil)
    }
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/PageDirectoryTests`. Expected: `error: cannot find 'PageDirectory' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/PageDirectory.swift`:

```swift
import Foundation

/// Page numbers fixed in code (spec §4, Reserved pages): never stored, refused by `ressac page set`.
enum ReservedPage: Int, CaseIterable, Sendable {
    case index = 100, unread = 101, today = 102, starred = 103
    case needsTriage = 800, allFeeds = 801, cemetery = 802, guide = 899

    /// The core's titles (`pages::list`), so that the app and `ressac page list` agree.
    var title: String {
        switch self {
        case .index: "Index"
        case .unread: "Unread"
        case .today: "Today"
        case .starred: "Starred"
        case .needsTriage: "Needs triage"
        case .allFeeds: "All feeds"
        case .cemetery: "Cemetery"
        case .guide: "Guide"
        }
    }

    var destination: PageDestination {
        switch self {
        case .index: .index
        case .unread: .source(.unread)
        case .today: .source(.today)
        case .starred: .source(.starred)
        case .needsTriage: .source(.garden(.needsTriage))
        case .allFeeds: .source(.garden(.all))
        case .cemetery: .source(.garden(.cemetery))
        case .guide: .guide
        }
    }

    /// The count shown beside the page: entries for the smart lists, feeds to triage for P800.
    func count(in snapshot: SidebarSnapshot) -> Int {
        switch self {
        case .unread: snapshot.unread
        case .today: snapshot.today
        case .starred: snapshot.starred
        case .needsTriage: snapshot.needsTriage
        case .index, .allFeeds, .cemetery, .guide: 0
        }
    }

    /// The Garden's three pages, drawn in magenta in the index (spec §5).
    var isGarden: Bool {
        switch self {
        case .needsTriage, .allFeeds, .cemetery: true
        case .index, .unread, .today, .starred, .guide: false
        }
    }
}

/// Where a page number leads: a source of the window, or one of the two pages drawn in the
/// reader column (P100 SOMMAIRE, P899 GUIDE).
enum PageDestination: Equatable, Sendable {
    case source(Source)
    case index
    case guide
}

/// Every page of the library (spec §6, Page numbers): the reserved pages, then the folders and
/// feeds the core numbered (`folder.page`, `feed.page`), in the sidebar's display order.
struct PageDirectory: Equatable, Sendable {
    struct Row: Equatable, Sendable {
        var number: Int
        var title: String
        /// Unread entries (a folder sums its subtree), or the reserved page's `count(in:)`.
        var unread: Int
        var isReserved: Bool
        /// 0 for reserved pages and root items, one more per folder level.
        var depth: Int
    }

    /// Reserved pages, then folders and feeds depth-first in display order.
    let rows: [Row]
    private let destinations: [Int: PageDestination]
    private let numbers: [Source: Int]
    private let feedPages: [Int64: Int]

    init(snapshot: SidebarSnapshot) {
        var rows: [Row] = []
        var destinations: [Int: PageDestination] = [:]
        var numbers: [Source: Int] = [:]
        var feedPages: [Int64: Int] = [:]
        for page in ReservedPage.allCases {
            rows.append(Row(number: page.rawValue, title: page.title, unread: page.count(in: snapshot),
                            isReserved: true, depth: 0))
            destinations[page.rawValue] = page.destination
            if case let .source(source) = page.destination { numbers[source] = page.rawValue }
        }
        func walk(_ nodes: [SidebarNode], depth: Int) {
            for node in nodes {
                // A node not numbered yet is skipped; a reserved number can only come from a
                // corrupt database and never hides the reserved page.
                if let number = node.page, ReservedPage(rawValue: number) == nil {
                    rows.append(Row(number: number, title: node.title, unread: node.unread, isReserved: false, depth: depth))
                    destinations[number] = .source(node.source)
                    numbers[node.source] = number
                    if case let .feed(feed) = node.kind { feedPages[feed.feedId] = number }
                }
                walk(node.children ?? [], depth: depth + 1)
            }
        }
        walk(SidebarNode.tree(from: snapshot), depth: 0)
        self.rows = rows
        self.destinations = destinations
        self.numbers = numbers
        self.feedPages = feedPages
    }

    /// Nil for a number no page holds: the header then shows `NOT FOUND`.
    func destination(for number: Int) -> PageDestination? {
        destinations[number]
    }

    /// The page of a source, reserved ones included; nil while a folder or feed has no number.
    func number(for source: Source) -> Int? {
        numbers[source]
    }

    /// A feed's page, for the reader's sub-page line (`412 3/20`, phase 10).
    func page(forFeed feedId: Int64) -> Int? {
        feedPages[feedId]
    }
}

private extension SidebarNode {
    var page: Int? {
        switch kind {
        case let .folder(folder): folder.page
        case let .feed(feed): feed.page
        }
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/PageDirectoryTests`. Expected: `✔ Test run with 5 tests in 1 suite passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/PageDirectory.swift apps/macos/RessacTests/PageDirectoryTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): page directory with reserved pages and numbered folders and feeds

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 7a.2: `PageEntry`, the three-digit state machine

**Files:**
- Create: `apps/macos/Ressac/Antiope/PageEntry.swift`
- Test: `apps/macos/RessacTests/PageDirectoryTests.swift` (append a suite)

**Interfaces:**
- Consumes: nothing.
- Produces: `struct PageEntry: Equatable, Sendable` with `Outcome { pending, complete(Int) }`, `private(set) var digits: String`, `display`, `isActive`, `mutating type(_:) -> Outcome`, `mutating correct()`, `mutating cancel()`; `PageEntry()` is idle.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/PageDirectoryTests.swift`:

```swift
@Suite struct PageEntryTests {
    @Test func theThirdDigitCompletesTheNumber() {
        var entry = PageEntry()
        #expect(!entry.isActive && entry.display == "")
        #expect(entry.type(4) == .pending)
        #expect(entry.isActive && entry.display == "4__")
        #expect(entry.type(1) == .pending)
        #expect(entry.display == "41_")
        #expect(entry.type(2) == .complete(412))
        #expect(!entry.isActive && entry.display == "" && entry.digits == "")
        #expect(entry.type(8) == .pending)  // the next digit starts a new number
        #expect(entry.display == "8__")
    }

    @Test func correctionErasesTheLastDigit() {
        var entry = PageEntry()
        entry.correct()  // nothing typed: nothing happens
        #expect(!entry.isActive)
        _ = entry.type(4)
        _ = entry.type(1)
        entry.correct()
        #expect(entry.display == "4__")
        entry.correct()
        #expect(!entry.isActive)
        _ = entry.type(8)
        _ = entry.type(0)
        #expect(entry.type(1) == .complete(801))
    }

    @Test func cancelEmptiesTheEntry() {
        var entry = PageEntry()
        _ = entry.type(4)
        _ = entry.type(1)
        entry.cancel()
        #expect(entry == PageEntry())
        #expect(entry.type(1) == .pending)
    }

    @Test func leadingZerosAndStrayValues() {
        var entry = PageEntry()
        #expect(entry.type(12) == .pending)  // not a digit: ignored
        #expect(entry.type(-1) == .pending)
        #expect(!entry.isActive)
        _ = entry.type(0)
        _ = entry.type(4)
        #expect(entry.display == "04_")
        #expect(entry.type(2) == .complete(42))  // no page has it: the caller shows NOT FOUND
    }
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/PageEntryTests`. Expected: `error: cannot find 'PageEntry' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/PageEntry.swift`:

```swift
import Foundation

/// The three-digit page number being typed (spec §6, Page numbers): a digit starts it, the third
/// one completes it; ⌫ (CORRECTION) erases the last digit, Esc (ANNULATION) cancels it.
struct PageEntry: Equatable, Sendable {
    enum Outcome: Equatable, Sendable {
        case pending
        case complete(Int)
    }

    static let length = 3

    private(set) var digits = ""

    /// The digits typed so far, padded with `_` ("4__", "41_"), for the header; empty when idle.
    var display: String {
        isActive ? digits + String(repeating: "_", count: Self.length - digits.count) : ""
    }

    var isActive: Bool { !digits.isEmpty }

    /// Adds `digit` (0...9; any other value is ignored). The third digit completes the number and
    /// leaves the entry idle again.
    mutating func type(_ digit: Int) -> Outcome {
        guard (0...9).contains(digit) else { return .pending }
        digits.append(String(digit))
        guard digits.count == Self.length else { return .pending }
        let number = digits.reduce(0) { $0 * 10 + ($1.wholeNumberValue ?? 0) }
        digits = ""
        return .complete(number)
    }

    mutating func correct() {
        if !digits.isEmpty { digits.removeLast() }
    }

    mutating func cancel() {
        digits = ""
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/PageDirectoryTests -only-testing:RessacTests/PageEntryTests`. Expected: `✔ Test run with 9 tests in 2 suites passed`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/PageEntry.swift apps/macos/RessacTests/PageDirectoryTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): three-digit page entry with correction and cancel

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Verification gate (7a)

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

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` (9 more Swift tests than after phase 6), `no warnings in our sources`. Push, then wait for Clément's approval before 7b.

---

# Part 7b: `AntiopeState` and the keys

### Task 7b.1: Antiope keys, commands and the focus provider protocol

**Files:**
- Modify: `apps/macos/Ressac/Reading/ReadingKeyRouter.swift` (whole file below)
- Modify: `apps/macos/Ressac/App/AppState.swift:304-310` (`perform(_:isRepeat:)`)
- Test: `apps/macos/RessacTests/KeyboardTests.swift` (whole file below)

**Interfaces:**
- Consumes: `Appearance` (phase 4), `AppState.perform(_:isRepeat:)`.
- Produces:
  - `ReadingKeys.Key` gains `.digit(Int)`, `.delete`, `.escape`, `.option(Character)`, `.question`;
  - `ReadingCommand` gains `.pageDigit(Int)`, `.pageCorrect`, `.pageCancel`, `.sommaire`, `.guide`, `.repetition`, `.toggleImages`, `.nextColumn`, `.previousColumn`, `.nextIndexRow`, `.previousIndexRow`;
  - `ReadingKeys.command(for:in:appearance:)` replaces `command(for:in:)`;
  - `@MainActor protocol KeyFocusProvider: AnyObject { func keyFocus(in window: NSWindow?) -> KeyFocus }`;
  - `@MainActor final class ResponderFocusProvider: KeyFocusProvider` with `focus(of:in:)` and `static func isTextInput(_:)`;
  - `ReadingKeyRouter` keeps `init(state:)` in this task and reads its focus through a `ResponderFocusProvider` (task 7b.3 changes the initialiser).

**Key rules (Global Constraints, Keys):**
- Every look: a text field keeps every key (`KeyFocus.textInput` gives nil, Review Focus #2). `j`, `k`, `m`, `s`, Space and the arrows of the native rules keep their meaning; `r`, `b`, `u`, `o`, `w` stay unbound so the Garden's own keys reach it.
- Antiope only: digits → `.pageDigit`, `⌫` → `.pageCorrect`, `Esc` → `.pageCancel`, `⌥S` → `.sommaire`, `⌥G` and `?` → `.guide`, `⌥R` → `.repetition`, `i` → `.toggleImages` (in every focus but text, a focused button included: "a digit typed outside a text field starts the entry"); `→`/`←` → `.nextColumn`/`.previousColumn` in the three columns (a focused button keeps its arrows); `↓`/`↑` in the index column → `.nextIndexRow`/`.previousIndexRow`. Antiope's rules are tried before the native ones, so `→` never becomes `.focusEntryList` there.
- Native: none of the new commands is ever produced.

- [ ] **Step 1: Write the failing tests.** Replace `apps/macos/RessacTests/KeyboardTests.swift` with:

```swift
import AppKit
import Testing
import WebKit

@testable import Ressac

/// The reading window's keys are ruled by `ReadingKeyRouter` before AppKit offers them to the
/// focused view (acceptance fix 1 and 3): the sidebar's and the list's type-select and the
/// reader's own Space scrolling used to take `j`, `k` and Space before the Go menu saw them.
@MainActor @Suite struct ReadingKeysTests {
    static let readingFocuses: [KeyFocus] = [.sidebar, .entryList, .reader, .window]
    static let antiopeOnlyKeys: [ReadingKeys.Key] = (0...9).map(ReadingKeys.Key.digit)
        + [.delete, .escape, .option("s"), .option("g"), .option("r"), .question, .letter("i")]

    @Test func lettersAndSpaceActEverywhereButInTextFields() {
        for appearance in Appearance.allCases {
            for focus in Self.readingFocuses {
                #expect(ReadingKeys.command(for: .letter("j"), in: focus, appearance: appearance) == .nextEntry)
                #expect(ReadingKeys.command(for: .letter("k"), in: focus, appearance: appearance) == .previousEntry)
                #expect(ReadingKeys.command(for: .letter("m"), in: focus, appearance: appearance) == .toggleRead)
                #expect(ReadingKeys.command(for: .letter("s"), in: focus, appearance: appearance) == .toggleStar)
                #expect(ReadingKeys.command(for: .space, in: focus, appearance: appearance) == .advance)
            }
            for key in [ReadingKeys.Key.letter("j"), .letter("k"), .space, .up, .down, .left, .right] {
                #expect(ReadingKeys.command(for: key, in: .textInput, appearance: appearance) == nil)
            }
            #expect(ReadingKeys.command(for: .letter("x"), in: .entryList, appearance: appearance) == nil)
            #expect(ReadingKeys.command(for: .space, in: .control, appearance: appearance) == nil)  // a focused button keeps Space
        }
    }

    @Test func arrowsFollowTheFocus() {
        for focus in [KeyFocus.entryList, .reader] {
            #expect(ReadingKeys.command(for: .down, in: focus, appearance: .native) == .nextEntry)
            #expect(ReadingKeys.command(for: .up, in: focus, appearance: .native) == .previousEntry)
        }
        #expect(ReadingKeys.command(for: .down, in: .sidebar, appearance: .native) == nil)  // the sidebar's own arrows
        #expect(ReadingKeys.command(for: .up, in: .sidebar, appearance: .native) == nil)
        #expect(ReadingKeys.command(for: .right, in: .sidebar, appearance: .native) == .focusEntryList)
        #expect(ReadingKeys.command(for: .left, in: .entryList, appearance: .native) == .focusSidebar)
        #expect(ReadingKeys.command(for: .left, in: .sidebar, appearance: .native) == nil)
        #expect(ReadingKeys.command(for: .right, in: .entryList, appearance: .native) == nil)
        #expect(ReadingKeys.command(for: .left, in: .reader, appearance: .native) == nil)
    }

    @Test func modifiedKeysAreLeftAlone() {
        #expect(ReadingKeys.key(for: keyEvent("j")) == .letter("j"))
        #expect(ReadingKeys.key(for: keyEvent(" ")) == .space)
        #expect(ReadingKeys.key(for: arrowEvent(NSDownArrowFunctionKey)) == .down)
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: .command)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("J", modifiers: .shift)) == nil)
        #expect(ReadingKeys.key(for: arrowEvent(NSDownArrowFunctionKey, modifiers: .shift)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("J", modifiers: .capsLock)) == .letter("j"))
        // A posted event carries a private flag (bit 29): it must not hide the key.
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: NSEvent.ModifierFlags(rawValue: 1 << 29))) == .letter("j"))
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: .control)) == nil)
        // ⌥ and a letter is a key Antiope may use; ⌥J is none of them, in either look.
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: .option)) == .option("j"))
        for appearance in Appearance.allCases {
            #expect(ReadingKeys.command(for: .option("j"), in: .entryList, appearance: appearance) == nil)
        }
    }

    @Test func antiopeKeysAreRecognised() {
        #expect(ReadingKeys.key(for: keyEvent("4")) == .digit(4))
        #expect(ReadingKeys.key(for: keyEvent("4", modifiers: .shift)) == .digit(4))  // AZERTY top row
        #expect(ReadingKeys.key(for: keyEvent("0", modifiers: .numericPad)) == .digit(0))  // keypad
        #expect(ReadingKeys.key(for: keyEvent("4", modifiers: .command)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("4", modifiers: .option)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("\u{7F}")) == .delete)
        #expect(ReadingKeys.key(for: keyEvent("\u{7F}", modifiers: .option)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("\u{1B}")) == .escape)
        #expect(ReadingKeys.key(for: keyEvent("s", modifiers: .option)) == .option("s"))
        #expect(ReadingKeys.key(for: keyEvent("S", modifiers: [.option, .shift])) == nil)
        #expect(ReadingKeys.key(for: keyEvent("g", modifiers: [.option, .command])) == nil)
        #expect(ReadingKeys.key(for: keyEvent("?", modifiers: .shift)) == .question)
        #expect(ReadingKeys.key(for: keyEvent("?")) == .question)
        #expect(ReadingKeys.key(for: keyEvent("?", modifiers: .command)) == nil)
    }

    @Test func antiopeKeysMapToTheirCommands() {
        for focus in Self.readingFocuses + [.control] {
            for digit in 0...9 {
                #expect(ReadingKeys.command(for: .digit(digit), in: focus, appearance: .antiope) == .pageDigit(digit))
            }
            #expect(ReadingKeys.command(for: .delete, in: focus, appearance: .antiope) == .pageCorrect)
            #expect(ReadingKeys.command(for: .escape, in: focus, appearance: .antiope) == .pageCancel)
            #expect(ReadingKeys.command(for: .option("s"), in: focus, appearance: .antiope) == .sommaire)
            #expect(ReadingKeys.command(for: .option("g"), in: focus, appearance: .antiope) == .guide)
            #expect(ReadingKeys.command(for: .question, in: focus, appearance: .antiope) == .guide)
            #expect(ReadingKeys.command(for: .option("r"), in: focus, appearance: .antiope) == .repetition)
            #expect(ReadingKeys.command(for: .letter("i"), in: focus, appearance: .antiope) == .toggleImages)
        }
        for focus in Self.readingFocuses {
            #expect(ReadingKeys.command(for: .right, in: focus, appearance: .antiope) == .nextColumn)
            #expect(ReadingKeys.command(for: .left, in: focus, appearance: .antiope) == .previousColumn)
        }
        #expect(ReadingKeys.command(for: .right, in: .control, appearance: .antiope) == nil)
        #expect(ReadingKeys.command(for: .down, in: .sidebar, appearance: .antiope) == .nextIndexRow)
        #expect(ReadingKeys.command(for: .up, in: .sidebar, appearance: .antiope) == .previousIndexRow)
        #expect(ReadingKeys.command(for: .down, in: .entryList, appearance: .antiope) == .nextEntry)
        #expect(ReadingKeys.command(for: .up, in: .reader, appearance: .antiope) == .previousEntry)
        #expect(ReadingKeys.command(for: .option("x"), in: .reader, appearance: .antiope) == nil)
    }

    /// Review Focus #2: digits typed in the search prompt or any other text field are text.
    @Test func digitsInATextFieldNeverBecomePageNumbers() {
        for appearance in Appearance.allCases {
            for key in Self.antiopeOnlyKeys + [.left, .right, .up, .down] {
                #expect(ReadingKeys.command(for: key, in: .textInput, appearance: appearance) == nil)
            }
        }
    }

    @Test func nativeNeverProducesAnAntiopeCommand() {
        let nativeCommands: [ReadingCommand] = [
            .nextEntry, .previousEntry, .advance, .toggleRead, .toggleStar, .focusEntryList, .focusSidebar,
        ]
        let alphabet = "abcdefghijklmnopqrstuvwxyz"
        let keys: [ReadingKeys.Key] = alphabet.map(ReadingKeys.Key.letter) + alphabet.map(ReadingKeys.Key.option)
            + Self.antiopeOnlyKeys + [.space, .up, .down, .left, .right]
        for key in keys {
            for focus in Self.readingFocuses + [.textInput, .control] {
                if let command = ReadingKeys.command(for: key, in: focus, appearance: .native) {
                    #expect(nativeCommands.contains(command), "\(key) in \(focus) gave \(command)")
                }
            }
        }
        for key in Self.antiopeOnlyKeys {
            #expect(ReadingKeys.command(for: key, in: .entryList, appearance: .native) == nil)
        }
    }

    /// No clash with `j`, `k`, `m`, `s` or the Garden's keys (Global Constraints, Keys).
    @Test func readingAndGardenLettersKeepTheirMeaningInAntiope() {
        for focus in [KeyFocus.entryList, .reader] {
            for letter: Character in ["j", "k", "m", "s"] {
                #expect(ReadingKeys.command(for: .letter(letter), in: focus, appearance: .antiope)
                    == ReadingKeys.command(for: .letter(letter), in: focus, appearance: .native))
            }
            for letter: Character in ["r", "b", "u", "o", "w"] {
                #expect(ReadingKeys.command(for: .letter(letter), in: focus, appearance: .antiope) == nil)
            }
        }
    }
}

@MainActor @Suite struct ReadingKeyRouterTests {
    /// A window laid out like `MainView`: sidebar, list and reader columns in one split view.
    @MainActor final class Harness {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 500),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let sidebar = NSOutlineView()
        let list = NSTableView()
        let search = NSSearchField()
        let reader = WKWebView()

        init() {
            let split = NSSplitView()
            split.isVertical = true
            let listColumn = NSView()
            listColumn.addSubview(search)
            listColumn.addSubview(Self.scrolled(list))
            for pane in [Self.scrolled(sidebar), listColumn, reader] { split.addArrangedSubview(pane) }
            window.contentView = split
        }

        static func scrolled(_ table: NSTableView) -> NSView {
            table.addTableColumn(NSTableColumn(identifier: .init("c")))
            let scroll = NSScrollView()
            scroll.documentView = table
            let wrapper = NSView()  // NavigationSplitView wraps each column too
            wrapper.addSubview(scroll)
            return wrapper
        }
    }

    func makeRouter() async throws -> (ReadingKeyRouter, Harness, AppState, TestDatabase, [Int64]) {
        let (state, test, _) = try makeState()
        let older = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        let newer = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.autoReadDelay = .seconds(60)
        let harness = Harness()
        let router = ReadingKeyRouter(state: state)
        router.window = harness.window
        return (router, harness, state, test, [newer, older])
    }

    @Test func theFocusedViewIsClassifiedByColumn() async throws {
        let (_, h, _, _, _) = try await makeRouter()
        let provider = ResponderFocusProvider()
        #expect(provider.focus(of: h.sidebar, in: h.window) == .sidebar)
        #expect(provider.focus(of: h.list, in: h.window) == .entryList)
        #expect(provider.focus(of: h.reader, in: h.window) == .reader)
        #expect(provider.focus(of: h.window, in: h.window) == .window)
        #expect(provider.focus(of: nil, in: h.window) == .window)
        #expect(h.window.makeFirstResponder(h.search))
        #expect(provider.keyFocus(in: h.window) == .textInput)  // the field editor
    }

    @Test func spaceInTheReaderMovesOnInsteadOfReachingTheWebView() async throws {
        let (router, h, state, _, ids) = try await makeRouter()
        state.selectedEntryId = ids[0]
        #expect(h.window.makeFirstResponder(h.reader))
        #expect(router.handle(keyEvent(" ", window: h.window)))
        #expect(await eventually { state.selectedEntryId == ids[1] })
    }

    @Test func jInTheSidebarIsNextEntryNotTypeSelect() async throws {
        let (router, h, state, _, ids) = try await makeRouter()
        #expect(h.window.makeFirstResponder(h.sidebar))
        #expect(router.handle(keyEvent("j", window: h.window)))
        #expect(state.selectedEntryId == ids[0])
        #expect(router.handle(keyEvent("k", window: h.window)))
        #expect(state.selectedEntryId == ids[0])
    }

    @Test func arrowsInTheListMoveAndMarkReadLikeJ() async throws {
        let (router, h, state, test, ids) = try await makeRouter()
        state.selectedEntryId = ids[0]
        #expect(await eventually { state.selectedEntry != nil })
        #expect(h.window.makeFirstResponder(h.list))
        #expect(router.handle(arrowEvent(NSDownArrowFunctionKey, window: h.window)))
        #expect(state.selectedEntryId == ids[1])
        #expect(await eventually { (try? test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [ids[0]])) != nil })
        #expect(router.handle(arrowEvent(NSUpArrowFunctionKey, window: h.window)))
        #expect(state.selectedEntryId == ids[0])
    }

    @Test func rightFromTheSidebarFocusesTheListOnItsFirstEntry() async throws {
        let (router, h, state, _, ids) = try await makeRouter()
        #expect(h.window.makeFirstResponder(h.sidebar))
        #expect(router.handle(arrowEvent(NSRightArrowFunctionKey, window: h.window)))
        #expect(h.window.firstResponder === h.list)
        #expect(state.selectedEntryId == ids[0])
        state.selectedEntryId = ids[1]
        #expect(h.window.makeFirstResponder(h.sidebar))
        #expect(router.handle(arrowEvent(NSRightArrowFunctionKey, window: h.window)))
        #expect(state.selectedEntryId == ids[1])  // an existing selection is kept
        #expect(router.handle(arrowEvent(NSLeftArrowFunctionKey, window: h.window)))
        #expect(h.window.firstResponder === h.sidebar)
    }

    @Test func textFieldsAndOtherWindowsKeepTheirKeys() async throws {
        let (router, h, state, _, _) = try await makeRouter()
        #expect(h.window.makeFirstResponder(h.search))
        for event in [keyEvent("j", window: h.window), keyEvent(" ", window: h.window),
                      arrowEvent(NSDownArrowFunctionKey, window: h.window), arrowEvent(NSLeftArrowFunctionKey, window: h.window)] {
            #expect(!router.handle(event))
        }
        #expect(state.selectedEntryId == nil)
        let other = NSWindow(contentRect: .zero, styleMask: [.titled], backing: .buffered, defer: false)
        #expect(h.window.makeFirstResponder(h.list))
        #expect(!router.handle(keyEvent("j", window: other)))
        #expect(state.selectedEntryId == nil)
    }

    @Test func heldKeysNeitherToggleNorCrossToTheNextEntry() async throws {
        let (router, h, state, test, ids) = try await makeRouter()
        state.selectedEntryId = ids[0]
        #expect(await eventually { state.selectedEntry != nil })
        state.readerPageDown = { _ in false }  // at the bottom
        #expect(h.window.makeFirstResponder(h.reader))
        for key in ["m", "s", " "] {
            #expect(router.handle(keyEvent(key, window: h.window, isARepeat: true)))  // spent, so no menu acts
        }
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.selectedEntryId == ids[0])
        #expect(try test.one(String.self, "SELECT read_at FROM entry WHERE id = ?", [ids[0]]) == nil)
        #expect(state.selectedEntry?.isStarred == false)
    }

    @Test func theGardenKeepsItsOwnKeys() async throws {
        let (router, h, state, _, _) = try await makeRouter()
        state.selection = .garden(.needsTriage)
        #expect(h.window.makeFirstResponder(h.list))
        for event in [keyEvent("k", window: h.window), keyEvent(" ", window: h.window),
                      arrowEvent(NSDownArrowFunctionKey, window: h.window)] {
            #expect(!router.handle(event))  // `k` is Keep, the arrows move in the table
        }
        // The arrows between the columns work in the Garden too (Minor 5).
        #expect(h.window.makeFirstResponder(h.sidebar))
        #expect(router.handle(arrowEvent(NSRightArrowFunctionKey, window: h.window)))
        #expect(h.window.firstResponder === h.list)
        #expect(state.selectedEntryId == nil)  // no entry to pick in a Garden scope
        #expect(router.handle(arrowEvent(NSLeftArrowFunctionKey, window: h.window)))
        #expect(h.window.firstResponder === h.sidebar)
    }

    /// The native window binds no Antiope key: the list's type-select and the menus keep them.
    @Test func digitsAndAntiopeKeysPassThroughTheNativeWindow() async throws {
        let (router, h, state, _, _) = try await makeRouter()
        #expect(h.window.makeFirstResponder(h.list))
        for event in [keyEvent("4", window: h.window), keyEvent("4", modifiers: .shift, window: h.window),
                      keyEvent("s", modifiers: .option, window: h.window), keyEvent("?", modifiers: .shift, window: h.window),
                      keyEvent("i", window: h.window), keyEvent("\u{1B}", window: h.window)] {
            #expect(!router.handle(event))
        }
        #expect(state.selection == .unread && state.selectedEntryId == nil)
    }
}

@MainActor
func keyEvent(_ characters: String, modifiers: NSEvent.ModifierFlags = [], window: NSWindow? = nil,
              isARepeat: Bool = false) -> NSEvent {
    NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                     windowNumber: window?.windowNumber ?? 0, context: nil, characters: characters,
                     charactersIgnoringModifiers: characters, isARepeat: isARepeat, keyCode: 0)!
}

@MainActor
func arrowEvent(_ key: Int, modifiers: NSEvent.ModifierFlags = [], window: NSWindow? = nil) -> NSEvent {
    let characters = String(Character(UnicodeScalar(UInt16(key))!))
    return NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers.union([.numericPad, .function]),
                            timestamp: 0, windowNumber: window?.windowNumber ?? 0, context: nil, characters: characters,
                            charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 0)!
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/ReadingKeysTests -only-testing:RessacTests/ReadingKeyRouterTests`. Expected: errors such as `type 'ReadingKeys.Key' has no member 'digit'` and `cannot find 'ResponderFocusProvider' in scope`.

- [ ] **Step 3: Implement the keys.** Replace `apps/macos/Ressac/Reading/ReadingKeyRouter.swift` with:

```swift
import AppKit
import WebKit

/// What a reading key asks the window to do (spec §7, Keyboard; Antiope spec §6).
enum ReadingCommand: Equatable, Sendable {
    case nextEntry, previousEntry, advance, toggleRead, toggleStar
    /// `→` in the sidebar: the list takes the keyboard, on its first entry when none is selected
    /// (in a Garden scope, the Garden table takes it).
    case focusEntryList
    /// `←` in the list: back to the sidebar.
    case focusSidebar

    // Antiope only: `ReadingKeys` never produces these in Native, `AntiopeState.handle(_:)`
    // performs them and `AppState.perform(_:isRepeat:)` declines them.

    /// A digit of a page number.
    case pageDigit(Int)
    /// `⌫`, CORRECTION: erases the last digit typed.
    case pageCorrect
    /// `Esc`, ANNULATION: cancels the page number.
    case pageCancel
    /// `⌥S`: P100.
    case sommaire
    /// `⌥G` or `?`: P899, the key help.
    case guide
    /// `⌥R`: refreshes the selection, as ⌘R.
    case repetition
    /// `i`: swaps every image of the article between its mosaic and the original.
    case toggleImages
    /// `→` and `←`: the next or previous of the three columns.
    case nextColumn, previousColumn
    /// `↓` and `↑` in the index column: the next or previous page of the index.
    case nextIndexRow, previousIndexRow
}

/// The part of the reading window that has the keyboard.
enum KeyFocus: Equatable, Sendable {
    case sidebar, entryList, reader
    /// No view has the keyboard: the window itself is the first responder.
    case window
    /// The search field or any other text: every key is typing.
    case textInput
    /// Another control (a toolbar button reached with full keyboard access…).
    case control
}

/// Tells the router which part of the window has the keyboard. The native window asks AppKit's
/// responder chain (`ResponderFocusProvider`); Antiope keeps its own column (`AntiopeState`).
@MainActor protocol KeyFocusProvider: AnyObject {
    func keyFocus(in window: NSWindow?) -> KeyFocus
}

/// The native window's focus: the first responder's column in `MainView`'s split view.
@MainActor final class ResponderFocusProvider: KeyFocusProvider {
    func keyFocus(in window: NSWindow?) -> KeyFocus {
        focus(of: window?.firstResponder, in: window)
    }

    func focus(of responder: NSResponder?, in window: NSWindow?) -> KeyFocus {
        guard let responder, responder !== window else { return .window }
        if Self.isTextInput(responder) { return .textInput }
        guard let view = responder as? NSView else { return .control }
        if MainColumns.ancestors(of: view).contains(where: { $0 is WKWebView }) { return .reader }
        switch MainColumns.column(of: view, in: window) {
        case MainColumns.sidebar?: return .sidebar
        case MainColumns.list?: return view is NSTableView ? .entryList : .control
        case MainColumns.reader?: return .reader
        default: return .control
        }
    }

    /// A field editor or a text field: every key is typing there.
    static func isTextInput(_ responder: NSResponder?) -> Bool {
        responder is NSText || responder is NSTextField
    }
}

/// The single-key commands of the reading window and where each one applies.
enum ReadingKeys {
    enum Key: Equatable, Sendable {
        case letter(Character)
        case space, up, down, left, right
        /// 0...9, from the keypad, or from the top row with or without Shift (a French AZERTY
        /// keyboard needs Shift for digits).
        case digit(Int)
        /// `⌫`, the Delete key.
        case delete
        case escape
        /// `⌥` and a key, lowercased (`charactersIgnoringModifiers` gives the unshifted letter).
        case option(Character)
        /// `?`, Shift included.
        case question
    }

    /// The key of a press, or nil for the chords this window leaves to the menus and the views.
    static func key(for event: NSEvent) -> Key? {
        // Only the modifiers a person holds count: arrows carry `.numericPad` and `.function`,
        // and posted events can carry private bits.
        let held = event.modifierFlags.intersection([.shift, .control, .option, .command])
        switch event.specialKey {
        case .upArrow?: return held.isEmpty ? .up : nil
        case .downArrow?: return held.isEmpty ? .down : nil
        case .leftArrow?: return held.isEmpty ? .left : nil
        case .rightArrow?: return held.isEmpty ? .right : nil
        default: break
        }
        guard let characters = event.charactersIgnoringModifiers, characters.count == 1,
              let character = characters.first
        else { return nil }
        switch character {
        case "\u{7F}": return held.isEmpty ? .delete : nil
        case "\u{1B}": return held.isEmpty ? .escape : nil
        case "?": return held.isSubset(of: .shift) ? .question : nil
        default: break
        }
        if character.isASCII, let digit = character.wholeNumberValue {
            return held.isSubset(of: .shift) ? .digit(digit) : nil
        }
        guard event.specialKey == nil, let lowered = character.lowercased().first else { return nil }
        if held == .option { return lowered.isLetter ? .option(lowered) : nil }
        guard held.isEmpty else { return nil }
        return lowered == " " ? .space : .letter(lowered)
    }

    /// NetNewsWire's rules, in both looks: letters and Space act wherever the reading window is not
    /// typing; `↓`/`↑` are `j`/`k` in the list and the reader, the sidebar keeps its own; `→`
    /// leaves the sidebar for the list and `←` comes back. Antiope's keys (spec §6) are tried
    /// first in Antiope and never exist in Native.
    static func command(for key: Key, in focus: KeyFocus, appearance: Appearance) -> ReadingCommand? {
        guard focus != .textInput else { return nil }
        if appearance == .antiope, let command = antiopeCommand(for: key, in: focus) { return command }
        return nativeCommand(for: key, in: focus)
    }

    private static func nativeCommand(for key: Key, in focus: KeyFocus) -> ReadingCommand? {
        switch (key, focus) {
        case (.letter("j"), _): .nextEntry
        case (.letter("k"), _): .previousEntry
        case (.letter("m"), _): .toggleRead
        case (.letter("s"), _): .toggleStar
        case (.space, .sidebar), (.space, .entryList), (.space, .reader), (.space, .window): .advance
        case (.down, .entryList), (.down, .reader): .nextEntry
        case (.up, .entryList), (.up, .reader): .previousEntry
        case (.right, .sidebar): .focusEntryList
        case (.left, .entryList): .focusSidebar
        default: nil
        }
    }

    /// A digit typed outside a text field starts a page number, even with a button focused; the
    /// arrows stay with a focused button.
    private static func antiopeCommand(for key: Key, in focus: KeyFocus) -> ReadingCommand? {
        let inColumn = focus != .control
        switch key {
        case let .digit(digit): return .pageDigit(digit)
        case .delete: return .pageCorrect
        case .escape: return .pageCancel
        case .option("s"): return .sommaire
        case .option("g"), .question: return .guide
        case .option("r"): return .repetition
        case .letter("i"): return .toggleImages
        case .right: return inColumn ? .nextColumn : nil
        case .left: return inColumn ? .previousColumn : nil
        case .down: return focus == .sidebar ? .nextIndexRow : nil
        case .up: return focus == .sidebar ? .previousIndexRow : nil
        default: return nil
        }
    }
}

/// Rules the reading window's keys before AppKit offers them to the focused view, the way
/// NetNewsWire's window controller does. Menu key equivalents without a modifier are not
/// enough: AppKit tries them only once the first responder has declined the key, and the
/// sidebar and the list (type-select, Space) and the reader (WebKit scrolls with Space, even
/// at the bottom) each keep some of `j`, `k` and Space for themselves (acceptance fix 1).
@MainActor final class ReadingKeyRouter {
    weak var window: NSWindow?
    let state: AppState
    let focus: any KeyFocusProvider = ResponderFocusProvider()
    private var monitor: Any?

    init(state: AppState) {
        self.state = state
    }

    isolated deinit {
        stop()
    }

    /// Starts routing the application's key presses (only this window's are taken).
    func start() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let handled = MainActor.assumeIsolated { self?.handle(event) ?? false }
            return handled ? nil : event
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    /// True when the key was used, so that nothing else receives it. A command that cannot act
    /// here (the Garden, an empty list) lets the key go on to the focused view.
    func handle(_ event: NSEvent) -> Bool {
        guard let window, event.type == .keyDown, event.windowNumber == window.windowNumber,
              let key = ReadingKeys.key(for: event),
              let command = ReadingKeys.command(for: key, in: focus.keyFocus(in: window), appearance: .native),
              state.perform(command, isRepeat: event.isARepeat)
        else { return false }
        switch command {
        case .focusEntryList: focusTable(inColumn: MainColumns.list, of: window)
        case .focusSidebar: focusTable(inColumn: MainColumns.sidebar, of: window)
        default: break
        }
        return true
    }

    private func focusTable(inColumn index: Int, of window: NSWindow) {
        guard let columns = MainColumns.columns(in: window), columns.indices.contains(index),
              let table = MainColumns.firstTable(in: columns[index])
        else { return }
        window.makeFirstResponder(table)
    }
}

/// The three columns of `MainView`'s `NavigationSplitView`, found in the window's view tree.
@MainActor private enum MainColumns {
    static let sidebar = 0, list = 1, reader = 2

    static func ancestors(of view: NSView) -> some Sequence<NSView> {
        sequence(first: view, next: \.superview)
    }

    /// The three-column split view of the window.
    static func columns(in window: NSWindow?) -> [NSView]? {
        guard let root = window?.contentView else { return nil }
        var pending = [root]
        while !pending.isEmpty {
            let view = pending.removeFirst()
            if let split = view as? NSSplitView, split.arrangedSubviews.count == 3 { return split.arrangedSubviews }
            pending += view.subviews
        }
        return nil
    }

    static func column(of view: NSView, in window: NSWindow?) -> Int? {
        guard let columns = columns(in: window) else { return nil }
        return ancestors(of: view).lazy.compactMap { ancestor in columns.firstIndex { $0 === ancestor } }.first
    }

    static func firstTable(in root: NSView) -> NSTableView? {
        var pending = [root]
        while !pending.isEmpty {
            let view = pending.removeFirst()
            if let table = view as? NSTableView { return table }
            pending += view.subviews
        }
        return nil
    }
}
```

- [ ] **Step 4: Make `AppState.perform` exhaustive again.** In `apps/macos/Ressac/App/AppState.swift`, replace:

```swift
        case .focusSidebar:
            break
        }
        return true
    }
```

with:

```swift
        case .focusSidebar:
            break
        case .pageDigit, .pageCorrect, .pageCancel, .sommaire, .guide, .repetition, .toggleImages,
             .nextColumn, .previousColumn, .nextIndexRow, .previousIndexRow:
            // Antiope's own keys: `AntiopeState.handle(_:)` had them first and declined them,
            // so the key goes on to the focused view.
            return false
        }
        return true
    }
```

- [ ] **Step 5: Run** with `-only-testing:RessacTests/ReadingKeysTests -only-testing:RessacTests/ReadingKeyRouterTests`. Expected: `✔ Test run with 17 tests in 2 suites passed`.

- [ ] **Step 6: Commit.**

```sh
git add apps/macos/Ressac/Reading/ReadingKeyRouter.swift apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/KeyboardTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): page-number and Minitel keys, focus provider protocol

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 7b.2: `AntiopeState`

**Files:**
- Create: `apps/macos/Ressac/Antiope/AntiopeState.swift`
- Test: `apps/macos/RessacTests/KeyboardTests.swift` (append `ManualClock` and `AntiopeStateTests`)

**Interfaces:**
- Consumes: `PageDirectory`, `ReservedPage`, `PageEntry` (7a); `ReadingCommand`, `KeyFocus`, `KeyFocusProvider`, `ResponderFocusProvider.isTextInput(_:)` (7b.1); `AppState` (`sidebar`, `selection`, `selectedEntry`, `canNavigateEntries`, `canRefresh`, `refreshCurrent()`, `perform(_:isRepeat:)`, `isSearchFocused`, `searchText`).
- Produces (later phases rely on these):
  - `enum AntiopeColumn: Sendable { case index, entries, reader }` with `next` and `previous`;
  - `enum ReaderPage: Equatable, Sendable { case article, index, guide }`;
  - `@MainActor @Observable final class AntiopeState: KeyFocusProvider`:
    - pinned: `column`, `entry`, `notFound`, `readerPage`, `isSearchPromptOpen`, `init(app:clock:)`, `handle(_:) -> Bool`, `jump(to:)`;
    - added: `let app: AppState`, `static let notFoundDuration: Duration` (2 s), `var directory: PageDirectory`, `var currentPage: Int?`, `func closeSearchPrompt(clearing: Bool)`, `var imageToggle: (@MainActor () -> Void)?`, `func keyFocus(in:) -> KeyFocus`.

**Rules:**
- `jump(to:)`: cancels any typed number. Unknown number: `notFound` is set, the selection stays, and a task on the injected clock clears `notFound` after `notFoundDuration` (spec §10). Known number: the source becomes `app.selection` (the entry list follows through `AppState`), the reader shows the article, the index column takes the focus. P100 and P899 switch `readerPage` and keep the selection.
- A digit clears a `NOT FOUND` at once (the header shows the new digits).
- SUITE and RETOUR are today's `.advance`, `.nextEntry` and `.previousEntry`: `AntiopeState` never consumes them, `AppState` performs them, so Space still scrolls the article before moving on. When the reader shows P100 or P899, they first bring the article back.
- RÉPÉTITION is `app.refreshCurrent()` (⌘R's command), declined while a long run is active (`canRefresh`).
- `→` into the entry column selects the first entry when none is selected, as `→` from the native sidebar does (`app.perform(.focusEntryList)`).
- `↓`/`↑` in the index walk `directory.rows` from `currentPage`; the first and last pages spend the key without moving.
- `keyFocus(in:)`: `.textInput` while the search prompt is open or a text view is first responder; else `.sidebar`, `.entryList` or `.reader` for the index, entry and reader columns.

- [ ] **Step 1: Write the failing tests.** Append to `apps/macos/RessacTests/KeyboardTests.swift`:

```swift
/// A clock that moves only when the test advances it (the 2-second `NOT FOUND`).
final class ManualClock: Clock, @unchecked Sendable {
    struct Instant: InstantProtocol {
        var offset: Swift.Duration

        func advanced(by duration: Swift.Duration) -> Instant { Instant(offset: offset + duration) }
        func duration(to other: Instant) -> Swift.Duration { other.offset - offset }
        static func < (a: Instant, b: Instant) -> Bool { a.offset < b.offset }
    }

    private let lock = NSLock()
    private var current = Instant(offset: .zero)
    private var sleepers: [(deadline: Instant, continuation: CheckedContinuation<Void, any Error>)] = []

    var now: Instant { lock.withLock { current } }
    var minimumResolution: Swift.Duration { .zero }
    /// Tasks waiting on this clock: a test advances it only once the sleeper is in place.
    var sleeperCount: Int { lock.withLock { sleepers.count } }

    func sleep(until deadline: Instant, tolerance: Swift.Duration? = nil) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            let isDue = lock.withLock {
                guard deadline > current else { return true }
                sleepers.append((deadline, continuation))
                return false
            }
            if isDue { continuation.resume() }
        }
    }

    func advance(by duration: Swift.Duration) {
        let due = lock.withLock {
            current = current.advanced(by: duration)
            let due = sleepers.filter { $0.deadline <= current }
            sleepers.removeAll { $0.deadline <= current }
            return due
        }
        for sleeper in due { sleeper.continuation.resume() }
    }
}

@MainActor @Suite struct AntiopeStateTests {
    func make(clock: any Clock<Duration> = ContinuousClock()) throws -> (AntiopeState, AppState, TestDatabase, Stub) {
        let (state, test, stub) = try makeState()
        return (AntiopeState(app: state, clock: clock), state, test, stub)
    }

    /// Types a page number digit by digit, as the keys would.
    func dial(_ number: String, into antiope: AntiopeState) {
        for digit in number.compactMap(\.wholeNumberValue) {
            #expect(antiope.handle(.pageDigit(digit)))
        }
    }

    func feedPage(_ id: Int64, in state: AppState) throws -> Int {
        try #require(state.sidebar.feeds.first { $0.feedId == id }?.page)
    }

    @Test func aTypedPageSelectsItsSourceAndFocusesTheIndex() throws {
        let (antiope, state, test, _) = try make()
        let entry = try test.entry(feed: 8)
        let page = try String(feedPage(8, in: state))
        antiope.column = .reader
        dial(String(page.prefix(2)), into: antiope)
        #expect(antiope.entry.display == String(page.prefix(2)) + "_")
        #expect(state.selection == .unread)  // nothing happens before the third digit
        dial(String(page.suffix(1)), into: antiope)
        #expect(state.selection == .feed(8))
        #expect(state.entries.map(\.entryId) == [entry])  // the entry list follows
        #expect(antiope.column == .index && antiope.readerPage == .article)
        #expect(antiope.currentPage.map { String($0) } == page)
        #expect(!antiope.entry.isActive && antiope.notFound == nil)
    }

    @Test func correctionAndCancelEditTheEntry() throws {
        let (antiope, _, _, _) = try make()
        #expect(!antiope.handle(.pageCorrect))  // nothing typed: the key goes on
        #expect(!antiope.handle(.pageCancel))
        dial("41", into: antiope)
        #expect(antiope.handle(.pageCorrect))
        #expect(antiope.entry.display == "4__")
        #expect(antiope.handle(.pageCancel))
        #expect(!antiope.entry.isActive && antiope.entry.display == "")
    }

    @Test func anUnknownPageShowsNotFoundForTwoSeconds() async throws {
        let clock = ManualClock()
        let (antiope, state, _, _) = try make(clock: clock)
        #expect(antiope.directory.destination(for: 437) == nil)
        dial("437", into: antiope)
        #expect(antiope.notFound == 437)
        #expect(state.selection == .unread && antiope.currentPage == 101)  // selection unchanged
        #expect(!antiope.entry.isActive)
        #expect(await eventually { clock.sleeperCount == 1 })
        clock.advance(by: .milliseconds(1999))
        try await Task.sleep(for: .milliseconds(50))
        #expect(antiope.notFound == 437)
        clock.advance(by: .milliseconds(1))
        #expect(await eventually { antiope.notFound == nil })
    }

    @Test func aNewDigitClearsNotFoundAtOnce() async throws {
        let clock = ManualClock()
        let (antiope, _, _, _) = try make(clock: clock)
        dial("437", into: antiope)
        #expect(await eventually { clock.sleeperCount == 1 })
        dial("1", into: antiope)
        #expect(antiope.notFound == nil && antiope.entry.display == "1__")
        clock.advance(by: .seconds(2))  // the cancelled reset must not touch a later NOT FOUND
        #expect(antiope.directory.destination(for: 199) == nil)
        dial("99", into: antiope)
        #expect(antiope.notFound == 199)
        try await Task.sleep(for: .milliseconds(50))
        #expect(antiope.notFound == 199)
        #expect(await eventually { clock.sleeperCount == 1 })
        clock.advance(by: .seconds(2))  // lets the last reset run, so no continuation leaks
        #expect(await eventually { antiope.notFound == nil })
    }

    @Test func reservedPagesOpenTheirDestination() throws {
        let (antiope, state, _, _) = try make()
        antiope.jump(to: 102)
        #expect(state.selection == .today)
        antiope.jump(to: 800)
        #expect(state.selection == .garden(.needsTriage) && antiope.currentPage == 800)
        antiope.jump(to: 100)
        #expect(antiope.readerPage == .index && antiope.currentPage == 100)
        #expect(state.selection == .garden(.needsTriage))  // P100 keeps the selection
        #expect(antiope.handle(.guide))
        #expect(antiope.readerPage == .guide && antiope.currentPage == 899)
        #expect(antiope.handle(.sommaire))
        #expect(antiope.readerPage == .index)
        antiope.jump(to: 103)
        #expect(state.selection == .starred && antiope.readerPage == .article)
    }

    @Test func suiteAndRetourStayWithAppStateAndBringTheArticleBack() async throws {
        let (antiope, state, test, _) = try make()
        let newer = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        let older = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.autoReadDelay = .seconds(60)
        #expect(antiope.handle(.guide))
        #expect(!antiope.handle(.nextEntry))  // not Antiope's: the router hands it to AppState
        #expect(antiope.readerPage == .article)
        #expect(state.perform(.nextEntry))
        #expect(state.selectedEntryId == newer)
        #expect(!antiope.handle(.advance) && !antiope.handle(.previousEntry))
        #expect(state.perform(.nextEntry))
        #expect(state.selectedEntryId == older)
    }

    @Test func repetitionRefreshesTheSelection() async throws {
        let (antiope, state, _, stub) = try make()
        state.selection = .feed(3)
        #expect(antiope.handle(.repetition))
        #expect(await eventually { (try? calls(stub))?.isEmpty == false })
        #expect(try calls(stub).map { $0.split(separator: " ").dropFirst(3).joined(separator: " ") } == ["fetch --feed 3"])
    }

    @Test func imagesToggleOnlyOnAnArticleWithAReader() async throws {
        let (antiope, state, test, _) = try make()
        var toggles = 0
        #expect(!antiope.handle(.toggleImages))  // no Antiope reader yet
        antiope.imageToggle = { toggles += 1 }
        #expect(!antiope.handle(.toggleImages))  // no entry selected
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = id
        #expect(await eventually { state.selectedEntry != nil })
        #expect(antiope.handle(.toggleImages))
        #expect(toggles == 1)
        #expect(antiope.handle(.guide))
        #expect(!antiope.handle(.toggleImages))  // P899 has no images
        #expect(toggles == 1)
    }

    @Test func theIndexArrowsWalkThePages() throws {
        let (antiope, state, _, _) = try make()
        #expect(antiope.handle(.previousIndexRow))  // 101 → 100
        #expect(antiope.readerPage == .index && antiope.currentPage == 100)
        #expect(antiope.handle(.previousIndexRow))  // the first page: spent, no move
        #expect(antiope.currentPage == 100)
        #expect(antiope.handle(.nextIndexRow))
        #expect(state.selection == .unread && antiope.readerPage == .article)
        #expect(antiope.handle(.nextIndexRow))
        #expect(state.selection == .today && antiope.column == .index)
    }

    @Test func columnsMoveLeftAndRight() async throws {
        let (antiope, state, test, _) = try make()
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.autoReadDelay = .seconds(60)
        #expect(!antiope.handle(.previousColumn))  // the index is the first column
        #expect(antiope.handle(.nextColumn))
        #expect(antiope.column == .entries && state.selectedEntryId == id)  // as → from the native sidebar
        #expect(antiope.handle(.nextColumn))
        #expect(antiope.column == .reader)
        #expect(!antiope.handle(.nextColumn))  // the reader is the last
        #expect(antiope.handle(.previousColumn))
        #expect(antiope.column == .entries)
    }

    @Test func theFocusFollowsTheColumnAndThePrompt() throws {
        let (antiope, state, _, _) = try make()
        #expect(antiope.keyFocus(in: nil) == .sidebar)
        antiope.column = .entries
        #expect(antiope.keyFocus(in: nil) == .entryList)
        antiope.column = .reader
        #expect(antiope.keyFocus(in: nil) == .reader)
        state.isSearchFocused = true  // ⌘F, through the Search menu item
        #expect(antiope.isSearchPromptOpen && antiope.keyFocus(in: nil) == .textInput)
        state.searchText = "tidal"
        antiope.closeSearchPrompt(clearing: false)  // Return
        #expect(!antiope.isSearchPromptOpen && state.searchText == "tidal" && state.letterShortcutsEnabled)
        antiope.isSearchPromptOpen = true
        antiope.closeSearchPrompt(clearing: true)  // Esc
        #expect(!state.isSearchFocused && state.searchText.isEmpty)
    }
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/AntiopeStateTests`. Expected: `error: cannot find 'AntiopeState' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Antiope/AntiopeState.swift`:

```swift
import AppKit
import Observation

/// The three columns of the Antiope window, left to right (spec §5, Focus and keys).
enum AntiopeColumn: Sendable {
    case index, entries, reader

    var next: AntiopeColumn? {
        switch self {
        case .index: .entries
        case .entries: .reader
        case .reader: nil
        }
    }

    var previous: AntiopeColumn? {
        switch self {
        case .index: nil
        case .entries: .index
        case .reader: .entries
        }
    }
}

/// What the reader column shows: the selected entry, P100 SOMMAIRE or P899 GUIDE.
enum ReaderPage: Equatable, Sendable {
    case article, index, guide
}

/// What only the Antiope look needs, beside the shared `AppState` (spec §5, approach A): the
/// focused column, the page number being typed, `NOT FOUND`, the page in the reader column. It is
/// also that window's `KeyFocusProvider`, and `ReadingKeyRouter` offers it every command first.
@MainActor @Observable final class AntiopeState {
    /// How long an unknown page shows `NOT FOUND` in the header (spec §6, §10).
    static let notFoundDuration: Duration = .seconds(2)

    var column: AntiopeColumn = .index
    var entry = PageEntry()
    /// The unknown page just typed, shown in red in the header until `notFoundDuration` passes.
    var notFound: Int?
    var readerPage: ReaderPage = .article
    /// The `SEARCH?` prompt of the Minitel bar. It is `AppState.isSearchFocused`, so ⌘F (the
    /// Search menu item) opens it and the letter shortcuts stay off while it is open.
    var isSearchPromptOpen: Bool {
        get { app.isSearchFocused }
        set { app.isSearchFocused = newValue }
    }
    /// Set by the Antiope reader (phase 11): swaps every image of the article between its
    /// mosaic and the original. `i` does nothing while it is nil.
    @ObservationIgnored var imageToggle: (@MainActor () -> Void)?

    let app: AppState
    private let clock: any Clock<Duration>
    @ObservationIgnored private var notFoundReset: Task<Void, Never>?

    init(app: AppState, clock: any Clock<Duration> = ContinuousClock()) {
        self.app = app
        self.clock = clock
    }

    /// Every page of the library, from the live sidebar snapshot.
    var directory: PageDirectory { PageDirectory(snapshot: app.sidebar) }

    /// The page on screen: P100 or P899 in the reader column, else the selection's page (nil
    /// while the selected folder or feed has no number yet).
    var currentPage: Int? {
        switch readerPage {
        case .index: ReservedPage.index.rawValue
        case .guide: ReservedPage.guide.rawValue
        case .article: directory.number(for: app.selection)
        }
    }

    /// Goes to a page (spec §6): its row is selected in the index, the entry list follows, the
    /// index column takes the focus; P100 and P899 open in the reader. An unknown number shows
    /// `NOT FOUND` and changes nothing else.
    func jump(to number: Int) {
        entry.cancel()
        guard let destination = directory.destination(for: number) else {
            showNotFound(number)
            return
        }
        clearNotFound()
        switch destination {
        case let .source(source):
            app.selection = source
            readerPage = .article
        case .index:
            readerPage = .index
        case .guide:
            readerPage = .guide
        }
        column = .index
    }

    /// True when Antiope used the command; false hands it to `AppState.perform(_:isRepeat:)`,
    /// then to the focused view.
    func handle(_ command: ReadingCommand) -> Bool {
        switch command {
        case let .pageDigit(digit):
            clearNotFound()
            if case let .complete(number) = entry.type(digit) { jump(to: number) }
            return true
        case .pageCorrect:
            guard entry.isActive else { return false }
            entry.correct()
            return true
        case .pageCancel:
            guard entry.isActive else { return false }
            entry.cancel()
            return true
        case .sommaire:
            jump(to: ReservedPage.index.rawValue)
            return true
        case .guide:
            jump(to: ReservedPage.guide.rawValue)
            return true
        case .repetition:
            guard app.canRefresh else { return false }
            app.refreshCurrent()
            return true
        case .toggleImages:
            guard readerPage == .article, app.selectedEntry != nil, let imageToggle else { return false }
            imageToggle()
            return true
        case .nextColumn:
            return move(to: column.next)
        case .previousColumn:
            return move(to: column.previous)
        case .nextIndexRow:
            return moveInIndex(by: 1)
        case .previousIndexRow:
            return moveInIndex(by: -1)
        case .nextEntry, .previousEntry, .advance:
            // SUITE and RETOUR are today's commands and `AppState` performs them (Space still
            // scrolls the article first). Leaving P100 or P899 brings the article back.
            if readerPage != .article, app.canNavigateEntries { readerPage = .article }
            return false
        case .toggleRead, .toggleStar, .focusEntryList, .focusSidebar:
            return false
        }
    }

    /// Return keeps the search, Esc clears it (spec §5, Search); both close the prompt. The root
    /// view also closes it when Antiope appears and goes away, so that neither look inherits an
    /// open prompt (Review Focus #5).
    func closeSearchPrompt(clearing: Bool) {
        if clearing { app.searchText = "" }
        isSearchPromptOpen = false
    }

    // MARK: Private

    private func move(to target: AntiopeColumn?) -> Bool {
        guard let target else { return false }
        if target == .entries {
            // As `→` from the native sidebar: the list takes the keyboard on its first entry.
            app.perform(.focusEntryList)
        }
        column = target
        return true
    }

    private func moveInIndex(by offset: Int) -> Bool {
        let rows = directory.rows
        let current = currentPage.flatMap { page in rows.firstIndex { $0.number == page } }
        let target = current.map { $0 + offset } ?? 0
        if rows.indices.contains(target) { jump(to: rows[target].number) }
        return true
    }

    private func showNotFound(_ number: Int) {
        notFoundReset?.cancel()
        notFound = number
        notFoundReset = Task { [weak self, clock] in
            try? await clock.sleep(for: Self.notFoundDuration)
            guard !Task.isCancelled else { return }
            self?.notFound = nil
        }
    }

    private func clearNotFound() {
        notFoundReset?.cancel()
        notFoundReset = nil
        notFound = nil
    }
}

extension AntiopeState: KeyFocusProvider {
    /// The focused column, or `.textInput` while the search prompt or any text view has the
    /// keyboard: digits typed there never start a page number (Review Focus #2).
    func keyFocus(in window: NSWindow?) -> KeyFocus {
        if isSearchPromptOpen || ResponderFocusProvider.isTextInput(window?.firstResponder) { return .textInput }
        switch column {
        case .index: return .sidebar
        case .entries: return .entryList
        case .reader: return .reader
        }
    }
}
```

- [ ] **Step 4: Run** with `-only-testing:RessacTests/AntiopeStateTests`. Expected: `✔ Test run with 11 tests in 1 suite passed`. If `anUnknownPageShowsNotFoundForTwoSeconds` waits forever on `sleeperCount`, check that `showNotFound` calls `clock.sleep(for:)` on the injected clock, not `Task.sleep`.

- [ ] **Step 5: Commit.**

```sh
git add apps/macos/Ressac/Antiope/AntiopeState.swift apps/macos/RessacTests/KeyboardTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): window state with page jumps, NOT FOUND, columns and search prompt

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Task 7b.3: the router offers commands to Antiope first; one reading-window reader for both looks

**Files:**
- Modify: `apps/macos/Ressac/Reading/ReadingKeyRouter.swift` (the `ReadingKeyRouter` class, `import SwiftUI`, and the new `ReadingWindowReader` at the end)
- Modify: `apps/macos/Ressac/Reading/MainView.swift:28` and `:206-247` (the private `ReadingWindowReader` goes)
- Modify: `apps/macos/Ressac/App/AppState.swift:121-122` and `:192-204` (reading-window holds)
- Test: `apps/macos/RessacTests/KeyboardTests.swift` (`makeRouter`, plus `AntiopeKeyRouterTests` and `ReadingWindowTests`)

**Interfaces:**
- Consumes: `AntiopeState` (7b.2), `KeyFocusProvider`, `ResponderFocusProvider`.
- Produces:
  - `ReadingKeyRouter(state: AppState, focus: any KeyFocusProvider, antiope: AntiopeState?)`;
  - `struct ReadingWindowReader: NSViewRepresentable`, `init(makeRouter: @escaping @MainActor () -> ReadingKeyRouter)`, with `WindowView` and `WindowView.leave()`. Phase 8 puts `ReadingWindowReader { ReadingKeyRouter(state: state, focus: antiope, antiope: antiope) }` behind `AntiopeRootView`.

**Rules:**
- The router asks `focus.keyFocus(in:)`; the look is Antiope exactly when `antiope` is not nil.
- A held key that acts once per press (digits, Esc, ⌥S, ⌥G, `?`, ⌥R, `i`, `←`, `→`) is spent without acting: holding `4` must not type `444`. `⌫` and the index arrows repeat.
- Each command goes to `antiope?.handle(_:)` first, then to `state.perform(_:isRepeat:)`.
- A window stays a reading window while any reader holds it; each `WindowView` unregisters its window exactly once.

- [ ] **Step 1: Write the failing tests.** In `apps/macos/RessacTests/KeyboardTests.swift`, in `ReadingKeyRouterTests.makeRouter()`, replace:

```swift
        let router = ReadingKeyRouter(state: state)
```

with:

```swift
        let router = ReadingKeyRouter(state: state, focus: ResponderFocusProvider(), antiope: nil)
```

Then append:

```swift
/// The Antiope window: no split view, the focus is `AntiopeState.column`.
@MainActor @Suite struct AntiopeKeyRouterTests {
    func makeRouter() throws -> (ReadingKeyRouter, NSWindow, NSTextField, AntiopeState, AppState, TestDatabase) {
        let (state, test, _) = try makeState()
        let antiope = AntiopeState(app: state)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 500),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let field = NSTextField()
        window.contentView = NSView()
        window.contentView?.addSubview(field)
        let router = ReadingKeyRouter(state: state, focus: antiope, antiope: antiope)
        router.window = window
        return (router, window, field, antiope, state, test)
    }

    func press(_ digits: String, _ router: ReadingKeyRouter, _ window: NSWindow) -> [Bool] {
        digits.map { router.handle(keyEvent(String($0), window: window)) }
    }

    @Test func typedDigitsJumpToTheFeedsPage() throws {
        let (router, window, _, antiope, state, _) = try makeRouter()
        let page = try #require(state.sidebar.feeds.first { $0.feedId == 8 }?.page)
        antiope.column = .reader
        #expect(press(String(page), router, window) == [true, true, true])
        #expect(state.selection == .feed(8))
        #expect(antiope.column == .index)
    }

    /// Review Focus #2 at the router: a focused text field, or the open prompt, keeps the digits.
    @Test func digitsGoToAFocusedTextFieldOrTheOpenPrompt() throws {
        let (router, window, field, antiope, state, _) = try makeRouter()
        #expect(window.makeFirstResponder(field))
        #expect(press("401", router, window) == [false, false, false])
        #expect(!antiope.entry.isActive && state.selection == .unread)
        #expect(window.makeFirstResponder(nil))
        antiope.isSearchPromptOpen = true
        #expect(press("401", router, window) == [false, false, false])
        #expect(!antiope.entry.isActive && state.selection == .unread)
        #expect(!router.handle(keyEvent("s", modifiers: .option, window: window)))
        #expect(antiope.readerPage == .article)
    }

    @Test func aHeldDigitTypesOnce() throws {
        let (router, window, _, antiope, _, _) = try makeRouter()
        #expect(router.handle(keyEvent("4", window: window)))
        #expect(router.handle(keyEvent("4", window: window, isARepeat: true)))  // spent
        #expect(antiope.entry.display == "4__")
        #expect(router.handle(keyEvent("\u{7F}", window: window)))
        #expect(!antiope.entry.isActive)
        #expect(!router.handle(keyEvent("\u{1B}", window: window)))  // nothing to cancel: the key goes on
    }

    @Test func arrowsMoveBetweenTheColumnsAndThroughTheIndex() async throws {
        let (router, window, _, antiope, state, test) = try makeRouter()
        let id = try test.entry(feed: 1)
        try await state.database.notifyExternalChanges()
        #expect(await eventually { !state.entries.isEmpty })
        state.autoReadDelay = .seconds(60)
        #expect(router.handle(arrowEvent(NSDownArrowFunctionKey, window: window)))  // 101 → 102
        #expect(state.selection == .today)
        #expect(router.handle(arrowEvent(NSUpArrowFunctionKey, window: window)))
        #expect(state.selection == .unread)
        #expect(router.handle(arrowEvent(NSRightArrowFunctionKey, window: window)))
        #expect(antiope.column == .entries && state.selectedEntryId == id)
        #expect(router.handle(arrowEvent(NSRightArrowFunctionKey, window: window)))
        #expect(antiope.column == .reader)
        #expect(!router.handle(arrowEvent(NSRightArrowFunctionKey, window: window)))  // nothing further right
        #expect(router.handle(arrowEvent(NSLeftArrowFunctionKey, window: window)))
        #expect(antiope.column == .entries)
    }

    @Test func optionKeysOpenTheSpecialPages() throws {
        let (router, window, _, antiope, _, _) = try makeRouter()
        #expect(router.handle(keyEvent("g", modifiers: .option, window: window)))
        #expect(antiope.readerPage == .guide)
        #expect(router.handle(keyEvent("s", modifiers: .option, window: window)))
        #expect(antiope.readerPage == .index && antiope.currentPage == 100)
        #expect(router.handle(keyEvent("?", modifiers: .shift, window: window)))
        #expect(antiope.readerPage == .guide)
    }
}

@MainActor @Suite struct ReadingWindowTests {
    /// A live Appearance switch may add the new look's reader before the old one leaves: the
    /// window must stay a reading window until the last reader is gone.
    @Test func aWindowStaysReadingUntilTheLastReaderLeaves() throws {
        let (state, _, _) = try makeState()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let other = NSWindow(contentRect: .zero, styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSView()
        func reader() -> ReadingWindowReader.WindowView {
            ReadingWindowReader.WindowView(router: ReadingKeyRouter(state: state, focus: ResponderFocusProvider(), antiope: nil))
        }
        let old = reader()
        let new = reader()
        window.contentView?.addSubview(old)
        window.contentView?.addSubview(new)  // the new look arrives first
        old.removeFromSuperview()
        ReadingWindowReader.dismantleNSView(old, coordinator: ())  // SwiftUI dismantles it too: no second unregistration
        state.windowDidChangeKey(other, isKey: true)
        #expect(!state.letterShortcutsEnabled)  // another window has the keyboard
        state.windowDidChangeKey(window, isKey: true)
        #expect(state.letterShortcutsEnabled)  // still a reading window
        new.removeFromSuperview()
        state.windowDidChangeKey(window, isKey: true)
        #expect(!state.letterShortcutsEnabled)  // no reading window left: the letters are off
    }
}
```

- [ ] **Step 2: Run** with `-only-testing:RessacTests/AntiopeKeyRouterTests -only-testing:RessacTests/ReadingWindowTests`. Expected: `error: extra arguments at positions #2, #3 in call` (the router's initialiser) and `'ReadingWindowReader' is inaccessible due to 'private' protection level` or `cannot find 'ReadingWindowReader' in scope`.

- [ ] **Step 3: Implement the router.** In `apps/macos/Ressac/Reading/ReadingKeyRouter.swift`, add `import SwiftUI` after `import AppKit`, and replace the whole `ReadingKeyRouter` class with:

```swift
/// Rules the reading window's keys before AppKit offers them to the focused view, the way
/// NetNewsWire's window controller does. Menu key equivalents without a modifier are not
/// enough: AppKit tries them only once the first responder has declined the key, and the
/// sidebar and the list (type-select, Space) and the reader (WebKit scrolls with Space, even
/// at the bottom) each keep some of `j`, `k` and Space for themselves (acceptance fix 1).
/// In Antiope, `AntiopeState` is offered each command before `AppState`.
@MainActor final class ReadingKeyRouter {
    weak var window: NSWindow?
    let state: AppState
    let focus: any KeyFocusProvider
    /// The Antiope window's state; nil in the native window.
    let antiope: AntiopeState?
    private var monitor: Any?

    init(state: AppState, focus: any KeyFocusProvider, antiope: AntiopeState?) {
        self.state = state
        self.focus = focus
        self.antiope = antiope
    }

    isolated deinit {
        stop()
    }

    /// Starts routing the application's key presses (only this window's are taken).
    func start() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let handled = MainActor.assumeIsolated { self?.handle(event) ?? false }
            return handled ? nil : event
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    /// True when the key was used, so that nothing else receives it. A command that cannot act
    /// here (the Garden, an empty list) lets the key go on to the focused view.
    func handle(_ event: NSEvent) -> Bool {
        guard let window, event.type == .keyDown, event.windowNumber == window.windowNumber,
              let key = ReadingKeys.key(for: event),
              let command = ReadingKeys.command(for: key, in: focus.keyFocus(in: window),
                                                appearance: antiope == nil ? .native : .antiope)
        else { return false }
        // A held digit must not type `444`, nor a held ⌥R queue refreshes: spent, nothing done.
        if event.isARepeat, command.actsOncePerPress { return true }
        if let antiope, antiope.handle(command) { return true }
        guard state.perform(command, isRepeat: event.isARepeat) else { return false }
        switch command {
        case .focusEntryList: focusTable(inColumn: MainColumns.list, of: window)
        case .focusSidebar: focusTable(inColumn: MainColumns.sidebar, of: window)
        default: break
        }
        return true
    }

    private func focusTable(inColumn index: Int, of window: NSWindow) {
        guard let columns = MainColumns.columns(in: window), columns.indices.contains(index),
              let table = MainColumns.firstTable(in: columns[index])
        else { return }
        window.makeFirstResponder(table)
    }
}

private extension ReadingCommand {
    /// Antiope keys that act once per press; `⌫` and the index arrows repeat when held.
    var actsOncePerPress: Bool {
        switch self {
        case .pageDigit, .pageCancel, .sommaire, .guide, .repetition, .toggleImages, .nextColumn, .previousColumn:
            true
        default:
            false
        }
    }
}
```

Then append at the end of the same file:

```swift
/// Tells `AppState` which window is a reading window, to gate single-letter shortcuts, and routes
/// that window's keys. Each look puts one behind its root view with its own router (`MainView`,
/// `AntiopeRootView`).
struct ReadingWindowReader: NSViewRepresentable {
    let makeRouter: @MainActor () -> ReadingKeyRouter

    func makeNSView(context: Context) -> WindowView { WindowView(router: makeRouter()) }

    func updateNSView(_ view: WindowView, context: Context) {}

    static func dismantleNSView(_ view: WindowView, coordinator: ()) {
        view.leave()
    }

    final class WindowView: NSView {
        let router: ReadingKeyRouter
        /// The window this view registered, so that it unregisters it exactly once.
        private weak var registered: NSWindow?

        init(router: ReadingKeyRouter) {
            self.router = router
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            if newWindow !== window { leave() }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            router.window = window
            guard let window else { return }
            if registered !== window {
                router.state.registerReadingWindow(window)
                registered = window
            }
            router.start()
        }

        /// Stops routing and gives the window back, once.
        func leave() {
            router.stop()
            if let registered { router.state.unregisterReadingWindow(registered) }
            registered = nil
        }
    }
}
```

- [ ] **Step 4: Use it from `MainView`.** In `apps/macos/Ressac/Reading/MainView.swift`, replace:

```swift
        .background(ReadingWindowReader(state: state))
```

with:

```swift
        .background(ReadingWindowReader { [state] in
            ReadingKeyRouter(state: state, focus: ResponderFocusProvider(), antiope: nil)
        })
```

and delete the whole private `ReadingWindowReader` struct at the end of the file, from its doc comment `/// Tells \`AppState\` which window is the reading window, to gate single-letter shortcuts, and` down to the file's last closing brace.

- [ ] **Step 5: Count the readers holding a window.** In `apps/macos/Ressac/App/AppState.swift`, replace:

```swift
    /// The windows showing `MainView`, each registered by its own view.
    @ObservationIgnored private let readingWindows = NSHashTable<NSWindow>.weakObjects()
```

with:

```swift
    /// The windows showing a reading root view (`MainView` or `AntiopeRootView`), each
    /// registered by its `ReadingWindowReader`.
    @ObservationIgnored private let readingWindows = NSHashTable<NSWindow>.weakObjects()
    /// How many readers hold each of them: on a live Appearance switch the new look's reader can
    /// arrive before the old one leaves.
    @ObservationIgnored private var readingWindowHolds: [ObjectIdentifier: Int] = [:]
```

and replace:

```swift
    /// Called by the main view (`ReadingWindowReader`) when it enters a window.
    func registerReadingWindow(_ window: NSWindow) {
        readingWindows.add(window)
        readingWindowIsKey = window.isKeyWindow
    }

    /// Called by the main view when it leaves `window`. Another window's registration stays,
    /// and with no reading window left the letters are off.
    func unregisterReadingWindow(_ window: NSWindow) {
        guard readingWindows.contains(window) else { return }
        readingWindows.remove(window)
        readingWindowIsKey = readingWindows.allObjects.contains { $0.isKeyWindow }
    }
```

with:

```swift
    /// Called by a root view's `ReadingWindowReader` when it enters a window.
    func registerReadingWindow(_ window: NSWindow) {
        readingWindows.add(window)
        readingWindowHolds[ObjectIdentifier(window), default: 0] += 1
        readingWindowIsKey = window.isKeyWindow
    }

    /// Called by a root view's reader when it leaves `window`. The window stays a reading
    /// window while another reader holds it; another window's registration stays too, and with
    /// no reading window left the letters are off.
    func unregisterReadingWindow(_ window: NSWindow) {
        let id = ObjectIdentifier(window)
        guard readingWindows.contains(window), let holds = readingWindowHolds[id] else { return }
        guard holds == 1 else {
            readingWindowHolds[id] = holds - 1
            return
        }
        readingWindowHolds[id] = nil
        readingWindows.remove(window)
        readingWindowIsKey = readingWindows.allObjects.contains { $0.isKeyWindow }
    }
```

- [ ] **Step 6: Run** the keyboard suites: `-only-testing:RessacTests/ReadingKeysTests -only-testing:RessacTests/ReadingKeyRouterTests -only-testing:RessacTests/AntiopeStateTests -only-testing:RessacTests/AntiopeKeyRouterTests -only-testing:RessacTests/ReadingWindowTests`. Expected: `✔ Test run with 34 tests in 5 suites passed`. Then the whole suite: `** TEST SUCCEEDED **`.

- [ ] **Step 7: Commit.**

```sh
git add apps/macos/Ressac/Reading/ReadingKeyRouter.swift apps/macos/Ressac/Reading/MainView.swift apps/macos/Ressac/App/AppState.swift apps/macos/RessacTests/KeyboardTests.swift
git commit -m "$(cat <<'EOF'
macOS app (Antiope): router offers keys to Antiope first; one reading-window reader for both looks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

### Verification gate (7b)

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

Expected: Rust checks unchanged, `** TEST SUCCEEDED **` (23 more Swift tests than after 7a: 8 key-mapping tests replace 3 old ones, one native router test, 11 state tests, 5 Antiope router tests, 1 window test), `no warnings in our sources`. Push.

**Manual checks carried to phase 14** (they need a person at the keyboard; never simulate keystrokes into the running app):
1. Native look: `j`, `k`, `m`, `s`, Space and the arrows behave exactly as before; typing digits in the sidebar still type-selects.
2. On the French keyboard, Shift-4-1-2 in Antiope jumps to 412 (phase 8 makes it visible).

### Notes for the executor

- **`charactersIgnoringModifiers`** keeps Shift but drops Option: ⌥S gives `s`, Shift-4 on AZERTY gives `4`, Shift-comma gives `?`. That is why the tests build ⌥ events with the plain letter.
- **`specialKey`** is `.delete` for `\u{7F}` on some systems and nil on others; `key(for:)` reads the character first, so both work.
- **`any Clock<Duration>`** is a constrained existential: `clock.sleep(for:)` opens it, and `Clock` refines `Sendable`, so the reset task can capture it.
- **`ManualClock.sleeperCount`**: the reset task registers its sleep asynchronously; advancing the clock before that would move the deadline with it. The tests wait for the sleeper first.
- **`@Observable` and computed properties**: `isSearchPromptOpen` and `currentPage` are computed, so views reading them track `AppState.isSearchFocused`, `AppState.selection` and `AppState.sidebar` through the macro's tracking of those stored properties.
- **`ReadingWindowReader`'s closure** runs once, in `makeNSView`; re-renders of the root view build no new router.
