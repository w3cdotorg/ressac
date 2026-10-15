# TODO

Prochaines étapes de Ressac, au 15/10/2026. Le détail des décisions passées est dans `notes.md`.

## Mise en service

- [x] Compiler l'appli en Release ; elle tourne depuis `apps/macos/build/Ressac.app` (commandes dans le README, section « The Mac app »).
- [x] Premier lancement : déposer `~/Documents/vienna_20260415.opml` sur la fenêtre, laisser l'import et le premier rafraîchissement se faire.
- [x] Faire le tri dans le jardin, dont la fusion de « Google Blog: Research » dans « Google Blog: Innovation & AI ».
- [x] Exporter un OPML propre une fois le tri fini.
- [ ] Optionnel : lien `ressac` dans le `PATH` vers `~/Sites/ressac/apps/macos/build/Ressac.app/Contents/Helpers/ressac`.

## Sous-projets suivants

Ordre convenu le 14/10/2026 : 3, puis 2b.

- [ ] **3 : rythme et journal du matin.** Tri des flux par rythme de publication, édition du matin.
- [ ] **2b : la lecture dans le terminal.** Trois colonnes en ratatui sur le même schéma (v3) : lu, favori, recherche, article complet, avec clappuccino comme référence. Habillage Antiope du terminal compris (palette et numéros de page du sous-projet 4).
- [x] **4 : thème Antiope (appli Mac).** Fait le 15/10/2026 sur la branche `antiope` ; recette et trouvailles dans `notes.md`.

## Reporté (relectures du sous-projet 2a)

Aucun n'est bloquant ; chacun a été jugé « peut attendre » lors de la relecture finale.

### Sécurité et réseau

- [x] `discover_one` : l'adresse de flux lue dans la page du site passe par le client protégé des articles ; refusée, elle compte comme « rien trouvé » (todo-fixes, phase 1).
- [x] Client protégé (articles, autodécouverte) : `.no_proxy()`, les variables de proxy sont ignorées ; les flux gardent le proxy (todo-fixes, phase 1).
- [x] Pause par hôte des articles : chaque hôte est mémorisé, et les flux et sondes https du même rafraîchissement comptent (todo-fixes, phase 1).

### Lecture

- [x] Listes d'articles : 500 lignes à la fois, la suite au défilement ou au clavier ; `Espace` trouve le prochain non lu au-delà des lignes chargées par une requête et un changement de page ; une marque « lu » ne relit plus que la page chargée et deux comptes (todo-fixes, phases 5 et 6).
- [x] Plafond du HTML analysé porté à 512 Ko (todo-fixes, phase 6).
- [x] Nettoyage : l'estimation de profondeur suit libxml, plus de 2048 attributs sur une balise donnent du texte simple, deux fils de nettoyage au plus (un abandonné, un vivant) pour qu'un nouvel article ne soit jamais bloqué (todo-fixes, phase 6).
- [x] Images : largeur et hauteur reprises du style ou des attributs `data-` du flux ; en Antiope, la boîte `.mosaic` réserve sa hauteur avant l'arrivée de l'image (todo-fixes, phase 6).
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
- [x] CI macOS sur l'image de préversion `xcode-27`, avec une étape qui compare les tests déclarés aux tests exécutés et échoue si le compte est illisible ; droits `contents: read`, `persist-credentials: false`, délais de 30 min (`check`) et 60 min (`macos`) (todo-fixes, phase 7).
- [x] CI : `actions/checkout@v7` (Node 24) dans les deux jobs (todo-fixes, phase 7).
- [x] Indice de spam réservé aux flux non vivants, décision du 15/10/2026 (todo-fixes, phase 3a).

### Antiope (sous-projet 4)

- [ ] Défilement des gros dossiers : accrocs même sans écran CRT (38 ms/s sur la page 400), optimisation possible (voir `notes.md`).
- [x] Mineurs des relectures : un `DateFormatter` gardé par motif et fuseau, lignes de P100 et boutons de la bannière hors de la boucle du clavier complet (todo-fixes, phase 8), liens de la bannière native et de `GardenInspector` par `AppState.openURL` (phase 7), `main.rs` découpé (phase 2).

### Durcissement proposé, non fait

- [ ] CI : épingler les actions GitHub par SHA plutôt que par étiquette.

### À vérifier à la main (Clément)

- [ ] Alerte « binaire `ressac` absent » au lancement.
- [ ] Un relancement garde la sélection.
- [ ] « Show Log » et « Open Site » dans le jardin.
- [ ] Clavier complet : P100 et la bannière.
- [ ] Comptes du Cimetière dans les deux apparences.
- [ ] Premier passage de la CI sur l'image de préversion `xcode-27`.
