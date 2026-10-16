# Phase 2: `Ressac::fetch_image`

**Runs after:** Phase 1b. Order of the sub-project: 1a → 1b → **2** → 3 → 4a → 4b → 5a → 5b → 6 → 7 → 8 → 9a → 9b → 10.

Read the plan index (`docs/plans/2026-10-16-terminal-reading.md`) first. Global Constraints apply. This phase implements the core half of spec §7 (Images: fetching) and the `fetch_image` row of spec §10. It owns plan-index **Review Focus 4** (core half: an image URL that is local or private, answers HTML, or is far too large is refused with a placeholder-friendly error, without hanging). The decode limit (20 000 × 20 000) is phase 6's half.

**Builds on (existing code, read before writing):**
- `Ressac { articles: LocalSource, limits: FetchLimits, … }` (`service.rs`): `articles` is `LocalSource::for_articles(limits, false)` in production, the client used for article pages and autodiscovered feeds. `Ressac::allow_private_articles(self) -> Result<Self>` rebuilds it with the guard off, for tests against a local mock server.
- `LocalSource::fetch_raw(&self, url, etag, last_modified) -> RawResponse` (`source.rs`): follows up to `max_redirects` (5) redirects by hand; with the guard on, every URL of the chain is checked with `is_public_url` (http/https only; `localhost`, `*.localhost`, `*.local` and non-public literal IPs refused) **before** it is requested, names are resolved through `PublicOnlyResolver` (non-public addresses dropped, closing DNS rebinding), and proxies are ignored; a refusal sets `raw.refused`. The body is capped at `limits.max_bytes` (10 MB by default; `ErrorKind::TooLarge`, body cleared), the whole exchange at `limits.timeout` (30 s; `ErrorKind::Timeout`).
- `RawResponse { status, error: Option<ErrorKind>, final_url, chain, content_type, body, refused, … }`; `ErrorKind::{TooLarge, Timeout, Http(u16), …}` with `label() -> String` (`"HTTP 404"`).
- `Error` (`error.rs`, `thiserror`); `Ressac::with_parts(store, limits, wayback_base)`; `ressac_core::{error, service, source, store}` are public modules.

**Why an outer timeout.** `LocalSource`'s timeout is the feed client's 30 s. Spec §7 caps an image at 15 s, so `fetch_image` wraps `fetch_raw` in `tokio::time::timeout(IMAGE_TIMEOUT, …)`. The 10 MB cap is the source's own `max_bytes` (the same 10 MB), plus an explicit check of the body length, because the source raises its cap for bodies that look like podcast feeds.

**What is not testable here.** A redirect from a public host to a private address needs a public host; the guard checks every hop with the same code that refuses the start URL (`LocalSource::follow`), so the tests cover the start-URL refusal (loopback through wiremock, a literal private IP, `localhost`) and rely on that shared path for later hops.

**Step 0 check.** `service.rs` (1 773 lines) only gains two constants, two methods, one free function and a small test module at its end; it is not restructured, so the Step 0 rule does not apply. `error.rs` (42 lines) gains one variant.

**Files (3):**
- Modify: `crates/ressac-core/src/error.rs` (`Error::ImageRefused`)
- Modify: `crates/ressac-core/src/service.rs` (`IMAGE_MAX_BYTES`, `IMAGE_TIMEOUT`, `fetch_image`, `fetch_image_within`, `image_body`, `mod image_tests`)
- Create: `crates/ressac-core/tests/fetch_image.rs`

---

### Task 2.1: the refusal error and the response check

- [ ] **Step 1: Write the failing integration tests.** Create `crates/ressac-core/tests/fetch_image.rs`:

```rust
//! `Ressac::fetch_image` against a local mock server (spec §7, Images; Review Focus 4).

use ressac_core::error::Error;
use ressac_core::service::{IMAGE_MAX_BYTES, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The first bytes of a PNG file; the core does not decode, so a stub is enough.
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];

/// `private_ok`: the guard off, to reach the mock server on 127.0.0.1.
fn app(private_ok: bool) -> Ressac {
    let app = Ressac::with_parts(
        Store::open_in_memory().unwrap(),
        FetchLimits::default(),
        "http://127.0.0.1:9",
    )
    .unwrap();
    if private_ok {
        app.allow_private_articles().unwrap()
    } else {
        app
    }
}

async fn serve(server: &MockServer, at: &str, status: u16, body: Vec<u8>, content_type: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, content_type))
        .mount(server)
        .await;
}

/// The refusal reason, or a panic naming what came back instead.
fn refused(outcome: Result<Vec<u8>, Error>) -> String {
    match outcome {
        Err(Error::ImageRefused(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_image_comes_back_whole() {
    let server = MockServer::start().await;
    serve(&server, "/a.png", 200, PNG.to_vec(), "image/png").await;
    let bytes = app(true).fetch_image(&format!("{}/a.png", server.uri())).await.unwrap();
    assert_eq!(bytes, PNG);
}

#[tokio::test]
async fn a_redirect_to_an_image_is_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/old.png"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/new.png"))
        .mount(&server)
        .await;
    serve(&server, "/new.png", 200, PNG.to_vec(), "IMAGE/PNG; charset=binary").await;
    let bytes = app(true).fetch_image(&format!("{}/old.png", server.uri())).await.unwrap();
    assert_eq!(bytes, PNG, "content types compare case-insensitively");
}

#[tokio::test]
async fn a_page_instead_of_an_image_is_refused() {
    let server = MockServer::start().await;
    serve(&server, "/a.png", 200, b"<html>Sign in</html>".to_vec(), "text/html").await;
    let why = refused(app(true).fetch_image(&format!("{}/a.png", server.uri())).await);
    assert_eq!(why, "not an image");
}

#[tokio::test]
async fn an_image_over_10_mb_is_refused() {
    let server = MockServer::start().await;
    serve(&server, "/big.jpg", 200, vec![0u8; IMAGE_MAX_BYTES + 1], "image/jpeg").await;
    let why = refused(app(true).fetch_image(&format!("{}/big.jpg", server.uri())).await);
    assert_eq!(why, "larger than 10 MB");
}

#[tokio::test]
async fn an_http_error_is_named() {
    let server = MockServer::start().await;
    serve(&server, "/gone.png", 404, Vec::new(), "text/plain").await;
    let why = refused(app(true).fetch_image(&format!("{}/gone.png", server.uri())).await);
    assert_eq!(why, "HTTP 404");
}

#[tokio::test]
async fn local_and_private_addresses_are_refused() {
    let server = MockServer::start().await;
    serve(&server, "/a.png", 200, PNG.to_vec(), "image/png").await;
    let guarded = app(false);
    for url in [
        format!("{}/a.png", server.uri()), // 127.0.0.1
        "http://10.0.0.1/a.png".to_string(),
        "http://localhost:1/a.png".to_string(),
        "http://[::1]:1/a.png".to_string(),
    ] {
        assert_eq!(refused(guarded.fetch_image(&url).await), "local or private address", "{url}");
    }
    assert!(server.received_requests().await.unwrap().is_empty(), "nothing reached the server");
}

#[tokio::test]
async fn only_web_addresses_are_fetched() {
    let guarded = app(false);
    for url in ["file:///etc/passwd", "data:image/png;base64,AAAA", "not a url", "ftp://example.org/a.png"] {
        assert_eq!(refused(guarded.fetch_image(url).await), "not a web address", "{url}");
    }
}
```

- [ ] **Step 2: Run them to see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo test -p ressac-core --test fetch_image 2>&1 | tail -n 5
```

Expected: compile errors `cannot find value IMAGE_MAX_BYTES in module service`, `no variant ImageRefused`, `no method named fetch_image`.

- [ ] **Step 3: Implement.** In `crates/ressac-core/src/error.rs`, add the variant after `EditionNotDue`:

```rust
    /// An image the terminal reader asked for and will show as a placeholder (spec §7).
    #[error("Image refused: {0}")]
    ImageRefused(String),
```

In `crates/ressac-core/src/service.rs`, next to the other module constants (after `pub const FULL_ARTICLES`):

```rust
/// The largest image the terminal reader downloads (the app's mosaic limit, spec §7).
pub const IMAGE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// How long one image may take, redirects and body included (spec §7).
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(15);
```

In `impl Ressac`, after `extract_entry`:

```rust
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
```

After `article_from` (free functions near `fetch_article`):

```rust
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
```

(`url` is already a dependency of `ressac-core`; `Duration`, `Error`, `ErrorKind`, `RawResponse` are already imported in `service.rs`.)

- [ ] **Step 4: Run the tests.**

```bash
cargo test -p ressac-core --test fetch_image 2>&1 | tail -n 5
```

Expected: `test result: ok. 7 passed`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/error.rs crates/ressac-core/src/service.rs crates/ressac-core/tests/fetch_image.rs
git commit -m "Core: fetch_image for the terminal reader, through the guarded article client

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2.2: the 15-second limit

- [ ] **Step 1: Write the failing test.** At the end of `service.rs`, a small test module (the existing `mod tests` is large; this one only exercises the private `fetch_image_within`):

```rust
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
        let app = Ressac::with_parts(Store::open_in_memory().unwrap(), FetchLimits::default(), "http://127.0.0.1:9")
            .unwrap()
            .allow_private_articles()
            .unwrap();
        let started = Instant::now();
        let err = app
            .fetch_image_within(&format!("{}/slow.png", server.uri()), Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(matches!(&err, Error::ImageRefused(why) if why == "timed out"), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(2), "the limit is the caller's, not the source's 30 s");
    }

    #[test]
    fn the_limits_are_the_spec_s() {
        assert_eq!(IMAGE_MAX_BYTES, 10 * 1024 * 1024);
        assert_eq!(IMAGE_TIMEOUT, Duration::from_secs(15));
    }
}
```

- [ ] **Step 2: Run it.** Task 2.1 already implemented `fetch_image_within`, so check that the test fails for the right reason by temporarily replacing `limit` with `IMAGE_TIMEOUT` in `fetch_image_within`'s `tokio::time::timeout` call:

```bash
cargo test -p ressac-core --lib service::image_tests 2>&1 | tail -n 5
```

Expected with the temporary change: `a_slow_image_times_out_without_hanging` fails on the elapsed-time assertion (the server's 5 s delay is reached). Restore `limit`.

- [ ] **Step 3: Run the tests and the gate.**

```bash
cargo test -p ressac-core --lib service::image_tests 2>&1 | tail -n 5
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E "^test result|FAILED" | head
```

Expected: `test result: ok. 2 passed`; clippy clean; every `test result: ok`.

- [ ] **Step 4: Commit.**

```bash
git add crates/ressac-core/src/service.rs
git commit -m "Core: an image gets 15 seconds, whatever the feed client allows

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 2

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: all green.

**Interface produced for later phases (exact):** `ressac_core::service::{IMAGE_MAX_BYTES, IMAGE_TIMEOUT}`, `Ressac::fetch_image(&self, url: &str) -> Result<Vec<u8>>` (async), `ressac_core::error::Error::ImageRefused(String)` with the reasons `"not a web address"`, `"local or private address"`, `"timed out"`, `"larger than 10 MB"`, `"not an image"`, or an `ErrorKind::label()` (`"HTTP 404"`). The reader shows `[image: alt text]` for any of them; phase 6 decodes the bytes with `image`'s limits.
