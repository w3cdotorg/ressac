import AppKit
import Observation
import SwiftUI

/// The note under the window after an add: where the feed landed.
enum AddFeedNote {
    static func text(for feed: AddedFeed) -> String {
        switch (feed.theme, feed.like) {
        case let (theme?, like?): "In \(theme), like \(like)."
        case let (theme?, nil): "In \(theme)."
        case (nil, _): "Added to Uncharted waters."
        }
    }
}

/// One line of the sheet's Folder pop-up: the top level, or a folder under its parent.
struct FolderChoice: Identifiable, Equatable {
    /// Nil for the top level ("Feeds").
    let folderId: Int64?
    let name: String
    let depth: Int

    var id: Int64 { folderId ?? 0 }

    /// The menu text: one em space per level under the top.
    var title: String { String(repeating: "\u{2003}", count: depth) + name }

    /// "Feeds" first, then every folder in sidebar order, each under its parent.
    static func choices(from tree: [SidebarNode]) -> [FolderChoice] {
        func walk(_ nodes: [SidebarNode], depth: Int) -> [FolderChoice] {
            nodes.flatMap { node -> [FolderChoice] in
                guard case let .folder(folder) = node.kind else { return [] }
                return [FolderChoice(folderId: folder.folderId, name: folder.name, depth: depth)]
                    + walk(node.children ?? [], depth: depth + 1)
            }
        }
        return [FolderChoice(folderId: nil, name: "Feeds", depth: 0)] + walk(tree, depth: 1)
    }
}

/// The Add Feed sheet's state: the address, the optional theme, and what the last attempt said.
@MainActor @Observable final class AddFeedModel {
    var address = ""
    /// A theme key; nil lets the core inherit the theme of the site's other feeds.
    var themeKey: String?
    /// A folder id; nil is the top level.
    var folderId: Int64?
    var isRunning = false
    private(set) var errorMessage: String?
    /// The error is not a plain refusal: the log may say more.
    private(set) var showsLog = false

    init(address: String = "") {
        self.address = address
    }

    var canAdd: Bool {
        !isRunning && !address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The clipboard text when it is, on its own, a web address (http or https); else nil.
    static func clipboardAddress(_ text: String?) -> String? {
        guard let text else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.contains(where: \.isWhitespace), let url = URL(string: trimmed),
            let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
            url.host?.isEmpty == false
        else { return nil }
        return trimmed
    }

    /// Runs the add. True, with the sheet closed, on success; false, with the sentence in
    /// `errorMessage` and the sheet open, otherwise.
    func add(to state: AppState) async -> Bool {
        guard canAdd else { return false }
        isRunning = true
        errorMessage = nil
        showsLog = false
        defer { isRunning = false }
        let result = await state.addFeed(
            address: address.trimmingCharacters(in: .whitespacesAndNewlines), theme: themeKey,
            folder: folderId)
        switch result {
        case .success:
            state.pendingSheet = nil
            return true
        case let .failure(failure):
            errorMessage = failure.message
            showsLog = failure.exitCode != 2
            return false
        }
    }
}

struct AddFeedSheet: View {
    let state: AppState
    @State private var model = AddFeedModel(address: AddFeedModel.clipboardAddress(NSPasteboard.general.string(forType: .string)) ?? "")
    @State private var themes: [ThemeRow] = []
    @FocusState private var addressFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add a Feed").font(.headline)
            VStack(alignment: .leading, spacing: 6) {
                TextField("Feed or site address", text: $model.address)
                    .textFieldStyle(.roundedBorder)
                    .focused($addressFocused)
                    .disabled(model.isRunning)
                    .onSubmit(add)
                if let message = model.errorMessage {
                    HStack(alignment: .firstTextBaseline) {
                        Text(message).foregroundStyle(.red).textSelection(.enabled)
                        if model.showsLog {
                            Spacer()
                            Button("Show Log") { state.openURL(DatabaseLocation.logsURL) }.controlSize(.small)
                        }
                    }
                    .font(.callout)
                }
            }
            Picker("Theme", selection: $model.themeKey) {
                Text("None").tag(String?.none)
                ForEach(themes) { theme in
                    Text(theme.name).tag(Optional(theme.key))
                }
            }
            .pickerStyle(.menu)
            .disabled(model.isRunning)
            Picker("Folder", selection: $model.folderId) {
                ForEach(FolderChoice.choices(from: state.sidebarTree)) { choice in
                    Text(choice.title).tag(choice.folderId)
                }
            }
            .pickerStyle(.menu)
            .disabled(model.isRunning)
            HStack {
                if model.isRunning, let progress = state.runs.progress {
                    ProgressView(value: progress.fraction).progressViewStyle(.linear).frame(width: 120)
                } else if model.isRunning {
                    ProgressView().controlSize(.small)
                }
                Spacer()
                Button("Cancel") { state.pendingSheet = nil }
                    .keyboardShortcut(.cancelAction)
                Button("Add", action: add)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!model.canAdd)
            }
        }
        .padding(20)
        .frame(width: 420)
        .onAppear { addressFocused = true }
        .task {
            do {
                for try await rows in Queries.themes().values(in: state.database.reader) { themes = rows }
            } catch {
                themes = []
            }
        }
    }

    private func add() {
        Task { _ = await model.add(to: state) }
    }
}

/// "In Tech, like Daring Fireball.": a quiet line under the window that goes away by itself.
struct AddFeedNoteView: View {
    let note: String
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            Text(note).lineLimit(2).textSelection(.enabled)
            Spacer()
            Button("Dismiss", systemImage: "xmark", action: dismiss).labelStyle(.iconOnly).buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .task(id: note) {
            try? await Task.sleep(for: .seconds(6))
            if !Task.isCancelled { dismiss() }
        }
    }
}
