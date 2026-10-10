# Ressac — Sub-project 1: Core & Garden

- **Date:** 2026-10-10
- **Status:** Draft, awaiting review
- **Scope:** first of four sub-projects (1. core & garden, 2. reading, 3. cadence & morning edition, 4. Antiope theme)

## 1. Goal

Turn a 15-year-old Vienna OPML (187 feeds, many dead, moved or hijacked) into a clean, trusted subscription list, and lay the shared core every later Ressac interface builds on.

Success looks like: `ressac import` + `ressac check` + a session in `ressac garden` produce a clean OPML export, with every removal backed by visible evidence and nothing deleted without an explicit decision.

## 2. Context and decisions already made

- **Product:** Ressac, a personal RSS/Atom/JSON Feed reader for macOS with two interfaces: a terminal UI (Rust, ratatui) and a native app (SwiftUI, sub-project 2). Retro teletext theme named **Antiope** (sub-project 4).
- **Users:** one person, one Mac. Sync with a service (FreshRSS, Miniflux, Feedbin) may come later; the design keeps a seam for it but does not build it.
- **Architecture (approach C):** a Rust core plus the terminal UI ship as one binary, `ressac`. The Swift app (later) reads the same SQLite database through GRDB and launches `ressac` as a subprocess to import and refresh. The versioned SQL schema is the contract between them. The core computes and stores everything "smart" (health, cadence, later editions and content kinds); interfaces only display and record reading state.
- **Language:** all user-facing strings in English. Code and docs in English. `notes.md` (the working log) stays in French.
- **Real-world fixture:** `~/Documents/vienna_20260415.opml`. Never committed (personal subscriptions); a hand-written fixture reproduces its traps (see §9).

## 3. Scope

### In scope

| Command | Purpose |
|---|---|
| `ressac import <file.opml> [--dry-run]` | Create the database if needed; import folders and feeds; merge exact duplicates; idempotent. |
| `ressac fetch [--force]` | Fetch all non-buried feeds politely; store entries; recompute health. |
| `ressac check [--force]` | `fetch` (same freshness rule), then Wayback lookups for Dead/Hijacked feeds, then a summary report. |
| `ressac garden` | Full-screen triage UI: keep, repair, bury. |
| `ressac export [-o file] [--cemetery]` | Write a clean OPML 2.0 (or, with `--cemetery`, the buried feeds only). |

Global flags: `--db <path>` (also `RESSAC_DB`), `--json` (NDJSON events on stdout), `--ascii` (no emoji), `-v` (log to stderr).

### Out of scope

Reading entries, the SwiftUI app, content-kind detection logic, cadence views and the morning edition, the Antiope theme, sync, configuration files.

## 4. Architecture

```
ressac/
├── Cargo.toml                 # workspace
├── crates/
│   ├── ressac-core/           # library, no terminal code
│   │   ├── store/             # SQLite access, migrations runner
│   │   ├── opml/              # import & export
│   │   ├── source/            # `Source` trait + LocalSource (HTTP fetching)
│   │   ├── parse/             # feed-rs wrapper, encoding, entry identity
│   │   ├── health/            # pure diagnosis rules + hijack scoring
│   │   └── wayback/           # archive.org availability client
│   └── ressac/                # binary: clap CLI, progress output, ratatui garden
├── schema/                    # 0001_init.sql, … — the Swift contract
├── tests/fixtures/            # hand-written OPML and feed samples
└── docs/specs/
```

**Unit boundaries:**

- `health` is a pure function: `diagnose(&FeedEvidence) -> Diagnosis`. No I/O, so the rules are table-tested.
- `source::Source` is the single entry point for new content:

  ```rust
  trait Source {
      async fn refresh(&self, feeds: &[FeedRef], events: &EventSink) -> Result<RefreshReport>;
  }
  ```

  `LocalSource` fetches over HTTP today. A future `FreshRssSource` replaces it without touching store, health or UI.
- `store` owns every SQL statement. Other modules never write SQL.
- The binary depends only on `ressac-core`'s public API.

**Main dependencies:** `tokio`, `reqwest` (rustls), `feed-rs`, `quick-xml`, `rusqlite` (bundled SQLite), `url`, `scraper` (feed autodiscovery), `whatlang`, `clap`, `ratatui` + `crossterm`, `indicatif`, `serde`/`serde_json`, `thiserror`, `anyhow` (binary only), `tracing` + `tracing-appender`. Tests: `wiremock`, `insta`, `tempfile`.

## 5. Data model

**Database:**

- **Location:** `~/Library/Application Support/Ressac/ressac.sqlite`, overridable with `--db` or `RESSAC_DB`.
- **Mode:** WAL, so the future Swift app can read while `ressac` writes.
- **Timestamps:** ISO 8601 text, UTC.

| Table | Key columns |
|---|---|
| `schema_meta` | `version`. A binary refuses a database whose version is newer than it knows. |
| `folder` | `id`, `parent_id` (nullable), `name`, `position`. |
| `feed` | Identity and placement: `id`, `folder_id`, `position`, `title`, `added_at`. URLs: `xml_url` (current), `xml_url_original`, `html_url`, `url_key` (normalized, unique among non-buried feeds), `final_url` (after last redirect chain). Descriptions: `description_original` (from OPML), `description_current` (from last parse). HTTP cache: `etag`, `last_modified`. Lifecycle: `state` (`active` / `buried`), `merged_into_id` (nullable), `opml_attrs` (JSON of all original outline attributes). Probe caches: `https_available` (nullable bool), `discovered_url`, `discovery_checked_at`, `wayback_url`. |
| `fetch_log` | `feed_id`, `run_id`, `at`, `http_status`, `error_kind`, `redirect_chain` (JSON list of `{status, url}`), `content_type`, `bytes`, `duration_ms`, `entry_count`, `not_modified`, `network_suspect`. Last 20 rows per feed kept. |
| `entry` | `id`, `feed_id`, `guid` (unique per feed), `url`, `title`, `author`, `published_at`, `published_at_is_fallback`, `updated_at`, `summary_html`, `content_html`, `content_kind` (nullable, filled in sub-project 2), `read_at`, `starred_at`, `fetched_at`. |
| `feed_health` | `feed_id` (PK), `status`, `reason` (one English sentence), `evidence` (JSON), `remarks` (JSON list), `proposed_action` (JSON or null), `hijack_score`. Cadence: `last_entry_at`, `median_interval_days`, `entries_per_month`. Housekeeping: `computed_at`. |
| `garden_decision` | `feed_id`, `decision` (`keep` / `repair` / `bury`), `status_at_decision`, `action` (JSON of the applied repair, if any), `previous` (JSON of the feed fields it changed, for undo), `decided_at`, `note`. Latest row per feed wins; history kept for undo. |

**Contract views:** views prefixed `v_` are the stable API for other clients. Sub-project 1 ships `v_feed_overview`: feed, folder path, health, latest decision.

**Nothing is deleted.** Burying sets `state = 'buried'`. Merging a duplicate buries it with `merged_into_id` set. Entries stay. Export skips buried feeds.

**Migrations:**

- **Storage:** plain SQL files in `schema/`, embedded with `include_str!`.
- **Execution:** each one runs in its own transaction.
- **Backup:** before any migration of an existing database, the file is copied to `ressac.sqlite.bak-v<old>`.

## 6. Import and fetch

### OPML import

- **Parser:** `quick-xml`. Accepts OPML 1.0/2.0, nested outlines, name from `text` or `title`, multi-line descriptions and entities.
- **Escaping repair:** double-escaped ampersands in URLs (`&amp;amp;` → `&`) are repaired.
- **URL key:** lowercase host, strip leading `www.`, strip trailing `/`, ignore scheme. Used only for comparison; original URLs are kept.
- **Exact duplicates** (same `url_key`) collapse into the first occurrence. Same-name feeds with different URLs are **not** merged (Vienna's `(1)` suffixes are homonyms).
- **Idempotent:** re-importing the same file changes nothing, even after triage: a feed already exists when any feed, buried or not, has its URL key as current or original address. New feeds and folders go after the existing items of their folder.
- **`--dry-run`:** prints added / merged / skipped counts and lists, and writes nothing.

### Fetch

- **Concurrency:** at most 16 requests in flight, 1 per host, 1 s gap between requests to the same host.
- **Conditional requests:** `If-None-Match` / `If-Modified-Since`. A `304` is a success with no parsing.
- **Redirects:** followed manually, at most 5, and every hop is recorded (status + URL).
- **Limits:** 30 s total timeout, 10 MB body cap.
- **User-Agent:** `Ressac/<version>`. No browser impersonation; `403` is recorded as blocked.
- **Skips:** feeds fetched less than 30 min ago (unless `--force`) and buried feeds.
- **Parsing (`feed-rs`):** RSS 0.9x/1.0 (RDF)/2.0, Atom, JSON Feed.
  - **Charset:** read from the XML declaration or the HTTP header. ISO-8859-1 support is verified first in the implementation plan.
  - **HTML instead of a feed:** `HtmlInsteadOfFeed` error, recorded as evidence.
- **Entry identity:** `feed-rs` entry id: the feed's GUID/id when present, otherwise `feed-rs`'s stable hash of first link + title (verified on feed-rs 3.0.0).
- **Missing or unparseable dates:** use fetch time and set `published_at_is_fallback`.
- **Persistence:** one transaction per feed (log row + entries + health).
- **Health:** recomputed for each feed after its fetch.

### Autodiscovery and Wayback

- **Autodiscovery:** when a feed meets the Dead conditions, fetch its `html_url` once and read `<link rel="alternate" type="application/(rss|atom)+xml|application/feed+json">`. If it points to a different, parseable feed, propose it (status Moved).
- **Wayback (`check` only):** for Dead and Hijacked feeds, query `https://archive.org/wayback/available?url=<html_url>&timestamp=<last_entry_at>`, at 1 request per second. The closest snapshot URL is stored in `feed.wayback_url` (it survives re-diagnosis).

### Output

- **Human mode:** an `indicatif` progress bar, then a summary.
- **`--json` mode:** one JSON object per line on stdout:
  - `{"event":"started","total":187}`
  - `{"event":"feed_done","feed_id":…,"title":"…","error":null|"HTTP 404"}`
  - `{"event":"finished","summary":{…}}`: the diagnosis comes here, once every feed is fetched.

## 7. Diagnosis

`diagnose` returns one **primary status** plus secondary **remarks**. Evaluation order (first match wins) is also the garden's urgency order:

| # | Status | Rule |
|---|---|---|
| 1 | 👯 **Duplicate** | Another active feed has the same normalized `final_url`, **or** ≥ 80 % of this feed's last 20 entry GUIDs/links appear in another active feed. The older feed (lower `id`) is canonical; the other is the Duplicate. |
| 2 | 🦠 **Hijacked** | `hijack_score ≥ 0.6` (see below). |
| 3 | 🚚 **Moved** | The leading run of `301`/`308` hops ends on a different `url_key` that parses as a feed (a temporary hop ends the run), **or** the feed meets the Dead conditions below and autodiscovery found a different, parseable feed. |
| 4 | 🪦 **Dead** | Last attempt returned `404`/`410` or NXDOMAIN, **or** ≥ 5 consecutive failures spanning ≥ 14 days. Failures discounted by the network guard (§8) don't count. |
| 5 | 🤒 **Ailing** | Last attempt failed with a transient error (`5xx`, `429`, `403`, `Timeout`, `Tls`, `Parse`, `Encoding`, `TooLarge`). |
| 6 | 🦴 **Fossil** | Responding, last entry > 2 years ago. |
| 7 | 🌙 **Dormant** | Responding, last entry between 90 days and 2 years ago. |
| 8 | 🌿 **Alive** | Responding, last entry < 90 days ago. |

A feed that has never been fetched has status `Unchecked`.

**Hijack score** (sum of matched signals, capped at 1.0; weights are initial values to calibrate on the real run):

| Signal | Weight |
|---|---|
| Spam lexicon hit in title, description or recent entries (multilingual list in code: casino, slot, betting, loan, `xóc đĩa`, `trúng lớn`, …) | 0.4 |
| Description language ≠ language of the feed title + older entries (`whatlang`, only on texts ≥ 50 chars with confidence ≥ 0.8) | 0.4 |
| Language of the 5 newest entries ≠ language of the older stored entries (when there are any) | 0.4 |
| Redirect chain ends on a different registrable domain | 0.3 |
| `HtmlInsteadOfFeed` on a URL that used to serve a feed | 0.3 |

*Calibrated 2026-10-10 on the real Vienna run, pending user confirmation:* the redirect and `HtmlInsteadOfFeed` signals count only alongside lexicon or language evidence; alone they describe a site its owner closed (FiveThirtyEight → ABC News), not spam. Lexicon terms match at a word start, optionally plural, then a word end or a number (`casinos`, `casino88`, `togel4d`); `slot` counts only when numbered (`slot88`).

**Remarks** (secondary, any status):

- `https_available`: the `https://` variant of an `http://` feed returns the same feed (same channel title and at least one common entry).
- `http_only`
- `similar_title`: same folder, normalized titles within edit distance 2, one of them Dead/Fossil (the Orange Drenka case).
- `date_fallback`: more than half the entries use the fallback date.

**Proposed actions:**

| Action | Applies to |
|---|---|
| `upgrade_https` | `https_available` remark |
| `follow_redirect(url)` | Moved via 301/308 |
| `adopt_discovered(url)` | Moved via autodiscovery |
| `merge_into(feed_id)` | Duplicate (into the canonical feed); `similar_title` on the Dead/Fossil side, into the Alive/Dormant side |
| `bury` | Dead, Hijacked |

**Nothing is applied without a decision.** The only automatic step is diagnosis.

**Constants:** thresholds (90 days, 2 years, 5 failures, 14 days, 80 %, 0.6) are named constants in `health`. No config file.

## 8. Error handling

- **Per-feed errors never abort a run.** `ErrorKind` = `Dns`, `Connect`, `Tls`, `Timeout`, `Http(u16)`, `Parse`, `Encoding`, `TooLarge`, `HtmlInsteadOfFeed`.
- **Fatal errors** (clear message, exit code `1`):
  - database locked or unwritable;
  - schema newer than binary;
  - unreadable OPML (with line and column).
- **Exit codes:** `0` success (sick feeds are not a failure), `1` fatal, `2` usage, `130` interrupted (Ctrl-C).
- **Network guard:** if a run attempts at least 4 feeds and > 50 % of them fail with `Dns` or `Connect`, the run prints `Network looks unreachable — results not recorded as failures`. Those attempts are logged with a `network_suspect` flag and excluded from Dead counting.
- **Ctrl-C:** stop scheduling, finish in-flight transactions, print `Interrupted: N feeds were not fetched.` on stderr and exit `130`. A second Ctrl-C exits at once.
- **Logging:** `tracing` to `~/Library/Logs/Ressac/ressac.log` (daily rotation); `-v` mirrors to stderr.

## 9. Garden UI

Full-screen ratatui app. Layout:

```
┌ Ressac · Garden ─────────────────────────────────────────────────────────────────────┐
│ 187 feeds  👯 1  🦠 4  🚚 14  🪦 31  🤒 9  🦴 27  🌙 38  🌿 63      triage: 12 / 86    │
├─ To triage (urgency ↓) ───────────────┬─ Bibliobsession 2.0 ─────────────────────────┤
│ 👯 Daring Fireball (1)   Infos  = #41 │ 🦠 Hijacked · score 0.86          Biblis      │
│▶🦠 Bibliobsession 2.0    Biblis vi ⚠  │ Feed   http://www.bibliobsession.net/feed/    │
│ 🚚 Slashdot              Infos  301   │ Site   https://bibliobsession.net/            │
│ 🪦 Re/code               Infos  410   │ Evidence                                     │
│ 🦴 Heading East          Blogs  2011  │  • description in Vietnamese (0.97), title   │
│ 🌙 PhD et maracuja       potes  14 mo │    and entries in French until 2019          │
│   …                                   │  • spam lexicon: "xóc đĩa", "trúng lớn"      │
│                                       │ Last healthy entries                         │
│                                       │  2019-04-02  Bibliothèques et communs…       │
│                                       │ Wayback  closest snapshot 2021-06-12   [w]   │
│                                       │ Proposed  bury → cemetery                    │
├───────────────────────────────────────┴──────────────────────────────────────────────┤
│ k keep  r repair  b bury  ↵ details  o site  w wayback  u undo  f filter              │
│ R apply safe repairs (52)  ? help  q quit                                             │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

**Queue:**

- **Order:** status urgency, then folder, then title.
- **List columns:** status glyph, title, top-level folder, and one short clue (language flag, HTTP code, last-entry year, months asleep, `= #id` for duplicates).
- **What needs triage:** an active feed whose status is Duplicate, Hijacked, Moved, Dead, Ailing or Fossil, **and** that has no decision yet, **or** whose current status ranks strictly more urgent (§7 order) than its `status_at_decision`. Example: a Fossil you kept comes back if it turns Dead; it does not come back if it turns Dormant. A repair is measured against Alive instead, once the feed has been fetched again: any problem status after a repair brings it back.
- **Defaults:** Alive and Dormant feeds count as kept. They stay browsable but never enter the queue, so the counter reads `decided / needing triage`.

**Keys:**

| Key | Effect |
|---|---|
| `k` | Keep: record the decision and move to the next feed. On a buried feed (shown dimmed, clue `buried`, under status and folder filters), Keep restores it, unless another active feed now uses its address; `r` and `b` refuse until then. |
| `r` | Repair: apply the proposed repair action. Disabled when there is none. |
| `b` | Bury: record the decision and set `state = buried`. |
| `u` | Undo the last decision, restoring `previous` (so repairs and burials are reverted too). |
| `↵` | Expand the detail pane: full evidence, redirect chain, fetch log. |
| `o` | Open `html_url` in the browser. |
| `w` | Open the Wayback snapshot. |
| `f` | Filter by status or folder. |
| `R` | List all **safe** repairs, then apply them after `y` confirmation, recording one `repair` decision per feed. Safe repairs are `upgrade_https` and `follow_redirect`; `adopt_discovered` and merges always need individual review. |
| `?` | Help. |
| `q` | Quit. Decisions are persisted immediately, nothing to save. |

**Glyphs:** emoji by default. `--ascii` maps statuses to `=` Duplicate, `!` Hijacked, `>` Moved, `x` Dead, `~` Ailing, `#` Fossil, `z` Dormant, `+` Alive, `?` Unchecked, for terminals that render newer emoji (🪦, 🦴) at the wrong width.

**`ressac check` summary:**

```
Checking 187 feeds… ████████████████████ 187/187  (2m14s)

  🌿 Alive      63    🦴 Fossil     27    🚚 Moved      14  (11 repairable)
  🌙 Dormant    38    🤒 Ailing      9    🪦 Dead       31
  🦠 Hijacked    4    👯 Duplicate   1

  52 safe repairs available (41 × https, 11 × redirect)
  → run `ressac garden` to triage
```

**Export:**

- **Structure:** OPML 2.0 with the folder tree, feeds in `position` order, active feeds only.
- **Attributes:** original outline attributes are preserved, except `xmlUrl` (current URL) and `description` (omitted when it hits the spam lexicon).
- **`--cemetery`:** writes buried feeds with their `wayback_url` in a `ressac:wayback` attribute.

## 10. Testing

| Level | What |
|---|---|
| Unit | URL key normalization; OPML parsing (including the escaping repair); `diagnose` as a table of evidence → expected diagnosis; hijack scoring; queue membership rule (§9). |
| Integration | `wiremock` server covering 301/302/308 chains, 304, 404/410, parked HTML page, ISO-8859-1 RSS, RDF, JSON Feed, slow response (timeout), oversized body, global network failure (guard). End-to-end `import → fetch → diagnose → export` on a temp database. |
| UI snapshots | ratatui `TestBackend` + `insta` for the garden in its main states (queue, detail, filter, confirm dialog, `--ascii`). |
| Schema contract | Test asserting the live schema equals `schema/*.sql` applied in order; produces `tests/fixtures/contract.sqlite` for the Swift app's tests in sub-project 2. |
| Fixture | `tests/fixtures/vienna-like.opml`, hand-written. It covers nested folders, `(1)` homonyms, an http/https duplicate, `&amp;amp;`, a spam description, a JS-polluted description, a JSON Feed, an RDF feed, a moved blog. Personal sites use `example.org`; big public sites (xkcd, Daring Fireball) keep real URLs. |
| Manual run | `ressac check` against the real Vienna OPML; results and threshold calibration logged in `notes.md`. |

**CI (GitHub Actions, `ubuntu-latest`; the core is portable and macOS runners cost 10× the minutes on a private repo):** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. The same three commands are run locally before any phase is reported done.

## 11. Risks and open points

- ~~`feed-rs` charset handling~~: verified on 2026-10-10, feed-rs 3.0.0 decodes ISO-8859-1 from the XML declaration.
- **Hijack weights are guesses** until calibrated on the real OPML. False positives are harmless (nothing auto-buries) but noisy.
- **Emoji width** varies by terminal; `--ascii` is the escape hatch.
- **Rate-limited hosts** (Tumblr, YouTube) may answer `429`. It is treated as Ailing, and the per-host 1 s gap is the first knob to turn.
