# Phase 7: the Journal and the edition build in the reader

**Runs after:** phase 6 (images). Phases 4b (`ReaderState`, `Command`, `Event`, `JobOutcome`), 5a (`native.rs`) and 5b (`reader/mod.rs`, the event loop and its jobs) must be merged: this phase adds to them.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §6 "The Journal" and "Building the edition", the Journal rows of §5 (`Enter` on an overflow row, `r` retries), and Review Focus 5. It mirrors the app's `JournalModel.swift` and `JournalLayout` (`apps/macos/Ressac/App/JournalModel.swift`, `Reading/JournalList.swift`) so that both readers behave alike.

**Deviations from the plan index** (to reconcile before phase 8 consumes them):

1. **`JobOutcome::EditionBuiltElsewhere` is never produced.** The core cannot tell "already built before the refresh" from "built by another process during the refresh": `Ressac::build_edition` returns `EditionOutcome::NotDue` for both (`service.rs`, the `Err(Error::EditionNotDue) => return Ok(EditionOutcome::NotDue)` arm). Both map to `JobOutcome::EditionNotDue`, which the state handles exactly as the app handles its `builtElsewhere` refusal: phase back to idle, reload the Journal, no message. **Reconciled:** the variant is removed from `JobOutcome` (plan index and phase 4b); Review Focus 5 is the `EditionNotDue` test.
2. **New public names** (added here, used by phase 9's Antiope Journal page):
   - `tui/reader/journal.rs`: `JournalPhase`, `JournalLine`, `journal_lines`, `journal_header`, `placeholder`, `progress_text`, `offers_retry`, `outcome_of`, `EditionProgress`, constants `BUILDING`, `END_LINE`, `EMPTY_LINE`, `NO_EDITION`, `OFFLINE`.
   - `ReaderState::start(&mut self, now: DateTime<Utc>, local: DateTime<FixedOffset>) -> Vec<Command>` (the first commands of a session), `ReaderState::journal_phase(&self) -> &JournalPhase`, `ReaderState::journal(&self) -> Option<&JournalView>`.
   **Reconciled:** phase 4b already defines `start(&mut self, now, local)` (it stores `now` and `local`, returns `LoadSources`, `LoadJournal`, the reload). This phase replaces its body with the one below (same signature) and adds `journal_phase()` and `journal()`; 4b's public field `journal` is reused, never redeclared.
3. **Progress for the edition** reaches the state as `Event::Progress { label: BUILDING.into(), done, total }` (the master plan's `Progress` event), produced by `EditionProgress`, a core `EventSink` adapter.

**Files (4):**
- Create: `crates/ressac/src/tui/reader/journal.rs` (pure Journal model, outcome mapping, the progress sink; tests inline)
- Modify: `crates/ressac/src/tui/reader/state.rs` (Journal phase, start, rollover on `Tick`, `r` retry, `Enter` on overflow, `JobDone` handling; tests inline)
- Modify: `crates/ressac/src/tui/reader/native.rs` (Journal column, header, placeholder, end line, top-line progress; `TestBackend` snapshots inline)
- Modify: `crates/ressac/src/tui/reader/mod.rs` (`mod journal;`, executing `Command::BuildEdition` as a job)

Insta snapshot files under `crates/ressac/src/tui/reader/snapshots/` are added and do not count.

**Builds on (exact, from the code on `main` and the plan index):**
- `ressac_core::service::{Ressac, EditionOptions, EditionOutcome, EventSink, Event as CoreEvent}`; `Ressac::build_edition(&self, opts: EditionOptions, now: DateTime<Utc>, tz: &impl TimeZone, sink: &dyn EventSink, stop: Arc<AtomicBool>) -> Result<EditionOutcome>`; `EditionOutcome::{Built(EditionReport), NotDue, Offline, Interrupted}`; `EditionOptions { caps: Caps, rebuild: bool }`; core events `CoreEvent::Started { total }` and `CoreEvent::FeedDone { .. }`.
- `ressac_core::edition::{day_start, Caps, Section}`; `day_start<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Utc>` (04:00 local of the day `now` belongs to).
- `ressac_core::store::{JournalView, JournalItem, EntryRow, EditionRecord, EditionOverflowRow, Source}` (phase 1a); `JournalView { edition: EditionRecord, items: Vec<JournalItem>, overflow: Vec<EditionOverflowRow> }`, items in rank order, active feeds only; `EditionOverflowRow { section, feed_id, feed_title, page, hidden_count }`; `EditionRecord { id, built_at, window_start, window_end, unreachable }`.
- Phase 4b: `Command::{BuildEdition, LoadJournal, LoadSources, LoadEntries { .. }, LoadEntry(i64)}`, `Event::{Journal(Option<JournalView>), Progress { label, done, total }, JobDone(JobOutcome), Tick { now, local }}`, `JobOutcome::{EditionBuilt, EditionNotDue, EditionBuiltElsewhere, Offline, Failed(String), ..}`, `ReaderState::{on_key, on_event, source, focus}`.
- `crate::output::printable` for every feed-supplied string drawn.

**Mirrored from the app (exact strings, Global Constraints):** header `Friday 16 October · 43 entries · 12 read`, ` · 3 feeds unreachable` / ` · 1 feed unreachable`, `1 entry`; sections `Front Page`, `This Week`, `The Wire`; overflow `+12 more in Actualitté`; `That's the edition.`; `Nothing new this morning.`; `No edition yet`; `Offline: no edition yet`; progress `Building this morning's edition… 34/156`, short `Edition… 34/156`, `Edition…` before the total is known. Retry is offered in the offline, failed and due states (`JournalListView.offersRetry`).

---

## Sub-phase 7: the Journal

### Task 7.1: the pure Journal model (`journal.rs`)

- [ ] **Step 1: Write the failing tests.** Create `crates/ressac/src/tui/reader/journal.rs` with only the test module and the `mod journal;` line in `reader/mod.rs` (next to the other `mod` lines):

```rust
//! The Journal in the terminal: the same rows, header, placeholders and build outcomes as the
//! app's `JournalModel` and `JournalLayout` (spec §6). Pure, except `EditionProgress`, a core
//! event sink that turns the build's refresh events into progress.

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};
    use ressac_core::edition::Section;
    use ressac_core::store::{EditionOverflowRow, EditionRecord, EntryRow, JournalItem, JournalView};

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn row(entry_id: i64, feed_id: i64, read: bool) -> EntryRow {
        EntryRow {
            entry_id,
            feed_id,
            feed_title: format!("Feed {feed_id}"),
            title: Some(format!("Entry {entry_id}")),
            author: None,
            url: Some(format!("https://example.org/{entry_id}")),
            published_at: at("2026-10-16T05:00:00Z"),
            is_read: read,
            is_starred: false,
            has_fulltext: false,
        }
    }

    fn view(items: Vec<(i64, i64, Section, bool)>, overflow: Vec<(Section, i64, u32)>, unreachable: u32) -> JournalView {
        JournalView {
            edition: EditionRecord {
                id: 1,
                built_at: at("2026-10-16T06:10:00Z"),
                window_start: at("2026-10-15T06:10:00Z"),
                window_end: at("2026-10-16T06:10:00Z"),
                unreachable,
            },
            items: items
                .into_iter()
                .enumerate()
                .map(|(i, (entry, feed, section, read))| JournalItem { row: row(entry, feed, read), section, rank: i as u32 + 1 })
                .collect(),
            overflow: overflow
                .into_iter()
                .map(|(section, feed_id, hidden_count)| EditionOverflowRow {
                    section,
                    feed_id,
                    feed_title: if feed_id == 7 { "Actualitté".into() } else { format!("Feed {feed_id}") },
                    page: None,
                    hidden_count,
                })
                .collect(),
        }
    }

    fn paris() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    #[test]
    fn lines_follow_the_sections_with_overflow_after_the_feed_s_last_entry() {
        let v = view(
            vec![
                (10, 3, Section::Front, false),
                (20, 7, Section::Week, false),
                (21, 7, Section::Week, false),
                (22, 8, Section::Week, false),
                (30, 9, Section::Wire, false),
            ],
            vec![(Section::Week, 7, 12), (Section::Wire, 4, 2)],
            0,
        );
        assert_eq!(
            journal_lines(&v),
            vec![
                JournalLine::Heading(Section::Front),
                JournalLine::Entry(0),
                JournalLine::Heading(Section::Week),
                JournalLine::Entry(1),
                JournalLine::Entry(2),
                JournalLine::Overflow(0),
                JournalLine::Entry(3),
                JournalLine::Heading(Section::Wire),
                JournalLine::Entry(4),
                // Feed 4 has no listed entry: its overflow row closes the section.
                JournalLine::Overflow(1),
            ]
        );
    }

    #[test]
    fn an_empty_section_is_omitted() {
        let v = view(vec![(30, 9, Section::Wire, false)], vec![], 0);
        assert_eq!(journal_lines(&v), vec![JournalLine::Heading(Section::Wire), JournalLine::Entry(0)]);
    }

    #[test]
    fn the_header_matches_the_app() {
        let v = view(vec![(1, 1, Section::Wire, true), (2, 2, Section::Wire, false)], vec![], 0);
        assert_eq!(journal_header(&v, &paris()), "Friday 16 October · 2 entries · 1 read");
        let one = view(vec![(1, 1, Section::Wire, false)], vec![], 1);
        assert_eq!(journal_header(&one, &paris()), "Friday 16 October · 1 entry · 0 read · 1 feed unreachable");
        let three = view(vec![(1, 1, Section::Wire, false)], vec![], 3);
        assert!(journal_header(&three, &paris()).ends_with(" · 3 feeds unreachable"));
    }

    #[test]
    fn placeholders_match_the_app() {
        let full = view(vec![(1, 1, Section::Wire, false)], vec![], 0);
        let empty = view(vec![], vec![], 0);
        assert_eq!(placeholder(&JournalPhase::Building { done: 34, total: 156 }, Some(&full)).as_deref(),
                   Some("Building this morning's edition… 34/156"));
        assert_eq!(placeholder(&JournalPhase::Building { done: 0, total: 0 }, None).as_deref(),
                   Some("Building this morning's edition…"));
        assert_eq!(placeholder(&JournalPhase::Offline, Some(&full)).as_deref(), Some("Offline: no edition yet"));
        assert_eq!(placeholder(&JournalPhase::Failed("disk full".into()), None).as_deref(), Some("disk full"));
        assert_eq!(placeholder(&JournalPhase::Idle, None).as_deref(), Some("No edition yet"));
        assert_eq!(placeholder(&JournalPhase::Due, Some(&empty)).as_deref(), Some("Nothing new this morning."));
        assert_eq!(placeholder(&JournalPhase::Idle, Some(&full)), None);
    }

    #[test]
    fn the_end_line_shows_once_every_entry_is_read() {
        assert!(!is_finished(&view(vec![], vec![], 0)));
        assert!(!is_finished(&view(vec![(1, 1, Section::Wire, false)], vec![], 0)));
        assert!(is_finished(&view(vec![(1, 1, Section::Wire, true)], vec![], 0)));
    }

    #[test]
    fn the_progress_shortens_when_it_does_not_fit() {
        assert_eq!(progress_text(34, 156, 80), "Building this morning's edition… 34/156");
        assert_eq!(progress_text(34, 156, 30), "Edition… 34/156");
        assert_eq!(progress_text(0, 0, 80), "Building this morning's edition…");
        assert_eq!(progress_text(0, 0, 20), "Edition…");
    }

    #[test]
    fn retry_is_offered_offline_failed_and_due() {
        assert!(offers_retry(&JournalPhase::Offline));
        assert!(offers_retry(&JournalPhase::Failed("x".into())));
        assert!(offers_retry(&JournalPhase::Due));
        assert!(!offers_retry(&JournalPhase::Idle));
        assert!(!offers_retry(&JournalPhase::Building { done: 1, total: 2 }));
    }

    #[test]
    fn build_outcomes_map_to_job_outcomes() {
        use ressac_core::service::EditionOutcome;
        assert_eq!(outcome_of(Ok(EditionOutcome::NotDue)), JobOutcome::EditionNotDue);
        assert_eq!(outcome_of(Ok(EditionOutcome::Offline)), JobOutcome::Offline);
        assert_eq!(outcome_of(Ok(EditionOutcome::Interrupted)),
                   JobOutcome::Failed("The edition build was stopped.".into()));
        assert_eq!(outcome_of(Err(ressac_core::Error::Invalid("disk full".into()))),
                   JobOutcome::Failed("disk full".into()));
    }

    #[test]
    fn the_progress_sink_counts_the_refresh() {
        use std::sync::Mutex;
        let seen = Mutex::new(Vec::new());
        let sink = EditionProgress::new(|done, total| seen.lock().unwrap().push((done, total)));
        ressac_core::service::EventSink::emit(&sink, &ressac_core::service::Event::Started { total: 3 });
        for id in 1..=2 {
            ressac_core::service::EventSink::emit(
                &sink,
                &ressac_core::service::Event::FeedDone { feed_id: id, title: "t".into(), error: None },
            );
        }
        assert_eq!(*seen.lock().unwrap(), vec![(0, 3), (1, 3), (2, 3)]);
    }

    #[test]
    fn the_day_changes_at_four_local() {
        let local = |s: &str| paris().from_local_datetime(&s.parse().unwrap()).unwrap();
        assert_eq!(day_of(local("2026-10-17T03:59:00")), day_of(local("2026-10-16T12:00:00")));
        assert_ne!(day_of(local("2026-10-17T04:00:00")), day_of(local("2026-10-17T03:59:00")));
    }
}
```

`JobOutcome` must derive `PartialEq, Eq, Debug` for these asserts; if phase 4b did not derive them, add the derive in `state.rs` in this task.

- [ ] **Step 2: Run it to see it fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo test -p ressac --bin ressac tui::reader::journal
```

Expected: compile errors, `cannot find function journal_lines`, `cannot find type JournalPhase`, and the like.

- [ ] **Step 3: Implement.** Above the test module in `journal.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use ressac_core::edition::{Section, day_start};
use ressac_core::service::{EditionOutcome, Event as CoreEvent, EventSink};
use ressac_core::store::JournalView;

use super::state::JobOutcome;

/// The progress label of an edition build (`RunCoordinator.editionProgressLabel` in the app).
pub const BUILDING: &str = "Building this morning's edition…";
pub const END_LINE: &str = "That's the edition.";
pub const EMPTY_LINE: &str = "Nothing new this morning.";
pub const NO_EDITION: &str = "No edition yet";
pub const OFFLINE: &str = "Offline: no edition yet";
const STOPPED: &str = "The edition build was stopped.";

/// Where the Journal stands, as the app's `JournalModel.Phase`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JournalPhase {
    #[default]
    Idle,
    /// A build was turned away (another long job held the database): Retry is offered.
    Due,
    Building { done: usize, total: usize },
    Offline,
    Failed(String),
}

/// One row of the Journal column: a section heading, an entry (index into `items`) or an
/// overflow row (index into `overflow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalLine {
    Heading(Section),
    Entry(usize),
    Overflow(usize),
}

const SECTIONS: [Section; 3] = [Section::Front, Section::Week, Section::Wire];

/// The sections in order; inside each, the entries by rank and, right after a capped feed's
/// last entry, its overflow row (at the section's end when none of its entries is listed).
/// Empty sections are omitted. Same rule as the app's `JournalLayout.sections`.
pub fn journal_lines(view: &JournalView) -> Vec<JournalLine> {
    let mut lines = Vec::new();
    for section in SECTIONS {
        let mut rows: Vec<JournalLine> = view
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.section == section)
            .map(|(i, _)| JournalLine::Entry(i))
            .collect();
        for (o, more) in view.overflow.iter().enumerate().filter(|(_, m)| m.section == section) {
            let last = rows.iter().rposition(|line| {
                matches!(line, JournalLine::Entry(i) if view.items[*i].row.feed_id == more.feed_id)
            });
            let at = last.map_or(rows.len(), |p| p + 1);
            rows.insert(at, JournalLine::Overflow(o));
        }
        if !rows.is_empty() {
            lines.push(JournalLine::Heading(section));
            lines.extend(rows);
        }
    }
    lines
}

/// "Friday 16 October · 43 entries · 12 read", plus " · 3 feeds unreachable".
pub fn journal_header(view: &JournalView, tz: &impl TimeZone<Offset = impl std::fmt::Display>) -> String
where
{
    let date = view.edition.built_at.with_timezone(tz).format("%A %-d %B").to_string();
    let count = view.items.len();
    let read = view.items.iter().filter(|item| item.row.is_read).count();
    let mut parts = vec![
        date,
        if count == 1 { "1 entry".to_string() } else { format!("{count} entries") },
        format!("{read} read"),
    ];
    match view.edition.unreachable {
        0 => {}
        1 => parts.push("1 feed unreachable".into()),
        n => parts.push(format!("{n} feeds unreachable")),
    }
    parts.join(" · ")
}

/// Every entry read (an empty edition is never "finished": it shows `EMPTY_LINE` instead).
pub fn is_finished(view: &JournalView) -> bool {
    !view.items.is_empty() && view.items.iter().all(|item| item.row.is_read)
}

/// What the Journal column shows instead of the edition, or `None` to show it.
pub fn placeholder(phase: &JournalPhase, view: Option<&JournalView>) -> Option<String> {
    match phase {
        JournalPhase::Building { done, total } => Some(if *total > 0 {
            format!("{BUILDING} {done}/{total}")
        } else {
            BUILDING.to_string()
        }),
        JournalPhase::Offline => Some(OFFLINE.to_string()),
        JournalPhase::Failed(message) => Some(message.clone()),
        JournalPhase::Idle | JournalPhase::Due => match view {
            None => Some(NO_EDITION.to_string()),
            Some(v) if v.items.is_empty() => Some(EMPTY_LINE.to_string()),
            Some(_) => None,
        },
    }
}

/// The top line's progress: the full text when it fits `width` cells, else the short one.
pub fn progress_text(done: usize, total: usize, width: u16) -> String {
    let (full, short) = if total > 0 {
        (format!("{BUILDING} {done}/{total}"), format!("Edition… {done}/{total}"))
    } else {
        (BUILDING.to_string(), "Edition…".to_string())
    };
    if full.chars().count() <= usize::from(width) { full } else { short }
}

/// `r` builds again in these states (the app's Retry button).
pub fn offers_retry(phase: &JournalPhase) -> bool {
    matches!(phase, JournalPhase::Offline | JournalPhase::Failed(_) | JournalPhase::Due)
}

/// The edition job's result as the state sees it. `NotDue` covers both "already built this
/// morning" and "built by another process during the refresh" (the core does not tell them
/// apart); the state reloads the Journal quietly either way.
pub fn outcome_of(result: ressac_core::Result<EditionOutcome>) -> JobOutcome {
    match result {
        Ok(EditionOutcome::Built(_)) => JobOutcome::EditionBuilt,
        Ok(EditionOutcome::NotDue) => JobOutcome::EditionNotDue,
        Ok(EditionOutcome::Offline) => JobOutcome::Offline,
        Ok(EditionOutcome::Interrupted) => JobOutcome::Failed(STOPPED.into()),
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

/// The 04:00 day a local time belongs to: a change between two ticks is the rollover.
pub fn day_of(local: DateTime<FixedOffset>) -> DateTime<Utc> {
    day_start(local)
}

/// A core event sink for `Ressac::build_edition`: `Started` gives the total, each `FeedDone`
/// one more done. `report(done, total)` is called on each, from the job's thread.
pub struct EditionProgress<F: Fn(usize, usize) + Send + Sync> {
    total: AtomicUsize,
    done: AtomicUsize,
    report: F,
}

impl<F: Fn(usize, usize) + Send + Sync> EditionProgress<F> {
    pub fn new(report: F) -> Self {
        EditionProgress { total: AtomicUsize::new(0), done: AtomicUsize::new(0), report }
    }
}

impl<F: Fn(usize, usize) + Send + Sync> EventSink for EditionProgress<F> {
    fn emit(&self, event: &CoreEvent) {
        match event {
            CoreEvent::Started { total } => {
                self.total.store(*total, Ordering::Relaxed);
                self.done.store(0, Ordering::Relaxed);
                (self.report)(0, *total);
            }
            CoreEvent::FeedDone { .. } => {
                let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
                (self.report)(done, self.total.load(Ordering::Relaxed));
            }
            _ => {}
        }
    }
}
```

Notes for the implementer:
- `journal_header`'s bound: write it as `pub fn journal_header<Tz: TimeZone>(view: &JournalView, tz: &Tz) -> String where Tz::Offset: std::fmt::Display` if the bound above does not compile; chrono's `%A`/`%B` are English, which matches the app's `en_US_POSIX`. Callers pass `chrono::Local` in the app path and a `FixedOffset` in tests.
- Feed titles reach the screen through `printable` in the renderer (Task 7.3), not here.

- [ ] **Step 4: Run it to see it pass.**

```bash
cargo test -p ressac --bin ressac tui::reader::journal
```

Expected: 10 passed.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/journal.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/state.rs
git commit -m "Reader: the Journal's rows, header, placeholders and build outcomes, as the app's

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

(`state.rs` only if the `JobOutcome` derive had to be added.)

### Task 7.2: the Journal in the reader state

The state learns the Journal's phase, starts a session with a build, rebuilds at the 04:00 rollover, retries with `r`, opens a feed from an overflow row, and handles each build outcome. Review Focus 5 is tested here.

- [ ] **Step 1: Write the failing tests.** Append to the test module of `crates/ressac/src/tui/reader/state.rs` (reuse its existing helpers for keys and a default state; the ones below are self-contained in case they differ):

```rust
mod journal_tests {
    use super::super::*;
    use crate::tui::reader::journal::{JournalPhase, BUILDING};
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ressac_core::edition::Section;
    use ressac_core::store::{EditionOverflowRow, EditionRecord, EntryRow, JournalItem, JournalView, Source};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn paris() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }
    fn local(s: &str) -> DateTime<FixedOffset> {
        paris().from_local_datetime(&s.parse().unwrap()).unwrap()
    }
    fn tick(s: &str) -> Event {
        let l = local(s);
        Event::Tick { now: l.with_timezone(&Utc), local: l }
    }
    fn state() -> ReaderState {
        ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 40)
    }
    fn journal(read: bool) -> JournalView {
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        let row = |id: i64, feed: i64| EntryRow {
            entry_id: id, feed_id: feed, feed_title: format!("Feed {feed}"), title: Some(format!("E{id}")),
            author: None, url: None, published_at: at("2026-10-16T05:00:00Z"),
            is_read: read, is_starred: false, has_fulltext: false,
        };
        JournalView {
            edition: EditionRecord { id: 1, built_at: at("2026-10-16T06:00:00Z"), window_start: at("2026-10-15T06:00:00Z"),
                                     window_end: at("2026-10-16T06:00:00Z"), unreachable: 0 },
            items: vec![JournalItem { row: row(10, 7), section: Section::Week, rank: 1 }],
            overflow: vec![EditionOverflowRow { section: Section::Week, feed_id: 7, feed_title: "Actualitté".into(),
                                                page: Some(403), hidden_count: 12 }],
        }
    }

    #[test]
    fn a_session_starts_with_sources_journal_and_a_build() {
        let mut s = state();
        let commands = s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        assert!(commands.contains(&Command::LoadSources));
        assert!(commands.contains(&Command::LoadJournal));
        assert!(commands.contains(&Command::BuildEdition));
        assert_eq!(s.source(), Source::Journal);
        // Not shown as building until the refresh reports: a not-due build returns at once.
        assert_eq!(s.journal_phase(), &JournalPhase::Idle);
    }

    #[test]
    fn progress_shows_the_build_and_a_second_build_is_not_started() {
        let mut s = state();
        s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        s.on_event(Event::Progress { label: BUILDING.into(), done: 3, total: 156 });
        assert_eq!(s.journal_phase(), &JournalPhase::Building { done: 3, total: 156 });
        // The rollover or `r` while a build runs start nothing.
        assert!(!s.on_event(tick("2026-10-17T04:00:01")).contains(&Command::BuildEdition));
    }

    /// Review Focus 5: the reader stays open across 04:00; the app builds first; the reader's
    /// build is turned away by the core's due re-check; the Journal reloads, no error shown.
    #[test]
    fn across_four_the_reader_builds_and_quietly_reloads_when_the_app_was_first() {
        let mut s = state();
        s.start(local("2026-10-16T22:00:00").with_timezone(&Utc), local("2026-10-16T22:00:00"));
        s.on_event(Event::JobDone(JobOutcome::EditionNotDue));
        assert!(!s.on_event(tick("2026-10-17T03:59:59")).contains(&Command::BuildEdition));
        let at_four = s.on_event(tick("2026-10-17T04:00:00"));
        assert!(at_four.contains(&Command::BuildEdition));
        s.on_event(Event::Progress { label: BUILDING.into(), done: 0, total: 156 });
        let done = s.on_event(Event::JobDone(JobOutcome::EditionNotDue));
        assert!(done.contains(&Command::LoadJournal));
        assert_eq!(s.journal_phase(), &JournalPhase::Idle);
        assert_eq!(s.message.as_deref(), None);
    }

    #[test]
    fn offline_and_failed_offer_r_as_retry() {
        let mut s = state();
        s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        s.on_event(Event::JobDone(JobOutcome::Offline));
        assert_eq!(s.journal_phase(), &JournalPhase::Offline);
        assert!(s.on_key(key('r')).contains(&Command::BuildEdition));
        s.on_event(Event::JobDone(JobOutcome::Failed("disk full".into())));
        assert_eq!(s.journal_phase(), &JournalPhase::Failed("disk full".into()));
        assert!(s.on_key(key('r')).contains(&Command::BuildEdition));
        s.on_event(Event::JobDone(JobOutcome::EditionBuilt));
        assert_eq!(s.journal_phase(), &JournalPhase::Idle);
        // Idle: `r` is the ordinary refresh again.
        assert!(!s.on_key(key('r')).contains(&Command::BuildEdition));
    }

    #[test]
    fn a_built_edition_reloads_the_journal_and_the_counts() {
        let mut s = state();
        s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        let commands = s.on_event(Event::JobDone(JobOutcome::EditionBuilt));
        assert!(commands.contains(&Command::LoadJournal));
        assert!(commands.contains(&Command::LoadSources));
    }

    #[test]
    fn enter_on_an_overflow_row_opens_its_feed() {
        let mut s = state();
        s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        s.on_event(Event::Journal(Some(journal(false))));
        // Lines: heading (skipped), entry 10, overflow of feed 7. ↓ reaches the overflow row.
        s.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let commands = s.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(s.source(), Source::Feed(7));
        assert!(commands.iter().any(|c| matches!(c, Command::LoadEntries { source: Source::Feed(7), .. })));
    }

    #[test]
    fn j_and_k_walk_the_entries_and_skip_headings_and_overflow() {
        let mut s = state();
        s.start(local("2026-10-16T08:00:00").with_timezone(&Utc), local("2026-10-16T08:00:00"));
        s.on_event(Event::Journal(Some(journal(false))));
        assert_eq!(s.selected_entry(), Some(10));
        s.on_key(key('j'));
        assert_eq!(s.selected_entry(), Some(10), "the overflow row is not an entry");
    }
}
```

`Command` must derive `PartialEq` and `Debug` for `contains`; `ReaderState::message(&self) -> Option<&str>` is the top/bottom-line message accessor. If phase 4b named it differently, use its name and say so in the report.

- [ ] **Step 2: Run them to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::reader::state::tests::journal_tests
```

Expected: errors on `start`, `journal_phase`, and failing asserts on `BuildEdition`/`LoadJournal`.

- [ ] **Step 3: Implement in `state.rs`.**

1. Fields on `ReaderState`:

```rust
    // `journal: Option<JournalView>` already exists (phase 4b, public): not redeclared.
    journal_phase: crate::tui::reader::journal::JournalPhase,
    /// True from `BuildEdition` until its `JobDone`: no second build meanwhile.
    edition_job: bool,
    /// The 04:00 day of the last tick; a change is the rollover.
    last_day: Option<chrono::DateTime<chrono::Utc>>,
    /// The Journal column's cursor, an index into `journal_lines(view)`.
    journal_cursor: usize,
```

initialised to `None`, `JournalPhase::Idle`, `false`, `None`, `0` in `new`.

2. Methods:

```rust
    /// The first commands of a session: sources, the Journal, and an edition build (the core
    /// returns at once when today's edition exists, so nothing shows as building).
    pub fn start(&mut self, now: DateTime<Utc>, local: DateTime<FixedOffset>) -> Vec<Command> {
        self.now = Some(now);
        self.local = Some(local);
        self.last_day = Some(journal::day_of(local));
        let mut commands = vec![Command::LoadSources, Command::LoadJournal, self.reload()];
        commands.extend(self.build_edition());
        commands
    }
    // Phase 4b's `the_reader_opens_on_the_journal` expects the first three commands; this
    // phase appends `Command::BuildEdition` to its expected list in the same commit.

    pub fn journal_phase(&self) -> &JournalPhase {
        &self.journal_phase
    }

    pub fn journal(&self) -> Option<&JournalView> {
        self.journal.as_ref()
    }

    fn build_edition(&mut self) -> Option<Command> {
        if self.edition_job {
            return None;
        }
        self.edition_job = true;
        Some(Command::BuildEdition)
    }

    /// The rollover: the first tick of a new 04:00 day starts a build.
    fn on_tick_journal(&mut self, local: DateTime<FixedOffset>) -> Vec<Command> {
        let day = journal::day_of(local);
        let rolled = self.last_day.is_some_and(|d| d != day);
        self.last_day = Some(day);
        if rolled { self.build_edition().into_iter().collect() } else { Vec::new() }
    }

    fn on_edition_done(&mut self, outcome: &JobOutcome) -> Vec<Command> {
        self.edition_job = false;
        match outcome {
            JobOutcome::EditionBuilt | JobOutcome::EditionNotDue => {
                self.journal_phase = JournalPhase::Idle;
                vec![Command::LoadJournal, Command::LoadSources]
            }
            JobOutcome::Offline => {
                self.journal_phase = JournalPhase::Offline;
                Vec::new()
            }
            JobOutcome::Failed(message) => {
                self.journal_phase = JournalPhase::Failed(message.clone());
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
```

3. In `on_event`:
   - `Event::Journal(view)` → `self.journal = view;` clamp `journal_cursor` to the lines; if the cursor sits on a heading, move it to the first entry line; when the source is the Journal, select the cursor's entry.
   - `Event::Progress { label, done, total } if label == journal::BUILDING` → `self.journal_phase = JournalPhase::Building { done, total }` (other labels keep phase 5b's handling).
   - `Event::JobDone(outcome)` for the edition outcomes (`EditionBuilt | EditionNotDue | Offline`) and for `Failed` **while `edition_job` is true** → `on_edition_done`. A `Failed` from another job keeps phase 5b's handling.
   - `Event::Tick { local, .. }` → also `on_tick_journal(local)` (append its commands to phase 5b's tick handling).

4. In `on_key`, when the source is `Source::Journal` and the entries column has the focus:
   - `r`: `if journal::offers_retry(&self.journal_phase) { return self.build_edition().into_iter().collect(); }` before the ordinary refresh.
   - `↓` / `↑`: move `journal_cursor` over the lines, skipping `Heading`; an `Entry` line selects that entry (`LoadEntry`), an `Overflow` line selects nothing.
   - `j` / `k`, `Space`: move over `Entry` lines only (next/previous entry; Space after the reader's last page moves to the next unread `Entry`, never leaving the Journal), as for other sources.
   - `Enter` on an `Overflow(o)` line: switch the source to `Source::Feed(view.overflow[o].feed_id)` and return its `LoadEntries { source, filter: self.filter, after: None }` (same as choosing that feed in the sources column).

- [ ] **Step 4: Run the tests.**

```bash
cargo test -p ressac --bin ressac tui::reader::state
```

Expected: the new `journal_tests` pass, and every earlier state test still passes.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/state.rs
git commit -m "Reader: the Journal builds at start and at 04:00, r retries, an overflow row opens its feed

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7.3: the Journal in the Native rendering

- [ ] **Step 1: Write the failing snapshot tests.** Append to `native.rs`'s test module (reuse its `draw(state, width, height) -> String` helper from phase 5a; it renders through `TestBackend`):

```rust
    fn journal_state(read: bool, unreachable: u32) -> ReaderState {
        // Build a state on the Journal with one Front entry, two Week entries of Actualitté
        // with an overflow of 12, and one Wire entry with a control character in its title.
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 140, 30);
        let local = chrono::FixedOffset::east_opt(7200).unwrap().with_ymd_and_hms(2026, 10, 16, 8, 0, 0).unwrap();
        s.start(local.with_timezone(&chrono::Utc), local);
        s.on_event(Event::JobDone(JobOutcome::EditionNotDue));
        s.on_event(Event::Journal(Some(sample_journal(read, unreachable))));
        s
    }

    #[test]
    fn journal_wide() {
        insta::assert_snapshot!(draw(&journal_state(false, 3), 140, 30));
    }

    #[test]
    fn journal_finished_shows_the_end_line() {
        let out = draw(&journal_state(true, 0), 140, 30);
        assert!(out.contains("That's the edition."));
        insta::assert_snapshot!(out);
    }

    #[test]
    fn journal_building_progress_on_the_top_line() {
        let mut s = journal_state(false, 0);
        s.on_event(Event::Progress { label: journal::BUILDING.into(), done: 34, total: 156 });
        let wide = draw(&s, 140, 30);
        assert!(wide.lines().next().unwrap().contains("Building this morning's edition… 34/156"));
        let narrow = draw(&s, 70, 30);
        assert!(narrow.lines().next().unwrap().contains("Edition… 34/156"));
    }

    #[test]
    fn journal_placeholders() {
        let mut s = ReaderState::new(TerminalSettings { look: Look::Native, images: true }, 100, 20);
        s.on_event(Event::Journal(None));
        assert!(draw(&s, 100, 20).contains("No edition yet"));
        s.on_event(Event::JobDone(JobOutcome::Offline));
        let offline = draw(&s, 100, 20);
        assert!(offline.contains("Offline: no edition yet"));
        assert!(offline.contains("r retry"));
    }

    #[test]
    fn journal_escape_sequences_never_reach_the_terminal() {
        let out = draw(&journal_state(false, 0), 140, 30);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}') && !out.contains('\u{9b}'));
    }
```

with a `sample_journal(read: bool, unreachable: u32) -> JournalView` helper in the same test module building: Front: entry 1 of feed 3 "Le Monde diplomatique"; Week: entries 2 and 3 of feed 7 "Actualitté" and overflow `(Week, 7, "Actualitté", Some(403), 12)`; Wire: entry 4 of feed 9 titled `"Bourse\u{1b}]0;x\u{7} du jour"`; `built_at` 2026-10-16T06:10:00Z; every entry's `is_read = read`.

- [ ] **Step 2: Run to see them fail.**

```bash
cargo test -p ressac --bin ressac tui::reader::native::tests::journal
```

Expected: failures (no Journal rendering yet) and new pending snapshots.

- [ ] **Step 3: Implement in `native.rs`.**

- When `state.source() == Source::Journal`, the entries column draws, in this order:
  1. the header line `journal_header(view, &chrono::Local)` in bold (in tests the state's zone: pass the zone the renderer already uses for dates; if phase 5a renders dates with `chrono::Local`, make the header's zone a parameter of `render` through the state, `ReaderState::zone()`, or snapshot tests become machine-dependent — fix the zone to `FixedOffset +02:00` in tests);
  2. if `placeholder(state.journal_phase(), state.journal())` is `Some(text)`: that text, dimmed, centred vertically in the column, and below it `r retry` when `offers_retry(phase)`; nothing else;
  3. otherwise the `journal_lines(view)`: `Heading(section)` as `Front Page` / `This Week` / `The Wire` bold, with a blank line before (except the first); `Entry(i)` as the usual entry row of phase 5a (unread dot, title, feed), from `view.items[i].row`; `Overflow(o)` as `+{hidden_count} more in {feed_title}` dimmed and italic, indented two cells; the cursor's line in reverse video when the column has the focus;
  4. `END_LINE` after the last line when `is_finished(view)`.
- Every feed-supplied string (titles, feed titles) through `crate::output::printable` and truncated to the column width (the existing row helper of phase 5a does both; reuse it).
- The top line: when `state.journal_phase()` is `Building { done, total }`, the progress text is `progress_text(done, total, top_width_left)` where `top_width_left` is the width the top line has after the source name; otherwise phase 5a's top line.
- The bottom key hint on the Journal adds `r retry` in the offline, failed and due states, else `r refresh`.

- [ ] **Step 4: Review and accept the snapshots, run the module.**

```bash
cargo insta test -p ressac --review   # or: cargo test … then cargo insta review
cargo test -p ressac --bin ressac tui::reader::native
```

Check in the snapshots: the sections in order, the overflow row right after Actualitté's last entry, the header with ` · 3 feeds unreachable`, `�` in place of the escape characters, nothing past the column's right edge. Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/native.rs crates/ressac/src/tui/reader/snapshots/
git commit -m "Reader: the Journal column, its placeholders and the build progress in Native

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7.4: running the build job

- [ ] **Step 1: Write the failing test.** In `reader/mod.rs`'s test module (phase 5b set up a way to execute commands against a test `Ressac` on a temp database with a wiremock server; reuse it). If phase 5b's command executor is a function `execute(app, command, tx)`, test it directly:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn build_edition_reports_progress_then_the_outcome() {
        // A database with two feeds served by wiremock (phase 5b's helper), no edition yet.
        let (app, _server, _dir) = test_app_with_two_feeds().await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        execute(&app, Command::BuildEdition, tx.clone()).await;
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert!(matches!(events.first(), Some(Event::Progress { label, done: 0, total: 2 }) if label == journal::BUILDING));
        assert!(matches!(events.last(), Some(Event::JobDone(JobOutcome::EditionBuilt))));
        // A second build the same morning: not due, no progress at all.
        execute(&app, Command::BuildEdition, tx).await;
        let again: Vec<Event> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(again.len(), 1);
        assert!(matches!(again[0], Event::JobDone(JobOutcome::EditionNotDue)));
    }
```

Adapt the helper names to phase 5b's (`test_app_with_two_feeds`, `execute`, the channel type); keep the assertions.

- [ ] **Step 2: Run to see it fail.**

```bash
cargo test -p ressac --bin ressac tui::reader::tests::build_edition_reports_progress_then_the_outcome
```

Expected: `BuildEdition` not handled (or `todo!`/no event).

- [ ] **Step 3: Implement** the `Command::BuildEdition` arm of the executor:

```rust
        Command::BuildEdition => {
            let progress_tx = tx.clone();
            let sink = journal::EditionProgress::new(move |done, total| {
                let _ = progress_tx.send(Event::Progress { label: journal::BUILDING.into(), done, total });
            });
            let result = app
                .build_edition(
                    EditionOptions { caps: Caps::default(), rebuild: false },
                    Utc::now(),
                    &chrono::Local,
                    &sink,
                    stop.clone(),
                )
                .await;
            let _ = tx.send(Event::JobDone(journal::outcome_of(result)));
        }
```

`stop` is the session's stop flag (phase 5b; quitting raises it so a running build ends as `Interrupted` without writing). Run it as a background job exactly like phase 5b runs refreshes, so that the other sources stay readable.

- [ ] **Step 4: Run, then the whole gate.**

```bash
cargo test -p ressac --bin ressac tui::reader
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
```

Expected: all green.

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac/src/tui/reader/mod.rs
git commit -m "Reader: the edition build runs as a background job with its progress

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Gate 7

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `INSTA_UPDATE=no cargo test --workspace`: green.
- Manual, throwaway database only (never `~/Library/...`):

```bash
T=$(mktemp -d); cp tests/fixtures/contract.sqlite "$T/r.sqlite"
HOME="$T" cargo run -q -p ressac -- --db "$T/r.sqlite" read
```

The reader opens on the Journal (the fixture's hand-written edition, or a fresh build if none is due); `q` quits.
