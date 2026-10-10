# Phase 8: Decisions with undo, garden queue logic, OPML export

Read the plan index first. Global Constraints apply. Spec §9 (queue rule, keys, export).

**Files (5):**
- Create: `crates/ressac-core/src/store/decisions.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (add `mod decisions; pub use decisions::{DecisionRecord, FeedSnapshot};`)
- Create: `crates/ressac-core/src/garden.rs` (tests inline)
- Modify: `crates/ressac-core/src/opml.rs` (add `export` + tests)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod garden;`)

### Task 8.1: Decisions with undo

**Interfaces:**
- Consumes: `model::{Action, Decision, FeedId, Status, ts, parse_ts}`, `url_key::url_key`, `error::Error::Invalid`.
- Produces:
  - `struct FeedSnapshot { xml_url: String, url_key: String, state: String, merged_into_id: Option<FeedId> }` (serde)
  - `struct DecisionRecord { id: i64, feed_id: FeedId, decision: Decision, status_at_decision: Status, action: Option<Action>, decided_at: DateTime<Utc> }`
  - `Store::record_decision(&self, feed_id, decision: Decision, status: Status, action: Option<&Action>, now) -> Result<()>`
  - `Store::undo_last_decision(&self) -> Result<Option<FeedId>>`
  - `Store::latest_decisions(&self) -> Result<HashMap<FeedId, DecisionRecord>>`

Effects of `record_decision`, all in one transaction, with a snapshot of the feed taken first:

| Decision | Effect |
|---|---|
| `Keep` | Nothing. |
| `Bury` | `state = 'buried'`. |
| `Repair` with `UpgradeHttps` / `FollowRedirect` / `AdoptDiscovered { url }` | `xml_url = url`, `url_key = url_key(url)`, `etag` and `last_modified` set to NULL. If another active feed already has that key: `Error::Invalid("Feed #N already uses <url>; merge into it instead.")` and nothing is written. |
| `Repair` with `MergeInto { feed_id: target }` | `state = 'buried'`, `merged_into_id = target`. |
| `Repair` with `Bury` or no action | `Error::Invalid("Nothing to repair for this feed.")`. |

- [ ] **Step 1: Write the failing tests** in `store/decisions.rs`

```rust
//! Garden decisions: applied to the feed with a snapshot so they can be undone.

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
        s.import_outlines(&opml::parse(xml).unwrap(), false, now()).unwrap();
        s
    }

    #[test]
    fn keep_records_without_changing_the_feed() {
        let s = store();
        let before = s.feed(1).unwrap();
        s.record_decision(1, Decision::Keep, Status::Fossil, None, now()).unwrap();
        assert_eq!(s.feed(1).unwrap(), before);
        let d = &s.latest_decisions().unwrap()[&1];
        assert_eq!((d.decision, d.status_at_decision), (Decision::Keep, Status::Fossil));
    }

    #[test]
    fn bury_then_undo_restores_the_feed() {
        let s = store();
        s.record_decision(1, Decision::Bury, Status::Dead, None, now()).unwrap();
        assert_eq!(s.feed(1).unwrap().state, "buried");
        assert_eq!(s.undo_last_decision().unwrap(), Some(1));
        assert_eq!(s.feed(1).unwrap().state, "active");
        assert!(s.latest_decisions().unwrap().is_empty());
        assert_eq!(s.undo_last_decision().unwrap(), None);
    }

    #[test]
    fn repair_changes_the_address_and_undo_reverts_it() {
        let s = store();
        let action = Action::UpgradeHttps { url: "https://a.example.org/feed".into() };
        s.record_decision(1, Decision::Repair, Status::Alive, Some(&action), now()).unwrap();
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
        s.record_decision(1, Decision::Repair, Status::Duplicate, Some(&Action::MergeInto { feed_id: 2 }), now())
            .unwrap();
        let f = s.feed(1).unwrap();
        assert_eq!((f.state.as_str(), f.merged_into_id), ("buried", Some(2)));
    }

    #[test]
    fn repair_onto_existing_feed_fails_cleanly() {
        let s = store();
        let action = Action::FollowRedirect { url: "https://www.b.example.org/feed/".into() };
        let err = s.record_decision(1, Decision::Repair, Status::Moved, Some(&action), now()).unwrap_err();
        assert!(err.to_string().contains("Feed #2 already uses"), "{err}");
        assert_eq!(s.feed(1).unwrap().xml_url, "http://a.example.org/feed");
        assert!(s.latest_decisions().unwrap().is_empty());
    }

    #[test]
    fn repair_without_a_repair_action_is_refused() {
        let s = store();
        assert!(s.record_decision(1, Decision::Repair, Status::Dead, Some(&Action::Bury), now()).is_err());
        assert!(s.record_decision(1, Decision::Repair, Status::Dead, None, now()).is_err());
    }
}
```

- [ ] **Step 2:** Wire `mod decisions;` + re-exports in `store/mod.rs`. Run `cargo test -p ressac-core store::decisions`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
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
            |r| Ok(FeedSnapshot { xml_url: r.get(0)?, url_key: r.get(1)?, state: r.get(2)?, merged_into_id: r.get(3)? }),
        )?;
        match (decision, action) {
            (Decision::Keep, _) => {}
            (Decision::Bury, _) => {
                tx.execute("UPDATE feed SET state = 'buried' WHERE id = ?1", [feed_id])?;
            }
            (
                Decision::Repair,
                Some(Action::UpgradeHttps { url } | Action::FollowRedirect { url } | Action::AdoptDiscovered { url }),
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
                    return Err(Error::Invalid(format!("Feed #{other} already uses {url}; merge into it instead.")));
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
            (Decision::Repair, _) => return Err(Error::Invalid("Nothing to repair for this feed.".into())),
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
        let Some((id, feed_id, previous)) = last else { return Ok(None) };
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
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core store::decisions` → `6 passed`.

- [ ] **Step 5: Commit** ("Garden decisions with snapshot-based undo", both attribution lines).

### Task 8.2: Garden queue logic

**Interfaces:**
- Consumes: `Store::{feeds, folder_paths, diagnoses, diagnosis, latest_decisions, entries, record_decision, undo_last_decision}`, `model::*`.
- Produces (`ressac_core::garden`):
  - `struct GardenRow { feed_id, title, folder_path: String, top_folder: String, xml_url: String, html_url: Option<String>, wayback_url: Option<String>, buried: bool, diagnosis: Option<Diagnosis>, decision: Option<DecisionRecord>, recent: Vec<(DateTime<Utc>, String)> }`. Methods: `status()`, `action()`, `needs_triage()`.
  - `needs_triage(status: Status, decided_at: Option<Status>) -> bool`
  - `load_rows(store: &Store) -> Result<Vec<GardenRow>>`, sorted by urgency, then top folder, then title (case-insensitive)
  - `struct Progress { decided: usize, needing: usize }` and `progress(rows: &[GardenRow]) -> Progress`
  - `safe_repairs(rows: &[GardenRow]) -> Vec<(FeedId, Action)>`
  - `decide(store, feed_id, decision, now) -> Result<()>`
  - `undo(store) -> Result<Option<FeedId>>`
  - `apply_safe_repairs(store, now) -> Result<(usize, Vec<String>)>`, returning (applied, skipped messages)

- [ ] **Step 1: Write the failing tests** in `garden.rs`

```rust
//! Garden logic, independent of any UI: rows, the triage queue, decisions.

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
        s.import_outlines(&opml::parse(xml).unwrap(), false, now()).unwrap();
        s.save_diagnoses(
            &HashMap::from([
                (1, diag(Status::Dead, Some(Action::Bury))),
                (2, diag(Status::Alive, Some(Action::UpgradeHttps { url: "https://a.example.org/feed".into() }))),
                (3, diag(Status::Fossil, None)),
                (4, diag(Status::Moved, Some(Action::FollowRedirect { url: "https://m2.example.org/feed".into() }))),
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
        assert!(needs_triage(Status::Dead, Some(Status::Fossil)), "worse than when decided");
        assert!(!needs_triage(Status::Fossil, Some(Status::Dead)), "better than when decided");
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
        assert_eq!(progress(&load_rows(&s).unwrap()), Progress { decided: 0, needing: 3 });
        decide(&s, 3, Decision::Keep, now()).unwrap();
        decide(&s, 1, Decision::Bury, now()).unwrap();
        let rows = load_rows(&s).unwrap();
        assert_eq!(progress(&rows), Progress { decided: 2, needing: 3 });
        assert!(rows.iter().find(|r| r.feed_id == 1).unwrap().buried);
        undo(&s).unwrap();
        assert_eq!(progress(&load_rows(&s).unwrap()), Progress { decided: 1, needing: 3 });
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
        assert!(safe_repairs(&load_rows(&s).unwrap()).is_empty(), "decided feeds are not offered again");
    }

    #[test]
    fn repair_uses_the_proposed_action() {
        let s = store();
        decide(&s, 4, Decision::Repair, now()).unwrap();
        assert_eq!(s.feed(4).unwrap().xml_url, "https://m2.example.org/feed");
        assert!(decide(&s, 3, Decision::Repair, now()).is_err(), "Fossil has nothing to repair");
    }
}
```

- [ ] **Step 2:** Add `pub mod garden;` to `lib.rs`; run `cargo test -p ressac-core garden`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests:

```rust
use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::model::{Action, Decision, Diagnosis, FeedId, Status};
use crate::store::{DecisionRecord, Store};

/// Entries shown in the detail pane.
const RECENT_ENTRIES: usize = 3;

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
}

impl GardenRow {
    pub fn status(&self) -> Status {
        self.diagnosis.as_ref().map_or(Status::Unchecked, |d| d.status)
    }

    pub fn action(&self) -> Option<&Action> {
        self.diagnosis.as_ref().and_then(|d| d.action.as_ref())
    }

    pub fn needs_triage(&self) -> bool {
        !self.buried && needs_triage(self.status(), self.decision.as_ref().map(|d| d.status_at_decision))
    }
}

/// A feed needs triage when its status is a problem and it is undecided, or it got
/// strictly more urgent than when it was decided (spec §9).
pub fn needs_triage(status: Status, decided_at: Option<Status>) -> bool {
    status.needs_triage() && decided_at.is_none_or(|prev| status.urgency() < prev.urgency())
}

pub fn load_rows(store: &Store) -> Result<Vec<GardenRow>> {
    let paths = store.folder_paths()?;
    let mut diags = store.diagnoses()?;
    let mut decisions = store.latest_decisions()?;
    let mut rows = Vec::new();
    for f in store.feeds()? {
        let folder_path = f.folder_id.and_then(|id| paths.get(&id).cloned()).unwrap_or_default();
        let top_folder = folder_path.split(" / ").next().unwrap_or_default().to_string();
        let recent = store
            .entries(f.id, RECENT_ENTRIES)?
            .into_iter()
            .map(|e| (e.published_at, e.title.unwrap_or_else(|| "(untitled)".into())))
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
        });
    }
    rows.sort_by(|a, b| {
        (a.status().urgency(), &a.top_folder, a.title.to_lowercase())
            .cmp(&(b.status().urgency(), &b.top_folder, b.title.to_lowercase()))
    });
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub decided: usize,
    pub needing: usize,
}

pub fn progress(rows: &[GardenRow]) -> Progress {
    let triage: Vec<&GardenRow> = rows.iter().filter(|r| r.status().needs_triage()).collect();
    Progress { decided: triage.iter().filter(|r| !r.needs_triage()).count(), needing: triage.len() }
}

/// Undecided active feeds whose proposed action is safe, in row order.
pub fn safe_repairs(rows: &[GardenRow]) -> Vec<(FeedId, Action)> {
    rows.iter()
        .filter(|r| !r.buried && r.decision.is_none())
        .filter_map(|r| r.action().filter(|a| a.is_safe()).map(|a| (r.feed_id, a.clone())))
        .collect()
}

pub fn decide(store: &Store, feed_id: FeedId, decision: Decision, now: DateTime<Utc>) -> Result<()> {
    let diagnosis = store.diagnosis(feed_id)?;
    let status = diagnosis.as_ref().map_or(Status::Unchecked, |d| d.status);
    let action = match decision {
        Decision::Repair => diagnosis.and_then(|d| d.action),
        Decision::Keep | Decision::Bury => None,
    };
    store.record_decision(feed_id, decision, status, action.as_ref(), now)
}

pub fn undo(store: &Store) -> Result<Option<FeedId>> {
    store.undo_last_decision()
}

pub fn apply_safe_repairs(store: &Store, now: DateTime<Utc>) -> Result<(usize, Vec<String>)> {
    let rows = load_rows(store)?;
    let mut applied = 0;
    let mut skipped = Vec::new();
    for (feed_id, action) in safe_repairs(&rows) {
        let status = rows.iter().find(|r| r.feed_id == feed_id).map_or(Status::Unchecked, GardenRow::status);
        match store.record_decision(feed_id, Decision::Repair, status, Some(&action), now) {
            Ok(()) => applied += 1,
            Err(crate::Error::Invalid(msg)) => skipped.push(format!("#{feed_id}: {msg}")),
            Err(other) => return Err(other),
        }
    }
    Ok((applied, skipped))
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core garden` → `5 passed`.

- [ ] **Step 5: Commit** ("Garden queue logic: rows, triage rule, progress, safe repairs", both attribution lines).

### Task 8.3: OPML export

**Interfaces:**
- Consumes: `Store::{folders, feeds}`, `opml::{parse, clean_description}`, `health::hijack::has_spam`.
- Produces: `opml::export(store: &Store, cemetery: bool) -> Result<String>`.
  - Writes OPML 2.0 with the folder tree, in import order.
  - Active feeds only, or buried feeds only with `cemetery`.
  - Empty folders are skipped.

Attribute rules:
- Order: `text`, `type`, `xmlUrl`, `htmlUrl`, `description`, then the remaining original attributes alphabetically.
- `xmlUrl` is the current URL.
- `description` is cleaned with `clean_description`, and omitted when empty or when `has_spam` matches.
- With `cemetery`: add `ressac:wayback` when known, and declare `xmlns:ressac="https://github.com/w3cdotorg/ressac"` on `<opml>`.

- [ ] **Step 1: Append the failing tests** to the `tests` module of `opml.rs`

```rust
    mod export_tests {
        use super::super::*;
        use crate::model::{Decision, Status};
        use crate::store::Store;

        const FIXTURE: &str = include_str!("../../../tests/fixtures/vienna-like.opml");

        fn imported() -> Store {
            let s = Store::open_in_memory().unwrap();
            s.import_outlines(&parse(FIXTURE).unwrap(), false, "2026-10-10T08:00:00Z".parse().unwrap()).unwrap();
            s
        }

        fn all_feeds(items: &[Outline]) -> Vec<OpmlFeed> {
            items
                .iter()
                .flat_map(|o| match o {
                    Outline::Feed(f) => vec![f.clone()],
                    Outline::Folder { children, .. } => all_feeds(children),
                })
                .collect()
        }

        #[test]
        fn export_round_trips_the_tree_and_cleans_attributes() {
            let xml = export(&imported(), false).unwrap();
            assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">"));
            let items = parse(&xml).unwrap();
            let feeds = all_feeds(&items);
            assert_eq!(feeds.len(), 11);
            let Outline::Folder { name, children } = &items[0] else { panic!() };
            assert_eq!(name, "Blogs");
            assert!(matches!(&children[1], Outline::Folder { name, .. } if name == "potes"));
            let flickr = feeds.iter().find(|f| f.title.starts_with("Photos")).unwrap();
            assert!(!flickr.attrs["xmlUrl"].contains("&amp;"));
            assert_eq!(feeds.iter().find(|f| f.title == "Bibliobsession 2.0").unwrap().description, None);
            assert_eq!(
                feeds.iter().find(|f| f.title == "Girl on the Couch").unwrap().description.as_deref(),
                Some("A book is a loaded gun in the house next door")
            );
        }

        #[test]
        fn buried_feeds_go_to_the_cemetery_only() {
            let s = imported();
            let girl = s.feeds().unwrap().into_iter().find(|f| f.title == "Girl on the Couch").unwrap();
            s.record_decision(girl.id, Decision::Bury, Status::Dead, None, "2026-10-10T08:00:00Z".parse().unwrap())
                .unwrap();
            s.set_wayback(girl.id, "http://web.archive.org/web/2012/girl").unwrap();
            assert_eq!(all_feeds(&parse(&export(&s, false).unwrap()).unwrap()).len(), 10);
            let cemetery = export(&s, true).unwrap();
            assert!(cemetery.contains("xmlns:ressac=\"https://github.com/w3cdotorg/ressac\""));
            let buried = all_feeds(&parse(&cemetery).unwrap());
            assert_eq!(buried.len(), 1);
            assert_eq!(buried[0].attrs["ressac:wayback"], "http://web.archive.org/web/2012/girl");
        }
    }
```

- [ ] **Step 2: Run** `cargo test -p ressac-core opml`. Expected: compile errors (`export` missing).

- [ ] **Step 3: Implement.** Add to `opml.rs` (above the tests; extend the `use` list with `crate::store::{FeedRow, FolderRow, Store}`, `crate::health::hijack::has_spam`, `crate::model::FolderId`, `quick_xml::escape::escape`):

```rust
const RESSAC_NS: &str = "https://github.com/w3cdotorg/ressac";
const LEADING_ATTRS: [&str; 5] = ["text", "type", "xmlUrl", "htmlUrl", "description"];

/// OPML 2.0 export of active feeds, or of the cemetery (buried feeds) with Wayback links.
pub fn export(store: &Store, cemetery: bool) -> Result<String> {
    let folders = store.folders()?;
    let feeds: Vec<FeedRow> = store
        .feeds()?
        .into_iter()
        .filter(|f| if cemetery { f.state == "buried" } else { f.state == "active" })
        .collect();
    let title = if cemetery { "Ressac cemetery" } else { "Ressac subscriptions" };
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    if cemetery {
        out.push_str(&format!("<opml version=\"2.0\" xmlns:ressac=\"{RESSAC_NS}\">\n"));
    } else {
        out.push_str("<opml version=\"2.0\">\n");
    }
    out.push_str(&format!(
        "  <head>\n    <title>{title}</title>\n    <dateCreated>{}</dateCreated>\n  </head>\n  <body>\n",
        chrono::Utc::now().to_rfc2822()
    ));
    write_level(&mut out, None, 2, &folders, &feeds, cemetery);
    out.push_str("  </body>\n</opml>\n");
    Ok(out)
}

enum Node<'a> {
    Folder(&'a FolderRow),
    Feed(&'a FeedRow),
}

fn write_level(out: &mut String, parent: Option<FolderId>, depth: usize, folders: &[FolderRow], feeds: &[FeedRow], cemetery: bool) {
    let mut nodes: Vec<(i64, Node)> = folders
        .iter()
        .filter(|f| f.parent_id == parent)
        .map(|f| (f.position, Node::Folder(f)))
        .chain(feeds.iter().filter(|f| f.folder_id == parent).map(|f| (f.position, Node::Feed(f))))
        .collect();
    nodes.sort_by_key(|(position, _)| *position);
    let indent = "  ".repeat(depth);
    for (_, node) in nodes {
        match node {
            Node::Folder(folder) if has_feeds(folder.id, folders, feeds) => {
                out.push_str(&format!("{indent}<outline text=\"{}\">\n", escape(folder.name.as_str())));
                write_level(out, Some(folder.id), depth + 1, folders, feeds, cemetery);
                out.push_str(&format!("{indent}</outline>\n"));
            }
            Node::Folder(_) => {}
            Node::Feed(feed) => out.push_str(&format!("{indent}{}\n", feed_outline(feed, cemetery))),
        }
    }
}

fn has_feeds(folder: FolderId, folders: &[FolderRow], feeds: &[FeedRow]) -> bool {
    feeds.iter().any(|f| f.folder_id == Some(folder))
        || folders.iter().filter(|c| c.parent_id == Some(folder)).any(|c| has_feeds(c.id, folders, feeds))
}

fn feed_outline(feed: &FeedRow, cemetery: bool) -> String {
    let mut attrs: BTreeMap<String, String> = serde_json::from_str(&feed.opml_attrs).unwrap_or_default();
    attrs.insert("text".into(), feed.title.clone());
    attrs.entry("type".into()).or_insert_with(|| "rss".into());
    attrs.insert("xmlUrl".into(), feed.xml_url.clone());
    match &feed.html_url {
        Some(u) => attrs.insert("htmlUrl".into(), u.clone()),
        None => attrs.remove("htmlUrl"),
    };
    let description = attrs
        .get("description")
        .cloned()
        .or_else(|| feed.description_original.clone())
        .and_then(|d| clean_description(&d))
        .filter(|d| !has_spam(d));
    match description {
        Some(d) => attrs.insert("description".into(), d),
        None => attrs.remove("description"),
    };
    if cemetery && let Some(w) = &feed.wayback_url {
        attrs.insert("ressac:wayback".into(), w.clone());
    }
    let mut parts = Vec::new();
    for key in LEADING_ATTRS {
        if let Some(v) = attrs.remove(key) {
            parts.push(format!("{key}=\"{}\"", escape(v.as_str())));
        }
    }
    parts.extend(attrs.iter().map(|(k, v)| format!("{k}=\"{}\"", escape(v.as_str()))));
    format!("<outline {}/>", parts.join(" "))
}
```

Note: `parse` treats an outline with `xmlUrl` as a feed, so a `title` attribute kept from the original is harmless. The original `xmlUrl` (with `&amp;`) is replaced by the current URL, so `&amp;amp;` never reappears.

- [ ] **Step 4: Run.** `cargo test -p ressac-core opml` → `7 passed`.

- [ ] **Step 5: Verification gate**, then commit and push ("OPML export with cleaned attributes and cemetery", both attribution lines).
