//! The Journal, offscreen, through `--screenshot` (spec §7.5; inventory §3).
#![cfg(target_os = "linux")]

mod common;
use common::*;

fn editions(db: &std::path::Path) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM edition", [], |r| r.get(0))
        .unwrap()
}

/// The ids of the entries marked read.
fn read_entries(db: &std::path::Path) -> Vec<i64> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT id FROM entry WHERE read_at IS NOT NULL ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn the_journal_lists_the_edition_by_section_and_builds_nothing_in_recette() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(Some(&db), &[("TZ", "UTC")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    // How many are read at that point depends on the walk's auto-read timers (1 s on a selection).
    for part in [
        "walk: 23-journal rows=4 phase=idle placeholder= header=Saturday 10 October · 3 entries · ",
        " read · 1 feed unreachable focus=entries",
    ] {
        assert!(err.contains(part), "{part} missing in {err}");
    }
    assert_drawn(&run.shots.join("23-journal.png"));
    assert_eq!(editions(&db), 1, "no automatic build in recette");
}

#[test]
fn mark_all_as_read_in_the_journal_marks_the_edition_and_not_the_overflow() {
    let run = Run::new();
    let db = run.copy_fixture();
    assert_eq!(read_entries(&db), [2], "the fixture reads only entry 2");
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "mark-all")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-journal rows=4 can=true",
        "walk: 03-mark-all can=false",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    // The edition's entries (1, 5, 6) are read; 3 and 4, outside it, stay unread; 2 stays read.
    assert_eq!(read_entries(&db), [1, 2, 5, 6]);
}

#[test]
fn an_offline_build_shows_retry_and_the_banner_and_writes_nothing() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "UPDATE feed SET xml_url = 'http://127.0.0.' || id || ':9/feed' WHERE state = 'active'",
        )
        .unwrap();
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "journal")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 05-offline phase=offline placeholder=Offline: no edition yet retry=true banner=The network is unreachable: no edition built.",
        "walk: 06-retry phase=building",
        "walk: 08-retried phase=offline after-trigger=offline",
        // A fetch in flight turned the build away: due, with Retry; Retry builds once it is over.
        "walk: 11-refused phase=due retry=true active=false",
        "walk: 12-retry-again phase=building",
        "walk: 14-offline-again phase=offline",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(
        !err.contains("no way into the run coordinator"),
        "the launch build found no Runs: {err}"
    );
    assert_drawn(&run.shots.join("05-offline.png"));
    assert_eq!(editions(&db), 1, "offline: nothing written");
}
