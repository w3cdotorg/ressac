//! `Ressac::build_edition` against a local mock web: refresh, then compose and store the edition.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use ressac_core::edition::{Caps, Section};
use ressac_core::service::{EditionOptions, EditionOutcome, Event, EventSink, Ressac};
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

/// Raises the stop flag as soon as the refresh starts, as Ctrl-C or quitting the app would.
struct StopOnStart {
    stop: Arc<AtomicBool>,
}

impl EventSink for StopOnStart {
    fn emit(&self, event: &Event) {
        if matches!(event, Event::Started { .. }) {
            self.stop.store(true, Ordering::Relaxed);
        }
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

fn go() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn hours(h: i64) -> chrono::Duration {
    chrono::Duration::hours(h)
}

/// Three feeds: Daily (entries a day apart), Weekly (six days apart), and a Silent feed that
/// wakes up (one entry now, the previous one 800 days ago), plus a gone feed when `with_gone`.
async fn morning_app(server: &MockServer, with_gone: bool) -> Ressac {
    let base = server.uri();
    let now = Utc::now();
    serve(
        server,
        "/daily.xml",
        ok_rss(rss(
            "Daily",
            &[
                ("d1", now - hours(1)),
                ("d2", now - hours(25)),
                ("d3", now - hours(49)),
            ],
        )),
    )
    .await;
    serve(
        server,
        "/weekly.xml",
        ok_rss(rss(
            "Weekly",
            &[
                ("w1", now - hours(2)),
                ("w2", now - hours(6 * 24)),
                ("w3", now - hours(12 * 24)),
            ],
        )),
    )
    .await;
    serve(
        server,
        "/wakes.xml",
        ok_rss(rss(
            "Wakes",
            &[("s1", now - hours(3)), ("s0", now - hours(800 * 24))],
        )),
    )
    .await;
    serve(server, "/gone.xml", ResponseTemplate::new(410)).await;
    let gone = if with_gone {
        format!(r#"<outline text="Gone" xmlUrl="{base}/gone.xml"/>"#)
    } else {
        String::new()
    };
    let opml = format!(
        r#"<opml><body><outline text="Infos">
        <outline text="Daily" xmlUrl="{base}/daily.xml"/>
        <outline text="Weekly" xmlUrl="{base}/weekly.xml"/>
        <outline text="Wakes" xmlUrl="{base}/wakes.xml"/>{gone}
        </outline></body></opml>"#
    );
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
    app.import_opml(&opml, false).unwrap();
    app
}

fn built(outcome: EditionOutcome) -> ressac_core::service::EditionReport {
    match outcome {
        EditionOutcome::Built(r) => r,
        other => panic!("expected an edition, got {other:?}"),
    }
}

#[tokio::test]
async fn a_morning_build_composes_sections_and_ends_with_the_edition_event() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    let recorder = Recorder::default();
    let report = built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &recorder, go())
            .await
            .unwrap(),
    );
    // Wakes → Rare → front page (1); Weekly → w1 and w2 (w3 arrived read: older than 7 days on
    // first fill); Daily → d1 only, d2 and d3 overflow.
    assert_eq!((report.front, report.week, report.wire), (1, 2, 1));
    assert_eq!(report.entries, 4);
    assert_eq!(report.overflow, 2);
    assert_eq!(report.unreachable, 0);
    let events = recorder.0.lock().unwrap();
    assert!(matches!(events.first(), Some(Event::Started { .. })));
    assert_eq!(events.last(), Some(&report.event()));

    let view = app.edition_today().unwrap().unwrap();
    assert_eq!(view.edition.id, report.id);
    assert_eq!(view.entries[0].section, Section::Front);
    assert_eq!(view.entries[0].feed_title, "Wakes");
    assert_eq!(
        view.entries.iter().map(|e| e.rank).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(view.overflow.len(), 1);
    assert_eq!(view.overflow[0].hidden_count, 2);
}

#[tokio::test]
async fn a_second_build_the_same_morning_does_not_even_refresh() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    let recorder = Recorder::default();
    let again = app
        .build_edition(EditionOptions::default(), Utc::now(), &Utc, &recorder, go())
        .await
        .unwrap();
    assert_eq!(again, EditionOutcome::NotDue);
    assert!(
        recorder.0.lock().unwrap().is_empty(),
        "no refresh when nothing is due"
    );
}

#[tokio::test]
async fn the_next_morning_starts_where_the_previous_edition_ended() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    let first = app.store().latest_edition().unwrap().unwrap();
    let tomorrow = Utc::now() + chrono::Duration::days(1);
    let second = built(
        app.build_edition(
            EditionOptions::default(),
            tomorrow,
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    let record = app.store().latest_edition().unwrap().unwrap();
    assert_eq!(record.window_start, first.window_end);
    // Stored with millisecond precision; `tomorrow` carries nanoseconds.
    assert_eq!(
        record.window_end.timestamp_millis(),
        tomorrow.timestamp_millis()
    );
    // The feeds were fetched minutes ago: nothing new, an empty edition, the window still moves.
    assert_eq!(second.entries, 0);
    assert!(app.edition_today().unwrap().unwrap().entries.is_empty());
}

#[tokio::test]
async fn a_failed_feed_is_counted_and_the_edition_is_built_anyway() {
    let server = MockServer::start().await;
    let app = morning_app(&server, true).await;
    let report = built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(report.unreachable, 1);
    assert_eq!(app.edition_today().unwrap().unwrap().edition.unreachable, 1);
}

#[tokio::test]
async fn an_unreachable_network_builds_nothing() {
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
    let outcome = app
        .build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap();
    assert_eq!(outcome, EditionOutcome::Offline);
    assert_eq!(app.store().latest_edition().unwrap(), None);
}

/// Review Focus 4: a build stopped during its refresh writes nothing, and the next build gets
/// the whole window.
#[tokio::test]
async fn a_stopped_build_writes_nothing_and_the_next_one_builds() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    let stop = go();
    let outcome = app
        .build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &StopOnStart { stop: stop.clone() },
            stop,
        )
        .await
        .unwrap();
    assert_eq!(outcome, EditionOutcome::Interrupted);
    assert_eq!(app.store().latest_edition().unwrap(), None);

    built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    let record = app.store().latest_edition().unwrap().unwrap();
    assert_eq!(
        record.window_start,
        record.window_end - chrono::Duration::hours(24)
    );
}

#[tokio::test]
async fn a_rebuild_tries_other_caps_on_the_same_window() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    let first = app.store().latest_edition().unwrap().unwrap();
    let opts = EditionOptions {
        caps: Caps { week: 3, wire: 0 },
        rebuild: true,
    };
    let report = built(
        app.build_edition(opts, Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    assert_eq!(report.wire, 0);
    assert_eq!(report.overflow, 3, "every daily entry left in its feed");
    let second = app.store().latest_edition().unwrap().unwrap();
    // No AUTOINCREMENT on `edition`: the freed rowid may be reused, so compare built_at.
    assert_ne!(second.built_at, first.built_at);
    assert_eq!(second.window_start, first.window_start);
}

#[tokio::test]
async fn no_edition_yet() {
    let app = Ressac::with_parts(
        Store::open_in_memory().unwrap(),
        limits(),
        "http://127.0.0.1:9",
    )
    .unwrap();
    assert_eq!(app.edition_today().unwrap(), None);
}

/// Spec §9: offline means no edition and an unchanged window, and a Retry within 30 minutes
/// (the feeds were just tried) must see the network failure again instead of finding nothing
/// due and writing an empty edition.
#[tokio::test]
async fn a_retry_right_after_an_offline_build_is_offline_again_then_builds() {
    let addr = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let base = format!("http://{addr}");
    let outlines: String = (1..=4)
        .map(|i| format!(r#"<outline text="F{i}" xmlUrl="{base}/f{i}.xml"/>"#))
        .collect();
    let app = Ressac::with_parts(
        Store::open_in_memory().unwrap(),
        limits(),
        "http://127.0.0.1:9",
    )
    .unwrap();
    app.import_opml(&format!("<opml><body>{outlines}</body></opml>"), false)
        .unwrap();
    for _ in 0..2 {
        let outcome = app
            .build_edition(
                EditionOptions::default(),
                Utc::now(),
                &Utc,
                &Recorder::default(),
                go(),
            )
            .await
            .unwrap();
        assert_eq!(outcome, EditionOutcome::Offline);
        assert_eq!(app.store().latest_edition().unwrap(), None);
    }

    // The network comes back: the same port now answers.
    let server = MockServer::builder()
        .listener(std::net::TcpListener::bind(addr).unwrap())
        .start()
        .await;
    let now = Utc::now();
    for i in 1..=4 {
        serve(
            &server,
            &format!("/f{i}.xml"),
            ok_rss(rss(&format!("F{i}"), &[(&format!("g{i}"), now - hours(1))])),
        )
        .await;
    }
    let report = built(
        app.build_edition(
            EditionOptions::default(),
            Utc::now(),
            &Utc,
            &Recorder::default(),
            go(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(report.entries, 4);
    assert_eq!(report.unreachable, 0);
}
