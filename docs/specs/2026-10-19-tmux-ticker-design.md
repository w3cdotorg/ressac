# Ressac — Sub-project 5: the tmux ticker

- **Date:** 2026-10-19
- **Status:** Draft, awaiting review
- **Scope:** a `ressac ticker` command that tmux calls every 15 seconds to show, in the status bar, one recent unread entry the morning edition left out; a click on it opens the entry in the browser and marks it read. No schema change (v4), no change to the app.
- **Builds on:** `docs/specs/2026-10-10-core-and-garden-design.md` (*core spec*), `docs/specs/2026-10-16-cadence-and-edition-design.md` (*edition spec*), `docs/specs/2026-10-16-terminal-reading-design.md` (*terminal spec*), all merged.
- **Origin:** idea 13 of the 19/10 brainstorm on other ways to present feeds (mockups: claude.ai artifact "Ressac au large").

## 1. Goal

Serendipity. Clément glances at his tmux status bar now and then and sees, one at a time, entries he let slip by: recent, unread, and not in any Journal edition (what the caps left out this morning, what arrived after the edition). If a title catches his eye, one click opens it in the browser, and Ressac counts it as read.

Success looks like: Clément works in a Ghostty window with tmux. At the top right of the bar, between the windows and his pane title, clock and date, a line reads `Actualitté · Prix Goncourt : la deuxième sélection…`. Fifteen seconds later it shows an entry from another feed. He clicks a Libération title: the article opens in his browser, and the entry is read in Ressac.app and in the terminal reader. Ressac.app was not running and nothing else in his bar changed.

## 2. Decisions taken with the user (2026-10-19)

- **Purpose:** serendipity, not news monitoring. Not the "rare feeds first, then The Wire" order of the first mockup.
- **Pool:** unread entries, recent, outside the Journal. "Recent" is 7 days (proposed by Claude, accepted).
- **Click:** opens the entry's web page in the browser, not Ressac. Marks the entry read.
- **Click mechanism:** a tmux user range (`#[range=user|X]`) and a `MouseDown1Status` binding, tested on 19/10 in an isolated tmux 3.8 server in Ghostty: a plain click works. An OSC 8 link (`#[link=…]`) did not open, with `mouse on`, so it is rejected.
- **Pace:** a new entry every 15 seconds (tmux's default `status-interval`, already in effect).
- **Order:** random, no entry shown twice until every other one has been, and feeds alternate so a feed with many left-out entries (Actualitté) does not take over the bar.
- **Placement:** the existing right side stays: the tmux-continuum save hook (first, invisible), then the ticker, then the pane title, time and date. The session name stays on the left. Colours are those of Clément's Solarized Dark bar.
- **Empty pool:** the ticker shows nothing.
- **Ressac need not run:** the command reads the database directly.
- **Installation:** at acceptance, Claude shows the lines for `~/.tmux.conf` and adds them only with Clément's go.

## 3. Scope

### In scope

| Item | Where |
|---|---|
| Ticker pool query (read-only) | core, `store/library.rs` (`ticker_pool`) |
| Two non-creating, non-migrating store opens | core, `store/mod.rs` (`Store::open_existing`, `Store::open_existing_query_only`) |
| Selection rule, pure and seeded | binary, `ticker.rs` |
| Rotation state file | binary, `ticker.rs` |
| `ressac ticker` and `ressac ticker open <token>` | binary, `cli.rs`, `commands/ticker.rs` |
| tmux snippet in the README | `README.md` |

### Out of scope

- The app (no menu bar item, no ticker view).
- A "Ressac" pane opened on the entry (Clément chose the browser).
- Any change to the edition, to rhythm groups or to the schema.
- Other bars (day-dashboard, SketchyBar). The plain output of `ressac ticker` (§5) keeps that door open.

## 4. The pool

An entry is in the pool when all of these hold:

1. its feed is active (`v_entry` already filters on `feed.state = 'active'`);
2. it is unread (`read_at IS NULL`);
3. `published_at` is within the last 7 days of `now` (Claude's call: `published_at` rather than `fetched_at`, so that a feed imported or repaired today does not flood the pool with old entries; `published_at` already falls back to the fetch time when a feed gives no date);
4. it appears in no edition at all (`NOT EXISTS (SELECT 1 FROM edition_entry WHERE entry_id = e.id)`), not only today's, since past editions were shown too;
5. it has a URL with an `http` or `https` scheme;
6. it has a non-blank title (Claude's call: a line with only a feed name tells nothing).

The query returns `entry_id`, `feed_id`, `feed_title`, `title`, `url`, `published_at`. No limit is needed for correctness; the builder may cap it (e.g. 2 000 rows, newest first) for speed. It must stay well under 50 ms on the real database (≈ 66 entries per edition, a few thousand recent entries).

## 5. The command

### `ressac ticker [--tmux] [--width N]`

Prints one line and exits 0, always. On any error (database missing, locked beyond the timeout, schema not v4, I/O on the state file) it prints an empty line and logs the error to `~/Library/Logs/Ressac/` like the other commands. Nothing on stderr: tmux would ignore it, and other callers should not see noise.

- Without `--tmux`: plain text, `Feed title · Entry title`, for other bars and for trying it by hand.
- With `--tmux`: the same text wrapped in a user range, with the feed name in bold:
  `#[range=user|rs<entry_id>]#[bold]Feed title#[nobold] · Entry title#[norange]`
  - The range argument is `rs` followed by the entry id. It must stay within tmux's 15-byte limit (`rs` + up to 13 digits).
  - Every `#` in the feed or entry title is doubled (`##`), and control characters and newlines are removed, so that a title cannot inject tmux formats or styles.
  - No colours: the line inherits `status-right-style`, so it follows whatever theme the bar has.
- `--width N` (default 70) caps the whole visible text in terminal columns, by display width (CJK and emoji count as two), cutting the entry title first and ending it with `…`. If the feed title alone is wider than half of `N`, it is cut too.
- Empty pool: an empty line.

**Opening the database.** Two new constructors beside `Store::open`, which stays unchanged for every other caller (Claude's call):

- `Store::open_existing(path)`: the file must exist (never created, nor its directory); no migration and no backup, the schema version must equal `SCHEMA_VERSION`, otherwise an error; WAL and foreign keys as in `configure`.
- `Store::open_existing_query_only(path)`: the same, plus `PRAGMA query_only = ON` and a busy timeout of 250 ms, so a status redraw never waits on a long write. Not `SQLITE_OPEN_READ_ONLY`: a read-only open of a WAL database whose `-shm` file is missing fails with "SQLite error 14" (met during the app's acceptance on 11/10, notes.md), and the `-shm` file disappears whenever the last connection closes.

The database path is resolved as for every other command (`--db`, then `RESSAC_DB`, then the default path).

### `ressac ticker open <token>`

Called by the tmux binding with the clicked range's argument.

1. Parse `<token>`: `rs` followed by digits, otherwise stop.
2. Open the store with `Store::open_existing`: if the file is missing or the schema version is not `SCHEMA_VERSION`, stop. A click in the status bar must never create a database or trigger a migration (Claude's call; migrating stays the app's and the other commands' job).
3. Read the entry. If it is gone, or its URL is not `http`/`https`, stop.
4. Mark it read (`set_read(entry_id, true, now)`), then open its URL with the `open` crate (already a dependency of the reader). Marking first means a failure to open the browser still leaves a consistent state, and the next ticker call skips the entry.

It prints nothing and exits 0 in every case, "stop" included; each stop is logged with its reason. `run-shell` shows a command's output and a non-zero exit status in the active pane, so anything else would pop up in Clément's work (Claude's call, after reading tmux's manual).

## 6. Selection and rotation

Each tmux call is a new process, so the rotation lives in a small state file:
`~/Library/Application Support/Ressac/ticker.json` (next to `terminal.json`), holding:

- `shown`: the entry ids shown in the current cycle;
- `last_feed`: the feed of the last entry shown;
- `current`: the entry on display and its `slot`, the number of the 15-second slot it was picked in (`unix_seconds / 15`).

**One entry per slot, whatever the number of callers** (Claude's call). tmux runs `#()` per client, so two attached clients, or a redraw, can call the ticker more than once in 15 seconds. A call whose slot equals `current.slot` reprints `current` instead of picking, unless `current` has left the pool (read, for instance after a click), in which case it picks at once. Every client then shows the same entry, and the rotation keeps its 15-second pace. The whole read-pick-write sequence runs under an exclusive `flock` on `ticker.json.lock`, taken with a short timeout (200 ms); a call that cannot take it reprints `current` from the file without writing.

Selection, as a pure function `pick(pool, state, rng) -> Option<(Entry, State)>`:

1. Candidates = pool minus `shown`. If empty and the pool is not, start a new cycle (`shown` cleared) and take the whole pool.
2. Group candidates by feed. Choose a feed uniformly at random among those different from `last_feed`; if `last_feed` is the only feed left, choose it.
3. Choose an entry uniformly at random within that feed.
4. Add it to `shown`, set `last_feed`, return it.

Choosing the feed first, then the entry, is what keeps a feed with forty left-out entries from appearing forty times as often as a feed with one.

- `shown` is pruned of ids no longer in the pool (read, too old, deleted) on every call, so the file stays small.
- The state file is written atomically (temporary file, then rename). A missing or unreadable file means an empty state; it is never an error.
- Randomness: a seedable generator injected into `pick`, so tests are deterministic. Seeding in production from the time is enough.

## 7. tmux configuration

Shown in the README and installed at acceptance (§9). The binary is the helper inside the app Clément runs, by absolute path, since `ressac` is not on his `PATH`:

```tmux
# Ressac ticker: a recent unread entry the Journal left out, every 15 s; click to open it.
set -g status-right-length 160
set -g status-right '#(/path/to/Ressac.app/Contents/Helpers/ressac ticker --tmux) "#{=21:#{q/h:pane_title}}" %H:%M %d-%b-%y'
bind -n MouseDown1Status if -F '#{m/r:^rs[0-9]+$,#{mouse_status_range}}' \
  "run-shell -b '/path/to/Ressac.app/Contents/Helpers/ressac ticker open #{q:mouse_status_range}'" \
  'switch-client -t ='
```

The binding accepts only `rs` followed by digits (a regular expression) and passes the range quoted, and the pane title is escaped (`q/h`), so no program can add a clickable range or a style to the bar through its pane title.

- These lines go **before** the plugin manager's `run 'tpack init'`: tmux-continuum prepends its save hook to `status-right` when it loads, so it must find the new value. Claude checks after reloading that `status-right` still starts with `continuum_save.sh`.
- `switch-client -t =` is tmux's default `MouseDown1Status` action, kept for every click outside the ticker (clicks on window names are handled by their own ranges).
- `status-interval` stays at its default, 15.

## 8. Tests

- **Core:** `ticker_pool` on a fixture database: the five rules of §4 each excluded by a dedicated entry (read, older than 7 days, in an old edition, in today's edition, buried feed, `mailto:` URL); one entry that qualifies.
- **Core:** `open_existing` and `open_existing_query_only` do not create a missing file or directory, refuse another schema version without backing up or migrating; the query-only one cannot write and opens a WAL database whose `-shm` file is missing.
- **Selection:** with a seeded generator: no repeat within a cycle; a new cycle after exhaustion; never the same feed twice in a row when another feed has candidates; with one feed of 40 entries and one of 1, the small feed appears in about half the picks over a cycle's first rounds (statistical bound on a fixed seed, not a flaky test).
- **Formatting:** `#` doubled, control characters removed, width by display columns with a CJK title and an emoji, `…` cut, the 15-byte range limit.
- **State file:** round trip, pruning, a corrupt file read as empty; two calls in the same slot print the same entry; a call in the same slot after `current` was read picks a new one; a held lock leads to a reprint without a write.
- **CLI:** `ressac ticker` on a missing database prints an empty line and exits 0; `ticker open` with a bad token, an unknown entry or a missing database prints nothing, exits 0 and changes nothing; with a valid one it marks the entry read in a temporary database (the browser call is behind a trait or a flag so the test does not open a browser).
- Never against the real database.

## 9. Acceptance

With Clément, after the code is merged into a branch build:

1. On a copy of the real database, with a temporary HOME holding `Library/Application Support/Ressac/` (so the live state is never touched): call `HOME=$T ressac --db <copy> ticker` once every 15 seconds, twenty times (about five minutes), and time one call (target well under 50 ms). Expected: twenty entries, never the same feed twice in a row when another feed has candidates, none from an edition, none read. (Twenty calls in a row would all fall in one 15-second slot and print the same entry by design; and `--db` alone would still read and write the live `~/Library/Application Support/Ressac/ticker.json`, since the state path derives from HOME.)
2. Claude shows the `~/.tmux.conf` lines; with Clément's go, adds them before `run 'tpack init'`, reloads tmux and checks the continuum hook.
3. Clément watches the bar for a minute (two attached tmux clients can differ for a few seconds of each slot, their status timers not being aligned to the slots): a new entry every 15 seconds, his pane title, time and date unchanged, clicks on windows still work.
4. Clément clicks a title: the browser opens it; the entry is read in the terminal reader (and in Ressac.app if open).
5. Rollback is documented: removing the three lines restores the bar.

## 10. Left to the builder

- The module layout inside the binary (`ticker.rs` plus `commands/ticker.rs`, or one module).
- The random generator crate (`fastrand` or `rand` with a small RNG), as long as `pick` is seedable.
- The pool's speed cap, if any.
