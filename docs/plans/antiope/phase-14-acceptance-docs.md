# Phase 14: Manual acceptance on the real database, docs, pull request

**Runs after:** phase 13.

Read the plan index first (`docs/plans/2026-10-14-antiope.md`). Global Constraints apply. Spec §11 (manual check before declaring done: nine items, real database, recorded in `notes.md`, no simulated keystrokes), §10 (v3 migration: backup, rollback), §1 (success criterion).

**Goal of the phase:** Clément runs the spec's nine-item checklist in the Release app installed in `/Applications`, on his real database (migrated to v3 by the app itself); the results go into `notes.md` as a table, the README documents Antiope and `ressac page`, `TODO.md` closes sub-project 4, and the branch goes up as a pull request that Clément merges himself.

**Files (3):**
- Modify: `notes.md` (acceptance table and findings, in French, no em dash)
- Modify: `README.md` (Antiope section, `ressac page list/set`, schema v3 note)
- Modify: `TODO.md` (sub-project 4 done, order 3 then 2b)

Scratch files (`recette.sh`, the journal, snapshots, the PR body) live in a temporary directory and are never committed.

**Consumes:** the finished branch `antiope` (phases 1 to 13, each approved by Clément); phase 13's hand-off (the two hitch ratios, the bleed decision, Clément's answers on the CRT screen); `ressac page list` (phase 2); the launcher's migration (`ressac migrate`, backup `ressac.sqlite.bak-v2` written by the core next to the database, `crates/ressac-core/src/store/mod.rs`).

**Safety rules for this phase:**
- **The real database** (`~/Library/Application Support/Ressac/ressac.sqlite`) is written only by the app Clément uses, as always. Claude reads it only through SQLite's online backup (`sqlite3 -readonly … ".backup …"`) into the temporary directory, and every query or `ressac` command runs on those snapshots. No test (Rust or Swift) touches it: they use `tests/fixtures/contract.sqlite` copies in temporary directories.
- **No simulated keystrokes or clicks** towards any app (rule since 11/10, `notes.md`): Clément performs every step of the checklist; Claude prepares, observes the database, and keeps the journal.
- **Rollback stays possible** until Clément says otherwise: the previous `/Applications/Ressac.app` is moved aside, not deleted, and the migration's `ressac.sqlite.bak-v2` is kept.

---

### Task 14.1: Release build, helper script and a pre-flight on a snapshot

**Files:** none in the repository.

- [ ] **Step 1: Build a Release app in a temporary directory.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
ACCEPT=$(mktemp -d -t ressac-antiope); mkdir -p "$ACCEPT/home"; echo "$ACCEPT"
(cd apps/macos && xcodegen generate && xcodebuild -project Ressac.xcodeproj -scheme Ressac -configuration Release -derivedDataPath "$ACCEPT/dd" build)
APP="$ACCEPT/dd/Build/Products/Release/Ressac.app"
"$APP/Contents/Helpers/ressac" --version
ls "$APP/Contents/Resources/Fonts/" | grep -i bedstead
```

Expected: `BUILD SUCCEEDED`, the helper's version line, and `bedstead.otf` with its licence note. Write `$ACCEPT` down: every later step uses it.

- [ ] **Step 2: Write the helper script.** It replaces the variables of the protocol, as `recette.sh` did for the app (`notes.md`, 11/10), and only ever reads the real database through a snapshot.

```bash
cat > "$ACCEPT/recette.sh" <<'EOF'
#!/bin/zsh
# Antiope acceptance helper (phase 14, never committed). Reads the real database only through a
# read-only online backup into this directory; never writes to it.
set -euo pipefail
ACCEPT=${0:A:h}
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
HELPER=${HELPER:-/Applications/Ressac.app/Contents/Helpers/ressac}

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
      SELECT 'folders', count(*) FROM folder;
      SELECT 'feeds ' || state, count(*) FROM feed GROUP BY state;
      SELECT 'pages', count(*), min(page), max(page) FROM v_page;
      SELECT 'unread', count(*) FROM entry WHERE read_at IS NULL;
      SELECT 'to triage', count(*) FROM feed_health h JOIN feed f ON f.id = h.feed_id
        WHERE h.needs_triage = 1 AND f.state = 'active';" ;;
  checks)
    s=${2:-$(snap)}
    sqlite3 -readonly "$s" "
      SELECT 'every folder and feed has a page',
        (SELECT count(*) FROM folder) + (SELECT count(*) FROM feed) = (SELECT count(*) FROM v_page);
      SELECT 'no page used twice', NOT EXISTS (SELECT page FROM v_page GROUP BY page HAVING count(*) > 1);
      SELECT 'no reserved page used',
        NOT EXISTS (SELECT 1 FROM v_page WHERE page IN (100, 101, 102, 103, 800, 801, 802, 899));
      SELECT 'all in 110-799', NOT EXISTS (SELECT 1 FROM v_page WHERE page NOT BETWEEN 110 AND 799);
      SELECT 'root folder', page, title FROM v_page WHERE kind = 'folder' AND parent_page IS NULL ORDER BY page;" ;;
  pages)
    s=${2:-$(snap)}
    HOME="$ACCEPT/home" "$HELPER" --db "$s" page list ;;
  find)
    q=${2//\'/}
    sqlite3 -readonly "$(snap)" "SELECT page, kind, title FROM v_page WHERE title LIKE '%$q%' ORDER BY page;" ;;
  unknown)
    sqlite3 -readonly "$(snap)" "
      WITH RECURSIVE n(x) AS (SELECT 110 UNION ALL SELECT x + 1 FROM n WHERE x < 799)
      SELECT x FROM n
      WHERE x NOT IN (SELECT page FROM v_page) AND x NOT IN (100, 101, 102, 103, 800, 801, 802, 899)
      ORDER BY abs(x - 437) LIMIT 1;" ;;
  odd)
    sqlite3 -readonly -separator $'\t' "$(snap)" "
      SELECT 'feed', page, replace(title, char(10), ' ') FROM v_page WHERE kind = 'feed'
      UNION ALL
      SELECT 'entry', d.page, replace(e.title, char(10), ' ') FROM entry e JOIN feed d ON d.id = e.feed_id
        WHERE e.title IS NOT NULL;" \
    | python3 -I -c '
import sys
def odd(c):
    o = ord(c)
    return 0x300 <= o <= 0x36F or 0x2600 <= o <= 0x27BF or 0x3000 <= o <= 0x9FFF or 0xAC00 <= o <= 0xD7AF or o >= 0x1F000
for line in sys.stdin:
    parts = line.rstrip("\n").split("\t", 2)
    if len(parts) == 3 and any(odd(c) for c in parts[2]):
        print(*parts, sep="\t")' | head -n 20 ;;
  images)
    sqlite3 -readonly "$(snap)" "
      SELECT d.page, e.id, substr(e.title, 1, 60) FROM entry e JOIN feed d ON d.id = e.feed_id
      WHERE coalesce(e.content_html, e.summary_html) LIKE '%<img%' ORDER BY e.published_at DESC LIMIT 5;" ;;
  largest)
    sqlite3 -readonly "$(snap)" "
      WITH RECURSIVE tree(root, id) AS (
        SELECT id, id FROM folder WHERE parent_id IS NULL
        UNION ALL SELECT t.root, f.id FROM folder f JOIN tree t ON f.parent_id = t.id)
      SELECT r.page, r.name, count(e.id) AS entries
      FROM tree t JOIN folder r ON r.id = t.root JOIN feed d ON d.folder_id = t.id JOIN entry e ON e.feed_id = d.id
      GROUP BY t.root ORDER BY entries DESC LIMIT 3;" ;;
  decisions)
    sqlite3 -readonly "$(snap)" "
      SELECT d.id, f.page, f.title, d.decision, d.decided_at FROM garden_decision d JOIN feed f ON f.id = d.feed_id
      ORDER BY d.id DESC LIMIT 5;" ;;
  *)
    print "usage: recette.sh snap | summary [db] | checks [db] | pages [db] | find TEXT | unknown | odd | images | largest | decisions" ;;
esac
EOF
chmod +x "$ACCEPT/recette.sh"
printf '# Journal de recette Antiope (%s)\n\n' "$(date +%d/%m/%Y)" > "$ACCEPT/journal.md"
```

The journal is where Claude notes what Clément reports and what the snapshots show, one timestamped line per observation (`printf '%s %s\n' "$(date +%H:%M)" "…" >> "$ACCEPT/journal.md"`); Task 14.3 turns it into the `notes.md` table.

- [ ] **Step 3: Pre-flight: migrate a snapshot with the new helper.** Before the real database meets the new app, the same migration runs on a copy.

```bash
REAL="$HOME/Library/Application Support/Ressac/ressac.sqlite"
sqlite3 -readonly "$REAL" ".backup '$ACCEPT/preflight.sqlite'"
sqlite3 -readonly "$ACCEPT/preflight.sqlite" "SELECT version FROM schema_meta"
time HOME="$ACCEPT/home" "$APP/Contents/Helpers/ressac" --db "$ACCEPT/preflight.sqlite" migrate
ls "$ACCEPT"/preflight.sqlite.bak-v*
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" summary "$ACCEPT/preflight.sqlite"
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" checks "$ACCEPT/preflight.sqlite"
HELPER="$APP/Contents/Helpers/ressac" "$ACCEPT/recette.sh" pages "$ACCEPT/preflight.sqlite" | head -n 30
```

Expected: version `2` before; the migration in a few seconds; `preflight.sqlite.bak-v2`; `schema|3`; every `checks` line ending in `|1`; the root folders on `200`, `300`, `400`, `500` in display order (Blogs, Biblis, Infos, Jon per `notes.md`); `page list` starting with the eight reserved pages (`100 reserved - Index`, …), then folders and feeds. Any `|0`, a reserved page in use, or a migration error: stop here and report to Clément with the output; the real database has not been touched.

- [ ] **Step 4: Prepare the checklist's concrete values** on the pre-flight copy (same numbers the real database will get, since assignment is deterministic):

```bash
P=$ACCEPT/preflight.sqlite
sqlite3 -readonly "$P" "SELECT page, title FROM v_page WHERE title LIKE 'Courrier international%' OR page = 412;"
sqlite3 -readonly "$P" "WITH RECURSIVE n(x) AS (SELECT 110 UNION ALL SELECT x + 1 FROM n WHERE x < 799)
  SELECT x FROM n WHERE x NOT IN (SELECT page FROM v_page) AND x NOT IN (100,101,102,103,800,801,802,899)
  ORDER BY abs(x - 437) LIMIT 1;"
```

and with `REAL` snapshots once the app has migrated (Task 14.2), `recette.sh odd`, `recette.sh images` and `recette.sh largest`. Note in the journal: the page to type for item 2 (Courrier international if it exists, else page 412's feed, else the first feed page in the 400s), the unknown page (the free number closest to 437), two titles with emoji, CJK or combining marks for item 6, one article with images for item 4.

### Task 14.2: Install the build and let the app migrate the real database

**Files:** none in the repository.

- [ ] **Step 1: Ask Clément to quit Ressac**, then check:

```bash
pgrep -x Ressac && echo "still running" || echo "quit"
```

Expected: `quit`. Do not go on while it runs.

- [ ] **Step 2: Install, keeping the previous build for a rollback.** These are the README's commands (section "The Mac app"), with the old copy moved aside instead of overwritten:

```bash
[ -d /Applications/Ressac.app ] && mv /Applications/Ressac.app "$ACCEPT/Ressac-previous.app"
cp -R "$APP" /Applications/
touch /Applications/Ressac.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Ressac.app
/Applications/Ressac.app/Contents/Helpers/ressac --version
```

Expected: the new helper's version line. The `touch` and `lsregister` refresh the Dock icon (`notes.md`, icon note).

- [ ] **Step 3: Clément opens Ressac** from `/Applications`. The launch screen says "Updating the database…" for a moment, then the native window shows his folders. Then:

```bash
ls -l "$HOME/Library/Application Support/Ressac/ressac.sqlite.bak-v2"
sqlite3 -readonly "$HOME/Library/Application Support/Ressac/ressac.sqlite.bak-v2" "SELECT version FROM schema_meta"
"$ACCEPT/recette.sh" summary
"$ACCEPT/recette.sh" checks
```

Expected: the backup exists with version `2`; the summary says `schema|3` with the same folder, feed, page and unread counts as the pre-flight (unread may differ if Clément read in between); every check `|1`. Journal line: the starting point (schema 2 → 3, folders, active and buried feeds, pages and their range, unread, feeds to triage). If the app shows a migration failure instead, stop: the core rolled back and kept the backup (spec §10); report the launch screen's message to Clément and restore with `mv "$ACCEPT/Ressac-previous.app" /Applications/Ressac.app` only on his word.

### Task 14.3: The checklist, performed by Clément

**Files:** none in the repository (observations go to `$ACCEPT/journal.md`).

- [ ] **Step 1: Send the checklist and wait.** Send it in French, as written, with the values of Task 14.1 Step 4 filled in. One answer per item: OK, or what went wrong.

> Recette Antiope, sur ta vraie base (déjà passée en v3, sauvegarde `ressac.sqlite.bak-v2` à côté). Je ne touche ni au clavier ni à la souris : je lis seulement des copies de la base.
>
> 1. **Bascule** : Settings ▸ Appearance sur Antiope, la fenêtre devient noire sans relancer ; reviens en Natif puis encore en Antiope : la sélection, l'article ouvert et le texte de recherche sont conservés. Refais la bascule pendant un rafraîchissement complet (`⇧⌘R`) : pas de plantage, le rafraîchissement continue.
> 2. **Pages** : tape `<page de l'item 2>` hors de tout champ de texte : l'en-tête affiche `P4__`, puis la page s'ouvre. Tape ensuite `<page inconnue>` : `P<page inconnue> NOT FOUND` en rouge pendant 2 s, la sélection ne bouge pas. Essaie aussi `⌫` (CORRECTION) et `Esc` (ANNULATION) en cours de saisie.
> 3. **Lecture** : lis quelques articles avec SUITE / RETOUR seulement (Espace, `j`, `k`, ou les touches cliquables de la barre) ; l'en-tête du lecteur affiche `<page> 3/20` et suit.
> 4. **Mosaïques** : ouvre `<article avec images>` ; les images sont en mosaïque ; clique une mosaïque (l'original apparaît), puis `i` deux fois (toutes en original, puis toutes en mosaïque).
> 5. **Jardin** : tape `800`, passe dans la colonne des flux avec `→`, trie un flux avec `k`, `r` ou `b`, puis annule avec `u` (ou `⌘Z`). La ligne perd son point jaune et affiche sa décision, la barre de progression avance puis recule.
> 6. **Séparateurs** : va sur `<page du titre 1>` et `<page du titre 2>` (titres avec émoji, CJK ou accents combinés) : les barres bleues entre colonnes restent parfaitement droites, les caractères absents sont des `?` magenta.
> 7. **Tailles** : `⌘+` et `⌘−` de 15 à 30 points, `⌘0` revient à 20 ; repasse en Natif : sa taille n'a pas bougé (chaque apparence garde la sienne, `⌘0` y revient à 17) ; puis, en Antiope, Settings ▸ largeur « 40 columns » : le texte reste net, les titres en double hauteur aussi, aucune colonne ne se décale.
> 8. **VoiceOver** (`⌘F5`) sur la colonne d'index : il lit les vrais titres (accents, émojis) et les compteurs, jamais les `?` ; les touches Minitel sont annoncées (« Sommaire, go to page 100 »).
> 9. **Écran CRT** : coche Settings ▸ CRT screen : clics et défilement tombent au bon endroit, les lignes restent nettes à toutes les tailles, l'article défile sous des lignes fixes ; la case est grisée en Natif.

- [ ] **Step 2: Observe while he tests.** Useful at each item, all on snapshots:

```bash
"$ACCEPT/recette.sh" decisions      # item 5: the decision, then its undo
"$ACCEPT/recette.sh" summary        # unread count moving during item 3, feeds to triage during item 5
```

Add one journal line per item with what Clément reported and what the database showed. Item 5 is confirmed independently when the decision appears in `decisions` and the undo removes it (or marks it, depending on the core's undo); note which.

- [ ] **Step 3: A KO is a decision, not a fix.** For any item that fails, describe to Clément what was observed (his words, the snapshot's figures) and fix it only with his go-ahead, in the phase file that owns the code, with that phase's tests and gate; then repeat the item. Nothing in this phase's three files changes behaviour.

### Task 14.4: Record the acceptance and the findings in `notes.md`

**Files:**
- Modify: `notes.md`

- [ ] **Step 1: Insert the section** before the line `## Paysage existant (10/10/2026)` (after the "Appli Mac (sous-projet 2a)" section), filled from the journal, phase 13's hand-off and Task 14.2. The angle brackets are the parts to replace; keep the French, without em dash (use a colon, a comma or parentheses instead).

```markdown
## Thème Antiope (sous-projet 4)

### Recette manuelle (<JJ/MM/AAAA>)

Base : la vraie base (`~/Library/Application Support/Ressac/ressac.sqlite`), migrée de la v2 à la v3 par l'appli à son premier lancement, sauvegarde `ressac.sqlite.bak-v2` à côté. Appli : build Release installé dans `/Applications` (l'ancien gardé à part jusqu'à la fin de la recette). Recette faite par Clément ; Claude n'a envoyé aucune frappe ni aucun clic, il a seulement lu des copies de la base (`sqlite3 -readonly … .backup`) avec un petit `recette.sh` jetable et tenu le journal. Aucun test n'a écrit dans la vraie base. Migration essayée d'abord sur une copie (<durée>, mêmes numéros de page).

- Point de départ : schéma 2 puis 3, <N> dossiers, <N> flux actifs et <N> enterrés, <N> pages attribuées (de <min> à <max>), dossiers racine en <200, 300, 400, 500>, <N> entrées non lues, <N> flux à trier.

| # | Vérification (spec §11) | Résultat | Observations |
|---|---|---|---|
| 1 | Passage en Antiope et retour sans relancer | <OK ou KO> | <sélection, article et recherche conservés ; bascule pendant un rafraîchissement> |
| 2 | `<page>` puis une page inconnue (`<page inconnue>`) | <OK ou KO> | <affichage `P4__`, NOT FOUND 2 s, CORRECTION et ANNULATION> |
| 3 | Lecture avec SUITE / RETOUR seulement | <OK ou KO> | <sous-pages, Espace qui fait défiler avant de passer> |
| 4 | Mosaïque vers l'original et retour avec `i` | <OK ou KO> | <article testé, temps de chargement, échecs `IMAGE`> |
| 5 | Tri d'un flux en P800 puis annulation | <OK ou KO> | <flux, décision vue dans la base puis annulée> |
| 6 | Séparateurs droits avec émojis ou CJK | <OK ou KO> | <titres testés, `?` magenta> |
| 7 | Tailles 15 à 30 et largeur 40 colonnes | <OK ou KO> | <netteté, double hauteur> |
| 8 | VoiceOver sur la colonne d'index | <OK ou KO> | <ce qui est lu> |
| 9 | Écran CRT : clics, défilement, lignes nettes, case grisée en Natif | <OK ou KO> | <remarques, contraste augmenté> |

<Pour chaque KO : ce qui s'est passé, et la suite décidée avec Clément.>

### Trouvailles

- **Fluidité de l'écran CRT (phase 13)** : enregistrement Animation Hitches de 20 s en faisant défiler le plus gros dossier (<nom>, page <page>, <N> entrées) : <ratio> ms/s avec l'écran CRT, <ratio> ms/s sans (seuil retenu : 5 ms/s). Décision : <bavure complète | lueur seule>.
- **Contraste augmenté** : <la bavure disparaît, lignes et vignette restent : vérifié ou non>.
- **Bedstead sur la vraie base** : <glyphes absents rencontrés (les `?` magenta) et titres concernés, ou aucun>.
- **Numéros de page** : <répartition réelle : centaines des dossiers racine, centaines pleines ou non, flux hors dossier dans 110 à 199>.
- <Autre trouvaille de la recette, une ligne chacune.>
```

- [ ] **Step 2: Check the French.** `grep -n "—" notes.md` prints nothing new in the section (the only matches, if any, are older lines). `git diff notes.md` shows one inserted section and no other change.

- [ ] **Step 3: Commit.**

```bash
git add notes.md
git commit -m "notes.md : recette du thème Antiope sur la vraie base et trouvailles" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 14.5: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Check the names the README will quote.** `grep -n "Tab(\|Picker(\"Appearance\|Toggle(\"CRT\|40 columns" apps/macos/Ressac/Settings/*.swift` gives the Settings tab that holds Appearance (the text below says Reading; use the real one) and confirms the "40 columns" label.

- [ ] **Step 2: Subcommands.** In the "More subcommands" block, replace the line

```
ressac migrate                         # bring the database to the current schema (v2)
```

with

```
ressac migrate                         # bring the database to the current schema (v3)
ressac page list                       # every page number, reserved ones first; --json for NDJSON
ressac page set feed 12 412            # renumber a feed (or a folder) by hand, within 110-799
```

- [ ] **Step 3: Schema note.** Replace the paragraph that starts with `**Schema v2.**` by:

```markdown
**Schema v3.** The Mac app migrates the shared database to schema v3 on first launch, after a backup next to it (`ressac.sqlite.bak-v2`, or `ressac.sqlite.bak-v1` for an older database). v3 gives every folder and feed a three-digit page number, used by the Antiope look and listed by `ressac page list`; numbers never change on their own. A `ressac` built before v3 refuses the database ("database created by a newer Ressac"). Get a v3 CLI with `cargo install --path crates/ressac`, or use the app's own copy at `Ressac.app/Contents/Helpers/ressac`.
```

- [ ] **Step 4: Settings bullet.** In "The Mac app", replace

```
- **Settings (`⌘,`):** article font (SF Pro, New York, Charter, Newsreader, Atkinson Hyperlegible Next), text size (`⌘+` / `⌘−` / `⌘0` in the View menu), line width, database location, logs.
```

with

```
- **Settings (`⌘,`):** appearance (Native or Antiope, see below), article font (SF Pro, New York, Charter, Newsreader, Atkinson Hyperlegible Next), text size (`⌘+` / `⌘−` / `⌘0` in the View menu), line width (three widths or exactly 40 columns), database location, logs.
```

- [ ] **Step 5: The Antiope section.** Append at the end of the file, after the paragraph about Newsreader and Atkinson Hyperlegible Next:

````markdown

## Antiope

Settings ▸ Reading ▸ Appearance switches the reading window to **Antiope**, a French teletext set of the 1980s, and back, without a relaunch: the Bedstead pixel font everywhere, the eight teletext colours on black, a page header with a running clock, three columns counted in character cells (index, entries, reader) and the Minitel keys at the bottom. Nothing is lost either way: selection, open article and search text are shared with the native look.

- **Pages.** Every folder and feed has a three-digit page number, stored in the database and shared with the terminal (`ressac page list`). Type the three digits anywhere outside a text field to jump; `⌫` (CORRECTION) erases a digit, `Esc` (ANNULATION) cancels. An unknown page shows `NOT FOUND` for two seconds.

  | Page | Shows |
  |---|---|
  | 100 | SOMMAIRE, every page with its unread count |
  | 101, 102, 103 | Unread, Today, Starred |
  | 110–199 | Feeds outside any folder |
  | 200–700 | Root folders (six at most), each followed by its subfolders and feeds |
  | 800, 801, 802 | Garden: Needs triage, All feeds, Cemetery |
  | 899 | GUIDE, every key of the window |

- **Minitel keys** (also clickable): SOMMAIRE `⌥S`, SUITE `Space` or `j`, RETOUR `k`, GUIDE `⌥G` or `?`, RÉPÉTITION `⌥R` (refresh the selection). `←` and `→` move between the columns, `⌘F` opens the `SEARCH?` prompt. An article is a sub-page of its feed's page ("412 3/20").
- **Images** show as two-colour block mosaics. Click one, or press `i` for the whole article, to see the originals.
- **Garden.** Pages 800 to 802 list the feeds by status under coloured headings, with a progress row and the usual keys (`k` keep, `r` repair, `b` bury, `u` undo, `o` site, `w` Wayback, `⇧R` safe repairs); the reader column tells why a feed is there.
- **Text size** snaps to 15, 20, 25 or 30 pt in Antiope (`⌘+` / `⌘−`, `⌘0` for 20), so that the pixels stay sharp. Each look keeps its own size: switching looks never changes the other one.
- **CRT screen** (Settings, Antiope only): scanlines, a vignette, rounded tube corners and a slight colour bleed. Increase Contrast in the accessibility settings turns the bleed off.

Bedstead (Ben Harris) is dedicated to the public domain (CC0); its licence note ships next to it in `Ressac.app/Contents/Resources/Fonts/`.
````

- [ ] **Step 6: Check.** `git diff README.md`: the three replaced lines or paragraphs, the new section, nothing else; the table renders (each row starts and ends with `|`); every key named matches the plan index's Global Constraints table.

- [ ] **Step 7: Commit.**

```bash
git add README.md
git commit -m "README: Antiope, ressac page list/set, schema v3" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 14.6: `TODO.md`

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Date line.** Replace `Prochaines étapes de Ressac, au 11/10/2026.` with `Prochaines étapes de Ressac, au <JJ/MM/AAAA>.` (today's date).

- [ ] **Step 2: Sub-projects.** Replace the whole section from `## Sous-projets suivants` to the line before `## Reporté (relectures du sous-projet 2a)` with:

```markdown
## Sous-projets suivants

Ordre convenu le 14/10/2026 : 3, puis 2b.

- [ ] **3 : rythme et journal du matin.** Tri des flux par rythme de publication, édition du matin.
- [ ] **2b : la lecture dans le terminal.** Trois colonnes en ratatui sur le même schéma (v3) : lu, favori, recherche, article complet, avec clappuccino comme référence. Habillage Antiope du terminal compris (palette et numéros de page du sous-projet 4).
- [x] **4 : thème Antiope (appli Mac).** Fait le <JJ/MM/AAAA> sur la branche `antiope` ; recette et trouvailles dans `notes.md`.

```

- [ ] **Step 3: Mise en service.** Ask Clément which items of "Mise en service" are done (his real database exists since 14/10, so the build and the first launch at least) and tick those (`- [ ]` → `- [x]`); change nothing else in that section.

- [ ] **Step 4: Deferred items from the acceptance.** If Task 14.3 or 14.4 left points that Clément chose to postpone, add after the last `### …` subsection of "Reporté":

```markdown
### Antiope (sous-projet 4)

- [ ] <point reporté, une ligne chacun, sans tiret cadratin>
```

and nothing if there are none.

- [ ] **Step 5: Check and commit.** `grep -n "—" TODO.md` prints no new line; `git diff TODO.md` shows only the changes above.

```bash
git add TODO.md
git commit -m "TODO.md : sous-projet 4 terminé, ordre 3 puis 2b" \
  -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 14.7: Final gate, push and pull request

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

- [ ] **Step 2: Push.**

```bash
git -c credential.helper= -c credential.helper='!gh auth git-credential' push -u origin antiope
```

- [ ] **Step 3: Write the PR body**, with the counts of Step 1 and the results of Task 14.3 filled in:

```bash
cat > "$ACCEPT/pr.md" <<'EOF'
## Summary

Sub-project 4: **Antiope**, a second, complete look for Ressac.app, after French teletext of the 1980s. Spec: `docs/specs/2026-10-14-antiope-design.md`; plan: `docs/plans/2026-10-14-antiope.md` and `docs/plans/antiope/`.

- **Core, schema v3:** `folder.page`, `feed.page` and the view `v_page`; page numbers assigned once at migration, import and every new feed, never renumbered on their own; `ressac page list [--json]` and `ressac page set feed|folder <id> <page>`.
- **App:** Settings ▸ Appearance (Native / Antiope), switched live; Bedstead everywhere, the eight teletext colours, page header with clock, three cell-grid columns, three-digit page entry, Minitel keys, P100 SOMMAIRE and P899 GUIDE, search prompt.
- **Reader:** Antiope stylesheet, sizes 15 to 30 pt, 40 columns, double-height titles, sub-pages ("412 3/20"); images as mosaics with a swap to the original (`i`).
- **Garden:** P800 to P802 by status with coloured headings and a mosaic progress row, the inspector as a teletext page, banners as a red row, `REFRESHING n/m` in the Minitel bar.
- **CRT screen:** optional scanlines, vignette, tube corners and colour bleed; off with Increase Contrast for the bleed.
- `GardenStatusStyle` replaces the two copies of the status colours.

## Tests

- Rust: <N> tests, `cargo fmt`, `cargo clippy -D warnings`.
- Swift: <N> tests (Swift Testing), zero warnings.
- Manual acceptance on the real database (migrated to v3 by the app, backup `ressac.sqlite.bak-v2`): <9/9 OK, or the list of KO and what was decided>. Details in `notes.md`, section « Thème Antiope (sous-projet 4) ».
- CRT scrolling check: <ratio> ms/s of hitches with the CRT screen, <ratio> without (threshold 5 ms/s); <full bleed kept | glow only>.

## Notes

- A `ressac` built before this branch refuses a v3 database; the README says how to get a v3 CLI.
- CI: the macOS job still skips the app build until GitHub has Xcode 27 (allowed failure).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
tail -n 1 "$ACCEPT/pr.md"
```

Expected: the last line is exactly `🤖 Generated with [Claude Code](https://claude.com/claude-code)` (the robot emoji itself, not an escape sequence; if the terminal shows `\U0001F916` or similar, rewrite that line with the character).

- [ ] **Step 4: Open the pull request, do not merge.**

```bash
gh pr create --repo w3cdotorg/ressac --base main --head antiope \
  --title "Sub-project 4: the Antiope theme" --body-file "$ACCEPT/pr.md"
gh pr view --repo w3cdotorg/ressac antiope --json url,state,mergeable
```

Expected: the PR URL, `"state":"OPEN"`. Never run `gh pr merge`: Clément reviews and merges himself.

- [ ] **Step 5: Hand over and clean up later.** Send Clément the PR link, the acceptance summary and the reminder that `$ACCEPT/Ressac-previous.app` and `ressac.sqlite.bak-v2` are the rollback. Remove the temporary directory (`rm -rf "$ACCEPT"`) only when he says the previous build is no longer needed; keep `ressac.sqlite.bak-v2` unless he asks.

## Notes for the executor

- **Values that depend on phases 4 and 9:** the Settings tab name ("Reading" in the README text) and the Minitel key accessibility label quoted in item 8 ("Sommaire, go to page 100", spec §8). Task 14.5 Step 1 checks the first; correct the checklist wording if phase 8 chose other labels.
- **Why a pre-flight copy:** the real database is migrated by the app, once, with a backup, but a surprise in page assignment (a reserved page in use, a collision between a folder and a feed) is cheaper to find on a copy. Assignment is deterministic, so the copy's page numbers are the ones the real database gets, and the checklist's values come from it.
- **`sqlite3 -readonly … .backup`** copies a consistent snapshot through SQLite's backup API, WAL content included, and never writes to the source; with the app closed and no `-shm` file, the read-only open may create the shared-memory file next to the database, which is not a change to the database.
- **Undo in item 5:** the core's `garden undo` may delete the decision row or record a reversal; `recette.sh decisions` shows which, and the journal notes it.
