//! The SQLite schema is the contract with the SwiftUI app (spec §4–5).

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use ressac_core::extract::{Article, ExtractError};
use ressac_core::garden;
use ressac_core::model::{
    Action, Cadence, Decision, Diagnosis, ErrorKind, Evidence, FeedId, Hop, Remark, Status, ts,
};
use ressac_core::parse::{ParsedEntry, ParsedFeed};
use ressac_core::source::FetchAttempt;
use ressac_core::store::Store;
use rusqlite::{Connection, OpenFlags};

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

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .unwrap();
    stmt.query_map([table], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// A store freshly created by this binary, reopened as a plain connection.
fn live_store() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let conn = Connection::open(&path).unwrap();
    (dir, conn)
}

fn fixture() -> Connection {
    Connection::open_with_flags(
        Path::new(ROOT).join("tests/fixtures/contract.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
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

/// The committed fixture must not drift from the schema the store creates.
#[test]
fn contract_fixture_schema_matches_the_live_store() {
    let (_dir, live) = live_store();
    assert_eq!(schema_of(&fixture()), schema_of(&live));
}

#[test]
fn overview_view_columns_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_feed_overview"),
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

#[test]
fn reading_columns_and_views_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_entry"),
        [
            "entry_id",
            "feed_id",
            "feed_title",
            "folder_id",
            "title",
            "author",
            "url",
            "published_at",
            "is_read",
            "is_starred",
            "has_fulltext",
            "summary_html",
            "content_html"
        ]
    );
    assert_eq!(columns(&conn, "v_unread_count"), ["feed_id", "unread"]);
    assert_eq!(
        columns(&conn, "v_garden_row"),
        [
            "feed_id",
            "title",
            "folder_path",
            "state",
            "status",
            "reason",
            "clue",
            "evidence",
            "remarks",
            "proposed_action",
            "needs_triage",
            "last_decision",
            "wayback_url",
            "html_url",
            "xml_url"
        ]
    );
    assert_eq!(
        columns(&conn, "entry_fulltext"),
        ["entry_id", "fetched_at", "html", "title", "byline", "error"]
    );
    assert_eq!(columns(&conn, "entry_fts"), ["title", "author", "body"]);
    assert!(columns(&conn, "feed").contains(&"prefer_full_text".to_string()));
    assert!(columns(&conn, "feed_health").contains(&"needs_triage".to_string()));
    let version: i64 = conn
        .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
    assert_eq!(version, ressac_core::store::SCHEMA_VERSION);
}

#[test]
fn page_columns_and_view_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_page"),
        ["page", "kind", "ref_id", "title", "parent_page"]
    );
    assert!(columns(&conn, "folder").contains(&"page".to_string()));
    assert!(columns(&conn, "feed").contains(&"page".to_string()));
}

/// What the Swift tests rely on in the committed fixture (rows found by guid or title).
#[test]
fn contract_fixture_covers_the_reading_cases() {
    let db = fixture();
    let int = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    let text = |sql: &str| -> String { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(int("SELECT version FROM schema_meta"), 3);

    // One FTS row per entry; the backlog rule on xkcd's first fill.
    assert_eq!(int("SELECT count(*) FROM entry"), 6);
    assert_eq!(
        int("SELECT count(*) FROM entry WHERE id NOT IN (SELECT rowid FROM entry_fts)"),
        0
    );
    assert_eq!(
        int("SELECT count(*) FROM entry WHERE read_at IS NOT NULL"),
        1
    );
    assert_eq!(
        int("SELECT read_at IS NOT NULL FROM entry WHERE guid = 'x-old'"),
        1
    );
    assert_eq!(
        int(
            "SELECT unread FROM v_unread_count WHERE feed_id = (SELECT id FROM feed WHERE title = 'xkcd.com')"
        ),
        3
    );

    // Review Focus 1: no link, no title, only a summary; listed and searchable.
    assert_eq!(
        int(
            "SELECT count(*) FROM v_entry WHERE title IS NULL AND url IS NULL AND summary_html IS NOT NULL"
        ),
        1
    );
    assert_eq!(
        int(
            "SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'link' AND rowid = (SELECT id FROM entry WHERE guid = 'x-untitled')"
        ),
        1
    );
    // Search folds diacritics; one starred entry.
    assert_eq!(
        int(
            "SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'cafe' AND rowid = (SELECT id FROM entry WHERE guid = 'x-starred')"
        ),
        1
    );
    assert_eq!(int("SELECT count(*) FROM v_entry WHERE is_starred"), 1);

    // Full text: one article (its text replaced the FTS body), one stored failure.
    assert_eq!(int("SELECT count(*) FROM v_entry WHERE has_fulltext"), 1);
    assert_eq!(
        int(
            "SELECT count(*) FROM entry_fts WHERE entry_fts MATCH 'whole' AND rowid = (SELECT id FROM entry WHERE guid = 'df-extracted')"
        ),
        1
    );
    assert_eq!(
        text("SELECT error FROM entry_fulltext WHERE error IS NOT NULL"),
        "The page is behind a login."
    );
    assert_eq!(
        int("SELECT count(*) FROM feed WHERE prefer_full_text = 1"),
        1
    );

    // Garden.
    assert_eq!(
        int("SELECT count(*) FROM v_garden_row WHERE needs_triage = 1"),
        3
    );
    assert_eq!(
        int("SELECT count(*) FROM v_garden_row WHERE state = 'buried'"),
        1
    );
    assert_eq!(
        text("SELECT last_decision FROM v_garden_row WHERE title = 'Unphotographable'"),
        "keep"
    );
    assert_eq!(
        text("SELECT clue FROM v_garden_row WHERE title = 'Unphotographable'"),
        "410"
    );
    assert_eq!(
        int("SELECT count(*) FROM v_garden_row WHERE wayback_url IS NOT NULL"),
        1
    );
    assert_eq!(int("SELECT count(*) FROM fetch_log"), 3);
}

/// Regenerates the fixture shared with the Swift tests:
/// `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract`.
#[test]
fn write_contract_fixture_when_asked() {
    if std::env::var_os("RESSAC_WRITE_CONTRACT").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("contract.sqlite");
    fill_contract_store(&work);
    let target = Path::new(ROOT).join("tests/fixtures/contract.sqlite");
    let _ = std::fs::remove_file(&target);
    Connection::open(&work)
        .unwrap()
        .execute("VACUUM INTO ?1", [target.to_str().unwrap()])
        .unwrap();
}

fn now() -> DateTime<Utc> {
    "2026-10-10T08:00:00Z".parse().unwrap()
}

/// A bare entry (no link, no title, no author): add fields with struct update syntax.
fn parsed(guid: &str, days_ago: i64, summary: &str) -> ParsedEntry {
    ParsedEntry {
        guid: guid.into(),
        url: None,
        title: None,
        author: None,
        published_at: now() - Duration::days(days_ago),
        published_at_is_fallback: false,
        updated_at: None,
        summary_html: Some(summary.into()),
        content_html: None,
    }
}

/// A successful fetch of the feed's own address at `now()`.
fn fetched(store: &Store, feed_id: FeedId, entries: Vec<ParsedEntry>) -> FetchAttempt {
    let url = store.feed(feed_id).unwrap().xml_url;
    FetchAttempt {
        feed_id,
        at: now(),
        requested_url: url.clone(),
        final_url: url,
        http_status: Some(200),
        error: None,
        redirect_chain: vec![],
        content_type: Some("application/rss+xml".into()),
        bytes: 2048,
        duration_ms: 120,
        not_modified: false,
        etag: None,
        last_modified: None,
        feed: Some(ParsedFeed {
            title: None,
            description: None,
            site_url: None,
            entries,
        }),
    }
}

/// A fetch an hour ago that hops to https (301) and ends on HTTP 410.
fn gone(store: &Store, feed_id: FeedId) -> FetchAttempt {
    let url = store.feed(feed_id).unwrap().xml_url;
    let https = url.replacen("http://", "https://", 1);
    FetchAttempt {
        feed_id,
        at: now() - Duration::hours(1),
        requested_url: url,
        final_url: https.clone(),
        http_status: Some(410),
        error: Some(ErrorKind::Http(410)),
        redirect_chain: vec![Hop {
            status: 301,
            url: https,
        }],
        content_type: Some("text/html".into()),
        bytes: 0,
        duration_ms: 80,
        not_modified: false,
        etag: None,
        last_modified: None,
        feed: None,
    }
}

fn diagnosis(status: Status, clue: &str, action: Option<Action>) -> Diagnosis {
    Diagnosis {
        status,
        reason: format!("{} in the contract fixture.", status.as_str()),
        evidence: Evidence {
            bullets: vec![format!("Clue: {clue}.")],
            clue: clue.into(),
            language: None,
        },
        remarks: vec![],
        action,
        hijack_score: 0.0,
        cadence: Cadence::default(),
    }
}

/// The fixture's content, described in `contract_fixture_covers_the_reading_cases`.
fn fill_contract_store(path: &Path) {
    let store = Store::open(path).unwrap();
    let opml =
        std::fs::read_to_string(Path::new(ROOT).join("tests/fixtures/vienna-like.opml")).unwrap();
    store
        .import_outlines(&ressac_core::opml::parse(&opml).unwrap(), false, now())
        .unwrap();
    let feeds = store.feeds().unwrap();
    let id = |title: &str| feeds.iter().find(|f| f.title == title).unwrap().id;
    let (xkcd, fireball, orange, photos) = (
        id("xkcd.com"),
        id("Daring Fireball"),
        id("Orange Drenka"),
        id("Photos from bouquiniste"),
    );
    let (girl, biblio, unphoto) = (
        id("Girl on the Couch"),
        id("Bibliobsession 2.0"),
        id("Unphotographable"),
    );

    // Reading: xkcd's first fill (the backlog rule marks x-old read) and two truncated
    // Daring Fireball entries.
    store
        .record_attempt(
            "contract",
            &fetched(
                &store,
                xkcd,
                vec![
                    ParsedEntry {
                        url: Some("https://xkcd.com/3001/".into()),
                        title: Some("Recent comic".into()),
                        author: Some("Randall Munroe".into()),
                        ..parsed("x-recent", 1, "<p>A recent <b>comic</b> about tides.</p>")
                    },
                    ParsedEntry {
                        url: Some("https://xkcd.com/2900/".into()),
                        title: Some("Old comic".into()),
                        ..parsed("x-old", 30, "<p>An old comic from the backlog.</p>")
                    },
                    parsed(
                        "x-untitled",
                        2,
                        "<p>Only a summary, with no link and no title.</p>",
                    ),
                    ParsedEntry {
                        url: Some("https://xkcd.com/3000/".into()),
                        title: Some("Café comic".into()),
                        content_html: Some(
                            "<p>Full content about a <i>café</i> by the sea.</p>".into(),
                        ),
                        ..parsed("x-starred", 3, "<p>Short.</p>")
                    },
                ],
            ),
        )
        .unwrap();
    store
        .record_attempt(
            "contract",
            &fetched(
                &store,
                fireball,
                vec![
                    ParsedEntry {
                        url: Some("https://daringfireball.net/2026/10/tides".into()),
                        title: Some("Linked: Tides".into()),
                        author: Some("John Gruber".into()),
                        ..parsed("df-extracted", 1, "<p>Truncated…</p>")
                    },
                    ParsedEntry {
                        url: Some("https://daringfireball.net/2026/10/walled".into()),
                        title: Some("Walled".into()),
                        ..parsed("df-walled", 2, "<p>Truncated too…</p>")
                    },
                ],
            ),
        )
        .unwrap();
    store
        .record_attempt("contract", &gone(&store, unphoto))
        .unwrap();

    // Full text: one article, one stored failure, and the feed that prefers full text.
    let conn = Connection::open(path).unwrap();
    let entry = |guid: &str| -> i64 {
        conn.query_row("SELECT id FROM entry WHERE guid = ?1", [guid], |r| r.get(0))
            .unwrap()
    };
    store.set_prefer_full_text(fireball, true).unwrap();
    let article = Article {
        html: "<article><p>The whole article about tides.</p></article>".into(),
        title: Some("Tides".into()),
        byline: Some("John Gruber".into()),
        text: "The whole article about tides.".into(),
    };
    store
        .save_fulltext(entry("df-extracted"), &Ok(article), now())
        .unwrap();
    store
        .save_fulltext(entry("df-walled"), &Err(ExtractError::LoginWall), now())
        .unwrap();
    // `starred_at` is client-written (the app's column), so the fixture sets it directly.
    conn.execute(
        "UPDATE entry SET starred_at = ?1 WHERE guid = 'x-starred'",
        [ts(now())],
    )
    .unwrap();

    // Garden: three feeds need triage; one Dead feed is kept, one feed is buried.
    store
        .save_diagnoses(
            &HashMap::from([
                (xkcd, diagnosis(Status::Alive, "", None)),
                (fireball, diagnosis(Status::Alive, "", None)),
                (
                    girl,
                    Diagnosis {
                        remarks: vec![Remark::HttpOnly],
                        ..diagnosis(
                            Status::Moved,
                            "301",
                            Some(Action::FollowRedirect {
                                url: "https://girl.example.org/feed".into(),
                            }),
                        )
                    },
                ),
                (
                    biblio,
                    diagnosis(Status::Hijacked, "vie ⚠", Some(Action::Bury)),
                ),
                (orange, diagnosis(Status::Fossil, "2014", None)),
                (unphoto, diagnosis(Status::Dead, "410", Some(Action::Bury))),
            ]),
            now(),
        )
        .unwrap();
    store
        .set_wayback(
            unphoto,
            "http://web.archive.org/web/2019/http://www.unphotographable.com/",
        )
        .unwrap();
    garden::decide(&store, unphoto, Decision::Keep, now()).unwrap();
    garden::decide(&store, photos, Decision::Bury, now()).unwrap();
    garden::recompute_triage(&store).unwrap();
}
