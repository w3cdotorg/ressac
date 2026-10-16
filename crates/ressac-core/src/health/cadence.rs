//! Publication rhythm of a feed, from its stored entries.

use chrono::{DateTime, Duration, Utc};

use crate::model::{Cadence, CadenceGroup};
use crate::store::EntryRecord;

/// Intervals are measured over at most this many most recent entries.
const INTERVAL_WINDOW: usize = 21;

/// A feed whose latest entry is older than this is Silent (spec §4).
pub const SILENT_AFTER_DAYS: i64 = 365;
/// Median gap (days) under which a feed is Daily; then Weekly, then Monthly; Rare above.
pub const DAILY_BELOW_DAYS: f64 = 1.5;
pub const WEEKLY_BELOW_DAYS: f64 = 10.0;
pub const MONTHLY_BELOW_DAYS: f64 = 45.0;

/// Mean length of a month in days, for `entries_per_month`.
const DAYS_PER_MONTH: f64 = 30.4375;

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
    let last_entry_at = dates.first().copied();
    let median_interval_days = median(intervals);
    Cadence {
        last_entry_at,
        median_interval_days,
        entries_per_month: entries_per_month(&dates, now),
        group: classify(last_entry_at, median_interval_days, now),
    }
}

/// The rhythm group, first match wins: Silent (no entry, or the latest older than
/// `SILENT_AFTER_DAYS`), then Daily, Weekly, Monthly by median gap, else Rare (a gap of
/// 45 days or more, or no gap at all: a single entry this year).
pub fn classify(
    last_entry_at: Option<DateTime<Utc>>,
    median_interval_days: Option<f64>,
    now: DateTime<Utc>,
) -> CadenceGroup {
    let Some(last) = last_entry_at else {
        return CadenceGroup::Silent;
    };
    if now - last > Duration::days(SILENT_AFTER_DAYS) {
        return CadenceGroup::Silent;
    }
    match median_interval_days {
        Some(m) if m < DAILY_BELOW_DAYS => CadenceGroup::Daily,
        Some(m) if m < WEEKLY_BELOW_DAYS => CadenceGroup::Weekly,
        Some(m) if m < MONTHLY_BELOW_DAYS => CadenceGroup::Monthly,
        _ => CadenceGroup::Rare,
    }
}

/// Entries of the last 365 days over the months they cover, from the oldest of them to
/// `now` and at least one month. Dividing by twelve made a feed imported last week, which
/// only exposes its 20 latest items, look like it published under two a month.
fn entries_per_month(dates_newest_first: &[DateTime<Utc>], now: DateTime<Utc>) -> f64 {
    let year_ago = now - Duration::days(365);
    let recent: Vec<&DateTime<Utc>> = dates_newest_first
        .iter()
        .filter(|d| **d >= year_ago)
        .collect();
    let Some(oldest) = recent.last() else {
        return 0.0;
    };
    let months = ((now - **oldest).num_seconds() as f64 / 86_400.0 / DAYS_PER_MONTH).max(1.0);
    recent.len() as f64 / months
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
    fn computes_last_entry_median_interval_monthly_rate_and_group() {
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
        // 3 entries in the last 365 days, the oldest 30 days ago: under a month, so one month.
        assert_eq!(c.entries_per_month, 3.0);
        assert_eq!(c.group, CadenceGroup::Monthly);
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
        assert_eq!(
            c.group,
            CadenceGroup::Rare,
            "one entry this year, no gap to measure"
        );
    }

    fn at(days_ago: i64) -> Option<DateTime<Utc>> {
        Some(now() - Duration::days(days_ago))
    }

    #[test]
    fn median_thresholds_pick_the_group_first_match_wins() {
        let recent = Some(now() - Duration::hours(1));
        for (median, group) in [
            (0.0, CadenceGroup::Daily),
            (1.4999, CadenceGroup::Daily),
            (1.5, CadenceGroup::Weekly),
            (9.999, CadenceGroup::Weekly),
            (10.0, CadenceGroup::Monthly),
            (44.999, CadenceGroup::Monthly),
            (45.0, CadenceGroup::Rare),
            (400.0, CadenceGroup::Rare),
        ] {
            assert_eq!(
                classify(recent, Some(median), now()),
                group,
                "median {median}"
            );
        }
        assert_eq!(classify(recent, None, now()), CadenceGroup::Rare);
    }

    #[test]
    fn silent_means_no_entry_or_none_for_a_year() {
        assert_eq!(classify(None, None, now()), CadenceGroup::Silent);
        assert_eq!(classify(None, Some(0.5), now()), CadenceGroup::Silent);
        // Exactly 365 days is still this year; one second more is silence, whatever the median.
        assert_eq!(classify(at(365), None, now()), CadenceGroup::Rare);
        let just_over = Some(now() - Duration::days(365) - Duration::seconds(1));
        assert_eq!(classify(just_over, Some(0.5), now()), CadenceGroup::Silent);
    }

    #[test]
    fn batch_posting_is_daily() {
        // Five photos uploaded at once, then one the day before: median gap 0.
        let mut entries: Vec<EntryRecord> = (0..5)
            .map(|i| EntryRecord {
                guid: format!("p{i}"),
                ..entry(0, false)
            })
            .collect();
        entries.push(entry(1, false));
        let c = cadence(&entries, now());
        assert_eq!(c.median_interval_days, Some(0.0));
        assert_eq!(c.group, CadenceGroup::Daily);
    }

    #[test]
    fn gaps_are_counted_in_utc_seconds_across_a_dst_change() {
        // Europe/Paris leaves summer time on 2026-10-25; the gap stays exactly one day.
        let e = |ts: &str| EntryRecord {
            published_at: ts.parse().unwrap(),
            ..entry(0, false)
        };
        let c = cadence(
            &[e("2026-10-24T23:30:00Z"), e("2026-10-25T23:30:00Z")],
            "2026-10-26T08:00:00Z".parse().unwrap(),
        );
        assert_eq!(c.median_interval_days, Some(1.0));
        assert_eq!(c.group, CadenceGroup::Daily);
    }

    #[test]
    fn a_feed_imported_recently_is_not_divided_by_twelve() {
        // 20 entries over the last 10 days: the span is under a month, so one month.
        let fresh: Vec<EntryRecord> = (0..20)
            .map(|i| EntryRecord {
                guid: format!("f{i}"),
                published_at: now() - Duration::hours(12 * i),
                ..entry(0, false)
            })
            .collect();
        assert_eq!(cadence(&fresh, now()).entries_per_month, 20.0);
        // 12 entries every 10 days, the oldest 110 days ago: 12 over 110 / 30.4375 months.
        let spread: Vec<EntryRecord> = (0..12).map(|i| entry(10 * i, false)).collect();
        let rate = cadence(&spread, now()).entries_per_month;
        assert!((rate - 12.0 / (110.0 / 30.4375)).abs() < 1e-9, "{rate}");
        // Nothing this year: no rate.
        assert_eq!(cadence(&[entry(400, false)], now()).entries_per_month, 0.0);
    }

    #[test]
    fn an_old_feed_is_silent_even_with_a_tight_median() {
        let c = cadence(
            &[entry(400, false), entry(401, false), entry(402, false)],
            now(),
        );
        assert_eq!(c.median_interval_days, Some(1.0));
        assert_eq!(c.group, CadenceGroup::Silent);
    }
}
