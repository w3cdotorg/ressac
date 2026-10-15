# Ressac — Sub-project 4: the Antiope theme (macOS app)

- **Date:** 2026-10-14
- **Status:** Draft, awaiting review
- **Scope:** the Antiope teletext theme for the Mac app, plus the page numbers it needs in the core. The terminal UI gets its Antiope look with sub-project 2b, reusing the palette and the page numbers defined here.
- **Builds on:** `docs/specs/2026-10-10-core-and-garden-design.md` (*the core spec*) and `docs/specs/2026-10-10-mac-app-design.md` (*the app spec*), both merged.
- **Order agreed with the user:** 4 (this spec), then 3 (cadence and morning edition), then 2b (terminal reading).

## 1. Goal

A second, complete look for Ressac.app that turns it into a French teletext set of the 1980s (Antiope): pixel font everywhere, the eight teletext colours on black, a page header with a running clock, three-digit page numbers you type to jump, articles as sub-pages, Minitel keys at the bottom, and images shown as two-colour block mosaics.

Success looks like: Clément switches Settings → Appearance to Antiope, the window turns black without a relaunch, he types `412` to land on Courrier international, reads it with SUITE / RETOUR (Space, `j`, `k`), clicks a mosaic to see the original photo, triages a feed in P800 with the usual keys, and switches back to Native with nothing lost.

## 2. Decisions taken with the user (2026-10-14)

- **Placement:** a theme you choose (`Native` / `Antiope`); Antiope dresses the whole reading window, the native look stays available.
- **Fidelity:** *everything in pixels*. The teletext font is used everywhere in the window, article bodies included, with the eight pure colours and no anti-aliasing.
- **Teletext behaviours, all four kept:** the page header with clock, page numbers typed on the keyboard, the Minitel key bar, mosaics.
- **Key bar style:** Minitel keys (SOMMAIRE, SUITE, RETOUR, GUIDE, RÉPÉTITION) in reverse video.
- **Images:** shown as mosaics; a click (or `i`) swaps in the original.
- **Layout:** option A of the mockups, three teletext columns (index · entries · reader). **The column separators must be perfectly straight:** column widths are counted in character cells and no row may push them.
- **Architecture:** a separate root view sharing the app's state (§5, approach A), and page numbers stored in the core (§4, approach A).
- **Articles are sub-pages** of their feed's page ("412 3/20").
- **Font:** **Bedstead** (Ben Harris, CC0), modelled on the SAA5050 teletext character generator. Checked on 2026-10-14: every French letter (é è ê ë à â î ï ô ù û ü ÿ ç œ æ and their capitals), « » ’ “ ” … – — € °, the 60 sextant mosaics (U+1FB00–1FB3B) and the separated mosaics (U+1CE51 onwards) are present; one advance width (600/1000 em), so one font pixel is 1/10 of the font size. It is not the Antiope glyph set (no free one exists); swapping fonts later changes nothing else.
- **Language:** the app spec's rule stands, every user-facing string is in English. **One exception:** the Minitel key labels stay in French, as they are the device's own key names.
- **CRT screen (added the same day):** an optional checkbox, in the spirit of LeLion-web's `Shaders/Crt.gdshader` (curvature 0.045, scanlines 0.28, chromatic offset 1.2, vignette 0.35). No real barrel distortion: SwiftUI shaders cannot reach the `WKWebView`, and a curved image over straight hit areas would misplace clicks. Curvature is suggested by the tube's rounded corners instead (§9).
- **Mockups:** `.superpowers/brainstorm/*/content/layout.html` (local, not committed).

## 3. Scope

### In scope

**Core (Rust), schema v3:**

| Item | Purpose |
|---|---|
| Schema v3 migration | `folder.page`, `feed.page`, view `v_page` (§4). |
| Page assignment | At migration, import and whenever a folder or feed is added. |
| `ressac page list` | Every page, reserved ones included; `--json` for NDJSON. |
| `ressac page set feed\|folder <id> <page>` | Renumber by hand. |

**App (Swift):**

- Settings → Appearance (Native / Antiope), applied live.
- `Teletext/` module: palette, font metrics, cell text, mosaic renderer.
- `AntiopeRootView`: page header, three cell-grid columns, Minitel bar, page-number entry, P100 SOMMAIRE and P899 GUIDE pages, search prompt.
- The reader in Antiope (stylesheet, size steps, 40-column width, double-height titles) and image mosaics with a swap to the original.
- The garden in Antiope (P800, P801, P802), banners and refresh progress.
- The optional CRT screen (§9).
- `GardenStatusStyle`, replacing the two copies of the status colours.

### Out of scope

The Antiope look in the terminal UI (2b); separated-mosaic rendering; a true Antiope glyph design; theming sheets, alerts, the Settings window or the launch screens; teletext "reveal"; sound; animated mosaics.

## 4. Core: page numbers (schema v3, `schema/0003_pages.sql`)

### Data

- `folder.page INTEGER` and `feed.page INTEGER`, each with a unique index. Folder and feed pages share one number space: no folder may hold a feed's number and vice versa (enforced by the assignment code and checked by `page set`, since SQLite cannot index across two tables).
- `v_page(page, kind, ref_id, title, parent_page)`: `kind` is `folder` or `feed`; buried feeds stay listed (the Cemetery needs them). This view is the contract for the app and the future TUI.
- Migration v2 → v3: backup first, one transaction, rollback on failure, exactly as v2.

### Reserved pages

Fixed in code, never stored, refused by `page set`:

| Page | Destination |
|---|---|
| 100 | SOMMAIRE (index of every page) |
| 101 | Unread |
| 102 | Today |
| 103 | Starred |
| 800 | Garden: Needs triage |
| 801 | Garden: All feeds |
| 802 | Garden: Cemetery |
| 899 | GUIDE (key help) |

### Assignment

1. Root folders, in display order, take 200, 300, … 700 (six at most).
2. Inside a root folder's hundred, its subfolders and feeds take the next free number, depth-first in display order (Infos 400: first feed 401, …).
3. Feeds without a folder take 110–199.
4. When a hundred is full, or for a seventh root folder onwards, the item takes the lowest free number in 110–799 outside the reserved pages.
5. Numbers are assigned once and never change on their own. A buried feed keeps its number; a number is not reused while its folder or feed exists.

### CLI

- `ressac page list [--json]`: one line per page (reserved first), `page kind id title`. With `--json`, one NDJSON object per page.
- `ressac page set feed|folder <id> <page>`: exit `2` for a reserved page, a number already taken or out of 110–799, or an unknown id; exit `1` for a database error.

### Sub-pages

Not stored. An entry's sub-page is its position in the list currently shown (feed, folder or smart list, with the current filter and sort), written "3/20". The page part is always the entry's own feed page, even in Unread.

## 5. App architecture

### Switch

- `@AppStorage("appearance")`, values `native` (default) and `antiope`.
- `LaunchView` shows `MainView` or `AntiopeRootView` for the `.ready` phase. Switching is live and keeps the selection, the selected entry and the search text, which all live in `AppState`.
- Everything else is shared: `AppState`, queries, `RunCoordinator`, menu commands, writes.
- If Bedstead fails to register, the app logs it and stays in Native. It never draws Antiope in a fallback font.

### `Teletext/` module

- **`TeletextPalette`:** black `#000000`, red `#FF0000`, green `#00FF00`, yellow `#FFFF00`, blue `#0000FF`, magenta `#FF00FF`, cyan `#00FFFF`, white `#FFFFFF`. No other colour appears in an Antiope view. Blue text on black is hard to read, so blue is used for separators, backgrounds and mosaics only, never for text.
- **`TeletextFont`:** Bedstead, bundled in `Resources/Fonts` with its licence note, registered through `ATSApplicationFontsPath` like Newsreader. `cell(forSize:)` returns the cell (0.6 em × 1.0 em).
- **`CellText`:** fits a string into exactly N cells. It truncates by grapheme cluster (no ellipsis, teletext had none), pads with spaces, and replaces any grapheme Bedstead lacks (emoji, CJK…) with a magenta `?`. It also returns the original string for accessibility.
- **`MosaicRenderer`:** see §7.

### Window

- Hidden title bar with full-size content (the black reaches the top, the traffic lights sit over the header), no toolbar.
- From top to bottom: `PageHeader` (one row), an optional banner row, the three columns, `MinitelBar` (one row).
- **Columns:** `PageIndexColumn` 22 cells wide, `EntryPageColumn` 28 cells, the reader takes the rest. The separators are separate one-cell views drawing a blue `▐` on every row: they never come from text, so a stray glyph cannot bend them. Rows are `ScrollView` + `LazyVStack` of fixed-width cell rows (no native `List`, so no AppKit highlight, insets or vibrancy). Selection is reverse video (black on cyan in the index, black on white in the entries).
- **Index column rows:** `101 Unread    1083`; the page number in cyan, root folders in green capitals, unread counts in yellow, the Garden line in magenta with its triage count in red.
- **Entry rows:** a yellow `●` for unread, a white `*` for starred, the title over two rows at most.
- **Empty states:** one cyan line, centred ("NO UNREAD ENTRIES").

### Focus and keys

- `AntiopeFocus` holds the active column (`index`, `entries`, `reader`) or `textInput` while the search prompt is open.
- `ReadingKeyRouter` asks a `KeyFocusProvider` for the active column instead of reading the `NSTableView` responder directly. The native window keeps today's responder-based provider; Antiope uses `AntiopeFocus`. The existing `ReadingCommand` mapping is unchanged.
- Arrows `←` and `→` move between the three columns in Antiope.

### Search

`⌘F` opens a `SEARCH? ____` prompt in the Minitel bar, bound to the same `searchText`; `Return` keeps it, `Esc` clears it and closes the prompt.

### Settings

- A new **Appearance** picker (Native / Antiope) at the top.
- In Antiope, the font picker is disabled with the note "Bedstead in Antiope".
- Text size snaps to 15, 20, 25 or 30 points (20 by default), so that one font pixel always lands on whole Retina pixels. `⌘+` and `⌘−` move one step. Antiope keeps **its own stored size** (`reader.size.antiope`, default 20), separate from the Native size (`reader.size`), so switching looks never changes the other one; `⌘+`, `⌘−` and `⌘0` act on the size of the look shown (decision of 2026-10-14: a shared size would turn today's 17 into 15).
- Line width gains a **40 columns** option (exactly 40 cells), available in both looks.
- A **CRT screen** checkbox (`@AppStorage("antiope.crt")`, off by default) under the Appearance picker, enabled only when Appearance is Antiope (§9).

## 6. Teletext navigation

### Page header

`P412 RESSAC` … `WED 14 OCT 09:47:12`: page number in yellow, date in cyan, time in yellow, ticking every second.

- While digits are typed, the header shows them (`P4__`).
- During a refresh, the page number counts up the way a TV searches for a page; it settles when the run ends. This is decorative and stays off when "Reduce motion" is on.
- An unknown page shows `P437 NOT FOUND` in red for 2 seconds, then the header returns to the current page.

### Page numbers

- A digit typed outside a text field starts the entry. Jumping is automatic on the third digit.
- `⌫` (CORRECTION) erases the last digit, `Esc` (ANNULATION) cancels the entry.
- A jump selects the matching row in the index column, the entry list follows, and the index column takes the focus. Reserved pages open their destination.
- No single digit is bound to anything today, so no existing command is shadowed.

### Minitel bar

Reverse-video keys, clickable:

| Key | Action | Keyboard |
|---|---|---|
| SOMMAIRE | go to P100 | `⌥S` |
| SUITE | next sub-page (today's Space behaviour) | `Space`, `j` |
| RETOUR | previous sub-page | `k` |
| GUIDE | P899, key help in the reader | `⌥G`, `?` |
| RÉPÉTITION | refresh the selection (as `⌘R`) | `⌥R` |

Right of the keys, hints that follow the focus: `m read · s star · i images` while reading, `k keep · r repair · b bury · u undo` in the garden. While a run is going, the hints give way to `REFRESHING 42/155`.

### Sub-pages

The reader's header line reads `412 3/20`. SUITE and RETOUR are today's next/previous entry commands, including Space scrolling the article before moving on. Only the look changes.

### P100 SOMMAIRE and P899 GUIDE

Both are drawn in the reader column.

- **P100:** a double-height `RESSAC` with a mosaic wave logo, then every page in two columns with its unread count, reserved pages in colour. A click or a typed number goes there.
- **P899:** every key of the window, grouped as the GUIDE of a Minitel service.

## 7. The reader in Antiope

### Stylesheet

`ArticleTemplate` adds `data-theme="antiope"` on `:root`, next to `data-scheme`. `reader.css` gains an Antiope block that ignores the light/dark scheme:

- Bedstead everywhere (`--reader-font`, `--ui-font` and `--mono-font`), with `-webkit-font-smoothing: none`. The web content process cannot see fonts registered by the app, so Bedstead is embedded with `@font-face`, the way `ReaderFont.bundledFaces` already does for Newsreader.
- Black paper, white ink.
- Title in yellow, double height: `transform: scaleY(2)` with a matching bottom margin, so that it takes two rows.
- Meta line in cyan.
- `h2` in green, `h3` in cyan.
- Links in cyan, without underline.
- Blockquotes with a magenta mosaic bar on the left.
- Code in green.
- `mark` in yellow reverse video.
- The full-article notice in red reverse video.
- Line height: exactly 1.2 em in the body (12 font pixels), 1.0 em for mosaics.

### Mosaics

**Loading.**
- In Antiope, the sanitiser rewrites each `<img src>` to `ressac-mosaic:` + the percent-encoded original URL, and wraps it in a link to `ressac-toggle:<n>`.
- A `WKURLSchemeHandler` fetches the original with `URLSession`: http and https only, 10 MB at most, 15-second timeout.
- It decodes the image with ImageIO as a downsampled thumbnail; a GIF uses its first frame.

**Rendering, `MosaicRenderer` (pure function).**
- It cuts the image into cells of 2 × 3 sub-pixels, at most 40 cells wide and never wider than the reader column.
- For each cell it picks the best foreground/background pair among the eight colours, by least squared error in RGB, then sets each sub-pixel to one of the two, as the teletext constraint demands.
- The output is a PNG with one device pixel per font pixel × scale, shown with `image-rendering: pixelated`.

**Swapping to the original.**
- A click on a mosaic (the `ressac-toggle:` link, caught by the navigation delegate) swaps it for the original.
- `i` swaps every image of the article both ways.
- The swap runs app JavaScript through `callAsyncJavaScript`; page JavaScript stays off.

**Cache and failures.**
- Mosaics live in an `NSCache`, keyed by URL and width.
- A failure (unreachable, too large, unreadable format such as SVG) gives a magenta `IMAGE` block with the alt text; a click still loads the original.

## 8. The garden and the other surfaces in Antiope

### Garden pages

**P800, P801 and P802.** The entry column lists the feeds by status, under coloured headings:

| Status | Colour |
|---|---|
| Dead | red |
| Hijacked | magenta |
| Moved | cyan |
| Ailing | yellow |
| Duplicate | white |
| Fossil | blue (heading as reverse video, white on blue) |
| Dormant | green |
| Alive | green |

A mosaic progress row (`▐████░░ 12/80`) replaces the gauge.

**Inspector.** It is drawn as a teletext page in the reader column: name and folder path, the status in double height, then the evidence, remarks, links, redirects and fetch log in coloured sections. Every garden key works as today (`k`, `r`, `b`, `u`, `o`, `w`, `⇧R`).

### `GardenStatusStyle`

One type gives each status its native colour and its teletext colour. The two copies disagree today (Hijacked, Duplicate); the sidebar's values win (Hijacked purple, Duplicate teal), so the inspector changes for those two (decision of 2026-10-14). It replaces `SidebarRow.color(for:)` in `MainView.swift` and `statusColor(_:)` in `GardenInspector.swift`.

### Banners

`BannerView`'s messages show as a red reverse-video row under the header.

### What stays native

Launch screens (migration, too-new database, failure), the OPML import sheet, system alerts, the Settings window and the menus.

### Accessibility

- VoiceOver reads the original strings, never the fitted cells or the `?` replacements.
- The Minitel keys carry readable labels ("Sommaire, go to page 100").
- The page counter respects Reduce motion.

## 9. The CRT screen

Active when Appearance is Antiope **and** the CRT screen box is checked. In Native the box is disabled and nothing is drawn.

### Overlay

- `CRTOverlay`: one view over the whole Antiope window (header, columns, reader, Minitel bar), with `allowsHitTesting(false)` so every click and scroll goes through. It sits above the `WKWebView`, so the reader gets the same treatment as the SwiftUI columns.
- **Scanlines:** one dark band per font-pixel row (a font pixel is 1/10 of the text size, §5), covering the lower 40 % of that row in black at opacity 0.28, LeLion's value. The bands are aligned on the cell grid of the columns, so they stay crisp and regular. In the reader they stay fixed to the window while the article scrolls under them, as on a real tube.
- **Vignette:** a radial darkening, black at opacity 0.35 at the corners, clear at the centre.
- **Tube corners:** black rounded corners, with a radius of 3 % of the window's shorter side, suggesting the curved glass without moving any content.
- **Cost:** drawn with `Canvas` once per size and text-size change; nothing animates.

### Colour bleed

The chromatic offset needs the underlying pixels, which an overlay cannot read, so each surface draws its own:

- **SwiftUI cells:** a red copy of the text at +1 pt and a blue copy at −1 pt, both at opacity 0.45, plus a 2 pt glow of the text's own colour. Applied by one `crtBleed()` modifier on `CellText` views, a no-op when the CRT screen is off.
- **Reader:** `ArticleTemplate` adds `data-crt` on `:root`, and `reader.css` gives text `text-shadow: 1px 0 rgba(255,0,0,.45), -1px 0 rgba(0,0,255,.45), 0 0 2px currentColor`. Mosaics and original images get no bleed; the scanlines and vignette already cover them.

### What it does not do

No barrel distortion (see §2), no flicker, no noise, no phosphor persistence. The effect is static, so Reduce motion has nothing to switch off. Increase contrast in the accessibility settings turns the bleed off and keeps the scanlines and vignette.

## 10. Error handling

| Case | Behaviour |
|---|---|
| v3 migration fails | Rollback, backup kept, the launch screen's failure message as for v2. |
| Older `ressac` on a v3 database | Refuses it with today's "database created by a newer Ressac"; the app's bundled copy is v3. |
| `page set` invalid | Exit `2` with the reason; nothing written. |
| Unknown page typed | `NOT FOUND` in the header for 2 s; selection unchanged. |
| Image fetch or decode fails | `IMAGE` block, original still reachable. |
| Bedstead missing | Log, stay in Native, Appearance picker disabled with a note. |

## 11. Testing

### Rust

- Page assignment on the contract database (fixture `tests/fixtures/contract.sqlite`, regenerated at v3).
- Stability after adding and after burying a feed.
- Overflow of a hundred, and a seventh root folder.
- `page set`: reserved, taken, out of range, unknown id (exit `2`).
- `page list --json` shape.
- The schema drift test at v3.

### Swift (Swift Testing)

- **`CellText`:** truncation, padding, missing glyphs, emoji and combining marks, accessibility string.
- **Page entry as a state machine:** digits, correction, cancel, jump on the third digit, unknown page and its 2-second reset.
- **Key mapping:** `⌥S`, `⌥G`, `⌥R`, `?`, `i`, digits, and no clash with `j`, `k`, `m`, `s` or the garden keys (on the model of `KeyboardTests`).
- **`MosaicRenderer`:** flat colours, a two-colour cell, quantisation to the eight colours, maximum width.
- **Scheme handler:** size cap, refused schemes, failure placeholder.
- **Settings:** Antiope size steps; live Native ↔ Antiope switch keeping the selection.
- **CRT screen:** active only for Antiope + checked; scanline bands land on font-pixel rows for each text size (15, 20, 25, 30); the overlay never takes a hit test; `data-crt` present in the article page exactly when active; bleed off with Increase contrast.

### Manual check before declaring done

Run on the real database and record it in `notes.md`. No simulated keystrokes into an app the user is using (rule since 11/10); in-process tests only.

1. Switch to Antiope and back without relaunching.
2. Type `412`, then an unknown page.
3. Read with SUITE / RETOUR only.
4. Swap a mosaic to its original and back with `i`.
5. Triage a feed in P800 and undo.
6. Check the separators stay straight on feeds with emoji or CJK titles.
7. Try sizes 15 to 30 and the 40-column width.
8. VoiceOver on the index column.
9. CRT screen on: clicks and scrolling land where expected, lines stay crisp at every size, the box is disabled in Native.

### CI

Unchanged: the Linux job runs the Rust checks; the macOS app job stays allowed-to-fail until GitHub has Xcode 27.

## 12. Phases

Each phase touches at most five files; Clément approves each phase before the next starts.

1. **Core:** schema v3, page assignment, `v_page`, `ressac page list/set`.
2. **App foundations:** Bedstead, `Teletext/`, Appearance setting and root switch, `GardenStatusStyle`.
3. **Antiope frame:** header, three columns, Minitel bar, focus provider, digits and keys, P100 / P899, search prompt.
4. **Antiope reader:** stylesheet, size steps, 40 columns, double height.
5. **Mosaics:** renderer, scheme handler, swap to the original.
6. **Garden in Antiope**, banners and progress, accessibility, README and `notes.md`.
7. **CRT screen:** `CRTOverlay`, `crtBleed()`, the `data-crt` stylesheet rules, the Settings checkbox.

## 13. Risks and open points

- **Bedstead is SAA5050, not Antiope:** accepted. A future Antiope-shaped font drops in via `TeletextFont`.
- **Pure blue on black** is barely legible: blue never carries text (§5).
- **Long reading in a pixel font** may tire: the 15–30 sizes and the 40-column width are the levers; the Native look remains one switch away.
- **Mosaic fetches** add a second download of each image when the original is shown. Accepted; the original is only loaded on demand.
- **CRT bleed doubles text drawing** in the columns (two tinted copies per row): checked for scrolling smoothness in phase 7 on the largest folder; the fallback is the glow alone.
- **`scaleY(2)` double height** in WebKit may blur at some sizes: checked in phase 4, with the fallback being a font size of 2 em in the same row grid.
