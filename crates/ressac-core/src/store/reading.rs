//! Reading data owned by the core: extracted articles (`entry_fulltext`), the search
//! index (`entry_fts`) and the per-feed full-text preference.

use rusqlite::{Connection, params};
use scraper::{Html, Node};

/// Elements whose boundaries separate words, so `<p>a</p><p>b</p>` reads "a b".
const WORD_BREAKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "td",
    "th",
    "tr",
    "ul",
];

/// Elements whose text is never shown.
const HIDDEN: &[&str] = &["script", "style", "template", "noscript"];

/// Plain text of an HTML fragment: tags stripped, entities decoded, scripts and styles
/// dropped, whitespace collapsed.
pub fn plain_text(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    let mut out = String::with_capacity(html.len());
    for node in fragment.root_element().descendants() {
        match node.value() {
            Node::Text(text) => {
                let hidden = node.ancestors().any(|a| {
                    a.value()
                        .as_element()
                        .is_some_and(|e| HIDDEN.contains(&e.name()))
                });
                if !hidden {
                    out.push_str(text);
                }
            }
            Node::Element(e) if WORD_BREAKS.contains(&e.name()) => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Writes the search row of a feed entry (rowid = entry id) after an insert or a change.
/// `html` is the entry's `content_html`, else its `summary_html`. An extracted article
/// keeps its body: only the title and author follow the feed.
pub(super) fn index_entry(
    conn: &Connection,
    entry_id: i64,
    title: Option<&str>,
    author: Option<&str>,
    html: Option<&str>,
) -> rusqlite::Result<()> {
    let has_article: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM entry_fulltext WHERE entry_id = ?1 AND html IS NOT NULL)",
        [entry_id],
        |r| r.get(0),
    )?;
    if has_article {
        conn.execute(
            "UPDATE entry_fts SET title = ?2, author = ?3 WHERE rowid = ?1",
            params![entry_id, title, author],
        )?;
    } else {
        conn.execute("DELETE FROM entry_fts WHERE rowid = ?1", [entry_id])?;
        conn.execute(
            "INSERT INTO entry_fts (rowid, title, author, body) VALUES (?1, ?2, ?3, ?4)",
            params![
                entry_id,
                title,
                author,
                html.map(plain_text).unwrap_or_default()
            ],
        )?;
    }
    Ok(())
}

/// Fills `entry_fts` from every existing entry. Runs once, inside migration 2.
pub(super) fn backfill_fts(conn: &Connection) -> rusqlite::Result<usize> {
    let mut stmt =
        conn.prepare("SELECT id, title, author, COALESCE(content_html, summary_html) FROM entry")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, title, author, html) in &rows {
        index_entry(
            conn,
            *id,
            title.as_deref(),
            author.as_deref(),
            html.as_deref(),
        )?;
    }
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_strips_tags_decodes_entities_and_collapses_space() {
        assert_eq!(
            plain_text(
                "<p>Caf&eacute; &amp; <b>cr</b>&egrave;me</p><p>Next\n\n line</p>\
                 <script>var x = 1;</script><style>p { color: red }</style>"
            ),
            "Café & crème Next line"
        );
        assert_eq!(plain_text("one<br>two"), "one two");
        assert_eq!(plain_text("plain words, no tags"), "plain words, no tags");
        assert_eq!(plain_text(""), "");
    }
}
