# Release names and the KDE app RPM: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** from the next tag on, `release.yml` publishes `Ressac-<v>-macos-arm64.zip`, `Ressac-cli-<v>-{macos-arm64,linux-x86_64,linux-aarch64}.tar.gz` and `Ressac-<v>-linux-{x86_64,aarch64}.rpm` (the KDE app and the terminal app for Fedora), all in `SHA256SUMS`; the README says how to install each.

**Architecture:** two shell scripts in `apps/linux/packaging/` do the RPM work, so CI and a local replay run the same code: `build-rpm.sh` (cargo release build, `rpmbuild -bb` of a spec that packages the prebuilt binaries, copy to the release name) and `verify-rpm.sh` (install the RPM in a fresh Fedora container and prove the app starts offscreen). `release.yml` gains a `linux-app` job (matrix x86_64/aarch64, `container: fedora:44`) that calls them; the archive names change in the existing jobs.

**Tech Stack:** bash, `rpm-build` (`rpmbuild`), `desktop-file-utils`, GitHub Actions (`fedora:44` containers on `ubuntu-latest` and `ubuntu-22.04-arm`), OrbStack/Docker for the local replay (`--platform linux/arm64` native, `linux/amd64` through Rosetta), `actionlint`.

**Spec:** `docs/specs/2026-11-26-release-rpm-design.md` (approved 27/11/2026; Clément added: update `README.md`).

## Global Constraints

- File names, exactly: `Ressac-<v>-macos-arm64.zip`, `Ressac-cli-<v>-macos-arm64.tar.gz`, `Ressac-cli-<v>-linux-x86_64.tar.gz`, `Ressac-cli-<v>-linux-aarch64.tar.gz`, `Ressac-<v>-linux-x86_64.rpm`, `Ressac-<v>-linux-aarch64.rpm`, `SHA256SUMS`. The archives keep a single `ressac` binary at their top.
- RPM package name `ressac`; contents exactly `/usr/bin/ressac`, `/usr/bin/ressac-kde`, `/usr/share/applications/org.w3cdotorg.ressac.desktop`, `/usr/share/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg`, plus the licence files under `/usr/share/licenses/ressac/`.
- `License: MIT AND CC0-1.0 AND OFL-1.1 AND ISC` (workspace MIT; Bedstead CC0; Newsreader, Atkinson Hyperlegible Next, Cormorant Garamond, Cormorant SC, IBM Plex Mono OFL; d3 ISC, all compiled into `ressac-kde`). `%license` ships `apps/macos/Ressac/Resources/Fonts/*-OFL.txt`, `Bedstead-CC0.txt`, `apps/macos/Ressac/Resources/Chart/LICENSE-d3.txt`, `apps/macos/Ressac/Resources/Chart/fonts/*-OFL.txt`.
- Built on `fedora:44`, one RPM per architecture; RPM `Release: 1%{?dist}`.
- Nothing under `apps/macos/` changes; `apps/linux/install.sh` does not change.
- Never: push, open a PR, merge or tag without Clément; `dnf install` on marghe's VM without his go-ahead; the real database; the `fsvbuild`, `gotenberg` or `wc4*` containers.
- README in English; `TODO.md` and `notes.md` in French, no em dash in French prose.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb`.
- Gate at the end: `actionlint` clean; `cargo fmt --all --check`; `bash -n` and `shellcheck` on the two scripts; the local replay of Task 1 green on aarch64 (x86_64 too unless Rosetta emulation makes it impractical: then say so).

## Review Focus

1. **A QML module or WebEngine missing at run time** (rpm sees only libraries, not QML imports): a fresh `fedora:44` with only the RPM's dependencies must start `ressac-kde` offscreen. Test: `verify-rpm.sh` runs the app's `--screenshot` walk on the contract fixture as a non-root user and requires exit 0 and `01-ready.png`.
2. **A pre-release tag** (`v0.4.0-beta.1`): RPM `Version` cannot hold `-`. `build-rpm.sh` maps `-` to `~` (RPM's pre-release ordering) for `Version`, while the file name keeps the tag's version. Test: Task 1 step runs `build-rpm.sh 0.0.0-beta.1 …` once and checks `rpm -qp --qf '%{VERSION}'` is `0.0.0~beta.1` and the file is `Ressac-0.0.0-beta.1-linux-<arch>.rpm`.
3. **The user already has the `install.sh` copy in `~/.local`**: its desktop entry has the same id and wins over the RPM's in the Plasma launcher. The README's RPM paragraph says to run `apps/linux/install.sh --uninstall` first; Task 3 does it on the VM before installing.
4. **`publish` without an RPM**: the existing missing-artifact check lists all six files. Test: Task 2's diff review plus `actionlint`; the list is one array in `publish`.
5. **Removing the package**: `dnf remove ressac` leaves `~/.local/share/ressac/` and `~/.config/ressac/` alone (the RPM owns only `/usr` files). Test: `verify-rpm.sh` creates a database in the test user's home, removes the package, and checks the database is still there.

---

### Task 1: `apps/linux/packaging/` (spec, desktop entry, build and verify scripts) and a local replay

**Files:**
- Create: `apps/linux/packaging/ressac.spec`
- Create: `apps/linux/packaging/org.w3cdotorg.ressac.desktop`
- Create: `apps/linux/packaging/build-rpm.sh`
- Create: `apps/linux/packaging/verify-rpm.sh`

**Interfaces:**
- Produces: `apps/linux/packaging/build-rpm.sh VERSION OUTDIR`: run from the repository root inside a Fedora 44 container that has `packages.txt` plus `rpm-build desktop-file-utils` installed; builds `cargo build --release --locked -p ressac -p ressac-kde`, runs `rpmbuild -bb` with `--define "_topdir $tmp" --define "ressac_version <VERSION with - → ~>" --define "ressac_srcdir $PWD"`, and writes `OUTDIR/Ressac-VERSION-linux-$(uname -m).rpm`; prints that path last. `apps/linux/packaging/verify-rpm.sh RPM SRCDIR`: run as root in a fresh `fedora:44` container where `SRCDIR` is a read-only copy of the repository (the fixture comes from it); exits 0 only if every check below passes.
- The desktop entry has the same keys as the one `install.sh` writes (`apps/linux/install.sh:100-110`), with `Exec=ressac-kde` and no absolute path.

- [ ] **Step 1: Write `verify-rpm.sh` first** (it is the test). Checks, in order, each printing `verify: <name> ok` or failing with a message:
  1. `dnf install -y "$RPM"` succeeds (pulls the dependencies).
  2. `rpm -ql ressac` equals the Global Constraints file list plus the `%license` files (sorted comparison).
  3. `ressac --version` prints `ressac <VERSION>`.
  4. `ldd /usr/bin/ressac-kde | grep 'not found'` prints nothing.
  5. `desktop-file-validate /usr/share/applications/org.w3cdotorg.ressac.desktop` passes.
  6. As a new non-root user `verify`: copy `$SRCDIR/tests/fixtures/contract.sqlite` to its home, run `QT_QPA_PLATFORM=offscreen RESSAC_DB=<copy> RESSAC_RECETTE_WALK=library timeout 120 ressac-kde --screenshot <dir>` (pick the shortest existing walk that loads the reading page; read `apps/linux/qml/Main.qml`'s chooser), exit 0 and `<dir>/01-ready.png` (or the walk's first shot) exists and is non-empty.
  7. `dnf remove -y ressac`; `/usr/bin/ressac-kde` is gone and the user's database copy is still there.
- [ ] **Step 2: Run it against nothing to see it fail**: in a throwaway container, `docker run --rm --platform linux/arm64 -v "$PWD":/src:ro fedora:44 bash /src/apps/linux/packaging/verify-rpm.sh /nonexistent.rpm /src` → fails at check 1.
- [ ] **Step 3: Write `ressac.spec`, the desktop entry and `build-rpm.sh`** to the Interfaces above. The spec has no `%build` (binaries are prebuilt) and `%install` copies from `%{ressac_srcdir}`; `Requires:` covers what rpm's automatic library dependencies miss: the QML modules and runtime pieces from `apps/linux/dev/packages.txt` (`kf6-kirigami`, `kf6-qqc2-desktop-style`, `qt6-qtwebengine`, `qt6-qtwebchannel`, `qt6-qtdeclarative`), and `Recommends: breeze-icon-theme` (decide each from what the app loads at run time; justify in the report).
- [ ] **Step 4: Local replay, aarch64**: in a throwaway `fedora:44` container (`--platform linux/arm64`) with a copy of the tree (not the dev container, not a bind mount the build writes into), install `packages.txt` plus `rpm-build desktop-file-utils`, run `build-rpm.sh 0.3.0 /out` as a non-root user; then `verify-rpm.sh /out/Ressac-0.3.0-linux-aarch64.rpm /src` in a second, fresh `fedora:44` with the tree mounted read-only at `/src`. Both exit 0, all seven `verify:` lines `ok`. Keep the RPM at `.superpowers/sdd/release-rpm/Ressac-0.3.0-linux-aarch64.rpm` for Task 3.
- [ ] **Step 5: Pre-release version (Review Focus 2)**: `build-rpm.sh 0.0.0-beta.1 /out` → the file name and `rpm -qp --qf '%{VERSION}\n'` = `0.0.0~beta.1`.
- [ ] **Step 6: Local replay, x86_64** (`--platform linux/amd64`, OrbStack's Rosetta): same as Step 4. If it is impractical (over 45 minutes or an emulation failure unrelated to the scripts), stop, record why, and rely on CI.
- [ ] **Step 7: `shellcheck` and `bash -n` on both scripts; commit**

```bash
git add apps/linux/packaging/
git commit -m "Linux: RPM packaging for the KDE app (spec, desktop entry, build and verify scripts)" # + the two attribution lines
```

### Task 2: `release.yml` (new names, `linux-app` job, six-file publish), README and TODO

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `README.md` (line 123's terminal install sentence; the « Releases » paragraph at line 221; « The Linux app (KDE) » section's **Install** at line 156)
- Modify: `TODO.md` (sub-project 8 line: PR #19 merged on 26/11/2026, release 0.3.0; the « Au premier tag » line ticked with the 0.3.0 facts: all jobs green, `ubuntu-22.04-arm` offered, checksums OK, glibc 2.34)

**Interfaces:**
- Consumes: Task 1's two scripts, as specified.
- Produces: release artifacts named per Global Constraints; job ids `version`, `macos`, `linux`, `linux-app`, `linux-app-verify`, `publish`; artifact names `linux-app-x86_64`, `linux-app-aarch64`.

- [ ] **Step 1: Rename** in `macos` and `linux` jobs: `ressac-$VERSION-macos-arm64.tar.gz` → `Ressac-cli-$VERSION-macos-arm64.tar.gz`, `ressac-$VERSION-linux-$ARCH.tar.gz` → `Ressac-cli-$VERSION-linux-$ARCH.tar.gz`; the `.zip` is unchanged.
- [ ] **Step 2: Add two jobs.** `linux-app`: `needs: version`, matrix `x86_64`/`ubuntu-latest` and `aarch64`/`ubuntu-22.04-arm`, `container: fedora:44`, `timeout-minutes: 60`; steps: install `git-core` (checkout needs it, as in `ci.yml`'s `fedora` job), checkout, `dnf install -y $(sed 's/#.*//' apps/linux/dev/packages.txt) rpm-build desktop-file-utils`, create a non-root user and give it the checkout (as `ci.yml` does), run `build-rpm.sh "$VERSION" "$RUNNER_TEMP/dist"` as that user, upload `linux-app-${{ matrix.arch }}` with `if-no-files-found: error`. `linux-app-verify`: `needs: [version, linux-app]`, same matrix and runners, `container: fedora:44` (a fresh one, so the RPM's dependencies are proven without the build packages); steps: install `git-core`, checkout, download `linux-app-${{ matrix.arch }}`, run `verify-rpm.sh <the .rpm> "$GITHUB_WORKSPACE"`.
- [ ] **Step 3: `publish`**: `needs` gains `linux-app-verify`; the missing-artifact list has the six files of Global Constraints (one bash array); `sha256sum` covers `*.zip *.tar.gz *.rpm`; the release upload includes the RPMs.
- [ ] **Step 4: `actionlint`** → no output, exit 0.
- [ ] **Step 5: README**: line 123 and the « Releases » paragraph use the new names and list the RPMs; « The Linux app (KDE) » **Install** starts with the RPM route (`sudo dnf install ./Ressac-X.Y.Z-linux-<arch>.rpm`, Fedora 44 or newer; remove an earlier `install.sh` copy first with `apps/linux/install.sh --uninstall`, because its launcher entry would hide the RPM's; `sudo dnf remove ressac` uninstalls and keeps the database), then keeps the from-a-clone route as it is.
- [ ] **Step 6: TODO.md** lines as in **Files**.
- [ ] **Step 7: Commit**

```bash
git add .github/workflows/release.yml README.md TODO.md
git commit -m "Release: Ressac-cli archive names and the KDE app RPMs for Fedora" # + the two attribution lines
```

### Task 3: the RPM on marghe (stops for Clément)

**Files:** none (a ledger line; `notes.md` gets a short « RPM de l'appli KDE (27/11/2026) » paragraph with the outcome, committed with Task 2's files if Task 2 is not yet committed, else its own commit).

- [ ] **Step 1**: copy Task 1's aarch64 RPM to the VM (`scp` to marghe, then `prlctl exec` as in `apps/linux/dev/marghe.sh`), into `/tmp/`.
- [ ] **Step 2: ⏸ STOP, ask Clément** for the go-ahead to run on the VM: `~/.local/bin` copy removed with `apps/linux/install.sh --uninstall` from `~/src/ressac` (his data untouched), then `sudo dnf install /tmp/Ressac-0.3.0-linux-aarch64.rpm`. The `sudo` needs his password or `prlctl exec` as root: ask which.
- [ ] **Step 3**: after his go-ahead, run them; he launches Ressac from the Plasma launcher and checks it opens on his database. Record his answer; leave the RPM installed or remove it as he prefers.
- [ ] **Step 4**: `notes.md` paragraph (French, no em dash): what was built, the seven verify checks, the VM result.
