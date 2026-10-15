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
