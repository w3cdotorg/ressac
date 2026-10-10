# Ressac

Un lecteur de flux RSS/Atom/JSON Feed pour macOS, en deux interfaces : terminal (Rust, ratatui) et appli native (SwiftUI). Le flux et le reflux de vos abonnements, avec le ménage en prime.

Thème graphique rétro : **Antiope**, en hommage au télétexte français.

## Usage

```sh
ressac import ~/Documents/vienna.opml   # --dry-run to preview
ressac check                           # fetch, diagnose, look up archives
ressac garden                          # triage: k keep, r repair, b bury, u undo, ? help
ressac export -o clean.opml            # --cemetery for buried feeds
```

Global flags: `--db <file>` (or `RESSAC_DB`), `--json`, `--ascii`, `-v`.
The database lives in `~/Library/Application Support/Ressac/ressac.sqlite`, logs in `~/Library/Logs/Ressac/`.
