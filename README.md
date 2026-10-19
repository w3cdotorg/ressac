# Ressac

Un lecteur de flux RSS/Atom/JSON Feed pour macOS, en deux interfaces : terminal (Rust, ratatui) et appli native (SwiftUI). Le flux et le reflux de vos abonnements, avec le ménage en prime.

Thème graphique rétro : **Antiope**, en hommage au télétexte français.

## Usage

```sh
ressac                                  # read: the Journal first, then every source (ressac read is the same)
ressac import ~/Documents/vienna.opml   # --dry-run to preview
ressac check                           # fetch, diagnose, look up archives
ressac garden                          # triage: k keep, r repair, b bury, u undo, ? help
ressac export -o clean.opml            # --cemetery for buried feeds
```

More subcommands (the Mac app drives them too):

```sh
ressac fetch --feed 12                 # repeatable; or --folder 3: refetch only those (the two conflict)
ressac extract 4521                    # fetch and extract the full article of an entry
ressac garden decide 12 keep           # keep | repair | bury, without the terminal UI
ressac garden undo                     # revert the latest decision
ressac garden apply-safe               # apply every undecided safe repair; --json also gives applied and skipped
ressac migrate                         # bring the database to the current schema (v4)
ressac page list                       # every page number, reserved ones first; --json for NDJSON
ressac page set feed 12 412            # renumber a feed (or a folder) by hand, within 110-799
ressac edition build                   # refresh, then compose this morning's edition (once a day, from 04:00)
ressac edition show                    # today's edition by section; --json for NDJSON
ressac feed cadence 12 daily           # force a feed's rhythm group (daily, weekly, monthly, rare, silent) or auto
```

Bare `ressac` opens the reader and no longer prints usage: scripts that need it call `ressac --help`. `--json` makes no sense for an interactive reader, so `ressac --json` and `ressac --json read` are refused (exit 2).

Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
Exit codes: `0` done, `1` error, `2` a refused request (an unknown id, nothing to repair, an address another active feed already uses, an edition already built this morning, no network to build one), `130` stopped by Ctrl-C.
The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`.

**Schema v4.** The Mac app migrates the shared database to schema v4 on first launch, after a backup next to it (`ressac.sqlite.bak-v3`, or the backup of whichever older version it found). v4 adds each feed's rhythm group, the per-feed override and the morning editions; v3 already gave every folder and feed a three-digit page number (`ressac page list`). A `ressac` built before v4 refuses the database ("database created by a newer Ressac"). Get a v4 CLI with `cargo install --path crates/ressac`, or use the app's own copy at `Ressac.app/Contents/Helpers/ressac`.

## The terminal reader

`ressac` with no subcommand (or `ressac read`) opens a keyboard reader on the same database as the Mac app; both can stay open, each sees the other's changes within a couple of seconds. It opens on the Journal and builds this morning's edition when it is due, as the app does. `ressac --json` is refused: the reader is interactive.

**Adaptive columns.** Three when the pane is 120 columns or wider (sources, entries, reader), two from 80 (the sources open over the list with `h`), one below 80 (`Enter` goes down, `Esc` comes back). The focused column is framed in double accent with a reversed title.

**Native keys**

| Key | Action |
|---|---|
| `j` / `k` | next / previous entry (sources in the sources column) |
| `Space` | scroll, then move to the next unread entry of the source |
| `h` / `l`, arrows, `Tab` | change column |
| `Enter` / `Esc` | go down / come back (one column) |
| `m` / `s` | toggle read / starred |
| `A` | mark everything in the source read |
| `u` | all entries or unread only |
| `/` | search (`Enter` apply, `Esc` cancel) |
| `f` | full article |
| `o`, then `1`-`9` | open the article, or a numbered link |
| `i` | show or hide images |
| `r` / `R` | refresh the source / everything |
| `c` | a feed's rhythm group |
| `t` | switch Native and Antiope |
| `?` | help |
| `q` | quit |

On a garden page (800 to 802) the garden's keys apply: `k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `R` safe repairs.

**Antiope keys.** The teletext look, as in the app: type a page number (`104` the Journal, `105` to `109` the rhythm groups, `800` to `802` the garden, `100` SOMMAIRE, `899` GUIDE), `⌫` corrects, `Esc` cancels; SUITE `Space` or `j`, RETOUR `k`, SOMMAIRE `S`, GUIDE `?`, RÉPÉTITION `r`. Images become teletext mosaics; `i` shows the original.

**Images.** Shown inline when the terminal speaks a graphics protocol (Kitty, as Ghostty does, iTerm2 or Sixel; under tmux, `set -g allow-passthrough on`), as half-blocks otherwise, or as `[image: …]` when they cannot load or are turned off. Under tmux the outer terminal is read from the client's terminal name when the library finds no protocol. `RESSAC_IMAGES=kitty|iterm2|sixel|halfblocks|off` forces the choice (`off` shows alt text and never queries the terminal). The decision is logged as "image protocol".

**Settings.** The look and the images switch are kept in `~/Library/Application Support/Ressac/terminal.json`.

**Logs.** `~/Library/Logs/Ressac/` (with `-v`, tracing also goes to stderr, which garbles the alternate screen: prefer the log file).

## The Mac app

`Ressac.app` (macOS 27) reads the same database as the terminal and runs its own copy of `ressac`, bundled at `Ressac.app/Contents/Helpers/ressac`, for everything that fetches or decides. Reading state (read, starred) is written by the app directly; the garden's decisions go through `ressac garden decide`, so the terminal garden sees them, and `u` / `⌘Z` undo in either place.

The app icon source is `apps/macos/Ressac/Resources/AppIcon.svg` (a 1024 squircle-template drawing; the PNGs in `Assets.xcassets/AppIcon.appiconset` are exported from it).

Build it with Xcode 27 and XcodeGen. The Release app goes to `apps/macos/build/` (ignored by git) and runs from there:

```sh
cd apps/macos
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$TMPDIR/ressac-build" build
rm -rf build/Ressac.app && mkdir -p build
cp -R "$TMPDIR/ressac-build/Build/Products/Release/Ressac.app" build/
open build/Ressac.app
```

**Releases.** Pushing a `vX.Y.Z` tag runs `.github/workflows/release.yml`: it checks that the tag matches both `Cargo.toml` and `MARKETING_VERSION` in `apps/macos/project.yml`, builds the Release app on the `xcode-27` runner and publishes a GitHub Release with `Ressac-X.Y.Z-macos-arm64.zip` (the app), `ressac-X.Y.Z-macos-arm64.tar.gz` (the terminal binary) and `SHA256SUMS`. The builds are signed ad hoc and not notarized: after downloading, clear the quarantine flag (`xattr -dr com.apple.quarantine /Applications/Ressac.app`). A tag with a suffix (`v0.2.0-beta.1`) gives a pre-release.

```sh
git tag -a v0.1.0 -m "Ressac 0.1.0" && git push origin v0.1.0
```

Quit Ressac before replacing the build. To install it for every launch instead, copy `build/Ressac.app` to `/Applications/` (the commands below assume that location; with the local build, use `apps/macos/build/Ressac.app`).

- **First run:** drop an OPML file on the window (or choose one, `⌘O`). A sheet shows what `ressac import --dry-run` found, then the app imports and fetches every feed.
- **Reading:** three columns (sidebar with the Journal, Unread, Today, Starred, your folders, the Rhythm groups and the Garden; entry list; reader). `j`/`k` next/previous, `Space` scrolls then moves on, `m` read, `s` star, `⇧⌘A` mark all read, `⌘R` refresh the selection, `⇧⌘R` refresh everything, `⌘F` search, `⌘↩` open in the browser. Refresh is manual only. Long lists load 500 entries at a time; scrolling or `j` loads the next ones, and `Space` jumps to the next unread entry even beyond the loaded rows.
- **Journal:** the first time you open Ressac each day (a day starts at 04:00), it refreshes every feed and composes a morning edition of what appeared since the previous one: **Front Page** (feeds that publish monthly or less, every new entry), **This Week** (weekly feeds, three entries each at most) and **The Wire** (daily feeds, their latest entry). What the caps leave out stays unread in its feed, behind a `+12 more in …` line. The edition is frozen once built; read it to `That's the edition.`. Offline, the Journal says so and offers Retry. `⇧⌘A` marks only the edition read.
- **Rhythm:** every feed is Daily, Weekly, Monthly, Rare or Silent, from the median gap between its recent entries. The sidebar's Rhythm section lists each group like a folder; the garden's inspector shows a feed's rhythm and lets you force its group (`Auto` gives it back to the computation), and Garden ▸ All feeds has a Rhythm column.
- **Full article:** the toolbar's Full Article toggle fetches and extracts the page of a truncated entry; a feed's context menu offers "Always fetch full article". Articles render with JavaScript off, and every link opens in your browser.
- **Garden:** the sidebar's Needs triage, All feeds and Cemetery lists, with the terminal's keys: `k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `⇧R` apply safe repairs. The Cemetery shows how many entries each buried feed left unread.
- **Settings (`⌘,`):** appearance (Native or Antiope, see below), article font (SF Pro, New York, Charter, Newsreader, Atkinson Hyperlegible Next), text size (`⌘+` / `⌘−` / `⌘0` in the View menu), line width (three widths or exactly 40 columns), database location, logs.
- **Unread only (`⌥⌘U`):** View ▸ Show Unread Only filters the entry list to unread entries, in both looks. In Antiope the `ENTRIES  ALL/UNREAD` column title is a clickable toggle for the same setting.
- **Another database for one launch:** `open -n --env RESSAC_DB=/path/to/other.sqlite /Applications/Ressac.app` (`RESSAC_DB` also sets the terminal's database).

Newsreader and Atkinson Hyperlegible Next are bundled under the SIL Open Font License 1.1; their licences ship next to them in `Ressac.app/Contents/Resources/Fonts/`.

## Antiope

Settings ▸ Reading ▸ Appearance switches the reading window to **Antiope**, a French teletext set of the 1980s, and back, without a relaunch: the Bedstead pixel font everywhere, the eight teletext colours on black, a page header with a running clock, three columns counted in character cells (index, entries, reader, each with a reverse-video title) and the Minitel keys at the bottom. Nothing is lost either way: selection, open article and search text are shared with the native look.

- **Pages.** Every folder and feed has a three-digit page number, stored in the database and shared with the terminal (`ressac page list`). Type the three digits anywhere outside a text field to jump; `⌫` (CORRECTION) erases a digit, `Esc` (ANNULATION) cancels. An unknown page shows `NOT FOUND` for two seconds.

  | Page | Shows |
  |---|---|
  | 100 | SOMMAIRE, every page with its unread count |
  | 101, 102, 103 | Unread, Today, Starred |
  | 104 | JOURNAL, this morning's edition; its entries are sub-pages ("104 3/43"), and SUITE after the last one shows FIN DE L'EDITION |
  | 105-109 | Rhythm groups: Daily, Weekly, Monthly, Rare, Silent |
  | 110-199 | Feeds outside any folder |
  | 200-700 | Root folders (six at most), each followed by its subfolders and feeds |
  | 800, 801, 802 | Garden: Needs triage, All feeds, Cemetery |
  | 899 | GUIDE, every key of the window |

- **Minitel keys** (also clickable): SOMMAIRE `⌥S`, SUITE `Space` or `j`, RETOUR `k`, GUIDE `⌥G` or `?`, RÉPÉTITION `⌥R` (refresh the selection). `←` and `→` move between the columns, `⌘F` opens the `SEARCH?` prompt. An article is a sub-page of its feed's page ("412 3/20").
- **Unread only.** `⌥⌘U` (View ▸ Show Unread Only) or a click on the `ENTRIES` title switches the entry column between `ALL` and `UNREAD`.
- **Images** show as two-colour block mosaics. Click one, or press `i` for the whole article, to see the originals.
- **Garden.** Pages 800 to 802 list the feeds by status under coloured headings, with a progress row and the usual keys (`k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `⇧R` safe repairs); the reader column tells why a feed is there.
- **Text size** snaps to 15, 20, 25 or 30 pt in Antiope (`⌘+` / `⌘−`, `⌘0` for 20), so that the pixels stay sharp. Each look keeps its own size: switching looks never changes the other one.
- **CRT screen** (Settings, Antiope only): scanlines, a vignette, rounded tube corners and a slight colour bleed. Increase Contrast in the accessibility settings turns the bleed off.

Bedstead (Ben Harris) is dedicated to the public domain (CC0); its licence note ships next to it in `Ressac.app/Contents/Resources/Fonts/`.
