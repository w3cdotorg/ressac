//! The Journal's rules, look-agnostic (the Mac's `JournalList` and `JournalModel`): the layout
//! of the morning edition by section, its header line and placeholders.

use std::fmt::Display;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use ressac_core::edition::{Section, day_start};
use ressac_core::model::FeedId;
use ressac_core::service::{EditionOutcome, Event};
use ressac_core::store::{EditionOverflowRow, JournalView, Store};

use crate::runs::{self, COULD_NOT_READ, RunEnd, RunFailure};

pub const BUILDING: &str = runs::EDITION_LABEL;
pub const END_LINE: &str = "That's the edition.";
pub const EMPTY_LINE: &str = "Nothing new this morning.";
pub const NO_EDITION: &str = "No edition yet";
pub const OFFLINE: &str = "Offline: no edition yet";
pub const RETRY: &str = "Retry";
pub const OFFLINE_REFUSAL: &str = "The network is unreachable: no edition built.";
pub const INTERRUPTED: &str = "Interrupted.";
/// After a failure, automatic triggers leave the edition alone for this long.
pub const AUTOMATIC_RETRY_DELAY: TimeDelta = TimeDelta::minutes(15);

pub const SECTIONS: [Section; 3] = [Section::Front, Section::Week, Section::Wire];

pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Front => "Front Page",
        Section::Week => "This Week",
        Section::Wire => "The Wire",
    }
}

/// A row of the Journal list: an entry, or the "+N more in Feed" row after its feed.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalRow {
    Entry(i64),
    Overflow(EditionOverflowRow),
}

impl JournalRow {
    /// A stable identity: `entry-<id>` or `overflow-<section>-<feed>`.
    pub fn id(&self) -> String {
        match self {
            JournalRow::Entry(id) => format!("entry-{id}"),
            JournalRow::Overflow(row) => {
                format!("overflow-{}-{}", row.section.as_str(), row.feed_id)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalSection {
    pub section: Section,
    pub rows: Vec<JournalRow>,
}

/// The list laid out by section. `listed`: (entry id, feed id) in list order. An overflow row
/// goes right after the last listed entry of its feed in its section, at the section's end
/// when the feed has none; sections without rows are omitted; no edition, no sections.
pub fn sections(listed: &[(i64, FeedId)], view: Option<&JournalView>) -> Vec<JournalSection> {
    let Some(view) = view else {
        return Vec::new();
    };
    SECTIONS
        .into_iter()
        .filter_map(|section| {
            let entries: Vec<(i64, FeedId)> = listed
                .iter()
                .copied()
                .filter(|(id, _)| {
                    view.items
                        .iter()
                        .any(|item| item.row.entry_id == *id && item.section == section)
                })
                .collect();
            let overflow: Vec<&EditionOverflowRow> = view
                .overflow
                .iter()
                .filter(|o| o.section == section)
                .collect();
            let mut rows = Vec::with_capacity(entries.len() + overflow.len());
            for (index, (id, feed)) in entries.iter().enumerate() {
                rows.push(JournalRow::Entry(*id));
                let last_of_feed = !entries[index + 1..].iter().any(|(_, f)| f == feed);
                if last_of_feed {
                    rows.extend(
                        overflow
                            .iter()
                            .filter(|o| o.feed_id == *feed)
                            .map(|o| JournalRow::Overflow((*o).clone())),
                    );
                }
            }
            rows.extend(
                overflow
                    .iter()
                    .filter(|o| !entries.iter().any(|(_, f)| *f == o.feed_id))
                    .map(|o| JournalRow::Overflow((*o).clone())),
            );
            (!rows.is_empty()).then_some(JournalSection { section, rows })
        })
        .collect()
}

/// "Friday 16 October · 3 entries · 1 read · 2 feeds unreachable", the day read in `tz`.
pub fn header<Tz: TimeZone>(view: &JournalView, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    let count = view.items.len();
    let mut parts = vec![
        view.edition
            .built_at
            .with_timezone(tz)
            .format("%A %-d %B")
            .to_string(),
        if count == 1 {
            "1 entry".to_string()
        } else {
            format!("{count} entries")
        },
        format!("{} read", read_count(view)),
    ];
    match view.edition.unreachable {
        0 => {}
        1 => parts.push("1 feed unreachable".into()),
        n => parts.push(format!("{n} feeds unreachable")),
    }
    parts.join(" · ")
}

pub fn read_count(view: &JournalView) -> usize {
    view.items.iter().filter(|item| item.row.is_read).count()
}

/// At least one entry and all read: the end line shows.
pub fn is_finished(view: &JournalView) -> bool {
    !view.items.is_empty() && read_count(view) == view.items.len()
}

/// "+12 more in Actualitté".
pub fn overflow_label(row: &EditionOverflowRow) -> String {
    format!("+{} more in {}", row.hidden_count, row.feed_title)
}

/// Where the morning edition stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// The edition is due but its build was turned away (another long run is active).
    Due,
    Building {
        done: usize,
        total: usize,
    },
    Offline,
    Failed(String),
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Due => "due",
            Phase::Building { .. } => "building",
            Phase::Offline => "offline",
            Phase::Failed(_) => "failed",
        }
    }

    /// The edition could not be built: the placeholder offers Retry.
    pub fn offers_retry(&self) -> bool {
        matches!(self, Phase::Offline | Phase::Failed(_) | Phase::Due)
    }
}

/// What replaces the list, if anything.
pub fn placeholder(phase: &Phase, view: Option<&JournalView>) -> Option<String> {
    match phase {
        Phase::Building { done, total } if *total > 0 => Some(format!("{BUILDING} {done}/{total}")),
        Phase::Building { .. } => Some(BUILDING.to_string()),
        Phase::Offline => Some(OFFLINE.to_string()),
        Phase::Failed(message) => Some(message.clone()),
        Phase::Idle | Phase::Due => match view {
            None => Some(NO_EDITION.to_string()),
            Some(v) if v.items.is_empty() => Some(EMPTY_LINE.to_string()),
            Some(_) => None,
        },
    }
}

/// The 04:00 day start seen at a clock check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockCheck {
    pub day_start: DateTime<Utc>,
}

impl ClockCheck {
    pub fn at<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> ClockCheck {
        ClockCheck {
            day_start: day_start(now.with_timezone(tz)),
        }
    }
}

/// The day start of `now`, in the zone as it is now, differs from the one `last_check` saw:
/// 04:00 passed, the machine resumed across it, or the time zone changed.
pub fn due_after<Tz: TimeZone>(last_check: &ClockCheck, now: DateTime<Utc>, tz: &Tz) -> bool {
    ClockCheck::at(now, tz) != *last_check
}

/// No edition built since the 04:00 day start of `now` in `tz`.
pub fn is_due<Tz: TimeZone>(
    store: &Store,
    now: DateTime<Utc>,
    tz: &Tz,
) -> ressac_core::Result<bool> {
    store.edition_due(day_start(now.with_timezone(tz)))
}

/// An outcome the run reads as a failure: the banner's sentence, and whether it is a refusal
/// (shown without Show Log).
pub fn edition_failure(outcome: &EditionOutcome) -> Option<RunFailure> {
    match outcome {
        EditionOutcome::Offline => Some(RunFailure {
            message: OFFLINE_REFUSAL.into(),
            refusal: true,
        }),
        EditionOutcome::Interrupted => Some(RunFailure {
            message: INTERRUPTED.into(),
            refusal: false,
        }),
        EditionOutcome::Built(_) | EditionOutcome::NotDue => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Launch, the app becoming active, the minute check: waits out a failure.
    Automatic,
    /// The Retry button: never waits.
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Start the build as a long run.
    Build,
}

/// The phase machine (the Mac's `JournalModel`): one check or build at a time, the 15-minute
/// wait after a failure (in memory only) and the clock check.
#[derive(Debug, Clone, Default)]
pub struct JournalModel {
    phase: Phase,
    busy: bool,
    not_before: Option<DateTime<Utc>>,
    clock: Option<ClockCheck>,
}

impl JournalModel {
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// A trigger fired: true when the caller should now check whether an edition is due
    /// (`is_due`, then `checked`). Records the clock check either way, so the minute tick
    /// fires once per day start.
    pub fn begin<Tz: TimeZone>(&mut self, trigger: Trigger, now: DateTime<Utc>, tz: &Tz) -> bool {
        self.clock = Some(ClockCheck::at(now, tz));
        if self.busy {
            return false;
        }
        if trigger == Trigger::Automatic
            && let Some(not_before) = self.not_before
            && now < not_before
            && not_before - now <= AUTOMATIC_RETRY_DELAY
        {
            return false;
        }
        self.busy = true;
        true
    }

    /// The due check's answer.
    pub fn checked(&mut self, due: ressac_core::Result<bool>) -> Step {
        match due {
            Ok(false) => {
                self.phase = Phase::Idle;
                self.not_before = None;
                self.busy = false;
                Step::Nothing
            }
            Ok(true) => {
                self.phase = Phase::Building { done: 0, total: 0 };
                Step::Build
            }
            Err(error) => {
                self.phase = Phase::Failed(format!("{COULD_NOT_READ}{error}"));
                self.busy = false;
                Step::Nothing
            }
        }
    }

    /// A build event, counted only while building.
    pub fn event(&mut self, event: &Event) {
        let Phase::Building { done, total } = &mut self.phase else {
            return;
        };
        match event {
            Event::Started { total: n } => {
                *done = 0;
                *total = *n;
            }
            Event::FeedDone { .. } => *done += 1,
            _ => {}
        }
    }

    pub fn finished(&mut self, end: RunEnd<EditionOutcome>, now: DateTime<Utc>) {
        self.busy = false;
        self.phase = match end {
            RunEnd::Done(EditionOutcome::Built(_) | EditionOutcome::NotDue) => Phase::Idle,
            RunEnd::Done(EditionOutcome::Offline) => Phase::Offline,
            RunEnd::Done(EditionOutcome::Interrupted) => Phase::Failed(INTERRUPTED.into()),
            RunEnd::Refused => Phase::Due,
            RunEnd::Failed(failure) => Phase::Failed(failure.message),
        };
        self.not_before = matches!(self.phase, Phase::Offline | Phase::Failed(_))
            .then(|| now + AUTOMATIC_RETRY_DELAY);
    }

    /// The once-a-minute wall-clock check: no check yet, or the 04:00 day start moved since
    /// the last one (04:00 passed, a resume across it, a time-zone change).
    pub fn clock_tick<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> bool {
        self.clock
            .as_ref()
            .is_none_or(|last| due_after(last, now, tz))
    }

    /// The wait after an offline or failed build is over and nothing is running: the minute
    /// check replays the automatic build (`begin` records the clock even when it declines, so
    /// the day check alone would never fire again).
    pub fn wait_is_over(&self, now: DateTime<Utc>) -> bool {
        !self.busy
            && matches!(self.phase, Phase::Offline | Phase::Failed(_))
            && self.not_before.is_some_and(|not_before| now >= not_before)
    }

    /// What the once-a-minute tick asks: the day turned, or a build is due for a retry.
    pub fn minute_check<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> bool {
        self.clock_tick(now, tz) || self.wait_is_over(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entries::EntryList;
    use crate::live::Invalidation;
    use crate::runs::{Admission, Banner, RunCoordinator, RunFailure, RunRequest};
    use crate::runs::{DirectWrite, RunEnd};
    use crate::test_db::{Paris, TestDb, at, today};
    use EditionOutcome::Built;
    use chrono::FixedOffset;
    use chrono::TimeDelta;
    use ressac_core::clock::today_start;
    use ressac_core::edition::Section;
    use ressac_core::model::FeedId;
    use ressac_core::service::{EditionOutcome, EditionReport, Event};
    use ressac_core::store::{
        EditionOverflowRow, EditionRecord, EntryRow, JournalItem, JournalView, ListFilter,
        ListQuery, Source,
    };

    use Section::{Front, Week, Wire};

    fn view(
        items: &[(i64, Section, u32, FeedId)],
        overflow: &[(Section, FeedId, u32, &str)],
        read: &[i64],
        unreachable: u32,
    ) -> JournalView {
        let built_at = at("2026-10-16T05:12:00Z");
        JournalView {
            edition: EditionRecord {
                id: 1,
                built_at,
                window_start: built_at - chrono::TimeDelta::hours(24),
                window_end: built_at,
                unreachable,
            },
            items: items
                .iter()
                .map(|&(id, section, rank, feed)| JournalItem {
                    row: EntryRow {
                        entry_id: id,
                        feed_id: feed,
                        feed_title: format!("Feed {feed}"),
                        title: Some(format!("Entry {id}")),
                        author: None,
                        url: None,
                        published_at: built_at,
                        is_read: read.contains(&id),
                        is_starred: false,
                        has_fulltext: false,
                    },
                    section,
                    rank,
                })
                .collect(),
            overflow: overflow
                .iter()
                .map(
                    |&(section, feed_id, hidden_count, title)| EditionOverflowRow {
                        section,
                        feed_id,
                        feed_title: title.into(),
                        page: None,
                        hidden_count,
                    },
                )
                .collect(),
        }
    }

    #[test]
    fn entries_go_under_their_section_and_overflow_follows_its_feed() {
        let v = view(
            &[
                (1, Front, 1, 5),
                (2, Week, 2, 3),
                (3, Week, 3, 3),
                (4, Week, 4, 4),
                (5, Wire, 5, 1),
            ],
            &[
                (Week, 3, 2, "Feed 3"),
                (Wire, 1, 12, "Feed 1"),
                (Wire, 9, 4, "Feed 9"),
            ],
            &[],
            0,
        );
        let s = sections(&[(1, 5), (2, 3), (3, 3), (4, 4), (5, 1)], Some(&v));
        assert_eq!(
            s.iter().map(|s| s.section).collect::<Vec<_>>(),
            [Front, Week, Wire]
        );
        let ids: Vec<Vec<String>> = s
            .iter()
            .map(|s| s.rows.iter().map(JournalRow::id).collect())
            .collect();
        assert_eq!(
            ids,
            [
                vec!["entry-1"],
                vec!["entry-2", "entry-3", "overflow-week-3", "entry-4"],
                vec!["entry-5", "overflow-wire-1", "overflow-wire-9"]
            ]
        ); // feed 9 has no row: its overflow closes the section
        // A filtered list (entry 3 hidden by the unread filter): the overflow follows feed 3's last listed entry.
        let filtered = sections(&[(1, 5), (2, 3), (4, 4), (5, 1)], Some(&v));
        assert_eq!(
            filtered[1]
                .rows
                .iter()
                .map(JournalRow::id)
                .collect::<Vec<_>>(),
            ["entry-2", "overflow-week-3", "entry-4"]
        );
    }

    #[test]
    fn a_section_without_rows_is_omitted() {
        let v = view(&[(2, Week, 1, 3)], &[], &[], 0);
        assert_eq!(
            sections(&[(2, 3)], Some(&v))
                .iter()
                .map(|s| s.section)
                .collect::<Vec<_>>(),
            [Week]
        );
        assert!(sections(&[(2, 3)], None).is_empty());
        assert!(
            sections(&[(7, 3)], Some(&v)).is_empty(),
            "an entry outside the edition is not the Journal's"
        );
        assert_eq!(
            SECTIONS.map(section_title),
            ["Front Page", "This Week", "The Wire"]
        );
    }

    #[test]
    fn retry_is_offered_whenever_the_edition_could_not_be_built() {
        for phase in [
            Phase::Offline,
            Phase::Failed("Database is locked.".into()),
            Phase::Due,
        ] {
            assert!(phase.offers_retry(), "{phase:?}");
        }
        for phase in [Phase::Idle, Phase::Building { done: 1, total: 2 }] {
            assert!(!phase.offers_retry(), "{phase:?}");
        }
        assert_eq!(
            [
                Phase::Idle,
                Phase::Due,
                Phase::Building { done: 0, total: 0 },
                Phase::Offline,
                Phase::Failed(String::new())
            ]
            .map(|p| p.as_str()),
            ["idle", "due", "building", "offline", "failed"]
        );
    }

    #[test]
    fn the_header_counts_entries_read_entries_and_unreachable_feeds() {
        let paris = FixedOffset::east_opt(2 * 3600).unwrap();
        let items = [(1, Front, 1, 1), (2, Wire, 2, 2), (3, Wire, 3, 3)];
        assert_eq!(
            header(&view(&items, &[], &[2], 0), &paris),
            "Friday 16 October · 3 entries · 1 read"
        );
        assert_eq!(
            header(&view(&items, &[], &[], 3), &paris),
            "Friday 16 October · 3 entries · 0 read · 3 feeds unreachable"
        );
        assert_eq!(
            header(&view(&[(1, Front, 1, 1)], &[], &[], 1), &paris),
            "Friday 16 October · 1 entry · 0 read · 1 feed unreachable"
        );
    }

    #[test]
    fn placeholders_say_what_the_journal_is_waiting_for() {
        let full = view(&[(1, Front, 1, 1)], &[], &[], 0);
        let empty = view(&[], &[], &[], 0);
        assert_eq!(
            placeholder(&Phase::Building { done: 0, total: 0 }, Some(&full)).as_deref(),
            Some("Building this morning's edition…")
        );
        assert_eq!(
            placeholder(
                &Phase::Building {
                    done: 34,
                    total: 156
                },
                None
            )
            .as_deref(),
            Some("Building this morning's edition… 34/156")
        );
        assert_eq!(
            placeholder(&Phase::Offline, Some(&full)).as_deref(),
            Some("Offline: no edition yet")
        );
        assert_eq!(
            placeholder(&Phase::Failed("error: disk full".into()), None).as_deref(),
            Some("error: disk full")
        );
        assert_eq!(
            placeholder(&Phase::Idle, Some(&empty)).as_deref(),
            Some("Nothing new this morning.")
        );
        assert_eq!(
            placeholder(&Phase::Idle, None).as_deref(),
            Some("No edition yet")
        );
        assert_eq!(placeholder(&Phase::Idle, Some(&full)), None);
        assert_eq!(placeholder(&Phase::Due, Some(&full)), None);
        assert_eq!(END_LINE, "That's the edition.");
        assert!(!is_finished(&empty), "an empty edition is never finished");
        assert!(is_finished(&view(&[(1, Front, 1, 1)], &[], &[1], 0)));
    }

    #[test]
    fn overflow_rows_name_the_feed() {
        let row = |hidden, title: &str| EditionOverflowRow {
            section: Wire,
            feed_id: 4,
            feed_title: title.into(),
            page: None,
            hidden_count: hidden,
        };
        assert_eq!(
            overflow_label(&row(12, "Actualitté")),
            "+12 more in Actualitté"
        );
        assert_eq!(overflow_label(&row(1, "Fabula")), "+1 more in Fabula");
    }

    #[test]
    fn a_feed_buried_after_the_build_leaves_every_journal_surface() {
        let db = TestDb::new();
        let buried = db.entry(1, "2026-10-15T08:00:00.000Z", false);
        let first = db.entry(2, "2026-10-15T08:00:00.000Z", false);
        let second = db.entry(8, "2026-10-15T08:00:00.000Z", false);
        db.edition(&[(buried, "front"), (first, "wire"), (second, "wire")]);
        db.execute("UPDATE feed SET state = 'buried' WHERE id = 1");
        let store = db.store();
        let v = store.journal().unwrap().unwrap();
        assert_eq!(
            v.items.iter().map(|i| i.row.entry_id).collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(store.sources(today()).unwrap().counts.journal, 2);
        assert!(!is_finished(&v) && read_count(&v) == 0);
        DirectWrite::MarkAllRead {
            source: Source::Journal,
            today_start: today(),
        }
        .apply(&store, at("2026-10-16T09:00:00Z"))
        .unwrap();
        assert!(is_finished(&store.journal().unwrap().unwrap()));
        assert_eq!(
            db.read_at(buried),
            None,
            "the buried feed's entry is left alone"
        );
    }

    #[test]
    fn an_edition_whose_every_feed_was_buried_reads_as_empty() {
        let db = TestDb::new();
        let only = db.entry(1, "2026-10-15T08:00:00.000Z", false);
        db.edition(&[(only, "wire")]);
        db.execute("UPDATE feed SET state = 'buried' WHERE id = 1");
        let v = db.store().journal().unwrap().unwrap();
        assert!(v.items.is_empty() && !is_finished(&v));
        assert_eq!(
            placeholder(&Phase::Idle, Some(&v)).as_deref(),
            Some("Nothing new this morning.")
        );
    }

    #[test]
    fn mark_all_read_in_the_journal_leaves_the_overflow_unread() {
        let db = TestDb::new();
        let first = db.entry(1, "2026-10-15T08:00:00.000Z", false);
        let second = db.entry(2, "2026-10-15T08:00:00.000Z", false);
        let left_out = db.entry(1, "2026-10-14T08:00:00.000Z", false);
        let edition = db.edition(&[(first, "wire"), (second, "wire")]);
        db.overflow(edition, 1, "wire", 1);
        DirectWrite::MarkAllRead {
            source: Source::Journal,
            today_start: today(),
        }
        .apply(&db.store(), at("2026-10-16T09:00:00Z"))
        .unwrap();
        assert!(db.read_at(first).is_some() && db.read_at(second).is_some());
        assert_eq!(db.read_at(left_out), None);
    }

    #[test]
    fn space_stays_inside_the_edition_in_rank_order() {
        let db = TestDb::new();
        let first = db.entry(1, "2026-10-15T06:00:00.000Z", false);
        let second = db.entry(2, "2026-10-15T09:00:00.000Z", false);
        db.entry(8, "2026-10-15T12:00:00.000Z", false); // newer, unread, not in the edition
        db.edition(&[(first, "front"), (second, "wire")]);
        let store = db.store();
        let mut list = EntryList::new(ListQuery {
            source: Source::Journal,
            filter: ListFilter::All,
            search: None,
        });
        list.restart(&store, None, today()).unwrap();
        assert_eq!(
            list.next_unread(&store, Some(first), today()).unwrap(),
            Some(second)
        );
        assert_eq!(
            list.next_unread(&store, Some(second), today()).unwrap(),
            None
        );
    }

    fn built() -> EditionOutcome {
        Built(EditionReport {
            id: 1,
            entries: 0,
            front: 0,
            week: 0,
            wire: 0,
            overflow: 0,
            unreachable: 0,
        })
    }

    fn failed(m: &str) -> RunEnd<EditionOutcome> {
        RunEnd::Failed(RunFailure {
            message: m.into(),
            refusal: false,
        })
    }

    fn mins(n: i64) -> TimeDelta {
        TimeDelta::minutes(n)
    }

    const T0: &str = "2026-10-16T07:30:00Z";

    /// Master Review Focus 5.
    #[test]
    fn due_after_notices_a_resume_across_four_and_a_zone_change() {
        let night = ClockCheck::at(at("2026-10-15T23:30:00Z"), &Paris); // 01:30 Paris
        assert!(!due_after(&night, at("2026-10-16T01:59:00Z"), &Paris)); // 03:59
        assert!(due_after(&night, at("2026-10-16T05:30:00Z"), &Paris)); // asleep six hours, awake at 07:30
        let morning = ClockCheck::at(at("2026-10-16T03:00:00Z"), &Paris); // 05:00 Paris
        assert!(!due_after(&morning, at("2026-10-16T09:00:00Z"), &Paris)); // six hours that do not cross 04:00
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        assert!(due_after(&morning, at("2026-10-16T03:01:00Z"), &tokyo)); // the zone changed: 12:01 in Tokyo, another day start
        assert!(!due_after(
            &ClockCheck::at(at("2026-10-16T03:01:00Z"), &tokyo),
            at("2026-10-16T03:02:00Z"),
            &tokyo
        ));
    }

    #[test]
    fn a_zone_change_while_running_makes_the_edition_due_at_the_next_tick() {
        let db = TestDb::new();
        db.built("2026-10-15T20:30:00.000Z"); // 05:30 in Tokyo, Friday's edition there
        let store = db.store();
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        let mut m = JournalModel::default();
        assert!(m.begin(Trigger::Automatic, at("2026-10-16T05:59:00Z"), &tokyo));
        assert_eq!(
            m.checked(is_due(&store, at("2026-10-16T05:59:00Z"), &tokyo)),
            Step::Nothing
        );
        assert!(
            m.clock_tick(at("2026-10-16T06:00:00Z"), &Paris),
            "moved to Paris: 08:00 there, another 04:00"
        );
        assert!(m.begin(Trigger::Automatic, at("2026-10-16T06:00:00Z"), &Paris));
        assert_eq!(
            m.checked(is_due(&store, at("2026-10-16T06:00:00Z"), &Paris)),
            Step::Build
        );
    }

    #[test]
    fn today_moves_to_the_new_midnight_after_a_zone_change() {
        let mut seen = Invalidation::default();
        assert!(!seen.day_seen(today_start(
            at("2026-10-16T06:00:00Z").with_timezone(&Paris)
        )));
        assert!(!seen.day_seen(today_start(
            at("2026-10-16T06:01:00Z").with_timezone(&Paris)
        )));
        let los_angeles = FixedOffset::west_opt(7 * 3600).unwrap(); // still 15 October there
        assert!(seen.day_seen(today_start(
            at("2026-10-16T06:02:00Z").with_timezone(&los_angeles)
        )));
    }

    #[test]
    fn the_day_turns_at_four_local_time_across_dst() {
        let tick =
            |last: &str, now: &str| due_after(&ClockCheck::at(at(last), &Paris), at(now), &Paris);
        assert!(tick("2026-10-25T02:59:00Z", "2026-10-25T03:00:00Z")); // 25 October: 04:00 Paris is 03:00 UTC
        assert!(!tick("2026-10-25T02:58:00Z", "2026-10-25T02:59:00Z"));
        assert!(tick("2026-03-29T01:59:00Z", "2026-03-29T02:00:00Z")); // 29 March: 04:00 Paris is 02:00 UTC
        assert!(!tick("2026-03-29T01:58:00Z", "2026-03-29T01:59:00Z"));
        assert!(tick("2026-10-17T01:59:00Z", "2026-10-17T02:00:00Z")); // summer time: 02:00 UTC
    }

    #[test]
    fn the_minute_tick_turns_once_at_four() {
        let mut m = JournalModel::default();
        let mut now = at(T0); // 09:30 Paris
        assert!(m.begin(Trigger::Automatic, now, &Paris));
        m.checked(Ok(false));
        let mut turned = Vec::new();
        while now < at("2026-10-17T07:30:00Z") {
            now += mins(1);
            if m.clock_tick(now, &Paris) {
                turned.push(now);
                assert!(m.begin(Trigger::Automatic, now, &Paris));
                m.checked(Ok(false));
            }
        }
        assert_eq!(turned, [at("2026-10-17T02:00:00Z")]);
    }

    #[test]
    fn the_minute_check_replays_the_build_once_the_wait_after_a_failure_is_over() {
        let mut m = JournalModel::default();
        let start = at(T0);
        assert!(m.begin(Trigger::Automatic, start, &Paris));
        assert_eq!(m.checked(Ok(true)), Step::Build);
        m.finished(RunEnd::Done(EditionOutcome::Offline), start);
        // `begin` recorded the clock: the day has not turned, so only the wait can fire.
        assert!(!m.minute_check(start + mins(1), &Paris));
        assert!(!m.minute_check(start + mins(14), &Paris));
        let ready = start + mins(15);
        assert!(m.minute_check(ready, &Paris));
        assert!(m.begin(Trigger::Automatic, ready, &Paris));
        assert!(!m.minute_check(ready, &Paris), "busy: one check at a time");
        assert_eq!(m.checked(Ok(true)), Step::Build);
        m.finished(
            RunEnd::Failed(RunFailure {
                message: "boom".into(),
                refusal: false,
            }),
            ready,
        );
        assert!(
            !m.minute_check(ready + mins(14), &Paris),
            "a new wait began"
        );
        assert!(m.minute_check(ready + mins(15), &Paris));
        // Built: nothing more to replay.
        assert!(m.begin(Trigger::Automatic, ready + mins(15), &Paris));
        assert_eq!(m.checked(Ok(true)), Step::Build);
        m.finished(RunEnd::Done(built()), ready + mins(15));
        assert!(!m.minute_check(ready + mins(60), &Paris));
    }

    #[test]
    fn an_edition_built_after_four_is_not_due_and_one_built_before_belongs_to_yesterday() {
        let db = TestDb::new();
        let store = db.store();
        assert!(is_due(&store, at(T0), &Paris).unwrap(), "no edition at all");
        db.built("2026-10-16T01:30:00.000Z"); // 03:30 Paris: yesterday's
        assert!(is_due(&store, at(T0), &Paris).unwrap());
        db.built("2026-10-16T02:30:00.000Z"); // 04:30 Paris
        assert!(!is_due(&store, at(T0), &Paris).unwrap());
    }

    #[test]
    fn before_four_the_day_is_still_yesterday() {
        let db = TestDb::new();
        db.built("2026-10-15T05:00:00.000Z"); // yesterday 07:00
        assert!(!is_due(&db.store(), at("2026-10-16T01:30:00Z"), &Paris).unwrap()); // 03:30 Paris
    }

    #[test]
    fn a_due_edition_is_built_and_shown() {
        let mut m = JournalModel::default();
        assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
        assert_eq!(m.checked(Ok(true)), Step::Build);
        assert_eq!(m.phase(), &Phase::Building { done: 0, total: 0 });
        m.finished(RunEnd::Done(built()), at(T0));
        assert_eq!(m.phase(), &Phase::Idle);
        assert!(!m.is_busy());
    }

    #[test]
    fn building_reports_its_progress() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.event(&Event::Started { total: 3 });
        for feed_id in [1, 2] {
            m.event(&Event::FeedDone {
                feed_id,
                title: "A".into(),
                error: None,
            });
        }
        assert_eq!(m.phase(), &Phase::Building { done: 2, total: 3 });
    }

    #[test]
    fn offline_keeps_the_edition_due_and_retry_builds_it() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(RunEnd::Done(EditionOutcome::Offline), at(T0));
        assert_eq!(m.phase(), &Phase::Offline);
        assert!(m.begin(Trigger::Retry, at(T0), &Paris), "Retry never waits");
        assert_eq!(m.checked(Ok(true)), Step::Build);
    }

    #[test]
    fn an_edition_built_meanwhile_elsewhere_is_simply_shown() {
        assert_eq!(edition_failure(&EditionOutcome::NotDue), None);
        let mut runs = RunCoordinator::default();
        let request = RunRequest::BuildEdition { rebuild: false };
        runs.admit(&request);
        assert_eq!(
            runs.conclude(&request, Ok(EditionOutcome::NotDue), edition_failure),
            RunEnd::Done(EditionOutcome::NotDue)
        );
        assert_eq!(runs.banner(), None);
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(RunEnd::Done(EditionOutcome::NotDue), at(T0));
        assert_eq!(m.phase(), &Phase::Idle);
    }

    #[test]
    fn an_offline_refusal_keeps_its_banner() {
        let mut runs = RunCoordinator::default();
        let request = RunRequest::BuildEdition { rebuild: false };
        runs.admit(&request);
        runs.event(
            &request,
            &Event::NetworkUnreachable {
                failed: 11,
                attempted: 11,
            },
        );
        assert_eq!(
            runs.conclude(&request, Ok(EditionOutcome::Offline), edition_failure),
            RunEnd::Done(EditionOutcome::Offline)
        );
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "The network is unreachable: no edition built.".into(),
                show_log: false
            })
        );
    }

    #[test]
    fn a_failed_or_stopped_build_leaves_the_edition_due() {
        for (end, message) in [
            (failed("disk full"), "disk full"),
            (
                failed("The job stopped unexpectedly."),
                "The job stopped unexpectedly.",
            ),
            (RunEnd::Done(EditionOutcome::Interrupted), "Interrupted."),
        ] {
            let mut m = JournalModel::default();
            m.begin(Trigger::Automatic, at(T0), &Paris);
            m.checked(Ok(true));
            m.finished(end, at(T0));
            assert_eq!(m.phase(), &Phase::Failed(message.into()));
            let mut relaunched = JournalModel::default(); // the wait is in memory only
            assert!(relaunched.begin(Trigger::Automatic, at(T0) + mins(1), &Paris));
        }
        assert_eq!(
            edition_failure(&EditionOutcome::Interrupted),
            Some(RunFailure {
                message: "Interrupted.".into(),
                refusal: false
            })
        );
    }

    #[test]
    fn automatic_triggers_wait_fifteen_minutes_after_a_failure() {
        for end in [failed("disk full"), RunEnd::Done(EditionOutcome::Offline)] {
            let mut m = JournalModel::default();
            m.begin(Trigger::Automatic, at(T0), &Paris);
            m.checked(Ok(true));
            m.finished(end, at(T0));
            assert!(!m.begin(Trigger::Automatic, at(T0) + mins(5), &Paris));
            assert!(m.begin(Trigger::Automatic, at(T0) + mins(16), &Paris));
        }
    }

    #[test]
    fn retry_builds_during_the_window_and_a_manual_failure_restarts_it() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(failed("disk full"), at(T0));
        assert!(m.begin(Trigger::Retry, at(T0) + mins(10), &Paris));
        m.checked(Ok(true));
        m.finished(failed("disk full"), at(T0) + mins(10));
        assert!(
            !m.begin(Trigger::Automatic, at(T0) + mins(20), &Paris),
            "10 minutes after the manual failure"
        );
        assert!(m.begin(Trigger::Automatic, at(T0) + mins(26), &Paris));
    }

    #[test]
    fn a_success_clears_the_window() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(failed("disk full"), at(T0));
        m.begin(Trigger::Retry, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(RunEnd::Done(built()), at(T0));
        assert!(m.begin(Trigger::Automatic, at(T0) + mins(1), &Paris));
    }

    #[test]
    fn finding_the_edition_not_due_clears_the_window() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(failed("disk full"), at(T0));
        assert!(m.begin(Trigger::Retry, at(T0) + mins(2), &Paris));
        assert_eq!(m.checked(Ok(false)), Step::Nothing); // built in a terminal meanwhile
        assert_eq!(m.phase(), &Phase::Idle);
        assert!(m.begin(Trigger::Automatic, at(T0) + mins(3), &Paris));
    }

    #[test]
    fn a_clock_moved_backwards_expires_the_window() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(failed("disk full"), at(T0));
        assert!(m.begin(Trigger::Automatic, at(T0) - mins(60), &Paris));
    }

    #[test]
    fn a_build_turned_away_by_another_long_run_stays_due() {
        let mut runs = RunCoordinator::default();
        runs.admit(&RunRequest::FetchAll);
        assert_eq!(
            runs.admit(&RunRequest::BuildEdition { rebuild: false }),
            Admission::Refused
        );
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        m.checked(Ok(true));
        m.finished(RunEnd::Refused, at(T0));
        assert_eq!(m.phase(), &Phase::Due);
        assert!(m.phase().offers_retry());
        assert!(
            m.begin(Trigger::Automatic, at(T0) + mins(1), &Paris),
            "no wait after a refusal"
        );
    }

    #[test]
    fn a_second_trigger_during_a_build_does_nothing() {
        let mut m = JournalModel::default();
        assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
        assert!(!m.begin(Trigger::Automatic, at(T0), &Paris));
        m.checked(Ok(true));
        assert!(!m.begin(Trigger::Retry, at(T0), &Paris));
        m.finished(RunEnd::Done(built()), at(T0));
        assert!(m.begin(Trigger::Automatic, at(T0), &Paris));
    }

    #[test]
    fn a_database_error_while_checking_fails_with_its_sentence() {
        let mut m = JournalModel::default();
        m.begin(Trigger::Automatic, at(T0), &Paris);
        assert_eq!(
            m.checked(Err(ressac_core::Error::Invalid("disk I/O error".into()))),
            Step::Nothing
        );
        assert_eq!(
            m.phase(),
            &Phase::Failed("Could not read the database: disk I/O error".into())
        );
        assert!(!m.is_busy());
    }
}
