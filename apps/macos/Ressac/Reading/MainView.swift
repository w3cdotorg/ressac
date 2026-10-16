import SwiftUI

/// The three-column window (spec §7): sidebar 220 pt, list 320 pt, reader.
struct MainView: View {
    @Bindable var state: AppState
    @FocusState private var searchFocused: Bool

    var body: some View {
        NavigationSplitView {
            SidebarView(state: state)
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } content: {
            Group {
                if case let .garden(scope) = state.selection {
                    GardenTable(state: state, scope: scope)
                } else if state.selection == .journal {
                    JournalListView(state: state)
                } else {
                    EntryListView(state: state)
                }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 480)
        } detail: {
            if case let .garden(scope) = state.selection {
                GardenInspector(state: state, scope: scope)
            } else {
                ReaderView(state: state)
            }
        }
        .background(ReadingWindowReader { [state] in
            ReadingKeyRouter(state: state, focus: ResponderFocusProvider(), antiope: nil)
        })
        .searchable(text: $state.searchText, placement: .toolbar, prompt: "Search")
        .searchFocused($searchFocused)
        .onChange(of: searchFocused) { _, focused in state.isSearchFocused = focused }
        .onChange(of: state.isSearchFocused) { _, focused in searchFocused = focused }
        .toolbar {
            ToolbarItem {
                Picker("Show", selection: $state.unreadOnly) {
                    Text("All").tag(false)
                    Text("Unread").tag(true)
                }
                .pickerStyle(.segmented)
                .disabled(state.selection.isGarden)
            }
            ToolbarItem {
                if let progress = state.runs.progress {
                    let text = JournalLayout.progressText(progress)
                    // The full text when it fits the pill, else the short one (acceptance, item 2).
                    ProgressView(value: progress.fraction) {
                        ViewThatFits(in: .horizontal) {
                            Text(text)
                            Text(JournalLayout.compactProgressText(progress))
                        }
                        .lineLimit(1)
                        .monospacedDigit()
                    }
                    .progressViewStyle(.linear)
                    .frame(width: JournalLayout.progressContentWidth(progress))
                    .padding(.horizontal, JournalLayout.progressInset)
                    .help(text)
                } else {
                    Button("Refresh", systemImage: "arrow.clockwise") { state.refreshCurrent() }
                        .disabled(!state.canRefresh)
                }
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if let banner = state.runs.banner {
                BannerView(banner: banner) { state.runs.banner = nil }
            }
        }
    }
}

struct BannerView: View {
    let banner: Banner
    let dismiss: () -> Void

    @Environment(AppState.self) private var state

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(banner.message).lineLimit(2).textSelection(.enabled)
            Spacer()
            if banner.showLog {
                Button("Show Log") { state.openURL(DatabaseLocation.logsURL) }
            }
            Button("Dismiss", systemImage: "xmark", action: dismiss).labelStyle(.iconOnly).buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

struct SidebarView: View {
    @Bindable var state: AppState

    var body: some View {
        List(selection: $state.selection) {
            Section("Smart Lists") {
                Label("Journal", systemImage: "newspaper").badge(state.sidebar.journalUnread).tag(Source.journal)
                Label("Unread", systemImage: "circle.inset.filled").badge(state.sidebar.unread).tag(Source.unread)
                Label("Today", systemImage: "sun.max").badge(state.sidebar.today).tag(Source.today)
                Label("Starred", systemImage: "star").badge(state.sidebar.starred).tag(Source.starred)
            }
            Section("Feeds") {
                OutlineGroup(state.sidebarTree, children: \.children) { node in
                    SidebarRow(node: node, state: state).tag(node.source)
                }
            }
            Section("Rhythm") {
                // Silent stays listed at zero: it is where feeds that stopped publishing end up.
                ForEach(CadenceGroup.allCases, id: \.self) { group in
                    Label(group.title, systemImage: group.symbol)
                        .badge(state.sidebar.cadenceUnread[group] ?? 0)
                        .tag(Source.cadence(group))
                }
            }
            Section("Garden") {
                Label("Needs Triage", systemImage: "leaf").badge(state.sidebar.needsTriage)
                    .tag(Source.garden(.needsTriage))
                Label("All Feeds", systemImage: "list.bullet").tag(Source.garden(.all))
                Label("Cemetery", systemImage: "archivebox").tag(Source.garden(.cemetery))
            }
        }
        .listStyle(.sidebar)
    }
}

struct SidebarRow: View {
    let node: SidebarNode
    let state: AppState

    var body: some View {
        switch node.kind {
        case .folder:
            Label(node.title, systemImage: "folder").badge(node.unread)
        case let .feed(feed):
            HStack(spacing: 6) {
                Text(feed.title).lineLimit(1)
                if feed.isSick, let status = feed.status {
                    Circle().fill(GardenStatusStyle.nativeColor(for: status)).frame(width: 7, height: 7).help(status)
                }
            }
            .badge(feed.unread)
            .contextMenu {
                Button("Refresh") { state.refresh(feedId: feed.feedId) }.disabled(!state.canRefresh)
                Toggle("Always Fetch Full Article", isOn: Binding(
                    get: { feed.preferFullText },
                    set: { state.setPreferFullText(feedId: feed.feedId, on: $0) }))
                Button("Mark All as Read") { state.markAllRead(in: .feed(feed.feedId)) }.disabled(feed.unread == 0)
                Button("Open Site") { state.openSite(feedId: feed.feedId) }.disabled(feed.htmlUrl == nil)
                Button("Show in Garden") { state.showInGarden(feedId: feed.feedId) }
            }
        }
    }
}

struct EntryListView: View {
    @Bindable var state: AppState

    var body: some View {
        ScrollViewReader { proxy in
            List(state.entries, selection: $state.selectedEntryId) { entry in
                EntryRowView(entry: entry)
                    .tag(entry.entryId)
                    .onAppear { state.entryRowAppeared(entry.entryId) }
                    .onDrag {
                        guard let link = entry.link else { return NSItemProvider() }
                        let provider = NSItemProvider(object: link as NSURL)
                        provider.suggestedName = entry.displayTitle
                        return provider
                    }
            }
            // `j`, `k` and Space select from the keyboard: the list follows, or the selection
            // would move out of sight. On the next turn: a click changes the selection from
            // inside the table's delegate, where scrolling the table is a reentrant operation.
            .onChange(of: state.selectedEntryId) { _, id in
                guard let id else { return }
                Task { @MainActor in proxy.scrollTo(id) }
            }
        }
        .overlay {
            if state.entries.isEmpty {
                Text(state.emptyListMessage).foregroundStyle(.secondary)
            }
        }
    }
}

struct EntryRowView: View {
    let entry: EntryRow

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Circle().fill(entry.isRead ? .clear : Color.accentColor).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(entry.feedTitle).lineLimit(1)
                    Spacer()
                    if entry.isStarred { Image(systemName: "star.fill").foregroundStyle(.yellow) }
                    Text(entry.publishedAt, format: .relative(presentation: .named))
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                Text(entry.displayTitle)
                    .fontWeight(entry.isRead ? .regular : .semibold)
                    .foregroundStyle(entry.isRead ? .secondary : .primary)
                    .lineLimit(2)
                Text(entry.excerpt).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.vertical, 3)
    }
}

private extension CadenceGroup {
    var symbol: String {
        switch self {
        case .daily: "bolt"
        case .weekly: "calendar"
        case .monthly: "moon"
        case .rare: "sparkles"
        case .silent: "zzz"
        }
    }
}
