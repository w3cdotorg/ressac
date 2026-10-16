# Ressac — Sub-project 3: cadence and the morning edition

- **Date:** 2026-10-16
- **Status:** Draft, awaiting review
- **Scope:** publication-rhythm groups for every feed (core, Mac app in Native and Antiope, CLI) and a daily morning edition composed and stored by the core. The terminal UI shows both with sub-project 2b, through the views defined here.
- **Builds on:** `docs/specs/2026-10-10-core-and-garden-design.md` (*the core spec*), `docs/specs/2026-10-10-mac-app-design.md` (*the app spec*) and `docs/specs/2026-10-14-antiope-design.md` (*the Antiope spec*), all merged.
- **Order agreed with the user:** 3 (this spec), then 2b (terminal reading).

## 1. Goal

A finished morning paper. Once a day, the first time Clément opens Ressac.app, the app refreshes every feed and the core composes an edition: what appeared since the previous edition, rare feeds on the front page, chatty feeds capped to their latest entry. He reads it from top to bottom and reaches its end. What the caps left out stays unread in its feed.

Alongside it, every feed gets a rhythm group (Daily, Weekly, Monthly, Rare, Silent), computed by the core and shown as reading lists in the sidebar, in the inspector and in the garden.

Success looks like: Clément opens Ressac.app on a Monday morning, watches "Building this morning's edition… 34/156", reads about 45 entries under Front Page, This Week and The Wire, opens "+12 more in Actualitté" once, reaches "That's the edition.", then forces The Globe and Mail to Daily in the inspector and sees it move in the sidebar's Rhythm section. In Antiope, he types `104`, reads the edition as sub-pages `104 1/45` … `104 45/45` and lands on FIN DE L'EDITION.

## 2. Decisions taken with the user (2026-10-16)

- **The edition is finished, not a live view (option A).** It is frozen when built; later refreshes do not change it.
- **Built on the first app launch of the day (option B).** No LaunchAgent, no background refresh. A day starts at 04:00 local time.
- **Sections by rhythm (option A):** Front Page (Monthly and Rare feeds, uncapped), This Week (Weekly, capped), The Wire (Daily, capped). Entries left out stay unread in their feed.
- **Rhythm outside the edition (option C):** rhythm groups as sidebar reading lists *and* rhythm shown in the inspector and Garden › All feeds.
- **Only today's edition is shown (option A).** Every edition stays in the database (the next window needs the previous one's end), so browsing past editions can come later without a schema change.
- **Automatic classification with a per-feed override (option A).**
- **Architecture (approach 1):** the core composes and stores the edition; the app runs `ressac edition build` as a subprocess and reads contract views, as it already does for `fetch`. Rejected: composing in Swift (the terminal would need a second copy of the rules) and a SQL-only edition (a view cannot be frozen).
- **Language:** every user-facing string is in English, section titles included. **One exception, in Antiope only:** the end-of-edition line reads `FIN DE L'EDITION`, in the spirit of the Minitel key labels.

## 3. Scope

### In scope

**Core (Rust), schema v4:**

| Item | Purpose |
|---|---|
| Rhythm groups | Classification from the median interval (§4), stored in `feed_health`, recomputed with health. |
| `entries_per_month` fix | Measured over the span the stored entries actually cover (§4). |
| Per-feed override | `feed.cadence_override`, set from the app or the CLI. |
| Schema v4 migration | Columns, tables `edition`, `edition_entry`, `edition_overflow`, views `v_cadence`, `v_edition_today`, index on `entry.fetched_at` (§5). |
| Edition builder | Window, eligibility, sections, caps, order (§6). |
| Reserved pages 104–109 | JOURNAL and the five rhythm groups. |
| CLI | `ressac edition build`, `ressac edition show`, `ressac feed cadence` (§7). |

**App (Swift):**

- Sidebar: Journal at the top, a Rhythm section with the five groups.
- Journal screen: section headings, overflow rows, progress line, end line, mark-edition-read.
- Edition build on launch, with progress in the existing refresh banner, and the offline state.
- Inspector: rhythm line and override menu. Garden › All feeds: Rhythm column.
- Antiope: P104 JOURNAL with sub-pages, P105–P109, SOMMAIRE entries, build progress, rhythm line in the Antiope inspector.

### Out of scope

A LaunchAgent or any background refresh; notifications or a Dock badge; browsing past editions; caps or sections edited from the app; per-feed "always on the front page" flags (a Rare override covers it); AI ranking or summaries; the terminal UI (2b).

## 4. Core: rhythm groups

### Classification

Computed in `health/cadence.rs` from the same input as today (at most the 21 most recent dated entries, fallback dates only when a feed has nothing else):

| Group | Rule (first match wins) |
|---|---|
| `silent` | no entry at all, or the latest entry is more than 365 days old |
| `daily` | median interval < 1.5 days |
| `weekly` | median interval < 10 days |
| `monthly` | median interval < 45 days |
| `rare` | median interval ≥ 45 days, or a single entry within the last 365 days (no interval to measure) |

Thresholds are constants in `cadence.rs`. Measured on the real database on 2026-10-16 (156 active feeds): 57 daily, 28 weekly, 17 monthly, 4 rare, 50 silent. Feeds that post in batches (photo uploads, median 0 days) land in `daily` and are therefore capped; this is intended.

- `Cadence` gains `group: CadenceGroup`. `diagnose` stores it in `feed_health.cadence_group` with the other health fields, so every refresh and every `check` recomputes it.
- Buried feeds keep their last stored group but are excluded from every rhythm list and from the edition.
- **Effective group** = `feed.cadence_override` if set, else `feed_health.cadence_group`. Only the effective group is used by the sidebar, the pages and the edition.

### `entries_per_month` fix

Today it counts stored entries of the last 365 days and divides by 12, so a feed imported on 2026-10-10 that exposes 20 items reads as 1.7 per month whatever it publishes (The Globe and Mail: 8.3). New rule: entries of the last 365 days divided by the months they span (from the oldest of them to `now`, at least one month). It stays informative only; classification never reads it.

### Override

- `feed.cadence_override`: `NULL` (automatic) or one of the five group names.
- Set with `store::feeds::set_cadence_override(feed_id, Option<CadenceGroup>)`; refused for an unknown or buried feed.
- An override is never cleared automatically.

## 5. Core: schema v4 (`schema/0004_edition.sql`)

Migration v3 → v4 exactly as v3: backup next to the database first (`ressac.sqlite.bak-v3`), one transaction, rollback on failure.

```sql
ALTER TABLE feed ADD COLUMN cadence_override TEXT
  CHECK (cadence_override IN ('daily','weekly','monthly','rare','silent'));
ALTER TABLE feed_health ADD COLUMN cadence_group TEXT NOT NULL DEFAULT 'silent';

CREATE INDEX entry_fetched ON entry(fetched_at);

CREATE TABLE edition (
  id           INTEGER PRIMARY KEY,
  built_at     TEXT NOT NULL,   -- UTC
  window_start TEXT NOT NULL,   -- UTC, exclusive
  window_end   TEXT NOT NULL,   -- UTC, inclusive; the next edition's window_start
  unreachable  INTEGER NOT NULL DEFAULT 0  -- feeds whose fetch failed during the build
);

CREATE TABLE edition_entry (
  edition_id INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  entry_id   INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
  section    TEXT NOT NULL CHECK (section IN ('front','week','wire')),
  rank       INTEGER NOT NULL,  -- 1-based, across the whole edition
  PRIMARY KEY (edition_id, entry_id)
);

CREATE TABLE edition_overflow (
  edition_id   INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  feed_id      INTEGER NOT NULL REFERENCES feed(id) ON DELETE CASCADE,
  section      TEXT NOT NULL CHECK (section IN ('front','week','wire')),
  hidden_count INTEGER NOT NULL,
  PRIMARY KEY (edition_id, feed_id)
);
```

The migration fills `cadence_group` by running the classification on every feed, so the app never sees the default.

**Contract views** (the app and the future TUI read only these):

- `v_cadence(feed_id, title, page, folder_path, effective_group, computed_group, is_override, median_interval_days, entries_per_month, last_entry_at, unread)`, active feeds only.
- `v_edition_today(edition_id, built_at, window_start, window_end, unreachable, entry_id, section, rank, feed_id, feed_title, title, published_at, read_at, starred_at)`: the latest edition with one row per entry, live read and starred state.
- `v_edition_overflow(edition_id, section, feed_id, feed_title, page, hidden_count)` for the latest edition.

**Reserved pages** (`store/pages.rs`, `RESERVED_PAGES` grows from 8 to 14):

| Page | Destination |
|---|---|
| 104 | JOURNAL (today's edition) |
| 105 | Daily |
| 106 | Weekly |
| 107 | Monthly |
| 108 | Rare |
| 109 | Silent |

No feed or folder holds one of these numbers today (feeds without a folder start at 110). The migration checks it anyway and moves any holder to the lowest free number, as the assignment rules already do.

## 6. Core: building an edition

### When

- An edition is **due** when no edition has `built_at` at or after today's 04:00 local time (yesterday's 04:00 before 04:00).
- `service::build_edition` runs a full refresh (the existing `refresh`, which diagnoses every active feed and so updates the groups), then composes and writes the edition in one transaction. The "is it due" check is repeated inside that transaction; if another process wrote an edition meanwhile, it stops with a refusal (§9).

### Window

- `window_start` = the previous edition's `window_end`; for the very first edition, `window_end − 24 h`.
- `window_end` = the time the refresh finished.

### Eligibility

An entry is eligible when all three hold:

1. `fetched_at` is in `(window_start, window_end]`. First-seen time, not `published_at`: some feeds backdate. `fetched_at` is set on insert only, so an edited entry never comes back.
2. It is unread when the edition is built.
3. `published_at` ≥ `window_start − 7 days`, so a feed just added, or one that republishes its archive, does not flood the edition. (The fetcher already marks a new feed's entries older than 7 days as read; this guard also covers older feeds that republish.)

Entries of buried feeds are never eligible.

### Sections and caps

| Section | Effective groups | Cap per feed |
|---|---|---|
| `front` (Front Page) | `monthly`, `rare` | none |
| `week` (This Week) | `weekly` | 3, most recent first |
| `wire` (The Wire) | `daily` | 1, the most recent |

- A `silent` feed that publishes again is reclassified by the refresh that precedes the composition (one recent entry → `rare`), so it reaches the front page. A feed forced to `silent` never appears.
- Caps apply **per edition**, not per day: a Monday edition covering the weekend has the same shape as any other, with larger overflows.
- Each eligible entry beyond a cap adds to that feed's `edition_overflow.hidden_count`. Overflow entries stay unread and untouched.
- Caps are constants (`CAP_WEEK = 3`, `CAP_WIRE = 1`) that `edition build` can override for a trial (§7).

### Order

Sections in the order front, week, wire. Inside a section, feeds by page number (which follows the folder order), then entries by `published_at` descending. `rank` numbers the whole edition from 1, which gives the Antiope sub-page numbers directly.

Measured on the week before 2026-10-14: daily feeds publish about 134 entries a day; with a cap of 1 this gives about 35, plus about 7 weekly entries and the front page, so an edition of 40 to 45 entries.

### Empty edition

When nothing is eligible, the edition is written anyway with no entries. The window advances.

## 7. CLI

- `ressac edition build [--cap-week N] [--cap-wire N] [--rebuild]`
  - Emits the NDJSON events of `fetch` (started, feedDone, networkUnreachable, finished), then one `edition` event: `{"event":"edition","id":…,"entries":…,"front":…,"week":…,"wire":…,"overflow":…,"unreachable":…}`.
  - Exit `2` (refusal) when no edition is due, unless `--rebuild`.
  - `--rebuild` deletes today's edition and builds again with the same `window_start`; entries read since then are no longer eligible. Meant for trying caps.
  - Exit `2` with no edition written when the network is unreachable (§9).
- `ressac edition show [--json]`: today's edition by section, `rank page title`, overflow lines, then a summary line. `--json` emits one NDJSON object per row of `v_edition_today`, then one per overflow.
- `ressac feed cadence <id> auto|daily|weekly|monthly|rare|silent`: sets or clears the override. Exit `2` for an unknown or buried feed.
- `ressac page list` shows 104–109 among the reserved pages; `ressac page set` refuses them (exit `2`), as for the other reserved pages.

## 8. App

### Native

**Sidebar.** `Journal` (symbol `newspaper`) at the top, above Unread, badge = unread entries of today's edition. A **Rhythm** section after the folders and before the Garden: Daily, Weekly, Monthly, Rare, Silent, each a reading list of the entries of its feeds with an unread badge. Silent stays listed at zero.

**Journal screen (entry list).**

- Header line: `Friday 16 October · 43 entries · 12 read`, plus `· 3 feeds unreachable` when `unreachable > 0`.
- Section headings `Front Page`, `This Week`, `The Wire`; a section without entries is omitted.
- Overflow row, greyed, under the last entry of its feed: `+12 more in Actualitté`. A click selects that feed in the sidebar.
- Reading behaves as everywhere: `j` / `k`, `Space` moves on to the next unread entry of the edition, `m`, `s`. Read state is the entry's own, so reading here marks it read everywhere.
- `⇧⌘A` marks the edition's entries read, never the overflow.
- When every entry is read: a closing line `That's the edition.`
- Empty edition: `Nothing new this morning.`

**Build on launch.** At launch, when the app comes back to the foreground, and at the 04:00 rollover while it runs (the same mechanism that already rolls Today over at midnight in `AppState`, moved into `JournalModel` for the edition), the app asks the core whether an edition is due (a read of `edition` through GRDB). If so, it starts `ressac edition build` as a long run through `RunCoordinator`. The refresh banner reads `Building this morning's edition… 34/156`. Until the `edition` event, the Journal shows that line; every other list stays usable. A second launch or a manual `⇧⌘R` during the build follows the coordinator's existing rule for long runs.

**Inspector.** A rhythm line `Rhythm: Daily · median 0.4 d · last entry 2 h ago` and a menu `Auto / Daily / Weekly / Monthly / Rare / Silent`; when overridden, the line ends with `(set by you)`. The menu runs `ressac feed cadence` as a short run.

**Garden › All feeds.** A sortable `Rhythm` column (effective group, override marked).

### Antiope

- **P104 JOURNAL.** In the entry column, section headings coloured as the P800 status headings: `FRONT PAGE` yellow, `THIS WEEK` cyan, `THE WIRE` green. Overflow rows in blue, `+12 MORE IN ACTUALITTE`, opened with a click or `↩`.
- **Sub-pages.** An edition entry is a sub-page of P104 by `rank`: `104 3/43`. SUITE (`Space`, `j`) and RETOUR (`k`) walk the edition in order. After the last entry, a double-height `FIN DE L'EDITION` line.
- **P105–P109.** The rhythm groups, each shown like a folder page (its feeds, their unread counts).
- **P100 SOMMAIRE** lists 104–109 with the other reserved pages, in colour, with their counts. **P899 GUIDE** is unchanged (no new key).
- **Build progress.** The page number searches as during a refresh (decorative, off with Reduce Motion); the banner reads `EDITION IN PROGRESS 34/156`.
- **Inspector.** The rhythm line and the override menu use the existing Antiope garden controls.

### Data layer

- `Queries.swift`: the edition (`v_edition_today`, `v_edition_overflow`), the rhythm lists (`v_cadence` joined to `v_entry`), the "edition due" check.
- `Records.swift`: `EditionRow`, `EditionOverflow`, `CadenceRow`, `CadenceGroup`.
- `RessacCLI.swift`: `RunRequest.buildEdition`, `RunRequest.setCadence(feedId:, group:)`; `Events.swift` decodes the `edition` event.
- `AppState.swift` is already 823 lines: the Journal state goes in a new `JournalModel` file, not into `AppState`.

## 9. Error handling

| Case | Behaviour |
|---|---|
| Network unreachable (existing `networkUnreachable` event, every fetch failed) | No edition written, window unchanged, exit `2`. Native: `Offline: no edition yet` with a Retry button; Antiope: the same text in the banner, RÉPÉTITION retries. |
| Some feeds fail | The edition is built with what arrived; `edition.unreachable` counts the failures and the header shows them. Their entries arrive later with a later `fetched_at`, so the next edition picks them up. |
| Two builds at once (app and CLI) | The due check is repeated inside the write transaction; the second one exits `2` and the app simply reloads the Journal. |
| App quit during the build | The refresh's work is kept; no edition is written; the next launch sees an edition still due and builds it. |
| Build failure (database error) | Exit `1`; the app shows the existing error banner with Show Log; no partial edition (one transaction). |
| Override on an unknown or buried feed | Exit `2`, the app shows the CLI's sentence in the banner (existing refusal path). |
| Entry or feed deleted later | Cascades remove its edition rows; nothing deletes entries today, this is defensive. |

## 10. Testing

### Rust

- Classification: each threshold and its boundary, batch posting (median 0), one entry in a year, no entry, fallback dates only.
- `entries_per_month`: a feed imported recently with 20 entries over 10 days.
- Override: set, clear, refused on a buried or unknown feed; effective group in `v_cadence`.
- Composition, on a fixture database with a fixed clock:
  - window from the previous edition, first edition, Monday window;
  - unread only; the 7-day guard; buried feeds excluded;
  - caps per edition, overflow counts, caps from flags;
  - order and `rank`; a silent feed that wakes up lands on the front page;
  - empty edition advances the window.
- Due rule around 04:00 local time (before, after, timezone with DST change).
- Concurrency: a second build inside the same day is refused; `--rebuild` keeps `window_start`.
- Migration v3 → v4 on a v3 fixture; reserved-page holder moved; schema contract test and `tests/fixtures/contract.sqlite` regenerated.
- CLI: exit codes of `edition build`, `edition show`, `feed cadence`, `page set 104`.

### Swift (Swift Testing, on `contract.sqlite`)

- Journal model: sections, overflow rows placed under their feed, header counts, end line, empty edition, `⇧⌘A` limited to the edition, `Space` stays inside the edition.
- Sidebar: Journal badge, Rhythm counts, Silent at zero.
- Edition due check and the launch flow with a stubbed engine (due, not due, offline, refusal).
- Inspector override menu sends the right `RunRequest`.
- Antiope: `PageDirectory` resolves 104–109, sub-page numbering `104 n/N`, SOMMAIRE lists the new pages.

### Manual check before declaring done (with Clément)

1. Migration v4 tried first on a copy of the real database, then on the real one (backup `ressac.sqlite.bak-v3`).
2. First launch of the day: progress banner, then the Journal.
3. Read the edition to `That's the edition.`; open one overflow row.
4. Force a feed's rhythm in the inspector; see it move in the sidebar and in Garden › All feeds; set it back to Auto.
5. Second launch the same day: no new build.
6. Network off before 04:00 the next day, then launch: offline state, Retry once online.
7. Antiope: `104`, walk the sub-pages to FIN DE L'EDITION; `105`–`109`; SOMMAIRE.

### CI

The existing workflow runs the new Rust and Swift tests; no new job.

## 11. Phases

1. **Core rhythm, in memory:** `CadenceGroup`, classification in `cadence.rs`, `entries_per_month` fix; no schema change.
2. **Schema v4:** migration, `cadence_group` stored by `diagnose`, override in the store, views, reserved pages, contract fixture.
3. **Edition builder:** window, eligibility, sections, caps, order, due rule.
4. **CLI:** `edition build|show`, `feed cadence`, events.
5. **App data layer and Native:** queries, records, run requests, `JournalModel`, sidebar, Journal screen, launch flow, inspector, garden column.
6. **Antiope:** P104 sub-pages, P105–P109, SOMMAIRE, progress, inspector.
7. **Acceptance:** migration of the real database, manual check, docs (`README.md`, `TODO.md`, `notes.md`).

## 12. Risks and open points

- **Thresholds and caps** come from one week of data; they are constants on purpose and `--rebuild` with flags lets us tune them after a few real mornings.
- **First launch is slow** (a full refresh, tens of seconds). Accepted with option B; the Journal says what is happening and the rest of the app stays usable.
- **App left open overnight:** the 04:00 rollover builds the edition while nobody watches; the window then ends at 04:00, which is what a morning paper does. A Mac asleep at 04:00 builds on wake, when the rollover fires late.
