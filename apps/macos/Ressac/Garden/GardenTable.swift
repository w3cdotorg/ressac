import SwiftUI

/// The Garden's list column (spec §7, mockup option A): one section per status, a header
/// with progress, safe repairs and undo, and the terminal garden's keys.
struct GardenTable: View {
    @Bindable var state: AppState
    let scope: GardenScope

    @State private var store = GardenStore()
    @FocusState private var listFocused: Bool

    private var rows: [GardenRow] { store.rows }
    private var retained: Set<Int64> { store.retained(state) }

    var body: some View {
        let visible = GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained)
        let sections = GardenModel.sections(visible, scope: scope)
        let safeRepairs = GardenModel.safeRepairs(rows)

        VStack(spacing: 0) {
            header(visibleCount: visible.count, safeRepairCount: safeRepairs.count)
            Divider()
            if sections.isEmpty {
                ContentUnavailableView(GardenModel.emptyMessage(for: scope), systemImage: "leaf")
            } else {
                List(selection: $state.selectedFeedId) {
                    ForEach(sections) { section in
                        Section(section.header) {
                            ForEach(section.rows) { row in
                                GardenRowView(row: row, dimmed: GardenModel.isDimmed(row, scope: scope, seenInQueue: retained))
                                    .tag(row.feedId)
                            }
                        }
                    }
                }
                .focused($listFocused)
                .onKeyPress(phases: .down) { press in
                    handle(press.characters, modifiers: press.modifiers, safeRepairCount: safeRepairs.count)
                }
            }
        }
        .confirmationDialog(GardenModel.applyQuestion(safeRepairs.count), isPresented: Binding(get: { store.confirmingSafeRepairs }, set: { store.setConfirmingSafeRepairs($0) }), titleVisibility: .visible) {
            Button(safeRepairs.count == 1 ? "Apply" : "Apply All") { state.gardenApplySafe() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(GardenModel.safeRepairPreview(safeRepairs))
        }
        .task(id: scope) {
            listFocused = true
            await store.observe(page: scope, app: state)
        }
    }

    private func header(visibleCount: Int, safeRepairCount: Int) -> some View {
        HStack(spacing: 8) {
            Text(GardenModel.title(for: scope))
                .font(.headline)
            Text(countLabel(visibleCount: visibleCount))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(.quaternary, in: Capsule())
            Spacer(minLength: 8)
            if let message = store.message {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .transition(.opacity)
            }
            if scope != .cemetery {
                Button(GardenModel.applyButtonTitle(safeRepairCount)) { store.confirmSafeRepairs() }
                    .buttonStyle(.borderedProminent)
                    .disabled(safeRepairCount == 0)
                    .help("Apply every safe repair: https upgrades and permanent redirects (⇧R)")
            }
            Button("Undo") { state.gardenUndo() }
                .keyboardShortcut("z", modifiers: .command)
                .help("Undo the last decision, here or in the terminal garden (U or ⌘Z)")
        }
        .controlSize(.small)
        .padding(.horizontal, 12)
        .frame(height: 44)
        .animation(.easeOut(duration: 0.15), value: store.message)
    }

    private func countLabel(visibleCount: Int) -> String {
        switch scope {
        case .needsTriage:
            let progress = GardenModel.progress(rows)
            return "\(progress.left) of \(progress.total) left"
        case .all, .cemetery:
            return visibleCount == 1 ? "1 feed" : "\(visibleCount) feeds"
        }
    }

    private func handle(_ key: String, modifiers: EventModifiers, safeRepairCount: Int) -> KeyPress.Result {
        let row = rows.first { $0.feedId == state.selectedFeedId }
        guard let command = GardenModel.command(for: key, modifiers: modifiers, row: row, safeRepairCount: safeRepairCount) else {
            return .ignored
        }
        store.perform(command, app: state)
        return .handled
    }
}

private struct GardenRowView: View {
    let row: GardenRow
    let dimmed: Bool

    var body: some View {
        HStack(spacing: 8) {
            Text(GardenModel.glyph(GardenModel.status(of: row)))
                .frame(width: 22)
            Text(row.title)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            if let clue = row.clue, !clue.isEmpty {
                Text(clue)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let unread = GardenModel.unreadLabel(for: row) {
                Text(unread)
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let chip = GardenModel.chip(for: row) {
                Text(chip)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(.quaternary, in: Capsule())
            }
        }
        .opacity(dimmed ? 0.45 : 1)
        .animation(.easeOut(duration: 0.2), value: dimmed)
        .help([row.folderPath, row.reason].compactMap { $0 }.joined(separator: "\n"))
    }
}
