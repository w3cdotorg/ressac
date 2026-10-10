# Phase 1: Full-article extraction (`dom_smoothie` spike, pure `extract.rs`)

Read the plan index first. Global Constraints apply. This phase implements spec §6 (Full-article extraction: the extractor and its failure sentences; fetching comes in Phase 4) and settles the risk in spec §10 (Readability port quality).

**Files (5):**
- Modify: `Cargo.toml` (workspace pin `dom_smoothie = "0.18.2"`; `Cargo.lock` follows)
- Modify: `crates/ressac-core/Cargo.toml`
- Create: `crates/ressac-core/src/extract.rs` (unit tests and the spike inline)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod extract;`)
- Modify: `notes.md` (spike results, in French)

### Task 1.1: Spike: `dom_smoothie` on 10 real pages from the user's feeds

The spike decides whether we commit to `dom_smoothie`. It is an `#[ignore]`d test that fetches real pages through the core's own `LocalSource::fetch_raw` (same user agent, redirects and limits as Phase 4 will use) and prints what the extractor makes of them. It never runs in CI.

**Interfaces:**
- Consumes:
  - `source::{LocalSource, FetchLimits}`: `LocalSource::new(FetchLimits) -> Result<LocalSource>` and `async fn fetch_raw(&self, url: &str, etag: Option<&str>, last_modified: Option<&str>) -> RawResponse`, where `RawResponse { status: Option<u16>, error: Option<ErrorKind>, final_url: String, body: Vec<u8>, .. }`. On an error, `body` is empty.
  - `parse::{CHALLENGE_SNIFF_BYTES, is_challenge_page}` (both `pub(crate)`).
  - `dom_smoothie 0.18.2`: `Readability::new<T: Into<StrTendril>>(html: T, document_url: Option<&str>, cfg: Option<Config>) -> Result<Readability, ReadabilityError>`; `Readability::is_probably_readable(&self) -> bool` (call it before `parse`, which mutates the document); `Readability::parse(&mut self) -> Result<dom_smoothie::Article, ReadabilityError>`. The crate's `Article` has `title: String`, `byline: Option<String>`, `content: StrTendril` (HTML), `text_content: StrTendril`.
- Produces: `extract::spike::real_pages`, an ignored test, and a section in `notes.md`.

- [ ] **Step 1: Pin the crate.** In the root `Cargo.toml`, `[workspace.dependencies]`, insert after the `clap` line:

```toml
dom_smoothie = "0.18.2"
```

In `crates/ressac-core/Cargo.toml`, `[dependencies]`, insert after `chrono.workspace = true`:

```toml
dom_smoothie.workspace = true
```

- [ ] **Step 2: Create the module with the spike only.** In `crates/ressac-core/src/lib.rs`, add `pub mod extract;` after `pub mod error;`. Create `crates/ressac-core/src/extract.rs`:

```rust
//! Full-article extraction: turns a fetched web page into a readable article with
//! `dom_smoothie`, a Rust port of Mozilla Readability. Pure: fetching belongs to the
//! service.

/// Real-page check (plan phase 1 spike). Hits the network, so it only runs by hand:
/// `RESSAC_SPIKE_URLS=/path/to/urls.txt cargo test -p ressac-core extract::spike -- --ignored --nocapture`.
/// The file lists one URL per line; `#` starts a comment line. Keep it outside the repository.
#[cfg(test)]
mod spike {
    use std::time::Instant;

    use dom_smoothie::{Config, Readability};

    use crate::parse::{CHALLENGE_SNIFF_BYTES, is_challenge_page};
    use crate::source::{FetchLimits, LocalSource};

    #[tokio::test]
    #[ignore = "hits the network; run by hand with RESSAC_SPIKE_URLS"]
    async fn real_pages() {
        let list = std::env::var("RESSAC_SPIKE_URLS").expect("RESSAC_SPIKE_URLS names a URL list");
        let urls = std::fs::read_to_string(list).expect("readable URL list");
        let source = LocalSource::new(FetchLimits::default()).expect("HTTP client");
        for url in urls
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let raw = source.fetch_raw(url, None, None).await;
            let challenge =
                is_challenge_page(&raw.body[..raw.body.len().min(CHALLENGE_SNIFF_BYTES)]);
            println!(
                "## {url}\n   status={:?} error={:?} bytes={} challenge={challenge} final={}",
                raw.status,
                raw.error,
                raw.body.len(),
                raw.final_url
            );
            let html = String::from_utf8_lossy(&raw.body);
            let started = Instant::now();
            let parsed = Readability::new(
                html.as_ref(),
                Some(raw.final_url.as_str()),
                Some(Config::default()),
            )
            .and_then(|mut r| {
                let readable = r.is_probably_readable();
                r.parse().map(|a| (readable, a))
            });
            let ms = started.elapsed().as_millis();
            match parsed {
                Ok((readable, a)) => {
                    let text = a
                        .text_content
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    println!(
                        "   parsed in {ms} ms: readable={readable} title={:?} byline={:?} chars={} html_bytes={}",
                        a.title,
                        a.byline,
                        text.chars().count(),
                        a.content.len()
                    );
                    println!("   starts: {}", text.chars().take(160).collect::<String>());
                }
                Err(e) => println!("   dom_smoothie error in {ms} ms: {e}"),
            }
        }
    }
}
```

Run `cargo test -p ressac-core extract`. Expected: it compiles, `Cargo.lock` gains `dom_smoothie 0.18.2`, `dom_query 0.28.0`, `flagset 0.4.7`, `gjson 0.8.1`, `html-escape 0.2.15`, `nom 8.0.0`, `bit-set 0.8.0` and `bit-vec 0.8.0`, and the output shows `test extract::spike::real_pages ... ignored`.

- [ ] **Step 3: Write the URL list outside the repository** (never commit it: these are the user's subscriptions). Save it as `$TMPDIR/ressac-spike-urls.txt`. The 10 pages were picked on 2026-10-10 from the scratch database of the calibration run, `/private/tmp/claude-504/-Users-willow-Sites--Claude-output/3cb86ed3-73df-4143-b664-9a195543b572/scratchpad/calib-run/run2.sqlite` (real entries of the user's feeds; all ten feeds give only a summary or nothing in the feed itself):

```text
# FR news, DataDome in front
https://www.liberation.fr/societe/passage-a-lheure-dhiver-cette-annee-un-changement-bien-precoce-20261010_NRL2RQMGGRFL7DEEIAOCVVBMHE/
# FR news
https://www.lemonde.fr/politique/article/2026/10/10/primaire-a-gauche-plus-de-71-000-personnes-ont-deja-vote-selon-la-commission-d-organisation_6790987_823448.html
# FR monthly (SPIP)
https://www.monde-diplomatique.fr/carnet/2026-10-09-Espagne-la-greve-contre-la-rente
# FR blog (WordPress)
https://larepubliquedeslivres.com/splendeur-et-miseres-de-balzac-a-sarah-kane/
# FR blog
https://www.morganetual.com/pourquoi-imprimer-ses-photos-en-2026/
# EN news
https://www.wired.com/story/ai-is-getting-really-good-at-messing-with-cybercriminals/
# EN news
https://www.bbc.co.uk/sport/cycling/articles/cqx2vm911870o?at_medium=RSS&at_campaign=rss
# EN magazine
https://nautil.us/why-old-dogs-might-be-able-to-teach-alzheimers-researchers-new-tricks-1285678/
# EN blog (short quote post)
https://simonwillison.net/2026/Oct/10/the-new-york-times/
# EN Substack
https://centerforhumanetechnology.substack.com/p/ai-everywhere-how-cht-is-driving-acf
```

If a page has gone (404) by the time you run this, replace it with the newest entry of the same feed, read-only, for example:

```bash
sqlite3 -readonly /private/tmp/claude-504/-Users-willow-Sites--Claude-output/3cb86ed3-73df-4143-b664-9a195543b572/scratchpad/calib-run/run2.sqlite \
  "SELECT e.url FROM entry e JOIN feed f ON f.id = e.feed_id WHERE f.title = 'Wired News: Technology' ORDER BY e.published_at DESC LIMIT 1"
```

- [ ] **Step 4: Run the spike.**

```bash
RESSAC_SPIKE_URLS="$TMPDIR/ressac-spike-urls.txt" cargo test -p ressac-core extract::spike -- --ignored --nocapture
```

Expected, close to the plan writer's dry run of 2026-10-10 (debug build; timings vary):

| Page | Fetch | Extractor result |
|---|---|---|
| liberation.fr | `Http(403)`, empty body (DataDome) | `failed to grab the article` |
| lemonde.fr | 200, `challenge=true` ("Client Challenge") | parsed, `readable=false`, title "Client Challenge", 209 chars |
| monde-diplomatique.fr | 200 | "Espagne, la grève contre la rente", no byline, 2 076 chars, 19 ms |
| larepubliquedeslivres.com | 200 | title with " - La République des livres", byline "Passou", 6 910 chars |
| morganetual.com | 200 | "Pourquoi imprimer ses photos en 2026 ?", byline "OpenAI" (wrong, from the page metadata), 7 420 chars |
| wired.com | 200, 1.4 MB | full title, byline "Lily Hay Newman, Matt Burgess", 5 597 chars, 42 ms |
| bbc.co.uk | 200 | full title, byline "Mike Peter", 2 465 chars |
| nautil.us | 200 (redirect drops the trailing `/`) | full title, byline "Julie Moreno", 4 919 chars |
| simonwillison.net | 200 | "A quote from The New York Times", `readable=false`, 922 chars |
| substack.com | 200 | full title, byline "Center for Humane Technology", 2 109 chars |

- [ ] **Step 5: Decide.** Keep `dom_smoothie` when all three hold:
  1. at least 7 of the pages that are neither a fetch error nor a challenge give a non-empty title and at least 250 characters whose first sentence is the article's lead, not navigation or a cookie notice (the dry run: 8 of 8);
  2. no panic;
  3. every parse takes under 500 ms in the debug build.

  If any fails, stop and report to the controller with the output. That is a genuine decision (spec §10's fallback: keep the feed content and record "No article text found."), not something to work around.

- [ ] **Step 6: Record the results in `notes.md`.** Append this section at the end of the file, with your own run's figures in place of the dry run's (French, no em dash):

```markdown
## Extraction d'articles : essai de `dom_smoothie` 0.18.2 (phase 1 du sous-projet 2a)

Essai du JJ/MM/AAAA sur 10 pages réelles tirées des flux de l'utilisateur (base jetable du calibrage, liste d'URL gardée hors du dépôt), récupérées par `LocalSource::fetch_raw`. Commande : `RESSAC_SPIKE_URLS=… cargo test -p ressac-core extract::spike -- --ignored --nocapture`.

- **Verdict** : retenu. 8 articles sur 8 extraits (titre, texte de 922 à 7 420 caractères qui commence bien par le chapeau de l'article), aucune panique, 7 à 42 ms par page en debug (Wired : page de 1,4 Mo, 42 ms).
- **Pages refusées** : Le Monde sert une page « Client Challenge » en 200 ; `is_challenge_page` la reconnaît. Sans ce filtre, l'extracteur en tire 209 caractères avec le titre « Client Challenge » : d'où le contrôle des pages anti-robots *avant* l'extraction et le seuil `MIN_ARTICLE_CHARS` à 250. Libération répond 403 avec une page DataDome (`ct.captcha-delivery.com/c.js`) que nos marqueurs ne reconnaissent pas (ils attendent `captcha-delivery.com/captcha`) : `fetch_raw` rend `Http(403)`, l'extraction stockera « HTTP 403. ».
- **Auteur** : peut être faux. Morgane Tual donne « OpenAI » (métadonnées de la page). Affiché tel quel, jamais utilisé pour un calcul.
- **Titre** : parfois suivi du nom du site (« … - La République des livres »). Gardé tel quel.
- **Texte court** : le billet-citation de Simon Willison (922 caractères) est jugé `is_probably_readable() == false` mais bien extrait : on ne se sert pas de `is_probably_readable`.
- **API** : `Readability::new(html, Some(url), Some(Config::default()))` refuse une URL relative (`BadDocumentURL`) ; avec une URL absolue, liens et images relatifs deviennent absolus. `parse()` échoue en `GrabFailed` sur une page vide. `content` et `text_content` sont des `StrTendril`. `dom_smoothie` partage `html5ever` 0.39 avec `scraper` 0.27 : pas de second analyseur HTML dans l'arbre.
```

- [ ] **Step 7: Check and commit.**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/ressac-core/Cargo.toml crates/ressac-core/src/lib.rs crates/ressac-core/src/extract.rs notes.md
git commit -m "Spike: dom_smoothie on ten real article pages"
```

### Task 1.2: `extract_article`, pure

**Interfaces:**
- Consumes: `dom_smoothie::{Config, Readability}` (above); `scraper::{Html, Selector}`; `model::ErrorKind` with `label()` ("HTTP 404", "Timeout", "response too large"); `parse::{CHALLENGE_SNIFF_BYTES, is_challenge_page}`; `url::Url::parse`.
- Produces (`ressac_core::extract`):
  - `pub const MIN_ARTICLE_CHARS: usize = 250;`
  - `pub struct Article { pub html: String, pub title: Option<String>, pub byline: Option<String>, pub text: String }`, deriving `Debug, Clone, PartialEq, Eq`. `text` is plain text with whitespace collapsed (Phase 2 writes it into `entry_fts.body`).
  - `pub enum ExtractError { NoContent, LoginWall, Challenge, Fetch(ErrorKind) }`, deriving `Debug, Clone, PartialEq, Eq`, with `pub fn sentence(&self) -> String`: "No article text found.", "The page is behind a login.", "Anti-bot challenge page.", and for `Fetch(kind)` the label with its first letter capitalised plus a full stop ("HTTP 404.", "Timeout.", "Response too large.").
  - `pub fn extract_article(html: &str, url: &str) -> Result<Article, ExtractError>`. It never returns `Fetch`; that variant is for Phase 4. Rules, in order:
    1. the first `CHALLENGE_SNIFF_BYTES` of the page match `is_challenge_page` → `Challenge`;
    2. the extractor yields at least `MIN_ARTICLE_CHARS` characters of text → `Ok`;
    3. the page has an `<input type="password">` (any case) → `LoginWall`;
    4. otherwise `NoContent`.

- [ ] **Step 1: Write the failing tests.** In `extract.rs`, insert this module between the module doc comment and `mod spike`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://blog.example.org/2026/10/tides";

    /// Three paragraphs of real prose, wrapped in the chrome of a typical blog page.
    fn article_page(extra_body: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html lang="en"><head><title>On tides - Example Blog</title>
<meta name="author" content="Ada Lovelace"></head><body>
<nav><a href="/">Home</a> <a href="/about">About</a> <a href="/archive">Archive</a></nav>
{extra_body}
<article><h1>On tides</h1>
<p>The sea climbs the beach twice a day, and nobody in the village remembers a time when it did not. The fishermen read it like a clock, the children like a promise, and the old men on the bench by the harbour like a newspaper that never runs out of news.</p>
<p>Every spring the high tides reach the dunes. Then the wind turns, the water draws back further than anyone thought possible, and a road of wet sand opens towards the island. People walk it in boots, carrying bread and letters, and come back before the water does.</p>
<p>Some years the sea keeps a little of what it takes: a fence post, a boat, once a whole <a href="/2019/10/chapel">chapel</a>. The village rebuilds and the tide comes in again, as patient as ever. <img src="/img/harbour.jpg" alt="The harbour at dawn"></p>
</article>
<footer>Copyright 2026 Example Blog. All rights reserved.</footer>
</body></html>"#
        )
    }

    #[test]
    fn an_article_page_yields_title_byline_and_clean_text() {
        let a = extract_article(&article_page(""), URL).unwrap();
        assert_eq!(a.title.as_deref(), Some("On tides - Example Blog"));
        assert_eq!(a.byline.as_deref(), Some("Ada Lovelace"));
        assert!(
            a.text.starts_with("The sea climbs the beach twice a day"),
            "{}",
            a.text
        );
        assert!(a.text.contains("as patient as ever."));
        assert!(!a.text.contains("  "), "whitespace is collapsed");
        assert!(!a.html.contains("All rights reserved"), "footer dropped");
        assert!(!a.html.contains(">Archive<"), "navigation dropped");
    }

    #[test]
    fn relative_links_and_images_become_absolute() {
        let a = extract_article(&article_page(""), URL).unwrap();
        assert!(
            a.html.contains("https://blog.example.org/img/harbour.jpg"),
            "{}",
            a.html
        );
        assert!(a.html.contains("https://blog.example.org/2019/10/chapel"));
    }

    #[test]
    fn a_relative_document_url_still_extracts() {
        let a = extract_article(&article_page(""), "not a url").unwrap();
        assert!(a.text.contains("road of wet sand"));
    }

    #[test]
    fn a_sign_in_page_is_a_login_wall() {
        let page = r#"<!DOCTYPE html><html><head><title>Sign in</title></head><body>
<main><h1>Subscribers only</h1><p>Sign in to continue reading.</p>
<form action="/session" method="post"><input type="email" name="email">
<input type="PASSWORD" name="password"><button>Sign in</button></form></main></body></html>"#;
        assert_eq!(
            extract_article(page, "https://news.example.org/login?next=/a"),
            Err(ExtractError::LoginWall)
        );
    }

    #[test]
    fn a_sign_in_form_beside_a_real_article_is_not_a_wall() {
        let form = r#"<div class="modal"><form><input type="password" name="p"></form></div>"#;
        let a = extract_article(&article_page(form), URL).unwrap();
        assert!(a.text.contains("road of wet sand"));
    }

    #[test]
    fn challenge_pages_are_recognised_before_extraction() {
        for page in [
            "<!DOCTYPE html><html><head><title>Just a moment...</title></head><body><p>Checking your browser.</p></body></html>",
            "<!DOCTYPE html><html><head><title>Client Challenge</title></head><body><noscript>JavaScript is disabled in your browser.</noscript></body></html>",
        ] {
            assert_eq!(
                extract_article(page, URL),
                Err(ExtractError::Challenge),
                "{page}"
            );
        }
    }

    #[test]
    fn a_page_without_prose_has_no_content() {
        let stub = "<html><head><title>example.org</title></head><body><p>Please enable JS and disable any ad blocker</p></body></html>";
        assert_eq!(extract_article(stub, URL), Err(ExtractError::NoContent));
        assert_eq!(extract_article("", URL), Err(ExtractError::NoContent));
    }

    #[test]
    fn error_sentences_match_the_spec() {
        assert_eq!(ExtractError::NoContent.sentence(), "No article text found.");
        assert_eq!(
            ExtractError::LoginWall.sentence(),
            "The page is behind a login."
        );
        assert_eq!(
            ExtractError::Challenge.sentence(),
            "Anti-bot challenge page."
        );
        assert_eq!(
            ExtractError::Fetch(ErrorKind::Http(404)).sentence(),
            "HTTP 404."
        );
        assert_eq!(
            ExtractError::Fetch(ErrorKind::Timeout).sentence(),
            "Timeout."
        );
        assert_eq!(
            ExtractError::Fetch(ErrorKind::TooLarge).sentence(),
            "Response too large."
        );
    }
}
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core extract::tests`. Expected: compile errors, `cannot find function extract_article`, `cannot find type ExtractError`.

- [ ] **Step 3: Implement.** Insert between the module doc comment and `mod tests`:

```rust
use dom_smoothie::{Config, Readability};
use scraper::{Html, Selector};

use crate::model::ErrorKind;
use crate::parse::{CHALLENGE_SNIFF_BYTES, is_challenge_page};

/// Below this many characters of text, an extraction is not an article: "enable
/// JavaScript" stubs, cookie notices and sign-in prompts all fall under it.
pub const MIN_ARTICLE_CHARS: usize = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// The extractor's HTML, kept as is; the app sanitises it before rendering.
    pub html: String,
    pub title: Option<String>,
    pub byline: Option<String>,
    /// Plain text with whitespace collapsed, for the search index.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    NoContent,
    LoginWall,
    Challenge,
    Fetch(ErrorKind),
}

impl ExtractError {
    /// One English sentence, stored in `entry_fulltext.error` and shown in the reader.
    pub fn sentence(&self) -> String {
        match self {
            ExtractError::NoContent => "No article text found.".to_string(),
            ExtractError::LoginWall => "The page is behind a login.".to_string(),
            ExtractError::Challenge => "Anti-bot challenge page.".to_string(),
            ExtractError::Fetch(kind) => format!("{}.", capitalised(&kind.label())),
        }
    }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Extracts the article from `html`, the page found at `url` (the final address, after
/// redirects). Challenge pages are recognised before extraction; a sign-in form only
/// counts as a login wall when no article came out.
pub fn extract_article(html: &str, url: &str) -> Result<Article, ExtractError> {
    let head = &html.as_bytes()[..html.len().min(CHALLENGE_SNIFF_BYTES)];
    if is_challenge_page(head) {
        return Err(ExtractError::Challenge);
    }
    match readable(html, url) {
        Some(article) if article.text.chars().count() >= MIN_ARTICLE_CHARS => Ok(article),
        _ if has_password_field(html) => Err(ExtractError::LoginWall),
        _ => Err(ExtractError::NoContent),
    }
}

fn readable(html: &str, url: &str) -> Option<Article> {
    // dom_smoothie refuses a relative document URL; without one, links stay as written.
    let doc_url = url::Url::parse(url).is_ok().then_some(url);
    let mut readability = Readability::new(html, doc_url, Some(Config::default())).ok()?;
    let article = readability.parse().ok()?;
    Some(Article {
        html: article.content.to_string(),
        title: non_empty(&article.title),
        byline: article.byline.as_deref().and_then(non_empty),
        text: article
            .text_content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    })
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn has_password_field(html: &str) -> bool {
    let inputs = Selector::parse("input").expect("static selector");
    Html::parse_document(html).select(&inputs).any(|input| {
        input
            .value()
            .attr("type")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("password"))
    })
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core extract` → `8 passed; 0 failed; 1 ignored`.

- [ ] **Step 5: Point the spike at `extract_article`**, so it stays useful as the by-hand check on real pages (Phase 13's manual run can reuse it). Replace the whole `mod spike` with:

```rust
/// Real-page check (plan phase 1 spike). Hits the network, so it only runs by hand:
/// `RESSAC_SPIKE_URLS=/path/to/urls.txt cargo test -p ressac-core extract::spike -- --ignored --nocapture`.
/// The file lists one URL per line; `#` starts a comment line. Keep it outside the repository.
#[cfg(test)]
mod spike {
    use std::time::Instant;

    use super::extract_article;
    use crate::source::{FetchLimits, LocalSource};

    #[tokio::test]
    #[ignore = "hits the network; run by hand with RESSAC_SPIKE_URLS"]
    async fn real_pages() {
        let list = std::env::var("RESSAC_SPIKE_URLS").expect("RESSAC_SPIKE_URLS names a URL list");
        let urls = std::fs::read_to_string(list).expect("readable URL list");
        let source = LocalSource::new(FetchLimits::default()).expect("HTTP client");
        for url in urls
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let raw = source.fetch_raw(url, None, None).await;
            println!(
                "## {url}\n   status={:?} error={:?} bytes={} final={}",
                raw.status,
                raw.error,
                raw.body.len(),
                raw.final_url
            );
            let html = String::from_utf8_lossy(&raw.body);
            let started = Instant::now();
            let outcome = extract_article(&html, &raw.final_url);
            let ms = started.elapsed().as_millis();
            match outcome {
                Ok(a) => {
                    println!(
                        "   ok in {ms} ms: title={:?} byline={:?} chars={} html_bytes={}",
                        a.title,
                        a.byline,
                        a.text.chars().count(),
                        a.html.len()
                    );
                    println!(
                        "   starts: {}",
                        a.text.chars().take(160).collect::<String>()
                    );
                }
                Err(e) => println!("   failed in {ms} ms: {}", e.sentence()),
            }
        }
    }
}
```

Run it once more with the same URL list. Expected: the same eight articles `ok`, Le Monde `failed …: Anti-bot challenge page.`, Libération `failed …: No article text found.` (its 403 body is empty here; Phase 4 stores the fetch error instead).

- [ ] **Step 6: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/extract.rs
git commit -m "Extract articles with dom_smoothie: challenge, login wall and no-content outcomes"
```

### Verification gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green; the spike shows as `ignored`.

### Notes for the executor

- **Verified on 2026-10-10** in a scratch copy of the repository: both tasks compile, the gate passes, and the spike gives the table above.
- `Readability::new` errors only with `BadDocumentURL` (relative URL); `parse` errors with `GrabFailed` (empty page) or `TooManyElements` (only when `Config::max_elements_to_parse > 0`; the default `0` means unlimited). Both become `NoContent` here.
- `is_probably_readable` is deliberately unused: it says `false` for Simon Willison's short but genuine post. `MIN_ARTICLE_CHARS` is the only length rule.
- A sign-in form only counts when the extraction failed: many article pages (the New Yorker, Substack) ship a hidden sign-in modal in their HTML.
- Login walls are recognised by page content only. Recognising them by address (`health::looks_like_a_wall`, private to `health/mod.rs`) would need a sixth file; a redirect to a login page that serves a password form is caught anyway.
- `ExtractError::Fetch` capitalises the label's first letter, so `TooLarge` reads "Response too large." rather than "response too large.". For `Http` and `Timeout` the result equals the plan index's `format!("{}.", kind.label())`.
- Never commit the URL list, and do not hard-code those URLs anywhere in the repository.
