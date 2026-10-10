# Phase 4: Feed parsing, polite HTTP source, fetch log and entries

Read the plan index first. Global Constraints apply. feed-rs and reqwest notes: `notes.md` § "API des crates".

**Files (5):**
- Create: `crates/ressac-core/src/parse.rs` (tests inline)
- Create: `crates/ressac-core/src/source.rs` (wiremock tests inline)
- Create: `crates/ressac-core/src/store/fetch.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (add `mod fetch; pub use fetch::*;`)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod parse; pub mod source;`)

### Task 4.1: Feed parsing

**Interfaces:**
- Consumes: `model::ErrorKind`.
- Produces (`ressac_core::parse`):
  - `struct ParsedFeed { title: Option<String>, description: Option<String>, site_url: Option<String>, entries: Vec<ParsedEntry> }`
  - `struct ParsedEntry { guid: String, url: Option<String>, title: Option<String>, author: Option<String>, published_at: DateTime<Utc>, published_at_is_fallback: bool, updated_at: Option<DateTime<Utc>>, summary_html: Option<String>, content_html: Option<String> }`
  - `looks_like_html(content_type: Option<&str>, body: &[u8]) -> bool`
  - `parse_feed(body: &[u8], content_type: Option<&str>, base_url: &str, fetched_at: DateTime<Utc>) -> Result<ParsedFeed, ErrorKind>`

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/src/parse.rs`

```rust
//! Feed parsing (RSS 0.9x/1.0/2.0, Atom, JSON Feed) into Ressac's own shapes.

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
        let f = parse_feed(RSS.as_bytes(), Some("application/rss+xml"), "https://blog.example.org/feed", now()).unwrap();
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
        body.extend_from_slice(b"</title><link>http://x.org</link><description>d</description></channel></rss>");
        let f = parse_feed(&body, None, "http://x.org/rss", now()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Café"));
    }

    #[test]
    fn parses_rdf_and_json_feed() {
        let rdf = r#"<?xml version="1.0"?><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns="http://purl.org/rss/1.0/"><channel rdf:about="http://u.com/"><title>U</title><link>http://u.com/</link><description>d</description></channel><item rdf:about="http://u.com/1"><title>One</title><link>http://u.com/1</link></item></rdf:RDF>"#;
        assert_eq!(parse_feed(rdf.as_bytes(), None, "http://u.com/index.rdf", now()).unwrap().entries.len(), 1);
        let json = r#"{"version":"https://jsonfeed.org/version/1.1","title":"Six Colors","home_page_url":"https://sixcolors.com/","items":[{"id":"1","url":"https://sixcolors.com/post/1","title":"Hello","date_published":"2026-10-01T10:00:00Z","content_html":"<p>Hi</p>"}]}"#;
        let f = parse_feed(json.as_bytes(), Some("application/feed+json"), "https://sixcolors.com/feed.json", now()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Six Colors"));
        assert_eq!(f.entries[0].content_html.as_deref(), Some("<p>Hi</p>"));
    }

    #[test]
    fn html_page_is_not_a_feed() {
        let html = b"<!DOCTYPE html><html><head><title>Parked</title></head><body>buy</body></html>";
        assert_eq!(parse_feed(html, Some("text/html"), "http://x.org/feed", now()), Err(ErrorKind::HtmlInsteadOfFeed));
        assert!(looks_like_html(None, b"  <HTML><body></body></HTML>"));
    }

    #[test]
    fn rss_with_html_content_type_still_parses() {
        assert!(!looks_like_html(Some("text/html; charset=utf-8"), RSS.as_bytes()));
        assert!(parse_feed(RSS.as_bytes(), Some("text/html; charset=utf-8"), "https://blog.example.org/feed", now()).is_ok());
    }

    #[test]
    fn bom_and_leading_whitespace_parse() {
        let mut body = vec![0xEF, 0xBB, 0xBF, b' ', b'\n'];
        body.extend_from_slice(RSS.as_bytes());
        assert_eq!(parse_feed(&body, None, "https://blog.example.org/feed", now()).unwrap().entries.len(), 2);
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert_eq!(parse_feed(b"not a feed", None, "http://x.org", now()), Err(ErrorKind::Parse));
    }

    #[test]
    fn future_dates_fall_back_to_fetch_time() {
        let rss = RSS.replace("Mon, 05 Oct 2026", "Fri, 05 Oct 2035");
        let f = parse_feed(rss.as_bytes(), None, "https://blog.example.org/feed", now()).unwrap();
        assert!(f.entries[0].published_at_is_fallback);
        assert_eq!(f.entries[0].published_at, now());
    }
}
```

- [ ] **Step 2: Register** `pub mod parse;` in `lib.rs`. Run `cargo test -p ressac-core parse`.
Expected: compile errors (`parse_feed` not found).

- [ ] **Step 3: Implement.** Insert above the tests in `parse.rs`:

```rust
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
    let declared_html = content_type.is_some_and(|ct| ct.to_ascii_lowercase().contains("text/html"));
    let looks_like_feed = ["<?xml", "<rss", "<feed", "<rdf", "{"].iter().any(|p| head.starts_with(p));
    declared_html && !looks_like_feed
}

fn trim_leading(body: &[u8]) -> &[u8] {
    let body = body.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(body);
    let start = body.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(body.len());
    &body[start..]
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
    let parser = feed_rs::parser::Builder::new().base_uri(Some(base_url)).build();
    let feed = parser.parse(body).map_err(|e| match e {
        feed_rs::parser::ParseFeedError::XmlReader(x) if x.to_string().contains("Encoding") => ErrorKind::Encoding,
        _ => ErrorKind::Parse,
    })?;
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
                author: e.authors.first().map(|p| p.name.clone()),
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ressac-core parse`
Expected: `9 passed`. If `XmlError` has no `Display`, match `format!("{x:?}")` instead and note it in `notes.md`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/parse.rs crates/ressac-core/src/lib.rs
git commit -m "Feed parsing with HTML sniffing, BOM tolerance and date fallback

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4.2: Polite HTTP source

**Interfaces:**
- Consumes: `parse::{ParsedFeed, parse_feed}`, `model::{ErrorKind, FeedId, Hop}`, `error::Result`.
- Produces (`ressac_core::source`):
  - `USER_AGENT: &str`
  - `struct FetchLimits { max_in_flight: usize, per_host_gap: Duration, timeout: Duration, max_bytes: usize, max_redirects: usize }`, with `Default` = 16, 1 s, 30 s, 10 MB, 5
  - `struct FeedRef { id: FeedId, url: String, etag: Option<String>, last_modified: Option<String> }`
  - `struct RawResponse { status, error, final_url, chain, content_type, etag, last_modified, not_modified, body }`
  - `struct FetchAttempt { feed_id, at, requested_url, final_url, http_status, error, redirect_chain, content_type, bytes, duration_ms, not_modified, etag, last_modified, feed: Option<ParsedFeed> }`
  - `trait Source { fn refresh(&self, feeds: Vec<FeedRef>, tx: mpsc::Sender<FetchAttempt>, stop: Arc<AtomicBool>) -> impl Future<Output = ()> + Send; }`
  - `LocalSource` (`Clone`), with `new(FetchLimits) -> Result<Self>`, `async fetch_raw(&self, url, etag, last_modified) -> RawResponse`, `async fetch_one(&self, &FeedRef) -> FetchAttempt`, and `impl Source`

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/src/source.rs`

```rust
//! The single entry point for new content. `LocalSource` fetches over HTTP politely;
//! a future sync source (FreshRSS, Miniflux) implements the same `Source` trait.

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://example.org/</link><description>d</description><item><guid>1</guid><title>One</title><pubDate>Mon, 05 Oct 2026 10:00:00 GMT</pubDate></item></channel></rss>"#;

    fn fast() -> FetchLimits {
        FetchLimits { per_host_gap: Duration::from_millis(5), timeout: Duration::from_secs(2), ..FetchLimits::default() }
    }

    fn feed(id: FeedId, url: String) -> FeedRef {
        FeedRef { id, url, etag: None, last_modified: None }
    }

    async fn serve_rss(server: &MockServer, at: &str) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(RSS, "application/rss+xml")
                    .insert_header("ETag", "\"v1\""),
            )
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn fetches_and_parses_a_feed() {
        let server = MockServer::start().await;
        serve_rss(&server, "/feed").await;
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, format!("{}/feed", server.uri()))).await;
        assert_eq!(a.error, None);
        assert_eq!(a.http_status, Some(200));
        assert_eq!(a.etag.as_deref(), Some("\"v1\""));
        assert_eq!(a.feed.unwrap().entries.len(), 1);
    }

    #[tokio::test]
    async fn conditional_request_gets_not_modified() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304))
            .with_priority(1)
            .mount(&server)
            .await;
        serve_rss(&server, "/feed").await;
        let mut r = feed(1, format!("{}/feed", server.uri()));
        r.etag = Some("\"v1\"".into());
        let a = LocalSource::new(fast()).unwrap().fetch_one(&r).await;
        assert!(a.not_modified);
        assert_eq!(a.error, None);
        assert!(a.feed.is_none());
    }

    #[tokio::test]
    async fn relative_redirect_is_resolved_and_recorded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/old"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", "/new"))
            .mount(&server)
            .await;
        serve_rss(&server, "/new").await;
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, format!("{}/old", server.uri()))).await;
        assert_eq!(a.error, None);
        assert_eq!(a.redirect_chain, vec![Hop { status: 301, url: format!("{}/new", server.uri()) }]);
        assert_eq!(a.final_url, format!("{}/new", server.uri()));
    }

    #[tokio::test]
    async fn gone_is_an_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(410)).mount(&server).await;
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, format!("{}/feed", server.uri()))).await;
        assert_eq!(a.error, Some(ErrorKind::Http(410)));
        assert_eq!(a.http_status, Some(410));
    }

    #[tokio::test]
    async fn html_page_is_flagged() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<!DOCTYPE html><html><body>parked</body></html>", "text/html"))
            .mount(&server)
            .await;
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, format!("{}/feed", server.uri()))).await;
        assert_eq!(a.error, Some(ErrorKind::HtmlInsteadOfFeed));
    }

    #[tokio::test]
    async fn slow_server_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(RSS, "application/rss+xml").set_delay(Duration::from_secs(3)))
            .mount(&server)
            .await;
        let limits = FetchLimits { timeout: Duration::from_millis(300), ..fast() };
        let a = LocalSource::new(limits).unwrap().fetch_one(&feed(1, format!("{}/feed", server.uri()))).await;
        assert_eq!(a.error, Some(ErrorKind::Timeout));
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let server = MockServer::start().await;
        serve_rss(&server, "/feed").await;
        let limits = FetchLimits { max_bytes: 100, ..fast() };
        let a = LocalSource::new(limits).unwrap().fetch_one(&feed(1, format!("{}/feed", server.uri()))).await;
        assert_eq!(a.error, Some(ErrorKind::TooLarge));
    }

    #[tokio::test]
    async fn redirect_loop_stops_after_five_hops() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", "/loop"))
            .mount(&server)
            .await;
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, format!("{}/loop", server.uri()))).await;
        assert_eq!(a.error, Some(ErrorKind::Http(301)));
        assert_eq!(a.redirect_chain.len(), 5);
    }

    #[tokio::test]
    async fn unknown_domain_is_a_dns_error() {
        let a = LocalSource::new(fast()).unwrap().fetch_one(&feed(1, "http://ressac-test.invalid/feed".into())).await;
        assert_eq!(a.error, Some(ErrorKind::Dns));
    }

    #[tokio::test]
    async fn refresh_delivers_one_attempt_per_feed() {
        let server = MockServer::start().await;
        for p in ["/a", "/b", "/c"] {
            serve_rss(&server, p).await;
        }
        let feeds = ["/a", "/b", "/c"]
            .iter()
            .enumerate()
            .map(|(i, p)| feed(i as FeedId + 1, format!("{}{p}", server.uri())))
            .collect();
        let (tx, mut rx) = mpsc::channel(8);
        LocalSource::new(fast()).unwrap().refresh(feeds, tx, Arc::new(AtomicBool::new(false))).await;
        let mut ids = Vec::new();
        while let Some(a) = rx.recv().await {
            assert_eq!(a.error, None);
            ids.push(a.feed_id);
        }
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn refresh_stops_when_asked() {
        let (tx, mut rx) = mpsc::channel(8);
        let feeds = vec![feed(1, "http://ressac-test.invalid/a".into())];
        LocalSource::new(fast()).unwrap().refresh(feeds, tx, Arc::new(AtomicBool::new(true))).await;
        assert!(rx.recv().await.is_none());
    }
}
```

- [ ] **Step 2: Register** `pub mod source;` in `lib.rs`. Run `cargo test -p ressac-core source`.
Expected: compile errors (`LocalSource` not found).

- [ ] **Step 3: Implement.** Insert above the tests in `source.rs`:

```rust
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::header::{CONTENT_TYPE, ETAG, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use url::Url;

use crate::error::Result;
use crate::model::{ErrorKind, FeedId, Hop};
use crate::parse::{ParsedFeed, parse_feed};

pub const USER_AGENT: &str = concat!("Ressac/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct FetchLimits {
    pub max_in_flight: usize,
    pub per_host_gap: Duration,
    pub timeout: Duration,
    pub max_bytes: usize,
    pub max_redirects: usize,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self {
            max_in_flight: 16,
            per_host_gap: Duration::from_secs(1),
            timeout: Duration::from_secs(30),
            max_bytes: 10 * 1024 * 1024,
            max_redirects: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRef {
    pub id: FeedId,
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Outcome of one GET with redirects followed by hand.
#[derive(Debug, Clone, Default)]
pub struct RawResponse {
    pub status: Option<u16>,
    pub error: Option<ErrorKind>,
    pub final_url: String,
    pub chain: Vec<Hop>,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub not_modified: bool,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FetchAttempt {
    pub feed_id: FeedId,
    pub at: DateTime<Utc>,
    pub requested_url: String,
    pub final_url: String,
    pub http_status: Option<u16>,
    pub error: Option<ErrorKind>,
    pub redirect_chain: Vec<Hop>,
    pub content_type: Option<String>,
    pub bytes: u64,
    pub duration_ms: u64,
    pub not_modified: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub feed: Option<ParsedFeed>,
}

pub trait Source {
    /// Fetches every feed and sends one attempt per feed on `tx`.
    /// Stops scheduling new requests once `stop` is set; in-flight ones complete.
    fn refresh(
        &self,
        feeds: Vec<FeedRef>,
        tx: mpsc::Sender<FetchAttempt>,
        stop: Arc<AtomicBool>,
    ) -> impl Future<Output = ()> + Send;
}

#[derive(Clone)]
pub struct LocalSource {
    client: reqwest::Client,
    limits: FetchLimits,
}

impl LocalSource {
    pub fn new(limits: FetchLimits) -> Result<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(limits.timeout)
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self { client, limits })
    }

    /// GET `url`, following up to `max_redirects` redirects by hand to record every hop.
    /// Conditional headers are sent on the first request only.
    pub async fn fetch_raw(&self, url: &str, etag: Option<&str>, last_modified: Option<&str>) -> RawResponse {
        let mut out = RawResponse { final_url: url.to_string(), ..RawResponse::default() };
        let mut current = url.to_string();
        loop {
            let mut req = self.client.get(&current);
            if out.chain.is_empty() {
                if let Some(e) = etag {
                    req = req.header(IF_NONE_MATCH, e);
                }
                if let Some(lm) = last_modified {
                    req = req.header(IF_MODIFIED_SINCE, lm);
                }
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    out.error = Some(classify(&e));
                    return out;
                }
            };
            let status = resp.status().as_u16();
            out.status = Some(status);
            out.final_url = current.clone();
            if (300..400).contains(&status) && status != 304 {
                let next = header(&resp, LOCATION)
                    .and_then(|loc| Url::parse(&current).ok()?.join(&loc).ok());
                match next {
                    Some(next) if out.chain.len() < self.limits.max_redirects => {
                        current = next.to_string();
                        out.chain.push(Hop { status, url: current.clone() });
                        continue;
                    }
                    _ => {
                        out.error = Some(ErrorKind::Http(status));
                        return out;
                    }
                }
            }
            out.content_type = header(&resp, CONTENT_TYPE);
            out.etag = header(&resp, ETAG);
            out.last_modified = header(&resp, LAST_MODIFIED);
            if status == 304 {
                out.not_modified = true;
                return out;
            }
            if !(200..300).contains(&status) {
                out.error = Some(ErrorKind::Http(status));
                return out;
            }
            let mut resp = resp;
            loop {
                match resp.chunk().await {
                    Ok(Some(chunk)) => {
                        if out.body.len() + chunk.len() > self.limits.max_bytes {
                            out.body.clear();
                            out.error = Some(ErrorKind::TooLarge);
                            return out;
                        }
                        out.body.extend_from_slice(&chunk);
                    }
                    Ok(None) => return out,
                    Err(e) => {
                        out.error = Some(classify(&e));
                        return out;
                    }
                }
            }
        }
    }

    pub async fn fetch_one(&self, feed: &FeedRef) -> FetchAttempt {
        let started = Instant::now();
        let at = Utc::now();
        let raw = self
            .fetch_raw(&feed.url, feed.etag.as_deref(), feed.last_modified.as_deref())
            .await;
        let mut error = raw.error.clone();
        let parsed = if error.is_none() && !raw.not_modified {
            match parse_feed(&raw.body, raw.content_type.as_deref(), &raw.final_url, at) {
                Ok(f) => Some(f),
                Err(kind) => {
                    error = Some(kind);
                    None
                }
            }
        } else {
            None
        };
        FetchAttempt {
            feed_id: feed.id,
            at,
            requested_url: feed.url.clone(),
            final_url: raw.final_url,
            http_status: raw.status,
            error,
            redirect_chain: raw.chain,
            content_type: raw.content_type,
            bytes: raw.body.len() as u64,
            duration_ms: started.elapsed().as_millis() as u64,
            not_modified: raw.not_modified,
            etag: raw.etag,
            last_modified: raw.last_modified,
            feed: parsed,
        }
    }
}

impl Source for LocalSource {
    fn refresh(
        &self,
        feeds: Vec<FeedRef>,
        tx: mpsc::Sender<FetchAttempt>,
        stop: Arc<AtomicBool>,
    ) -> impl Future<Output = ()> + Send {
        async move {
            // One sequential lane per host, lanes run concurrently under a global cap.
            let mut by_host: BTreeMap<String, Vec<FeedRef>> = BTreeMap::new();
            for feed in feeds {
                by_host.entry(host_of(&feed.url)).or_default().push(feed);
            }
            let permits = Arc::new(Semaphore::new(self.limits.max_in_flight.max(1)));
            let mut lanes = JoinSet::new();
            for (_, lane) in by_host {
                let (source, tx, stop, permits) = (self.clone(), tx.clone(), stop.clone(), permits.clone());
                lanes.spawn(async move {
                    for (i, feed) in lane.iter().enumerate() {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        if i > 0 {
                            tokio::time::sleep(source.limits.per_host_gap).await;
                        }
                        let Ok(_permit) = permits.acquire().await else { break };
                        let attempt = source.fetch_one(feed).await;
                        if tx.send(attempt).await.is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            while lanes.join_next().await.is_some() {}
        }
    }
}

fn header(resp: &reqwest::Response, name: HeaderName) -> Option<String> {
    resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Maps a reqwest error to an `ErrorKind` by inspecting the whole source chain.
fn classify(e: &reqwest::Error) -> ErrorKind {
    if e.is_timeout() {
        return ErrorKind::Timeout;
    }
    let mut text = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        text.push_str(" | ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    let text = text.to_ascii_lowercase();
    if text.contains("dns error") || text.contains("failed to lookup address") || text.contains("nodename nor servname") {
        ErrorKind::Dns
    } else if text.contains("certificate") || text.contains("tls") || text.contains("handshake") {
        ErrorKind::Tls
    } else if e.is_body() || e.is_decode() {
        ErrorKind::Parse
    } else {
        ErrorKind::Connect
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ressac-core source`
Expected: `11 passed`. If `unknown_domain_is_a_dns_error` yields `Connect`, print the error chain (`eprintln!` in `classify` temporarily). Add the platform's wording to the `Dns` match and log it in `notes.md`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/source.rs crates/ressac-core/src/lib.rs
git commit -m "Polite HTTP source: per-host lanes, manual redirects, caps, error classes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4.3: Fetch log and entries in the store

**Interfaces:**
- Consumes: `source::{FetchAttempt, FeedRef}`, `model::{ErrorKind, Hop, ts, parse_ts}`.
- Produces (re-exported from `ressac_core::store`):
  - `FETCH_LOG_KEEP: i64 = 20`, `FRESH_FOR_MINUTES: i64 = 30`
  - `struct AttemptRecord { at, http_status: Option<u16>, error: Option<ErrorKind>, redirect_chain: Vec<Hop>, content_type: Option<String>, network_suspect: bool, not_modified: bool, entry_count: Option<i64> }`
  - `struct EntryRecord { guid, url: Option<String>, title: Option<String>, summary_html: Option<String>, published_at: DateTime<Utc>, is_fallback: bool, fetched_at: DateTime<Utc> }`
  - `Store::record_attempt(&self, run_id: &str, a: &FetchAttempt) -> Result<usize>`. Returns the number of new entries.
  - `Store::mark_network_suspect(&self, run_id: &str) -> Result<usize>`
  - `Store::feeds_to_fetch(&self, now: DateTime<Utc>, force: bool) -> Result<Vec<FeedRef>>`
  - `Store::attempts(&self, feed_id, limit: usize) -> Result<Vec<AttemptRecord>>`, newest first
  - `Store::entries(&self, feed_id, limit: usize) -> Result<Vec<EntryRecord>>`, newest first

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/src/store/fetch.rs`

```rust
//! Fetch log, entries and fetch scheduling.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opml;
    use crate::parse::{ParsedEntry, ParsedFeed};

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn store_with_feeds(n: usize) -> Store {
        let body: String = (1..=n)
            .map(|i| format!(r#"<outline text="F{i}" xmlUrl="https://f{i}.example.org/feed"/>"#))
            .collect();
        let xml = format!("<opml><body>{body}</body></opml>");
        let store = Store::open_in_memory().unwrap();
        store.import_outlines(&opml::parse(&xml).unwrap(), false, now()).unwrap();
        store
    }

    fn entry(guid: &str) -> ParsedEntry {
        ParsedEntry {
            guid: guid.into(),
            url: Some(format!("https://e.example.org/{guid}")),
            title: Some(guid.to_uppercase()),
            author: None,
            published_at: now() - Duration::days(1),
            published_at_is_fallback: false,
            updated_at: None,
            summary_html: None,
            content_html: None,
        }
    }

    fn attempt(feed_id: FeedId, at: DateTime<Utc>, error: Option<ErrorKind>, guids: &[&str]) -> FetchAttempt {
        FetchAttempt {
            feed_id,
            at,
            requested_url: "https://f1.example.org/feed".into(),
            final_url: "https://f1.example.org/feed".into(),
            http_status: Some(if error.is_some() { 503 } else { 200 }),
            error: error.clone(),
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            bytes: 100,
            duration_ms: 12,
            not_modified: false,
            etag: Some("\"e1\"".into()),
            last_modified: None,
            feed: error.is_none().then(|| ParsedFeed {
                title: Some("F".into()),
                description: Some("Desc".into()),
                site_url: None,
                entries: guids.iter().map(|g| entry(g)).collect(),
            }),
        }
    }

    #[test]
    fn records_entries_and_counts_only_new_ones() {
        let store = store_with_feeds(1);
        assert_eq!(store.record_attempt("r1", &attempt(1, now(), None, &["a", "b"])).unwrap(), 2);
        assert_eq!(store.record_attempt("r2", &attempt(1, now(), None, &["b", "c"])).unwrap(), 1);
        let entries = store.entries(1, 10).unwrap();
        assert_eq!(entries.len(), 3);
        let feed = store.feed(1).unwrap();
        assert_eq!(feed.etag.as_deref(), Some("\"e1\""));
        assert_eq!(feed.description_current.as_deref(), Some("Desc"));
        assert_eq!(feed.final_url.as_deref(), Some("https://f1.example.org/feed"));
    }

    #[test]
    fn log_is_trimmed_to_last_twenty() {
        let store = store_with_feeds(1);
        for i in 0..25 {
            store.record_attempt("r", &attempt(1, now() + Duration::minutes(i), None, &[])).unwrap();
        }
        let attempts = store.attempts(1, 100).unwrap();
        assert_eq!(attempts.len(), 20);
        assert_eq!(attempts[0].at, now() + Duration::minutes(24), "newest first");
    }

    #[test]
    fn errors_round_trip_and_network_suspects_are_marked_per_run() {
        let store = store_with_feeds(3);
        store.record_attempt("r1", &attempt(1, now(), Some(ErrorKind::Dns), &[])).unwrap();
        store.record_attempt("r1", &attempt(2, now(), Some(ErrorKind::Http(410)), &[])).unwrap();
        store.record_attempt("r0", &attempt(3, now(), Some(ErrorKind::Connect), &[])).unwrap();
        assert_eq!(store.mark_network_suspect("r1").unwrap(), 1);
        assert!(store.attempts(1, 1).unwrap()[0].network_suspect);
        assert_eq!(store.attempts(2, 1).unwrap()[0].error, Some(ErrorKind::Http(410)));
        assert!(!store.attempts(3, 1).unwrap()[0].network_suspect, "other runs untouched");
    }

    #[test]
    fn feeds_to_fetch_skips_fresh_and_buried_feeds() {
        let store = store_with_feeds(3);
        store.record_attempt("r", &attempt(1, now() - Duration::minutes(10), None, &[])).unwrap();
        store.conn().execute("UPDATE feed SET state = 'buried' WHERE id = 3", []).unwrap();
        let ids: Vec<FeedId> = store.feeds_to_fetch(now(), false).unwrap().iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![2]);
        let ids: Vec<FeedId> = store.feeds_to_fetch(now(), true).unwrap().iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![1, 2]);
        let refs = store.feeds_to_fetch(now(), true).unwrap();
        assert_eq!(refs[0].etag.as_deref(), Some("\"e1\""));
    }
}
```

- [ ] **Step 2: Wire the module.** In `store/mod.rs` add `mod fetch;` and `pub use fetch::{AttemptRecord, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};`. Run `cargo test -p ressac-core store::fetch`.
Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests in `store/fetch.rs`:

```rust
use chrono::{DateTime, Duration, Utc};
use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::{ErrorKind, FeedId, Hop, parse_ts, ts};
use crate::source::{FeedRef, FetchAttempt};

pub const FETCH_LOG_KEEP: i64 = 20;
pub const FRESH_FOR_MINUTES: i64 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRecord {
    pub at: DateTime<Utc>,
    pub http_status: Option<u16>,
    pub error: Option<ErrorKind>,
    pub redirect_chain: Vec<Hop>,
    pub content_type: Option<String>,
    pub network_suspect: bool,
    pub not_modified: bool,
    pub entry_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryRecord {
    pub guid: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary_html: Option<String>,
    pub published_at: DateTime<Utc>,
    pub is_fallback: bool,
    pub fetched_at: DateTime<Utc>,
}

fn bad_timestamp(col: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(col, "timestamp".into(), rusqlite::types::Type::Text)
}

impl Store {
    /// Writes one attempt in a transaction: log row (trimmed to the last 20), feed cache
    /// fields, and entries. Returns how many entries were new.
    pub fn record_attempt(&self, run_id: &str, a: &FetchAttempt) -> Result<usize> {
        let tx = self.conn().unchecked_transaction()?;
        let error_json = a.error.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute(
            "INSERT INTO fetch_log (feed_id, run_id, at, http_status, error_kind, redirect_chain, content_type, \
             bytes, duration_ms, entry_count, not_modified) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                a.feed_id,
                run_id,
                ts(a.at),
                a.http_status,
                error_json,
                serde_json::to_string(&a.redirect_chain)?,
                a.content_type,
                a.bytes as i64,
                a.duration_ms as i64,
                a.feed.as_ref().map(|f| f.entries.len() as i64),
                a.not_modified
            ],
        )?;
        tx.execute(
            "DELETE FROM fetch_log WHERE feed_id = ?1 AND id NOT IN \
             (SELECT id FROM fetch_log WHERE feed_id = ?1 ORDER BY at DESC, id DESC LIMIT ?2)",
            params![a.feed_id, FETCH_LOG_KEEP],
        )?;
        if a.error.is_none() {
            tx.execute(
                "UPDATE feed SET final_url = ?2, etag = COALESCE(?3, etag), \
                 last_modified = COALESCE(?4, last_modified) WHERE id = ?1",
                params![a.feed_id, a.final_url, a.etag, a.last_modified],
            )?;
        }
        let mut added = 0;
        if let Some(feed) = &a.feed {
            tx.execute(
                "UPDATE feed SET description_current = ?2 WHERE id = ?1",
                params![a.feed_id, feed.description],
            )?;
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO entry (feed_id, guid, url, title, author, published_at, \
                 published_at_is_fallback, updated_at, summary_html, content_html, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            let mut update = tx.prepare(
                "UPDATE entry SET url = ?3, title = ?4, updated_at = ?5, summary_html = ?6, content_html = ?7 \
                 WHERE feed_id = ?1 AND guid = ?2",
            )?;
            for e in &feed.entries {
                let inserted = insert.execute(params![
                    a.feed_id,
                    e.guid,
                    e.url,
                    e.title,
                    e.author,
                    ts(e.published_at),
                    e.published_at_is_fallback,
                    e.updated_at.map(ts),
                    e.summary_html,
                    e.content_html,
                    ts(a.at)
                ])?;
                if inserted == 1 {
                    added += 1;
                } else {
                    update.execute(params![
                        a.feed_id,
                        e.guid,
                        e.url,
                        e.title,
                        e.updated_at.map(ts),
                        e.summary_html,
                        e.content_html
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    /// Flags this run's DNS/connection failures as caused by the user's network.
    pub fn mark_network_suspect(&self, run_id: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE fetch_log SET network_suspect = 1 WHERE run_id = ?1 AND error_kind IN ('\"Dns\"', '\"Connect\"')",
            [run_id],
        )?)
    }

    /// Active feeds not fetched in the last 30 minutes (all active feeds when `force`).
    pub fn feeds_to_fetch(&self, now: DateTime<Utc>, force: bool) -> Result<Vec<FeedRef>> {
        let fresh_since = ts(now - Duration::minutes(FRESH_FOR_MINUTES));
        let mut stmt = self.conn().prepare(
            "SELECT f.id, f.xml_url, f.etag, f.last_modified FROM feed f WHERE f.state = 'active' AND \
             (?1 OR NOT EXISTS (SELECT 1 FROM fetch_log l WHERE l.feed_id = f.id AND l.at > ?2)) ORDER BY f.id",
        )?;
        let rows = stmt
            .query_map(params![force, fresh_since], |r| {
                Ok(FeedRef { id: r.get(0)?, url: r.get(1)?, etag: r.get(2)?, last_modified: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn attempts(&self, feed_id: FeedId, limit: usize) -> Result<Vec<AttemptRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT at, http_status, error_kind, redirect_chain, content_type, network_suspect, not_modified, \
             entry_count FROM fetch_log WHERE feed_id = ?1 ORDER BY at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![feed_id, limit as i64], |r| {
                let at: String = r.get(0)?;
                let error: Option<String> = r.get(2)?;
                let chain: String = r.get(3)?;
                Ok(AttemptRecord {
                    at: parse_ts(&at).ok_or_else(|| bad_timestamp(0))?,
                    http_status: r.get(1)?,
                    error: error.and_then(|e| serde_json::from_str(&e).ok()),
                    redirect_chain: serde_json::from_str(&chain).unwrap_or_default(),
                    content_type: r.get(4)?,
                    network_suspect: r.get(5)?,
                    not_modified: r.get(6)?,
                    entry_count: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn entries(&self, feed_id: FeedId, limit: usize) -> Result<Vec<EntryRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT guid, url, title, summary_html, published_at, published_at_is_fallback, fetched_at \
             FROM entry WHERE feed_id = ?1 ORDER BY published_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![feed_id, limit as i64], |r| {
                let published: String = r.get(4)?;
                let fetched: String = r.get(6)?;
                Ok(EntryRecord {
                    guid: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    summary_html: r.get(3)?,
                    published_at: parse_ts(&published).ok_or_else(|| bad_timestamp(4))?,
                    is_fallback: r.get(5)?,
                    fetched_at: parse_ts(&fetched).ok_or_else(|| bad_timestamp(6))?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ressac-core store`
Expected: all store tests pass (`13`).

- [ ] **Step 5: Verification gate** (fmt, clippy `-D warnings`, test).

- [ ] **Step 6: Commit and push**

```bash
git add crates/ressac-core/src/store
git commit -m "Fetch log with trimming, entries upsert, scheduling and network-suspect marking

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Phase gate:** report and wait for approval before Phase 5.
