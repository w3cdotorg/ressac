//! Garden logic, independent of any UI: rows, the triage queue, decisions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::model::{Action, Decision, Diagnosis, FeedId, Status};
use crate::store::{AttemptRecord, DecisionRecord, Store};

/// Entries shown in the detail pane.
const RECENT_ENTRIES: usize = 3;
/// Fetch attempts shown in the expanded detail pane.
const RECENT_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct GardenRow {
    pub feed_id: FeedId,
    pub title: String,
    pub folder_path: String,
    pub top_folder: String,
    pub xml_url: String,
    pub html_url: Option<String>,
    pub wayback_url: Option<String>,
    pub buried: bool,
    pub diagnosis: Option<Diagnosis>,
    pub decision: Option<DecisionRecord>,
    pub recent: Vec<(DateTime<Utc>, String)>,
    /// Last fetch attempts, newest first.
    pub attempts: Vec<AttemptRecord>,
}

impl GardenRow {
    pub fn status(&self) -> Status {
        self.diagnosis
            .as_ref()
            .map_or(Status::Unchecked, |d| d.status)
    }

    pub fn action(&self) -> Option<&Action> {
        self.diagnosis.as_ref().and_then(|d| d.action.as_ref())
    }

    pub fn needs_triage(&self) -> bool {
        !self.buried
            && needs_triage(
                self.status(),
                self.decision.as_ref().map(|d| self.baseline(d)),
            )
    }

    /// The status a decision is measured against. A repair is meant to leave the feed
    /// healthy, so once the feed was fetched again after it, any problem status brings it
    /// back (Alive baseline). Until then its diagnosis still describes the old address.
    fn baseline(&self, d: &DecisionRecord) -> Status {
        let fetched_since = self.attempts.first().is_some_and(|a| a.at > d.decided_at);
        if d.decision == Decision::Repair && fetched_since {
            Status::Alive
        } else {
            d.status_at_decision
        }
    }
}

/// A feed needs triage when its status is a problem and it is undecided, or it got
/// strictly more urgent than its decision baseline (spec §9).
pub fn needs_triage(status: Status, decided_at: Option<Status>) -> bool {
    status.needs_triage() && decided_at.is_none_or(|prev| status.urgency() < prev.urgency())
}

pub fn load_rows(store: &Store) -> Result<Vec<GardenRow>> {
    let paths = store.folder_paths()?;
    let mut diags = store.diagnoses()?;
    let mut decisions = store.latest_decisions()?;
    let mut rows = Vec::new();
    for f in store.feeds()? {
        let folder_path = f
            .folder_id
            .and_then(|id| paths.get(&id).cloned())
            .unwrap_or_default();
        let top_folder = folder_path
            .split(" / ")
            .next()
            .unwrap_or_default()
            .to_string();
        let recent = store
            .entries(f.id, RECENT_ENTRIES)?
            .into_iter()
            .map(|e| {
                (
                    e.published_at,
                    e.title.unwrap_or_else(|| "(untitled)".into()),
                )
            })
            .collect();
        rows.push(GardenRow {
            feed_id: f.id,
            title: f.title,
            folder_path,
            top_folder,
            xml_url: f.xml_url,
            html_url: f.html_url,
            wayback_url: f.wayback_url,
            buried: f.state == "buried",
            diagnosis: diags.remove(&f.id),
            decision: decisions.remove(&f.id),
            recent,
            attempts: store.attempts(f.id, RECENT_ATTEMPTS)?,
        });
    }
    rows.sort_by(|a, b| {
        (a.status().urgency(), &a.top_folder, a.title.to_lowercase()).cmp(&(
            b.status().urgency(),
            &b.top_folder,
            b.title.to_lowercase(),
        ))
    });
    Ok(rows)
}

/// Stores every feed's triage verdict (`feed_health.needs_triage`) so that other clients
/// read the rule instead of reimplementing it. Called after every diagnosis, decision and undo.
pub fn recompute_triage(store: &Store) -> Result<()> {
    let flags: HashMap<FeedId, bool> = load_rows(store)?
        .iter()
        .map(|r| (r.feed_id, r.needs_triage()))
        .collect();
    store.save_triage(&flags)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub decided: usize,
    pub needing: usize,
}

pub fn progress(rows: &[GardenRow]) -> Progress {
    let triage: Vec<&GardenRow> = rows.iter().filter(|r| r.status().needs_triage()).collect();
    Progress {
        decided: triage.iter().filter(|r| !r.needs_triage()).count(),
        needing: triage.len(),
    }
}

/// Undecided active feeds whose proposed action is safe, in row order.
pub fn safe_repairs(rows: &[GardenRow]) -> Vec<(FeedId, Action)> {
    rows.iter()
        .filter(|r| !r.buried && r.decision.is_none())
        .filter_map(|r| {
            r.action()
                .filter(|a| a.is_safe())
                .map(|a| (r.feed_id, a.clone()))
        })
        .collect()
}

pub fn decide(
    store: &Store,
    feed_id: FeedId,
    decision: Decision,
    now: DateTime<Utc>,
) -> Result<()> {
    let diagnosis = store.diagnosis(feed_id)?;
    let status = diagnosis.as_ref().map_or(Status::Unchecked, |d| d.status);
    let action = match decision {
        Decision::Repair => diagnosis.and_then(|d| d.action),
        Decision::Keep | Decision::Bury => None,
    };
    store.record_decision(feed_id, decision, status, action.as_ref(), now)?;
    recompute_triage(store)
}

pub fn undo(store: &Store) -> Result<Option<FeedId>> {
    let undone = store.undo_last_decision()?;
    recompute_triage(store)?;
    Ok(undone)
}

pub fn apply_safe_repairs(store: &Store, now: DateTime<Utc>) -> Result<(usize, Vec<String>)> {
    let rows = load_rows(store)?;
    let mut applied = 0;
    let mut skipped = Vec::new();
    for (feed_id, action) in safe_repairs(&rows) {
        let status = rows
            .iter()
            .find(|r| r.feed_id == feed_id)
            .map_or(Status::Unchecked, GardenRow::status);
        match store.record_decision(feed_id, Decision::Repair, status, Some(&action), now) {
            Ok(()) => applied += 1,
            Err(crate::Error::Invalid(msg)) => skipped.push(format!("#{feed_id}: {msg}")),
            Err(other) => return Err(other),
        }
    }
    recompute_triage(store)?;
    Ok((applied, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cadence, Evidence};
    use crate::opml;
    use std::collections::HashMap;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn diag(status: Status, action: Option<Action>) -> Diagnosis {
        Diagnosis {
            status,
            reason: format!("{status:?}"),
            evidence: Evidence::default(),
            remarks: vec![],
            action,
            hijack_score: 0.0,
            cadence: Cadence::default(),
        }
    }

    /// Feeds: 1 Dead (Blogs), 2 Alive with https upgrade (Blogs), 3 Fossil (Infos), 4 Moved via 301 (Infos).
    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body>
            <outline text="Blogs"><outline text="Zed" xmlUrl="http://z.example.org/feed"/><outline text="alpha" xmlUrl="http://a.example.org/feed"/></outline>
            <outline text="Infos"><outline text="Old" xmlUrl="https://o.example.org/feed"/><outline text="Mover" xmlUrl="https://m.example.org/feed"/></outline>
            </body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        s.save_diagnoses(
            &HashMap::from([
                (1, diag(Status::Dead, Some(Action::Bury))),
                (
                    2,
                    diag(
                        Status::Alive,
                        Some(Action::UpgradeHttps {
                            url: "https://a.example.org/feed".into(),
                        }),
                    ),
                ),
                (3, diag(Status::Fossil, None)),
                (
                    4,
                    diag(
                        Status::Moved,
                        Some(Action::FollowRedirect {
                            url: "https://m2.example.org/feed".into(),
                        }),
                    ),
                ),
            ]),
            now(),
        )
        .unwrap();
        s
    }

    #[test]
    fn queue_rule() {
        assert!(needs_triage(Status::Dead, None));
        assert!(!needs_triage(Status::Alive, None));
        assert!(!needs_triage(Status::Fossil, Some(Status::Fossil)));
        assert!(
            needs_triage(Status::Dead, Some(Status::Fossil)),
            "worse than when decided"
        );
        assert!(
            !needs_triage(Status::Fossil, Some(Status::Dead)),
            "better than when decided"
        );
    }

    #[test]
    fn rows_are_sorted_by_urgency_then_folder_then_title() {
        let rows = load_rows(&store()).unwrap();
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Mover", "Zed", "Old", "alpha"]);
        assert_eq!(rows[0].top_folder, "Infos");
        assert_eq!(rows[0].folder_path, "Infos");
    }

    #[test]
    fn progress_counts_triage_statuses() {
        let s = store();
        assert_eq!(
            progress(&load_rows(&s).unwrap()),
            Progress {
                decided: 0,
                needing: 3
            }
        );
        decide(&s, 3, Decision::Keep, now()).unwrap();
        decide(&s, 1, Decision::Bury, now()).unwrap();
        let rows = load_rows(&s).unwrap();
        assert_eq!(
            progress(&rows),
            Progress {
                decided: 2,
                needing: 3
            }
        );
        assert!(rows.iter().find(|r| r.feed_id == 1).unwrap().buried);
        undo(&s).unwrap();
        assert_eq!(
            progress(&load_rows(&s).unwrap()),
            Progress {
                decided: 1,
                needing: 3
            }
        );
    }

    #[test]
    fn safe_repairs_include_alive_https_upgrades_and_redirects() {
        let s = store();
        let repairs = safe_repairs(&load_rows(&s).unwrap());
        let ids: Vec<FeedId> = repairs.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![4, 2]);
        let (applied, skipped) = apply_safe_repairs(&s, now()).unwrap();
        assert_eq!((applied, skipped.len()), (2, 0));
        assert_eq!(s.feed(2).unwrap().xml_url, "https://a.example.org/feed");
        assert!(
            safe_repairs(&load_rows(&s).unwrap()).is_empty(),
            "decided feeds are not offered again"
        );
    }

    fn failed_attempt(feed_id: FeedId, at: DateTime<Utc>) -> crate::source::FetchAttempt {
        crate::source::FetchAttempt {
            feed_id,
            at,
            requested_url: "http://z.example.org/feed".into(),
            final_url: "https://z.example.org/feed".into(),
            http_status: Some(404),
            error: Some(crate::model::ErrorKind::Http(404)),
            redirect_chain: vec![crate::model::Hop {
                status: 301,
                url: "https://z.example.org/feed".into(),
            }],
            content_type: None,
            bytes: 0,
            duration_ms: 5,
            not_modified: false,
            etag: None,
            last_modified: None,
            feed: None,
        }
    }

    #[test]
    fn rows_carry_the_last_five_attempts_newest_first() {
        let s = store();
        for i in 0..7 {
            s.record_attempt("r", &failed_attempt(1, now() + chrono::Duration::hours(i)))
                .unwrap();
        }
        let rows = load_rows(&s).unwrap();
        let zed = rows.iter().find(|r| r.feed_id == 1).unwrap();
        assert_eq!(zed.attempts.len(), 5);
        assert_eq!(zed.attempts[0].at, now() + chrono::Duration::hours(6));
        assert_eq!(zed.attempts[0].redirect_chain[0].status, 301);
        assert!(
            rows.iter()
                .find(|r| r.feed_id == 2)
                .unwrap()
                .attempts
                .is_empty()
        );
    }

    #[test]
    fn a_repaired_feed_comes_back_on_any_problem_found_after_the_repair() {
        let s = store();
        decide(&s, 4, Decision::Repair, now()).unwrap();
        let row = |s: &Store| {
            load_rows(s)
                .unwrap()
                .into_iter()
                .find(|r| r.feed_id == 4)
                .unwrap()
        };
        assert!(
            !row(&s).needs_triage(),
            "the stale Moved diagnosis predates the repair"
        );
        s.record_attempt("r", &failed_attempt(4, now() + chrono::Duration::hours(1)))
            .unwrap();
        for status in [Status::Dead, Status::Fossil] {
            s.save_diagnoses(&HashMap::from([(4, diag(status, None))]), now())
                .unwrap();
            assert!(
                row(&s).needs_triage(),
                "{status:?} after a repair of a Moved feed"
            );
        }
        s.save_diagnoses(&HashMap::from([(4, diag(Status::Dormant, None))]), now())
            .unwrap();
        assert!(!row(&s).needs_triage());
    }

    #[test]
    fn repair_uses_the_proposed_action() {
        let s = store();
        decide(&s, 4, Decision::Repair, now()).unwrap();
        assert_eq!(s.feed(4).unwrap().xml_url, "https://m2.example.org/feed");
        assert!(
            decide(&s, 3, Decision::Repair, now()).is_err(),
            "Fossil has nothing to repair"
        );
    }

    /// `needs_triage` as other clients read it, by feed id.
    fn stored_flags(s: &Store) -> Vec<(FeedId, bool)> {
        s.conn()
            .prepare("SELECT feed_id, needs_triage FROM v_garden_row ORDER BY feed_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn assert_flags_match_rows(s: &Store) {
        let mut expected: Vec<(FeedId, bool)> = load_rows(s)
            .unwrap()
            .iter()
            .map(|r| (r.feed_id, r.needs_triage()))
            .collect();
        expected.sort();
        assert_eq!(stored_flags(s), expected);
    }

    #[test]
    fn triage_flags_follow_decisions_undo_and_safe_repairs() {
        let s = store();
        recompute_triage(&s).unwrap();
        assert_eq!(
            stored_flags(&s),
            [(1, true), (2, false), (3, true), (4, true)]
        );
        decide(&s, 3, Decision::Keep, now()).unwrap();
        decide(&s, 1, Decision::Bury, now()).unwrap();
        assert_eq!(
            stored_flags(&s),
            [(1, false), (2, false), (3, false), (4, true)]
        );
        assert_eq!(undo(&s).unwrap(), Some(1));
        assert_eq!(stored_flags(&s)[0], (1, true), "undo brings the feed back");
        apply_safe_repairs(&s, now()).unwrap();
        assert_eq!(stored_flags(&s)[3], (4, false), "repaired");
        assert_flags_match_rows(&s);
    }
}
