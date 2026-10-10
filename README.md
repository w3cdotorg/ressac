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

Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`.

## The Mac app

`Ressac.app` (macOS 27) reads the same database as the terminal and runs its own copy of `ressac`, bundled at `Ressac.app/Contents/Helpers/ressac`, for everything that fetches or decides. Reading state (read, starred) is written by the app directly; the garden's decisions go through `ressac garden decide`, so the terminal garden sees them, and `u` / `⌘Z` undo in either place.

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
- **Settings (`⌘,`):** article font (SF Pro, New York, Charter, Newsreader, Atkinson Hyperlegible Next), text size (`⌘+` / `⌘−` / `⌘0` in the View menu), line width, database location, logs.
- **Another database for one launch:** `open -n --env RESSAC_DB=/path/to/other.sqlite /Applications/Ressac.app` (`RESSAC_DB` also sets the terminal's database).

Newsreader and Atkinson Hyperlegible Next are bundled under the SIL Open Font License 1.1; their licences ship next to them in `Ressac.app/Contents/Resources/Fonts/`.
