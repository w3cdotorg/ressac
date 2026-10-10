# Phase 7: Health persistence, `Ressac` facade, network guard, end-to-end test

Read the plan index first. Global Constraints apply. Spec §6 (fetch, autodiscovery, Wayback) and §8 (network guard).

**Files (5):**
- Create: `crates/ressac-core/src/store/health.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (add `mod health;`)
- Create: `crates/ressac-core/src/service.rs`
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod service;`)
- Create: `crates/ressac-core/tests/end_to_end.rs`

### Task 7.1: Persist diagnoses and probe results

**Interfaces:**
- Consumes: `model::{Diagnosis, Evidence, Remark, Action, Status, Cadence, ts, parse_ts}`.
- Produces (methods on `Store`):
  - `save_diagnoses(&self, diags: &HashMap<FeedId, Diagnosis>, now: DateTime<Utc>) -> Result<()>`, in one transaction, upserting
  - `diagnosis(&self, feed_id: FeedId) -> Result<Option<Diagnosis>>`
  - `diagnoses(&self) -> Result<HashMap<FeedId, Diagnosis>>`
  - `set_https_available(&self, feed_id: FeedId, available: bool) -> Result<()>`
  - `set_discovery(&self, feed_id: FeedId, url: Option<&str>, now: DateTime<Utc>) -> Result<()>`
  - `set_wayback(&self, feed_id: FeedId, url: &str) -> Result<()>`

- [ ] **Step 1: Write the failing tests** in `store/health.rs`

```rust
//! Diagnoses (`feed_health`) and probe caches on `feed`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Cadence, Evidence, Remark, Status};
    use crate::opml;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body><outline text="A" xmlUrl="http://a.example.org/feed"/><outline text="B" xmlUrl="https://b.example.org/feed"/></body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now()).unwrap();
        s
    }

    fn sample() -> Diagnosis {
        Diagnosis {
            status: Status::Dead,
            reason: "The server says the feed is gone (HTTP 410).".into(),
            evidence: Evidence { bullets: vec!["b1".into()], clue: "410".into(), language: None },
            remarks: vec![Remark::HttpOnly, Remark::SimilarTitle { feed_id: 2 }],
            action: Some(Action::Bury),
            hijack_score: 0.3,
            cadence: Cadence { last_entry_at: Some(now()), median_interval_days: Some(7.0), entries_per_month: 1.5 },
        }
    }

    #[test]
    fn diagnosis_round_trips_and_upserts() {
        let s = store();
        s.save_diagnoses(&HashMap::from([(1, sample())]), now()).unwrap();
        assert_eq!(s.diagnosis(1).unwrap(), Some(sample()));
        let mut changed = sample();
        changed.status = Status::Alive;
        changed.action = None;
        s.save_diagnoses(&HashMap::from([(1, changed.clone())]), now()).unwrap();
        assert_eq!(s.diagnosis(1).unwrap(), Some(changed));
        assert_eq!(s.diagnosis(2).unwrap(), None);
        assert_eq!(s.diagnoses().unwrap().len(), 1);
    }

    #[test]
    fn probe_caches_are_stored_on_the_feed() {
        let s = store();
        s.set_https_available(1, true).unwrap();
        s.set_discovery(2, Some("https://b.example.org/new.xml"), now()).unwrap();
        s.set_wayback(2, "http://web.archive.org/web/2019/x").unwrap();
        assert_eq!(s.feed(1).unwrap().https_available, Some(true));
        let b = s.feed(2).unwrap();
        assert_eq!(b.discovered_url.as_deref(), Some("https://b.example.org/new.xml"));
        assert_eq!(b.discovery_checked_at.as_deref(), Some("2026-10-10T08:00:00.000Z"));
        assert_eq!(b.wayback_url.as_deref(), Some("http://web.archive.org/web/2019/x"));
    }
}
```

- [ ] **Step 2:** Add `mod health;` in `store/mod.rs`; run `cargo test -p ressac-core store::health`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Row, params};

use super::Store;
use crate::error::Result;
use crate::model::{Cadence, Diagnosis, FeedId, Status, parse_ts, ts};

const HEALTH_COLUMNS: &str = "feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
     last_entry_at, median_interval_days, entries_per_month";

fn json_error(col: usize, e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
}

fn health_row(r: &Row<'_>) -> rusqlite::Result<(FeedId, Diagnosis)> {
    let status: String = r.get(1)?;
    let evidence: String = r.get(3)?;
    let remarks: String = r.get(4)?;
    let action: Option<String> = r.get(5)?;
    let last_entry: Option<String> = r.get(7)?;
    Ok((
        r.get(0)?,
        Diagnosis {
            status: Status::parse(&status).unwrap_or(Status::Unchecked),
            reason: r.get(2)?,
            evidence: serde_json::from_str(&evidence).map_err(|e| json_error(3, e))?,
            remarks: serde_json::from_str(&remarks).map_err(|e| json_error(4, e))?,
            action: action.map(|a| serde_json::from_str(&a)).transpose().map_err(|e| json_error(5, e))?,
            hijack_score: r.get::<_, f64>(6)? as f32,
            cadence: Cadence {
                last_entry_at: last_entry.as_deref().and_then(parse_ts),
                median_interval_days: r.get(8)?,
                entries_per_month: r.get(9)?,
            },
        },
    ))
}

impl Store {
    pub fn save_diagnoses(&self, diags: &HashMap<FeedId, Diagnosis>, now: DateTime<Utc>) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
                 last_entry_at, median_interval_days, entries_per_month, computed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT(feed_id) DO UPDATE SET status = excluded.status, reason = excluded.reason, \
                 evidence = excluded.evidence, remarks = excluded.remarks, proposed_action = excluded.proposed_action, \
                 hijack_score = excluded.hijack_score, last_entry_at = excluded.last_entry_at, \
                 median_interval_days = excluded.median_interval_days, entries_per_month = excluded.entries_per_month, \
                 computed_at = excluded.computed_at",
            )?;
            for (feed_id, d) in diags {
                stmt.execute(params![
                    feed_id,
                    d.status.as_str(),
                    d.reason,
                    serde_json::to_string(&d.evidence)?,
                    serde_json::to_string(&d.remarks)?,
                    d.action.as_ref().map(serde_json::to_string).transpose()?,
                    f64::from(d.hijack_score),
                    d.cadence.last_entry_at.map(ts),
                    d.cadence.median_interval_days,
                    d.cadence.entries_per_month,
                    ts(now)
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn diagnosis(&self, feed_id: FeedId) -> Result<Option<Diagnosis>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {HEALTH_COLUMNS} FROM feed_health WHERE feed_id = ?1"))?;
        let mut rows = stmt.query_map([feed_id], health_row)?;
        Ok(rows.next().transpose()?.map(|(_, d)| d))
    }

    pub fn diagnoses(&self) -> Result<HashMap<FeedId, Diagnosis>> {
        let mut stmt = self.conn().prepare(&format!("SELECT {HEALTH_COLUMNS} FROM feed_health"))?;
        let rows = stmt.query_map([], health_row)?.collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }

    pub fn set_https_available(&self, feed_id: FeedId, available: bool) -> Result<()> {
        self.conn()
            .execute("UPDATE feed SET https_available = ?2 WHERE id = ?1", params![feed_id, available])?;
        Ok(())
    }

    pub fn set_discovery(&self, feed_id: FeedId, url: Option<&str>, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET discovered_url = ?2, discovery_checked_at = ?3 WHERE id = ?1",
            params![feed_id, url, ts(now)],
        )?;
        Ok(())
    }

    pub fn set_wayback(&self, feed_id: FeedId, url: &str) -> Result<()> {
        self.conn().execute("UPDATE feed SET wayback_url = ?2 WHERE id = ?1", params![feed_id, url])?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core store::health` → `2 passed`.

- [ ] **Step 5: Commit** ("Persist diagnoses and probe caches", both attribution lines).

### Task 7.2: The `Ressac` facade and the end-to-end test

**Interfaces:**
- Consumes: everything above. Exact names: `Store::{open, feeds, feeds_to_fetch, record_attempt, mark_network_suspect, attempts, entries, save_diagnoses, diagnoses, set_https_available, set_discovery, set_wayback, import_outlines}`; `health::{diagnose, FeedEvidence}`; `health::links::{LinkInput, find_duplicates, apply_similar_titles}`; `discover::{alternate_feeds, same_feed}`; `parse::{ParsedFeed, parse_feed}`; `source::{FeedRef, FetchLimits, LocalSource, Source}`; `wayback::Wayback`; `opml::{parse, clean_description}`; `url_key::{url_key, https_variant}`.
- Produces (`ressac_core::service`):
  - `RefreshOptions { force: bool }`
  - `enum Event`, serialized with tag `event` in snake_case: `Started { total }`, `FeedDone { feed_id, title, error: Option<String> }`, `NetworkUnreachable { failed, attempted }`, `Probing { what, total }`, `WaybackDone { found }`, `Finished { summary }`
  - `trait EventSink { fn emit(&self, event: &Event); }`, `NullSink`
  - `Summary { total, counts: BTreeMap<String, usize>, https_repairs, redirect_repairs, network_unreachable }`, with `count(Status)` and `safe_repairs()`
  - `Ressac` with:
    - `open(&Path)`, `with_parts(Store, FetchLimits, wayback_base: &str)`, `store(&self) -> &Store`
    - `import_opml(&self, xml, dry_run) -> Result<ImportReport>`
    - `async refresh(&self, RefreshOptions, &dyn EventSink, Arc<AtomicBool>) -> Result<Summary>`
    - `diagnose_all(&self, now) -> Result<Summary>`
    - `async lookup_wayback(&self, &dyn EventSink, Arc<AtomicBool>) -> Result<usize>`
    - `summary(&self) -> Result<Summary>`

- [ ] **Step 1: Write the failing end-to-end test** `crates/ressac-core/tests/end_to_end.rs`

```rust
//! Import → refresh → diagnose → Wayback against a local mock web.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use ressac_core::model::{Action, Status};
use ressac_core::service::{Event, EventSink, RefreshOptions, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl EventSink for Recorder {
    fn emit(&self, event: &Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn rss(title: &str, items: &[(&str, DateTime<Utc>)]) -> String {
    let items: String = items
        .iter()
        .map(|(id, d)| {
            format!(
                "<item><guid>{id}</guid><title>Entry {id}</title><link>http://example.org/{id}</link><pubDate>{}</pubDate></item>",
                d.to_rfc2822()
            )
        })
        .collect();
    format!(r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://example.org/</link><description>A feed</description>{items}</channel></rss>"#)
}

async fn serve(server: &MockServer, at: &str, template: ResponseTemplate) {
    Mock::given(method("GET")).and(path(at)).respond_with(template).mount(server).await;
}

fn ok_rss(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml")
}

fn limits() -> FetchLimits {
    FetchLimits { per_host_gap: Duration::ZERO, timeout: Duration::from_secs(5), ..FetchLimits::default() }
}

const SPAM: &str = "Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày. Tham gia ngay để thử thách bản thân.";

#[tokio::test]
async fn full_pipeline_classifies_every_kind_of_feed() {
    let server = MockServer::start().await;
    let base = server.uri();
    let recent = Utc::now() - chrono::Duration::days(5);
    let old: DateTime<Utc> = "2015-03-01T10:00:00Z".parse().unwrap();

    serve(&server, "/alive.xml", ok_rss(rss("Alive", &[("a1", recent), ("a2", recent - chrono::Duration::days(7)), ("a3", recent - chrono::Duration::days(14))]))).await;
    serve(&server, "/gone.xml", ResponseTemplate::new(410)).await;
    serve(&server, "/old.xml", ok_rss(rss("Old", &[("o1", old)]))).await;
    serve(&server, "/moved.xml", ResponseTemplate::new(301).insert_header("Location", "/new.xml")).await;
    serve(&server, "/new.xml", ok_rss(rss("New home", &[("n1", recent)]))).await;
    serve(&server, "/lost.xml", ResponseTemplate::new(404)).await;
    serve(&server, "/lost-site", ResponseTemplate::new(200).set_body_raw(r#"<html><head><link rel="alternate" type="application/rss+xml" href="/found.xml"></head></html>"#, "text/html")).await;
    serve(&server, "/found.xml", ok_rss(rss("Found", &[("f1", recent)]))).await;
    serve(&server, "/spam.xml", ResponseTemplate::new(200).set_body_raw("<!DOCTYPE html><html><body>casino</body></html>", "text/html")).await;
    serve(&server, "/mirror.xml", ResponseTemplate::new(301).insert_header("Location", "/alive.xml")).await;
    serve(&server, "/wayback/available", ResponseTemplate::new(200).set_body_raw(r#"{"archived_snapshots":{"closest":{"available":true,"url":"http://web.archive.org/web/2019/x","timestamp":"20190101000000","status":"200"}}}"#, "application/json")).await;

    let opml = format!(
        r#"<opml version="1.0"><body><outline text="Test">
        <outline text="Alive" xmlUrl="{base}/alive.xml" htmlUrl="{base}/"/>
        <outline text="Gone" xmlUrl="{base}/gone.xml" htmlUrl="{base}/gone-site"/>
        <outline text="Old" xmlUrl="{base}/old.xml" htmlUrl="{base}/"/>
        <outline text="Moved" xmlUrl="{base}/moved.xml" htmlUrl="{base}/"/>
        <outline text="Lost" xmlUrl="{base}/lost.xml" htmlUrl="{base}/lost-site"/>
        <outline text="Spam" xmlUrl="{base}/spam.xml" htmlUrl="{base}/spam-site" description="{SPAM}"/>
        <outline text="Mirror" xmlUrl="{base}/mirror.xml" htmlUrl="{base}/"/>
        </outline></body></opml>"#
    );

    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
    assert_eq!(app.import_opml(&opml, false).unwrap().feeds_added, 7);
    let recorder = Recorder::default();
    let summary = app
        .refresh(RefreshOptions::default(), &recorder, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    assert!(!summary.network_unreachable);
    assert_eq!(summary.total, 7);

    let feeds = app.store().feeds().unwrap();
    let diags = app.store().diagnoses().unwrap();
    let by_title = |t: &str| {
        let f = feeds.iter().find(|f| f.title == t).unwrap();
        (f.id, diags[&f.id].clone())
    };
    let (alive_id, alive) = by_title("Alive");
    assert_eq!(alive.status, Status::Alive);
    assert_eq!(by_title("Gone").1.status, Status::Dead);
    assert_eq!(by_title("Old").1.status, Status::Fossil);
    let moved = by_title("Moved").1;
    assert_eq!(moved.status, Status::Moved);
    assert_eq!(moved.action, Some(Action::FollowRedirect { url: format!("{base}/new.xml") }));
    let lost = by_title("Lost").1;
    assert_eq!(lost.status, Status::Moved);
    assert_eq!(lost.action, Some(Action::AdoptDiscovered { url: format!("{base}/found.xml") }));
    assert_eq!(by_title("Spam").1.status, Status::Hijacked);
    let mirror = by_title("Mirror").1;
    assert_eq!(mirror.status, Status::Duplicate);
    assert_eq!(mirror.action, Some(Action::MergeInto { feed_id: alive_id }));

    let events = recorder.0.lock().unwrap().clone();
    assert!(matches!(events.first(), Some(Event::Started { total: 7 })));
    assert!(matches!(events.last(), Some(Event::Finished { .. })));
    assert_eq!(events.iter().filter(|e| matches!(e, Event::FeedDone { .. })).count(), 7);

    let found = app.lookup_wayback(&recorder, Arc::new(AtomicBool::new(false))).await.unwrap();
    assert_eq!(found, 2, "Dead + Hijacked");
    let gone = app.store().feeds().unwrap().into_iter().find(|f| f.title == "Gone").unwrap();
    assert_eq!(gone.wayback_url.as_deref(), Some("http://web.archive.org/web/2019/x"));

    // A second run within 30 minutes fetches nothing and keeps the diagnoses.
    let again = app.refresh(RefreshOptions::default(), &recorder, Arc::new(AtomicBool::new(false))).await.unwrap();
    assert_eq!(again.count(Status::Alive), 1);
}

#[tokio::test]
async fn unreachable_network_does_not_kill_feeds() {
    let opml = r#"<opml><body>
        <outline text="A" xmlUrl="http://a.ressac-test.invalid/feed"/>
        <outline text="B" xmlUrl="http://b.ressac-test.invalid/feed"/>
        <outline text="C" xmlUrl="http://c.ressac-test.invalid/feed"/>
        <outline text="D" xmlUrl="http://d.ressac-test.invalid/feed"/>
        </body></opml>"#;
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), "http://127.0.0.1:9").unwrap();
    app.import_opml(opml, false).unwrap();
    let recorder = Recorder::default();
    let summary = app
        .refresh(RefreshOptions::default(), &recorder, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    assert!(summary.network_unreachable);
    assert_eq!(summary.count(Status::Unchecked), 4);
    assert_eq!(summary.count(Status::Dead), 0);
    let events = recorder.0.lock().unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::NetworkUnreachable { failed: 4, attempted: 4 })));
}
```

- [ ] **Step 2:** Add `pub mod service;` to `lib.rs` and create an empty `service.rs` with its doc comment. Run `cargo test -p ressac-core --test end_to_end`. Expected: compile errors (`Ressac` not found).

- [ ] **Step 3: Implement** `crates/ressac-core/src/service.rs`

```rust
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
    Started { total: usize },
    FeedDone { feed_id: FeedId, title: String, error: Option<String> },
    NetworkUnreachable { failed: usize, attempted: usize },
    Probing { what: String, total: usize },
    WaybackDone { found: usize },
    Finished { summary: Summary },
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
        Self::with_parts(Store::open(path)?, FetchLimits::default(), PRODUCTION_WAYBACK)
    }

    pub fn with_parts(store: Store, limits: FetchLimits, wayback_base: &str) -> Result<Self> {
        let wayback_gap = limits.per_host_gap;
        Ok(Self { store, source: LocalSource::new(limits)?, wayback: Wayback::new(wayback_base)?, wayback_gap })
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
    pub async fn refresh(&self, opts: RefreshOptions, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<Summary> {
        let started = Utc::now();
        let run_id = ts(started);
        let due = self.store.feeds_to_fetch(started, opts.force)?;
        let titles: HashMap<FeedId, String> = self.store.feeds()?.into_iter().map(|f| (f.id, f.title)).collect();
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
            tracing::warn!(network_failures, attempted, "network looks unreachable; failures not counted");
            sink.emit(&Event::NetworkUnreachable { failed: network_failures, attempted });
        } else if !stop.load(Ordering::Relaxed) {
            self.probe_https(&fresh, sink, stop.clone()).await?;
            self.diagnose_all(Utc::now())?;
            self.discover_replacements(sink, Utc::now()).await?;
        }
        let mut summary = self.diagnose_all(Utc::now())?;
        summary.network_unreachable = unreachable;
        tracing::info!(attempted, "refresh finished");
        sink.emit(&Event::Finished { summary: summary.clone() });
        Ok(summary)
    }

    /// For freshly fetched `http://` feeds never probed: does `https://` serve the same feed?
    async fn probe_https(&self, fresh: &HashMap<FeedId, ParsedFeed>, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<()> {
        let candidates: Vec<FeedRef> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| f.state == "active" && f.https_available.is_none() && fresh.contains_key(&f.id))
            .filter_map(|f| https_variant(&f.xml_url).map(|url| FeedRef { id: f.id, url, etag: None, last_modified: None }))
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        sink.emit(&Event::Probing { what: "https".into(), total: candidates.len() });
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
                    && f.discovery_checked_at.as_deref().and_then(parse_ts).is_none_or(|t| t < recheck_before)
            })
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        sink.emit(&Event::Probing { what: "autodiscovery".into(), total: candidates.len() });
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
        let feeds: Vec<FeedRow> = self.store.feeds()?.into_iter().filter(|f| f.state == "active").collect();
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
                guids: entries.iter().take(LINK_GUIDS).map(|e| e.guid.clone()).collect(),
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
    pub async fn lookup_wayback(&self, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<usize> {
        let diags = self.store.diagnoses()?;
        let targets: Vec<(FeedId, String, Option<DateTime<Utc>>)> = self
            .store
            .feeds()?
            .into_iter()
            .filter(|f| f.state == "active" && f.wayback_url.is_none())
            .filter_map(|f| {
                let d = diags.get(&f.id)?;
                matches!(d.status, Status::Dead | Status::Hijacked)
                    .then(|| f.html_url.clone().map(|site| (f.id, site, d.cadence.last_entry_at)))
                    .flatten()
            })
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        sink.emit(&Event::Probing { what: "wayback".into(), total: targets.len() });
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
        for f in self.store.feeds()?.into_iter().filter(|f| f.state == "active") {
            summary.total += 1;
            let d = diags.get(&f.id);
            let status = d.map_or(Status::Unchecked, |d| d.status);
            *summary.counts.entry(status.as_str().to_string()).or_default() += 1;
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
    let page = source.fetch_raw(feed.html_url.as_deref()?, None, None).await;
    if page.error.is_some() {
        return None;
    }
    let html = String::from_utf8_lossy(&page.body);
    let candidate = alternate_feeds(&html, &page.final_url)
        .into_iter()
        .find(|u| url_key(u) != feed.url_key)?;
    let raw = source.fetch_raw(&candidate, None, None).await;
    let parses = raw.error.is_none() && parse_feed(&raw.body, raw.content_type.as_deref(), &raw.final_url, now).is_ok();
    parses.then_some(candidate)
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --test end_to_end` → `2 passed`. Common pitfalls:
  - The refresh future holds `&Store` (`!Sync`) across awaits. That is fine because tests and `main` await it directly; never `tokio::spawn` it.
  - If `fetching.await` returns a `JoinError`, it is ignored on purpose (see the comment).

- [ ] **Step 5: Verification gate**, then commit and push ("Ressac facade: refresh pipeline, network guard, probes, Wayback; end-to-end test", both attribution lines).
