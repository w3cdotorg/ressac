import SwiftUI

/// Menu bar commands with every shortcut of spec §7. The items live in small views so
/// that SwiftUI tracks the `AppState` properties they read.
struct RessacCommands: Commands {
    let state: AppState?

    var body: some Commands {
        CommandGroup(replacing: .newItem) { FileMenuItems(state: state) }
        CommandGroup(replacing: .textEditing) { SearchMenuItem(state: state) }
        CommandGroup(after: .toolbar) { TextSizeMenuItems() }
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

/// Reader text size in points (12...28, default 17). The key is phase 10's `ReaderStyle.Keys.size`.
private struct TextSizeMenuItems: View {
    @AppStorage("reader.size") private var textSize = 17.0

    var body: some View {
        Button("Bigger Text") { textSize = min(28, textSize + 1) }
            .keyboardShortcut("+")
        Button("Smaller Text") { textSize = max(12, textSize - 1) }
            .keyboardShortcut("-")
        Button("Actual Size") { textSize = 17 }
            .keyboardShortcut("0")
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
