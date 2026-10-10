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
    /// Callers must drain the receiver concurrently with this future.
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
                        out.chain.push(Hop {
                            status,
                            url: current.clone(),
                        });
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
                    // Release the permit before a possibly slow send.
                    drop(permit);
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
    if text.contains("dns error")
        || text.contains("failed to lookup address")
        || text.contains("nodename nor servname")
    {
        ErrorKind::Dns
    } else if text.contains("certificate") || text.contains("tls") || text.contains("handshake") {
        ErrorKind::Tls
    } else if e.is_body() || e.is_decode() {
        ErrorKind::Parse
    } else {
        ErrorKind::Connect
    }
}

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
