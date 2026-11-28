//! The Sea Chart window's rules (the Mac's `ChartModel` and `ChartWebView`): the refresh
//! schedule as a state machine over `Instant`s, the version check, the in-process survey,
//! the page bridge's message validation, the prefs argument and save rule, the navigation
//! rule, the page's document queue and the files the `ressac:chart/` scheme serves. No Qt:
//! the KDE app only translates.

use std::path::Path;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ressac_core::model::FeedId;
use ressac_core::store::{ChartEdit, ChartEditDone, MAX_EDIT, Store};
use serde_json::Value;

use crate::article::js_string;
use crate::runs::RunEnd;
use crate::sanitize::is_web_url;
use crate::settings::{ChartInk, ChartPrefs, ChartView, Settings, SettingsStore};

pub const PREFIX: &str = "ressac:chart/";
pub const PAGE_URL: &str = "ressac:chart/index.html";
/// The only document version this build draws.
pub const SUPPORTED_VERSION: u64 = 1;
/// Changes within one window share a single survey, run at its end.
pub const DEBOUNCE: Duration = Duration::from_secs(2);
/// Ages and the cartouche's time move with the clock: survey again this often while open.
pub const RESURVEY: Duration = Duration::from_secs(300);
/// How often the window's timer calls [`ChartModel::tick`].
pub const TICK_MS: i32 = 250;
pub const WINDOW_TITLE: &str = "Sea Chart";
pub const NOT_OPEN: &str = "The database is not open. The main window says why.";
pub const NOT_DRAWN: &str = "The chart could not be drawn";
pub const RETRY: &str = "Retry";
pub const UNREADABLE: &str = "ressac printed a chart this app cannot read.";

/// The message for a document of another version.
pub fn unsupported(version: u64) -> String {
    format!(
        "This build draws chart version {SUPPORTED_VERSION}, but ressac produced version {version}."
    )
}

/// A file served under [`PREFIX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartFile {
    pub path: &'static str,
    pub mime: &'static str,
}

/// The page, its scripts, style and fonts, as in `apps/macos/Ressac/Resources/Chart`.
pub const FILES: [ChartFile; 14] = [
    ChartFile {
        path: "index.html",
        mime: "text/html; charset=utf-8",
    },
    ChartFile {
        path: "chart.css",
        mime: "text/css",
    },
    ChartFile {
        path: "d3.min.js",
        mime: "text/javascript",
    },
    ChartFile {
        path: "chart-core.js",
        mime: "text/javascript",
    },
    ChartFile {
        path: "chart-survey.js",
        mime: "text/javascript",
    },
    ChartFile {
        path: "chart-draw.js",
        mime: "text/javascript",
    },
    ChartFile {
        path: "fonts/CormorantGaramond-Medium.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/CormorantGaramond-SemiBold.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/CormorantGaramond-Italic.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/CormorantGaramond-MediumItalic.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/CormorantSC-Medium.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/CormorantSC-SemiBold.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/IBMPlexMono-Regular.woff2",
        mime: "font/woff2",
    },
    ChartFile {
        path: "fonts/IBMPlexMono-Medium.woff2",
        mime: "font/woff2",
    },
];

/// The file named exactly `path` (the part after [`PREFIX`]): no query, no other spelling.
pub fn file(path: &str) -> Option<&'static ChartFile> {
    FILES.iter().find(|file| file.path == path)
}

pub const ADAPTER_SCRIPT: &str = r#"(function () {
  "use strict";
  var pending = [], target = null;
  function send(message) {
    var text = JSON.stringify(message);
    if (target) target.post(text); else pending.push(text);
  }
  var ressac = Object.freeze({postMessage: send});
  Object.defineProperty(window, "webkit", {value: Object.freeze({messageHandlers: Object.freeze({ressac: ressac})})});
  // Qt WebEngine shows a cursor bitmap one pixel per logical pixel: the page draws its quill for that (quillCursor)
  document.documentElement.classList.add("qt-cursors");
  new QWebChannel(qt.webChannelTransport, function (channel) {
    target = channel.objects.ressac;
    pending.splice(0).forEach(function (text) { target.post(text); });
  });
})();
"#;

pub const PROBE_SCRIPT: &str = r#"(function () {
  const c = document.querySelector("canvas");
  let colours = 0, print = "";
  if (c && c.width && c.height) {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    const seen = new Set(); let p = 0;
    for (let i = 0; i < d.length; i += 4 * 61) { const v = (d[i] << 16) | (d[i + 1] << 8) | d[i + 2]; seen.add(v); p = (p * 31 + v) >>> 0; }
    colours = seen.size; print = String(p);
  }
  const theme = [...document.querySelectorAll(".bar button")].find(b => b.textContent === "By theme");
  const faces = [...document.fonts].filter(f => f.status === "loaded").length;
  return "colours=" + colours + " print=" + print + " faces=" + faces + " ink=" + (document.documentElement.dataset.ink || "")
    + " byTheme=" + (theme ? theme.getAttribute("aria-pressed") : "") + " home=" + typeof Chart + " " + location.href;
})()"#;

/// Checks a survey's output: a JSON object with the supported `version`.
pub fn check_document(json: &str) -> Result<(), String> {
    let version = serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|value| value.get("version")?.as_u64())
        .ok_or_else(|| UNREADABLE.to_string())?;
    if version == SUPPORTED_VERSION {
        Ok(())
    } else {
        Err(unsupported(version))
    }
}

/// The chart as `ressac chart --json` prints it, read with its own query-only connection
/// (it never creates a database).
pub fn survey(db: &Path, now: DateTime<Utc>) -> Result<String, String> {
    let store = Store::open_existing_query_only(db).map_err(|e| e.to_string())?;
    let snapshot = store.chart_snapshot(now).map_err(|e| e.to_string())?;
    serde_json::to_string(&snapshot).map_err(|e| e.to_string())
}

/// What the window shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Loading,
    Ready(String),
    Failed(String),
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Loading => "loading",
            Phase::Ready(_) => "ready",
            Phase::Failed(_) => "failed",
        }
    }
}

/// Whether two survey documents are the same chart apart from `surveyed_at` (the survey's own
/// clock, which differs on every survey). Anything that does not parse is not the same.
fn same_chart(a: &str, b: &str) -> bool {
    let strip = |text: &str| {
        let mut value: Value = serde_json::from_str(text).ok()?;
        value.as_object_mut()?.remove("surveyed_at");
        Some(value)
    };
    matches!((strip(a), strip(b)), (Some(a), Some(b)) if a == b)
}

/// A running survey's ticket: a result for any other ticket is stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Survey(pub u64);

/// When the chart is surveyed: on open, once per debounce window after a change, every
/// resurvey while open, and once more after a request made during a survey.
#[derive(Debug, Clone)]
pub struct ChartModel {
    phase: Phase,
    last_error: Option<String>,
    open: bool,
    running: Option<Survey>,
    next_ticket: u64,
    rerun: bool,
    scheduled: Option<Instant>,
    next_survey: Option<Instant>,
    debounce: Duration,
    resurvey: Duration,
}

impl Default for ChartModel {
    fn default() -> Self {
        ChartModel::with_timing(DEBOUNCE, RESURVEY)
    }
}

impl ChartModel {
    pub fn with_timing(debounce: Duration, resurvey: Duration) -> ChartModel {
        ChartModel {
            phase: Phase::Loading,
            last_error: None,
            open: false,
            running: None,
            next_ticket: 0,
            rerun: false,
            scheduled: None,
            next_survey: None,
            debounce,
            resurvey,
        }
    }

    /// The window opens: surveys now and again every resurvey. Already open: nothing.
    pub fn open(&mut self, now: Instant) -> Option<Survey> {
        if self.open {
            return None;
        }
        self.open = true;
        self.next_survey = Some(now + self.resurvey);
        self.refresh()
    }

    /// The window closes: the running survey, the schedule and the rerun are forgotten, and
    /// a late result never reaches the closed chart.
    pub fn close(&mut self) {
        // The ticket counter keeps counting: a survey from before can never match a later one.
        *self = ChartModel {
            next_ticket: self.next_ticket,
            ..ChartModel::with_timing(self.debounce, self.resurvey)
        };
    }

    /// Surveys now (open, Retry). Asked during a survey, it reruns once after that one.
    pub fn refresh(&mut self) -> Option<Survey> {
        if !self.open {
            return None;
        }
        if self.running.is_some() {
            self.rerun = true;
            return None;
        }
        self.next_ticket += 1;
        let survey = Survey(self.next_ticket);
        self.running = Some(survey);
        Some(survey)
    }

    /// The app's own edit was just committed: the running survey (it may have read the chart
    /// before the commit, and would show it after the page's reply), the scheduled one and the
    /// rerun are dropped, and one fresh survey starts now. A late result of the dropped survey
    /// is stale: [`ChartModel::finished`] ignores it.
    pub fn refresh_after_edit(&mut self) -> Option<Survey> {
        self.running = None;
        self.scheduled = None;
        self.rerun = false;
        self.refresh()
    }

    /// The database changed. Changes within one debounce window share a single survey, run
    /// at the end of the window: a steady trickle of writes still draws once per window.
    pub fn note_change(&mut self, now: Instant) {
        if self.open && self.scheduled.is_none() {
            self.scheduled = Some(now + self.debounce);
        }
    }

    /// The clock moved: a survey when the debounce window ended or the resurvey is due.
    pub fn tick(&mut self, now: Instant) -> Option<Survey> {
        if !self.open {
            return None;
        }
        let mut due = false;
        if self.scheduled.is_some_and(|at| at <= now) {
            self.scheduled = None;
            due = true;
        }
        if self.next_survey.is_some_and(|at| at <= now) {
            self.next_survey = Some(now + self.resurvey);
            due = true;
        }
        if due { self.refresh() } else { None }
    }

    /// A survey ended. A stale one is ignored; a failed refresh keeps the last chart; one that
    /// differs from the shown chart only by its clock keeps the shown document (the page gets no
    /// `Chart.update`). Returns the rerun asked for during this survey, if any.
    pub fn finished(&mut self, survey: Survey, result: Result<String, String>) -> Option<Survey> {
        if self.running != Some(survey) {
            return None;
        }
        self.running = None;
        match result.and_then(|json| check_document(&json).map(|()| json)) {
            Ok(json) => {
                self.last_error = None;
                if !matches!(&self.phase, Phase::Ready(shown) if same_chart(shown, &json)) {
                    self.phase = Phase::Ready(json);
                }
            }
            Err(message) => {
                self.last_error = Some(message.clone());
                if !matches!(self.phase, Phase::Ready(_)) {
                    self.phase = Phase::Failed(message);
                }
            }
        }
        if std::mem::take(&mut self.rerun) {
            self.refresh()
        } else {
            None
        }
    }

    /// Recette: a document handed in as a successful survey would be, while open.
    pub fn show_document(&mut self, json: String) -> Result<(), String> {
        if !self.open {
            return Err(NOT_OPEN.to_string());
        }
        check_document(&json)?;
        self.last_error = None;
        self.phase = Phase::Ready(json);
        Ok(())
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// Why the last survey failed, whatever the phase; `None` after a good one.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

/// What a page message asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartAction {
    OpenFeed(FeedId),
    OpenUrl(String),
    Prefs(ChartPrefs),
    Edit { seq: i64, edit: String },
    Survey,
}

/// A whole JSON number as an `i64` (`7` and `7.0`), nothing else.
fn feed_id(value: &Value) -> Option<FeedId> {
    let number = value.as_number()?;
    if let Some(id) = number.as_i64() {
        return Some(id);
    }
    let float = number.as_f64()?;
    // 2^63 is the first f64 above i64::MAX.
    (float.fract() == 0.0
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&float))
    .then_some(float as i64)
}

/// A page message (the adapter's JSON text): only the three known types with well-formed
/// fields; anything else is ignored.
pub fn handle(message: &str) -> Option<ChartAction> {
    let value: Value = serde_json::from_str(message).ok()?;
    match value.get("type")?.as_str()? {
        "openFeed" => feed_id(value.get("id")?).map(ChartAction::OpenFeed),
        "openURL" => {
            let url = value.get("url")?.as_str()?;
            is_web_url(url).then(|| ChartAction::OpenUrl(url.to_string()))
        }
        "prefs" => Some(ChartAction::Prefs(ChartPrefs {
            view: ChartView::parse(value.get("view")?.as_str()?)?,
            ink: ChartInk::parse(value.get("ink")?.as_str()?)?,
            animate: value.get("animate")?.as_bool()?,
        })),
        "edit" => {
            let seq = feed_id(value.get("seq")?)?;
            let edit = value.get("edit")?.as_str()?;
            // The page counts from 1; the cap is in UTF-8 bytes; the text must be an edit.
            (seq >= 1 && edit.len() <= MAX_EDIT && serde_json::from_str::<ChartEdit>(edit).is_ok())
                .then(|| ChartAction::Edit {
                    seq,
                    edit: edit.to_string(),
                })
        }
        "survey" => Some(ChartAction::Survey),
        _ => None,
    }
}

/// The prefs argument of `Chart.load`: still under Reduce Motion, whatever is stored; the
/// page also learns Reduce Motion itself.
pub fn page_prefs(prefs: &ChartPrefs, reduce_motion: bool) -> String {
    format!(
        r#"{{"view":"{}","ink":"{}","animate":{},"reduceMotion":{}}}"#,
        prefs.view.as_str(),
        prefs.ink.as_str(),
        prefs.animate && !reduce_motion,
        reduce_motion
    )
}

/// Stores what the page chose: view and ink always, animate not under Reduce Motion (the
/// page was told to stand still, which is not a choice).
pub fn save_prefs(
    settings: &mut Settings,
    store: &mut dyn SettingsStore,
    prefs: &ChartPrefs,
    reduce_motion: bool,
) {
    settings.set_chart_view(store, prefs.view.as_str());
    settings.set_chart_ink(store, prefs.ink.as_str());
    if !reduce_motion {
        settings.set_chart_animate(store, prefs.animate);
    }
}

/// Only the chart page, in the main frame, is navigable.
pub fn allows_navigation(url: &str, main_frame: bool) -> bool {
    main_frame && url == PAGE_URL
}

/// The page's reply to an edit (`Chart.edited`'s argument): `done` carries the core's
/// `chart_edit` event; a refusal keeps the core's sentence; a turned-away run is a failure: it
/// is turned away when there is no session, the database is not open.
pub fn edit_result(seq: i64, end: &RunEnd<ChartEditDone>) -> String {
    let text = |t: &str| serde_json::to_string(t).unwrap_or_else(|_| "\"\"".into());
    match end {
        RunEnd::Done(done) => format!(r#"{{"seq":{seq},"ok":true,"done":{}}}"#, done.event()),
        RunEnd::Failed(failure) => format!(
            r#"{{"seq":{seq},"ok":false,"refused":{},"error":{}}}"#,
            failure.refusal,
            text(&failure.message)
        ),
        RunEnd::Refused => format!(
            r#"{{"seq":{seq},"ok":false,"refused":false,"error":{}}}"#,
            text(NOT_OPEN)
        ),
    }
}

/// The script that hands a result to the page: the text is one JavaScript string literal.
pub fn edited_script(result: &str) -> String {
    format!("Chart.edited({})", js_string(result))
}

pub fn load_script(json: &str, prefs: &str) -> String {
    format!("Chart.load({}, {prefs})", js_string(json))
}

pub fn update_script(json: &str) -> String {
    format!("Chart.update({})", js_string(json))
}

/// What the view does after a load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageStep {
    Run(String),
    GoHome,
    Nothing,
}

/// The page's document queue: the latest document waits until the page has loaded, goes in
/// with `Chart.load` first and `Chart.update` after, and is never sent twice.
#[derive(Debug, Clone, Default)]
pub struct ChartPage {
    latest: Option<String>,
    shown: Option<String>,
    loaded: bool,
}

impl ChartPage {
    /// The script to run for `json`, when the page is loaded and does not show it yet.
    pub fn show(&mut self, json: &str, prefs: &str) -> Option<String> {
        self.latest = Some(json.to_string());
        if !self.loaded || self.shown.as_deref() == Some(json) {
            return None;
        }
        let script = if self.shown.is_none() {
            load_script(json, prefs)
        } else {
            update_script(json)
        };
        self.shown = Some(json.to_string());
        Some(script)
    }

    /// A load began: the page starts empty.
    pub fn load_started(&mut self) {
        self.loaded = false;
        self.shown = None;
    }

    /// A load ended. The chart page loaded: draw the waiting document. Any other page loaded
    /// (the view was taken elsewhere): go home. A failed load changes nothing.
    pub fn load_finished(&mut self, url: &str, succeeded: bool, prefs: &str) -> PageStep {
        if !succeeded {
            return PageStep::Nothing;
        }
        if url != PAGE_URL {
            self.loaded = false;
            self.shown = None;
            return PageStep::GoHome;
        }
        self.loaded = true;
        self.shown = None;
        match self.latest.clone() {
            Some(json) => {
                let script = load_script(&json, prefs);
                self.shown = Some(json);
                PageStep::Run(script)
            }
            None => PageStep::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::runs::{BUSY, RunFailure};
    use crate::settings::{ChartInk, ChartPrefs, ChartView, Settings};
    use crate::test_db::{TestDb, at};

    fn doc(version: u64) -> String {
        format!(r#"{{"version":{version},"feeds":[]}}"#)
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    // ChartModelTests
    #[test]
    fn decodes_version_1() {
        let mut model = ChartModel::default();
        assert_eq!(model.phase(), &Phase::Loading);
        let s = model.open(Instant::now()).unwrap();
        assert_eq!(model.finished(s, Ok(doc(1))), None);
        assert_eq!(model.phase(), &Phase::Ready(doc(1)));
    }

    #[test]
    fn refuses_other_versions_and_unreadable_output() {
        let mut model = ChartModel::default();
        let s = model.open(Instant::now()).unwrap();
        model.finished(s, Ok(doc(2)));
        assert_eq!(
            model.phase(),
            &Phase::Failed(
                "This build draws chart version 1, but ressac produced version 2.".into()
            )
        );
        let s = model.refresh().unwrap();
        model.finished(s, Ok("nope".into()));
        assert_eq!(
            model.phase(),
            &Phase::Failed("ressac printed a chart this app cannot read.".into())
        );
        assert_eq!(
            check_document(r#"{"feeds":[]}"#),
            Err(UNREADABLE.to_string())
        );
    }

    #[test]
    fn a_failed_survey_shows_its_message() {
        let mut model = ChartModel::default();
        let s = model.open(Instant::now()).unwrap();
        model.finished(s, Err("database is locked".into()));
        assert_eq!(model.phase(), &Phase::Failed("database is locked".into()));
        assert_eq!(model.last_error(), Some("database is locked"));
    }

    #[test]
    fn debounces_bursts() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(300), RESURVEY);
        let s = model.open(t0).unwrap();
        model.finished(s, Ok(doc(1)));
        for i in 0..5 {
            model.note_change(t0 + ms(20 * i));
        }
        assert_eq!(model.tick(t0 + ms(299)), None);
        let s = model.tick(t0 + ms(300)).expect("one survey for the burst");
        model.finished(s, Ok(doc(1)));
        assert_eq!(model.tick(t0 + ms(800)), None, "the burst is spent");
        model.note_change(t0 + ms(900)); // a later change starts a new window
        assert_eq!(model.tick(t0 + ms(1199)), None);
        assert!(model.tick(t0 + ms(1200)).is_some());
    }

    #[test]
    fn a_change_during_a_survey_is_not_lost() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(20), RESURVEY);
        let first = model.open(t0).unwrap();
        model.note_change(t0);
        assert_eq!(
            model.tick(t0 + ms(20)),
            None,
            "a survey is running: it reruns after"
        );
        let second = model.finished(first, Ok(doc(1))).expect("the rerun");
        assert_ne!(second, first);
        assert_eq!(model.finished(second, Ok(doc(1))), None, "only once");
    }

    /// Final review I-2: a survey that began before the edit's commit never shows after it,
    /// and an edit is one survey, not two or three.
    #[test]
    fn a_survey_from_before_an_edit_never_publishes_after_it() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(20), RESURVEY);
        let s = model.open(t0).unwrap();
        model.finished(s, Ok(doc(1)));
        let stale = model.refresh().expect("a survey begins before the commit");
        model.note_change(t0 + ms(5)); // and a notice schedules another
        let fresh = model
            .refresh_after_edit()
            .expect("the edit surveys afresh at once");
        assert_ne!(fresh, stale);
        let before = r#"{"version":1,"feeds":[],"seas":"before"}"#;
        let after = r#"{"version":1,"feeds":[],"seas":"after"}"#;
        assert_eq!(model.finished(stale, Ok(before.into())), None, "stale");
        assert_eq!(model.phase(), &Phase::Ready(doc(1)), "never shown");
        assert_eq!(model.finished(fresh, Ok(after.into())), None, "no rerun");
        assert_eq!(model.phase(), &Phase::Ready(after.into()));
        assert_eq!(model.tick(t0 + ms(100)), None, "the scheduled one went");
        model.close();
        assert_eq!(model.refresh_after_edit(), None, "closed: nothing");
    }

    /// A survey that differs only by its clock (`surveyed_at`) keeps the shown document, so the
    /// page gets no `Chart.update` for it (moved from the KDE glue, T15 Minor 3).
    #[test]
    fn a_survey_that_differs_only_by_its_clock_keeps_the_shown_document() {
        let a = r#"{"version":1,"surveyed_at":"2026-11-27T10:00:00Z","seas":[{"id":1}]}"#;
        let b = r#"{"seas":[{"id":1}],"version":1,"surveyed_at":"2026-11-27T10:05:00Z"}"#;
        let c = r#"{"version":1,"surveyed_at":"2026-11-27T10:05:00Z","seas":[{"id":2}]}"#;
        let mut model = ChartModel::default();
        let s = model.open(Instant::now()).unwrap();
        model.finished(s, Ok(a.into()));
        let s = model.refresh().unwrap();
        model.finished(s, Ok(b.into()));
        assert_eq!(
            model.phase(),
            &Phase::Ready(a.into()),
            "the shown one stays"
        );
        let s = model.refresh().unwrap();
        model.finished(s, Ok(c.into()));
        assert_eq!(
            model.phase(),
            &Phase::Ready(c.into()),
            "a real change shows"
        );
        assert!(!same_chart(a, "{"), "what does not parse is not the same");
    }

    #[test]
    fn failed_refresh_keeps_last_chart() {
        let mut model = ChartModel::default();
        let s = model.open(Instant::now()).unwrap();
        model.finished(s, Ok(doc(1)));
        let s = model.refresh().unwrap();
        model.finished(s, Err("boom".into()));
        assert_eq!(
            (model.phase(), model.last_error()),
            (&Phase::Ready(doc(1)), Some("boom"))
        );
        let s = model.refresh().unwrap();
        model.finished(s, Ok(doc(1)));
        assert_eq!(model.last_error(), None);
    }

    #[test]
    fn following_draws_on_open_then_on_later_changes() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(50), RESURVEY);
        let s = model.open(t0).expect("drawn on open");
        model.finished(s, Ok(doc(1)));
        assert_eq!(model.open(t0 + ms(10)), None, "already open");
        assert_eq!(model.tick(t0 + ms(300)), None, "no change, no survey");
        model.note_change(t0 + ms(300));
        assert!(model.tick(t0 + ms(350)).is_some());
    }

    #[test]
    fn closing_drops_a_running_survey_and_the_schedule() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(150), ms(1000));
        let s = model.open(t0).unwrap();
        model.note_change(t0 + ms(10));
        model.close();
        assert_eq!(model.finished(s, Ok(doc(1))), None);
        assert_eq!(
            (model.phase(), model.is_open()),
            (&Phase::Loading, false),
            "the result never reaches the closed chart"
        );
        model.note_change(t0 + ms(200));
        assert_eq!(model.tick(t0 + ms(5000)), None);
        assert_eq!(model.refresh(), None);
    }

    #[test]
    fn following_resurveys_periodically_until_closed() {
        let t0 = Instant::now();
        let mut model = ChartModel::with_timing(ms(50), ms(100));
        let s = model.open(t0).unwrap();
        model.finished(s, Ok(doc(1)));
        let s = model.tick(t0 + ms(100)).expect("ages move with the clock");
        model.finished(s, Ok(doc(1)));
        assert_eq!(model.tick(t0 + ms(150)), None);
        assert!(model.tick(t0 + ms(200)).is_some());
        model.close();
        assert_eq!(model.tick(t0 + ms(1000)), None);
        assert_eq!(RESURVEY, Duration::from_secs(300));
        assert_eq!(DEBOUNCE, Duration::from_secs(2));
    }

    #[test]
    fn a_document_file_is_shown_like_a_survey() {
        let mut model = ChartModel::default();
        assert_eq!(
            model.show_document(doc(1)),
            Err(NOT_OPEN.to_string()),
            "closed: nothing to show it in"
        );
        let s = model.open(Instant::now()).unwrap();
        model.finished(s, Err("boom".into()));
        assert_eq!(model.show_document(doc(2)), Err(unsupported(2)));
        assert_eq!(model.show_document(doc(1)), Ok(()));
        assert_eq!(
            (model.phase(), model.last_error()),
            (&Phase::Ready(doc(1)), None)
        );
    }

    #[test]
    fn survey_reads_the_store_like_the_cli() {
        let db = TestDb::new();
        let now = at("2026-10-12T09:00:00.000Z");
        let json = survey(&db.path, now).unwrap();
        assert_eq!(
            json,
            serde_json::to_string(&db.store().chart_snapshot(now).unwrap()).unwrap()
        );
        assert_eq!(check_document(&json), Ok(()));
        let missing = db.path.with_file_name("missing.sqlite");
        assert!(survey(&missing, now).is_err());
        assert!(!missing.exists(), "a survey never creates a database");
    }

    // ChartBridgeTests
    #[test]
    fn bridge_accepts_known_messages() {
        assert_eq!(
            handle(r#"{"type":"openFeed","id":42}"#),
            Some(ChartAction::OpenFeed(42))
        );
        assert_eq!(
            handle(r#"{"type":"openFeed","id":7.0}"#),
            Some(ChartAction::OpenFeed(7))
        );
        assert_eq!(
            handle(r#"{"type":"openURL","url":"https://example.org/a?b=1"}"#),
            Some(ChartAction::OpenUrl("https://example.org/a?b=1".into()))
        );
        assert_eq!(
            handle(r#"{"type":"openURL","url":"http://example.org/"}"#),
            Some(ChartAction::OpenUrl("http://example.org/".into()))
        );
        assert_eq!(
            handle(r#"{"type":"prefs","view":"themes","ink":"nuit","animate":false}"#),
            Some(ChartAction::Prefs(ChartPrefs {
                view: ChartView::Themes,
                ink: ChartInk::Nuit,
                animate: false
            }))
        );
    }

    #[test]
    fn bridge_refuses_file_and_javascript_urls() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<p>x</p>",
            "ftp://example.org/",
            "ressac-toggle:1",
            "http://",
            "https:no-host",
            "not a url",
            "",
        ] {
            assert_eq!(
                handle(&serde_json::json!({"type": "openURL", "url": url}).to_string()),
                None,
                "{url}"
            );
        }
        assert_eq!(handle(r#"{"type":"openURL","url":1}"#), None);
        assert_eq!(handle(r#"{"type":"openURL"}"#), None);
    }

    #[test]
    fn bridge_ignores_unknown_types() {
        for message in [
            r#"{"type":"navigate","url":"https://example.org/"}"#,
            r#"{"type":"OPENFEED","id":1}"#,
            r#"{"id":1}"#,
            r#""openFeed""#,
            r#"["openFeed",1]"#,
            "null",
            "not json",
            "",
            "undefined",
            r#"{"type":"openFeed","id":"42"}"#,
            r#"{"type":"openFeed","id":1.5}"#,
            r#"{"type":"openFeed","id":true}"#,
            r#"{"type":"openFeed","id":1e300}"#,
            r#"{"type":"prefs","view":"sideways","ink":"bleu","animate":true}"#,
            r#"{"type":"prefs","view":"harbour","ink":"green","animate":true}"#,
            r#"{"type":"prefs","view":"harbour","ink":"bleu","animate":1}"#,
            r#"{"type":"prefs","view":"harbour","ink":"bleu"}"#,
        ] {
            assert_eq!(handle(message), None, "{message}");
        }
    }

    #[test]
    fn prefs_persist() {
        let mut store: BTreeMap<String, String> = BTreeMap::new();
        let mut settings = Settings::load(&store);
        assert_eq!(
            settings.chart,
            ChartPrefs {
                view: ChartView::Harbour,
                ink: ChartInk::Bleu,
                animate: true
            }
        );
        save_prefs(
            &mut settings,
            &mut store,
            &ChartPrefs {
                view: ChartView::Themes,
                ink: ChartInk::Nuit,
                animate: false,
            },
            false,
        );
        assert_eq!(
            (
                store["chart.view"].as_str(),
                store["chart.ink"].as_str(),
                store["chart.animate"].as_str()
            ),
            ("themes", "nuit", "false")
        );
        assert_eq!(
            Settings::load(&store).chart,
            ChartPrefs {
                view: ChartView::Themes,
                ink: ChartInk::Nuit,
                animate: false
            }
        );
        store.insert("chart.view".into(), "sideways".into());
        assert_eq!(
            Settings::load(&store).chart,
            ChartPrefs {
                view: ChartView::Harbour,
                ink: ChartInk::Nuit,
                animate: false
            }
        );
    }

    #[test]
    fn reduce_motion_starts_still_and_keeps_the_stored_choice() {
        let prefs = ChartPrefs {
            view: ChartView::Themes,
            ink: ChartInk::Nuit,
            animate: true,
        };
        assert_eq!(
            page_prefs(&prefs, false),
            r#"{"view":"themes","ink":"nuit","animate":true,"reduceMotion":false}"#
        );
        assert_eq!(
            page_prefs(&prefs, true),
            r#"{"view":"themes","ink":"nuit","animate":false,"reduceMotion":true}"#
        );
        let mut store: BTreeMap<String, String> = BTreeMap::new();
        let mut settings = Settings::load(&store);
        save_prefs(&mut settings, &mut store, &prefs, false);
        save_prefs(
            &mut settings,
            &mut store,
            &ChartPrefs {
                view: ChartView::Harbour,
                ink: ChartInk::Nuit,
                animate: false,
            },
            true,
        );
        assert_eq!(
            Settings::load(&store).chart,
            ChartPrefs {
                view: ChartView::Harbour,
                ink: ChartInk::Nuit,
                animate: true
            }
        );
    }

    #[test]
    fn a_retired_or_unknown_stored_ink_reads_back_as_blue() {
        assert_eq!(ChartInk::ALL, [ChartInk::Bleu, ChartInk::Nuit]);
        for stored in ["pop", "pirate", "rouge", ""] {
            let store = BTreeMap::from([
                ("chart.ink".to_string(), stored.to_string()),
                ("chart.view".to_string(), "themes".to_string()),
            ]);
            assert_eq!(
                Settings::load(&store).chart,
                ChartPrefs {
                    view: ChartView::Themes,
                    ink: ChartInk::Bleu,
                    animate: true
                },
                "{stored}"
            );
            let message = serde_json::json!({"type": "prefs", "view": "themes", "ink": stored, "animate": true}).to_string();
            assert_eq!(handle(&message), None, "{stored}");
        }
    }

    #[test]
    fn only_the_chart_page_is_navigable() {
        assert!(allows_navigation(PAGE_URL, true));
        assert!(!allows_navigation(PAGE_URL, false));
        for url in [
            "https://example.org/",
            "http://example.org/index.html",
            "ressac:chart/chart.css",
            "ressac:chart/LICENSE-d3.txt",
            "file:///etc/passwd",
            "about:blank",
            "data:text/html,x",
            "ressac:chart/index.html?x=1",
            "qrc:/chart/index.html",
            "ressac:page/1-1",
            "",
        ] {
            assert!(!allows_navigation(url, true), "{url}");
        }
    }

    #[test]
    fn documents_wait_for_the_page_then_load_then_update() {
        let prefs = r#"{"view":"harbour","ink":"bleu","animate":true}"#;
        let mut page = ChartPage::default();
        assert_eq!(page.show(&doc(1), prefs), None, "the page has not loaded");
        page.load_started();
        assert_eq!(
            page.load_finished(PAGE_URL, true, prefs),
            PageStep::Run(load_script(&doc(1), prefs))
        );
        assert_eq!(
            page.show(&doc(1), prefs),
            None,
            "the same document is not sent twice"
        );
        let next = r#"{"version":1,"feeds":[{"id":1}]}"#;
        assert_eq!(page.show(next, prefs), Some(update_script(next)));
        page.load_started(); // a reload: the page starts empty
        let still = r#"{"view":"harbour","ink":"bleu","animate":false}"#;
        assert_eq!(
            page.load_finished(PAGE_URL, true, still),
            PageStep::Run(load_script(next, still))
        );
        assert_eq!(
            page.load_finished("about:blank", true, prefs),
            PageStep::GoHome
        );
        assert_eq!(
            page.load_finished("ressac:chart/chart.css", false, prefs),
            PageStep::Nothing
        );
        assert_eq!(
            ChartPage::default().load_finished(PAGE_URL, true, prefs),
            PageStep::Nothing,
            "nothing to draw yet"
        );
    }

    const JOIN: &str = r#"{"op":"join","feed":3,"land":{"id":1,"name":"Trio"}}"#;

    fn edit_message(seq: serde_json::Value, edit: serde_json::Value) -> String {
        serde_json::json!({"type": "edit", "seq": seq, "edit": edit}).to_string()
    }

    fn done() -> ChartEditDone {
        ChartEditDone {
            label: "Join Trio".into(),
            undo: ChartEdit::Patch {
                label: "Leave Trio".into(),
                changes: vec![],
            },
        }
    }

    #[test]
    fn bridge_accepts_an_edit() {
        assert_eq!(
            handle(&edit_message(7.into(), JOIN.into())),
            Some(ChartAction::Edit {
                seq: 7,
                edit: JOIN.into()
            })
        );
        assert_eq!(
            handle(r#"{"type":"edit","seq":2.0,"edit":"{\"op\":\"new_sea\",\"name\":\"Kites\"}"}"#),
            Some(ChartAction::Edit {
                seq: 2,
                edit: r#"{"op":"new_sea","name":"Kites"}"#.into()
            })
        );
        assert_eq!(handle(r#"{"type":"survey"}"#), Some(ChartAction::Survey));
        // The cap is in bytes and inclusive.
        let name = "a".repeat(MAX_EDIT - r#"{"op":"new_sea","name":""}"#.len());
        let at_cap = format!(r#"{{"op":"new_sea","name":"{name}"}}"#);
        assert_eq!(at_cap.len(), MAX_EDIT);
        assert!(handle(&edit_message(1.into(), at_cap.clone().into())).is_some());
    }

    #[test]
    fn bridge_refuses_malformed_edits() {
        for seq in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("1"),
            serde_json::json!(true),
            serde_json::json!(null),
        ] {
            assert_eq!(
                handle(&edit_message(seq.clone(), JOIN.into())),
                None,
                "{seq}"
            );
        }
        assert_eq!(handle(r#"{"type":"edit","edit":"{}"}"#), None, "no seq");
        assert_eq!(handle(r#"{"type":"edit","seq":1}"#), None, "no edit");
        for edit in [
            serde_json::json!(5),
            serde_json::json!({"op": "join"}),
            serde_json::json!("not json"),
            serde_json::json!("[]"),
            serde_json::json!(r#"{"feed":3}"#),
            serde_json::json!(r#"{"op":"drop_tables"}"#),
            serde_json::json!(r#"{"op":"join","feed":"x","land":{"id":1,"name":"T"}}"#),
        ] {
            assert_eq!(
                handle(&edit_message(1.into(), edit.clone())),
                None,
                "{edit}"
            );
        }
        // One byte over the cap, and multibyte text counted in bytes, not characters.
        let name = "a".repeat(MAX_EDIT - r#"{"op":"new_sea","name":""}"#.len() + 1);
        let over = format!(r#"{{"op":"new_sea","name":"{name}"}}"#);
        assert_eq!(handle(&edit_message(1.into(), over.into())), None);
        let name = "é".repeat(MAX_EDIT / 2);
        let wide = format!(r#"{{"op":"new_sea","name":"{name}"}}"#);
        assert!(wide.chars().count() < MAX_EDIT && wide.len() > MAX_EDIT);
        assert_eq!(handle(&edit_message(1.into(), wide.into())), None);
    }

    #[test]
    fn edit_results_are_the_page_format() {
        let ok: Value = serde_json::from_str(&edit_result(4, &RunEnd::Done(done()))).unwrap();
        assert_eq!(ok["seq"], 4);
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["done"]["event"], "chart_edit");
        assert_eq!(ok["done"]["label"], "Join Trio");
        assert_eq!(ok["done"]["undo"]["op"], "patch");
        assert!(ok.get("error").is_none());

        let refused = RunEnd::<ChartEditDone>::Failed(RunFailure {
            message: "The chart changed; nothing was done.".into(),
            refusal: true,
        });
        assert_eq!(
            edit_result(5, &refused),
            r#"{"seq":5,"ok":false,"refused":true,"error":"The chart changed; nothing was done."}"#
        );
        let failed = RunEnd::<ChartEditDone>::Failed(RunFailure {
            message: BUSY.into(),
            refusal: false,
        });
        assert_eq!(
            edit_result(6, &failed),
            format!(r#"{{"seq":6,"ok":false,"refused":false,"error":"{BUSY}"}}"#)
        );
        // Turned away: there is no session (final review Minor 7: not "busy").
        assert_eq!(
            edit_result(8, &RunEnd::<ChartEditDone>::Refused),
            format!(r#"{{"seq":8,"ok":false,"refused":false,"error":"{NOT_OPEN}"}}"#)
        );
    }

    #[test]
    fn edited_script_never_splices() {
        let hostile = "A \"q\" \\ </script><script>alert(1)</script> \u{2028}\u{2029}\n'";
        let end = RunEnd::<ChartEditDone>::Failed(RunFailure {
            message: hostile.into(),
            refusal: true,
        });
        let result = edit_result(1, &end);
        let script = edited_script(&result);
        assert_eq!(script, format!("Chart.edited({})", js_string(&result)));
        assert!(!script.contains("</script>"));
        assert!(!script.contains('\u{2028}') && !script.contains('\u{2029}'));
        assert!(!script.contains('\n'));
        // A hostile label in a done event too.
        let mut d = done();
        d.label = hostile.into();
        let script = edited_script(&edit_result(2, &RunEnd::Done(d)));
        assert!(!script.contains("</script>"));
        assert!(!script.contains('\u{2028}') && !script.contains('\u{2029}'));
        // The literal reads back as the result text.
        let literal = script
            .strip_prefix("Chart.edited(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap();
        let back: String = serde_json::from_str(literal).unwrap();
        let parsed: Value = serde_json::from_str(&back).unwrap();
        assert_eq!(parsed["done"]["label"], hostile);
    }

    #[test]
    fn page_prefs_carry_reduce_motion() {
        let prefs = ChartPrefs {
            view: ChartView::Harbour,
            ink: ChartInk::Bleu,
            animate: true,
        };
        let on: Value = serde_json::from_str(&page_prefs(&prefs, true)).unwrap();
        assert_eq!(on["reduceMotion"], true);
        assert_eq!(on["animate"], false);
        let off: Value = serde_json::from_str(&page_prefs(&prefs, false)).unwrap();
        assert_eq!(off["reduceMotion"], false);
        assert_eq!(off["animate"], true);
    }

    #[test]
    fn scripts_never_splice_the_document() {
        let hostile =
            r#"{"version":1,"feeds":[{"title":"</script><script>alert(1)</script> \"x"}]}"#;
        let prefs = r#"{"view":"harbour","ink":"bleu","animate":true}"#;
        assert_eq!(
            load_script(hostile, prefs),
            format!("Chart.load({}, {prefs})", js_string(hostile))
        );
        assert_eq!(
            update_script(hostile),
            format!("Chart.update({})", js_string(hostile))
        );
        assert!(!load_script(hostile, prefs).contains("</script>"));
    }

    #[test]
    fn the_chart_files_are_the_mac_files() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/macos/Ressac/Resources/Chart");
        for f in FILES {
            assert!(dir.join(f.path).is_file(), "{}", f.path);
            assert_eq!(file(f.path), Some(&f));
        }
        let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
        for referenced in [
            "chart.css",
            "d3.min.js",
            "chart-core.js",
            "chart-survey.js",
            "chart-draw.js",
        ] {
            assert!(
                html.contains(&format!("\"{referenced}\"")) && file(referenced).is_some(),
                "{referenced}"
            );
        }
        let woff2 = std::fs::read_dir(dir.join("fonts"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "woff2"))
            .count();
        assert_eq!(
            FILES.iter().filter(|f| f.mime == "font/woff2").count(),
            woff2
        );
        for refused in [
            "LICENSE-d3.txt",
            "fonts/IBMPlexMono-OFL.txt",
            "../reader.css",
            "/index.html",
            "index.html?x",
            "",
        ] {
            assert_eq!(file(refused), None, "{refused}");
        }
        assert_eq!(
            unsupported(2),
            "This build draws chart version 1, but ressac produced version 2."
        );
        assert_eq!(
            (NOT_OPEN, NOT_DRAWN, RETRY, WINDOW_TITLE),
            (
                "The database is not open. The main window says why.",
                "The chart could not be drawn",
                "Retry",
                "Sea Chart"
            )
        );
    }

    #[test]
    fn the_adapter_defines_the_webkit_handler_over_the_channel() {
        for part in [
            "window, \"webkit\"",
            "messageHandlers",
            "ressac: ressac",
            "new QWebChannel(qt.webChannelTransport",
            "channel.objects.ressac",
            "JSON.stringify(message)",
            "classList.add(\"qt-cursors\")",
        ] {
            assert!(ADAPTER_SCRIPT.contains(part), "{part}");
        }
    }
}
