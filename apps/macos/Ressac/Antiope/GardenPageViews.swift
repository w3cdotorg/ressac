import SwiftUI

/// The entry column of P800, P801 and P802 (spec §8): the feeds by status under coloured headings,
/// the mosaic progress row, and the terminal garden's keys.
struct GardenEntryPage: View {
    let scope: GardenScope

    @Environment(AppState.self) private var state
    @Environment(AntiopeState.self) private var antiope
    @Environment(GardenStore.self) private var store
    @Environment(\.teletextSize) private var size
    @FocusState private var focused: Bool

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        let lines = GardenPage.entryLines(store.rows, scope: scope, retained: store.retained(state),
                                          selected: state.selectedFeedId, cells: EntryPageColumn.cells)
        let safeRepairs = GardenModel.safeRepairs(store.rows)

        ScrollViewReader { proxy in
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        GardenLineView(line: line)
                            .contentShape(Rectangle())
                            .onTapGesture { select(line) }
                    }
                }
            }
            .scrollIndicators(.never)
            .onChange(of: state.selectedFeedId) { _, feedId in
                guard let feedId else { return }
                proxy.scrollTo("feed-\(feedId)")
            }
        }
        .frame(width: Double(EntryPageColumn.cells) * cell.width)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(TeletextColor.black.color)
        .focusable(interactions: .edit)
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(phases: .down) { press in
            handle(press, safeRepairCount: safeRepairs.count)
        }
        .background { undoShortcut }
        .confirmationDialog(GardenModel.applyQuestion(safeRepairs.count), isPresented: Binding(get: { store.confirmingSafeRepairs }, set: { store.setConfirmingSafeRepairs($0) }),
                            titleVisibility: .visible) {
            Button(safeRepairs.count == 1 ? "Apply" : "Apply All") { state.gardenApplySafe() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(GardenModel.safeRepairPreview(safeRepairs))
        }
        .task(id: scope) {
            focused = !antiope.isSearchPromptOpen
            await store.observe(page: scope, app: state)
        }
        .onChange(of: antiope.isSearchPromptOpen) { _, open in
            focused = !open
        }
        .onDisappear {
            store.clearMessage()
        }
    }

    /// ⌘Z undoes the last garden decision, like the native table's Undo button.
    private var undoShortcut: some View {
        Button("Undo Garden Decision") { store.perform(.undo, app: state) }
            .keyboardShortcut("z", modifiers: .command)
            .frame(width: 0, height: 0)
            .opacity(0)
            .accessibilityHidden(true)
    }

    /// The terminal garden's keys through `GardenModel.command`, as `GardenTable.handle`; `↑`/`↓`
    /// walk the list unless the index column is active.
    private func handle(_ press: KeyPress, safeRepairCount: Int) -> KeyPress.Result {
        if press.key == .downArrow || press.key == .upArrow {
            guard GardenPage.takesArrows(column: antiope.column) else { return .ignored }
            let ordered = GardenPage.orderedRows(store.rows, scope: scope, retained: store.retained(state))
            state.selectedFeedId = GardenPage.move(state.selectedFeedId, in: ordered, by: press.key == .downArrow ? 1 : -1)
            return .handled
        }
        guard let command = GardenModel.command(for: press.characters, modifiers: press.modifiers,
                                                row: store.selectedRow(state), safeRepairCount: safeRepairCount)
        else { return .ignored }
        store.perform(command, app: state)
        return .handled
    }

    private func select(_ line: GardenPage.Line) {
        guard case let .feed(feedId) = line.role else { return }
        state.selectedFeedId = feedId
        antiope.column = .entries
        focused = true
    }
}

/// The reader column of the garden pages: the selected feed's inspector as a teletext page.
struct GardenInspectorPage: View {
    @Environment(AppState.self) private var state
    @Environment(GardenStore.self) private var store
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let lines = GardenPage.inspectorLines(store.selectedRow(state), log: store.log,
                                                  cells: Int(proxy.size.width / cell.width))
            ScrollView(.vertical) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(lines) { line in
                        GardenLineView(line: line)
                    }
                }
            }
            .scrollIndicators(.never)
        }
        .background(TeletextColor.black.color)
        .task(id: state.selectedFeedId) {
            await store.observeLog(feedId: state.selectedFeedId, app: state)
        }
    }
}

/// `RunCoordinator`'s banner as a red reverse-video row under the page header (spec §8).
struct AntiopeBannerRow: View {
    let banner: Banner
    let dismiss: () -> Void

    @Environment(AppState.self) private var state
    @Environment(\.teletextSize) private var size

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            let row = GardenPage.bannerRow(banner, cells: Int(proxy.size.width / cell.width))
            HStack(spacing: 0) {
                CellLineView(line: row.message)
                if let log = row.log {
                    Button {
                        state.openURL(DatabaseLocation.logsURL)
                    } label: {
                        CellLineView(line: log)
                    }
                    .buttonStyle(.plain)
                }
                Button(action: dismiss) {
                    CellLineView(line: row.dismiss)
                }
                .buttonStyle(.plain)
            }
        }
        .frame(height: cell.height)
        .background(TeletextColor.red.color)
        .help(banner.message)
    }
}

/// One row of a garden page: phase 8's `CellLineView`, or phase 9's `DoubleHeight` for the
/// inspector's status. A row with nothing to read is hidden from VoiceOver.
struct GardenLineView: View {
    let line: GardenPage.Line

    var body: some View {
        Group {
            if line.isDoubleHeight {
                DoubleHeight(line: line.cells)
            } else {
                CellLineView(line: line.cells)
            }
        }
        .accessibilityHidden(line.cells.accessibilityText.isEmpty)
        .accessibilityAddTraits(line.isSelected ? .isSelected : [])
    }
}
