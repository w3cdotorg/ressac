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
use crate::edition::{Caps, Composition, Section, day_start};
use crate::error::{Error, Result};
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
use crate::store::{
    CadenceRow, EditionParams, EditionRecord, FeedRow, ImportReport, NewFeed, Store, ThemeSource,
};
pub use crate::store::{EditionEntryRow, EditionOverflowRow, EditionView};
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
/// The largest image the terminal reader downloads (the app's mosaic limit, spec §7).
pub const IMAGE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// How long one image may take, redirects and body included (spec §7).
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(15);

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
    /// `ressac edition build`: the edition just written.
    Edition {
        id: i64,
        entries: usize,
        front: usize,
        week: usize,
        wire: usize,
        overflow: u32,
        unreachable: u32,
    },
}

pub trait EventSink {
    fn emit(&self, event: &Event);
}

pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _: &Event) {}
}

/// Passes every event on and counts the feeds whose fetch failed.
struct FailureCounter<'a> {
    inner: &'a dyn EventSink,
    failed: std::sync::atomic::AtomicU32,
}

impl EventSink for FailureCounter<'_> {
    fn emit(&self, event: &Event) {
        if let Event::FeedDone { error: Some(_), .. } = event {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        self.inner.emit(event);
    }
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

/// What `Ressac::build_edition` is asked to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditionOptions {
    pub caps: Caps,
    /// Replace today's edition, keeping its window start.
    pub rebuild: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionReport {
    pub id: i64,
    pub entries: usize,
    pub front: usize,
    pub week: usize,
    pub wire: usize,
    /// Sum of the overflow counts: eligible entries the caps left in their feeds.
    pub overflow: u32,
    /// Feeds whose fetch failed during the build's refresh.
    pub unreachable: u32,
}

impl EditionReport {
    pub fn new(record: &EditionRecord, comp: &Composition) -> Self {
        let count = |s: Section| comp.entries.iter().filter(|p| p.section == s).count();
        EditionReport {
            id: record.id,
            entries: comp.entries.len(),
            front: count(Section::Front),
            week: count(Section::Week),
            wire: count(Section::Wire),
            overflow: comp.overflow.iter().map(|o| o.hidden_count).sum(),
            unreachable: record.unreachable,
        }
    }

    pub fn event(&self) -> Event {
        Event::Edition {
            id: self.id,
            entries: self.entries,
            front: self.front,
            week: self.week,
            wire: self.wire,
            overflow: self.overflow,
            unreachable: self.unreachable,
        }
    }
}

/// How a build ended. Only `Built` wrote anything.
#[derive(Debug, Clone, PartialEq)]
pub enum EditionOutcome {
    Built(EditionReport),
    /// An edition already exists since today's 04:00 (and `rebuild` was not asked).
    NotDue,
    /// The refresh found the network unreachable: no edition, the window stays where it was.
    Offline,
    /// The stop flag was raised during the refresh: no edition.
    Interrupted,
}

/// What `Ressac::add_feed` did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddedFeed {
    pub id: FeedId,
    pub title: String,
    pub xml_url: String,
    pub page: Option<u16>,
    /// The theme's display name.
    pub theme: Option<String>,
    pub theme_source: ThemeSource,
    pub territory: Option<String>,
    /// The address typed was a site page and the feed was found through it.
    pub discovered: bool,
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

    /// Adds one feed from `input`, a feed address or a site page whose first announced feed
    /// is taken (spec §4). Everything fetched goes through the guarded client: public web
    /// addresses only. Refuses an address already followed or buried, then inserts the feed
    /// at the bottom of the root (with the theme `theme` names, else its site's) and fetches
    /// it right away; a failed fetch keeps the feed.
    pub async fn add_feed(
        &self,
        input: &str,
        theme: Option<&str>,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<AddedFeed> {
        // Refused before any network work; the insert checks it again in its transaction.
        let theme = theme
            .map(|given| self.store.resolve_theme(given))
            .transpose()?;
        let theme = theme.as_ref().map(|t| t.key.as_str());
        let typed = normalise_address(input)?;
        if let Some((id, title, buried)) = self.store.find_duplicate(&[&typed])? {
            return Err(Error::FeedExists { id, title, buried });
        }
        let now = Utc::now();
        let page = fetch_guarded(&self.articles, &typed).await?;
        // The feed's address as resolved (the typed one, or the announced link); the address
        // a fetch ended at is only the base of what it contains. Every address the feed
        // answers to is checked for duplicates, so a new address that redirects to a stored
        // feed is found too.
        let (feed, xml_url, site_url, discovered, known_as) = match parse_feed(
            &page.body,
            page.content_type.as_deref(),
            &page.final_url,
            now,
        ) {
            Ok(feed) => (
                feed,
                typed.clone(),
                None,
                false,
                vec![page.final_url.clone()],
            ),
            Err(_) => {
                let html = String::from_utf8_lossy(&page.body);
                let first = alternate_feeds(&html, &page.final_url)
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::NoFeedFound(typed.clone()))?;
                if let Some((id, title, buried)) = self.store.find_duplicate(&[&first])? {
                    return Err(Error::FeedExists { id, title, buried });
                }
                let raw = fetch_guarded(&self.articles, &first)
                    .await
                    .map_err(|_| Error::NoFeedFound(first.clone()))?;
                let feed = parse_feed(&raw.body, raw.content_type.as_deref(), &raw.final_url, now)
                    .map_err(|_| Error::NoFeedFound(first.clone()))?;
                let known_as = vec![typed.clone(), raw.final_url];
                (feed, first, Some(page.final_url), true, known_as)
            }
        };
        let candidates: Vec<&str> = std::iter::once(xml_url.as_str())
            .chain(known_as.iter().map(String::as_str))
            .collect();
        if let Some((id, title, buried)) = self.store.find_duplicate(&candidates)? {
            return Err(Error::FeedExists { id, title, buried });
        }
        let new = NewFeed {
            html_url: feed.site_url.clone().or(site_url),
            title: feed.title.clone().unwrap_or_default(),
            xml_url,
            known_as,
        };
        let inserted = self.store.insert_added_feed(&new, theme, now)?;
        let opts = RefreshOptions {
            force: true,
            only: Some(vec![inserted.id]),
        };
        // The first fetch goes through the guarded client too: the address was typed or
        // announced by a page, so it may redirect somewhere private. Later scheduled
        // refreshes use the feed client.
        if let Err(e) = self.refresh_via(&self.articles, opts, sink, stop).await {
            tracing::warn!(feed = inserted.id, error = %e, "the first fetch of an added feed failed");
        }
        Ok(AddedFeed {
            id: inserted.id,
            title: inserted.title,
            xml_url: new.xml_url,
            page: inserted.page,
            theme: inserted.theme.map(|t| t.name),
            theme_source: inserted.theme_source,
            territory: inserted.territory,
            discovered,
        })
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
        self.refresh_via(&self.source, opts, sink, stop).await
    }

    /// `refresh`, fetching the feeds themselves through `source`.
    async fn refresh_via(
        &self,
        source: &LocalSource,
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
        let source_in_use = source;
        let source = source.clone();
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
            self.probe_https(source_in_use, &fresh, &mut pacer, sink, stop.clone())
                .await?;
            if !stop.load(Ordering::Relaxed) {
                // Triage waits for the final diagnosis below.
                self.save_fresh_diagnoses(Utc::now())?;
                self.discover_replacements(source_in_use, sink, Utc::now(), &stop)
                    .await?;
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
        source: &LocalSource,
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
        let source = source.clone();
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
        site: &LocalSource,
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
        let guarded = &self.articles;
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
            let entries = self.store.recent_entries(f.id, EVIDENCE_ENTRIES)?;
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

    /// Downloads an image for the terminal reader (spec §7, Images) through the article
    /// client: http(s) only, local and private addresses refused at every hop and after DNS
    /// resolution, at most `IMAGE_MAX_BYTES` within `IMAGE_TIMEOUT`, and an `image/*` answer.
    pub async fn fetch_image(&self, url: &str) -> Result<Vec<u8>> {
        self.fetch_image_within(url, IMAGE_TIMEOUT).await
    }

    /// `fetch_image` with its time limit as a parameter (tests use a short one).
    async fn fetch_image_within(&self, url: &str, limit: Duration) -> Result<Vec<u8>> {
        let web = url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
        if !web {
            return Err(Error::ImageRefused("not a web address".into()));
        }
        let raw = tokio::time::timeout(limit, self.articles.fetch_raw(url, None, None))
            .await
            .map_err(|_| Error::ImageRefused("timed out".into()))?;
        image_body(raw)
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

    /// The morning edition (spec §6): due check (skipped with `rebuild`), a refresh of the due
    /// feeds, then one transaction that composes and stores the edition. `now` and `tz` decide
    /// the day (04:00 local). `built_at`, which the due rule reads, is `now` plus the time the
    /// call took (the caller's clock, so a past injected day never meets today's date). The
    /// window ends at the later of that and the latest `fetched_at` this refresh wrote (the
    /// refresh stamps with the real clock), so everything the build fetched is inside it.
    pub async fn build_edition(
        &self,
        opts: EditionOptions,
        now: DateTime<Utc>,
        tz: &impl chrono::TimeZone,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<EditionOutcome> {
        let day = day_start(now.with_timezone(tz));
        let started = std::time::Instant::now();
        let run_start = Utc::now();
        if !opts.rebuild && !self.store.edition_due(day)? {
            return Ok(EditionOutcome::NotDue);
        }
        let counter = FailureCounter {
            inner: sink,
            failed: std::sync::atomic::AtomicU32::new(0),
        };
        let summary = self
            .refresh(
                // Force: a feed fetched within 30 minutes (even a failed try) would otherwise
                // be skipped, so a Retry after an offline build would see no network failure
                // and write an empty edition (spec §9).
                RefreshOptions {
                    force: true,
                    only: None,
                },
                &counter,
                stop.clone(),
            )
            .await?;
        if stop.load(Ordering::Relaxed) {
            return Ok(EditionOutcome::Interrupted);
        }
        if summary.network_unreachable {
            return Ok(EditionOutcome::Offline);
        }
        let built_at = now + chrono::Duration::from_std(started.elapsed()).unwrap_or_default();
        let end = self
            .store
            .latest_fetch_since(run_start)?
            .map_or(built_at, |fetched| fetched.max(built_at));
        let params = EditionParams {
            day_start: day,
            window_end: end,
            built_at,
            caps: opts.caps,
            unreachable: counter.failed.load(Ordering::Relaxed),
            rebuild: opts.rebuild,
        };
        let (record, composition) = match self.store.write_edition(&params) {
            Ok(written) => written,
            // Another process built it while this one was refreshing.
            Err(Error::EditionNotDue) => return Ok(EditionOutcome::NotDue),
            Err(e) => return Err(e),
        };
        let report = EditionReport::new(&record, &composition);
        tracing::info!(id = report.id, entries = report.entries, "edition built");
        sink.emit(&report.event());
        Ok(EditionOutcome::Built(report))
    }

    /// The latest edition with its entries and overflow; `None` before the first one.
    pub fn edition_today(&self) -> Result<Option<EditionView>> {
        self.store.edition_view()
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

/// The image in `raw`, or why the reader shows a placeholder instead.
fn image_body(raw: RawResponse) -> Result<Vec<u8>> {
    let refuse = |why: &str| -> Result<Vec<u8>> { Err(Error::ImageRefused(why.to_string())) };
    if raw.refused {
        return refuse("local or private address");
    }
    match &raw.error {
        Some(ErrorKind::TooLarge) => return refuse("larger than 10 MB"),
        Some(ErrorKind::Timeout) => return refuse("timed out"),
        Some(kind) => return refuse(&kind.label()),
        None => {}
    }
    let is_image = raw
        .content_type
        .as_deref()
        .is_some_and(|ct| ct.trim_start().to_ascii_lowercase().starts_with("image/"));
    if !is_image {
        return refuse("not an image");
    }
    // The source raises its cap for bodies that look like podcast feeds: check again here.
    if raw.body.len() > IMAGE_MAX_BYTES {
        return refuse("larger than 10 MB");
    }
    Ok(raw.body)
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

/// A typed address as an http(s) URL: trimmed, `https://` when it names no scheme.
/// Anything else (`ftp://`, `mailto:`, an empty string) is `Error::InvalidAddress`.
fn normalise_address(input: &str) -> Result<String> {
    let input = input.trim();
    let has_scheme = input.contains("://")
        || input.split_once(':').is_some_and(|(scheme, rest)| {
            // `host:8080/...` is a port, not a scheme.
            !rest.starts_with(|c: char| c.is_ascii_digit())
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        });
    let full = if has_scheme {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    match url::Url::parse(&full) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            Ok(u.to_string())
        }
        _ => Err(Error::InvalidAddress),
    }
}

/// Fetches `url` through the guarded client for `add_feed`: a private address or a failure
/// is an error worded for a person.
async fn fetch_guarded(source: &LocalSource, url: &str) -> Result<RawResponse> {
    let raw = source.fetch_raw(url, None, None).await;
    if raw.refused {
        return Err(Error::Invalid(
            "Ressac only follows public web addresses.".into(),
        ));
    }
    match &raw.error {
        Some(kind) => Err(Error::Unreachable {
            url: url.to_owned(),
            reason: kind.label(),
        }),
        None => Ok(raw),
    }
}

/// `Some(found)` once the site was checked (`found` is `None` when it announces nothing new);
/// `None` when it could not be checked (transient page-fetch failure, a site page the
/// client refuses, no site address).
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
    // A site page the client refuses (a LAN address) was never looked at.
    if page.refused {
        return None;
    }
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

    /// A site page the guarded client refuses (a LAN address) was never looked at: it is
    /// not checked, so nothing is cleared or stamped.
    #[tokio::test]
    async fn a_refused_site_page_is_not_checked() {
        let guarded = LocalSource::for_articles(FetchLimits::default(), false).unwrap();
        for site in [
            "http://nas.local/",
            "http://192.168.1.10/",
            "http://localhost/",
        ] {
            let found = discover_one(&guarded, &guarded, &row(site.to_owned()), Utc::now()).await;
            assert_eq!(found, None, "{site}");
        }
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
    fn edition_events_have_stable_json() {
        let event = Event::Edition {
            id: 12,
            entries: 43,
            front: 2,
            week: 7,
            wire: 34,
            overflow: 61,
            unreachable: 3,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"event":"edition","id":12,"entries":43,"front":2,"week":7,"wire":34,"overflow":61,"unreachable":3}"#
        );
    }

    #[test]
    fn an_edition_report_counts_sections_and_hidden_entries() {
        use crate::edition::{Composition, Overflow, Placed, Section};
        let record = crate::store::EditionRecord {
            id: 4,
            built_at: Utc::now(),
            window_start: Utc::now(),
            window_end: Utc::now(),
            unreachable: 1,
        };
        let comp = Composition {
            entries: vec![
                Placed {
                    entry_id: 1,
                    section: Section::Front,
                    rank: 1,
                },
                Placed {
                    entry_id: 2,
                    section: Section::Wire,
                    rank: 2,
                },
                Placed {
                    entry_id: 3,
                    section: Section::Wire,
                    rank: 3,
                },
            ],
            overflow: vec![
                Overflow {
                    feed_id: 1,
                    section: Section::Wire,
                    hidden_count: 4,
                },
                Overflow {
                    feed_id: 2,
                    section: Section::Week,
                    hidden_count: 2,
                },
            ],
        };
        let report = EditionReport::new(&record, &comp);
        assert_eq!(
            report,
            EditionReport {
                id: 4,
                entries: 3,
                front: 1,
                week: 0,
                wire: 2,
                overflow: 6,
                unreachable: 1
            }
        );
        assert_eq!(
            report.event(),
            Event::Edition {
                id: 4,
                entries: 3,
                front: 1,
                week: 0,
                wire: 2,
                overflow: 6,
                unreachable: 1
            }
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

    // ---- add_feed ----

    fn add_app() -> Ressac {
        let limits = FetchLimits {
            per_host_gap: Duration::ZERO,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            limits,
            "http://127.0.0.1:9",
        )
        .unwrap()
        .allow_private_articles()
        .unwrap()
    }

    async fn add(app: &Ressac, input: &str) -> Result<AddedFeed> {
        app.add_feed(input, None, &NullSink, Arc::new(AtomicBool::new(false)))
            .await
    }

    fn page_announcing(links: &[&str]) -> String {
        let links: String = links
            .iter()
            .map(|l| format!(r#"<link rel="alternate" type="application/rss+xml" href="{l}">"#))
            .collect();
        format!("<html><head>{links}</head><body>hello</body></html>")
    }

    fn count(app: &Ressac, table: &str) -> i64 {
        app.store()
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn normalises_scheme_less_input() {
        assert_eq!(
            normalise_address("  example.org/feed ").unwrap(),
            "https://example.org/feed"
        );
        assert_eq!(
            normalise_address("http://example.org").unwrap(),
            "http://example.org/"
        );
        assert_eq!(
            normalise_address("localhost:8080/x").unwrap(),
            "https://localhost:8080/x"
        );
    }

    #[test]
    fn refuses_non_http() {
        for bad in [
            "",
            "   ",
            "ftp://example.org/feed",
            "file:///etc/passwd",
            "mailto:a@example.org",
            "javascript:alert(1)",
            "https://",
            "not a url",
        ] {
            assert!(
                matches!(normalise_address(bad), Err(Error::InvalidAddress)),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn adds_a_feed_address_directly() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        assert_eq!(got.title, "T");
        assert_eq!(got.xml_url, format!("{}/feed.xml", server.uri()));
        assert!(!got.discovered);
        assert_eq!(got.page, Some(110));
        assert_eq!(got.theme, None);
        assert_eq!(got.theme_source, ThemeSource::None);
        let row = app.store().feed(got.id).unwrap();
        assert_eq!(row.folder_id, None);
        assert_eq!(row.xml_url_original, row.xml_url);
        assert_eq!(row.html_url.as_deref(), Some("http://example.org/"));
    }

    #[tokio::test]
    async fn discovers_the_first_announced_feed() {
        let server = MockServer::start().await;
        let html = page_announcing(&["/first.xml", "/second.xml"]);
        serve(&server, "/", 200, &html, "text/html").await;
        serve(&server, "/first.xml", 200, RSS, "application/rss+xml").await;
        serve(&server, "/second.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &server.uri()).await.unwrap();
        assert!(got.discovered);
        assert_eq!(got.xml_url, format!("{}/first.xml", server.uri()));
        let seen: Vec<String> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect();
        assert!(!seen.contains(&"/second.xml".to_owned()), "{seen:?}");
    }

    #[tokio::test]
    async fn relative_feed_link_resolves_against_the_final_url() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("GET"))
            .and(path("/old"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", "/blog/index.html"))
            .mount(&server)
            .await;
        let html = page_announcing(&["feed.xml"]);
        serve(&server, "/blog/index.html", 200, &html, "text/html").await;
        serve(&server, "/blog/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &format!("{}/old", server.uri())).await.unwrap();
        assert_eq!(got.xml_url, format!("{}/blog/feed.xml", server.uri()));
        let row = app.store().feed(got.id).unwrap();
        assert_eq!(row.xml_url, got.xml_url);
        assert_eq!(row.xml_url_original, got.xml_url);
    }

    async fn redirect(server: &MockServer, from: &str, to: &str) {
        Mock::given(wiremock::matchers::method("GET"))
            .and(path(from))
            .respond_with(ResponseTemplate::new(301).insert_header("location", to))
            .mount(server)
            .await;
    }

    /// A loopback listener that counts the connections it is offered.
    struct Tap(std::net::TcpListener);

    impl Tap {
        fn new() -> Self {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.set_nonblocking(true).unwrap();
            Self(l)
        }

        fn port(&self) -> u16 {
            self.0.local_addr().unwrap().port()
        }

        fn connections(&self) -> usize {
            std::iter::from_fn(|| self.0.accept().ok()).count()
        }
    }

    #[tokio::test]
    async fn the_https_probe_uses_the_source_it_is_given() {
        let tap = Tap::new();
        let app = add_app();
        let xml = format!(
            r#"<opml><body><outline text="T" xmlUrl="http://127.0.0.1:{}/feed.xml"/></body></opml>"#,
            tap.port()
        );
        app.import_opml(&xml, false).unwrap();
        let fresh = HashMap::from([(1, feed(&["a"]))]);
        let guarded = LocalSource::for_articles(app.limits.clone(), false).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut pacer = HostPacer::new(Duration::ZERO);
        app.probe_https(&guarded, &fresh, &mut pacer, &NullSink, stop.clone())
            .await
            .unwrap();
        assert_eq!(tap.connections(), 0, "the guarded source never connects");
        // Control: the feed client does connect to the https variant.
        app.store()
            .conn()
            .execute("UPDATE feed SET https_available = NULL", [])
            .unwrap();
        app.probe_https(&app.source, &fresh, &mut pacer, &NullSink, stop)
            .await
            .unwrap();
        assert_eq!(tap.connections(), 1);
    }

    #[tokio::test]
    async fn discovery_fetches_the_site_through_the_source_it_is_given() {
        let tap = Tap::new();
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 410, "", "text/plain").await;
        let app = add_app();
        let xml = format!(
            r#"<opml><body><outline text="T" xmlUrl="{}/feed.xml"/></body></opml>"#,
            server.uri()
        );
        app.import_opml(&xml, false).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        app.refresh(RefreshOptions::default(), &NullSink, stop.clone())
            .await
            .unwrap();
        assert_eq!(
            app.store().diagnosis(1).unwrap().map(|d| d.status),
            Some(Status::Dead)
        );
        app.store()
            .conn()
            .execute(
                "UPDATE feed SET html_url = ?1, discovery_checked_at = NULL",
                [format!("http://127.0.0.1:{}/", tap.port())],
            )
            .unwrap();
        let guarded = LocalSource::for_articles(app.limits.clone(), false).unwrap();
        app.discover_replacements(&guarded, &NullSink, Utc::now(), &stop)
            .await
            .unwrap();
        assert_eq!(tap.connections(), 0, "the guarded source never connects");
        app.store()
            .conn()
            .execute("UPDATE feed SET discovery_checked_at = NULL", [])
            .unwrap();
        app.discover_replacements(&app.source, &NullSink, Utc::now(), &stop)
            .await
            .unwrap();
        assert_eq!(tap.connections(), 1, "control: the feed client connects");
    }

    #[tokio::test]
    async fn a_direct_address_is_stored_as_typed_not_as_redirected() {
        let server = MockServer::start().await;
        redirect(&server, "/old.xml", "/new.xml").await;
        serve(&server, "/new.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let typed = format!("{}/old.xml", server.uri());
        let got = add(&app, &typed).await.unwrap();
        assert_eq!(got.xml_url, typed);
        let row = app.store().feed(got.id).unwrap();
        assert_eq!(row.xml_url, typed);
        assert_eq!(row.xml_url_original, typed);
    }

    #[tokio::test]
    async fn an_announced_link_that_redirects_to_a_followed_feed_is_a_duplicate() {
        let server = MockServer::start().await;
        let base = server.uri();
        redirect(&server, "/old.xml", "/new.xml").await;
        serve(&server, "/new.xml", 200, RSS, "application/rss+xml").await;
        serve(
            &server,
            "/",
            200,
            &page_announcing(&["/old.xml"]),
            "text/html",
        )
        .await;
        let app = add_app();
        app.import_opml(
            &format!(r#"<opml><body><outline text="Old" xmlUrl="{base}/new.xml"/></body></opml>"#),
            false,
        )
        .unwrap();
        let err = add(&app, &base).await.unwrap_err();
        assert!(matches!(err, Error::FeedExists { id: 1, .. }), "{err:?}");
        assert_eq!(count(&app, "feed"), 1);
    }

    #[tokio::test]
    async fn refresh_via_fetches_the_feed_through_the_source_it_is_given() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        // Same feed, now fetched the way `add_feed` does it, in production configuration.
        let guarded = LocalSource::for_articles(app.limits.clone(), false).unwrap();
        server.reset().await;
        app.store().conn().execute("DELETE FROM entry", []).unwrap();
        let opts = RefreshOptions {
            force: true,
            only: Some(vec![got.id]),
        };
        app.refresh_via(&guarded, opts, &NullSink, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
        assert_eq!(count(&app, "feed"), 1, "the feed stays");
        assert_eq!(count(&app, "entry"), 0);
        assert_eq!(app.store().attempts(got.id, 1).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn page_without_feed_is_refused() {
        let server = MockServer::start().await;
        serve(&server, "/", 200, &page_announcing(&[]), "text/html").await;
        let app = add_app();
        let err = add(&app, &server.uri()).await.unwrap_err();
        assert!(matches!(err, Error::NoFeedFound(_)), "{err:?}");
        assert_eq!(err.to_string(), "This page announces no feed.");
        assert_eq!(count(&app, "feed"), 0);
    }

    #[tokio::test]
    async fn first_link_not_a_feed_is_refused() {
        let server = MockServer::start().await;
        let html = page_announcing(&["/broken.xml", "/good.xml"]);
        serve(&server, "/", 200, &html, "text/html").await;
        serve(
            &server,
            "/broken.xml",
            200,
            "<html>nope</html>",
            "text/html",
        )
        .await;
        serve(&server, "/good.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let err = add(&app, &server.uri()).await.unwrap_err();
        match &err {
            Error::NoFeedFound(tried) => assert!(tried.ends_with("/broken.xml"), "{tried}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(count(&app, "feed"), 0);
    }

    #[tokio::test]
    async fn typed_address_already_followed_is_refused() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let url = format!("{}/feed.xml", server.uri());
        let first = add(&app, &url).await.unwrap();
        let before = server.received_requests().await.unwrap().len();
        let err = add(&app, &url).await.unwrap_err();
        assert!(
            matches!(&err, Error::FeedExists { id, buried: false, .. } if *id == first.id),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "Already followed: T.");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            before,
            "the typed address is checked before any network work"
        );
        garden::decide(
            app.store(),
            first.id,
            crate::model::Decision::Bury,
            Utc::now(),
        )
        .unwrap();
        let err = add(&app, &url).await.unwrap_err();
        assert_eq!(err.to_string(), "Already in the Cemetery: T.");
        assert_eq!(count(&app, "feed"), 1);
    }

    #[tokio::test]
    async fn a_resolved_feed_already_followed_is_refused_after_discovery() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/",
            200,
            &page_announcing(&["/feed.xml"]),
            "text/html",
        )
        .await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        let err = add(&app, &server.uri()).await.unwrap_err();
        assert!(matches!(err, Error::FeedExists { .. }), "{err:?}");
        assert_eq!(count(&app, "feed"), 1);
    }

    #[tokio::test]
    async fn private_address_refused() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let limits = FetchLimits {
            per_host_gap: Duration::ZERO,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        // Production configuration: no `allow_private_articles`.
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            limits,
            "http://127.0.0.1:9",
        )
        .unwrap();
        let err = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
        assert_eq!(count(&app, "feed"), 0);
    }

    #[tokio::test]
    async fn a_theme_is_chosen_by_key_or_by_name_ignoring_case() {
        use crate::store::{ChartImport, ThemeDef};
        for given in ["edu", "edu", "  education AND edtech ", "ÉDUCATION"] {
            let server = MockServer::start().await;
            serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
            let app = add_app();
            let theme = |key: &str, name: &str| ThemeDef {
                key: key.into(),
                name: name.into(),
                sea: name.to_uppercase(),
            };
            app.store()
                .import_chart(
                    &ChartImport {
                        themes: vec![
                            theme("edu", "Education and EdTech"),
                            theme("accent", "Éducation"),
                        ],
                        feeds: Default::default(),
                        territories: vec![],
                    },
                    false,
                )
                .unwrap();
            let got = app
                .add_feed(
                    &format!("{}/feed.xml", server.uri()),
                    Some(given),
                    &NullSink,
                    Arc::new(AtomicBool::new(false)),
                )
                .await
                .unwrap();
            let want = if given == "ÉDUCATION" {
                "Éducation"
            } else {
                "Education and EdTech"
            };
            assert_eq!(got.theme.as_deref(), Some(want), "{given:?}");
            assert_eq!(got.theme_source, ThemeSource::Chosen);
        }
    }

    #[tokio::test]
    async fn unknown_theme_is_refused_before_anything_is_written() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let err = app
            .add_feed(
                &format!("{}/feed.xml", server.uri()),
                Some("nope"),
                &NullSink,
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::UnknownTheme { .. }), "{err:?}");
        assert_eq!(count(&app, "feed"), 0);
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an unknown theme is refused before any network work"
        );
    }

    #[tokio::test]
    async fn an_add_leaves_other_feeds_discovery_alone() {
        let server = MockServer::start().await;
        serve(&server, "/dead.xml", 410, "", "text/plain").await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let xml = format!(
            r#"<opml><body><outline text="L" xmlUrl="{}/dead.xml"/></body></opml>"#,
            server.uri()
        );
        app.import_opml(&xml, false).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        app.refresh(RefreshOptions::default(), &NullSink, stop)
            .await
            .unwrap();
        assert_eq!(
            app.store().diagnosis(1).unwrap().map(|d| d.status),
            Some(Status::Dead)
        );
        app.store()
            .conn()
            .execute(
                "UPDATE feed SET html_url = 'http://nas.local/', \
                 discovered_url = 'https://nas.example/new.xml', discovery_checked_at = NULL",
                [],
            )
            .unwrap();
        // The production client refuses LAN addresses: the add path's guarded source must
        // not mistake that for "checked, nothing found".
        let state = |app: &Ressac| -> (Option<String>, Option<String>) {
            app.store()
                .conn()
                .query_row(
                    "SELECT discovered_url, discovery_checked_at FROM feed WHERE id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        let before = state(&app);
        let added = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        assert_eq!(state(&app), before, "the add itself");
        // The add's first refresh runs its discovery pass on the guarded client, which
        // refuses a LAN site page: do the same with the production guard on.
        let guarded = LocalSource::for_articles(app.limits.clone(), false).unwrap();
        assert!(added.id > 1);
        // A pending discovery makes the feed Moved; put it back to Dead, as when the
        // diagnosis ran before the discovery landed.
        app.store()
            .conn()
            .execute(
                "UPDATE feed_health SET status = 'Dead' WHERE feed_id = 1",
                [],
            )
            .unwrap();
        app.discover_replacements(&guarded, &NullSink, Utc::now(), &AtomicBool::new(false))
            .await
            .unwrap();
        assert_eq!(state(&app), before, "a guarded discovery pass");
    }

    #[tokio::test]
    async fn the_announced_link_is_not_requested_when_already_followed() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/",
            200,
            &page_announcing(&["/feed.xml"]),
            "text/html",
        )
        .await;
        let app = add_app();
        let announced = format!("{}/feed.xml", server.uri());
        app.import_opml(
            &format!(r#"<opml><body><outline text="T" xmlUrl="{announced}"/></body></opml>"#),
            false,
        )
        .unwrap();
        let err = add(&app, &server.uri()).await.unwrap_err();
        assert!(matches!(err, Error::FeedExists { .. }), "{err:?}");
        let paths: Vec<String> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect();
        assert_eq!(paths, ["/"], "the announced link is never requested");
    }

    fn html_url_of(app: &Ressac, id: FeedId) -> Option<String> {
        app.store()
            .conn()
            .query_row("SELECT html_url FROM feed WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    #[tokio::test]
    async fn a_protocol_relative_announced_link_resolves_against_the_page() {
        let server = MockServer::start().await;
        let authority = server.uri().trim_start_matches("http://").to_owned();
        let link = format!("//{authority}/rss.xml");
        serve(&server, "/", 200, &page_announcing(&[&link]), "text/html").await;
        serve(&server, "/rss.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &server.uri()).await.unwrap();
        assert_eq!(got.xml_url, format!("{}/rss.xml", server.uri()));
        assert!(got.discovered);
    }

    #[tokio::test]
    async fn html_url_falls_back_to_the_page_the_redirect_ended_at() {
        let server = MockServer::start().await;
        let bare = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><description>d</description><item><guid>1</guid><title>One</title></item></channel></rss>"#;
        Mock::given(wiremock::matchers::method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", "/home/"))
            .mount(&server)
            .await;
        serve(
            &server,
            "/home/",
            200,
            &page_announcing(&["/rss.xml"]),
            "text/html",
        )
        .await;
        serve(&server, "/rss.xml", 200, bare, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &format!("{}/start", server.uri())).await.unwrap();
        assert_eq!(
            html_url_of(&app, got.id),
            Some(format!("{}/home/", server.uri()))
        );
    }

    #[tokio::test]
    async fn fetches_right_after_adding() {
        let server = MockServer::start().await;
        serve(&server, "/feed.xml", 200, RSS, "application/rss+xml").await;
        let app = add_app();
        let got = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        let entries: i64 = app
            .store()
            .conn()
            .query_row(
                "SELECT count(*) FROM entry WHERE feed_id = ?1",
                [got.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(entries, 1);
    }

    #[tokio::test]
    async fn failed_fetch_keeps_the_feed() {
        let server = MockServer::start().await;
        // The first request (the add's own look at the address) succeeds, the next fail.
        Mock::given(wiremock::matchers::method("GET"))
            .and(path("/feed.xml"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(RSS, "application/rss+xml"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        serve(&server, "/feed.xml", 500, "", "text/plain").await;
        let app = add_app();
        let got = add(&app, &format!("{}/feed.xml", server.uri()))
            .await
            .unwrap();
        assert_eq!(count(&app, "feed"), 1);
        assert_eq!(count(&app, "entry"), 0);
        let attempts = app.store().attempts(got.id, 1).unwrap();
        assert_eq!(attempts.len(), 1, "the failed fetch is recorded");
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn a_slow_image_times_out_without_hanging() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(vec![0u8; 8], "image/png")
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap()
        .allow_private_articles()
        .unwrap();
        let started = Instant::now();
        let err = app
            .fetch_image_within(
                &format!("{}/slow.png", server.uri()),
                Duration::from_millis(300),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::ImageRefused(why) if why == "timed out"),
            "{err:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the limit is the caller's, not the source's 30 s"
        );
    }

    #[test]
    fn the_limits_are_the_spec_s() {
        assert_eq!(IMAGE_MAX_BYTES, 10 * 1024 * 1024);
        assert_eq!(IMAGE_TIMEOUT, Duration::from_secs(15));
    }
}
