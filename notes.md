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
