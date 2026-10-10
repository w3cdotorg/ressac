//! OPML import (parsing) and export.

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
