# Phase 13: macOS CI job, README, manual acceptance, `notes.md`

Read the plan index first. Global Constraints apply. Spec §9 (Manual check before declaring done; CI), §10 (CI image may lag).

**Files (3):**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `notes.md`

**Consumes:** the finished app of Phases 6–12; `RESSAC_DB`, which takes precedence over the Settings choice and the default path at launch (Phase 7 `AppDatabase.defaultURL`, Phase 11 `DatabaseLocation.current`); the bundled helper at `Ressac.app/Contents/Helpers/ressac` (Phase 6).

**Safety rule for this phase:** the acceptance run works on a **copy** in a temporary directory, through `--db` for the terminal and `RESSAC_DB` for the app (with `HOME` in the temporary directory, as in Phase 9's smoke runs). Nothing runs against `~/Library/Application Support/Ressac/`; the real file, if it exists, is only read once by `sqlite3 -readonly … .backup`. The user's OPML (`~/Documents/vienna_20260415.opml`) is only read, never copied into the repository.

### Task 13.1: macOS CI job

**Interfaces:**
- Consumes: the existing `check` job (Linux, Rust gate), `apps/macos/project.yml`.
- Produces: a `macos` job on `macos-latest`. A first step checks for Xcode 27 and the macOS 27 SDK and selects `/Applications/Xcode_27*.app` when the default Xcode is older. With them, the job installs Rust and XcodeGen, generates the project and runs `xcodebuild test`, and any failure fails the job. Without them, it skips the build with a warning and ends on a step marked `continue-on-error`, so the gap shows as an allowed failure (spec §9) and the workflow stays green.

- [ ] **Step 1: Add the job.** Append to `.github/workflows/ci.yml`, under `jobs:` (same indentation as `check:`):

```yaml
  macos:
    name: macOS app
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check for Xcode 27 and the macOS 27 SDK
        id: xcode
        shell: bash
        run: |
          set -euo pipefail
          required=27
          major() { xcodebuild -version | awk 'NR == 1 { split($2, v, "."); print v[1] }'; }
          if [ "$(major)" -lt "$required" ]; then
            candidate=$(ls -d /Applications/Xcode_"$required"*.app 2>/dev/null | sort -V | tail -n 1 || true)
            if [ -n "$candidate" ]; then
              sudo xcode-select -s "$candidate/Contents/Developer"
            fi
          fi
          xcode=$(xcodebuild -version | head -n 1)
          sdk=$(xcrun --sdk macosx --show-sdk-version)
          if [ "$(major)" -ge "$required" ] && [ "${sdk%%.*}" -ge "$required" ]; then
            echo "available=true" >> "$GITHUB_OUTPUT"
            echo "Using $xcode with the macOS $sdk SDK."
          else
            echo "available=false" >> "$GITHUB_OUTPUT"
            echo "::warning title=Xcode $required missing::This runner has $xcode with the macOS $sdk SDK; the app needs Xcode $required and the macOS $required SDK. Build and tests are skipped (spec §9, notes.md)."
          fi
      - uses: dtolnay/rust-toolchain@stable
        if: steps.xcode.outputs.available == 'true'
      - uses: Swatinem/rust-cache@v2
        if: steps.xcode.outputs.available == 'true'
      - name: Install XcodeGen
        if: steps.xcode.outputs.available == 'true'
        run: brew install xcodegen
      - name: Generate the Xcode project
        if: steps.xcode.outputs.available == 'true'
        working-directory: apps/macos
        run: xcodegen generate
      - name: Build and test the app
        if: steps.xcode.outputs.available == 'true'
        working-directory: apps/macos
        run: xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
      - name: Allowed failure, no Xcode 27 on this runner
        if: steps.xcode.outputs.available != 'true'
        continue-on-error: true
        run: |
          echo "::error title=Xcode 27 missing::The macOS app was not built or tested on this runner (allowed to fail, see notes.md)."
          exit 1
```

The Rust toolchain is needed on this job too: the app's build phase compiles the bundled `ressac` with `cargo build --release -p ressac` (Phase 6).

- [ ] **Step 2: Check the file and the script locally.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
ruby -ryaml -e 'w = YAML.load_file(".github/workflows/ci.yml"); puts w["jobs"].keys.inspect; puts w["jobs"]["macos"]["steps"].size'
```

Expected: `["check", "macos"]` and `9`. Then run the check step's script with a temporary `GITHUB_OUTPUT` (extract it from the workflow so the tested text is the committed one):

```bash
OUT=$(mktemp); SCRIPT=$(mktemp)
ruby -ryaml -e 'puts YAML.load_file(".github/workflows/ci.yml")["jobs"]["macos"]["steps"][1]["run"]' > "$SCRIPT"
GITHUB_OUTPUT="$OUT" bash "$SCRIPT"; cat "$OUT"
sed 's/required=27/required=28/' "$SCRIPT" > "$SCRIPT.28"; GITHUB_OUTPUT="$OUT" bash "$SCRIPT.28"; tail -n 1 "$OUT"
rm -f "$OUT" "$SCRIPT" "$SCRIPT.28"
```

Expected on this Mac: `Using Xcode 27.0 with the macOS 27.0 SDK.` and `available=true`; with the bar raised to 28, the `::warning title=Xcode 28 missing::…` line and `available=false` (the `ls` of `/Applications/Xcode_28*.app` finds nothing and `|| true` keeps `set -e` quiet). This was run on this Mac while writing the plan, with the same results.

- [ ] **Step 3: Commit and push, then read the run.**

```bash
git add .github/workflows/ci.yml
git commit -m "CI: macOS job building and testing the app when Xcode 27 is available"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
gh run list --branch mac-app --workflow CI --limit 1
gh run watch "$(gh run list --branch mac-app --workflow CI --limit 1 --json databaseId --jq '.[0].databaseId')" --exit-status || true
gh run view "$(gh run list --branch mac-app --workflow CI --limit 1 --json databaseId --jq '.[0].databaseId')" --log | grep -E "Using Xcode|Xcode 27 missing|TEST (SUCCEEDED|FAILED)" | head -n 5
```

Note which case happened (Xcode version and SDK on `macos-latest`, tests passed or job skipped) for Task 13.3's `notes.md` entry. If Xcode 27 was found and the tests failed on CI only, read the log: a missing `cargo` in the build phase's `PATH` or a code-signing complaint are the likely causes; report before changing Phase 6's files.

### Task 13.2: README section on the app

**Interfaces:**
- Consumes: the app's behaviour (spec §7), `RESSAC_DB` and Settings ▸ Data (Phase 11).
- Produces: a `## The Mac app` section in `README.md`, in English like the existing Usage section.

- [ ] **Step 1: Edit `README.md`.** Append after the Usage section (after the line ending `logs in `~/Library/Logs/Ressac/`.`):

````markdown
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
````

- [ ] **Step 2: Check the rendering.** `gh markdown-preview` is not installed; read the diff instead: `git diff README.md` shows only the new section, the fenced block nested in the section is closed, and no line of the existing Usage section changed.

- [ ] **Step 3: Commit.**

```bash
git add README.md
git commit -m "README: the Mac app"
```

### Task 13.3: Manual acceptance on a copy of the real database

**Interfaces:**
- Consumes: a Release build of the app; the real database if it exists, else the user's OPML.
- Produces: the spec §9 checklist run by the user on a copy, and its results in `notes.md` (French, no em dash in French prose).

This task needs the user at the keyboard: steps 1–3 prepare everything, step 4 is the one genuine stop of the phase, and step 5 records what the user saw.

- [ ] **Step 1: Build a Release app in a temporary directory.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
ACCEPT=$(mktemp -d -t ressac-acceptance)
echo "$ACCEPT"
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$ACCEPT/dd" build)
APP="$ACCEPT/dd/Build/Products/Release/Ressac.app"
"$APP/Contents/Helpers/ressac" --version
```

Expected: `BUILD SUCCEEDED` and the helper's version line.

- [ ] **Step 2: Make the copy.** The real database is only read, through SQLite's online backup, which gives a consistent snapshot even while WAL files exist. When it does not exist yet (the case on 10/10, see `notes.md`), build one the way the user would, in the temporary directory, with `HOME` redirected so even the helper's logs stay there:

```bash
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
BEFORE=$( [ -f "$REAL" ] && stat -f '%m %z' "$REAL" || echo absent )
if [ -f "$REAL" ]; then
  sqlite3 -readonly "$REAL" ".backup '$ACCEPT/ressac.sqlite'"
  echo "copied the real database"
else
  mkdir -p "$ACCEPT/home"
  HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/ressac.sqlite" import "$HOME/Documents/vienna_20260415.opml"
  HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/ressac.sqlite" check
  echo "built a database from the Vienna OPML"
fi
sqlite3 -readonly "$ACCEPT/ressac.sqlite" "SELECT version FROM schema_meta; SELECT count(*) FROM feed WHERE state = 'active'; SELECT count(*) FROM entry WHERE read_at IS NULL;"
```

Note in passing which branch ran, the schema version (`1` means the app will migrate it on launch: Review Focus #4 on real data) and the two counts. Expected for the OPML branch, from the first real run in `notes.md`: 186 active feeds, schema `2`. Inside the `else` branch `$HOME` in the OPML path is still the real home: the shell expands it before running the command with the redirected `HOME`.

- [ ] **Step 3: Launch the app on the copy and check the real file is untouched.**

```bash
mkdir -p "$ACCEPT/home"
RESSAC_DB="$ACCEPT/ressac.sqlite" HOME="$ACCEPT/home" "$APP/Contents/MacOS/Ressac" >"$ACCEPT/app.log" 2>&1 &
AFTER=$( [ -f "$REAL" ] && stat -f '%m %z' "$REAL" || echo absent ); [ "$BEFORE" = "$AFTER" ] && echo "real database untouched"
```

Expected: the window opens on the copy (its feed count matches step 2; for a schema `1` copy, a migration sheet appears first), and `real database untouched`. Run the second line again at the end of step 4.

- [ ] **Step 4: Hand the checklist to the user and wait.** Send it as written, with the two paths filled in, and wait for the answers. Each line is one spec §9 item (the extra lines cover what the earlier phases could not check on real data):

1. **Launch on the real database** (the copy at `$ACCEPT/ressac.sqlite`): the sidebar shows your folders with unread counts, health dots on sick feeds, the Garden counts; nothing is blank or slow to appear.
2. **Read with the keyboard only:** `j`/`k` and arrows, `Space` scrolls then jumps to the next unread, an entry selected for one second is marked read, `m` toggles it back, `s` stars, `⇧⌘A` marks the list read, `⌘F` searches (accents ignored: try a word with and without its accent), `Esc` closes the search, `⌘↩` opens the browser, `⌘R` refreshes the selected feed.
3. **Extract a truncated article:** Full Article on an entry whose feed only gives a summary: spinner, then the full text with the "full article" badge. Then on a site behind a login or an anti-bot wall: the feed text stays and "Full article unavailable: …" appears. Turn on "Always fetch full article" on a feed and refresh it (`⌘R`): the progress bar shows "Fetching full articles".
4. **Triage three feeds and undo one** in Garden ▸ Needs triage: decided rows dim, the count goes down, the selection moves on; `u` or `⌘Z` undoes the last decision; Apply Safe Repairs asks first.
5. **The terminal sees the same decisions:** in Terminal, `"$APP/Contents/Helpers/ressac" --db "$ACCEPT/ressac.sqlite" garden` (replace `$APP` and `$ACCEPT` by the paths above): the two remaining decisions appear, the undone one does not.
6. **Light and dark, fonts and sizes:** switch the system appearance with an article open (separate palettes, not inverted; images slightly dimmed in dark), try the five fonts in Settings, `⌘+` / `⌘−` / `⌘0`, the three line widths.
7. **Export:** File ▸ Export OPML… (`⇧⌘E`) to the Desktop, then `grep -c xmlUrl` on it equals the active feed count of step 2 minus the feeds buried in step 4.
8. **First run:** quit, then `RESSAC_DB="$ACCEPT/empty.sqlite" HOME="$ACCEPT/home" "$APP/Contents/MacOS/Ressac" &`, drop `tests/fixtures/vienna-like.opml` on the window, check the sheet ("11 feeds in 7 new folders."), press Cancel.

While the user tests, keep `sqlite3 -readonly "$ACCEPT/ressac.sqlite" "SELECT f.title, d.decision, d.decided_at FROM garden_decision d JOIN feed f ON f.id = d.feed_id ORDER BY d.id"` at hand to confirm item 5 independently.

- [ ] **Step 5: Record the results in `notes.md`.** Insert this section before `## Paysage existant (10/10/2026)`, with the observations filled in from the user's answers and steps 1–3 (the angle brackets are the parts to replace; keep the French, without em dash):

```markdown
## Appli Mac (sous-projet 2a)

### Recette manuelle (<JJ/MM/AAAA>)

Base : <copie de la vraie base par `sqlite3 -readonly … .backup` | base construite depuis l'OPML Vienna (import puis check)> dans un dossier temporaire, ouverte par `RESSAC_DB` (appli) et `--db` (terminal). La vraie base n'a pas bougé (date et taille identiques avant et après). Appli : build Release, `ressac` embarqué.

- Point de départ : schéma <1 ou 2>, <N> flux actifs, <N> entrées non lues.

| # | Vérification (spec §9) | Résultat | Observations |
|---|---|---|---|
| 1 | Lancement sur la vraie base | <OK ou KO> | <migration, temps d'ouverture, compteurs> |
| 2 | Lecture au clavier seul | <OK ou KO> | <touches qui posent problème> |
| 3 | Article complet d'un flux tronqué | <OK ou KO> | <site testé, échec affiché, « Always fetch full article »> |
| 4 | Tri de trois flux, une annulation | <OK ou KO> | <flux et décisions> |
| 5 | Mêmes décisions dans `ressac garden` | <OK ou KO> | <ce que montre le terminal> |
| 6 | Clair et sombre, polices, tailles | <OK ou KO> | <remarques de lisibilité> |
| 7 | Export OPML | <OK ou KO> | <nombre de `xmlUrl`> |
| 8 | Premier lancement (dépôt d'OPML) | <OK ou KO> | <texte de la feuille> |

<Pour chaque KO : ce qui s'est passé, et la suite décidée avec Clément.>

### CI macOS

- `macos-latest` : <Xcode et SDK trouvés par l'étape de vérification> ; le job <construit et teste l'appli | saute la construction, échec autorisé> (exécution <identifiant gh run>).
```

A `KO` is a genuine decision: describe it to the user with what was observed, and fix it only with their go-ahead, in the phase file that owns the code (outside this phase's three files).

- [ ] **Step 6: Commit.**

```bash
git add notes.md
git commit -m "notes.md : recette manuelle de l'appli Mac et état de la CI macOS"
```

Then quit the app and remove the temporary directory: `rm -rf "$ACCEPT"`.

### Task 13.4: Final verification and push

- [ ] **Step 1: Verification gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test
```

Expected: all green, zero warnings in our sources; `git status` clean apart from the ignored generated `apps/macos/Ressac.xcodeproj`.

- [ ] **Step 2: Check nothing private is tracked.**

```bash
git ls-files | grep -iE '\.opml$|\.sqlite$|xcodeproj' 
```

Expected: only `tests/fixtures/vienna-like.opml` and `tests/fixtures/contract.sqlite`.

- [ ] **Step 3: Push.**

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

## Notes for the executor

- **The check script was run on this Mac** while writing the plan (Xcode 27.0, macOS 27.0 SDK): `available=true`; with `required=28`: the warning and `available=false`. Apple's `sort` (2.3-Apple) supports `-V`. `ruby -ryaml` is available at `/opt/homebrew/bin/ruby` (PyYAML is not installed).
- **Step-level `continue-on-error`** makes only that step's failure tolerated: when Xcode 27 is present, a failing `xcodebuild test` still fails the job, which is the point of checking first rather than marking the whole job `continue-on-error`.
- **`RESSAC_DB` beats the Settings choice** (Phase 11 `DatabaseLocation.current`), and it is not persisted, so the user's Settings stay untouched by the acceptance run; Settings ▸ Data says "RESSAC_DB is set, so this launch uses it instead." during the run.
- **`sqlite3 -readonly SRC ".backup DEST"`** reads the source through SQLite's backup API, which copies a consistent snapshot including committed WAL content, and never writes to the source.
- **No real database on 10/10** (`notes.md`: « la vraie base de Clément n'existe pas encore »), so step 2 most likely takes the OPML branch: one `import` plus one `check` takes about 1 min 12 s (first real run).
