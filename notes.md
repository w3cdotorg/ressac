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
