# Phase 10: acceptance in the terminal, docs, pull request

**Runs after:** phase 9b.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`): Global Constraints (Language: `notes.md` in French, no em dash in French prose; Tests and the real database; Push). Spec §10 (manual check, seven items, in Ghostty under tmux), §1 (success criterion), §12 (risks: image protocol detection under tmux).

**Goal of the phase:** Clément runs the Release `ressac` in a tmux pane of Ghostty on his real database and goes through the spec's seven items; the results go into `notes.md`; the README documents the reader and its keys; `TODO.md` closes sub-project 2b; the branch goes up as a pull request, which is merged after CI is green **once Clément gives the go at that point**.

**Files (3):**
- Modify: `notes.md` (acceptance table and findings, in French, no em dash)
- Modify: `README.md` (the reader: `ressac`, `ressac read`, columns, keys, looks, images, the settings file)
- Modify: `TODO.md` (2b done; the deferred items found on the way)

Scratch files (`recette.sh`, the journal, snapshots, the PR body) live in a temporary directory under the session's scratchpad and are never committed.

**Safety rules for this phase:**
- **Two explicit stops:** before Task 10.2 (Clément starts reading his real database with the new binary, which writes `read_at`/`starred_at` and may build today's edition) and before Task 10.6 Step 2 (push, pull request, merge). A go for one is not a go for the other.
- **The real database** (`~/Library/Application Support/Ressac/ressac.sqlite`) is written only by the programs Clément uses (the reader, the app). Claude reads it only through SQLite's online backup (`sqlite3 -readonly … ".backup …"`) into the temporary directory; every query runs on those snapshots. When a copy must lose its edition, delete with `PRAGMA foreign_keys = ON` (otherwise orphan `edition_entry` rows block the next build, sub-project 3's acceptance).
- **No simulated keystrokes** towards any app or terminal: Clément types every key; Claude prepares, reads snapshots, keeps the journal.
- **Never ask Clément to cut the network** (it also cuts Claude's session). An offline check, if needed, uses a copy whose feed URLs point at distinct `.invalid` hosts (`http://feedN.invalid/rss`), as sub-project 3's acceptance did.
- **Nothing is installed** in `~/.cargo/bin` or elsewhere: Clément runs the binary from the build directory.

---

### Task 10.1: Release build, helper script, pre-flight on a snapshot

**Files:** none in the repository.

- [ ] **Step 1: Build in a temporary directory.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
ACCEPT=$(mktemp -d "$TMPDIR/ressac-reader.XXXX"); mkdir -p "$ACCEPT/home"; echo "$ACCEPT"
cargo build --release -p ressac --target-dir "$ACCEPT/target" 2>&1 | tail -2
BIN="$ACCEPT/target/release/ressac"
"$BIN" --version
"$BIN" read --help | head -5
```

Expected: `Finished release`, the version line, the `read` help. Write `$ACCEPT` and `$BIN` down; every later step uses them. (When the session gives a scratchpad directory, create `ACCEPT` there instead of `$TMPDIR`.)

- [ ] **Step 2: Check the environment Clément will use** (read-only):

```bash
echo "TERM_PROGRAM=$TERM_PROGRAM TERM=$TERM"; tmux -V; tmux show -g allow-passthrough
ls /Applications | grep -i -E 'ghostty|iterm|warp'
```

Expected on this Mac (16/10/2026): tmux 3.8, `allow-passthrough on`, Ghostty and iTerm2 installed. If passthrough is off, say so to Clément before item 3 (images then fall back to half-blocks; that is a valid outcome, not a failure).

- [ ] **Step 3: Write the helper script** (never committed):

```bash
cat > "$ACCEPT/recette.sh" <<'EOF'
#!/bin/zsh
# Terminal reader acceptance helper (phase 10, never committed). Reads the real database only
# through a read-only online backup into this directory; never writes to it.
set -euo pipefail
ACCEPT=${0:A:h}
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
snap() {
  rm -f "$ACCEPT/snap.sqlite" "$ACCEPT/snap.sqlite-wal" "$ACCEPT/snap.sqlite-shm"
  sqlite3 -readonly "$REAL" ".backup '$ACCEPT/snap.sqlite'"
  print -r -- "$ACCEPT/snap.sqlite"
}
case ${1:-help} in
  snap) snap ;;
  summary)
    s=$(snap)
    sqlite3 "$s" "
      SELECT 'schema', version FROM schema_meta;
      SELECT 'unread', count(*) FROM entry WHERE read_at IS NULL;
      SELECT 'starred', count(*) FROM entry WHERE starred_at IS NOT NULL;
      SELECT 'editions', count(*), datetime(max(built_at), 'localtime') FROM edition;
      SELECT 'journal read', sum(read_at IS NOT NULL), count(*) FROM v_edition_today;" ;;
  recent)
    # Entries whose read or star state changed in the last N minutes (default 10).
    s=$(snap); m=${2:-10}
    sqlite3 "$s" "
      SELECT e.id, substr(coalesce(e.title,'(untitled)'),1,60), e.read_at IS NOT NULL, e.starred_at IS NOT NULL
      FROM entry e
      WHERE e.read_at >= strftime('%Y-%m-%dT%H:%M:%fZ','now','-$m minutes')
         OR e.starred_at >= strftime('%Y-%m-%dT%H:%M:%fZ','now','-$m minutes')
      ORDER BY coalesce(e.read_at, e.starred_at) DESC LIMIT 20;" ;;
  decisions)
    s=$(snap)
    sqlite3 "$s" "SELECT id, feed_id, decision, datetime(decided_at,'localtime') FROM garden_decision ORDER BY id DESC LIMIT 5;" ;;
  *) print "usage: recette.sh snap | summary | recent [minutes] | decisions" ;;
esac
EOF
chmod +x "$ACCEPT/recette.sh"
printf '# Journal de recette, lecture dans le terminal (%s)\n\n' "$(date +%d/%m/%Y)" > "$ACCEPT/journal.md"
"$ACCEPT/recette.sh" summary
```

Expected: `schema|4`, the unread and starred counts, today's edition if the app built it. These are the starting point; one journal line records them (`printf '%s %s\n' "$(date +%H:%M)" "…" >> "$ACCEPT/journal.md"`). (`garden_decision` columns checked against `schema/0001_init.sql`: `decided_at`; an undo deletes the latest row.)

- [ ] **Step 4: Pre-flight on a copy** (Claude's own check that the binary opens the database and lists the Journal; it runs the non-interactive commands only):

```bash
cp "$("$ACCEPT/recette.sh" snap)" "$ACCEPT/preflight.sqlite"
HOME="$ACCEPT/home" "$BIN" --db "$ACCEPT/preflight.sqlite" edition show | head -20
HOME="$ACCEPT/home" "$BIN" --db "$ACCEPT/preflight.sqlite" page list | head -16
```

Expected: today's edition by section (or `No edition yet.`), the 14 reserved pages. The interactive reader is not started by Claude (no terminal to drive, no keystrokes).

### Task 10.2: Stop, then the checklist performed by Clément

**Files:** none in the repository (observations go to `$ACCEPT/journal.md`).

- [ ] **Step 1: Stop and ask Clément for his go**, with the pre-flight's figures, the binary path, and what running it on the real database will do (it marks entries read or starred as he reads; it builds today's edition on launch if the app has not; the app sees the same changes). Nothing below runs without it.

- [ ] **Step 2: Send the checklist and wait.** In French, as written, with `$BIN` filled in. One answer per item: OK, or what went wrong.

> Recette de la lecture dans le terminal, sur ta vraie base. Je ne touche ni au clavier ni à la souris : je lis seulement des copies de la base.
>
> Dans un volet tmux de Ghostty, lance : `<$BIN>`
>
> 1. **Ouverture** : le lecteur s'ouvre sur le Journal du jour (s'il n'existait pas encore, la première ligne montre `Building this morning's edition… n/N` puis l'édition apparaît, les autres sources restent lisibles pendant ce temps).
> 2. **Lecture** : lis avec `j`, `k` et Espace jusqu'à `That's the edition.` (Espace fait défiler puis passe au non-lu suivant sans sortir du Journal). Dans un article avec des liens, `o` puis `3` ouvre le troisième lien dans le navigateur ; `o` seul ouvre l'article.
> 3. **Images** : ouvre un article avec des photos. Elles s'affichent en vraies images dans le volet ; `i` les masque et les remet. Si elles apparaissent en blocs colorés, dis-le moi (c'est le repli prévu, pas une panne).
> 4. **Volet étroit** : réduis le volet à environ 70 colonnes : une seule colonne à la fois (`Entrée` pour descendre, `Échap` pour remonter). Élargis : les colonnes reviennent sur le même article.
> 5. **Antiope** : `t`, puis tape `104` : rubriques en couleur, lignes « +N MORE IN … » en blanc sur bleu, en-tête `104 n/N` dans le lecteur ; SUITE (`j`) jusqu'à `FIN DE L'EDITION`, RETOUR (`k`) revient au dernier article. Tape `105`, puis `800`, puis `100` (SOMMAIRE) et `899` (GUIDE). `t` pour revenir en Native.
> 6. **Avec l'appli** : ouvre Ressac.app à côté. Marque un article lu dans l'une, il doit apparaître lu dans l'autre en quelques secondes ; pareil dans l'autre sens.
> 7. **Jardin** : va sur Needs triage (dans les sources, ou `800` en Antiope), garde un flux avec `k`, puis annule avec `u`.
>
> `q` pour quitter.

- [ ] **Step 3: Observe while he tests** (snapshots only):

```bash
"$ACCEPT/recette.sh" summary      # item 1: one edition today; item 2: journal read count rising
"$ACCEPT/recette.sh" recent 5     # items 2 and 6: the entries just read or starred, from either program
"$ACCEPT/recette.sh" decisions    # item 7: a keep, then the undo leaves the previous decision in force
```

One journal line per item with what Clément reported and what the snapshot showed. Item 6 is confirmed when `recent` lists the entry he named with `read_at` set, whichever program he used.

- [ ] **Step 4: A KO is a decision, not a fix.** For any failing item, describe to Clément what was observed (his words, the snapshot's figures, a screenshot if he sends one) and fix it only with his go-ahead: write the failing test in the phase that owns the code, fix, run the gate, rebuild (`Task 10.1 Step 1`), and repeat the item. Known fragile spot (spec §12): the image protocol query under tmux (clappuccino's notes: picker after `ratatui::init()`, the unwrapped DA1, Warp forced to iTerm2). If the first keystroke is swallowed or the keyboard freezes after launch, the picker is the first suspect.

### Task 10.3: Record the acceptance in `notes.md`

**Files:** Modify `notes.md`.

- [ ] **Step 1: Add a section** after « Rythme et journal du matin (sous-projet 3) », in French, no em dash, in the same shape as that section:

```markdown
## Lecture dans le terminal (sous-projet 2b)

### Recette manuelle (JJ/MM/2026)

Base : la vraie base (schéma 4). Lecteur : `ressac` en Release, lancé par Clément dans un volet tmux de Ghostty (tmux 3.8, `allow-passthrough on`) ; Claude n'a envoyé aucune frappe, il a lu des copies de la base avec un `recette.sh` jetable et tenu le journal.

- Point de départ : <non-lus, favoris, édition du jour>.

| # | Vérification (spec §10) | Résultat | Observations |
|---|---|---|---|
| 1 | Ouverture sur le Journal, fabrication si besoin | | |
| 2 | Lecture au clavier, liens `o` + numéro | | |
| 3 | Images dans le volet, `i` | | <protocole détecté : Kitty, iTerm2, Sixel ou demi-blocs> |
| 4 | Volet étroit puis élargi | | |
| 5 | Antiope : 104, sous-pages, FIN DE L'EDITION, 105, 800, 100, 899 | | |
| 6 | Appli et terminal ouverts ensemble | | <délai observé> |
| 7 | Jardin : garder puis annuler | | |

### Trouvailles

- <une puce par trouvaille : ce qui a été vu, la cause, la correction et son commit, ou « reporté dans TODO.md »>
```

Fill every cell from `$ACCEPT/journal.md`; a KO fixed during the acceptance shows « KO puis OK » with the fix's commit. Keep the pitfalls worth remembering (image protocol under tmux, the reader and the app writing the same base).

- [ ] **Step 2: Commit**

```bash
git add notes.md
git commit -m "notes.md : recette de la lecture dans le terminal, trouvailles

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10.4: README

**Files:** Modify `README.md`.

- [ ] **Step 1: In `## Usage`**, make the first block start with the reader:

```sh
ressac                                  # read: the Journal first, then every source (ressac read is the same)
ressac import ~/Documents/vienna.opml   # --dry-run to preview
```

- [ ] **Step 2: Add a section `## The terminal reader`** before `## The Mac app`:

```markdown
## The terminal reader

`ressac` with no subcommand (or `ressac read`) opens a keyboard reader on the same database as the Mac app; both can stay open, each sees the other's changes within a couple of seconds. It opens on the Journal and builds this morning's edition when it is due, as the app does.

- **Columns:** three when the pane is 120 columns or wider (sources, entries, reader), two from 80 (the sources open over the list with `h`), one below 80 (`Enter` goes down, `Esc` comes back).
- **Keys:** `j` / `k` next and previous entry, Space scrolls then moves to the next unread entry of the source, `h` / `l` or the arrows or `Tab` change column, `m` read, `s` star, `A` mark all read, `u` all or unread only, `/` search, `f` full article, `o` open the article (`o` then `1`–`9` opens a numbered link), `i` images, `r` / `R` refresh the source or everything, `c` a feed's rhythm, `t` Native or Antiope, `?` help, `q` quit. On a garden page the garden's keys apply (`k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `R` safe repairs).
- **Antiope:** the teletext look, as in the app: type a page number (`104` the Journal, `105`–`109` the rhythm groups, `800`–`802` the garden, `100` SOMMAIRE, `899` GUIDE), `⌫` corrects, `Esc` cancels; SUITE `Space` or `j`, RETOUR `k`, SOMMAIRE `S`, GUIDE `?`, RÉPÉTITION `r`. Images become teletext mosaics; `i` shows the original.
- **Images:** shown inline when the terminal speaks a graphics protocol (Kitty, as Ghostty does, iTerm2 or Sixel; under tmux, `set -g allow-passthrough on`), as half-blocks otherwise, or as `[image: …]` when they cannot load or are turned off.
- **Settings:** the look and the images switch are kept in `~/Library/Application Support/Ressac/terminal.json`.
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "README: the terminal reader, its columns, keys, Antiope pages and images

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10.5: `TODO.md`

**Files:** Modify `TODO.md`.

- [ ] **Step 1:** In « Sous-projets suivants », tick 2b:

```markdown
- [x] **2b : la lecture dans le terminal.** Fait le JJ/MM/2026 sur la branche `terminal-reading` ; recette et trouvailles dans `notes.md`.
```

and remove the « Ordre convenu le 14/10/2026 : 3, puis 2b. » line, every planned sub-project being done.

- [ ] **Step 2:** Add a section « ### Lecture dans le terminal (sous-projet 2b) » under « Reporté », one line per item deferred during the phases' reviews and the acceptance (from the reports and the ledger), in French, no em dash. Always list, if still true: separated mosaics in Antiope; syncing the look with the app's Appearance; a separate crate for the reader if compile times grow.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "TODO.md : sous-projet 2b terminé, reports de la lecture dans le terminal

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10.6: Final gate, push, pull request, merge

- [ ] **Step 1: The full gate**, the Swift suite included once (no Swift change in this sub-project, but the contract fixture and the app must still pass):

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && INSTA_UPDATE=no cargo test --workspace
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test 2>&1 | grep -E '\*\* TEST|error:' )
git status --short
git ls-files | grep -E '\.opml$|\.sqlite$|xcodeproj' 
```

Expected: everything green, `** TEST SUCCEEDED **`, a clean tree, and only `tests/fixtures/contract.sqlite` and `tests/fixtures/vienna-like.opml` tracked among those patterns.

- [ ] **Step 2: Ask Clément for the go, then push, open the PR, and merge after CI.** Write the PR body to `$ACCEPT/pr.md` (summary of core / terminal / Antiope / images, tests with counts, the acceptance 7/7 with any KO fixed, notes on deferred items, ending with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`), read it once for wording that does not match the spec's approved decisions, then:

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push -u origin terminal-reading
gh pr create --base main --head terminal-reading --title "Sub-project 2b: reading in the terminal" --body-file "$ACCEPT/pr.md"
gh pr checks <number> --watch --interval 30
gh pr merge <number> --merge
git switch main && git -c credential.helper= -c credential.helper='!gh auth git-credential' pull --ff-only
```

Merge only when every check passes; a red check is reported to Clément with its log, never retried blindly. After the merge, `main` is checked out locally and up to date.
