# Phase 8: the garden as a reader screen, the rhythm menu, `ressac` without subcommand

**Runs after:** phase 7. Phases 4b (`ReaderState`), 5a (`native.rs`), 5b (`reader/mod.rs` and its command executor) and 7 (Journal) are merged.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §2 "Entry point (A)", the garden part of spec §5 (the garden's keys win on a garden page; the bottom line says which key set is active), spec §6 "Rhythm" (the rhythm line, `c` and the override menu), and the CLI item of the plan index's "Phases 5–9" (`Cli.command: Option<Command>`).

**Four sub-phases** (the 5-file rule): 8a prepares the existing garden for embedding (Step 0, two filters, `render_in`, one shared command executor); 8b embeds it in the reader; 8c adds the rhythm line and menu; 8d makes `ressac` without subcommand open the reader.

**Deviations from the plan index** (reconcile before phase 9, which draws the garden pages P800–P802 and the rhythm in Antiope):

1. **Garden destinations are not core `Source`s.** The core's `Source` has no garden variant, so the reader gets its own: `pub enum GardenView { NeedsTriage, AllFeeds, Cemetery }` in `reader/state.rs`, and `ReaderState::garden_view(&self) -> Option<GardenView>` (the garden screen shown, if any); `ReaderState::source()` keeps the last reading source. If phase 4b already models the sources column's garden items, use its names and say so.
2. **New commands and events:** `Command::LoadGarden`, `Command::LoadCadence(FeedId)`, `Event::GardenRows(Vec<GardenRow>)`, `Event::GardenMessage(Option<String>)`, `Event::Cadence(RhythmInfo)`.
3. **New public names:** `tui::state::Filter::{All, Cemetery}`, `GardenState::with_filter`, `tui::ui::render_in`, `tui::apply_garden_command`, `tui::reader::rhythm::{RhythmInfo, rhythm_text, rhythm_line, ago}` (reconciled: no `RhythmMenu`, `MenuOutcome`, `MENU`; 4b's `Overlay::Rhythm` is the menu), `commands::read`, `cli::Command::Read`.
4. **`reader::run` from an async `main`:** `main` is `#[tokio::main]` (multi-thread). `commands::read` calls `tokio::task::block_in_place(|| tui::reader::run(&app, &path))` if phase 5b's `run` is synchronous and spawns its jobs on `tokio::runtime::Handle::current()`; if phase 5b made `run` async, it awaits it instead. A `run` that builds its own `Runtime` would panic inside `main`'s: change it to `Handle::current()` in 8d's first task and say so.

**Builds on (exact, from the code on `main`):**
- `crates/ressac/src/tui/state.rs`: `GardenState { rows, visible, selected, mode, filter, message, ascii }`, `GardenState::new(rows, ascii)`, `on_key(KeyEvent) -> Command`, `reload(rows)`, `selected_row()`, `Mode::{Queue, Detail, ConfirmRepairs, Help}`, `Filter::{Queue, Status(Status), Folder(String)}` with `matches` and `label`, `Command::{None, Quit, Decide(FeedId, Decision), Undo, ApplySafeRepairs, Open(String)}`; test helpers `sample_rows()`, `sample_rows_with_buried()` (feed 3 buried).
- `crates/ressac/src/tui/ui.rs`: `pub fn render(frame, state)` (header / list | detail / footer, popups through `popup(frame, title, lines)` centred on `frame.area()`); eight insta snapshots in `crates/ressac/src/tui/snapshots/` that must not change.
- `crates/ressac/src/tui/mod.rs`: `run(app, ascii)` and its `event_loop`, which executes the garden commands inline (`garden::decide`, `garden::undo` + `output::undo_text`, `garden::apply_safe_repairs` + `output::apply_safe_text`, `open::that`).
- `ressac_core::garden::{GardenRow, load_rows, decide, undo, apply_safe_repairs}`; `GardenRow.diagnosis: Option<Diagnosis>` with `diagnosis.cadence: Cadence { last_entry_at, median_interval_days, entries_per_month, group }`; `GardenRow.buried`.
- `ressac_core::service::Ressac::set_cadence(feed_id, Option<CadenceGroup>) -> Result<CadenceRow>`; `Store::cadence_row(feed_id) -> Result<CadenceRow>` (never-diagnosed feed → computed Silent); `CadenceRow { feed_id, effective_group, computed_group, is_override }`; `Store::diagnosis(feed_id) -> Result<Option<Diagnosis>>`; `Error::{UnknownFeed, FeedBuried}`.
- `crate::commands::feed::group_title(CadenceGroup) -> &'static str` (`"Daily"` … `"Silent"`), public since sub-project 3: reused, not duplicated.
- `crate::commands::{Globals, open_with_feeds, NO_FEEDS}`; `crates/ressac/tests/cli.rs` helpers `ressac`, `stdout`, `stderr`.
- The app's wording (`GardenModel.rhythmText` and `ago`): `Rhythm: Daily · median 0.4 d · last entry 2 h ago`, ` (set by you)`; median `%.1f` under 10 days, rounded integer from 10; ago: `N min ago` under an hour, `N h ago` under 48 h, `N d ago` under 60 days, then `N mo ago` (days / 30).

---

**Reconciled with the other phases (controller, 2026-10-16; these rules override the code below where they differ):**
- `ReaderState`'s fields are public (phase 4b); `message` is a field (`s.message.as_deref()`), the source switch is 4b's `go_to(source)`, the clock is `state.local`.
- The rhythm override menu is phase 4b's `Overlay::Rhythm { feed_id, choice }` (with `RHYTHM_CHOICES`, `rhythm_menu()` and `on_rhythm_key`, sending `Command::SetCadence`). 8c does **not** add `RhythmMenu`, `MenuOutcome` or a `rhythm_menu` field: it binds `c` on the garden pages to 4b's menu (the selected garden row's feed) and its tests read `s.overlay` (`matches!(s.overlay, Overlay::Rhythm { feed_id: 12, .. })`) instead of `s.rhythm_menu()`. 8c keeps `RhythmInfo`, `rhythm_text`, `rhythm_line`, `ago`, `Command::LoadCadence`, `Event::Cadence(RhythmInfo)` and the line on the garden page and on a feed source.
- `RhythmInfo` is filled from phase 1b's `CadenceDetail` (`Store::cadence_details()`, over `v_cadence`: effective and computed groups, override flag, median, last entry), not from `CadenceRow` plus the diagnosis.
- `commands::read` and `Command::Read` belong to 5b.3; 8d makes `Cli.command` an `Option` and routes `None` to `commands::read`, which calls `tui::reader::run(&app, &g.db, &settings)` directly (synchronous, like the garden's `tui::run`).
- `JobOutcome::EditionBuiltElsewhere` no longer exists.

## Sub-phase 8a: the garden, ready to be embedded

**Files (3):** `crates/ressac/src/tui/state.rs`, `crates/ressac/src/tui/ui.rs`, `crates/ressac/src/tui/mod.rs`.

### Task 8a.0: Step 0, dead-code check on `tui/state.rs` and `tui/ui.rs`

Both files are over 300 lines (616, 402) and this sub-phase restructures them.

- [ ] **Step 1: Look for dead code, debug output and unused imports.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd crates/ressac/src/tui
grep -n "dbg!\|println!\|eprintln!\|todo!\|unimplemented!\|#\[allow(dead_code\|#\[allow(unused" state.rs ui.rs
for f in $(grep -ho "pub fn [a-z_]*" state.rs ui.rs | awk '{print $3}' | sort -u); do
  printf '%s %s\n' "$f" "$(grep -rw "$f" ../ --include=*.rs | grep -v "fn $f" | wc -l)"
done
cd - >/dev/null
cargo clippy -p ressac --all-targets -- -D warnings -W dead_code
```

Expected: no debug macro, no allow attribute; every `pub fn` used at least once outside its definition; clippy clean.

- [ ] **Step 2: Only if something is found**, remove it and commit it alone:

```bash
git add crates/ressac/src/tui/state.rs crates/ressac/src/tui/ui.rs
git commit -m "Garden TUI: Step 0, dead code removed before embedding it in the reader

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

If nothing is found, write "Step 0: nothing to remove" in the report and make no commit.

### Task 8a.1: two filters, `All` and `Cemetery`

The reader shows the garden as three destinations (spec §4 "Sources"): Needs triage is today's `Filter::Queue`; All feeds and Cemetery need two filters.

- [ ] **Step 1: Write the failing tests** in `state.rs`'s test module:

```rust
    #[test]
    fn all_feeds_lists_the_active_feeds_and_the_cemetery_the_buried_ones() {
        let all = GardenState::with_filter(sample_rows_with_buried(), false, Filter::All);
        assert!(all.visible.iter().all(|&i| !all.rows[i].buried));
        assert_eq!(all.visible.len(), all.rows.iter().filter(|r| !r.buried).count());
        assert_eq!(all.filter.label(), "all feeds");

        let cemetery = GardenState::with_filter(sample_rows_with_buried(), false, Filter::Cemetery);
        assert_eq!(cemetery.visible.iter().map(|&i| cemetery.rows[i].feed_id).collect::<Vec<_>>(), vec![3]);
        assert_eq!(cemetery.filter.label(), "cemetery");
    }

    #[test]
    fn f_from_all_feeds_starts_the_usual_cycle() {
        let mut s = GardenState::with_filter(sample_rows(), false, Filter::All);
        s.on_key(key(KeyCode::Char('f')));
        assert_eq!(s.filter, Filter::Queue);
    }

    #[test]
    fn new_still_opens_on_the_queue() {
        assert_eq!(GardenState::new(sample_rows(), false).filter, Filter::Queue);
    }
```

- [ ] **Step 2: Run them to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::state
```

Expected: `no variant named All`, `no function named with_filter`.

- [ ] **Step 3: Implement.** In `Filter`:

```rust
    /// Every active feed (the reader's "All feeds").
    All,
    /// The buried feeds (the reader's "Cemetery").
    Cemetery,
```

in `label`: `Filter::All => "all feeds".to_string(), Filter::Cemetery => "cemetery".to_string(),`; in `matches`: `Filter::All => !row.buried, Filter::Cemetery => row.buried,`. `filters()` is unchanged (its cycle stays Queue, statuses, folders; from `All` or `Cemetery`, `next_filter` finds no position and starts at Queue, as the test pins). Then:

```rust
    /// A garden opened on another filter than the queue (the reader's garden destinations).
    pub fn with_filter(rows: Vec<GardenRow>, ascii: bool, filter: Filter) -> Self {
        let mut state = GardenState::new(rows, ascii);
        state.filter = filter;
        state.selected = 0;
        state.refilter();
        state
    }
```

- [ ] **Step 4: Run the garden tests.**

```bash
cargo test -p ressac --bin ressac tui::
```

Expected: all pass, the eight `ui` snapshots unchanged.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/state.rs
git commit -m "Garden: All feeds and Cemetery filters, and a garden opened on a given filter

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8a.2: `render_in` and one garden command executor

- [ ] **Step 1: Write the failing tests.** In `ui.rs`'s test module:

```rust
    #[test]
    fn render_in_draws_the_garden_inside_an_area_only() {
        let state = GardenState::new(sample_rows(), false);
        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = Rect::new(30, 2, 110, 20);
                render_in(frame, area, &state);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Nothing drawn left of the area or above it.
        for y in 0..24 {
            for x in 0..30 {
                assert_eq!(buffer[(x, y)].symbol(), " ", "({x},{y})");
            }
        }
        for x in 0..140 {
            assert_eq!(buffer[(x, 0)].symbol(), " ");
        }
        let text: String = (2..22).map(|y| (30..140).map(|x| buffer[(x, y)].symbol()).collect::<String>() + "\n").collect();
        assert!(text.contains("Bibliobsession 2.0"));
    }

    #[test]
    fn popups_stay_inside_the_area() {
        let mut state = GardenState::new(sample_rows(), false);
        state.on_key(ratatui::crossterm::event::KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
        terminal.draw(|frame| render_in(frame, Rect::new(30, 2, 110, 20), &state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        for y in 0..24 {
            for x in 0..30 {
                assert_eq!(buffer[(x, y)].symbol(), " ");
            }
        }
    }
```

In `mod.rs`, a test module for the shared executor:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::model::Decision;

    #[test]
    fn the_shared_executor_decides_and_undoes() {
        let dir = tempfile::tempdir().unwrap();
        let app = Ressac::open(&dir.path().join("r.sqlite")).unwrap();
        app.import_opml(include_str!("../../../../tests/fixtures/vienna-like.opml"), false).unwrap();
        let feed = app.store().feeds().unwrap()[0].id;
        assert_eq!(apply_garden_command(&app, state::Command::Decide(feed, Decision::Bury)).unwrap(), None);
        assert!(app.store().feeds().unwrap().iter().any(|f| f.id == feed && f.state == "buried"));
        let message = apply_garden_command(&app, state::Command::Undo).unwrap();
        assert!(message.is_some());
        assert_eq!(apply_garden_command(&app, state::Command::None).unwrap(), None);
    }
}
```

(If `Ressac::import_opml` needs feeds to be fetched before a decision is accepted, use the garden test fixture the core's `garden.rs` tests use; keep the decide/undo assertions.)

- [ ] **Step 2: Run them to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::ui tui::tests
```

Expected: `cannot find function render_in`, `cannot find function apply_garden_command`.

- [ ] **Step 3: Implement.** In `ui.rs`:

```rust
pub fn render(frame: &mut Frame, state: &GardenState) {
    render_in(frame, frame.area(), state);
}

/// The garden inside `area` (the whole frame for `ressac garden`, the reader's columns for
/// the garden destinations). Popups are centred on `area`.
pub fn render_in(frame: &mut Frame, area: Rect, state: &GardenState) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(area);
    // … the existing body of `render`, unchanged, with `area` for `frame.area()` …
    match state.mode {
        Mode::ConfirmRepairs => popup(frame, area, " Apply safe repairs ", confirm_lines(state)),
        Mode::Help => popup(frame, area, " Help ", help_lines()),
        Mode::Queue | Mode::Detail => {}
    }
}
```

and `popup(frame, area, title, lines)` centres on `area` instead of `frame.area()`. The eight existing snapshots must stay byte-identical (`render` passes `frame.area()`).

In `mod.rs`, extract the inline match of `event_loop` into:

```rust
/// Runs one garden command against the database. `Ok(Some(message))` is a line to show,
/// `Ok(None)` nothing; `Err` is shown as the message too. Shared by `ressac garden` and the
/// reader's garden destinations.
pub fn apply_garden_command(app: &Ressac, command: state::Command) -> anyhow::Result<Option<String>> {
    let store = app.store();
    match command {
        state::Command::None | state::Command::Quit => Ok(None),
        state::Command::Decide(feed_id, decision) => {
            garden::decide(store, feed_id, decision, Utc::now())?;
            Ok(None)
        }
        state::Command::Undo => Ok(Some(crate::output::undo_text(garden::undo(store)?))),
        state::Command::ApplySafeRepairs => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            Ok(Some(crate::output::apply_safe_text(applied, &skipped)))
        }
        state::Command::Open(url) => {
            open::that(&url)?;
            Ok(None)
        }
    }
}
```

and `event_loop` becomes: `Command::None => continue, Command::Quit => return Ok(()), command => match apply_garden_command(app, command) { Ok(message) => { state.reload(garden::load_rows(app.store())?); state.message = message; } Err(e) => state.message = Some(e.to_string()) }`. Behaviour of `ressac garden` is unchanged.

- [ ] **Step 4: Run the garden and CLI tests.**

```bash
cargo test -p ressac
```

Expected: all pass; `cargo insta pending-snapshots` lists nothing (snapshots unchanged).

- [ ] **Step 5: Commit, then the gate.**

```bash
git add crates/ressac/src/tui/ui.rs crates/ressac/src/tui/mod.rs
git commit -m "Garden: render inside an area, one command executor for the garden and the reader

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
```

---

## Sub-phase 8b: the garden destinations in the reader

**Files (3):** `crates/ressac/src/tui/reader/state.rs`, `crates/ressac/src/tui/reader/native.rs`, `crates/ressac/src/tui/reader/mod.rs`.

### Task 8b.1: garden destinations in the state, with the garden's keys

- [ ] **Step 1: Write the failing tests** in `reader/state.rs`'s test module:

```rust
mod garden_tests {
    use super::super::*;
    use crate::tui::state::{self as garden_state, sample_rows, Mode};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ressac_core::model::Decision;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn on_needs_triage() -> ReaderState {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        let commands = s.open_garden(GardenView::NeedsTriage);
        assert!(commands.contains(&Command::LoadGarden));
        s.on_event(Event::GardenRows(sample_rows()));
        s
    }

    #[test]
    fn a_garden_destination_shows_its_filter() {
        let s = on_needs_triage();
        assert_eq!(s.garden_view(), Some(GardenView::NeedsTriage));
        assert_eq!(s.garden().unwrap().filter, garden_state::Filter::Queue);
        let mut all = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        all.open_garden(GardenView::AllFeeds);
        all.on_event(Event::GardenRows(sample_rows()));
        assert_eq!(all.garden().unwrap().filter, garden_state::Filter::All);
    }

    #[test]
    fn the_garden_s_keys_win_on_a_garden_page() {
        let mut s = on_needs_triage();
        // k keeps (not "previous entry"), r repairs, b buries, u undoes.
        let keep = s.on_key(key(KeyCode::Char('k')));
        assert!(matches!(keep.as_slice(), [Command::Garden(garden_state::Command::Decide(_, Decision::Keep))]));
        assert_eq!(s.on_key(key(KeyCode::Char('u'))), vec![Command::Garden(garden_state::Command::Undo)]);
        // ↓ moves the garden's cursor.
        let before = s.garden().unwrap().selected;
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.garden().unwrap().selected, before + 1);
    }

    #[test]
    fn q_quits_the_reader_and_esc_goes_back_to_the_sources() {
        let mut s = on_needs_triage();
        assert_eq!(s.on_key(key(KeyCode::Esc)), Vec::<Command>::new());
        assert_eq!(s.focus(), Column::Sources);
        let mut s = on_needs_triage();
        assert_eq!(s.on_key(key(KeyCode::Char('q'))), vec![Command::Quit]);
    }

    #[test]
    fn esc_closes_the_garden_s_detail_first() {
        let mut s = on_needs_triage();
        s.on_key(key(KeyCode::Enter));
        assert_eq!(s.garden().unwrap().mode, Mode::Detail);
        s.on_key(key(KeyCode::Esc));
        assert_eq!(s.garden().unwrap().mode, Mode::Queue);
        assert_ne!(s.focus(), Column::Sources);
    }

    #[test]
    fn rows_reload_after_a_decision_keep_the_position() {
        let mut s = on_needs_triage();
        s.on_key(key(KeyCode::Down));
        let selected = s.garden().unwrap().selected;
        s.on_event(Event::GardenRows(sample_rows()));
        assert_eq!(s.garden().unwrap().selected, selected);
        s.on_event(Event::GardenMessage(Some("Restored Re/code.".into())));
        assert_eq!(s.garden().unwrap().message.as_deref(), Some("Restored Re/code."));
    }

    #[test]
    fn t_and_left_still_belong_to_the_reader() {
        let mut s = on_needs_triage();
        assert!(s.on_key(key(KeyCode::Char('t'))).contains(&Command::SaveSettings));
        assert_eq!(s.look(), Look::Antiope);
        s.on_key(key(KeyCode::Left));
        assert_eq!(s.focus(), Column::Sources);
    }
}
```

- [ ] **Step 2: Run to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::reader::state::tests::garden_tests
```

Expected: `cannot find type GardenView`, `no method open_garden`.

- [ ] **Step 3: Implement in `reader/state.rs`.**

```rust
/// The reader's garden destinations (spec §4): Needs triage, All feeds, Cemetery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GardenView {
    NeedsTriage,
    AllFeeds,
    Cemetery,
}

impl GardenView {
    pub fn title(self) -> &'static str {
        match self {
            GardenView::NeedsTriage => "Needs triage",
            GardenView::AllFeeds => "All feeds",
            GardenView::Cemetery => "Cemetery",
        }
    }

    fn filter(self) -> crate::tui::state::Filter {
        use crate::tui::state::Filter;
        match self {
            GardenView::NeedsTriage => Filter::Queue,
            GardenView::AllFeeds => Filter::All,
            GardenView::Cemetery => Filter::Cemetery,
        }
    }
}
```

Fields: `garden_view: Option<GardenView>`, `garden: Option<crate::tui::state::GardenState>`. Methods:

```rust
    pub fn garden_view(&self) -> Option<GardenView> { self.garden_view }
    pub fn garden(&self) -> Option<&crate::tui::state::GardenState> { self.garden.as_ref() }

    /// Choosing a garden item in the sources column: the garden takes the entries and reader
    /// columns, the focus moves to it.
    pub fn open_garden(&mut self, view: GardenView) -> Vec<Command> {
        self.garden_view = Some(view);
        self.garden = None;
        self.focus = Column::Entries;
        vec![Command::LoadGarden]
    }
```

In `on_event`:
- `Event::GardenRows(rows)`: if `self.garden` is `Some`, `reload(rows)` (the garden keeps its cursor); else `self.garden = Some(GardenState::with_filter(rows, false, view.filter()))` for the current `garden_view`.
- `Event::GardenMessage(message)`: `garden.message = message`.

In `on_key`, **first**, when `self.garden_view.is_some()` and the focus is not `Column::Sources`:

```rust
        if let (Some(_), Some(garden)) = (self.garden_view, self.garden.as_mut()) {
            use crate::tui::state::{Command as G, Mode};
            match key.code {
                KeyCode::Char('q') if matches!(garden.mode, Mode::Queue | Mode::Detail) => return vec![Command::Quit],
                KeyCode::Esc if garden.mode == Mode::Queue => {
                    self.focus = Column::Sources;
                    return Vec::new();
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Tab if garden.mode == Mode::Queue => {
                    self.focus = Column::Sources;
                    return Vec::new();
                }
                KeyCode::Char('t') => { /* fall through to the reader's own `t` below */ }
                _ => {
                    return match garden.on_key(key) {
                        G::None => Vec::new(),
                        G::Quit => vec![Command::Quit], // Ctrl-C
                        command => vec![Command::Garden(command)],
                    };
                }
            }
        }
```

`c` is handled in 8c. Choosing a reading source in the sources column clears `garden_view` and `garden`.

The sources column's list (phase 4b/5a) gains three items after the rhythm groups: `Needs triage` with `counts.needs_triage`, `All feeds`, `Cemetery`; `Enter` (or selecting them, as for other sources) calls `open_garden`. If phase 4b already lists them, wire its items to `open_garden`.

- [ ] **Step 4: Run the state tests.**

```bash
cargo test -p ressac --bin ressac tui::reader::state
```

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/state.rs
git commit -m "Reader: Needs triage, All feeds and Cemetery as destinations, with the garden's keys

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8b.2: drawing the garden destinations and running their commands

- [ ] **Step 1: Write the failing tests.** In `native.rs`'s test module:

```rust
    #[test]
    fn garden_destination_wide() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 30);
        s.open_garden(GardenView::NeedsTriage);
        s.on_event(Event::GardenRows(crate::tui::state::sample_rows()));
        let out = draw(&s, 140, 30);
        assert!(out.contains("Bibliobsession 2.0"));
        assert!(out.contains("k keep"), "the bottom line shows the garden's keys");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn garden_destination_narrow_is_one_column() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 70, 24);
        s.open_garden(GardenView::Cemetery);
        s.on_event(Event::GardenRows(crate::tui::state::sample_rows_with_buried()));
        insta::assert_snapshot!(draw(&s, 70, 24));
    }
```

In `reader/mod.rs`'s test module (phase 5b's executor helpers):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn garden_commands_reload_the_rows_and_report_the_message() {
        let (app, _server, _dir) = test_app_with_two_feeds().await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        execute(&app, Command::LoadGarden, tx.clone()).await;
        let Some(Event::GardenRows(rows)) = rx.recv().await else { panic!("rows") };
        let feed = rows[0].feed_id;
        execute(&app, Command::Garden(crate::tui::state::Command::Decide(feed, ressac_core::model::Decision::Bury)), tx).await;
        let events: Vec<Event> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(events.iter().any(|e| matches!(e, Event::GardenRows(r) if r.iter().any(|row| row.feed_id == feed && row.buried))));
        assert!(events.iter().any(|e| matches!(e, Event::GardenMessage(None))));
        assert!(events.iter().any(|e| matches!(e, Event::Sources(_))), "counts change after a bury");
    }
```

- [ ] **Step 2: Run to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::reader
```

- [ ] **Step 3: Implement.**
  - `native.rs`: when `state.garden_view()` and `state.garden()` are `Some`, the entries and reader columns are one area (the union of `ColumnRects.entries` and `.reader`, or the single visible column in `LayoutClass::One`), drawn with `crate::tui::ui::render_in(frame, area, garden)`; the sources column, when visible, highlights the garden item. The reader's own bottom line is not drawn there (the garden's footer is the bottom line, which says the garden's keys are active). Until the rows arrive: `Loading…` dimmed.
  - `reader/mod.rs` executor:
    - `Command::LoadGarden` → `garden::load_rows(app.store())` → `Event::GardenRows(rows)`;
    - `Command::Garden(command)` → `crate::tui::apply_garden_command(app, command)`; then always `Event::GardenRows(garden::load_rows(..)?)`, `Event::GardenMessage(result message, or the error's text)`, and a reload of the sources (`Event::Sources(..)`, as `LoadSources` does) because Needs triage's count and the rhythm groups change.

- [ ] **Step 4: Review the snapshots and run the gate.**

```bash
cargo insta review
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
```

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/native.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader: the garden destinations drawn in the reader's columns, their commands run

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Sub-phase 8c: the rhythm line and the override menu

**Files (4):** Create `crates/ressac/src/tui/reader/rhythm.rs`; modify `reader/state.rs`, `reader/native.rs`, `reader/mod.rs`.

### Task 8c.1: the rhythm text and the menu (pure)

- [ ] **Step 1: Write the failing tests.** Create `rhythm.rs` with its test module and declare `mod rhythm;` in `reader/mod.rs` (`pub mod` if phase 9 reaches it from `antiope.rs` as `super::rhythm`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ressac_core::model::CadenceGroup;

    fn now() -> DateTime<Utc> {
        "2026-10-16T18:00:00Z".parse().unwrap()
    }
    fn info(effective: CadenceGroup, is_override: bool, median: Option<f64>, last_h: Option<i64>) -> RhythmInfo {
        RhythmInfo { feed_id: 12, effective, computed: CadenceGroup::Weekly, is_override,
                     median_days: median, last_entry_at: last_h.map(|h| now() - Duration::hours(h)) }
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn the_rhythm_text_matches_the_app() {
        assert_eq!(rhythm_text(&info(CadenceGroup::Daily, false, Some(0.42), Some(2)), now()),
                   "Daily · median 0.4 d · last entry 2 h ago");
        assert_eq!(rhythm_text(&info(CadenceGroup::Daily, true, Some(0.42), Some(2)), now()),
                   "Daily · median 0.4 d · last entry 2 h ago (set by you)");
        assert_eq!(rhythm_text(&info(CadenceGroup::Monthly, false, Some(12.6), None), now()),
                   "Monthly · median 13 d");
        assert_eq!(rhythm_text(&info(CadenceGroup::Silent, false, None, None), now()), "Silent");
        assert_eq!(rhythm_line(&info(CadenceGroup::Rare, false, None, None), now()), "Rhythm: Rare");
    }

    #[test]
    fn ago_matches_the_app() {
        let t = now();
        assert_eq!(ago(t - Duration::minutes(45), t), "45 min ago");
        assert_eq!(ago(t - Duration::hours(47), t), "47 h ago");
        assert_eq!(ago(t - Duration::days(3), t), "3 d ago");
        assert_eq!(ago(t - Duration::days(400), t), "13 mo ago");
        assert_eq!(ago(t + Duration::minutes(5), t), "0 min ago");
    }

    #[test]
    fn the_menu_starts_on_the_current_choice_and_returns_a_group() {
        let mut auto = RhythmMenu::new(12, None);
        assert_eq!(MENU[auto.cursor].0, "Auto");
        let mut forced = RhythmMenu::new(12, Some(CadenceGroup::Rare));
        assert_eq!(MENU[forced.cursor].0, "Rare");
        assert_eq!(auto.on_key(key(KeyCode::Down)), MenuOutcome::Stay);
        assert_eq!(auto.on_key(key(KeyCode::Enter)), MenuOutcome::Choose(Some(CadenceGroup::Daily)));
        assert_eq!(forced.on_key(key(KeyCode::Char('k'))), MenuOutcome::Stay);
        assert_eq!(MENU[forced.cursor].0, "Monthly");
        assert_eq!(forced.on_key(key(KeyCode::Esc)), MenuOutcome::Close);
        let mut top = RhythmMenu::new(12, None);
        top.on_key(key(KeyCode::Up));
        assert_eq!(top.cursor, 0, "clamped");
    }
}
```

- [ ] **Step 2: Run to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::reader::rhythm
```

- [ ] **Step 3: Implement** above the tests:

```rust
//! A feed's rhythm line and the override menu (spec §6 "Rhythm"), worded as the app's
//! `GardenModel.rhythmText`.

use chrono::{DateTime, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ressac_core::model::{CadenceGroup, FeedId};

use crate::commands::feed::group_title;

/// What the rhythm line needs: the effective and computed groups (core `CadenceRow`) and the
/// median gap and last entry (the feed's stored diagnosis).
#[derive(Debug, Clone, PartialEq)]
pub struct RhythmInfo {
    pub feed_id: FeedId,
    pub effective: CadenceGroup,
    pub computed: CadenceGroup,
    pub is_override: bool,
    pub median_days: Option<f64>,
    pub last_entry_at: Option<DateTime<Utc>>,
}

/// "Daily · median 0.4 d · last entry 2 h ago", plus " (set by you)".
pub fn rhythm_text(info: &RhythmInfo, now: DateTime<Utc>) -> String {
    let mut parts = vec![group_title(info.effective).to_string()];
    if let Some(median) = info.median_days {
        parts.push(if median < 10.0 {
            format!("median {median:.1} d")
        } else {
            format!("median {} d", median.round() as i64)
        });
    }
    if let Some(last) = info.last_entry_at {
        parts.push(format!("last entry {}", ago(last, now)));
    }
    let mut text = parts.join(" · ");
    if info.is_override {
        text.push_str(" (set by you)");
    }
    text
}

/// "Rhythm: " + the rhythm text: the inspector's line.
pub fn rhythm_line(info: &RhythmInfo, now: DateTime<Utc>) -> String {
    format!("Rhythm: {}", rhythm_text(info, now))
}

/// "45 min ago", "2 h ago" (under two days), "3 d ago" (under 60 days), then "13 mo ago".
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - then).num_minutes().max(0);
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours} h ago");
    }
    let days = hours / 24;
    if days < 60 {
        return format!("{days} d ago");
    }
    format!("{} mo ago", days / 30)
}

/// The override menu's items, in the app's order.
pub const MENU: [(&str, Option<CadenceGroup>); 6] = [
    ("Auto", None),
    ("Daily", Some(CadenceGroup::Daily)),
    ("Weekly", Some(CadenceGroup::Weekly)),
    ("Monthly", Some(CadenceGroup::Monthly)),
    ("Rare", Some(CadenceGroup::Rare)),
    ("Silent", Some(CadenceGroup::Silent)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    Stay,
    Close,
    Choose(Option<CadenceGroup>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmMenu {
    pub feed_id: FeedId,
    pub cursor: usize,
}

impl RhythmMenu {
    /// Opens on the feed's override, or on Auto.
    pub fn new(feed_id: FeedId, current_override: Option<CadenceGroup>) -> Self {
        let cursor = MENU.iter().position(|(_, g)| *g == current_override).unwrap_or(0);
        RhythmMenu { feed_id, cursor }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> MenuOutcome {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.cursor = (self.cursor + 1).min(MENU.len() - 1),
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Enter => return MenuOutcome::Choose(MENU[self.cursor].1),
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => return MenuOutcome::Close,
            _ => {}
        }
        MenuOutcome::Stay
    }
}
```

- [ ] **Step 4: Run** `cargo test -p ressac --bin ressac tui::reader::rhythm`. Expected: 3 passed.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/rhythm.rs crates/ressac/src/tui/reader/mod.rs
git commit -m "Reader: the rhythm line and the override menu, worded as the app's

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8c.2: `c`, the line on the garden page and on a feed source, the executor

- [ ] **Step 1: Write the failing tests.** In `reader/state.rs` (inside `garden_tests` or a new `rhythm_tests` module):

```rust
    #[test]
    fn moving_in_the_garden_loads_the_selected_feed_s_rhythm() {
        let mut s = on_needs_triage();
        let first = s.garden().unwrap().selected_row().unwrap().feed_id;
        // Opening the rows already asked for the first feed's rhythm.
        let mut s2 = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        s2.open_garden(GardenView::NeedsTriage);
        assert!(s2.on_event(Event::GardenRows(sample_rows())).contains(&Command::LoadCadence(first)));
        let commands = s.on_key(key(KeyCode::Down));
        let second = s.garden().unwrap().selected_row().unwrap().feed_id;
        assert!(commands.contains(&Command::LoadCadence(second)));
    }

    #[test]
    fn c_opens_the_menu_and_enter_sets_the_rhythm() {
        let mut s = on_needs_triage();
        let feed = s.garden().unwrap().selected_row().unwrap().feed_id;
        s.on_event(Event::Cadence(RhythmInfo { feed_id: feed, effective: CadenceGroup::Weekly, computed: CadenceGroup::Weekly,
                                               is_override: false, median_days: Some(1.5), last_entry_at: None }));
        assert!(s.on_key(key(KeyCode::Char('c'))).is_empty());
        assert!(s.rhythm_menu().is_some());
        // The menu has the keys now: ↓ then Enter forces Daily.
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Enter)), vec![Command::SetCadence(feed, Some(CadenceGroup::Daily))]);
        assert!(s.rhythm_menu().is_none());
    }

    #[test]
    fn c_on_a_buried_feed_says_so() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        s.open_garden(GardenView::Cemetery);
        s.on_event(Event::GardenRows(crate::tui::state::sample_rows_with_buried()));
        assert!(s.on_key(key(KeyCode::Char('c'))).is_empty());
        assert!(s.rhythm_menu().is_none());
        assert_eq!(s.garden().unwrap().message.as_deref(), Some("Feed #3 is buried."));
    }

    #[test]
    fn c_on_a_feed_source_opens_the_menu_too() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        let commands = s.go_to(Source::Feed(12));
        assert!(commands.contains(&Command::LoadCadence(12)));
        s.on_event(Event::Cadence(RhythmInfo { feed_id: 12, effective: CadenceGroup::Rare, computed: CadenceGroup::Rare,
                                               is_override: false, median_days: None, last_entry_at: None }));
        s.on_key(key(KeyCode::Char('c')));
        assert_eq!(s.rhythm_menu().map(|m| m.feed_id), Some(12));
    }
```

`ReaderState::go_to(Source) -> Vec<Command>` is phase 4b's way of choosing a source (reconciled name).

In `reader/mod.rs`'s tests:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn set_cadence_returns_the_new_line_and_reloads_the_counts() {
        let (app, _server, _dir) = test_app_with_two_feeds().await;
        let feed = app.store().feeds().unwrap()[0].id;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        execute(&app, Command::SetCadence(feed, Some(ressac_core::model::CadenceGroup::Daily)), tx.clone()).await;
        let events: Vec<Event> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(events.iter().any(|e| matches!(e, Event::Cadence(i) if i.feed_id == feed && i.is_override)));
        assert!(events.iter().any(|e| matches!(e, Event::Sources(_))));
    }
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement.**
  - `state.rs`: fields `rhythm: Option<RhythmInfo>`, `rhythm_menu: Option<RhythmMenu>`; `pub fn rhythm(&self) -> Option<&RhythmInfo>`, `pub fn rhythm_menu(&self) -> Option<&RhythmMenu>`.
  - When the menu is open, `on_key` sends every key to it **before anything else**: `Choose(group)` → close it, `vec![Command::SetCadence(menu.feed_id, group)]`; `Close` → close it, nothing; `Stay` → nothing.
  - The selected feed: on a garden page, `garden.selected_row().map(|r| r.feed_id)`; on a `Source::Feed(id)`, `id`. Whenever it changes (after `GardenRows` and after a garden key, compare before/after; on choosing a feed source), push `Command::LoadCadence(id)` and clear `self.rhythm` if it belongs to another feed.
  - `c` (in the garden branch of 8b, before forwarding to the garden; and in the reading keys on a `Source::Feed`): if the selected garden row is buried → `garden.message = Some(format!("Feed #{id} is buried."))`; else open `RhythmMenu::new(id, self.rhythm.filter(|r| r.feed_id == id && r.is_override).map(|r| r.effective))`.
  - `Event::Cadence(info)`: keep it when `info.feed_id` is the selected feed.
  - `mod.rs` executor:
    - `Command::LoadCadence(id)` → `let row = app.store().cadence_row(id)?; let cadence = app.store().diagnosis(id)?.map(|d| d.cadence);` → `Event::Cadence(RhythmInfo { feed_id: id, effective: row.effective_group, computed: row.computed_group, is_override: row.is_override, median_days: cadence.as_ref().and_then(|c| c.median_interval_days), last_entry_at: cadence.and_then(|c| c.last_entry_at) })`.
    - `Command::SetCadence(id, group)` → `app.set_cadence(id, group)`; on `Ok`, the same `Event::Cadence` (re-read as above) and a sources reload (the rhythm groups' counts); on `Err(Error::UnknownFeed | Error::FeedBuried)` → the error's sentence as the garden message (`Event::GardenMessage(Some(..))`) or the reader's message on a feed source; other errors likewise, as text.
  - `native.rs`:
    - garden page: the line `rhythm_line(info, now)` dimmed, right under the garden's header (split one row off the top of the garden area before `render_in`); nothing for a buried feed or before the info arrives;
    - feed source: the same line under the entries column's header;
    - the menu: a popup centred on the reader's area, title ` Rhythm `, the six items, the cursor's in reverse video, the current override (or `Auto`) marked with `•`; the bottom line reads `↑↓ choose  ↵ set  esc close`.
  Every string drawn here is ours (no feed text), except nothing: no `printable` needed beyond what 8b draws.

- [ ] **Step 4: Snapshot the garden page with its rhythm line and the open menu, review, run the gate.**

```rust
    #[test]
    fn garden_rhythm_line_and_menu() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 30);
        s.open_garden(GardenView::NeedsTriage);
        s.on_event(Event::GardenRows(crate::tui::state::sample_rows()));
        let feed = s.garden().unwrap().selected_row().unwrap().feed_id;
        s.on_event(Event::Cadence(RhythmInfo { feed_id: feed, effective: CadenceGroup::Weekly, computed: CadenceGroup::Rare,
                                               is_override: true, median_days: Some(1.5), last_entry_at: None }));
        let line = draw(&s, 140, 30);
        assert!(line.contains("Rhythm: Weekly · median 1.5 d (set by you)"));
        s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        insta::assert_snapshot!(draw(&s, 140, 30));
    }
```

`draw` renders with the state's clock; if `native.rs` takes "now" from the state (phase 5b's `Tick`), send a `Tick` first so the snapshot is stable.

```bash
cargo insta review
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
```

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/state.rs crates/ressac/src/tui/reader/native.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader: c forces a feed's rhythm from the garden or a feed, its line shown above

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Sub-phase 8d: `ressac` without subcommand opens the reader

**Files (4):** `crates/ressac/src/cli.rs`, `crates/ressac/src/main.rs`, `crates/ressac/src/commands/mod.rs`, `crates/ressac/tests/cli.rs`.

Step 0: `cli.rs` (283 lines) and `main.rs` (105) are under or barely over the limit and are not restructured beyond the dispatch; no Step 0 commit.

### Task 8d.1: `Option<Command>` and `Read`

- [ ] **Step 1: Write the failing tests.** In `cli.rs`'s test module:

```rust
    #[test]
    fn no_subcommand_and_read_both_open_the_reader() {
        assert!(Cli::try_parse_from(["ressac"]).unwrap().command.is_none());
        let with_db = Cli::try_parse_from(["ressac", "--db", "/tmp/x.sqlite"]).unwrap();
        assert!(with_db.command.is_none());
        assert_eq!(with_db.db.as_deref(), Some(std::path::Path::new("/tmp/x.sqlite")));
        assert!(matches!(Cli::try_parse_from(["ressac", "read"]).unwrap().command, Some(Command::Read)));
    }
```

and every existing `matches!(x.command, Command::…)` in this module becomes `matches!(x.command, Some(Command::…))` (`bare_garden_still_opens_the_tui`, `feed_is_repeatable_and_exclusive_with_folder`, `page_set_parses_its_kind_id_and_page`, and any other; grep `\.command` in the module).

In `crates/ressac/tests/cli.rs`:

```rust
#[test]
fn bare_ressac_and_read_give_the_first_run_hint_on_an_empty_database() {
    let home = tempfile::tempdir().unwrap();
    for args in [&[][..], &["read"][..]] {
        let db = home.path().join("missing.sqlite");
        let output = ressac(home.path())
            .arg("--db")
            .arg(&db)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        assert_eq!(stdout(&output), "");
        assert_eq!(stderr(&output).trim(), "No feeds yet: run \"ressac import <file.opml>\" first.");
        assert!(!db.exists(), "nothing is created");
    }
}

#[test]
fn read_without_a_terminal_fails_cleanly() {
    // With feeds, the reader needs a terminal; under `cargo test` stdin is not one.
    let home = tempfile::tempdir().unwrap();
    let db = home.path().join("r.sqlite");
    ressac(home.path()).arg("--db").arg(&db).arg("import").arg(FIXTURE).output().unwrap();
    let output = ressac(home.path()).arg("--db").arg(&db).arg("read").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("is this a terminal?"), "{}", stderr(&output));
}

#[test]
fn help_lists_read() {
    let home = tempfile::tempdir().unwrap();
    let output = ressac(home.path()).arg("--help").output().unwrap();
    assert!(stdout(&output).contains("read"));
}
```

(Adapt the helper call shapes to `tests/cli.rs`'s real `ressac(...)`, `stdout`, `stderr`, `FIXTURE`. The second test's expected text is phase 5b's `ratatui::try_init` context; if phase 5b wrote another sentence, assert on it.)

- [ ] **Step 2: Run to see them fail.**

```bash
cargo test -p ressac
```

Expected: `cli.rs` compile errors on `Command::Read` and `Option`; the integration tests fail (bare `ressac` prints clap's usage and exits 2).

- [ ] **Step 3: Implement.**
  - `cli.rs`:

```rust
    /// What to do; without one, `ressac` opens the reader
    #[command(subcommand)]
    pub command: Option<Command>,
```

    and the first variant of `Command`:

```rust
    /// Read your feeds in the terminal (the default when no command is given)
    Read,
```

  - `commands/mod.rs`:

```rust
/// `ressac` and `ressac read`: the reader (spec §2, entry point A). On an empty or missing
/// database, the first-run hint, as every command that needs feeds.
pub fn read(g: &Globals) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let settings = crate::tui::reader::settings::TerminalSettings::default_path();
    crate::tui::reader::run(&app, &g.db, &settings)?; // synchronous, jobs on their own threads (as `tui::run`)
    Ok(ExitCode::SUCCESS)
}
```

    (Reconciled: phase 5b.3 already adds `Command::Read` and this `commands::read`; 8d keeps them and only adds the `None` route below. If 5b's version is already there, do not redeclare it.)
  - `main.rs`, in `run`:

```rust
    let Some(command) = cli.command else {
        return commands::read(&g);
    };
    match command {
        Command::Read => commands::read(&g),
        Command::Import { file, dry_run } => commands::import(&g, &file, dry_run, &mut out),
        // … every other arm unchanged …
    }
```

    (move the `let mut out = …` above `let Some(command)` if the borrow checker asks; `read` does not write to `out`.)
  - `ressac garden` keeps opening the garden alone (`Command::Garden { action: None }` → `tui::run`), unchanged.

- [ ] **Step 4: Run the gate.**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
```

Expected: all green, including the three new CLI tests and the updated parse tests.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/cli.rs crates/ressac/src/main.rs crates/ressac/src/commands/mod.rs crates/ressac/tests/cli.rs
git commit -m "CLI: ressac without a command, or ressac read, opens the reader

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 8

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `INSTA_UPDATE=no cargo test --workspace`: green; the eight garden snapshots of `crates/ressac/src/tui/snapshots/` unchanged (`git diff --stat main -- crates/ressac/src/tui/snapshots/` empty).
- Manual, throwaway database only:

```bash
T=$(mktemp -d); cp tests/fixtures/contract.sqlite "$T/r.sqlite"
HOME="$T" cargo run -q -p ressac -- --db "$T/r.sqlite"          # the reader, on the Journal
HOME="$T" cargo run -q -p ressac -- --db "$T/r.sqlite" garden   # the garden alone, as before
```

In the reader: choose Needs triage, `k` keeps a feed, `u` undoes, `c` then `↓` `↵` forces Daily (the line ends with `(set by you)`), `c` `↑` `↵` back to Auto, `Esc` to the sources, `q` quits.
