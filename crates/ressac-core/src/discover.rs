//! Feed autodiscovery in HTML pages, and "same feed" comparison for the https probe.

use scraper::{Html, Selector};
use url::Url;

use crate::parse::ParsedFeed;

const FEED_TYPES: &[&str] = &[
    "application/rss+xml",
    "application/atom+xml",
    "application/feed+json",
];

pub fn alternate_feeds(html: &str, base: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse(r#"link[rel~="alternate"]"#).expect("static selector is valid");
    let base = Url::parse(base).ok();
    doc.select(&selector)
        .filter(|link| {
            link.value().attr("type").is_some_and(|t| {
                let mime = t
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                FEED_TYPES.contains(&mime.as_str())
            })
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
    a.title.is_some()
        && a.title == b.title
        && a.entries.iter().any(|x| {
            b.entries
                .iter()
                .any(|y| x.guid == y.guid || (x.url.is_some() && x.url == y.url))
        })
}

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
        assert!(same_feed(
            &feed("Blog", &["1", "2"]),
            &feed("Blog", &["2", "3"])
        ));
        assert!(!same_feed(&feed("Blog", &["1"]), &feed("Other", &["1"])));
        assert!(!same_feed(&feed("Blog", &["1"]), &feed("Blog", &["9"])));
    }

    #[test]
    fn untitled_feeds_are_never_the_same() {
        let mut a = feed("x", &["1"]);
        let mut b = feed("x", &["1"]);
        a.title = None;
        b.title = None;
        assert!(!same_feed(&a, &b));
    }

    #[test]
    fn mime_parameters_are_ignored() {
        let html = r#"<link rel="alternate" type="Application/RSS+XML; charset=utf-8" href="/f">"#;
        assert_eq!(
            alternate_feeds(html, "https://x.org/"),
            vec!["https://x.org/f".to_string()]
        );
    }
}
