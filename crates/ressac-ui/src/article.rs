//! The reader's page (the Mac's `ArticleTemplate.page`, `ArticlePage`, `LinkPolicy`, the Full
//! Article state of `AppState` and `ReaderView.fullArticleHelp`): the HTML the `ressac:` scheme
//! serves for an entry, which content it shows, when a new page is due, which navigations the
//! view may follow, and the app-world scripts the view runs. Article JavaScript never runs: the
//! view has JavaScript off, the page's CSP forbids scripts, and `sanitize` removes them.

use std::collections::BTreeSet;

use chrono::{DateTime, TimeZone, Utc};
use ressac_core::service::ExtractOutcome;
use ressac_core::store::{EntryDetail, EntryRow, FulltextRow};

use crate::entries::display_title;
use crate::sanitize::{Images, is_web_url, sanitize};
use crate::settings::{Appearance, LineWidth, ReaderFont, Settings, effective_size};
use crate::worker::JobFailure;

/// The Mac's stylesheet, reused in place.
pub const READER_CSS: &str = include_str!("../../../apps/macos/Ressac/Resources/reader.css");
/// No scripts, frames, objects, forms or base URL; images from the web and the mosaic scheme;
/// styles inline; fonts from our own scheme.
pub const CSP: &str = "default-src 'none'; img-src http: https: data: ressac-mosaic:; style-src 'unsafe-inline'; font-src ressac:; base-uri 'none'; form-action 'none'";
pub const PAGE_PREFIX: &str = "ressac:page/";
pub const ASSET_PREFIX: &str = "ressac:asset/";
/// The recette's proof page: every kind of article script, unsanitised, with no CSP.
pub const PROOF_URL: &str = "ressac:page/proof";
pub const PROOF_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Reader proof</title>
<script>document.documentElement.setAttribute("data-inline", "ran");</script>
<script src="ressac:asset/proof.js"></script>
</head>
<body onload="document.documentElement.setAttribute('data-onload', 'ran')">
<img src="data:," onerror="document.documentElement.setAttribute('data-onerror', 'ran')">
<p><a id="jslink" href="javascript:document.documentElement.setAttribute('data-jslink', 'ran')">A javascript: link</a></p>
<div class="content"><p>Article scripts never run in the reader.</p></div>
</body></html>"#;
/// `proof.js`: what an external article script would do if it ran.
pub const PROOF_SCRIPT: &str = r#"document.documentElement.setAttribute("data-external", "ran");"#;
/// The recette's app-world probe of the proof page: the app's script runs, no article script did.
pub const RECETTE_PROBE: &str = r#"(() => { const d = document.documentElement; d.setAttribute("data-app", "ran"); const j = document.getElementById("jslink"); if (j) { j.click(); } return JSON.stringify({ app: d.getAttribute("data-app"), inline: d.getAttribute("data-inline"), external: d.getAttribute("data-external"), onload: d.getAttribute("data-onload"), onerror: d.getAttribute("data-onerror"), jslink: d.getAttribute("data-jslink"), scripts: document.querySelectorAll("script").length, paragraphs: document.querySelectorAll(".content p").length }); })()"#;
pub const NO_ENTRY_TITLE: &str = "No Entry Selected";
pub const NO_ENTRY_TEXT: &str = "Choose an entry in the list to read it here.";
pub const FULL_ARTICLE_UNAVAILABLE: &str = "Full article unavailable: ";

/// A file the `ressac:asset/` scheme serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub file: &'static str,
    pub mime: &'static str,
}

/// Everything `ressac:asset/` serves: the bundled fonts (from the Mac's `Resources/Fonts`) and
/// the recette's external script.
pub const ASSETS: [Asset; 6] = [
    Asset {
        file: "Newsreader-Variable.ttf",
        mime: "font/ttf",
    },
    Asset {
        file: "Newsreader-Italic-Variable.ttf",
        mime: "font/ttf",
    },
    Asset {
        file: "AtkinsonHyperlegibleNext-Variable.ttf",
        mime: "font/ttf",
    },
    Asset {
        file: "AtkinsonHyperlegibleNext-Italic-Variable.ttf",
        mime: "font/ttf",
    },
    Asset {
        file: "bedstead.otf",
        mime: "font/otf",
    },
    Asset {
        file: "proof.js",
        mime: "text/javascript",
    },
];

/// The asset named exactly `file`: no path, no other spelling.
pub fn asset(file: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|asset| asset.file == file)
}

/// The address of an entry's page; `revision` changes whenever the page identity does.
pub fn page_url(entry_id: i64, revision: u64) -> String {
    format!("{PAGE_PREFIX}{entry_id}-{revision}")
}

/// The system's light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

impl ColorScheme {
    pub fn parse(text: &str) -> Option<ColorScheme> {
        match text {
            "light" => Some(ColorScheme::Light),
            "dark" => Some(ColorScheme::Dark),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
        }
    }
}

/// The look the reader draws, and whether the CRT screen box is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderTheme {
    pub appearance: Appearance,
    pub crt: bool,
}

impl ReaderTheme {
    pub const NATIVE: ReaderTheme = ReaderTheme {
        appearance: Appearance::Native,
        crt: false,
    };

    /// The CRT bleed applies: Antiope with the box checked.
    pub fn shows_crt(self) -> bool {
        self.appearance == Appearance::Antiope && self.crt
    }
}

/// The reading font, size and column width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReaderStyle {
    pub font: ReaderFont,
    pub size: f64,
    pub width: LineWidth,
}

impl Default for ReaderStyle {
    fn default() -> ReaderStyle {
        ReaderStyle {
            font: ReaderFont::SystemSans,
            size: crate::settings::READER_SIZE_DEFAULT,
            width: LineWidth::Medium,
        }
    }
}

impl ReaderStyle {
    /// The style of `look`: its own stored size, as that look draws it.
    pub fn of(settings: &Settings, look: Appearance) -> ReaderStyle {
        let size = match look {
            Appearance::Native => settings.reader_size,
            Appearance::Antiope => settings.reader_size_antiope,
        };
        ReaderStyle {
            font: settings.reader_font,
            size: effective_size(size, look),
            width: settings.reader_width,
        }
    }
}

/// The CSS font stack of `font`, the chosen face first (Linux's system faces are Noto).
pub fn css_family(font: ReaderFont) -> &'static str {
    match font {
        ReaderFont::SystemSans => r#"system-ui, "Noto Sans", sans-serif"#,
        ReaderFont::SystemSerif => r#""Noto Serif", serif"#,
        ReaderFont::Newsreader => r#""Newsreader", "Noto Serif", serif"#,
        ReaderFont::Atkinson => r#""Atkinson Hyperlegible Next", system-ui, sans-serif"#,
    }
}

/// The bundled faces of `font`: (asset file, italic). None for the system's fonts.
pub fn bundled_faces(font: ReaderFont) -> &'static [(&'static str, bool)] {
    match font {
        ReaderFont::Newsreader => &[
            ("Newsreader-Variable.ttf", false),
            ("Newsreader-Italic-Variable.ttf", true),
        ],
        ReaderFont::Atkinson => &[
            ("AtkinsonHyperlegibleNext-Variable.ttf", false),
            ("AtkinsonHyperlegibleNext-Italic-Variable.ttf", true),
        ],
        ReaderFont::SystemSans | ReaderFont::SystemSerif => &[],
    }
}

/// The `@font-face` rules of `font`'s bundled faces, one per line, served by `ressac:asset/`.
pub fn font_faces(font: ReaderFont) -> String {
    let family = match font {
        ReaderFont::Newsreader => "Newsreader",
        ReaderFont::Atkinson => "Atkinson Hyperlegible Next",
        ReaderFont::SystemSans | ReaderFont::SystemSerif => return String::new(),
    };
    bundled_faces(font)
        .iter()
        .map(|(file, italic)| {
            let style = if *italic { "italic" } else { "normal" };
            format!(
                r#"@font-face {{ font-family: "{family}"; font-style: {style}; font-weight: 200 800; src: url("{ASSET_PREFIX}{file}") format("truetype"); }}"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Bedstead, Antiope's font.
pub fn teletext_face() -> String {
    format!(
        r#"@font-face {{ font-family: "Bedstead"; font-style: normal; font-weight: 400; src: url("{ASSET_PREFIX}bedstead.otf") format("opentype"); }}"#
    )
}

/// An entry's place in the list shown, under its own feed's page (Antiope): "412 3/20".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubpageLabel {
    pub page: Option<u16>,
    pub index: usize,
    pub count: usize,
}

impl SubpageLabel {
    /// "412 3/20", or "3/20" for a feed without a page number yet.
    pub fn text(&self) -> String {
        match self.page {
            Some(page) => format!("{page} {}/{}", self.index, self.count),
            None => format!("{}/{}", self.index, self.count),
        }
    }
}

/// Everything the reader's page depends on.
#[derive(Debug, Clone, PartialEq)]
pub struct ArticlePage {
    pub entry: EntryDetail,
    /// The entry's stored full article, if any.
    pub fulltext: Option<FulltextRow>,
    /// The Full Article toggle is on.
    pub show_full: bool,
    pub style: ReaderStyle,
    pub scheme: ColorScheme,
    pub theme: ReaderTheme,
    pub subpage: Option<SubpageLabel>,
}

impl ArticlePage {
    /// The page, with the date in `tz` (the Mac's `ArticleTemplate.page`, line for line).
    pub fn html<Tz: TimeZone>(&self, tz: &Tz) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        let row = &self.entry.row;
        let antiope = self.theme.appearance == Appearance::Antiope;
        let link = row.url.as_deref().filter(|url| is_web_url(url));
        let full = self.fulltext.as_ref().filter(|_| self.show_full);
        let full_html = full.and_then(|full| full.html.as_deref());
        let source = full_html
            .or_else(|| {
                [&self.entry.content_html, &self.entry.summary_html]
                    .into_iter()
                    .flatten()
                    .map(String::as_str)
                    .find(|html| !html.trim().is_empty())
            })
            .unwrap_or("");
        let images = if antiope {
            Images::Mosaics
        } else {
            Images::Web
        };
        let body = sanitize(source, link, images);

        let mut meta = vec![
            escape_text(&row.feed_title),
            escape_text(&long_date(row.published_at, tz)),
        ];
        if full_html.is_some() {
            meta.push(r#"<span class="badge">full article</span>"#.to_string());
        }
        let title = escape_text(&display_title(row.title.as_deref()));
        let heading = match link {
            Some(url) => format!(r#"<a href="{}">{title}</a>"#, escape_attribute(url)),
            None => title.clone(),
        };
        let byline = full_html
            .and(full.and_then(|full| full.byline.as_deref()))
            .or(row.author.as_deref());
        let notice = full.and_then(|full| full.error.as_deref());
        let empty_text = match link {
            Some(_) => "This entry has no text. Open it in the browser to read it.",
            None => "This entry has no text.",
        };
        let size = effective_size(self.style.size, self.theme.appearance);

        let faces = if antiope {
            teletext_face()
        } else {
            font_faces(self.style.font)
        };
        let teletext = if antiope {
            format!(
                r#" --teletext-font: "Bedstead", monospace; --font-pixel: {};"#,
                size / 10.0
            )
        } else {
            String::new()
        };
        let subpage = if antiope {
            let text = self.subpage.map(|label| label.text()).unwrap_or_default();
            format!(
                r#"<p class="subpage" id="subpage">{}</p>"#,
                escape_text(&text)
            )
        } else {
            String::new()
        };
        let heading = if antiope {
            format!(
                r#"<span class="dh-space" aria-hidden="true">{title}</span><span class="dh-ink">{heading}</span>"#
            )
        } else {
            heading
        };
        let byline = byline
            .map(|b| format!(r#"<p class="byline">{}</p>"#, escape_text(b)))
            .unwrap_or_default();
        let notice = notice
            .map(|error| {
                format!(
                    r#"<p class="notice">{}</p>"#,
                    escape_text(&format!("{FULL_ARTICLE_UNAVAILABLE}{error}"))
                )
            })
            .unwrap_or_default();
        let content = if body.is_empty() {
            format!(r#"<p class="empty">{empty_text}</p>"#)
        } else {
            body
        };
        format!(
            r#"<!doctype html>
<html {root}>
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<meta name="referrer" content="no-referrer">
<style>
{faces}
{READER_CSS}
:root {{ --reader-font: {family}; --reader-size: {size}px; --reader-measure: {measure}ch;{teletext} }}
</style>
</head>
<body>
<article>
{subpage}
<p class="meta">{meta}</p>
<h1 class="title">{heading}</h1>
{byline}
{notice}
<div class="content">{content}</div>
</article>
</body>
</html>"#,
            root = root_attributes(self.scheme, self.theme),
            family = css_family(self.style.font),
            size = size as i64,
            measure = self.style.width.max_ch(),
            meta = meta.join(r#"<span class="dot">·</span>"#),
        )
    }

    /// The same page: a change of read or starred flag, of the full-article flags of the row, or
    /// of the Antiope sub-page line (rewritten in place) never reloads it and loses its scroll.
    pub fn same_page(&self, other: &ArticlePage) -> bool {
        let EntryDetail {
            row,
            summary_html,
            content_html,
            fulltext_html,
            fulltext_error,
            prefer_full_text: _,
        } = &self.entry;
        let EntryRow {
            entry_id,
            feed_id,
            feed_title,
            title,
            author,
            url,
            published_at,
            is_read: _,
            is_starred: _,
            has_fulltext: _,
        } = row;
        let o = &other.entry;
        *entry_id == o.row.entry_id
            && *feed_id == o.row.feed_id
            && *feed_title == o.row.feed_title
            && *title == o.row.title
            && *author == o.row.author
            && *url == o.row.url
            && *published_at == o.row.published_at
            && *summary_html == o.summary_html
            && *content_html == o.content_html
            && *fulltext_html == o.fulltext_html
            && *fulltext_error == o.fulltext_error
            && self.fulltext == other.fulltext
            && self.show_full == other.show_full
            && self.style == other.style
            && self.scheme == other.scheme
            && self.theme == other.theme
    }
}

/// `:root`'s attributes: the light or dark scheme, plus `data-theme="antiope"` and, with the CRT
/// screen on, `data-crt`.
fn root_attributes(scheme: ColorScheme, theme: ReaderTheme) -> String {
    let mut attributes = format!(r#"lang="en" data-scheme="{}""#, scheme.as_str());
    if theme.appearance == Appearance::Antiope {
        attributes.push_str(r#" data-theme="antiope""#);
    }
    if theme.shows_crt() {
        attributes.push_str(" data-crt");
    }
    attributes
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attribute(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

/// "October 9, 2026 at 10:00 AM", in `tz`.
pub fn long_date<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.with_timezone(tz)
        .format("%B %-d, %Y at %-I:%M %p")
        .to_string()
}

/// A `WebEngineNavigationRequest.navigationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    LinkClicked,
    Typed,
    FormSubmitted,
    BackForward,
    Reload,
    Other,
    Redirect,
}

impl Navigation {
    /// Qt's value (0…6 in this order); anything else is `Other`.
    pub fn from_qt(value: i32) -> Navigation {
        match value {
            0 => Navigation::LinkClicked,
            1 => Navigation::Typed,
            2 => Navigation::FormSubmitted,
            3 => Navigation::BackForward,
            4 => Navigation::Reload,
            6 => Navigation::Redirect,
            _ => Navigation::Other,
        }
    }
}

/// What the reader does with a navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkDecision {
    Allow,
    Cancel,
    OpenExternally(String),
    ToggleImage(u32),
}

impl LinkDecision {
    /// "allow", "cancel", "open:<url>" or "toggle:<n>", for QML.
    pub fn encode(&self) -> String {
        match self {
            LinkDecision::Allow => "allow".to_string(),
            LinkDecision::Cancel => "cancel".to_string(),
            LinkDecision::OpenExternally(url) => format!("open:{url}"),
            LinkDecision::ToggleImage(index) => format!("toggle:{index}"),
        }
    }
}

pub use crate::sanitize::TOGGLE_SCHEME;

/// Only the page the pane set loads (typed, main frame, under `PAGE_PREFIX`); a clicked link
/// swaps a mosaic or, when it is a web address, opens in the browser; everything else (other
/// pages, forms, reloads, redirects, history, other schemes) is cancelled.
pub fn decide(kind: Navigation, url: &str, main_frame: bool, expected_page: &str) -> LinkDecision {
    match kind {
        Navigation::Typed if main_frame && url == expected_page && url.starts_with(PAGE_PREFIX) => {
            LinkDecision::Allow
        }
        Navigation::LinkClicked => match toggle_index(url) {
            Some(index) => LinkDecision::ToggleImage(index),
            None if is_web_url(url) => LinkDecision::OpenExternally(url.to_string()),
            None => LinkDecision::Cancel,
        },
        _ => LinkDecision::Cancel,
    }
}

/// `n` of `ressac-toggle:<n>` (the scheme in any case), or None for any other address.
pub fn toggle_index(url: &str) -> Option<u32> {
    let (scheme, index) = url.split_once(':')?;
    if !scheme.eq_ignore_ascii_case(TOGGLE_SCHEME) {
        return None;
    }
    index.parse().ok()
}

/// What the Full Article toggle does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullTextStep {
    /// Back to the feed's text.
    ShowFeedText,
    /// Show the article already stored.
    ShowStored,
    /// Fetch it (after a failure, a retry).
    Extract(i64),
    Nothing,
}

/// The Full Article toggle's state (the Mac's `readerShowsFullText`, `extractingEntryIds` and
/// `fullTextNotice`).
#[derive(Debug, Clone, Default)]
pub struct FullText {
    /// The reader asks for the full article.
    shows: bool,
    extracting: BTreeSet<i64>,
    /// Why the last fetch for the selected entry failed.
    notice: Option<String>,
}

impl FullText {
    /// A new selection: a stored full article is shown at once; the notice goes.
    pub fn select(&mut self, entry: Option<&EntryRow>) {
        self.shows = entry.is_some_and(|entry| entry.has_fulltext);
        self.notice = None;
    }

    /// The toggle: on, back to the feed's text; off with a stored article, show it; otherwise
    /// fetch it.
    pub fn request(&mut self, entry: &EntryRow, fulltext: Option<&FulltextRow>) -> FullTextStep {
        if !has_web_link(entry) {
            return FullTextStep::Nothing;
        }
        if self.is_showing(fulltext) {
            self.shows = false;
            return FullTextStep::ShowFeedText;
        }
        if entry.has_fulltext {
            self.shows = true;
            return FullTextStep::ShowStored;
        }
        if self.extracting.contains(&entry.entry_id) {
            return FullTextStep::Nothing;
        }
        self.shows = true;
        self.extracting.insert(entry.entry_id);
        self.notice = None;
        FullTextStep::Extract(entry.entry_id)
    }

    /// A fetch is over: a failure becomes the notice while its entry is still the selected one.
    pub fn finished(
        &mut self,
        entry_id: i64,
        selected: Option<i64>,
        outcome: Result<ExtractOutcome, JobFailure>,
    ) {
        self.extracting.remove(&entry_id);
        if let Some(sentence) = extraction_sentence(entry_id, &outcome)
            && self.notice.is_none()
            && selected == Some(entry_id)
        {
            self.notice = Some(format!("{FULL_ARTICLE_UNAVAILABLE}{sentence}"));
        }
    }

    /// The reader asks for the full article (the page keeps its notice).
    pub fn shows(&self) -> bool {
        self.shows
    }

    /// The selected entry's full article is being fetched.
    pub fn is_extracting(&self, selected: Option<i64>) -> bool {
        selected.is_some_and(|id| self.extracting.contains(&id))
    }

    /// The full article is on screen (`fulltext` is the selected entry's): the button is on.
    pub fn is_showing(&self, fulltext: Option<&FulltextRow>) -> bool {
        self.shows && fulltext.is_some_and(|full| full.html.is_some())
    }

    /// A web link, and not already fetching it.
    pub fn can_request(&self, entry: Option<&EntryRow>) -> bool {
        entry.is_some_and(|entry| has_web_link(entry) && !self.extracting.contains(&entry.entry_id))
    }

    /// Why the full article asked for is not shown, while its notice is on screen.
    pub fn failure(&self, selected: Option<i64>, fulltext: Option<&FulltextRow>) -> Option<String> {
        if !self.shows || self.is_extracting(selected) || self.is_showing(fulltext) {
            return None;
        }
        match fulltext.and_then(|full| full.error.as_deref()) {
            Some(error) => Some(format!("{FULL_ARTICLE_UNAVAILABLE}{error}")),
            None => self.notice.clone(),
        }
    }

    /// The bar above the page: a fetch that failed without a stored error (a stored one is
    /// already in the page).
    pub fn run_failure_notice(
        &self,
        selected: Option<i64>,
        fulltext: Option<&FulltextRow>,
    ) -> Option<String> {
        if !self.shows
            || self.is_extracting(selected)
            || fulltext.is_some_and(|full| full.error.is_some())
        {
            return None;
        }
        self.notice.clone()
    }
}

fn has_web_link(entry: &EntryRow) -> bool {
    entry.url.as_deref().is_some_and(is_web_url)
}

/// A fetch's outcome in the terminal reader's words; None when the article was saved.
pub fn extraction_sentence(
    entry_id: i64,
    outcome: &Result<ExtractOutcome, JobFailure>,
) -> Option<String> {
    match outcome {
        Ok(ExtractOutcome::Saved) => None,
        Ok(ExtractOutcome::Failed(sentence)) => Some(sentence.clone()),
        Ok(ExtractOutcome::NoUrl) => Some("This entry has no link.".to_string()),
        Ok(ExtractOutcome::UnknownEntry) => Some(format!("No entry #{entry_id}.")),
        Err(failure) => Some(failure.message()),
    }
}

/// The Full Article button's tooltip: what it does, or why it is off.
pub fn full_article_help(
    entry: Option<&EntryRow>,
    showing: bool,
    extracting: bool,
    failure: Option<&str>,
) -> String {
    let Some(entry) = entry else {
        return "Select an entry to read its full article".to_string();
    };
    if !has_web_link(entry) {
        return "This entry has no web page to fetch the full article from".to_string();
    }
    if extracting {
        return "Fetching the full article…".to_string();
    }
    if let Some(failure) = failure {
        let separator = if failure.ends_with('.') { " " } else { ". " };
        return format!("{failure}{separator}Click to try again");
    }
    if showing {
        "Showing the full article. Click to show the feed's text".to_string()
    } else {
        "Show the full article, fetched from the site".to_string()
    }
}

/// `text` as a JSON string literal, safe inside a `<script>` too: quotes, backslashes, control
/// characters, U+2028, U+2029 and `<` escaped. For app-world scripts only, never for markup:
/// it does not escape `&` or `"` the HTML way, so it is no HTML attribute value.
pub fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' | '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Space in the reader: scroll the page by most of a screen, unless the page the pane asked for
/// is not the one on screen yet ("spent": the key does nothing) or the page is at its end
/// ("bottom": move on).
pub fn page_down_script(expected_url: &str) -> String {
    format!(
        r#"(() => {{ if (location.href !== {expected}) {{ return "spent"; }} if (document.readyState === "loading") {{ return "spent"; }} const page = document.scrollingElement; if (page.scrollTop + window.innerHeight >= page.scrollHeight - 2) {{ return "bottom"; }} window.scrollBy({{ top: Math.round(window.innerHeight * 0.9), behavior: "smooth" }}); return "scrolled"; }})()"#,
        expected = js_string(expected_url)
    )
}

/// Swaps mosaic `index` and its original (a click on the mosaic): evaluates to whether the
/// original is now shown. App JavaScript; the page's own stays off.
pub fn toggle_image_script(index: u32) -> String {
    format!(
        r#"(() => {{ const box = document.getElementById("m" + {index}); if (!box) {{ return false; }} const original = !box.classList.contains("original"); box.classList.toggle("original", original); box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic; return original; }})()"#
    )
}

/// `i`: every image of the article both ways, originals while any mosaic shows, else mosaics.
/// Evaluates to how many boxes it swapped.
pub fn toggle_all_images_script() -> String {
    r#"(() => { const boxes = Array.from(document.querySelectorAll(".mosaic")); const original = boxes.some((box) => !box.classList.contains("original")); for (const box of boxes) { box.classList.toggle("original", original); box.querySelector("img").src = original ? box.dataset.original : box.dataset.mosaic; } return boxes.length; })()"#.to_string()
}

/// Flags every box showing the mosaic address `source` as failed (`data-failed`), so the
/// stylesheet shows the IMAGE line. Evaluates to nothing.
pub fn mark_failed_script(source: &str) -> String {
    format!(
        r#"(() => {{ const source = {source}; for (const box of document.querySelectorAll(".mosaic")) {{ const image = box.querySelector("img"); if (box.dataset.mosaic === source || (image && image.src === source)) {{ box.dataset.failed = ""; }} }} }})()"#,
        source = js_string(source)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    const ARTICLE: &str = "https://blog.example.org/2026/10/post.html";

    /// The Mac's hostile input, verbatim (ReaderTests.swift:67-77).
    const HOSTILE: &str = r#"<meta http-equiv="refresh" content="0;url=https://evil.example/">
<base href="https://evil.example/">
<p onclick="steal()" style="color:red">Hello <b onmouseover="x()">world</b></p>
<script>alert(1)</script><iframe src="https://evil.example/frame"></iframe>
<object data="x.swf"></object><embed src="x.swf">
<form action="https://evil.example/"><input name="q"><button>Go</button></form>
<a href="javascript:alert(1)">bad link</a> <a href=" JaVaScRiPt:alert(2)">bad two</a>
<img src="javascript:alert(3)"><img src="https://img.example.org/a.png" onerror="x()">
<svg><script>alert(4)</script></svg><style>body { display: none }</style>"#;

    fn entry(
        id: i64,
        title: Option<&str>,
        url: Option<&str>,
        summary: Option<&str>,
        content: Option<&str>,
    ) -> EntryDetail {
        EntryDetail {
            row: EntryRow {
                entry_id: id,
                feed_id: 7,
                feed_title: "Example Blog".into(),
                title: title.map(str::to_owned),
                author: None,
                url: url.map(str::to_owned),
                published_at: Utc.with_ymd_and_hms(2026, 10, 9, 8, 0, 0).unwrap(),
                is_read: false,
                is_starred: false,
                has_fulltext: false,
            },
            summary_html: summary.map(str::to_owned),
            content_html: content.map(str::to_owned),
            fulltext_html: None,
            fulltext_error: None,
            prefer_full_text: false,
        }
    }

    fn full(html: Option<&str>, byline: Option<&str>, error: Option<&str>) -> FulltextRow {
        FulltextRow {
            fetched_at: Utc.with_ymd_and_hms(2026, 10, 10, 9, 0, 0).unwrap(),
            html: html.map(str::to_owned),
            title: html.map(|_| "Full title".to_owned()),
            byline: byline.map(str::to_owned),
            error: error.map(str::to_owned),
        }
    }

    /// UTC+2, Paris in October.
    fn paris() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn base_page() -> ArticlePage {
        ArticlePage {
            entry: entry(
                1,
                Some("A title"),
                Some(ARTICLE),
                Some("<p>Feed summary</p>"),
                None,
            ),
            fulltext: None,
            show_full: false,
            style: ReaderStyle::default(),
            scheme: ColorScheme::Light,
            theme: ReaderTheme::NATIVE,
            subpage: None,
        }
    }

    fn page(e: EntryDetail, f: Option<FulltextRow>, show_full: bool) -> String {
        ArticlePage {
            entry: e,
            fulltext: f,
            show_full,
            ..base_page()
        }
        .html(&paris())
    }

    fn antiope(crt: bool) -> ReaderTheme {
        ReaderTheme {
            appearance: Appearance::Antiope,
            crt,
        }
    }

    // ArticlePageTests

    #[test]
    fn shows_the_full_article_only_when_toggled_and_cached() {
        let full = full(Some("<p>Full body</p>"), Some("Ada Lovelace"), None);
        let feed = || {
            entry(
                1,
                Some("A title"),
                Some(ARTICLE),
                Some("<p>Feed summary</p>"),
                Some("<p>Feed body</p>"),
            )
        };
        let on = page(feed(), Some(full.clone()), true);
        assert!(on.contains("Full body") && !on.contains("Feed body"));
        assert!(on.contains(r#"<span class="badge">full article</span>"#));
        assert!(on.contains("Ada Lovelace"));
        let off = page(feed(), Some(full), false);
        assert!(off.contains("Feed body") && !off.contains("Full body"));
        assert!(!off.contains(r#"<span class="badge">"#));
    }

    #[test]
    fn falls_back_from_content_to_summary() {
        let html = page(
            entry(
                1,
                Some("A title"),
                Some(ARTICLE),
                Some("<p>Feed summary</p>"),
                Some("  "),
            ),
            None,
            false,
        );
        assert!(html.contains("Feed summary"));
    }

    #[test]
    fn failed_extraction_keeps_the_feed_content_with_a_notice() {
        let failed = full(None, None, Some("HTTP 404."));
        let html = page(
            entry(
                1,
                Some("A title"),
                Some(ARTICLE),
                None,
                Some("<p>Feed body</p>"),
            ),
            Some(failed),
            true,
        );
        assert!(html.contains(r#"<p class="notice">Full article unavailable: HTTP 404.</p>"#));
        assert!(html.contains("Feed body"));
        let off = page(
            entry(
                1,
                Some("A title"),
                Some(ARTICLE),
                None,
                Some("<p>Feed body</p>"),
            ),
            Some(full(None, None, Some("HTTP 404."))),
            false,
        );
        assert!(!off.contains(r#"<p class="notice">"#));
    }

    /// Review Focus #1 of the Mac, rendering side: no url, no title, only summary_html.
    #[test]
    fn untitled_entry_without_link_renders() {
        let html = page(
            entry(1, None, None, Some("<p>Only a summary</p>"), None),
            None,
            false,
        );
        assert!(html.contains(r#"<h1 class="title">Untitled</h1>"#));
        assert!(html.contains("Only a summary"));
    }

    #[test]
    fn empty_entry_says_so() {
        let html = page(
            entry(1, Some("A title"), Some(ARTICLE), None, None),
            None,
            false,
        );
        assert!(html.contains(
            r#"<p class="empty">This entry has no text. Open it in the browser to read it.</p>"#
        ));
        let html = page(
            entry(1, Some("A title"), Some("mailto:a@b.example"), None, None),
            None,
            false,
        );
        assert!(html.contains(r#"<p class="empty">This entry has no text.</p>"#));
        assert!(html.contains(r#"<h1 class="title">A title</h1>"#)); // no link to a non-web address
    }

    #[test]
    fn escapes_entry_metadata() {
        let mut e = entry(
            1,
            Some("<script>x()</script> & co"),
            Some(ARTICLE),
            Some("<p>s</p>"),
            None,
        );
        e.row.feed_title = "<b>Feed</b>".into();
        e.row.author = Some("A <i>B</i>".into());
        let html = page(e, None, false);
        assert!(html.contains("&lt;script&gt;x()&lt;/script&gt; &amp; co"));
        assert!(
            html.contains("&lt;b&gt;Feed&lt;/b&gt;")
                && html.contains(r#"<p class="byline">A &lt;i&gt;B&lt;/i&gt;</p>"#)
        );
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn applies_style_scheme_and_policy() {
        let style = ReaderStyle {
            font: ReaderFont::SystemSerif,
            size: 20.0,
            width: LineWidth::Wide,
        };
        let html = ArticlePage {
            style,
            scheme: ColorScheme::Dark,
            ..base_page()
        }
        .html(&paris());
        for want in [
            r#"data-scheme="dark""#,
            "--reader-size: 20px",
            "--reader-measure: 80ch",
            r#"--reader-font: "Noto Serif""#,
            "default-src 'none'",
            "font-src ressac:",
            r#"<a href="https://blog.example.org/2026/10/post.html">A title</a>"#,
            r#"<p class="meta">Example Blog<span class="dot">·</span>October 9, 2026 at 10:00 AM</p>"#,
        ] {
            assert!(html.contains(want), "{want}");
        }
        assert!(!html.contains("@font-face"));
        assert!(
            base_page()
                .html(&paris())
                .contains(r#"data-scheme="light""#)
        );
    }

    #[test]
    fn embeds_the_mac_stylesheet() {
        assert!(READER_CSS.contains(r#":root[data-scheme="dark"]"#));
        assert!(base_page().html(&paris()).contains("--paper"));
    }

    #[test]
    fn the_page_is_the_mac_template_line_for_line() {
        let html = base_page().html(&paris());
        let head = format!(
            "<!doctype html>\n<html lang=\"en\" data-scheme=\"light\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<style>\n\n{READER_CSS}\n:root {{ --reader-font: system-ui, \"Noto Sans\", sans-serif; --reader-size: 17px; --reader-measure: 68ch; }}\n</style>\n</head>\n<body>\n<article>\n\n"
        );
        assert!(html.starts_with(&head), "{}", &html[..html.len().min(400)]);
        assert!(html.ends_with(concat!(
            "\n<p class=\"meta\">Example Blog<span class=\"dot\">·</span>October 9, 2026 at 10:00 AM</p>\n",
            "<h1 class=\"title\"><a href=\"https://blog.example.org/2026/10/post.html\">A title</a></h1>\n",
            "\n\n<div class=\"content\"><p>Feed summary</p></div>\n</article>\n</body>\n</html>"
        )));
    }

    #[test]
    fn reader_style_defaults_and_linux_font_stacks() {
        assert_eq!(
            ReaderStyle::default(),
            ReaderStyle {
                font: ReaderFont::SystemSans,
                size: 17.0,
                width: LineWidth::Medium
            }
        );
        assert!(css_family(ReaderFont::SystemSans).starts_with("system-ui"));
        assert!(css_family(ReaderFont::SystemSerif).starts_with("\"Noto Serif\""));
        assert!(css_family(ReaderFont::Newsreader).starts_with("\"Newsreader\""));
        assert!(css_family(ReaderFont::Atkinson).starts_with("\"Atkinson Hyperlegible Next\""));
        assert!(bundled_faces(ReaderFont::SystemSans).is_empty());
        assert_eq!(
            bundled_faces(ReaderFont::Newsreader)
                .iter()
                .map(|f| f.1)
                .collect::<Vec<_>>(),
            [false, true]
        );
        assert!(font_faces(ReaderFont::Newsreader).contains(r#"@font-face { font-family: "Newsreader"; font-style: normal; font-weight: 200 800; src: url("ressac:asset/Newsreader-Variable.ttf") format("truetype"); }"#));
        assert_eq!(font_faces(ReaderFont::SystemSerif), "");
        assert_eq!(
            font_faces(ReaderFont::Atkinson),
            concat!(
                r#"@font-face { font-family: "Atkinson Hyperlegible Next"; font-style: normal; font-weight: 200 800; src: url("ressac:asset/AtkinsonHyperlegibleNext-Variable.ttf") format("truetype"); }"#,
                "\n",
                r#"@font-face { font-family: "Atkinson Hyperlegible Next"; font-style: italic; font-weight: 200 800; src: url("ressac:asset/AtkinsonHyperlegibleNext-Italic-Variable.ttf") format("truetype"); }"#,
            )
        );
        for face in ASSETS.iter().filter(|a| a.mime.starts_with("font/")) {
            let path = format!(
                "{}/../../apps/macos/Ressac/Resources/Fonts/{}",
                env!("CARGO_MANIFEST_DIR"),
                face.file
            );
            assert!(std::path::Path::new(&path).is_file(), "{path}");
        }
    }

    #[test]
    fn the_style_follows_the_settings_of_the_look() {
        let settings = Settings {
            reader_font: ReaderFont::Newsreader,
            reader_size: 40.0,
            reader_size_antiope: 17.0,
            reader_width: LineWidth::Forty,
            ..Settings::default()
        };
        assert_eq!(
            ReaderStyle::of(&settings, Appearance::Native),
            ReaderStyle {
                font: ReaderFont::Newsreader,
                size: 28.0,
                width: LineWidth::Forty
            }
        );
        assert_eq!(ReaderStyle::of(&settings, Appearance::Antiope).size, 15.0);
    }

    #[test]
    fn assets_are_an_allowlist() {
        assert_eq!(
            asset("Newsreader-Variable.ttf").map(|a| a.mime),
            Some("font/ttf")
        );
        assert_eq!(asset("bedstead.otf").map(|a| a.mime), Some("font/otf"));
        assert_eq!(asset("proof.js").map(|a| a.mime), Some("text/javascript"));
        for bad in [
            "../Cargo.toml",
            "reader.css",
            "Newsreader-Variable.TTF",
            "",
            "fonts/bedstead.otf",
        ] {
            assert!(asset(bad).is_none(), "{bad}");
        }
        assert_eq!(page_url(12, 3), "ressac:page/12-3");
        assert_eq!(ColorScheme::parse("dark"), Some(ColorScheme::Dark));
        assert_eq!(ColorScheme::parse("Dark"), None);
        assert_eq!(ColorScheme::Light.as_str(), "light");
    }

    #[test]
    fn the_page_never_carries_a_script_or_a_data_font() {
        let html = page(
            entry(1, Some("t"), Some(ARTICLE), None, Some(HOSTILE)),
            None,
            false,
        );
        assert!(!html.contains("<script") && !html.contains("base64") && !html.contains("onerror"));
        assert!(html.contains(&format!(
            r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#
        )));
        let newsreader = ArticlePage {
            style: ReaderStyle {
                font: ReaderFont::Newsreader,
                ..ReaderStyle::default()
            },
            ..base_page()
        }
        .html(&paris());
        assert!(
            newsreader.contains(r#"url("ressac:asset/Newsreader-Variable.ttf")"#)
                && !newsreader.contains("data:font")
        );
    }

    // LinkPolicyTests

    #[test]
    fn only_our_own_page_loads_and_clicked_web_links_go_to_the_browser() {
        let page = "ressac:page/1-1";
        assert_eq!(
            decide(Navigation::Typed, page, true, page),
            LinkDecision::Allow
        );
        assert_eq!(
            decide(Navigation::Typed, PROOF_URL, true, PROOF_URL),
            LinkDecision::Allow
        );
        assert_eq!(
            decide(Navigation::Typed, "ressac:page/2-1", true, page),
            LinkDecision::Cancel
        );
        assert_eq!(
            decide(Navigation::Typed, page, false, page),
            LinkDecision::Cancel
        );
        assert_eq!(
            decide(
                Navigation::Typed,
                "https://example.org/a",
                true,
                "https://example.org/a"
            ),
            LinkDecision::Cancel
        );
        assert_eq!(
            decide(Navigation::LinkClicked, "https://example.org/a", true, page),
            LinkDecision::OpenExternally("https://example.org/a".into())
        );
        for (kind, url) in [
            (Navigation::LinkClicked, "javascript:alert(1)"),
            (Navigation::LinkClicked, "file:///etc/passwd"),
            (Navigation::LinkClicked, "mailto:a@b.example"),
            (Navigation::LinkClicked, "https:///nohost"),
            (Navigation::Other, "https://example.org/a"),
            (Navigation::FormSubmitted, "https://example.org/a"),
            (Navigation::Reload, page),
            (Navigation::Redirect, "https://example.org/a"),
            (Navigation::BackForward, page),
        ] {
            assert_eq!(
                decide(kind, url, true, page),
                LinkDecision::Cancel,
                "{kind:?} {url}"
            );
        }
        assert_eq!(Navigation::from_qt(0), Navigation::LinkClicked);
        assert_eq!(Navigation::from_qt(1), Navigation::Typed);
        assert_eq!(Navigation::from_qt(6), Navigation::Redirect);
        assert_eq!(Navigation::from_qt(42), Navigation::Other);
        assert_eq!(Navigation::from_qt(-1), Navigation::Other);
        assert_eq!(
            LinkDecision::OpenExternally("https://e.org/".into()).encode(),
            "open:https://e.org/"
        );
        assert_eq!(LinkDecision::Allow.encode(), "allow");
        assert_eq!(LinkDecision::Cancel.encode(), "cancel");
    }

    #[test]
    fn a_toggle_link_swaps_its_image() {
        assert_eq!(
            decide(
                Navigation::LinkClicked,
                "ressac-toggle:3",
                true,
                "ressac:page/1-1"
            ),
            LinkDecision::ToggleImage(3)
        );
        assert_eq!(toggle_index("RESSAC-TOGGLE:0"), Some(0));
        for bad in [
            "ressac-toggle:-1",
            "ressac-toggle:x",
            "ressac-toggle:",
            "ressac:page/1-1",
        ] {
            assert_eq!(toggle_index(bad), None, "{bad}");
        }
        assert_eq!(LinkDecision::ToggleImage(3).encode(), "toggle:3");
    }

    // ReaderViewTests, AntiopeReaderViewTests

    #[test]
    fn page_identity_ignores_read_and_star_flags() {
        let a = base_page();
        let mut b = a.clone();
        (b.entry.row.is_read, b.entry.row.is_starred) = (true, true);
        b.subpage = Some(SubpageLabel {
            page: Some(412),
            index: 1,
            count: 3,
        });
        let mut c = a.clone();
        c.show_full = true;
        let mut d = a.clone();
        d.theme = ReaderTheme {
            appearance: Appearance::Antiope,
            crt: true,
        };
        assert!(a.same_page(&b));
        assert!(!a.same_page(&c) && !a.same_page(&d));
        let mut e = a.clone();
        (e.entry.row.has_fulltext, e.entry.prefer_full_text) = (true, true);
        let mut f = a.clone();
        f.entry.row.feed_title = "Renamed".into();
        let mut g = a.clone();
        g.fulltext = Some(full(Some("<p>Full</p>"), None, None));
        assert!(a.same_page(&e));
        assert!(!a.same_page(&f) && !a.same_page(&g));
    }

    #[test]
    fn page_identity_ignores_the_subpage_but_not_the_theme() {
        let a = ArticlePage {
            scheme: ColorScheme::Dark,
            theme: antiope(false),
            subpage: Some(SubpageLabel {
                page: Some(412),
                index: 1,
                count: 2,
            }),
            ..base_page()
        };
        let mut b = a.clone();
        b.subpage = Some(SubpageLabel {
            page: Some(412),
            index: 1,
            count: 3,
        });
        let mut c = a.clone();
        c.theme = antiope(true);
        assert!(a.same_page(&b));
        assert!(!a.same_page(&c));
    }

    // FullArticleButtonTests

    #[test]
    fn the_tooltip_gives_the_state_and_why_the_button_is_off() {
        let row = entry(1, Some("A title"), Some(ARTICLE), None, None).row;
        let no_link = entry(1, Some("A title"), None, None, None).row;
        assert_eq!(
            full_article_help(None, false, false, None),
            "Select an entry to read its full article"
        );
        assert_eq!(
            full_article_help(Some(&no_link), false, false, None),
            "This entry has no web page to fetch the full article from"
        );
        assert_eq!(
            full_article_help(Some(&row), false, true, None),
            "Fetching the full article…"
        );
        assert_eq!(
            full_article_help(Some(&row), false, false, None),
            "Show the full article, fetched from the site"
        );
        assert_eq!(
            full_article_help(Some(&row), true, false, None),
            "Showing the full article. Click to show the feed's text"
        );
        assert_eq!(
            full_article_help(
                Some(&row),
                false,
                false,
                Some("Full article unavailable: HTTP 403.")
            ),
            "Full article unavailable: HTTP 403. Click to try again"
        );
        assert_eq!(
            full_article_help(
                Some(&row),
                false,
                false,
                Some("Full article unavailable: no answer")
            ),
            "Full article unavailable: no answer. Click to try again"
        );
    }

    // SubpageLabelTests

    #[test]
    fn the_label_reads_page_index_and_count() {
        assert_eq!(
            SubpageLabel {
                page: Some(412),
                index: 3,
                count: 20
            }
            .text(),
            "412 3/20"
        );
        assert_eq!(
            SubpageLabel {
                page: None,
                index: 1,
                count: 1
            }
            .text(),
            "1/1"
        );
    }

    // AntiopePageTests

    fn antiope_page(
        theme: ReaderTheme,
        subpage: Option<SubpageLabel>,
        style: ReaderStyle,
        title: Option<&str>,
    ) -> String {
        ArticlePage {
            entry: entry(1, title, Some(ARTICLE), Some("<p>Feed summary</p>"), None),
            style,
            scheme: ColorScheme::Dark,
            theme,
            subpage,
            ..base_page()
        }
        .html(&paris())
    }

    #[test]
    fn native_stays_as_it_was() {
        // The page embeds reader.css, whose text names the Antiope selectors: check the markup itself.
        let native = antiope_page(
            ReaderTheme::NATIVE,
            Some(SubpageLabel {
                page: Some(412),
                index: 3,
                count: 20,
            }),
            ReaderStyle::default(),
            Some("A title"),
        );
        assert!(native.contains(r#"<html lang="en" data-scheme="dark">"#));
        assert!(!native.contains(r#"<p class="subpage""#));
        assert!(!native.contains(r#"--teletext-font: "Bedstead""#));
        assert!(native.contains(
            r#"<h1 class="title"><a href="https://blog.example.org/2026/10/post.html">A title</a></h1>"#
        ));
    }

    #[test]
    fn antiope_marks_the_root_snaps_the_size_and_doubles_the_title() {
        let html = antiope_page(
            antiope(false),
            Some(SubpageLabel {
                page: Some(412),
                index: 3,
                count: 20,
            }),
            ReaderStyle {
                font: ReaderFont::SystemSerif,
                size: 17.0,
                width: LineWidth::Forty,
            },
            Some("A title"),
        );
        assert!(html.contains(r#"<html lang="en" data-scheme="dark" data-theme="antiope">"#));
        assert!(html.contains("--reader-size: 15px"));
        assert!(html.contains("--reader-measure: 40ch"));
        assert!(html.contains(r#"--teletext-font: "Bedstead", monospace; --font-pixel: 1.5; }"#));
        assert!(html.contains(r#"<p class="subpage" id="subpage">412 3/20</p>"#));
        assert!(html.contains(r#"<h1 class="title"><span class="dh-space" aria-hidden="true">A title</span><span class="dh-ink"><a href="https://blog.example.org/2026/10/post.html">A title</a></span></h1>"#));
        // The sub-page line exists even before the list knows the entry, so it can be filled in place.
        assert!(
            antiope_page(
                antiope(false),
                None,
                ReaderStyle::default(),
                Some("A title")
            )
            .contains(r#"<p class="subpage" id="subpage"></p>"#)
        );
        assert!(
            antiope_page(
                antiope(false),
                None,
                ReaderStyle::default(),
                Some("<b>&</b>")
            )
            .contains(
                r#"<span class="dh-space" aria-hidden="true">&lt;b&gt;&amp;&lt;/b&gt;</span>"#
            )
        );
        for (size, pixel) in [(20.0, "2"), (25.0, "2.5"), (30.0, "3")] {
            let style = ReaderStyle {
                size,
                ..ReaderStyle::default()
            };
            assert!(
                antiope_page(antiope(false), None, style, Some("t"))
                    .contains(&format!("--font-pixel: {pixel};"))
            );
        }
    }

    #[test]
    fn data_crt_only_for_antiope_with_the_box_checked() {
        let root = |theme| antiope_page(theme, None, ReaderStyle::default(), Some("A title"));
        assert!(
            root(ReaderTheme {
                appearance: Appearance::Native,
                crt: true
            })
            .contains(r#"<html lang="en" data-scheme="dark">"#)
        );
        assert!(
            root(antiope(false))
                .contains(r#"<html lang="en" data-scheme="dark" data-theme="antiope">"#)
        );
        assert!(
            root(antiope(true))
                .contains(r#"<html lang="en" data-scheme="dark" data-theme="antiope" data-crt>"#)
        );
        assert!(!ReaderTheme::NATIVE.shows_crt() && antiope(true).shows_crt());
        assert!(
            !ReaderTheme {
                appearance: Appearance::Native,
                crt: true
            }
            .shows_crt()
        );
    }

    // AntiopeStylesheetTests

    /// The `#RGB`…`#RRGGBBAA` colours of `css`, upper case.
    fn hex_colours(css: &str) -> std::collections::BTreeSet<String> {
        let bytes = css.as_bytes();
        let mut colours = std::collections::BTreeSet::new();
        for (start, _) in css.match_indices('#') {
            let digits = bytes[start + 1..]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            let boundary = bytes
                .get(start + 1 + digits)
                .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
            if (3..=8).contains(&digits) && boundary {
                colours.insert(css[start..start + 1 + digits].to_ascii_uppercase());
            }
        }
        colours
    }

    #[test]
    fn the_antiope_section_uses_only_the_eight_colours() {
        let start = READER_CSS.find("Antiope begin").unwrap() + "Antiope begin".len();
        let end = READER_CSS.find("Antiope end").unwrap();
        let section = &READER_CSS[start..end];
        let eight = [
            "#000000", "#FF0000", "#00FF00", "#FFFF00", "#0000FF", "#FF00FF", "#00FFFF", "#FFFFFF",
        ];
        assert_eq!(
            hex_colours(section),
            eight.iter().map(|c| c.to_string()).collect()
        );
        for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
            assert!(!section.contains(function), "{function}");
        }
        assert!(section.contains("--smoothing: none;"));
        assert!(section.contains("transform: scaleY(2);"));
    }

    #[test]
    fn the_crt_bleed_is_text_only_and_only_under_data_crt() {
        assert!(READER_CSS.contains(concat!(
            r#":root[data-theme="antiope"][data-crt] body {"#,
            "\n  text-shadow: 1px 0 rgba(255, 0, 0, 0.45), -1px 0 rgba(0, 0, 255, 0.45), 0 0 2px currentColor;"
        )));
    }

    #[test]
    fn bedstead_is_linked_in_antiope_only() {
        let face = r#"@font-face { font-family: "Bedstead"; font-style: normal; font-weight: 400; src: url("ressac:asset/bedstead.otf") format("opentype"); }"#;
        assert_eq!(teletext_face(), face);
        let style = ReaderStyle {
            font: ReaderFont::Newsreader,
            ..ReaderStyle::default()
        };
        let antiope = ArticlePage {
            style,
            theme: antiope(false),
            ..base_page()
        }
        .html(&paris());
        let native = ArticlePage {
            style,
            ..base_page()
        }
        .html(&paris());
        assert!(antiope.contains(face));
        assert!(!antiope.contains(r#"font-family: "Newsreader""#));
        assert!(!native.contains(face) && native.contains(r#"font-family: "Newsreader""#));
    }

    // FullArticleStateTests

    #[test]
    fn a_failed_extraction_leaves_the_button_off_and_the_notice_up() {
        let row = entry(
            6,
            Some("Walled"),
            Some("https://e.org/w"),
            Some("<p>s</p>"),
            None,
        )
        .row;
        let mut ft = FullText::default();
        ft.select(Some(&row));
        assert!(!ft.shows());
        assert_eq!(ft.request(&row, None), FullTextStep::Extract(6));
        assert!(
            ft.is_extracting(Some(6)) && !ft.is_extracting(Some(5)) && !ft.can_request(Some(&row))
        );
        ft.finished(6, Some(6), Ok(ExtractOutcome::Failed("HTTP 403.".into())));
        let stored = full(None, None, Some("HTTP 403."));
        assert!(!ft.is_showing(Some(&stored)) && ft.shows());
        assert_eq!(
            ft.failure(Some(6), Some(&stored)).as_deref(),
            Some("Full article unavailable: HTTP 403.")
        );
        assert_eq!(ft.run_failure_notice(Some(6), Some(&stored)), None); // the page shows the stored error itself
        assert_eq!(
            ft.run_failure_notice(Some(6), None).as_deref(),
            Some("Full article unavailable: HTTP 403.")
        );
        ft.select(Some(&row));
        assert!(!ft.shows() && ft.failure(Some(6), None).is_none());
    }

    #[test]
    fn the_button_is_on_only_with_the_full_article_shown() {
        let mut row = entry(3, Some("Kept"), Some("https://e.org/k"), None, None).row;
        row.has_fulltext = true;
        let stored = full(Some("<p>Full</p>"), None, None);
        let mut ft = FullText::default();
        ft.select(Some(&row));
        assert!(
            ft.shows()
                && ft.is_showing(Some(&stored))
                && ft.failure(Some(3), Some(&stored)).is_none()
        );
        assert_eq!(ft.request(&row, Some(&stored)), FullTextStep::ShowFeedText);
        assert!(!ft.shows() && !ft.is_showing(Some(&stored)));
        assert_eq!(ft.request(&row, Some(&stored)), FullTextStep::ShowStored);
        assert!(ft.shows() && ft.is_showing(Some(&stored)));
        let no_link = entry(4, Some("t"), None, None, None).row;
        assert_eq!(ft.request(&no_link, None), FullTextStep::Nothing);
        assert!(!ft.can_request(Some(&no_link)) && !ft.can_request(None));
    }

    #[test]
    fn an_extraction_busies_only_its_own_entry() {
        let one = entry(1, Some("a"), Some("https://e.org/a"), None, None).row;
        let two = entry(2, Some("b"), Some("https://e.org/b"), None, None).row;
        let mut ft = FullText::default();
        ft.select(Some(&one));
        assert_eq!(ft.request(&one, None), FullTextStep::Extract(1));
        assert_eq!(ft.request(&one, None), FullTextStep::Nothing); // already fetching
        ft.select(Some(&two));
        assert!(!ft.is_extracting(Some(2)) && ft.can_request(Some(&two)));
        assert_eq!(
            full_article_help(Some(&two), false, ft.is_extracting(Some(2)), None),
            "Show the full article, fetched from the site"
        );
        ft.select(Some(&one));
        assert!(ft.is_extracting(Some(1)));
        ft.select(Some(&two));
        ft.finished(1, Some(2), Err(JobFailure::Panicked));
        assert!(!ft.is_extracting(Some(1)));
        assert_eq!(ft.run_failure_notice(Some(2), None), None);
        assert_eq!(ft.failure(Some(2), None), None);
    }

    #[test]
    fn a_saved_extraction_sets_no_notice() {
        let row = entry(5, Some("t"), Some("https://e.org/t"), None, None).row;
        let mut ft = FullText::default();
        ft.select(Some(&row));
        assert_eq!(ft.request(&row, None), FullTextStep::Extract(5));
        ft.finished(5, Some(5), Ok(ExtractOutcome::Saved));
        assert!(ft.shows() && ft.failure(Some(5), None).is_none());
        assert!(ft.is_showing(Some(&full(Some("<p>Full</p>"), None, None))));
    }

    #[test]
    fn extraction_outcomes_read_like_the_terminal_reader() {
        assert_eq!(extraction_sentence(7, &Ok(ExtractOutcome::Saved)), None);
        assert_eq!(
            extraction_sentence(7, &Ok(ExtractOutcome::Failed("HTTP 404.".into()))).as_deref(),
            Some("HTTP 404.")
        );
        assert_eq!(
            extraction_sentence(7, &Ok(ExtractOutcome::NoUrl)).as_deref(),
            Some("This entry has no link.")
        );
        assert_eq!(
            extraction_sentence(7, &Ok(ExtractOutcome::UnknownEntry)).as_deref(),
            Some("No entry #7.")
        );
        assert_eq!(
            extraction_sentence(7, &Err(JobFailure::Panicked)).as_deref(),
            Some("The job stopped unexpectedly.")
        );
    }

    #[test]
    fn the_date_is_long_in_the_zone_given() {
        let at = Utc.with_ymd_and_hms(2026, 10, 9, 22, 5, 0).unwrap();
        assert_eq!(long_date(at, &paris()), "October 10, 2026 at 12:05 AM");
        assert_eq!(long_date(at, &Utc), "October 9, 2026 at 10:05 PM");
    }

    /// Review Focus 2.
    #[test]
    fn page_down_script_spends_the_key_until_the_page_is_ours() {
        let s = page_down_script("ressac:page/1-2");
        for want in [
            r#"location.href !== "ressac:page/1-2""#,
            r#"document.readyState === "loading""#,
            "page.scrollTop + window.innerHeight >= page.scrollHeight - 2",
            "Math.round(window.innerHeight * 0.9)",
            r#"return "bottom""#,
            r#"return "spent""#,
            r#"return "scrolled""#,
        ] {
            assert!(s.contains(want), "{want}");
        }
        assert_eq!(js_string(r#"a"b\c</script>"#), r#""a\"b\\c\u003c/script>""#);
        assert_eq!(js_string("x\u{2028}y\n"), r#""x\u2028y\n""#);
        assert_eq!(js_string("\u{2029}\t\u{1}\r"), r#""\u2029\t\u0001\r""#);
    }

    #[test]
    fn antiope_pages_draw_mosaics_and_native_pages_do_not() {
        let html = r#"<p><img src="https://img.example.org/a.png" alt="Cat"></p>"#;
        let draw = |theme| {
            ArticlePage {
                entry: entry(1, Some("T"), Some(ARTICLE), Some(html), None),
                theme,
                ..base_page()
            }
            .html(&paris())
        };
        let antiope_page = draw(antiope(false));
        assert!(antiope_page.contains(r#"<a class="mosaic" href="ressac-toggle:0" id="m0""#));
        assert!(!antiope_page.contains(r#"src="https://img.example.org"#));
        let native = draw(ReaderTheme::NATIVE);
        assert!(native.contains(r#"src="https://img.example.org/a.png" loading="lazy""#));
        assert!(!native.contains(r#"class="mosaic""#));
    }

    #[test]
    fn the_image_scripts_are_app_world_and_escaped() {
        let toggle = toggle_image_script(3);
        assert!(toggle.contains(r#"getElementById("m" + 3)"#), "{toggle}");
        assert!(toggle.contains("box.dataset.original") && toggle.contains("box.dataset.mosaic"));
        assert!(toggle.contains(r#"classList.toggle("original", original)"#));
        let all = toggle_all_images_script();
        assert!(
            all.contains(r#"querySelectorAll(".mosaic")"#) && all.contains("return boxes.length")
        );
        let source = r#"https://x/"a"#;
        let failed = mark_failed_script(source);
        assert!(failed.contains(&js_string(source)), "{failed}");
        assert!(failed.contains("dataset.failed"));
        assert!(!failed.contains(source), "the address only appears escaped");
    }

    #[test]
    fn the_proof_page_carries_every_kind_of_article_script() {
        for needle in [
            "<script>",
            r#"<script src="ressac:asset/proof.js">"#,
            "onload=",
            "onerror=",
            "href=\"javascript:",
        ] {
            assert!(PROOF_PAGE.contains(needle), "{needle}");
        }
        assert!(!PROOF_PAGE.contains("Content-Security-Policy")); // the view alone must hold
        assert!(RECETTE_PROBE.starts_with("(() => {") && RECETTE_PROBE.contains("\"data-app\""));
        assert!(PROOF_URL.starts_with(PAGE_PREFIX));
    }
}
