//! The Antiope look in the real app (spec §7.9; inventory §5).
#![cfg(target_os = "linux")]
mod common;
use common::*;

/// Rows of the default Antiope size (`ANTIOPE_SIZE_DEFAULT`), cells 12 px wide.
const SIZE: u32 = 20;
const CELL: u32 = 12;
/// The blue rules: one cell after the 22-cell index and after the 28-cell entries, blue on their right half.
const RULES: [u32; 2] = [22, 22 + 1 + 28];
const BLUE: [u8; 3] = [0, 0, 255];

/// The Mac's `awkwardTitles` (ressac-ui `pages::frame` tests), verbatim.
fn awkward() -> Vec<String> {
    vec![
        "Café 🎉 東京".into(),
        "e\u{301}cole de\u{301}ja\u{300} la\u{300}".into(),
        "👩🏽‍💻👨‍👩‍👧‍👦🇫🇷🏳️‍🌈".into(),
        "Long title with words ".repeat(12),
        "Supercalifragilisticexpialidociously-long-hyphenated".into(),
        String::new(),
        "   ".into(),
        "Tab\tand\nnewline".into(),
        "مرحبا بالعالم".into(),
        "Ελληνικά και Русский".into(),
    ]
}

/// The Antiope view's top-left corner in the window (below the menu bar), from the walk's log.
fn origin(err: &str) -> (u32, u32) {
    let line = walk_line(err, "walk: a01-origin ");
    let value = |key: &str| -> u32 {
        let v: f64 = line
            .split(key)
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        v.round() as u32
    };
    (value(" x="), value(" y="))
}

/// The x range of the blue half of rule `cell` (the right half of that cell).
fn rule_xs(cell: u32) -> std::ops::Range<u32> {
    cell * CELL + CELL / 2..(cell + 1) * CELL
}

/// Every row of the columns area (below the header, above the Minitel bar) holds each rule's
/// blue stripe, at the same x, with no blue just left or right of it.
fn assert_rules_straight(shot: &image::RgbImage, (ox, oy): (u32, u32), name: &str) {
    let bottom = shot.height() - SIZE;
    for y in oy + SIZE..bottom {
        for cell in RULES {
            let xs = rule_xs(cell);
            for x in xs.clone() {
                assert_eq!(
                    shot.get_pixel(ox + x, y).0,
                    BLUE,
                    "{name}: rule {cell} at x {} y {y}",
                    ox + x
                );
            }
            assert_ne!(
                shot.get_pixel(ox + xs.start - 1, y).0,
                BLUE,
                "{name}: rule {cell} wider on the left at y {y}"
            );
            assert_ne!(
                shot.get_pixel(ox + xs.end, y).0,
                BLUE,
                "{name}: rule {cell} wider on the right at y {y}"
            );
        }
    }
}

/// Task 7's wiring: the directory loads, digits route to Antiope, the header follows.
#[test]
fn bridges_load_and_answer_keys() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let out = run.launch_with(
        Some(&run.copy_fixture()),
        &[("RESSAC_RECETTE_WALK", "antiope-bridges")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    // 14 reserved rows + the fixture's numbered folders (7) and active feeds (10; feed 4 is buried):
    // `sqlite3 contract.sqlite "SELECT count(*) FROM v_page"` gives 18, the buried feed's 205 included.
    assert!(
        err.contains(
            "walk: b01 current=101 index=31 digit=handled select=handled;select:feed:1 header=P201"
        ),
        "{err}"
    );
}

#[test]
fn the_antiope_look_reads_pages_keys_and_columns() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "antiope")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in [
        "walk: a01-antiope look=antiope page=101 column=index focus=antiope-index",
        "walk: a02-dial selected=feed:1 page=201",
        "walk: a03-not-found selected=feed:1 page=201",
        "walk: a03-cleared shown=true cleared=true page=201",
        "walk: a04-entries column=entries selected=1",
        "walk: a04-reader column=reader focus=antiope-reader",
        "walk: a05-sommaire reader=index page=100",
        "walk: a06-guide reader=guide page=899",
        "walk: a07-suite reader=article same=true",
        "walk: a08-index-arrows column=index page=202",
        "walk: a09-native look=native selected=folder:2 focus=sidebar",
        "walk: a09-kept source=true entry=true search=comic view=true",
        "walk: a09-letters verb=nextEntry handled=true moved=true focus=sidebar",
        "walk: a10-awkward focus=antiope-index",
    ] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    let o = origin(&err);
    // a03: the header reads P437 NOT FOUND in red, the name (RESSAC, white) hidden.
    let shot = image::open(run.shots.join("a03-not-found.png"))
        .unwrap()
        .to_rgb8();
    assert!(has_colour(
        &shot,
        row_rect(0, 0, 18, SIZE, 1.0).offset(o),
        [255, 0, 0]
    ));
    assert!(!has_colour(
        &shot,
        row_rect(0, 0, 22, SIZE, 1.0).offset(o),
        [255, 255, 255]
    ));
    // a02: the same header shows the page and RESSAC in white.
    let a02 = image::open(run.shots.join("a02-dial.png"))
        .unwrap()
        .to_rgb8();
    assert!(has_colour(
        &a02,
        row_rect(0, 5, 6, SIZE, 1.0).offset(o),
        [255, 255, 255]
    ));
    // a01: a black page to the window's foot right of the Minitel keys; both rules straight.
    let a01 = image::open(run.shots.join("a01-antiope.png"))
        .unwrap()
        .to_rgb8();
    assert_eq!(
        a01.get_pixel(a01.width() - 5, a01.height() - 5).0,
        [0, 0, 0]
    );
    assert_rules_straight(&a01, o, "a01");
    // a09: Native again, its panes drawn (not black).
    let a09 = image::open(run.shots.join("a09-native.png"))
        .unwrap()
        .to_rgb8();
    assert_ne!(
        a09.get_pixel(a09.width() / 2, a09.height() / 2).0,
        [0, 0, 0]
    );
    // The walk only read: the database holds the fixture's six entries.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM entry", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 6);
}

/// Review Focus 1: awkward titles never bend a column.
#[test]
fn awkward_titles_keep_the_separators_straight() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let db = run.copy_fixture();
    let titles = awkward();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        for (id, title) in (1..=6).zip(&titles) {
            conn.execute(
                "UPDATE entry SET title = ?1 WHERE id = ?2",
                rusqlite::params![title, id],
            )
            .unwrap();
        }
        // The index too: every feed but the buried one gets an awkward title.
        for (id, title) in [1, 2, 3, 5, 6, 7, 8, 9, 10, 11]
            .into_iter()
            .zip(titles.iter().cycle())
        {
            conn.execute(
                "UPDATE feed SET title = ?1 WHERE id = ?2",
                rusqlite::params![title, id],
            )
            .unwrap();
        }
    }
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "antiope")]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    let o = origin(&err);
    let shot = image::open(run.shots.join("a10-awkward.png"))
        .unwrap()
        .to_rgb8();
    assert_rules_straight(&shot, o, "a10");
    // Nothing but the eight colours in the index and entries columns (rules included).
    let columns = Rect {
        x: o.0,
        y: o.1 + SIZE,
        width: (RULES[1] + 1) * CELL,
        height: shot.height() - SIZE - (o.1 + SIZE),
    };
    for (x, y) in columns.pixels() {
        let p = shot.get_pixel(x, y).0;
        assert!(TELETEXT.contains(&p), "a10: off-palette {p:?} at {x},{y}");
    }
}

#[test]
fn scrolling_two_thousand_entries_is_logged() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let db = run.copy_fixture();
    insert_entries(&db, 1, 2000);
    let out = run.launch_with(
        Some(&db),
        &[
            ("RESSAC_RECETTE_WALK", "antiope-scroll"),
            ("QT_QPA_UPDATE_IDLE_TIME", "0"),
        ],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for step in [1, 10] {
        let line = walk_line(&err, &format!("frames: antiope step={step} n="));
        let p95: f64 = line
            .split(" p95=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(p95 < 16.7, "{line}"); // smooth in the container's software renderer
        println!("{line}");
    }
    // `j` held on long unread articles: each press marks the entry it leaves read. The list
    // already shows the mark (its optimistic rows), so a write only asks for the coalesced
    // counts reload, never a full reload (which re-read every loaded row, once per press).
    let holds = [
        "native hold-j-page",
        "antiope hold-j-page",
        "antiope hold-j",
    ]
    .map(|hold| {
        let line = walk_line(&err, &format!("frames: {hold} n="));
        let counts = walk_line(&err, &format!("walk: {hold} counts "));
        println!("{line}\n{counts}");
        (line, counts)
    });
    for (line, counts) in holds {
        assert!(line.contains(" presses=60 moved=60 "), "{line}");
        // `settled` counts the 60 presses' marks; under load, the 1 s auto-read of the row where
        // the hold stops (AUTO_READ_DELAY) can settle inside this hold's window too (seen: 61),
        // so one more is allowed, never fewer.
        let settled: u32 = field(counts, "settled");
        assert!(
            counts.contains(" wrote=0 ") && (60..=61).contains(&settled),
            "{counts}"
        );
        // The sidebar badge (and Antiope's index) show the 60 marks soon after the release.
        let after: u32 = field(counts, "after");
        assert!(counts.contains("caught=true") && after <= 1500, "{counts}");
        // No long frame comes from the writes: the long frames are the presses whose own
        // handling fills a frame (`slow`: the list's per-press sync, which grows with the
        // loaded rows and the load on the machine), the coalesced counts reloads (at most 2 in
        // a 2 s hold), the `data_version` echo of the writes (a full reload per 3 s poll), and
        // under load a few render frames a slow press pushed past 16.7 ms (`SLACK`). Before
        // the fix every write added its own full reload: 58 to 75 long frames for 60 presses.
        // `frameDriver` flips a square on every swap, so p95 is diluted by filler frames.
        const SLACK: u32 = 5;
        let over16: u32 = field(line, "over16");
        let slow: u32 = field(line, "slow");
        let invalidated: u32 = field(counts, "invalidated");
        assert!(over16 <= slow + 2 + invalidated + SLACK, "{line}\n{counts}");
    }
}

/// The number after ` key=` in a logged line.
fn field<T: std::str::FromStr>(line: &str, key: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    line.split(&format!(" {key}="))
        .nth(1)
        .unwrap_or_else(|| panic!("{key} missing in {line}"))
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
