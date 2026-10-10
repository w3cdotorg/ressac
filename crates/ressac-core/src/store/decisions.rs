//! Garden decisions: applied to the feed with a snapshot so they can be undone.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::{Error, Result};
use crate::model::{Action, Decision, FeedId, Status, parse_ts, ts};
use crate::url_key::url_key;

/// The feed fields a decision may change, saved for undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedSnapshot {
    pub xml_url: String,
    pub url_key: String,
    pub state: String,
    pub merged_into_id: Option<FeedId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRecord {
    pub id: i64,
    pub feed_id: FeedId,
    pub decision: Decision,
    pub status_at_decision: Status,
    pub action: Option<Action>,
    pub decided_at: DateTime<Utc>,
}

impl Store {
    pub fn record_decision(
        &self,
        feed_id: FeedId,
        decision: Decision,
        status: Status,
        action: Option<&Action>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        let before = tx.query_row(
            "SELECT xml_url, url_key, state, merged_into_id FROM feed WHERE id = ?1",
            [feed_id],
            |r| {
                Ok(FeedSnapshot {
                    xml_url: r.get(0)?,
                    url_key: r.get(1)?,
                    state: r.get(2)?,
                    merged_into_id: r.get(3)?,
                })
            },
        )?;
        match (decision, action) {
            (Decision::Keep, _) => {}
            (Decision::Bury, _) => {
                tx.execute("UPDATE feed SET state = 'buried' WHERE id = ?1", [feed_id])?;
            }
            (
                Decision::Repair,
                Some(
                    Action::UpgradeHttps { url }
                    | Action::FollowRedirect { url }
                    | Action::AdoptDiscovered { url },
                ),
            ) => {
                let key = url_key(url);
                let clash: Option<FeedId> = tx
                    .query_row(
                        "SELECT id FROM feed WHERE url_key = ?1 AND state = 'active' AND id <> ?2",
                        params![key, feed_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(other) = clash {
                    return Err(Error::Invalid(format!(
                        "Feed #{other} already uses {url}; merge into it instead."
                    )));
                }
                tx.execute(
                    "UPDATE feed SET xml_url = ?2, url_key = ?3, etag = NULL, last_modified = NULL WHERE id = ?1",
                    params![feed_id, url, key],
                )?;
            }
            (Decision::Repair, Some(Action::MergeInto { feed_id: target })) => {
                tx.execute(
                    "UPDATE feed SET state = 'buried', merged_into_id = ?2 WHERE id = ?1",
                    params![feed_id, target],
                )?;
            }
            (Decision::Repair, _) => {
                return Err(Error::Invalid("Nothing to repair for this feed.".into()));
            }
        }
        tx.execute(
            "INSERT INTO garden_decision (feed_id, decision, status_at_decision, action, previous, decided_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                feed_id,
                decision.as_str(),
                status.as_str(),
                action.map(serde_json::to_string).transpose()?,
                serde_json::to_string(&before)?,
                ts(now)
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Reverts the most recent decision (any feed) and deletes it.
    pub fn undo_last_decision(&self) -> Result<Option<FeedId>> {
        let tx = self.conn().unchecked_transaction()?;
        let last: Option<(i64, FeedId, String)> = tx
            .query_row(
                "SELECT id, feed_id, previous FROM garden_decision ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((id, feed_id, previous)) = last else {
            return Ok(None);
        };
        let snap: FeedSnapshot = serde_json::from_str(&previous)?;
        tx.execute(
            "UPDATE feed SET xml_url = ?2, url_key = ?3, state = ?4, merged_into_id = ?5 WHERE id = ?1",
            params![feed_id, snap.xml_url, snap.url_key, snap.state, snap.merged_into_id],
        )?;
        tx.execute("DELETE FROM garden_decision WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(Some(feed_id))
    }

    pub fn latest_decisions(&self) -> Result<HashMap<FeedId, DecisionRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, feed_id, decision, status_at_decision, action, decided_at FROM garden_decision \
             WHERE id IN (SELECT max(id) FROM garden_decision GROUP BY feed_id)",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let decision: String = r.get(2)?;
                let status: String = r.get(3)?;
                let action: Option<String> = r.get(4)?;
                let decided_at: String = r.get(5)?;
                Ok(DecisionRecord {
                    id: r.get(0)?,
                    feed_id: r.get(1)?,
                    decision: Decision::parse(&decision).unwrap_or(Decision::Keep),
                    status_at_decision: Status::parse(&status).unwrap_or(Status::Unchecked),
                    action: action.and_then(|a| serde_json::from_str(&a).ok()),
                    decided_at: parse_ts(&decided_at).unwrap_or_default(),
                })
            })?
            .map(|r| r.map(|d| (d.feed_id, d)))
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opml;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body>
            <outline text="A" xmlUrl="http://a.example.org/feed"/>
            <outline text="B" xmlUrl="https://b.example.org/feed"/>
            </body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        s
    }

    #[test]
    fn keep_records_without_changing_the_feed() {
        let s = store();
        let before = s.feed(1).unwrap();
        s.record_decision(1, Decision::Keep, Status::Fossil, None, now())
            .unwrap();
        assert_eq!(s.feed(1).unwrap(), before);
        let d = &s.latest_decisions().unwrap()[&1];
        assert_eq!(
            (d.decision, d.status_at_decision),
            (Decision::Keep, Status::Fossil)
        );
    }

    #[test]
    fn bury_then_undo_restores_the_feed() {
        let s = store();
        s.record_decision(1, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        assert_eq!(s.feed(1).unwrap().state, "buried");
        assert_eq!(s.undo_last_decision().unwrap(), Some(1));
        assert_eq!(s.feed(1).unwrap().state, "active");
        assert!(s.latest_decisions().unwrap().is_empty());
        assert_eq!(s.undo_last_decision().unwrap(), None);
    }

    #[test]
    fn repair_changes_the_address_and_undo_reverts_it() {
        let s = store();
        let action = Action::UpgradeHttps {
            url: "https://a.example.org/feed".into(),
        };
        s.record_decision(1, Decision::Repair, Status::Alive, Some(&action), now())
            .unwrap();
        let f = s.feed(1).unwrap();
        assert_eq!(f.xml_url, "https://a.example.org/feed");
        assert_eq!(f.etag, None);
        assert_eq!(s.latest_decisions().unwrap()[&1].action, Some(action));
        s.undo_last_decision().unwrap();
        assert_eq!(s.feed(1).unwrap().xml_url, "http://a.example.org/feed");
    }

    #[test]
    fn merge_buries_into_the_target() {
        let s = store();
        s.record_decision(
            1,
            Decision::Repair,
            Status::Duplicate,
            Some(&Action::MergeInto { feed_id: 2 }),
            now(),
        )
        .unwrap();
        let f = s.feed(1).unwrap();
        assert_eq!((f.state.as_str(), f.merged_into_id), ("buried", Some(2)));
    }

    #[test]
    fn repair_onto_existing_feed_fails_cleanly() {
        let s = store();
        let action = Action::FollowRedirect {
            url: "https://www.b.example.org/feed/".into(),
        };
        let err = s
            .record_decision(1, Decision::Repair, Status::Moved, Some(&action), now())
            .unwrap_err();
        assert!(err.to_string().contains("Feed #2 already uses"), "{err}");
        assert_eq!(s.feed(1).unwrap().xml_url, "http://a.example.org/feed");
        assert!(s.latest_decisions().unwrap().is_empty());
    }

    #[test]
    fn repair_without_a_repair_action_is_refused() {
        let s = store();
        assert!(
            s.record_decision(
                1,
                Decision::Repair,
                Status::Dead,
                Some(&Action::Bury),
                now()
            )
            .is_err()
        );
        assert!(
            s.record_decision(1, Decision::Repair, Status::Dead, None, now())
                .is_err()
        );
    }
}
