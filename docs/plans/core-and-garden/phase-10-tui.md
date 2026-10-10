# Phase 10: `ressac garden` (ratatui)

Read the plan index first. Global Constraints apply. Spec §9 (layout, keys, glyphs). ratatui 0.30 notes: `notes.md`.

**Files (5):**
- Create: `crates/ressac/src/tui/state.rs` (tests inline)
- Create: `crates/ressac/src/tui/ui.rs` (insta snapshot tests inline)
- Create: `crates/ressac/src/tui/mod.rs`
- Modify: `crates/ressac/src/main.rs` (add `mod tui;` and the `Garden` subcommand)
- Modify: `README.md` (usage section)

### Task 10.1: Garden state and key handling

**Interfaces:**
- Consumes: `ressac_core::garden::{GardenRow, Progress, progress, safe_repairs}`, `ressac_core::model::{Decision, FeedId, Status}`.
- Produces (`tui::state`):
  - `enum Mode { Queue, Detail, ConfirmRepairs, Help }`
  - `enum Filter { Queue, Status(Status), Folder(String) }`, with `label()`
  - `enum Command { None, Quit, Decide(FeedId, Decision), Undo, ApplySafeRepairs, Open(String) }`
  - `struct GardenState { rows, visible: Vec<usize>, selected: usize, mode, filter, message: Option<String>, ascii: bool }`
  - `GardenState` methods: `new(rows, ascii)`, `selected_row()`, `progress()`, `safe_repair_count()`, `reload(rows)`, `on_key(KeyEvent) -> Command`
  - `#[cfg(test)] pub(crate) fn sample_rows() -> Vec<GardenRow>`, shared with `ui.rs` tests

Key map (spec §9):

| Key | Effect |
|---|---|
| `↑` / `↓` | Move one row |
| `PgUp` / `PgDn` | Move 10 rows |
| `Enter` | Toggle Detail |
| `Esc` | Leave Detail, or quit from Queue |
| `q`, `Ctrl-C` | Quit |
| `k` | Decide Keep |
| `b` | Decide Bury |
| `r` | Decide Repair, only if the row's action `is_repair()`; otherwise message `Nothing to repair for this feed.` |
| `u` | Undo |
| `o` | Open the site, or message `This feed has no site address.` |
| `w` | Open the Wayback snapshot, or message ``No Wayback snapshot yet (run `ressac check`).`` |
| `f` | Cycle filters: Queue → each status present (ALL order) → each top folder (sorted) → Queue; message `Filter: <label>` |
| `R` | ConfirmRepairs if any safe repair, else message `No safe repairs pending.` |
| `?` | Help |

In ConfirmRepairs, `y` returns `ApplySafeRepairs` and any other key cancels. In Help, any key closes it.

- [ ] **Step 1: Write the failing tests** in `tui/state.rs`

```rust
//! Garden screen state and key handling. Pure: no terminal, no database.

#[cfg(test)]
pub(crate) fn sample_rows() -> Vec<GardenRow> {
    use chrono::{DateTime, Utc};
    use ressac_core::model::{Action, Cadence, Diagnosis, Evidence};

    let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
    let row = |id: FeedId, title: &str, folder: &str, status: Status, clue: &str, action: Option<Action>| GardenRow {
        feed_id: id,
        title: title.into(),
        folder_path: folder.into(),
        top_folder: folder.split(" / ").next().unwrap().into(),
        xml_url: format!("https://feed{id}.example.org/rss"),
        html_url: Some(format!("https://feed{id}.example.org/")),
        wayback_url: None,
        buried: false,
        diagnosis: Some(Diagnosis {
            status,
            reason: format!("{} reason.", status.as_str()),
            evidence: Evidence { bullets: vec![format!("{} reason.", status.as_str())], clue: clue.into(), language: None },
            remarks: vec![],
            action,
            hijack_score: 0.0,
            cadence: Cadence::default(),
        }),
        decision: None,
        recent: vec![],
    };
    let mut biblio = row(1, "Bibliobsession 2.0", "Biblis", Status::Hijacked, "vie ⚠", Some(Action::Bury));
    if let Some(d) = biblio.diagnosis.as_mut() {
        d.reason = "The site now seems to serve unrelated or spam content.".into();
        d.hijack_score = 0.86;
        d.evidence.bullets = vec![
            "Description is in vie (0.97) while the feed is in fra.".into(),
            "Spam vocabulary: \"xóc đĩa\", \"trúng lớn\".".into(),
            "The feed address now returns an HTML page.".into(),
        ];
        d.evidence.language = Some("vie".into());
    }
    biblio.wayback_url = Some("http://web.archive.org/web/20210612000000/https://bibliobsession.net/".into());
    biblio.recent = vec![
        (at("2019-04-02T10:00:00Z"), "Bibliothèques et communs numériques".into()),
        (at("2019-02-11T10:00:00Z"), "Le grand débat en bibliothèque ?".into()),
    ];
    vec![
        biblio,
        row(2, "Slashdot", "Infos", Status::Moved, "301", Some(Action::FollowRedirect { url: "https://slashdot.org/rss".into() })),
        row(3, "Re/code", "Infos", Status::Dead, "410", Some(Action::Bury)),
        row(4, "Heading East", "Blogs", Status::Fossil, "2011", None),
        row(5, "PhD et maracuja", "Blogs / potes", Status::Dormant, "14 mo", None),
        row(6, "xkcd.com", "Blogs", Status::Alive, "3 d", Some(Action::UpgradeHttps { url: "https://xkcd.com/rss.xml".into() })),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state() -> GardenState {
        GardenState::new(sample_rows(), false)
    }

    #[test]
    fn queue_shows_only_feeds_needing_triage() {
        let s = state();
        let ids: Vec<FeedId> = s.visible.iter().map(|&i| s.rows[i].feed_id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert_eq!(s.selected_row().unwrap().feed_id, 1);
    }

    #[test]
    fn decisions_target_the_selected_feed() {
        let mut s = state();
        assert_eq!(s.on_key(key(KeyCode::Char('k'))), Command::Decide(1, Decision::Keep));
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('r'))), Command::Decide(2, Decision::Repair));
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('b'))), Command::Decide(3, Decision::Bury));
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('r'))), Command::None);
        assert_eq!(s.message.as_deref(), Some("Nothing to repair for this feed."));
    }

    #[test]
    fn navigation_is_clamped() {
        let mut s = state();
        s.on_key(key(KeyCode::Up));
        assert_eq!(s.selected, 0);
        s.on_key(key(KeyCode::PageDown));
        assert_eq!(s.selected, 3);
    }

    #[test]
    fn open_keys_use_site_and_wayback() {
        let mut s = state();
        assert_eq!(
            s.on_key(key(KeyCode::Char('w'))),
            Command::Open("http://web.archive.org/web/20210612000000/https://bibliobsession.net/".into())
        );
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('w'))), Command::None);
        assert!(s.message.as_deref().unwrap().starts_with("No Wayback snapshot"));
        assert_eq!(s.on_key(key(KeyCode::Char('o'))), Command::Open("https://feed2.example.org/".into()));
    }

    #[test]
    fn filters_cycle_through_statuses_and_folders() {
        let mut s = state();
        s.on_key(key(KeyCode::Char('f')));
        assert_eq!(s.filter, Filter::Status(Status::Hijacked));
        for _ in 0..5 {
            s.on_key(key(KeyCode::Char('f')));
        }
        assert_eq!(s.filter, Filter::Status(Status::Alive));
        assert_eq!(s.visible.len(), 1);
        s.on_key(key(KeyCode::Char('f')));
        assert_eq!(s.filter, Filter::Folder("Biblis".into()));
        assert_eq!(s.message.as_deref(), Some("Filter: folder Biblis"));
    }

    #[test]
    fn bulk_repairs_need_confirmation() {
        let mut s = state();
        assert_eq!(s.safe_repair_count(), 2);
        assert_eq!(s.on_key(key(KeyCode::Char('R'))), Command::None);
        assert_eq!(s.mode, Mode::ConfirmRepairs);
        assert_eq!(s.on_key(key(KeyCode::Char('n'))), Command::None);
        assert_eq!(s.mode, Mode::Queue);
        s.on_key(key(KeyCode::Char('R')));
        assert_eq!(s.on_key(key(KeyCode::Char('y'))), Command::ApplySafeRepairs);
    }

    #[test]
    fn detail_help_and_quit() {
        let mut s = state();
        s.on_key(key(KeyCode::Enter));
        assert_eq!(s.mode, Mode::Detail);
        assert_eq!(s.on_key(key(KeyCode::Esc)), Command::None);
        assert_eq!(s.mode, Mode::Queue);
        s.on_key(key(KeyCode::Char('?')));
        assert_eq!(s.mode, Mode::Help);
        s.on_key(key(KeyCode::Char('x')));
        assert_eq!(s.mode, Mode::Queue);
        assert_eq!(s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)), Command::Quit);
        assert_eq!(s.on_key(key(KeyCode::Esc)), Command::Quit);
    }

    #[test]
    fn reload_keeps_the_position_so_the_next_feed_is_selected() {
        let mut s = state();
        s.on_key(key(KeyCode::Down));
        let mut rows = sample_rows();
        rows.retain(|r| r.feed_id != 2);
        s.reload(rows);
        assert_eq!(s.selected_row().unwrap().feed_id, 3);
    }
}
```

- [ ] **Step 2:** Create `tui/mod.rs` containing `pub mod state; pub mod ui;` and an empty `ui.rs` (doc comment only). Add `mod tui;` to `main.rs`. Run `cargo test -p ressac tui::state`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert at the top of `tui/state.rs` (below the doc comment):

```rust
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ressac_core::garden::{GardenRow, Progress, progress, safe_repairs};
use ressac_core::model::{Decision, FeedId, Status};

const PAGE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Queue,
    Detail,
    ConfirmRepairs,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Feeds needing triage (the default view).
    Queue,
    Status(Status),
    Folder(String),
}

impl Filter {
    pub fn label(&self) -> String {
        match self {
            Filter::Queue => "to triage".to_string(),
            Filter::Status(s) => s.as_str().to_string(),
            Filter::Folder(f) => format!("folder {f}"),
        }
    }

    fn matches(&self, row: &GardenRow) -> bool {
        match self {
            Filter::Queue => row.needs_triage(),
            Filter::Status(s) => row.status() == *s,
            Filter::Folder(f) => &row.top_folder == f,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    None,
    Quit,
    Decide(FeedId, Decision),
    Undo,
    ApplySafeRepairs,
    Open(String),
}

pub struct GardenState {
    pub rows: Vec<GardenRow>,
    /// Indexes into `rows` shown by the current filter.
    pub visible: Vec<usize>,
    /// Index into `visible`.
    pub selected: usize,
    pub mode: Mode,
    pub filter: Filter,
    pub message: Option<String>,
    pub ascii: bool,
}

impl GardenState {
    pub fn new(rows: Vec<GardenRow>, ascii: bool) -> Self {
        let mut state = GardenState {
            rows,
            visible: Vec::new(),
            selected: 0,
            mode: Mode::Queue,
            filter: Filter::Queue,
            message: None,
            ascii,
        };
        state.refilter();
        state
    }

    pub fn selected_row(&self) -> Option<&GardenRow> {
        self.visible.get(self.selected).map(|&i| &self.rows[i])
    }

    pub fn progress(&self) -> Progress {
        progress(&self.rows)
    }

    pub fn safe_repair_count(&self) -> usize {
        safe_repairs(&self.rows).len()
    }

    /// Replaces the rows after a database change, keeping the cursor position so the
    /// feed after a just-decided one becomes selected.
    pub fn reload(&mut self, rows: Vec<GardenRow>) {
        self.rows = rows;
        self.refilter();
    }

    fn refilter(&mut self) {
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| self.filter.matches(r))
            .map(|(i, _)| i)
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
    }

    fn filters(&self) -> Vec<Filter> {
        let mut out = vec![Filter::Queue];
        out.extend(
            Status::ALL
                .into_iter()
                .filter(|s| self.rows.iter().any(|r| r.status() == *s))
                .map(Filter::Status),
        );
        let mut folders: Vec<String> = self.rows.iter().map(|r| r.top_folder.clone()).collect();
        folders.sort();
        folders.dedup();
        out.extend(folders.into_iter().map(Filter::Folder));
        out
    }

    fn next_filter(&mut self) {
        let all = self.filters();
        let next = all.iter().position(|f| *f == self.filter).map_or(0, |i| (i + 1) % all.len());
        self.filter = all[next].clone();
        self.selected = 0;
        self.refilter();
        self.message = Some(format!("Filter: {}", self.filter.label()));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Command {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Command::Quit;
        }
        self.message = None;
        match self.mode {
            Mode::Help => {
                self.mode = Mode::Queue;
                Command::None
            }
            Mode::ConfirmRepairs => {
                self.mode = Mode::Queue;
                if key.code == KeyCode::Char('y') { Command::ApplySafeRepairs } else { Command::None }
            }
            Mode::Queue | Mode::Detail => self.on_list_key(key.code),
        }
    }

    fn on_list_key(&mut self, code: KeyCode) -> Command {
        let last = self.visible.len().saturating_sub(1);
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(PAGE),
            KeyCode::PageDown => self.selected = (self.selected + PAGE).min(last),
            KeyCode::Enter => {
                self.mode = if self.mode == Mode::Detail { Mode::Queue } else { Mode::Detail };
            }
            KeyCode::Esc if self.mode == Mode::Detail => self.mode = Mode::Queue,
            KeyCode::Esc | KeyCode::Char('q') => return Command::Quit,
            KeyCode::Char('k') => return self.decide(Decision::Keep),
            KeyCode::Char('b') => return self.decide(Decision::Bury),
            KeyCode::Char('r') => {
                let repairable = self.selected_row().map(|r| r.action().is_some_and(|a| a.is_repair()));
                match repairable {
                    Some(true) => return self.decide(Decision::Repair),
                    Some(false) => self.message = Some("Nothing to repair for this feed.".into()),
                    None => {}
                }
            }
            KeyCode::Char('u') => return Command::Undo,
            KeyCode::Char('o') => {
                return self.open(|r| r.html_url.clone(), "This feed has no site address.");
            }
            KeyCode::Char('w') => {
                return self.open(|r| r.wayback_url.clone(), "No Wayback snapshot yet (run `ressac check`).");
            }
            KeyCode::Char('f') => self.next_filter(),
            KeyCode::Char('R') => {
                if self.safe_repair_count() > 0 {
                    self.mode = Mode::ConfirmRepairs;
                } else {
                    self.message = Some("No safe repairs pending.".into());
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
        Command::None
    }

    fn decide(&self, decision: Decision) -> Command {
        self.selected_row().map_or(Command::None, |r| Command::Decide(r.feed_id, decision))
    }

    fn open(&mut self, pick: impl Fn(&GardenRow) -> Option<String>, missing: &str) -> Command {
        match self.selected_row().map(pick) {
            Some(Some(url)) => Command::Open(url),
            Some(None) => {
                self.message = Some(missing.to_string());
                Command::None
            }
            None => Command::None,
        }
    }
}
```

The filter-cycle test relies on the statuses present: Hijacked, Moved, Dead, Fossil, Dormant, Alive (6 filters), then folders `Biblis`, `Blogs`, `Infos`.

- [ ] **Step 4: Run.** `cargo test -p ressac tui::state` → `8 passed`.

- [ ] **Step 5: Commit** ("Garden state and key handling", both attribution lines).

### Task 10.2: Rendering, event loop, `garden` subcommand

**Interfaces:**
- Consumes: `tui::state::*`, `output::glyph`, `ressac_core::garden::{load_rows, decide, undo, apply_safe_repairs}`, `ressac_core::service::Ressac`.
- Produces:
  - `tui::ui::render(frame: &mut Frame, state: &GardenState)`
  - `tui::run(app: &Ressac, ascii: bool) -> anyhow::Result<()>`
  - the `ressac garden` subcommand

- [ ] **Step 1: Write the failing snapshot tests** in `tui/ui.rs`

```rust
//! Garden rendering. Layout: header, list | detail, footer (spec §9).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{GardenState, sample_rows};
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
        insta::assert_snapshot!(draw(&s, 110, 24));
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
        rows[0].title = "A very long feed title that will never fit in a narrow terminal column".into();
        insta::assert_snapshot!(draw(&GardenState::new(rows, false), 60, 20));
    }

    #[test]
    fn garden_empty_queue() {
        let mut rows = sample_rows();
        rows.retain(|r| !r.needs_triage());
        insta::assert_snapshot!(draw(&GardenState::new(rows, false), 80, 12));
    }
}
```

- [ ] **Step 2: Implement** `tui/ui.rs` (above the tests):

```rust
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ressac_core::garden::{GardenRow, safe_repairs};
use ressac_core::model::Status;

use crate::output::glyph;
use crate::tui::state::{GardenState, Mode};

const KEYS: &str = "k keep  r repair  b bury  ↵ details  o site  w wayback  u undo  f filter  R safe repairs  ? help  q quit";

pub fn render(frame: &mut Frame, state: &GardenState) {
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(2)]).areas(frame.area());
    frame.render_widget(Paragraph::new(header_line(state)), header);
    if state.mode == Mode::Detail {
        render_detail(frame, state, body);
    } else {
        let [list, detail] = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).areas(body);
        render_list(frame, state, list);
        render_detail(frame, state, detail);
    }
    let footer_text = match &state.message {
        Some(msg) => Line::from(msg.clone()).bold(),
        None => Line::from(KEYS).dim(),
    };
    frame.render_widget(Paragraph::new(footer_text).wrap(Wrap { trim: true }), footer);
    match state.mode {
        Mode::ConfirmRepairs => popup(frame, " Apply safe repairs ", confirm_lines(state)),
        Mode::Help => popup(frame, " Help ", help_lines()),
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
    spans.push(Span::raw(format!("   triage: {} / {}", p.decided, p.needing)));
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
            let clue = r.diagnosis.as_ref().map(|d| d.evidence.clue.clone()).unwrap_or_default();
            ListItem::new(format!(
                "{} {} {} {}",
                glyph(r.status(), state.ascii),
                truncate(&r.title, title_width),
                truncate(&r.top_folder, 6),
                clue
            ))
        })
        .collect();
    let title = format!(" {} ({}) ", capitalize(&state.filter.label()), state.visible.len());
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_symbol("▶")
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default().with_selected((!state.visible.is_empty()).then_some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn render_detail(frame: &mut Frame, state: &GardenState, area: Rect) {
    let Some(row) = state.selected_row() else {
        let empty = Paragraph::new("Nothing to triage here. Press f to browse other feeds, q to quit.")
            .block(Block::bordered())
            .wrap(Wrap { trim: true });
        frame.render_widget(empty, area);
        return;
    };
    frame.render_widget(
        Paragraph::new(detail_lines(row, state.ascii))
            .block(Block::bordered().title(format!(" {} ", row.title)))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn detail_lines(row: &GardenRow, ascii: bool) -> Vec<Line<'static>> {
    let status = row.status();
    let mut lines = Vec::new();
    let mut head = format!("{} {}", glyph(status, ascii), status.as_str());
    if let Some(d) = &row.diagnosis
        && status == Status::Hijacked
    {
        head.push_str(&format!(" · score {:.2}", d.hijack_score));
    }
    lines.push(Line::from(vec![Span::raw(head).bold(), Span::raw(format!("   {}", row.folder_path))]));
    if let Some(d) = &row.diagnosis {
        lines.push(Line::from(d.reason.clone()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!("Feed  {}", row.xml_url)));
    if let Some(site) = &row.html_url {
        lines.push(Line::from(format!("Site  {site}")));
    }
    if let Some(d) = row.diagnosis.as_ref().filter(|d| !d.evidence.bullets.is_empty()) {
        lines.push(Line::from(""));
        lines.push(Line::from("Evidence").bold());
        lines.extend(d.evidence.bullets.iter().map(|b| Line::from(format!(" • {b}"))));
    }
    if !row.recent.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Last entries").bold());
        lines.extend(
            row.recent
                .iter()
                .map(|(at, title)| Line::from(format!(" {}  {title}", at.format("%Y-%m-%d")))),
        );
    }
    lines.push(Line::from(""));
    if let Some(w) = &row.wayback_url {
        lines.push(Line::from(format!("Wayback   {w}  [w]")));
    }
    if let Some(action) = row.action() {
        lines.push(Line::from(format!("Proposed  {}", action.label())));
    }
    if let Some(dec) = &row.decision {
        lines.push(Line::from(format!(
            "Decision  {} on {}",
            dec.decision.as_str(),
            dec.decided_at.format("%Y-%m-%d")
        )));
    }
    lines
}

fn confirm_lines(state: &GardenState) -> Vec<Line<'static>> {
    let repairs = safe_repairs(&state.rows);
    let mut lines = vec![Line::from(format!("Apply {} safe repairs?", repairs.len())).bold(), Line::from("")];
    for (feed_id, action) in repairs.iter().take(8) {
        let title = state.rows.iter().find(|r| r.feed_id == *feed_id).map_or("", |r| r.title.as_str());
        lines.push(Line::from(format!(" {title}: {}", action.label())));
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

fn popup(frame: &mut Frame, title: &str, lines: Vec<Line<'static>>) {
    let height = (lines.len() as u16).saturating_add(2);
    let [vertical] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(frame.area());
    let [area] = Layout::horizontal([Constraint::Percentage(70)]).flex(Flex::Center).areas(vertical);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title.to_string())).wrap(Wrap { trim: true }),
        area,
    );
}
```

- [ ] **Step 3: Generate and review snapshots.** Run: `INSTA_UPDATE=always cargo test -p ressac tui::ui`. Then open each new file in `crates/ressac/src/tui/snapshots/` and check:
  - the header counts and `triage: 0 / 4`;
  - `▶` on the first row;
  - Bibliobsession's evidence and Wayback line;
  - truncated titles end with `…` in `garden_narrow`;
  - the popups are centered.

  Fix the layout code (not the snapshot) if anything is off. Then re-run `cargo test -p ressac tui::ui` without the env var → `7 passed`.

- [ ] **Step 4: Implement** `tui/mod.rs`

```rust
//! `ressac garden`: the full-screen triage UI.

pub mod state;
pub mod ui;

use chrono::Utc;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ressac_core::garden;
use ressac_core::service::Ressac;

use state::{Command, GardenState};

pub fn run(app: &Ressac, ascii: bool) -> anyhow::Result<()> {
    let mut state = GardenState::new(garden::load_rows(app.store())?, ascii);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, app, &mut state);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &Ressac, state: &mut GardenState) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let store = app.store();
        let outcome: anyhow::Result<Option<String>> = match state.on_key(key) {
            Command::None => continue,
            Command::Quit => return Ok(()),
            Command::Decide(feed_id, decision) => garden::decide(store, feed_id, decision, Utc::now())
                .map(|()| None)
                .map_err(Into::into),
            Command::Undo => garden::undo(store)
                .map(|undone| Some(undone.map_or("Nothing to undo.".into(), |id| format!("Undid the last decision (#{id})."))))
                .map_err(Into::into),
            Command::ApplySafeRepairs => garden::apply_safe_repairs(store, Utc::now())
                .map(|(applied, skipped)| {
                    let mut msg = format!("Applied {applied} safe repairs.");
                    if !skipped.is_empty() {
                        msg.push_str(&format!(" Skipped {}: {}", skipped.len(), skipped.join("; ")));
                    }
                    Some(msg)
                })
                .map_err(Into::into),
            Command::Open(url) => open::that(&url).map(|()| None).map_err(Into::into),
        };
        match outcome {
            Ok(message) => {
                state.reload(garden::load_rows(store)?);
                state.message = message;
            }
            Err(e) => state.message = Some(e.to_string()),
        }
    }
}
```

- [ ] **Step 5: Wire the subcommand** in `main.rs`: add `mod tui;`, a `Garden` variant (doc comment `/// Triage feeds in a full-screen garden`) to `Command`, and this arm in `run`:

```rust
        Command::Garden => tui::run(&app, cli.ascii)?,
```

- [ ] **Step 6: Update `README.md`** by replacing the "Projet en phase de conception" line with:

````markdown
## Usage

```sh
ressac import ~/Documents/vienna.opml   # --dry-run to preview
ressac check                           # fetch, diagnose, look up archives
ressac garden                          # triage: k keep, r repair, b bury, u undo, ? help
ressac export -o clean.opml            # --cemetery for buried feeds
```

Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`.
````

- [ ] **Step 7: Smoke test by hand.** Run:

```bash
cargo run -p ressac -- --db /tmp/ressac-smoke.sqlite import tests/fixtures/vienna-like.opml
cargo run -p ressac -- --db /tmp/ressac-smoke.sqlite check
```

Use the scratchpad path instead of `/tmp` when one is available. The fixture contains real public sites (xkcd, Daring Fireball, Six Colors, YouTube); the `example.org` ones fail as expected.

Running the TUI itself needs a real terminal. If you cannot run it interactively, say so in your report, and rely on the snapshots.

- [ ] **Step 8: Verification gate**, then commit and push ("ressac garden: ratatui triage UI with snapshots; README usage", both attribution lines).
