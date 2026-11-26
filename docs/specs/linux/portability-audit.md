# Ressac on Linux: portability audit

Repo: `/Users/willow/Sites/ressac` at `9b8c56f` (read-only audit, nothing modified).
Target: Fedora KDE aarch64 in a Parallels VM with its own database, as a base for a Qt/Kirigami GUI that links `ressac-core`.

**Summary.** The code already compiles and tests pass on Linux (CI runs on `ubuntu-latest`). What breaks on Linux is meaning, not the build:

- every path is hard-coded under `~/Library`;
- one user-facing sentence names `Ressac.app`;
- the ticker state ignores `--db`;
- image-protocol detection only knows macOS terminals.

The core has no `cfg(target_os)` code and no macOS-only dependency. The only native libraries that matter (`security-framework`, `core-foundation`) come in through `rustls-platform-verifier`, which on Linux uses the system CA bundle.

---

## 1. macOS assumptions, file by file

### 1.1 Hard-coded paths (all derive from `$HOME`)

| What | Where | Notes |
|---|---|---|
| Default DB `~/Library/Application Support/Ressac/ressac.sqlite` | `crates/ressac-core/src/store/mod.rs:88-94` (`default_path`) | With `HOME` unset it falls back to `unwrap_or_default()`, which gives a path relative to the current directory (`Library/Application Support/...`). |
| Its test | `crates/ressac-core/src/store/mod.rs:358-365` (`default_path_is_under_application_support`) | Checks only the string, so it passes on Linux today. Must become per-OS. |
| CLI default DB | `crates/ressac/src/main.rs:49-52` (`unwrap_or_else(default_path)`) | `--db` / `RESSAC_DB` take priority (`cli.rs:18`). |
| `--db` help text | `crates/ressac/src/cli.rs:17` (`[default: ~/Library/Application Support/Ressac/ressac.sqlite]`) | Static doc comment. |
| Log dir `~/Library/Logs/Ressac` | `crates/ressac/src/main.rs:129-141` (`init_logging`) | Daily `ressac.log.YYYY-MM-DD`. With no `HOME` there are no file logs. |
| Log dir test | `crates/ressac/tests/cli.rs:1287-1300` (`ticker_survives_an_unwritable_log_directory`, `#[cfg(unix)]`, chmod 0500) | When run as root, the chmod has no effect and the test passes trivially. |
| `terminal.json` (look + images switch) | `crates/ressac/src/tui/reader/settings.rs:1-3` (doc), `:46-49` (`default_path` = DB path with file name `terminal.json`) | Called from `crates/ressac/src/commands/mod.rs:85`. |
| Its test | `crates/ressac/src/tui/reader/settings.rs:106-113` (`the_default_path_sits_next_to_the_database`) | Must become per-OS. |
| `ticker.json`, `ticker.json.tmp`, `ticker.json.lock` | `crates/ressac/src/ticker/state_file.rs:1-2` (doc), `:14-16` (`default_path`), `:26-32` (tmp + rename), `:34-54` (lock) | |
| Ticker state ignores `--db` | `crates/ressac/src/main.rs:107` (`ticker::state_file::default_path()`) | **Existing bug, Mac included.** `TickerState` holds entry ids (`ticker/pick.rs:42-48`), which only make sense for one DB. A ticker on `--db other.sqlite` shares the rotation of the default DB. |
| Terminal settings ignore `--db` | `crates/ressac/src/commands/mod.rs:85` | Harmless (these are preferences), but the files are not grouped consistently. |
| Ticker "state dir missing = Ressac never ran" | `crates/ressac/src/commands/ticker.rs:68-79`; `state_file.rs:34-43` never creates the directory | Today the DB directory doubles as the state directory. If state moves to its own XDG dir, **nothing creates it** and the ticker stays empty forever. |
| Migration backups `ressac.sqlite.bak-vN` | `crates/ressac-core/src/store/mod.rs:232-245` | Written next to the DB, so they follow it. Portable. |
| Swift side (stays as is) | `apps/macos/Ressac/Data/Database.swift:22-28`, `apps/macos/Ressac/Settings/SettingsView.swift:11` | macOS only. |
| Docs | `README.md:3` ("pour macOS"), `:44`, `:85`, `:87`, `:104`, `:106`; `docs/specs/*` | |

### 1.2 Strings and docs that name the Mac app

- `crates/ressac/src/tui/reader/jobs.rs:25-33`: `sentence()` turns "database is locked" into **"The database is busy (is Ressac.app writing?)"**. This is shown to the user in the TUI. Its tests are at `jobs.rs:290`, `tui/reader/mod.rs:1151` and `tui/reader/antiope.rs:2674`. Comments at `tui/reader/mod.rs:620`, `:868` and `store/library.rs:625-626` also name it.
- `crates/ressac-core/src/store/library.rs:2` and `crates/ressac-core/tests/contract.rs:1` describe the schema as "the contract with the SwiftUI app". This is a comment, not a problem.

### 1.3 Opening URLs (`open` crate 5.4.4)

- `crates/ressac/src/main.rs:120-127` (`open_in_browser`, used by `ticker open`, with the `RESSAC_NO_BROWSER=1` test hook).
- `crates/ressac/src/tui/mod.rs:42` (garden TUI) and `crates/ressac/src/tui/reader/mod.rs:311` (reader `OpenUrl`).
- On Linux, `open::that` tries `xdg-open` → `gio open` → `gnome-open` → `kde-open` (plus WSL detection). Stdio goes to `/dev/null`, so the TUI is not garbled. However, `that` **blocks until the launcher exits**. `kde-open` returns at once, but some `xdg-open` handlers run the browser in the foreground. That would freeze the TUI loop, and also tmux's `run-shell` for `ticker open`. Recommendation: use `open::that_detached`. Requires `xdg-utils` (installed on Fedora KDE).

### 1.4 tmux

- Ticker: `crates/ressac/src/cli.rs:115-125`, `crates/ressac/src/ticker/line.rs` (range markup, `#` escaping), `crates/ressac/src/commands/ticker.rs` (whole file). Plain tmux, nothing OS-specific. The README's `status-right` example points at `/path/to/Ressac.app/Contents/Helpers/ressac` (`README.md:98`); Linux needs its own snippet.
- Image detection under tmux: `crates/ressac/src/tui/reader/images.rs:194-207` (`picker.tmux_detected()`, `TMUX` env check), `:291-320` (`tmux_client_term`: `tmux display-message -p [-t $TMUX_PANE] '#{client_termname} #{client_termtype}'`, 500 ms timeout). Portable.

### 1.5 Terminal assumptions (Ghostty / Kitty / iTerm2 / Warp)

- `crates/ressac/src/tui/reader/images.rs:186-188`: if `TERM_PROGRAM=WarpTerminal`, Kitty is downgraded to iTerm2. Harmless on Linux.
- `crates/ressac/src/tui/reader/images.rs:253-283` (`decide_protocol`): under tmux, when nothing was detected, `ghostty|kitty` → Kitty, `iterm|wezterm` → iTerm2, anything else → half-blocks. Tests are at `:716-791`. **Konsole** (Fedora KDE's terminal; it supports Sixel, and Kitty graphics in recent versions) and **foot** are unknown here. Inside tmux on Konsole you get half-blocks unless you set `RESSAC_IMAGES=sixel`. Outside tmux, `Picker::from_query_stdio()` should detect Konsole by itself. Add a `konsole` mapping only after a manual check of what `client_termtype` reports.
- `README.md:83` lists Ghostty/iTerm2 only.

### 1.6 Clipboard, mouse, Cmd keys

None: there is no clipboard, OSC 52, mouse capture or Super/Cmd modifier anywhere in `crates/ressac/src`.

### 1.7 Signals and exit codes

- `crates/ressac/src/main.rs:54-65`: `tokio::signal::ctrl_c()`. The first SIGINT sets `stop`, the second calls `exit(130)` (`commands/mod.rs:27-28`). **SIGTERM is not handled** on either OS. The Mac app cancels with SIGINT (`process.interrupt()`, `RessacCLI.swift:229-233`, `:280-282`). A Qt front-end that drives the CLI with `QProcess::terminate()` would send SIGTERM and kill the process without the 130 path. SQLite rolls back, so it is safe, but the exit status differs. Either add a SIGTERM handler or document "send SIGINT".
- The TUI panic hook and the ratatui restore are platform-neutral.

### 1.8 File locking

- `crates/ressac/src/ticker/state_file.rs:38-54`: `std::fs::File::try_lock()`, which is `flock(2)` on both macOS and Linux. Portable on local filesystems.
- SQLite locking: see section 5. **Do not put the DB or the state dir on a Parallels shared folder (`/media/psf/...`)**. WAL needs working shared memory (mmap of `-shm`) and POSIX locks, which shared or network filesystems do not reliably provide. Keep the VM's DB on the VM's own ext4/btrfs and never open the Mac's DB from the VM.

### 1.9 Symlink and permission tests

`crates/ressac/src/commands/chart.rs:211-262` (`#[cfg(unix)]`: `open_exclusive_never_follows_a_symlink`, a 0600-preserving write, `write_replacing_replaces_a_symlink_instead_of_following_it`) and `crates/ressac/tests/cli.rs:1287-1300`. All `cfg(unix)`, all run on Linux. These are the only `cfg` uses other than `cfg(test)` / `cfg(debug_assertions)`. There is no `target_os` anywhere.

### 1.10 Time zones

- Production uses `chrono::Local` (`crates/ressac/src/commands/edition.rs:62`, `:255`; `crates/ressac/src/tui/reader/mod.rs:83-104` `Clock<Tz = Local>`). On Linux `Local` reads `TZ`, then `/etc/localtime`. **A fresh Parallels VM is often left in UTC**, which shifts "Today" and the 04:00 edition boundary. Check with `timedatectl`. No SQL uses `localtime` or `'now'`: all day boundaries are computed in Rust.
- Tests use a hand-written `Paris` `TimeZone` (`crates/ressac-core/src/edition.rs:~480-510`) or `FixedOffset`, so they do not depend on the host zone.
- `today_start` (local midnight) lives in the CLI crate (`crates/ressac/src/tui/reader/mod.rs:63-76`), while `edition::day_start` is in core (`crates/ressac-core/src/edition.rs:164`). A GUI needs "Today", so `today_start` should move to core.

### 1.11 Build prerequisites on Fedora aarch64 (not code, but they will bite)

- `rust-version = "1.99"` (`Cargo.toml:8`). Use rustup; Fedora's packaged rustc may lag.
- `aws-lc-sys 0.45` (rustls crypto) and `libsqlite3-sys` (bundled) need `gcc gcc-c++`; keep `cmake` installed as a fallback for aws-lc.
- TLS uses `rustls-platform-verifier` 0.7, which reads the system trust store (`/etc/pki/tls`). Fine.

---

## 2. Proposed XDG mapping

### 2.1 Which file goes where

On macOS nothing changes. The Swift app hard-codes the same DB path and both must keep agreeing.

| File | macOS (unchanged) | Linux / other Unix | Why |
|---|---|---|---|
| `ressac.sqlite` (+ `-wal`, `-shm`, `.bak-vN`) | `~/Library/Application Support/Ressac/` | `$XDG_DATA_HOME/ressac/` (default `~/.local/share/ressac/`) | User data that cannot be rebuilt. |
| `terminal.json` (look, images) | same dir as DB | `$XDG_CONFIG_HOME/ressac/terminal.json` (default `~/.config/ressac/`) | User preference. |
| `ticker.json`, `.tmp`, `.lock` | same dir as DB | `$XDG_STATE_HOME/ressac/ticker.json` (default `~/.local/state/ressac/`) | Rotation state that can be thrown away. |
| Logs `ressac.log.YYYY-MM-DD` | `~/Library/Logs/Ressac/` | `$XDG_STATE_HOME/ressac/logs/` | The XDG spec puts logs and history in state. |
| Cache | none today (images are kept in memory only) | `$XDG_CACHE_HOME/ressac/` reserved for a later GUI image or thumbnail cache | |

Resolution rules:

- `RESSAC_DB` and `--db` keep their current priority over everything.
- An `XDG_*` value is used only if it is an absolute path. The spec says relative values must be ignored.
- If `HOME` is missing too, return an error instead of the current silent relative path.
- An optional `RESSAC_STATE_DIR` override is useful for tests and for running two ressac setups side by side. Do not add more variables than that.

**Ticker state is per DB.** For the default DB use `state_dir/ticker.json`. For any other `--db`, use `<db dir>/<db file stem>.ticker.json`, or `state_dir/ticker-<short hash of canonical db path>.json`. This fixes the existing bug on macOS as well.

**Directory creation.**

- `Store::open` already creates the data dir.
- Logging should create the `logs` dir (it does today).
- The ticker should create the state dir only when the DB exists. Keep the "no DB → empty line, nothing created" behaviour: the test at `tests/cli.rs:1262-1270` asserts that no DB is created, and should also assert that no state dir is created.

**Legacy detection.** If a Linux user already ran ressac before this change, `~/Library/Application Support/Ressac/ressac.sqlite` exists on Linux. Log a one-time warning naming the old file (or have `ressac migrate` offer to move it). Do not move it silently.

### 2.2 Single source of truth: `ressac_core::paths`

Put it in core. The GUI needs the DB path, the log dir and (probably) the config dir, without depending on the CLI crate.

```rust
// crates/ressac-core/src/paths.rs
pub struct Paths { data: PathBuf, config: PathBuf, state: PathBuf, logs: PathBuf, cache: PathBuf }
impl Paths {
    pub fn from_env() -> Result<Paths>;                        // cfg!(target_os = "macos") picks the layout
    pub(crate) fn resolve(macos: bool, env: impl Fn(&str) -> Option<OsString>) -> Result<Paths>; // pure, testable for both OSes on any host
    pub fn database(&self) -> PathBuf;
    pub fn terminal_settings(&self) -> PathBuf;
    pub fn ticker_state(&self, db: &Path) -> PathBuf;          // per-DB rule above
    pub fn log_dir(&self) -> PathBuf;
    pub fn cache_dir(&self) -> PathBuf;
}
```

Write it by hand (about 80 lines plus tests) instead of using `dirs` or `directories`. Those crates map config on macOS to `~/Library/Application Support` under a bundle-id-style name (e.g. `org.x.Ressac`), which does not match the existing paths.

Keep `store::default_path()` as a thin wrapper over `Paths::from_env()?.database()`, or deprecate it.

### 2.3 Call sites that must change

1. `crates/ressac-core/src/lib.rs`: add `pub mod paths;`.
2. `crates/ressac-core/src/store/mod.rs:88-94`: `default_path` delegates to `Paths`. Make the test at `:358-365` per-OS, and add `resolve()` tests for both layouts.
3. `crates/ressac/src/main.rs:49-52`: default DB (unchanged if the wrapper stays). `:107`: ticker state becomes `paths.ticker_state(&db)`. `:129-141`: `init_logging` uses `paths.log_dir()`, and the doc comment at `:129` changes.
4. `crates/ressac/src/cli.rs:17`: the help text becomes per-OS via `#[cfg_attr(target_os = "macos", doc = "…~/Library/…")]` / `#[cfg_attr(not(target_os = "macos"), doc = "… [default: $XDG_DATA_HOME/ressac/ressac.sqlite]")]`. `cfg_attr` is expanded before clap's derive reads the docs.
5. `crates/ressac/src/tui/reader/settings.rs:1-3, 46-49, 106-113`: path from `Paths::terminal_settings()`; the test becomes per-OS.
6. `crates/ressac/src/commands/mod.rs:85`: caller (unchanged if `TerminalSettings::default_path` delegates).
7. `crates/ressac/src/ticker/state_file.rs:1-2, 14-16`: takes the DB path. Create the dir in `commands/ticker.rs:68-79` only when the DB exists.
8. `crates/ressac/tests/cli.rs`: the `ressac()` helper at `:18-26` and the direct `Command::new` calls at `:195-201`, `:1275-1280` and `:1677-1682` must `env_remove("XDG_DATA_HOME"/"XDG_CONFIG_HOME"/"XDG_STATE_HOME"/"XDG_CACHE_HOME")`, or point them at the tempdir. Otherwise a developer shell's real XDG dirs get logs written into them. The log test at `:1287-1300` becomes per-OS (`Library/Logs/Ressac` vs `.local/state/ressac/logs`).
9. `crates/ressac/src/tui/reader/jobs.rs:29`: replace "is Ressac.app writing?" with something like "is another Ressac writing?". Update the tests at `jobs.rs:290`, `tui/reader/mod.rs:1151` and `tui/reader/antiope.rs:2674`.
10. `README.md:3, 44, 83, 85, 87, 98, 104, 106`: add a Linux paths section and a Linux tmux snippet.
11. Swift (`Database.swift:27`, `SettingsView.swift:11`) does not change. Add a contract note that the macOS layout is frozen.

The searches the CLAUDE.md rule requires found no string-literal, dynamic, or re-export references to `default_path` beyond the ones above.

---

## 3. The API a GUI would need from `ressac-core`, and whether it works in-process

### 3.1 How the CLI uses the core

The CLI is a **thin adapter**. Every command opens `ressac_core::service::Ressac` (`crates/ressac/src/commands/mod.rs:61-69`) and calls the matching service or `Store` method, then formats text or JSON (`output.rs`). The TUI reader is **already an in-process client of the same API**: it reads through `app.store()` on the UI thread and runs network jobs in background threads (`crates/ressac/src/tui/reader/jobs.rs:78-111`). That is exactly the model a Qt app needs.

### 3.2 Map from GUI feature to core call

**Reading** (sync, `Store`, one connection per thread):

| Need | Call |
|---|---|
| Sidebar | `Store::sources(today_start)` → `SourcesSnapshot` (`store/library.rs:314`) |
| Entry list | `entries(source, filter, cursor)` (`:387`), with `ListFilter`, `ListCursor`, `PAGE_SIZE` |
| Search | `search_entries` (`:501`); `fts_query` (`:276`) |
| Article | `entry_detail` (`:521`); `fulltext` (`store/reading.rs:182`); `plain_text` |
| Journal (morning edition) | `journal()` (`:455`); `Ressac::edition_today()` (`service.rs:901`) |
| Rhythm groups | `cadence_details` (`:555`) |
| Feeds and folders | `feeds`, `feed`, `feed_count`, `folders`, `feeds_in_folder`, `folder_paths` (`store/feeds.rs`) |
| Garden | `garden::load_rows`, `progress`, `safe_repairs` (`garden.rs:83-170`); `diagnosis`, `diagnoses` (`store/health.rs`); `latest_decisions` |
| Health summary | `Ressac::summary()` (`service.rs:923`) |
| Pages | `Ressac::pages()` (`:906`) |
| Chart | `Store::chart_snapshot(now)` (`store/chart_snapshot.rs:113`; `Serialize`, the same JSON the Mac app reads); `themes` |
| External changes | `Store::data_version()` (`store/library.rs:627`) |

**Writes** (sync):

| Need | Call |
|---|---|
| Read / star / mark all | `Store::set_read`, `set_starred`, `mark_all_read` (`store/library.rs:583-620`) |
| Full-text preference | `set_prefer_full_text` (`store/reading.rs:212`) |
| Garden | `garden::decide(store, id, decision, now)`, `garden::undo`, `garden::apply_safe_repairs` (`garden.rs:171-200`) |
| Rhythm override | `Ressac::set_cadence` (`service.rs:917`) |
| Page number | `Ressac::set_page` (`:911`) |
| Theme | `Store::resolve_theme` + `set_feed_theme` (`store/chart.rs:222-249`) |
| Move feed | `Store::resolve_folder` + `move_feed` (`store/feeds.rs:360, 452`) |
| Chart import / export | `import_chart`, `export_chart` (`store/chart.rs:249, 329`) |
| OPML import / export | `Ressac::import_opml(xml, dry_run)` (`service.rs:305`); `opml::export(store, cemetery)` (`opml.rs:200`) |
| Migrate | implicit in `Store::open` / `Ressac::open`, then `garden::recompute_triage` (as `commands/mod.rs:163-174` does) |

**Network** (async):

| Need | Call |
|---|---|
| Fetch | `Ressac::refresh(RefreshOptions{force, only}, &dyn EventSink, Arc<AtomicBool>)` (`service.rs:414`) |
| Health check | `refresh`, then `lookup_wayback` (`:725`), then `summary` |
| Add feed | `add_feed(addr, theme, folder, sink, stop)` → `AddedFeed` (`:316`) |
| Extract article | `extract_entry(id, sink)` → `ExtractOutcome` (`:777`) |
| Image | `fetch_image(url)` (`:798`; SSRF-guarded, 10 MB / 15 s limits) |
| Edition | `build_edition(opts, now, &Local, sink, stop)` → `EditionOutcome` (`:838`) |

### 3.3 Verdict: usable in-process today, with known constraints

**Threading.** `Ressac` owns a `rusqlite::Connection`, which is `Send` but **`!Sync`**. Its `async fn(&self, …, sink: &dyn EventSink, …)` futures are therefore **`!Send`**: `EventSink` has no `Sync` bound. They cannot go on a multi-thread tokio `spawn`. Use the TUI pattern:

- one worker thread per job;
- its own `Ressac::open(db)` (its own connection);
- a `new_current_thread().enable_all()` runtime with `block_on`;
- results sent back over a channel.

With cxx-qt, hand results to Qt via `qt_thread().queue(...)`. Keep one `Store` on the GUI thread for reads.

Optional cleanup: make it `trait EventSink: Send + Sync` (every existing sink already is: `output::Sink`, `ProgressSink`, `NullSink`).

**Tokio.** Tokio is a hard dependency of core (`features = ["full"]`). `refresh` calls `tokio::spawn` (`service.rs:450`, `:547`) and `source.rs` uses `JoinSet`, `Semaphore` and `lookup_host`, so a tokio runtime must be current. A current-thread runtime is enough.

**Progress.** `EventSink::emit(&Event)` is a synchronous callback on the job thread. `Event` is `Serialize` and tagged (`service.rs:66-118`). The variants are:

- `Started`, `FeedDone`, `NetworkUnreachable`, `Probing`, `WaybackDone`, `Finished`;
- `Extracted`, `Decided`, `Undone`, `Page`, `Edition`.

`added`, `cadence`, `theme` and `moved` are **not** `Event` variants: the CLI builds their JSON itself (`output.rs:238-279`, `commands/feed.rs:86, 116`). In-process they are simply return values.

**Cancellation.** It is cooperative, through `Arc<AtomicBool>` checked between feeds and lanes. A request already in flight finishes or times out. There is no hard abort; the CLI's second Ctrl-C is CLI-only.

**Concurrency with another process on the same DB.**

- WAL is set in `Store::configure` (`store/mod.rs:169-176`), with `busy_timeout` 5 s for writers and 250 ms for `open_existing_query_only` (`:120-158`).
- `write_tx()` uses `BEGIN IMMEDIATE` (`:251-259`), so there is no `SQLITE_BUSY_SNAPSHOT`.
- Migrations take the IMMEDIATE lock per version and re-read the version (`:197-229`), so two processes migrating at once are safe.
- `build_edition` handles "another process built it" (`service.rs:886-889`).
- Other processes' commits are detected by polling `PRAGMA data_version`: every 2 s while idle in the TUI (`tui/reader/mod.rs:115`, `:620-645`), and through `DataVersionWatcher` in the Mac app. A connection's own commits do not change its own `data_version`, so the GUI must reload after its own writes.
- **Nothing enforces "one long run at a time"** across processes. The Mac app (`RunKind` long/short) and the TUI ("long-job slot") both do it client-side, and the GUI must too. Two concurrent refreshes are correct but wasteful.

**Logic that lives in the CLI and should move to core for a GUI:**

- `check_decision` (unknown, buried or non-repairable feed: `commands/garden.rs:31-51`). Without it, a GUI could "repair" a feed that has no repair.
- `subset()` validation of `--feed` / `--folder` (`commands/network.rs:28-49`).
- The `open_with_feeds` first-run rule (`commands/mod.rs:90-101`).
- `today_start` (`tui/reader/mod.rs:63-76`).
- Log init (`main.rs:129-152`).
- The "refusal (exit 2) vs fatal" classification is spread over `commands/*::*_error`. The core's `Error` enum is already typed, so the GUI can match on it directly.

**Schema gating.** `Ressac::open` always migrates, and backs up first. A GUI should first probe with `Store::open_existing_query_only`, which returns `SchemaMismatch` / `SchemaTooNew`, and ask before migrating, as the Mac app does (`knownSchemaVersion = 6`).

**Serde on read models.** `ChartSnapshot`, `EditionView`, `PageRow` and `Summary` derive `Serialize`. `EntryRow`, `SourcesSnapshot`, `EntryDetail`, `JournalView`, `GardenRow`, `FeedRow` and `FolderRow` do not. That is fine for a cxx-qt bridge mapping fields by hand. Add `Serialize` only if you want to pass JSON into QML.

**Same SQLite library only.** The GUI must **not** open the DB with QtSql or a system libsqlite in the same process as ressac-core. Two SQLite copies in one process break POSIX-lock bookkeeping: closing a descriptor in one copy drops the other copy's locks, which risks corruption. All DB access in the Qt app should go through ressac-core (bundled SQLite 3 with FTS5).

**Chart rendering.** The chart page (`apps/macos/Ressac/Resources/Chart/{index.html, chart-core.js, chart-draw.js, d3.min.js, chart.css}`, tested by `node --test apps/macos/ChartTests/*.test.cjs`) is plain HTML/JS fed the `ChartSnapshot` JSON. It can be reused unchanged in a QtWebEngine view.

**Crate packaging.** Migrations are `include_str!("../../../../schema/…")` (`store/mod.rs:45-55`), so ressac-core cannot be published to crates.io as it is. It works as a path dependency (Qt app inside the workspace, e.g. `apps/linux/`) or as a git dependency.

### 3.4 The CLI/JSON contract the Mac app relies on

From `apps/macos/Ressac/Engine/RessacCLI.swift` and `Events.swift`.

**Invocation**

- The app runs `ressac --db <path> --json <args>` with stdin `/dev/null` (`RessacCLI.swift:237-257`). The binary is `Ressac.app/Contents/Helpers/ressac`.
- The DB path comes from `RESSAC_DB`, else the default path (`Database.swift:22-28`).

**Output**

- stdout is NDJSON, one object per line, each with an `"event"` tag in snake_case. Lines are read as they arrive, using a custom line splitter that copes with UTF-8 split across reads.
- A line that is not JSON is logged and skipped.
- A JSON object with an unknown tag becomes `.unknown(raw)`. `import --json` relies on this: it prints a report object without an `event` key.

**Events the app decodes** (`Events.swift:33-110`):

| Event | Fields |
|---|---|
| `started` | `total` |
| `feed_done` | `feed_id`, `title`, `error?` |
| `network_unreachable` | `failed`, `attempted` |
| `probing` | `what`, `total` |
| `wayback_done` | `found` |
| `finished` | `summary?` (`total`, `counts{Status→n}`, `https_repairs`, `redirect_repairs`, `network_unreachable`) |
| `extracted` | `entry_id`, `ok`, `error?` |
| `decided` | `feed_id`, `decision` |
| `undone` | `feed_id?` |
| `edition` | `id`, `entries`, `overflow`, `unreachable` (the CLI also sends `front`/`week`/`wire`; ignored) |
| `cadence` | `feed_id`, `group`, `override` (the CLI also sends `computed`) |
| `added` | `id`, `title`, `theme?`, `like?`, `folder?` (the CLI also sends `xml_url`, `page`, `territory`) |

`apply-safe` prints the `finished` event with extra `applied` / `skipped` keys. `feed theme` and `feed move` print `theme` / `moved` objects, which the app treats as unknown.

**Commands the app issues** (`RessacCLI.swift:64-136`)

| Kind | Commands |
|---|---|
| Long (exclusive among themselves) | `fetch --force` / `fetch --feed N…` / `fetch --folder N`; `import <file>`; `edition build [--rebuild]`; `add <addr> [--theme K] [--folder N]` |
| Short | `import --dry-run`, `export --output F [--cemetery]`, `extract N`, `garden decide N keep\|repair\|bury`, `garden undo`, `garden apply-safe`, `migrate`, `feed cadence N <group\|auto>`, `feed theme N <key\|none>`, `feed move N <id\|root>` |
| Whole-stdout capture | `chart`, which returns the `ChartSnapshot` JSON document |

Short runs may overlap a long run and are serialised among themselves.

**Exit codes**

- `0`: success.
- `2`: a refusal, but only for `garden decide`, `garden undo`, `edition build`, `feed cadence`, `feed theme`, `feed move` and `add`. Any other `2` is treated as an app bug.
- `130`: interrupted.
- `1`: fatal.
- `127`: the app's own code for "could not launch".
- A negative value: death by signal.

The banner shows the last non-empty stderr line.

**Cancellation.** The app sends SIGINT.

**Direct database access.** The app opens the DB itself through GRDB (system SQLite, separate process) for every read and for the `read_at` / `starred_at` / `prefer_full_text` writes (`Data/Writes.swift`). It notices external commits through `data_version`. It never migrates itself: it runs `ressac migrate` when `schema_meta.version < 6`.

---

## 4. Release and CI

Current state:

- `ci.yml` has a `check` job on `ubuntu-latest` (x86_64): fmt, `clippy -D warnings`, `cargo test --workspace`, and the Node chart tests. It also has a `macos` job on a custom `xcode-27` runner that may fail without blocking.
- `release.yml` is a single job on `xcode-27`. It checks that the tag, `Cargo.toml` `workspace.package.version` and `project.yml` `MARKETING_VERSION` match. It then builds `Ressac.app` (arm64), extracts the `ressac` helper into `ressac-$V-macos-arm64.tar.gz`, writes `SHA256SUMS` with `shasum`, and runs `gh release create` with Mac-only notes (quarantine instructions).

### 4.1 CI additions

1. **`linux-arm64` job** on `ubuntu-24.04-arm` (GitHub's hosted arm64 Linux runner; check that it is available to this repo's plan). Same steps as `check`. This catches aarch64-only issues (aws-lc build, `c_char` signedness, etc.). Optionally make `check` a matrix over `ubuntu-latest` / `ubuntu-24.04-arm`.
2. **`fedora` job**: `container: fedora:<current>` with `dnf install -y gcc gcc-c++ cmake tmux xdg-utils`, rustup, then `cargo test --workspace`. This matches the target distro (glibc, CA store paths). Run it once with `XDG_*` set to a tempdir and once without, to prove the tests are hermetic.
3. **`qt` job**, once the Qt app exists: Fedora container with `qt6-qtbase-devel qt6-qtdeclarative-devel kf6-kirigami-devel kf6-qqc2-desktop-style extra-cmake-modules cmake ninja-build gcc-c++` (plus `qt6-qtwebengine-devel` for the chart). Build with cxx-qt (cargo-driven) or CMake + Corrosion. Test with `QT_QPA_PLATFORM=offscreen` (`cargo test -p ressac-qt` and/or `ctest`, plus QML tests via `qmltestrunner`). Keep it allowed to fail until it stabilises, like the `macos` job.

### 4.2 Release additions

Restructure into four jobs:

1. **`version`** (ubuntu): the current tag/version check. Also check the Qt app's version source (e.g. a CMake `project(VERSION)`, or reuse the Cargo workspace version).
2. **`macos`**: today's build steps, uploading the artifacts with `actions/upload-artifact` instead of publishing.
3. **`linux`** matrix `{x86_64: ubuntu-22.04, aarch64: ubuntu-22.04-arm or ubuntu-24.04-arm}`:
   - run `cargo build --release --locked -p ressac`, then smoke-test with `./target/release/ressac --version` and `ressac --db $(mktemp -d)/t.sqlite migrate --json`;
   - produce `tar -czf ressac-$V-linux-<arch>.tar.gz`.

   Build on the **oldest** Ubuntu that is practical to keep the glibc floor low (22.04 → glibc 2.35; Fedora 41+ ships far newer). A musl static build is possible but aws-lc-sys + bundled SQLite under musl adds friction. Not worth it for one Fedora VM.
4. **`publish`** (needs all): download the artifacts, compute one `SHA256SUMS` with `sha256sum` (Linux runner), and run `gh release create`. Add a Linux section to the notes: no quarantine; it needs `xdg-utils` for opening links and the paths from section 2. Later, add the Qt app as a tarball, Flatpak or RPM; decide when the GUI exists.

### 4.3 Local gate while Actions has no credits (until 2026-11-02)

Add a `scripts/check-linux.sh`, run in the Fedora VM:

```sh
set -euo pipefail
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# hermetic: a clean HOME and no XDG dirs
env -i PATH="$PATH" HOME="$(mktemp -d)" cargo test --workspace
# smoke test of the default layout
H=$(mktemp -d); HOME=$H ./target/debug/ressac import tests/fixtures/vienna-like.opml && test -f "$H/.local/share/ressac/ressac.sqlite"
node --test 'apps/macos/ChartTests/*.test.cjs'   # if node is installed
```

- Keep the Mac's `cargo test` and `xcodebuild test` as the macOS gate.
- `act` can replay `ci.yml`'s Ubuntu jobs locally under Podman/Docker if wanted. It cannot replay `xcode-27`.

---

## 5. Schema and migration

**Embedding.** Migrations are embedded at compile time from `schema/0001_init.sql` … `0006_title_entities.sql` (`crates/ressac-core/src/store/mod.rs:41-55`; `SCHEMA_VERSION = 6`). The version is stored in `schema_meta.version` (`read_version`, `:71-81`).

**Open paths**

| Function | Behaviour |
|---|---|
| `Store::open(path)` (`:97-109`) | `create_dir_all(parent)` (default permissions, umask-dependent), then `Connection::open`, `configure` (WAL, `foreign_keys`, `busy_timeout` 5 s), then `migrate`. |
| `open_existing` / `open_existing_query_only` (`:113-157`) | Never create, migrate or back up. They fail with `DatabaseMissing` / `SchemaMismatch`. The query-only variant sets `PRAGMA query_only` and a 250 ms busy timeout, and leaves `journal_mode` alone (tests at `:994-1047`). |
| `open_in_memory` | Used for dry runs. |

**`migrate()`** (`:182-230`)

- It refuses DBs that are too new.
- If any step is pending and `current > 0`, it first runs `backup(current)`: `VACUUM INTO '<db>.sqlite.bak-v<current>'`, keeping a backup that already exists (`:232-245`).
- Each version runs in its own `BEGIN IMMEDIATE` and re-reads the version under the lock.
- Data fix-ups are hooked to specific versions: v2 FTS backfill, v3 page assignment, v4 reserving pages 104-109 and backfilling rhythm groups, v6 decoding HTML entities in titles.

**`ressac migrate`** (`crates/ressac/src/commands/mod.rs:162-174`) opens the DB, which migrates it, then runs `garden::recompute_triage` and prints a `finished` event. The Mac app calls it when its schema check says `needsMigration`.

**Contract test.** `crates/ressac-core/tests/contract.rs` checks the schema the Swift app reads, including the views `v_entry`, `v_unread_count`, `v_garden_row`, `v_folder_path`, `v_feed_overview`, `v_page`, `v_cadence`, `v_edition_today` and `v_edition_overflow`.

**Platform-specific points.** Almost none; this part is portable.

- SQLite is bundled (`rusqlite` `bundled`), so every platform gets the same version with FTS5. There are no time functions in SQL.
- `VACUUM INTO` gets the path through `to_string_lossy()` (`:242`). A Linux path that is not valid UTF-8 would send the backup to a mangled name. This is an edge case; decide whether to reject such DB paths.
- WAL needs a local filesystem with working mmap and POSIX locks, so **not** `/media/psf` (Parallels shared folders), NFS or SMB.
- The DB directory is created with default permissions (0755 under the usual umask). Fedora home directories are 0700, so this is fine. Consider 0700 for `~/.local/share/ressac` anyway.
- The Linux DB is new and separate, so it starts at v6 directly with no backup. No cross-platform DB migration is needed.

---

## 6. Sub-project 1, "Linux base": the change list, sized

The CLAUDE.md rule caps each phase at 5 files.

| # | Change | Files | Size |
|---|---|---|---|
| P1 | `ressac_core::paths` (macOS layout + XDG, pure `resolve()` with tests for both); `default_path` delegates; `main.rs` logging dir; `cli.rs` per-OS `--db` help | `ressac-core/src/paths.rs` (new), `ressac-core/src/lib.rs`, `ressac-core/src/store/mod.rs`, `ressac/src/main.rs`, `ressac/src/cli.rs` | M (~150 LOC + ~10 tests) |
| P2 | `terminal.json` → config dir; ticker state per DB + state-dir creation only when the DB exists; hermetic `XDG_*` handling in the CLI tests; per-OS log-dir test | `tui/reader/settings.rs`, `ticker/state_file.rs`, `commands/ticker.rs`, `main.rs:107` (or fold into P1), `tests/cli.rs` | S-M |
| P3 | Neutral "database is busy" sentence (+ 3 tests); `open::that_detached`; one-time warning for a legacy `~/Library` DB on Linux | `tui/reader/jobs.rs`, `tui/reader/mod.rs`, `tui/reader/antiope.rs`, `tui/mod.rs`, `main.rs` | S |
| P4 | README Linux section (paths, tmux snippet with the Linux binary path, Konsole images / `RESSAC_IMAGES`, TZ and shared-folder warnings) | `README.md` | S |
| P5 | `scripts/check-linux.sh`; CI `linux-arm64` + `fedora` jobs; split `release.yml` into version / macos / linux matrix / publish | `scripts/check-linux.sh`, `.github/workflows/ci.yml`, `.github/workflows/release.yml` | M (release split) |
| P6 (GUI prep, optional for "base") | Move `check_decision`, feed-subset validation and `today_start` into core; `EventSink: Send + Sync`; a core log-init helper; optional `Serialize` on read models | `ressac-core/src/garden.rs`, `service.rs`, `edition.rs`, `ressac/src/commands/garden.rs`, `commands/network.rs` | S-M |
| P7 (after a manual test in Konsole + tmux) | `decide_protocol`: map `konsole` (and `foot`) | `tui/reader/images.rs` | XS |

Overall, P1-P5 is about 2-3 focused days. No changes are needed in the schema, the Swift app or the core's network or storage logic.
