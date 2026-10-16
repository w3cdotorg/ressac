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
    let bytes = app(true)
        .fetch_image(&format!("{}/a.png", server.uri()))
        .await
        .unwrap();
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
    serve(
        &server,
        "/new.png",
        200,
        PNG.to_vec(),
        "IMAGE/PNG; charset=binary",
    )
    .await;
    let bytes = app(true)
        .fetch_image(&format!("{}/old.png", server.uri()))
        .await
        .unwrap();
    assert_eq!(bytes, PNG, "content types compare case-insensitively");
}

#[tokio::test]
async fn a_page_instead_of_an_image_is_refused() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/a.png",
        200,
        b"<html>Sign in</html>".to_vec(),
        "text/html",
    )
    .await;
    let why = refused(
        app(true)
            .fetch_image(&format!("{}/a.png", server.uri()))
            .await,
    );
    assert_eq!(why, "not an image");
}

#[tokio::test]
async fn an_image_over_10_mb_is_refused() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/big.jpg",
        200,
        vec![0u8; IMAGE_MAX_BYTES + 1],
        "image/jpeg",
    )
    .await;
    let why = refused(
        app(true)
            .fetch_image(&format!("{}/big.jpg", server.uri()))
            .await,
    );
    assert_eq!(why, "larger than 10 MB");
}

#[tokio::test]
async fn an_http_error_is_named() {
    let server = MockServer::start().await;
    serve(&server, "/gone.png", 404, Vec::new(), "text/plain").await;
    let why = refused(
        app(true)
            .fetch_image(&format!("{}/gone.png", server.uri()))
            .await,
    );
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
        assert_eq!(
            refused(guarded.fetch_image(&url).await),
            "local or private address",
            "{url}"
        );
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached the server"
    );
}

#[tokio::test]
async fn only_web_addresses_are_fetched() {
    let guarded = app(false);
    for url in [
        "file:///etc/passwd",
        "data:image/png;base64,AAAA",
        "not a url",
        "ftp://example.org/a.png",
    ] {
        assert_eq!(
            refused(guarded.fetch_image(url).await),
            "not a web address",
            "{url}"
        );
    }
}
