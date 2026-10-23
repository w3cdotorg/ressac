//! The garden, offscreen, through `--screenshot` (spec §7.6; inventory §4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

#[test]
fn the_garden_walk_triages_repairs_undoes_and_sets_rhythm_and_theme() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db).unwrap().execute_batch(
        "INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), \
         ('lit', 'Literature', 'SEA OF LETTERS', 1); \
         UPDATE feed SET theme_id = (SELECT id FROM theme WHERE key = 'tech') WHERE id = 8;").unwrap();
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "garden")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 02-triage count=3 of 4 left selected=6 rows=3 apply=Apply 1 Safe Repair focus=garden sections=🦠 HIJACKED · 1|🚚 MOVED · 1|🦴 FOSSIL · 1",
        "walk: 05-kept count=2 of 4 left selected=5 rows=3 kept6=true",
        "walk: 06-confirm confirming=true question=Apply 1 safe repair? preview=Girl on the Couch: follow redirect to https://girl.example.org/feed confirm=Apply",
        "walk: 09-applied count=1 of 4 left selected=2",
        "walk: 12-undone count=2 of 4 left selected=2",
        "walk: 13-all-rhythm count=10 feeds sections=⚡ DAILY · 1|📅 WEEKLY · 2|🌗 MONTHLY · 1|💤 SILENT · 2|❔ UNCHECKED · 4",
        "walk: 14-inspector rhythmIndex=0 theme=2 themes=None|Literature|Technology rhythm=Rhythm: Weekly · median 4.0 d · last entry ",
        "walk: 17-set-done rhythmIndex=1 theme=1 rhythm=Rhythm: Daily · median 4.0 d · last entry ",
        "reader: open https://daringfireball.net/",
        "walk: 19-cemetery count=1 feed selected=4 keep=Restore apply=false",
        "walk: 20-buried-key message=This feed is buried; press k to restore it first.",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(
        err.lines()
            .any(|l| l.contains("walk: 17-set-done") && l.ends_with(" (set by you)")),
        "{err}"
    );
    for shot in [
        "02-triage.png",
        "06-confirm.png",
        "14-inspector.png",
        "19-cemetery.png",
    ] {
        assert_drawn(&run.shots.join(shot));
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let decisions: Vec<(i64, String)> = conn
        .prepare("SELECT feed_id, decision FROM garden_decision ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        decisions,
        [(7, "keep".into()), (4, "bury".into()), (6, "keep".into())],
        "the repair of 5 was undone"
    );
    let girl: String = conn
        .query_row("SELECT xml_url FROM feed WHERE id = 5", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        girl, "http://girl.example.org/rss",
        "undo put the address back"
    );
    let (rhythm, theme): (Option<String>, Option<String>) = conn.query_row(
        "SELECT f.cadence_override, t.key FROM feed f LEFT JOIN theme t ON t.id = f.theme_id WHERE f.id = 8", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(
        (rhythm.as_deref(), theme.as_deref()),
        (Some("daily"), Some("lit"))
    );
}
