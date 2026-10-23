# Phase B10: smoke test, complete walk, final review, README, recette on marghe, push and PR

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** sub-project 8 is finished and handed over: an offscreen smoke test reaches every page of the app and checks the database writes, `--screenshot` walks every state the spec lists, a whole-branch review by the most capable model finds nothing left that matters, the Swift suite still passes, the README explains the Linux app, Clément runs the recette on marghe, and the branch is pushed with a pull request only after his go-ahead. **Never merged by Claude.**

**Runs after:** B9 (CRT kept or cut). Master plan `docs/plans/2026-10-23-linux.md` (row B10; Review Focus 1–5; Global Constraints, all of them; Execution notes: "A final whole-branch review before B10's push"). Spec §1 (the success scenario), §4 (scope, out of scope, anti-goals), §7.11, §8.2, §8.3, §8.4, §9 (stops for Clément), §10. Inventory §10 (what the tests mirror) and §11 (known gaps: decided in the phases). Ledger `.superpowers/sdd/2026-10-23-linux/progress.md` (deviations, deferred minors, the B8e and B9 recette checklist items). Every phase file in `docs/plans/linux/`.

**Architecture:** no new module. B10a adds the smoke walk and completes the default walk in `Main.qml` with one Rust test; B10b is review and fix rounds; B10c is documentation and the one Swift run; B10d is Clément's recette on his VM with his own database; B10e is the push and the PR.

**Tech Stack:** as the earlier phases; `xcodegen` and `xcodebuild` on the Mac for the Swift suite; `gh` for the PR; marghe's VM through `apps/linux/dev/marghe.sh`.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md`.

## Deviations from the master plan

1. **Five sub-phases** (B10a smoke and walk, B10b final review and its fix rounds, B10c docs and the Swift run, B10d recette, B10e push and PR), each within the 5-file cap; a fix round is its own commit of at most 5 files.
2. **The complete walk is the default `--screenshot` walk** (spec §8.2): B10a appends the states earlier phases left to named walks (garden, Antiope pages, Sea Chart, Settings), so `ressac-kde --screenshot <dir>` alone shows everything; the named walks stay for their tests.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `apps/linux/qml/Main.qml` | B10a T1 | `smokeSteps` (walk `smoke`), the default walk's closing states |
| `README.md` | B10c T3 | "Linux app" section; the `## Linux` intro no longer says "on its way" |
| `notes.md` | B10c T3; B10d T4 | « Bilan de la partie B », the Swift run; « Recette finale (B10) » |
| `TODO.md` | B10c T3; B10d T4 | the sub-project 8 line; deferred minors; recette anomalies |
| (fix rounds) | B10b T2, B10d T4 | whatever the review or the recette points at, ≤ 5 files per round |
| tests (not counted) | | `apps/linux/tests/smoke.rs` (new); existing tests whose exact shot lists grow |

## Interfaces

- **Consumes:** every walk and hook of B1–B9 (`Recette.{grab, key, walk, frameLogStart, frameLogStop}`, `Main.qml`'s chooser and `walkPaused`, `tests/common`), the models' invokables, `apps/linux/install.sh`, `apps/linux/dev/{dev.sh, marghe.sh}`.
- **Produces:** walk `smoke`; the final README; the PR.

## Global Constraints

- Every Global Constraint of the master plan holds at the end; the final review checks each one (Task 2's list).
- **Never:** touch the `fsvbuild`, `gotenberg` or `wc4*` containers; use the real database (Claude never reads or exports Clément's main-Mac database; the recette uses the VM's own); the network in tests; `dnf install` on the VM without Clément's go-ahead; **push, open a PR or merge without Clément; merge at all.**
- **Commits:** one per task or fix round on branch `linux`, messages ending with the two attribution lines.
- **Gates (end of each sub-phase that changes files):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

## Review Focus

1. **A first launch on marghe with no database** (spec §1): the window offers to import an OPML file, the import fetches, nothing is written next to the working directory, the database lands in `~/.local/share/ressac/`. Test: B10d's recette item 1; the smoke run on an empty `XDG_DATA_HOME` (Task 1 `the_first_run_on_an_empty_home_offers_the_import`).
2. **The app and `ressac` in Konsole on one database at once** (master Review Focus 3): marking read while `ressac fetch` runs waits, never freezes, and reverts with the busy sentence on a timeout. Tests: B2b's test (re-run in the gate) and recette item 8.
3. **Scale 2 everywhere** (spec §2b): every window drawn at device pixel ratio 2 in the smoke run; Antiope's cells whole device pixels. Test: Task 1 runs the smoke walk with `QT_SCALE_FACTOR=2`.
4. **The Mac side unchanged** (spec anti-goal 1, master "macOS frozen"): no file under `apps/macos/` changed on the branch, `reader.css` and the chart page read in place, the Swift suite green. Tests: Task 2's diff check, Task 3's Swift run.
5. **Qt-free Mac build** (anti-goal 4): `cargo test --workspace` on the Mac without Qt, the Ubuntu CI job's `--exclude ressac-kde`. Tests: the gate itself; Task 2 checks `default-members` and the `cfg(target_os = "linux")` dependency tables.

---

## B10a: smoke test and the complete walk

### Task 1 (B10a): walk `smoke`, the default walk completed, `tests/smoke.rs`

**Files:**
- Modify: `apps/linux/qml/Main.qml` (Step 0: remove its dead code first in its own commit, or record "no dead code" in the ledger)
- Test (not counted): `apps/linux/tests/smoke.rs` (new); `apps/linux/tests/*.rs` whose exact list of default-walk shots grows

**Interfaces:**
- Consumes: as above.
- Produces: `smokeSteps`; the default walk's closing states.

`smokeSteps` (walk `smoke`, each step one shot and one `walk: smoke-<name> …` line; the copy has feeds pointing at `127.0.0.<id>:9`):

| Step | State | Log (key values) |
|---|---|---|
| `s01-native` | Native, Unread selected, first entry selected, reader loaded | `entries=<n> page=ressac:page/<id>-` |
| `s02-keys` | `Recette.key` `j`, `m`, `s` in the entries list | `selected=<id2> read=<id1>,<id2>? starred=<id2>` |
| `s03-journal` | Journal | `journal=true` |
| `s04-garden` | Needs Triage, first feed, inspector | `garden=true inspector=<title>` |
| `s05-antiope` | Antiope, P101 | `look=antiope page=101` |
| `s06-p201` | digits `2 0 1` | `page=201` |
| `s07-p100`, `s08-p899`, `s09-p104`, `s10-p800` | Alt+S, `?`, dial 104, dial 800 | `reader=index`, `reader=guide`, `selected=journal`, `selected=garden:needs-triage` |
| `s11-chart` | `openChart()` (B7), wait | `chart=drawn` (B7's draw probe) |
| `s12-settings` | Settings window, Reading page | `appearance=antiope` |
| `s13-native-again` | Native | `look=native` |

The default walk (no `RESSAC_RECETTE_WALK`) appends, after its last existing step, `90-garden` (Needs Triage with the inspector), `91-antiope-101`, `92-antiope-100`, `93-antiope-899`, `94-antiope-104`, `95-antiope-800`, `96-chart`, `97-settings`, `98-native`, unless an earlier phase already shows that state in the default walk (then reuse it; never duplicate).

- [ ] **Step 1: Write the failing test** `apps/linux/tests/smoke.rs`:

```rust
//! The offscreen smoke test (spec §8.3): every page, every window drawn, keys reach the database.
#![cfg(target_os = "linux")]
mod common;
use common::*;

const SHOTS: [&str; 13] = ["s01-native", "s02-keys", "s03-journal", "s04-garden", "s05-antiope", "s06-p201",
    "s07-p100", "s08-p899", "s09-p104", "s10-p800", "s11-chart", "s12-settings", "s13-native-again"];

fn smoke(scale: &str) {
    let run = Run::new();
    let db = run.copy_fixture();
    unreachable_feeds(&db);                                        // feeds at 127.0.0.<id>:9: no traffic leaves the box
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "smoke"), ("QT_SCALE_FACTOR", scale)]);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for shot in SHOTS {
        let png = run.shots().join(format!("{shot}.png"));
        if shot == "s12-settings" { assert_drawn_min(&png, 500, 300) } else { assert_drawn(&png) }
        let (w, _) = image::image_dimensions(&png).unwrap();
        assert!(scale == "1" || w >= 1800, "{shot} drawn at device pixel ratio {scale}: {w} px wide");
    }
    for line in ["walk: smoke-s05-antiope look=antiope page=101", "walk: smoke-s06-p201 page=201",
                 "walk: smoke-s07-p100 reader=index", "walk: smoke-s08-p899 reader=guide", "walk: smoke-s09-p104 selected=journal",
                 "walk: smoke-s10-p800 selected=garden:needs-triage", "walk: smoke-s11-chart chart=drawn", "walk: smoke-s13-native-again look=native"] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    let starred: i64 = conn.query_row("SELECT COUNT(*) FROM entry WHERE starred_at IS NOT NULL", [], |r| r.get(0)).unwrap();
    let fixture: i64 = rusqlite::Connection::open(fixture_path()).unwrap()
        .query_row("SELECT COUNT(*) FROM entry WHERE starred_at IS NOT NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(starred, fixture + 1, "s starred one entry");
    // j marked the entry it left read; m toggled the next one (the exact ids are read from the s02 line).
}

#[test] fn the_smoke_walk_reaches_every_page_at_scale_1() { smoke("1") }
#[test] fn the_smoke_walk_reaches_every_page_at_scale_2() { smoke("2") }

#[test]
fn the_default_walk_shows_every_state_the_spec_lists() {
    let run = Run::new();
    let out = run.launch(Some(&run.copy_fixture()));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    for shot in ["90-garden", "91-antiope-101", "92-antiope-100", "93-antiope-899", "94-antiope-104", "95-antiope-800", "96-chart", "97-settings", "98-native"] {
        assert_drawn_min(&run.shots().join(format!("{shot}.png")), 500, 300);   // or the earlier phase's equivalent shot
    }
}

#[test]
fn the_first_run_on_an_empty_home_offers_the_import() {
    let run = Run::new();                                           // its own empty XDG_DATA_HOME, no RESSAC_DB
    let out = run.launch_with(None, &[("RESSAC_RECETTE_WALK", "first-run")]);   // B6d's walk
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(run.data_home().join("ressac/ressac.sqlite").exists());
    assert!(!run.cwd().join("ressac.sqlite").exists(), "nothing next to the working directory");
}
```

  (`unreachable_feeds`, `fixture_path`, `run.data_home()`, `run.cwd()` in `tests/common/mod.rs` if missing; replace the `s02` comment by assertions on the ids the step logs.)
- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test smoke 2>&1 | grep -E "panicked|test result"'` → FAIL (no `smoke` walk).
- [ ] **Step 3: Implement** the steps (Step 0 commit first).
- [ ] **Step 4: Verify and look** — the same command → `ok.`; every suite (`cargo test --locked -p ressac-kde`) → every `ok.` (grow exact shot lists in older tests if any compares the whole default list). Open `s05-antiope.png`, `s11-chart.png`, `s12-settings.png` at scale 2. Then the full gate (B10a ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/Main.qml apps/linux/tests/smoke.rs apps/linux/tests/common/mod.rs
git commit -m "ressac-kde: offscreen smoke test over every page and window, complete screenshot walk

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B10b: final whole-branch review

### Task 2 (B10b): review by the most capable model, then fix rounds

**Files:** none in the review itself; each fix round lists its own (≤ 5).

**Interfaces:**
- Consumes: the whole branch.
- Produces: a clean verdict in the ledger.

- [ ] **Step 1: Mechanical checks** (record each result in the ledger):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
base=$(git merge-base main linux)
git diff --stat "$base"..linux -- apps/macos/                        # empty: the Mac app is untouched
git diff "$base"..linux -- crates/ressac-core/src/store/schema* | head   # no schema change (v6), whatever the file name
grep -rn "QtSql\|QSqlDatabase\|libsqlite" apps/linux/ || echo "no second SQLite"   # anti-goal 2
grep -n "default-members" Cargo.toml                                # ressac-kde absent from default-members
grep -n "target_os = \"linux\"" apps/linux/Cargo.toml               # every Qt dependency Linux-only
grep -rn "unwrap()\|expect(" apps/linux/src crates/ressac-ui/src --include=*.rs | grep -v "mod tests\|#\[cfg(test)\]\|test_db" | wc -l   # each one justified or removed
grep -rn "—" notes.md TODO.md | grep -v "^\s*-\s*\`" | head            # no em dash in French prose (code and quotes excepted)
cargo test --workspace 2>&1 | grep -c "test result: ok"               # Mac, no Qt
```

- [ ] **Step 2: Dispatch the reviewer** — one subagent on the most capable model the orchestrator has (at the time of writing `opus`, Claude Opus 5.5), brief (exact, plus the outputs of Step 1):
  « Review the whole branch `linux` of /Users/willow/Sites/ressac against `main` (`git diff $(git merge-base main linux)..linux`, about <n> commits). Read first: `docs/specs/2026-10-23-linux-design.md`, `docs/plans/2026-10-23-linux.md` (Global Constraints, Review Focus), `docs/specs/linux/mac-app-inventory.md` §11, the ledger `.superpowers/sdd/2026-10-23-linux/progress.md` (deviations and deferred minors already accepted). Report, with file:line, every Critical (data loss, a write outside the four allowed columns, article JavaScript running, a network access in tests, a Mac path or behaviour changed, the Mac build needing Qt, a crash or a frozen window) and Important (a spec or inventory behaviour missing or wrong, an exact string differing from the inventory, a race between the GUI thread and the workers, an unbounded memory or thread growth, dead or duplicated code, a test that cannot fail) finding; then Minor ones in one list. Check each master Review Focus item and each phase file's Review Focus against the code and the tests that claim to cover it. Do not modify files, do not run git write commands. »
- [ ] **Step 3: Fix rounds** — for each Critical and Important finding (or group sharing a cause): a fix of at most 5 files with a test that fails first, the full gate, one commit `Linux: final review fix <n> (<what>)` with the attribution lines; at most **5 rounds**. After the fixes, send the reviewer (same agent, `SendMessage`) the new commits for a re-check. Minor findings: fixed when trivial and inside an existing round's files, else one `TODO.md` line each under « ### Linux (sous-projet 8) » (French) or a ledger "accepted" line with the reason.
- [ ] **Step 4: Verdict** — the ledger row « B10b: review clean (<k> rounds, <m> minors deferred) ». A sixth round needed, or a Critical left: **⏸ STOP** and report to Clément instead of going on.

---

## B10c: README, notes, TODO, the Swift suite

### Task 3 (B10c): "Linux app" in the README, the bilan, the one Swift run

**Files:**
- Modify: `README.md`, `notes.md`, `TODO.md`

**Interfaces:**
- Consumes: the finished branch; `apps/linux/install.sh` (its options and messages), `apps/linux/dev/packages.txt` (the one package list), `apps/linux/dev/dev.sh`, the walks' names.
- Produces: the documentation the PR points to.

- [ ] **Step 1: The Swift suite, once** (master: "B10 runs it once"):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd /Users/willow/Sites/ressac/apps/macos && xcodegen generate && \
  xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' -derivedDataPath "$TMPDIR/ressac-dd" test 2>&1 | tail -40
```

  Expected: `** TEST SUCCEEDED **` (492 tests). When the only failure is `ChartBridgeTests`' real WebKit drawing (flaky locally, `TODO.md:128`), run it once more and say so; any other failure is a regression of the branch (the Mac sources did not change, so it comes from a shared resource or the core): fix it as a B10b round before going on.
- [ ] **Step 2: README** — `## Linux`'s first paragraph: the terminal side **and the KDE app** run on Linux (drop "on its way"). Add, right after `## Linux` and before `## The Mac app`, a `## The Linux app (KDE)` section, every command tried in the container or on the VM before it is written:
  - **Install:** `apps/linux/install.sh` from a clone (checks the Fedora packages of `apps/linux/dev/packages.txt`, prints the `sudo dnf install …` line when some are missing and stops; builds `ressac` and `ressac-kde` in release mode; installs `~/.local/bin/ressac`, `~/.local/bin/ressac-kde`, `~/.local/share/applications/org.w3cdotorg.ressac.desktop`, the icon); `--software-rendering` (for a VM whose GPU draws blank or flickering views: `QT_QUICK_BACKEND=software`, `QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu` in the desktop entry); `--uninstall` (exactly those files, never the database). Fedora 44, aarch64 tried; x86_64 should build.
  - **First launch:** from the Plasma launcher; with no feed, "No Feeds Yet — Drop an OPML file here, or choose one…"; `ressac export > ressac.opml` on another machine gives one. The app and `ressac` in Konsole share `~/.local/share/ressac/ressac.sqlite`.
  - **Files:** settings `~/.config/ressac/ressac-kde.conf` (the Mac app's keys), logs `~/.local/state/ressac/logs/` (Settings ▸ Data ▸ Show Logs).
  - **Keys:** the spec §7.2 table (Mac → KDE), verbatim; letters act when the reading window has focus and no text field does.
  - **Antiope:** Settings ▸ Reading ▸ Appearance; sizes 15/20/25/30 (Ctrl++ / Ctrl+−); digits for pages, Backspace and Esc, Alt+S / Alt+G (or `?`) / Alt+R, ←/→ between columns, `i` for mosaics; the CRT screen (when kept by B9).
  - **Sea Chart:** Ctrl+Shift+M, its own window, remembered size; Plasma's animation speed at Instant stops its animation.
  - **For developers:** `apps/linux/dev/dev.sh check`; `ressac-kde --screenshot <dir>` (walks every state and quits; `RESSAC_RECETTE_WALK=<name>` for one walk: `garden`, `journal`, `antiope`, `antiope-pages`, `mosaic`, `grid`, `smoke`…); `RESSAC_FRAME_LOG=1` (frame times every 10 s in the log); `apps/linux/dev/marghe.sh` (a Parallels VM: push, install, screenshots in the user's session).
- [ ] **Step 3: notes.md** (French, no em dash in prose), under « Ressac sous Linux (sous-projet 8) »: « ### Bilan de la partie B (<date>) »: what the app does and what it does not (spec §4 out of scope, the Mac gaps of inventory §11 kept or fixed: Theme picker in Antiope, private drag type), the deviations that matter for the next person (in-process core, `ressac:` scheme and app-world scripts, the key filter, painted Antiope rows, mosaics through `fetch_image`, CRT kept or cut), the test counts (`ressac-ui`, `ressac-kde`, the Swift run of Step 1), the review rounds.
- [ ] **Step 4: TODO.md** — the « 8 : Ressac sous Linux » line of « Sous-projets suivants »: partie B (appli KDE) faite le <date>, en recette sur marghe, PR à ouvrir; the deferred minors of B10b under « ### Linux (sous-projet 8) ».
- [ ] **Step 5: Verify** — `grep -n "on its way" README.md` prints nothing; every command quoted in the new section was run (list them in the ledger with their result); the full gate.
- [ ] **Step 6: Commit**

```bash
git add README.md notes.md TODO.md
git commit -m "README: the Linux app (install, first launch, keys, Antiope, Sea Chart, tooling); bilan of part B

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B10d: the recette on marghe — stops for Clément

### Task 4 (B10d): install on the VM, Clément's recette, the verdict

**Files:**
- Modify: `notes.md` (« ### Recette finale (B10, <date>) »)
- Modify (only if something is deferred): `TODO.md`
- (fix rounds, if the recette finds something Clément wants fixed now: ≤ 5 files each, gates, commits)

**Interfaces:**
- Consumes: `apps/linux/dev/marghe.sh` (`check`, `push`, `install`, `dnf`, `shots`, `manual`), the ledger's recette checklist (B8e anomalies, B9's CRT items).
- Produces: Clément's verdict.

- [ ] **Step 1: Ready** — full gate green on `HEAD`, `git status --porcelain` empty; `apps/linux/dev/marghe.sh check` (`Fedora release 44`, `aarch64`, `plasma session: yes|no`).
- [ ] **Step 2: Install** — `apps/linux/dev/marghe.sh push` → `pushed <sha> (Cargo.lock matches)`; `apps/linux/dev/marghe.sh install` (with `--software-rendering` if checkpoint 2 imposed it) → `install.sh: installed ressac and ressac-kde in …/.local/bin`. A missing package → B3h Step 6's question (Clément's go-ahead before `RESSAC_VM_DNF_OK=1 apps/linux/dev/marghe.sh dnf <packages>`). Under the hood: `ssh willow@marghe.local "prlctl exec 'Fedora Linux' runuser -u willow -- env HOME=<home> … sh -c 'cd <home>/src/ressac && CARGO_TARGET_DIR=<home>/.cache/ressac-build apps/linux/install.sh'"`.
- [ ] **Step 3: Last screenshots in his session** (only with `plasma session: yes`): `apps/linux/dev/marghe.sh shots final-gpu --look antiope` (the complete default walk, fixture copy) → `exit=0`; look at `91-antiope-101.png`, `96-chart.png`, `97-settings.png`.
- [ ] **Step 4: ⏸ STOP — Clément's recette** (exact message, then the checklist, numbered):
  « Recette finale de Ressac pour KDE sur marghe : l'appli est installée dans ta session (commit `<sha>`, `~/.local/bin/ressac-kde`, entrée « Ressac » dans le lanceur). Elle utilise sa propre base, `~/.local/share/ressac/ressac.sqlite`, vide pour l'instant ; je n'y touche pas. Pour l'OPML, exporte tes abonnements sur le Mac principal avec `ressac export > ~/Desktop/ressac.opml` et copie le fichier dans la VM comme tu veux. Voici la liste ; dis-moi pour chaque point si c'est bon, et ce qui ne va pas : »
  1. Lancer Ressac depuis le lanceur de Plasma ; la fenêtre propose d'importer un fichier OPML (« No Feeds Yet ») ; importer le fichier, vérifier l'aperçu, confirmer ; les flux se chargent.
  2. Lire avec `j`, `k`, Espace (défile puis passe au suivant), `m`, `s`, Ctrl+Entrée (navigateur), l'article complet ; Ctrl+F.
  3. Le Journal : à la première ouverture après 4 h du matin il se construit ; le lire jusqu'à « That's the edition. ».
  4. Le jardin : trier quelques flux (`k`, `r`, `b`, `u`), Ctrl+Z, l'inspecteur (rythme, thème).
  5. Ajouter un flux (Ctrl+N avec une adresse copiée dans Firefox) ; exporter l'OPML (Ctrl+Maj+E) ; glisser un flux dans un dossier.
  6. Réglages : police, taille, largeur ; Apparence Antiope ; taper `104`, `201`, `437`, Alt+S, Alt+G, Alt+R, `i` sur un article avec images, les colonnes au clavier ; [si gardé] l'écran CRT : allure, clic et molette à travers l'écran.
  7. La carte marine (Ctrl+Maj+M) sur un deuxième bureau virtuel ; cliquer une île.
  8. Dans Konsole, `ressac` sur la même base pendant que l'appli est ouverte (`ressac fetch` puis marquer des articles lus dans l'appli) ; le bandeau tmux (README, section Linux).
  9. <les points reportés du registre : anomalies du point d'étape 2, CRT à garder ou non, etc.>
  Wait for his answers. Nothing else happens on the VM meanwhile.
- [ ] **Step 5: Act on his answers** — each anomaly he wants fixed now: a fix round (test first where testable, ≤ 5 files, gates, commit `Linux: recette fix <n> (<what>)`, then `marghe.sh push && marghe.sh install` and ask him to look again); each he defers: a `TODO.md` line; a CRT he rejects: B9's cut procedure (revert, TODO line, notes).
- [ ] **Step 6: Verdict in `notes.md`** (French, no em dash in prose): « ### Recette finale (B10, <date>) »: commit, rendering mode, his answers point by point, what was fixed, what is deferred.
- [ ] **Step 7: Commit**

```bash
git add notes.md TODO.md
git commit -m "Linux: final recette on marghe (verdict, deferred points)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B10e: push and pull request — stops before, never merges

### Task 5 (B10e): ask, push, open the PR

**Files:** none.

- [ ] **Step 1: Final state** — full gate green; `git status --porcelain` empty; `git log --oneline main..linux | wc -l` (commit count); `git log main..linux --format='%B' | grep -c "Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"` equals the commit count (every commit carries the attribution, or the ledger explains the exceptions made by earlier sessions).
- [ ] **Step 2: ⏸ STOP — ask Clément** (exact message): « Tout est prêt sur la branche `linux` : <n> commits depuis `main`, portes vertes sur le Mac et dans le conteneur Fedora, recette validée le <date>. Puis-je pousser la branche sur `origin` et ouvrir la pull request vers `main` ? Je ne fusionnerai pas : la fusion reste à toi. (Rappel : les crédits Actions reviennent le 02/11/2026, la CI ne tournera peut-être pas avant.) » Wait for an explicit yes. A no or a question: stop here.
- [ ] **Step 3: Push** (after his yes only):

```bash
cd /Users/willow/Sites/ressac
git push -u origin linux
```

- [ ] **Step 4: Open the PR** — body written to a temporary file (`$TMPDIR/ressac-linux-pr.md`), no `@` mention of anyone:

```markdown
## Ressac on Linux (sub-project 8)

**Part A — the terminal side on Linux.** One source of truth for file locations (`ressac_core::paths`: XDG on Linux, macOS unchanged), the tmux ticker's state per database (fixes a Mac bug too), small CLI fixes, a Fedora 44 dev container and the Linux gate, CI and release jobs for the Linux CLI.

**Part B — a KDE app, `ressac-kde`.** Qt 6 / QML / Kirigami through cxx-qt 0.10, linking `ressac-core` in-process; every rule in the Qt-free crate `ressac-ui` (tested on the Mac). Native reading with the Mac's sanitiser rules and a `ressac:` scheme (article JavaScript never runs), the Journal, the garden, Add Feed and OPML, Settings, the Sea Chart (unchanged page over QWebChannel), Antiope (painted teletext rows, SAA5050 mosaics through the core's guarded `fetch_image`), the CRT screen <kept | left out, see TODO.md>.

- Spec: `docs/specs/2026-10-23-linux-design.md`; plan: `docs/plans/2026-10-23-linux.md` and `docs/plans/linux/`; findings and decisions: `notes.md` (« Ressac sous Linux »).
- Tests: `cargo test --workspace` on the Mac (no Qt needed), `apps/linux/dev/dev.sh check` in Fedora 44 (offscreen walks and smoke test), the Swift suite unchanged and green, two VM checkpoints and the final recette on marghe (Fedora KDE 44 aarch64, scale 2).
- Not done: see `TODO.md` (« Linux (sous-projet 8) »).
- CI: Actions credits are back on 2026-11-02; until then the gates were run locally.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb
```

```bash
gh pr create --base main --head linux --title "Ressac on Linux: XDG paths, Fedora tooling and the KDE app (sub-project 8)" --body-file "$TMPDIR/ressac-linux-pr.md"
```

- [ ] **Step 5: Report and stop** — give Clément the PR URL; ledger row « B10e: PR #<n> opened, not merged ». **Never run `gh pr merge` or `git merge`; never push to `main`.** The orchestration ends here.
