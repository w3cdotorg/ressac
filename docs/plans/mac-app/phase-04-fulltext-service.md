# Phase 4: `Ressac::extract_entry`, full text during refresh, new events

Read the plan index first. Global Constraints apply. Spec §6 ("Full-article extraction", "During fetch", "Extracted HTML safety") and the event shapes of §6.

**Files (3):**
- Modify: `crates/ressac-core/src/service.rs` (tests inline)
- Modify: `crates/ressac-core/tests/end_to_end.rs`
- Modify: `crates/ressac/src/output.rs` (the progress sink must stay exhaustive over `Event`)

**Builds on (fixed by earlier phases, do not redefine):**
- Phase 1, `ressac_core::extract`: `pub struct Article { pub html: String, pub title: Option<String>, pub byline: Option<String>, pub text: String }`, `pub enum ExtractError { NoContent, LoginWall, Challenge, Fetch(ErrorKind) }` with `fn sentence(&self) -> String`, and `pub fn extract_article(html: &str, url: &str) -> Result<Article, ExtractError>`.
- Phase 2, `Store`:
  - `save_fulltext(&self, entry_id: i64, outcome: &Result<Article, ExtractError>, now: DateTime<Utc>) -> Result<()>`, an upsert, so a later call replaces an earlier failure;
  - `fulltext(&self, entry_id: i64) -> Result<Option<FulltextRow>>`;
  - `prefer_full_text(&self, feed_id) -> Result<bool>` and `set_prefer_full_text(&self, feed_id, on) -> Result<()>`;
  - `entry_for_extract(&self, entry_id) -> Result<Option<(FeedId, Option<String>)>>`;
  - `entries_needing_fulltext(&self, feed_id, since, limit) -> Result<Vec<(i64, String)>>`, which returns entries fetched at or after `since` that have no `entry_fulltext` row.
- Phase 3: `RefreshOptions { force, only }` (still `Default`), and `refresh` honours `only`.

Start by checking that the tree is green: `cargo test --workspace`.

### Task 4.1: New events and the `ExtractOutcome` type

**Interfaces:**
- Consumes: `service::{Event, EventSink, Summary}` (existing).
- Produces (`ressac_core::service`):
  - `pub const FULLTEXT_PER_FEED_PER_RUN: usize = 20;`
  - `pub const FULL_ARTICLES: &str = "full articles";`, the `what` of the `Probing` event that precedes the full-text pass;
  - `Event::Extracted { entry_id: i64, ok: bool, error: Option<String> }`, `Event::Decided { feed_id: FeedId, decision: String }` and `Event::Undone { feed_id: Option<FeedId> }`, serialised with the tags `"extracted"`, `"decided"` and `"undone"`. `None` serialises as `null` and is never skipped;
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum ExtractOutcome { Saved, Failed(String), NoUrl, UnknownEntry }`.
- Produces (`ressac` binary, `output.rs`): the private `fn probing_message(what: &str, total: usize) -> String`. The progress sink ignores the three new events.

- [ ] **Step 1: Write the failing tests.**

In `crates/ressac-core/src/service.rs`, inside `mod tests`, append:

```rust
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
```

In `crates/ressac/src/output.rs`, inside `mod tests`, append:

```rust
    #[test]
    fn probing_message_names_full_articles_by_count() {
        assert_eq!(probing_message("https", 3), "probing https for 3 feeds…");
        assert_eq!(
            probing_message(ressac_core::service::FULL_ARTICLES, 20),
            "fetching 20 full articles…"
        );
    }
```

- [ ] **Step 2: Run red.** `cargo test -p ressac-core --lib service::tests::reading_and_garden_events_have_stable_json`. Expected: compile errors, ``no variant named `Extracted` `` and ``cannot find value `FULL_ARTICLES` ``.

- [ ] **Step 3: Implement in `service.rs`.**

Below `pub const PRODUCTION_WAYBACK: &str = "https://archive.org";`, add:

```rust
/// New entries extracted per `prefer_full_text` feed and refresh (spec §6).
pub const FULLTEXT_PER_FEED_PER_RUN: usize = 20;
/// The `what` of the `Probing` event announcing the full-article pass of a refresh.
pub const FULL_ARTICLES: &str = "full articles";
```

Replace the `Event` enum with:

```rust
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
}
```

Below `impl EventSink for NullSink { … }`, add:

```rust
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
```

- [ ] **Step 4: Implement in `output.rs`.** In `impl EventSink for Sink`, replace the two last arms of the `Display::Progress(bar) => match event { … }` block:

```rust
                Event::Probing { what, total } => bar.set_message(format!("probing {what} for {total} feeds…")),
                Event::WaybackDone { .. } | Event::Finished { .. } => {}
```

with:

```rust
                Event::Probing { what, total } => bar.set_message(probing_message(what, *total)),
                Event::WaybackDone { .. }
                | Event::Finished { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. } => {}
```

Below the `impl EventSink for Sink` block, add:

```rust
/// Progress-bar message for a probe phase. The full-article pass counts entries, not feeds.
fn probing_message(what: &str, total: usize) -> String {
    if what == FULL_ARTICLES {
        format!("fetching {total} full articles…")
    } else {
        format!("probing {what} for {total} feeds…")
    }
}
```

and change the import line `use ressac_core::service::{Event, EventSink, Summary};` to:

```rust
use ressac_core::service::{Event, EventSink, FULL_ARTICLES, Summary};
```

- [ ] **Step 5: Run green.** `cargo test -p ressac-core --lib service::tests` and `cargo test -p ressac --bin ressac output::tests`. Expected: all pass, including the two new tests.

- [ ] **Step 6: Commit.**

```bash
git add crates/ressac-core/src/service.rs crates/ressac/src/output.rs
git commit -m "Extracted, Decided and Undone events; ExtractOutcome"
```

### Task 4.2: `Ressac::extract_entry`

**Interfaces:**
- Consumes: `extract::{Article, ExtractError, extract_article}`, `Store::{entry_for_extract, save_fulltext, fulltext}`, `LocalSource::fetch_raw(&self, url: &str, etag: Option<&str>, last_modified: Option<&str>) -> RawResponse` (follows redirects by hand, applies `FetchLimits::timeout` (30 s by default) to the whole exchange, caps the body, maps 401/403/429/503 challenge bodies to `ErrorKind::Challenge`), and the existing private `is_challenged(&RawResponse) -> bool` in `service.rs`.
- Produces:
  - `pub async fn extract_entry(&self, entry_id: i64, sink: &dyn EventSink) -> Result<ExtractOutcome>` on `Ressac`;
  - private `type Extraction = std::result::Result<Article, ExtractError>;`;
  - private `fn article_from(raw: &RawResponse) -> Extraction`, which is pure. It maps a challenge (an error or a 200 interstitial) to `Challenge`, `Http(401)` to `LoginWall`, any other fetch error to `Fetch(kind)`, and a body that is not HTML (PDF, image, feed) to `NoContent`. Otherwise it calls `extract_article(body, final_url)`;
  - private `async fn fetch_article(source: &LocalSource, url: &str) -> Extraction`;
  - private `fn save_extraction(&self, entry_id: i64, outcome: &Extraction, sink: &dyn EventSink) -> Result<Option<String>>` on `Ressac`, which stores the outcome, emits `Extracted` and returns the failure sentence.

Review Focus 1 is pinned here: `extract_without_url_is_a_usage_error`. Phase 5 turns `NoUrl` into exit `2` with "This entry has no link.".

- [ ] **Step 1: Write the failing tests** in `service.rs` `mod tests`. Add these imports at the top of `mod tests`, below the existing `use` lines:

```rust
    use std::sync::Mutex;
```

Then append:

```rust
    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventSink for Recorder {
        fn emit(&self, event: &Event) {
            self.0.lock().unwrap().push(event.clone());
        }
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
        let store = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body><outline text="T" xmlUrl="https://t.example/feed"/></body></opml>"#;
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
        serve(&server, "/article", 200, &article_page(), "text/html; charset=utf-8").await;
        let (app, linked, _) = app_with_entries(&server.uri());
        let recorder = Recorder::default();
        assert_eq!(
            app.extract_entry(linked, &recorder).await.unwrap(),
            ExtractOutcome::Saved
        );
        let row = app.store().fulltext(linked).unwrap().expect("a stored article");
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

    #[tokio::test]
    async fn extract_of_an_unknown_entry_is_reported() {
        let (app, _, _) = app_with_entries("http://127.0.0.1:9");
        assert_eq!(
            app.extract_entry(9_999, &NullSink).await.unwrap(),
            ExtractOutcome::UnknownEntry
        );
    }
```

- [ ] **Step 2: Run red.** `cargo test -p ressac-core --lib service::tests`. Expected: compile errors, ``cannot find function `article_from` `` and ``no method named `extract_entry` ``.

- [ ] **Step 3: Implement in `service.rs`.**

Add to the `use crate::…` imports:

```rust
use crate::extract::{Article, ExtractError, extract_article};
```

Below the `ExtractOutcome` enum, add:

```rust
/// What fetching and extracting one article page gave.
type Extraction = std::result::Result<Article, ExtractError>;
```

Inside `impl Ressac`, below `summary`, add:

```rust
    /// Fetches the entry's page, extracts the article and stores the outcome (spec §6).
    /// A failure is stored too, as one English sentence; a later call retries.
    pub async fn extract_entry(&self, entry_id: i64, sink: &dyn EventSink) -> Result<ExtractOutcome> {
        let Some((_, url)) = self.store.entry_for_extract(entry_id)? else {
            return Ok(ExtractOutcome::UnknownEntry);
        };
        let Some(url) = url.filter(|u| !u.trim().is_empty()) else {
            return Ok(ExtractOutcome::NoUrl);
        };
        let outcome = fetch_article(&self.source, &url).await;
        Ok(match self.save_extraction(entry_id, &outcome, sink)? {
            None => ExtractOutcome::Saved,
            Some(sentence) => ExtractOutcome::Failed(sentence),
        })
    }

    /// Stores one extraction outcome and emits `Extracted`. Returns the failure sentence.
    fn save_extraction(&self, entry_id: i64, outcome: &Extraction, sink: &dyn EventSink) -> Result<Option<String>> {
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
```

Below the existing free function `is_challenged`, add:

```rust
/// Fetches an article page through the feed source: same client, user agent, body cap and
/// timeout (the whole exchange, redirects included).
async fn fetch_article(source: &LocalSource, url: &str) -> Extraction {
    article_from(&source.fetch_raw(url, None, None).await)
}

/// Maps a page fetch to an extraction outcome. A 401 asks for credentials, so it reads as a
/// login wall; other fetch errors keep their kind ("HTTP 404.", "Timeout.").
fn article_from(raw: &RawResponse) -> Extraction {
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
```

- [ ] **Step 4: Run green.** `cargo test -p ressac-core --lib service::tests`. Expected: every test passes (the 6 new ones among them).

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac-core/src/service.rs
git commit -m "Ressac::extract_entry: fetch the entry page, extract and store the article"
```

### Task 4.3: Full text during refresh

**Interfaces:**
- Consumes: `Store::{prefer_full_text, entries_needing_fulltext}`, `save_extraction` and `fetch_article` (Task 4.2), and `FULLTEXT_PER_FEED_PER_RUN` and `FULL_ARTICLES` (Task 4.1).
- Produces:
  - the private method `async fn fetch_full_texts(&self, fetched: &[FeedId], since: DateTime<Utc>, sink: &dyn EventSink, stop: &AtomicBool) -> Result<()>` on `Ressac`;
  - `refresh` calls it after the probes and before the final diagnosis, unless the network guard tripped or a stop was requested. `fetched` is this run's feeds that returned a parsed feed, in ascending id order, and `since` is the run's start;
  - the field `wayback_gap` becomes `host_gap`, "the pause between two requests to the same host" (Wayback lookups and consecutive article pages on one host). It is still `limits.per_host_gap`.

- [ ] **Step 1: Write the failing tests** at the end of `crates/ressac-core/tests/end_to_end.rs`. Extend the imports at the top of the file:
  - `use std::sync::atomic::AtomicBool;` becomes `use std::sync::atomic::{AtomicBool, Ordering};`;
  - `use ressac_core::model::{Action, Status};` becomes `use ressac_core::model::{Action, FeedId, Status};`;
  - `use ressac_core::service::{Event, EventSink, RefreshOptions, Ressac};` becomes `use ressac_core::service::{Event, EventSink, FULL_ARTICLES, FULLTEXT_PER_FEED_PER_RUN, RefreshOptions, Ressac};`;
  - `use wiremock::matchers::{method, path};` becomes `use wiremock::matchers::{method, path, path_regex};`.

  Keep any import that Phase 3 added. Then append:

```rust
const PARA: &str = "The tide comes in twice a day, pulled by the moon and the sun, and the shore keeps a careful record of every visit in sand, shells and drifting weed.";

fn article_page() -> String {
    let body: String = (0..8)
        .map(|i| format!("<p>Paragraph {i}. {PARA} {PARA}</p>"))
        .collect();
    format!(
        "<!DOCTYPE html><html><head><title>Tides</title></head><body><article><h1>Tides</h1>{body}</article></body></html>"
    )
}

/// An RSS feed whose items link to `{base}/page/{guid}` (no dates: they read as just published).
fn linked_rss(title: &str, base: &str, guids: &[String]) -> String {
    let items: String = guids
        .iter()
        .map(|g| format!("<item><guid>{g}</guid><title>Entry {g}</title><link>{base}/page/{g}</link></item>"))
        .collect();
    format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>{base}/</link><description>A feed</description>{items}</channel></rss>"#
    )
}

/// Two feeds served by `server`: Long (25 linked entries, "Always fetch full article" on)
/// and Short (2 linked entries, off). Returns the app and both feed ids.
async fn full_text_app(server: &MockServer) -> (Ressac, FeedId, FeedId) {
    let base = server.uri();
    let long: Vec<String> = (0..25).map(|i| format!("l{i}")).collect();
    let short: Vec<String> = (0..2).map(|i| format!("s{i}")).collect();
    serve(server, "/long.xml", ok_rss(linked_rss("Long", &base, &long))).await;
    serve(server, "/short.xml", ok_rss(linked_rss("Short", &base, &short))).await;
    Mock::given(method("GET"))
        .and(path_regex("^/page/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(article_page(), "text/html"))
        .mount(server)
        .await;
    let opml = format!(
        r#"<opml><body><outline text="Long" xmlUrl="{base}/long.xml"/><outline text="Short" xmlUrl="{base}/short.xml"/></body></opml>"#
    );
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
    app.import_opml(&opml, false).unwrap();
    let id = |title: &str| {
        app.store()
            .feeds()
            .unwrap()
            .into_iter()
            .find(|f| f.title == title)
            .unwrap()
            .id
    };
    let (long_id, short_id) = (id("Long"), id("Short"));
    app.store().set_prefer_full_text(long_id, true).unwrap();
    (app, long_id, short_id)
}

fn epoch() -> DateTime<Utc> {
    "2000-01-01T00:00:00Z".parse().unwrap()
}

#[tokio::test]
async fn refresh_fetches_full_articles_for_feeds_that_prefer_them() {
    let server = MockServer::start().await;
    let (app, long, short) = full_text_app(&server).await;
    let recorder = Recorder::default();
    app.refresh(RefreshOptions::default(), &recorder, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();

    let left = |feed_id| app.store().entries_needing_fulltext(feed_id, epoch(), 100).unwrap().len();
    assert_eq!(left(long), 25 - FULLTEXT_PER_FEED_PER_RUN, "at most 20 per feed and run");
    assert_eq!(left(short), 2, "Short does not prefer full articles");

    let events = recorder.0.lock().unwrap().clone();
    let announced = events
        .iter()
        .position(|e| {
            *e == Event::Probing {
                what: FULL_ARTICLES.into(),
                total: FULLTEXT_PER_FEED_PER_RUN,
            }
        })
        .expect("the full-article pass is announced");
    let extracted: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::Extracted { ok: true, error: None, .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(extracted.len(), FULLTEXT_PER_FEED_PER_RUN);
    assert!(extracted.iter().all(|i| *i > announced));
    assert!(matches!(events.last(), Some(Event::Finished { .. })));

    // A forced second run stores no new entries, so nothing more is extracted.
    let again = Recorder::default();
    app.refresh(
        RefreshOptions {
            force: true,
            ..RefreshOptions::default()
        },
        &again,
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap();
    assert!(!again.0.lock().unwrap().iter().any(|e| matches!(e, Event::Extracted { .. })));
}

/// Raises the stop flag on the first extraction, as Ctrl-C or the app's stop button would.
struct StopOnFirstExtract {
    events: Mutex<Vec<Event>>,
    stop: Arc<AtomicBool>,
}

impl EventSink for StopOnFirstExtract {
    fn emit(&self, event: &Event) {
        if matches!(event, Event::Extracted { .. }) {
            self.stop.store(true, Ordering::Relaxed);
        }
        self.events.lock().unwrap().push(event.clone());
    }
}

#[tokio::test]
async fn a_stop_request_ends_the_full_article_pass_between_entries() {
    let server = MockServer::start().await;
    let (app, long, _) = full_text_app(&server).await;
    let stop = Arc::new(AtomicBool::new(false));
    let sink = StopOnFirstExtract {
        events: Mutex::default(),
        stop: stop.clone(),
    };
    app.refresh(RefreshOptions::default(), &sink, stop).await.unwrap();
    let events = sink.events.lock().unwrap();
    assert_eq!(
        events.iter().filter(|e| matches!(e, Event::Extracted { .. })).count(),
        1
    );
    assert!(matches!(events.last(), Some(Event::Finished { .. })));
    assert_eq!(
        app.store().entries_needing_fulltext(long, epoch(), 100).unwrap().len(),
        24
    );
}
```

- [ ] **Step 2: Run red.** `cargo test -p ressac-core --test end_to_end`. Expected: the two new tests fail. `refresh_fetches_full_articles_for_feeds_that_prefer_them` panics on `left(long)` (`25` instead of `5`), and `a_stop_request_ends_the_full_article_pass_between_entries` asserts `0` extractions against the expected `1`. The existing tests still pass.

- [ ] **Step 3: Implement in `service.rs`.**

Add `use url::Url;` below `use tokio::sync::mpsc;`.

Rename the field, its doc comment and both uses:
- in `pub struct Ressac`, replace
  ```rust
      /// Pause between Wayback requests (1 request per second in production).
      wayback_gap: Duration,
  ```
  with
  ```rust
      /// Pause between two requests to the same host: Wayback lookups and consecutive
      /// article pages (1 request per second in production).
      host_gap: Duration,
  ```
- in `with_parts`, replace `let wayback_gap = limits.per_host_gap;` with `let host_gap = limits.per_host_gap;` and the struct field `wayback_gap,` with `host_gap,`;
- in `lookup_wayback`, replace `tokio::time::sleep(self.wayback_gap).await;` with `tokio::time::sleep(self.host_gap).await;`.

In `refresh`, insert this block just before `let mut summary = self.diagnose_all(Utc::now())?;`, the final diagnosis. Leave Phase 3's `only` handling above it untouched:

```rust
        if !unreachable && !stop.load(Ordering::Relaxed) {
            let mut fetched: Vec<FeedId> = fresh.keys().copied().collect();
            fetched.sort_unstable();
            self.fetch_full_texts(&fetched, started, sink, &stop).await?;
        }
```

Inside `impl Ressac`, below `discover_replacements`, add:

```rust
    /// Extracts the entries this run stored for `prefer_full_text` feeds, at most
    /// `FULLTEXT_PER_FEED_PER_RUN` per feed. Stops between entries on request, and waits
    /// `host_gap` between two pages of the same host.
    async fn fetch_full_texts(
        &self,
        fetched: &[FeedId],
        since: DateTime<Utc>,
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
        let mut previous_host: Option<String> = None;
        for (entry_id, url) in queue {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let host = page_host(&url);
            if host.is_some() && host == previous_host {
                tokio::time::sleep(self.host_gap).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            let outcome = fetch_article(&self.source, &url).await;
            self.save_extraction(entry_id, &outcome, sink)?;
            previous_host = host;
        }
        Ok(())
    }
```

Below `fetch_article`, add:

```rust
/// The host of an article URL, for the per-host pause.
fn page_host(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_owned)
}
```

- [ ] **Step 4: Run green.** `cargo test -p ressac-core --test end_to_end` (all pass), then `cargo test -p ressac-core` (nothing else broke).

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac-core/src/service.rs crates/ressac-core/tests/end_to_end.rs
git commit -m "Fetch full articles during refresh for feeds that prefer them"
```

### Verification gate

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. Fix what `cargo fmt` reports by running `cargo fmt --all`, then commit `"cargo fmt"` if anything changed. Then push (Global Constraints).

### Notes for the executor

- **`output.rs` changes together with `Event`.** The progress sink in `crates/ressac/src/output.rs` matches `Event` exhaustively. Adding the variants without its arm breaks the workspace build, so Task 4.1 changes both files.
- **Do not spawn `refresh` or `extract_entry`.** `Store` wraps a `rusqlite::Connection`, which is `!Sync`. The futures hold `&Store` across awaits, which is fine because callers `.await` them directly. Never `tokio::spawn` them.
- **Reuse `is_challenged` in `article_from`.** It already exists in `service.rs`, and the `CHALLENGE` constant used by the tests is in `service.rs`'s `mod tests`. A challenge is checked before the error mapping, so a 403 challenge reads "Anti-bot challenge page." and not "HTTP 403.".
- **The 401 mapping is this phase's choice.** `Http(401)` → `LoginWall` is not in the spec text; 401 literally means "credentials required". Phase 1 may also detect HTML login walls inside `extract_article`. Both lead to the same sentence.
- **Pages are decoded with `String::from_utf8_lossy`.** The workspace has no `encoding_rs` dependency, and `reqwest` is built without its `charset` feature, so non-UTF-8 pages lose their accented characters. This is accepted for v1. If Phase 1's spike found this to matter, raise it before changing dependencies.
- **`wiremock`:** `server.reset().await` drops every mock, and `path_regex` exists in 0.6.5 (`wiremock::matchers::path_regex`). Both were checked in the registry source.
- **Only new entries are extracted.** `entries_needing_fulltext(feed, since = run start, …)` relies on Phase 2 setting `entry.fetched_at` on insert only, which is the current upsert's behaviour. Entries 21 and later of a large first fill are never extracted automatically. This is accepted by the spec ("new entries … at most 20 per feed per run"), and the reader's toggle fetches them on demand.
- **The 20 extracted entries are not specified.** Which 20 of a 25-entry fill get extracted depends on Phase 2's `ORDER BY`, so the tests count rows and never assert specific entries.
- **The article fixture was checked against `dom_smoothie` 0.18.2.** `article_page()` was run through `Readability::new(html, Some(url), None)?.parse()`, which gave 2,496 characters of text containing "Paragraph 7.". If Phase 1's `extract_article` adds a stricter length floor, lengthen the page by raising the paragraph count, not by changing the assertions.
