//! URL helpers: comparison keys and repairs of malformed OPML URLs.

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
    raw.strip_prefix("http://")
        .map(|rest| format!("https://{rest}"))
}

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
        assert_eq!(
            url_key("http://127.0.0.1:8080/a.xml"),
            "127.0.0.1:8080/a.xml"
        );
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
        assert_eq!(
            site_of("https://feeds.bbci.co.uk/news/rss.xml").as_deref(),
            Some("bbci.co.uk")
        );
        assert_eq!(
            site_of("https://www.lemonde.fr/rss/une.xml").as_deref(),
            Some("lemonde.fr")
        );
        assert_eq!(site_of("not a url"), None);
    }

    #[test]
    fn https_variant_only_for_http() {
        assert_eq!(
            https_variant("http://a.org/f").as_deref(),
            Some("https://a.org/f")
        );
        assert_eq!(https_variant("https://a.org/f"), None);
    }
}
