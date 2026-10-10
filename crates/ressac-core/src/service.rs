//! The `Ressac` facade used by every interface: import, refresh (fetch → probes →
//! diagnosis), Wayback lookups and summaries.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::discover::{alternate_feeds, same_feed};
use crate::error::Result;
use crate::garden;
use crate::health::links::{LinkInput, apply_similar_titles, find_duplicates};
use crate::health::{FeedEvidence, diagnose};
use crate::model::{Action, Diagnosis, ErrorKind, FeedId, Status, parse_ts, ts};
use crate::opml::{self, clean_description};
use crate::parse::{
    CHALLENGE_SNIFF_BYTES, ParsedFeed, is_challenge_page, looks_like_html, parse_feed,
};
use crate::source::{FeedRef, FetchAttempt, FetchLimits, LocalSource, RawResponse, Source};
use crate::store::{FeedRow, ImportReport, Store};
use crate::url_key::{https_variant, url_key};
use crate::wayback::Wayback;

/// The network guard needs at least this many attempts to trip.
pub const MIN_ATTEMPTS_FOR_GUARD: usize = 4;
pub const EVIDENCE_ATTEMPTS: usize = 20;
pub const EVIDENCE_ENTRIES: usize = 200;
const LINK_GUIDS: usize = 20;
pub const DISCOVERY_RECHECK_DAYS: i64 = 30;
const PROBE_CONCURRENCY: usize = 8;
pub const PRODUCTION_WAYBACK: &str = "https://archive.org";

#[derive(Debug, Clone, Default)]
pub struct RefreshOptions {
    /// Ignore the 30-minute freshness rule.
    pub force: bool,
    /// Restrict the fetch to these feeds (buried ones are still skipped). Diagnosis
    /// always covers every feed, because duplicate detection is cross-feed.
    pub only: Option<Vec<FeedId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Started {
        total: usize,
    },
    FeedDone {
        feed_id: FeedId,
        title: String,
        error: Option<String>,
    },
    NetworkUnreachable {
        failed: usize,
        attempted: usize,
    },
    Probing {
        what: String,
        total: usize,
    },
    WaybackDone {
        found: usize,
    },
    Finished {
        summary: Summary,
    },
}

pub trait EventSink {
    fn emit(&self, event: &Event);
}

pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _: &Event) {}
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub total: usize,
    /// Status name → number of active feeds.
    pub counts: BTreeMap<String, usize>,
    pub https_repairs: usize,
    pub redirect_repairs: usize,
    pub network_unreachable: bool,
}

impl Summary {
    pub fn count(&self, status: Status) -> usize {
        self.counts.get(status.as_str()).copied().unwrap_or(0)
    }

    pub fn safe_repairs(&self) -> usize {
        self.https_repairs + self.redirect_repairs
    }
}

pub struct Ressac {
    store: Store,
    source: LocalSource,
    wayback: Wayback,
    /// Pause between Wayback requests (1 request per second in production).
    wayback_gap: Duration,
}

impl Ressac {
    pub fn open(path: &Path) -> Result<Self> {
        Self::with_parts(
            Store::open(path)?,
            FetchLimits::default(),
            PRODUCTION_WAYBACK,
        )
    }

    pub fn with_parts(store: Store, limits: FetchLimits, wayback_base: &str) -> Result<Self> {
        let wayback_gap = limits.per_host_gap;
        Ok(Self {
            store,
            source: LocalSource::new(limits)?,
            wayback: Wayback::new(wayback_base)?,
            wayback_gap,
        })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn import_opml(&self, xml: &str, dry_run: bool) -> Result<ImportReport> {
        let outlines = opml::parse(xml)?;
        self.store.import_outlines(&outlines, dry_run, Utc::now())
    }

    /// Fetches due feeds, or the due feeds among `opts.only` (writing each attempt as it
    /// arrives), applies the network guard, probes https variants and autodiscovery, then
    /// diagnoses every active feed.
    pub async fn refresh(
        &self,
        opts: RefreshOptions,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<Summary> {
        let started = Utc::now();
        let run_id = ts(started);
        let mut due = self.store.feeds_to_fetch(started, opts.force)?;
        if let Some(only) = &opts.only {
            due.retain(|f| only.contains(&f.id));
        }
        let titles: HashMap<FeedId, String> = self
            .store
            .feeds()?
            .into_iter()
            .map(|f| (f.id, f.title))
            .collect();
        sink.emit(&Event::Started { total: due.len() });
        tracing::info!(feeds = due.len(), force = opts.force, "refresh started");

        let (tx, mut rx) = mpsc::channel(64);
        let source = self.source.clone();
        let lane_stop = stop.clone();
        let fetching = tokio::spawn(async move { source.refresh(due, tx, lane_stop).await });
        let (mut attempted, mut network_failures) = (0usize, 0usize);
        let mut fresh: HashMap<FeedId, ParsedFeed> = HashMap::new();
        while let Some(attempt) = rx.recv().await {
            attempted += 1;
            if attempt.error.as_ref().is_some_and(|e| e.is_network()) {
                network_failures += 1;
            }
            self.store.record_attempt(&run_id, &attempt)?;
            sink.emit(&Event::FeedDone {
                feed_id: attempt.feed_id,
                title: titles.get(&attempt.feed_id).cloned().unwrap_or_default(),
                error: attempt.error.as_ref().map(|e| e.label()),
            });
            if let Some(feed) = attempt.feed {
                fresh.insert(attempt.feed_id, feed);
            }
        }
        // A panicking lane only loses its own feeds; the rest of the run is still valid.
        if let Err(e) = fetching.await {
            tracing::error!(error = %e, "fetch lane failed");
        }

        let unreachable = attempted >= MIN_ATTEMPTS_FOR_GUARD && network_failures * 2 > attempted;
        if unreachable {
            self.store.mark_network_suspect(&run_id)?;
            tracing::warn!(
                network_failures,
                attempted,
                "network looks unreachable; failures not counted"
            );
            sink.emit(&Event::NetworkUnreachable {
                failed: network_failures,
                attempted,
            });
        } else if !stop.load(Ordering::Relaxed) {
            self.probe_https(&fresh, sink, stop.clone()).await?;
            if !stop.load(Ordering::Relaxed) {
                self.diagnose_all(Utc::now())?;
                self.discover_replacements(sink, Utc::now(), &stop).await?;
            }
        }
        let mut summary = self.diagnose_all(Utc::now())?;
        summary.network_unreachable = unreachable;
        tracing::info!(attempted, "refresh finished");
        sink.emit(&Event::Finished {
            summary: summary.clone(),
        });
        Ok(summary)
    }

    /// For freshly fetched `http://` feeds never probed: does `https://` serve the same feed?
    async fn probe_https(
        &self,
        fresh: &HashMap<FeedId, ParsedFeed>,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<()> {
        let candidates: Vec<FeedRef> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| {
                f.state == "active" && f.https_available.is_none() && fresh.contains_key(&f.id)
            })
            .filter_map(|f| {
                https_variant(&f.xml_url).map(|url| FeedRef {
                    id: f.id,
                    url,
                    etag: None,
                    last_modified: None,
                })
            })
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        sink.emit(&Event::Probing {
            what: "https".into(),
            total: candidates.len(),
        });
        let (tx, mut rx) = mpsc::channel(64);
        let source = self.source.clone();
        let probing = tokio::spawn(async move { source.refresh(candidates, tx, stop).await });
        while let Some(attempt) = rx.recv().await {
            if let Some(http) = fresh.get(&attempt.feed_id)
                && let Some(verdict) = https_verdict(&attempt, http)
            {
                self.store.set_https_available(attempt.feed_id, verdict)?;
            }
        }
        if let Err(e) = probing.await {
            tracing::error!(error = %e, "fetch lane failed");
        }
        Ok(())
    }

    /// For Dead feeds with a site address, looks for a replacement feed announced by the site.
    async fn discover_replacements(
        &self,
        sink: &dyn EventSink,
        now: DateTime<Utc>,
        stop: &AtomicBool,
    ) -> Result<()> {
        let diags = self.store.diagnoses()?;
        let recheck_before = now - chrono::Duration::days(DISCOVERY_RECHECK_DAYS);
        let candidates: Vec<FeedRow> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| {
                f.state == "active"
                    && f.html_url.is_some()
                    && diags.get(&f.id).is_some_and(|d| d.status == Status::Dead)
                    && f.discovery_checked_at
                        .as_deref()
                        .and_then(parse_ts)
                        .is_none_or(|t| t < recheck_before)
            })
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        sink.emit(&Event::Probing {
            what: "autodiscovery".into(),
            total: candidates.len(),
        });
        let source = &self.source;
        let found: Vec<(FeedId, Option<Option<String>>)> = stream::iter(candidates)
            .map(|f| async move {
                if stop.load(Ordering::Relaxed) {
                    return (f.id, None);
                }
                (f.id, discover_one(source, &f, now).await)
            })
            .buffer_unordered(PROBE_CONCURRENCY)
            .collect()
            .await;
        // `None` means the site could not be checked: leave it unstamped so the next run retries.
        for (id, checked) in found {
            if let Some(url) = checked {
                self.store.set_discovery(id, url.as_deref(), now)?;
            }
        }
        Ok(())
    }

    /// Diagnoses every active feed, stores the results and the garden's triage flags.
    pub fn diagnose_all(&self, now: DateTime<Utc>) -> Result<Summary> {
        let feeds: Vec<FeedRow> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| f.state == "active")
            .collect();
        let mut inputs = Vec::with_capacity(feeds.len());
        let mut evidence = Vec::with_capacity(feeds.len());
        for f in &feeds {
            let attempts = self.store.attempts(f.id, EVIDENCE_ATTEMPTS)?;
            let entries = self.store.entries(f.id, EVIDENCE_ENTRIES)?;
            inputs.push(LinkInput {
                feed_id: f.id,
                folder_id: f.folder_id,
                title: f.title.clone(),
                final_key: url_key(f.final_url.as_deref().unwrap_or(&f.xml_url)),
                guids: entries
                    .iter()
                    .take(LINK_GUIDS)
                    .map(|e| e.guid.clone())
                    .collect(),
            });
            evidence.push(FeedEvidence {
                feed_id: f.id,
                title: f.title.clone(),
                xml_url: f.xml_url.clone(),
                url_key: f.url_key.clone(),
                description: f
                    .description_current
                    .as_deref()
                    .or(f.description_original.as_deref())
                    .and_then(clean_description),
                attempts,
                entries,
                duplicate_of: None,
                discovered_url: f.discovered_url.clone(),
                https_available: f.https_available,
                now,
            });
        }
        let dups = find_duplicates(&inputs);
        let mut diags: HashMap<FeedId, Diagnosis> = evidence
            .into_iter()
            .map(|mut ev| {
                ev.duplicate_of = dups.get(&ev.feed_id).copied();
                (ev.feed_id, diagnose(&ev))
            })
            .collect();
        apply_similar_titles(&inputs, &mut diags);
        self.store.save_diagnoses(&diags, now)?;
        garden::recompute_triage(&self.store)?;
        self.summary()
    }

    /// Looks up a Wayback snapshot for Dead and Hijacked feeds that have none yet.
    pub async fn lookup_wayback(
        &self,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<usize> {
        let diags = self.store.diagnoses()?;
        let targets: Vec<(FeedId, String, Option<DateTime<Utc>>)> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| f.state == "active" && f.wayback_url.is_none())
            .filter_map(|f| {
                let d = diags.get(&f.id)?;
                matches!(d.status, Status::Dead | Status::Hijacked)
                    .then(|| {
                        f.html_url
                            .clone()
                            .map(|site| (f.id, site, d.cadence.last_entry_at))
                    })
                    .flatten()
            })
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        sink.emit(&Event::Probing {
            what: "wayback".into(),
            total: targets.len(),
        });
        let mut found = 0;
        for (i, (feed_id, site, around)) in targets.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if i > 0 {
                tokio::time::sleep(self.wayback_gap).await;
            }
            match self.wayback.closest(site, *around).await {
                Ok(Some(url)) => {
                    self.store.set_wayback(*feed_id, &url)?;
                    found += 1;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(feed_id, error = %e, "wayback lookup failed"),
            }
        }
        sink.emit(&Event::WaybackDone { found });
        Ok(found)
    }

    /// Status counts and safe repairs over active feeds (decisions are not considered).
    pub fn summary(&self) -> Result<Summary> {
        let diags = self.store.diagnoses()?;
        let mut summary = Summary::default();
        for f in self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| f.state == "active")
        {
            summary.total += 1;
            let d = diags.get(&f.id);
            let status = d.map_or(Status::Unchecked, |d| d.status);
            *summary
                .counts
                .entry(status.as_str().to_string())
                .or_default() += 1;
            match d.and_then(|d| d.action.as_ref()) {
                Some(Action::UpgradeHttps { .. }) => summary.https_repairs += 1,
                Some(Action::FollowRedirect { .. }) => summary.redirect_repairs += 1,
                _ => {}
            }
        }
        Ok(summary)
    }
}

/// What an https probe tells about a feed. `None` when the failure may be transient
/// (the answer stays unknown and the next run probes again).
fn https_verdict(attempt: &FetchAttempt, http: &ParsedFeed) -> Option<bool> {
    match (&attempt.feed, &attempt.error) {
        (Some(https), _) => Some(same_feed(https, http)),
        (None, Some(e)) if is_transient(e) => None,
        (None, _) => Some(false),
    }
}

/// Failures that say nothing about the site itself (a challenge page hides it from us).
fn is_transient(e: &ErrorKind) -> bool {
    matches!(
        e,
        ErrorKind::Dns | ErrorKind::Connect | ErrorKind::Timeout | ErrorKind::Challenge
    )
}

/// True when a fetch ended on an anti-bot gate (a `Challenge` error, or a 200 HTML page
/// that is the interstitial): the site was not really seen.
fn is_challenged(raw: &RawResponse) -> bool {
    raw.error == Some(ErrorKind::Challenge)
        || (raw.error.is_none()
            && looks_like_html(raw.content_type.as_deref(), &raw.body)
            && is_challenge_page(&raw.body[..raw.body.len().min(CHALLENGE_SNIFF_BYTES)]))
}

/// `Some(found)` once the site was checked (`found` is `None` when it announces nothing new);
/// `None` when it could not be checked (transient page-fetch failure, no site address).
async fn discover_one(
    source: &LocalSource,
    feed: &FeedRow,
    now: DateTime<Utc>,
) -> Option<Option<String>> {
    let page = source
        .fetch_raw(feed.html_url.as_deref()?, None, None)
        .await;
    if is_challenged(&page) {
        return None;
    }
    match &page.error {
        Some(e) if is_transient(e) => return None,
        Some(_) => return Some(None),
        None => {}
    }
    let html = String::from_utf8_lossy(&page.body);
    let Some(candidate) = alternate_feeds(&html, &page.final_url)
        .into_iter()
        .find(|u| url_key(u) != feed.url_key)
    else {
        return Some(None);
    };
    let raw = source.fetch_raw(&candidate, None, None).await;
    if is_challenged(&raw) {
        return None;
    }
    let parses = raw.error.is_none()
        && parse_feed(&raw.body, raw.content_type.as_deref(), &raw.final_url, now).is_ok();
    Some(parses.then_some(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ParsedEntry;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const CHALLENGE: &str =
        "<!DOCTYPE html><html><head><title>Just a moment...</title></head></html>";
    const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://example.org/</link><description>d</description><item><guid>1</guid><title>One</title></item></channel></rss>"#;

    fn row(html_url: String) -> FeedRow {
        FeedRow {
            id: 1,
            folder_id: None,
            position: 0,
            title: "T".into(),
            xml_url: "https://old.example/feed".into(),
            xml_url_original: "https://old.example/feed".into(),
            html_url: Some(html_url),
            url_key: url_key("https://old.example/feed"),
            final_url: None,
            description_original: None,
            description_current: None,
            etag: None,
            last_modified: None,
            state: "active".into(),
            merged_into_id: None,
            opml_attrs: "{}".into(),
            https_available: None,
            discovered_url: None,
            discovery_checked_at: None,
            wayback_url: None,
        }
    }

    async fn serve(server: &MockServer, at: &str, status: u16, body: &str, ct: &str) {
        Mock::given(wiremock::matchers::method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(status).set_body_raw(body.to_string(), ct))
            .mount(server)
            .await;
    }

    async fn discover(server: &MockServer) -> Option<Option<String>> {
        let source = LocalSource::new(FetchLimits::default()).unwrap();
        discover_one(&source, &row(format!("{}/", server.uri())), Utc::now()).await
    }

    #[tokio::test]
    async fn a_challenge_site_page_is_not_checked() {
        let server = MockServer::start().await;
        serve(&server, "/", 200, CHALLENGE, "text/html").await;
        assert_eq!(discover(&server).await, None);
    }

    #[tokio::test]
    async fn a_403_challenge_site_page_is_not_checked() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/",
            403,
            "<script>window._cf_chl_opt={}</script>",
            "text/html",
        )
        .await;
        assert_eq!(discover(&server).await, None);
    }

    #[tokio::test]
    async fn a_challenge_on_the_candidate_feed_is_not_checked() {
        let server = MockServer::start().await;
        let page = r#"<html><head><link rel="alternate" type="application/rss+xml" href="/new.xml"></head></html>"#;
        serve(&server, "/", 200, page, "text/html").await;
        serve(&server, "/new.xml", 200, CHALLENGE, "text/html").await;
        assert_eq!(discover(&server).await, None);
    }

    #[tokio::test]
    async fn a_plain_site_without_a_feed_is_checked_and_empty() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/",
            200,
            "<html><body>hi</body></html>",
            "text/html",
        )
        .await;
        assert_eq!(discover(&server).await, Some(None));
    }

    #[tokio::test]
    async fn a_parsing_candidate_is_found() {
        let server = MockServer::start().await;
        let page = r#"<html><head><link rel="alternate" type="application/rss+xml" href="/new.xml"></head></html>"#;
        serve(&server, "/", 200, page, "text/html").await;
        serve(&server, "/new.xml", 200, RSS, "application/rss+xml").await;
        assert_eq!(
            discover(&server).await,
            Some(Some(format!("{}/new.xml", server.uri())))
        );
    }

    fn feed(guids: &[&str]) -> ParsedFeed {
        ParsedFeed {
            title: Some("T".into()),
            description: None,
            site_url: None,
            entries: guids
                .iter()
                .map(|g| ParsedEntry {
                    guid: g.to_string(),
                    url: None,
                    title: None,
                    author: None,
                    published_at: Utc::now(),
                    published_at_is_fallback: false,
                    updated_at: None,
                    summary_html: None,
                    content_html: None,
                })
                .collect(),
        }
    }

    fn attempt(feed: Option<ParsedFeed>, error: Option<ErrorKind>) -> FetchAttempt {
        FetchAttempt {
            feed_id: 1,
            at: Utc::now(),
            requested_url: "https://x.example/feed".into(),
            final_url: "https://x.example/feed".into(),
            http_status: None,
            error,
            redirect_chain: vec![],
            content_type: None,
            bytes: 0,
            duration_ms: 0,
            not_modified: false,
            etag: None,
            last_modified: None,
            feed,
        }
    }

    #[test]
    fn https_verdict_compares_parsed_feeds() {
        let http = feed(&["a", "b", "c"]);
        assert_eq!(
            https_verdict(&attempt(Some(feed(&["a", "b", "c"])), None), &http),
            Some(true)
        );
        assert_eq!(
            https_verdict(&attempt(Some(feed(&["x", "y", "z"])), None), &http),
            Some(false)
        );
    }

    #[test]
    fn https_verdict_is_definitive_for_site_answers_only() {
        let http = feed(&["a"]);
        for e in [
            ErrorKind::Tls,
            ErrorKind::Http(404),
            ErrorKind::Parse,
            ErrorKind::Encoding,
            ErrorKind::HtmlInsteadOfFeed,
            ErrorKind::TooLarge,
        ] {
            assert_eq!(
                https_verdict(&attempt(None, Some(e.clone())), &http),
                Some(false),
                "{e:?}"
            );
        }
        for e in [
            ErrorKind::Dns,
            ErrorKind::Connect,
            ErrorKind::Timeout,
            ErrorKind::Challenge,
        ] {
            assert_eq!(
                https_verdict(&attempt(None, Some(e.clone())), &http),
                None,
                "{e:?}"
            );
        }
    }

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<Event>>);

    impl EventSink for Recorder {
        fn emit(&self, event: &Event) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[tokio::test]
    async fn a_subset_refresh_fetches_only_the_named_active_feeds_and_diagnoses_all() {
        let server = MockServer::start().await;
        let base = server.uri();
        serve(&server, "/gone.xml", 410, "", "text/plain").await;
        serve(&server, "/other.xml", 200, RSS, "application/rss+xml").await;
        serve(&server, "/buried.xml", 200, RSS, "application/rss+xml").await;
        let limits = FetchLimits {
            per_host_gap: Duration::ZERO,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits, &base).unwrap();
        let opml = format!(
            r#"<opml><body><outline text="Gone" xmlUrl="{base}/gone.xml"/>
            <outline text="Other" xmlUrl="{base}/other.xml"/>
            <outline text="Buried" xmlUrl="{base}/buried.xml"/></body></opml>"#
        );
        app.import_opml(&opml, false).unwrap();
        garden::decide(app.store(), 3, crate::model::Decision::Bury, Utc::now()).unwrap();

        let recorder = Recorder::default();
        let opts = RefreshOptions {
            force: false,
            only: Some(vec![1, 3]),
        };
        app.refresh(opts, &recorder, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();

        let events = recorder.0.lock().unwrap().clone();
        assert!(matches!(events.first(), Some(Event::Started { total: 1 })));
        let done: Vec<FeedId> = events
            .iter()
            .filter_map(|e| match e {
                Event::FeedDone { feed_id, .. } => Some(*feed_id),
                _ => None,
            })
            .collect();
        assert_eq!(done, [1], "the buried feed is skipped even when named");
        assert!(app.store().attempts(2, 1).unwrap().is_empty());
        assert!(app.store().attempts(3, 1).unwrap().is_empty());
        assert_eq!(
            app.store().diagnosis(1).unwrap().map(|d| d.status),
            Some(Status::Dead)
        );
        assert_eq!(
            app.store().diagnosis(2).unwrap().map(|d| d.status),
            Some(Status::Unchecked),
            "diagnosis covers every active feed"
        );
        let flag: bool = app
            .store()
            .conn()
            .query_row(
                "SELECT needs_triage FROM v_garden_row WHERE feed_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(flag, "diagnose_all stores the triage flags");
    }
}
