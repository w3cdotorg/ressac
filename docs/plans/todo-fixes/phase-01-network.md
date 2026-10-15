# Phase 1: Network guard and per-host pacing

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** nothing. This is the first phase of the todo-fixes plan, on branch `todo-fixes` at `855cd2c` (`main` right after PR #5). The phases run in order: 1, 2, 3 (3a, 3b, 3c), 4 (4a, 4b), 5, 6, 7, 8, then 9 (`TODO.md`, `notes.md`, `README.md`). This one touches Rust only. Phase 3 (Task 3.6) edits `crates/ressac-core/src/service.rs` again, next to this phase's `refresh` edits, and matches on the text this phase leaves. At the end, run the verification gate and stop: wait for Clément's approval before Phase 2.

**Goal:** close the three open items of `TODO.md` « Reporté ▸ Sécurité et réseau ». (1) `discover_one` fetches the feed address it reads from a site page through the guarded article client, so a third-party page can no longer make Ressac request a local or private address (a blind SSRF). (2) The guarded client ignores proxy variables, because a proxy would resolve the target name itself, out of reach of the DNS-time check. (3) The full-article pass waits out the per-host gap after *any* earlier request of the same run to that host: earlier articles (A, B, A), the feed fetch, and the https probe. The fourth item of that section, Tailscale CGNAT, is decided (keep refusing 100.64/10) and only leaves the TODO.

**Tech stack:** Rust 2024 (rust-version 1.99), reqwest 0.13.5 (`default-features = false, features = ["rustls"]`), tokio, wiremock 0.6.5.

## Decision on item 2: `.no_proxy()` on the guarded client (yes), not on the feed client

It is right, for the guarded client only, for four reasons. All were checked against reqwest 0.13.5 and hyper-util 0.1.21 in a scratch copy:

- **The guard cannot hold through a proxy.** The guard has three layers: the start URL, each redirect hop, and DNS resolution (`PublicOnlyResolver`). With `HTTP_PROXY`, `HTTPS_PROXY` or `ALL_PROXY` set (hyper-util's `Matcher::from_env`), reqwest connects to the proxy and sends it the target URL. The proxy resolves the target name and connects to it. `PublicOnlyResolver` only ever sees the proxy's name.
  - The literal-address check of `is_public_url` still runs. But a public-looking name that resolves to `10.x` or `169.254.169.254`, or a DNS-rebinding name, gets through.
  - The proxy then fetches the page from wherever it sits. A proxy is often inside the LAN, or on localhost for a debugging proxy.
- **It also fixes a misleading failure.** A proxy whose own name resolves to a private address, such as a typical company proxy or `http://localhost:PORT`, is refused by `PublicOnlyResolver` itself. Measured with `HTTP_PROXY=http://localhost:<port>`: `refused=true`, `error=Some(Dns)`, and no request reaches the proxy. Today, every article on such a machine fails with "The page is on a local or private network." With `.no_proxy()`, Ressac connects directly instead.
- **The cost is small and contained.**
  - Behind a mandatory proxy with no direct route, full articles and autodiscovered feed addresses fail. The reader falls back to the feed's summary, and autodiscovery records "nothing found" for 30 days.
  - Feed fetching keeps honouring the proxy, because `LocalSource::new` is unchanged. The feed URLs come from the user's own OPML.
  - The Mac app runs the CLI with the GUI environment. That environment rarely carries proxy variables, so the change mostly affects terminal use.
- **macOS system proxy settings are not involved.** reqwest reads them only with its `system-proxy` feature, which this workspace does not enable: `Cargo.lock` has no `system-configuration` crate. So only the environment variables matter, and `.no_proxy()` clears them.

`allow_private = true` (the test hook) keeps today's behaviour, proxy included. The proxy bypass is part of the guard, so lifting the guard lifts it too.

## Files (4)

- Modify: `crates/ressac-core/src/service.rs` (1355 lines): guard in `discover_one`, pacer wiring, tests.
- Modify: `crates/ressac-core/src/source.rs` (1232 lines): `.no_proxy()`, `HostPacer`, tests.
- Modify: `crates/ressac-core/tests/end_to_end.rs` (460 lines): the pipeline test lifts the guard for its local mock web.
- Create: `crates/ressac-core/tests/article_proxy.rs`: its own test binary, because it sets process-wide proxy variables.

`TODO.md` is not touched: Phase 9 ticks the three items and removes the Tailscale line.

**Step 0 rule:** all changes are local edits and additions: one new parameter, one new type, and one helper removed because it becomes unused. No file over 300 lines is restructured, so no cleanup commit is needed. Check with `wc -l` when the phase starts. If `service.rs` or `source.rs` has been restructured since `855cd2c`, apply the rule.

**Line numbers** are those of `855cd2c`. Earlier tasks shift later ones, so always match on the quoted text, and re-read the file before each group of at most three edits (CLAUDE.md rules 6 and 9).

**Builds on (existing code):**
- `LocalSource::new(limits)` (feed client: no guard, honours proxies) and `LocalSource::for_articles(limits, allow_private)` (guarded unless `allow_private`). Both go through the private `LocalSource::build(limits, public_only)`.
- `LocalSource::fetch_raw(&self, url, etag, last_modified) -> RawResponse`. `RawResponse.refused` is set when the guard refused the start URL, a hop, or every resolved address.
- `Ressac` fields: `source` (feed client), `articles` (guarded client), `host_gap` (`limits.per_host_gap`, 1 s in production). `Ressac::allow_private_articles()` swaps `articles` for an unguarded one. The CLI calls it under `RESSAC_ALLOW_PRIVATE_ARTICLES` in debug builds (`crates/ressac/src/main.rs:221-229`).
- `FetchAttempt { requested_url, final_url, redirect_chain: Vec<Hop>, .. }`, with `Hop { status: u16, url: String }`.
- `source::host_of(url: &str) -> String`, which returns `""` when there is no host (`source.rs:556-561`).

**Produces:**
- `async fn discover_one(site: &LocalSource, guarded: &LocalSource, feed: &FeedRow, now: DateTime<Utc>) -> Option<Option<String>>`, private to `service.rs`.
- `pub(crate) struct HostPacer` in `source.rs`, with `new(gap: Duration) -> Self`, `note(&mut self, url: &str, at: Instant)`, `note_attempt(&mut self, attempt: &FetchAttempt, at: Instant)` and `delay(&self, url: &str, now: Instant) -> Duration`.
- New private signatures in `service.rs`:
  - `probe_https(&self, fresh, pacer: &mut HostPacer, sink, stop)`;
  - `fetch_full_texts(&self, fetched, since, pacer: &mut HostPacer, sink, stop)`.

## Review focus

1. **A refused candidate counts as "checked, nothing found"** (`Some(None)`), so a site is not re-fetched on every refresh. The next check comes after `DISCOVERY_RECHECK_DAYS` (30). This is pinned by `a_candidate_on_a_local_address_is_refused_without_a_request`.
2. **The proxy test must prove that the proxy is really in place.** The feed client is the control and must reach the mock proxy. Otherwise the "no request" assertion would hold for the wrong reason (`the_article_client_bypasses_proxy_variables`).
3. **A, B, A pays only what is left of the gap**, not a full extra gap, and a host seen long ago pays nothing (`the_pacer_waits_out_the_gap_per_host_whatever_came_between`).
4. **A redirected feed fetch counts for every host it reached** (`the_pacer_notes_every_host_a_feed_fetch_touched`).
5. **The pacing timing test only asserts a lower bound** (`elapsed >= gap`), so a slow machine cannot make it flaky.

---

### Task 1.1: Autodiscovery fetches the announced feed address through the guarded client

**Files:**
- Modify: `crates/ressac-core/src/service.rs:148`, `:178-179`, `:354-360`, `:664-690` and tests `:742-745`, `:791-792`
- Modify: `crates/ressac-core/tests/end_to_end.rs:134`

The site page address (`html_url`) comes from the user's OPML, so it stays on the feed client, like `xml_url`. The feed address read *from* that page is third-party content, so it goes through `self.articles`.

- [ ] **Step 1: Write the failing test and switch the test helper to the new signature**

In `crates/ressac-core/src/service.rs`, replace the helper at lines 742-745:

```rust
    async fn discover(server: &MockServer) -> Option<Option<String>> {
        let source = LocalSource::new(FetchLimits::default()).unwrap();
        discover_one(&source, &row(format!("{}/", server.uri())), Utc::now()).await
    }
```

with:

```rust
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
```

Then insert a new test before `a_parsing_candidate_is_found`. Replace lines 791-792:

```rust
    #[tokio::test]
    async fn a_parsing_candidate_is_found() {
```

with:

```rust
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
```

- [ ] **Step 2: Run it and watch it fail**

Run: `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH" && cargo test -p ressac-core --lib service::tests`

Expected: the build fails twice with `error[E0061]: this function takes 3 arguments but 4 arguments were supplied`, once in `discover` and once in the new test.

- [ ] **Step 3: Implement**

In `crates/ressac-core/src/service.rs`, replace lines 664-673:

```rust
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
```

with:

```rust
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
```

Replace lines 689-690:

```rust
    let raw = source.fetch_raw(&candidate, None, None).await;
    if is_challenged(&raw) {
```

with:

```rust
    let raw = guarded.fetch_raw(&candidate, None, None).await;
    if raw.refused {
        return Some(None);
    }
    if is_challenged(&raw) {
```

Replace in `discover_replacements`, lines 354-360:

```rust
        let source = &self.source;
        let found: Vec<(FeedId, Option<Option<String>>)> = stream::iter(candidates)
            .map(|f| async move {
                if stop.load(Ordering::Relaxed) {
                    return (f.id, None);
                }
                (f.id, discover_one(source, &f, now).await)
```

with:

```rust
        let (site, guarded) = (&self.source, &self.articles);
        let found: Vec<(FeedId, Option<Option<String>>)> = stream::iter(candidates)
            .map(|f| async move {
                if stop.load(Ordering::Relaxed) {
                    return (f.id, None);
                }
                (f.id, discover_one(site, guarded, &f, now).await)
```

Update the two doc comments that describe the guarded client. Replace line 148:

```rust
    /// Fetches article pages, whose URLs come from feeds: refuses local and private targets.
```

with:

```rust
    /// Fetches URLs read from third-party content (article pages, feed addresses announced by
    /// a site page): refuses local and private targets.
```

Replace lines 178-179:

```rust
    /// Lets article pages be fetched from local and private addresses. For tests against a
    /// local mock server and for the `RESSAC_ALLOW_PRIVATE_ARTICLES` test hook of the CLI.
```

with:

```rust
    /// Lets article pages and autodiscovered feed addresses be fetched from local and private
    /// addresses. For tests against a local mock server and for the
    /// `RESSAC_ALLOW_PRIVATE_ARTICLES` test hook of the CLI.
```

- [ ] **Step 4: Run the unit tests, then the end-to-end test**

Run: `cargo test -p ressac-core --lib service::tests`

Expected: PASS, 21 tests including `a_candidate_on_a_local_address_is_refused_without_a_request`, `a_parsing_candidate_is_found` and `a_challenge_on_the_candidate_feed_is_not_checked`.

Run: `cargo test -p ressac-core --test end_to_end`

Expected: FAIL in `full_pipeline_classifies_every_kind_of_feed` at `end_to_end.rs:167` (`assert_eq!(lost.status, Status::Moved)`), with `left: Dead, right: Moved`. The mock web is on 127.0.0.1, so the guard now refuses `/found.xml`. This is the intended behaviour, and the test has to opt out as the other full-text tests in that file already do (`end_to_end.rs:317-320`).

- [ ] **Step 5: Lift the guard in the pipeline test**

In `crates/ressac-core/tests/end_to_end.rs`, replace line 134:

```rust
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
```

with:

```rust
    // The mock web is on 127.0.0.1: lift the guard so autodiscovery may reach `/found.xml`.
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base)
        .unwrap()
        .allow_private_articles()
        .unwrap();
```

Run: `cargo test -p ressac-core --test end_to_end && cargo test -p ressac --test cli`

Expected: PASS (4 tests and 17 tests). `cli.rs` already sets `RESSAC_ALLOW_PRIVATE_ARTICLES` for every command (`cli.rs:20`).

- [ ] **Step 6: Commit**

```bash
git add crates/ressac-core/src/service.rs crates/ressac-core/tests/end_to_end.rs
git commit -F - <<'EOF'
Autodiscovery: fetch the announced feed address through the guarded client

The feed address read from a site page is third-party content: a page could
point it at a local or private address (blind SSRF). It now goes through the
article client, which refuses such targets; a refused candidate counts as
nothing found. The site page itself, from the user's OPML, stays on the feed
client.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 1.2: The guarded client ignores proxy variables

**Files:**
- Create: `crates/ressac-core/tests/article_proxy.rs`
- Modify: `crates/ressac-core/src/source.rs:229-231`, `:241-243`

This test needs its own binary. reqwest reads proxy variables when a client is built, and setting them in the `ressac-core` unit-test binary would leak into every concurrent test. In edition 2024, `set_var` is `unsafe`. The test runs on a single-threaded runtime and is the only test in its binary.

`203.0.113.7` is TEST-NET-3 (RFC 5737). `is_public_ip` treats it as public, so the URL guard lets it through, and nothing answers there. The only way a request to it can be seen is through the mock proxy. The feed client is the control: it must go through the proxy, which proves that the variable is in effect.

- [ ] **Step 1: Write the failing test**

Create `crates/ressac-core/tests/article_proxy.rs`:

```rust
//! The article client ignores proxy variables: a proxy would resolve the page's host
//! itself, out of reach of the public-address check. The feed client keeps them.
//!
//! Its own test binary, with a single test: proxy variables are process-wide and read
//! when a client is built, so they must not leak into the other tests.

use std::time::Duration;

use ressac_core::source::{FetchLimits, LocalSource};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A public address (TEST-NET-3, RFC 5737) that nothing answers on: reaching the proxy
/// is the only way a request to it can be seen.
const PUBLIC_PAGE: &str = "http://203.0.113.7/page";

#[test]
fn the_article_client_bypasses_proxy_variables() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let proxy = rt.block_on(MockServer::start());
    rt.block_on(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("proxied", "text/html"))
            .mount(&proxy),
    );
    // SAFETY: this binary holds this one test, and the runtime above is single-threaded
    // (its I/O driver runs on this thread), so nothing else reads the environment now.
    unsafe {
        for name in [
            "NO_PROXY",
            "no_proxy",
            "ALL_PROXY",
            "all_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "http_proxy",
            "REQUEST_METHOD",
        ] {
            std::env::remove_var(name);
        }
        std::env::set_var("HTTP_PROXY", proxy.uri());
    }
    let limits = FetchLimits {
        timeout: Duration::from_millis(500),
        ..FetchLimits::default()
    };

    // Control: the feed client honours the variable, so the proxy is really in place.
    let feeds = LocalSource::new(limits.clone()).unwrap();
    let via_proxy = rt.block_on(feeds.fetch_raw(PUBLIC_PAGE, None, None));
    assert_eq!(via_proxy.status, Some(200));
    let seen = || rt.block_on(proxy.received_requests()).unwrap().len();
    assert_eq!(seen(), 1);

    let articles = LocalSource::for_articles(limits, false).unwrap();
    let direct = rt.block_on(articles.fetch_raw(PUBLIC_PAGE, None, None));
    assert_eq!(direct.status, None, "the page itself cannot answer");
    assert_eq!(
        seen(),
        1,
        "the article client must not go through the proxy"
    );
}
```

`REQUEST_METHOD` is removed because hyper-util ignores `HTTP_PROXY` when that variable is set (CGI). `NO_PROXY` is removed so that the developer's own exclusions cannot hide the proxy.

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p ressac-core --test article_proxy`

Expected: FAIL at the `direct.status` assertion, with `assertion left == right failed: the page itself cannot answer`, `left: Some(200)`, `right: None`. The guarded client went through the proxy.

- [ ] **Step 3: Implement**

In `crates/ressac-core/src/source.rs`, replace lines 241-243:

```rust
        if public_only {
            builder = builder.dns_resolver(Arc::new(PublicOnlyResolver));
        }
```

with:

```rust
        if public_only {
            // No proxy either: a proxy resolves the target name itself, which would skip
            // `PublicOnlyResolver`, and it may sit on the very network the guard protects.
            builder = builder
                .dns_resolver(Arc::new(PublicOnlyResolver))
                .no_proxy();
        }
```

Replace lines 229-231:

```rust
    /// A source for article pages, whose URLs come from feeds: http(s) only, and local or
    /// private targets are refused at the start URL, at every redirect hop and after DNS
    /// resolution. `allow_private` turns that off (tests against a local mock server).
```

with:

```rust
    /// A source for URLs read from third-party content (article pages, feed addresses found
    /// by autodiscovery): http(s) only, local or private targets are refused at the start
    /// URL, at every redirect hop and after DNS resolution, and proxy variables are ignored.
    /// `allow_private` turns all of that off (tests against a local mock server).
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cargo test -p ressac-core --test article_proxy && cargo test -p ressac-core --lib source::tests`

Expected: PASS. The proxy test takes about 0.5 s, because the direct connection to 203.0.113.7 times out, or fails at once without a network. The source unit tests are unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/tests/article_proxy.rs crates/ressac-core/src/source.rs
git commit -F - <<'EOF'
Article client: ignore proxy variables

Behind HTTP_PROXY, HTTPS_PROXY or ALL_PROXY the proxy resolves the target
name, so the DNS-time public-address check never ran, and a proxy whose own
name is private made every article fail as "local or private network". The
guarded client now connects directly; feed fetching keeps the proxy.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 1.3: Per-host pacing remembers every host and counts the feed fetches

**Files:**
- Modify: `crates/ressac-core/src/source.rs:4`, `:556-561` and tests before `:1159`
- Modify: `crates/ressac-core/src/service.rs`, as follows:
  - imports `:8`, `:14`, `:27`;
  - doc `:152-153`;
  - `refresh` `:222-225`, `:257`, `:266-267`;
  - `probe_https` `:278-283`, `:311-312`;
  - `fetch_full_texts` `:375-381`, `:402-420`;
  - `page_host` `:639-642`;
  - tests `:1287-1289`, before `:1294`.

**Design:**
- `HostPacer` keeps the last request time of every host. `delay` returns what is left of the gap, so A, B, A waits only the remainder.
- `refresh` creates one pacer and notes every host a feed fetch reached: the requested URL and each redirect hop.
- `probe_https` notes its https probes, which run after the feed fetch and before the full-article pass.
- `fetch_full_texts` waits on the pacer and notes each article it fetched.
- Times are taken when the attempt is received from the channel. That is after the fetch ended, so the pause is never shorter than intended.
- `HostPacer` lives in `source.rs`, next to the lane pacing and `host_of`. `page_host` in `service.rs` becomes unused and is removed.

This task has two red-green cycles and one commit. Until Step 6, `HostPacer` is used only by tests, and a lib build alone would warn about dead code.

- [ ] **Step 1: Write the failing unit tests for `HostPacer`**

In `crates/ressac-core/src/source.rs`, replace lines 1159-1160 (the start of `refresh_delivers_one_attempt_per_feed`):

```rust
    #[tokio::test]
    async fn refresh_delivers_one_attempt_per_feed() {
```

with:

```rust
    #[test]
    fn the_pacer_waits_out_the_gap_per_host_whatever_came_between() {
        let gap = Duration::from_secs(1);
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut pacer = HostPacer::new(gap);
        assert_eq!(pacer.delay("https://a.example/1", t0), Duration::ZERO);
        pacer.note("https://a.example/1", t0);
        pacer.note("https://b.example/1", t0 + ms(300));
        // A, B, A: the second A still owes what is left of the gap since the first one.
        assert_eq!(pacer.delay("https://a.example/2", t0 + ms(400)), ms(600));
        assert_eq!(pacer.delay("https://b.example/2", t0 + ms(400)), ms(900));
        assert_eq!(pacer.delay("https://A.Example/3", t0 + ms(400)), ms(600));
        assert_eq!(pacer.delay("https://a.example/4", t0 + gap), Duration::ZERO);
        assert_eq!(pacer.delay("https://c.example/1", t0), Duration::ZERO);
        assert_eq!(pacer.delay("not a url", t0), Duration::ZERO);
    }

    #[test]
    fn the_pacer_keeps_the_latest_request_of_a_host() {
        let gap = Duration::from_secs(1);
        let t0 = Instant::now();
        let mut pacer = HostPacer::new(gap);
        pacer.note("https://a.example/late", t0 + Duration::from_millis(500));
        pacer.note("https://a.example/early", t0);
        assert_eq!(
            pacer.delay("https://a.example/x", t0 + Duration::from_millis(500)),
            gap
        );
    }

    #[test]
    fn the_pacer_notes_every_host_a_feed_fetch_touched() {
        let gap = Duration::from_secs(1);
        let t0 = Instant::now();
        let mut pacer = HostPacer::new(gap);
        let attempt = FetchAttempt {
            feed_id: 1,
            at: Utc::now(),
            requested_url: "http://old.example/feed".into(),
            final_url: "https://new.example/feed".into(),
            http_status: Some(200),
            error: None,
            redirect_chain: vec![Hop {
                status: 301,
                url: "https://new.example/feed".into(),
            }],
            content_type: None,
            bytes: 0,
            duration_ms: 0,
            not_modified: false,
            etag: None,
            last_modified: None,
            feed: None,
        };
        pacer.note_attempt(&attempt, t0);
        for url in ["http://old.example/a", "https://new.example/a"] {
            assert_eq!(pacer.delay(url, t0), gap, "{url}");
        }
    }

    #[tokio::test]
    async fn refresh_delivers_one_attempt_per_feed() {
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ressac-core --lib source::tests::the_pacer`

Expected: the build fails with `error[E0433]: cannot find type HostPacer in this scope`, three times.

- [ ] **Step 3: Implement `HostPacer`**

In `crates/ressac-core/src/source.rs`, replace line 4:

```rust
use std::collections::BTreeMap;
```

with:

```rust
use std::collections::{BTreeMap, HashMap};
```

Replace lines 556-561:

```rust
fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}
```

with:

```rust
fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Paces requests made one after the other (the full-article pass): at most one request per
/// `gap` to a host. Every host is remembered, not only the previous one, and the feed
/// fetches of the same run are noted too, so A, B, A and a feed followed by one of its
/// articles both wait out the gap.
#[derive(Debug)]
pub(crate) struct HostPacer {
    gap: Duration,
    last: HashMap<String, Instant>,
}

impl HostPacer {
    pub(crate) fn new(gap: Duration) -> Self {
        Self {
            gap,
            last: HashMap::new(),
        }
    }

    /// Records a request to the host of `url` at `at`. An earlier `at` never moves it back;
    /// a URL without a host is ignored.
    pub(crate) fn note(&mut self, url: &str, at: Instant) {
        let host = host_of(url);
        if host.is_empty() {
            return;
        }
        let last = self.last.entry(host).or_insert(at);
        *last = (*last).max(at);
    }

    /// Records every host a fetch attempt reached: the requested URL and each redirect hop.
    pub(crate) fn note_attempt(&mut self, attempt: &FetchAttempt, at: Instant) {
        self.note(&attempt.requested_url, at);
        for hop in &attempt.redirect_chain {
            self.note(&hop.url, at);
        }
    }

    /// How long to wait at `now` before requesting `url`: what is left of the gap since the
    /// host's last request, zero for a host not requested yet.
    pub(crate) fn delay(&self, url: &str, now: Instant) -> Duration {
        self.last.get(&host_of(url)).map_or(Duration::ZERO, |&at| {
            self.gap.saturating_sub(now.saturating_duration_since(at))
        })
    }
}
```

`Instant` and `Duration` are already imported (`source.rs:8`). The `url` crate lowercases domain hosts, so `A.Example` and `a.example` share one entry.

- [ ] **Step 4: Run the unit tests and watch them pass**

Run: `cargo test -p ressac-core --lib source::tests`

Expected: PASS, 34 tests including the three `the_pacer_*` tests.

- [ ] **Step 5: Write the failing refresh-level test**

In `crates/ressac-core/src/service.rs`, insert before the doc comment at line 1294. Replace lines 1294-1295:

```rust
    /// Review Focus 1: an entry with no link (and no title) cannot be extracted; nothing is
    /// stored or emitted, and the CLI turns this into exit 2 (Phase 5).
```

with:

```rust
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
```

Run: `cargo test -p ressac-core --lib the_first_article_waits_out_the_gap_after_its_feed`

Expected: FAIL with `the article followed its feed without a pause: 3…ms` (about 30-40 ms: no sleep happens anywhere in that run today).

- [ ] **Step 6: Wire the pacer into `service.rs`**

Re-read the file first, and again after every three edits.

Imports. Replace line 8:

```rust
use std::time::Duration;
```

with:

```rust
use std::time::{Duration, Instant};
```

Replace lines 13-14 (`url::Url` is only used by `page_host`, removed below):

```rust
use tokio::sync::mpsc;
use url::Url;
```

with:

```rust
use tokio::sync::mpsc;
```

Replace line 27:

```rust
use crate::source::{FeedRef, FetchAttempt, FetchLimits, LocalSource, RawResponse, Source};
```

with:

```rust
use crate::source::{
    FeedRef, FetchAttempt, FetchLimits, HostPacer, LocalSource, RawResponse, Source,
};
```

Field doc. Replace lines 152-153:

```rust
    /// Pause between two requests to the same host: Wayback lookups and consecutive
    /// article pages (1 request per second in production).
```

with:

```rust
    /// Pause between two requests to the same host: Wayback lookups, and article pages after
    /// any request of the run to their host (1 request per second in production).
```

`refresh`. Replace lines 222-225:

```rust
        let (mut attempted, mut network_failures) = (0usize, 0usize);
        let mut fresh: HashMap<FeedId, ParsedFeed> = HashMap::new();
        while let Some(attempt) = rx.recv().await {
            attempted += 1;
```

with:

```rust
        let (mut attempted, mut network_failures) = (0usize, 0usize);
        let mut fresh: HashMap<FeedId, ParsedFeed> = HashMap::new();
        let mut pacer = HostPacer::new(self.host_gap);
        while let Some(attempt) = rx.recv().await {
            attempted += 1;
            pacer.note_attempt(&attempt, Instant::now());
```

Replace line 257:

```rust
            self.probe_https(&fresh, sink, stop.clone()).await?;
```

with:

```rust
            self.probe_https(&fresh, &mut pacer, sink, stop.clone())
                .await?;
```

Replace lines 266-267:

```rust
            self.fetch_full_texts(&fetched, started, sink, &stop)
                .await?;
```

with:

```rust
            self.fetch_full_texts(&fetched, started, &mut pacer, sink, &stop)
                .await?;
```

`probe_https`. Replace lines 278-283:

```rust
    /// For freshly fetched `http://` feeds never probed: does `https://` serve the same feed?
    async fn probe_https(
        &self,
        fresh: &HashMap<FeedId, ParsedFeed>,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
```

with:

```rust
    /// For freshly fetched `http://` feeds never probed: does `https://` serve the same feed?
    /// Each probe is noted in `pacer`.
    async fn probe_https(
        &self,
        fresh: &HashMap<FeedId, ParsedFeed>,
        pacer: &mut HostPacer,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
```

Replace lines 311-312:

```rust
        while let Some(attempt) = rx.recv().await {
            if let Some(http) = fresh.get(&attempt.feed_id)
```

with:

```rust
        while let Some(attempt) = rx.recv().await {
            pacer.note_attempt(&attempt, Instant::now());
            if let Some(http) = fresh.get(&attempt.feed_id)
```

`fetch_full_texts`. Replace lines 375-381:

```rust
    /// `FULLTEXT_PER_FEED_PER_RUN` per feed. Stops between entries on request, and waits
    /// `host_gap` between two pages of the same host.
    async fn fetch_full_texts(
        &self,
        fetched: &[FeedId],
        since: DateTime<Utc>,
        sink: &dyn EventSink,
```

with:

```rust
    /// `FULLTEXT_PER_FEED_PER_RUN` per feed. Stops between entries on request, and keeps
    /// `host_gap` between two requests to the same host, counting those already in `pacer`.
    async fn fetch_full_texts(
        &self,
        fetched: &[FeedId],
        since: DateTime<Utc>,
        pacer: &mut HostPacer,
        sink: &dyn EventSink,
```

Replace lines 402-420:

```rust
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
            let outcome = fetch_article(&self.articles, &url).await;
            if let Err(e) = self.save_extraction(entry_id, &outcome, sink) {
                // One entry's store error must not end the pass or the refresh.
                tracing::warn!(entry_id, error = %e, "storing a full article failed");
            }
            previous_host = host;
        }
```

with:

```rust
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
```

Remove the now-unused helper. Replace lines 639-643:

```rust
/// The host of an article URL, for the per-host pause.
fn page_host(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_owned)
}

```

with nothing. The next line, `/// Maps a page fetch to an extraction outcome. …`, follows the end of `fetch_article` after one blank line.

The existing test calls the old signature. Replace lines 1287-1289:

```rust
        app.fetch_full_texts(&[1], since, &NullSink, &stop)
            .await
            .expect("the pass survives a store error");
```

with:

```rust
        app.fetch_full_texts(
            &[1],
            since,
            &mut HostPacer::new(Duration::ZERO),
            &NullSink,
            &stop,
        )
        .await
        .expect("the pass survives a store error");
```

Before removing `page_host`, search separately for any other reference (CLAUDE.md rule 10). The only call site is the loop replaced above:

```bash
grep -rn "page_host" crates/ apps/
grep -rn "previous_host" crates/
grep -rn "url::Url\|Url::" crates/ressac-core/src/service.rs
```

Expected after the edits: no output for the first two. The third returns nothing either, because `service.rs` no longer uses `Url`.

- [ ] **Step 7: Run the tests and watch them pass**

Run: `cargo test -p ressac-core`

Expected: PASS in every core binary: the lib (255 passed, 1 ignored), `article_proxy` (1), `contract` (8) and `end_to_end` (4), with 0 doctests. That includes `the_first_article_waits_out_the_gap_after_its_feed` (about 0.4 s) and `a_store_error_on_one_entry_does_not_end_the_full_text_pass`.

- [ ] **Step 8: Commit**

```bash
git add crates/ressac-core/src/source.rs crates/ressac-core/src/service.rs
git commit -F - <<'EOF'
Full articles: pace per host across the whole refresh

The pause between two article pages only compared with the previous entry's
host, so A, B, A did not wait, and the first article of a feed followed the
feed fetch at once. A HostPacer now remembers every host, is seeded with the
run's feed fetches and https probes, and waits only what is left of the gap.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 1.4: The verification gate

`TODO.md` is not edited here: the TODO ticks of every phase move to the closing docs phase (Phase 9).

- [ ] **Step 1: Run the verification gate**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected:
- `fmt` prints nothing;
- `clippy` ends with `Finished` and no warning;
- every `test result:` line says `0 failed`: the `ressac` bin unit tests (34), `cli.rs` (17), the core lib (255 passed, 1 ignored), `article_proxy` (1), `contract` (8) and `end_to_end` (4).

No Swift file changed, so the Xcode gate is not needed for this phase.

- [ ] **Step 2: Stop.** Report the gate output and wait for Clément's approval before Phase 2.

---

## Notes for the executor

- **Validated in a scratch copy.** Every edit above was applied to a copy of `855cd2c`. Each "expected failure" was observed as written, and the full Rust gate passed after the last task (fmt, clippy `-D warnings`, `cargo test --workspace`).
- **Out of scope, on purpose:**
  - **Site page redirects.** The site page of autodiscovery (`html_url`) stays on the feed client. It comes from the user's OPML, as feed URLs do, and its redirects are followed unguarded, exactly like feed redirects.
  - **Wayback.** The Wayback client has a fixed base and is not concerned.
  - **Pacing between the feed fetch and its own https probe.** The probe of `https://host` can still follow the `http://host` feed fetch at once. They run in separate lane sets, and fixing that would mean one shared clock inside `LocalSource`.
  - **Discovery requests in the pacer.** The site pages and candidates fetched for Dead feeds are not noted. A Dead feed's host rarely appears in the same run's full-article pass, and noting them would need a pacer shared across `buffer_unordered` tasks.
  - **Article redirects.** Only the requested host is noted after an article fetch, not the hosts it was redirected to.
  - **`extract_entry`.** On-demand extraction from the reader is one user-driven request and is not paced.
- **Names now slightly narrower than what they cover.** The `articles` field, `LocalSource::for_articles`, `Ressac::allow_private_articles` and `RESSAC_ALLOW_PRIVATE_ARTICLES` also cover autodiscovered feed addresses now. Renaming them would touch `main.rs` and `cli.rs`, beyond this phase's five files, so the doc comments say it instead. A rename can ride along with the `main.rs` split planned before sub-project 2b.
- **Tailscale.** Decided on 15/10/2026: keep refusing 100.64/10. No code change; Phase 9 removes the TODO line.
