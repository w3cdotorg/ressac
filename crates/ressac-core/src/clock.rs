//! Calendar bounds that depend on the local zone. Callers pass `now` in the zone they mean
//! (production: `chrono::Local`; tests: a fixed or test zone), never read the clock here. The
//! Journal's own day, from 04:00, is `edition::day_start`.

use chrono::{DateTime, TimeZone, Utc};

/// Local midnight of the day `now` belongs to, in UTC (the app's "Today").
pub fn today_start<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Utc> {
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).expect("00:00 exists");
    let zone = now.timezone();
    zone.from_local_datetime(&midnight)
        .earliest()
        // A zone that skips midnight (DST at 00:00): the day starts at the first valid hour.
        .or_else(|| {
            zone.from_local_datetime(&(midnight + chrono::Duration::hours(1)))
                .earliest()
        })
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::offset::MappedLocalTime;
    use chrono::{FixedOffset, NaiveDate, NaiveDateTime};

    #[test]
    fn today_starts_at_local_midnight() {
        let paris_summer = FixedOffset::east_opt(2 * 3600).unwrap();
        let midnight = Utc.with_ymd_and_hms(2026, 10, 15, 22, 0, 0).unwrap();
        for (h, m) in [(0, 30), (23, 59)] {
            let now = paris_summer
                .with_ymd_and_hms(2026, 10, 16, h, m, 0)
                .unwrap();
            assert_eq!(today_start(now), midnight, "{h}:{m}");
        }
    }

    /// UTC-4 until 2026-09-06 04:00 UTC, then UTC-3: local 00:00 to 00:59 on 6 September does not
    /// exist (a spring change at midnight, as in Santiago).
    #[derive(Debug, Clone, Copy)]
    struct SkipsMidnight;

    impl SkipsMidnight {
        fn offset_at_utc(utc: &NaiveDateTime) -> FixedOffset {
            let change = NaiveDate::from_ymd_opt(2026, 9, 6)
                .unwrap()
                .and_hms_opt(4, 0, 0)
                .unwrap();
            FixedOffset::west_opt(if *utc < change { 4 * 3600 } else { 3 * 3600 }).unwrap()
        }
    }

    impl TimeZone for SkipsMidnight {
        type Offset = FixedOffset;

        fn from_offset(_: &FixedOffset) -> Self {
            SkipsMidnight
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
            self.offset_from_local_datetime(&local.and_hms_opt(12, 0, 0).unwrap())
        }

        fn offset_from_local_datetime(
            &self,
            local: &NaiveDateTime,
        ) -> MappedLocalTime<FixedOffset> {
            let fits: Vec<FixedOffset> = [4, 3]
                .into_iter()
                .map(|h| FixedOffset::west_opt(h * 3600).unwrap())
                .filter(|o| Self::offset_at_utc(&(*local - *o)) == *o)
                .collect();
            match fits.as_slice() {
                [] => MappedLocalTime::None,
                [o] => MappedLocalTime::Single(*o),
                [a, b, ..] => MappedLocalTime::Ambiguous(*a, *b),
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
            Self::offset_at_utc(&utc.and_hms_opt(12, 0, 0).unwrap())
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
            Self::offset_at_utc(utc)
        }
    }

    #[test]
    fn a_day_whose_midnight_is_skipped_starts_at_the_first_valid_hour() {
        let noon = SkipsMidnight
            .with_ymd_and_hms(2026, 9, 6, 12, 0, 0)
            .unwrap();
        // 01:00 local, UTC-3.
        assert_eq!(
            today_start(noon),
            Utc.with_ymd_and_hms(2026, 9, 6, 4, 0, 0).unwrap()
        );
        let day_before = SkipsMidnight
            .with_ymd_and_hms(2026, 9, 5, 18, 0, 0)
            .unwrap();
        // 00:00 local, UTC-4.
        assert_eq!(
            today_start(day_before),
            Utc.with_ymd_and_hms(2026, 9, 5, 4, 0, 0).unwrap()
        );
    }
}
