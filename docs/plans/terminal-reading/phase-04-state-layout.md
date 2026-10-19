# Phase 4: adaptive columns, the terminal settings, the reader state and its keys

**Runs after:** phase 3 (`tui/reader/html.rs`), and phase 1 (the core reading API: `Source`, `ListFilter`, `SourcesSnapshot`, `EntryRow`, `EntryPage`, `ListCursor`, `JournalView`, `EntryDetail`).

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §5 (state and keys, the look setting) and §6 "Columns", and owns **Review Focus 3** (resize 140 → 70 → 140 with an article open) plus the state half of **Review Focus 2** (a reload keeps the selected entry).

Two sub-phases, each ending with the Rust gate:

| Sub-phase | Files | Deliverable |
|---|---|---|
| 4a | `crates/ressac/Cargo.toml`, `tui/reader/mod.rs`, `tui/reader/layout.rs` (new), `tui/reader/settings.rs` (new) | `Look`, `Column`, `LayoutClass`, `layout_class`, `ColumnRects`, `columns`, `text_width`, `text_height`; `TerminalSettings` load/save/default path |
| 4b | `tui/reader/mod.rs`, `tui/reader/sidebar.rs` (new), `tui/reader/state.rs` (new) | `SidebarRow` and the flat sources list; `ReaderState`, `Command`, `Event`, `JobOutcome`, every Native key of spec §5 |

**Builds on (exact):**
- Phase 3: `crate::tui::reader::html::{render_html, RenderedArticle, is_web_url}`; the module root `tui/reader/mod.rs` carries `#![cfg_attr(not(test), allow(dead_code, …))]` until phase 5b.
- Phase 1 (plan index, "Phase 1"): `ressac_core::store::{Source, ListFilter, SourceCounts, SourceNode, SourceNodeKind, SourcesSnapshot, EntryRow, EntryPage, ListCursor, JournalView, EntryDetail}` with exactly the fields of the plan index. `Source` and `ListCursor` derive `PartialEq` (they appear in commands compared by tests).
- `ressac_core::model::{CadenceGroup, FeedId, FolderId}` (`FeedId`/`FolderId` are `i64`; `CadenceGroup::ALL: [CadenceGroup; 5]` = Daily, Weekly, Monthly, Rare, Silent).
- `ressac_core::store::default_path()` (`$HOME/Library/Application Support/Ressac/ressac.sqlite`).
- `crate::tui::state::Command` (the garden's command enum, `Debug + Clone + PartialEq + Eq`), carried by `Command::Garden` for phase 8.
- The garden's key pattern (`GardenState::on_key(KeyEvent) -> Command`, Ctrl-C quits, pure state, `KeyEvent::new(code, KeyModifiers::NONE)` in tests).

**Deviations from the plan index (reported, all additive):**
1. `Look` and `Column` are defined in `layout.rs` (the index lists them under phase 4 without a file); `Look` derives `Serialize`/`Deserialize` (`"native"`/`"antiope"`) for the settings file.
2. `layout.rs` adds `text_width(rect, look)` / `text_height(rect, look)` (Native draws a border, Antiope does not), so the state and the renderers agree on the text area.
3. `ReaderState`'s fields are `pub`, like `GardenState`'s, so the renderers (phases 5, 9) read them directly; the index's accessors (`look`, `focus`, `source`, `selected_entry`) exist too, plus `class()` and `start()` (the first commands: sources, Journal, its entries).
4. A new pure module `sidebar.rs` (`SidebarRow`, `rows`, `index_of`, `step`) turns `SourcesSnapshot` into the sources column; the garden rows (Needs triage, All feeds, Cemetery) are added by phase 8.
5. `Event::Entry` carries `Box<EntryDetail>`: `EntryDetail` holds four optional HTML strings and an `EntryRow`, which makes the variant far larger than the others (`clippy::large_enum_variant`, warn by default, fails `-D warnings`).
6. `Command` derives `Debug, Clone, PartialEq`; `Event` and `JobOutcome` derive `Debug, Clone` (`JobOutcome` also `PartialEq`).
7. The Journal's overflow rows (`Enter` opens the feed) belong to phase 7, which owns the Journal view; phase 4 navigates entries only. The Antiope keys are phase 9's.

**Exact strings introduced here (English, spec §5–§6):** `That's the edition.`, `No more unread entries here.`, `No entry selected.`, `This entry has no link.`, `No feed selected.`, `No link [9].` (number varies), `Fetching the full article…`, `Offline: no edition yet`, `Open: Enter for the article, 1-9 for a link, Esc to cancel.`, sources `Journal`, `Unread`, `Today`, `Starred`, headings `Feeds`, `Rhythm`.

---

## Sub-phase 4a: columns and settings

### Task 4a.1: `layout.rs`

- [ ] **Step 1: Dependency and module.** In `crates/ressac/Cargo.toml`, under `[dependencies]`, after `scraper.workspace = true` (added in phase 3):

```toml
serde.workspace = true
```

(`serde = { version = "1.0.229", features = ["derive"] }` is already a workspace dependency.) In `crates/ressac/src/tui/reader/mod.rs`, after `pub mod html;`:

```rust
pub mod layout;
pub mod settings;
```

- [ ] **Step 2: Write `layout.rs` with the tests first** (the functions return placeholders so the crate compiles and the tests fail). Create `crates/ressac/src/tui/reader/layout.rs`:

```rust
//! The reader's columns for the pane's size (spec §6, Columns): three when wide, two in the
//! middle, one when narrow. Pure: a size in, rectangles out.

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

/// The two looks of the reader (spec §2, Looks). Stored as `"native"` / `"antiope"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    #[default]
    Native,
    Antiope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Sources,
    Entries,
    Reader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutClass {
    Three,
    Two,
    One,
    TooSmall,
}

pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 10;
pub const TWO_COLUMNS_FROM: u16 = 80;
pub const THREE_COLUMNS_FROM: u16 = 120;

pub fn layout_class(_width: u16, _height: u16) -> LayoutClass {
    LayoutClass::TooSmall
}

/// Where each visible column goes; `top` and `bottom` are the status lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnRects {
    pub sources: Option<Rect>,
    pub entries: Option<Rect>,
    pub reader: Option<Rect>,
    pub top: Rect,
    pub bottom: Rect,
}

pub fn columns(area: Rect, _class: LayoutClass, _focus: Column, _look: Look) -> ColumnRects {
    ColumnRects { sources: None, entries: None, reader: None, top: area, bottom: area }
}

/// The text area inside a column: Native draws a one-cell border, Antiope none.
pub fn text_width(rect: Rect, _look: Look) -> u16 {
    rect.width
}

pub fn text_height(rect: Rect, _look: Look) -> u16 {
    rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn the_class_follows_the_pane_size() {
        assert_eq!(layout_class(39, 30), LayoutClass::TooSmall);
        assert_eq!(layout_class(140, 9), LayoutClass::TooSmall);
        assert_eq!(layout_class(40, 10), LayoutClass::One);
        assert_eq!(layout_class(79, 24), LayoutClass::One);
        assert_eq!(layout_class(80, 24), LayoutClass::Two);
        assert_eq!(layout_class(119, 24), LayoutClass::Two);
        assert_eq!(layout_class(120, 24), LayoutClass::Three);
    }

    #[test]
    fn three_native_columns_and_the_status_lines() {
        let r = columns(rect(0, 0, 140, 30), LayoutClass::Three, Column::Entries, Look::Native);
        assert_eq!(r.top, rect(0, 0, 140, 1));
        assert_eq!(r.bottom, rect(0, 29, 140, 1));
        assert_eq!(r.sources, Some(rect(0, 1, 26, 28)));
        assert_eq!(r.entries, Some(rect(26, 1, 40, 28)));
        assert_eq!(r.reader, Some(rect(66, 1, 74, 28)));
    }

    #[test]
    fn three_antiope_columns_leave_a_separator_cell() {
        let r = columns(rect(0, 0, 140, 30), LayoutClass::Three, Column::Reader, Look::Antiope);
        assert_eq!(r.sources, Some(rect(0, 1, 22, 28)));
        assert_eq!(r.entries, Some(rect(23, 1, 28, 28)));
        assert_eq!(r.reader, Some(rect(52, 1, 88, 28)));
    }

    #[test]
    fn two_columns_show_the_sources_over_the_list_when_focused() {
        let r = columns(rect(0, 0, 100, 30), LayoutClass::Two, Column::Entries, Look::Native);
        assert_eq!(r.sources, None);
        assert_eq!(r.entries, Some(rect(0, 1, 40, 28)));
        assert_eq!(r.reader, Some(rect(40, 1, 60, 28)));
        let r = columns(rect(0, 0, 100, 30), LayoutClass::Two, Column::Sources, Look::Native);
        assert_eq!(r.sources, Some(rect(0, 1, 26, 28)));
        assert_eq!(r.entries, Some(rect(0, 1, 40, 28)));
    }

    #[test]
    fn one_column_is_the_focused_one() {
        let area = rect(0, 0, 70, 24);
        let body = Some(rect(0, 1, 70, 22));
        let r = columns(area, LayoutClass::One, Column::Reader, Look::Native);
        assert_eq!((r.sources, r.entries, r.reader), (None, None, body));
        let r = columns(area, LayoutClass::One, Column::Sources, Look::Native);
        assert_eq!((r.sources, r.entries, r.reader), (body, None, None));
    }

    #[test]
    fn too_small_shows_no_column() {
        let r = columns(rect(0, 0, 30, 8), LayoutClass::TooSmall, Column::Entries, Look::Native);
        assert_eq!((r.sources, r.entries, r.reader), (None, None, None));
    }

    #[test]
    fn a_narrow_pane_never_overflows() {
        let r = columns(rect(0, 0, 120, 30), LayoutClass::Three, Column::Entries, Look::Native);
        let reader = r.reader.unwrap();
        assert_eq!(reader.x + reader.width, 120);
    }

    #[test]
    fn the_text_area_is_inside_the_native_border() {
        let column = rect(66, 1, 74, 28);
        assert_eq!(text_width(column, Look::Native), 72);
        assert_eq!(text_height(column, Look::Native), 26);
        assert_eq!(text_width(column, Look::Antiope), 74);
        assert_eq!(text_height(column, Look::Antiope), 28);
    }
}
```

- [ ] **Step 3: Run and see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo test -p ressac tui::reader::layout
```

Expected: every test fails on the placeholder values (e.g. `left: TooSmall, right: One`).

- [ ] **Step 4: Implement.** Replace the three placeholder functions and `text_width` / `text_height` with:

```rust
pub fn layout_class(width: u16, height: u16) -> LayoutClass {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        LayoutClass::TooSmall
    } else if width >= THREE_COLUMNS_FROM {
        LayoutClass::Three
    } else if width >= TWO_COLUMNS_FROM {
        LayoutClass::Two
    } else {
        LayoutClass::One
    }
}

/// Column widths in cells: Native (spec §6) and Antiope (the app's cell widths, spec §8), with
/// the Antiope `▐` separator cell between columns.
struct Widths {
    sources: u16,
    entries: u16,
    gap: u16,
}

fn widths(look: Look) -> Widths {
    match look {
        Look::Native => Widths { sources: 26, entries: 40, gap: 0 },
        Look::Antiope => Widths { sources: 22, entries: 28, gap: 1 },
    }
}

pub fn columns(area: Rect, class: LayoutClass, focus: Column, look: Look) -> ColumnRects {
    let top = Rect { height: area.height.min(1), ..area };
    let bottom = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: area.height.min(1),
        ..area
    };
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    let w = widths(look);
    let (sources, entries, reader) = match class {
        LayoutClass::TooSmall => (None, None, None),
        LayoutClass::Three => (
            Some(slice(body, 0, w.sources)),
            Some(slice(body, w.sources + w.gap, w.entries)),
            Some(rest(body, w.sources + w.gap + w.entries + w.gap)),
        ),
        LayoutClass::Two => (
            (focus == Column::Sources).then(|| slice(body, 0, w.sources)),
            Some(slice(body, 0, w.entries)),
            Some(rest(body, w.entries + w.gap)),
        ),
        LayoutClass::One => match focus {
            Column::Sources => (Some(body), None, None),
            Column::Entries => (None, Some(body), None),
            Column::Reader => (None, None, Some(body)),
        },
    };
    ColumnRects { sources, entries, reader, top, bottom }
}

fn slice(body: Rect, offset: u16, width: u16) -> Rect {
    let offset = offset.min(body.width);
    Rect {
        x: body.x + offset,
        width: width.min(body.width - offset),
        ..body
    }
}

fn rest(body: Rect, offset: u16) -> Rect {
    let offset = offset.min(body.width);
    Rect {
        x: body.x + offset,
        width: body.width - offset,
        ..body
    }
}

/// The text area inside a column: Native draws a one-cell border, Antiope none.
pub fn text_width(rect: Rect, look: Look) -> u16 {
    match look {
        Look::Native => rect.width.saturating_sub(2),
        Look::Antiope => rect.width,
    }
}

pub fn text_height(rect: Rect, look: Look) -> u16 {
    match look {
        Look::Native => rect.height.saturating_sub(2),
        Look::Antiope => rect.height,
    }
}
```

- [ ] **Step 5: Run.** `cargo test -p ressac tui::reader::layout` → 8 passed.

### Task 4a.2: `settings.rs`

- [ ] **Step 1: Write the module with its tests.** Create `crates/ressac/src/tui/reader/settings.rs` (the tests are written against the final code; run them once with `load` returning `Self::default()` unconditionally to see `round_trip` and `a_partial_file_keeps_the_other_defaults` fail, then put the real body back):

```rust
//! The terminal's own settings (spec §5, The look setting): the look and whether images show,
//! in `terminal.json` next to the database. A missing or unreadable file means the defaults;
//! saving never blocks reading (the caller ignores a failed save, beyond a message).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::layout::Look;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub look: Look,
    pub images: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        TerminalSettings { look: Look::Native, images: true }
    }
}

impl TerminalSettings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Written to a temporary file then renamed, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    /// `~/Library/Application Support/Ressac/terminal.json`, next to the default database.
    pub fn default_path() -> PathBuf {
        ressac_core::store::default_path().with_file_name("terminal.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_means_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = TerminalSettings::load(&dir.path().join("terminal.json"));
        assert_eq!(settings, TerminalSettings { look: Look::Native, images: true });
    }

    #[test]
    fn an_unreadable_file_means_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("terminal.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(TerminalSettings::load(&path), TerminalSettings::default());
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/er/terminal.json");
        let settings = TerminalSettings { look: Look::Antiope, images: false };
        settings.save(&path).unwrap();
        assert_eq!(TerminalSettings::load(&path), settings);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"look\": \"antiope\""), "{raw}");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("terminal.json");
        std::fs::write(&path, br#"{"look":"antiope"}"#).unwrap();
        assert_eq!(
            TerminalSettings::load(&path),
            TerminalSettings { look: Look::Antiope, images: true }
        );
    }

    #[test]
    fn the_default_path_sits_next_to_the_database() {
        let path = TerminalSettings::default_path();
        assert!(path.ends_with("Library/Application Support/Ressac/terminal.json"), "{path:?}");
    }
}
```

- [ ] **Step 2: Run.** `cargo test -p ressac tui::reader::settings` → 5 passed (after the RED check described in Step 1).

### Task 4a.3: Gate and commit

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/Cargo.toml crates/ressac/src/tui/reader/mod.rs \
  crates/ressac/src/tui/reader/layout.rs crates/ressac/src/tui/reader/settings.rs
git commit -m "Reader: adaptive columns for the pane's width, the terminal's look and image settings

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: green, no warning.

---

## Sub-phase 4b: the sources column and the reader state

### Task 4b.1: `sidebar.rs`

- [ ] **Step 1: Module.** In `tui/reader/mod.rs`, after `pub mod settings;`:

```rust
pub mod sidebar;
pub mod state;
```

(`state.rs` is created in Task 4b.2; create it now with only its module doc comment `//! The reader's state and keys (spec §5).` so the crate compiles.)

- [ ] **Step 2: Write `sidebar.rs` with its tests** (to see RED, first write `rows` returning `Vec::new()` and `step` returning `from`; the tests below fail; then write the bodies shown):

```rust
//! The sources column as a flat list of rows (spec §4, Sources; §6): the smart lists, the
//! folder tree in the app's display order, then the rhythm groups. Pure. The garden's pages
//! join in phase 8.

use ressac_core::model::{CadenceGroup, FeedId, FolderId};
use ressac_core::store::{Source, SourceNode, SourceNodeKind, SourcesSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub enum SidebarRow {
    /// Not selectable: `Feeds`, `Rhythm`.
    Heading(&'static str),
    Smart { source: Source, title: &'static str, unread: usize },
    Folder { id: FolderId, name: String, depth: usize, page: Option<u16>, unread: usize },
    Feed { id: FeedId, title: String, depth: usize, page: Option<u16>, unread: usize },
    Rhythm { group: CadenceGroup, unread: usize },
}

impl SidebarRow {
    /// The list this row opens; `None` for a heading.
    pub fn source(&self) -> Option<Source> {
        match self {
            SidebarRow::Heading(_) => None,
            SidebarRow::Smart { source, .. } => Some(*source),
            SidebarRow::Folder { id, .. } => Some(Source::Folder(*id)),
            SidebarRow::Feed { id, .. } => Some(Source::Feed(*id)),
            SidebarRow::Rhythm { group, .. } => Some(Source::Cadence(*group)),
        }
    }
}

pub fn rows(snapshot: &SourcesSnapshot) -> Vec<SidebarRow> {
    let counts = &snapshot.counts;
    let mut rows = vec![
        SidebarRow::Smart { source: Source::Journal, title: "Journal", unread: counts.journal },
        SidebarRow::Smart { source: Source::Unread, title: "Unread", unread: counts.unread },
        SidebarRow::Smart { source: Source::Today, title: "Today", unread: counts.today },
        SidebarRow::Smart { source: Source::Starred, title: "Starred", unread: counts.starred },
    ];
    if !snapshot.tree.is_empty() {
        rows.push(SidebarRow::Heading("Feeds"));
        push_nodes(&mut rows, &snapshot.tree, 0);
    }
    rows.push(SidebarRow::Heading("Rhythm"));
    rows.extend(CadenceGroup::ALL.iter().map(|&group| SidebarRow::Rhythm {
        group,
        unread: counts.cadence.get(&group).copied().unwrap_or(0),
    }));
    rows
}

fn push_nodes(rows: &mut Vec<SidebarRow>, nodes: &[SourceNode], depth: usize) {
    for node in nodes {
        rows.push(match &node.kind {
            SourceNodeKind::Folder { id, name } => SidebarRow::Folder {
                id: *id,
                name: name.clone(),
                depth,
                page: node.page,
                unread: node.unread,
            },
            SourceNodeKind::Feed { id, title } => SidebarRow::Feed {
                id: *id,
                title: title.clone(),
                depth,
                page: node.page,
                unread: node.unread,
            },
        });
        push_nodes(rows, &node.children, depth + 1);
    }
}

pub fn index_of(rows: &[SidebarRow], source: Source) -> Option<usize> {
    rows.iter().position(|row| row.source() == Some(source))
}

/// The next selectable row `delta` steps away, skipping headings; `from` at either end.
pub fn step(rows: &[SidebarRow], from: usize, delta: isize) -> usize {
    let mut at = from;
    loop {
        match at.checked_add_signed(delta).filter(|&next| next < rows.len()) {
            None => return from,
            Some(next) if rows[next].source().is_some() => return next,
            Some(next) => at = next,
        }
    }
}

#[cfg(test)]
pub(crate) fn sample_snapshot() -> SourcesSnapshot {
    use ressac_core::store::SourceCounts;
    let mut cadence: std::collections::BTreeMap<CadenceGroup, usize> =
        CadenceGroup::ALL.iter().map(|&group| (group, 0)).collect();
    cadence.insert(CadenceGroup::Daily, 3);
    SourcesSnapshot {
        counts: SourceCounts {
            journal: 2,
            unread: 5,
            today: 1,
            starred: 0,
            needs_triage: 4,
            cadence,
        },
        tree: vec![SourceNode {
            kind: SourceNodeKind::Folder { id: 1, name: "Blogs".into() },
            page: Some(200),
            unread: 3,
            children: vec![SourceNode {
                kind: SourceNodeKind::Feed { id: 7, title: "Daring Fireball".into() },
                page: Some(201),
                unread: 3,
                children: vec![],
            }],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_lists_then_the_tree_then_the_rhythm_groups() {
        let rows = rows(&sample_snapshot());
        assert_eq!(rows.len(), 13);
        assert_eq!(rows[0], SidebarRow::Smart { source: Source::Journal, title: "Journal", unread: 2 });
        assert_eq!(rows[3], SidebarRow::Smart { source: Source::Starred, title: "Starred", unread: 0 });
        assert_eq!(rows[4], SidebarRow::Heading("Feeds"));
        assert_eq!(
            rows[5],
            SidebarRow::Folder { id: 1, name: "Blogs".into(), depth: 0, page: Some(200), unread: 3 }
        );
        assert_eq!(
            rows[6],
            SidebarRow::Feed { id: 7, title: "Daring Fireball".into(), depth: 1, page: Some(201), unread: 3 }
        );
        assert_eq!(rows[7], SidebarRow::Heading("Rhythm"));
        assert_eq!(rows[8], SidebarRow::Rhythm { group: CadenceGroup::Daily, unread: 3 });
        assert_eq!(rows[12], SidebarRow::Rhythm { group: CadenceGroup::Silent, unread: 0 });
    }

    #[test]
    fn no_feeds_heading_without_feeds() {
        let mut snapshot = sample_snapshot();
        snapshot.tree.clear();
        let rows = rows(&snapshot);
        assert!(!rows.contains(&SidebarRow::Heading("Feeds")));
        assert_eq!(rows[4], SidebarRow::Heading("Rhythm"));
    }

    #[test]
    fn stepping_skips_headings_and_stops_at_the_ends() {
        let rows = rows(&sample_snapshot());
        assert_eq!(step(&rows, 3, 1), 5);
        assert_eq!(step(&rows, 5, -1), 3);
        assert_eq!(step(&rows, 6, 1), 8);
        assert_eq!(step(&rows, 0, -1), 0);
        assert_eq!(step(&rows, 12, 1), 12);
    }

    #[test]
    fn a_source_is_found_by_its_row() {
        let rows = rows(&sample_snapshot());
        assert_eq!(index_of(&rows, Source::Feed(7)), Some(6));
        assert_eq!(index_of(&rows, Source::Cadence(CadenceGroup::Rare)), Some(11));
        assert_eq!(index_of(&rows, Source::Feed(99)), None);
    }
}
```

- [ ] **Step 3: Run.** `cargo test -p ressac tui::reader::sidebar` → 4 passed.

### Task 4b.2: `state.rs`, tests first

- [ ] **Step 1: Write the full test module at the end of `state.rs`** (the production code of Task 4b.3 goes above it; to see RED, first paste the types of Task 4b.3 Step 1 with every `ReaderState` method body replaced by `unimplemented!()` — `cargo test -p ressac tui::reader::state` then fails in every test):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::reader::sidebar::sample_snapshot;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn row(id: i64, feed: FeedId, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: feed,
            feed_title: format!("Feed {feed}"),
            title: Some(format!("Entry {id}")),
            author: None,
            url: Some(format!("https://e.test/{id}")),
            published_at: "2026-10-16T08:00:00Z".parse().unwrap(),
            is_read: read,
            is_starred: false,
            has_fulltext: false,
        }
    }

    fn page(rows: Vec<EntryRow>, next: Option<ListCursor>) -> EntryPage {
        let unread = rows.iter().filter(|r| !r.is_read).count();
        EntryPage { total: rows.len(), unread, rows, next }
    }

    fn cursor() -> ListCursor {
        ListCursor { published_at: "2026-10-15T08:00:00.000Z".into(), entry_id: 1 }
    }

    fn detail_with(id: i64, html: &str) -> Box<EntryDetail> {
        Box::new(EntryDetail {
            row: row(id, 7, false),
            summary_html: None,
            content_html: Some(html.to_string()),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        })
    }

    /// `paragraphs` one-line paragraphs: 2 × paragraphs − 1 lines once rendered.
    fn detail(id: i64, paragraphs: usize) -> Box<EntryDetail> {
        let html: String = (0..paragraphs).map(|i| format!("<p>Paragraph {i}</p>")).collect();
        detail_with(id, &html)
    }

    fn key(state: &mut ReaderState, code: KeyCode) -> Vec<Command> {
        state.on_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn char_key(state: &mut ReaderState, c: char) -> Vec<Command> {
        key(state, KeyCode::Char(c))
    }

    fn article_text(state: &ReaderState) -> String {
        state
            .article
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .map(|line| line.spans.iter().map(|s| &*s.content).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn entries(source: Source, rows: Vec<EntryRow>, next: Option<ListCursor>) -> Event {
        Event::Entries { source, page: page(rows, next), appended: false }
    }

    /// 140 × 40 (three columns) on the Journal, entries 1–4, entries 2 and 4 already read,
    /// entry 1 shown (not marked read: nobody asked for it yet).
    /// A fixed clock: 16 October 2026, 06:00 UTC, 08:00 in a +02:00 zone.
    fn test_clock() -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::FixedOffset>) {
        use chrono::TimeZone;
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 16, 6, 0, 0).unwrap();
        (now, now.with_timezone(&chrono::FixedOffset::east_opt(7200).unwrap()))
    }

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        let (now, local) = test_clock();
        state.start(now, local);
        state.on_event(Event::Sources(sample_snapshot()));
        let rows = vec![row(1, 7, false), row(2, 7, true), row(3, 8, false), row(4, 8, true)];
        state.on_event(entries(Source::Journal, rows, None));
        state
    }

    #[test]
    fn the_reader_opens_on_the_journal() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        assert_eq!(state.source(), Source::Journal);
        assert_eq!(state.focus(), Column::Entries);
        assert_eq!(state.class(), LayoutClass::Three);
        assert_eq!(state.look(), Look::Native);
        let (now, local) = test_clock();
        assert_eq!(
            state.start(now, local),
            [
                Command::LoadSources,
                Command::LoadJournal,
                Command::LoadEntries { source: Source::Journal, filter: ListFilter::All, after: None },
            ]
        );
    }

    #[test]
    fn the_first_entry_is_shown_without_being_marked_read() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        let commands = state.on_event(entries(Source::Journal, vec![row(1, 7, false)], None));
        assert_eq!(commands, [Command::LoadEntry(1)]);
        assert_eq!(state.selected_entry(), Some(1));
        assert!(!state.entries[0].is_read);
    }

    #[test]
    fn one_column_waits_for_enter_before_showing_an_entry() {
        let mut state = ReaderState::new(TerminalSettings::default(), 70, 40);
        assert_eq!(state.class(), LayoutClass::One);
        let commands = state.on_event(entries(Source::Journal, vec![row(1, 7, false)], None));
        assert!(commands.is_empty());
        assert_eq!(state.selected_entry(), None);
    }

    #[test]
    fn entries_of_another_source_are_ignored() {
        let mut state = reader();
        state.on_event(entries(Source::Unread, vec![row(9, 7, false)], None));
        assert_eq!(state.entries.len(), 4);
    }

    #[test]
    fn j_and_k_move_and_mark_what_they_open_as_read() {
        let mut state = reader();
        assert_eq!(state.unread, 2);
        assert_eq!(char_key(&mut state, 'j'), [Command::LoadEntry(2)]);
        assert_eq!(
            char_key(&mut state, 'j'),
            [Command::LoadEntry(3), Command::SetRead(3, true)]
        );
        assert_eq!(state.unread, 1);
        assert_eq!(key(&mut state, KeyCode::Up), [Command::LoadEntry(2)]);
        assert_eq!(
            char_key(&mut state, 'k'),
            [Command::LoadEntry(1), Command::SetRead(1, true)]
        );
        assert_eq!(state.unread, 0);
        assert!(char_key(&mut state, 'k').is_empty());
    }

    #[test]
    fn j_past_the_loaded_rows_asks_for_the_next_page() {
        let mut state = reader();
        for _ in 0..3 {
            char_key(&mut state, 'j');
        }
        assert!(char_key(&mut state, 'j').is_empty());
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.on_event(entries(Source::Unread, vec![], None));
        state.source = Source::Unread;
        state.on_event(entries(Source::Unread, vec![row(1, 7, false)], Some(cursor())));
        assert_eq!(
            char_key(&mut state, 'j'),
            [Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: Some(cursor()),
            }]
        );
    }

    #[test]
    fn space_scrolls_the_article_then_moves_to_the_next_unread() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        assert_eq!(state.article.as_ref().unwrap().lines.len(), 79);
        // Reader text height at 140 × 40: 38 − 2 (border) − 3 (header) = 33; max scroll 46.
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.scroll, 32);
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.scroll, 46);
        assert_eq!(
            char_key(&mut state, ' '),
            [Command::LoadEntry(3), Command::SetRead(3, true)]
        );
        assert_eq!(state.scroll, 0);
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.message.as_deref(), Some("That's the edition."));
    }

    #[test]
    fn space_waits_for_the_next_page_then_carries_on() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Unread;
        state.on_event(entries(Source::Unread, vec![row(1, 7, true)], Some(cursor())));
        assert_eq!(
            char_key(&mut state, ' '),
            [Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: Some(cursor()),
            }]
        );
        let more = Event::Entries {
            source: Source::Unread,
            page: page(vec![row(5, 7, false)], None),
            appended: true,
        };
        assert_eq!(state.on_event(more), [Command::LoadEntry(5), Command::SetRead(5, true)]);
    }

    #[test]
    fn space_at_the_end_of_a_list_says_so() {
        let mut state = ReaderState::new(TerminalSettings::default(), 140, 40);
        state.source = Source::Starred;
        state.on_event(entries(Source::Starred, vec![row(1, 7, true)], None));
        assert!(char_key(&mut state, ' ').is_empty());
        assert_eq!(state.message.as_deref(), Some("No more unread entries here."));
    }

    #[test]
    fn arrows_in_the_sources_column_switch_the_list() {
        let mut state = reader();
        key(&mut state, KeyCode::Left);
        assert_eq!(state.focus(), Column::Sources);
        assert_eq!(
            key(&mut state, KeyCode::Down),
            [Command::LoadEntries { source: Source::Unread, filter: ListFilter::All, after: None }]
        );
        assert_eq!(state.source(), Source::Unread);
        assert!(state.entries.is_empty());
        assert_eq!(state.selected_entry(), None);
        key(&mut state, KeyCode::Down);
        key(&mut state, KeyCode::Down);
        assert_eq!(
            key(&mut state, KeyCode::Down),
            [Command::LoadEntries { source: Source::Folder(1), filter: ListFilter::All, after: None }]
        );
        assert_eq!(state.sidebar_index, 5);
        key(&mut state, KeyCode::Up);
        key(&mut state, KeyCode::Up);
        key(&mut state, KeyCode::Up);
        assert_eq!(
            key(&mut state, KeyCode::Up),
            [
                Command::LoadJournal,
                Command::LoadEntries { source: Source::Journal, filter: ListFilter::All, after: None },
            ]
        );
    }

    #[test]
    fn columns_in_three_column_mode() {
        let mut state = reader();
        char_key(&mut state, 'l');
        assert_eq!(state.focus(), Column::Reader);
        char_key(&mut state, 'h');
        assert_eq!(state.focus(), Column::Entries);
        char_key(&mut state, 'h');
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Right);
        assert_eq!(state.focus(), Column::Entries);
        key(&mut state, KeyCode::Tab);
        key(&mut state, KeyCode::Tab);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Sources);
    }

    #[test]
    fn two_column_mode_opens_the_sources_over_the_list() {
        let mut state = reader();
        state.on_event(Event::Resize { width: 100, height: 40 });
        assert_eq!(state.class(), LayoutClass::Two);
        key(&mut state, KeyCode::Left);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Entries);
    }

    #[test]
    fn one_column_mode_walks_in_with_enter_and_out_with_esc() {
        let mut state = ReaderState::new(TerminalSettings::default(), 70, 40);
        state.on_event(Event::Sources(sample_snapshot()));
        state.on_event(entries(Source::Journal, vec![row(1, 7, false)], None));
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::LoadEntry(1), Command::SetRead(1, true)]
        );
        assert_eq!(state.focus(), Column::Reader);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Entries);
        key(&mut state, KeyCode::Esc);
        assert_eq!(state.focus(), Column::Sources);
        key(&mut state, KeyCode::Enter);
        assert_eq!(state.focus(), Column::Entries);
    }

    #[test]
    fn m_s_shift_a_and_u() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 'm'), [Command::SetRead(1, true)]);
        assert_eq!(state.unread, 1);
        assert_eq!(char_key(&mut state, 'm'), [Command::SetRead(1, false)]);
        assert_eq!(state.unread, 2);
        assert_eq!(char_key(&mut state, 's'), [Command::SetStarred(1, true)]);
        assert!(state.entries[0].is_starred);
        let shifted = state.on_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(shifted, [Command::MarkAllRead(Source::Journal)]);
        assert_eq!(
            char_key(&mut state, 'u'),
            [Command::LoadEntries { source: Source::Journal, filter: ListFilter::Unread, after: None }]
        );
    }

    #[test]
    fn search_is_typed_then_run_then_cleared() {
        let mut state = reader();
        assert!(char_key(&mut state, '/').is_empty());
        for c in ['r', 'u', 'x'] {
            assert!(char_key(&mut state, c).is_empty());
        }
        key(&mut state, KeyCode::Backspace);
        char_key(&mut state, 's');
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::Search { source: Source::Journal, text: "rus".into() }]
        );
        assert_eq!(
            key(&mut state, KeyCode::Esc),
            [Command::LoadEntries { source: Source::Journal, filter: ListFilter::All, after: None }]
        );
        assert_eq!(state.search, None);
    }

    #[test]
    fn full_text_is_extracted_once_then_toggled() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 3)));
        assert_eq!(char_key(&mut state, 'f'), [Command::Extract(1)]);
        assert_eq!(state.message.as_deref(), Some("Fetching the full article…"));
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::Extracted { entry_id: 1, error: None })),
            [Command::LoadEntry(1)]
        );
        let mut full = detail(1, 3);
        full.fulltext_html = Some("<p>Full article</p>".into());
        state.on_event(Event::Entry(full));
        assert!(article_text(&state).contains("Full article"));
        assert!(char_key(&mut state, 'f').is_empty());
        assert!(article_text(&state).contains("Paragraph 0"));
        assert!(char_key(&mut state, 'f').is_empty());
        assert!(article_text(&state).contains("Full article"));
    }

    #[test]
    fn o_opens_the_article_or_a_numbered_link() {
        let mut state = reader();
        assert!(char_key(&mut state, 'o').is_empty());
        assert_eq!(state.message.as_deref(), Some("No entry selected."));
        state.on_event(Event::Entry(detail_with(
            1,
            "<p><a href='https://e.test/a'>a</a> <a href='https://e.test/b'>b</a></p>",
        )));
        char_key(&mut state, 'o');
        assert_eq!(state.overlay, Overlay::LinkPrompt);
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::OpenUrl("https://e.test/1".into())]
        );
        char_key(&mut state, 'o');
        assert_eq!(char_key(&mut state, '2'), [Command::OpenUrl("https://e.test/b".into())]);
        char_key(&mut state, 'o');
        assert!(char_key(&mut state, '9').is_empty());
        assert_eq!(state.message.as_deref(), Some("No link [9]."));
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn c_overrides_the_rhythm_of_the_entry_s_feed() {
        let mut state = reader();
        char_key(&mut state, 'c');
        assert_eq!(state.overlay, Overlay::Rhythm { feed_id: 7, choice: 0 });
        key(&mut state, KeyCode::Down);
        char_key(&mut state, 'j');
        assert_eq!(
            key(&mut state, KeyCode::Enter),
            [Command::SetCadence(7, Some(CadenceGroup::Weekly))]
        );
        assert_eq!(state.overlay, Overlay::None);
        char_key(&mut state, 'c');
        assert!(key(&mut state, KeyCode::Esc).is_empty());
        assert_eq!(state.overlay, Overlay::None);
        let mut empty = ReaderState::new(TerminalSettings::default(), 140, 40);
        char_key(&mut empty, 'c');
        assert_eq!(empty.message.as_deref(), Some("No feed selected."));
    }

    #[test]
    fn t_and_i_change_the_settings() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 't'), [Command::SaveSettings]);
        assert_eq!(state.look(), Look::Antiope);
        char_key(&mut state, 't');
        assert_eq!(state.look(), Look::Native);
        assert_eq!(char_key(&mut state, 'i'), [Command::SaveSettings]);
        assert!(!state.settings.images);
    }

    #[test]
    fn q_ctrl_c_and_help() {
        let mut state = reader();
        assert_eq!(char_key(&mut state, 'q'), [Command::Quit]);
        let ctrl_c = state.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(ctrl_c, [Command::Quit]);
        assert!(char_key(&mut state, '?').is_empty());
        assert_eq!(state.overlay, Overlay::Help);
        assert!(char_key(&mut state, 'j').is_empty());
        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.selected_entry(), Some(1));
    }

    /// Review Focus 2: the app marks entries read; the reader reloads and keeps its place.
    #[test]
    fn a_reload_keeps_the_selected_entry() {
        let mut state = reader();
        char_key(&mut state, 'j');
        assert_eq!(
            state.on_event(Event::DataChanged),
            [
                Command::LoadSources,
                Command::LoadJournal,
                Command::LoadEntries { source: Source::Journal, filter: ListFilter::All, after: None },
            ]
        );
        let reloaded = vec![row(3, 8, false), row(2, 7, true), row(1, 7, true)];
        assert!(state.on_event(entries(Source::Journal, reloaded, None)).is_empty());
        assert_eq!(state.selected_entry(), Some(2));
        assert_eq!(state.unread, 1);
    }

    /// Review Focus 3: 140 → 70 → 140 with an article open keeps the entry and the scroll.
    #[test]
    fn resizing_keeps_the_entry_and_the_scroll() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        char_key(&mut state, ' ');
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize { width: 70, height: 40 });
        assert_eq!(state.class(), LayoutClass::One);
        assert_eq!(state.focus(), Column::Reader);
        assert_eq!(state.selected_entry(), Some(1));
        assert_eq!(state.scroll, 32);
        state.on_event(Event::Resize { width: 140, height: 40 });
        assert_eq!(state.class(), LayoutClass::Three);
        assert_eq!(state.focus(), Column::Reader);
        assert_eq!(state.scroll, 32);
    }

    #[test]
    fn a_shorter_pane_clamps_the_scroll() {
        let mut state = reader();
        state.on_event(Event::Entry(detail(1, 40)));
        char_key(&mut state, ' ');
        char_key(&mut state, ' ');
        assert_eq!(state.scroll, 46);
        state.on_event(Event::Resize { width: 140, height: 80 });
        // 78 − 2 − 3 = 73 visible lines: 79 − 73 = 6 at most.
        assert_eq!(state.scroll, 6);
    }

    #[test]
    fn job_outcomes() {
        let mut state = reader();
        state.on_event(Event::Progress { label: "Fetching feeds".into(), done: 3, total: 9 });
        assert_eq!(state.progress, Some(("Fetching feeds".into(), 3, 9)));
        assert_eq!(
            state.on_event(Event::JobDone(JobOutcome::Refreshed)),
            [
                Command::LoadSources,
                Command::LoadEntries { source: Source::Journal, filter: ListFilter::All, after: None },
            ]
        );
        assert_eq!(state.progress, None);
        assert!(state.on_event(Event::JobDone(JobOutcome::Offline)).is_empty());
        assert_eq!(state.message.as_deref(), Some("Offline: no edition yet"));
        state.on_event(Event::JobDone(JobOutcome::Failed("The database is busy.".into())));
        assert_eq!(state.message.as_deref(), Some("The database is busy."));
    }
}
```

### Task 4b.3: `state.rs`, the code

- [ ] **Step 1: Put the production code above the test module** (replacing the doc-comment-only stub):

```rust
//! The reader's state and keys (spec §5). Pure: no terminal, no database, no clock. Keys and
//! events come in, `Command`s go out to the event loop (phase 5b), which runs them and answers
//! with `Event`s; the same pattern as the garden's `GardenState`.

use chrono::{DateTime, FixedOffset, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ressac_core::model::{CadenceGroup, FeedId};
use ressac_core::store::{
    EntryDetail, EntryPage, EntryRow, JournalView, ListCursor, ListFilter, Source, SourcesSnapshot,
};

use super::html::{RenderedArticle, is_web_url, render_html};
use super::layout::{Column, LayoutClass, Look, columns, layout_class, text_height, text_width};
use super::settings::TerminalSettings;
use super::sidebar::{self, SidebarRow};

/// Lines of the reader column taken by the article header (title; feed, author, date; state).
pub const READER_HEADER_LINES: usize = 3;

pub const END_OF_EDITION: &str = "That's the edition.";
pub const NO_MORE_UNREAD: &str = "No more unread entries here.";
pub const NO_ENTRY: &str = "No entry selected.";
pub const NO_LINK: &str = "This entry has no link.";
pub const NO_FEED: &str = "No feed selected.";
pub const FETCHING_FULL: &str = "Fetching the full article…";
pub const OFFLINE: &str = "Offline: no edition yet";
pub const OPEN_PROMPT: &str = "Open: Enter for the article, 1-9 for a link, Esc to cancel.";

/// The rhythm menu, in the app's order; `None` is Auto.
pub const RHYTHM_CHOICES: [Option<CadenceGroup>; 6] = [
    None,
    Some(CadenceGroup::Daily),
    Some(CadenceGroup::Weekly),
    Some(CadenceGroup::Monthly),
    Some(CadenceGroup::Rare),
    Some(CadenceGroup::Silent),
];

/// What the event loop must do (phase 5b runs them).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadSources,
    LoadEntries { source: Source, filter: ListFilter, after: Option<ListCursor> },
    LoadJournal,
    Search { source: Source, text: String },
    LoadEntry(i64),
    SetRead(i64, bool),
    SetStarred(i64, bool),
    MarkAllRead(Source),
    Extract(i64),
    FetchImage(String),
    BuildEdition,
    Refresh(Option<Source>),
    OpenUrl(String),
    SetCadence(FeedId, Option<CadenceGroup>),
    /// A garden command from the garden screen (phase 8).
    Garden(crate::tui::state::Command),
    SaveSettings,
    Quit,
}

/// What the event loop reports back.
#[derive(Debug, Clone)]
pub enum Event {
    Sources(SourcesSnapshot),
    Entries { source: Source, page: EntryPage, appended: bool },
    Journal(Option<JournalView>),
    /// Boxed: far larger than the other variants (`clippy::large_enum_variant`).
    Entry(Box<EntryDetail>),
    ImageReady { url: String },
    ImageFailed { url: String },
    Progress { label: String, done: usize, total: usize },
    JobDone(JobOutcome),
    DataChanged,
    Tick { now: DateTime<Utc>, local: DateTime<FixedOffset> },
    Resize { width: u16, height: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobOutcome {
    Refreshed,
    EditionBuilt,
    /// Also when another process built today's edition meanwhile: the core's due re-check
    /// returns NotDue in both cases (phase 7).
    EditionNotDue,
    Offline,
    Failed(String),
    Extracted { entry_id: i64, error: Option<String> },
}

/// A modal layer over the columns; it takes the next key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    /// After `o`: Enter opens the article, a digit a numbered link.
    LinkPrompt,
    Rhythm { feed_id: FeedId, choice: usize },
}

pub struct ReaderState {
    pub settings: TerminalSettings,
    pub width: u16,
    pub height: u16,
    pub class: LayoutClass,
    pub focus: Column,
    pub source: Source,
    pub filter: ListFilter,
    pub sources: Option<SourcesSnapshot>,
    pub sidebar: Vec<SidebarRow>,
    pub sidebar_index: usize,
    pub entries: Vec<EntryRow>,
    /// Rows and unread rows of the whole list, loaded or not.
    pub total: usize,
    pub unread: usize,
    pub next: Option<ListCursor>,
    pub journal: Option<JournalView>,
    pub selected: Option<i64>,
    pub detail: Option<EntryDetail>,
    pub article: Option<RenderedArticle>,
    pub show_fulltext: bool,
    /// First article line shown.
    pub scroll: usize,
    pub search: Option<String>,
    pub search_editing: bool,
    pub overlay: Overlay,
    pub message: Option<String>,
    pub progress: Option<(String, usize, usize)>,
    pub now: Option<DateTime<Utc>>,
    /// The local time of the last `Tick` (or `start`): every date shown uses it, never `chrono::Local`.
    pub local: Option<DateTime<FixedOffset>>,
    /// Space reached the end of the loaded rows: carry on once the next page arrives.
    pending_advance: bool,
}

impl ReaderState {
    /// The reader opens on the Journal (spec §6), the entry list focused.
    pub fn new(settings: TerminalSettings, width: u16, height: u16) -> Self {
        ReaderState {
            settings,
            width,
            height,
            class: layout_class(width, height),
            focus: Column::Entries,
            source: Source::Journal,
            filter: ListFilter::All,
            sources: None,
            sidebar: Vec::new(),
            sidebar_index: 0,
            entries: Vec::new(),
            total: 0,
            unread: 0,
            next: None,
            journal: None,
            selected: None,
            detail: None,
            article: None,
            show_fulltext: false,
            scroll: 0,
            search: None,
            search_editing: false,
            overlay: Overlay::None,
            message: None,
            progress: None,
            now: None,
            local: None,
            pending_advance: false,
        }
    }

    /// The first commands: the sources, the Journal and its entries. Stores the clock; phase 7
    /// extends this body with the edition build (same signature, see the plan index).
    pub fn start(&mut self, now: DateTime<Utc>, local: DateTime<FixedOffset>) -> Vec<Command> {
        self.now = Some(now);
        self.local = Some(local);
        vec![Command::LoadSources, Command::LoadJournal, self.reload()]
    }

    /// Entry points for other screens (Antiope page jumps, phase 9).
    pub fn go_to(&mut self, source: Source) -> Vec<Command> {
        self.switch_source(source)
    }

    pub fn select_entry(&mut self, entry_id: i64) -> Vec<Command> {
        match self.entries.iter().position(|row| row.entry_id == entry_id) {
            Some(index) => self.select(index, true),
            None => Vec::new(),
        }
    }

    pub fn set_focus(&mut self, column: Column) {
        self.focus = column;
    }

    pub fn look(&self) -> Look {
        self.settings.look
    }

    pub fn focus(&self) -> Column {
        self.focus
    }

    pub fn source(&self) -> Source {
        self.source
    }

    pub fn selected_entry(&self) -> Option<i64> {
        self.selected
    }

    pub fn class(&self) -> LayoutClass {
        self.class
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Command::Quit];
        }
        if self.search_editing {
            return self.on_search_key(key.code);
        }
        match self.overlay {
            Overlay::Help => {
                self.overlay = Overlay::None;
                return Vec::new();
            }
            Overlay::LinkPrompt => return self.on_link_key(key.code),
            Overlay::Rhythm { .. } => return self.on_rhythm_key(key.code),
            Overlay::None => {}
        }
        self.message = None;
        match key.code {
            KeyCode::Char('q') => vec![Command::Quit],
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                Vec::new()
            }
            KeyCode::Char('t') => {
                self.settings.look = match self.settings.look {
                    Look::Native => Look::Antiope,
                    Look::Antiope => Look::Native,
                };
                self.render_article();
                vec![Command::SaveSettings]
            }
            KeyCode::Char('i') => {
                self.settings.images = !self.settings.images;
                vec![Command::SaveSettings]
            }
            KeyCode::Char('j') => self.move_entry(1),
            KeyCode::Char('k') => self.move_entry(-1),
            KeyCode::Down if self.focus == Column::Sources => self.move_source(1),
            KeyCode::Up if self.focus == Column::Sources => self.move_source(-1),
            KeyCode::Down => self.move_entry(1),
            KeyCode::Up => self.move_entry(-1),
            KeyCode::Char(' ') => self.advance(),
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = match self.focus {
                    Column::Reader => Column::Entries,
                    _ => Column::Sources,
                };
                Vec::new()
            }
            KeyCode::Char('l') | KeyCode::Right => self.focus_right(false),
            KeyCode::Tab => self.focus_right(true),
            KeyCode::Enter => self.enter(),
            KeyCode::Esc => self.escape(),
            KeyCode::Char('m') => self.toggle_read(),
            KeyCode::Char('s') => self.toggle_star(),
            KeyCode::Char('A') => vec![Command::MarkAllRead(self.source)],
            KeyCode::Char('u') => {
                self.filter = match self.filter {
                    ListFilter::All => ListFilter::Unread,
                    ListFilter::Unread => ListFilter::All,
                };
                self.next = None;
                vec![self.reload()]
            }
            KeyCode::Char('/') => {
                self.search_editing = true;
                self.search.get_or_insert_with(String::new);
                Vec::new()
            }
            KeyCode::Char('f') => self.full_text(),
            KeyCode::Char('o') => self.open_prompt(),
            KeyCode::Char('r') => vec![Command::Refresh(Some(self.source))],
            KeyCode::Char('R') => vec![Command::Refresh(None)],
            KeyCode::Char('c') => self.rhythm_menu(),
            _ => Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::Sources(snapshot) => {
                self.sidebar = sidebar::rows(&snapshot);
                let fallback = self.sidebar_index.min(self.sidebar.len().saturating_sub(1));
                self.sidebar_index = sidebar::index_of(&self.sidebar, self.source).unwrap_or(fallback);
                self.sources = Some(snapshot);
                Vec::new()
            }
            Event::Entries { source, page, appended } => self.entries_loaded(source, page, appended),
            Event::Journal(view) => {
                self.journal = view;
                Vec::new()
            }
            Event::Entry(detail) => {
                self.entry_loaded(*detail);
                Vec::new()
            }
            Event::ImageReady { .. } | Event::ImageFailed { .. } => Vec::new(),
            Event::Progress { label, done, total } => {
                self.progress = Some((label, done, total));
                Vec::new()
            }
            Event::JobDone(outcome) => self.job_done(outcome),
            Event::DataChanged => self.data_changed(),
            Event::Tick { now, local } => {
                self.now = Some(now);
                self.local = Some(local);
                Vec::new()
            }
            Event::Resize { width, height } => {
                self.resize(width, height);
                Vec::new()
            }
        }
    }

    /// The current list again: the search when one is active, else the source.
    fn reload(&self) -> Command {
        match &self.search {
            Some(text) if !text.is_empty() && !self.search_editing => {
                Command::Search { source: self.source, text: text.clone() }
            }
            _ => Command::LoadEntries { source: self.source, filter: self.filter, after: None },
        }
    }

    fn index(&self) -> Option<usize> {
        let id = self.selected?;
        self.entries.iter().position(|row| row.entry_id == id)
    }

    /// Selects row `index`; opening it by hand (`mark_read`) marks it read, as the app does.
    fn select(&mut self, index: usize, mark_read: bool) -> Vec<Command> {
        let Some(row) = self.entries.get(index) else {
            return Vec::new();
        };
        let (id, was_read) = (row.entry_id, row.is_read);
        let mut commands = Vec::new();
        if self.selected != Some(id) {
            self.selected = Some(id);
            self.detail = None;
            self.article = None;
            self.scroll = 0;
            self.show_fulltext = false;
            commands.push(Command::LoadEntry(id));
        }
        if mark_read && !was_read {
            self.entries[index].is_read = true;
            self.unread = self.unread.saturating_sub(1);
            commands.push(Command::SetRead(id, true));
        }
        commands
    }

    fn move_entry(&mut self, delta: isize) -> Vec<Command> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let target = self.index().map_or(0, |i| i.saturating_add_signed(delta));
        if target >= self.entries.len() {
            return match &self.next {
                Some(cursor) if delta > 0 => vec![Command::LoadEntries {
                    source: self.source,
                    filter: self.filter,
                    after: Some(cursor.clone()),
                }],
                _ => Vec::new(),
            };
        }
        if Some(target) == self.index() {
            return Vec::new();
        }
        self.select(target, true)
    }

    fn reader_rect(&self) -> Option<Rect> {
        let area = Rect::new(0, 0, self.width, self.height);
        columns(area, self.class, Column::Reader, self.look()).reader
    }

    /// Article lines visible at once.
    fn page_height(&self) -> usize {
        self.reader_rect().map_or(1, |rect| {
            usize::from(text_height(rect, self.look()))
                .saturating_sub(READER_HEADER_LINES)
                .max(1)
        })
    }

    fn reader_text_width(&self) -> u16 {
        self.reader_rect()
            .map_or(self.width, |rect| text_width(rect, self.look()))
    }

    fn max_scroll(&self) -> usize {
        self.article
            .as_ref()
            .map_or(0, |a| a.lines.len().saturating_sub(self.page_height()))
    }

    /// Space (spec §5): scroll the article, then the next unread entry of the source.
    fn advance(&mut self) -> Vec<Command> {
        let max = self.max_scroll();
        if self.article.is_some() && self.scroll < max {
            let step = self.page_height().saturating_sub(1).max(1);
            self.scroll = (self.scroll + step).min(max);
            return Vec::new();
        }
        let from = self.index().map_or(0, |i| i + 1).min(self.entries.len());
        if let Some(offset) = self.entries[from..].iter().position(|row| !row.is_read) {
            if self.class == LayoutClass::One {
                self.focus = Column::Reader;
            }
            return self.select(from + offset, true);
        }
        if let Some(cursor) = self.next.clone() {
            self.pending_advance = true;
            return vec![Command::LoadEntries {
                source: self.source,
                filter: self.filter,
                after: Some(cursor),
            }];
        }
        let end = if self.source == Source::Journal { END_OF_EDITION } else { NO_MORE_UNREAD };
        self.message = Some(end.to_string());
        Vec::new()
    }

    fn focus_right(&mut self, wrap: bool) -> Vec<Command> {
        match self.focus {
            Column::Sources => self.open_sidebar_row(),
            Column::Entries => {
                self.focus = Column::Reader;
                if self.selected.is_none() && !self.entries.is_empty() {
                    return self.select(0, true);
                }
                Vec::new()
            }
            Column::Reader => {
                if wrap {
                    self.focus = Column::Sources;
                }
                Vec::new()
            }
        }
    }

    fn enter(&mut self) -> Vec<Command> {
        match self.focus {
            Column::Sources => self.open_sidebar_row(),
            Column::Entries => self.focus_right(false),
            Column::Reader => Vec::new(),
        }
    }

    fn escape(&mut self) -> Vec<Command> {
        if self.search.is_some() {
            self.search = None;
            return vec![self.reload()];
        }
        self.focus = match (self.class, self.focus) {
            (LayoutClass::One, Column::Reader) => Column::Entries,
            (LayoutClass::One, Column::Entries) => Column::Sources,
            (LayoutClass::Two, Column::Sources) => Column::Entries,
            (_, focus) => focus,
        };
        Vec::new()
    }

    fn open_sidebar_row(&mut self) -> Vec<Command> {
        self.focus = Column::Entries;
        match self.sidebar.get(self.sidebar_index).and_then(SidebarRow::source) {
            Some(source) => self.switch_source(source),
            None => Vec::new(),
        }
    }

    fn move_source(&mut self, delta: isize) -> Vec<Command> {
        if self.sidebar.is_empty() {
            return Vec::new();
        }
        self.sidebar_index = sidebar::step(&self.sidebar, self.sidebar_index, delta);
        match self.sidebar[self.sidebar_index].source() {
            Some(source) => self.switch_source(source),
            None => Vec::new(),
        }
    }

    fn switch_source(&mut self, source: Source) -> Vec<Command> {
        if let Some(index) = sidebar::index_of(&self.sidebar, source) {
            self.sidebar_index = index;
        }
        if source == self.source {
            return Vec::new();
        }
        self.source = source;
        self.entries.clear();
        self.total = 0;
        self.unread = 0;
        self.next = None;
        self.selected = None;
        self.detail = None;
        self.article = None;
        self.scroll = 0;
        self.search = None;
        self.pending_advance = false;
        let mut commands = Vec::new();
        if source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands.push(self.reload());
        commands
    }

    fn toggle_read(&mut self) -> Vec<Command> {
        let Some(index) = self.index() else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let row = &mut self.entries[index];
        row.is_read = !row.is_read;
        let (id, read) = (row.entry_id, row.is_read);
        if read {
            self.unread = self.unread.saturating_sub(1);
        } else {
            self.unread += 1;
        }
        if let Some(detail) = self.detail.as_mut().filter(|d| d.row.entry_id == id) {
            detail.row.is_read = read;
        }
        vec![Command::SetRead(id, read)]
    }

    fn toggle_star(&mut self) -> Vec<Command> {
        let Some(index) = self.index() else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let row = &mut self.entries[index];
        row.is_starred = !row.is_starred;
        let (id, starred) = (row.entry_id, row.is_starred);
        if let Some(detail) = self.detail.as_mut().filter(|d| d.row.entry_id == id) {
            detail.row.is_starred = starred;
        }
        vec![Command::SetStarred(id, starred)]
    }

    /// `f`: the stored full text, the feed's content again, or an extraction.
    fn full_text(&mut self) -> Vec<Command> {
        let Some(detail) = &self.detail else {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        };
        let id = detail.row.entry_id;
        let has_fulltext = detail.fulltext_html.is_some();
        if self.show_fulltext || has_fulltext {
            self.show_fulltext = !self.show_fulltext;
            self.render_article();
            return Vec::new();
        }
        self.message = Some(FETCHING_FULL.into());
        vec![Command::Extract(id)]
    }

    /// The article for the reader's current text width.
    fn render_article(&mut self) {
        let Some(detail) = &self.detail else {
            self.article = None;
            return;
        };
        let html = if self.show_fulltext {
            detail.fulltext_html.as_deref()
        } else {
            detail.content_html.as_deref().or(detail.summary_html.as_deref())
        };
        let article = render_html(
            html.unwrap_or_default(),
            self.reader_text_width(),
            detail.row.url.as_deref(),
        );
        self.article = Some(article);
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn open_prompt(&mut self) -> Vec<Command> {
        if self.detail.is_none() {
            self.message = Some(NO_ENTRY.into());
            return Vec::new();
        }
        self.overlay = Overlay::LinkPrompt;
        self.message = Some(OPEN_PROMPT.into());
        Vec::new()
    }

    fn on_link_key(&mut self, code: KeyCode) -> Vec<Command> {
        self.overlay = Overlay::None;
        self.message = None;
        match code {
            KeyCode::Enter => {
                let url = self.detail.as_ref().and_then(|d| d.row.url.clone());
                match url.filter(|u| is_web_url(u)) {
                    Some(url) => vec![Command::OpenUrl(url)],
                    None => {
                        self.message = Some(NO_LINK.into());
                        Vec::new()
                    }
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                let number = c.to_digit(10).map_or(0, |d| d as usize);
                match self.article.as_ref().and_then(|a| a.links.get(number - 1)) {
                    Some(url) => vec![Command::OpenUrl(url.clone())],
                    None => {
                        self.message = Some(format!("No link [{number}]."));
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    fn on_search_key(&mut self, code: KeyCode) -> Vec<Command> {
        match code {
            KeyCode::Char(c) => {
                self.search.get_or_insert_with(String::new).push(c);
                Vec::new()
            }
            KeyCode::Backspace => {
                if let Some(text) = self.search.as_mut() {
                    text.pop();
                }
                Vec::new()
            }
            KeyCode::Enter => {
                self.search_editing = false;
                if self.search.as_deref().is_some_and(str::is_empty) {
                    self.search = None;
                }
                vec![self.reload()]
            }
            KeyCode::Esc => {
                self.search_editing = false;
                self.search = None;
                vec![self.reload()]
            }
            _ => Vec::new(),
        }
    }

    /// `c`: the rhythm menu for the feed in view (a feed source, else the entry's feed).
    fn rhythm_menu(&mut self) -> Vec<Command> {
        let feed = match self.source {
            Source::Feed(id) => Some(id),
            _ => self.index().map(|i| self.entries[i].feed_id),
        };
        match feed {
            Some(feed_id) => self.overlay = Overlay::Rhythm { feed_id, choice: 0 },
            None => self.message = Some(NO_FEED.into()),
        }
        Vec::new()
    }

    fn on_rhythm_key(&mut self, code: KeyCode) -> Vec<Command> {
        let Overlay::Rhythm { feed_id, choice } = self.overlay else {
            return Vec::new();
        };
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                let choice = (choice + 1).min(RHYTHM_CHOICES.len() - 1);
                self.overlay = Overlay::Rhythm { feed_id, choice };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.overlay = Overlay::Rhythm { feed_id, choice: choice.saturating_sub(1) };
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                return vec![Command::SetCadence(feed_id, RHYTHM_CHOICES[choice])];
            }
            _ => self.overlay = Overlay::None,
        }
        Vec::new()
    }

    fn reader_visible(&self) -> bool {
        self.class != LayoutClass::One || self.focus == Column::Reader
    }

    fn entries_loaded(&mut self, source: Source, page: EntryPage, appended: bool) -> Vec<Command> {
        if source != self.source {
            return Vec::new();
        }
        if appended {
            self.entries.extend(page.rows);
        } else {
            self.entries = page.rows;
        }
        self.total = page.total;
        self.unread = page.unread;
        self.next = page.next;
        if self.index().is_none() {
            self.selected = None;
            self.detail = None;
            self.article = None;
        }
        if std::mem::take(&mut self.pending_advance) {
            return self.advance();
        }
        if self.selected.is_none() && !self.entries.is_empty() && self.reader_visible() {
            return self.select(0, false);
        }
        Vec::new()
    }

    fn entry_loaded(&mut self, detail: EntryDetail) {
        if self.selected != Some(detail.row.entry_id) {
            return;
        }
        let same = self
            .detail
            .as_ref()
            .is_some_and(|d| d.row.entry_id == detail.row.entry_id);
        if !same {
            self.scroll = 0;
            self.show_fulltext = detail.prefer_full_text && detail.fulltext_html.is_some();
        } else if self.show_fulltext && detail.fulltext_html.is_none() {
            self.show_fulltext = false;
        }
        self.detail = Some(detail);
        self.render_article();
    }

    fn job_done(&mut self, outcome: JobOutcome) -> Vec<Command> {
        self.progress = None;
        match outcome {
            JobOutcome::Refreshed => vec![Command::LoadSources, self.reload()],
            JobOutcome::EditionBuilt | JobOutcome::EditionNotDue => self.data_changed(),
            JobOutcome::Offline => {
                self.message = Some(OFFLINE.into());
                Vec::new()
            }
            JobOutcome::Failed(sentence) => {
                self.message = Some(sentence);
                Vec::new()
            }
            JobOutcome::Extracted { entry_id, error } => {
                if self.selected != Some(entry_id) {
                    return Vec::new();
                }
                self.message = None;
                if error.is_none() {
                    self.show_fulltext = true;
                }
                vec![Command::LoadEntry(entry_id)]
            }
        }
    }

    /// Another process (or this one) changed the database: reload, keeping the selection.
    fn data_changed(&mut self) -> Vec<Command> {
        let mut commands = vec![Command::LoadSources];
        if self.source == Source::Journal {
            commands.push(Command::LoadJournal);
        }
        commands.push(self.reload());
        commands
    }

    /// Review Focus 3: the selection and the scroll survive; a narrow pane keeps the article.
    fn resize(&mut self, width: u16, height: u16) {
        let before = self.class;
        self.width = width;
        self.height = height;
        self.class = layout_class(width, height);
        if self.class == LayoutClass::One
            && before != LayoutClass::One
            && self.detail.is_some()
            && self.focus != Column::Sources
        {
            self.focus = Column::Reader;
        }
        self.render_article();
    }
}
```

Notes for the implementer:
- `render_article` clamps the scroll to the new maximum; `resize` relies on it (test `a_shorter_pane_clamps_the_scroll`).
- In `full_text`, `f` with a stored full text toggles between it and the feed's content; without one it asks for an extraction, and `JobDone(Extracted { error: None })` turns the full text on before reloading the entry (`entry_loaded` keeps it on for the same entry when the full text arrived).
- `Command::Garden`, `Command::FetchImage`, `Command::BuildEdition`, `Event::ImageReady/ImageFailed` and `Event::Tick`'s `local` are produced or used by phases 6–8; the module's `allow(dead_code)` (until phase 5b) covers them, and phase 5b's event loop handles every variant.
- If a test's arithmetic disagrees with the code (scroll values, line counts), recompute from `layout.rs` and `READER_HEADER_LINES` before changing anything, and report which side was wrong.

- [ ] **Step 2: Run.**

```bash
cargo test -p ressac tui::reader::state
```

Expected: every test in `tui::reader::state::tests` passes.

### Task 4b.4: Gate and commit

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
git add crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/sidebar.rs \
  crates/ressac/src/tui/reader/state.rs
git commit -m "Reader: the sources column and the reader's state with every Native key

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: green, no warning.

## Gate 4

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `INSTA_UPDATE=no cargo test --workspace`: green after 4a and after 4b.
- Review Focus 3: `resizing_keeps_the_entry_and_the_scroll` and `a_shorter_pane_clamps_the_scroll` pass.
- Review Focus 2 (state side): `a_reload_keeps_the_selected_entry` passes.
- Two commits on `terminal-reading` (4a, 4b).
