//! The chart snapshot: what `ressac chart --json` prints (sea chart, spec §5 and §6).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use rusqlite::params;
use serde::Serialize;

use super::Store;
use crate::error::Result;
use crate::model::{CadenceGroup, FeedId, parse_ts, ts};

/// Feeds whose last entry is older than this are buried and left off the chart.
const CHART_DAYS: i64 = 182;
/// Entries older than this no longer count towards a feed's activity.
const ACTIVITY_DAYS: i64 = 30;
/// Activity halves every this many days.
const HALF_LIFE_DAYS: f64 = 3.0;
/// Entries listed per feed, newest first.
const ENTRIES_PER_FEED: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartSnapshot {
    pub version: u32,
    pub surveyed_at: String,
    pub totals: ChartTotals,
    pub themes: Vec<ChartTheme>,
    pub feeds: Vec<ChartFeed>,
    pub territories: Vec<ChartTerritory>,
    pub continents: Vec<ChartContinent>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartTotals {
    pub feeds: usize,
    pub unread_week: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartTheme {
    pub key: String,
    pub name: String,
    pub sea: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartFeed {
    pub id: FeedId,
    pub title: String,
    pub theme: Option<String>,
    pub folder: Option<String>,
    pub group: CadenceGroup,
    pub age_days: f64,
    pub unread_week: usize,
    pub entries_week: usize,
    pub activity: f64,
    pub page: Option<u16>,
    pub entries: Vec<ChartEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartEntry {
    pub id: i64,
    pub title: String,
    pub url: Option<String>,
    pub day: String,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartTerritory {
    pub name: String,
    pub feeds: Vec<FeedId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartContinent {
    pub name: String,
    pub theme: String,
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn days_between(later: DateTime<Utc>, earlier: DateTime<Utc>) -> f64 {
    (later - earlier).num_milliseconds() as f64 / 86_400_000.0
}

/// The calendar day of `published` in `zone`: the page shows it as a date where the chart is
/// read, so the snapshot takes the local zone (`chrono::Local`). An unreadable timestamp keeps
/// its first ten characters.
fn entry_day<Tz: TimeZone>(published: &str, zone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    parse_ts(published)
        .map(|d| d.with_timezone(zone).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| published.chars().take(10).collect())
}

fn http_url(url: Option<String>) -> Option<String> {
    url.filter(|u| {
        let lower = u.trim_start().to_ascii_lowercase();
        lower.starts_with("http://") || lower.starts_with("https://")
    })
}

impl Store {
    /// The sea chart as data: chart feeds (active, not waiting for triage, an entry within
    /// 182 days of `now`), the themes and territories they belong to, and week totals.
    pub fn chart_snapshot(&self, now: DateTime<Utc>) -> Result<ChartSnapshot> {
        let conn = self.conn();
        let cutoff = ts(now - Duration::days(CHART_DAYS));
        let week_start = ts(now - Duration::days(7));
        let activity_start = ts(now - Duration::days(ACTIVITY_DAYS));

        let mut stmt = conn.prepare(
            "SELECT f.id, f.title, t.key, fp.path, COALESCE(f.cadence_override, h.cadence_group), \
                    f.page, \
                    (SELECT MAX(e.published_at) FROM entry e WHERE e.feed_id = f.id) AS last_at \
             FROM feed f \
             LEFT JOIN feed_health h ON h.feed_id = f.id \
             LEFT JOIN v_folder_path fp ON fp.folder_id = f.folder_id \
             LEFT JOIN theme t ON t.id = f.theme_id \
             WHERE f.state = 'active' AND COALESCE(h.needs_triage, 0) = 0 AND last_at >= ?1 \
             ORDER BY f.id",
        )?;
        let mut feeds: Vec<ChartFeed> = Vec::new();
        for row in stmt.query_map([&cutoff], |r| {
            Ok((
                r.get::<_, FeedId>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<u16>>(5)?,
                r.get::<_, String>(6)?,
            ))
        })? {
            let (id, title, theme, folder, group, page, last_at) = row?;
            let last = parse_ts(&last_at).unwrap_or(now);
            feeds.push(ChartFeed {
                id,
                title,
                theme,
                folder,
                group: group
                    .and_then(|g| CadenceGroup::parse(&g))
                    .unwrap_or_default(),
                age_days: round3(days_between(now, last).max(0.0)),
                unread_week: 0,
                entries_week: 0,
                activity: 0.0,
                page,
                entries: Vec::new(),
            });
        }
        drop(stmt);

        let index: HashMap<FeedId, usize> =
            feeds.iter().enumerate().map(|(i, f)| (f.id, i)).collect();

        // Week and activity figures come from the entries of the last 30 days.
        let mut stmt = conn.prepare(
            "SELECT feed_id, published_at, read_at IS NULL FROM entry WHERE published_at >= ?1",
        )?;
        let mut activity = vec![0.0_f64; feeds.len()];
        for row in stmt.query_map([&activity_start], |r| {
            Ok((
                r.get::<_, FeedId>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })? {
            let (feed_id, published, unread) = row?;
            let (Some(&i), Some(published_at)) = (index.get(&feed_id), parse_ts(&published)) else {
                continue;
            };
            let age = days_between(now, published_at).max(0.0);
            activity[i] += (-age / HALF_LIFE_DAYS).exp2();
            if published >= week_start {
                feeds[i].entries_week += 1;
                if unread {
                    feeds[i].unread_week += 1;
                }
            }
        }
        drop(stmt);

        let mut stmt = conn.prepare(
            "SELECT id, title, url, published_at, read_at IS NOT NULL FROM entry \
             WHERE feed_id = ?1 ORDER BY published_at DESC, id DESC LIMIT ?2",
        )?;
        for (feed, activity) in feeds.iter_mut().zip(activity) {
            feed.activity = round3(activity);
            for row in stmt.query_map(params![feed.id, ENTRIES_PER_FEED as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            })? {
                let (id, title, url, published, read) = row?;
                feed.entries.push(ChartEntry {
                    id,
                    title: title.unwrap_or_default(),
                    url: http_url(url),
                    day: entry_day(&published, &Local),
                    read,
                });
            }
        }
        drop(stmt);

        let themes = self
            .themes()?
            .into_iter()
            .filter_map(|t| {
                let count = feeds
                    .iter()
                    .filter(|f| f.theme.as_deref() == Some(t.key.as_str()))
                    .count();
                (count > 0).then_some(ChartTheme {
                    key: t.key,
                    name: t.name,
                    sea: t.sea,
                    count,
                })
            })
            .collect::<Vec<_>>();

        let mut territories: Vec<ChartTerritory> = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT t.name, tf.feed_id FROM territory t \
             JOIN territory_feed tf ON tf.territory_id = t.id \
             WHERE t.kind = 'territory' ORDER BY t.id, tf.position",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, FeedId>(1)?)))? {
            let (name, feed_id) = row?;
            if !index.contains_key(&feed_id) {
                continue;
            }
            match territories.last_mut() {
                Some(t) if t.name == name => t.feeds.push(feed_id),
                _ => territories.push(ChartTerritory {
                    name,
                    feeds: vec![feed_id],
                }),
            }
        }
        drop(stmt);
        territories.retain(|t| t.feeds.len() >= 2);

        let mut stmt = conn.prepare(
            "SELECT t.name, th.key FROM territory t JOIN theme th ON th.id = t.theme_id \
             WHERE t.kind = 'continent' ORDER BY t.id",
        )?;
        let continents = stmt
            .query_map([], |r| {
                Ok(ChartContinent {
                    name: r.get(0)?,
                    theme: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|c| themes.iter().any(|t| t.key == c.theme))
            .collect();

        Ok(ChartSnapshot {
            version: 1,
            surveyed_at: ts(now),
            totals: ChartTotals {
                feeds: feeds.len(),
                unread_week: feeds.iter().map(|f| f.unread_week).sum(),
            },
            themes,
            feeds,
            territories,
            continents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::chart::{ChartImport, TerritoryDef, ThemeDef};
    use crate::store::library::tests::{add_entry, add_feed, at};

    fn now() -> DateTime<Utc> {
        at("2026-10-19T12:00:00Z")
    }

    /// An entry published `hours` before `now()`.
    fn entry(s: &Store, id: i64, feed: i64, hours: i64, read: bool) {
        let published = ts(now() - Duration::hours(hours));
        add_entry(s, id, feed, &format!("Entry {id}"), &published, read, false);
    }

    fn store_with_feeds(n: i64) -> Store {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=n {
            add_feed(&s, id, None, id, None, "active", "daily");
        }
        s
    }

    fn ids(snap: &ChartSnapshot) -> Vec<i64> {
        snap.feeds.iter().map(|f| f.id).collect()
    }

    #[test]
    fn cutoff_is_182_days_inclusive() {
        let s = store_with_feeds(2);
        entry(&s, 1, 1, 182 * 24, false);
        entry(&s, 2, 2, 182 * 24 + 1, false);
        assert_eq!(ids(&s.chart_snapshot(now()).unwrap()), [1]);
    }

    #[test]
    fn buried_feeds_are_left_off() {
        let s = store_with_feeds(3);
        entry(&s, 1, 1, 24, false);
        entry(&s, 2, 2, 24, false);
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 2", [])
            .unwrap();
        // Feed 3 has no entry at all.
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(ids(&snap), [1]);
        assert_eq!(snap.totals.feeds, 1);
    }

    #[test]
    fn triage_feeds_are_left_off() {
        let s = store_with_feeds(2);
        entry(&s, 1, 1, 24, false);
        entry(&s, 2, 2, 24, false);
        s.conn()
            .execute(
                "UPDATE feed_health SET needs_triage = 1 WHERE feed_id = 2",
                [],
            )
            .unwrap();
        assert_eq!(ids(&s.chart_snapshot(now()).unwrap()), [1]);
    }

    #[test]
    fn activity_halves_every_three_days() {
        let s = store_with_feeds(1);
        entry(&s, 1, 1, 0, true);
        entry(&s, 2, 1, 3 * 24, true);
        entry(&s, 3, 1, 6 * 24, true);
        entry(&s, 4, 1, 31 * 24, true);
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(snap.feeds[0].activity, 1.75);
        assert_eq!(snap.feeds[0].age_days, 0.0);
    }

    #[test]
    fn weekly_counts() {
        let s = store_with_feeds(1);
        entry(&s, 1, 1, 24, false);
        entry(&s, 2, 1, 2 * 24, true);
        entry(&s, 3, 1, 6 * 24, false);
        entry(&s, 4, 1, 8 * 24, false);
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(snap.feeds[0].entries_week, 3);
        assert_eq!(snap.feeds[0].unread_week, 2);
        assert_eq!(snap.totals.unread_week, 2);
        assert_eq!(snap.feeds[0].age_days, 1.0);
    }

    #[test]
    fn entry_day_is_the_calendar_day_where_the_chart_is_read() {
        use chrono::FixedOffset;
        let late = "2026-10-18T23:30:00.000Z";
        let paris = FixedOffset::east_opt(2 * 3600).unwrap();
        let new_york = FixedOffset::west_opt(4 * 3600).unwrap();
        assert_eq!(entry_day(late, &paris), "2026-10-19");
        assert_eq!(entry_day(late, &new_york), "2026-10-18");
        assert_eq!(
            entry_day("2026-10-18T02:00:00.000Z", &new_york),
            "2026-10-17"
        );
        // An unreadable timestamp keeps its first ten characters.
        assert_eq!(entry_day("2026-10-18 garbled", &paris), "2026-10-18");
    }

    #[test]
    fn five_latest_entries_newest_first() {
        let s = store_with_feeds(1);
        for id in 1..=7 {
            entry(&s, id, 1, id * 24, id == 2);
        }
        let entries = &s.chart_snapshot(now()).unwrap().feeds[0].entries;
        let got: Vec<_> = entries.iter().map(|e| e.id).collect();
        assert_eq!(got, [1, 2, 3, 4, 5]);
        assert_eq!(entries[0].day, "2026-10-18");
        assert!(entries[1].read && !entries[0].read);
    }

    #[test]
    fn non_http_urls_are_null() {
        let s = store_with_feeds(1);
        for (id, url) in [
            (1, Some("https://a.example.org/x")),
            (2, Some("javascript:alert(1)")),
            (3, Some("file:///etc/passwd")),
            (4, None),
        ] {
            entry(&s, id, 1, id * 24, false);
            s.conn()
                .execute("UPDATE entry SET url = ?1 WHERE id = ?2", params![url, id])
                .unwrap();
        }
        let urls: Vec<_> = s.chart_snapshot(now()).unwrap().feeds[0]
            .entries
            .iter()
            .map(|e| e.url.clone())
            .collect();
        assert_eq!(
            urls,
            [Some("https://a.example.org/x".to_owned()), None, None, None]
        );
    }

    fn themed_import(territories: Vec<TerritoryDef>) -> ChartImport {
        ChartImport {
            themes: ["a", "b"]
                .map(|k| ThemeDef {
                    key: k.into(),
                    name: format!("Name {k}"),
                    sea: format!("SEA {k}"),
                })
                .to_vec(),
            feeds: (1..=3)
                .map(|i| (format!("https://f{i}.example.org/feed"), "a".to_owned()))
                .collect(),
            territories,
        }
    }

    fn terr(name: &str, feeds: &[i64]) -> TerritoryDef {
        TerritoryDef {
            name: name.into(),
            feeds: feeds
                .iter()
                .map(|i| format!("https://f{i}.example.org/feed"))
                .collect(),
            continent: None,
        }
    }

    #[test]
    fn territory_with_one_chart_member_is_dropped() {
        let s = store_with_feeds(4);
        for id in 1..=3 {
            entry(&s, id, id, 24, false);
        }
        // Feed 4 is buried by age.
        entry(&s, 4, 4, 300 * 24, false);
        let territories = vec![
            terr("KEPT", &[3, 1]),
            terr("DROPPED", &[2, 4]),
            TerritoryDef {
                name: "CONTINENT".into(),
                feeds: vec![],
                continent: Some("a".into()),
            },
            TerritoryDef {
                name: "EMPTY CONTINENT".into(),
                feeds: vec![],
                continent: Some("b".into()),
            },
        ];
        s.import_chart(&themed_import(territories), false).unwrap();
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(
            snap.territories,
            [ChartTerritory {
                name: "KEPT".into(),
                feeds: vec![3, 1]
            }]
        );
        assert_eq!(
            snap.continents,
            [ChartContinent {
                name: "CONTINENT".into(),
                theme: "a".into()
            }]
        );
        let themes: Vec<_> = snap
            .themes
            .iter()
            .map(|t| (t.key.as_str(), t.count))
            .collect();
        assert_eq!(themes, [("a", 3)]);
    }

    #[test]
    fn unthemed_feed_has_null_theme() {
        let s = store_with_feeds(1);
        entry(&s, 1, 1, 24, false);
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(snap.feeds[0].theme, None);
        assert!(snap.themes.is_empty());
    }

    #[test]
    fn silent_group_recent_feed_is_kept() {
        let s = Store::open_in_memory().unwrap();
        add_feed(&s, 1, None, 1, None, "active", "silent");
        entry(&s, 1, 1, 24, false);
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(ids(&snap), [1]);
        assert_eq!(snap.feeds[0].group, CadenceGroup::Silent);
    }

    #[test]
    fn undiagnosed_feed_is_kept() {
        let s = store_with_feeds(1);
        add_feed(&s, 2, None, 2, None, "active", "daily");
        s.conn()
            .execute("DELETE FROM feed_health WHERE feed_id = 2", [])
            .unwrap();
        entry(&s, 1, 1, 24, false);
        entry(&s, 2, 2, 24, false);
        let snap = s.chart_snapshot(now()).unwrap();
        assert_eq!(ids(&snap), [1, 2]);
        assert_eq!(snap.feeds[1].group, CadenceGroup::Silent);
    }

    #[test]
    fn json_shape() {
        let s = store_with_feeds(2);
        entry(&s, 1, 1, 24, false);
        entry(&s, 2, 2, 48, false);
        s.import_chart(&themed_import(vec![terr("PAIR", &[1, 2])]), false)
            .unwrap();
        let value = serde_json::to_value(s.chart_snapshot(now()).unwrap()).unwrap();
        let keys = |v: &serde_json::Value| {
            let mut k: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(
            keys(&value),
            [
                "continents",
                "feeds",
                "surveyed_at",
                "territories",
                "themes",
                "totals",
                "version"
            ]
        );
        assert_eq!(value["version"], 1);
        assert_eq!(value["surveyed_at"], "2026-10-19T12:00:00.000Z");
        assert_eq!(keys(&value["totals"]), ["feeds", "unread_week"]);
        assert_eq!(keys(&value["themes"][0]), ["count", "key", "name", "sea"]);
        assert_eq!(
            keys(&value["feeds"][0]),
            [
                "activity",
                "age_days",
                "entries",
                "entries_week",
                "folder",
                "group",
                "id",
                "page",
                "theme",
                "title",
                "unread_week"
            ]
        );
        assert_eq!(value["feeds"][0]["group"], "daily");
        assert_eq!(value["feeds"][0]["theme"], "a");
        assert!(value["feeds"][0]["folder"].is_null());
        assert_eq!(
            keys(&value["feeds"][0]["entries"][0]),
            ["day", "id", "read", "title", "url"]
        );
        assert_eq!(keys(&value["territories"][0]), ["feeds", "name"]);
    }
}
