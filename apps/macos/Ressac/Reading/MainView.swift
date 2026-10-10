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
                if state.selection.isGarden {
                    // Phase 12 replaces this with the Garden table.
                    Text("Garden").foregroundStyle(.secondary)
                } else {
                    EntryListView(state: state)
                }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 480)
        } detail: {
            if state.selection.isGarden {
                // Phase 12 replaces this with the Garden inspector.
                Text("No feed selected").foregroundStyle(.secondary)
            } else {
                // Phase 10 replaces this with the WKWebView reader. Pass the complete
                // row (`selectedEntry`), never a list row: list rows carry no content.
                ReaderPlaceholder(entry: state.selectedEntry)
            }
        }
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
                    ProgressView(value: progress.fraction) { Text(progress.label) }
                        .progressViewStyle(.linear)
                        .frame(width: 140)
                        .help(progress.label)
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

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(banner.message).lineLimit(2).textSelection(.enabled)
            Spacer()
            if banner.showLog {
                Button("Show Log") {
                    NSWorkspace.shared.open(URL.homeDirectory.appending(path: "Library/Logs/Ressac"))
                }
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
                Label("Unread", systemImage: "circle.inset.filled").badge(state.sidebar.unread).tag(Source.unread)
                Label("Today", systemImage: "sun.max").badge(state.sidebar.today).tag(Source.today)
                Label("Starred", systemImage: "star").badge(state.sidebar.starred).tag(Source.starred)
            }
            Section("Feeds") {
                OutlineGroup(state.sidebarTree, children: \.children) { node in
                    SidebarRow(node: node, state: state).tag(node.source)
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
                    Circle().fill(Self.color(for: status)).frame(width: 7, height: 7).help(status)
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

    static func color(for status: String) -> Color {
        switch status {
        case "Dead": .red
        case "Hijacked": .purple
        case "Moved": .blue
        case "Duplicate": .teal
        default: .orange
        }
    }
}

struct EntryListView: View {
    @Bindable var state: AppState

    var body: some View {
        List(state.entries, selection: $state.selectedEntryId) { entry in
            EntryRowView(entry: entry)
                .tag(entry.entryId)
                .onDrag {
                    guard let link = entry.link else { return NSItemProvider() }
                    let provider = NSItemProvider(object: link as NSURL)
                    provider.suggestedName = entry.displayTitle
                    return provider
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

/// Stand-in for the reader column until phase 10.
struct ReaderPlaceholder: View {
    let entry: EntryRow?

    var body: some View {
        if let entry {
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    Text(entry.displayTitle).font(.title)
                    Text(entry.feedTitle).foregroundStyle(.secondary)
                    Text(HTMLText.plain(entry.contentHtml ?? entry.summaryHtml ?? "", limit: 20_000))
                }
                .padding(24)
                .frame(maxWidth: 680, alignment: .leading)
            }
        } else {
            Text("No entry selected").foregroundStyle(.secondary)
        }
    }
}
