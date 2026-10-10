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
