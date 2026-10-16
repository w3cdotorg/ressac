# Ressac — Sub-project 2b: reading in the terminal

- **Date:** 2026-10-16
- **Status:** Draft, awaiting review
- **Scope:** a reading interface in the `ressac` binary (ratatui), with the Journal, the rhythm groups, the garden as a destination, real images, and both looks (Native and Antiope). A reading API in the core so that the terminal never reimplements the app's rules. No schema change (v4).
- **Builds on:** `docs/specs/2026-10-10-core-and-garden-design.md` (*core spec*), `docs/specs/2026-10-10-mac-app-design.md` (*app spec*), `docs/specs/2026-10-14-antiope-design.md` (*Antiope spec*), `docs/specs/2026-10-16-cadence-and-edition-design.md` (*edition spec*), all merged.
- **Reference:** clappuccino (`wooclap/clappuccino`, local clone `~/Sites/LearningShow16-TUI`, `crates/wooclap-cli`): ratatui 0.30, `ratatui-image` 11.1.0, `image` 0.25.10, and its notes on the image protocol picker (§7).

## 1. Goal

A quick keyboard reader for a tmux pane next to the day dashboard and Claude Code, most often for the morning Journal. It reads the same database as Ressac.app, follows the same rules, and can stay open while the app runs.

Success looks like: Clément opens a tmux pane in Ghostty and types `ressac`. The Journal of the day is there (built on the spot if the app has not built it yet). He reads it with `j`, `k` and Space, sees the photos of an article inline, opens a link with `o 3`, narrows the pane to 70 columns and keeps reading in one column, presses `t` to switch to Antiope, types `104` and walks the edition as sub-pages to FIN DE L'EDITION, then quits with `q`. Ressac.app, open meanwhile, shows the same entries as read.

## 2. Decisions taken with the user (2026-10-16)

- **Usage (A):** a tmux pane for quick keyboard reading, the Journal first.
- **Images (C):** real images with `ratatui-image` when the terminal supports a graphics protocol (Ghostty: Kitty protocol through tmux `allow-passthrough on`, checked on this Mac: tmux 3.8, passthrough on), half-blocks otherwise, `[image: alt text]` when an image cannot load or images are off.
- **Looks (A):** both Native and Antiope, switched with a key, the choice remembered.
- **Layout (A):** adaptive, three columns when wide, two in the middle, one when narrow.
- **Journal and refresh (A):** like the app: the edition is built on launch when due, a key refreshes, a key retries after an offline build.
- **Entry point (A):** `ressac` with no subcommand opens the reader (`ressac read` is an alias); the garden becomes a destination of the sources column, with today's garden keys; `ressac garden` still opens the garden directly.
- **Architecture (approach 1):** a `tui/reader/` module in the `ressac` binary with a pure state machine separate from rendering (like `GardenState`), data read through a new core reading API on the contract views, writes through core functions, the existing garden reused as a screen. Rejected: a separate `ressac-tui` crate (it would move working garden code for little gain; possible later if the image dependencies weigh on compile times) and clappuccino's single large `ui/mod.rs` (2 677 lines).
- **Antiope images:** half-blocks quantised to the eight teletext colours; `i` shows the real image.
- **Language:** every user-facing string in English, except the Minitel key labels and `FIN DE L'EDITION` in Antiope (as in the app).

## 3. Scope

### In scope

**Core (Rust):**

| Item | Purpose |
|---|---|
| Reading API (`store/library.rs`, `Ressac` methods) | Sources, entry lists, search, entry content, read/star/mark-all, all through the contract views (§4). |
| `Ressac::fetch_image` | Guarded image download for the terminal (§7). |
| `search_ids` public | Full-text search for the terminal. |

**Terminal (`crates/ressac/src/tui/`):**

- `reader/state.rs`: the pure reader state, keys → commands.
- `reader/layout.rs`: the adaptive columns.
- `reader/html.rs`: HTML → styled terminal text.
- `reader/images.rs`: protocol picker, loading, cache, teletext quantisation.
- `reader/native.rs` and `reader/antiope.rs`: rendering for each look.
- `reader/mod.rs`: the event loop (keys, ticks, background jobs).
- The garden (`state.rs`, `ui.rs`) embedded as a screen of the reader.
- CLI: `ressac` without subcommand and `ressac read` open the reader.

### Out of scope

Background refresh beyond the edition rule; notifications; mouse support beyond what ratatui gives for free; separated mosaics or a true Antiope glyph set; a CRT effect; double-height text (ratatui has no DECDHL); syncing the look with the app's own Appearance setting; Windows or Linux terminals beyond what works by default; a new schema version.

## 4. Core: the reading API

A new module `crates/ressac-core/src/store/library.rs` and thin `Ressac` methods. Every rule matches what the app does, so that the terminal and the app show the same lists. Reads go through `v_entry`, `v_cadence`, `v_edition_today`, `v_edition_overflow`, `v_page`, `v_garden_row`.

### Sources

`Ressac::sources(now) -> SourcesSnapshot`:

- smart lists with unread counts: Journal (unread entries of today's edition, active feeds only, counted over `v_entry`), Unread, Today (since local midnight), Starred;
- the folder tree in display order with each folder's and feed's page and unread count (same tree as the app's sidebar);
- the five rhythm groups with their unread counts (Silent listed at zero);
- the garden: Needs triage (with its count), All feeds, Cemetery.

### Entry lists

`Ressac::entries(source, filter, page) -> EntryPage` with `Source = Journal | Unread | Today | Starred | Folder(id) | Feed(id) | Cadence(group)` and `filter = All | Unread`:

- newest first by `published_at` then `entry_id`, pages of 500 (as the app);
- the Journal: ordered by rank, always loaded whole, restricted to active feeds, with its header (`EditionRecord`), sections and overflow rows (`v_edition_overflow`, active feeds only);
- `Ressac::search(source, text, page)`: the existing FTS (`search_ids`, made public) restricted to the source.

### Entry content

`Ressac::entry(id) -> EntryDetail`: title, feed, author, dates, link, read/star state, and the HTML to show: the stored full text when it exists and the feed prefers it or the user asked for it, else `content_html`, else `summary_html`. Full text on demand: the existing `Ressac::extract_entry`.

### Writes

The same client-writable columns as the app (`entry.read_at`, `entry.starred_at`), through new core functions: `set_read(id, bool)`, `set_starred(id, bool)`, `mark_all_read(source)` (the Journal marks its entries only, never the overflow). The rhythm override goes through `Ressac::set_cadence`; garden decisions through the existing garden functions. No new column; schema stays v4.

### Living next to the app

The database is in WAL mode. The terminal re-reads after each of its own writes, and when idle checks `PRAGMA data_version` every 2 seconds; a change reloads the sources and the current list, keeping the selection. Busy database: wait up to 5 s (`busy_timeout`), then a banner (§9).

## 5. The reader state and keys

`ReaderState` is pure: it takes a `Key` or an `Event` (a loaded list, a finished job, a tick, a resize) and returns `Command`s (load, write, extract, fetch image, build edition, open URL, quit). The event loop executes them. This is the pattern of `GardenState`.

### Native keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | next / previous entry (in the sources column, `↓` / `↑` move between sources) |
| `Space` | scroll the article, then the next unread entry of the source, never leaving it |
| `h` / `l`, `←` / `→`, `Tab` | change column; `Enter` opens, `Esc` goes back (one-column mode) |
| `m`, `s` | read / unread, star |
| `A` | mark all read in the source (Journal: not the overflow) |
| `u` | all / unread only |
| `/` | search in the source; `Esc` clears |
| `f` | full article (extraction), or back to the feed's content |
| `o`, then `1`…`9` | `o` opens the article in the browser; `o` followed by a number opens that link |
| `i` | images on / off (Native), real image / mosaic (Antiope) |
| `r`, `R` | refresh the source, refresh everything; on an offline Journal, `r` retries the edition |
| `c` | rhythm override menu for the selected feed (in the garden, and on a feed source) |
| `t` | switch Native / Antiope |
| `?` | help; `q` quit |

On a garden page, the garden's keys win (`k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `R` apply safe repairs), navigation with `↑` / `↓` as today; the bottom line says which key set is active.

### Antiope keys

Digits type a page number, `⌫` corrects, `Esc` cancels. Minitel keys: SUITE (`Space` or `j`), RETOUR (`k`), SOMMAIRE (`S`), GUIDE (`?`), RÉPÉTITION (`r`). Links open with `o` then a number.

### The look setting

Stored in `~/Library/Application Support/Ressac/terminal.json` (`{"look":"native"|"antiope","images":true}`), Native on first launch. A missing or unreadable file means the defaults; writing it never blocks reading.

## 6. Layout and the Journal

### Columns

| Pane width | Columns |
|---|---|
| ≥ 120 | sources (26), entries (40), reader (the rest) |
| 80–119 | entries (40), reader; the sources open over them with `h` / `←` from the list |
| < 80 | one column: sources → entries (`Enter`) → reader (`Enter`), `Esc` goes up |

Re-laid out on every resize, keeping the selection. The focused column has a highlighted border. Top line: source name, unread count, filter; during a job, the progress (`Building this morning's edition… 34/156`, shortened to `Edition… 34/156` when it does not fit). Bottom line: the keys that apply. Below 40 × 10: `Window too small`.

The reader opens on the **Journal** (the app opens on Unread; the terminal's use is the morning read).

### The Journal

- Header `Friday 16 October · 43 entries · 12 read`, with ` · 3 feeds unreachable` / ` · 1 feed unreachable`.
- Sections `Front Page`, `This Week`, `The Wire`; empty sections omitted; overflow rows `+12 more in Actualitté` under their feed, `Enter` on one opens that feed.
- `That's the edition.` once every entry is read; `Nothing new this morning.` for an empty edition; `No edition yet` before the first one.

### Building the edition

The same rule as the app, through `Ressac::build_edition` (forced refresh, 04:00 local due rule): at launch when due, and at the 04:00 rollover if the reader stays open. The build runs as a background job; the other sources stay readable; the progress shows on the top line. Offline: `Offline: no edition yet`, `r` retries. A refusal because another process built it: reload the Journal, no banner. A build already running in the app: the core's due re-check refuses the second one; same handling.

### Rhythm

The five groups are sources, as in the app. A feed's rhythm line (`Rhythm: Daily · median 0.4 d · last entry 2 h ago`, ` (set by you)`) shows in the garden page and on a feed source's header; `c` opens the override menu `Auto / Daily / Weekly / Monthly / Rare / Silent`.

## 7. Rendering articles and images

### Text

`reader/html.rs` converts the entry's HTML into styled ratatui `Text`, with `scraper`:

- headings bold and underlined; bold and italic kept; lists with bullets and numbers; blockquotes with a `│` gutter; `pre` and `code` keep their spacing; tables as rows of cells separated by `│`;
- every link numbered after its text (`text[3]`), and the list of links at the end of the article;
- wrapped to the column width, at most 72 characters for readability;
- every string from a feed goes through the control-character neutralisation (`output::printable`, already in the `ressac` binary, reused as is) before it reaches the terminal (terminal-escape injection);
- the article header: title, feed, author, date, state (read, starred, full text).

### Images

- **Picker:** `ratatui_image::picker::Picker::from_query_stdio()`, created **after** `ratatui::init()`. Clappuccino's notes (`app.rs` near the picker, `preview.rs` `PreviewState::new`): the query thread snapshots the termios and restores it when its stdin read unblocks; created before init it restores line mode and freezes the keyboard under tmux; it can also swallow the first keystroke when the terminal does not answer, which clappuccino defuses by emitting an unwrapped DA1 so the thread ends on its own. Warp advertises Kitty but renders only iTerm2: force iTerm2 there. Ghostty under tmux with passthrough: Kitty.
- **Fallback:** no protocol detected → half-blocks; load failure or images off → `[image: alt text]`.
- **Fetching:** a new `Ressac::fetch_image(url) -> Result<Vec<u8>>` through the core's guarded client (private addresses refused, http/https only), at most 10 MB and 15 s (the app's mosaic limits); decoding with `image`'s limits (at most 4096 × 4096 decoded).
- **Display:** lazy (only images on screen or near it), loaded in the background with `[loading image…]` meanwhile; at most half the reader's height; an in-memory cache of the 32 most recent images.
- **Antiope:** half-blocks with every pixel mapped to the nearest of the eight teletext colours; `i` swaps in the real image.
- **Dependencies:** `ratatui-image = { version = "11.1.0", default-features = false, features = ["crossterm", "image-defaults"] }` and `image = "0.25.10"`, the versions clappuccino runs with ratatui 0.30.2.

## 8. Antiope in the terminal

- Palette: the eight teletext colours in true colour on black, blue never colouring text; the whole interface in capitals with accents kept.
- Page header `P412 RESSAC … FRI 16 OCT 18:33:12`, the clock ticking every second; during a refresh the page number searches like a TV set.
- Columns of the app's widths in cells (index 22, entries 28, reader the rest), `▐` blue separators; they fold with the width like Native.
- Minitel bar in reverse video (SOMMAIRE, SUITE, RETOUR, GUIDE, RÉPÉTITION). An unknown page shows `P437 NOT FOUND` in red for 2 s.
- Pages: P100 SOMMAIRE, P899 GUIDE, P104 JOURNAL (coloured sections FRONT PAGE yellow / THIS WEEK cyan / THE WIRE green, `+12 MORE IN ACTUALITTÉ` white on blue, sub-pages `104 3/43`, `FIN DE L'EDITION`), P105–P109 rhythm groups, P800–P802 garden, feeds and folders by their page numbers.
- Double height does not exist in ratatui: titles and `FIN DE L'EDITION` use a full-width reverse-video yellow line instead.

## 9. Error handling

| Case | Behaviour |
|---|---|
| No database or no feeds | The existing first-run hint (`ressac import …`), `q` quits. |
| Database newer than the binary | The existing refusal message, exit 1, terminal restored. |
| Database busy (the app writing) | Wait 5 s, then a banner `The database is busy (is Ressac.app writing?)`, `r` retries. |
| Extraction fails | The stored sentence as a line in the reader, as the app does. |
| Image fails | `[image: alt text]` in place; no modal. |
| Edition offline / built elsewhere / failed | As §6; a failed build shows its sentence on the top line, `r` retries. |
| Window too small | `Window too small` until it grows. |
| Panic or Ctrl-C | The terminal is always restored (a guard, as the garden does today). |

## 10. Testing

### Rust, core

- Reading API on the contract fixture: sources and counts, Today's local midnight, lists and paging, Journal (rank order, active feeds only, header, overflow), search per source, read/star, mark-all (Journal excludes overflow).
- `fetch_image` with wiremock: private address refused, non-http refused, size cap, timeout, a huge image refused at decode.

### Rust, terminal

- `ReaderState`: keys → commands in each column and mode, Space inside the source, garden key set, Antiope page entry, look switch keeping the selection.
- Layout: the three width classes and the transitions on resize.
- `html.rs` with `insta` snapshots: headings, lists, quotes, code, tables, numbered links, control characters neutralised.
- Rendering with ratatui's `TestBackend` at 140, 100 and 70 columns, Native and Antiope, `insta` snapshots.
- Image quantisation to the eight teletext colours (pure function).

### Manual check (with Clément, in Ghostty under tmux)

1. `ressac` opens on the Journal; built on launch if due.
2. Read with `j`, `k`, Space to `That's the edition.`; `o 3` opens a link.
3. An article with photos: real images; `i` toggles them.
4. Narrow the pane to 70 columns: one column; widen it back.
5. `t` to Antiope: `104`, sub-pages, FIN DE L'EDITION, `105`, `800`.
6. Ressac.app open at the same time: reading in one shows in the other within a few seconds.
7. Garden: triage a feed with `k` and undo with `u`.

## 11. Phases

1. Core reading API.
2. `fetch_image` and the image dependencies.
3. The HTML converter.
4. The reader state and keys.
5. Layout and the Native rendering.
6. Images.
7. The Journal and the edition build.
8. The garden as a reader screen, `ressac` without subcommand.
9. Antiope.
10. Acceptance and docs.

## 12. Risks and open points

- **Image protocol detection under tmux** is fragile (clappuccino's notes); the half-block fallback keeps the reader usable whatever happens.
- **Compile time** grows with `image`; if it becomes a nuisance, the reader can move to its own crate later.
- **The app's Appearance and the terminal's look** are separate settings by design.
