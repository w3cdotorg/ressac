//! A feed's rhythm line (spec §6 "Rhythm"), worded as the app's `GardenModel.rhythmText`. The
//! override menu itself is the state's `Overlay::Rhythm`. Pure: the time comes from the caller.

use chrono::{DateTime, Utc};
use ressac_core::model::{CadenceGroup, FeedId};
use ressac_core::store::CadenceDetail;

use crate::commands::feed::group_title;

/// What the rhythm line needs, from the feed's `CadenceDetail` (`Store::cadence_details`).
#[derive(Debug, Clone, PartialEq)]
pub struct RhythmInfo {
    pub feed_id: FeedId,
    pub effective: CadenceGroup,
    pub computed: CadenceGroup,
    pub is_override: bool,
    pub median_days: Option<f64>,
    pub last_entry_at: Option<DateTime<Utc>>,
}

impl From<&CadenceDetail> for RhythmInfo {
    fn from(detail: &CadenceDetail) -> Self {
        RhythmInfo {
            feed_id: detail.feed_id,
            effective: detail.effective_group,
            computed: detail.computed_group,
            is_override: detail.is_override,
            median_days: detail.median_interval_days,
            last_entry_at: detail.last_entry_at,
        }
    }
}

/// "Daily · median 0.4 d · last entry 2 h ago", plus " (set by you)".
pub fn rhythm_text(info: &RhythmInfo, now: DateTime<Utc>) -> String {
    let mut parts = vec![group_title(info.effective).to_string()];
    if let Some(median) = info.median_days {
        parts.push(if median < 10.0 {
            format!("median {median:.1} d")
        } else {
            format!("median {} d", median.round() as i64)
        });
    }
    if let Some(last) = info.last_entry_at {
        parts.push(format!("last entry {}", ago(last, now)));
    }
    let mut text = parts.join(" · ");
    if info.is_override {
        text.push_str(" (set by you)");
    }
    text
}

/// "Rhythm: " + the rhythm text: the line shown on a garden page and on a feed.
pub fn rhythm_line(info: &RhythmInfo, now: DateTime<Utc>) -> String {
    format!("Rhythm: {}", rhythm_text(info, now))
}

/// "45 min ago", "2 h ago" (under two days), "3 d ago" (under 60 days), then "13 mo ago".
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - then).num_minutes().max(0);
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours} h ago");
    }
    let days = hours / 24;
    if days < 60 {
        return format!("{days} d ago");
    }
    format!("{} mo ago", days / 30)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        "2026-10-16T18:00:00Z".parse().unwrap()
    }

    fn info(
        effective: CadenceGroup,
        is_override: bool,
        median: Option<f64>,
        last_h: Option<i64>,
    ) -> RhythmInfo {
        RhythmInfo {
            feed_id: 12,
            effective,
            computed: CadenceGroup::Weekly,
            is_override,
            median_days: median,
            last_entry_at: last_h.map(|h| now() - Duration::hours(h)),
        }
    }

    #[test]
    fn the_rhythm_text_matches_the_app() {
        assert_eq!(
            rhythm_text(
                &info(CadenceGroup::Daily, false, Some(0.42), Some(2)),
                now()
            ),
            "Daily · median 0.4 d · last entry 2 h ago"
        );
        assert_eq!(
            rhythm_text(&info(CadenceGroup::Daily, true, Some(0.42), Some(2)), now()),
            "Daily · median 0.4 d · last entry 2 h ago (set by you)"
        );
        assert_eq!(
            rhythm_text(&info(CadenceGroup::Monthly, false, Some(12.6), None), now()),
            "Monthly · median 13 d"
        );
        assert_eq!(
            rhythm_text(&info(CadenceGroup::Silent, false, None, None), now()),
            "Silent"
        );
        assert_eq!(
            rhythm_line(&info(CadenceGroup::Rare, false, None, None), now()),
            "Rhythm: Rare"
        );
    }

    #[test]
    fn ago_matches_the_app() {
        let t = now();
        assert_eq!(ago(t - Duration::minutes(45), t), "45 min ago");
        assert_eq!(ago(t - Duration::hours(47), t), "47 h ago");
        assert_eq!(ago(t - Duration::days(3), t), "3 d ago");
        assert_eq!(ago(t - Duration::days(400), t), "13 mo ago");
        assert_eq!(ago(t + Duration::minutes(5), t), "0 min ago");
    }

    #[test]
    fn a_cadence_detail_becomes_the_line() {
        let detail = CadenceDetail {
            feed_id: 4,
            effective_group: CadenceGroup::Rare,
            computed_group: CadenceGroup::Weekly,
            is_override: true,
            median_interval_days: Some(11.0),
            entries_per_month: 3.0,
            last_entry_at: Some(now() - Duration::days(70)),
            unread: 2,
        };
        let info = RhythmInfo::from(&detail);
        assert_eq!(info.feed_id, 4);
        assert_eq!(
            rhythm_line(&info, now()),
            "Rhythm: Rare · median 11 d · last entry 2 mo ago (set by you)"
        );
    }
}
