//! Diagnoses (`feed_health`) and probe caches on `feed`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Row, params};

use super::Store;
use crate::error::Result;
use crate::model::{Cadence, Diagnosis, FeedId, Status, parse_ts, ts};

const HEALTH_COLUMNS: &str = "feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
     last_entry_at, median_interval_days, entries_per_month";

fn json_error(col: usize, e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
}

fn health_row(r: &Row<'_>) -> rusqlite::Result<(FeedId, Diagnosis)> {
    let status: String = r.get(1)?;
    let evidence: String = r.get(3)?;
    let remarks: String = r.get(4)?;
    let action: Option<String> = r.get(5)?;
    let last_entry: Option<String> = r.get(7)?;
    Ok((
        r.get(0)?,
        Diagnosis {
            status: Status::parse(&status).unwrap_or(Status::Unchecked),
            reason: r.get(2)?,
            evidence: serde_json::from_str(&evidence).map_err(|e| json_error(3, e))?,
            remarks: serde_json::from_str(&remarks).map_err(|e| json_error(4, e))?,
            action: action
                .map(|a| serde_json::from_str(&a))
                .transpose()
                .map_err(|e| json_error(5, e))?,
            hijack_score: r.get::<_, f64>(6)? as f32,
            cadence: Cadence {
                last_entry_at: last_entry.as_deref().and_then(parse_ts),
                median_interval_days: r.get(8)?,
                entries_per_month: r.get(9)?,
            },
        },
    ))
}

impl Store {
    pub fn save_diagnoses(
        &self,
        diags: &HashMap<FeedId, Diagnosis>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
                 last_entry_at, median_interval_days, entries_per_month, computed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT(feed_id) DO UPDATE SET status = excluded.status, reason = excluded.reason, \
                 evidence = excluded.evidence, remarks = excluded.remarks, proposed_action = excluded.proposed_action, \
                 hijack_score = excluded.hijack_score, last_entry_at = excluded.last_entry_at, \
                 median_interval_days = excluded.median_interval_days, entries_per_month = excluded.entries_per_month, \
                 computed_at = excluded.computed_at",
            )?;
            for (feed_id, d) in diags {
                stmt.execute(params![
                    feed_id,
                    d.status.as_str(),
                    d.reason,
                    serde_json::to_string(&d.evidence)?,
                    serde_json::to_string(&d.remarks)?,
                    d.action.as_ref().map(serde_json::to_string).transpose()?,
                    f64::from(d.hijack_score),
                    d.cadence.last_entry_at.map(ts),
                    d.cadence.median_interval_days,
                    d.cadence.entries_per_month,
                    ts(now)
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn diagnosis(&self, feed_id: FeedId) -> Result<Option<Diagnosis>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {HEALTH_COLUMNS} FROM feed_health WHERE feed_id = ?1"
        ))?;
        let mut rows = stmt.query_map([feed_id], health_row)?;
        Ok(rows.next().transpose()?.map(|(_, d)| d))
    }

    pub fn diagnoses(&self) -> Result<HashMap<FeedId, Diagnosis>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {HEALTH_COLUMNS} FROM feed_health"))?;
        let rows = stmt
            .query_map([], health_row)?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }

    pub fn set_https_available(&self, feed_id: FeedId, available: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET https_available = ?2 WHERE id = ?1",
            params![feed_id, available],
        )?;
        Ok(())
    }

    pub fn set_discovery(
        &self,
        feed_id: FeedId,
        url: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET discovered_url = ?2, discovery_checked_at = ?3 WHERE id = ?1",
            params![feed_id, url, ts(now)],
        )?;
        Ok(())
    }

    pub fn set_wayback(&self, feed_id: FeedId, url: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET wayback_url = ?2 WHERE id = ?1",
            params![feed_id, url],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Cadence, Evidence, Remark, Status};
    use crate::opml;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body><outline text="A" xmlUrl="http://a.example.org/feed"/><outline text="B" xmlUrl="https://b.example.org/feed"/></body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        s
    }

    fn sample() -> Diagnosis {
        Diagnosis {
            status: Status::Dead,
            reason: "The server says the feed is gone (HTTP 410).".into(),
            evidence: Evidence {
                bullets: vec!["b1".into()],
                clue: "410".into(),
                language: None,
            },
            remarks: vec![Remark::HttpOnly, Remark::SimilarTitle { feed_id: 2 }],
            action: Some(Action::Bury),
            hijack_score: 0.3,
            cadence: Cadence {
                last_entry_at: Some(now()),
                median_interval_days: Some(7.0),
                entries_per_month: 1.5,
            },
        }
    }

    #[test]
    fn diagnosis_round_trips_and_upserts() {
        let s = store();
        s.save_diagnoses(&HashMap::from([(1, sample())]), now())
            .unwrap();
        assert_eq!(s.diagnosis(1).unwrap(), Some(sample()));
        let mut changed = sample();
        changed.status = Status::Alive;
        changed.action = None;
        s.save_diagnoses(&HashMap::from([(1, changed.clone())]), now())
            .unwrap();
        assert_eq!(s.diagnosis(1).unwrap(), Some(changed));
        assert_eq!(s.diagnosis(2).unwrap(), None);
        assert_eq!(s.diagnoses().unwrap().len(), 1);
    }

    #[test]
    fn probe_caches_are_stored_on_the_feed() {
        let s = store();
        s.set_https_available(1, true).unwrap();
        s.set_discovery(2, Some("https://b.example.org/new.xml"), now())
            .unwrap();
        s.set_wayback(2, "http://web.archive.org/web/2019/x")
            .unwrap();
        assert_eq!(s.feed(1).unwrap().https_available, Some(true));
        let b = s.feed(2).unwrap();
        assert_eq!(
            b.discovered_url.as_deref(),
            Some("https://b.example.org/new.xml")
        );
        assert_eq!(
            b.discovery_checked_at.as_deref(),
            Some("2026-10-10T08:00:00.000Z")
        );
        assert_eq!(
            b.wayback_url.as_deref(),
            Some("http://web.archive.org/web/2019/x")
        );
    }
}
