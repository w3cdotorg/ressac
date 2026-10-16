//! The Native look of the reader: sources, entries and the article, folded to the pane's width.
//! Pure drawing: the state is read through `ReaderState::view`, nothing is mutated, and the
//! article's measured height goes back to the loop as `ReaderMetrics`. Dates come from the
//! state's local clock, never from the machine's.

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;
use ressac_core::store::{EntryDetail, EntryRow, ListFilter};

use super::html::{ImageRef, MAX_TEXT_WIDTH};
use super::images::{ImageState, ImageStore, Variant};
use super::layout::{Column, LayoutClass, columns};
use super::state::{RHYTHM_CHOICES, ReaderState, ReaderView, SourceLine, group_label};
use crate::output::printable;

pub const EDITION_PROGRESS_LABEL: &str = "Building this morning's edition…";
pub const FETCH_PROGRESS_LABEL: &str = "Fetching feeds";
pub const EXTRACT_PROGRESS_LABEL: &str = "Fetching the full article";
pub const TOO_SMALL: &str = "Window too small";
pub const LOADING_IMAGE: &str = "[loading image…]";
const HELP_HINT: &str = "? help";
const EDITING_HINT: &str = "Enter apply  Esc cancel";

/// Rows of the article header: the title; feed, author and date; the entry's state.
const HEADER_ROWS: u16 = 3;

/// What the loop gives back to the state after a draw (Space needs the article's height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReaderMetrics {
    /// Article rows (the numbered links list is part of them).
    pub total_rows: usize,
    /// Rows visible under the header.
    pub viewport_rows: u16,
}

/// The progress line, shortened when the space is short: the edition's label becomes
/// `Edition… n/N`; other labels keep their first word and an ellipsis.
pub fn progress_text(label: &str, done: usize, total: usize, width: u16) -> String {
    let full = format!("{label} {done}/{total}");
    if full.chars().count() <= usize::from(width) {
        return full;
    }
    if label == EDITION_PROGRESS_LABEL {
        return format!("Edition… {done}/{total}");
    }
    let first = label.split_whitespace().next().unwrap_or(label);
    format!("{first}…")
}

const FOCUS: Style = Style::new().fg(Color::Cyan);
const DIM: Style = Style::new().fg(Color::DarkGray);
const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

fn frame_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(if focused { FOCUS } else { DIM })
        .title(Span::raw(format!(" {title} ")))
}

pub fn render(frame: &mut Frame, state: &ReaderState, images: &mut ImageStore) -> ReaderMetrics {
    let view = state.view();
    let area = frame.area();
    if view.layout == LayoutClass::TooSmall {
        let line = Rect {
            y: area.y + area.height / 2,
            height: area.height.min(1),
            ..area
        };
        frame.render_widget(Paragraph::new(TOO_SMALL).alignment(Alignment::Center), line);
        return ReaderMetrics::default();
    }
    let rects = columns(area, view.layout, view.focus, view.look);
    render_top_line(frame, &view, rects.top);
    frame.render_widget(bottom_line(&view, rects.bottom.width), rects.bottom);
    if let Some(rect) = rects.entries {
        render_entries(frame, &view, rect, view.focus == Column::Entries);
    }
    let metrics = match rects.reader {
        Some(rect) => render_reader(frame, &view, images, rect, view.focus == Column::Reader),
        None => ReaderMetrics::default(),
    };
    // Last: in the two-column class the sources slide over the entries.
    if let Some(rect) = rects.sources {
        frame.render_widget(Clear, rect);
        render_sources(frame, &view, rect, view.focus == Column::Sources);
    }
    if view.help {
        render_help(frame, area);
    }
    if let Some((choice, _)) = view.rhythm {
        render_rhythm(frame, area, choice);
    }
    metrics
}

fn render_top_line(frame: &mut Frame, view: &ReaderView<'_>, area: Rect) {
    let filter = match view.filter {
        ListFilter::All => "all",
        ListFilter::Unread => "unread only",
    };
    let mut left = format!("{} · {} unread · {filter}", view.source_title, view.unread);
    if let Some(text) = view.search {
        left.push_str(&format!(" · /{}", printable(text)));
    }
    let right = match (view.progress, view.message) {
        (Some((label, done, total)), _) => progress_text(label, done, total, area.width / 2),
        (None, Some(message)) => printable(message).into_owned(),
        (None, None) => String::new(),
    };
    // The right part gets what it measures (display cells, not chars), at most half the line.
    let right = Line::styled(right, Style::new().fg(Color::Yellow));
    let right_width = u16::try_from(right.width())
        .unwrap_or(u16::MAX)
        .min(area.width / 2);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);
    frame.render_widget(Paragraph::new(Line::styled(left, BOLD)), left_area);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        right_area,
    );
}

/// The keys of the focused column, the last hint (`? help`) always kept.
fn bottom_line(view: &ReaderView<'_>, width: u16) -> Paragraph<'static> {
    if view.editing {
        return Paragraph::new(Span::styled(EDITING_HINT, DIM));
    }
    let hints: &[&str] = match view.focus {
        Column::Sources => &["↑↓ source", "→ list", "r refresh", "t Antiope", "q quit"],
        Column::Entries => &[
            "j/k entry",
            "Space next",
            "m read",
            "s star",
            "A all read",
            "u unread",
            "/ search",
        ],
        Column::Reader => &[
            "Space scroll",
            "j/k entry",
            "f full text",
            "o open",
            "i images",
        ],
    };
    let mut line = String::new();
    for hint in hints {
        let fits =
            line.chars().count() + hint.chars().count() + 2 + HELP_HINT.len() <= usize::from(width);
        if !fits {
            break;
        }
        line.push_str(hint);
        line.push_str("  ");
    }
    line.push_str(HELP_HINT);
    Paragraph::new(Span::styled(line, DIM))
}

fn render_sources(frame: &mut Frame, view: &ReaderView<'_>, area: Rect, focused: bool) {
    let items: Vec<ListItem> = view.source_lines.iter().map(source_item).collect();
    let selected = view.source_lines.iter().position(|l| l.selected);
    let mut list_state = ListState::default().with_selected(selected);
    let list = List::new(items)
        .block(frame_block("Sources", focused))
        .highlight_style(SELECTED);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn source_item(line: &SourceLine) -> ListItem<'static> {
    if line.heading {
        return ListItem::new(Line::styled(
            line.label.clone(),
            DIM.add_modifier(Modifier::BOLD),
        ));
    }
    let indent = "  ".repeat(usize::from(line.depth));
    let count = if line.unread > 0 {
        format!(" {}", line.unread)
    } else {
        String::new()
    };
    ListItem::new(Line::from(vec![
        Span::raw(format!("{indent}{}", line.label)),
        Span::styled(count, DIM),
    ]))
}

fn render_entries(frame: &mut Frame, view: &ReaderView<'_>, area: Rect, focused: bool) {
    let items: Vec<ListItem> = view
        .entries
        .iter()
        .map(|row| entry_item(row, view.now_local))
        .collect();
    let mut list_state = ListState::default().with_selected(view.entry_cursor);
    let list = List::new(items)
        .block(frame_block(&view.source_title, focused))
        .highlight_style(SELECTED);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn entry_item(row: &EntryRow, now: DateTime<FixedOffset>) -> ListItem<'static> {
    let mark = if row.is_read { " " } else { "●" };
    let star = if row.is_starred { "★" } else { " " };
    let title = printable(row.title.as_deref().unwrap_or("(untitled)")).into_owned();
    let style = if row.is_read { DIM } else { BOLD };
    ListItem::new(vec![
        Line::from(vec![
            Span::raw(format!("{mark}{star} ")),
            Span::styled(title, style),
        ]),
        Line::styled(
            format!(
                "   {} · {}",
                printable(&row.feed_title),
                short_date(row.published_at, now)
            ),
            DIM,
        ),
    ])
}

/// `09:30` today, `Thu 15 Oct` this year, `15 Oct 2025` before; in the state's local offset.
fn short_date(at: DateTime<Utc>, now: DateTime<FixedOffset>) -> String {
    let local = at.with_timezone(now.offset());
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else if local.format("%Y").to_string() == now.format("%Y").to_string() {
        local.format("%a %-d %b").to_string()
    } else {
        local.format("%-d %b %Y").to_string()
    }
}

/// What one article line becomes on screen.
enum Piece<'a> {
    /// The line as `render_html` made it (also `[image: alt text]` for a failed image or with
    /// images off).
    Text(&'a Line<'static>),
    /// An image that is on its way: one placeholder row.
    Loading,
    /// A decoded image, drawn in the rows reserved for it.
    Image { url: &'a str },
}

fn render_reader(
    frame: &mut Frame,
    view: &ReaderView<'_>,
    images: &mut ImageStore,
    area: Rect,
    focused: bool,
) -> ReaderMetrics {
    let block = frame_block("Article", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let (Some(entry), Some(article)) = (view.entry, view.article) else {
        let text = if view.entry_cursor.is_some() {
            "Loading…"
        } else {
            "No entry selected"
        };
        frame.render_widget(Paragraph::new(Span::styled(text, DIM)), inner);
        return ReaderMetrics {
            total_rows: 0,
            viewport_rows: inner.height,
        };
    };
    let width = inner.width.min(MAX_TEXT_WIDTH);
    let header = header_lines(entry, view);
    let header_rows = u16::try_from(header.len())
        .unwrap_or(HEADER_ROWS)
        .min(inner.height);
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(header_rows), Constraint::Min(0)])
            .flex(Flex::Start)
            .areas(inner);
    frame.render_widget(
        Paragraph::new(header),
        Rect {
            width,
            ..header_area
        },
    );
    // `render_html` already ends the article with its numbered links list. Each line is one
    // block; an image's placeholder line grows into the rows of the picture (at most half the
    // body), so scrolling is in screen rows.
    let viewport = usize::from(body_area.height);
    let max_image_rows = (body_area.height / 2).max(1);
    let show_images = view.images && images.enabled();
    let at_line: HashMap<usize, &ImageRef> = article.images.iter().map(|i| (i.line, i)).collect();
    let scroll = view.reader_scroll;
    let mut top = 0usize;
    for (index, line) in article.lines.iter().enumerate() {
        let (piece, rows) = match at_line.get(&index) {
            Some(image) if show_images => match images.state_of(&image.url) {
                ImageState::Ready => {
                    let rows = images.rows_for(&image.url, Variant::Native, width, max_image_rows);
                    (Piece::Image { url: &image.url }, usize::from(rows))
                }
                ImageState::Failed => (Piece::Text(line), 1),
                ImageState::Absent | ImageState::Loading => {
                    // Lazy loading: from one viewport above to two below the visible rows.
                    if top + viewport >= scroll && top <= scroll + 2 * viewport {
                        images.want(&image.url);
                    }
                    (Piece::Loading, 1)
                }
            },
            _ => (Piece::Text(line), 1),
        };
        // A picture cannot be cut: only blocks wholly inside the viewport are drawn.
        if top >= scroll && top + rows <= scroll + viewport {
            let place = Rect {
                x: body_area.x,
                y: body_area.y + u16::try_from(top - scroll).unwrap_or(u16::MAX),
                width,
                height: u16::try_from(rows).unwrap_or(u16::MAX),
            };
            match piece {
                Piece::Text(line) => frame.render_widget(Paragraph::new(line.clone()), place),
                Piece::Loading => {
                    frame.render_widget(Paragraph::new(Span::styled(LOADING_IMAGE, DIM)), place);
                }
                Piece::Image { url, .. } => {
                    if let Some(protocol) = images.protocol(url, Variant::Native) {
                        frame.render_stateful_widget(
                            StatefulImage::<StatefulProtocol>::default(),
                            place,
                            protocol,
                        );
                    }
                }
            }
        }
        top += rows;
    }
    ReaderMetrics {
        total_rows: top,
        viewport_rows: body_area.height,
    }
}

fn header_lines(entry: &EntryDetail, view: &ReaderView<'_>) -> Vec<Line<'static>> {
    let row = &entry.row;
    let title = printable(row.title.as_deref().unwrap_or("(untitled)")).into_owned();
    let mut by = printable(&row.feed_title).into_owned();
    if let Some(author) = row.author.as_deref() {
        by.push_str(&format!(" · {}", printable(author)));
    }
    by.push_str(&format!(
        " · {}",
        short_date(row.published_at, view.now_local)
    ));
    let mut state = vec![if row.is_read { "read" } else { "unread" }];
    if row.is_starred {
        state.push("starred");
    }
    if view.show_fulltext {
        state.push("full text");
    }
    let mut lines = vec![
        Line::styled(title, BOLD),
        Line::styled(by, DIM),
        Line::styled(state.join(" · "), DIM),
    ];
    // A failed `f` stays visible: the extraction's own sentence, one line.
    if let Some(error) = entry.fulltext_error.as_deref() {
        lines.push(Line::styled(
            printable(error).into_owned(),
            Style::new().fg(Color::Red),
        ));
    }
    lines
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [rect] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(row);
    rect
}

const HELP: [&str; 13] = [
    "j/k  next, previous entry",
    "Space  scroll, then next unread",
    "h/l  move between columns",
    "m  read or unread   s  star",
    "A  mark the list read",
    "u  unread only   /  search",
    "f  full text   o  open a link",
    "c  rhythm of the feed",
    "r  refresh   R  refresh all",
    "i  images   t  Antiope",
    "Esc  back   q  quit",
    "",
    "Any key closes this help.",
];

fn render_help(frame: &mut Frame, area: Rect) {
    let rect = centered(area, 40, HELP.len() as u16 + 2);
    frame.render_widget(Clear, rect);
    let lines: Vec<Line> = HELP.iter().map(|text| Line::raw(*text)).collect();
    frame.render_widget(Paragraph::new(lines).block(frame_block("Keys", true)), rect);
}

fn render_rhythm(frame: &mut Frame, area: Rect, choice: usize) {
    let rect = centered(area, 24, RHYTHM_CHOICES.len() as u16 + 2);
    frame.render_widget(Clear, rect);
    let items: Vec<ListItem> = RHYTHM_CHOICES
        .iter()
        .map(|group| ListItem::new(group.map_or_else(|| "Auto".to_string(), group_label)))
        .collect();
    let mut list_state = ListState::default().with_selected(Some(choice));
    let list = List::new(items)
        .block(frame_block("Rhythm", true))
        .highlight_style(SELECTED);
    frame.render_stateful_widget(list, rect, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::layout::Look;
    use crate::tui::reader::settings::TerminalSettings;
    use crate::tui::reader::state::{Event, JobOutcome};
    use chrono::{FixedOffset, TimeZone, Utc};
    use image::{DynamicImage, Rgb, RgbImage};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui_image::picker::Picker;
    use ressac_core::model::CadenceGroup;
    use ressac_core::store::{
        EntryPage, Source, SourceCounts, SourceNode, SourceNodeKind, SourcesSnapshot,
    };
    use std::collections::BTreeMap;

    const PHOTO: &str = "https://example.com/photo.jpg";
    const WITH_IMAGE: &str = "<p>Avant.</p><img src=\"https://example.com/photo.jpg\" \
        alt=\"Le port\"><p>Après.</p>";

    const ARTICLE: &str = "<h2>Le jardin</h2><p>Un <b>long</b> texte avec un \
        <a href=\"https://example.com/a\">premier lien</a> et un \
        <a href=\"https://example.com/b\">second</a>.</p><ul><li>un</li><li>deux</li></ul>";

    fn row(id: i64, title: &str, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: 3,
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
                    id: 1,
                    name: "Infos".into(),
                },
                page: Some(400),
                unread: 3,
                children: vec![SourceNode {
                    kind: SourceNodeKind::Feed {
                        id: 3,
                        title: "Courrier international".into(),
                    },
                    page: Some(401),
                    unread: 3,
                    children: vec![],
                }],
            }],
        }
    }

    fn detail_for(row: EntryRow, error: Option<&str>) -> EntryDetail {
        EntryDetail {
            row,
            summary_html: None,
            content_html: Some(ARTICLE.into()),
            fulltext_html: None,
            fulltext_error: error.map(str::to_string),
            prefer_full_text: false,
        }
    }

    /// The Unread list of `rows`, with the first one open in the reader.
    fn state(width: u16, height: u16, rows: Vec<EntryRow>) -> ReaderState {
        let mut s = ReaderState::new(
            TerminalSettings {
                look: Look::Native,
                images: true,
            },
            width,
            height,
        );
        s.on_event(Event::Sources(snapshot()));
        s.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 7, 0, 0).unwrap(),
            local: FixedOffset::east_opt(7200)
                .unwrap()
                .with_ymd_and_hms(2026, 10, 16, 9, 0, 0)
                .unwrap(),
        });
        s.go_to(Source::Unread);
        let unread = rows.iter().filter(|r| !r.is_read).count();
        s.on_event(Event::Entries {
            source: Source::Unread,
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
        s.on_event(Event::Entry(Box::new(detail_for(open, None))));
        s
    }

    fn terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(width, height)).unwrap()
    }

    fn draw(s: &ReaderState, width: u16, height: u16) -> String {
        draw_with(s, &mut ImageStore::new(Picker::halfblocks()), width, height)
    }

    fn draw_with(s: &ReaderState, images: &mut ImageStore, width: u16, height: u16) -> String {
        let mut terminal = terminal(width, height);
        terminal
            .draw(|f| {
                render(f, s, images);
            })
            .unwrap();
        terminal.backend().to_string()
    }

    fn rows() -> Vec<EntryRow> {
        vec![
            row(11, "Le jardin des flux", false),
            row(12, "Un deuxième article", false),
            row(13, "Lu hier", true),
        ]
    }

    #[test]
    fn native_three_columns() {
        insta::assert_snapshot!(draw(&state(140, 30, rows()), 140, 30));
    }

    #[test]
    fn native_two_columns() {
        insta::assert_snapshot!(draw(&state(100, 30, rows()), 100, 30));
    }

    #[test]
    fn native_one_column() {
        let mut s = state(70, 30, rows());
        s.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        insta::assert_snapshot!(draw(&s, 70, 30));
    }

    #[test]
    fn native_window_too_small() {
        let s = state(30, 8, rows());
        let out = draw(&s, 30, 8);
        assert!(out.contains(TOO_SMALL), "{out}");
    }

    #[test]
    fn the_sources_slide_over_the_entries_in_two_columns() {
        let mut s = state(100, 30, rows());
        s.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        s.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        let out = draw(&s, 100, 30);
        assert!(out.contains("Sources"), "{out}");
        assert!(out.contains("Courrier international"), "{out}");
    }

    /// Review Focus 1: an OSC title-setting sequence, BEL, CSI (U+009B), an emoji and CJK.
    #[test]
    fn escape_sequences_and_wide_titles_stay_inside_their_column() {
        let hostile = "Café ☕ 東京 \u{1b}]0;pwned\u{7} \u{9b}31m fin";
        for (width, height) in [(100u16, 20u16), (70, 20)] {
            let s = state(
                width,
                height,
                vec![row(11, hostile, false), row(12, "Autre", false)],
            );
            let mut terminal = terminal(width, height);
            terminal
                .draw(|f| {
                    render(f, &s, &mut ImageStore::new(Picker::halfblocks()));
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let out = terminal.backend().to_string();
            assert!(
                !out.chars()
                    .any(|c| c == '\u{1b}' || c == '\u{7}' || c == '\u{9b}'),
                "{out:?}"
            );
            assert!(
                out.contains('\u{FFFD}'),
                "controls are shown as U+FFFD: {out}"
            );
            assert!(out.contains("東京"), "{out}");
            // The entries column is the first 40 cells (the whole width when narrow): its right
            // border is intact on every body row, so nothing spilled past the edge.
            let edge = if width < 80 { width - 1 } else { 39 };
            for y in 1..height - 1 {
                let symbol = buffer[(edge, y)].symbol();
                assert!(
                    matches!(symbol, "│" | "┐" | "┘"),
                    "row {y} spills past the column: {symbol:?}\n{out}"
                );
            }
        }
    }

    #[test]
    fn progress_text_shortens_the_edition_label_when_needed() {
        assert_eq!(
            progress_text(EDITION_PROGRESS_LABEL, 34, 156, 60),
            "Building this morning's edition… 34/156"
        );
        assert_eq!(
            progress_text(EDITION_PROGRESS_LABEL, 34, 156, 30),
            "Edition… 34/156"
        );
        assert_eq!(
            progress_text(FETCH_PROGRESS_LABEL, 3, 9, 40),
            "Fetching feeds 3/9"
        );
        assert_eq!(progress_text(FETCH_PROGRESS_LABEL, 3, 9, 10), "Fetching…");
    }

    #[test]
    fn the_progress_line_replaces_the_message_on_the_top_line() {
        let mut s = state(140, 30, rows());
        s.on_event(Event::Progress {
            label: EDITION_PROGRESS_LABEL.into(),
            done: 34,
            total: 156,
        });
        let out = draw(&s, 140, 30);
        assert!(
            out.lines()
                .next()
                .unwrap()
                .contains("Building this morning's edition… 34/156"),
            "{out}"
        );
        s.on_event(Event::JobDone(JobOutcome::Refreshed));
        assert!(!draw(&s, 140, 30).contains("34/156"));
    }

    fn last_line(out: &str) -> &str {
        out.lines().last().unwrap().trim_matches('"').trim_end()
    }

    #[test]
    fn the_bottom_line_keeps_help_when_the_hints_do_not_fit() {
        let s = state(140, 30, rows());
        assert!(last_line(&draw(&s, 140, 30)).ends_with("/ search  ? help"));
        let narrow = draw(&s, 40, 30);
        let line = last_line(&narrow);
        assert!(
            line.ends_with("? help") && line.starts_with("j/k entry"),
            "{line}"
        );
        assert!(line.chars().count() <= 40, "{line}");
    }

    #[test]
    fn the_bottom_line_says_how_to_end_a_search_or_a_link_choice() {
        let mut s = state(140, 30, rows());
        s.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(last_line(&draw(&s, 140, 30)), "Enter apply  Esc cancel");
        s.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        s.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(last_line(&draw(&s, 140, 30)), "Enter apply  Esc cancel");
    }

    #[test]
    fn the_article_is_wrapped_to_72_columns_at_most() {
        let s = state(140, 30, rows());
        let mut terminal = terminal(140, 30);
        let mut metrics = None;
        terminal
            .draw(|f| {
                metrics = Some(render(f, &s, &mut ImageStore::new(Picker::halfblocks())));
            })
            .unwrap();
        let metrics = metrics.unwrap();
        assert!(metrics.total_rows > 0);
        assert!(metrics.viewport_rows > 0 && metrics.viewport_rows < 30);
        let out = terminal.backend().to_string();
        assert!(out.contains("premier lien[1]"), "{out}");
        assert!(out.contains("[2] https://example.com/b"), "{out}");
    }

    #[test]
    fn dates_follow_the_states_local_offset() {
        // 06:30 UTC on the 16th is 08:30 in the state's +02:00 zone.
        let out = draw(&state(140, 30, rows()), 140, 30);
        assert!(out.contains("08:30"), "{out}");
        assert!(!out.contains("06:30"), "{out}");
    }

    #[test]
    fn a_failed_extraction_is_visible_in_the_reader() {
        let mut s = state(140, 30, rows());
        let open = s.entries[0].clone();
        s.on_event(Event::Entry(Box::new(detail_for(
            open,
            Some("Could not extract the article: HTTP 403"),
        ))));
        let out = draw(&s, 140, 30);
        assert!(
            out.contains("Could not extract the article: HTTP 403"),
            "{out}"
        );
    }

    #[test]
    fn the_help_and_the_rhythm_menu_are_drawn() {
        let mut s = state(140, 30, rows());
        s.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(draw(&s, 140, 30).contains("Any key closes this help."));
        s.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        let out = draw(&s, 140, 30);
        assert!(out.contains("Auto") && out.contains("Silent"), "{out}");
    }

    /// The first entry open on an article with one photo; `images_on` is the `i` setting.
    fn image_state(width: u16, height: u16, images_on: bool) -> ReaderState {
        let mut s = state(width, height, rows());
        s.settings.images = images_on;
        let open = s.entries[0].clone();
        s.on_event(Event::Entry(Box::new(EntryDetail {
            content_html: Some(WITH_IMAGE.into()),
            ..detail_for(open, None)
        })));
        s
    }

    fn photo() -> DynamicImage {
        // Rows of different colours: a uniform picture would draw as blank cells with a background.
        DynamicImage::ImageRgb8(RgbImage::from_fn(400, 300, |_, y| {
            Rgb([(y % 256) as u8, 120, 255 - (y % 256) as u8])
        }))
    }

    #[test]
    fn an_image_not_loaded_yet_is_wanted_and_shows_loading() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        let out = draw_with(&s, &mut images, 140, 30);
        assert!(out.contains(LOADING_IMAGE), "{out}");
        assert_eq!(images.take_wanted(), vec![PHOTO.to_string()]);
    }

    #[test]
    fn a_ready_image_is_drawn_in_half_blocks_without_a_protocol() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.ingest(PHOTO, photo());
        let out = draw_with(&s, &mut images, 140, 30);
        assert!(
            out.contains('▀') || out.contains('▄'),
            "half-blocks drawn: {out}"
        );
        assert!(!out.contains(LOADING_IMAGE));
        assert!(images.rows_for(PHOTO, Variant::Native, 72, 13) <= 13);
    }

    #[test]
    fn a_failed_image_or_images_off_show_the_alt_text() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.fail(PHOTO);
        assert!(draw_with(&s, &mut images, 140, 30).contains("[image: Le port]"));

        let off = image_state(140, 30, false);
        let mut fresh = ImageStore::new(Picker::halfblocks());
        assert!(draw_with(&off, &mut fresh, 140, 30).contains("[image: Le port]"));
        assert!(
            fresh.take_wanted().is_empty(),
            "images off: nothing is fetched"
        );
        assert_eq!(fresh.state_of(PHOTO), ImageState::Absent);

        let mut disabled = ImageStore::disabled();
        let out = draw_with(&s, &mut disabled, 140, 30);
        assert!(
            out.contains("[image: Le port]") && !out.contains(LOADING_IMAGE),
            "{out}"
        );
    }

    #[test]
    fn an_image_far_below_the_viewport_is_not_wanted_yet() {
        let filler = "<p>Ligne de remplissage.</p>".repeat(120);
        let html = format!("{filler}<img src=\"{PHOTO}\" alt=\"Loin\">");
        let mut s = state(140, 30, rows());
        let open = s.entries[0].clone();
        s.on_event(Event::Entry(Box::new(EntryDetail {
            content_html: Some(html),
            ..detail_for(open, None)
        })));
        let mut images = ImageStore::new(Picker::halfblocks());
        draw_with(&s, &mut images, 140, 30);
        assert!(images.take_wanted().is_empty(), "lazy loading");
    }

    #[test]
    fn native_reader_with_an_image() {
        let s = image_state(140, 30, true);
        let mut images = ImageStore::new(Picker::halfblocks());
        images.ingest(PHOTO, photo());
        insta::assert_snapshot!(draw_with(&s, &mut images, 140, 30));
    }
}
