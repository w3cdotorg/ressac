import AppKit
import SwiftUI
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
