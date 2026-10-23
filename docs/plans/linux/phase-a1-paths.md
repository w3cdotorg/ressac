# Phase A1: `ressac_core::paths`, the one source of truth for file locations

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every file location Ressac uses comes from one pure resolver in `ressac-core`: macOS paths byte-identical, XDG on Linux; the default database, the log directory and the `--db` help go through it.

**Runs after:** nothing (first phase of sub-project 8). Read the master plan `docs/plans/2026-10-23-linux.md` first (Interfaces, Global Constraints, Review Focus) and the spec `docs/specs/2026-10-23-linux-design.md` §5.1. Call sites: `docs/specs/linux/portability-audit.md` §1.1 and §2.3 (re-verified at `cbe551a`: line numbers below are current).

**Files (5 counted):**

| File | Task | Responsibility |
|---|---|---|
| `crates/ressac-core/src/paths.rs` (new) | 1 | `Layout`, `Paths`, `PathsError`, the per-database ticker rule, the legacy check; unit tests inline |
| `crates/ressac-core/src/lib.rs` | 1 | `pub mod paths;` |
| `crates/ressac-core/src/store/mod.rs:88-94, 358-365` | 2 | `default_path` delegates; its test per OS |
| `crates/ressac/src/main.rs:20-24, 129-141` | 2 | `Paths::from_env()` once; `init_logging(dir, verbose)` |
| `crates/ressac/src/cli.rs:17` | 2 | `--db` help per OS |
| `crates/ressac/tests/cli.rs` (test file, not counted) | 2 | per-OS log directory, help text |

**Interfaces produced (master plan, names fixed):**

```rust
// crates/ressac-core/src/paths.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout { MacOs, Xdg }
impl Layout { pub fn current() -> Layout } // MacOs when cfg!(target_os = "macos"), else Xdg

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths { /* private: home, data, config, state, logs, cache: PathBuf */ }
impl Paths {
    pub fn from_env() -> Result<Paths, PathsError>; // resolve(Layout::current(), &|k| std::env::var_os(k))
    pub fn resolve(layout: Layout, env: &dyn Fn(&str) -> Option<OsString>) -> Result<Paths, PathsError>;
    pub fn database(&self) -> PathBuf;
    pub fn terminal_settings(&self) -> PathBuf;
    pub fn ticker_state(&self, db: &Path) -> PathBuf;
    pub fn log_dir(&self) -> PathBuf;
    pub fn cache_dir(&self) -> PathBuf;
    pub fn legacy_macos_database(&self) -> Option<PathBuf>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathsError {
    #[error("HOME is not set to an absolute path: Ressac cannot tell where its files live")]
    NoHome,
}
```

`ressac_core::store::default_path() -> PathBuf` keeps its signature. `crates/ressac/src/main.rs` gains `fn init_logging(dir: &Path, verbose: bool) -> Option<WorkerGuard>` (the signature B0b moves to `ressac_core::logging::init`).

**Consumed later:** A2 calls `ticker_state` and `terminal_settings` from the CLI; A3 calls `legacy_macos_database`; B0b and B1b call `from_env`, `log_dir`, `database`.

**Deviation from the master plan:** the phase table puts "`terminal.json` routed through it" in A1. With `paths.rs`, `lib.rs`, `store/mod.rs`, `main.rs` and `cli.rs`, A1 already touches 5 files, so routing `terminal.json` (`tui/reader/settings.rs`) moves to A2 (A2 Task 3), as in the audit's own P1/P2 split. `Paths::terminal_settings` itself is still built and tested here. `ticker_state` (with the per-database rule) is built here because the master plan lists it in A1's interface; A2 wires it into the CLI.

**Decisions this phase pins (the spec leaves them open):**

- `HOME` unset, empty, or relative → `PathsError::NoHome`, on both layouts, even when every `XDG_*` is set: no path is ever relative to the working directory.
- An `XDG_*` value counts only when non-empty and absolute (Review Focus 1).
- macOS `cache_dir()` is `~/Library/Caches/Ressac` (the spec has no macOS cache; nothing creates or uses it yet).
- The ticker hash is FNV-1a 64 (hand-written, no new crate) over `canonical(db).as_os_str().as_encoded_bytes()`, printed as `{:016x}`. `canonical(p)` is `std::fs::canonicalize(p)`, falling back to `std::path::absolute(p)` (then `p`) when the file does not exist. The default database is recognised by `canonical(db) == canonical(self.database())`.

**macOS stays byte-identical.** Proven by the new `macos_layout_matches_the_frozen_paths` (exact strings, with `XDG_*` set and ignored) and by the existing `store::tests::default_path_is_under_application_support`, `tui::reader::settings::tests::the_default_path_sits_next_to_the_database` and `tests/cli.rs` `ticker_survives_an_unwritable_log_directory`, which keep passing unchanged on the Mac.

**Step 0:** not triggered. No file over 300 lines is restructured (`store/mod.rs` changes one function body and one test; `cli.rs` one doc attribute).

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

### Task 1: the `paths` module

**Files:**
- Create: `crates/ressac-core/src/paths.rs`
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod paths;` in alphabetical order, after `pub mod parse;`)
- Test: inline `#[cfg(test)] mod tests` in `paths.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: everything in the header's "Interfaces produced" block, plus private `fn fnv1a64(bytes: &[u8]) -> u64` and private `fn canonical(p: &Path) -> PathBuf`.

Layout table the tests pin (`H` = `HOME`):

| | `MacOs` | `Xdg` (default when the variable is unusable) |
|---|---|---|
| data (`database()` = data/`ressac.sqlite`) | `H/Library/Application Support/Ressac` | `$XDG_DATA_HOME/ressac` (`H/.local/share/ressac`) |
| config (`terminal_settings()` = config/`terminal.json`) | = data | `$XDG_CONFIG_HOME/ressac` (`H/.config/ressac`) |
| state (`ticker_state`) | = data | `$XDG_STATE_HOME/ressac` (`H/.local/state/ressac`) |
| `log_dir()` | `H/Library/Logs/Ressac` | state/`logs` |
| `cache_dir()` | `H/Library/Caches/Ressac` | `$XDG_CACHE_HOME/ressac` (`H/.cache/ressac`) |
| `legacy_macos_database()` | always `None` | `Some(H/Library/Application Support/Ressac/ressac.sqlite)` if that file exists, else `None` |

`ticker_state(db)`: state/`ticker.json` when `db` is the default database (any spelling), else state/`ticker-<fnv1a64 of canonical(db), 16 lowercase hex>.json`.

- [ ] **Step 1: Write the failing tests** (the module holds only a `//!` doc line and this test module; add `pub mod paths;` to `lib.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const VARS: [&str; 4] = ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"];

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs.iter().map(|(k, v)| (k.to_string(), OsString::from(v))).collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn resolve(layout: Layout, pairs: &[(&str, &str)]) -> Result<Paths, PathsError> {
        Paths::resolve(layout, &env(pairs))
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn layout_current_matches_the_build_target() {
        let expected = if cfg!(target_os = "macos") { Layout::MacOs } else { Layout::Xdg };
        assert_eq!(Layout::current(), expected);
    }

    #[test]
    fn macos_layout_matches_the_frozen_paths() {
        // The Swift app hard-codes the same database path: these strings never change.
        let paths = resolve(
            Layout::MacOs,
            &[
                ("HOME", "/Users/clement"),
                ("XDG_DATA_HOME", "/x/data"),
                ("XDG_CONFIG_HOME", "/x/config"),
                ("XDG_STATE_HOME", "/x/state"),
                ("XDG_CACHE_HOME", "/x/cache"),
            ],
        )
        .unwrap();
        let dir = "/Users/clement/Library/Application Support/Ressac";
        assert_eq!(paths.database(), p(&format!("{dir}/ressac.sqlite")));
        assert_eq!(paths.terminal_settings(), p(&format!("{dir}/terminal.json")));
        assert_eq!(paths.ticker_state(&paths.database()), p(&format!("{dir}/ticker.json")));
        assert_eq!(paths.log_dir(), p("/Users/clement/Library/Logs/Ressac"));
        assert_eq!(paths.cache_dir(), p("/Users/clement/Library/Caches/Ressac"));
        assert_eq!(paths.legacy_macos_database(), None);
    }

    #[test]
    fn xdg_layout_defaults_under_home() {
        let paths = resolve(Layout::Xdg, &[("HOME", "/home/clement")]).unwrap();
        assert_eq!(paths.database(), p("/home/clement/.local/share/ressac/ressac.sqlite"));
        assert_eq!(paths.terminal_settings(), p("/home/clement/.config/ressac/terminal.json"));
        assert_eq!(paths.ticker_state(&paths.database()), p("/home/clement/.local/state/ressac/ticker.json"));
        assert_eq!(paths.log_dir(), p("/home/clement/.local/state/ressac/logs"));
        assert_eq!(paths.cache_dir(), p("/home/clement/.cache/ressac"));
    }

    #[test]
    fn xdg_layout_follows_absolute_xdg_variables() {
        let paths = resolve(
            Layout::Xdg,
            &[
                ("HOME", "/home/clement"),
                ("XDG_DATA_HOME", "/x/data"),
                ("XDG_CONFIG_HOME", "/x/config"),
                ("XDG_STATE_HOME", "/x/state"),
                ("XDG_CACHE_HOME", "/x/cache"),
            ],
        )
        .unwrap();
        assert_eq!(paths.database(), p("/x/data/ressac/ressac.sqlite"));
        assert_eq!(paths.terminal_settings(), p("/x/config/ressac/terminal.json"));
        assert_eq!(paths.ticker_state(&paths.database()), p("/x/state/ressac/ticker.json"));
        assert_eq!(paths.log_dir(), p("/x/state/ressac/logs"));
        assert_eq!(paths.cache_dir(), p("/x/cache/ressac"));
    }

    #[test]
    fn empty_or_relative_xdg_values_are_ignored() {
        // Review Focus 1: some shells export XDG_DATA_HOME="" or a relative value.
        let defaults = resolve(Layout::Xdg, &[("HOME", "/home/clement")]).unwrap();
        for var in VARS {
            for value in ["", "rel/dir", "./rel"] {
                let paths = resolve(Layout::Xdg, &[("HOME", "/home/clement"), (var, value)]).unwrap();
                assert_eq!(paths, defaults, "{var}={value:?}");
                for path in [paths.database(), paths.terminal_settings(), paths.log_dir(), paths.cache_dir()] {
                    assert!(path.is_absolute(), "{var}={value:?} gave {path:?}");
                }
            }
        }
    }

    #[test]
    fn no_usable_home_is_an_error_on_both_layouts() {
        let all_xdg: Vec<(&str, &str)> = VARS.iter().map(|v| (*v, "/x")).collect();
        for layout in [Layout::MacOs, Layout::Xdg] {
            assert_eq!(resolve(layout, &all_xdg), Err(PathsError::NoHome), "{layout:?}, no HOME");
            for home in ["", "relative/home"] {
                let mut pairs = all_xdg.clone();
                pairs.push(("HOME", home));
                assert_eq!(resolve(layout, &pairs), Err(PathsError::NoHome), "{layout:?}, HOME={home:?}");
            }
        }
    }

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn another_database_gets_a_hashed_file_in_the_state_directory() {
        for layout in [Layout::MacOs, Layout::Xdg] {
            let paths = resolve(layout, &[("HOME", "/home/clement")]).unwrap();
            let state = paths.ticker_state(&paths.database());
            let other = paths.ticker_state(Path::new("/nowhere/r.sqlite"));
            assert_eq!(other.parent(), state.parent(), "{layout:?}");
            assert_eq!(
                other.file_name().unwrap().to_str().unwrap(),
                format!("ticker-{:016x}.json", fnv1a64(b"/nowhere/r.sqlite"))
            );
            assert_ne!(paths.ticker_state(Path::new("/nowhere/s.sqlite")), other);
        }
    }

    #[cfg(unix)]
    #[test]
    fn one_database_named_several_ways_has_one_ticker_state() {
        // Review Focus 2, resolver side (A2 adds the CLI side with a `./` spelling).
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Layout::Xdg, &{
            let h = home.path().as_os_str().to_owned();
            move |k: &str| (k == "HOME").then(|| h.clone())
        })
        .unwrap();
        let dir = home.path().join("dbs");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("r.sqlite"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("r.sqlite"), dir.join("link.sqlite")).unwrap();
        let one = paths.ticker_state(&dir.join("r.sqlite"));
        assert_eq!(paths.ticker_state(&dir.join("sub/../r.sqlite")), one);
        assert_eq!(paths.ticker_state(&dir.join("link.sqlite")), one);
        assert!(one.file_name().unwrap().to_str().unwrap().starts_with("ticker-"));

        // The default database stays `ticker.json`, also through a symlink.
        let default = paths.database();
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        std::fs::write(&default, b"").unwrap();
        std::os::unix::fs::symlink(&default, dir.join("default-link.sqlite")).unwrap();
        let ticker_json = paths.ticker_state(&default);
        assert_eq!(ticker_json.file_name().unwrap(), "ticker.json");
        assert_eq!(paths.ticker_state(&dir.join("default-link.sqlite")), ticker_json);
    }

    #[test]
    fn the_legacy_mac_database_is_reported_only_on_xdg_and_only_when_it_exists() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path().as_os_str().to_owned();
        let env = move |k: &str| (k == "HOME").then(|| h.clone());
        let xdg = Paths::resolve(Layout::Xdg, &env).unwrap();
        let mac = Paths::resolve(Layout::MacOs, &env).unwrap();
        assert_eq!(xdg.legacy_macos_database(), None);
        let legacy = home.path().join("Library/Application Support/Ressac/ressac.sqlite");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"").unwrap();
        assert_eq!(xdg.legacy_macos_database(), Some(legacy));
        assert_eq!(mac.legacy_macos_database(), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ressac-core paths::`
Expected: compile errors (`cannot find type Layout`, `Paths`, `PathsError`, function `fnv1a64`).

- [ ] **Step 3: Implement `paths.rs`** with the header's signatures and the layout table above. `resolve` reads `HOME` (must be non-empty and absolute, else `Err(PathsError::NoHome)`) and, for `Layout::Xdg`, each `XDG_*` through one helper `fn xdg(env, var, fallback: PathBuf) -> PathBuf` that keeps the value only when it is non-empty and absolute. `fnv1a64`: offset basis `0xcbf29ce484222325`, prime `0x100000001b3`, for each byte `hash ^= b; hash = hash.wrapping_mul(prime)`. The module doc says the macOS layout is frozen (the Swift app's `Database.swift` hard-codes the same database path) and that `resolve` is pure so both layouts are tested on any host (apart from `ticker_state`'s and `legacy_macos_database`'s file checks).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ressac-core paths::`
Expected: `test result: ok. 10 passed` (9 on a non-Unix host).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all && cargo clippy -p ressac-core --all-targets -- -D warnings`
Expected: no warning.

```bash
git add crates/ressac-core/src/paths.rs crates/ressac-core/src/lib.rs
git commit -m "Core: ressac_core::paths, macOS layout frozen, XDG on Linux

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: the default database, the logs and the `--db` help go through `Paths`

**Files:**
- Modify: `crates/ressac-core/src/store/mod.rs:88-94` (`default_path`), `:358-365` (its test)
- Modify: `crates/ressac/src/main.rs:20-24` (`main`), `:129-141` (`init_logging`)
- Modify: `crates/ressac/src/cli.rs:17` (the `db` field's doc comment)
- Test: `crates/ressac/tests/cli.rs` (`ticker_survives_an_unwritable_log_directory` at `:1287-1300`, two new tests)

**Interfaces:**
- Consumes: Task 1's `Paths::from_env`, `Paths::database`, `Paths::log_dir`.
- Produces: `store::default_path() -> PathBuf` (same signature: `Paths::from_env()?.database()`; on `Err`, `tracing::error!` the error and return the old `HOME`-joined path, `unwrap_or_default` included); `fn init_logging(dir: &Path, verbose: bool) -> Option<tracing_appender::non_blocking::WorkerGuard>` in `main.rs`; `main` computes `let paths = ressac_core::paths::Paths::from_env().ok();` once, right after `Cli::parse()`, and logs only when it is `Some` (as today, no usable `HOME` means no file logs). `run(cli)` keeps its signature in this phase; A2 passes `paths` to it.

- [ ] **Step 1: Write the failing tests**

In `crates/ressac-core/src/store/mod.rs`, keep `default_path_is_under_application_support` but put `#[cfg(target_os = "macos")]` on it, and add beside it:

```rust
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn default_path_is_under_the_xdg_data_directory() {
        let p = default_path();
        assert!(p.ends_with("ressac/ressac.sqlite"), "{p:?}");
        assert!(!p.to_string_lossy().contains("Library/Application Support"), "{p:?}");
    }
```

In `crates/ressac/tests/cli.rs`, add the helper and the tests, and make the log test use the helper (`let logs = log_dir(home.path());` instead of `home.path().join("Library/Logs/Ressac")`):

```rust
/// The layout's log directory for `home` (macOS frozen; XDG defaults, the tests clear `XDG_*`).
fn log_dir(home: &Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Logs/Ressac")
    } else {
        home.join(".local/state/ressac/logs")
    }
}

#[test]
fn a_run_creates_the_layout_s_log_directory() {
    let home = tempfile::tempdir().unwrap();
    let out = ressac(home.path(), &["export"]);
    assert!(log_dir(home.path()).is_dir(), "{out:?}");
}

#[test]
fn db_help_names_the_layout_s_default() {
    let home = tempfile::tempdir().unwrap();
    let help = stdout(&ressac(home.path(), &["--help"]));
    let expected = if cfg!(target_os = "macos") {
        "Database file [default: ~/Library/Application Support/Ressac/ressac.sqlite]"
    } else {
        "Database file [default: ~/.local/share/ressac/ressac.sqlite, or $XDG_DATA_HOME/ressac/ressac.sqlite]"
    };
    assert!(help.contains(expected), "{help}");
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p ressac-core store::tests::default_path && cargo test -p ressac --test cli -- a_run_creates_the_layout_s_log_directory db_help_names_the_layout_s_default ticker_survives_an_unwritable_log_directory`
Expected on the Mac: all PASS already (the macOS paths do not change: this is the byte-identical proof). The Linux variants fail until this task's code exists; they run in the Linux gate from A4. Record this in the commit message's body.

- [ ] **Step 3: Implement**

  - `store::default_path` as in the Interfaces block; its doc says it is the default database of `Paths::from_env()` and names the fallback.
  - `main.rs`: `init_logging(dir: &Path, verbose: bool)`: same body minus the `HOME` lookup (`create_dir_all(dir)`, builder `.build(dir)`); doc comment: "Daily-rotated log file in `dir` (`Paths::log_dir`: `~/Library/Logs/Ressac` on macOS, `$XDG_STATE_HOME/ressac/logs` on Linux), mirrored to stderr with `-v`." `main`: `let _log_guard = paths.as_ref().and_then(|p| init_logging(&p.log_dir(), cli.verbose));`.
  - `cli.rs:17`: replace the doc line with the two exact strings of the test, through `#[cfg_attr(target_os = "macos", doc = "…")]` and `#[cfg_attr(not(target_os = "macos"), doc = "…")]` (`cfg_attr` expands before clap's derive reads the doc).

- [ ] **Step 4: Run the tests to verify they pass, then the Mac gate**

Run: `cargo test -p ressac-core store::tests && cargo test -p ressac --test cli`
Expected: `test result: ok.` for both.
Run the four gate commands of the header. Expected: as listed there.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/mod.rs crates/ressac/src/main.rs crates/ressac/src/cli.rs crates/ressac/tests/cli.rs
git commit -m "Default database, logs and --db help from ressac_core::paths

macOS paths unchanged; the Linux variants of the tests run from phase A4.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
