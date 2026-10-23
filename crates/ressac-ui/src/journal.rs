//! The Journal's rules, look-agnostic (the Mac's `JournalList` and `JournalModel`): the layout
//! of the morning edition by section, its header line and placeholders.

use std::fmt::Display;

use chrono::TimeZone;
use ressac_core::edition::Section;
use ressac_core::model::FeedId;
use ressac_core::store::{EditionOverflowRow, JournalView};

use crate::runs;

pub const BUILDING: &str = runs::EDITION_LABEL;
pub const END_LINE: &str = "That's the edition.";
pub const EMPTY_LINE: &str = "Nothing new this morning.";
pub const NO_EDITION: &str = "No edition yet";
pub const OFFLINE: &str = "Offline: no edition yet";
pub const RETRY: &str = "Retry";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entries::EntryList;
    use crate::runs::DirectWrite;
    use crate::test_db::{TestDb, at, today};
    use chrono::FixedOffset;
    use ressac_core::edition::Section;
    use ressac_core::model::FeedId;
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
}
