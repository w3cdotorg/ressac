//! Runs the real binary.

use std::path::Path;
use std::process::{Command, Output};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/vienna-like.opml"
);

fn ressac(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ressac"))
        .args(["--db", home.join("db.sqlite").to_str().unwrap()])
        .args(args)
        .env("HOME", home)
        .output()
        .expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
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
    use std::process::Stdio;
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_ressac"))
        .args([
            "--db",
            home.path().join("db.sqlite").to_str().unwrap(),
            "export",
        ])
        .env("HOME", home.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let code = child.wait().unwrap().code();
    assert!(matches!(code, Some(0) | Some(1)), "{code:?}");
}

#[test]
fn usage_errors_exit_2() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(ressac(home.path(), &["frobnicate"]).status.code(), Some(2));
}

#[tokio::test]
async fn fetch_json_streams_events() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
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
