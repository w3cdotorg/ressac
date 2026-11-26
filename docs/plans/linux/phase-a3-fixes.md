# Phase A3: neutral wording, detached browser, SIGTERM, legacy database warning

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the four small CLI fixes of spec §5.1–§5.2: the busy sentence no longer names `Ressac.app`; every link opens through one detached call; SIGTERM is a first Ctrl-C on macOS and Linux; on Linux, a database left at the old `~/Library` path is named in the log once per run.

**Runs after:** A2 (`run(cli, paths: Option<&Paths>)`, `hermetic()` in `tests/cli.rs`). Master plan `docs/plans/2026-10-23-linux.md`; spec §5.1 (last bullets) and §5.2; audit §1.2, §1.3, §1.7, §2.1 "Legacy detection".

**Files (5 counted):**

| File | Task | Responsibility |
|---|---|---|
| `crates/ressac/src/tui/reader/jobs.rs:25-33, 288-310` | 1 | `pub const BUSY`, `sentence` returns it |
| `crates/ressac/src/tui/reader/mod.rs:311, 620, 868, 1148-1153` | 1, 2 | test uses `jobs::BUSY`; comments; `OpenUrl` through `crate::open_in_browser` |
| `crates/ressac/src/tui/reader/antiope.rs:2674` | 1 | the test's failure reason uses `jobs::BUSY` |
| `crates/ressac/src/tui/mod.rs:41-44` | 2 | garden `Open` through `crate::open_in_browser` |
| `crates/ressac/src/main.rs` | 2, 3, 4 | `open_in_browser` detached; `Interrupts`; `legacy_database_warning` and its call |
| `crates/ressac/tests/cli.rs` (test file, not counted) | 3, 4 | SIGTERM tests, legacy warning tests |

**Interfaces consumed:** A1 `ressac_core::paths::{Layout, Paths}`, `Paths::resolve`, `Paths::database`, `Paths::legacy_macos_database() -> Option<PathBuf>` (`Some` only on the XDG layout and only when `~/Library/Application Support/Ressac/ressac.sqlite` exists); A2 `run(cli: Cli, paths: Option<&Paths>)`, `hermetic(home) -> Command`, `channel(title, items)` (existing helper in `tests/cli.rs`), `stderr(&Output)`.

**Interfaces produced:**
- `crate::tui::reader::jobs::BUSY: &str = "The database is busy (is another Ressac writing?)"`.
- `fn open_in_browser(url: &str) -> std::io::Result<()>` in `main.rs` (crate root, so `crate::open_in_browser` from `tui`): `RESSAC_NO_BROWSER=1` logs only (unchanged), else `open::that_detached(url)`. The only `open::` call in the crate.
- `struct Interrupts` in `main.rs`: `fn new() -> Interrupts` (registers the SIGTERM listener, `#[cfg(unix)]`, `tokio::signal::unix::signal(SignalKind::terminate()).ok()`), `async fn next(&mut self) -> bool` (resolves at the next SIGINT or SIGTERM, `tokio::select!` over `tokio::signal::ctrl_c()` and the SIGTERM stream; `false` when no listener could be set up).
- `fn legacy_database_warning(paths: &Paths, db: &Path) -> Option<String>` in `main.rs`.

**Decisions this phase pins:**
- The busy sentence becomes a constant, so the three tests that pinned the old text (`jobs.rs:290`, `reader/mod.rs:1151`, `antiope.rs:2674`) follow it; the jobs test still spells the sentence out once. The comments at `reader/mod.rs:620` and `:868` say "the app or another `ressac`". `store/library.rs:626` (a correct comment about Ressac.app) is left alone: not worth a sixth file.
- SIGTERM shares SIGINT's handler: the first of either sets `stop` (clean stop, exit 130 where the command honours `stop`), the second of either calls `exit(130)`. The SIGTERM listener is registered in `run` **before** the handler task is spawned and before dispatch, so a SIGTERM arriving during a command never takes the default path.
- The legacy warning names both files, is emitted once per process from `run` (after `db` is known), at `warn`, except for `ressac ticker` and `ressac ticker open` (both forms of `Command::Ticker`), where it is `debug`: tmux runs those every 15 s, and `commands/ticker.rs` already logs expected conditions at `debug` for that reason. No warning when `db` is the legacy file itself (compared after `std::fs::canonicalize` of both; a `db` that does not exist is never the legacy file).

**Known limitation (to record in the ledger):** the reader and garden TUIs do not watch `Globals::stop` (Ctrl-C is a key there, not a signal; `tui/mod.rs`'s loop blocks on `event::read`). With this phase, an external SIGTERM to a TUI behaves like an external SIGINT today: the first is ignored, the second exits 130 without restoring the terminal. Before, one SIGTERM killed it, also without restoring. Making both TUIs quit cleanly on the first signal needs `commands/mod.rs`, `commands/garden.rs` and a polling garden loop: beyond this phase's 5 files. Proposed as a follow-up line in `TODO.md` (A6).

**macOS stays byte-identical** (paths): this phase changes no path. `legacy_database_warning` returns `None` on the macOS layout (`the_mac_layout_never_warns`, `the_mac_never_calls_its_own_database_legacy`).

**Step 0:** not triggered. `reader/mod.rs` (1289 lines) and `antiope.rs` (3214) get a one-line change each and string edits; nothing is restructured.

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

### Task 1: "is another Ressac writing?"

**Files:**
- Modify: `crates/ressac/src/tui/reader/jobs.rs:25-33` (doc of `sentence`, the constant), test `:288-310`
- Modify: `crates/ressac/src/tui/reader/mod.rs:620, 868` (comments), test `:1148-1153`
- Modify: `crates/ressac/src/tui/reader/antiope.rs:2674` (test)

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub const BUSY: &str` in `jobs.rs` (header). `sentence(text: String) -> String` keeps its signature and returns `BUSY.into()` for "database is locked" / "database table is locked".

- [ ] **Step 1: Write the failing test.** In `jobs.rs`, replace the first line of `a_busy_database_gets_the_busy_sentence` and add one assertion:

```rust
    #[test]
    fn a_busy_database_gets_the_busy_sentence() {
        assert_eq!(BUSY, "The database is busy (is another Ressac writing?)");
        let busy = BUSY;
        assert_eq!(sentence("database is locked".into()), busy);
        assert_eq!(sentence("database table is locked".into()), busy);
        assert_eq!(sentence("boom".into()), "boom");
        // … the edition_outcome and extract_outcome assertions stay as they are.
    }
```

In `reader/mod.rs:1150-1151`: `assert_eq!(busy_or(&anyhow::anyhow!("database is locked")), jobs::BUSY);`. In `antiope.rs:2674`: `&JournalPhase::Failed(crate::tui::reader::jobs::BUSY.into()),` (the test checks that a long reason wraps; the new sentence is longer, so it still wraps).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac --bin ressac tui::reader`
Expected: compile error `cannot find value BUSY`.

- [ ] **Step 3: Implement** the constant and `sentence`; its doc: "A busy database (the app or another `ressac` writing for longer than the busy timeout) gets `BUSY`; any other text is kept." Comments: `:620` "The clock, and what the app, another `ressac` or a job wrote."; `:868` "…what another process's writes look like to the reader."

- [ ] **Step 4: Run the tests and the search**

Run: `cargo test -p ressac --bin ressac tui::reader`
Expected: `test result: ok.`
Run: `grep -rn 'Ressac\.app writing' crates/`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/tui/reader/jobs.rs crates/ressac/src/tui/reader/mod.rs crates/ressac/src/tui/reader/antiope.rs
git commit -m "Reader: \"is another Ressac writing?\" (no Ressac.app on Linux)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: one detached way to open a link

**Files:**
- Modify: `crates/ressac/src/main.rs:119-127` (`open_in_browser`)
- Modify: `crates/ressac/src/tui/mod.rs:41-44` (garden `Command::Open`)
- Modify: `crates/ressac/src/tui/reader/mod.rs:311` (`Command::OpenUrl`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `open_in_browser` (header). The garden arm becomes `crate::open_in_browser(&url)?; Ok(None)`; the reader arm `crate::open_in_browser(&url).map(|()| Vec::new()).map_err(Into::into)`. A side effect worth having: `RESSAC_NO_BROWSER=1` now also keeps the TUIs from launching a browser.

No unit test can observe "did not wait for the launcher"; the proof is that `open::that_detached` (open 5.4.4, `src/lib.rs:285`) spawns without waiting, plus the existing `ticker_open_is_silent` and `ticker::tests::open_*` tests, which keep passing through the same function.

- [ ] **Step 1: Implement.** `open_in_browser`'s doc: "Opens a URL in the default browser without waiting for the launcher (`xdg-open` may run the browser in the foreground); `RESSAC_NO_BROWSER=1` only logs it (tests)."

- [ ] **Step 2: Verify the call sites**

Run: `grep -rn 'open::that' crates/ressac/src`
Expected: exactly one line, `crates/ressac/src/main.rs:…:    open::that_detached(url)`.
Run: `cargo test -p ressac`
Expected: `test result: ok.` for every target.

- [ ] **Step 3: Commit**

```bash
git add crates/ressac/src/main.rs crates/ressac/src/tui/mod.rs crates/ressac/src/tui/reader/mod.rs
git commit -m "Open links with open::that_detached, through one function

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 3: SIGTERM is a first Ctrl-C

**Files:**
- Modify: `crates/ressac/src/main.rs:53-65` (the handler task in `run`; `Interrupts`)
- Test: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: A2's `hermetic`; existing `channel`, `ressac_async`, `stderr`; `commands::INTERRUPTED` (130).
- Produces: `Interrupts` (header). In `run`: `let mut interrupts = Interrupts::new();` then `tokio::spawn(async move { if interrupts.next().await { stop.store(true, Relaxed); if interrupts.next().await { std::process::exit(INTERRUPTED.into()) } } })`. The comment above it: "First Ctrl-C or SIGTERM: stop scheduling and finish cleanly. Second one: leave now."

- [ ] **Step 1: Write the failing tests** (`use std::process::Stdio; use std::time::{Duration, Instant};` at the top of `tests/cli.rs` if not there)

```rust
/// `ressac fetch` on four feeds of one host that answer after `delay`, started in the
/// background. Returns once the server has seen a request: the signal listeners are in place.
#[cfg(unix)]
async fn slow_fetch(server: &MockServer, delay: Duration) -> (tempfile::TempDir, std::process::Child) {
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(channel("Slow", ""), "application/rss+xml")
                .set_delay(delay),
        )
        .mount(server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let outlines: String = (1..=4)
        .map(|i| format!(r#"<outline text="S{i}" xmlUrl="{}/s{i}.xml"/>"#, server.uri()))
        .collect();
    let opml = home.path().join("slow.opml");
    std::fs::write(&opml, format!("<opml><body>{outlines}</body></opml>")).unwrap();
    assert!(ressac_async(home.path(), &["import", opml.to_str().unwrap()]).await.status.success());
    let child = hermetic(home.path())
        .args(["--db", home.path().join("db.sqlite").to_str().unwrap(), "fetch"])
        .env("RESSAC_ALLOW_PRIVATE_ARTICLES", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.received_requests().await.unwrap_or_default().is_empty() {
        assert!(Instant::now() < deadline, "the fetch never reached the server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (home, child)
}

#[cfg(unix)]
fn signal(name: &str, pid: u32) {
    let status = Command::new("kill").args([name, &pid.to_string()]).status().unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_stops_a_fetch_like_a_first_ctrl_c() {
    let server = MockServer::start().await;
    let (_home, child) = slow_fetch(&server, Duration::from_millis(1500)).await;
    signal("-TERM", child.id());
    let out = tokio::task::spawn_blocking(move || child.wait_with_output()).await.unwrap().unwrap();
    assert_eq!(out.status.code(), Some(130), "{out:?}");
    assert!(stderr(&out).contains("Interrupted: "), "{}", stderr(&out));
}

#[cfg(unix)]
#[tokio::test]
async fn a_second_signal_of_either_kind_leaves_at_once() {
    let server = MockServer::start().await;
    let (_home, child) = slow_fetch(&server, Duration::from_secs(20)).await;
    let started = Instant::now();
    signal("-TERM", child.id());
    // Two signals sent together may be delivered as one.
    tokio::time::sleep(Duration::from_millis(200)).await;
    signal("-INT", child.id());
    let out = tokio::task::spawn_blocking(move || child.wait_with_output()).await.unwrap().unwrap();
    assert_eq!(out.status.code(), Some(130), "{out:?}");
    assert!(started.elapsed() < Duration::from_secs(10), "waited for the slow feed");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac --test cli -- sigterm_stops a_second_signal`
Expected: both FAIL with `left: None, right: Some(130)` (SIGTERM kills the process; `code()` is `None`).

- [ ] **Step 3: Implement** `Interrupts` and the handler task as in the Interfaces block. `Interrupts::new()` runs in `run` before `tokio::spawn`, so the SIGTERM listener exists before dispatch; on a non-Unix target `Interrupts` holds nothing and `next` waits for Ctrl-C only.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac --test cli -- sigterm_stops a_second_signal`
Expected: `test result: ok. 2 passed`. Then `cargo test -p ressac --test cli` (whole suite) passes.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/main.rs crates/ressac/tests/cli.rs
git commit -m "SIGTERM stops like a first Ctrl-C (exit 130), macOS and Linux

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 4: the legacy `~/Library` database, named once per run on Linux

**Files:**
- Modify: `crates/ressac/src/main.rs` (`legacy_database_warning`, its call in `run`, a new `#[cfg(test)] mod tests`)
- Test: `crates/ressac/src/main.rs` (inline), `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: `Paths::resolve`, `Paths::database`, `Paths::legacy_macos_database` (A1); `run(cli, paths)` (A2).
- Produces: `fn legacy_database_warning(paths: &Paths, db: &Path) -> Option<String>`, exact text below. In `run`, once, right after `db` is resolved: `if let Some(text) = paths.and_then(|p| legacy_database_warning(p, &db))`, then `tracing::debug!("{text}")` when `cli.command` is `Some(Command::Ticker { .. })`, else `tracing::warn!("{text}")`.

- [ ] **Step 1: Write the failing tests.** In `main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::paths::{Layout, Paths};

    fn paths(layout: Layout, home: &Path) -> Paths {
        let home = home.as_os_str().to_owned();
        Paths::resolve(layout, &move |k: &str| (k == "HOME").then(|| home.clone())).unwrap()
    }

    fn make_legacy(home: &Path) -> PathBuf {
        let legacy = home.join("Library/Application Support/Ressac/ressac.sqlite");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"").unwrap();
        legacy
    }

    #[test]
    fn a_legacy_database_on_linux_is_named_with_the_one_in_use() {
        let home = tempfile::tempdir().unwrap();
        let paths = paths(Layout::Xdg, home.path());
        let db = paths.database();
        assert_eq!(legacy_database_warning(&paths, &db), None);
        let legacy = make_legacy(home.path());
        assert_eq!(
            legacy_database_warning(&paths, &db),
            Some(format!(
                "An older Ressac database is still at {}, unused: this run uses {}. \
                 Move it there, or pass --db, to keep using it.",
                legacy.display(),
                db.display()
            ))
        );
        assert_eq!(legacy_database_warning(&paths, &legacy), None, "--db on the old file");
    }

    #[test]
    fn the_mac_layout_never_warns() {
        let home = tempfile::tempdir().unwrap();
        let paths = paths(Layout::MacOs, home.path());
        make_legacy(home.path());
        assert_eq!(legacy_database_warning(&paths, &home.path().join("db.sqlite")), None);
    }
}
```

In `tests/cli.rs`:

```rust
const LEGACY: &str = "An older Ressac database is still at";

fn make_legacy_db(home: &Path) {
    let legacy = home.join("Library/Application Support/Ressac/ressac.sqlite");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, b"").unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_legacy_database_is_warned_about_once_per_run() {
    let home = tempfile::tempdir().unwrap();
    make_legacy_db(home.path());
    let out = ressac(home.path(), &["-v", "export"]);
    assert_eq!(stderr(&out).matches(LEGACY).count(), 1, "{}", stderr(&out));
    // The tmux ticker runs every 15 s: debug only, so not on stderr at the default level.
    let out = ressac(home.path(), &["-v", "ticker", "--tmux"]);
    assert!(!stderr(&out).contains(LEGACY), "{}", stderr(&out));
}

#[cfg(target_os = "macos")]
#[test]
fn the_mac_never_calls_its_own_database_legacy() {
    let home = tempfile::tempdir().unwrap();
    make_legacy_db(home.path());
    let out = ressac(home.path(), &["-v", "export"]);
    assert!(!stderr(&out).contains(LEGACY), "{}", stderr(&out));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac --bin ressac -- a_legacy_database_on_linux the_mac_layout_never_warns`
Expected: compile error `cannot find function legacy_database_warning`.

- [ ] **Step 3: Implement** `legacy_database_warning` (`paths.legacy_macos_database()?`, then `None` when `canonicalize(db)` equals `canonicalize(legacy)`, else the sentence of the test) and the call in `run` as in the Interfaces block. The warning is a log line only: nothing is moved, nothing is printed on stdout.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac --bin ressac -- a_legacy_database_on_linux the_mac_layout_never_warns && cargo test -p ressac --test cli -- legacy`
Expected: `test result: ok.` (on the Mac: the two unit tests and `the_mac_never_calls_its_own_database_legacy`; the Linux CLI test runs in the Linux gate from A4).
Then the Mac gate (four commands of the header). Expected: as listed there.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac/src/main.rs crates/ressac/tests/cli.rs
git commit -m "Linux: name a leftover ~/Library database once per run (never moved)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
