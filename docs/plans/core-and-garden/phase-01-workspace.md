# Phase 1: Workspace, URL keys, CI

Read the plan index `docs/plans/2026-10-10-core-and-garden.md` first (Global Constraints apply).

**Files (5):**
- Create: `Cargo.toml`
- Create: `crates/ressac-core/Cargo.toml`
- Create: `crates/ressac-core/src/lib.rs`
- Create: `crates/ressac-core/src/url_key.rs` (tests inline)
- Create: `.github/workflows/ci.yml`

### Task 1.1: Workspace and core crate with URL helpers

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `ressac_core::url_key::url_key(raw: &str) -> String`
  - `repair_url(raw: &str) -> String`
  - `site_of(raw: &str) -> Option<String>`
  - `https_variant(raw: &str) -> Option<String>`

- [ ] **Step 1: Create the workspace manifest** `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["crates/ressac-core"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.99"
license = "MIT"

[workspace.dependencies]
anyhow = "1.0.104"
chrono = { version = "0.4.45", default-features = false, features = ["clock", "serde", "std"] }
clap = { version = "4.6.7", features = ["derive", "env"] }
feed-rs = "3.0.0"
futures = "0.3.34"
indicatif = "0.18.6"
insta = "1.49.0"
open = "5.4.4"
quick-xml = "0.42.0"
ratatui = "0.30.2"
reqwest = { version = "0.13.5", default-features = false, features = ["rustls"] }
rusqlite = { version = "0.40.2", features = ["bundled"] }
scraper = "0.27.0"
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
strsim = "0.11.1"
tempfile = "3.27.0"
thiserror = "2.0.21"
tokio = { version = "1.53.2", features = ["full"] }
tracing = "0.1.44"
tracing-appender = "0.2.5"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
url = "2.5.8"
whatlang = "0.18.0"
wiremock = "0.6.5"
```

- [ ] **Step 2: Create** `crates/ressac-core/Cargo.toml`. All core dependencies are declared now, so later phases don't touch this file.

```toml
[package]
name = "ressac-core"
description = "Storage, fetching, diagnosis and OPML for the Ressac feed reader"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
chrono.workspace = true
feed-rs.workspace = true
futures.workspace = true
quick-xml.workspace = true
reqwest.workspace = true
rusqlite.workspace = true
scraper.workspace = true
serde.workspace = true
serde_json.workspace = true
strsim.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
url.workspace = true
whatlang.workspace = true

[dev-dependencies]
tempfile.workspace = true
wiremock.workspace = true
```

- [ ] **Step 3: Create** `crates/ressac-core/src/lib.rs`

```rust
//! Ressac core: storage, fetching, diagnosis and OPML for the Ressac feed reader.

pub mod url_key;
```

- [ ] **Step 4: Write the failing tests** in `crates/ressac-core/src/url_key.rs`

```rust
//! URL helpers: comparison keys and repairs of malformed OPML URLs.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_ignores_scheme_www_and_trailing_slash() {
        assert_eq!(
            url_key("http://daringfireball.net/feeds/main"),
            url_key("https://daringfireball.net/feeds/main/")
        );
        assert_eq!(url_key("https://www.xkcd.com/rss.xml"), "xkcd.com/rss.xml");
    }

    #[test]
    fn key_keeps_query_and_path_case() {
        assert_eq!(
            url_key("http://lebrunremy.be/WordPress/?feed=rss2"),
            "lebrunremy.be/WordPress?feed=rss2"
        );
    }

    #[test]
    fn key_differs_for_different_feeds() {
        assert_ne!(
            url_key("https://orange.example.org/feed/"),
            url_key("http://orange-drenka.example.net/feeds/posts/default?alt=rss")
        );
    }

    #[test]
    fn key_keeps_explicit_port() {
        assert_eq!(url_key("http://127.0.0.1:8080/a.xml"), "127.0.0.1:8080/a.xml");
    }

    #[test]
    fn repair_double_escaped_ampersand() {
        assert_eq!(
            repair_url(" http://api.flickr.com/x?id=1&amp;lang=en-us&format=rss_200 "),
            "http://api.flickr.com/x?id=1&lang=en-us&format=rss_200"
        );
    }

    #[test]
    fn site_of_handles_country_second_level_domains() {
        assert_eq!(site_of("https://feeds.bbci.co.uk/news/rss.xml").as_deref(), Some("bbci.co.uk"));
        assert_eq!(site_of("https://www.lemonde.fr/rss/une.xml").as_deref(), Some("lemonde.fr"));
        assert_eq!(site_of("not a url"), None);
    }

    #[test]
    fn https_variant_only_for_http() {
        assert_eq!(https_variant("http://a.org/f").as_deref(), Some("https://a.org/f"));
        assert_eq!(https_variant("https://a.org/f"), None);
    }
}
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `export PATH="/opt/homebrew/bin:$PATH"; cargo test -p ressac-core url_key`
Expected: compile errors `cannot find function url_key` (and the other three).

- [ ] **Step 6: Implement.** Add above the `#[cfg(test)]` block of `url_key.rs`:

```rust
use url::Url;

/// Comparison key for a feed URL: scheme-less, lowercase host without `www.`,
/// explicit port kept, no trailing slash, query kept. Used only to compare
/// URLs, never to fetch them.
pub fn url_key(raw: &str) -> String {
    let repaired = repair_url(raw);
    match Url::parse(&repaired) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("").to_ascii_lowercase();
            let host = host.strip_prefix("www.").unwrap_or(&host);
            let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
            let path = u.path().trim_end_matches('/');
            let query = u.query().map(|q| format!("?{q}")).unwrap_or_default();
            format!("{host}{port}{path}{query}")
        }
        Err(_) => repaired.trim_end_matches('/').to_ascii_lowercase(),
    }
}

/// Repairs URLs damaged by double HTML escaping: an OPML `&amp;amp;` decodes
/// to a literal `&amp;`, which must become `&`.
pub fn repair_url(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    while s.contains("&amp;") {
        s = s.replace("&amp;", "&");
    }
    s
}

/// Approximate registrable domain: the last two host labels, or the last three
/// when the domain ends like `co.uk` / `com.au` (short label + 2-letter TLD).
pub fn site_of(raw: &str) -> Option<String> {
    let host = Url::parse(raw).ok()?.host_str()?.to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    if n < 2 {
        return Some(host);
    }
    let take = if n >= 3 && labels[n - 2].len() <= 3 && labels[n - 1].len() == 2 {
        3
    } else {
        2
    };
    Some(labels[n - take..].join("."))
}

/// The `https://` variant of an `http://` URL, `None` for anything else.
pub fn https_variant(raw: &str) -> Option<String> {
    raw.strip_prefix("http://").map(|rest| format!("https://{rest}"))
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p ressac-core url_key`
Expected: `7 passed`.

- [ ] **Step 8: Add CI** `.github/workflows/ci.yml`

```yaml
name: CI
on:
  push:
  pull_request:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 9: Verification gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three succeed. Run `cargo fmt --all` first if formatting fails.

- [ ] **Step 10: Commit and push**

```bash
git add Cargo.toml Cargo.lock crates/ressac-core .github/workflows/ci.yml
git commit -m "Workspace, ressac-core crate, URL keys and CI

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Phase gate:** report the results to the user and wait for approval before Phase 2.
