//! The offscreen smoke test (spec §8.3): every page, every window drawn, keys reach the database.
#![cfg(target_os = "linux")]

mod common;
use common::*;

const SHOTS: [&str; 13] = [
    "s01-native",
    "s02-keys",
    "s03-journal",
    "s04-garden",
    "s05-antiope",
    "s06-p201",
    "s07-p100",
    "s08-p899",
    "s09-p104",
    "s10-p800",
    "s11-chart",
    "s12-settings",
    "s13-native-again",
];

/// The value of ` key=` in a walk line (up to the next space).
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let rest = line
        .split(&format!(" {key}="))
        .nth(1)
        .unwrap_or_else(|| panic!("{key} missing in {line}"));
    rest.split(' ').next().unwrap()
}

fn smoke(scale: &str) {
    let run = Run::new();
    let db = run.copy_fixture();
    unreachable_feeds(&db); // feeds at 127.0.0.<id>:9: no traffic leaves the box
    let out = run.launch_with(
        Some(&db),
        &[
            ("TZ", "UTC"),
            ("RESSAC_RECETTE_WALK", "smoke"),
            ("QT_SCALE_FACTOR", scale),
        ],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(!err.contains("timed out"), "a smoke wait ran out: {err}");
    for shot in SHOTS {
        let png = run.shots.join(format!("{shot}.png"));
        if shot == "s12-settings" || shot == "s11-chart" {
            assert_drawn_min(&png, 500, 300)
        } else {
            assert_drawn(&png)
        }
        let (w, _) = image::image_dimensions(&png).unwrap();
        let want = if shot == "s12-settings" { 700 } else { 1800 };
        assert!(
            scale == "1" || w >= want,
            "{shot} drawn at device pixel ratio {scale}: {w} px wide"
        );
    }
    let s01 = walk_line(&err, "walk: smoke-s01-native ");
    let first: i64 = field(s01, "page")
        .strip_prefix("ressac:page/")
        .and_then(|rest| rest.split('-').next())
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| panic!("no page id in {s01}"));
    assert!(field(s01, "entries").parse::<i64>().unwrap() >= 2, "{s01}");
    // j left the first entry read, m marked the next one, s starred it: the ids are the step's own.
    let s02 = walk_line(&err, "walk: smoke-s02-keys ");
    let second: i64 = field(s02, "selected").parse().unwrap();
    assert_ne!(second, first, "j moved on from the first entry: {s02}");
    assert_eq!(field(s02, "read"), format!("{first},{second}"), "{s02}");
    assert_eq!(field(s02, "starred"), second.to_string(), "{s02}");
    for line in [
        "walk: smoke-s03-journal journal=true".to_string(),
        "walk: smoke-s05-antiope look=antiope page=101".to_string(),
        "walk: smoke-s06-p201 page=201".to_string(),
        "walk: smoke-s07-p100 reader=index".to_string(),
        "walk: smoke-s08-p899 reader=guide".to_string(),
        "walk: smoke-s09-p104 selected=journal".to_string(),
        "walk: smoke-s10-p800 selected=garden:needs-triage".to_string(),
        "walk: smoke-s11-chart chart=drawn".to_string(),
        "walk: smoke-s12-settings appearance=antiope".to_string(),
        "walk: smoke-s13-native-again look=native".to_string(),
    ] {
        assert!(err.contains(&line), "{line} missing in {err}");
    }
    let s04 = walk_line(&err, "walk: smoke-s04-garden ");
    assert!(
        s04.contains(" garden=true inspector=") && !s04.ends_with("inspector="),
        "{s04}"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let fixture = rusqlite::Connection::open(fixture_path()).unwrap();
    let flags = |c: &rusqlite::Connection, id: i64| -> (bool, bool) {
        c.query_row(
            "SELECT read_at IS NOT NULL, starred_at IS NOT NULL FROM entry WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert!(
        !flags(&fixture, second).1,
        "the fixture's entry {second} is not starred, so s starred it"
    );
    assert!(flags(&conn, first).0, "j marked entry {first} read");
    assert_eq!(
        flags(&conn, second),
        (true, true),
        "m read and s starred {second}"
    );
    let starred = |c: &rusqlite::Connection| -> i64 {
        c.query_row(
            "SELECT COUNT(*) FROM entry WHERE starred_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(starred(&conn), starred(&fixture) + 1, "s starred one entry");
}

#[test]
fn the_smoke_walk_reaches_every_page_at_scale_1() {
    smoke("1")
}

#[test]
fn the_smoke_walk_reaches_every_page_at_scale_2() {
    smoke("2")
}

#[test]
fn the_default_walk_shows_every_state_the_spec_lists() {
    let run = Run::new();
    let out = run.launch(Some(&run.copy_fixture()));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(!err.contains("timed out"), "{err}");
    // Needs Triage with its inspector is B4's `06-garden` (never shown twice); the rest closes the walk.
    for shot in [
        "06-garden",
        "91-antiope-101",
        "92-antiope-100",
        "93-antiope-899",
        "94-antiope-104",
        "95-antiope-800",
        "96-chart",
        "97-settings",
        "98-native",
    ] {
        assert_drawn_min(&run.shots.join(format!("{shot}.png")), 500, 300);
    }
    for line in [
        "walk: 91-antiope-101 look=antiope page=101",
        "walk: 92-antiope-100 reader=index",
        "walk: 93-antiope-899 reader=guide",
        "walk: 94-antiope-104 selected=journal",
        "walk: 95-antiope-800 selected=garden:needs-triage",
        "walk: 96-chart chart=drawn",
        "walk: 97-settings appearance=antiope",
        "walk: 98-native look=native",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    let names = shots(&run);
    assert_eq!(
        &names[names.len() - 8..],
        [
            "91-antiope-101.png",
            "92-antiope-100.png",
            "93-antiope-899.png",
            "94-antiope-104.png",
            "95-antiope-800.png",
            "96-chart.png",
            "97-settings.png",
            "98-native.png",
        ],
        "the closing states come last"
    );
}

#[test]
fn the_first_run_on_an_empty_home_offers_the_import() {
    // --screenshot needs RESSAC_DB, so the walk starts on a file that is not a database and the
    // hook presses "Use Default Database": the launch then runs on the empty home, as a first launch.
    let run = Run::new();
    let bad = run.home.join("notes.sqlite");
    std::fs::write(
        &bad,
        "only words, long enough to fill the first page of a SQLite header",
    )
    .unwrap();
    std::fs::write(
        run.home.join("walk.opml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><head><title>First</title></head><body>
<outline text="Alpha" type="rss" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline text="Beta" type="rss" xmlUrl="http://127.0.0.22:9/beta.xml"/>
</body></opml>"#,
    )
    .unwrap();
    std::fs::write(run.home.join("notes.txt"), "not a feed list").unwrap();
    let out = run.launch_with(
        Some(&bad),
        &[
            ("TZ", "UTC"),
            ("RESSAC_RECETTE_WALK", "library"),
            ("RESSAC_RECETTE_PRESS_DEFAULT", "drop-db"),
        ],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-first-run empty=true add=true import=true page=true",
        "walk: 03-drop started=true again=false busy=true",
        "walk: 10-imported empty=false step=idle",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    let db = run.data_home().join("ressac/ressac.sqlite");
    assert!(db.exists(), "the database lands under XDG_DATA_HOME/ressac");
    let feeds: i64 = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM feed", [], |r| r.get(0))
        .unwrap();
    assert_eq!(feeds, 2, "the import wrote into it");
    let beside: Vec<_> = std::fs::read_dir(run.cwd()).unwrap().collect();
    assert!(
        beside.is_empty(),
        "nothing next to the working directory: {beside:?}"
    );
    assert!(!run.home.join("ressac.sqlite").exists());
}
