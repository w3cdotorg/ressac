# Phase A4: the Fedora 44 dev container and the Linux gate

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** one command on the Mac, `apps/linux/dev/dev.sh check`, runs the whole gate (fmt, clippy, tests, chart tests, a smoke test of the XDG layout) on Fedora 44 aarch64, as a non-root user, in an OrbStack container that never writes into the Mac's `target/` and never touches another container.

**Runs after:** A3 (A1–A3 merged on `linux`). Master plan `docs/plans/2026-10-23-linux.md` (Global Constraints: Fedora, Gates, Never); spec §5.4 and §8.1; audit §4.3; spike notes §1 (dnf list, `lld`), §2 (the root-only test failure), "Recommendation" (non-root, `QT_FORCE_STDERR_LOGGING=1`).

**Files (4 counted, plus at most 1 fix in Task 3):**

| File | Task | Responsibility |
|---|---|---|
| `apps/linux/dev/packages.txt` (new) | 1 | The one dnf list (spike §1 + `nodejs`, `rustfmt`, `clippy`, small base tools); read by the Containerfile, by CI (A5) and later by `install.sh` (B1b) |
| `apps/linux/dev/Containerfile` (new) | 1 | `fedora:44`, the packages, user `ressac` with the Mac's uid, `CARGO_HOME` / `CARGO_TARGET_DIR` inside the container |
| `apps/linux/dev/dev.sh` (new, executable) | 1, 2 | `image`, `up`, `sh [command…]`, `check`; container `ressac-fedora` only |
| `scripts/check-linux.sh` (new, executable) | 2 | The Linux gate: guards, the four gate commands, a hermeticity check, the XDG layout smoke test |
| one source or test file (only if needed) | 3 | The fix for a Linux-only failure of the A1–A3 tests (see Task 3's rule) |

**Interfaces produced (later phases rely on these names):**

- Image `ressac-fedora:44`; container `ressac-fedora` (hostname `ressac-fedora`, so OrbStack serves `ressac-fedora.orb.local`, which B1b's VNC check uses); named volumes `ressac-fedora-target` (at `/home/ressac/target`) and `ressac-fedora-cargo` (at `/home/ressac/.cargo`). The repo is bind-mounted read-write at **`/work/ressac`**, the working directory of every `exec`.
- Container environment: `CARGO_HOME=/home/ressac/.cargo`, `CARGO_TARGET_DIR=/home/ressac/target`, `QT_FORCE_STDERR_LOGGING=1`. User `ressac`, uid = the Mac's `id -u` (504 on Clément's Mac) unless the image already has that uid, in which case the next free one (`useradd` picks it).
- `apps/linux/dev/dev.sh image | up | sh [command…] | check`: `sh` with arguments runs `bash -lc "<arguments>"` as `ressac` and returns its exit code; without, an interactive login shell. `check` runs `bash scripts/check-linux.sh` as `ressac` and returns its exit code. Anything else prints the usage line on stderr and exits 2.
- `scripts/check-linux.sh`: exit 0 and last line `check-linux: OK (fmt, clippy, test, chart, layout smoke)`; exit 2 with `check-linux: <reason>` for a refused environment; otherwise the failing command's own exit code. It uses `--workspace` everywhere, so once B1a makes `ressac-kde` a workspace member (not a default member) it is built, linted and tested here with no change to this script. It exports `QT_QPA_PLATFORM=offscreen` unless already set (harmless before B1a).
- `apps/linux/dev/packages.txt`: one package name per line; `#` starts a comment (whole line or end of line). Consumers read it with `sed 's/#.*//' packages.txt | xargs`.

**Consumed:** nothing from A1–A3's code; their Linux-only test variants run here for the first time (`store::tests::default_path_is_under_the_xdg_data_directory`, `tests/cli.rs` `a_run_creates_the_layout_s_log_directory`, `db_help_names_the_layout_s_default`, A2's per-database ticker tests, A3's `a_legacy_database_is_warned_about_once_per_run`, the SIGTERM tests). Review Focus 1 and 2 are pinned by A1/A2 tests that this gate now also runs on Linux.

**Decisions this phase pins:**

- **Safety.** `dev.sh` holds the container, image and volume names as constants and has no option to change them. The only container it ever runs, starts, execs into or removes is `ressac-fedora`; it never calls `docker system prune`, `docker volume prune`, `docker rm` on another name, or `docker ps -q | xargs …`. It refuses to run unless `docker context show` prints `orbstack` (`dev.sh: the docker context is <x>, not orbstack`, exit 1), and says `dev.sh: OrbStack is not running (open -a OrbStack)` (exit 1) when `docker info` fails. `PATH` gets `$HOME/.orbstack/bin` and `/opt/homebrew/bin` first.
- **The Mac's tree stays untouched.** The build output lives in the `ressac-fedora-target` volume, the registry in `ressac-fedora-cargo`. `check-linux.sh` refuses a `CARGO_TARGET_DIR` inside the repository, runs clippy and the tests with `--locked` (a stale `Cargo.lock` fails instead of being rewritten), and the gate never writes in `/work/ressac` (`cargo fmt --check` only reads). Proven by `git status --porcelain` being empty after a check.
- **`up` recreates on a new image.** `up` compares `docker inspect -f '{{.Image}}' ressac-fedora` with `docker image inspect -f '{{.Id}}' ressac-fedora:44`: missing container → `docker run -d --init --platform linux/arm64 --name ressac-fedora --hostname ressac-fedora -v "<repo>:/work/ressac" -v ressac-fedora-target:/home/ressac/target -v ressac-fedora-cargo:/home/ressac/.cargo -w /work/ressac ressac-fedora:44 sleep infinity`; another image → `docker rm -f ressac-fedora` then the same run; stopped → `docker start ressac-fedora`; running → nothing. It ends with `ressac-fedora is running (image <12 hex>).`. `check` and `sh` call `up` first; `up` builds the image first if `ressac-fedora:44` does not exist.
- **Hermetic test run.** `cargo test` runs with `HOME` and the four `XDG_*` pointed into a fresh temp directory (`CARGO_HOME` pinned first, so the registry does not move), and with `RESSAC_DB` and `RUST_LOG` unset. Afterwards that directory must still be empty: a test that writes into the real `HOME` or `XDG_*` fails the gate (`check-linux: tests wrote outside their temp dirs:` followed by up to 20 paths). This is the audit §4.3 "hermetic" run, done once instead of twice.
- **Layout smoke test.** With a fresh `HOME`, no `XDG_*` and no `RESSAC_DB`, `$CARGO_TARGET_DIR/debug/ressac import tests/fixtures/vienna-like.opml` must create `$HOME/.local/share/ressac/ressac.sqlite` and `$HOME/.local/state/ressac/logs/`, and nothing under `$HOME/Library`.

**Step 0:** not triggered (new files only; Task 3's fix, if any, is a local change).

**Gate (end of phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

---

### Task 1: the image and `dev.sh image | up | sh`

**Files:**
- Create: `apps/linux/dev/packages.txt`
- Create: `apps/linux/dev/Containerfile`
- Create: `apps/linux/dev/dev.sh` (`chmod +x`)

**Interfaces:**
- Consumes: nothing.
- Produces: the image, container, volume and path names of the header; `dev.sh image | up | sh [command…]` (Task 2 adds `check`).

`packages.txt`, exactly these, grouped under comments: the spike's list `rust cargo cmake gcc gcc-c++ clang lld make pkgconf-pkg-config qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtwebengine-devel qt6-qtwebchannel-devel kf6-kirigami kf6-kirigami-devel kf6-qqc2-desktop-style kf6-kcoreaddons-devel kf6-ki18n-devel breeze-icon-theme mesa-dri-drivers mesa-libGL mesa-libEGL dejavu-sans-fonts tmux sqlite openssl-devel`, then `rustfmt clippy` (separate packages on Fedora; the gate needs both), `nodejs` (chart tests; Node 22 or newer reads the quoted glob), `shadow-utils` (`useradd`), `util-linux-core` and `procps-ng` (`kill` for A3's SIGTERM tests, `runuser` for CI), `findutils`, `diffutils`, `tar`, `gzip`, `git-core` (CI's checkout in a container). A comment heads the file: "The one Fedora package list: apps/linux/dev/Containerfile, .github/workflows/ci.yml (fedora job) and apps/linux/install.sh read it. lld is required: GNU ld fails to link cxx-qt (docs/specs/linux/qt-spike-notes.md)."

`Containerfile`: `FROM fedora:44`; `ARG USER_UID=1000`; `COPY packages.txt /tmp/packages.txt`; one `RUN` with `dnf install -y $(sed 's/#.*//' /tmp/packages.txt)`, `dnf clean all`, then the user: `useradd --create-home --user-group --uid "$USER_UID" ressac` when `getent passwd "$USER_UID"` finds nothing, else `useradd --create-home --user-group ressac`; then `mkdir -p /home/ressac/target /home/ressac/.cargo /work/ressac` and `chown ressac:ressac` on the first two (a named volume mounted there copies this ownership on first use). `ENV CARGO_HOME=/home/ressac/.cargo CARGO_TARGET_DIR=/home/ressac/target QT_FORCE_STDERR_LOGGING=1`; `USER ressac`; `WORKDIR /work/ressac`. No `CMD` (`dev.sh up` passes `sleep infinity`).

`dev.sh`: `set -euo pipefail`; constants `NAME=ressac-fedora`, `IMAGE=ressac-fedora:44`, `TARGET_VOLUME=ressac-fedora-target`, `CARGO_VOLUME=ressac-fedora-cargo`, `PLATFORM=linux/arm64`, `MOUNT=/work/ressac`; `dir` = this script's directory, `repo` = `$dir/../../..` resolved with `pwd -P`. Subcommands:
- `image`: `docker build --pull --platform "$PLATFORM" --build-arg USER_UID="$(id -u)" -t "$IMAGE" -f "$dir/Containerfile" "$dir"`.
- `up`: as in the header's decision.
- `sh`: `up`, then `docker exec $tty -u ressac -w "$MOUNT" "$NAME" bash -l` (no arguments) or `… bash -lc "$*"`; `$tty` is `-it` when stdin and stdout are terminals, else `-i`.

- [ ] **Step 1: Write the failing check.** Before creating the files, run the checks below; each must fail now (`No such file or directory`).

```bash
export PATH="/opt/homebrew/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
docker ps -a --format '{{.Names}} {{.State}}' | sort > "$TMPDIR/containers-before.txt"   # kept for Step 4
apps/linux/dev/dev.sh nope; echo "exit=$?"
```

Expected now: `no such file or directory`, `exit=127`. After Step 3: `usage: apps/linux/dev/dev.sh image|up|sh [command…]|check` on stderr and `exit=2` (`check` is listed now and answers `usage` until Task 2).

- [ ] **Step 2: Write `packages.txt`, the `Containerfile` and `dev.sh`** as specified above.

- [ ] **Step 3: Lint, build, start**

Run: `shellcheck apps/linux/dev/dev.sh`
Expected: no output, exit 0.

Run: `apps/linux/dev/dev.sh image`
Expected: ends with `naming to docker.io/library/ressac-fedora:44` (first build about 3–5 min: the Qt and WebEngine packages).

Run: `apps/linux/dev/dev.sh up && apps/linux/dev/dev.sh up`
Expected: both print `ressac-fedora is running (image <12 hex>).`; the second does not recreate it (`docker ps --filter name=^ressac-fedora$ --format '{{.RunningFor}}'` unchanged).

Run:
```bash
apps/linux/dev/dev.sh sh 'id -un; id -u; pwd; rustc --version; cargo fmt --version; cargo clippy --version; ld.lld --version; node --version; tmux -V; type -P kill; echo "$CARGO_HOME $CARGO_TARGET_DIR $QT_FORCE_STDERR_LOGGING"; test -r Cargo.toml && echo repo-readable'
```
Expected, line by line: `ressac`; the Mac's `id -u` (504); `/work/ressac`; `rustc 1.99.…`; `rustfmt 1.…`; `clippy 0.1.99 …`; `LLD 2…`; `v2…` (22 or newer); `tmux 3.…`; `/usr/bin/kill` (the binary A3's SIGTERM tests run, not the shell builtin); `/home/ressac/.cargo /home/ressac/target 1`; `repo-readable`.

- [ ] **Step 4: Nothing else was touched**

Run: `docker ps -a --format '{{.Names}} {{.State}}' | sort | diff "$TMPDIR/containers-before.txt" -`
Expected: only an added line `ressac-fedora running` (or none if it already existed); `fsvbuild`, `gotenberg` and any `wc4*` lines identical.
Run: `git status --porcelain`
Expected: only the three new files.

- [ ] **Step 5: Commit**

```bash
git add apps/linux/dev/packages.txt apps/linux/dev/Containerfile apps/linux/dev/dev.sh
git commit -m "Linux dev container: Fedora 44 aarch64, non-root user, dev.sh image/up/sh

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: `scripts/check-linux.sh` and `dev.sh check`

**Files:**
- Create: `scripts/check-linux.sh` (`chmod +x`)
- Modify: `apps/linux/dev/dev.sh` (the `check` subcommand)

**Interfaces:**
- Consumes: Task 1's container and `sh` plumbing.
- Produces: `scripts/check-linux.sh` and `dev.sh check` as in the header.

`check-linux.sh`, in this order (`set -euo pipefail`; `die()` prints `check-linux: <reason>` on stderr and exits 2):
1. Guards: `uname -s` is `Linux`, else `die "run me on Linux (apps/linux/dev/dev.sh check)"`; `id -u` is not 0, else `die "run me as a non-root user (the Chromium sandbox and the store tests need it)"`.
2. `cd` to the repository root (`$(dirname "$0")/..`, `pwd -P`). `export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/target}"`; `die "CARGO_TARGET_DIR is inside the repository: <dir>"` when `realpath -m "$CARGO_TARGET_DIR"` is the repo root or under it. `export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" QT_FORCE_STDERR_LOGGING=1`.
3. `cargo fmt --all --check`.
4. `cargo clippy --locked --workspace --all-targets -- -D warnings`.
5. `scratch=$(mktemp -d)` with `trap 'rm -rf "$scratch"' EXIT`; `mkdir "$scratch/home"`; `env -u RESSAC_DB -u RUST_LOG HOME="$scratch/home" XDG_DATA_HOME="$scratch/xdg/data" XDG_CONFIG_HOME="$scratch/xdg/config" XDG_STATE_HOME="$scratch/xdg/state" XDG_CACHE_HOME="$scratch/xdg/cache" cargo test --locked --workspace`; then `leftover=$(find "$scratch/home" "$scratch/xdg" -mindepth 1 2>/dev/null | head -n 20)` must be empty, else `die` with the header's sentence and the paths.
6. Layout smoke test of the header, in `$scratch/smoke`; each failed `test` dies with `layout smoke: <what is missing or present>`.
7. `node --test 'apps/macos/ChartTests/*.test.cjs'`.
8. `echo "check-linux: OK (fmt, clippy, test, chart, layout smoke)"`.

`dev.sh check`: `up`, then `docker exec $tty -u ressac -w "$MOUNT" "$NAME" bash scripts/check-linux.sh`, exit code passed through.

- [ ] **Step 1: Write the failing checks**

```bash
apps/linux/dev/dev.sh check; echo "exit=$?"                       # now: usage, exit=2
```

- [ ] **Step 2: Implement** `scripts/check-linux.sh` and `dev.sh check` as above.

- [ ] **Step 3: The guards refuse what they must**

Run: `shellcheck scripts/check-linux.sh apps/linux/dev/dev.sh`
Expected: no output, exit 0.

Run: `bash scripts/check-linux.sh; echo "exit=$?"` (on the Mac)
Expected: `check-linux: run me on Linux (apps/linux/dev/dev.sh check)`, `exit=2`.

Run: `docker exec -u root -w /work/ressac ressac-fedora bash scripts/check-linux.sh; echo "exit=$?"`
Expected: `check-linux: run me as a non-root user (the Chromium sandbox and the store tests need it)`, `exit=2`.

Run: `apps/linux/dev/dev.sh sh 'CARGO_TARGET_DIR=/work/ressac/target bash scripts/check-linux.sh'; echo "exit=$?"`
Expected: `check-linux: CARGO_TARGET_DIR is inside the repository: /work/ressac/target`, `exit=2`; and on the Mac `ls target` shows no new entry (nothing was built).

- [ ] **Step 4: Commit** (the full run is Task 3)

```bash
git add scripts/check-linux.sh apps/linux/dev/dev.sh
git commit -m "Linux gate: scripts/check-linux.sh, run by dev.sh check as a non-root user

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 3: the Linux gate green

**Files:**
- Modify (only if a failure needs it): one source or test file, see the rule below.

**Interfaces:**
- Consumes: `dev.sh check` (Task 2); the A1–A3 tests named in the header.
- Produces: a green Linux gate, recorded with its numbers in the ledger.

- [ ] **Step 1: Run the gate**

Run: `apps/linux/dev/dev.sh check 2>&1 | tee "$TMPDIR/check-linux-first.log"; echo "exit=${PIPESTATUS[0]}"`
Expected (goal): last line `check-linux: OK (fmt, clippy, test, chart, layout smoke)`, `exit=0`. Known: `tui::reader::jobs::tests::a_job_that_cannot_open_the_database_reports_a_failure` fails only as root (spike §2); as `ressac` it must pass. If a stale `/nonexistent` directory exists in the container from an earlier root run, remove it with `docker exec -u root ressac-fedora rm -rf /nonexistent` and say so in the ledger.

- [ ] **Step 2: Triage each failure** (superpowers:systematic-debugging). A Linux-only failure is either a real Linux bug in A1–A3's code or a test that assumed macOS. Fix the cause, never by deleting an assertion or by `#[cfg(target_os = "macos")]` on a test that has a meaningful Linux expectation (a test with no Linux meaning may get the `cfg` with a one-line comment why). **Budget rule:** this phase can absorb one more file. If the fixes need two or more files, stop after Step 1, write each failure (test name, first assertion message, suspected file) in the ledger, and report to the controller: the fixes become a sub-phase A4b of at most 5 files.

- [ ] **Step 3: Run the gate again until it is green**

Run: `apps/linux/dev/dev.sh check; echo "exit=$?"`
Expected: `check-linux: OK (fmt, clippy, test, chart, layout smoke)`, `exit=0`. Then the four Mac gate commands of the header, all green.

- [ ] **Step 4: The Mac's tree and the other containers are untouched**

Run: `git status --porcelain && docker ps -a --format '{{.Names}} {{.State}}' | sort | diff "$TMPDIR/containers-before.txt" -`
Expected: `git status` empty (besides the fix, if any, before its commit); the diff shows only `ressac-fedora`.

- [ ] **Step 5: Record and commit**

Write in the ledger (`.superpowers/sdd/2026-10-23-linux/progress.md`, A4 row): cold build time, the total of `passed` / `ignored` from the test run, each Linux-only failure met and its fix. These numbers feed A6's `notes.md`. If a fix was needed:

```bash
git add <the fixed file>
git commit -m "Linux gate: <test name> on Linux (<one-line cause>)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
