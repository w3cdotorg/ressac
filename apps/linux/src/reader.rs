//! The `Reader` QML singleton and the `ressac:` scheme's Rust side. The page and its identity
//! are `ressac_ui::article`'s; this object loads the selected entry, builds the page when its
//! identity changes and keeps it (with the proof page and the bundled fonts) where the C++
//! scheme handler asks for it, `ressac_scheme_serve`.

use std::cell::RefCell;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use ressac_core::service::{ExtractOutcome, NullSink};
use ressac_core::store::{EntryDetail, FulltextRow};
use ressac_ui::article::{
    ASSET_PREFIX, ArticlePage, ColorScheme, FullText, FullTextStep, Navigation, PAGE_PREFIX,
    PROOF_PAGE, PROOF_SCRIPT, PROOF_URL, RECETTE_PROBE, ReaderStyle, ReaderTheme, SubpageLabel,
    asset, decide, full_article_help, mark_failed_script, page_down_script, page_url,
    subpage_script, toggle_all_images_script, toggle_image_script,
};
use ressac_ui::chart;
use ressac_ui::chrono;
use ressac_ui::runs::read_failure;
use ressac_ui::sanitize::is_web_url;
use ressac_ui::settings::{Appearance, LineWidth, ReaderFont, effective_size};
use ressac_ui::worker::JobResult;

use crate::launcher::with_session;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i64, entry_id, READ = entry_id, WRITE = set_entry_id, NOTIFY)]
        #[qproperty(QString, color_scheme, READ = color_scheme, WRITE = set_color_scheme, NOTIFY)]
        #[qproperty(QString, look, READ = look, WRITE = set_look, NOTIFY)]
        #[qproperty(QString, reader_font, READ = reader_font, WRITE = set_reader_font, NOTIFY)]
        #[qproperty(bool, crt, READ = crt, WRITE = set_crt, NOTIFY)]
        #[qproperty(f64, text_size, READ = text_size, WRITE = set_text_size, NOTIFY)]
        #[qproperty(QString, reader_width, READ = reader_width, WRITE = set_reader_width, NOTIFY)]
        #[qproperty(bool, has_entry, READ = has_entry, NOTIFY = state_changed)]
        #[qproperty(QString, page_url, READ = page_url, NOTIFY = page_changed)]
        #[qproperty(QString, link, READ = link, NOTIFY = state_changed)]
        #[qproperty(bool, is_starred, READ = is_starred, NOTIFY = state_changed)]
        #[qproperty(QString, proof_url, READ = proof_url, CONSTANT)]
        #[qproperty(bool, is_showing_full_text, READ = is_showing_full_text, NOTIFY = state_changed)]
        #[qproperty(bool, is_extracting, READ = is_extracting, NOTIFY = state_changed)]
        #[qproperty(bool, can_request_full_text, READ = can_request_full_text, NOTIFY = state_changed)]
        #[qproperty(QString, full_text_help, READ = full_text_help, NOTIFY = state_changed)]
        #[qproperty(QString, run_failure_notice, READ = run_failure_notice, NOTIFY = state_changed)]
        // Antiope's sub-page line (`Antiope.subpage*`): page (−1 none), index (0 no label), count.
        #[qproperty(i32, subpage_page, READ, WRITE = set_subpage_page, NOTIFY = subpage_changed)]
        #[qproperty(i32, subpage_index, READ, WRITE = set_subpage_index, NOTIFY = subpage_changed)]
        #[qproperty(i32, subpage_count, READ, WRITE = set_subpage_count, NOTIFY = subpage_changed)]
        type Reader = super::ReaderRust;

        fn entry_id(self: &Reader) -> i64;
        fn set_entry_id(self: Pin<&mut Reader>, value: i64);
        fn color_scheme(self: &Reader) -> QString;
        fn set_color_scheme(self: Pin<&mut Reader>, value: &QString);
        fn look(self: &Reader) -> QString;
        fn set_look(self: Pin<&mut Reader>, value: &QString);
        fn reader_font(self: &Reader) -> QString;
        fn set_reader_font(self: Pin<&mut Reader>, value: &QString);
        fn text_size(self: &Reader) -> f64;
        fn set_text_size(self: Pin<&mut Reader>, value: f64);
        fn crt(self: &Reader) -> bool;
        fn set_crt(self: Pin<&mut Reader>, value: bool);
        fn reader_width(self: &Reader) -> QString;
        fn set_reader_width(self: Pin<&mut Reader>, value: &QString);
        fn has_entry(self: &Reader) -> bool;
        fn page_url(self: &Reader) -> QString;
        fn link(self: &Reader) -> QString;
        fn is_starred(self: &Reader) -> bool;
        fn proof_url(self: &Reader) -> QString;
        fn is_showing_full_text(self: &Reader) -> bool;
        fn is_extracting(self: &Reader) -> bool;
        fn can_request_full_text(self: &Reader) -> bool;
        fn full_text_help(self: &Reader) -> QString;
        fn run_failure_notice(self: &Reader) -> QString;
        fn set_subpage_page(self: Pin<&mut Reader>, value: i32);
        fn set_subpage_index(self: Pin<&mut Reader>, value: i32);
        fn set_subpage_count(self: Pin<&mut Reader>, value: i32);

        /// Reads the selected entry again (the data changed) and rebuilds the page.
        #[qinvokable]
        fn reload(self: Pin<&mut Reader>);

        /// The recette's app-world probe of the proof page.
        #[qinvokable]
        fn probe_script(self: &Reader) -> QString;

        /// The Full Article toggle: the feed's text, the stored article, or a fetch.
        #[qinvokable]
        fn request_full_text(self: Pin<&mut Reader>);

        /// The link policy's answer for a navigation: "allow", "cancel", "open:<url>" or
        /// "toggle:<n>". `kind` is `WebEngineNavigationRequest.navigationType`.
        #[qinvokable]
        fn decide(
            self: &Reader,
            url: &QString,
            kind: i32,
            main_frame: bool,
            expected: &QString,
        ) -> QString;

        /// The app-world script of Space: scroll the page `expected` is, or say why not.
        #[qinvokable]
        fn page_down_script(self: &Reader, expected: &QString) -> QString;

        /// The app-world script rewriting the page's sub-page line in place ("412 3/20").
        #[qinvokable]
        fn subpage_script(self: &Reader) -> QString;

        /// The app-world script swapping mosaic `index` and its original (a click on the
        /// mosaic); evaluates to whether the original now shows (`false` for no such box).
        #[qinvokable]
        fn toggle_image_script(self: &Reader, index: i32) -> QString;

        /// The app-world script of `i`: every image of the article both ways; evaluates to
        /// how many boxes it swapped.
        #[qinvokable]
        fn toggle_all_script(self: &Reader) -> QString;

        /// The app-world script flagging every box showing the mosaic address `url` as failed
        /// (`Mosaics.failed(url)`), so the IMAGE line shows.
        #[qinvokable]
        fn mark_failed_script(self: &Reader, url: &QString) -> QString;

        /// The entry, its flags or the page state changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Reader>);

        /// The sub-page label changed; the page keeps its address (`subpageScript` updates it).
        #[qsignal]
        fn subpage_changed(self: Pin<&mut Reader>);

        /// The page identity changed: `pageUrl` is another page.
        #[qsignal]
        fn page_changed(self: Pin<&mut Reader>);

        /// A read failed: tell the window's banner.
        #[qsignal]
        fn banner(self: Pin<&mut Reader>, message: &QString, show_log: bool);

        /// The reader wrote to the database (a fetched full article): the lists reload.
        #[qsignal]
        fn wrote(self: Pin<&mut Reader>);
    }

    impl cxx_qt::Threading for Reader {}
}

pub struct ReaderRust {
    entry_id: i64,
    color_scheme: QString,
    look: QString,
    reader_font: QString,
    text_size: f64,
    /// The CRT's reader bleed (`data-crt`), only in Antiope (`ReaderTheme::shows_crt`).
    crt: bool,
    reader_width: QString,
    detail: Option<EntryDetail>,
    fulltext: Option<FulltextRow>,
    full_text: FullText,
    page: Option<ArticlePage>,
    revision: u64,
    page_url: QString,
    /// A new selection could not be read yet (no session): the next read resets the toggle.
    select_pending: bool,
    subpage_page: i32,
    subpage_index: i32,
    subpage_count: i32,
}

impl Default for ReaderRust {
    fn default() -> Self {
        ReaderRust {
            entry_id: -1,
            color_scheme: QString::default(),
            look: QString::default(),
            reader_font: QString::default(),
            text_size: ReaderStyle::default().size,
            crt: false,
            reader_width: QString::default(),
            detail: None,
            fulltext: None,
            full_text: FullText::default(),
            page: None,
            revision: 0,
            page_url: QString::default(),
            select_pending: false,
            subpage_page: -1,
            subpage_index: 0,
            subpage_count: 0,
        }
    }
}

impl ReaderRust {
    /// The selected entry's id.
    fn selected_id(&self) -> Option<i64> {
        self.detail.as_ref().map(|detail| detail.row.entry_id)
    }

    /// The page the current state asks for; `None` without an entry.
    fn wanted_page(&self) -> Option<ArticlePage> {
        let entry = self.detail.clone()?;
        let look = Appearance::parse(&self.look.to_string()).unwrap_or(Appearance::Native);
        let defaults = ReaderStyle::default();
        Some(ArticlePage {
            entry,
            fulltext: self.fulltext.clone(),
            show_full: self.full_text.shows(),
            style: ReaderStyle {
                font: ReaderFont::parse(&self.reader_font.to_string()).unwrap_or(defaults.font),
                size: effective_size(self.text_size, look),
                width: LineWidth::parse(&self.reader_width.to_string()).unwrap_or(defaults.width),
            },
            scheme: ColorScheme::parse(&self.color_scheme.to_string())
                .unwrap_or(ColorScheme::Light),
            theme: ReaderTheme {
                appearance: look,
                crt: self.crt,
            },
            subpage: self.subpage_label(),
        })
    }

    fn subpage_label(&self) -> Option<SubpageLabel> {
        subpage_label(self.subpage_page, self.subpage_index, self.subpage_count)
    }
}

/// The label of `Antiope.subpage*`: `None` for index 0 (no entry, or not in the list); a page
/// below 0 or beyond `u16` is no page ("3/20").
fn subpage_label(page: i32, index: i32, count: i32) -> Option<SubpageLabel> {
    let index = usize::try_from(index).ok().filter(|index| *index > 0)?;
    Some(SubpageLabel {
        page: u16::try_from(page).ok(),
        index,
        count: usize::try_from(count).unwrap_or(0),
    })
}

impl qobject::Reader {
    fn entry_id(&self) -> i64 {
        self.rust().entry_id
    }

    fn set_entry_id(mut self: Pin<&mut Self>, value: i64) {
        if self.rust().entry_id == value {
            return;
        }
        self.as_mut().rust_mut().get_mut().entry_id = value;
        self.as_mut().read(true);
        self.as_mut().rebuild();
        self.entry_id_changed();
    }

    fn color_scheme(&self) -> QString {
        self.rust().color_scheme.clone()
    }

    fn set_color_scheme(mut self: Pin<&mut Self>, value: &QString) {
        if self.rust().color_scheme == *value {
            return;
        }
        self.as_mut().rust_mut().get_mut().color_scheme = value.clone();
        self.as_mut().color_scheme_changed();
        self.rebuild();
    }

    fn look(&self) -> QString {
        self.rust().look.clone()
    }

    fn set_look(mut self: Pin<&mut Self>, value: &QString) {
        if self.rust().look == *value {
            return;
        }
        self.as_mut().rust_mut().get_mut().look = value.clone();
        self.as_mut().look_changed();
        self.rebuild();
    }

    fn reader_font(&self) -> QString {
        self.rust().reader_font.clone()
    }

    fn set_reader_font(mut self: Pin<&mut Self>, value: &QString) {
        if self.rust().reader_font == *value {
            return;
        }
        self.as_mut().rust_mut().get_mut().reader_font = value.clone();
        self.as_mut().reader_font_changed();
        self.rebuild();
    }

    fn text_size(&self) -> f64 {
        self.rust().text_size
    }

    fn set_text_size(mut self: Pin<&mut Self>, value: f64) {
        if self.rust().text_size == value {
            return;
        }
        self.as_mut().rust_mut().get_mut().text_size = value;
        self.as_mut().text_size_changed();
        self.rebuild();
    }

    fn crt(&self) -> bool {
        self.rust().crt
    }

    /// A change that alters the page (`ArticlePage::same_page`: only in Antiope) reloads it once.
    fn set_crt(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().crt == value {
            return;
        }
        self.as_mut().rust_mut().get_mut().crt = value;
        self.as_mut().crt_changed();
        self.rebuild();
    }

    fn reader_width(&self) -> QString {
        self.rust().reader_width.clone()
    }

    fn set_reader_width(mut self: Pin<&mut Self>, value: &QString) {
        if self.rust().reader_width == *value {
            return;
        }
        self.as_mut().rust_mut().get_mut().reader_width = value.clone();
        self.as_mut().reader_width_changed();
        self.rebuild();
    }

    fn has_entry(&self) -> bool {
        self.rust().detail.is_some()
    }

    fn page_url(&self) -> QString {
        self.rust().page_url.clone()
    }

    /// The entry's address when it is a web link.
    fn link(&self) -> QString {
        let url = self
            .rust()
            .detail
            .as_ref()
            .and_then(|detail| detail.row.url.as_deref())
            .filter(|url| is_web_url(url));
        QString::from(url.unwrap_or(""))
    }

    fn is_starred(&self) -> bool {
        self.rust()
            .detail
            .as_ref()
            .is_some_and(|detail| detail.row.is_starred)
    }

    fn proof_url(&self) -> QString {
        QString::from(PROOF_URL)
    }

    fn probe_script(&self) -> QString {
        QString::from(RECETTE_PROBE)
    }

    fn decide(&self, url: &QString, kind: i32, main_frame: bool, expected: &QString) -> QString {
        let decision = decide(
            Navigation::from_qt(kind),
            &url.to_string(),
            main_frame,
            &expected.to_string(),
        );
        QString::from(decision.encode().as_str())
    }

    fn page_down_script(&self, expected: &QString) -> QString {
        QString::from(page_down_script(&expected.to_string()).as_str())
    }

    fn is_showing_full_text(&self) -> bool {
        let rust = self.rust();
        rust.full_text.is_showing(rust.fulltext.as_ref())
    }

    fn is_extracting(&self) -> bool {
        let rust = self.rust();
        rust.full_text.is_extracting(rust.selected_id())
    }

    fn can_request_full_text(&self) -> bool {
        let rust = self.rust();
        rust.full_text
            .can_request(rust.detail.as_ref().map(|detail| &detail.row))
    }

    fn full_text_help(&self) -> QString {
        let rust = self.rust();
        let failure = rust
            .full_text
            .failure(rust.selected_id(), rust.fulltext.as_ref());
        QString::from(
            full_article_help(
                rust.detail.as_ref().map(|detail| &detail.row),
                self.is_showing_full_text(),
                self.is_extracting(),
                failure.as_deref(),
            )
            .as_str(),
        )
    }

    fn run_failure_notice(&self) -> QString {
        let rust = self.rust();
        let notice = rust
            .full_text
            .run_failure_notice(rust.selected_id(), rust.fulltext.as_ref());
        QString::from(notice.unwrap_or_default().as_str())
    }

    fn set_subpage_page(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().subpage_page != value {
            self.as_mut().rust_mut().get_mut().subpage_page = value;
            self.subpage_moved();
        }
    }

    fn set_subpage_index(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().subpage_index != value {
            self.as_mut().rust_mut().get_mut().subpage_index = value;
            self.subpage_moved();
        }
    }

    fn set_subpage_count(mut self: Pin<&mut Self>, value: i32) {
        if self.rust().subpage_count != value {
            self.as_mut().rust_mut().get_mut().subpage_count = value;
            self.subpage_moved();
        }
    }

    /// The label is not part of the page identity: the page keeps its address (no reload);
    /// the stored HTML gets the new line, so a later load of the same address shows it too.
    fn subpage_moved(mut self: Pin<&mut Self>) {
        let rust = self.as_mut().rust_mut().get_mut();
        let label = rust.subpage_label();
        if let Some(page) = rust.page.as_mut()
            && page.subpage != label
        {
            page.subpage = label;
            if page.theme.appearance == Appearance::Antiope {
                store_page(&rust.page_url.to_string(), page.html(&chrono::Local));
            }
        }
        self.subpage_changed();
    }

    fn subpage_script(&self) -> QString {
        let text = self
            .rust()
            .subpage_label()
            .map(|label| label.text())
            .unwrap_or_default();
        QString::from(subpage_script(&text).as_str())
    }

    fn toggle_image_script(&self, index: i32) -> QString {
        match u32::try_from(index) {
            Ok(index) => QString::from(toggle_image_script(index).as_str()),
            Err(_) => QString::from("false"),
        }
    }

    fn toggle_all_script(&self) -> QString {
        QString::from(toggle_all_images_script().as_str())
    }

    fn mark_failed_script(&self, url: &QString) -> QString {
        QString::from(mark_failed_script(&url.to_string()).as_str())
    }

    fn request_full_text(mut self: Pin<&mut Self>) {
        // Without a session (cannot happen once the window is up) there is nothing to fetch with.
        if with_session(|_| ()).is_none() {
            return;
        }
        let rust = self.as_mut().rust_mut().get_mut();
        let Some(row) = rust.detail.as_ref().map(|detail| detail.row.clone()) else {
            return;
        };
        match rust.full_text.request(&row, rust.fulltext.as_ref()) {
            FullTextStep::Nothing => {}
            FullTextStep::ShowFeedText | FullTextStep::ShowStored => self.rebuild(),
            FullTextStep::Extract(id) => {
                self.as_mut().state_changed();
                let qt = self.qt_thread();
                with_session(|session| {
                    session.worker.short(
                        move |cx| cx.block_on(cx.app.extract_entry(id, &NullSink)),
                        move |result| {
                            let queued =
                                qt.queue(move |reader| reader.extraction_finished(id, result));
                            if queued.is_err() {
                                tracing::debug!("the app quit before an extraction finished");
                            }
                        },
                    );
                });
            }
        }
    }

    /// A fetch is over, back on the GUI thread: a failure becomes the notice, the stored
    /// result (article or error) is read again, and the lists reload.
    fn extraction_finished(mut self: Pin<&mut Self>, id: i64, result: JobResult<ExtractOutcome>) {
        let selected = self.rust().selected_id();
        self.as_mut()
            .rust_mut()
            .get_mut()
            .full_text
            .finished(id, selected, result);
        self.as_mut().reload();
        self.wrote();
    }

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().read(false);
        self.rebuild();
    }

    /// Reads the entry and its stored full article; a new selection (`selecting`) also resets
    /// the Full Article state. A failed read leaves the page as it was and tells the banner;
    /// before the session exists nothing is read and the next read (`reload`) catches up.
    fn read(mut self: Pin<&mut Self>, selecting: bool) {
        let selecting = selecting || self.rust().select_pending;
        let id = self.rust().entry_id;
        let read = if id < 0 {
            Some((Ok(None), Ok(None)))
        } else {
            with_session(|session| (session.store.entry_detail(id), session.store.fulltext(id)))
        };
        let (detail, fulltext) = match read {
            // No session (the launcher has not finished): nothing to read yet.
            None => {
                self.rust_mut().get_mut().select_pending = selecting;
                return;
            }
            Some((Err(error), _)) | Some((_, Err(error))) => {
                let banner = read_failure(&error);
                self.as_mut()
                    .banner(&QString::from(banner.message.as_str()), banner.show_log);
                return;
            }
            Some((Ok(detail), Ok(fulltext))) => (detail, fulltext),
        };
        let rust = self.rust_mut().get_mut();
        rust.select_pending = false;
        if selecting {
            rust.full_text
                .select(detail.as_ref().map(|detail| &detail.row));
        }
        // The stored article belongs to this entry only (it is read by the entry's id).
        rust.fulltext = detail.as_ref().and(fulltext);
        rust.detail = detail;
    }

    /// Builds the page for the state; a new page identity gets a new address and its HTML in
    /// the store, the previous page's gone.
    fn rebuild(mut self: Pin<&mut Self>) {
        let rust = self.as_mut().rust_mut().get_mut();
        let wanted = rust.wanted_page();
        let new_identity = match (&rust.page, &wanted) {
            (None, None) => false,
            (Some(old), Some(new)) => !old.same_page(new),
            _ => true,
        };
        if new_identity {
            rust.revision += 1;
            match &wanted {
                Some(page) => {
                    let url = page_url(page.entry.row.entry_id, rust.revision);
                    store_page(&url, page.html(&chrono::Local));
                    rust.page_url = QString::from(url.as_str());
                }
                None => {
                    clear_page();
                    rust.page_url = QString::default();
                }
            }
        }
        rust.page = wanted;
        if new_identity {
            self.as_mut().page_changed();
        }
        self.state_changed();
    }
}

thread_local! {
    /// The entry page on screen, by address: the scheme handler runs on the GUI thread.
    static PAGE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Keeps `html` under `url`, replacing the previous page.
fn store_page(url: &str, html: String) {
    PAGE.with(|slot| *slot.borrow_mut() = Some((url.to_string(), html)));
}

fn clear_page() {
    PAGE.with(|slot| *slot.borrow_mut() = None);
}

/// Appends the page stored under `url` to `body`; false when it is not the stored one.
fn write_stored_page(url: &str, body: &mut Vec<u8>) -> bool {
    PAGE.with(|slot| {
        let Ok(slot) = slot.try_borrow() else {
            return false;
        };
        match slot.as_ref().filter(|(stored, _)| stored == url) {
            Some((_, html)) => {
                body.extend_from_slice(html.as_bytes());
                true
            }
            None => false,
        }
    })
}

/// The bytes of a bundled asset (the Mac's fonts, reused in place).
fn asset_bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "Newsreader-Variable.ttf" => {
            include_bytes!("../../macos/Ressac/Resources/Fonts/Newsreader-Variable.ttf").as_slice()
        }
        "Newsreader-Italic-Variable.ttf" => {
            include_bytes!("../../macos/Ressac/Resources/Fonts/Newsreader-Italic-Variable.ttf")
                .as_slice()
        }
        "AtkinsonHyperlegibleNext-Variable.ttf" => include_bytes!(
            "../../macos/Ressac/Resources/Fonts/AtkinsonHyperlegibleNext-Variable.ttf"
        )
        .as_slice(),
        "AtkinsonHyperlegibleNext-Italic-Variable.ttf" => include_bytes!(
            "../../macos/Ressac/Resources/Fonts/AtkinsonHyperlegibleNext-Italic-Variable.ttf"
        )
        .as_slice(),
        "bedstead.otf" => {
            include_bytes!("../../macos/Ressac/Resources/Fonts/bedstead.otf").as_slice()
        }
        "proof.js" => PROOF_SCRIPT.as_bytes(),
        _ => return None,
    })
}

/// What the C++ `ressac:` handler serves: the proof page, the current entry page, the bundled
/// assets, written straight into `body`. False for any other address (the handler then answers
/// "not found"). Called from C++: it never panics.
pub fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool {
    const HTML: &str = "text/html; charset=utf-8";
    body.clear();
    let kind = if url == PROOF_URL {
        body.extend_from_slice(PROOF_PAGE.as_bytes());
        HTML
    } else if let Some(path) = url.strip_prefix(chart::PREFIX) {
        match (chart::file(path), crate::chart::file_bytes(path)) {
            (Some(known), Some(bytes)) => {
                body.extend_from_slice(bytes);
                known.mime
            }
            _ => return false,
        }
    } else if url.starts_with(PAGE_PREFIX) {
        if !write_stored_page(url, body) {
            return false;
        }
        HTML
    } else if let Some(file) = url.strip_prefix(ASSET_PREFIX) {
        match (asset(file), asset_bytes(file)) {
            (Some(known), Some(bytes)) => {
                body.extend_from_slice(bytes);
                known.mime
            }
            _ => return false,
        }
    } else {
        return false;
    };
    mime.clear();
    mime.push_str(kind);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subpage_label_needs_an_index_and_a_page_in_range() {
        let text = |page, index, count| subpage_label(page, index, count).map(|l| l.text());
        assert_eq!(text(412, 3, 20).as_deref(), Some("412 3/20"));
        assert_eq!(text(-1, 3, 20).as_deref(), Some("3/20"));
        assert_eq!(text(70_000, 3, 20).as_deref(), Some("3/20"));
        assert_eq!(text(412, 0, 20), None);
        assert_eq!(text(412, -2, 20), None);
        assert_eq!(text(412, 1, -5).as_deref(), Some("412 1/0"));
    }

    #[test]
    fn every_asset_is_served_with_its_mime_type_and_bytes() {
        for known in ressac_ui::article::ASSETS {
            let (mut mime, mut body) = (String::new(), vec![0u8; 3]);
            let url = format!("{ASSET_PREFIX}{}", known.file);
            assert!(ressac_scheme_serve(&url, &mut mime, &mut body), "{url}");
            assert_eq!(mime, known.mime, "{url}");
            assert!(!body.is_empty(), "{url}");
            assert_eq!(Some(body.as_slice()), asset_bytes(known.file), "{url}");
        }
    }

    #[test]
    fn assets_and_pages_are_served_and_nothing_else() {
        let (mut mime, mut body) = (String::new(), Vec::new());
        assert!(ressac_scheme_serve(
            "ressac:asset/Newsreader-Variable.ttf",
            &mut mime,
            &mut body
        ));
        assert_eq!(
            (mime.as_str(), &body[..4]),
            ("font/ttf", &[0u8, 1, 0, 0][..])
        );
        body.clear();
        assert!(ressac_scheme_serve(
            "ressac:asset/bedstead.otf",
            &mut mime,
            &mut body
        ));
        assert_eq!((mime.as_str(), &body[..4]), ("font/otf", &b"OTTO"[..]));
        body.clear();
        assert!(ressac_scheme_serve(
            ressac_ui::article::PROOF_URL,
            &mut mime,
            &mut body
        ));
        assert!(
            mime.starts_with("text/html") && String::from_utf8_lossy(&body).contains("data-inline")
        );
        for bad in [
            "ressac:asset/../Cargo.toml",
            "ressac:asset/reader.css",
            "ressac:page/1-1",
            "ressac:other",
            "https://e.org/",
        ] {
            body.clear();
            assert!(!ressac_scheme_serve(bad, &mut mime, &mut body), "{bad}");
        }
        store_page("ressac:page/1-1", "<p>x</p>".into());
        body.clear();
        assert!(
            ressac_scheme_serve("ressac:page/1-1", &mut mime, &mut body) && body == b"<p>x</p>"
        );
        store_page("ressac:page/2-2", "<p>y</p>".into());
        assert!(!ressac_scheme_serve(
            "ressac:page/1-1",
            &mut mime,
            &mut Vec::new()
        ));
    }
}
