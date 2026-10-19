//! The SQLite schema is the contract with the SwiftUI app (spec §4–5).

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use ressac_core::extract::{Article, ExtractError};
use ressac_core::garden;
use ressac_core::model::{
    Action, Cadence, CadenceGroup, Decision, Diagnosis, ErrorKind, Evidence, FeedId, Hop, Remark,
    Status, ts,
};
use ressac_core::parse::{ParsedEntry, ParsedFeed};
use ressac_core::source::FetchAttempt;
use ressac_core::store::Store;
use rusqlite::{Connection, OpenFlags, params};

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
    assert_eq!(version, 5);
    assert_eq!(version, ressac_core::store::SCHEMA_VERSION);
}

/// `v_entry` and `v_unread_count` cover active feeds only, so a buried feed's entries leave
/// the reading lists. The Cemetery counts them on `entry` itself (`Queries.garden` in the
/// app), which needs its `feed_id` and `read_at` columns.
#[test]
fn buried_feeds_leave_the_reading_views_but_keep_their_unread_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("buried.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .import_outlines(
            &ressac_core::opml::parse(
                r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            )
            .unwrap(),
            false,
            now(),
        )
        .unwrap();
    let entries = vec![
        parsed("a1", 1, "<p>One.</p>"),
        parsed("a2", 2, "<p>Two.</p>"),
    ];
    store
        .record_attempt("contract", &fetched(&store, 1, entries))
        .unwrap();
    garden::decide(&store, 1, Decision::Bury, now()).unwrap();
    let conn = Connection::open(&path).unwrap();
    let int = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(int("SELECT count(*) FROM v_entry"), 0);
    assert_eq!(int("SELECT count(*) FROM v_unread_count"), 0);
    assert_eq!(
        int("SELECT count(*) FROM entry WHERE feed_id = 1 AND read_at IS NULL"),
        2
    );
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

#[test]
fn cadence_and_edition_columns_and_views_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_cadence"),
        [
            "feed_id",
            "title",
            "page",
            "folder_path",
            "effective_group",
            "computed_group",
            "is_override",
            "median_interval_days",
            "entries_per_month",
            "last_entry_at",
            "unread"
        ]
    );
    assert_eq!(
        columns(&conn, "v_edition_today"),
        [
            "edition_id",
            "built_at",
            "window_start",
            "window_end",
            "unreachable",
            "entry_id",
            "section",
            "rank",
            "feed_id",
            "feed_title",
            "title",
            "published_at",
            "read_at",
            "starred_at"
        ]
    );
    assert_eq!(
        columns(&conn, "v_edition_overflow"),
        [
            "edition_id",
            "section",
            "feed_id",
            "feed_title",
            "page",
            "hidden_count"
        ]
    );
    assert_eq!(
        columns(&conn, "edition"),
        [
            "id",
            "built_at",
            "window_start",
            "window_end",
            "unreachable"
        ]
    );
    assert_eq!(
        columns(&conn, "edition_entry"),
        ["edition_id", "entry_id", "section", "rank"]
    );
    assert_eq!(
        columns(&conn, "edition_overflow"),
        ["edition_id", "feed_id", "section", "hidden_count"]
    );
    assert!(columns(&conn, "feed").contains(&"cadence_override".to_string()));
    assert!(columns(&conn, "feed_health").contains(&"cadence_group".to_string()));
}

/// What the app reads of the sea chart's tables through GRDB (`Queries.themes()`,
/// `Queries.feedThemeKeys()`), and the tables its change observation watches.
#[test]
fn chart_columns_read_by_the_app_are_stable() {
    let (_dir, conn) = live_store();
    let theme = columns(&conn, "theme");
    for column in ["id", "key", "name", "sea_name", "position"] {
        assert!(theme.contains(&column.to_string()), "theme.{column}");
    }
    assert!(columns(&conn, "feed").contains(&"theme_id".to_string()));
    for table in ["territory", "territory_feed"] {
        assert!(!columns(&conn, table).is_empty(), "{table}");
    }
}

/// What the Swift tests rely on in the committed fixture (rows found by guid or title).
#[test]
fn contract_fixture_covers_the_reading_cases() {
    let db = fixture();
    let int = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    let text = |sql: &str| -> String { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(int("SELECT version FROM schema_meta"), 5);

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

/// Spec §11: page assignment on the contract database. The Swift tests rely on these
/// numbers (looked up by title, never by id).
#[test]
fn contract_fixture_numbers_every_folder_and_feed() {
    let db = fixture();
    let mut stmt = db
        .prepare("SELECT page, kind, title, parent_page FROM v_page ORDER BY page")
        .unwrap();
    let rows: Vec<(u16, String, String, Option<u16>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        (200, "folder", "Blogs", None),
        (201, "feed", "xkcd.com", Some(200)),
        (202, "folder", "potes", Some(200)),
        (203, "feed", "Orange Drenka", Some(202)),
        (204, "feed", "Orange-Drenka", Some(202)),
        (205, "feed", "Photos from bouquiniste", Some(202)),
        (206, "feed", "Girl on the Couch", Some(200)),
        (300, "folder", "Biblis", None),
        (301, "feed", "Bibliobsession 2.0", Some(300)),
        (302, "feed", "Unphotographable", Some(300)),
        (400, "folder", "Infos", None),
        (401, "feed", "Daring Fireball", Some(400)),
        (402, "feed", "Six Colors", Some(400)),
        (403, "folder", "Science", Some(400)),
        (404, "feed", "Center for Humane Technology", Some(403)),
        (500, "folder", "Jon", None),
        (501, "folder", "YouTube", Some(500)),
        (502, "feed", "Center for Humane Technology (1)", Some(501)),
    ]
    .map(|(page, kind, title, parent)| (page, kind.to_string(), title.to_string(), parent));
    assert_eq!(
        rows, expected,
        "every folder and feed, the buried Photos included"
    );
    let state: String = db
        .query_row("SELECT state FROM feed WHERE page = 205", [], |r| r.get(0))
        .unwrap();
    assert_eq!(state, "buried");
}

/// What the Swift tests rely on for rhythm and the edition (rows found by title or guid).
#[test]
fn contract_fixture_covers_rhythm_and_the_edition() {
    let db = fixture();
    let int = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };

    // v_cadence: diagnosed active feeds only (the buried Photos and the undiagnosed are out).
    let mut stmt = db
        .prepare("SELECT title, effective_group, computed_group, is_override FROM v_cadence ORDER BY page")
        .unwrap();
    let rows: Vec<(String, String, String, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        ("xkcd.com", "daily", "daily", false),
        ("Orange Drenka", "silent", "silent", false),
        ("Girl on the Couch", "monthly", "monthly", false),
        ("Bibliobsession 2.0", "weekly", "rare", true),
        ("Unphotographable", "silent", "silent", false),
        ("Daring Fireball", "weekly", "weekly", false),
    ]
    .map(|(t, e, c, o)| (t.to_string(), e.to_string(), c.to_string(), o));
    assert_eq!(rows, expected);
    assert_eq!(
        int("SELECT unread FROM v_cadence WHERE title = 'xkcd.com'"),
        3
    );

    // The edition: This Week then The Wire, no Front Page; xkcd keeps two entries.
    assert_eq!(int("SELECT count(*) FROM edition"), 1);
    assert_eq!(int("SELECT unreachable FROM edition"), 1);
    let mut stmt = db
        .prepare("SELECT rank, section, feed_title, title FROM v_edition_today ORDER BY rank")
        .unwrap();
    let entries: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        (1, "week", "Daring Fireball", "Linked: Tides"),
        (2, "week", "Daring Fireball", "Walled"),
        (3, "wire", "xkcd.com", "Recent comic"),
    ]
    .map(|(r, s, f, t)| (r, s.to_string(), f.to_string(), t.to_string()));
    assert_eq!(entries, expected);
    let overflow: (String, String, u16, i64) = db
        .query_row(
            "SELECT section, feed_title, page, hidden_count FROM v_edition_overflow",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(overflow, ("wire".into(), "xkcd.com".into(), 201, 2));
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

/// A stored rhythm: the latest entry a day ago (in 2014 for Silent), `median` days apart.
fn rhythm(group: CadenceGroup, median: f64) -> Cadence {
    let last = if group == CadenceGroup::Silent {
        "2014-06-01T00:00:00Z".parse().unwrap()
    } else {
        now() - Duration::days(1)
    };
    Cadence {
        last_entry_at: Some(last),
        median_interval_days: Some(median),
        entries_per_month: 30.4375 / median,
        group,
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
                (
                    xkcd,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Daily, 0.5),
                        ..diagnosis(Status::Alive, "", None)
                    },
                ),
                (
                    fireball,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Weekly, 4.0),
                        ..diagnosis(Status::Alive, "", None)
                    },
                ),
                (
                    girl,
                    Diagnosis {
                        remarks: vec![Remark::HttpOnly],
                        cadence: rhythm(CadenceGroup::Monthly, 20.0),
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
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Rare, 60.0),
                        ..diagnosis(Status::Hijacked, "vie ⚠", Some(Action::Bury))
                    },
                ),
                (
                    orange,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Silent, 30.0),
                        ..diagnosis(Status::Fossil, "2014", None)
                    },
                ),
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

    // Rhythm: one forced group. The column is core-written (`ressac feed cadence`); the
    // fixture sets it directly, like `starred_at` above.
    conn.execute(
        "UPDATE feed SET cadence_override = 'weekly' WHERE id = ?1",
        [biblio],
    )
    .unwrap();
    // Morning edition, written by hand (the edition builder is phase 2b): Daring Fireball's
    // two entries in This Week, xkcd's newest in The Wire and two more left in xkcd.
    conn.execute(
        "INSERT INTO edition (id, built_at, window_start, window_end, unreachable) VALUES (1, ?1, ?2, ?1, 1)",
        params![ts(now()), ts(now() - Duration::days(1))],
    )
    .unwrap();
    for (guid, section, rank) in [
        ("df-extracted", "week", 1),
        ("df-walled", "week", 2),
        ("x-recent", "wire", 3),
    ] {
        conn.execute(
            "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?1, ?2, ?3)",
            params![entry(guid), section, rank],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (1, ?1, 'wire', 2)",
        [xkcd],
    )
    .unwrap();
}
