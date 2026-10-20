//! Feed diagnosis: pure functions from evidence to a diagnosis. No I/O.

mod cadence;
pub mod hijack;
pub mod links;

pub use cadence::{
    DAILY_BELOW_DAYS, MONTHLY_BELOW_DAYS, SILENT_AFTER_DAYS, WEEKLY_BELOW_DAYS, cadence, classify,
};

use chrono::{DateTime, Duration, Utc};

use crate::model::{Action, Cadence, Diagnosis, ErrorKind, Evidence, FeedId, Hop, Remark, Status};
use crate::store::{AttemptRecord, EntryRecord};
use crate::url_key::{https_variant, site_of, url_key};

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
    let mut v = verdict(ev, &cadence, &hijack, upgrade);
    // The hint is for feeds already in doubt: never on an Alive feed, where a few
    // spam-like words are noise (decision of 15/10/2026), nor on a verdict that is not an
    // assessment of the feed itself.
    let hinted = !matches!(
        v.status,
        Status::Alive | Status::Hijacked | Status::Duplicate | Status::Unchecked
    );
    if hinted && hijack.score > 0.0 {
        v.bullets.push(format!(
            "Possible spam signs (score {:.1}, below the {HIJACK_THRESHOLD:.1} threshold):",
            hijack.score
        ));
        v.bullets.extend(hijack.bullets.iter().cloned());
    }
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
    if let Some(DeadReason {
        reason,
        clue,
        off_site_target,
    }) = dead_reason(ev)
    {
        // A feed announced by the new owner's site is not a replacement.
        let on_new_site = |u: &String| {
            off_site_target
                .as_ref()
                .is_some_and(|target| site_of(u) == site_of(target))
        };
        let adoptable = |u: &&String| url_key(u) != ev.url_key && !on_new_site(u);
        if let Some(url) = ev.discovered_url.as_ref().filter(adoptable) {
            let mut v = Verdict::new(
                Status::Moved,
                "The feed is gone, but its site now announces another one.",
                "→ new",
                Some(Action::AdoptDiscovered { url: url.clone() }),
            );
            v.bullets = vec![reason, format!("The site links to {url}.")];
            return v;
        }
        let mut v = Verdict::new(Status::Dead, reason, clue, Some(Action::Bury));
        if let Some(target) = &off_site_target {
            v.bullets.push(format!("{} → {target}", ev.xml_url));
        }
        // We cannot tell the same owner from a new one, so the feed stays buried,
        // but it is shown rather than hidden.
        if let Some(url) = ev
            .discovered_url
            .as_ref()
            .filter(|u| url_key(u) != ev.url_key && on_new_site(u))
        {
            v.bullets.push(format!(
                "The new site announces {url}; adopt it by hand only if it is the same publication."
            ));
        }
        return v;
    }
    if let Some(err) = &last.error {
        let mut v = Verdict::new(
            Status::Ailing,
            format!("Last fetch failed: {}.", err.label()),
            err.clue(),
            upgrade,
        );
        if unconfirmed_gone(ev) {
            v.bullets.push(
                "It worked on the fetch before; the next fetch will tell if it is gone.".into(),
            );
        }
        return v;
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

/// The first hop status and the URL reached by the leading run of permanent hops
/// (`301`/`308`), when that URL is another feed. A temporary hop ends the run: its
/// target may change again, so it is never proposed as the new address.
fn permanent_redirect(last: &AttemptRecord, own_key: &str) -> Option<(u16, String)> {
    let hops: Vec<&Hop> = last
        .redirect_chain
        .iter()
        .take_while(|h| h.status == 301 || h.status == 308)
        .collect();
    let target = &hops.last()?.url;
    (url_key(target) != own_key).then(|| (hops[0].status, target.clone()))
}

/// Why a feed is Dead, with the redirect target when the feed was shut down for good
/// by sending it to another site.
#[derive(Debug, Clone, PartialEq)]
pub struct DeadReason {
    pub reason: String,
    pub clue: String,
    pub off_site_target: Option<String>,
}

impl DeadReason {
    fn new(reason: impl Into<String>, clue: impl Into<String>) -> Self {
        DeadReason {
            reason: reason.into(),
            clue: clue.into(),
            off_site_target: None,
        }
    }
}

/// The last hop's URL when the chain left the feed's own site.
fn off_site_hop<'a>(last: &'a AttemptRecord, xml_url: &str) -> Option<&'a str> {
    let hop = last.redirect_chain.last()?;
    let (target, own) = (host_of(&hop.url)?, host_of(xml_url)?);
    let bare = |h: &str| h.strip_prefix("www.").unwrap_or(h).to_string();
    let (target, own) = (bare(&target), bare(&own));
    let same_host = target == own
        || target.ends_with(&format!(".{own}"))
        || own.ends_with(&format!(".{target}"));
    (!same_host && site_of(&hop.url)? != site_of(xml_url)?).then_some(hop.url.as_str())
}

/// Single tokens that mark a consent, login or account wall rather than a shutdown page.
const WALL_TOKENS: [&str; 11] = [
    "consent",
    "login",
    "signin",
    "auth",
    "auth0",
    "oauth",
    "authorize",
    "sso",
    "okta",
    "account",
    "accounts",
];

/// Whether the host labels and path segments, split on `-` `_` `.` `/`, hold a wall token
/// (`login`, `sign_in`, `log-in`, `auth0`, `okta`, ...). `authors` is not a wall.
fn looks_like_a_wall(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let text = format!("{}{}", u.host_str().unwrap_or(""), u.path()).to_ascii_lowercase();
    let tokens: Vec<&str> = text
        .split(['-', '_', '.', '/'])
        .filter(|t| !t.is_empty())
        .collect();
    tokens.iter().any(|t| WALL_TOKENS.contains(t))
        || tokens
            .windows(2)
            .any(|w| matches!(w, ["log" | "sign", "in"]))
}

/// Whether the URL's path looks like a feed (`feed`, `rss`, `atom`, `xml`, `rdf`).
fn looks_like_a_feed(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let path = u.path().to_ascii_lowercase();
    ["feed", "rss", "atom", "xml", "rdf"]
        .iter()
        .any(|w| path.contains(w))
}

/// HTML where a feed should be, or a client error other than 408/429. A 401/403 or a
/// challenge page whose final address looks like a feed is a firewall in front of a live
/// publisher; on any other page it is the new site's own gate.
fn is_definitive_off_site(err: &ErrorKind, target: &str) -> bool {
    match err {
        ErrorKind::HtmlInsteadOfFeed => true,
        ErrorKind::Http(401 | 403) | ErrorKind::Challenge => !looks_like_a_feed(target),
        ErrorKind::Http(code) => (400..500).contains(code) && !matches!(code, 408 | 429),
        _ => false,
    }
}

/// Whether the last meaningful attempt is a 404/410 right after one that worked. A whole
/// host can answer that way for a few hours (YouTube, 20/10/2026), so the next fetch has
/// to confirm it before the feed is Dead.
fn unconfirmed_gone(ev: &FeedEvidence) -> bool {
    let mut meaningful = ev.attempts.iter().filter(|a| !a.network_suspect);
    matches!(
        meaningful.next().and_then(|a| a.error.as_ref()),
        Some(ErrorKind::Http(404 | 410))
    ) && meaningful.next().is_some_and(|a| a.error.is_none())
}

/// Dead rule (spec §7): 404/410 on the last meaningful attempt, unless the one before
/// worked; NXDOMAIN; a redirect to another site that serves no feed; or a long enough
/// streak of failures.
pub fn dead_reason(ev: &FeedEvidence) -> Option<DeadReason> {
    if unconfirmed_gone(ev) {
        return None;
    }
    let meaningful: Vec<&AttemptRecord> =
        ev.attempts.iter().filter(|a| !a.network_suspect).collect();
    let last = meaningful.first()?;
    match &last.error {
        Some(ErrorKind::Http(code @ (404 | 410))) => {
            return Some(DeadReason::new(
                format!("The server says the feed is gone (HTTP {code})."),
                code.to_string(),
            ));
        }
        Some(ErrorKind::Dns) => {
            // The failing lookup is the last hop's host when the chain left our own.
            let redirected_away = last
                .redirect_chain
                .last()
                .is_some_and(|h| host_of(&h.url) != host_of(&ev.xml_url));
            let reason = if redirected_away {
                "The site it redirects to no longer resolves."
            } else {
                "The domain no longer resolves."
            };
            return Some(DeadReason::new(reason, "DNS"));
        }
        // Only a definitive answer from the other site counts: 5xx, timeouts, TLS,
        // challenge pages and the like may be transient or fixable.
        Some(err) => {
            if let Some(target) = off_site_hop(last, &ev.xml_url)
                && is_definitive_off_site(err, target)
                && !looks_like_a_wall(target)
            {
                return Some(DeadReason {
                    reason: "The feed now redirects to another site that serves no feed.".into(),
                    clue: "→ off".into(),
                    off_site_target: Some(target.to_string()),
                });
            }
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
            return Some(DeadReason::new(
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

fn host_of(raw: &str) -> Option<String> {
    url::Url::parse(raw)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
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
    if last.is_some_and(|a| matches!(a.error, Some(ErrorKind::Http(403) | ErrorKind::Challenge))) {
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
            let d = diagnose(&ev(
                vec![failed(0, kind), failed(1, ErrorKind::Timeout), ok(30)],
                vec![entry(40)],
            ));
            assert_eq!(d.status, Status::Dead);
            assert_eq!(d.evidence.clue, clue);
            assert_eq!(d.action, Some(Action::Bury));
        }
        let first = diagnose(&ev(vec![failed(0, ErrorKind::Http(404))], vec![]));
        assert_eq!(
            first.status,
            Status::Dead,
            "a first fetch has nothing to wait for"
        );
    }

    /// Real run 2026-10-20: YouTube answered 404 for all 13 of its feeds in one run and
    /// 200 again a few hours later. A 404/410 right after a fetch that worked waits for
    /// the next fetch.
    #[test]
    fn gone_right_after_a_working_fetch_waits_for_confirmation() {
        for code in [404, 410] {
            let d = diagnose(&ev(
                vec![failed(0, ErrorKind::Http(code)), ok(1)],
                vec![entry(3)],
            ));
            assert_eq!(d.status, Status::Ailing, "{code}");
            assert_eq!(d.reason, format!("Last fetch failed: HTTP {code}."));
            assert_eq!(d.evidence.clue, code.to_string());
            assert_eq!(d.action, None);
            assert_eq!(
                d.evidence.bullets.last().unwrap(),
                "It worked on the fetch before; the next fetch will tell if it is gone."
            );
            let confirmed = diagnose(&ev(
                vec![
                    failed(0, ErrorKind::Http(code)),
                    failed(1, ErrorKind::Http(code)),
                    ok(2),
                ],
                vec![entry(3)],
            ));
            assert_eq!(confirmed.status, Status::Dead, "{code} confirmed");
        }
    }

    #[test]
    fn unconfirmed_gone_skips_network_suspects_and_adopts_nothing() {
        let mut suspect = failed(1, ErrorKind::Dns);
        suspect.network_suspect = true;
        let mut e = ev(
            vec![failed(0, ErrorKind::Http(404)), suspect, ok(2)],
            vec![entry(3)],
        );
        assert_eq!(diagnose(&e).status, Status::Ailing);
        e.discovered_url = Some("https://blog.example.org/new-feed.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Ailing);
        assert_eq!(d.action, None);
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

    /// Real run 2026-10-10: Actualitté answers 301 (www → bare domain, same url_key),
    /// then 302 to /rss-main.rss. Only the permanent hops may move a feed.
    #[test]
    fn temporary_hop_after_a_permanent_one_is_not_a_move() {
        let mut e = ev(vec![], vec![entry(1)]);
        e.xml_url = "https://www.actualitte.com/flux/rss/actualites.xml".into();
        e.url_key = url_key(&e.xml_url);
        let mut a = ok(0);
        a.redirect_chain = vec![
            Hop {
                status: 301,
                url: "https://actualitte.com/flux/rss/actualites.xml".into(),
            },
            Hop {
                status: 302,
                url: "https://actualitte.com/rss-main.rss".into(),
            },
        ];
        e.attempts = vec![a];
        assert_eq!(diagnose(&e).status, Status::Alive);
    }

    #[test]
    fn move_target_is_the_end_of_the_permanent_hops() {
        let mut a = ok(0);
        a.redirect_chain = vec![
            Hop {
                status: 301,
                url: "https://newblog.example.org/rss".into(),
            },
            Hop {
                status: 302,
                url: "https://cdn.example.net/newblog/rss?sig=1".into(),
            },
        ];
        let d = diagnose(&ev(vec![a], vec![entry(1)]));
        assert_eq!(d.status, Status::Moved);
        assert_eq!(
            d.action,
            Some(Action::FollowRedirect {
                url: "https://newblog.example.org/rss".into()
            })
        );
    }

    /// Real run 2026-10-10: action button dot net resolves, but redirects through
    /// innisfreehouseschool.com to cinderellaaffair.org, which does not.
    #[test]
    fn dns_failure_on_a_redirect_target_says_so() {
        let mut a = failed(0, ErrorKind::Dns);
        a.http_status = Some(301);
        a.redirect_chain = vec![
            Hop {
                status: 301,
                url: "https://innisfreehouseschool.com/".into(),
            },
            Hop {
                status: 301,
                url: "https://cinderellaaffair.org/".into(),
            },
        ];
        let d = diagnose(&ev(vec![a], vec![]));
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.reason, "The site it redirects to no longer resolves.");
        assert_eq!(d.evidence.clue, "DNS");
        let own = diagnose(&ev(vec![failed(0, ErrorKind::Dns)], vec![]));
        assert_eq!(own.reason, "The domain no longer resolves.");
    }

    fn redirected(kind: ErrorKind, hops: &[(u16, &str)]) -> AttemptRecord {
        let mut a = failed(0, kind);
        a.redirect_chain = hops
            .iter()
            .map(|(status, url)| Hop {
                status: *status,
                url: (*url).into(),
            })
            .collect();
        a
    }

    /// Real run 2026-10-10: FiveThirtyEight (301 to an ABC News HTML page) was Hijacked.
    #[test]
    fn owner_shutdown_redirect_is_dead_not_hijacked() {
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://abcnews.com/politics")],
        );
        let d = diagnose(&ev(vec![a], vec![]));
        assert_eq!(d.status, Status::Dead);
        assert_ne!(d.status, Status::Hijacked);
        assert_eq!(
            d.reason,
            "The feed now redirects to another site that serves no feed."
        );
        assert_eq!(d.evidence.clue, "→ off");
        assert_eq!(
            d.evidence.bullets.last().unwrap(),
            "https://blog.example.org/feed → https://abcnews.com/politics"
        );
        assert_eq!(d.action, Some(Action::Bury));
    }

    /// Real run 2026-10-10: ioudgine (Typepad closed) goes 302 to networksolutions.com, 403.
    #[test]
    fn closed_host_redirect_ending_in_403_is_dead() {
        let a = redirected(
            ErrorKind::Http(403),
            &[(302, "https://www.networksolutions.com/typepad")],
        );
        let d = diagnose(&ev(vec![a], vec![]));
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.evidence.clue, "→ off");
    }

    #[test]
    fn same_site_redirect_ending_in_html_stays_ailing() {
        let mut e = ev(vec![], vec![]);
        e.xml_url = "https://www.example.com/feed".into();
        e.url_key = url_key(&e.xml_url);
        e.attempts = vec![redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://example.com/blog")],
        )];
        assert_eq!(diagnose(&e).status, Status::Ailing);
    }

    #[test]
    fn off_site_redirect_ending_in_a_transient_error_stays_ailing() {
        for kind in [
            ErrorKind::Http(503),
            ErrorKind::Http(500),
            ErrorKind::Http(429),
            ErrorKind::Http(408),
            ErrorKind::Timeout,
        ] {
            let a = redirected(kind.clone(), &[(301, "https://publisher.example.net/rss")]);
            let d = diagnose(&ev(vec![a], vec![]));
            assert_eq!(d.status, Status::Ailing, "{kind:?}");
        }
    }

    #[test]
    fn off_site_shutdown_does_not_adopt_a_feed_of_the_new_owner() {
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://abcnews.com/politics")],
        );
        let mut e = ev(vec![a], vec![]);
        e.discovered_url = Some("https://abcnews.com/rss.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.action, Some(Action::Bury));
        // A feed found on the feed's own site is still a replacement.
        e.discovered_url = Some("https://blog.example.org/new.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Moved);
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
    fn challenge_page_is_ailing_and_blocked() {
        let d = diagnose(&ev(vec![failed(0, ErrorKind::Challenge)], vec![]));
        assert_eq!(d.status, Status::Ailing);
        assert_eq!(d.reason, "Last fetch failed: anti-bot challenge page.");
        assert!(d.remarks.contains(&Remark::Blocked));
        let plain = diagnose(&ev(vec![failed(0, ErrorKind::Http(503))], vec![]));
        assert!(!plain.remarks.contains(&Remark::Blocked));
    }

    /// Real run 2026-10-10: Bibliobsession 2.0 is Dead on a 404 with a hijack score of 0.4.
    #[test]
    fn spam_hint_below_the_threshold_is_shown_in_the_evidence() {
        let mut e = ev(vec![failed(0, ErrorKind::Http(404))], vec![]);
        e.description = Some("Les meilleurs casino en ligne pour les joueurs français.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.reason, "The server says the feed is gone (HTTP 404).");
        assert_eq!(d.evidence.clue, "404");
        assert_eq!(d.evidence.language, None);
        let at = d
            .evidence
            .bullets
            .iter()
            .position(|b| b == "Possible spam signs (score 0.4, below the 0.6 threshold):")
            .unwrap_or_else(|| panic!("{:?}", d.evidence.bullets));
        assert!(d.evidence.bullets[at + 1].contains("casino"));
    }

    /// Decision of 15/10/2026: the hint is for feeds that are not alive. On the real run,
    /// ReadWriteWeb (Alive) showed it for betting words in a tech site.
    #[test]
    fn alive_feed_with_spam_words_gets_no_hint() {
        let mut e = ev(vec![ok(0)], vec![entry(3)]);
        e.description = Some("Les meilleurs casino en ligne pour les joueurs français.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Alive);
        assert!(d.hijack_score > 0.0, "the score itself is still stored");
        assert!(
            d.evidence
                .bullets
                .iter()
                .all(|b| !b.contains("spam") && !b.contains("casino")),
            "{:?}",
            d.evidence.bullets
        );
    }

    #[test]
    fn dormant_and_fossil_feeds_keep_the_hint() {
        for (days, status) in [(200, Status::Dormant), (800, Status::Fossil)] {
            let mut e = ev(vec![ok(0)], vec![entry(days)]);
            e.description = Some("Les meilleurs casino en ligne pour les joueurs français.".into());
            let d = diagnose(&e);
            assert_eq!(d.status, status);
            assert!(
                d.evidence
                    .bullets
                    .iter()
                    .any(|b| b.starts_with("Possible spam signs (score 0.4")),
                "{status:?}: {:?}",
                d.evidence.bullets
            );
        }
    }

    #[test]
    fn clean_feed_gets_no_spam_hint() {
        let d = diagnose(&ev(vec![ok(0)], vec![entry(3)]));
        assert_eq!(d.status, Status::Alive);
        assert!(d.evidence.bullets.iter().all(|b| !b.contains("spam")));
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

    fn feed_at(url: &str, attempt: AttemptRecord) -> FeedEvidence {
        let mut e = ev(vec![attempt], vec![]);
        e.xml_url = url.into();
        e.url_key = url_key(url);
        e
    }

    /// Review I4: `www.` and subdomain hops on short ccTLD domains stay on the same site.
    #[test]
    fn www_and_subdomain_hops_are_the_same_site() {
        for (feed, hop) in [
            ("https://www.rfi.fr/fr/podcasts/feed", "https://rfi.fr/fr/"),
            ("https://feeds.nos.nl/nosnieuws", "https://nos.nl/"),
            ("https://rfi.fr/feed", "https://www.rfi.fr/"),
            ("https://news.example.net/feed", "https://example.net/"),
        ] {
            let a = redirected(ErrorKind::HtmlInsteadOfFeed, &[(301, hop)]);
            let d = diagnose(&feed_at(feed, a));
            assert_eq!(d.status, Status::Ailing, "{feed}");
        }
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://www.bbc.co.uk/news")],
        );
        let d = diagnose(&feed_at("https://other.co.uk/feed", a));
        assert_eq!(d.status, Status::Dead);
    }

    /// Review I5 (a): a 401/403 behind a redirect to a feed-looking URL is a WAF, not a shutdown.
    #[test]
    fn off_site_403_on_a_feed_looking_target_stays_ailing_and_blocked() {
        for target in [
            "https://example.com/feed/",
            "https://example.com/blog/RSS.xml",
            "https://example.com/atom",
            "https://example.com/index.rdf",
        ] {
            for code in [401, 403] {
                let a = redirected(ErrorKind::Http(code), &[(301, target)]);
                let mut e = feed_at("http://feeds.feedburner.com/example", a);
                e.url_key = url_key(&e.xml_url);
                let d = diagnose(&e);
                assert_eq!(d.status, Status::Ailing, "{code} {target}");
                if code == 403 {
                    assert!(d.remarks.contains(&Remark::Blocked));
                }
            }
        }
        // A 404 on a feed-looking target is still gone, and HTML is still a shutdown.
        let a = redirected(ErrorKind::Http(404), &[(301, "https://example.com/feed/")]);
        assert_eq!(
            diagnose(&feed_at("http://a.example.org/f", a)).status,
            Status::Dead
        );
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://example.com/feed/")],
        );
        assert_eq!(
            diagnose(&feed_at("http://a.example.org/f", a)).status,
            Status::Dead
        );
    }

    /// Review M9: consent and login walls are not shutdowns.
    #[test]
    fn consent_and_login_walls_do_not_trigger_the_off_site_rule() {
        for target in [
            "https://consent.yahoo.com/collectConsent?sessionId=1",
            "https://login.microsoftonline.com/common/oauth2",
            "https://example.com/login?next=/feed",
            "https://example.com/users/sign-in",
            "https://example.com/signin.php",
            "https://accounts.google.com/ServiceLogin",
            "https://example.com/sso/start",
            "https://auth.example.com/",
            "https://example.com/account/home",
            "https://wordpress.com/log-in?redirect_to=x",
            "https://example.com/users/sign_in",
            "https://tenant.auth0.com/login",
            "https://acme.okta.com/app",
            "https://example.com/authorize?client=1",
            "https://example.com/oauth/start",
            "https://example.com/Users/Sign-In",
        ] {
            let a = redirected(ErrorKind::HtmlInsteadOfFeed, &[(302, target)]);
            let d = diagnose(&feed_at("https://blog.example.org/feed", a));
            assert_eq!(d.status, Status::Ailing, "{target}");
        }
        // Similar-looking words are not walls.
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://example.com/authors/jane")],
        );
        assert_eq!(
            diagnose(&feed_at("https://blog.example.org/feed", a)).status,
            Status::Dead
        );
    }

    /// Review I6: a feed announced by the new site is shown, never adopted.
    #[test]
    fn off_site_shutdown_shows_the_feed_the_new_site_announces() {
        let a = redirected(
            ErrorKind::HtmlInsteadOfFeed,
            &[(301, "https://newhome.example.net/")],
        );
        let mut e = ev(vec![a], vec![]);
        e.discovered_url = Some("https://newhome.example.net/feed.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.action, Some(Action::Bury));
        assert!(d.evidence.bullets.contains(
            &"The new site announces https://newhome.example.net/feed.xml; adopt it by hand only if it is the same publication."
                .to_string()
        ));
        // Without a discovered feed, no such bullet.
        e.discovered_url = None;
        let d = diagnose(&e);
        assert!(d.evidence.bullets.iter().all(|b| !b.contains("announces")));
    }

    /// Review M10: the hint is for assessed feeds only, never added to a Hijacked verdict.
    #[test]
    fn hijacked_feed_does_not_get_the_hint_bullets_twice() {
        let mut e = ev(vec![failed(0, ErrorKind::HtmlInsteadOfFeed)], vec![]);
        e.description =
            Some("Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Hijacked);
        assert!(
            d.evidence
                .bullets
                .iter()
                .all(|b| !b.contains("Possible spam"))
        );
        let mut sorted = d.evidence.bullets.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            d.evidence.bullets.len(),
            "{:?}",
            d.evidence.bullets
        );
    }

    /// Re-review R1: a challenge on a non-feed page of another site is the new site's gate.
    #[test]
    fn off_site_challenge_follows_the_403_rule() {
        let a = redirected(
            ErrorKind::Challenge,
            &[(302, "https://www.networksolutions.com/typepad")],
        );
        let d = diagnose(&feed_at("http://ioudgine.typepad.com/blog/atom.xml", a));
        assert_eq!(d.status, Status::Dead);
        assert_eq!(d.evidence.clue, "→ off");
        let a = redirected(ErrorKind::Challenge, &[(301, "https://example.com/feed/")]);
        let d = diagnose(&feed_at("http://feeds.feedburner.com/example", a));
        assert_eq!(d.status, Status::Ailing);
        assert!(d.remarks.contains(&Remark::Blocked));
    }
}
