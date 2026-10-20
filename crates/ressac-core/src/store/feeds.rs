//! Folders and feeds: OPML import and read access.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::chart::{join_sibling_territory, majority_theme, siblings_by_host};
use super::{Store, ThemeDef, pages};
use crate::error::{Error, Result};
use crate::model::{CadenceGroup, FeedId, FolderId, ts};
use crate::opml::Outline;
use crate::url_key::{host_key, url_key};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub folders_added: usize,
    pub feeds_added: usize,
    /// (dropped title, kept title) for exact duplicates inside the imported file.
    pub merged: Vec<(String, String)>,
    /// Feeds already in the database, in any state, under their current or original address.
    pub skipped_existing: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedRow {
    pub id: FeedId,
    pub folder_id: Option<FolderId>,
    pub position: i64,
    pub title: String,
    pub xml_url: String,
    pub xml_url_original: String,
    pub html_url: Option<String>,
    pub url_key: String,
    pub final_url: Option<String>,
    pub description_original: Option<String>,
    pub description_current: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub state: String,
    pub merged_into_id: Option<FeedId>,
    pub opml_attrs: String,
    pub https_available: Option<bool>,
    pub discovered_url: Option<String>,
    pub discovery_checked_at: Option<String>,
    pub wayback_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    pub position: i64,
}

/// One feed's rhythm as the apps see it in `v_cadence`: the group in use, the computed one,
/// and whether Clément forced it (spec §4, Override).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CadenceRow {
    pub feed_id: FeedId,
    pub effective_group: CadenceGroup,
    pub computed_group: CadenceGroup,
    pub is_override: bool,
}

/// A group name read from the database, or a conversion error naming the column.
fn group_at(r: &Row<'_>, col: usize) -> rusqlite::Result<Option<CadenceGroup>> {
    let name: Option<String> = r.get(col)?;
    name.map(|n| {
        CadenceGroup::parse(&n).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                col,
                rusqlite::types::Type::Text,
                format!("unknown rhythm group {n:?}").into(),
            )
        })
    })
    .transpose()
}

const FEED_COLUMNS: &str = "id, folder_id, position, title, xml_url, xml_url_original, html_url, url_key, \
     final_url, description_original, description_current, etag, last_modified, state, merged_into_id, \
     opml_attrs, https_available, discovered_url, discovery_checked_at, wayback_url";

fn feed_row(r: &Row<'_>) -> rusqlite::Result<FeedRow> {
    Ok(FeedRow {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        position: r.get(2)?,
        title: r.get(3)?,
        xml_url: r.get(4)?,
        xml_url_original: r.get(5)?,
        html_url: r.get(6)?,
        url_key: r.get(7)?,
        final_url: r.get(8)?,
        description_original: r.get(9)?,
        description_current: r.get(10)?,
        etag: r.get(11)?,
        last_modified: r.get(12)?,
        state: r.get(13)?,
        merged_into_id: r.get(14)?,
        opml_attrs: r.get(15)?,
        https_available: r.get(16)?,
        discovered_url: r.get(17)?,
        discovery_checked_at: r.get(18)?,
        wayback_url: r.get(19)?,
    })
}

/// `Store::find_duplicate` on any connection, so a write transaction can run it too.
fn duplicate_in(conn: &Connection, urls: &[&str]) -> Result<Option<(FeedId, String, bool)>> {
    let keys: HashSet<String> = urls.iter().map(|u| url_key(u)).collect();
    let mut stmt =
        conn.prepare("SELECT id, title, state, url_key, xml_url_original FROM feed ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, FeedId>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (id, title, state, key, original) = row?;
        if keys.contains(&key) || keys.contains(&url_key(&original)) {
            return Ok(Some((id, title, state == "buried")));
        }
    }
    Ok(None)
}

/// What `Store::move_feed` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Moved {
    pub feed: FeedId,
    pub title: String,
    pub from: Option<FolderId>,
    pub to: Option<FolderId>,
    pub old_page: Option<u16>,
    pub new_page: Option<u16>,
    /// False when the feed already sat in the target: nothing was written.
    pub changed: bool,
}

/// A feed to add: the resolved feed address, the site's page when known, and its title
/// (an empty one falls back to the site's host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFeed {
    pub xml_url: String,
    pub html_url: Option<String>,
    pub title: String,
}

/// Where an added feed's theme came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ThemeSource {
    /// The caller named it.
    Chosen,
    /// Inherited from the feeds of the same site; `id`/`title` name the sibling that holds it.
    Sibling { id: FeedId, title: String },
    /// No theme: no choice made and no themed sibling.
    None,
}

/// What `Store::insert_added_feed` wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inserted {
    pub id: FeedId,
    pub title: String,
    pub page: Option<u16>,
    pub theme: Option<ThemeDef>,
    pub theme_source: ThemeSource,
    /// The territory joined through a sibling, never a continent.
    pub territory: Option<String>,
}

impl Store {
    /// The first feed, buried or not, whose address (current or original) has the same
    /// `url_key` as any of `urls`: its id, title and whether it is buried. Reads only.
    pub fn find_duplicate(&self, urls: &[&str]) -> Result<Option<(FeedId, String, bool)>> {
        duplicate_in(self.conn(), urls)
    }

    /// Adds one feed at the bottom of the root, with a root page, in one transaction. The
    /// theme is `theme` (a known key, else `UnknownTheme` before anything is written) or,
    /// when none is given, the one most active feeds of the same site share (`host_key`;
    /// ties: the lowest feed id's); with a themed or not sibling in a territory, the feed
    /// joins the lowest-id one. Checks again, inside its own write transaction, that the new
    /// address is not already followed (`Error::FeedExists`), so two concurrent adds cannot
    /// both pass; callers check earlier with `find_duplicate` to avoid network work.
    pub fn insert_added_feed(
        &self,
        new: &NewFeed,
        theme: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Inserted> {
        let tx = self.write_tx()?;
        if let Some((id, title, buried)) = duplicate_in(&tx, &[&new.xml_url])? {
            return Err(Error::FeedExists { id, title, buried });
        }
        let chosen: Option<(i64, ThemeDef)> = match theme {
            Some(key) => Some(
                tx.query_row(
                    "SELECT id, key, name, sea_name FROM theme WHERE key = ?1",
                    [key],
                    |r| {
                        Ok((
                            r.get(0)?,
                            ThemeDef {
                                key: r.get(1)?,
                                name: r.get(2)?,
                                sea: r.get(3)?,
                            },
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| Error::UnknownTheme(key.to_owned()))?,
            ),
            None => None,
        };
        let site = new.html_url.as_deref().unwrap_or(&new.xml_url);
        let host = host_key(site).or_else(|| host_key(&new.xml_url));
        let title = match new.title.trim() {
            "" => host.clone().unwrap_or_else(|| new.xml_url.clone()),
            t => t.to_owned(),
        };
        let siblings = match (&chosen, &host) {
            (None, Some(host)) => siblings_by_host(&tx, host)?,
            _ => Vec::new(),
        };
        let (theme_id, theme_source) = match (&chosen, majority_theme(&siblings)) {
            (Some((id, _)), _) => (Some(*id), ThemeSource::Chosen),
            (None, Some((id, from))) => {
                let sibling_title: String =
                    tx.query_row("SELECT title FROM feed WHERE id = ?1", [from], |r| r.get(0))?;
                (
                    Some(id),
                    ThemeSource::Sibling {
                        id: from,
                        title: sibling_title,
                    },
                )
            }
            (None, None) => (None, ThemeSource::None),
        };
        tx.execute(
            "INSERT INTO feed (folder_id, position, title, xml_url, xml_url_original, html_url, \
             url_key, theme_id, added_at) VALUES (NULL, ?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7)",
            params![
                next_position(&tx, None)?,
                title,
                new.xml_url,
                new.html_url,
                url_key(&new.xml_url),
                theme_id,
                ts(now)
            ],
        )?;
        let id = tx.last_insert_rowid();
        pages::assign_missing(&tx)?;
        let territory = join_sibling_territory(&tx, &siblings, id)?;
        let page = tx.query_row("SELECT page FROM feed WHERE id = ?1", [id], |r| r.get(0))?;
        let theme = match (chosen, theme_id) {
            (Some((_, def)), _) => Some(def),
            (None, Some(theme_id)) => Some(tx.query_row(
                "SELECT key, name, sea_name FROM theme WHERE id = ?1",
                [theme_id],
                |r| {
                    Ok(ThemeDef {
                        key: r.get(0)?,
                        name: r.get(1)?,
                        sea: r.get(2)?,
                    })
                },
            )?),
            (None, None) => None,
        };
        tx.commit()?;
        Ok(Inserted {
            id,
            title,
            page,
            theme,
            theme_source,
            territory,
        })
    }

    /// Imports folders and feeds. Folders are matched by (parent, name). A feed already
    /// exists when any feed, buried or not, has its URL key as current or original address,
    /// so triage (burials, merges, repairs) survives a re-import. New items go after the
    /// existing ones of their folder and get their page numbers in the same transaction.
    /// A dry run executes the same statements and rolls them back.
    pub fn import_outlines(
        &self,
        outlines: &[Outline],
        dry_run: bool,
        now: DateTime<Utc>,
    ) -> Result<ImportReport> {
        let tx = self.write_tx()?;
        let mut report = ImportReport::default();
        let known = known_keys(&tx)?;
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut ctx = ImportCtx {
            tx: &tx,
            now: &ts(now),
            known: &known,
            seen: &mut seen,
            report: &mut report,
        };
        import_level(&mut ctx, outlines, None)?;
        super::pages::assign_missing(&tx)?;
        if dry_run {
            tx.rollback()?;
        } else {
            tx.commit()?;
        }
        Ok(report)
    }

    pub fn feeds(&self) -> Result<Vec<FeedRow>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {FEED_COLUMNS} FROM feed ORDER BY id"))?;
        let rows = stmt
            .query_map([], feed_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Number of feeds in any state.
    pub fn feed_count(&self) -> Result<usize> {
        let n: i64 = self
            .conn()
            .query_row("SELECT count(*) FROM feed", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn feed(&self, id: FeedId) -> Result<FeedRow> {
        Ok(self.conn().query_row(
            &format!("SELECT {FEED_COLUMNS} FROM feed WHERE id = ?1"),
            [id],
            feed_row,
        )?)
    }

    /// Moves a feed, buried or not, to the end of `folder` (`None` = the root). It takes the
    /// lowest free page of its new home (see `pages::page_after_move`) and keeps its own when
    /// the home is full or already holds it. Refuses an unknown feed or folder; nothing is
    /// written then. Already there: nothing is written and `changed` is false.
    pub fn move_feed(&self, feed: FeedId, folder: Option<FolderId>) -> Result<Moved> {
        let tx = self.write_tx()?;
        let row: Option<(String, Option<FolderId>, Option<u16>)> = tx
            .query_row(
                "SELECT title, folder_id, page FROM feed WHERE id = ?1",
                [feed],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((title, from, old_page)) = row else {
            return Err(Error::UnknownFeed(feed));
        };
        let new_page = pages::page_after_move(&tx, old_page, folder)?;
        if from == folder {
            return Ok(Moved {
                feed,
                title,
                from,
                to: folder,
                old_page,
                new_page: old_page,
                changed: false,
            });
        }
        let position = next_position(&tx, folder)?;
        tx.execute(
            "UPDATE feed SET folder_id = ?2, position = ?3, page = ?4 WHERE id = ?1",
            params![feed, folder, position, new_page],
        )?;
        tx.commit()?;
        Ok(Moved {
            feed,
            title,
            from,
            to: folder,
            old_page,
            new_page,
            changed: true,
        })
    }

    /// Forces a feed's rhythm group, or clears the override (`None`). Refuses an unknown
    /// feed and a buried one; nothing is written then. Never cleared automatically.
    pub fn set_cadence_override(&self, feed_id: FeedId, group: Option<CadenceGroup>) -> Result<()> {
        let tx = self.write_tx()?;
        let state: Option<String> = tx
            .query_row("SELECT state FROM feed WHERE id = ?1", [feed_id], |r| {
                r.get(0)
            })
            .optional()?;
        match state.as_deref() {
            None => return Err(Error::UnknownFeed(feed_id)),
            Some("active") => {}
            Some(_) => return Err(Error::FeedBuried(feed_id)),
        }
        tx.execute(
            "UPDATE feed SET cadence_override = ?2 WHERE id = ?1",
            params![feed_id, group.map(CadenceGroup::as_str)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// A feed's rhythm, diagnosed or not (computed `Silent` without a diagnosis).
    pub fn cadence_row(&self, feed_id: FeedId) -> Result<CadenceRow> {
        let row = self
            .conn()
            .query_row(
                "SELECT d.cadence_override, h.cadence_group FROM feed d \
                 LEFT JOIN feed_health h ON h.feed_id = d.id WHERE d.id = ?1",
                [feed_id],
                |r| Ok((group_at(r, 0)?, group_at(r, 1)?)),
            )
            .optional()?;
        let Some((forced, computed)) = row else {
            return Err(Error::UnknownFeed(feed_id));
        };
        let computed = computed.unwrap_or_default();
        Ok(CadenceRow {
            feed_id,
            effective_group: forced.unwrap_or(computed),
            computed_group: computed,
            is_override: forced.is_some(),
        })
    }

    pub fn folders(&self) -> Result<Vec<FolderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, parent_id, name, position FROM folder ORDER BY parent_id, position",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(FolderRow {
                    id: r.get(0)?,
                    parent_id: r.get(1)?,
                    name: r.get(2)?,
                    position: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Active feeds of a folder and of its sub-folders, recursively, in id order. An
    /// unknown folder has none.
    pub fn feeds_in_folder(&self, folder_id: FolderId) -> Result<Vec<FeedId>> {
        let mut stmt = self.conn().prepare(
            "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION SELECT f.id FROM folder f JOIN sub ON f.parent_id = sub.id) \
             SELECT id FROM feed WHERE state = 'active' AND folder_id IN (SELECT id FROM sub) ORDER BY id",
        )?;
        let ids = stmt
            .query_map([folder_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    /// Folder id → "Parent / Child" path, from the `v_folder_path` contract view.
    pub fn folder_paths(&self) -> Result<HashMap<FolderId, String>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT folder_id, path FROM v_folder_path")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }
}

/// URL keys of every feed in the database, from both its current and original address.
fn known_keys(tx: &Connection) -> Result<HashSet<String>> {
    let mut stmt = tx.prepare("SELECT url_key, xml_url_original FROM feed")?;
    let mut keys = HashSet::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, original) = row?;
        keys.insert(key);
        keys.insert(url_key(&original));
    }
    Ok(keys)
}

/// First free position in a folder (or at the root), after its feeds and sub-folders.
fn next_position(tx: &Connection, parent: Option<FolderId>) -> Result<i64> {
    let max: Option<i64> = tx.query_row(
        "SELECT max(position) FROM (SELECT position FROM feed WHERE folder_id IS ?1 \
         UNION ALL SELECT position FROM folder WHERE parent_id IS ?1)",
        [parent],
        |r| r.get(0),
    )?;
    Ok(max.map_or(0, |m| m + 1))
}

struct ImportCtx<'a> {
    tx: &'a Connection,
    now: &'a str,
    /// Keys present before the import started.
    known: &'a HashSet<String>,
    /// Keys added by this import → title kept, for duplicates inside the file.
    seen: &'a mut HashMap<String, String>,
    report: &'a mut ImportReport,
}

fn import_level(
    ctx: &mut ImportCtx<'_>,
    items: &[Outline],
    parent: Option<FolderId>,
) -> Result<()> {
    let tx = ctx.tx;
    for (index, item) in items.iter().enumerate() {
        // The file order, pushed past anything already in the folder (a re-import appends).
        let position = || -> Result<i64> { Ok(next_position(tx, parent)?.max(index as i64)) };
        match item {
            Outline::Folder { name, children } => {
                let existing: Option<FolderId> = tx
                    .query_row(
                        "SELECT id FROM folder WHERE parent_id IS ?1 AND name = ?2",
                        params![parent, name],
                        |r| r.get(0),
                    )
                    .optional()?;
                let id = match existing {
                    Some(id) => id,
                    None => {
                        tx.execute(
                            "INSERT INTO folder (parent_id, name, position) VALUES (?1, ?2, ?3)",
                            params![parent, name, position()?],
                        )?;
                        ctx.report.folders_added += 1;
                        tx.last_insert_rowid()
                    }
                };
                import_level(ctx, children, Some(id))?;
            }
            Outline::Feed(feed) => {
                let key = url_key(&feed.xml_url);
                if let Some(kept) = ctx.seen.get(&key) {
                    ctx.report.merged.push((feed.title.clone(), kept.clone()));
                    continue;
                }
                if ctx.known.contains(&key) {
                    ctx.report.skipped_existing += 1;
                    continue;
                }
                let original = feed
                    .attrs
                    .get("xmlUrl")
                    .cloned()
                    .unwrap_or_else(|| feed.xml_url.clone());
                tx.execute(
                    "INSERT INTO feed (folder_id, position, title, xml_url, xml_url_original, html_url, url_key, \
                     description_original, opml_attrs, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        parent,
                        position()?,
                        feed.title,
                        feed.xml_url,
                        original,
                        feed.html_url,
                        key,
                        feed.description,
                        serde_json::to_string(&feed.attrs)?,
                        ctx.now
                    ],
                )?;
                ctx.seen.insert(key, feed.title.clone());
                ctx.report.feeds_added += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Cadence, CadenceGroup, Decision, Diagnosis, Evidence, Status};
    use crate::opml;

    const FIXTURE: &str = include_str!("../../../../tests/fixtures/vienna-like.opml");

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn imported() -> (Store, ImportReport) {
        let store = Store::open_in_memory().unwrap();
        let report = store
            .import_outlines(&opml::parse(FIXTURE).unwrap(), false, now())
            .unwrap();
        (store, report)
    }

    #[test]
    fn imports_folders_and_feeds_merging_exact_duplicates() {
        let (store, report) = imported();
        assert_eq!(report.folders_added, 7);
        assert_eq!(report.feeds_added, 11);
        assert_eq!(
            report.merged,
            vec![(
                "Daring Fireball (1)".to_string(),
                "Daring Fireball".to_string()
            )]
        );
        assert_eq!(report.skipped_existing, 0);
        assert_eq!(store.feeds().unwrap().len(), 11);
        assert_eq!(store.feed_count().unwrap(), 11);
    }

    #[test]
    fn reimport_is_idempotent() {
        let (store, _) = imported();
        let again = store
            .import_outlines(&opml::parse(FIXTURE).unwrap(), false, now())
            .unwrap();
        assert_eq!(
            (
                again.folders_added,
                again.feeds_added,
                again.skipped_existing
            ),
            (0, 0, 12)
        );
        assert_eq!(store.feeds().unwrap().len(), 11);
    }

    #[test]
    fn dry_run_writes_nothing_but_reports_the_same() {
        let store = Store::open_in_memory().unwrap();
        let report = store
            .import_outlines(&opml::parse(FIXTURE).unwrap(), true, now())
            .unwrap();
        assert_eq!(report.feeds_added, 11);
        assert!(store.feeds().unwrap().is_empty());
        assert!(store.folders().unwrap().is_empty());
    }

    #[test]
    fn keeps_homonyms_and_folder_paths() {
        let (store, _) = imported();
        let feeds = store.feeds().unwrap();
        let cht: Vec<_> = feeds
            .iter()
            .filter(|f| f.title.starts_with("Center for Humane"))
            .collect();
        assert_eq!(cht.len(), 2);
        let paths = store.folder_paths().unwrap();
        let yt = cht.iter().find(|f| f.xml_url.contains("youtube")).unwrap();
        assert_eq!(paths[&yt.folder_id.unwrap()], "Jon / YouTube");
    }

    /// Export without the `dateCreated` line, which carries the current time.
    fn export_body(store: &Store, cemetery: bool) -> String {
        opml::export(store, cemetery)
            .unwrap()
            .lines()
            .filter(|l| !l.contains("<dateCreated>"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn reimport_after_triage_changes_nothing() {
        let (store, _) = imported();
        let feeds = store.feeds().unwrap();
        let (buried, repaired) = (feeds[0].id, feeds[1].id);
        store
            .record_decision(buried, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        let moved = Action::FollowRedirect {
            url: "https://moved.example.net/new-feed".into(),
        };
        store
            .record_decision(
                repaired,
                Decision::Repair,
                Status::Moved,
                Some(&moved),
                now(),
            )
            .unwrap();
        let (feeds_before, export_before, cemetery_before) = (
            store.feeds().unwrap(),
            export_body(&store, false),
            export_body(&store, true),
        );
        let again = store
            .import_outlines(&opml::parse(FIXTURE).unwrap(), false, now())
            .unwrap();
        assert_eq!((again.folders_added, again.feeds_added), (0, 0));
        assert_eq!(
            store.feeds().unwrap(),
            feeds_before,
            "states and URLs unchanged"
        );
        assert_eq!(export_body(&store, false), export_before);
        assert_eq!(export_body(&store, true), cemetery_before);
    }

    #[test]
    fn reimport_appends_new_feeds_after_existing_ones() {
        let store = Store::open_in_memory().unwrap();
        let first = r#"<opml><body><outline text="Blogs">
            <outline text="X" xmlUrl="https://x.example.org/feed"/>
            <outline text="Y" xmlUrl="https://y.example.org/feed"/>
            </outline></body></opml>"#;
        store
            .import_outlines(&opml::parse(first).unwrap(), false, now())
            .unwrap();
        let second = r#"<opml><body><outline text="Blogs">
            <outline text="Z" xmlUrl="https://z.example.org/feed"/>
            <outline text="Sub"><outline text="W" xmlUrl="https://w.example.org/feed"/></outline>
            </outline></body></opml>"#;
        let report = store
            .import_outlines(&opml::parse(second).unwrap(), false, now())
            .unwrap();
        assert_eq!((report.folders_added, report.feeds_added), (1, 2));
        let positions: Vec<(String, i64)> = store
            .feeds()
            .unwrap()
            .into_iter()
            .filter(|f| f.title != "W")
            .map(|f| (f.title, f.position))
            .collect();
        assert_eq!(
            positions,
            [("X".into(), 0), ("Y".into(), 1), ("Z".into(), 2)]
        );
        let sub = store
            .folders()
            .unwrap()
            .into_iter()
            .find(|f| f.name == "Sub")
            .unwrap();
        assert_eq!(sub.position, 3, "a new sub-folder goes after them too");
        let body = export_body(&store, false);
        let order: Vec<usize> = ["\"X\"", "\"Y\"", "\"Z\"", "\"Sub\""]
            .iter()
            .map(|t| body.find(t).unwrap())
            .collect();
        assert!(order.is_sorted(), "{body}");
    }

    #[test]
    fn stores_original_url_and_attributes() {
        let (store, _) = imported();
        let flickr = store
            .feeds()
            .unwrap()
            .into_iter()
            .find(|f| f.title.starts_with("Photos"))
            .unwrap();
        assert!(flickr.xml_url.contains("&lang=en-us"));
        assert!(flickr.xml_url_original.contains("&amp;lang"));
        assert_eq!(flickr.state, "active");
        assert!(flickr.opml_attrs.contains("\"type\":\"rss\""));
        assert_eq!(store.feed(flickr.id).unwrap().title, flickr.title);
    }

    #[test]
    fn feeds_in_folder_is_recursive_and_skips_buried_feeds() {
        let store = Store::open_in_memory().unwrap();
        let xml = r#"<opml><body>
            <outline text="Top"><outline text="A" xmlUrl="https://a.example.org/feed"/>
              <outline text="Sub"><outline text="B" xmlUrl="https://b.example.org/feed"/>
                <outline text="C" xmlUrl="https://c.example.org/feed"/></outline></outline>
            <outline text="Other"><outline text="D" xmlUrl="https://d.example.org/feed"/></outline>
            </body></opml>"#;
        store
            .import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        let folder = |name: &str| {
            store
                .folders()
                .unwrap()
                .into_iter()
                .find(|f| f.name == name)
                .unwrap()
                .id
        };
        let feed = |title: &str| {
            store
                .feeds()
                .unwrap()
                .into_iter()
                .find(|f| f.title == title)
                .unwrap()
                .id
        };
        store
            .record_decision(feed("C"), Decision::Bury, Status::Dead, None, now())
            .unwrap();
        assert_eq!(
            store.feeds_in_folder(folder("Top")).unwrap(),
            [feed("A"), feed("B")]
        );
        assert_eq!(store.feeds_in_folder(folder("Sub")).unwrap(), [feed("B")]);
        assert_eq!(store.feeds_in_folder(folder("Other")).unwrap(), [feed("D")]);
        assert!(store.feeds_in_folder(999).unwrap().is_empty());
    }

    fn pages(store: &Store) -> Vec<(String, Option<u16>)> {
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT name, page FROM folder UNION ALL SELECT title, page FROM feed ORDER BY 2, 1",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn import_numbers_new_folders_and_feeds_and_keeps_earlier_numbers() {
        let store = Store::open_in_memory().unwrap();
        let first = r#"<opml><body><outline text="Blogs">
            <outline text="X" xmlUrl="https://x.example.org/feed"/>
            <outline text="Y" xmlUrl="https://y.example.org/feed"/>
            </outline><outline text="Solo" xmlUrl="https://solo.example.org/feed"/></body></opml>"#;
        store
            .import_outlines(&opml::parse(first).unwrap(), false, now())
            .unwrap();
        let x = store.feeds().unwrap()[0].id;
        store
            .record_decision(x, Decision::Bury, Status::Dead, None, now())
            .unwrap();
        let second = r#"<opml><body><outline text="Blogs">
            <outline text="Z" xmlUrl="https://z.example.org/feed"/>
            <outline text="Sub"><outline text="W" xmlUrl="https://w.example.org/feed"/></outline>
            </outline></body></opml>"#;
        store
            .import_outlines(&opml::parse(second).unwrap(), false, now())
            .unwrap();
        let expected: Vec<(String, Option<u16>)> = [
            ("Solo", 110),
            ("Blogs", 200),
            ("X", 201),
            ("Y", 202),
            ("Z", 203),
            ("Sub", 204),
            ("W", 205),
        ]
        .iter()
        .map(|(t, p)| (t.to_string(), Some(*p)))
        .collect();
        assert_eq!(pages(&store), expected, "the buried X keeps 201");
    }

    #[test]
    fn a_dry_run_numbers_nothing() {
        let (store, _) = imported();
        let extra = r#"<opml><body><outline text="New" xmlUrl="https://new.example.org/feed"/></body></opml>"#;
        let before = pages(&store);
        store
            .import_outlines(&opml::parse(extra).unwrap(), true, now())
            .unwrap();
        assert_eq!(pages(&store), before);
    }

    fn diagnosed(store: &Store, feed_id: FeedId, group: CadenceGroup) {
        let d = Diagnosis {
            status: Status::Alive,
            reason: "r".into(),
            evidence: Evidence::default(),
            remarks: vec![],
            action: None,
            hijack_score: 0.0,
            cadence: Cadence {
                group,
                ..Cadence::default()
            },
        };
        store
            .save_diagnoses(&HashMap::from([(feed_id, d)]), now())
            .unwrap();
    }

    #[test]
    fn an_override_wins_until_cleared_and_survives_a_new_diagnosis() {
        let (store, _) = imported();
        diagnosed(&store, 1, CadenceGroup::Weekly);
        store
            .set_cadence_override(1, Some(CadenceGroup::Daily))
            .unwrap();
        assert_eq!(
            store.cadence_row(1).unwrap(),
            CadenceRow {
                feed_id: 1,
                effective_group: CadenceGroup::Daily,
                computed_group: CadenceGroup::Weekly,
                is_override: true,
            }
        );
        // The next refresh reclassifies the feed; the override is never cleared on its own.
        diagnosed(&store, 1, CadenceGroup::Rare);
        let row = store.cadence_row(1).unwrap();
        assert_eq!(
            (row.effective_group, row.computed_group, row.is_override),
            (CadenceGroup::Daily, CadenceGroup::Rare, true)
        );
        let effective: String = store
            .conn()
            .query_row(
                "SELECT effective_group FROM v_cadence WHERE feed_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(effective, "daily");
        store.set_cadence_override(1, None).unwrap();
        assert_eq!(
            store.cadence_row(1).unwrap(),
            CadenceRow {
                feed_id: 1,
                effective_group: CadenceGroup::Rare,
                computed_group: CadenceGroup::Rare,
                is_override: false,
            }
        );
    }

    #[test]
    fn a_feed_without_a_diagnosis_reads_as_silent() {
        let (store, _) = imported();
        assert_eq!(
            store.cadence_row(2).unwrap(),
            CadenceRow {
                feed_id: 2,
                effective_group: CadenceGroup::Silent,
                computed_group: CadenceGroup::Silent,
                is_override: false,
            }
        );
        store
            .set_cadence_override(2, Some(CadenceGroup::Rare))
            .unwrap();
        assert_eq!(
            store.cadence_row(2).unwrap().effective_group,
            CadenceGroup::Rare
        );
    }

    #[test]
    fn the_override_refuses_an_unknown_or_buried_feed_and_writes_nothing() {
        let (store, _) = imported();
        let err = store
            .set_cadence_override(999, Some(CadenceGroup::Daily))
            .unwrap_err();
        assert!(matches!(err, Error::UnknownFeed(999)), "{err:?}");
        assert_eq!(err.to_string(), "No feed #999.");
        assert!(matches!(
            store.cadence_row(999).unwrap_err(),
            Error::UnknownFeed(999)
        ));
        store
            .conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 3", [])
            .unwrap();
        let err = store
            .set_cadence_override(3, Some(CadenceGroup::Daily))
            .unwrap_err();
        assert!(matches!(err, Error::FeedBuried(3)), "{err:?}");
        assert_eq!(err.to_string(), "Feed #3 is buried.");
        assert!(
            !store.cadence_row(3).unwrap().is_override,
            "nothing written"
        );
    }

    /// A folder named "Folder <id>".
    fn folder(s: &Store, id: i64, parent: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO folder (id, parent_id, name, position) VALUES (?1, ?2, ?3, ?4)",
                params![id, parent, format!("Folder {id}"), position],
            )
            .unwrap();
    }

    /// A feed titled "Feed <id>".
    fn feed(s: &Store, id: i64, folder: Option<i64>, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, folder_id, position, title, xml_url, xml_url_original, url_key, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, '2026-10-14T08:00:00.000Z')",
                params![
                    id,
                    folder,
                    position,
                    format!("Feed {id}"),
                    format!("https://f{id}.example.org/feed"),
                    format!("f{id}.example.org/feed")
                ],
            )
            .unwrap();
    }

    fn folder_page(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM folder WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn page_of(s: &Store, id: i64) -> Option<u16> {
        s.conn()
            .query_row("SELECT page FROM feed WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    /// Folder 1 (200) holding feed 10 (201) and subfolder 2 (202); root feed 20 (110).
    fn numbered() -> Store {
        let s = Store::open_in_memory().unwrap();
        folder(&s, 1, None, 0);
        feed(&s, 10, Some(1), 0);
        folder(&s, 2, Some(1), 1);
        feed(&s, 20, None, 0);
        pages::assign_missing(s.conn()).unwrap();
        s
    }

    #[test]
    fn move_to_a_folder_takes_its_range() {
        let s = numbered();
        let moved = s.move_feed(20, Some(1)).unwrap();
        assert_eq!(
            moved,
            Moved {
                feed: 20,
                title: "Feed 20".into(),
                from: None,
                to: Some(1),
                old_page: Some(110),
                new_page: Some(203),
                changed: true,
            }
        );
        let row = s.feed(20).unwrap();
        assert_eq!((row.folder_id, page_of(&s, 20)), (Some(1), Some(203)));
        assert_eq!(row.position, 2, "last after feed 10 (0) and folder 2 (1)");
    }

    #[test]
    fn move_to_a_subfolder_uses_the_root_folders_range() {
        let s = numbered();
        let moved = s.move_feed(20, Some(2)).unwrap();
        assert_eq!(moved.new_page, Some(203));
        assert_eq!(s.feed(20).unwrap().folder_id, Some(2));
        folder(&s, 3, Some(2), 1);
        pages::assign_missing(s.conn()).unwrap(); // the sub-subfolder takes 204
        feed(&s, 21, None, 1);
        pages::assign_missing(s.conn()).unwrap(); // 111
        assert_eq!(s.move_feed(21, Some(3)).unwrap().new_page, Some(205));
    }

    #[test]
    fn move_to_the_root_takes_110_to_199() {
        let s = numbered();
        let moved = s.move_feed(10, None).unwrap();
        assert_eq!(
            (moved.from, moved.to, moved.old_page, moved.new_page),
            (Some(1), None, Some(201), Some(111))
        );
        let row = s.feed(10).unwrap();
        assert_eq!((row.folder_id, row.position), (None, 1), "after feed 20");
        // The vacated page is free again.
        feed(&s, 11, Some(1), 5);
        pages::assign_missing(s.conn()).unwrap();
        assert_eq!(page_of(&s, 11), Some(201));
    }

    #[test]
    fn full_range_keeps_the_page() {
        let s = numbered();
        for id in 100..189 {
            feed(&s, id, None, id); // with feed 20 and 10's future seat: 110–199 is 90 pages
        }
        pages::assign_missing(s.conn()).unwrap();
        assert_eq!(page_of(&s, 188), Some(199), "the root range is full");
        let moved = s.move_feed(10, None).unwrap();
        assert_eq!(
            (moved.old_page, moved.new_page, moved.changed),
            (Some(201), Some(201), true)
        );
        assert_eq!(s.feed(10).unwrap().folder_id, None);
        assert_eq!(page_of(&s, 10), Some(201));
    }

    #[test]
    fn move_is_a_no_op_when_already_there() {
        let s = numbered();
        let moved = s.move_feed(10, Some(1)).unwrap();
        assert!(!moved.changed);
        assert_eq!((moved.old_page, moved.new_page), (Some(201), Some(201)));
        let row = s.feed(10).unwrap();
        assert_eq!(
            (row.folder_id, row.position, page_of(&s, row.id)),
            (Some(1), 0, Some(201))
        );
        assert!(!s.move_feed(20, None).unwrap().changed, "the root too");
    }

    #[test]
    fn unknown_feed_or_folder_refused() {
        let s = numbered();
        let err = s.move_feed(99, Some(1)).unwrap_err();
        assert!(matches!(err, Error::UnknownFeed(99)), "{err:?}");
        let err = s.move_feed(20, Some(99)).unwrap_err();
        assert!(matches!(err, Error::UnknownFolder(99)), "{err:?}");
        assert_eq!(err.to_string(), "No folder #99.");
        let err = s.move_feed(10, Some(99)).unwrap_err();
        assert!(
            matches!(err, Error::UnknownFolder(99)),
            "refused even from where it is"
        );
        let row = s.feed(20).unwrap();
        assert_eq!(
            (row.folder_id, row.position, page_of(&s, row.id)),
            (None, 0, Some(110))
        );
    }

    #[test]
    fn buried_feed_can_move() {
        let s = numbered();
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 20", [])
            .unwrap();
        let moved = s.move_feed(20, Some(1)).unwrap();
        assert_eq!(moved.new_page, Some(203));
        assert_eq!(s.feed(20).unwrap().state, "buried");
    }

    #[test]
    fn a_root_folder_without_a_hundred_uses_the_overflow_pool() {
        let s = Store::open_in_memory().unwrap();
        for i in 1..=7 {
            folder(&s, i, None, i);
        }
        feed(&s, 20, None, 8);
        feed(&s, 21, Some(7), 0);
        pages::assign_missing(s.conn()).unwrap();
        assert_eq!(page_of(&s, 20), Some(110));
        assert_eq!(folder_page(&s, 7), Some(111), "folder 7 has no hundred");
        assert_eq!(page_of(&s, 21), Some(112), "nor do its contents");
        let moved = s.move_feed(20, Some(7)).unwrap();
        assert_eq!(
            (moved.old_page, moved.new_page),
            (Some(110), Some(110)),
            "its own page is the lowest free once it leaves"
        );
        feed(&s, 22, None, 9);
        pages::assign_missing(s.conn()).unwrap();
        assert_eq!(page_of(&s, 22), Some(113), "112 is held");
        assert_eq!(
            s.move_feed(22, Some(7)).unwrap().new_page,
            Some(113),
            "the pool, not 110-199 or a hundred"
        );
    }

    #[test]
    fn a_page_already_in_the_new_range_is_kept() {
        let s = numbered();
        feed(&s, 11, Some(2), 0);
        pages::assign_missing(s.conn()).unwrap(); // 203
        let moved = s.move_feed(11, Some(1)).unwrap();
        assert_eq!((moved.old_page, moved.new_page), (Some(203), Some(203)));
    }

    fn new_feed(xml: &str, html: Option<&str>, title: &str) -> NewFeed {
        NewFeed {
            xml_url: xml.to_owned(),
            html_url: html.map(str::to_owned),
            title: title.to_owned(),
        }
    }

    fn add_themes(s: &Store) {
        for (id, key) in [(1, "tech"), (2, "arts")] {
            s.conn()
                .execute(
                    "INSERT INTO theme (id, key, name, sea_name, position) VALUES (?1, ?2, ?3, ?4, ?1)",
                    params![id, key, key.to_uppercase(), format!("{key} sea")],
                )
                .unwrap();
        }
    }

    /// A root feed on `host`, themed `theme_id`.
    fn site_feed(s: &Store, id: i64, host: &str, theme_id: Option<i64>) {
        s.conn()
            .execute(
                "INSERT INTO feed (id, position, title, xml_url, xml_url_original, html_url, url_key, theme_id, added_at) \
                 VALUES (?1, ?1, ?2, ?3, ?3, ?4, ?5, ?6, '2026-10-14T08:00:00.000Z')",
                params![
                    id,
                    format!("Feed {id}"),
                    format!("https://{host}/feed{id}.xml"),
                    format!("https://{host}/"),
                    format!("{host}/feed{id}.xml"),
                    theme_id
                ],
            )
            .unwrap();
    }

    fn territory(s: &Store, id: i64, kind: &str, name: &str, members: &[i64]) {
        let theme = (kind == "continent").then_some(1);
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind, theme_id) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, kind, theme],
            )
            .unwrap();
        for (position, feed) in members.iter().enumerate() {
            s.conn()
                .execute(
                    "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, ?3)",
                    params![id, feed, position as i64],
                )
                .unwrap();
        }
    }

    fn members(s: &Store, territory: i64) -> Vec<i64> {
        let mut stmt = s
            .conn()
            .prepare("SELECT feed_id FROM territory_feed WHERE territory_id = ?1 ORDER BY position")
            .unwrap();
        stmt.query_map([territory], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn duplicate_by_xml_url_and_original() {
        let s = Store::open_in_memory().unwrap();
        site_feed(&s, 1, "example.org", None);
        s.conn()
            .execute(
                "UPDATE feed SET xml_url = 'https://new.example.org/rss', url_key = 'new.example.org/rss' WHERE id = 1",
                [],
            )
            .unwrap();
        let by_current = s
            .find_duplicate(&["http://www.new.example.org/rss/"])
            .unwrap();
        assert_eq!(by_current, Some((1, "Feed 1".to_owned(), false)));
        let by_original = s
            .find_duplicate(&["https://elsewhere.net/x", "https://example.org/feed1.xml"])
            .unwrap();
        assert_eq!(by_original, Some((1, "Feed 1".to_owned(), false)));
        assert_eq!(
            s.find_duplicate(&["https://example.org/other"]).unwrap(),
            None
        );
    }

    #[test]
    fn duplicate_buried_reported() {
        let s = Store::open_in_memory().unwrap();
        site_feed(&s, 1, "example.org", None);
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            s.find_duplicate(&["https://example.org/feed1.xml"])
                .unwrap(),
            Some((1, "Feed 1".to_owned(), true))
        );
    }

    #[test]
    fn insert_refuses_an_address_already_followed_inside_its_own_transaction() {
        let s = Store::open_in_memory().unwrap();
        site_feed(&s, 1, "example.org", None);
        let err = s
            .insert_added_feed(
                &new_feed("https://EXAMPLE.org/feed1.xml/", None, "Again"),
                None,
                now(),
            )
            .unwrap_err();
        assert!(
            matches!(
                &err,
                Error::FeedExists {
                    id: 1,
                    buried: false,
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "Already followed: Feed 1.");
        assert_eq!(s.feed_count().unwrap(), 1, "nothing is written");
    }

    #[test]
    fn inserted_at_the_bottom_of_root_with_a_root_page() {
        let s = numbered();
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://fresh.example.net/rss",
                    Some("https://fresh.example.net/"),
                    "Fresh",
                ),
                None,
                now(),
            )
            .unwrap();
        let row = s.feed(got.id).unwrap();
        assert_eq!(row.folder_id, None);
        assert_eq!(row.title, "Fresh");
        assert_eq!(row.xml_url, "https://fresh.example.net/rss");
        assert_eq!(row.state, "active");
        assert_eq!(got.page, Some(111), "feed 20 holds 110");
        assert_eq!(page_of(&s, got.id), Some(111));
        assert_eq!(got.theme_source, ThemeSource::None);
        let max_other: i64 = s
            .conn()
            .query_row(
                "SELECT max(position) FROM (SELECT position FROM feed WHERE folder_id IS NULL AND id <> ?1 \
                 UNION ALL SELECT position FROM folder WHERE parent_id IS NULL)",
                [got.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(row.position > max_other);
    }

    #[test]
    fn an_empty_title_falls_back_to_the_host() {
        let s = Store::open_in_memory().unwrap();
        let got = s
            .insert_added_feed(
                &new_feed("https://www.example.org/rss", None, "  "),
                None,
                now(),
            )
            .unwrap();
        assert_eq!(got.title, "example.org");
    }

    #[test]
    fn chosen_theme_wins() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        site_feed(&s, 1, "example.org", Some(1));
        territory(&s, 1, "territory", "T", &[1]);
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://example.org/new.xml",
                    Some("https://example.org/"),
                    "New",
                ),
                Some("arts"),
                now(),
            )
            .unwrap();
        assert_eq!(got.theme_source, ThemeSource::Chosen);
        assert_eq!(got.theme.unwrap().key, "arts");
        assert_eq!(got.territory, None);
        assert_eq!(members(&s, 1), vec![1]);
    }

    #[test]
    fn unknown_theme_refused_before_writing() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        let err = s
            .insert_added_feed(
                &new_feed("https://example.org/x", None, "X"),
                Some("nope"),
                now(),
            )
            .unwrap_err();
        assert!(matches!(err, Error::UnknownTheme(k) if k == "nope"));
        assert_eq!(s.feed_count().unwrap(), 0);
    }

    #[test]
    fn majority_sibling_theme_tie_lowest_id() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        site_feed(&s, 1, "example.org", Some(2));
        site_feed(&s, 2, "www.example.org", Some(1));
        site_feed(&s, 3, "example.org", Some(1));
        site_feed(&s, 4, "blog.example.org", Some(2));
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://example.org/new.xml",
                    Some("https://example.org/"),
                    "New",
                ),
                None,
                now(),
            )
            .unwrap();
        // tech has 2 votes (feeds 2, 3) against arts' 1 (feed 4 is another host).
        assert_eq!(got.theme.as_ref().unwrap().key, "tech");
        assert_eq!(
            got.theme_source,
            ThemeSource::Sibling {
                id: 2,
                title: "Feed 2".to_owned()
            }
        );
    }

    #[test]
    fn a_tie_goes_to_the_lowest_feed_id_theme() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        site_feed(&s, 1, "example.org", Some(2));
        site_feed(&s, 2, "example.org", Some(1));
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://example.org/new.xml",
                    Some("https://example.org/"),
                    "New",
                ),
                None,
                now(),
            )
            .unwrap();
        assert_eq!(got.theme.unwrap().key, "arts");
        assert_eq!(
            got.theme_source,
            ThemeSource::Sibling {
                id: 1,
                title: "Feed 1".to_owned()
            }
        );
    }

    #[test]
    fn joins_lowest_territory_never_continent() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        site_feed(&s, 1, "example.org", Some(1));
        site_feed(&s, 2, "example.org", Some(1));
        territory(&s, 1, "continent", "CONTINENT", &[1]);
        territory(&s, 3, "territory", "HIGH", &[1]);
        territory(&s, 2, "territory", "LOW", &[2]);
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://example.org/new.xml",
                    Some("https://example.org/"),
                    "New",
                ),
                None,
                now(),
            )
            .unwrap();
        assert_eq!(got.territory.as_deref(), Some("LOW"));
        assert_eq!(members(&s, 2), vec![2, got.id]);
        assert_eq!(members(&s, 3), vec![1]);
        assert_eq!(members(&s, 1), vec![1], "never the continent");
    }

    #[test]
    fn no_sibling_no_theme() {
        let s = Store::open_in_memory().unwrap();
        add_themes(&s);
        site_feed(&s, 1, "other.org", Some(1));
        site_feed(&s, 2, "blog.example.org", Some(1));
        let got = s
            .insert_added_feed(
                &new_feed(
                    "https://example.org/new.xml",
                    Some("https://example.org/"),
                    "New",
                ),
                None,
                now(),
            )
            .unwrap();
        assert_eq!(got.theme, None);
        assert_eq!(got.theme_source, ThemeSource::None);
        assert_eq!(got.territory, None);
        let stored: Option<i64> = s
            .conn()
            .query_row("SELECT theme_id FROM feed WHERE id = ?1", [got.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored, None);
    }
}
