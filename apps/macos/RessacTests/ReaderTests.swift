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
        // 200 KB of the same shape: within the cap, so it returns within the budget (parsed or fallback).
        let medium = String(repeating: "<b><div>x</b>", count: 15_000)
        let mediumStart = Date()
        _ = ArticleTemplate.sanitize(medium)
        #expect(Date().timeIntervalSince(mediumStart) < 1.5)
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
        #expect(ReaderView.fullArticleHelp(for: nil, showsFullText: false, isExtracting: false)
            == "Select an entry to read its full article")
        #expect(ReaderView.fullArticleHelp(for: entry(url: nil), showsFullText: false, isExtracting: false)
            == "This entry has no web page to fetch the full article from")
        #expect(ReaderView.fullArticleHelp(for: entry(), showsFullText: true, isExtracting: true)
            == "Fetching the full article…")
        #expect(ReaderView.fullArticleHelp(for: entry(), showsFullText: false, isExtracting: false)
            == "Show the full article, fetched from the site")
        #expect(ReaderView.fullArticleHelp(for: entry(), showsFullText: true, isExtracting: false)
            == "Showing the full article. Click to show the feed's text")
    }
}
