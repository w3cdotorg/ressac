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
