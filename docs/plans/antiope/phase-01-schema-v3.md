# Phase 1: Schema v3 and page numbers in the core

**Runs after:** nothing: it is the first phase (1a, then 1b). Execution order of the sub-project: 1a → 1b → 6 → 2 → 3 → 4 → 5 → 7a → 7b → 8 → 9 → 10a → 10b → 11a → 11b → 12 → 13 → 14.

Read the plan index first. Global Constraints apply. This phase implements spec §4 (data, reserved pages, assignment, migration), the refusals of `page set` at the core level, the v3 rows of §10 and the Rust half of §11 except the CLI tests, which are Phase 2's. None of the plan index's five Review Focus items belongs to this phase (they are owned by Swift phases).

## Two sub-phases (deviation from the plan index)

The plan index puts schema, assignment, the import and decision hooks and the error variants in one phase. Against the real code that is seven files, because bumping `SCHEMA_VERSION` ripples into two test files outside the store:

- `crates/ressac-core/tests/contract.rs`: `reading_columns_and_views_are_stable` and `contract_fixture_covers_the_reading_cases` assert version `2`, and `contract_fixture_schema_matches_the_live_store` compares the committed v2 fixture with a v3 store;
- `crates/ressac/tests/cli.rs`: `migrate_reports_the_schema_version` expects `Schema version 2.`.

So the phase that bumps the version must touch both test files and regenerate the fixture. This file therefore holds **Phase 1a** (5 files) and **Phase 1b** (4 files), each with its own verification gate and its own approval stop. Consequences:

- **The fixture regeneration and the schema drift test move here from Phase 2** (1a regenerates at v3; 1b regenerates again once import numbers pages, and pins those numbers). Phase 2 no longer touches `contract.rs` or the fixture.
- **`error.rs` and `pages::set` move to Phase 1b.**
- **`store/decisions.rs` needs no hook and is not touched.** No transaction in it inserts a feed: `record_decision` and `undo_last_decision` only `UPDATE` (the two `INSERT INTO feed` in that file are in its tests). A buried, merged or restored feed keeps its row, hence its page; `buried_and_merged_feeds_keep_their_numbers` pins it.

**Phase 1a files (5):**
- Create: `schema/0003_pages.sql`
- Create: `crates/ressac-core/src/store/pages.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (`pub mod pages;`, version 3, migration entry, page backfill, tests)
- Modify: `crates/ressac-core/tests/contract.rs` (version 3, `v_page` columns)
- Modify: `crates/ressac/tests/cli.rs` (`Schema version 3.`)
- Regenerated binary asset, not counted: `tests/fixtures/contract.sqlite`

**Phase 1b files (4):**
- Modify: `crates/ressac-core/src/error.rs` (four variants)
- Modify: `crates/ressac-core/src/store/pages.rs` (`set`, tests)
- Modify: `crates/ressac-core/src/store/feeds.rs` (hook at the end of the import transaction, tests)
- Modify: `crates/ressac-core/tests/contract.rs` (page numbers of the fixture)
- Regenerated binary asset, not counted: `tests/fixtures/contract.sqlite`

**Step 0 rule:** every change here is additive (constants, one migration entry, one hook line, new functions and tests). No file over 300 lines is restructured (`store/mod.rs` 372 lines, `store/feeds.rs` 515, `contract.rs` 510, `cli.rs` 475), so no cleanup commit is needed. Check the counts with `wc -l` when the phase starts; if one of these files has been restructured since, apply the rule.

**Builds on (existing code, signatures unchanged):**
- `Store::{open, open_in_memory, schema_version, conn, write_tx}`. The private `Store::migrate` takes a `VACUUM INTO <file>.bak-v<old>` backup when the file already had a schema, then runs each pending migration in one `IMMEDIATE` transaction, re-reading the version under the lock; after migration 2's SQL it runs `reading::backfill_fts(&tx)` in the same transaction. The page backfill follows that pattern exactly.
- `Store::import_outlines(&self, outlines: &[Outline], dry_run: bool, now: DateTime<Utc>) -> Result<ImportReport>`: one `write_tx`, rolled back on a dry run.
- `Store::record_decision(&self, feed_id, decision, status, action, now) -> Result<()>` (tests only).
- `crate::error::{Error, Result}`; rusqlite 0.40.2 implements `FromSql` and `ToSql` for `u16` (range-checked), so pages are read and bound as `u16` directly.
- The fixture hook: `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked`.

**Produces** (plan index, Phase 1, exact names and signatures):
- `schema/0003_pages.sql`, exactly the index's SQL plus comments and `UPDATE schema_meta SET version = 3;` (as `0002` does);
- `store::SCHEMA_VERSION = 3`, `MIGRATIONS` gains `(3, include_str!("../../../../schema/0003_pages.sql"))`, and `pages::assign_missing` runs after migration 3's SQL in the same transaction;
- in `crates/ressac-core/src/store/pages.rs`: `RESERVED_PAGES`, `ASSIGNABLE`, `PageKind`, `PageRow`, `assign_missing`, `list` (1a) and `set` (1b);
- in `error.rs` (1b): `PageReserved(u16)`, `PageOutOfRange(u16)`, `PageTaken { page: u16 }`, `UnknownPageTarget { kind: &'static str, id: i64 }`;
- `assign_missing` at the end of the import transaction (1b).

**Additions the index does not list** (needed by Phase 2):
- the module is public, `pub mod pages;` in `store/mod.rs`, so the path is `ressac_core::store::pages::{PageKind, PageRow, RESERVED_PAGES, ASSIGNABLE, assign_missing, list, set}`. Re-exporting them from `store` would put bare `list` and `set` there, so they stay in their module. `service.rs`, outside `store`, calls them through this path;
- `impl PageKind { pub fn as_str(self) -> &'static str }` gives `"folder"`, `"feed"` or `"reserved"`. `set` uses it for `UnknownPageTarget.kind`, Phase 2 for its text lines;
- error messages (Phase 2 prints them on stderr as they are): `Page 800 is reserved.`, `Page 900 is not between 110 and 799.`, `Page 201 is already taken.`, `No feed #99.` / `No folder #99.`.

**Assignment, where the index leaves a choice:**
- **Two passes.** The first pass walks the tree in display order and places only the items that have a home range: a root folder's own hundred, the contents of a root folder whose page is a hundred, and root feeds (110–199). The second pass gives every item left over the lowest free number of the pool, in the same order. An overflowing item therefore never takes a number that an item placed later in the walk would have had at home (`a_full_hundred_overflows_after_every_item_with_a_home`).
- **"Next free"** means the lowest free number of the range.
- **A root folder whose hundred is already held** (by any folder or feed, after a `page set`) overflows like a seventh root folder, and so do its contents (`folders_and_feeds_share_one_number_space`).
- **Pool exhausted** (more than 684 overflowing items): the item keeps a NULL page, which leaves it out of `v_page`, and a `tracing::warn!` is logged (`an_exhausted_pool_leaves_the_last_items_unnumbered`).

Start by checking that the tree is green: `export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"` then `cargo test --workspace` (expected: `ressac` unit tests 31, `cli` 13, `ressac-core` lib `227 passed; 0 failed; 1 ignored`, `contract` 6, `end_to_end` 4).

---

# Phase 1a: migration, assignment, list

### Task 1.1: Migration 0003 and page assignment

**Files:**
- Create: `schema/0003_pages.sql`
- Create: `crates/ressac-core/src/store/pages.rs`
- Modify: `crates/ressac-core/src/store/mod.rs`
- Modify: `crates/ressac-core/tests/contract.rs`
- Modify: `crates/ressac/tests/cli.rs`
- Regenerate: `tests/fixtures/contract.sqlite`

**Interfaces:**
- Consumes: `Store::open_in_memory`, `Store::conn` (`pub(crate)`), `Store::record_decision`, `crate::model::{Action, Decision, Status}`, `tracing::warn!`.
- Produces:
  - `pub const RESERVED_PAGES: [u16; 8] = [100, 101, 102, 103, 800, 801, 802, 899];`
  - `pub const ASSIGNABLE: std::ops::RangeInclusive<u16> = 110..=799;`
  - `pub enum PageKind { Folder, Feed, Reserved }` deriving `Debug, Clone, Copy, PartialEq, Eq, Serialize`, `#[serde(rename_all = "lowercase")]`, with `pub fn as_str(self) -> &'static str`;
  - `pub fn assign_missing(conn: &rusqlite::Connection) -> Result<usize>`;
  - `store::SCHEMA_VERSION = 3`; migration 3 runs `pages::assign_missing(&tx)` before its version update and commit.

- [ ] **Step 1: Write the failing tests.**

In `crates/ressac-core/src/store/mod.rs`, declare the module after `mod health;`:

```rust
mod health;
pub mod pages;
mod reading;
```

Create `crates/ressac-core/src/store/pages.rs` with its doc comment and its tests only:

```rust
//! Teletext page numbers (schema v3). The core gives every folder and feed a number once
//! and never changes it on its own; the apps read them through `v_page`.

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::model::{Action, Decision, Status};
    use crate::store::Store;

    fn now() -> DateTime<Utc> {
        "2026-10-14T08:00:00Z".parse().unwrap()
    }

    /// A folder named "Folder <id>".
    fn folder(s: &Store, id: i64, parent: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO folder (id, parent_id, name, position) VALUES (?1, ?2, ?3, ?4)",
                params![id, parent, format!("Folder {id}"), position],
            )
            .unwrap();
    }

    /// An active feed titled "Feed <id>", at https://f<id>.example.org/feed.
    fn feed(s: &Store, id: i64, folder: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, '2026-10-14T08:00:00.000Z')",
                params![
                    id,
                    folder,
                    position,
                    format!("Feed {id}"),
                    format!("https://f{id}.example.org/feed"),
                    format!("f{id}.example.org/feed")
                ],
            )
            .unwrap();
    }

    fn folder_page(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM folder WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn feed_page(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM feed WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn assign(s: &Store) -> usize {
        assign_missing(s.conn()).unwrap()
    }

    #[test]
    fn root_folders_take_hundreds_in_display_order() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 2);
        folder(&s, 2, None, 0);
        feed(&s, 10, None, 0); // same position as folder 2: folders come first
        folder(&s, 3, None, 1);
        folder(&s, 4, None, 1); // same position as folder 3: lower id first
        assert_eq!(assign(&s), 5);
        assert_eq!(
            [2, 3, 4, 1].map(|id| folder_page(&s, id)),
            [Some(200), Some(300), Some(400), Some(500)]
        );
        assert_eq!(
            feed_page(&s, 10),
            Some(110),
            "a root feed is not a root folder"
        );
    }

    #[test]
    fn contents_fill_their_hundred_depth_first_in_display_order() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0); // Infos
        feed(&s, 10, Some(1), 0); // A
        folder(&s, 2, Some(1), 1); // Sub, before E at the same position
        feed(&s, 11, Some(2), 0); // B
        feed(&s, 12, Some(2), 1); // C
        feed(&s, 14, Some(1), 1); // E
        feed(&s, 13, Some(1), 2); // D
        assign(&s);
        assert_eq!(folder_page(&s, 1), Some(200));
        assert_eq!(
            [
                feed_page(&s, 10),
                folder_page(&s, 2),
                feed_page(&s, 11),
                feed_page(&s, 12)
            ],
            [Some(201), Some(202), Some(203), Some(204)],
            "pre-order: a folder before its contents"
        );
        assert_eq!(
            [feed_page(&s, 14), feed_page(&s, 13)],
            [Some(205), Some(206)]
        );
    }

    #[test]
    fn root_feeds_take_110_to_199() {
        let s = Store::open_in_memory().unwrap();
        feed(&s, 1, None, 2);
        feed(&s, 2, None, 0);
        feed(&s, 3, None, 1);
        folder(&s, 4, None, 3);
        assign(&s);
        assert_eq!(
            [2, 3, 1].map(|id| feed_page(&s, id)),
            [Some(110), Some(111), Some(112)]
        );
        assert_eq!(folder_page(&s, 4), Some(200));
    }

    #[test]
    fn a_full_hundred_overflows_after_every_item_with_a_home() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        for id in 1..=100 {
            feed(&s, id, Some(1), id);
        }
        feed(&s, 101, None, 1); // a root feed shown after the full folder
        assert_eq!(assign(&s), 102);
        assert_eq!(feed_page(&s, 1), Some(201));
        assert_eq!(feed_page(&s, 99), Some(299));
        assert_eq!(
            feed_page(&s, 101),
            Some(110),
            "homed items are placed first"
        );
        assert_eq!(
            feed_page(&s, 100),
            Some(111),
            "then the overflow takes the lowest free"
        );
    }

    #[test]
    fn a_seventh_root_folder_and_its_contents_take_the_lowest_free_pages() {
        let s = Store::open_in_memory().unwrap();
        for i in 1..=7 {
            folder(&s, i, None, i);
            feed(&s, 10 + i, Some(i), 0);
        }
        feed(&s, 20, None, 8);
        assign(&s);
        assert_eq!(
            (1..=6).map(|i| folder_page(&s, i)).collect::<Vec<_>>(),
            [200, 300, 400, 500, 600, 700].map(Some)
        );
        assert_eq!(feed_page(&s, 16), Some(701));
        assert_eq!(feed_page(&s, 20), Some(110));
        assert_eq!(folder_page(&s, 7), Some(111));
        assert_eq!(
            feed_page(&s, 17),
            Some(112),
            "no hundred, so its contents overflow too"
        );
    }

    #[test]
    fn the_pool_never_yields_a_reserved_page_or_a_hundred() {
        let mut used = BTreeSet::new();
        while let Some(page) = pool_next(&used) {
            assert!(ASSIGNABLE.contains(&page), "{page}");
            assert!(!RESERVED_PAGES.contains(&page), "{page}");
            assert!(!page.is_multiple_of(100), "{page}");
            used.insert(page);
        }
        assert_eq!(used.len(), 690 - 6, "110…799 minus 200, 300, … 700");
        assert_eq!(used.first(), Some(&110));
        assert_eq!(used.last(), Some(&799));
    }

    #[test]
    fn overflow_skips_a_free_hundred() {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=91 {
            feed(&s, id, None, id);
        }
        assign(&s);
        assert_eq!(feed_page(&s, 90), Some(199));
        assert_eq!(feed_page(&s, 91), Some(201), "200 is free but a hundred");
    }

    #[test]
    fn an_exhausted_pool_leaves_the_last_items_unnumbered() {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=685 {
            feed(&s, id, None, id);
        }
        assert_eq!(
            assign(&s),
            684,
            "110…799 holds 684 numbers outside the hundreds"
        );
        assert_eq!(feed_page(&s, 684), Some(799));
        assert_eq!(feed_page(&s, 685), None);
    }

    #[test]
    fn existing_numbers_are_never_changed() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        feed(&s, 11, Some(1), 1);
        assert_eq!(assign(&s), 3);
        s.conn()
            .execute("UPDATE feed SET page = 150 WHERE id = 10", [])
            .unwrap();
        feed(&s, 12, Some(1), 2);
        feed(&s, 13, Some(1), -1);
        assert_eq!(assign(&s), 2, "only the NULL pages");
        assert_eq!(feed_page(&s, 10), Some(150));
        assert_eq!(feed_page(&s, 11), Some(202));
        assert_eq!(
            feed_page(&s, 13),
            Some(201),
            "the lowest free number of the hundred"
        );
        assert_eq!(feed_page(&s, 12), Some(203));
        assert_eq!(assign(&s), 0, "idempotent");
    }

    #[test]
    fn buried_and_merged_feeds_keep_their_numbers() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        for id in [10, 11, 12] {
            feed(&s, id, Some(1), id);
        }
        assign(&s);
        s.record_decision(10, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        s.record_decision(
            11,
            Decision::Repair,
            Status::Duplicate,
            Some(&Action::MergeInto { feed_id: 12 }),
            now(),
        )
        .unwrap();
        assert_eq!(assign(&s), 0);
        assert_eq!(
            [10, 11, 12].map(|id| feed_page(&s, id)),
            [Some(201), Some(202), Some(203)]
        );
        feed(&s, 13, Some(1), 13);
        assign(&s);
        assert_eq!(feed_page(&s, 13), Some(204), "201 and 202 are still held");
    }

    #[test]
    fn folders_and_feeds_share_one_number_space() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        folder(&s, 2, Some(1), 1);
        feed(&s, 10, Some(1), 0);
        folder(&s, 3, None, 1);
        feed(&s, 20, None, 2);
        s.conn()
            .execute_batch(
                "UPDATE folder SET page = 201 WHERE id = 2; UPDATE feed SET page = 300 WHERE id = 20;",
            )
            .unwrap();
        assign(&s);
        assert_eq!(feed_page(&s, 10), Some(202), "201 is a folder's");
        assert_eq!(
            folder_page(&s, 3),
            Some(110),
            "300 is a feed's, so the second root folder overflows"
        );
    }
}
```

In `store/mod.rs`, `mod tests`:

- rename `fresh_store_has_schema_v2` to `fresh_store_has_schema_v3`, make its first assertion `assert_eq!(store.schema_version().unwrap(), 3);`, and add `"v_page",` to the expected names between `"v_garden_row",` and `"v_unread_count",`;
- in `reopening_a_file_is_idempotent_and_uses_wal`, `migration_from_v1_backfills_fts_and_keeps_read_state` and `migration_survives_an_existing_backup_and_reopening` (two places), and on the last line of `a_failing_migration_leaves_v1_untouched_and_is_retryable`, change `schema_version().unwrap(), 2)` to `schema_version().unwrap(), 3)`. That is five assertions; `grep -n "schema_version().unwrap(), 2" crates/ressac-core/src/store/mod.rs` must print nothing afterwards;
- append, at the end of `mod tests` (before its closing `}`):

```rust
    /// A v2 database: two root items, a subfolder and a buried feed, no page column yet.
    fn v2_database(path: &Path) -> Connection {
        let v2 = Connection::open(path).unwrap();
        v2.execute_batch(MIGRATIONS[0].1).unwrap();
        v2.execute_batch(MIGRATIONS[1].1).unwrap();
        v2.execute_batch(
            "INSERT INTO folder (id, parent_id, name, position) VALUES (1, NULL, 'Infos', 0), (2, 1, 'Sub', 1);
             INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, state, added_at) VALUES \
             (1, NULL, 1, 'Root', 'https://r.example.org/feed', 'https://r.example.org/feed', 'r.example.org/feed', 'active', '2026-01-01T00:00:00.000Z'), \
             (2, 1, 0, 'A', 'https://a.example.org/feed', 'https://a.example.org/feed', 'a.example.org/feed', 'active', '2026-01-01T00:00:00.000Z'), \
             (3, 2, 0, 'B', 'https://b.example.org/feed', 'https://b.example.org/feed', 'b.example.org/feed', 'buried', '2026-01-01T00:00:00.000Z');",
        )
        .unwrap();
        v2
    }

    #[test]
    fn migration_from_v2_numbers_every_folder_and_feed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        drop(v2_database(&path));
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 3);
        assert!(path.with_extension("sqlite.bak-v2").exists());
        let missing: i64 = store
            .conn()
            .query_row(
                "SELECT (SELECT count(*) FROM folder WHERE page IS NULL) + (SELECT count(*) FROM feed WHERE page IS NULL)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing, 0);
        let pages: Vec<(u16, String, i64)> = store
            .conn()
            .prepare("SELECT page, kind, ref_id FROM v_page ORDER BY page")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            pages,
            [
                (110, "feed".to_string(), 1),
                (200, "folder".to_string(), 1),
                (201, "feed".to_string(), 2),
                (202, "folder".to_string(), 2),
                (203, "feed".to_string(), 3),
            ],
            "the buried feed is numbered and listed too"
        );
    }

    #[test]
    fn a_failing_v3_migration_leaves_v2_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v2 = v2_database(&path);
            // Poison: 0003 creates this view after its ALTERs and indexes have already run.
            v2.execute_batch("CREATE VIEW v_page AS SELECT 1 AS x")
                .unwrap();
        }
        assert!(Store::open(&path).is_err());
        let raw = Connection::open(&path).unwrap();
        let version: i64 = raw
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let columns: i64 = raw
            .query_row(
                "SELECT (SELECT count(*) FROM pragma_table_info('feed') WHERE name = 'page') \
                 + (SELECT count(*) FROM pragma_table_info('folder') WHERE name = 'page')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 0, "ALTERs rolled back");
        assert!(path.with_extension("sqlite.bak-v2").exists());
        raw.execute_batch("DROP VIEW v_page").unwrap();
        drop(raw);
        assert_eq!(Store::open(&path).unwrap().schema_version().unwrap(), 3);
    }
```

In `crates/ressac-core/tests/contract.rs`, in `reading_columns_and_views_are_stable`, change `assert_eq!(version, 2);` to `assert_eq!(version, 3);`; in `contract_fixture_covers_the_reading_cases`, change `assert_eq!(int("SELECT version FROM schema_meta"), 2);` to `assert_eq!(int("SELECT version FROM schema_meta"), 3);`. Then insert this test right after `reading_columns_and_views_are_stable`:

```rust
#[test]
fn page_columns_and_view_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_page"),
        ["page", "kind", "ref_id", "title", "parent_page"]
    );
    assert!(columns(&conn, "folder").contains(&"page".to_string()));
    assert!(columns(&conn, "feed").contains(&"page".to_string()));
}
```

In `crates/ressac/tests/cli.rs`, in `migrate_reports_the_schema_version`, change `assert_eq!(stdout(&text), "Schema version 2.\n");` to:

```rust
    assert_eq!(stdout(&text), "Schema version 3.\n");
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib store::`. Expected: the lib tests do not compile. The new tests reach the module through `use super::*`, which is still empty, so rustc reports ``cannot find function `assign_missing` in this scope``, ``cannot find function `pool_next` in this scope``, ``cannot find value `ASSIGNABLE` in this scope``, ``cannot find value `RESERVED_PAGES` in this scope``, ``cannot find type `BTreeSet` in this scope`` and ``cannot find macro `params` in this scope``.

- [ ] **Step 3: Implement.**

Create `schema/0003_pages.sql`:

```sql
-- Ressac schema v3: teletext page numbers for the Antiope theme (sub-project 4).
-- Folders and feeds share one number space; the core enforces it across both tables.
ALTER TABLE folder ADD COLUMN page INTEGER;
ALTER TABLE feed ADD COLUMN page INTEGER;
CREATE UNIQUE INDEX folder_page ON folder(page);
CREATE UNIQUE INDEX feed_page ON feed(page);

-- Contract view for the apps: every numbered folder and feed, buried feeds included.
CREATE VIEW v_page AS
  SELECT f.page, 'folder' AS kind, f.id AS ref_id, f.name AS title,
         (SELECT p.page FROM folder p WHERE p.id = f.parent_id) AS parent_page
  FROM folder f WHERE f.page IS NOT NULL
  UNION ALL
  SELECT d.page, 'feed', d.id, d.title,
         (SELECT p.page FROM folder p WHERE p.id = d.folder_id)
  FROM feed d WHERE d.page IS NOT NULL;

UPDATE schema_meta SET version = 3;
```

In `crates/ressac-core/src/store/pages.rs`, insert between the doc comment and `#[cfg(test)]`:

```rust
use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::Result;

/// Pages fixed in code and never stored: Index, Unread, Today, Starred, the three garden
/// pages and the Guide.
pub const RESERVED_PAGES: [u16; 8] = [100, 101, 102, 103, 800, 801, 802, 899];
/// The pages a folder or a feed may hold.
pub const ASSIGNABLE: RangeInclusive<u16> = 110..=799;

/// Feeds without a folder.
const ROOT_FEEDS: RangeInclusive<u16> = 110..=199;
/// Root folders that get a hundred of their own (200 to 700).
const ROOT_HUNDREDS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Folder,
    Feed,
    Reserved,
}

impl PageKind {
    /// The `kind` of `v_page` and of the CLI's lines: "folder", "feed" or "reserved".
    pub fn as_str(self) -> &'static str {
        match self {
            PageKind::Folder => "folder",
            PageKind::Feed => "feed",
            PageKind::Reserved => "reserved",
        }
    }
}

/// A folder or a feed as the assignment sees it.
#[derive(Debug, Clone, Copy)]
struct Node {
    kind: PageKind,
    id: i64,
    position: i64,
    page: Option<u16>,
}

impl Node {
    /// Display order inside a parent: position, then folders before feeds, then id
    /// (the app's `SidebarNode.tree`).
    fn order(&self) -> (i64, u8, i64) {
        let rank = if self.kind == PageKind::Folder { 0 } else { 1 };
        (self.position, rank, self.id)
    }
}

/// Every folder and feed, grouped by parent folder (`None` = top level), in display order.
struct Tree {
    children: HashMap<Option<i64>, Vec<Node>>,
}

impl Tree {
    fn load(conn: &Connection) -> Result<Self> {
        let mut children: HashMap<Option<i64>, Vec<Node>> = HashMap::new();
        for (kind, sql) in [
            (
                PageKind::Folder,
                "SELECT id, parent_id, position, page FROM folder",
            ),
            (
                PageKind::Feed,
                "SELECT id, folder_id, position, page FROM feed",
            ),
        ] {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, Option<i64>>(1)?,
                        Node {
                            kind,
                            id: r.get(0)?,
                            position: r.get(2)?,
                            page: r.get(3)?,
                        },
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (parent, node) in rows {
                children.entry(parent).or_default().push(node);
            }
        }
        for nodes in children.values_mut() {
            nodes.sort_by_key(Node::order);
        }
        Ok(Tree { children })
    }

    fn children(&self, parent: Option<i64>) -> &[Node] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// The contents of a folder, depth-first, each folder before its own contents.
    fn descendants(&self, folder: i64, out: &mut Vec<Node>) {
        for node in self.children(Some(folder)) {
            out.push(*node);
            if node.kind == PageKind::Folder {
                self.descendants(node.id, out);
            }
        }
    }

    fn used(&self) -> BTreeSet<u16> {
        self.children
            .values()
            .flatten()
            .filter_map(|n| n.page)
            .collect()
    }
}

/// The lowest number of `range` that nobody holds.
fn lowest_free(used: &BTreeSet<u16>, range: RangeInclusive<u16>) -> Option<u16> {
    range.into_iter().find(|n| !used.contains(n))
}

/// The overflow pool: the lowest free assignable number that is neither reserved nor a
/// hundred.
fn pool_next(used: &BTreeSet<u16>) -> Option<u16> {
    ASSIGNABLE
        .into_iter()
        .find(|n| !n.is_multiple_of(100) && !RESERVED_PAGES.contains(n) && !used.contains(n))
}

/// 200, 300, … 700: a root folder's own hundred.
fn is_hundred(page: u16) -> bool {
    page.is_multiple_of(100) && (200..=700).contains(&page)
}

/// What one pass of `assign_missing` has decided so far.
struct Assignment {
    used: BTreeSet<u16>,
    assigned: Vec<(Node, u16)>,
    /// Items with no home range or a full one, in display order, for the pool.
    overflow: Vec<Node>,
}

impl Assignment {
    /// Returns the node's page: the one it holds, else the lowest free number of `home`.
    /// `None` means it waits for the overflow pool.
    fn place(&mut self, node: Node, home: Option<RangeInclusive<u16>>) -> Option<u16> {
        if node.page.is_some() {
            return node.page;
        }
        match home.and_then(|range| lowest_free(&self.used, range)) {
            Some(page) => {
                self.used.insert(page);
                self.assigned.push((node, page));
                Some(page)
            }
            None => {
                self.overflow.push(node);
                None
            }
        }
    }

    /// Second pass: every overflowed item takes the lowest free number of the pool. With
    /// none left (more than 684 items), the item keeps no page.
    fn drain_overflow(&mut self) {
        for node in std::mem::take(&mut self.overflow) {
            match pool_next(&self.used) {
                Some(page) => {
                    self.used.insert(page);
                    self.assigned.push((node, page));
                }
                None => {
                    tracing::warn!(id = node.id, kind = node.kind.as_str(), "no free page left")
                }
            }
        }
    }
}

/// Gives a number to every folder and feed whose `page` is NULL; returns how many were
/// assigned. Numbers already given never change. Items with a home range (a root folder's
/// hundred, its contents, the root feeds' 110–199) are placed first, in display order;
/// the others then take the lowest free numbers of the overflow pool, in the same order.
pub fn assign_missing(conn: &Connection) -> Result<usize> {
    let tree = Tree::load(conn)?;
    let mut pass = Assignment {
        used: tree.used(),
        assigned: Vec::new(),
        overflow: Vec::new(),
    };
    let mut root_folders = 0;
    for &node in tree.children(None) {
        match node.kind {
            PageKind::Folder => {
                let hundred =
                    (root_folders < ROOT_HUNDREDS).then(|| (root_folders as u16 + 2) * 100);
                root_folders += 1;
                let page = pass.place(node, hundred.map(|h| h..=h));
                let home = page.filter(|p| is_hundred(*p)).map(|h| h + 1..=h + 99);
                let mut inside = Vec::new();
                tree.descendants(node.id, &mut inside);
                for child in inside {
                    pass.place(child, home.clone());
                }
            }
            PageKind::Feed => {
                pass.place(node, Some(ROOT_FEEDS));
            }
            PageKind::Reserved => {}
        }
    }
    pass.drain_overflow();
    for (node, page) in &pass.assigned {
        let sql = match node.kind {
            PageKind::Folder => "UPDATE folder SET page = ?2 WHERE id = ?1",
            _ => "UPDATE feed SET page = ?2 WHERE id = ?1",
        };
        conn.execute(sql, params![node.id, page])?;
    }
    Ok(pass.assigned.len())
}
```

In `crates/ressac-core/src/store/mod.rs`, replace everything from `/// Highest schema version this binary knows.` down to the blank line before `/// `schema_meta.version`, or 0 for a database without it.` with:

```rust
/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 3;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../schema/0001_init.sql")),
    (2, include_str!("../../../../schema/0002_reading.sql")),
    (3, include_str!("../../../../schema/0003_pages.sql")),
];

/// The migration after which existing entries get their search rows.
const FTS_MIGRATION: i64 = 2;

/// The migration after which existing folders and feeds get their page numbers.
const PAGES_MIGRATION: i64 = 3;
```

and in `fn migrate`, make the body of the transaction (from `tx.execute_batch(sql)?;` to the `UPDATE schema_meta` line) read:

```rust
            tx.execute_batch(sql)?;
            if *version == FTS_MIGRATION {
                reading::backfill_fts(&tx)?;
            }
            if *version == PAGES_MIGRATION {
                pages::assign_missing(&tx)?;
            }
            tx.execute("UPDATE schema_meta SET version = ?1", [version])?;
```

- [ ] **Step 4: Run the tests.**

`cargo test -p ressac-core --lib` → `240 passed; 0 failed; 1 ignored` (the Phase 1 spike of sub-project 2a stays ignored).

`cargo test -p ressac-core --test contract` now fails twice: `contract_fixture_schema_matches_the_live_store` (the committed fixture is still v2, so its schema lacks `page` and `v_page`) and `contract_fixture_covers_the_reading_cases` (`left: 2, right: 3`). Regenerate the fixture with the existing hook, then run everything:

```bash
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked
cargo test --workspace
sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT version FROM schema_meta; SELECT count(*) FROM v_page;"
```

Expected: every suite green (`ressac` 31, `cli` 13, lib 240 + 1 ignored, `contract` 7, `end_to_end` 4); `sqlite3` prints `3` then `0` (import does not number pages before Task 1.4).

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add schema/0003_pages.sql crates/ressac-core/src/store/pages.rs crates/ressac-core/src/store/mod.rs crates/ressac-core/tests/contract.rs crates/ressac/tests/cli.rs tests/fixtures/contract.sqlite
git commit -m "Schema v3: page numbers for folders and feeds, assigned at migration" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 1.2: `pages::list`

**Files:**
- Modify: `crates/ressac-core/src/store/pages.rs`

**Interfaces:**
- Consumes: `v_page` (Task 1.1), `RESERVED_PAGES`, `PageKind`.
- Produces:
  - `pub struct PageRow { pub page: u16, pub kind: PageKind, pub id: Option<i64>, pub title: String, pub parent_page: Option<u16> }` deriving `Debug, Clone, PartialEq, Eq, Serialize`. It serializes as `{"page":201,"kind":"feed","id":10,"title":"Feed 10","parent_page":200}`;
  - `pub fn list(conn: &rusqlite::Connection) -> Result<Vec<PageRow>>`: the eight reserved pages in `RESERVED_PAGES` order, titled `Index`, `Unread`, `Today`, `Starred`, `Needs triage`, `All feeds`, `Cemetery`, `Guide` (kind `Reserved`, no id, no parent), then `v_page` ordered by page.

- [ ] **Step 1: Write the failing test.** In `pages.rs`, `mod tests`, add `use serde_json::json;` under `use chrono::{DateTime, Utc};`, and append at the end of the module:

```rust
    #[test]
    fn list_gives_reserved_pages_then_v_page_by_page() {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        folder(&s, 2, Some(1), 1);
        feed(&s, 11, None, 1);
        assign(&s);
        s.record_decision(10, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        let rows = list(s.conn()).unwrap();
        let reserved: Vec<(u16, &str)> = rows[..8]
            .iter()
            .map(|r| (r.page, r.title.as_str()))
            .collect();
        assert_eq!(
            reserved,
            [
                (100, "Index"),
                (101, "Unread"),
                (102, "Today"),
                (103, "Starred"),
                (800, "Needs triage"),
                (801, "All feeds"),
                (802, "Cemetery"),
                (899, "Guide")
            ]
        );
        assert!(
            rows[..8]
                .iter()
                .all(|r| r.kind == PageKind::Reserved && r.id.is_none())
        );
        let stored: Vec<(u16, PageKind, Option<i64>, Option<u16>)> = rows[8..]
            .iter()
            .map(|r| (r.page, r.kind, r.id, r.parent_page))
            .collect();
        assert_eq!(
            stored,
            [
                (110, PageKind::Feed, Some(11), None),
                (200, PageKind::Folder, Some(1), None),
                (201, PageKind::Feed, Some(10), Some(200)),
                (202, PageKind::Folder, Some(2), Some(200)),
            ],
            "the buried feed 10 stays listed"
        );
        assert_eq!(
            serde_json::to_value(&rows[10]).unwrap(),
            json!({"page": 201, "kind": "feed", "id": 10, "title": "Feed 10", "parent_page": 200})
        );
        assert_eq!(
            serde_json::to_value(&rows[0]).unwrap(),
            json!({"page": 100, "kind": "reserved", "id": null, "title": "Index", "parent_page": null})
        );
    }
```

- [ ] **Step 2: Run it.** `cargo test -p ressac-core --lib store::pages`. Expected: compile error ``cannot find function `list` in this scope``.

- [ ] **Step 3: Implement.** In `pages.rs`, insert the titles right after `pub const ASSIGNABLE…;`:

```rust
/// Titles of `RESERVED_PAGES`, in the same order.
const RESERVED_TITLES: [&str; 8] = [
    "Index",
    "Unread",
    "Today",
    "Starred",
    "Needs triage",
    "All feeds",
    "Cemetery",
    "Guide",
];
```

the row type right after the `impl PageKind { … }` block:

```rust
/// One line of `ressac page list`: a reserved page (no id), or a numbered folder or feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageRow {
    pub page: u16,
    pub kind: PageKind,
    pub id: Option<i64>,
    pub title: String,
    pub parent_page: Option<u16>,
}
```

and `list` right after `assign_missing`:

```rust
/// Reserved pages first (in `RESERVED_PAGES` order), then every row of `v_page` by page.
pub fn list(conn: &Connection) -> Result<Vec<PageRow>> {
    let mut rows: Vec<PageRow> = RESERVED_PAGES
        .iter()
        .zip(RESERVED_TITLES)
        .map(|(&page, title)| PageRow {
            page,
            kind: PageKind::Reserved,
            id: None,
            title: title.to_string(),
            parent_page: None,
        })
        .collect();
    let mut stmt = conn.prepare(
        "SELECT page, kind, ref_id, title, parent_page FROM v_page ORDER BY page, kind, ref_id",
    )?;
    let stored = stmt
        .query_map([], |r| {
            let kind: String = r.get(1)?;
            let kind = match kind.as_str() {
                "folder" => PageKind::Folder,
                "feed" => PageKind::Feed,
                _ => {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        format!("invalid kind {kind:?} in v_page").into(),
                    ));
                }
            };
            Ok(PageRow {
                page: r.get(0)?,
                kind,
                id: Some(r.get(2)?),
                title: r.get(3)?,
                parent_page: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.extend(stored);
    Ok(rows)
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib store::pages` → `12 passed`; `cargo test -p ressac-core --lib` → `241 passed; 0 failed; 1 ignored`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/store/pages.rs
git commit -m "List every page: reserved pages, then v_page in page order" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Verification gate (Phase 1a)

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green: `ressac` 31, `cli` 13, lib `241 passed; 0 failed; 1 ignored`, `contract` 7, `end_to_end` 4. If `cargo fmt` reports anything, run `cargo fmt --all` and commit `"cargo fmt"` with the trailer. **Stop here and wait for Clément's approval before Phase 1b.**

---

# Phase 1b: `page set`, numbers on import

### Task 1.3: Error variants and `pages::set`

**Files:**
- Modify: `crates/ressac-core/src/error.rs`
- Modify: `crates/ressac-core/src/store/pages.rs`

**Interfaces:**
- Consumes: `RESERVED_PAGES`, `ASSIGNABLE`, `PageKind::as_str`, `assign_missing` (tests); `rusqlite::{OptionalExtension, Transaction, TransactionBehavior}` (`Transaction::new_unchecked(conn, Immediate)`, the way `Store::write_tx` opens its transactions).
- Produces:
  - `Error::PageReserved(u16)` → `Page {0} is reserved.`;
  - `Error::PageOutOfRange(u16)` → `Page {0} is not between 110 and 799.`;
  - `Error::PageTaken { page: u16 }` → `Page {page} is already taken.`;
  - `Error::UnknownPageTarget { kind: &'static str, id: i64 }` → `No {kind} #{id}.`;
  - `pub fn set(conn: &rusqlite::Connection, kind: PageKind, id: i64, page: u16) -> Result<()>`. It checks, in this order: kind (`Reserved` → `Error::Invalid`), reserved page, range, unknown id, then whether any *other* folder or feed holds the page. Nothing is written on a refusal, and giving an item its own page is a no-op `Ok`. It opens its own `IMMEDIATE` transaction, so it must not be called inside another one.

- [ ] **Step 1: Write the failing tests.** Append at the end of `mod tests` in `pages.rs`:

```rust
    /// Root folder 1 (200) holding feeds 10 (201) and 11 (202), subfolder 2 (203).
    fn numbered() -> Store {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        feed(&s, 11, Some(1), 1);
        folder(&s, 2, Some(1), 2);
        assign(&s);
        s
    }

    #[test]
    fn set_renumbers_a_feed_or_a_folder() {
        let s = numbered();
        set(s.conn(), PageKind::Feed, 10, 412).unwrap();
        set(s.conn(), PageKind::Folder, 2, 150).unwrap();
        assert_eq!(feed_page(&s, 10), Some(412));
        assert_eq!(folder_page(&s, 2), Some(150));
        set(s.conn(), PageKind::Feed, 10, 412).unwrap(); // its own page: nothing to do
        set(s.conn(), PageKind::Feed, 11, 201).unwrap(); // 201 was freed above
        assert_eq!(feed_page(&s, 11), Some(201));
        assert_eq!(assign(&s), 0, "a renumbered item is never renumbered again");
    }

    #[test]
    fn set_refuses_reserved_pages() {
        let s = numbered();
        for page in RESERVED_PAGES {
            let err = set(s.conn(), PageKind::Feed, 10, page).unwrap_err();
            assert!(
                matches!(err, Error::PageReserved(p) if p == page),
                "{err:?}"
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 800)
                .unwrap_err()
                .to_string(),
            "Page 800 is reserved."
        );
        assert_eq!(feed_page(&s, 10), Some(201));
    }

    #[test]
    fn set_refuses_pages_outside_110_to_799() {
        let s = numbered();
        for page in [0, 99, 104, 109, 900, u16::MAX] {
            let err = set(s.conn(), PageKind::Feed, 10, page).unwrap_err();
            assert!(
                matches!(err, Error::PageOutOfRange(p) if p == page),
                "{err:?}"
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 900)
                .unwrap_err()
                .to_string(),
            "Page 900 is not between 110 and 799."
        );
        assert_eq!(feed_page(&s, 10), Some(201));
    }

    #[test]
    fn set_refuses_a_page_taken_by_a_feed_or_a_folder() {
        let s = numbered();
        let taken = |kind, id, page| {
            let err = set(s.conn(), kind, id, page).unwrap_err();
            assert!(
                matches!(err, Error::PageTaken { page: p } if p == page),
                "{err:?}"
            );
            err.to_string()
        };
        assert_eq!(taken(PageKind::Feed, 10, 202), "Page 202 is already taken.");
        taken(PageKind::Feed, 10, 203); // a folder's page
        taken(PageKind::Folder, 2, 201); // a feed's page
        taken(PageKind::Folder, 2, 200); // another folder's page
        assert_eq!(feed_page(&s, 10), Some(201));
        assert_eq!(folder_page(&s, 2), Some(203));
    }

    #[test]
    fn set_refuses_an_unknown_id_and_the_reserved_kind() {
        let s = numbered();
        let err = set(s.conn(), PageKind::Feed, 99, 300).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownPageTarget {
                    kind: "feed",
                    id: 99
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "No feed #99.");
        let err = set(s.conn(), PageKind::Folder, 10, 300).unwrap_err();
        assert_eq!(err.to_string(), "No folder #10.", "ids are per table");
        assert!(matches!(
            set(s.conn(), PageKind::Reserved, 1, 300),
            Err(Error::Invalid(_))
        ));
    }
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib store::pages`. Expected: compile errors ``cannot find function `set` in this scope`` (12 times) and ``cannot find type `Error` in this scope`` (5 times).

- [ ] **Step 3: Implement.**

In `crates/ressac-core/src/error.rs`, replace the last variant and the enum's closing brace (`#[error("{0}")] Invalid(String), }`) with:

```rust
    #[error("{0}")]
    Invalid(String),
    #[error("Page {0} is reserved.")]
    PageReserved(u16),
    #[error("Page {0} is not between 110 and 799.")]
    PageOutOfRange(u16),
    #[error("Page {page} is already taken.")]
    PageTaken { page: u16 },
    #[error("No {kind} #{id}.")]
    UnknownPageTarget { kind: &'static str, id: i64 },
}
```

In `pages.rs`, replace the two imports `use rusqlite::{Connection, params};` and `use crate::error::Result;` with:

```rust
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
```

and

```rust
use crate::error::{Error, Result};
```

then add `set` right after `list`, before `#[cfg(test)]`:

```rust
/// Renumbers one folder or feed; `kind` must be Folder or Feed. Refuses a reserved page,
/// a page outside `ASSIGNABLE`, an unknown id and a page held by any other folder or feed;
/// nothing is written then. Giving an item the page it already holds changes nothing.
/// Opens its own write transaction, so it must not run inside another one.
pub fn set(conn: &Connection, kind: PageKind, id: i64, page: u16) -> Result<()> {
    let table = match kind {
        PageKind::Folder => "folder",
        PageKind::Feed => "feed",
        PageKind::Reserved => {
            return Err(Error::Invalid(
                "Only a folder or a feed can be renumbered.".into(),
            ));
        }
    };
    if RESERVED_PAGES.contains(&page) {
        return Err(Error::PageReserved(page));
    }
    if !ASSIGNABLE.contains(&page) {
        return Err(Error::PageOutOfRange(page));
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let current: Option<Option<u16>> = tx
        .query_row(
            &format!("SELECT page FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(current) = current else {
        return Err(Error::UnknownPageTarget {
            kind: kind.as_str(),
            id,
        });
    };
    if current == Some(page) {
        return Ok(());
    }
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM folder WHERE page = ?1) OR EXISTS (SELECT 1 FROM feed WHERE page = ?1)",
        [page],
        |r| r.get(0),
    )?;
    if taken {
        return Err(Error::PageTaken { page });
    }
    tx.execute(
        &format!("UPDATE {table} SET page = ?2 WHERE id = ?1"),
        params![id, page],
    )?;
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib store::pages` → `17 passed`; `cargo test -p ressac-core --lib` → `246 passed; 0 failed; 1 ignored`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/error.rs crates/ressac-core/src/store/pages.rs
git commit -m "Renumber a folder or a feed by hand; refuse reserved, out-of-range, taken pages and unknown ids" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 1.4: Numbers on import, and the fixture's pages

**Files:**
- Modify: `crates/ressac-core/src/store/feeds.rs`
- Modify: `crates/ressac-core/tests/contract.rs`
- Regenerate: `tests/fixtures/contract.sqlite`

**Interfaces:**
- Consumes: `pages::assign_missing` (Task 1.1), through `super::pages`.
- Produces: `Store::import_outlines` numbers every new folder and feed inside its own transaction (a dry run rolls the numbers back with the rest). The committed fixture holds the 18 numbers listed in `contract_fixture_numbers_every_folder_and_feed`, which the Swift phases rely on (look them up by title, never by id).

- [ ] **Step 1: Write the failing tests.** Append at the end of `mod tests` in `crates/ressac-core/src/store/feeds.rs`:

```rust
    fn pages(store: &Store) -> Vec<(String, Option<u16>)> {
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT name, page FROM folder UNION ALL SELECT title, page FROM feed ORDER BY 2, 1",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn import_numbers_new_folders_and_feeds_and_keeps_earlier_numbers() {
        let store = Store::open_in_memory().unwrap();
        let first = r#"<opml><body><outline text="Blogs">
            <outline text="X" xmlUrl="https://x.example.org/feed"/>
            <outline text="Y" xmlUrl="https://y.example.org/feed"/>
            </outline><outline text="Solo" xmlUrl="https://solo.example.org/feed"/></body></opml>"#;
        store
            .import_outlines(&opml::parse(first).unwrap(), false, now())
            .unwrap();
        let x = store.feeds().unwrap()[0].id;
        store
            .record_decision(x, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        let second = r#"<opml><body><outline text="Blogs">
            <outline text="Z" xmlUrl="https://z.example.org/feed"/>
            <outline text="Sub"><outline text="W" xmlUrl="https://w.example.org/feed"/></outline>
            </outline></body></opml>"#;
        store
            .import_outlines(&opml::parse(second).unwrap(), false, now())
            .unwrap();
        let expected: Vec<(String, Option<u16>)> = [
            ("Solo", 110),
            ("Blogs", 200),
            ("X", 201),
            ("Y", 202),
            ("Z", 203),
            ("Sub", 204),
            ("W", 205),
        ]
        .iter()
        .map(|(t, p)| (t.to_string(), Some(*p)))
        .collect();
        assert_eq!(pages(&store), expected, "the buried X keeps 201");
    }

    #[test]
    fn a_dry_run_numbers_nothing() {
        let (store, _) = imported();
        let extra = r#"<opml><body><outline text="New" xmlUrl="https://new.example.org/feed"/></body></opml>"#;
        let before = pages(&store);
        store
            .import_outlines(&opml::parse(extra).unwrap(), true, now())
            .unwrap();
        assert_eq!(pages(&store), before);
    }
```

In `crates/ressac-core/tests/contract.rs`, insert before `/// Regenerates the fixture shared with the Swift tests:`:

```rust
/// Spec §11: page assignment on the contract database. The Swift tests rely on these
/// numbers (looked up by title, never by id).
#[test]
fn contract_fixture_numbers_every_folder_and_feed() {
    let db = fixture();
    let mut stmt = db
        .prepare("SELECT page, kind, title, parent_page FROM v_page ORDER BY page")
        .unwrap();
    let rows: Vec<(u16, String, String, Option<u16>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        (200, "folder", "Blogs", None),
        (201, "feed", "xkcd.com", Some(200)),
        (202, "folder", "potes", Some(200)),
        (203, "feed", "Orange Drenka", Some(202)),
        (204, "feed", "Orange-Drenka", Some(202)),
        (205, "feed", "Photos from bouquiniste", Some(202)),
        (206, "feed", "Girl on the Couch", Some(200)),
        (300, "folder", "Biblis", None),
        (301, "feed", "Bibliobsession 2.0", Some(300)),
        (302, "feed", "Unphotographable", Some(300)),
        (400, "folder", "Infos", None),
        (401, "feed", "Daring Fireball", Some(400)),
        (402, "feed", "Six Colors", Some(400)),
        (403, "folder", "Science", Some(400)),
        (404, "feed", "Center for Humane Technology", Some(403)),
        (500, "folder", "Jon", None),
        (501, "folder", "YouTube", Some(500)),
        (502, "feed", "Center for Humane Technology (1)", Some(501)),
    ]
    .map(|(page, kind, title, parent)| (page, kind.to_string(), title.to_string(), parent));
    assert_eq!(
        rows, expected,
        "every folder and feed, the buried Photos included"
    );
    let state: String = db
        .query_row("SELECT state FROM feed WHERE page = 205", [], |r| r.get(0))
        .unwrap();
    assert_eq!(state, "buried");
}
```

- [ ] **Step 2: Run them.**

`cargo test -p ressac-core --lib store::feeds`. Expected: one failure, `import_numbers_new_folders_and_feeds_and_keeps_earlier_numbers`, whose left side lists every page as `None`. `a_dry_run_numbers_nothing` already passes: it guards the rollback (numbering outside the import transaction, or after the rollback, would make it fail).

`cargo test -p ressac-core --test contract contract_fixture_numbers_every_folder_and_feed`. Expected: failure with `left: []`, since the committed fixture's import did not number anything.

- [ ] **Step 3: Implement.** In `crates/ressac-core/src/store/feeds.rs`, replace the doc comment of `import_outlines` with:

```rust
    /// Imports folders and feeds. Folders are matched by (parent, name). A feed already
    /// exists when any feed, buried or not, has its URL key as current or original address,
    /// so triage (burials, merges, repairs) survives a re-import. New items go after the
    /// existing ones of their folder and get their page numbers in the same transaction.
    /// A dry run executes the same statements and rolls them back.
```

and, in its body, number the new items right after the walk:

```rust
        import_level(&mut ctx, outlines, None)?;
        super::pages::assign_missing(&tx)?;
        if dry_run {
```

- [ ] **Step 4: Run, regenerate, run.**

```bash
cargo test -p ressac-core --lib
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked
cargo test --workspace
sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT count(*) FROM v_page; SELECT page FROM v_page WHERE title = 'Infos'; SELECT state FROM feed WHERE page = 205;"
```

Expected: lib `248 passed; 0 failed; 1 ignored`; the hook passes (`1 passed`); every suite green, `contract` `8 passed`; `sqlite3` prints `18`, `400`, `buried`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/store/feeds.rs crates/ressac-core/tests/contract.rs tests/fixtures/contract.sqlite
git commit -m "Number new folders and feeds at import; contract fixture with page numbers" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Verification gate (Phase 1b)

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green: `ressac` 31, `cli` 13, lib `248 passed; 0 failed; 1 ignored`, `contract` 8, `end_to_end` 4. Then push (Global Constraints). **Stop and wait for Clément's approval before Phase 2.**

### Notes for the executor

- **Verified on 2026-10-14** in a scratch copy of the repository, task by task, with Rust 1.99.0: each task's end state passes `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; the counts above come from that run.
- **The Swift test suite goes red from Task 1.1 until Phase 6.** `apps/macos/RessacTests` copy `tests/fixtures/contract.sqlite` (`TestDatabase` in `DataTests.swift`, `GardenTests`, `ReadingFixRound1Tests`, the migration stubs of `AppStateTests`). While `AppDatabase.knownSchemaVersion` is still `2`, `check(at:)` on the v3 fixture is `.tooNew(3)`, so those tests fail. The Rust gates of Phases 1 and 2 do not build the app; Phase 6 runs right after Phase 1b, before Phase 2 and before any other Swift phase, which closes the gap. Do not touch Swift files here. Phase 6 reads the fixture as Phase 1b regenerates it; Phase 2 leaves it as it is.
- **`pages` is a public module** because `service.rs` sits outside `store` and calls `pages::list` and `pages::set` (Phase 2). Its functions take a `&rusqlite::Connection`; only the crate can reach the store's connection (`Store::conn` is `pub(crate)`).
- **Why two passes:** with a single pass, the 100th feed of a full hundred would take 110 and push the root feeds shown after it to 111 onwards, so a root feed's number would depend on whether some folder overflowed earlier in the walk.
- **No page change outside `page set`.** `assign_missing` only fills NULL pages. Renaming, moving (no such command yet), burying, merging, restoring and undoing never renumber. A future "move feed to folder" command keeps the old number unless it nulls `page` first; that is a later decision.
- **Folder page and its children:** `page set folder` changes only the folder. Its contents keep their numbers, and `v_page.parent_page` follows the folder's new page (Phase 2's CLI test checks it).
- **Duplicates across tables** cannot happen through the core: assignment and `set` check both tables, and each table has its unique index. `list` orders by `page, kind, ref_id`, so even a row written by hand outside the core would list deterministically.
- **The migration on real data:** to try it, copy a v2 database and run `ressac --db <copy> migrate` (or `export`). Afterwards `sqlite3 <copy> "SELECT count(*) FROM folder WHERE page IS NULL; SELECT count(*) FROM feed WHERE page IS NULL"` prints `0` twice, and `<copy>.bak-v2` sits next to it. Never on `~/Library/…`.
- **Fixture ids are not stable** across regenerations in principle; the contract test and the Swift phases look rows up by title.
