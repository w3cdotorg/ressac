# Phase 3: OPML parsing and idempotent import

Read the plan index first. Global Constraints apply. quick-xml 0.42 notes: `notes.md` § "API des crates".

**Files (5):**
- Create: `tests/fixtures/vienna-like.opml`
- Create: `crates/ressac-core/src/opml.rs` (tests inline)
- Create: `crates/ressac-core/src/store/feeds.rs` (tests inline)
- Modify: `crates/ressac-core/src/store/mod.rs` (add `mod feeds; pub use feeds::*;`)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod opml;`)

### Task 3.1: Fixture and OPML parser

**Interfaces:**
- Consumes: `url_key::repair_url`, `error::{Error, Result}`.
- Produces (`ressac_core::opml`):
  - `enum Outline { Folder { name: String, children: Vec<Outline> }, Feed(OpmlFeed) }`
  - `struct OpmlFeed { title: String, xml_url: String, html_url: Option<String>, description: Option<String>, attrs: BTreeMap<String, String> }`
  - `parse(xml: &str) -> Result<Vec<Outline>>`
  - `clean_description(raw: &str) -> Option<String>`

- [ ] **Step 1: Create the fixture** `tests/fixtures/vienna-like.opml`. It is hand-written and reproduces the traps of the real Vienna export. Personal sites use `example.org`/`example.net`. Keep the blank lines and JavaScript in the "Girl on the Couch" description exactly.

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<opml version="1.0">
    <head>
        <title>Vienna Subscriptions</title>
        <dateCreated>2026-04-15 10:02:51 +0000</dateCreated>
    </head>
    <body>
        <outline text="Blogs">
            <outline htmlUrl="https://xkcd.com/" xmlUrl="https://www.xkcd.com/rss.xml" type="rss" description="xkcd.com: A webcomic of romance and math humor." text="xkcd.com"/>
            <outline text="potes">
                <outline htmlUrl="https://orange.example.org" xmlUrl="https://orange.example.org/feed/" type="rss" description="" text="Orange Drenka"/>
                <outline htmlUrl="http://orange-drenka.example.net/" xmlUrl="http://orange-drenka.example.net/feeds/posts/default?alt=rss" type="rss" description="" text="Orange-Drenka"/>
                <outline htmlUrl="https://www.flickr.com/photos/bouquiniste/" xmlUrl="http://api.flickr.com/services/feeds/photos_public.gne?id=48609153@N00&amp;amp;lang=en-us&amp;format=rss_200" type="rss" description="" text="Photos from bouquiniste"/>
            </outline>
            <outline htmlUrl="https://girl.example.org/" xmlUrl="http://girl.example.org/rss" type="rss" description="A book is a loaded gun in the house next door




var sc_project=6114696; 
var sc_invisible=1; 
var sc_security=&quot;618df32f&quot;; 



" text="Girl on the Couch"/>
        </outline>
        <outline text="Biblis">
            <outline htmlUrl="https://biblio.example.org/" xmlUrl="http://www.biblio.example.org/feed/" type="rss" description="Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày. Tham gia ngay để thử thách bản thân và nhận ngay phần thưởng cực hấp dẫn!" text="Bibliobsession 2.0"/>
            <outline htmlUrl="http://www.unphotographable.com/" xmlUrl="http://www.unphotographable.com/index.rdf" type="rss" description="" text="Unphotographable"/>
        </outline>
        <outline text="Infos">
            <outline htmlUrl="https://daringfireball.net/" xmlUrl="http://daringfireball.net/feeds/main" type="rss" description="By John Gruber" text="Daring Fireball"/>
            <outline htmlUrl="https://daringfireball.net/" xmlUrl="https://daringfireball.net/feeds/main" type="rss" description="By John Gruber" text="Daring Fireball (1)"/>
            <outline htmlUrl="https://sixcolors.com/?feed=json" xmlUrl="https://sixcolors.com/feed.json" type="rss" description="Apple, technology, and other stuff by Jason Snell &amp;amp; Dan Moren" text="Six Colors"/>
            <outline text="Science">
                <outline htmlUrl="https://cht.example.org" xmlUrl="https://cht.example.org/feed" type="rss" description="Ensuring that the most consequential technologies serve humanity" text="Center for Humane Technology"/>
            </outline>
        </outline>
        <outline text="Jon">
            <outline text="YouTube">
                <outline htmlUrl="https://www.youtube.com/channel/UCFECM-p3CF81Tp_l2sJsiyg" xmlUrl="https://www.youtube.com/feeds/videos.xml?channel_id=UCFECM-p3CF81Tp_l2sJsiyg" type="rss" description="" text="Center for Humane Technology (1)"/>
            </outline>
        </outline>
    </body>
</opml>
```

Expected shape:
- **Folders:** 7 (Blogs, potes, Biblis, Infos, Science, Jon, YouTube).
- **Feeds:** 12. Daring Fireball (1) is the same feed as Daring Fireball (http vs https), so the import keeps 11.

- [ ] **Step 2: Write the failing tests** in `crates/ressac-core/src/opml.rs`

```rust
//! OPML import (parsing) and export.

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/vienna-like.opml");

    fn feeds(items: &[Outline]) -> Vec<&OpmlFeed> {
        let mut out = Vec::new();
        for item in items {
            match item {
                Outline::Feed(f) => out.push(f),
                Outline::Folder { children, .. } => out.extend(feeds(children)),
            }
        }
        out
    }

    fn find<'a>(items: &'a [Outline], title: &str) -> &'a OpmlFeed {
        feeds(items).into_iter().find(|f| f.title == title).expect(title)
    }

    #[test]
    fn parses_nested_folders() {
        let items = parse(FIXTURE).unwrap();
        let names: Vec<&str> = items
            .iter()
            .map(|o| match o {
                Outline::Folder { name, .. } => name.as_str(),
                Outline::Feed(f) => f.title.as_str(),
            })
            .collect();
        assert_eq!(names, ["Blogs", "Biblis", "Infos", "Jon"]);
        let Outline::Folder { children, .. } = &items[0] else { panic!("Blogs is a folder") };
        assert_eq!(children.len(), 3);
        assert!(matches!(&children[1], Outline::Folder { name, children } if name == "potes" && children.len() == 3));
        assert_eq!(feeds(&items).len(), 12);
    }

    #[test]
    fn repairs_double_escaped_urls() {
        let items = parse(FIXTURE).unwrap();
        let flickr = find(&items, "Photos from bouquiniste");
        assert_eq!(
            flickr.xml_url,
            "http://api.flickr.com/services/feeds/photos_public.gne?id=48609153@N00&lang=en-us&format=rss_200"
        );
        assert!(flickr.attrs["xmlUrl"].contains("&amp;lang"), "original attribute kept as read");
    }

    #[test]
    fn keeps_homonyms_and_metadata() {
        let items = parse(FIXTURE).unwrap();
        let cht = find(&items, "Center for Humane Technology (1)");
        assert!(cht.xml_url.contains("youtube.com"));
        assert_eq!(find(&items, "xkcd.com").html_url.as_deref(), Some("https://xkcd.com/"));
        assert_eq!(find(&items, "Orange Drenka").description, None, "empty description is None");
    }

    #[test]
    fn cleans_javascript_polluted_descriptions() {
        let items = parse(FIXTURE).unwrap();
        let raw = find(&items, "Girl on the Couch").description.clone().unwrap();
        assert!(raw.contains("sc_project"));
        assert_eq!(clean_description(&raw).as_deref(), Some("A book is a loaded gun in the house next door"));
        assert_eq!(clean_description("<p>Hello <b>world</b></p>").as_deref(), Some("Hello world"));
        assert_eq!(clean_description("  var _gaq = _gaq || [];").as_deref(), None);
    }

    #[test]
    fn reports_position_of_malformed_opml() {
        let bad = "<opml>\n<body>\n<outline text=\"x\">\n</body></opml>";
        match parse(bad) {
            Err(Error::Opml { line, .. }) => assert!(line >= 3, "line {line}"),
            other => panic!("expected OPML error, got {other:?}"),
        }
        assert!(matches!(parse("<opml><body><outline text=\"x\">"), Err(Error::Opml { .. })));
    }
}
```

- [ ] **Step 3: Register** `pub mod opml;` in `lib.rs`, then run to see the tests fail.

Run: `cargo test -p ressac-core opml`
Expected: compile errors (`parse`, `Outline` not found).

- [ ] **Step 4: Implement.** Insert above the tests in `opml.rs`:

```rust
use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::error::{Error, Result};
use crate::url_key::repair_url;

#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    Folder { name: String, children: Vec<Outline> },
    Feed(OpmlFeed),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpmlFeed {
    pub title: String,
    /// Repaired URL (see `url_key::repair_url`).
    pub xml_url: String,
    pub html_url: Option<String>,
    /// Raw description, trimmed; `None` when empty.
    pub description: Option<String>,
    /// Every attribute of the outline as read, for a faithful export.
    pub attrs: BTreeMap<String, String>,
}

enum Frame {
    Root(Vec<Outline>),
    Folder(String, Vec<Outline>),
    /// Children of a feed outline (rare, ignored).
    Ignored,
}

/// Parses the `<body>` outlines of an OPML 1.0 or 2.0 document.
pub fn parse(xml: &str) -> Result<Vec<Outline>> {
    let mut reader = Reader::from_reader(xml.as_bytes());
    let mut buf = Vec::new();
    let mut stack = vec![Frame::Root(Vec::new())];
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| opml_error(xml, reader.error_position() as usize, &e.to_string()))?;
        let position = reader.buffer_position() as usize;
        match event {
            Event::Start(e) if e.name().0 == "outline" => {
                let attrs = attributes(&e).map_err(|m| opml_error(xml, position, &m))?;
                match feed_from(attrs) {
                    Ok(feed) => {
                        push(&mut stack, Outline::Feed(feed));
                        stack.push(Frame::Ignored);
                    }
                    Err(attrs) => stack.push(Frame::Folder(folder_name(&attrs), Vec::new())),
                }
            }
            Event::Empty(e) if e.name().0 == "outline" => {
                let attrs = attributes(&e).map_err(|m| opml_error(xml, position, &m))?;
                let item = match feed_from(attrs) {
                    Ok(feed) => Outline::Feed(feed),
                    Err(attrs) => Outline::Folder { name: folder_name(&attrs), children: Vec::new() },
                };
                push(&mut stack, item);
            }
            Event::End(e) if e.name().0 == "outline" => match stack.pop() {
                Some(Frame::Folder(name, children)) => push(&mut stack, Outline::Folder { name, children }),
                Some(Frame::Ignored) => {}
                _ => return Err(opml_error(xml, position, "unexpected </outline>")),
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    match (stack.pop(), stack.is_empty()) {
        (Some(Frame::Root(items)), true) => Ok(items),
        _ => Err(opml_error(xml, xml.len(), "unclosed <outline>")),
    }
}

fn push(stack: &mut [Frame], item: Outline) {
    match stack.last_mut() {
        Some(Frame::Root(items)) | Some(Frame::Folder(_, items)) => items.push(item),
        Some(Frame::Ignored) | None => {}
    }
}

fn attributes(e: &BytesStart) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|err| err.to_string())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| err.to_string())?;
        map.insert(attr.key.0.to_string(), value.into_owned());
    }
    Ok(map)
}

/// A feed outline has a non-empty `xmlUrl`; anything else is a folder (attributes handed back).
fn feed_from(attrs: BTreeMap<String, String>) -> std::result::Result<OpmlFeed, BTreeMap<String, String>> {
    if !attrs.get("xmlUrl").is_some_and(|u| !u.trim().is_empty()) {
        return Err(attrs);
    }
    let xml_url = repair_url(&attrs["xmlUrl"]);
    let title = attrs
        .get("text")
        .or_else(|| attrs.get("title"))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| xml_url.clone());
    let html_url = attrs.get("htmlUrl").map(|u| repair_url(u)).filter(|u| !u.is_empty());
    let description = attrs
        .get("description")
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    Ok(OpmlFeed { title, xml_url, html_url, description, attrs })
}

fn folder_name(attrs: &BTreeMap<String, String>) -> String {
    attrs
        .get("text")
        .or_else(|| attrs.get("title"))
        .cloned()
        .unwrap_or_else(|| "Untitled".to_string())
}

fn opml_error(xml: &str, position: usize, message: &str) -> Error {
    let bytes = &xml.as_bytes()[..position.min(xml.len())];
    let line = bytes.iter().filter(|b| **b == b'\n').count() + 1;
    let line_start = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    Error::Opml { line, column: bytes.len() - line_start + 1, message: message.to_string() }
}

/// Markers of scripts pasted into descriptions by old blog platforms (Tumblr, StatCounter).
const SCRIPT_MARKERS: &[&str] = &["var ", "(function", "_gaq", "sc_project", "document.write"];

/// Plain-text description for display and comparison: tags stripped, scripts cut off,
/// whitespace collapsed. `None` when nothing readable is left.
pub fn clean_description(raw: &str) -> Option<String> {
    let text = strip_tags(raw);
    let cut = SCRIPT_MARKERS
        .iter()
        .filter_map(|m| text.find(m))
        .min()
        .unwrap_or(text.len());
    let clean = text[..cut].split_whitespace().collect::<Vec<_>>().join(" ");
    (!clean.is_empty()).then_some(clean)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}
```

If `reader.error_position()` doesn't exist in quick-xml 0.42, use `reader.buffer_position()` in the `map_err` too. Log the finding in `notes.md`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p ressac-core opml`
Expected: `5 passed`.

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/vienna-like.opml crates/ressac-core/src/opml.rs crates/ressac-core/src/lib.rs
git commit -m "OPML parser with URL repair, description cleaning and error positions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3.2: Import into the store

**Interfaces:**
- Consumes: `opml::{Outline, OpmlFeed}`, `url_key::url_key`, `model::{ts, FeedId, FolderId}`, `Store::conn()`.
- Produces (re-exported from `ressac_core::store`):
  - `struct ImportReport { folders_added: usize, feeds_added: usize, merged: Vec<(String, String)>, skipped_existing: usize }`. `merged` holds (dropped title, kept title).
  - `struct FeedRow`, with fields as in the code below
  - `struct FolderRow { id, parent_id, name, position }`
  - `Store::import_outlines(&self, outlines: &[Outline], dry_run: bool, now: DateTime<Utc>) -> Result<ImportReport>`
  - `Store::feeds(&self) -> Result<Vec<FeedRow>>`, ordered by `id`
  - `Store::feed(&self, id: FeedId) -> Result<FeedRow>`
  - `Store::folders(&self) -> Result<Vec<FolderRow>>`
  - `Store::folder_paths(&self) -> Result<HashMap<FolderId, String>>`

- [ ] **Step 1: Write the failing tests** in `crates/ressac-core/src/store/feeds.rs`

```rust
//! Folders and feeds: OPML import and read access.

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
        let report = store.import_outlines(&opml::parse(FIXTURE).unwrap(), false, now()).unwrap();
        (store, report)
    }

    #[test]
    fn imports_folders_and_feeds_merging_exact_duplicates() {
        let (store, report) = imported();
        assert_eq!(report.folders_added, 7);
        assert_eq!(report.feeds_added, 11);
        assert_eq!(report.merged, vec![("Daring Fireball (1)".to_string(), "Daring Fireball".to_string())]);
        assert_eq!(report.skipped_existing, 0);
        assert_eq!(store.feeds().unwrap().len(), 11);
    }

    #[test]
    fn reimport_is_idempotent() {
        let (store, _) = imported();
        let again = store.import_outlines(&opml::parse(FIXTURE).unwrap(), false, now()).unwrap();
        assert_eq!((again.folders_added, again.feeds_added, again.skipped_existing), (0, 0, 12));
        assert_eq!(store.feeds().unwrap().len(), 11);
    }

    #[test]
    fn dry_run_writes_nothing_but_reports_the_same() {
        let store = Store::open_in_memory().unwrap();
        let report = store.import_outlines(&opml::parse(FIXTURE).unwrap(), true, now()).unwrap();
        assert_eq!(report.feeds_added, 11);
        assert!(store.feeds().unwrap().is_empty());
        assert!(store.folders().unwrap().is_empty());
    }

    #[test]
    fn keeps_homonyms_and_folder_paths() {
        let (store, _) = imported();
        let feeds = store.feeds().unwrap();
        let cht: Vec<_> = feeds.iter().filter(|f| f.title.starts_with("Center for Humane")).collect();
        assert_eq!(cht.len(), 2);
        let paths = store.folder_paths().unwrap();
        let yt = cht.iter().find(|f| f.xml_url.contains("youtube")).unwrap();
        assert_eq!(paths[&yt.folder_id.unwrap()], "Jon / YouTube");
    }

    #[test]
    fn stores_original_url_and_attributes() {
        let (store, _) = imported();
        let flickr = store.feeds().unwrap().into_iter().find(|f| f.title.starts_with("Photos")).unwrap();
        assert!(flickr.xml_url.contains("&lang=en-us"));
        assert!(flickr.xml_url_original.contains("&amp;lang"));
        assert_eq!(flickr.state, "active");
        assert!(flickr.opml_attrs.contains("\"type\":\"rss\""));
        assert_eq!(store.feed(flickr.id).unwrap().title, flickr.title);
    }
}
```

- [ ] **Step 2: Wire the module.** At the top of `store/mod.rs`, after the doc comment, add:

```rust
mod feeds;

pub use feeds::{FeedRow, FolderRow, ImportReport};
```

Run: `cargo test -p ressac-core store::feeds`
Expected: compile errors (`import_outlines` not found).

- [ ] **Step 3: Implement.** Insert above the tests in `store/feeds.rs`:

```rust
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
    pub fn import_outlines(&self, outlines: &[Outline], dry_run: bool, now: DateTime<Utc>) -> Result<ImportReport> {
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
        let mut stmt = self.conn().prepare(&format!("SELECT {FEED_COLUMNS} FROM feed ORDER BY id"))?;
        let rows = stmt.query_map([], feed_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn feed(&self, id: FeedId) -> Result<FeedRow> {
        Ok(self
            .conn()
            .query_row(&format!("SELECT {FEED_COLUMNS} FROM feed WHERE id = ?1"), [id], feed_row)?)
    }

    pub fn folders(&self) -> Result<Vec<FolderRow>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, parent_id, name, position FROM folder ORDER BY parent_id, position")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(FolderRow { id: r.get(0)?, parent_id: r.get(1)?, name: r.get(2)?, position: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Folder id → "Parent / Child" path, from the `v_folder_path` contract view.
    pub fn folder_paths(&self) -> Result<HashMap<FolderId, String>> {
        let mut stmt = self.conn().prepare("SELECT folder_id, path FROM v_folder_path")?;
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
                let original = feed.attrs.get("xmlUrl").cloned().unwrap_or_else(|| feed.xml_url.clone());
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ressac-core store`
Expected: all store tests pass (`4` from Phase 2 + `5` new).

- [ ] **Step 5: Verification gate** (fmt, clippy `-D warnings`, test).

- [ ] **Step 6: Commit and push**

```bash
git add crates/ressac-core/src/store
git commit -m "Idempotent OPML import into the store, with dry run

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Phase gate:** report and wait for approval before Phase 4.
