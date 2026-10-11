# Phase 2: Schema v2, the backlog rule, search upkeep and the full-text store

Read the plan index first. Global Constraints apply. This phase implements spec §5 (schema v2, the backlog rule, FTS upkeep, existing databases) and the storage half of §6 (full-article results). Review Focus item 4 is pinned here.

**Files (4):**
- Create: `schema/0002_reading.sql`
- Modify: `crates/ressac-core/src/store/mod.rs` (migration list, FTS backfill, `mod reading;`, tests)
- Create: `crates/ressac-core/src/store/reading.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/fetch.rs` (backlog rule, FTS upkeep, tests)
- Regenerated binary asset, not counted: `tests/fixtures/contract.sqlite`

### Task 2.1: Migration 0002, `plain_text` and the FTS backfill

**Interfaces:**
- Consumes: `Store::{open, open_in_memory, schema_version, conn}`; the private runner `Store::migrate` (one transaction per pending migration, a `VACUUM INTO …bak-v<old>` backup first when the file already had a schema); `scraper::{Html, Node}` (scraper 0.27: `Html::parse_fragment`, `ElementRef::descendants`/`ancestors` through its `Deref` to `ego_tree::NodeRef`, `Node::Text` derefs to `str`, `Element::name()`).
- Produces:
  - `schema/0002_reading.sql`, exactly the plan index's SQL (comments added);
  - `store::SCHEMA_VERSION = 2`; `MIGRATIONS` gains `(2, include_str!("../../../../schema/0002_reading.sql"))`; after migration 2's SQL, in the same transaction, `reading::backfill_fts` fills `entry_fts`;
  - `pub fn plain_text(html: &str) -> String` in `store/reading.rs`, re-exported as `ressac_core::store::plain_text`;
  - `pub(super) fn index_entry(conn: &Connection, entry_id: i64, title: Option<&str>, author: Option<&str>, html: Option<&str>) -> rusqlite::Result<()>`, the single place that writes a feed entry's search row (Task 2.2 calls it);
  - `pub(super) fn backfill_fts(conn: &Connection) -> rusqlite::Result<usize>`;
  - test-only `Store::search_ids(&self, query: &str) -> Result<Vec<i64>>` (in `store/mod.rs`, `#[cfg(test)]`).

- [ ] **Step 1: Write the failing tests.**

In `crates/ressac-core/src/store/mod.rs`, inside `impl Store`, after `fn conn`, add the test helper:

```rust
    /// Entry ids whose search row matches an FTS5 query, in id order.
    #[cfg(test)]
    pub(crate) fn search_ids(&self, query: &str) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT rowid FROM entry_fts WHERE entry_fts MATCH ?1 ORDER BY rowid")?;
        let ids = stmt
            .query_map([query], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }
```

In the same file's `mod tests`, replace `fresh_store_has_schema_v1` with:

```rust
    #[test]
    fn fresh_store_has_schema_v2() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        let names = object_names(&store);
        for expected in [
            "entry",
            "entry_fts",
            "entry_fulltext",
            "feed",
            "feed_health",
            "fetch_log",
            "folder",
            "garden_decision",
            "schema_meta",
            "v_entry",
            "v_feed_overview",
            "v_folder_path",
            "v_garden_row",
            "v_unread_count",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }
```

In `reopening_a_file_is_idempotent_and_uses_wal`, change `assert_eq!(store.schema_version().unwrap(), 1);` to `assert_eq!(store.schema_version().unwrap(), 2);`. In `newer_schema_is_refused`, change the pattern `known: 1,` to `known: SCHEMA_VERSION,`. Then append this test at the end of `mod tests` (Review Focus 4):

```rust
    /// Review Focus 4: a sub-project 1 database opens at v2 with every entry searchable.
    #[test]
    fn migration_from_v1_backfills_fts_and_keeps_read_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v1 = Connection::open(&path).unwrap();
            v1.execute_batch(MIGRATIONS[0].1).unwrap();
            v1.execute_batch(
                "INSERT INTO feed (id, position, title, xml_url, xml_url_original, url_key, added_at) \
                 VALUES (1, 0, 'F', 'https://f.example.org/feed', 'https://f.example.org/feed', \
                 'f.example.org/feed', '2026-01-01T00:00:00.000Z');
                 INSERT INTO entry (id, feed_id, guid, title, author, published_at, summary_html, \
                 content_html, read_at, fetched_at) VALUES \
                 (1, 1, 'a', 'Tides', NULL, '2020-01-01T00:00:00.000Z', '<p>harbour</p>', NULL, NULL, \
                 '2026-01-01T00:00:00.000Z'), \
                 (2, 1, 'b', NULL, 'Ada', '2020-01-02T00:00:00.000Z', '<p>short</p>', \
                 '<p>Lighthouse &amp; pier</p>', '2026-01-05T00:00:00.000Z', '2026-01-01T00:00:00.000Z');",
            )
            .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        assert!(path.with_extension("sqlite.bak-v1").exists());
        let read: Vec<Option<String>> = store
            .conn()
            .prepare("SELECT read_at FROM entry ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            read,
            [None, Some("2026-01-05T00:00:00.000Z".to_string())],
            "no backlog rule on migration"
        );
        let indexed: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM entry_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 2);
        assert_eq!(store.search_ids("harbour").unwrap(), [1]);
        assert_eq!(store.search_ids("lighthouse").unwrap(), [2]);
        assert_eq!(store.search_ids("author:ada").unwrap(), [2]);
        assert!(
            store.search_ids("short").unwrap().is_empty(),
            "content_html wins over summary_html"
        );
    }
```

Create `crates/ressac-core/src/store/reading.rs` with only its doc comment and its first test, and add `mod reading;` after `mod health;` in `store/mod.rs`:

```rust
//! Reading data owned by the core: extracted articles (`entry_fulltext`), the search
//! index (`entry_fts`) and the per-feed full-text preference.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_strips_tags_decodes_entities_and_collapses_space() {
        assert_eq!(
            plain_text(
                "<p>Caf&eacute; &amp; <b>cr</b>&egrave;me</p><p>Next\n\n line</p>\
                 <script>var x = 1;</script><style>p { color: red }</style>"
            ),
            "Café & crème Next line"
        );
        assert_eq!(plain_text("one<br>two"), "one two");
        assert_eq!(plain_text("plain words, no tags"), "plain words, no tags");
        assert_eq!(plain_text(""), "");
    }
}
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core store::`. Expected: compile error `cannot find function plain_text in this scope` (reading.rs tests).

- [ ] **Step 3: Implement.**

Create `schema/0002_reading.sql`:

```sql
-- Ressac schema v2: reading state for the apps, full text and search (sub-project 2a).
ALTER TABLE feed ADD COLUMN prefer_full_text INTEGER NOT NULL DEFAULT 0;
ALTER TABLE feed_health ADD COLUMN needs_triage INTEGER NOT NULL DEFAULT 0;

CREATE TABLE entry_fulltext (
  entry_id   INTEGER PRIMARY KEY REFERENCES entry(id),
  fetched_at TEXT NOT NULL,
  html       TEXT,
  title      TEXT,
  byline     TEXT,
  error      TEXT,
  CHECK ((html IS NULL) <> (error IS NULL))
);

-- rowid = entry.id. Kept up to date by the core (no triggers: HTML to text needs Rust).
CREATE VIRTUAL TABLE entry_fts USING fts5(title, author, body, tokenize = 'unicode61 remove_diacritics 2');

CREATE INDEX entry_unread ON entry(feed_id) WHERE read_at IS NULL;
CREATE INDEX entry_starred ON entry(starred_at) WHERE starred_at IS NOT NULL;

-- Contract views for the apps.
CREATE VIEW v_entry AS
SELECT e.id AS entry_id, e.feed_id, f.title AS feed_title, f.folder_id,
       e.title, e.author, e.url, e.published_at,
       (e.read_at IS NOT NULL) AS is_read, (e.starred_at IS NOT NULL) AS is_starred,
       EXISTS (SELECT 1 FROM entry_fulltext t WHERE t.entry_id = e.id AND t.html IS NOT NULL) AS has_fulltext,
       e.summary_html, e.content_html
FROM entry e JOIN feed f ON f.id = e.feed_id
WHERE f.state = 'active';

CREATE VIEW v_unread_count AS
SELECT f.id AS feed_id, COUNT(e.id) AS unread
FROM feed f LEFT JOIN entry e ON e.feed_id = f.id AND e.read_at IS NULL
WHERE f.state = 'active'
GROUP BY f.id;

CREATE VIEW v_garden_row AS
SELECT f.id AS feed_id, f.title, fp.path AS folder_path, f.state,
       h.status, h.reason, json_extract(h.evidence, '$.clue') AS clue,
       h.evidence, h.remarks, h.proposed_action, COALESCE(h.needs_triage, 0) AS needs_triage,
       (SELECT d.decision FROM garden_decision d WHERE d.feed_id = f.id ORDER BY d.id DESC LIMIT 1) AS last_decision,
       f.wayback_url, f.html_url, f.xml_url
FROM feed f
LEFT JOIN v_folder_path fp ON fp.folder_id = f.folder_id
LEFT JOIN feed_health h ON h.feed_id = f.id;

UPDATE schema_meta SET version = 2;
```

In `crates/ressac-core/src/store/reading.rs`, insert between the doc comment and `mod tests`:

```rust
use rusqlite::{Connection, params};
use scraper::{Html, Node};

/// Elements whose boundaries separate words, so `<p>a</p><p>b</p>` reads "a b".
const WORD_BREAKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "td",
    "th",
    "tr",
    "ul",
];

/// Elements whose text is never shown.
const HIDDEN: &[&str] = &["script", "style", "template", "noscript"];

/// Plain text of an HTML fragment: tags stripped, entities decoded, scripts and styles
/// dropped, whitespace collapsed.
pub fn plain_text(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    let mut out = String::with_capacity(html.len());
    for node in fragment.root_element().descendants() {
        match node.value() {
            Node::Text(text) => {
                let hidden = node.ancestors().any(|a| {
                    a.value()
                        .as_element()
                        .is_some_and(|e| HIDDEN.contains(&e.name()))
                });
                if !hidden {
                    out.push_str(text);
                }
            }
            Node::Element(e) if WORD_BREAKS.contains(&e.name()) => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Writes the search row of a feed entry (rowid = entry id) after an insert or a change.
/// `html` is the entry's `content_html`, else its `summary_html`. An extracted article
/// keeps its body: only the title and author follow the feed.
pub(super) fn index_entry(
    conn: &Connection,
    entry_id: i64,
    title: Option<&str>,
    author: Option<&str>,
    html: Option<&str>,
) -> rusqlite::Result<()> {
    let has_article: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM entry_fulltext WHERE entry_id = ?1 AND html IS NOT NULL)",
        [entry_id],
        |r| r.get(0),
    )?;
    if has_article {
        conn.execute(
            "UPDATE entry_fts SET title = ?2, author = ?3 WHERE rowid = ?1",
            params![entry_id, title, author],
        )?;
    } else {
        conn.execute("DELETE FROM entry_fts WHERE rowid = ?1", [entry_id])?;
        conn.execute(
            "INSERT INTO entry_fts (rowid, title, author, body) VALUES (?1, ?2, ?3, ?4)",
            params![
                entry_id,
                title,
                author,
                html.map(plain_text).unwrap_or_default()
            ],
        )?;
    }
    Ok(())
}

/// Fills `entry_fts` from every existing entry. Runs once, inside migration 2.
pub(super) fn backfill_fts(conn: &Connection) -> rusqlite::Result<usize> {
    let mut stmt =
        conn.prepare("SELECT id, title, author, COALESCE(content_html, summary_html) FROM entry")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, title, author, html) in &rows {
        index_entry(
            conn,
            *id,
            title.as_deref(),
            author.as_deref(),
            html.as_deref(),
        )?;
    }
    Ok(rows.len())
}
```

In `crates/ressac-core/src/store/mod.rs`, replace the re-exports, the version constant and the migration list (from `pub use decisions::…` down to `const MIGRATIONS…;`) with:

```rust
pub use decisions::{DecisionRecord, FeedSnapshot};
pub use feeds::{FeedRow, FolderRow, ImportReport};
pub use fetch::{AttemptRecord, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
pub use reading::plain_text;

/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 2;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../schema/0001_init.sql")),
    (2, include_str!("../../../../schema/0002_reading.sql")),
];

/// The migration after which existing entries get their search rows.
const FTS_MIGRATION: i64 = 2;
```

and, in `fn migrate`, make the loop body:

```rust
        for (version, sql) in pending {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            if *version == FTS_MIGRATION {
                reading::backfill_fts(&tx)?;
            }
            tx.execute("UPDATE schema_meta SET version = ?1", [version])?;
            tx.commit()?;
        }
```

- [ ] **Step 4: Run the unit tests.** `cargo test -p ressac-core --lib` → `198 passed; 0 failed; 1 ignored` (the Phase 1 spike).

- [ ] **Step 5: Regenerate the contract fixture.** `cargo test -p ressac-core --test contract` now fails in `contract_fixture_schema_matches_the_live_store` (the committed fixture is still v1). Regenerate it at v2 with the existing test hook, then rerun:

```bash
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked
cargo test --workspace
```

Expected: every suite green. `sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT version FROM schema_meta"` prints `2`. (Phase 5 enriches the fixture's data; this step only keeps the schema in step.)

- [ ] **Step 6: Commit.**

```bash
cargo fmt --all
git add schema/0002_reading.sql crates/ressac-core/src/store/mod.rs crates/ressac-core/src/store/reading.rs tests/fixtures/contract.sqlite
git commit -m "Schema v2: full text, search index, reading views; backfill search on migration"
```

### Task 2.2: The backlog rule and search upkeep on fetch

**Interfaces:**
- Consumes: `reading::index_entry` (Task 2.1); `Store::record_attempt(&self, run_id: &str, a: &FetchAttempt) -> Result<usize>` (signature unchanged); `rusqlite::OptionalExtension` (`Result<T>::optional() -> Result<Option<T>>`), and `Statement::query_row(params, f)` on an `UPDATE … RETURNING` statement.
- Produces:
  - `pub const BACKLOG_DAYS: i64 = 7` in `store/fetch.rs`, re-exported from `store`;
  - in `record_attempt`, for a feed with no entry yet, an entry with `published_at < at − 7 days` is inserted with `read_at = ts(at)`;
  - every inserted entry gets its search row; an existing entry is rewritten, and its search row redone, only when one of `url`, `title`, `updated_at`, `summary_html`, `content_html` changed.

- [ ] **Step 1: Write the failing tests.** Append to `mod tests` in `crates/ressac-core/src/store/fetch.rs`:

```rust
    fn read_at(store: &Store, guid: &str) -> Option<String> {
        store
            .conn()
            .query_row("SELECT read_at FROM entry WHERE guid = ?1", [guid], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn dated(guid: &str, days_ago: i64) -> ParsedEntry {
        ParsedEntry {
            published_at: now() - Duration::days(days_ago),
            ..entry(guid)
        }
    }

    #[test]
    fn first_fill_stores_entries_older_than_seven_days_as_read() {
        let store = store_with_feeds(1);
        let mut first = attempt(1, now(), None, &[]);
        first.feed.as_mut().unwrap().entries = vec![dated("new", 6), dated("old", 8)];
        store.record_attempt("r1", &first).unwrap();
        assert_eq!(read_at(&store, "new"), None);
        assert_eq!(
            read_at(&store, "old").as_deref(),
            Some("2026-10-10T08:00:00.000Z")
        );
        let mut second = attempt(1, now() + Duration::hours(1), None, &[]);
        second.feed.as_mut().unwrap().entries = vec![dated("older", 30)];
        store.record_attempt("r2", &second).unwrap();
        assert_eq!(
            read_at(&store, "older"),
            None,
            "only the first fill has a backlog"
        );
    }

    #[test]
    fn entries_are_indexed_and_reindexed_only_when_they_change() {
        let store = store_with_feeds(1);
        let mut first = attempt(1, now(), None, &[]);
        let mut e = entry("a");
        e.summary_html = Some("<p>alpha &amp; omega</p>".into());
        first.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r1", &first).unwrap();
        assert_eq!(store.search_ids("alpha").unwrap(), [1]);
        assert_eq!(store.search_ids("title:A").unwrap(), [1]);

        e.content_html = Some("<p>beta</p>".into());
        let mut second = attempt(1, now() + Duration::hours(1), None, &[]);
        second.feed.as_mut().unwrap().entries = vec![e.clone()];
        store.record_attempt("r2", &second).unwrap();
        assert_eq!(
            store.search_ids("beta").unwrap(),
            [1],
            "content wins over summary"
        );
        assert!(store.search_ids("alpha").unwrap().is_empty());

        // An identical entry leaves the row alone: a body rewritten by hand survives.
        store
            .conn()
            .execute("UPDATE entry_fts SET body = 'gamma' WHERE rowid = 1", [])
            .unwrap();
        let mut third = attempt(1, now() + Duration::hours(2), None, &[]);
        third.feed.as_mut().unwrap().entries = vec![e];
        store.record_attempt("r3", &third).unwrap();
        assert_eq!(store.search_ids("gamma").unwrap(), [1]);
    }
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core store::fetch`. Expected: 2 failures. `first_fill_stores_entries_older_than_seven_days_as_read` panics with `left: None, right: Some("2026-10-10T08:00:00.000Z")`; `entries_are_indexed_and_reindexed_only_when_they_change` panics with `left: [], right: [1]`.

- [ ] **Step 3: Implement** in `crates/ressac-core/src/store/fetch.rs`.

Replace the imports and constants at the top (up to and including `pub const FRESH_FOR_MINUTES`) with:

```rust
use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, params};

use super::Store;
use super::reading::index_entry;
use crate::error::Result;
use crate::model::{ErrorKind, FeedId, Hop, parse_ts, ts};
use crate::source::{FeedRef, FetchAttempt};

pub const FETCH_LOG_KEEP: i64 = 20;
pub const FRESH_FOR_MINUTES: i64 = 30;
/// On a feed's first fill, entries published more than this many days before the fetch
/// arrive already read.
pub const BACKLOG_DAYS: i64 = 7;
```

Replace the doc comment of `record_attempt` with:

```rust
    /// Writes one attempt in a transaction: log row (trimmed to the last 20), feed cache
    /// fields, entries and their search rows. On a feed's first fill, entries published
    /// more than `BACKLOG_DAYS` before the fetch arrive read. Returns how many entries were new.
```

and replace its entries block (from `let mut added = 0;` to the line before `tx.commit()?;`) with:

```rust
        let mut added = 0;
        if let Some(feed) = &a.feed {
            tx.execute(
                "UPDATE feed SET description_current = ?2 WHERE id = ?1",
                params![a.feed_id, feed.description],
            )?;
            let first_fill: bool = tx.query_row(
                "SELECT NOT EXISTS (SELECT 1 FROM entry WHERE feed_id = ?1)",
                [a.feed_id],
                |r| r.get(0),
            )?;
            let backlog_before = a.at - Duration::days(BACKLOG_DAYS);
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO entry (feed_id, guid, url, title, author, published_at, \
                 published_at_is_fallback, updated_at, summary_html, content_html, read_at, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            // Only a real change rewrites the row, and only then is the search row redone.
            let mut update = tx.prepare(
                "UPDATE entry SET url = ?3, title = ?4, updated_at = ?5, summary_html = ?6, content_html = ?7 \
                 WHERE feed_id = ?1 AND guid = ?2 AND (url IS NOT ?3 OR title IS NOT ?4 \
                 OR updated_at IS NOT ?5 OR summary_html IS NOT ?6 OR content_html IS NOT ?7) \
                 RETURNING id, author",
            )?;
            for e in &feed.entries {
                let read_at = (first_fill && e.published_at < backlog_before).then(|| ts(a.at));
                let inserted = insert.execute(params![
                    a.feed_id,
                    e.guid,
                    e.url,
                    e.title,
                    e.author,
                    ts(e.published_at),
                    e.published_at_is_fallback,
                    e.updated_at.map(ts),
                    e.summary_html,
                    e.content_html,
                    read_at,
                    ts(a.at)
                ])?;
                let html = e.content_html.as_deref().or(e.summary_html.as_deref());
                if inserted == 1 {
                    added += 1;
                    let id = tx.last_insert_rowid();
                    index_entry(&tx, id, e.title.as_deref(), e.author.as_deref(), html)?;
                } else {
                    let changed: Option<(i64, Option<String>)> = update
                        .query_row(
                            params![
                                a.feed_id,
                                e.guid,
                                e.url,
                                e.title,
                                e.updated_at.map(ts),
                                e.summary_html,
                                e.content_html
                            ],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    if let Some((id, author)) = changed {
                        index_entry(&tx, id, e.title.as_deref(), author.as_deref(), html)?;
                    }
                }
            }
        }
```

In `crates/ressac-core/src/store/mod.rs`, re-export the constant:

```rust
pub use fetch::{AttemptRecord, BACKLOG_DAYS, EntryRecord, FETCH_LOG_KEEP, FRESH_FOR_MINUTES};
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core store::fetch` → `6 passed`; `cargo test --workspace` green (the end-to-end tests, whose entries are 5 to 14 days old and arrive on a first fill, only check health, never read state).

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/store/fetch.rs crates/ressac-core/src/store/mod.rs
git commit -m "Backlog rule on a feed's first fill; keep the search index in step with entries"
```

### Task 2.3: The full-text store

**Interfaces:**
- Consumes: `extract::{Article, ExtractError}` (Phase 1; `ExtractError::sentence`); `reading::index_entry`; `fetch::bad_timestamp` (made `pub(super)` here); `model::{FeedId, parse_ts, ts}`.
- Produces (methods on `Store`, in `store/reading.rs`; `FulltextRow` re-exported from `store`):
  - `pub struct FulltextRow { pub fetched_at: DateTime<Utc>, pub html: Option<String>, pub title: Option<String>, pub byline: Option<String>, pub error: Option<String> }`, deriving `Debug, Clone, PartialEq`;
  - `save_fulltext(&self, entry_id: i64, outcome: &std::result::Result<Article, ExtractError>, now: DateTime<Utc>) -> Result<()>`: an article upserts `html`, `title`, `byline`, clears `error`, and sets `entry_fts.body = article.text`; a failure stores `error = e.sentence()`, except that it never replaces an article stored earlier;
  - `fulltext(&self, entry_id: i64) -> Result<Option<FulltextRow>>`;
  - `prefer_full_text(&self, feed_id: FeedId) -> Result<bool>` (an unknown feed is an error) and `set_prefer_full_text(&self, feed_id: FeedId, on: bool) -> Result<()>`;
  - `entry_for_extract(&self, entry_id: i64) -> Result<Option<(FeedId, Option<String>)>>`: `None` for an unknown entry, `Some((feed, None))` for an entry without a link;
  - `entries_needing_fulltext(&self, feed_id: FeedId, since: DateTime<Utc>, limit: usize) -> Result<Vec<(i64, String)>>`: entries of the feed with `fetched_at >= since` (first stored in this run, since `fetched_at` is never updated), a link, and no `entry_fulltext` row (neither article nor error), newest `published_at` first, at most `limit`.

- [ ] **Step 1: Write the failing tests.** In `crates/ressac-core/src/store/reading.rs`, replace the head of `mod tests` (`use super::*;`) with the imports and helpers below, keep the `plain_text…` test, and append the other tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ErrorKind;
    use crate::opml;
    use crate::parse::{ParsedEntry, ParsedFeed};
    use crate::source::FetchAttempt;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn entry(guid: &str, url: Option<&str>, summary: &str, days_ago: i64) -> ParsedEntry {
        ParsedEntry {
            guid: guid.into(),
            url: url.map(str::to_string),
            title: Some(format!("Title {guid}")),
            author: Some("Ada".into()),
            published_at: now() - chrono::Duration::days(days_ago),
            published_at_is_fallback: false,
            updated_at: None,
            summary_html: Some(summary.into()),
            content_html: None,
        }
    }

    fn attempt(at: DateTime<Utc>, entries: Vec<ParsedEntry>) -> FetchAttempt {
        FetchAttempt {
            feed_id: 1,
            at,
            requested_url: "https://f.example.org/feed".into(),
            final_url: "https://f.example.org/feed".into(),
            http_status: Some(200),
            error: None,
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            bytes: 100,
            duration_ms: 5,
            not_modified: false,
            etag: None,
            last_modified: None,
            feed: Some(ParsedFeed {
                title: Some("F".into()),
                description: None,
                site_url: None,
                entries,
            }),
        }
    }

    /// One feed with two entries: id 1 "a" (with a link) and id 2 "b" (no link).
    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml =
            r#"<opml><body><outline text="F" xmlUrl="https://f.example.org/feed"/></body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        s.record_attempt(
            "r1",
            &attempt(
                now(),
                vec![
                    entry("a", Some("https://f.example.org/a"), "<p>harbour</p>", 1),
                    entry("b", None, "<p>lighthouse</p>", 2),
                ],
            ),
        )
        .unwrap();
        s
    }

    fn article() -> Article {
        Article {
            html: "<div><p>The road of wet sand.</p></div>".into(),
            title: Some("On tides".into()),
            byline: Some("Ada Lovelace".into()),
            text: "The road of wet sand.".into(),
        }
    }

    // fn plain_text_strips_tags_decodes_entities_and_collapses_space stays here, unchanged.

    #[test]
    fn an_article_is_stored_and_becomes_the_search_body() {
        let s = store();
        assert_eq!(s.search_ids("harbour").unwrap(), [1]);
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.fetched_at, now());
        assert_eq!(row.html.as_deref(), Some(article().html.as_str()));
        assert_eq!(row.title.as_deref(), Some("On tides"));
        assert_eq!(row.byline.as_deref(), Some("Ada Lovelace"));
        assert_eq!(row.error, None);
        assert_eq!(s.search_ids("sand").unwrap(), [1]);
        assert!(s.search_ids("harbour").unwrap().is_empty());
        assert_eq!(s.fulltext(2).unwrap(), None);
    }

    #[test]
    fn a_failure_is_stored_as_its_sentence_and_a_later_article_replaces_it() {
        let s = store();
        s.save_fulltext(1, &Err(ExtractError::Fetch(ErrorKind::Http(404))), now())
            .unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.error.as_deref(), Some("HTTP 404."));
        assert_eq!(row.html, None);
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!((row.html.is_some(), row.error), (true, None));
    }

    #[test]
    fn a_failed_retry_keeps_the_article() {
        let s = store();
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let later = now() + chrono::Duration::hours(1);
        s.save_fulltext(1, &Err(ExtractError::LoginWall), later)
            .unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.html.as_deref(), Some(article().html.as_str()));
        assert_eq!((row.error, row.fetched_at), (None, now()));
    }

    #[test]
    fn a_changed_feed_entry_keeps_the_article_body_but_follows_its_title() {
        let s = store();
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let mut changed = entry("a", Some("https://f.example.org/a"), "<p>pier</p>", 1);
        changed.title = Some("Renamed".into());
        s.record_attempt(
            "r2",
            &attempt(now() + chrono::Duration::hours(1), vec![changed]),
        )
        .unwrap();
        assert_eq!(s.search_ids("sand").unwrap(), [1]);
        assert!(s.search_ids("pier").unwrap().is_empty());
        assert_eq!(s.search_ids("title:renamed").unwrap(), [1]);
    }

    #[test]
    fn prefer_full_text_defaults_off_and_toggles() {
        let s = store();
        assert!(!s.prefer_full_text(1).unwrap());
        s.set_prefer_full_text(1, true).unwrap();
        assert!(s.prefer_full_text(1).unwrap());
        assert!(s.prefer_full_text(99).is_err(), "unknown feed");
    }

    #[test]
    fn entry_for_extract_gives_the_feed_and_the_link() {
        let s = store();
        assert_eq!(
            s.entry_for_extract(1).unwrap(),
            Some((1, Some("https://f.example.org/a".to_string())))
        );
        assert_eq!(s.entry_for_extract(2).unwrap(), Some((1, None)));
        assert_eq!(s.entry_for_extract(99).unwrap(), None);
    }

    #[test]
    fn entries_needing_fulltext_are_this_runs_linked_entries_without_a_row() {
        let s = store();
        let run = now() + chrono::Duration::hours(1);
        s.record_attempt(
            "r2",
            &attempt(
                run,
                vec![
                    entry("c", Some("https://f.example.org/c"), "<p>c</p>", 0),
                    entry("d", Some("https://f.example.org/d"), "<p>d</p>", 0),
                    entry("e", Some("https://f.example.org/e"), "<p>e</p>", 0),
                ],
            ),
        )
        .unwrap();
        s.save_fulltext(4, &Err(ExtractError::NoContent), run)
            .unwrap();
        let due = s.entries_needing_fulltext(1, run, 20).unwrap();
        let ids: Vec<i64> = due.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [5, 3], "not before the run, not already tried");
        assert_eq!(due[1].1, "https://f.example.org/c");
        assert_eq!(s.entries_needing_fulltext(1, run, 1).unwrap().len(), 1);
        assert_eq!(
            s.entries_needing_fulltext(1, now(), 20).unwrap().len(),
            3,
            "entry 2 has no link"
        );
    }
}
```

(The comment line marks where the existing `plain_text…` test stays; do not keep the comment.)

- [ ] **Step 2: Run them.** `cargo test -p ressac-core store::reading`. Expected: compile errors, `cannot find type DateTime`, `cannot find struct Article`, `no method named save_fulltext found for struct Store`.

- [ ] **Step 3: Implement.**

In `crates/ressac-core/src/store/fetch.rs`, make the timestamp helper visible to the sibling module: `fn bad_timestamp(col: usize) -> rusqlite::Error {` becomes `pub(super) fn bad_timestamp(col: usize) -> rusqlite::Error {`.

In `crates/ressac-core/src/store/reading.rs`, replace the two `use` lines with this block and the struct below it:

```rust
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use scraper::{Html, Node};

use super::Store;
use super::fetch::bad_timestamp;
use crate::error::Result;
use crate::extract::{Article, ExtractError};
use crate::model::{FeedId, parse_ts, ts};

#[derive(Debug, Clone, PartialEq)]
pub struct FulltextRow {
    pub fetched_at: DateTime<Utc>,
    pub html: Option<String>,
    pub title: Option<String>,
    pub byline: Option<String>,
    pub error: Option<String>,
}
```

Then insert after `fn backfill_fts`, before `mod tests`:

```rust
impl Store {
    /// Stores the outcome of an extraction. An article replaces any earlier error and
    /// becomes the entry's search body. A failure is stored as its sentence, except that
    /// it never replaces an article extracted earlier.
    pub fn save_fulltext(
        &self,
        entry_id: i64,
        outcome: &std::result::Result<Article, ExtractError>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        match outcome {
            Ok(article) => {
                tx.execute(
                    "INSERT INTO entry_fulltext (entry_id, fetched_at, html, title, byline, error) \
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL) \
                     ON CONFLICT(entry_id) DO UPDATE SET fetched_at = excluded.fetched_at, \
                     html = excluded.html, title = excluded.title, byline = excluded.byline, error = NULL",
                    params![entry_id, ts(now), article.html, article.title, article.byline],
                )?;
                tx.execute(
                    "UPDATE entry_fts SET body = ?2 WHERE rowid = ?1",
                    params![entry_id, article.text],
                )?;
            }
            Err(e) => {
                tx.execute(
                    "INSERT INTO entry_fulltext (entry_id, fetched_at, error) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(entry_id) DO UPDATE SET fetched_at = excluded.fetched_at, \
                     error = excluded.error WHERE entry_fulltext.html IS NULL",
                    params![entry_id, ts(now), e.sentence()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn fulltext(&self, entry_id: i64) -> Result<Option<FulltextRow>> {
        let row = self
            .conn()
            .query_row(
                "SELECT fetched_at, html, title, byline, error FROM entry_fulltext WHERE entry_id = ?1",
                [entry_id],
                |r| {
                    let fetched: String = r.get(0)?;
                    Ok(FulltextRow {
                        fetched_at: parse_ts(&fetched).ok_or_else(|| bad_timestamp(0))?,
                        html: r.get(1)?,
                        title: r.get(2)?,
                        byline: r.get(3)?,
                        error: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn prefer_full_text(&self, feed_id: FeedId) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT prefer_full_text FROM feed WHERE id = ?1",
            [feed_id],
            |r| r.get(0),
        )?)
    }

    /// The app writes this column itself; the core only needs it in tests.
    pub fn set_prefer_full_text(&self, feed_id: FeedId, on: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET prefer_full_text = ?2 WHERE id = ?1",
            params![feed_id, on],
        )?;
        Ok(())
    }

    /// The entry's feed and link, or `None` for an unknown entry.
    pub fn entry_for_extract(&self, entry_id: i64) -> Result<Option<(FeedId, Option<String>)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT feed_id, url FROM entry WHERE id = ?1",
                [entry_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Entries of `feed_id` first stored at or after `since` (this run), with a link and
    /// no extraction yet, newest first.
    pub fn entries_needing_fulltext(
        &self,
        feed_id: FeedId,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn().prepare(
            "SELECT e.id, e.url FROM entry e WHERE e.feed_id = ?1 AND e.fetched_at >= ?2 \
             AND e.url IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM entry_fulltext t WHERE t.entry_id = e.id) \
             ORDER BY e.published_at DESC, e.id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![feed_id, ts(since), limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
```

In `crates/ressac-core/src/store/mod.rs`, change `pub use reading::plain_text;` to:

```rust
pub use reading::{FulltextRow, plain_text};
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core store::reading` → `8 passed`; `cargo test -p ressac-core --lib` → `207 passed; 0 failed; 1 ignored`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/store/reading.rs crates/ressac-core/src/store/fetch.rs crates/ressac-core/src/store/mod.rs
git commit -m "Store extracted articles, their failures and the full-text preference"
```

### Verification gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green, including `crates/ressac-core/tests/contract.rs` against the regenerated fixture.

### Notes for the executor

- **Verified on 2026-10-10** in a scratch copy of the repository, task by task: each task's end state passes `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; the counts above come from that run.
- **Real data:** a copy of the calibration database (10 222 entries, v1) migrates in 0.7 s with a release build (`ressac --db <copy> export` triggers it). Afterwards: 10 222 FTS rows, no entry marked read, `MATCH 'velo'` finds « vélo » (`remove_diacritics 2`), and `ressac.sqlite.bak-v1` sits next to the file. Never try this on `~/Library/…`.
- **FTS5** is compiled into rusqlite's bundled SQLite (`libsqlite3-sys 0.38.2`, `build.rs` passes `-DSQLITE_ENABLE_FTS5`). The virtual table adds shadow tables (`entry_fts_config`, `_content`, `_data`, `_docsize`, `_idx`) to `sqlite_master`; `store_schema_equals_the_sql_files` still passes because both sides create them.
- A search row is replaced with `DELETE` then `INSERT` on `rowid`; FTS5 also accepts `UPDATE entry_fts SET col = … WHERE rowid = …`, used when only some columns change.
- `UPDATE … RETURNING` through `Statement::query_row(…).optional()`: SQLite applies the whole update on the first step, and no matching row (nothing changed) gives `None`. The `IS NOT` guard means an unchanged entry no longer rewrites its row on every fetch; the stored values stay the same as before.
- `search_ids` lives in `store/mod.rs` rather than `reading.rs` so that Task 2.1 compiles without unused imports (`Store` and `Result` would only be used by test code there).
- **Triage flags after migration:** `needs_triage` starts at 0 for every feed; Phase 3 recomputes it after each diagnosis, decision and undo. A migrated database shows an empty "Needs triage" until one of those runs, unless `ressac migrate` (Phase 5) calls `garden::recompute_triage` after opening the store.
- `entries_needing_fulltext` relies on `entry.fetched_at` never being updated after the insert (true: the update path does not touch it).
