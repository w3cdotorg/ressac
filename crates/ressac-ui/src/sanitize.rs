//! The reader's HTML sanitiser (the Mac's `ArticleTemplate.sanitize`). Feed and extracted HTML
//! is hostile: an allowlist of inert elements and attributes survives, written back out by our
//! own serialiser; links and images keep only http(s) addresses (images also `data:image/…`,
//! never SVG). Input too big, too deep or with too many attributes on one tag skips the parser
//! and is shown as escaped plain text; the parse runs on a large-stack worker within a budget.
//!
//! Parsing is html5ever (through `scraper`), whose work grows with the square of the nesting
//! depth, so the depth guard follows html5ever's own tree-building rules (`OpenElements`).

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use scraper::node::Element;
use scraper::{Html, Node};
use url::Url;

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

/// What becomes of images (B8d adds Antiope's mosaics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Images {
    Web,
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
        Some(rendered) => (rendered, false),
        None => (plain_text_fallback(html), true),
    }
}

/// One step of the serialiser's walk: a node to write at a depth, or an element to close.
enum Step<N> {
    Visit(N, usize),
    Close(&'static str),
}

/// Parses `html` as a body fragment and writes the allowed tree back out, without recursion;
/// nodes deeper than `MAX_NESTING_DEPTH` are dropped.
fn render(html: &str, base: Option<&Url>, images: Images) -> String {
    let Images::Web = images;
    let document = Html::parse_fragment(html);
    let mut out = String::with_capacity(html.len());
    let mut pending: Vec<_> = document
        .root_element()
        .children()
        .rev()
        .map(|child| Step::Visit(child, 1))
        .collect();
    while let Some(step) = pending.pop() {
        let (node, depth) = match step {
            Step::Close(tag) => {
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
                continue;
            }
            Step::Visit(node, depth) => (node, depth),
        };
        if depth > MAX_NESTING_DEPTH {
            continue;
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
                    .map(|child| Step::Visit(child, depth + 1)),
            );
            continue;
        };
        let tag = KEPT[kept];
        let mut attributes = safe_attributes(element, tag, base);
        if tag == "img" {
            if !attributes.iter().any(|(name, _)| *name == "src") {
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
            pending.extend(
                node.children()
                    .rev()
                    .map(|child| Step::Visit(child, depth + 1)),
            );
        }
    }
    out
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

// MARK: Memo

/// The last `MEMO_CAPACITY` results, least recently used first.
struct Memo {
    entries: Vec<MemoEntry>,
    hits: u64,
    misses: u64,
}

struct MemoEntry {
    html: String,
    base: Option<String>,
    images: Images,
    value: String,
}

static MEMO: Mutex<Memo> = Mutex::new(Memo {
    entries: Vec::new(),
    hits: 0,
    misses: 0,
});

fn memo() -> MutexGuard<'static, Memo> {
    lock(&MEMO)
}

impl Memo {
    fn position(&self, html: &str, base: Option<&str>, images: Images) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.images == images && e.base.as_deref() == base && e.html == html)
    }

    fn lookup(&mut self, html: &str, base: Option<&str>, images: Images) -> Option<String> {
        let Some(index) = self.position(html, base, images) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        let entry = self.entries.remove(index);
        let value = entry.value.clone();
        self.entries.push(entry);
        Some(value)
    }

    fn store(&mut self, html: &str, base: Option<&str>, images: Images, value: &str) {
        if let Some(index) = self.position(html, base, images) {
            self.entries.remove(index);
        } else if self.entries.len() >= MEMO_CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(MemoEntry {
            html: html.to_owned(),
            base: base.map(str::to_owned),
            images,
            value: value.to_owned(),
        });
    }
}

/// The memo's (hits, misses), for tests.
pub fn memo_stats() -> (u64, u64) {
    let memo = memo();
    (memo.hits, memo.misses)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// MARK: Large-stack workers

/// A counting semaphore: at most `slots` workers run at once, so abandoned parses never pile up.
pub struct Gate {
    free: Mutex<usize>,
    freed: Condvar,
}

impl Gate {
    pub fn new(slots: usize) -> Arc<Gate> {
        Arc::new(Gate {
            free: Mutex::new(slots),
            freed: Condvar::new(),
        })
    }

    fn enter(&self) -> Slot<'_> {
        let mut free = lock(&self.free);
        while *free == 0 {
            free = self
                .freed
                .wait(free)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *free -= 1;
        Slot(self)
    }
}

/// A taken slot, given back when dropped (also when the work panics).
struct Slot<'a>(&'a Gate);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *lock(&self.0.free) += 1;
        self.0.freed.notify_one();
    }
}

/// The gate every sanitiser parse passes through: `MAX_WORKERS` slots.
pub fn shared_gate() -> &'static Arc<Gate> {
    static GATE: OnceLock<Arc<Gate>> = OnceLock::new();
    GATE.get_or_init(|| Gate::new(MAX_WORKERS))
}

/// A worker's result, whether it is over, and whether its caller stopped waiting.
struct Job<T> {
    state: Mutex<JobState<T>>,
    over: Condvar,
}

struct JobState<T> {
    result: Option<T>,
    over: bool,
    abandoned: bool,
}

/// Marks the job over when dropped, so a panicking worker still wakes its caller.
struct Over<'a, T>(&'a Job<T>);

impl<T> Drop for Over<'_, T> {
    fn drop(&mut self) {
        lock(&self.0.state).over = true;
        self.0.over.notify_all();
    }
}

/// Runs `work` on a new thread with a `WORKER_STACK_BYTES` stack and waits up to `timeout` for
/// it. The worker first takes a slot of `gate`, and skips its work when its caller stopped
/// waiting meanwhile. A worker already running cannot be cancelled: it finishes and its late
/// result is dropped. `None` when the time runs out, the work panics or no thread starts.
pub fn on_large_stack<T: Send + 'static>(
    timeout: Duration,
    gate: &Arc<Gate>,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let job = Arc::new(Job {
        state: Mutex::new(JobState {
            result: None,
            over: false,
            abandoned: false,
        }),
        over: Condvar::new(),
    });
    let worker_job = Arc::clone(&job);
    let worker_gate = Arc::clone(gate);
    let spawned = std::thread::Builder::new()
        .name("ressac-sanitize".into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            // Declared first, dropped last: the slot is free before the caller wakes.
            let _over = Over(&worker_job);
            let _slot = worker_gate.enter();
            if !lock(&worker_job.state).abandoned {
                let result = work();
                lock(&worker_job.state).result = Some(result);
            }
        });
    if spawned.is_err() {
        return None;
    }
    let deadline = Instant::now().checked_add(timeout);
    let mut state = lock(&job.state);
    loop {
        let now = Instant::now();
        if let Some(deadline) = deadline
            && now >= deadline
        {
            state.abandoned = true;
            return None;
        }
        if state.over {
            return state.result.take();
        }
        state = match deadline {
            Some(deadline) => {
                job.over
                    .wait_timeout(state, deadline - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => job.over.wait(state).unwrap_or_else(PoisonError::into_inner),
        };
    }
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

// MARK: Depth estimate

/// html5ever's special elements (HTML namespace, sorted).
#[rustfmt::skip]
const SPECIAL: [&str; 82] = [
    "address", "applet", "area", "article", "aside", "base", "basefont", "bgsound", "blockquote",
    "body", "br", "button", "caption", "center", "col", "colgroup", "dd", "details", "dir", "div",
    "dl", "dt", "embed", "fieldset", "figcaption", "figure", "footer", "form", "frame", "frameset",
    "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html", "iframe", "img",
    "input", "isindex", "li", "link", "listing", "main", "marquee", "menu", "meta", "nav",
    "noembed", "noframes", "noscript", "object", "ol", "p", "param", "plaintext", "pre", "script",
    "section", "select", "source", "style", "summary", "table", "tbody", "td", "template",
    "textarea", "tfoot", "th", "thead", "title", "tr", "track", "ul", "wbr", "xmp",
];

/// Start tags that end foreign content (SVG, MathML) in html5ever (sorted); `font` too with a
/// `color`, `face` or `size` attribute.
#[rustfmt::skip]
const BREAKOUT: [&str; 44] = [
    "b", "big", "blockquote", "body", "br", "center", "code", "dd", "div", "dl", "dt", "em",
    "embed", "h1", "h2", "h3", "h4", "h5", "h6", "head", "hr", "i", "img", "li", "listing", "menu",
    "meta", "nobr", "ol", "p", "pre", "ruby", "s", "small", "span", "strike", "strong", "sub",
    "sup", "table", "tt", "u", "ul", "var",
];

/// Reopened formatting elements not counted: html5ever adds a few on its own, and counting them
/// would send ordinary misnested markup to plain text. With a void element on top, the
/// estimate stays within five levels of html5ever (the tests' slack).
const REOPENED_NOT_COUNTED: usize = 4;

/// Conservative, linear-time estimate of the deepest element nesting html5ever builds for
/// `html` as a body fragment: a light tokenizer (tag names, attributes, self-closing slashes,
/// comments and raw text read as html5ever reads them) drives a model of html5ever's stack of
/// open elements (optional end tags, scopes, special elements, the adoption agency, reopened
/// formatting elements, the form pointer, foreign content, and the Mac's table rules). Void
/// elements push nothing. Stops once the depth passes `stopping_above` and returns that depth.
pub fn estimated_nesting_depth(html: &str, stopping_above: usize) -> usize {
    let bytes = html.as_bytes();
    let mut tree = OpenElements::new(stopping_above);
    let mut i = 0;
    while i < bytes.len() && tree.deepest <= stopping_above {
        if bytes[i] != b'<' {
            i = bytes[i..]
                .iter()
                .position(|&b| b == b'<')
                .map_or(bytes.len(), |p| i + p);
            tree.text();
            continue;
        }
        match bytes.get(i + 1).copied() {
            Some(b'!') if bytes[i..].starts_with(b"<!--") => i = after_comment(bytes, i + 4),
            Some(b'!') if tree.in_foreign_content() && bytes[i..].starts_with(b"<![CDATA[") => {
                tree.text();
                i = index_after(bytes, b"]]>", i + 9);
            }
            // Bogus comments and doctypes end at the first `>`.
            Some(b'!' | b'?') => i = index_after(bytes, b">", i + 2),
            Some(b'/') => match bytes.get(i + 2).copied() {
                Some(b) if b.is_ascii_alphabetic() => {
                    // A tag the input ends inside is dropped, and nothing follows it.
                    let Some(tag) = read_tag(bytes, i + 2) else {
                        break;
                    };
                    tree.end(&html[i + 2..tag.name_end].to_ascii_lowercase());
                    i = tag.end;
                }
                Some(b'>') => i += 3,
                Some(_) => i = index_after(bytes, b">", i + 2),
                None => {
                    tree.text();
                    i += 2;
                }
            },
            Some(b) if b.is_ascii_alphabetic() => {
                let Some(tag) = read_tag(bytes, i + 1) else {
                    break;
                };
                let name = html[i + 1..tag.name_end].to_ascii_lowercase();
                i = tag.end;
                match tree.start(&name, &tag, bytes) {
                    Some(RawText::UntilEndTag) => i = raw_text_end(bytes, i, name.as_bytes()),
                    Some(RawText::Script) => i = script_data_end(bytes, i),
                    Some(RawText::ToTheEnd) => i = bytes.len(),
                    None => {}
                }
            }
            _ => {
                tree.text();
                i += 1;
            }
        }
    }
    tree.deepest.min(stopping_above.saturating_add(1))
}

fn is_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

/// A tag as html5ever's tokenizer reads it.
struct Tag {
    name_end: usize,
    /// Just past its `>`.
    end: usize,
    self_closing: bool,
    attributes: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}

impl Tag {
    /// Its attributes, names lowercased, the first of duplicates kept, sorted: html5ever's
    /// identity for formatting elements.
    fn attribute_set(&self, bytes: &[u8]) -> Vec<(Box<str>, Box<str>)> {
        let mut set: Vec<(Box<str>, Box<str>)> = Vec::with_capacity(self.attributes.len());
        for (name, value) in &self.attributes {
            let name = String::from_utf8_lossy(&bytes[name.clone()])
                .to_ascii_lowercase()
                .into_boxed_str();
            if !set.iter().any(|(n, _)| *n == name) {
                set.push((name, String::from_utf8_lossy(&bytes[value.clone()]).into()));
            }
        }
        set.sort();
        set
    }

    fn has_attribute(&self, bytes: &[u8], names: &[&str]) -> bool {
        self.attributes.iter().any(|(name, _)| {
            names
                .iter()
                .any(|n| bytes[name.clone()].eq_ignore_ascii_case(n.as_bytes()))
        })
    }
}

/// Reads the tag whose name starts at `start` (just after `<` or `</`), through html5ever's tag,
/// attribute and self-closing states. None when the input ends inside it.
fn read_tag(bytes: &[u8], start: usize) -> Option<Tag> {
    let mut i = start;
    while i < bytes.len() && !is_space(bytes[i]) && bytes[i] != b'/' && bytes[i] != b'>' {
        i += 1;
    }
    let name_end = i;
    let mut attributes = Vec::new();
    // The previous byte was a `/` outside any attribute value.
    let mut slash = false;
    loop {
        let b = *bytes.get(i)?;
        if std::mem::take(&mut slash) && b == b'>' {
            return Some(Tag {
                name_end,
                end: i + 1,
                self_closing: true,
                attributes,
            });
        }
        match b {
            b'>' => {
                return Some(Tag {
                    name_end,
                    end: i + 1,
                    self_closing: false,
                    attributes,
                });
            }
            b'/' => {
                slash = true;
                i += 1;
            }
            b if is_space(b) => i += 1,
            _ => {
                // An attribute name (its first character may be `=`), then maybe a value.
                let name_start = i;
                i += 1;
                while bytes
                    .get(i)
                    .is_some_and(|&c| !is_space(c) && !matches!(c, b'/' | b'>' | b'='))
                {
                    i += 1;
                }
                let name = name_start..i;
                while bytes.get(i).is_some_and(|&c| is_space(c)) {
                    i += 1;
                }
                if bytes.get(i) != Some(&b'=') {
                    attributes.push((name, i..i));
                    continue;
                }
                i += 1;
                while bytes.get(i).is_some_and(|&c| is_space(c)) {
                    i += 1;
                }
                match *bytes.get(i)? {
                    quote @ (b'"' | b'\'') => {
                        let close = i + 1 + bytes[i + 1..].iter().position(|&c| c == quote)?;
                        attributes.push((name, i + 1..close));
                        i = close + 1;
                    }
                    b'>' => attributes.push((name, i..i)),
                    _ => {
                        let value_start = i;
                        while bytes.get(i).is_some_and(|&c| !is_space(c) && c != b'>') {
                            i += 1;
                        }
                        attributes.push((name, value_start..i));
                    }
                }
            }
        }
    }
}

/// Just past the comment whose `<!--` ends before `from`: `<!-->` and `<!--->` end at once,
/// any other at the first `-->` or `--!>`.
fn after_comment(bytes: &[u8], from: usize) -> usize {
    match bytes.get(from..) {
        Some([b'>', ..]) => return from + 1,
        Some([b'-', b'>', ..]) => return from + 2,
        _ => {}
    }
    let mut i = from;
    while i + 2 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' {
            match bytes[i + 2] {
                b'>' => return i + 3,
                b'!' if bytes.get(i + 3) == Some(&b'>') => return i + 4,
                _ => {}
            }
        }
        i += 1;
    }
    bytes.len()
}

/// `</name` followed by a space, `/` or `>` at `i`, in any case.
fn is_end_tag_at(bytes: &[u8], i: usize, name: &[u8]) -> bool {
    bytes.get(i..i + 2) == Some(b"</")
        && bytes
            .get(i + 2..i + 2 + name.len())
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
        && bytes
            .get(i + 2 + name.len())
            .is_some_and(|&b| is_space(b) || b == b'/' || b == b'>')
}

/// Where the raw text of a `name` element (RCDATA or RAWTEXT) ends: its end tag, or the end.
fn raw_text_end(bytes: &[u8], from: usize, name: &[u8]) -> usize {
    (from..bytes.len())
        .find(|&i| bytes[i] == b'<' && is_end_tag_at(bytes, i, name))
        .unwrap_or(bytes.len())
}

/// Where a script's text ends, through html5ever's escaped and double-escaped script states
/// (`<!--` … `<script>` … `</script>` … `-->` stays text).
fn script_data_end(bytes: &[u8], from: usize) -> usize {
    #[derive(PartialEq)]
    enum State {
        Data,
        Escaped,
        DoubleEscaped,
    }
    let mut state = State::Data;
    let mut i = from;
    while i < bytes.len() {
        let here = &bytes[i..];
        match state {
            State::Data if here.starts_with(b"<!--") => {
                state = State::Escaped;
                i += 2; // the dashes may end it at once: `<!-->`
                continue;
            }
            State::Escaped | State::DoubleEscaped if here.starts_with(b"-->") => {
                state = State::Data;
                i += 3;
                continue;
            }
            State::Data | State::Escaped if is_end_tag_at(bytes, i, b"script") => return i,
            State::Escaped
                if here.len() > 7
                    && here[0] == b'<'
                    && here[1..7].eq_ignore_ascii_case(b"script")
                    && (is_space(here[7]) || matches!(here[7], b'/' | b'>')) =>
            {
                state = State::DoubleEscaped;
                i += 7;
                continue;
            }
            State::DoubleEscaped if is_end_tag_at(bytes, i, b"script") => {
                state = State::Escaped;
                i += 8;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// What the tokenizer reads after a start tag.
enum RawText {
    /// Text up to the element's own end tag (`textarea`, `title`, `style`, `xmp`…).
    UntilEndTag,
    Script,
    /// `plaintext`: text to the end of the input.
    ToTheEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Namespace {
    Html,
    Svg,
    MathMl,
}

/// An element on the model's stack. `id`s are never reused.
struct Open {
    name: Box<str>,
    namespace: Namespace,
    id: usize,
    reopened: bool,
    /// Ancestors just below it that left the stack but not the tree (`detach`).
    hidden: usize,
}

impl Open {
    fn is_html(&self, name: &str) -> bool {
        self.namespace == Namespace::Html && &*self.name == name
    }

    fn is_special(&self) -> bool {
        self.namespace == Namespace::Html && SPECIAL.binary_search(&&*self.name).is_ok()
    }

    /// Where start tags and text are read as HTML again.
    fn is_integration_point(&self) -> bool {
        match self.namespace {
            Namespace::Html => false,
            Namespace::Svg => matches!(&*self.name, "foreignobject" | "desc" | "title"),
            Namespace::MathMl => matches!(&*self.name, "mi" | "mo" | "mn" | "ms" | "mtext"),
        }
    }

    fn bounds(&self, scope: Scope) -> bool {
        match (self.namespace, scope) {
            (Namespace::Html, Scope::Table) => matches!(&*self.name, "html" | "table" | "template"),
            (Namespace::Html, _) => {
                matches!(
                    &*self.name,
                    "applet"
                        | "caption"
                        | "html"
                        | "marquee"
                        | "object"
                        | "select"
                        | "table"
                        | "td"
                        | "template"
                        | "th"
                ) || (scope == Scope::ListItem && matches!(&*self.name, "ol" | "ul"))
                    || (scope == Scope::Button && &*self.name == "button")
            }
            (_, Scope::Table) => false,
            _ => self.is_integration_point(),
        }
    }
}

/// What a template holds, decided by its first start tag (html5ever's template insertion
/// modes): a table's parts (in table), columns, rows (in table body), cells (in row), or a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateContent {
    Undecided,
    Table,
    Columns,
    Rows,
    Cells,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Default,
    ListItem,
    Button,
    Table,
}

/// An entry of html5ever's list of active formatting elements.
enum Formatting {
    Marker,
    Element {
        id: usize,
        name: Box<str>,
        attributes: Vec<(Box<str>, Box<str>)>,
    },
}

/// A model of html5ever's stack of open elements and list of active formatting elements, in
/// the "in body" insertion mode, with the Mac's rules for tables.
struct OpenElements {
    stack: Vec<Open>,
    /// Open elements by name, so a stray end tag costs nothing.
    open: HashMap<Box<str>, usize>,
    on_stack: Vec<bool>,
    in_formatting: Vec<bool>,
    formatting: Vec<Formatting>,
    /// Open templates (element id) and what they hold, innermost last.
    templates: Vec<(usize, TemplateContent)>,
    form: Option<usize>,
    reopened: usize,
    hidden: usize,
    deepest: usize,
    limit: usize,
}

impl OpenElements {
    fn new(limit: usize) -> OpenElements {
        OpenElements {
            stack: Vec::new(),
            open: HashMap::new(),
            on_stack: Vec::new(),
            in_formatting: Vec::new(),
            formatting: Vec::new(),
            templates: Vec::new(),
            form: None,
            reopened: 0,
            hidden: 0,
            deepest: 0,
            limit,
        }
    }

    fn past_limit(&self) -> bool {
        self.deepest > self.limit
    }

    fn new_id(&mut self) -> usize {
        self.on_stack.push(false);
        self.in_formatting.push(false);
        self.on_stack.len() - 1
    }

    fn insert(&mut self, index: usize, name: &str, namespace: Namespace, reopened: bool) -> usize {
        let id = self.new_id();
        self.on_stack[id] = true;
        *self.open.entry(name.into()).or_default() += 1;
        self.reopened += usize::from(reopened);
        self.stack.insert(
            index,
            Open {
                name: name.into(),
                namespace,
                id,
                reopened,
                hidden: 0,
            },
        );
        let depth = self.stack.len() + self.hidden - self.reopened.min(REOPENED_NOT_COUNTED);
        self.deepest = self.deepest.max(depth);
        id
    }

    fn push(&mut self, name: &str) -> usize {
        self.insert(self.stack.len(), name, Namespace::Html, false)
    }

    fn forget(&mut self, open: Open) {
        if let Some(count) = self.open.get_mut(&open.name) {
            *count -= 1;
        }
        self.on_stack[open.id] = false;
        self.reopened -= usize::from(open.reopened);
        self.hidden -= open.hidden;
        if open.namespace == Namespace::Html
            && matches!(
                &*open.name,
                "applet" | "caption" | "marquee" | "object" | "td" | "template" | "th"
            )
        {
            self.clear_formatting_to_marker();
        }
        if self.templates.last().is_some_and(|&(id, _)| id == open.id) {
            self.templates.pop();
        }
    }

    fn pop(&mut self) {
        if let Some(open) = self.stack.pop() {
            self.forget(open);
        }
    }

    /// Pops down to and including the element at `index`.
    fn truncate(&mut self, index: usize) {
        while self.stack.len() > index {
            self.pop();
        }
    }

    /// Takes the element at `index` out of the stack and of the path to the elements above it
    /// (the adoption agency moves them).
    fn remove(&mut self, index: usize) {
        let open = self.stack.remove(index);
        self.forget(open);
    }

    /// Takes the element at `index` out of the stack only: it stays the ancestor of the
    /// elements above it, so the depth keeps it.
    fn detach(&mut self, index: usize) {
        let open = self.stack.remove(index);
        let carried = open.hidden + 1;
        self.forget(open);
        if let Some(above) = self.stack.get_mut(index) {
            above.hidden += carried;
            self.hidden += carried;
        }
    }

    fn is_open(&self, name: &str) -> bool {
        self.open.get(name).is_some_and(|&count| count > 0)
    }

    fn top(&self) -> Option<&Open> {
        self.stack.last()
    }

    fn top_is_html(&self, names: &[&str]) -> bool {
        self.top()
            .is_some_and(|top| top.namespace == Namespace::Html && names.contains(&&*top.name))
    }

    fn index_of(&self, id: usize) -> Option<usize> {
        if !self.on_stack[id] {
            return None;
        }
        self.stack.iter().rposition(|open| open.id == id)
    }

    /// The topmost HTML element `wanted` picks, when no `scope` boundary lies above it.
    fn in_scope(&self, scope: Scope, wanted: impl Fn(&Open) -> bool) -> Option<usize> {
        for (index, open) in self.stack.iter().enumerate().rev() {
            if open.namespace == Namespace::Html && wanted(open) {
                return Some(index);
            }
            if open.bounds(scope) {
                return None;
            }
        }
        None
    }

    fn named_in_scope(&self, name: &str, scope: Scope) -> Option<usize> {
        if !self.is_open(name) {
            return None;
        }
        self.in_scope(scope, |open| &*open.name == name)
    }

    fn generate_implied_end_tags(&mut self, except: Option<&str>) {
        const IMPLIED: [&str; 10] = [
            "dd", "dt", "li", "optgroup", "option", "p", "rb", "rp", "rt", "rtc",
        ];
        while self.top().is_some_and(|top| {
            top.namespace == Namespace::Html
                && IMPLIED.contains(&&*top.name)
                && Some(&*top.name) != except
        }) {
            self.pop();
        }
    }

    fn close_p_in_button_scope(&mut self) {
        if let Some(index) = self.named_in_scope("p", Scope::Button) {
            self.truncate(index);
        }
    }

    /// The adjusted current node is foreign: `<![CDATA[` is a section, not a bogus comment.
    fn in_foreign_content(&self) -> bool {
        self.top()
            .is_some_and(|top| top.namespace != Namespace::Html)
    }

    fn text(&mut self) {
        if self
            .top()
            .is_some_and(|top| top.namespace != Namespace::Html && !top.is_integration_point())
        {
            return;
        }
        self.reconstruct();
    }

    fn start(&mut self, name: &str, tag: &Tag, bytes: &[u8]) -> Option<RawText> {
        if let Some(top) = self.top()
            && top.namespace != Namespace::Html
        {
            let read_as_html = match top.namespace {
                Namespace::MathMl if &*top.name == "annotation-xml" => name == "svg",
                Namespace::MathMl => {
                    top.is_integration_point() && !matches!(name, "mglyph" | "malignmark")
                }
                _ => top.is_integration_point(),
            };
            if !read_as_html {
                let breakout = BREAKOUT.binary_search(&name).is_ok()
                    || (name == "font" && tag.has_attribute(bytes, &["color", "face", "size"]));
                if !breakout {
                    let namespace = top.namespace;
                    if !tag.self_closing {
                        self.insert(self.stack.len(), name, namespace, false);
                    }
                    return None;
                }
                while self.top().is_some_and(|top| {
                    top.namespace != Namespace::Html && !top.is_integration_point()
                }) {
                    self.pop();
                }
            }
        }
        self.start_html(name, tag, bytes)
    }

    fn start_html(&mut self, name: &str, tag: &Tag, bytes: &[u8]) -> Option<RawText> {
        if let Some((id, content)) = self.templates.last_mut()
            && *content == TemplateContent::Undecided
            && self.stack.last().is_some_and(|top| top.id == *id)
        {
            *content = match name {
                "caption" | "colgroup" | "tbody" | "tfoot" | "thead" => TemplateContent::Table,
                "col" => TemplateContent::Columns,
                "tr" => TemplateContent::Rows,
                "td" | "th" => TemplateContent::Cells,
                "base" | "basefont" | "bgsound" | "link" | "meta" | "noframes" | "script"
                | "style" | "template" | "title" => TemplateContent::Undecided,
                _ => TemplateContent::Body,
            };
        }
        // A template of columns ignores anything but columns and templates.
        if !matches!(name, "col" | "template")
            && self
                .top()
                .zip(self.templates.last())
                .is_some_and(|(top, &(id, content))| {
                    top.id == id && content == TemplateContent::Columns
                })
        {
            return None;
        }
        match name {
            // Ignored in a body, or inserted and popped at once without reopening anything.
            "html" | "body" | "head" | "frameset" | "frame" | "base" | "basefont" | "bgsound"
            | "link" | "meta" | "param" | "source" | "track" => {}
            "caption" | "col" | "colgroup" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr" => {
                self.table_part(name)
            }
            "area" | "br" | "embed" | "img" | "image" | "keygen" | "wbr" => self.reconstruct(),
            "input" => {
                if let Some(index) = self.named_in_scope("select", Scope::Default) {
                    self.truncate(index);
                }
                self.reconstruct();
            }
            "hr" => self.close_p_in_button_scope(),
            "address" | "article" | "aside" | "blockquote" | "center" | "details" | "dialog"
            | "dir" | "div" | "dl" | "fieldset" | "figcaption" | "figure" | "footer" | "header"
            | "hgroup" | "listing" | "main" | "menu" | "nav" | "ol" | "p" | "pre" | "search"
            | "section" | "summary" | "table" | "ul" => {
                self.close_p_in_button_scope();
                self.push(name);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.close_p_in_button_scope();
                if self.top_is_html(&["h1", "h2", "h3", "h4", "h5", "h6"]) {
                    self.pop();
                }
                self.push(name);
            }
            "form" => {
                let in_template = self.is_open("template");
                if self.form.is_some() && !in_template {
                    return None;
                }
                self.close_p_in_button_scope();
                let id = self.push(name);
                if !in_template {
                    self.form = Some(id);
                }
            }
            "li" | "dd" | "dt" => self.list_item(name),
            "plaintext" => {
                self.close_p_in_button_scope();
                self.push(name);
                return Some(RawText::ToTheEnd);
            }
            "button" => {
                if let Some(index) = self.named_in_scope("button", Scope::Default) {
                    self.truncate(index);
                }
                self.reconstruct();
                self.push(name);
            }
            "a" => {
                if let Some(id) = self.formatting_named("a").map(|(_, id)| id) {
                    self.adoption_agency("a");
                    if let Some(index) = self.formatting_index(id) {
                        self.remove_formatting(index);
                    }
                    if let Some(index) = self.index_of(id) {
                        self.detach(index);
                    }
                }
                self.reconstruct();
                self.create_formatting(name, tag.attribute_set(bytes));
            }
            "b" | "big" | "code" | "em" | "font" | "i" | "s" | "small" | "strike" | "strong"
            | "tt" | "u" => {
                self.reconstruct();
                self.create_formatting(name, tag.attribute_set(bytes));
            }
            "nobr" => {
                self.reconstruct();
                if self.named_in_scope("nobr", Scope::Default).is_some() {
                    self.adoption_agency("nobr");
                    self.reconstruct();
                }
                self.create_formatting(name, tag.attribute_set(bytes));
            }
            "applet" | "marquee" | "object" => {
                self.reconstruct();
                self.push(name);
                self.formatting.push(Formatting::Marker);
            }
            "select" => match self.named_in_scope("select", Scope::Default) {
                Some(index) => self.truncate(index),
                None => {
                    self.reconstruct();
                    self.push(name);
                }
            },
            "option" | "optgroup" => {
                if self.named_in_scope("select", Scope::Default).is_some() {
                    self.generate_implied_end_tags((name == "option").then_some("optgroup"));
                } else if self.top_is_html(&["option"]) {
                    self.pop();
                }
                self.reconstruct();
                self.push(name);
            }
            "rb" | "rtc" | "rp" | "rt" => {
                if self.named_in_scope("ruby", Scope::Default).is_some() {
                    self.generate_implied_end_tags(matches!(name, "rp" | "rt").then_some("rtc"));
                }
                self.push(name);
            }
            "math" | "svg" => {
                self.reconstruct();
                if !tag.self_closing {
                    let namespace = if name == "svg" {
                        Namespace::Svg
                    } else {
                        Namespace::MathMl
                    };
                    self.insert(self.stack.len(), name, namespace, false);
                }
            }
            "textarea" | "title" | "style" | "noframes" | "iframe" | "noembed" | "noscript" => {
                self.push(name);
                return Some(RawText::UntilEndTag);
            }
            "xmp" => {
                self.close_p_in_button_scope();
                self.reconstruct();
                self.push(name);
                return Some(RawText::UntilEndTag);
            }
            "script" => {
                self.push(name);
                return Some(RawText::Script);
            }
            "template" => {
                let id = self.push(name);
                self.templates.push((id, TemplateContent::Undecided));
                self.formatting.push(Formatting::Marker);
            }
            _ => {
                self.reconstruct();
                self.push(name);
            }
        }
        None
    }

    /// The Mac's table rules: table parts are ignored outside a table (here, or a template
    /// whose content starts with one); `td`/`th` close an open cell, `tr` an open row; under a
    /// `table` a row gets its `tbody`, a cell its `tbody` and `tr`. Plus html5ever's clearing of
    /// foster-parented content, closing of captions and implied column groups.
    fn table_part(&mut self, name: &str) {
        let table = self.stack.iter().rposition(|open| open.is_html("table"));
        let template = self
            .templates
            .last()
            .and_then(|&(id, content)| Some((self.index_of(id)?, content)));
        let in_table = match (table, template) {
            (Some(table), Some((template, content))) if template > table => {
                content != TemplateContent::Body
            }
            (Some(_), _) => true,
            (None, Some((_, content))) => content != TemplateContent::Body,
            (None, None) => false,
        };
        if !in_table {
            return;
        }
        // Content foster-parented out of a table, body, row or column group is closed first
        // (html5ever clears the stack back to that context); a cell's or caption's stays.
        let context = self.stack.iter().rposition(|open| {
            open.namespace == Namespace::Html
                && matches!(
                    &*open.name,
                    "caption"
                        | "colgroup"
                        | "table"
                        | "tbody"
                        | "td"
                        | "template"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                )
        });
        let cell = matches!(name, "td" | "th");
        if let Some(context) = context {
            // A template of rows takes only rows and cells, one of cells only cells.
            let template = self
                .templates
                .last()
                .filter(|&&(id, _)| id == self.stack[context].id);
            match template.map(|&(_, content)| content) {
                Some(TemplateContent::Rows) if !cell && name != "tr" => return,
                Some(TemplateContent::Cells) if !cell => return,
                _ => {}
            }
            // In a body, a table's other parts need a `table`, `tbody` or `tfoot` in table scope
            // (html5ever's set: no `thead`).
            if matches!(&*self.stack[context].name, "tbody" | "thead" | "tfoot")
                && !cell
                && name != "tr"
                && self
                    .in_scope(Scope::Table, |open| {
                        matches!(&*open.name, "table" | "tbody" | "tfoot")
                    })
                    .is_none()
            {
                return;
            }
            match &*self.stack[context].name {
                // A cell keeps its content open (the Mac's rules, below).
                "td" | "th" => {}
                // Any table part closes a caption.
                "caption" => self.truncate(context),
                _ => self.truncate(context + 1),
            }
        }
        // A column group holds only columns.
        if name != "col" && self.top_is_html(&["colgroup"]) {
            self.pop();
        }
        if cell && self.top_is_html(&["td", "th"]) {
            self.pop();
        } else if name == "tr" {
            while self.top_is_html(&["td", "th", "tr"]) {
                self.pop();
            }
        }
        let template_on_top = |tree: &Self, content: TemplateContent| {
            tree.top()
                .zip(tree.templates.last())
                .is_some_and(|(top, &(id, c))| top.id == id && c == content)
        };
        if (cell || name == "tr")
            && (self.top_is_html(&["table"]) || template_on_top(self, TemplateContent::Table))
        {
            self.push("tbody");
        }
        if cell
            && (self.top_is_html(&["tbody", "thead", "tfoot"])
                || template_on_top(self, TemplateContent::Rows))
        {
            self.push("tr");
        }
        // A column under a table opens the column group it implies.
        if name == "col"
            && (self.top_is_html(&["table"]) || template_on_top(self, TemplateContent::Table))
        {
            self.push("colgroup");
        }
        if name != "col" {
            self.push(name);
            if matches!(name, "td" | "th" | "caption") {
                self.formatting.push(Formatting::Marker);
            }
        }
    }

    /// `li`, `dd` and `dt` close the open one of their kind unless a special element other than
    /// `address`, `div` or `p` lies above it.
    fn list_item(&mut self, name: &str) {
        let list = name == "li";
        for index in (0..self.stack.len()).rev() {
            let open = &self.stack[index];
            let closes = open.namespace == Namespace::Html
                && if list {
                    &*open.name == "li"
                } else {
                    matches!(&*open.name, "dd" | "dt")
                };
            if closes {
                self.truncate(index);
                break;
            }
            if open.is_special() && !matches!(&*open.name, "address" | "div" | "p") {
                break;
            }
        }
        self.close_p_in_button_scope();
        self.push(name);
    }

    fn end(&mut self, name: &str) {
        if self.in_foreign_content() {
            if matches!(name, "br" | "p") {
                while self.top().is_some_and(|top| {
                    top.namespace != Namespace::Html && !top.is_integration_point()
                }) {
                    self.pop();
                }
            } else {
                // Down through foreign elements to the first of that name; an HTML element on the
                // way hands the tag to the HTML rules.
                for index in (0..self.stack.len()).rev() {
                    let open = &self.stack[index];
                    if index + 1 != self.stack.len() && open.namespace == Namespace::Html {
                        break;
                    }
                    if &*open.name == name {
                        self.truncate(index);
                        return;
                    }
                    if index == 0 {
                        return;
                    }
                }
            }
        }
        match name {
            "html" | "body" | "head" => {}
            "br" => self.reconstruct(),
            "p" => self.close_p_in_button_scope(),
            "li" => self.close_in_scope(name, Scope::ListItem),
            "dd" | "dt" => self.close_in_scope(name, Scope::Default),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let heading =
                    |open: &Open| matches!(&*open.name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
                if let Some(index) = self.in_scope(Scope::Default, heading) {
                    self.truncate(index);
                }
            }
            "a" | "b" | "big" | "code" | "em" | "font" | "i" | "nobr" | "s" | "small"
            | "strike" | "strong" | "tt" | "u" => self.adoption_agency(name),
            "form" => self.end_form(),
            "template" => {
                if let Some(index) = self.stack.iter().rposition(|open| open.is_html("template")) {
                    self.truncate(index);
                }
            }
            "colgroup" => {
                if self.top_is_html(&["colgroup"]) {
                    self.pop();
                }
            }
            "caption" | "table" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr" => {
                self.close_table_part(name)
            }
            "address" | "applet" | "article" | "aside" | "blockquote" | "button" | "center"
            | "details" | "dialog" | "dir" | "div" | "dl" | "fieldset" | "figcaption"
            | "figure" | "footer" | "header" | "hgroup" | "listing" | "main" | "marquee"
            | "menu" | "nav" | "object" | "ol" | "pre" | "search" | "section" | "select"
            | "summary" | "ul" => self.close_in_scope(name, Scope::Default),
            _ => self.any_other_end_tag(name),
        }
    }

    /// A table part's end tag pops only through what html5ever pops through in the mode that
    /// part sets: a row or a body through rows and bodies up to a cell, then the cell's
    /// content; a cell through its content. Anything else above (a caption, content
    /// foster-parented out of a row) means html5ever is in another mode: the tag is ignored.
    fn close_table_part(&mut self, name: &str) {
        let Some(index) = self.named_in_scope(name, Scope::Table) else {
            return;
        };
        let structure = |open: &Open, names: &[&str]| {
            open.namespace == Namespace::Html && names.contains(&&*open.name)
        };
        let closes = match name {
            "td" | "th" => !self.stack[index + 1..].iter().any(|open| {
                structure(
                    open,
                    &[
                        "caption", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr",
                    ],
                )
            }),
            "tr" | "tbody" | "thead" | "tfoot" => self.stack[index + 1..]
                .iter()
                .take_while(|open| !structure(open, &["td", "th"]))
                .all(|open| structure(open, &["tbody", "tfoot", "thead", "tr"])),
            _ => true,
        };
        if closes {
            self.truncate(index);
        }
    }

    fn close_in_scope(&mut self, name: &str, scope: Scope) {
        if let Some(index) = self.named_in_scope(name, scope) {
            self.truncate(index);
        }
    }

    /// Pops down to the topmost element of that name, unless a special element lies above it.
    fn any_other_end_tag(&mut self, name: &str) {
        if !self.is_open(name) {
            return;
        }
        for index in (0..self.stack.len()).rev() {
            if self.stack[index].is_html(name) {
                self.truncate(index);
                return;
            }
            if self.stack[index].is_special() {
                return;
            }
        }
    }

    /// `</form>` takes the form pointer and removes that element alone: what it holds stays open.
    fn end_form(&mut self) {
        if self.is_open("template") {
            self.close_in_scope("form", Scope::Default);
            return;
        }
        let Some(form) = self.form.take() else { return };
        let Some(index) = self.index_of(form) else {
            return;
        };
        if self.stack[index + 1..]
            .iter()
            .any(|open| open.bounds(Scope::Default))
        {
            return;
        }
        self.generate_implied_end_tags(None);
        if let Some(index) = self.index_of(form) {
            self.detach(index);
        }
    }

    // Active formatting elements.

    fn after_last_marker(&self) -> usize {
        self.formatting
            .iter()
            .rposition(|entry| matches!(entry, Formatting::Marker))
            .map_or(0, |index| index + 1)
    }

    /// The last entry named `name` after the last marker: (index, element id).
    fn formatting_named(&self, name: &str) -> Option<(usize, usize)> {
        let start = self.after_last_marker();
        (start..self.formatting.len())
            .rev()
            .find_map(|index| match &self.formatting[index] {
                Formatting::Element { id, name: n, .. } if &**n == name => Some((index, *id)),
                _ => None,
            })
    }

    fn formatting_index(&self, id: usize) -> Option<usize> {
        if !self.in_formatting[id] {
            return None;
        }
        self.formatting
            .iter()
            .rposition(|entry| matches!(entry, Formatting::Element { id: e, .. } if *e == id))
    }

    fn remove_formatting(&mut self, index: usize) {
        if let Formatting::Element { id, .. } = self.formatting.remove(index) {
            self.in_formatting[id] = false;
        }
    }

    fn set_formatting_id(&mut self, index: usize, new: usize) {
        if let Formatting::Element { id, .. } = &mut self.formatting[index] {
            self.in_formatting[*id] = false;
            *id = new;
            self.in_formatting[new] = true;
        }
    }

    fn clear_formatting_to_marker(&mut self) {
        while let Some(entry) = self.formatting.pop() {
            match entry {
                Formatting::Marker => break,
                Formatting::Element { id, .. } => self.in_formatting[id] = false,
            }
        }
    }

    /// Pushes a formatting element; a fourth identical one after the last marker (same name and
    /// attributes) drops the earliest from the list (the "Noah's Ark" clause).
    fn create_formatting(&mut self, name: &str, attributes: Vec<(Box<str>, Box<str>)>) {
        let start = self.after_last_marker();
        let identical = |entry: &Formatting| match entry {
            Formatting::Element {
                name: n,
                attributes: a,
                ..
            } => &**n == name && *a == attributes,
            Formatting::Marker => false,
        };
        let same: Vec<usize> = (start..self.formatting.len())
            .filter(|&index| identical(&self.formatting[index]))
            .collect();
        if same.len() >= 3 {
            self.remove_formatting(same[0]);
        }
        let id = self.push(name);
        self.formatting.push(Formatting::Element {
            id,
            name: name.into(),
            attributes,
        });
        self.in_formatting[id] = true;
    }

    /// Reopens the formatting elements closed since the last marker, as html5ever does before
    /// most start tags and text.
    fn reconstruct(&mut self) {
        let is_open_or_marker = |entry: &Formatting, on_stack: &[bool]| match entry {
            Formatting::Marker => true,
            Formatting::Element { id, .. } => on_stack[*id],
        };
        match self.formatting.last() {
            Some(last) if !is_open_or_marker(last, &self.on_stack) => {}
            _ => return,
        }
        let mut first = self.formatting.len() - 1;
        while first > 0 && !is_open_or_marker(&self.formatting[first - 1], &self.on_stack) {
            first -= 1;
        }
        for index in first..self.formatting.len() {
            let Formatting::Element { name, .. } = &self.formatting[index] else {
                continue;
            };
            let name = name.clone();
            let id = self.insert(self.stack.len(), &name, Namespace::Html, true);
            self.set_formatting_id(index, id);
            if self.past_limit() {
                return;
            }
        }
    }

    /// html5ever's adoption agency algorithm for the end tag `subject`, on the stack and the list.
    fn adoption_agency(&mut self, subject: &str) {
        if let Some(top) = self.top()
            && top.is_html(subject)
            && !self.in_formatting[top.id]
        {
            self.pop();
            return;
        }
        for _ in 0..8 {
            let Some((entry, element)) = self.formatting_named(subject) else {
                self.any_other_end_tag(subject);
                return;
            };
            let Some(element_index) = self.index_of(element) else {
                self.remove_formatting(entry);
                return;
            };
            if self.stack[element_index + 1..]
                .iter()
                .any(|open| open.bounds(Scope::Default))
            {
                return;
            }
            let Some(block_index) =
                (element_index + 1..self.stack.len()).find(|&i| self.stack[i].is_special())
            else {
                self.truncate(element_index);
                self.remove_formatting(entry);
                return;
            };
            let block = self.stack[block_index].id;
            // `Some(id)`: the new element goes after that entry, not in the formatting element's.
            let mut bookmark = None;
            let mut node_index = block_index;
            let mut last = block;
            let mut counter = 0;
            loop {
                counter += 1;
                node_index -= 1;
                let node = self.stack[node_index].id;
                if node == element {
                    break;
                }
                let position = self.formatting_index(node);
                if counter > 3 {
                    if let Some(position) = position {
                        self.remove_formatting(position);
                    }
                    self.remove(node_index);
                    continue;
                }
                let Some(position) = position else {
                    self.remove(node_index);
                    continue;
                };
                // The node is replaced by a new element of the same name.
                let new = self.new_id();
                let old = std::mem::replace(&mut self.stack[node_index].id, new);
                self.on_stack[old] = false;
                self.on_stack[new] = true;
                self.set_formatting_id(position, new);
                if last == block {
                    bookmark = Some(new);
                }
                last = new;
            }
            // The formatting element's entry goes to its new element.
            let Some(entry) = self.formatting_index(element) else {
                return;
            };
            let new_entry = match &self.formatting[entry] {
                Formatting::Element {
                    name, attributes, ..
                } => (name.clone(), attributes.clone()),
                Formatting::Marker => return,
            };
            let Some(element_index) = self.index_of(element) else {
                return;
            };
            self.remove(element_index);
            let Some(block_index) = self.index_of(block) else {
                return;
            };
            let new = self.insert(block_index + 1, subject, Namespace::Html, false);
            match bookmark.and_then(|after| self.formatting_index(after)) {
                Some(after) => {
                    self.formatting.insert(
                        after + 1,
                        Formatting::Element {
                            id: new,
                            name: new_entry.0,
                            attributes: new_entry.1,
                        },
                    );
                    self.in_formatting[new] = true;
                    if let Some(old) = self.formatting_index(element) {
                        self.remove_formatting(old);
                    }
                }
                None => self.set_formatting_id(entry, new),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
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

    /// The deepest element nesting html5ever builds for `html`, below the fragment's `html`
    /// root, walked without recursion (template contents included).
    fn html5ever_depth(html: &str) -> usize {
        let document = Html::parse_fragment(html);
        let mut deepest = 0;
        let mut pending = vec![(*document.root_element(), 0)];
        while let Some((node, depth)) = pending.pop() {
            for child in node.children() {
                match child.value() {
                    Node::Element(_) => {
                        deepest = deepest.max(depth + 1);
                        pending.push((child, depth + 1));
                    }
                    Node::Fragment => pending.push((child, depth)),
                    _ => {}
                }
            }
        }
        deepest
    }

    /// A xorshift generator, so the soups are the same on every run.
    struct Xorshift(u64);

    impl Xorshift {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }
    }

    const TABLE_TAGS: [&str; 10] = [
        "table", "caption", "colgroup", "col", "thead", "tbody", "tfoot", "tr", "td", "th",
    ];

    /// `count` seeded tag soups: tags from the kept, dropped and table lists, opened, closed
    /// or separated by text at random.
    fn tag_soups(count: usize, seed: u64) -> Vec<String> {
        let mut random = Xorshift(seed);
        let tags: Vec<&str> = KEPT
            .iter()
            .chain(DROPPED.iter())
            .chain(TABLE_TAGS.iter())
            .copied()
            .collect();
        (0..count)
            .map(|_| {
                let mut soup = String::new();
                for _ in 0..10 + random.below(50) {
                    let tag = tags[random.below(tags.len())];
                    match random.below(3) {
                        0 => soup += &format!("<{tag}>"),
                        1 => soup += &format!("</{tag}>"),
                        _ => soup.push('t'),
                    }
                }
                soup
            })
            .collect()
    }

    /// `count` seeded markup soups for the tokenizer: fragments of tags, attributes, quotes,
    /// comments, raw text, foreign content and self-closing slashes.
    fn markup_soups(count: usize, seed: u64) -> Vec<String> {
        const PIECES: [&str; 48] = [
            "<",
            ">",
            "/",
            "/>",
            "</",
            "<!",
            "<!--",
            "-->",
            "--!>",
            "<!-->",
            "<?",
            " ",
            "=",
            "\"",
            "'",
            "x",
            "div",
            "b",
            "i",
            "a",
            "p",
            "li",
            "ul",
            "dd",
            "h1",
            "h2",
            "span",
            "form",
            "table",
            "tr",
            "td",
            "select",
            "option",
            "button",
            "object",
            "template",
            "script",
            "style",
            "title",
            "textarea",
            "svg",
            "math",
            "mi",
            "foreignObject",
            "desc",
            "font color=red",
            "body",
            "nobr",
        ];
        let mut random = Xorshift(seed);
        (0..count)
            .map(|_| {
                (0..20 + random.below(120))
                    .map(|_| PIECES[random.below(PIECES.len())])
                    .collect()
            })
            .collect()
    }

    /// `count` seeded short runs of 2 to 7 tokens (tags, text) from both soups, each repeated
    /// 60 times: a model that misses one level per repeat shows here.
    fn repeated_units(count: usize, seed: u64) -> Vec<String> {
        let mut random = Xorshift(seed);
        tag_soups(count / 2, seed ^ 0x55)
            .into_iter()
            .chain(markup_soups(count - count / 2, seed ^ 0xAA))
            .map(|soup| {
                let mut unit = String::new();
                let mut rest = soup.as_str();
                for _ in 0..2 + random.below(6) {
                    if rest.is_empty() {
                        break;
                    }
                    let end = match rest.strip_prefix('<') {
                        Some(tail) => tail.find('>').map_or(rest.len(), |i| i + 2),
                        None => rest.find('<').unwrap_or(rest.len()),
                    };
                    unit.push_str(&rest[..end]);
                    rest = &rest[end..];
                }
                unit.repeat(60)
            })
            .collect()
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
        // The serialiser itself stops at the cap: deeper nodes are dropped, not recursed into.
        assert_eq!(
            render(&("<span>".repeat(300) + "lost"), None, Images::Web),
            "<span>".repeat(256) + &"</span>".repeat(256)
        );
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
    fn inline_end_tags_that_cannot_close_a_block_count_as_depth() {
        // html5ever's adoption agency moves the `div` out of the `b` and closes the `b`: one level
        // deeper each time (measured: 1,001 levels for 1,000 repeats), not the Mac's two (libxml).
        let html = "<b><div>x</b>".repeat(5_000);
        assert_eq!(estimated_nesting_depth(&html, usize::MAX), 5_001);
        let started = Instant::now();
        let c = clean(&html);
        assert!(
            started.elapsed() < Duration::from_millis(200)
                && !c.contains("<div")
                && c.contains('x')
        );
        assert_eq!(
            estimated_nesting_depth(&"<div><b>x</div>".repeat(1_000), usize::MAX),
            2
        );
        assert_eq!(
            estimated_nesting_depth(&"<i><p>x</i>".repeat(1_000), usize::MAX),
            2
        );
        assert_eq!(estimated_nesting_depth(&html, 256), 257);
    }

    #[test]
    fn the_depth_estimate_bounds_html5ever() {
        let shapes = [
            "<b><div>x</b>".repeat(300),
            "<div><b>x</div>".repeat(300),
            "<i><p>x</i>".repeat(300),
            r#"<a href="https://e.org">x<p>y</a>"#.repeat(300),
            "<span><div>x</span>".repeat(300),
            "<td><div>x</td>".repeat(300),
            "<table><tr><td>x</b>".repeat(100),
            "<ul><li>x<p>y</ul>".repeat(300),
        ];
        for html in &shapes {
            assert!(
                estimated_nesting_depth(html, usize::MAX) + 5 >= html5ever_depth(html),
                "{}",
                &html[..30]
            );
        }
        for soup in tag_soups(1_500, 0x5EED) {
            assert!(
                estimated_nesting_depth(&soup, usize::MAX) + 5 >= html5ever_depth(&soup),
                "{soup}"
            );
        }
    }

    /// Shapes that slipped past the Mac's rules into html5ever, where 500 KB of each took from
    /// 2.8 to 27 s (measured with `scraper` 0.27.0): a tag name with a dash, an abruptly closed
    /// comment, an end tag hidden in a script, `body` reopened, XHTML-style `<div/>`, formatting
    /// reopened outside its block, `</li>` across a list, `</h1>` closing an `h2`, `</form>`
    /// leaving its children open, and a section closed by `</svg>`.
    #[test]
    fn guard_bypasses_are_counted() {
        let shapes = [
            ("", "<div-x></div>"),
            ("<!-->", "<div>"),
            ("<!--->", "<div>"),
            ("", "<div><script></div></script>"),
            ("", "<script><!--</script><div>"),
            ("", "<body><div></body>"),
            ("", "<div/>"),
            ("", "<div><b></div>x"),
            ("", "<li><ul>x</li>"),
            ("", "<h1><div><h2>x</h1>"),
            ("", "<form><div></form>"),
            ("", "<svg><section></svg><x/>"),
            ("", "<svg><p></p><x/>"),
            ("", "<svg><foreignObject><p></p></foreignObject><style><g>"),
            ("", "<a><div>"),
            ("", "<span><object>x</span>"),
            ("", "<template><td>"),
        ];
        for (head, unit) in shapes {
            let html = head.to_string() + &unit.repeat(300);
            let estimate = estimated_nesting_depth(&html, usize::MAX);
            assert!(estimate + 5 >= html5ever_depth(&html), "{unit}: {estimate}");
            assert!(estimate > MAX_NESTING_DEPTH, "{unit}: {estimate}");
            let big = head.to_string() + &unit.repeat(500 * 1024 / unit.len());
            let started = Instant::now();
            assert_eq!(clean(&big), plain_text_fallback(&big), "{unit}");
            assert!(
                started.elapsed() < Duration::from_millis(300),
                "{unit}: {:?}",
                started.elapsed()
            );
        }
    }

    /// Soups that break the estimate's bound (by more than the slack), shortest first.
    fn misses(soups: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut misses: Vec<String> = soups
            .into_iter()
            .filter(|soup| estimated_nesting_depth(soup, usize::MAX) + 5 < html5ever_depth(soup))
            .collect();
        misses.sort_by_key(String::len);
        misses
    }

    #[test]
    fn markup_soups_and_repeated_units_never_trip_the_estimate() {
        let soups = markup_soups(1_500, 0xC0FFEE)
            .into_iter()
            .chain(repeated_units(600, 0xBEEF));
        assert_eq!(misses(soups), Vec::<String>::new());
        for soup in markup_soups(300, 0xFACE) {
            let _ = most_attributes_on_a_tag(&soup, usize::MAX);
            let _ = plain_text_fallback(&soup);
        }
    }

    /// The estimate is a hand-written model of html5ever: when touching it, run this one too
    /// (`FUZZ_ROUNDS=1800 cargo test --release -p ressac-ui deep_fuzz -- --ignored`, about two
    /// minutes: 4.7 million soups, none missed on 23/10/2026).
    #[test]
    #[ignore]
    fn deep_fuzz_of_the_depth_estimate() {
        let rounds: u64 = std::env::var("FUZZ_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let mut found = Vec::new();
        for seed in 1..=rounds {
            let soups = markup_soups(1_000, seed * 7_919)
                .into_iter()
                .chain(tag_soups(1_000, seed * 104_729))
                .chain(repeated_units(600, seed * 31 + 7));
            found.extend(misses(soups));
        }
        found.sort_by_key(String::len);
        assert!(
            found.is_empty(),
            "{} misses, the shortest:\n{}",
            found.len(),
            found[..found.len().min(10)].join("\n")
        );
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
    fn abandoned_workers_are_bounded_and_never_block_the_next_article() {
        #[derive(Default)]
        struct Probe {
            running: AtomicUsize,
            peak: AtomicUsize,
            runs: AtomicUsize,
        }
        let probe = Arc::new(Probe::default());
        let gate = Gate::new(MAX_WORKERS);
        let work = {
            let probe = Arc::clone(&probe);
            move || {
                let now = probe.running.fetch_add(1, SeqCst) + 1;
                probe.peak.fetch_max(now, SeqCst);
                probe.runs.fetch_add(1, SeqCst);
                std::thread::sleep(Duration::from_secs(1));
                probe.running.fetch_sub(1, SeqCst);
                "done"
            }
        };
        assert_eq!(
            on_large_stack(Duration::from_millis(300), &gate, work.clone()),
            None
        ); // abandoned while it runs
        // One abandoned worker is running: the next article still gets its worker, within its budget.
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, work.clone()),
            Some("done")
        );
        assert_eq!(probe.peak.load(SeqCst), 2); // it ran beside the abandoned one, not after it
        // Two running (one abandoned), then more callers that give up: they wait, then skip.
        assert_eq!(
            on_large_stack(Duration::from_millis(300), &gate, work.clone()),
            None
        );
        for _ in 0..3 {
            assert_eq!(
                on_large_stack(Duration::from_millis(50), &gate, work.clone()),
                None
            );
        }
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, work),
            Some("done")
        );
        assert!(probe.peak.load(SeqCst) <= 2);
        assert!(probe.runs.load(SeqCst) <= 5);
    }

    #[test]
    fn a_panicking_worker_frees_its_slot_and_gives_nothing() {
        let gate = Gate::new(1);
        assert_eq!(
            on_large_stack(Duration::from_secs(5), &gate, || -> u8 {
                panic!("expected by the test")
            }),
            None
        );
        assert_eq!(on_large_stack(Duration::from_secs(5), &gate, || 7), Some(7));
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
    fn optional_end_tags_stay_on_the_parsed_path() {
        let rows = "<tr><td>a<td>b".repeat(150);
        let defs = "<dt>t<dd>d".repeat(150);
        let items = "<li><p>item".repeat(150);
        for html in [rows, defs, items] {
            assert!(
                estimated_nesting_depth(&html, usize::MAX) < 10,
                "{}",
                &html[..20]
            );
        }
    }

    #[test]
    fn sanitise_is_memoised() {
        let html = format!("<p>memo test {:?}</p>", std::time::SystemTime::now());
        let first = clean(&html);
        let (hits, _) = memo_stats();
        assert_eq!(clean(&html), first);
        assert!(memo_stats().0 > hits);
    }

    #[test]
    fn a_timed_out_parse_is_not_memoised() {
        let html = "<p>budget <b>test</b> 4242</p>";
        let timed_out = sanitize_within(html, None, Images::Web, Duration::ZERO);
        assert!(timed_out.contains("4242") && !timed_out.contains("<b>")); // the plain-text fallback
        assert!(sanitize(html, None, Images::Web).contains("<b>test</b>")); // not remembered
    }

    #[test]
    fn parse_runs_on_the_large_stack_thread() {
        let caller = std::thread::current().id();
        let worker = on_large_stack(Duration::from_secs(5), shared_gate(), || {
            std::thread::current().id()
        });
        assert!(matches!(worker, Some(id) if id != caller), "{worker:?}");
        assert!(clean("<p>hi</p>").contains("<p>hi"));
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
        for list in [
            &KEPT[..],
            &DROPPED[..],
            &VOID[..],
            &SPECIAL[..],
            &BREAKOUT[..],
        ] {
            assert!(list.windows(2).all(|pair| pair[0] < pair[1]), "{list:?}");
        }
    }
}
