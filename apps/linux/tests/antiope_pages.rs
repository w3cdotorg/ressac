//! Antiope's pages in the real app (spec §7.9; inventory §5): SOMMAIRE, GUIDE, P104 to the end
//! page, the garden, the search prompt, the banner and Settings.
#![cfg(target_os = "linux")]
mod common;
use common::*;

/// Rows of the default Antiope size (`ANTIOPE_SIZE_DEFAULT`), cells 12 px wide.
const SIZE: u32 = 20;
/// The columns' first cells: the 22-cell index, a rule, the 28-cell entries, a rule, the reader.
const ENTRIES: u32 = 22 + 1;
const ENTRY_CELLS: u32 = 28;
const READER: u32 = ENTRIES + ENTRY_CELLS + 1;
const RED: [u8; 3] = [255, 0, 0];
const YELLOW: [u8; 3] = [255, 255, 0];
const CYAN: [u8; 3] = [0, 255, 255];

/// The Antiope view's top-left corner in the window (below the menu bar), from the walk's log.
fn origin(err: &str) -> (u32, u32) {
    let line = walk_line(err, "walk: c01-origin ");
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

/// The copy: a newer edition (entries 1 and 3 on the front page, two more of feed 1 held back),
/// feed 2 ailing and waiting for triage (P800 lists it under a yellow AILING heading; the
/// fixture has it a fossil, white on blue), every address on a closed loopback port.
fn prepare(db: &std::path::Path) {
    rusqlite::Connection::open(db)
        .unwrap()
        .execute_batch(
            "INSERT INTO edition (id, built_at, window_start, window_end, unreachable) \
               VALUES (2, '2026-10-11T08:00:00.000Z', '2026-10-10T08:00:00.000Z', '2026-10-11T08:00:00.000Z', 0); \
             INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (2, 1, 'front', 1), (2, 3, 'front', 2); \
             INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (2, 1, 'front', 2); \
             INSERT OR REPLACE INTO feed_health (feed_id, status, reason, evidence, remarks, last_entry_at, \
               median_interval_days, entries_per_month, computed_at, needs_triage, cadence_group) \
               VALUES (2, 'Ailing', 'Ailing in the walk''s copy.', '{\"bullets\":[\"Clue: 2014.\"],\"clue\":\"2014\",\"language\":null}', \
               '[]', '2014-06-01T00:00:00.000Z', 30.0, 1.0145833333333334, '2026-10-10T08:00:00.000Z', 1, 'silent'); \
             UPDATE feed SET xml_url = 'http://127.0.0.' || id || ':9/feed', html_url = 'http://127.0.0.' || id || ':9/'; \
             UPDATE entry SET url = 'http://127.0.0.1:9/entry/' || id;",
        )
        .unwrap();
}

/// The longest vertical run of `colour` pixels in `rect` (a glyph's stem), in pixels.
fn tallest_run(shot: &image::RgbImage, rect: Rect, colour: [u8; 3]) -> u32 {
    let mut tallest = 0;
    for x in rect.x..rect.x + rect.width {
        let mut run = 0;
        for y in rect.y..rect.y + rect.height {
            run = if shot.get_pixel(x, y).0 == colour {
                run + 1
            } else {
                0
            };
            tallest = tallest.max(run);
        }
    }
    tallest
}

#[test]
fn the_antiope_pages_walk_reads_every_page() {
    let run = Run::new();
    let db = run.copy_fixture();
    prepare(&db);
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "antiope-pages")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    // c05c's write wait and c06b/c06c's waits log when they run out, rather than failing a step later.
    assert!(!err.contains("walk: wait timed out"), "{err}");
    // Each step's whole line, as logged.
    for line in [
        // P100: RESSAC in double height, the logo's two rows, a blank, then the 31 pages of
        // the directory (14 reserved, 7 folders, 10 feeds) two by two: 16 rows.
        "walk: c01-sommaire reader=index rows=20 first=RESSAC",
        "walk: c02-guide reader=guide",
        // The edition's header, a blank, FRONT PAGE, entries 1 and 3, feed 1's overflow row; not
        // every entry read, so no FIN DE L'EDITION row.
        "walk: c03-journal selected=journal rows=6 end=false",
        // SUITE past the edition's last entry (3) marks it read and lands on the end page.
        "walk: c04-suite-end reader=end read=true",
        "walk: c05-retour reader=article",
        // From the end page, a page dialled leaves it.
        "walk: c05b-end-jump before=end reader=article page=201",
        // The edition marked read away from P104 (on P201): arriving on P104 reads it again.
        "walk: c05c-change-away page=201",
        "walk: c05d-journal-again selected=journal read=2/2 end=true",
        // P800 lists Hijacked 6, Moved 5 and Ailing 2; the inspector shows the first, 6.
        "walk: c06-garden selected=garden:needs-triage column=entries inspector=25 entries-focus=true",
        // `k` keeps 6 through the entries column's rows (B5's keep path): no refusal message.
        "walk: c06b-garden-key message=",
        // Shift+R asks about Moved 5's repair; the confirm button has the keyboard, and a look
        // switch drops the question.
        "walk: c06c-repairs confirming=true count=2 of 4 left",
        "walk: c06d-repairs-switch focus=Apply look=native confirming=false",
        "walk: c06e-back look=antiope page=800 confirming=false",
        // Digits in the prompt are text, one per press: the header never shows a page being
        // typed (`P4`, `P41`) nor NOT FOUND, and the page stays.
        "walk: c07-prompt-digits search=412 page=800 prompt=false typed=4|41|412 headers=P800|P800|P800 after=P800",
        // The prompt goes with the view, its text kept; `j` reaches the window filter at once as
        // Native's command (not typed into a field). It acts on nothing here: a garden has no
        // entry list to move in.
        "walk: c08-switch-with-prompt look=native search=412x focus=sidebar letter=nextEntry handled=false search-after=412x",
        "walk: c09-banner banner=true",
        "walk: c10-settings appearance=antiope bedstead=true",
    ] {
        let step = line.split(' ').take(2).collect::<Vec<_>>().join(" ");
        assert_eq!(walk_line(&err, &format!("{step} ")), line, "{err}");
    }
    let o = origin(&err);
    let shot = |name: &str| {
        image::open(run.shots.join(format!("{name}.png")))
            .unwrap()
            .to_rgb8()
    };
    // c01: the double-height RESSAC in the reader column's first two rows (below its title):
    // a yellow stem taller than one row, which a single-height glyph never is.
    let c01 = shot("c01-sommaire");
    let reader_cells = (c01.width() - o.0) * 10 / (6 * SIZE) - READER;
    let title = Rect {
        height: 2 * SIZE,
        ..row_rect(2, READER, reader_cells, SIZE, 1.0)
    }
    .offset(o);
    let tallest = tallest_run(&c01, title, YELLOW);
    assert!(
        tallest > SIZE && tallest <= 2 * SIZE,
        "c01: the title's tallest yellow stem is {tallest} px"
    );
    // c03: P104's header row (the date) in cyan, the entries column's first row.
    let c03 = shot("c03-journal");
    assert!(has_colour(
        &c03,
        row_rect(2, ENTRIES, ENTRY_CELLS, SIZE, 1.0).offset(o),
        CYAN
    ));
    // c06: P800's AILING heading, yellow text in the entries column (a feed row's only yellow
    // is its dot in the first cell; the progress label is right of cell 20).
    let c06 = shot("c06-garden");
    let rows = (c06.height() - o.1) / SIZE;
    assert!(
        (2..rows).any(|row| {
            let heading = row_rect(row, ENTRIES + 2, 18, SIZE, 1.0).offset(o);
            has_colour(&c06, heading, YELLOW) || has_colour(&c06, heading, RED)
        }),
        "c06: no red or yellow heading in the entries column"
    );
    // c09: the banner's row under the header, red.
    let c09 = shot("c09-banner");
    assert!(has_colour(
        &c09,
        row_rect(1, 0, 22, SIZE, 1.0).offset(o),
        RED
    ));
    // c10: the Settings window.
    assert_drawn_min(&run.shots.join("c10-settings.png"), 300, 300);
    // The edition's two entries are read: one by SUITE past the last, one by the change away from P104.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let unread: i64 = conn
        .query_row(
            "SELECT count(*) FROM entry WHERE id IN (1, 3) AND read_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unread, 0);
    // `k` kept feed 6; the dropped question applied nothing (feed 5 keeps the copy's address).
    let kept: i64 = conn
        .query_row(
            "SELECT count(*) FROM garden_decision WHERE feed_id = 6 AND decision = 'keep'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, 1);
    let moved: String = conn
        .query_row("SELECT xml_url FROM feed WHERE id = 5", [], |r| r.get(0))
        .unwrap();
    assert_eq!(moved, "http://127.0.0.5:9/feed");
}
