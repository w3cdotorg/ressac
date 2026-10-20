//! Feed parsing (RSS 0.9x/1.0/2.0, Atom, JSON Feed) into Ressac's own shapes.

use chrono::{DateTime, Duration, Utc};

use crate::model::ErrorKind;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFeed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_url: Option<String>,
    pub entries: Vec<ParsedEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEntry {
    /// feed-rs id: the feed's GUID/id, or feed-rs's stable hash of first link + title.
    pub guid: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub published_at: DateTime<Utc>,
    /// True when the feed gave no usable date and `published_at` is the fetch time.
    pub published_at_is_fallback: bool,
    pub updated_at: Option<DateTime<Utc>>,
    pub summary_html: Option<String>,
    pub content_html: Option<String>,
}

/// Decodes the HTML entities a title still holds after the XML parser: some feeds encode
/// titles twice (`&amp;#39;` in the XML), so the parser leaves `&#39;`. One pass, never more;
/// text without an entity comes back as it is. The only place titles are decoded: every
/// title entering the store (parsed feeds, OPML) goes through here.
pub(crate) fn decode_title_entities(title: &str) -> String {
    html_escape::decode_html_entities(title).into_owned()
}

const HTML_SNIFF_BYTES: usize = 512;

/// True for an HTML page: by its first bytes, or by a `text/html` content type when the
/// body does not start like XML or JSON (servers sometimes send RSS as `text/html`).
pub fn looks_like_html(content_type: Option<&str>, body: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&body[..body.len().min(HTML_SNIFF_BYTES)])
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if head.starts_with("<!doctype html") || head.starts_with("<html") {
        return true;
    }
    let declared_html =
        content_type.is_some_and(|ct| ct.to_ascii_lowercase().contains("text/html"));
    let looks_like_feed = ["<?xml", "<rss", "<feed", "<rdf", "{"]
        .iter()
        .any(|p| head.starts_with(p));
    declared_html && !looks_like_feed
}

pub(crate) const CHALLENGE_SNIFF_BYTES: usize = 64 * 1024;

/// Fingerprints of anti-bot interstitials (lowercase): Akamai/Anubis-style "Client
/// Challenge", Cloudflare, DDoS-Guard, DataDome and PerimeterX. `challenge-platform` is
/// deliberately absent: Cloudflare's JS detections inject it into ordinary pages.
const CHALLENGE_MARKERS: [&[u8]; 8] = [
    b"<title>client challenge",
    b"<title>just a moment",
    b"attention required! | cloudflare",
    b"cf-chl-",
    b"_cf_chl_opt",
    b"<title>ddos-guard",
    b"captcha-delivery.com/captcha",
    b"px-captcha",
];

/// True when an HTML head (case-insensitive) is an anti-bot challenge, not a real page.
pub(crate) fn is_challenge_page(head: &[u8]) -> bool {
    let lower = head.to_ascii_lowercase();
    CHALLENGE_MARKERS
        .iter()
        .any(|m| lower.windows(m.len()).any(|w| w == *m))
}

fn trim_leading(body: &[u8]) -> &[u8] {
    let body = body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body);
    let start = body
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(body.len());
    &body[start..]
}

fn parse_xml(body: &[u8], base_url: &str) -> Result<feed_rs::model::Feed, ErrorKind> {
    let parser = feed_rs::parser::Builder::new()
        .base_uri(Some(base_url))
        .build();
    parser.parse(body).map_err(|e| match e {
        feed_rs::parser::ParseFeedError::XmlReader(x) if x.to_string().contains("Encoding") => {
            ErrorKind::Encoding
        }
        _ => ErrorKind::Parse,
    })
}

/// Repairs the usual ways feeds are not well-formed XML: bare `&`, HTML entities XML does
/// not declare and control bytes XML 1.0 forbids. Common HTML entities (`&nbsp;`,
/// `&eacute;`, `&hellip;`) become numeric references, so titles show real characters;
/// unknown names become literal text (`&amp;name;`). Numeric references to characters
/// XML forbids are dropped. `<![CDATA[ ... ]]>` sections are copied untouched. Each `&`
/// costs at most `MAX_REFERENCE_BYTES`, so the repair is linear in the body size.
fn sanitize_xml(body: &[u8]) -> Vec<u8> {
    const CDATA_OPEN: &[u8] = b"<![CDATA[";
    const CDATA_CLOSE: &[u8] = b"]]>";
    let mut out = Vec::with_capacity(body.len() + 64);
    let mut i = 0;
    while i < body.len() {
        let rest = &body[i..];
        if rest.starts_with(CDATA_OPEN) {
            let end = rest
                .windows(CDATA_CLOSE.len())
                .position(|w| w == CDATA_CLOSE)
                .map_or(rest.len(), |p| p + CDATA_CLOSE.len());
            out.extend_from_slice(&rest[..end]);
            i += end;
            continue;
        }
        match body[i] {
            b'&' => match reference_at(rest) {
                Some((len, Reference::Keep)) => {
                    out.extend_from_slice(&rest[..len]);
                    i += len;
                }
                Some((len, Reference::Numeric(code))) => {
                    out.extend_from_slice(format!("&#{code};").as_bytes());
                    i += len;
                }
                Some((len, Reference::Drop)) => i += len,
                Some((len, Reference::Literal)) => {
                    out.extend_from_slice(b"&amp;");
                    out.extend_from_slice(&rest[1..len]);
                    i += len;
                }
                None => {
                    out.extend_from_slice(b"&amp;");
                    i += 1;
                }
            },
            0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F => i += 1,
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// Longest reference name looked at, `;` included. A `&` with no `;` this close is bare.
const MAX_REFERENCE_BYTES: usize = 34;

/// What to do with a well-formed `&...;` reference.
#[derive(Debug, PartialEq)]
enum Reference {
    /// XML defines it: copy as is.
    Keep,
    /// A common HTML entity: write this numeric reference instead.
    Numeric(u32),
    /// A numeric reference to a character XML 1.0 forbids: remove it.
    Drop,
    /// An unknown name: escape the `&` so it reads as literal text.
    Literal,
}

/// Length of the well-formed reference at the start of `s` (which begins with `&`) and
/// what to do with it. `None` when no valid reference starts here. Looks at no more than
/// `MAX_REFERENCE_BYTES` bytes.
fn reference_at(s: &[u8]) -> Option<(usize, Reference)> {
    let window = &s[..s.len().min(MAX_REFERENCE_BYTES)];
    let semi = window.iter().position(|&b| b == b';')?;
    let name = &s[1..semi];
    let kind = match name {
        [b'#', b'x', digits @ ..] if !digits.is_empty() => {
            digits.iter().all(u8::is_ascii_hexdigit).then_some(())?;
            code_reference(digits, 16)
        }
        [b'#', digits @ ..] if !digits.is_empty() => {
            digits.iter().all(u8::is_ascii_digit).then_some(())?;
            code_reference(digits, 10)
        }
        [first, tail @ ..]
            if (first.is_ascii_alphabetic() || *first == b'_')
                && tail
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) =>
        {
            if matches!(name, b"amp" | b"lt" | b"gt" | b"quot" | b"apos") {
                Reference::Keep
            } else {
                html_entity(name).map_or(Reference::Literal, Reference::Numeric)
            }
        }
        _ => return None,
    };
    Some((semi + 1, kind))
}

/// A numeric reference is kept when it names a character XML 1.0 allows, else dropped.
fn code_reference(digits: &[u8], radix: u32) -> Reference {
    let allowed = std::str::from_utf8(digits)
        .ok()
        .and_then(|d| u32::from_str_radix(d, radix).ok())
        .is_some_and(|c| {
            matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF)
        });
    if allowed {
        Reference::Keep
    } else {
        Reference::Drop
    }
}

/// HTML 4 Latin-1 entities, from U+00A0 to U+00FF in order.
const LATIN1_ENTITIES: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// Code point of a common HTML entity: Latin-1 plus the typographic ones.
fn html_entity(name: &[u8]) -> Option<u32> {
    if let Some(i) = LATIN1_ENTITIES.iter().position(|e| e.as_bytes() == name) {
        return Some(0xA0 + i as u32);
    }
    Some(match name {
        b"ndash" => 0x2013,
        b"mdash" => 0x2014,
        b"lsquo" => 0x2018,
        b"rsquo" => 0x2019,
        b"sbquo" => 0x201A,
        b"ldquo" => 0x201C,
        b"rdquo" => 0x201D,
        b"bdquo" => 0x201E,
        b"hellip" => 0x2026,
        b"bull" => 0x2022,
        b"trade" => 0x2122,
        b"euro" => 0x20AC,
        _ => return None,
    })
}

pub fn parse_feed(
    body: &[u8],
    content_type: Option<&str>,
    base_url: &str,
    fetched_at: DateTime<Utc>,
) -> Result<ParsedFeed, ErrorKind> {
    let body = trim_leading(body);
    if looks_like_html(content_type, body) {
        return Err(
            if is_challenge_page(&body[..body.len().min(CHALLENGE_SNIFF_BYTES)]) {
                ErrorKind::Challenge
            } else {
                ErrorKind::HtmlInsteadOfFeed
            },
        );
    }
    let feed = match parse_xml(body, base_url) {
        Ok(feed) if body.starts_with(b"<") => {
            // The XML parser keeps undeclared HTML entities as literal text (`&eacute;`),
            // which would show in plain-text titles: prefer the repaired reading when the
            // repair changed something and still parses.
            let repaired = sanitize_xml(body);
            match (repaired != body)
                .then(|| parse_xml(&repaired, base_url).ok())
                .flatten()
            {
                Some(better) => better,
                None => feed,
            }
        }
        Ok(feed) => feed,
        // XML only: a JSON Feed with a raw control character must not have its `&` rewritten.
        Err(ErrorKind::Parse) if body.starts_with(b"<") => {
            // Be lenient like NetNewsWire and Vienna: one retry on a repaired copy, and the
            // original error stands when that fails too.
            let repaired = sanitize_xml(body);
            let feed = parse_xml(&repaired, base_url).map_err(|_| ErrorKind::Parse)?;
            tracing::debug!(base_url, "feed parsed only after XML repair");
            feed
        }
        Err(e) => return Err(e),
    };
    let latest_plausible = fetched_at + Duration::days(1);
    let entries = feed
        .entries
        .into_iter()
        .map(|e| {
            let (published_at, fallback) = match e.published.or(e.updated) {
                Some(d) if d <= latest_plausible => (d, false),
                _ => (fetched_at, true),
            };
            ParsedEntry {
                guid: e.id,
                url: e.links.first().map(|l| l.href.clone()),
                title: e.title.map(|t| decode_title_entities(&t.content)),
                author: e.authors.first().and_then(|p| p.name.clone()),
                published_at,
                published_at_is_fallback: fallback,
                updated_at: e.updated,
                summary_html: e.summary.map(|t| t.content),
                content_html: e.content.and_then(|c| c.body),
            }
        })
        .collect();
    Ok(ParsedFeed {
        title: feed.title.map(|t| decode_title_entities(&t.content)),
        description: feed.description.map(|t| t.content),
        site_url: feed
            .links
            .iter()
            .find(|l| l.rel.as_deref() != Some("self"))
            .map(|l| l.href.clone()),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    const RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>Blog</title><link>https://blog.example.org/</link>
<description>A blog</description>
<item><guid>a1</guid><title>First</title><link>https://blog.example.org/1</link>
<pubDate>Mon, 05 Oct 2026 10:00:00 GMT</pubDate><description>Hello</description></item>
<item><title>No guid, no date</title><link>https://blog.example.org/2</link></item>
</channel></rss>"#;

    #[test]
    fn parses_rss2() {
        let f = parse_feed(
            RSS.as_bytes(),
            Some("application/rss+xml"),
            "https://blog.example.org/feed",
            now(),
        )
        .unwrap();
        assert_eq!(f.title.as_deref(), Some("Blog"));
        assert_eq!(f.description.as_deref(), Some("A blog"));
        assert_eq!(f.site_url.as_deref(), Some("https://blog.example.org/"));
        assert_eq!(f.entries.len(), 2);
        let first = &f.entries[0];
        assert_eq!(first.guid, "a1");
        assert_eq!(first.url.as_deref(), Some("https://blog.example.org/1"));
        assert!(!first.published_at_is_fallback);
        assert_eq!(first.summary_html.as_deref(), Some("Hello"));
        let second = &f.entries[1];
        assert!(!second.guid.is_empty(), "feed-rs generates a stable id");
        assert!(second.published_at_is_fallback);
        assert_eq!(second.published_at, now());
    }

    #[test]
    fn generated_ids_are_stable() {
        let a = parse_feed(RSS.as_bytes(), None, "https://blog.example.org/feed", now()).unwrap();
        let b = parse_feed(RSS.as_bytes(), None, "https://blog.example.org/feed", now()).unwrap();
        assert_eq!(a.entries[1].guid, b.entries[1].guid);
    }

    #[test]
    fn decodes_latin1() {
        let mut body = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><rss version=\"2.0\"><channel><title>Caf".to_vec();
        body.push(0xE9);
        body.extend_from_slice(
            b"</title><link>http://x.org</link><description>d</description></channel></rss>",
        );
        let f = parse_feed(&body, None, "http://x.org/rss", now()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Café"));
    }

    #[test]
    fn parses_rdf_and_json_feed() {
        let rdf = r#"<?xml version="1.0"?><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns="http://purl.org/rss/1.0/"><channel rdf:about="http://u.com/"><title>U</title><link>http://u.com/</link><description>d</description></channel><item rdf:about="http://u.com/1"><title>One</title><link>http://u.com/1</link></item></rdf:RDF>"#;
        assert_eq!(
            parse_feed(rdf.as_bytes(), None, "http://u.com/index.rdf", now())
                .unwrap()
                .entries
                .len(),
            1
        );
        let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"Six Colors","home_page_url":"https://sixcolors.com/","items":[{"id":"1","url":"https://sixcolors.com/post/1","title":"Hello","date_published":"2026-10-01T10:00:00Z","content_html":"<p>Hi</p>"}]}"#;
        let f = parse_feed(
            json.as_bytes(),
            Some("application/feed+json"),
            "https://sixcolors.com/feed.json",
            now(),
        )
        .unwrap();
        assert_eq!(f.title.as_deref(), Some("Six Colors"));
        assert_eq!(f.entries[0].content_html.as_deref(), Some("<p>Hi</p>"));
    }

    #[test]
    fn html_page_is_not_a_feed() {
        let html =
            b"<!DOCTYPE html><html><head><title>Parked</title></head><body>buy</body></html>";
        assert_eq!(
            parse_feed(html, Some("text/html"), "http://x.org/feed", now()),
            Err(ErrorKind::HtmlInsteadOfFeed)
        );
        assert!(looks_like_html(None, b"  <HTML><body></body></HTML>"));
    }

    #[test]
    fn challenge_pages_are_recognised() {
        for page in [
            "<html><head><title>Client Challenge</title></head>",
            "<html><title>Just a moment...</title>",
            "<title>Attention Required! | Cloudflare</title>",
            "<div id=\"cf-chl-widget-abc\"></div>",
            "<script>window._cf_chl_opt={}</script>",
            "<TITLE>DDoS-Guard</TITLE>",
            "<iframe src=\"https://geo.captcha-delivery.com/captcha/\">",
            "<div id=\"px-captcha\"></div>",
        ] {
            assert!(is_challenge_page(page.as_bytes()), "{page}");
        }
        assert!(!is_challenge_page(
            b"<!DOCTYPE html><html><head><title>My blog</title></head><body>Posts</body></html>"
        ));
    }

    #[test]
    fn challenge_html_is_a_challenge_and_plain_html_is_not() {
        let challenge = b"<!DOCTYPE html><html><head><title>Client Challenge</title></head></html>";
        assert_eq!(
            parse_feed(challenge, Some("text/html"), "http://x.org/feed", now()),
            Err(ErrorKind::Challenge)
        );
        let blog =
            b"<!DOCTYPE html><html><head><title>My blog</title></head><body>hi</body></html>";
        assert_eq!(
            parse_feed(blog, Some("text/html"), "http://x.org/feed", now()),
            Err(ErrorKind::HtmlInsteadOfFeed)
        );
    }

    #[test]
    fn rss_with_html_content_type_still_parses() {
        assert!(!looks_like_html(
            Some("text/html; charset=utf-8"),
            RSS.as_bytes()
        ));
        assert!(
            parse_feed(
                RSS.as_bytes(),
                Some("text/html; charset=utf-8"),
                "https://blog.example.org/feed",
                now()
            )
            .is_ok()
        );
    }

    #[test]
    fn bom_and_leading_whitespace_parse() {
        let mut body = vec![0xEF, 0xBB, 0xBF, b' ', b'\n'];
        body.extend_from_slice(RSS.as_bytes());
        assert_eq!(
            parse_feed(&body, None, "https://blog.example.org/feed", now())
                .unwrap()
                .entries
                .len(),
            2
        );
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert_eq!(
            parse_feed(b"not a feed", None, "http://x.org", now()),
            Err(ErrorKind::Parse)
        );
    }

    const BARE_AMP: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>Photos &amp; more</title><link>https://u.example/</link>
<description>d</description>
<item><guid>1</guid><title>Fish & Chips</title><description>caf&eacute; &nbsp;au lait &#233; &#xE9; &lt;b&gt;</description></item>
<item><guid>2</guid><title>AT&T&nbsp;news</title></item>
</channel></rss>"#;

    #[test]
    fn broken_xml_is_read_leniently() {
        assert!(parse_xml(BARE_AMP.as_bytes(), "https://u.example/feed").is_err());
        let f = parse_feed(BARE_AMP.as_bytes(), None, "https://u.example/feed", now()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Photos & more"));
        assert_eq!(f.entries[0].title.as_deref(), Some("Fish & Chips"));
        assert_eq!(f.entries[1].title.as_deref(), Some("AT&T\u{a0}news"));
        let summary = f.entries[0].summary_html.as_deref().unwrap();
        assert!(
            summary.contains("café") && summary.contains('\u{a0}'),
            "{summary}"
        );
    }

    #[test]
    fn sanitizer_fixes_ampersands_and_entities() {
        let fixed = |s: &str| String::from_utf8(sanitize_xml(s.as_bytes())).unwrap();
        assert_eq!(fixed("a & b"), "a &amp; b");
        assert_eq!(fixed("a &"), "a &amp;");
        assert_eq!(fixed("&;&#;&#x;&1a;"), "&amp;;&amp;#;&amp;#x;&amp;1a;");
        assert_eq!(
            fixed("&amp;&lt;&gt;&quot;&apos;"),
            "&amp;&lt;&gt;&quot;&apos;"
        );
        assert_eq!(fixed("&#38;&#x26;&#X26;"), "&#38;&#x26;&amp;#X26;");
        assert_eq!(
            fixed("&nbsp;&eacute;&_a.b-c;&bogus;"),
            "&#160;&#233;&amp;_a.b-c;&amp;bogus;"
        );
        assert_eq!(fixed("AT&T; x"), "AT&amp;T; x");
    }

    #[test]
    fn sanitizer_drops_illegal_control_bytes_only() {
        let body = b"a\x00b\x08c\x0Bd\x0Ce\x0Ef\x1Fg\th\ni\rj\x7F";
        assert_eq!(sanitize_xml(body), b"abcdefg\th\ni\rj\x7F");
    }

    #[test]
    fn sanitizer_leaves_cdata_alone() {
        let body = "<d><![CDATA[a & b &nbsp; \u{1}x]]> & <![CDATA[ unterminated &";
        let out = String::from_utf8(sanitize_xml(body.as_bytes())).unwrap();
        assert_eq!(
            out,
            "<d><![CDATA[a & b &nbsp; \u{1}x]]> &amp; <![CDATA[ unterminated &"
        );
    }

    #[test]
    fn sanitizer_keeps_non_ascii_bytes() {
        assert_eq!(sanitize_xml("é & ü".as_bytes()), "é &amp; ü".as_bytes());
        assert_eq!(
            sanitize_xml(&[0xE9, b'&']),
            [0xE9, b'&', b'a', b'm', b'p', b';']
        );
    }

    #[test]
    fn cdata_content_survives_the_lenient_retry() {
        let rss = BARE_AMP.replace(
            "<description>d</description>",
            "<description><![CDATA[<p>Q&A &nbsp;</p>]]></description>",
        );
        let f = parse_feed(rss.as_bytes(), None, "https://u.example/feed", now()).unwrap();
        assert_eq!(f.description.as_deref(), Some("<p>Q&A &nbsp;</p>"));
    }

    #[test]
    fn unrepairable_xml_keeps_the_original_error() {
        assert_eq!(
            parse_feed(b"<rss><channel><title>x & y", None, "http://x.org", now()),
            Err(ErrorKind::Parse)
        );
    }

    #[test]
    fn sanitizer_is_linear_on_bare_ampersands_without_semicolons() {
        let body = "a & b ".repeat(170_000); // about 1 MB, no ';' anywhere
        let started = std::time::Instant::now();
        let out = sanitize_xml(body.as_bytes());
        let elapsed = started.elapsed();
        assert_eq!(out, "a &amp; b ".repeat(170_000).as_bytes());
        let bound = if cfg!(debug_assertions) {
            std::time::Duration::from_secs(2)
        } else {
            std::time::Duration::from_millis(100)
        };
        assert!(elapsed < bound, "took {elapsed:?}");
    }

    #[test]
    fn jsd_script_on_a_small_page_is_not_a_challenge() {
        let parked = b"<!DOCTYPE html><html><head><title>Parked</title></head><body>for sale\
<script src=\"/cdn-cgi/challenge-platform/scripts/jsd/main.js\"></script></body></html>";
        assert_eq!(
            parse_feed(parked, Some("text/html"), "http://x.org/feed", now()),
            Err(ErrorKind::HtmlInsteadOfFeed)
        );
        assert!(!is_challenge_page(parked));
        assert!(!is_challenge_page(
            b"<script src=\"https://ct.captcha-delivery.com/c.js\"></script>"
        ));
    }

    #[test]
    fn sanitizer_maps_common_html_entities_to_numeric_references() {
        let fixed = |s: &str| String::from_utf8(sanitize_xml(s.as_bytes())).unwrap();
        for (name, code) in [
            ("nbsp", 160),
            ("iexcl", 161),
            ("agrave", 224),
            ("Eacute", 201),
            ("ccedil", 231),
            ("ntilde", 241),
            ("szlig", 223),
            ("uuml", 252),
            ("yuml", 255),
            ("laquo", 171),
            ("raquo", 187),
            ("copy", 169),
            ("reg", 174),
            ("deg", 176),
            ("middot", 183),
            ("ndash", 8211),
            ("mdash", 8212),
            ("lsquo", 8216),
            ("rsquo", 8217),
            ("sbquo", 8218),
            ("ldquo", 8220),
            ("rdquo", 8221),
            ("bdquo", 8222),
            ("hellip", 8230),
            ("bull", 8226),
            ("trade", 8482),
            ("euro", 8364),
        ] {
            assert_eq!(fixed(&format!("&{name};")), format!("&#{code};"), "{name}");
        }
        assert_eq!(fixed("&NBSP;&Foo;&amp;"), "&amp;NBSP;&amp;Foo;&amp;");
    }

    #[test]
    fn html_entities_in_titles_become_real_characters() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://u.example/</link><description>d</description><item><guid>1</guid><title>Caf&eacute; &ndash; l&rsquo;&eacute;t&eacute; &hellip;</title></item></channel></rss>"#;
        let f = parse_feed(rss.as_bytes(), None, "http://u.example/feed", now()).unwrap();
        assert_eq!(f.entries[0].title.as_deref(), Some("Café – l’été …"));
    }

    #[test]
    fn a_title_encoded_twice_is_decoded_once_more() {
        let rss = |title: &str| {
            format!(
                r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://u.example/</link><description>d</description><item><guid>1</guid><title>{title}</title></item></channel></rss>"#
            )
        };
        let parsed = |title: &str| {
            parse_feed(rss(title).as_bytes(), None, "http://u.example/feed", now()).unwrap()
        };
        for (xml, want) in [
            ("America&amp;#39;s Earliest", "America's Earliest"),
            ("America&amp;#x27;s Earliest", "America's Earliest"),
            ("Q&amp;amp;A", "Q&A"),
            ("Say &amp;quot;hi&amp;quot;", "Say \"hi\""),
            ("a&amp;nbsp;b", "a\u{a0}b"),
            // Encoded once: the parser's decoding is all there is.
            ("America&#39;s", "America's"),
            ("Q&amp;A", "Q&A"),
            // A bare ampersand and look-alikes stay.
            ("Fish & Chips", "Fish & Chips"),
            ("AT&T", "AT&T"),
            ("R&D; sort of", "R&D; sort of"),
        ] {
            let f = parsed(xml);
            assert_eq!(f.title.as_deref(), Some(want), "feed title {xml}");
            assert_eq!(
                f.entries[0].title.as_deref(),
                Some(want),
                "entry title {xml}"
            );
        }
    }

    #[test]
    fn decode_title_entities_decodes_one_pass_only() {
        assert_eq!(
            decode_title_entities("a &#39; b &#x41; &lt;i&gt;"),
            "a ' b A <i>"
        );
        // The decoded `&amp;#39;` is `&#39;`: a second pass would turn it into `'`.
        assert_eq!(decode_title_entities("&amp;#39;"), "&#39;");
        assert_eq!(decode_title_entities("plain & text"), "plain & text");
        assert_eq!(decode_title_entities("&notanentity; &"), "&notanentity; &");
    }

    #[test]
    fn numeric_references_xml_forbids_are_dropped() {
        let fixed = |s: &str| String::from_utf8(sanitize_xml(s.as_bytes())).unwrap();
        assert_eq!(
            fixed("a&#0;b&#1;c&#xD800;d&#xFFFE;e&#xFFFF;f&#x110000;g&#xFFFFFF;h&#99999999999;i"),
            "abcdefghi"
        );
        assert_eq!(
            fixed("&#9;&#10;&#13;&#x20;&#xD7FF;&#xE000;&#x10000;&#x10FFFF;"),
            "&#9;&#10;&#13;&#x20;&#xD7FF;&#xE000;&#x10000;&#x10FFFF;"
        );
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T&#0;itle &</title><link>http://u.example/</link><description>d</description></channel></rss>"#;
        let f = parse_feed(rss.as_bytes(), None, "http://u.example/feed", now()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Title &"));
    }

    #[test]
    fn json_feed_with_a_raw_control_character_is_not_repaired() {
        let json = "{\"version\":\"https://jsonfeed.org/version/1.1\",\"title\":\"T\x01\",\"items\":[{\"id\":\"1\",\"url\":\"https://x.org/?a=1&b=2\"}]}";
        assert_eq!(
            parse_feed(json.as_bytes(), None, "https://x.org/feed.json", now()),
            Err(ErrorKind::Parse)
        );
    }

    #[test]
    fn future_dates_fall_back_to_fetch_time() {
        let rss = RSS.replace("Mon, 05 Oct 2026", "Fri, 05 Oct 2035");
        let f = parse_feed(rss.as_bytes(), None, "https://blog.example.org/feed", now()).unwrap();
        assert!(f.entries[0].published_at_is_fallback);
        assert_eq!(f.entries[0].published_at, now());
    }
}
