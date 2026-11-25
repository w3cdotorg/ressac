//! The reading page, offscreen, through `--screenshot` (spec §7.3; inventory §1.3, §2.3, §2.4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

#[test]
fn the_reading_page_walks_sources_entries_search_banner_and_garden() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert_eq!(
        &shots(&run)[..7],
        [
            "01-ready.png",
            "02-folder-blogs.png",
            "03-starred.png",
            "04-search.png",
            "05-banner.png",
            "06-garden.png",
            "07-unread-filter.png",
        ]
    );
    for shot in shots(&run) {
        // The closing states of the walk include the Sea Chart and Settings windows, smaller than the main one.
        if shot == "97-settings.png" || shot == "96-chart.png" {
            assert_drawn_min(&run.shots.join(&shot), 500, 300);
        } else {
            assert_drawn(&run.shots.join(&shot));
        }
    }
    assert_ne!(
        std::fs::read(run.shots.join("02-folder-blogs.png")).unwrap(),
        std::fs::read(run.shots.join("04-search.png")).unwrap()
    );
    for line in [
        "walk: 02-folder-blogs entries=4 expanded=true",
        "walk: 03-starred selected=1",
        "walk: 04-search entries=3",
        "walk: 05-banner visible=true",
        "walk: 06-garden active=false unreadEnabled=false",
        "walk: 07-unread-filter clicked=true all=true unread=false",
        "walk: 07-feed-menu first=true again=true other=false feedId=2 badge=3",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(!err.contains("Could not read the database"), "{err}");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let starred: Vec<i64> = conn
        .prepare("SELECT id FROM entry WHERE starred_at IS NOT NULL ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        starred,
        [1, 4],
        "the walk starred Blogs' newest entry (1); 4 was starred already"
    );
}
