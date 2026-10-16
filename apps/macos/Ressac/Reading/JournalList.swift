import SwiftUI

/// One row of the Journal list: an entry, or the "+12 more in …" line of a capped feed.
enum JournalRow: Identifiable, Equatable {
    case entry(EntryRow)
    case overflow(EditionOverflowItem)

    var id: String {
        switch self {
        case let .entry(entry): "entry-\(entry.entryId)"
        case let .overflow(item): "overflow-\(item.section.rawValue)-\(item.feedId)"
        }
    }
}

/// One section of the Journal (Front Page, This Week, The Wire) and its rows.
struct JournalSection: Identifiable, Equatable {
    let section: EditionSection
    let rows: [JournalRow]

    var id: EditionSection { section }
}

/// The Journal screen's text and layout, pure (spec §8, Journal screen). `AppState.entries` gives
/// the rows (rank order, live read state, search); the edition gives their sections and the
/// overflow.
@MainActor enum JournalLayout {
    static let building = RunCoordinator.editionProgressLabel
    static let endLine = "That's the edition."
    static let emptyLine = "Nothing new this morning."

    /// The edition's sections in order, each with its listed entries and, right after the last
    /// entry of a capped feed, that feed's overflow row (at the section's end when none of its
    /// entries is listed). A section with no row is omitted.
    static func sections(entries: [EntryRow], edition: Edition?) -> [JournalSection] {
        guard let edition else { return [] }
        let sectionOf = Dictionary(edition.items.map { ($0.entryId, $0.section) }, uniquingKeysWith: { first, _ in first })
        return EditionSection.allCases.compactMap { section in
            var rows = entries.filter { sectionOf[$0.entryId] == section }.map(JournalRow.entry)
            for item in edition.overflow where item.section == section {
                let last = rows.lastIndex { row in
                    if case let .entry(entry) = row { entry.feedId == item.feedId } else { false }
                }
                rows.insert(.overflow(item), at: last.map { $0 + 1 } ?? rows.endIndex)
            }
            return rows.isEmpty ? nil : JournalSection(section: section, rows: rows)
        }
    }

    /// "Friday 16 October · 43 entries · 12 read", plus " · 3 feeds unreachable".
    static func header(_ edition: Edition, calendar: Calendar = .autoupdatingCurrent) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "EEEE d MMMM"
        let count = edition.items.count
        var parts = [formatter.string(from: edition.info.builtAt), count == 1 ? "1 entry" : "\(count) entries",
                     "\(edition.readCount) read"]
        let unreachable = edition.info.unreachable
        if unreachable > 0 {
            parts.append(unreachable == 1 ? "1 feed unreachable" : "\(unreachable) feeds unreachable")
        }
        return parts.joined(separator: " · ")
    }

    /// What the Journal shows instead of the list, or nil to show the edition.
    static func placeholder(phase: JournalModel.Phase, edition: Edition?) -> String? {
        switch phase {
        case let .building(done, total):
            return total > 0 ? "\(building) \(done)/\(total)" : building
        case .offline:
            return "Offline: no edition yet"
        case let .failed(message):
            return message
        case .idle, .due:
            guard let edition else { return "No edition yet" }
            return edition.items.isEmpty ? emptyLine : nil
        }
    }

    /// The toolbar progress text: an edition build also counts its feeds ("… 34/156").
    static func progressText(_ progress: RunProgress) -> String {
        guard progress.label == RunCoordinator.editionProgressLabel, progress.total > 0 else { return progress.label }
        return "\(progress.label) \(progress.done)/\(progress.total)"
    }

    /// "+12 more in Actualitté".
    static func overflowLabel(_ item: EditionOverflowItem) -> String {
        "+\(item.hiddenCount) more in \(item.feedTitle)"
    }
}

/// The list column while the Journal is selected (spec §8).
struct JournalListView: View {
    @Bindable var state: AppState

    var body: some View {
        let journal = state.journal
        if let message = JournalLayout.placeholder(phase: journal.phase, edition: journal.edition) {
            VStack(spacing: 10) {
                if case .building = journal.phase {
                    ProgressView().controlSize(.small)
                }
                Text(message)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .monospacedDigit()
                if Self.offersRetry(journal.phase) {
                    Button("Retry") { Task { await journal.retry() } }
                }
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let edition = journal.edition {
            VStack(spacing: 0) {
                Text(JournalLayout.header(edition))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                Divider()
                list(edition)
            }
        }
    }

    /// Whether the placeholder offers Retry: the build failed, or was turned away (`due`).
    static func offersRetry(_ phase: JournalModel.Phase) -> Bool {
        switch phase {
        case .offline, .failed, .due: true
        case .idle, .building: false
        }
    }

    private func list(_ edition: Edition) -> some View {
        ScrollViewReader { proxy in
            List(selection: $state.selectedEntryId) {
                ForEach(JournalLayout.sections(entries: state.entries, edition: edition)) { section in
                    Section(section.section.title) {
                        ForEach(section.rows) { row in
                            switch row {
                            case let .entry(entry):
                                EntryRowView(entry: entry).tag(entry.entryId)
                            case let .overflow(item):
                                // Not selectable: a click opens the feed in the sidebar.
                                Button(JournalLayout.overflowLabel(item)) { state.selection = .feed(item.feedId) }
                                    .buttonStyle(.plain)
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if edition.isFinished {
                    Text(JournalLayout.endLine)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                }
            }
            // As in `EntryListView`: the keyboard moves the selection, the list follows on the next turn.
            .onChange(of: state.selectedEntryId) { _, id in
                guard let id else { return }
                Task { @MainActor in proxy.scrollTo("entry-\(id)") }
            }
        }
    }
}
