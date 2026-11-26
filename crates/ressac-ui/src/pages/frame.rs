//! The frame of the Antiope window as cell rows (the Mac's `PageHeader.line`,
//! `PageIndexColumn.line`, `EntryPageColumn`, `ColumnTitle`, `MinitelKey`, `MinitelBar`,
//! `SearchPrompt.label`, `GardenPage.bannerRow`, `AntiopeReaderLines`): header, column titles,
//! index and entry rows, the Minitel bar and the banner. Pure: every width in cells, the clock
//! and the zone come in as arguments. Blue never colours text.

use std::fmt::Display;

use chrono::{DateTime, TimeZone, Utc};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    AntiopeColumn, DirRow, INDEX_CELLS, Line, LineRole, PageDirectory, PageEntry, ReservedPage,
    len, wide,
};
use crate::entries::display_title;
use crate::journal::Phase;
use crate::runs::RunProgress;
use crate::teletext::{CellLine, CellSpan, Color, DatePattern, crt, date};

/// What the header shows in place of the current page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderLabel {
    /// The page on screen; `None` while the selection has no number yet (`P---`).
    Page(Option<u16>),
    /// The digits being typed, `"4__"`.
    Typing(String),
    /// An unknown page, in red for two seconds.
    NotFound(u16),
    /// The counter rolling during a refresh, as a TV searches for a page.
    Searching(u16),
}

/// Typing first, then `NOT FOUND`, then the rolling counter, then the page.
pub fn header_label(
    entry: &PageEntry,
    not_found: Option<u16>,
    current: Option<u16>,
    rolling: Option<u16>,
) -> HeaderLabel {
    if entry.is_active() {
        return HeaderLabel::Typing(entry.display());
    }
    if let Some(number) = not_found {
        return HeaderLabel::NotFound(number);
    }
    if let Some(number) = rolling {
        return HeaderLabel::Searching(number);
    }
    HeaderLabel::Page(current)
}

/// 100 to 899, one step every tenth of a second since the Unix epoch.
pub fn rolling_page(now: DateTime<Utc>) -> u16 {
    let tenths = now.timestamp_millis().div_euclid(100);
    100 + u16::try_from(tenths.rem_euclid(800)).unwrap_or(0)
}

/// The page in yellow (red when not found) and `RESSAC` in white on the left; the date in cyan
/// and the time in yellow at the right edge. Exactly `width` cells: when the window is narrow,
/// the time keeps its cells first, then the date, then the page. `right_margin` blank cells
/// close the row, inside `width` (never more than a quarter of it, and never at the time's
/// expense).
pub fn header_line<Tz: TimeZone>(
    label: &HeaderLabel,
    now: DateTime<Utc>,
    tz: &Tz,
    width: usize,
    right_margin: usize,
) -> CellLine
where
    Tz::Offset: Display,
{
    let day = format!("{} ", date(now, tz, DatePattern::Day));
    let time = date(now, tz, DatePattern::Time);
    let (page, color, shows_name) = match label {
        HeaderLabel::Page(number) => (
            format!("P{}", number.map_or("---".to_string(), |n| n.to_string())),
            Color::Yellow,
            true,
        ),
        HeaderLabel::Typing(digits) => (format!("P{digits}"), Color::Yellow, true),
        HeaderLabel::Searching(number) => (format!("P{number}"), Color::Yellow, true),
        HeaderLabel::NotFound(number) => (format!("P{number} NOT FOUND"), Color::Red, false),
    };
    let margin = right_margin
        .min(width / 4)
        .min(width.saturating_sub(len(&time)));
    let mut remaining = width - margin;
    let mut take = |wanted: usize| {
        let taken = wanted.min(remaining);
        remaining -= taken;
        taken
    };
    let time_cells = take(len(&time));
    let day_cells = take(len(&day));
    let page_cells = take(len(&page) + 1);
    let name_cells = remaining;
    let mut spans = vec![
        CellSpan::new(&page, wide(page_cells), color),
        CellSpan::new(
            if shows_name { "RESSAC" } else { "" },
            wide(name_cells),
            Color::White,
        ),
        CellSpan::new(&day, wide(day_cells), Color::Cyan),
        CellSpan::new(&time, wide(time_cells), Color::Yellow),
    ];
    if margin > 0 {
        spans.push(CellSpan::new("", wide(margin), Color::White));
    }
    CellLine::new(spans)
}

/// Blank cells kept at the right of the header. The CRT screen paints the tube's corner over
/// the top right of the window and the bleed draws up to `BLEED_OFFSET + GLOW_RADIUS` beyond a
/// glyph: with the screen on the margin covers both, rounded up to whole cells; with it off
/// nothing covers the corner, so the time stays flush with the edge.
pub fn right_margin_cells(
    window_width: f64,
    window_height: f64,
    cell_width: f64,
    crt_on: bool,
) -> usize {
    if !crt_on || cell_width <= 0.0 {
        return 0;
    }
    let reach =
        crt::corner_radius(window_width, window_height) + crt::BLEED_OFFSET + crt::GLOW_RADIUS;
    (reach / cell_width).ceil() as usize
}

pub fn column_label(column: AntiopeColumn) -> &'static str {
    match column {
        AntiopeColumn::Index => "INDEX",
        AntiopeColumn::Entries => "ENTRIES",
        AntiopeColumn::Reader => "READER",
    }
}

/// The one-row title at the top of each column: the focused column's in reverse video (black
/// on cyan), the others cyan on black, exactly as wide as the column. `unread_only` is `None`
/// where the filter does not apply (a garden page, the other columns); the entries title then
/// shows `ENTRIES  ALL` or `ENTRIES  UNREAD`.
pub fn column_title(
    column: AntiopeColumn,
    active: bool,
    width: usize,
    unread_only: Option<bool>,
    blank_when_inactive: bool,
) -> CellLine {
    if blank_when_inactive && !active {
        return CellLine::new(vec![CellSpan::new("", wide(width), Color::Cyan)]);
    }
    let mut label = column_label(column).to_string();
    if let Some(unread) = unread_only {
        label.push_str(if unread { "  UNREAD" } else { "  ALL" });
    }
    let line = CellLine::new(vec![CellSpan::new(
        &format!(" {label}"),
        wide(width),
        Color::Cyan,
    )]);
    if active {
        line.reversed(Color::Black, Color::Cyan)
    } else {
        line
    }
}

/// `101 Unread         1083`: the number in cyan, the title indented by depth, the count in
/// yellow at the right edge. Root folders in green capitals, the garden in magenta with P800's
/// triage count in red; P100 colours the other reserved titles yellow (`highlights_reserved`).
/// Selected: black on cyan. Always exactly `width` cells.
pub fn index_line(
    row: &DirRow,
    width: usize,
    root_folder: bool,
    selected: bool,
    highlights_reserved: bool,
) -> CellLine {
    let reserved = row
        .reserved
        .then(|| ReservedPage::from_number(row.number))
        .flatten();
    let count = if row.unread > 0 {
        row.unread.min(99_999).to_string()
    } else {
        String::new()
    };
    let number_cells = width.min(4);
    let count_cells = if count.is_empty() {
        0
    } else {
        (count.len() + 1).min(width - number_cells)
    };
    let title_cells = width - number_cells - count_cells;
    let indent = " ".repeat(row.depth.min(3));
    let title = if root_folder {
        row.title.to_uppercase()
    } else {
        row.title.clone()
    };
    let title_color = if reserved.is_some_and(ReservedPage::is_garden) {
        Color::Magenta
    } else if reserved.is_some() && highlights_reserved {
        Color::Yellow
    } else if root_folder {
        Color::Green
    } else {
        Color::White
    };
    let count_color = if reserved == Some(ReservedPage::NeedsTriage) {
        Color::Red
    } else {
        Color::Yellow
    };
    let line = CellLine::new(vec![
        CellSpan::new(&format!("{} ", row.number), wide(number_cells), Color::Cyan),
        CellSpan::new(&format!("{indent}{title}"), wide(title_cells), title_color),
        CellSpan::new(
            &if count.is_empty() {
                String::new()
            } else {
                format!(" {count}")
            },
            wide(count_cells),
            count_color,
        ),
    ]);
    if selected {
        line.reversed(Color::Black, Color::Cyan)
    } else {
        line
    }
}

/// Every page of the directory as a clickable row (`page-<n>`, role `Jump`), 22 cells wide;
/// the current page's row in reverse video.
pub fn index_rows(directory: &PageDirectory, current: Option<u16>) -> Vec<Line> {
    directory
        .rows()
        .iter()
        .map(|row| {
            let selected = Some(row.number) == current;
            let cells = index_line(
                row,
                INDEX_CELLS,
                directory.is_root_folder(row),
                selected,
                false,
            );
            let mut line = Line::new(
                format!("page-{}", row.number),
                LineRole::Jump(row.number),
                cells,
            );
            line.selected = selected;
            line
        })
        .collect()
}

/// The title over two rows at most, words separated by single spaces: the first row ends at
/// the last word that fits (a first word longer than the row is cut), the second takes the
/// rest, which the span truncates. Graphemes, not bytes.
pub fn wrap_title(title: &str, width: usize) -> Vec<String> {
    let words: Vec<&str> = title.split_whitespace().collect();
    let text = words.join(" ");
    if width == 0 || len(&text) <= width {
        return vec![text];
    }
    let mut first = String::new();
    let mut used = 0;
    for word in &words {
        let candidate = if first.is_empty() {
            (*word).to_string()
        } else {
            format!("{first} {word}")
        };
        if len(&candidate) > width {
            break;
        }
        first = candidate;
        used += 1;
    }
    if first.is_empty() {
        let head: String = text.graphemes(true).take(width).collect();
        let tail: String = text.graphemes(true).skip(width).collect();
        return vec![head, tail];
    }
    vec![first, words[used..].join(" ")]
}

/// One or two rows: a yellow `●` when unread, the title wrapped over two rows at most, a white
/// `*` when starred, the markers on the first row only. Selected: black on white. Every row is
/// exactly `width` cells.
pub fn entry_lines(
    title: Option<&str>,
    read: bool,
    starred: bool,
    width: usize,
    selected: bool,
) -> Vec<CellLine> {
    let marker_cells = width.min(2);
    let title_cells = width.saturating_sub(4);
    let star_cells = width - marker_cells - title_cells;
    wrap_title(&display_title(title), title_cells)
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let line = CellLine::new(vec![
                CellSpan::new(
                    if index == 0 && !read { "●" } else { "" },
                    wide(marker_cells),
                    Color::Yellow,
                ),
                CellSpan::new(text, wide(title_cells), Color::White),
                CellSpan::new(
                    if index == 0 && starred { " *" } else { "" },
                    wide(star_cells),
                    Color::White,
                ),
            ]);
            if selected {
                line.reversed(Color::Black, Color::White)
            } else {
                line
            }
        })
        .collect()
}

/// The message in capitals, one cyan row, centred.
pub fn empty_line(message: &str, width: usize) -> CellLine {
    CellLine::centred(&message.to_uppercase(), width, Color::Cyan)
}

/// Nothing selected, for instance after typing a page number.
pub fn no_entry_line(width: usize) -> CellLine {
    CellLine::centred("NO ENTRY SELECTED", width, Color::Cyan)
}

/// The run-failure notice: reverse video, black on red, one row of exactly `width` cells.
pub fn failure_notice_line(notice: &str, width: usize) -> CellLine {
    CellLine::new(vec![
        CellSpan::new(&format!(" {notice}"), wide(width), Color::Black).on(Color::Red),
    ])
}

/// A key of the Minitel bar: its French label and the verb the QML turns into a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinitelKey {
    Sommaire,
    Suite,
    Retour,
    Guide,
    Repetition,
}

impl MinitelKey {
    pub const ALL: [MinitelKey; 5] = [
        MinitelKey::Sommaire,
        MinitelKey::Suite,
        MinitelKey::Retour,
        MinitelKey::Guide,
        MinitelKey::Repetition,
    ];

    /// The device's own key names, the one exception to English in the interface.
    pub fn label(self) -> &'static str {
        match self {
            MinitelKey::Sommaire => "SOMMAIRE",
            MinitelKey::Suite => "SUITE",
            MinitelKey::Retour => "RETOUR",
            MinitelKey::Guide => "GUIDE",
            MinitelKey::Repetition => "RÉPÉTITION",
        }
    }

    /// The keyboard key's command, so that a click and a key do one thing.
    pub fn verb(self) -> &'static str {
        match self {
            MinitelKey::Sommaire => "sommaire",
            MinitelKey::Suite => "advance",
            MinitelKey::Retour => "previousEntry",
            MinitelKey::Guide => "guide",
            MinitelKey::Repetition => "repetition",
        }
    }

    /// ` SOMMAIRE `, black on cyan.
    pub fn line(self) -> CellLine {
        let label = self.label();
        CellLine::new(vec![
            CellSpan::new(&format!(" {label} "), wide(len(label) + 2), Color::Black)
                .on(Color::Cyan),
        ])
    }
}

/// Each key with its two padding cells and the black cell after it.
pub const KEYS_CELLS: usize = 49;

/// The text right of the keys and its colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub text: String,
    pub color: Color,
}

impl Hint {
    fn new(text: impl Into<String>, color: Color) -> Hint {
        Hint {
            text: text.into(),
            color,
        }
    }
}

/// The search in force first, then the keys of the garden or of the focused column.
pub fn hint(column: AntiopeColumn, is_garden: bool, search_text: &str) -> Hint {
    if !search_text.is_empty() {
        return Hint::new(format!("SEARCH: {search_text}"), Color::Cyan);
    }
    if is_garden {
        return Hint::new("k keep · r repair · b bury · u undo", Color::White);
    }
    Hint::new(
        match column {
            AntiopeColumn::Index => "type a page number · ? guide",
            AntiopeColumn::Entries => "j next · k previous · m read",
            AntiopeColumn::Reader => "m read · s star · i images",
        },
        Color::White,
    )
}

/// `EDITION IN PROGRESS 34/156`, or without the count while the total is unknown.
pub fn progress_text(done: usize, total: usize) -> String {
    if total > 0 {
        format!("EDITION IN PROGRESS {done}/{total}")
    } else {
        "EDITION IN PROGRESS".to_string()
    }
}

/// What replaces the hints: the garden's last refusal, else `EDITION IN PROGRESS 34/156` while
/// the morning edition builds, else `REFRESHING 42/155` while a run is going, all in yellow.
/// `None` keeps the hints.
pub fn status(
    progress: Option<&RunProgress>,
    message: Option<&str>,
    journal: &Phase,
) -> Option<Hint> {
    if let Some(message) = message {
        return Some(Hint::new(message, Color::Yellow));
    }
    if let Phase::Building { done, total } = journal {
        return Some(Hint::new(progress_text(*done, *total), Color::Yellow));
    }
    let progress = progress?;
    Some(Hint::new(
        if progress.total > 0 {
            format!("REFRESHING {}/{}", progress.done, progress.total)
        } else {
            "REFRESHING".to_string()
        },
        Color::Yellow,
    ))
}

/// The hint after one blank cell, exactly `width` cells.
pub fn hint_line(hint: &Hint, width: usize) -> CellLine {
    CellLine::new(vec![CellSpan::new(
        &format!(" {}", hint.text),
        wide(width),
        hint.color,
    )])
}

/// `SEARCH? ` in the Minitel bar, opened by Ctrl+F.
pub const SEARCH_LABEL: &str = "SEARCH? ";

/// ` SEARCH? ` in cyan, `min(9, width)` cells; the field itself is the QML's.
pub fn search_label_line(width: usize) -> CellLine {
    CellLine::new(vec![CellSpan::new(
        &format!(" {SEARCH_LABEL}"),
        wide(width.min(SEARCH_LABEL.len() + 1)),
        Color::Cyan,
    )])
}

/// The run's banner as one red reverse-video row: the message in white, then the clickable
/// keys in yellow, all on red, exactly the window's width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerRow {
    pub message: CellLine,
    /// ` LOG `, when the log explains the failure.
    pub log: Option<CellLine>,
    /// ` X `, to dismiss.
    pub dismiss: CellLine,
}

impl BannerRow {
    pub fn width(&self) -> usize {
        self.message.width() + self.log.as_ref().map_or(0, CellLine::width) + self.dismiss.width()
    }
}

pub fn banner_row(message: &str, show_log: bool, cells_wide: usize) -> BannerRow {
    let key = |label: &str| {
        CellLine::new(vec![
            CellSpan::new(label, wide(len(label)), Color::Yellow).on(Color::Red),
        ])
    };
    let log = show_log.then(|| key(" LOG "));
    let dismiss = key(" X ");
    let message_cells = cells_wide
        .saturating_sub(log.as_ref().map_or(0, CellLine::width))
        .saturating_sub(dismiss.width());
    BannerRow {
        message: CellLine::new(vec![
            CellSpan::new(&format!(" {message}"), wide(message_cells), Color::White).on(Color::Red),
        ]),
        log,
        dismiss,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;

    use crate::runs::RunProgress;
    use crate::test_db::{Paris, paged_snapshot};

    fn t(span: &CellSpan) -> String {
        span.text()
    }

    const AWKWARD: [&str; 10] = [
        "Café 🎉 東京",
        "e\u{301}cole de\u{301}ja\u{300} la\u{300}",
        "👩🏽‍💻👨‍👩‍👧‍👦🇫🇷🏳️‍🌈",
        "Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words Long title with words ",
        "Supercalifragilisticexpialidociously-long-hyphenated",
        "",
        "   ",
        "Tab\tand\nnewline",
        "مرحبا بالعالم",
        "Ελληνικά και Русский",
    ];

    fn row(number: u16, title: &str, unread: usize, reserved: bool, depth: usize) -> DirRow {
        DirRow {
            number,
            title: title.to_string(),
            unread,
            reserved,
            depth,
        }
    }

    fn all_spans(line: &CellLine, f: impl Fn(&CellSpan) -> bool) -> bool {
        line.spans.iter().all(f)
    }

    /// 2026-10-14 07:47:12 UTC, 09:47:12 in Paris.
    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_791_964_032, 0).unwrap()
    }

    // IndexColumnTests

    #[test]
    fn every_index_row_is_exactly_22_cells() {
        let directory = PageDirectory::new(&paged_snapshot());
        let mut rows = directory.rows().to_vec();
        rows.extend(AWKWARD.iter().map(|title| row(412, title, 3, false, 1)));
        rows.push(row(413, "Huge", 1_234_567, false, 0));
        rows.push(row(414, "Deep", 0, false, 9));
        for r in &rows {
            for selected in [false, true] {
                for highlights in [false, true] {
                    let line = index_line(
                        r,
                        INDEX_CELLS,
                        directory.is_root_folder(r),
                        selected,
                        highlights,
                    );
                    assert_eq!(line.width(), 22, "{}", r.title);
                }
            }
        }
        for width in [0, 1, 3, 4, 8, 11, 30] {
            let r = row(101, "Unread", 1083, true, 0);
            assert_eq!(index_line(&r, width, false, false, false).width(), width);
        }
    }

    #[test]
    fn index_colours_follow_the_spec() {
        let directory = PageDirectory::new(&paged_snapshot());
        let unread = index_line(&row(101, "Unread", 1083, true, 0), 22, false, false, false);
        assert_eq!(
            unread.spans.iter().map(t).collect::<Vec<_>>(),
            ["101 ", "Unread       ", " 1083"]
        );
        assert_eq!(
            unread.spans.iter().map(|s| s.fg).collect::<Vec<_>>(),
            [Color::Cyan, Color::White, Color::Yellow]
        );

        let infos = directory.rows().iter().find(|r| r.number == 400).unwrap();
        assert!(directory.is_root_folder(infos));
        let folder = index_line(infos, 22, true, false, false);
        assert!(t(&folder.spans[1]).starts_with("INFOS") && folder.spans[1].fg == Color::Green);

        let science = directory.rows().iter().find(|r| r.number == 402).unwrap();
        assert!(!directory.is_root_folder(science));
        let nested = index_line(science, 22, false, false, false);
        assert!(t(&nested.spans[1]).starts_with(" Science") && nested.spans[1].fg == Color::White);

        let garden = index_line(
            &row(800, "Needs triage", 7, true, 0),
            22,
            false,
            false,
            false,
        );
        assert_eq!(
            garden.spans.iter().map(|s| s.fg).collect::<Vec<_>>(),
            [Color::Cyan, Color::Magenta, Color::Red]
        );
        let cemetery = index_line(&row(802, "Cemetery", 0, true, 0), 22, false, false, false);
        assert!(cemetery.spans[1].fg == Color::Magenta && cemetery.spans[2].cells() == 0);

        let selected = index_line(&row(101, "Unread", 1083, true, 0), 22, false, true, false);
        assert!(all_spans(&selected, |s| s.fg == Color::Black && s.bg == Color::Cyan));
    }

    #[test]
    fn the_sommaire_colours_the_other_reserved_titles() {
        let today = index_line(&row(102, "Today", 4, true, 0), 20, false, false, true);
        assert_eq!(today.spans[1].fg, Color::Yellow);
        let triage = index_line(
            &row(800, "Needs triage", 7, true, 0),
            20,
            false,
            false,
            true,
        );
        assert_eq!(triage.spans[1].fg, Color::Magenta);
        let feed = index_line(
            &row(401, "Daring Fireball", 2, false, 1),
            20,
            false,
            false,
            true,
        );
        assert_eq!(feed.spans[1].fg, Color::White);
    }

    // EntryColumnTests

    #[test]
    fn every_entry_row_is_exactly_28_cells() {
        let titles: Vec<Option<&str>> = AWKWARD.iter().map(|t| Some(*t)).chain([None]).collect();
        for title in titles {
            for read in [false, true] {
                for starred in [false, true] {
                    for selected in [false, true] {
                        let lines = entry_lines(title, read, starred, 28, selected);
                        assert!((1..=2).contains(&lines.len()), "{title:?}");
                        assert!(lines.iter().all(|l| l.width() == 28), "{title:?}");
                    }
                }
            }
        }
        for width in [0, 1, 3, 4, 10] {
            assert!(
                entry_lines(Some("Café 🎉 東京"), false, false, width, false)
                    .iter()
                    .all(|l| l.width() == width)
            );
        }
    }

    #[test]
    fn markers_colours_and_reverse_video() {
        let title = "Apple announces something rather long today";
        let unread = entry_lines(Some(title), false, true, 28, false);
        assert_eq!(unread.len(), 2);
        assert!(t(&unread[0].spans[0]) == "● " && unread[0].spans[0].fg == Color::Yellow);
        assert_eq!(
            t(&unread[0].spans[1]),
            format!("Apple announces {}", " ".repeat(8))
        );
        assert!(t(&unread[0].spans[2]) == " *" && unread[0].spans[2].fg == Color::White);
        assert!(t(&unread[1].spans[0]) == "  " && t(&unread[1].spans[2]) == "  ");
        assert!(t(&unread[1].spans[1]).starts_with("something rather long"));

        let read = entry_lines(Some("Short"), true, false, 28, false);
        assert!(read.len() == 1 && t(&read[0].spans[0]) == "  " && t(&read[0].spans[2]) == "  ");

        let selected = entry_lines(Some(title), false, false, 28, true);
        assert!(
            selected
                .iter()
                .all(|l| all_spans(l, |s| s.fg == Color::Black && s.bg == Color::White))
        );

        let untitled = entry_lines(None, true, false, 28, false);
        assert!(t(&untitled[0].spans[1]).starts_with("Untitled"));
    }

    #[test]
    fn titles_wrap_on_words_over_two_rows_at_most() {
        assert_eq!(wrap_title("Short title", 24), ["Short title"]);
        assert_eq!(wrap_title("  spaced   out  ", 24), ["spaced out"]);
        assert_eq!(wrap_title("", 24), [""]);
        assert_eq!(
            wrap_title("The quick brown fox jumps over the lazy dog", 24),
            ["The quick brown fox", "jumps over the lazy dog"]
        );
        assert_eq!(
            wrap_title("Supercalifragilisticexpialidocious and more", 24),
            ["Supercalifragilisticexpi", "alidocious and more"]
        );
        assert_eq!(
            wrap_title("Café 🎉 東京 ensemble", 10),
            ["Café 🎉 東京", "ensemble"]
        );
        assert_eq!(wrap_title("anything", 0), ["anything"]);
    }

    // ColumnTitleTests

    const COLUMNS: [AntiopeColumn; 3] = [
        AntiopeColumn::Index,
        AntiopeColumn::Entries,
        AntiopeColumn::Reader,
    ];

    #[test]
    fn every_title_is_exactly_as_wide_as_its_column() {
        for column in COLUMNS {
            for active in [false, true] {
                for width in [0, 1, 5, 22, 28, 31, 120] {
                    assert_eq!(
                        column_title(column, active, width, None, false).width(),
                        width
                    );
                }
            }
        }
        assert_eq!(COLUMNS.map(column_label), ["INDEX", "ENTRIES", "READER"]);
    }

    #[test]
    fn only_the_active_column_is_in_reverse_video() {
        for column in COLUMNS {
            let active = column_title(column, true, 22, None, false);
            assert!(all_spans(&active, |s| s.fg == Color::Black && s.bg == Color::Cyan));
            let inactive = column_title(column, false, 22, None, false);
            assert!(all_spans(&inactive, |s| s.fg == Color::Cyan && s.bg == Color::Black));
            assert!(t(&inactive.spans[0]).starts_with(&format!(" {}", column_label(column))));
        }
    }

    #[test]
    fn the_entries_title_shows_the_filter() {
        let all = column_title(AntiopeColumn::Entries, false, 28, Some(false), false);
        assert_eq!(t(&all.spans[0]), format!(" ENTRIES  ALL{}", " ".repeat(15)));
        let unread = column_title(AntiopeColumn::Entries, true, 28, Some(true), false);
        assert!(t(&unread.spans[0]).starts_with(" ENTRIES  UNREAD") && unread.width() == 28);
        assert!(all_spans(&unread, |s| s.fg == Color::Black && s.bg == Color::Cyan));
        let plain = column_title(AntiopeColumn::Entries, false, 28, None, false);
        assert!(t(&plain.spans[0]).starts_with(" ENTRIES "));
        assert!(!t(&plain.spans[0]).contains("ALL"));
    }

    #[test]
    fn the_heading_pages_blank_the_reader_title_unless_active() {
        for width in [0, 24, 31, 80] {
            let blank = column_title(AntiopeColumn::Reader, false, width, None, true);
            let active = column_title(AntiopeColumn::Reader, true, width, None, true);
            assert!(blank.width() == width && active.width() == width);
            assert!(blank.text().chars().all(|c| c == ' '));
        }
        let active = column_title(AntiopeColumn::Reader, true, 30, None, true);
        assert!(t(&active.spans[0]).starts_with(" READER"));
        assert!(all_spans(&active, |s| s.fg == Color::Black && s.bg == Color::Cyan));
        let plain = column_title(AntiopeColumn::Reader, false, 30, None, false);
        assert!(t(&plain.spans[0]).starts_with(" READER"));
    }

    #[test]
    fn blue_never_colours_a_title() {
        for column in COLUMNS {
            for active in [false, true] {
                for filter in [None, Some(false), Some(true)] {
                    let line = column_title(column, active, 22, filter, false);
                    assert!(all_spans(&line, |s| s.fg != Color::Blue));
                }
            }
        }
    }

    // MinitelBarTests

    #[test]
    fn the_keys_are_the_minitels_in_reverse_video() {
        assert_eq!(
            MinitelKey::ALL.map(MinitelKey::label),
            ["SOMMAIRE", "SUITE", "RETOUR", "GUIDE", "RÉPÉTITION"]
        );
        assert_eq!(
            MinitelKey::ALL.map(MinitelKey::verb),
            [
                "sommaire",
                "advance",
                "previousEntry",
                "guide",
                "repetition"
            ]
        );
        for key in MinitelKey::ALL {
            let line = key.line();
            assert_eq!(line.width(), key.label().chars().count() + 2);
            assert_eq!(t(&line.spans[0]), format!(" {} ", key.label()));
            assert!(all_spans(&line, |s| s.fg == Color::Black && s.bg == Color::Cyan));
        }
        let total: usize = MinitelKey::ALL
            .iter()
            .map(|k| k.label().chars().count() + 3)
            .sum();
        assert_eq!(total, KEYS_CELLS);
        assert_eq!(KEYS_CELLS, 49);
    }

    #[test]
    fn hints_follow_the_search_the_garden_and_the_focus() {
        assert_eq!(
            hint(AntiopeColumn::Entries, true, "tidal"),
            Hint {
                text: "SEARCH: tidal".into(),
                color: Color::Cyan
            }
        );
        assert_eq!(
            hint(AntiopeColumn::Entries, true, ""),
            Hint {
                text: "k keep · r repair · b bury · u undo".into(),
                color: Color::White
            }
        );
        assert_eq!(
            hint(AntiopeColumn::Index, false, "").text,
            "type a page number · ? guide"
        );
        assert_eq!(
            hint(AntiopeColumn::Entries, false, "").text,
            "j next · k previous · m read"
        );
        assert_eq!(
            hint(AntiopeColumn::Reader, false, "").text,
            "m read · s star · i images"
        );
    }

    #[test]
    fn the_hint_line_fills_the_rest_of_the_bar() {
        let h = Hint {
            text: "m read · s star · i images".into(),
            color: Color::White,
        };
        for width in [0, 10, 26, 80] {
            assert_eq!(hint_line(&h, width).width(), width);
        }
        assert!(t(&hint_line(&h, 30).spans[0]).starts_with(" m read"));
    }

    #[test]
    fn the_minitel_bar_shows_refusals_then_the_edition_then_runs_then_hints() {
        let run = RunProgress {
            done: 42,
            total: 155,
            label: "Fetching feeds".into(),
        };
        let idle = Phase::Idle;
        assert_eq!(
            status(Some(&run), None, &idle),
            Some(Hint {
                text: "REFRESHING 42/155".into(),
                color: Color::Yellow
            })
        );
        let empty = RunProgress {
            done: 0,
            total: 0,
            label: "Fetching feeds".into(),
        };
        assert_eq!(
            status(Some(&empty), None, &idle).unwrap().text,
            "REFRESHING"
        );
        assert_eq!(
            status(Some(&run), Some("Nothing to repair for this feed."), &idle)
                .unwrap()
                .text,
            "Nothing to repair for this feed."
        );
        let building = Phase::Building {
            done: 34,
            total: 156,
        };
        assert_eq!(
            status(Some(&run), None, &building),
            Some(Hint {
                text: "EDITION IN PROGRESS 34/156".into(),
                color: Color::Yellow
            })
        );
        assert_eq!(status(None, None, &idle), None);
        assert_eq!(progress_text(0, 0), "EDITION IN PROGRESS");
        assert_eq!(
            hint_line(&status(Some(&run), None, &idle).unwrap(), 30).width(),
            30
        );
    }

    // PageHeaderTests

    #[test]
    fn label_precedence() {
        let mut typing = PageEntry::default();
        typing.type_digit(4);
        assert_eq!(
            header_label(&typing, Some(437), Some(412), Some(300)),
            HeaderLabel::Typing("4__".into())
        );
        let idle = PageEntry::default();
        assert_eq!(
            header_label(&idle, Some(437), Some(412), Some(300)),
            HeaderLabel::NotFound(437)
        );
        assert_eq!(
            header_label(&idle, None, Some(412), Some(300)),
            HeaderLabel::Searching(300)
        );
        assert_eq!(
            header_label(&idle, None, Some(412), None),
            HeaderLabel::Page(Some(412))
        );
        assert_eq!(
            header_label(&idle, None, None, None),
            HeaderLabel::Page(None)
        );
    }

    #[test]
    fn the_header_fills_the_width_with_page_name_date_and_time() {
        let line = header_line(&HeaderLabel::Page(Some(412)), at(), &Paris, 40, 0);
        assert_eq!(line.width(), 40);
        assert_eq!(
            line.spans.iter().map(t).collect::<Vec<_>>(),
            ["P412 ", "RESSAC          ", "WED 14 OCT ", "09:47:12"]
        );
        assert_eq!(
            line.spans.iter().map(|s| s.fg).collect::<Vec<_>>(),
            [Color::Yellow, Color::White, Color::Cyan, Color::Yellow]
        );
        for width in [0, 5, 8, 19, 24, 75, 160] {
            for label in [HeaderLabel::NotFound(437), HeaderLabel::Page(Some(412))] {
                assert_eq!(header_line(&label, at(), &Paris, width, 0).width(), width);
            }
        }
        let narrow = header_line(&HeaderLabel::Page(Some(412)), at(), &Paris, 8, 0);
        assert_eq!(t(&narrow.spans[3]), "09:47:12");
    }

    #[test]
    fn the_time_stays_clear_of_the_tube_corner_at_every_size() {
        let label = HeaderLabel::Page(Some(412));
        for size in [15.0, 20.0, 25.0, 30.0] {
            let (cell_width, _) = crate::teletext::cell_size(size);
            for (w, h) in [(700.0, 500.0), (1000.0, 700.0), (1600.0, 1000.0)] {
                for crt_on in [false, true] {
                    let width = (w / cell_width) as usize;
                    let margin = right_margin_cells(w, h, cell_width, crt_on);
                    let line = header_line(&label, at(), &Paris, width, margin);
                    assert_eq!(line.width(), width, "size {size}");
                    assert_eq!(t(&line.spans[3]), "09:47:12", "size {size}");
                    let ends_at = line.spans[..4].iter().map(CellSpan::cells).sum::<usize>() as f64
                        * cell_width;
                    if crt_on {
                        let reach = crt::corner_radius(w, h) + crt::BLEED_OFFSET + crt::GLOW_RADIUS;
                        assert!(ends_at <= w - reach, "size {size}, window {w}x{h}");
                        assert!(
                            margin > 0
                                && line.spans.len() == 5
                                && t(&line.spans[4]).chars().all(|c| c == ' ')
                        );
                    } else {
                        assert!(
                            margin == 0
                                && line.spans.len() == 4
                                && ends_at == width as f64 * cell_width
                        );
                    }
                }
            }
        }
        let narrow = header_line(&label, at(), &Paris, 8, 9);
        assert!(t(&narrow.spans[3]) == "09:47:12" && narrow.width() == 8);
    }

    #[test]
    fn typing_not_found_and_no_page() {
        let typing = header_line(&HeaderLabel::Typing("41_".into()), at(), &Paris, 40, 0);
        assert!(t(&typing.spans[0]) == "P41_ " && typing.spans[0].fg == Color::Yellow);
        let not_found = header_line(&HeaderLabel::NotFound(437), at(), &Paris, 40, 0);
        assert!(t(&not_found.spans[0]) == "P437 NOT FOUND " && not_found.spans[0].fg == Color::Red);
        assert!(t(&not_found.spans[1]).chars().all(|c| c == ' '));
        let none = header_line(&HeaderLabel::Page(None), at(), &Paris, 40, 0);
        assert_eq!(t(&none.spans[0]), "P--- ");
    }

    #[test]
    fn the_counter_rolls_through_every_page() {
        let start = DateTime::from_timestamp_millis(1_791_964_032_050).unwrap();
        let first = rolling_page(start);
        let next = rolling_page(start + chrono::TimeDelta::milliseconds(100));
        assert_eq!(next, (first - 100 + 1) % 800 + 100);
        for step in 0..1_600 {
            let page = rolling_page(start + chrono::TimeDelta::milliseconds(step * 100));
            assert!((100..=899).contains(&page));
        }
        let before = DateTime::from_timestamp_millis(-12_340).unwrap();
        assert!((100..=899).contains(&rolling_page(before)));
        let wrap = DateTime::from_timestamp_millis(79_900).unwrap();
        assert_eq!(rolling_page(wrap), 899);
        assert_eq!(
            rolling_page(wrap + chrono::TimeDelta::milliseconds(100)),
            100
        );
    }

    #[test]
    fn blue_never_colours_text() {
        let directory = PageDirectory::new(&paged_snapshot());
        let mut lines: Vec<CellLine> = Vec::new();
        for r in directory.rows() {
            for selected in [false, true] {
                for highlights in [false, true] {
                    lines.push(index_line(
                        r,
                        22,
                        directory.is_root_folder(r),
                        selected,
                        highlights,
                    ));
                }
            }
        }
        for read in [false, true] {
            for selected in [false, true] {
                lines.extend(entry_lines(Some("A title"), read, true, 28, selected));
            }
        }
        for label in [
            HeaderLabel::Page(Some(412)),
            HeaderLabel::Page(None),
            HeaderLabel::Typing("4__".into()),
            HeaderLabel::NotFound(437),
            HeaderLabel::Searching(300),
        ] {
            lines.push(header_line(&label, at(), &Paris, 60, 0));
        }
        lines.extend(MinitelKey::ALL.map(MinitelKey::line));
        for column in COLUMNS {
            for garden in [false, true] {
                for search in ["", "tidal"] {
                    lines.push(hint_line(&hint(column, garden, search), 30));
                }
            }
        }
        lines.push(empty_line("No unread entries", 28));
        lines.push(no_entry_line(40));
        lines.push(search_label_line(30));
        assert!(lines.iter().all(|l| all_spans(l, |s| s.fg != Color::Blue)));
    }

    // Empty state, notices, search label, banner

    #[test]
    fn the_empty_message_is_one_centred_cyan_line() {
        let line = empty_line("No unread entries", 28);
        assert_eq!(line.width(), 28);
        assert_eq!(t(&line.spans[0]), "     NO UNREAD ENTRIES      ");
        assert_eq!(line.spans[0].fg, Color::Cyan);
        assert_eq!(
            empty_line("A message far longer than the twenty-eight cells", 28).width(),
            28
        );
        for width in [0, 10, 17, 40, 41] {
            let none = no_entry_line(width);
            assert_eq!(none.width(), width);
            assert!(all_spans(&none, |s| s.fg == Color::Cyan && s.bg == Color::Black));
        }
        assert_eq!(no_entry_line(40).text().trim(), "NO ENTRY SELECTED");
        for width in [0, 10, 40, 80] {
            let notice = failure_notice_line("Could not fetch the page.", width);
            assert_eq!(notice.width(), width);
            assert!(all_spans(&notice, |s| s.fg == Color::Black && s.bg == Color::Red));
        }
        assert!(failure_notice_line("Gone", 10).text().starts_with(" Gone"));
    }

    #[test]
    fn the_banner_row_fills_the_window_width() {
        let row = banner_row(
            "ressac exited with code 1: the database is locked.",
            true,
            60,
        );
        assert_eq!(row.width(), 60);
        assert!(all_spans(&row.message, |s| s.fg == Color::White && s.bg == Color::Red));
        for key in [row.log.as_ref().unwrap(), &row.dismiss] {
            assert!(all_spans(key, |s| s.fg == Color::Yellow && s.bg == Color::Red));
        }
        assert_eq!(row.log.as_ref().unwrap().text(), " LOG ");
        assert_eq!(row.dismiss.text(), " X ");
        assert!(t(&row.message.spans[0]).starts_with(" ressac exited with code 1"));
        let short = banner_row(
            "The network looks unreachable: 12 of 186 feeds could not connect.",
            false,
            40,
        );
        assert!(short.log.is_none());
        assert_eq!(short.width(), 40);
    }

    #[test]
    fn the_search_label_asks_as_a_terminal_would() {
        assert_eq!(SEARCH_LABEL, "SEARCH? ");
        assert_eq!(search_label_line(30).text(), " SEARCH? ");
        assert_eq!(search_label_line(4).width(), 4);
        assert_eq!(search_label_line(30).spans[0].fg, Color::Cyan);
    }

    #[test]
    fn index_rows_reverse_the_current_page() {
        let directory = PageDirectory::new(&paged_snapshot());
        let rows = index_rows(&directory, Some(401));
        assert_eq!(rows.len(), directory.rows().len());
        for line in &rows {
            assert_eq!(line.cells.width(), 22);
            let reversed = line.id == "page-401";
            assert_eq!(line.selected, reversed);
            assert_eq!(
                all_spans(&line.cells, |s| s.fg == Color::Black && s.bg == Color::Cyan),
                reversed
            );
            let number: u16 = line.id.trim_start_matches("page-").parse().unwrap();
            assert_eq!(line.action(), format!("jump:{number}"));
        }
        assert!(index_rows(&directory, None).iter().all(|l| !l.selected));
    }
}
