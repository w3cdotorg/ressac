# Ressac — Sub-project 6: the sea chart

- **Date:** 2026-10-19
- **Status:** Draft, reviewed by a builder check
- **Scope:** a "Sea Chart" window in the Mac app that draws the living feeds as an engraved sea chart, in the spirit of Marine Le Breton's hand-drawn charts; per-feed themes and editable territories in the database (schema v5); the chart data computed by the core; no terminal version.
- **Builds on:** core, app, edition and terminal specs (all merged); the tmux ticker spec for the "store opens" pattern.
- **Reference mockup:** claude.ai artifact "Ressac Sea Chart" (version 18, 19/10/2026), whose source is kept outside the repository (scratchpad `carte/head.part`, `body.part`, `build.py`, `themes.py`). The mockup is the visual reference: when this spec is silent on a drawing detail, the mockup's behaviour is the answer.

## 1. Goal

A beautiful full-screen view to leave on a second display: Clément's feeds as islands on a sea chart, where geography says something true about them. From Ressac Harbour, distance tells how long each feed has been quiet; themes set the bearings; feeds from one publisher share a strip of land; whole themes become continents. Each island glows with its activity. A click on an island lists its five latest entries; a click on an entry opens it in the browser.

Success looks like: Clément presses ⇧⌘M, drags the Sea Chart window to his second screen and makes it full screen. The chart is drawn from his database in under a second, gusts of ink drift over the open sea, the morning papers glow copper near the quay. He clicks LE MONDE GROUP's Télérama headland and reads the latest entry in his browser. In the garden inspector he gives a feed another theme; the chart redraws within a few seconds and the island sails to its new sea. In the "By theme" view the other islands stay where they were; the harbour view drifts a little every day by design, since distance is time.

## 2. Decisions taken with the user (19/10/2026)

All taken while iterating on the mockup:

- **Window:** a separate window of the Mac app, for a second screen and full screen. Not in the terminal.
- **Language:** English everywhere on the chart.
- **Style:** Marine Le Breton's spirit: one ink, stippled shallows, depth lines, open water filled with short pen strokes that follow the wind offshore and run parallel to the coast near land.
- **Two views:** "From the harbour" (default): distance from the port = time since the feed's last entry, on a stretched log scale whose edge is 6 months, with range rings at 1 day, 1 week, 1 month, 3 months, 6 months; bearing = theme sector. "By theme": one archipelago per theme, static.
- **Feeds shown:** active feeds whose last entry is at most 6 months old (182 days). Buried feeds and feeds silent for longer are not on the chart. Feeds waiting for triage in the garden are left off too (Clément, 19/10: everything should be triaged); feeds in the Silent rhythm group that published within 6 months stay too, drawn as small islets marked "ED" (existence doubtful).
- **Themes:** each feed has one theme; seas are themes (15 proposed: AI labs and tools, AI voices, Product and engineering, Education and EdTech, Tech Apple and geeks, Digital rights, World press, French press, Science and ideas, Language and words, Books letters and libraries, Art design and images, Cities and places, Comics humour and fandoms, Friends' journals). Proposed by Claude, editable.
- **Territories:** feeds from one publisher (or a family such as Groupe Le Monde) share one landmass, each feed a headland; named in small capitals along the land. **Continents:** a whole theme as one landmass (four proposed: Land of Comics, Republic of Letters, Silicon Shores, Terra Libertas). Landmasses have invented coastlines (hull of the group, pushed out and frayed), not ridges.
- **Islands:** size by rhythm group; an island is one feed; islands never merge.
- **Activity light:** a soft halo at each island's heart, no ringed dot; score = Σ over entries of the last 30 days of 2^(−age_days/3); colour from a per-ink ramp, log-scaled to the busiest feed; the busiest ones flicker when animated.
- **Inks:** Blue ink (default), White on black; each with its own activity ramp (blue: celadon → copper; night: teal → luminous amber). A stored ink that is not offered reads back as Blue ink. (The Pop and Pirates inks were removed at acceptance, 2026-10-20.)
- **Animation:** gusts of ink along the sea strokes, the harbour light sweeping, ripples round islands with unread entries, flicker of the busiest lights; one toggle; off under Reduce Motion.
- **Hover:** feed name, unread this week, entries this week. **Click:** popover with the five latest entries (date, title linked, unread in bold), then theme, folder, rhythm, last entry, unread.
- **Labels never overlap:** placement by priority (harbour, continents, seas on open water, territories along their land, feeds busiest first with eight spots then a smaller size then a leader line, soundings, range rings); a name that finds no room shows on hover.
- **Updating:** the chart is always computed from the database, never stored as a picture, and no position is stored. It redraws when the database changes. In "By theme", each feed's target position depends only on its own name and theme, so a newcomer moves no other island by more than a few units (islands settle one by one, see §7); a feed leaving the chart, or changing theme, lets the later islands of that sea move up (Claude's call: no stored positions, which would need the page to write back to the database). The harbour view is a global, deterministic layout that drifts with time.

## 2b. Calls made after the builder check (Claude's)

- "Silent" in §2 means silent for more than 6 months. Filters: `state = 'active'`, not waiting for triage (`v_garden_row.needs_triage = 0`), last entry within 182 days. The rhythm group comes from `v_cadence.effective_group`; undiagnosed feeds are `silent` and, when recent, show as ED islets.
- Determinism, no-overlap and stability tests run on fixed inputs (a fixture with fixed `age_days`).
- Change detection: the app's `AppState` already owns an `ExternalChangeMonitor` and notifies GRDB observers (`AppDatabase.notifyExternalChanges`). The plan adds a change counter on `AppState` bumped on external changes and on the app's own writes, which `ChartModel` observes. Selecting a feed from the chart sets `AppState.selection = .feed(id)` (the `Source` enum in `Data/Queries.swift`) and brings the main window forward.
- `page` may be null; the popover then omits it.
- `Resources/Chart/` is a folder resource in `project.yml`, like `Resources/Fonts`, so the WKWebView can read it with one file-URL grant. d3 is the cdnjs 7.9.0 build (SHA-384 `CjloA8y00+1SDAUkjs099PVfnY2KmDC2BZnws9kh8D/lX1s46w6EPhpXdqMfjK6i`), bundled with its ISC licence; fonts are fetched once at build-time from their upstream repositories (OFL) and committed as `woff2`.
- The seed import file (English names, `xml_url` keys) is produced by Claude at acceptance from the mockup's `themes.py`, `build.py` (English names) and a fresh copy of the real database; it is not a build task and never enters the repository.

## 3. Scope

### In scope

| Item | Where |
|---|---|
| Schema v5: `theme`, `feed.theme_id`, `territory`, `territory_feed` | `schema/0005_chart.sql`, `store/mod.rs` |
| Theme and territory store API, chart snapshot | core `store/chart.rs` |
| `ressac chart --json`, `ressac chart import <file>`, `ressac feed theme <id> <key>` | binary `cli.rs`, `commands/chart.rs` |
| The chart page: pure layout and labelling module, drawing module, page, bundled d3 and fonts | `apps/macos/Ressac/Resources/Chart/` |
| JavaScript tests run by Node's built-in test runner | `apps/macos/ChartTests/` |
| Sea Chart window (WKWebView), ⇧⌘M, refresh on database changes, click → main window | `apps/macos/Ressac/Chart/` |
| Theme picker in the garden inspector | `Garden/GardenInspector.swift` |
| CI step for the JavaScript tests, README, notes, TODO | `.github/workflows/ci.yml`, docs |

### Out of scope

- Terminal rendering of the chart.
- "Add feed" (a separate, blocking sub-project). When it lands, it will propose a theme; until then new feeds (OPML import) are unthemed.
- A territory editor in the app: territories are edited through `ressac chart import` (re-importable). An in-app editor is a later step (TODO).
- Writing the user's theme proposal into the repository: the seed file is generated at acceptance from the mockup's `themes.py` and imported with Clément's go.

## 4. Schema v5

```sql
CREATE TABLE theme (
  id       INTEGER PRIMARY KEY,
  key      TEXT NOT NULL UNIQUE,      -- stable identifier, e.g. 'ai-labs'
  name     TEXT NOT NULL,             -- 'AI labs and tools'
  sea_name TEXT NOT NULL,             -- 'SEA OF THE LABS'
  position INTEGER NOT NULL           -- order round the harbour; neighbours touch
);
ALTER TABLE feed ADD COLUMN theme_id INTEGER REFERENCES theme(id) ON DELETE SET NULL;
CREATE TABLE territory (
  id       INTEGER PRIMARY KEY,
  name     TEXT NOT NULL UNIQUE,      -- 'LE MONDE GROUP'
  kind     TEXT NOT NULL CHECK (kind IN ('territory', 'continent')),
  theme_id INTEGER REFERENCES theme(id) ON DELETE CASCADE,  -- continents only: the whole theme
  CHECK ((kind = 'continent') = (theme_id IS NOT NULL))
);
CREATE TABLE territory_feed (
  territory_id INTEGER NOT NULL REFERENCES territory(id) ON DELETE CASCADE,
  feed_id      INTEGER NOT NULL REFERENCES feed(id) ON DELETE CASCADE,
  position     INTEGER NOT NULL,
  PRIMARY KEY (territory_id, feed_id)
);
```

- Migration 5 follows the existing pattern (backup of v4, `schema_meta.version = 5`). The Mac app's known schema version moves to 5 in the same change, so the app migrates on its next launch as for v3 and v4.
- No contract view: the apps draw chart data only from `ressac chart --json` (§5), since the score and the six-month rule live in Rust. The macOS app also reads `theme` and `feed.theme_id` directly, for the garden inspector's Theme pop-up; only the core writes them.

## 5. Core and CLI

### Store API (`store/chart.rs`)

- `themes()`, `set_feed_theme(feed_id, Option<theme key>)` (unknown key → error, buried feed allowed), `import_chart(&ChartImport) -> ChartImportReport` (one transaction: upsert themes by key; set themes of feeds matched by `xml_url` (also `xml_url_original`); replace territories with those in the file; report unmatched URLs and feeds left unthemed).
- `chart_snapshot(now) -> ChartSnapshot`:
  - feeds: active, not waiting for triage, last entry (`published_at`) within 182 days of `now`; for each: id, title, theme key (or null), folder path, effective rhythm group, `age_days` since the last entry, `unread_week` (unread, published in the last 7 days), `entries_week`, `activity` = Σ 2^(−age/3) over entries of the last 30 days (ages clamped at 0), `page`, and the five latest entries (id, title, url (http/https only, else null), published day in the local time zone, read);
  - themes in `position` order with feed counts (only themes that have chart feeds), plus an implicit "UNCHARTED WATERS" group for unthemed feeds, placed last;
  - territories (kind, name, member ids in position order, only members on the chart, dropped when fewer than 2) and continents (name, theme key);
  - `surveyed_at` (now) and totals (feeds, unread this week).
- All of it read through one store open; `now` injected.

### CLI

- `ressac chart --json`: prints the snapshot as one JSON document (the contract in §6), exit 0; without `--json` a short human summary (feeds per theme, territories). Database opened like every other command (it may migrate: the app calls it after its own launch check).
- `ressac chart import <file.json>`: imports themes, feed themes and territories (§5 format below); prints the report (territories with fewer than 2 matched feeds listed apart and not counted); `--dry-run` reports what would be imported without writing. Theme keys may not be empty, `none` (any case) or start with `-` (exit 2).
- `ressac feed theme <feed-id> <key|none>`: sets or clears a feed's theme.

Import file:

```json
{"themes": [{"key": "ai-labs", "name": "AI labs and tools", "sea": "SEA OF THE LABS"}],
 "feeds": {"https://www.anthropic.com/news/rss.xml": "ai-labs"},
 "territories": [{"name": "ANTHROPIC COAST", "feeds": ["https://…", "https://…"]},
                 {"name": "SILICON SHORES", "continent": "tech"}]}
```

Theme `position` is the order in the file.

## 6. Chart JSON contract

`{"version": 1, "surveyed_at": "…Z", "totals": {"feeds": n, "unread_week": n}, "themes": [{"key", "name", "sea", "count"}], "feeds": [{"id", "title", "theme", "folder", "group", "age_days", "unread_week", "entries_week", "activity", "page", "entries": [{"id", "title", "url", "day", "read"}]}], "territories": [{"name", "feeds": [ids]}], "continents": [{"name", "theme"}]}`

Unthemed feeds have `"theme": null` and belong to the `UNCHARTED WATERS` sea drawn by the page. A snapshot test pins the shape.

## 7. The chart page (`Resources/Chart/`)

Ported from the mockup, split in two classic scripts (no ES modules: they load from a file URL in WKWebView):

- `chart-core.js`: pure functions, no DOM, usable from Node (`module.exports` when present, `window.ChartCore` otherwise): seeded random and noise, theme sectors, harbour and theme placement (the harbour view uses d3 forces; "By theme" settles islands one by one, in feed-id order, each at the nearest free spot to its seeded target, with closer spacing when a sea is full, on a 6 × 3 grid with a cell reserved for UNCHARTED WATERS; islands keep clear of the compass rose and the cartouche in both views), height field, territory groups and invented coastlines, label placement (returns placements; never two overlapping boxes), heat ramp per ink.
- `chart-draw.js`: canvas drawing (stipple, sea strokes, land, isobaths, labels, compass rose, neatline, cartouche), the overlay animation (gusts, harbour light, ripples, flickering lights), hover, popover, toolbar (view, ink, animation, key), and the bridge (§8).
- `index.html`, `chart.css`, `d3.min.js` 7.9.0 (bundled, with its ISC licence), fonts Cormorant Garamond, Cormorant SC, IBM Plex Mono as local `woff2` with their OFL licences. No network access at all.
- The page fills the window; the 1920 × 1080 chart is scaled to fit and letterboxed in the paper colour. A slim toolbar (view, ink, animation, key) floats top right and fades when the pointer is still.
- Changes from the mockup: no header, theme list or notes; the chart's own key opens from the toolbar; the cartouche shows the real survey time; unthemed feeds go to UNCHARTED WATERS.

## 8. The Sea Chart window (Swift)

- A `Window("Sea Chart", id: "sea-chart")` scene opened with ⇧⌘M (View menu "Sea Chart"), resizable, full screen allowed, remembers its frame.
- `ChartModel` (`@Observable`): runs `ressac chart --json` through the bundled executable (a plain run that captures stdout, not the event stream), decodes only `version` to check it is 1, and hands the raw JSON to the page. It reruns on window open and when the app sees a database change (its existing observation of external changes and its own writes), debounced to at most one run per 2 s, and every 5 minutes while the window is open (ages and the survey time move with the clock). Terminal writes are polled for while the app is active or the chart window is visible on screen (its occlusion state), so a chart on a second display stays live behind another app.
- `ChartWebView` (`NSViewRepresentable` around `WKWebView`): loads `index.html` with read access to the Chart folder; after load calls `Chart.load(json, prefs)` and later `Chart.update(json)`. A script message handler `ressac` receives `{type: "openFeed", id}` (bring the main window forward and select the feed), `{type: "openURL", url}` (http/https only, opened with `NSWorkspace`), `{type: "prefs", view, ink, animate}` (stored in `UserDefaults`). Any navigation away from the bundled page is cancelled.
- Reduce Motion (`NSWorkspace.accessibilityDisplayShouldReduceMotion`) starts the page with animation off.
- When `ressac chart` fails, the window shows a plain message and a Retry button.

## 9. Theme editing

- Garden inspector: a "Theme" pop-up under the rhythm control (themes by position, plus "None"), which runs `ressac feed theme` like the rhythm override runs `ressac feed cadence`.
- Terminal: `ressac feed theme`; territories through `ressac chart import`.

## 10. Tests

- **Core:** migration 4 → 5 on a v4 fixture (backup made, version 5, existing data intact); theme CRUD; import (matching by `xml_url` and `xml_url_original`, unmatched URLs reported, territories replaced, continents tied to themes, dry run writes nothing); snapshot rules (182-day cutoff inclusive, buried excluded, activity formula on fixed ages, weekly counts, five latest entries newest first, non-http URLs nulled, territories with fewer than 2 chart members dropped, unthemed feeds).
- **CLI:** `chart --json` insta snapshot on a fixture; `chart import` report; `feed theme` set and clear, unknown key exits 2.
- **JavaScript (Node `node --test`):** on a fixture of 100 synthetic feeds: placement is deterministic for a given input; no two islands closer than the sum of their radii × 0.8; adding one feed moves no other island in "By theme" by more than 40 chart units; label placements never overlap each other or the cartouche and compass boxes; every territory group has at least 2 members; heat ramp endpoints per ink.
- **Swift:** `ChartModel` decodes the version, refuses another version, debounces bursts of changes into one run; the message handler accepts only http/https URLs and known message types.
- Never against the real database.

## 11. Acceptance (with Clément, after the code is complete — not part of the automated run)

1. Generate the seed import file from the mockup's `themes.py` (feed ids → `xml_url`), dry-run it against a copy of the real database, then import it into the real database with Clément's go.
2. Rebuild the app (with Clément's go, Ressac quit); open the Sea Chart, check both views and both inks, hover, popover, entry links, click → main window.
3. Change a feed's theme in the garden inspector: the chart redraws within a few seconds, the island moves to its new sea.
4. Full screen on the second display; animation toggle; Reduce Motion.

## 12. Left to the builder

- Internal names and module layout inside the two scripts, as long as `chart-core.js` stays DOM-free and testable.
- How fonts are subset (all three families, Latin with French accents).
- The exact debounce mechanism in Swift.
