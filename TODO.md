# TODO

Prochaines étapes de Ressac, au 16/10/2026. Le détail des décisions passées est dans `notes.md`.

## Mise en service

- [x] Compiler l'appli en Release ; elle tourne depuis `apps/macos/build/Ressac.app` (commandes dans le README, section « The Mac app »).
- [x] Premier lancement : déposer `~/Documents/vienna_20260415.opml` sur la fenêtre, laisser l'import et le premier rafraîchissement se faire.
- [x] Faire le tri dans le jardin, dont la fusion de « Google Blog: Research » dans « Google Blog: Innovation & AI ».
- [x] Exporter un OPML propre une fois le tri fini.
- [ ] Optionnel : lien `ressac` dans le `PATH` vers `~/Sites/ressac/apps/macos/build/Ressac.app/Contents/Helpers/ressac`.

## Sous-projets suivants

- [ ] **Ajouter un flux (bloquant).** Aujourd'hui, un flux n'entre que par l'import OPML. Il faut un « Add feed » dans l'appli et dans le terminal, qui accepte soit l'URL d'un flux (RSS, Atom, JSON Feed), soit l'adresse d'un site, avec détection du flux (`discover::alternate_feeds` lit déjà les `<link rel="alternate">` d'une page, pour les réparations du jardin) et un choix quand la page en annonce plusieurs. Demandé par Clément le 19/10/2026.
- [x] **6 : la carte marine.** Code fait le 19/10/2026 sur la branche `sea-chart` (spec `docs/specs/2026-10-19-sea-chart-design.md`) ; recette en attente (import de l'amorce dans la vraie base, remplacement de l'appli, essai des quatre encres) ; trouvailles dans `notes.md`.
- [x] **5 : le bandeau tmux.** Fait le 19/10/2026 sur la branche `tmux-ticker` ; recette et trouvailles dans `notes.md`.
- [x] **2b : la lecture dans le terminal.** Fait le 19/10/2026 sur la branche `terminal-reading` ; recette et trouvailles dans `notes.md`.
- [x] **3 : rythme et journal du matin.** Fait le 16/10/2026 sur la branche `cadence-edition` ; recette et trouvailles dans `notes.md`.
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
- [x] Images : largeur et hauteur reprises des attributs `width` et `height` ou du style en pixels (jamais des attributs `data-`, qui décrivent l'image d'origine et non celle servie) ; en Antiope, la boîte `.mosaic` réserve sa hauteur avant l'arrivée de l'image (todo-fixes, phase 6).
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

### Rythme et journal (sous-projet 3)

- [ ] Pas de temporisation des relances automatiques après un échec hors ligne ou une panne : à chaque retour au premier plan (Cmd-Tab), l'appli relance la fabrication tant que le réseau ou la CLI est en défaut, et chaque relance force un rafraîchissement. À revoir après une semaine de vrais matins.
- [ ] `ressac edition show` avec `--cap-week 0` ou `--cap-wire 0` (option d'essai) : une section sans entrée mais avec du débordement n'affiche pas ses lignes « +N more ».
- [ ] Le décompte et la liste d'Aujourd'hui (`Queries.swift`) utilisent `Calendar.current`, antérieur à cette branche : ils ne suivent un changement de fuseau qu'au relancement.
- [ ] La liste du Journal reprend le clavier à chaque apparition (comme le jardin) ; à revoir si cela gêne la saisie ailleurs.
- [ ] Antiope : lignes de débordement ouvertes au clic seulement, sans `↩` (écart assumé par rapport à la spec).
- [ ] Mineurs des relectures : la fabrication d'une édition peut reprendre l'id d'une édition reconstruite (pas d'AUTOINCREMENT, change de schéma si besoin) ; le Journal masque les entrées d'un flux enterré en cours de journée (choix assumé) ; sélecteur Statut | Rythme du jardin qui peut prendre le focus des touches `k`, `r`, `b` ; quelques tests à renforcer (voir le registre local `.superpowers/`, hors dépôt).

### Lecture dans le terminal (sous-projet 2b)

- [ ] `c` (rythme) n'est pas conditionné à l'affichage de la liste : derrière une page du jardin, il ouvre le menu pour un flux caché (une écriture exige quand même un choix explicite dans le menu). Idem Entrée sur une ligne de débordement sous le placeholder du Journal (navigation seulement).
- [ ] Mémoire des images : bornée, mais environ 330 Mo dans le pire cas ; à revoir si cela gêne.
- [ ] `r` sur la bannière occupée (fabrication ou tâche en cours) lance un rafraîchissement réseau.
- [ ] Le type de contenu d'une image n'est vérifié qu'après avoir mis le corps en mémoire (32 Mo au plus, échec transitoire).
- [ ] Avec `-v`, la trace va aussi sur stderr, par-dessus l'écran alterné ; un signal externe (SIGINT, SIGTERM) laisse le terminal en mode brut, comme le jardin.
- [ ] L'inspecteur du jardin ne défile pas sous environ 30 lignes.
- [ ] HTML au-delà de 64 niveaux d'imbrication : texte aplati, blocs collés, liens et images perdus ; lignes de `<pre>` coupées par la colonne, non repliées.
- [ ] Caractères de contrôle bidirectionnels (U+202E) : affichés tels quels (nuisance d'usurpation, pas d'injection d'échappement : ratatui écarte les caractères de contrôle). Le libellé de dossier du titre de la liste du jardin ne passe pas par `printable` (cosmétique).
- [ ] Course résiduelle à `window_end` : un autre écrivain peut dater `fetched_at` avant `window_end` mais valider après la transaction de l'édition (antérieur à cette branche).
- [ ] Antiope : mosaïques séparées ; synchroniser le look du terminal avec l'Apparence de l'appli ; une crate séparée pour le lecteur si les temps de compilation augmentent.
- [ ] À signaler en amont : `ratatui-image` 11.1.0 abandonne le protocole détecté quand aucune taille de police ne répond (sous tmux), et son constructeur avec police (`Picker::from_fontsize`) est déprécié.
- [ ] Mineurs des relectures (détail dans le registre local `.superpowers/`, hors dépôt) : un champ invalide efface tout `terminal.json`, la ligne de rythme du jardin fait sauter une rangée à l'arrivée, une entrée lue par une écriture propre reste dans Non lus jusqu'au prochain rechargement complet (comme l'appli).

### Bandeau tmux (sous-projet 5)

- [ ] Volume du journal : chaque appel en échec écrit un avertissement (environ 5 760 lignes par jour et par client si la base manque) ; passer en `debug!` les cas attendus ou limiter la cadence.
- [ ] Erreurs de `try_lock` autres que « déjà pris » traitées comme un verrou pris (chemin de réimpression, sans gravité).
- [ ] Titres faits seulement de caractères invisibles non blancs (U+200B) : ligne `Flux · ` apparemment vide ; caractères bidirectionnels (U+202E) et séparateurs de ligne Unicode non retirés (usurpation visuelle, pas d'injection) ; coupe par caractère et non par graphème (une séquence ZWJ ou VS16 peut déborder d'une ou deux colonnes).
- [ ] La connexion en lecture seule peut faire un point de contrôle du WAL en se fermant si elle est la dernière (pages déjà validées, aucune écriture logique) ; `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` le supprimerait.
- [ ] Deux clients tmux attachés peuvent montrer deux articles différents pendant quelques secondes de chaque tranche (leurs minuteries ne sont pas alignées sur les tranches).
- [ ] Mineurs des relectures (registre local, hors dépôt) : `path.exists()` vrai pour un dossier, pas de test du plafond de 2 000 lignes, recherches linéaires dans `shown`, quelques tests à renforcer.

### Carte marine (sous-projet 6)

- [ ] Éditeur de territoires dans l'appli (aujourd'hui : `ressac chart import` seulement).
- [ ] Proposition de thème à l'« Add feed » (dépend du chantier « Ajouter un flux »).
- [ ] Étiquettes courbes le long des côtes.
- [ ] Positions mémorisées pour la vue du port (elle dérive chaque jour par construction).
- [ ] Un flux qui quitte la carte, ou qui change de thème, laisse les îles suivantes de sa mer remonter d'un cran (placement îlot par îlot, par ordre d'id).
- [ ] Vue « By theme » : sur une mer pleine, l'espacement se resserre ; au-delà d'environ 230 flux, des îles peuvent se toucher.
- [ ] Page : les rafales ne sont pas remises à zéro à la reconstruction (traits périmés pendant environ 3 s) ; une carte vide montre « By theme » enfoncé ; les erreurs de dessin ne vont que dans la console ; l'infobulle ne se retourne pas au bord ; le focus n'est pas rendu à la fermeture de la fenêtre contextuelle ; la police Cormorant italique 400 n'est pas chargée dans WebKit (pied de la fenêtre contextuelle seulement).
- [ ] Appli : `hasReadingWindow` se fie au compte d'une table à références faibles (préférer `allObjects`) ; `showReadingWindow` prend une fenêtre quelconque ; le bloc de lancement est dupliqué entre `ChartWindow` et `RessacApp` ; aucun test de cycle de références ; « The main window says why » peut ne rien désigner.
- [ ] Appli : le `WindowGroup` principal a maintenant l'id `main` et peut oublier son cadre une fois (note de version).
- [ ] CLI : `feed theme` sur une base vide sort en code 0 avant de vérifier la clé (comme `cadence`) ; les messages d'erreur de stderr reprennent tels quels clés, noms et chemins du fichier d'import (à assainir une fois, au puits stderr de `main.rs`).
- [ ] Cœur : un territoire avec `continent` et `feeds` ignore `feeds` sans avertir ; `unthemed_feeds` compte aussi les flux enterrés ; instantané lu hors d'une transaction unique ; territoires regroupés par nom et non par id.
- [ ] Tests à renforcer (détail dans le registre local `.superpowers/`, hors dépôt) : renommage et réordonnancement à la réimportation, rapport des URL inconnues en essai à blanc, précédence de `xml_url`, thème vide du test de migration v4 vers v5.

### Durcissement proposé, non fait

- [ ] CI : épingler les actions GitHub par SHA plutôt que par étiquette.

### Vérifié à la main par Clément (15/10/2026)

- [x] Alerte « binaire `ressac` absent » au lancement.
- [x] Une fenêtre rouverte (⌘W puis clic sur l'icône du Dock) garde la sélection.
- [x] « Show Log » et « Open Site » dans le jardin.
- [x] Clavier complet : P100 et la bannière.
- [x] Comptes du Cimetière dans les deux apparences.
- [x] Premier passage de la CI sur l'image de préversion `xcode-27` : Xcode 27.0, SDK macOS 27.0, 350 tests déclarés et exécutés (PR #6).
