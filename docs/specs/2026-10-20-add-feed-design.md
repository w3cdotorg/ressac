# Ressac — Sub-project 7: add a feed

- **Date:** 2026-10-20
- **Status:** Draft, reviewed by a builder check
- **Scope:** add one feed by its feed address or by a site address (feed discovery), from the Mac app and the terminal; move a feed into a folder (drag and drop in the sidebar, `ressac feed move`); a new feed inherits the sea chart theme of a known site. Schema unchanged (v5).
- **Builds on:** core, app, edition, terminal and sea chart specs (all merged).

## 1. Goal

Today a feed only enters Ressac through an OPML import, so subscribing to a new site means going through Vienna and exporting an OPML file. Clément wants to paste an address and be done.

Success looks like: in Ressac.app, Clément chooses File ▸ Add Feed…, pastes `https://www.example.org/`, presses Add. Ressac finds the feed the page announces, adds it at the bottom of the sidebar's feed list outside any folder, fetches it, and its entries appear. He drags it onto the Blogs folder. Since another feed of the same site already sits in "Tech, Apple and geeks", the new feed is in that sea on the Sea Chart without him doing anything. Pasting a page that announces no feed shows "This page announces no feed." and adds nothing.

## 2. Decisions taken with the user (20/10/2026)

- **Input:** one address, either a feed (RSS, Atom, JSON Feed) or a site page. For a page, Ressac reads the `<link rel="alternate">` feeds it announces and **takes the first**; none → an error, nothing added.
- **Where it lands:** alone, **at the bottom of the root feeds** (outside any folder). Clément then **drags it onto a folder** in the sidebar.
- **Moving:** drag and drop of a feed onto a folder (or onto the root) in the sidebar; `ressac feed move <feed-id> <folder-id|root>` in the terminal. The Antiope page number follows the folder (§5).
- **Sea chart:** if a feed already followed comes from the **same site** (same host, `www.` ignored), the new feed takes its theme and joins its territory. Otherwise the Add Feed sheet offers an **optional Theme pop-up**; left empty, the feed goes to UNCHARTED WATERS and gets a theme later from the garden inspector.
- **Proposed by Claude, accepted:** the feed is fetched right after it is added; an address already followed is refused with a message (no duplicate); the usual network guards apply (no private address, size and time limits).
- **Ruled out:** any AI classification of the new feed (the rule since 10/10).

## 2b. Calls made after the builder check (Claude's)

- **Same site** means the same host with a leading `www.` removed, deliberately stricter than `url_key::site_of` (registrable domain): with `site_of`, every `*.substack.com` or `*.github.io` feed would count as one site and pass its theme to unrelated newcomers.
- **The database is canonical** for themes and territories. The seed file used at the sea chart's acceptance was a one-off; a later `ressac chart import` replaces themes and territories wholesale, so it would drop what an add inherited unless the file includes it. The README says so; a `ressac chart export` is a TODO.
- **Move and pages:** a moved feed takes the lowest free page of the range its new place would get from `pages::assign_missing`'s rules: a subfolder's feed uses its root folder's range; a root folder without a hundred (overflow) uses the overflow pool; root → 110–199. The old page is free as soon as the feed leaves it. Full range → the feed keeps its page. Implemented as a small "lowest free page in a range" helper shared with `assign_missing` (refactor it if that keeps one source of truth).
- **Duplicates** are checked on both the typed address and the resolved feed address (`url_key` against `xml_url` and `xml_url_original` of every feed, buried included). No content matching (`same_feed`). The feed is stored with the address literally resolved; later refreshes may upgrade it to https as for any feed.
- **The immediate fetch** reuses `refresh` with `only = [id]` and `force`; its whole-database diagnosis pass runs too, which is acceptable (it already runs after every fetch). The `added` event comes last in the `--json` stream; the app's event decoder already turns unknown events into `.unknown`, so it is added to the decoder explicitly.
- **⌘N** is free (the app replaces the New item group). **Root drop target:** the sidebar's "Feeds" section header accepts a dropped feed (back to the root); folder rows accept it into that folder.

## 3. Scope

### In scope

| Item | Where |
|---|---|
| `Ressac::add_feed(input, theme, sink)`: resolve (feed or page), check duplicates, insert at the bottom of the root feeds, inherit theme and territory, fetch | core `service.rs` + `store/feeds.rs` |
| `Store::move_feed(feed, Option<folder>)` with page renumbering | core `store/feeds.rs` + `store/pages.rs` |
| `ressac add <address> [--theme <key>]`, `ressac feed move <feed-id> <folder-id\|root>` | binary `cli.rs`, `commands/` |
| File ▸ Add Feed… (⌘N) sheet with an address field, an optional Theme pop-up, progress and errors | app `App/`, `Engine/RessacCLI.swift` |
| Drag a feed onto a folder (or the root) in the sidebar, Native look | app `Reading/` sidebar |
| README, notes, TODO | docs |

### Out of scope

- Choosing among several announced feeds (the first wins).
- Choosing a folder in the Add Feed sheet (the drag does it).
- Antiope keyboard commands for adding or moving (the Antiope look shows the result; adding is done from the menu, which works in both looks).
- Adding from the terminal reader's UI (the `ressac add` command covers the terminal).
- Reordering feeds inside a folder by drag (only moving between folders).

## 4. Adding a feed (core)

`Ressac::add_feed(input: &str, theme: Option<&str>, sink: &dyn EventSink) -> Result<AddedFeed>`:

1. **Normalise the input:** trim; no scheme → `https://`; only `http`/`https`; otherwise `Error::Invalid("Enter a web address.")`.
2. **Fetch it** through the guarded client (private addresses refused, the existing size and time limits).
   - If the body parses as a feed (the existing parser), that address is the feed.
   - Else, if it is HTML, `discover::alternate_feeds` lists the announced feeds; take the **first**, fetch it, and it must parse as a feed. None → `Error::NoFeedFound` ("This page announces no feed."); the first does not parse → `Error::NoFeedFound` too, with the address in the log.
   - Network failure → the existing network error, worded for a person ("Could not reach …").
3. **Duplicates:** if an active or buried feed has the same `url_key` (of `xml_url` or `xml_url_original`) as the resolved feed address, refuse: `Error::FeedExists { id, title, buried }` ("Already followed: <title>." or "Already in the Cemetery: <title>."). Nothing written.
4. **Insert** in one transaction: `folder_id` NULL, `position` = max position among root feeds + 1, title from the feed (else the host), `xml_url` = `xml_url_original` = the resolved feed address, `html_url` from the feed (else the site address), `url_key`, `state` active, `added_at` now; `pages::assign_missing` gives it a root page (110–199, as today).
5. **Theme and territory:** `theme` given (a known key, else `UnknownTheme` before anything is written) → set it. Otherwise look for **sibling feeds**: active feeds whose site host (from `html_url`, else `xml_url`; lowercase, leading `www.` removed) equals the new feed's host. If siblings exist: take the theme most of them share (ties: the lowest feed id's), and if one of them belongs to a territory (kind `territory`, not continent), append the new feed to that territory (lowest territory id if several). No siblings and no theme → unthemed.
6. **Fetch** the new feed right away (the existing refresh with `only = [id]`, forced), so entries and health exist; progress goes to `sink`. A fetch failure does not undo the add (the garden will show it).
7. Return `AddedFeed { id, title, xml_url, page, theme: Option<String>, territory: Option<String>, discovered: bool }`.

## 5. Moving a feed

`Store::move_feed(feed: FeedId, folder: Option<FolderId>) -> Result<Moved>`:

- Unknown feed or folder → errors (exit 2 in the CLI). Buried feeds can be moved too.
- Sets `folder_id`; `position` = last in the target folder (or root).
- **Page:** pages never change on their own, but a move is an explicit change: the feed takes the lowest free page in its new home's range (a root folder's hundred and its contents' range, or 110–199 for the root), using the same rules as `pages::assign_missing`; when the range is full it keeps its page. Returns old and new page.
- No-op when the feed is already there.

## 6. CLI

- `ressac add <address> [--theme <key>]`: progress like `fetch`; prints `Added #<id> <title> (page <n>)`, plus `Theme: <name> (same site as <title>)` or `Theme: none (Uncharted waters)`, plus `Territory: <name>` when joined; with `--json`, the event stream ends with an `added` event carrying `AddedFeed`. Exit 2 for no feed found, already followed, unknown theme, invalid address; 1 for network errors.
- `ressac feed move <feed-id> <folder-id|root>`: prints the move and the page change; exit 2 for unknown ids.
- All feed-supplied text through `output::printable`.

## 7. The Mac app

- **File ▸ Add Feed… (⌘N)** opens a sheet on the main window: an address field (prefilled from the clipboard when it holds a web address), an optional **Theme** pop-up (None + themes by position), Add and Cancel. Add runs `ressac --json add <address> [--theme key]` through the existing run machinery (progress, refusals with their sentence, failures with the log button). On success the sheet closes, the sidebar selects the new feed, and a short note says the theme it took ("In Tech, Apple and geeks, like Daring Fireball"). Errors stay in the sheet, under the field.
- **Drag to a folder (Native look):** feed rows in the sidebar are draggable; folder rows and the feed list's root accept them; dropping runs `ressac feed move`. A drop on the folder it already belongs to does nothing. The selection follows the moved feed.
- The Antiope look shows the result (new page numbers) but gets no new controls.

## 8. Tests

- **Core:** input normalisation; a feed address added directly; a page with two announced feeds → the first; a page with none → `NoFeedFound`; first announced address not a feed → `NoFeedFound`; duplicate by `url_key` (active and buried) → `FeedExists`; insert at the bottom of root feeds with a 110–199 page; theme from `--theme`; sibling inheritance (majority theme, tie → lowest id; territory joined; continent not counted as a territory); unknown theme refused before writing; private address refused; the fetch runs and a failing fetch keeps the feed. Network through wiremock, like existing service tests.
- **Move:** to a folder (position last, page in the folder's range), back to root (110–199), full range keeps the page, unknown ids, no-op.
- **CLI:** `add` output and exit codes, `--json` `added` event, `feed move` output and exit codes, controls neutralised.
- **App:** request arguments for add and move; the sheet's validation (empty field), refusal display; the drop handler maps a feed onto a folder id and ignores same-folder drops.
- Never against the real database or the real network.

## 9. Acceptance (with Clément)

1. Add a site that announces a feed; it lands at the bottom of the root feeds, fetched, with entries.
2. Add a feed address directly; add an address already followed (refused); a page without feed (refused).
3. Drag the new feed onto a folder: it moves, its Antiope page changes.
4. Add a second feed of a site already themed: it appears in that sea (and territory) on the Sea Chart.
5. `ressac add` and `ressac feed move` from the terminal.

## 10. Left to the builder

- Error type names and the exact progress events reused.
- SwiftUI drag and drop API choice (`draggable`/`dropDestination` with a transferable feed id, or `onDrag`/`onDrop`).
- Whether the sibling lookup lives in the store or the service.
