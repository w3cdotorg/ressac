//! Garden rendering. Layout: header, list | detail, footer (spec §9).

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ressac_core::garden::{GardenRow, safe_repairs};
use ressac_core::model::Status;
use ressac_core::store::AttemptRecord;

use crate::output::{glyph, printable};
use crate::tui::state::{GardenState, Mode};

fn keys_hint(repairs: usize, back_hint: Option<&str>) -> String {
    let mut hint = format!(
        "k keep  r repair  b bury  ↵ details  o site  w wayback  u undo  f filter  R safe repairs ({repairs})  ? help  q quit"
    );
    if let Some(back) = back_hint {
        hint.push_str("  ");
        hint.push_str(back);
    }
    hint
}

pub fn render(frame: &mut Frame, state: &GardenState) {
    render_in(frame, frame.area(), state, None);
}

/// The garden inside `area` (the whole frame for `ressac garden`, the reader's columns for
/// the garden destinations). Popups are centred on `area`. `back_hint` ends the footer's keys
/// (the reader says how to go back to its sources; `ressac garden` has nowhere to go back to).
pub fn render_in(frame: &mut Frame, area: Rect, state: &GardenState, back_hint: Option<&str>) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(header_line(state)), header);
    if state.mode == Mode::Detail {
        render_detail(frame, state, body);
    } else {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
                .areas(body);
        render_list(frame, state, list);
        render_detail(frame, state, detail);
    }
    let footer_text = match &state.message {
        Some(msg) => Line::from(printable(msg).into_owned()).bold(),
        None => Line::from(keys_hint(state.safe_repair_count(), back_hint)).dim(),
    };
    frame.render_widget(
        Paragraph::new(footer_text).wrap(Wrap { trim: true }),
        footer,
    );
    match state.mode {
        Mode::ConfirmRepairs => popup(frame, area, " Apply safe repairs ", confirm_lines(state)),
        Mode::Help => popup(frame, area, " Help ", help_lines()),
        Mode::Queue | Mode::Detail => {}
    }
}

fn header_line(state: &GardenState) -> Line<'static> {
    let active: Vec<&GardenRow> = state.rows.iter().filter(|r| !r.buried).collect();
    let mut spans = vec![Span::raw(format!("Ressac · Garden   {} feeds ", active.len())).bold()];
    for status in Status::ALL {
        let n = active.iter().filter(|r| r.status() == status).count();
        if n > 0 {
            spans.push(Span::raw(format!(" {} {n}", glyph(status, state.ascii))));
        }
    }
    let p = state.progress();
    spans.push(Span::raw(format!(
        "   triage: {} / {}",
        p.decided, p.needing
    )));
    Line::from(spans)
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        format!("{text:<width$}")
    } else {
        let cut: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn render_list(frame: &mut Frame, state: &GardenState, area: Rect) {
    // glyph (2) + spaces + folder (8) + clue (7) + borders (2)
    let title_width = (area.width as usize).saturating_sub(22).max(8);
    let items: Vec<ListItem> = state
        .visible
        .iter()
        .map(|&i| {
            let r = &state.rows[i];
            let clue = if r.buried {
                "buried".to_string()
            } else {
                r.diagnosis
                    .as_ref()
                    .map(|d| printable(&d.evidence.clue).into_owned())
                    .unwrap_or_default()
            };
            let item = ListItem::new(format!(
                "{} {} {} {}",
                glyph(r.status(), state.ascii),
                truncate(&printable(&r.title), title_width),
                truncate(&printable(&r.top_folder), 6),
                clue
            ));
            if r.buried { item.dim() } else { item }
        })
        .collect();
    let title = format!(
        " {} ({}) ",
        capitalize(&state.filter.label()),
        state.visible.len()
    );
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_symbol("▶")
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut list_state =
        ListState::default().with_selected((!state.visible.is_empty()).then_some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn render_detail(frame: &mut Frame, state: &GardenState, area: Rect) {
    let Some(row) = state.selected_row() else {
        let empty =
            Paragraph::new("Nothing to triage here. Press f to browse other feeds, q to quit.")
                .block(Block::bordered())
                .wrap(Wrap { trim: true });
        frame.render_widget(empty, area);
        return;
    };
    frame.render_widget(
        Paragraph::new(detail_lines(row, state.ascii, state.mode == Mode::Detail))
            .block(Block::bordered().title(format!(" {} ", printable(&row.title))))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The side pane; `expanded` (↵) adds the redirect chain and the fetch log (spec §9).
fn detail_lines(row: &GardenRow, ascii: bool, expanded: bool) -> Vec<Line<'static>> {
    let status = row.status();
    let mut lines = Vec::new();
    let mut head = format!("{} {}", glyph(status, ascii), status.as_str());
    if let Some(d) = &row.diagnosis
        && status == Status::Hijacked
    {
        head.push_str(&format!(" · score {:.2}", d.hijack_score));
    }
    lines.push(Line::from(vec![
        Span::raw(head).bold(),
        Span::raw(format!("   {}", printable(&row.folder_path))),
    ]));
    if row.buried {
        let on = row
            .decision
            .as_ref()
            .map(|d| format!(" on {}", d.decided_at.format("%Y-%m-%d")))
            .unwrap_or_default();
        lines.push(Line::from(format!("Buried{on}. Press k to restore it.")).bold());
    }
    if let Some(d) = &row.diagnosis {
        lines.push(Line::from(printable(&d.reason).into_owned()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!("Feed  {}", printable(&row.xml_url))));
    if let Some(site) = &row.html_url {
        lines.push(Line::from(format!("Site  {}", printable(site))));
    }
    if let Some(d) = row
        .diagnosis
        .as_ref()
        .filter(|d| !d.evidence.bullets.is_empty())
    {
        lines.push(Line::from(""));
        lines.push(Line::from("Evidence").bold());
        lines.extend(
            d.evidence
                .bullets
                .iter()
                .map(|b| Line::from(format!(" • {}", printable(b)))),
        );
    }
    if !row.recent.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Last entries").bold());
        lines.extend(row.recent.iter().map(|(at, title)| {
            Line::from(format!(" {}  {}", at.format("%Y-%m-%d"), printable(title)))
        }));
    }
    lines.push(Line::from(""));
    if let Some(w) = &row.wayback_url {
        lines.push(Line::from(format!("Wayback   {}  [w]", printable(w))));
    }
    if let Some(action) = row.action().filter(|_| !row.buried) {
        lines.push(Line::from(format!("Proposed  {}", action.label())));
    }
    if let Some(dec) = &row.decision {
        lines.push(Line::from(format!(
            "Decision  {} on {}",
            dec.decision.as_str(),
            dec.decided_at.format("%Y-%m-%d")
        )));
    }
    if expanded {
        lines.extend(fetch_history_lines(row));
    }
    lines
}

/// "Redirects" (the latest attempt's chain) and "Fetch log" (newest first).
fn fetch_history_lines(row: &GardenRow) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(latest) = row.attempts.first()
        && !latest.redirect_chain.is_empty()
    {
        lines.push(Line::from(""));
        lines.push(Line::from("Redirects").bold());
        lines.extend(
            latest
                .redirect_chain
                .iter()
                .map(|hop| Line::from(format!(" {} → {}", hop.status, printable(&hop.url)))),
        );
    }
    if !row.attempts.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Fetch log").bold());
        lines.extend(row.attempts.iter().map(|a| {
            Line::from(format!(
                " {}  {}",
                a.at.format("%Y-%m-%d %H:%M"),
                printable(&attempt_outcome(a))
            ))
        }));
    }
    lines
}

/// `HTTP 200 · 14 entries`, `HTTP 304 · not modified`, or the error label.
fn attempt_outcome(a: &AttemptRecord) -> String {
    let mut out = match (&a.error, a.http_status) {
        (Some(e), _) => e.label(),
        (None, Some(code)) => format!("HTTP {code}"),
        (None, None) => "no response".to_string(),
    };
    if a.error.is_none() {
        if a.not_modified {
            out.push_str(" · not modified");
        } else if let Some(n) = a.entry_count {
            out.push_str(&format!(" · {n} entries"));
        }
    }
    if a.network_suspect {
        out.push_str(" (network?)");
    }
    out
}

fn confirm_lines(state: &GardenState) -> Vec<Line<'static>> {
    let repairs = safe_repairs(&state.rows);
    let mut lines = vec![
        Line::from(format!("Apply {} safe repairs?", repairs.len())).bold(),
        Line::from(""),
    ];
    for (feed_id, action) in repairs.iter().take(8) {
        let title = state
            .rows
            .iter()
            .find(|r| r.feed_id == *feed_id)
            .map_or("", |r| r.title.as_str());
        lines.push(Line::from(format!(
            " {}: {}",
            printable(title),
            action.label()
        )));
    }
    if repairs.len() > 8 {
        lines.push(Line::from(format!(" … and {} more", repairs.len() - 8)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("y apply   any other key cancel").dim());
    lines
}

fn help_lines() -> Vec<Line<'static>> {
    [
        "↑ ↓ PgUp PgDn  move",
        "k  keep this feed",
        "r  apply the proposed repair",
        "b  bury it (moves to the cemetery, nothing is deleted)",
        "u  undo the last decision",
        "↵  toggle details      f  cycle filters",
        "o  open the site       w  open the Wayback snapshot",
        "R  apply all safe repairs (https, permanent redirects)",
        "q  quit (decisions are already saved)",
    ]
    .into_iter()
    .map(Line::from)
    .collect()
}

fn popup(frame: &mut Frame, within: Rect, title: &str, lines: Vec<Line<'static>>) {
    let height = (lines.len() as u16).saturating_add(2);
    let [vertical] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(within);
    let [area] = Layout::horizontal([Constraint::Percentage(70)])
        .flex(Flex::Center)
        .areas(vertical);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(title.to_string()))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{GardenState, sample_rows, sample_rows_with_buried};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn draw(state: &GardenState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, state)).unwrap();
        terminal.backend().to_string()
    }

    fn press(state: &mut GardenState, code: KeyCode) {
        state.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn garden_queue() {
        insta::assert_snapshot!(draw(&GardenState::new(sample_rows(), false), 110, 24));
    }

    #[test]
    fn garden_ascii() {
        insta::assert_snapshot!(draw(&GardenState::new(sample_rows(), true), 110, 24));
    }

    #[test]
    fn garden_detail() {
        let mut s = GardenState::new(sample_rows(), false);
        press(&mut s, KeyCode::Enter);
        insta::assert_snapshot!(draw(&s, 110, 34));
    }

    #[test]
    fn garden_confirm_repairs() {
        let mut s = GardenState::new(sample_rows(), false);
        press(&mut s, KeyCode::Char('R'));
        insta::assert_snapshot!(draw(&s, 110, 24));
    }

    #[test]
    fn garden_help() {
        let mut s = GardenState::new(sample_rows(), false);
        press(&mut s, KeyCode::Char('?'));
        insta::assert_snapshot!(draw(&s, 110, 24));
    }

    #[test]
    fn garden_narrow() {
        let mut rows = sample_rows();
        rows[0].title =
            "A very long feed title that will never fit in a narrow terminal column".into();
        insta::assert_snapshot!(draw(&GardenState::new(rows, false), 60, 20));
    }

    #[test]
    fn garden_buried_under_status_filter() {
        let mut s = GardenState::new(sample_rows_with_buried(), false);
        for _ in 0..3 {
            press(&mut s, KeyCode::Char('f'));
        }
        insta::assert_snapshot!(draw(&s, 110, 24));
        let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
        terminal.draw(|f| render(f, &s)).unwrap();
        let title_cell = &terminal.backend().buffer()[(5, 2)];
        assert_eq!(title_cell.symbol(), "R");
        assert!(
            title_cell.modifier.contains(Modifier::DIM),
            "buried rows are dimmed"
        );
    }

    #[test]
    fn garden_empty_queue() {
        let mut rows = sample_rows();
        rows.retain(|r| !r.needs_triage());
        insta::assert_snapshot!(draw(&GardenState::new(rows, false), 80, 12));
    }

    #[test]
    fn render_in_draws_the_garden_inside_an_area_only() {
        let state = GardenState::new(sample_rows(), false);
        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal
            .draw(|frame| render_in(frame, Rect::new(30, 2, 110, 20), &state, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        for y in 0..24 {
            for x in 0..30 {
                assert_eq!(buffer[(x, y)].symbol(), " ", "({x},{y})");
            }
        }
        for x in 0..140 {
            assert_eq!(buffer[(x, 0)].symbol(), " ");
        }
        let text: String = (2..22)
            .map(|y| {
                (30..140)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert!(text.contains("Bibliobsession 2.0"));
    }

    #[test]
    fn the_back_hint_ends_the_footer_only_when_given() {
        let state = GardenState::new(sample_rows(), false);
        let draw_in = |hint: Option<&str>| {
            let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
            terminal
                .draw(|f| render_in(f, f.area(), &state, hint))
                .unwrap();
            terminal.backend().to_string()
        };
        assert!(draw_in(Some("Esc sources")).contains("q quit  Esc sources"));
        assert!(!draw_in(None).contains("Esc sources"));
        assert!(!draw(&state, 140, 24).contains("Esc sources"));
    }

    #[test]
    fn feed_text_with_control_characters_never_reaches_the_buffer() {
        let mut rows = sample_rows();
        rows[0].title = "Bad\u{1b}]0;x\u{7} \u{9b}31m feed".into();
        rows[0].xml_url = "https://x.example/\u{1b}[2J".into();
        let mut s = GardenState::new(rows, false);
        s.message = Some("note\u{1b}[31m".into());
        let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
        terminal.draw(|f| render(f, &s)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut all = String::new();
        for cell in buffer.content() {
            assert!(!cell.symbol().chars().any(char::is_control), "{cell:?}");
            all.push_str(cell.symbol());
        }
        assert!(all.contains('\u{FFFD}'));
        let text = draw(&s, 110, 24);
        assert!(!text.contains('\u{1b}') && !text.contains('\u{9b}'));
    }

    #[test]
    fn popups_stay_inside_the_area() {
        let mut state = GardenState::new(sample_rows(), false);
        press(&mut state, KeyCode::Char('?'));
        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal
            .draw(|frame| render_in(frame, Rect::new(30, 2, 110, 20), &state, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        for y in 0..24 {
            for x in 0..30 {
                assert_eq!(buffer[(x, y)].symbol(), " ");
            }
        }
        let text: String = (2..22)
            .map(|y| {
                (30..140)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert!(text.contains("Help"));
    }
}
