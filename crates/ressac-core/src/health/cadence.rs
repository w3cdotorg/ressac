//! Publication rhythm of a feed, from its stored entries.

use chrono::{DateTime, Duration, Utc};

use crate::model::Cadence;
use crate::store::EntryRecord;

/// Intervals are measured over at most this many most recent entries.
const INTERVAL_WINDOW: usize = 21;

pub fn cadence(entries: &[EntryRecord], now: DateTime<Utc>) -> Cadence {
    let mut dates: Vec<DateTime<Utc>> = entries
        .iter()
        .filter(|e| !e.is_fallback)
        .map(|e| e.published_at)
        .collect();
    if dates.is_empty() {
        dates = entries.iter().map(|e| e.published_at).collect();
    }
    dates.sort_unstable_by(|a, b| b.cmp(a));
    let intervals: Vec<f64> = dates
        .iter()
        .take(INTERVAL_WINDOW)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (*w[0] - *w[1]).num_seconds() as f64 / 86_400.0)
        .collect();
    let year_ago = now - Duration::days(365);
    Cadence {
        last_entry_at: dates.first().copied(),
        median_interval_days: median(intervals),
        entries_per_month: dates.iter().filter(|d| **d >= year_ago).count() as f64 / 12.0,
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn entry(days_ago: i64, fallback: bool) -> EntryRecord {
        EntryRecord {
            guid: format!("g{days_ago}"),
            url: None,
            title: None,
            summary_html: None,
            published_at: now() - Duration::days(days_ago),
            is_fallback: fallback,
            fetched_at: now(),
        }
    }

    #[test]
    fn empty_feed_has_no_cadence() {
        assert_eq!(cadence(&[], now()), Cadence::default());
    }

    #[test]
    fn computes_last_entry_median_interval_and_monthly_rate() {
        let c = cadence(
            &[
                entry(0, false),
                entry(10, false),
                entry(30, false),
                entry(400, false),
            ],
            now(),
        );
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, Some(20.0)); // intervals 10, 20, 370
        assert_eq!(c.entries_per_month, 0.25); // 3 entries in the last 365 days
    }

    #[test]
    fn prefers_real_dates_over_fallback_dates() {
        let c = cadence(&[entry(0, true), entry(800, false)], now());
        assert_eq!(c.last_entry_at, Some(now() - Duration::days(800)));
    }

    #[test]
    fn uses_fallback_dates_when_nothing_else_exists() {
        let c = cadence(&[entry(0, true)], now());
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, None);
    }
}
