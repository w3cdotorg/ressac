import SwiftUI
import WebKit

/// The reader column (spec §7, Reader): the article page and its toolbar. It renders
/// `state.selectedEntry`, the complete row (list rows carry no content).
struct ReaderView: View {
    @Bindable var state: AppState

    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(ReaderStyle.Keys.font) private var font = ReaderFont.sfPro
    @AppStorage(ReaderStyle.Keys.size) private var size = ReaderStyle.defaultSize
    @AppStorage(ReaderStyle.Keys.width) private var width = LineWidth.medium
    @State private var controller = ReaderController()

    var body: some View {
        Group {
            if let entry = state.selectedEntry {
                ArticleWebView(page: page(for: entry), controller: controller)
                    .safeAreaInset(edge: .top, spacing: 0) { runFailureNotice }
            } else {
                ContentUnavailableView("No Entry Selected", systemImage: "doc.richtext",
                                       description: Text("Choose an entry in the list to read it here."))
            }
        }
        .toolbar { toolbarContent }
        .onAppear { state.readerPageDown = { [controller] in await controller.pageDown() } }
        .onDisappear { state.readerPageDown = nil }
    }

    private func page(for entry: EntryRow) -> ArticlePage {
        ArticlePage(entry: entry, fulltext: state.selectedFulltext, showFull: state.readerShowsFullText,
                    style: ReaderStyle(font: font, size: ReaderStyle.clampedSize(size), width: width),
                    colorScheme: colorScheme)
    }

    /// `fullTextNotice` when the extraction failed without storing an error row (the run itself
    /// failed); a stored error is already shown inside the page, so it is not repeated here.
    @ViewBuilder
    private var runFailureNotice: some View {
        if state.readerShowsFullText, let notice = state.fullTextNotice, state.selectedFulltext?.error == nil {
            Text(notice)
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .background(.bar)
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            if state.isExtracting {
                ProgressView().controlSize(.small)
            }
            Toggle(isOn: Binding(get: { state.readerShowsFullText }, set: { _ in state.requestFullText() })) {
                Label("Full Article", systemImage: "doc.plaintext")
            }
            .toggleStyle(.button)
            .help("Show the full article, fetched from the site")
            .disabled(state.selectedEntry?.link == nil || state.isExtracting)

            Button {
                state.toggleStar()
            } label: {
                let starred = state.selectedEntry?.isStarred == true
                Label(starred ? "Unstar" : "Star", systemImage: starred ? "star.fill" : "star")
            }
            .help("Star (S)")
            .disabled(!state.canToggleEntry)

            Button { state.openInBrowser() } label: {
                Label("Open in Browser", systemImage: "safari")
            }
            .help("Open in Browser (⌘↩)")
            .disabled(!state.canOpenInBrowser)

            if let entry = state.selectedEntry, let link = entry.link {
                ShareLink(item: link, subject: Text(entry.displayTitle))
                    .help("Share")
            }
        }
    }
}

/// Everything the page depends on. Read and starred flags are left out on purpose: marking the
/// entry read must not reload the page and lose the scroll position.
struct ArticlePage: Equatable {
    let entry: EntryRow
    let fulltext: FulltextRow?
    let showFull: Bool
    let style: ReaderStyle
    let colorScheme: ColorScheme

    static func == (a: ArticlePage, b: ArticlePage) -> Bool {
        a.entry.entryId == b.entry.entryId && a.entry.title == b.entry.title && a.entry.url == b.entry.url
            && a.entry.author == b.entry.author && a.entry.summaryHtml == b.entry.summaryHtml
            && a.entry.contentHtml == b.entry.contentHtml && a.fulltext == b.fulltext && a.showFull == b.showFull
            && a.style == b.style && a.colorScheme == b.colorScheme
    }

    func html() -> String {
        ArticleTemplate.page(for: entry, fulltext: fulltext, showFull: showFull, style: style, colorScheme: colorScheme)
    }
}

/// Lets `AppState.advance()` scroll the reader with Space (through `readerPageDown`).
@MainActor final class ReaderController {
    weak var webView: WKWebView?

    /// Scrolls one screen down. Returns `false` when the article already shows its end, so
    /// `AppState` moves on to the next unread entry.
    func pageDown() async -> Bool {
        guard let webView else { return false }
        let script = """
        const page = document.scrollingElement;
        if (page.scrollTop + window.innerHeight >= page.scrollHeight - 2) { return false; }
        window.scrollBy({ top: Math.round(window.innerHeight * 0.9), behavior: "smooth" });
        return true;
        """
        let result = try? await webView.callAsyncJavaScript(script, contentWorld: .page)
        return (result as? Bool) ?? false
    }
}

/// `WKWebView` with JavaScript off, no persistent website data, and every link sent to the browser.
struct ArticleWebView: NSViewRepresentable {
    let page: ArticlePage
    let controller: ReaderController

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.websiteDataStore = .nonPersistent()
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.allowsBackForwardNavigationGestures = false
        webView.allowsMagnification = false
        webView.underPageBackgroundColor = NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                ? NSColor(srgbRed: 0x1C / 255, green: 0x1C / 255, blue: 0x1E / 255, alpha: 1)
                : .white
        }
        controller.webView = webView
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        guard context.coordinator.page != page else { return }
        context.coordinator.page = page
        webView.loadHTMLString(page.html(), baseURL: nil)
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate {
        var page: ArticlePage?

        func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                     decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
            switch LinkPolicy.decide(type: navigationAction.navigationType, url: navigationAction.request.url) {
            case .allow:
                decisionHandler(.allow)
            case .cancel:
                decisionHandler(.cancel)
            case .openExternally(let url):
                decisionHandler(.cancel)
                NSWorkspace.shared.open(url)
            }
        }
    }
}
