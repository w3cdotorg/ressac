//! Runs the real binary.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

use ressac_core::model::{Action, Cadence, Diagnosis, Evidence, FeedId, Status};
use ressac_core::store::Store;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/vienna-like.opml"
);

fn ressac(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ressac"))
        .args(["--db", home.join("db.sqlite").to_str().unwrap()])
        .args(args)
        .env("HOME", home)
        .env("RESSAC_ALLOW_PRIVATE_ARTICLES", "1")
        .output()
        .expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn json_lines(o: &Output) -> Vec<Value> {
    stdout(o)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Runs the binary off the async runtime, so a wiremock server on it keeps answering.
async fn ressac_async(home: &Path, args: &[&str]) -> Output {
    let home = home.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        ressac(&home, &args)
    })
    .await
    .unwrap()
}

const PARA: &str = "The tide comes in twice a day, pulled by the moon and the sun, and the shore keeps a careful record of every visit in sand, shells and drifting weed.";

fn article_page() -> String {
    let body: String = (0..8)
        .map(|i| format!("<p>Paragraph {i}. {PARA} {PARA}</p>"))
        .collect();
    format!(
        "<!DOCTYPE html><html><head><title>Tides</title></head><body><article><h1>Tides</h1>{body}</article></body></html>"
    )
}

fn channel(title: &str, items: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://e.org/</link><description>d</description>{items}</channel></rss>"#
    )
}

/// Serves four feeds and two pages, and imports them as News { A, B, Sub { C } } then D at
/// the top level: feed ids 1–4 in that order, folder ids News = 1 and Sub = 2 (ids follow
/// the OPML's depth-first order on a fresh database). Feed A holds, in order: an entry
/// linking to /article, one linking to /missing (404), and one with no link and no title.
async fn reading_home(server: &MockServer) -> tempfile::TempDir {
    let base = server.uri();
    let a_items = format!(
        "<item><guid>linked</guid><title>Tides</title><link>{base}/article</link></item>\
         <item><guid>broken</guid><title>Gone</title><link>{base}/missing</link></item>\
         <item><guid>bare</guid><description>Only a summary.</description></item>"
    );
    for (at, body) in [
        ("/a.xml", channel("A", &a_items)),
        ("/b.xml", channel("B", "")),
        ("/c.xml", channel("C", "")),
        ("/d.xml", channel("D", "")),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml"))
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(article_page(), "text/html"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("reading.opml");
    std::fs::write(
        &opml,
        format!(
            r#"<opml><body><outline text="News"><outline text="A" xmlUrl="{base}/a.xml"/><outline text="B" xmlUrl="{base}/b.xml"/><outline text="Sub"><outline text="C" xmlUrl="{base}/c.xml"/></outline></outline><outline text="D" xmlUrl="{base}/d.xml"/></body></opml>"#
        ),
    )
    .unwrap();
    let import = ressac_async(home.path(), &["import", opml.to_str().unwrap()]).await;
    assert!(import.status.success(), "{import:?}");
    home
}

#[test]
fn dry_run_import_reports_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let out = ressac(home.path(), &["import", FIXTURE, "--dry-run"]);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("Imported 11 feeds in 7 new folders."));
    assert!(
        !home.path().join("db.sqlite").exists(),
        "dry run on a fresh home writes no db"
    );
    let export = ressac(home.path(), &["export"]);
    assert!(!stdout(&export).contains("xkcd"));
}

#[test]
fn import_then_export_round_trips() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let out_file = home.path().join("clean.opml");
    let export = ressac(home.path(), &["export", "-o", out_file.to_str().unwrap()]);
    assert!(export.status.success(), "{export:?}");
    let xml = std::fs::read_to_string(out_file).unwrap();
    assert!(xml.contains("<opml version=\"2.0\">"));
    assert!(xml.contains("xkcd.com"));
}

#[test]
fn json_import_is_machine_readable() {
    let home = tempfile::tempdir().unwrap();
    let out = ressac(home.path(), &["--json", "import", FIXTURE]);
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["feeds_added"], 11);
}

#[test]
fn fatal_errors_exit_1_with_a_message() {
    let home = tempfile::tempdir().unwrap();
    let missing = ressac(home.path(), &["import", "/nonexistent.opml"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing.stderr)
            .starts_with("error: cannot read /nonexistent.opml")
    );

    let bad = home.path().join("bad.opml");
    std::fs::write(&bad, "<opml>\n<body>\n<outline text=\"x\">\n</body></opml>").unwrap();
    let malformed = ressac(home.path(), &["import", bad.to_str().unwrap()]);
    assert_eq!(malformed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("cannot read OPML at line"));
    assert!(
        !home.path().join("db.sqlite").exists(),
        "malformed OPML creates no db"
    );
}

#[test]
fn closed_stdout_does_not_panic() {
    use std::io::Read;
    use std::process::Stdio;
    let home = tempfile::tempdir().unwrap();
    // Several hundred KB of OPML, far above the 64 KB pipe buffer, so the write must hit EPIPE.
    let mut xml = String::from("<opml version=\"2.0\"><body>");
    for i in 0..3000 {
        xml.push_str(&format!(
            "<outline text=\"Feed number {i} with a reasonably long title to pad the export\" xmlUrl=\"https://example{i}.org/feed.xml\" htmlUrl=\"https://example{i}.org/\"/>"
        ));
    }
    xml.push_str("</body></opml>");
    let big = home.path().join("big.opml");
    std::fs::write(&big, xml).unwrap();
    assert!(
        ressac(home.path(), &["import", big.to_str().unwrap()])
            .status
            .success()
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_ressac"))
        .args([
            "--db",
            home.path().join("db.sqlite").to_str().unwrap(),
            "export",
        ])
        .env("HOME", home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let code = child.wait().unwrap().code();
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn commands_on_an_empty_store_hint_at_import() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        &["fetch"][..],
        &["check"],
        &["garden"],
        &["export"],
        &["export", "--cemetery"],
        &["--json", "fetch"],
        &["fetch", "--feed", "1"],
        &["extract", "1"],
        &["garden", "decide", "1", "keep"],
        &["garden", "undo"],
        &["garden", "apply-safe"],
        &["page", "list"],
        &["page", "set", "feed", "1", "300"],
        &["edition", "build"],
        &["--json", "edition", "build"],
        &["edition", "show"],
    ] {
        let out = ressac(home.path(), args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "No feeds yet: run \"ressac import <file.opml>\" first.\n",
            "{args:?}"
        );
        assert!(out.stdout.is_empty(), "{args:?}: nothing else runs");
    }
}

#[test]
fn usage_errors_exit_2() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        &["frobnicate"][..],
        &["garden", "frobnicate"],
        &["garden", "decide", "1", "frobnicate"],
        &["extract", "x"],
        &["fetch", "--feed", "1", "--folder", "1"],
        &["page"],
        &["page", "set", "entry", "1", "300"],
        &["page", "set", "feed", "1", "70000"],
        &["edition"],
        &["edition", "frobnicate"],
        &["edition", "build", "--cap-wire", "x"],
        &["edition", "build", "--cap-week", "-1"],
    ] {
        assert_eq!(ressac(home.path(), args).status.code(), Some(2), "{args:?}");
    }
}

#[tokio::test]
async fn fetch_json_streams_events() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><link>http://e.org/</link><description>d</description></channel></rss>"#,
            "application/rss+xml",
        ))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("one.opml");
    std::fs::write(
        &opml,
        format!(
            r#"<opml><body><outline text="T" xmlUrl="{}/feed"/></body></opml>"#,
            server.uri()
        ),
    )
    .unwrap();
    let home_path = home.path().to_path_buf();
    let out = tokio::task::spawn_blocking(move || {
        assert!(
            ressac(&home_path, &["import", opml.to_str().unwrap()])
                .status
                .success()
        );
        ressac(&home_path, &["--json", "fetch"])
    })
    .await
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    let lines: Vec<serde_json::Value> = stdout(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.first().unwrap()["event"], "started");
    assert_eq!(lines.last().unwrap()["event"], "finished");
    assert!(lines.iter().any(|l| l["event"] == "feed_done"));
}

#[tokio::test]
async fn fetch_subsets_imply_force_and_include_subfolders() {
    let server = MockServer::start().await;
    let home = reading_home(&server).await;
    let started = |o: &Output| json_lines(o)[0].clone();

    let two = ressac_async(
        home.path(),
        &["--json", "fetch", "--feed", "1", "--feed", "4"],
    )
    .await;
    assert!(two.status.success(), "{two:?}");
    assert_eq!(started(&two), json!({"event": "started", "total": 2}));
    let again = ressac_async(home.path(), &["--json", "fetch", "--feed", "1"]).await;
    assert_eq!(
        started(&again),
        json!({"event": "started", "total": 1}),
        "fetched seconds ago, fetched again: --feed implies --force"
    );
    let news = ressac_async(home.path(), &["--json", "fetch", "--folder", "1"]).await;
    assert_eq!(
        started(&news),
        json!({"event": "started", "total": 3}),
        "News holds A, B and Sub/C"
    );
    let sub = ressac_async(home.path(), &["--json", "fetch", "--folder", "2"]).await;
    assert_eq!(started(&sub), json!({"event": "started", "total": 1}));
    assert_eq!(json_lines(&sub).last().unwrap()["event"], "finished");
}

#[test]
fn fetch_subsets_reject_unknown_ids_and_skip_buried_feeds() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let feed = ressac(home.path(), &["fetch", "--feed", "99"]);
    assert_eq!(feed.status.code(), Some(2));
    assert_eq!(stderr(&feed), "No feed #99.\n");
    let folder = ressac(home.path(), &["fetch", "--folder", "99"]);
    assert_eq!(folder.status.code(), Some(2));
    assert_eq!(stderr(&folder), "No folder #99.\n");

    // Nothing touches the network: the only named feed is buried.
    assert!(
        ressac(home.path(), &["garden", "decide", "1", "bury"])
            .status
            .success()
    );
    let buried = ressac(home.path(), &["--json", "fetch", "--feed", "1"]);
    assert!(buried.status.success(), "{buried:?}");
    let lines = json_lines(&buried);
    assert_eq!(
        lines[0],
        json!({"event": "started", "total": 0}),
        "a buried feed is skipped even when named"
    );
    assert_eq!(lines.last().unwrap()["event"], "finished");
}

#[tokio::test]
async fn extract_stores_articles_and_refuses_unusable_entries() {
    let server = MockServer::start().await;
    let home = reading_home(&server).await;
    let fetched = ressac_async(home.path(), &["fetch", "--feed", "1"]).await;
    assert!(fetched.status.success(), "{fetched:?}");

    let saved = ressac_async(home.path(), &["--json", "extract", "1"]).await;
    assert!(saved.status.success(), "{saved:?}");
    assert_eq!(
        json_lines(&saved),
        [json!({"event": "extracted", "entry_id": 1, "ok": true, "error": null})]
    );
    let failed = ressac_async(home.path(), &["--json", "extract", "2"]).await;
    assert_eq!(
        failed.status.code(),
        Some(0),
        "a failed extraction is data, not a fatal error"
    );
    assert_eq!(
        json_lines(&failed),
        [json!({"event": "extracted", "entry_id": 2, "ok": false, "error": "HTTP 404."})]
    );
    assert_eq!(
        stdout(&ressac_async(home.path(), &["extract", "1"]).await),
        "Saved the full article of entry #1.\n"
    );
    assert_eq!(
        stdout(&ressac_async(home.path(), &["extract", "2"]).await),
        "Full article unavailable: HTTP 404.\n"
    );

    // Review Focus 1: no link, no title, only a summary.
    let no_link = ressac_async(home.path(), &["--json", "extract", "3"]).await;
    assert_eq!(no_link.status.code(), Some(2));
    assert_eq!(stderr(&no_link), "This entry has no link.\n");
    assert!(no_link.stdout.is_empty());
    let unknown = ressac_async(home.path(), &["extract", "999"]).await;
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(stderr(&unknown), "No entry #999.\n");
}

#[test]
fn garden_subcommands_decide_undo_and_apply_safe() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);

    // Feed #2 is "Orange Drenka" (https://orange.example.org/feed/).
    let buried = run(&["--json", "garden", "decide", "2", "bury"]);
    assert!(buried.status.success(), "{buried:?}");
    assert_eq!(
        json_lines(&buried),
        [json!({"event": "decided", "feed_id": 2, "decision": "bury"})]
    );
    assert!(!stdout(&run(&["export"])).contains("orange.example.org/feed/"));
    for decision in ["bury", "repair"] {
        let refused = run(&["garden", "decide", "2", decision]);
        assert_eq!(refused.status.code(), Some(2), "{decision}");
        assert_eq!(
            stderr(&refused),
            "Feed #2 is buried; only keep restores it.\n"
        );
        assert!(refused.stdout.is_empty());
    }
    let restored = run(&["--json", "garden", "decide", "2", "keep"]);
    assert_eq!(
        json_lines(&restored),
        [json!({"event": "decided", "feed_id": 2, "decision": "keep"})]
    );
    assert!(stdout(&run(&["export"])).contains("orange.example.org/feed/"));

    let nothing = run(&["garden", "decide", "1", "repair"]);
    assert_eq!(nothing.status.code(), Some(2));
    assert_eq!(stderr(&nothing), "Nothing to repair for feed #1.\n");
    let unknown = run(&["garden", "decide", "99", "keep"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(stderr(&unknown), "No feed #99.\n");

    // One shared stack: the keep, then the bury, then nothing left.
    for expected in [json!(2), json!(2), Value::Null] {
        let undo = run(&["--json", "garden", "undo"]);
        assert!(undo.status.success(), "{undo:?}");
        assert_eq!(
            json_lines(&undo),
            [json!({"event": "undone", "feed_id": expected})]
        );
    }
    assert_eq!(stdout(&run(&["garden", "undo"])), "Nothing to undo.\n");
    assert_eq!(
        stdout(&run(&["garden", "decide", "5", "keep"])),
        "Kept feed #5.\n"
    );

    let applied = run(&["--json", "garden", "apply-safe"]);
    assert!(applied.status.success(), "{applied:?}");
    let lines = json_lines(&applied);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 11);
    assert_eq!(
        stdout(&run(&["garden", "apply-safe"])),
        "Applied 0 safe repairs.\n"
    );
}

/// Stores diagnoses as a refresh would, so that the garden has repairs to propose.
fn seed_diagnoses(home: &Path, diagnoses: &[(FeedId, Status, Action)]) {
    let store = Store::open(&home.join("db.sqlite")).unwrap();
    let diagnoses: HashMap<FeedId, Diagnosis> = diagnoses
        .iter()
        .map(|(id, status, action)| {
            let diagnosis = Diagnosis {
                status: *status,
                reason: format!("{status:?}"),
                evidence: Evidence::default(),
                remarks: vec![],
                action: Some(action.clone()),
                hijack_score: 0.0,
                cadence: Cadence::default(),
            };
            (*id, diagnosis)
        })
        .collect();
    store
        .save_diagnoses(&diagnoses, chrono::Utc::now())
        .unwrap();
}

/// A repair or a restore onto an address another active feed already uses is refused like
/// the other refusals: exit 2, the bare message, nothing on stdout.
#[test]
fn decisions_on_a_taken_address_exit_2() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    // Feed #2 is Orange Drenka (https://orange.example.org/feed/), #3 Orange-Drenka,
    // #9 Six Colors (https://sixcolors.com/feed.json), #10 Center for Humane Technology.
    seed_diagnoses(
        home.path(),
        &[
            (
                3,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://orange.example.org/feed/".into(),
                },
            ),
            (
                10,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://sixcolors.com/feed.json".into(),
                },
            ),
        ],
    );

    let repair = run(&["--json", "garden", "decide", "3", "repair"]);
    assert_eq!(repair.status.code(), Some(2), "{repair:?}");
    assert_eq!(
        stderr(&repair),
        "Feed #2 already uses https://orange.example.org/feed/; merge into it instead.\n"
    );
    assert!(repair.stdout.is_empty());

    // Six Colors buried, its address goes to #10; restoring Six Colors is then refused.
    assert!(run(&["garden", "decide", "9", "bury"]).status.success());
    assert!(run(&["garden", "decide", "10", "repair"]).status.success());
    let keep = run(&["garden", "decide", "9", "keep"]);
    assert_eq!(keep.status.code(), Some(2), "{keep:?}");
    assert_eq!(
        stderr(&keep),
        "Feed #10 already uses https://sixcolors.com/feed.json; cannot restore.\n"
    );
    assert!(keep.stdout.is_empty());
}

/// `apply-safe --json` prints the `finished` event with what it did: the number of repairs
/// applied and why the others were skipped. The text output says the same.
#[test]
fn apply_safe_json_counts_applied_and_skipped_repairs() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    // #3's redirect target is #2's address, so it is skipped; #5's https upgrade applies.
    seed_diagnoses(
        home.path(),
        &[
            (
                3,
                Status::Moved,
                Action::FollowRedirect {
                    url: "https://orange.example.org/feed/".into(),
                },
            ),
            (
                5,
                Status::Alive,
                Action::UpgradeHttps {
                    url: "https://girl.example.org/rss".into(),
                },
            ),
        ],
    );
    let skipped =
        "#3: Feed #2 already uses https://orange.example.org/feed/; merge into it instead.";

    let applied = run(&["--json", "garden", "apply-safe"]);
    assert!(applied.status.success(), "{applied:?}");
    let lines = json_lines(&applied);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 11);
    assert_eq!(lines[0]["applied"], 1);
    assert_eq!(lines[0]["skipped"], json!([skipped]));
    assert!(stdout(&run(&["export"])).contains("https://girl.example.org/rss"));

    // #3 is still undecided, so a second run skips it again; #5 is decided now.
    assert_eq!(
        stdout(&run(&["garden", "apply-safe"])),
        format!("Applied 0 safe repairs. Skipped 1: {skipped}\n")
    );
}

#[test]
fn migrate_reports_the_schema_version() {
    let home = tempfile::tempdir().unwrap();
    let text = ressac(home.path(), &["migrate"]);
    assert!(text.status.success(), "{text:?}");
    assert_eq!(stdout(&text), "Schema version 4.\n");
    assert!(home.path().join("db.sqlite").exists());
    let json = ressac(home.path(), &["--json", "migrate"]);
    assert!(json.status.success(), "{json:?}");
    let lines = json_lines(&json);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"], "finished");
    assert_eq!(lines[0]["summary"]["total"], 0);
}

#[test]
fn page_list_prints_reserved_pages_then_every_folder_and_feed() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let text = ressac(home.path(), &["page", "list"]);
    assert!(text.status.success(), "{text:?}");
    assert_eq!(
        stdout(&text),
        "100 reserved - Index\n\
         101 reserved - Unread\n\
         102 reserved - Today\n\
         103 reserved - Starred\n\
         104 reserved - Journal\n\
         105 reserved - Daily\n\
         106 reserved - Weekly\n\
         107 reserved - Monthly\n\
         108 reserved - Rare\n\
         109 reserved - Silent\n\
         800 reserved - Needs triage\n\
         801 reserved - All feeds\n\
         802 reserved - Cemetery\n\
         899 reserved - Guide\n\
         200 folder 1 Blogs\n\
         201 feed 1 xkcd.com\n\
         202 folder 2 potes\n\
         203 feed 2 Orange Drenka\n\
         204 feed 3 Orange-Drenka\n\
         205 feed 4 Photos from bouquiniste\n\
         206 feed 5 Girl on the Couch\n\
         300 folder 3 Biblis\n\
         301 feed 6 Bibliobsession 2.0\n\
         302 feed 7 Unphotographable\n\
         400 folder 4 Infos\n\
         401 feed 8 Daring Fireball\n\
         402 feed 9 Six Colors\n\
         403 folder 5 Science\n\
         404 feed 10 Center for Humane Technology\n\
         500 folder 6 Jon\n\
         501 folder 7 YouTube\n\
         502 feed 11 Center for Humane Technology (1)\n"
    );

    let json = ressac(home.path(), &["--json", "page", "list"]);
    assert!(json.status.success(), "{json:?}");
    let lines = json_lines(&json);
    assert_eq!(lines.len(), 32);
    assert_eq!(
        lines[0],
        json!({"event": "page", "page": 100, "kind": "reserved", "id": null, "title": "Index", "parent_page": null})
    );
    assert_eq!(
        lines[17],
        json!({"event": "page", "page": 203, "kind": "feed", "id": 2, "title": "Orange Drenka", "parent_page": 202})
    );
    assert!(lines.iter().all(|l| l["event"] == "page"));
}

#[test]
fn page_set_renumbers_and_prints_the_row() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);

    let feed = run(&["page", "set", "feed", "2", "412"]);
    assert!(feed.status.success(), "{feed:?}");
    assert_eq!(stdout(&feed), "412 feed 2 Orange Drenka\n");
    let folder = run(&["--json", "page", "set", "folder", "4", "450"]);
    assert!(folder.status.success(), "{folder:?}");
    assert_eq!(
        json_lines(&folder),
        [
            json!({"event": "page", "page": 450, "kind": "folder", "id": 4, "title": "Infos", "parent_page": null})
        ]
    );
    let list = stdout(&run(&["page", "list"]));
    assert!(list.contains("412 feed 2 Orange Drenka\n"), "{list}");
    assert!(!list.contains("203 feed 2"), "{list}");
    let infos = json_lines(&run(&["--json", "page", "list"]));
    let df = infos
        .iter()
        .find(|l| l["title"] == "Daring Fireball")
        .unwrap();
    assert_eq!(
        df["parent_page"], 450,
        "children follow their folder's new page"
    );
}

#[test]
fn page_set_refusals_exit_2_and_write_nothing() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    for (args, message) in [
        (
            &["page", "set", "feed", "2", "800"][..],
            "Page 800 is reserved.\n",
        ),
        (
            &["page", "set", "feed", "2", "100"],
            "Page 100 is reserved.\n",
        ),
        (
            &["page", "set", "feed", "2", "900"],
            "Page 900 is not between 110 and 799.\n",
        ),
        (
            &["page", "set", "feed", "2", "109"],
            "Page 109 is reserved.\n",
        ),
        (
            &["page", "set", "feed", "2", "201"],
            "Page 201 is already taken.\n",
        ),
        (
            &["page", "set", "feed", "2", "200"],
            "Page 200 is already taken.\n",
        ),
        (
            &["page", "set", "folder", "1", "201"],
            "Page 201 is already taken.\n",
        ),
        (&["page", "set", "feed", "99", "300"], "No feed #99.\n"),
        (&["page", "set", "folder", "99", "300"], "No folder #99.\n"),
        (
            &["--json", "page", "set", "feed", "99", "300"],
            "No feed #99.\n",
        ),
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert_eq!(stderr(&out), message, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let list = stdout(&run(&["page", "list"]));
    assert!(
        list.contains("203 feed 2 Orange Drenka\n"),
        "unchanged: {list}"
    );
}

#[test]
fn page_commands_exit_1_on_a_database_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("db.sqlite"), b"this is not a database").unwrap();
    for args in [&["page", "list"][..], &["page", "set", "feed", "1", "300"]] {
        let out = ressac(home.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(
            stderr(&out).starts_with("error: cannot open database"),
            "{out:?}"
        );
    }
}

/// One RSS item dated `ago` before now.
fn dated_item(guid: &str, title: &str, ago: chrono::Duration) -> String {
    let date = (chrono::Utc::now() - ago).to_rfc2822();
    format!("<item><guid>{guid}</guid><title>{title}</title><pubDate>{date}</pubDate></item>")
}

/// Three top-level feeds with a known rhythm, imported but not fetched: Rare (#1, page 110),
/// Weekly (#2, 111) and Wire (#3, 112). See Task 3a.2 for the edition they give.
async fn edition_home(server: &MockServer) -> tempfile::TempDir {
    use chrono::Duration;
    let feeds = [
        (
            "Rare",
            "/rare.xml",
            dated_item("r1", "Rare news", Duration::hours(20))
                + &dated_item("r0", "Rare old", Duration::days(100)),
        ),
        (
            "Weekly",
            "/weekly.xml",
            [1, 6, 11, 16]
                .iter()
                .map(|d| {
                    dated_item(
                        &format!("w{d}"),
                        &format!("Weekly {d}d"),
                        Duration::days(*d),
                    )
                })
                .collect::<String>(),
        ),
        (
            "Wire",
            "/wire.xml",
            [1, 3, 5]
                .iter()
                .map(|h| dated_item(&format!("h{h}"), &format!("Wire {h}h"), Duration::hours(*h)))
                .collect::<String>(),
        ),
    ];
    let base = server.uri();
    let mut outlines = String::new();
    for (title, at, items) in &feeds {
        Mock::given(method("GET"))
            .and(path(*at))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(channel(title, items), "application/rss+xml"),
            )
            .mount(server)
            .await;
        outlines.push_str(&format!(r#"<outline text="{title}" xmlUrl="{base}{at}"/>"#));
    }
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("edition.opml");
    std::fs::write(&opml, format!("<opml><body>{outlines}</body></opml>")).unwrap();
    let import = ressac_async(home.path(), &["import", opml.to_str().unwrap()]).await;
    assert!(import.status.success(), "{import:?}");
    home
}

#[tokio::test]
async fn edition_build_composes_once_a_morning() {
    let server = MockServer::start().await;
    let home = edition_home(&server).await;

    let built = ressac_async(home.path(), &["--json", "edition", "build"]).await;
    assert!(built.status.success(), "{built:?}");
    let lines = json_lines(&built);
    assert_eq!(lines.first().unwrap()["event"], "started");
    assert!(lines.iter().any(|l| l["event"] == "feed_done"));
    assert_eq!(
        lines.last().unwrap(),
        &json!({"event": "edition", "id": 1, "entries": 4, "front": 1, "week": 2,
                "wire": 1, "overflow": 2, "unreachable": 0})
    );

    let again = ressac_async(home.path(), &["edition", "build"]).await;
    assert_eq!(again.status.code(), Some(2), "{again:?}");
    assert_eq!(
        stderr(&again),
        "An edition was already built this morning.\n"
    );
    assert!(again.stdout.is_empty(), "refused before any refresh");

    // Same window, other caps: the wire keeps all three entries of Wire.
    let rebuilt = ressac_async(
        home.path(),
        &["edition", "build", "--rebuild", "--cap-wire", "3"],
    )
    .await;
    assert!(rebuilt.status.success(), "{rebuilt:?}");
    let text = stdout(&rebuilt);
    assert!(
        text.contains(": 6 entries (front 1, week 2, wire 3)."),
        "{text}"
    );
    assert!(
        text.contains("run `ressac garden` to triage"),
        "the refresh summary comes first: {text}"
    );
}

#[tokio::test]
async fn edition_show_prints_sections_overflow_and_header() {
    let server = MockServer::start().await;
    let home = edition_home(&server).await;
    let built = ressac_async(home.path(), &["edition", "build"]).await;
    assert!(built.status.success(), "{built:?}");

    let text = ressac_async(home.path(), &["edition", "show"]).await;
    assert!(text.status.success(), "{text:?}");
    let out = stdout(&text);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[..lines.len() - 1],
        [
            "Front Page",
            "1 110 Rare news",
            "This Week",
            "2 111 Weekly 1d",
            "3 111 Weekly 6d",
            "The Wire",
            "4 112 Wire 1h",
            "+2 more in Wire",
        ]
    );
    assert!(
        lines.last().unwrap().ends_with(" · 4 entries · 0 read"),
        "{out}"
    );

    let json = ressac_async(home.path(), &["--json", "edition", "show"]).await;
    assert!(json.status.success(), "{json:?}");
    let rows = json_lines(&json);
    assert_eq!(rows.len(), 5, "4 entries, then 1 overflow");
    assert_eq!(rows[0]["event"], "edition_entry");
    assert_eq!(rows[0]["edition_id"], 1);
    assert_eq!(rows[0]["section"], "front");
    assert_eq!(rows[0]["rank"], 1);
    assert_eq!(rows[0]["feed_id"], 1);
    assert_eq!(rows[0]["feed_title"], "Rare");
    assert_eq!(rows[0]["title"], "Rare news");
    assert_eq!(rows[0]["read_at"], Value::Null);
    assert_eq!(rows[0]["unreachable"], 0);
    for key in [
        "built_at",
        "window_start",
        "window_end",
        "published_at",
        "entry_id",
    ] {
        assert!(!rows[0][key].is_null(), "{key}");
    }
    assert_eq!(rows[3]["section"], "wire");
    assert_eq!(rows[3]["title"], "Wire 1h");
    assert_eq!(
        rows[4],
        json!({"event": "edition_overflow", "edition_id": 1, "section": "wire",
               "feed_id": 3, "feed_title": "Wire", "page": 112, "hidden_count": 2})
    );
}

#[test]
fn edition_show_before_any_edition() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let text = ressac(home.path(), &["edition", "show"]);
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    assert_eq!(stdout(&text), "No edition yet.\n");
    let json = ressac(home.path(), &["--json", "edition", "show"]);
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    assert!(json.stdout.is_empty(), "an empty NDJSON stream");
}

/// Four feeds on a port nobody listens to: every fetch fails to connect, so the refresh's
/// network guard (4 attempts at least, more than half network failures) calls the network
/// unreachable, and no edition is written (spec §9).
#[test]
fn edition_build_offline_writes_nothing() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let home = tempfile::tempdir().unwrap();
    let outlines: String = (1..=4)
        .map(|i| format!(r#"<outline text="F{i}" xmlUrl="http://127.0.0.1:{port}/f{i}.xml"/>"#))
        .collect();
    let opml = home.path().join("closed.opml");
    std::fs::write(&opml, format!("<opml><body>{outlines}</body></opml>")).unwrap();
    assert!(
        ressac(home.path(), &["import", opml.to_str().unwrap()])
            .status
            .success()
    );

    let out = ressac(home.path(), &["edition", "build"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        stderr(&out).ends_with("The network is unreachable: no edition built.\n"),
        "{out:?}"
    );
    assert_eq!(
        stdout(&ressac(home.path(), &["edition", "show"])),
        "No edition yet.\n"
    );

    let json = ressac(home.path(), &["--json", "edition", "build"]);
    assert_eq!(
        json.status.code(),
        Some(2),
        "still due, still offline: {json:?}"
    );
    assert!(
        json_lines(&json)
            .iter()
            .any(|l| l["event"] == "network_unreachable"),
        "{json:?}"
    );
    assert!(!json_lines(&json).iter().any(|l| l["event"] == "edition"));
}

#[test]
fn edition_commands_exit_1_on_a_database_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("db.sqlite"), b"this is not a database").unwrap();
    for args in [&["edition", "show"][..], &["edition", "build"]] {
        let out = ressac(home.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(
            stderr(&out).starts_with("error: cannot open database"),
            "{out:?}"
        );
    }
}

#[tokio::test]
async fn edition_show_and_page_list_neutralise_terminal_escapes() {
    let server = MockServer::start().await;
    let body = r#"{"version":"https://jsonfeed.org/version/1.1","title":"Feed \u001b[31mred","items":[{"id":"1","title":"Hi \u001b]0;pwn\u0007 \u009b2J","content_text":"x","date_published":"2099-01-01T00:00:00Z"}]}"#;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/feed+json"))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let opml = home.path().join("j.opml");
    std::fs::write(
        &opml,
        format!(
            r#"<opml><body><outline text="J" xmlUrl="{}/j.json"/></body></opml>"#,
            server.uri()
        ),
    )
    .unwrap();
    assert!(
        ressac_async(home.path(), &["import", opml.to_str().unwrap()])
            .await
            .status
            .success()
    );
    assert!(
        ressac_async(home.path(), &["edition", "build"])
            .await
            .status
            .success()
    );
    for args in [&["edition", "show"][..], &["page", "list"]] {
        let out = stdout(&ressac_async(home.path(), args).await);
        if args[0] == "edition" {
            assert!(out.contains("Hi \u{FFFD}]0;pwn"), "{out:?}");
        }
        assert!(
            !out.chars().any(|c| c.is_control() && c != '\n'),
            "{args:?}: {out:?}"
        );
    }
}
