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
/// not declare (`&nbsp;`, `&eacute;`) and control bytes XML 1.0 forbids. Both kinds of
/// entity become literal text (`&amp;nbsp;`), which renders right later because summaries
/// are HTML. `<![CDATA[ ... ]]>` sections are copied untouched.
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
            b'&' => match entity_len(rest) {
                Some((len, true)) => {
                    out.extend_from_slice(&rest[..len]);
                    i += len;
                }
                Some((len, false)) => {
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

/// Length of the well-formed reference at the start of `s` (which begins with `&`), and
/// whether XML itself defines it. `None` when no valid reference starts here.
fn entity_len(s: &[u8]) -> Option<(usize, bool)> {
    let semi = s.iter().position(|&b| b == b';')?;
    let name = &s[1..semi];
    let defined = match name {
        [b'#', b'x', digits @ ..] if !digits.is_empty() => {
            digits.iter().all(u8::is_ascii_hexdigit).then_some(true)?
        }
        [b'#', digits @ ..] if !digits.is_empty() => {
            digits.iter().all(u8::is_ascii_digit).then_some(true)?
        }
        [first, tail @ ..]
            if (first.is_ascii_alphabetic() || *first == b'_')
                && tail
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) =>
        {
            matches!(name, b"amp" | b"lt" | b"gt" | b"quot" | b"apos")
        }
        _ => return None,
    };
    Some((semi + 1, defined))
}

pub fn parse_feed(
    body: &[u8],
    content_type: Option<&str>,
    base_url: &str,
    fetched_at: DateTime<Utc>,
) -> Result<ParsedFeed, ErrorKind> {
    let body = trim_leading(body);
    if looks_like_html(content_type, body) {
        return Err(ErrorKind::HtmlInsteadOfFeed);
    }
    let feed = match parse_xml(body, base_url) {
        Ok(feed) => feed,
        Err(ErrorKind::Parse) => {
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
                title: e.title.map(|t| t.content),
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
        title: feed.title.map(|t| t.content),
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
        assert_eq!(f.entries[1].title.as_deref(), Some("AT&T&nbsp;news"));
        let summary = f.entries[0].summary_html.as_deref().unwrap();
        assert!(
            summary.contains("&eacute;") && summary.contains("&nbsp;"),
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
            fixed("&nbsp;&eacute;&_a.b-c;"),
            "&amp;nbsp;&amp;eacute;&amp;_a.b-c;"
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
    fn future_dates_fall_back_to_fetch_time() {
        let rss = RSS.replace("Mon, 05 Oct 2026", "Fri, 05 Oct 2035");
        let f = parse_feed(rss.as_bytes(), None, "https://blog.example.org/feed", now()).unwrap();
        assert!(f.entries[0].published_at_is_fallback);
        assert_eq!(f.entries[0].published_at, now());
    }
}
