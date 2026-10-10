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
