import SwiftUI

/// A key of the Minitel bar (spec §6): its French label, what VoiceOver reads, what it does.
enum MinitelKey: CaseIterable, Sendable {
    case sommaire, suite, retour, guide, repetition

    /// The device's own key names, the one exception to English in the interface.
    var label: String {
        switch self {
        case .sommaire: "SOMMAIRE"
        case .suite: "SUITE"
        case .retour: "RETOUR"
        case .guide: "GUIDE"
        case .repetition: "RÉPÉTITION"
        }
    }

    var accessibilityLabel: String {
        switch self {
        case .sommaire: "Sommaire, go to page 100"
        case .suite: "Suite, next page"
        case .retour: "Retour, previous page"
        case .guide: "Guide, key help on page 899"
        case .repetition: "Répétition, refresh"
        }
    }

    /// The keyboard key's command, so that a click and a key do one thing.
    var command: ReadingCommand {
        switch self {
        case .sommaire: .sommaire
        case .suite: .advance
        case .retour: .previousEntry
        case .guide: .guide
        case .repetition: .repetition
        }
    }

    /// ` SOMMAIRE `, black on cyan.
    var line: CellLine {
        CellLine([CellSpan(" \(label) ", cells: label.count + 2, foreground: .black, background: .cyan)],
                 accessibilityText: accessibilityLabel)
    }
}

/// The text right of the keys and its colour.
struct MinitelHint: Equatable, Sendable {
    var text: String
    var colour: TeletextColor
}

/// The bottom row: the Minitel keys, then hints that follow the focus (spec §6, Minitel bar).
struct MinitelBar: View {
    let width: Int

    /// Each key with its two padding cells and the black cell after it.
    static let keysCells = MinitelKey.allCases.reduce(0) { $0 + $1.label.count + 3 }

    @Environment(AppState.self) private var app
    @Environment(AntiopeState.self) private var antiope

    var body: some View {
        HStack(spacing: 0) {
            ForEach(MinitelKey.allCases, id: \.self) { key in
                Button { press(key) } label: { CellLineView(line: key.line) }
                    .buttonStyle(.plain)
                    .accessibilityLabel(key.accessibilityLabel)
                CellLineView(line: .blank(1))
            }
            CellLineView(line: Self.hintLine(
                Self.hint(column: antiope.column, isGarden: app.selection.isGarden, searchText: app.searchText),
                width: max(0, width - Self.keysCells)))
        }
    }

    private func press(_ key: MinitelKey) {
        if !antiope.handle(key.command) { app.perform(key.command) }
    }

    /// The search in force first, then the keys of the Garden or of the focused column.
    static func hint(column: AntiopeColumn, isGarden: Bool, searchText: String) -> MinitelHint {
        if !searchText.isEmpty { return MinitelHint(text: "SEARCH: \(searchText)", colour: .cyan) }
        if isGarden { return MinitelHint(text: "k keep · r repair · b bury · u undo", colour: .white) }
        switch column {
        case .index: return MinitelHint(text: "type a page number · ? guide", colour: .white)
        case .entries: return MinitelHint(text: "j next · k previous · m read", colour: .white)
        case .reader: return MinitelHint(text: "m read · s star · i images", colour: .white)
        }
    }

    /// The hint after one blank cell, exactly `width` cells.
    static func hintLine(_ hint: MinitelHint, width: Int) -> CellLine {
        CellLine([CellSpan(" " + hint.text, cells: width, foreground: hint.colour)], accessibilityText: hint.text)
    }
}
