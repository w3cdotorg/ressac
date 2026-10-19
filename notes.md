# Notes de travail

Trouvailles, décisions et pièges rencontrés en route. Le plus récent en haut de chaque section.

## Décisions (brainstorming du 10/10/2026)

- **Nom** : Ressac (RSS caché dedans, flux et reflux). Commande : `ressac`. Thème télétexte : **Antiope**.
- **Directions retenues** : trois colonnes (base), rendu adapté au contenu (BD, vidéo, longs formats), journal du matin, tri par rythme de publication, jardin (santé des flux). Le « rédacteur en chef » IA est écarté.
- **Priorité** : le ménage des abonnements.
- **Périmètre** : un seul utilisateur, ce Mac uniquement, synchro (FreshRSS/Miniflux/Feedbin) possible plus tard. Toute la récupération passe par une seule porte dans le cœur pour pouvoir y brancher la synchro.
- **Architecture (approche C)** : cœur Rust + interface terminal ratatui dans un seul binaire ; appli SwiftUI qui lit la même base SQLite via GRDB et lance `ressac` en sous-processus pour importer/rafraîchir. Le schéma SQL versionné (+ vues stables) est le contrat. Le cœur calcule et stocke santé, rythme, éditions, type de contenu ; les interfaces affichent.
- **Langue** : toutes les interfaces en anglais (statuts : Duplicate, Hijacked, Dead, Moved, Ailing, Fossil, Dormant, Alive ; touches du jardin `k` keep, `r` repair, `b` bury). Spec et code en anglais, `notes.md` en français.
- **Découpage** : 1) cœur + ménage, 2) lecture (terminal + Mac), 3) rythme + journal, 4) thème Antiope.
- **Dépôt** : créé en **privé** (l'OPML de test contient des abonnements personnels) ; à passer en public plus tard si souhaité. Ne jamais committer l'OPML réel, seulement une version anonymisée.

## OPML Vienna (`~/Documents/vienna_20260415.opml`)

- 187 flux : 107 au premier niveau de dossier, 80 dans un sous-dossier. 4 dossiers racine (Blogs, Biblis, Infos, Jon), sous-dossiers jusqu'à 2 niveaux.
- 100 flux en `http://` seulement.
- 29 flux via Tumblr ou FeedBurner, hébergeurs à risque (FeedBurner surtout).
- **Vrai doublon** : Daring Fireball en http et https (même URL normalisée).
- **Suffixes « (1) »** : collisions de noms à la Vienna, pas des doublons (ex. Center for Humane Technology substack vs chaîne YouTube). Ne pas fusionner sur le nom.
- **Blog déménagé** : Orange Drenka existe en WordPress et en Blogspot, deux flux distincts du même auteur. Cas « déménagé » à détecter, pas un doublon.
- **Domaines détournés** repérables à la description : Bibliobsession (casino vietnamien), apps & stacks (urinoirs), Publishers Weekly (avocate brésilienne), Hipstamatic Skyrim (recettes).
- **Descriptions polluées** par du JavaScript (Google Analytics, StatCounter) récupéré sur Tumblr : nettoyer avant affichage ou comparaison.
- **URL mal échappée** : Flickr bouquiniste contient `&amp;amp;` (double échappement) : réparer à l'import.
- **Formats** : un JSON Feed (Six Colors, `feed.json`, et son `htmlUrl` pointe vers `?feed=json`), un RDF (`index.rdf`), 13 flux YouTube, des flux GitHub releases (Atom), des flux générés (raw.githubusercontent.com Olshansk).
- Sites probablement fermés : Recode, longform.org, FiveThirtyEight. À confirmer par `ressac check`.

## Premier essai réel (10/10/2026)

Base jetable dans le scratchpad (`--db …/vienna-run.sqlite`, `HOME` redirigé pour les journaux), OPML lu en lecture seule. Aucune décision de jardin prise : le tri reste à faire.

- **Import** : 186 flux dans 16 dossiers, 1 fusion (« Daring Fireball (1) » → « Daring Fireball »), conforme à l'attendu.
- **Durée** : premier `check` en 1 min 12 s (les 186 récupérations en 22 s, le reste en sondes https, autodécouverte et Wayback à 1 requête/s). Un `check` relancé dans les 30 min ne refait que le diagnostic et les Wayback manquants : 20 s.
- **Avant calibrage** : Alive 91, Fossil 63, Dormant 13, Ailing 6, Dead 6, Moved 5, Hijacked 1, Duplicate 1. 88 réparations sûres (83 https, 5 redirections).
- **Après calibrage** : Alive 92, Fossil 63, Dormant 13, Ailing 7, Dead 6, Moved 4, Hijacked 0, Duplicate 1. 87 réparations sûres (83 https, 4 redirections). Aucun `Unchecked`, aucun 429.
- **Export** : `ressac export | grep -c xmlUrl` = 186 ; `--cemetery` = 0 (rien n'est enterré).

### Verdicts notables

- **Dead** : Bibliobsession 2.0 (404 après passage en https ; score de détournement 0,4 grâce au lexique vietnamien de la description OPML), Blog de veille bm et Renaissance du cinéma (410 WordPress.com), Bom Bom (410 Canalblog), Technologies du Langage (blog.veronis.fr ne résout plus), action button dot net (301 vers innisfreehouseschool.com puis cinderellaaffair.org, qui ne résout pas : le domaine a été revendu ; motif désormais « The site it redirects to no longer resolves. », voir la reprise ci-dessous).
- **Moved (vrais positifs)** : Libération (rss.liberation.fr → outboundfeeds Arc), Schneier on Security (atom.xml → /feed/atom/), Morgane Tual (?feed=rss2 → www…/feed/), **Six Colors** : le serveur répond bien `301` de `/feed.json` vers `/?feed=json` (JSON Feed valide, `application/feed+json`), donc vrai positif.
- **Ailing** : **Unphotographable** : le serveur sert bien `index.rdf` (200, `application/rdf+xml`, 23 Ko) mais le XML est invalide (`&` nus dans les descriptions, `xmllint` : « xmlParseEntityRef: no name »), donc vrai positif ; Re/code (500 « Fastly error: unknown domain » : le site n'existe plus) ; FiveThirtyEight (301 vers abcnews.com/politics, page HTML) ; POLICEtcetera (blog Le Monde : 301 vers lemonde.fr/blog/moreas/feed/, qui renvoie une page « Client Challenge » anti-robots) ; ioudgine (Typepad fermé : 302 vers networksolutions.com puis 403) ; UnderwaterTimes.com (403 Cloudflare, y compris avec un User-Agent de navigateur) ; Slate's Culture Gabfest (12 018 024 octets, au-dessus du plafond de 10 Mo).
- **Duplicate** : Google Blog: Research (#158) : ses 20 entrées figurent toutes dans Google Blog: Innovation & AI (#156). C'est une sous-catégorie, donc un sous-ensemble : conforme à la règle, mais la fusion reste au choix de l'utilisateur.
- **Fossil** : 63 flux, dernière entrée entre 2008 et 2024. longform.org (fermé en 2024) répond toujours avec ses anciennes entrées : Fossil 2024, pas Dead. Orange Drenka (2014) et Orange-Drenka (2013) sont tous deux Fossil : pas de remarque `similar_title`, qui exige un côté Alive/Dormant.
- **Remarques** : 84 `https_available` et 16 `http_only` sur les 100 flux en `http://`. 2 `blocked` (403 : ioudgine, UnderwaterTimes.com).
- **Témoins vivants** : BBC News, Le Monde.fr, Le Monde diplomatique, xkcd, les deux flux de Simon Willison, Daring Fireball : tous Alive, sans faux positif.
- **Scores de détournement résiduels** (sous le seuil, statut inchangé) : ReadWriteWeb 0,4 (« betting », « sportsbook » : le site publie réellement du contenu de paris en ligne), cpb news et Girl on the Couch 0,4 (description en anglais, billets en français : autrices bilingues).

### Faux positifs corrigés (commit d4da65c)

1. **Lexique de spam en sous-chaîne** : « cialis » trouvé dans « socialist », « specialist », « écosocialiste », « specialising ». Score 0,4 à tort sur Libération, Pluralistic, OpenAI News, Bookride, ANIMALS TALKING IN ALL CAPS, Smoke & a Coke. Corrigé : le terme doit commencer un mot (`lexicon_matches_whole_words_only`) ; voir la reprise ci-dessous pour les pluriels et les numéros. Effet de bord utile : l'export OPML ne supprime plus les descriptions qui contiennent « specialist ».
2. **FiveThirtyEight classé Hijacked** : redirection vers un autre domaine (0,3) + page HTML (0,3) = 0,6 sans aucun indice de contenu. C'est la signature d'un site fermé par son propriétaire, pas d'un spam. Corrigé : la redirection et la page HTML ne comptent qu'en appui d'un signal de contenu (lexique ou langue) (`redirect_to_html_without_content_signal_is_not_a_hijack`, `owner_shutdown_redirect_is_ailing_not_hijacked`). Résout aussi l'écart spec/plan de la phase 5 sur `HtmlInsteadOfFeed` : sans signal de contenu, la page HTML ne pèse plus rien. Seuil `HIJACK_THRESHOLD` inchangé, poids inchangés.
3. **Actualitté classé Moved** : `301` www → domaine nu (même `url_key`), puis `302` vers `/rss-main.rss` ; la cible temporaire était proposée comme réparation « sûre ». Corrigé : seule la suite initiale de sauts permanents (301/308) peut déplacer un flux (`temporary_hop_after_a_permanent_one_is_not_a_move`, `move_target_is_the_end_of_the_permanent_hops`).

### Reprise après revue (même jour)

- **Rappel du lexique** : le mot entier strict ratait « casinos », « casino88 », « togel4d ». Désormais le terme commence un mot, accepte un « s » de pluriel, puis une fin de mot ou un numéro. « slot » seul (« time slot ») ne compte pas, seulement numéroté (« slot88 ») ; « slotted » ne compte pas. Simulation hors ligne sur les 186 flux : aucun résultat du lexique ne change.
- **Mécanisme du correctif FiveThirtyEight épinglé** : lexique + redirection = 0,7 ; langue + HTML = 0,7 ; page parquée (redirection + HTML, sans contenu) = 0,0, compromis accepté (elle reste Ailing puis Dead).
- **Motif Dead DNS** : quand l'échec DNS porte sur la cible d'une redirection vers un autre hôte, le motif dit « The site it redirects to no longer resolves. » (action button dot net). Technologies du Langage garde « The domain no longer resolves. ».
- **Dérive de la base de contrat** : un test compare le schéma de `tests/fixtures/contract.sqlite` (ouvert en lecture seule) à celui de la base vivante.
- **Spec §7** mise à jour (Moved par la suite initiale de 301/308, note de calibrage du détournement en attente de confirmation).
- Re-diagnostic par `ressac fetch` dans la fenêtre de 30 min, sans réseau (toujours 186 lignes de `fetch_log`, 1 passage) : comptes inchangés (Alive 92, Fossil 63, Dormant 13, Ailing 7, Dead 6, Moved 4, Hijacked 0, Duplicate 1).

### Réponses de l'utilisateur (10/10/2026)

- **Détournements non détectés** (apps & stacks, Publishers Weekly, HIPSTAMATIC SKYRIM) : ils restent Fossil, pas de nouveau signal.
- **Sites fermés vus comme Ailing au premier passage** : raccourci Dead quand la redirection mène hors du site et finit en HTML ou en 4xx (hors 408 et 429). Arbitrage : les 5xx, Timeout, Tls, Connect, `Challenge`, `TooLarge` et `Parse` sur l'autre site ne déclenchent pas la règle, car ils peuvent être passagers ou réparables (par exemple un flux feedburner redirigé vers le site d'un éditeur qui a un 500 pendant un jour). Re/code (500 sans redirection) reste donc Ailing. Un flux annoncé par le site du nouveau propriétaire (ABC News) n'est jamais adopté comme remplaçant.
- **Page anti-robots** (POLICEtcetera) : classée `Challenge` (nouveau `ErrorKind`), remarque `blocked`, statut Ailing.
- **Plafond de 10 Mo** : 32 Mo pour les podcasts (marqueurs iTunes, Podcasting 2.0 ou enclosure audio/vidéo dans l'en-tête).
- **XML invalide** (Unphotographable) : relecture tolérante, une seule fois, sur une copie réparée.
- **Google Blog: Research** : fusion validée, à faire par l'utilisateur dans le jardin (`r` sur la ligne Duplicate).
- **Bibliobsession 2.0** : l'indice de spam est affiché dans les preuves (« Possible spam signs (score 0.4, below the 0.6 threshold): »), sans changer le statut ni la raison.
- **Règle du détournement (§7)** : confirmée.

### Revue du calibrage (10/10/2026)

Une revue du lot de calibrage a donné six points importants et dix mineurs, tous arbitrés puis corrigés.

- **Robustesse** : assainisseur XML borné (coût linéaire), décision podcast prise une seule fois sur le premier Mo, détection des pages anti-robots limitée à de vrais marqueurs (le script `challenge-platform` seul ne compte plus) et étendue aux réponses 401/403/429/503 (en-tête `cf-mitigated` ou corps). La découverte automatique traite une page anti-robots comme « non vérifiable ». Les entités HTML nommées sont converties en vrais caractères, y compris dans les titres, et les références numériques interdites sont supprimées. Pas de relecture tolérante sur un JSON Feed.
- **Même site** : `site_of` ne garde trois étiquettes que pour un suffixe connu (`co.uk`, `com.au`, `gouv.fr`…), et `www.` ou un sous-domaine du même hôte compte comme le même site (`www.rfi.fr` et `rfi.fr`).
- **403 hors site** : un 401/403 ou une page anti-robots ne déclenche la règle Dead que si l'adresse finale ne ressemble pas à un flux (un pare-feu devant un éditeur vivant reste Ailing, `blocked`). La règle ne se déclenche pas non plus sur un mur de consentement ou de connexion.
- **Arbitrage I6** : on garde Dead + Bury quand un flux est redirigé hors du site, car on ne peut pas distinguer le même propriétaire d'un nouveau. Coût : un blog qui a déménagé sur un nouveau domaine est proposé à l'enterrement, avec la nouvelle adresse affichée dans les preuves (« The new site announces … ») pour une adoption à la main.
- **Remarque `blocked`** : conservée, c'est un fait. Elle peut donc accompagner un statut Dead (ioudgine). La spec §7 le dit.
- **Indice de spam (M8)** : aucun changement de comportement, l'utilisateur voulait l'indice. Sur le run réel, 5 flux l'affichent : Bibliobsession (le cas visé), ReadWriteWeb (Alive, vocabulaire de paris dans un site tech) et trois flux Fossil avec une langue incohérente. On pourra le restreindre plus tard (statuts non Alive, ou termes du lexique seulement).
- **Identifiants d'articles (R3)** : les entrées sans GUID dont le titre contenait une entité non déclarée changent d'identifiant une fois après la mise à jour (le titre est désormais haché avec le vrai caractère), donc quelques doublons d'articles apparaissent dans une base existante au premier relevé. C'est ponctuel, et la vraie base de Clément n'existe pas encore.

## Appli Mac (sous-projet 2a)

### Recette manuelle (10 et 11/10/2026)

Base : construite depuis l'OPML Vienna (import puis check) dans un dossier temporaire, ouverte par `RESSAC_DB` (appli) et `--db` (terminal). La vraie base n'existe pas encore : rien n'a été créé dans `~/Library/Application Support/Ressac/`. Appli : build Release, `ressac` embarqué. Un petit script `recette.sh` (`app`, `garden`, `first-run`) a remplacé les variables du protocole, trop pénibles à recopier.

- Point de départ : schéma 2, 186 flux actifs, 10 224 entrées dont 1 083 non lues (règle des 7 jours), 80 flux à trier.

| # | Vérification (spec §9) | Résultat | Observations |
|---|---|---|---|
| 1 | Lancement sur la vraie base | OK | dossiers, compteurs, pastilles et jardin affichés d'emblée |
| 2 | Lecture au clavier seul | KO puis OK | `j`/`k`/Espace gardés par la vue qui avait le focus (sélection par frappe de la barre latérale, liste, WebKit) ; corrigé par un routeur de touches au niveau de la fenêtre, flèches à la NetNewsWire, liste qui suit la sélection |
| 3 | Article complet d'un flux tronqué | KO puis OK | le bouton existait mais s'affichait en petite icône sans titre ; désormais bouton titré, éteint après un échec avec la raison affichée |
| 4 | Tri de trois flux, une annulation | OK | |
| 5 | Mêmes décisions dans `ressac garden` | OK | et une décision prise dans le terminal remonte dans l'appli ouverte (veille sur `data_version`) |
| 6 | Clair et sombre, polices, tailles | OK | |
| 7 | Export OPML | OK | |
| 8 | Premier lancement (dépôt d'OPML) | KO puis OK | « SQLite error 14 » : ouverture en lecture seule d'une base WAL sans fichier `-shm` ; les ouvertures de vérification passent en lecture-écriture |

- **Icône** : vague dont la crête porte `<rss><item><title>` (piste B1 retenue par Clément) ; source `apps/macos/Ressac/Resources/AppIcon.svg`. Après une recompilation au même chemin, le Dock gardait l'ancienne icône : `touch` du paquet puis `lsregister -f`.
- **Incident** : pendant le débogage du clavier, un sous-agent a envoyé 4 ou 5 frappes `j` par System Events à l'application au premier plan (vers 10 h 00, le 11/10). Règle désormais : aucune frappe simulée vers une appli que Clément utilise, seulement des tests en processus.
- **À surveiller** : l'avertissement « reentrant operation in its NSTableView delegate » vient en partie du défilement de la liste vers la sélection (corrigé) ; une autre source possible n'est pas reproduite.

### CI macOS

- `xcode-27` (15/10/2026) : le job macOS passe sur cette image de préversion (macOS 27, Xcode 27.0) pour construire et tester l'appli à chaque envoi. Une étape compare les tests déclarés dans `RessacTests` aux tests exécutés et échoue si le compte est illisible. Le workflow a `permissions: contents: read` et `persist-credentials: false`, avec des délais de 30 minutes (`check`) et 60 minutes (`macos`). Si l'image pose problème, revenir à `runs-on: macos-latest`, une seule ligne de `ci.yml`.
- **Releases** (19/10/2026) : `release.yml` se déclenche sur les étiquettes `v*.*.*`, refuse une étiquette qui ne correspond pas aux versions de `Cargo.toml` et de `project.yml`, construit l'appli Release (arm64) sur `xcode-27` sans repli possible, puis publie l'appli zippée par `ditto`, le binaire `ressac` et `SHA256SUMS` avec `gh release create --generate-notes`. Signature ad hoc, pas de notarisation (choix de Clément) : les notes de version expliquent `xattr -dr com.apple.quarantine`. Empaquetage essayé en local sur le build du 19/10 (signature intacte après décompression, sommes OK). Le job ne relance pas les tests : `ci.yml` tourne aussi sur l'envoi de l'étiquette.
- `macos-latest` (11/10/2026) : Xcode 26.6 et SDK macOS 26.5, donc la construction et les tests de l'appli sont sautés, échec autorisé, job vert avec un avertissement (exécution 38126512109). L'appli n'est vérifiée qu'en local tant que GitHub n'a pas Xcode 27.

## Thème Antiope (sous-projet 4)

### Recette manuelle (15/10/2026)

Base : la vraie base (`~/Library/Application Support/Ressac/ressac.sqlite`), migrée de la v2 à la v3 par l'appli à son premier lancement, sauvegarde `ressac.sqlite.bak-v2` à côté (version 2, 61,6 Mo). Appli : build Release 8a5bf4a (puis les correctifs D à F, voir plus bas) à la place de l'ancien `/Applications/Ressac.app`. Recette faite par Clément ; Claude n'a envoyé aucune frappe ni aucun clic, il a seulement lu des copies de la base avec un petit `recette.sh` jetable et tenu le journal. Aucun test n'a écrit dans la vraie base. Migration essayée d'abord sur une copie (0,19 s, mêmes numéros de page que sur la vraie base).

- Point de départ : schéma 2 puis 3, 16 dossiers, 155 flux actifs et 31 enterrés, 202 pages attribuées (de 200 à 548), dossiers racine en 200 (Blogs), 300 (Biblis), 400 (Infos) et 500 (Jon), 1079 entrées non lues, 0 flux à trier. Aucune page sous 200 : pas de flux hors dossier. Tous les contrôles d'intégrité à 1.

| # | Vérification (spec §11) | Résultat | Observations |
|---|---|---|---|
| 1 | Passage en Antiope et retour sans relancer | OK | |
| 2 | Page 415 (Courrier international) puis une page inconnue (399) | OK | NOT FOUND, CORRECTION et ANNULATION vérifiés |
| 3 | Lecture avec SUITE / RETOUR seulement | OK | SUITE depuis P100 ou P899 ramène l'article |
| 4 | Mosaïque vers l'original et retour avec `i` | KO puis OK | l'article proposé (page 462, Slashdot) n'avait que des icônes de partage dans des liens : refait sur la page 206 (Dinosaur Comics), OK |
| 5 | Tri d'un flux en P800 puis annulation | OK | |
| 6 | Séparateurs droits avec émojis ou CJK | OK | pages 209 (CJK) et 527 (émoji) ; aucun accent combiné dans la base |
| 7 | Tailles 15 à 30 et largeur 40 colonnes | OK | |
| 8 | VoiceOver sur la colonne d'index | OK | |
| 9 | Écran CRT : clics, défilement, lignes nettes, case grisée en Natif | OK | contraste augmenté vérifié |

Le seul KO (4) venait du choix de l'article de test, pas du code. Trois remarques de Clément en cours de recette ont été traitées sur place, puis revérifiées par lui :

- **D** (8f8d926, 8e6f585, 45094e2) : pas de repère de la colonne active, donc titres de colonne en vidéo inverse ; une bascule tous / non lus, donc « Show Unread Only » (`⌥⌘U`, menu View, pour les deux habillages) et un titre `ENTRIES  ALL/UNREAD` cliquable ; l'horloge en haut à droite était coupée par le coin du CRT, donc une marge droite.
- **E** (bfd0cc2) : plus de titre READER en double sur P100 et P899, et un clic sur le titre ENTRIES rend la colonne active. Le doublon du jardin est gardé, à la demande de Clément.
- **F** (730bad9) : avec « Afficher les barres de défilement : toujours » (souris), les colonnes Antiope réservaient la piste d'un défileur classique et coupaient la fin des lignes (1028 devenait 102). Correctif : `.scrollIndicators(.never)`, et la barre du lecteur est masquée en Antiope.

### Trouvailles

- **Fluidité de l'écran CRT (phase 13)** : enregistrement Animation Hitches de 20 s à la souris sur la page 400 (Infos, 6002 entrées) : 59 accrocs sur 525 images, soit 53,3 ms/s avec l'écran CRT ; 32 sur 508, soit 38,3 ms/s sans (seuil retenu : 5 ms/s). Les deux dépassent le seuil : la bavure n'est pas la cause, et la bavure complète est gardée comme le plan le prévoit. Clément ne voit pas de différence à l'œil. Les premières mesures, où la case avait été basculée pendant l'enregistrement, sont écartées.
- **Défilement des gros dossiers** : il accroche aussi sans CRT (38 ms/s). Optimisation possible plus tard, pas bloquante.
- **Contraste augmenté** : vérifié en recette (point 9).
- **`sqlite3 -readonly` et les copies `.backup`** : une copie faite par `.backup` d'une base WAL garde l'indicateur WAL sans fichiers `-wal` ni `-shm`, et l'ouverture en lecture seule échoue. Le `recette.sh` utilise `file:…?immutable=1`. Sur la vraie base, `.backup` n'a changé que la date du fichier `-shm`.
- **Numéros de page** : 202 pages de 200 à 548, racines en 200, 300, 400 et 500, aucune collision ; l'attribution est déterministe, l'essai sur copie a donné les mêmes numéros.
- **Mineurs reportés** : les remarques mineures des relectures restent ouvertes (caches de `PageDirectory` et de `DateFormatter`, focus des boutons au clavier complet, `GardenInspector` et la bannière du journal qui appellent `NSWorkspace` en direct, quelques tests à renforcer, `main.rs` à découper avant le sous-projet 2b). Le détail est dans le registre local `.superpowers/`, hors dépôt.

## Corrections du TODO (todo-fixes, 15/10/2026)

Plan en neuf phases (`docs/plans/todo-fixes/`), rejoué dans l'ordre sur une copie du dépôt avant l'exécution. Décisions de Clément du 15/10/2026 :

- **Tailscale** : la plage CGNAT 100.64/10 reste refusée pour l'article complet, par choix. Rien à changer.
- **Indice de spam** : plus jamais sur un flux vivant (ReadWriteWeb et quatre autres l'affichaient pour du bruit). Le score reste stocké.
- **Refus du jardin** : `keep`, `repair` ou `undo` vers une adresse déjà prise sortent en code 2. L'appli y voit un refus et non un bogue de son appel : bandeau avec la phrase de la CLI, sans bouton du journal, et journal au niveau erreur. Les autres sorties en code 2 restent des fautes.
- **Cimetière** : il compte les non-lus des flux enterrés directement sur `entry`, sans changer le schéma (toujours v3).
- **CI** : job macOS sur l'image de préversion `xcode-27`, voir « CI macOS ».

Le reste : garde anti-SSRF de l'autodécouverte, client protégé sans proxy, pause par hôte sur tout le rafraîchissement, `main.rs` découpé, auteur des articles suivi, triage recalculé une fois, listes d'articles par pages de 500, nettoyage HTML borné et plafond à 512 Ko, tailles d'images reprises du flux, lancement unique, alerte quand `ressac` manque, `DateFormatter` gardés, boutons Antiope hors du clavier complet. Restent ouverts : l'avertissement NSTableView (n'agir qu'avec une trace) et les accrocs de défilement des gros dossiers en Antiope (reportés).

Correctifs faits pendant l'exécution, après l'écriture du plan :

- **Phase 5** : `Espace` trouve le prochain non lu au-delà des lignes chargées avec une seule requête (`Queries.nextUnread`) et un seul changement de page.
- **Phase 6** : la hauteur réservée de la mosaïque est posée sur la boîte `.mosaic`, pas sur l'image ; la borne d'attributs est 2048 et non 256 ; la porte des fils d'analyse admet 2 fils (un abandonné, un vivant), donc un nouvel article n'est jamais bloqué par une analyse abandonnée.
- **Phase 7** : l'étape de comptage des tests de la CI échoue quand le compte est illisible ; le workflow a `permissions: contents: read` et `persist-credentials: false` ; délais de 30 minutes pour `check` et 60 minutes pour `macos`.

Comptes à la fin : `ressac` 35 tests unitaires et 19 tests de CLI, cœur 260 (et 1 ignoré), puis 1 (`article_proxy`), 9 (contrat) et 4 (`end_to_end`) dans ses tests d'intégration ; appli 349 tests dans 84 suites, sans avertissement.

À vérifier à la main par Clément : l'alerte quand `ressac` manque, une fenêtre rouverte (⌘W puis Dock) qui garde la sélection, « Show Log » et « Open Site », le clavier complet sur P100 et la bannière, les comptes du Cimetière dans les deux apparences, et le premier passage de la CI sur l'image `xcode-27`.

Durcissement proposé, non fait : épingler les actions GitHub par SHA.

## Rythme et journal du matin (sous-projet 3)

### Recette manuelle (16/10/2026)

Base : la vraie base, migrée de la v3 à la v4 par l'appli à son premier lancement, sauvegarde `ressac.sqlite.bak-v3` à côté. Appli : build Release à la place de l'ancien `apps/macos/build/Ressac.app` (gardé à part jusqu'à la fin de la recette) ; trois builds se sont succédé pendant la recette (04fbb43, puis 1eb214e, puis f1b1456 pour la recette finale, voir plus bas). Recette faite par Clément ; Claude n'a envoyé aucune frappe ni aucun clic, il a seulement lu des copies de la base avec un petit `recette.sh` jetable et tenu le journal. L'essai hors ligne (item 6) a tourné sur des copies dont l'édition du jour avait été retirée. Migration essayée d'abord sur une copie (0,149 s), puis une édition d'essai fabriquée sur une seconde copie (72 entrées).

- Point de départ : schéma 3 puis 4, 156 flux actifs (30 enterrés), groupes 57 Daily, 28 Weekly, 17 Monthly, 4 Rare, 50 Silent (mêmes comptes que la mesure du spec), 1024 entrées non lues. Première édition sur la vraie base : 73 entrées (une : 0, semaine : 19, fil : 54), 624 laissées dans leurs flux, 0 flux injoignable ; l'édition du jour est numérotée 1, construite à 16:23.

| # | Vérification (spec §10) | Résultat | Observations |
|---|---|---|---|
| 1 | Migration v4, d'abord sur une copie | OK | 0,149 s sur la copie ; sauvegarde `bak-v3` (version 3) présente ; numéros 104 à 109 libres, aucune page déplacée ; les six contrôles d'intégrité à 1 |
| 2 | Première ouverture du jour : progression puis Journal | OK | « rapide » selon Clément ; une seule édition, fenêtre de 24 h ; la pastille de progression coupait son texte (voir A) |
| 3 | Lecture jusqu'à « That's the edition. », une ligne de débordement | KO puis OK | sur le Journal, `j`/`k` et les flèches étaient inopérants et rien n'était sélectionné tant qu'on ne cliquait pas dans un article (voir B) ; refait sur le build 1eb214e : clavier, sélection bleue et « +N more » OK |
| 4 | Forçage du rythme dans l'inspecteur, puis Auto | OK | Anthropic Research forcé Daily puis rendu à Auto |
| 5 | Deuxième lancement le même jour | OK | toujours une seule édition |
| 6 | Hors ligne puis Retry (copie) | KO puis OK | première copie mal préparée (voir C) ; refait sur le build 1eb214e : copie hors ligne, Offline puis Retry toujours hors ligne et 0 édition ; copie en ligne, édition 1 construite (67 entrées, 0 injoignable) |
| 7 | Antiope : 104, sous-pages, FIN DE L'EDITION, 105 à 109, SOMMAIRE | OK | P104, sous-pages, `FIN DE L'EDITION`, P105 à P109 et SOMMAIRE |

Verdict final : 7 vérifications sur 7 OK sur le build f1b1456. Les deux KO (3 et 6) et la remarque sur la pastille ont été traités avec Clément :

- **A, pastille de progression** (e8bb52f, f1b1456) : le texte « Building this morning's edition… 152/156 » était rogné à gauche. Décision de Clément : le raccourcir pendant la fabrication si la pastille est trop étroite. `ViewThatFits` choisit le texte compact « Edition… n/N » (le texte complet, mesuré à 249,7 pt à 13 pt, reste dans l'infobulle). Le texte restait rogné, aussi pour « Fetching feeds » : défaut antérieur à la branche, le contenu touchait les bords arrondis de la capsule. Correctif : une marge intérieure. Vérifié par Clément sur le build f1b1456.
- **B, clavier du Journal** (1eb214e) : en passant sur le Journal, la table de la liste est remplacée et le focus tombe sur la fenêtre, ce qui explique les flèches mais pas `j`/`k` ; la cause exacte n'a pas été reproduite hors de l'appli. Correctif : la liste du Journal prend le clavier quand elle apparaît (comme le jardin) et les lignes « +N more » ne sont plus focalisables. Vérifié par Clément.
- **C, essai hors ligne** : voir la trouvaille sur la préparation de la copie ci-dessous. Ce n'était pas un défaut de l'appli.

### Trouvailles

- **Taille de l'édition** : 73 entrées le premier jour (estimation de la spec : 40 à 45), dont 54 pour le fil, un article pour chacun des 54 flux Daily : le nombre de flux Daily actifs fixe la taille de cette section. L'édition couvre plus qu'un jour : la copie d'essai datait du 14/10, donc deux jours d'articles neufs sont tombés dans la première fenêtre de 24 h (garde de 7 jours). Plafonds gardés à 3 et 1, rien d'ajusté sur cette première édition ; la section « une » est vide (2 non lus seulement dans Monthly et Rare avant le rafraîchissement). Le plus gros débordement : Actualitté 79, The Globe and Mail 58, Libération 49, 624 entrées cachées au total.
- **Groupes de rythme sur la vraie base** : 57 Daily, 28 Weekly, 17 Monthly, 4 Rare, 50 Silent sur 156 flux actifs. Rien d'étrange dans le classement (les quatre Rare : Manuscript Road Trip, SUBNORMALITY!, Long-Forgotten, The Politics of Systems). Le forçage d'un flux (Anthropic Research) puis son retour à Auto fonctionnent.
- **Durée de la première fabrication** : 23,2 s sur la copie d'essai avec le réseau (rafraîchissement compris) ; sur la vraie base, « rapide » selon Clément, non chronométrée.
- **`entries_per_month` corrigé** : la nouvelle règle (entrées des 365 derniers jours divisées par les mois qu'elles couvrent) remplace la division par 12 ; non rejouée au cas par cas pendant la recette, le cas visé par le spec est The Globe and Mail (8,3 avant).
- **Écarts assumés par rapport à la spec** : lignes de débordement ouvertes au clic seulement en Antiope (pas de `↩`), titres de flux en capitales accentuées (`ACTUALITTÉ`).
- **Un nouvel essai ne réutilise pas un identifiant d'édition** : sans AUTOINCREMENT, une reconstruction peut reprendre l'id 1 ; l'appli suit l'édition la plus récente et compare aussi `builtAt`, `--rebuild` reste un outil d'essai.
- **Retry hors ligne force le rafraîchissement** : `build_edition` rafraîchit avec `force = true`, sinon un Retry dans les 30 minutes d'une fabrication hors ligne ne trouvait aucun flux échu, ne voyait pas la panne réseau et une édition vide consommait la journée (spec §9 : hors ligne, pas d'édition et fenêtre inchangée). Coût : les flux relevés depuis moins de 30 minutes sont redemandés (GET conditionnel, surtout des 304).
- **Caractères de contrôle neutralisés dans la CLI** : les titres de flux et d'articles imprimés en texte (`edition show`, résumé de `edition build`, `feed cadence`, `page list`, barre de progression) passent par un `printable` qui remplace les caractères de contrôle ; le JSON, échappé par serde, n'est pas concerné. Les C0 sont déjà écartés par le XML, mais pas les C1 (U+009B, CSI) ni les JSON Feed.
- **Flux enterré après la fabrication** : le cœur et la CLI gardent l'édition figée, mais l'appli ne montre que les flux actifs (`v_edition_today` et `v_edition_overflow` filtrées dans `Queries.edition()`, pastille de non-lus calculée sur `v_entry`), pour rester d'accord avec la liste et avec ce que le lecteur peut ouvrir. Une entrée non lue d'un flux enterré en cours de journée disparaît donc du Journal.
- **Préparer la copie de l'essai hors ligne** : supprimer les éditions d'une copie avec `PRAGMA foreign_keys=ON`, sinon `edition_entry` garde 73 lignes orphelines et le Retry ne peut pas écrire (conflit de clé primaire sur l'id 1 réutilisé) : première tentative de l'item 6, artefact de préparation.
- **Simuler la panne réseau** : donner à chaque flux actif un hôte distinct en `.invalid` (`http://feedN.invalid/rss`) ; la CLI répond « The network is unreachable… » en 2 s, code 2. Avec un seul hôte sur un port fermé, la copie a mis 2 min 36 à échouer à cause de la pause par hôte.
- **`sqlite3 -readonly` et les copies WAL** : comme en sous-projet 4, l'ouverture en lecture seule d'une copie `.backup` sans `-shm` ni `-wal` échoue ; le drapeau est retiré pour les copies et gardé pour la lecture de la vraie base. Dans le script, une apostrophe dans le message d'un `${VAR:?…}` casse l'analyse de zsh.
- **Mineurs reportés** : voir `TODO.md`, section « Rythme et journal (sous-projet 3) ». Le détail des relectures est dans le registre local `.superpowers/`, hors dépôt.

## Lecture dans le terminal (sous-projet 2b)

### Recette manuelle (19/10/2026)

Base : la vraie base (schéma 4). Lecteur : `ressac` en Release, lancé par Clément dans un volet tmux de Ghostty (tmux 3.8, `allow-passthrough on`) ; Claude n'a envoyé aucune frappe, il a lu des copies de la base avec un `recette.sh` jetable et tenu le journal.

- Point de départ : 1785 non lus, 0 favori, une édition (celle du 16/10, 16:23), Journal lu 18/73.

| # | Vérification (spec §10) | Résultat | Observations |
|---|---|---|---|
| 1 | Ouverture sur le Journal, fabrication si besoin | OK | édition 2 fabriquée au lancement (15:05:02) |
| 2 | Lecture au clavier, liens `o` + numéro | OK | Journal lu 66/66, les entrées lues se retrouvent dans la liste des récentes |
| 3 | Images dans le volet, `i` | KO puis OK | demi-blocs au premier essai ; vraies images en Kitty après correction (2aa435f), protocole détecté : Halfblocks, choisi : Kitty |
| 4 | Volet étroit puis élargi | OK | remarque sur la colonne active (voir plus bas), corrigée (95d5f05) |
| 5 | Antiope : 104, sous-pages, FIN DE L'EDITION, 105, 800, 100, 899 | OK | |
| 6 | Appli et terminal ouverts ensemble | OK | lecture croisée appli et terminal |
| 7 | Jardin : garder puis annuler | OK | `k` puis `u`, aucune décision nouvelle en vigueur |

Verdict : 7 vérifications sur 7 OK. Deux remarques de Clément pendant la recette, corrigées sur son go puis revérifiées par lui sur le build ba88e5f (colonne active nette, vraies images dans Ghostty sous tmux).

### Trouvailles

- **Colonne active peu visible** (95d5f05) : à gauche et à droite on ne voyait pas quelle colonne avait le focus. Le défaut concerne le terminal seulement, l'appli macOS la montre bien (rectificatif de Clément). En Native, la colonne active a une bordure double accent et un titre en vidéo inverse, les autres sont atténuées ; en Antiope, la barre de sélection des colonnes inactives passe en blanc sur bleu.
- **Images en demi-blocs sous tmux** (2aa435f) : `ratatui-image` 11.1.0 abandonne le protocole détecté quand aucune taille de police ne répond (ce qui arrive sous tmux). Cause trouvée grâce à la ligne de journal « image protocol » ajoutée pour l'occasion : `detected=Halfblocks`, `client_term=xterm-ghostty ghostty 1.3.1`, `chosen=Kitty`. Correction : sous tmux, repli sur `client_termname` (ghostty ou kitty donne Kitty, iterm ou wezterm donne iTerm2), interrogé avec `tmux display-message -t $TMUX_PANE`, seulement si la bibliothèque a rendu Halfblocks et que tmux est détecté. `RESSAC_IMAGES=kitty|iterm2|sixel|halfblocks|off` force le choix. Piège : sans cette ligne de journal, la cause restait invisible.
- **Horloge de `build_edition`** (ba88e5f, 0c97389, fac2305) : un test dépendant de la date a explosé le 19/10 (`the_edition_s_day_is_the_injected_clock_s_not_the_machine_s`, jour injecté du 17/10). Cause dans le code de production : `window_end` et `built_at` valaient `Utc::now().max(now)`, donc une horloge injectée dans le passé produisait une édition datée de la machine. Désormais `built_at` (règle « une édition par matin ») est l'horloge de l'appelant plus la durée de l'appel, et `window_end` est le maximum de `built_at` et du dernier `fetched_at` écrit par ce rafraîchissement, de sorte que les articles que la fabrication vient de relever tombent toujours dans sa fenêtre. Deux tests : un build de forme production et un jour passé injecté dont le rafraîchissement date la machine (il échoue avec l'ancienne règle).
- **Arbitrages du chantier** : une revue de sécurité (déni de service) a imposé un plafond de profondeur sur le HTML (au-delà de 64 niveaux, le sous-arbre est aplati sans récursion) et une largeur minimale pour l'habillage des listes et citations imbriquées. Les caractères de contrôle passent par `printable` dans la CLI ; dans le lecteur, `set_stringn` de ratatui écarte déjà tout graphème contenant un caractère de contrôle (C0 et C1, vérifié dans `ratatui-core` 0.1.1), ce n'est donc pas une injection, seulement de la défense en profondeur. Les chemins de restauration du terminal (sortie normale, erreur, panique) ont été corrigés à la relecture de la phase 5b. La mémoire des images est bornée en taille, et les échecs transitoires (réseau) se distinguent des permanents (refus, taille).
- **Reportés** : voir `TODO.md`, section « Lecture dans le terminal (sous-projet 2b) ». Le détail des relectures est dans le registre local `.superpowers/`, hors dépôt.

## Bandeau tmux (sous-projet 5)

Idée 13 du brainstorming du 19/10 sur d'autres présentations des flux (maquettes : artefact claude.ai « Ressac au large »). But : la sérendipité, voir passer de temps en temps un article laissé de côté. Spec `docs/specs/2026-10-19-tmux-ticker-design.md`, plan `docs/plans/2026-10-19-tmux-ticker.md`, exécution en sous-agents (une relecture par tâche, relecture finale Opus).

- **Clic** : un lien OSC 8 (`#[link=…]`) dans la barre ne s'ouvre pas avec `mouse on`, essayé le 19/10 dans un serveur tmux isolé ; une zone `#[range=user|…]` et une liaison `MouseDown1Status` fonctionnent d'un simple clic.
- **Injection** (relecture finale) : la première version de la liaison acceptait toute zone commençant par `rs` et passait l'argument sans guillemets à `run-shell`, donc à `sh`. Un titre de volet (`#{pane_title}`, modifiable par n'importe quel programme d'un volet) pouvait porter une zone `rs;commande`. Corrigé avant toute installation : expression régulière `^rs[0-9]+$`, `#{q:…}`, et titre du volet échappé par `q/h`. Vérifié sur un serveur tmux isolé.
- **Lenteur** : `NOT EXISTS` sur `edition_entry` parcourait la table pour chaque article (355 à 404 ms sur un an d'éditions simulé) ; `NOT IN` ramène à 6 ms, et un test vérifie le plan de requête.
- **Silence** : `run-shell` affiche dans le volet actif toute sortie et tout code non nul, d'où la règle « rien sur stdout ni stderr, code 0 » pour les deux commandes ; `init_logging` paniquait si `~/Library/Logs/Ressac` n'était pas inscriptible (code 101), corrigé.
- **WAL** : un écrivain ne bloque jamais un lecteur en mode WAL, donc les tests de « base occupée » prévus au plan ne pouvaient pas échouer ; l'ouverture en lecture seule n'utilise pas `SQLITE_OPEN_READ_ONLY` (erreur 14 sans fichier `-shm`) mais `query_only` et une attente de 250 ms, sans jamais toucher au mode de journal.

### Recette (19/10/2026)

Base : copie de la vraie base par `.backup` en lecture seule, HOME temporaire pour ne jamais toucher au `ticker.json` réel. Constat de départ : il ne restait qu'un non-lu dans toute la base, 1 615 articles récents ayant été marqués lus d'un coup à 17:15 ; sur la copie seulement, ces articles ont été remis en non lus.

| # | Vérification (spec §9) | Résultat | Observations |
|---|---|---|---|
| 1 | Vingt appels espacés de 15 s sur la copie | OK | 20 articles distincts de 19 flux, jamais deux fois le même flux de suite, tous non lus, récents et hors édition ; 10 à 40 ms par appel (0,5 s pour le tout premier, à froid) |
| 2 | Lignes ajoutées à `~/.tmux.conf` | Fait par Clément | Appli locale recompilée depuis la branche (ancienne gardée dans le scratchpad de la session) |

## Paysage existant (10/10/2026)

- macOS : NetNewsWire (Swift, MIT, la référence), quick-rss (SwiftUI), Petal Feed (accessibilité, juillet 2026, pas open source a priori).
- Terminal : Newsboat (le plus complet), NewsGoat (Go, bubbletea), rsrssr (Rust, ratatui, images en ligne), Canto, sfeed.
- Presque tous suivent le modèle trois colonnes + compteur de non-lus.

## Outils disponibles sur la machine

- Swift 6.4, Xcode, cargo, go, python3. `gh` connecté au compte w3cdotorg.
- Noms vérifiés le 10/10 : `ressac` libre sur crates.io et Homebrew.

## Pièges d'outillage

- `git push` échoue (« could not read Username ») : aucun credential helper global pour GitHub. Pousser avec `git -c credential.helper= -c credential.helper='!gh auth git-credential' push`, sans modifier la config git globale.

## API des crates (essai du 10/10/2026, versions épinglées)

- **feed-rs 3.0.0** : décode l'ISO-8859-1 d'après la déclaration XML (« Café crème » OK). Une page HTML donne `ParseError(NoFeedRoot)`. Les entrées sans GUID reçoivent un id stable (hash du premier lien + titre) : on l'utilise tel quel comme identité d'article. RDF reconnu (`FeedType::RSS1`).
- **quick-xml 0.42.0** : API passée aux chaînes. `QName(pub &str)`, comparer `e.name().0 == "outline"`. `unescape_value()` n'existe plus quand la feature `encoding` est active (feed-rs l'active) : utiliser `a.normalized_value(quick_xml::XmlVersion::Implicit1_0)`. Les retours à la ligne des attributs deviennent des espaces (normalisation XML). Le `&amp;amp;` de Flickr ressort en `&amp;` après décodage : la réparation se fait ensuite.
- **whatlang 0.18.0** : codes ISO 639-3 (`vie`, `fra`, `eng`), pas 639-1.
- **reqwest 0.13.5** : feature `rustls` (pas `rustls-tls`) ; `redirect::Policy::none()`, `.timeout()`, `.user_agent()` inchangés. Pas de feature `json` : désérialiser avec `serde_json::from_slice(&resp.bytes().await?)`.
- **ratatui 0.30.2** : `ratatui::init()` / `ratatui::restore()`, `DefaultTerminal`, crossterm réexporté (`ratatui::crossterm`). `TestBackend` affiche les émojis sur deux cellules.
- **rusqlite 0.40.2** (bundled) : SQLite 3.53.2.
- **Ambiguïté de types** : avec ratatui dans le même binaire, `x.as_ref() == b"..."` ne compile plus (compact_str ajoute des `AsRef`) ; comparer des `&str` explicitement.

- **rusqlite 0.40.2, phase 2** : le code du plan compile tel quel (`query_row("PRAGMA journal_mode = WAL")`, `VACUUM INTO ?1`, `busy_timeout`, `transaction()` sur `&mut Connection`). Seul ajustement : `Store::conn` n'est utilisé que par les tests tant que les modules de requêtes n'existent pas, donc `#[allow(dead_code)]` temporaire pour clippy `-D warnings` (à retirer quand un module l'utilise).
- **quick-xml 0.42.0, phase 3** : le code du plan compile tel quel (`reader.error_position()` existe, `normalized_value(XmlVersion::Implicit1_0)`, `e.name().0 == "outline"`). Aucun ajustement. rusqlite : `unchecked_transaction()`, `params![Option<i64>, ..]` et `IS ?1` fonctionnent tels quels. `#[allow(dead_code)]` de `Store::conn` retiré (utilisé par `store/feeds.rs`).
- **feed-rs 3.0.0, phase 4** : `Person::name` est un `Option<String>` (le plan supposait `String`) : `e.authors.first().and_then(|p| p.name.clone())`. `XmlError` a bien un `Display` (le test `Encoding` du plan compile tel quel).
- **reqwest 0.13.5 / DNS, phase 4** : sur macOS, `unknown_domain_is_a_dns_error` (domaine `.invalid`) passe avec les chaînes de `classify` du plan (« dns error » / « failed to lookup address »), aucune extension nécessaire.
- **Clippy `manual_async_fn`, phase 4** : le trait `Source` garde `-> impl Future<Output = ()> + Send`, mais l'impl de `LocalSource` s'écrit `async fn refresh(...)` (clippy `-D warnings` refuse la forme `impl Future` + `async move`). Le futur reste `Send`, les tests `refresh_*` passent.
- **rusqlite 0.40.2, phase 4** : le SQL et les `params!` de `store/fetch.rs` compilent tels quels (`unchecked_transaction`, booléens liés directement, `Option<u16>`).

### Phase 5 (diagnostic pur)
- whatlang 0.18 : aucune phrase des tests de détournement n'a été mal détectée, `MIN_DETECT_CONFIDENCE = 0.8` conservé sans allonger de phrase. Les 7 tests `hijack` passent du premier coup.
- clippy (rustc 1.99) : `manual_is_multiple_of` refuse `len % 2 == 0` ; utiliser `len.is_multiple_of(2)` dans la médiane.
- Test planifié `age_of_last_entry_decides_alive_dormant_fossil` : 1000 jours avant 2026-10-10 donne 2024-01-14, pas 2023. Attendu corrigé en "2024" (le code suit la spec : indice = année de la dernière entrée).
- Écart spec/plan : la spec exige `HtmlInsteadOfFeed` « sur une URL qui servait un flux » ; le plan pèse 0.3 dès que la dernière tentative est en `HtmlInsteadOfFeed`, sans vérifier l'historique. Gardé tel quel (à calibrer au vrai run).

### Vague finale de corrections (10/10/2026)
- **reqwest 0.13.5 / classification** : `is_connect()` est vrai aussi pour les échecs DNS et pour les échecs de poignée de main TLS (rustls). Tester `is_connect()` avant tout renverrait donc DNS et TLS en `Connect`. Ordre retenu : délai, DNS (texte), TLS (texte et `is_connect()`), corps/décodage, sinon `Connect`.
- **rustls** : un serveur HTTP en clair interrogé en `https://` donne « received corrupt message of type InvalidContentType », sans le mot « tls » ; le classement le rangeait en `Connect`. Marqueurs rustls ajoutés (« received corrupt message », « peer is incompatible », « peer misbehaved », « received fatal alert »).
- **`reqwest::Error::without_url()`** consomme l'erreur : `classify` prend désormais l'erreur par valeur. Le message de premier niveau contient l'URL complète (« error sending request for url (…) ») ; les sources de la chaîne, elles, ne la contiennent pas.
- **Délai total** : `tokio::time::timeout` autour d'une boucle qui écrit dans un `&mut RawResponse` ; à l'échéance, le futur est abandonné, l'emprunt se termine et la chaîne de redirections déjà vue reste lisible.
- **Ctrl-C en mode TUI** : en mode brut, crossterm désactive ISIG, Ctrl-C arrive comme touche et non comme signal ; le second Ctrl-C (sortie 130) ne concerne donc que `fetch` et `check`.

### Recette de l'appli Mac (11/10/2026)
- **Raccourcis sans modificateur dans les menus SwiftUI** : AppKit ne les essaie qu'après la vue qui a le clavier. La barre latérale et la liste (`SwiftUIOutlineListView`, sous-classe de `NSOutlineView`) prennent la barre d'espace et font de la sélection par frappe : `j` dans la barre latérale saute au dossier « Jon ». Le `WKWebView` du lecteur garde la barre d'espace pour son propre défilement, même tout en bas. Les touches de lecture passent donc par un moniteur local (`ReadingKeyRouter`), avant la vue, comme le fait NetNewsWire ; les menus gardent leurs raccourcis pour l'affichage et le clic.
- **Drapeaux des événements clavier** : un événement posté par `CGEvent.postToPid` porte un bit privé (1 << 29) dans `modifierFlags`, en plus de `.numericPad` et `.function` sur les flèches. On ne regarde que Maj, Contrôle, Option et Commande.
- **Barre d'outils (macOS 27)** : un `Label` y est affiché en icône seule, fondu dans la même capsule de verre que ses voisins. `.labelStyle(.titleAndIcon)` et un `ToolbarSpacer(.fixed)` de chaque côté donnent au bouton Full Article sa propre capsule, avec son titre.
- **`callAsyncJavaScript`** fonctionne avec `allowsContentJavaScript = false` : seul le JavaScript de la page est coupé, pas celui de l'appli (défilement par la barre d'espace vérifié sur un long article).
- **Liste et sélection au clavier** : une `List` SwiftUI ne fait pas défiler jusqu'à une sélection changée par le code ; `ScrollViewReader.scrollTo` sur `selectedEntryId`.

## Extraction d'articles : essai de `dom_smoothie` 0.18.2 (phase 1 du sous-projet 2a)

Essai du 10/10/2026 sur 10 pages réelles tirées des flux de l'utilisateur (base jetable du calibrage, liste d'URL gardée hors du dépôt), récupérées par `LocalSource::fetch_raw`. Commande : `RESSAC_SPIKE_URLS=… cargo test -p ressac-core extract::spike -- --ignored --nocapture`.

- **Verdict** : retenu. 8 articles sur 8 extraits (titre, texte de 922 à 7 420 caractères qui commence bien par le chapeau de l'article), aucune panique, 10 à 60 ms par page en debug (Wired : page de 1,4 Mo, 60 ms).
- **Pages refusées** : Le Monde sert une page « Client Challenge » en 200 ; `is_challenge_page` la reconnaît. Sans ce filtre, l'extracteur en tire 209 caractères avec le titre « Client Challenge » : d'où le contrôle des pages anti-robots *avant* l'extraction et le seuil `MIN_ARTICLE_CHARS` à 250. Libération répond 403 avec un corps vide ici : `fetch_raw` rend `Http(403)`, l'extraction stockera « HTTP 403. ».
- **Auteur** : peut être faux. Morgane Tual donne « OpenAI » (métadonnées de la page). Affiché tel quel, jamais utilisé pour un calcul. BBC donne deux auteurs (« Mike Peter, Matt Warwick »).
- **Titre** : parfois suivi du nom du site (« … - La République des livres »), et peut contenir des espaces insécables (Nautilus : `\u{a0}`). Gardé tel quel.
- **Texte court** : le billet-citation de Simon Willison (922 caractères) est jugé `is_probably_readable() == false` mais bien extrait : on ne se sert pas de `is_probably_readable`.
- **API** : `Readability::new(html, Some(url), Some(Config::default()))` refuse une URL relative (`BadDocumentURL`) ; avec une URL absolue, liens et images relatifs deviennent absolus. `parse()` échoue en `GrabFailed` sur une page vide. `content` et `text_content` sont des `StrTendril`.
