//! The Journal in the terminal: the same rows, header, placeholders and build states as the
//! app's `JournalModel` and `JournalLayout` (spec §6). Pure: no terminal, no clock, no database.
//! The build itself runs as a job (`jobs::ProgressSink` reports its progress, `jobs::edition_outcome`
//! its result).

use std::fmt::Display;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use ressac_core::edition::{Section, day_start};
use ressac_core::store::JournalView;

/// The progress label of an edition build (`RunCoordinator.editionProgressLabel` in the app).
pub const BUILDING: &str = "Building this morning's edition…";
pub const END_LINE: &str = "That's the edition.";
pub const EMPTY_LINE: &str = "Nothing new this morning.";
pub const NO_EDITION: &str = "No edition yet";
pub const OFFLINE: &str = "Offline: no edition yet";
/// The short form of the progress, used when `BUILDING` does not fit.
const BUILDING_SHORT: &str = "Edition…";

/// Where the Journal stands, as the app's `JournalModel.Phase`. Only the edition job's own
/// outcome moves it: another job's failure is none of its business.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JournalPhase {
    #[default]
    Idle,
    Building {
        done: usize,
        total: usize,
    },
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

/// `Front Page`, `This Week`, `The Wire`.
pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Front => "Front Page",
        Section::Week => "This Week",
        Section::Wire => "The Wire",
    }
}

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
        for (o, more) in view
            .overflow
            .iter()
            .enumerate()
            .filter(|(_, m)| m.section == section)
        {
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

/// "Friday 16 October · 43 entries · 12 read", plus " · 3 feeds unreachable". The date is the
/// edition's, in `tz` (the reader passes its clock's offset).
pub fn journal_header<Tz: TimeZone>(view: &JournalView, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    let date = view
        .edition
        .built_at
        .with_timezone(tz)
        .format("%A %-d %B")
        .to_string();
    let count = view.items.len();
    let read = view.items.iter().filter(|item| item.row.is_read).count();
    let mut parts = vec![
        date,
        if count == 1 {
            "1 entry".to_string()
        } else {
            format!("{count} entries")
        },
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
        JournalPhase::Idle => match view {
            None => Some(NO_EDITION.to_string()),
            Some(v) if v.items.is_empty() => Some(EMPTY_LINE.to_string()),
            Some(_) => None,
        },
    }
}

/// The top line's progress: the full text when it fits `width` cells, else the short one.
/// Before the total is known there is no count.
pub fn progress_text(done: usize, total: usize, width: u16) -> String {
    let (full, short) = if total > 0 {
        (
            format!("{BUILDING} {done}/{total}"),
            format!("{BUILDING_SHORT} {done}/{total}"),
        )
    } else {
        (BUILDING.to_string(), BUILDING_SHORT.to_string())
    };
    if full.chars().count() <= usize::from(width) {
        full
    } else {
        short
    }
}

/// `r` builds again in these states (the app's Retry button).
pub fn offers_retry(phase: &JournalPhase) -> bool {
    matches!(phase, JournalPhase::Offline | JournalPhase::Failed(_))
}

/// The 04:00 day a local time belongs to: a change between two ticks is the rollover.
pub fn day_of(local: DateTime<FixedOffset>) -> DateTime<Utc> {
    day_start(local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::store::{EditionOverflowRow, EditionRecord, EntryRow, JournalItem};

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

    fn view(
        items: Vec<(i64, i64, Section, bool)>,
        overflow: Vec<(Section, i64, u32)>,
        unreachable: u32,
    ) -> JournalView {
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
                .map(|(i, (entry, feed, section, read))| JournalItem {
                    row: row(entry, feed, read),
                    section,
                    rank: i as u32 + 1,
                })
                .collect(),
            overflow: overflow
                .into_iter()
                .map(|(section, feed_id, hidden_count)| EditionOverflowRow {
                    section,
                    feed_id,
                    feed_title: if feed_id == 7 {
                        "Actualitté".into()
                    } else {
                        format!("Feed {feed_id}")
                    },
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
        assert_eq!(
            journal_lines(&v),
            vec![JournalLine::Heading(Section::Wire), JournalLine::Entry(0)]
        );
    }

    #[test]
    fn the_sections_are_titled_as_in_the_app() {
        assert_eq!(section_title(Section::Front), "Front Page");
        assert_eq!(section_title(Section::Week), "This Week");
        assert_eq!(section_title(Section::Wire), "The Wire");
    }

    #[test]
    fn the_header_matches_the_app() {
        let v = view(
            vec![(1, 1, Section::Wire, true), (2, 2, Section::Wire, false)],
            vec![],
            0,
        );
        assert_eq!(
            journal_header(&v, &paris()),
            "Friday 16 October · 2 entries · 1 read"
        );
        let one = view(vec![(1, 1, Section::Wire, false)], vec![], 1);
        assert_eq!(
            journal_header(&one, &paris()),
            "Friday 16 October · 1 entry · 0 read · 1 feed unreachable"
        );
        let three = view(vec![(1, 1, Section::Wire, false)], vec![], 3);
        assert!(journal_header(&three, &paris()).ends_with(" · 3 feeds unreachable"));
    }

    #[test]
    fn the_header_date_is_the_edition_s_in_the_given_zone() {
        // 23:30 UTC on the 16th is already the 17th at +02:00.
        let mut v = view(vec![(1, 1, Section::Wire, false)], vec![], 0);
        v.edition.built_at = at("2026-10-16T23:30:00Z");
        assert!(journal_header(&v, &paris()).starts_with("Saturday 17 October"));
        assert!(journal_header(&v, &Utc).starts_with("Friday 16 October"));
    }

    #[test]
    fn placeholders_match_the_app() {
        let full = view(vec![(1, 1, Section::Wire, false)], vec![], 0);
        let empty = view(vec![], vec![], 0);
        assert_eq!(
            placeholder(
                &JournalPhase::Building {
                    done: 34,
                    total: 156
                },
                Some(&full)
            )
            .as_deref(),
            Some("Building this morning's edition… 34/156")
        );
        assert_eq!(
            placeholder(&JournalPhase::Building { done: 0, total: 0 }, None).as_deref(),
            Some("Building this morning's edition…")
        );
        assert_eq!(
            placeholder(&JournalPhase::Offline, Some(&full)).as_deref(),
            Some("Offline: no edition yet")
        );
        assert_eq!(
            placeholder(&JournalPhase::Failed("disk full".into()), None).as_deref(),
            Some("disk full")
        );
        assert_eq!(
            placeholder(&JournalPhase::Idle, None).as_deref(),
            Some("No edition yet")
        );
        assert_eq!(
            placeholder(&JournalPhase::Idle, Some(&empty)).as_deref(),
            Some("Nothing new this morning.")
        );
        assert_eq!(placeholder(&JournalPhase::Idle, Some(&full)), None);
    }

    #[test]
    fn the_end_line_shows_once_every_entry_is_read() {
        assert!(!is_finished(&view(vec![], vec![], 0)));
        assert!(!is_finished(&view(
            vec![(1, 1, Section::Wire, false)],
            vec![],
            0
        )));
        assert!(is_finished(&view(
            vec![(1, 1, Section::Wire, true)],
            vec![],
            0
        )));
    }

    #[test]
    fn the_progress_shortens_when_it_does_not_fit() {
        assert_eq!(
            progress_text(34, 156, 80),
            "Building this morning's edition… 34/156"
        );
        assert_eq!(progress_text(34, 156, 30), "Edition… 34/156");
        assert_eq!(progress_text(0, 0, 80), "Building this morning's edition…");
        assert_eq!(progress_text(0, 0, 20), "Edition…");
    }

    #[test]
    fn retry_is_offered_offline_and_failed() {
        assert!(offers_retry(&JournalPhase::Offline));
        assert!(offers_retry(&JournalPhase::Failed("x".into())));
        assert!(!offers_retry(&JournalPhase::Idle));
        assert!(!offers_retry(&JournalPhase::Building { done: 1, total: 2 }));
    }

    #[test]
    fn the_day_changes_at_four_local() {
        let local = |s: &str| paris().from_local_datetime(&s.parse().unwrap()).unwrap();
        assert_eq!(
            day_of(local("2026-10-17T03:59:00")),
            day_of(local("2026-10-16T12:00:00"))
        );
        assert_ne!(
            day_of(local("2026-10-17T04:00:00")),
            day_of(local("2026-10-17T03:59:00"))
        );
    }
}
