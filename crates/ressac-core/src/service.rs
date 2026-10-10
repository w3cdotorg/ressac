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
use crate::health::links::{LinkInput, apply_similar_titles, find_duplicates};
use crate::health::{FeedEvidence, diagnose};
use crate::model::{Action, Diagnosis, FeedId, Status, parse_ts, ts};
use crate::opml::{self, clean_description};
use crate::parse::{ParsedFeed, parse_feed};
use crate::source::{FeedRef, FetchLimits, LocalSource, Source};
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

#[derive(Debug, Clone, Copy, Default)]
pub struct RefreshOptions {
    pub force: bool,
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

    /// Fetches due feeds (writing each attempt as it arrives), applies the network guard,
    /// probes https variants and autodiscovery, then diagnoses every active feed.
    pub async fn refresh(
        &self,
        opts: RefreshOptions,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<Summary> {
        let started = Utc::now();
        let run_id = ts(started);
        let due = self.store.feeds_to_fetch(started, opts.force)?;
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
        let _ = fetching.await;

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
            self.diagnose_all(Utc::now())?;
            self.discover_replacements(sink, Utc::now()).await?;
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
            let same = match (&attempt.feed, fresh.get(&attempt.feed_id)) {
                (Some(https), Some(http)) => same_feed(https, http),
                _ => false,
            };
            self.store.set_https_available(attempt.feed_id, same)?;
        }
        let _ = probing.await;
        Ok(())
    }

    /// For Dead feeds with a site address, looks for a replacement feed announced by the site.
    async fn discover_replacements(&self, sink: &dyn EventSink, now: DateTime<Utc>) -> Result<()> {
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
        let found: Vec<(FeedId, Option<String>)> = stream::iter(candidates)
            .map(|f| async move { (f.id, discover_one(source, &f, now).await) })
            .buffer_unordered(PROBE_CONCURRENCY)
            .collect()
            .await;
        for (id, url) in found {
            self.store.set_discovery(id, url.as_deref(), now)?;
        }
        Ok(())
    }

    /// Diagnoses every active feed and stores the results.
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

async fn discover_one(source: &LocalSource, feed: &FeedRow, now: DateTime<Utc>) -> Option<String> {
    let page = source
        .fetch_raw(feed.html_url.as_deref()?, None, None)
        .await;
    if page.error.is_some() {
        return None;
    }
    let html = String::from_utf8_lossy(&page.body);
    let candidate = alternate_feeds(&html, &page.final_url)
        .into_iter()
        .find(|u| url_key(u) != feed.url_key)?;
    let raw = source.fetch_raw(&candidate, None, None).await;
    let parses = raw.error.is_none()
        && parse_feed(&raw.body, raw.content_type.as_deref(), &raw.final_url, now).is_ok();
    parses.then_some(candidate)
}
