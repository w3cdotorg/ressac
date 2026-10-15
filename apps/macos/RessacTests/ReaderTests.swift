import Foundation
import SwiftUI
import Synchronization
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
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80, 40])
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
        #expect(clean.contains(#"<img src="https://img.example.org/a.png" loading="lazy">"#))
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
        #expect(clean.contains(#"<img src="https://blog.example.org/2026/10/img/a.png" loading="lazy">"#))
        #expect(clean.contains("<a>mail</a>"))
        let noBase = ArticleTemplate.sanitize(#"<a href="/post/2">rel</a><img src="/a.png">"#, baseURL: nil)
        #expect(noBase == "<a>rel</a>")
    }

    @Test func unwrapsUnknownElementsAndFiltersImages() {
        let clean = ArticleTemplate.sanitize(#"<font color="red">kept text</font><img alt="no source"><img src="data:image/png;base64,AAAA"><img src="data:image/svg+xml;base64,AAAA">"#)
        #expect(clean.contains("kept text"))
        #expect(!clean.contains("<font"))
        #expect(!clean.contains("no source"))
        #expect(clean.contains(#"<img src="data:image/png;base64,AAAA" loading="lazy">"#))
        #expect(!clean.contains("svg"))
    }

    @Test func escapesTextAndAttributes() {
        let clean = ArticleTemplate.sanitize(#"<p title='say "hi"'>1 &lt; 2 &amp;&amp; 3 &gt; 2</p>"#)
        #expect(clean.contains(#"<p title="say &quot;hi&quot;">1 &lt; 2 &amp;&amp; 3 &gt; 2"#))
    }

    @Test func deepNestingDoesNotOverflowTheStack() {
        let deep = String(repeating: "<div>", count: 100_000) + "deep text" + String(repeating: "</div>", count: 100_000)
        let clean = ArticleTemplate.sanitize(deep)
        #expect(clean.contains("deep text"))
        #expect(!clean.contains("<div"))
        // 300 levels: over the cap, so the plain-text path (no elements survive).
        let medium = String(repeating: "<blockquote>", count: 300) + "mid text" + String(repeating: "</blockquote>", count: 300)
        #expect(ArticleTemplate.estimatedNestingDepth(medium) == 300)
        let plain = ArticleTemplate.sanitize(medium)
        #expect(plain == "<p>mid text</p>")
        // 200 levels: still parsed normally.
        let ok = String(repeating: "<blockquote>", count: 200) + "ok text" + String(repeating: "</blockquote>", count: 200)
        #expect(ArticleTemplate.sanitize(ok).contains("<blockquote>"))
        // Void tags and comments do not count as depth.
        #expect(ArticleTemplate.estimatedNestingDepth("<p>a<br>b<img src=\"x>\"><!-- <div><div> --></p>") == 1)
        // Bypass shapes: stray close tags, and a quote inside an unquoted attribute value.
        let stray = String(repeating: "<div></p>", count: 40_000) + "stray text"
        #expect(ArticleTemplate.estimatedNestingDepth(stray) > ArticleTemplate.maxNestingDepth)
        #expect(ArticleTemplate.sanitize(stray).contains("stray text"))
        let quote = "<p x=a'b>" + String(repeating: "<div>", count: 60_000) + "quote text'>"
        #expect(ArticleTemplate.estimatedNestingDepth(quote) > ArticleTemplate.maxNestingDepth)
        #expect(ArticleTemplate.sanitize(quote).contains("quote text"))
        // A normal article with unclosed paragraphs stays on the parsed path.
        let loose = (0..<500).map { "<p>para \($0)" }.joined() + "<ul><li>a<li>b<li>c</ul>"
        #expect(ArticleTemplate.estimatedNestingDepth(loose) < 10)
        #expect(ArticleTemplate.sanitize(loose).contains("<ul>"))
        let paragraphs = ArticleTemplate.plainTextFallback("<p>one &amp; two</p>\n\n<p>three &lt;x&gt;</p>")
        #expect(paragraphs == "<p>one &amp; two</p>\n<p>three &lt;x&gt;</p>")
    }

    @Test func misnestedEndTagsAreBoundedByTheSizeCapAndBudget() {
        let big = String(repeating: "<b><div></b>", count: 100_000)
        let start = Date()
        let clean = ArticleTemplate.sanitize(big)
        #expect(Date().timeIntervalSince(start) < 1.5)
        #expect(!clean.contains("<div"))
        // 200 KB of the same shape: the depth estimate sends it to the plain-text path, unparsed.
        let medium = String(repeating: "<b><div>x</b>", count: 15_000)
        let mediumStart = Date()
        _ = ArticleTemplate.sanitize(medium)
        #expect(Date().timeIntervalSince(mediumStart) < 0.2)
    }

    /// libxml ignores `</b>` while a `div` is open, so this nests two levels deeper each time and
    /// libxml's time grows with the square of the depth: the estimate follows it, the text is plain.
    @Test func inlineEndTagsThatCannotCloseABlockCountAsDepth() {
        let html = String(repeating: "<b><div>x</b>", count: 5_000)  // 63 KB: 0.8 s in libxml
        #expect(ArticleTemplate.estimatedNestingDepth(html) == 10_000)
        let start = Date()
        let clean = ArticleTemplate.sanitize(html)
        #expect(Date().timeIntervalSince(start) < 0.2)
        #expect(!clean.contains("<div") && clean.contains("x"))
        // Where libxml does close: `</div>` closes a `b`, and `</i>` a `p` (the same priority).
        #expect(ArticleTemplate.estimatedNestingDepth(String(repeating: "<div><b>x</div>", count: 1_000)) == 2)
        #expect(ArticleTemplate.estimatedNestingDepth(String(repeating: "<i><p>x</i>", count: 1_000)) == 2)
        // The scan stops once past the limit.
        #expect(ArticleTemplate.estimatedNestingDepth(html, stoppingAbove: 256) == 257)
    }

    /// The estimate against libxml itself: never below its depth by more than the few elements
    /// libxml adds on its own.
    @Test func theDepthEstimateFollowsLibxml() throws {
        func depth(_ node: XMLNode) -> Int {
            1 + (node.children ?? []).filter { $0.kind == .element }.map(depth).reduce(0, max)
        }
        let shapes = [
            String(repeating: "<b><div>x</b>", count: 300), String(repeating: "<div><b>x</div>", count: 300),
            String(repeating: "<i><p>x</i>", count: 300), String(repeating: #"<a href="https://e.org">x<p>y</a>"#, count: 300),
            String(repeating: "<span><div>x</span>", count: 300), String(repeating: "<td><div>x</td>", count: 300),
            String(repeating: "<table><tr><td>x</b>", count: 100), String(repeating: "<ul><li>x<p>y</ul>", count: 300),
        ]
        for html in shapes {
            let document = try XMLDocument(xmlString: "<html><body>\(html)</body></html>",
                                           options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever])
            let body = try #require(document.rootElement()?.children?.first { $0.name == "body" })
            #expect(ArticleTemplate.estimatedNestingDepth(html) + 5 >= depth(body) - 1, "\(html.prefix(30))")
        }
    }

    /// libxml checks a tag's attributes pairwise: past `maxAttributesPerTag`, plain text. The
    /// fallback strips tags, so the markup carries a `<b>` that only the parsed path keeps.
    @Test func aTagWithThousandsOfAttributesIsShownAsText() {
        func tag(_ count: Int) -> String { "<p " + (0..<count).map { "a\($0)=x" }.joined(separator: " ") + "><b>kept</b></p>" }
        let many = tag(20_000)  // 170 KB: 0.7 s in libxml
        #expect(ArticleTemplate.mostAttributesOnATag(many) == 20_000)
        #expect(ArticleTemplate.mostAttributesOnATag(many, stoppingAbove: 2_048) == 2_049)
        let start = Date()
        let clean = ArticleTemplate.sanitize(many)
        #expect(Date().timeIntervalSince(start) < 0.2)
        #expect(!clean.contains("<b>") && clean.contains("kept"))
        #expect(!ArticleTemplate.sanitize(tag(2_049)).contains("<b>"))  // just over the bound: text
        #expect(ArticleTemplate.sanitize(tag(2_000)).contains("<b>kept</b>"))  // about the bound: parsed
        // Quoted values are skipped whole, spaces and `>` included; unquoted ones up to a space.
        #expect(ArticleTemplate.mostAttributesOnATag(#"<p>x</p><img src="a b>c" alt='d e' width=600 data-x = "1">"#) == 4)
        #expect(ArticleTemplate.mostAttributesOnATag("<p>no attributes</p><br/>") == 0)
    }

    /// At most two workers at once (an abandoned one still running plus a live one), through a gate
    /// of the test's own so that the shared one stays free for other suites. A new article is
    /// never blocked by one abandoned worker; a third waits its turn, then skips it.
    @Test func abandonedWorkersAreBoundedAndNeverBlockTheNextArticle() {
        final class Probe: Sendable {
            let running = Mutex(0)
            let peak = Mutex(0)
            let runs = Mutex(0)
        }
        let probe = Probe()
        let gate = DispatchSemaphore(value: ArticleTemplate.maxWorkers)
        let work: @Sendable () -> String = {
            let now = probe.running.withLock { $0 += 1; return $0 }
            probe.peak.withLock { $0 = max($0, now) }
            probe.runs.withLock { $0 += 1 }
            Thread.sleep(forTimeInterval: 1)
            probe.running.withLock { $0 -= 1 }
            return "done"
        }
        #expect(ArticleTemplate.onLargeStack(timeout: 0.3, gate: gate, work) == nil)  // abandoned while it runs
        // One abandoned worker is running: the next article still gets its worker, within its budget.
        #expect(ArticleTemplate.onLargeStack(timeout: 5, gate: gate, work) == "done")
        #expect(probe.peak.withLock { $0 } == 2)  // it ran beside the abandoned one, not after it
        // Two running (one abandoned), then more callers that give up: they wait, then skip.
        #expect(ArticleTemplate.onLargeStack(timeout: 0.3, gate: gate, work) == nil)
        for _ in 0..<3 {
            #expect(ArticleTemplate.onLargeStack(timeout: 0.05, gate: gate, work) == nil)
        }
        #expect(ArticleTemplate.onLargeStack(timeout: 5, gate: gate, work) == "done")
        #expect(probe.peak.withLock { $0 } <= 2)
        #expect(probe.runs.withLock { $0 } <= 5)
    }

    /// Up to 512 KB the article is parsed; the largest stored one was 358 KB on 15/10/2026.
    @Test func articlesUpTo512KBAreParsed() {
        let paragraph = "<p>" + String(repeating: "Words of an ordinary long article. ", count: 12) + "<b>bold</b></p>\n"
        let big = String(repeating: paragraph, count: 400_000 / paragraph.utf8.count)
        #expect(big.utf8.count > 390_000)
        #expect(ArticleTemplate.sanitize(big + "<!-- \(UUID()) -->", budget: 10).contains("<b>bold</b>"))
        let huge = String(repeating: paragraph, count: 540_000 / paragraph.utf8.count)
        #expect(!ArticleTemplate.sanitize(huge, budget: 10).contains("<b>"))
    }

    /// Space reads the page's height: an image holds its place before it loads when its feed
    /// gives its size for this very src, in an attribute pair or a style. The data attributes of WordPress and
    /// Blogger describe the original upload, not the served thumbnail, so they give none.
    @Test func imagesGetTheSizeTheirFeedGives() {
        let html = #"<img src="https://e.org/a.png" style="width: 600px; height:400px"><img src="https://e.org/b.png" data-orig-size="1200,800">"#
            + #"<img src="https://e.org/c.png" data-original-width="640" data-original-height="480"><img src="https://e.org/d.png" width="50%" style="height: 10px">"#
            + #"<img src="https://e.org/e.png" width="320"><img src="https://e.org/f.png">"#
        let clean = ArticleTemplate.sanitize(html)
        #expect(clean.contains(#"<img src="https://e.org/a.png" width="600" height="400" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/b.png" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/c.png" loading="lazy">"#))
        #expect(ArticleTemplate.declaredSize(["data-orig-size": "1200,800"]) == nil)
        #expect(ArticleTemplate.declaredSize(["data-original-width": "640", "data-original-height": "480"]) == nil)
        #expect(clean.contains(#"<img src="https://e.org/d.png" width="50%" loading="lazy">"#))  // its own attribute wins
        #expect(clean.contains(#"<img src="https://e.org/e.png" width="320" loading="lazy">"#))
        #expect(clean.contains(#"<img src="https://e.org/f.png" loading="lazy">"#))
        #expect(!clean.contains("style") && !clean.contains("data-"))
        #expect(ArticleTemplate.declaredSize(["width": "600px", "height": " 400 "]) == ArticleTemplate.ImageSize(width: 600, height: 400))
        #expect(ArticleTemplate.declaredSize(["width": "0", "height": "400"]) == nil)
        #expect(ArticleTemplate.declaredSize(["style": "width:100%;height:auto"]) == nil)
    }

    /// Antiope: the mosaic holds the height its renderer will draw.
    @Test func mosaicsHoldTheirHeightWhenTheFeedGivesASize() {
        let clean = ArticleTemplate.sanitize(#"<img src="https://e.org/a.png" width="600" height="400"><img src="https://e.org/b.png">"#, mosaics: true)
        // 600 pixels: 40 columns (the most); 0.6 × 40 × 400 / 600 = 16 rows of 10 font pixels.
        // The box holds the height, in font pixels; the image itself is drawn at its real grid.
        #expect(clean.contains(#"style="min-height: calc(160px * var(--font-pixel))"><img src="#))
        #expect(clean.contains(#"alt="" loading="lazy"><span class="mosaic-alt">"#))
        #expect(clean.components(separatedBy: "min-height").count == 2)  // the image without a size has none
    }

    @Test func optionalEndTagsStayOnTheParsedPath() {
        let rows = String(repeating: "<tr><td>a<td>b", count: 150)
        let defs = String(repeating: "<dt>t<dd>d", count: 150)
        let items = String(repeating: "<li><p>item", count: 150)
        for html in [rows, defs, items] {
            #expect(ArticleTemplate.estimatedNestingDepth(html) < 10)
        }
    }

    @Test func sanitiseIsMemoised() {
        let html = "<p>memo test \(UUID().uuidString)</p>"
        let first = ArticleTemplate.sanitize(html)
        let before = ArticleTemplate.cacheStats()
        #expect(ArticleTemplate.sanitize(html) == first)
        let after = ArticleTemplate.cacheStats()
        // Other tests may use the cache concurrently, so the counters can only be bounded from below.
        #expect(after.hits >= before.hits + 1)
    }

    @Test func parseRunsOnTheLargeStackThread() {
        let probe = ArticleTemplate.onLargeStack { "\(Thread.isMainThread)" }
        #expect(probe == "false")
        #expect(ArticleTemplate.sanitize("<p>hi</p>").contains("<p>hi"))
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
        #expect(!off.contains(#"<span class="badge">"#))
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

/// Acceptance fix 2: the Full Article button says what it does, and why it is off.
@Suite struct FullArticleButtonTests {
    @MainActor @Test func theTooltipGivesTheStateAndWhyTheButtonIsOff() {
        func help(_ entry: EntryRow?, showing: Bool = false, extracting: Bool = false, failure: String? = nil) -> String {
            ReaderView.fullArticleHelp(for: entry, isShowingFullText: showing, isExtracting: extracting, failure: failure)
        }
        #expect(help(nil) == "Select an entry to read its full article")
        #expect(help(entry(url: nil)) == "This entry has no web page to fetch the full article from")
        #expect(help(entry(), extracting: true) == "Fetching the full article…")
        #expect(help(entry()) == "Show the full article, fetched from the site")
        #expect(help(entry(), showing: true) == "Showing the full article. Click to show the feed's text")
        #expect(help(entry(), failure: "Full article unavailable: HTTP 403.")
            == "Full article unavailable: HTTP 403. Click to try again")
    }
}

@Suite struct AntiopeReaderStyleTests {
    @Test func antiopeStepsThroughBedsteadSizes() {
        #expect(ReaderStyle.effectiveSize(17, appearance: .antiope) == 15)
        #expect(ReaderStyle.effectiveSize(17, appearance: .native) == 17)
        #expect(ReaderStyle.effectiveSize(.nan, appearance: .antiope) == 20)
        #expect(ReaderStyle.effectiveSize(40, appearance: .native) == 28)
        #expect(ReaderStyle.bigger(17, appearance: .antiope) == 20)  // drawn at 15, so the next step is 20
        #expect(ReaderStyle.smaller(17, appearance: .antiope) == 15)
        #expect(ReaderStyle.bigger(20, appearance: .antiope) == 25)
        #expect(ReaderStyle.bigger(30, appearance: .antiope) == 30)
        #expect(ReaderStyle.smaller(25, appearance: .antiope) == 20)
        #expect(ReaderStyle.smaller(15, appearance: .antiope) == 15)
        #expect(ReaderStyle.bigger(17, appearance: .native) == 18)
        #expect(ReaderStyle.smaller(17, appearance: .native) == 16)
        #expect(ReaderStyle.actualSize(for: .antiope) == 20)
        #expect(ReaderStyle.actualSize(for: .native) == 17)
    }

    @Test func fortyColumnsAndTheTheme() {
        #expect(LineWidth.forty.maxCh == 40)
        #expect(LineWidth.forty.displayName == "40 columns")
        #expect(LineWidth.allCases.map(\.maxCh) == [58, 68, 80, 40])
        #expect(ReaderTheme.native == ReaderTheme(appearance: .native, crt: false))
        #expect(!ReaderTheme(appearance: .native, crt: true).showsCRT)
        #expect(!ReaderTheme(appearance: .antiope, crt: false).showsCRT)
        #expect(ReaderTheme(appearance: .antiope, crt: true).showsCRT)
    }
}

/// Decision of 2026-10-14: each look keeps its own text size, and ⌘+, ⌘− and ⌘0 act on the
/// look shown. Every test writes a throwaway defaults suite: the test host's are Clément's.
@Suite struct TextSizePerLookTests {
    private func withDefaults(_ body: (UserDefaults) throws -> Void) throws {
        let suite = "ressac.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        try body(defaults)
    }

    @Test func switchingLooksNeverChangesTheOtherSize() throws {
        #expect(ReaderStyle.sizeKey(for: .native) == "reader.size")
        #expect(ReaderStyle.sizeKey(for: .antiope) == "reader.size.antiope")
        try withDefaults { defaults in
            #expect(ReaderStyle.storedSize(for: .native, in: defaults) == 17)
            #expect(ReaderStyle.storedSize(for: .antiope, in: defaults) == 20)  // not Native's 17 snapped to 15
            ReaderStyle.resize(.bigger, appearance: .antiope, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 25)
            #expect(defaults.object(forKey: ReaderStyle.Keys.size) == nil)  // Native is found as it was
            ReaderStyle.resize(.smaller, appearance: .native, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 16)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 25)
        }
    }

    @Test func actualSizeRestoresTheDefaultOfTheLookShown() throws {
        try withDefaults { defaults in
            defaults.set(22.0, forKey: ReaderStyle.Keys.size)
            defaults.set(30.0, forKey: ReaderStyle.Keys.antiopeSize)
            ReaderStyle.resize(.actualSize, appearance: .antiope, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 20)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 22)
            ReaderStyle.resize(.actualSize, appearance: .native, in: defaults)
            #expect(defaults.double(forKey: ReaderStyle.Keys.size) == 17)
            #expect(defaults.double(forKey: ReaderStyle.Keys.antiopeSize) == 20)
        }
    }

    /// Whatever is stored (a hand edit, an older build), Antiope draws a step and the menu
    /// stores a step.
    @Test func theAntiopeSizeIsAlwaysAStep() throws {
        try withDefaults { defaults in
            for stored in [17.0, 0, -3, 22.5, 99] {
                #expect(TeletextFont.sizeSteps.contains(ReaderStyle.effectiveSize(stored, appearance: .antiope)), "\(stored)")
                for command in [TextSizeCommand.bigger, .smaller, .actualSize] {
                    defaults.set(stored, forKey: ReaderStyle.Keys.antiopeSize)
                    ReaderStyle.resize(command, appearance: .antiope, in: defaults)
                    #expect(TeletextFont.sizeSteps.contains(defaults.double(forKey: ReaderStyle.Keys.antiopeSize)),
                            "\(stored) \(command)")
                }
            }
        }
    }
}

@Suite struct SubpageLabelTests {
    @Test func theLabelReadsPageIndexAndCount() {
        #expect(SubpageLabel(page: 412, index: 3, count: 20).text == "412 3/20")
        #expect(SubpageLabel(page: nil, index: 1, count: 1).text == "1/1")
        let list = [entry(id: 5), entry(id: 6), entry(id: 7)]
        #expect(SubpageLabel.of(entryId: 6, in: list) { $0 == 7 ? 412 : nil } == SubpageLabel(page: 412, index: 2, count: 3))
        #expect(SubpageLabel.of(entryId: 9, in: list) { _ in 412 } == nil)
        // A long list loads its first rows only: the count is the whole list's.
        #expect(SubpageLabel.of(entryId: 6, in: list, total: 5_907) { _ in 412 } == SubpageLabel(page: 412, index: 2, count: 5_907))
    }
}

@Suite struct AntiopePageTests {
    private let antiope = ReaderTheme(appearance: .antiope, crt: false)

    private func page(theme: ReaderTheme, subpage: SubpageLabel? = nil, style: ReaderStyle = ReaderStyle(), title: String? = "A title") -> String {
        ArticleTemplate.page(for: entry(title: title), fulltext: nil, showFull: false, style: style, colorScheme: .dark,
                             theme: theme, subpage: subpage)
    }

    @Test func nativeStaysAsItWas() {
        // The page embeds reader.css, whose text names the Antiope selectors: check the markup itself.
        let native = page(theme: .native, subpage: SubpageLabel(page: 412, index: 3, count: 20))
        #expect(native.contains(#"<html lang="en" data-scheme="dark">"#))
        #expect(!native.contains(#"<p class="subpage""#))
        #expect(!native.contains(#"--teletext-font: "Bedstead""#))
        #expect(native.contains(#"<h1 class="title"><a href="https://blog.example.org/2026/10/post.html">A title</a></h1>"#))
    }

    @Test func antiopeMarksTheRootSnapsTheSizeAndDoublesTheTitle() {
        let html = page(theme: antiope, subpage: SubpageLabel(page: 412, index: 3, count: 20),
                        style: ReaderStyle(font: .charter, size: 17, width: .forty))
        #expect(html.contains(#"<html lang="en" data-scheme="dark" data-theme="antiope">"#))
        #expect(html.contains("--reader-size: 15px"))
        #expect(html.contains("--reader-measure: 40ch"))
        #expect(html.contains(#"--teletext-font: "Bedstead", monospace;"#))
        #expect(html.contains(#"<p class="subpage" id="subpage">412 3/20</p>"#))
        #expect(html.contains(#"<h1 class="title"><span class="dh-space" aria-hidden="true">A title</span><span class="dh-ink"><a href="https://blog.example.org/2026/10/post.html">A title</a></span></h1>"#))
        // The sub-page line exists even before the list knows the entry, so it can be filled in place.
        #expect(page(theme: antiope).contains(#"<p class="subpage" id="subpage"></p>"#))
        #expect(page(theme: antiope, title: "<b>&</b>").contains(#"<span class="dh-space" aria-hidden="true">&lt;b&gt;&amp;&lt;/b&gt;</span>"#))
    }

    @Test func dataCRTOnlyForAntiopeWithTheBoxChecked() {
        #expect(page(theme: ReaderTheme(appearance: .native, crt: true)).contains(#"<html lang="en" data-scheme="dark">"#))
        #expect(page(theme: antiope).contains(#"<html lang="en" data-scheme="dark" data-theme="antiope">"#))
        #expect(page(theme: ReaderTheme(appearance: .antiope, crt: true))
            .contains(#"<html lang="en" data-scheme="dark" data-theme="antiope" data-crt>"#))
    }
}

/// Spec §5: no colour but the eight in an Antiope view; §9: the bleed is text-only, under data-crt.
@Suite struct AntiopeStylesheetTests {
    @Test func theAntiopeSectionUsesOnlyTheEightColours() throws {
        let css = ArticleTemplate.readerCSS
        let start = try #require(css.range(of: "Antiope begin"))
        let end = try #require(css.range(of: "Antiope end"))
        let section = String(css[start.upperBound..<end.lowerBound])
        let colours = Set(section.matches(of: /#[0-9A-Fa-f]{3,8}\b/).map { String($0.output).uppercased() })
        #expect(colours == Set(TeletextColor.allCases.map(\.css)))
        for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
            #expect(!section.contains(function), "\(function)")
        }
        #expect(section.contains("--smoothing: none;"))
        #expect(section.contains("transform: scaleY(2);"))
    }

    @Test func theCRTBleedIsTextOnlyAndOnlyUnderDataCRT() {
        #expect(ArticleTemplate.readerCSS.contains(#":root[data-theme="antiope"][data-crt] body {"#
            + "\n  text-shadow: 1px 0 rgba(255, 0, 0, 0.45), -1px 0 rgba(0, 0, 255, 0.45), 0 0 2px currentColor;"))
    }

    @Test func bedsteadIsEmbeddedInAntiopeOnly() {
        let face = #"@font-face { font-family: "Bedstead"; font-style: normal; font-weight: 400; src: url("data:font/otf;base64,"#
        let antiope = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: ReaderStyle(font: .newsreader),
                                           colorScheme: .light, theme: ReaderTheme(appearance: .antiope, crt: false))
        let native = ArticleTemplate.page(for: entry(), fulltext: nil, showFull: false, style: ReaderStyle(font: .newsreader),
                                          colorScheme: .light)
        #expect(antiope.contains(face))
        #expect(!antiope.contains(#"font-family: "Newsreader""#))
        #expect(!native.contains(face))
    }
}

@Suite struct AntiopeReaderViewTests {
    @Test func pageIdentityIgnoresTheSubpageButNotTheTheme() {
        let a = ArticlePage(entry: entry(), fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .dark,
                            theme: ReaderTheme(appearance: .antiope, crt: false), subpage: SubpageLabel(page: 412, index: 1, count: 2))
        var b = a
        b.subpage = SubpageLabel(page: 412, index: 1, count: 3)
        var c = a
        c.theme = ReaderTheme(appearance: .antiope, crt: true)
        #expect(a == b)
        #expect(a != c)
    }

    @Test func theEmptyReaderIsOneCyanEnglishLineAsWideAsTheColumn() {
        for width in [0, 10, 17, 40, 41] {
            let line = AntiopeReaderLines.noEntry(width: width)
            #expect(line.width == width)
            #expect(line.accessibilityText == "No entry selected")
            #expect(line.spans.allSatisfy { $0.foreground == .cyan && $0.background == .black })
        }
        let line = AntiopeReaderLines.noEntry(width: 40)
        #expect(line.spans.map(\.cells.accessibilityText).joined().trimmingCharacters(in: .whitespaces) == "NO ENTRY SELECTED")
    }

    @Test func theRunFailureNoticeIsRedReverseVideoAsWideAsTheColumn() {
        for width in [0, 10, 40, 80] {
            let line = AntiopeReaderLines.failureNotice("Could not fetch the page.", width: width)
            #expect(line.width == width)
            #expect(line.accessibilityText == "Could not fetch the page.")
            #expect(line.spans.allSatisfy { $0.foreground == .black && $0.background == .red })
        }
    }
}

@MainActor @Suite struct SubpageLineTests {
    @Test func theLineChangesInPlaceWithoutAReload() async throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        let webView = WKWebView(frame: window.contentLayoutRect)
        window.contentView = webView
        let controller = ReaderController()
        controller.webView = webView
        let coordinator = ArticleWebView.Coordinator(controller: controller)
        webView.navigationDelegate = coordinator
        let html = ArticleTemplate.page(for: entry(content: String(repeating: "<p>Line of a long article.</p>", count: 200)),
                                        fulltext: nil, showFull: false, style: ReaderStyle(), colorScheme: .dark,
                                        theme: ReaderTheme(appearance: .antiope, crt: false), subpage: SubpageLabel(page: 412, index: 3, count: 20))
        controller.load(entryId: 1, html: html)
        #expect(await eventually { controller.loadedEntryId == 1 })
        _ = try await webView.callAsyncJavaScript("window.scrollTo(0, 300)", contentWorld: .page)
        await controller.showSubpage(SubpageLabel(page: 412, index: 3, count: 21))
        let line = try await webView.callAsyncJavaScript(#"return document.getElementById("subpage").textContent"#, contentWorld: .page)
        #expect(line as? String == "412 3/21")
        let scroll = try await webView.callAsyncJavaScript("return window.scrollY", contentWorld: .page)
        #expect((scroll as? Double ?? 0) > 0)  // same document: the position is kept
    }
}

/// Both looks load the next page of a long list as its last rows appear.
@MainActor @Suite struct EntryListPagingTests {
    /// Five entries listed two at a time, hosted in `root` (an offscreen window, laid out by hand).
    private func pagesIn(_ root: (AppState) -> some View) async throws -> Bool {
        let (state, test, _) = try makeState()
        defer { withExtendedLifetime(test) {} }  // its directory holds the database
        for day in 1...5 {
            try test.entry(feed: 1, published: String(format: "2026-10-%02dT08:00:00.000Z", day))
        }
        state.listPageSize = 2
        state.selection = .feed(1)
        #expect(state.entries.count == 2)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: root(state))
        defer { window.contentView = nil }
        return await eventually {
            window.contentView?.layoutSubtreeIfNeeded()
            return state.entries.count == 5
        }
    }

    @Test func theNativeListLoadsMoreAsItsLastRowsAppear() async throws {
        #expect(try await pagesIn { EntryListView(state: $0) })
    }

    @Test func theAntiopeColumnLoadsMoreAsItsLastRowsAppear() async throws {
        #expect(try await pagesIn { EntryPageColumn().environment($0).environment(AntiopeState(app: $0)) })
    }
}

/// The image box holds its height before the image arrives, in both looks.
@MainActor @Suite struct ImageBoxTests {
    @Test func anImageOfKnownSizeHoldsItsPlace() async throws {
        let content = #"<p>Text.</p><img src="http://127.0.0.1:9/a.png" style="width:600px;height:400px">"#
        // The box that holds the place: the mosaic's in Antiope (its image is not stretched), the image's in Native.
        for theme in [ReaderTheme.native, ReaderTheme(appearance: .antiope, crt: false)] {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
            let webView = WKWebView(frame: window.contentLayoutRect)
            window.contentView = webView
            let controller = ReaderController()
            controller.webView = webView
            let coordinator = ArticleWebView.Coordinator(controller: controller)
            webView.navigationDelegate = coordinator
            let html = ArticleTemplate.page(for: entry(content: content), fulltext: nil, showFull: false, style: ReaderStyle(),
                                            colorScheme: .dark, theme: theme)
            controller.load(entryId: 1, html: html)
            #expect(await eventually { controller.loadedEntryId == 1 })
            let height = try await webView.callAsyncJavaScript(
                #"return document.querySelector(".content .mosaic, .content img").getBoundingClientRect().height"#, contentWorld: .page)
            #expect((height as? Double ?? 0) > 100, "\(theme.appearance)")
        }
    }
}
