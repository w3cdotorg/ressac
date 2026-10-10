//! The SQLite schema is the contract with the SwiftUI app (spec §4–5).

use std::path::Path;

use ressac_core::model::{Decision, Status};
use ressac_core::store::Store;
use rusqlite::Connection;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn schema_of(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT type || ' ' || name || ': ' || sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn migrations() -> Vec<String> {
    let mut files: Vec<_> = std::fs::read_dir(Path::new(ROOT).join("schema"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

#[test]
fn store_schema_equals_the_sql_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let live = schema_of(&Connection::open(&path).unwrap());
    let raw = Connection::open_in_memory().unwrap();
    for sql in migrations() {
        raw.execute_batch(&sql).unwrap();
    }
    assert_eq!(live, schema_of(&raw));
}

#[test]
fn overview_view_columns_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let conn = Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info('v_feed_overview')")
        .unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        cols,
        [
            "feed_id",
            "title",
            "xml_url",
            "html_url",
            "state",
            "folder_path",
            "status",
            "reason",
            "last_entry_at",
            "decision"
        ]
    );
}

/// Regenerates the fixture for the Swift app: `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract`.
#[test]
fn write_contract_fixture_when_asked() {
    if std::env::var_os("RESSAC_WRITE_CONTRACT").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("contract.sqlite");
    let store = Store::open(&work).unwrap();
    let opml =
        std::fs::read_to_string(Path::new(ROOT).join("tests/fixtures/vienna-like.opml")).unwrap();
    let outlines = ressac_core::opml::parse(&opml).unwrap();
    let now = "2026-10-10T08:00:00Z".parse().unwrap();
    store.import_outlines(&outlines, false, now).unwrap();
    store
        .record_decision(1, Decision::Keep, Status::Unchecked, None, now)
        .unwrap();
    drop(store);
    let target = Path::new(ROOT).join("tests/fixtures/contract.sqlite");
    let _ = std::fs::remove_file(&target);
    Connection::open(&work)
        .unwrap()
        .execute("VACUUM INTO ?1", [target.to_str().unwrap()])
        .unwrap();
}
