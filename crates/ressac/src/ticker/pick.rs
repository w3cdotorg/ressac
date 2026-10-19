//! Which entry the ticker shows: rotation by feed within a 15-second slot. Pure, no I/O.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ressac_core::store::TickerEntry;
use serde::{Deserialize, Serialize};

/// Length of one display slot: every caller within a slot sees the same entry.
pub const SLOT_SECONDS: i64 = 15;

/// The slot a moment falls in.
pub fn slot(now: DateTime<Utc>) -> i64 {
    now.timestamp().div_euclid(SLOT_SECONDS)
}

/// SplitMix64: tiny, seedable, good enough to pick a headline.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n` must be positive).
    pub fn below(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }
}

/// What persists between two `ressac ticker` calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TickerState {
    /// Entry ids already shown in the current cycle.
    pub shown: Vec<i64>,
    pub last_feed: Option<i64>,
    pub current: Option<i64>,
    pub slot: Option<i64>,
}

/// The entry to show in `now_slot` and the state to persist.
///
/// Within a slot the current entry is reprinted while it is still in the pool. Otherwise a new
/// one is drawn: a feed first (never the previous one when another has candidates), then an
/// entry in it, among those not shown yet in this cycle. Feeds and entries are visited in
/// ascending id order, so the outcome depends only on the pool, the state and the seed.
pub fn next(
    pool: &[TickerEntry],
    state: &TickerState,
    now_slot: i64,
    rng: &mut Rng,
) -> (Option<TickerEntry>, TickerState) {
    if pool.is_empty() {
        return (None, TickerState::default());
    }
    let mut state = state.clone();
    state
        .shown
        .retain(|id| pool.iter().any(|e| e.entry_id == *id));

    if state.slot == Some(now_slot)
        && let Some(current) = state
            .current
            .and_then(|id| pool.iter().find(|e| e.entry_id == id))
    {
        return (Some(current.clone()), state);
    }

    let mut candidates: Vec<&TickerEntry> = pool
        .iter()
        .filter(|e| !state.shown.contains(&e.entry_id))
        .collect();
    if candidates.is_empty() {
        state.shown.clear();
        candidates = pool.iter().collect();
    }
    candidates.sort_by_key(|e| e.entry_id);

    let mut by_feed: BTreeMap<i64, Vec<&TickerEntry>> = BTreeMap::new();
    for e in candidates {
        by_feed.entry(e.feed_id).or_default().push(e);
    }
    let mut feeds: Vec<i64> = by_feed
        .keys()
        .copied()
        .filter(|f| Some(*f) != state.last_feed)
        .collect();
    if feeds.is_empty() {
        feeds = by_feed.keys().copied().collect();
    }
    let feed = feeds[rng.below(feeds.len())];
    let in_feed = &by_feed[&feed];
    let chosen = in_feed[rng.below(in_feed.len())];

    state.shown.push(chosen.entry_id);
    state.last_feed = Some(chosen.feed_id);
    state.current = Some(chosen.entry_id);
    state.slot = Some(now_slot);
    (Some(chosen.clone()), state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn entry(id: i64, feed: i64) -> TickerEntry {
        TickerEntry {
            entry_id: id,
            feed_id: feed,
            feed_title: format!("Feed {feed}"),
            title: format!("Entry {id}"),
            url: format!("https://example.com/{id}"),
        }
    }

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn empty_pool_gives_nothing_and_clears_state() {
        let state = TickerState {
            shown: vec![1, 2],
            last_feed: Some(1),
            current: Some(2),
            slot: Some(5),
        };
        let (picked, new) = next(&[], &state, 6, &mut Rng::new(1));
        assert_eq!(picked, None);
        assert_eq!(new, TickerState::default());
    }

    #[test]
    fn no_repeat_within_a_cycle() {
        let pool: Vec<_> = (1..=6).map(|i| entry(i, (i - 1) % 3 + 1)).collect();
        let mut rng = Rng::new(3);
        let mut state = TickerState::default();
        let mut seen = HashSet::new();
        for s in 0..6 {
            let (picked, new) = next(&pool, &state, s, &mut rng);
            assert!(seen.insert(picked.unwrap().entry_id));
            state = new;
        }
        assert_eq!(seen.len(), 6);
        let (picked, new) = next(&pool, &state, 6, &mut rng);
        assert!(picked.is_some());
        assert_eq!(new.shown.len(), 1);
    }

    #[test]
    fn feeds_alternate_when_possible() {
        let mut pool: Vec<_> = (1..=5).map(|i| entry(i, 1)).collect();
        pool.push(entry(6, 2));
        pool.push(entry(7, 3));
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let mut state = TickerState::default();
            let mut feeds = Vec::new();
            for s in 0..3 {
                let (picked, new) = next(&pool, &state, s, &mut rng);
                feeds.push(picked.unwrap().feed_id);
                state = new;
            }
            assert!(
                feeds.windows(2).all(|w| w[0] != w[1]),
                "seed {seed}: {feeds:?}"
            );
        }
    }

    #[test]
    fn a_big_feed_does_not_dominate() {
        let mut pool: Vec<_> = (1..=40).map(|i| entry(i, 1)).collect();
        pool.push(entry(100, 2));
        let mut rng = Rng::new(7);
        let mut state = TickerState::default();
        let mut feeds = Vec::new();
        for s in 0..10 {
            let (picked, new) = next(&pool, &state, s, &mut rng);
            feeds.push(picked.unwrap().feed_id);
            state = new;
        }
        assert!(feeds[..2].contains(&2), "{feeds:?}");
        let b_at = feeds.iter().position(|f| *f == 2).unwrap();
        assert!(feeds[..=b_at].windows(2).all(|w| w[0] != w[1]), "{feeds:?}");
    }

    #[test]
    fn same_slot_reprints_current() {
        let pool: Vec<_> = (1..=5).map(|i| entry(i, i)).collect();
        let mut rng = Rng::new(9);
        let (first, state) = next(&pool, &TickerState::default(), 10, &mut rng);
        let (second, again) = next(&pool, &state, 10, &mut rng);
        assert_eq!(first, second);
        assert_eq!(state, again);
    }

    #[test]
    fn current_left_the_pool_picks_at_once() {
        let pool: Vec<_> = (1..=5).map(|i| entry(i, i)).collect();
        let mut rng = Rng::new(9);
        let (first, state) = next(&pool, &TickerState::default(), 10, &mut rng);
        let first = first.unwrap();
        let smaller: Vec<_> = pool
            .iter()
            .filter(|e| e.entry_id != first.entry_id)
            .cloned()
            .collect();
        let (second, new) = next(&smaller, &state, 10, &mut rng);
        let second = second.unwrap();
        assert_ne!(second.entry_id, first.entry_id);
        assert_eq!(new.current, Some(second.entry_id));
        assert!(!new.shown.contains(&first.entry_id));
    }

    #[test]
    fn shown_is_pruned() {
        let pool: Vec<_> = (1..=3).map(|i| entry(i, i)).collect();
        let state = TickerState {
            shown: vec![1, 98, 99],
            last_feed: None,
            current: None,
            slot: None,
        };
        let (picked, new) = next(&pool, &state, 1, &mut Rng::new(2));
        let id = picked.unwrap().entry_id;
        assert_ne!(id, 1);
        assert!(new.shown.iter().all(|i| [1, 2, 3].contains(i)));
        assert_eq!(new.shown.len(), 2);
    }

    #[test]
    fn slot_boundaries() {
        assert_eq!(
            slot(at("2026-10-19T12:00:14Z")),
            slot(at("2026-10-19T12:00:00Z"))
        );
        assert_eq!(
            slot(at("2026-10-19T12:00:15Z")),
            slot(at("2026-10-19T12:00:00Z")) + 1
        );
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            let x = a.below(100);
            assert_eq!(x, b.below(100));
            assert!(x < 100);
        }
    }
}
