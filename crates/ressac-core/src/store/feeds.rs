//! Folders and feeds: OPML import and read access.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::Store;
use crate::error::Result;
use crate::model::{FeedId, FolderId, ts};
use crate::opml::Outline;
use crate::url_key::url_key;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub folders_added: usize,
    pub feeds_added: usize,
    /// (dropped title, kept title) for exact duplicates inside the imported file.
    pub merged: Vec<(String, String)>,
    /// Feeds whose URL key already exists among active feeds in the database.
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

impl Store {
    /// Imports folders and feeds. Folders are matched by (parent, name); feeds by URL key
    /// among active feeds. A dry run executes the same statements and rolls them back.
    pub fn import_outlines(
        &self,
        outlines: &[Outline],
        dry_run: bool,
        now: DateTime<Utc>,
    ) -> Result<ImportReport> {
        let tx = self.conn().unchecked_transaction()?;
        let mut report = ImportReport::default();
        let mut seen: HashMap<String, String> = HashMap::new();
        import_level(&tx, outlines, None, &ts(now), &mut seen, &mut report)?;
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

    pub fn feed(&self, id: FeedId) -> Result<FeedRow> {
        Ok(self.conn().query_row(
            &format!("SELECT {FEED_COLUMNS} FROM feed WHERE id = ?1"),
            [id],
            feed_row,
        )?)
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

fn import_level(
    tx: &Connection,
    items: &[Outline],
    parent: Option<FolderId>,
    now: &str,
    seen: &mut HashMap<String, String>,
    report: &mut ImportReport,
) -> Result<()> {
    for (position, item) in items.iter().enumerate() {
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
                            params![parent, name, position as i64],
                        )?;
                        report.folders_added += 1;
                        tx.last_insert_rowid()
                    }
                };
                import_level(tx, children, Some(id), now, seen, report)?;
            }
            Outline::Feed(feed) => {
                let key = url_key(&feed.xml_url);
                if let Some(kept) = seen.get(&key) {
                    report.merged.push((feed.title.clone(), kept.clone()));
                    continue;
                }
                let exists: i64 = tx.query_row(
                    "SELECT count(*) FROM feed WHERE url_key = ?1 AND state = 'active'",
                    [&key],
                    |r| r.get(0),
                )?;
                if exists > 0 {
                    report.skipped_existing += 1;
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
                        position as i64,
                        feed.title,
                        feed.xml_url,
                        original,
                        feed.html_url,
                        key,
                        feed.description,
                        serde_json::to_string(&feed.attrs)?,
                        now
                    ],
                )?;
                seen.insert(key, feed.title.clone());
                report.feeds_added += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
