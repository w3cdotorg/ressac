# Phase 10: Reader web view, sanitised article template, `reader.css`, reader style

Read the plan index first. Global Constraints apply. Spec §7 (Reader: rendering, safety, content shown, toolbar; Settings: font, size, width), §8 (extraction failure shown inline), Review Focus #3 (and the rendering side of #1).

**Files (5):**
- Create: `apps/macos/Ressac/Settings/ReaderStyle.swift`
- Create: `apps/macos/Ressac/Reading/ArticleTemplate.swift` (also holds `LinkPolicy`, the reader's navigation rule)
- Create: `apps/macos/Ressac/Resources/reader.css`
- Create: `apps/macos/Ressac/Reading/ReaderView.swift`
- Create: `apps/macos/RessacTests/ReaderTests.swift`

**Wiring is in Phase 12.** Phase 9's detail column shows `ReaderPlaceholder(entry: state.selectedEntry)` in `Reading/MainView.swift`. Swapping that line for `ReaderView(state: state)` would be a sixth file here, so Phase 12, which edits `MainView.swift` anyway for the Garden, makes the swap and deletes `ReaderPlaceholder`; the reader's manual check runs there. Everything in this phase is covered by unit tests and builds on its own.

**Consumes from Phases 6–9** (names as Phases 7 and 9 define them):
- `EntryRow` and `FulltextRow` (Phase 7) with their memberwise initialisers (`EntryRow(entryId:feedId:feedTitle:folderId:title:author:url:publishedAt:isRead:isStarred:hasFulltext:summaryHtml:contentHtml:)`, `FulltextRow(entryId:html:title:byline:error:)`), `EntryRow.displayTitle` ("Untitled" fallback) and `EntryRow.link` (http(s) `URL?`).
- `AppState` (Phase 9): `selectedEntry` (the complete row from `Queries.entry(id:)`; list rows have `contentHtml = nil` and a shortened summary, so the reader never renders a list row), `selectedFulltext`, `readerShowsFullText`, `isExtracting`, `fullTextNotice`, `requestFullText()`, `toggleStar()`, `openInBrowser()`, `canToggleEntry`, `canOpenInBrowser`, and `readerPageDown: (@MainActor () async -> Bool)?`, which `advance()` (Space) calls before moving to the next unread entry.
- Phase 9's View menu writes `@AppStorage("reader.size")` for Bigger Text `⌘+`, Smaller Text `⌘−` and Actual Size `⌘0`: `ReaderStyle.Keys.size` is that key, and this phase adds no menu item.
- The test target is hosted in the app (Phase 6), so `Bundle.main` is `Ressac.app` in tests (resources and, from Phase 11, fonts are found there).

**Design decisions verified on this Mac (Xcode 27, macOS 27 SDK) before writing this phase:**
- The page is loaded with `loadHTMLString(_:baseURL: nil)`: the main-frame navigation is `.other` to `about:blank`, and images load from both `http:` and `https:`. With an `https` base URL, plain-`http` images would be mixed content, so the sanitiser resolves relative addresses itself (the feed's `<base>` is removed anyway).
- `defaultWebpagePreferences.allowsContentJavaScript = false` stops inline scripts, while `callAsyncJavaScript` from the app still runs (used to page down with `Space`).
- Fonts registered by the app (`ATSApplicationFontsPath`, `CTFontManagerRegisterFontsForURL`) are **not** visible to WebKit's content process, and an `@font-face` served by a custom URL scheme did not load either. A `data:` URL does, so the bundled fonts are inlined, built once per process.

### Task 10.1: `ReaderStyle`, fonts and line widths

**Interfaces:**
- Consumes: nothing new.
- Produces (`Settings/ReaderStyle.swift`):
  - `enum ReaderFont: String, CaseIterable, Identifiable, Sendable { case sfPro, newYork, charter, newsreader, atkinson }` with `displayName: String`, `cssFamily: String`, `bundledFaces: [BundledFace]`, `func swiftUIFont(size: Double) -> Font`;
  - `struct BundledFace: Equatable, Sendable { let file: String; let italic: Bool; let weights: ClosedRange<Int> }`;
  - `enum LineWidth: String, CaseIterable, Identifiable, Sendable { case narrow, medium, wide }` with `maxCh` (58, 68, 80) and `displayName`;
  - `struct ReaderStyle: Equatable, Sendable { var font = .sfPro; var size = 17.0; var width = .medium }`, with `defaultSize`, `sizeRange` (`12...28`), `sizeStep` (`1`), `Keys` (`reader.font`, `reader.size`, `reader.width`, the `@AppStorage` keys; `reader.size` is the key Phase 9's menu already writes), `static func clampedSize(_:)`, `bigger(_:)`, `smaller(_:)` and `static func load(from: UserDefaults) -> ReaderStyle`.

- [ ] **Step 1: Write the failing tests.** Create `apps/macos/RessacTests/ReaderTests.swift`:

```swift
import Foundation
import SwiftUI
import Testing
import WebKit
@testable import Ressac

private let articleURL = "https://blog.example.org/2026/10/post.html"

private func entry(id: Int64 = 1, title: String? = "A title", url: String? = articleURL,
                   summary: String? = "<p>Feed summary</p>", content: String? = nil) -> EntryRow {
    EntryRow(entryId: id, feedId: 7, feedTitle: "Example Blog", folderId: nil, title: title, author: nil, url: url,
             publishedAt: Date(timeIntervalSince1970: 1_791_619_200), isRead: false, isStarred: false,
             hasFulltext: false, summaryHtml: summary, contentHtml: content)
}

@Suite struct ReaderStyleTests {
    @Test func defaultsMatchTheSpec() {
        let style = ReaderStyle()
        #expect(style.font == .sfPro)
        #expect(style.size == 17)
        #expect(style.width == .medium)
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80])
        #expect(ReaderFont.allCases.map(\.displayName) == ["SF Pro", "New York", "Charter", "Newsreader", "Atkinson Hyperlegible Next"])
    }

    @Test func textSizeStepsAndClamps() {
        #expect(ReaderStyle.bigger(17) == 18)
        #expect(ReaderStyle.smaller(17) == 16)
        #expect(ReaderStyle.bigger(28) == 28)
        #expect(ReaderStyle.smaller(12) == 12)
        #expect(ReaderStyle.clampedSize(40) == 28)
        #expect(ReaderStyle.clampedSize(3) == 12)
        #expect(ReaderStyle.clampedSize(16.6) == 17)
        #expect(ReaderStyle.clampedSize(.nan) == 17)
    }

    @Test func styleLoadsFromDefaultsAndIgnoresGarbage() throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle())
        defaults.set("newsreader", forKey: ReaderStyle.Keys.font)
        defaults.set(21.0, forKey: ReaderStyle.Keys.size)
        defaults.set("wide", forKey: ReaderStyle.Keys.width)
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle(font: .newsreader, size: 21, width: .wide))
        defaults.set("comic-sans", forKey: ReaderStyle.Keys.font)
        defaults.set(99.0, forKey: ReaderStyle.Keys.size)
        defaults.set("huge", forKey: ReaderStyle.Keys.width)
        #expect(ReaderStyle.load(from: defaults) == ReaderStyle(font: .sfPro, size: 28, width: .medium))
    }

    @Test func cssFamiliesPutTheChosenFaceFirst() {
        #expect(ReaderFont.sfPro.cssFamily.hasPrefix("-apple-system"))
        #expect(ReaderFont.newYork.cssFamily.hasPrefix("ui-serif"))
        #expect(ReaderFont.charter.cssFamily.hasPrefix("Charter"))
        #expect(ReaderFont.newsreader.cssFamily.hasPrefix("\"Newsreader\""))
        #expect(ReaderFont.atkinson.cssFamily.hasPrefix("\"Atkinson Hyperlegible Next\""))
        #expect(ReaderFont.sfPro.bundledFaces.isEmpty)
        #expect(ReaderFont.newsreader.bundledFaces.map(\.italic) == [false, true])
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ReaderStyleTests`. Expected: the build fails with `cannot find 'ReaderStyle' in scope` (and the same for `LineWidth`, `ReaderFont`).

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Settings/ReaderStyle.swift`:

```swift
import SwiftUI

/// Article fonts offered in Settings (spec §7). The interface itself always stays in SF Pro.
enum ReaderFont: String, CaseIterable, Identifiable, Sendable {
    case sfPro, newYork, charter, newsreader, atkinson

    var id: Self { self }

    var displayName: String {
        switch self {
        case .sfPro: "SF Pro"
        case .newYork: "New York"
        case .charter: "Charter"
        case .newsreader: "Newsreader"
        case .atkinson: "Atkinson Hyperlegible Next"
        }
    }

    /// CSS `font-family` for the article page. Bundled fonts come first under the family
    /// name their `@font-face` rules declare (see `ArticleTemplate`).
    var cssFamily: String {
        switch self {
        case .sfPro: #"-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif"#
        case .newYork: #"ui-serif, "New York", Georgia, serif"#
        case .charter: #"Charter, "Charter BT", Georgia, serif"#
        case .newsreader: #""Newsreader", ui-serif, Georgia, serif"#
        case .atkinson: #""Atkinson Hyperlegible Next", -apple-system, sans-serif"#
        }
    }

    /// Font files shipped in `Resources/Fonts` (Phase 11) and embedded in the article page,
    /// because the web content process cannot see fonts registered by the app.
    var bundledFaces: [BundledFace] {
        switch self {
        case .newsreader:
            [BundledFace(file: "Newsreader-Variable", italic: false, weights: 200...800),
             BundledFace(file: "Newsreader-Italic-Variable", italic: true, weights: 200...800)]
        case .atkinson:
            [BundledFace(file: "AtkinsonHyperlegibleNext-Variable", italic: false, weights: 200...800),
             BundledFace(file: "AtkinsonHyperlegibleNext-Italic-Variable", italic: true, weights: 200...800)]
        case .sfPro, .newYork, .charter:
            []
        }
    }

    /// The same face for SwiftUI previews (Settings). Bundled fonts are registered for the app
    /// process through `ATSApplicationFontsPath` (Phase 11).
    func swiftUIFont(size: Double) -> Font {
        switch self {
        case .sfPro: .system(size: size)
        case .newYork: .system(size: size, design: .serif)
        case .charter: .custom("Charter", size: size)
        case .newsreader: .custom("Newsreader", size: size)
        case .atkinson: .custom("Atkinson Hyperlegible Next", size: size)
        }
    }
}

struct BundledFace: Equatable, Sendable {
    /// File name in `Resources/Fonts`, without the `.ttf` extension.
    let file: String
    let italic: Bool
    /// The `wght` axis range of the variable font.
    let weights: ClosedRange<Int>
}

enum LineWidth: String, CaseIterable, Identifiable, Sendable {
    case narrow, medium, wide

    var id: Self { self }

    /// Maximum line length, in `ch` units of the article font.
    var maxCh: Int {
        switch self {
        case .narrow: 58
        case .medium: 68
        case .wide: 80
        }
    }

    var displayName: String {
        switch self {
        case .narrow: "Narrow"
        case .medium: "Medium"
        case .wide: "Wide"
        }
    }
}

/// How articles look. Persisted with `@AppStorage` under `Keys`; Phase 9's View menu
/// (Bigger Text ⌘+, Smaller Text ⌘−, Actual Size ⌘0) writes `Keys.size` directly.
struct ReaderStyle: Equatable, Sendable {
    var font: ReaderFont = .sfPro
    var size: Double = ReaderStyle.defaultSize
    var width: LineWidth = .medium

    static let defaultSize = 17.0
    static let sizeRange = 12.0...28.0
    static let sizeStep = 1.0

    enum Keys {
        static let font = "reader.font"
        static let size = "reader.size"
        static let width = "reader.width"
    }

    static func clampedSize(_ size: Double) -> Double {
        guard size.isFinite else { return defaultSize }
        return min(max(size.rounded(), sizeRange.lowerBound), sizeRange.upperBound)
    }

    static func bigger(_ size: Double) -> Double { clampedSize(size + sizeStep) }
    static func smaller(_ size: Double) -> Double { clampedSize(size - sizeStep) }

    /// Reads the persisted style; unknown or missing values fall back to the defaults.
    static func load(from defaults: UserDefaults) -> ReaderStyle {
        var style = ReaderStyle()
        if let raw = defaults.string(forKey: Keys.font), let font = ReaderFont(rawValue: raw) {
            style.font = font
        }
        if defaults.object(forKey: Keys.size) != nil {
            style.size = clampedSize(defaults.double(forKey: Keys.size))
        }
        if let raw = defaults.string(forKey: Keys.width), let width = LineWidth(rawValue: raw) {
            style.width = width
        }
        return style
    }
}
```

- [ ] **Step 4: Run green.** Same command. Expected: `ReaderStyleTests` passes, 4 tests.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Settings/ReaderStyle.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader style: fonts, text size and line width settings"
```

### Task 10.2: The sanitiser and the link policy

**Interfaces:**
- Consumes: `XMLDocument` (Foundation) with `[.documentTidyHTML, .nodeLoadExternalEntitiesNever]`; `WKNavigationType` (WebKit).
- Produces (`Reading/ArticleTemplate.swift`):
  - `enum ArticleTemplate` with `static func sanitize(_ html: String, baseURL: URL? = nil) -> String`. The master's `sanitize(_:)` call shape still works; `baseURL` resolves relative `href`/`src` because `<base>` is stripped. Also `static let droppedElements`, `static let allowedElements`, `static func safeURL(_:baseURL:allowImageData:) -> String?`, `static func escapeText(_:)`, `static func escapeAttribute(_:)`;
  - `enum LinkPolicy` with `enum Decision: Equatable { case allow, cancel, openExternally(URL) }`, `static func decide(type: WKNavigationType, url: URL?) -> Decision` and `static func webURL(_ string: String?) -> URL?` (http(s) with a host, else `nil`; Phase 12 uses it too).
- Review Focus #3 is pinned here: `sanitizer_strips_active_content`.

Rules: elements in `droppedElements` disappear with their content (`script`, `style`, `iframe`, `object`, `embed`, `form` and its controls, `meta`, `base`, `link`, `svg`, `math`, media…); elements in `allowedElements` are kept with an attribute allowlist (`href` on `a`, `src alt width height` on `img`, table spans, list numbering, `title lang dir` everywhere); any other element is unwrapped (its children stay). `on*`, `style`, `class` and `id` never survive. `href` keeps only absolute `http(s)`; `src` also accepts `data:image/…` except SVG; an `img` left without `src` is dropped. Comments and processing instructions are dropped. Text and attribute values are re-escaped by our own serialiser (XML serialisation would emit `<a/>` and `<br></br>`, which HTML parses wrongly). If tidy cannot parse the input at all, the whole input is escaped as text.

- [ ] **Step 1: Write the failing tests.** Append to `ReaderTests.swift`:

```swift
@Suite struct SanitizerTests {
    /// Review Focus #3: hostile feed HTML must be inert in the reader.
    @Test("sanitizer_strips_active_content") func sanitizerStripsActiveContent() {
        let hostile = """
        <meta http-equiv="refresh" content="0;url=https://evil.example/">
        <base href="https://evil.example/">
        <p onclick="steal()" style="color:red">Hello <b onmouseover="x()">world</b></p>
        <script>alert(1)</script><iframe src="https://evil.example/frame"></iframe>
        <object data="x.swf"></object><embed src="x.swf">
        <form action="https://evil.example/"><input name="q"><button>Go</button></form>
        <a href="javascript:alert(1)">bad link</a> <a href=" JaVaScRiPt:alert(2)">bad two</a>
        <img src="javascript:alert(3)"><img src="https://img.example.org/a.png" onerror="x()">
        <svg><script>alert(4)</script></svg><style>body { display: none }</style>
        """
        let clean = ArticleTemplate.sanitize(hostile, baseURL: URL(string: articleURL))
        for needle in ["<script", "<iframe", "<object", "<embed", "<form", "<input", "<button", "<meta", "<base",
                       "<style", "<svg", "http-equiv", "onclick", "onmouseover", "onerror", "style=",
                       "javascript:", "JaVaScRiPt", "evil.example", "alert("] {
            #expect(!clean.contains(needle), "still contains \(needle)")
        }
        #expect(clean.contains("<p>Hello <b>world</b>"))
        #expect(clean.contains("<a>bad link</a>"))
        #expect(clean.contains(#"<img src="https://img.example.org/a.png">"#))
    }

    @Test func keepsTextEntitiesAndStructure() {
        let clean = ArticleTemplate.sanitize("<h2>Part</h2><p>caf\u{e9} &eacute; &amp; <em>it</em></p><ul><li>one</li></ul><pre>  a\n    b</pre>")
        #expect(clean.contains("<h2>Part"))
        #expect(clean.contains("<p>café é &amp; <em>it</em>"))
        #expect(clean.contains("<li>one</li>"))
        #expect(clean.contains("<pre>  a\n    b"))
    }

    @Test func resolvesRelativeAddressesAgainstTheEntry() {
        let html = ##"<a href="/post/2">rel</a><a href="#notes">frag</a><img src="img/a.png"><a href="mailto:a@b.example">mail</a>"##
        let clean = ArticleTemplate.sanitize(html, baseURL: URL(string: articleURL))
        #expect(clean.contains(#"<a href="https://blog.example.org/post/2">rel</a>"#))
        #expect(clean.contains(#"<a href="https://blog.example.org/2026/10/post.html#notes">frag</a>"#))
        #expect(clean.contains(#"<img src="https://blog.example.org/2026/10/img/a.png">"#))
        #expect(clean.contains("<a>mail</a>"))
        let noBase = ArticleTemplate.sanitize(#"<a href="/post/2">rel</a><img src="/a.png">"#, baseURL: nil)
        #expect(noBase == "<a>rel</a>")
    }

    @Test func unwrapsUnknownElementsAndFiltersImages() {
        let clean = ArticleTemplate.sanitize(#"<font color="red">kept text</font><img alt="no source"><img src="data:image/png;base64,AAAA"><img src="data:image/svg+xml;base64,AAAA">"#)
        #expect(clean.contains("kept text"))
        #expect(!clean.contains("<font"))
        #expect(!clean.contains("no source"))
        #expect(clean.contains(#"<img src="data:image/png;base64,AAAA">"#))
        #expect(!clean.contains("svg"))
    }

    @Test func escapesTextAndAttributes() {
        let clean = ArticleTemplate.sanitize(#"<p title='say "hi"'>1 &lt; 2 &amp;&amp; 3 &gt; 2</p>"#)
        #expect(clean.contains(#"<p title="say &quot;hi&quot;">1 &lt; 2 &amp;&amp; 3 &gt; 2"#))
    }

    @Test func survivesGarbage() {
        #expect(ArticleTemplate.sanitize("") == "")
        #expect(ArticleTemplate.sanitize("< not a tag > 3 < 4").contains("&lt; not a tag &gt; 3 &lt; 4"))
        #expect(!ArticleTemplate.sanitize("<p>a\u{0}b<script>x</script>").contains("<script"))
        #expect(ArticleTemplate.sanitize("plain & text") == "plain &amp; text")
    }
}

@Suite struct LinkPolicyTests {
    @Test func onlyOurOwnPageLoadsAndClickedWebLinksGoToTheBrowser() {
        let web = URL(string: "https://example.org/a")!
        #expect(LinkPolicy.decide(type: .other, url: URL(string: "about:blank")) == .allow)
        #expect(LinkPolicy.decide(type: .linkActivated, url: web) == .openExternally(web))
        #expect(LinkPolicy.decide(type: .linkActivated, url: URL(string: "javascript:alert(1)")) == .cancel)
        #expect(LinkPolicy.decide(type: .linkActivated, url: URL(string: "file:///etc/passwd")) == .cancel)
        #expect(LinkPolicy.decide(type: .other, url: web) == .cancel)
        #expect(LinkPolicy.decide(type: .formSubmitted, url: web) == .cancel)
        #expect(LinkPolicy.decide(type: .reload, url: URL(string: "about:blank")) == .cancel)
        #expect(LinkPolicy.webURL("mailto:a@b.example") == nil)
        #expect(LinkPolicy.webURL("https:///nohost") == nil)
        #expect(LinkPolicy.webURL(nil) == nil)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests`. Expected: build failure, `type 'ArticleTemplate' has no member 'sanitize'` / `cannot find 'ArticleTemplate' in scope`, and `cannot find 'LinkPolicy' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Reading/ArticleTemplate.swift`:

```swift
import Foundation
import SwiftUI
import WebKit

/// Builds the reader's HTML page. Feed and extracted HTML are untrusted: `sanitize` keeps an
/// allowlist of inert elements and attributes, the page forbids scripts, frames, objects and
/// forms through its Content-Security-Policy, and the web view runs with JavaScript off.
enum ArticleTemplate {
    // MARK: Sanitiser

    /// Removed together with everything inside them.
    static let droppedElements: Set<String> = [
        "applet", "audio", "base", "button", "canvas", "dialog", "embed", "form", "frame", "frameset",
        "head", "iframe", "input", "link", "math", "meta", "noscript", "object", "option", "script",
        "select", "source", "style", "svg", "template", "textarea", "title", "track", "video",
    ]

    /// Kept, with their allowed attributes. Any other element is unwrapped: its children stay.
    static let allowedElements: Set<String> = [
        "a", "abbr", "article", "aside", "b", "bdi", "bdo", "blockquote", "br", "caption", "cite", "code",
        "col", "colgroup", "dd", "del", "details", "dfn", "div", "dl", "dt", "em", "figcaption", "figure",
        "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "i", "img", "ins", "kbd", "li", "main",
        "mark", "nav", "ol", "p", "pre", "q", "rp", "rt", "ruby", "s", "samp", "section", "small", "span",
        "strong", "sub", "summary", "sup", "table", "tbody", "td", "tfoot", "th", "thead", "time", "tr",
        "u", "ul", "var", "wbr",
    ]

    private static let voidElements: Set<String> = ["br", "col", "hr", "img", "wbr"]
    private static let globalAttributes: Set<String> = ["dir", "lang", "title"]
    private static let elementAttributes: [String: Set<String>] = [
        "a": ["href"],
        "col": ["span"],
        "colgroup": ["span"],
        "details": ["open"],
        "img": ["alt", "height", "src", "width"],
        "li": ["value"],
        "ol": ["reversed", "start", "type"],
        "td": ["colspan", "rowspan"],
        "th": ["colspan", "rowspan", "scope"],
        "time": ["datetime"],
    ]

    /// Returns inert HTML. Relative `href`/`src` are resolved against `baseURL` (the entry's
    /// address), since `<base>` is removed; links and images keep only http(s) addresses
    /// (images may also be `data:image/…`, except SVG).
    static func sanitize(_ html: String, baseURL: URL? = nil) -> String {
        let wrapped = "<html><body>\(html)</body></html>"
        guard let document = try? XMLDocument(xmlString: wrapped, options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever]),
              let body = document.rootElement()?.children?.first(where: { name(of: $0) == "body" })
        else {
            return "<p>\(escapeText(html))</p>"
        }
        var out = ""
        for child in body.children ?? [] {
            render(child, baseURL: baseURL, into: &out)
        }
        return out
    }

    private static func name(of node: XMLNode) -> String {
        (node.localName ?? node.name ?? "").lowercased()
    }

    private static func render(_ node: XMLNode, baseURL: URL?, into out: inout String) {
        switch node.kind {
        case .text:
            out += escapeText(node.stringValue ?? "")
        case .element:
            guard let element = node as? XMLElement else { return }
            let tag = name(of: element)
            if droppedElements.contains(tag) { return }
            guard allowedElements.contains(tag) else {
                for child in element.children ?? [] { render(child, baseURL: baseURL, into: &out) }
                return
            }
            let attributes = safeAttributes(of: element, tag: tag, baseURL: baseURL)
            if tag == "img" && !attributes.contains(where: { $0.name == "src" }) { return }
            out += "<\(tag)"
            for attribute in attributes {
                out += " \(attribute.name)=\"\(escapeAttribute(attribute.value))\""
            }
            out += ">"
            guard !voidElements.contains(tag) else { return }
            for child in element.children ?? [] { render(child, baseURL: baseURL, into: &out) }
            out += "</\(tag)>"
        default:
            // Comments, processing instructions, DTD nodes: dropped.
            return
        }
    }

    private static func safeAttributes(of element: XMLElement, tag: String, baseURL: URL?) -> [(name: String, value: String)] {
        let allowed = globalAttributes.union(elementAttributes[tag] ?? [])
        return (element.attributes ?? []).compactMap { attribute in
            let key = name(of: attribute)
            guard allowed.contains(key), let value = attribute.stringValue else { return nil }
            switch key {
            case "href": return safeURL(value, baseURL: baseURL, allowImageData: false).map { (key, $0) }
            case "src": return safeURL(value, baseURL: baseURL, allowImageData: true).map { (key, $0) }
            default: return (key, value)
            }
        }
    }

    static func safeURL(_ raw: String, baseURL: URL?, allowImageData: Bool) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines.union(.controlCharacters))
        let lowered = trimmed.lowercased()
        if allowImageData && lowered.hasPrefix("data:image/") && !lowered.hasPrefix("data:image/svg") {
            return trimmed
        }
        guard let url = URL(string: trimmed, relativeTo: baseURL)?.absoluteURL,
              let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https"
        else { return nil }
        return url.absoluteString
    }

    static func escapeText(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    static func escapeAttribute(_ text: String) -> String {
        escapeText(text).replacingOccurrences(of: "\"", with: "&quot;")
    }
}

/// Which navigations the reader allows. Only the page we load ourselves (about:blank, since
/// it is loaded without a base URL) is shown; a clicked http(s) link opens in the default
/// browser; everything else (redirects, form posts, other schemes) is cancelled.
enum LinkPolicy {
    enum Decision: Equatable {
        case allow
        case cancel
        case openExternally(URL)
    }

    static func decide(type: WKNavigationType, url: URL?) -> Decision {
        switch type {
        case .other where url?.absoluteString == "about:blank":
            return .allow
        case .linkActivated:
            return webURL(url?.absoluteString).map(Decision.openExternally) ?? .cancel
        default:
            return .cancel
        }
    }

    /// Feed content is untrusted: only http(s) addresses may reach the system opener.
    static func webURL(_ string: String?) -> URL? {
        guard let string, let url = URL(string: string), let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https", url.host() != nil
        else { return nil }
        return url
    }
}
```

- [ ] **Step 4: Run green.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/SanitizerTests -only-testing:RessacTests/LinkPolicyTests`. Expected: 7 tests pass, among them `"sanitizer_strips_active_content"`.

- [ ] **Step 5: Commit.**

```bash
git add apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader: allowlist sanitiser for feed HTML and the link policy"
```

### Task 10.3: `reader.css` and the article page

**Interfaces:**
- Consumes: `EntryRow`, `FulltextRow`, `ReaderStyle`, `LinkPolicy.webURL`, `ColorScheme`.
- Produces (in `enum ArticleTemplate`):
  - `static func page(for entry: EntryRow, fulltext: FulltextRow?, showFull: Bool, style: ReaderStyle, colorScheme: ColorScheme) -> String` (master signature);
  - `static let contentSecurityPolicy: String`, `static let readerCSS: String` (the bundled `reader.css`), `static func fontFaces(for: ReaderFont) -> String` (`@font-face` rules with `data:` URLs, empty for system fonts or when the files are not bundled yet);
  - `Resources/reader.css`, whose `:root[data-scheme="light"]` and `:root[data-scheme="dark"]` blocks hold two separately tuned palettes; `--reader-font`, `--reader-size` and `--reader-measure` come from the page.

Content rules (spec §7): the full article when `showFull` is on and a cached `html` exists **for this entry**; otherwise the first non-blank of `content_html`, `summary_html`. A cached error with `showFull` on adds the one-line notice "Full article unavailable: <error>" above the feed content. The header shows feed title, date and, for the full article, a "full article" badge, then the title (a link to the article when the entry has an http(s) URL, else plain text), then the byline (extracted byline for the full article, else the entry's author). Everything from the feed is escaped or sanitised.

- [ ] **Step 1: Write the failing tests.** Append to `ReaderTests.swift`:

```swift
@Suite struct ArticlePageTests {
    private let full = FulltextRow(entryId: 1, html: "<p>Full body</p>", title: "Full title", byline: "Ada Lovelace", error: nil)

    @Test func showsTheFullArticleOnlyWhenToggledAndCached() {
        let feed = entry(content: "<p>Feed body</p>")
        let on = ArticleTemplate.page(for: feed, fulltext: full, showFull: true, style: ReaderStyle(), colorScheme: .light)
        #expect(on.contains("Full body"))
        #expect(!on.contains("Feed body"))
        #expect(on.contains(#"<span class="badge">full article</span>"#))
        #expect(on.contains("Ada Lovelace"))
        let off = ArticleTemplate.page(for: feed, fulltext: full, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(off.contains("Feed body"))
        #expect(!off.contains("Full body"))
        #expect(!off.contains("badge"))
        let otherEntry = ArticleTemplate.page(for: entry(id: 2, content: "<p>Feed body</p>"), fulltext: full, showFull: true, style: ReaderStyle(), colorScheme: .light)
        #expect(!otherEntry.contains("Full body"))
    }

    @Test func fallsBackFromContentToSummary() {
        let page = ArticleTemplate.page(for: entry(summary: "<p>Feed summary</p>", content: "  "), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains("Feed summary"))
    }

    @Test func failedExtractionKeepsTheFeedContentWithANotice() {
        let failed = FulltextRow(entryId: 1, html: nil, title: nil, byline: nil, error: "HTTP 404.")
        let page = ArticleTemplate.page(for: entry(content: "<p>Feed body</p>"), fulltext: failed, showFull: true, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains(#"<p class="notice">Full article unavailable: HTTP 404.</p>"#))
        #expect(page.contains("Feed body"))
    }

    /// Review Focus #1, rendering side: no url, no title, only summary_html.
    @Test func untitledEntryWithoutLinkRenders() {
        let page = ArticleTemplate.page(for: entry(title: nil, url: nil, summary: "<p>Only a summary</p>"), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains(#"<h1 class="title">Untitled</h1>"#))
        #expect(page.contains("Only a summary"))
    }

    @Test func emptyEntrySaysSo() {
        let page = ArticleTemplate.page(for: entry(summary: nil, content: nil), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains("This entry has no text. Open it in the browser to read it."))
    }

    @Test func escapesEntryMetadata() {
        let page = ArticleTemplate.page(for: entry(title: "<script>x()</script> & co"), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains("&lt;script&gt;x()&lt;/script&gt; &amp; co"))
        #expect(!page.contains("<script>"))
    }

    @Test func appliesStyleSchemeAndPolicy() {
        let style = ReaderStyle(font: .charter, size: 20, width: .wide)
        let page = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: style, colorScheme: .dark)
        #expect(page.contains(#"data-scheme="dark""#))
        #expect(page.contains("--reader-size: 20px"))
        #expect(page.contains("--reader-measure: 80ch"))
        #expect(page.contains("--reader-font: Charter"))
        #expect(page.contains("default-src 'none'"))
        #expect(!page.contains("@font-face"))
        #expect(page.contains(#"<a href="https://blog.example.org/2026/10/post.html">A title</a>"#))
        let light = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: style, colorScheme: .light)
        #expect(light.contains(#"data-scheme="light""#))
    }

    @Test func embedsTheBundledStylesheet() {
        #expect(ArticleTemplate.readerCSS.contains(#":root[data-scheme="dark"]"#))
        let page = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        #expect(page.contains("--paper"))
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ArticlePageTests`. Expected: build failure, `type 'ArticleTemplate' has no member 'page'` and `… 'readerCSS'`.

- [ ] **Step 3: Implement the stylesheet.** Create `apps/macos/Ressac/Resources/reader.css`. Phase 6's `Ressac` source group picks it up as a bundle resource (XcodeGen classifies `.css` as a resource; verified with XcodeGen 2.46.0 on a scratch project).

```css
/*
 * Ressac reader stylesheet. ArticleTemplate inlines it and sets, on :root,
 * --reader-font, --reader-size and --reader-measure (Settings), and
 * data-scheme="light|dark" (the window's appearance).
 *
 * The two palettes are tuned separately, not inverted: dark mode spreads its
 * greys further apart, softens the text to avoid glare, brightens links and
 * dims images slightly.
 */

:root {
  --ui-font: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif;
  --mono-font: ui-monospace, "SF Mono", Menlo, monospace;
  --reader-font: var(--ui-font);
  --reader-size: 17px;
  --reader-measure: 68ch;
}

:root[data-scheme="light"] {
  color-scheme: light;
  --paper: #FFFFFF;
  --ink: #1D1D1F;
  --ink-secondary: #6E6E73;
  --ink-tertiary: #8E8E93;
  --link: #0066CC;
  --link-underline: rgba(0, 102, 204, 0.35);
  --rule: rgba(0, 0, 0, 0.10);
  --code-bg: #F5F5F7;
  --quote-bar: #D2D2D7;
  --mark: #FFF1A8;
  --badge-bg: rgba(0, 0, 0, 0.05);
  --notice-bg: #FFF4E5;
  --notice-ink: #8A4B00;
  --selection: rgba(0, 122, 255, 0.18);
  --image-filter: none;
  --leading: 1.55;
  --smoothing: auto;
}

:root[data-scheme="dark"] {
  color-scheme: dark;
  --paper: #1C1C1E;
  --ink: #E3E3E8;
  --ink-secondary: #A1A1A6;
  --ink-tertiary: #7C7C80;
  --link: #6CB4FF;
  --link-underline: rgba(108, 180, 255, 0.40);
  --rule: rgba(255, 255, 255, 0.14);
  --code-bg: #2C2C2E;
  --quote-bar: #48484A;
  --mark: #5E4B00;
  --badge-bg: rgba(255, 255, 255, 0.09);
  --notice-bg: #3A2A12;
  --notice-ink: #FFC56B;
  --selection: rgba(10, 132, 255, 0.38);
  --image-filter: brightness(0.9) contrast(1.05);
  --leading: 1.6;
  --smoothing: antialiased;
}

html {
  background: var(--paper);
}

body {
  margin: 0;
  padding: 28px 40px 96px;
  background: var(--paper);
  color: var(--ink);
  font-family: var(--reader-font);
  font-size: var(--reader-size);
  line-height: var(--leading);
  -webkit-font-smoothing: var(--smoothing);
  -webkit-text-size-adjust: none;
  text-rendering: optimizeLegibility;
  font-kerning: normal;
  font-variant-ligatures: common-ligatures;
  hanging-punctuation: first;
  overflow-wrap: break-word;
}

::selection {
  background: var(--selection);
}

article {
  max-width: var(--reader-measure);
  margin: 0 auto;
}

/* Header: meta line and byline stay in the interface font. */

.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 6px;
  margin: 0 0 8px;
  font-family: var(--ui-font);
  font-size: 12px;
  line-height: 1.4;
  color: var(--ink-secondary);
}

.meta .dot {
  color: var(--ink-tertiary);
}

.meta .badge {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--badge-bg);
  font-size: 11px;
}

h1.title {
  margin: 0 0 6px;
  font-size: 1.62em;
  line-height: 1.2;
  font-weight: 700;
  letter-spacing: -0.015em;
}

h1.title a {
  color: inherit;
  text-decoration: none;
}

.byline {
  margin: 0 0 4px;
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ink-secondary);
}

.notice {
  margin: 14px 0 0;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--notice-bg);
  color: var(--notice-ink);
  font-family: var(--ui-font);
  font-size: 12px;
  line-height: 1.4;
}

.content {
  margin-top: 22px;
}

.content > :first-child {
  margin-top: 0;
}

.empty {
  color: var(--ink-tertiary);
  font-style: italic;
}

/* Body text. */

p, ul, ol, dl, pre, blockquote, table {
  margin: 0 0 1em;
}

h2, h3, h4, h5, h6 {
  margin: 1.6em 0 0.5em;
  line-height: 1.25;
  font-weight: 650;
  letter-spacing: -0.01em;
}

.content h1 { font-size: 1.35em; margin: 1.6em 0 0.5em; line-height: 1.25; }
h2 { font-size: 1.25em; }
h3 { font-size: 1.1em; }
h4, h5, h6 { font-size: 1em; }

a {
  color: var(--link);
  text-decoration-line: underline;
  text-decoration-color: var(--link-underline);
  text-decoration-thickness: 0.06em;
  text-underline-offset: 0.18em;
}

a:hover {
  text-decoration-color: currentColor;
}

ul, ol {
  padding-left: 1.4em;
}

li + li {
  margin-top: 0.25em;
}

img {
  display: block;
  max-width: 100%;
  height: auto;
  margin: 1.2em auto;
  border-radius: 6px;
  filter: var(--image-filter);
}

p img, a img, li img {
  display: inline-block;
  margin: 0;
  vertical-align: middle;
}

blockquote {
  margin-left: 0;
  padding-left: 1em;
  border-left: 3px solid var(--quote-bar);
  color: var(--ink-secondary);
}

code, kbd, samp, pre {
  font-family: var(--mono-font);
  font-size: 0.86em;
}

code {
  padding: 0.1em 0.3em;
  border-radius: 4px;
  background: var(--code-bg);
}

pre {
  padding: 12px 14px;
  border-radius: 8px;
  background: var(--code-bg);
  line-height: 1.45;
  overflow-x: auto;
}

pre code {
  padding: 0;
  background: none;
  font-size: 1em;
}

hr {
  margin: 2em 0;
  border: 0;
  border-top: 1px solid var(--rule);
}

table {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
  font-size: 0.9em;
}

th, td {
  padding: 6px 10px;
  border-bottom: 1px solid var(--rule);
  text-align: left;
  vertical-align: top;
}

mark {
  background: var(--mark);
  color: inherit;
}

small, sub, sup {
  font-size: 0.8em;
}

@media (max-width: 560px) {
  body {
    padding: 20px 20px 80px;
  }
}
```

- [ ] **Step 4: Implement the page.** In `ArticleTemplate.swift`, insert below the `escapeAttribute` function, before the closing brace of `enum ArticleTemplate`:

```swift
    // MARK: Page

    /// No scripts, frames, objects, forms or base URL; images from the web; fonts and styles inline.
    static let contentSecurityPolicy =
        "default-src 'none'; img-src http: https: data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'"

    static func page(for entry: EntryRow, fulltext: FulltextRow?, showFull: Bool, style: ReaderStyle, colorScheme: ColorScheme) -> String {
        let base = LinkPolicy.webURL(entry.url)
        let full = showFull ? fulltext.flatMap { $0.entryId == entry.entryId ? $0 : nil } : nil
        let fullHTML = full?.html
        let source = fullHTML ?? [entry.contentHtml, entry.summaryHtml]
            .compactMap { $0 }
            .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? ""
        let body = sanitize(source, baseURL: base)

        var meta = [escapeText(entry.feedTitle), escapeText(entry.publishedAt.formatted(date: .long, time: .shortened))]
        if fullHTML != nil { meta.append(#"<span class="badge">full article</span>"#) }

        let title = escapeText(entry.displayTitle)
        let heading = base.map { #"<a href="\#(escapeAttribute($0.absoluteString))">\#(title)</a>"# } ?? title
        let byline = (fullHTML != nil ? full?.byline : nil) ?? entry.author
        let notice = showFull ? full?.error.map { "Full article unavailable: \($0)" } : nil
        let emptyText = base == nil ? "This entry has no text." : "This entry has no text. Open it in the browser to read it."

        return """
        <!doctype html>
        <html lang="en" data-scheme="\(colorScheme == .dark ? "dark" : "light")">
        <head>
        <meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(contentSecurityPolicy)">
        <meta name="referrer" content="no-referrer">
        <style>
        \(fontFaces(for: style.font))
        \(readerCSS)
        :root { --reader-font: \(style.font.cssFamily); --reader-size: \(Int(ReaderStyle.clampedSize(style.size)))px; --reader-measure: \(style.width.maxCh)ch; }
        </style>
        </head>
        <body>
        <article>
        <p class="meta">\(meta.joined(separator: #"<span class="dot">·</span>"#))</p>
        <h1 class="title">\(heading)</h1>
        \(byline.map { #"<p class="byline">\#(escapeText($0))</p>"# } ?? "")
        \(notice.map { #"<p class="notice">\#(escapeText($0))</p>"# } ?? "")
        <div class="content">\(body.isEmpty ? #"<p class="empty">\#(emptyText)</p>"# : body)</div>
        </article>
        </body>
        </html>
        """
    }

    // MARK: Resources

    /// `reader.css` from the app bundle (unit tests run hosted in the app, so `Bundle.main` is it).
    static let readerCSS: String = Bundle.main.url(forResource: "reader", withExtension: "css")
        .flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? ""

    private static let newsreaderFaces = makeFontFaces(for: .newsreader)
    private static let atkinsonFaces = makeFontFaces(for: .atkinson)

    static func fontFaces(for font: ReaderFont) -> String {
        switch font {
        case .newsreader: newsreaderFaces
        case .atkinson: atkinsonFaces
        case .sfPro, .newYork, .charter: ""
        }
    }

    /// `@font-face` rules with the font files inlined as `data:` URLs (about 1.3 MB of base64 for
    /// Newsreader), built once per process. Missing files yield no rule and the CSS fallback applies.
    private static func makeFontFaces(for font: ReaderFont) -> String {
        let family = font.displayName
        return font.bundledFaces.compactMap { face -> String? in
            guard let url = Bundle.main.url(forResource: face.file, withExtension: "ttf", subdirectory: "Fonts"),
                  let data = try? Data(contentsOf: url)
            else { return nil }
            return """
            @font-face { font-family: "\(family)"; font-style: \(face.italic ? "italic" : "normal"); \
            font-weight: \(face.weights.lowerBound) \(face.weights.upperBound); \
            src: url("data:font/ttf;base64,\(data.base64EncodedString())") format("truetype"); }
            """
        }.joined(separator: "\n")
    }
```

- [ ] **Step 5: Run green.** Same command as Step 2. Expected: `ArticlePageTests` passes, 8 tests. If only `embedsTheBundledStylesheet` fails, `reader.css` is not in `Ressac.app/Contents/Resources/`: check the target's Copy Bundle Resources phase in the generated project.

- [ ] **Step 6: Commit.**

```bash
git add apps/macos/Ressac/Resources/reader.css apps/macos/Ressac/Reading/ArticleTemplate.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader: article page template and reader.css with light and dark palettes"
```

### Task 10.4: The reader view

**Interfaces:**
- Consumes: `AppState` (`selectedEntry`, `selectedFulltext`, `readerShowsFullText`, `isExtracting`, `fullTextNotice`, `requestFullText()`, `toggleStar()`, `openInBrowser()`, `canToggleEntry`, `canOpenInBrowser`, `readerPageDown`), `ArticleTemplate.page`, `LinkPolicy`, `ReaderStyle` keys.
- Produces (`Reading/ReaderView.swift`):
  - `struct ReaderView: View { @Bindable var state: AppState }` (same calling convention as Phase 9's `EntryListView(state:)`): the web view of `state.selectedEntry`, a "No Entry Selected" empty state, and the reader toolbar (Full Article toggle, a spinner while `isExtracting`, Star, Open in Browser, Share). A stored extraction error is shown inside the page ("Full article unavailable: …"); `fullTextNotice` is shown above the page only when the run itself failed and no error row exists, so the notice never appears twice;
  - `struct ArticlePage: Equatable` with `func html() -> String`. Its equality ignores `isRead`, `isStarred` and `hasFulltext`, so marking the entry read does not reload the page and lose the scroll position;
  - `struct ArticleWebView: NSViewRepresentable` (JavaScript off, non-persistent website data, no back/forward gestures, no magnification, dark under-page colour matching `--paper`; navigation delegate applying `LinkPolicy`);
  - `@MainActor final class ReaderController` with `func pageDown() async -> Bool`.

The toolbar items carry no keyboard shortcuts: `⌘↩`, `s` and Article ▸ Full Article belong to Phase 9's menus, so the help tags only name them. The Full Article toggle is disabled for entries without an http(s) link (Review Focus #1: `extract` would exit `2`) and while an extraction runs.

- [ ] **Step 1: Write the failing test.** Append to `ReaderTests.swift`:

```swift
@Suite struct ReaderViewTests {
    @Test func pageIdentityIgnoresReadAndStarFlags() {
        var read = entry()
        read.isRead = true
        read.isStarred = true
        let a = ArticlePage(entry: entry(), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        let b = ArticlePage(entry: read, fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .light)
        let c = ArticlePage(entry: entry(), fulltext: nil, showFull: true, style: ReaderStyle(), colorScheme: .light)
        #expect(a == b)
        #expect(a != c)
    }
}
```

- [ ] **Step 2: Run red.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ReaderViewTests`. Expected: build failure, `cannot find 'ArticlePage' in scope`.

- [ ] **Step 3: Implement.** Create `apps/macos/Ressac/Reading/ReaderView.swift`:

```swift
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
```

- [ ] **Step 4: Run green.** `cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test -only-testing:RessacTests/ReaderViewTests`. Expected: 1 test passes. Then run the whole suite (the verification gate below).

- [ ] **Step 5: Verification gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: all green, zero warnings in our sources.

- [ ] **Step 6: Commit and push.**

```bash
git add apps/macos/Ressac/Reading/ReaderView.swift apps/macos/RessacTests/ReaderTests.swift
git commit -m "Reader view: WKWebView with JavaScript off, links to the browser, full-article toggle"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

## Notes for the executor

- **Verified ahead of time.** Every Swift file of this phase was type-checked with Swift 6.4 (Swift 6 language mode, complete strict concurrency, warnings as errors) in a SwiftPM harness, against stand-ins for the Phase 7–9 types with the shapes of their phase files (`AppState` members, `EntryRow.link`), and the tests ran green there, except `embedsTheBundledStylesheet` (the harness has no app bundle). The `decidePolicyFor` signature with `@escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void` compiles in Swift 6 mode.
- **`NSXMLDocument`'s tidy is HTML 4.** HTML5 elements (`figure`, `figcaption`, `section`, `article`, `header`, `time`, `mark`, `details`, `picture`) are unwrapped by tidy itself before our allowlist runs: their content is kept, their semantics are lost (a `figcaption` becomes text after its image). Accepted for v1. Tidy also inserts a newline before the closing tag of block elements (`<p>x\n</p>`), which is why the tests match prefixes such as `<p>Hello <b>world</b>`.
- **`svg` is dropped by tidy**, but a `<script>` inside it is lifted out as a sibling, which our `droppedElements` then removes; `sanitizer_strips_active_content` covers that path.
- **The CSP meta tag** is defence in depth: JavaScript is already off and the sanitiser already removed active elements.
- **Size of a page with Newsreader:** about 1.3 MB of base64 font data per load (regular plus italic), rendered without visible delay in the WebKit experiment; `ArticlePage` equality avoids reloading for the optimistic read/star updates Phase 9 applies to `selectedEntry`.
- **Do not add `.keyboardShortcut` to the reader toolbar buttons:** the same shortcuts live in Phase 9's menus, and of two items sharing a key AppKit only ever tries the first (Phase 9's notes).
- **`ReaderView` is not on screen until Phase 12** swaps it into `MainView.swift`; nothing else references it before then, which Swift does not warn about.
- **`EntryRow` initialiser:** if Phase 7 gave `EntryRow` an explicit `init` or a different property order, adapt only the `entry(…)` helper at the top of `ReaderTests.swift`.
