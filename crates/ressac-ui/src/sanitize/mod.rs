//! The reader's HTML sanitiser (the Mac's `ArticleTemplate.sanitize`). Feed and extracted HTML
//! is hostile: an allowlist of inert elements and attributes survives, written back out by our
//! own serialiser; links and images keep only http(s) addresses (images also `data:image/…`,
//! never SVG). Input too big, too deep or with too many attributes on one tag skips the parser
//! and is shown as escaped plain text; the parse runs on a large-stack worker within a budget.
//!
//! Parsing is html5ever (through `scraper`), whose work grows with the square of the nesting
//! depth, so the depth guard follows html5ever's own tree-building rules (`depth`). The parse
//! runs on `worker`'s large-stack threads, and its results are memoised there.

mod depth;
mod worker;

use std::time::Duration;

use scraper::node::Element;
use scraper::{Html, Node};
use url::Url;

pub use depth::estimated_nesting_depth;
use worker::memo;

use crate::mosaic::{CELL_HEIGHT, MAX_COLUMNS, MosaicRequest, columns_for_width, grid};
pub use worker::{Gate, memo_stats, on_large_stack, shared_gate};

/// Past this, the plain-text fallback (the largest stored article was 358 KB on 15/10/2026).
pub const MAX_PARSED_BYTES: usize = 512 * 1024;
pub const MAX_NESTING_DEPTH: usize = 256;
/// A tag's attributes are compared pairwise for duplicates: past this, plain text.
pub const MAX_ATTRIBUTES_PER_TAG: usize = 2048;
/// The plain-text fallback's cut.
pub const MAX_INPUT_BYTES: usize = 2_000_000;
pub const PARSE_BUDGET: Duration = Duration::from_secs(1);
/// A running abandoned worker plus the live one: a new article always gets a worker.
pub const MAX_WORKERS: usize = 2;
pub const MEMO_CAPACITY: usize = 32;
/// A virtual reservation, so no parse can exhaust its thread's stack.
pub const WORKER_STACK_BYTES: usize = 256 << 20;

/// The scheme of a mosaic box's toggle link (`ressac-toggle:<n>`).
pub const TOGGLE_SCHEME: &str = "ressac-toggle";

/// What becomes of images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Images {
    /// Plain `img` elements (Native).
    Web,
    /// Antiope: each http(s) image becomes a numbered mosaic box (`ressac-mosaic:` source,
    /// original kept for the swap). `data:` images stay as they are.
    Mosaics,
}

/// Removed together with everything inside them (sorted).
const DROPPED: [&str; 29] = [
    "applet", "audio", "base", "button", "canvas", "dialog", "embed", "form", "frame", "frameset",
    "head", "iframe", "input", "link", "math", "meta", "noscript", "object", "option", "script",
    "select", "source", "style", "svg", "template", "textarea", "title", "track", "video",
];

/// Kept, with their allowed attributes (sorted). Any other element is unwrapped: its children stay.
#[rustfmt::skip]
const KEPT: [&str; 69] = [
    "a", "abbr", "article", "aside", "b", "bdi", "bdo", "blockquote", "br", "caption", "cite",
    "code", "col", "colgroup", "dd", "del", "details", "dfn", "div", "dl", "dt", "em", "figcaption",
    "figure", "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "i", "img", "ins",
    "kbd", "li", "main", "mark", "nav", "ol", "p", "pre", "q", "rp", "rt", "ruby", "s", "samp",
    "section", "small", "span", "strong", "sub", "summary", "sup", "table", "tbody", "td", "tfoot",
    "th", "thead", "time", "tr", "u", "ul", "var", "wbr",
];

/// HTML's void elements: never closed, never nested (sorted).
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Inert HTML for the reader, within `PARSE_BUDGET`, memoised.
pub fn sanitize(html: &str, base: Option<&str>, images: Images) -> String {
    sanitize_within(html, base, images, PARSE_BUDGET)
}

/// `sanitize` with its own parse budget. Relative `href`/`src` resolve against `base` when it
/// is a web address (`<base>` itself is removed).
pub fn sanitize_within(html: &str, base: Option<&str>, images: Images, budget: Duration) -> String {
    if let Some(cached) = memo().lookup(html, base, images) {
        return cached;
    }
    let (result, timed_out) = sanitize_uncached(html, base, images, budget);
    // A timed-out parse is a busy moment, not a property of the article: not remembered.
    if html.len() <= MAX_PARSED_BYTES && !timed_out {
        memo().store(html, base, images, &result);
    }
    result
}

/// The sanitised HTML, and whether the parse ran out of budget.
fn sanitize_uncached(
    html: &str,
    base: Option<&str>,
    images: Images,
    budget: Duration,
) -> (String, bool) {
    if html.len() > MAX_PARSED_BYTES
        || estimated_nesting_depth(html, MAX_NESTING_DEPTH) > MAX_NESTING_DEPTH
        || most_attributes_on_a_tag(html, MAX_ATTRIBUTES_PER_TAG) > MAX_ATTRIBUTES_PER_TAG
    {
        let cut = html.floor_char_boundary(MAX_INPUT_BYTES);
        return (plain_text_fallback(&html[..cut]), false);
    }
    let base = base
        .filter(|b| is_web_url(b))
        .and_then(|b| Url::parse(b).ok());
    let owned = html.to_owned();
    match on_large_stack(budget, shared_gate(), move || {
        render(&owned, base.as_ref(), images)
    }) {
        Some(Some(rendered)) => (rendered, false),
        // Deeper than the estimate said: a property of the article, so remembered.
        Some(None) => (plain_text_fallback(html), false),
        None => (plain_text_fallback(html), true),
    }
}

/// One step of the serialiser's walk: a node to write at a depth, or an element to close.
enum Step<N> {
    /// A node, its depth, and whether an `a` encloses it.
    Visit(N, usize, bool),
    Close(&'static str),
}

/// Parses `html` as a body fragment and writes the allowed tree back out, without recursion.
/// None when a node lies deeper than `MAX_NESTING_DEPTH` (the depth estimate may fall a few
/// levels short): nothing is dropped silently, the caller shows the plain text instead.
fn render(html: &str, base: Option<&Url>, images: Images) -> Option<String> {
    let mosaics = images == Images::Mosaics;
    let mut boxes = 0usize;
    let document = Html::parse_fragment(html);
    let mut out = String::with_capacity(html.len());
    let mut pending: Vec<_> = document
        .root_element()
        .children()
        .rev()
        .map(|child| Step::Visit(child, 1, false))
        .collect();
    while let Some(step) = pending.pop() {
        let (node, depth, in_link) = match step {
            Step::Close(tag) => {
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
                continue;
            }
            Step::Visit(node, depth, in_link) => (node, depth, in_link),
        };
        if depth > MAX_NESTING_DEPTH {
            return None;
        }
        let element = match node.value() {
            Node::Text(text) => {
                escape_text(&mut out, text);
                continue;
            }
            Node::Element(element) => element,
            // Comments, doctypes, processing instructions.
            _ => continue,
        };
        // Foreign (SVG, MathML) subtrees only start under a dropped `svg` or `math`.
        if &*element.name.ns != "http://www.w3.org/1999/xhtml"
            || DROPPED.binary_search(&element.name()).is_ok()
        {
            continue;
        }
        let Ok(kept) = KEPT.binary_search(&element.name()) else {
            pending.extend(
                node.children()
                    .rev()
                    .map(|child| Step::Visit(child, depth + 1, in_link)),
            );
            continue;
        };
        let tag = KEPT[kept];
        let mut attributes = safe_attributes(element, tag, base);
        if tag == "img" {
            let Some(source) = attributes
                .iter()
                .find_map(|(name, value)| (*name == "src").then_some(value.as_str()))
            else {
                continue;
            };
            if mosaics && is_web_url(source) {
                let alt = attributes
                    .iter()
                    .find_map(|(name, value)| (*name == "alt").then_some(value.as_str()))
                    .unwrap_or("");
                let all: Vec<(String, String)> = element
                    .attrs()
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect();
                write_mosaic_box(&mut out, boxes, source, alt, declared_size(&all), in_link);
                boxes += 1;
                continue;
            }
            // Its height is known before it loads, so Space never takes the page's end too early.
            if !attributes
                .iter()
                .any(|(name, _)| *name == "width" || *name == "height")
            {
                let all: Vec<(String, String)> = element
                    .attrs()
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect();
                if let Some(size) = declared_size(&all) {
                    attributes.push(("width", size.width.to_string()));
                    attributes.push(("height", size.height.to_string()));
                }
            }
        }
        out.push('<');
        out.push_str(tag);
        for (name, value) in &attributes {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_attribute(&mut out, value);
            out.push('"');
        }
        // Images below the fold wait until they are scrolled near.
        if tag == "img" {
            out.push_str(r#" loading="lazy""#);
        }
        out.push('>');
        if VOID.binary_search(&tag).is_err() {
            pending.push(Step::Close(tag));
            let in_link = in_link || tag == "a";
            pending.extend(
                node.children()
                    .rev()
                    .map(|child| Step::Visit(child, depth + 1, in_link)),
            );
        }
    }
    Some(out)
}

/// Image `index` as a mosaic box. The box keeps both addresses for the swap and an IMAGE line
/// with the alt text, shown by the stylesheet when the mosaic fails (`data-failed`). With the
/// original's `size`, the box keeps the mosaic's height before it arrives: the rows its renderer
/// will draw, in PNG pixels (one per font pixel) times `--font-pixel`. The box holds it, not
/// the `img`, which is always drawn at its real grid, never stretched. `original` is an
/// already-validated http(s) address; every value is escaped, and the sizes are integers.
fn write_mosaic_box(
    out: &mut String,
    index: usize,
    original: &str,
    alt: &str,
    size: Option<ImageSize>,
    in_link: bool,
) {
    let source = MosaicRequest::url_for(original, MAX_COLUMNS);
    // Inside a link the box is a span: the link keeps its meaning and nothing nests.
    let tag = if in_link { "span" } else { "a" };
    out.push('<');
    out.push_str(tag);
    out.push_str(r#" class="mosaic""#);
    if !in_link {
        out.push_str(" href=\"");
        out.push_str(TOGGLE_SCHEME);
        out.push_str(&format!(":{index}\""));
    }
    out.push_str(&format!(" id=\"m{index}\" data-original=\""));
    escape_attribute(out, original);
    out.push_str("\" data-mosaic=\"");
    escape_attribute(out, &source);
    out.push('"');
    if let Some(size) = size {
        let columns = columns_for_width(size.width, MAX_COLUMNS);
        let (_, rows) = grid(size.width, size.height, columns);
        out.push_str(&format!(
            " style=\"min-height: calc({}px * var(--font-pixel))\"",
            rows * CELL_HEIGHT
        ));
    }
    out.push_str("><img src=\"");
    escape_attribute(out, &source);
    out.push_str("\" alt=\"");
    escape_attribute(out, alt);
    out.push_str("\" loading=\"lazy\"><span class=\"mosaic-alt\">");
    out.push_str("IMAGE");
    if !alt.is_empty() {
        out.push(' ');
        escape_text(out, alt);
    }
    out.push_str("</span></");
    out.push_str(tag);
    out.push('>');
}

/// The attribute `name` of a kept `tag`, when the allowlist has it.
fn allowed_attribute(tag: &str, name: &str) -> Option<&'static str> {
    const GLOBAL: [&str; 3] = ["dir", "lang", "title"];
    let own: &[&'static str] = match tag {
        "a" => &["href"],
        "col" | "colgroup" => &["span"],
        "details" => &["open"],
        "img" => &["alt", "height", "src", "width"],
        "li" => &["value"],
        "ol" => &["reversed", "start", "type"],
        "td" => &["colspan", "rowspan"],
        "th" => &["colspan", "rowspan", "scope"],
        "time" => &["datetime"],
        _ => &[],
    };
    GLOBAL
        .iter()
        .chain(own)
        .find(|allowed| **allowed == name)
        .copied()
}

fn safe_attributes(
    element: &Element,
    tag: &str,
    base: Option<&Url>,
) -> Vec<(&'static str, String)> {
    element
        .attrs()
        .filter_map(|(name, value)| {
            let name = allowed_attribute(tag, name)?;
            let value = match name {
                "href" => safe_url(value, base, false)?,
                "src" => safe_url(value, base, true)?,
                _ => value.to_owned(),
            };
            Some((name, value))
        })
        .collect()
}

/// An http(s) address, resolved against `base`, or (`allow_image_data`) a `data:image/…` one
/// that is not SVG. Cleaned as a browser reads it: outer spaces and controls trimmed, tabs and
/// line breaks removed.
fn safe_url(raw: &str, base: Option<&Url>, allow_image_data: bool) -> Option<String> {
    let cleaned: String = raw
        .trim_matches(|c: char| c.is_whitespace() || c.is_control())
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let starts_with = |prefix: &str| {
        cleaned
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };
    if allow_image_data && starts_with("data:image/") && !starts_with("data:image/svg") {
        return Some(cleaned);
    }
    let url = match base {
        Some(base) => base.join(&cleaned).ok()?,
        None => Url::parse(&cleaned).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then(|| url.into())
}

fn escape_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn escape_attribute(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Feed content is untrusted: only these addresses reach the browser or become a base. An
/// http(s) scheme, `//`, a host right after it (`https:///nohost` has none, whatever a lenient
/// parser makes of it), and no control character anywhere.
pub fn is_web_url(text: &str) -> bool {
    let after_scheme = ["https://", "http://"].iter().find_map(|scheme| {
        text.get(..scheme.len())
            .filter(|head| head.eq_ignore_ascii_case(scheme))
            .map(|_| &text[scheme.len()..])
    });
    let Some(rest) = after_scheme else {
        return false;
    };
    if rest.is_empty()
        || rest.starts_with(['/', '\\'])
        || text.chars().any(|c| c.is_ascii_control())
    {
        return false;
    }
    Url::parse(text).is_ok_and(|url| url.host_str().is_some_and(|host| !host.is_empty()))
}

// MARK: Guards

/// Tags stripped by a linear scan, basic entities decoded, blank lines turned into paragraphs,
/// everything escaped.
pub fn plain_text_fallback(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut text = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'!' || next == b'?' || next == b'/' || next.is_ascii_alphabetic() {
                i = if bytes[i..].starts_with(b"<!--") {
                    index_after(bytes, b"-->", i + 4)
                } else {
                    index_after(bytes, b">", i + 1)
                };
                text.push(b' ');
                continue;
            }
        }
        text.push(bytes[i]);
        i += 1;
    }
    // Only ASCII-delimited runs were removed, so the text is still UTF-8.
    let decoded = String::from_utf8_lossy(&text)
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut paragraphs = Vec::new();
    for paragraph in decoded
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let mut escaped = String::from("<p>");
        escape_text(&mut escaped, paragraph);
        escaped.push_str("</p>");
        paragraphs.push(escaped);
    }
    paragraphs.join("\n")
}

/// The most attributes on one start tag. A value is skipped whole (quoted, it may hold spaces
/// and `>`), so only names count; an unquoted value is read up to a space or `>`. Linear; stops
/// as soon as a tag has more than `stopping_above` (and returns that count).
pub fn most_attributes_on_a_tag(html: &str, stopping_above: usize) -> usize {
    let bytes = html.as_bytes();
    let is_space = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\x0C' | b'\r');
    let (mut most, mut i) = (0, 0);
    while i < bytes.len() {
        if bytes[i] != b'<' || !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && !is_space(bytes[j]) && bytes[j] != b'>' && bytes[j] != b'/' {
            j += 1; // the tag name
        }
        let mut count = 0;
        while j < bytes.len() {
            match bytes[j] {
                b'>' => break,
                b'=' => {
                    j += 1;
                    while j < bytes.len() && is_space(bytes[j]) {
                        j += 1;
                    }
                    if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                        let quote = bytes[j];
                        j += 1;
                        while j < bytes.len() && bytes[j] != quote {
                            j += 1;
                        }
                        j += 1;
                    } else {
                        while j < bytes.len() && !is_space(bytes[j]) && bytes[j] != b'>' {
                            j += 1;
                        }
                    }
                }
                b if is_space(b) || b == b'/' => j += 1,
                _ => {
                    count += 1;
                    if count > stopping_above {
                        return count;
                    }
                    while j < bytes.len()
                        && !is_space(bytes[j])
                        && !matches!(bytes[j], b'>' | b'=' | b'/')
                    {
                        j += 1;
                    }
                }
            }
        }
        most = most.max(count);
        i = j + 1;
    }
    most
}

/// An image's size in CSS pixels, as its feed gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

/// The size an image's attributes give: `width` and `height`, else `width` and `height` in
/// pixels in `style`. None when neither gives both as whole pixels from 1 to 20,000 (a
/// percentage gives none). Only sizes of this very `src` count: the `data-*` sizes of WordPress
/// and Blogger describe the original upload and would stretch a thumbnail.
pub fn declared_size(attributes: &[(String, String)]) -> Option<ImageSize> {
    // Foundation's `.whitespaces`: spaces and tabs, not line breaks.
    fn is_blank(c: char) -> bool {
        c == '\t'
            || (c.is_whitespace()
                && !matches!(
                    c,
                    '\n' | '\r' | '\x0B' | '\x0C' | '\u{85}' | '\u{2028}' | '\u{2029}'
                ))
    }
    fn pixels(text: Option<&str>) -> Option<u32> {
        let text = text?.trim_matches(is_blank).to_lowercase();
        let value: i64 = text.strip_suffix("px").unwrap_or(&text).parse().ok()?;
        u32::try_from(value)
            .ok()
            .filter(|v| (1..=20_000).contains(v))
    }
    fn size(width: Option<&str>, height: Option<&str>) -> Option<ImageSize> {
        Some(ImageSize {
            width: pixels(width)?,
            height: pixels(height)?,
        })
    }
    let attribute = |name: &str| {
        attributes
            .iter()
            .rev()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    if let Some(found) = size(attribute("width"), attribute("height")) {
        return Some(found);
    }
    let style = attribute("style")?;
    let (mut width, mut height) = (None, None);
    for declaration in style.split(';') {
        // Swift's `split(separator: ":", maxSplits: 1)`: leading empty pieces do not count.
        let Some((key, value)) = declaration.trim_start_matches(':').split_once(':') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key.trim_matches(is_blank).to_lowercase().as_str() {
            "width" => width = Some(value),
            "height" => height = Some(value),
            _ => {}
        }
    }
    size(width, height)
}

fn index_after(bytes: &[u8], pattern: &[u8], from: usize) -> usize {
    let from = from.min(bytes.len());
    bytes[from..]
        .windows(pattern.len())
        .position(|w| w == pattern)
        .map_or(bytes.len(), |p| from + p + pattern.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const ARTICLE: &str = "https://blog.example.org/2026/10/post.html";

    /// The Mac's hostile input, verbatim (ReaderTests.swift:67-77).
    const HOSTILE: &str = r#"<meta http-equiv="refresh" content="0;url=https://evil.example/">
<base href="https://evil.example/">
<p onclick="steal()" style="color:red">Hello <b onmouseover="x()">world</b></p>
<script>alert(1)</script><iframe src="https://evil.example/frame"></iframe>
<object data="x.swf"></object><embed src="x.swf">
<form action="https://evil.example/"><input name="q"><button>Go</button></form>
<a href="javascript:alert(1)">bad link</a> <a href=" JaVaScRiPt:alert(2)">bad two</a>
<img src="javascript:alert(3)"><img src="https://img.example.org/a.png" onerror="x()">
<svg><script>alert(4)</script></svg><style>body { display: none }</style>"#;

    fn clean(html: &str) -> String {
        sanitize(html, Some(ARTICLE), Images::Web)
    }

    #[test]
    fn sanitizer_strips_active_content() {
        let c = clean(HOSTILE);
        for needle in [
            "<script",
            "<iframe",
            "<object",
            "<embed",
            "<form",
            "<input",
            "<button",
            "<meta",
            "<base",
            "<style",
            "<svg",
            "http-equiv",
            "onclick",
            "onmouseover",
            "onerror",
            "style=",
            "javascript:",
            "JaVaScRiPt",
            "evil.example",
            "alert(",
        ] {
            assert!(!c.contains(needle), "still contains {needle}");
        }
        assert!(c.contains("<p>Hello <b>world</b>"));
        assert!(c.contains("<a>bad link</a>"));
        assert!(c.contains(r#"<img src="https://img.example.org/a.png" loading="lazy">"#));
    }

    #[test]
    fn keeps_text_entities_and_structure() {
        let c = clean(
            "<h2>Part</h2><p>caf\u{e9} &eacute; &amp; <em>it</em></p><ul><li>one</li></ul><pre>  a\n    b</pre>",
        );
        for want in [
            "<h2>Part",
            "<p>café é &amp; <em>it</em>",
            "<li>one</li>",
            "<pre>  a\n    b",
        ] {
            assert!(c.contains(want), "{want}");
        }
    }

    #[test]
    fn resolves_relative_addresses_against_the_entry() {
        let c = clean(
            r##"<a href="/post/2">rel</a><a href="#notes">frag</a><img src="img/a.png"><a href="mailto:a@b.example">mail</a>"##,
        );
        assert!(c.contains(r#"<a href="https://blog.example.org/post/2">rel</a>"#));
        assert!(
            c.contains(r#"<a href="https://blog.example.org/2026/10/post.html#notes">frag</a>"#)
        );
        assert!(
            c.contains(r#"<img src="https://blog.example.org/2026/10/img/a.png" loading="lazy">"#)
        );
        assert!(c.contains("<a>mail</a>"));
        assert_eq!(
            sanitize(
                r#"<a href="/post/2">rel</a><img src="/a.png">"#,
                None,
                Images::Web
            ),
            "<a>rel</a>"
        );
    }

    #[test]
    fn unwraps_unknown_elements_and_filters_images() {
        let c = clean(
            r#"<font color="red">kept text</font><img alt="no source"><img src="data:image/png;base64,AAAA"><img src="data:image/svg+xml;base64,AAAA">"#,
        );
        assert!(c.contains("kept text"));
        assert!(!c.contains("<font"));
        assert!(!c.contains("no source"));
        assert!(c.contains(r#"<img src="data:image/png;base64,AAAA" loading="lazy">"#));
        assert!(!c.contains("svg"));
    }

    #[test]
    fn escapes_text_and_attributes() {
        assert!(
            clean(r#"<p title='say "hi"'>1 &lt; 2 &amp;&amp; 3 &gt; 2</p>"#)
                .contains(r#"<p title="say &quot;hi&quot;">1 &lt; 2 &amp;&amp; 3 &gt; 2"#)
        );
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let deep = "<div>".repeat(100_000) + "deep text" + &"</div>".repeat(100_000);
        let c = clean(&deep);
        assert!(c.contains("deep text") && !c.contains("<div"));
        let medium = "<blockquote>".repeat(300) + "mid text" + &"</blockquote>".repeat(300);
        assert_eq!(estimated_nesting_depth(&medium, usize::MAX), 300);
        assert_eq!(clean(&medium), "<p>mid text</p>");
        let ok = "<blockquote>".repeat(200) + "ok text" + &"</blockquote>".repeat(200);
        assert!(clean(&ok).contains("<blockquote>"));
        assert_eq!(
            estimated_nesting_depth(
                "<p>a<br>b<img src=\"x>\"><!-- <div><div> --></p>",
                usize::MAX
            ),
            1
        );
        let stray = "<div></p>".repeat(40_000) + "stray text";
        assert!(
            estimated_nesting_depth(&stray, usize::MAX) > MAX_NESTING_DEPTH
                && clean(&stray).contains("stray text")
        );
        let quote = "<p x=a'b>".to_string() + &"<div>".repeat(60_000) + "quote text'>";
        assert!(
            estimated_nesting_depth(&quote, usize::MAX) > MAX_NESTING_DEPTH
                && clean(&quote).contains("quote text")
        );
        let loose = (0..500).map(|i| format!("<p>para {i}")).collect::<String>()
            + "<ul><li>a<li>b<li>c</ul>";
        assert!(estimated_nesting_depth(&loose, usize::MAX) < 10 && clean(&loose).contains("<ul>"));
        assert_eq!(
            plain_text_fallback("<p>one &amp; two</p>\n\n<p>three &lt;x&gt;</p>"),
            "<p>one &amp; two</p>\n<p>three &lt;x&gt;</p>"
        );
        // The serialiser itself stops at the cap: a deeper node gives up the parsed path.
        assert_eq!(
            render(&("<span>".repeat(300) + "lost"), None, Images::Web),
            None
        );
        let at_cap = "<span>".repeat(255) + "kept";
        assert_eq!(
            render(&at_cap, None, Images::Web),
            Some("<span>".repeat(255) + "kept" + &"</span>".repeat(255))
        );
    }

    /// The estimate may fall a few levels short of html5ever (here four formatting elements
    /// reopened under 254 spans): the page is then the plain text, not a silently cut tree.
    #[test]
    fn nodes_past_the_cap_send_the_article_to_plain_text() {
        let html = "<p><b><i><u><s></p>".to_string() + &"<span>".repeat(254) + "deep text";
        assert!(estimated_nesting_depth(&html, MAX_NESTING_DEPTH) <= MAX_NESTING_DEPTH);
        assert_eq!(render(&html, None, Images::Web), None);
        assert_eq!(clean(&html), plain_text_fallback(&html));
        assert!(clean(&html).contains("deep text"));
    }

    #[test]
    fn encoded_and_foreign_addresses_never_survive() {
        let c = clean(concat!(
            r#"<a href="&#106;avascript:alert(1)">ref</a><a href="&#x6A;ava&#x09;script:alert(2)">tab</a>"#,
            r#"<img src="https://e.org/a.png" srcset="https://evil.example/b.png 2x">"#,
            r#"<img src="DATA:IMAGE/SVG+XML;base64,AAAA"><img src=" Data:Image/Svg+xml,<svg/>">"#,
            r#"<math><a href="https://e.org/m">in math</a></math><svg><a href="https://e.org/s">in svg</a></svg>"#,
        ));
        assert!(c.contains("<a>ref</a>") && c.contains("<a>tab</a>"), "{c}");
        assert!(
            c.contains(r#"<img src="https://e.org/a.png" loading="lazy">"#),
            "{c}"
        );
        for needle in [
            "javascript",
            "alert(",
            "srcset",
            "evil",
            "svg",
            "SVG",
            "math",
            "in math",
            "in svg",
            "e.org/m",
            "e.org/s",
        ] {
            assert!(!c.contains(needle), "{needle}: {c}");
        }
        assert_eq!(c.matches("<img").count(), 1, "{c}");
    }

    #[test]
    fn misnested_end_tags_are_bounded_by_the_size_cap_and_budget() {
        let started = Instant::now();
        assert!(!clean(&"<b><div></b>".repeat(100_000)).contains("<div"));
        assert!(started.elapsed() < Duration::from_millis(1500));
        let started = Instant::now();
        let _ = clean(&"<b><div>x</b>".repeat(15_000));
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn a_tag_with_thousands_of_attributes_is_shown_as_text() {
        let tag = |count: usize| {
            "<p ".to_string()
                + &(0..count)
                    .map(|i| format!("a{i}=x"))
                    .collect::<Vec<_>>()
                    .join(" ")
                + "><b>kept</b></p>"
        };
        let many = tag(20_000);
        assert_eq!(most_attributes_on_a_tag(&many, usize::MAX), 20_000);
        assert_eq!(most_attributes_on_a_tag(&many, 2_048), 2_049);
        let started = Instant::now();
        let c = clean(&many);
        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(!c.contains("<b>") && c.contains("kept"));
        assert!(!clean(&tag(2_049)).contains("<b>"));
        assert!(clean(&tag(2_000)).contains("<b>kept</b>"));
        assert_eq!(
            most_attributes_on_a_tag(
                r#"<p>x</p><img src="a b>c" alt='d e' width=600 data-x = "1">"#,
                usize::MAX
            ),
            4
        );
        assert_eq!(
            most_attributes_on_a_tag("<p>no attributes</p><br/>", usize::MAX),
            0
        );
    }

    #[test]
    fn articles_up_to_512_kb_are_parsed() {
        let paragraph = format!(
            "<p>{}<b>bold</b></p>\n",
            "Words of an ordinary long article. ".repeat(12)
        );
        let big = paragraph.repeat(400_000 / paragraph.len());
        assert!(big.len() > 390_000);
        assert!(
            sanitize_within(
                &format!("{big}<!-- unique -->"),
                None,
                Images::Web,
                Duration::from_secs(10)
            )
            .contains("<b>bold</b>")
        );
        let huge = paragraph.repeat(540_000 / paragraph.len());
        assert!(
            !sanitize_within(&huge, None, Images::Web, Duration::from_secs(10)).contains("<b>")
        );
    }

    /// Space reads the page's height: an image holds its place before it loads when its feed
    /// gives its size for this very src. WordPress and Blogger's data attributes describe the
    /// original upload, not the served thumbnail, so they give none.
    #[test]
    fn images_get_the_size_their_feed_gives() {
        let c = clean(concat!(
            r#"<img src="https://e.org/a.png" style="width: 600px; height:400px"><img src="https://e.org/b.png" data-orig-size="1200,800">"#,
            r#"<img src="https://e.org/c.png" data-original-width="640" data-original-height="480"><img src="https://e.org/d.png" width="50%" style="height: 10px">"#,
            r#"<img src="https://e.org/e.png" width="320"><img src="https://e.org/f.png">"#,
        ));
        assert!(c.contains(
            r#"<img src="https://e.org/a.png" width="600" height="400" loading="lazy">"#
        ));
        assert!(c.contains(r#"<img src="https://e.org/b.png" loading="lazy">"#));
        assert!(c.contains(r#"<img src="https://e.org/c.png" loading="lazy">"#));
        let attributes = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            declared_size(&attributes(&[("data-orig-size", "1200,800")])),
            None
        );
        assert_eq!(
            declared_size(&attributes(&[
                ("data-original-width", "640"),
                ("data-original-height", "480")
            ])),
            None
        );
        assert!(c.contains(r#"<img src="https://e.org/d.png" width="50%" loading="lazy">"#)); // its own attribute wins
        assert!(c.contains(r#"<img src="https://e.org/e.png" width="320" loading="lazy">"#));
        assert!(c.contains(r#"<img src="https://e.org/f.png" loading="lazy">"#));
        assert!(!c.contains("style") && !c.contains("data-"));
        assert_eq!(
            declared_size(&attributes(&[("width", "600px"), ("height", " 400 ")])),
            Some(ImageSize {
                width: 600,
                height: 400
            })
        );
        assert_eq!(
            declared_size(&attributes(&[("width", "0"), ("height", "400")])),
            None
        );
        assert_eq!(
            declared_size(&attributes(&[("style", "width:100%;height:auto")])),
            None
        );
    }

    #[test]
    fn survives_garbage() {
        assert_eq!(clean(""), "");
        assert!(clean("< not a tag > 3 < 4").contains("&lt; not a tag &gt; 3 &lt; 4"));
        assert!(!clean("<p>a\u{0}b<script>x</script>").contains("<script"));
        assert_eq!(clean("plain & text"), "plain &amp; text");
    }

    #[test]
    fn only_web_addresses_are_web_urls() {
        for good in [
            "https://example.org/a",
            "HTTP://e.org/a",
            "http://127.0.0.1:9/x",
        ] {
            assert!(is_web_url(good), "{good}");
        }
        for bad in [
            "https:///nohost",
            "https://",
            "javascript:alert(1)",
            " JaVaScRiPt:alert(2)",
            "mailto:a@b.example",
            "file:///etc/passwd",
            "ressac-toggle:3",
            "//e.org/a",
            "",
            r"https:\\e.org/a",
            "https://e.org/\na",
        ] {
            assert!(!is_web_url(bad), "{bad}");
        }
    }

    #[test]
    fn addresses_are_cleaned_like_a_browser_reads_them() {
        let c = clean(
            "<a href=\"java\tscript:alert(1)\">tab</a><img src=\"data:image/s\nvg+xml;base64,AAAA\"><a href=\" https://e.org \">sp</a>",
        );
        assert!(c.contains("<a>tab</a>") && !c.contains("svg"), "{c}");
        assert!(c.contains(r#"<a href="https://e.org/">sp</a>"#), "{c}");
    }

    /// Review Focus 1 (master Review Focus 4).
    #[test]
    fn hostile_html_is_inert_and_oversized_or_deep_input_is_plain_text() {
        let hostile = r#"<script>alert(1)</script><img src="x" onload="alert(2)" onerror="alert(3)"><a href="javascript:alert(4)">go</a><p>body</p>"#;
        let parsed = clean(hostile);
        for needle in ["<script", "onload", "onerror", "javascript:", "alert("] {
            assert!(!parsed.contains(needle), "{needle}");
        }
        let big = hostile.repeat(600 * 1024 / hostile.len() + 1);
        assert!(big.len() > 600 * 1024);
        let plain = clean(&big);
        assert!(
            plain.starts_with("<p>")
                && !plain.contains("<script")
                && !plain.contains("<a ")
                && !plain.contains("<img")
        );
        let deep = "<div>".repeat(300) + hostile + &"</div>".repeat(300);
        let plain = clean(&deep);
        assert!(!plain.contains("<div") && !plain.contains("<script") && plain.contains("body"));
    }

    #[test]
    fn the_element_lists_are_sorted_for_binary_search() {
        for list in [&KEPT[..], &DROPPED[..], &VOID[..]] {
            assert!(list.windows(2).all(|pair| pair[0] < pair[1]), "{list:?}");
        }
    }

    // MosaicMarkupTests

    mod mosaic_boxes {
        use super::*;
        use crate::mosaic::{MAX_COLUMNS, MosaicRequest};

        fn mosaics(html: &str, base: Option<&str>) -> String {
            sanitize(html, base, Images::Mosaics)
        }

        fn address(original: &str) -> String {
            MosaicRequest::url_for(original, MAX_COLUMNS).replace('&', "&amp;")
        }

        #[test]
        fn images_become_numbered_mosaic_boxes() {
            let html = r#"<p><img src="https://img.example.org/a.png" alt="A &quot;cat&quot;" width="800"></p><img src="/b.jpg"><img src="data:image/png;base64,AAAA">"#;
            let base = Some("https://blog.example.org/post.html");
            let clean = mosaics(html, base);
            let a = address("https://img.example.org/a.png");
            let b = address("https://blog.example.org/b.jpg");
            let first = format!(
                r#"<a class="mosaic" href="ressac-toggle:0" id="m0" data-original="https://img.example.org/a.png" data-mosaic="{a}"><img src="{a}" alt="A &quot;cat&quot;" loading="lazy"><span class="mosaic-alt">IMAGE A "cat"</span></a>"#
            );
            let second = format!(
                r#"<a class="mosaic" href="ressac-toggle:1" id="m1" data-original="https://blog.example.org/b.jpg" data-mosaic="{b}"><img src="{b}" alt="" loading="lazy"><span class="mosaic-alt">IMAGE</span></a>"#
            );
            assert!(clean.contains(&first), "{clean}");
            assert!(clean.contains(&second), "{clean}");
            // Inline data stays as it is.
            assert!(clean.contains(r#"<img src="data:image/png;base64,AAAA" loading="lazy">"#));
            assert!(!clean.contains("width="));
            // Without mosaics (Native) the same input keeps its plain images.
            assert!(
                sanitize(html, base, Images::Web).contains(
                    r#"<img alt="A &quot;cat&quot;" src="https://img.example.org/a.png" width="800" loading="lazy">"#
                )
            );
        }

        #[test]
        fn an_image_inside_a_link_keeps_the_link() {
            let clean = mosaics(
                r#"<a href="https://example.org/big.jpg"><img src="https://example.org/small.jpg"></a>"#,
                None,
            );
            assert!(
                clean.starts_with(
                    r#"<a href="https://example.org/big.jpg"><span class="mosaic" id="m0" data-original="https://example.org/small.jpg""#
                ),
                "{clean}"
            );
            assert!(!clean.contains("ressac-toggle"));
        }

        #[test]
        fn a_declared_size_reserves_the_mosaics_rows() {
            // 600 pixels: 40 columns; 0.6 x 40 x 400 / 600 = 16 rows of 10 font pixels. The
            // image without a size has no style.
            let clean = mosaics(
                r#"<img src="https://e.org/a.png" width="600" height="400"><img src="https://e.org/b.png">"#,
                None,
            );
            assert!(
                clean.contains(r#" style="min-height: calc(160px * var(--font-pixel))"><img src="#),
                "{clean}"
            );
            assert_eq!(clean.matches("min-height").count(), 1);
            // 120 x 120: 10 columns, 6 rows.
            let square = mosaics(
                r#"<img src="https://e.org/c.png" width="120" height="120">"#,
                None,
            );
            assert!(
                square.contains("min-height: calc(60px * var(--font-pixel))"),
                "{square}"
            );
        }

        #[test]
        fn hostile_values_stay_escaped_inside_a_box() {
            let clean = mosaics(
                r#"<img src="https://e.example/a.png?x=1&y=<b>&quot;" alt='"><script>alert(1)</script>'>"#,
                None,
            );
            assert!(!clean.contains("<script"), "{clean}");
            assert!(!clean.contains("<b>"), "{clean}");
            assert!(clean.contains("&lt;script&gt;"), "{clean}");
            for rejected in [
                r#"<img src="javascript:alert(1)">"#,
                r#"<img src="ftp://e.example/a.png">"#,
                r#"<img src="ressac-mosaic:image?cols=1&src=x">"#,
            ] {
                let clean = mosaics(rejected, None);
                assert!(!clean.contains("class=\"mosaic\""), "{rejected}: {clean}");
                assert!(!clean.contains("javascript:"), "{clean}");
            }
        }
    }
}
