import SwiftUI

/// Menu bar commands with every shortcut of spec §7. The items live in small views so
/// that SwiftUI tracks the `AppState` properties they read.
struct RessacCommands: Commands {
    let state: AppState?

    var body: some Commands {
        CommandGroup(replacing: .newItem) { FileMenuItems(state: state) }
        CommandGroup(replacing: .textEditing) { SearchMenuItem(state: state) }
        CommandGroup(after: .toolbar) {
            UnreadOnlyMenuItem(state: state)
            TextSizeMenuItems()
            SeaChartMenuItem()
        }
        CommandMenu("Go") { GoMenuItems(state: state) }
        CommandMenu("Article") { ArticleMenuItems(state: state) }
    }
}

/// A modifier-free shortcut, or none when the command cannot act. AppKit swallows a menu
/// key equivalent even when its item is disabled, so a bound `j` would never reach the
/// search field, and of two items sharing a key only the first is ever tried. In the reading
/// window these keys are taken earlier by `ReadingKeyRouter`; the menus show them and act on a click.
private func letter(_ key: KeyEquivalent, when enabled: Bool?) -> KeyboardShortcut? {
    enabled == true ? KeyboardShortcut(key, modifiers: []) : nil
}

private struct FileMenuItems: View {
    let state: AppState?

    var body: some View {
        Button("Import OPML…") { state?.pendingPanel = .importOPML }
            .keyboardShortcut("o")
            .disabled(state == nil)
        Button("Export OPML…") { state?.pendingPanel = .exportOPML }
            .keyboardShortcut("e", modifiers: [.command, .shift])
            .disabled(state == nil)
        Divider()
        Button("Refresh") { state?.refreshCurrent() }
            .keyboardShortcut("r")
            .disabled(state?.canRefresh != true)
        Button("Refresh All Feeds") { state?.refreshAll() }
            .keyboardShortcut("r", modifiers: [.command, .shift])
            .disabled(state?.canRefresh != true)
    }
}

private struct SearchMenuItem: View {
    let state: AppState?

    var body: some View {
        Button("Search") { state?.isSearchFocused = true }
            .keyboardShortcut("f")
            .disabled(state == nil)
    }
}

/// The All / Unread filter, for both looks: the View menu item and Antiope's entries title.
/// A Garden page lists its own rows, so the filter does not apply there (as the Native picker).
@MainActor enum UnreadFilterCommand {
    static func canToggle(_ state: AppState) -> Bool { !state.selection.isGarden }

    static func toggle(_ state: AppState) {
        guard canToggle(state) else { return }
        state.unreadOnly.toggle()
    }
}

private struct UnreadOnlyMenuItem: View {
    let state: AppState?

    var body: some View {
        Toggle("Show Unread Only", isOn: Binding(
            get: { state?.unreadOnly ?? false },
            set: { _ in if let state { UnreadFilterCommand.toggle(state) } }))
            .keyboardShortcut("u", modifiers: [.command, .option])
            .disabled(state.map { !UnreadFilterCommand.canToggle($0) } ?? true)
    }
}

/// Reader text size (spec §5): 12...28 by 1 pt in Native, Bedstead's steps in Antiope. Each look
/// has its own stored size (`ReaderStyle.sizeKey(for:)`); the commands act on the look shown.
/// The standard defaults are the store `@AppStorage` reads everywhere outside the tests.
private struct TextSizeMenuItems: View {
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native

    private var appearance: Appearance {
        Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
    }

    var body: some View {
        Button("Bigger Text") { ReaderStyle.resize(.bigger, appearance: appearance, in: .standard) }
            .keyboardShortcut("+")
        Button("Smaller Text") { ReaderStyle.resize(.smaller, appearance: appearance, in: .standard) }
            .keyboardShortcut("-")
        Button("Actual Size") { ReaderStyle.resize(.actualSize, appearance: appearance, in: .standard) }
            .keyboardShortcut("0")
        Divider()
    }
}

/// View ▸ Sea Chart (spec §8): opens the chart's window, or brings it forward.
private struct SeaChartMenuItem: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("Sea Chart") { openWindow(id: ChartWindow.id) }
            .keyboardShortcut("m", modifiers: [.command, .shift])
        Divider()
    }
}

private struct GoMenuItems: View {
    let state: AppState?

    var body: some View {
        Button("Next Entry") { state?.selectNext() }
            .keyboardShortcut(letter("j", when: state?.canNavigateEntries))
            .disabled(state?.canNavigateEntries != true)
        Button("Previous Entry") { state?.selectPrevious() }
            .keyboardShortcut(letter("k", when: state?.canNavigateEntries))
            .disabled(state?.canNavigateEntries != true)
        Button("Scroll Down or Next Unread") { state?.advance() }
        .keyboardShortcut(letter(.space, when: state?.canNavigateEntries))
        .disabled(state?.canNavigateEntries != true)
    }
}

private struct ArticleMenuItems: View {
    let state: AppState?

    var body: some View {
        Button(state?.selectedEntry?.isRead == true ? "Mark as Unread" : "Mark as Read") { state?.toggleRead() }
            .keyboardShortcut(letter("m", when: state?.canToggleEntry))
            .disabled(state?.canToggleEntry != true)
        Button(state?.selectedEntry?.isStarred == true ? "Unstar" : "Star") { state?.toggleStar() }
            .keyboardShortcut(letter("s", when: state?.canToggleEntry))
            .disabled(state?.canToggleEntry != true)
        Button("Mark All as Read") { state?.markAllRead() }
            .keyboardShortcut("a", modifiers: [.command, .shift])
            .disabled(state?.canMarkAllRead != true)
        Divider()
        Button("Full Article") { state?.requestFullText() }
            .disabled(state?.canRequestFullText != true)
        Button("Open in Browser") { state?.openInBrowser() }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(state?.canOpenInBrowser != true)
    }
}
