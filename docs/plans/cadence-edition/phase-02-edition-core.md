# Phase 2: the edition in the core

**Runs after:** Phase 1d (order of the sub-project: 1a → 1b → 4a → 1c → 1d → **2a → 2b** → 3a → 3b → 4b → 5a → 5b → 5c → 6a → 6b → 6c → 7).

Read the plan index (`docs/plans/2026-10-16-cadence-edition.md`) first. Global Constraints apply. This phase implements spec §6 (when, window, eligibility, sections and caps, order, empty edition), the core half of §9 (offline, partial failures, two builds at once, a quit during the build), and the Rust composition, due-rule and concurrency rows of §10. It owns plan-index Review Focus items **1** (DST day boundary, sub-phase 2a), **2** (entries fetched during the day go to tomorrow's edition), **3** (a feed buried after the build stays in today's edition) and the core half of **4** (a stopped build writes nothing), all in sub-phase 2b.

**Builds on (Phases 1a–1c, exact):**
- `ressac_core::model::CadenceGroup { Daily, Weekly, Monthly, Rare, Silent }` with `as_str(self) -> &'static str` (`"daily"` … `"silent"`) and `parse(&str) -> Option<CadenceGroup>`;
- schema v4 (`SCHEMA_VERSION = 4`): `feed.cadence_override`, `feed_health.cadence_group` (written by `save_diagnoses`), tables `edition`, `edition_entry`, `edition_overflow` (both with `ON DELETE CASCADE` on `edition_id`), index `entry_fetched`, views `v_cadence`, `v_edition_today`, `v_edition_overflow` exactly as in the plan index;
- classification during every refresh: `Ressac::refresh` ends with `diagnose_all`, which stores the group of every active feed. A feed whose new entry follows one 800 days older has a median of 800 days and is therefore `rare`.

**Existing code used as is (read before writing):**
- `Store::write_tx(&self) -> Result<rusqlite::Transaction<'_>>` (`store/mod.rs`, IMMEDIATE: the write lock is taken at the first statement, so the "due" re-check inside it is race-free between processes);
- `Store::conn(&self) -> &Connection` (`pub(crate)`); `PRAGMA foreign_keys = ON` is set in `Store::configure`, so the cascades work;
- `model::ts(DateTime<Utc>) -> String` (RFC 3339, milliseconds, `Z`) and `model::parse_ts(&str) -> Option<DateTime<Utc>>`. Every timestamp column is written with `ts`, so text comparison in SQL is chronological;
- `store::fetch::bad_timestamp(col)` is `pub(super)` in `store/fetch.rs`: usable from the new `store/edition.rs` as `super::fetch::bad_timestamp`;
- `entry.fetched_at` is set on insert only (`INSERT OR IGNORE … fetched_at = ts(a.at)`; the `UPDATE` branch never touches it), and on a feed's first fill entries published more than `BACKLOG_DAYS` (7) before the fetch arrive read;
- `Ressac::refresh(opts, sink, stop) -> Result<Summary>`: emits `Started`, one `FeedDone` per attempt (`error: Some(label)` on failure), `NetworkUnreachable` when the guard trips, then `Finished`; it returns `Ok` even when `stop` was raised; `Summary.network_unreachable` is the guard's verdict;
- `EventSink { fn emit(&self, event: &Event) }`, `NullSink`, and the `Recorder` pattern of `tests/end_to_end.rs`.

**Clock and time zone.** No time-zone crate is in `Cargo.lock` (`chrono` 0.4.45 with `clock`, `serde`, `std` only) and Global Constraints forbid a new one. `day_start` is generic over `chrono::TimeZone`; production passes `chrono::Local`; the DST tests use a small test-only `Paris` zone that implements `TimeZone` with the 2026 rules (CEST from 2026-03-29 01:00 UTC to 2026-10-25 01:00 UTC). This is deterministic and needs no environment variable.

**Why `window_end` is read from the real clock.** `refresh` stamps `fetched_at` with `Utc::now()` (via the attempts), not with the injected `now`. So `build_edition` ends the window at `max(Utc::now(), now)` after the refresh: an injected `now` in the future (tests simulating the next morning) moves the window forward, and a real build never leaves its own entries outside its window.

**Step 0 check.** `service.rs` (1504 lines) and `store/mod.rs` (467 lines) are over 300 lines, but this phase only appends a method, two types and an event variant to `service.rs`, and two lines to `store/mod.rs`; neither file is restructured, so the Step 0 rule does not apply. Task 2b.0 still greps them for debug leftovers, so that a leftover found now gets its own commit.

---

## Sub-phase 2a: the pure composition (`edition.rs`)

**Files (2):**
- Create: `crates/ressac-core/src/edition.rs` (tests inline)
- Modify: `crates/ressac-core/src/lib.rs` (`pub mod edition;`)

**Produces (plan index, Phase 2a, exact):** `CAP_WEEK`, `CAP_WIRE`, `BACKLOG_GUARD_DAYS`, `DAY_STARTS_AT_HOUR`, `FIRST_WINDOW_HOURS`, `Section` (+ `for_group`, `as_str`), `Caps` (+ `Default`), `Candidate`, `Placed`, `Overflow`, `Composition`, `compose`, `day_start`. One crate-private addition: `Section::parse(&str) -> Option<Section>` (`pub(crate)`, used by the store in 2b to read the `section` column back).

Before starting, run the quota guard:

```bash
python3 -c "import json;print([l['percentUsed'] for l in json.load(open('$HOME/.cache/claude-usage.json'))['limits'] if l['kind']=='seven_day'][0])"
```

Stop and ask Clément if it prints 94 or more.

### Task 2a.1: `Section`, `Caps`, `Candidate` and `compose`

- [ ] **Step 1: Write the failing tests**

Create `crates/ressac-core/src/edition.rs` with the module skeleton and its tests only (the bodies come in Step 3):

```rust
//! The morning edition's composition (spec §6): which section an entry goes to, the caps per
//! feed, the order and the ranks. Pure: `store::edition` selects the candidates and writes the
//! result.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CadenceGroup;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn cand(entry_id: i64, feed_id: FeedId, feed_page: Option<u16>, group: CadenceGroup, published: &str) -> Candidate {
        Candidate { entry_id, feed_id, feed_page, group, published_at: at(published) }
    }

    fn placed(c: &Composition) -> Vec<(i64, Section, u32)> {
        c.entries.iter().map(|p| (p.entry_id, p.section, p.rank)).collect()
    }

    #[test]
    fn groups_map_to_sections() {
        assert_eq!(Section::for_group(CadenceGroup::Rare), Some(Section::Front));
        assert_eq!(Section::for_group(CadenceGroup::Monthly), Some(Section::Front));
        assert_eq!(Section::for_group(CadenceGroup::Weekly), Some(Section::Week));
        assert_eq!(Section::for_group(CadenceGroup::Daily), Some(Section::Wire));
        assert_eq!(Section::for_group(CadenceGroup::Silent), None);
        for s in [Section::Front, Section::Week, Section::Wire] {
            assert_eq!(Section::parse(s.as_str()), Some(s));
        }
        assert_eq!(Section::parse("sports"), None);
        assert_eq!(serde_json::to_string(&Section::Wire).unwrap(), r#""wire""#);
    }

    #[test]
    fn default_caps_are_the_spec_constants() {
        assert_eq!(Caps::default(), Caps { week: CAP_WEEK, wire: CAP_WIRE });
        assert_eq!((CAP_WEEK, CAP_WIRE), (3, 1));
    }

    #[test]
    fn sections_come_in_order_and_ranks_run_across_the_edition() {
        let c = compose(
            &[
                cand(1, 10, Some(401), CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(2, 20, Some(402), CadenceGroup::Weekly, "2026-10-16T04:00:00Z"),
                cand(3, 30, Some(403), CadenceGroup::Rare, "2026-10-15T04:00:00Z"),
                cand(4, 40, Some(404), CadenceGroup::Monthly, "2026-10-15T03:00:00Z"),
            ],
            Caps::default(),
        );
        assert_eq!(
            placed(&c),
            [(3, Section::Front, 1), (4, Section::Front, 2), (2, Section::Week, 3), (1, Section::Wire, 4)]
        );
        assert!(c.overflow.is_empty());
    }

    #[test]
    fn feeds_follow_page_numbers_with_unnumbered_feeds_last() {
        let c = compose(
            &[
                cand(1, 7, None, CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(2, 9, Some(512), CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(3, 8, Some(203), CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
                cand(4, 3, None, CadenceGroup::Daily, "2026-10-16T05:00:00Z"),
            ],
            Caps::default(),
        );
        // Pages 203 then 512, then the feeds without a page by feed id (3 before 7).
        assert_eq!(placed(&c).iter().map(|p| p.0).collect::<Vec<_>>(), [3, 2, 4, 1]);
    }

    #[test]
    fn caps_apply_per_feed_and_the_rest_is_counted_as_overflow() {
        let mut cands = Vec::new();
        for i in 0..5 {
            // Feed 1, daily: five entries an hour apart; entry 104 is the newest.
            cands.push(cand(100 + i, 1, Some(401), CadenceGroup::Daily, &format!("2026-10-16T0{i}:00:00Z")));
            // Feed 2, weekly: five entries.
            cands.push(cand(200 + i, 2, Some(402), CadenceGroup::Weekly, &format!("2026-10-1{i}T08:00:00Z")));
        }
        // Feed 3, rare: front page is uncapped.
        for i in 0..4 {
            cands.push(cand(300 + i, 3, Some(403), CadenceGroup::Rare, &format!("2026-10-1{i}T09:00:00Z")));
        }
        let c = compose(&cands, Caps::default());
        assert_eq!(
            placed(&c),
            [
                (303, Section::Front, 1),
                (302, Section::Front, 2),
                (301, Section::Front, 3),
                (300, Section::Front, 4),
                (204, Section::Week, 5),
                (203, Section::Week, 6),
                (202, Section::Week, 7),
                (104, Section::Wire, 8),
            ]
        );
        assert_eq!(
            c.overflow,
            [
                Overflow { feed_id: 2, section: Section::Week, hidden_count: 2 },
                Overflow { feed_id: 1, section: Section::Wire, hidden_count: 4 },
            ]
        );
    }

    #[test]
    fn same_date_entries_keep_a_stable_order_by_id() {
        let c = compose(
            &[
                cand(5, 1, Some(401), CadenceGroup::Weekly, "2026-10-16T05:00:00Z"),
                cand(9, 1, Some(401), CadenceGroup::Weekly, "2026-10-16T05:00:00Z"),
            ],
            Caps::default(),
        );
        assert_eq!(placed(&c).iter().map(|p| p.0).collect::<Vec<_>>(), [9, 5]);
    }

    #[test]
    fn custom_caps_and_a_zero_cap() {
        let cands: Vec<Candidate> = (0..3)
            .map(|i| cand(i, 1, Some(401), CadenceGroup::Daily, &format!("2026-10-16T0{i}:00:00Z")))
            .collect();
        let none = compose(&cands, Caps { week: 3, wire: 0 });
        assert!(none.entries.is_empty());
        assert_eq!(none.overflow, [Overflow { feed_id: 1, section: Section::Wire, hidden_count: 3 }]);
        let two = compose(&cands, Caps { week: 3, wire: 2 });
        assert_eq!(placed(&two), [(2, Section::Wire, 1), (1, Section::Wire, 2)]);
    }

    #[test]
    fn silent_candidates_and_an_empty_input_give_nothing() {
        let c = compose(&[cand(1, 1, Some(401), CadenceGroup::Silent, "2026-10-16T05:00:00Z")], Caps::default());
        assert_eq!(c, Composition::default());
        assert_eq!(compose(&[], Caps::default()), Composition::default());
    }
}
```

Add `pub mod edition;` to `crates/ressac-core/src/lib.rs`, between `pub mod discover;` and `pub mod error;`:

```rust
pub mod discover;
pub mod edition;
pub mod error;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core edition::tests`
Expected: compile errors, `cannot find type Candidate in this scope`, `cannot find function compose`, and so on.

- [ ] **Step 3: Write the implementation**

Insert above `#[cfg(test)]` in `crates/ressac-core/src/edition.rs`:

```rust
use std::collections::BTreeMap;

use chrono::{DateTime, Duration, MappedLocalTime, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{CadenceGroup, FeedId};

/// Entries of one weekly feed kept in an edition.
pub const CAP_WEEK: usize = 3;
/// Entries of one daily feed kept in an edition: its most recent.
pub const CAP_WIRE: usize = 1;
/// An entry published more than this many days before the window opens is never eligible.
pub const BACKLOG_GUARD_DAYS: i64 = 7;
/// A day starts at this local hour: an edition built before it belongs to the day before.
pub const DAY_STARTS_AT_HOUR: u32 = 4;
/// The window of the very first edition.
pub const FIRST_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Front,
    Week,
    Wire,
}

impl Section {
    /// Monthly and Rare feeds make the front page; Silent feeds never appear.
    pub fn for_group(group: CadenceGroup) -> Option<Section> {
        match group {
            CadenceGroup::Monthly | CadenceGroup::Rare => Some(Section::Front),
            CadenceGroup::Weekly => Some(Section::Week),
            CadenceGroup::Daily => Some(Section::Wire),
            CadenceGroup::Silent => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Section::Front => "front",
            Section::Week => "week",
            Section::Wire => "wire",
        }
    }

    /// The inverse of `as_str`, for the `section` columns.
    pub(crate) fn parse(s: &str) -> Option<Section> {
        match s {
            "front" => Some(Section::Front),
            "week" => Some(Section::Week),
            "wire" => Some(Section::Wire),
            _ => None,
        }
    }

    fn cap(self, caps: Caps) -> usize {
        match self {
            Section::Front => usize::MAX,
            Section::Week => caps.week,
            Section::Wire => caps.wire,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub week: usize,
    pub wire: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps { week: CAP_WEEK, wire: CAP_WIRE }
    }
}

/// One eligible entry, with what the composition needs to place it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub entry_id: i64,
    pub feed_id: FeedId,
    pub feed_page: Option<u16>,
    /// The feed's effective group (override, else computed).
    pub group: CadenceGroup,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placed {
    pub entry_id: i64,
    pub section: Section,
    pub rank: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overflow {
    pub feed_id: FeedId,
    pub section: Section,
    pub hidden_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composition {
    pub entries: Vec<Placed>,
    pub overflow: Vec<Overflow>,
}

/// Sections front → week → wire; inside a section feeds by page (feeds without a page last,
/// then by feed id), entries by `published_at` then `entry_id`, newest first; caps per feed and
/// per edition; ranks from 1 across the whole edition.
pub fn compose(candidates: &[Candidate], caps: Caps) -> Composition {
    // (section, has no page, page, feed) orders the feeds; the value is that feed's entries.
    let mut feeds: BTreeMap<(Section, bool, u16, FeedId), Vec<&Candidate>> = BTreeMap::new();
    for c in candidates {
        if let Some(section) = Section::for_group(c.group) {
            let key = (section, c.feed_page.is_none(), c.feed_page.unwrap_or(0), c.feed_id);
            feeds.entry(key).or_default().push(c);
        }
    }
    let mut out = Composition::default();
    let mut rank = 0u32;
    for ((section, _, _, feed_id), mut entries) in feeds {
        entries.sort_by(|a, b| {
            b.published_at
                .cmp(&a.published_at)
                .then(b.entry_id.cmp(&a.entry_id))
        });
        let cap = section.cap(caps);
        for c in entries.iter().take(cap) {
            rank += 1;
            out.entries.push(Placed { entry_id: c.entry_id, section, rank });
        }
        let hidden = entries.len().saturating_sub(cap);
        if hidden > 0 {
            out.overflow.push(Overflow {
                feed_id,
                section,
                hidden_count: u32::try_from(hidden).unwrap_or(u32::MAX),
            });
        }
    }
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core edition::tests`
Expected: PASS, 7 tests (`day_start` is not written yet and has no test yet).

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/edition.rs crates/ressac-core/src/lib.rs
git commit -m "Core: compose an edition (sections, caps per feed, ranks)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2a.2: `day_start` (Review Focus 1)

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` of `crates/ressac-core/src/edition.rs`:

```rust
    use chrono::{FixedOffset, NaiveDate, NaiveDateTime};

    /// Europe/Paris for 2026 only: CEST (+2) from 2026-03-29 01:00 UTC to 2026-10-25 01:00 UTC,
    /// CET (+1) otherwise. Enough to pin both clock changes without a time-zone crate.
    #[derive(Debug, Clone, Copy)]
    struct Paris;

    impl Paris {
        fn offset_at_utc(utc: &NaiveDateTime) -> FixedOffset {
            let summer_from = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap().and_hms_opt(1, 0, 0).unwrap();
            let summer_until = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap().and_hms_opt(1, 0, 0).unwrap();
            let hours = if *utc >= summer_from && *utc < summer_until { 2 } else { 1 };
            FixedOffset::east_opt(hours * 3600).unwrap()
        }
    }

    impl TimeZone for Paris {
        type Offset = FixedOffset;

        fn from_offset(_: &FixedOffset) -> Self {
            Paris
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
            self.offset_from_local_datetime(&local.and_hms_opt(12, 0, 0).unwrap())
        }

        /// Both offsets are tried; the ones that read back are the answers. Two answers is the
        /// repeated hour (summer time first, it is the earlier instant); none is the skipped hour.
        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
            let fits: Vec<FixedOffset> = [2, 1]
                .into_iter()
                .map(|h| FixedOffset::east_opt(h * 3600).unwrap())
                .filter(|o| Paris::offset_at_utc(&(*local - *o)) == *o)
                .collect();
            match fits.as_slice() {
                [] => MappedLocalTime::None,
                [o] => MappedLocalTime::Single(*o),
                [a, b, ..] => MappedLocalTime::Ambiguous(*a, *b),
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
            Paris::offset_at_utc(&utc.and_hms_opt(12, 0, 0).unwrap())
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
            Paris::offset_at_utc(utc)
        }
    }

    fn paris(utc: &str) -> DateTime<Paris> {
        at(utc).with_timezone(&Paris)
    }

    #[test]
    fn the_paris_test_zone_follows_the_2026_changes() {
        assert_eq!(paris("2026-10-16T08:00:00Z").offset().local_minus_utc(), 7200);
        assert_eq!(paris("2026-12-01T08:00:00Z").offset().local_minus_utc(), 3600);
        let repeated = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap().and_hms_opt(2, 30, 0).unwrap();
        assert!(matches!(Paris.from_local_datetime(&repeated), MappedLocalTime::Ambiguous(..)));
        let skipped = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap().and_hms_opt(2, 30, 0).unwrap();
        assert!(matches!(Paris.from_local_datetime(&skipped), MappedLocalTime::None));
    }

    #[test]
    fn a_day_starts_at_four_in_utc() {
        assert_eq!(day_start(at("2026-10-16T03:59:59Z")), at("2026-10-15T04:00:00Z"));
        assert_eq!(day_start(at("2026-10-16T04:00:00Z")), at("2026-10-16T04:00:00Z"));
        assert_eq!(day_start(at("2026-10-16T23:30:00Z")), at("2026-10-16T04:00:00Z"));
    }

    #[test]
    fn a_day_starts_at_four_local_on_an_ordinary_paris_day() {
        // 06:00 CEST → today 04:00 CEST = 02:00 UTC.
        assert_eq!(day_start(paris("2026-10-16T04:00:00Z")), at("2026-10-16T02:00:00Z"));
        // 01:30 CEST, after midnight but before four → yesterday's 04:00 CEST.
        assert_eq!(day_start(paris("2026-10-15T23:30:00Z")), at("2026-10-15T02:00:00Z"));
    }

    /// Review Focus 1: the night of 2026-10-25, clocks go back from 03:00 CEST to 02:00 CET.
    #[test]
    fn the_autumn_change_keeps_four_local_on_both_sides() {
        // 02:30 CEST (the first 02:30) → still the 24th.
        assert_eq!(day_start(paris("2026-10-25T00:30:00Z")), at("2026-10-24T02:00:00Z"));
        // 03:30 CET, after the change → still the 24th: an edition built now counts for the day before.
        assert_eq!(day_start(paris("2026-10-25T02:30:00Z")), at("2026-10-24T02:00:00Z"));
        // 04:30 CET → the 25th, whose 04:00 is CET = 03:00 UTC.
        assert_eq!(day_start(paris("2026-10-25T03:30:00Z")), at("2026-10-25T03:00:00Z"));
    }

    /// Review Focus 1: the night of 2026-03-29, clocks go forward from 02:00 CET to 03:00 CEST.
    #[test]
    fn the_spring_change_keeps_four_local_on_both_sides() {
        // 03:30 CEST → the 28th, whose 04:00 was CET = 03:00 UTC.
        assert_eq!(day_start(paris("2026-03-29T01:30:00Z")), at("2026-03-28T03:00:00Z"));
        // 04:30 CEST → the 29th, 04:00 CEST = 02:00 UTC.
        assert_eq!(day_start(paris("2026-03-29T02:30:00Z")), at("2026-03-29T02:00:00Z"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core edition::tests`
Expected: compile error `cannot find function day_start in this scope`.

- [ ] **Step 3: Write the implementation**

Append after `compose` in `crates/ressac-core/src/edition.rs`:

```rust
/// `DAY_STARTS_AT_HOUR` local of the day `now` belongs to (the day before when `now` is earlier
/// than that hour), in UTC. A repeated hour takes its first occurrence; a skipped one takes the
/// first local hour after it that exists.
pub fn day_start<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Utc> {
    let tz = now.timezone();
    let local = now.naive_local();
    let date = if local.hour() < DAY_STARTS_AT_HOUR {
        local.date().pred_opt().unwrap_or(local.date())
    } else {
        local.date()
    };
    let start = date
        .and_hms_opt(DAY_STARTS_AT_HOUR, 0, 0)
        .expect("DAY_STARTS_AT_HOUR is a valid hour");
    (0..=3)
        .find_map(|h| {
            match tz.from_local_datetime(&(start + Duration::hours(h))) {
                MappedLocalTime::Single(t) | MappedLocalTime::Ambiguous(t, _) => Some(t.with_timezone(&Utc)),
                MappedLocalTime::None => None,
            }
        })
        .unwrap_or_else(|| Utc.from_utc_datetime(&start))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core edition::tests`
Expected: PASS, 12 tests.

- [ ] **Step 5: Gate and commit**

Run the Rust gate:

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: no diff from `fmt` (run `cargo fmt --all` first if the long `cand(...)` lines above were reflowed), clippy clean, every test passes.

```bash
git add crates/ressac-core/src/edition.rs
git commit -m "Core: a day starts at 04:00 local, across both clock changes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Sub-phase 2a ends here: stop for Clément's approval.**

---

## Sub-phase 2b: storing and building the edition

**Files (5):**
- Create: `crates/ressac-core/src/store/edition.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (`mod edition;`, re-exports)
- Modify: `crates/ressac/src/output.rs` (`Event::Edition` in the progress display's exhaustive `match`)
- Modify: `crates/ressac-core/src/service.rs` (`EditionOptions`, `EditionReport`, `EditionOutcome`, `Event::Edition`, `Ressac::build_edition`, `Ressac::edition_today`, one JSON test)
- Create: `crates/ressac-core/tests/edition.rs` (build against a local mock web)

**Produces (plan index, Phase 2b, exact):** `EditionRecord`, `EditionParams`, `Store::latest_edition`, `Store::edition_due`, `Store::edition_candidates`, `Store::write_edition`, `Error::EditionNotDue`, `EditionOptions`, `EditionReport`, `EditionOutcome { Built, NotDue, Offline, Interrupted }`, `Ressac::build_edition`, `Ressac::edition_today`, `EditionView`, `EditionEntryRow`, `EditionOverflowRow`, `Event::Edition`. The three view types are defined in `store/edition.rs` and re-exported from `ressac_core::store` and `ressac_core::service`, so both paths work for Phase 3.

Before starting, run the quota guard (see 2a); stop at 94 or more.

### Task 2b.0: Step 0 check on the two long files

- [ ] **Step 1: Look for debug leftovers**

Run:

```bash
grep -n "dbg!\|println!\|eprintln!\|todo!\|unimplemented!\|#\[allow(dead_code)\]" crates/ressac-core/src/service.rs crates/ressac-core/src/store/mod.rs
```

Expected: no output. If anything is printed, remove it (keep `tracing::` calls, they are the logs of record), run the Rust gate, and commit alone:

```bash
git add crates/ressac-core/src/service.rs crates/ressac-core/src/store/mod.rs
git commit -m "Core: Step 0, remove debug leftovers before the edition

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2b.1: candidates, due rule and writing an edition

- [ ] **Step 1: Write the failing tests**

`Error::EditionNotDue` (`An edition was already built this morning.`) already exists: Phase 1c declared it next to `UnknownFeed` and `FeedBuried`, so that this sub-phase has a file left for `output.rs`.

In `crates/ressac-core/src/store/mod.rs`, add the module next to the others and re-export its types:

```rust
mod decisions;
mod edition;
mod feeds;
```

```rust
pub use decisions::{DecisionRecord, FeedSnapshot};
pub use edition::{EditionEntryRow, EditionOverflowRow, EditionParams, EditionRecord, EditionView};
```

Create `crates/ressac-core/src/store/edition.rs` with only its tests for now:

```rust
//! Editions: candidate entries, the 04:00 due rule, and writing one edition in one transaction.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edition::{Caps, Section};
    use rusqlite::params;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// An active feed with its page and stored group (`daily`, `weekly`, `monthly`, `rare`, `silent`).
    fn add_feed(s: &Store, id: i64, page: Option<u16>, group: &str) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, position, title, xml_url, xml_url_original, url_key, added_at, page) \
                 VALUES (?1, ?1, ?2, ?3, ?3, ?3, '2026-01-01T00:00:00.000Z', ?4)",
                params![id, format!("Feed {id}"), format!("https://f{id}.example.org/feed"), page],
            )
            .unwrap();
        s.conn()
            .execute(
                "INSERT INTO feed_health (feed_id, status, reason, computed_at, cadence_group) \
                 VALUES (?1, 'Alive', '', '2026-01-01T00:00:00.000Z', ?2)",
                params![id, group],
            )
            .unwrap();
    }

    fn add_entry(s: &Store, id: i64, feed: i64, published: &str, fetched: &str) {
        s.conn()
            .execute(
                "INSERT INTO entry (id, feed_id, guid, title, published_at, fetched_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, feed, format!("g{id}"), format!("Entry {id}"), ts(at(published)), ts(at(fetched))],
            )
            .unwrap();
    }

    fn mark_read(s: &Store, id: i64) {
        s.conn()
            .execute("UPDATE entry SET read_at = '2026-10-16T07:00:00.000Z' WHERE id = ?1", [id])
            .unwrap();
    }

    /// Params for a build at `end` on the day starting at `day`.
    fn params_at(day: &str, end: &str) -> EditionParams {
        EditionParams {
            day_start: at(day),
            window_end: at(end),
            built_at: at(end),
            caps: Caps::default(),
            unreachable: 0,
            rebuild: false,
        }
    }

    fn ids(s: &Store) -> Vec<i64> {
        s.edition_view()
            .unwrap()
            .map(|v| v.entries.iter().map(|e| e.entry_id).collect())
            .unwrap_or_default()
    }

    fn edition_count(s: &Store) -> i64 {
        s.conn().query_row("SELECT count(*) FROM edition", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_first_edition_covers_the_last_24_hours() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-15T06:00:00Z", "2026-10-15T06:00:00Z"); // in
        add_entry(&s, 2, 1, "2026-10-15T04:00:00Z", "2026-10-15T04:00:00Z"); // 25 h before: out
        let (rec, comp) = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        assert_eq!(rec.window_start, at("2026-10-15T05:00:00Z"));
        assert_eq!(rec.window_end, at("2026-10-16T05:00:00Z"));
        assert_eq!(comp.entries.len(), 1);
        assert_eq!(ids(&s), [1]);
        assert_eq!(s.latest_edition().unwrap(), Some(rec));
    }

    /// Review Focus 2: entries fetched after the build leave today's edition alone and open tomorrow's.
    #[test]
    fn entries_fetched_after_the_build_go_to_the_next_edition() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T03:00:00Z", "2026-10-16T03:00:00Z");
        let (first, _) = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        add_entry(&s, 2, 1, "2026-10-16T11:00:00Z", "2026-10-16T11:30:00Z"); // a manual refresh at noon
        assert_eq!(ids(&s), [1], "today's edition is frozen");
        let (second, _) = s.write_edition(&params_at("2026-10-17T04:00:00Z", "2026-10-17T05:00:00Z")).unwrap();
        assert_eq!(second.window_start, first.window_end);
        assert_eq!(ids(&s), [2]);
    }

    #[test]
    fn a_monday_edition_spans_the_weekend_with_the_same_caps() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap(); // Friday
        add_entry(&s, 1, 1, "2026-10-17T08:00:00Z", "2026-10-17T09:00:00Z"); // Saturday
        add_entry(&s, 2, 1, "2026-10-18T08:00:00Z", "2026-10-18T09:00:00Z"); // Sunday
        add_entry(&s, 3, 1, "2026-10-19T03:00:00Z", "2026-10-19T04:30:00Z"); // Monday morning
        let (monday, comp) = s.write_edition(&params_at("2026-10-19T04:00:00Z", "2026-10-19T05:00:00Z")).unwrap();
        assert_eq!(monday.window_start, at("2026-10-16T05:00:00Z"));
        assert_eq!(ids(&s), [3], "one entry per daily feed");
        assert_eq!(comp.overflow.len(), 1);
        assert_eq!(comp.overflow[0].hidden_count, 2);
    }

    #[test]
    fn only_unread_entries_of_active_non_silent_feeds_within_the_guard() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_feed(&s, 2, Some(402), "silent");
        add_feed(&s, 3, Some(403), "weekly");
        s.conn().execute("UPDATE feed SET state = 'buried' WHERE id = 3", []).unwrap();
        let fetched = "2026-10-16T03:00:00Z";
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", fetched); // in
        add_entry(&s, 2, 1, "2026-10-16T01:00:00Z", fetched); // read: out
        mark_read(&s, 2);
        add_entry(&s, 3, 2, "2026-10-16T02:00:00Z", fetched); // silent feed: out
        add_entry(&s, 4, 3, "2026-10-16T02:00:00Z", fetched); // buried feed: out
        // The window opens 2026-10-15T05:00Z, so the guard is 2026-10-08T05:00Z.
        add_entry(&s, 5, 1, "2026-10-08T04:59:00Z", fetched); // republished archive: out
        add_entry(&s, 6, 1, "2026-10-08T05:00:00Z", fetched); // exactly at the guard: in
        let c = s
            .edition_candidates(at("2026-10-15T05:00:00Z"), at("2026-10-16T05:00:00Z"))
            .unwrap();
        let mut got: Vec<i64> = c.iter().map(|c| c.entry_id).collect();
        got.sort_unstable();
        assert_eq!(got, [1, 6]);
        assert_eq!(c[0].feed_page, Some(401));
        assert_eq!(c[0].group, CadenceGroup::Weekly);
    }

    #[test]
    fn an_override_wins_over_the_computed_group() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        s.conn().execute("UPDATE feed SET cadence_override = 'rare' WHERE id = 1", []).unwrap();
        add_feed(&s, 2, Some(402), "rare");
        s.conn().execute("UPDATE feed SET cadence_override = 'silent' WHERE id = 2", []).unwrap();
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 2, 1, "2026-10-16T01:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 3, 2, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        let (_, comp) = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        // Feed 1 forced to Rare: front page, uncapped. Feed 2 forced to Silent: absent.
        assert_eq!(comp.entries.iter().map(|p| (p.entry_id, p.section)).collect::<Vec<_>>(), [(1, Section::Front), (2, Section::Front)]);
    }

    #[test]
    fn the_due_rule_follows_the_day_start() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.edition_due(at("2026-10-16T04:00:00Z")).unwrap());
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        assert!(!s.edition_due(at("2026-10-16T04:00:00Z")).unwrap());
        assert!(s.edition_due(at("2026-10-17T04:00:00Z")).unwrap());
    }

    /// Two builds the same morning (the app and the CLI): the second is refused inside its
    /// transaction and writes nothing.
    #[test]
    fn a_second_build_the_same_morning_is_refused() {
        let s = Store::open_in_memory().unwrap();
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        let again = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T06:00:00Z"));
        assert!(matches!(again, Err(Error::EditionNotDue)));
        assert_eq!(edition_count(&s), 1);
    }

    #[test]
    fn a_rebuild_replaces_today_and_keeps_its_window_start() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        add_entry(&s, 1, 1, "2026-10-16T01:00:00Z", "2026-10-16T03:00:00Z");
        add_entry(&s, 2, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        let (first, _) = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        assert_eq!(ids(&s), [2]);
        mark_read(&s, 2);
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T09:00:00Z");
        p.rebuild = true;
        p.caps = Caps { week: 3, wire: 5 };
        let (second, _) = s.write_edition(&p).unwrap();
        assert_ne!(second.id, first.id);
        assert_eq!(second.window_start, first.window_start);
        assert_eq!(edition_count(&s), 1, "today's edition was replaced");
        assert_eq!(ids(&s), [1], "an entry read since then is no longer eligible");
    }

    #[test]
    fn a_rebuild_without_an_edition_today_builds_normally() {
        let s = Store::open_in_memory().unwrap();
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z");
        p.rebuild = true;
        let (rec, _) = s.write_edition(&p).unwrap();
        assert_eq!(rec.window_start, at("2026-10-15T05:00:00Z"));
    }

    #[test]
    fn an_empty_edition_is_stored_and_advances_the_window() {
        let s = Store::open_in_memory().unwrap();
        let (empty, comp) = s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        assert_eq!(comp, Composition::default());
        let view = s.edition_view().unwrap().expect("an empty edition is still an edition");
        assert_eq!(view.edition, empty);
        assert!(view.entries.is_empty() && view.overflow.is_empty());
        let (next, _) = s.write_edition(&params_at("2026-10-17T04:00:00Z", "2026-10-17T05:00:00Z")).unwrap();
        assert_eq!(next.window_start, empty.window_end);
    }

    #[test]
    fn the_view_lists_entries_by_rank_and_overflow_by_section() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "daily");
        add_feed(&s, 2, Some(402), "weekly");
        for i in 0..3 {
            add_entry(&s, 10 + i, 1, &format!("2026-10-16T0{i}:00:00Z"), "2026-10-16T03:00:00Z");
        }
        for i in 0..5 {
            add_entry(&s, 20 + i, 2, &format!("2026-10-1{i}T08:00:00Z"), "2026-10-16T03:00:00Z");
        }
        let mut p = params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z");
        p.unreachable = 3;
        s.write_edition(&p).unwrap();
        let v = s.edition_view().unwrap().unwrap();
        assert_eq!(v.edition.unreachable, 3);
        assert_eq!(
            v.entries.iter().map(|e| (e.rank, e.entry_id, e.section)).collect::<Vec<_>>(),
            [(1, 24, Section::Week), (2, 23, Section::Week), (3, 22, Section::Week), (4, 12, Section::Wire)]
        );
        assert_eq!(v.entries[0].feed_title, "Feed 2");
        assert!(v.entries[0].read_at.is_none());
        assert_eq!(
            v.overflow.iter().map(|o| (o.section, o.feed_id, o.hidden_count)).collect::<Vec<_>>(),
            [(Section::Week, 2, 2), (Section::Wire, 1, 2)]
        );
        assert_eq!(v.overflow[0].page, Some(402));
    }

    /// Review Focus 3: burying a feed after the build leaves today's edition as it was.
    #[test]
    fn a_feed_buried_after_the_build_stays_in_today_s_edition() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        s.conn().execute("UPDATE feed SET state = 'buried' WHERE id = 1", []).unwrap();
        assert_eq!(ids(&s), [1]);
    }

    #[test]
    fn read_state_in_the_view_is_live() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, Some(401), "weekly");
        add_entry(&s, 1, 1, "2026-10-16T02:00:00Z", "2026-10-16T03:00:00Z");
        s.write_edition(&params_at("2026-10-16T04:00:00Z", "2026-10-16T05:00:00Z")).unwrap();
        mark_read(&s, 1);
        let v = s.edition_view().unwrap().unwrap();
        assert_eq!(v.entries[0].read_at, Some(at("2026-10-16T07:00:00Z")));
    }

    #[test]
    fn no_edition_at_all() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.latest_edition().unwrap(), None);
        assert_eq!(s.edition_view().unwrap(), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core store::edition::tests`
Expected: compile errors in `store/edition.rs` (`cannot find type EditionParams`, `no method named write_edition`), and the `pub use edition::{…}` line in `store/mod.rs` fails to resolve.

- [ ] **Step 3: Write the implementation**

Insert above `#[cfg(test)]` in `crates/ressac-core/src/store/edition.rs`:

```rust
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::Store;
use super::fetch::bad_timestamp;
use crate::edition::{
    BACKLOG_GUARD_DAYS, Candidate, Caps, Composition, FIRST_WINDOW_HOURS, Section, compose,
};
use crate::error::{Error, Result};
use crate::model::{CadenceGroup, FeedId, parse_ts, ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionRecord {
    pub id: i64,
    pub built_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub unreachable: u32,
}

pub struct EditionParams {
    /// 04:00 local of the day being built, in UTC (`edition::day_start`).
    pub day_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub built_at: DateTime<Utc>,
    pub caps: Caps,
    /// Feeds whose fetch failed during the build's refresh.
    pub unreachable: u32,
    /// Replace today's edition, reusing its `window_start`.
    pub rebuild: bool,
}

/// One row of `v_edition_today`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionEntryRow {
    pub entry_id: i64,
    pub section: Section,
    pub rank: u32,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub title: Option<String>,
    pub published_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub starred_at: Option<DateTime<Utc>>,
}

/// One row of `v_edition_overflow`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionOverflowRow {
    pub section: Section,
    pub feed_id: FeedId,
    pub feed_title: String,
    pub page: Option<u16>,
    pub hidden_count: u32,
}

/// The latest edition with its entries (by rank) and its overflow (by section, then page).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionView {
    pub edition: EditionRecord,
    pub entries: Vec<EditionEntryRow>,
    pub overflow: Vec<EditionOverflowRow>,
}

const EDITION_COLUMNS: &str = "id, built_at, window_start, window_end, unreachable";

fn time(row: &Row<'_>, col: usize) -> rusqlite::Result<DateTime<Utc>> {
    let text: String = row.get(col)?;
    parse_ts(&text).ok_or_else(|| bad_timestamp(col))
}

fn optional_time(row: &Row<'_>, col: usize) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let text: Option<String> = row.get(col)?;
    text.map(|t| parse_ts(&t).ok_or_else(|| bad_timestamp(col))).transpose()
}

fn section(row: &Row<'_>, col: usize) -> rusqlite::Result<Section> {
    let text: String = row.get(col)?;
    Section::parse(&text).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(col, "section".into(), rusqlite::types::Type::Text)
    })
}

fn edition_record(row: &Row<'_>) -> rusqlite::Result<EditionRecord> {
    Ok(EditionRecord {
        id: row.get(0)?,
        built_at: time(row, 1)?,
        window_start: time(row, 2)?,
        window_end: time(row, 3)?,
        unreachable: row.get(4)?,
    })
}

fn latest(conn: &Connection) -> Result<Option<EditionRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {EDITION_COLUMNS} FROM edition ORDER BY id DESC LIMIT 1"),
            [],
            edition_record,
        )
        .optional()?)
}

/// The eligible entries of a window (spec §6, Eligibility).
fn candidates(conn: &Connection, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Candidate>> {
    let guard = start - Duration::days(BACKLOG_GUARD_DAYS);
    let mut stmt = conn.prepare(
        "SELECT e.id, e.feed_id, d.page, COALESCE(d.cadence_override, h.cadence_group), e.published_at \
         FROM entry e JOIN feed d ON d.id = e.feed_id JOIN feed_health h ON h.feed_id = d.id \
         WHERE d.state = 'active' AND e.read_at IS NULL \
           AND e.fetched_at > ?1 AND e.fetched_at <= ?2 AND e.published_at >= ?3 \
           AND COALESCE(d.cadence_override, h.cadence_group) <> 'silent'",
    )?;
    let rows = stmt
        .query_map(params![ts(start), ts(end), ts(guard)], |r| {
            let group: String = r.get(3)?;
            Ok(Candidate {
                entry_id: r.get(0)?,
                feed_id: r.get(1)?,
                feed_page: r.get(2)?,
                group: CadenceGroup::parse(&group).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(3, "cadence group".into(), rusqlite::types::Type::Text)
                })?,
                published_at: time(r, 4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

impl Store {
    pub fn latest_edition(&self) -> Result<Option<EditionRecord>> {
        latest(self.conn())
    }

    /// No edition has `built_at >= day_start`.
    pub fn edition_due(&self, day_start: DateTime<Utc>) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM edition WHERE built_at >= ?1)",
            [ts(day_start)],
            |r| r.get(0),
        )?)
    }

    /// Active feeds only, `fetched_at` in `(window_start, window_end]`, unread,
    /// `published_at >= window_start − BACKLOG_GUARD_DAYS`, effective group not Silent.
    pub fn edition_candidates(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Candidate>> {
        candidates(self.conn(), window_start, window_end)
    }

    /// In one write transaction: re-checks the due rule (with `rebuild`, deletes today's edition
    /// and reuses its `window_start`), selects the candidates, composes them and inserts the
    /// edition with its entries and overflow. Not due and not `rebuild` → `Error::EditionNotDue`.
    pub fn write_edition(&self, p: &EditionParams) -> Result<(EditionRecord, Composition)> {
        let tx = self.write_tx()?;
        let today: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, window_start FROM edition WHERE built_at >= ?1 ORDER BY id DESC LIMIT 1",
                [ts(p.day_start)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let window_start = match today {
            Some(_) if !p.rebuild => return Err(Error::EditionNotDue),
            Some((id, start)) => {
                tx.execute("DELETE FROM edition WHERE id = ?1", [id])?;
                parse_ts(&start).ok_or_else(|| bad_timestamp(1))?
            }
            None => match latest(&tx)? {
                Some(previous) => previous.window_end,
                None => p.window_end - Duration::hours(FIRST_WINDOW_HOURS),
            },
        };
        let composition = compose(&candidates(&tx, window_start, p.window_end)?, p.caps);
        tx.execute(
            "INSERT INTO edition (built_at, window_start, window_end, unreachable) VALUES (?1, ?2, ?3, ?4)",
            params![ts(p.built_at), ts(window_start), ts(p.window_end), p.unreachable],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut entry = tx.prepare(
                "INSERT INTO edition_entry (edition_id, entry_id, section, rank) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for e in &composition.entries {
                entry.execute(params![id, e.entry_id, e.section.as_str(), e.rank])?;
            }
            let mut overflow = tx.prepare(
                "INSERT INTO edition_overflow (edition_id, feed_id, section, hidden_count) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for o in &composition.overflow {
                overflow.execute(params![id, o.feed_id, o.section.as_str(), o.hidden_count])?;
            }
        }
        tx.commit()?;
        let record = EditionRecord {
            id,
            built_at: p.built_at,
            window_start,
            window_end: p.window_end,
            unreachable: p.unreachable,
        };
        Ok((record, composition))
    }

    /// The latest edition read through the contract views; `None` before the first edition.
    pub fn edition_view(&self) -> Result<Option<EditionView>> {
        let Some(edition) = latest(self.conn())? else {
            return Ok(None);
        };
        let entries = self
            .conn()
            .prepare(
                "SELECT entry_id, section, rank, feed_id, feed_title, title, published_at, read_at, starred_at \
                 FROM v_edition_today ORDER BY rank",
            )?
            .query_map([], |r| {
                Ok(EditionEntryRow {
                    entry_id: r.get(0)?,
                    section: section(r, 1)?,
                    rank: r.get(2)?,
                    feed_id: r.get(3)?,
                    feed_title: r.get(4)?,
                    title: r.get(5)?,
                    published_at: time(r, 6)?,
                    read_at: optional_time(r, 7)?,
                    starred_at: optional_time(r, 8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let overflow = self
            .conn()
            .prepare(
                "SELECT section, feed_id, feed_title, page, hidden_count FROM v_edition_overflow \
                 ORDER BY CASE section WHEN 'front' THEN 0 WHEN 'week' THEN 1 ELSE 2 END, \
                 page IS NULL, page, feed_id",
            )?
            .query_map([], |r| {
                Ok(EditionOverflowRow {
                    section: section(r, 0)?,
                    feed_id: r.get(1)?,
                    feed_title: r.get(2)?,
                    page: r.get(3)?,
                    hidden_count: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(EditionView { edition, entries, overflow }))
    }
}
```

Notes for the implementer:
- `latest(&tx)` works because `rusqlite::Transaction` derefs to `Connection`; `candidates(&tx, …)` likewise. Everything between `write_tx` and `commit` runs under the IMMEDIATE lock, so the due re-check and the insert are atomic against another process.
- `edition_view` is the store's name; the service exposes it as `Ressac::edition_today` (plan index).
- If the test module's `use super::*` does not bring `Error`, `CadenceGroup`, `Composition` and `ts` into scope, they are all imported at the top of the file, so it does.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core store::edition::tests`
Expected: PASS, 14 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/edition.rs crates/ressac-core/src/store/mod.rs
git commit -m "Core: select, write and read editions in one transaction

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2b.2: `Event::Edition` and the report types

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/ressac-core/src/service.rs`, after `page_events_have_stable_json`:

```rust
    #[test]
    fn edition_events_have_stable_json() {
        let event = Event::Edition {
            id: 12,
            entries: 43,
            front: 2,
            week: 7,
            wire: 34,
            overflow: 61,
            unreachable: 3,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"event":"edition","id":12,"entries":43,"front":2,"week":7,"wire":34,"overflow":61,"unreachable":3}"#
        );
    }

    #[test]
    fn an_edition_report_counts_sections_and_hidden_entries() {
        use crate::edition::{Composition, Overflow, Placed, Section};
        let record = crate::store::EditionRecord {
            id: 4,
            built_at: Utc::now(),
            window_start: Utc::now(),
            window_end: Utc::now(),
            unreachable: 1,
        };
        let comp = Composition {
            entries: vec![
                Placed { entry_id: 1, section: Section::Front, rank: 1 },
                Placed { entry_id: 2, section: Section::Wire, rank: 2 },
                Placed { entry_id: 3, section: Section::Wire, rank: 3 },
            ],
            overflow: vec![
                Overflow { feed_id: 1, section: Section::Wire, hidden_count: 4 },
                Overflow { feed_id: 2, section: Section::Week, hidden_count: 2 },
            ],
        };
        let report = EditionReport::new(&record, &comp);
        assert_eq!(
            report,
            EditionReport { id: 4, entries: 3, front: 1, week: 0, wire: 2, overflow: 6, unreachable: 1 }
        );
        assert_eq!(
            report.event(),
            Event::Edition { id: 4, entries: 3, front: 1, week: 0, wire: 2, overflow: 6, unreachable: 1 }
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core service::tests::edition`
Expected: compile errors, `no variant named Edition`, `cannot find type EditionReport`.

- [ ] **Step 3: Write the implementation**

In `crates/ressac-core/src/service.rs`, add the variant at the end of `enum Event`, after `Page(PageRow),`:

```rust
    /// `ressac edition build`: the edition just written.
    Edition {
        id: i64,
        entries: usize,
        front: usize,
        week: usize,
        wire: usize,
        overflow: u32,
        unreachable: u32,
    },
```

Extend the imports at the top of `service.rs`:

```rust
use crate::edition::{Caps, Composition, Section, day_start};
use crate::error::{Error, Result};
use crate::store::{CadenceRow, EditionParams, EditionRecord, FeedRow, ImportReport, Store};
pub use crate::store::{EditionEntryRow, EditionOverflowRow, EditionView};
```

(replacing the existing `use crate::error::Result;` line and the `use crate::store::{CadenceRow, FeedRow, ImportReport, Store};` line that Phase 1c left. If `Error` is already imported by then, keep a single import.)

Add the types after `impl Summary { … }`:

```rust
/// What `Ressac::build_edition` is asked to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditionOptions {
    pub caps: Caps,
    /// Replace today's edition, keeping its window start.
    pub rebuild: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditionReport {
    pub id: i64,
    pub entries: usize,
    pub front: usize,
    pub week: usize,
    pub wire: usize,
    /// Sum of the overflow counts: eligible entries the caps left in their feeds.
    pub overflow: u32,
    /// Feeds whose fetch failed during the build's refresh.
    pub unreachable: u32,
}

impl EditionReport {
    pub fn new(record: &EditionRecord, comp: &Composition) -> Self {
        let count = |s: Section| comp.entries.iter().filter(|p| p.section == s).count();
        EditionReport {
            id: record.id,
            entries: comp.entries.len(),
            front: count(Section::Front),
            week: count(Section::Week),
            wire: count(Section::Wire),
            overflow: comp.overflow.iter().map(|o| o.hidden_count).sum(),
            unreachable: record.unreachable,
        }
    }

    pub fn event(&self) -> Event {
        Event::Edition {
            id: self.id,
            entries: self.entries,
            front: self.front,
            week: self.week,
            wire: self.wire,
            overflow: self.overflow,
            unreachable: self.unreachable,
        }
    }
}

/// How a build ended. Only `Built` wrote anything.
#[derive(Debug, Clone, PartialEq)]
pub enum EditionOutcome {
    Built(EditionReport),
    /// An edition already exists since today's 04:00 (and `rebuild` was not asked).
    NotDue,
    /// The refresh found the network unreachable: no edition, the window stays where it was.
    Offline,
    /// The stop flag was raised during the refresh: no edition.
    Interrupted,
}
```

`EditionOptions` derives `Default` through `Caps: Default`; `Caps` derives `PartialEq, Eq` (2a), which is what the derive needs.

`Event` is matched exhaustively by the CLI's progress display (`crates/ressac/src/output.rs`, `Display::Progress(bar) => match event { … }`), and `cargo test --workspace` compiles that crate. Add the new variant to its no-op arm:

```rust
                Event::WaybackDone { .. }
                | Event::Finished { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. }
                | Event::Page(_)
                | Event::Edition { .. } => {}
```

The `--json` sink serialises every `Event` through serde, so `{"event":"edition",…}` needs nothing more there; the text summary line is Phase 3a's.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core --lib -- service::tests::edition_events_have_stable_json service::tests::an_edition_report_counts_sections_and_hidden_entries`
Expected: PASS, 2 tests. `cargo build -p ressac-core` warns that `day_start`, `EditionParams` and `Error` are unused: they are used by Task 2b.3; do not commit with `-D warnings` in between, go straight to 2b.3 (the commit below is made with the gate of 2b.3).

- [ ] **Step 5: No commit yet**

The imports above are only used by `build_edition`; this task is committed together with Task 2b.3 so that every commit passes clippy `-D warnings`.

### Task 2b.3: `Ressac::build_edition` and `Ressac::edition_today` (Review Focus 4)

- [ ] **Step 1: Write the failing tests**

Create `crates/ressac-core/tests/edition.rs`:

```rust
//! `Ressac::build_edition` against a local mock web: refresh, then compose and store the edition.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use ressac_core::edition::{Caps, Section};
use ressac_core::service::{EditionOptions, EditionOutcome, Event, EventSink, Ressac};
use ressac_core::source::FetchLimits;
use ressac_core::store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl EventSink for Recorder {
    fn emit(&self, event: &Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

/// Raises the stop flag as soon as the refresh starts, as Ctrl-C or quitting the app would.
struct StopOnStart {
    stop: Arc<AtomicBool>,
}

impl EventSink for StopOnStart {
    fn emit(&self, event: &Event) {
        if matches!(event, Event::Started { .. }) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }
}

fn rss(title: &str, items: &[(&str, DateTime<Utc>)]) -> String {
    let items: String = items
        .iter()
        .map(|(id, d)| {
            format!(
                "<item><guid>{id}</guid><title>Entry {id}</title><link>http://example.org/{id}</link><pubDate>{}</pubDate></item>",
                d.to_rfc2822()
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><link>http://example.org/</link><description>A feed</description>{items}</channel></rss>"#
    )
}

async fn serve(server: &MockServer, at: &str, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(template)
        .mount(server)
        .await;
}

fn ok_rss(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "application/rss+xml")
}

fn limits() -> FetchLimits {
    FetchLimits {
        per_host_gap: Duration::ZERO,
        timeout: Duration::from_secs(5),
        ..FetchLimits::default()
    }
}

fn go() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn hours(h: i64) -> chrono::Duration {
    chrono::Duration::hours(h)
}

/// Three feeds: Daily (entries a day apart), Weekly (six days apart), and a Silent feed that
/// wakes up (one entry now, the previous one 800 days ago), plus a gone feed when `with_gone`.
async fn morning_app(server: &MockServer, with_gone: bool) -> Ressac {
    let base = server.uri();
    let now = Utc::now();
    serve(
        server,
        "/daily.xml",
        ok_rss(rss("Daily", &[("d1", now - hours(1)), ("d2", now - hours(25)), ("d3", now - hours(49))])),
    )
    .await;
    serve(
        server,
        "/weekly.xml",
        ok_rss(rss("Weekly", &[("w1", now - hours(2)), ("w2", now - hours(6 * 24)), ("w3", now - hours(12 * 24))])),
    )
    .await;
    serve(
        server,
        "/wakes.xml",
        ok_rss(rss("Wakes", &[("s1", now - hours(3)), ("s0", now - hours(800 * 24))])),
    )
    .await;
    serve(server, "/gone.xml", ResponseTemplate::new(410)).await;
    let gone = if with_gone {
        format!(r#"<outline text="Gone" xmlUrl="{base}/gone.xml"/>"#)
    } else {
        String::new()
    };
    let opml = format!(
        r#"<opml><body><outline text="Infos">
        <outline text="Daily" xmlUrl="{base}/daily.xml"/>
        <outline text="Weekly" xmlUrl="{base}/weekly.xml"/>
        <outline text="Wakes" xmlUrl="{base}/wakes.xml"/>{gone}
        </outline></body></opml>"#
    );
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), &base).unwrap();
    app.import_opml(&opml, false).unwrap();
    app
}

fn built(outcome: EditionOutcome) -> ressac_core::service::EditionReport {
    match outcome {
        EditionOutcome::Built(r) => r,
        other => panic!("expected an edition, got {other:?}"),
    }
}

#[tokio::test]
async fn a_morning_build_composes_sections_and_ends_with_the_edition_event() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    let recorder = Recorder::default();
    let report = built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &recorder, go())
            .await
            .unwrap(),
    );
    // Wakes → Rare → front page (1); Weekly → w1 and w2 (w3 arrived read: older than 7 days on
    // first fill); Daily → d1 only, d2 and d3 overflow.
    assert_eq!((report.front, report.week, report.wire), (1, 2, 1));
    assert_eq!(report.entries, 4);
    assert_eq!(report.overflow, 2);
    assert_eq!(report.unreachable, 0);
    let events = recorder.0.lock().unwrap();
    assert!(matches!(events.first(), Some(Event::Started { .. })));
    assert_eq!(events.last(), Some(&report.event()));

    let view = app.edition_today().unwrap().unwrap();
    assert_eq!(view.edition.id, report.id);
    assert_eq!(view.entries[0].section, Section::Front);
    assert_eq!(view.entries[0].feed_title, "Wakes");
    assert_eq!(view.entries.iter().map(|e| e.rank).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(view.overflow.len(), 1);
    assert_eq!(view.overflow[0].hidden_count, 2);
}

#[tokio::test]
async fn a_second_build_the_same_morning_does_not_even_refresh() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    let recorder = Recorder::default();
    let again = app
        .build_edition(EditionOptions::default(), Utc::now(), &Utc, &recorder, go())
        .await
        .unwrap();
    assert_eq!(again, EditionOutcome::NotDue);
    assert!(recorder.0.lock().unwrap().is_empty(), "no refresh when nothing is due");
}

#[tokio::test]
async fn the_next_morning_starts_where_the_previous_edition_ended() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    let first = app.store().latest_edition().unwrap().unwrap();
    let tomorrow = Utc::now() + chrono::Duration::days(1);
    let second = built(
        app.build_edition(EditionOptions::default(), tomorrow, &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    let record = app.store().latest_edition().unwrap().unwrap();
    assert_eq!(record.window_start, first.window_end);
    assert_eq!(record.window_end, tomorrow);
    // The feeds were fetched minutes ago: nothing new, an empty edition, the window still moves.
    assert_eq!(second.entries, 0);
    assert!(app.edition_today().unwrap().unwrap().entries.is_empty());
}

#[tokio::test]
async fn a_failed_feed_is_counted_and_the_edition_is_built_anyway() {
    let server = MockServer::start().await;
    let app = morning_app(&server, true).await;
    let report = built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    assert_eq!(report.unreachable, 1);
    assert_eq!(app.edition_today().unwrap().unwrap().edition.unreachable, 1);
}

#[tokio::test]
async fn an_unreachable_network_builds_nothing() {
    let opml = r#"<opml><body>
        <outline text="A" xmlUrl="http://a.ressac-test.invalid/feed"/>
        <outline text="B" xmlUrl="http://b.ressac-test.invalid/feed"/>
        <outline text="C" xmlUrl="http://c.ressac-test.invalid/feed"/>
        <outline text="D" xmlUrl="http://d.ressac-test.invalid/feed"/>
        </body></opml>"#;
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), "http://127.0.0.1:9").unwrap();
    app.import_opml(opml, false).unwrap();
    let outcome = app
        .build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
        .await
        .unwrap();
    assert_eq!(outcome, EditionOutcome::Offline);
    assert_eq!(app.store().latest_edition().unwrap(), None);
}

/// Review Focus 4: a build stopped during its refresh writes nothing, and the next build gets
/// the whole window.
#[tokio::test]
async fn a_stopped_build_writes_nothing_and_the_next_one_builds() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    let stop = go();
    let outcome = app
        .build_edition(EditionOptions::default(), Utc::now(), &Utc, &StopOnStart { stop: stop.clone() }, stop)
        .await
        .unwrap();
    assert_eq!(outcome, EditionOutcome::Interrupted);
    assert_eq!(app.store().latest_edition().unwrap(), None);

    built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    let record = app.store().latest_edition().unwrap().unwrap();
    assert_eq!(record.window_start, record.window_end - chrono::Duration::hours(24));
}

#[tokio::test]
async fn a_rebuild_tries_other_caps_on_the_same_window() {
    let server = MockServer::start().await;
    let app = morning_app(&server, false).await;
    built(
        app.build_edition(EditionOptions::default(), Utc::now(), &Utc, &Recorder::default(), go())
            .await
            .unwrap(),
    );
    let first = app.store().latest_edition().unwrap().unwrap();
    let opts = EditionOptions { caps: Caps { week: 3, wire: 0 }, rebuild: true };
    let report = built(app.build_edition(opts, Utc::now(), &Utc, &Recorder::default(), go()).await.unwrap());
    assert_eq!(report.wire, 0);
    assert_eq!(report.overflow, 3, "every daily entry left in its feed");
    let second = app.store().latest_edition().unwrap().unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.window_start, first.window_start);
}

#[tokio::test]
async fn no_edition_yet() {
    let app = Ressac::with_parts(Store::open_in_memory().unwrap(), limits(), "http://127.0.0.1:9").unwrap();
    assert_eq!(app.edition_today().unwrap(), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core --test edition`
Expected: compile errors, `no method named build_edition found for struct Ressac`, `no method named edition_today`.

- [ ] **Step 3: Write the implementation**

In `crates/ressac-core/src/service.rs`, add a counting sink after `impl EventSink for NullSink { … }`:

```rust
/// Passes every event on and counts the feeds whose fetch failed.
struct FailureCounter<'a> {
    inner: &'a dyn EventSink,
    failed: std::sync::atomic::AtomicU32,
}

impl EventSink for FailureCounter<'_> {
    fn emit(&self, event: &Event) {
        if let Event::FeedDone { error: Some(_), .. } = event {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        self.inner.emit(event);
    }
}
```

Add the two methods in `impl Ressac`, just before `pub fn pages`:

```rust
    /// The morning edition (spec §6): due check (skipped with `rebuild`), a refresh of the due
    /// feeds, then one transaction that composes and stores the edition. `now` and `tz` decide
    /// the day (04:00 local); the window ends at the later of `now` and the real clock after
    /// the refresh, since the refresh stamps `fetched_at` with the real clock.
    pub async fn build_edition(
        &self,
        opts: EditionOptions,
        now: DateTime<Utc>,
        tz: &impl chrono::TimeZone,
        sink: &dyn EventSink,
        stop: Arc<AtomicBool>,
    ) -> Result<EditionOutcome> {
        let day = day_start(now.with_timezone(tz));
        if !opts.rebuild && !self.store.edition_due(day)? {
            return Ok(EditionOutcome::NotDue);
        }
        let counter = FailureCounter {
            inner: sink,
            failed: std::sync::atomic::AtomicU32::new(0),
        };
        let summary = self
            .refresh(RefreshOptions::default(), &counter, stop.clone())
            .await?;
        if stop.load(Ordering::Relaxed) {
            return Ok(EditionOutcome::Interrupted);
        }
        if summary.network_unreachable {
            return Ok(EditionOutcome::Offline);
        }
        let end = Utc::now().max(now);
        let params = EditionParams {
            day_start: day,
            window_end: end,
            built_at: end,
            caps: opts.caps,
            unreachable: counter.failed.load(Ordering::Relaxed),
            rebuild: opts.rebuild,
        };
        let (record, composition) = match self.store.write_edition(&params) {
            Ok(written) => written,
            // Another process built it while this one was refreshing.
            Err(Error::EditionNotDue) => return Ok(EditionOutcome::NotDue),
            Err(e) => return Err(e),
        };
        let report = EditionReport::new(&record, &composition);
        tracing::info!(id = report.id, entries = report.entries, "edition built");
        sink.emit(&report.event());
        Ok(EditionOutcome::Built(report))
    }

    /// The latest edition with its entries and overflow; `None` before the first one.
    pub fn edition_today(&self) -> Result<Option<EditionView>> {
        self.store.edition_view()
    }
```

`day_start` is computed before the first `.await`, so the generic `tz` is not held across await points beyond its reference; the CLI's `#[tokio::main]` future is driven with `block_on` and does not need to be `Send`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core --test edition`
Expected: PASS, 8 tests.

Run: `cargo test -p ressac-core service::tests`
Expected: PASS, including `edition_events_have_stable_json` and `an_edition_report_counts_sections_and_hidden_entries`.

- [ ] **Step 5: Gate and commit**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Expected: all green. If `fmt` reports diffs (long test lines), run `cargo fmt --all` and re-run the gate.

```bash
git add crates/ressac-core/src/service.rs crates/ressac-core/tests/edition.rs crates/ressac/src/output.rs
git commit -m "Core: build the morning edition after a refresh (offline, stopped, not due)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Sub-phase 2b ends here: stop for Clément's approval.**

---

## Spec coverage of this phase

| Spec item | Where |
|---|---|
| §6 When: due since 04:00 local, re-check in the write transaction | 2a.2 `day_start`; 2b.1 `edition_due`, `a_second_build_the_same_morning_is_refused` |
| §6 Window: previous `window_end`, first = 24 h | 2b.1 `the_first_edition_covers_the_last_24_hours`, `entries_fetched_after_the_build_go_to_the_next_edition` |
| §6 Eligibility: `fetched_at`, unread, 7-day guard, buried excluded | 2b.1 `only_unread_entries_of_active_non_silent_feeds_within_the_guard` |
| §6 Sections and caps, per edition, overflow | 2a.1 `caps_apply_per_feed…`; 2b.1 `a_monday_edition_spans_the_weekend…` |
| §6 A silent feed that wakes up reaches the front page | 2b.3 `a_morning_build_composes_sections…` (feed "Wakes") |
| §6 Order and rank | 2a.1 `sections_come_in_order…`, `feeds_follow_page_numbers…`; 2b.1 `the_view_lists_entries_by_rank…` |
| §6 Empty edition advances the window | 2b.1 `an_empty_edition_is_stored…`; 2b.3 `the_next_morning_starts…` |
| §4 Override wins (effective group) | 2b.1 `an_override_wins_over_the_computed_group` |
| §7 `--rebuild` keeps `window_start` | 2b.1 `a_rebuild_replaces_today…`; 2b.3 `a_rebuild_tries_other_caps…` |
| §9 Offline, some feeds fail, two builds, quit during the build | 2b.3 `an_unreachable_network…`, `a_failed_feed_is_counted…`, `a_second_build…`, `a_stopped_build_writes_nothing…` |
| Review Focus 1, 2, 3, 4 (core half) | 2a.2; 2b.1 (2 and 3); 2b.3 (4) |
