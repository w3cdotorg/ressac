# Ressac — Sub-project 8: Ressac on Linux (terminal, then a KDE app)

- **Date:** 2026-10-23
- **Status:** Draft, reviewed by a builder check, waiting for Clément's reading
- **Scope:** run Ressac on Fedora KDE with its own database: first the terminal side (reader, garden, ticker, every subcommand) with Linux file locations, then a graphical app in Qt 6 / QML / Kirigami that matches the Mac app feature for feature, the CRT screen being optional. The Mac app and its database layout do not change. Schema unchanged (v6).
- **Builds on:** every merged spec (core and garden, Mac app, Antiope, cadence and edition, terminal reading, tmux ticker, sea chart, add feed).
- **Companion documents** (in `docs/specs/linux/`, written for this spec on 23/10 at `9b8c56f`):
  - `mac-app-inventory.md`: what the Mac app does, behaviour by behaviour, with `file:line` citations. **It is the parity checklist**: the Linux app does what it lists unless this spec says otherwise.
  - `portability-audit.md`: every macOS assumption in the Rust crates, the proposed path layout, the core API a GUI needs, the CLI/JSON contract.
  - `qt-spike-notes.md` and `qt-spike/`: a throwaway build of Rust + cxx-qt + Kirigami + QtWebEngine on Fedora 44 aarch64 that worked, with its sources, screenshots and gotchas.

## 1. Goal

Clément has a second Mac, marghe, running **Fedora KDE Plasma in Parallels** (aarch64, Wayland). He wants Ressac there as a real desktop app, not only in a terminal: the same reading, Journal, garden, Antiope look, Sea Chart and themes as on his main Mac. Ressac on marghe lives its own life: **its own database, its own subscriptions**, no sync with the main Mac.

Success looks like: on marghe, Clément launches Ressac from the Plasma launcher. The first window offers to import an OPML file (for example one exported on the main Mac with `ressac export`). After the import the feeds are fetched. The next morning the Journal is built at the first launch. He reads with `j`, `k` and Space, sorts dead feeds in the garden, switches to Antiope and types `104`, opens the Sea Chart on a second virtual desktop. In Konsole, `ressac` opens the same database, and the tmux ticker works as on the Mac.

## 2. Decisions taken with the user (23/10/2026)

- **Target:** Fedora KDE Plasma on marghe, in a Parallels VM (so aarch64, Wayland, possibly without reliable GPU acceleration). Not Debian, not Asahi.
- **A graphical app, like the Mac one**, in **Qt 6 / QML with Kirigami** (Claude's proposal, accepted: the native toolkit of KDE).
- **Everything the Mac app does**: base, reading, Journal, garden, Antiope, Sea Chart, themes, add feed, OPML. **The CRT screen only if it is not too complicated** (§7.10: last phase, cut without regret).
- **Own database on marghe**, no shared state with the main Mac.
- **Tests** on OrbStack (Claude's choice): a Fedora container of the same release and architecture as marghe; the existing `fsvbuild` and `gotenberg` containers are not touched.
- **Claude may start the Fedora VM on marghe and run commands in it** (`ssh willow@marghe.local`, then `prlctl start "Fedora Linux"` / `prlctl exec "Fedora Linux" …`), to check early builds there.
- **No screen-reader (Orca) parity.** The VoiceOver behaviours in `mac-app-inventory.md` §5.9 are not ported.

### 2b. Facts checked on marghe's VM (23/10/2026)

- Fedora **44** KDE Plasma Desktop Edition, **aarch64**, 7 CPUs, 7.7 GB RAM, 52 GB free.
- Already installed: `qt6-qtbase` and `qt6-qtwebengine` 6.11.2, `kf6-kirigami` 6.31.0, `rust`/`cargo` 1.99.0, the same versions as the spike container.
- GPU: Parallels' virtio GPU with **virgl** (3D through OpenGL); no Parallels Tools package.
- Display: KWin output scale **2** (integer, 200 %), so Antiope's whole-pixel cells and scanlines snap cleanly; fractional scales are not a target.
- Time zone Europe/Paris. No autologin: a graphical check needs Clément to be logged in to Plasma.
- The VM shows no IP address to `prlctl list`; commands go through `prlctl exec` (as root; use `runuser -u willow` for the user's session).

## 3. Calls made by Claude (to check while reading)

1. **Two parts, in order.** Part A, the Linux base (paths, ticker, wording, scripts, release), is useful alone and is a prerequisite of Part B, the KDE app.
2. **The KDE app links `ressac-core` in-process.** The Mac app drives a bundled `ressac` binary through NDJSON because Swift cannot link Rust easily. A Rust app can call the core directly, as the terminal reader already does (`portability-audit.md` §3). One binary, typed errors instead of exit codes and stderr lines, no event parsing. The `ressac` CLI is still installed next to it for the terminal and tmux.
3. **Most of the app's logic in a Qt-free Rust crate** (`crates/ressac-ui`): view models, state machines, sanitiser, mosaics, cell text, keyboard maps. It builds and tests with plain `cargo test` on the Mac and in CI, like the rest of the workspace. The Qt crate stays a thin layer: bridges, QML, a little C++.
4. **Build with cargo only** (cxx-qt-build), as the spike did; no CMake. Installation is a script, not an RPM.
5. **Settings in `QSettings`** (`~/.config/ressac/ressac-kde.conf`), with the Mac app's keys. `terminal.json` stays the terminal reader's.
6. **Shortcuts follow KDE habits**: ⌘ becomes Ctrl, ⌥ becomes Alt, plus F5 for Refresh. Full table in §7.2.
7. **Two small fixes of known Mac gaps come for free**: a Theme picker in Antiope's garden inspector, and a private MIME type for dragged feeds (so dropping a feed into a text field no longer pastes `ressac-feed:<id>`). The other gaps listed in `mac-app-inventory.md` §11 are kept as they are.
8. **The Mac app's resources are reused in place**, not copied: `reader.css`, the chart page (`Resources/Chart/`), the Bedstead, Newsreader and Atkinson fonts. A Linux-only stylesheet or page variant is added only where Chromium needs it.
9. **No new schema version.** Moving validations into the core changes no table.

## 4. Scope

### In scope

| Item | Where |
|---|---|
| Linux file locations (XDG) with macOS unchanged | `ressac-core` `paths.rs`, CLI |
| Ticker state per database (fixes a Mac bug too) | CLI `ticker/` |
| Neutral wording, detached browser opening, SIGTERM like Ctrl-C | CLI |
| Linux check script, CI jobs, Linux CLI release artifacts | `scripts/`, `.github/workflows/` |
| Core API prepared for an in-process GUI (validations moved from the CLI) | `ressac-core` |
| Qt-free view-model crate | `crates/ressac-ui` (new) |
| The KDE app | `apps/linux/` (new workspace member `ressac-kde`) |
| Install script, `.desktop` file, icon | `apps/linux/` |
| Fedora dev container and recette tooling | `apps/linux/dev/` |

### Out of scope

- Any change to the Mac app or to the macOS paths.
- Sync between two Ressac databases.
- RPM, Flatpak, COPR; x86_64 testing of the app (it should build, nobody tries it).
- Translations (the UI is in English, as on the Mac, with Antiope's French labels).
- Background fetching, notifications, a system tray icon (the Mac app has none).
- GNOME-specific polish; Windows.
- Screen-reader accessibility (Orca); keyboard use stays complete.
- Fractional display scales (marghe runs at 200 %).

### Anti-goals (a failure even if it works)

- The Mac app or a Mac `ressac` sees a different path, or loses its settings.
- The KDE app opens the database with a second SQLite library (QtSql, system libsqlite): two copies in one process break each other's locks (`portability-audit.md` §3.3).
- An article's own JavaScript ever runs in the reader.
- `cargo test` on the Mac, or the Mac CI job, needs Qt installed.

## 5. Part A: the Linux base

Details, call sites and line numbers are in `portability-audit.md` §2. Summary:

### 5.1 One source of truth for paths

New `ressac_core::paths` (written by hand, no `dirs` crate: it would not reproduce the macOS names):

| File | macOS (unchanged) | Linux |
|---|---|---|
| `ressac.sqlite` and its `-wal`, `-shm`, `.bak-vN` | `~/Library/Application Support/Ressac/` | `$XDG_DATA_HOME/ressac/` (`~/.local/share/ressac/`) |
| `terminal.json` | next to the database | `$XDG_CONFIG_HOME/ressac/` (`~/.config/ressac/`) |
| Ticker state, lock | next to the database | `$XDG_STATE_HOME/ressac/` (`~/.local/state/ressac/`) |
| Logs | `~/Library/Logs/Ressac/` | `$XDG_STATE_HOME/ressac/logs/` |
| Cache | — | `$XDG_CACHE_HOME/ressac/` (reserved: mosaics stay in memory) |

- `--db` and `RESSAC_DB` keep priority. A relative `XDG_*` value is ignored (as the XDG spec says). No `HOME` → an error, never a path relative to the working directory.
- `Paths::resolve(macos: bool, env)` is pure, so both layouts are tested on any host.
- **Ticker state per database**: the default database keeps `ticker.json`; any other `--db` uses `ticker-<16 hex of the canonical path's hash>.json` in the same directory. This fixes the Mac bug where a second database shared the first one's rotation.
- The ticker creates its state directory only when the database exists (a missing database still means an empty line and nothing created).
- If `~/Library/Application Support/Ressac/ressac.sqlite` exists on Linux (a ressac run before this change), log a warning once per run naming it. Never move it.
- The `--db` help text is per OS (`cfg_attr` on the doc comment).
- CLI tests clear the developer's `XDG_*` variables (or point them at the temp dir).

### 5.2 Small fixes

- "is Ressac.app writing?" becomes "is another Ressac writing?" (`tui/reader/jobs.rs` and its tests).
- Opening a link uses `open::that_detached`, so the TUI never waits for `xdg-open`.
- SIGTERM is handled like the first Ctrl-C (clean stop, exit 130), on both systems.
- Konsole image protocol: **not** in this part. Half-blocks are the fallback today; a manual check on marghe decides if a later fix is worth it (TODO).

### 5.3 README

A "Linux" section: where files live, `cargo install --path crates/ressac`, the tmux snippet with the Linux binary path, the VM notes (keep the database on the VM's own disk, never a Parallels shared folder, since WAL needs a local filesystem; check the VM's time zone, often left on UTC, because the Journal's day starts at 04:00 local).

### 5.4 Checks and release

- `scripts/check-linux.sh`: the local gate when GitHub Actions has no credits (until 02/11/2026). It runs `fmt`, `clippy -D warnings`, `cargo test` and the chart's Node tests in the Fedora dev container (§8.1), as a non-root user.
- CI: the Ubuntu job keeps its three commands but **excludes `ressac-kde`** (`--workspace --exclude ressac-kde`); a new Fedora-container job builds and tests everything including the app (offscreen).
- Release: `release.yml` gains a Linux job building the CLI for aarch64 and x86_64 on `ubuntu-22.04` (older glibc, so the binary runs on most distributions) and attaches `ressac-<version>-linux-<arch>.tar.gz` with its checksum. The KDE app is not a release artifact in this sub-project (built from source on marghe, §8.4).

## 6. Core preparation for an in-process app

The core's API is already used in-process by the terminal reader. What the GUI still needs (`portability-audit.md` §3.3):

- **Move into the core** the validations that live in the CLI today, so the app and the CLI refuse the same things with the same sentences:
  - the garden decision check (`check_decision`: unknown feed, buried feed, nothing to repair);
  - the `--feed` / `--folder` subset check of a fetch;
  - the first-run rule (`open_with_feeds`);
  - `today_start` (local midnight as an RFC 3339 UTC bound), shared by the TUI and the app;
  - log initialisation (file appender in `paths.log_dir()`), callable by both binaries.
- **`EventSink: Send + Sync`** (every existing sink already is), so a job's progress can be forwarded across threads without wrappers.
- **Return values instead of CLI-built JSON** for add, cadence, theme and move (they are already returned; the CLI keeps printing them).
- **Schema probe without migration**: the app first opens with `Store::open_existing_query_only` and reads the version, then decides (migrate, refuse a newer database, fail), as the Mac app does with `ressac migrate`. Migration keeps making the `.bak-vN` backup.
- The CLI keeps its behaviour and exit codes; its commands now call the moved functions.

## 7. Part B: the KDE app

### 7.1 Architecture

```
crates/ressac-core   (unchanged role: schema, network, every computation)
crates/ressac-ui     (new, no Qt): view models and pure logic, tested by cargo test anywhere
apps/linux           (new member ressac-kde, Linux only): cxx-qt bridges, QML, ~100 lines of C++
```

- **`ressac-ui`** holds what the Swift app keeps in its testable models: sidebar tree and badges, entry list paging (500 rows, the "floor" rule, retained rows), search query building, Journal layout and phases, the edition clock (04:00), garden sections, retention, selection and key commands, keyboard maps (Native and Antiope) with held-key rules, page directory and page entry, cell text fitting, the reader's HTML sanitiser and page template, the mosaic renderer, the run coordinator's rules, add-feed and OPML preview texts. Port the Swift tests listed in `mac-app-inventory.md` §10.1 to Rust tests here wherever the logic moves here.
- **`ressac-kde`** (binary `ressac-kde`): cxx-qt QObjects exposing `ressac-ui` models to QML (list models via `#[base = QAbstractListModel]`, as in the spike), the QML views, and a small C++ shim for what cxx-qt does not wrap: `QtWebEngineQuick::initialize()`, `QApplication` (needed by the Breeze desktop style), the icon theme, two `QWebEngineUrlSchemeHandler`s, the window-level key filter, window grabs for recette.
- **Workspace**: `apps/linux` is a member but not in `default-members`, so `cargo build` and `cargo test` without `-p` on the Mac stay Qt-free. The member compiles only on Linux (`#![cfg(target_os = "linux")]` gate or a `compile_error!` elsewhere).
- **Threads.** The GUI thread owns one `Store` for reads and the four direct writes. Every network or long job runs on its own thread with its own `Ressac::open` and a current-thread tokio runtime (the terminal reader's pattern); progress and results come back through cxx-qt's `qt_thread().queue`. Cancellation is the core's stop flag. Short jobs (garden decisions, cadence, theme, move, export, extract) run FIFO on one worker thread, so a 5-second busy wait never freezes the window.
- **Run rules = the Mac's**: at most one long run (fetch, import, edition build, add), a second one refused, not queued; short runs serialised among themselves and allowed during a long run (`mac-app-inventory.md` §8.4).
- **Live data.** There is no GRDB observation in Rust. One invalidation signal reloads the visible models: after each own write, at most once per second during a long run's progress, at the end of every run, and when `PRAGMA data_version` changes. The poll runs every 3 s while a Ressac window is active, or while the Sea Chart window is open and not minimised (Wayland gives no occlusion information). Models diff by id so selection and scroll survive a reload.
- **Optimistic writes** for read, star and mark-all, reverted with the Mac's banner sentences on failure.

### 7.2 Windows, menus and shortcuts

- Main window: `Kirigami.ApplicationWindow` with a classic **menu bar** (Plasma's global menu picks it up) and a header toolbar. A **Sea Chart** window (separate top-level, size remembered). A **Settings** window (Kirigami `CategorizedSettings` or a plain dialog with two pages).
- Launch states as on the Mac (`mac-app-inventory.md` §1.1): starting, updating the database, too new (Use Default Database), failed (Use Default Database, Quit).
- Menu mnemonics never use S, G or R, so Alt+S, Alt+G and Alt+R stay free for Antiope (the Go menu is "G&o").

| Mac | KDE |
|---|---|
| ⌘N Add Feed… | Ctrl+N |
| ⌘O Import OPML… / ⇧⌘E Export OPML… | Ctrl+O / Ctrl+Shift+E |
| ⌘R Refresh / ⇧⌘R Refresh All | F5 and Ctrl+R / Ctrl+Shift+R |
| ⌘F Search | Ctrl+F |
| ⌥⌘U Show Unread Only | Ctrl+Alt+U |
| ⌘+ / ⌘− / ⌘0 text size | Ctrl++ / Ctrl+− / Ctrl+0 |
| ⇧⌘M Sea Chart | Ctrl+Shift+M |
| ⇧⌘A Mark All as Read | Ctrl+Shift+A |
| ⌘↩ Open in Browser | Ctrl+Return |
| ⌘Z garden Undo | Ctrl+Z |
| ⌘, Settings | Ctrl+Shift+, (KDE's Configure) |
| ⌥S / ⌥G / ⌥R (Antiope) | Alt+S / Alt+G / Alt+R |
| ⌘Q | Ctrl+Q |
| Letters `j k m s Space ? i`, digits, arrows | unchanged |

Letters act only when a reading window has focus and no text field does, as on the Mac.

### 7.3 Native reading

Parity with `mac-app-inventory.md` §2: three panes (sources, entries, reader) in a `SplitView` with the Mac's minimum and ideal widths; smart lists, folder tree, rhythm groups, garden scopes with the same badges and predicates; entry rows, paging, retained rows, unread filter, search over FTS within the source; 1-second auto-read and the `j` / Space read rules; the feed context menu; drag of an entry out (URL and title); feed → folder drag with the private MIME type `application/x-ressac-feed`.

- **Icons**: Breeze names chosen by the builder for each SF Symbol (newspaper, bolt, calendar, moon, sparkles, zzz, leaf…).
- **Fonts** (Settings): System sans (Plasma's, Noto Sans on Fedora), System serif (Noto Serif), Newsreader, Atkinson Hyperlegible Next (bundled). SF Pro, New York and Charter do not exist here. Default: System sans.
- **Light and dark** follow the Plasma colour scheme (`Kirigami.Theme`).
- **Share**: "Copy Link" instead of the macOS share sheet.

### 7.4 The reader page

- `QtQuick WebEngineView` with an off-the-record profile, no persistent storage, media needing a gesture.
- **The page comes from a custom scheme**, not `setHtml` (2 MB limit; base64 fonts make pages bigger). One handler serves `ressac:page/<entry>` (the article page) and `ressac:asset/<file>` (fonts, `reader.css`), so fonts are no longer inlined as base64.
- **Article JavaScript never runs**; the app's own scripts do (Space paging, mosaic swap, failed-image marks, Antiope sub-page line). The mechanism is the builder's: either page JavaScript off and app scripts in an isolated world, or JavaScript on with the CSP `script-src 'none'` and the sanitiser stripping every script. The first reader task proves it with a test page carrying inline, external and event-handler scripts.
- **Sanitiser in Rust** (`ressac-ui`), on html5ever/scraper (already a dependency), with the Mac allowlist and its guards: 512 KB input, nesting depth 256, 2048 attributes on one tag, 1 s budget on a big-stack thread, plain-text fallback, 32-entry memo (`mac-app-inventory.md` §2.7).
- **Links**: a clicked http(s) link opens in the default browser (`QDesktopServices`); `ressac-toggle:<n>` swaps a mosaic; everything else is cancelled.
- Full-article toggle, notices, reload policy (marking read never reloads), styles and widths as on the Mac. `reader.css` is reused; Chromium-only adjustments go in a small `reader-linux.css`.

### 7.5 Journal

Parity with `mac-app-inventory.md` §3: sections, overflow rows, header, placeholders, "That's the edition.", Space in rank order, phases and the 15-minute wait after a failure.

- **Triggers**: at launch, when the app becomes active (`Qt::ApplicationActive`), and a once-a-minute check of the wall clock that notices 04:00 passing, a resume from suspend and a time-zone change alike (a monotonic timer alone would miss the suspend). "Today" uses the same check for its midnight rollover.

### 7.6 Garden

Parity with `mac-app-inventory.md` §4: table with Status | Rhythm grouping, sections and glyphs, retention and dimming, selection rules, keys (`u R k r b o w`, chords ignored), the safe-repairs confirmation, the inspector with rhythm line and Auto…Silent picker, Theme picker, evidence, remarks, links and the fetch log.

### 7.7 Add feed, OPML, first run, settings

- Add Feed dialog (Ctrl+N) with clipboard prefill, Theme and Folder pickers, refusals in the dialog, the bottom note after success (`mac-app-inventory.md` §7.1).
- OPML import with the dry-run preview and confirmation, then fetch; drop of an `.opml` file on the window; export through the save dialog (`QtQuick.Dialogs`, which goes through the XDG portal).
- **First run**: no active feed → the window shows "No Feeds Yet — Drop an OPML file here, or choose one…".
- Settings: Reading page (appearance, CRT if it exists, font, size, line width, preview) and Data page (database path with Choose…, New…, Use Default, the relaunch rule, Show Logs). Same keys as on the Mac (`mac-app-inventory.md` §7.3) in `QSettings`.

### 7.8 Sea Chart

- A separate window holding a `WebEngineView` on the **unchanged** chart page (`chart-core.js`, `chart-draw.js`, `d3.min.js`, CSS, fonts), served from the app's resources.
- **Bridge**: the page posts to `window.webkit.messageHandlers.ressac`. A small adapter script defines that object on top of `QWebChannel`, so `chart-draw.js` stays untouched. If the page's CSP blocks the adapter or `qwebchannel.js`, a Linux copy of `index.html` with the needed `script-src` is allowed; the JS files are never forked.
- Data: `Store::chart_snapshot` serialised in-process (the same JSON as `ressac chart --json`), version check = 1. Same refresh rules: on open, debounced 2 s after a change, every 5 minutes, one rerun after a request during a run.
- Messages `openFeed`, `openURL`, `prefs` with the same validation; prefs in `QSettings` (`chart.view`, `chart.ink`, `chart.animate`).
- Reduce Motion: Plasma's animation speed set to instant (`Kirigami.Units.longDuration == 0`) turns animation off by default, as Reduce Motion does on the Mac.

### 7.9 Antiope

Parity with `mac-app-inventory.md` §5 (pages 100–109, 800–802, 899, header clock and rolling counter, Minitel bar, page entry, search prompt, Journal page and end page, garden pages and inspector page, banner row), plus the Theme picker on the inspector page (§3.7).

- **Bedstead** bundled (CC0), sizes 15/20/25/30, no anti-aliasing, the 8 teletext colours only, blue never on text.
- **Cell text** fitted in Rust (`ressac-ui`): one grapheme per cell, unsupported glyphs as a magenta `?`, exact widths.
- **Drawing the grid**: the Mac version already stutters on big folders with one SwiftUI `Text` per span. The builder picks between QML `Text` per span and one painted item per column after a measured test: **target** smooth scrolling of a 2,000-entry folder in the Fedora container's software renderer and on marghe's VM at scale 2, frame time logged. No accessibility constraint weighs on the choice (§2). The decision and the numbers go in `notes.md`.
- **Mosaics**: the SAA5050 sextant renderer ported to Rust (`ressac-ui`), fed by the core's `fetch_image` (already SSRF-guarded, 10 MB and 15 s limits), decoded with the `image` crate (pixel cap before decoding, first GIF frame), served by a `ressac-mosaic:` scheme handler with a 16 MB in-memory cache, the Mac's failure placeholder, and `i` / click to swap with the original.
- Reader stylesheet: `reader.css`'s Antiope block, with `40ch` = 40 cells.
- Window chrome: black background, Kirigami header hidden; KWin's title bar stays (no frameless window).

### 7.10 CRT screen (optional, last)

Scanlines, vignette, tube corners and colour bleed (`mac-app-inventory.md` §5.8), as a `ShaderEffect` or `Canvas` overlay that lets clicks through, plus the reader's CSS `text-shadow`. **Budget: one phase.** If it does not look right or costs more than that phase in the VM, the CRT toggle is left out and a line goes in `TODO.md`. There is no "Increase Contrast" on Plasma: the bleed simply follows the CRT toggle.

### 7.11 Errors and logs

- Same sentences as the Mac app for banners, refusals and failures. The core's typed errors replace exit codes: a refusal (the cases the CLI maps to exit 2) shows its sentence without Show Log; a failure shows Show Log, which opens `$XDG_STATE_HOME/ressac/logs/`.
- The app logs through the core's file logger (same daily files as the CLI). `QT_FORCE_STDERR_LOGGING=1` is set in the dev container so QML messages are visible (Fedora sends them to journald otherwise).

## 8. Environment, build and install

### 8.1 Dev container (OrbStack on the main Mac)

- `apps/linux/dev/`: a `Containerfile` from `fedora:44` (aarch64) with the dnf list of `qt-spike-notes.md` §1 (including `lld`, required: the GNU linker fails), a non-root user, the repo bind-mounted, `CARGO_TARGET_DIR` inside the container (never the Mac's `target/`).
- A script to build the image, start the container (name `ressac-fedora`), and run commands in it as the user.
- Tests and recettes always run as the non-root user (the Chromium sandbox stays on; one existing test needs it).

### 8.2 Recette tooling

- `ressac-kde --screenshot <dir>` (hidden flag): opens on a fixture database, walks a scripted list of states (Native panes, Journal, garden, Antiope pages, chart), grabs each window (WebEngine content included) and quits. Run offscreen in the container, PNGs land in the bind-mounted folder.
- Live check: `QT_QPA_PLATFORM=vnc` in the container, reachable from the Mac at `ressac-fedora.orb.local:5900` with a VNC viewer.
- The final recette is on marghe, by Clément.

### 8.3 Tests

- `ressac-ui`: the bulk, Rust unit tests mirroring the Swift suites the logic came from (`mac-app-inventory.md` §10.1), runnable on the Mac.
- `ressac-kde`: bridge tests where cxx-qt allows them, and an offscreen smoke test in the container: launch on `tests/fixtures/contract.sqlite` (copied), reach each page, check every grabbed image is not blank, press the main keys through a test hook, check the database writes.
- Chart: the Node tests run unchanged, plus one WebEngine test that loads the page with `apps/macos/ChartTests/fixture.json` and checks it drew.
- Never the real database, never the network (local `wiremock` servers as today).

### 8.4 Install on marghe

- `apps/linux/install.sh`: checks the Fedora packages (prints the `dnf install` line if some are missing), builds release `ressac` and `ressac-kde`, installs them in `~/.local/bin`, the `.desktop` file (`org.w3cdotorg.ressac.desktop`) in `~/.local/share/applications`, the icon (from `AppIcon.svg`) in `~/.local/share/icons/hicolor/scalable/apps`. `--uninstall` removes exactly those files, never the database.
- Built on marghe's VM itself (Fedora's own Qt, no glibc mismatch). The source reaches the VM without pushing anything: a `git bundle` or a tarball of the branch copied over ssh to marghe, then into the VM (`prlctl exec` with stdin, or a Parallels shared folder used only for the transfer, never for the database). The builder picks one and scripts it in `apps/linux/dev/`.
- Parallels: the VM has virgl, so QtWebEngine and Qt Quick should use the GPU. If a view is blank or flickers, the `.desktop` file sets software rendering (`QT_QUICK_BACKEND=software`, `QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu`); the VM checks below decide.

### 8.5 VM checks during the work

Two checkpoints on marghe's VM, run by Claude through `prlctl exec`, so the risky parts are known before the next phases build on them:

1. **After B3** (Native reading works): install, then run `ressac-kde --screenshot` in Clément's Plasma session (`runuser -u willow` with his `XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY`). Verdict on QtWebEngine with virgl, on keys reaching the router while the reader has focus, and on fonts at scale 2. **Stop**: Clément logs in to Plasma when asked; B4 does not start before the verdict.
2. **After B8's grid decision**: the same, on the Antiope pages, with the frame-time log.

Each checkpoint's findings go in `notes.md`. Nothing is installed on the VM outside `~/.local` and the dnf packages `install.sh` names (dnf only with Clément's go-ahead the first time).

## 9. Order of work

Each phase touches at most 5 files and ends with the local checks (repo rule).

| # | Phase | Part |
|---|---|---|
| A1–A5 | Paths module and call sites; ticker per database and test isolation; small fixes; README; check script, CI and release | A |
| B0 | Core preparation (§6) | B |
| B1 | Dev container, `ressac-ui` and `ressac-kde` skeletons, launch states, settings store, install script | B |
| B2 | Data layer: models, invalidation, `data_version` poll, run coordinator, optimistic writes | B |
| B3 | Native reading: panes, keyboard, reader page (scheme, sanitiser, scripts proof first), full article, search | B |
| B4 | Journal and its triggers | B |
| B5 | Garden table and inspector | B |
| B6 | Add feed, OPML, first run, drag and drop, Settings | B |
| B7 | Sea Chart | B |
| B8 | Antiope (grid decision first, then pages, Minitel bar, mosaics) | B |
| B9 | CRT (optional) | B |
| B10 | Smoke test, screenshots, install, recette on marghe | B |

Part A can be merged on its own before Part B starts. Stops for Clément: before any push, PR or merge; at the two VM checkpoints (§8.5, he logs in to Plasma); the first `dnf install` on the VM; and the final recette on marghe.

## 10. Risks

- **Size.** The Mac app is about 10,900 lines of Swift and 490 tests. Part B is the biggest sub-project so far; the phases are cut so that each one leaves a working app.
- **QtWebEngine in a VM** (virgl, not a real GPU): blank views or slowness. Mitigation: software rendering switch, checked on marghe at the B3 checkpoint (§8.5), not only at the end.
- **Antiope's pixel grid in Qt Quick** (hinting, anti-aliasing off, exact cell widths, performance): measured test first (§7.9).
- **Keys reaching the WebEngine view before the router**: the key filter sits on the window; B3 proves it with Space and `j` while the reader has focus.
- **cxx-qt 0.10 maturity**: the spike covered list models, properties, invokables, WebChannel and a C++ shim. Anything else unwrapped goes in the shim rather than fighting the generator.
- **Two processes on one database** (the app and `ressac` in Konsole): already handled by WAL, `BEGIN IMMEDIATE`, busy timeouts and `data_version`, as on the Mac.
