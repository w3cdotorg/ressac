# Ressac — Sub-project 2a: the macOS app (reading + garden)

- **Date:** 2026-10-10
- **Status:** Draft, awaiting review
- **Scope:** sub-project 2 is split in two. **2a (this spec):** the native macOS app, with reading and the garden. **2b (later):** reading in the terminal UI, on the same schema.
- **Builds on:** `docs/specs/2026-10-10-core-and-garden-design.md` (sub-project 1, merged), referred to below as *the core spec*.

## 1. Goal

A native Mac app to read the feeds that sub-project 1 cleaned up, and to keep tending them: three-column reading with read/starred state, full-text search, full-article extraction for truncated feeds, and the garden as a sidebar destination. It must feel like a Mac tool (Mail / NetNewsWire family), not a web page in a window.

Success looks like: Clément opens Ressac.app on the database `ressac import` + `ressac check` produced, reads his unread entries with the keyboard alone, fetches the full text of a truncated article, and triages the remaining sick feeds in the Garden with decisions the terminal garden sees too.

## 2. Context and decisions already made

- **Architecture (approach C, unchanged):** the Rust core owns the SQLite schema and every computation. The app reads the database through **GRDB** and launches the **bundled `ressac` binary** as a subprocess for everything that fetches, computes or decides. The versioned schema plus its views are the contract.
- **Client-writable columns:** the app writes only `entry.read_at`, `entry.starred_at` (both already in schema v1, never touched by the core's entry upserts) and the new `feed.prefer_full_text`. Everything else is written by the core.
- **Decisions taken with the user (2026-10-10):**
  - Mac app first, terminal reading after (2b).
  - Scope: reading **and** garden; content-adapted rendering (comics, video) is out.
  - Article pane: the feed's HTML in a clean reading view, **plus full-article extraction** (Readability-style, in the core).
  - Refresh: **manual only**. `⌘R` refreshes the current source (selected feed, or every feed of the selected folder; on a smart list, all feeds); `⇧⌘R` refreshes all feeds. A manual refresh ignores the 30-minute freshness rule.
  - Backlog: on a feed's first fill, entries published more than **7 days** before are stored as already read.
  - Garden: a **sidebar destination** (option A of the mockups), same keys and same undo stack as `ressac garden`.
  - Fonts: **SF Pro** by default; Settings offers SF Pro, New York, Charter, Newsreader, Atkinson Hyperlegible Next.
- **Platform:** this Mac only, macOS 27, Xcode 27, Swift 6.4. No signing beyond ad hoc, no notarization, no App Store.
- **Language:** every user-facing string in English. Code and docs in English; `notes.md` in French.
- **Mockups:** `.superpowers/brainstorm/*/content/{reading-layout,article-fonts,garden-placement}.html` (local, not committed).

## 3. Scope

### In scope

**Core (Rust), schema v2 and new commands:**

| Item | Purpose |
|---|---|
| Schema v2 migration | `feed.prefer_full_text`, `feed_health.needs_triage`, `entry_fulltext`, `entry_fts`, views `v_entry`, `v_garden_row`, `v_unread_count` (§5). |
| Backlog rule | First fill of a feed: entries older than 7 days arrive read. |
| `ressac fetch --feed <id>… / --folder <id>` | Refresh a subset; repeatable `--feed`. |
| `ressac extract <entry-id>` | Fetch the entry's page and store the extracted article. |
| Full text on fetch | For feeds with `prefer_full_text`, extract new entries after the fetch. |
| `ressac garden decide <feed-id> keep\|repair\|bury` | Non-interactive decision (same code path as the TUI keys). |
| `ressac garden undo` | Undo the last decision (shared stack). |
| `ressac garden apply-safe` | Apply all safe repairs (the TUI's `R`). |
| `ressac migrate` | Open the database and run pending migrations, nothing else. |
| `--json` on every new command | NDJSON events, same `Event` shape as `fetch`, plus `Extracted`, `Decided`, `Undone` (§6). |

`ressac garden` with no subcommand still opens the terminal garden.

**App (Swift), `apps/macos/`:**

- Three-column reading window: sidebar (smart lists, folders and feeds with unread counts, health dots, Garden section), entry list, reader.
- Read / unread, star, mark all read, full-text search, open in browser, share, drag a link out, drop an OPML in.
- Full-article toggle per entry, "Always fetch full article" per feed.
- Garden destination: triage table, inspector with evidence, keep / repair / bury / undo, apply safe repairs.
- Settings: font, text size, line width, database location.
- First-run screen (drop or choose an OPML), progress bar for subprocess runs, error banners.

### Out of scope

Content-adapted rendering (comics, video embeds), automatic or background refresh, Dock badge, notifications, the morning edition and cadence views (sub-project 3), the Antiope theme (sub-project 4), terminal reading (2b), sync, iCloud, signing and notarization, iOS.

## 4. Architecture

```
ressac/
├── crates/ressac-core/        # + store/fulltext.rs, + extract.rs, schema v2
├── crates/ressac/             # + fetch --feed/--folder, extract, garden subcommands, migrate
├── schema/0002_reading.sql    # migration v1 → v2
└── apps/macos/
    ├── project.yml            # xcodegen spec (the .xcodeproj is generated, not committed)
    ├── Ressac/
    │   ├── App/               # RessacApp, AppState, commands (menus + shortcuts)
    │   ├── Data/              # Database (GRDB pool), records, observations, writes
    │   ├── Engine/            # RessacCLI: spawns the bundled binary, parses NDJSON
    │   ├── Reading/           # Sidebar, EntryList, Reader (WKWebView), ArticleTemplate
    │   ├── Garden/            # GardenTable, Inspector
    │   ├── Settings/          # SettingsView, ReaderStyle
    │   └── Resources/         # reader.css, fonts (Newsreader, Atkinson Hyperlegible Next), assets
    └── RessacTests/           # Swift Testing
```

**Data flow.**

- **Reads:** GRDB `ValueObservation` on the contract views keeps every list live. When a subprocess writes, the UI follows on its own, with no manual reload.
- **Reading-state writes:** `read_at` and `starred_at` are written directly by the app, in short transactions.
- **Everything else:** goes through `RessacCLI`. It streams `Event`s into `AppState`, which feeds the progress bar. At most one long run (fetch, import, extract during fetch) at a time: a second refresh while one runs is refused and its trigger is disabled. Garden subcommands and single `extract` calls are short and may run alongside a fetch; they are serialised among themselves.

**Bundling.**

- A build phase runs `cargo build --release -p ressac` and copies `target/release/ressac` to `Ressac.app/Contents/Helpers/ressac`.
- The app always passes `--db <path> --json` to it.
- The database path defaults to the core's default (`~/Library/Application Support/Ressac/ressac.sqlite`), so the app and the terminal share it.

**Concurrency.**

- SQLite in WAL mode. GRDB `DatabasePool` with a 5-second busy timeout.
- The core already uses WAL. Its writes are short transactions, except a run's per-feed upserts, which are already per feed.

## 5. Data model: schema v2 (`schema/0002_reading.sql`)

`schema_meta.version` becomes 2. The core's migration runner applies `0002` after `0001`; a database newer than the binary is refused (core spec §8).

| Change | Definition | Written by |
|---|---|---|
| `feed.prefer_full_text` | `INTEGER NOT NULL DEFAULT 0` | app |
| `feed_health.needs_triage` | `INTEGER NOT NULL DEFAULT 0`. The garden's triage rule (core spec §9, with the repair baseline), stored so that other clients do not reimplement it. Recomputed after every diagnosis, decision and undo. | core |
| `entry_fulltext` | `entry_id INTEGER PRIMARY KEY REFERENCES entry(id)`, `fetched_at TEXT NOT NULL`, `html TEXT`, `title TEXT`, `byline TEXT`, `error TEXT`. Exactly one of `html` and `error` is set. | core |
| `entry_fts` | `fts5(title, author, body, tokenize = 'unicode61 remove_diacritics 2')`, `rowid = entry.id`. `body` holds plain text from `content_html`, else `summary_html`, replaced by the full text once extracted. | core |
| `v_entry` | One row per entry of an active feed: `entry_id`, `feed_id`, `feed_title`, `folder_id`, `title`, `author`, `url`, `published_at`, `is_read`, `is_starred`, `has_fulltext`, `summary_html`, `content_html`. | view |
| `v_unread_count` | `feed_id`, `unread` for active feeds. | view |
| `v_garden_row` | Everything the Garden needs per feed: `feed_id`, `title`, `folder_path`, `state`, `status`, `reason`, `clue` (from `evidence`), `evidence` (JSON), `remarks` (JSON), `proposed_action` (JSON), `needs_triage`, `last_decision`, `wayback_url`, `html_url`, `xml_url`. | view |

**Rules:**

- **Backlog:** when a fetch stores entries for a feed that had none, entries with `published_at < fetched_at − 7 days` get `read_at = fetched_at`.
- **FTS upkeep:** the core inserts and updates the FTS rows in the same transaction as the entry. There are no SQL triggers, because turning HTML into text needs Rust.
- **Existing databases:** migration v2 fills `entry_fts` from existing entries. It does **not** apply the backlog rule to them, since their entries are already unread. This is a one-off; the first-run path in the app explains "mark all read" if the count is large.
- **Contract test:** `tests/contract.rs` gains v2 checks (columns, views, backlog rule, FTS row per entry). `tests/fixtures/contract.sqlite` is regenerated at v2 and is also used by the Swift tests.

## 6. Core additions

**`ressac fetch --feed <id>… | --folder <id>`.**

- Restricts a run to those feeds; `--folder` includes subfolders. Buried feeds are skipped even when named.
- Same politeness, same events, same diagnosis afterwards.
- Diagnosis still runs over **all** feeds, because duplicate detection is cross-feed.

**Full-article extraction (`extract.rs`).**

- Fetches `entry.url` through the existing `Source` (same limits, user agent and per-host gap). The total timeout is 30 s.
- Extraction uses a Rust port of Mozilla Readability. The candidate is `dom_smoothie`; plan phase 1 confirms the crate on 10 real pages from the user's feeds before committing to it.
- The result goes to `entry_fulltext` (`html`, `title`, `byline`) and refreshes the FTS body.
- Failures store `error`, one English sentence: "The page is behind a login.", "Anti-bot challenge page.", "No article text found.", "HTTP 404.". A later `extract` call retries.
- `ressac extract <entry-id>` emits `Extracted { entry_id, ok, error }`.
- **During fetch:** for feeds with `prefer_full_text = 1`, new entries are extracted after the feed run, at most 20 per feed per run, and the progress bar shows "Fetching full articles". Ctrl-C and stop requests are honoured between entries.

**Extracted HTML safety.**

- The core keeps the extractor's HTML as is.
- The app is responsible for rendering safely: JavaScript off, no iframes (§7).

**Garden subcommands.**

- `decide`, `undo` and `apply-safe` call `garden::decide`, `garden::undo` and `garden::apply_safe_repairs`, then re-diagnose the touched feeds and recompute `needs_triage`.
- They emit `Decided { feed_id, decision }`, `Undone { feed_id }` and `Finished { summary }`.
- A decision on a buried or unknown feed, a repair with nothing to repair, and a decision or an undo the store refuses (an address another active feed already uses, a merge target that is not active) exit `2` with a message.

**Exit codes:** unchanged (`0`, `1`, `2`, `130`).

## 7. The app

### Window and navigation

**Layout:** `NavigationSplitView` with three columns (sidebar 220 pt, list 320 pt, reader). A unified toolbar keeps the top ~50 pt draggable, and the traffic lights sit in the sidebar.

**Sidebar sections:**

1. **Smart lists:**
   - Unread;
   - Today, entries published since local midnight;
   - Starred.
2. **Folders:** the folder tree, then its feeds, each with its unread count. A feed whose status is Ailing, Dead, Moved, Hijacked or Duplicate gets a small coloured dot, with the status name in its tooltip.
3. **Garden:**
   - Needs triage, with its count;
   - All feeds;
   - Cemetery (buried feeds).

**Feed context menu:**
- Refresh;
- Always fetch full article (toggle);
- Mark all read;
- Open site;
- Show in Garden.

**Entry list:**
- **Row content:** feed name, date, title (bold when unread, grey when read), two lines of summary. A blue dot marks unread entries.
- **Sort:** newest first.
- **Filters:** a toolbar segment switches between All and Unread.

**Search (`⌘F`):** a toolbar field that queries `entry_fts` within the current source. The result list replaces the entry list until the field is cleared or `Esc` is pressed.

### Reader

**Rendering:** a `WKWebView` loads a local HTML template (`ArticleTemplate`) with `reader.css` and the chosen font. Text runs at about 65 characters per line by default, and the template follows light and dark mode with separately tuned palettes, not inverted ones.

**Safety:**
- JavaScript is disabled.
- `<iframe>`, `<script>`, `<object>`, `<embed>` and `<form>` are removed before rendering.
- Images load from the network.
- Every link click opens in the default browser.

**Content shown:**
- the full article when it exists and the toggle is on;
- otherwise `content_html`, else `summary_html`.

**Toolbar:**
- **Full article:** a toggle. It runs `ressac extract` when nothing is cached and shows a spinner meanwhile. On failure, the feed content stays and a one-line notice appears: "Full article unavailable: <error>".
- **Star.**
- **Open in browser (`⌘↩`).**
- **Share.**

**Marking read:** an entry is marked read once it has been selected for 1 s, or as soon as the reader moves past it (`j`, `Space` at the bottom). `m` toggles it back.

### Garden

**Table** (option A of the mockups):
- one section per status, in urgency order;
- each row shows the glyph, title, clue and proposed action;
- decided rows are dimmed until the list is left.

**Header:**
- "N of M left";
- **Apply N safe repairs (`⇧R`)**;
- **Undo (`⌘Z`)**.

**Inspector:**
- the status;
- the evidence bullets;
- a disclosure with the redirect chain and the last fetch log rows;
- Wayback link;
- the buttons Keep (`k`), Repair (`r`, when an action exists), Bury (`b`), Open site (`o`) and Wayback (`w`).

**Behaviour:**
- `u` and `⌘Z` undo.
- Every action is a `ressac garden …` subprocess. The table updates through observation of `v_garden_row`.
- **All feeds** shows every feed; **Cemetery** shows buried feeds, and `k` there restores one, as in the TUI.

### Keyboard

| Key | Action |
|---|---|
| `j` / `↓`, `k` / `↑` | Next / previous entry (in the Garden, `k` is Keep; arrows navigate) |
| `Space` | Scroll the reader; at the bottom, next unread |
| `m` | Toggle read |
| `s` | Toggle star |
| `⇧⌘A` | Mark all read in the current list |
| `⌘R` / `⇧⌘R` | Refresh current source / all feeds |
| `⌘F` | Search |
| `⌘↩` | Open in browser |
| `⌘+` / `⌘−` / `⌘0` | Text size |
| `⌘,` | Settings |
| `⌘O` | Import OPML |
| `⇧⌘E` | Export OPML |

Every menu item shows its shortcut. A shortcut that does nothing in the current context is disabled in the menu.

### Settings (`⌘,`)

- **Reading:**
  - **Font:** SF Pro (default), New York, Charter, Newsreader, Atkinson Hyperlegible Next. Newsreader and Atkinson ship in the bundle under the OFL, with their licence files.
  - **Text size.**
  - **Line width:** narrow, medium or wide.
- **Data:**
  - **Database location:** a path picker. The default is shared with the terminal.
  - **Reveal logs.**

### First run and empty states

- **No database, or no feeds:**
  - a centred empty state reads "Drop an OPML file here, or choose one…", with a button;
  - the import runs `ressac import`, then `ressac fetch`, with progress.
- **Empty list:** a one-line empty state per context ("No unread entries", "Nothing to triage").

### Drag and drop

- **In:** dropping an `.opml` file on the window imports it, after a confirmation sheet that shows the `--dry-run` counts.
- **Out:** dragging an entry row exports its URL and title.

## 8. Error handling

- **Subprocess exit `1`:**
  - a non-modal banner under the toolbar shows the last stderr line and a **Show log** button, which opens `~/Library/Logs/Ressac/`;
  - the banner dismisses itself on the next successful run.
- **Subprocess exit `2`:** this is a bug in the app's call. It is logged and shown in the same banner.
- **Binary missing from the bundle:** a fatal alert at launch.
- **Schema version:**
  - **Older than 2:** the app runs `ressac migrate` before opening the database, with a progress sheet.
  - **Newer than the app knows:** the app refuses to open it and shows "This database was created by a newer Ressac".
- **Database busy:** GRDB waits up to 5 s. If it still fails, a banner reads "The database is busy (is `ressac` running in a terminal?)".
- **Read-state write failure:** the change is reverted in the UI and a banner is shown. It is never silently dropped.
- **Extraction failure:** shown inline in the reader (§7), never as a banner.

## 9. Testing

- **Rust:**
  - unit tests for the backlog rule, the subset fetch, `needs_triage` persistence, the extraction outcome mapping (on fixture HTML pages) and the FTS text;
  - CLI tests for the new subcommands, including exit `2` cases and NDJSON shapes;
  - the contract test at v2.
- **Swift (Swift Testing):**
  - records and observations against a copy of `tests/fixtures/contract.sqlite`;
  - `RessacCLI` NDJSON parsing against recorded event lines, and against a stub executable;
  - HTML sanitising before rendering;
  - keyboard-command routing in `AppState`.
  - UI is not snapshot-tested.
- **Manual check before declaring done**, recorded in `notes.md`:
  1. launch on the real database;
  2. read with the keyboard only;
  3. extract a truncated article;
  4. triage three feeds and undo one;
  5. check that `ressac garden` in the terminal shows the same decisions;
  6. switch light/dark, fonts and sizes.
- **CI:**
  - the existing Linux job keeps the Rust checks;
  - a new `macos-latest` job runs `xcodegen` and `xcodebuild test`. If GitHub's macOS image lacks Xcode 27 / the macOS 27 SDK, the job is marked allowed-to-fail and the gap is recorded in `notes.md`.

## 10. Risks and open points

- **Readability port quality:** checked in plan phase 1 on real pages. The fallback is to keep the feed content and record "No article text found.".
- **The macOS image on CI** may lag the local Xcode 27 (see §9).
- **WKWebView and remote images:** tracking pixels load. Accepted for v1; a "Block remote images" setting can come later.
- **Large unread backlog** for existing databases (§5): mitigated by `⇧⌘A`.
- **Concurrency:** two writers (app and core) on one WAL database. Writes on both sides are short. The 5-second busy timeout plus the banner covers the rest.
