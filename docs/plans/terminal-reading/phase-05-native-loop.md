# Phase 5: The Native rendering and the event loop

**Runs after:** phase 4b (5a), then 5b.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §5 (the keys reach real data), §6 "Columns" (the three layout classes drawn, the top and bottom lines, `Window too small`), §7 "Text" (the article drawn from `render_html`), §4 "Living next to the app" (`data_version` polling, re-read after own writes), the rows of §9 for a busy database, extraction failures and panics, and `ressac read`. Review Focus 1 belongs to 5a, Review Focus 2 to 5a (state) and 5b (polling).

**Goal of the phase:** `ressac read` opens a working Native reader: sources, entry lists, the article with numbered links, read/star/mark-all, search, full text, refresh and the edition build as background jobs with a progress line, a reload when Ressac.app writes; images, the Journal's sections, the garden screen and Antiope come in phases 6 to 9.

**Builds on (exact names from the plan index):**
- Phase 1 (`ressac_core::store`): `Source`, `ListFilter`, `SourcesSnapshot` (`counts`, `tree`), `SourceCounts`, `SourceNode` (`kind`, `page`, `unread`, `children`), `SourceNodeKind::{Folder { id, name }, Feed { id, title }}`, `EntryRow`, `EntryPage` (`rows`, `total`, `unread`, `next`), `ListCursor`, `JournalItem`, `JournalView` (`edition`, `items`, `overflow`), `EntryDetail` (`row`, `summary_html`, `content_html`, `fulltext_html`, `fulltext_error`, `prefer_full_text`), and on `Store`: `sources(today_start)`, `entries(source, filter, after, today_start)`, `journal()`, `search_entries(source, text, after, today_start)`, `entry_detail(id)`, `set_read(id, read, now)`, `set_starred(id, starred, now)`, `mark_all_read(source, today_start, now)`, `data_version()`.
- Phase 3 (`crate::tui::reader::html`): `render_html(html, width, base_url) -> RenderedArticle` (`lines`, `links`, `images`), `MAX_TEXT_WIDTH = 72`, `ImageRef { url, alt, line }`. Assumption used below: at `ImageRef.line`, `render_html` already emitted the fallback line `[image: alt text]` (phase 6 replaces that line by the image).
- Phase 4 (`crate::tui::reader::{layout, settings, state}`): `Look`, `Column`, `LayoutClass`, `layout_class(width, height)`, `columns(area, class, focus, look) -> ColumnRects { sources, entries, reader, top, bottom }`, `TerminalSettings { look, images }` with `load`, `save`, `default_path`, `Command` (all variants of the plan index), `Event` (all variants), `JobOutcome`, `ReaderState::{new, on_key, on_event, look, focus, source, selected_entry}`.

**Existing code used as is:** `crate::output::printable` (`output.rs:211`, controls → U+FFFD, borrowed when clean), `crate::output::interrupted_message` is not needed; the garden's loop in `tui/mod.rs` (`ratatui::try_init()`, which installs ratatui's panic hook that restores the terminal, `ratatui::restore()` on every exit path, `KeyEventKind::Press` filter); `commands::open_with_feeds(db)` (first-run hint, exit 0); `Ressac::refresh(opts, sink, stop)`, `Ressac::extract_entry(id, sink) -> ExtractOutcome::{Saved, Failed(String), NoUrl, UnknownEntry}`, `Ressac::build_edition(opts, now, tz, sink, stop) -> EditionOutcome::{Built(_), NotDue, Offline, Interrupted}`, `Ressac::set_cadence(feed_id, group)`, `EventSink`, `Event::{Started { total }, FeedDone { .. }, Finished { summary }}`, `Summary.network_unreachable`, `RefreshOptions { force, only }`, `EditionOptions` (`Default`), `Store::feeds_in_folder(folder_id)`.

**Not in this phase (later phases wire these command arms):** `Command::FetchImage` (phase 6), `Command::Garden` (phase 8). Their executor arms exist here but stay empty until then (5b, Task 5b.2, with the comment that names the phase).

**Reconciled with the other phases (controller, 2026-10-16; these rules override the code below where they differ):**
- `ReaderState`'s fields are public (phase 4b), plus 4b's methods `start(now, local)`, `look`, `focus`, `source`, `selected_entry`, `class`, `go_to`, `select_entry`, `set_focus`. 5a.0 keeps `ReaderView`, `SourceLine`, `view()` and `set_reader_metrics` as the renderers' borrowed projection, built from those public fields: `source_lines` from `state.sidebar` (4b's `SidebarRow`; phase 8b appends the garden lines), `now_local` from `state.local` (set by `start` and every `Tick`; never `chrono::Local` in render code), `reader_scroll` = `state.scroll`.
- 4b already keeps the selection across a reload (`a_reload_keeps_the_selected_entry`): drop 5a.0's duplicate test `a_reload_after_data_changed_keeps_the_selected_entry` and the "reload keeps the selected entry" rule of its Step 4.
- `RenderCache` is dropped: the article is `state.article`, rendered by 4b's `render_article` (re-rendered on resize and on `f`). `render(frame, state) -> ReaderMetrics` in 5a; phase 6 adds a third parameter `images: &mut ImageStore`. `set_reader_metrics` is kept: 4b's `max_scroll`/`page_height` use the reported metrics when present (images change the article's height in rows).
- `JobOutcome::EditionBuiltElsewhere` no longer exists (phase 7 deviation 1).
- `run(app, db, settings_path)` is synchronous and every job runs in its own thread with its own current-thread runtime: `commands::read` calls it directly from `main`, exactly as `commands::garden` calls `tui::run` (no `block_in_place`). 5b.3 owns `Command::Read` and `commands::read`; phase 8d only makes `Cli.command` optional and routes `None` to `commands::read`.
- The `data_version` loop test opens the second connection with `ressac_core::service::Ressac::open(&db)` and writes through its `store()` (no `rusqlite` dev-dependency).

---

## Sub-phase 5a: the Native rendering

**Files (3, snapshots excluded):**
- Create: `crates/ressac/src/tui/reader/native.rs` (rendering, `RenderCache`, `progress_text`, tests and `insta` snapshots inline)
- Modify: `crates/ressac/src/tui/reader/state.rs` (`ReaderView`, `SourceLine`, `view()`, `set_reader_metrics`, the selection kept across a reload)
- Modify: `crates/ressac/src/tui/reader/mod.rs` (`pub mod native;`)

**Produces** (5b, 6, 7 and 9 rely on these):

```rust
// state.rs
pub struct SourceLine { pub label: String, pub unread: usize, pub depth: u16, pub selected: bool, pub heading: bool }
pub struct ReaderView<'a> {
    pub look: Look, pub layout: LayoutClass, pub focus: Column, pub sources_overlay: bool,
    pub now_local: DateTime<FixedOffset>,
    pub source: Source, pub source_title: String, pub source_lines: Vec<SourceLine>,
    pub filter: ListFilter, pub search: Option<&'a str>,
    pub entries: &'a [EntryRow], pub entry_cursor: Option<usize>, pub total: usize, pub unread: usize,
    pub journal: Option<&'a JournalView>,
    pub entry: Option<&'a EntryDetail>, pub show_fulltext: bool, pub reader_scroll: usize,
    pub images: bool, pub progress: Option<(&'a str, usize, usize)>, pub message: Option<&'a str>, pub help: bool,
}
impl ReaderState {
    pub fn view(&self) -> ReaderView<'_>;
    /// The renderer reports the article's height in rows and the reader's visible rows; Space uses them.
    pub fn set_reader_metrics(&mut self, total_rows: usize, viewport_rows: u16);
}
// native.rs
pub const EDITION_PROGRESS_LABEL: &str = "Building this morning's edition…";
pub const FETCH_PROGRESS_LABEL: &str = "Fetching feeds";
pub const EXTRACT_PROGRESS_LABEL: &str = "Fetching the full article";
pub const TOO_SMALL: &str = "Window too small";
pub struct ReaderMetrics { pub total_rows: usize, pub viewport_rows: u16 }
#[derive(Default)] pub struct RenderCache { /* last rendered article, keyed by (entry_id, width, fulltext) */ }
pub fn render(frame: &mut Frame, state: &ReaderState, cache: &mut RenderCache) -> ReaderMetrics;
pub fn progress_text(label: &str, done: usize, total: usize, width: u16) -> String;
```

The plan index lists `native::render(frame, state, images)`; this phase's signature is `render(frame, state, cache)`, and phase 6 adds `images: &mut ImageStore` as a fourth parameter (the stateful image widget needs `&mut`). The returned `ReaderMetrics` keeps rendering free of state mutation: the loop (5b) passes it to `set_reader_metrics`.

### Task 5a.0: the read-only view of the state

Phase 4b owns `ReaderState`'s private fields; their names are in `docs/plans/terminal-reading/phase-04-state-layout.md`. The renderer must not reach into them, so this task adds one borrowed view and one setter.

- [ ] **Step 1: Check what 4b already exposes.** Open `crates/ressac/src/tui/reader/state.rs`. If an equivalent view already exists (same information under other names), keep it, map the names below to it in `native.rs`, and record the mapping in your report. Otherwise go on.

- [ ] **Step 2: Write the failing tests** at the end of the `#[cfg(test)] mod tests` of `state.rs`. They use only the plan index's public API, plus `view()` and `set_reader_metrics`. Review Focus 2 (state side) is the second test.

```rust
    use chrono::{FixedOffset, TimeZone, Utc};
    use ressac_core::model::CadenceGroup;
    use ressac_core::store::{
        EntryPage, EntryRow, ListFilter, Source, SourceCounts, SourceNode, SourceNodeKind,
        SourcesSnapshot,
    };
    use std::collections::BTreeMap;

    fn row(id: i64, title: &str, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: 3,
            feed_title: "Courrier international".into(),
            title: Some(title.into()),
            author: None,
            url: Some(format!("https://example.com/{id}")),
            published_at: Utc.with_ymd_and_hms(2026, 10, 16, 6, id as u32 % 60, 0).unwrap(),
            is_read: read,
            is_starred: false,
            has_fulltext: false,
        }
    }

    fn snapshot() -> SourcesSnapshot {
        let mut cadence = BTreeMap::new();
        for g in CadenceGroup::ALL {
            cadence.insert(g, 0);
        }
        SourcesSnapshot {
            counts: SourceCounts { journal: 0, unread: 3, today: 3, starred: 0, needs_triage: 1, cadence },
            tree: vec![SourceNode {
                kind: SourceNodeKind::Folder { id: 1, name: "Infos".into() },
                page: Some(400),
                unread: 3,
                children: vec![SourceNode {
                    kind: SourceNodeKind::Feed { id: 3, title: "Courrier international".into() },
                    page: Some(401),
                    unread: 3,
                    children: vec![],
                }],
            }],
        }
    }

    fn page(rows: Vec<EntryRow>) -> EntryPage {
        let unread = rows.iter().filter(|r| !r.is_read).count();
        EntryPage { total: rows.len(), unread, rows, next: None }
    }

    fn loaded(source: Source, rows: Vec<EntryRow>) -> ReaderState {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40);
        s.on_event(Event::Sources(snapshot()));
        s.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 7, 0, 0).unwrap(),
            local: FixedOffset::east_opt(7200).unwrap().with_ymd_and_hms(2026, 10, 16, 9, 0, 0).unwrap(),
        });
        s.on_event(Event::Entries { source, page: page(rows), appended: false });
        s
    }

    #[test]
    fn the_view_shows_what_the_state_holds() {
        let s = loaded(Source::Journal, vec![row(11, "First", false), row(12, "Second", false)]);
        let v = s.view();
        assert_eq!(v.look, Look::Native);
        assert_eq!(v.source, Source::Journal);
        assert_eq!(v.source_title, "Journal");
        assert_eq!(v.entries.len(), 2);
        assert_eq!(v.filter, ListFilter::All);
        assert!(v.source_lines.iter().any(|l| l.label == "Courrier international" && l.depth == 1));
        assert!(v.source_lines.iter().any(|l| l.label == "Needs triage" && l.unread == 1));
        assert_eq!(v.now_local.offset().local_minus_utc(), 7200);
    }

    #[test]
    fn a_reload_after_data_changed_keeps_the_selected_entry() {
        let mut s = loaded(Source::Unread, vec![row(11, "A", false), row(12, "B", false), row(13, "C", false)]);
        s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        let selected = s.selected_entry().expect("an entry is selected");
        let commands = s.on_event(Event::DataChanged);
        assert!(commands.iter().any(|c| matches!(c, Command::LoadSources)));
        assert!(commands.iter().any(|c| matches!(c, Command::LoadEntries { source: Source::Unread, .. })));
        // Ressac.app inserted a newer entry: the list shifts, the selection must not.
        s.on_event(Event::Entries {
            source: Source::Unread,
            page: page(vec![row(10, "New", false), row(11, "A", false), row(12, "B", false), row(13, "C", false)]),
            appended: false,
        });
        assert_eq!(s.selected_entry(), Some(selected));
    }

    #[test]
    fn reader_metrics_drive_space_scrolling() {
        let mut s = loaded(Source::Unread, vec![row(11, "A", false), row(12, "B", false)]);
        s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        s.set_reader_metrics(120, 30);
        let before = s.view().reader_scroll;
        s.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(s.view().reader_scroll > before, "Space scrolls a long article before moving on");
    }
```

- [ ] **Step 3: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::state::tests::the_view_shows_what_the_state_holds`
Expected: FAIL to compile, `no method named view found for struct ReaderState` (or, if 4b already has an equivalent, adapt the names per Step 1 and expect the selection test to tell whether 4b keeps the selection).

- [ ] **Step 4: Implement.** Add the two structs (verbatim from **Produces**) and the two methods. `view()` only borrows: each field comes from the matching private field of `ReaderState` (4b's names). Rules that are part of the contract:
  - `source_title`: `"Journal"`, `"Unread"`, `"Today"`, `"Starred"`, the folder's name or the feed's title from the snapshot, the group's title (`"Daily"` … `"Silent"`), passed through `crate::output::printable`.
  - `source_lines`: smart lists first (Journal, Unread, Today, Starred, each `depth` 0, `heading` false), then a heading line `Feeds` (`heading` true), the tree depth-first in snapshot order (`depth` = nesting, folders before their children), a heading `Rhythm` and the five groups (`Daily` … `Silent`, unread from `counts.cadence`), a heading `Garden` and `Needs triage` (unread = `counts.needs_triage`), `All feeds`, `Cemetery`. Every label through `printable`. `selected` marks the current source.
  - `progress`: the label and counts of the last `Event::Progress` while a job runs, `None` after its `JobDone`.
  - `set_reader_metrics` stores both numbers; Space scrolls by `viewport_rows - 2` while `reader_scroll + viewport_rows < total_rows`, else it moves to the next unread entry of the source (4b's rule).
  - A reload (`Event::Entries` with `appended: false` for the current source) keeps the selected entry id when it is still in the rows, else selects the row at the same index clamped to the list, else none.

- [ ] **Step 5: Run the state tests.**

Run: `cargo test -p ressac --bin ressac tui::reader::state`
Expected: PASS (every 4b test plus the three above).

- [ ] **Step 6: Commit.**

```bash
git add crates/ressac/src/tui/reader/state.rs
git commit -m "Reader: a borrowed view of the state for the renderers, reader metrics, the selection kept across reloads

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5a.1: `native.rs`, drawn at 140, 100 and 70 columns

- [ ] **Step 1: Write the failing tests** in a new `crates/ressac/src/tui/reader/native.rs`, then add `pub mod native;` to `tui/reader/mod.rs`. The module starts with only the tests and `use` lines so that the run fails on the missing items.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::layout::Look;
    use crate::tui::reader::settings::TerminalSettings;
    use crate::tui::reader::state::{Event, ReaderState};
    use chrono::{FixedOffset, TimeZone, Utc};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ressac_core::model::CadenceGroup;
    use ressac_core::store::{
        EntryDetail, EntryPage, EntryRow, Source, SourceCounts, SourceNode, SourceNodeKind,
        SourcesSnapshot,
    };
    use std::collections::BTreeMap;

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
            counts: SourceCounts { journal: 2, unread: 3, today: 3, starred: 1, needs_triage: 1, cadence },
            tree: vec![SourceNode {
                kind: SourceNodeKind::Folder { id: 1, name: "Infos".into() },
                page: Some(400),
                unread: 3,
                children: vec![SourceNode {
                    kind: SourceNodeKind::Feed { id: 3, title: "Courrier international".into() },
                    page: Some(401),
                    unread: 3,
                    children: vec![],
                }],
            }],
        }
    }

    fn state(width: u16, height: u16, rows: Vec<EntryRow>) -> ReaderState {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, width, height);
        s.on_event(Event::Sources(snapshot()));
        s.on_event(Event::Tick {
            now: Utc.with_ymd_and_hms(2026, 10, 16, 7, 0, 0).unwrap(),
            local: FixedOffset::east_opt(7200).unwrap().with_ymd_and_hms(2026, 10, 16, 9, 0, 0).unwrap(),
        });
        let unread = rows.iter().filter(|r| !r.is_read).count();
        s.on_event(Event::Entries {
            source: Source::Unread,
            page: EntryPage { total: rows.len(), unread, rows: rows.clone(), next: None },
            appended: false,
        });
        s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        let first = rows.first().expect("a row").clone();
        s.on_event(Event::Entry(EntryDetail {
            row: first,
            summary_html: None,
            content_html: Some(ARTICLE.into()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        }));
        s
    }

    fn draw(s: &ReaderState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut cache = RenderCache::default();
        terminal.draw(|f| { render(f, s, &mut cache); }).unwrap();
        terminal.backend().to_string()
    }

    fn rows() -> Vec<EntryRow> {
        vec![row(11, "Le jardin des flux", false), row(12, "Un deuxième article", false), row(13, "Lu hier", true)]
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
    fn escape_sequences_and_wide_titles_stay_inside_their_column() {
        // Review Focus 1: an OSC title-setting sequence, BEL, CSI (U+009B), an emoji and CJK.
        let hostile = "Café ☕ 東京 \u{1b}]0;pwned\u{7} \u{9b}31m fin";
        let s = state(100, 20, vec![row(11, hostile, false), row(12, "Autre", false)]);
        let out = draw(&s, 100, 20);
        assert!(!out.chars().any(|c| c == '\u{1b}' || c == '\u{7}' || c == '\u{9b}'), "{out:?}");
        assert!(out.contains('\u{FFFD}'), "controls are shown as U+FFFD");
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let mut cache = RenderCache::default();
        terminal.draw(|f| { render(f, &s, &mut cache); }).unwrap();
        // Every row is exactly the terminal width: nothing spilled past a column edge.
        for line in terminal.backend().to_string().lines() {
            let width: usize = line.chars().map(|c| if ('\u{2E80}'..='\u{9FFF}').contains(&c) || c == '☕' { 2 } else { 1 }).sum();
            assert!(width <= 100 + 2, "row wider than the pane: {line:?}");
        }
    }

    #[test]
    fn progress_text_shortens_the_edition_label_when_needed() {
        assert_eq!(
            progress_text(EDITION_PROGRESS_LABEL, 34, 156, 60),
            "Building this morning's edition… 34/156"
        );
        assert_eq!(progress_text(EDITION_PROGRESS_LABEL, 34, 156, 30), "Edition… 34/156");
        assert_eq!(progress_text(FETCH_PROGRESS_LABEL, 3, 9, 40), "Fetching feeds 3/9");
        assert_eq!(progress_text(FETCH_PROGRESS_LABEL, 3, 9, 10), "Fetching…");
    }

    #[test]
    fn the_article_is_wrapped_to_72_columns_at_most() {
        let s = state(140, 30, rows());
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        let mut cache = RenderCache::default();
        let mut metrics = None;
        terminal.draw(|f| { metrics = Some(render(f, &s, &mut cache)); }).unwrap();
        let metrics = metrics.unwrap();
        assert!(metrics.total_rows > 0);
        assert!(metrics.viewport_rows > 0 && metrics.viewport_rows < 30);
        assert!(terminal.backend().to_string().contains("premier lien[1]"));
    }
}
```

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::native`
Expected: FAIL to compile, `cannot find function render` / `RenderCache` / `progress_text` / `TOO_SMALL`.

- [ ] **Step 3: Implement** above the tests.

```rust
//! The Native look of the reader: sources, entries and the article, folded to the pane's width.

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ressac_core::store::{EntryDetail, EntryRow, ListFilter};

use super::html::{MAX_TEXT_WIDTH, RenderedArticle, render_html};
use super::layout::{Column, LayoutClass, columns};
use super::state::{ReaderState, ReaderView, SourceLine};
use crate::output::printable;

pub const EDITION_PROGRESS_LABEL: &str = "Building this morning's edition…";
pub const FETCH_PROGRESS_LABEL: &str = "Fetching feeds";
pub const EXTRACT_PROGRESS_LABEL: &str = "Fetching the full article";
pub const TOO_SMALL: &str = "Window too small";

/// What the loop gives back to the state after a draw (Space needs the article's height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReaderMetrics {
    pub total_rows: usize,
    pub viewport_rows: u16,
}

/// The last article converted by `render_html`, kept until the entry, the width or the
/// full-text choice changes (a draw happens on every key and tick).
#[derive(Default)]
pub struct RenderCache {
    key: Option<(i64, u16, bool)>,
    article: Option<RenderedArticle>,
}

impl RenderCache {
    fn article(&mut self, entry: &EntryDetail, fulltext: bool, width: u16) -> &RenderedArticle {
        let key = (entry.row.entry_id, width, fulltext);
        if self.key != Some(key) || self.article.is_none() {
            let html = article_html(entry, fulltext);
            self.article = Some(render_html(html, width, entry.row.url.as_deref()));
            self.key = Some(key);
        }
        self.article.as_ref().expect("set just above")
    }
}

/// The HTML the reader shows: the full text when asked for (or preferred) and stored, else the
/// feed's content, else its summary.
fn article_html(entry: &EntryDetail, fulltext: bool) -> &str {
    let full = entry.fulltext_html.as_deref().filter(|_| fulltext || entry.prefer_full_text);
    full.or(entry.content_html.as_deref())
        .or(entry.summary_html.as_deref())
        .unwrap_or("")
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

fn frame_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(if focused { FOCUS } else { DIM })
        .title(Span::raw(format!(" {title} ")))
}

pub fn render(frame: &mut Frame, state: &ReaderState, cache: &mut RenderCache) -> ReaderMetrics {
    let view = state.view();
    let area = frame.area();
    if view.layout == LayoutClass::TooSmall {
        let y = area.height / 2;
        let line = Rect { x: area.x, y: area.y + y, width: area.width, height: 1 };
        frame.render_widget(Paragraph::new(TOO_SMALL).alignment(Alignment::Center), line);
        return ReaderMetrics::default();
    }
    let rects = columns(area, view.layout, view.focus, view.look);
    frame.render_widget(top_line(&view, rects.top.width), rects.top);
    frame.render_widget(bottom_line(&view), rects.bottom);
    if let Some(r) = rects.sources {
        render_sources(frame, &view, r, view.focus == Column::Sources);
    }
    if let Some(r) = rects.entries {
        render_entries(frame, &view, r, view.focus == Column::Entries);
        if view.sources_overlay {
            let overlay = Rect { width: r.width.min(30), ..r };
            frame.render_widget(Clear, overlay);
            render_sources(frame, &view, overlay, true);
        }
    }
    match rects.reader {
        Some(r) => render_reader(frame, &view, cache, r, view.focus == Column::Reader),
        None => ReaderMetrics::default(),
    }
}

fn top_line(view: &ReaderView<'_>, width: u16) -> Paragraph<'static> {
    let filter = match view.filter {
        ListFilter::All => "all",
        ListFilter::Unread => "unread only",
    };
    let mut left = format!("{} · {} unread · {filter}", view.source_title, view.unread);
    if let Some(text) = view.search {
        left.push_str(&format!(" · /{}", printable(text)));
    }
    let right = match (view.progress, view.message) {
        (Some((label, done, total)), _) => progress_text(label, done, total, width / 2),
        (None, Some(message)) => printable(message).into_owned(),
        (None, None) => String::new(),
    };
    let pad = usize::from(width).saturating_sub(left.chars().count() + right.chars().count());
    Paragraph::new(Line::from(vec![
        Span::styled(left, Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, Style::new().fg(Color::Yellow)),
    ]))
}

fn bottom_line(view: &ReaderView<'_>) -> Paragraph<'static> {
    let keys = match view.focus {
        Column::Sources => "↑↓ source  → list  r refresh  t Antiope  ? help  q quit",
        Column::Entries => "j/k entry  Space next  m read  s star  A all read  u unread  / search  ? help",
        Column::Reader => "Space scroll  j/k entry  f full text  o open  o1-9 link  i images  ? help",
    };
    Paragraph::new(Span::styled(keys, DIM))
}

fn render_sources(frame: &mut Frame, view: &ReaderView<'_>, area: Rect, focused: bool) {
    let items: Vec<ListItem> = view.source_lines.iter().map(source_item).collect();
    let selected = view.source_lines.iter().position(|l| l.selected);
    let mut list_state = ListState::default().with_selected(selected);
    let list = List::new(items)
        .block(frame_block("Sources", focused))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn source_item(line: &SourceLine) -> ListItem<'static> {
    if line.heading {
        return ListItem::new(Line::styled(line.label.clone(), DIM.add_modifier(Modifier::BOLD)));
    }
    let indent = "  ".repeat(usize::from(line.depth));
    let count = if line.unread > 0 { format!(" {}", line.unread) } else { String::new() };
    ListItem::new(Line::from(vec![Span::raw(format!("{indent}{}", line.label)), Span::styled(count, DIM)]))
}

fn render_entries(frame: &mut Frame, view: &ReaderView<'_>, area: Rect, focused: bool) {
    let items: Vec<ListItem> = view.entries.iter().map(|r| entry_item(r, view.now_local)).collect();
    let mut list_state = ListState::default().with_selected(view.entry_cursor);
    let list = List::new(items)
        .block(frame_block(&view.source_title, focused))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn entry_item(row: &EntryRow, now: DateTime<FixedOffset>) -> ListItem<'static> {
    let mark = if row.is_read { " " } else { "●" };
    let star = if row.is_starred { "★" } else { " " };
    let title = printable(row.title.as_deref().unwrap_or("(untitled)")).into_owned();
    let style = if row.is_read { DIM } else { Style::new().add_modifier(Modifier::BOLD) };
    ListItem::new(vec![
        Line::from(vec![Span::raw(format!("{mark}{star} ")), Span::styled(title, style)]),
        Line::styled(
            format!("   {} · {}", printable(&row.feed_title), short_date(row.published_at, now)),
            DIM,
        ),
    ])
}

/// `09:30` today, `Thu 15 Oct` this year, `15 Oct 2025` before; in the pane's local offset.
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

fn render_reader(
    frame: &mut Frame,
    view: &ReaderView<'_>,
    cache: &mut RenderCache,
    area: Rect,
    focused: bool,
) -> ReaderMetrics {
    let block = frame_block("Article", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(entry) = view.entry else {
        frame.render_widget(Paragraph::new(Span::styled("No entry selected", DIM)), inner);
        return ReaderMetrics { total_rows: 0, viewport_rows: inner.height };
    };
    let width = inner.width.min(MAX_TEXT_WIDTH);
    let mut lines = header_lines(entry, view);
    let article = cache.article(entry, view.show_fulltext, width);
    lines.extend(article.lines.iter().cloned());
    if !article.links.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Links", Style::new().add_modifier(Modifier::BOLD)));
        for (i, link) in article.links.iter().enumerate() {
            lines.push(Line::styled(format!("[{}] {}", i + 1, printable(link)), DIM));
        }
    }
    let total_rows = lines.len();
    let text_area = Rect { width, ..inner };
    let scroll = u16::try_from(view.reader_scroll).unwrap_or(u16::MAX);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), text_area);
    ReaderMetrics { total_rows, viewport_rows: inner.height }
}

fn header_lines(entry: &EntryDetail, view: &ReaderView<'_>) -> Vec<Line<'static>> {
    let row = &entry.row;
    let title = printable(row.title.as_deref().unwrap_or("(untitled)")).into_owned();
    let mut by = printable(&row.feed_title).into_owned();
    if let Some(author) = row.author.as_deref() {
        by.push_str(&format!(" · {}", printable(author)));
    }
    by.push_str(&format!(" · {}", short_date(row.published_at, view.now_local)));
    let mut state = vec![if row.is_read { "read" } else { "unread" }];
    if row.is_starred {
        state.push("starred");
    }
    if view.show_fulltext || entry.prefer_full_text {
        state.push("full text");
    }
    let mut lines = vec![
        Line::styled(title, Style::new().add_modifier(Modifier::BOLD)),
        Line::styled(by, DIM),
        Line::styled(state.join(" · "), DIM),
    ];
    if let Some(error) = entry.fulltext_error.as_deref() {
        lines.push(Line::styled(printable(error).into_owned(), Style::new().fg(Color::Red)));
    }
    lines.push(Line::raw(""));
    lines
}
```

Notes for the implementer: `ListState::with_selected` exists in ratatui 0.30; if `List` does not keep the selected row in view at the snapshot heights, call `list_state.select(...)` and rely on ratatui's own offset (do not compute one). The width check of the Review Focus test approximates double-width characters for the two ranges used in the title; ratatui's buffer is the ground truth, so if the assertion is too coarse replace it with a cell-by-cell check through `terminal.backend().buffer()` (`Buffer::content`, each cell's `symbol()`), never relax it.

- [ ] **Step 4: Run the tests, review and accept the three snapshots.**

Run: `cargo test -p ressac --bin ressac tui::reader::native` then `cargo insta review` (or read the `.snap.new` files under `crates/ressac/src/tui/reader/snapshots/`), accept them when: three columns at 140 (Sources, Unread, Article with `premier lien[1]`, the Links list), two at 100 (entries and article, no sources), one at 70 (the article alone after `Enter`), the top line `Unread · 3 unread · all`.
Expected after acceptance: PASS, 7 tests.

- [ ] **Step 5: Gate and commit.**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/src/tui/reader/native.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader: the Native look at three, two and one column, controls neutralised, progress shortened

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Sub-phase 5b: the event loop, background jobs and `ressac read`

**Files (5):**
- Modify: `crates/ressac/src/tui/reader/mod.rs` (the event loop, the executor, `today_start`, polling)
- Create: `crates/ressac/src/tui/reader/jobs.rs` (background jobs on their own connection, progress sink)
- Modify: `crates/ressac/src/cli.rs` (`Command::Read`)
- Modify: `crates/ressac/src/main.rs` (dispatch)
- Modify: `crates/ressac/src/commands/mod.rs` (`read` handler; `open` becomes `pub(crate)` for the jobs)

Test file (does not count): `crates/ressac/tests/cli.rs`.

**Step 0 check:** `main.rs` (103 lines) and `commands/mod.rs` are under 300 lines; `cli.rs` is under 300. No Step 0 commit.

**Design** (spec §4 "Living next to the app", §6 "Building the edition"):
- The loop owns one `Ressac` (the one `commands::read` opened) for reads and the quick writes (`set_read`, `set_starred`, `mark_all_read`, `set_cadence`). Its connection's `PRAGMA data_version` changes only when **another** connection commits: Ressac.app, or a job.
- Long work (refresh, edition build, extraction, later images) runs in a `std::thread` with its own `Ressac::open(db)` and a current-thread tokio runtime; it talks back through a `std::sync::mpsc::Sender<Msg>`. `Ressac` is `Send` (it owns a `rusqlite::Connection`), never shared.
- One long job at a time (refresh or edition): the next one waits in a queue; extractions run alongside.
- After each of its own writes the loop feeds `Event::DataChanged` to the state (its own connection does not move `data_version`). Every 2 s without a key, it compares `data_version()` with the last value and feeds `DataChanged` when it moved.
- `Event::Tick` every second (Antiope's clock, the 04:00 rollover in phase 7).
- `q` (`Command::Quit`) raises the jobs' stop flag and leaves; a job interrupted this way writes nothing more (core behaviour).

### Task 5b.1: jobs on their own connection

- [ ] **Step 1: Write the failing tests** in a new `crates/ressac/src/tui/reader/jobs.rs` (add `pub mod jobs;` to `tui/reader/mod.rs`).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::state::{Event, JobOutcome};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn the_progress_sink_turns_core_events_into_progress() {
        let (tx, rx) = mpsc::channel();
        let sink = ProgressSink::new("Fetching feeds", tx);
        sink.emit(&CoreEvent::Started { total: 3 });
        sink.emit(&CoreEvent::FeedDone { feed_id: 1, title: "a".into(), error: None });
        sink.emit(&CoreEvent::FeedDone { feed_id: 2, title: "b".into(), error: Some("HTTP 404.".into()) });
        let got: Vec<(usize, usize)> = rx
            .try_iter()
            .filter_map(|m| match m {
                Msg::Event(Event::Progress { label, done, total }) if label == "Fetching feeds" => Some((done, total)),
                _ => None,
            })
            .collect();
        assert_eq!(got, vec![(0, 3), (1, 3), (2, 3)]);
    }

    #[test]
    fn a_job_runs_on_its_own_connection_and_reports_its_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        ressac_core::service::Ressac::open(&db).unwrap();
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        spawn(db.clone(), tx, stop, |app, _tx, _stop| async move {
            assert_eq!(app.store().feed_count().unwrap(), 0);
            JobOutcome::Refreshed
        });
        let msg = rx.recv_timeout(Duration::from_secs(5)).expect("the job reports");
        assert!(matches!(msg, Msg::Event(Event::JobDone(JobOutcome::Refreshed))));
    }

    #[test]
    fn a_job_that_cannot_open_the_database_reports_a_failure() {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        spawn("/nonexistent/dir/r.sqlite".into(), tx, stop, |_app, _tx, _stop| async move {
            JobOutcome::Refreshed
        });
        let msg = rx.recv_timeout(Duration::from_secs(5)).expect("the job reports");
        assert!(matches!(msg, Msg::Event(Event::JobDone(JobOutcome::Failed(_)))));
    }

    #[test]
    fn edition_outcomes_map_to_job_outcomes() {
        assert_eq!(edition_outcome(Ok(EditionOutcome::NotDue)), JobOutcome::EditionNotDue);
        assert_eq!(edition_outcome(Ok(EditionOutcome::Offline)), JobOutcome::Offline);
        assert_eq!(
            edition_outcome(Ok(EditionOutcome::Interrupted)),
            JobOutcome::Failed("Interrupted.".into())
        );
        assert!(matches!(
            edition_outcome(Err(ressac_core::Error::Invalid("boom".into()))),
            JobOutcome::Failed(m) if m.contains("boom")
        ));
    }

    #[test]
    fn extract_outcomes_map_to_job_outcomes() {
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::Saved)),
            JobOutcome::Extracted { entry_id: 7, error: None }
        );
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::Failed("HTTP 404.".into()))),
            JobOutcome::Extracted { entry_id: 7, error: Some("HTTP 404.".into()) }
        );
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::NoUrl)),
            JobOutcome::Extracted { entry_id: 7, error: Some("This entry has no link.".into()) }
        );
    }
}
```

`JobOutcome` must derive `PartialEq` and `Debug` for these assertions (phase 4b declares it; add the derives in this task if 4b did not, it is one line in `state.rs` and does not change the file count of 5b: `state.rs` is then the sixth file, so prefer asking 4b's implementer, or put the derive change in Task 5a.0's commit, which already touches `state.rs`).

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::jobs`
Expected: FAIL to compile, `cannot find type ProgressSink` / `Msg` / function `spawn`.

- [ ] **Step 3: Implement** above the tests.

```rust
//! Background work for the reader: each job opens its own connection on the same database,
//! runs on a current-thread runtime in its own thread, and reports through a channel.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

use ressac_core::service::{EditionOutcome, Event as CoreEvent, EventSink, ExtractOutcome, Ressac};

use super::state::{Event, JobOutcome};

/// What a job sends back to the loop. Images (phase 6) add their own variant.
pub enum Msg {
    Event(Event),
}

/// Turns the core's run events into `Event::Progress` for the top line.
pub struct ProgressSink {
    label: String,
    done: AtomicUsize,
    total: AtomicUsize,
    tx: Sender<Msg>,
}

impl ProgressSink {
    pub fn new(label: &str, tx: Sender<Msg>) -> Self {
        Self { label: label.to_owned(), done: AtomicUsize::new(0), total: AtomicUsize::new(0), tx }
    }

    fn send(&self) {
        let _ = self.tx.send(Msg::Event(Event::Progress {
            label: self.label.clone(),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }));
    }
}

impl EventSink for ProgressSink {
    fn emit(&self, event: &CoreEvent) {
        match event {
            CoreEvent::Started { total } => {
                self.total.store(*total, Ordering::Relaxed);
                self.send();
            }
            CoreEvent::FeedDone { .. } => {
                self.done.fetch_add(1, Ordering::Relaxed);
                self.send();
            }
            _ => {}
        }
    }
}

/// Runs `work` in its own thread with its own `Ressac` on `db`. The outcome becomes
/// `Event::JobDone`; a database that cannot be opened is a `Failed` outcome.
pub fn spawn<F, Fut>(db: PathBuf, tx: Sender<Msg>, stop: Arc<AtomicBool>, work: F)
where
    F: FnOnce(Ressac, Sender<Msg>, Arc<AtomicBool>) -> Fut + Send + 'static,
    Fut: Future<Output = JobOutcome>,
{
    std::thread::spawn(move || {
        let outcome = match (crate::commands::open(&db), tokio_runtime()) {
            (Ok(app), Ok(runtime)) => runtime.block_on(work(app, tx.clone(), stop)),
            (Err(e), _) => JobOutcome::Failed(format!("{e:#}")),
            (_, Err(e)) => JobOutcome::Failed(e.to_string()),
        };
        let _ = tx.send(Msg::Event(Event::JobDone(outcome)));
    });
}

fn tokio_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()
}

pub fn edition_outcome(result: ressac_core::Result<EditionOutcome>) -> JobOutcome {
    match result {
        Ok(EditionOutcome::Built(_)) => JobOutcome::EditionBuilt,
        Ok(EditionOutcome::NotDue) => JobOutcome::EditionNotDue,
        Ok(EditionOutcome::Offline) => JobOutcome::Offline,
        Ok(EditionOutcome::Interrupted) => JobOutcome::Failed("Interrupted.".into()),
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub fn extract_outcome(entry_id: i64, result: ressac_core::Result<ExtractOutcome>) -> JobOutcome {
    let error = match result {
        Ok(ExtractOutcome::Saved) => None,
        Ok(ExtractOutcome::Failed(sentence)) => Some(sentence),
        Ok(ExtractOutcome::NoUrl) => Some("This entry has no link.".into()),
        Ok(ExtractOutcome::UnknownEntry) => Some(format!("No entry #{entry_id}.")),
        Err(e) => Some(e.to_string()),
    };
    JobOutcome::Extracted { entry_id, error }
}
```

`crate::commands::open` is private today; Task 5b.3 makes it `pub(crate)` (it applies the debug-only private-address hook the CLI tests rely on). Do Task 5b.3's one-word visibility change in this commit if the build needs it first.

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ressac --bin ressac tui::reader::jobs`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/jobs.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/commands/mod.rs
git commit -m "Reader: background jobs on their own connection, progress from the core's events

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5b.2: the executor and the loop

- [ ] **Step 1: Write the failing tests** in `tui/reader/mod.rs` (`#[cfg(test)] mod tests`). The loop itself needs a terminal; the executor, `today_start` and the polling rule do not, and they carry the behaviour.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Utc};
    use ressac_core::store::{ListFilter, Source};
    use std::time::{Duration, Instant};

    fn paris_summer() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    #[test]
    fn today_starts_at_local_midnight() {
        let now = paris_summer().with_ymd_and_hms(2026, 10, 16, 0, 30, 0).unwrap();
        assert_eq!(today_start(now), Utc.with_ymd_and_hms(2026, 10, 15, 22, 0, 0).unwrap());
        let evening = paris_summer().with_ymd_and_hms(2026, 10, 16, 23, 59, 0).unwrap();
        assert_eq!(today_start(evening), Utc.with_ymd_and_hms(2026, 10, 15, 22, 0, 0).unwrap());
    }

    #[test]
    fn polling_waits_two_idle_seconds() {
        let start = Instant::now();
        let mut poll = Poll::new(start, 5);
        assert_eq!(poll.check(start + Duration::from_millis(1900), 6), None);
        assert_eq!(poll.check(start + Duration::from_secs(2), 6), Some(Event::DataChanged));
        assert_eq!(poll.check(start + Duration::from_secs(4), 6), None, "unchanged version");
        poll.keyed(start + Duration::from_secs(5));
        assert_eq!(poll.check(start + Duration::from_secs(6), 7), None, "a key resets the idle timer");
        assert_eq!(poll.check(start + Duration::from_secs(7), 7), Some(Event::DataChanged));
    }

    #[test]
    fn data_version_moves_when_another_connection_writes() {
        // Review Focus 2, loop side: what Ressac.app's writes look like to the reader.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        let reader = ressac_core::service::Ressac::open(&db).unwrap();
        let before = reader.store().data_version().unwrap();
        let other = rusqlite::Connection::open(&db).unwrap();
        other.execute_batch("INSERT INTO folder (name, position) VALUES ('X', 0)").unwrap();
        assert_ne!(reader.store().data_version().unwrap(), before);
    }

    #[test]
    fn own_writes_are_followed_by_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        let app = ressac_core::service::Ressac::open(&db).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(Command::MarkAllRead(Source::Unread), test_clock());
        assert!(events.contains(&Event::DataChanged), "{events:?}");
    }

    #[test]
    fn long_jobs_queue_behind_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        let app = ressac_core::service::Ressac::open(&db).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        exec.run(Command::Refresh(None), test_clock());
        exec.run(Command::BuildEdition, test_clock());
        assert_eq!(exec.queued(), 1, "the edition waits for the refresh");
        assert!(exec.long_job_running());
    }

    #[test]
    fn loading_a_list_answers_with_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        let app = ressac_core::service::Ressac::open(&db).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(
            Command::LoadEntries { source: Source::Unread, filter: ListFilter::All, after: None },
            test_clock(),
        );
        assert!(matches!(events.as_slice(), [Event::Entries { source: Source::Unread, appended: false, .. }]));
    }

    fn test_clock() -> Clock {
        let local = paris_summer().with_ymd_and_hms(2026, 10, 16, 9, 0, 0).unwrap();
        Clock { now: local.with_timezone(&Utc), local }
    }
}
```

`Event` needs `PartialEq` and `Debug` for `contains` and `assert_eq!`; if 4b did not derive them, add the derives in Task 5a.0's commit (see the note in Task 5b.1). The `folder` insert uses the schema's real columns (`name`, `position`; check `schema/0001_init.sql` and adjust the column list if it has other `NOT NULL` columns without default).

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --bin ressac tui::reader::tests`
Expected: FAIL to compile, `cannot find function today_start` / `Poll` / `Executor` / `Clock`.

- [ ] **Step 3: Implement** in `tui/reader/mod.rs` (keep the `pub mod` lines of phases 3–5a at the top).

```rust
//! `ressac read`: the reader's event loop. The state decides, this module executes.

pub mod html;
pub mod jobs;
pub mod layout;
pub mod native;
pub mod settings;
pub mod state;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyEventKind};
use ressac_core::service::{EditionOptions, RefreshOptions, Ressac};
use ressac_core::store::Source;

use jobs::{Msg, ProgressSink};
use native::{EDITION_PROGRESS_LABEL, EXTRACT_PROGRESS_LABEL, FETCH_PROGRESS_LABEL, RenderCache};
use settings::TerminalSettings;
use state::{Command, Event, JobOutcome, ReaderState};

const IDLE_POLL: Duration = Duration::from_secs(2);
const INPUT_WAIT: Duration = Duration::from_millis(100);

/// Local midnight of the day `now` belongs to, in UTC (the app's "Today").
pub fn today_start<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Utc> {
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).expect("00:00 exists");
    let zone = now.timezone();
    zone.from_local_datetime(&midnight)
        .earliest()
        // A zone that skips midnight (DST at 00:00): the day starts at the first valid minute.
        .or_else(|| zone.from_local_datetime(&(midnight + chrono::Duration::hours(1))).earliest())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc))
}

/// The two clocks a command needs: `now` for writes, `local` for "Today".
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub now: DateTime<Utc>,
    pub local: DateTime<FixedOffset>,
}

impl Clock {
    fn system() -> Self {
        let local = Local::now().fixed_offset();
        Self { now: local.with_timezone(&Utc), local }
    }
}

/// `PRAGMA data_version` polling: after two seconds without a key, a moved version is a reload.
pub struct Poll {
    last_key: Instant,
    last_check: Instant,
    version: i64,
}

impl Poll {
    pub fn new(now: Instant, version: i64) -> Self {
        Self { last_key: now, last_check: now, version }
    }

    pub fn keyed(&mut self, now: Instant) {
        self.last_key = now;
    }

    pub fn check(&mut self, now: Instant, version: i64) -> Option<Event> {
        if now.duration_since(self.last_key) < IDLE_POLL || now.duration_since(self.last_check) < IDLE_POLL {
            return None;
        }
        self.last_check = now;
        if version == self.version {
            return None;
        }
        self.version = version;
        Some(Event::DataChanged)
    }
}

/// Runs the state's commands: quick reads and writes on the loop's connection, long work in jobs.
pub struct Executor<'a> {
    app: &'a Ressac,
    db: PathBuf,
    tx: Sender<Msg>,
    stop: Arc<AtomicBool>,
    long_running: bool,
    queue: VecDeque<Command>,
}

impl<'a> Executor<'a> {
    pub fn new(app: &'a Ressac, db: PathBuf, tx: Sender<Msg>) -> Self {
        Self { app, db, tx, stop: Arc::new(AtomicBool::new(false)), long_running: false, queue: VecDeque::new() }
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn long_job_running(&self) -> bool {
        self.long_running
    }

    /// Called when a long job reports `JobDone`: the next queued one starts.
    pub fn long_job_done(&mut self, clock: Clock) -> Vec<Event> {
        self.long_running = false;
        match self.queue.pop_front() {
            Some(next) => self.run(next, clock),
            None => vec![],
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Runs one command; quick ones answer with events at once, jobs answer later on the channel.
    pub fn run(&mut self, command: Command, clock: Clock) -> Vec<Event> {
        // Copy the `&'a Ressac` first: a store borrowed through `&self` would block `self.refresh`.
        let app = self.app;
        let store = app.store();
        let today = today_start(clock.local);
        let result: anyhow::Result<Vec<Event>> = match command {
            Command::LoadSources => store.sources(today).map(|s| vec![Event::Sources(s)]).map_err(Into::into),
            Command::LoadEntries { source, filter, after } => store
                .entries(source, filter, after.as_ref(), today)
                .map(|page| vec![Event::Entries { source, page, appended: after.is_some() }])
                .map_err(Into::into),
            Command::LoadJournal => store.journal().map(|j| vec![Event::Journal(j)]).map_err(Into::into),
            Command::Search { source, text } => store
                .search_entries(source, &text, None, today)
                .map(|page| vec![Event::Entries { source, page, appended: false }])
                .map_err(Into::into),
            Command::LoadEntry(id) => store
                .entry_detail(id)
                .map(|d| d.map(Event::Entry).into_iter().collect())
                .map_err(Into::into),
            Command::SetRead(id, read) => store.set_read(id, read, clock.now).map(|()| vec![Event::DataChanged]).map_err(Into::into),
            Command::SetStarred(id, on) => store.set_starred(id, on, clock.now).map(|()| vec![Event::DataChanged]).map_err(Into::into),
            Command::MarkAllRead(source) => store
                .mark_all_read(source, today, clock.now)
                .map(|_| vec![Event::DataChanged])
                .map_err(Into::into),
            Command::SetCadence(feed, group) => {
                self.app.set_cadence(feed, group).map(|_| vec![Event::DataChanged]).map_err(Into::into)
            }
            Command::OpenUrl(url) => open::that(&url).map(|()| vec![]).map_err(Into::into),
            Command::Extract(id) => {
                let _ = self.tx.send(Msg::Event(Event::Progress { label: EXTRACT_PROGRESS_LABEL.into(), done: 0, total: 1 }));
                jobs::spawn(self.db.clone(), self.tx.clone(), self.stop.clone(), move |app, _tx, _stop| async move {
                    jobs::extract_outcome(id, app.extract_entry(id, &ressac_core::service::NullSink).await)
                });
                Ok(vec![])
            }
            Command::Refresh(_) | Command::BuildEdition if self.long_running => {
                self.queue.push_back(command);
                Ok(vec![])
            }
            Command::Refresh(source) => self.refresh(source),
            Command::BuildEdition => {
                self.long_running = true;
                jobs::spawn(self.db.clone(), self.tx.clone(), self.stop.clone(), |app, tx, stop| async move {
                    let sink = ProgressSink::new(EDITION_PROGRESS_LABEL, tx);
                    let now = Utc::now();
                    jobs::edition_outcome(app.build_edition(EditionOptions::default(), now, &Local, &sink, stop).await)
                });
                Ok(vec![])
            }
            // Phase 6 fetches images; phase 8 runs the garden's commands.
            Command::FetchImage(_) | Command::Garden(_) => Ok(vec![]),
            Command::SaveSettings | Command::Quit => Ok(vec![]),
        };
        result.unwrap_or_else(|e| vec![Event::JobDone(JobOutcome::Failed(busy_or(e)))])
    }

    fn refresh(&mut self, source: Option<Source>) -> anyhow::Result<Vec<Event>> {
        let only = match source {
            Some(Source::Feed(id)) => Some(vec![id]),
            Some(Source::Folder(id)) => Some(self.app.store().feeds_in_folder(id)?),
            _ => None,
        };
        self.long_running = true;
        jobs::spawn(self.db.clone(), self.tx.clone(), self.stop.clone(), move |app, tx, stop| async move {
            let sink = ProgressSink::new(FETCH_PROGRESS_LABEL, tx);
            match app.refresh(RefreshOptions { force: true, only }, &sink, stop).await {
                Ok(summary) if summary.network_unreachable => JobOutcome::Offline,
                Ok(_) => JobOutcome::Refreshed,
                Err(e) => JobOutcome::Failed(e.to_string()),
            }
        });
        Ok(vec![])
    }
}

/// A busy database (Ressac.app writing for more than the busy timeout) gets the spec's sentence.
fn busy_or(e: anyhow::Error) -> String {
    let busy = e.chain().any(|c| {
        c.downcast_ref::<rusqlite::Error>().is_some_and(|r| {
            matches!(r.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
        })
    });
    if busy { "The database is busy (is Ressac.app writing?)".into() } else { format!("{e:#}") }
}

/// Opens the reader on `app` (already opened by `commands::read`) until `q`.
pub fn run(app: &Ressac, db: &Path, settings_path: &Path) -> anyhow::Result<()> {
    let settings = TerminalSettings::load(settings_path);
    let mut terminal = ratatui::try_init().context("cannot start the reader (is this a terminal?)")?;
    let size = terminal.size()?;
    let mut state = ReaderState::new(settings, size.width, size.height);
    let result = event_loop(&mut terminal, app, db, settings_path, &mut state);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &Ressac,
    db: &Path,
    settings_path: &Path,
    state: &mut ReaderState,
) -> anyhow::Result<()> {
    let (tx, rx): (Sender<Msg>, Receiver<Msg>) = mpsc::channel();
    let mut exec = Executor::new(app, db.to_path_buf(), tx);
    let mut cache = RenderCache::default();
    let mut poll = Poll::new(Instant::now(), app.store().data_version()?);
    let mut last_tick = Instant::now();
    let mut pending: VecDeque<Command> = [Command::LoadSources, Command::LoadJournal]
        .into_iter()
        .chain(state.on_event(Event::Tick { now: Clock::system().now, local: Clock::system().local }))
        .collect();
    loop {
        // Execute everything the state asked for; quick answers go straight back to it.
        while let Some(command) = pending.pop_front() {
            match command {
                Command::Quit => {
                    exec.stop();
                    return Ok(());
                }
                Command::SaveSettings => {
                    if let Err(e) = state.settings().save(settings_path) {
                        tracing::warn!("cannot save the terminal settings: {e}");
                    }
                }
                other => {
                    for event in exec.run(other, Clock::system()) {
                        pending.extend(state.on_event(event));
                    }
                }
            }
        }
        let mut metrics = None;
        terminal.draw(|frame| metrics = Some(native::render(frame, state, &mut cache)))?;
        if let Some(m) = metrics {
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
        }
        // Messages from the jobs.
        while let Ok(Msg::Event(event)) = rx.try_recv() {
            let long_done = matches!(
                event,
                Event::JobDone(JobOutcome::Refreshed | JobOutcome::Offline | JobOutcome::EditionBuilt
                    | JobOutcome::EditionNotDue | JobOutcome::Failed(_))
            );
            pending.extend(state.on_event(event));
            if long_done {
                for e in exec.long_job_done(Clock::system()) {
                    pending.extend(state.on_event(e));
                }
            }
        }
        // Keys and resizes.
        if event::poll(INPUT_WAIT)? {
            match event::read()? {
                TermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    poll.keyed(Instant::now());
                    pending.extend(state.on_key(key));
                }
                TermEvent::Resize(width, height) => pending.extend(state.on_event(Event::Resize { width, height })),
                _ => {}
            }
        }
        // The clock, and what Ressac.app or a job wrote.
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            let clock = Clock::system();
            pending.extend(state.on_event(Event::Tick { now: clock.now, local: clock.local }));
        }
        if let Some(event) = poll.check(Instant::now(), app.store().data_version()?) {
            pending.extend(state.on_event(event));
        }
    }
}
```

Two details the implementer resolves against the real code:
- `state.settings()` (the current `TerminalSettings`, so `SaveSettings` can write the look and the images choice) is not in the plan index; if 4b has no such accessor, add it to Task 5a.0's commit (`pub fn settings(&self) -> TerminalSettings`).
- A `JobOutcome::Failed` may come from an extraction (not a long job). Extractions report `Extracted { .. }` on success and on failure (see `extract_outcome`), so `Failed` only ever comes from a long job or from a job that could not open the database; for the latter `long_job_done` is harmless (it pops the queue).
- `rusqlite` is not a direct dependency of the `ressac` binary. If `busy_or` needs it, match on the core's error instead: `ressac_core::Error::Db(e)` and `e.sqlite_error_code()` (the core re-exports nothing of rusqlite; check `crates/ressac-core/src/error.rs` and use `e.to_string().contains("database is locked")` only as the last resort). The test `data_version_moves_when_another_connection_writes` needs `rusqlite` as a **dev**-dependency of `crates/ressac` (`rusqlite.workspace = true` under `[dev-dependencies]`): that `Cargo.toml` edit is a test-only change in the sixth file; if the reviewer counts it, open the second connection with `ressac_core::service::Ressac::open(&db)` and write through `store().set_starred(...)` on an inserted entry instead.

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ressac --bin ressac tui::reader`
Expected: PASS (state, native, jobs and the six loop tests).

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/mod.rs
git commit -m "Reader: the event loop, the executor, data_version polling every two idle seconds

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5b.3: `ressac read`

- [ ] **Step 1: Write the failing CLI tests** at the end of `crates/ressac/tests/cli.rs`, with its helpers (`ressac`, `stdout`, `stderr`; read their exact signatures at the top of the file before pasting).

```rust
#[test]
fn read_on_an_empty_database_prints_the_first_run_hint() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("r.sqlite");
    let out = ressac(&dir, &["--db", db.to_str().unwrap(), "read"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("No feeds yet"), "{}", stderr(&out));
}

#[test]
fn read_without_a_terminal_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("r.sqlite");
    ressac(&dir, &["--db", db.to_str().unwrap(), "import", FIXTURE]);
    let out = ressac(&dir, &["--db", db.to_str().unwrap(), "read"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("is this a terminal?"), "{}", stderr(&out));
}
```

The second test relies on `ratatui::try_init()` failing when stdin/stdout are pipes (the test harness's `Command::output()`), which is what the garden's `run` already reports. If `try_init` succeeds without a TTY on CI, drop that test and keep the first one: never make a test open a real terminal.

- [ ] **Step 2: Run them to verify they fail.**

Run: `cargo test -p ressac --test cli read_`
Expected: FAIL, clap's `unrecognized subcommand 'read'`.

- [ ] **Step 3: Implement.**

`cli.rs`, in `enum Command` after `Garden`:

```rust
    /// Read your feeds in the terminal: the Journal first, sources, entries and the article
    Read,
```

`main.rs`, in the `match cli.command`:

```rust
        Command::Read => commands::read(&g),
```

`commands/mod.rs`: make `fn open` `pub(crate)` and add:

```rust
/// `ressac read`: the terminal reader on the database, until `q`.
pub fn read(g: &Globals) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    crate::tui::reader::run(&app, &g.db, &crate::tui::reader::settings::TerminalSettings::default_path())?;
    Ok(ExitCode::SUCCESS)
}
```

Phase 8 turns `cli.command` into `Option<Command>` so that `ressac` alone opens the reader; this phase does not.

- [ ] **Step 4: Run the tests and the gate.**

```bash
cargo test -p ressac --test cli read_
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
```

Expected: PASS everywhere, no warning.

- [ ] **Step 5: Look at it once, on a copy.** Never on the real database:

```bash
SCRATCH=$(mktemp -d); sqlite3 -readonly "$HOME/Library/Application Support/Ressac/ressac.sqlite" ".backup '$SCRATCH/r.sqlite'"
cargo run -q -p ressac -- --db "$SCRATCH/r.sqlite" read
```

In a real terminal: the Journal's rows, `j`/`k`, `Tab`, an article with numbered links, `/` search, `r` refresh with its progress, `q`. Report what you saw; do not script keystrokes.

- [ ] **Step 6: Commit.**

```bash
git add crates/ressac/src/cli.rs crates/ressac/src/main.rs crates/ressac/src/commands/mod.rs crates/ressac/tests/cli.rs
git commit -m "CLI: ressac read opens the terminal reader

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

## Deviations from the plan index (to reconcile)

1. `native::render(frame, state, cache) -> ReaderMetrics` (no `images` parameter until phase 6, which adds `images: &mut ImageStore`; `&mut` because `StatefulImage` needs it).
2. `ReaderState::view()`, `ReaderView`, `SourceLine`, `set_reader_metrics`, `settings()` are new public names on the state (Task 5a.0).
3. `tui::reader::run(app, db, settings_path)` takes the database path (jobs open their own connection); the index says `run(app, settings_path)`.
4. `Event` and `JobOutcome` need `Debug` and `PartialEq`.
5. New module `tui/reader/jobs.rs` (`Msg`, `ProgressSink`, `spawn`, `edition_outcome`, `extract_outcome`); `commands::open` becomes `pub(crate)`.
6. Progress labels live in `native.rs`: `EDITION_PROGRESS_LABEL`, `FETCH_PROGRESS_LABEL`, `EXTRACT_PROGRESS_LABEL`.
