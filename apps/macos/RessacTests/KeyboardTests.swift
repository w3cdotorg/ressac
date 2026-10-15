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
        let router = ReadingKeyRouter(state: state, focus: ResponderFocusProvider(), antiope: nil)
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

    @Test func suiteFromAReservedPageOnlyBringsTheArticleBack() async throws {
        let (antiope, state, test, _) = try make()
        let first = try test.entry(feed: 1, published: "2026-10-02T08:00:00.000Z")
        let second = try test.entry(feed: 1, published: "2026-10-01T08:00:00.000Z")
        try await state.database.notifyExternalChanges()
        #expect(await eventually { state.entries.count == 2 })
        state.autoReadDelay = .seconds(60)
        state.selectedEntryId = first
        #expect(await eventually { state.selectedEntry != nil })
        for page in [ReaderPage.guide, .index] {
            for command in [ReadingCommand.advance, .nextEntry, .previousEntry] {
                antiope.readerPage = page
                #expect(antiope.handle(command))  // Antiope's: AppState must not move on in the same press
                #expect(antiope.readerPage == .article)
                #expect(state.selectedEntryId == first)
                #expect(!antiope.handle(command))  // the article is back: now AppState's
            }
        }
        // The SUITE button of the Minitel bar takes the same path (`MinitelBar.press`).
        antiope.readerPage = .guide
        let key = MinitelKey.suite
        if !antiope.handle(key.command) { state.perform(key.command) }
        #expect(state.selectedEntryId == first && antiope.readerPage == .article)
        try await Task.sleep(for: .milliseconds(100))
        #expect(state.selectedEntryId == first)
        #expect(state.entries.first { $0.entryId == first }?.isRead == false)
        #expect(state.entries.first { $0.entryId == second }?.isRead == false)
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
