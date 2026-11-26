# Phase A5: CI on Fedora, Linux CLI release artifacts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ci.yml` gains a Fedora 44 container job that runs the Linux gate as a non-root user, and its Ubuntu job leaves `ressac-kde` out; `release.yml` is split into `version`, `macos`, `linux`, `publish` and attaches `ressac-<version>-linux-<arch>.tar.gz` (x86_64 and aarch64, built on Ubuntu 22.04) to the release, under one `SHA256SUMS`.

**Runs after:** A4 (`apps/linux/dev/packages.txt`, `scripts/check-linux.sh`). Master plan (phase table row 5, Gates, Never: no push); spec §5.4; audit §4.1, §4.2.

**GitHub Actions has no credits until 2026-11-02.** Nothing here can run on GitHub before then, and nothing is pushed in this phase anyway. Verification is: `actionlint` (installed: `/opt/homebrew/bin/actionlint` 1.7.12, with `shellcheck` for the `run:` scripts), plus a **local replay** of the two new Linux jobs in throwaway OrbStack containers, plus a careful reading against the checklist in each task. The first real run is a `TODO.md` line (A6).

**Files (3 counted):**

| File | Task | Responsibility |
|---|---|---|
| `.github/actionlint.yaml` (new) | 1 | Declares the custom runner label `xcode-27`, so `actionlint` is clean |
| `.github/workflows/ci.yml` | 1, 2 | SC2012 fix; Ubuntu job `--exclude ressac-kde`; new `fedora` job |
| `.github/workflows/release.yml` | 1, 3 | SC2012 fix; four jobs; Linux matrix; one `SHA256SUMS`; least privilege |

**Interfaces consumed:** A4's `apps/linux/dev/packages.txt` (read with `sed 's/#.*//' apps/linux/dev/packages.txt`) and `scripts/check-linux.sh` (non-root, exit 0 with last line `check-linux: OK (fmt, clippy, test, chart, layout smoke)`; defaults `CARGO_TARGET_DIR=$HOME/target`, `CARGO_HOME=$HOME/.cargo`).

**Interfaces produced:** CI job ids `check` (Ubuntu, unchanged id), `fedora`, `macos`; release job ids `version` (output `version`), `macos`, `linux` (matrix `arch` ∈ {`x86_64`, `aarch64`}), `publish`; artifact names `macos`, `linux-x86_64`, `linux-aarch64`; release files `Ressac-<v>-macos-arm64.zip`, `ressac-<v>-macos-arm64.tar.gz`, `ressac-<v>-linux-x86_64.tar.gz`, `ressac-<v>-linux-aarch64.tar.gz`, `SHA256SUMS`.

**Decisions this phase pins:**

- **`--exclude ressac-kde` is written now.** Checked with cargo 1.99.1 on a scratch workspace: `cargo check --workspace --exclude ressac-kde` and `cargo test --workspace --exclude ressac-kde` on a workspace without that package print `warning: excluded package(s) \`ressac-kde\` not found in workspace …` and exit 0. So the Ubuntu job is correct before and after B1a, and B1a does not touch `ci.yml`. A comment above the two commands says why: "ressac-kde needs Qt 6 and runs on Fedora only (job fedora); the warning before it exists is expected." `cargo fmt --all --check` keeps `--all` (it parses, it does not compile).
- **The Fedora job uses `container: fedora:44`** (the runner's architecture, x86_64; marghe's aarch64 is covered by `dev.sh check`), installs `packages.txt` (one list, shared with the dev container), creates user `ressac`, gives it the checkout, and runs `runuser -u ressac -- bash scripts/check-linux.sh`. Not allowed to fail. No cache in this phase (a cold build fits the 60-minute limit; add one later if it is slow).
- **Release on Ubuntu 22.04** for both architectures (`ubuntu-22.04`, `ubuntu-22.04-arm`, both listed in `actions/runner-images` today): glibc 2.35 floor. Rust through `rustup` (installed first when the image lacks it), stable toolchain, `cargo build --release --locked -p ressac`. The repository is private: if `ubuntu-22.04-arm` is not offered to it, the job waits in the queue; that is checked at the first tag (A6 `TODO.md` line), not guessed here.
- **Least privilege.** Top-level `permissions: contents: read`; only `publish` has `contents: write` and the token. The build jobs only upload artifacts.
- **Actions versions** (latest majors checked on 2026-10-23 with `gh api repos/<action>/releases/latest`): `actions/checkout@v7` (as today), `actions/upload-artifact@v7`, `actions/download-artifact@v8` (both `node24`; v8 has `pattern` and `merge-multiple`).
- **Checksums** are computed once, in `publish`, with `sha256sum` over every archive, in the format `shasum -a 256` gave (`<hash>  <file name>`, no `./`).

**Step 0:** not applicable (YAML).

**Gate (end of phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
actionlint                                                # no output, exit 0 (reads .github/actionlint.yaml)
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

---

### Task 1: `actionlint` clean on the current workflows

**Files:**
- Create: `.github/actionlint.yaml`
- Modify: `.github/workflows/ci.yml:38-45` (the `ls -d /Applications/Xcode_…` line), `.github/workflows/release.yml:37-44` (same line)

**Interfaces:**
- Consumes: nothing.
- Produces: a lint baseline with zero findings, so Tasks 2 and 3 read their own findings only.

- [ ] **Step 1: See the baseline fail**

Run: `actionlint`
Expected now (exit 1): `label "xcode-27" is unknown` at `ci.yml:29:14` and `release.yml:14:14`, and `SC2012:info` at `ci.yml:38:9` and `release.yml:37:9`.

- [ ] **Step 2: Fix**
  - `.github/actionlint.yaml`:
    ```yaml
    # GitHub's macOS 27 preview image (runs-on: xcode-27), unknown to actionlint 1.7.12.
    self-hosted-runner:
      labels:
        - xcode-27
    ```
  - In both "Select / Check for Xcode 27" scripts, replace `candidate=$(ls -d /Applications/Xcode_"$required"*.app 2>/dev/null | sort -V | tail -n 1 || true)` with `candidate=$(find /Applications -maxdepth 1 -name "Xcode_${required}*.app" 2>/dev/null | sort -V | tail -n 1 || true)` (same result: absolute paths, highest version last).

- [ ] **Step 3: Verify**

Run: `actionlint`
Expected: no output, exit 0.

- [ ] **Step 4: Commit**

```bash
git add .github/actionlint.yaml .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "CI: actionlint clean (xcode-27 label declared, find instead of ls)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: `ci.yml`: Ubuntu without `ressac-kde`, a Fedora job

**Files:**
- Modify: `.github/workflows/ci.yml` (`check` job's clippy and test lines; new `fedora` job between `check` and `macos`)

**Interfaces:**
- Consumes: A4's `packages.txt` and `check-linux.sh`.
- Produces: job `fedora`.

- [ ] **Step 1: Write the change**

In `check`: `cargo clippy --workspace --exclude ressac-kde --all-targets -- -D warnings` and `cargo test --workspace --exclude ressac-kde`, under the comment of the header. The new job:

```yaml
  fedora:
    name: Fedora 44 (Linux gate, non-root)
    runs-on: ubuntu-latest
    container: fedora:44
    timeout-minutes: 60
    steps:
      # The checkout action needs git inside the container to make a real clone.
      - name: Install git
        run: dnf install -y git-core
      - uses: actions/checkout@v7
        with:
          persist-credentials: false
      - name: Install the dev container's packages
        run: dnf install -y $(sed 's/#.*//' apps/linux/dev/packages.txt)
      # The tests and, from B1a on, QtWebEngine's sandbox need a non-root user (spec §8.1).
      - name: Create the non-root user
        run: |
          useradd --create-home --user-group ressac
          chown -R ressac:ressac "$GITHUB_WORKSPACE"
      - name: Linux gate (fmt, clippy, tests, chart, layout smoke)
        run: runuser -u ressac -- bash scripts/check-linux.sh
```

Reading checklist (tick each in the review): the job has no `continue-on-error`; `$(sed …)` is unquoted on purpose (word splitting into package names; if `actionlint`'s shellcheck flags SC2046, add `# shellcheck disable=SC2046` on the line above inside the script block, not a global ignore); `runuser` keeps `GITHUB_*` and sets `HOME=/home/ressac`, so `check-linux.sh` builds into `/home/ressac/target`, outside the checkout; `container:` jobs run steps with `bash` when it exists (Fedora has it).

- [ ] **Step 2: Lint**

Run: `actionlint`
Expected: no output, exit 0.

- [ ] **Step 3: Replay the job locally** (OrbStack, x86_64 is not available natively: replay on the Mac's arm64, which tests the script, not the architecture)

```bash
docker run --rm --name ressac-ci-replay -v "$PWD:/src:ro" fedora:44 bash -euo pipefail -c '
  dnf install -y git-core tar >/dev/null
  mkdir /checkout && tar -C /src --exclude=./target --exclude=./apps/macos/build --exclude=./.git -cf - . | tar -C /checkout -xf - && cd /checkout
  dnf install -y $(sed "s/#.*//" apps/linux/dev/packages.txt) >/dev/null
  useradd --create-home --user-group ressac && chown -R ressac:ressac /checkout
  runuser -u ressac -- bash scripts/check-linux.sh'
echo "exit=$?"
```

Expected: last lines `check-linux: OK (fmt, clippy, test, chart, layout smoke)` and `exit=0` (about 8–12 min: dnf, then a cold build). The container name is new and `--rm`; `docker ps -a` afterwards shows no `ressac-ci-replay`, and `fsvbuild` / `gotenberg` unchanged. Record the duration in the ledger.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "CI: Fedora 44 job running the Linux gate as a non-root user; Ubuntu job without ressac-kde

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 3: `release.yml`: version, macos, linux, publish

**Files:**
- Modify: `.github/workflows/release.yml` (whole file restructured; the Xcode selection, build and packaging steps keep their text)

**Interfaces:**
- Consumes: nothing new.
- Produces: the jobs, artifacts and release files of the header.

- [ ] **Step 1: Write the change**

- Top level: `permissions: contents: read` (was `write`); `on` and `concurrency` unchanged.
- **`version`** (`ubuntu-latest`, 5 min): checkout, then today's "Check that the tag matches the versions" step unchanged (id `version`); job `outputs: version: ${{ steps.version.outputs.version }}`.
- **`macos`** (`xcode-27`, 60 min, `needs: version`): today's steps from checkout to "Build the app", then "Package" without the `SHA256SUMS` line (`codesign --verify`, `--version`, `ditto`, `tar`), `VERSION: ${{ needs.version.outputs.version }}`; then `actions/upload-artifact@v7` with `name: macos`, `path: ${{ runner.temp }}/dist/*`, `if-no-files-found: error`, `retention-days: 7`. The tag check step leaves this job.
- **`linux`** (`needs: version`, 45 min):

```yaml
  linux:
    name: Linux CLI (${{ matrix.arch }})
    needs: version
    strategy:
      matrix:
        include:
          - arch: x86_64
            runner: ubuntu-22.04
          - arch: aarch64
            runner: ubuntu-22.04-arm
    runs-on: ${{ matrix.runner }}
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v7
        with:
          persist-credentials: false
      # Ubuntu 22.04 keeps the glibc floor at 2.35, so the binary runs on most distributions.
      - name: Install Rust
        run: |
          if ! command -v rustup >/dev/null; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
            echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
            export PATH="$HOME/.cargo/bin:$PATH"
          fi
          rustup toolchain install stable --profile minimal
          rustup default stable
      - name: Build
        run: cargo build --release --locked -p ressac
      - name: Smoke test
        shell: bash
        run: |
          set -euo pipefail
          ./target/release/ressac --version
          db="$(mktemp -d)/t.sqlite"
          HOME="$(mktemp -d)" ./target/release/ressac --db "$db" --json migrate
          echo "Highest glibc symbol: $(objdump -T target/release/ressac | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -n 1)"
      - name: Package
        shell: bash
        env:
          VERSION: ${{ needs.version.outputs.version }}
          ARCH: ${{ matrix.arch }}
        run: |
          set -euo pipefail
          mkdir -p "$RUNNER_TEMP/dist"
          tar -czf "$RUNNER_TEMP/dist/ressac-$VERSION-linux-$ARCH.tar.gz" -C target/release ressac
      - uses: actions/upload-artifact@v7
        with:
          name: linux-${{ matrix.arch }}
          path: ${{ runner.temp }}/dist/*.tar.gz
          if-no-files-found: error
          retention-days: 7
```

- **`publish`** (`ubuntu-latest`, 10 min, `needs: [version, macos, linux]`, `permissions: contents: write`): `actions/download-artifact@v8` with `path: dist`, `merge-multiple: true`; a step that, with `VERSION`, checks each of the four file names of the header exists in `dist/` (`::error title=Missing artifact::<name>` and exit 1 otherwise), then `(cd dist && sha256sum ./*.zip ./*.tar.gz | sed "s|  ./|  |" > SHA256SUMS)` and `cat dist/SHA256SUMS`; then today's "Publish the release" step with `"dist"/*.zip "dist"/*.tar.gz dist/SHA256SUMS` as files and these notes (Mac bullets unchanged; the Linux bullet new; the last bullet reworded):

```markdown
**Unsigned build.** Ressac is signed ad hoc, not with a Developer ID, and not notarized. macOS 27 on Apple silicon only.

- **App:** unzip `Ressac-…-macos-arm64.zip`, move `Ressac.app` to `/Applications/`, then clear the quarantine flag once: `xattr -dr com.apple.quarantine /Applications/Ressac.app` (or open it, then allow it under System Settings ▸ Privacy & Security).
- **Terminal:** `ressac-…-macos-arm64.tar.gz` holds the same `ressac` binary the app embeds; `tar -xzf` it somewhere on your `PATH` and run `xattr -d com.apple.quarantine ressac`.
- **Linux (terminal only):** `ressac-…-linux-x86_64.tar.gz` and `ressac-…-linux-aarch64.tar.gz` hold the `ressac` binary, built on Ubuntu 22.04 (glibc 2.35 or newer). `tar -xzf` it into a directory on your `PATH`, such as `~/.local/bin`. Links open through `xdg-open`; files live under `~/.local/share/ressac`, `~/.config/ressac` and `~/.local/state/ressac` (see the README's Linux section).
- `SHA256SUMS` lists every archive's checksum.
```

Reading checklist: every `needs.version.outputs.version` spelled the same; `publish` is the only job with `GH_TOKEN` and `contents: write`; no job uses `continue-on-error`; the macOS job no longer says "this job can write to the repository" in its Rust comment (reword to "rustup directly rather than a third-party action, as in every release job"); a pre-release tag still gets `--prerelease`.

- [ ] **Step 2: Lint**

Run: `actionlint`
Expected: no output, exit 0.

- [ ] **Step 3: Replay the aarch64 Linux job locally**

```bash
docker run --rm --name ressac-release-replay --platform linux/arm64 -v "$PWD:/src:ro" ubuntu:22.04 bash -euo pipefail -c '
  apt-get update -qq && apt-get install -y -qq build-essential curl ca-certificates cmake clang binutils >/dev/null
  mkdir /w && tar -C /src --exclude=./target --exclude=./apps/macos/build --exclude=./.git -cf - . | tar -C /w -xf - && cd /w
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null
  export PATH="$HOME/.cargo/bin:$PATH"
  cargo build --release --locked -p ressac
  ./target/release/ressac --version
  HOME="$(mktemp -d)" ./target/release/ressac --db "$(mktemp -d)/t.sqlite" --json migrate
  objdump -T target/release/ressac | grep -o "GLIBC_[0-9.]*" | sort -Vu | tail -n 1
  tar -czf /tmp/ressac-0.0.0-linux-aarch64.tar.gz -C target/release ressac && tar -tzf /tmp/ressac-0.0.0-linux-aarch64.tar.gz'
echo "exit=$?"
```

Expected: `ressac 0.2.0` (the workspace version), one JSON line with `"event":"finished"`, a glibc symbol `GLIBC_2.35` or lower, `ressac` listed by `tar -tzf`, `exit=0`. A failure caused by a package the bare image lacks but GitHub's runner image has (the runner ships build-essential, cmake, clang) is not a workflow bug: note it in the ledger and add that package to this replay command, not to the workflow. Record the build time.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "Release: version, macos, linux, publish jobs; Linux CLI tarballs for x86_64 and aarch64

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
