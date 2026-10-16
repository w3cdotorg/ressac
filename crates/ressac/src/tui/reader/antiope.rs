//! Antiope in the terminal (spec §8): the eight teletext colours on black, rows exactly as wide
//! as their column, the page header, the Minitel bar, P100 SOMMAIRE and P899 GUIDE, P104
//! JOURNAL with its sub-pages and `FIN DE L'EDITION`, the garden pages P800 to P802, and the
//! article with its images as teletext mosaics. Pure functions over plain data, and `render`.

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ressac_core::edition::Section;
use ressac_core::garden::GardenRow;
use ressac_core::model::{Action, Decision, Remark};
use ressac_core::store::pages::PageKind;
use ressac_core::store::{EditionOverflowRow, JournalView};

use super::article::{BodyLook, draw_body};
use super::html::{MAX_TEXT_WIDTH, is_web_url};
use super::images::{ImageStore, Variant, snap_to_teletext};
use super::journal::{self, BUILDING, JournalLine, JournalPhase};
use super::layout::{Column, LayoutClass, Look, columns};
use super::native::{EXTRACT_PROGRESS_LABEL, LOADING_IMAGE, ReaderMetrics, short_date};
use super::pages::{DirRow, PageDirectory, ReaderPage, subpage_label};
use super::state::{GardenView, RHYTHM_CHOICES, ReaderState, ReaderView, group_label};
use crate::output::printable;
use crate::tui::state::{Filter, GardenState, Mode};
use crate::tui::ui::attempt_outcome;

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
    Typing(String),
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

/// What the Minitel bar says: the rhythm menu's keys, the search prompt, a message, the garden's
/// confirmation or message, a running job (all urgent), else the hints of the focused column.
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
    if let Some(hint) = state.garden().and_then(garden_hint) {
        return hint;
    }
    let edition = view.progress.is_some_and(|(label, _, _)| label == BUILDING);
    MinitelHint::status(view.progress, edition).unwrap_or_else(|| {
        MinitelHint::keys(
            view.focus,
            view.garden,
            state.search.as_deref().unwrap_or(""),
        )
    })
}

/// The garden's own prompts: its confirmation, its help (any key closes it) and its message.
fn garden_hint(garden: &GardenState) -> Option<MinitelHint> {
    match garden.mode {
        Mode::ConfirmRepairs => Some(MinitelHint::urgent(
            format!(
                "APPLY {} SAFE REPAIRS? Y APPLY · ANY OTHER KEY CANCELS",
                garden.safe_repair_count()
            ),
            Tt::Yellow,
        )),
        Mode::Help => Some(MinitelHint::urgent(
            "ANY KEY CLOSES THE HELP".into(),
            Tt::White,
        )),
        Mode::Queue | Mode::Detail => garden
            .message
            .as_deref()
            .map(|m| MinitelHint::urgent(printable(m).to_uppercase(), Tt::Yellow)),
    }
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

/// Greedy word wrap of `text` into rows of at most `cells` display cells; a word wider than a
/// row (an address) is cut into full rows. Feed text goes through `printable` first.
pub fn wrap(text: &str, cells: usize) -> Vec<String> {
    let width = |t: &str| Span::raw(t).width();
    let mut rows = Vec::new();
    if cells == 0 {
        return rows;
    }
    let mut current = String::new();
    for raw in text.split_whitespace() {
        let word = printable(raw).into_owned();
        let mut rest = word.as_str();
        while width(rest) > cells {
            if !current.is_empty() {
                rows.push(std::mem::take(&mut current));
            }
            let (head, tail) = split_at_width(rest, cells);
            rows.push(head.to_string());
            rest = tail;
        }
        if current.is_empty() {
            current = rest.to_string();
        } else if width(&current) + 1 + width(rest) <= cells {
            current.push(' ');
            current.push_str(rest);
        } else {
            rows.push(std::mem::replace(&mut current, rest.to_string()));
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// `text` cut after at most `cells` display cells (one character at least, so that a column
/// narrower than a wide character still advances).
fn split_at_width(text: &str, cells: usize) -> (&str, &str) {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        let w = Span::raw(c.to_string()).width();
        if used + w > cells {
            return text.split_at(if i == 0 { c.len_utf8() } else { i });
        }
        used += w;
    }
    (text, "")
}

/// The teletext colours of a span of article text (the Native look's emphasis, without bold or
/// italics to draw it with): headings green, links cyan, strong yellow, emphasis magenta,
/// deleted red, code in reverse video, the rest white.
fn prose_colours(style: Style) -> (Tt, Tt) {
    let m = style.add_modifier;
    if m.contains(Modifier::BOLD | Modifier::UNDERLINED) {
        (Tt::Green, Tt::Black)
    } else if m.contains(Modifier::REVERSED) {
        (Tt::Black, Tt::White)
    } else if m.contains(Modifier::UNDERLINED) {
        (Tt::Cyan, Tt::Black)
    } else if m.contains(Modifier::BOLD) {
        (Tt::Yellow, Tt::Black)
    } else if m.contains(Modifier::ITALIC) {
        (Tt::Magenta, Tt::Black)
    } else if m.contains(Modifier::CROSSED_OUT) {
        (Tt::Red, Tt::Black)
    } else {
        (Tt::White, Tt::Black)
    }
}

/// One line of the article as exactly `width` cells: each span keeps its emphasis as a teletext
/// colour, controls neutralised, cut where the column ends.
pub fn prose_row(line: &Line<'_>, width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut remaining = width;
    for s in &line.spans {
        if remaining == 0 {
            break;
        }
        let clean = printable(&s.content);
        let cells = Span::raw(clean.as_ref()).width().min(remaining);
        let (fg, bg) = prose_colours(line.style.patch(s.style));
        spans.push(span(&clean, cells, fg, bg));
        remaining -= cells;
    }
    spans.push(span("", remaining, Tt::White, Tt::Black));
    row(spans)
}

/// P104's entry column (spec §8, §9): while the edition builds, only its progress; offline or
/// failed, why there is none; else the header, then each section with its entries in rank order
/// and each feed's overflow row under its last entry (the rows of `journal::journal_lines`, the
/// ones the cursor walks), then `FIN DE L'EDITION` once every entry is read. Every row is
/// exactly `width` cells. The second value is the row of the cursor.
pub fn journal_rows(
    journal: Option<&JournalView>,
    phase: &JournalPhase,
    lines: &[JournalLine],
    cursor: Option<usize>,
    offset: FixedOffset,
    width: usize,
) -> (Vec<Line<'static>>, Option<usize>) {
    let message = |rows: Vec<Line<'static>>| {
        let mut out = vec![blank(width)];
        out.extend(rows);
        (out, None)
    };
    match phase {
        JournalPhase::Building { done, total } => {
            return message(vec![centred(
                &counted("EDITION IN PROGRESS", *done, *total),
                width,
                Tt::Yellow,
                Tt::Black,
            )]);
        }
        JournalPhase::Offline => {
            return message(vec![
                centred("OFFLINE: NO EDITION YET", width, Tt::Black, Tt::Red),
                centred("RÉPÉTITION: RETRY", width, Tt::Cyan, Tt::Black),
            ]);
        }
        JournalPhase::Failed(reason) => {
            return message(
                wrap(&reason.to_uppercase(), width.saturating_sub(2))
                    .iter()
                    .map(|text| centred(text, width, Tt::Black, Tt::Red))
                    .collect(),
            );
        }
        JournalPhase::Idle => {}
    }
    let Some(view) = journal else {
        return message(vec![centred(
            &counted("EDITION IN PROGRESS", 0, 0),
            width,
            Tt::Yellow,
            Tt::Black,
        )]);
    };
    let read = view.items.iter().filter(|item| item.row.is_read).count();
    let count = format!(" {read}/{} READ", view.items.len());
    let count_cells = count.len().min(width);
    let day = view
        .edition
        .built_at
        .with_timezone(&offset)
        .format(" %a %d %b")
        .to_string()
        .to_uppercase();
    let mut out = vec![row(vec![
        span(&day, width - count_cells, Tt::Cyan, Tt::Black),
        span(&count, count_cells, Tt::Yellow, Tt::Black),
    ])];
    match view.edition.unreachable {
        0 => {}
        1 => out.push(row(vec![span(
            " 1 FEED UNREACHABLE",
            width,
            Tt::Red,
            Tt::Black,
        )])),
        n => out.push(row(vec![span(
            &format!(" {n} FEEDS UNREACHABLE"),
            width,
            Tt::Red,
            Tt::Black,
        )])),
    }
    if view.items.is_empty() {
        out.push(blank(width));
        out.push(centred(
            &journal::EMPTY_LINE.to_uppercase(),
            width,
            Tt::Cyan,
            Tt::Black,
        ));
        return (out, None);
    }
    let mut selected = None;
    for (index, line) in lines.iter().enumerate() {
        let is_cursor = cursor == Some(index);
        if is_cursor && !matches!(line, JournalLine::Heading(_)) {
            selected = Some(out.len());
        }
        match *line {
            JournalLine::Heading(section) => {
                out.push(blank(width));
                let entries = view.items.iter().filter(|i| i.section == section).count();
                out.push(section_heading(section, entries, width));
            }
            JournalLine::Entry(i) => {
                let r = &view.items[i].row;
                out.push(entry_line(
                    r.title.as_deref(),
                    r.is_read,
                    r.is_starred,
                    width,
                    is_cursor,
                ));
            }
            JournalLine::Overflow(o) => {
                out.push(overflow_line(&view.overflow[o], width, is_cursor));
            }
        }
    }
    if journal::is_finished(view) {
        out.push(blank(width));
        out.push(end_line(width));
    }
    (out, selected)
}

/// ` FRONT PAGE … 2`: yellow, ` THIS WEEK` cyan, ` THE WIRE` green, on black.
fn section_heading(section: Section, entries: usize, width: usize) -> Line<'static> {
    let colour = match section {
        Section::Front => Tt::Yellow,
        Section::Week => Tt::Cyan,
        Section::Wire => Tt::Green,
    };
    let number = format!(" {entries}");
    let number_cells = number.len().min(width);
    row(vec![
        span(
            &format!(" {}", journal::section_title(section).to_uppercase()),
            width - number_cells,
            colour,
            Tt::Black,
        ),
        span(&number, number_cells, colour, Tt::Black),
    ])
}

/// `  +12 MORE IN ACTUALITTÉ`: white on blue (blue never colours text), two cells in; black on
/// white under the cursor.
pub fn overflow_line(more: &EditionOverflowRow, width: usize, selected: bool) -> Line<'static> {
    let indent = 2.min(width);
    let (fg, bg, margin) = if selected {
        (Tt::Black, Tt::White, Tt::White)
    } else {
        (Tt::White, Tt::Blue, Tt::Black)
    };
    row(vec![
        span("", indent, Tt::White, margin),
        span(
            &format!(
                "+{} MORE IN {}",
                more.hidden_count,
                more.feed_title.to_uppercase()
            ),
            width - indent,
            fg,
            bg,
        ),
    ])
}

/// `FIN DE L'EDITION` on a full-width yellow bar (the terminal's double height).
pub fn end_line(width: usize) -> Line<'static> {
    title_bar("FIN DE L'EDITION", width)
}

/// A garden status in its teletext colours (spec §8): Fossil's blue never colours text, so its
/// heading is white on blue.
fn status_colours(status: &str) -> (Tt, Tt) {
    let colour = match status {
        "Dead" => Tt::Red,
        "Hijacked" => Tt::Magenta,
        "Moved" => Tt::Cyan,
        "Ailing" => Tt::Yellow,
        "Fossil" => Tt::Blue,
        "Dormant" | "Alive" => Tt::Green,
        _ => Tt::White,
    };
    if colour == Tt::Blue {
        (Tt::White, Tt::Blue)
    } else {
        (colour, Tt::Black)
    }
}

/// ` DEAD … 8`: a garden status heading with the number of feeds under it.
pub fn garden_heading(status: &str, count: usize, width: usize) -> Line<'static> {
    let (fg, bg) = status_colours(status);
    let number = format!(" {count}");
    let number_cells = number.len().min(width);
    row(vec![
        span(
            &format!(" {}", status.to_uppercase()),
            width - number_cells,
            fg,
            bg,
        ),
        span(&number, number_cells, fg, bg),
    ])
}

/// What a feed's row says at its right edge: the decision once made, else the proposed action.
fn garden_chip(r: &GardenRow) -> Option<(&'static str, Tt)> {
    if let Some(record) = r.decision.as_ref().filter(|_| !r.needs_triage()) {
        return Some((
            match record.decision {
                Decision::Keep => "kept",
                Decision::Repair => "repaired",
                Decision::Bury => "buried",
            },
            Tt::Green,
        ));
    }
    r.action().map(|action| {
        (
            match action {
                Action::UpgradeHttps { .. } => "https",
                Action::FollowRedirect { .. } => "follow",
                Action::AdoptDiscovered { .. } => "adopt",
                Action::MergeInto { .. } => "merge",
                Action::Bury => "bury",
            },
            Tt::Cyan,
        )
    })
}

/// `● Title … bury`: a yellow dot while the feed needs triage, the title, the chip in cyan (green
/// once decided); black on white when selected.
pub fn garden_feed_line(r: &GardenRow, selected: bool, width: usize) -> Line<'static> {
    let chip = garden_chip(r).map_or_else(String::new, |(text, _)| format!(" {text}"));
    let marker_cells = 2.min(width);
    let chip_cells = Span::raw(chip.as_str()).width().min(width - marker_cells);
    let (fg, bg) = if selected {
        (Tt::Black, Tt::White)
    } else {
        (Tt::White, Tt::Black)
    };
    let chip_colour = match garden_chip(r) {
        Some((_, colour)) if !selected => colour,
        _ => fg,
    };
    row(vec![
        span(
            if r.needs_triage() { "●" } else { "" },
            marker_cells,
            if selected { fg } else { Tt::Yellow },
            bg,
        ),
        span(&r.title, width - marker_cells - chip_cells, fg, bg),
        span(&chip, chip_cells, chip_colour, bg),
    ])
}

/// P800–P802's entry column (spec §8): the title and its count, P800's gauge, then the feeds in
/// the order the arrows walk them under a coloured heading per status. The second value is the
/// row of the selected feed.
pub fn garden_rows(
    garden: &GardenState,
    view: GardenView,
    width: usize,
) -> (Vec<Line<'static>>, Option<usize>) {
    let queue = garden.filter == Filter::Queue;
    let title = if garden.filter == view.filter() {
        view.title().to_uppercase()
    } else {
        garden.filter.label().to_uppercase()
    };
    let count = if queue {
        let p = garden.progress();
        format!(" {} LEFT", p.needing.saturating_sub(p.decided))
    } else if garden.visible.len() == 1 {
        " 1 FEED".to_string()
    } else {
        format!(" {} FEEDS", garden.visible.len())
    };
    let count_cells = count.len().min(width);
    let mut out = vec![row(vec![
        span(&title, width - count_cells, Tt::White, Tt::Black),
        span(&count, count_cells, Tt::Yellow, Tt::Black),
    ])];
    if view == GardenView::NeedsTriage {
        let p = garden.progress();
        out.push(gauge_line(p.decided, p.needing, width));
    }
    if garden.visible.is_empty() {
        let text = match garden.filter {
            Filter::Queue => "NOTHING TO TRIAGE",
            Filter::Cemetery => "THE CEMETERY IS EMPTY",
            _ => "NO FEEDS",
        };
        out.push(blank(width));
        out.push(centred(text, width, Tt::Cyan, Tt::Black));
        return (out, None);
    }
    let mut selected = None;
    let mut start = 0;
    while start < garden.visible.len() {
        let key = |at: usize| {
            let r = &garden.rows[garden.visible[at]];
            if r.buried {
                "Buried"
            } else {
                r.status().as_str()
            }
        };
        let mut end = start + 1;
        while end < garden.visible.len() && key(end) == key(start) {
            end += 1;
        }
        out.push(garden_heading(key(start), end - start, width));
        for at in start..end {
            if at == garden.selected {
                selected = Some(out.len());
            }
            out.push(garden_feed_line(
                &garden.rows[garden.visible[at]],
                at == garden.selected,
                width,
            ));
        }
        start = end;
    }
    (out, selected)
}

/// P800's mosaic gauge: `▐███░░░ 12/80`, feeds triaged over feeds with a problem. Blue is
/// allowed here: the shade is a mosaic.
fn gauge_line(done: usize, total: usize, width: usize) -> Line<'static> {
    let label = format!(" {done}/{total}");
    let label_cells = label.len().min(width.saturating_sub(1));
    let bar = width.saturating_sub(1 + label_cells);
    let filled = (bar * done).checked_div(total).unwrap_or(0).min(bar);
    row(vec![
        span("▐", 1.min(width), Tt::Cyan, Tt::Black),
        span(&"█".repeat(filled), filled, Tt::Green, Tt::Black),
        span(&"░".repeat(bar - filled), bar - filled, Tt::Blue, Tt::Black),
        span(&label, label_cells, Tt::Yellow, Tt::Black),
    ])
}

/// What a remark says, worded as the app's inspector.
fn remark_sentence(remark: &Remark) -> String {
    match remark {
        Remark::HttpsAvailable => "The https:// address serves the same feed.".into(),
        Remark::HttpOnly => "Only served over http://.".into(),
        Remark::SimilarTitle { feed_id } => {
            format!("A feed in the same folder has a similar title (#{feed_id}).")
        }
        Remark::DateFallback => "Most entries carry no date; Ressac used the fetch date.".into(),
        Remark::Blocked => "The last fetch was blocked (403 or an anti-bot page).".into(),
    }
}

/// The garden inspector as a teletext page `width` cells wide (spec §8): the name and the folder
/// path, the status on its heading row, the reason, the rhythm, then evidence, remarks, links,
/// redirects and the fetch log. Every row is exactly `width` cells.
pub fn garden_inspector(
    row_data: Option<&GardenRow>,
    rhythm: Option<&str>,
    offset: FixedOffset,
    width: usize,
) -> Vec<Line<'static>> {
    let mut page = TextPage {
        width,
        lines: Vec::new(),
    };
    let Some(r) = row_data else {
        page.blank();
        page.lines
            .push(centred("NO FEED SELECTED", width, Tt::Cyan, Tt::Black));
        return page.lines;
    };
    page.paragraph(&r.title, Tt::White, None);
    if !r.folder_path.is_empty() {
        page.paragraph(&r.folder_path, Tt::Cyan, None);
    }
    page.blank();
    let status = if r.buried {
        "Buried"
    } else {
        r.status().as_str()
    };
    let (fg, bg) = status_colours(status);
    page.lines.push(row(vec![span(
        &format!(" {}", status.to_uppercase()),
        width,
        fg,
        bg,
    )]));
    if r.buried {
        page.paragraph("Buried. Press k to restore it.", Tt::Yellow, None);
    }
    if let Some(d) = &r.diagnosis {
        page.paragraph(&d.reason, Tt::White, None);
    }
    if let Some(text) = rhythm.filter(|_| !r.buried) {
        page.blank();
        page.labeled("Rhythm", text);
    }
    if let Some(d) = r
        .diagnosis
        .as_ref()
        .filter(|d| !d.evidence.bullets.is_empty())
    {
        page.blank();
        page.heading("Evidence", Tt::Green);
        for bullet in &d.evidence.bullets {
            page.paragraph(bullet, Tt::White, Some(("- ".into(), Tt::Green)));
        }
    }
    if let Some(d) = r.diagnosis.as_ref().filter(|d| !d.remarks.is_empty()) {
        page.blank();
        page.heading("Remarks", Tt::Yellow);
        for remark in &d.remarks {
            page.paragraph(
                &remark_sentence(remark),
                Tt::White,
                Some(("- ".into(), Tt::Yellow)),
            );
        }
    }
    page.blank();
    page.heading("Links", Tt::Cyan);
    if let Some(action) = r.action().filter(|_| !r.buried) {
        page.labeled("Proposed", &action.label());
    }
    if let Some(record) = &r.decision {
        page.labeled(
            "Decision",
            match record.decision {
                Decision::Keep => "Keep",
                Decision::Repair => "Repair",
                Decision::Bury => "Bury",
            },
        );
    }
    page.labeled("Feed", &r.xml_url);
    for (label, url) in [("Site", &r.html_url), ("Wayback", &r.wayback_url)] {
        if let Some(url) = url.as_deref().filter(|u| is_web_url(u)) {
            page.labeled(label, url);
        }
    }
    if let Some(chain) = r
        .attempts
        .first()
        .map(|a| &a.redirect_chain)
        .filter(|c| !c.is_empty())
    {
        page.blank();
        page.heading("Redirects", Tt::Magenta);
        for hop in chain {
            page.paragraph(
                &hop.url,
                Tt::White,
                Some((format!("{} ", hop.status), Tt::Magenta)),
            );
        }
    }
    page.blank();
    page.heading("Fetch log", Tt::Green);
    if r.attempts.is_empty() {
        page.paragraph("Not fetched yet.", Tt::White, None);
    }
    for attempt in &r.attempts {
        let date = attempt
            .at
            .with_timezone(&offset)
            .format("%d %b %y %H:%M  ")
            .to_string()
            .to_uppercase();
        page.paragraph(&attempt_outcome(attempt), Tt::White, Some((date, Tt::Cyan)));
    }
    page.lines
}

/// Rows of a text page being built, each exactly `width` cells.
struct TextPage {
    width: usize,
    lines: Vec<Line<'static>>,
}

impl TextPage {
    fn blank(&mut self) {
        self.lines.push(blank(self.width));
    }

    fn heading(&mut self, title: &str, colour: Tt) {
        self.lines.push(row(vec![span(
            &title.to_uppercase(),
            self.width,
            colour,
            Tt::Black,
        )]));
    }

    /// `text` wrapped on words; `lead` (a label, a date, a dash) starts the first row and the
    /// following rows are indented by its width.
    fn paragraph(&mut self, text: &str, colour: Tt, lead: Option<(String, Tt)>) {
        let indent = lead
            .as_ref()
            .map_or(0, |(l, _)| Span::raw(l.as_str()).width())
            .min(self.width);
        for (index, piece) in wrap(text, self.width - indent).iter().enumerate() {
            let mut spans = Vec::new();
            if let Some((lead, lead_colour)) = &lead {
                spans.push(span(
                    if index == 0 { lead } else { "" },
                    indent,
                    *lead_colour,
                    Tt::Black,
                ));
            }
            spans.push(span(piece, self.width - indent, colour, Tt::Black));
            self.lines.push(row(spans));
        }
    }

    fn labeled(&mut self, label: &str, value: &str) {
        let lead = format!("{:<10}", label.to_uppercase());
        self.paragraph(value, Tt::White, Some((lead, Tt::Cyan)));
    }
}

/// The page in view, for the header and the index column's selection: P100 or P899 in the
/// reader, the end of the edition (P104), else the garden page or the source's page.
pub fn page_in_view(state: &ReaderState) -> Option<u16> {
    match state.antiope.reader_page() {
        ReaderPage::Sommaire => Some(100),
        ReaderPage::Guide => Some(899),
        ReaderPage::EndOfEdition => Some(104),
        ReaderPage::Article => match state.garden_view() {
            Some(view) => Some(state.directory.page_of_garden(view)),
            None => state.directory.page_of(state.source),
        },
    }
}

/// The header's label: the digits being typed first, then `NOT FOUND`, else the page in view.
pub fn header_label(state: &ReaderState) -> HeaderLabel {
    if let Some(typed) = state.antiope.typing() {
        return HeaderLabel::Typing(typed);
    }
    let now = state.now.unwrap_or(DateTime::UNIX_EPOCH);
    match state.antiope.not_found(now) {
        Some(page) => HeaderLabel::NotFound(page),
        None => HeaderLabel::Page(page_in_view(state)),
    }
}

/// Draws the reader in Antiope.
pub fn render(frame: &mut Frame, state: &ReaderState, images: &mut ImageStore) -> ReaderMetrics {
    let local = state
        .local
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.fixed_offset());
    render_at(frame, state, images, local, state.now)
}

/// `render` with the clock given: `local` shows in the header, `now` rolls the counter.
pub fn render_at(
    frame: &mut Frame,
    state: &ReaderState,
    images: &mut ImageStore,
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
    let dir = &state.directory;
    // The header: the page in view, or the counter searching while a job runs.
    let label = match (header_label(state), view.progress, now) {
        (HeaderLabel::Page(_), Some(_), Some(now)) => HeaderLabel::Searching(rolling_page(now)),
        (label, _, _) => label,
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
        let lines = entries_page(state, &view, rects.reader.is_none(), r);
        paint(frame, r, lines);
        if rects.reader.is_some() {
            separator(frame, r);
        }
    }
    let metrics = match rects.reader {
        Some(r) => reader_column(frame, r, state, &view, images),
        None => ReaderMetrics::default(),
    };
    if let Some(r) = rects.sources {
        let selected =
            page_in_view(state).and_then(|page| dir.rows().iter().position(|row| row.page == page));
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

/// The entries column: a garden page's feeds (its inspector instead, when the garden's detail is
/// open and no reader column is on screen), the Journal's edition, or the plain list of a source.
fn entries_page(
    state: &ReaderState,
    view: &ReaderView<'_>,
    alone: bool,
    area: Rect,
) -> Vec<Line<'static>> {
    let (width, height) = (usize::from(area.width), usize::from(area.height));
    if let Some(garden_view) = state.garden_view() {
        let Some(garden) = state.garden() else {
            return vec![centred("LOADING…", width, Tt::Cyan, Tt::Black)];
        };
        if alone && garden.mode == Mode::Detail {
            return garden_page(view, garden, width);
        }
        let (lines, selected) = garden_rows(garden, garden_view, width);
        return window(&lines, selected, height).to_vec();
    }
    entry_column(view, width, height)
}

/// The garden inspector of the selected feed, with the rhythm line the state holds for it.
fn garden_page(view: &ReaderView<'_>, garden: &GardenState, width: usize) -> Vec<Line<'static>> {
    let rhythm = view
        .rhythm_line
        .as_deref()
        .map(|line| line.trim_start_matches("Rhythm: "));
    garden_inspector(
        garden.selected_row(),
        rhythm,
        *view.now_local.offset(),
        width,
    )
}

/// The entry column of a reading source: the Journal's edition (spec §8) when it is showing, else
/// `●` yellow when unread, the title, `*` when starred, black on white when selected.
pub fn entry_column(view: &ReaderView<'_>, width: usize, height: usize) -> Vec<Line<'static>> {
    if view.journal_mode {
        let (lines, selected) = journal_rows(
            view.journal,
            view.journal_phase,
            &view.journal_lines,
            view.journal_cursor,
            *view.now_local.offset(),
            width,
        );
        return window(&lines, selected, height).to_vec();
    }
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

/// `104 3/43`: the open entry's place in its page, `None` while the source has no page number.
/// The Journal counts by rank in the edition, every other list by position.
fn subpage(view: &ReaderView<'_>, dir: &PageDirectory) -> Option<String> {
    let page = dir.page_of(view.source)?;
    let id = view.entry?.row.entry_id;
    let ranked = view.journal.filter(|_| view.journal_mode).and_then(|j| {
        j.items
            .iter()
            .find(|item| item.row.entry_id == id)
            .map(|item| (item.rank as usize, j.items.len()))
    });
    let (position, total) = ranked.or_else(|| {
        view.entry_cursor
            .map(|i| (i + 1, view.total.max(view.entries.len())))
    })?;
    Some(subpage_label(page, position, total))
}

/// The reader column: P100, P899, `FIN DE L'EDITION`, a garden page's inspector, or the article
/// under its sub-page line. With no entry to show it is P100, the page a Minitel starts on.
fn reader_column(
    frame: &mut Frame,
    area: Rect,
    state: &ReaderState,
    view: &ReaderView<'_>,
    images: &mut ImageStore,
) -> ReaderMetrics {
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    let dir = &state.directory;
    let page = state.antiope.reader_page();
    // A whole page of rows: scrolled only once it was chosen as a page.
    let whole = |frame: &mut Frame, lines: Vec<Line<'static>>, scrolls: bool| {
        let top = if scrolls {
            state.page_scroll.min(lines.len().saturating_sub(height))
        } else {
            0
        };
        let total = lines.len();
        paint(frame, area, lines.into_iter().skip(top).collect());
        ReaderMetrics {
            total_rows: total,
            viewport_rows: area.height,
            scroll: None,
        }
    };
    match page {
        ReaderPage::Sommaire => return whole(frame, sommaire_lines(dir, width), true),
        ReaderPage::Guide => return whole(frame, guide_lines(width), true),
        ReaderPage::EndOfEdition => {
            return whole(frame, vec![blank(width), end_line(width)], false);
        }
        ReaderPage::Article => {}
    }
    if let Some(garden) = state.garden() {
        return whole(frame, garden_page(view, garden, width), false);
    }
    if state.garden_view().is_some() {
        return whole(
            frame,
            vec![centred("LOADING…", width, Tt::Cyan, Tt::Black)],
            false,
        );
    }
    match (view.entry, view.article) {
        (Some(_), Some(article)) => {
            let mut lines = Vec::new();
            if let Some(label) = subpage(view, dir) {
                lines.push(row(vec![span(
                    &format!(" {label}"),
                    width,
                    Tt::Cyan,
                    Tt::Black,
                )]));
            }
            lines.extend(article_header(view, width));
            let header = lines.len().min(height);
            paint(frame, area, lines);
            let body = Rect {
                y: area.y + u16::try_from(header).unwrap_or(u16::MAX),
                height: area
                    .height
                    .saturating_sub(u16::try_from(header).unwrap_or(u16::MAX)),
                ..area
            };
            let variant = if state.antiope_real_images {
                Variant::Native
            } else {
                Variant::Teletext
            };
            let width16 = area.width;
            let drawn = draw_body(
                frame,
                body,
                article,
                view.images,
                view.reader_scroll,
                images,
                &BodyLook {
                    variant,
                    text_width: width16,
                    image_width: width16.min(MAX_TEXT_WIDTH),
                    paint_line: &|line| prose_row(line, width),
                    loading: row(vec![span(
                        &LOADING_IMAGE.to_uppercase(),
                        width,
                        Tt::White,
                        Tt::Black,
                    )]),
                    after_image: &|frame, place| {
                        if variant == Variant::Teletext {
                            snap_to_teletext(frame.buffer_mut(), place);
                        }
                    },
                },
            );
            ReaderMetrics {
                total_rows: drawn.total,
                viewport_rows: body.height,
                scroll: Some(drawn.scroll),
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
                scroll: None,
            }
        }
        _ => whole(frame, sommaire_lines(dir, width), true),
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
        EntryDetail, EntryPage, EntryRow, JournalItem, Source, SourceCounts, SourceNode,
        SourceNodeKind, SourcesSnapshot,
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

    fn press(s: &mut ReaderState, code: KeyCode) -> Vec<Command> {
        s.on_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_page(s: &mut ReaderState, digits: &str) -> Vec<Command> {
        let mut commands = Vec::new();
        for c in digits.chars() {
            commands = press(s, KeyCode::Char(c));
        }
        commands
    }

    fn antiope_state_on_page(page: u16) -> ReaderState {
        let mut s = antiope_state(140, 34);
        type_page(&mut s, &page.to_string());
        s
    }

    /// Feed 12's list with `rows`, the first unread one open in the reader.
    fn reading_state(width: u16, height: u16, rows: Vec<EntryRow>) -> ReaderState {
        reading_state_with(width, height, rows, ARTICLE, false)
    }

    fn reading_state_with(
        width: u16,
        height: u16,
        rows: Vec<EntryRow>,
        html: &str,
        images: bool,
    ) -> ReaderState {
        let mut s = antiope_state(width, height);
        s.settings.images = images;
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
            content_html: Some(html.into()),
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
                render_at(
                    f,
                    state,
                    &mut ImageStore::disabled(),
                    local(18, 33, 12),
                    state.now,
                );
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
        for page in [
            ReaderPage::Article,
            ReaderPage::Sommaire,
            ReaderPage::Guide,
            ReaderPage::EndOfEdition,
        ] {
            state.antiope.show(page);
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
            .draw(|f| {
                metrics = Some(render_at(
                    f,
                    &state,
                    &mut ImageStore::disabled(),
                    local(18, 33, 12),
                    state.now,
                ));
            })
            .unwrap();
        let m = metrics.unwrap();
        assert!(m.total_rows > 0);
        // The reader is 28 rows tall: the sub-page line and three header rows, the rest is the
        // viewport.
        assert_eq!(m.viewport_rows, 24);
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
    fn the_help_key_opens_the_guide_page_and_it_stays_until_another_page_is_chosen() {
        let mut state = reading_state(140, 30, rows());
        press(&mut state, KeyCode::Char('?'));
        let out = draw(&state, 140, 30);
        assert!(out.contains("SUITE: next page"), "{out}");
        assert!(row_text(&draw_terminal(&state, 140, 30), 0).starts_with("P899 RESSAC"));
        assert_eq!(state.focus(), Column::Reader);
        // Any other key leaves it where it is; choosing a page replaces it.
        press(&mut state, KeyCode::Char('x'));
        assert!(draw(&state, 140, 30).contains("SUITE: next page"));
        type_page(&mut state, "412");
        assert!(!draw(&state, 140, 30).contains("SUITE: next page"));
        assert!(row_text(&draw_terminal(&state, 140, 30), 0).starts_with("P412 RESSAC"));
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

    #[test]
    fn a_garden_page_is_drawn_in_teletext_and_its_selected_feed_is_on_screen() {
        let mut state = antiope_state(140, 30);
        type_page(&mut state, "800");
        state.on_event(Event::GardenRows(crate::tui::state::sample_rows()));
        let terminal = draw_terminal(&state, 140, 30);
        let out = terminal.backend().to_string();
        assert!(out.contains("NEEDS TRIAGE"), "{out}");
        assert!(out.contains("Bibliobsession 2.0"), "{out}");
        assert!(row_text(&terminal, 0).starts_with("P800 RESSAC"));
        // The inspector of the selected feed fills the reader column, its status on its own row.
        assert!(
            out.contains("HIJACKED") && out.contains("EVIDENCE"),
            "{out}"
        );
        // And the key that acts on it is the garden's.
        let commands = press(&mut state, KeyCode::Char('k'));
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

    /// P899 and P100 are pages of the reader: SUITE scrolls them on a short pane, and past their
    /// bottom it leaves them for the entry, as in the app.
    #[test]
    fn the_guide_and_the_sommaire_scroll_on_a_short_pane() {
        let mut state = reading_state(140, 30, rows());
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        let mut frame = |state: &mut ReaderState| {
            let mut m = ReaderMetrics::default();
            terminal
                .draw(|f| {
                    m = render_at(
                        f,
                        state,
                        &mut ImageStore::disabled(),
                        local(18, 33, 12),
                        state.now,
                    );
                })
                .unwrap();
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
            terminal.backend().to_string()
        };
        press(&mut state, KeyCode::Char('?'));
        let first = frame(&mut state);
        assert!(
            first.contains("PAGES") && !first.contains("NATIVE or ANTIOPE"),
            "{first}"
        );
        let mut presses = 0;
        while !frame(&mut state).contains("quit") {
            press(&mut state, KeyCode::Char('j'));
            presses += 1;
            assert!(presses < 40, "the guide never ends");
        }
        assert!(frame(&mut state).contains("NATIVE or ANTIOPE"));
        // `k` scrolls back; SUITE at the bottom brings the entry back.
        let bottom = state.page_scroll;
        press(&mut state, KeyCode::Char('k'));
        assert_eq!(state.page_scroll, bottom - 1);
        press(&mut state, KeyCode::Char(' '));
        frame(&mut state);
        assert_eq!(state.page_scroll, bottom);
        press(&mut state, KeyCode::Char('j'));
        let article = frame(&mut state);
        assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
        assert_eq!(state.page_scroll, 0);
        assert!(article.contains("Le jardin des flux"), "{article}");
        // The Sommaire too: a long page list is reachable through page navigation.
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
        state.on_event(Event::Pages(pages));
        press(&mut state, KeyCode::Char('S'));
        let top = frame(&mut state);
        assert!(!top.contains("Feed 59"), "{top}");
        press(&mut state, KeyCode::Char(' '));
        assert!(frame(&mut state).contains("Feed 59"));
    }

    #[test]
    fn a_tick_moves_the_header_clock_through_production_render() {
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

    // P104, the garden pages, the prose and the images.

    const CAFE: &str = "Café ☕ pédago";

    fn item(
        id: i64,
        feed: i64,
        title: &str,
        section: Section,
        rank: u32,
        read: bool,
    ) -> JournalItem {
        JournalItem {
            row: EntryRow {
                feed_id: feed,
                is_starred: false,
                ..entry(id, title, read)
            },
            section,
            rank,
        }
    }

    fn journal_view(read: bool) -> JournalView {
        JournalView {
            edition: ressac_core::store::EditionRecord {
                id: 1,
                built_at: Utc.with_ymd_and_hms(2026, 10, 16, 6, 10, 0).unwrap(),
                window_start: Utc.with_ymd_and_hms(2026, 10, 15, 6, 10, 0).unwrap(),
                window_end: Utc.with_ymd_and_hms(2026, 10, 16, 6, 10, 0).unwrap(),
                unreachable: 2,
            },
            items: vec![
                item(1, 3, "À la une", Section::Front, 1, read),
                item(2, 7, "Un article de la semaine", Section::Week, 2, read),
                item(3, 7, "Un autre article", Section::Week, 3, read),
                item(4, 8, &format!("{CAFE} · édition"), Section::Week, 4, read),
                item(5, 9, "Le fil", Section::Wire, 5, read),
            ],
            overflow: vec![
                EditionOverflowRow {
                    section: Section::Week,
                    feed_id: 7,
                    feed_title: CAFE.into(),
                    page: Some(403),
                    hidden_count: 12,
                },
                EditionOverflowRow {
                    section: Section::Wire,
                    feed_id: 4,
                    feed_title: "Actualitté".into(),
                    page: None,
                    hidden_count: 2,
                },
            ],
        }
    }

    /// The Journal's page with `read` entries, the first one open.
    fn journal_state(width: u16, height: u16, read: bool) -> ReaderState {
        let mut s = antiope_state(width, height);
        let j = journal_view(read);
        let rows: Vec<EntryRow> = j.items.iter().map(|i| i.row.clone()).collect();
        s.on_event(Event::Journal(Some(j)));
        s.on_event(Event::Entries {
            source: Source::Journal,
            page: EntryPage {
                total: rows.len(),
                unread: rows.iter().filter(|r| !r.is_read).count(),
                rows: rows.clone(),
                next: None,
            },
            appended: false,
            request: s.request,
        });
        s.on_event(Event::Entry(Box::new(EntryDetail {
            row: rows[0].clone(),
            summary_html: None,
            content_html: Some(ARTICLE.into()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        })));
        s
    }

    fn journal_text(state: &ReaderState, width: usize) -> Vec<Line<'static>> {
        let view = state.view();
        journal_rows(
            view.journal,
            view.journal_phase,
            &view.journal_lines,
            view.journal_cursor,
            *view.now_local.offset(),
            width,
        )
        .0
    }

    #[test]
    fn p104_has_coloured_sections_and_white_on_blue_overflow() {
        let state = journal_state(140, 34, false);
        let lines = journal_text(&state, 28);
        let find = |needle: &str| {
            lines
                .iter()
                .find(|l| text(l).contains(needle))
                .unwrap_or_else(|| {
                    panic!(
                        "{needle} in {:?}",
                        lines.iter().map(text).collect::<Vec<_>>()
                    )
                })
        };
        assert_eq!(
            find("FRONT PAGE").spans[0].style.fg,
            Some(Tt::Yellow.color())
        );
        assert_eq!(find("THIS WEEK").spans[0].style.fg, Some(Tt::Cyan.color()));
        assert_eq!(find("THE WIRE").spans[0].style.fg, Some(Tt::Green.color()));
        let more = find("+12 MORE IN CAFÉ ☕ PÉDAGO");
        assert_eq!(more.spans[1].style.bg, Some(Tt::Blue.color()));
        assert_eq!(more.spans[1].style.fg, Some(Tt::White.color()));
        // The overflow row sits under its feed's last entry, the cap-0 one at the section's end.
        let order: Vec<String> = lines.iter().map(|l| text(l).trim().to_string()).collect();
        let at = |needle: &str| order.iter().position(|t| t.contains(needle)).unwrap();
        assert!(at("Un autre article") + 1 == at("+12 MORE"));
        assert!(at("Le fil") + 1 == at("+2 MORE IN ACTUALITTÉ"));
        assert!(
            text(&lines[0]).starts_with(" FRI 16 OCT") && text(&lines[0]).ends_with(" 0/5 READ")
        );
        assert_eq!(text(&lines[1]).trim(), "2 FEEDS UNREACHABLE");
        assert_eq!(lines[1].spans[0].style.fg, Some(Tt::Red.color()));
        // The count by section, at the right edge.
        assert!(text(find("THIS WEEK")).trim_end().ends_with(" 3"));
    }

    /// Review Focus 1 and 5: every row is exactly its column wide, with an emoji in the title.
    #[test]
    fn every_antiope_row_is_exactly_its_width_with_cafe_pedago() {
        let state = journal_state(140, 34, false);
        let mut garden_rows = crate::tui::state::sample_rows();
        garden_rows[0].title = CAFE.into();
        garden_rows[0].folder_path = format!("{CAFE} / Infos");
        let garden = GardenState::new(garden_rows, false);
        for width in [1usize, 2, 3, 5, 22, 28, 40, 72] {
            let mut all: Vec<Line<'static>> = journal_text(&state, width);
            all.push(entry_line(Some(CAFE), false, true, width, false));
            all.push(entry_line(Some(CAFE), true, false, width, true));
            all.push(overflow_line(
                &journal_view(false).overflow[0],
                width,
                false,
            ));
            all.push(overflow_line(&journal_view(false).overflow[0], width, true));
            all.push(end_line(width));
            all.extend(garden_rows_for(&garden, width));
            all.extend(garden_inspector(
                garden.selected_row(),
                Some("Daily · ☕"),
                FixedOffset::east_opt(7200).unwrap(),
                width,
            ));
            for (i, line) in all.iter().enumerate() {
                assert_eq!(line.width(), width, "row {i} at {width}: {:?}", text(line));
            }
        }
    }

    fn garden_rows_for(garden: &GardenState, width: usize) -> Vec<Line<'static>> {
        garden_rows(garden, GardenView::NeedsTriage, width).0
    }

    #[test]
    fn p104_while_building_offline_failed_or_empty() {
        let mut state = journal_state(140, 34, false);
        let offset = FixedOffset::east_opt(7200).unwrap();
        let rows = |journal: Option<&JournalView>, phase: &JournalPhase| {
            journal_rows(journal, phase, &[], None, offset, 28).0
        };
        let building = rows(
            None,
            &JournalPhase::Building {
                done: 34,
                total: 156,
            },
        );
        assert_eq!(text(&building[0]).trim(), "");
        assert!(text(&building[1]).contains("EDITION IN PROGRESS 34/156"));
        let offline = rows(None, &JournalPhase::Offline);
        assert!(text(&offline[1]).contains("OFFLINE: NO EDITION YET"));
        assert_eq!(offline[1].spans[1].style.bg, Some(Tt::Red.color()));
        assert!(text(&offline[2]).contains("RÉPÉTITION: RETRY"));
        let failed = rows(
            None,
            &JournalPhase::Failed("The database is busy (is Ressac.app writing?)".into()),
        );
        assert!(failed.len() > 2, "a long reason wraps: {failed:?}");
        assert!(
            failed
                .iter()
                .skip(1)
                .all(|l| l.width() == 28 && l.spans[1].style.bg == Some(Tt::Red.color()))
        );
        let none = rows(None, &JournalPhase::Idle);
        assert!(text(&none[1]).contains("EDITION IN PROGRESS"));
        let mut empty = journal_view(false);
        empty.items.clear();
        empty.overflow.clear();
        let nothing = rows(Some(&empty), &JournalPhase::Idle);
        assert!(
            nothing
                .iter()
                .any(|l| text(l).contains("NOTHING NEW THIS MORNING."))
        );
        // And the page itself while the build runs.
        state.on_event(Event::Progress {
            label: BUILDING.into(),
            done: 3,
            total: 9,
        });
        assert!(draw(&state, 140, 34).contains("EDITION IN PROGRESS 3/9"));
    }

    #[test]
    fn fin_de_l_edition_closes_a_read_edition_with_a_yellow_bar() {
        let state = journal_state(140, 34, true);
        let lines = journal_text(&state, 28);
        let last = lines.last().unwrap();
        assert!(text(last).contains("FIN DE L'EDITION"));
        assert!(
            last.spans
                .iter()
                .all(|s| s.style.bg == Some(Tt::Yellow.color()))
        );
        assert_eq!(last.width(), 28);
        // An unread edition has no end yet.
        assert!(
            !journal_text(&journal_state(140, 34, false), 28)
                .iter()
                .any(|l| text(l).contains("FIN DE"))
        );
    }

    #[test]
    fn the_cursor_row_is_black_on_white_and_stays_in_view() {
        let mut state = journal_state(140, 12, false);
        for _ in 0..3 {
            press(&mut state, KeyCode::Char('j'));
        }
        let terminal = draw_terminal(&state, 140, 12);
        let buffer = terminal.backend().buffer();
        let selected: Vec<u16> = (1..11)
            .filter(|y| buffer[(23, *y)].bg == Tt::White.color())
            .collect();
        assert_eq!(selected.len(), 1, "one selected row in view");
        let selected_row = row_text(&terminal, selected[0]);
        assert!(selected_row.contains("pédago · édition"), "{selected_row}");
    }

    #[test]
    fn the_sub_page_line_counts_by_rank_in_the_journal_and_by_position_elsewhere() {
        let state = journal_state(140, 34, false);
        let terminal = draw_terminal(&state, 140, 34);
        assert!(
            row_text(&terminal, 1).contains(" 104 1/5"),
            "{}",
            row_text(&terminal, 1)
        );
        assert_eq!(terminal.backend().buffer()[(53, 1)].fg, Tt::Cyan.color());
        let feed = reading_state(140, 30, rows());
        assert!(draw(&feed, 140, 30).contains(" 412 1/3"));
        // No page yet (the list of pages has not arrived): no sub-page line, nothing else moves.
        let mut bare = reading_state(140, 30, rows());
        bare.on_event(Event::Pages(Vec::new()));
        assert!(!draw(&bare, 140, 30).contains("1/3"));
    }

    #[test]
    fn antiope_journal_at_140_and_70_columns() {
        let state = journal_state(140, 34, false);
        insta::assert_snapshot!("antiope_journal_140", draw(&state, 140, 34));
        let mut narrow = journal_state(70, 34, false);
        narrow.set_focus(Column::Entries);
        insta::assert_snapshot!("antiope_journal_70", draw(&narrow, 70, 34));
    }

    #[test]
    fn suite_from_the_last_entry_lands_on_the_end_page_and_retour_comes_back() {
        let mut state = journal_state(140, 34, false);
        for _ in 0..4 {
            press(&mut state, KeyCode::Char('j'));
        }
        assert_eq!(state.selected_entry(), Some(5));
        press(&mut state, KeyCode::Char('j'));
        assert_eq!(state.antiope.reader_page(), ReaderPage::EndOfEdition);
        assert!(
            state.entries[4].is_read,
            "the last entry is read on landing"
        );
        let terminal = draw_terminal(&state, 140, 34);
        assert!(row_text(&terminal, 0).starts_with("P104 RESSAC"));
        assert!(
            row_text(&terminal, 2).contains("FIN DE L'EDITION"),
            "{}",
            row_text(&terminal, 2)
        );
        assert_eq!(terminal.backend().buffer()[(53, 2)].bg, Tt::Yellow.color());
        insta::assert_snapshot!("antiope_fin_140", terminal.backend().to_string());
        assert!(press(&mut state, KeyCode::Char('j')).is_empty());
        press(&mut state, KeyCode::Char('k'));
        assert_eq!(state.antiope.reader_page(), ReaderPage::Article);
        assert_eq!(state.selected_entry(), Some(5));
    }

    #[test]
    fn garden_headings_use_the_status_colours_and_fossil_is_white_on_blue() {
        let dead = garden_heading("Dead", 3, 28);
        assert_eq!(dead.spans[0].style.fg, Some(Tt::Red.color()));
        assert_eq!(dead.spans[0].style.bg, Some(Tt::Black.color()));
        let fossil = garden_heading("Fossil", 2, 28);
        assert_eq!(fossil.spans[0].style.bg, Some(Tt::Blue.color()));
        assert_eq!(fossil.spans[0].style.fg, Some(Tt::White.color()));
        assert_eq!(text(&fossil).trim_end(), " FOSSIL                    2");
    }

    fn garden_state(width: u16, height: u16, page: &str) -> ReaderState {
        let mut s = antiope_state(width, height);
        type_page(&mut s, page);
        s.on_event(Event::GardenRows(crate::tui::state::sample_rows()));
        s
    }

    #[test]
    fn the_garden_pages_list_feeds_under_status_headings_with_the_progress_gauge() {
        let state = garden_state(140, 34, "800");
        let terminal = draw_terminal(&state, 140, 34);
        let column =
            |y: u16| -> String { row_text(&terminal, y).chars().skip(23).take(28).collect() };
        let top = column(1);
        assert!(
            top.starts_with("NEEDS TRIAGE") && top.trim_end().ends_with("LEFT"),
            "{top}"
        );
        let gauge = column(2);
        assert!(gauge.starts_with('▐') && gauge.contains('/'), "{gauge}");
        let out = terminal.backend().to_string();
        for heading in ["HIJACKED", "DEAD"] {
            assert!(out.contains(heading), "{heading}: {out}");
        }
        // The cursor row is black on white and the feed's chip is on its right.
        let selected = (3..33)
            .find(|y| terminal.backend().buffer()[(24, *y)].bg == Tt::White.color())
            .unwrap();
        assert!(column(selected).contains("Bibliobsession 2.0"));
        assert!(column(selected).trim_end().ends_with("bury"));
        insta::assert_snapshot!("antiope_garden_800_140", terminal.backend().to_string());
        let all = garden_state(140, 34, "801");
        let all_terminal = draw_terminal(&all, 140, 34);
        let all_top: String = row_text(&all_terminal, 1)
            .chars()
            .skip(23)
            .take(28)
            .collect();
        assert!(all_top.contains("FEEDS"), "{all_top}");
        let cemetery = garden_state(140, 34, "802");
        let out = draw(&cemetery, 140, 34);
        assert!(
            out.contains("CEMETERY") && out.contains("THE CEMETERY IS EMPTY"),
            "{out}"
        );
    }

    #[test]
    fn the_garden_keys_work_and_the_bar_asks_before_applying_repairs() {
        let mut state = garden_state(140, 34, "800");
        press(&mut state, KeyCode::Down);
        assert!(state.garden().unwrap().selected > 0);
        press(&mut state, KeyCode::Char('R'));
        let bar = row_text(&draw_terminal(&state, 140, 34), 33);
        assert!(bar.contains("SAFE REPAIRS? Y APPLY"), "{bar}");
        press(&mut state, KeyCode::Char('n'));
        press(&mut state, KeyCode::Char('f'));
        let bar = row_text(&draw_terminal(&state, 140, 34), 33);
        assert!(bar.contains("FILTER:"), "{bar}");
        // `?` is the GUIDE, which has the garden's keys, and it leaves the garden.
        press(&mut state, KeyCode::Char('?'));
        assert!(state.garden().is_none());
        assert_eq!(state.antiope.reader_page(), ReaderPage::Guide);
        assert!(draw(&state, 140, 34).contains("GARDEN"));
    }

    #[test]
    fn the_inspector_walks_the_evidence_links_and_fetch_log() {
        let state = garden_state(140, 60, "800");
        let out = draw(&state, 140, 60);
        for needle in [
            "bibliobsession 2.0",
            "hijacked",
            "evidence",
            "links",
            "fetch log",
            "redirects",
            "301",
            "feed",
        ] {
            assert!(out.to_lowercase().contains(needle), "{needle}: {out}");
        }
    }

    #[test]
    fn the_garden_detail_fills_a_single_column() {
        let mut state = garden_state(70, 34, "800");
        state.set_focus(Column::Entries);
        let list = draw(&state, 70, 34);
        assert!(!list.contains("EVIDENCE"), "{list}");
        press(&mut state, KeyCode::Enter);
        let detail = draw(&state, 70, 34);
        assert!(detail.contains("EVIDENCE"), "{detail}");
    }

    #[test]
    fn article_text_keeps_its_emphasis_as_teletext_colours() {
        let html = "<h2>Titre</h2><p>un <strong>fort</strong> <em>doux</em> <a href=\"https://x.test/\">lien</a> <code>code</code> <del>vieux</del> simple</p>";
        let state = reading_state_with(140, 30, rows(), html, false);
        let terminal = draw_terminal(&state, 140, 30);
        let buffer = terminal.backend().buffer();
        let colours = |word: &str| {
            let y = (0..30)
                .find(|y| row_text(&terminal, *y).contains(word))
                .unwrap_or_else(|| panic!("{word}"));
            let x = row_text(&terminal, y).find(word).unwrap();
            let x = row_text(&terminal, y)[..x].chars().count() as u16;
            (buffer[(x, y)].fg, buffer[(x, y)].bg)
        };
        let (black, white) = (Tt::Black.color(), Tt::White.color());
        assert_eq!(colours("Titre"), (Tt::Green.color(), black));
        assert_eq!(colours("fort"), (Tt::Yellow.color(), black));
        assert_eq!(colours("doux"), (Tt::Magenta.color(), black));
        assert_eq!(colours("lien"), (Tt::Cyan.color(), black));
        assert_eq!(colours("code"), (black, white));
        assert_eq!(colours("vieux"), (Tt::Red.color(), black));
        assert_eq!(colours("simple"), (white, black));
        for line in &state.article.as_ref().unwrap().lines {
            assert_eq!(prose_row(line, 31).width(), 31);
            assert_eq!(prose_row(line, 3).width(), 3);
        }
    }

    #[test]
    fn hostile_article_text_stays_in_its_column_in_the_reader() {
        let html = format!(
            "<p>Café ☕ 東京 {}]0;x{} fin {}31m</p>",
            '\x1b', '\x07', '\u{9b}'
        );
        for width in [140u16, 100, 70] {
            let state = reading_state_with(width, 30, rows(), &html, false);
            let out = draw(&state, width, 30);
            assert!(
                !out.chars()
                    .any(|c| c == '\x1b' || c == '\x07' || c == '\u{9b}'),
                "{out}"
            );
        }
    }

    const PHOTO: &str = "https://example.com/p.png";

    /// Red above, blue below: half-blocks have two colours to draw, and neither is a teletext
    /// colour in this photo's real version.
    fn photo() -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 300, |_, y| {
            if y < 150 {
                image::Rgb([200, 120, 40])
            } else {
                image::Rgb([40, 60, 200])
            }
        }))
    }

    fn image_state(real: bool) -> ReaderState {
        let html = format!("<p>Avant</p><img src=\"{PHOTO}\" alt=\"Le port\"><p>Après</p>");
        let mut s = reading_state_with(140, 30, rows(), &html, true);
        s.antiope_real_images = real;
        s
    }

    fn draw_with(state: &ReaderState, images: &mut ImageStore) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        terminal
            .draw(|f| {
                render(f, state, images);
            })
            .unwrap();
        terminal
    }

    #[test]
    fn antiope_images_are_wanted_then_drawn_as_teletext_mosaics() {
        use ratatui_image::picker::Picker;
        let state = image_state(false);
        let mut images = ImageStore::new(Picker::halfblocks());
        let loading = draw_with(&state, &mut images).backend().to_string();
        assert!(loading.contains("[LOADING IMAGE…]"), "{loading}");
        assert_eq!(images.take_wanted(), vec![PHOTO.to_string()]);
        images.ingest(PHOTO, photo());
        let terminal = draw_with(&state, &mut images);
        let out = terminal.backend().to_string();
        assert!(out.contains('▀') && !out.contains("LOADING IMAGE"), "{out}");
        // Every cell, the picture's included, is one of the eight colours.
        let palette: Vec<Color> = Tt::ALL.iter().map(|c| c.color()).collect();
        let buffer = terminal.backend().buffer();
        for y in 0..30 {
            for x in 0..140 {
                let cell = &buffer[(x, y)];
                assert!(
                    palette.contains(&cell.fg) && palette.contains(&cell.bg),
                    "{x},{y}: {cell:?}"
                );
            }
        }
        // The picture sits in the reader column, whole, below the header.
        assert!(images.rows_for(PHOTO, Variant::Teletext, 72, 10) <= 10);
    }

    #[test]
    fn i_swaps_the_mosaic_for_the_real_image() {
        use ratatui_image::picker::Picker;
        let mut state = image_state(false);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.ingest(PHOTO, photo());
        press(&mut state, KeyCode::Char('i'));
        assert!(state.antiope_real_images);
        assert!(
            state.settings.images,
            "the setting of the Native look is not touched"
        );
        let terminal = draw_with(&state, &mut images);
        let buffer = terminal.backend().buffer();
        let real = Color::Rgb(200, 120, 40);
        assert!(
            (0..30)
                .any(|y| (0..140).any(|x| buffer[(x, y)].fg == real || buffer[(x, y)].bg == real))
        );
        press(&mut state, KeyCode::Char('i'));
        assert!(!state.antiope_real_images);
        // `i` does nothing on a special page.
        press(&mut state, KeyCode::Char('S'));
        press(&mut state, KeyCode::Char('i'));
        assert!(!state.antiope_real_images);
    }

    #[test]
    fn images_off_or_failed_keep_the_alt_text() {
        use ratatui_image::picker::Picker;
        let state = image_state(false);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.fail(PHOTO);
        assert!(
            draw_with(&state, &mut images)
                .backend()
                .to_string()
                .contains("[image: Le port]")
        );
        let off = reading_state_with(
            140,
            30,
            rows(),
            &format!("<img src=\"{PHOTO}\" alt=\"Le port\">"),
            false,
        );
        let mut fresh = ImageStore::new(Picker::halfblocks());
        assert!(
            draw_with(&off, &mut fresh)
                .backend()
                .to_string()
                .contains("[image: Le port]")
        );
        assert!(fresh.take_wanted().is_empty());
    }

    #[test]
    fn the_header_label_types_then_reports_an_unknown_page_for_two_seconds() {
        let mut state = antiope_state(140, 34);
        assert_eq!(header_label(&state), HeaderLabel::Page(Some(104)));
        press(&mut state, KeyCode::Char('4'));
        assert_eq!(header_label(&state), HeaderLabel::Typing("4__".into()));
        assert!(row_text(&draw_terminal(&state, 140, 34), 0).starts_with("P4__ RESSAC"));
        press(&mut state, KeyCode::Backspace);
        assert_eq!(header_label(&state), HeaderLabel::Page(Some(104)));
        type_page(&mut state, "437");
        assert_eq!(header_label(&state), HeaderLabel::NotFound(437));
        let terminal = draw_terminal(&state, 140, 34);
        assert!(row_text(&terminal, 0).starts_with("P437 NOT FOUND"));
        assert_eq!(terminal.backend().buffer()[(0, 0)].fg, Tt::Red.color());
        state.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 16, 33, 13).unwrap(),
            local: local(18, 33, 13),
        });
        assert_eq!(
            header_label(&state),
            HeaderLabel::NotFound(437),
            "one second later"
        );
        state.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 16, 33, 14).unwrap(),
            local: local(18, 33, 14),
        });
        assert_eq!(header_label(&state), HeaderLabel::Page(Some(104)));
    }

    #[test]
    fn wrapping_counts_display_cells_and_cuts_long_words() {
        assert_eq!(wrap("un deux trois", 7), ["un deux", "trois"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("☕☕☕", 4), ["☕☕", "☕"]);
        assert_eq!(wrap("☕", 1), ["☕"]);
        assert!(wrap("x", 0).is_empty());
        assert_eq!(wrap("a\u{1b}b", 5), ["a\u{FFFD}b"]);
    }

    #[test]
    fn the_pages_of_the_rhythm_groups_and_folders_are_ordinary_lists() {
        let mut state = antiope_state(140, 34);
        let commands = type_page(&mut state, "105");
        assert_eq!(state.source(), Source::Cadence(CadenceGroup::Daily));
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::LoadEntries {
                source: Source::Cadence(_),
                ..
            }
        )));
        assert!(row_text(&draw_terminal(&state, 140, 34), 0).starts_with("P105 RESSAC"));
        type_page(&mut state, "400");
        assert_eq!(state.source(), Source::Folder(4));
        assert!(row_text(&draw_terminal(&state, 140, 34), 0).starts_with("P400 RESSAC"));
    }

    #[test]
    fn the_p100_fallback_scrolls_and_hands_the_keys_back_at_its_ends() {
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
        // No entry is open: the reader shows P100 on a short pane.
        let mut state = antiope_state(140, 20);
        state.on_event(Event::Pages(pages));
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        let mut frame = |state: &mut ReaderState| {
            let mut m = ReaderMetrics::default();
            terminal
                .draw(|f| {
                    m = render_at(
                        f,
                        state,
                        &mut ImageStore::disabled(),
                        local(18, 33, 12),
                        state.now,
                    );
                })
                .unwrap();
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
            terminal.backend().to_string()
        };
        assert!(!frame(&mut state).contains("Feed 59"));
        let mut presses = 0;
        while !frame(&mut state).contains("Feed 59") {
            assert!(press(&mut state, KeyCode::Char(' ')).is_empty());
            presses += 1;
            assert!(presses < 6, "the last line is reachable");
        }
        assert!(state.page_scroll > 0);
        // At the bottom the key is the list's again: it opens the first entry.
        state.on_event(Event::Entries {
            source: Source::Journal,
            page: EntryPage {
                total: 1,
                unread: 1,
                rows: vec![entry(11, "Un", false)],
                next: None,
            },
            appended: false,
            request: state.request,
        });
        state.selected = None;
        let commands = press(&mut state, KeyCode::Char('j'));
        assert!(commands.contains(&Command::LoadEntry(11)), "{commands:?}");
        assert_eq!(state.page_scroll, 0, "a new entry starts the page over");
    }

    #[test]
    fn i_with_images_off_turns_them_on_as_mosaics() {
        let mut state = reading_state(140, 30, rows());
        assert!(!state.settings.images);
        assert_eq!(
            press(&mut state, KeyCode::Char('i')),
            [Command::SaveSettings]
        );
        assert!(state.settings.images && !state.antiope_real_images);
        assert!(press(&mut state, KeyCode::Char('i')).is_empty());
        assert!(state.antiope_real_images);
    }
}
