# Phase 7: Manual acceptance on the real database, docs, pull request

**Runs after:** phase 6c.

Read the plan index first (`docs/plans/2026-10-16-cadence-edition.md`). Global Constraints apply. Spec §10 (manual check before declaring done: seven items, the v4 migration tried on a copy first), §9 (offline, refusals), §5 (v4 migration: backup `ressac.sqlite.bak-v3`, one transaction), §1 (success criterion).

**Goal of the phase:** the v4 migration is tried on a copy of the real database, then Clément installs the Release build and lets the app migrate the real one and build its first edition; he runs the spec's seven-item checklist; the results go into `notes.md`; the README documents the Journal, the rhythm groups and the new commands; `TODO.md` closes sub-project 3; the branch goes up as a pull request that **Clément merges himself**.

**Files (3):**
- Modify: `notes.md` (acceptance table and findings, in French, no em dash)
- Modify: `README.md` (Journal and Rhythm, `ressac edition`, `ressac feed cadence`, schema v4, pages 104 to 109)
- Modify: `TODO.md` (sub-project 3 done, 2b next on schema v4)

Scratch files (`recette.sh`, the journal, snapshots, the PR body) live in a temporary directory and are never committed.

**Consumes:** the finished branch `cadence-edition` (phases 1a to 6c, each approved by Clément); `ressac migrate`, `ressac edition build|show`, `ressac feed cadence` (phases 1b, 3a, 3b); the contract views `v_cadence`, `v_edition_today`, `v_edition_overflow` (phase 1b).

**Safety rules for this phase:**
- **Two explicit stops.** Before Task 7.2 (installing the build, which migrates the real database at its next launch) and before Task 7.7 Step 2 (push and pull request), Claude stops and waits for Clément's go. A go for one is not a go for the other.
- **The real database** (`~/Library/Application Support/Ressac/ressac.sqlite`) is written only by the app Clément uses. Claude reads it only through SQLite's online backup (`sqlite3 -readonly … ".backup …"`) into the temporary directory; every query, every `ressac` command and every write (item 6's offline copy) runs on those snapshots. No test touches it.
- **No simulated keystrokes or clicks** towards any app (rule since 11/10, `notes.md`): Clément performs every step of the checklist, launches included; Claude prepares, reads snapshots and keeps the journal.
- **Rollback stays possible** until Clément says otherwise: the previous app is moved aside, not deleted, and the migration's `ressac.sqlite.bak-v3` is kept.

---

### Task 7.1: Release build, helper script and a pre-flight on a snapshot

**Files:** none in the repository.

- [ ] **Step 1: Find the installed app.** The README installs it in `apps/macos/build/` and optionally in `/Applications/`; the acceptance replaces the one Clément launches.

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
for app in /Applications/Ressac.app "$PWD/apps/macos/build/Ressac.app"; do
  [ -d "$app" ] && print -r -- "$app  $("$app/Contents/Helpers/ressac" --version 2>/dev/null)"
done
pgrep -lf 'Ressac.app/Contents/MacOS/Ressac' || echo "not running"
```

If both exist, ask Clément which one he opens (the running one's path answers it when it runs). Write it down as `TARGET`.

- [ ] **Step 2: Build a Release app in a temporary directory.**

```bash
ACCEPT=$(mktemp -d -t ressac-edition); mkdir -p "$ACCEPT/home"; echo "$ACCEPT"
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$ACCEPT/dd" build)
APP="$ACCEPT/dd/Build/Products/Release/Ressac.app"
"$APP/Contents/Helpers/ressac" --version
HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" edition --help | head -n 5
```

Expected: `BUILD SUCCEEDED`, the helper's version line, the `edition` subcommand's help. Write `$ACCEPT`, `$APP` and `$TARGET` down: every later step uses them.

- [ ] **Step 3: Write the helper script.** Same pattern as sub-project 4's `recette.sh` (`notes.md`, 15/10): it only ever reads the real database through a snapshot.

```bash
cat > "$ACCEPT/recette.sh" <<'EOF'
#!/bin/zsh
# Cadence and edition acceptance helper (phase 7, never committed). Reads the real database only
# through a read-only online backup into this directory; never writes to it.
set -euo pipefail
ACCEPT=${0:A:h}
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
HELPER=${HELPER:?set HELPER to the installed app's Contents/Helpers/ressac}
LATEST="(SELECT max(id) FROM edition)"

snap() {
  sqlite3 -readonly "$REAL" ".backup '$ACCEPT/snap.sqlite'"
  print -r -- "$ACCEPT/snap.sqlite"
}

case ${1:-help} in
  snap)
    snap ;;
  summary)
    s=${2:-$(snap)}
    sqlite3 -readonly "$s" "
      SELECT 'schema', version FROM schema_meta;
      SELECT 'feeds ' || state, count(*) FROM feed GROUP BY state;
      SELECT 'unread', count(*) FROM entry WHERE read_at IS NULL;
      SELECT 'group', effective_group, count(*), sum(unread) FROM v_cadence GROUP BY effective_group ORDER BY 2;
      SELECT 'overrides', count(*) FROM feed WHERE cadence_override IS NOT NULL;
      SELECT 'editions', count(*), datetime(max(built_at), 'localtime') FROM edition;" ;;
  checks)
    s=${2:-$(snap)}
    sqlite3 -readonly "$s" "
      SELECT 'no reserved page used', NOT EXISTS (SELECT 1 FROM v_page
        WHERE page IN (100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 800, 801, 802, 899));
      SELECT 'every active feed classified', NOT EXISTS (SELECT 1 FROM feed f JOIN feed_health h ON h.feed_id = f.id
        WHERE f.state = 'active' AND h.cadence_group NOT IN ('daily', 'weekly', 'monthly', 'rare', 'silent'));
      SELECT 'ranks run 1 to N',
        (SELECT count(*) FROM edition_entry WHERE edition_id = $LATEST)
          = coalesce((SELECT max(rank) FROM edition_entry WHERE edition_id = $LATEST), 0)
        AND NOT EXISTS (SELECT rank FROM edition_entry WHERE edition_id = $LATEST GROUP BY rank HAVING count(*) > 1);
      SELECT 'windows chain', NOT EXISTS (SELECT 1 FROM edition a JOIN edition b
        ON b.id = (SELECT min(id) FROM edition WHERE id > a.id) WHERE b.window_start <> a.window_end);
      SELECT 'default caps held', NOT EXISTS (SELECT 1 FROM edition_entry ee JOIN entry e ON e.id = ee.entry_id
        WHERE ee.edition_id = $LATEST GROUP BY ee.section, e.feed_id
        HAVING (ee.section = 'week' AND count(*) > 3) OR (ee.section = 'wire' AND count(*) > 1));
      SELECT 'sections match groups', NOT EXISTS (SELECT 1 FROM edition_entry ee JOIN entry e ON e.id = ee.entry_id
        JOIN v_cadence c ON c.feed_id = e.feed_id WHERE ee.edition_id = $LATEST AND NOT (
          (ee.section = 'front' AND c.effective_group IN ('monthly', 'rare'))
          OR (ee.section = 'week' AND c.effective_group = 'weekly')
          OR (ee.section = 'wire' AND c.effective_group = 'daily')));" ;;
  editions)
    sqlite3 -readonly "$(snap)" "
      SELECT id, datetime(built_at, 'localtime'), datetime(window_start, 'localtime'), datetime(window_end, 'localtime'),
             unreachable, (SELECT count(*) FROM edition_entry WHERE edition_id = edition.id),
             (SELECT coalesce(sum(hidden_count), 0) FROM edition_overflow WHERE edition_id = edition.id)
      FROM edition ORDER BY id DESC LIMIT 5;" ;;
  edition)
    s=${2:-$(snap)}
    HOME="$ACCEPT/home" "$HELPER" --db "$s" edition show ;;
  progress)
    sqlite3 -readonly "$(snap)" "
      SELECT section, count(*), sum(read_at IS NOT NULL) FROM v_edition_today GROUP BY section;
      SELECT 'overflow', feed_title, page, hidden_count FROM v_edition_overflow ORDER BY hidden_count DESC LIMIT 3;" ;;
  feed)
    q=${2//\'/}
    sqlite3 -readonly "$(snap)" "
      SELECT feed_id, page, title, effective_group, computed_group, is_override,
             round(median_interval_days, 2), round(entries_per_month, 1)
      FROM v_cadence WHERE title LIKE '%$q%' ORDER BY page;" ;;
  *)
    print "usage: HELPER=… recette.sh snap | summary [db] | checks [db] | editions | edition [db] | progress | feed TEXT" ;;
esac
EOF
chmod +x "$ACCEPT/recette.sh"
printf '# Journal de recette, rythme et journal du matin (%s)\n\n' "$(date +%d/%m/%Y)" > "$ACCEPT/journal.md"
```

The journal holds one timestamped line per observation (`printf '%s %s\n' "$(date +%H:%M)" "…" >> "$ACCEPT/journal.md"`); Task 7.4 turns it into the `notes.md` table.

- [ ] **Step 4: Pre-flight: migrate a snapshot with the new helper** (spec §10 item 1, first half).

```bash
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
sqlite3 -readonly "$REAL" ".backup '$ACCEPT/preflight.sqlite'"
sqlite3 -readonly "$ACCEPT/preflight.sqlite" "SELECT version FROM schema_meta"
time HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/preflight.sqlite" migrate
ls "$ACCEPT"/preflight.sqlite.bak-v*
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" summary "$ACCEPT/preflight.sqlite"
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" checks "$ACCEPT/preflight.sqlite"
HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/preflight.sqlite" page list | head -n 16
```

Expected: version `3` before; the migration in well under a second (v3 took 0,19 s); `preflight.sqlite.bak-v3`; `schema|4`; one `group|…` line per rhythm group with counts near the spec's measure of 16/10 (about 57 daily, 28 weekly, 17 monthly, 4 rare, 50 silent); `overrides|0`; `editions|0|`; every `checks` line ending in `|1`; `page list` starting with the fourteen reserved pages, 104 `Journal` to 109 `Silent` among them. Any `|0`, a migration error, or group counts far from the measure (a whole group empty): stop here and report to Clément with the output; the real database has not been touched.

- [ ] **Step 5: Preview an edition on a second copy** (network, writes the copy only). It shows the size of the first real edition before Clément sees it.

```bash
cp "$ACCEPT/preflight.sqlite" "$ACCEPT/preview.sqlite"
time HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/preview.sqlite" edition build; echo "exit $?"
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" edition "$ACCEPT/preview.sqlite" | tail -n 40
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" checks "$ACCEPT/preview.sqlite"
```

Expected: exit `0` and the line `Edition #1: N entries (front a, week b, wire c), M more left in their feeds.`; every check `|1`. Note N, the three section counts and M in the journal: the spec expects 40 to 45 entries on an ordinary day; the first edition can be a little larger, since the real database has not been refreshed for a while and everything new arrives in this one refresh. Exit `2` with `The network is unreachable: no edition built.` means no network here: note it and go on (item 2 shows the real build). The copy is not used again.

- [ ] **Step 6: Prepare the checklist's concrete values** on the preview copy:

```bash
P=$ACCEPT/preview.sqlite
sqlite3 -readonly "$P" "SELECT feed_title, page, hidden_count FROM v_edition_overflow ORDER BY hidden_count DESC LIMIT 1;"
sqlite3 -readonly "$P" "SELECT feed_id, page, title, effective_group FROM v_cadence
  WHERE title LIKE 'The Globe and Mail%' OR (effective_group <> 'daily' AND entries_per_month > 30) ORDER BY page LIMIT 3;"
```

Note in the journal: the feed with the largest overflow (item 3); the feed to force for item 4 (The Globe and Mail if it is not already Daily, else the first feed of the second query, else any Weekly feed), with its page and current group.

### Task 7.2: Install the build and let the app migrate the real database

**Files:** none in the repository.

- [ ] **Step 1: Stop and ask Clément for his go**, with the pre-flight's figures (schema 3 → 4 on a copy, the group counts, the preview edition's size). Nothing below runs without it.

- [ ] **Step 2: Ask Clément to quit Ressac**, then check:

```bash
pgrep -x Ressac && echo "still running" || echo "quit"
```

Expected: `quit`. Do not go on while it runs.

- [ ] **Step 3: Install, keeping the previous build for a rollback.**

```bash
mv "$TARGET" "$ACCEPT/Ressac-previous.app"
cp -R "$APP" "$TARGET"
touch "$TARGET"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$TARGET"
"$TARGET/Contents/Helpers/ressac" --version
export HELPER="$TARGET/Contents/Helpers/ressac"
```

Expected: the new helper's version line. The `touch` and `lsregister` refresh the Dock icon (`notes.md`, icon note).

- [ ] **Step 4: Clément opens Ressac** (spec §10 item 1, second half, and item 2). The launch screen says "Updating the database…" for a moment; then, as it is the first launch of the day, the Journal shows `Building this morning's edition… n/N` and fills when the build ends. Then:

```bash
ls -l "$HOME/Library/Application Support/Ressac/ressac.sqlite.bak-v3"
sqlite3 -readonly "$HOME/Library/Application Support/Ressac/ressac.sqlite.bak-v3" "SELECT version FROM schema_meta"
"$ACCEPT/recette.sh" summary
"$ACCEPT/recette.sh" checks
"$ACCEPT/recette.sh" editions
```

Expected: the backup exists with version `3`; the summary says `schema|4`, the same groups as the pre-flight (a feed may have moved after the refresh), `editions|1|<today, after 04:00>`; every check `|1`; one edition whose window starts 24 hours before its end. Journal line: the starting point (schema 3 → 4, groups and their unread, the first edition's size, sections, overflow and unreachable feeds, the build's duration as Clément saw it). If the app shows a migration failure instead, stop: the core rolled back and kept the backup (spec §5); report the launch screen's message to Clément and restore with `mv "$ACCEPT/Ressac-previous.app" "$TARGET"` only on his word.

### Task 7.3: The checklist, performed by Clément

**Files:** none in the repository (observations go to `$ACCEPT/journal.md`).

- [ ] **Step 1: Prepare item 6's copy.** Item 6 needs a day with no edition yet; rather than waiting for tomorrow, it runs on a copy of the real database from which today's edition is removed. Claude writes only to the copy.

```bash
sqlite3 -readonly "$HOME/Library/Application Support/Ressac/ressac.sqlite" ".backup '$ACCEPT/offline.sqlite'"
sqlite3 "$ACCEPT/offline.sqlite" "DELETE FROM edition WHERE id = (SELECT max(id) FROM edition); SELECT count(*) FROM edition;"
print -r -- "open -n --env RESSAC_DB=\"$ACCEPT/offline.sqlite\" \"$TARGET\""
```

Expected: `0` editions left in the copy (the real one keeps its edition); the printed `open` line goes into item 6 of the message below.

- [ ] **Step 2: Send the checklist and wait.** Send it in French, as written, with the values of Tasks 7.1 Step 6 and 7.3 Step 1 filled in. One answer per item: OK, or what went wrong.

> Recette du journal du matin et du rythme, sur ta vraie base (déjà passée en v4, sauvegarde `ressac.sqlite.bak-v3` à côté). Je ne touche ni au clavier ni à la souris : je lis seulement des copies de la base.
>
> 1. **Migration** : déjà vue au lancement (écran « Updating the database… », puis tes dossiers). Rien à refaire, dis-moi seulement si quelque chose t'a surpris.
> 2. **Première ouverture du jour** : le Journal affichait `Building this morning's edition… n/N`, les autres listes restaient utilisables, puis l'édition est apparue. Combien de temps environ ? L'en-tête dit-il `<jour> · N entries · 0 read` ?
> 3. **Lecture** : lis l'édition jusqu'au bout avec `j`, `k` et Espace (Espace fait défiler puis passe à la suivante, sans sortir de l'édition), jusqu'à la ligne `That's the edition.`. En route, clique une fois la ligne `+<n> more in <flux au plus gros débordement>` : le flux s'ouvre, et ses articles en trop sont restés non lus. `⇧⌘A` sur le Journal ne marque que l'édition.
> 4. **Forçage** : dans le jardin, sélectionne `<flux de l'item 4>` ; l'inspecteur affiche `Rhythm: <groupe> · median … · last entry …`. Choisis `Daily` dans le menu : la ligne finit par `(set by you)`, le flux passe dans Rhythm ▸ Daily de la barre latérale, et la colonne Rhythm de Garden ▸ All feeds le montre. Remets `Auto`.
> 5. **Deuxième lancement** : quitte Ressac et rouvre-le : pas de nouvelle fabrication, le Journal montre la même édition avec ta lecture.
> 6. **Hors ligne** (sur une copie, ta vraie base n'est pas touchée) : quitte Ressac, coupe le Wi-Fi, lance `! <commande open de l'étape 1>`. Le Journal doit afficher `Offline: no edition yet` avec un bouton `Retry`. Rallume le Wi-Fi, clique `Retry` : l'édition se fabrique. Quitte cette fenêtre, puis rouvre Ressac normalement.
> 7. **Antiope** : passe en Antiope, tape `104` : rubriques FRONT PAGE, THIS WEEK, THE WIRE en couleur, lignes de débordement blanc sur bleu. Lis avec SUITE et RETOUR : l'en-tête du lecteur affiche `104 1/N`, `104 2/N`… ; après le dernier, `FIN DE L'EDITION` en double hauteur. Tape ensuite `105` à `109` (les groupes de rythme), puis `100` : le SOMMAIRE liste 104 à 109 avec leurs compteurs.

- [ ] **Step 3: Observe while he tests.** All on snapshots, never on the real file:

```bash
"$ACCEPT/recette.sh" progress       # item 3: read counts per section moving, the overflow feeds
"$ACCEPT/recette.sh" feed "<item 4 feed>"   # item 4: is_override 1 with daily, then 0 with the computed group
"$ACCEPT/recette.sh" editions       # item 5: still one edition for today; item 6 leaves the real list unchanged
sqlite3 -readonly "$ACCEPT/offline.sqlite" "SELECT count(*), max(unreachable) FROM edition;"   # item 6: 1 after Retry
```

Add one journal line per item with what Clément reported and what the database showed. Items 4 and 5 are confirmed independently by `feed` and `editions`; item 6 by the offline copy holding one edition after Retry and the real database still holding its own.

- [ ] **Step 4: A KO is a decision, not a fix.** For any item that fails, describe to Clément what was observed (his words, the snapshot's figures) and fix it only with his go-ahead, in the phase file that owns the code, with that phase's tests and gate; then repeat the item. Nothing in this phase's three files changes behaviour.

### Task 7.4: Record the acceptance and the findings in `notes.md`

**Files:**
- Modify: `notes.md`

- [ ] **Step 1: Insert the section** before the line `## Paysage existant (10/10/2026)` (after "Corrections du TODO (todo-fixes, 15/10/2026)"), filled from the journal and Tasks 7.1 and 7.2. The angle brackets are the parts to replace; keep the French, without em dash (use a colon, a comma or parentheses instead).

```markdown
## Rythme et journal du matin (sous-projet 3)

### Recette manuelle (<JJ/MM/AAAA>)

Base : la vraie base, migrée de la v3 à la v4 par l'appli à son premier lancement, sauvegarde `ressac.sqlite.bak-v3` à côté. Appli : build Release <commit> à la place de l'ancien `<TARGET>` (gardé à part jusqu'à la fin de la recette). Recette faite par Clément ; Claude n'a envoyé aucune frappe ni aucun clic, il a seulement lu des copies de la base avec un petit `recette.sh` jetable et tenu le journal. L'essai hors ligne (item 6) a tourné sur une copie dont l'édition du jour avait été retirée. Migration essayée d'abord sur une copie (<durée>), puis une édition d'essai fabriquée sur une seconde copie (<N> entrées).

- Point de départ : schéma 3 puis 4, <N> flux actifs, groupes <N> Daily, <N> Weekly, <N> Monthly, <N> Rare, <N> Silent, <N> entrées non lues. Première édition : <N> entrées (une <a>, semaine <b>, fil <c>), <M> laissées dans leurs flux, <N> flux injoignables, fabriquée en <durée>.

| # | Vérification (spec §10) | Résultat | Observations |
|---|---|---|---|
| 1 | Migration v4, d'abord sur une copie | <OK ou KO> | <durée, sauvegarde, numéros 104 à 109 libres> |
| 2 | Première ouverture du jour : progression puis Journal | <OK ou KO> | <durée, en-tête, autres listes utilisables> |
| 3 | Lecture jusqu'à « That's the edition. », une ligne de débordement | <OK ou KO> | <ordre j/k/Espace, flux ouvert, `⇧⌘A`> |
| 4 | Forçage du rythme dans l'inspecteur, puis Auto | <OK ou KO> | <flux, groupes avant et après, barre latérale et colonne Rhythm> |
| 5 | Deuxième lancement le même jour | <OK ou KO> | <nombre d'éditions inchangé> |
| 6 | Hors ligne puis Retry (copie) | <OK ou KO> | <message, édition fabriquée après Retry> |
| 7 | Antiope : 104, sous-pages, FIN DE L'EDITION, 105 à 109, SOMMAIRE | <OK ou KO> | <couleurs, débordement blanc sur bleu, sous-pages> |

<Pour chaque KO : ce qui s'est passé, et la suite décidée avec Clément.>

### Trouvailles

- **Taille de l'édition** : <N> entrées le premier jour (estimation de la spec : 40 à 45). <Plafonds gardés à 3 et 1, ou essai `--rebuild --cap-week … --cap-wire …` et décision>.
- **Groupes de rythme sur la vraie base** : <répartition, flux mal classés repérés et forcés ou non>.
- **`entries_per_month` corrigé** : <exemple, The Globe and Mail passé de 8,3 à …>.
- **Durée de la première fabrication** : <durée, ressentie et mesurée>.
- **Écarts assumés par rapport à la spec** : lignes de débordement ouvertes au clic seulement en Antiope (pas de `↩`), titres de flux en capitales accentuées (`ACTUALITTÉ`).
- <Autre trouvaille de la recette, une ligne chacune.>
```

- [ ] **Step 2: Check the French.** `grep -n "—" notes.md` prints nothing new in the section (older lines only, if any). `git diff notes.md` shows one inserted section and no other change.

- [ ] **Step 3: Commit.**

```bash
git add notes.md
git commit -m "notes.md : recette du rythme et du journal du matin sur la vraie base, trouvailles" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7.5: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Check the names the README will quote.** `grep -rn '"Journal"\|"Rhythm"\|"Retry"' apps/macos/Ressac/Reading apps/macos/Ressac/App` confirms the sidebar labels and the Retry button; `HOME="$ACCEPT/home" "$HELPER" edition --help` and `feed cadence --help` confirm the flags. Use the real ones if they differ from below.

- [ ] **Step 2: Subcommands.** In the "More subcommands" block, replace the line

```
ressac migrate                         # bring the database to the current schema (v3)
```

with

```
ressac migrate                         # bring the database to the current schema (v4)
```

and after the line `ressac page set feed 12 412            # renumber a feed (or a folder) by hand, within 110-799` add:

```
ressac edition build                   # refresh, then compose this morning's edition (once a day, from 04:00)
ressac edition show                    # today's edition by section; --json for NDJSON
ressac feed cadence 12 daily           # force a feed's rhythm group (daily, weekly, monthly, rare, silent) or auto
```

- [ ] **Step 3: Exit codes.** Replace

```
Exit codes: `0` done, `1` error, `2` a refused request (an unknown id, nothing to repair, an address another active feed already uses), `130` stopped by Ctrl-C.
```

with

```
Exit codes: `0` done, `1` error, `2` a refused request (an unknown id, nothing to repair, an address another active feed already uses, an edition already built this morning, no network to build one), `130` stopped by Ctrl-C.
```

- [ ] **Step 4: Schema note.** Replace the paragraph that starts with `**Schema v3.**` by:

```markdown
**Schema v4.** The Mac app migrates the shared database to schema v4 on first launch, after a backup next to it (`ressac.sqlite.bak-v3`, or the backup of whichever older version it found). v4 adds each feed's rhythm group, the per-feed override and the morning editions; v3 already gave every folder and feed a three-digit page number (`ressac page list`). A `ressac` built before v4 refuses the database ("database created by a newer Ressac"). Get a v4 CLI with `cargo install --path crates/ressac`, or use the app's own copy at `Ressac.app/Contents/Helpers/ressac`.
```

- [ ] **Step 5: The Mac app.** In the **Reading** bullet, replace

```
three columns (sidebar with Unread, Today, Starred, your folders and the Garden; entry list; reader).
```

with

```
three columns (sidebar with the Journal, Unread, Today, Starred, your folders, the Rhythm groups and the Garden; entry list; reader).
```

then add, right after the whole **Reading** bullet:

```markdown
- **Journal:** the first time you open Ressac each day (a day starts at 04:00), it refreshes every feed and composes a morning edition of what appeared since the previous one: **Front Page** (feeds that publish monthly or less, every new entry), **This Week** (weekly feeds, three entries each at most) and **The Wire** (daily feeds, their latest entry). What the caps leave out stays unread in its feed, behind a `+12 more in …` line. The edition is frozen once built; read it to `That's the edition.`. Offline, the Journal says so and offers Retry. `⇧⌘A` marks only the edition read.
- **Rhythm:** every feed is Daily, Weekly, Monthly, Rare or Silent, from the median gap between its recent entries. The sidebar's Rhythm section lists each group like a folder; the garden's inspector shows a feed's rhythm and lets you force its group (`Auto` gives it back to the computation), and Garden ▸ All feeds has a Rhythm column.
```

- [ ] **Step 6: Antiope pages.** In the Antiope table, after the row `| 101, 102, 103 | Unread, Today, Starred |`, add:

```markdown
  | 104 | JOURNAL, this morning's edition; its entries are sub-pages ("104 3/43"), and SUITE after the last one shows FIN DE L'EDITION |
  | 105-109 | Rhythm groups: Daily, Weekly, Monthly, Rare, Silent |
```

- [ ] **Step 7: Check.** `git diff README.md`: the replaced lines and paragraph, the two new bullets, the two table rows, nothing else; each table row starts and ends with `|`; every string quoted matches the plan index's Exact strings.

- [ ] **Step 8: Commit.**

```bash
git add README.md
git commit -m "README: the Journal, rhythm groups, ressac edition and feed cadence, schema v4" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7.6: `TODO.md`

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Date line.** Replace `Prochaines étapes de Ressac, au 15/10/2026.` with `Prochaines étapes de Ressac, au <JJ/MM/AAAA>.` (today's date; re-read the line first, it may have moved on since).

- [ ] **Step 2: Sub-projects.** Replace the whole section from `## Sous-projets suivants` to the line before `## Reporté (relectures du sous-projet 2a)` with:

```markdown
## Sous-projets suivants

Ordre convenu le 14/10/2026 : 3, puis 2b.

- [ ] **2b : la lecture dans le terminal.** Trois colonnes en ratatui sur le même schéma (v4) : lu, favori, recherche, article complet, avec clappuccino comme référence. Habillage Antiope du terminal compris (palette et numéros de page du sous-projet 4), ainsi que le journal du matin et les groupes de rythme (vues `v_edition_today`, `v_edition_overflow` et `v_cadence`).
- [x] **3 : rythme et journal du matin.** Fait le <JJ/MM/AAAA> sur la branche `cadence-edition` ; recette et trouvailles dans `notes.md`.
- [x] **4 : thème Antiope (appli Mac).** Fait le 15/10/2026 sur la branche `antiope` ; recette et trouvailles dans `notes.md`.
```

- [ ] **Step 3: Deferred items from the acceptance.** If Task 7.3 or 7.4 left points that Clément chose to postpone (the `↩` on Antiope overflow rows is the known candidate), add after the last `### …` subsection of "Reporté":

```markdown
### Rythme et journal (sous-projet 3)

- [ ] <point reporté, une ligne chacun, sans tiret cadratin>
```

and nothing if there are none.

- [ ] **Step 4: Check and commit.** `grep -n "—" TODO.md` prints no new line; `git diff TODO.md` shows only the changes above.

```bash
git add TODO.md
git commit -m "TODO.md : sous-projet 3 terminé, 2b ensuite sur le schéma v4" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7.7: Final gate, push and pull request

- [ ] **Step 1: Gate.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | tee "$ACCEPT/rust.log" | grep -E "^test result" | awk '{passed += $4; failed += $6} END {print passed " passed, " failed " failed"}'
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -destination 'platform=macOS' test) 2>&1 | tee "$ACCEPT/swift.log" | grep -E "Test run with|TEST (SUCCEEDED|FAILED)"
grep -c "warning:" "$ACCEPT/swift.log"
git status --short
git ls-files | grep -iE '\.opml$|\.sqlite$|xcodeproj'
```

Expected: fmt and clippy silent; `N passed, 0 failed` for Rust; `✔ Test run with N tests passed` and `** TEST SUCCEEDED **` for Swift; `0` warnings; an empty `git status`; only `tests/fixtures/vienna-like.opml` and `tests/fixtures/contract.sqlite` tracked among those patterns. Write the two test counts down for the PR body.

- [ ] **Step 2: Stop and ask Clément for his go to push and open the pull request**, with the gate's figures and the acceptance's result. Nothing below runs without it.

- [ ] **Step 3: Push.**

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push -u origin cadence-edition
```

- [ ] **Step 4: Write the PR body**, with the counts of Step 1 and the results of Task 7.3 filled in:

```bash
cat > "$ACCEPT/pr.md" <<'EOF'
## Summary

Sub-project 3: **rhythm groups and the morning edition**. Spec: `docs/specs/2026-10-16-cadence-and-edition-design.md`; plan: `docs/plans/2026-10-16-cadence-edition.md` and `docs/plans/cadence-edition/`.

- **Core, schema v4:** every feed's rhythm group (Daily, Weekly, Monthly, Rare, Silent) from the median gap between its recent entries, stored with its health; `entries_per_month` measured over the span the stored entries cover; a per-feed override; tables `edition`, `edition_entry`, `edition_overflow` and the views `v_cadence`, `v_edition_today`, `v_edition_overflow`; reserved pages 104 to 109.
- **Edition:** composed once a day from 04:00, after a refresh: entries first seen since the previous edition, unread, at most 7 days older than the window; Front Page (monthly and rare feeds), This Week (weekly, 3 per feed), The Wire (daily, 1 per feed); the rest stays unread in its feed. Frozen once built; offline, nothing is written and the window does not move.
- **CLI:** `ressac edition build [--cap-week N] [--cap-wire N] [--rebuild]`, `ressac edition show [--json]`, `ressac feed cadence <id> auto|daily|…`.
- **App, Native:** the Journal at the top of the sidebar, built on the first launch of the day with progress, sections, overflow rows, `That's the edition.`, offline Retry; the Rhythm section; the inspector's rhythm line and override menu; a Rhythm column in Garden ▸ All feeds.
- **App, Antiope:** P104 JOURNAL with coloured sections, sub-pages `104 n/N` and `FIN DE L'EDITION`; P105 to P109; SOMMAIRE; `EDITION IN PROGRESS n/N` in the Minitel bar; the rhythm and its override rows in the garden inspector.

## Tests

- Rust: <N> tests, `cargo fmt`, `cargo clippy -D warnings`.
- Swift: <N> tests (Swift Testing), zero warnings.
- Manual acceptance on the real database (migrated to v4 by the app, backup `ressac.sqlite.bak-v3`; the offline check on a copy): <7/7 OK, or the list of KO and what was decided>. Details in `notes.md`, section « Rythme et journal du matin (sous-projet 3) ».
- First real edition: <N> entries (front <a>, week <b>, wire <c>), <M> left in their feeds.

## Notes

- A `ressac` built before this branch refuses a v4 database; the README says how to get a v4 CLI.
- Sub-project 2b (terminal reading) reads the same three views.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

- [ ] **Step 5: Open the pull request, do not merge it.**

```bash
gh pr create --base main --head cadence-edition \
  --title "Sub-project 3: rhythm groups and the morning edition" --body-file "$ACCEPT/pr.md"
gh pr view --json number,url,state --jq '"\(.number) \(.url) \(.state)"'
```

Expected: `<number> <url> OPEN`. Give Clément the link; he merges it himself once CI is green. Do not run `gh pr merge`.

- [ ] **Step 6: Clean up only on Clément's word.** `$ACCEPT` holds the previous app and the snapshots; keep it until he says the new build is fine, then `rm -rf "$ACCEPT"` (one explicit path, never a glob).

## Notes for the executor

- **Item 6 never touches the real database:** `RESSAC_DB` points the launched copy of the app at `$ACCEPT/offline.sqlite`, and Clément quits that window before reopening Ressac normally. If the real database ends up with a second edition today, item 6 went wrong: stop and report.
- **The first real edition covers more than a day** when the database was last refreshed days ago: its window is 24 hours long, but everything fetched by its refresh is "first seen" inside it, within the 7-day guard. Say so in the findings rather than tuning caps on it.
- **The previous app** stays in `$ACCEPT/Ressac-previous.app`; a rollback also needs `ressac.sqlite.bak-v3` back in place (an older app refuses a v4 database). Both only on Clément's word.
