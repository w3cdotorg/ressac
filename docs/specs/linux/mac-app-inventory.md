# Ressac.app (macOS) — behaviour-level inventory for a Linux port

Source: `/Users/willow/Sites/ressac` at `9b8c56f` (merge of PR #18). All paths below are relative to
`apps/macos/Ressac/` unless they start with `apps/`, `docs/`, `crates/` or `schema/`.
Specs read first: `docs/specs/2026-10-10-mac-app-design.md` (2a), `2026-10-14-antiope-design.md` (4),
`2026-10-16-cadence-and-edition-design.md` (3), `2026-10-19-sea-chart-design.md` (6),
`2026-10-20-add-feed-design.md` (7), plus `TODO.md` and `notes.md`. Where shipped code differs
from a spec, the code is described and the gap is flagged **[spec gap]**.

Size (Swift, non-test): App 2,075 · Antiope 2,535 · Reading 2,242 · Data 1,078 · Garden 980 ·
Chart 592 · Settings 554 · Engine 479 · Teletext 349 = **10,884 lines**. Plus JS `chart-core.js`
520 + `chart-draw.js` 690, `chart.css` 103, `reader.css` 543. Tests: 8,454 Swift lines
(492 `@Test`), 484 JS lines (28 Node tests).

Build facts (`apps/macos/project.yml`): bundle id `org.w3cdotorg.ressac`, version 0.2.0, macOS 27,
Swift 6 strict concurrency, warnings as errors, no sandbox, ad-hoc signing. One SPM dependency:
GRDB 7.11.1. `Resources/Fonts` and `Resources/Chart` are *folder* resources. `ATSApplicationFontsPath
= Fonts` registers bundled fonts for the app process. A post-compile script
(`apps/macos/scripts/embed-helper.sh`) runs `cargo build --release -p ressac` and copies the binary
to `Ressac.app/Contents/Helpers/ressac` (ad-hoc codesigned).

---

## 0. Architecture in one paragraph

The Rust core owns the SQLite schema (v6) and every computation. The app (a) **reads** the database
directly through GRDB (`DatabasePool`, WAL, 5 s busy timeout) using live `ValueObservation`s on
contract views/tables; (b) **writes directly** only three client columns: `entry.read_at`,
`entry.starred_at`, `feed.prefer_full_text` (`Data/Writes.swift`); (c) runs the bundled `ressac`
binary as a subprocess for **everything else** (fetch, import, export, extract, garden decisions,
edition build, cadence/theme overrides, add, move, migrate, chart JSON), always as
`ressac --db <path> --json <subcommand…>`, parsing NDJSON events from stdout and keeping the last
stderr line as the error message. Writes made by other processes (a terminal `ressac`) are detected
by polling `PRAGMA data_version` every 3 s on a private connection. Two "looks" (Native, Antiope)
are two root views over one shared `AppState`.

---

## 1. Windows, scenes, menus, toolbar

### 1.1 Scenes (`App/RessacApp.swift`)
| Scene | id / title | Content | Notes |
|---|---|---|---|
| `WindowGroup` | id `main`, title "Ressac" | `LaunchView` | min 900×500. Its `.task` launches the DB (`Launcher.launch`) once; a second window's task is ignored (`AppState.swift:917`). Multiple reading windows possible (WindowGroup), all share one `AppState`. |
| `Window` | id `sea-chart`, "Sea Chart" | `ChartWindow` | default 1280×720, min 640×360, frame remembered by SwiftUI, full screen allowed. Its own `.task` also calls `launch` (window can be restored alone at launch). |
| `Settings` | — | `SettingsView` | ⌘, |

`LaunchView` phases (`RessacApp.swift:39-71`, `Launcher` in `AppState.swift:869-964`):
- `.starting` → spinner.
- `.migrating` → "Updating the database…" (runs `ressac migrate`, short run).
- `.tooNew(v)` → "This database was created by a newer Ressac" + **Use Default Database** button.
- `.failed(msg)` → message + **Use Default Database** + **Quit Ressac**.
- `.ready(state)` → `MainView` (Native) or `AntiopeRootView` (Antiope) per `Appearance.effective`,
  wrapped in `.libraryChrome(state)` (first-run screen, OPML drop/import/export, Add Feed sheet,
  add-feed note) and `.environment(state)`.
- Helper binary missing (`Bundle/Contents/Helpers/ressac` not executable) → `.failed("The ressac helper is
  missing from the app bundle. Rebuild the app.")` **and** a modal critical `NSAlert` "Ressac cannot
  start" with only **Quit Ressac** (`AppState.swift:921-926, 954-963`).
- "Use Default Database" clears the Settings DB path (`databasePath` default) and relaunches
  in-process (no app restart); `RESSAC_DB` env still wins.

### 1.2 Menu bar (`App/Commands.swift`)
Single-letter shortcuts are attached to menu items **only when the command is enabled**
(`letter(_:when:)`, `Commands.swift:25-27`) because AppKit swallows a disabled item's key equivalent;
the actual handling of those keys is done earlier by `ReadingKeyRouter` (§2.6). Every item is disabled
when it cannot act.

| Menu | Item | Shortcut | Action / enablement |
|---|---|---|---|
| File (replaces New group) | Add Feed… | ⌘N | `pendingSheet = .addFeed`; disabled before DB ready |
| File | Import OPML… | ⌘O | `pendingPanel = .importOPML` → file chooser |
| File | Export OPML… | ⇧⌘E | `pendingPanel = .exportOPML` → save panel "Ressac.opml" → `ressac export --output <path>` (never `--cemetery`) |
| File | Refresh | ⌘R | `refreshCurrent()`; disabled while a long run is active |
| File | Refresh All Feeds | ⇧⌘R | `fetch --force` all |
| Edit (replaces text-editing group) | Search | ⌘F | `isSearchFocused = true` (Native: focuses toolbar search; Antiope: opens `SEARCH?` prompt) |
| View (after toolbar group) | Show Unread Only (toggle) | ⌥⌘U | toggles `unreadOnly`; disabled in Garden scopes |
| View | Bigger Text / Smaller Text / Actual Size | ⌘+ / ⌘− / ⌘0 | acts on the size of the look shown (§7.3) |
| View | Sea Chart | ⇧⌘M | `openWindow(id: "sea-chart")` |
| Go | Next Entry | j | `selectNext()` (enabled: letters on, not Garden, list non-empty) |
| Go | Previous Entry | k | `selectPrevious()` |
| Go | Scroll Down or Next Unread | Space | `advance()` |
| Article | Mark as Read / Mark as Unread (title follows state) | m | `toggleRead()` |
| Article | Star / Unstar | s | `toggleStar()` |
| Article | Mark All as Read | ⇧⌘A | `markAllRead()` (disabled in Garden, during search, or when nothing unread) |
| Article | Full Article | — | `requestFullText()` |
| Article | Open in Browser | ⌘↩ | opens entry URL (http/https only) |
| Garden table (button, not menu) | Undo | ⌘Z | `ressac garden undo` (Native: header button with `.keyboardShortcut("z")`; Antiope: hidden zero-size button) |

"Letters enabled" = `!isSearchFocused && readingWindowIsKey` (`AppState.swift:333`); the key-window
tracking uses `NSWindow.didBecomeKey/didResignKey` notifications so typing in Settings/panels never
triggers j/k/m/s.

### 1.3 Native toolbar (`Reading/MainView.swift:33-72`, `Reading/ReaderView.swift:88-130`)
- Search field (`.searchable`, placement toolbar, prompt "Search"), bound to `AppState.searchText`;
  focus mirrored both ways with `isSearchFocused`.
- Segmented **Show: All | Unread** (`unreadOnly`), disabled in Garden.
- **Refresh** button (`arrow.clockwise`) — replaced by a **progress pill** while a run has progress:
  linear `ProgressView(value: fraction)` (indeterminate when fraction nil) with label text;
  `ViewThatFits` picks full text ("Building this morning's edition… 34/156" / "Fetching feeds" /
  "Fetching full articles" / "Checking https|autodiscovery|wayback") or compact ("Edition… 152/156");
  width 260 pt for edition, 140 otherwise, 10 pt inner inset; tooltip = full text (`JournalList.swift:82-103`).
- Reader items (primary action): **Full Article** toggle button with title+icon in its own glass
  capsule (fixed `ToolbarSpacer` on each side; macOS 27 quirk, notes.md:375), small spinner while
  extracting, tooltip explains state/why disabled (`ReaderView.swift:135-144`); **Star/Unstar**;
  **Open in Browser** (safari icon); **ShareLink** (system share menu) when the entry has a web link.
- Banner (`BannerView`, `MainView.swift:76-96`) inset at top under toolbar: ⚠️ + message (2 lines,
  selectable) + **Show Log** (opens `~/Library/Logs/Ressac/`) when `showLog` + dismiss ✕.
- Add-feed note bar inset at bottom (✓ + text, auto-dismiss after 6 s, `AddFeedSheet.swift:174-192`).

### 1.4 Antiope window chrome (`Antiope/AntiopeRootView.swift:256-311`)
Toolbar hidden; while Antiope root view is in the window: `titleVisibility = .hidden`,
`titlebarAppearsTransparent`, `.fullSizeContentView`, background black, `darkAqua` appearance;
all restored when switching back to Native (live switch, same window).

---

## 2. Reading (Native look)

### 2.1 Layout (`Reading/MainView.swift`)
`NavigationSplitView` 3 columns: sidebar (min 180 / ideal 220 / max 320), content (260/320/480),
detail. Content = `GardenTable` (garden scope) | `JournalListView` (Journal) | `EntryListView`.
Detail = `GardenInspector` (garden) | `ReaderView`.

### 2.2 Sources (`Data/Queries.swift:9-22`)
`Source = unread | today | starred | journal | cadence(group) | folder(id) | feed(id) | garden(needsTriage|all|cemetery)`.
Default selection: `.unread` (app opens on Unread, not Journal). Changing source clears retained
rows, decided-this-visit, garden selection and entry selection, restarts the list.

SQL predicates on `v_entry` (`Queries.swift:298-322`, shared by list, counts and mark-all-read):
- unread `NOT is_read`; today `published_at >= <local midnight as RFC3339 UTC>`;
  starred `is_starred`; journal `entry_id IN (SELECT entry_id FROM edition_entry WHERE edition_id=(SELECT MAX(id) FROM edition))`;
  cadence `feed_id IN (SELECT feed_id FROM v_cadence WHERE effective_group=?)`;
  feed `feed_id=?`; folder: recursive CTE over `folder.parent_id` (subfolders included).

### 2.3 Sidebar (`MainView.swift:98-168`, snapshot `Queries.sidebar`, `Queries.swift:88-130`)
Sections:
1. **Smart Lists**: Journal (`newspaper`, badge = unread entries of latest edition counted through
   `v_entry`), Unread (badge), Today (badge, since local midnight), Starred (badge).
2. **Feeds**: `OutlineGroup` folder tree from `folder(id,parent_id,name,position,page)` and active
   `feed` rows (`f.state='active'`), merged per parent and sorted by `(position, folders-before-feeds, id)`
   (`AppState.swift:39-56`). Folder badge = sum of subtree unread. Feed row: title (1 line) +
   7 pt coloured dot with status tooltip when status ∈ {Ailing, Dead, Moved, Hijacked, Duplicate}
   (`Records.swift:110-112`; colours `Garden/GardenStatusStyle.swift`), unread badge.
   The section header "Feeds" is a drop target (move feed to root).
3. **Rhythm**: Daily (`bolt`), Weekly (`calendar`), Monthly (`moon`), Rare (`sparkles`), Silent (`zzz`),
   badge = SUM(unread) from `v_cadence` per effective group; Silent always listed.
4. **Garden**: Needs Triage (`leaf`, badge = `v_garden_row` needs_triage AND active), All Feeds, Cemetery.

Feed context menu: **Refresh** (`fetch --feed id`, disabled during long run) · **Always Fetch Full
Article** toggle (direct write `feed.prefer_full_text`) · **Mark All as Read** (disabled if 0 unread) ·
**Open Site** (html_url, disabled if nil) · **Show in Garden** (selects `.garden(.all)` and the feed).

"Today" rolls over: `NSCalendarDayChanged` restarts sidebar observation and list
(`AppState.swift:234, 266-270`). **[spec gap / TODO]** uses `Calendar.current` → time-zone change
only followed after relaunch (TODO.md:65).

### 2.4 Entry list (`MainView.swift:170-226`, list logic `AppState.swift:698-864`, `Queries.swift`)
- Row: blue (accent) dot if unread; caption line = feed title · ★ if starred · relative date
  ("2 hours ago"); title (semibold+primary if unread, regular+secondary if read, 2 lines);
  excerpt = tags stripped, 6 entities decoded, whitespace collapsed, ≤280 chars + "…" (2 lines)
  (`Records.swift:55-93`). Title "Untitled" when blank.
- Sort newest first `(published_at DESC, entry_id DESC)`; Journal sorted by edition rank.
- List rows carry `substr(COALESCE(summary_html, content_html),1,1200)` and NULL content
  (`Queries.swift:73-80`); reader fetches full row separately.
- **Paging**: 500 rows per page (`listPageSize`), via a "floor" = `(published_at, entry_id)` of the
  oldest loaded row; query `... WHERE (published_at, entry_id) >= (?,?)`. Next page loads when one of
  the last 20 rows appears (deferred one turn), or when `j` passes the last loaded row. Totals
  (`COUNT(*)`, `SUM(NOT is_read)`) are for the whole list. New entries never push loaded rows out
  (floor only moves down). Journal always loads whole. (`AppState.swift:403-430, 734-778`, `Queries.swift:149-164, 363-379`.)
- **Retained rows**: entries read during the current visit stay listed (as read) in unread-only lists
  until the source changes (`AppState.merge`, `AppState.swift:848-864`).
- **Unread filter**: toolbar segment / ⌥⌘U; adds `AND NOT is_read`.
- Selection follows keyboard: `ScrollViewReader.scrollTo` on next run-loop turn.
- Empty overlay: "No results" (search) / "No unread entries" (unread or filter) / "No entries".
- **Drag out**: row `.onDrag` → `NSItemProvider(NSURL)` with `suggestedName` = title (only http/https link).

### 2.5 Read / starred / mark-all
- Auto-read: an entry selected for **1 s** is marked read (`autoReadDelay`, `AppState.swift:134, 533-542`).
- Moving past an unread entry with `j`/Space marks it read immediately (`markSelectedReadOnLeaving`).
  `k` (previous) does not.
- `m` toggles read (cancels pending auto-read when un-reading). `s` toggles star.
- All writes **optimistic**: UI updated first, reverted on failure + banner "Could not save the
  change: …" or "The database is busy (is `ressac` running in a terminal?)" (SQLITE_BUSY)
  (`AppState.swift:810-839`).
- SQL (`Data/Writes.swift`): `UPDATE entry SET read_at=? WHERE id IN (…500 chunk…) [AND read_at IS NULL when marking read]`
  (keeps first read time); `UPDATE entry SET starred_at=? WHERE id=?`; mark-all:
  `UPDATE entry SET read_at=? WHERE read_at IS NULL AND id IN (SELECT entry_id FROM v_entry WHERE <predicate>)`;
  `UPDATE feed SET prefer_full_text=? WHERE id=?`. Timestamps RFC 3339 UTC with milliseconds and `Z`
  (`Records.swift:5-16`), same as core `model::ts`.
- ⇧⌘A in Journal marks only the edition's entries (never the overflow).

### 2.6 Keyboard router (`Reading/ReadingKeyRouter.swift`)
A window-level **local key monitor** (`NSEvent.addLocalMonitorForEvents(.keyDown)`) intercepts keys
before the focused view (sidebar/list type-select and WebKit would otherwise eat j/k/Space,
notes.md:373). Only events of its own window; only Shift/Ctrl/Opt/Cmd modifiers considered
(arrows carry `.numericPad/.function`, posted events carry a private bit, notes.md:374).

Key decoding (`ReadingKeys.key(for:)`, lines 97-124): arrows (no modifiers), Delete (⌫), Esc,
`?` (Shift allowed), digits 0-9 (Shift allowed: AZERTY), ⌥+letter, plain letters, Space. Anything
with ⌘/⌃ is left to menus.

Focus classification (`ResponderFocusProvider`, lines 55-77): first responder → `.textInput`
(NSText/NSTextField), `.reader` (inside a WKWebView), `.sidebar`/`.entryList` (NSTableView in split
column 0/1), `.window`, `.control`.

Native mapping (`nativeCommand`, lines 136-149), never when `.textInput`:
| Key | Focus | Command |
|---|---|---|
| j | any | next entry |
| k | any | previous entry |
| m | any | toggle read |
| s | any | toggle star |
| Space | sidebar, list, reader, window | advance (scroll reader one screen, at bottom next unread) |
| ↓ / ↑ | list, reader | next / previous entry (sidebar keeps its own arrows) |
| → | sidebar | focus entry list (selects first entry if none) |
| ← | list | focus sidebar |

If `AppState.perform` returns false (Garden scope, empty list…) the key continues to the focused
view (so Garden's own `k`, `r`, `b`… work). Held keys: a repeated toggle is spent without toggling;
a held Space scrolls but never moves to the next entry (`AppState.swift:432-447`). Only one Space at
a time (`isAdvancing`).

Space semantics (`ReaderController.pageDown`, `ReaderView.swift:222-235`): JS via
`callAsyncJavaScript` (app world; page JS stays off): if `readyState==="loading"` → spent; if at
bottom (`scrollTop+innerHeight >= scrollHeight-2`) → false → `selectNextUnread()`; else
`scrollBy(0.9*innerHeight, smooth)`. While the new entry's navigation hasn't committed, Space is spent
without action. `selectNextUnread` can jump beyond loaded rows with one SQL query
(`Queries.nextUnread`, `Queries.swift:326-338`).

Antiope keys are listed in §5.5.

### 2.7 Reader (`Reading/ReaderView.swift`, `Reading/ArticleTemplate.swift`, `Resources/reader.css`)
- `WKWebView`: `allowsContentJavaScript = false`, `websiteDataStore = .nonPersistent()`, media
  require user action, no back/forward gestures, no magnification, custom URL scheme handler
  `ressac-mosaic:` (`ReaderView.swift:352-360`). Under-page colour: white / `#1C1C1E` dark; black in Antiope.
- Page loaded with `loadHTMLString(html, baseURL: nil)` (document URL `about:blank`).
- Reload policy: page identity = entry id, title, url, author, summary, content, fulltext row,
  showFull, style, colour scheme, theme — **not** read/star flags (marking read never reloads/loses
  scroll) and not the Antiope sub-page line (rewritten in place by JS) (`ReaderView.swift:174-188, 335-350`).
- **Content shown**: if full article toggled and `entry_fulltext.html` exists → it; else first non-blank
  of `content_html`, `summary_html`; else "This entry has no text[. Open it in the browser to read it.]".
- **Template** (`ArticleTemplate.page`, lines 538-584): `<html lang=en data-scheme=light|dark
  [data-theme=antiope] [data-crt]>`; CSP `default-src 'none'; img-src http: https: data: ressac-mosaic:;
  style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'`; `referrer no-referrer`;
  inline `<style>` = font faces (Newsreader/Atkinson/Bedstead as base64 `data:` URLs, because the web
  process can't see app-registered fonts) + reader.css + `:root{--reader-font;--reader-size:Npx;--reader-measure:Nch[;--teletext-font;--font-pixel]}`.
  Body: [Antiope `<p class=subpage id=subpage>412 3/20`] · `<p class=meta>` feed · long date+time ·
  [badge "full article"] · `<h1 class=title>` title linked to entry URL (Antiope: double-height via two
  spans `dh-space` hidden + `dh-ink` scaleY(2)) · byline (fulltext byline else author) ·
  `<p class=notice>Full article unavailable: <error>` · `<div class=content>`.
- **Sanitiser** (`ArticleTemplate.sanitize`, lines 9-530): parses with Foundation `XMLDocument`
  (`.documentTidyHTML`, libxml2/tidy, **macOS-specific**) then re-serialises an allowlist:
  dropped with content: applet audio base button canvas dialog embed form frame frameset head iframe
  input link math meta noscript object option script select source style svg template textarea title
  track video; kept elements (a, abbr, article … wbr — full list lines 20-27); other elements
  unwrapped. Attributes: global dir/lang/title; a href; col/colgroup span; details open;
  img alt/height/src/width; li value; ol reversed/start/type; td/th colspan/rowspan (+scope); time datetime.
  `href` only http(s) (relative resolved against entry URL); `src` http(s) or `data:image/*` except SVG.
  Images get `loading="lazy"` and width/height from attributes or `style` px (1…20000; `data-*` sizes
  ignored) so Space knows the page height early. Guards: input >512 KB, estimated nesting depth >256
  (libxml-faithful linear estimator), or >2048 attributes on one tag → escaped plain-text fallback;
  parse runs on a 256 MB-stack thread with a 1 s budget, max 2 workers, timeout → plain text (not
  memoised); LRU memo of 32 results. Comments/PI dropped.
- **Links** (`LinkPolicy`, lines 658-695): only `about:blank` (type other) loads; clicked http(s) link →
  cancel + `NSWorkspace.open` (default browser); `ressac-toggle:<n>` → swap mosaic n; everything else cancelled.
- **Images**: Native → loaded straight from the network by WebKit (tracking pixels accepted, spec §10).
  Antiope → mosaics (§5.7).
- **Full-article toggle** (`AppState.requestFullText`, lines 546-574): if showing → back to feed text;
  if `has_fulltext` → show stored; else run `ressac extract <id>` (short run), spinner, and on failure
  a notice "Full article unavailable: <error>" (from `extracted` event `error`, or the run failure).
  Stored `entry_fulltext.error` is rendered inside the page; a run failure without stored error is shown
  as a bar above the page. Extraction itself (Readability port `dom_smoothie`, 30 s, errors like
  "The page is behind a login.") is entirely in the core. Failures never use the banner.
- `readerShowsFullText` initialises to `has_fulltext` on selection (so feeds with stored full text open on it).
- Full-article fetching during refresh for `prefer_full_text` feeds: core (≤20/feed/run), progress
  label "Fetching full articles".
- Text size, font, width, scheme: §7.

reader.css palettes (Native): light paper #FFFFFF ink #1D1D1F link #0066CC …; dark paper #1C1C1E ink
#E3E3E8 link #6CB4FF, image filter brightness .9 contrast 1.05; body padding 28/40/96 px; article
`max-width: var(--reader-measure)` centred. Antiope block: §5.6. CRT block: §5.8.

### 2.8 Search (`Queries.swift:141-144, 343-351, 381-387`)
FTS5 over `entry_fts` (core-maintained: title, author, body; `unicode61 remove_diacritics 2`),
**within the current source**; every whitespace token quoted (`"word"`) so FTS operators are literal;
AND of all words; unread filter ignored during search; results replace the list until text cleared
(Esc in field / Antiope Esc). Paged like lists.

### 2.9 Drag and drop
- **Feed → folder** (`Reading/FeedDrop.swift`, `AppState.dropFeeds`, lines 621-637): feed rows
  `.draggable("ressac-feed:<id>")` (plain-text payload); folder rows and the "Feeds" header accept
  `String` drops; first payload with prefix+digits wins; drop onto current folder (or root feed on
  header) is ignored; else `ressac feed move <id> <folderId|root>` (short run). Core renumbers the
  Antiope page. Known gaps (TODO.md:124-125): dropping on a feed row inside a folder does nothing; dragging
  a feed into a text field pastes `ressac-feed:<id>`.
- **OPML file → window**: §7.2.
- **Entry → out**: URL + title (§2.4).

---

## 3. Journal / morning edition

Core composes and stores; app only checks, runs, displays (`App/JournalModel.swift`,
`Reading/JournalList.swift`, `Data/Edition.swift`).

- **Day start** 04:00 local (`EditionClock.dayStart`, `Edition.swift:117-133`; DST via Calendar).
- **Due check** (direct SQL): `SELECT NOT EXISTS (SELECT 1 FROM edition WHERE built_at >= ?)` with
  `?` = day start (`Queries.swift:278-284`).
- **Triggers** (`AppState.startSystemObservers`, lines 227-256): at launch, on
  `NSApplication.didBecomeActive`, and a rollover task sleeping until the next 04:00
  (`Task.sleep` on continuous clock ⇒ fires on wake if asleep) (`JournalModel.swift:96-118`).
  Not in unit tests. Only one check/build at a time (`isBusy`).
- **Build**: `ressac edition build` as a **long** run through `RunCoordinator` (refused if another long
  run active → phase `.due`, Retry offered). Events: `started{total}`, `feed_done…`,
  `network_unreachable`, then `edition{id,entries,front,week,wire,overflow,unreachable}`.
- **Phases** (`JournalModel.Phase`): idle · due · building(done,total) · offline · failed(msg).
  Exit 2: if `network_unreachable` was seen → offline; else treated as "built elsewhere"
  ("An edition was already built this morning." banner removed) → idle. Exit≠0,≠2 → failed.
  After offline/failed, **automatic** triggers wait 15 min (in memory; a backwards clock expires the
  wait); **Retry** is immediate and re-checks due-ness first (`JournalModel.swift:39-94, 125-157`).
- **Data** (`Queries.edition()`, `Queries.swift:225-245`): latest `edition` row (header even if empty) +
  `v_edition_today` rows (entry, section, rank, feed, titles, published_at, read/starred live) and
  `v_edition_overflow` (section, feed, title, page, hidden_count) — both filtered to feeds still
  `state='active'` (buried during the day → disappears). Edition is "finished" when ≥1 item and all read.
- **Native Journal screen** (`JournalListView`): placeholder instead of the list when:
  building → spinner + "Building this morning's edition… 34/156"; offline → "Offline: no edition yet" +
  Retry; failed → the error text + Retry; due → (no edition text) + Retry; no edition → "No edition yet";
  empty edition → "Nothing new this morning.". Otherwise: header
  `Friday 16 October · 43 entries · 12 read[ · 3 feeds unreachable]` (en_US_POSIX, "EEEE d MMMM",
  singulars), then sections **Front Page** (front: monthly+rare feeds, uncapped), **This Week** (weekly,
  cap 3/feed), **The Wire** (daily, cap 1/feed); empty sections omitted; entries in rank order
  (regular `EntryRowView`); an **overflow row** "+12 more in Actualitté" (greyed, not selectable, not
  focusable) placed right after the last listed entry of that feed (end of section if none), click →
  select that feed in sidebar; when finished, closing line "That's the edition.". The list grabs keyboard
  focus on appear (acceptance fix, notes.md:198). Rows come from `AppState.entries` (so search and
  unread filter apply) merged with edition sections (`JournalLayout.sections`).
- Space within Journal moves to next unread **of the edition, in rank order**.
- Refresh (⌘R) while Journal/rhythm/smart list selected = fetch all.
- **Antiope P104**: §5.4.

---

## 4. Garden

Shared state machine `Garden/GardenStore.swift` (both looks), pure logic `Garden/GardenModel.swift`,
Native views `GardenTable.swift` / `GardenInspector.swift`.

### 4.1 Data (`Queries.garden`, `Queries.swift:178-199`)
`v_garden_row`: feed_id, title, folder_path ("A / B"), state, status, reason, clue, evidence (JSON
`{bullets[], clue, language?}`), remarks (JSON array, tagged `type`: https_available, http_only,
similar_title{feed_id}, date_fallback, blocked; unknown kept), proposed_action (JSON tagged `type`:
upgrade_https{url}, follow_redirect{url}, adopt_discovered{url}, merge_into{feed_id}, bury; unknown
shown as "<type> (newer than this app)", never applied), needs_triage, last_decision, wayback_url,
html_url, xml_url. Scope filters: needsTriage `needs_triage AND state='active'`; all `state='active'`;
cemetery `state='buried'` plus an unread count computed on `entry` directly
(`v_unread_count` covers active feeds only). Order: urgency CASE (Duplicate 0, Hijacked 1, Moved 2,
Dead 3, Ailing 4, Fossil 5, Dormant 6, Alive 7, else 8), title NOCASE, id.
Fetch log: `SELECT id, at, http_status, error_kind, redirect_chain FROM fetch_log WHERE feed_id=? ORDER BY at DESC, id DESC LIMIT 20`
(`redirect_chain` JSON `[{status,url}]`; `error_kind` JSON like `"Dns"` or `{"Http":404}`).
The store observes `.all` and `.cemetery` concurrently and merges.

### 4.2 Table (Native `GardenTable`)
- Header row (44 pt): scope title ("Needs Triage"/"All Feeds"/"Cemetery"); capsule count
  ("N of M left" in triage — N = needs_triage count, M = rows with problem status
  {Duplicate,Hijacked,Moved,Dead,Ailing,Fossil}; else "N feed(s)"); in All Feeds a segmented
  **Status | Rhythm** grouping picker; last refusal message (caption, fades); **Apply N Safe Repairs**
  (prominent; hidden in Cemetery; disabled at 0; tooltip "…(⇧R)"); **Undo** (⌘Z).
- Sections per status in urgency order (`GardenModel.sections`), header `"<glyph> STATUS · count"`;
  glyphs 👯 Duplicate, 🦠 Hijacked, 🚚 Moved, 🪦 Dead, 🤒 Ailing, 🦴 Fossil, 🌙 Dormant, 🌿 Alive,
  ⚰️ Buried, ❔ Unchecked/unknown. Within section: top folder, then title (case-insensitive).
  Cemetery: a single "Buried" section. Rhythm grouping (All Feeds): ⚡ Daily, 📅 Weekly, 🌗 Monthly,
  ✨ Rare, 💤 Silent, ❔ Unchecked.
- Row: glyph, title, clue (secondary, monospaced digits), rhythm label in All Feeds ("Daily" or
  "Daily*" if overridden), "N unread" for buried feeds, chip (capsule): decision made → kept/repaired/
  buried; else action → https/follow/adopt/merge/bury; none but problem status → "wait". Tooltip:
  folder path + reason.
- **Retention/dimming**: feeds seen needing triage during this visit stay listed, dimmed (opacity .45),
  after being decided (here, via ⇧R, or in a terminal) until the scope is left; in All Feeds a feed
  buried during the visit stays dimmed in place (`GardenModel.swift:69-86`).
- **Selection**: on entering, first feed needing triage (else first); when the selected feed leaves the
  queue, selection moves to the next one still needing triage, wrapping (`selectionAfterUpdate`).
- Empty: "Nothing to triage" / "No feeds" / "The cemetery is empty".
- **Keys** (`GardenModel.command`, lines 165-200; `.onKeyPress` on the focused list; any ⌘/⌃/⌥ chord
  ignored so ⌘B never buries): `u` undo · `R` (⇧R) confirm safe repairs (or message "No safe repairs
  pending.") · `k` keep (= restore in Cemetery) · `r` repair if action is a repair else "Nothing to repair
  for this feed." · `b` bury · `o` open site ("This feed has no site address.") · `w` Wayback ("No Wayback
  snapshot yet (run `ressac check`).") · on buried feed `r`/`b` → "This feed is buried; press k to restore
  it first." · no selection → "Select a feed first." · non-web URL → "Refusing to open a non-web address.".
  Arrow keys navigate (native List). The table takes focus on appear.
- **Safe repairs**: rows active, undecided, action upgrade_https or follow_redirect. ⇧R/button opens a
  confirmation dialog "Apply N safe repairs?" listing first 8 "Title: label" lines + "and N more.",
  buttons Apply/Apply All + Cancel → `ressac garden apply-safe`.
- Decisions: `ressac garden decide <id> keep|repair|bury`; undo: `ressac garden undo` (shared undo stack
  with the terminal garden; `undone{feed_id?}` removes the id from `decidedThisVisit`). All short runs.
  Exit 2 from decide/undo = store refusal (address used by another active feed) → banner with CLI
  sentence, no Show Log.

### 4.3 Inspector (Native `GardenInspector`)
Title (selectable) + folder path; status line "<glyph> Status" in `nativeColor`
(Dead red, Hijacked purple, Moved blue, Ailing orange, Duplicate teal, Fossil brown, Dormant indigo,
Alive green, other secondary); if buried "Buried. Press k to restore it."; reason paragraph;
**Rhythm line** (active feeds with a `v_cadence` row): `Rhythm: Daily · median 0.4 d · last entry 2 h ago[ (set by you)]`
(median `%.1f` if <10 else integer; ages "N min ago" <60 min, "N h ago" <48 h, "N d ago" <60 d, else
"N mo ago") + pop-up **Auto / Daily / Weekly / Monthly / Rare / Silent** → `ressac feed cadence <id> <group|auto>`;
**Theme** pop-up (active feeds): None + themes by position (`SELECT id,key,name,sea_name,position FROM theme ORDER BY position,id`;
current from `feed JOIN theme`) → `ressac feed theme <id> <key|none>`; **Evidence** bullets; **Remarks**
sentences (`GardenModel.remarkSentence`); grid: Proposed (action label: "switch to URL", "follow redirect
to URL", "adopt discovered feed URL", "merge into #id", "bury → cemetery"), Decision, Feed (xml_url),
Site link, Wayback "Archived copy" link; buttons **Keep k / Restore k**, **Repair r** (only if action is a
repair; prominent), **Bury b** (prominent if action is bury), **Open Site o**, **Wayback w**; disclosure
"Redirects and fetch log": redirect chain of latest fetch ("301 → url"), then up to 20 log lines
(date + outcome label: "HTTP 404", "HTML page instead of a feed", "anti-bot challenge page",
"response too large", raw kind name, or "no response"; "Not fetched yet.").
Empty: "No Feed Selected — Select a feed to see why it is in the garden."

### 4.4 Wayback
`wayback_url` is computed by the core (`ressac check` looks archives up for dead feeds); the app only
displays/opens it (http/https only).

---

## 5. Antiope look

### 5.1 Switch and requirements
- `@AppStorage("appearance")` = `native` | `antiope`; applied live, keeps selection/entry/search/run
  (`RessacApp.swift:59-70`). Effective Antiope only if Bedstead registered (`TeletextFont.isAvailable`
  = NSFont lookup succeeded → `CTFontCopyCharacterSet`); otherwise logged once, Native forced, picker
  disabled with "Antiope needs the Bedstead font, which did not load." (`SettingsView.swift:97-108`).
- **Font**: `Resources/Fonts/bedstead.otf` — Bedstead 3.261 by Ben Harris et al., **CC0 1.0**
  (`Bedstead-CC0.txt`), SAA5050 teletext design. Metrics: advance 600/1000 em, ascent 800, descent
  200 ⇒ cell = 0.6 em × 1.0 em; 1 font pixel = size/10 (`Teletext/TeletextFont.swift`). Sizes snap to
  **15, 20 (default), 25, 30** pt (ties go up); `Font.custom(…, fixedSize:)` (ignores Dynamic Type).
  Embedded in the reader page as base64 `@font-face` (`ArticleTemplate.swift:619-627`).
- **Colours** (`Teletext/TeletextColor.swift`): black #000000, red #FF0000, green #00FF00, yellow
  #FFFF00, blue #0000FF, magenta #FF00FF, cyan #00FFFF, white #FFFFFF, in sRGB. No other colour; blue
  never colours text (only separators/backgrounds/mosaics; Fossil heading is white-on-blue).
- **Cell text** (`Teletext/CellText.swift`): every string is fitted to exactly N cells: one grapheme
  per cell (NFC composed), whitespace → space, format/control chars dropped, unsupported grapheme
  (emoji, CJK…) → magenta `?`; truncation without ellipsis; padding with spaces; original string kept
  for VoiceOver. `CellLine` = spans (text, fg, bg) of exact widths; `CellLineView` draws each span in a
  frame exactly `cells × cellWidth` wide (`Antiope/CellLine.swift`). Reverse video = swap colours.

### 5.2 Window layout (`Antiope/AntiopeRootView.swift`)
Top→bottom: `PageHeader` (1 row) · optional banner row (red) · three columns · `MinitelBar` (1 row).
Columns: index **22 cells**, separator (1 cell: right half blue, full height, a shape not a glyph),
entries **28 cells**, separator, reader (rest; min 24 cells; window min width = 22+1+28+1+24 cells).
Each column has a 1-row title (`INDEX`, `ENTRIES  ALL|UNREAD`, `READER`): active column reverse video
black-on-cyan, others cyan-on-black; ENTRIES title is clickable (focus column + toggle unread filter);
READER title blank while inactive on P100/P899/end page. Rows are `ScrollView + LazyVStack` of fixed
cell rows; `.scrollIndicators(.never)` (legacy scrollbars would steal cells, notes.md:142).

### 5.3 Pages and navigation
**Page numbers**: core-assigned `folder.page`, `feed.page` (schema v3; root folders 200,300…700,
contents next free numbers, root feeds 110-199, overflow pool 110-799). App builds a `PageDirectory`
from the sidebar snapshot (not from `ressac page list`) (`Antiope/PageDirectory.swift`): reserved pages
first, then folders/feeds depth-first in display order; rebuilt only when the snapshot changes.

Reserved pages (`ReservedPage`): 100 Index (SOMMAIRE, drawn in reader), 101 Unread, 102 Today,
103 Starred, 104 Journal, 105 Daily, 106 Weekly, 107 Monthly, 108 Rare, 109 Silent, 800 Needs triage,
801 All feeds, 802 Cemetery, 899 Guide (drawn in reader). Counts: unread for 101-109 (104 = edition
unread), triage count for 800 (red), 0 otherwise.

**Header** (`PageHeader`, lines 120-206): `P412 RESSAC … WED 14 OCT 09:47:12` — page in yellow (`P---`
if none), RESSAC white, date cyan (`EEE dd MMM` uppercased, en_US_POSIX, gregorian), time yellow
(`HH:mm:ss`), ticking each second (`TimelineView`). Precedence: typing `P4__` → `P437 NOT FOUND` (red,
2 s, hides RESSAC) → rolling counter during any long run (100…899, one step per 0.1 s; off under Reduce
Motion) → current page. Narrow window: time keeps cells first, then date, then page. With CRT on, a
right margin of `ceil((cornerRadius + 1 + 2)/cellWidth)` cells (≤ width/4) keeps the time clear of the
tube corner.

**Page entry** (`Antiope/PageEntry.swift`, `AntiopeState.jump`): digit outside text input starts
entry; 3rd digit jumps; ⌫ CORRECTION erases; Esc ANNULATION cancels; a new digit clears NOT FOUND.
Jump: selects the source (index row highlighted, entry list follows), reader shows article, focus to
index column; 100/899 open in the reader column; unknown → NOT FOUND 2 s, nothing else changes.

**Index column** (`PageIndexColumn`): row `NNN title…count` — number cyan (4 cells), title indented
by depth (≤3 spaces), root folders GREEN UPPERCASE, garden pages magenta, others white, count yellow
right-aligned (P800 red), max 99,999; selected row black-on-cyan; click = jump; scrolls to current.

**Entry column** (`EntryPageColumn`): per entry 1–2 rows: yellow `●` (unread) in 2 cells, title
word-wrapped over ≤2 rows in width−4 cells, ` *` white if starred; selected black-on-white; click
selects + reader article; empty = message uppercased, centred cyan ("NO UNREAD ENTRIES").
VoiceOver label "Title, Feed, unread, starred".

**Sub-pages**: reader line `412 3/20` = entry's own feed page + position in the current list / list
total (even inside Unread); Journal: `104 rank/N` (`SubpageLabel`, `ReaderView.swift:148-169`).

**Minitel bar** (`Antiope/MinitelBar.swift`): keys in reverse video black-on-cyan, clickable, each
followed by a black cell: ` SOMMAIRE ` (→100, ⌥S) · ` SUITE ` (advance: Space/j) · ` RETOUR `
(previous: k) · ` GUIDE ` (→899, ⌥G or ?) · ` RÉPÉTITION ` (refresh selection, ⌥R; on P104 when
offline/failed/due it retries the edition). Right of keys, one line of yellow status or white hints:
garden refusal message › `EDITION IN PROGRESS 34/156` › `REFRESHING 42/155` › hints: `SEARCH: <text>`
(cyan) if a search is active; garden `k keep · r repair · b bury · u undo`; index `type a page number · ? guide`;
entries `j next · k previous · m read`; reader `m read · s star · i images`. VoiceOver labels e.g.
"Sommaire, go to page 100". Keys labels stay French (only exception besides FIN DE L'EDITION).

**Search prompt**: ⌘F replaces the hints with ` SEARCH? ____` (cyan label, yellow Bedstead text
field bound to `searchText`); Return keeps search and closes; Esc clears and closes; losing focus
closes (search kept). Switching looks closes the prompt but keeps the text.

### 5.4 Page types (reader/entry columns)
- **Article pages** (any source): entries column + `ReaderView` (themed WKWebView, §5.6).
- **P100 SOMMAIRE** (`SpecialPages.swift:28-109`): double-height centred yellow `RESSAC`, two rows
  of cyan half-block wave (`" ▄▀▀▄   "` / `"▀    ▀▄▄"` repeated), blank, then every page in two
  column-major columns (index row style with reserved titles in yellow, garden magenta); click = jump.
- **P899 GUIDE** (`SpecialPages.swift:124-206`): double-height `GUIDE`, sections (heading black on
  green): PAGES (0-9, DEL, ESC, OPT S, OPT G ?), READING (SPACE J, K, UP DOWN, M, S, I, OPT R,
  CMD RETURN), COLUMNS (LEFT RIGHT, UP DOWN in index), GARDEN (K R B U O W, SHIFT R), MENUS (CMD F,
  CMD R, SHIFT CMD R, SHIFT CMD A, CMD + -, CMD O, SHIFT CMD E); key column 12 cells cyan, action white.
- **P104 JOURNAL** (`Antiope/JournalPage.swift`): entries column shows: building → centred yellow
  `EDITION IN PROGRESS n/N`; offline → reverse red `OFFLINE: NO EDITION YET` + cyan `RÉPÉTITION: RETRY`;
  failed → reason uppercased reverse red; no edition → `EDITION IN PROGRESS`; else header
  `FRI 16 OCT` cyan + `12/43 READ` yellow (+ red `3 FEEDS UNREACHABLE`); empty → `NOTHING NEW THIS MORNING.`;
  sections with heading ` FRONT PAGE … n` yellow / ` THIS WEEK` cyan / ` THE WIRE` green; entry rows as
  entry column; overflow rows `  +12 MORE IN ACTUALITTÉ` white on blue (click → feed's page, or select
  feed if unnumbered; mouse only, no keyboard — accepted gap, TODO.md:67); once all read, double-height
  yellow `FIN DE L'EDITION`. SUITE past the last entry (by rank) lands on an **end page** in the reader
  (double-height `FIN DE L'EDITION`) and marks the last entry read; `j` lands immediately, Space first
  scrolls the article; RETOUR from the end page returns to the last entry.
- **P105–P109**: rhythm group lists (regular entry column over `.cadence(group)`).
- **P800/P801/P802 garden pages** (`Antiope/GardenPage.swift`, `GardenPageViews.swift`): entry column:
  title row (`NEEDS TRIAGE … 68 LEFT` / `ALL FEEDS … 186 FEEDS`, count yellow); P800 progress gauge
  `▐` cyan + `█` green done + `░` blue remaining + ` done/total` yellow; status headings in teletext
  colour (Dead red, Hijacked magenta, Moved cyan, Ailing yellow, Duplicate white, Fossil white-on-blue,
  Dormant/Alive green, other white); feed rows: yellow `●` if needs triage, title, buried unread count
  yellow, chip (proposed action cyan / decision green); selected black-on-white; click selects.
  No Status|Rhythm grouping picker in Antiope (status only). Keys as Native table (`onKeyPress`), plus
  ↑/↓ walk the list unless the index column is active; hidden ⌘Z button; same confirmation dialog for ⇧R.
  Reader column = **inspector page**: title, folder path (cyan), double-height status heading, "Buried.
  Press k to restore it." (yellow), reason, `RHYTHM    Daily · median …` then 6 clickable rows
  `AUTO DAILY WEEKLY MONTHLY RARE SILENT` (current one reverse black-on-cyan) → `feed cadence`;
  EVIDENCE (green, `- ` bullets), REMARKS (yellow), LINKS (cyan: PROPOSED, DECISION, FEED, SITE,
  WAYBACK, labels padded to 10 cells), REDIRECTS (magenta, `301 url`), FETCH LOG (green; dates
  `dd MMM yy HH:mm` uppercased). **No Theme picker in Antiope [gap vs Native].**
- **Banner** (`GardenPage.bannerRow`): one red row: message white on red, ` LOG ` (yellow on red,
  opens logs) when applicable, ` X ` dismiss.
- **Empty reader**: centred cyan `NO ENTRY SELECTED`; run-failure notice: one row black on red.

### 5.5 Antiope keyboard (`ReadingKeys.antiopeCommand`, `AntiopeState.handle`)
Antiope commands are tried first; unhandled ones fall back to the Native mapping (§2.6).
| Key | Command |
|---|---|
| 0-9 (Shift ok) | page digit (even with a button focused; never in text input / open prompt) |
| ⌫ | CORRECTION (only while typing a number; held key repeats) |
| Esc | ANNULATION (only while typing) |
| ⌥S | SOMMAIRE (P100) |
| ⌥G, ? | GUIDE (P899) |
| ⌥R | RÉPÉTITION: refresh current source (⌘R semantics) or retry edition on P104 |
| i | swap all images mosaic↔original (article only) |
| → / ← | next / previous column (index → entries → reader); entering entries selects first entry |
| ↓ / ↑ in index column | next / previous page in the directory (jump) |
| j, Space / k | SUITE / RETOUR (on P100/P899 the first press only brings the article back) |
| m, s | read / star |
Once-per-press (held key spent): digits, Esc, ⌥S, ⌥G/?, ⌥R, i, ←/→. `AntiopeState` doubles as the
`KeyFocusProvider`: index→sidebar, entries→entryList, reader→reader, prompt/text field→textInput.

### 5.6 Antiope reader stylesheet (`reader.css:281-533`)
`data-theme="antiope"` ignores light/dark: Bedstead for every font role, black paper, white ink,
cyan links (no underline), magenta quote bar, green code, `mark` yellow reverse, notice red reverse,
one size/weight for headings, h2 green, h3 cyan, `--row: 1.2em`, `--cell: 0.6em`, every vertical
distance a whole number of rows, `-webkit-font-smoothing: none`, kerning/ligatures off, no scrollbar,
`.subpage` line, double-height title (`scaleY(2)` over a hidden copy). Line width 40 columns = `40ch`
= exactly 40 Bedstead cells. Size from `reader.size.antiope` snapped.

### 5.7 Mosaics (`Reading/MosaicSchemeHandler.swift`, `Teletext/MosaicRenderer.swift`)
1. Sanitiser (Antiope only) replaces each http(s) `<img>` by
   `<a class="mosaic" href="ressac-toggle:<n>" id="m<n>" data-original="<url>" data-mosaic="ressac-mosaic:image?cols=40&src=<percent-encoded url>" [style="min-height: calc(<rows*10>px * var(--font-pixel))"]><img src="ressac-mosaic:…" loading=lazy><span class="mosaic-alt">IMAGE <alt></span></a>`
   (a `<span>` instead of `<a>` when already inside a link). Reserved height computed from declared
   width/height so Space knows the page height.
2. `WKURLSchemeHandler` for `ressac-mosaic:` validates (cols 1…40, src http/https with host), checks
   an `NSCache` (16 MB cost limit, key "cols url"), else downloads with an **ephemeral URLSession**
   (no cookies, no cache, 15 s request+resource timeout, 10 MB cap enforced while streaming and from
   Content-Length, non-2xx → failure, redirects only to http(s)).
3. Decode with ImageIO: refuse >40 Mpx before decoding; first frame (GIF); thumbnail downsampled to
   ~2 image px per font px. Columns = ceil(width/12 px), ≤40; rows = round(0.6·cols·h/w), ≤100 (cols
   shrink to keep aspect).
4. Render (pure): draw over black (transparency → black); cell = 6×10 font px split 3+3 columns ×
   3+4+3 rows (SAA5050 sextants); for each cell average the 6 sub-pixels, choose the colour pair
   (a≤b among 8 colours, including uniform) minimising Σ min squared RGB distance, assign each
   sub-pixel to the nearer; canonical form (uniform → fg=bg mask 0; background = majority, tie → first
   in enum order). Output PNG with one pixel per font pixel (scale 1), shown with `zoom: var(--font-pixel)`
   and `image-rendering: pixelated`.
5. Response: `200 image/png`, `Cache-Control: no-store`. Failure → 1×1-cell black PNG + JS marks
   every box with that URL `data-failed` → CSS shows magenta `IMAGE <alt>` line (WebKit's own broken
   image icon is avoided). Transient failures (network, 429, 5xx) not cached; stopped tasks never
   called back.
6. Swap: click on `ressac-toggle:n` (navigation delegate) or `i` → app JS (`callAsyncJavaScript`,
   page JS stays off) toggles `.original` and swaps `img.src` between `data-mosaic` and
   `data-original` (`ReaderView.swift:237-270`). Original loads from the network only on demand.

### 5.8 CRT screen (`Antiope/CRTOverlay.swift`)
Active iff Antiope **and** `antiope.crt` checked. Not a GPU shader: a SwiftUI **`Canvas` overlay**
over the whole Antiope root view, `allowsHitTesting(false)`, accessibility-hidden, redrawn only when
text size changes (Equatable view), no animation:
- **Scanlines**: per font-pixel row (size/10 pt), a black band at opacity 0.28 covering the bottom
  40 % of the row, rounded to whole device pixels (≥1), aligned to window top; fixed while the reader
  scrolls under them.
- **Vignette**: radial gradient black 0 → 0.35 from centre to half-diagonal.
- **Tube corners**: black fill outside a continuous rounded rect, radius 3 % of the shorter side.
- **Colour bleed** (text only, off under Increase Contrast): SwiftUI `TextRenderer` on all `Text`
  below the root: 2 pt blur glow of the text's colour, red copy at +1 pt and blue copy at −1 pt at
  0.45 opacity (drawn via layer + `.sourceIn`), then the text. In the reader: `data-crt` on `:root` →
  `text-shadow: 1px 0 rgba(255,0,0,.45), -1px 0 rgba(0,0,255,.45), 0 0 2px currentColor`. Mosaics/images
  get no bleed. No barrel distortion (clicks must stay aligned with the WKWebView).
- Known perf note: big folders hitch while scrolling (38 ms/s without CRT, 53 with) (notes.md:146-147).

### 5.9 Accessibility
VoiceOver reads original strings (never fitted cells/`?`); wrapped rows read once; decorative rows
hidden; selection traits; Minitel key labels spelled out; Reduce Motion stops the page counter.
Buttons are `.focusable(false)` so Full Keyboard Access doesn't steal Space.

---

## 6. Sea chart

### 6.1 Window and model (`Chart/ChartWindow.swift`, `Chart/ChartModel.swift`)
- Window content: launching spinner; if DB not openable "The database is not open. The main window
  says why."; else `ChartScreen` (keyed by `AppState` identity — a new DB is a new chart).
- **Data**: `RessacCLI.capture(["chart"])` ⇒ `ressac --db <db> --json chart` (plain stdout capture, not
  the event stream; blocking read to EOF on a detached task; SIGINT on cancel). App decodes only
  `{"version":…}`; must equal **1**, else "This build draws chart version 1, but ressac produced version N.";
  decoding error → "ressac printed a chart this app cannot read."; run failure → its last stderr line.
- **Refresh rules**: on window open; on every `AppState.changeTick` change, debounced to one run per
  **2 s** window (trailing); every **5 min** while open (ages and survey time move); a request during a
  run reruns once after. Closing the window cancels everything. A failed refresh keeps the last chart
  and shows a bottom capsule "message + Retry"; failure with no chart → "The chart could not be drawn",
  message, Retry.
- `changeTick` (`AppState.swift:194-212`): a `ValueObservation` touching
  `entry, feed, feed_health, theme, territory, territory_feed` (`SELECT 1 FROM t LIMIT 1` each) without
  dedup ⇒ bumps on every commit GRDB is notified of (app writes, `notifyExternalChanges()` after runs
  and after external polls). (A region observation failed with disk I/O error inside the lock-free
  notify transaction — keep that in mind.)
- **Visibility**: `ChartVisibilityReader` follows `NSWindow.didChangeOcclusionStateNotification`; while
  the chart is visible (even behind another app on a 2nd display), `ExternalChangeMonitor` keeps polling.

### 6.2 Web view (`Chart/ChartWebView.swift`, `Resources/Chart/*`)
- WKWebView: non-persistent data store, JS **on**, no auto windows, no gestures/magnification/link
  preview. Loads `Chart/index.html` with `loadFileURL(…, allowingReadAccessTo: Chart/)`. Navigation
  policy: only the bundled index.html (file URL, no query, main frame); everything else cancelled; no
  new windows; web-process crash → reload.
- Page CSP: `default-src 'none'; script-src 'self' file:; style-src 'self' file:; font-src 'self' file:; img-src 'self' file: data:`
  (no inline code, no network). Scripts: `d3.min.js` (7.9.0, ISC), `chart-core.js` (pure, Node-testable,
  exports `ChartCore`), `chart-draw.js` (exports `ChartDraw` and frozen global `Chart = {load, update}`).
  Fonts: Cormorant Garamond (500, 600, italic 400, italic 500), Cormorant SC (500, 600), IBM Plex Mono
  (400, 500) as woff2, OFL, in `Chart/fonts/`.

### 6.3 Bridge — exact contract
**App → page** (via `callAsyncJavaScript` with arguments, never string-spliced, `ChartWebView.swift:176-186`):
- First document after each page load: `Chart.load(json, prefs)` where `json` is the raw §6.4 string and
  `prefs = {"view": "harbour"|"themes", "ink": "bleu"|"nuit", "animate": Bool}` (animate forced false
  under Reduce Motion).
- Later documents: `Chart.update(json)` (only if different from the one shown). Documents queue until
  fonts are loaded (≤3 s).
**Page → app** via `window.webkit.messageHandlers.ressac.postMessage(obj)` (handler name `ressac`,
main frame only; `chart-draw.js:46-49`):
- `{type:"openFeed", id:<integer>}` → `AppState.selection = .feed(id)`, bring a reading window to front
  (deminiaturize, activate); if no reading window exists, open the `main` window.
- `{type:"openURL", url:<string>}` → only http/https with host → default browser.
- `{type:"prefs", view, ink, animate}` (all three required, animate must be a JS boolean) →
  UserDefaults `chart.view`, `chart.ink`, `chart.animate` (animate not saved while Reduce Motion is on).
Anything else ignored (`ChartBridge.handle`, lines 61-95). A stored unknown ink ("pop", "pirate")
reads back as `bleu`.

### 6.4 Chart JSON (`ressac chart --json`, spec §6)
`{"version":1,"surveyed_at":"…Z","totals":{"feeds":n,"unread_week":n},"themes":[{"key","name","sea","count"}],
"feeds":[{"id","title","theme"|null,"folder","group","age_days","unread_week","entries_week","activity","page"|null,
"entries":[{"id","title","url"|null,"day":"YYYY-MM-DD","read"}]}],"territories":[{"name","feeds":[ids]}],
"continents":[{"name","theme"}]}` — feeds: active, not needing triage, last entry ≤182 days;
activity = Σ 2^(−age/3) over 30 days; 5 latest entries. Computed entirely in Rust.

### 6.5 Page behaviour (`chart-draw.js`)
1920×1080 chart scaled to fit and letterboxed (DPR ≤2), redrawn on resize (120 ms debounce) and DPR
change. Two canvases: base (static drawing) and overlay (animation ~30 fps via rAF: gusts of ink along
sea strokes, harbour light sweep, dashed ripples round islands with unread entries, flicker of busiest
lights). Views: "From the harbour" (distance = log time since last entry, rings 1 d/1 w/1 mo/3 mo/6 mo,
bearing = theme sector, d3 forces) and "By theme" (6×3 grid archipelagos, island-by-island placement in
id order, UNCHARTED WATERS cell). Inks: `bleu` "Blue ink" (paper #f7f8f6, ink #1f3c88, heat celadon→copper)
and `nuit` "White on black" (paper #0b0d12, ink #e8ecf4, heat teal→amber) (`chart-core.js:39-44`).
Floating top-right toolbar (fades after 2.5 s idle): View (2), Ink (2), Animation, Key (legend panel with
symbol canvases). Hover (within 10 chart units of an island): tooltip `title` + "N unread · N entries this
week" kept inside the window. Click island: popover (title button → openFeed; 5 latest entries: day,
title as link → openURL, unread in bold with ●; facts "theme · folder X · Rhythm · last entry N d ago ·
N unread this week · page N"; footnote "Click the name to open this feed in Ressac."); click elsewhere
closes popover and key; Esc closes popover then key. Empty chart: "No feed has published in six months".
Labels never overlap (priority placement); silent-but-recent feeds drawn as "ED" islets. Reduce Motion
via `prefers-reduced-motion` (animation off unless switched on by hand). Feed text only through
`textContent`/canvas text.
**No positions are persisted**: everything recomputed from the JSON each time ("By theme" is
deterministic and stable per feed; harbour view drifts with time by design).

---

## 7. Add feed, OPML, Settings

### 7.1 Add Feed sheet (`App/AddFeedSheet.swift`, `AppState.addFeed`)
⌘N → sheet "Add a Feed", 420 pt: address field "Feed or site address" (focused; prefilled from the
clipboard when it is exactly one http(s) URL with host), **Theme** pop-up (None + themes by position,
live from `theme` table), **Folder** pop-up ("Feeds" = top level, then all folders in sidebar order
indented with em spaces per depth; resets to top level if the folder disappears), Cancel (Esc) / Add
(Return; disabled when blank or running). Runs `ressac add <address> [--theme <key>] [--folder <id>]`
as a **long** run (refused while another long run: "A refresh is running. Try again when it ends.").
Progress bar (if events) or spinner. Success: sheet closes, sidebar selects new feed, bottom note
"In <theme>, like <sibling>." / "In <theme>." / "Added to Uncharted waters." (6 s). Errors stay in the
sheet in red (CLI's last stderr line), with **Show Log** unless exit 2 (refusals: "This page announces
no feed.", "Already followed: <title>.", "Already in the Cemetery: <title>.", unknown theme, invalid
address). `added` event: `{event:"added", id, title, xml_url, page, theme, territory, folder, discovered, like}`;
missing → "ressac did not report the added feed.". Folder pop-up shipped beyond spec (spec said drag only;
TODO.md:117). Known gaps: cancelling during an add lets it finish silently and still selects/notes.
Antiope has no add/move UI of its own (menu works in both looks).

### 7.2 OPML import / export (`App/OPMLImport.swift`)
- Entry points: drop of file URLs on the window (first `.opml` by extension; dashed accent overlay
  while targeted), ⌘O (`fileImporter`, type `.opml`), first-run "Choose OPML File…".
- **Preview**: direct `Process` run (not RunCoordinator) of
  `ressac --db <db> --json import --dry-run <file>`; last stdout line starting with `{` decoded:
  `{dry_run, folders_added, feeds_added, merged:[[dropped,kept]…], skipped_existing}`; failure →
  alert "This OPML file can't be imported" with the stderr last line (stripped of `error: `).
- **Confirmation sheet** (420 pt): "Import “file.opml”?", lines "N feeds in M new folders." /
  "No new feeds in this file." / "N duplicates inside the file will be merged:" + "“a” into “b”" lines /
  "N feeds already in Ressac will be skipped.", note "Ressac fetches every feed right after the import.",
  blocked note "A refresh is running; try again when it ends."; Import disabled when 0 new feeds or blocked.
- Confirm: `ressac import <file>` (long) then, if ok, `ressac fetch --force` (long).
- **First run**: whenever the sidebar has no active feeds, the whole window is replaced by
  "No Feeds Yet — Drop an OPML file here, or choose one…" + button + spinner when busy.
- **Export**: NSSavePanel sheet "Ressac.opml" (.opml) → `ressac export --output <path>` (short).
  Cemetery export (`--cemetery`) not exposed in UI.

### 7.3 Settings (`Settings/SettingsView.swift`, `ReaderStyle.swift`, `Appearance.swift`)
Two tabs, 540 pt wide. All persisted in **UserDefaults** (`org.w3cdotorg.ressac` domain via
`@AppStorage`); the app does **not** read/write `terminal.json` (that file is the terminal reader's
`{"look","images"}` in `~/Library/Application Support/Ressac/`, `crates/ressac/src/tui/reader/settings.rs`).

| Key | Values / default | UI |
|---|---|---|
| `appearance` | `native` (default) / `antiope` | Reading tab segmented "Appearance:" (disabled without Bedstead) |
| `antiope.crt` | Bool, false | "CRT screen" toggle, enabled only when Antiope shown; note "Scanlines, a vignette and color bleed over the Antiope window. Increase Contrast turns the bleed off." |
| `reader.font` | `sfPro` (default), `newYork`, `charter`, `newsreader`, `atkinson` | "Font:" picker (each item in its face); disabled in Antiope ("Bedstead in Antiope") |
| `reader.size` | Double, 17, range 12…28 step 1 | Native slider + "N pt"; ⌘+/⌘−/⌘0 |
| `reader.size.antiope` | Double, 20, snapped to 15/20/25/30 | 4-step slider in Antiope; ⌘+/⌘−/⌘0 step |
| `reader.width` | `narrow` 58ch, `medium` 68ch (default), `wide` 80ch, `forty` 40ch | "Line width:" segmented ("40 columns") |
| `databasePath` | String, empty = default | Data tab |
| `chart.view` / `chart.ink` / `chart.animate` | harbour / bleu / true | set by the chart page (§6.3) |
Preview line "The quick brown fox jumps over 1 234 lazy dogs. Il1 O0, « l'écart » se creuse." in the
chosen face/size; hint "⌘+ and ⌘− change the text size while you read; ⌘0 restores it."
Font CSS stacks (`ReaderStyle.swift:21-29`): SF Pro → `-apple-system, BlinkMacSystemFont, "SF Pro Text", …`;
New York → `ui-serif, "New York", Georgia, serif`; Charter → `Charter, "Charter BT", Georgia, serif`;
Newsreader and Atkinson Hyperlegible Next bundled (variable TTF + italic, wght 200–800, OFL) and
embedded as base64 `@font-face` in each article page (~1.3 MB for Newsreader).

**Data tab**: shows resolved path (middle-truncated, selectable); **Choose…** (NSOpenPanel, `.sqlite`),
**New…** (NSSavePanel "ressac.sqlite"; created by `migrate` at next launch; empty file treated as missing),
**Use Default**. A chosen path is validated: newer schema → "This database was created by a newer
Ressac."; not SQLite → "This file is not a usable Ressac database."; then "Ressac opens the new database
after a relaunch." + **Relaunch Now** (opens a new instance of the bundle, terminates this one). Notes:
"Shared with the ressac command in Terminal." / "In Terminal, pass --db with this path to use the same
database." / "RESSAC_DB is set, so this launch uses it instead.". **Logs: Reveal in Finder** (creates
and opens `~/Library/Logs/Ressac`).
DB resolution order (`DatabaseLocation.current`): env `RESSAC_DB` → `databasePath` (tilde-expanded) →
`~/Library/Application Support/Ressac/ressac.sqlite` (same default as the core).

---

## 8. Engine: how the app talks to the core

### 8.1 Process runner (`Engine/RessacCLI.swift`)
- Executable: `Bundle/Contents/Helpers/ressac` (must be executable).
- Invocation: `ressac --db <path> --json <args…>`, stdin `/dev/null`, separate stdout/stderr pipes.
- stdout read with `readabilityHandler` + `LineSplitter` (split on `\n`, strip `\r`, keep partial UTF-8
  across reads) so progress is live; each line → `EngineEvent.decode`: not a JSON object → logged and
  skipped; JSON object with unknown/absent `event` → `.unknown(raw)`.
- stderr drained concurrently, each non-empty line logged (os.Logger, redacted), **last non-empty
  line** kept as the failure message.
- Exit: 0 → stream finishes; signal → `RunFailure(exitCode: -signal)`; other → `RunFailure(code,
  lastStderrLine)`; launch failure → 127 "Cannot launch ressac: …". Message fallback "ressac failed with
  exit code N." / "ressac stopped unexpectedly (signal N).".
- Cancelling the consumer sends **SIGINT** (`process.interrupt()`), which the CLI handles as Ctrl-C (exit 130).
- `capture(args)` for one-document commands (`chart`): whole stdout.
- Logging redaction (`LogRedaction`): URLs lose userinfo, query (`?…`) and fragment; `add`'s address
  always redacted. Logger subsystem `org.w3cdotorg.ressac`, categories `engine`, `runs`, `chart`, `teletext`.
- The core itself writes daily-rotated logs to `~/Library/Logs/Ressac/` (`crates/ressac/src/main.rs:129-133`).

### 8.2 Every subcommand the app invokes (`RunRequest`, lines 64-155)
| Request | Arguments (after `--db … --json`) | Kind | Refusable on exit 2 | Banner on failure |
|---|---|---|---|---|
| fetch all | `fetch --force` | long | no | yes |
| fetch feeds | `fetch --feed <id> [--feed <id>…]` (implies force) | long | no | yes |
| fetch folder | `fetch --folder <id>` (subfolders, implies force) | long | no | yes |
| import | `import <path>` | long | no | yes |
| import preview | `import <path> --dry-run` (run outside RunCoordinator by `ImportPreview`, arg order `import --dry-run <path>`) | — | — | alert |
| export | `export --output <path>` [`--cemetery` unused] | short | no | yes |
| extract | `extract <entryId>` | short | no | **no** (inline) |
| garden decide | `garden decide <feedId> keep|repair|bury` | short | yes | yes (no log if refusal) |
| garden undo | `garden undo` | short | yes | yes |
| garden apply-safe | `garden apply-safe` | short | no | yes |
| migrate | `migrate` | short | no | (launch failure screen) |
| edition build | `edition build` [`--rebuild` unused] | long | yes | yes |
| feed cadence | `feed cadence <feedId> daily|weekly|monthly|rare|silent|auto` | short | yes | yes |
| feed theme | `feed theme <feedId> <key>|none` | short | yes | yes |
| feed move | `feed move <feedId> <folderId>|root` | short | yes | yes |
| add | `add <address> [--theme <key>] [--folder <id>]` | long | yes | **no** (sheet) |
| chart | `chart` (capture) | — | — | chart window |
Not used by the app: `check`, `page list/set`, `edition show`, `chart import/export`, `garden` TUI,
`ticker`, the TUI reader.

### 8.3 Event stream (`Engine/Events.swift`; core `service::Event`, `#[serde(tag="event", rename_all="snake_case")]`)
| `event` | Fields | App use |
|---|---|---|
| `started` | total | progress 0/total; label "Fetching feeds" or edition label |
| `feed_done` | feed_id, title, error? | progress +1; journal building +1 |
| `network_unreachable` | failed, attempted | banner "The network looks unreachable: F of A feeds could not connect." (kept after the run's success); journal → offline |
| `probing` | what ("https", "autodiscovery", "full articles", "wayback"), total | progress reset, label "Checking <what>" / "Fetching full articles" |
| `wayback_done` | found | — |
| `finished` | summary {total, counts{status:n}, https_repairs, redirect_repairs, network_unreachable} (+ `applied`, `skipped` for apply-safe, not decoded) | — |
| `extracted` | entry_id, ok, error? | progress +1; full-article notice |
| `decided` | feed_id, decision | — |
| `undone` | feed_id? | un-dims |
| `edition` | id, entries, front, week, wire, overflow, unreachable | (Journal reads DB instead) |
| `cadence` | feed_id, group, computed, override | — |
| `added` | id, title, xml_url, page, theme, territory, folder, discovered, like | add result |
| `theme`, `moved`, `page`, `report` | — | decoded as `.unknown` |
Import (non-dry-run) prints the same report object without `event` → `.unknown`.

### 8.4 RunCoordinator (`App/RunCoordinator.swift`)
- At most **one long run** (fetch, import, edition build, add); a second long request is refused
  immediately (`.refused`, logged) and the triggers are disabled (`canRefresh`). No queueing.
- **Short runs** are serialised FIFO among themselves (chained tasks) and may overlap a long run.
- Progress (`RunProgress{done,total,label}`) only for long runs; cleared at the end.
- During a run, database-change notifications at most once per second on progress events, and once
  at the end of every run (`onDatabaseChange` → `AppDatabase.notifyExternalChanges()`), so GRDB live
  queries refetch what the subprocess wrote.
- Banner: cleared by a successful run unless the long run itself set one (network warning); failure
  sets `Banner(message: lastStderrLine, showLog: !refusal)` unless the request suppresses banners
  (extract, add). Exit 2 on a non-refusable request is logged as a **fault** (app bug).
- No explicit cancel UI; the only cancellation is task cancellation → SIGINT.
- Fetch scheduling is **manual only** in the app (⌘R, ⇧⌘R, context menu, RÉPÉTITION) plus the daily
  edition build (which refreshes every due feed). The 30-minute freshness rule lives in the core;
  manual refreshes pass `--force` / `--feed` (implies force).

### 8.5 External change detection (`Data/ExternalChanges.swift`)
- `DataVersionWatcher`: own `DatabaseQueue` (read-write open + `PRAGMA query_only = ON`, because a
  read-only open fails on a WAL DB without `-shm`, notes.md:106), reads `PRAGMA data_version`
  (changes only for other connections' commits).
- `ExternalChangeMonitor` polls every **3 s** while `NSApp.isActive || isChartOnScreen`; on
  `didBecomeActive` it re-primes and notifies unconditionally. On change → `notifyExternalChanges()`
  = a deferred empty transaction calling `db.notifyChanges(in: .fullDatabase)` so every
  `ValueObservation` refetches.

### 8.6 Direct SQLite access (`Data/Database.swift`, `Queries.swift`, `Writes.swift`)
- Open: GRDB `DatabasePool` (WAL), busy timeout 5 s, label "Ressac".
- Schema check before open: own queue with query_only; file missing or 0 bytes → `.missing`;
  `SELECT version FROM schema_meta` → `< 6` needsMigration, `> 6` tooNew, `== 6` ok
  (`knownSchemaVersion = 6`, `Database.swift:12`). Missing/older → `ressac migrate` then re-check must
  be `.ok`, else failed screen with banner message or "The database could not be migrated.".
  **Backups are made by the core**, not the app: `VACUUM INTO ressac.sqlite.bak-v<old>` before each
  migration, one transaction, rollback on failure (`crates/ressac-core/src/store/mod.rs:232-242`).
- Reads (all live `ValueObservation`, `removeDuplicates`): sidebar snapshot (folder, feed+v_unread_count+
  feed_health, counts from v_entry, v_garden_row, edition_entry, v_cadence); entry pages and counts
  (v_entry, entry_fts, edition_entry for rank); single entry (v_entry full); entry_fulltext row; garden
  (v_garden_row, entry for cemetery unread); fetch_log; edition (edition, v_edition_today,
  v_edition_overflow, feed.state); v_cadence; theme; feed⋈theme keys; one-shot floor/nextUnread queries
  (v_entry, entry); edition due (edition.built_at); `changeTick` touch-queries.
- `EntryRow` decoded by hand for speed (5,907 rows: 213 ms → 23 ms, `Records.swift:65-77`).
- Writes: only the 4 statements in §2.5.
- Contract views/tables the app depends on: `v_entry`, `v_unread_count`, `v_garden_row`, `v_cadence`,
  `v_edition_today`, `v_edition_overflow`, `entry_fts`, `entry_fulltext`, `fetch_log`, `folder`
  (id,parent_id,name,position,page), `feed` (id,title,folder_id,position,state,prefer_full_text,
  html_url,page,theme_id), `feed_health` (status), `edition`, `edition_entry`, `theme`, `schema_meta`.

### 8.7 Concurrency rules worth keeping
Main-actor UI state; CLI in an actor; image downloads/decoding/rendering off-main; sanitiser on a
dedicated big-stack thread with timeout; at most one Space/advance in flight; list page loads deferred
one run-loop turn (avoid re-entrant table updates).

---

## 9. macOS-specific pieces and Qt/KDE mappings

| macOS feature (file) | What it does | Qt 6 / KDE / Linux equivalent |
|---|---|---|
| WKWebView reader, JS off, non-persistent store (`ReaderView.swift:352`) | Article rendering | QtWebEngine `QWebEngineView` with `QWebEngineProfile` off-the-record; `QWebEngineSettings::JavascriptEnabled=false`; app-side JS via `runJavaScript` in `QWebEngineScript::ApplicationWorld` still works with page JS disabled? **Verify**: with JavascriptEnabled=false QtWebEngine also blocks `runJavaScript` in MainWorld; use ApplicationWorld (isolated world) — test early (Space paging, mosaic swap, failed-mark depend on it). Alternative: keep JS enabled but rely on CSP `script-src 'none'` + sanitiser. |
| `loadHTMLString(baseURL:nil)` + navigation policy (`LinkPolicy`) | Only our page; clicks to browser | `setHtml()` (2 MB limit! base64 fonts make pages large → use `setContent` or a custom scheme serving the HTML) + subclass `QWebEnginePage::acceptNavigationRequest` (NavigationTypeLinkClicked → `QDesktopServices::openUrl`) |
| `WKURLSchemeHandler` `ressac-mosaic:` | Mosaic PNGs | `QWebEngineUrlSchemeHandler` + `QWebEngineUrlScheme::registerScheme` before app start (flags: SecureScheme/CorsEnabled as needed); must also be allowed by CSP `img-src` |
| `ressac-toggle:` link interception | Mosaic swap | `acceptNavigationRequest` on custom scheme |
| `callAsyncJavaScript` app world | Space paging, mosaic swap, sub-page line | `runJavaScript(code, worldId, callback)`; arguments must be JSON-escaped manually |
| `XMLDocument(.documentTidyHTML)` sanitiser | HTML → allowlist | Do it in Rust (`ammonia` crate / html5ever) — better: move sanitising into the core or the cxx-qt bridge; keep the depth/attribute/size guards semantics |
| ImageIO thumbnail + CoreGraphics PNG (`MosaicDecoder`, `MosaicRenderer.png`) | Decode/encode | Rust `image` crate (GIF first frame, max-pixel check before decode) or `QImageReader` (`setScaledSize`, `setAllocationLimit`) |
| URLSession ephemeral with byte cap (`MosaicFetcher`) | Download originals | Rust `reqwest` (core already has a guarded client) or `QNetworkAccessManager` + `setMaximumRedirectsAllowed`, abort on size |
| Chart WKWebView `loadFileURL(allowingReadAccessTo:)` + `messageHandlers.ressac.postMessage` | Chart page + bridge | QtWebEngine `load(qrc:/… or file:)` + **QWebChannel** (`qwebchannel.js` must be added to the page and to CSP), or keep the page's `bridge(win)` adapter: inject a shim defining `window.webkit.messageHandlers.ressac.postMessage` that forwards to the QWebChannel object, so `chart-draw.js` is unchanged. CSP allows only `'self' file:` scripts — inject via `QWebEngineScript` (allowed regardless of CSP in an isolated world? verify) |
| `Font.custom` + `ATSApplicationFontsPath` | Bundled fonts | `QFontDatabase::addApplicationFont`; in QML `FontLoader` |
| Pixel-exact `Text` cells, `TextRenderer` bleed | Antiope grid | QML `Text` with `renderType: Text.NativeRendering`/`QtRendering`, `font.hintingPreference`, antialiasing off (`font.preferNoShaping`, `layer`); or draw the grid in one `QQuickPaintedItem` per column (likely faster than thousands of Text items — note existing scroll hitches); bleed via `MultiEffect`/`ShaderEffect` |
| CRT `Canvas` overlay | Scanlines, vignette, corners | QML `Canvas` or `ShaderEffect` overlay with `enabled:false`/no mouse area (input passes through). A real GLSL shader is possible on Linux since QtWebEngine and QML share the scene graph, but keep "no barrel distortion" so clicks map 1:1. Web bleed stays CSS `text-shadow` |
| `NSEvent.addLocalMonitorForEvents` key router | Window-level keys before focused view | Install an event filter on the `QQuickWindow` (`QObject::installEventFilter`, `ShortcutOverride`/`KeyPress`) or QML `Keys.onPressed` on a root `FocusScope` with `Keys.priority: Keys.BeforeItem`; must beat QtWebEngine's key handling (QWebEngineView has its own focus proxy — filter on the window) |
| Responder chain focus classification | Which column has focus | `QQuickWindow::activeFocusItem` ancestry |
| `NSWorkspace.open` | Browser, logs folder | `QDesktopServices::openUrl` (KIO/xdg-open) |
| `ShareLink` | Share menu | KDE Purpose (`Purpose.AlternativesView`/`PurposeMenu`) or just "Copy Link" |
| `NSPasteboard` read for Add Feed prefill | — | `QGuiApplication::clipboard()->text()` (Wayland: clipboard only readable when focused — fine) |
| `NSOpenPanel` / `NSSavePanel` / `fileImporter` | Pick OPML / DB / export target | QML `FileDialog` (QtQuick.Dialogs) → XDG portal |
| `.dropDestination(URL)` / `.draggable(String)` / `.onDrag(NSURL)` | DnD | QML `DropArea` (`drop.urls`, `drop.text`), `Drag.mimeData` with `text/uri-list` + `text/plain`; use a private MIME type (`application/x-ressac-feed`) instead of `ressac-feed:` text to fix the paste-into-field gap |
| `NSAlert` critical (missing helper) | Fatal | `QMessageBox::critical` / Kirigami `PromptDialog` before main window |
| `NSCalendarDayChanged` | Midnight rollover | `QTimer` to next local midnight (+ re-arm on resume / tz change: `QTimeZone`, logind `PrepareForSleep` D-Bus signal) |
| `NSApplication.didBecomeActive` | Edition check + refetch | `QGuiApplication::applicationStateChanged(Qt::ApplicationActive)` |
| `Task.sleep` continuous clock (fires after wake) | 04:00 rollover | `QTimer` is monotonic and may not count suspend; also listen to logind `PrepareForSleep(false)` and re-check, or poll every minute |
| Window occlusion (`didChangeOcclusionState`) | Keep polling while chart visible | `QWindow::visibility()`/`exposed` (`isExposed()`); Wayland has no reliable occlusion → poll while chart window is open and not minimised |
| `NSWindow` key/main tracking | Letters only in reading window | `QGuiApplication::focusWindowChanged` |
| Hidden title bar / full-size black content for Antiope | Chrome | `Qt.FramelessWindowHint` not advisable on KDE; use Kirigami `ApplicationWindow` with `pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.None` and black background; KWin decorations stay |
| `os.Logger` | Logging | `qCInfo` categories / journald; core already logs to file — move to `$XDG_STATE_HOME/ressac/logs` |
| UserDefaults/`@AppStorage` | Settings | `KConfig` (`KConfigXT` .kcfg → generated settings class exposed to QML) in `~/.config/ressacrc`; or reuse/extend the core's `terminal.json` |
| `Bundle/Contents/Helpers/ressac` | Bundled CLI | Install `ressac` to `/usr/libexec/ressac/` or `/usr/bin`; with cxx-qt the CLI could become an in-process library call, **but** the contract today is the subprocess + NDJSON (exit codes 0/1/2/130 semantics, SIGINT cancel) — keep the subprocess at first for parity |
| `~/Library/Application Support/Ressac/ressac.sqlite`, `~/Library/Logs/Ressac` | Paths | **Hard-coded in the core** (`crates/ressac-core/src/store/mod.rs:88-93`, `crates/ressac/src/main.rs:133`, `crates/ressac/src/tui/reader/settings.rs:46`) → must become XDG (`$XDG_DATA_HOME/ressac/ressac.sqlite`, `$XDG_STATE_HOME/ressac/`) via `dirs`/`directories` in the core, or the Linux app always passes `--db` |
| Reduce Motion (`accessibilityDisplayShouldReduceMotion`, `\.accessibilityReduceMotion`) | Stop counter, chart animation | KDE `AnimationDurationFactor` (kdeglobals; `Kirigami.Units.longDuration == 0`/`Kirigami.Settings`), `prefers-reduced-motion` in QtWebEngine follows… verify (may need injecting) |
| Increase Contrast (`colorSchemeContrast`) | Disable bleed | No standard on KDE; an app setting or high-contrast colour scheme detection |
| Light/dark (`colorScheme`) | `data-scheme` | `Kirigami.Theme` / `QStyleHints::colorScheme()` (Qt 6.5+) |
| SF Symbols, `ContentUnavailableView`, `.badge`, segmented pickers | Visual | Breeze icons (`newspaper`→`news-subscribe`/`view-calendar-day`…), `Kirigami.PlaceholderMessage`, `Kirigami.NavigationTabBar`/`Controls.ToolButton` groups, list delegates with count badges (`Kirigami.Chip`/label) |
| `NavigationSplitView` 3 columns | Layout | `Kirigami.ColumnView`/`PageRow` with 3 pages, or `SplitView` (QtQuick.Controls) for desktop parity |
| `.searchable` toolbar field | Search | `Kirigami.SearchField` in the header |
| Dock badge, notifications, Services | **Not used** by the app (out of scope in spec §3) | nothing to port |
| SQLite via GRDB `ValueObservation` + `notifyChanges` | Live queries | In Rust (cxx-qt): rusqlite with a change-notification bus — emit a Qt signal after every own write, after every subprocess run/progress tick (≤1/s), and on `data_version` poll; re-run the visible queries and diff into `QAbstractListModel`s. There is no GRDB-like observation in Rust: design this layer deliberately |

---

## 10. Tests to mirror

### 10.1 Swift (`apps/macos/RessacTests`, Swift Testing, 492 `@Test`, hosted in the app)
Fixture: `tests/fixtures/contract.sqlite` (core-generated contract DB at current schema, copied per test
by a `TestDatabase` helper) and `tests/fixtures/vienna-like.opml`; CLI behaviour stubbed by tiny
`#!/bin/sh` scripts written to a temp dir (record args to a `calls` file, print canned NDJSON, exit codes,
signals) (`EngineTests.swift:63-66`, `ChartModelTests.swift:185-203`). Never the real DB/network.

| File | # | Covers |
|---|---|---|
| ReaderTests | 52 | ReaderStyle defaults/clamps/load, font CSS, sanitiser (active content stripped, entities, relative URLs, unwrap, escaping, deep nesting, misnested end tags, libxml depth estimate, attribute bomb, 512 KB, image sizes, memo, big-stack thread, garbage), LinkPolicy, page content selection, full-article notice, empty/untitled, metadata escaping, scheme/style, page identity ignores read/star, tooltip, Antiope size steps, 40 columns, sub-page label |
| KeyboardTests | 39 | key decoding, focus classification, every native/antiope mapping, held keys, garden keys pass through, journal focus, page entry jumps, NOT FOUND timing, SUITE/RETOUR on special pages, index arrows, column moves, prompt focus |
| DataTests | 34 | schema check (missing/v1/v2/v3/newer, older app vs v6), RESSAC_DB, WAL, timestamp format, v_entry decode, list truncation, sidebar snapshot + pages, filters, paging floor, FTS search, garden JSON, fulltext observation, external writes after notify, writes round-trip, scoping, concurrent writer, busy error, cadence rows, edition read/finished/due, 04:00 across DST |
| AppStateTests | 33 | sidebar tree, list/filters, next/prev + mark read, 1 s auto-read, optimistic writes + revert banner, mark-all, refresh targets, search, letters while typing, Space paging, inline full-text failure, garden runs, reload after runs, launch/migrate/too-new/missing helper/second launch, 500-row paging edge cases |
| AntiopeFrameTests | 29 | cell spans widths, magenta replacements, index/entry rows (22/28 cells), column titles & filter toggle, Minitel keys/hints, header label precedence, rolling counter, tube-corner margin, window dressing, formatter cache, blue never text |
| MosaicTests | 27 | renderer canonical cells, quantisation, transparency, limits, PNG geometry (3/4/3 rows), request URL round trip, refused schemes, decoder (GIF, HTML/SVG unreadable, pixel bomb), fetcher (HTTP error, 10 MB cut, redirects, timeout), scheme handler (PNG answer, cache, transient not cached, failure placeholder, stopped task), sanitiser mosaic boxes, toggle |
| AntiopeGardenTests | 27 | garden page rows/widths/colours/progress/chips/unread, empty pages, arrows, queue retention, inspector page, rhythm rows, banner row, Minitel bar statuses, command parity with Native |
| GardenTests | 21 | sections, retention/dimming, chord filtering, unknown action, progress, safe repairs, selection after update, keys, labels, JSON shapes, cemetery unread, status colours, rhythm line/ages/labels/grouping, cadence menu |
| ChartModelTests | 20 | version check, failures, debounce, change during run, keep last chart, follow/resurvey/cancel, capture prefix & big output & stderr, changeTick sources, visibility reader |
| JournalModelTests | 19 | tz change, due/not due around 04:00, progress, offline/retry, built elsewhere, banners, 15-min backoff, clock backwards, refused by long run, rollover across DST |
| AddFeedTests | 19 | request args, folder pop-up, `added` decode, validation, clipboard prefill, refusal vs failure display, success note, log redaction |
| JournalListTests | 17 | rank order, buried feed drops out, mark-all excludes overflow, Space in edition, rhythm lists/counts, refresh scope, sections/overflow placement, header, placeholders, toolbar progress text/compact/inset |
| FirstRunTests | 16 | bundled font licences, DB location order, dry-run JSON decode/args/run, summary text, confirm→import→fetch, blocked during refresh, failures, export, DB chooser validation, reset |
| AntiopeJournalTests | 14 | P104 rows/colours/widths/end line/states, overflow placement, SUITE to end page, RÉPÉTITION retry, sub-page by rank |
| AntiopePagesTests | 14 | SOMMAIRE columns/wave, GUIDE completeness, glyph coverage, search prompt, empty messages, legacy scrollers |
| ChartBridgeTests | 13 | message validation, URL schemes, prefs persistence + Reduce Motion, retired inks, navigation lock, openFeed/openURL, real WebKit page drawing `apps/macos/ChartTests/fixture.json` (flaky locally, TODO.md:128) |
| EngineTests | 13 | every event decodes, unknown/garbage, line splitter, request factories, prefix order, live streaming, signals, stderr last line, exit codes, missing exe, cancel→SIGINT |
| ReadingFixRound1Tests | 13 | Space double-press, auto-repeat, held toggles, pageDown waits for commit, failed extraction state, window registration, WAL without -shm, reading connections refuse writes |
| TeletextTests | 13 | Bedstead licence/metrics/charset, colours, snapping/steps, CellText fitting |
| RunCoordinatorTests | 12 | long-run exclusivity, progress, FIFO shorts, overlap, banners & refusals, edition reporting, extraction out of banner, network warning persistence, observations told |
| PageDirectoryTests | 11 | reserved pages & counts, order/depth, destinations, collisions, PageEntry state machine |
| MoveFeedTests | 11 | move args, drop decisions, payload parsing, selection behaviour |
| CRTTests | 9 | activation, contrast, scanline bands at 15/20/25/30, last band, corner radius, constants, no hit testing, redraw only on size, bleed padding |
| FixWaveTests | 7 | data_version watcher, poll refresh, background polling only with chart, notice never on another entry, Today at midnight, key window letters, timed-out sanitise not memoised |
| AppearanceTests | 4 | keys/values, fallback without Bedstead, live switch keeps state, picker |
| ThemePickerTests | 3 | theme request args, themes query, feed theme keys |
| SmokeTests | 2 | helper embedded and runs, GRDB linked |

### 10.2 JavaScript (`apps/macos/ChartTests`, `node --test`)
Fixture `fixture.json` generated by `make-fixture.cjs` (seeded mulberry32 20261019; 15 invented themes,
~100 synthetic feeds with fixed `age_days`). `preview.html` renders it in a browser
(`?view=themes&ink=nuit&animate=0&empty=1`).
- `chart-core.test.cjs` (17): deterministic placement, island radius by rhythm, no overlap (incl.
  crowded seas), clear of compass/cartouche, adding a feed moves others ≤40 units in "By theme", labels
  never overlap, busiest named first, label placement data, territory groups ≥2, unthemed → UNCHARTED
  WATERS, only bleu/nuit inks, heat endpoints, rings, empty chart harbour, height field, works with global d3.
- `chart-draw.test.cjs` (11): feed text never HTML, empty message, prefs round trip, survey time format,
  popover structure, only web links get href, tooltip counts and stays in window, load accepts string or
  object, bridge posts only when present, every placed label drawn.
These run unchanged under Node on Linux and the page itself should run unchanged in QtWebEngine.

---

## 11. Known gaps and quirks to decide on (from TODO.md / notes.md / code)
- Antiope inspector has no Theme picker; Antiope has no add/move controls; overflow rows click-only.
- Dropping a feed on a feed row inside a folder does nothing; dragging a feed into a text field pastes
  `ressac-feed:<id>`.
- Today count/list use `Calendar.current` (tz change needs relaunch).
- Journal list grabs keyboard each time it appears.
- Add Feed: cancel during add still completes and selects; clipboard re-read each time the sheet is built.
- Chart: `showReadingWindow` takes any reading window; launch block duplicated between ChartWindow and
  RessacApp; main WindowGroup id may forget its frame once.
- Antiope big-folder scrolling hitches (no CRT 38 ms/s); possible "reentrant operation in NSTableView
  delegate" warning (AppKit-specific).
- Garden Status|Rhythm picker can take the keyboard focus from k/r/b (TODO.md:68).
- `import --dry-run` runs outside the coordinator (could overlap a long run; read-only).
