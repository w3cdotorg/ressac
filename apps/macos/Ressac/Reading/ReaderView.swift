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
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var antiopeSize = TeletextFont.defaultSize
    @AppStorage(Appearance.Keys.appearance) private var storedAppearance = Appearance.native
    @AppStorage(Appearance.Keys.crt) private var crt = false
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(AntiopeState.self) private var antiope: AntiopeState?
    @State private var controller = ReaderController()

    var body: some View {
        Group {
            if let entry = state.selectedEntry {
                ArticleWebView(page: page(for: entry), controller: controller)
                    .safeAreaInset(edge: .top, spacing: 0) { runFailureNotice }
            } else if antiope != nil {
                AntiopeReaderLine { AntiopeReaderLines.noEntry(width: $0) }
            } else {
                ContentUnavailableView("No Entry Selected", systemImage: "doc.richtext",
                                       description: Text("Choose an entry in the list to read it here."))
            }
        }
        .toolbar { toolbarContent }
        .onAppear {
            state.readerPageDown = { [controller] id in await controller.pageDown(entryId: id) }
            antiope?.imageToggle = { [controller] in Task { await controller.toggleAllImages() } }
        }
        .onDisappear {
            state.readerPageDown = nil
            antiope?.imageToggle = nil
        }
    }

    private func page(for entry: EntryRow) -> ArticlePage {
        let theme = theme
        return ArticlePage(entry: entry, fulltext: state.selectedFulltext, showFull: state.readerShowsFullText,
                           style: ReaderStyle(font: font, size: ReaderStyle.effectiveSize(theme.appearance == .antiope ? antiopeSize : size,
                                                                                         appearance: theme.appearance), width: width),
                           colorScheme: colorScheme, theme: theme, subpage: subpage(for: entry, theme: theme))
    }

    /// Antiope only when Bedstead is there (spec §5). `crt` is true exactly when the CRT bleed
    /// applies: Antiope, the CRT screen box checked, Increase contrast off (spec §9).
    private var theme: ReaderTheme {
        let appearance = Appearance.effective(stored: storedAppearance, fontAvailable: TeletextFont.isAvailable)
        return ReaderTheme(appearance: appearance,
                           crt: CRTGeometry.bleedIsActive(CRTGeometry.isActive(appearance: appearance, crt: crt),
                                                          contrast: contrast))
    }

    /// "412 3/20" in Antiope (spec §6); nil in Native, so the native page never changes with it.
    private func subpage(for entry: EntryRow, theme: ReaderTheme) -> SubpageLabel? {
        guard theme.appearance == .antiope, let directory = antiope?.directory else { return nil }
        return SubpageLabel.of(entryId: entry.entryId, in: state.entries, total: state.entryTotal, page: directory.page(forFeed:))
    }

    /// `fullTextNotice` when the extraction failed without storing an error row (the run itself
    /// failed); a stored error is already shown inside the page, so it is not repeated here.
    @ViewBuilder
    private var runFailureNotice: some View {
        if state.readerShowsFullText, let notice = state.fullTextNotice, state.selectedFulltext?.error == nil {
            if antiope != nil {
                AntiopeReaderLine(fillsHeight: false) { AntiopeReaderLines.failureNotice(notice, width: $0) }
            } else {
                Text(notice)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 6)
                    .background(.bar)
            }
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

/// An entry's place in the list shown, under its own feed's page (spec §4, Sub-pages): "412 3/20".
struct SubpageLabel: Equatable, Sendable {
    var page: Int?
    var index: Int
    var count: Int

    /// "412 3/20", or "3/20" for a feed without a page number yet.
    var text: String { (page.map { "\($0) " } ?? "") + "\(index)/\(count)" }

    /// Numbered from 1 in `entries`, out of `total`: the whole list's size when only its first
    /// rows are loaded, `entries.count` when nil. Nil when the entry is not in the list (filtered out).
    static func of(entryId: Int64, in entries: [EntryRow], total: Int? = nil, page: (Int64) -> Int?) -> SubpageLabel? {
        guard let position = entries.firstIndex(where: { $0.entryId == entryId }) else { return nil }
        return SubpageLabel(page: page(entries[position].feedId), index: position + 1, count: total ?? entries.count)
    }
}

/// Everything the page depends on. Read and starred flags are left out on purpose: marking the
/// entry read must not reload the page and lose the scroll position. The sub-page line is left
/// out too: the list grows during a refresh, and `ArticleWebView` rewrites that line in place.
struct ArticlePage: Equatable {
    let entry: EntryRow
    let fulltext: FulltextRow?
    let showFull: Bool
    let style: ReaderStyle
    let colorScheme: ColorScheme
    var theme: ReaderTheme = .native
    var subpage: SubpageLabel?

    static func == (a: ArticlePage, b: ArticlePage) -> Bool {
        a.entry.entryId == b.entry.entryId && a.entry.title == b.entry.title && a.entry.url == b.entry.url
            && a.entry.author == b.entry.author && a.entry.summaryHtml == b.entry.summaryHtml
            && a.entry.contentHtml == b.entry.contentHtml && a.fulltext == b.fulltext && a.showFull == b.showFull
            && a.style == b.style && a.colorScheme == b.colorScheme && a.theme == b.theme
    }

    func html() -> String {
        ArticleTemplate.page(for: entry, fulltext: fulltext, showFull: showFull, style: style, colorScheme: colorScheme,
                             theme: theme, subpage: subpage)
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

    /// Swaps mosaic `index` and its original (a click on the mosaic, spec §7). Returns whether the
    /// original is now shown. App JavaScript; the page's own stays off.
    @discardableResult
    func toggleImage(_ index: Int) async -> Bool {
        guard let webView else { return false }
        let script = """
        const box = document.getElementById("m" + index);
        if (!box) { return false; }
        const original = !box.classList.contains("original");
        box.classList.toggle("original", original);
        box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic;
        return original;
        """
        let result = try? await webView.callAsyncJavaScript(script, arguments: ["index": index], contentWorld: .page)
        return (result as? Bool) ?? false
    }

    /// `i`: every image of the article both ways, originals while any mosaic shows, else mosaics.
    /// Returns how many images it swapped.
    @discardableResult
    func toggleAllImages() async -> Int {
        guard let webView else { return 0 }
        let script = """
        const boxes = Array.from(document.querySelectorAll(".mosaic"));
        const original = boxes.some((box) => !box.classList.contains("original"));
        for (const box of boxes) {
          box.classList.toggle("original", original);
          box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic;
        }
        return boxes.length;
        """
        let result = try? await webView.callAsyncJavaScript(script, contentWorld: .page)
        return (result as? Int) ?? 0
    }

    /// Rewrites the Antiope sub-page line of the page on screen, keeping its scroll position.
    func showSubpage(_ label: SubpageLabel?) async {
        guard let webView else { return }
        let script = #"const line = document.getElementById("subpage"); if (line) { line.textContent = text; }"#
        _ = try? await webView.callAsyncJavaScript(script, arguments: ["text": label?.text ?? ""], contentWorld: .page)
    }
}

/// What the Antiope reader column draws besides the article, as cell rows (spec §2, §5): no
/// system font, symbol or grey, whatever the page shows.
enum AntiopeReaderLines {
    /// Nothing selected, for instance after typing a page number: one centred cyan row.
    static func noEntry(width: Int) -> CellLine {
        var line = CellLine.centred("NO ENTRY SELECTED", width: width, foreground: .cyan)
        line.accessibilityText = "No entry selected"
        return line
    }

    /// The run-failure notice: reverse video, black on red, one row of exactly `width` cells.
    static func failureNotice(_ text: String, width: Int) -> CellLine {
        CellLine([CellSpan(" " + text, cells: width, foreground: .black, background: .red)], accessibilityText: text)
    }
}

/// One `AntiopeReaderLines` row sized to the reader column: centred in it, or one row tall.
private struct AntiopeReaderLine: View {
    let fillsHeight: Bool
    let line: (Int) -> CellLine
    @Environment(\.teletextSize) private var size

    init(fillsHeight: Bool = true, _ line: @escaping (Int) -> CellLine) {
        self.fillsHeight = fillsHeight
        self.line = line
    }

    var body: some View {
        let cell = TeletextFont.cell(forSize: size)
        GeometryReader { proxy in
            CellLineView(line: line(max(0, Int(proxy.size.width / cell.width))))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: fillsHeight ? .center : .topLeading)
        }
        .frame(height: fillsHeight ? nil : cell.height)
        .background(TeletextColor.black.color)
    }
}

/// `WKWebView` with JavaScript off and no persistent website data. Links go to the browser, except
/// the mosaic toggle links, which the page handles in-app.
struct ArticleWebView: NSViewRepresentable {
    let page: ArticlePage
    let controller: ReaderController

    func makeCoordinator() -> Coordinator { Coordinator(controller: controller) }

    func makeNSView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero, configuration: Self.configuration())
        webView.navigationDelegate = context.coordinator
        webView.allowsBackForwardNavigationGestures = false
        webView.allowsMagnification = false
        controller.webView = webView
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        webView.underPageBackgroundColor = page.theme.appearance == .antiope ? .black : Self.nativeUnderPage
        guard context.coordinator.page != page else {
            // Same document: only the sub-page line may have changed (the list moved under it).
            guard context.coordinator.page?.subpage != page.subpage else { return }
            context.coordinator.page = page
            if controller.loadedEntryId == page.entry.entryId {
                Task { await controller.showSubpage(page.subpage) }
            } else {
                controller.load(entryId: page.entry.entryId, html: page.html())  // still loading: nothing to keep
            }
            return
        }
        context.coordinator.page = page
        controller.load(entryId: page.entry.entryId, html: page.html())
    }

    /// JavaScript off, no persistent website data, and the mosaic handler (spec §7).
    static func configuration(mosaics: MosaicSchemeHandler = MosaicSchemeHandler()) -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.websiteDataStore = .nonPersistent()
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        configuration.setURLSchemeHandler(mosaics, forURLScheme: MosaicSchemeHandler.scheme)
        return configuration
    }

    /// The colour past the page's edges in Native; Antiope's is black (spec §7).
    private static let nativeUnderPage = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 0x1C / 255, green: 0x1C / 255, blue: 0x1E / 255, alpha: 1)
            : .white
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
            case .toggleImage(let index):
                decisionHandler(.cancel)
                Task { await controller.toggleImage(index) }
            }
        }
    }
}
