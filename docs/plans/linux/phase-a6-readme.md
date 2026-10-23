# Phase A6: README "Linux", `notes.md`, `TODO.md`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Part A is documented: the README says where Ressac keeps its files on Linux, how to install the terminal binary, how to wire the tmux ticker and what to watch in a VM; it also stops saying macOS-only things where Part A changed them (ticker state per database, SIGTERM, Linux tarballs); `notes.md` records Part A's decisions and findings in French; `TODO.md` gets the follow-ups.

**Runs after:** A5 (A1–A5 merged). Master plan (phase table row 6; Global Constraints: Language, `notes.md` in French without em dash in French prose); spec §5.1, §5.2 (Konsole images TODO), §5.3; the ledger `.superpowers/sdd/2026-10-23-linux/progress.md` (A1–A5 rows: deviations, A4's numbers, A5's replay times).

**Files (3 counted):**

| File | Task | Responsibility |
|---|---|---|
| `README.md` | 1 | Intro, Usage paths line, exit code 130, terminal reader settings and logs, tmux state files, new "## Linux" section, Releases paragraph |
| `notes.md` | 2 | New section "## Ressac sous Linux (sous-projet 8)" (French) |
| `TODO.md` | 2 | Sub-project 8 line; new "### Linux (sous-projet 8)" section |

**Interfaces consumed (what the text must match, as merged):** A1 paths (`~/.local/share/ressac/ressac.sqlite` with `-wal`, `-shm`, `.bak-vN`; `~/.config/ressac/terminal.json`; `~/.local/state/ressac/` for `ticker.json`, `ticker-<16 hex>.json`, their `.lock`, and `logs/`; `~/.cache/ressac/` reserved; relative or empty `XDG_*` ignored; no usable `HOME` is an error), A2 (state directory created only when the database exists), A3 (the busy sentence, `open::that_detached`, SIGTERM, the legacy warning), A4 (`apps/linux/dev/dev.sh check`), A5 (the Linux tarball names). **Before writing, read `crates/ressac/src/main.rs` as merged** for two facts the text states: which commands A3's SIGTERM handling covers (the controller's summary: commands that watch the stop flag stop cleanly with 130; the reader and garden TUIs keep the default SIGTERM), and the legacy warning's exact wording. Where the code says otherwise than this plan, the code wins and the ledger gets a line.

**Step 0:** not applicable (Markdown).

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

### Task 1: the README

**Files:**
- Modify: `README.md` (line 3 intro; line 44 database and logs; line 43 exit codes; "The terminal reader" Settings and Logs paragraphs; "The tmux ticker" rotation and log sentences; new "## Linux" between "## The tmux ticker" and "## The Mac app"; "**Releases.**" paragraph)

**Interfaces:**
- Consumes: the facts listed in the header.
- Produces: the anchor `#linux` (other sections link to it).

- [ ] **Step 1: Edit the existing lines**
  - Intro (French, no em dash): "Un lecteur de flux RSS/Atom/JSON Feed pour macOS, en deux interfaces" becomes "Un lecteur de flux RSS/Atom/JSON Feed pour macOS (et Linux pour le terminal), en deux interfaces".
  - Exit codes: "`130` stopped by Ctrl-C" becomes "`130` stopped by Ctrl-C (or SIGTERM, for the commands that stop cleanly)". Match A3's coverage as merged.
  - "The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`." becomes "On macOS the database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`; on Linux, see [Linux](#linux)."
  - Terminal reader, **Settings**: append " (on Linux, `~/.config/ressac/terminal.json`)". **Logs**: "`~/Library/Logs/Ressac/`" becomes "`~/Library/Logs/Ressac/` (on Linux `~/.local/state/ressac/logs/`)".
  - tmux: "The rotation is kept in `~/Library/Application Support/Ressac/ticker.json`." becomes "The rotation of the default database is kept in `ticker.json`, next to it on macOS (`~/Library/Application Support/Ressac/`) and in `~/.local/state/ressac/` on Linux; another database (`--db` or `RESSAC_DB`) keeps its own rotation in `ticker-<16 hex digits>.json` in the same directory, so two databases never share one, whatever path spelling names them." And "(set `RUST_LOG=debug` to see it in `~/Library/Logs/Ressac/`)" gains "or `~/.local/state/ressac/logs/` on Linux".
  - **Releases.** paragraph: after the macOS files, add "and `ressac-X.Y.Z-linux-x86_64.tar.gz` / `ressac-X.Y.Z-linux-aarch64.tar.gz` (the terminal binary for Linux, built on Ubuntu 22.04), all listed in `SHA256SUMS`"; "builds the Release app on the `xcode-27` runner" becomes "builds the Release app on the `xcode-27` runner and the Linux binaries on `ubuntu-22.04` and `ubuntu-22.04-arm`".

- [ ] **Step 2: Add the "## Linux" section** (English, as the rest of the README):

````markdown
## Linux

The terminal side runs on Linux: the reader, the garden, the tmux ticker and every subcommand. It keeps its own database; nothing is shared with a Mac. (A KDE app is on its way, see `docs/specs/2026-10-23-linux-design.md`.)

**Where the files live.** Ressac follows the XDG base directories. An `XDG_*` variable that is empty or not an absolute path is ignored, and Ressac refuses to run without an absolute `HOME` rather than write anywhere relative.

| File | Linux | macOS (unchanged) |
|---|---|---|
| Database `ressac.sqlite` (and `-wal`, `-shm`, the `.bak-vN` backups) | `$XDG_DATA_HOME/ressac/`, by default `~/.local/share/ressac/` | `~/Library/Application Support/Ressac/` |
| `terminal.json` (the reader's look and images switch) | `$XDG_CONFIG_HOME/ressac/`, by default `~/.config/ressac/` | next to the database |
| Ticker rotation (`ticker.json`, `ticker-<hash>.json`, their locks) | `$XDG_STATE_HOME/ressac/`, by default `~/.local/state/ressac/` | next to the database |
| Logs | `$XDG_STATE_HOME/ressac/logs/` | `~/Library/Logs/Ressac/` |

`--db` and `RESSAC_DB` still win over the default database. If a database exists at the macOS path under your Linux home (`~/Library/Application Support/Ressac/ressac.sqlite`, left by a `ressac` built before Linux support), each run names it in the log; Ressac never moves it. Move it to `~/.local/share/ressac/` yourself, or pass it with `--db`.

**Install.** Either download `ressac-X.Y.Z-linux-<arch>.tar.gz` from a release (`x86_64` or `aarch64`, glibc 2.35 or newer) and `tar -xzf` it into `~/.local/bin`, or build it:

```sh
sudo dnf install rust cargo gcc gcc-c++ cmake clang   # Fedora; other distributions: a Rust toolchain and a C compiler
cargo install --locked --path crates/ressac           # installs ~/.cargo/bin/ressac
```

Links open in your browser through `xdg-open` (package `xdg-utils`, present on desktop installs), without the terminal waiting for it.

**tmux.** The same lines as in [The tmux ticker](#the-tmux-ticker), with the Linux binary:

```tmux
# Ressac ticker: a recent unread entry the Journal left out, every 15 s; click to open it.
set -g status-right-length 160
set -g status-right '#(~/.cargo/bin/ressac ticker --tmux) "#{=21:#{q/h:pane_title}}" %H:%M %d-%b-%y'
bind -n MouseDown1Status if -F '#{m/r:^rs[0-9]+$,#{mouse_status_range}}' \
  "run-shell -b '~/.cargo/bin/ressac ticker open #{q:mouse_status_range}'" \
  'switch-client -t ='
```

(`~/.local/bin/ressac` if you installed a release tarball there.)

**In a virtual machine** (Parallels, UTM, VMware):

- Keep the database on the VM's own disk. Never point `--db` or `RESSAC_DB` at a shared folder from the host (Parallels mounts them under `/media/psf/`): SQLite's WAL mode needs a local file system, and a shared folder can corrupt the database or leave it locked.
- Check the VM's time zone (`timedatectl`; set it with `sudo timedatectl set-timezone Europe/Paris`). A VM is often left on UTC, and the Journal's day starts at 04:00 local time, so a wrong zone builds the morning edition at the wrong hour and moves "Today".

**Developing on Linux.** `apps/linux/dev/dev.sh check` runs the whole check (format, clippy, tests, the chart's tests and a smoke test of the paths above) in a Fedora 44 container under OrbStack, as a non-root user; `dev.sh sh` opens a shell there. The script only ever touches its own container, `ressac-fedora`.
````

- [ ] **Step 3: Verify the claims the section makes**
  - Anchors: `grep -n '](#linux)\|](#the-tmux-ticker)' README.md` lists the links; both headings exist (`grep -n '^## Linux$\|^## The tmux ticker$' README.md`).
  - The tmux snippet parses: extract it and source it in the dev container (tmux is in `packages.txt`):
    ```bash
    awk '/^## Linux$/{l=1} l&&/^```tmux$/{t=1;next} t&&/^```$/{exit} t' README.md > "$TMPDIR/ressac-linux.tmux"
    docker cp "$TMPDIR/ressac-linux.tmux" ressac-fedora:/tmp/ressac-linux.tmux
    apps/linux/dev/dev.sh sh 'tmux -L readme -f /dev/null new-session -d \; source-file /tmp/ressac-linux.tmux \; kill-server && echo tmux-ok'
    ```
    Expected: `tmux-ok`.
  - The build line works on a bare Fedora 44 (throwaway container, new name, `--rm`):
    ```bash
    docker run --rm --name ressac-readme-check --platform linux/arm64 -v "$PWD:/src:ro" fedora:44 bash -euo pipefail -c '
      dnf install -y rust cargo gcc gcc-c++ cmake clang tar >/dev/null
      mkdir /w && tar -C /src --exclude=./target --exclude=./apps/macos/build --exclude=./.git -cf - . | tar -C /w -xf -
      cd /w && cargo install --locked --path crates/ressac --root /tmp/inst && /tmp/inst/bin/ressac --version'
    ```
    Expected: `ressac 0.2.0`. If it fails for a missing package, add that package to the README's `dnf install` line (`tar` is only for this copy) and rerun.
  - No macOS-only sentence left unqualified: `grep -n 'Library/' README.md` shows only lines that say "macOS" or sit in the Mac app, Sea Chart or Antiope sections.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "README: Linux section (paths, install, tmux, VM notes); ticker state per database, SIGTERM, Linux tarballs

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

### Task 2: `notes.md` and `TODO.md`

**Files:**
- Modify: `notes.md` (new section right after "## Ajouter un flux (sous-projet 7)" and its subsections, before "## Paysage existant (10/10/2026)")
- Modify: `TODO.md` (header date; "## Sous-projets suivants" first line; new "### Linux (sous-projet 8)" after "### Ajouter un flux (sous-projet 7)", before "### Durcissement proposé, non fait")

**Interfaces:**
- Consumes: the ledger rows A1–A5 and the merged code (header).
- Produces: nothing used by code.

- [ ] **Step 1: Write the `notes.md` section.** French, the house style of the file (bold lead-in, then the fact), **no em dash anywhere in it** (use a colon or a comma). The fixed part:

```markdown
## Ressac sous Linux (sous-projet 8)

Demandé par Clément le 23/10/2026 : Ressac sur marghe (Fedora KDE Plasma 44, aarch64, dans Parallels), avec sa propre base. Spec `docs/specs/2026-10-23-linux-design.md`, plan `docs/plans/2026-10-23-linux.md`, une fiche par phase dans `docs/plans/linux/`. Partie A (le terminal) faite le <date>, sur la branche `linux`.

### Décisions de la partie A (prises par Claude)

- **Chemins** : un seul module, `ressac_core::paths`, écrit à la main (la crate `dirs` ne reproduirait pas les noms macOS). macOS ne bouge pas d'un octet. Linux suit XDG : base dans `~/.local/share/ressac/`, `terminal.json` dans `~/.config/ressac/`, bandeau et journaux dans `~/.local/state/ressac/`, `~/.cache/ressac/` réservé. Une variable `XDG_*` vide ou relative est ignorée ; sans `HOME` absolu, erreur `NoHome`, jamais un chemin relatif au dossier courant.
- **Bandeau tmux, un état par base** : la base par défaut garde `ticker.json` ; toute autre base a `ticker-<16 hex>.json` dans le même dossier (FNV-1a 64 du chemin canonique, écrit à la main, stable entre `./r.sqlite`, le chemin absolu et un lien symbolique). Corrige au passage le défaut du Mac, où une seconde base partageait la rotation de la première. Le dossier d'état n'est créé que si la base existe.
- **Ancienne base `~/Library` sous Linux** : nommée dans le journal une fois par lancement (`warn`, mais `debug` pour `ressac ticker`, que tmux lance toutes les 15 s), jamais déplacée.
- **Petites corrections** : « The database is busy (is another Ressac writing?) » au lieu de nommer Ressac.app ; les liens s'ouvrent par `open::that_detached`, le terminal n'attend plus `xdg-open` ; SIGTERM vaut un premier Ctrl-C (arrêt propre, code 130) pour les commandes qui surveillent l'arrêt, le lecteur et le jardin gardent le SIGTERM par défaut (voir TODO).
- **Conteneur de développement** : Fedora 44 aarch64 sous OrbStack (`ressac-fedora`), utilisateur non root `ressac` avec l'uid du Mac, rust 1.99 de dnf, `lld` obligatoire, dossier de compilation dans un volume du conteneur et jamais `target/` du Mac. `apps/linux/dev/dev.sh check` est la porte Linux tant qu'Actions n'a pas de crédits (jusqu'au 02/11/2026). Une seule liste de paquets, `apps/linux/dev/packages.txt`, lue par le conteneur et par la CI.
- **CI** : le job Ubuntu exclut `ressac-kde` dès maintenant (`--exclude` d'un paquet absent n'est qu'un avertissement de cargo, vérifié avec cargo 1.99.1) ; un job Fedora 44 lance le script de la porte locale en utilisateur non root.
- **Release** : binaire `ressac` Linux pour x86_64 et aarch64, construit sur Ubuntu 22.04 (glibc 2.35), en `tar.gz` ; un seul `SHA256SUMS` pour le Mac et Linux ; un job de publication à part, seul à pouvoir écrire dans le dépôt.

### Trouvailles de la construction
```

Then, under "Trouvailles de la construction", one bullet per fact taken from the ledger and the A4/A5 runs, each with its number: the first `dev.sh check` (cold build time, tests passed and ignored), every Linux-only failure met in A4 and its cause, the A5 replay times (Fedora job, aarch64 release build, highest glibc symbol seen), and every deviation from the master plan recorded in the ledger for A1–A5 (for example A1's `terminal.json` moved to A2). Check each against the SIGTERM coverage and the legacy sentence in `main.rs` as merged.

- [ ] **Step 2: Write the `TODO.md` lines.** Header: "Prochaines étapes de Ressac, au <date du jour>." Under "## Sous-projets suivants", first line:

```markdown
- [ ] **8 : Ressac sous Linux.** Partie A (terminal, chemins XDG, conteneur Fedora, CI et release Linux) faite le <date> sur la branche `linux` (spec `docs/specs/2026-10-23-linux-design.md`) ; partie B (appli KDE) en cours ; trouvailles dans `notes.md`.
```

New section:

```markdown
### Linux (sous-projet 8)

- [ ] Konsole : vérifier à la main sur marghe quel protocole d'images le lecteur y trouve (aujourd'hui des demi-blocs quand aucun n'est reconnu) et décider si un correctif vaut la peine (spec §5.2).
- [ ] Lecteur et jardin du terminal : un SIGTERM venu d'ailleurs les arrête sans restaurer le terminal (mode brut, écran alternatif). Il faudrait que les deux boucles surveillent le signal et quittent proprement (`commands/mod.rs`, `commands/garden.rs`, une boucle du jardin qui n'attend plus `event::read` sans fin).
- [ ] Après le 02/11/2026 (crédits Actions) : vérifier le premier passage de `ci.yml` (job `fedora`, avertissement attendu de `--exclude ressac-kde` tant que l'appli n'existe pas) et, au premier tag, celui de `release.yml` (le runner `ubuntu-22.04-arm` est-il offert à ce dépôt privé ? sinon le job attend sans fin dans la file).
```

- [ ] **Step 3: Verify**

Run: `awk '/^## Ressac sous Linux/{p=1} /^## Paysage existant/{p=0} p' notes.md | grep -c '—'`
Expected: `0`.
Run: `awk '/^### Linux \(sous-projet 8\)/{p=1} /^### Durcissement/{p=0} p' TODO.md | grep -c '—'`
Expected: `0`.
Run: `grep -n 'Konsole\|SIGTERM\|ubuntu-22.04-arm' TODO.md`
Expected: the three new lines.
Run: `grep -n '<date' notes.md TODO.md`
Expected: no output (every placeholder filled).

- [ ] **Step 4: Commit**

```bash
git add notes.md TODO.md
git commit -m "notes.md and TODO.md: Ressac on Linux, part A

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```
