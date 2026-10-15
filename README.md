# Ressac

Un lecteur de flux RSS/Atom/JSON Feed pour macOS, en deux interfaces : terminal (Rust, ratatui) et appli native (SwiftUI). Le flux et le reflux de vos abonnements, avec le ménage en prime.

Thème graphique rétro : **Antiope**, en hommage au télétexte français.

## Usage

```sh
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
ressac garden apply-safe               # apply every undecided safe repair
ressac migrate                         # bring the database to the current schema (v3)
ressac page list                       # every page number, reserved ones first; --json for NDJSON
ressac page set feed 12 412            # renumber a feed (or a folder) by hand, within 110-799
```

Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`.

**Schema v3.** The Mac app migrates the shared database to schema v3 on first launch, after a backup next to it (`ressac.sqlite.bak-v2`, or `ressac.sqlite.bak-v1` for an older database). v3 gives every folder and feed a three-digit page number, used by the Antiope look and listed by `ressac page list`; numbers never change on their own. A `ressac` built before v3 refuses the database ("database created by a newer Ressac"). Get a v3 CLI with `cargo install --path crates/ressac`, or use the app's own copy at `Ressac.app/Contents/Helpers/ressac`.

## The Mac app

`Ressac.app` (macOS 27) reads the same database as the terminal and runs its own copy of `ressac`, bundled at `Ressac.app/Contents/Helpers/ressac`, for everything that fetches or decides. Reading state (read, starred) is written by the app directly; the garden's decisions go through `ressac garden decide`, so the terminal garden sees them, and `u` / `⌘Z` undo in either place.

The app icon source is `apps/macos/Ressac/Resources/AppIcon.svg` (a 1024 squircle-template drawing; the PNGs in `Assets.xcassets/AppIcon.appiconset` are exported from it).

Build it with Xcode 27 and XcodeGen:

```sh
cd apps/macos
xcodegen generate
xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$TMPDIR/ressac-build" build
cp -R "$TMPDIR/ressac-build/Build/Products/Release/Ressac.app" /Applications/
```

- **First run:** drop an OPML file on the window (or choose one, `⌘O`). A sheet shows what `ressac import --dry-run` found, then the app imports and fetches every feed.
- **Reading:** three columns (sidebar with Unread, Today, Starred, your folders and the Garden; entry list; reader). `j`/`k` next/previous, `Space` scrolls then moves on, `m` read, `s` star, `⇧⌘A` mark all read, `⌘R` refresh the selection, `⇧⌘R` refresh everything, `⌘F` search, `⌘↩` open in the browser. Refresh is manual only.
- **Full article:** the toolbar's Full Article toggle fetches and extracts the page of a truncated entry; a feed's context menu offers "Always fetch full article". Articles render with JavaScript off, and every link opens in your browser.
- **Garden:** the sidebar's Needs triage, All feeds and Cemetery lists, with the terminal's keys: `k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `⇧R` apply safe repairs.
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
