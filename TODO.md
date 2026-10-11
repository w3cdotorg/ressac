# TODO

Prochaines étapes de Ressac, au 11/10/2026. Le détail des décisions passées est dans `notes.md`.

## À décider

- [ ] **CI macOS : `|| true` sur la détection de Xcode.** Les deux lignes `xcode=$(xcodebuild -version | head -n 1)` et `sdk=$(xcrun --sdk macosx --show-sdk-version)` de `.github/workflows/ci.yml` feraient échouer l'étape si `xcodebuild` lui-même plantait. La modification a été refusée par le garde-fou automatique de Claude Code ; le relecteur la juge sans conséquence (les images macOS de GitHub ont toujours un Xcode).

## Mise en service

- [ ] Compiler l'appli en Release et la copier dans `/Applications` (commandes dans le README, section « The Mac app »).
- [ ] Premier lancement : déposer `~/Documents/vienna_20260415.opml` sur la fenêtre, laisser l'import et le premier rafraîchissement se faire.
- [ ] Faire le tri dans le jardin, dont la fusion de « Google Blog: Research » dans « Google Blog: Innovation & AI ».
- [ ] Exporter un OPML propre une fois le tri fini.
- [ ] Optionnel : lien `ressac` dans le `PATH` vers `/Applications/Ressac.app/Contents/Helpers/ressac`.

## Sous-projets suivants

- [ ] **2b : la lecture dans le terminal.** Trois colonnes en ratatui sur le même schéma v2 (lu, favori, recherche, article complet), avec clappuccino comme référence.
- [ ] **3 : rythme et journal du matin.** Tri des flux par rythme de publication, édition du matin.
- [ ] **4 : thème Antiope.** Le télétexte, pour la TUI et l'appli.

## Reporté (relectures du sous-projet 2a)

Aucun n'est bloquant ; chacun a été jugé « peut attendre » lors de la relecture finale.

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
