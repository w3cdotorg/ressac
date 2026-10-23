//! Keys through the window filter, offscreen (spec §7.2, §10: keys reaching the router while the reader has focus).
#![cfg(target_os = "linux")]

mod common;
use common::*;

fn page_of<'a>(err: &'a str, step: &str) -> &'a str {
    err.lines()
        .find(|l| l.contains(step))
        .and_then(|l| l.split(" page=").nth(1))
        .unwrap_or_else(|| panic!("{step} in {err}"))
}

#[test]
fn reading_keys_reach_the_router_before_the_lists_the_reader_and_the_menus() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: 13-j-in-sidebar selected=1 focus=sidebar",
        "walk: 14-arrows-in-list selected=3 focus=entries",
        "walk: 15-space-in-reader selected=4",
        "walk: 16-text-field search=j selected=-1",
        "walk: 17-garden-keys verb=previousEntry handled=false",
        "walk: 18-auto-read selected=4 read=false page=ressac:page/4-",
        "walk: 21-auto-read-done read=true page=ressac:page/4-",
        "walk: 23-menu read=Mark as Unread enabled=true star=Unstar",
        "walk: 24-text-size bigger=18 actual=17",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    // The reader's own item has the focus (not a list), and k, j, m, m, s, s still act.
    let line = err
        .lines()
        .find(|l| l.contains("walk: 22-reader-keys"))
        .unwrap_or_else(|| panic!("walk 22 in {err}"));
    assert!(
        line.contains("focus=reader item=QtWebEngineCore::RenderWidgetHostViewQtDelegateItem")
            && line.ends_with(" k=1 j=4 m=false m2=true s=false s2=true"),
        "{line}"
    );
    assert_eq!(
        page_of(&err, "walk: 18-auto-read"),
        page_of(&err, "walk: 21-auto-read-done")
    ); // marking read never reloads
    let conn = rusqlite::Connection::open(&db).unwrap();
    let read: Vec<i64> = conn
        .prepare(
            "SELECT id FROM entry WHERE read_at IS NOT NULL AND id IN (1, 2, 3, 4) ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        read,
        [1, 2, 3, 4],
        "j/↓ and Space mark what they leave (1, 3), auto-read marks 4, 2 was read"
    );
}
