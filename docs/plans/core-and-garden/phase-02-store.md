# Phase 2: Domain types, errors, schema v1, store

Read the plan index first. Global Constraints apply.

**Files (5):**
- Create: `schema/0001_init.sql`
- Create: `crates/ressac-core/src/model.rs` (tests inline)
- Create: `crates/ressac-core/src/error.rs`
- Create: `crates/ressac-core/src/store/mod.rs` (tests inline)
- Modify: `crates/ressac-core/src/lib.rs`

### Task 2.1: Domain types

**Interfaces:**
- Consumes: nothing.
- Produces (`ressac_core::model`):
  - `FeedId = i64`, `FolderId = i64`
  - `ts(DateTime<Utc>) -> String`, `parse_ts(&str) -> Option<DateTime<Utc>>`
  - `Status` with `ALL`, `urgency()`, `as_str()`, `parse()`, `needs_triage()`
  - `ErrorKind` with `is_network()`, `label()`, `clue()`
  - `Hop { status: u16, url: String }`
  - `Decision` with `as_str()`, `parse()`
  - `Action` with `is_safe()`, `is_repair()`, `label()`
  - `Remark`, `Evidence { bullets, clue, language }`, `Cadence`, `Diagnosis`

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/src/model.rs`

```rust
//! Domain types shared by every module and persisted in the database.

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
    fn only_problem_statuses_need_triage() {
        let triage: Vec<_> = Status::ALL.into_iter().filter(|s| s.needs_triage()).collect();
        assert_eq!(
            triage,
            vec![Status::Duplicate, Status::Hijacked, Status::Moved, Status::Dead, Status::Ailing, Status::Fossil]
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
        assert_eq!(serde_json::from_str::<ErrorKind>(&json).unwrap(), ErrorKind::Http(410));
        assert_eq!(serde_json::to_string(&ErrorKind::Dns).unwrap(), "\"Dns\"");
        assert_eq!(ErrorKind::Http(410).label(), "HTTP 410");
        assert_eq!(ErrorKind::Timeout.clue(), "timeout");
        assert!(ErrorKind::Connect.is_network());
        assert!(!ErrorKind::Timeout.is_network());
    }

    #[test]
    fn action_safety() {
        assert!(Action::UpgradeHttps { url: "https://a".into() }.is_safe());
        assert!(Action::FollowRedirect { url: "https://a".into() }.is_safe());
        assert!(!Action::AdoptDiscovered { url: "https://a".into() }.is_safe());
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
```

- [ ] **Step 2: Add `mod model;` and run to verify failure.** In `lib.rs` add `pub mod model;`.

Run: `cargo test -p ressac-core model`
Expected: compile errors (`Status`, `ts`, … not found).

- [ ] **Step 3: Implement.** Insert above the test module in `model.rs`:

```rust
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub type FeedId = i64;
pub type FolderId = i64;

/// Database timestamp format: RFC 3339, UTC, millisecond precision, `Z` suffix.
pub fn ts(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
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
        Self::ALL.iter().position(|s| *s == self).unwrap_or(usize::MAX)
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
            Status::Duplicate | Status::Hijacked | Status::Moved | Status::Dead | Status::Ailing | Status::Fossil
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
        matches!(self, Action::UpgradeHttps { .. } | Action::FollowRedirect { .. })
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cadence {
    pub last_entry_at: Option<DateTime<Utc>>,
    pub median_interval_days: Option<f64>,
    pub entries_per_month: f64,
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ressac-core model`
Expected: `7 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/model.rs crates/ressac-core/src/lib.rs
git commit -m "Domain types: statuses, error kinds, actions, diagnosis

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2.2: Error type, schema v1, store open and migrations

**Interfaces:**
- Consumes: nothing beyond `model`.
- Produces:
  - `ressac_core::error::{Error, Result}`
  - `ressac_core::store::{Store, SCHEMA_VERSION, default_path}`
  - `Store::open(&Path) -> Result<Store>`, `Store::open_in_memory() -> Result<Store>`
  - `Store::schema_version(&self) -> Result<i64>`
  - `pub(crate) fn conn(&self) -> &rusqlite::Connection`

- [ ] **Step 1: Create** `crates/ressac-core/src/error.rs`

```rust
//! Crate-wide error type. Per-feed fetch failures are data (`model::ErrorKind`), not errors.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema v{found} is newer than this ressac (knows v{known}); update ressac")]
    SchemaTooNew { found: i64, known: i64 },
    #[error("cannot read OPML at line {line}, column {column}: {message}")]
    Opml { line: usize, column: usize, message: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 2: Create** `schema/0001_init.sql`. This file is the contract with the Swift app: never edit it after release; add `0002_*.sql` instead.

```sql
-- Ressac schema v1. Timestamps: RFC 3339 UTC text with milliseconds and 'Z'.
CREATE TABLE schema_meta (version INTEGER NOT NULL);
INSERT INTO schema_meta (version) VALUES (1);

CREATE TABLE folder (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES folder(id),
  name      TEXT NOT NULL,
  position  INTEGER NOT NULL
);

CREATE TABLE feed (
  id                   INTEGER PRIMARY KEY,
  folder_id            INTEGER REFERENCES folder(id),
  position             INTEGER NOT NULL,
  title                TEXT NOT NULL,
  xml_url              TEXT NOT NULL,
  xml_url_original     TEXT NOT NULL,
  html_url             TEXT,
  url_key              TEXT NOT NULL,
  final_url            TEXT,
  description_original TEXT,
  description_current  TEXT,
  etag                 TEXT,
  last_modified        TEXT,
  state                TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'buried')),
  merged_into_id       INTEGER REFERENCES feed(id),
  opml_attrs           TEXT NOT NULL DEFAULT '{}',
  https_available      INTEGER,
  discovered_url       TEXT,
  discovery_checked_at TEXT,
  wayback_url          TEXT,
  added_at             TEXT NOT NULL
);
CREATE UNIQUE INDEX feed_url_key_active ON feed(url_key) WHERE state = 'active';

CREATE TABLE fetch_log (
  id              INTEGER PRIMARY KEY,
  feed_id         INTEGER NOT NULL REFERENCES feed(id),
  run_id          TEXT NOT NULL,
  at              TEXT NOT NULL,
  http_status     INTEGER,
  error_kind      TEXT,             -- JSON of model::ErrorKind, e.g. "Dns" or {"Http":404}
  redirect_chain  TEXT NOT NULL DEFAULT '[]',
  content_type    TEXT,
  bytes           INTEGER NOT NULL DEFAULT 0,
  duration_ms     INTEGER NOT NULL DEFAULT 0,
  entry_count     INTEGER,
  not_modified    INTEGER NOT NULL DEFAULT 0,
  network_suspect INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX fetch_log_feed_at ON fetch_log(feed_id, at DESC);

CREATE TABLE entry (
  id                       INTEGER PRIMARY KEY,
  feed_id                  INTEGER NOT NULL REFERENCES feed(id),
  guid                     TEXT NOT NULL,
  url                      TEXT,
  title                    TEXT,
  author                   TEXT,
  published_at             TEXT NOT NULL,
  published_at_is_fallback INTEGER NOT NULL DEFAULT 0,
  updated_at               TEXT,
  summary_html             TEXT,
  content_html             TEXT,
  content_kind             TEXT,
  read_at                  TEXT,
  starred_at               TEXT,
  fetched_at               TEXT NOT NULL,
  UNIQUE (feed_id, guid)
);
CREATE INDEX entry_feed_published ON entry(feed_id, published_at DESC);

CREATE TABLE feed_health (
  feed_id              INTEGER PRIMARY KEY REFERENCES feed(id),
  status               TEXT NOT NULL,
  reason               TEXT NOT NULL,
  evidence             TEXT NOT NULL DEFAULT '{}',
  remarks              TEXT NOT NULL DEFAULT '[]',
  proposed_action      TEXT,
  hijack_score         REAL NOT NULL DEFAULT 0,
  last_entry_at        TEXT,
  median_interval_days REAL,
  entries_per_month    REAL NOT NULL DEFAULT 0,
  computed_at          TEXT NOT NULL
);

CREATE TABLE garden_decision (
  id                 INTEGER PRIMARY KEY,
  feed_id            INTEGER NOT NULL REFERENCES feed(id),
  decision           TEXT NOT NULL CHECK (decision IN ('keep', 'repair', 'bury')),
  status_at_decision TEXT NOT NULL,
  action             TEXT,
  previous           TEXT NOT NULL DEFAULT '{}',
  decided_at         TEXT NOT NULL,
  note               TEXT
);
CREATE INDEX garden_decision_feed ON garden_decision(feed_id, id DESC);

-- Contract views (stable API for other clients).
CREATE VIEW v_folder_path AS
WITH RECURSIVE p(id, path) AS (
  SELECT id, name FROM folder WHERE parent_id IS NULL
  UNION ALL
  SELECT f.id, p.path || ' / ' || f.name FROM folder f JOIN p ON f.parent_id = p.id
)
SELECT id AS folder_id, path FROM p;

CREATE VIEW v_feed_overview AS
SELECT f.id AS feed_id,
       f.title,
       f.xml_url,
       f.html_url,
       f.state,
       fp.path AS folder_path,
       h.status,
       h.reason,
       h.last_entry_at,
       (SELECT d.decision FROM garden_decision d
         WHERE d.feed_id = f.id ORDER BY d.id DESC LIMIT 1) AS decision
FROM feed f
LEFT JOIN v_folder_path fp ON fp.folder_id = f.folder_id
LEFT JOIN feed_health h ON h.feed_id = f.id;
```

- [ ] **Step 3: Write the failing tests** in `crates/ressac-core/src/store/mod.rs`

```rust
//! SQLite storage. Every SQL statement of the crate lives under this module.

#[cfg(test)]
mod tests {
    use super::*;

    fn object_names(store: &Store) -> Vec<String> {
        let mut stmt = store
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn fresh_store_has_schema_v1() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        let names = object_names(&store);
        for expected in [
            "entry", "feed", "feed_health", "fetch_log", "folder", "garden_decision",
            "schema_meta", "v_feed_overview", "v_folder_path",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn reopening_a_file_is_idempotent_and_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/ressac.sqlite");
        drop(Store::open(&path).unwrap());
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
        let mode: String = store.conn().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        let store = Store::open(&path).unwrap();
        store.conn().execute("UPDATE schema_meta SET version = 99", []).unwrap();
        drop(store);
        match Store::open(&path) {
            Err(Error::SchemaTooNew { found: 99, known: 1 }) => {}
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("newer schema was accepted"),
        }
    }

    #[test]
    fn default_path_is_under_application_support() {
        let p = default_path();
        assert!(p.ends_with("Library/Application Support/Ressac/ressac.sqlite"), "{p:?}");
    }
}
```

- [ ] **Step 4: Register the modules** in `lib.rs` so it reads:

```rust
//! Ressac core: storage, fetching, diagnosis and OPML for the Ressac feed reader.

pub mod error;
pub mod model;
pub mod store;
pub mod url_key;

pub use error::{Error, Result};
```

Run: `cargo test -p ressac-core store`
Expected: compile errors (`Store` not found).

- [ ] **Step 5: Implement.** Insert above the test module in `store/mod.rs`:

```rust
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 1;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../../../schema/0001_init.sql"))];

pub struct Store {
    conn: Connection,
    path: Option<PathBuf>,
}

/// `$HOME/Library/Application Support/Ressac/ressac.sqlite`.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/Ressac/ressac.sqlite")
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        let mut store = Store { conn, path: Some(path.to_path_buf()) };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut store = Store { conn: Connection::open_in_memory()?, path: None };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    fn configure(&self) -> Result<()> {
        // journal_mode returns a row, so it is read rather than executed.
        self.conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        self.conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        let has_meta: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
            [],
            |r| r.get(0),
        )?;
        if has_meta == 0 {
            return Ok(0);
        }
        Ok(self.conn.query_row("SELECT version FROM schema_meta", [], |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(Error::SchemaTooNew { found: current, known: SCHEMA_VERSION });
        }
        let pending: Vec<&(i64, &str)> = MIGRATIONS.iter().filter(|(v, _)| *v > current).collect();
        if pending.is_empty() {
            return Ok(());
        }
        if current > 0 {
            self.backup(current)?;
        }
        for (version, sql) in pending {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("UPDATE schema_meta SET version = ?1", [version])?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Copies the database to `ressac.sqlite.bak-v<version>` before migrating it.
    fn backup(&self, version: i64) -> Result<()> {
        if let Some(path) = &self.path {
            let dest = path.with_extension(format!("sqlite.bak-v{version}"));
            self.conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
        }
        Ok(())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p ressac-core store`
Expected: `4 passed`.

- [ ] **Step 7: Verification gate** (fmt, clippy `-D warnings`, test). All green.

- [ ] **Step 8: Commit and push**

```bash
git add schema crates/ressac-core/src
git commit -m "Error type, schema v1 and store with migrations

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Phase gate:** report and wait for approval before Phase 3.
