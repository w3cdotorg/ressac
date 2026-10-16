# Phase 9: Antiope in the terminal

**Runs after:** phase 8 (9a, then 9b).

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`): Global Constraints (Language, Exact strings (Antiope), Teletext palette, Reserved pages, Layout classes, Text, Images, Time), Review Focus #1 (rows within their column, applied here to Antiope rows), and "Interfaces between phases". Spec §5 (Antiope keys), §8 (Antiope in the terminal), §7 (Antiope images), §10 (rendering snapshots at 140 and 70 columns). The Antiope spec (`docs/specs/2026-10-14-antiope-design.md`) §6 (page header, page numbers, Minitel bar, P100, P899) and §8 (garden pages) are the model; the Swift code under `apps/macos/Ressac/Antiope/` is the reference for wording and colours (it is not changed).

**Reconciled with the other phases (controller, 2026-10-16; these rules override the code below where they differ):**
- `ReaderState`'s fields are public (phase 4b). The accessors this file calls map to: `sources()` → `state.sources.as_ref()`; `entries()` → `&state.entries`; `article()` → `state.article.as_ref()`; `progress()` → `state.progress` (`Option<(String, usize, usize)>`); `search_text()` → `state.search.as_deref()`; `is_searching()` → `state.search_editing`; `clock()` → `state.local` (`Option<DateTime<FixedOffset>>`, set by `start` and every `Tick`); `journal()` and `journal_phase()` → phase 7's methods (`journal_phase()` returns `&JournalPhase`); `garden()`, `garden_view()` → phase 8's methods; `go_to(source)`, `select_entry(id)`, `set_focus(column)` → phase 4b's methods; `go_to_garden(view)` → phase 8's `open_garden(view)`.
- `pages()` and `set_pages`: nobody loads the page list before this phase, so **9a** adds it: `pub pages: Vec<PageRow>` on `ReaderState` (empty in `new`), `Command::LoadPages` (sent by `start` and on `DataChanged`), `Event::Pages(Vec<PageRow>)` (stores them), and the executor arm in `mod.rs` (`app.pages()`). 9a then touches 4 files (`antiope.rs`, `pages.rs`, `mod.rs`, `state.rs`).
- Images: no `ImageMode`. Use phase 6's `Variant { Native, Teletext }`: Antiope draws `Variant::Teletext` and, while `i` shows the real image, `Variant::Native`; keep that toggle as a `pub antiope_real_images: bool` field (9b) instead of `ImageStore::set_mode`/`mode()`/`prepare_for_mode`. `ImageStore::disabled()` is provided by phase 6. The render signature is `antiope::render(frame: &mut Frame, state: &ReaderState, images: &mut ImageStore) -> ReaderMetrics`, like Native.
- `JobOutcome::EditionBuiltElsewhere` no longer exists.

**Sub-phases:**

| Sub-phase | Deliverable | Files (excluding `.snap`) |
|---|---|---|
| 9a | `tui/reader/antiope.rs`: teletext palette and cell fitting, page header with the clock and the searching counter, Minitel bar, folded columns with blue `▐` separators, index rows, P100 SOMMAIRE, P899 GUIDE, `render`; `tui/reader/pages.rs`: the page directory; `tui/reader/mod.rs`: render by look | 4 (with `state.rs` for the page list, see above) |
| 9b | Page entry (`⌫`, `Esc`, `P437 NOT FOUND`), navigation (`AntiopeNav`), P104 JOURNAL, sub-pages `104 n/N`, `FIN DE L'EDITION`, P105–P109, P800–P802, feed and folder pages, Antiope images (teletext half-blocks, `i` real image) | 4 (`pages.rs`, `antiope.rs`, `state.rs`, `images.rs`) |

**Builds on (plan index names; another phase owns each):**
- Phase 1: `store::{Source, SourcesSnapshot, SourceCounts, SourceNode, SourceNodeKind, EntryRow, JournalView, JournalItem}`, `EditionRecord` (`built_at`, `unreachable`), `EditionOverflowRow` (`section`, `feed_id`, `feed_title`, `page`, `hidden_count`), `edition::Section` (`Front`, `Week`, `Wire`), `CadenceGroup`.
- Existing core: `Ressac::pages() -> Result<Vec<PageRow>>`, `store::pages::{PageRow, PageKind, RESERVED_PAGES, ASSIGNABLE}` (titles `Index`, `Unread`, `Today`, `Starred`, `Journal`, `Daily`, `Weekly`, `Monthly`, `Rare`, `Silent`, `Needs triage`, `All feeds`, `Cemetery`, `Guide`), `garden::GardenRow` (`feed_id`, `title`, `folder_path`, `buried`, `status()`), `model::Status` (`ALL`, `as_str`, `urgency`).
- Phase 3: `RenderedArticle { lines, links, images }`, `ImageRef { url, alt, line }`.
- Phase 4: `Look`, `Column`, `LayoutClass`, `layout_class`, `ColumnRects`, `columns(area, class, focus, Look::Antiope)` (Antiope widths: index 22, entries 28, reader the rest; it folds like Native), `ReaderState` (`look()`, `focus()`, `source()`, `selected_entry()`), `Command`, `Event::Tick { now, local }`.
- Phases 5–8, **read accessors on `ReaderState`** this phase calls (each phase exposes the ones it owns; if a name differs, use the real one and say so in the report): `sources() -> Option<&SourcesSnapshot>`, `pages() -> &[PageRow]`, `entries() -> &[EntryRow]`, `journal() -> Option<&JournalView>`, `journal_phase() -> JournalPhase` (phase 7; variants `Idle`, `Due`, `Building { done, total }`, `Offline`, `Failed(String)`), `article() -> Option<&RenderedArticle>` (phase 5), `progress() -> Option<(&str, usize, usize)>` (phase 5), `garden() -> Option<&GardenState>` (phase 8), `search_text() -> &str`, `filter() -> ListFilter`; and the navigation hooks `pub(crate) fn go_to(&mut self, source: Source) -> Vec<Command>`, `pub(crate) fn go_to_garden(&mut self, view: GardenView) -> Vec<Command>`, `pub(crate) fn select_entry(&mut self, entry_id: i64) -> Vec<Command>`, `pub(crate) fn set_focus(&mut self, column: Column)` (phases 4b, 8).
- Phase 6: `images::{ImageStore, quantise_teletext, TELETEXT}`; `ImageStore` keeps one protocol per URL.
- Phase 8: `GardenView` (`NeedsTriage`, `AllFeeds`, `Cemetery`), the garden's `GardenState` with `rows` and `selected_row()`.
- Existing: `crate::output::printable` (control characters → U+FFFD).

**Decisions:**
- **Colours are exact RGB** (`Color::Rgb`), never the terminal's named ANSI colours, which themes repaint. Background black on every Antiope cell.
- **Widths come from ratatui** (`Span::width()`, which uses `unicode-width`), so a wide emoji counts two cells, as the terminal draws it. `fit()` truncates on display width and pads with spaces; a wide character that would cross the limit becomes a space. No new dependency.
- **Blue never colours text:** `span()` refuses a blue foreground unless the text is only block elements (U+2580–U+259F, the `▐` separators and mosaics); a debug assertion and a test pin it. Fossil headings and overflow rows are white on blue.
- **Double height does not exist** in a terminal: `RESSAC`, `GUIDE` and `FIN DE L'EDITION` are full-width black-on-yellow rows (spec §8).
- **The Minitel keys are black on cyan** (` SOMMAIRE `), as `MinitelKey.line` in the app; the spec's "reverse video" is this.
- **Accents stay in capitals** (`ACTUALITTÉ`), as the app does.
- **The GUIDE lists the terminal's keys** (spec §5), not the app's `⌘`/`⌥` chords.
- **Navigation lives in `AntiopeNav`** (pure, in `pages.rs`), so that `state.rs` only maps its outcome to the hooks above; the page directory is rebuilt from `pages()` and `sources()` whenever either reloads.

---

## Sub-phase 9a: the teletext frame

**Files:**
- Create: `crates/ressac/src/tui/reader/antiope.rs` (palette, cells, header, Minitel bar, columns, index rows, P100, P899, `render`, tests inline)
- Create: `crates/ressac/src/tui/reader/pages.rs` (`PageDirectory`, `DirRow`, `PageDestination`, tests inline)
- Modify: `crates/ressac/src/tui/reader/mod.rs` (`pub mod antiope; pub mod pages;`, render by look)

### Task 9a.1: palette, cells and the page directory

- [ ] **Step 1: Write the failing tests** at the end of `antiope.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::{Line, Span};

    #[test]
    fn the_palette_is_the_eight_teletext_colours() {
        let rgb: Vec<_> = Tt::ALL.iter().map(|c| c.rgb()).collect();
        assert_eq!(
            rgb,
            [(0, 0, 0), (255, 0, 0), (0, 255, 0), (255, 255, 0), (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255)]
        );
        assert_eq!(Tt::Cyan.color(), ratatui::style::Color::Rgb(0, 255, 255));
    }

    #[test]
    fn fit_counts_display_cells_and_pads() {
        assert_eq!(fit("ABC", 5), "ABC  ");
        assert_eq!(fit("ABCDEF", 4), "ABCD");
        // ☕ and 東 are two cells wide: 東 would cross the limit and becomes a space.
        assert_eq!(Span::raw(fit("CAFÉ ☕ 東", 8)).width(), 8);
        assert_eq!(fit("CAFÉ ☕ 東", 8), "CAFÉ ☕ ");
        assert_eq!(fit("", 3), "   ");
    }

    #[test]
    fn a_row_with_accents_and_emoji_is_exactly_its_width() {
        // Review Focus #1, Antiope side.
        let line = row(vec![
            span("  ", 2, Tt::White, Tt::Black),
            span("+3 MORE IN CAFÉ ☕ PÉDAGO", 26, Tt::White, Tt::Blue),
        ]);
        assert_eq!(line.width(), 28);
    }

    #[test]
    fn control_characters_never_reach_a_cell() {
        let s = span("A\u{1b}]0;x\u{7}B", 6, Tt::White, Tt::Black);
        assert!(!s.content.chars().any(|c| c.is_control()));
        assert_eq!(s.width(), 6);
    }

    #[test]
    fn blue_only_colours_block_elements() {
        assert!(blue_allowed("▐▐"));
        assert!(blue_allowed("▀▄█"));
        assert!(!blue_allowed("TEXT"));
    }

    #[test]
    #[should_panic(expected = "blue never colours text")]
    fn blue_text_is_refused_in_debug() {
        let _ = span("DEAD", 4, Tt::Blue, Tt::Black);
    }

    fn _unused(_: Line) {}
}
```

and at the end of `pages.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::store::pages::{PageKind, PageRow};

    fn reserved(page: u16, title: &str) -> PageRow {
        PageRow { page, kind: PageKind::Reserved, id: None, title: title.into(), parent_page: None }
    }

    pub(crate) fn sample_pages() -> Vec<PageRow> {
        let mut rows: Vec<PageRow> = ressac_core::store::pages::RESERVED_PAGES
            .iter()
            .zip(["Index", "Unread", "Today", "Starred", "Journal", "Daily", "Weekly", "Monthly", "Rare", "Silent",
                  "Needs triage", "All feeds", "Cemetery", "Guide"])
            .map(|(p, t)| reserved(*p, t))
            .collect();
        rows.push(PageRow { page: 400, kind: PageKind::Folder, id: Some(4), title: "Infos".into(), parent_page: None });
        rows.push(PageRow { page: 403, kind: PageKind::Feed, id: Some(31), title: "Actualitté".into(), parent_page: Some(400) });
        rows.push(PageRow { page: 412, kind: PageKind::Feed, id: Some(12), title: "Courrier international".into(), parent_page: Some(400) });
        rows.push(PageRow { page: 110, kind: PageKind::Feed, id: Some(2), title: "Daring Fireball".into(), parent_page: None });
        rows.sort_by_key(|r| (r.kind != PageKind::Reserved, r.page));
        rows
    }

    #[test]
    fn reserved_pages_go_where_the_app_sends_them() {
        let dir = PageDirectory::new(&sample_pages(), None);
        assert_eq!(dir.destination(100), Some(PageDestination::Sommaire));
        assert_eq!(dir.destination(101), Some(PageDestination::Source(Source::Unread)));
        assert_eq!(dir.destination(102), Some(PageDestination::Source(Source::Today)));
        assert_eq!(dir.destination(103), Some(PageDestination::Source(Source::Starred)));
        assert_eq!(dir.destination(104), Some(PageDestination::Source(Source::Journal)));
        assert_eq!(dir.destination(105), Some(PageDestination::Source(Source::Cadence(CadenceGroup::Daily))));
        assert_eq!(dir.destination(109), Some(PageDestination::Source(Source::Cadence(CadenceGroup::Silent))));
        assert_eq!(dir.destination(800), Some(PageDestination::Garden(GardenView::NeedsTriage)));
        assert_eq!(dir.destination(801), Some(PageDestination::Garden(GardenView::AllFeeds)));
        assert_eq!(dir.destination(802), Some(PageDestination::Garden(GardenView::Cemetery)));
        assert_eq!(dir.destination(899), Some(PageDestination::Guide));
    }

    #[test]
    fn folders_and_feeds_go_to_their_source() {
        let dir = PageDirectory::new(&sample_pages(), None);
        assert_eq!(dir.destination(400), Some(PageDestination::Source(Source::Folder(4))));
        assert_eq!(dir.destination(412), Some(PageDestination::Source(Source::Feed(12))));
        assert_eq!(dir.destination(437), None);
        assert_eq!(dir.page_of(Source::Feed(12)), Some(412));
        assert_eq!(dir.page_of(Source::Journal), Some(104));
    }

    #[test]
    fn rows_have_depth_and_root_folders_are_marked() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let infos = dir.rows().iter().find(|r| r.page == 400).unwrap();
        let actualitte = dir.rows().iter().find(|r| r.page == 403).unwrap();
        assert_eq!((infos.depth, infos.is_root_folder), (0, true));
        assert_eq!((actualitte.depth, actualitte.is_root_folder), (1, false));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

```sh
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)" && cargo test -p ressac reader::antiope reader::pages 2>&1 | tail -5
```

Expected: compile errors (`cannot find type Tt`, `PageDirectory`), the modules do not exist yet.

- [ ] **Step 3: Write `pages.rs`**

```rust
//! The teletext page directory of the terminal reader (Antiope spec §4, §6): every reserved page
//! and every numbered folder and feed, where each one leads, and its count.

use std::collections::HashMap;

use ressac_core::model::CadenceGroup;
use ressac_core::store::pages::{PageKind, PageRow};
use ressac_core::store::{Source, SourceNode, SourceNodeKind, SourcesSnapshot};

use crate::tui::reader::state::GardenView;

/// Where typing a page number leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDestination {
    Sommaire,
    Guide,
    Source(Source),
    Garden(GardenView),
}

/// One row of the index column and of P100.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRow {
    pub page: u16,
    pub title: String,
    pub kind: PageKind,
    pub depth: usize,
    pub is_root_folder: bool,
    /// Unread entries (feeds, folders, smart lists, rhythm groups) or feeds to triage (P800).
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageDirectory {
    rows: Vec<DirRow>,
    destinations: HashMap<u16, PageDestination>,
}

fn reserved_destination(page: u16) -> Option<PageDestination> {
    use PageDestination::*;
    Some(match page {
        100 => Sommaire,
        101 => Source(ressac_core::store::Source::Unread),
        102 => Source(ressac_core::store::Source::Today),
        103 => Source(ressac_core::store::Source::Starred),
        104 => Source(ressac_core::store::Source::Journal),
        105 => Source(ressac_core::store::Source::Cadence(CadenceGroup::Daily)),
        106 => Source(ressac_core::store::Source::Cadence(CadenceGroup::Weekly)),
        107 => Source(ressac_core::store::Source::Cadence(CadenceGroup::Monthly)),
        108 => Source(ressac_core::store::Source::Cadence(CadenceGroup::Rare)),
        109 => Source(ressac_core::store::Source::Cadence(CadenceGroup::Silent)),
        800 => Garden(GardenView::NeedsTriage),
        801 => Garden(GardenView::AllFeeds),
        802 => Garden(GardenView::Cemetery),
        899 => Guide,
        _ => return None,
    })
}

impl PageDirectory {
    /// `pages` as `Ressac::pages()` returns them (reserved first, then by page); `sources` gives
    /// the counts, absent until the first load.
    pub fn new(pages: &[PageRow], sources: Option<&SourcesSnapshot>) -> Self {
        let counts = sources.map(unread_by_node).unwrap_or_default();
        let by_page: HashMap<u16, &PageRow> = pages.iter().map(|r| (r.page, r)).collect();
        let mut destinations = HashMap::new();
        let mut rows = Vec::with_capacity(pages.len());
        for row in pages {
            let destination = match row.kind {
                PageKind::Reserved => reserved_destination(row.page),
                PageKind::Folder => row.id.map(|id| PageDestination::Source(Source::Folder(id))),
                PageKind::Feed => row.id.map(|id| PageDestination::Source(Source::Feed(id))),
            };
            if let Some(d) = destination {
                destinations.insert(row.page, d);
            }
            let mut depth = 0;
            let mut parent = row.parent_page;
            while let Some(p) = parent {
                depth += 1;
                parent = by_page.get(&p).and_then(|r| r.parent_page);
            }
            let count = match (row.kind, sources) {
                (PageKind::Reserved, Some(s)) => reserved_count(row.page, s),
                (PageKind::Folder, _) => row.id.and_then(|id| counts.get(&(PageKind::Folder, id)).copied()),
                (PageKind::Feed, _) => row.id.and_then(|id| counts.get(&(PageKind::Feed, id)).copied()),
                _ => None,
            };
            rows.push(DirRow {
                page: row.page,
                title: row.title.clone(),
                kind: row.kind,
                depth,
                is_root_folder: row.kind == PageKind::Folder && row.parent_page.is_none(),
                count,
            });
        }
        PageDirectory { rows, destinations }
    }

    pub fn rows(&self) -> &[DirRow] {
        &self.rows
    }

    pub fn destination(&self, page: u16) -> Option<PageDestination> {
        self.destinations.get(&page).copied()
    }

    /// The page a source sits on, for the header and the sub-page label.
    pub fn page_of(&self, source: Source) -> Option<u16> {
        self.destinations
            .iter()
            .find(|(_, d)| **d == PageDestination::Source(source))
            .map(|(p, _)| *p)
    }

    pub fn page_of_garden(&self, view: GardenView) -> u16 {
        match view {
            GardenView::NeedsTriage => 800,
            GardenView::AllFeeds => 801,
            GardenView::Cemetery => 802,
        }
    }
}

fn reserved_count(page: u16, s: &SourcesSnapshot) -> Option<usize> {
    let c = &s.counts;
    Some(match page {
        101 => c.unread,
        102 => c.today,
        103 => c.starred,
        104 => c.journal,
        105 => c.cadence.get(&CadenceGroup::Daily).copied().unwrap_or(0),
        106 => c.cadence.get(&CadenceGroup::Weekly).copied().unwrap_or(0),
        107 => c.cadence.get(&CadenceGroup::Monthly).copied().unwrap_or(0),
        108 => c.cadence.get(&CadenceGroup::Rare).copied().unwrap_or(0),
        109 => c.cadence.get(&CadenceGroup::Silent).copied().unwrap_or(0),
        800 => c.needs_triage,
        _ => return None,
    })
}

fn unread_by_node(s: &SourcesSnapshot) -> HashMap<(PageKind, i64), usize> {
    fn walk(nodes: &[SourceNode], out: &mut HashMap<(PageKind, i64), usize>) {
        for n in nodes {
            match &n.kind {
                SourceNodeKind::Folder { id, .. } => out.insert((PageKind::Folder, *id), n.unread),
                SourceNodeKind::Feed { id, .. } => out.insert((PageKind::Feed, *id), n.unread),
            };
            walk(&n.children, out);
        }
    }
    let mut out = HashMap::new();
    walk(&s.tree, &mut out);
    out
}
```

(The `FolderId` and `FeedId` aliases are `i64` in `ressac_core::model`; if phase 1's `Source` carries other types, convert at the two `Source::Folder`/`Source::Feed` sites.)

- [ ] **Step 4: Write the palette and cells part of `antiope.rs`**

```rust
//! Antiope in the terminal (spec §8): the eight teletext colours on black, rows exactly as wide
//! as their column, the page header, the Minitel bar, P100 SOMMAIRE and P899 GUIDE, then (9b)
//! the pages of the reader. Pure functions over plain data, and `render`.

use chrono::{DateTime, FixedOffset};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::output::printable;

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
    pub const ALL: [Tt; 8] = [Tt::Black, Tt::Red, Tt::Green, Tt::Yellow, Tt::Blue, Tt::Magenta, Tt::Cyan, Tt::White];

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

/// Block elements (U+2580–U+259F): the only glyphs blue may colour.
pub fn blue_allowed(text: &str) -> bool {
    text.chars().all(|c| c == ' ' || ('\u{2580}'..='\u{259F}').contains(&c))
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
    debug_assert!(fg != Tt::Blue || blue_allowed(&text), "blue never colours text");
    Span::styled(text, Style::default().fg(fg.color()).bg(bg.color()))
}

pub fn row(spans: Vec<Span<'static>>) -> Line<'static> {
    Line::from(spans)
}

/// `text` centred in `width` cells (one more on the right when the margin is odd).
pub fn centred(text: &str, width: usize, fg: Tt, bg: Tt) -> Line<'static> {
    let w = Span::raw(text.to_string()).width().min(width);
    let left = (width - w) / 2;
    row(vec![span("", left, fg, bg), span(text, width - left, fg, bg)])
}

pub fn blank(width: usize) -> Line<'static> {
    row(vec![span("", width, Tt::White, Tt::Black)])
}

/// The terminal's stand-in for double height (spec §8): a full-width black-on-yellow row.
pub fn title_bar(text: &str, width: usize) -> Line<'static> {
    centred(text, width, Tt::Black, Tt::Yellow)
}
```

- [ ] **Step 5: Wire the modules** in `crates/ressac/src/tui/reader/mod.rs` next to the other `pub mod` lines:

```rust
pub mod antiope;
pub mod pages;
```

- [ ] **Step 6: Run the tests**

```sh
cargo test -p ressac reader::antiope reader::pages 2>&1 | tail -5
```

Expected: all pass (`blue_text_is_refused_in_debug` relies on debug assertions, on in `cargo test`).

- [ ] **Step 7: Commit**

```sh
git add crates/ressac/src/tui/reader/antiope.rs crates/ressac/src/tui/reader/pages.rs crates/ressac/src/tui/reader/mod.rs
git commit -m "Reader, Antiope: teletext palette, cells fitted on display width, the page directory

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9a.2: header, Minitel bar, index rows, P100, P899 and `render`

- [ ] **Step 1: Write the failing tests** (append to the `tests` module of `antiope.rs`):

```rust
    use chrono::TimeZone;

    fn local(h: u32, m: u32, s: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600).unwrap().with_ymd_and_hms(2026, 10, 16, h, m, s).unwrap()
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
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
        let t = text(&header_line(&HeaderLabel::Page(Some(412)), local(9, 5, 0), 10));
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
        let t0 = chrono::Utc.with_ymd_and_hms(2026, 10, 16, 16, 0, 0).unwrap();
        let a = rolling_page(t0);
        let b = rolling_page(t0 + chrono::Duration::milliseconds(100));
        assert!((100..=899).contains(&a));
        assert_eq!(if a == 899 { 100 } else { a + 1 }, b);
    }

    #[test]
    fn the_minitel_bar_is_exactly_its_width() {
        for width in [20, 50, 140] {
            let line = minitel_bar(&MinitelHint::keys(Column::Entries, false, ""), width);
            assert_eq!(line.width(), width, "width {width}");
        }
        let line = minitel_bar(&MinitelHint::keys(Column::Entries, false, ""), 140);
        assert!(text(&line).starts_with(" SOMMAIRE  SUITE  RETOUR  GUIDE  RÉPÉTITION "));
        assert_eq!(line.spans[0].style.bg, Some(Tt::Cyan.color()));
    }

    #[test]
    fn the_status_replaces_the_hints() {
        let h = MinitelHint::status(Some(("Building this morning's edition…", 34, 156)), true).unwrap();
        assert_eq!(h.text, "EDITION IN PROGRESS 34/156");
        assert_eq!(h.colour, Tt::Yellow);
        let h = MinitelHint::status(Some(("Fetching feeds", 42, 155)), false).unwrap();
        assert_eq!(h.text, "REFRESHING 42/155");
        assert!(MinitelHint::status(None, false).is_none());
    }

    #[test]
    fn index_rows_follow_the_app() {
        let dir = crate::tui::reader::pages::PageDirectory::new(&crate::tui::reader::pages::tests::sample_pages(), None);
        let infos = dir.rows().iter().find(|r| r.page == 400).unwrap();
        let line = index_line(infos, 22, false, false);
        assert_eq!(line.width(), 22);
        assert!(text(&line).starts_with("400 INFOS"));
        assert_eq!(line.spans[1].style.fg, Some(Tt::Green.color()));
        let selected = index_line(infos, 22, true, false);
        assert_eq!(selected.spans[0].style.bg, Some(Tt::Cyan.color()));
    }

    #[test]
    fn the_guide_rows_are_exactly_the_width() {
        for line in guide_lines(40) {
            assert_eq!(line.width(), 40);
        }
        let all: String = guide_lines(60).iter().map(text).collect();
        assert!(all.contains("SUITE: next page"));
        assert!(all.contains("o then 1-9") || all.contains("O 1-9"));
    }

    #[test]
    fn sommaire_pairs_are_column_major() {
        let pairs = sommaire_pairs(&[1, 2, 3, 4, 5]);
        assert_eq!(pairs, vec![(1, Some(4)), (2, Some(5)), (3, None)]);
    }
```

Snapshot tests (they need `render`, written in Step 3), also in the tests module:

```rust
    use crate::tui::reader::state::ReaderState;
    use crate::tui::reader::settings::TerminalSettings;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(state: &ReaderState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let images = crate::tui::reader::images::ImageStore::disabled();
        terminal.draw(|f| render_at(f, state, &images, local(18, 33, 12), None)).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn antiope_sommaire_at_140_and_70_columns() {
        let state = crate::tui::reader::state::tests::antiope_state_on_page(100);
        insta::assert_snapshot!("antiope_sommaire_140", draw(&state, 140, 34));
        insta::assert_snapshot!("antiope_sommaire_70", draw(&state, 70, 34));
    }

    #[test]
    fn antiope_guide_at_140_columns() {
        let state = crate::tui::reader::state::tests::antiope_state_on_page(899);
        insta::assert_snapshot!("antiope_guide_140", draw(&state, 140, 40));
    }
```

`antiope_state_on_page(page)` is a test helper added to `state.rs`'s test module in 9b; in 9a, write it locally in `antiope.rs`'s tests as `fn antiope_state_on_page(page: u16) -> ReaderState` building `ReaderState::new(TerminalSettings { look: Look::Antiope, images: false }, 140, 34)`, feeding `Event::Sources` and the sample pages, and moving it to `state.rs` in 9b. `ImageStore::disabled()` is phase 6's constructor without a picker; if phase 6 named it otherwise, use that name.

- [ ] **Step 2: Run them to see them fail**

```sh
cargo test -p ressac reader::antiope 2>&1 | tail -5
```

Expected: compile errors on `header_line`, `minitel_bar`, `index_line`, `guide_lines`, `render_at`.

- [ ] **Step 3: Write the frame** (append to `antiope.rs`):

```rust
use crate::tui::reader::layout::{Column, ColumnRects, LayoutClass, Look, columns, layout_class};
use crate::tui::reader::pages::{DirRow, PageDirectory};
use crate::tui::reader::state::ReaderState;
use crate::tui::reader::images::ImageStore;
use ressac_core::store::pages::PageKind;

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

/// 100…899, one step every tenth of a second.
pub fn rolling_page(now: DateTime<chrono::Utc>) -> u16 {
    let tenths = now.timestamp_millis().div_euclid(100);
    (100 + tenths.rem_euclid(800)) as u16
}

/// `P412 RESSAC … FRI 16 OCT 18:33:12`: the page yellow (red when not found), `RESSAC` white,
/// the date cyan, the time yellow. When narrow, the time keeps its cells first, then the date,
/// then the page.
pub fn header_line(label: &HeaderLabel, local: DateTime<FixedOffset>, width: usize) -> Line<'static> {
    let day = local.format("%a %d %b ").to_string().to_uppercase();
    let time = local.format("%H:%M:%S").to_string();
    let (page, colour, name) = match label {
        HeaderLabel::Page(p) => (format!("P{}", p.map(|p| p.to_string()).unwrap_or_else(|| "---".into())), Tt::Yellow, true),
        HeaderLabel::Typing(d) => (format!("P{d}"), Tt::Yellow, true),
        HeaderLabel::Searching(n) => (format!("P{n}"), Tt::Yellow, true),
        HeaderLabel::NotFound(n) => (format!("P{n} NOT FOUND"), Tt::Red, false),
    };
    let mut remaining = width;
    let mut take = |wanted: usize| {
        let c = wanted.min(remaining);
        remaining -= c;
        c
    };
    let time_cells = take(time.len());
    let day_cells = take(day.len());
    let page_cells = take(page.chars().count() + 1);
    let name_cells = take(usize::MAX);
    row(vec![
        span(&page, page_cells, colour, Tt::Black),
        span(if name { "RESSAC" } else { "" }, name_cells, Tt::White, Tt::Black),
        span(&day, day_cells, Tt::Cyan, Tt::Black),
        span(&time, time_cells, Tt::Yellow, Tt::Black),
    ])
}

/// The text right of the Minitel keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinitelHint {
    pub text: String,
    pub colour: Tt,
}

impl MinitelHint {
    /// The search in force first, then the garden's keys or the focused column's.
    pub fn keys(column: Column, is_garden: bool, search: &str) -> Self {
        let (text, colour) = if !search.is_empty() {
            (format!("SEARCH: {search}"), Tt::Cyan)
        } else if is_garden {
            ("k keep · r repair · b bury · u undo".to_string(), Tt::White)
        } else {
            let t = match column {
                Column::Sources => "type a page number · ? guide",
                Column::Entries => "j next · k previous · m read",
                Column::Reader => "m read · s star · i images · o links",
            };
            (t.to_string(), Tt::White)
        };
        MinitelHint { text, colour }
    }

    /// While a job runs: `EDITION IN PROGRESS 34/156` for the edition, else `REFRESHING 42/155`,
    /// in yellow. `None` keeps the hints.
    pub fn status(progress: Option<(&str, usize, usize)>, edition: bool) -> Option<Self> {
        let (_, done, total) = progress?;
        let head = if edition { "EDITION IN PROGRESS" } else { "REFRESHING" };
        let text = if total > 0 { format!("{head} {done}/{total}") } else { head.to_string() };
        Some(MinitelHint { text, colour: Tt::Yellow })
    }
}

pub const MINITEL_KEYS: [&str; 5] = ["SOMMAIRE", "SUITE", "RETOUR", "GUIDE", "RÉPÉTITION"];

/// ` SOMMAIRE ` … black on cyan, a black cell after each, then the hint. Exactly `width` cells.
pub fn minitel_bar(hint: &MinitelHint, width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut remaining = width;
    for key in MINITEL_KEYS {
        let label = format!(" {key} ");
        let cells = Span::raw(label.clone()).width().min(remaining);
        spans.push(span(&label, cells, Tt::Black, Tt::Cyan));
        remaining -= cells;
        let gap = 1.min(remaining);
        spans.push(span("", gap, Tt::White, Tt::Black));
        remaining -= gap;
    }
    spans.push(span(&format!(" {}", hint.text), remaining, hint.colour, Tt::Black));
    row(spans)
}

/// `101 Unread … 1083`: the number cyan, the title indented by depth, the count yellow at the
/// right edge. Root folders in green capitals, garden pages magenta with P800's count red,
/// reserved titles yellow on P100 (`highlight_reserved`). Selected: black on cyan.
pub fn index_line(r: &DirRow, width: usize, selected: bool, highlight_reserved: bool) -> Line<'static> {
    let number = format!("{} ", r.page);
    let count = r.count.filter(|c| *c > 0).map(|c| format!(" {c}")).unwrap_or_default();
    let number_cells = number.len().min(width);
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
    let title = format!("{}{}", "  ".repeat(r.depth), if r.is_root_folder { r.title.to_uppercase() } else { r.title.clone() });
    let count_colour = if r.page == 800 { Tt::Red } else { Tt::Yellow };
    let (fg_override, bg) = if selected { (Some(Tt::Black), Tt::Cyan) } else { (None, Tt::Black) };
    row(vec![
        span(&number, number_cells, fg_override.unwrap_or(Tt::Cyan), bg),
        span(&title, title_cells, fg_override.unwrap_or(title_colour), bg),
        span(&count, count_cells, fg_override.unwrap_or(count_colour), bg),
    ])
}

/// Column-major pairs, as a printed index: the first half down the left.
pub fn sommaire_pairs<T: Clone>(items: &[T]) -> Vec<(T, Option<T>)> {
    let left = items.len().div_ceil(2);
    (0..left).map(|i| (items[i].clone(), items.get(i + left).cloned())).collect()
}

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
    ("PAGES", &[
        ("0-9", "type a page number"),
        ("DEL", "erase the last digit"),
        ("ESC", "cancel the number"),
        ("SHIFT S", "SOMMAIRE: every page"),
        ("?", "GUIDE: this page"),
    ]),
    ("READING", &[
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
    ]),
    ("COLUMNS", &[
        ("LEFT RIGHT", "change column (also H L TAB)"),
        ("ENTER ESC", "open, go back"),
    ]),
    ("GARDEN", &[
        ("K", "keep"),
        ("R", "repair"),
        ("B", "bury"),
        ("U", "undo"),
        ("O", "open the site"),
        ("W", "open the Wayback snapshot"),
        ("SHIFT R", "apply the safe repairs"),
        ("C", "rhythm of the feed"),
    ]),
    ("WINDOW", &[
        ("T", "NATIVE or ANTIOPE"),
        ("Q", "quit"),
    ]),
];

const KEY_CELLS: usize = 12;

/// `GUIDE` on a yellow bar, then per section a blank row, the heading black on green, one row
/// per key (keys cyan, action white). Every row is exactly `width` cells.
pub fn guide_lines(width: usize) -> Vec<Line<'static>> {
    let key_cells = KEY_CELLS.min(width);
    let mut lines = vec![title_bar("GUIDE", width)];
    for (title, keys) in GUIDE {
        lines.push(blank(width));
        lines.push(row(vec![span(&format!(" {title}"), width, Tt::Black, Tt::Green)]));
        for (k, action) in keys {
            lines.push(row(vec![
                span(&format!(" {k}"), key_cells, Tt::Cyan, Tt::Black),
                span(action, width - key_cells, Tt::White, Tt::Black),
            ]));
        }
    }
    lines
}

/// The `▐` separator, one cell wide, blue on black, the column's height.
fn separator(frame: &mut Frame, x: u16, y: u16, height: u16) {
    let lines: Vec<Line> = (0..height).map(|_| row(vec![span("▐", 1, Tt::Blue, Tt::Black)])).collect();
    frame.render_widget(Paragraph::new(lines), Rect::new(x, y, 1, height));
}

fn paint(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    let mut lines = lines;
    while lines.len() < area.height as usize {
        lines.push(blank(area.width as usize));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Draws the reader in Antiope. `now` drives the clock and the counter; `local` is `now` in the
/// user's zone (from `Event::Tick`).
pub fn render(frame: &mut Frame, state: &ReaderState, images: &ImageStore) {
    let (now, local) = state.clock();
    render_at(frame, state, images, local, Some(now));
}

pub fn render_at(
    frame: &mut Frame,
    state: &ReaderState,
    images: &ImageStore,
    local: DateTime<FixedOffset>,
    now: Option<DateTime<chrono::Utc>>,
) {
    let area = frame.area();
    let class = layout_class(area.width, area.height);
    if class == LayoutClass::TooSmall {
        paint(frame, area, vec![centred("WINDOW TOO SMALL", area.width as usize, Tt::Red, Tt::Black)]);
        return;
    }
    let rects: ColumnRects = columns(area, class, state.focus(), Look::Antiope);
    let dir = PageDirectory::new(state.pages(), state.sources());
    // Header.
    let rolling = state.progress().is_some() && now.is_some();
    let label = state
        .antiope_header_label(&dir)
        .unwrap_or_else(|| if rolling { HeaderLabel::Searching(rolling_page(now.unwrap())) } else { HeaderLabel::Page(None) });
    paint(frame, rects.top, vec![header_line(&label, local, rects.top.width as usize)]);
    // Bottom bar.
    let edition = state.progress().map(|(l, _, _)| l.starts_with("Building")).unwrap_or(false);
    let hint = MinitelHint::status(state.progress(), edition)
        .unwrap_or_else(|| MinitelHint::keys(state.focus(), state.garden().is_some(), state.search_text()));
    paint(frame, rects.bottom, vec![minitel_bar(&hint, rects.bottom.width as usize)]);
    // Columns, separated by `▐`.
    if let Some(r) = rects.sources {
        let lines = dir.rows().iter().map(|row| index_line(row, r.width as usize, dir.page_of_current(state) == Some(row.page), false)).collect();
        paint(frame, r, lines);
        separator(frame, r.x + r.width, r.y, r.height);
    }
    if let Some(r) = rects.entries {
        paint(frame, r, entry_column(state, r.width as usize, local));
        if rects.reader.is_some() {
            separator(frame, r.x + r.width, r.y, r.height);
        }
    }
    if let Some(r) = rects.reader {
        reader_column(frame, r, state, &dir, images);
    }
}
```

`entry_column` and `reader_column` are written in 9a as the generic list and the special pages, and extended in 9b:

```rust
/// The entry column for any source (9b adds the Journal): `●` yellow when unread, the title,
/// `*` white when starred, black on white when selected.
pub fn entry_column(state: &ReaderState, width: usize, _local: DateTime<FixedOffset>) -> Vec<Line<'static>> {
    let selected = state.selected_entry();
    state
        .entries()
        .iter()
        .map(|e| entry_line(e.entry_id, e.title.as_deref(), e.is_read, e.is_starred, width, selected == Some(e.entry_id)))
        .collect()
}

pub fn entry_line(id: i64, title: Option<&str>, read: bool, starred: bool, width: usize, selected: bool) -> Line<'static> {
    let _ = id;
    let (fg, bg) = if selected { (Tt::Black, Tt::White) } else { (Tt::White, Tt::Black) };
    let marker_cells = 2.min(width);
    let star_cells = 2.min(width - marker_cells);
    row(vec![
        span(if read { "" } else { "●" }, marker_cells, if selected { fg } else { Tt::Yellow }, bg),
        span(title.unwrap_or("Untitled"), width - marker_cells - star_cells, fg, bg),
        span(if starred { " *" } else { "" }, star_cells, fg, bg),
    ])
}

/// The reader column: P100, P899, or the article (9b adds FIN DE L'EDITION and the images).
fn reader_column(frame: &mut Frame, area: Rect, state: &ReaderState, dir: &PageDirectory, _images: &ImageStore) {
    let width = area.width as usize;
    let lines = match state.antiope_reader_page() {
        ReaderPage::Sommaire => sommaire_lines(dir, width),
        ReaderPage::Guide => guide_lines(width),
        _ => article_lines(state, width),
    };
    paint(frame, area, lines);
}

/// The article in teletext: its rendered lines recoloured white on black, links cyan.
pub fn article_lines(state: &ReaderState, width: usize) -> Vec<Line<'static>> {
    let Some(article) = state.article() else { return vec![] };
    article
        .lines
        .iter()
        .map(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            row(vec![span(&text, width, Tt::White, Tt::Black)])
        })
        .collect()
}
```

`ReaderPage` (`Article`, `Sommaire`, `Guide`, `EndOfEdition`), `ReaderState::antiope_reader_page()`, `ReaderState::antiope_header_label(&PageDirectory) -> Option<HeaderLabel>`, `ReaderState::clock()` and `PageDirectory::page_of_current(&ReaderState) -> Option<u16>` are added in 9b. In 9a, `antiope_reader_page()` returns `ReaderPage::Article` unless the state's Antiope page is 100 or 899 (a `pub(crate) antiope_page: Option<u16>` field set by the test helper), `antiope_header_label` returns `None`, `clock()` returns the last `Event::Tick` (`now`, `local`), and `page_of_current` is `self.page_of(state.source())`. Put `ReaderPage` in `pages.rs`.

- [ ] **Step 4: Render by look** in `tui/reader/mod.rs`, where the frame is drawn:

```rust
terminal.draw(|f| match state.look() {
    Look::Native => native::render(f, &state, &images),
    Look::Antiope => antiope::render(f, &state, &images),
})?;
```

and, so the clock ticks and the counter rolls, the loop's tick interval: 1 s, or 100 ms while `state.progress().is_some()` and the look is Antiope (spec §8).

- [ ] **Step 5: Run the tests, review the snapshots**

```sh
cargo test -p ressac reader::antiope 2>&1 | tail -5
cargo insta review   # accept the three new snapshots after reading them
```

Expected snapshots: P100 with `RESSAC` centred on the top row of the reader column, the wave, the pages in two columns; at 70 columns the index column folds away (layout class One/Two) and the reader shows P100 alone; P899 with the five sections.

- [ ] **Step 6: Gate and commit**

```sh
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/src/tui/reader/antiope.rs crates/ressac/src/tui/reader/pages.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader, Antiope: page header with clock, Minitel bar, index rows, SOMMAIRE and GUIDE

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Sub-phase 9b: pages, P104, sub-pages, garden pages and images

**Files:**
- Modify: `crates/ressac/src/tui/reader/pages.rs` (`PageEntry`, `AntiopeNav`, `NavOutcome`, `ReaderPage`)
- Modify: `crates/ressac/src/tui/reader/antiope.rs` (P104 rows, sub-page label, `FIN DE L'EDITION`, rhythm and garden pages, images)
- Modify: `crates/ressac/src/tui/reader/state.rs` (route Antiope keys through `AntiopeNav`, expose `antiope_*` accessors, test helper)
- Modify: `crates/ressac/src/tui/reader/images.rs` (teletext mosaic mode next to the real image)

### Task 9b.1: page entry and navigation (pure)

- [ ] **Step 1: Write the failing tests** (append to `pages.rs` tests):

```rust
    use chrono::{Duration, TimeZone, Utc};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    fn t0() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 16, 16, 0, 0).unwrap()
    }
    fn ctx<'a>(dir: &'a PageDirectory) -> NavContext<'a> {
        NavContext { dir, now: t0(), in_text_input: false, journal_last: Some(43), on_journal_entry: None, total: 43 }
    }

    #[test]
    fn three_digits_jump_and_show_while_typing() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        assert_eq!(nav.on_key(key(KeyCode::Char('4')), &ctx(&dir)), NavOutcome::Consumed);
        assert_eq!(nav.typing(), Some("4__".to_string()));
        nav.on_key(key(KeyCode::Char('1')), &ctx(&dir));
        assert_eq!(nav.on_key(key(KeyCode::Char('2')), &ctx(&dir)), NavOutcome::Jump(PageDestination::Source(Source::Feed(12))));
        assert_eq!(nav.typing(), None);
    }

    #[test]
    fn backspace_corrects_and_escape_cancels() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        nav.on_key(key(KeyCode::Char('4')), &ctx(&dir));
        nav.on_key(key(KeyCode::Char('3')), &ctx(&dir));
        nav.on_key(key(KeyCode::Backspace), &ctx(&dir));
        assert_eq!(nav.typing(), Some("4__".to_string()));
        nav.on_key(key(KeyCode::Esc), &ctx(&dir));
        assert_eq!(nav.typing(), None);
    }

    #[test]
    fn an_unknown_page_is_not_found_for_two_seconds() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        for c in ['4', '3', '7'] {
            nav.on_key(key(KeyCode::Char(c)), &ctx(&dir));
        }
        assert_eq!(nav.not_found(t0() + Duration::milliseconds(1999)), Some(437));
        assert_eq!(nav.not_found(t0() + Duration::seconds(2)), None);
    }

    #[test]
    fn digits_in_a_text_input_are_typing() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        let mut c = ctx(&dir);
        c.in_text_input = true;
        assert_eq!(nav.on_key(key(KeyCode::Char('4')), &c), NavOutcome::Unhandled);
    }

    #[test]
    fn sommaire_and_guide_keys() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        assert_eq!(nav.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT), &ctx(&dir)), NavOutcome::Jump(PageDestination::Sommaire));
        assert_eq!(nav.on_key(key(KeyCode::Char('?')), &ctx(&dir)), NavOutcome::Jump(PageDestination::Guide));
    }

    #[test]
    fn suite_past_the_editions_last_entry_lands_on_the_end_page() {
        let dir = PageDirectory::new(&sample_pages(), None);
        let mut nav = AntiopeNav::default();
        let mut c = ctx(&dir);
        c.on_journal_entry = Some(43);
        assert_eq!(nav.on_key(key(KeyCode::Char('j')), &c), NavOutcome::EndOfEdition);
        assert_eq!(nav.reader_page(), ReaderPage::EndOfEdition);
        assert_eq!(nav.on_key(key(KeyCode::Char('j')), &c), NavOutcome::Consumed);
        assert_eq!(nav.on_key(key(KeyCode::Char('k')), &c), NavOutcome::BackToLastEntry);
        assert_eq!(nav.reader_page(), ReaderPage::Article);
    }

    #[test]
    fn the_sub_page_label_counts_by_rank() {
        assert_eq!(subpage_label(104, 3, 43), "104 3/43");
        assert_eq!(subpage_label(412, 1, 20), "412 1/20");
    }
```

- [ ] **Step 2: Run them to see them fail** (`cargo test -p ressac reader::pages`): `AntiopeNav`, `NavContext`, `NavOutcome`, `ReaderPage`, `subpage_label` missing.

- [ ] **Step 3: Write the navigation** (append to `pages.rs`):

```rust
use chrono::{DateTime, Duration, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the reader column shows in Antiope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderPage {
    #[default]
    Article,
    Sommaire,
    Guide,
    /// After P104's last entry: `FIN DE L'EDITION`.
    EndOfEdition,
}

/// How long an unknown page shows `NOT FOUND` (Antiope spec §6).
pub const NOT_FOUND_FOR: Duration = Duration::seconds(2);

/// What the state needs to know for one key.
pub struct NavContext<'a> {
    pub dir: &'a PageDirectory,
    pub now: DateTime<Utc>,
    /// The search prompt has the keyboard: every key is typing there.
    pub in_text_input: bool,
    /// The rank of the Journal's last entry, when the Journal is shown.
    pub journal_last: Option<u32>,
    /// The rank of the Journal entry being read, when the reader shows one.
    pub on_journal_entry: Option<u32>,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavOutcome {
    /// Not an Antiope key: the Native handling goes on.
    Unhandled,
    /// Handled, nothing else to do.
    Consumed,
    Jump(PageDestination),
    EndOfEdition,
    BackToLastEntry,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AntiopeNav {
    digits: String,
    not_found: Option<(u16, DateTime<Utc>)>,
    reader_page: ReaderPage,
}

impl AntiopeNav {
    pub fn typing(&self) -> Option<String> {
        (!self.digits.is_empty()).then(|| format!("{:_<3}", self.digits))
    }

    pub fn not_found(&self, now: DateTime<Utc>) -> Option<u16> {
        self.not_found.filter(|(_, at)| now - *at < NOT_FOUND_FOR).map(|(p, _)| p)
    }

    pub fn reader_page(&self) -> ReaderPage {
        self.reader_page
    }

    pub fn show(&mut self, page: ReaderPage) {
        self.reader_page = page;
    }

    pub fn on_key(&mut self, key: KeyEvent, ctx: &NavContext) -> NavOutcome {
        if ctx.in_text_input {
            return NavOutcome::Unhandled;
        }
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() && key.modifiers - KeyModifiers::SHIFT == KeyModifiers::NONE => {
                self.digits.push(c);
                if self.digits.len() < 3 {
                    return NavOutcome::Consumed;
                }
                let page: u16 = std::mem::take(&mut self.digits).parse().unwrap_or(0);
                match ctx.dir.destination(page) {
                    Some(d) => {
                        self.reader_page = match d {
                            PageDestination::Sommaire => ReaderPage::Sommaire,
                            PageDestination::Guide => ReaderPage::Guide,
                            _ => ReaderPage::Article,
                        };
                        NavOutcome::Jump(d)
                    }
                    None => {
                        self.not_found = Some((page, ctx.now));
                        NavOutcome::Consumed
                    }
                }
            }
            KeyCode::Backspace if !self.digits.is_empty() => {
                self.digits.pop();
                NavOutcome::Consumed
            }
            KeyCode::Esc if !self.digits.is_empty() => {
                self.digits.clear();
                NavOutcome::Consumed
            }
            KeyCode::Char('S') => {
                self.reader_page = ReaderPage::Sommaire;
                NavOutcome::Jump(PageDestination::Sommaire)
            }
            KeyCode::Char('?') => {
                self.reader_page = ReaderPage::Guide;
                NavOutcome::Jump(PageDestination::Guide)
            }
            KeyCode::Char('j') | KeyCode::Char(' ')
                if self.reader_page == ReaderPage::EndOfEdition => NavOutcome::Consumed,
            KeyCode::Char('j') if ctx.on_journal_entry.is_some() && ctx.on_journal_entry == ctx.journal_last => {
                self.reader_page = ReaderPage::EndOfEdition;
                NavOutcome::EndOfEdition
            }
            KeyCode::Char('k') if self.reader_page == ReaderPage::EndOfEdition => {
                self.reader_page = ReaderPage::Article;
                NavOutcome::BackToLastEntry
            }
            _ => NavOutcome::Unhandled,
        }
    }
}

/// `104 3/43`: an entry is a sub-page of its source's page (Antiope spec §6).
pub fn subpage_label(page: u16, position: usize, total: usize) -> String {
    format!("{page} {position}/{total}")
}
```

Space on the last Journal entry first scrolls the article (phase 4b's `Space` logic); when the article cannot scroll further, `state.rs` asks `AntiopeNav` with `KeyCode::Char('j')` semantics (Task 9b.2), as the app does.

- [ ] **Step 4: Run the tests** (`cargo test -p ressac reader::pages`): pass.

- [ ] **Step 5: Commit**

```sh
git add crates/ressac/src/tui/reader/pages.rs
git commit -m "Reader, Antiope: page entry, NOT FOUND, SUITE to FIN DE L'EDITION, sub-page label

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9b.2: wire `AntiopeNav` into the state

- [ ] **Step 1: Write the failing tests** in `state.rs`'s test module (it already has helpers from phases 4b–8 to build a state with sources, pages, a Journal and entries; use them, and add `antiope_state_on_page`):

```rust
    pub(crate) fn antiope_state_on_page(page: u16) -> ReaderState {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Antiope, images: false }, 140, 34);
        s.on_event(Event::Sources(sample_sources()));
        s.set_pages(crate::tui::reader::pages::tests::sample_pages());
        s.on_event(Event::Journal(Some(sample_journal())));
        for c in page.to_string().chars() {
            s.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        s
    }

    #[test]
    fn typing_412_in_antiope_opens_the_feed() {
        let mut s = antiope_state_on_page(104);
        let cmds = type_page(&mut s, "412");
        assert_eq!(s.source(), Source::Feed(12));
        assert!(cmds.iter().any(|c| matches!(c, Command::LoadEntries { source: Source::Feed(12), .. })));
    }

    #[test]
    fn digits_do_nothing_in_native() {
        let mut s = native_state();
        let before = s.source();
        type_page(&mut s, "412");
        assert_eq!(s.source(), before);
    }

    #[test]
    fn typing_801_opens_the_garden() {
        let mut s = antiope_state_on_page(104);
        type_page(&mut s, "801");
        assert!(s.garden().is_some() || s.garden_view() == Some(GardenView::AllFeeds));
    }

    #[test]
    fn suite_walks_the_journal_by_rank_then_reaches_fin_de_l_edition() {
        let mut s = antiope_state_on_page(104);
        let last = sample_journal().items.iter().max_by_key(|i| i.rank).unwrap().row.entry_id;
        s.select_entry(last);
        s.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(s.antiope_reader_page(), ReaderPage::EndOfEdition);
        s.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(s.antiope_reader_page(), ReaderPage::Article);
        assert_eq!(s.selected_entry(), Some(last));
    }

    #[test]
    fn not_found_clears_on_a_later_tick() {
        let mut s = antiope_state_on_page(104);
        type_page(&mut s, "437");
        let dir = PageDirectory::new(s.pages(), s.sources());
        assert_eq!(s.antiope_header_label(&dir), Some(HeaderLabel::NotFound(437)));
        s.on_event(tick_after_seconds(2));
        assert_ne!(s.antiope_header_label(&dir), Some(HeaderLabel::NotFound(437)));
    }

    #[test]
    fn the_look_switch_keeps_the_selection() {
        let mut s = antiope_state_on_page(104);
        let first = s.selected_entry();
        s.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(s.look(), Look::Native);
        assert_eq!(s.selected_entry(), first);
    }
```

(`type_page`, `native_state`, `sample_sources`, `sample_journal`, `tick_after_seconds`, `set_pages`, `pending_garden` come from phases 4b–8; write the missing ones as small helpers in the test module. If phase 8 opens the garden synchronously, keep only `s.garden().is_some()`.)

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement in `state.rs`** (field `antiope: AntiopeNav`, set from `Look` and reset to `ReaderPage::Article` on a look switch):

```rust
    // At the top of `on_key`, before the Native mapping:
    if self.look() == Look::Antiope {
        let dir = PageDirectory::new(self.pages(), self.sources());
        let journal_last = self.journal().and_then(|j| j.items.iter().map(|i| i.rank).max());
        let on_journal_entry = (self.source() == Source::Journal)
            .then(|| self.selected_entry())
            .flatten()
            .and_then(|id| self.journal().and_then(|j| j.items.iter().find(|i| i.row.entry_id == id)).map(|i| i.rank));
        let ctx = NavContext {
            dir: &dir,
            now: self.clock().0,
            in_text_input: self.is_searching(),
            journal_last,
            on_journal_entry,
            total: self.entries().len(),
        };
        match self.antiope.on_key(key, &ctx) {
            NavOutcome::Unhandled => {}
            NavOutcome::Consumed | NavOutcome::EndOfEdition | NavOutcome::BackToLastEntry => return vec![],
            NavOutcome::Jump(PageDestination::Source(source)) => return self.go_to(source),
            NavOutcome::Jump(PageDestination::Garden(view)) => return self.go_to_garden(view),
            NavOutcome::Jump(PageDestination::Sommaire | PageDestination::Guide) => {
                self.set_focus(Column::Reader);
                return vec![];
            }
        }
    }

    pub fn antiope_reader_page(&self) -> ReaderPage {
        self.antiope.reader_page()
    }

    /// The header's label: typing first, then `NOT FOUND`, else `None` (the renderer rolls the
    /// counter during a job, or shows the current page).
    pub fn antiope_header_label(&self, dir: &PageDirectory) -> Option<HeaderLabel> {
        if let Some(t) = self.antiope.typing() {
            return Some(HeaderLabel::Typing(t.trim_start_matches('P').to_string()));
        }
        if let Some(p) = self.antiope.not_found(self.clock().0) {
            return Some(HeaderLabel::NotFound(p));
        }
        let page = match self.antiope.reader_page() {
            ReaderPage::Sommaire => Some(100),
            ReaderPage::Guide => Some(899),
            _ => self.garden_view().map(|v| dir.page_of_garden(v)).or_else(|| dir.page_of(self.source())),
        };
        (self.progress().is_none()).then_some(HeaderLabel::Page(page))
    }
```

`RÉPÉTITION` (`r`) in Antiope: when `journal_phase()` is `Offline`, `Failed(_)` or `Due` and the source is the Journal, emit `Command::BuildEdition`; else the Native refresh of the source. The `t` key keeps the selection (phase 4b already keeps it; the test pins it for Antiope → Native).

- [ ] **Step 4: Run the tests, then commit**

```sh
cargo test -p ressac reader::state 2>&1 | tail -5
git add crates/ressac/src/tui/reader/state.rs
git commit -m "Reader, Antiope: page numbers, SOMMAIRE, GUIDE and the end of the edition in the state

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9b.3: P104, the rhythm and garden pages, the sub-page line

- [ ] **Step 1: Write the failing tests** (append to `antiope.rs` tests):

```rust
    fn journal() -> ressac_core::store::JournalView {
        crate::tui::reader::state::tests::sample_journal()
    }

    #[test]
    fn p104_has_coloured_sections_and_white_on_blue_overflow() {
        let lines = journal_lines(Some(&journal()), &P104Status::Ready, None, 28, local(8, 0, 0));
        let all: Vec<String> = lines.iter().map(text).collect();
        let front = lines.iter().find(|l| text(l).contains("FRONT PAGE"));
        if let Some(front) = front {
            assert_eq!(front.spans[0].style.fg, Some(Tt::Yellow.color()));
        }
        let wire = lines.iter().find(|l| text(l).contains("THE WIRE")).unwrap();
        assert_eq!(wire.spans[0].style.fg, Some(Tt::Green.color()));
        let more = lines.iter().find(|l| text(l).contains("MORE IN")).unwrap();
        assert_eq!(more.spans[1].style.bg, Some(Tt::Blue.color()));
        assert_eq!(more.spans[1].style.fg, Some(Tt::White.color()));
        assert!(all.iter().all(|t| Span::raw(t.clone()).width() == 28));
    }

    #[test]
    fn p104_while_building_offline_failed_or_empty() {
        let building = journal_lines(None, &P104Status::Building { done: 34, total: 156 }, None, 28, local(8, 0, 0));
        assert!(text(&building[0]).contains("EDITION IN PROGRESS 34/156"));
        let offline = journal_lines(None, &P104Status::Offline, None, 28, local(8, 0, 0));
        assert!(text(&offline[0]).contains("OFFLINE: NO EDITION YET"));
        assert!(offline.iter().any(|l| text(l).contains("RÉPÉTITION: RETRY")));
        let none = journal_lines(None, &P104Status::Ready, None, 28, local(8, 0, 0));
        assert!(text(&none[0]).contains("EDITION IN PROGRESS"));
    }

    #[test]
    fn fin_de_l_edition_closes_a_read_edition() {
        let mut j = journal();
        for item in &mut j.items {
            item.row.is_read = true;
        }
        let lines = journal_lines(Some(&j), &P104Status::Ready, None, 28, local(8, 0, 0));
        let last = lines.last().unwrap();
        assert!(text(last).contains("FIN DE L'EDITION"));
        assert_eq!(last.spans[1].style.bg, Some(Tt::Yellow.color()));
    }

    #[test]
    fn the_header_counts_read_entries() {
        let lines = journal_lines(Some(&journal()), &P104Status::Ready, None, 28, local(8, 0, 0));
        assert!(text(&lines[0]).ends_with(" READ"));
        assert!(text(&lines[0]).starts_with(" FRI 16 OCT") || text(&lines[0]).starts_with(" "));
    }

    #[test]
    fn garden_headings_use_the_status_colours_and_fossil_is_white_on_blue() {
        let dead = garden_heading("Dead", 3, 28);
        assert_eq!(dead.spans[0].style.fg, Some(Tt::Red.color()));
        let fossil = garden_heading("Fossil", 2, 28);
        assert_eq!(fossil.spans[0].style.bg, Some(Tt::Blue.color()));
        assert_eq!(fossil.spans[0].style.fg, Some(Tt::White.color()));
    }

    #[test]
    fn antiope_journal_at_140_and_70_columns() {
        let state = crate::tui::reader::state::tests::antiope_state_on_page(104);
        insta::assert_snapshot!("antiope_journal_140", draw(&state, 140, 34));
        insta::assert_snapshot!("antiope_journal_70", draw(&state, 70, 34));
    }

    #[test]
    fn antiope_end_of_edition_page() {
        let mut state = crate::tui::reader::state::tests::antiope_state_on_page(104);
        crate::tui::reader::state::tests::read_to_the_end(&mut state);
        insta::assert_snapshot!("antiope_fin_140", draw(&state, 140, 34));
    }
```

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement** (append to `antiope.rs`):

```rust
use ressac_core::edition::Section;
use ressac_core::store::{EditionOverflowRow, JournalItem, JournalView};

/// What P104 shows besides the edition (phase 7's `JournalPhase` mapped by `render`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P104Status {
    Ready,
    Building { done: usize, total: usize },
    Offline,
    Failed(String),
}

pub fn section_colour(section: Section) -> Tt {
    match section {
        Section::Front => Tt::Yellow,
        Section::Week => Tt::Cyan,
        Section::Wire => Tt::Green,
    }
}

pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Front => "FRONT PAGE",
        Section::Week => "THIS WEEK",
        Section::Wire => "THE WIRE",
    }
}

pub fn progress_text(done: usize, total: usize) -> String {
    if total > 0 { format!("EDITION IN PROGRESS {done}/{total}") } else { "EDITION IN PROGRESS".into() }
}

/// P104's entry column (spec §8): while building or offline, why there is no edition; else the
/// header, each section with entries in rank order and each feed's overflow row under its last
/// entry, then `FIN DE L'EDITION` once every entry is read. Every row is exactly `width` cells.
pub fn journal_lines(
    journal: Option<&JournalView>,
    status: &P104Status,
    selected: Option<i64>,
    width: usize,
    local: DateTime<FixedOffset>,
) -> Vec<Line<'static>> {
    match status {
        P104Status::Building { done, total } => return vec![centred(&progress_text(*done, *total), width, Tt::Yellow, Tt::Black)],
        P104Status::Offline => {
            return vec![
                centred("OFFLINE: NO EDITION YET", width, Tt::Black, Tt::Red),
                centred("RÉPÉTITION: RETRY", width, Tt::Cyan, Tt::Black),
            ]
        }
        P104Status::Failed(reason) => return vec![centred(&reason.to_uppercase(), width, Tt::Black, Tt::Red)],
        P104Status::Ready => {}
    }
    let Some(j) = journal else {
        return vec![centred(&progress_text(0, 0), width, Tt::Yellow, Tt::Black)];
    };
    let read = j.items.iter().filter(|i| i.row.is_read).count();
    let count = format!(" {read}/{} READ", j.items.len());
    let count_cells = count.len().min(width);
    let day = local.format(" %a %d %b").to_string().to_uppercase();
    let mut lines = vec![row(vec![
        span(&day, width - count_cells, Tt::Cyan, Tt::Black),
        span(&count, count_cells, Tt::Yellow, Tt::Black),
    ])];
    let unreachable = j.edition.unreachable;
    if unreachable > 0 {
        let t = if unreachable == 1 { "1 FEED UNREACHABLE".to_string() } else { format!("{unreachable} FEEDS UNREACHABLE") };
        lines.push(row(vec![span(&format!(" {t}"), width, Tt::Red, Tt::Black)]));
    }
    if j.items.is_empty() {
        lines.push(blank(width));
        lines.push(centred("NOTHING NEW THIS MORNING.", width, Tt::Cyan, Tt::Black));
        return lines;
    }
    let mut items: Vec<&JournalItem> = j.items.iter().collect();
    items.sort_by_key(|i| i.rank);
    for section in [Section::Front, Section::Week, Section::Wire] {
        let entries: Vec<&&JournalItem> = items.iter().filter(|i| i.section == section).collect();
        let overflow: Vec<&EditionOverflowRow> = j.overflow.iter().filter(|o| o.section == section).collect();
        if entries.is_empty() && overflow.is_empty() {
            continue;
        }
        lines.push(blank(width));
        let colour = section_colour(section);
        let number = format!(" {}", entries.len());
        let number_cells = number.len().min(width);
        lines.push(row(vec![
            span(&format!(" {}", section_title(section)), width - number_cells, colour, Tt::Black),
            span(&number, number_cells, colour, Tt::Black),
        ]));
        let mut placed = std::collections::HashSet::new();
        for (index, item) in entries.iter().enumerate() {
            let r = &item.row;
            lines.push(entry_line(r.entry_id, r.title.as_deref(), r.is_read, r.is_starred, width, selected == Some(r.entry_id)));
            let last_of_feed = entries.get(index + 1).map(|n| n.row.feed_id != r.feed_id).unwrap_or(true);
            if last_of_feed && placed.insert(r.feed_id) {
                if let Some(more) = overflow.iter().find(|o| o.feed_id == r.feed_id) {
                    lines.push(overflow_line(more, width));
                }
            }
        }
        for more in overflow.iter().filter(|o| !placed.contains(&o.feed_id)) {
            lines.push(overflow_line(more, width));
        }
    }
    if items.iter().all(|i| i.row.is_read) {
        lines.push(blank(width));
        lines.push(end_line(width));
    }
    lines
}

/// `  +12 MORE IN ACTUALITTÉ`: white on blue (blue never colours text), two cells in.
pub fn overflow_line(more: &EditionOverflowRow, width: usize) -> Line<'static> {
    let indent = 2.min(width);
    row(vec![
        span("", indent, Tt::White, Tt::Black),
        span(&format!("+{} MORE IN {}", more.hidden_count, more.feed_title.to_uppercase()), width - indent, Tt::White, Tt::Blue),
    ])
}

/// `FIN DE L'EDITION` on a full-width yellow bar (the terminal's double height).
pub fn end_line(width: usize) -> Line<'static> {
    title_bar("FIN DE L'EDITION", width)
}

/// A garden status heading in its teletext colour (Fossil white on blue).
pub fn garden_heading(status: &str, count: usize, width: usize) -> Line<'static> {
    let colour = match status {
        "Dead" => Tt::Red,
        "Hijacked" => Tt::Magenta,
        "Moved" => Tt::Cyan,
        "Ailing" => Tt::Yellow,
        "Fossil" => Tt::Blue,
        "Dormant" | "Alive" => Tt::Green,
        _ => Tt::White,
    };
    let (fg, bg) = if colour == Tt::Blue { (Tt::White, Tt::Blue) } else { (colour, Tt::Black) };
    let number = format!(" {count}");
    let number_cells = number.len().min(width);
    row(vec![
        span(&format!(" {}", status.to_uppercase()), width - number_cells, fg, bg),
        span(&number, number_cells, fg, bg),
    ])
}

/// P800–P802: the garden's rows grouped by status in urgency order, each heading coloured.
pub fn garden_lines(rows: &[ressac_core::garden::GardenRow], selected: Option<i64>, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for status in ressac_core::model::Status::ALL {
        let group: Vec<_> = rows.iter().filter(|r| r.status() == status).collect();
        if group.is_empty() {
            continue;
        }
        lines.push(garden_heading(status.as_str(), group.len(), width));
        for r in group {
            lines.push(entry_line(r.feed_id, Some(&r.title), true, false, width, selected == Some(r.feed_id)));
        }
    }
    lines
}
```

Extend `entry_column` (9a) so that the Journal source draws `journal_lines(state.journal(), &p104_status(state), state.selected_entry(), width, local)` and a garden view draws `garden_lines(&garden.rows, garden.selected_row().map(|r| r.feed_id), width)`; P105–P109 are ordinary sources (`Source::Cadence`) and use the generic list. Map phase 7's phase:

```rust
fn p104_status(state: &ReaderState) -> P104Status {
    match state.journal_phase() {
        JournalPhase::Building { done, total } => P104Status::Building { done, total },
        JournalPhase::Offline => P104Status::Offline,
        JournalPhase::Failed(reason) => P104Status::Failed(reason.clone()),
        _ => P104Status::Ready,
    }
}
```

In `reader_column`, `ReaderPage::EndOfEdition` paints `vec![blank(width), end_line(width)]`; for an article, the first row is the sub-page line in cyan: `subpage_label(page, position, total)` where `page = dir.page_of(state.source())` (or `104` for the Journal), `position` is the entry's rank for the Journal and its 1-based index in `state.entries()` otherwise, `total` the edition's item count or the list's length.

- [ ] **Step 4: Run the tests, review snapshots, gate, commit**

```sh
cargo test -p ressac reader::antiope && cargo insta review
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/src/tui/reader/antiope.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader, Antiope: P104 JOURNAL, sub-pages, FIN DE L'EDITION, rhythm and garden pages

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9b.4: Antiope images

- [ ] **Step 1: Write the failing test** in `images.rs`'s tests (phase 6 owns `quantise_teletext` and its tests; this adds the mode):

```rust
    #[test]
    fn antiope_mode_uses_the_teletext_mosaic_unless_real_images_are_asked() {
        let mut store = ImageStore::disabled();
        assert_eq!(store.mode(), ImageMode::Native);
        store.set_mode(ImageMode::TeletextMosaic);
        assert_eq!(store.mode(), ImageMode::TeletextMosaic);
        store.toggle_teletext_real();
        assert_eq!(store.mode(), ImageMode::TeletextReal);
        store.toggle_teletext_real();
        assert_eq!(store.mode(), ImageMode::TeletextMosaic);
    }

    #[test]
    fn a_mosaic_is_drawn_with_half_blocks_in_the_eight_colours() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 20])));
        let q = prepare_for_mode(&img, ImageMode::TeletextMosaic);
        assert!(q.to_rgb8().pixels().all(|p| TELETEXT.contains(&[p[0], p[1], p[2]])));
        assert_eq!(mosaic_picker().protocol_type(), ratatui_image::picker::ProtocolType::Halfblocks);
    }
```

- [ ] **Step 2: Run it to see it fail.**

- [ ] **Step 3: Implement** in `images.rs`:

```rust
/// How images are drawn: Native uses the detected protocol; Antiope draws teletext mosaics
/// (half-blocks, the eight colours) and `i` swaps in the real image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageMode {
    #[default]
    Native,
    TeletextMosaic,
    TeletextReal,
}

/// Half-blocks, whatever the terminal supports: the mosaic must look the same everywhere.
pub fn mosaic_picker() -> ratatui_image::picker::Picker {
    ratatui_image::picker::Picker::halfblocks()
}

/// The image as it will be encoded for `mode`.
pub fn prepare_for_mode(img: &image::DynamicImage, mode: ImageMode) -> image::DynamicImage {
    match mode {
        ImageMode::TeletextMosaic => quantise_teletext(img),
        _ => img.clone(),
    }
}
```

and in `ImageStore`: a `mode: ImageMode` field, `mode()`, `set_mode()`, `toggle_teletext_real()`; the cache key becomes `(url, mode)`; in `TeletextMosaic` the protocol is built with `mosaic_picker()` from `prepare_for_mode(...)`, in the other modes with the detected picker as before. `ReaderState` sets `TeletextMosaic` when the look switches to Antiope and `Native` when it switches back; in Antiope, `i` calls `toggle_teletext_real()` (instead of images on/off).

- [ ] **Step 4: Gate and commit**

```sh
cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/src/tui/reader/images.rs crates/ressac/src/tui/reader/state.rs
git commit -m "Reader, Antiope: teletext mosaics for images, i for the original

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 9b

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
```

Then, with a throwaway database (never `~/Library`): `HOME=$(mktemp -d) cargo run -p ressac -- --db /tmp/…/r.sqlite` after an `import` of `tests/fixtures/vienna-like.opml`, press `t`, type `100`, `899`, `104`, `801`: the screens match the snapshots. A look at real images in Antiope is part of phase 10's manual check.
