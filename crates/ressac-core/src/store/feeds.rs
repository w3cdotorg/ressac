//! Folders and feeds: OPML import and read access.

use std::collections::{HashMap, HashSet};

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
    /// Imports folders and feeds. Folders are matched by (parent, name). A feed already
    /// exists when any feed, buried or not, has its URL key as current or original address,
    /// so triage (burials, merges, repairs) survives a re-import. New items go after the
    /// existing ones of their folder. A dry run executes the same statements and rolls them back.
    pub fn import_outlines(
        &self,
        outlines: &[Outline],
        dry_run: bool,
        now: DateTime<Utc>,
    ) -> Result<ImportReport> {
        let tx = self.conn().unchecked_transaction()?;
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
    use crate::model::{Action, Decision, Status};
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
}
