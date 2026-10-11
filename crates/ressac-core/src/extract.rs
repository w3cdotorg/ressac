//! Full-article extraction: turns a fetched web page into a readable article with
//! `dom_smoothie`, a Rust port of Mozilla Readability. Pure: fetching belongs to the
//! service.

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
    /// The page address is local or private (loopback, LAN, link-local...): never fetched.
    PrivateAddress,
    Fetch(ErrorKind),
}

impl ExtractError {
    /// One English sentence, stored in `entry_fulltext.error` and shown in the reader.
    pub fn sentence(&self) -> String {
        match self {
            ExtractError::NoContent => "No article text found.".to_string(),
            ExtractError::LoginWall => "The page is behind a login.".to_string(),
            ExtractError::Challenge => "Anti-bot challenge page.".to_string(),
            ExtractError::PrivateAddress => {
                "The page is on a local or private network.".to_string()
            }
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
