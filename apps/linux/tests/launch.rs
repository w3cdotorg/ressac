//! The launch states, offscreen, through `--screenshot` (spec §7.2, §8.2; inventory §1.1).
#![cfg(target_os = "linux")]

mod common;

use std::path::PathBuf;

use common::*;
use ressac_core::store::{SchemaState, Store};

#[test]
fn a_current_database_opens_ready() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_clean(&out);
    assert_eq!(shots(&run)[0], "01-ready.png");
    assert_eq!(shots(&run).len(), 6);
    assert_drawn(&run.shots.join("01-ready.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current);
    assert!(run.home.join("state/ressac/logs").is_dir()); // logging::init under XDG_STATE_HOME
    assert!(!run.home.join("config/ressac/ressac-kde.conf").exists()); // the launch only reads settings
}

#[test]
fn a_missing_or_empty_database_is_created_before_opening() {
    let run = Run::new();
    let missing = run.home.join("data/new/ressac.sqlite");
    let empty = run.home.join("empty.sqlite");
    std::fs::write(&empty, b"").unwrap();
    for db in [missing, empty] {
        let _ = std::fs::remove_dir_all(&run.shots);
        let out = run.launch(Some(&db));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(shots(&run)[0], "01-ready.png", "{db:?}");
        assert_eq!(shots(&run).len(), 6, "{db:?}");
        assert_eq!(Store::probe(&db).unwrap(), SchemaState::Current, "{db:?}");
    }
}

#[test]
fn a_newer_database_is_refused_and_left_alone() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute("UPDATE schema_meta SET version = 99", [])
        .unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_clean(&out);
    assert_eq!(shots(&run), ["01-too-new.png"]);
    assert_drawn(&run.shots.join("01-too-new.png"));
    assert_eq!(Store::probe(&db).unwrap(), SchemaState::Newer(99));
}

#[test]
fn a_file_that_is_not_a_database_fails_and_is_left_alone() {
    let run = Run::new();
    let db = run.home.join("notes.sqlite");
    let words = "not a database, only words, long enough to fill the first page of a SQLite header";
    std::fs::write(&db, words).unwrap();
    let out = run.launch(Some(&db));
    assert_eq!(out.status.code(), Some(0));
    assert_clean(&out);
    let log: String = std::fs::read_dir(run.home.join("state/ressac/logs"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
        .collect();
    let want = format!("launch: Failed(\"Cannot open {}: ", db.display());
    assert!(log.contains(&want), "{log}");
    assert_eq!(shots(&run), ["01-failed.png"]);
    assert_drawn(&run.shots.join("01-failed.png"));
    assert_eq!(std::fs::read_to_string(&db).unwrap(), words);
}

#[test]
fn the_screenshot_flag_needs_ressac_db() {
    let run = Run::new();
    let out = run.launch(None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        format!("{}\n", ressac_ui::launch::SCREENSHOT_NEEDS_DB)
    );
    assert!(!run.shots.exists());
}

/// A settings file whose `databasePath` points at `bad`, and a file that is not a database.
fn seed_bad_setting(run: &Run) -> PathBuf {
    let bad = run.home.join("notes.sqlite");
    std::fs::write(
        &bad,
        "only words, long enough to fill the first page of a SQLite header",
    )
    .unwrap();
    let conf = run.home.join("config/ressac/ressac-kde.conf");
    std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
    std::fs::write(
        &conf,
        format!(
            "[General]\ndatabasePath={}\nreader.size=21\n",
            bad.display()
        ),
    )
    .unwrap();
    bad
}

fn conf_text(run: &Run) -> String {
    std::fs::read_to_string(run.home.join("config/ressac/ressac-kde.conf")).unwrap()
}

#[test]
fn use_default_database_clears_the_setting_and_relaunches_on_the_default_location() {
    let run = Run::new();
    let bad = seed_bad_setting(&run);
    let default_db = run.home.join("data/ressac/ressac.sqlite");
    // The hook forgets RESSAC_DB (required by --screenshot) just before the press.
    let out = run.launch_with(Some(&bad), &[("RESSAC_RECETTE_PRESS_DEFAULT", "drop-db")]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_clean(&out);
    let names = shots(&run);
    assert_eq!(names.len(), 7, "{names:?}"); // the failed screen, then the ready one and its walk
    assert!(names.contains(&"01-failed.png".to_string()), "{names:?}");
    assert!(names.contains(&"02-ready.png".to_string()), "{names:?}");
    assert_drawn(&run.shots.join("02-ready.png"));
    assert_eq!(Store::probe(&default_db).unwrap(), SchemaState::Current);
    let conf = conf_text(&run);
    assert!(!conf.contains("databasePath"), "{conf}");
    assert!(conf.contains("reader.size=21"), "{conf}");
}

#[test]
fn ressac_db_still_wins_after_use_default_database() {
    let run = Run::new();
    let bad = seed_bad_setting(&run);
    let out = run.launch_with(Some(&bad), &[("RESSAC_RECETTE_PRESS_DEFAULT", "keep-db")]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(shots(&run), ["01-failed.png", "02-failed.png"]);
    assert!(!conf_text(&run).contains("databasePath"));
    assert!(
        !run.home.join("data/ressac/ressac.sqlite").exists(),
        "the default location was used"
    );
    assert!(
        std::fs::read_to_string(&bad)
            .unwrap()
            .starts_with("only words")
    );
}
