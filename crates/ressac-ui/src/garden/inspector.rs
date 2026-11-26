//! The inspector's content for one feed: the Mac's `GardenInspector`, as plain data the
//! window (B5d) and Antiope's inspector page (B8) lay out.

use std::fmt::Display;

use chrono::{DateTime, TimeZone, Utc};
use ressac_core::model::{Action, Decision, FeedId, Status};
use ressac_core::store::{AttemptRecord, CadenceDetail, ThemeDef};

use super::{
    GardenRow, outcome, redirect_line, remark_sentence, rhythm_index, rhythm_line, status_line,
    theme_index,
};
use crate::sanitize::is_web_url;
use crate::sources::status_color;

pub const NO_FEED_TITLE: &str = "No Feed Selected";
pub const NO_FEED_TEXT: &str = "Select a feed to see why it is in the garden.";
pub const BURIED_NOTE: &str = "Buried. Press k to restore it.";
pub const HISTORY_TITLE: &str = "Redirects and fetch log";
pub const NOT_FETCHED: &str = "Not fetched yet.";
pub const ARCHIVED_COPY: &str = "Archived copy";

/// The rhythm line and the Rhythm picker's current item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmControl {
    pub line: String,
    pub index: usize,
}

/// One fetch-log line: `"Oct 10, 2026, 7:00 AM"`, `"HTTP 410"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub at: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inspector {
    pub feed_id: FeedId,
    pub title: String,
    pub folder_path: String,
    pub status: Status,
    pub status_line: String,
    /// `sources::status_color`: `""` is the secondary colour.
    pub status_color: &'static str,
    pub buried: bool,
    pub reason: String,
    /// Active feeds with a `v_cadence` row.
    pub rhythm: Option<RhythmControl>,
    /// Index into `theme_names`; `None` for a buried feed.
    pub theme: Option<usize>,
    pub evidence: Vec<String>,
    pub remarks: Vec<String>,
    /// The action's label, active feeds only.
    pub proposed: Option<String>,
    /// "Keep", "Repair" or "Bury".
    pub decision: Option<String>,
    pub feed_url: String,
    /// Web addresses only.
    pub site: Option<String>,
    pub wayback: Option<String>,
    /// "Keep", or "Restore" for a buried feed.
    pub keep_title: &'static str,
    pub can_repair: bool,
    pub can_bury: bool,
    pub bury_prominent: bool,
    pub redirects: Vec<String>,
    pub log: Vec<LogLine>,
}

impl Inspector {
    /// `log` is the feed's fetch log, newest first; its dates are shown in `tz`.
    pub fn of<Tz: TimeZone>(
        row: &GardenRow,
        cadence: Option<&CadenceDetail>,
        themes: &[ThemeDef],
        theme_key: Option<&str>,
        log: &[AttemptRecord],
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Inspector
    where
        Tz::Offset: Display,
    {
        let status = row.status();
        let diagnosis = row.diagnosis.as_ref();
        let buried = row.buried;
        let active = |present: bool| present && !buried;
        let web = |address: &Option<String>| address.clone().filter(|a| is_web_url(a));
        Inspector {
            feed_id: row.feed_id,
            title: row.title.clone(),
            folder_path: row.folder_path.clone(),
            status,
            status_line: status_line(status),
            status_color: status_color(Some(status)),
            buried,
            reason: diagnosis.map(|d| d.reason.clone()).unwrap_or_default(),
            rhythm: cadence.filter(|_| !buried).map(|c| RhythmControl {
                line: rhythm_line(c, now),
                index: rhythm_index(c),
            }),
            theme: (!buried).then(|| theme_index(themes, theme_key)),
            evidence: diagnosis.map_or_else(Vec::new, |d| {
                // A bullet with nothing but a label and punctuation ("Clue: .") says nothing.
                d.evidence
                    .bullets
                    .iter()
                    .filter(|b| {
                        let words = b.trim_matches(|c: char| !c.is_alphanumeric());
                        !words.is_empty() && words != "Clue"
                    })
                    .cloned()
                    .collect()
            }),
            remarks: diagnosis.map_or_else(Vec::new, |d| {
                d.remarks.iter().map(remark_sentence).collect()
            }),
            proposed: row.action().filter(|_| !buried).map(Action::label),
            decision: row.decision.as_ref().map(|d| {
                match d.decision {
                    Decision::Keep => "Keep",
                    Decision::Repair => "Repair",
                    Decision::Bury => "Bury",
                }
                .to_string()
            }),
            feed_url: row.xml_url.clone(),
            site: web(&row.html_url),
            wayback: web(&row.wayback_url),
            keep_title: if buried { "Restore" } else { "Keep" },
            can_repair: active(row.action().is_some_and(Action::is_repair)),
            can_bury: !buried,
            bury_prominent: matches!(row.action(), Some(Action::Bury)),
            redirects: log.first().map_or_else(Vec::new, |a| {
                a.redirect_chain.iter().map(redirect_line).collect()
            }),
            log: log
                .iter()
                .map(|a| LogLine {
                    at: a
                        .at
                        .with_timezone(tz)
                        .format("%b %-d, %Y, %-I:%M %p")
                        .to_string(),
                    outcome: outcome(a),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{GardenVisit, LOG_LINES, load};
    use crate::sources::GardenScope;
    use crate::test_db::{TestDb, at};
    use ressac_core::store::Store;

    fn fixture() -> (TestDb, Store) {
        let db = TestDb::full();
        db.execute(
            "INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), \
             ('lit', 'Literature', 'SEA OF LETTERS', 1); \
             UPDATE feed SET theme_id = (SELECT id FROM theme WHERE key = 'tech') WHERE id = 7",
        );
        let store = db.store();
        (db, store)
    }

    fn inspect(store: &Store, feed: FeedId) -> Inspector {
        let scope = if feed == 4 {
            GardenScope::Cemetery
        } else {
            GardenScope::All
        };
        let mut v = GardenVisit::default();
        v.enter(scope);
        v.update(load(store).unwrap());
        v.select(feed);
        v.inspector(
            &store.attempts(feed, LOG_LINES).unwrap(),
            at("2026-10-16T09:00:00Z"),
            &Utc,
        )
        .unwrap()
    }

    #[test]
    fn the_inspector_reads_like_the_mac_inspector() {
        let (_db, store) = fixture();
        let i = inspect(&store, 7);
        assert_eq!(
            (
                i.title.as_str(),
                i.folder_path.as_str(),
                i.status_line.as_str(),
                i.status_color
            ),
            ("Unphotographable", "Biblis", "🪦 Dead", "red")
        );
        assert!(!i.buried);
        assert_eq!(
            i.rhythm,
            Some(RhythmControl {
                line: "Rhythm: Silent".into(),
                index: 0
            })
        );
        assert_eq!(i.theme, Some(2));
        assert_eq!(
            (i.proposed.as_deref(), i.decision.as_deref()),
            (Some("bury → cemetery"), Some("Keep"))
        );
        assert_eq!(i.feed_url, "http://www.unphotographable.com/index.rdf");
        assert_eq!(i.site.as_deref(), Some("http://www.unphotographable.com/"));
        assert_eq!(
            i.wayback.as_deref(),
            Some("http://web.archive.org/web/2019/http://www.unphotographable.com/")
        );
        assert_eq!(
            (i.keep_title, i.can_repair, i.can_bury, i.bury_prominent),
            ("Keep", false, true, true)
        );
        assert_eq!(
            i.redirects,
            ["301 → https://www.unphotographable.com/index.rdf"]
        );
        assert_eq!(
            i.log,
            [LogLine {
                at: "Oct 10, 2026, 7:00 AM".into(),
                outcome: "HTTP 410".into()
            }]
        );
        let girl = inspect(&store, 5);
        assert_eq!(
            (girl.can_repair, girl.bury_prominent, girl.status_color),
            (true, false, "blue")
        );
        assert_eq!(
            girl.proposed.as_deref(),
            Some("follow redirect to https://girl.example.org/feed")
        );
    }

    #[test]
    fn a_buried_feed_offers_restore_and_nothing_else() {
        let (_db, store) = fixture();
        let i = inspect(&store, 4);
        assert!(i.buried);
        assert_eq!(
            (i.keep_title, i.can_repair, i.can_bury),
            ("Restore", false, false)
        );
        assert_eq!(
            (i.rhythm.as_ref(), i.theme, i.proposed.as_deref()),
            (None, None, None)
        );
        assert!(i.log.is_empty() && i.redirects.is_empty());
        assert_eq!(
            [BURIED_NOTE, NOT_FETCHED, NO_FEED_TITLE, NO_FEED_TEXT],
            [
                "Buried. Press k to restore it.",
                "Not fetched yet.",
                "No Feed Selected",
                "Select a feed to see why it is in the garden."
            ]
        );
    }

    /// Review Focus 3.
    #[test]
    fn the_inspector_shows_web_addresses_only() {
        let (db, store) = fixture();
        db.execute("UPDATE feed SET html_url = 'javascript:alert(1)', wayback_url = 'file:///etc/passwd' WHERE id = 7");
        let i = inspect(&store, 7);
        assert_eq!((i.site, i.wayback), (None, None));
    }

    #[test]
    fn fetch_dates_are_shown_in_the_given_time_zone() {
        let (_db, store) = fixture();
        let zone = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        let mut v = GardenVisit::default();
        v.enter(GardenScope::All);
        v.update(load(&store).unwrap());
        v.select(7);
        let i = v
            .inspector(
                &store.attempts(7, LOG_LINES).unwrap(),
                at("2026-10-16T09:00:00Z"),
                &zone,
            )
            .unwrap();
        assert_eq!(i.log[0].at, "Oct 10, 2026, 9:00 AM");
    }

    #[test]
    fn an_unchecked_feed_has_no_reason_evidence_or_proposal() {
        let (_db, store) = fixture();
        let mut v = GardenVisit::default();
        v.enter(GardenScope::All);
        v.update(load(&store).unwrap());
        let unchecked = v
            .snapshot()
            .rows
            .iter()
            .find(|r| r.status() == Status::Unchecked)
            .expect("the fixture has an unchecked feed")
            .feed_id;
        v.select(unchecked);
        let log = store.attempts(unchecked, LOG_LINES).unwrap();
        assert!(log.is_empty(), "never fetched: the window says NOT_FETCHED");
        let i = v.inspector(&log, at("2026-10-16T09:00:00Z"), &Utc).unwrap();
        assert_eq!(
            (i.status_line.as_str(), i.status_color, i.reason.as_str()),
            ("❔ Unchecked", "", "")
        );
        assert!(i.evidence.is_empty() && i.remarks.is_empty());
        assert_eq!(i.proposed, None);
        assert!(!i.can_repair);
    }
}
