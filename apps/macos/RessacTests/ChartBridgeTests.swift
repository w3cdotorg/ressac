import AppKit
import Foundation
import Testing
import WebKit

@testable import Ressac

/// A private defaults suite: the test host is Ressac.app, whose standard defaults are the user's.
private func privateDefaults() throws -> (UserDefaults, String) {
    let suite = "ressac.tests.\(UUID().uuidString)"
    return (try #require(UserDefaults(suiteName: suite)), suite)
}

@Suite struct ChartBridgeTests {
    let bridge = ChartBridge()

    @Test func bridge_accepts_known_messages() {
        #expect(bridge.handle(["type": "openFeed", "id": NSNumber(value: 42)]) == .openFeed(42))
        // JavaScript numbers are doubles: a whole one is an id.
        #expect(bridge.handle(["type": "openFeed", "id": NSNumber(value: 7.0)]) == .openFeed(7))
        #expect(bridge.handle(["type": "openURL", "url": "https://example.org/a?b=1"])
            == .openURL(URL(string: "https://example.org/a?b=1")!))
        #expect(bridge.handle(["type": "openURL", "url": "http://example.org/"]) == .openURL(URL(string: "http://example.org/")!))
        #expect(bridge.handle(["type": "prefs", "view": "themes", "ink": "pirate", "animate": NSNumber(value: false)])
            == .prefs(ChartPrefs(view: .themes, ink: .pirate, animate: false)))
    }

    @Test func bridge_refuses_file_and_javascript_urls() {
        for url in ["file:///etc/passwd", "javascript:alert(1)", "data:text/html,<p>x</p>", "ftp://example.org/",
                    "ressac-toggle:1", "http://", "https:no-host", "not a url", ""] {
            #expect(bridge.handle(["type": "openURL", "url": url]) == nil, "\(url)")
        }
        #expect(bridge.handle(["type": "openURL", "url": NSNumber(value: 1)]) == nil)
        #expect(bridge.handle(["type": "openURL"]) == nil)
    }

    @Test func bridge_ignores_unknown_types() {
        #expect(bridge.handle(["type": "navigate", "url": "https://example.org/"]) == nil)
        #expect(bridge.handle(["type": "OPENFEED", "id": 1]) == nil)
        #expect(bridge.handle(["id": 1]) == nil)
        #expect(bridge.handle("openFeed") == nil)
        #expect(bridge.handle(["openFeed", 1]) == nil)
        #expect(bridge.handle(NSNull()) == nil)
        // Known types with malformed fields.
        #expect(bridge.handle(["type": "openFeed", "id": "42"]) == nil)
        #expect(bridge.handle(["type": "openFeed", "id": NSNumber(value: 1.5)]) == nil)
        #expect(bridge.handle(["type": "openFeed", "id": NSNumber(value: true)]) == nil)
        #expect(bridge.handle(["type": "openFeed", "id": NSNumber(value: 1e300)]) == nil)
        #expect(bridge.handle(["type": "prefs", "view": "sideways", "ink": "bleu", "animate": NSNumber(value: true)]) == nil)
        #expect(bridge.handle(["type": "prefs", "view": "harbour", "ink": "green", "animate": NSNumber(value: true)]) == nil)
        #expect(bridge.handle(["type": "prefs", "view": "harbour", "ink": "bleu", "animate": NSNumber(value: 1)]) == nil)
        #expect(bridge.handle(["type": "prefs", "view": "harbour", "ink": "bleu"]) == nil)
    }

    @Test func prefs_persist() throws {
        let (defaults, suite) = try privateDefaults()
        defer { defaults.removePersistentDomain(forName: suite) }
        // Nothing stored: the page's defaults.
        #expect(ChartPrefs.load(from: defaults) == ChartPrefs(view: .harbour, ink: .bleu, animate: true))

        ChartPrefs(view: .themes, ink: .nuit, animate: false).save(to: defaults, reduceMotion: false)
        #expect(defaults.string(forKey: "chart.view") == "themes")
        #expect(defaults.string(forKey: "chart.ink") == "nuit")
        #expect(defaults.object(forKey: "chart.animate") as? Bool == false)
        #expect(ChartPrefs.load(from: defaults) == ChartPrefs(view: .themes, ink: .nuit, animate: false))

        // Unknown stored values fall back one by one.
        defaults.set("sideways", forKey: "chart.view")
        #expect(ChartPrefs.load(from: defaults) == ChartPrefs(view: .harbour, ink: .nuit, animate: false))
    }

    /// Reduce Motion starts the page with the animation off (spec §8), and the page's forced-off
    /// state is not mistaken for the user's choice when it reports another change.
    @Test func reduce_motion_starts_still_and_keeps_the_stored_choice() throws {
        let (defaults, suite) = try privateDefaults()
        defer { defaults.removePersistentDomain(forName: suite) }
        let prefs = ChartPrefs(view: .themes, ink: .pop, animate: true)
        #expect(prefs.pageArgument(reduceMotion: false) as NSDictionary
            == ["view": "themes", "ink": "pop", "animate": true] as NSDictionary)
        #expect(prefs.pageArgument(reduceMotion: true) as NSDictionary
            == ["view": "themes", "ink": "pop", "animate": false] as NSDictionary)

        prefs.save(to: defaults, reduceMotion: false)
        ChartPrefs(view: .harbour, ink: .pirate, animate: false).save(to: defaults, reduceMotion: true)
        #expect(ChartPrefs.load(from: defaults) == ChartPrefs(view: .harbour, ink: .pirate, animate: true))
    }

    @Test func only_the_bundled_page_is_navigable() throws {
        let index = try #require(ChartPage.indexURL)
        #expect(index.lastPathComponent == "index.html")
        #expect(index.deletingLastPathComponent().lastPathComponent == "Chart")
        #expect(ChartPage.allowsNavigation(to: index, page: index))
        #expect(ChartPage.allowsNavigation(to: URL(filePath: index.path), page: index))
        let folder = index.deletingLastPathComponent()
        for url in [URL(string: "https://example.org/")!, URL(string: "http://example.org/index.html")!,
                    folder.appending(path: "chart.css"), folder.appending(path: "LICENSE-d3.txt"),
                    URL(filePath: "/etc/passwd"), URL(string: "about:blank")!, URL(string: "data:text/html,x")!,
                    URL(string: index.absoluteString + "?x=1")!] {
            #expect(!ChartPage.allowsNavigation(to: url, page: index), "\(url)")
        }
        #expect(!ChartPage.allowsNavigation(to: nil, page: index))
    }
}

/// What the window does with each message (spec §8).
@MainActor @Suite struct ChartActionTests {
    @Test func open_feed_selects_it() throws {
        let (state, _, _) = try makeState()
        let (defaults, suite) = try privateDefaults()
        defer { defaults.removePersistentDomain(forName: suite) }
        ChartActions.perform(.openFeed(3), state: state, defaults: defaults, reduceMotion: false)
        #expect(state.selection == .feed(3))
    }

    @Test func open_url_goes_to_the_system_opener() throws {
        let (state, _, _) = try makeState()
        let (defaults, suite) = try privateDefaults()
        defer { defaults.removePersistentDomain(forName: suite) }
        var opened: [URL] = []
        state.openURL = { opened.append($0) }
        ChartActions.perform(.openURL(URL(string: "https://example.org/")!), state: state, defaults: defaults, reduceMotion: false)
        #expect(opened == [URL(string: "https://example.org/")!])
    }

    @Test func prefs_are_stored() throws {
        let (state, _, _) = try makeState()
        let (defaults, suite) = try privateDefaults()
        defer { defaults.removePersistentDomain(forName: suite) }
        let prefs = ChartPrefs(view: .themes, ink: .pirate, animate: false)
        ChartActions.perform(.prefs(prefs), state: state, defaults: defaults, reduceMotion: false)
        #expect(ChartPrefs.load(from: defaults) == prefs)
    }
}

/// The bundled page in a real WKWebView (offscreen: the window is never shown), not just in a browser.
@MainActor @Suite(.serialized) struct ChartPageTests {
    static let fixture = TestDatabase.repoRoot.appending(path: "apps/macos/ChartTests/fixture.json")

    /// Records what the page sends to the app.
    @MainActor final class Recorder {
        var actions: [ChartAction] = []
    }

    @MainActor struct Harness {
        let window: NSWindow
        let page: ChartPage
        let recorder: Recorder

        func js(_ script: String, _ arguments: [String: Any] = [:]) async -> Any? {
            try? await page.webView.callAsyncJavaScript(script, arguments: arguments, contentWorld: .page)
        }
    }

    func harness(prefs: ChartPrefs = ChartPrefs(), reduceMotion: Bool = true) -> Harness {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1280, height: 720), styleMask: [.titled],
                              backing: .buffered, defer: false)
        let recorder = Recorder()
        let page = ChartPage(prefs: { prefs.pageArgument(reduceMotion: reduceMotion) },
                             onAction: { recorder.actions.append($0) })
        page.webView.frame = window.contentLayoutRect
        window.contentView = page.webView
        return Harness(window: window, page: page, recorder: recorder)
    }

    /// Distinct colours among sampled pixels of the base canvas, and a fingerprint of them; 0 while it is not sized.
    static let sample = """
        const c = document.querySelector("canvas");
        if (!c || !c.width || !c.height) return [0, ""];
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        const seen = new Set(); let print = 0;
        for (let i = 0; i < d.length; i += 4 * 61) {
          const v = (d[i] << 16) | (d[i + 1] << 8) | d[i + 2];
          seen.add(v); print = (print * 31 + v) >>> 0;
        }
        return [seen.size, String(print)];
        """

    func sample(_ h: Harness) async -> (colours: Int, print: String) {
        let pair = await h.js(Self.sample) as? [Any]
        return ((pair?.first as? Int) ?? 0, (pair?.last as? String) ?? "")
    }

    func waitForDrawing(_ h: Harness, differentFrom print: String? = nil) async throws -> (colours: Int, print: String) {
        var drawn = (colours: 0, print: "")
        for _ in 0..<200 {
            drawn = await sample(h)
            if drawn.colours > 8, drawn.print != print { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        return drawn
    }

    @Test func the_page_draws_the_fixture_in_webkit() async throws {
        let json = try String(contentsOf: Self.fixture, encoding: .utf8)
        let h = harness(prefs: ChartPrefs(view: .themes, ink: .nuit, animate: true))
        defer { h.page.close() }
        h.page.show(json)
        let first = try await waitForDrawing(h)
        #expect(first.colours > 8, "the base canvas stayed blank")

        // Everything came from the bundle under the page's CSP: scripts, stylesheet, fonts.
        #expect(await h.js("return typeof Chart.load") as? String == "function")
        #expect(await h.js(#"return getComputedStyle(document.querySelector(".bar")).position"#) as? String == "fixed")
        // A face blocked by the CSP would end in "error"; the faces the canvas draws with are loaded.
        // (Italic 400, used only by the popover's footer in CSS, loads when the popover first shows.)
        let faces = await h.js(#"return [...document.fonts].map(f => `${f.family} ${f.style} ${f.weight} ${f.status}`)"#) as? [String] ?? []
        #expect(faces.count == 8 && !faces.contains { $0.hasSuffix(" error") }, "\(faces)")
        for face in ["Cormorant Garamond normal 500", "Cormorant Garamond italic 500", "Cormorant SC normal 600", "IBM Plex Mono normal 400"] {
            #expect(faces.contains(face + " loaded"), "\(face): \(faces)")
        }
        // The prefs reached Chart.load (Reduce Motion in `pageArgument` is covered by ChartBridgeTests).
        #expect(await h.js("return document.documentElement.dataset.ink") as? String == "nuit")
        #expect(await h.js(#"return [...document.querySelectorAll(".bar button")].find(b => b.textContent === "By theme").getAttribute("aria-pressed")"#) as? String == "true")

        // A new document goes through Chart.update and redraws.
        var data = try #require(try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
        data["feeds"] = Array((data["feeds"] as? [Any] ?? []).prefix(3))
        data["territories"] = []
        let smaller = String(decoding: try JSONSerialization.data(withJSONObject: data), as: UTF8.self)
        h.page.show(smaller)
        let second = try await waitForDrawing(h, differentFrom: first.print)
        #expect(second.print != first.print, "Chart.update did not redraw")
    }

    @Test func page_messages_reach_the_app_through_the_bridge() async throws {
        let json = try String(contentsOf: Self.fixture, encoding: .utf8)
        let h = harness(prefs: ChartPrefs(view: .themes, ink: .pop, animate: false))
        defer { h.page.close() }
        h.page.show(json)
        _ = try await waitForDrawing(h)

        // The page's own toolbar reports the new view.
        _ = await h.js(#"[...document.querySelectorAll(".bar button")].find(b => b.textContent === "From the harbour").click()"#)
        #expect(await eventually { h.recorder.actions == [.prefs(ChartPrefs(view: .harbour, ink: .pop, animate: false))] })

        _ = await h.js(#"window.webkit.messageHandlers.ressac.postMessage({type: "openFeed", id: 12})"#)
        _ = await h.js(#"window.webkit.messageHandlers.ressac.postMessage({type: "openURL", url: "file:///etc/passwd"})"#)
        _ = await h.js(#"window.webkit.messageHandlers.ressac.postMessage({type: "openURL", url: "javascript:alert(1)"})"#)
        _ = await h.js(#"window.webkit.messageHandlers.ressac.postMessage({type: "openURL", url: "https://example.invalid/x"})"#)
        #expect(await eventually { h.recorder.actions.count == 3 })
        try await Task.sleep(for: .milliseconds(200))
        #expect(h.recorder.actions == [.prefs(ChartPrefs(view: .harbour, ink: .pop, animate: false)), .openFeed(12),
                                       .openURL(URL(string: "https://example.invalid/x")!)])
    }

    @Test func navigation_away_from_the_page_is_cancelled() async throws {
        let json = try String(contentsOf: Self.fixture, encoding: .utf8)
        let h = harness()
        defer { h.page.close() }
        h.page.show(json)
        _ = try await waitForDrawing(h)
        let index = try #require(ChartPage.indexURL)
        func stillHome() async -> Bool {
            await h.js("return typeof Chart") as? String == "object"
                && h.page.webView.url.map { ChartPage.allowsNavigation(to: $0, page: index) } == true
        }

        // Targets that would load if allowed (a bundled file, a blank page), and a web address: through a
        // link (what "Open Link" in the context menu and a middle click do) and through a script.
        for target in ["LICENSE-d3.txt", "about:blank", "https://example.invalid/"] {
            _ = await h.js(#"const a = document.createElement("a"); a.href = target; document.body.append(a); a.click()"#,
                           ["target": target])
            try await Task.sleep(for: .milliseconds(300))
            #expect(await stillHome(), "link to \(target)")
            _ = await h.js("location.href = target", ["target": target])
            try await Task.sleep(for: .milliseconds(300))
            #expect(await stillHome(), "location to \(target)")
        }
        // New windows are refused.
        #expect(await h.js(#"return window.open("https://example.invalid/") === null"#) as? Bool == true)
        #expect(h.recorder.actions.isEmpty)
    }
}
