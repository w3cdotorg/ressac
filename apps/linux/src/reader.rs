//! The `Reader` QML singleton and the `ressac:` scheme's Rust side. The page and its identity
//! are `ressac_ui::article`'s; this object loads the selected entry, builds the page when its
//! identity changes and keeps it (with the proof page and the bundled fonts) where the C++
//! scheme handler asks for it, `ressac_scheme_serve`.

use std::cell::RefCell;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use ressac_core::store::{EntryDetail, FulltextRow};
use ressac_ui::article::{
    ASSET_PREFIX, ArticlePage, ColorScheme, FullText, PAGE_PREFIX, PROOF_PAGE, PROOF_SCRIPT,
    PROOF_URL, RECETTE_PROBE, ReaderStyle, ReaderTheme, asset, page_url,
};
use ressac_ui::chrono;
use ressac_ui::runs::read_failure;
use ressac_ui::sanitize::is_web_url;
use ressac_ui::settings::{Appearance, LineWidth, ReaderFont, effective_size};

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
        #[qproperty(f64, text_size, READ = text_size, WRITE = set_text_size, NOTIFY)]
        #[qproperty(QString, reader_width, READ = reader_width, WRITE = set_reader_width, NOTIFY)]
        #[qproperty(bool, has_entry, READ = has_entry, NOTIFY = state_changed)]
        #[qproperty(QString, page_url, READ = page_url, NOTIFY = page_changed)]
        #[qproperty(QString, link, READ = link, NOTIFY = state_changed)]
        #[qproperty(bool, is_starred, READ = is_starred, NOTIFY = state_changed)]
        #[qproperty(QString, proof_url, READ = proof_url, CONSTANT)]
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
        fn reader_width(self: &Reader) -> QString;
        fn set_reader_width(self: Pin<&mut Reader>, value: &QString);
        fn has_entry(self: &Reader) -> bool;
        fn page_url(self: &Reader) -> QString;
        fn link(self: &Reader) -> QString;
        fn is_starred(self: &Reader) -> bool;
        fn proof_url(self: &Reader) -> QString;

        /// Reads the selected entry again (the data changed) and rebuilds the page.
        #[qinvokable]
        fn reload(self: Pin<&mut Reader>);

        /// The recette's app-world probe of the proof page.
        #[qinvokable]
        fn probe_script(self: &Reader) -> QString;

        /// The entry, its flags or the page state changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Reader>);

        /// The page identity changed: `pageUrl` is another page.
        #[qsignal]
        fn page_changed(self: Pin<&mut Reader>);

        /// A read failed: tell the window's banner.
        #[qsignal]
        fn banner(self: Pin<&mut Reader>, message: &QString, show_log: bool);
    }
}

pub struct ReaderRust {
    entry_id: i64,
    color_scheme: QString,
    look: QString,
    reader_font: QString,
    text_size: f64,
    reader_width: QString,
    detail: Option<EntryDetail>,
    fulltext: Option<FulltextRow>,
    full_text: FullText,
    page: Option<ArticlePage>,
    revision: u64,
    page_url: QString,
}

impl Default for ReaderRust {
    fn default() -> Self {
        ReaderRust {
            entry_id: -1,
            color_scheme: QString::default(),
            look: QString::default(),
            reader_font: QString::default(),
            text_size: ReaderStyle::default().size,
            reader_width: QString::default(),
            detail: None,
            fulltext: None,
            full_text: FullText::default(),
            page: None,
            revision: 0,
            page_url: QString::default(),
        }
    }
}

impl ReaderRust {
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
                crt: false,
            },
            subpage: None,
        })
    }
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
        self.as_mut().entry_id_changed();
        self.as_mut().read(true);
        self.rebuild();
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

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().read(false);
        self.rebuild();
    }

    /// Reads the entry and its stored full article; a new selection (`selecting`) also resets
    /// the Full Article state. A failed read leaves the page as it was and tells the banner.
    fn read(mut self: Pin<&mut Self>, selecting: bool) {
        let id = self.rust().entry_id;
        let read = if id < 0 {
            Some((Ok(None), Ok(None)))
        } else {
            with_session(|session| (session.store.entry_detail(id), session.store.fulltext(id)))
        };
        let (detail, fulltext) = match read {
            // No session (the launcher has not finished): nothing to read yet.
            None => return,
            Some((Err(error), _)) | Some((_, Err(error))) => {
                let banner = read_failure(&error);
                self.as_mut()
                    .banner(&QString::from(banner.message.as_str()), banner.show_log);
                return;
            }
            Some((Ok(detail), Ok(fulltext))) => (detail, fulltext),
        };
        let rust = self.rust_mut().get_mut();
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

fn stored_page(url: &str) -> Option<String> {
    PAGE.with(|slot| {
        slot.try_borrow()
            .ok()?
            .as_ref()
            .filter(|(stored, _)| stored == url)
            .map(|(_, html)| html.clone())
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
/// assets. False for any other address (the handler then answers "not found"). Called from C++:
/// it never panics.
pub fn ressac_scheme_serve(url: &str, mime: &mut String, body: &mut Vec<u8>) -> bool {
    let (kind, bytes) = if url == PROOF_URL {
        ("text/html; charset=utf-8", PROOF_PAGE.as_bytes().to_vec())
    } else if url.starts_with(PAGE_PREFIX) {
        match stored_page(url) {
            Some(html) => ("text/html; charset=utf-8", html.into_bytes()),
            None => return false,
        }
    } else if let Some(file) = url.strip_prefix(ASSET_PREFIX) {
        match (asset(file), asset_bytes(file)) {
            (Some(known), Some(bytes)) => (known.mime, bytes.to_vec()),
            _ => return false,
        }
    } else {
        return false;
    };
    mime.clear();
    mime.push_str(kind);
    body.clear();
    body.extend_from_slice(&bytes);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
