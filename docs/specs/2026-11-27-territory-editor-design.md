# The territory editor in the Sea Chart

Design, 27/11/2026. Chosen with Clément in the visual companion (`.superpowers/brainstorm/…/territory-editor-approaches.html`): approaches A and B for the gestures, C for everything else.

## Why

The Sea Chart is where a badly grouped publisher or a misplaced sea shows, so it is where Clément wants to fix it. Today territories, continents and the list of seas change only by writing a JSON file and running `ressac chart import`, which replaces every theme and territory at once and silently drops what feeds inherited when they were added unless the file was exported first (README « The database is canonical »). The editor makes those fixes direct, visible and undoable, and should be a pleasure to use: you survey and draw your own chart.

## Goals

1. **Survey mode (A).** A `✒ Survey` toggle in the chart's toolbar (next to `Key`) switches the chart into editing: a faint survey grid, a quill cursor, islands that can be dragged. Off by default; the chart is read-only and unchanged when it is off.
   - Drag an island onto a landmass (a territory): the feed joins that territory; the coastline redraws.
   - Drag an island onto a lone island: the two form a new territory, named at once in a cartouche (as in B).
   - Drag an island out to open water: it leaves its territory (a territory left with fewer than two feeds disappears from the chart, as today, and is deleted).
   - In "By theme", drop an island into another sea's water: the feed's theme changes to that sea (the garden inspector's Theme pop-up, by gesture).
   - Click a territory, continent or sea name: rename it in place, in small capitals, Enter to confirm, Esc to cancel.
   - An island dropped where nothing applies springs back; positions are never stored (islands stay placed by the chart's rules).
2. **Drawing a coastline (B).** In Survey mode, drawing a closed freehand loop around two or more islands makes them a new territory and asks its name in a small cartouche (`Name this land`). Islands already in another territory move to the new one. A loop that encloses every island of one sea offers `Make a continent` instead (the sea becomes one landmass). The drawn line is ink while drawing; the invented coastline replaces it once named.
3. **The cartographer's log (C).** A drawer on the right side of the chart, open in Survey mode (and with a toolbar button), titled `Cartographer's Log`:
   - the selected island: its feed title, `Sea:` and `Land:` pop-ups, `Leave Land` and `New Land…` buttons;
   - `Lands`: every territory and continent with its feed count; rename, delete, turn a continent back into a sea's open water;
   - `Seas`: every theme in display order with its feed count; rename, reorder (drag `≡` or Alt+↑/↓), `New Sea…`, delete (its feeds become unthemed, after a confirmation that names how many);
   - everything reachable from the keyboard: the drawer is the accessible path for every gesture of A and B.
4. **Undo.** Every edit is undone with ⌘Z on the Mac and Ctrl+Z in the KDE app (redo with ⇧⌘Z / Ctrl+Shift+Z), as many steps as were made while the chart window is open, like the garden's undo. The toolbar or log shows the last action's name (`Undo Join APPLE PRESS`).
5. **Direct writes.** Each edit is written to the database at once, in one transaction, and the chart redraws from the new snapshot. `ressac chart import` / `export` keep working and see the same data.
6. **Both apps.** The editor lives in the shared chart page (`apps/macos/Ressac/Resources/Chart/`: `index.html`, `chart-core.js`, `chart-draw.js`, `chart.css`), so the Mac app (WebKit) and the KDE app (QtWebEngine over QWebChannel) get it together. Each app only adds the path that carries an edit from the page to the core.
7. **The terminal.** The same edits exist as `ressac chart` subcommands, so the Mac app (which writes through the `ressac` helper, as it does for `ressac feed theme`) and scripts use one code path: create, rename, delete a territory; add and remove a feed; make and unmake a continent; create, rename, reorder, delete a theme.

## Non-goals

- Moving islands to positions of one's choosing (positions stay computed; the TODO line « Positions mémorisées pour la vue du port » stays open).
- Curved labels, drawn coastline shapes kept as drawn (the drawn loop only selects; the coastline stays invented by `chart-core.js`).
- Editing in the terminal reader (TUI) or in Antiope.
- Collaborative or multi-window editing beyond what live refresh already gives.

## Anti-goals

- An edit that loses data the way a careless re-import does: no edit may touch a feed, theme or territory it was not aimed at.
- A chart that can be edited by accident: nothing moves or changes unless Survey mode is on.
- Article or feed content gaining any way to run script or reach the bridge (feed titles and names come from feeds and the user; they stay text everywhere, as today).
- A gesture-only feature: every edit must also be possible from the log with the keyboard.

## Constraints

- Schema v5 stays (`theme`, `feed.theme_id`, `territory`, `territory_feed`); no migration unless the builder proves one is needed, and then it follows the existing migration rules (backup, version bump, both apps and the CLI refuse a newer schema).
- The chart page keeps working offline and in both web engines; `chart-core.js` stays pure (no DOM) and tested with `node --test`.
- The chart's existing contracts hold: the snapshot JSON (`version` 1) and the import/export file format; existing tests stay green (`ChartTests/*.test.cjs`, Swift `ChartBridgeTests`, KDE `tests/chart.rs`).
- UI strings in English, identical in both apps; the names in this document (`Survey`, `Cartographer's Log`, `Name this land`, `Make a continent`, `Leave Land`, `New Land…`, `New Sea…`, `Lands`, `Seas`, `Sea:`, `Land:`) are the strings.
- Theme keys follow the import rules (not empty, not `none`, not starting with `-`); a new sea gets a key derived from its name.
- Reduce Motion / Plasma's animation speed: no animated ink or drawing flourish when motion is reduced.

## Approaches (chosen in conversation)

- **A. Survey mode with drag and drop**: one gesture per action, everything visible. Moderately fun.
- **B. Drawing a coastline with the quill**: fast for grouping many feeds, the most fun.
- **C. The cartographer's log**: complete and safe (seas included, keyboard), not fun alone.

**Chosen: A + B for the gestures, C for everything else** (Clément, 27/11/2026). A and B give the direct, playful fixes where the problem is seen; C covers what gestures do badly (renaming lists, reordering seas, deleting, the keyboard).

## Decided

- The editor is in the chart page, shared by both apps (Clément agreed that it would serve both).
- Edits are written immediately, with undo, not saved as a draft.
- In "By theme", dropping into another sea changes the feed's theme (my call: it is the gesture form of the garden inspector's Theme pop-up and fixes the "wrong sea" case where it is seen).
- Positions are not editable (my call: they are computed by design; storing them is a separate TODO).

## Rules (my calls after the builder check, for Clément to check)

1. **One land per feed in the editor.** The schema allows a feed in several territories (`territory_feed`'s key is `(territory_id, feed_id)`), and the chart then keeps only the last placement (open TODO line). Every edit keeps a feed in at most one territory: joining, a loop or `New Land…` moves it out of its old one. Imports still accept what they accept today.
2. **Continents follow the sea.** A continent is a sea drawn as one landmass, so its islands belong to it through their theme. On such an island: dragging it into another sea changes its theme (and so leaves the continent); dropping it on a territory or in open water springs back with the hint `Part of the continent <NAME>: move it to another sea, or turn the continent back into open water.`; `Leave Land` is disabled with the same hint. A sea has at most one continent; `Make a continent` is offered only for a sea that has none and when the loop encloses every island of that sea and no other.
3. **Undo by inverse operations, not by re-import.** Re-importing an export would revert anything another writer did meanwhile (a feed added, a theme set from the garden), which the anti-goals forbid. Each edit records what it changed (memberships with their positions, old names, old order, a deleted sea's feeds and continent) and its undo restores exactly that, nothing else.
4. **Edits name their targets by id and are checked by the core.** The page sends ids from the snapshot it shows; the core refuses an edit whose target no longer exists or no longer matches (renamed or deleted elsewhere), writes nothing, and the page shows `The chart changed; nothing was done.` then redraws from a fresh snapshot. An undo that no longer applies is refused the same way.
5. **Names.** Territory, continent and sea names are trimmed, 1 to 40 characters, shown in small capitals, and unique among lands (the schema's `UNIQUE`) and among seas, compared without case. A clash keeps the field open with `A land with that name already exists.` / `A sea with that name already exists.` A new sea's key is the name in lower case with runs of non-alphanumerics turned to `-`, trimmed of `-`; if that key is taken, empty, `none` or starts with `-`, a number is appended (`comics-2`).
6. **The loop.** It selects the islands whose centre lies inside the drawn polygon, in any sea; fewer than two selected does nothing (the line fades). Selected islands leave their old territories; an old territory left with fewer than two feeds is deleted. The new territory's members keep their chart order.
7. **Deleting a sea** deletes its continent too (as the schema's cascade does); the confirmation names both: `Delete the sea SILICON SHORES? Its 12 feeds will be uncharted, and its continent removed.` Seas' order is the one display order, used in both views and by `ressac chart`.
8. **The bridges.** The page posts `{type: "edit", …}` through the existing message paths: `webkit.messageHandlers.ressac` on the Mac (`ChartBridge.handle`), the `Chart.receive` slot over QWebChannel in the KDE app. After an edit the page waits for the new snapshot, which both apps already push on database changes (the Mac's chart tables watch, the KDE app's `note_change`).

## Left to the builder

- The core store API for each edit (granular functions next to `import_chart` in `crates/ressac-core/src/store/chart.rs`), each one transaction, with tests; the CLI subcommand names and their `--json` output.
- Where the undo stack lives (page, app or core) and how each edit's inverse is recorded, within rule 3.
- The exact fields of the `edit` message and its validation on both sides (main frame only, known ops, ids checked by the core, rule 4).
- Hit-testing, the loop-enclosure test (point in polygon), the drop targets in each view, and how the page shows a pending edit until the new snapshot arrives.
- Where the territory name cartouche and inline rename sit so labels stay non-overlapping.
- The phases of the plan (likely: core and CLI; page in Survey mode with the log; Mac bridge; KDE bridge; recette on both).
