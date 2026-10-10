import GRDB
import SwiftUI

/// The Garden's list column (spec §7, mockup option A): one section per status, a header
/// with progress, safe repairs and undo, and the terminal garden's keys.
struct GardenTable: View {
    @Bindable var state: AppState
    let scope: GardenScope

    /// `Queries.garden(scope: .all)` (active feeds) and `.cemetery` (buried ones).
    @State private var active: [GardenRow] = []
    @State private var buried: [GardenRow] = []
    /// Feeds seen needing triage during this visit: once decided (here, by ⇧R or in the
    /// terminal) they stay, dimmed, until the list is left.
    @State private var seenInQueue: Set<Int64> = []
    @State private var message: String?
    @State private var confirmingSafeRepairs = false
    @FocusState private var listFocused: Bool

    private var rows: [GardenRow] { active + buried }
    private var retained: Set<Int64> { seenInQueue.union(state.decidedThisVisit) }

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
                    handle(press.characters, safeRepairCount: safeRepairs.count)
                }
            }
        }
        .confirmationDialog(GardenModel.applyQuestion(safeRepairs.count), isPresented: $confirmingSafeRepairs, titleVisibility: .visible) {
            Button(safeRepairs.count == 1 ? "Apply" : "Apply All") { state.gardenApplySafe() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(GardenModel.safeRepairPreview(safeRepairs))
        }
        .task(id: scope) {
            seenInQueue = []
            message = nil
            active = []
            listFocused = true
            guard scope != .cemetery else { return }
            await observe(.all) { active = $0 }
        }
        .task(id: scope) {
            buried = []
            guard scope != .all else { return }
            await observe(.cemetery) { buried = $0 }
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
            if let message {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .transition(.opacity)
            }
            if scope != .cemetery {
                Button(GardenModel.applyButtonTitle(safeRepairCount)) { confirmingSafeRepairs = true }
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
        .animation(.easeOut(duration: 0.15), value: message)
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

    private func handle(_ key: String, safeRepairCount: Int) -> KeyPress.Result {
        let row = rows.first { $0.feedId == state.selectedFeedId }
        guard let command = GardenModel.command(for: key, row: row, safeRepairCount: safeRepairCount) else {
            return .ignored
        }
        message = nil
        switch command {
        case .decide(let decision): state.gardenDecide(decision)
        case .undo: state.gardenUndo()
        case .open(let url): NSWorkspace.shared.open(url)
        case .confirmSafeRepairs: confirmingSafeRepairs = true
        case .message(let text): message = text
        }
        return .handled
    }

    private func observe(_ query: GardenScope, into store: ([GardenRow]) -> Void) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: state.database.reader) {
                let previous = rows
                store(fresh)
                rowsDidChange(previous: previous)
            }
        } catch {
            message = "The garden could not be read."
        }
    }

    /// Keeps the queue's memory and moves the selection on when the selected feed was decided.
    private func rowsDidChange(previous: [GardenRow]) {
        seenInQueue.formUnion(rows.filter(\.needsTriage).map(\.feedId))
        let ordered = GardenModel.sections(GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained), scope: scope)
            .flatMap(\.rows)
        if scope == .needsTriage {
            state.selectedFeedId = GardenModel.selectionAfterUpdate(
                selected: state.selectedFeedId, previous: previous, current: rows, ordered: ordered)
        }
        if !ordered.contains(where: { $0.feedId == state.selectedFeedId }) {
            state.selectedFeedId = ordered.first(where: \.needsTriage)?.feedId ?? ordered.first?.feedId
        }
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
