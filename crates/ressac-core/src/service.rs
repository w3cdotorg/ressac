//! The `Ressac` facade used by every interface: import, refresh (fetch → probes →
//! diagnosis), Wayback lookups and summaries.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::discover::{alternate_feeds, same_feed};
use crate::error::Result;
use crate::extract::{Article, ExtractError, extract_article};
use crate::garden;
use crate::health::links::{LinkInput, apply_similar_titles, find_duplicates};
use crate::health::{FeedEvidence, diagnose};
use crate::model::{Action, CadenceGroup, Diagnosis, ErrorKind, FeedId, Status, parse_ts, ts};
use crate::opml::{self, clean_description};
use crate::parse::{
    CHALLENGE_SNIFF_BYTES, ParsedFeed, is_challenge_page, looks_like_html, parse_feed,
};
use crate::source::{
    FeedRef, FetchAttempt, FetchLimits, HostPacer, LocalSource, RawResponse, Source,
};
use crate::store::pages::{self, PageKind, PageRow};
use crate::store::{CadenceRow, FeedRow, ImportReport, Store};
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
/// New entries extracted per `prefer_full_text` feed and refresh (spec §6).
pub const FULLTEXT_PER_FEED_PER_RUN: usize = 20;
/// The `what` of the `Probing` event announcing the full-article pass of a refresh.
pub const FULL_ARTICLES: &str = "full articles";

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
    /// One full-article extraction, stored either way. `error` is the stored sentence.
    Extracted {
        entry_id: i64,
        ok: bool,
        error: Option<String>,
    },
    /// A garden decision recorded by `ressac garden decide` (`keep`, `repair` or `bury`).
    Decided {
        feed_id: FeedId,
        decision: String,
    },
    /// `ressac garden undo`: the feed whose decision was undone, `None` when nothing was left.
    Undone {
        feed_id: Option<FeedId>,
    },
    /// One line of `ressac page list`, or the row `ressac page set` renumbered.
    Page(PageRow),
}

pub trait EventSink {
    fn emit(&self, event: &Event);
}

pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _: &Event) {}
}

/// What `Ressac::extract_entry` did. `NoUrl` and `UnknownEntry` store nothing and emit nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractOutcome {
    /// The article was extracted and stored.
    Saved,
    /// The failure sentence that was stored (for example "HTTP 404.").
    Failed(String),
    /// The entry has no link to fetch.
    NoUrl,
    /// No entry has this id.
    UnknownEntry,
}

/// What fetching and extracting one article page gave.
type Extraction = std::result::Result<Article, ExtractError>;

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
    /// Fetches URLs read from third-party content (article pages, feed addresses announced by
    /// a site page): refuses local and private targets.
    articles: LocalSource,
    limits: FetchLimits,
    wayback: Wayback,
    /// Pause between two requests to the same host: Wayback lookups, and article pages after
    /// any request of the run to their host (1 request per second in production).
    host_gap: Duration,
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
        let host_gap = limits.per_host_gap;
        Ok(Self {
            store,
            source: LocalSource::new(limits.clone())?,
            articles: LocalSource::for_articles(limits.clone(), false)?,
            limits,
            wayback: Wayback::new(wayback_base)?,
            host_gap,
        })
    }

    /// Lets article pages and autodiscovered feed addresses be fetched from local and private
    /// addresses. For tests against a local mock server and for the
    /// `RESSAC_ALLOW_PRIVATE_ARTICLES` test hook of the CLI.
    pub fn allow_private_articles(mut self) -> Result<Self> {
        self.articles = LocalSource::for_articles(self.limits.clone(), true)?;
        Ok(self)
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
        let mut pacer = HostPacer::new(self.host_gap);
        while let Some(attempt) = rx.recv().await {
            attempted += 1;
            pacer.note_attempt(&attempt, Instant::now());
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
            self.probe_https(&fresh, &mut pacer, sink, stop.clone())
                .await?;
            if !stop.load(Ordering::Relaxed) {
                // Triage waits for the final diagnosis below.
                self.save_fresh_diagnoses(Utc::now())?;
                self.discover_replacements(sink, Utc::now(), &stop).await?;
            }
        }
        if !unreachable && !stop.load(Ordering::Relaxed) {
            let mut fetched: Vec<FeedId> = fresh.keys().copied().collect();
            fetched.sort_unstable();
            self.fetch_full_texts(&fetched, started, &mut pacer, sink, &stop)
                .await?;
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
    /// Each probe is noted in `pacer`.
    async fn probe_https(
        &self,
        fresh: &HashMap<FeedId, ParsedFeed>,
        pacer: &mut HostPacer,
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
            pacer.note_attempt(&attempt, Instant::now());
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
        let (site, guarded) = (&self.source, &self.articles);
        let found: Vec<(FeedId, Option<Option<String>>)> = stream::iter(candidates)
            .map(|f| async move {
                if stop.load(Ordering::Relaxed) {
                    return (f.id, None);
                }
                (f.id, discover_one(site, guarded, &f, now).await)
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

    /// Extracts the entries this run stored for `prefer_full_text` feeds, at most
    /// `FULLTEXT_PER_FEED_PER_RUN` per feed. Stops between entries on request, and keeps
    /// `host_gap` between two requests to the same host, counting those already in `pacer`.
    async fn fetch_full_texts(
        &self,
        fetched: &[FeedId],
        since: DateTime<Utc>,
        pacer: &mut HostPacer,
        sink: &dyn EventSink,
        stop: &AtomicBool,
    ) -> Result<()> {
        let mut queue = Vec::new();
        for &feed_id in fetched {
            if self.store.prefer_full_text(feed_id)? {
                queue.extend(self.store.entries_needing_fulltext(
                    feed_id,
                    since,
                    FULLTEXT_PER_FEED_PER_RUN,
                )?);
            }
        }
        if queue.is_empty() {
            return Ok(());
        }
        sink.emit(&Event::Probing {
            what: FULL_ARTICLES.into(),
            total: queue.len(),
        });
        tracing::info!(entries = queue.len(), "fetching full articles");
        for (entry_id, url) in queue {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let wait = pacer.delay(&url, Instant::now());
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            let outcome = fetch_article(&self.articles, &url).await;
            pacer.note(&url, Instant::now());
            if let Err(e) = self.save_extraction(entry_id, &outcome, sink) {
                // One entry's store error must not end the pass or the refresh.
                tracing::warn!(entry_id, error = %e, "storing a full article failed");
            }
        }
        Ok(())
    }

    /// Diagnoses every active feed, stores the results and the garden's triage flags.
    pub fn diagnose_all(&self, now: DateTime<Utc>) -> Result<Summary> {
        self.save_fresh_diagnoses(now)?;
        garden::recompute_triage(&self.store)?;
        self.summary()
    }

    /// Diagnoses every active feed and stores the results, leaving the triage flags alone.
    fn save_fresh_diagnoses(&self, now: DateTime<Utc>) -> Result<()> {
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
        self.store.save_diagnoses(&diags, now)
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
                tokio::time::sleep(self.host_gap).await;
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

    /// Fetches the entry's page, extracts the article and stores the outcome (spec §6).
    /// A failure is stored too, as one English sentence; a later call retries.
    pub async fn extract_entry(
        &self,
        entry_id: i64,
        sink: &dyn EventSink,
    ) -> Result<ExtractOutcome> {
        let Some((_, url)) = self.store.entry_for_extract(entry_id)? else {
            return Ok(ExtractOutcome::UnknownEntry);
        };
        let Some(url) = url.filter(|u| !u.trim().is_empty()) else {
            return Ok(ExtractOutcome::NoUrl);
        };
        let outcome = fetch_article(&self.articles, &url).await;
        Ok(match self.save_extraction(entry_id, &outcome, sink)? {
            None => ExtractOutcome::Saved,
            Some(sentence) => ExtractOutcome::Failed(sentence),
        })
    }

    /// Stores one extraction outcome and emits `Extracted`. Returns the failure sentence.
    fn save_extraction(
        &self,
        entry_id: i64,
        outcome: &Extraction,
        sink: &dyn EventSink,
    ) -> Result<Option<String>> {
        self.store.save_fulltext(entry_id, outcome, Utc::now())?;
        let error = outcome.as_ref().err().map(ExtractError::sentence);
        tracing::info!(entry_id, error = error.as_deref(), "full article");
        sink.emit(&Event::Extracted {
            entry_id,
            ok: error.is_none(),
            error: error.clone(),
        });
        Ok(error)
    }

    /// Every teletext page: the reserved ones, then each numbered folder and feed.
    pub fn pages(&self) -> Result<Vec<PageRow>> {
        pages::list(self.store.conn())
    }

    /// Gives a folder or a feed another page; see `store::pages::set` for the refusals.
    pub fn set_page(&self, kind: PageKind, id: i64, page: u16) -> Result<()> {
        pages::set(self.store.conn(), kind, id, page)
    }

    /// Forces a feed's rhythm group, or gives it back to the computed one (`None`); see
    /// `Store::set_cadence_override` for the refusals. Returns the feed's row afterwards.
    pub fn set_cadence(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<CadenceRow> {
        self.store.set_cadence_override(feed_id, group)?;
        self.store.cadence_row(feed_id)
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

/// Fetches an article page through the article source: the feed client's user agent, body
/// cap and timeout (the whole exchange, redirects included), plus the public-address guard.
async fn fetch_article(source: &LocalSource, url: &str) -> Extraction {
    article_from(&source.fetch_raw(url, None, None).await)
}

/// Maps a page fetch to an extraction outcome. A 401 asks for credentials, so it reads as a
/// login wall; other fetch errors keep their kind ("HTTP 404.", "Timeout.").
fn article_from(raw: &RawResponse) -> Extraction {
    if raw.refused {
        return Err(ExtractError::PrivateAddress);
    }
    if is_challenged(raw) {
        return Err(ExtractError::Challenge);
    }
    match &raw.error {
        Some(ErrorKind::Http(401)) => return Err(ExtractError::LoginWall),
        Some(kind) => return Err(ExtractError::Fetch(kind.clone())),
        None => {}
    }
    if !looks_like_html(raw.content_type.as_deref(), &raw.body) {
        return Err(ExtractError::NoContent);
    }
    extract_article(&String::from_utf8_lossy(&raw.body), &raw.final_url)
}

/// `Some(found)` once the site was checked (`found` is `None` when it announces nothing new);
/// `None` when it could not be checked (transient page-fetch failure, no site address).
/// The site address comes from the user's OPML and goes through `site`, like feeds; the feed
/// address the page announces is third-party content and goes through `guarded`, which
/// refuses local and private targets (a refused candidate counts as nothing found).
async fn discover_one(
    site: &LocalSource,
    guarded: &LocalSource,
    feed: &FeedRow,
    now: DateTime<Utc>,
) -> Option<Option<String>> {
    let page = site.fetch_raw(feed.html_url.as_deref()?, None, None).await;
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
    let raw = guarded.fetch_raw(&candidate, None, None).await;
    if raw.refused {
        return Some(None);
    }
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
        let site = LocalSource::new(FetchLimits::default()).unwrap();
        let candidates = LocalSource::for_articles(FetchLimits::default(), true).unwrap();
        discover_one(
            &site,
            &candidates,
            &row(format!("{}/", server.uri())),
            Utc::now(),
        )
        .await
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

    /// SSRF guard: the feed address a site page announces is third-party content, so one on
    /// a local address is refused without being requested, and the site counts as checked.
    #[tokio::test]
    async fn a_candidate_on_a_local_address_is_refused_without_a_request() {
        let server = MockServer::start().await;
        let page = r#"<html><head><link rel="alternate" type="application/rss+xml" href="/new.xml"></head></html>"#;
        serve(&server, "/", 200, page, "text/html").await;
        serve(&server, "/new.xml", 200, RSS, "application/rss+xml").await;
        let site = LocalSource::new(FetchLimits::default()).unwrap();
        let guarded = LocalSource::for_articles(FetchLimits::default(), false).unwrap();
        let found = discover_one(
            &site,
            &guarded,
            &row(format!("{}/", server.uri())),
            Utc::now(),
        )
        .await;
        assert_eq!(found, Some(None));
        let paths: Vec<String> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect();
        assert_eq!(paths, ["/"], "only the site page is requested");
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

    /// A refresh diagnoses twice; only its last diagnosis, `diagnose_all`, writes the triage
    /// flags, so the first one leaves them alone.
    #[test]
    fn only_diagnose_all_writes_the_triage_flags() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            false,
        )
        .unwrap();
        let flag = |app: &Ressac| -> bool {
            app.store()
                .conn()
                .query_row(
                    "SELECT needs_triage FROM v_garden_row WHERE feed_id = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        app.save_fresh_diagnoses(Utc::now()).unwrap();
        // A stale flag on the Unchecked feed shows who writes it.
        app.store()
            .conn()
            .execute(
                "UPDATE feed_health SET needs_triage = 1 WHERE feed_id = 1",
                [],
            )
            .unwrap();
        app.save_fresh_diagnoses(Utc::now()).unwrap();
        assert!(flag(&app), "the first diagnosis leaves the flag alone");
        app.diagnose_all(Utc::now()).unwrap();
        assert!(!flag(&app), "diagnose_all recomputes it");
    }

    #[test]
    fn reading_and_garden_events_have_stable_json() {
        let json = |e: Event| serde_json::to_value(e).unwrap();
        assert_eq!(
            json(Event::Extracted {
                entry_id: 3,
                ok: false,
                error: Some("HTTP 404.".into()),
            }),
            serde_json::json!({"event": "extracted", "entry_id": 3, "ok": false, "error": "HTTP 404."})
        );
        assert_eq!(
            json(Event::Extracted {
                entry_id: 4,
                ok: true,
                error: None,
            }),
            serde_json::json!({"event": "extracted", "entry_id": 4, "ok": true, "error": null})
        );
        assert_eq!(
            json(Event::Decided {
                feed_id: 2,
                decision: "bury".into(),
            }),
            serde_json::json!({"event": "decided", "feed_id": 2, "decision": "bury"})
        );
        assert_eq!(
            json(Event::Undone { feed_id: None }),
            serde_json::json!({"event": "undone", "feed_id": null})
        );
        assert_eq!(
            json(Event::Probing {
                what: FULL_ARTICLES.into(),
                total: FULLTEXT_PER_FEED_PER_RUN,
            }),
            serde_json::json!({"event": "probing", "what": "full articles", "total": 20})
        );
    }

    #[test]
    fn page_events_have_stable_json() {
        let row = PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: Some(400),
        };
        assert_eq!(
            serde_json::to_string(&Event::Page(row)).unwrap(),
            r#"{"event":"page","page":412,"kind":"feed","id":12,"title":"Courrier international","parent_page":400}"#
        );
        let reserved = PageRow {
            page: 899,
            kind: PageKind::Reserved,
            id: None,
            title: "Guide".into(),
            parent_page: None,
        };
        assert_eq!(
            serde_json::to_value(Event::Page(reserved)).unwrap(),
            serde_json::json!({"event": "page", "page": 899, "kind": "reserved", "id": null, "title": "Guide", "parent_page": null})
        );
    }

    #[test]
    fn pages_and_set_page_go_through_the_store() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="Infos"><outline text="A" xmlUrl="https://a.example.org/feed"/></outline></body></opml>"#,
            false,
        )
        .unwrap();
        let numbered = |app: &Ressac| -> Vec<(u16, PageKind, Option<i64>)> {
            app.pages()
                .unwrap()
                .into_iter()
                .skip(pages::RESERVED_PAGES.len())
                .map(|r| (r.page, r.kind, r.id))
                .collect()
        };
        assert_eq!(
            numbered(&app),
            [
                (200, PageKind::Folder, Some(1)),
                (201, PageKind::Feed, Some(1))
            ]
        );
        app.set_page(PageKind::Feed, 1, 412).unwrap();
        assert_eq!(numbered(&app)[1], (412, PageKind::Feed, Some(1)));
        assert!(matches!(
            app.set_page(PageKind::Feed, 1, 200),
            Err(crate::Error::PageTaken { page: 200 })
        ));
    }

    #[test]
    fn set_cadence_goes_through_the_store_and_returns_the_row() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            false,
        )
        .unwrap();
        let row = app.set_cadence(1, Some(CadenceGroup::Weekly)).unwrap();
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            serde_json::json!({"feed_id": 1, "effective_group": "weekly", "computed_group": "silent", "is_override": true})
        );
        assert!(!app.set_cadence(1, None).unwrap().is_override);
        assert!(matches!(
            app.set_cadence(7, None).unwrap_err(),
            crate::error::Error::UnknownFeed(7)
        ));
    }

    const PARA: &str = "The tide comes in twice a day, pulled by the moon and the sun, and the shore keeps a careful record of every visit in sand, shells and drifting weed.";

    /// A page `dom_smoothie` extracts (checked against 0.18.2: about 2,500 characters of text).
    fn article_page() -> String {
        let body: String = (0..8)
            .map(|i| format!("<p>Paragraph {i}. {PARA} {PARA}</p>"))
            .collect();
        format!(
            "<!DOCTYPE html><html><head><title>Tides | Shore Notes</title><meta name=\"author\" content=\"Ada Lovelace\"></head><body><nav><a href=\"/\">Home</a></nav><article><h1>Tides</h1>{body}</article><footer>Footer</footer></body></html>"
        )
    }

    fn parsed_entry(guid: &str, url: Option<String>, title: Option<&str>) -> ParsedEntry {
        ParsedEntry {
            guid: guid.into(),
            url,
            title: title.map(Into::into),
            author: None,
            published_at: Utc::now(),
            published_at_is_fallback: false,
            updated_at: None,
            summary_html: Some("<p>Only a summary.</p>".into()),
            content_html: None,
        }
    }

    /// An app whose feed #1 holds `linked` (→ `{base}/article`) and `bare` (no link, no
    /// title, only a summary). Returns the app and both entry ids.
    fn app_with_entries(base: &str) -> (Ressac, i64, i64) {
        let (app, linked, bare) = guarded_app_with_entries(base);
        (app.allow_private_articles().unwrap(), linked, bare)
    }

    /// Like `app_with_entries`, with the production public-address guard still on.
    fn guarded_app_with_entries(base: &str) -> (Ressac, i64, i64) {
        let store = Store::open_in_memory().unwrap();
        let xml =
            r#"<opml><body><outline text="T" xmlUrl="https://t.example/feed"/></body></opml>"#;
        store
            .import_outlines(&opml::parse(xml).unwrap(), false, Utc::now())
            .unwrap();
        let parsed = ParsedFeed {
            title: Some("T".into()),
            description: None,
            site_url: None,
            entries: vec![
                parsed_entry("linked", Some(format!("{base}/article")), Some("Tides")),
                parsed_entry("bare", None, None),
            ],
        };
        store
            .record_attempt("r", &attempt(Some(parsed), None))
            .unwrap();
        let id = |guid: &str| -> i64 {
            store
                .conn()
                .query_row("SELECT id FROM entry WHERE guid = ?1", [guid], |r| r.get(0))
                .unwrap()
        };
        let (linked, bare) = (id("linked"), id("bare"));
        let limits = FetchLimits {
            per_host_gap: Duration::ZERO,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        let app = Ressac::with_parts(store, limits, "http://127.0.0.1:9").unwrap();
        (app, linked, bare)
    }

    fn raw(status: Option<u16>, error: Option<ErrorKind>, ct: &str, body: &str) -> RawResponse {
        RawResponse {
            status,
            error,
            final_url: "https://x.example/a".into(),
            content_type: Some(ct.into()),
            body: body.as_bytes().to_vec(),
            ..RawResponse::default()
        }
    }

    #[test]
    fn article_from_maps_fetch_outcomes() {
        let err = |r: RawResponse| article_from(&r).err();
        assert_eq!(
            err(raw(Some(403), Some(ErrorKind::Challenge), "text/html", "")),
            Some(ExtractError::Challenge)
        );
        assert_eq!(
            err(raw(Some(200), None, "text/html", CHALLENGE)),
            Some(ExtractError::Challenge),
            "a 200 interstitial is a challenge too"
        );
        assert_eq!(
            err(raw(Some(401), Some(ErrorKind::Http(401)), "text/html", "")),
            Some(ExtractError::LoginWall)
        );
        assert_eq!(
            err(raw(Some(404), Some(ErrorKind::Http(404)), "text/html", "")),
            Some(ExtractError::Fetch(ErrorKind::Http(404)))
        );
        assert_eq!(
            err(raw(None, Some(ErrorKind::Timeout), "", "")),
            Some(ExtractError::Fetch(ErrorKind::Timeout))
        );
        assert_eq!(
            err(raw(Some(200), None, "application/pdf", "%PDF-1.7")),
            Some(ExtractError::NoContent)
        );
        let article = article_from(&raw(Some(200), None, "text/html", &article_page())).unwrap();
        assert!(article.html.contains("Paragraph 7."), "{}", article.html);
    }

    #[tokio::test]
    async fn extract_entry_saves_the_article_and_reports_it() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/article",
            200,
            &article_page(),
            "text/html; charset=utf-8",
        )
        .await;
        let (app, linked, _) = app_with_entries(&server.uri());
        let recorder = Recorder::default();
        assert_eq!(
            app.extract_entry(linked, &recorder).await.unwrap(),
            ExtractOutcome::Saved
        );
        let row = app
            .store()
            .fulltext(linked)
            .unwrap()
            .expect("a stored article");
        assert!(row.html.as_deref().unwrap().contains("Paragraph 7."));
        assert_eq!(row.error, None);
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![Event::Extracted {
                entry_id: linked,
                ok: true,
                error: None,
            }]
        );
    }

    #[tokio::test]
    async fn a_failed_extraction_is_stored_and_a_retry_replaces_it() {
        let server = MockServer::start().await;
        serve(&server, "/article", 404, "gone", "text/plain").await;
        let (app, linked, _) = app_with_entries(&server.uri());
        let recorder = Recorder::default();
        assert_eq!(
            app.extract_entry(linked, &recorder).await.unwrap(),
            ExtractOutcome::Failed("HTTP 404.".into())
        );
        let failed = app.store().fulltext(linked).unwrap().unwrap();
        assert_eq!(failed.error.as_deref(), Some("HTTP 404."));
        assert_eq!(failed.html, None);

        server.reset().await;
        serve(&server, "/article", 200, &article_page(), "text/html").await;
        assert_eq!(
            app.extract_entry(linked, &recorder).await.unwrap(),
            ExtractOutcome::Saved
        );
        let saved = app.store().fulltext(linked).unwrap().unwrap();
        assert!(saved.html.is_some());
        assert_eq!(saved.error, None);
        assert_eq!(
            recorder.0.lock().unwrap()[0],
            Event::Extracted {
                entry_id: linked,
                ok: false,
                error: Some("HTTP 404.".into()),
            }
        );
    }

    #[tokio::test]
    async fn a_challenge_page_is_stored_as_such() {
        let server = MockServer::start().await;
        serve(&server, "/article", 200, CHALLENGE, "text/html").await;
        let (app, linked, _) = app_with_entries(&server.uri());
        assert_eq!(
            app.extract_entry(linked, &NullSink).await.unwrap(),
            ExtractOutcome::Failed("Anti-bot challenge page.".into())
        );
    }

    /// A store error on one entry is logged; the pass goes on with the next entries.
    #[tokio::test]
    async fn a_store_error_on_one_entry_does_not_end_the_full_text_pass() {
        let server = MockServer::start().await;
        serve(&server, "/article", 200, &article_page(), "text/html").await;
        let (app, linked, _) = app_with_entries(&server.uri());
        let more = ParsedFeed {
            title: Some("T".into()),
            description: None,
            site_url: None,
            entries: vec![parsed_entry(
                "second",
                Some(format!("{}/article", server.uri())),
                Some("Second"),
            )],
        };
        app.store()
            .record_attempt("r2", &attempt(Some(more), None))
            .unwrap();
        let second: i64 = app
            .store()
            .conn()
            .query_row("SELECT id FROM entry WHERE guid = 'second'", [], |r| {
                r.get(0)
            })
            .unwrap();
        app.store().set_prefer_full_text(1, true).unwrap();
        app.store()
            .conn()
            .execute_batch(&format!(
                "CREATE TRIGGER boom BEFORE INSERT ON entry_fulltext WHEN NEW.entry_id = {linked} \
                 BEGIN SELECT RAISE(ABORT, 'disk on fire'); END;"
            ))
            .unwrap();
        let stop = AtomicBool::new(false);
        let since = "2000-01-01T00:00:00Z".parse().unwrap();
        app.fetch_full_texts(
            &[1],
            since,
            &mut HostPacer::new(Duration::ZERO),
            &NullSink,
            &stop,
        )
        .await
        .expect("the pass survives a store error");
        assert!(app.store().fulltext(linked).unwrap().is_none());
        assert!(app.store().fulltext(second).unwrap().is_some());
    }

    /// The first article of a feed waits out the per-host gap after that feed's fetch (and
    /// its https probe): the pause used to count from the previous article only.
    #[tokio::test]
    async fn the_first_article_waits_out_the_gap_after_its_feed() {
        let server = MockServer::start().await;
        let base = server.uri();
        let rss = format!(
            r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>{base}/</link><description>d</description><item><guid>one</guid><title>One</title><link>{base}/article</link></item></channel></rss>"#
        );
        serve(&server, "/feed.xml", 200, &rss, "application/rss+xml").await;
        serve(&server, "/article", 200, &article_page(), "text/html").await;
        let gap = Duration::from_millis(400);
        let limits = FetchLimits {
            per_host_gap: gap,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits, &base)
            .unwrap()
            .allow_private_articles()
            .unwrap();
        app.import_opml(
            &format!(r#"<opml><body><outline text="T" xmlUrl="{base}/feed.xml"/></body></opml>"#),
            false,
        )
        .unwrap();
        app.store().set_prefer_full_text(1, true).unwrap();

        let started = std::time::Instant::now();
        app.refresh(
            RefreshOptions::default(),
            &NullSink,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        let elapsed = started.elapsed();

        let entry: i64 = app
            .store()
            .conn()
            .query_row("SELECT id FROM entry WHERE guid = 'one'", [], |r| r.get(0))
            .unwrap();
        assert!(
            app.store().fulltext(entry).unwrap().is_some(),
            "the full-article pass ran"
        );
        assert!(
            elapsed >= gap,
            "the article followed its feed without a pause: {elapsed:?}"
        );
    }

    /// Review Focus 1: an entry with no link (and no title) cannot be extracted; nothing is
    /// stored or emitted, and the CLI turns this into exit 2 (Phase 5).
    #[tokio::test]
    async fn extract_without_url_is_a_usage_error() {
        let (app, _, bare) = app_with_entries("http://127.0.0.1:9");
        let recorder = Recorder::default();
        assert_eq!(
            app.extract_entry(bare, &recorder).await.unwrap(),
            ExtractOutcome::NoUrl
        );
        assert!(app.store().fulltext(bare).unwrap().is_none());
        assert!(recorder.0.lock().unwrap().is_empty());
    }

    /// SSRF guard: a feed-controlled entry URL on a local address is never requested.
    #[tokio::test]
    async fn extract_refuses_local_addresses_without_sending_a_request() {
        let server = MockServer::start().await;
        serve(&server, "/article", 200, &article_page(), "text/html").await;
        let (app, linked, _) = guarded_app_with_entries(&server.uri());
        let recorder = Recorder::default();
        assert_eq!(
            app.extract_entry(linked, &recorder).await.unwrap(),
            ExtractOutcome::Failed("The page is on a local or private network.".into())
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        let row = app.store().fulltext(linked).unwrap().unwrap();
        assert_eq!(
            row.error.as_deref(),
            Some("The page is on a local or private network.")
        );
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![Event::Extracted {
                entry_id: linked,
                ok: false,
                error: Some("The page is on a local or private network.".into()),
            }]
        );
    }

    #[test]
    fn article_from_maps_a_refused_fetch() {
        let refused = RawResponse {
            refused: true,
            ..RawResponse::default()
        };
        assert_eq!(
            article_from(&refused).err(),
            Some(ExtractError::PrivateAddress)
        );
    }

    #[tokio::test]
    async fn extract_of_an_unknown_entry_is_reported() {
        let (app, _, _) = app_with_entries("http://127.0.0.1:9");
        assert_eq!(
            app.extract_entry(9_999, &NullSink).await.unwrap(),
            ExtractOutcome::UnknownEntry
        );
    }
}
