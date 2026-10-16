//! Domain types shared by every module and persisted in the database.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub type FeedId = i64;
pub type FolderId = i64;

/// Database timestamp format: RFC 3339, UTC, millisecond precision, `Z` suffix.
pub fn ts(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Duplicate,
    Hijacked,
    Moved,
    Dead,
    Ailing,
    Fossil,
    Dormant,
    Alive,
    Unchecked,
}

impl Status {
    /// Evaluation order of the diagnosis, which is also the garden's urgency order.
    pub const ALL: [Status; 9] = [
        Status::Duplicate,
        Status::Hijacked,
        Status::Moved,
        Status::Dead,
        Status::Ailing,
        Status::Fossil,
        Status::Dormant,
        Status::Alive,
        Status::Unchecked,
    ];

    /// 0 is the most urgent.
    pub fn urgency(self) -> usize {
        Self::ALL
            .iter()
            .position(|s| *s == self)
            .unwrap_or(usize::MAX)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Duplicate => "Duplicate",
            Status::Hijacked => "Hijacked",
            Status::Moved => "Moved",
            Status::Dead => "Dead",
            Status::Ailing => "Ailing",
            Status::Fossil => "Fossil",
            Status::Dormant => "Dormant",
            Status::Alive => "Alive",
            Status::Unchecked => "Unchecked",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|st| st.as_str() == s)
    }

    /// Statuses that put an undecided feed in the garden queue.
    pub fn needs_triage(self) -> bool {
        matches!(
            self,
            Status::Duplicate
                | Status::Hijacked
                | Status::Moved
                | Status::Dead
                | Status::Ailing
                | Status::Fossil
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    Dns,
    Connect,
    Tls,
    Timeout,
    Http(u16),
    Parse,
    Encoding,
    TooLarge,
    HtmlInsteadOfFeed,
    /// An anti-bot challenge page (Cloudflare, DataDome, ...) served instead of the feed.
    Challenge,
}

impl ErrorKind {
    /// Failures that can come from the user's own connection (network guard).
    pub fn is_network(&self) -> bool {
        matches!(self, ErrorKind::Dns | ErrorKind::Connect)
    }

    pub fn label(&self) -> String {
        match self {
            ErrorKind::Http(code) => format!("HTTP {code}"),
            ErrorKind::HtmlInsteadOfFeed => "HTML page instead of a feed".to_string(),
            ErrorKind::Challenge => "anti-bot challenge page".to_string(),
            ErrorKind::TooLarge => "response too large".to_string(),
            other => format!("{other:?}"),
        }
    }

    /// Short clue for the garden list column.
    pub fn clue(&self) -> String {
        match self {
            ErrorKind::Dns => "DNS".into(),
            ErrorKind::Connect => "conn".into(),
            ErrorKind::Tls => "TLS".into(),
            ErrorKind::Timeout => "timeout".into(),
            ErrorKind::Http(code) => code.to_string(),
            ErrorKind::Parse => "parse".into(),
            ErrorKind::Encoding => "charset".into(),
            ErrorKind::TooLarge => "huge".into(),
            ErrorKind::HtmlInsteadOfFeed => "HTML".into(),
            ErrorKind::Challenge => "bot".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hop {
    pub status: u16,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Keep,
    Repair,
    Bury,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Keep => "keep",
            Decision::Repair => "repair",
            Decision::Bury => "bury",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Decision::Keep, Decision::Repair, Decision::Bury]
            .into_iter()
            .find(|d| d.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    UpgradeHttps { url: String },
    FollowRedirect { url: String },
    AdoptDiscovered { url: String },
    MergeInto { feed_id: FeedId },
    Bury,
}

impl Action {
    /// Repairs that `R` may apply in bulk without individual review.
    pub fn is_safe(&self) -> bool {
        matches!(
            self,
            Action::UpgradeHttps { .. } | Action::FollowRedirect { .. }
        )
    }

    /// Actions applied by the garden's `r` key (everything except burying).
    pub fn is_repair(&self) -> bool {
        !matches!(self, Action::Bury)
    }

    pub fn label(&self) -> String {
        match self {
            Action::UpgradeHttps { url } => format!("switch to {url}"),
            Action::FollowRedirect { url } => format!("follow redirect to {url}"),
            Action::AdoptDiscovered { url } => format!("adopt discovered feed {url}"),
            Action::MergeInto { feed_id } => format!("merge into #{feed_id}"),
            Action::Bury => "bury → cemetery".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Remark {
    HttpsAvailable,
    HttpOnly,
    SimilarTitle { feed_id: FeedId },
    DateFallback,
    Blocked,
}

/// Human-readable proof behind a diagnosis.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// One English sentence per fact, shown in the garden's detail pane.
    pub bullets: Vec<String>,
    /// Short clue for the list column (`410`, `2011`, `14 mo`, `vie ⚠`, `= #41`).
    pub clue: String,
    /// ISO 639-3 code of a suspicious description language, if any.
    pub language: Option<String>,
}

/// A feed's publication rhythm (spec §4), from the median gap between its recent entries.
/// `Silent` is the default: no entry at all, or none for a year.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CadenceGroup {
    Daily,
    Weekly,
    Monthly,
    Rare,
    #[default]
    Silent,
}

impl CadenceGroup {
    /// Display order of the sidebar's Rhythm section and of pages 105 to 109.
    pub const ALL: [CadenceGroup; 5] = [
        CadenceGroup::Daily,
        CadenceGroup::Weekly,
        CadenceGroup::Monthly,
        CadenceGroup::Rare,
        CadenceGroup::Silent,
    ];

    /// The stored and printed name: `feed_health.cadence_group`, `feed.cadence_override`,
    /// the CLI's arguments and JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            CadenceGroup::Daily => "daily",
            CadenceGroup::Weekly => "weekly",
            CadenceGroup::Monthly => "monthly",
            CadenceGroup::Rare => "rare",
            CadenceGroup::Silent => "silent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == s)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cadence {
    pub last_entry_at: Option<DateTime<Utc>>,
    pub median_interval_days: Option<f64>,
    pub entries_per_month: f64,
    /// Computed by `health::classify`; the user's override lives on `feed`, not here.
    #[serde(default)]
    pub group: CadenceGroup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub status: Status,
    pub reason: String,
    pub evidence: Evidence,
    pub remarks: Vec<Remark>,
    pub action: Option<Action>,
    pub hijack_score: f32,
    pub cadence: Cadence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_order_is_urgency_order() {
        assert_eq!(Status::Duplicate.urgency(), 0);
        assert!(Status::Dead.urgency() < Status::Ailing.urgency());
        assert_eq!(Status::Unchecked.urgency(), 8);
    }

    #[test]
    fn status_round_trips_through_text() {
        for s in Status::ALL {
            assert_eq!(Status::parse(s.as_str()), Some(s));
        }
        assert_eq!(Status::parse("Zombie"), None);
    }

    #[test]
    fn cadence_group_round_trips_through_text_and_json() {
        for g in CadenceGroup::ALL {
            assert_eq!(CadenceGroup::parse(g.as_str()), Some(g));
            assert_eq!(
                serde_json::to_value(g).unwrap(),
                serde_json::Value::String(g.as_str().to_string())
            );
        }
        assert_eq!(
            CadenceGroup::ALL.map(CadenceGroup::as_str),
            ["daily", "weekly", "monthly", "rare", "silent"]
        );
        assert_eq!(CadenceGroup::parse("Daily"), None, "lowercase only");
        assert_eq!(CadenceGroup::default(), CadenceGroup::Silent);
    }

    #[test]
    fn a_cadence_without_a_group_reads_as_silent() {
        let c: Cadence = serde_json::from_str(
            r#"{"last_entry_at":null,"median_interval_days":null,"entries_per_month":0.0}"#,
        )
        .unwrap();
        assert_eq!(c.group, CadenceGroup::Silent);
        assert_eq!(Cadence::default().group, CadenceGroup::Silent);
    }

    #[test]
    fn only_problem_statuses_need_triage() {
        let triage: Vec<_> = Status::ALL
            .into_iter()
            .filter(|s| s.needs_triage())
            .collect();
        assert_eq!(
            triage,
            vec![
                Status::Duplicate,
                Status::Hijacked,
                Status::Moved,
                Status::Dead,
                Status::Ailing,
                Status::Fossil
            ]
        );
    }

    #[test]
    fn timestamps_round_trip() {
        let t: DateTime<Utc> = "2026-10-10T07:42:01.123Z".parse().unwrap();
        assert_eq!(ts(t), "2026-10-10T07:42:01.123Z");
        assert_eq!(parse_ts(&ts(t)), Some(t));
    }

    #[test]
    fn error_kind_json_and_labels() {
        let json = serde_json::to_string(&ErrorKind::Http(410)).unwrap();
        assert_eq!(
            serde_json::from_str::<ErrorKind>(&json).unwrap(),
            ErrorKind::Http(410)
        );
        assert_eq!(serde_json::to_string(&ErrorKind::Dns).unwrap(), "\"Dns\"");
        assert_eq!(ErrorKind::Http(410).label(), "HTTP 410");
        assert_eq!(ErrorKind::Timeout.clue(), "timeout");
        assert_eq!(
            serde_json::to_string(&ErrorKind::Challenge).unwrap(),
            "\"Challenge\""
        );
        assert_eq!(ErrorKind::Challenge.label(), "anti-bot challenge page");
        assert_eq!(ErrorKind::Challenge.clue(), "bot");
        assert!(!ErrorKind::Challenge.is_network());
        assert!(ErrorKind::Connect.is_network());
        assert!(!ErrorKind::Timeout.is_network());
    }

    #[test]
    fn action_safety() {
        assert!(
            Action::UpgradeHttps {
                url: "https://a".into()
            }
            .is_safe()
        );
        assert!(
            Action::FollowRedirect {
                url: "https://a".into()
            }
            .is_safe()
        );
        assert!(
            !Action::AdoptDiscovered {
                url: "https://a".into()
            }
            .is_safe()
        );
        assert!(!Action::MergeInto { feed_id: 1 }.is_safe());
        assert!(Action::MergeInto { feed_id: 1 }.is_repair());
        assert!(!Action::Bury.is_repair());
    }

    #[test]
    fn decision_round_trips() {
        for d in [Decision::Keep, Decision::Repair, Decision::Bury] {
            assert_eq!(Decision::parse(d.as_str()), Some(d));
        }
    }
}
