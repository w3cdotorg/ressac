# Phase 3: Persisted triage flags, subset refresh, re-diagnosis after decisions

Read the plan index first. Global Constraints apply. This phase implements spec §5 (`feed_health.needs_triage`: "Recomputed after every diagnosis, decision and undo") and the core half of §6 (`ressac fetch --feed … / --folder …`: subset, buried feeds skipped, diagnosis over all feeds). The CLI flags themselves come in Phase 5.

**Files (5):**
- Modify: `crates/ressac-core/src/store/health.rs` (`save_triage`, test)
- Modify: `crates/ressac-core/src/store/feeds.rs` (`feeds_in_folder`, test)
- Modify: `crates/ressac-core/src/garden.rs` (`recompute_triage`; `decide`, `undo`, `apply_safe_repairs` call it; test)
- Modify: `crates/ressac-core/src/service.rs` (`RefreshOptions.only`, `diagnose_all` recomputes; test)
- Modify: `crates/ressac/src/main.rs` (the two `RefreshOptions { force }` literals)

### Task 3.1: Store primitives: `save_triage` and `feeds_in_folder`

**Interfaces:**
- Consumes: schema v2 (`feed_health.needs_triage`, view `v_garden_row` with `COALESCE(h.needs_triage, 0) AS needs_triage`); `Store::{save_diagnoses, import_outlines, folders, feeds, record_decision}`.
- Produces (methods on `Store`):
  - `pub fn save_triage(&self, flags: &HashMap<FeedId, bool>) -> Result<()>`: one transaction of `UPDATE feed_health SET needs_triage = ? WHERE feed_id = ?`. A feed without a diagnosis has no `feed_health` row, is left alone, and reads 0 through `v_garden_row`. `save_diagnoses`' upsert does not touch the column, so a new diagnosis keeps the old flag until the next recompute.
  - `pub fn feeds_in_folder(&self, folder_id: FolderId) -> Result<Vec<FeedId>>`: active feeds of the folder and of every sub-folder, recursively, in id order; an unknown folder gives an empty list.

- [ ] **Step 1: Write the failing tests.**

Append to `mod tests` in `crates/ressac-core/src/store/health.rs`:

```rust
    fn triage_flags(s: &Store) -> Vec<(FeedId, bool)> {
        s.conn()
            .prepare("SELECT feed_id, needs_triage FROM v_garden_row ORDER BY feed_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn triage_flags_are_stored_and_survive_a_new_diagnosis() {
        let s = store();
        s.save_diagnoses(&HashMap::from([(1, sample())]), now())
            .unwrap();
        assert_eq!(triage_flags(&s), [(1, false), (2, false)]);
        s.save_triage(&HashMap::from([(1, true), (2, true)]))
            .unwrap();
        assert_eq!(
            triage_flags(&s),
            [(1, true), (2, false)],
            "feed 2 has no diagnosis row"
        );
        s.save_diagnoses(&HashMap::from([(1, sample())]), now())
            .unwrap();
        assert_eq!(triage_flags(&s)[0], (1, true));
        s.save_triage(&HashMap::from([(1, false)])).unwrap();
        assert_eq!(triage_flags(&s)[0], (1, false));
    }
```

Append to `mod tests` in `crates/ressac-core/src/store/feeds.rs`:

```rust
    #[test]
    fn feeds_in_folder_is_recursive_and_skips_buried_feeds() {
        let store = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body>
            <outline text="Top"><outline text="A" xmlUrl="https://a.example.org/feed"/>
              <outline text="Sub"><outline text="B" xmlUrl="https://b.example.org/feed"/>
                <outline text="C" xmlUrl="https://c.example.org/feed"/></outline></outline>
            <outline text="Other"><outline text="D" xmlUrl="https://d.example.org/feed"/></outline>
            </body></opml>"#;
        store
            .import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        let folder = |name: &str| {
            store
                .folders()
                .unwrap()
                .into_iter()
                .find(|f| f.name == name)
                .unwrap()
                .id
        };
        let feed = |title: &str| {
            store
                .feeds()
                .unwrap()
                .into_iter()
                .find(|f| f.title == title)
                .unwrap()
                .id
        };
        store
            .record_decision(feed("C"), Decision::Bury, Status::Dead, None, now())
            .unwrap();
        assert_eq!(
            store.feeds_in_folder(folder("Top")).unwrap(),
            [feed("A"), feed("B")]
        );
        assert_eq!(store.feeds_in_folder(folder("Sub")).unwrap(), [feed("B")]);
        assert_eq!(store.feeds_in_folder(folder("Other")).unwrap(), [feed("D")]);
        assert!(store.feeds_in_folder(999).unwrap().is_empty());
    }
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib store::`. Expected: compile errors, `no method named save_triage found for struct Store` and `no method named feeds_in_folder found`.

- [ ] **Step 3: Implement.**

In `crates/ressac-core/src/store/health.rs`, inside `impl Store`, before `pub fn diagnosis`:

```rust
    /// Stores the garden's triage verdicts in `feed_health.needs_triage`. A feed without
    /// a diagnosis has no row and reads as 0 through `v_garden_row`.
    pub fn save_triage(&self, flags: &HashMap<FeedId, bool>) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE feed_health SET needs_triage = ?2 WHERE feed_id = ?1")?;
            for (feed_id, flag) in flags {
                stmt.execute(params![feed_id, flag])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
```

In `crates/ressac-core/src/store/feeds.rs`, inside `impl Store`, before `pub fn folder_paths`:

```rust
    /// Active feeds of a folder and of its sub-folders, recursively, in id order. An
    /// unknown folder has none.
    pub fn feeds_in_folder(&self, folder_id: FolderId) -> Result<Vec<FeedId>> {
        let mut stmt = self.conn().prepare(
            "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION SELECT f.id FROM folder f JOIN sub ON f.parent_id = sub.id) \
             SELECT id FROM feed WHERE state = 'active' AND folder_id IN (SELECT id FROM sub) ORDER BY id",
        )?;
        let ids = stmt
            .query_map([folder_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib store::health` → `4 passed`; `cargo test -p ressac-core --lib store::feeds` → `8 passed`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/store/health.rs crates/ressac-core/src/store/feeds.rs
git commit -m "Store triage flags; list the active feeds of a folder tree"
```

### Task 3.2: `garden::recompute_triage`, called by every decision and undo

**Interfaces:**
- Consumes: `garden::{load_rows, GardenRow::needs_triage}` (the rule with the repair baseline, core spec §9); `Store::save_triage` (Task 3.1).
- Produces:
  - `pub fn recompute_triage(store: &Store) -> Result<()>` in `garden.rs`: the flag of every feed, active or buried (buried ones are always `false`), from `load_rows` and `GardenRow::needs_triage`;
  - `decide`, `undo` and `apply_safe_repairs` keep their signatures and call `recompute_triage` before returning `Ok`. A failed decision returns its error first and recomputes nothing (nothing changed).

- [ ] **Step 1: Write the failing test.** Append to `mod tests` in `crates/ressac-core/src/garden.rs` (it reuses the module's `store()`: 1 Dead, 2 Alive with an https upgrade, 3 Fossil, 4 Moved by a 301):

```rust
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
```

- [ ] **Step 2: Run it.** `cargo test -p ressac-core --lib garden`. Expected: compile error, `cannot find function recompute_triage in this scope`.

- [ ] **Step 3: Implement** in `crates/ressac-core/src/garden.rs`.

Add `use std::collections::HashMap;` (with a blank line) above `use chrono::{DateTime, Utc};`.

Insert after `pub fn load_rows` (before `pub struct Progress`):

```rust
/// Stores every feed's triage verdict (`feed_health.needs_triage`) so that other clients
/// read the rule instead of reimplementing it. Called after every diagnosis, decision and undo.
pub fn recompute_triage(store: &Store) -> Result<()> {
    let flags: HashMap<FeedId, bool> = load_rows(store)?
        .iter()
        .map(|r| (r.feed_id, r.needs_triage()))
        .collect();
    store.save_triage(&flags)
}
```

Replace the tail of `decide` and the whole of `undo`:

```rust
    store.record_decision(feed_id, decision, status, action.as_ref(), now)?;
    recompute_triage(store)
}

pub fn undo(store: &Store) -> Result<Option<FeedId>> {
    let undone = store.undo_last_decision()?;
    recompute_triage(store)?;
    Ok(undone)
}
```

In `apply_safe_repairs`, replace the final `Ok((applied, skipped))` with:

```rust
    recompute_triage(store)?;
    Ok((applied, skipped))
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib garden` → `8 passed`; `cargo test -p ressac` → green (the TUI goes through `garden::decide` and `garden::undo`; its snapshots do not change).

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/garden.rs
git commit -m "Recompute stored triage flags after every garden decision and undo"
```

### Task 3.3: Subset refresh and triage flags after every diagnosis

**Interfaces:**
- Consumes: `Store::feeds_to_fetch(now, force) -> Result<Vec<FeedRef>>` (active feeds only, so buried ones are never fetched); `garden::recompute_triage` (Task 3.2).
- Produces (`ressac_core::service`):
  - `#[derive(Debug, Clone, Default)] pub struct RefreshOptions { pub force: bool, pub only: Option<Vec<FeedId>> }` (it loses `Copy`);
  - `refresh` keeps only the due feeds whose id is in `only` when it is `Some`. `force` stays independent: the CLI (Phase 5) sets it for `--feed`/`--folder`. `Started { total }` counts the subset. Diagnosis, probes and the summary are unchanged and still cover every active feed;
  - `diagnose_all` calls `garden::recompute_triage(&self.store)` after `save_diagnoses`, so a refresh, a `check` and Phase 5's `decide`/`undo`/`apply-safe` re-diagnoses all leave fresh flags.

- [ ] **Step 1: Write the failing test.** Append to `mod tests` in `crates/ressac-core/src/service.rs` (the module already has `serve`, `RSS`, wiremock and `use super::*`):

```rust
    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<Event>>);

    impl EventSink for Recorder {
        fn emit(&self, event: &Event) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[tokio::test]
    async fn a_subset_refresh_fetches_only_the_named_active_feeds_and_diagnoses_all() {
        let server = MockServer::start().await;
        let base = server.uri();
        serve(&server, "/gone.xml", 410, "", "text/plain").await;
        serve(&server, "/other.xml", 200, RSS, "application/rss+xml").await;
        serve(&server, "/buried.xml", 200, RSS, "application/rss+xml").await;
        let limits = FetchLimits {
            per_host_gap: Duration::ZERO,
            timeout: Duration::from_secs(5),
            ..FetchLimits::default()
        };
        let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits, &base).unwrap();
        let opml = format!(
            r#"<opml><body><outline text="Gone" xmlUrl="{base}/gone.xml"/>
            <outline text="Other" xmlUrl="{base}/other.xml"/>
            <outline text="Buried" xmlUrl="{base}/buried.xml"/></body></opml>"#
        );
        app.import_opml(&opml, false).unwrap();
        garden::decide(app.store(), 3, crate::model::Decision::Bury, Utc::now()).unwrap();

        let recorder = Recorder::default();
        let opts = RefreshOptions {
            force: false,
            only: Some(vec![1, 3]),
        };
        app.refresh(opts, &recorder, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();

        let events = recorder.0.lock().unwrap().clone();
        assert!(matches!(events.first(), Some(Event::Started { total: 1 })));
        let done: Vec<FeedId> = events
            .iter()
            .filter_map(|e| match e {
                Event::FeedDone { feed_id, .. } => Some(*feed_id),
                _ => None,
            })
            .collect();
        assert_eq!(done, [1], "the buried feed is skipped even when named");
        assert!(app.store().attempts(2, 1).unwrap().is_empty());
        assert!(app.store().attempts(3, 1).unwrap().is_empty());
        assert_eq!(
            app.store().diagnosis(1).unwrap().map(|d| d.status),
            Some(Status::Dead)
        );
        assert_eq!(
            app.store().diagnosis(2).unwrap().map(|d| d.status),
            Some(Status::Unchecked),
            "diagnosis covers every active feed"
        );
        let flag: bool = app
            .store()
            .conn()
            .query_row(
                "SELECT needs_triage FROM v_garden_row WHERE feed_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(flag, "diagnose_all stores the triage flags");
    }
```

- [ ] **Step 2: Run it.** `cargo test -p ressac-core --lib service`. Expected: compile errors, `struct RefreshOptions has no field named only` and `use of undeclared crate or module garden`.

- [ ] **Step 3: Implement.**

In `crates/ressac-core/src/service.rs`, add `use crate::garden;` after `use crate::error::Result;`, and replace the options struct:

```rust
#[derive(Debug, Clone, Default)]
pub struct RefreshOptions {
    /// Ignore the 30-minute freshness rule.
    pub force: bool,
    /// Restrict the fetch to these feeds (buried ones are still skipped). Diagnosis
    /// always covers every feed, because duplicate detection is cross-feed.
    pub only: Option<Vec<FeedId>>,
}
```

Replace the doc comment of `refresh` and its `let due = …` line:

```rust
    /// Fetches due feeds, or the due feeds among `opts.only` (writing each attempt as it
    /// arrives), applies the network guard, probes https variants and autodiscovery, then
    /// diagnoses every active feed.
```

```rust
        let mut due = self.store.feeds_to_fetch(started, opts.force)?;
        if let Some(only) = &opts.only {
            due.retain(|f| only.contains(&f.id));
        }
```

In `diagnose_all`, change its doc comment to `/// Diagnoses every active feed, stores the results and the garden's triage flags.` and its last lines to:

```rust
        self.store.save_diagnoses(&diags, now)?;
        garden::recompute_triage(&self.store)?;
        self.summary()
```

In `crates/ressac/src/main.rs`, the two literals `RefreshOptions { force }` (in `Command::Fetch` and `Command::Check`) become:

```rust
RefreshOptions { force, only: None }
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib service` → `8 passed`; `cargo test --workspace` → green (the end-to-end tests use `RefreshOptions::default()`, which still compiles).

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/service.rs crates/ressac/src/main.rs
git commit -m "Refresh a subset of feeds; store triage flags after every diagnosis"
```

### Verification gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green (the core's unit tests: `211 passed; 0 failed; 1 ignored`).

### Notes for the executor

- **Verified on 2026-10-10** in a scratch copy of the repository, task by task, on top of Phases 1 and 2: each task's end state passes clippy with `-D warnings` and the workspace tests; the counts above come from that run.
- `RefreshOptions` loses `Copy` because of the `Vec`. Nothing in the workspace copies it: `main.rs` builds it inline and the end-to-end tests use `default()`. Phase 5 replaces the `main.rs` literals when it adds `--feed`/`--folder`.
- `only` filters after `feeds_to_fetch`, so it never revives a buried feed and never bypasses the freshness rule by itself. Phase 5 must pass `force: true` with a subset, as the plan index's CLI table says.
- `feeds_in_folder` returns an empty list for an unknown folder id. Phase 5 can tell "unknown" from "empty" with `Store::folders()` if it wants a usage error.
- `recompute_triage` costs one `load_rows`: two small queries per feed (about 190 feeds on the real database), a few milliseconds. `refresh` calls `diagnose_all` twice, so it recomputes twice; the TUI now also recomputes on each decision before its own reload.
- `UNION` (not `UNION ALL`) in the recursive folder query stops on a folder cycle; the import never creates one, but the query cannot loop.
- A database migrated from v1 has every flag at 0 until the first diagnosis, decision or undo. Phase 5's `ressac migrate` should call `garden::recompute_triage` after opening, so the app's "Needs triage" count is right at once (raised with the controller).
