//! Reading data owned by the core: extracted articles (`entry_fulltext`), the search
//! index (`entry_fts`) and the per-feed full-text preference.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use scraper::{Html, Node};

use super::Store;
use super::fetch::bad_timestamp;
use crate::error::Result;
use crate::extract::{Article, ExtractError};
use crate::model::{FeedId, parse_ts, ts};

#[derive(Debug, Clone, PartialEq)]
pub struct FulltextRow {
    pub fetched_at: DateTime<Utc>,
    pub html: Option<String>,
    pub title: Option<String>,
    pub byline: Option<String>,
    pub error: Option<String>,
}

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
    let mut rows = stmt.query([])?;
    let mut count = 0;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let title: Option<String> = r.get(1)?;
        let author: Option<String> = r.get(2)?;
        let html: Option<String> = r.get(3)?;
        index_entry(
            conn,
            id,
            title.as_deref(),
            author.as_deref(),
            html.as_deref(),
        )?;
        count += 1;
    }
    Ok(count)
}

impl Store {
    /// Stores the outcome of an extraction. An article replaces any earlier error and
    /// becomes the entry's search body. A failure is stored as its sentence, except that
    /// it never replaces an article extracted earlier.
    pub fn save_fulltext(
        &self,
        entry_id: i64,
        outcome: &std::result::Result<Article, ExtractError>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        match outcome {
            Ok(article) => {
                tx.execute(
                    "INSERT INTO entry_fulltext (entry_id, fetched_at, html, title, byline, error) \
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL) \
                     ON CONFLICT(entry_id) DO UPDATE SET fetched_at = excluded.fetched_at, \
                     html = excluded.html, title = excluded.title, byline = excluded.byline, error = NULL",
                    params![entry_id, ts(now), article.html, article.title, article.byline],
                )?;
                tx.execute(
                    "UPDATE entry_fts SET body = ?2 WHERE rowid = ?1",
                    params![entry_id, article.text],
                )?;
            }
            Err(e) => {
                tx.execute(
                    "INSERT INTO entry_fulltext (entry_id, fetched_at, error) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(entry_id) DO UPDATE SET fetched_at = excluded.fetched_at, \
                     error = excluded.error WHERE entry_fulltext.html IS NULL",
                    params![entry_id, ts(now), e.sentence()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn fulltext(&self, entry_id: i64) -> Result<Option<FulltextRow>> {
        let row = self
            .conn()
            .query_row(
                "SELECT fetched_at, html, title, byline, error FROM entry_fulltext WHERE entry_id = ?1",
                [entry_id],
                |r| {
                    let fetched: String = r.get(0)?;
                    Ok(FulltextRow {
                        fetched_at: parse_ts(&fetched).ok_or_else(|| bad_timestamp(0))?,
                        html: r.get(1)?,
                        title: r.get(2)?,
                        byline: r.get(3)?,
                        error: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn prefer_full_text(&self, feed_id: FeedId) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT prefer_full_text FROM feed WHERE id = ?1",
            [feed_id],
            |r| r.get(0),
        )?)
    }

    /// The app writes this column itself; the core only needs it in tests.
    pub fn set_prefer_full_text(&self, feed_id: FeedId, on: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE feed SET prefer_full_text = ?2 WHERE id = ?1",
            params![feed_id, on],
        )?;
        Ok(())
    }

    /// The entry's feed and link, or `None` for an unknown entry.
    pub fn entry_for_extract(&self, entry_id: i64) -> Result<Option<(FeedId, Option<String>)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT feed_id, url FROM entry WHERE id = ?1",
                [entry_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Entries of `feed_id` first stored at or after `since` (this run), with a link and
    /// no extraction yet, newest first.
    pub fn entries_needing_fulltext(
        &self,
        feed_id: FeedId,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn().prepare(
            "SELECT e.id, e.url FROM entry e WHERE e.feed_id = ?1 AND e.fetched_at >= ?2 \
             AND e.url IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM entry_fulltext t WHERE t.entry_id = e.id) \
             ORDER BY e.published_at DESC, e.id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![feed_id, ts(since), limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ErrorKind;
    use crate::opml;
    use crate::parse::{ParsedEntry, ParsedFeed};
    use crate::source::FetchAttempt;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn entry(guid: &str, url: Option<&str>, summary: &str, days_ago: i64) -> ParsedEntry {
        ParsedEntry {
            guid: guid.into(),
            url: url.map(str::to_string),
            title: Some(format!("Title {guid}")),
            author: Some("Ada".into()),
            published_at: now() - chrono::Duration::days(days_ago),
            published_at_is_fallback: false,
            updated_at: None,
            summary_html: Some(summary.into()),
            content_html: None,
        }
    }

    fn attempt(at: DateTime<Utc>, entries: Vec<ParsedEntry>) -> FetchAttempt {
        FetchAttempt {
            feed_id: 1,
            at,
            requested_url: "https://f.example.org/feed".into(),
            final_url: "https://f.example.org/feed".into(),
            http_status: Some(200),
            error: None,
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            bytes: 100,
            duration_ms: 5,
            not_modified: false,
            etag: None,
            last_modified: None,
            feed: Some(ParsedFeed {
                title: Some("F".into()),
                description: None,
                site_url: None,
                entries,
            }),
        }
    }

    /// One feed with two entries: id 1 "a" (with a link) and id 2 "b" (no link).
    fn store() -> Store {
        let s = Store::open_in_memory().unwrap();
        let xml =
            r#"<opml><body><outline text="F" xmlUrl="https://f.example.org/feed"/></body></opml>"#;
        s.import_outlines(&opml::parse(xml).unwrap(), false, now())
            .unwrap();
        s.record_attempt(
            "r1",
            &attempt(
                now(),
                vec![
                    entry("a", Some("https://f.example.org/a"), "<p>harbour</p>", 1),
                    entry("b", None, "<p>lighthouse</p>", 2),
                ],
            ),
        )
        .unwrap();
        s
    }

    fn article() -> Article {
        Article {
            html: "<div><p>The road of wet sand.</p></div>".into(),
            title: Some("On tides".into()),
            byline: Some("Ada Lovelace".into()),
            text: "The road of wet sand.".into(),
        }
    }

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

    #[test]
    fn an_article_is_stored_and_becomes_the_search_body() {
        let s = store();
        assert_eq!(s.search_ids("harbour").unwrap(), [1]);
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.fetched_at, now());
        assert_eq!(row.html.as_deref(), Some(article().html.as_str()));
        assert_eq!(row.title.as_deref(), Some("On tides"));
        assert_eq!(row.byline.as_deref(), Some("Ada Lovelace"));
        assert_eq!(row.error, None);
        assert_eq!(s.search_ids("sand").unwrap(), [1]);
        assert!(s.search_ids("harbour").unwrap().is_empty());
        assert_eq!(s.fulltext(2).unwrap(), None);
    }

    #[test]
    fn a_failure_is_stored_as_its_sentence_and_a_later_article_replaces_it() {
        let s = store();
        s.save_fulltext(1, &Err(ExtractError::Fetch(ErrorKind::Http(404))), now())
            .unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.error.as_deref(), Some("HTTP 404."));
        assert_eq!(row.html, None);
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!((row.html.is_some(), row.error), (true, None));
    }

    #[test]
    fn a_failed_retry_keeps_the_article() {
        let s = store();
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let later = now() + chrono::Duration::hours(1);
        s.save_fulltext(1, &Err(ExtractError::LoginWall), later)
            .unwrap();
        let row = s.fulltext(1).unwrap().unwrap();
        assert_eq!(row.html.as_deref(), Some(article().html.as_str()));
        assert_eq!((row.error, row.fetched_at), (None, now()));
    }

    #[test]
    fn a_changed_feed_entry_keeps_the_article_body_but_follows_its_title() {
        let s = store();
        s.save_fulltext(1, &Ok(article()), now()).unwrap();
        let mut changed = entry("a", Some("https://f.example.org/a"), "<p>pier</p>", 1);
        changed.title = Some("Renamed".into());
        s.record_attempt(
            "r2",
            &attempt(now() + chrono::Duration::hours(1), vec![changed]),
        )
        .unwrap();
        assert_eq!(s.search_ids("sand").unwrap(), [1]);
        assert!(s.search_ids("pier").unwrap().is_empty());
        assert_eq!(s.search_ids("title:renamed").unwrap(), [1]);
    }

    #[test]
    fn prefer_full_text_defaults_off_and_toggles() {
        let s = store();
        assert!(!s.prefer_full_text(1).unwrap());
        s.set_prefer_full_text(1, true).unwrap();
        assert!(s.prefer_full_text(1).unwrap());
        assert!(s.prefer_full_text(99).is_err(), "unknown feed");
    }

    #[test]
    fn entry_for_extract_gives_the_feed_and_the_link() {
        let s = store();
        assert_eq!(
            s.entry_for_extract(1).unwrap(),
            Some((1, Some("https://f.example.org/a".to_string())))
        );
        assert_eq!(s.entry_for_extract(2).unwrap(), Some((1, None)));
        assert_eq!(s.entry_for_extract(99).unwrap(), None);
    }

    #[test]
    fn entries_needing_fulltext_are_this_runs_linked_entries_without_a_row() {
        let s = store();
        let run = now() + chrono::Duration::hours(1);
        s.record_attempt(
            "r2",
            &attempt(
                run,
                vec![
                    entry("c", Some("https://f.example.org/c"), "<p>c</p>", 0),
                    entry("d", Some("https://f.example.org/d"), "<p>d</p>", 0),
                    entry("e", Some("https://f.example.org/e"), "<p>e</p>", 0),
                ],
            ),
        )
        .unwrap();
        s.save_fulltext(4, &Err(ExtractError::NoContent), run)
            .unwrap();
        let due = s.entries_needing_fulltext(1, run, 20).unwrap();
        let ids: Vec<i64> = due.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [5, 3], "not before the run, not already tried");
        assert_eq!(due[1].1, "https://f.example.org/c");
        assert_eq!(s.entries_needing_fulltext(1, run, 1).unwrap().len(), 1);
        assert_eq!(
            s.entries_needing_fulltext(1, now(), 20).unwrap().len(),
            3,
            "entry 2 has no link"
        );
    }
}
