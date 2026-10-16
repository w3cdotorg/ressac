# Phase 1: Rhythm groups, schema v4 and the override in the core

**Runs after:** nothing: it is the first phase. Execution order of the sub-project: 1a → 1b → 4a → 1c → 1d → 2a → 2b → 3a → 3b → 4b → 5a → 5b → 5c → 6a → 6b → 6c → 7.

Read the plan index (`docs/plans/2026-10-16-cadence-edition.md`) first. Its Global Constraints apply. This phase implements spec §4 (classification, the `entries_per_month` fix, the override), spec §5 (schema v4, contract views, reserved pages 104–109) and the Rust rows of spec §10 for classification, override, migration and the contract fixture. None of the plan index's five Review Focus items belongs to this phase (they are owned by phases 2a, 2b, 4a, 5a and 6b).

## Four sub-phases (deviation from the plan index)

The plan index lists 1a, 1b and 1c. Against the real code, the 14 reserved pages cannot ride with 1b or 1c without breaking the 5-file rule:

- the reserved list is pinned by `store/pages.rs` (unit tests), `service.rs` (`pages_and_set_page_go_through_the_store` skips 8 rows) and `crates/ressac/tests/cli.rs` (`page_list_prints_reserved_pages_then_every_folder_and_feed` lists them and counts 26 JSON lines);
- 1b already needs `crates/ressac/tests/cli.rs` for `Schema version 4.`, and 1c needs `store/mod.rs` to re-export `CadenceRow`.

So the reserved pages become **Phase 1d** (3 files), run right after 1c. 1b's migration still frees pages 104–109 (plain SQL in `store/mod.rs`, no constant needed: nothing can hold them anyway, since `ASSIGNABLE` starts at 110). Other consequences:

- **`health/mod.rs` joins 1a**: `classify` must be re-exported for the store (`pub use cadence::cadence;` only exports `cadence` today).
- **`store/health.rs` is touched twice**: in 1a only to fill the new `Cadence.group` field (no column yet, the read gives the default), in 1b to store and read it.
- **`Store::cadence_row` is a new public name** (1c): `Ressac::set_cadence` needs to re-read one feed, including a feed that has no `feed_health` row yet, which `v_cadence` (an inner join) cannot return.
- **The contract fixture gets rhythm and one edition in 1b**, written with plain SQL where the core has no writer yet (the override, the edition rows), so that Phase 4a's Swift tests have data. Phase 2b may replace the hand-written edition with `write_edition`; the rows the fixture test pins must stay the same.

**Phase 1a files (4):**
- Modify: `crates/ressac-core/src/model.rs` (`CadenceGroup`, `Cadence.group`, tests)
- Modify: `crates/ressac-core/src/health/cadence.rs` (thresholds, `classify`, `entries_per_month`, tests)
- Modify: `crates/ressac-core/src/health/mod.rs` (re-export)
- Modify: `crates/ressac-core/src/store/health.rs` (fill `group` on read, test sample)

**Phase 1b files (5):**
- Create: `schema/0004_edition.sql`
- Modify: `crates/ressac-core/src/store/mod.rs` (version 4, migration entry, v4 hook, tests)
- Modify: `crates/ressac-core/src/store/health.rs` (`cadence_group` stored and read, backfill, tests)
- Modify: `crates/ressac-core/tests/contract.rs` (version 4, columns, fixture rhythm and edition)
- Modify: `crates/ressac/tests/cli.rs` (`Schema version 4.`)
- Regenerated binary asset, not counted: `tests/fixtures/contract.sqlite`

**Phase 1c files (4):**
- Modify: `crates/ressac-core/src/error.rs` (`UnknownFeed`, `FeedBuried`, and `EditionNotDue` for Phase 2b, which has no file left for it)
- Modify: `crates/ressac-core/src/store/feeds.rs` (`CadenceRow`, `set_cadence_override`, `cadence_row`, tests)
- Modify: `crates/ressac-core/src/store/mod.rs` (re-export `CadenceRow`)
- Modify: `crates/ressac-core/src/service.rs` (`Ressac::set_cadence`, test)

**Phase 1d files (3):**
- Modify: `crates/ressac-core/src/store/pages.rs` (14 reserved pages, tests)
- Modify: `crates/ressac-core/src/service.rs` (test skips `RESERVED_PAGES.len()` rows)
- Modify: `crates/ressac/tests/cli.rs` (`page list` expectations, `page set … 109` now reserved)

**Step 0 rule:** every change here is additive (a type, constants, functions, one migration entry, one hook, tests). Files over 300 lines touched: `store/mod.rs` (467), `store/health.rs` (363), `store/pages.rs` (788), `store/feeds.rs` (577), `service.rs` (1504), `model.rs` (336), `contract.rs` (601), `cli.rs`. None is restructured. The plan index nevertheless asks for an explicit check of `store/mod.rs`; Task 1b.0 does it. If any of these files was restructured since 2026-10-16, apply the rule to it.

**Builds on (existing code, signatures unchanged):**
- `health::cadence::cadence(entries: &[EntryRecord], now: DateTime<Utc>) -> Cadence`: dates of non-fallback entries (all entries when none), newest first, intervals over the `INTERVAL_WINDOW = 21` most recent, `median`.
- `model::Cadence { last_entry_at, median_interval_days, entries_per_month }` (derives `Default`, `Serialize`, `Deserialize`); `model::{ts, parse_ts}`; the `Status::{ALL, as_str, parse}` pattern that `CadenceGroup` copies.
- `Store::{open, open_in_memory, schema_version, conn, write_tx}`. The private `Store::migrate` takes a `VACUUM INTO <file>.bak-v<old>` backup, then runs each pending entry of `MIGRATIONS` in one `IMMEDIATE` transaction, re-reading the version under the lock; after migration 2 it runs `reading::backfill_fts(&tx)`, after migration 3 `pages::assign_missing(&tx)`. The v4 hook follows that pattern.
- `Store::save_diagnoses(&self, diags: &HashMap<FeedId, Diagnosis>, now) -> Result<()>`, `Store::diagnosis(id)`, `Store::diagnoses()`, the private `health_row` and `unknown(col, what, value)` helpers of `store/health.rs`.
- `pages::assign_missing(conn: &Connection) -> Result<usize>`, `pages::list`, `pages::set` (checks `RESERVED_PAGES` before `ASSIGNABLE`, so a reserved page under 110 reads `Page N is reserved.`).
- Contract test machinery in `crates/ressac-core/tests/contract.rs`: `live_store()`, `columns(conn, table)`, `fixture()`, `fill_contract_store(path)`, `diagnosis(status, clue, action)`, `now()` = `2026-10-10T08:00:00Z`, and the regeneration switch `RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked`.
- CLI test helpers in `crates/ressac/tests/cli.rs`: `ressac(home, args)`, `stdout`, `json_lines`, `FIXTURE`.

**Produces for later phases (exact):**
- `ressac_core::model::CadenceGroup` (`ALL`, `as_str`, `parse`, serde lowercase, default `Silent`); `Cadence.group`.
- `ressac_core::health::{classify, SILENT_AFTER_DAYS, DAILY_BELOW_DAYS, WEEKLY_BELOW_DAYS, MONTHLY_BELOW_DAYS}`.
- Schema v4 exactly as in the plan index (tables, `v_cadence`, `v_edition_today`, `v_edition_overflow`, index `entry_fetched`).
- `ressac_core::store::CadenceRow`, `Store::set_cadence_override`, `Store::cadence_row`, `Ressac::set_cadence`, `Error::UnknownFeed`, `Error::FeedBuried`, `Error::EditionNotDue` (used from Phase 2b on).
- `pages::RESERVED_PAGES: [u16; 14]` with titles `Journal`, `Daily`, `Weekly`, `Monthly`, `Rare`, `Silent` for 104–109.
- **Contract fixture facts** (Phase 4a relies on them, by title):

  | Feed (page) | `effective_group` | `computed_group` | `is_override` |
  |---|---|---|---|
  | xkcd.com (201) | daily | daily | 0 |
  | Orange Drenka (203) | silent | silent | 0 |
  | Girl on the Couch (206) | monthly | monthly | 0 |
  | Bibliobsession 2.0 (301) | weekly | rare | 1 |
  | Unphotographable (302) | silent | silent | 0 |
  | Daring Fireball (401) | weekly | weekly | 0 |

  `v_cadence` has exactly these 6 rows (diagnosed active feeds; Photos from bouquiniste is buried, the others have no diagnosis). xkcd has 3 unread entries. One edition: `id 1`, `built_at 2026-10-10T08:00:00.000Z`, `window_start 2026-10-09T08:00:00.000Z`, `window_end 2026-10-10T08:00:00.000Z`, `unreachable 1`; entries by rank: 1 `week` Daring Fireball `Linked: Tides` (guid `df-extracted`), 2 `week` Daring Fireball `Walled` (`df-walled`), 3 `wire` xkcd.com `Recent comic` (`x-recent`); one overflow: `wire`, xkcd.com, page 201, `hidden_count 2`. No `front` entry (the "section omitted" case).

---

# Phase 1a: `CadenceGroup` and the classification

Run the quota check of the plan's execution rules before starting (`~/.cache/claude-usage.json`, `seven_day.percentUsed`).

### Task 1a.1: `CadenceGroup` in the model

**Files:**
- Modify: `crates/ressac-core/src/model.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub enum CadenceGroup { Daily, Weekly, Monthly, Rare, Silent }` with `ALL`, `as_str`, `parse`; `Cadence.group: CadenceGroup`.

- [ ] **Step 1: Write the failing tests.** In `model.rs`, `mod tests`, append after `status_round_trips_through_text`:

```rust
    #[test]
    fn cadence_group_round_trips_through_text_and_json() {
        for g in CadenceGroup::ALL {
            assert_eq!(CadenceGroup::parse(g.as_str()), Some(g));
            assert_eq!(
                serde_json::to_value(g).unwrap(),
                serde_json::Value::String(g.as_str().to_string())
            );
        }
        assert_eq!(
            CadenceGroup::ALL.map(CadenceGroup::as_str),
            ["daily", "weekly", "monthly", "rare", "silent"]
        );
        assert_eq!(CadenceGroup::parse("Daily"), None, "lowercase only");
        assert_eq!(CadenceGroup::default(), CadenceGroup::Silent);
    }

    #[test]
    fn a_cadence_without_a_group_reads_as_silent() {
        let c: Cadence =
            serde_json::from_str(r#"{"last_entry_at":null,"median_interval_days":null,"entries_per_month":0.0}"#)
                .unwrap();
        assert_eq!(c.group, CadenceGroup::Silent);
        assert_eq!(Cadence::default().group, CadenceGroup::Silent);
    }
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib model::`. Expected: compile error ``failed to resolve: use of undeclared type `CadenceGroup` `` and ``no field `group` on type `Cadence` ``.

- [ ] **Step 3: Implement.** In `model.rs`, insert right before `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] pub struct Cadence`:

```rust
/// A feed's publication rhythm (spec §4), from the median gap between its recent entries.
/// `Silent` is the default: no entry at all, or none for a year.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CadenceGroup {
    Daily,
    Weekly,
    Monthly,
    Rare,
    #[default]
    Silent,
}

impl CadenceGroup {
    /// Display order of the sidebar's Rhythm section and of pages 105 to 109.
    pub const ALL: [CadenceGroup; 5] = [
        CadenceGroup::Daily,
        CadenceGroup::Weekly,
        CadenceGroup::Monthly,
        CadenceGroup::Rare,
        CadenceGroup::Silent,
    ];

    /// The stored and printed name: `feed_health.cadence_group`, `feed.cadence_override`,
    /// the CLI's arguments and JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            CadenceGroup::Daily => "daily",
            CadenceGroup::Weekly => "weekly",
            CadenceGroup::Monthly => "monthly",
            CadenceGroup::Rare => "rare",
            CadenceGroup::Silent => "silent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == s)
    }
}
```

Then replace the `Cadence` struct with:

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cadence {
    pub last_entry_at: Option<DateTime<Utc>>,
    pub median_interval_days: Option<f64>,
    pub entries_per_month: f64,
    /// Computed by `health::classify`; the user's override lives on `feed`, not here.
    #[serde(default)]
    pub group: CadenceGroup,
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib model::`. Expected: the crate does not compile yet: ``missing field `group` in initializer of `Cadence` `` in `health/cadence.rs` and `store/health.rs` (twice). Task 1a.2 fixes both; do not commit yet.

### Task 1a.2: `classify`, `entries_per_month` over the covered span

**Files:**
- Modify: `crates/ressac-core/src/health/cadence.rs`
- Modify: `crates/ressac-core/src/health/mod.rs`
- Modify: `crates/ressac-core/src/store/health.rs`

**Interfaces:**
- Consumes: `CadenceGroup` (Task 1a.1).
- Produces: `pub fn classify(last_entry_at: Option<DateTime<Utc>>, median_interval_days: Option<f64>, now: DateTime<Utc>) -> CadenceGroup`; `pub const SILENT_AFTER_DAYS: i64 = 365; DAILY_BELOW_DAYS: f64 = 1.5; WEEKLY_BELOW_DAYS: f64 = 10.0; MONTHLY_BELOW_DAYS: f64 = 45.0`, re-exported from `crate::health`.

- [ ] **Step 1: Write the failing tests.** In `health/cadence.rs`, `mod tests`, replace `computes_last_entry_median_interval_and_monthly_rate` and `uses_fallback_dates_when_nothing_else_exists` with the versions below, and append the new tests at the end of the module:

```rust
    #[test]
    fn computes_last_entry_median_interval_monthly_rate_and_group() {
        let c = cadence(
            &[
                entry(0, false),
                entry(10, false),
                entry(30, false),
                entry(400, false),
            ],
            now(),
        );
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, Some(20.0)); // intervals 10, 20, 370
        // 3 entries in the last 365 days, the oldest 30 days ago: under a month, so one month.
        assert_eq!(c.entries_per_month, 3.0);
        assert_eq!(c.group, CadenceGroup::Monthly);
    }

    #[test]
    fn uses_fallback_dates_when_nothing_else_exists() {
        let c = cadence(&[entry(0, true)], now());
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, None);
        assert_eq!(c.group, CadenceGroup::Rare, "one entry this year, no gap to measure");
    }

    fn at(days_ago: i64) -> Option<DateTime<Utc>> {
        Some(now() - Duration::days(days_ago))
    }

    #[test]
    fn median_thresholds_pick_the_group_first_match_wins() {
        let recent = Some(now() - Duration::hours(1));
        for (median, group) in [
            (0.0, CadenceGroup::Daily),
            (1.4999, CadenceGroup::Daily),
            (1.5, CadenceGroup::Weekly),
            (9.999, CadenceGroup::Weekly),
            (10.0, CadenceGroup::Monthly),
            (44.999, CadenceGroup::Monthly),
            (45.0, CadenceGroup::Rare),
            (400.0, CadenceGroup::Rare),
        ] {
            assert_eq!(classify(recent, Some(median), now()), group, "median {median}");
        }
        assert_eq!(classify(recent, None, now()), CadenceGroup::Rare);
    }

    #[test]
    fn silent_means_no_entry_or_none_for_a_year() {
        assert_eq!(classify(None, None, now()), CadenceGroup::Silent);
        assert_eq!(classify(None, Some(0.5), now()), CadenceGroup::Silent);
        // Exactly 365 days is still this year; one second more is silence, whatever the median.
        assert_eq!(classify(at(365), None, now()), CadenceGroup::Rare);
        let just_over = Some(now() - Duration::days(365) - Duration::seconds(1));
        assert_eq!(classify(just_over, Some(0.5), now()), CadenceGroup::Silent);
    }

    #[test]
    fn batch_posting_is_daily() {
        // Five photos uploaded at once, then one the day before: median gap 0.
        let mut entries: Vec<EntryRecord> = (0..5)
            .map(|i| EntryRecord {
                guid: format!("p{i}"),
                ..entry(0, false)
            })
            .collect();
        entries.push(entry(1, false));
        let c = cadence(&entries, now());
        assert_eq!(c.median_interval_days, Some(0.0));
        assert_eq!(c.group, CadenceGroup::Daily);
    }

    #[test]
    fn gaps_are_counted_in_utc_seconds_across_a_dst_change() {
        // Europe/Paris leaves summer time on 2026-10-25; the gap stays exactly one day.
        let e = |ts: &str| EntryRecord {
            published_at: ts.parse().unwrap(),
            ..entry(0, false)
        };
        let c = cadence(
            &[e("2026-10-24T23:30:00Z"), e("2026-10-25T23:30:00Z")],
            "2026-10-26T08:00:00Z".parse().unwrap(),
        );
        assert_eq!(c.median_interval_days, Some(1.0));
        assert_eq!(c.group, CadenceGroup::Daily);
    }

    #[test]
    fn a_feed_imported_recently_is_not_divided_by_twelve() {
        // 20 entries over the last 10 days: the span is under a month, so one month.
        let fresh: Vec<EntryRecord> = (0..20)
            .map(|i| EntryRecord {
                guid: format!("f{i}"),
                published_at: now() - Duration::hours(12 * i),
                ..entry(0, false)
            })
            .collect();
        assert_eq!(cadence(&fresh, now()).entries_per_month, 20.0);
        // 12 entries every 10 days, the oldest 110 days ago: 12 over 110 / 30.4375 months.
        let spread: Vec<EntryRecord> = (0..12).map(|i| entry(10 * i, false)).collect();
        let rate = cadence(&spread, now()).entries_per_month;
        assert!((rate - 12.0 / (110.0 / 30.4375)).abs() < 1e-9, "{rate}");
        // Nothing this year: no rate.
        assert_eq!(cadence(&[entry(400, false)], now()).entries_per_month, 0.0);
    }

    #[test]
    fn an_old_feed_is_silent_even_with_a_tight_median() {
        let c = cadence(
            &[entry(400, false), entry(401, false), entry(402, false)],
            now(),
        );
        assert_eq!(c.median_interval_days, Some(1.0));
        assert_eq!(c.group, CadenceGroup::Silent);
    }
```

In `store/health.rs`, `mod tests`, change the `use` line to `use crate::model::{Action, Cadence, CadenceGroup, Evidence, Remark, Status};` and give `sample()`'s cadence its group:

```rust
            cadence: Cadence {
                last_entry_at: Some(now()),
                median_interval_days: Some(7.0),
                entries_per_month: 1.5,
                // Not stored before schema v4: reads back as the default.
                group: CadenceGroup::Silent,
            },
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib health::cadence`. Expected: compile errors ``cannot find function `classify` in this scope`` and ``cannot find type `CadenceGroup` in this scope``, plus the two ``missing field `group` `` errors from Task 1a.1.

- [ ] **Step 3: Implement.** Replace the top of `health/cadence.rs` (everything above `fn median`) with:

```rust
//! Publication rhythm of a feed, from its stored entries.

use chrono::{DateTime, Duration, Utc};

use crate::model::{Cadence, CadenceGroup};
use crate::store::EntryRecord;

/// Intervals are measured over at most this many most recent entries.
const INTERVAL_WINDOW: usize = 21;

/// A feed whose latest entry is older than this is Silent (spec §4).
pub const SILENT_AFTER_DAYS: i64 = 365;
/// Median gap (days) under which a feed is Daily; then Weekly, then Monthly; Rare above.
pub const DAILY_BELOW_DAYS: f64 = 1.5;
pub const WEEKLY_BELOW_DAYS: f64 = 10.0;
pub const MONTHLY_BELOW_DAYS: f64 = 45.0;

/// Mean length of a month in days, for `entries_per_month`.
const DAYS_PER_MONTH: f64 = 30.4375;

pub fn cadence(entries: &[EntryRecord], now: DateTime<Utc>) -> Cadence {
    let mut dates: Vec<DateTime<Utc>> = entries
        .iter()
        .filter(|e| !e.is_fallback)
        .map(|e| e.published_at)
        .collect();
    if dates.is_empty() {
        dates = entries.iter().map(|e| e.published_at).collect();
    }
    dates.sort_unstable_by(|a, b| b.cmp(a));
    let intervals: Vec<f64> = dates
        .iter()
        .take(INTERVAL_WINDOW)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (*w[0] - *w[1]).num_seconds() as f64 / 86_400.0)
        .collect();
    let last_entry_at = dates.first().copied();
    let median_interval_days = median(intervals);
    Cadence {
        last_entry_at,
        median_interval_days,
        entries_per_month: entries_per_month(&dates, now),
        group: classify(last_entry_at, median_interval_days, now),
    }
}

/// The rhythm group, first match wins: Silent (no entry, or the latest older than
/// `SILENT_AFTER_DAYS`), then Daily, Weekly, Monthly by median gap, else Rare (a gap of
/// 45 days or more, or no gap at all: a single entry this year).
pub fn classify(
    last_entry_at: Option<DateTime<Utc>>,
    median_interval_days: Option<f64>,
    now: DateTime<Utc>,
) -> CadenceGroup {
    let Some(last) = last_entry_at else {
        return CadenceGroup::Silent;
    };
    if now - last > Duration::days(SILENT_AFTER_DAYS) {
        return CadenceGroup::Silent;
    }
    match median_interval_days {
        Some(m) if m < DAILY_BELOW_DAYS => CadenceGroup::Daily,
        Some(m) if m < WEEKLY_BELOW_DAYS => CadenceGroup::Weekly,
        Some(m) if m < MONTHLY_BELOW_DAYS => CadenceGroup::Monthly,
        _ => CadenceGroup::Rare,
    }
}

/// Entries of the last 365 days over the months they cover, from the oldest of them to
/// `now` and at least one month. Dividing by twelve made a feed imported last week, which
/// only exposes its 20 latest items, look like it published under two a month.
fn entries_per_month(dates_newest_first: &[DateTime<Utc>], now: DateTime<Utc>) -> f64 {
    let year_ago = now - Duration::days(365);
    let recent: Vec<&DateTime<Utc>> = dates_newest_first
        .iter()
        .filter(|d| **d >= year_ago)
        .collect();
    let Some(oldest) = recent.last() else {
        return 0.0;
    };
    let months = ((now - **oldest).num_seconds() as f64 / 86_400.0 / DAYS_PER_MONTH).max(1.0);
    recent.len() as f64 / months
}
```

In `health/mod.rs`, replace `pub use cadence::cadence;` with:

```rust
pub use cadence::{
    DAILY_BELOW_DAYS, MONTHLY_BELOW_DAYS, SILENT_AFTER_DAYS, WEEKLY_BELOW_DAYS, cadence, classify,
};
```

In `store/health.rs`, change the import to `use crate::model::{Cadence, CadenceGroup, Decision, Diagnosis, FeedId, Status, parse_ts, ts};` and, in `health_row`, the cadence to:

```rust
            cadence: Cadence {
                last_entry_at: last_entry.as_deref().and_then(parse_ts),
                median_interval_days: r.get(8)?,
                entries_per_month: r.get(9)?,
                // Stored from schema v4 (phase 1b).
                group: CadenceGroup::default(),
            },
```

- [ ] **Step 4: Run the gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all   # the plan's code blocks are not guaranteed rustfmt-exact; format, then check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass, 0 failed. The health verdict's bullet (`About {:.1} entries per month over the last year.`) now prints the new rate; no test pins that number (checked on 2026-10-16: `grep -rn "per month" crates` only finds the format string).

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac-core/src/model.rs crates/ressac-core/src/health/cadence.rs crates/ressac-core/src/health/mod.rs crates/ressac-core/src/store/health.rs
git commit -m "Core: rhythm groups from the median gap, entries per month over the covered span

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Phase 1a gate:** the commands of Step 4. Stop for Clément's approval.

---

# Phase 1b: Schema v4

Run the quota check before starting.

### Task 1b.0: Step 0 check on `store/mod.rs`

`store/mod.rs` has 467 lines and gains a migration entry, a hook and tests.

- [ ] **Step 1: Look for dead code.**

```bash
wc -l crates/ressac-core/src/store/mod.rs
grep -n "dbg!\|println!\|eprintln!\|allow(dead_code)\|allow(unused" crates/ressac-core/src/store/mod.rs
```

Expected on 2026-10-16: no match (`search_ids` is `#[cfg(test)]` and used by the v1 migration test; unused imports would already fail clippy `-D warnings`). Nothing to remove, no commit. If the grep finds something, remove it, run the gate and commit it alone as `Store: remove dead code before schema v4` with the Co-Authored-By line.

### Task 1b.1: Migration 0004, `cadence_group` stored

**Files:**
- Create: `schema/0004_edition.sql`
- Modify: `crates/ressac-core/src/store/mod.rs`
- Modify: `crates/ressac-core/src/store/health.rs`
- Modify: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: `health::classify` (1a).
- Produces: schema v4, `SCHEMA_VERSION = 4`; `pub(super) fn health::backfill_cadence_groups(conn: &Connection, now: DateTime<Utc>) -> Result<usize>` (store-internal).

- [ ] **Step 1: Write the failing tests.**

In `store/health.rs`, `mod tests`, change `sample()`'s group to a stored one and replace its comment:

```rust
                group: CadenceGroup::Weekly,
```

(the line `// Not stored before schema v4: reads back as the default.` goes away), then append at the end of the module:

```rust
    #[test]
    fn unknown_cadence_group_is_an_error() {
        let s = store();
        s.save_diagnoses(&HashMap::from([(1, sample())]), now())
            .unwrap();
        let stored: String = s
            .conn()
            .query_row(
                "SELECT cadence_group FROM feed_health WHERE feed_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, "weekly");
        s.conn()
            .execute(
                "UPDATE feed_health SET cadence_group = 'hourly' WHERE feed_id = 1",
                [],
            )
            .unwrap();
        assert!(s.diagnosis(1).is_err());
    }

    #[test]
    fn backfill_classifies_every_stored_diagnosis_without_fetching() {
        let s = store();
        s.conn()
            .execute_batch(&format!(
                "INSERT INTO feed_health (feed_id, status, reason, last_entry_at, median_interval_days, computed_at) VALUES \
                 (1, 'Alive', 'r', '{recent}', 0.5, '{at}'), \
                 (2, 'Fossil', 'r', '2019-01-01T00:00:00.000Z', 3.0, '{at}');",
                recent = ts(now() - chrono::Duration::days(1)),
                at = ts(now()),
            ))
            .unwrap();
        assert_eq!(backfill_cadence_groups(s.conn(), now()).unwrap(), 2);
        let groups: Vec<String> = s
            .conn()
            .prepare("SELECT cadence_group FROM feed_health ORDER BY feed_id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(groups, ["daily", "silent"]);
    }
```

In `store/mod.rs`, `mod tests`:

1. Rename `fresh_store_has_schema_v3` to `fresh_store_has_schema_v4`, change its first assertion to `assert_eq!(store.schema_version().unwrap(), 4);` and add these names to its `expected` list (keep it alphabetical): `"edition"`, `"edition_entry"`, `"edition_overflow"` after `"entry"`…`"entry_fulltext"` as sorting puts them (`edition` < `entry`), and `"v_cadence"`, `"v_edition_overflow"`, `"v_edition_today"` among the views. The list becomes:

```rust
        for expected in [
            "edition",
            "edition_entry",
            "edition_overflow",
            "entry",
            "entry_fts",
            "entry_fulltext",
            "feed",
            "feed_health",
            "fetch_log",
            "folder",
            "garden_decision",
            "schema_meta",
            "v_cadence",
            "v_edition_overflow",
            "v_edition_today",
            "v_entry",
            "v_feed_overview",
            "v_folder_path",
            "v_garden_row",
            "v_page",
            "v_unread_count",
        ] {
```

2. Every other `schema_version().unwrap(), 3)` in the module becomes `schema_version().unwrap(), SCHEMA_VERSION)` (six tests: `reopening_a_file_is_idempotent_and_uses_wal`, `migration_from_v1_backfills_fts_and_keeps_read_state`, `migration_survives_an_existing_backup_and_reopening` twice, `a_failing_migration_leaves_v1_untouched_and_is_retryable`, `migration_from_v2_numbers_every_folder_and_feed`, `a_failing_v3_migration_leaves_v2_untouched`):

```bash
sed -i '' 's/schema_version().unwrap(), 3)/schema_version().unwrap(), SCHEMA_VERSION)/' crates/ressac-core/src/store/mod.rs
grep -n "schema_version().unwrap(), 3)" crates/ressac-core/src/store/mod.rs   # expected: no output
```

3. Append at the end of the module:

```rust
    /// A v3 database: `v2_database` numbered, a feed squatting page 105 (now reserved), two
    /// stored diagnoses (a daily feed, a fossil) and the buried feed without one.
    fn v3_database(path: &Path) -> Connection {
        let v3 = v2_database(path);
        v3.execute_batch(MIGRATIONS[2].1).unwrap();
        v3.execute_batch(&format!(
            "UPDATE folder SET page = 200 WHERE id = 1; UPDATE folder SET page = 202 WHERE id = 2;
             UPDATE feed SET page = 105 WHERE id = 1; UPDATE feed SET page = 201 WHERE id = 2;
             UPDATE feed SET page = 203 WHERE id = 3;
             INSERT INTO feed_health (feed_id, status, reason, last_entry_at, median_interval_days, computed_at) VALUES \
             (1, 'Alive', 'r', '{recent}', 0.5, '2026-10-14T08:00:00.000Z'), \
             (2, 'Fossil', 'r', '2019-01-01T00:00:00.000Z', 3.0, '2026-10-14T08:00:00.000Z');",
            recent = crate::model::ts(chrono::Utc::now() - chrono::Duration::days(1)),
        ))
        .unwrap();
        v3
    }

    #[test]
    fn migration_from_v3_frees_pages_104_to_109_and_classifies_every_feed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        drop(v3_database(&path));
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 4);
        assert!(path.with_extension("sqlite.bak-v3").exists());
        let page: u16 = store
            .conn()
            .query_row("SELECT page FROM feed WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(page, 110, "the root feed leaves 105 for the first root-feed number");
        let groups: Vec<(i64, String)> = store
            .conn()
            .prepare("SELECT feed_id, cadence_group FROM feed_health ORDER BY feed_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            groups,
            [(1, "daily".to_string()), (2, "silent".to_string())]
        );
        let overrides: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM feed WHERE cadence_override IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(overrides, 0);
    }

    #[test]
    fn a_failing_v4_migration_leaves_v3_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ressac.sqlite");
        {
            let v3 = v3_database(&path);
            // Poison: 0004 creates this table after its ALTERs have already run.
            v3.execute_batch("CREATE TABLE edition (x)").unwrap();
        }
        assert!(Store::open(&path).is_err());
        let raw = Connection::open(&path).unwrap();
        let version: i64 = raw
            .query_row("SELECT version FROM schema_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
        let columns: i64 = raw
            .query_row(
                "SELECT (SELECT count(*) FROM pragma_table_info('feed') WHERE name = 'cadence_override') \
                 + (SELECT count(*) FROM pragma_table_info('feed_health') WHERE name = 'cadence_group')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 0, "ALTERs rolled back");
        let page: u16 = raw
            .query_row("SELECT page FROM feed WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(page, 105, "the page move rolled back too");
        assert!(path.with_extension("sqlite.bak-v3").exists());
        raw.execute_batch("DROP TABLE edition").unwrap();
        drop(raw);
        assert_eq!(Store::open(&path).unwrap().schema_version().unwrap(), 4);
    }
```

In `crates/ressac/tests/cli.rs`, `migrate_reports_the_schema_version`: `assert_eq!(stdout(&text), "Schema version 4.\n");`.

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib store::`. Expected: compile error ``cannot find function `backfill_cadence_groups` in this scope``. (Once that compiles, the new store tests fail on version `3` and missing tables.)

- [ ] **Step 3: Implement.**

Create `schema/0004_edition.sql`:

```sql
-- Ressac schema v4: rhythm groups and the morning edition (sub-project 3).
-- The core writes every column and table below; the apps read them through the v_ views.
ALTER TABLE feed ADD COLUMN cadence_override TEXT
  CHECK (cadence_override IN ('daily', 'weekly', 'monthly', 'rare', 'silent'));
ALTER TABLE feed_health ADD COLUMN cadence_group TEXT NOT NULL DEFAULT 'silent';

CREATE INDEX entry_fetched ON entry(fetched_at);

CREATE TABLE edition (
  id           INTEGER PRIMARY KEY,
  built_at     TEXT NOT NULL,
  window_start TEXT NOT NULL,
  window_end   TEXT NOT NULL,
  unreachable  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE edition_entry (
  edition_id INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  entry_id   INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
  section    TEXT NOT NULL CHECK (section IN ('front', 'week', 'wire')),
  rank       INTEGER NOT NULL,
  PRIMARY KEY (edition_id, entry_id)
);

CREATE TABLE edition_overflow (
  edition_id   INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  feed_id      INTEGER NOT NULL REFERENCES feed(id) ON DELETE CASCADE,
  section      TEXT NOT NULL CHECK (section IN ('front', 'week', 'wire')),
  hidden_count INTEGER NOT NULL,
  PRIMARY KEY (edition_id, feed_id)
);

-- Contract view: the rhythm of every active feed with a diagnosis.
CREATE VIEW v_cadence AS
  SELECT d.id AS feed_id, d.title, d.page, p.path AS folder_path,
         COALESCE(d.cadence_override, h.cadence_group) AS effective_group,
         h.cadence_group AS computed_group,
         d.cadence_override IS NOT NULL AS is_override,
         h.median_interval_days, h.entries_per_month, h.last_entry_at,
         (SELECT COUNT(*) FROM entry e WHERE e.feed_id = d.id AND e.read_at IS NULL) AS unread
  FROM feed d JOIN feed_health h ON h.feed_id = d.id
  LEFT JOIN v_folder_path p ON p.folder_id = d.folder_id
  WHERE d.state = 'active';

-- Contract view: the latest edition, one row per entry, with live read and starred state.
-- Frozen: a feed buried after the build keeps its entries here.
CREATE VIEW v_edition_today AS
  SELECT ed.id AS edition_id, ed.built_at, ed.window_start, ed.window_end, ed.unreachable,
         ee.entry_id, ee.section, ee.rank, e.feed_id, d.title AS feed_title,
         e.title, e.published_at, e.read_at, e.starred_at
  FROM edition ed
  JOIN edition_entry ee ON ee.edition_id = ed.id
  JOIN entry e ON e.id = ee.entry_id
  JOIN feed d ON d.id = e.feed_id
  WHERE ed.id = (SELECT MAX(id) FROM edition);

-- Contract view: what the caps of the latest edition left in each feed.
CREATE VIEW v_edition_overflow AS
  SELECT o.edition_id, o.section, o.feed_id, d.title AS feed_title, d.page, o.hidden_count
  FROM edition_overflow o JOIN feed d ON d.id = o.feed_id
  WHERE o.edition_id = (SELECT MAX(id) FROM edition);

UPDATE schema_meta SET version = 4;
```

In `store/mod.rs`:

```rust
/// Highest schema version this binary knows.
pub const SCHEMA_VERSION: i64 = 4;

/// Ordered migrations, embedded at compile time from the repository's `schema/` folder.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../schema/0001_init.sql")),
    (2, include_str!("../../../../schema/0002_reading.sql")),
    (3, include_str!("../../../../schema/0003_pages.sql")),
    (4, include_str!("../../../../schema/0004_edition.sql")),
];
```

after `const PAGES_MIGRATION: i64 = 3;` add:

```rust
/// The migration after which pages 104 to 109 are reserved and stored diagnoses get their
/// rhythm group.
const CADENCE_MIGRATION: i64 = 4;
```

and in `migrate`, after the `PAGES_MIGRATION` block:

```rust
            if *version == CADENCE_MIGRATION {
                // Pages 104–109 become reserved (Journal and the rhythm groups). Nothing can
                // hold them (`ASSIGNABLE` starts at 110); a hand-edited database still gets
                // its squatter renumbered.
                tx.execute_batch(
                    "UPDATE folder SET page = NULL WHERE page BETWEEN 104 AND 109;
                     UPDATE feed SET page = NULL WHERE page BETWEEN 104 AND 109;",
                )?;
                pages::assign_missing(&tx)?;
                health::backfill_cadence_groups(&tx, chrono::Utc::now())?;
            }
```

In `store/health.rs`:

```rust
use crate::health::classify;
```

(next to the other `use crate::…` lines), then:

```rust
const HEALTH_COLUMNS: &str = "feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
     last_entry_at, median_interval_days, entries_per_month, cadence_group";
```

`health_row`: read the group after `let last_entry…`:

```rust
    let group: String = r.get(10)?;
```

and build the cadence as:

```rust
            cadence: Cadence {
                last_entry_at: last_entry.as_deref().and_then(parse_ts),
                median_interval_days: r.get(8)?,
                entries_per_month: r.get(9)?,
                group: CadenceGroup::parse(&group).ok_or_else(|| unknown(10, "rhythm group", &group))?,
            },
```

`save_diagnoses`: replace the statement and its parameters with:

```rust
            let mut stmt = tx.prepare(
                "INSERT INTO feed_health (feed_id, status, reason, evidence, remarks, proposed_action, hijack_score, \
                 last_entry_at, median_interval_days, entries_per_month, computed_at, cadence_group) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                 ON CONFLICT(feed_id) DO UPDATE SET status = excluded.status, reason = excluded.reason, \
                 evidence = excluded.evidence, remarks = excluded.remarks, proposed_action = excluded.proposed_action, \
                 hijack_score = excluded.hijack_score, last_entry_at = excluded.last_entry_at, \
                 median_interval_days = excluded.median_interval_days, entries_per_month = excluded.entries_per_month, \
                 computed_at = excluded.computed_at, cadence_group = excluded.cadence_group",
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
                    ts(now),
                    d.cadence.group.as_str()
                ])?;
            }
```

and append after `write_triage` (before `impl Store`):

```rust
/// Schema v4: gives every stored diagnosis its rhythm group from the cadence already in
/// `feed_health`, without fetching. Returns how many rows were classified.
pub(super) fn backfill_cadence_groups(
    conn: &rusqlite::Connection,
    now: DateTime<Utc>,
) -> Result<usize> {
    let rows: Vec<(FeedId, Option<String>, Option<f64>)> = conn
        .prepare("SELECT feed_id, last_entry_at, median_interval_days FROM feed_health")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut update =
        conn.prepare("UPDATE feed_health SET cadence_group = ?2 WHERE feed_id = ?1")?;
    for (feed_id, last_entry, median) in &rows {
        let group = classify(last_entry.as_deref().and_then(parse_ts), *median, now);
        update.execute(params![feed_id, group.as_str()])?;
    }
    Ok(rows.len())
}
```

The tests reach `ts`, `parse_ts` and `backfill_cadence_groups` through `use super::*;`.

- [ ] **Step 4: Run.**

```bash
cargo test -p ressac-core --lib store::
cargo test -p ressac --test cli migrate_reports_the_schema_version
```

Expected: all pass. `cargo test --workspace` still fails in `contract.rs` (version `3` and the committed v3 fixture): Task 1b.2.

- [ ] **Step 5: Do not commit yet** (the workspace is red until Task 1b.2 regenerates the fixture). Both tasks commit together.

### Task 1b.2: Contract tests and fixture at v4

**Files:**
- Modify: `crates/ressac-core/tests/contract.rs`
- Regenerate: `tests/fixtures/contract.sqlite`

**Interfaces:**
- Consumes: schema v4 (Task 1b.1), `CadenceGroup`.
- Produces: the contract fixture facts listed at the top of this file.

- [ ] **Step 1: Write the failing tests.** In `contract.rs`:

1. Imports: `use ressac_core::model::{Action, Cadence, CadenceGroup, Decision, Diagnosis, ErrorKind, Evidence, FeedId, Hop, Remark, Status, ts};` and `use rusqlite::{Connection, OpenFlags, params};`.
2. In `reading_columns_and_views_are_stable`: `assert_eq!(version, 4);`.
3. In `contract_fixture_covers_the_reading_cases`: `assert_eq!(int("SELECT version FROM schema_meta"), 4);`.
4. After `page_columns_and_view_are_stable`, add:

```rust
#[test]
fn cadence_and_edition_columns_and_views_are_stable() {
    let (_dir, conn) = live_store();
    assert_eq!(
        columns(&conn, "v_cadence"),
        [
            "feed_id",
            "title",
            "page",
            "folder_path",
            "effective_group",
            "computed_group",
            "is_override",
            "median_interval_days",
            "entries_per_month",
            "last_entry_at",
            "unread"
        ]
    );
    assert_eq!(
        columns(&conn, "v_edition_today"),
        [
            "edition_id",
            "built_at",
            "window_start",
            "window_end",
            "unreachable",
            "entry_id",
            "section",
            "rank",
            "feed_id",
            "feed_title",
            "title",
            "published_at",
            "read_at",
            "starred_at"
        ]
    );
    assert_eq!(
        columns(&conn, "v_edition_overflow"),
        [
            "edition_id",
            "section",
            "feed_id",
            "feed_title",
            "page",
            "hidden_count"
        ]
    );
    assert_eq!(
        columns(&conn, "edition"),
        ["id", "built_at", "window_start", "window_end", "unreachable"]
    );
    assert_eq!(
        columns(&conn, "edition_entry"),
        ["edition_id", "entry_id", "section", "rank"]
    );
    assert_eq!(
        columns(&conn, "edition_overflow"),
        ["edition_id", "feed_id", "section", "hidden_count"]
    );
    assert!(columns(&conn, "feed").contains(&"cadence_override".to_string()));
    assert!(columns(&conn, "feed_health").contains(&"cadence_group".to_string()));
}
```

5. After `contract_fixture_numbers_every_folder_and_feed`, add:

```rust
/// What the Swift tests rely on for rhythm and the edition (rows found by title or guid).
#[test]
fn contract_fixture_covers_rhythm_and_the_edition() {
    let db = fixture();
    let int = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };

    // v_cadence: diagnosed active feeds only (the buried Photos and the undiagnosed are out).
    let mut stmt = db
        .prepare("SELECT title, effective_group, computed_group, is_override FROM v_cadence ORDER BY page")
        .unwrap();
    let rows: Vec<(String, String, String, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        ("xkcd.com", "daily", "daily", false),
        ("Orange Drenka", "silent", "silent", false),
        ("Girl on the Couch", "monthly", "monthly", false),
        ("Bibliobsession 2.0", "weekly", "rare", true),
        ("Unphotographable", "silent", "silent", false),
        ("Daring Fireball", "weekly", "weekly", false),
    ]
    .map(|(t, e, c, o)| (t.to_string(), e.to_string(), c.to_string(), o));
    assert_eq!(rows, expected);
    assert_eq!(
        int("SELECT unread FROM v_cadence WHERE title = 'xkcd.com'"),
        3
    );

    // The edition: This Week then The Wire, no Front Page; xkcd keeps two entries.
    assert_eq!(int("SELECT count(*) FROM edition"), 1);
    assert_eq!(int("SELECT unreachable FROM edition"), 1);
    let mut stmt = db
        .prepare("SELECT rank, section, feed_title, title FROM v_edition_today ORDER BY rank")
        .unwrap();
    let entries: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let expected = [
        (1, "week", "Daring Fireball", "Linked: Tides"),
        (2, "week", "Daring Fireball", "Walled"),
        (3, "wire", "xkcd.com", "Recent comic"),
    ]
    .map(|(r, s, f, t)| (r, s.to_string(), f.to_string(), t.to_string()));
    assert_eq!(entries, expected);
    let overflow: (String, String, u16, i64) = db
        .query_row(
            "SELECT section, feed_title, page, hidden_count FROM v_edition_overflow",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(overflow, ("wire".into(), "xkcd.com".into(), 201, 2));
}
```

6. Fill the fixture. Add this helper after `diagnosis`:

```rust
/// A stored rhythm: the latest entry a day ago (in 2014 for Silent), `median` days apart.
fn rhythm(group: CadenceGroup, median: f64) -> Cadence {
    let last = if group == CadenceGroup::Silent {
        "2014-06-01T00:00:00Z".parse().unwrap()
    } else {
        now() - Duration::days(1)
    };
    Cadence {
        last_entry_at: Some(last),
        median_interval_days: Some(median),
        entries_per_month: 30.4375 / median,
        group,
    }
}
```

In `fill_contract_store`, replace the `save_diagnoses` map with:

```rust
            &HashMap::from([
                (
                    xkcd,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Daily, 0.5),
                        ..diagnosis(Status::Alive, "", None)
                    },
                ),
                (
                    fireball,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Weekly, 4.0),
                        ..diagnosis(Status::Alive, "", None)
                    },
                ),
                (
                    girl,
                    Diagnosis {
                        remarks: vec![Remark::HttpOnly],
                        cadence: rhythm(CadenceGroup::Monthly, 20.0),
                        ..diagnosis(
                            Status::Moved,
                            "301",
                            Some(Action::FollowRedirect {
                                url: "https://girl.example.org/feed".into(),
                            }),
                        )
                    },
                ),
                (
                    biblio,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Rare, 60.0),
                        ..diagnosis(Status::Hijacked, "vie ⚠", Some(Action::Bury))
                    },
                ),
                (
                    orange,
                    Diagnosis {
                        cadence: rhythm(CadenceGroup::Silent, 30.0),
                        ..diagnosis(Status::Fossil, "2014", None)
                    },
                ),
                (unphoto, diagnosis(Status::Dead, "410", Some(Action::Bury))),
            ]),
```

and append at the end of `fill_contract_store`, after `garden::recompute_triage(&store).unwrap();`:

```rust

    // Rhythm: one forced group. The column is core-written (`ressac feed cadence`); the
    // fixture sets it directly, like `starred_at` above.
    conn.execute(
        "UPDATE feed SET cadence_override = 'weekly' WHERE id = ?1",
        [biblio],
    )
    .unwrap();
    // Morning edition, written by hand (the edition builder is phase 2b): Daring Fireball's
    // two entries in This Week, xkcd's newest in The Wire and two more left in xkcd.
    conn.execute(
        "INSERT INTO edition (id, built_at, window_start, window_end, unreachable) VALUES (1, ?1, ?2, ?1, 1)",
        params![ts(now()), ts(now() - Duration::days(1))],
    )
    .unwrap();
    for (guid, section, rank) in [
        ("df-extracted", "week", 1),
        ("df-walled", "week", 2),
        ("x-recent", "wire", 3),
    ] {
        conn.execute(
            "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (1, ?1, ?2, ?3)",
            params![entry(guid), section, rank],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (1, ?1, 'wire', 2)",
        [xkcd],
    )
    .unwrap();
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --test contract`. Expected: `contract_fixture_schema_matches_the_live_store`, `contract_fixture_covers_the_reading_cases` and `contract_fixture_covers_rhythm_and_the_edition` fail (the committed fixture is still v3: no `v_cadence`, version 3); the column tests pass.

- [ ] **Step 3: Regenerate the fixture.**

```bash
RESSAC_WRITE_CONTRACT=1 cargo test -p ressac-core --test contract write_contract_fixture_when_asked
sqlite3 -readonly tests/fixtures/contract.sqlite "SELECT version FROM schema_meta; SELECT count(*) FROM v_cadence; SELECT count(*) FROM v_edition_today; SELECT hidden_count FROM v_edition_overflow;"
```

Expected output: `4`, `6`, `3`, `2`.

- [ ] **Step 4: Run the gate.**

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass, 0 failed. (The Swift suite is red until Phase 4a: `knownSchemaVersion` is still 3. Do not run it in this gate.)

- [ ] **Step 5: Commit (Tasks 1b.1 and 1b.2).**

```bash
git add schema/0004_edition.sql crates/ressac-core/src/store/mod.rs crates/ressac-core/src/store/health.rs crates/ressac-core/tests/contract.rs crates/ressac/tests/cli.rs tests/fixtures/contract.sqlite
git commit -m "Schema v4: rhythm groups stored, edition tables and contract views

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Phase 1b gate:** the commands of Task 1b.2 Step 4. Stop for Clément's approval. Next in order: Phase 4a (`phase-04-swift-data-engine.md`), then 1c.

---

# Phase 1c: The per-feed override

Run the quota check before starting.

### Task 1c.1: `Store::set_cadence_override` and `Store::cadence_row`

**Files:**
- Modify: `crates/ressac-core/src/error.rs`
- Modify: `crates/ressac-core/src/store/feeds.rs`
- Modify: `crates/ressac-core/src/store/mod.rs`

**Interfaces:**
- Consumes: `CadenceGroup`, schema v4.
- Produces:

```rust
// error.rs
#[error("No feed #{0}.")]
UnknownFeed(FeedId),
#[error("Feed #{0} is buried.")]
FeedBuried(FeedId),
#[error("An edition was already built this morning.")]
EditionNotDue,

// store/feeds.rs, re-exported as ressac_core::store::CadenceRow
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CadenceRow {
    pub feed_id: FeedId,
    pub effective_group: CadenceGroup,
    pub computed_group: CadenceGroup,
    pub is_override: bool,
}
impl Store {
    pub fn set_cadence_override(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<()>;
    pub fn cadence_row(&self, feed_id: FeedId) -> Result<CadenceRow>;
}
```

`cadence_row` reads `feed` LEFT JOIN `feed_health`, not `v_cadence`: a feed never diagnosed has no `feed_health` row, its computed group is then `Silent`. It refuses an unknown feed with `UnknownFeed`, and does not refuse a buried one (reading is harmless).

- [ ] **Step 1: Write the failing tests.** In `store/feeds.rs`, `mod tests`, change the model import to `use crate::model::{Action, Cadence, CadenceGroup, Decision, Diagnosis, Evidence, Status};` and append at the end:

```rust
    fn diagnosed(store: &Store, feed_id: FeedId, group: CadenceGroup) {
        let d = Diagnosis {
            status: Status::Alive,
            reason: "r".into(),
            evidence: Evidence::default(),
            remarks: vec![],
            action: None,
            hijack_score: 0.0,
            cadence: Cadence {
                group,
                ..Cadence::default()
            },
        };
        store
            .save_diagnoses(&HashMap::from([(feed_id, d)]), now())
            .unwrap();
    }

    #[test]
    fn an_override_wins_until_cleared_and_survives_a_new_diagnosis() {
        let (store, _) = imported();
        diagnosed(&store, 1, CadenceGroup::Weekly);
        store
            .set_cadence_override(1, Some(CadenceGroup::Daily))
            .unwrap();
        assert_eq!(
            store.cadence_row(1).unwrap(),
            CadenceRow {
                feed_id: 1,
                effective_group: CadenceGroup::Daily,
                computed_group: CadenceGroup::Weekly,
                is_override: true,
            }
        );
        // The next refresh reclassifies the feed; the override is never cleared on its own.
        diagnosed(&store, 1, CadenceGroup::Rare);
        let row = store.cadence_row(1).unwrap();
        assert_eq!(
            (row.effective_group, row.computed_group, row.is_override),
            (CadenceGroup::Daily, CadenceGroup::Rare, true)
        );
        let effective: String = store
            .conn()
            .query_row(
                "SELECT effective_group FROM v_cadence WHERE feed_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(effective, "daily");
        store.set_cadence_override(1, None).unwrap();
        assert_eq!(
            store.cadence_row(1).unwrap(),
            CadenceRow {
                feed_id: 1,
                effective_group: CadenceGroup::Rare,
                computed_group: CadenceGroup::Rare,
                is_override: false,
            }
        );
    }

    #[test]
    fn a_feed_without_a_diagnosis_reads_as_silent() {
        let (store, _) = imported();
        assert_eq!(
            store.cadence_row(2).unwrap(),
            CadenceRow {
                feed_id: 2,
                effective_group: CadenceGroup::Silent,
                computed_group: CadenceGroup::Silent,
                is_override: false,
            }
        );
        store
            .set_cadence_override(2, Some(CadenceGroup::Rare))
            .unwrap();
        assert_eq!(
            store.cadence_row(2).unwrap().effective_group,
            CadenceGroup::Rare
        );
    }

    #[test]
    fn the_override_refuses_an_unknown_or_buried_feed_and_writes_nothing() {
        let (store, _) = imported();
        let err = store
            .set_cadence_override(999, Some(CadenceGroup::Daily))
            .unwrap_err();
        assert!(matches!(err, Error::UnknownFeed(999)), "{err:?}");
        assert_eq!(err.to_string(), "No feed #999.");
        assert!(matches!(
            store.cadence_row(999).unwrap_err(),
            Error::UnknownFeed(999)
        ));
        store
            .conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 3", [])
            .unwrap();
        let err = store
            .set_cadence_override(3, Some(CadenceGroup::Daily))
            .unwrap_err();
        assert!(matches!(err, Error::FeedBuried(3)), "{err:?}");
        assert_eq!(err.to_string(), "Feed #3 is buried.");
        assert!(!store.cadence_row(3).unwrap().is_override, "nothing written");
    }
```

(`HashMap` is already imported at the top of `feeds.rs`; `Error` comes through `use super::*;` once Step 3 imports it.)

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib store::feeds`. Expected: compile errors ``no method named `set_cadence_override` found``, ``no method named `cadence_row` found``, ``cannot find struct `CadenceRow` ``, ``cannot find type `Error` ``.

- [ ] **Step 3: Implement.**

`error.rs`: add `use crate::model::FeedId;` under the module doc comment, and after `UnknownPageTarget`:

```rust
    #[error("No feed #{0}.")]
    UnknownFeed(FeedId),
    #[error("Feed #{0} is buried.")]
    FeedBuried(FeedId),
    /// Phase 2b's refusal; declared here because Phase 2b's five files are taken.
    #[error("An edition was already built this morning.")]
    EditionNotDue,
```

`store/feeds.rs` imports:

```rust
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::Store;
use crate::error::{Error, Result};
use crate::model::{CadenceGroup, FeedId, FolderId, ts};
```

after `FolderRow`:

```rust
/// One feed's rhythm as the apps see it in `v_cadence`: the group in use, the computed one,
/// and whether Clément forced it (spec §4, Override).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CadenceRow {
    pub feed_id: FeedId,
    pub effective_group: CadenceGroup,
    pub computed_group: CadenceGroup,
    pub is_override: bool,
}

/// A group name read from the database, or a conversion error naming the column.
fn group_at(r: &Row<'_>, col: usize) -> rusqlite::Result<Option<CadenceGroup>> {
    let name: Option<String> = r.get(col)?;
    name.map(|n| {
        CadenceGroup::parse(&n).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                col,
                rusqlite::types::Type::Text,
                format!("unknown rhythm group {n:?}").into(),
            )
        })
    })
    .transpose()
}
```

and in `impl Store`, after `pub fn feed`:

```rust
    /// Forces a feed's rhythm group, or clears the override (`None`). Refuses an unknown
    /// feed and a buried one; nothing is written then. Never cleared automatically.
    pub fn set_cadence_override(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<()> {
        let tx = self.write_tx()?;
        let state: Option<String> = tx
            .query_row("SELECT state FROM feed WHERE id = ?1", [feed_id], |r| {
                r.get(0)
            })
            .optional()?;
        match state.as_deref() {
            None => return Err(Error::UnknownFeed(feed_id)),
            Some("active") => {}
            Some(_) => return Err(Error::FeedBuried(feed_id)),
        }
        tx.execute(
            "UPDATE feed SET cadence_override = ?2 WHERE id = ?1",
            params![feed_id, group.map(CadenceGroup::as_str)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// A feed's rhythm, diagnosed or not (computed `Silent` without a diagnosis).
    pub fn cadence_row(&self, feed_id: FeedId) -> Result<CadenceRow> {
        let row = self
            .conn()
            .query_row(
                "SELECT d.cadence_override, h.cadence_group FROM feed d \
                 LEFT JOIN feed_health h ON h.feed_id = d.id WHERE d.id = ?1",
                [feed_id],
                |r| Ok((group_at(r, 0)?, group_at(r, 1)?)),
            )
            .optional()?;
        let Some((forced, computed)) = row else {
            return Err(Error::UnknownFeed(feed_id));
        };
        let computed = computed.unwrap_or_default();
        Ok(CadenceRow {
            feed_id,
            effective_group: forced.unwrap_or(computed),
            computed_group: computed,
            is_override: forced.is_some(),
        })
    }
```

`store/mod.rs`: `pub use feeds::{CadenceRow, FeedRow, FolderRow, ImportReport};`.

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib store::feeds`. Expected: all pass.

- [ ] **Step 5: Do not commit yet:** Task 1c.2 completes the phase; one commit for both.

### Task 1c.2: `Ressac::set_cadence`

**Files:**
- Modify: `crates/ressac-core/src/service.rs`

**Interfaces:**
- Consumes: `Store::set_cadence_override`, `Store::cadence_row`, `CadenceRow`.
- Produces: `pub fn Ressac::set_cadence(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<CadenceRow>` (used by `ressac feed cadence`, Phase 3b).

- [ ] **Step 1: Write the failing test.** In `service.rs`, `mod tests`, append after `pages_and_set_page_go_through_the_store`:

```rust
    #[test]
    fn set_cadence_goes_through_the_store_and_returns_the_row() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="A" xmlUrl="https://a.example.org/feed"/></body></opml>"#,
            false,
        )
        .unwrap();
        let row = app.set_cadence(1, Some(CadenceGroup::Weekly)).unwrap();
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            serde_json::json!({"feed_id": 1, "effective_group": "weekly", "computed_group": "silent", "is_override": true})
        );
        assert!(!app.set_cadence(1, None).unwrap().is_override);
        assert!(matches!(
            app.set_cadence(7, None).unwrap_err(),
            crate::error::Error::UnknownFeed(7)
        ));
    }
```

- [ ] **Step 2: Run it.** `cargo test -p ressac-core --lib service::tests::set_cadence`. Expected: compile errors ``no method named `set_cadence` found for struct `Ressac` `` and ``use of undeclared type `CadenceGroup` ``.

- [ ] **Step 3: Implement.** In `service.rs`, change the imports to:

```rust
use crate::model::{Action, CadenceGroup, Diagnosis, ErrorKind, FeedId, Status, parse_ts, ts};
```

```rust
use crate::store::{CadenceRow, FeedRow, ImportReport, Store};
```

and add right after `set_page`:

```rust
    /// Forces a feed's rhythm group, or gives it back to the computed one (`None`); see
    /// `Store::set_cadence_override` for the refusals. Returns the feed's row afterwards.
    pub fn set_cadence(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<CadenceRow> {
        self.store.set_cadence_override(feed_id, group)?;
        self.store.cadence_row(feed_id)
    }
```

- [ ] **Step 4: Run the gate.**

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass, 0 failed.

- [ ] **Step 5: Commit (Tasks 1c.1 and 1c.2).**

```bash
git add crates/ressac-core/src/error.rs crates/ressac-core/src/store/feeds.rs crates/ressac-core/src/store/mod.rs crates/ressac-core/src/service.rs
git commit -m "Core: per-feed rhythm override, refused on unknown and buried feeds

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Phase 1c gate:** the commands of Task 1c.2 Step 4. Stop for Clément's approval.

---

# Phase 1d: Reserved pages 104 to 109

Run the quota check before starting.

### Task 1d.1: Fourteen reserved pages

**Files:**
- Modify: `crates/ressac-core/src/store/pages.rs`
- Modify: `crates/ressac-core/src/service.rs`
- Modify: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub const RESERVED_PAGES: [u16; 14] = [100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899];` titles `Journal`, `Daily`, `Weekly`, `Monthly`, `Rare`, `Silent` for 104–109 (the app's `ReservedPage` titles in Phase 6a must equal them).

- [ ] **Step 1: Write the failing tests.**

In `pages.rs`, `mod tests`, `list_gives_reserved_pages_then_v_page_by_page`: every `rows[..8]` becomes `rows[..14]`, `rows[8..]` becomes `rows[14..]`, `rows[10]` becomes `rows[16]`, and the expected reserved list becomes:

```rust
            [
                (100, "Index"),
                (101, "Unread"),
                (102, "Today"),
                (103, "Starred"),
                (104, "Journal"),
                (105, "Daily"),
                (106, "Weekly"),
                (107, "Monthly"),
                (108, "Rare"),
                (109, "Silent"),
                (800, "Needs triage"),
                (801, "All feeds"),
                (802, "Cemetery"),
                (899, "Guide")
            ]
```

Append at the end of the module:

```rust
    #[test]
    fn journal_and_rhythm_pages_are_reserved_not_out_of_range() {
        let s = numbered();
        for page in 104..=109 {
            assert_eq!(
                set(s.conn(), PageKind::Feed, 10, page)
                    .unwrap_err()
                    .to_string(),
                format!("Page {page} is reserved.")
            );
        }
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 103)
                .unwrap_err()
                .to_string(),
            "Page 103 is reserved."
        );
        assert_eq!(
            set(s.conn(), PageKind::Feed, 10, 110).map(|_| feed_page(&s, 10)).unwrap(),
            Some(110),
            "110 stays the first assignable page"
        );
    }
```

In `service.rs`, `pages_and_set_page_go_through_the_store`: `.skip(8)` becomes `.skip(pages::RESERVED_PAGES.len())`.

In `crates/ressac/tests/cli.rs`, `page_list_prints_reserved_pages_then_every_folder_and_feed`: insert after `103 reserved - Starred\n\`:

```rust
         104 reserved - Journal\n\
         105 reserved - Daily\n\
         106 reserved - Weekly\n\
         107 reserved - Monthly\n\
         108 reserved - Rare\n\
         109 reserved - Silent\n\
```

and change `assert_eq!(lines.len(), 26);` to `assert_eq!(lines.len(), 32);` and `lines[11]` to `lines[17]`.

In the same file, `page_set_refusals_exit_2_and_write_nothing` (around line 705) expects `"Page 109 is not between 110 and 799.\n"` for `&["page", "set", "feed", "2", "109"]`; it becomes:

```rust
            "Page 109 is reserved.\n",
```

- [ ] **Step 2: Run them.**

```bash
cargo test -p ressac-core --lib store::pages
cargo test -p ressac --test cli page_
```

Expected: `list_gives_reserved_pages_then_v_page_by_page` fails (8 reserved rows: index out of range or `(800, "Needs triage")` where `(104, "Journal")` was expected); `journal_and_rhythm_pages_are_reserved_not_out_of_range` fails with `Page 104 is not between 110 and 799.`; the CLI tests fail on the missing lines and on `Page 109 is not between 110 and 799.`

- [ ] **Step 3: Implement.** In `pages.rs`:

```rust
/// Pages fixed in code and never stored: Index, Unread, Today, Starred, the Journal and the
/// five rhythm groups, the three garden pages and the Guide.
pub const RESERVED_PAGES: [u16; 14] = [
    100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899,
];
/// The pages a folder or a feed may hold.
pub const ASSIGNABLE: RangeInclusive<u16> = 110..=799;
/// Titles of `RESERVED_PAGES`, in the same order.
const RESERVED_TITLES: [&str; 14] = [
    "Index",
    "Unread",
    "Today",
    "Starred",
    "Journal",
    "Daily",
    "Weekly",
    "Monthly",
    "Rare",
    "Silent",
    "Needs triage",
    "All feeds",
    "Cemetery",
    "Guide",
];
```

`service.rs` already imports `crate::store::pages::{self, …}`, so `pages::RESERVED_PAGES` resolves in its tests through `use super::*;`.

- [ ] **Step 4: Run the gate.**

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass, 0 failed. (`page set … 800` and `… 100` keep their CLI refusals; `set_refuses_reserved_pages` now loops over 14 pages.)

- [ ] **Step 5: Commit.**

```bash
git add crates/ressac-core/src/store/pages.rs crates/ressac-core/src/service.rs crates/ressac/tests/cli.rs
git commit -m "Pages: reserve 104 to 109 for the Journal and the rhythm groups

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Phase 1d gate:** the commands of Step 4. Stop for Clément's approval. Next in order: Phase 2a (`phase-02-edition-core.md`).
