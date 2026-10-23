//! When the lists reload (the Mac's `AppState` and `DataWatcher` rules) and how a reload
//! becomes row operations: the 3 s `data_version` poll, the one-a-second reload during a run,
//! the midnight rollover, and `plan_rows`, which diffs two lists by id so a model never resets.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

/// How often `PRAGMA data_version` is polled while polling is on.
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);
/// The least gap between two reloads caused by a run's progress.
pub const PROGRESS_RELOAD_GAP: Duration = Duration::from_secs(1);

/// The watcher runs while a window is active or the chart is on screen.
pub fn should_poll(window_active: bool, chart_on_screen: bool) -> bool {
    window_active || chart_on_screen
}

/// The reload rules' memory: the last `data_version`, the last run notice, the last day.
#[derive(Debug, Clone, Default)]
pub struct Invalidation {
    version: Option<i64>,
    notice: Option<Instant>,
    today: Option<DateTime<Utc>>,
}

impl Invalidation {
    /// A poll's `data_version`: true when another connection committed since the last one.
    /// The first call is the baseline.
    pub fn version_seen(&mut self, version: i64) -> bool {
        let changed = self.version.is_some_and(|old| old != version);
        self.version = Some(version);
        changed
    }

    /// The window became active: a new baseline, and a reload.
    pub fn activated(&mut self, version: i64) -> bool {
        self.version = Some(version);
        true
    }

    /// A run's progress event: reloads at most once per `PROGRESS_RELOAD_GAP`.
    pub fn run_progress(&mut self, now: Instant) -> bool {
        let due = self
            .notice
            .is_none_or(|last| now.saturating_duration_since(last) >= PROGRESS_RELOAD_GAP);
        if due {
            self.notice = Some(now);
        }
        due
    }

    /// A run ended: always reloads, and counts as a notice.
    pub fn run_finished(&mut self, now: Instant) -> bool {
        self.notice = Some(now);
        true
    }

    /// Local midnight in UTC, checked every minute: true when the day changed since the last
    /// call. The first call is the baseline.
    pub fn day_seen(&mut self, today_start: DateTime<Utc>) -> bool {
        let changed = self.today.is_some_and(|old| old != today_start);
        self.today = Some(today_start);
        changed
    }
}

/// One change to a list model: remove old rows `first..=last`, insert `rows` at final index
/// `at`, or replace the rows from `first`.
#[derive(Debug, Clone, PartialEq)]
pub enum RowOp<T> {
    Remove { first: usize, last: usize },
    Insert { at: usize, rows: Vec<T> },
    Change { first: usize, rows: Vec<T> },
}

/// Indices (into `seq`) of a longest strictly increasing run of the `Some` values.
fn longest_increasing(seq: &[Option<usize>]) -> Vec<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, value) in seq.iter().enumerate() {
        let Some(value) = *value else { continue };
        let at = tails.partition_point(|&t| seq[t].is_some_and(|v| v < value));
        prev[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        run.push(i);
        at = prev[i];
    }
    run.reverse();
    run
}

/// Runs of consecutive numbers in an ascending list, as `(first, last)`.
fn runs(sorted: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &i in sorted {
        match out.last_mut() {
            Some((_, last)) if *last + 1 == i => *last = i,
            _ => out.push((i, i)),
        }
    }
    out
}

/// The operations that turn `old` into `new`, rows matched by `key`. Kept rows are a longest
/// run already in order, so a moved row is one removal and one insertion. Removals come
/// bottom-up in contiguous runs (old indices), then insertions top-down in contiguous runs
/// (final indices), then the changes of kept rows whose value differs (final indices).
pub fn plan_rows<T: Clone + PartialEq, K: Eq + Hash>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> K,
) -> Vec<RowOp<T>> {
    let at_new: HashMap<K, usize> = new.iter().enumerate().map(|(i, r)| (key(r), i)).collect();
    let seq: Vec<Option<usize>> = old.iter().map(|r| at_new.get(&key(r)).copied()).collect();
    let kept = longest_increasing(&seq);
    let kept_old: HashSet<usize> = kept.iter().copied().collect();
    let kept_new: HashSet<usize> = kept.iter().filter_map(|&o| seq[o]).collect();
    let mut ops = Vec::new();

    let gone: Vec<usize> = (0..old.len()).filter(|i| !kept_old.contains(i)).collect();
    for (first, last) in runs(&gone).into_iter().rev() {
        ops.push(RowOp::Remove { first, last });
    }
    let added: Vec<usize> = (0..new.len()).filter(|j| !kept_new.contains(j)).collect();
    for (at, end) in runs(&added) {
        ops.push(RowOp::Insert {
            at,
            rows: new[at..=end].to_vec(),
        });
    }
    let changed: Vec<usize> = kept
        .iter()
        .filter_map(|&o| seq[o].filter(|&j| old[o] != new[j]))
        .collect();
    for (first, last) in runs(&changed) {
        ops.push(RowOp::Change {
            first,
            rows: new[first..=last].to_vec(),
        });
    }
    ops
}

/// The reference semantics of the operations, which the bridges follow with their Qt calls.
pub fn apply_rows<T: Clone>(rows: &mut Vec<T>, ops: &[RowOp<T>]) {
    for op in ops {
        match op {
            RowOp::Remove { first, last } => {
                rows.drain(*first..=*last);
            }
            RowOp::Insert { at, rows: added } => {
                rows.splice(*at..*at, added.iter().cloned());
            }
            RowOp::Change {
                first,
                rows: changed,
            } => {
                for (i, row) in changed.iter().enumerate() {
                    rows[first + i] = row.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watcher_sees_only_other_connections_commits() {
        let db = crate::test_db::TestDb::new();
        let gui = db.store();
        let mut live = Invalidation::default();
        assert!(
            !live.version_seen(gui.data_version().unwrap()),
            "the first poll is the baseline"
        );
        db.entry(1, "2026-10-01T08:00:00.000Z", false);
        assert!(live.version_seen(gui.data_version().unwrap()));
        assert!(!live.version_seen(gui.data_version().unwrap()));
        db.execute("UPDATE entry SET read_at = '2026-10-10T08:00:00.000Z'");
        assert!(live.version_seen(gui.data_version().unwrap()));
    }

    #[test]
    fn polling_runs_while_a_window_is_active_or_the_chart_is_on_screen() {
        assert_eq!(
            [
                should_poll(true, false),
                should_poll(false, true),
                should_poll(true, true),
                should_poll(false, false)
            ],
            [true, true, true, false]
        );
        assert_eq!(
            (POLL_INTERVAL, PROGRESS_RELOAD_GAP),
            (Duration::from_secs(3), Duration::from_secs(1))
        );
    }

    #[test]
    fn becoming_active_reloads_once_and_takes_a_fresh_baseline() {
        let mut live = Invalidation::default();
        live.version_seen(5);
        assert!(live.activated(9));
        assert!(!live.version_seen(9));
        assert!(live.version_seen(10));
    }

    #[test]
    fn run_progress_reloads_at_most_once_a_second_and_every_run_end_reloads() {
        let mut live = Invalidation::default();
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        assert!(live.run_progress(t0));
        assert!(!live.run_progress(ms(999)));
        assert!(live.run_progress(ms(1000)));
        assert!(live.run_finished(ms(1100)));
        assert!(!live.run_progress(ms(1500)), "the end counts as a notice");
        assert!(live.run_progress(ms(2100)));
    }

    #[test]
    fn the_day_changes_at_local_midnight() {
        let mut live = Invalidation::default();
        let paris_midnight = |d: &str| crate::test_db::at(&format!("{d}T22:00:00Z"));
        assert!(!live.day_seen(paris_midnight("2026-10-15")), "baseline");
        assert!(!live.day_seen(paris_midnight("2026-10-15")));
        assert!(live.day_seen(paris_midnight("2026-10-16")));
    }

    fn rows(ids: &[u32]) -> Vec<(u32, u32)> {
        ids.iter().map(|&i| (i, 0)).collect()
    }
    fn rebuilds(old: &[(u32, u32)], new: &[(u32, u32)]) -> Vec<RowOp<(u32, u32)>> {
        let ops = plan_rows(old, new, |r| r.0);
        let mut got = old.to_vec();
        apply_rows(&mut got, &ops);
        assert_eq!(got, new, "{ops:?}");
        ops
    }

    #[test]
    fn a_reload_that_changes_nothing_plans_nothing() {
        let a = rows(&[1, 2, 3]);
        assert!(rebuilds(&a, &a).is_empty());
    }

    #[test]
    fn new_rows_on_top_a_gone_row_and_a_changed_row_are_three_operations() {
        let old = rows(&(1..=10).collect::<Vec<_>>());
        let mut new = rows(&[21, 20, 1, 3, 4, 5, 6, 7, 8, 9, 10]);
        new[7].1 = 1; // entry 7 changed (read, starred, title…)
        assert_eq!(
            rebuilds(&old, &new),
            [
                RowOp::Remove { first: 1, last: 1 },
                RowOp::Insert {
                    at: 0,
                    rows: rows(&[21, 20])
                },
                RowOp::Change {
                    first: 7,
                    rows: vec![(7, 1)]
                },
            ]
        );
    }

    #[test]
    fn a_moved_row_is_one_removal_and_one_insertion() {
        let old = rows(&[1, 2, 3, 4]);
        assert_eq!(
            rebuilds(&old, &rows(&[3, 1, 2, 4])),
            [
                RowOp::Remove { first: 2, last: 2 },
                RowOp::Insert {
                    at: 0,
                    rows: rows(&[3])
                }
            ]
        );
        rebuilds(&old, &rows(&[4, 3, 2, 1]));
        rebuilds(&old, &[]);
        rebuilds(&[], &old);
    }

    #[test]
    fn any_reload_plans_rows_that_rebuild_it_exactly() {
        let mut seed: u64 = 20_261_023;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as u32
        };
        for _ in 0..2000 {
            // plain loops: two closures cannot both borrow `next` mutably
            let mut old: Vec<(u32, u32)> = Vec::new();
            for i in 0..next() % 30 {
                if next() % 3 != 0 {
                    old.push((i, next() % 2));
                }
            }
            let mut new: Vec<(u32, u32)> = Vec::new();
            for r in &old {
                if next() % 4 != 0 {
                    new.push((r.0, next() % 2));
                }
            }
            for i in 0..next() % 5 {
                let at = next() as usize % (new.len() + 1);
                new.insert(at, (100 + i, 0));
            }
            if new.len() > 2 && next() % 2 == 0 {
                let r = new.remove(0);
                new.push(r);
            }
            rebuilds(&old, &new);
        }
    }
}
