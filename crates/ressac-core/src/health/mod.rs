//! Feed diagnosis: pure functions from evidence to a diagnosis. No I/O.

mod cadence;
pub mod hijack;

pub use cadence::cadence;

use chrono::{DateTime, Duration, Utc};

use crate::model::{Action, Cadence, Diagnosis, ErrorKind, Evidence, FeedId, Remark, Status};
use crate::store::{AttemptRecord, EntryRecord};
use crate::url_key::{https_variant, url_key};

pub const ALIVE_WITHIN_DAYS: i64 = 90;
pub const FOSSIL_AFTER_DAYS: i64 = 730;
pub const DEAD_FAILURE_STREAK: usize = 5;
pub const DEAD_STREAK_MIN_DAYS: i64 = 14;
pub const DUPLICATE_OVERLAP: f64 = 0.8;
pub const HIJACK_THRESHOLD: f32 = 0.6;
pub const SIMILAR_TITLE_DISTANCE: usize = 2;

/// Everything `diagnose` may look at for one feed. Cross-feed facts (`duplicate_of`)
/// are computed by the caller.
#[derive(Debug, Clone)]
pub struct FeedEvidence {
    pub feed_id: FeedId,
    pub title: String,
    pub xml_url: String,
    pub url_key: String,
    /// Cleaned description: the feed's current one, else the OPML one.
    pub description: Option<String>,
    /// Newest first.
    pub attempts: Vec<AttemptRecord>,
    /// Newest first.
    pub entries: Vec<EntryRecord>,
    pub duplicate_of: Option<FeedId>,
    pub discovered_url: Option<String>,
    pub https_available: Option<bool>,
    pub now: DateTime<Utc>,
}

struct Verdict {
    status: Status,
    reason: String,
    bullets: Vec<String>,
    clue: String,
    action: Option<Action>,
}

impl Verdict {
    fn new(
        status: Status,
        reason: impl Into<String>,
        clue: impl Into<String>,
        action: Option<Action>,
    ) -> Self {
        let reason = reason.into();
        Verdict {
            status,
            bullets: vec![reason.clone()],
            reason,
            clue: clue.into(),
            action,
        }
    }
}

pub fn diagnose(ev: &FeedEvidence) -> Diagnosis {
    let cadence = cadence(&ev.entries, ev.now);
    let hijack = hijack::assess(ev);
    let remarks = remarks(ev);
    let upgrade = remarks
        .contains(&Remark::HttpsAvailable)
        .then(|| Action::UpgradeHttps {
            url: https_target(ev),
        });
    let v = verdict(ev, &cadence, &hijack, upgrade);
    let language = (v.status == Status::Hijacked)
        .then(|| hijack.language.clone())
        .flatten();
    Diagnosis {
        status: v.status,
        reason: v.reason,
        evidence: Evidence {
            bullets: v.bullets,
            clue: v.clue,
            language,
        },
        remarks,
        action: v.action,
        hijack_score: hijack.score,
        cadence,
    }
}

fn last_meaningful(ev: &FeedEvidence) -> Option<&AttemptRecord> {
    ev.attempts.iter().find(|a| !a.network_suspect)
}

fn verdict(
    ev: &FeedEvidence,
    cadence: &Cadence,
    hijack: &hijack::HijackAssessment,
    upgrade: Option<Action>,
) -> Verdict {
    if let Some(other) = ev.duplicate_of {
        let mut v = Verdict::new(
            Status::Duplicate,
            format!("Same feed as #{other}."),
            format!("= #{other}"),
            Some(Action::MergeInto { feed_id: other }),
        );
        v.bullets = vec![format!(
            "Its final address or most of its entries match feed #{other}."
        )];
        return v;
    }
    if hijack.score >= HIJACK_THRESHOLD {
        let clue = hijack
            .language
            .as_ref()
            .map_or_else(|| "spam".to_string(), |l| format!("{l} ⚠"));
        let mut v = Verdict::new(
            Status::Hijacked,
            "The site now seems to serve unrelated or spam content.",
            clue,
            Some(Action::Bury),
        );
        v.bullets = hijack.bullets.clone();
        return v;
    }
    let Some(last) = last_meaningful(ev) else {
        return Verdict::new(Status::Unchecked, "Not fetched yet.", "new", None);
    };
    if last.error.is_none()
        && let Some((status, url)) = permanent_redirect(last, &ev.url_key)
    {
        let mut v = Verdict::new(
            Status::Moved,
            "The feed permanently redirects to a new address.",
            status.to_string(),
            Some(Action::FollowRedirect { url: url.clone() }),
        );
        v.bullets.push(format!("{} → {url}", ev.xml_url));
        return v;
    }
    if let Some((reason, clue)) = dead_reason(ev) {
        if let Some(url) = ev
            .discovered_url
            .as_ref()
            .filter(|u| url_key(u) != ev.url_key)
        {
            let mut v = Verdict::new(
                Status::Moved,
                "The feed is gone, but its site now announces another one.",
                "→ new",
                Some(Action::AdoptDiscovered { url: url.clone() }),
            );
            v.bullets = vec![reason, format!("The site links to {url}.")];
            return v;
        }
        return Verdict::new(Status::Dead, reason, clue, Some(Action::Bury));
    }
    if let Some(err) = &last.error {
        return Verdict::new(
            Status::Ailing,
            format!("Last fetch failed: {}.", err.label()),
            err.clue(),
            upgrade,
        );
    }
    let Some(last_entry) = cadence.last_entry_at else {
        return Verdict::new(
            Status::Fossil,
            "The feed responds but has no entries.",
            "empty",
            upgrade,
        );
    };
    let days = (ev.now - last_entry).num_days();
    let reason = format!("Last entry on {}.", last_entry.format("%Y-%m-%d"));
    let (status, clue) = if days < ALIVE_WITHIN_DAYS {
        (Status::Alive, format!("{days} d"))
    } else if days < FOSSIL_AFTER_DAYS {
        (Status::Dormant, format!("{} mo", days / 30))
    } else {
        (Status::Fossil, last_entry.format("%Y").to_string())
    };
    let mut v = Verdict::new(status, reason, clue, upgrade);
    v.bullets.push(format!(
        "About {:.1} entries per month over the last year.",
        cadence.entries_per_month
    ));
    if let Some(median) = cadence.median_interval_days {
        v.bullets
            .push(format!("Median gap between entries: {median:.0} days."));
    }
    v
}

/// The first permanent hop status and the final URL, when the chain ends on another feed.
fn permanent_redirect(last: &AttemptRecord, own_key: &str) -> Option<(u16, String)> {
    let permanent = last
        .redirect_chain
        .iter()
        .find(|h| h.status == 301 || h.status == 308)?;
    let target = &last.redirect_chain.last()?.url;
    (url_key(target) != own_key).then(|| (permanent.status, target.clone()))
}

/// Dead rule (spec §7): 404/410/NXDOMAIN on the last meaningful attempt, or a long
/// enough streak of failures. Returns (reason, clue).
pub fn dead_reason(ev: &FeedEvidence) -> Option<(String, String)> {
    let meaningful: Vec<&AttemptRecord> =
        ev.attempts.iter().filter(|a| !a.network_suspect).collect();
    let last = meaningful.first()?;
    match &last.error {
        Some(ErrorKind::Http(code @ (404 | 410))) => {
            return Some((
                format!("The server says the feed is gone (HTTP {code})."),
                code.to_string(),
            ));
        }
        Some(ErrorKind::Dns) => {
            return Some((
                "The domain no longer resolves.".to_string(),
                "DNS".to_string(),
            ));
        }
        _ => {}
    }
    let streak: Vec<&&AttemptRecord> = meaningful
        .iter()
        .take_while(|a| a.error.is_some())
        .collect();
    if streak.len() >= DEAD_FAILURE_STREAK {
        let span = streak[0].at - streak[streak.len() - 1].at;
        if span >= Duration::days(DEAD_STREAK_MIN_DAYS) {
            return Some((
                format!(
                    "{} failed fetches in a row over {} days.",
                    streak.len(),
                    span.num_days()
                ),
                format!("{}× fail", streak.len()),
            ));
        }
    }
    None
}

fn remarks(ev: &FeedEvidence) -> Vec<Remark> {
    let mut out = Vec::new();
    let last = last_meaningful(ev);
    if ev.xml_url.starts_with("http://") {
        let redirected_to_https = last
            .filter(|a| a.error.is_none())
            .and_then(|a| a.redirect_chain.last())
            .is_some_and(|h| h.url.starts_with("https://") && url_key(&h.url) == ev.url_key);
        if redirected_to_https || ev.https_available == Some(true) {
            out.push(Remark::HttpsAvailable);
        } else {
            out.push(Remark::HttpOnly);
        }
    }
    if last.is_some_and(|a| a.error == Some(ErrorKind::Http(403))) {
        out.push(Remark::Blocked);
    }
    let fallback = ev.entries.iter().filter(|e| e.is_fallback).count();
    if !ev.entries.is_empty() && fallback * 2 > ev.entries.len() {
        out.push(Remark::DateFallback);
    }
    out
}

fn https_target(ev: &FeedEvidence) -> String {
    last_meaningful(ev)
        .and_then(|a| a.redirect_chain.last())
        .map(|h| h.url.clone())
        .filter(|u| u.starts_with("https://") && url_key(u) == ev.url_key)
        .or_else(|| https_variant(&ev.xml_url))
        .unwrap_or_else(|| ev.xml_url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Hop;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn ago(days: i64) -> DateTime<Utc> {
        now() - Duration::days(days)
    }

    fn ok(days: i64) -> AttemptRecord {
        AttemptRecord {
            at: ago(days),
            http_status: Some(200),
            error: None,
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            network_suspect: false,
            not_modified: false,
            entry_count: Some(1),
        }
    }

    fn failed(days: i64, kind: ErrorKind) -> AttemptRecord {
        let http_status = match kind {
            ErrorKind::Http(code) => Some(code),
            _ => None,
        };
        AttemptRecord {
            http_status,
            error: Some(kind),
            entry_count: None,
            ..ok(days)
        }
    }

    fn entry(days: i64) -> EntryRecord {
        EntryRecord {
            guid: format!("g{days}"),
            url: None,
            title: Some(format!("Entry {days}")),
            summary_html: None,
            published_at: ago(days),
            is_fallback: false,
            fetched_at: now(),
        }
    }

    fn ev(attempts: Vec<AttemptRecord>, entries: Vec<EntryRecord>) -> FeedEvidence {
        FeedEvidence {
            feed_id: 7,
            title: "Blog".into(),
            xml_url: "https://blog.example.org/feed".into(),
            url_key: url_key("https://blog.example.org/feed"),
            description: None,
            attempts,
            entries,
            duplicate_of: None,
            discovered_url: None,
            https_available: None,
            now: now(),
        }
    }

    #[test]
    fn never_fetched_is_unchecked() {
        assert_eq!(diagnose(&ev(vec![], vec![])).status, Status::Unchecked);
    }

    #[test]
    fn age_of_last_entry_decides_alive_dormant_fossil() {
        // 1000 days before 2026-10-10 is 2024-01-14, so the fossil clue is "2024".
        let cases = [
            (10, Status::Alive, "10 d"),
            (200, Status::Dormant, "6 mo"),
            (1000, Status::Fossil, "2024"),
        ];
        for (days, status, clue) in cases {
            let d = diagnose(&ev(vec![ok(0)], vec![entry(days)]));
            assert_eq!(d.status, status, "{days} days");
            assert_eq!(d.evidence.clue, clue);
            assert_eq!(d.action, None);
        }
    }

    #[test]
    fn responding_feed_without_entries_is_a_fossil() {
        let d = diagnose(&ev(vec![ok(0)], vec![]));
        assert_eq!(
            (d.status, d.evidence.clue.as_str()),
            (Status::Fossil, "empty")
        );
    }

    #[test]
    fn gone_or_unresolvable_is_dead_at_once() {
        for (kind, clue) in [
            (ErrorKind::Http(410), "410"),
            (ErrorKind::Http(404), "404"),
            (ErrorKind::Dns, "DNS"),
        ] {
            let d = diagnose(&ev(vec![failed(0, kind), ok(30)], vec![entry(40)]));
            assert_eq!(d.status, Status::Dead);
            assert_eq!(d.evidence.clue, clue);
            assert_eq!(d.action, Some(Action::Bury));
        }
    }

    #[test]
    fn failure_streak_must_span_fourteen_days() {
        let long: Vec<_> = [0, 5, 10, 15, 20]
            .iter()
            .map(|d| failed(*d, ErrorKind::Timeout))
            .collect();
        assert_eq!(diagnose(&ev(long, vec![])).status, Status::Dead);
        let short: Vec<_> = [0, 1, 2, 3, 3]
            .iter()
            .map(|d| failed(*d, ErrorKind::Timeout))
            .collect();
        let d = diagnose(&ev(short, vec![]));
        assert_eq!(
            (d.status, d.evidence.clue.as_str()),
            (Status::Ailing, "timeout")
        );
    }

    #[test]
    fn network_suspect_attempts_are_ignored() {
        let mut suspect = failed(0, ErrorKind::Dns);
        suspect.network_suspect = true;
        assert_eq!(
            diagnose(&ev(vec![suspect, ok(1)], vec![entry(3)])).status,
            Status::Alive
        );
    }

    #[test]
    fn permanent_redirect_to_another_feed_is_moved() {
        let mut a = ok(0);
        a.redirect_chain = vec![Hop {
            status: 301,
            url: "https://newblog.example.org/rss".into(),
        }];
        let d = diagnose(&ev(vec![a], vec![entry(1)]));
        assert_eq!(d.status, Status::Moved);
        assert_eq!(
            d.action,
            Some(Action::FollowRedirect {
                url: "https://newblog.example.org/rss".into()
            })
        );
        assert_eq!(d.evidence.clue, "301");
    }

    #[test]
    fn temporary_redirect_is_not_a_move() {
        let mut a = ok(0);
        a.redirect_chain = vec![Hop {
            status: 302,
            url: "https://newblog.example.org/rss".into(),
        }];
        assert_eq!(diagnose(&ev(vec![a], vec![entry(1)])).status, Status::Alive);
    }

    #[test]
    fn http_to_https_redirect_is_a_safe_upgrade_not_a_move() {
        let mut e = ev(vec![], vec![entry(1)]);
        e.xml_url = "http://blog.example.org/feed".into();
        let mut a = ok(0);
        a.redirect_chain = vec![Hop {
            status: 301,
            url: "https://blog.example.org/feed".into(),
        }];
        e.attempts = vec![a];
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Alive);
        assert!(d.remarks.contains(&Remark::HttpsAvailable));
        assert!(!d.remarks.contains(&Remark::HttpOnly));
        assert_eq!(
            d.action,
            Some(Action::UpgradeHttps {
                url: "https://blog.example.org/feed".into()
            })
        );
    }

    #[test]
    fn probed_https_gives_upgrade_and_unprobed_http_is_remarked() {
        let mut e = ev(vec![ok(0)], vec![entry(1)]);
        e.xml_url = "http://blog.example.org/feed".into();
        assert!(diagnose(&e).remarks.contains(&Remark::HttpOnly));
        e.https_available = Some(true);
        assert_eq!(
            diagnose(&e).action,
            Some(Action::UpgradeHttps {
                url: "https://blog.example.org/feed".into()
            })
        );
    }

    #[test]
    fn dead_feed_with_discovered_replacement_is_moved() {
        let mut e = ev(vec![failed(0, ErrorKind::Http(404))], vec![]);
        e.discovered_url = Some("https://blog.example.org/new-feed.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Moved);
        assert_eq!(
            d.action,
            Some(Action::AdoptDiscovered {
                url: "https://blog.example.org/new-feed.xml".into()
            })
        );
    }

    #[test]
    fn duplicate_wins_over_everything() {
        let mut e = ev(vec![failed(0, ErrorKind::Http(410))], vec![]);
        e.duplicate_of = Some(41);
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Duplicate);
        assert_eq!(d.evidence.clue, "= #41");
        assert_eq!(d.action, Some(Action::MergeInto { feed_id: 41 }));
    }

    #[test]
    fn hijacked_beats_dead() {
        let mut e = ev(vec![failed(0, ErrorKind::HtmlInsteadOfFeed)], vec![]);
        e.description =
            Some("Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Hijacked);
        assert!(d.hijack_score >= HIJACK_THRESHOLD);
        assert_eq!(d.action, Some(Action::Bury));
        assert_eq!(d.evidence.clue, "spam");
    }

    #[test]
    fn server_errors_are_ailing_and_403_is_blocked() {
        assert_eq!(
            diagnose(&ev(vec![failed(0, ErrorKind::Http(503))], vec![])).status,
            Status::Ailing
        );
        let d = diagnose(&ev(vec![failed(0, ErrorKind::Http(403))], vec![]));
        assert_eq!(d.status, Status::Ailing);
        assert!(d.remarks.contains(&Remark::Blocked));
    }

    #[test]
    fn mostly_fallback_dates_are_remarked() {
        let mut e1 = entry(0);
        e1.is_fallback = true;
        let mut e2 = entry(0);
        e2.guid = "other".into();
        e2.is_fallback = true;
        assert!(
            diagnose(&ev(vec![ok(0)], vec![e1, e2]))
                .remarks
                .contains(&Remark::DateFallback)
        );
    }
}
