//! Themes, territories and the chart import file (sea chart, spec §4 and §5).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::{Error, Result};
use crate::model::FeedId;
use crate::url_key::host_key;

/// A theme as written in the import file: a stable key, a display name and the sea's name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDef {
    pub key: String,
    pub name: String,
    pub sea: String,
}

/// A territory (a named group of feeds) or, with `continent`, a whole theme given a name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerritoryDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continent: Option<String>,
}

/// The import file: themes in order, the theme of each feed URL, and the territories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartImport {
    pub themes: Vec<ThemeDef>,
    #[serde(default)]
    pub feeds: BTreeMap<String, String>,
    #[serde(default)]
    pub territories: Vec<TerritoryDef>,
}

/// What `Store::export_chart` read: the file `import_chart` takes back, and the feeds it could
/// not put in it (see `Store::export_chart`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartExport {
    pub chart: ChartImport,
    pub unaddressable: Vec<FeedId>,
}

/// What an import did (or, on a dry run, would do).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ChartImportReport {
    pub themes: usize,
    pub feeds_themed: usize,
    /// Territories (not continents) imported: those with at least two matched feeds.
    pub territories: usize,
    /// Territories stored with fewer than two matched feeds, which the chart leaves off.
    pub territories_too_small: Vec<String>,
    pub unmatched_urls: Vec<String>,
    pub unknown_theme_keys: Vec<String>,
    pub unthemed_feeds: usize,
}

/// A key `ressac feed theme FEED KEY` can name: not empty, not `none` in any letter case (the
/// word that clears a feed's theme there), and not starting with `-` (it would read as an option).
fn check_theme_key(key: &str) -> Result<()> {
    let problem = if key.is_empty() {
        "is empty"
    } else if key.eq_ignore_ascii_case("none") {
        "is reserved"
    } else if key.starts_with('-') {
        "starts with \"-\""
    } else {
        return Ok(());
    };
    Err(Error::Invalid(format!("Theme key \"{key}\" {problem}.")))
}

/// Active feeds on the same site as `host` (a `host_key`, compared with the host of each
/// feed's `html_url`, else `xml_url`), by feed id, each with its theme id.
pub(super) fn siblings_by_host(
    conn: &Connection,
    host: &str,
) -> Result<Vec<(FeedId, Option<i64>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, theme_id, COALESCE(html_url, xml_url) FROM feed \
         WHERE state = 'active' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, FeedId>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut siblings = Vec::new();
    for row in rows {
        let (id, theme_id, url) = row?;
        if host_key(&url).as_deref() == Some(host) {
            siblings.push((id, theme_id));
        }
    }
    Ok(siblings)
}

/// The theme most siblings share; a tie goes to the theme of the lowest feed id. Returns
/// that theme id and the lowest-id sibling holding it. `None` when no sibling has a theme.
pub(super) fn majority_theme(siblings: &[(FeedId, Option<i64>)]) -> Option<(i64, FeedId)> {
    let mut tally: Vec<(i64, FeedId, usize)> = Vec::new();
    // `siblings` is ordered by feed id, so the first sighting is the lowest id.
    for &(id, theme) in siblings {
        let Some(theme) = theme else { continue };
        match tally.iter_mut().find(|t| t.0 == theme) {
            Some(t) => t.2 += 1,
            None => tally.push((theme, id, 1)),
        }
    }
    // `max_by_key` keeps the last maximum, so rank ties by the lower id explicitly.
    tally
        .into_iter()
        .max_by_key(|&(_, first, count)| (count, std::cmp::Reverse(first)))
        .map(|(theme, first, _)| (theme, first))
}

/// Appends `feed` to the lowest-id territory (never a continent) any of `siblings` belongs
/// to, after its feeds; returns that territory's name. `None` when no sibling is in one.
pub(super) fn join_sibling_territory(
    conn: &Connection,
    siblings: &[(FeedId, Option<i64>)],
    feed: FeedId,
) -> Result<Option<String>> {
    let mut found: Option<(i64, String)> = None;
    for &(sibling, _) in siblings {
        let row: Option<(i64, String)> = conn
            .query_row(
                "SELECT t.id, t.name FROM territory_feed tf JOIN territory t ON t.id = tf.territory_id \
                 WHERE tf.feed_id = ?1 AND t.kind = 'territory' ORDER BY t.id LIMIT 1",
                [sibling],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some(row) = row
            && found.as_ref().is_none_or(|f| row.0 < f.0)
        {
            found = Some(row);
        }
    }
    let Some((territory, name)) = found else {
        return Ok(None);
    };
    conn.execute(
        "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, \
         (SELECT COALESCE(max(position) + 1, 0) FROM territory_feed WHERE territory_id = ?1))",
        params![territory, feed],
    )?;
    Ok(Some(name))
}

/// The feed each address resolves to when a chart file names it: `xml_url` wins over
/// `xml_url_original` when both name the same address, and of two feeds sharing one (a buried
/// twin) the lowest id wins. Import and export both read it, so they agree.
fn feed_owners(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut owners: HashMap<String, i64> = HashMap::new();
    for column in ["xml_url_original", "xml_url"] {
        let mut stmt = conn.prepare(&format!("SELECT {column}, id FROM feed ORDER BY id DESC"))?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (url, id) = row?;
            owners.insert(url, id);
        }
    }
    Ok(owners)
}

/// The id and definition of the theme `given` names (see `Store::resolve_theme`).
pub(super) fn resolve_theme_in(conn: &Connection, given: &str) -> Result<(i64, ThemeDef)> {
    let mut stmt =
        conn.prepare("SELECT id, key, name, sea_name FROM theme ORDER BY position, id")?;
    let all = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                ThemeDef {
                    key: r.get(1)?,
                    name: r.get(2)?,
                    sea: r.get(3)?,
                },
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let wanted = given.trim().to_lowercase();
    let found = all.iter().position(|(_, t)| t.key == given).or_else(|| {
        all.iter()
            .position(|(_, t)| t.name.trim().to_lowercase() == wanted)
    });
    match found {
        Some(i) => Ok(all.into_iter().nth(i).expect("index from position")),
        None => Err(Error::UnknownTheme {
            given: given.to_owned(),
            keys: all.into_iter().map(|(_, t)| t.key).collect(),
        }),
    }
}

impl Store {
    /// Themes in `position` order.
    pub fn themes(&self) -> Result<Vec<ThemeDef>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT key, name, sea_name FROM theme ORDER BY position, id")?;
        let rows = stmt.query_map([], |r| {
            Ok(ThemeDef {
                key: r.get(0)?,
                name: r.get(1)?,
                sea: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The theme `given` names: its key exactly, else its name compared case-insensitively
    /// after trimming. `UnknownTheme` (listing the valid keys) when neither matches. Every
    /// place that turns typed text into a theme goes through here.
    pub fn resolve_theme(&self, given: &str) -> Result<ThemeDef> {
        resolve_theme_in(self.conn(), given).map(|(_, def)| def)
    }

    /// Sets a feed's theme, given by key or name, or clears it with `None`. Buried feeds
    /// are allowed.
    pub fn set_feed_theme(&self, feed: FeedId, given: Option<&str>) -> Result<()> {
        let theme_id = match given {
            Some(given) => Some(resolve_theme_in(self.conn(), given)?.0),
            None => None,
        };
        let changed = self.conn().execute(
            "UPDATE feed SET theme_id = ?1 WHERE id = ?2",
            params![theme_id, feed],
        )?;
        if changed == 0 {
            return Err(Error::UnknownFeed(feed));
        }
        Ok(())
    }

    /// The database's themes, feed themes and territories as the file `import_chart` reads:
    /// themes in position order, each themed feed (buried ones too, as import accepts them)
    /// under its `xml_url`, territories and continents in creation order with their feeds in
    /// order. A feed whose addresses another feed claims first (a buried twin) cannot be named
    /// in a file that import would resolve to it; those ids come back in `unaddressable`
    /// instead of being exported under the wrong feed.
    pub fn export_chart(&self) -> Result<ChartExport> {
        let conn = self.conn();
        let owners = feed_owners(conn)?;
        let mut unaddressable = BTreeSet::new();
        let mut address_of: HashMap<i64, Option<String>> = HashMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT id, xml_url, xml_url_original FROM feed ORDER BY id")?;
            for row in stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (id, xml, original) = row?;
                let address = [xml, original]
                    .into_iter()
                    .find(|url| owners.get(url) == Some(&id));
                address_of.insert(id, address);
            }
        }
        let mut address = |id: i64| -> Option<String> {
            let found = address_of.get(&id).cloned().flatten();
            if found.is_none() {
                unaddressable.insert(id);
            }
            found
        };

        let mut feeds = BTreeMap::new();
        let themed: Vec<(i64, String)> = conn
            .prepare(
                "SELECT f.id, t.key FROM feed f JOIN theme t ON t.id = f.theme_id ORDER BY f.id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, key) in themed {
            if let Some(url) = address(id) {
                feeds.insert(url, key);
            }
        }

        let mut territories = Vec::new();
        let defs: Vec<(i64, String, Option<String>)> = conn
            .prepare(
                "SELECT t.id, t.name, th.key FROM territory t \
                 LEFT JOIN theme th ON th.id = t.theme_id ORDER BY t.id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, name, continent) in defs {
            let mut members = Vec::new();
            let ids: Vec<i64> = conn
                .prepare(
                    "SELECT feed_id FROM territory_feed WHERE territory_id = ?1 ORDER BY position",
                )?
                .query_map([id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            for feed in ids {
                members.extend(address(feed));
            }
            territories.push(TerritoryDef {
                name,
                feeds: members,
                continent,
            });
        }

        Ok(ChartExport {
            chart: ChartImport {
                themes: self.themes()?,
                feeds,
                territories,
            },
            unaddressable: unaddressable.into_iter().collect(),
        })
    }

    /// Applies an import file in one transaction; a dry run rolls it back but still reports.
    pub fn import_chart(&self, import: &ChartImport, dry_run: bool) -> Result<ChartImportReport> {
        let mut seen = HashSet::new();
        for t in &import.themes {
            check_theme_key(&t.key)?;
            if !seen.insert(t.key.as_str()) {
                return Err(Error::Invalid(format!(
                    "Theme \"{}\" appears twice.",
                    t.key
                )));
            }
        }
        seen.clear();
        for t in &import.territories {
            if !seen.insert(t.name.as_str()) {
                return Err(Error::Invalid(format!(
                    "Territory \"{}\" appears twice.",
                    t.name
                )));
            }
        }

        let tx = self.conn().unchecked_transaction()?;
        let mut report = ChartImportReport {
            themes: import.themes.len(),
            ..ChartImportReport::default()
        };

        let mut theme_ids: HashMap<&str, i64> = HashMap::new();
        for (position, t) in import.themes.iter().enumerate() {
            let id: i64 = tx.query_row(
                "INSERT INTO theme (key, name, sea_name, position) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(key) DO UPDATE SET name = excluded.name, \
                 sea_name = excluded.sea_name, position = excluded.position \
                 RETURNING id",
                params![t.key, t.name, t.sea, position as i64],
                |r| r.get(0),
            )?;
            theme_ids.insert(t.key.as_str(), id);
        }
        // A dropped theme leaves its feeds unthemed (ON DELETE SET NULL).
        let stored: Vec<(i64, String)> = tx
            .prepare("SELECT id, key FROM theme")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, key) in stored {
            if !theme_ids.contains_key(key.as_str()) {
                tx.execute("DELETE FROM theme WHERE id = ?1", [id])?;
            }
        }

        let feeds_by_url = feed_owners(&tx)?;

        let mut unknown_keys: Vec<String> = Vec::new();
        let mut note_unknown = |key: &str| {
            if !unknown_keys.iter().any(|k| k == key) {
                unknown_keys.push(key.to_owned());
            }
        };
        let note_unmatched = |report: &mut ChartImportReport, url: &str| {
            if !report.unmatched_urls.iter().any(|u| u == url) {
                report.unmatched_urls.push(url.to_owned());
            }
        };

        for (url, key) in &import.feeds {
            let Some(&theme_id) = theme_ids.get(key.as_str()) else {
                note_unknown(key);
                continue;
            };
            match feeds_by_url.get(url) {
                Some(&feed_id) => {
                    tx.execute(
                        "UPDATE feed SET theme_id = ?1 WHERE id = ?2",
                        params![theme_id, feed_id],
                    )?;
                    report.feeds_themed += 1;
                }
                None => note_unmatched(&mut report, url),
            }
        }

        tx.execute("DELETE FROM territory", [])?;
        for def in &import.territories {
            if let Some(key) = &def.continent {
                let Some(&theme_id) = theme_ids.get(key.as_str()) else {
                    note_unknown(key);
                    continue;
                };
                tx.execute(
                    "INSERT INTO territory (name, kind, theme_id) VALUES (?1, 'continent', ?2)",
                    params![def.name, theme_id],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO territory (name, kind) VALUES (?1, 'territory')",
                    [&def.name],
                )?;
                let territory_id = tx.last_insert_rowid();
                // Counts the feeds added: a URL listed twice adds its feed once.
                let mut position = 0_i64;
                for url in &def.feeds {
                    let Some(&feed_id) = feeds_by_url.get(url) else {
                        note_unmatched(&mut report, url);
                        continue;
                    };
                    let added = tx.execute(
                        "INSERT OR IGNORE INTO territory_feed (territory_id, feed_id, position) \
                         VALUES (?1, ?2, ?3)",
                        params![territory_id, feed_id, position],
                    )?;
                    position += added as i64;
                }
                // The chart draws a territory around two feeds or more (spec §5).
                if position < 2 {
                    report.territories_too_small.push(def.name.clone());
                    continue;
                }
            }
            report.territories += 1;
        }

        report.unthemed_feeds = tx.query_row(
            "SELECT count(*) FROM feed WHERE theme_id IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        )? as usize;
        report.unknown_theme_keys = unknown_keys;

        if dry_run {
            tx.rollback()?;
        } else {
            tx.commit()?;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::library::tests::add_feed;

    fn theme(key: &str) -> ThemeDef {
        ThemeDef {
            key: key.into(),
            name: format!("Name {key}"),
            sea: format!("SEA OF {}", key.to_uppercase()),
        }
    }

    fn url(id: i64) -> String {
        format!("https://f{id}.example.org/feed")
    }

    fn store_with_feeds(n: i64) -> Store {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=n {
            add_feed(&s, id, None, id, None, "active", "daily");
        }
        s
    }

    fn feed_theme(s: &Store, id: i64) -> Option<String> {
        s.conn()
            .query_row(
                "SELECT t.key FROM feed f LEFT JOIN theme t ON t.id = f.theme_id WHERE f.id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn count(s: &Store, table: &str) -> i64 {
        s.conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn import(
        themes: &[&str],
        feeds: &[(String, &str)],
        territories: Vec<TerritoryDef>,
    ) -> ChartImport {
        ChartImport {
            themes: themes.iter().map(|k| theme(k)).collect(),
            feeds: feeds
                .iter()
                .map(|(u, k)| (u.clone(), (*k).into()))
                .collect(),
            territories,
        }
    }

    fn territory(name: &str, feeds: &[String]) -> TerritoryDef {
        TerritoryDef {
            name: name.into(),
            feeds: feeds.to_vec(),
            continent: None,
        }
    }

    #[test]
    fn themes_in_file_order() {
        let s = store_with_feeds(0);
        s.import_chart(&import(&["zeta", "alpha", "mid"], &[], vec![]), false)
            .unwrap();
        let keys: Vec<_> = s.themes().unwrap().into_iter().map(|t| t.key).collect();
        assert_eq!(keys, ["zeta", "alpha", "mid"]);
        assert_eq!(s.themes().unwrap()[0], theme("zeta"));
    }

    #[test]
    fn set_and_clear_feed_theme() {
        let s = store_with_feeds(1);
        s.import_chart(&import(&["a"], &[], vec![]), false).unwrap();
        s.set_feed_theme(1, Some("a")).unwrap();
        assert_eq!(feed_theme(&s, 1).as_deref(), Some("a"));
        s.set_feed_theme(1, None).unwrap();
        assert_eq!(feed_theme(&s, 1), None);
    }

    #[test]
    fn unknown_theme_is_refused() {
        let s = store_with_feeds(1);
        let err = s.set_feed_theme(1, Some("nope")).unwrap_err();
        assert!(
            matches!(&err, Error::UnknownTheme { given, .. } if given == "nope"),
            "{err:?}"
        );
        assert_eq!(
            err.to_string(),
            "No theme \"nope\". No themes yet: import them with ressac chart import."
        );
        let err = s.set_feed_theme(99, None).unwrap_err();
        assert!(matches!(err, Error::UnknownFeed(99)), "{err:?}");
    }

    #[test]
    fn import_matches_original_urls() {
        let s = store_with_feeds(2);
        s.conn()
            .execute(
                "UPDATE feed SET xml_url = 'https://moved.example.org/feed' WHERE id = 2",
                [],
            )
            .unwrap();
        let report = s
            .import_chart(
                &import(&["a"], &[(url(1), "a"), (url(2), "a")], vec![]),
                false,
            )
            .unwrap();
        assert_eq!(report.feeds_themed, 2);
        assert!(report.unmatched_urls.is_empty());
        assert_eq!(feed_theme(&s, 2).as_deref(), Some("a"));
    }

    #[test]
    fn import_reports_unmatched() {
        let s = store_with_feeds(2);
        let report = s
            .import_chart(
                &import(
                    &["a"],
                    &[
                        (url(1), "a"),
                        ("https://ghost.example.org/feed".into(), "a"),
                        (url(2), "missing"),
                    ],
                    vec![],
                ),
                false,
            )
            .unwrap();
        assert_eq!(report.themes, 1);
        assert_eq!(report.feeds_themed, 1);
        assert_eq!(report.unmatched_urls, ["https://ghost.example.org/feed"]);
        assert_eq!(report.unknown_theme_keys, ["missing"]);
        assert_eq!(report.unthemed_feeds, 1);
        assert_eq!(feed_theme(&s, 2), None);
    }

    #[test]
    fn reimport_drops_a_theme() {
        let s = store_with_feeds(2);
        s.import_chart(
            &import(&["a", "b"], &[(url(1), "a"), (url(2), "b")], vec![]),
            false,
        )
        .unwrap();
        let report = s
            .import_chart(&import(&["a"], &[(url(1), "a")], vec![]), false)
            .unwrap();
        assert_eq!(count(&s, "theme"), 1);
        assert_eq!(feed_theme(&s, 1).as_deref(), Some("a"));
        assert_eq!(feed_theme(&s, 2), None);
        assert_eq!(report.unthemed_feeds, 1);
    }

    #[test]
    fn territories_replaced_on_reimport() {
        let s = store_with_feeds(3);
        let first = import(
            &["a"],
            &[],
            vec![
                territory(
                    "ONE",
                    &[url(1), url(2), "https://ghost.example.org/x".into()],
                ),
                TerritoryDef {
                    name: "ALL A".into(),
                    feeds: vec![],
                    continent: Some("a".into()),
                },
            ],
        );
        let report = s.import_chart(&first, false).unwrap();
        assert_eq!(report.territories, 2);
        assert_eq!(report.unmatched_urls, ["https://ghost.example.org/x"]);
        let members: Vec<(i64, i64)> = s
            .conn()
            .prepare(
                "SELECT feed_id, position FROM territory_feed tf JOIN territory t ON t.id = tf.territory_id \
                 WHERE t.name = 'ONE' ORDER BY position",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(members, [(1, 0), (2, 1)]);
        let kind: String = s
            .conn()
            .query_row("SELECT kind FROM territory WHERE name = 'ALL A'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kind, "continent");

        s.import_chart(
            &import(&["a"], &[], vec![territory("TWO", &[url(3)])]),
            false,
        )
        .unwrap();
        let names: Vec<String> = s
            .conn()
            .prepare("SELECT name FROM territory")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(names, ["TWO"]);
        assert_eq!(count(&s, "territory_feed"), 1);
    }

    #[test]
    fn continent_without_known_theme_reported() {
        let s = store_with_feeds(0);
        let report = s
            .import_chart(
                &import(
                    &["a"],
                    &[],
                    vec![TerritoryDef {
                        name: "LOST".into(),
                        feeds: vec![],
                        continent: Some("ghost".into()),
                    }],
                ),
                false,
            )
            .unwrap();
        assert_eq!(report.unknown_theme_keys, ["ghost"]);
        assert_eq!(report.territories, 0);
        assert_eq!(count(&s, "territory"), 0);
    }

    #[test]
    fn reserved_and_flag_like_theme_keys_are_refused() {
        let s = store_with_feeds(0);
        for key in ["", "none", "None", "NONE", "-x", "--labs"] {
            let err = s
                .import_chart(&import(&[key], &[], vec![]), false)
                .unwrap_err();
            assert!(matches!(err, Error::Invalid(_)), "{key:?}: {err:?}");
        }
        assert_eq!(count(&s, "theme"), 0);
        s.import_chart(&import(&["nonesuch", "a-b"], &[], vec![]), false)
            .unwrap();
        assert_eq!(count(&s, "theme"), 2);
    }

    #[test]
    fn territories_with_fewer_than_two_matched_feeds_are_reported() {
        let s = store_with_feeds(3);
        let report = s
            .import_chart(
                &import(
                    &["a"],
                    &[],
                    vec![
                        territory("PAIR", &[url(1), url(2)]),
                        territory("LONE", &[url(3), "https://ghost.example.org/x".into()]),
                        territory("EMPTY", &[]),
                        TerritoryDef {
                            name: "ALL A".into(),
                            feeds: vec![],
                            continent: Some("a".into()),
                        },
                    ],
                ),
                false,
            )
            .unwrap();
        assert_eq!(report.territories, 2, "PAIR and the continent");
        assert_eq!(report.territories_too_small, ["LONE", "EMPTY"]);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let s = store_with_feeds(2);
        let report = s
            .import_chart(
                &import(
                    &["a"],
                    &[(url(1), "a")],
                    vec![territory("ONE", &[url(1), url(2)])],
                ),
                true,
            )
            .unwrap();
        assert_eq!(report.themes, 1);
        assert_eq!(report.feeds_themed, 1);
        assert_eq!(report.territories, 1);
        assert_eq!(report.unthemed_feeds, 1);
        assert_eq!(count(&s, "theme"), 0);
        assert_eq!(count(&s, "territory"), 0);
        assert_eq!(feed_theme(&s, 1), None);
    }

    const NOW: &str = "2026-10-20T12:00:00Z";

    /// One recent entry per feed, so the chart draws them all.
    fn with_entries(s: &Store, n: i64) {
        for id in 1..=n {
            s.conn()
                .execute(
                    "INSERT INTO entry (id, feed_id, guid, title, published_at, fetched_at) \
                     VALUES (?1, ?1, ?2, ?3, '2026-10-19T08:00:00.000Z', '2026-10-19T08:00:00.000Z')",
                    params![id, format!("g{id}"), format!("Entry {id}")],
                )
                .unwrap();
        }
    }

    fn snapshot(s: &Store) -> serde_json::Value {
        let mut v = serde_json::to_value(s.chart_snapshot(NOW.parse().unwrap()).unwrap()).unwrap();
        v["surveyed_at"] = serde_json::Value::Null;
        v
    }

    fn continent(name: &str, key: &str) -> TerritoryDef {
        TerritoryDef {
            name: name.into(),
            feeds: vec![],
            continent: Some(key.into()),
        }
    }

    /// Three themes (one empty), three of five feeds themed, a territory and a continent.
    fn charted_store() -> Store {
        let s = store_with_feeds(5);
        with_entries(&s, 5);
        s.import_chart(
            &import(
                &["zeta", "alpha", "empty"],
                &[(url(3), "alpha"), (url(1), "zeta"), (url(4), "alpha")],
                vec![
                    territory("PAIR", &[url(4), url(3)]),
                    continent("ALL ALPHA", "alpha"),
                    territory("LONE", &[url(1)]),
                ],
            ),
            false,
        )
        .unwrap();
        s
    }

    #[test]
    fn export_writes_the_import_format_in_order() {
        let s = charted_store();
        let export = s.export_chart().unwrap();
        assert_eq!(export.unaddressable, Vec::<FeedId>::new());
        let chart = export.chart;
        let keys: Vec<_> = chart.themes.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            ["zeta", "alpha", "empty"],
            "position order, empty theme kept"
        );
        assert_eq!(chart.themes[0], theme("zeta"));
        assert_eq!(
            chart.feeds,
            BTreeMap::from([
                (url(1), "zeta".to_owned()),
                (url(3), "alpha".to_owned()),
                (url(4), "alpha".to_owned()),
            ])
        );
        assert_eq!(
            chart.territories,
            vec![
                territory("PAIR", &[url(4), url(3)]),
                continent("ALL ALPHA", "alpha"),
                territory("LONE", &[url(1)]),
            ],
            "creation order, feeds in territory order"
        );
    }

    #[test]
    fn an_exported_file_imports_into_a_fresh_store_to_the_same_chart() {
        let a = charted_store();
        let exported = a.export_chart().unwrap().chart;
        // The file form, as the CLI writes it.
        let text = serde_json::to_string_pretty(&exported).unwrap();
        let parsed: ChartImport = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, exported);

        let b = store_with_feeds(5);
        with_entries(&b, 5);
        let report = b.import_chart(&parsed, false).unwrap();
        assert!(report.unmatched_urls.is_empty() && report.unknown_theme_keys.is_empty());
        assert_eq!(snapshot(&a), snapshot(&b));
        assert_eq!(b.export_chart().unwrap().chart, exported);
        assert_eq!(a.themes().unwrap(), b.themes().unwrap());
    }

    #[test]
    fn export_includes_what_an_added_feed_inherited() {
        let s = store_with_feeds(3);
        for id in [1, 2] {
            s.conn()
                .execute(
                    "UPDATE feed SET xml_url = ?2, xml_url_original = ?2, html_url = 'https://site.example.org/' WHERE id = ?1",
                    params![id, format!("https://site.example.org/feed{id}.xml")],
                )
                .unwrap();
        }
        let site = |id: i64| format!("https://site.example.org/feed{id}.xml");
        s.import_chart(
            &import(
                &["a"],
                &[(site(1), "a")],
                vec![territory("SITE", &[site(1), site(2)])],
            ),
            false,
        )
        .unwrap();
        let added = s
            .insert_added_feed(
                &crate::store::NewFeed {
                    xml_url: "https://site.example.org/new.xml".into(),
                    html_url: Some("https://site.example.org/".into()),
                    title: "New".into(),
                    known_as: Vec::new(),
                },
                None,
                NOW.parse().unwrap(),
            )
            .unwrap();
        assert_eq!(added.territory.as_deref(), Some("SITE"));
        let chart = s.export_chart().unwrap().chart;
        assert_eq!(chart.feeds["https://site.example.org/new.xml"], "a");
        assert_eq!(
            chart.territories,
            [territory(
                "SITE",
                &[site(1), site(2), "https://site.example.org/new.xml".into()]
            )]
        );
    }

    #[test]
    fn export_keeps_the_assignments_of_buried_feeds() {
        let s = store_with_feeds(3);
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 2", [])
            .unwrap();
        s.import_chart(
            &import(
                &["a"],
                &[(url(1), "a"), (url(2), "a")],
                vec![territory("PAIR", &[url(1), url(2)])],
            ),
            false,
        )
        .unwrap();
        let chart = s.export_chart().unwrap().chart;
        assert_eq!(chart.feeds.len(), 2);
        assert_eq!(chart.territories, [territory("PAIR", &[url(1), url(2)])]);
    }

    #[test]
    fn a_feed_import_cannot_address_is_reported_not_exported() {
        let s = store_with_feeds(2);
        // Feed 2 is a buried twin of feed 1: import resolves their shared address to feed 1.
        s.conn()
            .execute(
                "UPDATE feed SET state = 'buried', xml_url = ?1, xml_url_original = ?1 WHERE id = 2",
                [url(1)],
            )
            .unwrap();
        s.conn()
            .execute_batch(
                "INSERT INTO theme (id, key, name, sea_name, position) VALUES (1, 'a', 'A', 'SEA', 0);
                 UPDATE feed SET theme_id = 1;",
            )
            .unwrap();
        let export = s.export_chart().unwrap();
        assert_eq!(
            export.chart.feeds,
            BTreeMap::from([(url(1), "a".to_owned())])
        );
        assert_eq!(export.unaddressable, [2]);
    }

    #[test]
    fn export_of_an_empty_store_is_an_empty_file() {
        let s = store_with_feeds(2);
        let chart = s.export_chart().unwrap().chart;
        assert_eq!(
            serde_json::to_value(&chart).unwrap(),
            serde_json::json!({"themes": [], "feeds": {}, "territories": []})
        );
    }
}
