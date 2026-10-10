import Foundation
import SwiftUI
import Synchronization
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
        let key = CacheKey(html: html, baseURL: baseURL)
        if let cached = cache.withLock({ $0.lookup(key) }) { return cached }
        let result = sanitizeUncached(html, baseURL: baseURL)
        if html.utf8.count <= maxParsedBytes { cache.withLock { $0.store(key, result) } }
        return result
    }

    private static func sanitizeUncached(_ html: String, baseURL: URL?) -> String {
        // Tidy and the recursive renderer overflow the stack on pathological nesting, and libxml's
        // repair of misnested end tags is quadratic, so such input (or anything big) skips the
        // parser and is shown as escaped plain text.
        let size = html.utf8.count
        if size > maxParsedBytes || estimatedNestingDepth(html) > maxNestingDepth {
            return size > maxInputBytes ? plainTextFallback(String(decoding: html.utf8.prefix(maxInputBytes), as: UTF8.self)) : plainTextFallback(html)
        }
        return onLargeStack(timeout: parseBudget) { parseAndRender(html, baseURL: baseURL) } ?? plainTextFallback(html)
    }

    // MARK: Memo

    private struct CacheKey: Hashable, Sendable {
        let html: String
        let baseURL: URL?
    }

    private struct Memo: Sendable {
        var values: [CacheKey: String] = [:]
        var order: [CacheKey] = []
        var hits = 0
        var misses = 0

        mutating func lookup(_ key: CacheKey) -> String? {
            guard let value = values[key] else { misses += 1; return nil }
            hits += 1
            if let index = order.firstIndex(of: key) { order.remove(at: index) }
            order.append(key)
            return value
        }

        mutating func store(_ key: CacheKey, _ value: String) {
            if values[key] == nil {
                order.append(key)
                if order.count > ArticleTemplate.cacheCapacity { values[order.removeFirst()] = nil }
            }
            values[key] = value
        }
    }

    static let cacheCapacity = 32
    private static let cache = Mutex(Memo())

    /// Cache hit and miss counts, for tests.
    static func cacheStats() -> (hits: Int, misses: Int) {
        cache.withLock { ($0.hits, $0.misses) }
    }

    private static func parseAndRender(_ html: String, baseURL: URL?) -> String {
        let wrapped = "<html><body>\(html)</body></html>"
        guard let document = try? XMLDocument(xmlString: wrapped, options: [.documentTidyHTML, .nodeLoadExternalEntitiesNever]),
              let body = document.rootElement()?.children?.first(where: { name(of: $0) == "body" })
        else {
            return "<p>\(escapeText(html))</p>"
        }
        var out = ""
        for child in body.children ?? [] {
            render(child, baseURL: baseURL, depth: 1, into: &out)
        }
        return out
    }

    private final class ResultBox: @unchecked Sendable { var value = "" }

    /// Runs `work` on a thread with a 256 MB stack (a virtual reservation) and waits for it, so
    /// libxml's recursive parser cannot exhaust the stack on input the depth guard let through.
    /// Returns `nil` when `timeout` passes first. The worker cannot be cancelled: it finishes in
    /// the background and its late result is discarded; its cost is bounded by `maxParsedBytes`.
    static func onLargeStack(timeout: TimeInterval = .infinity, _ work: @escaping @Sendable () -> String) -> String? {
        let box = ResultBox()
        let done = DispatchSemaphore(value: 0)
        let thread = Thread {
            box.value = work()
            done.signal()
        }
        thread.stackSize = 256 << 20
        thread.qualityOfService = .userInteractive
        thread.start()
        let deadline: DispatchTime = timeout.isFinite ? .now() + timeout : .distantFuture
        return done.wait(timeout: deadline) == .success ? box.value : nil
    }

    static let parseBudget: TimeInterval = 1.0
    static let maxParsedBytes = 256 * 1024
    static let maxNestingDepth = 256
    static let maxInputBytes = 2_000_000
    private static let voidTags: Set<String> = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
    ]

    /// Linear, non-recursive estimate of the deepest element nesting: opening tags minus closing
    /// tags, ignoring void and self-closing tags, comments, `<!…>` and `<?…>`.
    static func estimatedNestingDepth(_ html: String) -> Int {
        let bytes = Array(html.utf8)
        // Conservative: it may overestimate, never underestimate. A close tag pops only down to a
        // matching open name (a stray one pops nothing), and a tag ends at the first `>`.
        var stack: [String] = []
        var open: [String: Int] = [:]
        var deepest = 0, i = 0
        func pop() {
            if let name = stack.popLast() { open[name, default: 1] -= 1 }
        }
        while i < bytes.count {
            guard bytes[i] == 0x3C else { i += 1; continue }  // <
            let next = i + 1 < bytes.count ? bytes[i + 1] : 0
            if next == 0x21 || next == 0x3F {  // <! or <?
                if bytes[i...].starts(with: [0x3C, 0x21, 0x2D, 0x2D]) {
                    i = indexAfter([0x2D, 0x2D, 0x3E], in: bytes, from: i + 4)
                } else {
                    i = indexAfter([0x3E], in: bytes, from: i + 1)
                }
                continue
            }
            let closing = next == 0x2F
            let nameStart = closing ? i + 2 : i + 1
            var j = nameStart
            while j < bytes.count, isASCIILetterOrDigit(bytes[j]) { j += 1 }
            guard j > nameStart, isASCIILetter(bytes[nameStart]) else { i += 1; continue }
            let tag = String(decoding: bytes[nameStart..<j], as: UTF8.self).lowercased()
            var k = j
            while k < bytes.count, bytes[k] != 0x3E { k += 1 }
            if closing {
                if open[tag, default: 0] > 0 {
                    while let top = stack.last, top != tag { pop() }
                    pop()
                }
            } else if !voidTags.contains(tag) {
                closeOptionalEndTags(before: tag, stack: stack, pop: pop)
                stack.append(tag)
                open[tag, default: 0] += 1
                deepest = max(deepest, stack.count)
            }
            i = k + 1
        }
        return deepest
    }

    /// HTML's optional end tags: what an opening tag closes when it sits on top of the stack.
    private static func closeOptionalEndTags(before tag: String, stack: [String], pop: () -> Void) {
        var top = stack.count - 1
        func topIs(_ names: Set<String>) -> Bool { top >= 0 && names.contains(stack[top]) }
        switch tag {
        case "td", "th":
            if topIs(["td", "th"]) { pop() }
        case "tr":
            while topIs(["td", "th", "tr"]) { pop(); top -= 1 }
        case "dt", "dd":
            if topIs(["dt", "dd"]) { pop() }
        case "li":
            if topIs(["p"]) { pop(); top -= 1 }
            if topIs(["li"]) { pop() }
        case "p":
            if topIs(["p"]) { pop() }
        case "option":
            if topIs(["option"]) { pop() }
        default:
            break
        }
    }

    private static func indexAfter(_ pattern: [UInt8], in bytes: [UInt8], from start: Int) -> Int {
        var i = start
        while i + pattern.count <= bytes.count {
            if bytes[i] == pattern[0], bytes[i..<(i + pattern.count)].elementsEqual(pattern) { return i + pattern.count }
            i += 1
        }
        return bytes.count
    }

    private static func isASCIILetter(_ b: UInt8) -> Bool { (0x41...0x5A).contains(b) || (0x61...0x7A).contains(b) }
    private static func isASCIILetterOrDigit(_ b: UInt8) -> Bool { isASCIILetter(b) || (0x30...0x39).contains(b) }

    /// Tags stripped by a linear scan, basic entities decoded, blank lines turned into paragraphs,
    /// everything escaped.
    static func plainTextFallback(_ html: String) -> String {
        let bytes = Array(html.utf8)
        var text = [UInt8]()
        text.reserveCapacity(bytes.count)
        var i = 0
        while i < bytes.count {
            if bytes[i] == 0x3C, i + 1 < bytes.count {
                let next = bytes[i + 1]
                if next == 0x21 || next == 0x3F || next == 0x2F || isASCIILetter(next) {
                    if bytes[i...].starts(with: [0x3C, 0x21, 0x2D, 0x2D]) {
                        i = indexAfter([0x2D, 0x2D, 0x3E], in: bytes, from: i + 4)
                    } else {
                        i = indexAfter([0x3E], in: bytes, from: i + 1)
                    }
                    text.append(0x20)
                    continue
                }
            }
            text.append(bytes[i])
            i += 1
        }
        let decoded = String(decoding: text, as: UTF8.self)
            .replacingOccurrences(of: "&nbsp;", with: " ")
            .replacingOccurrences(of: "&lt;", with: "<")
            .replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&#39;", with: "'")
            .replacingOccurrences(of: "&amp;", with: "&")
        let paragraphs = decoded
            .replacingOccurrences(of: "\r\n", with: "\n")
            .components(separatedBy: "\n\n")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        return paragraphs.map { "<p>\(escapeText($0))</p>" }.joined(separator: "\n")
    }

    private static func name(of node: XMLNode) -> String {
        (node.localName ?? node.name ?? "").lowercased()
    }

    private static func render(_ node: XMLNode, baseURL: URL?, depth: Int, into out: inout String) {
        if depth > maxNestingDepth { return }
        switch node.kind {
        case .text:
            out += escapeText(node.stringValue ?? "")
        case .element:
            guard let element = node as? XMLElement else { return }
            let tag = name(of: element)
            if droppedElements.contains(tag) { return }
            guard allowedElements.contains(tag) else {
                for child in element.children ?? [] { render(child, baseURL: baseURL, depth: depth + 1, into: &out) }
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
            for child in element.children ?? [] { render(child, baseURL: baseURL, depth: depth + 1, into: &out) }
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
