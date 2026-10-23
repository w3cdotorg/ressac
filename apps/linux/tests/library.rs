//! Add feed, OPML, first run, Settings and drag and drop, offscreen (spec §7.7; inventory §2.9, §7).
#![cfg(target_os = "linux")]

mod common;
use common::*;

const WALK_OPML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><head><title>Walk</title></head><body>
<outline text="Walk">
<outline text="Alpha" type="rss" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline text="Alpha again" type="rss" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline text="Beta" type="rss" xmlUrl="http://127.0.0.22:9/beta.xml"/>
</outline>
<outline text="Gamma" type="rss" xmlUrl="http://127.0.0.23:9/gamma.xml"/>
</body></opml>"#;

/// An empty library (RESSAC_DB names a missing file: the launch creates it) and the walk's files.
fn prepare(run: &Run) -> std::path::PathBuf {
    std::fs::write(run.home.join("walk.opml"), WALK_OPML).unwrap();
    std::fs::write(run.home.join("notes.txt"), "not a feed list").unwrap();
    std::fs::write(
        run.home.join("junk.sqlite"),
        "this is not sqlite, just some text padding",
    )
    .unwrap();
    run.home.join("library.sqlite")
}

#[test]
fn first_run_imports_an_opml_then_adds_moves_exports_and_sets_the_database() {
    let run = Run::new();
    let db = prepare(&run);
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "library")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-first-run empty=true add=true import=true page=true",
        "walk: 03-drop started=true again=false busy=true",
        "walk: 05-confirm step=confirming title=Import “walk.opml”? lines=3 feeds in 1 new folder.|1 duplicate inside the file will be merged:|“Alpha again” into “Alpha” note=Ressac fetches every feed right after the import. canImport=true",
        "walk: 10-imported empty=false step=idle walk=true gamma=true longRun=false",
        "walk: 11-add-dialog open=true address=https://example.org/feed.xml themes=None folders=Feeds|\u{2003}Walk canAdd=true",
        "walk: 14-add-refusal error=Ressac only follows public web addresses. showLog=false open=true adding=false",
        "walk: 17-add-known error=Already followed: Alpha. showLog=false",
        "walk: 17-closed open=false",
        "walk: 18-export started=true refused=false",
        "walk: 21-drag moved=true ignored=false own=false notFeed=false",
        "walk: 23-dragged selected=feed:2",
        "walk: 23-drag-in moved=true",
        "walk: 25-entry-drag plain=https://example.org/a moz=\"https://example.org/a\\nA title\" none=0",
        "walk: 26-settings fonts=System Sans|System Serif|Newsreader|Atkinson Hyperlegible Next widths=Narrow|Medium|Wide|40 columns registered=true",
        "note=Shared with the ressac command in Terminal. ressacDb=true",
        "walk: 28-junk refusal=This file is not a usable Ressac database. relaunch=false",
        "walk: 29-new refusal= relaunch=true stored=true note=In Terminal, pass --db with this path to use the same database.",
        "settings: relaunch",
        "walk: 30-default stored= note=Shared with the ressac command in Terminal.",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    for shot in [
        "01-ready.png",
        "05-confirm.png",
        "11-add-dialog.png",
        "14-add-refusal.png",
        "10-imported.png",
    ] {
        assert_drawn(&run.shots.join(shot));
    }
    let data_line = err
        .lines()
        .find(|l| l.contains("walk: 27-data path="))
        .unwrap();
    assert!(
        data_line.contains(&format!(
            "path={}",
            run.home.join("data/ressac/ressac.sqlite").display()
        )),
        "{data_line}"
    );
    assert!(
        err.contains(&format!(
            "open: file://{}",
            run.home.join("state/ressac/logs").display()
        )),
        "{err}"
    );
    assert!(run.home.join("state/ressac/logs").is_dir());
    assert_drawn_min(&run.shots.join("26-settings.png"), 540, 400);
    assert_drawn_min(&run.shots.join("28-junk.png"), 540, 400);
    let conf =
        std::fs::read_to_string(run.home.join("config/ressac/ressac-kde.conf")).unwrap_or_default();
    assert!(
        !conf.contains("databasePath"),
        "Use Default removed the key: {conf}"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let folders: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM folder ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(folders, [(1, "Walk".to_string())]);
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM feed", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 3, "the refused adds wrote nothing");
    let attempts: i64 = conn
        .query_row("SELECT COUNT(*) FROM fetch_log", [], |r| r.get(0))
        .unwrap();
    assert!(
        attempts >= 3,
        "the import was followed by a fetch of every feed"
    );
    let exported = std::fs::read_to_string(run.home.join("Ressac.opml")).unwrap();
    assert!(
        exported.contains(r#"xmlUrl="http://127.0.0.22:9/beta.xml""#),
        "{exported}"
    );
    let placed: Vec<(i64, Option<i64>)> = conn
        .prepare("SELECT id, folder_id FROM feed ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        placed,
        [(1, Some(1)), (2, None), (3, Some(1))],
        "Beta to the top level, Gamma into Walk"
    );
}
