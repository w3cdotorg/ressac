# Phase 6: Cross-feed links, autodiscovery, Wayback client

Read the plan index first. Global Constraints apply.

**Files (5):**
- Create: `crates/ressac-core/src/health/links.rs` (tests inline)
- Modify: `crates/ressac-core/src/health/mod.rs` (add `pub mod links;`)
- Create: `crates/ressac-core/src/discover.rs` (tests inline)
- Create: `crates/ressac-core/src/wayback.rs` (wiremock tests inline)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod discover; pub mod wayback;`)

### Task 6.1: Duplicates and similar titles

**Interfaces:**
- Consumes: `health::{DUPLICATE_OVERLAP, SIMILAR_TITLE_DISTANCE}`, `model::{Action, Diagnosis, FeedId, FolderId, Remark, Status}`.
- Produces (`ressac_core::health::links`):
  - `struct LinkInput { feed_id: FeedId, folder_id: Option<FolderId>, title: String, final_key: String, guids: Vec<String> }`
  - `find_duplicates(feeds: &[LinkInput]) -> HashMap<FeedId, FeedId>`, mapping duplicate → canonical (the lower id)
  - `apply_similar_titles(feeds: &[LinkInput], diags: &mut HashMap<FeedId, Diagnosis>)`

- [ ] **Step 1: Write the failing tests** in `health/links.rs`

```rust
//! Facts that need several feeds at once: duplicates and look-alike titles.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cadence, Evidence};

    fn input(id: FeedId, folder: Option<FolderId>, title: &str, key: &str, guids: &[&str]) -> LinkInput {
        LinkInput {
            feed_id: id,
            folder_id: folder,
            title: title.into(),
            final_key: key.into(),
            guids: guids.iter().map(|g| g.to_string()).collect(),
        }
    }

    fn diag(status: Status) -> Diagnosis {
        Diagnosis {
            status,
            reason: String::new(),
            evidence: Evidence::default(),
            remarks: vec![],
            action: (status == Status::Dead).then_some(Action::Bury),
            hijack_score: 0.0,
            cadence: Cadence::default(),
        }
    }

    #[test]
    fn same_final_address_is_a_duplicate_of_the_older_feed() {
        let dups = find_duplicates(&[input(9, None, "B", "x.org/feed", &[]), input(3, None, "A", "x.org/feed", &[])]);
        assert_eq!(dups, HashMap::from([(9, 3)]));
    }

    #[test]
    fn mostly_shared_entries_make_a_duplicate() {
        let a = input(1, None, "A", "a.org/rss", &["1", "2", "3", "4", "5"]);
        let b = input(2, None, "B", "b.org/rss", &["1", "2", "3", "4", "z"]);
        let c = input(3, None, "C", "c.org/rss", &["1", "2", "x", "y", "z"]);
        let dups = find_duplicates(&[a, b, c]);
        assert_eq!(dups.get(&2), Some(&1));
        assert_eq!(dups.get(&3), None, "2/5 shared is not a duplicate");
    }

    #[test]
    fn tiny_feeds_are_never_duplicates_by_entries() {
        let dups = find_duplicates(&[input(1, None, "A", "a", &["1", "2"]), input(2, None, "B", "b", &["1", "2"])]);
        assert!(dups.is_empty());
    }

    #[test]
    fn dead_look_alike_in_same_folder_is_merged_into_the_living_one() {
        let feeds = [
            input(10, Some(5), "Orange Drenka", "orange.example.org/feed", &[]),
            input(11, Some(5), "Orange-Drenka", "orange-drenka.example.net/rss", &[]),
        ];
        let mut diags = HashMap::from([(10, diag(Status::Alive)), (11, diag(Status::Dead))]);
        apply_similar_titles(&feeds, &mut diags);
        assert_eq!(diags[&11].action, Some(Action::MergeInto { feed_id: 10 }));
        assert!(diags[&11].remarks.contains(&Remark::SimilarTitle { feed_id: 10 }));
        assert!(diags[&10].remarks.contains(&Remark::SimilarTitle { feed_id: 11 }));
        assert_eq!(diags[&10].action, None);
    }

    #[test]
    fn look_alikes_need_same_folder_long_titles_and_a_dead_side() {
        let other_folder = [input(1, Some(1), "Orange Drenka", "a", &[]), input(2, Some(2), "Orange-Drenka", "b", &[])];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Dead))]);
        apply_similar_titles(&other_folder, &mut diags);
        assert!(diags[&2].remarks.is_empty());

        let short = [input(1, Some(1), "BBC", "a", &[]), input(2, Some(1), "CBS", "b", &[])];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Dead))]);
        apply_similar_titles(&short, &mut diags);
        assert!(diags[&2].remarks.is_empty());

        let both_alive = [input(1, Some(1), "Orange Drenka", "a", &[]), input(2, Some(1), "Orange-Drenka", "b", &[])];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Alive))]);
        apply_similar_titles(&both_alive, &mut diags);
        assert!(diags[&2].remarks.is_empty());
    }
}
```

- [ ] **Step 2:** Add `pub mod links;` to `health/mod.rs` and run `cargo test -p ressac-core health::links`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use std::collections::{HashMap, HashSet};

use crate::health::{DUPLICATE_OVERLAP, SIMILAR_TITLE_DISTANCE};
use crate::model::{Action, Diagnosis, FeedId, FolderId, Remark, Status};

/// Entry-overlap duplicates need at least this many entries on both sides.
const MIN_ENTRIES_FOR_OVERLAP: usize = 3;
/// Normalized titles shorter than this are never compared.
const MIN_TITLE_CHARS: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct LinkInput {
    pub feed_id: FeedId,
    pub folder_id: Option<FolderId>,
    pub title: String,
    /// `url_key` of the final URL after redirects (or of `xml_url` when never fetched).
    pub final_key: String,
    /// GUIDs of the 20 newest entries.
    pub guids: Vec<String>,
}

pub fn find_duplicates(feeds: &[LinkInput]) -> HashMap<FeedId, FeedId> {
    let mut sorted: Vec<&LinkInput> = feeds.iter().collect();
    sorted.sort_by_key(|f| f.feed_id);
    let mut dups = HashMap::new();
    let mut by_key: HashMap<&str, FeedId> = HashMap::new();
    for f in &sorted {
        match by_key.get(f.final_key.as_str()) {
            Some(&canonical) => {
                dups.insert(f.feed_id, canonical);
            }
            None => {
                by_key.insert(&f.final_key, f.feed_id);
            }
        }
    }
    for (i, a) in sorted.iter().enumerate() {
        if dups.contains_key(&a.feed_id) {
            continue;
        }
        for b in &sorted[i + 1..] {
            if !dups.contains_key(&b.feed_id) && overlap(&a.guids, &b.guids) >= DUPLICATE_OVERLAP {
                dups.insert(b.feed_id, a.feed_id);
            }
        }
    }
    dups
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.len() < MIN_ENTRIES_FOR_OVERLAP || b.len() < MIN_ENTRIES_FOR_OVERLAP {
        return 0.0;
    }
    let set: HashSet<&String> = a.iter().collect();
    let common = b.iter().filter(|g| set.contains(g)).count();
    common as f64 / a.len().min(b.len()) as f64
}

fn normalized(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Marks Dead/Fossil feeds that look like an Alive/Dormant feed of the same folder
/// (a blog that moved platforms) and proposes merging them into the living one.
pub fn apply_similar_titles(feeds: &[LinkInput], diags: &mut HashMap<FeedId, Diagnosis>) {
    for gone in feeds {
        for alive in feeds {
            if gone.feed_id == alive.feed_id || gone.folder_id != alive.folder_id {
                continue;
            }
            let (a, b) = (normalized(&gone.title), normalized(&alive.title));
            if a.chars().count() < MIN_TITLE_CHARS || strsim::levenshtein(&a, &b) > SIMILAR_TITLE_DISTANCE {
                continue;
            }
            let gone_status = diags.get(&gone.feed_id).map(|d| d.status);
            let alive_status = diags.get(&alive.feed_id).map(|d| d.status);
            if !matches!(gone_status, Some(Status::Dead | Status::Fossil))
                || !matches!(alive_status, Some(Status::Alive | Status::Dormant))
            {
                continue;
            }
            if let Some(d) = diags.get_mut(&gone.feed_id) {
                d.remarks.push(Remark::SimilarTitle { feed_id: alive.feed_id });
                d.action = Some(Action::MergeInto { feed_id: alive.feed_id });
                d.evidence.bullets.push(format!(
                    "Looks like the same site as #{} \"{}\", which is still active.",
                    alive.feed_id, alive.title
                ));
            }
            if let Some(d) = diags.get_mut(&alive.feed_id) {
                d.remarks.push(Remark::SimilarTitle { feed_id: gone.feed_id });
            }
        }
    }
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core health::links` → `5 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/health
git commit -m "Cross-feed links: duplicates by address or entries, look-alike titles

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SCujX8NRvCZ5dpWQPiKUDm"
```

### Task 6.2: Autodiscovery helpers

**Interfaces:**
- Consumes: `parse::ParsedFeed`.
- Produces (`ressac_core::discover`):
  - `alternate_feeds(html: &str, base: &str) -> Vec<String>`. Returns absolute URLs of `<link rel~="alternate">` with a feed MIME type.
  - `same_feed(a: &ParsedFeed, b: &ParsedFeed) -> bool`. True when the titles are equal and at least one entry is shared (by GUID or URL).

- [ ] **Step 1: Write the failing tests** in `discover.rs`

```rust
//! Feed autodiscovery in HTML pages, and "same feed" comparison for the https probe.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{ParsedEntry, ParsedFeed};
    use chrono::Utc;

    #[test]
    fn finds_feed_links_and_resolves_relative_hrefs() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/style.css">
            <link rel="alternate" type="application/rss+xml" title="RSS" href="/feed.xml">
            <link rel="alternate home" type="application/atom+xml" href="https://cdn.example.org/atom">
            <link rel="alternate" type="text/html" hreflang="fr" href="/fr/">
            <link rel="alternate" type="application/feed+json" href="feed.json">
        </head></html>"#;
        assert_eq!(
            alternate_feeds(html, "https://blog.example.org/posts/"),
            vec![
                "https://blog.example.org/feed.xml".to_string(),
                "https://cdn.example.org/atom".to_string(),
                "https://blog.example.org/posts/feed.json".to_string(),
            ]
        );
    }

    #[test]
    fn page_without_feeds_gives_nothing() {
        assert!(alternate_feeds("<html><body>parked</body></html>", "https://x.org/").is_empty());
    }

    fn feed(title: &str, guids: &[&str]) -> ParsedFeed {
        ParsedFeed {
            title: Some(title.into()),
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

    #[test]
    fn same_feed_needs_title_and_a_shared_entry() {
        assert!(same_feed(&feed("Blog", &["1", "2"]), &feed("Blog", &["2", "3"])));
        assert!(!same_feed(&feed("Blog", &["1"]), &feed("Other", &["1"])));
        assert!(!same_feed(&feed("Blog", &["1"]), &feed("Blog", &["9"])));
    }
}
```

- [ ] **Step 2:** Add `pub mod discover;` to `lib.rs`; run `cargo test -p ressac-core discover`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use scraper::{Html, Selector};
use url::Url;

use crate::parse::ParsedFeed;

const FEED_TYPES: &[&str] = &["application/rss+xml", "application/atom+xml", "application/feed+json"];

pub fn alternate_feeds(html: &str, base: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse(r#"link[rel~="alternate"]"#).expect("static selector is valid");
    let base = Url::parse(base).ok();
    doc.select(&selector)
        .filter(|link| {
            link.value()
                .attr("type")
                .is_some_and(|t| FEED_TYPES.contains(&t.trim().to_ascii_lowercase().as_str()))
        })
        .filter_map(|link| link.value().attr("href"))
        .filter_map(|href| match &base {
            Some(b) => b.join(href).ok(),
            None => Url::parse(href).ok(),
        })
        .map(|u| u.to_string())
        .collect()
}

pub fn same_feed(a: &ParsedFeed, b: &ParsedFeed) -> bool {
    a.title == b.title
        && a.entries.iter().any(|x| {
            b.entries
                .iter()
                .any(|y| x.guid == y.guid || (x.url.is_some() && x.url == y.url))
        })
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core discover` → `3 passed`.

- [ ] **Step 5: Commit** (`git add crates/ressac-core/src/discover.rs crates/ressac-core/src/lib.rs`, message "Feed autodiscovery and same-feed comparison", with both attribution lines).

### Task 6.3: Wayback Machine client

**Interfaces:**
- Consumes: `source::USER_AGENT`, `error::{Error, Result}`.
- Produces (`ressac_core::wayback`):
  - `Wayback` (`Clone`), with `new(base: impl Into<String>) -> Result<Self>`
  - `async closest(&self, site: &str, around: Option<DateTime<Utc>>) -> Result<Option<String>>`. Queries `{base}/wayback/available?url=<site>&timestamp=<YYYYMMDD>`.

- [ ] **Step 1: Write the failing tests** in `wayback.rs`

```rust
//! archive.org availability API: the closest snapshot of a site around a date.

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FOUND: &str = r#"{"url":"bibliobsession.net","archived_snapshots":{"closest":{"status":"200","available":true,"url":"http://web.archive.org/web/20210612000000/https://bibliobsession.net/","timestamp":"20210612000000"}}}"#;

    #[tokio::test]
    async fn returns_the_closest_snapshot() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wayback/available"))
            .and(query_param("url", "https://bibliobsession.net/"))
            .and(query_param("timestamp", "20190402"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(FOUND, "application/json"))
            .mount(&server)
            .await;
        let around = "2019-04-02T10:00:00Z".parse().unwrap();
        let url = Wayback::new(format!("{}/", server.uri()))
            .unwrap()
            .closest("https://bibliobsession.net/", Some(around))
            .await
            .unwrap();
        assert_eq!(url.as_deref(), Some("http://web.archive.org/web/20210612000000/https://bibliobsession.net/"));
    }

    #[tokio::test]
    async fn no_snapshot_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"archived_snapshots":{}}"#, "application/json"))
            .mount(&server)
            .await;
        let url = Wayback::new(server.uri()).unwrap().closest("https://x.org/", None).await.unwrap();
        assert_eq!(url, None);
    }

    #[tokio::test]
    async fn server_error_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503)).mount(&server).await;
        assert!(Wayback::new(server.uri()).unwrap().closest("https://x.org/", None).await.is_err());
    }
}
```

- [ ] **Step 2:** Add `pub mod wayback;` to `lib.rs`; run `cargo test -p ressac-core wayback`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use std::time::Duration;

use chrono::{DateTime, Utc};
use url::Url;

use crate::error::{Error, Result};
use crate::source::USER_AGENT;

#[derive(Clone)]
pub struct Wayback {
    client: reqwest::Client,
    base: String,
}

impl Wayback {
    /// `base` is `https://archive.org` in production, a mock server in tests.
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self { client, base: base.into().trim_end_matches('/').to_string() })
    }

    pub async fn closest(&self, site: &str, around: Option<DateTime<Utc>>) -> Result<Option<String>> {
        let mut url = Url::parse(&format!("{}/wayback/available", self.base))
            .map_err(|e| Error::Invalid(format!("bad Wayback base URL: {e}")))?;
        url.query_pairs_mut().append_pair("url", site);
        if let Some(t) = around {
            url.query_pairs_mut().append_pair("timestamp", &t.format("%Y%m%d").to_string());
        }
        let body = self.client.get(url).send().await?.error_for_status()?.bytes().await?;
        let v: serde_json::Value = serde_json::from_slice(&body)?;
        let snap = &v["archived_snapshots"]["closest"];
        Ok(if snap["available"].as_bool() == Some(true) {
            snap["url"].as_str().map(str::to_owned)
        } else {
            None
        })
    }
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core wayback` → `3 passed`.

- [ ] **Step 5: Verification gate**, then commit and push (message "Wayback Machine availability client", both attribution lines).
