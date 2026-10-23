# Phase B3: Native reading (B3a–B3h)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the KDE window reads like the Mac app: the article page in a `WebEngineView` served by a `ressac:` scheme (no `setHtml`, no base64 fonts), built by a Rust sanitiser and template with the Mac's allowlist and guards, article JavaScript never running while the app's own scripts do; links to the browser, the Full Article toggle and its notices; `j`, `k`, `m`, `s`, Space and the arrows routed by a window-level key filter before the reader or a list sees them; the 1-second auto-read; the menu bar and shortcuts table of spec §7.2; then the first VM checkpoint on marghe, which stops for Clément.

**Runs after:** B2d (`EntriesModel`, `SourcesModel`, `Runs`, `Live`, `ReadingPage.qml` with an empty `readerPane`, `Session { db, store, worker }`, `with_session`, the `--screenshot` walk and `tests/common`). Master plan `docs/plans/2026-10-23-linux.md` (rows B3a–B3d; Interfaces; Global Constraints: Reader, Shortcuts, Language, Gates; Review Focus 4). Spec §7.2, §7.3, §7.4, §8.4, §8.5, §10. Inventory §1.2, §1.3 (reader toolbar items), §2.5–§2.7, §7.3 (fonts), §9, §10.1 (ReaderTests, KeyboardTests, ReadingFixRound1Tests). Mac sources: `apps/macos/Ressac/Reading/ArticleTemplate.swift`, `ReaderView.swift`, `ReadingKeyRouter.swift`, `App/AppState.swift:330-575, 775-812`; `apps/macos/Ressac/Resources/reader.css` (reused in place).

**Architecture:** every decision is Rust in `ressac-ui`, tested on the Mac: `sanitize` (html5ever through `scraper`, the Mac's allowlist and guards), `article` (page template, content choice, page identity, link policy, full-article state, the app-world scripts), `keys` (Qt key decoding, focus classification from a chain of tokens, the Native map, held keys, which keys the menus never see) and `entries` navigation (next, previous, next unread over the floor). `ressac-kde` adds a `Reader` QML singleton whose pages and assets a C++ `QWebEngineUrlSchemeHandler` serves by calling an `extern "Rust"` function, `ReaderPane.qml` (`WebEngineView`, JavaScript off, app scripts in `WebEngineScript.ApplicationWorld`), a window event filter in the shim that asks Rust for the verb and calls `ReadingPage.perform(verb)`, and `MainMenu.qml`.

**Tech Stack:** Rust 2024 / 1.99; `scraper` 0.27.0 (html5ever 0.39) and `url` 2.5.8 (both already workspace dependencies, new to `ressac-ui`: no `notes.md` line); cxx 1.0.205 / cxx-qt 0.10.0; Qt 6.11.2 WebEngineQuick + **WebEngineCore** (new `qt_module`), Kirigami 6.31, Breeze icons; Fedora 44 container; marghe's Fedora 44 VM through `ssh willow@marghe.local` and `prlctl exec "Fedora Linux"`.

**Spec:** `docs/specs/2026-10-23-linux-design.md`, with `docs/specs/linux/mac-app-inventory.md` (binding strings and numbers).

## Deviations from the master plan

1. **Eight sub-phases, not four** (5-file cap; C++ and QML files count). The master rows map as: B3a → **B3a**; B3b → **B3b** (scheme and `Reader`) + **B3c** (reader pane, links, full article); B3c → **B3d** (`keys`, navigation, core `next_unread`) + **B3e** (key filter, bridges) + **B3f** (QML routing, auto-read) + **B3g** (menus); B3d (VM checkpoint 1) → **B3h**. The ledger gets rows B3e–B3h. Every master name is kept.
2. **`reader.css` is inlined in the page** (`include_str!` of the Mac file, as the Mac does) and only the fonts go through `ressac:asset/<file>`. No `reader-linux.css`: nothing in `reader.css` needs Chromium changes (`-apple-system` falls back to `sans-serif`, `-webkit-font-smoothing` is ignored); a Chromium-only rule found later goes in `article::LINUX_CSS` appended after `READER_CSS`, not a new file.
3. **The sanitiser's depth estimate follows html5ever, not libxml.** Measured (scratch crate, `scraper` 0.27.0): html5ever ignores `<td>` outside a table and inserts `tbody`, so the Mac estimator under-counts `<td><div>x</td>`×300 (estimate 2, html5ever depth 300) and `<table><tr><td>x</b>`×100 (300 vs 400). html5ever is quadratic on depth (`<div></p>`×40000: 11.7 s; `<b><div>x</b>`×40000 in 520 KB: 6.3 s), so the guard still matters; its rules are pinned in Task 1. The 1 s budget stays the backstop.
4. **URLs are WHATWG-normalised** (`url` crate): `https://example.org` becomes `https://example.org/` in sanitised `href`/`src`. Foundation-only details of the Mac tests are adapted; `https:///nohost` is refused by an explicit rule (the `url` crate would read host `nohost`).
5. **`entries::web_link` (B2) delegates to `sanitize::is_web_url`** so the toolbar, the menus and the link policy share one rule.
6. **Settings menu item lives in File** ("Settings…", Ctrl+Shift+,): a "Settings" top-level menu would need a mnemonic other than S and its single item would duplicate the title. Items whose feature comes later (Add Feed…, Import OPML…, Export OPML…, Settings…, Sea Chart) are present and disabled until B6a / B6b / B7 wire them.
7. **The transfer to marghe's VM is scripted as `apps/linux/dev/marghe.sh`** (spec §8.4 lets the builder pick): `git archive` of `HEAD` streamed through `ssh` and `prlctl exec` stdin, with a base64 chunk fallback.

## Validated in a throwaway container (`ressac-b3-plan`, image `ressac-fedora:44`, user `ressac`, removed)

A C++ Qt test program (QML `WebEngineView`, offscreen) and a cxx-qt 0.10 crate, all checks passed:

- **(a) Article JavaScript never runs, app scripts do.** `settings.javascriptEnabled: false` + `WebEngineView.runJavaScript(script, WebEngineScript.ApplicationWorld, callback)`: the app script runs (reads and writes the DOM, `window.scrollBy`, `document.fonts`, returns a value to the callback). `runJavaScript(script, callback)` (MainWorld, world id 0) returns `undefined` and does not run. A test page with inline `<script>`, `<script src="ressac:asset/evil.js">` (never even requested), `<body onload>`, `<img onerror>` and a `javascript:` link (clicked from the app world): **nothing ran, with or without CSP**; the control run (JavaScript on, no CSP) ran all five. JavaScript on + CSP `default-src 'none'` also blocks all five (the alternative works, but MainWorld then runs). Chosen: JavaScript off + CSP + sanitiser, app scripts in `WebEngineScript.ApplicationWorld` (world id 1).
- **(b) The `ressac:` scheme.** `QWebEngineUrlScheme scheme("ressac"); scheme.setSyntax(QWebEngineUrlScheme::Syntax::Path);` no flags, `QWebEngineUrlScheme::registerScheme(scheme)` **before** `QtWebEngineQuick::initialize()`; a `QWebEngineUrlSchemeHandler` subclass (`requestStarted(QWebEngineUrlRequestJob *job)` → `job->reply(mime, new QBuffer(job))` or `job->fail(QWebEngineUrlRequestJob::UrlNotFound)`) installed with `QQuickWebEngineProfile::defaultProfile()->installUrlSchemeHandler("ressac", handler)` after the application object exists. The handler runs **on the GUI thread**. `QQuickWebEngineProfile::defaultProfile()` is off-the-record in Qt 6.11 (`isOffTheRecord() == true`, empty `storageName`, `MemoryHttpCache`). A 3.95 MB page loaded; fonts and a stylesheet from `ressac:asset/…` load same-origin (origin `ressac:`, no CORS header) with CSP `font-src ressac:` / `style-src ressac:`; an `http:` image from a local server loads (no mixed-content block). `job->requestUrl().path()` is `page/hostile`. In the cxx-qt crate the handler calls an `extern "Rust"` function declared in `src/lib.rs`'s `#[cxx::bridge]`, whose header is `#include "<crate>/src/lib.cxx.h"` (here `ressac-kde/src/lib.cxx.h`); the page came back through the WebEngineView. **Linking needs `.qt_module("WebEngineCore")`** (with only `WebEngineQuick`: `ld.lld: error: undefined symbol: QWebEngineUrlScheme::registerScheme…`).
- **(c) Links.** `onNavigationRequested: (request) => …` (`request.url`, `request.navigationType`, `request.isMainFrame`, `request.accept()` / `request.reject()`): our own load is `WebEngineNavigationRequest.TypedNavigation` (1) in the main frame; a real click on an `https:` link is `LinkClickedNavigation` (0) and `reject()` leaves the page in place; a real click on `ressac-toggle:3` also arrives as `LinkClickedNavigation` **with the default `settings.unknownUrlSchemePolicy`**, and `reject()` opens nothing (a `QDesktopServices::setUrlHandler` spy saw no call). With `WebEngineSettings.DisallowUnknownUrlSchemes` the toggle click never reaches `navigationRequested`, and registering `ressac-toggle` as a scheme sends the page to `about:blank#blocked`: keep the default policy and do not register it. `target="_blank"` arrives as `onNewWindowRequested` (`request.requestedUrl`). `file:` links are refused by Chromium before any request. Opening outside: `Qt.openUrlExternally(url)` (= `QDesktopServices::openUrl`).
- **(d) Keys before WebEngine.** `QQuickWindow::installEventFilter(filter)`: the filter sees `QEvent::ShortcutOverride` then `QEvent::KeyPress` with `obj == window` **before** the focused `QtWebEngineCore::RenderWidgetHostViewQtDelegateItem`. Accepting the override (`event->accept(); return true;`) stops a QQC2 `Action { shortcut: "M" }` and a `Shortcut { sequence: "K" }` from firing while the `KeyPress` still arrives; returning `true` from the `KeyPress` keeps it from Chromium (Space and ↓ no longer scrolled). Returning `false` lets it through (a `TextField` typed `jm`; Chromium scrolled). Unmodified chords pass untouched (Ctrl+R still fired its `Action`). QML `Keys.priority: Keys.BeforeItem` on the `WebEngineView` never fires (focus is its internal delegate item); `Keys` on an ancestor sees only what Chromium declined, asynchronously after the renderer round trip (Space and ↓ never). From C++, `QMetaObject::invokeMethod(item, "perform", Qt::DirectConnection, Q_RETURN_ARG(QVariant, r), Q_ARG(QVariant, …))` calls an untyped QML function synchronously and returns its bool. Focus chains seen: `RenderWidgetHostViewQtDelegateItem < QQuickWebEngineView#reader < QQuickItem#readingPage < …`, `TextField_QMLTYPE_n#search < …` (inherits `QQuickTextInput`).
- **Space while loading.** With an image that never answers (a socket that never accepts): the document is `interactive`, `WebEngineView.loading` stays true, and the app-world page-down script scrolls; just after `url` is set to the next page and before it commits, `location.href` is still the old page, so comparing it with the expected URL spends the key. This replaces the Mac's commit tracking.

## File map

| File | Sub-phase / task | Responsibility |
|---|---|---|
| `crates/ressac-ui/Cargo.toml` | B3a T1 | `scraper.workspace = true`, `url.workspace = true` |
| `crates/ressac-ui/src/lib.rs` | B3a T1, T2; B3d T7 | `pub mod sanitize; pub mod article; pub mod keys;` |
| `crates/ressac-ui/src/sanitize.rs` (new) | B3a T1 | allowlist, guards, memo, big-stack worker, `is_web_url`, `declared_size` |
| `crates/ressac-ui/src/entries.rs` | B3a T1; B3d T6 | `web_link` delegates; `next_entry`, `previous_entry`, `next_unread`, `AUTO_READ_DELAY` |
| `crates/ressac-ui/src/article.rs` (new) | B3a T2 | template, assets, page URLs, identity, link policy, full-article state, help text, app-world scripts |
| `apps/linux/build.rs` | B3b T3 | `.qt_module("WebEngineCore")` |
| `apps/linux/cpp/shim.cpp` | B3b T3; B3e T8 | scheme registration and handler; key filter, `ressac_send_key` |
| `apps/linux/src/lib.rs` | B3b T3; B3e T8 | `extern "Rust"` `ressac_scheme_serve`, `ressac_route_key`; new shim calls |
| `apps/linux/src/reader.rs` (new) | B3b T3; B3c T5 | `Reader` singleton, page store, asset bytes, `ressac_scheme_serve` |
| `apps/linux/qml/ReaderPane.qml` (new) | B3c T4, T5 | `WebEngineView`, placeholder, link handling, page-down, notice, context menu |
| `apps/linux/qml/ReadingPage.qml` | B3c T4, T5; B3f T9 | reader in the third pane, reader toolbar items, `perform`, auto-read |
| `apps/linux/qml/Main.qml` | B3c T5; B3f T9; B3g T10 | walk steps; `menuBar: MainMenu` |
| `crates/ressac-core/src/store/library.rs` | B3d T6 | `Store::next_unread` |
| `crates/ressac-ui/src/keys.rs` (new) | B3d T7 | `Key`, `KeyFocus`, `Command`, `decode`, `classify`, `command`, `claims_override`, `verb`, `route` |
| `apps/linux/cpp/shim.h` | B3e T8 | `ressac_install_key_filter`, `ressac_send_key` |
| `apps/linux/src/entries.rs` | B3e T8 | `EntriesModel` navigation invokables and properties |
| `apps/linux/src/settings.rs` | B3e T8 | `AppSettings.resize(look, command)` |
| `apps/linux/qml/SourcesPane.qml`, `EntriesPane.qml` | B3f T9 | object names, focus, keyboard selection |
| `apps/linux/src/recette.rs` | B3f T9 | `Recette.key(key, text, modifiers)` |
| `apps/linux/qml/MainMenu.qml` (new) | B3g T10 | the menu bar and the shortcuts table |
| `apps/linux/dev/marghe.sh` (new), `notes.md`, `TODO.md` | B3h T11 | VM transfer, install, session run; verdict |
| tests (not counted) | | `apps/linux/tests/reader.rs`, `keys.rs`, `menus.rs`; `tests/reading.rs`, `tests/launch.rs` (shot-list assertions loosened in T5) |

Files per sub-phase: B3a 5, B3b 4, B3c 4, B3d 4, B3e 5, B3f 5, B3g 2, B3h 3.

## Interfaces produced (B4 onwards rely on these names)

**`ressac_ui::sanitize`** (Task 1):

```rust
pub const MAX_PARSED_BYTES: usize = 512 * 1024;
pub const MAX_NESTING_DEPTH: usize = 256;
pub const MAX_ATTRIBUTES_PER_TAG: usize = 2048;
pub const MAX_INPUT_BYTES: usize = 2_000_000;            // the plain-text fallback's cut
pub const PARSE_BUDGET: Duration = Duration::from_secs(1);
pub const MAX_WORKERS: usize = 2;
pub const MEMO_CAPACITY: usize = 32;
pub const WORKER_STACK_BYTES: usize = 256 << 20;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Images { Web }                                   // B8d adds Mosaics
pub fn sanitize(html: &str, base: Option<&str>, images: Images) -> String;          // PARSE_BUDGET, memoised
pub fn sanitize_within(html: &str, base: Option<&str>, images: Images, budget: Duration) -> String;
pub fn is_web_url(text: &str) -> bool;                    // http(s), a host, "//" then a non-'/' after the scheme
pub fn estimated_nesting_depth(html: &str, stopping_above: usize) -> usize;
pub fn most_attributes_on_a_tag(html: &str, stopping_above: usize) -> usize;
pub fn plain_text_fallback(html: &str) -> String;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize { pub width: u32, pub height: u32 }
pub fn declared_size(attributes: &[(String, String)]) -> Option<ImageSize>;
pub struct Gate { /* counting semaphore: Mutex<usize> + Condvar */ }
impl Gate { pub fn new(slots: usize) -> Arc<Gate>; }
pub fn shared_gate() -> &'static Arc<Gate>;               // MAX_WORKERS slots
pub fn on_large_stack<T: Send + 'static>(timeout: Duration, gate: &Arc<Gate>,
    work: impl FnOnce() -> T + Send + 'static) -> Option<T>;
pub fn memo_stats() -> (u64, u64);                        // (hits, misses), for tests
```

**`ressac_ui::article`** (Task 2):

```rust
pub const READER_CSS: &str = include_str!("../../../apps/macos/Ressac/Resources/reader.css");
pub const CSP: &str = "default-src 'none'; img-src http: https: data: ressac-mosaic:; style-src 'unsafe-inline'; font-src ressac:; base-uri 'none'; form-action 'none'";
pub const PAGE_PREFIX: &str = "ressac:page/";
pub const ASSET_PREFIX: &str = "ressac:asset/";
pub const PROOF_URL: &str = "ressac:page/proof";
pub const PROOF_PAGE: &str = /* exact copy, Task 2 */;
pub const PROOF_SCRIPT: &str = r#"document.documentElement.setAttribute("data-external", "ran");"#;
pub const RECETTE_PROBE: &str = /* exact copy, Task 2 */;
pub const NO_ENTRY_TITLE: &str = "No Entry Selected";
pub const NO_ENTRY_TEXT: &str = "Choose an entry in the list to read it here.";
pub const FULL_ARTICLE_UNAVAILABLE: &str = "Full article unavailable: ";
pub struct Asset { pub file: &'static str, pub mime: &'static str }
pub const ASSETS: [Asset; 6];   // Newsreader-Variable.ttf, Newsreader-Italic-Variable.ttf,
                                // AtkinsonHyperlegibleNext-Variable.ttf, AtkinsonHyperlegibleNext-Italic-Variable.ttf
                                // ("font/ttf"), bedstead.otf ("font/otf"), proof.js ("text/javascript")
pub fn asset(file: &str) -> Option<&'static Asset>;      // exact match only
pub fn page_url(entry_id: i64, revision: u64) -> String; // "ressac:page/<id>-<revision>"
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ColorScheme { Light, Dark }   // parse("light"|"dark"), as_str
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct ReaderTheme { pub appearance: Appearance, pub crt: bool }
impl ReaderTheme { pub const NATIVE: ReaderTheme; pub fn shows_crt(self) -> bool; }  // antiope && crt
#[derive(Debug, Clone, Copy, PartialEq)] pub struct ReaderStyle { pub font: ReaderFont, pub size: f64, pub width: LineWidth }
impl Default for ReaderStyle { /* SystemSans, 17, Medium */ }
impl ReaderStyle { pub fn of(settings: &Settings, look: Appearance) -> ReaderStyle; }  // size = effective_size of the look's key
pub fn css_family(font: ReaderFont) -> &'static str;
pub fn bundled_faces(font: ReaderFont) -> &'static [(&'static str, bool)]; // (file, italic)
pub fn font_faces(font: ReaderFont) -> String;
pub fn teletext_face() -> String;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubpageLabel { pub page: Option<u16>, pub index: usize, pub count: usize }
impl SubpageLabel { pub fn text(&self) -> String; }      // B8 adds of() / of_edition()
#[derive(Debug, Clone, PartialEq)]
pub struct ArticlePage { pub entry: EntryDetail, pub fulltext: Option<FulltextRow>, pub show_full: bool,
    pub style: ReaderStyle, pub scheme: ColorScheme, pub theme: ReaderTheme, pub subpage: Option<SubpageLabel> }
impl ArticlePage {
    pub fn html<Tz: TimeZone>(&self, tz: &Tz) -> String where Tz::Offset: std::fmt::Display;
    pub fn same_page(&self, other: &ArticlePage) -> bool; // ignores is_read, is_starred, has_fulltext, prefer_full_text, subpage
}
pub fn long_date<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String where Tz::Offset: std::fmt::Display; // "October 9, 2026 at 10:00 AM"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation { LinkClicked, Typed, FormSubmitted, BackForward, Reload, Other, Redirect }
impl Navigation { pub fn from_qt(value: i32) -> Navigation; }  // 0..=6 in that order, anything else Other
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkDecision { Allow, Cancel, OpenExternally(String), ToggleImage(u32) }
impl LinkDecision { pub fn encode(&self) -> String; }    // "allow" "cancel" "open:<url>" "toggle:<n>"
pub const TOGGLE_SCHEME: &str = "ressac-toggle";
pub fn decide(kind: Navigation, url: &str, main_frame: bool, expected_page: &str) -> LinkDecision;
pub fn toggle_index(url: &str) -> Option<u32>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullTextStep { ShowFeedText, ShowStored, Extract(i64), Nothing }
#[derive(Debug, Clone, Default)]
pub struct FullText { /* shows, extracting: BTreeSet<i64>, notice: Option<String> */ }
impl FullText {
    pub fn select(&mut self, entry: Option<&EntryRow>);  // shows = has_fulltext; notice cleared
    pub fn request(&mut self, entry: &EntryRow, fulltext: Option<&FulltextRow>) -> FullTextStep;
    pub fn finished(&mut self, entry_id: i64, selected: Option<i64>, outcome: Result<ExtractOutcome, JobFailure>);
    pub fn shows(&self) -> bool;
    pub fn is_extracting(&self, selected: Option<i64>) -> bool;
    pub fn is_showing(&self, fulltext: Option<&FulltextRow>) -> bool;
    pub fn can_request(&self, entry: Option<&EntryRow>) -> bool;      // a web link and not extracting it
    pub fn failure(&self, selected: Option<i64>, fulltext: Option<&FulltextRow>) -> Option<String>;
    pub fn run_failure_notice(&self, selected: Option<i64>, fulltext: Option<&FulltextRow>) -> Option<String>;
}
pub fn extraction_sentence(entry_id: i64, outcome: &Result<ExtractOutcome, JobFailure>) -> Option<String>;
pub fn full_article_help(entry: Option<&EntryRow>, showing: bool, extracting: bool, failure: Option<&str>) -> String;
pub fn js_string(text: &str) -> String;                   // a JSON string literal, safe inside <script> too
pub fn page_down_script(expected_url: &str) -> String;    // exact copy, Task 2
```

**`ressac_ui::entries`** additions (Task 6) and core (Task 6):

```rust
pub const AUTO_READ_DELAY: Duration = Duration::from_secs(1);
impl EntryList {
    pub fn next_entry(&mut self, store: &Store, selected: Option<i64>, today_start: DateTime<Utc>) -> ressac_core::Result<Option<i64>>;
    pub fn previous_entry(&self, selected: Option<i64>) -> Option<i64>;
    pub fn next_unread(&mut self, store: &Store, selected: Option<i64>, today_start: DateTime<Utc>) -> ressac_core::Result<Option<i64>>;
    pub fn auto_read_due(&self, selected: Option<i64>, entry_id: i64) -> bool;
}
// ressac_core::store
impl Store { pub fn next_unread(&self, list: &ListQuery, after: Option<i64>, today_start: DateTime<Utc>) -> Result<Option<ListCursor>>; }
```

**`ressac_ui::keys`** (Task 7; B8a extends it for Antiope):

```rust
pub mod qt { pub const KEY_ESCAPE: i32 = 0x0100_0000; pub const KEY_BACKSPACE: i32 = 0x0100_0003;
    pub const KEY_LEFT: i32 = 0x0100_0012; pub const KEY_UP: i32 = 0x0100_0013; pub const KEY_RIGHT: i32 = 0x0100_0014;
    pub const KEY_DOWN: i32 = 0x0100_0015; pub const KEY_SPACE: i32 = 0x20; pub const KEY_QUESTION: i32 = 0x3F;
    pub const SHIFT: u32 = 0x0200_0000; pub const CONTROL: u32 = 0x0400_0000; pub const ALT: u32 = 0x0800_0000;
    pub const META: u32 = 0x1000_0000; pub const KEYPAD: u32 = 0x2000_0000; }
pub mod focus { pub const TEXT: &str = "text"; pub const WEB: &str = "web"; pub const CONTROL: &str = "control";
    pub const POPUP: &str = "popup"; pub const ITEM: &str = "item"; pub const SOURCES_LIST: &str = "sourcesList";
    pub const ENTRIES_LIST: &str = "entriesList"; pub const READER_PANE: &str = "readerPane"; pub const READING_PAGE: &str = "readingPage"; }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key { Letter(char), Space, Up, Down, Left, Right, Digit(u8), Delete, Escape, Option(char), Question }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFocus { Sidebar, EntryList, Reader, Window, TextInput, Control }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command { NextEntry, PreviousEntry, Advance, ToggleRead, ToggleStar, FocusEntryList, FocusSidebar } // B8a adds Antiope's
pub fn decode(qt_key: i32, text: &str, modifiers: u32) -> Option<Key>;
pub fn classify(chain: &[&str]) -> KeyFocus;
pub fn command(key: Key, focus: KeyFocus, look: Appearance) -> Option<Command>;
pub fn claims_override(key: Key, focus: KeyFocus, look: Appearance) -> bool;
pub fn verb(command: Command, repeat: bool) -> &'static str;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route { pub claims_override: bool, pub verb: Option<&'static str> }
pub fn route(qt_key: i32, text: &str, modifiers: u32, repeat: bool, chain: &[&str], look: Appearance) -> Route;
```

Verbs (the only names QML switches on): `nextEntry`, `previousEntry`, `advance`, `advanceHeld`, `toggleRead`, `toggleReadHeld`, `toggleStar`, `toggleStarHeld`, `focusEntryList`, `focusSidebar`.

**`ressac-kde`:**

- **Shim** (`cpp/shim.{h,cpp}`, `src/lib.rs`): `ressac_init_webengine()` now also registers the `ressac` scheme (B8d adds `ressac-mosaic` there); `ressac_create_application()` now also installs the `ressac` handler on the default profile; new `ressac_install_key_filter()` (after `Main.qml` produced the window) and `ressac_send_key(key: i32, modifiers: u32, text: &str) -> bool`; `extern "Rust"`: `fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool` (in `reader.rs`, `use`d into `lib.rs`) and `fn ressac_route_key(key: i32, text: &str, modifiers: u32, repeat: bool, chain: &str, look: &str, verb: &mut String) -> bool` (in `lib.rs`: `chain` split on `|`, returns `claims_override`). `shim.cpp` includes `"ressac-kde/src/lib.cxx.h"`.
- **`Reader`** (QML singleton, `reader.rs`): WRITE properties `entryId: i64` (−1 none), `colorScheme: QString`, `look: QString`, `readerFont: QString`, `textSize: f64`, `readerWidth: QString`; READ properties (`NOTIFY = state_changed`) `hasEntry: bool`, `pageUrl: QString` (`""` without an entry; changes only when the page identity changes), `link: QString` (`""` = none), `isStarred: bool`, `proofUrl: QString` (`CONSTANT`, `PROOF_URL`; loaded only in recette mode); from Task 5 `isShowingFullText`, `isExtracting`, `canRequestFullText: bool`, `fullTextHelp: QString`, `runFailureNotice: QString`; invokables `reload()`, `probeScript() -> QString`, from Task 5 `decide(url: QString, kind: i32, mainFrame: bool, expected: QString) -> QString`, `requestFullText()`, `pageDownScript(expected: QString) -> QString`; signals `stateChanged()`, `banner(message: QString, showLog: bool)`, `wrote()`.
- **`ReaderPane.qml`** (`objectName: "readerPane"`): `function focusWeb()`, `function pageDown(entryId, done)` (`done(true)` = the key is spent or the page scrolled, `done(false)` = move on), `function runAppScript(script, done)` (B8: sub-page line, image swap), `function openExternally(url)`, `function copyText(text)`, `function clickTitleLink()` (walk hook).
- **`ReadingPage.qml`** (`objectName: "readingPage"`): `property string look: "native"` (B8 binds it), `function perform(verb)` → bool (B8 adds verbs), `focusSidebar()`, `focusEntries()`, `focusSearch()`, `focusName()` → `"sidebar" | "entries" | "reader" | "search" | ""`, `openInBrowser()`, `readonly property alias reader: readerPane`, `readonly property bool searchFocused`, `property string lastVerb`, `property bool lastHandled` (walk logs).
- **`EntriesModel`** additions: invokables `selectNext() -> bool`, `selectPrevious() -> bool`, `selectNextUnread() -> bool`, `selectFirstIfNone()`, `selectEntry(entryId: i64) -> bool`, `autoRead(entryId: i64)`; properties (`READ`) `canNavigate: bool` and `canToggle: bool` (`NOTIFY = list_changed` / `selection_changed`), `selectedRead: bool`, `selectedStarred: bool` (`NOTIFY = selection_changed`, emitted also when the selected row changes), `autoReadDelay: i32` (`CONSTANT`, 1000).
- **`AppSettings.resize(look: QString, command: QString) -> f64`** (`"bigger" | "smaller" | "actualSize"`; emits the changed size's notify).
- **`Recette.key(key: i32, text: QString, modifiers: i32) -> bool`** (recette only; `false` when inactive).
- **`MainMenu.qml`**: a `QQC2.MenuBar` with `property Item page` and action aliases `refresh`, `refreshAll`, `search`, `showUnreadOnly`, `biggerText`, `smallerText`, `actualSize`, `nextEntry`, `previousEntry`, `advance`, `toggleRead`, `toggleStar`, `markAllRead`, `fullArticle`, `openInBrowser`, and the disabled `addFeed`, `importOpml`, `exportOpml`, `settings`, `seaChart` (B6a, B6b, B7 set `enabled`/`onTriggered`), `function openMenu(name)`, `function closeMenus()`.

**Consumed:** B2's `EntriesModel` (`select`, `toggleRead`, `toggleStarred`, `markAllRead`, `selectedEntryId`, `selectedRow`, `searchText`, `unreadOnly`, `active`, `canMarkAllRead`, `selectionChanged`, `listChanged`, `wrote`), `SourcesModel` (`selectKey`, `select`, `selectedRow`, `isGarden`, `selectedKey`), `Runs` (`refreshSource`, `refreshAll`, `canRefresh`, `showBanner`), `ReadingPage.reloadAll()`, `with_session`, `Session.worker` (`Worker::short`), `runs::read_failure`, `worker::JobFailure`, `entries::{EntryList, UNTITLED, web_link}`, `tests/common` (`Run`, `copy_fixture`, `launch`, `assert_drawn`, `assert_clean`, `shots`); core `Store::{entry_detail, fulltext}`, `EntryDetail`, `FulltextRow`, `ListQuery`, `ListCursor`, `service::{ExtractOutcome, NullSink, Ressac::extract_entry}`; B1's `settings::{Settings, Appearance, ReaderFont, LineWidth, TextSizeCommand, effective_size}`, `QSettingsStore`, `shim::ressac_grab_window`.

**Decisions this phase pins:**

- **Three layers against article scripts:** the view (`javascriptEnabled: false`, `javascriptCanOpenWindows: false`), the CSP (`script-src` falls back to `default-src 'none'`), the sanitiser. App scripts only through `runJavaScript(…, WebEngineScript.ApplicationWorld, …)`, built in Rust (`js_string` for every value).
- **Pages by URL, not `setHtml`:** each new page identity bumps a revision, so `pageUrl` changes and the view navigates; marking read or starring never changes it. The page store keeps the current page and the proof page only, on the GUI thread (`thread_local!`).
- **Link policy:** allow only `Typed` + main frame + `url == expected` (the URL the pane set) under `PAGE_PREFIX`; `LinkClicked` → toggle, or open externally when `is_web_url`, else cancel; everything else cancelled; a new-window request is decided as a `LinkClicked` and never opens a window.
- **Keys:** the filter sits on the main `QQuickWindow` only (Settings and the Sea Chart window are other top-levels). It claims the override of every letter and Space outside a text field or popup, and of any other key that maps to a command, so single-key menu shortcuts are labels only in the reading window (the Mac's model: the router does the work); chords and mnemonics (Alt+F) pass. A claimed `KeyPress` whose verb `perform` declines continues to the focused item (the Garden's own keys, a focused button's Space).
- **Auto-read** is a QML `Timer` restarted when `selectedEntryId` changes, firing `EntriesModel.autoRead(id)`, which writes only if that entry is still selected and unread. `j`, `↓` and Space mark the entry they leave read at once; `k` and `↑` never.
- **Recette:** in `--screenshot` mode the reader first loads `PROOF_URL` and logs its probe, logs `reader: <url> <probe>` after each page load, and logs `reader: open <url>` instead of opening a browser.

**Step 0:** not triggered. `library.rs` (core), `entries.rs` (ui and bridge), `settings.rs` and `shim.cpp` only gain items; no file over 300 lines is restructured.

**Gate (end of each sub-phase):**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.orbstack/bin:$PATH"
cd /Users/willow/Sites/ressac
cargo fmt --all --check                                   # no output, exit 0
cargo clippy --workspace --all-targets -- -D warnings     # ends with "Finished", no warning
cargo test --workspace                                    # every "test result: ok."
node --test 'apps/macos/ChartTests/*.test.cjs'            # "# fail 0"
apps/linux/dev/dev.sh check                               # last line "check-linux: OK (fmt, clippy, test, chart, layout smoke)"
```

`cargo test -p ressac-ui` gains about 3 s (one worker test sleeps 1 s twice, the budget tests wait up to 1 s).

## Global Constraints

- **Reader:** article JavaScript never runs; sanitiser guards 512 KB, depth 256, 2048 attributes, 1 s, 32-entry memo; CSP as the Mac page's adapted to the `ressac:` scheme.
- **Shortcuts:** spec §7.2 table, verbatim: Ctrl+N, Ctrl+O / Ctrl+Shift+E, F5 and Ctrl+R / Ctrl+Shift+R, Ctrl+F, Ctrl+Alt+U, Ctrl++ / Ctrl+− / Ctrl+0, Ctrl+Shift+M, Ctrl+Shift+A, Ctrl+Return, Ctrl+Z (garden, B5), Ctrl+Shift+, , Alt+S / Alt+G / Alt+R (Antiope, B8), Ctrl+Q; letters `j k m s Space ? i`, digits, arrows unchanged. Menu mnemonics never use S, G, R ("G&o").
- **Language:** UI strings in English and identical to the Mac app's (inventory quotes).
- **Settings keys:** `reader.font` (`systemSans` default, `systemSerif`, `newsreader`, `atkinson`), `reader.size` (17, 12…28), `reader.size.antiope` (20), `reader.width` (`narrow`, `medium` default, `wide`, `forty`).
- **Fonts:** System sans (Noto Sans on Fedora), System serif (Noto Serif), Newsreader, Atkinson Hyperlegible Next (bundled, reused in place from `apps/macos/Ressac/Resources/Fonts/`).
- **Time:** functions depending on now or the zone take them as parameters.
- **Never:** the real database; the network in tests; `dnf install` on marghe's VM without Clément's first go-ahead; push, PR or merge.
- **Commits:** one per task on branch `linux`, messages ending with the two attribution lines.

## Review Focus

1. **Hostile article HTML** (master Review Focus 4: inline `<script>`, `onload=`, `javascript:` links, a 600 KB body, nesting 300 deep) → nothing executes, oversized or deep input falls back to plain text, the page still loads. Tests: Task 1 `hostile_html_is_inert_and_oversized_or_deep_input_is_plain_text`; Task 2 `the_page_never_carries_a_script_or_a_data_font`; Task 4 `article_scripts_never_run_and_a_hostile_600_kb_entry_still_loads` (the proof page, unsanitised, and the sanitised entry in the real view).
2. **Space while the next page loads or an image hangs** → Space scrolls the committed page, is spent while the new page has not replaced the old one, and at the bottom moves to the next unread entry; it never reads the previous page's position. Tests: Task 2 `page_down_script_spends_the_key_until_the_page_is_ours`; Task 9 walk step `14-space-in-reader`.
3. **`j`, `m`, Space typed in the search field, a text field or an open menu** → typing, no navigation, no read mark, no menu shortcut. Tests: Task 7 `the_focused_item_is_classified_by_its_chain`, `menus_never_see_plain_reading_keys_but_keep_their_chords`; Task 9 step `15-text-field` (and entry 4 still unread at step 17).
4. **Clicks on links of an article** (`javascript:`, `file:`, `mailto:`, a `ressac-toggle:` link in Native, `target="_blank"`, middle click) → only http(s) addresses reach the browser; the reader never navigates away from its page; nothing opens a window. Tests: Task 2 `only_our_own_page_loads_and_clicked_web_links_go_to_the_browser`, `a_toggle_link_swaps_its_image`; Task 5 walk step `09-link`.
5. **Marking read while reading** (the 1 s auto-read, `m`, a star) → the page is not reloaded and keeps its scroll. Tests: Task 2 `page_identity_ignores_read_and_star_flags`; Task 9 steps `17-auto-read` / `20-auto-read-done` (same `pageUrl`, entry read).

---

## B3a: sanitiser and page template (`ressac-ui`)

### Task 1 (B3a): `ressac_ui::sanitize`

**Files:**
- Modify: `crates/ressac-ui/Cargo.toml` (`scraper.workspace = true`, `url.workspace = true` under `[dependencies]`)
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod sanitize;`)
- Create: `crates/ressac-ui/src/sanitize.rs` (tests in its `mod tests`)
- Modify: `crates/ressac-ui/src/entries.rs` (`web_link` keeps its signature and returns the text unchanged when `sanitize::is_web_url(text)`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `ressac_ui::sanitize` as in the header.

Rules (port of `ArticleTemplate.swift:9-530`, the lists verbatim): dropped with content `applet audio base button canvas dialog embed form frame frameset head iframe input link math meta noscript object option script select source style svg template textarea title track video`; kept `a abbr article aside b bdi bdo blockquote br caption cite code col colgroup dd del details dfn div dl dt em figcaption figure footer h1 h2 h3 h4 h5 h6 header hr i img ins kbd li main mark nav ol p pre q rp rt ruby s samp section small span strong sub summary sup table tbody td tfoot th thead time tr u ul var wbr`; other elements unwrapped; attributes: global `dir lang title`, `a href`, `col`/`colgroup span`, `details open`, `img alt height src width`, `li value`, `ol reversed start type`, `td colspan rowspan`, `th colspan rowspan scope`, `time datetime`; `href` only http(s) after resolving against `base` (when `is_web_url(base)`), `src` http(s) or `data:image/*` but not `data:image/svg`; an `img` without a surviving `src` is dropped; `img` gets `loading="lazy"` last and `width`/`height` from `declared_size` of all its attributes when it has neither; text escapes `& < >`, attributes also `"`; comments and doctypes dropped; void elements `area base br col embed hr img input link meta param source track wbr` never closed. Parsing: `scraper::Html::parse_fragment`, then an own serialiser over the fragment root's children, depth-capped at `MAX_NESTING_DEPTH` (deeper nodes dropped). Guards in this order: `len > MAX_PARSED_BYTES`, `estimated_nesting_depth(html, 256) > 256`, `most_attributes_on_a_tag(html, 2048) > 2048` → `plain_text_fallback` of the first `MAX_INPUT_BYTES` (at a char boundary); else the parse on `on_large_stack(budget, shared_gate(), …)` (thread stack `WORKER_STACK_BYTES`), `None` → plain text, **not memoised**. Memo: LRU of `MEMO_CAPACITY` keyed by `(html, base, images)`, inputs over `MAX_PARSED_BYTES` not stored. `on_large_stack`: the worker takes a gate slot, skips its work when its caller already gave up, frees the slot; the caller waits up to `timeout`; a late result is dropped (the Mac's `Job`). `plain_text_fallback`, `most_attributes_on_a_tag`, `declared_size` (1…20 000 whole pixels from `width`/`height`, else from `style` `width:`/`height:` px; `data-*` never) are line-for-line ports.

`estimated_nesting_depth` (html5ever-faithful, conservative; linear over bytes; stops once past `stopping_above` and returns that depth): skip `<!-- … -->`, `<!…>`, `<?…>`; a tag name is ASCII letters/digits after `<` or `</`, the tag ends at the first `>`; void tags and self-closing `…/>` push nothing. **Start tag:** apply the Mac's optional-end-tag closes (`td`/`th` close an open `td`/`th` on top; `tr` pops `td th tr`; `dt`/`dd` close `dt`/`dd`; `li` closes a `p` then an `li`; `p` closes `p`; `option` closes `option`); `caption col colgroup tbody tfoot thead tr td th` are **ignored** (no push) when no `table` is open; when the top is `table`, `tr` first pushes an implied `tbody`, and `td`/`th` push implied `tbody` and `tr` as needed; then push. **End tag:** ignored when its name is not open; for a *special* element (`address article aside blockquote body caption center col colgroup dd details dialog dir div dl dt fieldset figcaption figure footer form h1 h2 h3 h4 h5 h6 header hgroup html li listing main menu nav ol p pre section summary table tbody td tfoot th thead tr ul`) pop down to it unless a scope boundary (`table td th caption html template applet object marquee`) lies above it; for any other element pop down to it only when no special element lies above it. The test below holds the estimate against html5ever's real depth.

- [ ] **Step 1: Write the failing tests** in `sanitize.rs` (`const ARTICLE: &str = "https://blog.example.org/2026/10/post.html";` and `fn clean(html: &str) -> String { sanitize(html, Some(ARTICLE), Images::Web) }`; `fn html5ever_depth(html: &str) -> usize` = deepest element count below the fragment's `html` root, walked iteratively):

```rust
#[test]
fn sanitizer_strips_active_content() {
    // The Mac's hostile input, verbatim (ReaderTests.swift:67-77).
    let c = clean(HOSTILE);
    for needle in ["<script", "<iframe", "<object", "<embed", "<form", "<input", "<button", "<meta", "<base",
        "<style", "<svg", "http-equiv", "onclick", "onmouseover", "onerror", "style=", "javascript:", "JaVaScRiPt",
        "evil.example", "alert("] {
        assert!(!c.contains(needle), "still contains {needle}");
    }
    assert!(c.contains("<p>Hello <b>world</b>"));
    assert!(c.contains("<a>bad link</a>"));
    assert!(c.contains(r#"<img src="https://img.example.org/a.png" loading="lazy">"#));
}

#[test]
fn keeps_text_entities_and_structure() {
    let c = clean("<h2>Part</h2><p>caf\u{e9} &eacute; &amp; <em>it</em></p><ul><li>one</li></ul><pre>  a\n    b</pre>");
    for want in ["<h2>Part", "<p>café é &amp; <em>it</em>", "<li>one</li>", "<pre>  a\n    b"] { assert!(c.contains(want), "{want}"); }
}

#[test]
fn resolves_relative_addresses_against_the_entry() {
    let c = clean(r##"<a href="/post/2">rel</a><a href="#notes">frag</a><img src="img/a.png"><a href="mailto:a@b.example">mail</a>"##);
    assert!(c.contains(r#"<a href="https://blog.example.org/post/2">rel</a>"#));
    assert!(c.contains(r#"<a href="https://blog.example.org/2026/10/post.html#notes">frag</a>"#));
    assert!(c.contains(r#"<img src="https://blog.example.org/2026/10/img/a.png" loading="lazy">"#));
    assert!(c.contains("<a>mail</a>"));
    assert_eq!(sanitize(r#"<a href="/post/2">rel</a><img src="/a.png">"#, None, Images::Web), "<a>rel</a>");
}

#[test]
fn unwraps_unknown_elements_and_filters_images() { /* the Mac's input; "kept text", no "<font", no "no source",
    the png data image kept with loading="lazy", no "svg" */ }

#[test]
fn escapes_text_and_attributes() {
    assert!(clean(r#"<p title='say "hi"'>1 &lt; 2 &amp;&amp; 3 &gt; 2</p>"#)
        .contains(r#"<p title="say &quot;hi&quot;">1 &lt; 2 &amp;&amp; 3 &gt; 2"#));
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let deep = "<div>".repeat(100_000) + "deep text" + &"</div>".repeat(100_000);
    let c = clean(&deep);
    assert!(c.contains("deep text") && !c.contains("<div"));
    let medium = "<blockquote>".repeat(300) + "mid text" + &"</blockquote>".repeat(300);
    assert_eq!(estimated_nesting_depth(&medium, usize::MAX), 300);
    assert_eq!(clean(&medium), "<p>mid text</p>");
    let ok = "<blockquote>".repeat(200) + "ok text" + &"</blockquote>".repeat(200);
    assert!(clean(&ok).contains("<blockquote>"));
    assert_eq!(estimated_nesting_depth("<p>a<br>b<img src=\"x>\"><!-- <div><div> --></p>", usize::MAX), 1);
    let stray = "<div></p>".repeat(40_000) + "stray text";
    assert!(estimated_nesting_depth(&stray, usize::MAX) > MAX_NESTING_DEPTH && clean(&stray).contains("stray text"));
    let quote = "<p x=a'b>".to_string() + &"<div>".repeat(60_000) + "quote text'>";
    assert!(estimated_nesting_depth(&quote, usize::MAX) > MAX_NESTING_DEPTH && clean(&quote).contains("quote text"));
    let loose = (0..500).map(|i| format!("<p>para {i}")).collect::<String>() + "<ul><li>a<li>b<li>c</ul>";
    assert!(estimated_nesting_depth(&loose, usize::MAX) < 10 && clean(&loose).contains("<ul>"));
    assert_eq!(plain_text_fallback("<p>one &amp; two</p>\n\n<p>three &lt;x&gt;</p>"), "<p>one &amp; two</p>\n<p>three &lt;x&gt;</p>");
}

#[test]
fn misnested_end_tags_are_bounded_by_the_size_cap_and_budget() {
    let started = Instant::now();
    assert!(!clean(&"<b><div></b>".repeat(100_000)).contains("<div"));
    assert!(started.elapsed() < Duration::from_millis(1500));
    let started = Instant::now();
    let _ = clean(&"<b><div>x</b>".repeat(15_000));
    assert!(started.elapsed() < Duration::from_millis(200));
}

#[test]
fn inline_end_tags_that_cannot_close_a_block_count_as_depth() {
    let html = "<b><div>x</b>".repeat(5_000);
    assert_eq!(estimated_nesting_depth(&html, usize::MAX), 10_000);
    let started = Instant::now();
    let c = clean(&html);
    assert!(started.elapsed() < Duration::from_millis(200) && !c.contains("<div") && c.contains('x'));
    assert_eq!(estimated_nesting_depth(&"<div><b>x</div>".repeat(1_000), usize::MAX), 2);
    assert_eq!(estimated_nesting_depth(&"<i><p>x</i>".repeat(1_000), usize::MAX), 2);
    assert_eq!(estimated_nesting_depth(&html, 256), 257);
}

#[test]
fn the_depth_estimate_bounds_html5ever() {
    let shapes = ["<b><div>x</b>".repeat(300), "<div><b>x</div>".repeat(300), "<i><p>x</i>".repeat(300),
        r#"<a href="https://e.org">x<p>y</a>"#.repeat(300), "<span><div>x</span>".repeat(300),
        "<td><div>x</td>".repeat(300), "<table><tr><td>x</b>".repeat(100), "<ul><li>x<p>y</ul>".repeat(300)];
    for html in &shapes {
        assert!(estimated_nesting_depth(html, usize::MAX) + 5 >= html5ever_depth(html), "{}", &html[..30]);
    }
    // 1500 seeded tag soups (xorshift, tags from the kept, dropped and table lists, random open/close/text).
    for soup in tag_soups(1_500, 0x5EED) {
        assert!(estimated_nesting_depth(&soup, usize::MAX) + 5 >= html5ever_depth(&soup), "{soup}");
    }
}

#[test]
fn a_tag_with_thousands_of_attributes_is_shown_as_text() { /* the Mac's test: 20_000 → count 20_000, stopping 2_048 →
    2_049, < 0.2 s, plain; tag(2_049) plain; tag(2_000) keeps "<b>kept</b>"; the quoted-values line == 4; no attributes == 0 */ }

#[test]
fn abandoned_workers_are_bounded_and_never_block_the_next_article() {
    // The Mac's test with Gate::new(MAX_WORKERS) and 1 s sleeping work: 0.3 s → None; 5 s → Some("done") with peak 2;
    // 0.3 s → None; three 0.05 s → None; 5 s → Some("done"); peak <= 2; runs <= 5.
}

#[test]
fn articles_up_to_512_kb_are_parsed() {
    let paragraph = format!("<p>{}<b>bold</b></p>\n", "Words of an ordinary long article. ".repeat(12));
    let big = paragraph.repeat(400_000 / paragraph.len());
    assert!(big.len() > 390_000);
    assert!(sanitize_within(&format!("{big}<!-- unique -->"), None, Images::Web, Duration::from_secs(10)).contains("<b>bold</b>"));
    let huge = paragraph.repeat(540_000 / paragraph.len());
    assert!(!sanitize_within(&huge, None, Images::Web, Duration::from_secs(10)).contains("<b>"));
}

#[test]
fn images_get_the_size_their_feed_gives() { /* the Mac's test, verbatim expectations (a: 600×400 from style;
    b, c: data-* ignored; d keeps width="50%"; e keeps width="320"; f none; no "style", no "data-"; declared_size
    cases: 600px/" 400 " → 600×400, "0" → None, percentages → None) */ }

#[test]
fn optional_end_tags_stay_on_the_parsed_path() { /* rows, defs, items ×150: estimate < 10 */ }

#[test]
fn sanitise_is_memoised() {
    let html = format!("<p>memo test {:?}</p>", std::time::SystemTime::now());
    let first = clean(&html);
    let (hits, _) = memo_stats();
    assert_eq!(clean(&html), first);
    assert!(memo_stats().0 >= hits + 1);
}

#[test]
fn a_timed_out_parse_is_not_memoised() {
    let html = "<p>budget <b>test</b> 4242</p>";
    let timed_out = sanitize_within(html, None, Images::Web, Duration::ZERO);
    assert!(timed_out.contains("4242") && !timed_out.contains("<b>"));   // the plain-text fallback
    assert!(sanitize(html, None, Images::Web).contains("<b>test</b>"));    // not remembered
}

#[test]
fn parse_runs_on_the_large_stack_thread() {
    let caller = std::thread::current().id();
    let worker = on_large_stack(Duration::from_secs(5), shared_gate(), || std::thread::current().id());
    assert!(matches!(worker, Some(id) if id != caller), "{worker:?}");
    assert!(clean("<p>hi</p>").contains("<p>hi"));
}

#[test]
fn survives_garbage() {
    assert_eq!(clean(""), "");
    assert!(clean("< not a tag > 3 < 4").contains("&lt; not a tag &gt; 3 &lt; 4"));
    assert!(!clean("<p>a\u{0}b<script>x</script>").contains("<script"));
    assert_eq!(clean("plain & text"), "plain &amp; text");
}

#[test]
fn only_web_addresses_are_web_urls() {
    for good in ["https://example.org/a", "HTTP://e.org/a", "http://127.0.0.1:9/x"] { assert!(is_web_url(good), "{good}"); }
    for bad in ["https:///nohost", "https://", "javascript:alert(1)", " JaVaScRiPt:alert(2)", "mailto:a@b.example",
        "file:///etc/passwd", "ressac-toggle:3", "//e.org/a", ""] { assert!(!is_web_url(bad), "{bad}"); }
}

/// Review Focus 1 (master Review Focus 4).
#[test]
fn hostile_html_is_inert_and_oversized_or_deep_input_is_plain_text() {
    let hostile = r#"<script>alert(1)</script><img src="x" onload="alert(2)" onerror="alert(3)"><a href="javascript:alert(4)">go</a><p>body</p>"#;
    let parsed = clean(hostile);
    for needle in ["<script", "onload", "onerror", "javascript:", "alert("] { assert!(!parsed.contains(needle), "{needle}"); }
    let big = hostile.repeat(600 * 1024 / hostile.len() + 1);
    assert!(big.len() > 600 * 1024);
    let plain = clean(&big);
    assert!(plain.starts_with("<p>") && !plain.contains("<script") && !plain.contains("<a ") && !plain.contains("<img"));
    let deep = "<div>".repeat(300) + hostile + &"</div>".repeat(300);
    let plain = clean(&deep);
    assert!(!plain.contains("<div") && !plain.contains("<script") && plain.contains("body"));
}
```

In `entries.rs`'s `mod tests`, extend `only_web_links_are_links` with `assert_eq!(web_link(Some("https:///nohost")), None);`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ressac-ui sanitize 2>&1 | tail -3`
Expected: compile error, `unresolved import` / `cannot find function sanitize` (the module is empty).

- [ ] **Step 3: Implement** `sanitize.rs` and the `entries::web_link` change as specified above.
- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p ressac-ui 2>&1 | grep "test result"`
Expected: `test result: ok.` (B2's `entries` tests included).

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/Cargo.toml crates/ressac-ui/src/lib.rs crates/ressac-ui/src/sanitize.rs crates/ressac-ui/src/entries.rs Cargo.lock
git commit -m "ressac-ui: article sanitiser (html5ever, the Mac allowlist and guards)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 2 (B3a): `ressac_ui::article`

**Files:**
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod article;`)
- Create: `crates/ressac-ui/src/article.rs` (tests in its `mod tests`)

**Interfaces:**
- Consumes: Task 1 (`sanitize`, `is_web_url`, `Images`); `settings::{Settings, Appearance, ReaderFont, LineWidth, effective_size}`; `entries::UNTITLED`, `entries::display_title`; `worker::JobFailure`; core `EntryDetail`, `EntryRow`, `FulltextRow`, `service::ExtractOutcome`.
- Produces: `ressac_ui::article` as in the header.

Values: `css_family`: SystemSans `system-ui, "Noto Sans", sans-serif`; SystemSerif `"Noto Serif", serif`; Newsreader `"Newsreader", "Noto Serif", serif`; Atkinson `"Atkinson Hyperlegible Next", system-ui, sans-serif`. `bundled_faces`: Newsreader `[("Newsreader-Variable.ttf", false), ("Newsreader-Italic-Variable.ttf", true)]`, Atkinson the same with `AtkinsonHyperlegibleNext-…`, the system fonts `[]`. A face: `@font-face { font-family: "<Newsreader | Atkinson Hyperlegible Next>"; font-style: <normal|italic>; font-weight: 200 800; src: url("ressac:asset/<file>") format("truetype"); }`, one per line. `teletext_face()`: `@font-face { font-family: "Bedstead"; font-style: normal; font-weight: 400; src: url("ressac:asset/bedstead.otf") format("opentype"); }`. The template is `ArticleTemplate.page` (`ArticleTemplate.swift:538-584`) line for line, with: `<style>` = (`teletext_face()` in Antiope, else `font_faces(style.font)`) + `READER_CSS` + `:root { --reader-font: <css_family>; --reader-size: <effective size as integer>px; --reader-measure: <max_ch>ch;[ --teletext-font: "Bedstead", monospace; --font-pixel: <size/10, shortest form: 1.5, 2, 2.5, 3>;] }`; meta joined by `<span class="dot">·</span>` = feed title, `long_date(published_at, tz)`, and `<span class="badge">full article</span>` when the full article's HTML is shown; the heading links to the entry's URL only when `is_web_url`; byline = the full article's byline when its HTML is shown, else the author; notice `<p class="notice">Full article unavailable: <error></p>` only when `show_full` and the stored row has an error; empty text `This entry has no text.` or, with a web link, `This entry has no text. Open it in the browser to read it.`; root attributes `lang="en" data-scheme="light|dark"[ data-theme="antiope"][ data-crt]`; the Antiope sub-page line and the double-height title exactly as the Mac's. `long_date`: `%B %-d, %Y at %-I:%M %p` in `tz`. `decide`: as pinned in the header. `FullText` is `AppState.requestFullText` / `fullTextFailure` / `entryDidChange` (`AppState.swift:341-355, 547-575, 781-800`) with `finished` setting the notice `FULL_ARTICLE_UNAVAILABLE + extraction_sentence(…)` when the outcome is not `Saved`, no notice is set yet and `selected == Some(entry_id)`. `extraction_sentence` = the terminal reader's `extract_outcome` sentences (`Failed(s)` → `s`, `NoUrl` → `This entry has no link.`, `UnknownEntry` → `No entry #<id>.`, `Err(f)` → `f.message()`). `full_article_help` = `ReaderView.fullArticleHelp` strings verbatim. `js_string`: `serde`-free JSON string literal escaping `"` `\` control characters, U+2028, U+2029 and `<` (as `<`).

Exact copies:

```rust
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

pub const RECETTE_PROBE: &str = r#"(() => { const d = document.documentElement; d.setAttribute("data-app", "ran"); const j = document.getElementById("jslink"); if (j) { j.click(); } return JSON.stringify({ app: d.getAttribute("data-app"), inline: d.getAttribute("data-inline"), external: d.getAttribute("data-external"), onload: d.getAttribute("data-onload"), onerror: d.getAttribute("data-onerror"), jslink: d.getAttribute("data-jslink"), scripts: document.querySelectorAll("script").length, paragraphs: document.querySelectorAll(".content p").length }); })()"#;

pub fn page_down_script(expected_url: &str) -> String {
    format!(r#"(() => {{ if (location.href !== {expected}) {{ return "spent"; }} if (document.readyState === "loading") {{ return "spent"; }} const page = document.scrollingElement; if (page.scrollTop + window.innerHeight >= page.scrollHeight - 2) {{ return "bottom"; }} window.scrollBy({{ top: Math.round(window.innerHeight * 0.9), behavior: "smooth" }}); return "scrolled"; }})()"#,
        expected = js_string(expected_url))
}
```

- [ ] **Step 1: Write the failing tests** in `article.rs` (helpers: `fn entry(id: i64, title: Option<&str>, url: Option<&str>, summary: Option<&str>, content: Option<&str>) -> EntryDetail` with feed title "Example Blog", `published_at` 2026-10-09T08:00:00Z, unread, unstarred; `fn full(html: Option<&str>, byline: Option<&str>, error: Option<&str>) -> FulltextRow`; `fn paris() -> FixedOffset` = UTC+2; `fn base_page() -> ArticlePage` = `entry(1, Some("A title"), Some(ARTICLE), Some("<p>Feed summary</p>"), None)`, no full text, `show_full` false, `ReaderStyle::default()`, `Light`, `ReaderTheme::NATIVE`, no subpage; `fn page(e: EntryDetail, f: Option<FulltextRow>, show_full: bool) -> String` = `base_page()` with those fields, `.html(&paris())`; `ARTICLE` and `HOSTILE` as in Task 1's tests, repeated as consts here). Port, by name, the Mac's `ArticlePageTests`, `LinkPolicyTests`, `ReaderViewTests`, `FullArticleButtonTests`, `SubpageLabelTests`, `AntiopePageTests` (`nativeStaysAsItWas`, `antiopeMarksTheRootSnapsTheSizeAndDoublesTheTitle`, `dataCRTOnlyForAntiopeWithTheBoxChecked`), `AntiopeStylesheetTests`, `bedsteadIsEmbeddedInAntiopeOnly` (as `bedstead_is_linked_in_antiope_only`), `pageIdentityIgnoresTheSubpageButNotTheTheme`, and the logic of `FullArticleStateTests`, with these Linux values:

```rust
#[test]
fn reader_style_defaults_and_linux_font_stacks() {
    assert_eq!(ReaderStyle::default(), ReaderStyle { font: ReaderFont::SystemSans, size: 17.0, width: LineWidth::Medium });
    assert!(css_family(ReaderFont::SystemSans).starts_with("system-ui"));
    assert!(css_family(ReaderFont::SystemSerif).starts_with("\"Noto Serif\""));
    assert!(css_family(ReaderFont::Newsreader).starts_with("\"Newsreader\""));
    assert!(css_family(ReaderFont::Atkinson).starts_with("\"Atkinson Hyperlegible Next\""));
    assert!(bundled_faces(ReaderFont::SystemSans).is_empty());
    assert_eq!(bundled_faces(ReaderFont::Newsreader).iter().map(|f| f.1).collect::<Vec<_>>(), [false, true]);
    assert!(font_faces(ReaderFont::Newsreader).contains(r#"@font-face { font-family: "Newsreader"; font-style: normal; font-weight: 200 800; src: url("ressac:asset/Newsreader-Variable.ttf") format("truetype"); }"#));
    assert_eq!(font_faces(ReaderFont::SystemSerif), "");
}

#[test]
fn assets_are_an_allowlist() {
    assert_eq!(asset("Newsreader-Variable.ttf").map(|a| a.mime), Some("font/ttf"));
    assert_eq!(asset("bedstead.otf").map(|a| a.mime), Some("font/otf"));
    assert_eq!(asset("proof.js").map(|a| a.mime), Some("text/javascript"));
    for bad in ["../Cargo.toml", "reader.css", "Newsreader-Variable.TTF", "", "fonts/bedstead.otf"] { assert!(asset(bad).is_none(), "{bad}"); }
    assert_eq!(page_url(12, 3), "ressac:page/12-3");
}

#[test]
fn applies_style_scheme_and_policy() {
    let style = ReaderStyle { font: ReaderFont::SystemSerif, size: 20.0, width: LineWidth::Wide };
    let html = ArticlePage { style, scheme: ColorScheme::Dark, ..base_page() }.html(&paris());
    for want in [r#"data-scheme="dark""#, "--reader-size: 20px", "--reader-measure: 80ch", r#"--reader-font: "Noto Serif""#,
        "default-src 'none'", "font-src ressac:", r#"<a href="https://blog.example.org/2026/10/post.html">A title</a>"#,
        r#"<p class="meta">Example Blog<span class="dot">·</span>October 9, 2026 at 10:00 AM</p>"#] {
        assert!(html.contains(want), "{want}");
    }
    assert!(!html.contains("@font-face"));
}

#[test]
fn the_page_never_carries_a_script_or_a_data_font() {
    let html = page(entry(1, Some("t"), Some(ARTICLE), None, Some(HOSTILE)), None, false);
    assert!(!html.contains("<script") && !html.contains("base64") && !html.contains("onerror"));
    assert!(html.contains(&format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#)));
    let newsreader = ArticlePage { style: ReaderStyle { font: ReaderFont::Newsreader, ..ReaderStyle::default() }, ..base_page() }.html(&paris());
    assert!(newsreader.contains(r#"url("ressac:asset/Newsreader-Variable.ttf")"#) && !newsreader.contains("data:font"));
}

#[test]
fn only_our_own_page_loads_and_clicked_web_links_go_to_the_browser() {
    let page = "ressac:page/1-1";
    assert_eq!(decide(Navigation::Typed, page, true, page), LinkDecision::Allow);
    assert_eq!(decide(Navigation::Typed, PROOF_URL, true, PROOF_URL), LinkDecision::Allow);
    assert_eq!(decide(Navigation::Typed, "ressac:page/2-1", true, page), LinkDecision::Cancel);
    assert_eq!(decide(Navigation::Typed, page, false, page), LinkDecision::Cancel);
    assert_eq!(decide(Navigation::Typed, "https://example.org/a", true, "https://example.org/a"), LinkDecision::Cancel);
    assert_eq!(decide(Navigation::LinkClicked, "https://example.org/a", true, page), LinkDecision::OpenExternally("https://example.org/a".into()));
    for (kind, url) in [(Navigation::LinkClicked, "javascript:alert(1)"), (Navigation::LinkClicked, "file:///etc/passwd"),
        (Navigation::LinkClicked, "mailto:a@b.example"), (Navigation::LinkClicked, "https:///nohost"), (Navigation::Other, "https://example.org/a"),
        (Navigation::FormSubmitted, "https://example.org/a"), (Navigation::Reload, page), (Navigation::Redirect, "https://example.org/a"),
        (Navigation::BackForward, page)] {
        assert_eq!(decide(kind, url, true, page), LinkDecision::Cancel, "{kind:?} {url}");
    }
    assert_eq!(Navigation::from_qt(0), Navigation::LinkClicked);
    assert_eq!(Navigation::from_qt(1), Navigation::Typed);
    assert_eq!(Navigation::from_qt(42), Navigation::Other);
    assert_eq!(LinkDecision::OpenExternally("https://e.org/".into()).encode(), "open:https://e.org/");
}

#[test]
fn a_toggle_link_swaps_its_image() {
    assert_eq!(decide(Navigation::LinkClicked, "ressac-toggle:3", true, "ressac:page/1-1"), LinkDecision::ToggleImage(3));
    assert_eq!(toggle_index("RESSAC-TOGGLE:0"), Some(0));
    for bad in ["ressac-toggle:-1", "ressac-toggle:x", "ressac-toggle:", "ressac:page/1-1"] { assert_eq!(toggle_index(bad), None, "{bad}"); }
    assert_eq!(LinkDecision::ToggleImage(3).encode(), "toggle:3");
}

#[test]
fn page_identity_ignores_read_and_star_flags() {
    let a = base_page();
    let mut b = a.clone();
    (b.entry.row.is_read, b.entry.row.is_starred) = (true, true);
    b.subpage = Some(SubpageLabel { page: Some(412), index: 1, count: 3 });
    let mut c = a.clone();
    c.show_full = true;
    let mut d = a.clone();
    d.theme = ReaderTheme { appearance: Appearance::Antiope, crt: true };
    assert!(a.same_page(&b));
    assert!(!a.same_page(&c) && !a.same_page(&d));
}

#[test]
fn the_tooltip_gives_the_state_and_why_the_button_is_off() {
    // ReaderView.fullArticleHelp's six strings, verbatim (ReaderTests.swift:427-440).
}

#[test]
fn a_failed_extraction_leaves_the_button_off_and_the_notice_up() {
    let row = entry(6, Some("Walled"), Some("https://e.org/w"), Some("<p>s</p>"), None).row;
    let mut ft = FullText::default();
    ft.select(Some(&row));
    assert!(!ft.shows());
    assert_eq!(ft.request(&row, None), FullTextStep::Extract(6));
    assert!(ft.is_extracting(Some(6)) && !ft.is_extracting(Some(5)) && !ft.can_request(Some(&row)));
    ft.finished(6, Some(6), Ok(ExtractOutcome::Failed("HTTP 403.".into())));
    let stored = full(None, None, Some("HTTP 403."));
    assert!(!ft.is_showing(Some(&stored)) && ft.shows());
    assert_eq!(ft.failure(Some(6), Some(&stored)).as_deref(), Some("Full article unavailable: HTTP 403."));
    assert_eq!(ft.run_failure_notice(Some(6), Some(&stored)), None); // the page shows the stored error itself
    assert_eq!(ft.run_failure_notice(Some(6), None).as_deref(), Some("Full article unavailable: HTTP 403."));
    ft.select(Some(&row));
    assert!(!ft.shows() && ft.failure(Some(6), None).is_none());
}

#[test]
fn the_button_is_on_only_with_the_full_article_shown() { /* has_fulltext: select → shows; request → ShowFeedText,
    shows false; request → ShowStored, shows true; is_showing(Some(full(Some("<p>Full</p>"), …))) */ }

#[test]
fn an_extraction_busies_only_its_own_entry() { /* Extract(1) started, select row 2 → is_extracting(Some(2)) false and
    can_request(Some(&row2)) true; finished(1, Some(2), Err(JobFailure::Panicked)) sets no notice for entry 2 */ }

#[test]
fn extraction_outcomes_read_like_the_terminal_reader() {
    assert_eq!(extraction_sentence(7, &Ok(ExtractOutcome::Saved)), None);
    assert_eq!(extraction_sentence(7, &Ok(ExtractOutcome::Failed("HTTP 404.".into()))).as_deref(), Some("HTTP 404."));
    assert_eq!(extraction_sentence(7, &Ok(ExtractOutcome::NoUrl)).as_deref(), Some("This entry has no link."));
    assert_eq!(extraction_sentence(7, &Ok(ExtractOutcome::UnknownEntry)).as_deref(), Some("No entry #7."));
    assert_eq!(extraction_sentence(7, &Err(JobFailure::Panicked)).as_deref(), Some("The job stopped unexpectedly."));
}

/// Review Focus 2.
#[test]
fn page_down_script_spends_the_key_until_the_page_is_ours() {
    let s = page_down_script("ressac:page/1-2");
    for want in [r#"location.href !== "ressac:page/1-2""#, r#"document.readyState === "loading""#,
        "page.scrollTop + window.innerHeight >= page.scrollHeight - 2", "Math.round(window.innerHeight * 0.9)",
        r#"return "bottom""#, r#"return "spent""#, r#"return "scrolled""#] {
        assert!(s.contains(want), "{want}");
    }
    assert_eq!(js_string(r#"a"b\c</script>"#), r#""a\"b\\c</script>""#);
    assert_eq!(js_string("x\u{2028}y\n"), r#""x y\n""#);
}

#[test]
fn the_proof_page_carries_every_kind_of_article_script() {
    for needle in ["<script>", r#"<script src="ressac:asset/proof.js">"#, "onload=", "onerror=", "href=\"javascript:"] {
        assert!(PROOF_PAGE.contains(needle), "{needle}");
    }
    assert!(!PROOF_PAGE.contains("Content-Security-Policy")); // the view alone must hold
    assert!(RECETTE_PROBE.starts_with("(() => {") && RECETTE_PROBE.contains("\"data-app\""));
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui article 2>&1 | tail -3` → compile error (`article` unresolved).
- [ ] **Step 3: Implement** `article.rs` as specified.
- [ ] **Step 4: Run them to verify they pass** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B3a ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/lib.rs crates/ressac-ui/src/article.rs
git commit -m "ressac-ui: reader page template, link policy, full-article state, app-world scripts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3b: the `ressac:` scheme and the `Reader` object

### Task 3 (B3b): scheme registration, handler, `Reader` singleton, page store

**Files:**
- Modify: `apps/linux/build.rs` (`.qt_module("WebEngineCore")` after `WebEngineQuick`)
- Modify: `apps/linux/cpp/shim.cpp` (`#include "ressac-kde/src/lib.cxx.h"`, `<QtWebEngineCore/QWebEngineUrlScheme>`, `<QtWebEngineCore/QWebEngineUrlSchemeHandler>`, `<QtWebEngineCore/QWebEngineUrlRequestJob>`, `<QtWebEngineQuick/QQuickWebEngineProfile>`, `<QtCore/QBuffer>`)
- Modify: `apps/linux/src/lib.rs` (`mod reader; use reader::ressac_scheme_serve;`; in the `shim` bridge: `extern "Rust" { fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool; }`)
- Create: `apps/linux/src/reader.rs` (the `Reader` bridge, `thread_local!` page store, asset bytes, `ressac_scheme_serve`, unit tests)

**Interfaces:**
- Consumes: Tasks 1–2; `with_session`.
- Produces: the `Reader` singleton's B3b properties and invokables (header); `ressac_scheme_serve`. `proofUrl` is always `PROOF_URL`; `ReaderPane` loads it only when `Recette.active` (Task 4), so `reader.rs` needs nothing from `recette.rs`. Serving the inert proof page outside recette is harmless.

Behaviour:
- `ressac_init_webengine()`: before `QtWebEngineQuick::initialize()`, register `ressac` (`Syntax::Path`, no flags). `ressac_create_application()`: after `new QApplication`, `QQuickWebEngineProfile::defaultProfile()->installUrlSchemeHandler("ressac", new RessacSchemeHandler(qApp))`; `qWarning("ressac-kde: the default web profile is not off the record")` when `!isOffTheRecord()`. `RessacSchemeHandler::requestStarted`: `ressac_scheme_serve(job->requestUrl().toString(QUrl::FullyEncoded).toStdString(), mime, body)`; true → `QBuffer` parented to the job holding a copy of `body`, `job->reply(mime, buffer)`; false → `job->fail(QWebEngineUrlRequestJob::UrlNotFound)`.
- `ressac_scheme_serve`: `PROOF_URL` → `PROOF_PAGE`; other `PAGE_PREFIX` URLs → the page store (`text/html; charset=utf-8`); `ASSET_PREFIX` + a file that `article::asset` knows → its bytes (`include_bytes!("../../macos/Ressac/Resources/Fonts/<file>")`, `PROOF_SCRIPT` for `proof.js`) and its mime; anything else false. Never panics (FFI).
- `Reader` state: `ReaderRust { entry_id: i64, color_scheme: QString, look: QString, reader_font: QString, text_size: f64, reader_width: QString, detail: Option<EntryDetail>, fulltext: Option<FulltextRow>, full_text: FullText, page: Option<ArticlePage>, revision: u64, page_url: QString }`. Each WRITE setter that changes its value calls `rebuild`. `set_entry_id` first loads the detail (`reload`'s read) and calls `full_text.select(detail row)`. `reload()`: for `entry_id >= 0`, `with_session(|s| (s.store.entry_detail(id), s.store.fulltext(id)))`; `Ok(None)` (gone or buried) → no entry; `Err(e)` → `banner(read_failure(&e))` and the page stays; then `rebuild`. `rebuild`: no detail → `hasEntry` false, `page_url` `""`; else the new `ArticlePage { entry, fulltext, show_full: full_text.shows(), style: ReaderStyle { font: ReaderFont::parse(..) or default, size: effective_size(text_size, look), width: LineWidth::parse(..) or default }, scheme: ColorScheme::parse(..) or Light, theme: ReaderTheme { appearance: Appearance::parse(look) or Native, crt: false }, subpage: None }`; when `!same_page` with the old one: `revision += 1`, `page_url = page_url(id, revision)`, store `html(&chrono::Local)` under it (replacing the previous page), `page_changed`; always `state_changed`. `link` = the row's URL when `is_web_url`; `isStarred` = the row's flag. `probeScript()` → `RECETTE_PROBE`.

- [ ] **Step 1: Write the failing tests** in `reader.rs`'s `#[cfg(test)] mod tests` (they link like B1's library tests):

```rust
#[test]
fn assets_and_pages_are_served_and_nothing_else() {
    let (mut mime, mut body) = (String::new(), Vec::new());
    assert!(ressac_scheme_serve("ressac:asset/Newsreader-Variable.ttf", &mut mime, &mut body));
    assert_eq!((mime.as_str(), &body[..4]), ("font/ttf", &[0u8, 1, 0, 0][..]));
    body.clear();
    assert!(ressac_scheme_serve("ressac:asset/bedstead.otf", &mut mime, &mut body));
    assert_eq!((mime.as_str(), &body[..4]), ("font/otf", &b"OTTO"[..]));
    body.clear();
    assert!(ressac_scheme_serve(ressac_ui::article::PROOF_URL, &mut mime, &mut body));
    assert!(mime.starts_with("text/html") && String::from_utf8_lossy(&body).contains("data-inline"));
    for bad in ["ressac:asset/../Cargo.toml", "ressac:asset/reader.css", "ressac:page/1-1", "ressac:other", "https://e.org/"] {
        body.clear();
        assert!(!ressac_scheme_serve(bad, &mut mime, &mut body), "{bad}");
    }
    store_page("ressac:page/1-1", "<p>x</p>".into());
    body.clear();
    assert!(ressac_scheme_serve("ressac:page/1-1", &mut mime, &mut body) && body == b"<p>x</p>");
    store_page("ressac:page/2-2", "<p>y</p>".into());   // the previous page is dropped
    assert!(!ressac_scheme_serve("ressac:page/1-1", &mut mime, &mut Vec::new()));
}
```

(`fn store_page(url: &str, html: String)` is the module's private store writer.)

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --lib 2>&1 | grep -E "error|test result" | head -3'` → `error[E0425]: cannot find function` (`ressac_scheme_serve`).
- [ ] **Step 3: Implement** the four files as above.
- [ ] **Step 4: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `test result: ok.` (B1/B2's `launch` and `reading` suites unchanged: the singleton is registered, not yet used). Then the full gate (B3b ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/build.rs apps/linux/cpp/shim.cpp apps/linux/src/lib.rs apps/linux/src/reader.rs
git commit -m "ressac-kde: ressac: scheme (pages and fonts from Rust) and the Reader object

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3c: the reader pane

### Task 4 (B3c): `ReaderPane.qml` in the third pane, the script proof

**Files:**
- Create: `apps/linux/qml/ReaderPane.qml`
- Modify: `apps/linux/qml/ReadingPage.qml` (the `readerPane` `Item` becomes `ReaderPane { id: readerPane; QQC2.SplitView.fillWidth: true; QQC2.SplitView.minimumWidth: 300 }`; `Binding`s: `Reader.entryId` ← `EntriesModel.selectedEntryId`, `Reader.colorScheme` ← `Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark ? "dark" : "light"`, `Reader.look` ← `page.look` (add `property string look: "native"` now), `Reader.readerFont` ← `AppSettings.readerFont`, `Reader.textSize` ← `look === "antiope" ? AppSettings.readerSizeAntiope : AppSettings.readerSize`, `Reader.readerWidth` ← `AppSettings.readerWidth`; `reloadAll()` also calls `Reader.reload()`; `Reader.onBanner` → `Runs.showBanner`)
- Test (not counted): `apps/linux/tests/reader.rs`

**Interfaces:**
- Consumes: Task 3's `Reader`; `Recette.active`.
- Produces: `ReaderPane.qml` with `objectName: "readerPane"`, `focusWeb()`, `runAppScript(script, done)`.

`ReaderPane.qml` (root `Item`): `property bool proofing: Recette.active`; a `WebEngineView { id: web; anchors.fill: parent; visible: Reader.hasEntry || pane.proofing; url: pane.proofing ? Reader.proofUrl : Reader.pageUrl; backgroundColor: Reader.colorScheme === "dark" ? "#1C1C1E" : "#FFFFFF"; settings.javascriptEnabled: false; settings.javascriptCanOpenWindows: false; settings.focusOnNavigationEnabled: false; settings.playbackRequiresUserGesture: true; settings.pluginsEnabled: false; settings.localContentCanAccessFileUrls: false }` (leave `unknownUrlSchemePolicy` at its default: see (c)); `onNavigationRequested: (request) => (String(request.url) === String(web.url) && request.navigationType === WebEngineNavigationRequest.TypedNavigation && request.isMainFrame) ? request.accept() : request.reject()` (Task 5 replaces it with `Reader.decide`); `onJavaScriptConsoleMessage: (level, message) => console.debug("reader page:", message)`; `onLoadingChanged: (info) => { if (info.status !== WebEngineView.LoadSucceededStatus || !Recette.active) return; … }` — for the proof page run `Reader.probeScript()` in `WebEngineScript.ApplicationWorld`, then `console.warn("reader-proof: " + result)` and `pane.proofing = false`; for any other page `console.warn("reader: " + info.url + " " + result)`. `function runAppScript(script, done) { web.runJavaScript(script, WebEngineScript.ApplicationWorld, done) }`; `function focusWeb() { web.forceActiveFocus() }`. `Kirigami.PlaceholderMessage { anchors.centerIn: parent; visible: !Reader.hasEntry && !pane.proofing; icon.name: "document-preview"; text: "No Entry Selected"; explanation: "Choose an entry in the list to read it here." }`.

- [ ] **Step 1: Write the failing test** `apps/linux/tests/reader.rs`:

```rust
//! The reader page offscreen (spec §7.4; master Review Focus 4).
#![cfg(target_os = "linux")]

mod common;
use common::*;

const MARKS_NULL: &str = r#""inline":null,"external":null,"onload":null,"onerror":null,"jslink":null"#;

/// 600 KB of hostile markup, 300 levels deep, for entry 1 (the walk's `03-starred` shows it).
fn hostile_body() -> String {
    let unit = concat!(r#"<script>document.documentElement.setAttribute("data-inline","ran")</script><img src="x" onerror="document.documentElement.setAttribute('data-onerror','ran')"><a href="javascript:void(0)">x</a><p>Hostile paragraph.</p>"#, "\n\n");
    "<div>".repeat(300) + &unit.repeat(600 * 1024 / unit.len() + 1) + &"</div>".repeat(300)
}

#[test]
fn article_scripts_never_run_and_a_hostile_600_kb_entry_still_loads() {
    let run = Run::new();
    let db = run.copy_fixture();
    rusqlite::Connection::open(&db).unwrap()
        .execute("UPDATE entry SET content_html = ?1 WHERE id = 1", [hostile_body()]).unwrap();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(err.contains(&format!(r#"reader-proof: {{"app":"ran",{MARKS_NULL},"scripts":2,"#)), "{err}");
    let line = err.lines().find(|l| l.contains("reader: ressac:page/1-")).unwrap_or_else(|| panic!("no page for entry 1 in {err}"));
    assert!(line.contains(&format!(r#""app":"ran",{MARKS_NULL},"scripts":0,"#)), "{line}");
    let paragraphs: usize = line.split(r#""paragraphs":"#).nth(1).unwrap().trim_end_matches(['}', ' ']).parse().unwrap();
    assert!(paragraphs > 100, "{line}");  // the plain-text fallback, one paragraph per blank-line block
    assert_drawn(&run.shots.join("03-starred.png"));
}
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test reader 2>&1 | grep -E "panicked|test result"'` → FAIL: no `reader-proof:` line.
- [ ] **Step 3: Implement** `ReaderPane.qml` and the `ReadingPage.qml` changes (the plain-text fallback makes one paragraph per blank-line block, hence the unit's trailing `\n\n`).
- [ ] **Step 4: Run it to verify it passes, and look at the screens** — the command of Step 2 → `test result: ok.`; then the B2 screenshot command (B2 Task 9 Step 4) and open `target-shots/03-starred.png`: the article text in the third pane, light palette, meta line, title. `rm -rf target-shots`.
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/ReaderPane.qml apps/linux/qml/ReadingPage.qml apps/linux/tests/reader.rs
git commit -m "ressac-kde: reader pane (WebEngineView, JavaScript off, app scripts in their own world)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 5 (B3c): links, full article, reader toolbar, page-down

**Files:**
- Modify: `apps/linux/src/reader.rs` (`decide`, `requestFullText`, `pageDownScript`; properties `isShowingFullText`, `isExtracting`, `canRequestFullText`, `fullTextHelp`, `runFailureNotice`; signal `wrote`)
- Modify: `apps/linux/qml/ReaderPane.qml` (navigation through `Reader.decide`, `onNewWindowRequested`, context menu, run-failure bar, `pageDown`, `openExternally`, `copyText`, `clickTitleLink`)
- Modify: `apps/linux/qml/ReadingPage.qml` (reader toolbar items; `Reader.onWrote` → `reloadAll()`; `readonly property alias reader: readerPane`)
- Modify: `apps/linux/qml/Main.qml` (the reading `Loader` gets `id: reading` if B2 left it without one; walk steps 07–11)
- Test (not counted): `apps/linux/tests/reader.rs` (second test), `apps/linux/tests/reading.rs` and `apps/linux/tests/launch.rs` (their exact shot lists become prefixes: `assert_eq!(&shots(&run)[..6], [...])` and "at least six shots", so later walk steps never touch them again)

**Interfaces:**
- Consumes: Task 2 (`decide`, `LinkDecision::encode`, `FullText`, `page_down_script`), Task 3, B2's `Session.worker`.
- Produces: `Reader.decide/requestFullText/pageDownScript` and the full-text properties; `ReaderPane.pageDown/openExternally/copyText/clickTitleLink`; `ReadingPage.reader`.

Behaviour:
- `decide(url, kind, mainFrame, expected)` → `article::decide(Navigation::from_qt(kind), url, mainFrame, expected).encode()`. QML: `onNavigationRequested: (request) => { const d = Reader.decide(String(request.url), request.navigationType, request.isMainFrame, String(web.url)); if (d === "allow") { request.accept(); return } request.reject(); if (d.startsWith("open:")) pane.openExternally(d.slice(5)) }` (a `toggle:` answer is B8d's); `onNewWindowRequested: (request) => { const d = Reader.decide(String(request.requestedUrl), 0, true, ""); if (d.startsWith("open:")) pane.openExternally(d.slice(5)) }`. `openExternally(url)`: recette → `console.warn("reader: open " + url)`, else `Qt.openUrlExternally(url)`.
- Context menu: `onContextMenuRequested: (request) => { request.accepted = true; menu.selection = request.selectedText; const d = Reader.decide(String(request.linkUrl), 0, true, ""); menu.link = d.startsWith("open:") ? d.slice(5) : ""; menu.popup() }`, a `QQC2.Menu` with "Copy" (`enabled: selection !== ""`, `web.triggerWebAction(WebEngineView.Copy)`) and "Copy Link" (`visible: link !== ""`, `pane.copyText(link)`). `copyText(text)`: a hidden `TextEdit` (`text = …; selectAll(); copy()`).
- Run-failure bar above the view: `QQC2.Label { visible: Reader.runFailureNotice !== ""; text: Reader.runFailureNotice; color: Kirigami.Theme.disabledTextColor; leftPadding: 16; rightPadding: 16; topPadding: 6; bottomPadding: 6; background: Rectangle { color: Kirigami.Theme.alternateBackgroundColor } }`; the web view anchors under it.
- `pageDown(entryId, done)`: `entryId !== Reader.entryId` → `done(true)` (the selected entry's page is not the one on screen yet: spent); `!Reader.hasEntry` → `done(false)`; else `runAppScript(Reader.pageDownScript(String(web.url)), r => done(r === "spent" || r === "scrolled"))`.
- `clickTitleLink()` (walk hook): `runAppScript('document.querySelector("h1.title a").click()', function () {})` — a synthetic app-world click reaches `navigationRequested` as `LinkClickedNavigation` (validated).
- `requestFullText()`: no selected row → nothing; `full_text.request(row, fulltext)`: `ShowFeedText` / `ShowStored` → `rebuild`; `Extract(id)` → `state_changed`, then `with_session(|s| s.worker.short(move |cx| cx.block_on(cx.app.extract_entry(id, &NullSink)), done))` with `done` queueing `extraction_finished(id, result)` → `full_text.finished(id, selected, result)`, `reload()`, `wrote`. `Nothing` → nothing. Getters: `isShowingFullText` = `full_text.is_showing(fulltext)`, `isExtracting` = `is_extracting(selected)`, `canRequestFullText` = `can_request(row)`, `fullTextHelp` = `full_article_help(row, showing, extracting, failure)`, `runFailureNotice` = `run_failure_notice(…)` or `""`.
- Toolbar (after B2's items, a `QQC2.ToolSeparator`): `QQC2.BusyIndicator { visible: Reader.isExtracting; running: visible; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: implicitWidth }`; `QQC2.ToolButton { text: "Full Article"; icon.name: "view-readermode"; display: QQC2.AbstractButton.TextBesideIcon; checkable: true; checked: Reader.isShowingFullText; enabled: Reader.canRequestFullText; onClicked: Reader.requestFullText(); QQC2.ToolTip.text: Reader.fullTextHelp; QQC2.ToolTip.visible: hovered }` (re-assert `checked` after the click, the state is Reader's); `QQC2.ToolButton { text: Reader.isStarred ? "Unstar" : "Star"; icon.name: Reader.isStarred ? "starred-symbolic" : "non-starred-symbolic"; display: IconOnly; enabled: Reader.hasEntry && EntriesModel.selectedRow >= 0; onClicked: EntriesModel.toggleStarred(EntriesModel.selectedRow); tooltip "Star (S)" }`; `QQC2.ToolButton { text: "Open in Browser"; icon.name: "internet-web-browser"; display: IconOnly; enabled: Reader.link !== ""; onClicked: readerPane.openExternally(Reader.link); tooltip "Open in Browser (Ctrl+Return)" }`; `QQC2.ToolButton { text: "Copy Link"; icon.name: "edit-copy"; display: IconOnly; enabled: Reader.link !== ""; onClicked: readerPane.copyText(Reader.link); tooltip "Copy Link" }`.
- Walk steps appended to `Main.qml`'s `walkSteps` (Daring Fireball, `feed:8`, lists entries 5 then 6 whatever their read state):

```qml
{ name: "07-full-article", enter: function () { SourcesModel.selectKey("feed:8"); EntriesModel.select(0)
    console.warn("walk: 07-full-article entry=" + Reader.entryId + " showing=" + Reader.isShowingFullText) } },
{ name: "08-feed-text", enter: function () { Reader.requestFullText(); console.warn("walk: 08-feed-text showing=" + Reader.isShowingFullText) } },
{ name: "09-link", enter: function () { reading.item.reader.clickTitleLink() } },
{ name: "10-extraction", enter: function () { EntriesModel.select(1); Reader.requestFullText()
    console.warn("walk: 10-extraction entry=" + Reader.entryId + " extracting=" + Reader.isExtracting) } },
{ name: "11-notice", enter: function () { console.warn("walk: 11-notice showing=" + Reader.isShowingFullText + " help=" + Reader.fullTextHelp) } },
```

- [ ] **Step 1: Write the failing test** (second test in `tests/reader.rs`):

```rust
#[test]
fn the_reader_shows_full_articles_sends_links_out_and_reports_a_failed_extraction() {
    let run = Run::new();
    let db = run.copy_fixture();
    // Entry 6's extraction must fail without the network: a loopback address, refused by the core's guard.
    rusqlite::Connection::open(&db).unwrap()
        .execute("UPDATE entry SET url = 'http://127.0.0.1:9/walled' WHERE id = 6", []).unwrap();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    for line in ["walk: 07-full-article entry=5 showing=true", "walk: 08-feed-text showing=false",
        "reader: open https://daringfireball.net/2026/10/tides", "walk: 10-extraction entry=6 extracting=true",
        "walk: 11-notice showing=false help=Full article unavailable: "] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert!(err.lines().any(|l| l.contains("walk: 11-notice") && l.ends_with("Click to try again")), "{err}");
    let stored: String = rusqlite::Connection::open(&db).unwrap()
        .query_row("SELECT error FROM entry_fulltext WHERE entry_id = 6", [], |r| r.get(0)).unwrap();
    assert_ne!(stored, "The page is behind a login.");   // a new attempt was stored
    for shot in ["07-full-article.png", "08-feed-text.png", "11-notice.png"] { assert_drawn(&run.shots.join(shot)); }
}
```

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test reader 2>&1 | grep -E "panicked|test result"'` → FAIL: `walk: 07-full-article … missing`.
- [ ] **Step 3: Implement** the four files and loosen the two shot-list assertions.
- [ ] **Step 4: Verify** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"; cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1'` → every `ok.`, `Finished`. Open `07-full-article.png` (badge "full article", the stored text) and `11-notice.png` (notice inside the page). Then the full gate (B3c ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/src/reader.rs apps/linux/qml/ReaderPane.qml apps/linux/qml/ReadingPage.qml apps/linux/qml/Main.qml apps/linux/tests/reader.rs apps/linux/tests/reading.rs apps/linux/tests/launch.rs
git commit -m "ressac-kde: reader links to the browser, Full Article toggle and notices, reader toolbar

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3d: navigation and keys (`ressac-ui`, core)

### Task 6 (B3d): `Store::next_unread`, `EntryList` navigation

**Files:**
- Modify: `crates/ressac-core/src/store/library.rs` (`Store::next_unread` + a test in its `mod tests`)
- Modify: `crates/ressac-ui/src/entries.rs` (the four `EntryList` methods, `AUTO_READ_DELAY`, tests)

**Interfaces:**
- Consumes: core `list_condition`, `ListCursor`; B2's `EntryList` internals (floor, rows, `load_more`), `test_db`.
- Produces: `Store::next_unread`; `EntryList::{next_entry, previous_entry, next_unread, auto_read_due}`; `entries::AUTO_READ_DELAY`.

Behaviour (`AppState.swift:361-430`, `Queries.swift:326-338`): `Store::next_unread` → `None` for `Source::Journal` or an empty condition; else the first row of `<list condition> AND NOT is_read [AND (published_at, entry_id) < (<after's published_at>, after)]` by `published_at DESC, entry_id DESC`, as a `ListCursor`. `next_entry`: the row after `selected` (row 0 without a selection); when `selected` is the last loaded row and the list has more, `load_more` first; `None` at the end. `previous_entry`: the row before (row 0 without a selection), never loads. `next_unread`: the first loaded row after `selected` that is unread and not `selected`; when none and the list has more, one `Store::next_unread(query, last loaded id)` and, if found below the floor, the floor moves down to it (rows reloaded once), then scan again. `auto_read_due`: `selected == Some(entry_id)` and that row is loaded and unread.

- [ ] **Step 1: Write the failing tests.** Core (`library.rs`, with the module's own test helpers): `next_unread_is_the_first_unread_row_after_an_entry` — one feed with e1 (3 October, unread), e2 (2 October, read), e3 (1 October, unread), list `ListQuery { source: Source::Feed(feed), filter: ListFilter::All, search: None }`: `after None` → e1's cursor, `after e1` → e3's, `after e3` → `None`; the same with `Source::Journal` → `None`. `entries.rs` (with `test_db`, page size 2, five entries in feed 1 published 1…5 October, newest first):

```rust
#[test] fn next_entry_starts_at_the_first_row_and_stops_at_the_last() { /* None → first id; last of a whole list → None */ }
#[test] fn next_entry_past_the_last_loaded_row_loads_the_next_page_first() { /* rows 2 loaded, selected = 2nd → 3rd id, rows().len() == 4 */ }
#[test] fn previous_entry_never_loads_and_starts_at_the_first_row() { /* None → first; first → None; rows().len() unchanged */ }
#[test] fn next_unread_skips_read_rows_and_the_selection() { /* rows 2..3 read → 4th */ }
#[test] fn next_unread_beyond_the_loaded_rows_loads_down_to_it_with_one_query() {
    // Only the oldest entry is unread; two rows loaded: next_unread(selected = first) == Some(oldest) and the
    // list now holds every row down to it (rows().len() == 5), loaded in one move of the floor.
}
#[test] fn auto_read_is_due_only_for_the_entry_still_selected_and_unread() {
    assert_eq!(AUTO_READ_DELAY, Duration::from_secs(1));
    /* due(Some(a), a) true; due(Some(b), a) false; after set_read_local([a], true) false */
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-core next_unread 2>&1 | tail -2; cargo test -p ressac-ui next_ 2>&1 | tail -2` → compile errors (`no method named next_unread`).
- [ ] **Step 3: Implement** both.
- [ ] **Step 4: Verify** — `cargo test -p ressac-core -p ressac-ui 2>&1 | grep "test result"` → every `ok.`.
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/store/library.rs crates/ressac-ui/src/entries.rs
git commit -m "ressac-ui: next, previous and next-unread entry over the paging floor

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

### Task 7 (B3d): `ressac_ui::keys`

**Files:**
- Modify: `crates/ressac-ui/src/lib.rs` (`pub mod keys;`)
- Create: `crates/ressac-ui/src/keys.rs` (tests in its `mod tests`)

**Interfaces:**
- Consumes: `settings::Appearance`.
- Produces: `ressac_ui::keys` as in the header.

Rules (`ReadingKeyRouter.swift:97-149`): `decode` keeps only Shift/Control/Alt/Meta (Keypad ignored); arrows only with no modifier; Backspace → `Delete`, Esc → `Escape` (no modifier); `?` (key `0x3F` or text `"?"`) with at most Shift → `Question`; a digit (text `"0"`…`"9"`, or keys `0x30`–`0x39`) with at most Shift → `Digit`; Alt alone + a key `A`–`Z` → `Option(lowercase)`; no modifier + key `A`–`Z` → `Letter(lowercase of the key code, so Caps Lock reads as the letter)`; Space with no modifier; anything with Control or Meta → `None`. `classify(chain)` (tokens from the focus item upwards): empty → `Window`; any `popup` or first `text` → `TextInput`; any `web` or `readerPane` → `Reader`; any `entriesList` → `EntryList`; any `sourcesList` → `Sidebar`; first `item` or `readingPage` → `Window`; else `Control`. `command` = `nativeCommand` for every `look` in B3 (B8a tries Antiope's first). `claims_override` = `focus != TextInput && (matches!(key, Letter(_) | Space) || command(key, focus, look).is_some())`. `verb`: `Advance`+repeat → `advanceHeld`, `ToggleRead`+repeat → `toggleReadHeld`, `ToggleStar`+repeat → `toggleStarHeld`, else the command's camelCase name. `route` combines them (`None` key → `Route { claims_override: false, verb: None }`).

- [ ] **Step 1: Write the failing tests** (port of `ReadingKeysTests`, Native parts, `KeyboardTests.swift:16-147`, with Qt key events):

```rust
const NONE: u32 = 0;
fn k(key: i32, text: &str, mods: u32) -> Option<Key> { decode(key, text, mods) }

#[test]
fn modified_keys_are_left_alone() {
    assert_eq!(k(0x4A, "j", NONE), Some(Key::Letter('j')));
    assert_eq!(k(qt::KEY_SPACE, " ", NONE), Some(Key::Space));
    assert_eq!(k(qt::KEY_DOWN, "", NONE), Some(Key::Down));
    assert_eq!(k(qt::KEY_DOWN, "", qt::KEYPAD), Some(Key::Down));
    for (key, text, mods) in [(0x4A, "J", qt::SHIFT), (0x4A, "j", qt::CONTROL), (0x4A, "j", qt::META),
        (qt::KEY_DOWN, "", qt::SHIFT), (0x34, "4", qt::ALT), (0x53, "S", qt::ALT | qt::SHIFT), (qt::KEY_BACKSPACE, "", qt::ALT),
        (0x3F, "?", qt::CONTROL)] {
        assert_eq!(k(key, text, mods), None, "{key:#x} {mods:#x}");
    }
    assert_eq!(k(0x4A, "J", NONE), Some(Key::Letter('j')));           // Caps Lock
    assert_eq!(k(0x4A, "j", qt::ALT), Some(Key::Option('j')));
    assert_eq!(k(0x34, "4", NONE), Some(Key::Digit(4)));
    assert_eq!(k(0x26, "1", qt::SHIFT), Some(Key::Digit(1)));          // AZERTY top row
    assert_eq!(k(0x30, "0", qt::KEYPAD), Some(Key::Digit(0)));
    assert_eq!(k(qt::KEY_BACKSPACE, "\u{8}", NONE), Some(Key::Delete));
    assert_eq!(k(qt::KEY_ESCAPE, "\u{1b}", NONE), Some(Key::Escape));
    assert_eq!(k(0x3F, "?", qt::SHIFT), Some(Key::Question));
}

#[test]
fn letters_and_space_act_everywhere_but_in_text_fields() { /* for both looks and Sidebar/EntryList/Reader/Window:
    j→NextEntry, k→PreviousEntry, m→ToggleRead, s→ToggleStar, Space→Advance; every key in TextInput → None;
    x in EntryList → None; Space in Control → None */ }

#[test]
fn arrows_follow_the_focus() { /* ↓/↑ in EntryList and Reader → Next/Previous; Sidebar ↓/↑ → None; → in Sidebar →
    FocusEntryList; ← in EntryList → FocusSidebar; ← in Sidebar, → in EntryList, ← in Reader → None */ }

#[test]
fn native_never_produces_an_antiope_command() { /* digits, Delete, Escape, Option(any), Question, Letter('i')
    in EntryList → None for look Native */ }

#[test]
fn the_focused_item_is_classified_by_its_chain() {
    use focus::*;
    assert_eq!(classify(&[WEB, "reader", READING_PAGE]), KeyFocus::Reader);          // seen in the container
    assert_eq!(classify(&[ITEM, WEB, READER_PANE, READING_PAGE]), KeyFocus::Reader);
    assert_eq!(classify(&[TEXT, "search", READING_PAGE]), KeyFocus::TextInput);
    assert_eq!(classify(&[ENTRIES_LIST, "entriesPane", READING_PAGE]), KeyFocus::EntryList);
    assert_eq!(classify(&[CONTROL, ITEM, ENTRIES_LIST, "entriesPane"]), KeyFocus::EntryList);  // a delegate
    assert_eq!(classify(&[SOURCES_LIST, "sourcesPane", READING_PAGE]), KeyFocus::Sidebar);
    assert_eq!(classify(&[CONTROL, READING_PAGE]), KeyFocus::Control);                         // a toolbar button
    assert_eq!(classify(&[CONTROL, ITEM, POPUP, READING_PAGE]), KeyFocus::TextInput);          // an open menu
    assert_eq!(classify(&[READING_PAGE]), KeyFocus::Window);
    assert_eq!(classify(&[ITEM, ITEM]), KeyFocus::Window);
    assert_eq!(classify(&[]), KeyFocus::Window);
}

/// Review Focus 3.
#[test]
fn menus_never_see_plain_reading_keys_but_keep_their_chords() {
    for focus in [KeyFocus::Sidebar, KeyFocus::EntryList, KeyFocus::Reader, KeyFocus::Window, KeyFocus::Control] {
        for key in [Key::Letter('j'), Key::Letter('m'), Key::Letter('x'), Key::Space] {
            assert!(claims_override(key, focus, Appearance::Native), "{key:?} {focus:?}");
        }
    }
    for key in [Key::Letter('j'), Key::Space, Key::Down] { assert!(!claims_override(key, KeyFocus::TextInput, Appearance::Native)); }
    assert!(claims_override(Key::Down, KeyFocus::EntryList, Appearance::Native));
    assert!(!claims_override(Key::Down, KeyFocus::Sidebar, Appearance::Native));
    for key in [Key::Option('f'), Key::Option('s'), Key::Escape, Key::Digit(4)] {
        assert!(!claims_override(key, KeyFocus::EntryList, Appearance::Native), "{key:?}");  // mnemonics, Antiope keys
    }
}

#[test]
fn held_keys_spend_toggles_and_never_let_space_move_on() {
    assert_eq!([verb(Command::ToggleRead, true), verb(Command::ToggleStar, true), verb(Command::Advance, true)],
               ["toggleReadHeld", "toggleStarHeld", "advanceHeld"]);
    assert_eq!([verb(Command::NextEntry, true), verb(Command::ToggleRead, false), verb(Command::FocusEntryList, false)],
               ["nextEntry", "toggleRead", "focusEntryList"]);
}

#[test]
fn route_combines_decoding_focus_and_held_rules() {
    use focus::*;
    assert_eq!(route(qt::KEY_SPACE, " ", NONE, false, &[WEB, READER_PANE], Appearance::Native),
               Route { claims_override: true, verb: Some("advance") });
    assert_eq!(route(0x4A, "j", NONE, false, &[TEXT, "search"], Appearance::Native), Route { claims_override: false, verb: None });
    assert_eq!(route(0x46, "f", qt::ALT, false, &[ENTRIES_LIST], Appearance::Native), Route { claims_override: false, verb: None });
    assert_eq!(route(0x4D, "m", NONE, true, &[ENTRIES_LIST], Appearance::Native), Route { claims_override: true, verb: Some("toggleReadHeld") });
}
```

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-ui keys 2>&1 | tail -2` → compile error.
- [ ] **Step 3: Implement** `keys.rs`.
- [ ] **Step 4: Verify** — `cargo test -p ressac-ui 2>&1 | grep "test result"` → `ok.`; then the full gate (B3d ends here).
- [ ] **Step 5: Commit**

```bash
git add crates/ressac-ui/src/lib.rs crates/ressac-ui/src/keys.rs
git commit -m "ressac-ui: reading keys (Qt decoding, focus from the item chain, Native map, held keys)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3e: the window key filter and the bridges

### Task 8 (B3e): key filter in the shim, `ressac_route_key`, `EntriesModel` navigation, `AppSettings.resize`

**Files:**
- Modify: `apps/linux/cpp/shim.h` (`void ressac_install_key_filter();`, `bool ressac_send_key(int key, unsigned int modifiers, rust::Str text);`)
- Modify: `apps/linux/cpp/shim.cpp` (`ReadingKeyFilter`, the two functions)
- Modify: `apps/linux/src/lib.rs` (bridge declarations; `extern "Rust" { fn ressac_route_key(…) -> bool; }` and its body here; `run()` calls `shim::ressac_install_key_filter()` right after `ressac_has_window()` succeeds)
- Modify: `apps/linux/src/entries.rs` (`EntriesModel` additions of the header)
- Modify: `apps/linux/src/settings.rs` (`AppSettings.resize`)

**Interfaces:**
- Consumes: Task 7 (`keys::route`, `focus::*`), Task 6 (`EntryList` navigation), B2's local edits and worker writes (`set_read_local`, `DirectWrite::SetRead`), B1's `Settings::resize`, `TextSizeCommand`.
- Produces: the shim functions, `ressac_route_key`, the `EntriesModel` and `AppSettings` additions of the header.

Behaviour:
- `ReadingKeyFilter : QObject` installed on the first `QQuickWindow` (`first_quick_window(false)`); `eventFilter(obj, e)`: only `obj == window` and `ShortcutOverride` / `KeyPress`; the page = the `QQuickItem` named `readingPage`, found depth-first over `window->contentItem()->childItems()` and kept in a `QPointer` (re-found when null; none → return false); the chain = for each item from `window->activeFocusItem()` up: `popup` if it inherits `QQuickPopupItem`, `text` if `QQuickTextInput` or `QQuickTextEdit`, `web` if `QQuickWebEngineView`, `control` if `QQuickAbstractButton`, else its `objectName()` or `item`; joined with `|`; `look` = `page->property("look").toString()`; `claims = ressac_route_key(k->key(), k->text().toStdString(), uint(k->modifiers()), k->isAutoRepeat(), chain, look, verb)`. `ShortcutOverride`: `claims` → `e->accept(); return true`. `KeyPress`: empty verb → false; else `QMetaObject::invokeMethod(page, "perform", Qt::DirectConnection, Q_RETURN_ARG(QVariant, r), Q_ARG(QVariant, QString(verb)))` and return `r.toBool()`.
- `ressac_send_key(key, modifiers, text)`: on the first visible `QQuickWindow`, `QCoreApplication::sendEvent` a `ShortcutOverride` (ignored first), then the `KeyPress`, then the `KeyRelease`; returns whether the `KeyPress` was consumed (`sendEvent`'s result). It does not go through the shortcut map (the container experiment covered that path).
- `ressac_route_key` (in `lib.rs`): `keys::route(key, text, modifiers, repeat, &chain.split('|').filter(|t| !t.is_empty()).collect::<Vec<_>>(), Appearance::parse(look).unwrap_or_default())`; `*verb = route.verb.unwrap_or("").into()`; returns `route.claims_override`.
- `EntriesModel`: `selectNext()` → `list.next_entry(store, selected, today)`; a target → if the leaving row is unread, the same optimistic read write as `toggleRead` (local edit + `DirectWrite::SetRead` on the worker, banner on failure), then select the target; returns whether it moved. `selectPrevious()` → `previous_entry`, no read mark. `selectNextUnread()` → `next_unread` with the leaving mark. `selectFirstIfNone()` → selects row 0 when nothing is selected. `selectEntry(id)` → selects a loaded row. `autoRead(id)` → when `auto_read_due(selected, id)`, the read write. `canNavigate` = `active && !rows.is_empty()`; `canToggle` = `active && selected.is_some()`; `selectedRead` / `selectedStarred` from the selected row (false without one); `autoReadDelay` = `AUTO_READ_DELAY.as_millis()`. Errors from the store → `banner(read_failure(&e))`.
- `AppSettings.resize(look, command)`: `"bigger"` / `"smaller"` / `"actualSize"` → `TextSizeCommand`; an unknown command returns the current size and writes nothing; else `Settings::resize(store, Appearance::parse(look).unwrap_or_default(), cmd)` and the size property's notify.

- [ ] **Step 1: Write the failing check** — after adding the declarations to `lib.rs` only: `apps/linux/dev/dev.sh sh 'cargo build --locked -p ressac-kde 2>&1 | grep -E "^error" | head -3'` → `error: … undefined symbol: ressac_install_key_filter` (or cxx's missing-definition error).
- [ ] **Step 2: Implement** the five files as above.
- [ ] **Step 3: Verify** — `apps/linux/dev/dev.sh sh 'cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1; cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → `Finished`; every `ok.` (no walk step presses a key yet: `perform` does not exist, so the filter's invoke fails quietly and routes nothing). Then the full gate (B3e ends here).
- [ ] **Step 4: Commit**

```bash
git add apps/linux/cpp/shim.h apps/linux/cpp/shim.cpp apps/linux/src/lib.rs apps/linux/src/entries.rs apps/linux/src/settings.rs
git commit -m "ressac-kde: window key filter before WebEngine, entry navigation and text size bridges

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3f: routing in QML, auto-read

### Task 9 (B3f): `perform`, Space, focus, auto-read, `Recette.key`

**Files:**
- Modify: `apps/linux/qml/ReadingPage.qml` (`objectName: "readingPage"`; `perform`, `advance`, focus functions, `focusName`, `readonly property bool searchFocused: searchField.activeFocus`, `openInBrowser`, `lastVerb`/`lastHandled`, the auto-read `Timer`; the search field gets `id: searchField` and `objectName: "search"`)
- Modify: `apps/linux/qml/SourcesPane.qml` (its `ListView`: `objectName: "sourcesList"`, `focus: true`; `function focusList()`; ↑/↓ with `Keys.onUpPressed`/`onDownPressed` stepping `SourcesModel.select(i)` over rows until `SourcesModel.selectedRow === i`, skipping headings; delegates `focusPolicy: Qt.NoFocus`, a click also `list.forceActiveFocus()`)
- Modify: `apps/linux/qml/EntriesPane.qml` (its `ListView`: `objectName: "entriesList"`; `function focusList()`; delegates `focusPolicy: Qt.NoFocus`, a click also `list.forceActiveFocus()`)
- Modify: `apps/linux/qml/Main.qml` (walk steps 12–20)
- Modify: `apps/linux/src/recette.rs` (`#[qinvokable] fn key(self: &Recette, key: i32, text: &QString, modifiers: i32) -> bool` → `shim::ressac_send_key` when active)
- Test (not counted): `apps/linux/tests/keys.rs`

**Interfaces:**
- Consumes: Task 8, Task 5 (`readerPane.pageDown`), B2's models.
- Produces: `ReadingPage.perform(verb)` and the functions of the header; `Recette.key`.

`perform(verb)` (untyped, returns a bool; sets `lastVerb`, `lastHandled`):

| verb | when it cannot act (returns false) | action (returns true) |
|---|---|---|
| `nextEntry` / `previousEntry` | `!EntriesModel.canNavigate` | `EntriesModel.selectNext()` / `selectPrevious()` |
| `advance` / `advanceHeld` | `!EntriesModel.canNavigate` | `advance(verb === "advanceHeld")` |
| `toggleRead` | `!EntriesModel.canToggle` | `if (EntriesModel.selectedRead) autoRead.stop()`; `EntriesModel.toggleRead(EntriesModel.selectedRow)` |
| `toggleStar` | `!EntriesModel.canToggle` | `EntriesModel.toggleStarred(EntriesModel.selectedRow)` |
| `toggleReadHeld` / `toggleStarHeld` | `!EntriesModel.canToggle` | nothing (spent) |
| `focusEntryList` | — | `if (!SourcesModel.isGarden) EntriesModel.selectFirstIfNone()`; `entriesPane.focusList()` |
| `focusSidebar` | — | `sourcesPane.focusList()` |

`advance(held)`: when `advancing`, return (the key is spent); `advancing = true`; `const id = EntriesModel.selectedEntryId`; `readerPane.pageDown(id, function (spentOrScrolled) { advancing = false; if (spentOrScrolled || held || EntriesModel.selectedEntryId !== id) return; EntriesModel.selectNextUnread() })`. Auto-read: `Timer { id: autoRead; interval: EntriesModel.autoReadDelay; property real entryId: -1; onTriggered: EntriesModel.autoRead(entryId) }` and `Connections { target: EntriesModel; function onSelectionChanged() { if (EntriesModel.selectedEntryId !== autoRead.entryId) { autoRead.entryId = EntriesModel.selectedEntryId; if (autoRead.entryId >= 0) autoRead.restart(); else autoRead.stop() } } }`. `openInBrowser()`: `if (Reader.link !== "") readerPane.openExternally(Reader.link)`. `focusName()`: which of the sources list, entries list, reader (`readerPane` has active focus within) or search field has `activeFocus`.

Walk steps (xkcd is `feed:1`: entries 1, 3, 4, 2 newest first; entry 2 read in the fixture; `starred` lists 1 (starred by step 03) and 4):

```qml
{ name: "12-j-in-sidebar", enter: function () { SourcesModel.selectKey("feed:1"); reading.item.focusSidebar(); Recette.key(Qt.Key_J, "j", 0)
    console.warn("walk: 12-j-in-sidebar selected=" + EntriesModel.selectedEntryId + " focus=" + reading.item.focusName()) } },
{ name: "13-arrows-in-list", enter: function () { Recette.key(Qt.Key_Right, "", 0); Recette.key(Qt.Key_Down, "", 0)
    console.warn("walk: 13-arrows-in-list selected=" + EntriesModel.selectedEntryId + " focus=" + reading.item.focusName()) } },
{ name: "14-space-in-reader", enter: function () { reading.item.reader.focusWeb(); Recette.key(Qt.Key_Space, " ", 0) } },
{ name: "15-text-field", enter: function () { console.warn("walk: 14-space-in-reader selected=" + EntriesModel.selectedEntryId)
    reading.item.focusSearch(); Recette.key(Qt.Key_J, "j", 0)
    console.warn("walk: 15-text-field search=" + EntriesModel.searchText + " selected=" + EntriesModel.selectedEntryId) } },
{ name: "16-garden-keys", enter: function () { EntriesModel.searchText = ""; SourcesModel.selectKey("garden:needs-triage"); reading.item.focusSidebar(); Recette.key(Qt.Key_K, "k", 0)
    console.warn("walk: 16-garden-keys verb=" + reading.item.lastVerb + " handled=" + reading.item.lastHandled) } },
{ name: "17-auto-read", enter: function () { SourcesModel.selectKey("starred"); EntriesModel.select(1)
    console.warn("walk: 17-auto-read selected=" + EntriesModel.selectedEntryId + " read=" + EntriesModel.selectedRead + " page=" + Reader.pageUrl) } },
{ name: "18-auto-read-wait", enter: function () {} },
{ name: "19-auto-read-wait", enter: function () {} },
{ name: "20-auto-read-done", enter: function () { console.warn("walk: 20-auto-read-done read=" + EntriesModel.selectedRead + " page=" + Reader.pageUrl) } },
```

- [ ] **Step 1: Write the failing test** `apps/linux/tests/keys.rs`:

```rust
//! Keys through the window filter, offscreen (spec §7.2, §10: keys reaching the router while the reader has focus).
#![cfg(target_os = "linux")]

mod common;
use common::*;

fn page_of<'a>(err: &'a str, step: &str) -> &'a str {
    err.lines().find(|l| l.contains(step)).and_then(|l| l.split(" page=").nth(1)).unwrap_or_else(|| panic!("{step} in {err}"))
}

#[test]
fn reading_keys_reach_the_router_before_the_lists_the_reader_and_the_menus() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch(Some(&db));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    for line in ["walk: 12-j-in-sidebar selected=1 focus=sidebar", "walk: 13-arrows-in-list selected=3 focus=entries",
        "walk: 14-space-in-reader selected=4", "walk: 15-text-field search=j selected=-1",
        "walk: 16-garden-keys verb=previousEntry handled=false", "walk: 17-auto-read selected=4 read=false page=ressac:page/4-",
        "walk: 20-auto-read-done read=true page=ressac:page/4-"] {
        assert!(err.contains(line), "{line} missing in {err}");
    }
    assert_eq!(page_of(&err, "walk: 17-auto-read"), page_of(&err, "walk: 20-auto-read-done")); // marking read never reloads
    let conn = rusqlite::Connection::open(&db).unwrap();
    let read: Vec<i64> = conn.prepare("SELECT id FROM entry WHERE read_at IS NOT NULL AND id IN (1, 2, 3, 4) ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(read, [1, 2, 3, 4], "j/↓ and Space mark what they leave (1, 3), auto-read marks 4, 2 was read");
}
```

(Step 13: `→` from the sidebar keeps the selection (1) and focuses the list, `↓` moves to 3 and marks 1 read. Step 14: entry 3's page is short, so Space at its bottom moves to the next unread, 4, and marks 3 read. Step 15: `j` types into the search field; the search for "j" matches nothing, so the selection is cleared and 4 stays unread, which step 17 shows.)

- [ ] **Step 2: Run it to verify it fails** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde --test keys 2>&1 | grep -E "panicked|test result"'` → FAIL: `walk: 12-j-in-sidebar … missing`.
- [ ] **Step 3: Implement** the five files.
- [ ] **Step 4: Verify** — `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"; cargo clippy --locked -p ressac-kde --all-targets -- -D warnings 2>&1 | tail -1'` → every `ok.` (`launch`, `reading`, `reader`, `keys`), `Finished`. If step 20's `read=true` is flaky because the walk's three 400 ms gaps fall under 1 s with fast grabs, add one more empty wait step (never shorten `AUTO_READ_DELAY`). Then the full gate (B3f ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/ReadingPage.qml apps/linux/qml/SourcesPane.qml apps/linux/qml/EntriesPane.qml apps/linux/qml/Main.qml apps/linux/src/recette.rs apps/linux/tests/keys.rs
git commit -m "ressac-kde: j, k, m, s, Space and arrows routed from the window, 1 s auto-read

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3g: menus and shortcuts

### Task 10 (B3g): `MainMenu.qml`

**Files:**
- Create: `apps/linux/qml/MainMenu.qml`
- Modify: `apps/linux/qml/Main.qml` (`menuBar: MainMenu { id: mainMenu; page: reading.item }` — `reading.item` is null until Ready, so every item is disabled then; walk steps 21–22)
- Test (not counted): `apps/linux/tests/menus.rs` (runs on the Mac too: it reads the QML file)

**Interfaces:**
- Consumes: Tasks 5, 8, 9; `Runs`, `EntriesModel`, `SourcesModel`, `Reader`, `AppSettings`.
- Produces: `MainMenu.qml` as in the header.

Content (`QQC2.MenuBar`; `readonly property bool ready: page !== null`; `readonly property bool letters: ready && !page.searchFocused`; every `QQC2.Action` has an `id` aliased by the header's name):

| Menu | Item | `shortcut` | `enabled` | `onTriggered` |
|---|---|---|---|---|
| `&File` | Add Feed… | `"Ctrl+N"` | `false` (B6a) | — |
| | Import OPML… | `"Ctrl+O"` | `false` (B6a) | — |
| | Export OPML… | `"Ctrl+Shift+E"` | `false` (B6a) | — |
| | Refresh | `"Ctrl+R"` | `ready && Runs.canRefresh` | `Runs.refreshSource(SourcesModel.selectedKey)` |
| | Refresh All Feeds | `"Ctrl+Shift+R"` | `ready && Runs.canRefresh` | `Runs.refreshAll()` |
| | Settings… | `"Ctrl+Shift+,"` | `false` (B6b) | — |
| | Quit Ressac | `StandardKey.Quit` | `true` | `Qt.quit()` |
| `&Edit` | Search | `StandardKey.Find` | `ready` | `page.focusSearch()` |
| `&View` | Show Unread Only (checkable, `checked: EntriesModel.unreadOnly`) | `"Ctrl+Alt+U"` | `ready && !SourcesModel.isGarden` | `EntriesModel.unreadOnly = checked` |
| | Bigger Text | `StandardKey.ZoomIn` | `ready` | `AppSettings.resize(page.look, "bigger")` |
| | Smaller Text | `StandardKey.ZoomOut` | `ready` | `AppSettings.resize(page.look, "smaller")` |
| | Actual Size | `"Ctrl+0"` | `ready` | `AppSettings.resize(page.look, "actualSize")` |
| | Sea Chart | `"Ctrl+Shift+M"` | `false` (B7) | — |
| `G&o` | Next Entry | `"J"` | `letters && EntriesModel.canNavigate` | `page.perform("nextEntry")` |
| | Previous Entry | `"K"` | `letters && EntriesModel.canNavigate` | `page.perform("previousEntry")` |
| | Scroll Down or Next Unread | `"Space"` | `letters && EntriesModel.canNavigate` | `page.perform("advance")` |
| `&Article` | `EntriesModel.selectedRead ? "Mark as Unread" : "Mark as Read"` | `"M"` | `letters && EntriesModel.canToggle` | `page.perform("toggleRead")` |
| | `EntriesModel.selectedStarred ? "Unstar" : "Star"` | `"S"` | `letters && EntriesModel.canToggle` | `page.perform("toggleStar")` |
| | Mark All as Read | `"Ctrl+Shift+A"` | `ready && EntriesModel.canMarkAllRead` | `EntriesModel.markAllRead()` |
| | Full Article (checkable, `checked: Reader.isShowingFullText`) | — | `Reader.canRequestFullText` | `Reader.requestFullText()` |
| | Open in Browser | `"Ctrl+Return"` | `Reader.link !== ""` | `page.openInBrowser()` |

Plus, inside the `MenuBar`, `Shortcut { sequence: "F5"; enabled: refresh.enabled; onActivated: refresh.trigger() }`. Separators: after Export OPML…, after Refresh All Feeds, after Show Unread Only, after Actual Size, after Mark All as Read. `openMenu(name)` opens `fileMenu`/`editMenu`/`viewMenu`/`goMenu`/`articleMenu` by name (`"file"` … `"article"`); `closeMenus()` closes them. Walk steps:

```qml
{ name: "21-menu", enter: function () { mainMenu.openMenu("article")
    console.warn("walk: 21-menu read=" + mainMenu.toggleRead.text + " enabled=" + mainMenu.toggleRead.enabled + " star=" + mainMenu.toggleStar.text) } },
{ name: "22-text-size", enter: function () { mainMenu.closeMenus(); mainMenu.biggerText.trigger(); const bigger = AppSettings.readerSize
    mainMenu.actualSize.trigger(); console.warn("walk: 22-text-size bigger=" + bigger + " actual=" + AppSettings.readerSize) } },
```

(At step 21 the selection is entry 4 in Starred, read by step 20: "Mark as Unread", enabled, "Unstar".)

- [ ] **Step 1: Write the failing tests** `apps/linux/tests/menus.rs`:

```rust
//! The menu bar is spec §7.2's table (read from the QML file, on every platform).
const MENU: &str = include_str!("../qml/MainMenu.qml");

#[test]
fn the_shortcuts_are_the_spec_table() {
    for want in [r#"shortcut: "Ctrl+N""#, r#"shortcut: "Ctrl+O""#, r#"shortcut: "Ctrl+Shift+E""#, r#"shortcut: "Ctrl+R""#,
        r#"shortcut: "Ctrl+Shift+R""#, r#"sequence: "F5""#, "shortcut: StandardKey.Find", r#"shortcut: "Ctrl+Alt+U""#,
        "shortcut: StandardKey.ZoomIn", "shortcut: StandardKey.ZoomOut", r#"shortcut: "Ctrl+0""#, r#"shortcut: "Ctrl+Shift+M""#,
        r#"shortcut: "Ctrl+Shift+A""#, r#"shortcut: "Ctrl+Return""#, r#"shortcut: "Ctrl+Shift+,""#, "shortcut: StandardKey.Quit",
        r#"shortcut: "J""#, r#"shortcut: "K""#, r#"shortcut: "Space""#, r#"shortcut: "M""#, r#"shortcut: "S""#] {
        assert!(MENU.contains(want), "{want}");
    }
    for title in ["Add Feed…", "Import OPML…", "Export OPML…", "Refresh All Feeds", "Settings…", "Quit Ressac", "Search",
        "Show Unread Only", "Bigger Text", "Smaller Text", "Actual Size", "Sea Chart", "Next Entry", "Previous Entry",
        "Scroll Down or Next Unread", "Mark as Read", "Mark as Unread", "Star", "Unstar", "Mark All as Read", "Full Article",
        "Open in Browser"] {
        assert!(MENU.contains(&format!("\"{title}\"")), "{title}");
    }
}

#[test]
fn menu_mnemonics_never_use_s_g_or_r() {
    let titles: Vec<&str> = MENU.lines().filter_map(|l| l.trim().strip_prefix("title: \"")).map(|t| t.trim_end_matches('"')).collect();
    assert_eq!(titles, ["&File", "&Edit", "&View", "G&o", "&Article"]);
    for title in titles {
        let mnemonic = title.split('&').nth(1).and_then(|r| r.chars().next()).unwrap().to_ascii_lowercase();
        assert!(!['s', 'g', 'r'].contains(&mnemonic), "{title}");
    }
}
```

and append to `tests/keys.rs`'s test: `for line in ["walk: 21-menu read=Mark as Unread enabled=true star=Unstar", "walk: 22-text-size bigger=18 actual=17"] { assert!(err.contains(line), "{line}"); }`.

- [ ] **Step 2: Run them to verify they fail** — `cargo test -p ressac-kde --test menus 2>&1 | tail -3` (on the Mac) → compile error: `couldn't read …/qml/MainMenu.qml`.
- [ ] **Step 3: Implement** `MainMenu.qml` and the `Main.qml` changes.
- [ ] **Step 4: Verify** — `cargo test -p ressac-kde --test menus` on the Mac → `ok.`; `apps/linux/dev/dev.sh sh 'cargo test --locked -p ressac-kde 2>&1 | grep "test result"'` → every `ok.`; open `21-menu.png` (the Article menu open, its shortcuts shown at the right). Then the full gate (B3g ends here).
- [ ] **Step 5: Commit**

```bash
git add apps/linux/qml/MainMenu.qml apps/linux/qml/Main.qml apps/linux/tests/menus.rs apps/linux/tests/keys.rs
git commit -m "ressac-kde: menu bar with the KDE shortcuts table (mnemonics never S, G, R)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

---

## B3h: VM checkpoint 1 (spec §8.5) — stops for Clément

### Task 11 (B3h): transfer, install, screenshots in Clément's Plasma session, verdict

**Files:**
- Create: `apps/linux/dev/marghe.sh`
- Modify: `notes.md` (section « Point d'étape VM 1 (B3) » under « Ressac sous Linux (sous-projet 8) »)
- Modify (only if something is deferred): `TODO.md`

**Interfaces:**
- Consumes: `apps/linux/install.sh` (B1d; honours `CARGO_TARGET_DIR`, `--software-rendering`, prints the `dnf install` line and exits 1 when packages are missing, never runs dnf), the `--screenshot` walk (steps 01–22), `tests/fixtures/contract.sqlite`.
- Produces: `apps/linux/dev/marghe.sh` (reused by B8e and B10).

`apps/linux/dev/marghe.sh` (bash, `set -euo pipefail`; constants `HOST=willow@marghe.local`, `VM="Fedora Linux"`, `VM_USER=willow`; the VM user's home read once with `getent passwd willow`; VM tree `$vm_home/src/ressac`, build dir `$vm_home/.cache/ressac-build`, run dir `/tmp/ressac-vm`; pulls to `${TMPDIR:-/tmp}/ressac-vm/<label>/` on the Mac, never into the repo). Every VM command runs as `ssh "$HOST" "prlctl exec '$VM' …"`; user commands as `runuser -u willow -- env HOME=$vm_home USER=willow LOGNAME=willow …`. Subcommands:
- `check` — `prlctl status`; a stopped VM is started (`prlctl start "Fedora Linux"`, spec §2) and polled with `prlctl exec … true` for up to 180 s; prints `/etc/fedora-release`, `uname -m`, `id willow`, and `plasma session: yes|no` (`pgrep -u willow -x plasmashell`).
- `push` — warns when the work tree is dirty (only `HEAD` is sent); `git archive --format=tar.gz --prefix=ressac/ HEAD`; probes stdin forwarding with `printf ok | ssh … "prlctl exec '$VM' cat"`; `ok` → streams the archive into `tar -xzf - -C $vm_home/src/ressac.new` (as willow); otherwise base64 chunks of 48 KB appended to `/tmp/ressac.tar.gz.b64` by successive `prlctl exec` calls, then decoded and extracted; swaps `ressac.new/ressac` into `$vm_home/src/ressac` (the old tree removed; the build dir kept); checks `sha256sum` of the VM's `Cargo.lock` against `git show HEAD:Cargo.lock | shasum -a 256`; prints `pushed <short sha> (Cargo.lock matches)`.
- `install [--software-rendering]` — runs `cd $vm_home/src/ressac && CARGO_TARGET_DIR=$vm_home/.cache/ressac-build apps/linux/install.sh [--software-rendering]` as willow, prints its output and wall time, keeps its exit code.
- `dnf <package…>` — refuses unless `RESSAC_VM_DNF_OK=1` (Clément's go-ahead); then `prlctl exec '$VM' dnf install -y <packages>` (root).
- `shots <label> [--software-rendering] [--font <systemSans|systemSerif|newsreader|atkinson>]` — runs `marghe.sh --in-vm shots …` from the pushed tree as willow; the VM side: no `plasmashell` for willow → exit 3 with `Clément is not logged in to Plasma on the VM`; takes `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`, `QT_QPA_PLATFORMTHEME`, `KDE_FULL_SESSION` and `DISPLAY` from `/proc/<plasmashell pid>/environ`; saves `~/.config/ressac/ressac-kde.conf` (or notes it is absent); with `--font`, writes `[General]` / `reader.font=<font>` into it; copies the fixture to `/tmp/ressac-vm/<label>/r.sqlite`; runs `RESSAC_DB=…/r.sqlite QT_FORCE_STDERR_LOGGING=1 [QT_QUICK_BACKEND=software QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu] timeout 120 $vm_home/.local/bin/ressac-kde --screenshot /tmp/ressac-vm/<label>/shots > …/run.log 2>&1`, appends `exit=<code>`; restores (or removes) the settings file; then the Mac side pulls `tar -czf - -C /tmp/ressac-vm/<label> . | base64 -w0` and unpacks it, printing the local path.
- `manual` — starts `ressac-kde` detached in the session (same environment, `RESSAC_DB=/tmp/ressac-vm/manual/r.sqlite`, a fresh fixture copy, no `--screenshot`) for Clément's hands-on check; `manual stop` ends it (`pkill -u willow -x ressac-kde`) and removes the copy.

- [ ] **Step 1: Gate and clean tree.** The full gate (Mac and Linux) is green on `HEAD`; `git status --porcelain` prints nothing.
- [ ] **Step 2: Write `apps/linux/dev/marghe.sh`** as above. Check: `bash -n apps/linux/dev/marghe.sh && (command -v shellcheck >/dev/null && shellcheck apps/linux/dev/marghe.sh || true)`; `apps/linux/dev/marghe.sh` without arguments prints the usage and exits 2.
- [ ] **Step 3: Reach the VM** — `apps/linux/dev/marghe.sh check` → `Fedora release 44`, `aarch64`, `uid=…(willow)`, and `plasma session: yes|no`.
- [ ] **Step 4: Push** — `apps/linux/dev/marghe.sh push` → `pushed <sha> (Cargo.lock matches)`; note which path worked (stdin or chunks).
- [ ] **Step 5: Install** — `apps/linux/dev/marghe.sh install`. Expected either `install.sh: installed ressac and ressac-kde in …/.local/bin` (go to Step 7) or `install.sh: missing Fedora packages; install them with:` and a `sudo dnf install …` line (go to Step 6).
- [ ] **Step 6: ⏸ STOP — ask Clément** (exact message, with the package line copied from Step 5): « Point d'étape VM 1 : l'appli est copiée sur la VM Fedora de marghe (commit `<sha>`). Pour la compiler il manque ces paquets : `<liste>`. Puis-je lancer `dnf install` dans la VM (première fois, ton accord est requis) ? Et peux-tu ouvrir ta session Plasma sur la VM ? » Wait for his answer. With his go-ahead: `RESSAC_VM_DNF_OK=1 apps/linux/dev/marghe.sh dnf <packages>`, then `apps/linux/dev/marghe.sh install` again → `installed …` (record the release build time).
- [ ] **Step 7: Screenshots in his session** — `apps/linux/dev/marghe.sh check` must say `plasma session: yes` (else ask him to log in and wait). Run `apps/linux/dev/marghe.sh shots gpu`. Expected: `exit=0` in `run.log`, the `reader-proof:` line with every mark `null`, the walk lines of `tests/keys.rs` and `tests/reader.rs` (the `07`–`22` steps), PNGs at device pixel ratio 2 (`file 01-ready.png` → about 2200 × 1440). Look at `03-starred.png`, `07-full-article.png`, `11-notice.png`, `14-space-in-reader.png`, `21-menu.png` with the Read tool: article text drawn (not blank, black or garbled), Breeze style and icons, crisp text at scale 2. Grep `run.log` for `virgl`, `GL`, `QRhi`, `GPU process`, `crash`.
- [ ] **Step 8: Software rendering, only if Step 7 shows a blank, black or flickering reader** — `apps/linux/dev/marghe.sh shots software --software-rendering`; compare; if it fixes it, `apps/linux/dev/marghe.sh install --software-rendering` so the `.desktop` file carries the switch, and say so in the verdict.
- [ ] **Step 9: Fonts** — `apps/linux/dev/marghe.sh shots newsreader --font newsreader` → `03-starred.png` in Newsreader (served by `ressac:asset/`; a serif with Newsreader's shapes, not Noto Serif), crisp at scale 2.
- [ ] **Step 10: Real keys** — `apps/linux/dev/marghe.sh manual`, then ask Clément (exact message): « L'appli est ouverte dans ta session sur une copie de la base de test. Peux-tu cliquer dans un article puis taper Espace (défile, puis passe au suivant non lu), j, k, m, s et les flèches, puis Ctrl+R et Ctrl+Maj+A, et me dire si tout réagit et si la barre de menus apparaît dans le menu global de Plasma (si tu l'utilises) ? » Record his answer; `apps/linux/dev/marghe.sh manual stop`.
- [ ] **Step 11: Verdict in `notes.md`** (French, no em dash in prose), under « Ressac sous Linux (sous-projet 8) »: « ### Point d'étape VM 1 (B3, <date>) » with: commit testé et chemin de transfert (entrée standard ou morceaux base64); paquets installés par dnf (ou aucun); durée de la compilation release; rendu (virgl tenu ou rendu logiciel imposé, avec la raison); page de l'article (dessinée, preuve des scripts : aucun script d'article exécuté); polices à l'échelle 2 (Noto Sans, Newsreader); touches (parcours automatique, puis vérification de Clément au clavier réel); menu global; anomalies. Each anomaly not fixed now gets a `TODO.md` line under the Linux section.
- [ ] **Step 12: Commit**

```bash
git add apps/linux/dev/marghe.sh notes.md TODO.md
git commit -m "Linux: VM checkpoint 1 on marghe (marghe.sh transfer and session run, verdict)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01AuT5b6J38qtEBs4rgmJ6rb"
```

(`TODO.md` only when Step 11 added a line.)

- [ ] **Step 13: ⏸ STOP for Clément.** Report the verdict (rendering, keys, fonts at scale 2, anything deferred) with the paths of the pulled screenshots, and wait. **B4 does not start before Clément has read the verdict.** Nothing is pushed.
