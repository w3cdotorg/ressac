//! The `Runs` and `Live` QML singletons: the one long run at a time with its progress and
//! banner, and the rules that tell the window when the data changed. The rules are
//! `ressac_ui`'s (`runs`, `live`); these objects translate them to Qt and bring the worker's
//! callbacks back to the GUI thread.

use std::cell::RefCell;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use ressac_core::clock::today_start;
use ressac_core::logging::redact_text;
use ressac_core::paths::Paths;
use ressac_core::service::{Event, EventSink};
use ressac_ui::chrono;
use ressac_ui::live::{Invalidation, POLL_INTERVAL, should_poll};
use ressac_ui::runs::{
    Admission, Banner, RunCoordinator, RunEnd, RunFailure, RunRequest, compact_progress_text,
    fetch_options, progress_text, progress_width,
};
use ressac_ui::sources::Selection;
use ressac_ui::worker::{JobContext, JobResult};

use crate::launcher::with_session;

/// Width of the pill without progress (`runs::progress_width` of a fetch).
const IDLE_PROGRESS_WIDTH: i32 = 140;

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
        #[qproperty(bool, long_run_active, READ = long_run_active, NOTIFY = state_changed)]
        #[qproperty(bool, can_refresh, READ = can_refresh, NOTIFY = state_changed)]
        #[qproperty(bool, progress_visible, READ = progress_visible, NOTIFY = state_changed)]
        #[qproperty(QString, progress_text, READ = progress_text, NOTIFY = state_changed)]
        #[qproperty(QString, progress_compact_text, READ = progress_compact_text, NOTIFY = state_changed)]
        #[qproperty(f64, progress_fraction, READ = progress_fraction, NOTIFY = state_changed)]
        #[qproperty(i32, progress_width, READ = progress_width, NOTIFY = state_changed)]
        #[qproperty(bool, banner_visible, READ = banner_visible, NOTIFY = state_changed)]
        #[qproperty(QString, banner_message, READ = banner_message, NOTIFY = state_changed)]
        #[qproperty(bool, banner_show_log, READ = banner_show_log, NOTIFY = state_changed)]
        #[qproperty(QString, log_dir, READ = log_dir, CONSTANT)]
        type Runs = super::RunsRust;

        fn long_run_active(self: &Runs) -> bool;
        fn can_refresh(self: &Runs) -> bool;
        fn progress_visible(self: &Runs) -> bool;
        fn progress_text(self: &Runs) -> QString;
        fn progress_compact_text(self: &Runs) -> QString;
        fn progress_fraction(self: &Runs) -> f64;
        fn progress_width(self: &Runs) -> i32;
        fn banner_visible(self: &Runs) -> bool;
        fn banner_message(self: &Runs) -> QString;
        fn banner_show_log(self: &Runs) -> bool;
        fn log_dir(self: &Runs) -> QString;

        /// The toolbar's Refresh with nothing selected: every feed.
        #[qinvokable]
        fn refresh_all(self: Pin<&mut Runs>);

        /// Refresh for the selection `key` (a feed, a folder, anything else: every feed).
        #[qinvokable]
        fn refresh_source(self: Pin<&mut Runs>, key: &QString);

        #[qinvokable]
        fn refresh_feed(self: Pin<&mut Runs>, feed_id: i64);

        #[qinvokable]
        fn show_banner(self: Pin<&mut Runs>, message: &QString, show_log: bool);

        #[qinvokable]
        fn dismiss_banner(self: Pin<&mut Runs>);

        /// The run's state (progress, banner, activity) changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Runs>);

        /// A run changed the database: reload (progress at most once a second, every run end).
        #[qsignal]
        fn database_changed(self: Pin<&mut Runs>);
    }

    impl cxx_qt::Threading for Runs {}
    impl cxx_qt::Initialize for Runs {}

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, window_active, READ, WRITE = set_window_active, NOTIFY)]
        #[qproperty(bool, chart_on_screen, READ, WRITE = set_chart_on_screen, NOTIFY)]
        #[qproperty(bool, polling, READ = polling, NOTIFY = polling_changed)]
        #[qproperty(i32, poll_interval, READ = poll_interval, CONSTANT)]
        type Live = super::LiveRust;

        fn polling(self: &Live) -> bool;
        fn poll_interval(self: &Live) -> i32;

        fn set_window_active(self: Pin<&mut Live>, active: bool);

        fn set_chart_on_screen(self: Pin<&mut Live>, on_screen: bool);

        /// The 3 s timer: has another connection committed?
        #[qinvokable]
        fn poll(self: Pin<&mut Live>);

        /// The 60 s timer: did the day change?
        #[qinvokable]
        fn minute_tick(self: Pin<&mut Live>);

        /// The data may have changed (another commit, a new day, the window came back).
        #[qsignal]
        fn invalidated(self: Pin<&mut Live>);

        #[qsignal]
        fn polling_changed(self: Pin<&mut Live>);
    }
}

pub struct RunsRust {
    coordinator: RunCoordinator,
    invalidation: Invalidation,
    /// The long run in flight: its events and its end are told in terms of it.
    current: Option<RunRequest>,
    log_dir: String,
}

impl Default for RunsRust {
    fn default() -> Self {
        RunsRust {
            coordinator: RunCoordinator::default(),
            invalidation: Invalidation::default(),
            current: None,
            log_dir: Paths::from_env()
                .map(|paths| paths.log_dir().to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

impl qobject::Runs {
    fn long_run_active(&self) -> bool {
        self.rust().coordinator.long_run_active()
    }

    fn can_refresh(&self) -> bool {
        self.rust().coordinator.can_refresh()
    }

    fn progress_visible(&self) -> bool {
        self.rust().coordinator.progress().is_some()
    }

    fn progress_text(&self) -> QString {
        let text = self.rust().coordinator.progress().map(progress_text);
        QString::from(text.unwrap_or_default().as_str())
    }

    fn progress_compact_text(&self) -> QString {
        let text = self
            .rust()
            .coordinator
            .progress()
            .map(compact_progress_text);
        QString::from(text.unwrap_or_default().as_str())
    }

    /// −1: indeterminate (or no progress).
    fn progress_fraction(&self) -> f64 {
        self.rust()
            .coordinator
            .progress()
            .and_then(|progress| progress.fraction())
            .unwrap_or(-1.0)
    }

    fn progress_width(&self) -> i32 {
        self.rust()
            .coordinator
            .progress()
            .map_or(IDLE_PROGRESS_WIDTH, |progress| {
                i32::try_from(progress_width(progress)).unwrap_or(IDLE_PROGRESS_WIDTH)
            })
    }

    fn banner_visible(&self) -> bool {
        self.rust().coordinator.banner().is_some()
    }

    fn banner_message(&self) -> QString {
        let banner = self.rust().coordinator.banner();
        QString::from(banner.map_or("", |banner| banner.message.as_str()))
    }

    fn banner_show_log(&self) -> bool {
        self.rust()
            .coordinator
            .banner()
            .is_some_and(|banner| banner.show_log)
    }

    fn log_dir(&self) -> QString {
        QString::from(self.rust().log_dir.as_str())
    }

    fn refresh_all(self: Pin<&mut Self>) {
        self.fetch(RunRequest::FetchAll);
    }

    fn refresh_source(self: Pin<&mut Self>, key: &QString) {
        let Some(selection) = Selection::parse_key(&key.to_string()) else {
            return;
        };
        self.fetch(RunRequest::refresh(&selection));
    }

    fn refresh_feed(self: Pin<&mut Self>, feed_id: i64) {
        self.fetch(RunRequest::FetchFeeds(vec![feed_id]));
    }

    fn show_banner(mut self: Pin<&mut Self>, message: &QString, show_log: bool) {
        let banner = Banner {
            message: message.to_string(),
            show_log,
        };
        self.as_mut()
            .rust_mut()
            .get_mut()
            .coordinator
            .show_banner(banner);
        self.state_changed();
    }

    fn dismiss_banner(mut self: Pin<&mut Self>) {
        self.as_mut()
            .rust_mut()
            .get_mut()
            .coordinator
            .dismiss_banner();
        self.state_changed();
    }

    /// A refresh: the fetch job as a long run nobody waits for.
    fn fetch(self: Pin<&mut Self>, request: RunRequest) {
        let job_request = request.clone();
        self.start_long(
            request,
            move |cx, sink, stop| {
                let options = fetch_options(cx.app.store(), &job_request)?;
                cx.block_on(cx.app.refresh(options, sink, stop)).map(drop)
            },
            |_: &()| None,
            |_| {},
            |_| {},
        );
    }

    /// Admits a long run (a second one is refused, not queued) and hands it to the worker.
    /// `events` sees every event on the worker's thread; `end` runs on the GUI thread once the
    /// run is concluded.
    fn start_long<T: Send + 'static>(
        mut self: Pin<&mut Self>,
        request: RunRequest,
        job: impl FnOnce(&JobContext<'_>, &dyn EventSink, Arc<AtomicBool>) -> ressac_core::Result<T>
        + Send
        + 'static,
        failure_of: impl FnOnce(&T) -> Option<RunFailure> + Send + 'static,
        events: impl Fn(&Event) + Send + Sync + 'static,
        end: impl FnOnce(RunEnd<T>) + Send + 'static,
    ) {
        let for_events = self.qt_thread();
        let for_done = self.qt_thread();
        let done_request = request.clone();
        // Taken by the worker's callback when the run starts; what is left was never started.
        let mut end = Some(end);
        // Admission and the hand-off to the worker are one step: an admitted run always has a
        // worker, so the coordinator is never left holding a run nobody ends.
        let started = with_session(|session| {
            let rust = self.as_mut().rust_mut().get_mut();
            if rust.coordinator.admit(&request) == Admission::Refused {
                return false;
            }
            rust.current = Some(request);
            session.worker.long(
                job,
                move |event| {
                    let queued = event.clone();
                    if for_events
                        .queue(move |runs| runs.run_event(&queued))
                        .is_err()
                    {
                        tracing::debug!("the app quit during a run");
                    }
                    events(event);
                },
                {
                    let end = end.take();
                    move |result| {
                        if for_done
                            .queue(move |runs| {
                                runs.long_finished(&done_request, result, failure_of, end);
                            })
                            .is_err()
                        {
                            tracing::debug!("the app quit before a run finished");
                        }
                    }
                },
            );
            true
        });
        if started == Some(true) {
            self.state_changed();
        } else if let Some(end) = end {
            // No session (nothing ran, nothing to show) or a long run already active.
            end(RunEnd::Refused);
        }
    }

    /// One event of the long run, on the GUI thread.
    fn run_event(mut self: Pin<&mut Self>, event: &Event) {
        let Some(request) = self.rust().current.clone() else {
            return;
        };
        let rust = self.as_mut().rust_mut().get_mut();
        if rust.coordinator.event(&request, event) && rust.invalidation.run_progress(Instant::now())
        {
            self.as_mut().database_changed();
        }
        self.state_changed();
    }

    /// The long run ended, on the GUI thread.
    fn long_finished<T>(
        mut self: Pin<&mut Self>,
        request: &RunRequest,
        result: JobResult<T>,
        failure_of: impl FnOnce(&T) -> Option<RunFailure>,
        end: Option<impl FnOnce(RunEnd<T>)>,
    ) {
        self.as_mut().rust_mut().get_mut().current = None;
        let run_end = self.as_mut().conclude(request, result, failure_of);
        if let Some(end) = end {
            end(run_end);
        }
    }

    /// Queues a short run on the worker (they overlap a long run and one another's order is
    /// kept); `end` runs on the GUI thread once the run is concluded.
    fn start_short<T: Send + 'static>(
        mut self: Pin<&mut Self>,
        request: RunRequest,
        job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
        end: impl FnOnce(RunEnd<T>) + Send + 'static,
    ) {
        if with_session(|_| ()).is_none() {
            end(RunEnd::Refused);
            return;
        }
        // A short run is always admitted: the call keeps the coordinator's one door.
        self.as_mut()
            .rust_mut()
            .get_mut()
            .coordinator
            .admit(&request);
        let for_done = self.qt_thread();
        with_session(|session| {
            session.worker.short(job, move |result| {
                if for_done
                    .queue(move |runs| runs.short_finished(request, result, end))
                    .is_err()
                {
                    tracing::debug!("the app quit before a run finished");
                }
            });
        });
    }

    /// A short run ended, on the GUI thread.
    fn short_finished<T>(
        mut self: Pin<&mut Self>,
        request: RunRequest,
        result: JobResult<T>,
        end: impl FnOnce(RunEnd<T>),
    ) {
        let run_end = self.as_mut().conclude(&request, result, |_| None);
        end(run_end);
    }

    /// The coordinator's verdict on a finished run, logged (the request's redacted label, the
    /// redacted message), then the reload and the state signals.
    fn conclude<T>(
        mut self: Pin<&mut Self>,
        request: &RunRequest,
        result: JobResult<T>,
        failure_of: impl FnOnce(&T) -> Option<RunFailure>,
    ) -> RunEnd<T> {
        let rust = self.as_mut().rust_mut().get_mut();
        let run_end = rust.coordinator.conclude(request, result, failure_of);
        match &run_end {
            RunEnd::Failed(failure) if failure.refusal => tracing::info!(
                "{} refused: {}",
                request.log_label(),
                redact_text(&failure.message)
            ),
            RunEnd::Failed(failure) => tracing::warn!(
                "{} failed: {}",
                request.log_label(),
                redact_text(&failure.message)
            ),
            RunEnd::Done(_) | RunEnd::Refused => {}
        }
        let reload = rust.invalidation.run_finished(Instant::now());
        if reload {
            self.as_mut().database_changed();
        }
        self.state_changed();
        run_end
    }
}

thread_local! {
    // The GUI thread's handle on `Runs`, registered when QML first touches the singleton:
    // the one way in for every bridge that starts a run.
    static RUNS: RefCell<Option<CxxQtThread<qobject::Runs>>> = const { RefCell::new(None) };
}

impl cxx_qt::Initialize for qobject::Runs {
    fn initialize(self: Pin<&mut Self>) {
        let handle = self.qt_thread();
        RUNS.with(|slot| *slot.borrow_mut() = Some(handle));
    }
}

/// Queues `start` onto `Runs`; false when there is no handle (the singleton was never created)
/// or the queue refused (the app is quitting).
fn queue_on_runs(start: impl FnOnce(Pin<&mut qobject::Runs>) + Send + 'static) -> bool {
    RUNS.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|runs| runs.queue(start).is_ok())
    })
}

/// Starts a long run through the one coordinator: one at a time (a second is turned away and
/// `end` gets `Refused`), its progress and banner shared. `job` runs on its own thread and
/// connection; `failure_of` reads a value as a failure (an offline edition); `events` sees
/// every event on the job's thread; `end` runs on the GUI thread.
pub(crate) fn submit_long<T: Send + 'static>(
    request: RunRequest,
    job: impl FnOnce(&JobContext<'_>, &dyn EventSink, Arc<AtomicBool>) -> ressac_core::Result<T>
    + Send
    + 'static,
    failure_of: impl FnOnce(&T) -> Option<RunFailure> + Send + 'static,
    events: impl Fn(&Event) + Send + Sync + 'static,
    end: impl FnOnce(RunEnd<T>) + Send + 'static,
) {
    // The end is shared between the queued start and the failure path of this function.
    let end = Arc::new(Mutex::new(Some(end)));
    let queued = end.clone();
    let label = request.log_label();
    let sent = queue_on_runs(move |runs| {
        if let Some(end) = take(&queued) {
            runs.start_long(request, job, failure_of, events, end);
        }
    });
    if !sent {
        tracing::error!("cannot start {label}: no way into the run coordinator");
        if let Some(end) = take(&end) {
            end(RunEnd::Refused);
        }
    }
}

/// Queues a short run (any number may overlap a long one, in order) through the coordinator;
/// `end` runs on the GUI thread.
pub(crate) fn submit_short<T: Send + 'static>(
    request: RunRequest,
    job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
    end: impl FnOnce(RunEnd<T>) + Send + 'static,
) {
    let end = Arc::new(Mutex::new(Some(end)));
    let queued = end.clone();
    let label = request.log_label();
    let sent = queue_on_runs(move |runs| {
        if let Some(end) = take(&queued) {
            runs.start_short(request, job, end);
        }
    });
    if !sent {
        tracing::error!("cannot start {label}: no way into the run coordinator");
        if let Some(end) = take(&end) {
            end(RunEnd::Refused);
        }
    }
}

/// The value out of a shared one-shot slot.
fn take<F>(slot: &Mutex<Option<F>>) -> Option<F> {
    slot.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[derive(Default)]
pub struct LiveRust {
    invalidation: Invalidation,
    window_active: bool,
    chart_on_screen: bool,
}

impl qobject::Live {
    fn polling(&self) -> bool {
        should_poll(self.rust().window_active, self.rust().chart_on_screen)
    }

    fn poll_interval(&self) -> i32 {
        i32::try_from(POLL_INTERVAL.as_millis()).unwrap_or(3000)
    }

    fn set_window_active(mut self: Pin<&mut Self>, active: bool) {
        let before = self.as_ref().polling();
        let was_active = self.rust().window_active;
        if was_active == active {
            return;
        }
        self.as_mut().rust_mut().get_mut().window_active = active;
        self.as_mut().window_active_changed();
        if active && !was_active {
            // Back on screen: reload once now, from a fresh baseline.
            match with_session(|session| session.store.data_version()) {
                Some(Ok(version)) => {
                    self.as_mut()
                        .rust_mut()
                        .get_mut()
                        .invalidation
                        .activated(version);
                }
                Some(Err(error)) => tracing::warn!("cannot read data_version: {error}"),
                None => {}
            }
            self.as_mut().invalidated();
        }
        self.polling_may_have_changed(before);
    }

    fn set_chart_on_screen(mut self: Pin<&mut Self>, on_screen: bool) {
        if self.rust().chart_on_screen == on_screen {
            return;
        }
        let before = self.as_ref().polling();
        self.as_mut().rust_mut().get_mut().chart_on_screen = on_screen;
        self.as_mut().chart_on_screen_changed();
        self.polling_may_have_changed(before);
    }

    fn polling_may_have_changed(self: Pin<&mut Self>, before: bool) {
        if self.as_ref().polling() != before {
            self.polling_changed();
        }
    }

    fn poll(mut self: Pin<&mut Self>) {
        if !self.as_ref().polling() {
            return;
        }
        match with_session(|session| session.store.data_version()) {
            Some(Ok(version)) => {
                if self
                    .as_mut()
                    .rust_mut()
                    .get_mut()
                    .invalidation
                    .version_seen(version)
                {
                    self.invalidated();
                }
            }
            // Transient (a writer holds the lock); the next tick tries again.
            Some(Err(error)) => tracing::warn!("cannot read data_version: {error}"),
            None => {}
        }
    }

    fn minute_tick(mut self: Pin<&mut Self>) {
        let today = today_start(chrono::Local::now());
        if self
            .as_mut()
            .rust_mut()
            .get_mut()
            .invalidation
            .day_seen(today)
        {
            self.invalidated();
        }
    }
}
