//! The single entry point for new content. `LocalSource` fetches over HTTP politely;
//! a future sync source (FreshRSS, Miniflux) implements the same `Source` trait.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::header::{
    CONTENT_TYPE, ETAG, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION,
};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use url::Url;

use crate::error::Result;
use crate::model::{ErrorKind, FeedId, Hop};
use crate::parse::{CHALLENGE_SNIFF_BYTES, ParsedFeed, is_challenge_page, parse_feed};

pub const USER_AGENT: &str = concat!("Ressac/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct FetchLimits {
    pub max_in_flight: usize,
    pub per_host_gap: Duration,
    pub timeout: Duration,
    pub max_bytes: usize,
    /// Higher cap for podcast feeds (see `looks_like_podcast`), whose full-history
    /// feeds can run far past `max_bytes`. 16 in flight x 32 MB makes the body buffers
    /// alone about 512 MB; parsing and repair copies come on top.
    pub max_podcast_bytes: usize,
    pub max_redirects: usize,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self {
            max_in_flight: 16,
            per_host_gap: Duration::from_secs(1),
            timeout: Duration::from_secs(30),
            max_bytes: 10 * 1024 * 1024,
            max_podcast_bytes: 32 * 1024 * 1024,
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
    /// Callers must drain the receiver concurrently with this future; a slow consumer
    /// deliberately throttles fetching (a permit is held until its attempt is sent).
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
    /// Conditional headers are sent on the first request only. The timeout covers the
    /// whole exchange, redirects and body included; on expiry the hops seen so far are kept.
    pub async fn fetch_raw(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> RawResponse {
        let mut out = RawResponse {
            final_url: url.to_string(),
            ..RawResponse::default()
        };
        let exchange = self.follow(url, etag, last_modified, &mut out);
        if tokio::time::timeout(self.limits.timeout, exchange)
            .await
            .is_err()
        {
            out.error = Some(ErrorKind::Timeout);
            out.body.clear();
        }
        out
    }

    /// The redirect loop of `fetch_raw`, writing into `out` as it goes.
    async fn follow(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        out: &mut RawResponse,
    ) {
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
                    out.error = Some(classify(e));
                    return;
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
                        out.chain.push(Hop {
                            status,
                            url: current.clone(),
                        });
                        continue;
                    }
                    _ => {
                        out.error = Some(ErrorKind::Http(status));
                        return;
                    }
                }
            }
            out.content_type = header(&resp, CONTENT_TYPE);
            out.etag = header(&resp, ETAG);
            out.last_modified = header(&resp, LAST_MODIFIED);
            if status == 304 {
                out.not_modified = true;
                return;
            }
            if !(200..300).contains(&status) {
                out.error = Some(
                    if matches!(status, 401 | 403 | 429 | 503) && is_challenge_response(resp).await
                    {
                        ErrorKind::Challenge
                    } else {
                        ErrorKind::Http(status)
                    },
                );
                return;
            }
            let mut resp = resp;
            let mut cap = CapDecision::default();
            loop {
                match resp.chunk().await {
                    Ok(Some(chunk)) => {
                        let allowed = cap.cap(&self.limits, &out.body, &chunk);
                        if out.body.len() + chunk.len() > allowed {
                            out.body.clear();
                            out.error = Some(ErrorKind::TooLarge);
                            return;
                        }
                        out.body.extend_from_slice(&chunk);
                    }
                    Ok(None) => return,
                    Err(e) => {
                        out.error = Some(classify(e));
                        return;
                    }
                }
            }
        }
    }

    pub async fn fetch_one(&self, feed: &FeedRef) -> FetchAttempt {
        let started = Instant::now();
        let at = Utc::now();
        let raw = self
            .fetch_raw(
                &feed.url,
                feed.etag.as_deref(),
                feed.last_modified.as_deref(),
            )
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
    async fn refresh(
        &self,
        feeds: Vec<FeedRef>,
        tx: mpsc::Sender<FetchAttempt>,
        stop: Arc<AtomicBool>,
    ) {
        // One sequential lane per host, lanes run concurrently under a global cap.
        let mut by_host: BTreeMap<String, Vec<FeedRef>> = BTreeMap::new();
        for feed in feeds {
            by_host.entry(host_of(&feed.url)).or_default().push(feed);
        }
        let permits = Arc::new(Semaphore::new(self.limits.max_in_flight.max(1)));
        let mut lanes = JoinSet::new();
        for (_, lane) in by_host {
            let (source, tx, stop, permits) =
                (self.clone(), tx.clone(), stop.clone(), permits.clone());
            lanes.spawn(async move {
                for (i, feed) in lane.iter().enumerate() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if i > 0 {
                        tokio::time::sleep(source.limits.per_host_gap).await;
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    let Ok(permit) = permits.acquire().await else {
                        break;
                    };
                    // `stop` may have been set while this lane waited for the gap or a permit.
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let attempt = source.fetch_one(feed).await;
                    // The permit is held through the send: a slow consumer throttles fetching,
                    // which bounds in-memory results to the permit count plus channel capacity.
                    let sent = tx.send(attempt).await;
                    drop(permit);
                    if sent.is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        while lanes.join_next().await.is_some() {}
    }
}

/// How much of a feed is searched for podcast markers: they sit in the channel head.
const PODCAST_PROBE_BYTES: usize = 1024 * 1024;

/// The body cap in force: podcasts get the higher one.
fn body_cap(limits: &FetchLimits, podcast: bool) -> usize {
    if podcast {
        limits.max_podcast_bytes.max(limits.max_bytes)
    } else {
        limits.max_bytes
    }
}

/// Decides once per response whether the feed is a podcast, the first time the body
/// outgrows `max_bytes`, then reuses the answer for every later chunk.
#[derive(Debug, Default)]
struct CapDecision {
    podcast: Option<bool>,
}

impl CapDecision {
    /// The cap that applies once `chunk` is appended to `buffered`.
    fn cap(&mut self, limits: &FetchLimits, buffered: &[u8], chunk: &[u8]) -> usize {
        if buffered.len() + chunk.len() <= limits.max_bytes {
            return limits.max_bytes;
        }
        let podcast = *self.podcast.get_or_insert_with(|| {
            let mut head =
                Vec::with_capacity(PODCAST_PROBE_BYTES.min(buffered.len() + chunk.len()));
            head.extend_from_slice(&buffered[..buffered.len().min(PODCAST_PROBE_BYTES)]);
            let room = PODCAST_PROBE_BYTES - head.len();
            head.extend_from_slice(&chunk[..chunk.len().min(room)]);
            looks_like_podcast(&head)
        });
        body_cap(limits, podcast)
    }
}

/// True for the answer of an anti-bot gate on a non-2xx status: the `cf-mitigated:
/// challenge` header, or (reading at most 64 KB, errors ignored) a challenge page body.
async fn is_challenge_response(mut resp: reqwest::Response) -> bool {
    if header(&resp, HeaderName::from_static("cf-mitigated"))
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("challenge"))
    {
        return true;
    }
    let mut head = Vec::new();
    while head.len() < CHALLENGE_SNIFF_BYTES {
        match resp.chunk().await {
            Ok(Some(chunk)) => head.extend_from_slice(&chunk),
            _ => break,
        }
    }
    head.truncate(CHALLENGE_SNIFF_BYTES);
    is_challenge_page(&head)
}

/// True when the head of a feed (what is buffered so far) carries a podcast marker:
/// an iTunes/Podcasting 2.0 namespace or tag, or an audio/video `<enclosure>`.
fn looks_like_podcast(head: &[u8]) -> bool {
    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }
    if [&b"xmlns:itunes"[..], b"xmlns:podcast", b"<itunes:"]
        .iter()
        .any(|m| contains(head, m))
    {
        return true;
    }
    let mut rest = head;
    while let Some(i) = rest.windows(10).position(|w| w == b"<enclosure") {
        rest = &rest[i + 10..];
        let tag = &rest[..rest.iter().position(|&b| b == b'>').unwrap_or(rest.len())];
        if [
            &b"type=\"audio/"[..],
            b"type=\"video/",
            b"type='audio/",
            b"type='video/",
        ]
        .iter()
        .any(|m| contains(tag, m))
        {
            return true;
        }
    }
    false
}

fn header(resp: &reqwest::Response, name: HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Maps a reqwest error to an `ErrorKind` by inspecting the whole source chain.
/// The URL is dropped first, so a host or path containing "tls" cannot fool the match.
/// reqwest reports DNS and TLS handshake failures as connect errors too, hence the order.
fn classify(e: reqwest::Error) -> ErrorKind {
    if e.is_timeout() {
        return ErrorKind::Timeout;
    }
    let e = e.without_url();
    let mut text = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        text.push_str(" | ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    let text = text.to_ascii_lowercase();
    if text.contains("dns error")
        || text.contains("failed to lookup address")
        || text.contains("nodename nor servname")
    {
        ErrorKind::Dns
    } else if e.is_connect() && TLS_MARKERS.iter().any(|m| text.contains(m)) {
        ErrorKind::Tls
    } else if e.is_body() || e.is_decode() {
        ErrorKind::Parse
    } else {
        ErrorKind::Connect
    }
}

/// Fragments of TLS failures as rustls and the connector word them.
const TLS_MARKERS: [&str; 7] = [
    "tls",
    "certificate",
    "handshake",
    "received corrupt message",
    "peer is incompatible",
    "peer misbehaved",
    "received fatal alert",
];

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://example.org/</link><description>d</description><item><guid>1</guid><title>One</title><pubDate>Mon, 05 Oct 2026 10:00:00 GMT</pubDate></item></channel></rss>"#;

    fn fast() -> FetchLimits {
        FetchLimits {
            per_host_gap: Duration::from_millis(5),
            timeout: Duration::from_secs(2),
            ..FetchLimits::default()
        }
    }

    fn feed(id: FeedId, url: String) -> FeedRef {
        FeedRef {
            id,
            url,
            etag: None,
            last_modified: None,
        }
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
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await;
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
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/old", server.uri())))
            .await;
        assert_eq!(a.error, None);
        assert_eq!(
            a.redirect_chain,
            vec![Hop {
                status: 301,
                url: format!("{}/new", server.uri())
            }]
        );
        assert_eq!(a.final_url, format!("{}/new", server.uri()));
    }

    #[tokio::test]
    async fn gone_is_an_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(410))
            .mount(&server)
            .await;
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Http(410)));
        assert_eq!(a.http_status, Some(410));
    }

    #[tokio::test]
    async fn html_page_is_flagged() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "<!DOCTYPE html><html><body>parked</body></html>",
                "text/html",
            ))
            .mount(&server)
            .await;
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::HtmlInsteadOfFeed));
    }

    #[tokio::test]
    async fn slow_server_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(RSS, "application/rss+xml")
                    .set_delay(Duration::from_secs(3)),
            )
            .mount(&server)
            .await;
        let limits = FetchLimits {
            timeout: Duration::from_millis(300),
            ..fast()
        };
        let a = LocalSource::new(limits)
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Timeout));
    }

    #[tokio::test]
    async fn the_timeout_covers_the_whole_redirect_chain() {
        let server = MockServer::start().await;
        for (from, to) in [("/a", "/b"), ("/b", "/c"), ("/c", "/d")] {
            Mock::given(method("GET"))
                .and(path(from))
                .respond_with(
                    ResponseTemplate::new(301)
                        .insert_header("Location", to)
                        .set_delay(Duration::from_millis(200)),
                )
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/d"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(RSS, "application/rss+xml")
                    .set_delay(Duration::from_millis(200)),
            )
            .mount(&server)
            .await;
        let limits = FetchLimits {
            timeout: Duration::from_millis(500),
            ..fast()
        };
        let a = LocalSource::new(limits)
            .unwrap()
            .fetch_one(&feed(1, format!("{}/a", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Timeout));
        assert!(
            !a.redirect_chain.is_empty() && a.redirect_chain.len() < 3,
            "the hops seen before the deadline are kept: {:?}",
            a.redirect_chain
        );
        assert!(a.feed.is_none());
    }

    #[tokio::test]
    async fn tls_in_the_url_does_not_make_a_refused_connection_a_tls_error() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(
                1,
                format!("http://127.0.0.1:{port}/tls/handshake/certificate.xml"),
            ))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Connect));
    }

    #[tokio::test]
    async fn a_failed_handshake_is_a_tls_error() {
        let server = MockServer::start().await;
        serve_rss(&server, "/feed").await;
        let https = server.uri().replace("http://", "https://");
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{https}/feed")))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Tls));
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let server = MockServer::start().await;
        serve_rss(&server, "/feed").await;
        let limits = FetchLimits {
            max_bytes: 100,
            ..fast()
        };
        let a = LocalSource::new(limits)
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::TooLarge));
    }

    fn podcast_body(items: usize) -> String {
        let mut b = String::from(
            r#"<?xml version="1.0"?><rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"><channel><title>Pod</title><link>http://example.org/</link><description>d</description>"#,
        );
        for i in 0..items {
            b.push_str(&format!(
                "<item><guid>{i}</guid><title>Episode {i}</title></item>"
            ));
        }
        b.push_str("</channel></rss>");
        b
    }

    async fn fetch_body(body: String, limits: FetchLimits) -> FetchAttempt {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/feed"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml"))
            .mount(&server)
            .await;
        LocalSource::new(limits)
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await
    }

    #[test]
    fn podcast_markers_are_recognised() {
        assert!(looks_like_podcast(b"<rss xmlns:itunes=\"x\">"));
        assert!(looks_like_podcast(b"<rss xmlns:podcast=\"x\">"));
        assert!(looks_like_podcast(
            b"<item><itunes:duration>1</itunes:duration>"
        ));
        assert!(looks_like_podcast(
            b"<enclosure url=\"a.mp3\" length=\"1\" type=\"audio/mpeg\"/>"
        ));
        assert!(looks_like_podcast(
            b"<enclosure type=\"video/mp4\" url=\"a\"/>"
        ));
        assert!(!looks_like_podcast(b"<rss><channel><title>Blog</title>"));
        assert!(!looks_like_podcast(
            b"<enclosure url=\"a.jpg\" type=\"image/jpeg\"/> audio/ type=\"audio/\""
        ));
        assert!(!looks_like_podcast(b""));
        assert!(looks_like_podcast(
            b"<enclosure url='a.mp3' type='audio/mpeg'/>"
        ));
        assert!(looks_like_podcast(b"<enclosure type='video/mp4' url='a'/>"));
    }

    fn cap_limits() -> FetchLimits {
        FetchLimits {
            max_bytes: 100,
            max_podcast_bytes: 1000,
            ..FetchLimits::default()
        }
    }

    #[test]
    fn body_cap_picks_the_podcast_cap_only_for_podcasts() {
        assert_eq!(body_cap(&cap_limits(), false), 100);
        assert_eq!(body_cap(&cap_limits(), true), 1000);
    }

    #[test]
    fn cap_is_not_decided_under_max_bytes() {
        let mut d = CapDecision::default();
        assert_eq!(d.cap(&cap_limits(), b"", &[b'x'; 100]), 100);
        assert_eq!(d.podcast, None);
    }

    #[test]
    fn marker_only_in_the_overflowing_chunk_counts() {
        let mut d = CapDecision::default();
        let limits = cap_limits();
        let first = vec![b'x'; 90];
        assert_eq!(
            d.cap(&limits, &first, b"<enclosure type=\"audio/mpeg\">"),
            1000
        );
        assert_eq!(d.podcast, Some(true));
        let mut d = CapDecision::default();
        assert_eq!(
            d.cap(&limits, &first, b"<enclosure type='audio/mpeg'>"),
            1000
        );
    }

    #[test]
    fn marker_in_the_buffered_head_counts_and_decision_is_kept() {
        let mut d = CapDecision::default();
        let limits = cap_limits();
        let mut buffered = b"<rss xmlns:itunes=\"x\">".to_vec();
        buffered.resize(90, b' ');
        assert_eq!(d.cap(&limits, &buffered, &[b'y'; 20]), 1000);
        // Later chunks reuse the answer, whatever they hold.
        assert_eq!(d.cap(&limits, &buffered, &[b'p'; 50]), 1000);
    }

    #[test]
    fn non_podcast_decision_is_not_flipped_by_a_later_marker() {
        let mut d = CapDecision::default();
        let limits = cap_limits();
        assert_eq!(d.cap(&limits, &[b'x'; 90], &[b'y'; 20]), 100);
        assert_eq!(d.podcast, Some(false));
        let late = b"<enclosure type=\"audio/mpeg\">";
        assert_eq!(d.cap(&limits, &[b'x'; 110], late), 100);
    }

    #[test]
    fn podcast_probe_looks_at_the_first_megabyte_only() {
        let mut d = CapDecision::default();
        let limits = FetchLimits {
            max_bytes: 10,
            max_podcast_bytes: 1 << 30,
            ..FetchLimits::default()
        };
        let mut buffered = vec![b' '; PODCAST_PROBE_BYTES];
        buffered.extend_from_slice(b"<itunes:author>");
        assert_eq!(d.cap(&limits, &buffered, b"more"), 10);
    }

    async fn serve_status(
        status: u16,
        body: &str,
        header_pair: Option<(&str, &str)>,
    ) -> FetchAttempt {
        let server = MockServer::start().await;
        let mut t = ResponseTemplate::new(status).set_body_raw(body.to_string(), "text/html");
        if let Some((k, v)) = header_pair {
            t = t.insert_header(k, v);
        }
        Mock::given(method("GET"))
            .respond_with(t)
            .mount(&server)
            .await;
        LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/feed", server.uri())))
            .await
    }

    #[tokio::test]
    async fn a_403_challenge_page_is_a_challenge() {
        let a = serve_status(
            403,
            "<html><script>window._cf_chl_opt={cvId:'3'}</script></html>",
            None,
        )
        .await;
        assert_eq!(a.error, Some(ErrorKind::Challenge));
        assert_eq!(a.http_status, Some(403));
    }

    #[tokio::test]
    async fn a_plain_403_stays_http_403() {
        let a = serve_status(403, "<html><body>Forbidden</body></html>", None).await;
        assert_eq!(a.error, Some(ErrorKind::Http(403)));
    }

    #[tokio::test]
    async fn a_cf_mitigated_header_alone_is_a_challenge() {
        let a = serve_status(403, "", Some(("cf-mitigated", "challenge"))).await;
        assert_eq!(a.error, Some(ErrorKind::Challenge));
    }

    #[tokio::test]
    async fn a_404_with_a_challenge_looking_body_stays_http_404() {
        let a = serve_status(404, "<script>window._cf_chl_opt={}</script>", None).await;
        assert_eq!(a.error, Some(ErrorKind::Http(404)));
    }

    #[tokio::test]
    async fn non_podcast_over_the_cap_is_too_large() {
        let a = fetch_body(
            RSS.replace("<item>", &format!("<!--{}--><item>", "x".repeat(500))),
            FetchLimits {
                max_bytes: 100,
                max_podcast_bytes: 100_000,
                ..fast()
            },
        )
        .await;
        assert_eq!(a.error, Some(ErrorKind::TooLarge));
    }

    #[tokio::test]
    async fn podcast_over_the_cap_but_under_the_podcast_cap_is_fetched() {
        let a = fetch_body(
            podcast_body(20),
            FetchLimits {
                max_bytes: 300,
                max_podcast_bytes: 100_000,
                ..fast()
            },
        )
        .await;
        assert_eq!(a.error, None);
        assert_eq!(a.feed.unwrap().entries.len(), 20);
    }

    #[tokio::test]
    async fn podcast_over_the_podcast_cap_is_too_large() {
        let a = fetch_body(
            podcast_body(20),
            FetchLimits {
                max_bytes: 300,
                max_podcast_bytes: 600,
                ..fast()
            },
        )
        .await;
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
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, format!("{}/loop", server.uri())))
            .await;
        assert_eq!(a.error, Some(ErrorKind::Http(301)));
        assert_eq!(a.redirect_chain.len(), 5);
    }

    #[tokio::test]
    async fn unknown_domain_is_a_dns_error() {
        let a = LocalSource::new(fast())
            .unwrap()
            .fetch_one(&feed(1, "http://ressac-test.invalid/feed".into()))
            .await;
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
        LocalSource::new(fast())
            .unwrap()
            .refresh(feeds, tx, Arc::new(AtomicBool::new(false)))
            .await;
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
        LocalSource::new(fast())
            .unwrap()
            .refresh(feeds, tx, Arc::new(AtomicBool::new(true)))
            .await;
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn stop_set_mid_run_halts_queued_requests() {
        let server = MockServer::start().await;
        let paths = ["/a", "/b", "/c", "/d"];
        for p in paths {
            serve_rss(&server, p).await;
        }
        let feeds = paths
            .iter()
            .enumerate()
            .map(|(i, p)| feed(i as FeedId + 1, format!("{}{p}", server.uri())))
            .collect();
        let limits = FetchLimits {
            max_in_flight: 1,
            per_host_gap: Duration::from_millis(150),
            ..fast()
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::channel(8);
        let consumer_stop = stop.clone();
        let consumer = tokio::spawn(async move {
            let mut got = 0;
            while rx.recv().await.is_some() {
                got += 1;
                // Stop lands while the lane is already waiting out the per-host gap.
                tokio::time::sleep(Duration::from_millis(50)).await;
                consumer_stop.store(true, Ordering::Relaxed);
            }
            got
        });
        LocalSource::new(limits)
            .unwrap()
            .refresh(feeds, tx, stop)
            .await;
        let got = consumer.await.unwrap();
        assert_eq!(got, 1, "no request may start after stop is set");
    }
}
