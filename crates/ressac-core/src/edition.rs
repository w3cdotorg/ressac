//! The morning edition's composition (spec §6): which section an entry goes to, the caps per
//! feed, the order and the ranks. Pure: `store::edition` selects the candidates and writes the
//! result.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{CadenceGroup, FeedId};

/// Entries of one weekly feed kept in an edition.
pub const CAP_WEEK: usize = 3;
/// Entries of one daily feed kept in an edition: its most recent.
pub const CAP_WIRE: usize = 1;
/// An entry published more than this many days before the window opens is never eligible.
pub const BACKLOG_GUARD_DAYS: i64 = 7;
/// A day starts at this local hour: an edition built before it belongs to the day before.
pub const DAY_STARTS_AT_HOUR: u32 = 4;
/// The window of the very first edition.
pub const FIRST_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Front,
    Week,
    Wire,
}

impl Section {
    /// Monthly and Rare feeds make the front page; Silent feeds never appear.
    pub fn for_group(group: CadenceGroup) -> Option<Section> {
        match group {
            CadenceGroup::Monthly | CadenceGroup::Rare => Some(Section::Front),
            CadenceGroup::Weekly => Some(Section::Week),
            CadenceGroup::Daily => Some(Section::Wire),
            CadenceGroup::Silent => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Section::Front => "front",
            Section::Week => "week",
            Section::Wire => "wire",
        }
    }

    /// The inverse of `as_str`, for the `section` columns.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by store::edition in sub-phase 2b")
    )]
    pub(crate) fn parse(s: &str) -> Option<Section> {
        match s {
            "front" => Some(Section::Front),
            "week" => Some(Section::Week),
            "wire" => Some(Section::Wire),
            _ => None,
        }
    }

    fn cap(self, caps: Caps) -> usize {
        match self {
            Section::Front => usize::MAX,
            Section::Week => caps.week,
            Section::Wire => caps.wire,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub week: usize,
    pub wire: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps {
            week: CAP_WEEK,
            wire: CAP_WIRE,
        }
    }
}

/// One eligible entry, with what the composition needs to place it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_page: Option<u16>,
    /// The feed's effective group (override, else computed).
    pub group: CadenceGroup,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placed {
    pub entry_id: i64,
    pub section: Section,
    pub rank: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overflow {
    pub feed_id: FeedId,
    pub section: Section,
    pub hidden_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composition {
    pub entries: Vec<Placed>,
    pub overflow: Vec<Overflow>,
}

/// Sections front → week → wire; inside a section feeds by page (feeds without a page last,
/// then by feed id), entries by `published_at` then `entry_id`, newest first; caps per feed and
/// per edition; ranks from 1 across the whole edition.
pub fn compose(candidates: &[Candidate], caps: Caps) -> Composition {
    // (section, has no page, page, feed) orders the feeds; the value is that feed's entries.
    let mut feeds: BTreeMap<(Section, bool, u16, FeedId), Vec<&Candidate>> = BTreeMap::new();
    for c in candidates {
        if let Some(section) = Section::for_group(c.group) {
            let key = (
                section,
                c.feed_page.is_none(),
                c.feed_page.unwrap_or(0),
                c.feed_id,
            );
            feeds.entry(key).or_default().push(c);
        }
    }
    let mut out = Composition::default();
    let mut rank = 0u32;
    for ((section, _, _, feed_id), mut entries) in feeds {
        entries.sort_by(|a, b| {
            b.published_at
                .cmp(&a.published_at)
                .then(b.entry_id.cmp(&a.entry_id))
        });
        let cap = section.cap(caps);
        for c in entries.iter().take(cap) {
            rank += 1;
            out.entries.push(Placed {
                entry_id: c.entry_id,
                section,
                rank,
            });
        }
        let hidden = entries.len().saturating_sub(cap);
        if hidden > 0 {
            out.overflow.push(Overflow {
                feed_id,
                section,
                hidden_count: u32::try_from(hidden).unwrap_or(u32::MAX),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CadenceGroup;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn cand(
        entry_id: i64,
        feed_id: FeedId,
        feed_page: Option<u16>,
        group: CadenceGroup,
        published: &str,
    ) -> Candidate {
        Candidate {
            entry_id,
            feed_id,
            feed_page,
            group,
            published_at: at(published),
        }
    }

    fn placed(c: &Composition) -> Vec<(i64, Section, u32)> {
        c.entries
            .iter()
            .map(|p| (p.entry_id, p.section, p.rank))
            .collect()
    }

    #[test]
    fn groups_map_to_sections() {
        assert_eq!(Section::for_group(CadenceGroup::Rare), Some(Section::Front));
        assert_eq!(
            Section::for_group(CadenceGroup::Monthly),
            Some(Section::Front)
        );
        assert_eq!(
            Section::for_group(CadenceGroup::Weekly),
            Some(Section::Week)
        );
        assert_eq!(Section::for_group(CadenceGroup::Daily), Some(Section::Wire));
        assert_eq!(Section::for_group(CadenceGroup::Silent), None);
        for s in [Section::Front, Section::Week, Section::Wire] {
            assert_eq!(Section::parse(s.as_str()), Some(s));
        }
        assert_eq!(Section::parse("sports"), None);
        assert_eq!(serde_json::to_string(&Section::Wire).unwrap(), r#""wire""#);
    }

    #[test]
    fn default_caps_are_the_spec_constants() {
        assert_eq!(
            Caps::default(),
            Caps {
                week: CAP_WEEK,
                wire: CAP_WIRE
            }
        );
        assert_eq!((CAP_WEEK, CAP_WIRE), (3, 1));
    }

    #[test]
    fn sections_come_in_order_and_ranks_run_across_the_edition() {
        let c = compose(
            &[
                cand(
                    1,
                    10,
                    Some(401),
                    CadenceGroup::Daily,
                    "2026-10-16T05:00:00Z",
                ),
                cand(
                    2,
                    20,
                    Some(402),
                    CadenceGroup::Weekly,
                    "2026-10-16T04:00:00Z",
                ),
                cand(3, 30, Some(403), CadenceGroup::Rare, "2026-10-15T04:00:00Z"),
                cand(
                    4,
                    40,
                    Some(404),
                    CadenceGroup::Monthly,
                    "2026-10-15T03:00:00Z",
                ),
            ],
            Caps::default(),
        );
        assert_eq!(
            placed(&c),
            [
                (3, Section::Front, 1),
                (4, Section::Front, 2),
                (2, Section::Week, 3),
                (1, Section::Wire, 4)
            ]
        );
        assert!(c.overflow.is_empty());
    }

    #[test]
    fn feeds_follow_page_numbers_with_unnumbered_feeds_last() {
        let c = compose(
            &[
                cand(1, 7, None, CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(2, 9, Some(512), CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(3, 8, Some(203), CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(4, 3, None, CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
            ],
            Caps::default(),
        );
        // Pages 203 then 512, then the feeds without a page by feed id (3 before 7).
        assert_eq!(
            placed(&c).iter().map(|p| p.0).collect::<Vec<_>>(),
            [3, 2, 4, 1]
        );
    }

    #[test]
    fn caps_apply_per_feed_and_the_rest_is_counted_as_overflow() {
        let mut cands = Vec::new();
        for i in 0..5 {
            // Feed 1, daily: five entries an hour apart; entry 104 is the newest.
            cands.push(cand(
                100 + i,
                1,
                Some(401),
                CadenceGroup::Daily,
                &format!("2026-10-16T0{i}:00:00Z"),
            ));
            // Feed 2, weekly: five entries.
            cands.push(cand(
                200 + i,
                2,
                Some(402),
                CadenceGroup::Weekly,
                &format!("2026-10-1{i}T08:00:00Z"),
            ));
        }
        // Feed 3, rare: front page is uncapped.
        for i in 0..4 {
            cands.push(cand(
                300 + i,
                3,
                Some(403),
                CadenceGroup::Rare,
                &format!("2026-10-1{i}T09:00:00Z"),
            ));
        }
        let c = compose(&cands, Caps::default());
        assert_eq!(
            placed(&c),
            [
                (303, Section::Front, 1),
                (302, Section::Front, 2),
                (301, Section::Front, 3),
                (300, Section::Front, 4),
                (204, Section::Week, 5),
                (203, Section::Week, 6),
                (202, Section::Week, 7),
                (104, Section::Wire, 8),
            ]
        );
        assert_eq!(
            c.overflow,
            [
                Overflow {
                    feed_id: 2,
                    section: Section::Week,
                    hidden_count: 2
                },
                Overflow {
                    feed_id: 1,
                    section: Section::Wire,
                    hidden_count: 4
                },
            ]
        );
    }

    #[test]
    fn same_date_entries_keep_a_stable_order_by_id() {
        let c = compose(
            &[
                cand(
                    5,
                    1,
                    Some(401),
                    CadenceGroup::Weekly,
                    "2026-10-16T05:00:00Z",
                ),
                cand(
                    9,
                    1,
                    Some(401),
                    CadenceGroup::Weekly,
                    "2026-10-16T05:00:00Z",
                ),
            ],
            Caps::default(),
        );
        assert_eq!(placed(&c).iter().map(|p| p.0).collect::<Vec<_>>(), [9, 5]);
    }

    #[test]
    fn custom_caps_and_a_zero_cap() {
        let cands: Vec<Candidate> = (0..3)
            .map(|i| {
                cand(
                    i,
                    1,
                    Some(401),
                    CadenceGroup::Daily,
                    &format!("2026-10-16T0{i}:00:00Z"),
                )
            })
            .collect();
        let none = compose(&cands, Caps { week: 3, wire: 0 });
        assert!(none.entries.is_empty());
        assert_eq!(
            none.overflow,
            [Overflow {
                feed_id: 1,
                section: Section::Wire,
                hidden_count: 3
            }]
        );
        let two = compose(&cands, Caps { week: 3, wire: 2 });
        assert_eq!(placed(&two), [(2, Section::Wire, 1), (1, Section::Wire, 2)]);
    }

    #[test]
    fn silent_candidates_and_an_empty_input_give_nothing() {
        let c = compose(
            &[cand(
                1,
                1,
                Some(401),
                CadenceGroup::Silent,
                "2026-10-16T05:00:00Z",
            )],
            Caps::default(),
        );
        assert_eq!(c, Composition::default());
        assert_eq!(compose(&[], Caps::default()), Composition::default());
    }
}
