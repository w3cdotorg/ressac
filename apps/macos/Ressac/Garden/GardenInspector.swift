import GRDB
import SwiftUI

/// The Garden's detail column: why a feed is here, and what to do with it (spec §7).
struct GardenInspector: View {
    @Bindable var state: AppState
    let scope: GardenScope

    @State private var active: [GardenRow] = []
    @State private var buried: [GardenRow] = []
    @State private var log: [FetchLogRow] = []
    @State private var showsHistory = false
    /// `v_cadence` by feed: active feeds only, so a buried feed shows no rhythm.
    @State private var cadence: [Int64: CadenceRow] = [:]
    /// Every theme by position, and each themed feed's theme key (spec §9).
    @State private var themes: [ThemeRow] = []
    @State private var themeKeys: [Int64: String] = [:]

    var body: some View {
        Group {
            if let row = (active + buried).first(where: { $0.feedId == state.selectedFeedId }) {
                ScrollView {
                    details(row)
                        .padding(20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .task(id: row.feedId) { await observeLog(of: row.feedId) }
            } else {
                ContentUnavailableView("No Feed Selected", systemImage: "leaf",
                                       description: Text("Select a feed to see why it is in the garden."))
            }
        }
        // Active feeds and the Cemetery: a feed buried during the visit stays inspectable.
        .task(id: scope) { await observe(.all) { active = $0 } }
        .task(id: scope) { await observe(.cemetery) { buried = $0 } }
        .task { await observeCadence() }
        .task { await observeThemes() }
        .task { await observeThemeKeys() }
    }

    @ViewBuilder
    private func details(_ row: GardenRow) -> some View {
        let status = GardenModel.status(of: row)
        let buried = GardenModel.isBuried(row)
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.title3.weight(.semibold))
                    .textSelection(.enabled)
                if let path = row.folderPath, !path.isEmpty {
                    Text(path).font(.callout).foregroundStyle(.secondary)
                }
            }

            VStack(alignment: .leading, spacing: 4) {
                Text("\(GardenModel.glyph(status)) \(status)")
                    .font(.headline)
                    .foregroundStyle(GardenStatusStyle.nativeColor(for: status))
                if buried {
                    Text("Buried. Press k to restore it.").font(.callout.weight(.medium))
                }
                if let reason = row.reason {
                    Text(reason).fixedSize(horizontal: false, vertical: true)
                }
            }

            if let rhythm = cadence[row.feedId], !buried {
                rhythmControl(rhythm, feedId: row.feedId)
            }
            if !buried {
                themeControl(feedId: row.feedId)
            }

            if let bullets = row.evidence?.bullets, !bullets.isEmpty {
                section("Evidence", lines: bullets)
            }
            if !row.remarks.isEmpty {
                section("Remarks", lines: row.remarks.map(GardenModel.remarkSentence))
            }

            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 10, verticalSpacing: 4) {
                if let action = row.proposedAction, !buried {
                    labeled("Proposed", Text(action.label))
                }
                if let decision = row.lastDecision {
                    labeled("Decision", Text(decision.capitalized))
                }
                labeled("Feed", Text(row.xmlUrl).textSelection(.enabled))
                if let site = LinkPolicy.webURL(row.htmlUrl) {
                    labeled("Site", Link(site.absoluteString, destination: site))
                }
                if let wayback = LinkPolicy.webURL(row.waybackUrl) {
                    labeled("Wayback", Link("Archived copy", destination: wayback))
                }
            }
            .font(.callout)

            actions(row, buried: buried)

            DisclosureGroup("Redirects and fetch log", isExpanded: $showsHistory) {
                history
            }
            .font(.callout)
        }
    }

    private func actions(_ row: GardenRow, buried: Bool) -> some View {
        let action = row.proposedAction
        return HStack(spacing: 6) {
            KeyHintButton(buried ? "Restore" : "Keep", key: "k", prominent: false) { state.gardenDecide(.keep) }
            if let action, action.isRepair, !buried {
                KeyHintButton("Repair", key: "r", prominent: true) { state.gardenDecide(.repair) }
            }
            if !buried {
                KeyHintButton("Bury", key: "b", prominent: action == .bury) { state.gardenDecide(.bury) }
            }
            KeyHintButton("Open Site", key: "o", prominent: false) {
                if let site = LinkPolicy.webURL(row.htmlUrl) { state.openURL(site) }
            }
            .disabled(LinkPolicy.webURL(row.htmlUrl) == nil)
            KeyHintButton("Wayback", key: "w", prominent: false) {
                if let wayback = LinkPolicy.webURL(row.waybackUrl) { state.openURL(wayback) }
            }
            .disabled(LinkPolicy.webURL(row.waybackUrl) == nil)
        }
        .controlSize(.small)
    }

    /// "Rhythm: Daily · median 0.4 d · last entry 2 h ago" and the Auto / group menu.
    private func rhythmControl(_ rhythm: CadenceRow, feedId: Int64) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(GardenModel.rhythmLine(rhythm, now: state.now()))
                .fixedSize(horizontal: false, vertical: true)
            Picker("Rhythm", selection: Binding(
                get: { rhythm.isOverride ? Optional(rhythm.effective) : nil },
                set: { state.setCadence(feedId: feedId, $0) })) {
                Text("Auto").tag(CadenceGroup?.none)
                ForEach(CadenceGroup.allCases, id: \.self) { group in
                    Text(group.title).tag(Optional(group))
                }
            }
            .pickerStyle(.menu)
            .labelsHidden()
            .fixedSize()
            .controlSize(.small)
            .help("Auto follows how often the feed publishes; a group chosen here stays until you choose Auto.")
        }
        .font(.callout)
    }

    /// "Theme: [None | themes by position]": the sea the feed sails on in the Sea Chart.
    private func themeControl(feedId: Int64) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("Theme")
            Picker("Theme", selection: Binding(
                get: { themeKeys[feedId] },
                set: { state.setTheme(feedId: feedId, key: $0) })) {
                Text("None").tag(String?.none)
                ForEach(themes) { theme in
                    Text(theme.name).tag(Optional(theme.key))
                }
            }
            .pickerStyle(.menu)
            .labelsHidden()
            .fixedSize()
            .controlSize(.small)
            .help("The sea this feed sails on in the Sea Chart. None leaves it in the uncharted waters.")
        }
        .font(.callout)
    }

    @ViewBuilder
    private var history: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let chain = log.first?.redirectChain, !chain.isEmpty {
                Text("Redirects").font(.callout.weight(.semibold))
                ForEach(Array(chain.enumerated()), id: \.offset) { _, hop in
                    Text("\(hop.status) → \(hop.url)")
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            if log.isEmpty {
                Text("Not fetched yet.").foregroundStyle(.secondary)
            } else {
                Text("Fetch log").font(.callout.weight(.semibold))
                ForEach(log) { entry in
                    HStack(alignment: .firstTextBaseline) {
                        Text(entry.at.formatted(date: .abbreviated, time: .shortened))
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                        Text(GardenModel.outcome(of: entry))
                    }
                    .font(.caption)
                }
            }
        }
        .padding(.top, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func section(_ title: String, lines: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.callout.weight(.semibold))
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("•").foregroundStyle(.secondary)
                    Text(line).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                }
            }
        }
    }

    private func labeled(_ label: String, _ value: some View) -> some View {
        GridRow {
            Text(label).foregroundStyle(.secondary).gridColumnAlignment(.trailing)
            value
        }
    }

    private func observe(_ query: GardenScope, into store: ([GardenRow]) -> Void) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: state.database.reader) {
                store(fresh)
            }
        } catch {
            store([])
        }
    }

    private func observeCadence() async {
        do {
            for try await rows in Queries.cadenceRows().values(in: state.database.reader) {
                cadence = GardenModel.byFeed(rows)
            }
        } catch {
            cadence = [:]
        }
    }

    private func observeThemes() async {
        do {
            for try await rows in Queries.themes().values(in: state.database.reader) {
                themes = rows
            }
        } catch {
            themes = []
        }
    }

    private func observeThemeKeys() async {
        do {
            for try await keys in Queries.feedThemeKeys().values(in: state.database.reader) {
                themeKeys = keys
            }
        } catch {
            themeKeys = [:]
        }
    }

    private func observeLog(of feedId: Int64) async {
        log = []
        do {
            for try await fresh in Queries.fetchLog(feedId: feedId).values(in: state.database.reader) {
                log = fresh
            }
        } catch {
            log = []
        }
    }
}

/// A button with its single-key shortcut shown in a lighter weight, as in the mockup ("Keep k").
private struct KeyHintButton: View {
    let title: String
    let key: String
    let prominent: Bool
    let action: () -> Void

    init(_ title: String, key: String, prominent: Bool, action: @escaping () -> Void) {
        self.title = title
        self.key = key
        self.prominent = prominent
        self.action = action
    }

    var body: some View {
        if prominent {
            Button(action: action) { label }.buttonStyle(.borderedProminent)
        } else {
            Button(action: action) { label }.buttonStyle(.bordered)
        }
    }

    private var label: some View {
        HStack(spacing: 4) {
            Text(title)
            Text(key).font(.caption2).opacity(0.6)
        }
    }
}
