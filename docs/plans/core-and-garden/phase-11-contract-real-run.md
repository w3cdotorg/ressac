# Phase 11: Schema contract, real Vienna run, calibration

Read the plan index first. Global Constraints apply. Spec §10 (contract test, manual run) and §11 (calibration).

**Files (≤5):**
- Create: `crates/ressac-core/tests/contract.rs`
- Modify: `.gitignore` (allow `tests/fixtures/contract.sqlite`)
- Create (generated): `tests/fixtures/contract.sqlite`
- Modify: `notes.md` (results of the real run)
- Modify, only if calibration requires it: `crates/ressac-core/src/health/mod.rs` or `health/hijack.rs` (constants, and the test that pins the change)

### Task 11.1: Schema contract test and fixture database for the Swift app

**Interfaces:**
- Consumes: `ressac_core::store::Store`, `ressac_core::service::Ressac`, the `schema/*.sql` files, the fixture OPML.
- Produces: `tests/fixtures/contract.sqlite`. This is a schema-v1 database with the fixture imported and one decision. Sub-project 2 (SwiftUI) reads it in its own tests.

- [ ] **Step 1: Write the tests** `crates/ressac-core/tests/contract.rs`

```rust
//! The SQLite schema is the contract with the SwiftUI app (spec §4–5).

use std::path::Path;

use ressac_core::model::{Decision, Status};
use ressac_core::store::Store;
use rusqlite::Connection;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn schema_of(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT type || ' ' || name || ': ' || sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name")
        .unwrap();
    stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
}

fn migrations() -> Vec<String> {
    let mut files: Vec<_> = std::fs::read_dir(Path::new(ROOT).join("schema"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files.iter().map(|p| std::fs::read_to_string(p).unwrap()).collect()
}

#[test]
fn store_schema_equals_the_sql_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let live = schema_of(&Connection::open(&path).unwrap());
    let raw = Connection::open_in_memory().unwrap();
    for sql in migrations() {
        raw.execute_batch(&sql).unwrap();
    }
    assert_eq!(live, schema_of(&raw));
}

#[test]
fn overview_view_columns_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.sqlite");
    drop(Store::open(&path).unwrap());
    let conn = Connection::open(&path).unwrap();
    let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('v_feed_overview')").unwrap();
    let cols: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(
        cols,
        ["feed_id", "title", "xml_url", "html_url", "state", "folder_path", "status", "reason", "last_entry_at", "decision"]
    );
}

/// Regenerates the fixture for the Swift app: `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract`.
#[test]
fn write_contract_fixture_when_asked() {
    if std::env::var_os("RESSAC_WRITE_CONTRACT").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("contract.sqlite");
    let store = Store::open(&work).unwrap();
    let opml = std::fs::read_to_string(Path::new(ROOT).join("tests/fixtures/vienna-like.opml")).unwrap();
    let outlines = ressac_core::opml::parse(&opml).unwrap();
    let now = "2026-10-10T08:00:00Z".parse().unwrap();
    store.import_outlines(&outlines, false, now).unwrap();
    store.record_decision(1, Decision::Keep, Status::Unchecked, None, now).unwrap();
    drop(store);
    let target = Path::new(ROOT).join("tests/fixtures/contract.sqlite");
    let _ = std::fs::remove_file(&target);
    Connection::open(&work)
        .unwrap()
        .execute("VACUUM INTO ?1", [target.to_str().unwrap()])
        .unwrap();
}
```

- [ ] **Step 2: Run.**

```bash
cargo test -p ressac-core --test contract
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract
```

Expected: `3 passed` both times, and `tests/fixtures/contract.sqlite` exists. Check it with `sqlite3 tests/fixtures/contract.sqlite "select count(*) from v_feed_overview"`, which should print `11`.

- [ ] **Step 3: Allow the fixture** in `.gitignore` by appending `!tests/fixtures/contract.sqlite` after the `*.sqlite` lines.

- [ ] **Step 4: Verification gate**, then commit and push ("Schema contract test and fixture database for the Swift app", both attribution lines).

### Task 11.2: Real run on the Vienna OPML and calibration

This is the spec's manual run. It touches the user's real file **read-only**. It uses a database in the scratchpad, never the default path, and **never takes garden decisions on the user's behalf**.

- [ ] **Step 1: Build and run**

```bash
cargo build --release -p ressac
DB=<scratchpad>/vienna-run.sqlite
./target/release/ressac --db "$DB" import ~/Documents/vienna_20260415.opml --dry-run
./target/release/ressac --db "$DB" import ~/Documents/vienna_20260415.opml
time ./target/release/ressac --db "$DB" check
```

Expected:
- **Import:** 186 feeds added (187 minus the Daring Fireball duplicate) in 13 folders, with 1 merge.
- **Check:** runs for a few minutes, then prints the summary.

- [ ] **Step 2: Inspect the results**

```bash
sqlite3 -header -column "$DB" "select status, count(*) from v_feed_overview group by status order by count(*) desc"
sqlite3 -header -column "$DB" "select o.title, o.status, h.hijack_score, json_extract(h.evidence,'$.clue') clue from v_feed_overview o join feed_health h using(feed_id) where o.status in ('Hijacked','Dead','Moved','Duplicate') order by o.status, o.title"
```

Check against what we already know (`notes.md` § OPML Vienna):

| Feed(s) | Expected |
|---|---|
| Bibliobsession 2.0, apps & stacks, Publishers Weekly, HIPSTAMATIC SKYRIM | Hijacked, or at least Dead / Ailing with spam evidence |
| Re/code, longform.org, FiveThirtyEight | Dead or Moved |
| Orange Drenka / Orange-Drenka | `similar_title` remark if one side is dead |
| Most of the 100 `http://` feeds | `HttpsAvailable` remark |

Also check:
- whether false positives exist among obviously live sites (BBC, Le Monde, xkcd, Simon Willison);
- whether `Unchecked` remains for any feed, and why;
- whether 403 or 429 appear, and on which hosts.

- [ ] **Step 3: Calibrate only with evidence.** For each wrong verdict:
  1. Explain the cause in one line.
  2. Add a unit test reproducing it in the owning `health` test module (Red).
  3. Adjust the rule or constant (Green).
  4. Re-run `diagnose` without refetching: `./target/release/ressac --db "$DB" fetch` within 30 minutes only re-diagnoses.

  Constants may change only with a pinned test. Never loosen `HIJACK_THRESHOLD` to make one feed pass.

- [ ] **Step 4: Log the run in `notes.md`** (in French) under a new section "Premier essai réel (date)":
  - duration and summary counts;
  - notable verdicts, with the feeds named;
  - false positives found and how they were fixed (commit hashes);
  - open questions for the user;
  - export sizes: `ressac export | grep -c xmlUrl` and `ressac export --cemetery | grep -c xmlUrl`. Expect 0 for the cemetery, since nothing is buried yet.

- [ ] **Step 5: Verification gate**, then commit and push ("Real Vienna run: calibration and notes", both attribution lines).

- [ ] **Step 6: Hand-off to the user.**
  - Report the summary.
  - Give the exact commands to triage with their real database (`ressac import …`, `ressac check`, `ressac garden`), using the default path this time.
  - List the remaining open questions.

  The triage itself is the user's.
