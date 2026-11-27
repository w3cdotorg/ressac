//! The `Chart` QML singleton: the Sea Chart window's state and its page bridge. The rules are
//! `ressac_ui::chart`'s (`ChartModel`, `ChartPage`, `handle`, prefs, navigation); this object
//! translates them to Qt, runs the survey on a thread of its own (a query-only connection, not
//! the GUI thread and not the short lane) and serves the Mac's chart files, byte for byte,
//! under `ressac:chart/` through `reader::ressac_scheme_serve`.

use std::pin::Pin;
use std::time::Instant;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use ressac_core::logging::redact_text;
use ressac_ui::chart::{
    self, ADAPTER_SCRIPT, ChartAction, ChartModel, ChartPage, NOT_DRAWN, NOT_OPEN, PAGE_URL,
    PROBE_SCRIPT, PageStep, Phase, RETRY, Survey, TICK_MS, WINDOW_TITLE,
};
use ressac_ui::chrono;
use ressac_ui::library::local_path;
use ressac_ui::settings::Settings;

use crate::launcher::with_session;
use crate::settings::QSettingsStore;

/// The most a page message may weigh before it is parsed: the three real messages are a few
/// dozen bytes, and the page is not trusted with more.
const MAX_MESSAGE: usize = 64 * 1024;

/// The bytes of a chart file, the Mac's own (`apps/macos/Ressac/Resources/Chart`), by the exact
/// path of an `ressac_ui::chart::FILES` entry.
pub(crate) fn file_bytes(path: &str) -> Option<&'static [u8]> {
    macro_rules! bundled {
        ($($file:literal),* $(,)?) => {
            match path {
                $($file => include_bytes!(concat!("../../macos/Ressac/Resources/Chart/", $file))
                    .as_slice(),)*
                _ => return None,
            }
        };
    }
    Some(bundled!(
        "index.html",
        "chart.css",
        "d3.min.js",
        "chart-core.js",
        "chart-survey.js",
        "chart-draw.js",
        "fonts/CormorantGaramond-Medium.woff2",
        "fonts/CormorantGaramond-SemiBold.woff2",
        "fonts/CormorantGaramond-Italic.woff2",
        "fonts/CormorantGaramond-MediumItalic.woff2",
        "fonts/CormorantSC-Medium.woff2",
        "fonts/CormorantSC-SemiBold.woff2",
        "fonts/IBMPlexMono-Regular.woff2",
        "fonts/IBMPlexMono-Medium.woff2",
    ))
}

/// What a page message asks of the app; `None` for anything oversized, malformed or unknown.
fn parse_message(text: &str) -> Option<ChartAction> {
    if text.len() > MAX_MESSAGE {
        return None;
    }
    chart::handle(text)
}

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
        #[qproperty(QString, phase, READ = phase, NOTIFY = state_changed)]
        #[qproperty(QString, message, READ = message, NOTIFY = state_changed)]
        #[qproperty(QString, refresh_error, READ = refresh_error, NOTIFY = state_changed)]
        #[qproperty(bool, open, READ = open, NOTIFY = state_changed)]
        #[qproperty(i32, surveys, READ = surveys, NOTIFY = state_changed)]
        #[qproperty(bool, reduce_motion, READ = reduce_motion, WRITE = set_reduce_motion, NOTIFY)]
        #[qproperty(QString, page_url, READ = page_url, CONSTANT)]
        #[qproperty(QString, adapter_script, READ = adapter_script, CONSTANT)]
        #[qproperty(QString, probe_script, READ = probe_script, CONSTANT)]
        #[qproperty(QString, window_title, READ = window_title, CONSTANT)]
        #[qproperty(QString, not_open_text, READ = not_open_text, CONSTANT)]
        #[qproperty(QString, not_drawn_text, READ = not_drawn_text, CONSTANT)]
        #[qproperty(QString, retry_text, READ = retry_text, CONSTANT)]
        #[qproperty(i32, tick_ms, READ = tick_ms, CONSTANT)]
        type Chart = super::ChartRust;

        fn phase(self: &Chart) -> QString;
        fn message(self: &Chart) -> QString;
        fn refresh_error(self: &Chart) -> QString;
        fn open(self: &Chart) -> bool;
        fn surveys(self: &Chart) -> i32;
        fn reduce_motion(self: &Chart) -> bool;
        fn set_reduce_motion(self: Pin<&mut Chart>, value: bool);
        fn page_url(self: &Chart) -> QString;
        fn adapter_script(self: &Chart) -> QString;
        fn probe_script(self: &Chart) -> QString;
        fn window_title(self: &Chart) -> QString;
        fn not_open_text(self: &Chart) -> QString;
        fn not_drawn_text(self: &Chart) -> QString;
        fn retry_text(self: &Chart) -> QString;
        fn tick_ms(self: &Chart) -> i32;

        /// The window opened: survey now, then on changes and every five minutes.
        #[qinvokable]
        fn open_window(self: Pin<&mut Chart>);

        /// The window closed: the running survey and the schedule are forgotten.
        #[qinvokable]
        fn close_window(self: Pin<&mut Chart>);

        /// The database changed (the reload funnel).
        #[qinvokable]
        fn note_change(self: Pin<&mut Chart>);

        /// The window's 250 ms clock.
        #[qinvokable]
        fn tick(self: Pin<&mut Chart>);

        /// Retry after a failed survey.
        #[qinvokable]
        fn retry(self: Pin<&mut Chart>);

        /// The view began a load: the page starts empty.
        #[qinvokable]
        fn page_load_started(self: Pin<&mut Chart>);

        /// The view ended a load (`url`, whether it succeeded).
        #[qinvokable]
        fn page_loaded(self: Pin<&mut Chart>, url: &QString, ok: bool);

        /// Only the chart page, in the main frame, may be navigated to.
        #[qinvokable]
        fn allows_navigation(self: &Chart, url: &QString, main_frame: bool) -> bool;

        /// A message from the page (the adapter's JSON text).
        #[qinvokable]
        fn receive(self: Pin<&mut Chart>, text: &QString);

        /// Recette: a document file (a `file:` URL) shown as a finished survey would be.
        #[qinvokable]
        fn show_file(self: Pin<&mut Chart>, url: &QString) -> bool;

        /// The phase, the message, the error or the survey count changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Chart>);

        /// Run this script in the page (a document to draw).
        #[qsignal]
        fn run_script(self: Pin<&mut Chart>, script: &QString);

        /// The view was taken elsewhere: load the chart page again.
        #[qsignal]
        fn go_home(self: Pin<&mut Chart>);

        /// The page asked for a feed: `key` is the sidebar's (`feed:<id>`).
        #[qsignal]
        fn open_feed(self: Pin<&mut Chart>, key: &QString);

        /// The page asked for a web address.
        #[qsignal]
        fn open_url(self: Pin<&mut Chart>, url: &QString);

        /// The page chose a view, an ink or animation: store them (`AppSettings.saveChartPrefs`).
        #[qsignal]
        fn prefs_reported(self: Pin<&mut Chart>, view: &QString, ink: &QString, animate: bool);
    }

    impl cxx_qt::Threading for Chart {}
}

#[derive(Default)]
pub struct ChartRust {
    model: ChartModel,
    page: ChartPage,
    reduce_motion: bool,
    surveys: i32,
    /// The survey whose thread is out; a result for any other ticket is stale (a reopened window
    /// gets new tickets) and is neither counted, logged nor shown.
    in_flight: Option<Survey>,
}

impl ChartRust {
    /// The prefs argument of `Chart.load`, from what is stored now.
    fn prefs(&self) -> String {
        chart::page_prefs(
            &Settings::load(&QSettingsStore::default_file()).chart,
            self.reduce_motion,
        )
    }
}

impl qobject::Chart {
    fn phase(&self) -> QString {
        QString::from(self.rust().model.phase().as_str())
    }

    fn message(&self) -> QString {
        match self.rust().model.phase() {
            Phase::Failed(text) => QString::from(text.as_str()),
            _ => QString::default(),
        }
    }

    /// Why the last refresh failed while the old chart stays on screen.
    fn refresh_error(&self) -> QString {
        let model = &self.rust().model;
        match (model.phase(), model.last_error()) {
            (Phase::Ready(_), Some(error)) => QString::from(error),
            _ => QString::default(),
        }
    }

    fn open(&self) -> bool {
        self.rust().model.is_open()
    }

    fn surveys(&self) -> i32 {
        self.rust().surveys
    }

    fn reduce_motion(&self) -> bool {
        self.rust().reduce_motion
    }

    fn set_reduce_motion(mut self: Pin<&mut Self>, value: bool) {
        if self.rust().reduce_motion != value {
            self.as_mut().rust_mut().get_mut().reduce_motion = value;
            self.reduce_motion_changed();
        }
    }

    fn page_url(&self) -> QString {
        QString::from(PAGE_URL)
    }

    fn adapter_script(&self) -> QString {
        QString::from(ADAPTER_SCRIPT)
    }

    fn probe_script(&self) -> QString {
        QString::from(PROBE_SCRIPT)
    }

    fn window_title(&self) -> QString {
        QString::from(WINDOW_TITLE)
    }

    fn not_open_text(&self) -> QString {
        QString::from(NOT_OPEN)
    }

    fn not_drawn_text(&self) -> QString {
        QString::from(NOT_DRAWN)
    }

    fn retry_text(&self) -> QString {
        QString::from(RETRY)
    }

    fn tick_ms(&self) -> i32 {
        TICK_MS
    }

    fn open_window(mut self: Pin<&mut Self>) {
        let survey = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .model
            .open(Instant::now());
        if let Some(survey) = survey {
            self.as_mut().start(survey);
        }
        self.state_changed();
    }

    fn close_window(mut self: Pin<&mut Self>) {
        let this = self.as_mut().rust_mut().get_mut();
        this.model.close();
        this.in_flight = None;
        this.page = ChartPage::default();
        self.state_changed();
    }

    fn note_change(self: Pin<&mut Self>) {
        self.rust_mut().get_mut().model.note_change(Instant::now());
    }

    fn tick(mut self: Pin<&mut Self>) {
        let survey = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .model
            .tick(Instant::now());
        if let Some(survey) = survey {
            self.start(survey);
        }
    }

    fn retry(mut self: Pin<&mut Self>) {
        let survey = self.as_mut().rust_mut().get_mut().model.refresh();
        if let Some(survey) = survey {
            self.start(survey);
        }
    }

    /// Runs a survey on a thread of its own; its result comes back to `survey_done`.
    fn start(mut self: Pin<&mut Self>, survey: Survey) {
        self.as_mut().rust_mut().get_mut().in_flight = Some(survey);
        let Some(db) = with_session(|session| session.worker.db().to_path_buf()) else {
            self.survey_done(survey, Err(NOT_OPEN.to_string()));
            return;
        };
        let qt = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("chart-survey".to_string())
            .spawn(move || {
                let result = chart::survey(&db, chrono::Utc::now());
                if qt
                    .queue(move |chart| chart.survey_done(survey, result))
                    .is_err()
                {
                    tracing::debug!("the app quit during a survey");
                }
            });
        if let Err(error) = spawned {
            self.survey_done(survey, Err(format!("cannot start the survey: {error}")));
        }
    }

    /// A survey's result, back on the GUI thread.
    fn survey_done(mut self: Pin<&mut Self>, survey: Survey, result: Result<String, String>) {
        let this = self.as_mut().rust_mut().get_mut();
        if this.in_flight != Some(survey) {
            return;
        }
        this.in_flight = None;
        if let Err(message) = &result {
            tracing::warn!("chart survey failed: {}", redact_text(message));
        }
        let ok = result.is_ok();
        let next = this.model.finished(survey, result);
        if ok && matches!(this.model.phase(), Phase::Ready(_)) {
            this.surveys = this.surveys.saturating_add(1);
        }
        self.as_mut().show_current();
        if let Some(next) = next {
            self.as_mut().start(next);
        }
        self.state_changed();
    }

    /// Sends the current document to the page when it is loaded and does not show it yet.
    fn show_current(mut self: Pin<&mut Self>) {
        let Phase::Ready(json) = self.rust().model.phase().clone() else {
            return;
        };
        let prefs = self.rust().prefs();
        let script = self.as_mut().rust_mut().get_mut().page.show(&json, &prefs);
        if let Some(script) = script {
            self.run_script(&QString::from(script.as_str()));
        }
    }

    fn page_load_started(self: Pin<&mut Self>) {
        self.rust_mut().get_mut().page.load_started();
    }

    fn page_loaded(mut self: Pin<&mut Self>, url: &QString, ok: bool) {
        let prefs = self.rust().prefs();
        let step =
            self.as_mut()
                .rust_mut()
                .get_mut()
                .page
                .load_finished(&url.to_string(), ok, &prefs);
        match step {
            PageStep::Run(script) => self.run_script(&QString::from(script.as_str())),
            PageStep::GoHome => self.go_home(),
            PageStep::Nothing => {}
        }
    }

    fn allows_navigation(&self, url: &QString, main_frame: bool) -> bool {
        chart::allows_navigation(&url.to_string(), main_frame)
    }

    fn receive(self: Pin<&mut Self>, text: &QString) {
        // The cap is checked on the Qt string first, before any copy into a Rust one: a message
        // of more UTF-16 units than the cap is more bytes than it too.
        if usize::try_from(text.len()).map_or(true, |units| units > MAX_MESSAGE) {
            tracing::debug!("chart: ignored an oversized page message");
            return;
        }
        match parse_message(&text.to_string()) {
            Some(ChartAction::OpenFeed(id)) => {
                self.open_feed(&QString::from(format!("feed:{id}").as_str()));
            }
            Some(ChartAction::OpenUrl(url)) => self.open_url(&QString::from(url.as_str())),
            Some(ChartAction::Prefs(prefs)) => self.prefs_reported(
                &QString::from(prefs.view.as_str()),
                &QString::from(prefs.ink.as_str()),
                prefs.animate,
            ),
            None => tracing::debug!("chart: ignored a page message"),
        }
    }

    fn show_file(mut self: Pin<&mut Self>, url: &QString) -> bool {
        let url = url.to_string();
        let read = local_path(&url)
            .ok_or_else(|| "not a file address".to_string())
            .and_then(|path| std::fs::read_to_string(path).map_err(|e| e.to_string()));
        let shown =
            read.and_then(|json| self.as_mut().rust_mut().get_mut().model.show_document(json));
        match shown {
            Ok(()) => {
                self.as_mut().show_current();
                self.state_changed();
                true
            }
            Err(error) => {
                tracing::warn!("chart: cannot show {url}: {}", redact_text(&error));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_message_over_the_cap_is_ignored_before_parsing() {
        let ok = r#"{"type":"openFeed","id":12}"#;
        assert_eq!(parse_message(ok), Some(ChartAction::OpenFeed(12)));
        let padded = format!("{ok}{}", " ".repeat(MAX_MESSAGE));
        assert!(padded.len() > MAX_MESSAGE);
        assert_eq!(parse_message(&padded), None);
        assert_eq!(parse_message("{"), None);
    }

    #[test]
    fn every_listed_file_has_bytes_and_nothing_else_does() {
        for f in ressac_ui::chart::FILES {
            assert!(
                file_bytes(f.path).is_some_and(|b| !b.is_empty()),
                "{}",
                f.path
            );
        }
        assert!(file_bytes("LICENSE-d3.txt").is_none());
        assert!(file_bytes("").is_none());
    }

    #[test]
    fn chart_files_are_served_byte_for_byte() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../macos/Ressac/Resources/Chart");
        for f in ressac_ui::chart::FILES {
            let (mut mime, mut body) = (String::new(), Vec::new());
            assert!(
                crate::reader::ressac_scheme_serve(
                    &format!("ressac:chart/{}", f.path),
                    &mut mime,
                    &mut body
                ),
                "{}",
                f.path
            );
            assert_eq!(mime, f.mime);
            assert_eq!(
                body,
                std::fs::read(dir.join(f.path)).unwrap(),
                "{} differs from the Mac file",
                f.path
            );
        }
        for refused in [
            "ressac:chart/LICENSE-d3.txt",
            "ressac:chart/../reader.css",
            "ressac:chart/",
            "ressac:chart/index.html?x=1",
        ] {
            assert!(
                !crate::reader::ressac_scheme_serve(refused, &mut String::new(), &mut Vec::new()),
                "{refused}"
            );
        }
    }
}
