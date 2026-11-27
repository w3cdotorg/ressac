# TODO

Prochaines étapes de Ressac, au 23/10/2026. Le détail des décisions passées est dans `notes.md`.

## Mise en service

- [x] Compiler l'appli en Release ; elle tourne depuis `apps/macos/build/Ressac.app` (commandes dans le README, section « The Mac app »).
- [x] Premier lancement : déposer `~/Documents/vienna_20260415.opml` sur la fenêtre, laisser l'import et le premier rafraîchissement se faire.
- [x] Faire le tri dans le jardin, dont la fusion de « Google Blog: Research » dans « Google Blog: Innovation & AI ».
- [x] Exporter un OPML propre une fois le tri fini.
- [ ] Optionnel : lien `ressac` dans le `PATH` vers `~/Sites/ressac/apps/macos/build/Ressac.app/Contents/Helpers/ressac`.

## Sous-projets suivants

- [x] **8 : Ressac sous Linux.** Fait le 27/11/2026. Partie A (terminal, chemins XDG, conteneur Fedora, CI et release Linux) faite le 23/10/2026 sur la branche `linux` (spec `docs/specs/2026-10-23-linux-design.md`) ; partie B (appli KDE) faite le 25/11/2026, recette validée sur marghe le 26/11/2026, partie B mergée le 26/11/2026 (PR #19), releases 0.3.0 (26/11/2026) et 0.4.0 (27/11/2026, la première avec les RPM de l'appli KDE) ; bilan et trouvailles dans `notes.md`.
- [x] **7 : ajouter un flux.** Code fait le 20/10/2026 sur la branche `add-feed` (spec `docs/specs/2026-10-20-add-feed-design.md`) ; recette en attente ; trouvailles dans `notes.md`.
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

- [x] Pas de temporisation des relances automatiques après un échec hors ligne ou une panne : à chaque retour au premier plan (Cmd-Tab), l'appli relance la fabrication tant que le réseau ou la CLI est en défaut, et chaque relance force un rafraîchissement. À revoir après une semaine de vrais matins. Fait : l'appli attend 15 minutes après un échec avant une relance automatique ; Retry reste immédiat.
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

- [x] Entités HTML dans certains titres de flux : `&#39;` s'affiche tel quel dans le bandeau (« America&#39;s Earliest… ») ; décoder les entités quand les titres sont enregistrés ou affichés (existant avant la carte marine, vu à la recette de la carte le 20/10). Fait : un seul assistant (`parse::decode_title_entities`) décode une fois de plus à l'enregistrement (flux analysés et titres de flux OPML), et le schéma v6 répare une fois les titres déjà en base.
- [x] Volume du journal : chaque appel en échec écrit un avertissement (environ 5 760 lignes par jour et par client si la base manque) ; passer en `debug!` les cas attendus ou limiter la cadence. Fait : base absente, base occupée ou verrouillée, schéma différent et verrou d'état disputé passent en `debug!` ; le reste reste en `warn!`.
- [ ] Erreurs de `try_lock` autres que « déjà pris » traitées comme un verrou pris (chemin de réimpression, sans gravité).
- [ ] Titres faits seulement de caractères invisibles non blancs (U+200B) : ligne `Flux · ` apparemment vide ; caractères bidirectionnels (U+202E) et séparateurs de ligne Unicode non retirés (usurpation visuelle, pas d'injection) ; coupe par caractère et non par graphème (une séquence ZWJ ou VS16 peut déborder d'une ou deux colonnes).
- [ ] La connexion en lecture seule peut faire un point de contrôle du WAL en se fermant si elle est la dernière (pages déjà validées, aucune écriture logique) ; `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` le supprimerait.
- [ ] Deux clients tmux attachés peuvent montrer deux articles différents pendant quelques secondes de chaque tranche (leurs minuteries ne sont pas alignées sur les tranches).
- [ ] Mineurs des relectures (registre local, hors dépôt) : `path.exists()` vrai pour un dossier, pas de test du plafond de 2 000 lignes, recherches linéaires dans `shown`, quelques tests à renforcer.

### Carte marine (sous-projet 6)

- [x] Éditeur de territoires dans l'appli (aujourd'hui : `ressac chart import` seulement). Fait le 27/11/2026 sur la branche `territory-editor` (spec et plan du 27/11/2026) ; recette en attente.
- [x] Proposition de thème à l'« Add feed » : fait au sous-projet 7 (thème hérité du site, liste « Theme » facultative).
- [ ] Étiquettes courbes le long des côtes.
- [ ] Positions mémorisées pour la vue du port (elle dérive chaque jour par construction).
- [ ] Un flux qui quitte la carte, ou qui change de thème, laisse les îles suivantes de sa mer remonter d'un cran (placement îlot par îlot, par ordre d'id).
- [ ] Vue « By theme » : sur une mer pleine, l'espacement se resserre ; au-delà d'environ 230 flux, des îles peuvent se toucher.
- [ ] Page : les rafales ne sont pas remises à zéro à la reconstruction (traits périmés pendant environ 3 s) ; une carte vide montre « By theme » enfoncé ; les erreurs de dessin ne vont que dans la console ; l'infobulle ne se retourne pas au bord ; le focus n'est pas rendu à la fermeture de la fenêtre contextuelle ; la police Cormorant italique 400 n'est pas chargée dans WebKit (pied de la fenêtre contextuelle seulement).
- [ ] Appli : `showReadingWindow` prend une fenêtre quelconque ; le bloc de lancement est dupliqué entre `ChartWindow` et `RessacApp` ; aucun test de cycle de références ; « The main window says why » peut ne rien désigner.
- [ ] Appli : le `WindowGroup` principal a maintenant l'id `main` et peut oublier son cadre une fois (note de version).
- [ ] CLI : `feed theme` sur une base vide sort en code 0 avant de vérifier la clé (comme `cadence`) ; les messages d'erreur de stderr reprennent tels quels clés, noms et chemins du fichier d'import (à assainir une fois, au puits stderr de `main.rs`).
- [ ] Cœur : un territoire avec `continent` et `feeds` ignore `feeds` sans avertir ; `unthemed_feeds` compte aussi les flux enterrés ; instantané lu hors d'une transaction unique .
- [ ] Un flux présent dans deux territoires ne prend que le placement du dernier ; l'éditeur garde un seul territoire par flux ; une importation peut encore en mettre deux.
- [ ] Vue du port : le rabattement dans le cadre et la poussée hors du quai peuvent faire chevaucher des îles sur les marges.
- [ ] Animation : la chaleur et la couleur de chaque île sont recalculées à chaque image.
- [ ] Coût des rafraîchissements pendant une longue collecte : une reconstruction complète toutes les 2 s, à surveiller.
- [ ] Tests à renforcer (détail dans le registre local `.superpowers/`, hors dépôt) : renommage et réordonnancement à la réimportation, rapport des URL inconnues en essai à blanc, précédence de `xml_url`, thème vide du test de migration v4 vers v5.

### Ajouter un flux (sous-projet 7)

- [ ] Choisir parmi plusieurs flux annoncés (aujourd'hui le premier est pris).
- [x] Choisir le dossier dans la fenêtre d'ajout. Fait : `ressac add --folder` (id, nom ou `root`) et un menu Folder dans la fenêtre ; le dossier n'est pas déplié dans la barre latérale.
- [ ] Déplier le dossier du flux ajouté dans la barre latérale.
- [x] Commandes Antiope pour ajouter et déplacer un flux : hors périmètre (Clément, 27/11/2026) : Antiope reste un thème de lecture, ajouter et déplacer un flux relève de l'aspect principal.
- [x] `ressac chart export` (la base fait foi : une réimportation efface les thèmes hérités). Fait : écrit le format exact de `chart import` (sortie standard ou `--output`), héritages compris.
- [ ] Les flux venus d'une découverte gardent le client protégé à chaque rafraîchissement (aujourd'hui seul le premier passe par lui ; demande un drapeau par flux).
- [ ] Les erreurs réseau sur le lien annoncé sont rapportées comme « aucun flux ».
- [ ] Annuler pendant un ajout laisse l'ajout se finir en silence.
- [ ] Déposer un flux sur une ligne de flux d'un dossier ne fait rien (seuls les dossiers et l'en-tête « Feeds » acceptent).
- [ ] Glisser un flux dans un champ de texte y colle `ressac-feed:<id>`.
- [x] Masquage des adresses : l'adresse passée à `add` est maintenant toujours masquée dans les journaux de l'appli, quelle que soit sa forme (hôte sans point, crochets, espaces). Reste : un mot hors `add` qui ressemble à une adresse sans en être une (un e-mail) peut être masqué à tort.
- [ ] Pendant un ajout, la recherche de remplacement (`discover_replacements`) passe aussi sur les autres flux morts publics, faute de tenir compte de `only` : quelques téléchargements en plus.
- [ ] Test WebKit `the_page_draws_the_fixture_in_webkit` instable en local (polices pas toujours chargées sur une suite complète).
- [ ] Déplacer un flux entre deux sous-dossiers d'un même dossier racine en débordement peut changer sa page (le pool ne « contient » jamais une page ; contraire à l'esprit de la règle « une page déjà dans la plage est gardée »).
- [ ] `ressac add` crée la base avant de valider l'adresse (`ressac add ftp://x` sur une machine neuve).
- [ ] Pas de test d'un ajout pendant un rafraîchissement (repose sur les transactions IMMEDIATE et le busy_timeout de 5 s).
- [ ] Le `warn!(error = %e)` après un premier rafraîchissement raté n'affiche pas d'adresse aujourd'hui (`refresh_via` ne renvoie que des erreurs de base ou de diagnostic) ; à garder ainsi (journaliser le genre d'erreur, ou un commentaire qui fixe cet invariant).
- [ ] Petites dettes du cœur : la résolution d'un thème par clé ou par nom est maintenant unique (`resolve_theme_in`), mais la lecture d'une ligne de thème en `ThemeDef` reste écrite à trois endroits (`resolve_theme_in`, `themes`, relecture dans `insert_added_feed`) ; `COALESCE(html_url, xml_url)` prend un `html_url` vide pour présent (`NULLIF`) ; un territoire peut différer du thème hérité ; dérivations `Serialize` inutilisées ; ménage préalable (« Step 0 ») non fait sur `feeds.rs` ; `service.rs` à découper (`service/add.rs`) ; un déplacement sans effet calcule quand même la page.
- [ ] CLI : pas de test qu'un refus en `--json` laisse stdout vide.
- [ ] Appli : le presse-papiers est relu à chaque recréation de la fenêtre d'ajout ; un ajout qui se termine après Annuler sélectionne quand même le flux, affiche la note et peut fermer une fenêtre rouverte entre-temps ; deux notes identiques de suite ne relancent pas la minuterie.

### Linux (sous-projet 8)

- [x] Konsole : le lecteur du terminal choisit maintenant iTerm2 sous `KONSOLE_VERSION` quand rien n'est détecté (B10, essais de Clément le 25/11 : `iterm2` et `sixel` dessinent, `kitty` n'efface pas). Vérifié par Clément sur la VM le 27/11/2026, dans Konsole et sans `RESSAC_IMAGES`.
- [x] Réglages › Données : « Relaunch Now » absent à la VM 2 : comportement attendu, comme sur le Mac le bouton n'apparaît qu'après un chemin de base changé (confirmé par Clément le 25/11/2026).
- [x] Listes déroulantes de dossiers (Add Feed) : tous les dossiers au même niveau, sans retrait (fait en B10, `FolderChoice::title` renvoie le nom seul) ; deux dossiers homonymes se ressemblent, choix de Clément.
- [x] `marghe.sh` recopie maintenant `KDE_SESSION_VERSION`, `XDG_SESSION_DESKTOP` et `DESKTOP_SESSION` de la session (B10). Vérifié par Clément sur la VM le 27/11/2026 : « Open Site » du jardin ouvre le navigateur dans `manual`.
- [ ] Lecteur et jardin du terminal : un SIGTERM venu d'ailleurs les arrête sans restaurer le terminal (mode brut, écran alternatif). Il faudrait que les deux boucles surveillent le signal et quittent proprement (`commands/mod.rs`, `commands/garden.rs`, une boucle du jardin qui n'attend plus `event::read` sans fin).
- [x] CI : premier passage du job `fedora` sur la PR #19 (26/11/2026), vert après 98819aa (les tests qui ouvrent l'appli passent chacun leur tour) ; le job Ubuntu se réduit à `cargo build --locked -p ressac` sans Qt (le reste tourne dans `fedora`) et `push` ne lance la CI que sur `main` (une pull request ne tourne plus deux fois).
- [x] Au premier tag : `release.yml` vérifié sur la 0.3.0 (26/11/2026), tous les jobs verts ; le runner `ubuntu-22.04-arm` est bien offert à ce dépôt, les sommes de contrôle vérifiées, glibc 2.34 au plus.
- [x] Premier passage des jobs RPM (`linux-app`, `linux-app-verify`) de `release.yml` : fait en 0.4.0 (27/11/2026), tous les jobs verts, conteneurs `fedora:44` sur `ubuntu-22.04-arm` compris, sommes de contrôle vérifiées.
- [x] Release : le build x86_64 est rejoué (0.3.0 et 0.4.0 sur GitHub, 27/11/2026).
- [ ] Release : le plafond glibc (2.35) est affiché, pas contrôlé ; ajouter un garde-fou qui fait échouer le job de release quand le plus haut symbole `GLIBC_` dépasse 2.35.
- [ ] Recette `--screenshot` : attendre le `loadingChanged` (ou la première peinture) de la page WebEngine avant chaque capture ; sur la VM, le panneau du lecteur sort gris, vide ou périmé sur plusieurs captures (artefact de minutage, Clément ne le voit pas à la main). Le menu global de Plasma est vérifié (VM 2, 25/11) ; à la main, Clément n'a pas vu de lecteur gris au premier affichage.
- [ ] Appli KDE : l'écran CRT d'Antiope (lignes de balayage, vignette, coins du tube, bavure des couleurs ; réglage antiope.crt) n'est pas porté : le défilement de 2000 articles à l'échelle 2 (conteneur, rendu logiciel, build debug) dépasse le budget de 2 ms par image, p95 au pas de 10 articles par image de 2,48 à 5,36, de 2,60 à 5,29, de 2,67 à 4,87 et de 2,49 à 4,85 ms (+2,2 à +2,9 ms ; au pas de 1, +0,6 ms), le code de B9a seul coûtait déjà +2,0 à +2,8 ms, et sur la VM (Fedora 45, virgl) la surcouche n'apparaît qu'après un redimensionnement de la fenêtre. Pistes : un ShaderEffect avec des shaders .qsb (paquet qt6-qtshadertools) sur le rendu OpenGL de la VM, ou une bavure pré-calculée par ligne peinte (spec §7.10). Le code est dans l'historique (commits aeed578, 051e202, 51c8eb5), avec deux pistes déjà mesurées : sans les copies horizontales de la lueur (+1,2 à +1,9 ms) et un repeint à l'apparition.
- [x] Appli Mac : les dossiers du menu déroulant d'ajout d'un flux au même niveau, sans retrait, comme dans l'appli KDE depuis B10 (choix de Clément, 25/11/2026) ; fait le 27/11/2026 (`FolderChoice.title` renvoie le nom seul ; les Réglages n'ont pas de menu de dossiers).
- [x] Appli KDE : `Live.windowActive` suit maintenant l'application (`Qt.application.state === Qt.ApplicationActive`, comme `NSApp.isActive` sur le Mac) et non la fenêtre principale seule ; fait le 27/11/2026 (revue finale, M1). Test : le parcours `settings-size` réduit la fenêtre principale, Réglages ouverts (le rendu hors écran garde les deux fenêtres actives).
- [ ] Appli KDE : l'extraction de l'article complet passe par `session.worker.short` et non par `runs::submit_short` (`apps/linux/src/reader.rs`), à rebours de la règle de B4 (Runs, seul chemin de toute exécution) ; même file d'attente, mais le coordinateur ne la voit pas. Il faudrait une demande `RunRequest` pour l'extraction (revue finale, M2).
- [ ] Appli KDE : `std::thread::spawn` panique si le fil ne peut pas démarrer (`worker.rs` : voies et exécution longue ; `launcher.rs`) ; passer par `thread::Builder::spawn` avec un chemin d'erreur, comme `chart.rs`, `mosaic.rs` et `sanitize/worker.rs` (revue finale, M3).
- [ ] ressac-ui : `live::apply_rows` et `EntryList::with_page_size` ne servent qu'aux tests mais sont `pub` ; les passer en `#[cfg(test)]` ou `pub(crate)` (revue finale, M4 ; `ReaderTheme::NATIVE` est fait).
- [ ] Appli KDE : `runs::PROGRESS_INSET` est recopié en littéral dans `ReadingPage.qml` ; l'exposer par `Runs` ou retirer la constante (revue finale, M5).
- [ ] Test `smoke` : aucune fenêtre surgissante n'est capturée à l'échelle 2 (Add Feed, confirmation d'import, Réglages › Données, article en mosaïques) ; en ajouter une ou deux (revue finale, M6).
- [ ] Appli KDE, comme le Mac : un `databasePath` relatif dans `ressac-kde.conf` s'emploie tel quel, relatif au dossier courant (`launch.rs`) ; le refuser dans `database_location` ou le résoudre contre `HOME` (revue finale, M9).
- [ ] `CellRow::paint` appelle `ressac_bedstead_family()` (FFI) et alloue une `String` à chaque peinture (`cellrow.rs`) ; la lire une fois (`OnceLock`) (revue finale, M12, reste de B8b1).
- [x] Appli KDE : l'entrée de menu Sea Chart est grisée tant que la base n'est pas prête (`Launcher.phase === Launcher.Ready`, comme Add Feed) ; fait le 27/11/2026 (revue finale, M14).
- [x] Appli KDE : `visibility !== Window.Minimized` ne se déclenche jamais sous Wayland (`SeaChartWindow.qml`, `ReadingPage.qml`) : fait le 27/11/2026. La carte marine suit l'exposition de sa fenêtre (`WebRedraw.exposed`, faux dès l'arrêt du graphe de scène, vrai à l'image suivante ; test : le parcours `redraw`) ; `ReadingPage.qml` suit l'état de l'application (ci-dessous) ; le test de `Main.qml` reste, commenté, pour X11. À vérifier sur la VM (réduction réelle sous Wayland).
- [ ] Appli KDE : le lecteur blanc après le verrouillage de la session (B8e) n'a pas été retesté avec la correction de redessin de `88ce669` ; la correction n'a pas non plus été essayée en rendu logiciel.
- [ ] Appli KDE, Réglages › Lecture : en Antiope à 30 pt l'aperçu fait 3 à 4 lignes et sa distance au bas de la fenêtre (hauteur 520) n'est pas mesurée ; aucun test ne simule un glissement du curseur sur le curseur de taille (recette 26/11, `66de864`).

### Durcissement proposé, non fait

- [ ] CI : épingler les actions GitHub par SHA plutôt que par étiquette.

### Vérifié à la main par Clément (15/10/2026)

- [x] Alerte « binaire `ressac` absent » au lancement.
- [x] Une fenêtre rouverte (⌘W puis clic sur l'icône du Dock) garde la sélection.
- [x] « Show Log » et « Open Site » dans le jardin.
- [x] Clavier complet : P100 et la bannière.
- [x] Comptes du Cimetière dans les deux apparences.
- [x] Premier passage de la CI sur l'image de préversion `xcode-27` : Xcode 27.0, SDK macOS 27.0, 350 tests déclarés et exécutés (PR #6).
