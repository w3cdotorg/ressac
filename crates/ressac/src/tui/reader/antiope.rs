//! Antiope in the terminal (spec §8): the eight teletext colours on black, rows exactly as wide
//! as their column, the page header, the Minitel bar, P100 SOMMAIRE and P899 GUIDE, then (9b)
//! the pages of the reader. Pure functions over plain data, and `render`.
//!
//! The garden pages (P800 to P802) are drawn by the Native look until phase 9b gives them a
//! teletext page: `render` hands them to `native::render`, so that the garden's keys, which stay
//! live under `t`, always act on a row that is on screen.

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::images::ImageStore;
use super::layout::{Column, LayoutClass, Look, columns};
use super::native::{self, EXTRACT_PROGRESS_LABEL, ReaderMetrics, short_date};
use super::pages::{DirRow, PageDirectory, ReaderPage};
use super::state::{RHYTHM_CHOICES, ReaderState, ReaderView, group_label};
use crate::output::printable;
use crate::tui::reader::journal::BUILDING;
use ressac_core::store::pages::PageKind;

/// The teletext palette (Antiope spec §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tt {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Tt {
    #[cfg(test)]
    pub const ALL: [Tt; 8] = [
        Tt::Black,
        Tt::Red,
        Tt::Green,
        Tt::Yellow,
        Tt::Blue,
        Tt::Magenta,
        Tt::Cyan,
        Tt::White,
    ];

    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Tt::Black => (0x00, 0x00, 0x00),
            Tt::Red => (0xFF, 0x00, 0x00),
            Tt::Green => (0x00, 0xFF, 0x00),
            Tt::Yellow => (0xFF, 0xFF, 0x00),
            Tt::Blue => (0x00, 0x00, 0xFF),
            Tt::Magenta => (0xFF, 0x00, 0xFF),
            Tt::Cyan => (0x00, 0xFF, 0xFF),
            Tt::White => (0xFF, 0xFF, 0xFF),
        }
    }

    pub fn color(self) -> Color {
        let (r, g, b) = self.rgb();
        Color::Rgb(r, g, b)
    }
}

/// Block elements (U+2580 to U+259F): the only glyphs blue may colour.
pub fn blue_allowed(text: &str) -> bool {
    text.chars()
        .all(|c| c == ' ' || ('\u{2580}'..='\u{259F}').contains(&c))
}

/// `text` cut or padded to exactly `cells` display cells. A wide character that would cross the
/// limit becomes a space, so the row keeps its width.
pub fn fit(text: &str, cells: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = Span::raw(c.to_string()).width();
        if used + w > cells {
            break;
        }
        out.push(c);
        used += w;
    }
    out.extend(std::iter::repeat_n(' ', cells - used));
    out
}

/// One run of cells: control characters neutralised, fitted, coloured.
pub fn span(text: &str, cells: usize, fg: Tt, bg: Tt) -> Span<'static> {
    let clean = printable(text);
    let text = fit(&clean, cells);
    debug_assert!(
        fg != Tt::Blue || blue_allowed(&text),
        "blue never colours text"
    );
    Span::styled(text, Style::new().fg(fg.color()).bg(bg.color()))
}

pub fn row(spans: Vec<Span<'static>>) -> Line<'static> {
    Line::from(spans)
}

/// `text` centred in `width` cells (one more on the right when the margin is odd).
pub fn centred(text: &str, width: usize, fg: Tt, bg: Tt) -> Line<'static> {
    let w = Span::raw(text.to_string()).width().min(width);
    let left = (width - w) / 2;
    row(vec![
        span("", left, fg, bg),
        span(text, width - left, fg, bg),
    ])
}

pub fn blank(width: usize) -> Line<'static> {
    row(vec![span("", width, Tt::White, Tt::Black)])
}

/// The terminal's stand-in for double height (spec §8): a full-width black-on-yellow row.
pub fn title_bar(text: &str, width: usize) -> Line<'static> {
    centred(text, width, Tt::Black, Tt::Yellow)
}

/// What the header shows left of `RESSAC` (Antiope spec §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderLabel {
    Page(Option<u16>),
    /// The digits typed so far, `4__`.
    #[allow(dead_code, reason = "phase 9b types a page number")]
    Typing(String),
    #[allow(dead_code, reason = "phase 9b reports an unknown page")]
    NotFound(u16),
    /// The counter rolling while a job runs.
    Searching(u16),
}

/// 100 to 899, one step every tenth of a second.
pub fn rolling_page(now: DateTime<Utc>) -> u16 {
    let tenths = now.timestamp_millis().div_euclid(100);
    u16::try_from(100 + tenths.rem_euclid(800)).unwrap_or(100)
}

/// `P412 RESSAC … FRI 16 OCT 18:33:12`: the page yellow (red when not found), `RESSAC` white,
/// the date cyan, the time yellow. When narrow, the time keeps its cells first, then the date,
/// then the page.
pub fn header_line(
    label: &HeaderLabel,
    local: DateTime<FixedOffset>,
    width: usize,
) -> Line<'static> {
    let day = local.format("%a %d %b ").to_string().to_uppercase();
    let time = local.format("%H:%M:%S").to_string();
    let (page, colour, name) = match label {
        HeaderLabel::Page(p) => (
            format!("P{}", p.map_or_else(|| "---".into(), |p| p.to_string())),
            Tt::Yellow,
            true,
        ),
        HeaderLabel::Typing(d) => (format!("P{d}"), Tt::Yellow, true),
        HeaderLabel::Searching(n) => (format!("P{n}"), Tt::Yellow, true),
        HeaderLabel::NotFound(n) => (format!("P{n} NOT FOUND"), Tt::Red, false),
    };
    let mut remaining = width;
    let mut take = |wanted: usize| {
        let cells = wanted.min(remaining);
        remaining -= cells;
        cells
    };
    let time_cells = take(time.len());
    let day_cells = take(day.len());
    let page_cells = take(page.chars().count() + 1);
    let name_cells = take(usize::MAX);
    row(vec![
        span(&page, page_cells, colour, Tt::Black),
        span(
            if name { "RESSAC" } else { "" },
            name_cells,
            Tt::White,
            Tt::Black,
        ),
        span(&day, day_cells, Tt::Cyan, Tt::Black),
        span(&time, time_cells, Tt::Yellow, Tt::Black),
    ])
}

/// The text right of the Minitel keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinitelHint {
    pub text: String,
    pub colour: Tt,
    /// A status, a prompt or a message: it must be read whole, so on a narrow bar it replaces
    /// the keys instead of being cut beside them.
    pub urgent: bool,
}

impl MinitelHint {
    /// The search in force first, then the garden's keys or the focused column's.
    pub fn keys(column: Column, is_garden: bool, search: &str) -> Self {
        let (text, colour) = if !search.is_empty() {
            (format!("SEARCH: {search}"), Tt::Cyan)
        } else if is_garden {
            ("K KEEP · R REPAIR · B BURY · U UNDO".to_string(), Tt::White)
        } else {
            let t = match column {
                Column::Sources => "TYPE A PAGE NUMBER · ? GUIDE",
                Column::Entries => "J NEXT · K PREVIOUS · M READ",
                Column::Reader => "M READ · S STAR · I IMAGES · O LINKS",
            };
            (t.to_string(), Tt::White)
        };
        MinitelHint {
            text,
            colour,
            urgent: false,
        }
    }

    /// While a job runs: `EDITION IN PROGRESS 34/156` for the edition, `FETCHING ARTICLE` for a
    /// full-text extraction, else `REFRESHING 42/155`, in yellow. `None` keeps the hints.
    pub fn status(progress: Option<(&str, usize, usize)>, edition: bool) -> Option<Self> {
        let (label, done, total) = progress?;
        let text = if edition {
            counted("EDITION IN PROGRESS", done, total)
        } else if label == EXTRACT_PROGRESS_LABEL {
            "FETCHING ARTICLE".to_string()
        } else {
            counted("REFRESHING", done, total)
        };
        Some(MinitelHint {
            text,
            colour: Tt::Yellow,
            urgent: true,
        })
    }

    fn urgent(text: String, colour: Tt) -> Self {
        MinitelHint {
            text,
            colour,
            urgent: true,
        }
    }
}

fn counted(head: &str, done: usize, total: usize) -> String {
    if total > 0 {
        format!("{head} {done}/{total}")
    } else {
        head.to_string()
    }
}

pub const MINITEL_KEYS: [&str; 5] = ["SOMMAIRE", "SUITE", "RETOUR", "GUIDE", "RÉPÉTITION"];

/// Cells the five keys take, each with its two padding cells and the black cell after it.
fn keys_cells() -> usize {
    MINITEL_KEYS.iter().map(|k| Span::raw(*k).width() + 3).sum()
}

/// ` SOMMAIRE ` … black on cyan, a black cell after each, then the hint. Exactly `width` cells.
/// An urgent hint that would not fit beside the keys takes the whole bar.
pub fn minitel_bar(hint: &MinitelHint, width: usize) -> Line<'static> {
    let hint_cells = Span::raw(hint.text.as_str()).width() + 1;
    if hint.urgent && keys_cells() + hint_cells > width {
        return row(vec![span(
            &format!(" {}", words_within(&hint.text, width.saturating_sub(1))),
            width,
            hint.colour,
            Tt::Black,
        )]);
    }
    let mut spans = Vec::new();
    let mut remaining = width;
    for key in MINITEL_KEYS {
        let label = format!(" {key} ");
        let cells = Span::raw(label.as_str()).width().min(remaining);
        spans.push(span(&label, cells, Tt::Black, Tt::Cyan));
        remaining -= cells;
        let gap = 1.min(remaining);
        spans.push(span("", gap, Tt::White, Tt::Black));
        remaining -= gap;
    }
    spans.push(span(
        &format!(" {}", words_within(&hint.text, remaining.saturating_sub(1))),
        remaining,
        hint.colour,
        Tt::Black,
    ));
    row(spans)
}

/// `text` cut to at most `cells` display cells: whole ` · ` segments when it has any and one
/// fits, else at a word. One word wider than `cells` is cut where it must.
pub fn words_within(text: &str, cells: usize) -> String {
    if Span::raw(text).width() <= cells {
        return text.to_string();
    }
    let mut kept = String::new();
    for segment in text.split(" · ") {
        let next = if kept.is_empty() {
            segment.to_string()
        } else {
            format!("{kept} · {segment}")
        };
        if Span::raw(next.as_str()).width() > cells {
            break;
        }
        kept = next;
    }
    if !kept.is_empty() {
        return kept;
    }
    let cut = fit(text, cells);
    let whole = text[cut.trim_end().len()..].starts_with(' ');
    match cut.trim_end().rfind(' ') {
        Some(i) if !whole => cut[..i].trim_end().to_string(),
        _ => cut.trim_end().to_string(),
    }
}

/// What the Minitel bar says: the rhythm menu's keys, the search prompt, a message, a running job
/// (all urgent), else the hints of the focused column.
fn bar_hint(state: &ReaderState, view: &ReaderView<'_>) -> MinitelHint {
    if view.rhythm.is_some() {
        return MinitelHint::urgent("UP DOWN CHOOSE · ENTER SET · ESC CLOSE".into(), Tt::White);
    }
    if state.search_editing {
        let text = state.search.as_deref().unwrap_or("");
        return MinitelHint::urgent(format!("SEARCH? {text}_"), Tt::Cyan);
    }
    if let Some(message) = view.message {
        return MinitelHint::urgent(printable(message).to_uppercase(), Tt::Yellow);
    }
    let edition = view.progress.is_some_and(|(label, _, _)| label == BUILDING);
    MinitelHint::status(view.progress, edition).unwrap_or_else(|| {
        MinitelHint::keys(view.focus, false, state.search.as_deref().unwrap_or(""))
    })
}

/// `101 Unread … 1083`: the number cyan, the title indented by depth, the count yellow at the
/// right edge. Root folders in green capitals, garden pages magenta with P800's count red,
/// reserved titles yellow on P100 (`highlight_reserved`). Selected: black on cyan.
pub fn index_line(
    r: &DirRow,
    width: usize,
    selected: bool,
    highlight_reserved: bool,
) -> Line<'static> {
    let count = r
        .count
        .filter(|c| *c > 0)
        .map(|c| format!(" {}", c.min(99_999)))
        .unwrap_or_default();
    let number_cells = 4.min(width);
    let count_cells = count.len().min(width - number_cells);
    let title_cells = width - number_cells - count_cells;
    let garden = matches!(r.page, 800..=802);
    let title_colour = if garden {
        Tt::Magenta
    } else if r.kind == PageKind::Reserved && highlight_reserved {
        Tt::Yellow
    } else if r.is_root_folder {
        Tt::Green
    } else {
        Tt::White
    };
    let title = format!(
        "{}{}",
        " ".repeat(r.depth.min(3)),
        if r.is_root_folder {
            r.title.to_uppercase()
        } else {
            r.title.clone()
        }
    );
    let count_colour = if r.page == 800 { Tt::Red } else { Tt::Yellow };
    let (reversed, bg) = if selected {
        (Some(Tt::Black), Tt::Cyan)
    } else {
        (None, Tt::Black)
    };
    row(vec![
        span(
            &format!("{} ", r.page),
            number_cells,
            reversed.unwrap_or(Tt::Cyan),
            bg,
        ),
        span(&title, title_cells, reversed.unwrap_or(title_colour), bg),
        span(&count, count_cells, reversed.unwrap_or(count_colour), bg),
    ])
}

/// Column-major pairs, as a printed index: the first half down the left.
pub fn sommaire_pairs<T: Clone>(items: &[T]) -> Vec<(T, Option<T>)> {
    let left = items.len().div_ceil(2);
    (0..left)
        .map(|i| (items[i].clone(), items.get(i + left).cloned()))
        .collect()
}

/// The wave of the app icon in half-block mosaics, repeated across the page.
const WAVE: (&str, &str) = (" ▄▀▀▄   ", "▀    ▀▄▄");

/// P100: `RESSAC` on a yellow bar, two rows of the cyan wave, then every page in two columns.
pub fn sommaire_lines(dir: &PageDirectory, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![title_bar("RESSAC", width)];
    for pattern in [WAVE.0, WAVE.1] {
        let cells: Vec<char> = pattern.chars().collect();
        let text: String = (0..width).map(|i| cells[i % cells.len()]).collect();
        lines.push(row(vec![span(&text, width, Tt::Cyan, Tt::Black)]));
    }
    lines.push(blank(width));
    let half = width.saturating_sub(1) / 2;
    for (left, right) in sommaire_pairs(dir.rows()) {
        let mut spans = index_line(&left, half, false, true).spans;
        spans.push(span("", width - 2 * half, Tt::White, Tt::Black));
        match right {
            Some(r) => spans.extend(index_line(&r, half, false, true).spans),
            None => spans.push(span("", half, Tt::White, Tt::Black)),
        }
        lines.push(row(spans));
    }
    lines
}

/// P899: the terminal's keys (spec §5), grouped as a Minitel GUIDE page.
pub const GUIDE: [(&str, &[(&str, &str)]); 5] = [
    (
        "PAGES",
        &[
            ("0-9", "type a page number"),
            ("DEL", "erase the last digit"),
            ("ESC", "cancel the number"),
            ("SHIFT S", "SOMMAIRE: every page"),
            ("?", "GUIDE: this page"),
        ],
    ),
    (
        "READING",
        &[
            ("SPACE  J", "SUITE: next page"),
            ("K", "RETOUR: previous page"),
            ("UP DOWN", "previous or next entry"),
            ("M", "mark read or unread"),
            ("S", "star or unstar"),
            ("I", "mosaics or original images"),
            ("F", "full article"),
            ("O 1-9", "open the article, or link 1-9"),
            ("R", "RÉPÉTITION: refresh"),
            ("SHIFT R", "refresh every feed"),
            ("SHIFT A", "mark all as read"),
            ("U", "all or unread only"),
            ("/", "search: ESC clears it"),
        ],
    ),
    (
        "COLUMNS",
        &[
            ("LEFT RIGHT", "change column (also H L TAB)"),
            ("ENTER ESC", "open, go back"),
        ],
    ),
    (
        "GARDEN",
        &[
            ("K", "keep"),
            ("R", "repair"),
            ("B", "bury"),
            ("U", "undo"),
            ("O", "open the site"),
            ("W", "open the Wayback snapshot"),
            ("SHIFT R", "apply the safe repairs"),
            ("C", "rhythm of the feed"),
        ],
    ),
    ("WINDOW", &[("T", "NATIVE or ANTIOPE"), ("Q", "quit")]),
];

const KEY_CELLS: usize = 12;

/// `GUIDE` on a yellow bar, then per section a blank row, the heading black on green, one row
/// per key (keys cyan, action white). Every row is exactly `width` cells.
pub fn guide_lines(width: usize) -> Vec<Line<'static>> {
    let key_cells = KEY_CELLS.min(width);
    let mut lines = vec![title_bar("GUIDE", width)];
    for (title, keys) in GUIDE {
        lines.push(blank(width));
        lines.push(row(vec![span(
            &format!(" {title}"),
            width,
            Tt::Black,
            Tt::Green,
        )]));
        for (k, action) in keys {
            lines.push(row(vec![
                span(&format!(" {k}"), key_cells, Tt::Cyan, Tt::Black),
                span(action, width - key_cells, Tt::White, Tt::Black),
            ]));
        }
    }
    lines
}

/// `entry_line`'s row: `●` yellow when unread, the title, `*` when starred; black on white when
/// selected.
pub fn entry_line(
    title: Option<&str>,
    read: bool,
    starred: bool,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let (fg, bg) = if selected {
        (Tt::Black, Tt::White)
    } else {
        (Tt::White, Tt::Black)
    };
    let marker_cells = 2.min(width);
    let star_cells = 2.min(width - marker_cells);
    row(vec![
        span(
            if read { "" } else { "●" },
            marker_cells,
            if selected { fg } else { Tt::Yellow },
            bg,
        ),
        span(
            title.unwrap_or("Untitled"),
            width - marker_cells - star_cells,
            fg,
            bg,
        ),
        span(if starred { " *" } else { "" }, star_cells, fg, bg),
    ])
}

/// The article's header rows: the title, the feed with author and date, the state in capitals,
/// and a failed full-text extraction in red.
fn article_header(view: &ReaderView<'_>, width: usize) -> Vec<Line<'static>> {
    let Some(entry) = view.entry else {
        return Vec::new();
    };
    let r = &entry.row;
    let mut by = r.feed_title.clone();
    if let Some(author) = r.author.as_deref() {
        by.push_str(&format!(" · {author}"));
    }
    by.push_str(&format!(
        " · {}",
        short_date(r.published_at, view.now_local).to_uppercase()
    ));
    let mut state = vec![if r.is_read { "READ" } else { "UNREAD" }];
    if r.is_starred {
        state.push("STARRED");
    }
    if view.show_fulltext {
        state.push("FULL TEXT");
    }
    let mut lines = vec![
        row(vec![span(
            r.title.as_deref().unwrap_or("(untitled)"),
            width,
            Tt::White,
            Tt::Black,
        )]),
        row(vec![span(&by, width, Tt::Cyan, Tt::Black)]),
        row(vec![span(&state.join(" · "), width, Tt::Yellow, Tt::Black)]),
    ];
    if let Some(error) = entry.fulltext_error.as_deref() {
        lines.push(row(vec![span(error, width, Tt::Red, Tt::Black)]));
    }
    lines
}

/// The rhythm menu (`c` on a feed): a green heading and the six choices, the cursor black on
/// cyan, the feed's current setting marked with `•`.
fn rhythm_menu_lines(choice: usize, current: Option<usize>, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![row(vec![span(" RHYTHM", width, Tt::Black, Tt::Green)])];
    for (i, group) in RHYTHM_CHOICES.iter().enumerate() {
        let name = group.map_or_else(|| "Auto".to_string(), group_label);
        let mark = if Some(i) == current { "•" } else { " " };
        let (fg, bg) = if i == choice {
            (Tt::Black, Tt::Cyan)
        } else {
            (Tt::White, Tt::Black)
        };
        lines.push(row(vec![span(
            &format!(" {mark} {}", name.to_uppercase()),
            width,
            fg,
            bg,
        )]));
    }
    lines
}

/// The rows of `items` to draw in `height` rows so that `selected` stays in view, the selection
/// kept near the middle once the list is longer than the column. Stateless: the same selection
/// always draws the same rows.
fn window<T>(items: &[T], selected: Option<usize>, height: usize) -> &[T] {
    if items.len() <= height {
        return items;
    }
    let top = selected
        .map_or(0, |s| s.saturating_sub(height / 2))
        .min(items.len() - height);
    &items[top..top + height]
}

/// Fills `area` with `lines`, padded with black rows (and cut) to its height.
fn paint(frame: &mut Frame, area: Rect, mut lines: Vec<Line<'static>>) {
    lines.truncate(usize::from(area.height));
    while lines.len() < usize::from(area.height) {
        lines.push(blank(usize::from(area.width)));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The `▐` separator, one cell wide, blue on black, the column's height.
fn separator(frame: &mut Frame, column: Rect) {
    let x = column.x + column.width;
    if x >= frame.area().right() {
        return;
    }
    let lines: Vec<Line> = (0..column.height)
        .map(|_| row(vec![span("▐", 1, Tt::Blue, Tt::Black)]))
        .collect();
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(x, column.y, 1, column.height),
    );
}

/// Draws the reader in Antiope. The garden pages are drawn by the Native look until 9b.
pub fn render(frame: &mut Frame, state: &ReaderState, images: &mut ImageStore) -> ReaderMetrics {
    if state.garden_view().is_some() {
        return native::render(frame, state, images);
    }
    let local = state
        .local
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.fixed_offset());
    render_at(frame, state, local, state.now)
}

/// `render` with the clock given: `local` shows in the header, `now` rolls the counter.
pub fn render_at(
    frame: &mut Frame,
    state: &ReaderState,
    local: DateTime<FixedOffset>,
    now: Option<DateTime<Utc>>,
) -> ReaderMetrics {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::new().fg(Tt::White.color()).bg(Tt::Black.color())),
        area,
    );
    let view = state.view();
    if view.layout == LayoutClass::TooSmall {
        let middle = Rect {
            y: area.y + area.height / 2,
            height: area.height.min(1),
            ..area
        };
        let line = centred(
            "WINDOW TOO SMALL",
            usize::from(area.width),
            Tt::Red,
            Tt::Black,
        );
        frame.render_widget(Paragraph::new(line), middle);
        return ReaderMetrics::default();
    }
    let rects = columns(area, view.layout, view.focus, Look::Antiope);
    let dir = PageDirectory::new(&state.pages, state.sources.as_ref());
    // The header: the page in view, or the counter searching while a job runs.
    let current = if view.help {
        Some(899)
    } else {
        state.antiope_page.or_else(|| dir.page_of(state.source))
    };
    let label = match (view.progress, now) {
        (Some(_), Some(now)) => HeaderLabel::Searching(rolling_page(now)),
        _ => HeaderLabel::Page(current),
    };
    paint(
        frame,
        rects.top,
        vec![header_line(&label, local, usize::from(rects.top.width))],
    );
    paint(
        frame,
        rects.bottom,
        vec![minitel_bar(
            &bar_hint(state, &view),
            usize::from(rects.bottom.width),
        )],
    );
    // The entries and the reader first: in the two-column class the index slides over them.
    if let Some(r) = rects.entries {
        paint(
            frame,
            r,
            entry_column(&view, usize::from(r.width), usize::from(r.height)),
        );
        if rects.reader.is_some() {
            separator(frame, r);
        }
    }
    let metrics = match rects.reader {
        Some(r) => reader_column(frame, r, state, &view, &dir),
        None => ReaderMetrics::default(),
    };
    if let Some(r) = rects.sources {
        let selected = dir
            .page_of(state.source)
            .and_then(|page| dir.rows().iter().position(|row| row.page == page));
        let lines: Vec<Line<'static>> = dir
            .rows()
            .iter()
            .enumerate()
            .map(|(i, row)| index_line(row, usize::from(r.width), selected == Some(i), false))
            .collect();
        let shown = window(&lines, selected, usize::from(r.height)).to_vec();
        paint(frame, r, shown);
        separator(frame, r);
    }
    if let Some((choice, _)) = view.rhythm {
        let width = 22.min(area.width);
        let lines = rhythm_menu_lines(choice, view.rhythm_current, usize::from(width));
        let height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .min(area.height);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Paragraph::new(lines), popup);
    }
    metrics
}

/// The entry column for any source (9b adds the Journal): `●` yellow when unread, the title,
/// `*` when starred, black on white when selected.
pub fn entry_column(view: &ReaderView<'_>, width: usize, height: usize) -> Vec<Line<'static>> {
    let lines: Vec<Line<'static>> = view
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            entry_line(
                e.title.as_deref(),
                e.is_read,
                e.is_starred,
                width,
                view.entry_cursor == Some(i),
            )
        })
        .collect();
    window(&lines, view.entry_cursor, height).to_vec()
}

/// The reader column: P100, P899, or the article (9b adds FIN DE L'EDITION and the images). With
/// no entry to show it is P100, the page a Minitel starts on.
fn reader_column(
    frame: &mut Frame,
    area: Rect,
    state: &ReaderState,
    view: &ReaderView<'_>,
    dir: &PageDirectory,
) -> ReaderMetrics {
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    let special = |frame: &mut Frame, lines: Vec<Line<'static>>| {
        // The scroll is the article's: a special page follows it only once it was chosen as a page.
        let top = if view.help {
            state.guide_scroll.min(lines.len().saturating_sub(height))
        } else if state.antiope_page.is_some() {
            state.scroll.min(lines.len().saturating_sub(height))
        } else {
            0
        };
        let total = lines.len();
        paint(frame, area, lines.into_iter().skip(top).collect());
        ReaderMetrics {
            total_rows: total,
            viewport_rows: area.height,
        }
    };
    match ReaderPage::of(state.antiope_page, view.help) {
        ReaderPage::Sommaire => special(frame, sommaire_lines(dir, width)),
        ReaderPage::Guide => special(frame, guide_lines(width)),
        ReaderPage::Article => match (view.entry, view.article) {
            (Some(_), Some(article)) => {
                let mut lines = article_header(view, width);
                let header = lines.len();
                let body = height.saturating_sub(header);
                lines.extend(
                    article
                        .lines
                        .iter()
                        .skip(view.reader_scroll)
                        .take(body)
                        .map(|l| {
                            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                            row(vec![span(&text, width, Tt::White, Tt::Black)])
                        }),
                );
                paint(frame, area, lines);
                ReaderMetrics {
                    total_rows: article.lines.len(),
                    viewport_rows: u16::try_from(body).unwrap_or(u16::MAX),
                }
            }
            _ if view.entry_cursor.is_some() => {
                paint(
                    frame,
                    area,
                    vec![row(vec![span("LOADING…", width, Tt::White, Tt::Black)])],
                );
                ReaderMetrics {
                    total_rows: 0,
                    viewport_rows: area.height,
                }
            }
            _ => special(frame, sommaire_lines(dir, width)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::pages::tests::sample_pages;
    use crate::tui::reader::settings::TerminalSettings;
    use crate::tui::reader::state::{Command, Event, GardenView};
    use chrono::TimeZone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ressac_core::model::CadenceGroup;
    use ressac_core::store::{
        EntryDetail, EntryPage, EntryRow, Source, SourceCounts, SourceNode, SourceNodeKind,
        SourcesSnapshot,
    };
    use std::collections::BTreeMap;

    fn local(h: u32, m: u32, s: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 16, h, m, s)
            .unwrap()
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn the_palette_is_the_eight_teletext_colours() {
        let rgb: Vec<_> = Tt::ALL.iter().map(|c| c.rgb()).collect();
        assert_eq!(
            rgb,
            [
                (0, 0, 0),
                (255, 0, 0),
                (0, 255, 0),
                (255, 255, 0),
                (0, 0, 255),
                (255, 0, 255),
                (0, 255, 255),
                (255, 255, 255)
            ]
        );
        assert_eq!(Tt::Cyan.color(), Color::Rgb(0, 255, 255));
    }

    #[test]
    fn fit_counts_display_cells_and_pads() {
        assert_eq!(fit("ABC", 5), "ABC  ");
        assert_eq!(fit("ABCDEF", 4), "ABCD");
        // The coffee cup and the CJK character are two cells wide: the CJK one would cross the
        // limit and becomes a space.
        assert_eq!(Span::raw(fit("CAFÉ ☕ 東", 8)).width(), 8);
        assert_eq!(fit("CAFÉ ☕ 東", 8), "CAFÉ ☕ ");
        assert_eq!(fit("", 3), "   ");
    }

    #[test]
    fn a_row_with_accents_and_emoji_is_exactly_its_width() {
        // Review Focus 1, Antiope side.
        let line = row(vec![
            span("  ", 2, Tt::White, Tt::Black),
            span("+3 MORE IN CAFÉ ☕ PÉDAGO", 26, Tt::White, Tt::Blue),
        ]);
        assert_eq!(line.width(), 28);
    }

    #[test]
    fn control_characters_never_reach_a_cell() {
        let hostile = format!("A{}]0;x{}B", '\x1b', '\x07');
        let s = span(&hostile, 6, Tt::White, Tt::Black);
        assert!(!s.content.chars().any(char::is_control));
        assert_eq!(s.width(), 6);
    }

    #[test]
    fn blue_only_colours_block_elements() {
        assert!(blue_allowed("▐▐"));
        assert!(blue_allowed("▀▄█"));
        assert!(!blue_allowed("TEXT"));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "blue never colours text")]
    fn blue_text_is_refused_in_debug() {
        let _ = span("DEAD", 4, Tt::Blue, Tt::Black);
    }

    #[test]
    fn the_header_shows_page_name_date_and_time() {
        let line = header_line(&HeaderLabel::Page(Some(412)), local(18, 33, 12), 60);
        assert_eq!(line.width(), 60);
        let t = text(&line);
        assert!(t.starts_with("P412 RESSAC"), "{t}");
        assert!(t.ends_with("FRI 16 OCT 18:33:12"), "{t}");
    }

    #[test]
    fn a_narrow_header_keeps_the_time_first() {
        // 10 cells: the time keeps its 8, the date gets the 2 left, the page none.
        let t = text(&header_line(
            &HeaderLabel::Page(Some(412)),
            local(9, 5, 0),
            10,
        ));
        assert_eq!(t, "FR09:05:00");
    }

    #[test]
    fn not_found_is_red_and_hides_the_name() {
        let line = header_line(&HeaderLabel::NotFound(437), local(9, 0, 0), 60);
        assert!(text(&line).starts_with("P437 NOT FOUND"));
        assert_eq!(line.spans[0].style.fg, Some(Tt::Red.color()));
        assert!(!text(&line).contains("RESSAC"));
    }

    #[test]
    fn the_counter_rolls_from_100_to_899() {
        let t0 = Utc.with_ymd_and_hms(2026, 10, 16, 16, 0, 0).unwrap();
        let a = rolling_page(t0);
        let b = rolling_page(t0 + chrono::Duration::milliseconds(100));
        assert!((100..=899).contains(&a));
        assert_eq!(if a == 899 { 100 } else { a + 1 }, b);
        // The end of the range wraps: tenth 799 of the cycle is 899, the next is 100.
        let last = (0..800)
            .map(|i| t0 + chrono::Duration::milliseconds(100 * i))
            .find(|t| rolling_page(*t) == 899)
            .unwrap();
        assert_eq!(
            rolling_page(last + chrono::Duration::milliseconds(100)),
            100
        );
    }

    #[test]
    fn the_minitel_bar_is_exactly_its_width() {
        for width in [20, 50, 140] {
            let line = minitel_bar(&MinitelHint::keys(Column::Entries, false, ""), width);
            assert_eq!(line.width(), width, "width {width}");
        }
        let line = minitel_bar(&MinitelHint::keys(Column::Entries, false, ""), 140);
        assert!(text(&line).starts_with(" SOMMAIRE   SUITE   RETOUR   GUIDE   RÉPÉTITION "));
        assert_eq!(line.spans[0].style.bg, Some(Tt::Cyan.color()));
        assert_eq!(line.spans[0].style.fg, Some(Tt::Black.color()));
    }

    #[test]
    fn the_status_replaces_the_hints() {
        let h = MinitelHint::status(Some((BUILDING, 34, 156)), true).unwrap();
        assert_eq!(h.text, "EDITION IN PROGRESS 34/156");
        assert_eq!(h.colour, Tt::Yellow);
        let h = MinitelHint::status(Some(("Fetching feeds", 42, 155)), false).unwrap();
        assert_eq!(h.text, "REFRESHING 42/155");
        let h = MinitelHint::status(Some((EXTRACT_PROGRESS_LABEL, 0, 1)), false).unwrap();
        assert_eq!(h.text, "FETCHING ARTICLE");
        assert!(MinitelHint::status(None, false).is_none());
    }

    #[test]
    fn an_urgent_hint_takes_the_bar_when_it_does_not_fit_beside_the_keys() {
        let hint = MinitelHint::status(Some(("Fetching feeds", 42, 155)), false).unwrap();
        let narrow = minitel_bar(&hint, 40);
        assert_eq!(narrow.width(), 40);
        assert!(text(&narrow).trim_end().ends_with("REFRESHING 42/155"));
        assert!(!text(&narrow).contains("SOMMAIRE"));
        let wide = minitel_bar(&hint, 140);
        assert!(text(&wide).contains("SOMMAIRE") && text(&wide).contains("REFRESHING 42/155"));
        // The plain hints never push the keys out.
        let plain = minitel_bar(&MinitelHint::keys(Column::Reader, false, ""), 60);
        assert!(text(&plain).starts_with(" SOMMAIRE "));
    }

    #[test]
    fn index_rows_follow_the_app() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let infos = dir.rows().iter().find(|r| r.page == 400).unwrap();
        let line = index_line(infos, 22, false, false);
        assert_eq!(line.width(), 22);
        assert!(text(&line).starts_with("400 INFOS"));
        assert_eq!(line.spans[1].style.fg, Some(Tt::Green.color()));
        let selected = index_line(infos, 22, true, false);
        assert_eq!(selected.spans[0].style.bg, Some(Tt::Cyan.color()));
        assert_eq!(selected.spans[0].style.fg, Some(Tt::Black.color()));
        let child = dir.rows().iter().find(|r| r.page == 403).unwrap();
        assert!(text(&index_line(child, 22, false, false)).starts_with("403  Actualitté"));
    }

    #[test]
    fn counts_sit_at_the_right_edge_and_the_triage_count_is_red() {
        let mut dir_rows = PageDirectory::new(&sample_pages(), None).rows().to_vec();
        let triage = dir_rows.iter_mut().find(|r| r.page == 800).unwrap();
        triage.count = Some(7);
        let line = index_line(triage, 22, false, false);
        assert_eq!(line.width(), 22);
        assert!(text(&line).ends_with(" 7"));
        assert_eq!(line.spans[1].style.fg, Some(Tt::Magenta.color()));
        assert_eq!(line.spans[2].style.fg, Some(Tt::Red.color()));
    }

    #[test]
    fn the_guide_rows_are_exactly_the_width() {
        for line in guide_lines(40) {
            assert_eq!(line.width(), 40);
        }
        let all: String = guide_lines(60).iter().map(text).collect();
        assert!(all.contains("SUITE: next page"));
        assert!(all.contains("O 1-9"));
    }

    #[test]
    fn sommaire_pairs_are_column_major() {
        let pairs = sommaire_pairs(&[1, 2, 3, 4, 5]);
        assert_eq!(pairs, vec![(1, Some(4)), (2, Some(5)), (3, None)]);
    }

    #[test]
    fn sommaire_rows_are_exactly_the_width() {
        let dir = PageDirectory::new(&sample_pages(), None);
        for width in [20, 59, 60, 89] {
            for line in sommaire_lines(&dir, width) {
                assert_eq!(line.width(), width, "width {width}");
            }
        }
    }

    #[test]
    fn a_long_list_keeps_the_selection_in_view() {
        let items: Vec<usize> = (0..100).collect();
        assert_eq!(window(&items, Some(0), 10), &items[0..10]);
        let shown = window(&items, Some(50), 10);
        assert_eq!(shown.len(), 10);
        assert!(shown.contains(&50));
        assert_eq!(window(&items, Some(99), 10), &items[90..100]);
        assert_eq!(window(&items, None, 10), &items[0..10]);
        assert_eq!(window(&items[..5], Some(3), 10).len(), 5);
    }

    // The state under test: the Antiope look, three feeds and the page list, the Unread list.

    const ARTICLE: &str = "<h2>Le jardin</h2><p>Un long texte avec un \
        <a href=\"https://example.com/a\">premier lien</a>.</p>";

    fn entry(id: i64, title: &str, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: 12,
            feed_title: "Courrier international".into(),
            title: Some(title.into()),
            author: Some("Anne".into()),
            url: Some(format!("https://example.com/{id}")),
            published_at: Utc.with_ymd_and_hms(2026, 10, 16, 6, 30, 0).unwrap(),
            is_read: read,
            is_starred: id == 12,
            has_fulltext: false,
        }
    }

    fn snapshot() -> SourcesSnapshot {
        let cadence: BTreeMap<_, _> = CadenceGroup::ALL.iter().map(|g| (*g, 0)).collect();
        SourcesSnapshot {
            counts: SourceCounts {
                journal: 2,
                unread: 3,
                today: 3,
                starred: 1,
                needs_triage: 1,
                cadence,
            },
            tree: vec![SourceNode {
                kind: SourceNodeKind::Folder {
                    id: 4,
                    name: "Infos".into(),
                },
                page: Some(400),
                unread: 3,
                children: vec![SourceNode {
                    kind: SourceNodeKind::Feed {
                        id: 12,
                        title: "Courrier international".into(),
                    },
                    page: Some(412),
                    unread: 3,
                    children: vec![],
                }],
            }],
        }
    }

    fn at_six_thirty_three(s: &mut ReaderState) {
        s.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 16, 33, 12).unwrap(),
            local: local(18, 33, 12),
        });
    }

    fn antiope_state(width: u16, height: u16) -> ReaderState {
        let mut s = ReaderState::new(
            TerminalSettings {
                look: Look::Antiope,
                images: false,
            },
            width,
            height,
        );
        s.on_event(Event::Sources(snapshot()));
        s.on_event(Event::Pages(sample_pages()));
        at_six_thirty_three(&mut s);
        s
    }

    fn antiope_state_on_page(page: u16) -> ReaderState {
        let mut s = antiope_state(140, 34);
        s.antiope_page = Some(page);
        s
    }

    /// Feed 12's list with `rows`, the first unread one open in the reader.
    fn reading_state(width: u16, height: u16, rows: Vec<EntryRow>) -> ReaderState {
        let mut s = antiope_state(width, height);
        s.go_to(Source::Feed(12));
        let unread = rows.iter().filter(|r| !r.is_read).count();
        s.on_event(Event::Entries {
            source: Source::Feed(12),
            page: EntryPage {
                total: rows.len(),
                unread,
                rows,
                next: None,
            },
            appended: false,
            request: s.request,
        });
        if s.selected_entry().is_none() {
            s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        }
        let open = s.entries[s.view().entry_cursor.expect("an entry is selected")].clone();
        s.on_event(Event::Entry(Box::new(EntryDetail {
            row: open,
            summary_html: None,
            content_html: Some(ARTICLE.into()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        })));
        s
    }

    fn rows() -> Vec<EntryRow> {
        vec![
            entry(11, "Le jardin des flux", false),
            entry(12, "Un deuxième article", false),
            entry(13, "Lu hier", true),
        ]
    }

    fn draw_terminal(state: &ReaderState, width: u16, height: u16) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                render_at(f, state, local(18, 33, 12), state.now);
            })
            .unwrap();
        terminal
    }

    fn draw(state: &ReaderState, width: u16, height: u16) -> String {
        draw_terminal(state, width, height).backend().to_string()
    }

    fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn antiope_sommaire_at_140_and_70_columns() {
        let state = antiope_state_on_page(100);
        insta::assert_snapshot!("antiope_sommaire_140", draw(&state, 140, 34));
        let mut narrow = antiope_state_on_page(100);
        narrow.on_event(Event::Resize {
            width: 70,
            height: 34,
        });
        narrow.set_focus(Column::Reader);
        insta::assert_snapshot!("antiope_sommaire_70", draw(&narrow, 70, 34));
    }

    #[test]
    fn antiope_guide_at_140_columns() {
        // Tall enough for the five sections.
        let mut state = antiope_state_on_page(899);
        state.on_event(Event::Resize {
            width: 140,
            height: 44,
        });
        insta::assert_snapshot!("antiope_guide_140", draw(&state, 140, 44));
    }

    #[test]
    fn antiope_reading_at_140_and_70_columns() {
        let state = reading_state(140, 30, rows());
        insta::assert_snapshot!("antiope_reading_140", draw(&state, 140, 30));
        let mut narrow = reading_state(70, 30, rows());
        narrow.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        insta::assert_snapshot!("antiope_reading_70", draw(&narrow, 70, 30));
    }

    #[test]
    fn the_header_and_the_bar_frame_the_page() {
        let state = reading_state(140, 30, rows());
        let terminal = draw_terminal(&state, 140, 30);
        let top = row_text(&terminal, 0);
        assert!(top.starts_with("P412 RESSAC"), "{top}");
        assert!(top.ends_with("FRI 16 OCT 18:33:12"), "{top}");
        let bottom = row_text(&terminal, 29);
        assert!(
            bottom.starts_with(" SOMMAIRE   SUITE   RETOUR   GUIDE   RÉPÉTITION "),
            "{bottom}"
        );
    }

    #[test]
    fn the_counter_searches_while_a_job_runs_and_the_bar_says_which() {
        let mut state = reading_state(140, 30, rows());
        state.on_event(Event::Progress {
            label: BUILDING.into(),
            done: 34,
            total: 156,
        });
        let terminal = draw_terminal(&state, 140, 30);
        let top = row_text(&terminal, 0);
        let page = rolling_page(state.now.unwrap());
        assert!(top.starts_with(&format!("P{page} RESSAC")), "{top}");
        assert!(row_text(&terminal, 29).contains("EDITION IN PROGRESS 34/156"));
    }

    #[test]
    fn the_index_follows_the_source_and_the_separators_are_blue_blocks() {
        let state = reading_state(140, 30, rows());
        let terminal = draw_terminal(&state, 140, 30);
        let buffer = terminal.backend().buffer();
        // The selected page (412) is black on cyan; the separators sit at 22 and 51.
        let selected_row = (1..29)
            .find(|y| row_text(&terminal, *y).starts_with("412 "))
            .expect("the page 412 is listed");
        assert_eq!(buffer[(0, selected_row)].bg, Tt::Cyan.color());
        for y in 1..29 {
            for x in [22, 51] {
                assert_eq!(buffer[(x, y)].symbol(), "▐", "x {x} y {y}");
                assert_eq!(buffer[(x, y)].fg, Tt::Blue.color());
            }
        }
    }

    #[test]
    fn blue_never_colours_text_and_every_cell_is_on_the_palette() {
        let mut state = reading_state(140, 30, rows());
        for page in [None, Some(100), Some(899)] {
            state.antiope_page = page;
            let terminal = draw_terminal(&state, 140, 30);
            let buffer = terminal.backend().buffer();
            let palette: Vec<Color> = Tt::ALL.iter().map(|c| c.color()).collect();
            for y in 0..30 {
                for x in 0..140 {
                    let cell = &buffer[(x, y)];
                    assert!(palette.contains(&cell.bg), "bg at {x},{y}: {:?}", cell.bg);
                    assert!(palette.contains(&cell.fg), "fg at {x},{y}: {:?}", cell.fg);
                    if cell.fg == Tt::Blue.color() {
                        assert!(
                            blue_allowed(cell.symbol()),
                            "blue text {:?} at {x},{y}",
                            cell.symbol()
                        );
                    }
                }
            }
        }
    }

    /// Review Focus 1, Antiope side: escape sequences, an emoji and CJK in a 28-cell column.
    #[test]
    fn escape_sequences_and_wide_titles_stay_inside_their_column() {
        let hostile = format!(
            "Café ☕ 東京 {}]0;pwned{} {}31m fin",
            '\x1b', '\x07', '\u{9b}'
        );
        for (width, height) in [(140u16, 20u16), (100, 20)] {
            let state = reading_state(
                width,
                height,
                vec![entry(11, &hostile, false), entry(12, "Autre", false)],
            );
            let terminal = draw_terminal(&state, width, height);
            let out = terminal.backend().to_string();
            assert!(
                !out.chars()
                    .any(|c| c == '\x1b' || c == '\x07' || c == '\u{9b}'),
                "{out}"
            );
            let buffer = terminal.backend().buffer();
            // The entries column is 28 cells from x 23 (three columns) or 0 (two columns); the
            // cell after it is always the separator, whatever the title holds.
            let separator_x = if width >= 120 { 51 } else { 28 };
            for y in 1..height - 1 {
                assert_eq!(
                    buffer[(separator_x, y)].symbol(),
                    "▐",
                    "{width}x{height} row {y}"
                );
            }
        }
    }

    #[test]
    fn the_article_scrolls_and_reports_its_metrics() {
        let state = reading_state(140, 30, rows());
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        let mut metrics = None;
        terminal
            .draw(|f| metrics = Some(render_at(f, &state, local(18, 33, 12), state.now)))
            .unwrap();
        let m = metrics.unwrap();
        assert!(m.total_rows > 0);
        // The reader is 28 rows tall: three header rows, the rest is the viewport.
        assert_eq!(m.viewport_rows, 25);
        let out = terminal.backend().to_string();
        assert!(out.contains("Le jardin des flux"), "{out}");
        assert!(out.contains("UNREAD"), "{out}");
    }

    #[test]
    fn no_entry_shows_p100_in_the_reader() {
        let state = antiope_state(140, 30);
        let out = draw(&state, 140, 30);
        assert!(out.contains("RESSAC") && out.contains("400 INFOS"), "{out}");
    }

    #[test]
    fn the_help_key_shows_the_guide_until_the_next_key() {
        let mut state = reading_state(140, 30, rows());
        state.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let out = draw(&state, 140, 30);
        assert!(out.contains("SUITE: next page"), "{out}");
        assert!(row_text(&draw_terminal(&state, 140, 30), 0).starts_with("P899 RESSAC"));
        state.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!draw(&state, 140, 30).contains("SUITE: next page"));
    }

    #[test]
    fn the_rhythm_menu_is_drawn_while_it_has_the_keyboard() {
        let mut state = reading_state(140, 30, rows());
        state.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        let out = draw(&state, 140, 30);
        assert!(
            out.contains("RHYTHM") && out.contains("WEEKLY") && out.contains("AUTO"),
            "{out}"
        );
        assert!(out.contains("ESC CLOSE"), "{out}");
    }

    #[test]
    fn the_search_prompt_and_the_messages_are_on_the_bar() {
        let mut state = reading_state(140, 30, rows());
        state.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        state.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(row_text(&draw_terminal(&state, 140, 30), 29).contains("SEARCH? x_"));
        state.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(row_text(&draw_terminal(&state, 140, 30), 29).contains("SEARCH: x"));
        let mut prompt = reading_state(140, 30, rows());
        prompt.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(row_text(&draw_terminal(&prompt, 140, 30), 29).contains("OPEN: ENTER"));
    }

    #[test]
    fn window_too_small_is_a_red_row() {
        let state = antiope_state(30, 8);
        let terminal = draw_terminal(&state, 30, 8);
        assert!(row_text(&terminal, 4).contains("WINDOW TOO SMALL"));
        assert_eq!(terminal.backend().buffer()[(10, 4)].fg, Tt::Red.color());
    }

    /// 9a draws the garden pages in Native (9b gives them a teletext page), so the garden keys,
    /// which stay live under `t`, never act on an invisible row.
    #[test]
    fn a_garden_page_is_drawn_by_native_until_9b_and_its_row_is_on_screen() {
        use crate::tui::reader::images::ImageStore;
        let mut state = antiope_state(140, 30);
        state.open_garden(GardenView::NeedsTriage);
        state.on_event(Event::GardenRows(crate::tui::state::sample_rows()));
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        terminal
            .draw(|f| {
                render(f, &state, &mut ImageStore::disabled());
            })
            .unwrap();
        let out = terminal.backend().to_string();
        assert!(out.contains("Needs triage"), "{out}");
        assert!(
            out.contains("Bibliobsession 2.0"),
            "the selected row is visible: {out}"
        );
        // And the key that acts on it is the garden's.
        let commands = state.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(
            commands.iter().any(|c| matches!(c, Command::Garden(_))),
            "{commands:?}"
        );
    }

    #[test]
    fn the_sources_slide_over_the_entries_in_two_columns() {
        let mut state = reading_state(100, 30, rows());
        state.set_focus(Column::Sources);
        let terminal = draw_terminal(&state, 100, 30);
        assert!(
            (1..29).any(|y| row_text(&terminal, y).starts_with("412 ")),
            "the index is on screen"
        );
        assert_eq!(terminal.backend().buffer()[(22, 5)].symbol(), "▐");
    }

    #[test]
    fn a_page_list_longer_than_the_column_scrolls_to_the_selected_page() {
        let mut pages = sample_pages();
        for n in 0..60u16 {
            pages.push(ressac_core::store::pages::PageRow {
                page: 500 + n,
                kind: PageKind::Feed,
                id: Some(1000 + i64::from(n)),
                title: format!("Feed {n}"),
                parent_page: None,
            });
        }
        let mut state = reading_state(140, 20, rows());
        state.on_event(Event::Pages(pages));
        let terminal = draw_terminal(&state, 140, 20);
        assert!(
            (1..19).any(|y| row_text(&terminal, y).starts_with("412 ")),
            "the selected page stays on screen"
        );
    }

    #[test]
    fn the_guide_is_reachable_on_a_short_pane() {
        let mut state = reading_state(140, 30, rows());
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        let mut frame = |state: &mut ReaderState| {
            let mut m = ReaderMetrics::default();
            terminal
                .draw(|f| m = render_at(f, state, local(18, 33, 12), state.now))
                .unwrap();
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
            terminal.backend().to_string()
        };
        state.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let first = frame(&mut state);
        assert!(
            first.contains("PAGES") && !first.contains("NATIVE or ANTIOPE"),
            "{first}"
        );
        for _ in 0..4 {
            state.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
            frame(&mut state);
        }
        let last = frame(&mut state);
        assert!(
            last.contains("NATIVE or ANTIOPE") && last.contains("quit"),
            "{last}"
        );
        // Up scrolls back; any other key closes and resets.
        state.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(state.guide_scroll > 0);
        state.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(state.guide_scroll, 0);
        assert!(!frame(&mut state).contains("NATIVE or ANTIOPE"));
    }

    #[test]
    fn a_tick_moves_the_header_clock_through_production_render() {
        use crate::tui::reader::images::ImageStore;
        let mut state = reading_state(140, 30, rows());
        let top = |state: &ReaderState| {
            let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
            terminal
                .draw(|f| {
                    render(f, state, &mut ImageStore::disabled());
                })
                .unwrap();
            row_text(&terminal, 0)
        };
        assert!(top(&state).ends_with("FRI 16 OCT 18:33:12"));
        state.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 16, 33, 13).unwrap(),
            local: local(18, 33, 13),
        });
        assert!(top(&state).ends_with("FRI 16 OCT 18:33:13"));
    }

    #[test]
    fn hints_are_capitals_and_cut_at_a_word() {
        assert_eq!(
            words_within("J NEXT · K PREVIOUS · M READ", 40),
            "J NEXT · K PREVIOUS · M READ"
        );
        assert_eq!(
            words_within("J NEXT · K PREVIOUS · M READ", 21),
            "J NEXT · K PREVIOUS"
        );
        assert_eq!(words_within("J NEXT · K PREVIOUS · M READ", 14), "J NEXT");
        assert_eq!(words_within("ABCDEFGHIJ", 4), "ABCD");
        let line = minitel_bar(&MinitelHint::keys(Column::Entries, false, ""), 60);
        assert_eq!(line.width(), 60);
        assert!(
            text(&line).trim_end().ends_with("J NEXT"),
            "{}",
            text(&line)
        );
        for column in [Column::Sources, Column::Entries, Column::Reader] {
            let h = MinitelHint::keys(column, false, "").text;
            assert_eq!(h, h.to_uppercase());
        }
    }
}
