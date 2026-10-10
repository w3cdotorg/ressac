//! Import → refresh → diagnose → Wayback against a local mock web.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use ressac_core::model::{Action, Status};
use ressac_core::service::{Event, EventSink, RefreshOptions, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl EventSink for Recorder {
    fn emit(&self, event: &Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn rss(title: &str, items: &[(&str, DateTime<Utc>)]) -> String {
    let items: String = items
        .iter()
        .map(|(id, d)| {
            format!(
                "<item><guid>{id}</guid><title>Entry {id}</title><link>http://example.org/{id}</link><pubDate>{}</pubDate></item>",
                d.to_rfc2822()
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://example.org/</link><description>A feed</description>{items}</channel></rss>"#
    )
}

async fn serve(server: &MockServer, at: &str, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(template)
        .mount(server)
        .await;
}

fn ok_rss(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml")
}

fn limits() -> FetchLimits {
    FetchLimits {
        per_host_gap: Duration::ZERO,
        timeout: Duration::from_secs(5),
        ..FetchLimits::default()
    }
}

const SPAM: &str = "Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày. Tham gia ngay để thử thách bản thân.";

#[tokio::test]
async fn full_pipeline_classifies_every_kind_of_feed() {
    let server = MockServer::start().await;
    let base = server.uri();
    let recent = Utc::now() - chrono::Duration::days(5);
    let old: DateTime<Utc> = "2015-03-01T10:00:00Z".parse().unwrap();

    serve(
        &server,
        "/alive.xml",
        ok_rss(rss(
            "Alive",
            &[
                ("a1", recent),
                ("a2", recent - chrono::Duration::days(7)),
                ("a3", recent - chrono::Duration::days(14)),
            ],
        )),
    )
    .await;
    serve(&server, "/gone.xml", ResponseTemplate::new(410)).await;
    serve(&server, "/old.xml", ok_rss(rss("Old", &[("o1", old)]))).await;
    serve(
        &server,
        "/moved.xml",
        ResponseTemplate::new(301).insert_header("Location", "/new.xml"),
    )
    .await;
    serve(
        &server,
        "/new.xml",
        ok_rss(rss("New home", &[("n1", recent)])),
    )
    .await;
    serve(&server, "/lost.xml", ResponseTemplate::new(404)).await;
    serve(&server, "/lost-site", ResponseTemplate::new(200).set_body_raw(r#"<html><head><link rel="alternate" type="application/rss+xml" href="/found.xml"></head></html>"#, "text/html")).await;
    serve(
        &server,
        "/found.xml",
        ok_rss(rss("Found", &[("f1", recent)])),
    )
    .await;
    serve(
        &server,
        "/spam.xml",
        ResponseTemplate::new(200).set_body_raw(
            "<!DOCTYPE html><html><body>casino</body></html>",
            "text/html",
        ),
    )
    .await;
    serve(
        &server,
        "/mirror.xml",
        ResponseTemplate::new(301).insert_header("Location", "/alive.xml"),
    )
    .await;
    serve(&server, "/wayback/available", ResponseTemplate::new(200).set_body_raw(r#"{"archived_snapshots":{"closest":{"available":true,"url":"http://web.archive.org/web/2019/x","timestamp":"20190101000000","status":"200"}}}"#, "application/json")).await;

    let opml = format!(
        r#"<opml version="1.0"><body><outline text="Test">
        <outline text="Alive" xmlUrl="{base}/alive.xml" htmlUrl="{base}/"/>
        <outline text="Gone" xmlUrl="{base}/gone.xml" htmlUrl="{base}/gone-site"/>
        <outline text="Old" xmlUrl="{base}/old.xml" htmlUrl="{base}/"/>
        <outline text="Moved" xmlUrl="{base}/moved.xml" htmlUrl="{base}/"/>
        <outline text="Lost" xmlUrl="{base}/lost.xml" htmlUrl="{base}/lost-site"/>
        <outline text="Spam" xmlUrl="{base}/spam.xml" htmlUrl="{base}/spam-site" description="{SPAM}"/>
        <outline text="Mirror" xmlUrl="{base}/mirror.xml" htmlUrl="{base}/"/>
        </outline></body></opml>"#
    );

    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
    assert_eq!(app.import_opml(&opml, false).unwrap().feeds_added, 7);
    let recorder = Recorder::default();
    let summary = app
        .refresh(
            RefreshOptions::default(),
            &recorder,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert!(!summary.network_unreachable);
    assert_eq!(summary.total, 7);

    let feeds = app.store().feeds().unwrap();
    let diags = app.store().diagnoses().unwrap();
    let by_title = |t: &str| {
        let f = feeds.iter().find(|f| f.title == t).unwrap();
        (f.id, diags[&f.id].clone())
    };
    let (alive_id, alive) = by_title("Alive");
    assert_eq!(alive.status, Status::Alive);
    assert_eq!(by_title("Gone").1.status, Status::Dead);
    assert_eq!(by_title("Old").1.status, Status::Fossil);
    let moved = by_title("Moved").1;
    assert_eq!(moved.status, Status::Moved);
    assert_eq!(
        moved.action,
        Some(Action::FollowRedirect {
            url: format!("{base}/new.xml")
        })
    );
    let lost = by_title("Lost").1;
    assert_eq!(lost.status, Status::Moved);
    assert_eq!(
        lost.action,
        Some(Action::AdoptDiscovered {
            url: format!("{base}/found.xml")
        })
    );
    assert_eq!(by_title("Spam").1.status, Status::Hijacked);
    let mirror = by_title("Mirror").1;
    assert_eq!(mirror.status, Status::Duplicate);
    assert_eq!(mirror.action, Some(Action::MergeInto { feed_id: alive_id }));

    let events = recorder.0.lock().unwrap().clone();
    assert!(matches!(events.first(), Some(Event::Started { total: 7 })));
    assert!(matches!(events.last(), Some(Event::Finished { .. })));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::FeedDone { .. }))
            .count(),
        7
    );

    let found = app
        .lookup_wayback(&recorder, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    assert_eq!(found, 2, "Dead + Hijacked");
    let gone = app
        .store()
        .feeds()
        .unwrap()
        .into_iter()
        .find(|f| f.title == "Gone")
        .unwrap();
    assert_eq!(
        gone.wayback_url.as_deref(),
        Some("http://web.archive.org/web/2019/x")
    );

    // A second run within 30 minutes fetches nothing and keeps the diagnoses.
    let again = app
        .refresh(
            RefreshOptions::default(),
            &recorder,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert_eq!(again.count(Status::Alive), 1);
}

#[tokio::test]
async fn unreachable_network_does_not_kill_feeds() {
    let opml = r#"<opml><body>
        <outline text="A" xmlUrl="http://a.ressac-test.invalid/feed"/>
        <outline text="B" xmlUrl="http://b.ressac-test.invalid/feed"/>
        <outline text="C" xmlUrl="http://c.ressac-test.invalid/feed"/>
        <outline text="D" xmlUrl="http://d.ressac-test.invalid/feed"/>
        </body></opml>"#;
    let app = Ressac::with_parts(
        Store::open_in_memory().unwrap(),
        limits(),
        "http://127.0.0.1:9",
    )
    .unwrap();
    app.import_opml(opml, false).unwrap();
    let recorder = Recorder::default();
    let summary = app
        .refresh(
            RefreshOptions::default(),
            &recorder,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    assert!(summary.network_unreachable);
    assert_eq!(summary.count(Status::Unchecked), 4);
    assert_eq!(summary.count(Status::Dead), 0);
    let events = recorder.0.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::NetworkUnreachable {
            failed: 4,
            attempted: 4
        }
    )));
}
