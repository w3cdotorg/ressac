//! The pages Antiope draws itself, as rows: P100 SOMMAIRE, P899 GUIDE, the end of the
//! edition and P104's morning edition (the Mac's `SommairePage`, `GuidePage`, `JournalPage`
//! and `EndOfEditionPage`). Pure: widths in cells, the zone comes in as an argument.

use std::collections::HashSet;
use std::fmt::Display;

use chrono::TimeZone;
use ressac_core::edition::Section;
use ressac_core::model::FeedId;
use ressac_core::store::{EditionOverflowRow, JournalItem, JournalView};

use super::frame::{empty_line, entry_lines, progress_text};
use super::{DirRow, Line, LineRole, PageDirectory, ReaderPage, blank_line, len, wide};
use crate::journal::{self, Phase};
use crate::teletext::{CellLine, CellSpan, Color, DatePattern, date};

/// The wave of the app icon in half-block mosaics, repeated across the page.
pub const WAVE_TOP: &str = " ▄▀▀▄   ";
pub const WAVE_BOTTOM: &str = "▀    ▀▄▄";

/// Column-major, as a printed index: the first half of the pages down the left, the rest down
/// the right. Each pair is the indices of its left and right rows.
pub fn sommaire_pairs(rows: &[DirRow]) -> Vec<(usize, Option<usize>)> {
    let left = rows.len().div_ceil(2);
    (0..left)
        .map(|index| (index, (index + left < rows.len()).then_some(index + left)))
        .collect()
}

/// Each column's width; the one or two cells left over separate them.
pub fn half_width(width: usize) -> usize {
    width.saturating_sub(1) / 2
}

/// An index row with the reserved titles in colour.
pub fn sommaire_line(row: &DirRow, width: usize, directory: &PageDirectory) -> CellLine {
    super::frame::index_line(row, width, directory.is_root_folder(row), false, true)
}

/// `RESSAC`, centred in yellow (drawn in double height).
pub fn sommaire_title(width: usize) -> CellLine {
    CellLine::centred("RESSAC", width, Color::Yellow)
}

/// Two rows of the wave in cyan.
pub fn sommaire_logo(width: usize) -> [CellLine; 2] {
    [WAVE_TOP, WAVE_BOTTOM].map(|pattern| {
        let pattern: Vec<char> = pattern.chars().collect();
        let row: String = (0..width).map(|i| pattern[i % pattern.len()]).collect();
        CellLine::new(vec![CellSpan::new(&row, wide(width), Color::Cyan)])
    })
}

/// P100: a double-height `RESSAC` over the wave, a blank row, then every page in two columns
/// with its count (rows of role `Pair`, clicked on the left or right half).
pub fn sommaire_page(directory: &PageDirectory, width: usize) -> Vec<Line> {
    let mut title = Line::new("title", LineRole::Title, sommaire_title(width));
    title.double_height = true;
    let mut lines = vec![title];
    for (index, logo) in sommaire_logo(width).into_iter().enumerate() {
        lines.push(Line::new(format!("logo-{index}"), LineRole::Text, logo));
    }
    lines.push(blank_line("blank", width));
    let rows = directory.rows();
    let half = half_width(width);
    for (index, (left, right)) in sommaire_pairs(rows).into_iter().enumerate() {
        let mut spans = sommaire_line(&rows[left], half, directory).spans;
        spans.extend(CellLine::blank(width - 2 * half).spans);
        match right {
            Some(right) => spans.extend(sommaire_line(&rows[right], half, directory).spans),
            None => spans.extend(CellLine::blank(half).spans),
        }
        lines.push(Line::new(
            format!("pair-{index}"),
            LineRole::Pair {
                left: rows[left].number,
                right: right.map(|r| rows[r].number),
                half,
            },
            CellLine::new(spans),
        ));
    }
    lines
}

/// One line of P899: the keys, spelled out, and what they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideKey {
    pub keys: &'static str,
    pub action: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideSection {
    pub title: &'static str,
    pub keys: &'static [GuideKey],
}

const fn key(keys: &'static str, action: &'static str) -> GuideKey {
    GuideKey { keys, action }
}

/// Every key of the window, grouped as the GUIDE page of a Minitel service, spelled in
/// capitals: Bedstead has no modifier symbols.
pub const GUIDE_SECTIONS: [GuideSection; 5] = [
    GuideSection {
        title: "PAGES",
        keys: &[
            key("0-9", "type a page number"),
            key("BACKSPACE", "erase the last digit"),
            key("ESC", "cancel the number"),
            key("ALT S", "SOMMAIRE: every page"),
            key("ALT G  ?", "GUIDE: this page"),
        ],
    },
    GuideSection {
        title: "READING",
        keys: &[
            key("SPACE  J", "SUITE: next page"),
            key("K", "RETOUR: previous page"),
            key("UP DOWN", "previous or next entry"),
            key("M", "mark read or unread"),
            key("S", "star or unstar"),
            key("I", "mosaics or original images"),
            key("ALT R", "RÉPÉTITION: refresh"),
            key("CTRL RETURN", "open in the browser"),
        ],
    },
    GuideSection {
        title: "COLUMNS",
        keys: &[
            key("LEFT RIGHT", "change column"),
            key("UP DOWN", "in the index: previous or next page"),
        ],
    },
    GuideSection {
        title: "GARDEN",
        keys: &[
            key("K", "keep"),
            key("R", "repair"),
            key("B", "bury"),
            key("U", "undo"),
            key("O", "open the site"),
            key("W", "open the Wayback snapshot"),
            key("SHIFT R", "apply the safe repairs"),
        ],
    },
    GuideSection {
        title: "MENUS",
        keys: &[
            key("CTRL F", "search: RETURN keeps it, ESC clears it"),
            key("CTRL R  F5", "refresh the selection"),
            key("CTRL SHIFT R", "refresh every feed"),
            key("CTRL SHIFT A", "mark all as read"),
            key("CTRL + -", "bigger or smaller text"),
            key("CTRL O", "import OPML"),
            key("CTRL SHIFT E", "export OPML"),
        ],
    },
];

/// The key column: a leading cell, the longest keys (`CTRL SHIFT R`), and one blank cell before the action.
pub const GUIDE_KEY_CELLS: usize = 14;

/// For each section: a blank row, the heading black on green, then one row per key, the keys
/// in cyan and the action in white. Every row is exactly `width` cells.
pub fn guide_lines(width: usize) -> Vec<CellLine> {
    let key_cells = GUIDE_KEY_CELLS.min(width);
    let mut lines = Vec::new();
    for section in GUIDE_SECTIONS {
        lines.push(CellLine::blank(width));
        lines.push(CellLine::new(vec![
            CellSpan::new(&format!(" {}", section.title), wide(width), Color::Black)
                .on(Color::Green),
        ]));
        for key in section.keys {
            lines.push(CellLine::new(vec![
                CellSpan::new(&format!(" {}", key.keys), wide(key_cells), Color::Cyan),
                CellSpan::new(key.action, wide(width - key_cells), Color::White),
            ]));
        }
    }
    lines
}

/// P899: `GUIDE` in double height, then the keys.
pub fn guide_page(width: usize) -> Vec<Line> {
    let mut title = Line::new(
        "title",
        LineRole::Title,
        CellLine::centred("GUIDE", width, Color::Yellow),
    );
    title.double_height = true;
    let mut lines = vec![title];
    for (index, cells) in guide_lines(width).into_iter().enumerate() {
        let role = if cells.text().trim().is_empty() {
            LineRole::Blank
        } else {
            LineRole::Text
        };
        lines.push(Line::new(format!("guide-{index}"), role, cells));
    }
    lines
}

/// `FIN DE L'EDITION`: the Minitel exception, yellow, centred, in double height.
pub fn end_line(width: usize) -> Line {
    let mut line = Line::new(
        "end",
        LineRole::End,
        CellLine::centred("FIN DE L'EDITION", width, Color::Yellow),
    );
    line.double_height = true;
    line
}

/// What the reader column draws for a page that is not an article.
pub fn reader_page_lines(page: ReaderPage, directory: &PageDirectory, width: usize) -> Vec<Line> {
    match page {
        ReaderPage::Article => Vec::new(),
        ReaderPage::Index => sommaire_page(directory, width),
        ReaderPage::Guide => guide_page(width),
        ReaderPage::EndOfEdition => vec![end_line(width)],
    }
}

/// Section headings: FRONT PAGE yellow, THIS WEEK cyan, THE WIRE green, on black.
pub fn section_color(section: Section) -> Color {
    match section {
        Section::Front => Color::Yellow,
        Section::Week => Color::Cyan,
        Section::Wire => Color::Green,
    }
}

/// The entry column of P104. While the edition builds, only its progress; offline or failed,
/// why there is none; else the header, then each section with entries (heading, entries in
/// rank order, each feed's overflow row under its last entry), then `FIN DE L'EDITION` once
/// every entry is read. Every row is exactly `cells` wide.
pub fn journal_page<Tz: TimeZone>(
    view: Option<&JournalView>,
    phase: &Phase,
    selected: Option<i64>,
    cells: usize,
    tz: &Tz,
) -> Vec<Line>
where
    Tz::Offset: Display,
{
    match phase {
        Phase::Building { done, total } => {
            return message(&progress_text(*done, *total), Color::Yellow, false, cells);
        }
        Phase::Offline => {
            let mut lines = message("OFFLINE: NO EDITION YET", Color::Red, true, cells);
            lines.push(Line::new(
                "retry",
                LineRole::Message,
                CellLine::centred("RÉPÉTITION: RETRY", cells, Color::Cyan),
            ));
            return lines;
        }
        Phase::Failed(reason) => {
            return message(&reason.to_uppercase(), Color::Red, true, cells);
        }
        Phase::Idle | Phase::Due => {}
    }
    let Some(view) = view else {
        return message(&progress_text(0, 0), Color::Yellow, false, cells);
    };
    let mut lines = journal_header_lines(view, cells, tz);
    if view.items.is_empty() {
        lines.push(blank_line("blank-empty", cells));
        lines.push(Line::new(
            "empty",
            LineRole::Message,
            empty_line(journal::EMPTY_LINE, cells),
        ));
        return lines;
    }
    let mut items: Vec<&JournalItem> = view.items.iter().collect();
    items.sort_by_key(|item| item.rank);
    for section in journal::SECTIONS {
        let entries: Vec<&JournalItem> = items
            .iter()
            .copied()
            .filter(|i| i.section == section)
            .collect();
        let overflow: Vec<&EditionOverflowRow> = view
            .overflow
            .iter()
            .filter(|o| o.section == section)
            .collect();
        if entries.is_empty() && overflow.is_empty() {
            continue;
        }
        lines.push(blank_line(format!("blank-{}", section.as_str()), cells));
        lines.push(journal_heading_line(section, entries.len(), cells));
        let mut placed: HashSet<FeedId> = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            lines.extend(journal_entry_lines(
                entry,
                Some(entry.row.entry_id) == selected,
                cells,
            ));
            let last_of_feed = !entries[index + 1..]
                .iter()
                .any(|later| later.row.feed_id == entry.row.feed_id);
            if last_of_feed
                && let Some(more) = overflow.iter().find(|o| o.feed_id == entry.row.feed_id)
            {
                placed.insert(more.feed_id);
                lines.push(overflow_line(more, cells));
            }
        }
        // A cap of 0 (`edition build --cap-wire 0`) leaves overflow with no entry above it.
        for more in overflow.iter().filter(|o| !placed.contains(&o.feed_id)) {
            lines.push(overflow_line(more, cells));
        }
    }
    if journal::is_finished(view) {
        lines.push(blank_line("blank-end", cells));
        lines.push(end_line(cells));
    }
    lines
}

/// `FRI 16 OCT` in cyan, ` 12/43 READ` in yellow at the right edge; a second row
/// `3 FEEDS UNREACHABLE` in red when some feeds failed.
pub fn journal_header_lines<Tz: TimeZone>(view: &JournalView, cells: usize, tz: &Tz) -> Vec<Line>
where
    Tz::Offset: Display,
{
    let day = date(view.edition.built_at, tz, DatePattern::Day);
    let count = format!(" {}/{} READ", journal::read_count(view), view.items.len());
    let count_cells = len(&count).min(cells);
    let mut lines = vec![Line::new(
        "header",
        LineRole::Header,
        CellLine::new(vec![
            CellSpan::new(&format!(" {day}"), wide(cells - count_cells), Color::Cyan),
            CellSpan::new(&count, wide(count_cells), Color::Yellow),
        ]),
    )];
    let unreachable = view.edition.unreachable;
    if unreachable > 0 {
        let text = if unreachable == 1 {
            "1 FEED UNREACHABLE".to_string()
        } else {
            format!("{unreachable} FEEDS UNREACHABLE")
        };
        lines.push(Line::new(
            "unreachable",
            LineRole::Header,
            CellLine::new(vec![CellSpan::new(
                &format!(" {text}"),
                wide(cells),
                Color::Red,
            )]),
        ));
    }
    lines
}

/// ` FRONT PAGE ... 2`, in the section's colour on black.
pub fn journal_heading_line(section: Section, count: usize, cells: usize) -> Line {
    let color = section_color(section);
    let number = format!(" {count}");
    let number_cells = len(&number).min(cells);
    Line::new(
        format!("heading-{}", section.as_str()),
        LineRole::Heading(section.as_str().to_string()),
        CellLine::new(vec![
            CellSpan::new(
                &format!(" {}", journal::section_title(section).to_uppercase()),
                wide(cells - number_cells),
                color,
            ),
            CellSpan::new(&number, wide(number_cells), color),
        ]),
    )
}

/// As the entries column: a yellow `●` when unread, the title over two rows at most, a white
/// `*` when starred, markers on the first row only; black on white when selected.
pub fn journal_entry_lines(item: &JournalItem, selected: bool, cells: usize) -> Vec<Line> {
    entry_lines(
        item.row.title.as_deref(),
        item.row.is_read,
        item.row.is_starred,
        cells,
        selected,
    )
    .into_iter()
    .enumerate()
    .map(|(index, cells)| {
        let mut line = Line::new(
            format!("entry-{}-{index}", item.row.entry_id),
            LineRole::Entry(item.row.entry_id),
            cells,
        );
        line.selected = selected;
        line
    })
    .collect()
}

/// `  +12 MORE IN ACTUALITTÉ`: white on blue (blue never colours text), two cells in, under
/// the entry titles.
pub fn overflow_line(row: &EditionOverflowRow, cells: usize) -> Line {
    let indent = cells.min(2);
    Line::new(
        format!("overflow-{}-{}", row.section.as_str(), row.feed_id),
        LineRole::Overflow {
            feed: row.feed_id,
            page: row.page,
        },
        CellLine::new(vec![
            CellSpan::new("", wide(indent), Color::White),
            CellSpan::new(
                &journal::overflow_label(row).to_uppercase(),
                wide(cells - indent),
                Color::White,
            )
            .on(Color::Blue),
        ]),
    )
}

/// The edition's last entry by rank: SUITE past it lands on FIN DE L'EDITION.
pub fn last_entry_id(view: Option<&JournalView>) -> Option<i64> {
    view?
        .items
        .iter()
        .max_by_key(|item| item.rank)
        .map(|item| item.row.entry_id)
}

/// A blank row, then one centred row: `color` on black, or black on `color` when `reversed`.
fn message(text: &str, color: Color, reversed: bool, width: usize) -> Vec<Line> {
    let mut line = CellLine::centred(text, width, color);
    if reversed {
        line = line.reversed(Color::Black, color);
    }
    vec![
        blank_line("blank-message", width),
        Line::new("message", LineRole::Message, line),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    use ressac_core::store::SourceCounts;

    use crate::test_db::{Paris, journal_item, journal_view, paged_snapshot};

    fn t(span: &CellSpan) -> String {
        span.text()
    }

    fn all_spans(line: &CellLine, f: impl Fn(&CellSpan) -> bool) -> bool {
        line.spans.iter().all(f)
    }

    fn directory() -> PageDirectory {
        PageDirectory::new(&paged_snapshot())
    }

    const CELLS: usize = 28;

    // SommairePageTests

    #[test]
    fn pages_run_down_the_left_column_then_the_right() {
        let d = directory();
        let rows = d.rows();
        let pairs = sommaire_pairs(rows);
        assert_eq!(
            pairs
                .iter()
                .map(|(l, _)| rows[*l].number)
                .collect::<Vec<_>>(),
            [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800]
        );
        assert_eq!(
            pairs
                .iter()
                .map(|(_, r)| r.map(|r| rows[r].number))
                .collect::<Vec<_>>(),
            [
                Some(801),
                Some(802),
                Some(899),
                Some(200),
                Some(201),
                Some(400),
                Some(401),
                Some(402),
                Some(403),
                Some(110),
                None
            ]
        );
        assert!(sommaire_pairs(&[]).is_empty());
        assert_eq!(sommaire_pairs(&rows[..2]), [(0, Some(1))]);
    }

    #[test]
    fn every_row_fills_the_reader_width() {
        let d = directory();
        for width in [0, 1, 2, 9, 40, 41, 80] {
            let half = half_width(width);
            assert!(width - 2 * half >= width.min(1));
            for row in d.rows() {
                assert_eq!(sommaire_line(row, half, &d).width(), half);
            }
            assert_eq!(sommaire_title(width).width(), width);
            assert!(sommaire_logo(width).iter().all(|l| l.width() == width));
            for line in sommaire_page(&d, width) {
                assert_eq!(line.cells.width(), width, "{}", line.id);
            }
        }
        assert_eq!(half_width(0), 0);
    }

    #[test]
    fn reserved_pages_are_in_colour() {
        let mut snapshot = paged_snapshot();
        snapshot.counts = SourceCounts {
            journal: 43,
            ..snapshot.counts
        };
        let d = PageDirectory::new(&snapshot);
        let line = |number: u16| {
            let row = d.rows().iter().find(|r| r.number == number).unwrap();
            sommaire_line(row, 20, &d)
        };
        assert_eq!(line(101).spans[1].fg, Color::Yellow);
        let journal = line(104);
        assert!(journal.spans[1].fg == Color::Yellow && t(&journal.spans[2]) == " 43");
        for rhythm in 105..=109 {
            assert_eq!(line(rhythm).spans[1].fg, Color::Yellow);
        }
        let triage = line(800);
        assert!(triage.spans[1].fg == Color::Magenta && triage.spans[2].fg == Color::Red);
        assert_eq!(line(200).spans[1].fg, Color::Green);
        assert_eq!(line(201).spans[1].fg, Color::White);
    }

    #[test]
    fn the_title_is_centred_and_the_wave_is_unbroken() {
        let title = sommaire_title(20);
        assert_eq!(t(&title.spans[0]), "       RESSAC       ");
        assert_eq!(title.spans[0].fg, Color::Yellow);
        let logo = sommaire_logo(16);
        let top: Vec<char> = t(&logo[0].spans[0]).chars().collect();
        let bottom: Vec<char> = t(&logo[1].spans[0]).chars().collect();
        assert_eq!(top.iter().collect::<String>(), " ▄▀▀▄    ▄▀▀▄   ");
        assert_eq!(bottom.iter().collect::<String>(), "▀    ▀▄▄▀    ▀▄▄");
        for column in 0..16 {
            assert!(
                (top[column] == ' ') != (bottom[column] == ' '),
                "column {column} holds one block"
            );
        }
        assert!(logo.iter().all(|l| all_spans(l, |s| s.fg == Color::Cyan)));
    }

    #[test]
    fn the_sommaire_page_is_a_title_a_logo_a_blank_and_the_pairs() {
        let d = directory();
        let lines = sommaire_page(&d, 40);
        assert_eq!(lines.len(), 4 + 11);
        assert!(lines[0].double_height && lines[0].role == LineRole::Title);
        assert_eq!(lines[3].role, LineRole::Blank);
        assert_eq!(
            lines[4].role,
            LineRole::Pair {
                left: 100,
                right: Some(801),
                half: 19
            }
        );
        assert_eq!(lines[4].action(), "pair:100:801:19");
        assert_eq!(lines.last().unwrap().action(), "pair:800:-:19");
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
    }

    // GuidePageTests

    #[test]
    fn every_guide_line_fills_the_width() {
        for width in [0, 5, 12, 30, 60] {
            let lines = guide_lines(width);
            let expected: usize = GUIDE_SECTIONS.iter().map(|s| 2 + s.keys.len()).sum();
            assert_eq!(lines.len(), expected);
            assert!(lines.iter().all(|l| l.width() == width));
            let page = guide_page(width);
            assert_eq!(page.len(), expected + 1);
            assert!(page.iter().all(|l| l.cells.width() == width));
        }
    }

    #[test]
    fn every_key_of_the_window_is_listed() {
        let keys: HashSet<&str> = GUIDE_SECTIONS
            .iter()
            .flat_map(|s| s.keys.iter().map(|k| k.keys))
            .collect();
        for key in [
            "0-9",
            "BACKSPACE",
            "ESC",
            "ALT S",
            "ALT G  ?",
            "ALT R",
            "I",
            "SPACE  J",
            "K",
            "M",
            "S",
            "LEFT RIGHT",
            "UP DOWN",
            "R",
            "B",
            "U",
            "O",
            "W",
            "SHIFT R",
            "CTRL F",
            "CTRL R  F5",
        ] {
            assert!(keys.contains(key), "{key}");
        }
        assert_eq!(
            GUIDE_SECTIONS.map(|s| s.title),
            ["PAGES", "READING", "COLUMNS", "GARDEN", "MENUS"]
        );
        assert!(
            GUIDE_SECTIONS
                .iter()
                .flat_map(|s| s.keys)
                .all(|k| k.keys.len() + 2 <= GUIDE_KEY_CELLS)
        );
    }

    #[test]
    fn headings_keys_and_actions_are_coloured() {
        let lines = guide_lines(40);
        assert_eq!(lines[0], CellLine::blank(40));
        assert!(t(&lines[1].spans[0]).starts_with(" PAGES"));
        assert!(lines[1].spans[0].fg == Color::Black && lines[1].spans[0].bg == Color::Green);
        assert_eq!(t(&lines[2].spans[0]), " 0-9          ");
        assert_eq!(lines[2].spans[0].cells(), 14);
        assert_eq!(lines[2].spans[0].fg, Color::Cyan);
        assert_eq!(lines[2].spans[1].fg, Color::White);
        assert!(lines.iter().all(|l| all_spans(l, |s| s.fg != Color::Blue)));
    }

    /// Bedstead draws every character of P100 and P899: no magenta `?` on a help page.
    #[test]
    fn the_help_pages_draw_without_a_missing_glyph() {
        let mut lines = guide_lines(60);
        lines.extend(sommaire_logo(16));
        lines.push(sommaire_title(20));
        lines.push(end_line(40).cells);
        assert!(lines.iter().all(|l| {
            l.spans
                .iter()
                .all(|s| s.runs.iter().all(|r| !r.replacement))
        }));
    }

    // JournalPageTests

    fn page(view: &JournalView, selected: Option<i64>) -> Vec<Line> {
        journal_page(Some(view), &Phase::Idle, selected, CELLS, &Paris)
    }

    #[test]
    fn sections_come_in_order_with_their_entries_and_overflow() {
        let view = journal_view(&[], 0, None);
        let lines = page(&view, None);
        let roles: Vec<LineRole> = lines.iter().map(|l| l.role.clone()).collect();
        assert_eq!(
            roles,
            [
                LineRole::Header,
                LineRole::Blank,
                LineRole::Heading("front".into()),
                LineRole::Entry(1),
                LineRole::Blank,
                LineRole::Heading("week".into()),
                LineRole::Entry(2),
                LineRole::Entry(3),
                LineRole::Blank,
                LineRole::Heading("wire".into()),
                LineRole::Entry(4),
                LineRole::Overflow {
                    feed: 10,
                    page: Some(412)
                },
            ]
        );
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
        let header = &lines[0].cells;
        assert!(t(&header.spans[0]).trim() == "FRI 16 OCT" && header.spans[0].fg == Color::Cyan);
        assert!(t(&header.spans[1]) == " 0/4 READ" && header.spans[1].fg == Color::Yellow);
        let heading = |i: usize| {
            (
                t(&lines[i].cells.spans[0]).trim().to_string(),
                t(&lines[i].cells.spans[1]),
            )
        };
        assert_eq!(heading(2), ("FRONT PAGE".into(), " 1".into()));
        assert_eq!(heading(5), ("THIS WEEK".into(), " 2".into()));
        assert_eq!(heading(9), ("THE WIRE".into(), " 1".into()));
        assert_eq!(lines[2].id, "heading-front");
        assert_eq!(lines[3].id, "entry-1-0");
        assert_eq!(lines[11].id, "overflow-wire-10");
        assert_eq!(lines[11].action(), "overflow:10:412");
    }

    #[test]
    fn headings_are_coloured_and_blue_never_colours_text() {
        let view = journal_view(&[], 0, None);
        let lines = page(&view, None);
        assert_eq!(section_color(Section::Front), Color::Yellow);
        assert_eq!(section_color(Section::Week), Color::Cyan);
        assert_eq!(section_color(Section::Wire), Color::Green);
        for line in &lines {
            if let LineRole::Heading(id) = &line.role {
                let section = journal::SECTIONS
                    .into_iter()
                    .find(|s| s.as_str() == id)
                    .unwrap();
                assert!(all_spans(&line.cells, |s| s.fg == section_color(section)
                    && s.bg == Color::Black));
            }
        }
        let overflow = lines.last().unwrap();
        assert!(
            overflow.cells.spans[1].fg == Color::White && overflow.cells.spans[1].bg == Color::Blue
        );
        assert!(
            !lines
                .iter()
                .flat_map(|l| &l.cells.spans)
                .any(|s| s.fg == Color::Blue)
        );
    }

    #[test]
    fn entries_show_unread_and_starred_markers_and_the_selection() {
        let view = journal_view(&[3], 0, None);
        let lines = page(&view, Some(2));
        let two = lines.iter().find(|l| l.role == LineRole::Entry(2)).unwrap();
        assert!(two.selected);
        assert!(all_spans(&two.cells, |s| s.fg == Color::Black && s.bg == Color::White));
        assert!(t(&two.cells.spans[0]) == "● " && t(&two.cells.spans[2]) == " *");
        let three = lines.iter().find(|l| l.role == LineRole::Entry(3)).unwrap();
        assert!(!three.selected && t(&three.cells.spans[0]) == "  ");
        assert_eq!(three.action(), "entry:3");
    }

    #[test]
    fn an_overflow_row_is_exactly_the_column_width() {
        let awkward = EditionOverflowRow {
            section: Section::Wire,
            feed_id: 5,
            feed_title: "Café ☕ Pédago".into(),
            page: None,
            hidden_count: 3,
        };
        let line = overflow_line(&awkward, CELLS);
        assert_eq!(line.cells.width(), CELLS);
        assert_eq!(line.text(), "  +3 MORE IN CAFÉ ? PÉDAGO  ");
        assert!(
            line.cells.spans[1]
                .runs
                .iter()
                .any(|r| r.replacement && r.text == "?")
        );
        assert_eq!(
            line.role,
            LineRole::Overflow {
                feed: 5,
                page: None
            }
        );
        let long = EditionOverflowRow {
            section: Section::Week,
            feed_id: 6,
            feed_title: "東京".repeat(30),
            page: Some(300),
            hidden_count: 120,
        };
        assert_eq!(overflow_line(&long, CELLS).cells.width(), CELLS);
    }

    #[test]
    fn every_journal_row_is_exactly_the_width_asked() {
        let mut items = journal_view(&[], 0, None).items;
        items[0].row.title = Some(
            "Café 🎉 東京 e\u{301}cole, a title far longer than two rows of the column".into(),
        );
        items[1].row.title = None;
        let view = journal_view(&[], 3, Some(items));
        for width in [0, 1, 2, 9, 28, 40] {
            for phase in [
                Phase::Idle,
                Phase::Building {
                    done: 34,
                    total: 156,
                },
                Phase::Offline,
                Phase::Failed("Database is locked.".into()),
            ] {
                for line in journal_page(Some(&view), &phase, Some(1), width, &Paris) {
                    assert_eq!(line.cells.width(), width, "{} at {width}", line.id);
                }
            }
        }
    }

    #[test]
    fn the_end_line_comes_once_every_entry_is_read() {
        let reading = journal_view(&[1, 2, 3], 0, None);
        assert!(!page(&reading, None).iter().any(|l| l.role == LineRole::End));
        let view = journal_view(&[1, 2, 3, 4], 0, None);
        let done = page(&view, None);
        let end = done.last().unwrap();
        assert!(end.role == LineRole::End && end.double_height);
        assert_eq!(end.text().trim(), "FIN DE L'EDITION");
        assert_eq!(end.cells.spans[0].fg, Color::Yellow);
        assert_eq!(end.id, "end");
        assert_eq!(done[done.len() - 2].id, "blank-end");
        assert_eq!(t(&done[0].cells.spans[1]), " 4/4 READ");
    }

    #[test]
    fn empty_building_offline_and_unreachable() {
        let empty_view = journal_view(&[], 0, Some(vec![]));
        let empty = journal_page(Some(&empty_view), &Phase::Idle, None, CELLS, &Paris);
        assert_eq!(
            empty.iter().map(|l| l.role.clone()).collect::<Vec<_>>(),
            [LineRole::Header, LineRole::Blank, LineRole::Message]
        );
        assert_eq!(empty[2].text().trim(), "NOTHING NEW THIS MORNING.");
        assert_eq!(empty[1].id, "blank-empty");
        assert_eq!(empty[2].id, "empty");
        assert!(!empty.iter().any(|l| l.role == LineRole::End));

        let view = journal_view(&[], 0, None);
        let building = journal_page(
            Some(&view),
            &Phase::Building {
                done: 34,
                total: 156,
            },
            None,
            CELLS,
            &Paris,
        );
        assert_eq!(
            building.iter().map(|l| l.role.clone()).collect::<Vec<_>>(),
            [LineRole::Blank, LineRole::Message]
        );
        assert_eq!(building[1].text().trim(), "EDITION IN PROGRESS 34/156");
        assert_eq!(building[0].id, "blank-message");
        let none = journal_page(None, &Phase::Due, None, CELLS, &Paris);
        assert_eq!(none[1].text().trim(), "EDITION IN PROGRESS");

        let offline = journal_page(None, &Phase::Offline, None, CELLS, &Paris);
        assert_eq!(
            offline.iter().map(|l| l.role.clone()).collect::<Vec<_>>(),
            [LineRole::Blank, LineRole::Message, LineRole::Message]
        );
        assert_eq!(offline[1].text().trim(), "OFFLINE: NO EDITION YET");
        assert!(all_spans(&offline[1].cells, |s| s.fg == Color::Black
            && s.bg == Color::Red));
        assert_eq!(offline[2].text().trim(), "RÉPÉTITION: RETRY");
        assert_eq!(offline[2].cells.spans[0].fg, Color::Cyan);
        assert_eq!(offline[2].id, "retry");
        let failed = journal_page(
            None,
            &Phase::Failed("Database is locked.".into()),
            None,
            CELLS,
            &Paris,
        );
        assert_eq!(failed[1].text().trim(), "DATABASE IS LOCKED.");

        let three = journal_header_lines(&journal_view(&[], 3, None), CELLS, &Paris);
        assert!(three.len() == 2 && three[1].text().trim() == "3 FEEDS UNREACHABLE");
        assert_eq!(three[1].cells.spans[0].fg, Color::Red);
        assert_eq!(three[1].id, "unreachable");
        let one = journal_header_lines(&journal_view(&[], 1, None), CELLS, &Paris);
        assert_eq!(one[1].text().trim(), "1 FEED UNREACHABLE");
        assert_eq!(
            journal_header_lines(&journal_view(&[], 0, None), CELLS, &Paris).len(),
            1
        );
    }

    #[test]
    fn overflow_without_a_placed_entry_closes_its_section() {
        let mut view = journal_view(&[], 0, None);
        view.overflow.push(EditionOverflowRow {
            section: Section::Week,
            feed_id: 99,
            feed_title: "Capped out".into(),
            page: None,
            hidden_count: 4,
        });
        let roles: Vec<LineRole> = page(&view, None).iter().map(|l| l.role.clone()).collect();
        let week = roles
            .iter()
            .position(|r| *r == LineRole::Heading("week".into()))
            .unwrap();
        assert_eq!(
            roles[week..week + 4],
            [
                LineRole::Heading("week".into()),
                LineRole::Entry(2),
                LineRole::Entry(3),
                LineRole::Overflow {
                    feed: 99,
                    page: None
                },
            ]
        );
        assert_eq!(last_entry_id(Some(&view)), Some(4));
        assert_eq!(last_entry_id(None), None);
    }

    #[test]
    fn a_feeds_overflow_follows_its_last_entry_of_the_section() {
        // Two runs of one feed in a section: the overflow row follows the feed's last entry.
        let items = vec![
            journal_item(1, Section::Wire, 1, 10, "Actualité", Some("One"), false),
            journal_item(2, Section::Wire, 2, 11, "Other", Some("Two"), false),
            journal_item(3, Section::Wire, 3, 10, "Actualité", Some("Three"), false),
        ];
        let view = journal_view(&[], 0, Some(items));
        let lines = page(&view, None);
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
        let roles: Vec<LineRole> = lines.iter().map(|l| l.role.clone()).collect();
        assert_eq!(
            roles[roles.len() - 2..],
            [
                LineRole::Entry(3),
                LineRole::Overflow {
                    feed: 10,
                    page: Some(412)
                }
            ]
        );
        assert_eq!(
            roles
                .iter()
                .filter(|r| matches!(r, LineRole::Overflow { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn the_end_page_is_double_height() {
        let d = directory();
        let lines = reader_page_lines(ReaderPage::EndOfEdition, &d, 40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].double_height && lines[0].text().trim() == "FIN DE L'EDITION");
        assert!(reader_page_lines(ReaderPage::Article, &d, 40).is_empty());
        assert_eq!(
            reader_page_lines(ReaderPage::Index, &d, 40),
            sommaire_page(&d, 40)
        );
        assert_eq!(reader_page_lines(ReaderPage::Guide, &d, 40), guide_page(40));
        assert!(guide_page(40)[0].double_height);
        assert_eq!(guide_page(40)[0].text().trim(), "GUIDE");
    }
}
