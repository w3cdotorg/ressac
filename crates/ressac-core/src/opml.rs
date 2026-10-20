//! OPML import (parsing) and export.

use std::collections::BTreeMap;

use quick_xml::escape::escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::error::{Error, Result};
use crate::health::hijack::has_spam;
use crate::model::FolderId;
use crate::parse::decode_title_entities;
use crate::store::{FeedRow, FolderRow, Store};
use crate::url_key::repair_url;

#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    Folder {
        name: String,
        children: Vec<Outline>,
    },
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
                    Err(attrs) => Outline::Folder {
                        name: folder_name(&attrs),
                        children: Vec::new(),
                    },
                };
                push(&mut stack, item);
            }
            Event::End(e) if e.name().0 == "outline" => match stack.pop() {
                Some(Frame::Folder(name, children)) => {
                    push(&mut stack, Outline::Folder { name, children })
                }
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
fn feed_from(
    attrs: BTreeMap<String, String>,
) -> std::result::Result<OpmlFeed, BTreeMap<String, String>> {
    if !attrs.get("xmlUrl").is_some_and(|u| !u.trim().is_empty()) {
        return Err(attrs);
    }
    let xml_url = repair_url(&attrs["xmlUrl"]);
    let title = attrs
        .get("text")
        .or_else(|| attrs.get("title"))
        .map(|t| decode_title_entities(t.trim()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| xml_url.clone());
    let html_url = attrs
        .get("htmlUrl")
        .map(|u| repair_url(u))
        .filter(|u| !u.is_empty());
    let description = attrs
        .get("description")
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    Ok(OpmlFeed {
        title,
        xml_url,
        html_url,
        description,
        attrs,
    })
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
    Error::Opml {
        line,
        column: bytes.len() - line_start + 1,
        message: message.to_string(),
    }
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

const RESSAC_NS: &str = "https://github.com/w3cdotorg/ressac";
const LEADING_ATTRS: [&str; 5] = ["text", "type", "xmlUrl", "htmlUrl", "description"];

/// OPML 2.0 export of active feeds, or of the cemetery (buried feeds) with Wayback links.
pub fn export(store: &Store, cemetery: bool) -> Result<String> {
    let folders = store.folders()?;
    let feeds: Vec<FeedRow> = store
        .feeds()?
        .into_iter()
        .filter(|f| {
            if cemetery {
                f.state == "buried"
            } else {
                f.state == "active"
            }
        })
        .collect();
    let title = if cemetery {
        "Ressac cemetery"
    } else {
        "Ressac subscriptions"
    };
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    if cemetery {
        out.push_str(&format!(
            "<opml version=\"2.0\" xmlns:ressac=\"{RESSAC_NS}\">\n"
        ));
    } else {
        out.push_str("<opml version=\"2.0\">\n");
    }
    out.push_str(&format!(
        "  <head>\n    <title>{title}</title>\n    <dateCreated>{}</dateCreated>\n  </head>\n  <body>\n",
        chrono::Utc::now().to_rfc2822()
    ));
    write_level(&mut out, None, 2, &folders, &feeds, cemetery);
    out.push_str("  </body>\n</opml>\n");
    Ok(out)
}

enum Node<'a> {
    Folder(&'a FolderRow),
    Feed(&'a FeedRow),
}

fn write_level(
    out: &mut String,
    parent: Option<FolderId>,
    depth: usize,
    folders: &[FolderRow],
    feeds: &[FeedRow],
    cemetery: bool,
) {
    let mut nodes: Vec<(i64, Node)> = folders
        .iter()
        .filter(|f| f.parent_id == parent)
        .map(|f| (f.position, Node::Folder(f)))
        .chain(
            feeds
                .iter()
                .filter(|f| f.folder_id == parent)
                .map(|f| (f.position, Node::Feed(f))),
        )
        .collect();
    nodes.sort_by_key(|(position, _)| *position);
    let indent = "  ".repeat(depth);
    for (_, node) in nodes {
        match node {
            Node::Folder(folder) if has_feeds(folder.id, folders, feeds) => {
                out.push_str(&format!(
                    "{indent}<outline text=\"{}\">\n",
                    escape(folder.name.as_str())
                ));
                write_level(out, Some(folder.id), depth + 1, folders, feeds, cemetery);
                out.push_str(&format!("{indent}</outline>\n"));
            }
            Node::Folder(_) => {}
            Node::Feed(feed) => {
                out.push_str(&format!("{indent}{}\n", feed_outline(feed, cemetery)))
            }
        }
    }
}

fn has_feeds(folder: FolderId, folders: &[FolderRow], feeds: &[FeedRow]) -> bool {
    feeds.iter().any(|f| f.folder_id == Some(folder))
        || folders
            .iter()
            .filter(|c| c.parent_id == Some(folder))
            .any(|c| has_feeds(c.id, folders, feeds))
}

fn feed_outline(feed: &FeedRow, cemetery: bool) -> String {
    let mut attrs: BTreeMap<String, String> =
        serde_json::from_str(&feed.opml_attrs).unwrap_or_default();
    attrs.insert("text".into(), feed.title.clone());
    attrs.entry("type".into()).or_insert_with(|| "rss".into());
    attrs.insert("xmlUrl".into(), feed.xml_url.clone());
    match &feed.html_url {
        Some(u) => attrs.insert("htmlUrl".into(), u.clone()),
        None => attrs.remove("htmlUrl"),
    };
    let description = attrs
        .get("description")
        .cloned()
        .or_else(|| feed.description_original.clone())
        .and_then(|d| clean_description(&d))
        .filter(|d| !has_spam(d));
    match description {
        Some(d) => attrs.insert("description".into(), d),
        None => attrs.remove("description"),
    };
    attrs.remove("ressac:wayback");
    if cemetery && let Some(w) = &feed.wayback_url {
        attrs.insert("ressac:wayback".into(), w.clone());
    }
    let mut parts = Vec::new();
    for key in LEADING_ATTRS {
        if let Some(v) = attrs.remove(key) {
            parts.push(format!("{key}=\"{}\"", escape(v.as_str())));
        }
    }
    parts.extend(
        attrs
            .iter()
            .map(|(k, v)| format!("{k}=\"{}\"", escape(v.as_str()))),
    );
    format!("<outline {}/>", parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/vienna-like.opml");

    #[test]
    fn c1_references_in_outline_titles_become_windows_1252() {
        let xml = r#"<opml><body><outline text="l&#146;a" xmlUrl="https://a.example/f"/><outline text="l&amp;#146;b" xmlUrl="https://b.example/f"/></body></opml>"#;
        let outlines = parse(xml).unwrap();
        let titles: Vec<&str> = feeds(&outlines).iter().map(|f| f.title.as_str()).collect();
        assert_eq!(titles, ["l\u{2019}a", "l\u{2019}b"]);
    }

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
        feeds(items)
            .into_iter()
            .find(|f| f.title == title)
            .expect(title)
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
        let Outline::Folder { children, .. } = &items[0] else {
            panic!("Blogs is a folder")
        };
        assert_eq!(children.len(), 3);
        assert!(
            matches!(&children[1], Outline::Folder { name, children } if name == "potes" && children.len() == 3)
        );
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
        assert!(
            flickr.attrs["xmlUrl"].contains("&amp;lang"),
            "original attribute kept as read"
        );
    }

    #[test]
    fn keeps_homonyms_and_metadata() {
        let items = parse(FIXTURE).unwrap();
        let cht = find(&items, "Center for Humane Technology (1)");
        assert!(cht.xml_url.contains("youtube.com"));
        assert_eq!(
            find(&items, "xkcd.com").html_url.as_deref(),
            Some("https://xkcd.com/")
        );
        assert_eq!(
            find(&items, "Orange Drenka").description,
            None,
            "empty description is None"
        );
    }

    #[test]
    fn cleans_javascript_polluted_descriptions() {
        let items = parse(FIXTURE).unwrap();
        let raw = find(&items, "Girl on the Couch")
            .description
            .clone()
            .unwrap();
        assert!(raw.contains("sc_project"));
        assert_eq!(
            clean_description(&raw).as_deref(),
            Some("A book is a loaded gun in the house next door")
        );
        assert_eq!(
            clean_description("<p>Hello <b>world</b></p>").as_deref(),
            Some("Hello world")
        );
        assert_eq!(
            clean_description("  var _gaq = _gaq || [];").as_deref(),
            None
        );
    }

    #[test]
    fn reports_position_of_malformed_opml() {
        let bad = "<opml>\n<body>\n<outline text=\"x\">\n</body></opml>";
        match parse(bad) {
            Err(Error::Opml { line, .. }) => assert!(line >= 3, "line {line}"),
            other => panic!("expected OPML error, got {other:?}"),
        }
        assert!(matches!(
            parse("<opml><body><outline text=\"x\">"),
            Err(Error::Opml { .. })
        ));
    }

    mod export_tests {
        use super::super::*;
        use crate::model::{Decision, Status};
        use crate::store::Store;

        const FIXTURE: &str = include_str!("../../../tests/fixtures/vienna-like.opml");

        fn imported() -> Store {
            let s = Store::open_in_memory().unwrap();
            s.import_outlines(
                &parse(FIXTURE).unwrap(),
                false,
                "2026-10-10T08:00:00Z".parse().unwrap(),
            )
            .unwrap();
            s
        }

        fn all_feeds(items: &[Outline]) -> Vec<OpmlFeed> {
            items
                .iter()
                .flat_map(|o| match o {
                    Outline::Feed(f) => vec![f.clone()],
                    Outline::Folder { children, .. } => all_feeds(children),
                })
                .collect()
        }

        #[test]
        fn export_round_trips_the_tree_and_cleans_attributes() {
            let xml = export(&imported(), false).unwrap();
            assert!(
                xml.starts_with(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">"
                )
            );
            let items = parse(&xml).unwrap();
            let feeds = all_feeds(&items);
            assert_eq!(feeds.len(), 11);
            let Outline::Folder { name, children } = &items[0] else {
                panic!()
            };
            assert_eq!(name, "Blogs");
            assert!(matches!(&children[1], Outline::Folder { name, .. } if name == "potes"));
            let flickr = feeds
                .iter()
                .find(|f| f.title.starts_with("Photos"))
                .unwrap();
            assert!(!flickr.attrs["xmlUrl"].contains("&amp;"));
            assert_eq!(
                feeds
                    .iter()
                    .find(|f| f.title == "Bibliobsession 2.0")
                    .unwrap()
                    .description,
                None
            );
            assert_eq!(
                feeds
                    .iter()
                    .find(|f| f.title == "Girl on the Couch")
                    .unwrap()
                    .description
                    .as_deref(),
                Some("A book is a loaded gun in the house next door")
            );
        }

        #[test]
        fn buried_feeds_go_to_the_cemetery_only() {
            let s = imported();
            let girl = s
                .feeds()
                .unwrap()
                .into_iter()
                .find(|f| f.title == "Girl on the Couch")
                .unwrap();
            s.record_decision(
                girl.id,
                Decision::Bury,
                Status::Dead,
                None,
                "2026-10-10T08:00:00Z".parse().unwrap(),
            )
            .unwrap();
            s.set_wayback(girl.id, "http://web.archive.org/web/2012/girl")
                .unwrap();
            assert_eq!(
                all_feeds(&parse(&export(&s, false).unwrap()).unwrap()).len(),
                10
            );
            let cemetery = export(&s, true).unwrap();
            assert!(cemetery.contains("xmlns:ressac=\"https://github.com/w3cdotorg/ressac\""));
            let buried = all_feeds(&parse(&cemetery).unwrap());
            assert_eq!(buried.len(), 1);
            assert_eq!(
                buried[0].attrs["ressac:wayback"],
                "http://web.archive.org/web/2012/girl"
            );
        }

        #[test]
        fn stale_wayback_attribute_is_dropped_from_a_normal_export() {
            let s = Store::open_in_memory().unwrap();
            let xml = r#"<opml xmlns:ressac="x"><body><outline text="A" xmlUrl="http://a.example.org/f" ressac:wayback="http://web.archive.org/x"/></body></opml>"#;
            s.import_outlines(
                &parse(xml).unwrap(),
                false,
                "2026-10-10T08:00:00Z".parse().unwrap(),
            )
            .unwrap();
            let out = export(&s, false).unwrap();
            assert!(!out.contains("ressac:wayback"), "{out}");
        }

        #[test]
        fn special_characters_survive_a_round_trip() {
            let s = Store::open_in_memory().unwrap();
            let xml = r#"<opml><body><outline text="Say &quot;hi&quot; &lt;b&gt;" xmlUrl="http://a.example.org/f?a=1&amp;b=2"/></body></opml>"#;
            s.import_outlines(
                &parse(xml).unwrap(),
                false,
                "2026-10-10T08:00:00Z".parse().unwrap(),
            )
            .unwrap();
            let feeds = all_feeds(&parse(&export(&s, false).unwrap()).unwrap());
            assert_eq!(feeds[0].title, "Say \"hi\" <b>");
            assert_eq!(feeds[0].xml_url, "http://a.example.org/f?a=1&b=2");
        }

        #[test]
        fn a_feed_title_encoded_twice_is_decoded_once_more() {
            let xml = r#"<opml><body><outline text="America&amp;#39;s Q&amp;amp;A" xmlUrl="http://a.example.org/f"/><outline text="Fish &amp; Chips" xmlUrl="http://b.example.org/f"/></body></opml>"#;
            let feeds = all_feeds(&parse(xml).unwrap());
            assert_eq!(feeds[0].title, "America's Q&A");
            assert_eq!(feeds[1].title, "Fish & Chips");
        }
    }
}
