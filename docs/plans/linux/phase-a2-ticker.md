# Phase A2: ticker state per database, `terminal.json` in the config directory, hermetic CLI tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the CLI takes its per-file locations from `ressac_core::paths`: one ticker state per database (fixing the Mac bug where `--db other.sqlite` shared the default rotation), the state directory created only when the database exists, `terminal.json` in the config directory; and the CLI tests never see the developer's `XDG_*`, `RESSAC_DB` or `RUST_LOG`.

**Runs after:** A1 (`ressac_core::paths` merged). Master plan `docs/plans/2026-10-23-linux.md` (Interfaces, Global Constraints, Review Focus 2); spec §5.1 ("Ticker state per database", "creates its state directory only when the database exists", "CLI tests clear the developer's `XDG_*`"); audit §1.1 and §2.1 "Directory creation".

**Files (4 counted):**

| File | Task | Responsibility |
|---|---|---|
| `crates/ressac/tests/cli.rs` (test file, not counted) | 1, 2, 3 | `hermetic()` helper; per-database ticker tests; Review Focus 2 |
| `crates/ressac/src/main.rs:49-52, 103-114` | 2 | `run(cli, paths)`; the ticker's state path is `paths.ticker_state(&g.db)` |
| `crates/ressac/src/commands/ticker.rs:61-80` | 2 | missing database → nothing touched; else create the state directory, then lock |
| `crates/ressac/src/ticker/state_file.rs:1-2, 14-16` | 2 | `default_path` removed; module doc |
| `crates/ressac/src/tui/reader/settings.rs:1-3, 46-49, 106-113` | 3 | `TerminalSettings::default_path` delegates to `Paths::terminal_settings` |

**Interfaces consumed (A1):** `ressac_core::paths::{Paths, PathsError}`, `Paths::from_env() -> Result<Paths, PathsError>`, `Paths::ticker_state(&self, db: &Path) -> PathBuf` (state/`ticker.json` for the default database under any spelling, else state/`ticker-<16 lowercase hex>.json`), `Paths::terminal_settings(&self) -> PathBuf`, `store::default_path() -> PathBuf`. `main` already holds `let paths = Paths::from_env().ok();`.

**Interfaces produced:** `async fn run(cli: Cli, paths: Option<&Paths>) -> anyhow::Result<ExitCode>` in `main.rs` (A3 adds the legacy warning there); `TerminalSettings::default_path() -> PathBuf` unchanged in signature; `ticker::state_file::default_path` deleted (its only caller is `main.rs:107`; searched: no string, re-export or test reference). In `tests/cli.rs`: `fn hermetic(home: &Path) -> Command`, `fn state_dir(home: &Path) -> PathBuf`, `fn ticker_files(dir: &Path) -> Vec<String>`, used again by A3.

**Deviation from the master plan:** Task 3 (`terminal.json`) is listed under A1 in the phase table; it moved here to keep A1 at 5 files (see A1's header).

**Behaviour changes, on purpose:**
- macOS: a non-default `--db` now gets `~/Library/Application Support/Ressac/ticker-<hash>.json` instead of sharing `ticker.json` (the bug fix). The default database keeps `ticker.json`, byte-identical.
- The unit test `show_in_a_missing_directory_creates_nothing` (database present, state directory missing) asserted the old rule "a missing directory means Ressac never ran". The spec reverses it: the directory is now created when the database exists. The test is replaced by the three tests of Task 2; "nothing created" now keys on the missing database. On Linux this matters because logging creates `$XDG_STATE_HOME/ressac/logs`, so the state directory exists after any run.

**macOS stays byte-identical** (default paths): `paths::tests::macos_layout_matches_the_frozen_paths` (A1), `settings::tests::the_default_path_sits_next_to_the_database` (kept, `#[cfg(target_os = "macos")]`), and the new CLI test `the_default_database_keeps_ticker_json`.

**Step 0:** not triggered. `commands/ticker.rs` (433 lines) gains a guard at the top of `pick`; `tests/cli.rs` gains a helper and its call sites change one line each. No restructuring.

**Gate (end of phase, Mac only before A4):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
```

---

### Task 1: CLI tests isolated from the developer's environment

**Files:**
- Test: `crates/ressac/tests/cli.rs` (helper; `ressac()` at `:18-26`, the `export` child at `:195-201`, `ticker_open_is_silent` at `:1275-1280`, `ressac_guarded` at `:1677-1682`)

**Interfaces:**
- Consumes: nothing.
- Produces: `fn hermetic(home: &Path) -> Command`: `Command::new(env!("CARGO_BIN_EXE_ressac"))` with `HOME` = `home` and `env_remove` of `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`, `RESSAC_DB`, `RUST_LOG`. It sets no `--db` (clap refuses `--db` twice; callers add it).

- [ ] **Step 1: Write the failing test**

```rust
const SCRUBBED: [&str; 6] = [
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "RESSAC_DB",
    "RUST_LOG",
];

#[test]
fn every_run_drops_the_developer_s_xdg_variables() {
    let home = tempfile::tempdir().unwrap();
    let cmd = hermetic(home.path());
    let envs: HashMap<_, _> = cmd.get_envs().collect();
    assert_eq!(envs.get(std::ffi::OsStr::new("HOME")), Some(&Some(home.path().as_os_str())));
    for var in SCRUBBED {
        // `get_envs` reports a removed variable as `None`.
        assert_eq!(envs.get(std::ffi::OsStr::new(var)), Some(&None), "{var}");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p ressac --test cli every_run_drops`
Expected: compile error `cannot find function hermetic`.

- [ ] **Step 3: Implement `hermetic`** and build the four call sites on it (`hermetic(home).args(["--db", …])…`); `ressac()` keeps `RESSAC_ALLOW_PRIVATE_ARTICLES=1`, `ressac_guarded` keeps its `env_remove` of it.

- [ ] **Step 4: Run the whole CLI suite with a hostile environment**

Run: `XDG_DATA_HOME=/tmp/ressac-xdg-leak XDG_STATE_HOME=/tmp/ressac-xdg-leak RESSAC_DB=/tmp/ressac-xdg-leak/x.sqlite cargo test -p ressac --test cli; ls /tmp/ressac-xdg-leak`
Expected: `test result: ok.`, then `ls: /tmp/ressac-xdg-leak: No such file or directory`.
Check: `grep -n 'Command::new(env!("CARGO_BIN_EXE_ressac"))' crates/ressac/tests/cli.rs` prints one line (inside `hermetic`).

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/tests/cli.rs
git commit -m "CLI tests: drop the developer's XDG_*, RESSAC_DB and RUST_LOG

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: one ticker state per database, its directory created only when the database exists

**Files:**
- Modify: `crates/ressac/src/main.rs:20-24` (`main` passes `paths.as_ref()`), `:48-52` (`run` signature), `:103-114` (the `Ticker` arm)
- Modify: `crates/ressac/src/commands/ticker.rs:61-80` (`pick`), tests at `:257-263`
- Modify: `crates/ressac/src/ticker/state_file.rs:1-2` (doc), `:14-16` (delete `default_path`)
- Test: `crates/ressac/src/commands/ticker.rs` (inline), `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: `Paths::ticker_state` (A1); Task 1's `hermetic`.
- Produces: `run(cli: Cli, paths: Option<&Paths>)`. The `Ticker { action: None }` arm calls `commands::ticker::show(&g, &p.ticker_state(&g.db), …)` when `paths` is `Some(p)`; when `None` (no usable `HOME`), it logs at `debug` and prints the empty line (`writeln!(out)?`, exit 0), as every other ticker failure does. `show`'s and `state_file::{load, save, lock}`'s signatures do not change.

New order at the top of `pick` (algorithm, the signature does not say it):
1. `!g.db.exists()` → `tracing::debug!` and `return Ok(String::new())`: no directory, no lock file (the database is missing: nothing to show, nothing to create).
2. `std::fs::create_dir_all(state_path.parent())`; on error, `tracing::warn!` and return the empty line.
3. The existing lock / open / pick / save sequence, unchanged.

- [ ] **Step 1: Write the failing tests**

In `commands/ticker.rs`, delete `show_in_a_missing_directory_creates_nothing` and add:

```rust
    #[test]
    fn show_creates_the_state_directory_when_the_database_exists() {
        let mut f = fixture();
        f.state = f.dir.path().join("state/ressac/ticker.json");
        assert_ne!(show_at(&f, NOW), "\n");
        assert!(f.state.exists());
    }

    #[test]
    fn show_on_a_missing_database_creates_nothing() {
        let mut f = fixture();
        f.g.db = f.dir.path().join("missing.sqlite");
        f.state = f.dir.path().join("state/ticker.json");
        assert_eq!(show_at(&f, NOW), "\n");
        assert!(!f.dir.path().join("state").exists());
        assert!(!f.g.db.exists());
    }

    #[test]
    fn show_on_a_missing_database_leaves_an_existing_state_directory_empty() {
        // Linux: logging has created `$XDG_STATE_HOME/ressac/logs`, so the directory is there.
        let mut f = fixture();
        f.g.db = f.dir.path().join("missing.sqlite");
        let state = f.dir.path().join("state");
        std::fs::create_dir_all(state.join("logs")).unwrap();
        f.state = state.join("ticker.json");
        assert_eq!(show_at(&f, NOW), "\n");
        let names: Vec<_> = std::fs::read_dir(&state).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("logs")]);
    }
```

In `tests/cli.rs`:

```rust
/// Where the layout keeps the ticker state for `home` (macOS: beside the default database).
fn state_dir(home: &Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Ressac")
    } else {
        home.join(".local/state/ressac")
    }
}

/// The layout's default database for `home`.
fn default_db(home: &Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Ressac/ressac.sqlite")
    } else {
        home.join(".local/share/ressac/ressac.sqlite")
    }
}

/// The names starting with `ticker` in `dir`, sorted (none when `dir` is missing).
fn ticker_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("ticker"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn is_hashed_ticker_name(name: &str) -> bool {
    name.strip_prefix("ticker-")
        .and_then(|rest| rest.strip_suffix(".json"))
        .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

#[test]
fn the_default_database_keeps_ticker_json() {
    let home = tempfile::tempdir().unwrap();
    assert!(hermetic(home.path()).args(["import", FIXTURE]).output().unwrap().status.success());
    assert!(default_db(home.path()).exists());
    let out = hermetic(home.path()).args(["ticker", "--tmux"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(ticker_files(&state_dir(home.path())), ["ticker.json", "ticker.json.lock"]);
}

#[test]
fn each_database_gets_its_own_ticker_state() {
    let home = tempfile::tempdir().unwrap();
    for db in ["one.sqlite", "two.sqlite"] {
        let db = home.path().join(db);
        let db = db.to_str().unwrap();
        assert!(hermetic(home.path()).args(["--db", db, "import", FIXTURE]).output().unwrap().status.success());
        assert!(hermetic(home.path()).args(["--db", db, "ticker", "--tmux"]).output().unwrap().status.success());
    }
    let states: Vec<String> = ticker_files(&state_dir(home.path()))
        .into_iter()
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert_eq!(states.len(), 2, "{states:?}");
    assert!(states.iter().all(|n| is_hashed_ticker_name(n)), "{states:?}");
}

#[cfg(unix)]
#[test]
fn one_database_named_four_ways_shares_one_ticker_state() {
    // Review Focus 2: `./r.sqlite` from its folder, the absolute path, a `..` detour, a symlink.
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("dbs");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let db = dir.join("r.sqlite");
    assert!(hermetic(home.path()).args(["--db", db.to_str().unwrap(), "import", FIXTURE]).output().unwrap().status.success());
    std::os::unix::fs::symlink(&db, dir.join("link.sqlite")).unwrap();
    for spelling in [
        "./r.sqlite".to_string(),
        db.to_str().unwrap().to_string(),
        dir.join("sub/../r.sqlite").to_str().unwrap().to_string(),
        dir.join("link.sqlite").to_str().unwrap().to_string(),
    ] {
        let out = hermetic(home.path())
            .current_dir(&dir)
            .args(["--db", spelling.as_str(), "ticker", "--tmux"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{spelling}: {out:?}");
    }
    let files = ticker_files(&state_dir(home.path()));
    assert_eq!(files.len(), 2, "one state and its lock: {files:?}");
    assert!(is_hashed_ticker_name(&files[0]), "{files:?}");
    assert_eq!(files[1], format!("{}.lock", files[0]));
}
```

and extend `ticker_on_a_missing_database_prints_an_empty_line` (`:1263-1270`) with:

```rust
    assert!(ticker_files(&state_dir(home.path())).is_empty());
    #[cfg(target_os = "macos")]
    assert!(!state_dir(home.path()).exists());
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac --bin ressac commands::ticker::tests && cargo test -p ressac --test cli -- ticker each_database one_database the_default_database`
Expected: `show_creates_the_state_directory_when_the_database_exists` FAILS (empty line, no directory); `show_on_a_missing_database_leaves_an_existing_state_directory_empty` FAILS (`ticker.json.lock` created); `each_database_gets_its_own_ticker_state` and `one_database_named_four_ways_shares_one_ticker_state` FAIL (state directory never created for a non-default database, so `ticker_files` is empty).

- [ ] **Step 3: Implement** `run(cli, paths)`, the `Ticker` arm, `pick`'s new order and the removal of `state_file::default_path`, as in the Interfaces block. `state_file.rs`'s module doc becomes: "The ticker's state on disk (`Paths::ticker_state`: `ticker.json` for the default database, `ticker-<hash>.json` for any other, in the state directory) and the lock that serialises the `ressac ticker` processes tmux starts." The comment in `pick` about "Ressac never ran" goes; the lock's `NotFound` branch stays (a directory removed between steps 2 and 3).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac --bin ressac commands::ticker::tests && cargo test -p ressac --test cli`
Expected: `test result: ok.` for both.
Run: `grep -rn 'state_file::default_path' crates/`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/main.rs crates/ressac/src/commands/ticker.rs crates/ressac/src/ticker/state_file.rs crates/ressac/tests/cli.rs
git commit -m "Ticker: one state per database, state directory only when the database exists

Fixes the shared rotation of a second --db on the Mac too.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 3: `terminal.json` in the config directory

**Files:**
- Modify: `crates/ressac/src/tui/reader/settings.rs:1-3` (module doc), `:46-49` (`default_path`), `:106-113` (test)
- Test: inline in `settings.rs`

**Interfaces:**
- Consumes: `Paths::from_env`, `Paths::terminal_settings` (A1), `store::default_path`.
- Produces: `TerminalSettings::default_path() -> PathBuf` = `Paths::from_env()` → `terminal_settings()`; on `Err`, the old `store::default_path().with_file_name("terminal.json")` (that call already logs the error). Its caller `commands/mod.rs:85` does not change.

- [ ] **Step 1: Write the failing test.** Put `#[cfg(target_os = "macos")]` on `the_default_path_sits_next_to_the_database` (kept as is: the Mac proof) and add:

```rust
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_default_path_is_in_the_config_directory() {
        let path = TerminalSettings::default_path();
        assert!(path.ends_with("ressac/terminal.json"), "{path:?}");
        assert_ne!(path.parent(), ressac_core::store::default_path().parent(), "{path:?}");
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test -p ressac --bin ressac tui::reader::settings::tests`
Expected on the Mac: PASS (macOS unchanged); the Linux test fails until Step 3 and runs in the Linux gate from A4.

- [ ] **Step 3: Implement** `default_path` as in the Interfaces block. Module doc: "…in `terminal.json` (`Paths::terminal_settings`: next to the database on macOS, `$XDG_CONFIG_HOME/ressac/` on Linux)…"; the method's doc names both.

- [ ] **Step 4: Run the Mac gate** (the four commands of the header). Expected: as listed there.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/tui/reader/settings.rs
git commit -m "terminal.json from ressac_core::paths (config directory on Linux)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
