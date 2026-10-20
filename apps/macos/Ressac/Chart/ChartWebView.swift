import AppKit
import SwiftUI
import WebKit
import os

/// The two ways of laying out the chart (spec §2): from the harbour (distance is time) or by theme.
enum ChartLayout: String, CaseIterable, Sendable {
    case harbour, themes
}

/// The chart's inks (spec §2). A stored ink that is no longer offered ("pop", "pirate") reads back as `bleu`.
enum ChartInk: String, CaseIterable, Sendable {
    case bleu, nuit
}

/// The page's display choices, kept in `UserDefaults` (spec §8). The page owns them while it is open
/// and reports each change; the app hands them back at the next load.
struct ChartPrefs: Equatable, Sendable {
    var view: ChartLayout = .harbour
    var ink: ChartInk = .bleu
    var animate = true

    enum Keys {
        static let view = "chart.view"
        static let ink = "chart.ink"
        static let animate = "chart.animate"
    }

    /// The stored choices; a missing or unknown value falls back to the page's default.
    static func load(from defaults: UserDefaults) -> ChartPrefs {
        let fallback = ChartPrefs()
        return ChartPrefs(
            view: defaults.string(forKey: Keys.view).flatMap(ChartLayout.init(rawValue:)) ?? fallback.view,
            ink: defaults.string(forKey: Keys.ink).flatMap(ChartInk.init(rawValue:)) ?? fallback.ink,
            animate: defaults.object(forKey: Keys.animate) as? Bool ?? fallback.animate)
    }

    /// Stores what the page reported. Under Reduce Motion the page starts still whatever the stored
    /// choice, so its `animate` says nothing about the user's choice and the stored one is kept.
    func save(to defaults: UserDefaults, reduceMotion: Bool) {
        defaults.set(view.rawValue, forKey: Keys.view)
        defaults.set(ink.rawValue, forKey: Keys.ink)
        if !reduceMotion { defaults.set(animate, forKey: Keys.animate) }
    }

    /// The `prefs` argument of `Chart.load`: Reduce Motion starts the page with the animation off.
    func pageArgument(reduceMotion: Bool) -> [String: Any] {
        ["view": view.rawValue, "ink": ink.rawValue, "animate": animate && !reduceMotion]
    }
}

/// What the page may ask of the app (spec §8).
enum ChartAction: Equatable, Sendable {
    case openFeed(Int64)
    case openURL(URL)
    case prefs(ChartPrefs)
}

/// Reads the page's messages. Only the three known types, with well-formed fields, become actions;
/// only http and https addresses with a host may be opened. Anything else is ignored.
struct ChartBridge {
    func handle(_ body: Any) -> ChartAction? {
        guard let message = body as? [String: Any], let type = message["type"] as? String else { return nil }
        switch type {
        case "openFeed":
            return Self.integer(message["id"]).map(ChartAction.openFeed)
        case "openURL":
            return LinkPolicy.webURL(message["url"] as? String).map(ChartAction.openURL)
        case "prefs":
            guard let view = (message["view"] as? String).flatMap(ChartLayout.init(rawValue:)),
                  let ink = (message["ink"] as? String).flatMap(ChartInk.init(rawValue:)),
                  let animate = Self.boolean(message["animate"])
            else { return nil }
            return .prefs(ChartPrefs(view: view, ink: ink, animate: animate))
        default:
            return nil
        }
    }

    /// A JavaScript number (a double) that is a whole `Int64`; never a boolean.
    private static func integer(_ value: Any?) -> Int64? {
        guard let number = value as? NSNumber, !isBoolean(number) else { return nil }
        return Int64(exactly: number.doubleValue)
    }

    /// A JavaScript boolean; never a number.
    private static func boolean(_ value: Any?) -> Bool? {
        guard let number = value as? NSNumber, isBoolean(number) else { return nil }
        return number.boolValue
    }

    private static func isBoolean(_ number: NSNumber) -> Bool {
        CFGetTypeID(number) == CFBooleanGetTypeID()
    }
}

/// The bundled chart page in its web view (spec §7, §8): loads `index.html` with read access to the
/// Chart folder only, hands it documents through `Chart.load` then `Chart.update` (as call arguments,
/// never spliced into the script), turns its messages into actions, and cancels every navigation
/// that is not the bundled page and every new window.
@MainActor final class ChartPage: NSObject, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler {
    /// The bundled page, in the `Chart` folder resource.
    nonisolated static var indexURL: URL? {
        Bundle.main.url(forResource: "Chart", withExtension: nil)?.appending(path: "index.html")
    }

    static let handlerName = "ressac"

    let webView: WKWebView
    var onAction: @MainActor (ChartAction) -> Void

    /// The `prefs` argument of each `Chart.load`, read at each load (a reload follows what the page reported).
    private let prefs: @MainActor () -> [String: Any]
    private let bridge = ChartBridge()
    /// The newest document handed in, drawn or still waiting for the page.
    private var latest: String?
    /// The document the page has, or nil until `Chart.load` ran on this load of the page.
    private var shown: String?
    private var isLoaded = false

    private static let log = Logger(subsystem: "org.w3cdotorg.ressac", category: "chart")

    init(prefs: @escaping @MainActor () -> [String: Any], onAction: @escaping @MainActor (ChartAction) -> Void) {
        self.prefs = prefs
        self.onAction = onAction
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        webView = WKWebView(frame: .zero, configuration: configuration)
        webView.allowsBackForwardNavigationGestures = false
        webView.allowsMagnification = false
        webView.allowsLinkPreview = false
        super.init()
        // The content controller holds its handlers strongly: a weak proxy avoids a cycle through the web view.
        configuration.userContentController.add(WeakMessageHandler(self), name: Self.handlerName)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        loadPage()
    }

    /// Draws `json` (a §6 document): at once when the page is ready, else once it has loaded.
    func show(_ json: String) {
        latest = json
        guard isLoaded, json != shown else { return }
        send(json)
    }

    /// The window closed: no more messages, navigation or loads.
    func close() {
        webView.configuration.userContentController.removeScriptMessageHandler(forName: Self.handlerName)
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
        webView.stopLoading()
        isLoaded = false
    }

    /// True only for the bundled page itself (no query): every other address, bundled or not, is refused.
    nonisolated static func allowsNavigation(to url: URL?, page: URL) -> Bool {
        guard let url, url.isFileURL, url.query() == nil else { return false }
        return url.standardizedFileURL.resolvingSymlinksInPath().path
            == page.standardizedFileURL.resolvingSymlinksInPath().path
    }

    private func loadPage() {
        guard let index = Self.indexURL else {
            Self.log.error("the Chart folder is missing from the app bundle")
            return
        }
        isLoaded = false
        shown = nil
        webView.loadFileURL(index, allowingReadAccessTo: index.deletingLastPathComponent())
    }

    /// The first document of a page load goes through `Chart.load` with the prefs, the next ones through `Chart.update`.
    private func send(_ json: String) {
        let first = shown == nil
        shown = json
        let script = first ? "Chart.load(json, prefs)" : "Chart.update(json)"
        let arguments: [String: Any] = first ? ["json": json, "prefs": prefs()] : ["json": json]
        webView.callAsyncJavaScript(script, arguments: arguments, in: nil, in: .page) { result in
            if case let .failure(error) = result {
                Self.log.error("the chart page refused a document: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    // MARK: WKNavigationDelegate

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
        let allowed = Self.indexURL.map { Self.allowsNavigation(to: navigationAction.request.url, page: $0) } ?? false
        decisionHandler(allowed && navigationAction.targetFrame?.isMainFrame == true ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // A first load or a reload (context menu): the page starts empty either way.
        isLoaded = true
        shown = nil
        if let latest { send(latest) }
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        Self.log.error("the chart page's web process ended; reloading")
        loadPage()
    }

    // MARK: WKUIDelegate

    /// No new windows: links in the popover go through the bridge, not through WebKit.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        nil
    }

    // MARK: WKScriptMessageHandler

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame, let action = bridge.handle(message.body) else { return }
        onAction(action)
    }
}

/// Forwards script messages to a handler it does not retain.
@MainActor private final class WeakMessageHandler: NSObject, WKScriptMessageHandler {
    private weak var target: (any WKScriptMessageHandler)?

    init(_ target: any WKScriptMessageHandler) {
        self.target = target
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(userContentController, didReceive: message)
    }
}

/// The chart page in a SwiftUI view: one `ChartPage` per view, closed when the view goes away.
struct ChartWebView: NSViewRepresentable {
    let json: String
    let prefs: @MainActor () -> [String: Any]
    let onAction: @MainActor (ChartAction) -> Void

    func makeCoordinator() -> ChartPage {
        ChartPage(prefs: prefs, onAction: onAction)
    }

    func makeNSView(context: Context) -> WKWebView {
        context.coordinator.webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.onAction = onAction
        context.coordinator.show(json)
    }

    static func dismantleNSView(_ webView: WKWebView, coordinator: ChartPage) {
        coordinator.close()
    }
}
