import AppKit
import WebKit

/// What a reading key asks `AppState` to do (spec §7, Keyboard).
enum ReadingCommand: Equatable, Sendable {
    case nextEntry, previousEntry, advance, toggleRead, toggleStar
    /// `→` in the sidebar: the list takes the keyboard, on its first entry when none is selected.
    case focusEntryList
    /// `←` in the list: back to the sidebar.
    case focusSidebar
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

/// The single-key commands of the reading window and where each one applies.
enum ReadingKeys {
    enum Key: Equatable, Sendable {
        case letter(Character)
        case space, up, down, left, right
    }

    /// The key of a press without Shift, Control, Option or Command (Caps Lock is fine), or nil.
    static func key(for event: NSEvent) -> Key? {
        // Only the modifiers a person holds count: arrows carry `.numericPad` and `.function`,
        // and posted events can carry private bits.
        guard event.modifierFlags.isDisjoint(with: [.shift, .control, .option, .command]) else { return nil }
        switch event.specialKey {
        case .upArrow?: return .up
        case .downArrow?: return .down
        case .leftArrow?: return .left
        case .rightArrow?: return .right
        case nil: break
        default: return nil
        }
        guard let characters = event.charactersIgnoringModifiers, characters.count == 1,
              let character = characters.lowercased().first
        else { return nil }
        return character == " " ? .space : .letter(character)
    }

    /// NetNewsWire's rules: letters and Space act wherever the reading window is not typing;
    /// `↓`/`↑` are `j`/`k` in the list and the reader, the sidebar keeps its own; `→` leaves the
    /// sidebar for the list and `←` comes back.
    static func command(for key: Key, in focus: KeyFocus) -> ReadingCommand? {
        switch (key, focus) {
        case (_, .textInput): nil
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
}

/// Rules the reading window's keys before AppKit offers them to the focused view, the way
/// NetNewsWire's window controller does. Menu key equivalents without a modifier are not
/// enough: AppKit tries them only once the first responder has declined the key, and the
/// sidebar and the list (type-select, Space) and the reader (WebKit scrolls with Space, even
/// at the bottom) each keep some of `j`, `k` and Space for themselves (acceptance fix 1).
@MainActor final class ReadingKeyRouter {
    weak var window: NSWindow?
    let state: AppState
    private var monitor: Any?

    init(state: AppState) {
        self.state = state
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
              let command = ReadingKeys.command(for: key, in: focus(of: window.firstResponder)),
              state.perform(command)
        else { return false }
        switch command {
        case .focusEntryList: focusTable(inColumn: Column.list)
        case .focusSidebar: focusTable(inColumn: Column.sidebar)
        case .nextEntry, .previousEntry, .advance, .toggleRead, .toggleStar: break
        }
        return true
    }

    /// The columns of `MainView`'s `NavigationSplitView`, in order.
    private enum Column {
        static let sidebar = 0, list = 1, reader = 2
    }

    func focus(of responder: NSResponder?) -> KeyFocus {
        guard let responder, responder !== window else { return .window }
        if responder is NSText || responder is NSTextField { return .textInput }
        guard let view = responder as? NSView else { return .control }
        if ancestors(of: view).contains(where: { $0 is WKWebView }) { return .reader }
        switch column(of: view) {
        case Column.sidebar?: return .sidebar
        case Column.list?: return view is NSTableView ? .entryList : .control
        case Column.reader?: return .reader
        default: return .control
        }
    }

    private func ancestors(of view: NSView) -> some Sequence<NSView> {
        sequence(first: view, next: \.superview)
    }

    /// The three-column split view of the window.
    private var columns: [NSView]? {
        guard let root = window?.contentView else { return nil }
        var pending = [root]
        while !pending.isEmpty {
            let view = pending.removeFirst()
            if let split = view as? NSSplitView, split.arrangedSubviews.count == 3 { return split.arrangedSubviews }
            pending += view.subviews
        }
        return nil
    }

    private func column(of view: NSView) -> Int? {
        guard let columns else { return nil }
        return ancestors(of: view).lazy.compactMap { ancestor in columns.firstIndex { $0 === ancestor } }.first
    }

    private func focusTable(inColumn index: Int) {
        guard let columns, columns.indices.contains(index), let table = firstTable(in: columns[index]) else { return }
        window?.makeFirstResponder(table)
    }

    private func firstTable(in root: NSView) -> NSTableView? {
        var pending = [root]
        while !pending.isEmpty {
            let view = pending.removeFirst()
            if let table = view as? NSTableView { return table }
            pending += view.subviews
        }
        return nil
    }
}
