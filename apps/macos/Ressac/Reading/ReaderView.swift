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
        .onAppear { state.readerPageDown = { [controller] id in await controller.pageDown(entryId: id) } }
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
        // Its own item, titled: an icon alone in the toolbar's glass group went unnoticed
        // (acceptance fix 2). The button's tint tells on from off; the tooltip says why it is off.
        ToolbarSpacer(.fixed, placement: .primaryAction)
        ToolbarItem(placement: .primaryAction) {
            HStack(spacing: 6) {
                if state.isExtracting {
                    ProgressView().controlSize(.small)
                }
                Toggle(isOn: Binding(get: { state.isShowingFullText }, set: { _ in state.requestFullText() })) {
                    Label("Full Article", systemImage: "doc.plaintext")
                }
                .toggleStyle(.button)
                .labelStyle(.titleAndIcon)
                .help(Self.fullArticleHelp(for: state.selectedEntry, isShowingFullText: state.isShowingFullText,
                                           isExtracting: state.isExtracting, failure: state.fullTextFailure))
                .disabled(!state.canRequestFullText)
            }
        }
        ToolbarSpacer(.fixed, placement: .primaryAction)
        ToolbarItemGroup(placement: .primaryAction) {
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

extension ReaderView {
    /// The Full Article button's tooltip, which also explains why it is disabled.
    static func fullArticleHelp(for entry: EntryRow?, isShowingFullText: Bool, isExtracting: Bool,
                                failure: String?) -> String {
        guard let entry else { return "Select an entry to read its full article" }
        guard entry.link != nil else { return "This entry has no web page to fetch the full article from" }
        if isExtracting { return "Fetching the full article…" }
        if let failure { return failure + (failure.hasSuffix(".") ? " " : ". ") + "Click to try again" }
        return isShowingFullText
            ? "Showing the full article. Click to show the feed's text"
            : "Show the full article, fetched from the site"
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
    /// The entry whose page is on screen (its navigation committed); nil until then.
    private(set) var loadedEntryId: Int64?
    private var loadingEntryId: Int64?
    private var navigation: WKNavigation?

    func load(entryId: Int64, html: String) {
        loadedEntryId = nil
        loadingEntryId = entryId
        navigation = webView?.loadHTMLString(html, baseURL: nil)
    }

    /// Called by the navigation delegate once the new document replaced the old one, without
    /// waiting for its images (the load event): a slow image or a hanging tracking pixel must not
    /// keep Space dead. A failed load counts too, or Space would stay spent.
    func navigationCommitted(_ committed: WKNavigation?) {
        guard committed === navigation else { return }  // a load that a newer one replaced
        loadedEntryId = loadingEntryId
    }

    /// Scrolls one screen of `entryId`'s page down. Returns `false` when the article already shows
    /// its end, so `AppState` moves on to the next unread entry, and `true` without scrolling
    /// until that entry's document is committed: Space must not read the previous page's
    /// position (acceptance fix, review rounds 1 and 2).
    func pageDown(entryId: Int64) async -> Bool {
        guard let webView else { return false }
        guard loadedEntryId == entryId else { return true }
        // Committed but still parsing: the height is not known yet, so the key is spent.
        let script = """
        if (document.readyState === "loading") { return true; }
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

    func makeCoordinator() -> Coordinator { Coordinator(controller: controller) }

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
        controller.load(entryId: page.entry.entryId, html: page.html())
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate {
        var page: ArticlePage?
        let controller: ReaderController

        init(controller: ReaderController) {
            self.controller = controller
        }

        func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
            controller.navigationCommitted(navigation)
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error) {
            controller.navigationCommitted(navigation)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: any Error) {
            controller.navigationCommitted(navigation)
        }

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
