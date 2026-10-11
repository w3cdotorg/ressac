import AppKit
import Testing
import WebKit

@testable import Ressac

/// The reading window's keys are ruled by `ReadingKeyRouter` before AppKit offers them to the
/// focused view (acceptance fix 1 and 3): the sidebar's and the list's type-select and the
/// reader's own Space scrolling used to take `j`, `k` and Space before the Go menu saw them.
@MainActor @Suite struct ReadingKeysTests {
    @Test func lettersAndSpaceActEverywhereButInTextFields() {
        for focus in [KeyFocus.sidebar, .entryList, .reader, .window] {
            #expect(ReadingKeys.command(for: .letter("j"), in: focus) == .nextEntry)
            #expect(ReadingKeys.command(for: .letter("k"), in: focus) == .previousEntry)
            #expect(ReadingKeys.command(for: .letter("m"), in: focus) == .toggleRead)
            #expect(ReadingKeys.command(for: .letter("s"), in: focus) == .toggleStar)
            #expect(ReadingKeys.command(for: .space, in: focus) == .advance)
        }
        for key in [ReadingKeys.Key.letter("j"), .letter("k"), .space, .up, .down, .left, .right] {
            #expect(ReadingKeys.command(for: key, in: .textInput) == nil)
        }
        #expect(ReadingKeys.command(for: .letter("x"), in: .entryList) == nil)
        #expect(ReadingKeys.command(for: .space, in: .control) == nil)  // a focused button keeps Space
    }

    @Test func arrowsFollowTheFocus() {
        for focus in [KeyFocus.entryList, .reader] {
            #expect(ReadingKeys.command(for: .down, in: focus) == .nextEntry)
            #expect(ReadingKeys.command(for: .up, in: focus) == .previousEntry)
        }
        #expect(ReadingKeys.command(for: .down, in: .sidebar) == nil)  // the sidebar's own arrows
        #expect(ReadingKeys.command(for: .up, in: .sidebar) == nil)
        #expect(ReadingKeys.command(for: .right, in: .sidebar) == .focusEntryList)
        #expect(ReadingKeys.command(for: .left, in: .entryList) == .focusSidebar)
        #expect(ReadingKeys.command(for: .left, in: .sidebar) == nil)
        #expect(ReadingKeys.command(for: .right, in: .entryList) == nil)
        #expect(ReadingKeys.command(for: .left, in: .reader) == nil)
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
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: .option)) == nil)
        #expect(ReadingKeys.key(for: keyEvent("j", modifiers: .control)) == nil)
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
        let (router, h, _, _, _) = try await makeRouter()
        #expect(router.focus(of: h.sidebar) == .sidebar)
        #expect(router.focus(of: h.list) == .entryList)
        #expect(router.focus(of: h.reader) == .reader)
        #expect(router.focus(of: h.window) == .window)
        #expect(router.focus(of: nil) == .window)
        #expect(h.window.makeFirstResponder(h.search))
        #expect(router.focus(of: h.window.firstResponder) == .textInput)  // the field editor
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
