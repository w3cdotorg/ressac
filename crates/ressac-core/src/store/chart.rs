//! Themes, territories and the chart import file (sea chart, spec §4 and §5).

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::{Error, Result};
use crate::model::FeedId;

/// A theme as written in the import file: a stable key, a display name and the sea's name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDef {
    pub key: String,
    pub name: String,
    pub sea: String,
}

/// A territory (a named group of feeds) or, with `continent`, a whole theme given a name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TerritoryDef {
    pub name: String,
    #[serde(default)]
    pub feeds: Vec<String>,
    #[serde(default)]
    pub continent: Option<String>,
}

/// The import file: themes in order, the theme of each feed URL, and the territories.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChartImport {
    pub themes: Vec<ThemeDef>,
    #[serde(default)]
    pub feeds: BTreeMap<String, String>,
    #[serde(default)]
    pub territories: Vec<TerritoryDef>,
}

/// What an import did (or, on a dry run, would do).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ChartImportReport {
    pub themes: usize,
    pub feeds_themed: usize,
    pub territories: usize,
    pub unmatched_urls: Vec<String>,
    pub unknown_theme_keys: Vec<String>,
    pub unthemed_feeds: usize,
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

    /// Sets a feed's theme, or clears it with `None`. Buried feeds are allowed.
    pub fn set_feed_theme(&self, feed: FeedId, key: Option<&str>) -> Result<()> {
        let theme_id = match key {
            Some(key) => Some(
                self.conn()
                    .query_row("SELECT id FROM theme WHERE key = ?1", [key], |r| {
                        r.get::<_, i64>(0)
                    })
                    .optional()?
                    .ok_or_else(|| Error::UnknownTheme(key.to_owned()))?,
            ),
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

    /// Applies an import file in one transaction; a dry run rolls it back but still reports.
    pub fn import_chart(&self, import: &ChartImport, dry_run: bool) -> Result<ChartImportReport> {
        let mut seen = HashSet::new();
        for t in &import.themes {
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

        // `xml_url` wins over `xml_url_original` when both name the same address.
        let mut feeds_by_url: HashMap<String, i64> = HashMap::new();
        for column in ["xml_url_original", "xml_url"] {
            let mut stmt =
                tx.prepare(&format!("SELECT {column}, id FROM feed ORDER BY id DESC"))?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (url, id) = row?;
                feeds_by_url.insert(url, id);
            }
        }

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
            matches!(&err, Error::UnknownTheme(k) if k == "nope"),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "No theme \"nope\".");
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
}
