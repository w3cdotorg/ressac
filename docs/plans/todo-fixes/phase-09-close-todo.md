# Phase 9: close the TODO items the plan fixed

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Runs after:** Phase 8, the last code phase, after Phases 1 to 7 (3a, 3b, 3c, 4a and 4b included), applied in order. No earlier phase edits `TODO.md`, `notes.md` or `README.md`, so the line numbers below are those of `855cd2c`; match on the quoted texts anyway.

**Goal:** docs only. `TODO.md` ticks every item Phases 1 to 8 fixed (each phase's notes list them), deletes the Tailscale line (kept by design, decided on 15/10/2026) and the CI `|| true` item of « À décider » (done in Phase 7, so the section goes), and keeps the NSTableView warning and the Antiope scroll-hitch line open. `notes.md` gets a short dated note in French. `README.md` follows the behaviour it describes that changed: `apply-safe --json`, the exit codes, the paged entry lists and the Cemetery's unread counts.

**Language:** `TODO.md` and `notes.md` are in French, `README.md` in English. The French prose has no em dash (`—`); Step checks of each task grep for it.

## Files (3)

- Modify: `TODO.md` (63 lines): lines 5-8 (« À décider »), lines 29-63 (« Reporté »).
- Modify: `notes.md` (212 lines): line 114 (CI macOS), a new section before line 151.
- Modify: `README.md` (85 lines): lines 23, 29, 54, 56.

**Step 0:** no code, nothing to clean. No test: the check is the diff, a grep for em dashes and a grep for the open items.

---

### Task 9.1: `TODO.md`

**Files:**
- Modify: `TODO.md:5-8`, `:29-63`

- [ ] **Step 1: Remove « À décider ».** Its only item, `|| true` on the Xcode detection, is done (Phase 7, Task 7.5). Replace lines 5-8:

```markdown
## À décider

- [ ] **CI macOS : `|| true` sur la détection de Xcode.** Les deux lignes `xcode=$(xcodebuild -version | head -n 1)` et `sdk=$(xcrun --sdk macosx --show-sdk-version)` de `.github/workflows/ci.yml` feraient échouer l'étape si `xcodebuild` lui-même plantait. La modification a été refusée par le garde-fou automatique de Claude Code ; le relecteur la juge sans conséquence (les images macOS de GitHub ont toujours un Xcode).

```

with nothing. Line 4 (blank) is then followed by `## Mise en service`.

- [ ] **Step 2: Tick « Reporté ».** Replace lines 29-63 (from `### Sécurité et réseau` to the end of the file):

```markdown
### Sécurité et réseau

- [ ] `discover_one` (sous-projet 1) suit une adresse de flux lue dans une page HTML avec le client non protégé : SSRF aveugle (Ressac ne lit pas la réponse). Lui appliquer la même garde que pour les articles.
- [ ] Client des articles : les variables `HTTP_PROXY`, `HTTPS_PROXY` et `ALL_PROXY` sont honorées, donc la vérification à la résolution DNS est sautée derrière un proxy. Envisager `.no_proxy()`.
- [ ] Les adresses Tailscale (plage CGNAT 100.64/10) sont refusées pour l'article complet : à assouplir si besoin.
- [ ] Pause par hôte des articles : seul l'hôte de l'entrée précédente est mémorisé (A, B, A ne fait pas de pause), et aucune pause entre le dernier flux et le premier article d'un même hôte.

### Lecture

- [ ] La liste d'articles n'a pas de limite et elle est relue à chaque marque « lu » : un dossier de plusieurs milliers d'entrées en mode « All » le sentira.
- [ ] Plafond de 256 Ko de HTML : les très gros articles s'affichent en texte simple.
- [ ] Environ 600 Ko de HTML « plat » mais invalide prennent 2 à 3 s à nettoyer, et les fils de nettoyage abandonnés peuvent se chevaucher.
- [ ] Une image sans dimensions en fin d'article peut faire passer `Espace` à l'article suivant un peu trop tôt : écrire largeur et hauteur quand le flux les donne.
- [ ] Avertissement « reentrant operation in its NSTableView delegate » : une source corrigée, une autre possible non reproduite. N'agir qu'avec une trace.

### Cœur et CLI

- [ ] `keep` ou `repair` vers une adresse déjà utilisée par un flux actif sort en code 1 au lieu de 2.
- [ ] `garden apply-safe --json` ne donne que le résumé, pas le nombre de réparations appliquées et sautées.
- [ ] Un changement d'auteur d'un article n'atteint ni `entry` ni l'index de recherche (comportement hérité de la v1).
- [ ] `v_entry` et `v_unread_count` cachent les flux enterrés : le Cimetière ne peut pas afficher leurs non-lus.
- [ ] `recompute_triage` relit toutes les lignes, deux fois par rafraîchissement.

### Appli et outillage

- [ ] Binaire `ressac` absent du paquet : message dans la fenêtre au lieu d'une alerte système (`NSAlert`) comme le dit la spec.
- [ ] Le lancement n'est pas protégé contre une double exécution, et le branchement `onDatabaseChange` n'a pas de test.
- [ ] CI macOS : aucune vérification du nombre de tests réellement exécutés ; à revoir quand GitHub aura Xcode 27.
- [ ] CI : `actions/checkout@v4` tourne sur Node 20, déprécié (avertissement GitHub) ; passer à une version Node 24.
- [ ] Indice de spam affiché sur 5 flux du vrai jeu (dont ReadWriteWeb, vivant) : le restreindre plus tard aux statuts non vivants ou au seul lexique, si ça gêne.

### Antiope (sous-projet 4)

- [ ] Défilement des gros dossiers : accrocs même sans écran CRT (38 ms/s sur la page 400), optimisation possible (voir `notes.md`).
- [ ] Mineurs des relectures restés ouverts : `DateFormatter` recréé à chaque seconde par l'en-tête, lignes de P100 et boutons de la bannière encore focalisables au clavier complet, `NSWorkspace` appelé en direct par la bannière native et `GardenInspector`, `main.rs` à découper avant le sous-projet 2b.
```

with:

```markdown
### Sécurité et réseau

- [x] `discover_one` : l'adresse de flux lue dans la page du site passe par le client protégé des articles ; refusée, elle compte comme « rien trouvé » (todo-fixes, phase 1).
- [x] Client protégé (articles, autodécouverte) : `.no_proxy()`, les variables de proxy sont ignorées ; les flux gardent le proxy (todo-fixes, phase 1).
- [x] Pause par hôte des articles : chaque hôte est mémorisé, et les flux et sondes https du même rafraîchissement comptent (todo-fixes, phase 1).

### Lecture

- [x] Listes d'articles : 500 lignes à la fois, la suite au défilement ou au clavier ; une marque « lu » ne relit plus que la page chargée et deux comptes (todo-fixes, phases 5 et 6).
- [x] Plafond du HTML analysé porté à 512 Ko (todo-fixes, phase 6).
- [x] Nettoyage : l'estimation de profondeur suit libxml, plus de 256 attributs sur une balise donnent du texte simple, un seul fil de nettoyage à la fois (todo-fixes, phase 6).
- [x] Images : largeur et hauteur reprises du style ou des attributs `data-` du flux ; en Antiope, la mosaïque garde sa hauteur avant d'arriver (todo-fixes, phase 6).
- [ ] Avertissement « reentrant operation in its NSTableView delegate » : une source corrigée, une autre possible non reproduite. N'agir qu'avec une trace.

### Cœur et CLI

- [x] `keep`, `repair` ou `undo` vers une adresse déjà utilisée par un flux actif sortent en code 2 ; l'appli le montre comme un refus, pas comme un bogue (todo-fixes, phases 3a et 3c).
- [x] `garden apply-safe --json` : la ligne `finished` donne aussi `applied` et `skipped` (todo-fixes, phase 3a).
- [x] Un changement d'auteur atteint `entry` et l'index de recherche (todo-fixes, phase 3b).
- [x] Le Cimetière compte les non-lus des flux enterrés sur `entry`, schéma inchangé ; `v_entry` et `v_unread_count` restent limitées aux flux actifs (todo-fixes, phase 4).
- [x] `recompute_triage` lit une ligne légère par flux, et un rafraîchissement ne calcule les indicateurs qu'une fois (todo-fixes, phase 3b).

### Appli et outillage

- [x] Binaire `ressac` absent du paquet : alerte système fatale au lancement (todo-fixes, phase 7).
- [x] Un seul lancement à la fois, quel que soit le nombre de fenêtres ; le branchement `onDatabaseChange` a son test (todo-fixes, phase 7).
- [x] CI macOS sur l'image de préversion `xcode-27`, avec une étape qui compare les tests déclarés aux tests exécutés (todo-fixes, phase 7).
- [x] CI : `actions/checkout@v7` (Node 24) dans les deux jobs (todo-fixes, phase 7).
- [x] Indice de spam réservé aux flux non vivants, décision du 15/10/2026 (todo-fixes, phase 3a).

### Antiope (sous-projet 4)

- [ ] Défilement des gros dossiers : accrocs même sans écran CRT (38 ms/s sur la page 400), optimisation possible (voir `notes.md`).
- [x] Mineurs des relectures : un `DateFormatter` gardé par motif et fuseau, lignes de P100 et boutons de la bannière hors de la boucle du clavier complet (todo-fixes, phase 8), liens de la bannière native et de `GardenInspector` par `AppState.openURL` (phase 7), `main.rs` découpé (phase 2).
```

The Tailscale line goes without a tick: Clément decided on 15/10/2026 to keep refusing 100.64/10 (`non_public_addresses_are_classified` pins it), and `notes.md` records it (Task 9.2).

- [ ] **Step 3: Check.**

```bash
cd /Users/willow/Sites/ressac
grep -c '^- \[ \]' TODO.md
grep -n '^- \[ \]' TODO.md
grep -c 'À décider\|Tailscale\|— ' TODO.md
```

Expected: `5`; then the five open lines: « Optionnel : lien `ressac` dans le `PATH` », sub-projects 3 and 2b, the NSTableView warning and the scroll hitches of large folders. The last `grep -c` prints `0`.

---

### Task 9.2: `notes.md`

**Files:**
- Modify: `notes.md:114`, before `:151`

- [ ] **Step 1: The CI note.** Replace line 114:

```markdown
- `macos-latest` (11/10/2026) : Xcode 26.6 et SDK macOS 26.5, donc la construction et les tests de l'appli sont sautés, échec autorisé, job vert avec un avertissement (exécution 38126512109). L'appli n'est vérifiée qu'en local tant que GitHub n'a pas Xcode 27.
```

with:

```markdown
- `xcode-27` (15/10/2026) : le job macOS passe sur cette image de préversion (macOS 27, Xcode 27.0) pour construire et tester l'appli à chaque envoi. Une étape compare les tests déclarés dans `RessacTests` aux tests exécutés (347 et 347 en local). Si l'image pose problème, revenir à `runs-on: macos-latest`, une seule ligne de `ci.yml`.
- `macos-latest` (11/10/2026) : Xcode 26.6 et SDK macOS 26.5, donc la construction et les tests de l'appli sont sautés, échec autorisé, job vert avec un avertissement (exécution 38126512109). L'appli n'est vérifiée qu'en local tant que GitHub n'a pas Xcode 27.
```

- [ ] **Step 2: The note of this plan.** Insert before line 151, `## Paysage existant (10/10/2026)`:

```markdown
## Corrections du TODO (todo-fixes, 15/10/2026)

Plan en neuf phases (`docs/plans/todo-fixes/`), rejoué dans l'ordre sur une copie du dépôt avant l'exécution. Décisions de Clément du 15/10/2026 :

- **Tailscale** : la plage CGNAT 100.64/10 reste refusée pour l'article complet, par choix. Rien à changer.
- **Indice de spam** : plus jamais sur un flux vivant (ReadWriteWeb et quatre autres l'affichaient pour du bruit). Le score reste stocké.
- **Refus du jardin** : `keep`, `repair` ou `undo` vers une adresse déjà prise sortent en code 2. L'appli y voit un refus et non un bogue de son appel : bandeau avec la phrase de la CLI, sans bouton du journal, et journal au niveau erreur. Les autres sorties en code 2 restent des fautes.
- **Cimetière** : il compte les non-lus des flux enterrés directement sur `entry`, sans changer le schéma (toujours v3).
- **CI** : job macOS sur l'image de préversion `xcode-27`, voir « CI macOS ».

Le reste : garde anti-SSRF de l'autodécouverte, client protégé sans proxy, pause par hôte sur tout le rafraîchissement, `main.rs` découpé, auteur des articles suivi, triage recalculé une fois, listes d'articles par pages de 500, nettoyage HTML borné et plafond à 512 Ko, tailles d'images reprises du flux, lancement unique, alerte quand `ressac` manque, `DateFormatter` gardés, boutons Antiope hors du clavier complet. Restent ouverts : l'avertissement NSTableView (n'agir qu'avec une trace) et les accrocs de défilement des gros dossiers en Antiope (reportés).

Comptes à la fin : `ressac` 35 tests unitaires et 19 tests de CLI, cœur 260 (et 1 ignoré), puis 1, 9 et 4 dans ses tests d'intégration ; appli 347 tests dans 84 suites, sans avertissement.

```

- [ ] **Step 3: Check.**

```bash
cd /Users/willow/Sites/ressac
grep -c '—' notes.md TODO.md
grep -n '^## ' notes.md
```

Expected: `notes.md:0` and `TODO.md:0`; the new heading sits between `## Thème Antiope (sous-projet 4)` and `## Paysage existant (10/10/2026)`.

---

### Task 9.3: `README.md`

**Files:**
- Modify: `README.md:23`, `:29`, `:54`, `:56`

- [ ] **Step 1: `apply-safe --json` and the exit codes.** Replace line 23:

```markdown
ressac garden apply-safe               # apply every undecided safe repair
```

with:

```markdown
ressac garden apply-safe               # apply every undecided safe repair; --json also gives applied and skipped
```

Replace line 29:

```markdown
Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
```

with:

```markdown
Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
Exit codes: `0` done, `1` error, `2` a refused request (an unknown id, nothing to repair, an address another active feed already uses), `130` stopped by Ctrl-C.
```

- [ ] **Step 2: Paged lists and the Cemetery's counts.** In line 54 (`- **Reading:** …`), replace:

```markdown
`⌘↩` open in the browser. Refresh is manual only.
```

with:

```markdown
`⌘↩` open in the browser. Refresh is manual only. Long lists load 500 entries at a time; scrolling, `j` or `Space` loads the next ones.
```

In line 56 (`- **Garden:** …`), replace:

```markdown
`o` site, `w` Wayback, `⇧R` apply safe repairs.
```

with:

```markdown
`o` site, `w` Wayback, `⇧R` apply safe repairs. The Cemetery shows how many entries each buried feed left unread.
```

- [ ] **Step 3: Check and commit.**

```bash
cd /Users/willow/Sites/ressac
git diff --stat
git add TODO.md notes.md README.md
git commit -F - <<'EOF'
Docs: close the TODO items fixed by the todo-fixes plan

TODO.md ticks what phases 1 to 8 fixed, drops the Tailscale line (kept by
design) and the « À décider » section; notes.md records the decisions of
15/10/2026; the README follows apply-safe --json, the exit codes, the
paged entry lists and the Cemetery's counts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

Expected: `3 files changed`.

- [ ] **Step 4: Stop.** Report the diff and wait for Clément's approval. The plan is done.

---

## Notes for the executor

- **Ticked, not removed.** Every fixed item stays in `TODO.md` as `[x]` with its phase, as the file already does for sub-project 4. Only the Tailscale line (decided, nothing done) and the « À décider » section (its one item done) go.
- **What stays open:** the NSTableView reentrancy warning (act only with a trace) and the Antiope large-folder scroll hitches (postponed), both untouched by the plan, plus « Mise en service » and « Sous-projets suivants », which the plan does not concern.
- **README, left alone:** the proxy, the SSRF guard, the per-host pacing, the sanitiser bounds, the image sizes, the launch guard and the fatal alert. The README does not describe them.
