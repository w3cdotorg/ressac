//! The reader page offscreen (spec §7.4; master Review Focus 4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

const MARKS_NULL: &str =
    r#""inline":null,"external":null,"onload":null,"onerror":null,"jslink":null"#;

/// 600 KB of hostile markup, 300 levels deep, for entry 1 (the walk's `03-starred` shows it).
fn hostile_body() -> String {
    let unit = concat!(
        r#"<script>document.documentElement.setAttribute("data-inline","ran")</script><img src="x" onerror="document.documentElement.setAttribute('data-onerror','ran')"><a href="javascript:void(0)">x</a><p>Hostile paragraph.</p>"#,
        "\n\n"
    );
    "<div>".repeat(300) + &unit.repeat(600 * 1024 / unit.len() + 1) + &"</div>".repeat(300)
}

#[test]
fn article_scripts_never_run_and_a_hostile_600_kb_entry_still_loads() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE entry SET content_html = ?1 WHERE id = 1",
            [hostile_body()],
        )
        .unwrap();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(
        err.contains(&format!(
            r#"reader-proof: {{"app":"ran",{MARKS_NULL},"scripts":2,"#
        )),
        "{err}"
    );
    let line = err
        .lines()
        .find(|l| l.contains("reader: ressac:page/1-"))
        .unwrap_or_else(|| panic!("no page for entry 1 in {err}"));
    assert!(
        line.contains(&format!(r#""app":"ran",{MARKS_NULL},"scripts":0,"#)),
        "{line}"
    );
    let paragraphs: usize = line
        .split(r#""paragraphs":"#)
        .nth(1)
        .unwrap()
        .trim_end_matches(['}', ' '])
        .parse()
        .unwrap();
    assert!(paragraphs > 100, "{line}"); // the plain-text fallback, one paragraph per blank-line block
    assert_drawn(&run.shots.join("03-starred.png"));
}
