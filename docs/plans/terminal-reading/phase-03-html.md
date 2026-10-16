# Phase 3: an entry's HTML as styled terminal text (`tui/reader/html.rs`)

**Runs after:** phase 2 (`fetch_image`). It uses nothing from phases 1–2: it only needs `scraper`, `url`, ratatui and `output::printable`, all already in the workspace.

Read the plan index first (`docs/plans/2026-10-16-terminal-reading.md`). Global Constraints apply. This phase implements spec §7 "Text": headings, emphasis, lists, quotes, code, tables, numbered links with a links list at the end, image placeholders, wrapping to the column and to at most 72 characters, every feed string through `printable`, relative URLs resolved against the entry's link. It owns the `html.rs` half of **Review Focus 1** (escape sequences and emoji/CJK in article text).

**Files (4, plus insta snapshots):**
- Modify: `crates/ressac/Cargo.toml` (`scraper.workspace = true`)
- Modify: `crates/ressac/src/tui/mod.rs` (`pub mod reader;`)
- Create: `crates/ressac/src/tui/reader/mod.rs` (module root of the reader, `pub mod html;`)
- Create: `crates/ressac/src/tui/reader/html.rs` (the converter, tests inline)
- Snapshots (do not count): `crates/ressac/src/tui/reader/snapshots/ressac__tui__reader__html__tests__*.snap`

**Builds on (existing code, exact):**
- `crate::output::printable(text: &str) -> Cow<'_, str>` (`crates/ressac/src/output.rs:211`): every control character (ESC, BEL, C1 such as U+009B, but also tab and newline) becomes U+FFFD. Consequence used below: whitespace is collapsed **before** `printable`, so newlines and tabs never turn into U+FFFD.
- `scraper` 0.27.0: `Html::parse_fragment`, `ElementRef`, `ElementRef::wrap`, `ElementRef::children`, `ElementRef::text`, `ElementRef::select`, `Node::Text` (derefs to `&str`), `Selector::parse`. `scraper` does **not** re-export `ego_tree`, so the walker never names `NodeRef`: it iterates `el.children()` and matches `child.value()`, as `ressac_core::store::plain_text` does.
- ratatui 0.30.2: `Span::width()` (unicode width, so CJK counts 2 and emoji their real width), `Span { content: Cow<str>, style }` with public fields, `Style::new()`, `Style::add_modifier`, `Modifier`.
- `url::Url::parse`, `Url::join` (workspace `url = "2.5.8"`, already a dependency of `ressac`).

**Interface produced (exact, from the plan index):**

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef { pub url: String, pub alt: String, pub line: usize }
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedArticle { pub lines: Vec<Line<'static>>, pub links: Vec<String>, pub images: Vec<ImageRef> }
pub const MAX_TEXT_WIDTH: u16 = 72;
pub fn render_html(html: &str, width: u16, base_url: Option<&str>) -> RenderedArticle;
```

**Addition (deviation, reported):** `pub fn is_web_url(url: &str) -> bool` lives in `html.rs`; the reader state (phase 4b) reuses it to refuse non-web links, instead of a third copy of the garden's private helper.

**Rendering rules (one definition; the tests pin each):**

| HTML | Terminal |
|---|---|
| text | whitespace collapsed to single spaces, words through `printable`; entities decoded by `scraper` |
| `script`, `style`, `template`, `noscript`, `head`, `iframe`, `object`, `svg`, `button`, `form` | dropped with their content |
| `h1`–`h6` | own line, `BOLD \| UNDERLINED`, blank line before (unless first) and after |
| `p`, `figure`, `dl`, `details` | own block, blank line after |
| `div`, `section`, `article`, `header`, `footer`, `main`, `aside`, `figcaption`, `summary`, `address`, `dt`, `dd`, `li` (outside a list), `nav` | own block, no blank line |
| `br` | line break |
| `strong`, `b` / `em`, `i`, `cite` / `u`, `ins` / `s`, `del`, `strike` / `code`, `kbd`, `samp`, `tt` | `BOLD` / `ITALIC` / `UNDERLINED` / `CROSSED_OUT` / `REVERSED` |
| `a href` | link text `UNDERLINED`, then `[n]` glued after it when the href resolves to http(s); the same URL keeps its number; other schemes get no number |
| `ul` / `ol start=` | `• ` / `3. ` on the first line of each item, continuation lines indented by the marker's width; nested lists indent further; a blank line after a top-level list only |
| `blockquote` | every line prefixed `│ ` (nested quotes stack), blank line before and after |
| `pre` | lines kept as they are (tabs → 4 spaces, trailing `\r` dropped, through `printable`), not wrapped (the renderer clips), blank line before and after |
| `table` | one line per `tr`: cell texts joined with ` │ `, a row made only of `th` in `BOLD`, wrapped like a paragraph; blank line before and after |
| `hr` | a line of `─` across the text width |
| `img` | own line `[image: alt]` (or `[image]` without alt), `DIM`; an `ImageRef { url, alt, line }` when `src` resolves to http(s) (`line` = that line's index) |
| end | trailing blank lines removed; if any link: blank line, `Links` in `BOLD`, then `[1] url` per link |

Width: `min(width, 72)`, at least 10. A word wider than the line is cut at the line width (by character widths). Spaces between two words of the same style take that style (underlined link words stay joined), otherwise the default style. Adjacent spans of the same style are merged.

---

### Task 3.1: Module skeleton and the failing tests

- [ ] **Step 1: Dependency.** In `crates/ressac/Cargo.toml`, under `[dependencies]`, after `ratatui.workspace = true`:

```toml
scraper.workspace = true
```

(`scraper = "0.27.0"` is already in the root `[workspace.dependencies]`; no new crate.)

- [ ] **Step 2: Wire the module.** In `crates/ressac/src/tui/mod.rs`, after `pub mod state;`:

```rust
pub mod reader;
```

Create `crates/ressac/src/tui/reader/mod.rs`:

```rust
//! The reading interface (sub-project 2b): `ressac` without a subcommand and `ressac read`.
//! Its pieces arrive phase by phase and are wired into the CLI in phase 5b; until then they are
//! exercised by their tests only, hence the dead-code allowance (removed in phase 5b).
#![cfg_attr(
    not(test),
    allow(dead_code, reason = "wired into the event loop and the CLI in phase 5b")
)]

pub mod html;
```

- [ ] **Step 3: Write `html.rs` with the public items stubbed and the full test module.** Create `crates/ressac/src/tui/reader/html.rs`:

```rust
//! An entry's HTML as styled terminal lines (spec §7, Text): headings, emphasis, lists, quotes,
//! code, tables, numbered links and image placeholders, wrapped to the reader column and never
//! wider than `MAX_TEXT_WIDTH`. Every string that comes from a feed goes through
//! `output::printable`, so nothing in an article reaches the terminal as an escape sequence.

use ratatui::text::Line;

/// Reader text never runs wider than this, however wide the column (spec §7).
pub const MAX_TEXT_WIDTH: u16 = 72;

/// An image of the article: where its placeholder line is, and what to fetch (phase 6).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    pub url: String,
    pub alt: String,
    /// Index in `RenderedArticle::lines` of the `[image: …]` placeholder.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedArticle {
    pub lines: Vec<Line<'static>>,
    /// Numbered links, `links[0]` is `[1]`.
    pub links: Vec<String>,
    pub images: Vec<ImageRef>,
}

/// Whether `url` is an absolute http(s) address, the only kind the reader opens or fetches.
pub fn is_web_url(_url: &str) -> bool {
    unimplemented!()
}

/// The article for a column `width` cells wide; relative links and images resolve against
/// `base_url` (the entry's link).
pub fn render_html(_html: &str, _width: u16, _base_url: Option<&str>) -> RenderedArticle {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Modifier, Style};

    const BASE: &str = "https://example.org/blog/post.html";

    fn text(article: &RenderedArticle) -> Vec<String> {
        article
            .lines
            .iter()
            .map(|line| line.spans.iter().map(|s| &*s.content).collect::<String>())
            .collect()
    }

    fn lines(html: &str) -> Vec<String> {
        text(&render_html(html, 80, Some(BASE)))
    }

    fn style_of(article: &RenderedArticle, word: &str) -> Style {
        article
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.trim() == word)
            .unwrap_or_else(|| panic!("no span {word:?}"))
            .style
    }

    #[test]
    fn headings_paragraphs_and_inline_text() {
        assert_eq!(
            lines("<h1>Title</h1><p>Hello <b>bold</b> and <i>it</i>.</p><p>Second</p>"),
            ["Title", "", "Hello bold and it.", "", "Second"]
        );
    }

    #[test]
    fn whitespace_is_collapsed_and_entities_decoded() {
        assert_eq!(
            lines("<p>A&amp;B   &lt;tag&gt;\n\t next</p>"),
            ["A&B <tag> next"]
        );
    }

    #[test]
    fn hidden_elements_are_dropped() {
        assert_eq!(
            lines("<p>a<script>evil()</script><style>p{}</style>b</p><noscript>x</noscript>"),
            ["ab"]
        );
    }

    #[test]
    fn styles_follow_the_markup() {
        let article = render_html(
            "<h1>Title</h1><p><b>bold</b> <i>it</i> <a href='https://e.test/'>go</a> <code>x</code></p>",
            80,
            None,
        );
        let title = style_of(&article, "Title");
        assert!(title.add_modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
        assert!(style_of(&article, "bold").add_modifier.contains(Modifier::BOLD));
        assert!(style_of(&article, "it").add_modifier.contains(Modifier::ITALIC));
        assert!(style_of(&article, "go").add_modifier.contains(Modifier::UNDERLINED));
        assert!(style_of(&article, "x").add_modifier.contains(Modifier::REVERSED));
        assert_eq!(style_of(&article, "[1]"), Style::default());
    }

    #[test]
    fn lists_have_markers_and_nest() {
        assert_eq!(
            lines(
                "<ul><li>One</li><li>Two <ul><li>Inner</li></ul></li></ul>\
                 <ol start=\"3\"><li>Three</li><li>Four</li></ol>"
            ),
            ["• One", "• Two", "  • Inner", "", "3. Three", "4. Four"]
        );
    }

    #[test]
    fn list_items_wrap_under_their_text() {
        let article = render_html("<ul><li>alpha beta gamma delta</li></ul>", 14, None);
        assert_eq!(text(&article), ["• alpha beta", "  gamma delta"]);
    }

    #[test]
    fn blockquotes_have_a_gutter_and_wrap_inside_it() {
        let article = render_html(
            "<blockquote><p>alpha beta gamma delta epsilon</p></blockquote>",
            20,
            None,
        );
        assert_eq!(text(&article), ["│ alpha beta gamma", "│ delta epsilon"]);
    }

    #[test]
    fn preformatted_text_keeps_its_spacing() {
        assert_eq!(
            lines("<p>Code:</p><pre>fn main() {\n\tlet x = 1;\r\n}</pre>"),
            ["Code:", "", "fn main() {", "    let x = 1;", "}"]
        );
    }

    #[test]
    fn tables_are_rows_of_cells() {
        let article = render_html(
            "<table><tr><th>Name</th><th>Score</th></tr><tr><td>Ann</td><td>12</td></tr></table>",
            80,
            None,
        );
        assert_eq!(text(&article), ["Name │ Score", "Ann │ 12"]);
        assert!(style_of(&article, "Name").add_modifier.contains(Modifier::BOLD));
        assert!(!style_of(&article, "Ann").add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn a_rule_spans_the_text_width() {
        let article = render_html("<p>a</p><hr><p>b</p>", 20, None);
        assert_eq!(text(&article), ["a", "", "─".repeat(20).as_str(), "", "b"]);
    }

    #[test]
    fn links_are_numbered_resolved_and_listed() {
        let article = render_html(
            "<p>See <a href=\"/about\">about</a>, <a href=\"https://x.test/a b\">x</a> \
             and <a href=\"javascript:alert(1)\">js</a>.</p>",
            80,
            Some(BASE),
        );
        assert_eq!(
            text(&article),
            [
                "See about[1], x[2] and js.",
                "",
                "Links",
                "[1] https://example.org/about",
                "[2] https://x.test/a%20b",
            ]
        );
        assert_eq!(article.links, ["https://example.org/about", "https://x.test/a%20b"]);
    }

    #[test]
    fn the_same_link_keeps_its_number() {
        let article = render_html(
            "<p><a href='https://e.test/'>a</a> <a href='https://e.test/'>b</a></p>",
            80,
            None,
        );
        assert_eq!(text(&article)[0], "a[1] b[1]");
        assert_eq!(article.links.len(), 1);
    }

    #[test]
    fn a_relative_link_without_a_base_gets_no_number() {
        let article = render_html("<p><a href='/x'>x</a></p>", 80, None);
        assert_eq!(text(&article), ["x"]);
        assert!(article.links.is_empty());
    }

    #[test]
    fn images_are_placeholders_with_their_line() {
        let article = render_html(
            "<p>Before</p><img src=\"pic.jpg\" alt=\"A  cat\">\
             <p><img src=\"data:image/png;base64,xx\"> After</p>",
            80,
            Some(BASE),
        );
        assert_eq!(text(&article), ["Before", "", "[image: A cat]", "[image]", "After"]);
        assert_eq!(
            article.images,
            [ImageRef {
                url: "https://example.org/blog/pic.jpg".into(),
                alt: "A cat".into(),
                line: 2,
            }]
        );
        assert!(style_of(&article, "[image]").add_modifier.contains(Modifier::DIM));
    }

    /// Review Focus 1: escape sequences in titles, text, link text and alt never reach the
    /// terminal; emoji and CJK keep their width.
    #[test]
    fn control_characters_never_reach_the_terminal() {
        let article = render_html(
            "<h2>Café ☕ 東京 \u{1b}]0;x\u{7}</h2>\
             <p>evil\u{9b}31m text <a href=\"https://e.test/\">li\u{1b}nk</a></p>\
             <img src=\"https://e.test/i.png\" alt=\"al\u{1b}t\">",
            80,
            None,
        );
        let lines = text(&article);
        assert_eq!(lines[0], "Café ☕ 東京 \u{FFFD}]0;x\u{FFFD}");
        assert_eq!(lines[2], "evil\u{FFFD}31m text li\u{FFFD}nk[1]");
        assert_eq!(lines[4], "[image: al\u{FFFD}t]");
        for span in article.lines.iter().flat_map(|l| l.spans.iter()) {
            assert!(!span.content.chars().any(char::is_control), "{:?}", span.content);
        }
    }

    #[test]
    fn text_never_runs_wider_than_72_columns() {
        let html = format!("<p>{}</p>", "word ".repeat(30));
        let article = render_html(&html, 200, None);
        let lines = text(&article);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], vec!["word"; 14].join(" "));
        assert!(article.lines.iter().all(|line| line.width() <= 72));
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut_by_character_width() {
        let article = render_html("<p>東京東京東京東京</p>", 10, None);
        assert_eq!(text(&article), ["東京東京東", "京東京"]);
    }

    #[test]
    fn web_urls_are_http_or_https_only() {
        assert!(is_web_url("https://e.test/a"));
        assert!(is_web_url("http://e.test/"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("mailto:a@e.test"));
        assert!(!is_web_url("/relative"));
    }

    #[test]
    fn a_sample_article() {
        let html = "<h2>Morning notes</h2>\
            <p>The <b>tide</b> came in at <i>six</i>, see <a href=\"/tides\">the table</a>.</p>\
            <blockquote><p>Flux et reflux, the sea keeps its own calendar.</p></blockquote>\
            <ul><li>Bring a coat</li><li>Check the <a href=\"https://e.test/wind\">wind</a></li></ul>\
            <img src=\"/img/harbour.jpg\" alt=\"The harbour\">\
            <pre>high  06:12\nlow   12:30</pre>\
            <table><tr><th>Day</th><th>Coef</th></tr><tr><td>Fri</td><td>95</td></tr></table>";
        let article = render_html(html, 60, Some(BASE));
        insta::assert_snapshot!(text(&article).join("\n"));
    }
}
```

- [ ] **Step 4: Run the tests and see them fail.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd "$(git rev-parse --show-toplevel)"
INSTA_UPDATE=no cargo test -p ressac tui::reader::html
```

Expected: every test fails with `not implemented` (the stubs panic). The crate compiles.

### Task 3.2: The converter

- [ ] **Step 1: Replace the stubbed items of `html.rs` (everything above `#[cfg(test)]`) with:**

```rust
//! An entry's HTML as styled terminal lines (spec §7, Text): headings, emphasis, lists, quotes,
//! code, tables, numbered links and image placeholders, wrapped to the reader column and never
//! wider than `MAX_TEXT_WIDTH`. Every string that comes from a feed goes through
//! `output::printable`, so nothing in an article reaches the terminal as an escape sequence.

use std::sync::LazyLock;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use scraper::{ElementRef, Html, Node, Selector};
use url::Url;

use crate::output::printable;

/// Reader text never runs wider than this, however wide the column (spec §7).
pub const MAX_TEXT_WIDTH: u16 = 72;
/// Narrowest wrap width: a narrower column still gets lines this wide (the renderer clips).
const MIN_TEXT_WIDTH: usize = 10;
/// Elements whose content is never shown.
const HIDDEN: &[&str] = &[
    "script", "style", "template", "noscript", "head", "iframe", "object", "svg", "button", "form",
];
/// Blocks followed by a blank line.
const SPACED_BLOCKS: &[&str] = &["p", "figure", "dl", "details"];
/// Blocks that only start and end a line.
const BLOCKS: &[&str] = &[
    "div", "section", "article", "header", "footer", "main", "aside", "figcaption", "summary",
    "address", "dt", "dd", "li", "nav",
];

static ROWS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("tr").expect("`tr` is a valid selector"));

/// An image of the article: where its placeholder line is, and what to fetch (phase 6).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    pub url: String,
    pub alt: String,
    /// Index in `RenderedArticle::lines` of the `[image: …]` placeholder.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedArticle {
    pub lines: Vec<Line<'static>>,
    /// Numbered links, `links[0]` is `[1]`.
    pub links: Vec<String>,
    pub images: Vec<ImageRef>,
}

/// Whether `url` is an absolute http(s) address, the only kind the reader opens or fetches.
pub fn is_web_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// The article for a column `width` cells wide; relative links and images resolve against
/// `base_url` (the entry's link).
pub fn render_html(html: &str, width: u16, base_url: Option<&str>) -> RenderedArticle {
    let mut writer = Writer {
        width: usize::from(width.min(MAX_TEXT_WIDTH)).max(MIN_TEXT_WIDTH),
        base: base_url.and_then(|b| Url::parse(b).ok()),
        lines: Vec::new(),
        links: Vec::new(),
        images: Vec::new(),
        para: Vec::new(),
        space: false,
        bullet: None,
        last_blank: false,
    };
    let fragment = Html::parse_fragment(html);
    let root = Ctx::default();
    writer.children(fragment.root_element(), &root);
    writer.flush(&root);
    writer.trim_trailing_blank();
    writer.links_section();
    RenderedArticle {
        lines: writer.lines,
        links: writer.links,
        images: writer.images,
    }
}

/// One word of a paragraph, waiting to be wrapped.
struct Token {
    text: String,
    style: Style,
    /// Whether a space separated it from the previous word in the source.
    space: bool,
}

/// What the enclosing elements impose on the text inside them.
#[derive(Clone, Default)]
struct Ctx {
    style: Style,
    /// Prefix of every line (quote gutters, list indents).
    prefix: String,
    /// List nesting depth.
    depth: usize,
}

impl Ctx {
    fn styled(&self, modifier: Modifier) -> Ctx {
        Ctx {
            style: self.style.add_modifier(modifier),
            ..self.clone()
        }
    }
}

struct Writer {
    /// Text width, prefixes included.
    width: usize,
    base: Option<Url>,
    lines: Vec<Line<'static>>,
    links: Vec<String>,
    images: Vec<ImageRef>,
    /// The paragraph being collected.
    para: Vec<Token>,
    /// The source had whitespace since the last word.
    space: bool,
    /// A list marker for the first line of the next flushed paragraph.
    bullet: Option<String>,
    last_blank: bool,
}

impl Writer {
    fn children(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        for child in el.children() {
            match child.value() {
                Node::Text(text) => self.push_text(text, ctx.style),
                Node::Element(_) => {
                    if let Some(element) = ElementRef::wrap(child) {
                        self.element(element, ctx);
                    }
                }
                _ => {}
            }
        }
    }

    fn element(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        let name = el.value().name();
        if HIDDEN.contains(&name) {
            return;
        }
        match name {
            "br" => self.flush(ctx),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush(ctx);
                self.blank();
                self.children(el, &ctx.styled(Modifier::BOLD | Modifier::UNDERLINED));
                self.flush(ctx);
                self.blank();
            }
            "strong" | "b" => self.children(el, &ctx.styled(Modifier::BOLD)),
            "em" | "i" | "cite" => self.children(el, &ctx.styled(Modifier::ITALIC)),
            "u" | "ins" => self.children(el, &ctx.styled(Modifier::UNDERLINED)),
            "s" | "del" | "strike" => self.children(el, &ctx.styled(Modifier::CROSSED_OUT)),
            "code" | "kbd" | "samp" | "tt" => self.children(el, &ctx.styled(Modifier::REVERSED)),
            "a" => self.link(el, ctx),
            "img" => self.image(el, ctx),
            "ul" | "menu" => self.list(el, ctx, false),
            "ol" => self.list(el, ctx, true),
            "blockquote" => {
                self.flush(ctx);
                self.blank();
                let inner = Ctx {
                    prefix: format!("{}│ ", ctx.prefix),
                    ..ctx.clone()
                };
                self.children(el, &inner);
                self.flush(&inner);
                self.blank();
            }
            "pre" => self.pre(el, ctx),
            "table" => self.table(el, ctx),
            "hr" => {
                self.flush(ctx);
                self.blank();
                let rule = "─".repeat(self.width.saturating_sub(width_of(&ctx.prefix)));
                self.push_line(&ctx.prefix, vec![Span::raw(rule)]);
                self.blank();
            }
            _ if SPACED_BLOCKS.contains(&name) => {
                self.flush(ctx);
                self.children(el, ctx);
                self.flush(ctx);
                self.blank();
            }
            _ if BLOCKS.contains(&name) => {
                self.flush(ctx);
                self.children(el, ctx);
                self.flush(ctx);
            }
            _ => self.children(el, ctx),
        }
    }

    /// Words of `text`, whitespace collapsed first (so `printable` never sees a newline).
    fn push_text(&mut self, text: &str, style: Style) {
        let mut words = text.split_whitespace().peekable();
        if words.peek().is_none() {
            if !text.is_empty() {
                self.space = true;
            }
            return;
        }
        let mut space = self.space || text.starts_with(char::is_whitespace);
        for word in words {
            self.para.push(Token {
                text: printable(word).into_owned(),
                style,
                space,
            });
            space = true;
        }
        self.space = text.ends_with(char::is_whitespace);
    }

    /// A marker glued to the previous word (`[3]`); the source's spacing carries on after it.
    fn push_marker(&mut self, text: String) {
        let space = self.space;
        self.para.push(Token {
            text,
            style: Style::default(),
            space: false,
        });
        self.space = space;
    }

    fn link(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        self.children(el, &ctx.styled(Modifier::UNDERLINED));
        let Some(url) = el.value().attr("href").and_then(|href| self.resolve(href)) else {
            return;
        };
        let number = match self.links.iter().position(|known| *known == url) {
            Some(index) => index + 1,
            None => {
                self.links.push(url);
                self.links.len()
            }
        };
        self.push_marker(format!("[{number}]"));
    }

    fn image(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        self.flush(ctx);
        let alt = el.value().attr("alt").map(collapse).unwrap_or_default();
        let label = if alt.is_empty() {
            "[image]".to_string()
        } else {
            format!("[image: {alt}]")
        };
        let line = self.lines.len();
        self.push_line(
            &ctx.prefix,
            vec![Span::styled(label, Style::new().add_modifier(Modifier::DIM))],
        );
        if let Some(url) = el.value().attr("src").and_then(|src| self.resolve(src)) {
            self.images.push(ImageRef { url, alt, line });
        }
    }

    fn list(&mut self, el: ElementRef<'_>, ctx: &Ctx, ordered: bool) {
        self.flush(ctx);
        let mut number = el
            .value()
            .attr("start")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(1);
        let items = el
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|child| child.value().name() == "li");
        for item in items {
            let marker = if ordered {
                format!("{number}. ")
            } else {
                "• ".to_string()
            };
            let inner = Ctx {
                prefix: format!("{}{}", ctx.prefix, " ".repeat(width_of(&marker))),
                depth: ctx.depth + 1,
                ..ctx.clone()
            };
            self.bullet = Some(format!("{}{marker}", ctx.prefix));
            self.children(item, &inner);
            self.flush(&inner);
            self.bullet = None;
            number += 1;
        }
        if ctx.depth == 0 {
            self.blank();
        }
    }

    fn pre(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        self.flush(ctx);
        self.blank();
        let raw: String = el.text().collect();
        let body = raw.strip_suffix('\n').unwrap_or(&raw);
        for line in body.split('\n') {
            let line = line.trim_end_matches('\r').replace('\t', "    ");
            self.push_line(&ctx.prefix, vec![Span::raw(printable(&line).into_owned())]);
        }
        self.blank();
    }

    fn table(&mut self, el: ElementRef<'_>, ctx: &Ctx) {
        self.flush(ctx);
        self.blank();
        for row in el.select(&ROWS) {
            let cells: Vec<ElementRef<'_>> = row
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|cell| matches!(cell.value().name(), "td" | "th"))
                .collect();
            if cells.is_empty() {
                continue;
            }
            let header = cells.iter().all(|cell| cell.value().name() == "th");
            let style = if header {
                ctx.style.add_modifier(Modifier::BOLD)
            } else {
                ctx.style
            };
            let text = cells
                .iter()
                .map(|cell| collapse(&cell.text().collect::<String>()))
                .collect::<Vec<_>>()
                .join(" │ ");
            self.push_text(&text, style);
            self.flush(ctx);
        }
        self.blank();
    }

    /// The paragraph collected so far, wrapped to the width left by its prefix.
    fn flush(&mut self, ctx: &Ctx) {
        self.space = false;
        if self.para.is_empty() {
            return;
        }
        let tokens = std::mem::take(&mut self.para);
        let first = self.bullet.take().unwrap_or_else(|| ctx.prefix.clone());
        let rest = ctx.prefix.clone();
        let avail = self
            .width
            .saturating_sub(width_of(&first).max(width_of(&rest)))
            .max(4);
        for (index, spans) in wrap(&tokens, avail).into_iter().enumerate() {
            let prefix = if index == 0 { &first } else { &rest };
            self.push_line(prefix, spans);
        }
    }

    fn push_line(&mut self, prefix: &str, spans: Vec<Span<'static>>) {
        let mut all = Vec::with_capacity(spans.len() + 1);
        if !prefix.is_empty() {
            all.push(Span::raw(prefix.to_string()));
        }
        all.extend(merge(spans));
        self.lines.push(Line::from(all));
        self.last_blank = false;
    }

    fn blank(&mut self) {
        if !self.lines.is_empty() && !self.last_blank {
            self.lines.push(Line::default());
            self.last_blank = true;
        }
    }

    fn trim_trailing_blank(&mut self) {
        while self.last_blank {
            self.lines.pop();
            self.last_blank = self.lines.last().is_some_and(|line| line.spans.is_empty());
        }
    }

    fn links_section(&mut self) {
        if self.links.is_empty() {
            return;
        }
        self.blank();
        self.push_line(
            "",
            vec![Span::styled("Links", Style::new().add_modifier(Modifier::BOLD))],
        );
        let listed: Vec<Span<'static>> = self
            .links
            .iter()
            .enumerate()
            .map(|(index, url)| Span::raw(format!("[{}] {}", index + 1, printable(url))))
            .collect();
        for span in listed {
            self.push_line("", vec![span]);
        }
    }

    /// `href` as an absolute http(s) URL, resolved against the entry's link when relative.
    fn resolve(&self, href: &str) -> Option<String> {
        let href = href.trim();
        let url = match &self.base {
            Some(base) => base.join(href).ok()?,
            None => Url::parse(href).ok()?,
        };
        is_web_url(url.as_str()).then(|| url.to_string())
    }
}

/// Whitespace collapsed, then controls neutralised.
fn collapse(text: &str) -> String {
    printable(&text.split_whitespace().collect::<Vec<_>>().join(" ")).into_owned()
}

fn width_of(text: &str) -> usize {
    Span::raw(text).width()
}

/// Greedy wrapping to `avail` cells; a space takes the style of the words around it when both
/// share one, so underlined link words stay joined.
fn wrap(tokens: &[Token], avail: usize) -> Vec<Vec<Span<'static>>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0;
    let mut previous: Option<Style> = None;
    for token in tokens {
        for (piece_index, piece) in split_to_fit(&token.text, avail).into_iter().enumerate() {
            let width = width_of(&piece);
            let spaced = used > 0 && piece_index == 0 && token.space;
            let gap = usize::from(spaced);
            if used > 0 && used + gap + width > avail {
                lines.push(Vec::new());
                used = 0;
                previous = None;
            } else if spaced {
                let style = if previous == Some(token.style) {
                    token.style
                } else {
                    Style::default()
                };
                lines.last_mut().expect("never empty").push(Span::styled(" ", style));
                used += 1;
            }
            lines
                .last_mut()
                .expect("never empty")
                .push(Span::styled(piece, token.style));
            used += width;
            previous = Some(token.style);
        }
    }
    lines
}

/// `text` cut into pieces no wider than `avail` cells (a word wider than the line).
fn split_to_fit(text: &str, avail: usize) -> Vec<String> {
    if width_of(text) <= avail {
        return vec![text.to_string()];
    }
    let mut pieces = vec![String::new()];
    let mut used = 0;
    let mut buffer = [0; 4];
    for c in text.chars() {
        let width = width_of(c.encode_utf8(&mut buffer));
        if used > 0 && used + width > avail {
            pieces.push(String::new());
            used = 0;
        }
        pieces.last_mut().expect("never empty").push(c);
        used += width;
    }
    pieces
}

/// Adjacent spans of the same style become one.
fn merge(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let mut merged: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if last.style == span.style => last.content.to_mut().push_str(&span.content),
            _ => merged.push(span),
        }
    }
    merged
}
```

Notes for the implementer:
- `self.push_line(&ctx.prefix, …)` borrows `ctx`, not `self`, so it compiles; `self.width` is read before the mutable call.
- If html5ever normalises a test input differently than expected (for instance whitespace around `<li>`), fix the **implementation** first; change a test only when the expectation itself contradicts the rendering rules table above, and say so in the report.

- [ ] **Step 2: Run the unit tests (the snapshot test still fails, it has no snapshot yet).**

```bash
INSTA_UPDATE=no cargo test -p ressac tui::reader::html
```

Expected: every test passes except `a_sample_article`, which fails with a missing snapshot.

### Task 3.3: The sample article snapshot, then the gate

`cargo-insta` is not installed on this Mac: snapshots are created with `INSTA_UPDATE=always` and reviewed as files.

- [ ] **Step 1: Create and review the snapshot.**

```bash
INSTA_UPDATE=always cargo test -p ressac tui::reader::html::tests::a_sample_article
cat crates/ressac/src/tui/reader/snapshots/ressac__tui__reader__html__tests__a_sample_article.snap
```

Check by eye, line by line: `Morning notes` first; `The tide came in at six, see the table[1].`; a blank line; `│ Flux et reflux, the sea keeps its own calendar.` (wrapped under `│ ` at 60 columns if needed); `• Bring a coat`, `• Check the wind[2]`; `[image: The harbour]`; `high  06:12` and `low   12:30` with their double spaces; `Day │ Coef`, `Fri │ 95`; then `Links`, `[1] https://example.org/tides`, `[2] https://e.test/wind`. No line wider than 60. If anything differs, the implementation is wrong: fix it and regenerate.

- [ ] **Step 2: The gate.**

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
INSTA_UPDATE=no cargo test --workspace
git status --short
```

Expected: no formatting change left, no warning (the module's `allow(dead_code)` covers the not-yet-used public items), every test green; `git status` lists the four files and the one `.snap`, no `.snap.new`.

- [ ] **Step 3: Commit.**

```bash
git add crates/ressac/Cargo.toml crates/ressac/src/tui/mod.rs crates/ressac/src/tui/reader/mod.rs \
  crates/ressac/src/tui/reader/html.rs \
  crates/ressac/src/tui/reader/snapshots/ressac__tui__reader__html__tests__a_sample_article.snap
git commit -m "Reader: an entry's HTML as styled terminal text, links numbered, images as placeholders

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

## Gate 3

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `INSTA_UPDATE=no cargo test --workspace`: green.
- Review Focus 1 (article side): `control_characters_never_reach_the_terminal` passes.
- One commit on `terminal-reading`.
