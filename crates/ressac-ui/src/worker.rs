//! The job threads, without Qt (the terminal reader's `jobs.rs`, for the desktop app): one
//! short-run thread that runs jobs in submission order on its own `Ressac`, and one thread per
//! long run with its own connection and event forwarding. Callbacks run on these threads; the
//! KDE bridges hand them to the GUI thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ressac_core::Error;
use ressac_core::service::{Event, EventSink, Ressac};
use tokio::runtime::{Builder, Runtime};

/// Why a job did not produce its value.
#[derive(Debug)]
pub enum JobFailure {
    Core(Error),
    /// The job panicked; the worker carries on.
    Panicked,
}

impl JobFailure {
    /// Another connection holds the database (a `ressac` in a terminal).
    pub fn is_busy(&self) -> bool {
        matches!(self, JobFailure::Core(e) if e.is_lock_contention())
    }

    pub fn message(&self) -> String {
        match self {
            JobFailure::Core(e) => e.to_string(),
            JobFailure::Panicked => "The job stopped unexpectedly.".to_string(),
        }
    }
}

pub type JobResult<T> = Result<T, JobFailure>;

/// What a job runs with: its own `Ressac` (its own connection) and a runtime.
pub struct JobContext<'a> {
    pub app: &'a Ressac,
    runtime: &'a Runtime,
}

impl JobContext<'_> {
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }
}

fn runtime() -> ressac_core::Result<Runtime> {
    Ok(Builder::new_current_thread().enable_all().build()?)
}

fn outcome<T>(caught: std::thread::Result<ressac_core::Result<T>>) -> JobResult<T> {
    match caught {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(JobFailure::Core(error)),
        Err(_) => Err(JobFailure::Panicked),
    }
}

/// The short thread's state: the connection opened on the first job (and again after a
/// failed open) and the runtime.
struct ShortState {
    db: PathBuf,
    app: Option<Ressac>,
    runtime: Option<Runtime>,
}

type ShortJob = Box<dyn FnOnce(&mut ShortState) + Send>;

struct ForwardSink<F>(F);

impl<F: Fn(&Event) + Send + Sync> EventSink for ForwardSink<F> {
    fn emit(&self, event: &Event) {
        (self.0)(event);
    }
}

/// Starts a FIFO thread with its own state (connection, runtime).
fn spawn_lane(db: &Path) -> (mpsc::Sender<ShortJob>, JoinHandle<()>) {
    let (jobs_tx, jobs) = mpsc::channel::<ShortJob>();
    let mut state = ShortState {
        db: db.to_path_buf(),
        app: None,
        runtime: None,
    };
    let handle = thread::spawn(move || {
        for job in jobs {
            job(&mut state);
        }
    });
    (jobs_tx, handle)
}

/// Wraps a job and its callback for a lane: panics in either are caught (the lane carries on).
fn lane_job<T: Send + 'static>(
    job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
    done: impl FnOnce(JobResult<T>) + Send + 'static,
) -> ShortJob {
    Box::new(move |state| {
        let caught = catch_unwind(AssertUnwindSafe(|| {
            if state.runtime.is_none() {
                state.runtime = Some(runtime()?);
            }
            if state.app.is_none() {
                state.app = Some(Ressac::open(&state.db)?);
            }
            let (Some(app), Some(runtime)) = (&state.app, &state.runtime) else {
                unreachable!("both were just set");
            };
            job(&JobContext { app, runtime })
        }));
        // A failed open is tried again by the next job.
        finish(outcome(caught), done);
    })
}

/// Runs `done`, logging a panic instead of letting it end the thread.
fn finish<T>(result: JobResult<T>, done: impl FnOnce(JobResult<T>)) {
    if catch_unwind(AssertUnwindSafe(|| done(result))).is_err() {
        tracing::error!("a job's completion callback panicked");
    }
}

pub struct Worker {
    db: PathBuf,
    short: mpsc::Sender<ShortJob>,
    direct: mpsc::Sender<ShortJob>,
    /// The two lane threads, joined by `shutdown`.
    lanes: Vec<JoinHandle<()>>,
    /// The flag of the current (or last) long run; each long run gets its own.
    stop: Mutex<Arc<AtomicBool>>,
}

impl Worker {
    /// Starts the short-run and direct-write threads; each opens the database on its first job.
    pub fn new(db: &Path) -> Worker {
        let (short, short_thread) = spawn_lane(db);
        let (direct, direct_thread) = spawn_lane(db);
        Worker {
            db: db.to_path_buf(),
            short,
            direct,
            lanes: vec![short_thread, direct_thread],
            stop: Mutex::new(Arc::new(AtomicBool::new(false))),
        }
    }

    pub fn db(&self) -> &Path {
        &self.db
    }

    /// Queues a job on the short thread (one at a time, in submission order). `done` runs on
    /// that thread afterwards.
    pub fn short<T: Send + 'static>(
        &self,
        job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
        done: impl FnOnce(JobResult<T>) + Send + 'static,
    ) {
        // The thread ends only when the worker is gone, so a failed send has no one to tell.
        let _ = self.short.send(lane_job(job, done));
    }

    /// Queues a direct write (read, star, mark all, full-text preference) on its own thread
    /// and connection, ordered among direct writes only, so it never waits behind a short
    /// job such as a network extraction.
    pub fn direct<T: Send + 'static>(
        &self,
        job: impl FnOnce(&JobContext<'_>) -> ressac_core::Result<T> + Send + 'static,
        done: impl FnOnce(JobResult<T>) + Send + 'static,
    ) {
        let _ = self.direct.send(lane_job(job, done));
    }

    /// Runs a job on a new thread with its own connection and runtime. `events` receives the
    /// job's events on that thread, then `done` runs there too.
    pub fn long<T: Send + 'static>(
        &self,
        job: impl FnOnce(&JobContext<'_>, &dyn EventSink, Arc<AtomicBool>) -> ressac_core::Result<T>
        + Send
        + 'static,
        events: impl Fn(&Event) + Send + Sync + 'static,
        done: impl FnOnce(JobResult<T>) + Send + 'static,
    ) {
        let db = self.db.clone();
        let stop = Arc::new(AtomicBool::new(false));
        *self.stop.lock().unwrap_or_else(|e| e.into_inner()) = stop.clone();
        thread::spawn(move || {
            let caught = catch_unwind(AssertUnwindSafe(|| {
                let app = Ressac::open(&db)?;
                let runtime = runtime()?;
                let sink = ForwardSink(events);
                job(
                    &JobContext {
                        app: &app,
                        runtime: &runtime,
                    },
                    &sink,
                    stop,
                )
            }));
            finish(outcome(caught), done);
        });
    }

    /// Ends the worker before the process tears Qt down: stops the long run, closes the lanes
    /// (each finishes the job it is running) and waits up to `wait` in all for them. True when
    /// every lane ended; a lane still busy is logged and left to the process exit.
    pub fn shutdown(mut self, wait: Duration) -> bool {
        self.stop();
        // Senders whose receivers are gone: dropping the real ones ends the lanes' loops.
        self.short = mpsc::channel().0;
        self.direct = mpsc::channel().0;
        let deadline = Instant::now() + wait;
        let mut all = true;
        for lane in std::mem::take(&mut self.lanes) {
            while !lane.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if lane.is_finished() {
                let _ = lane.join();
            } else {
                tracing::warn!("a worker lane was still busy {wait:?} after the shutdown began");
                all = false;
            }
        }
        all
    }

    /// Asks the current long job to stop; it checks its flag between feeds. The next long run
    /// starts with a fresh flag.
    pub fn stop(&self) {
        self.stop
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store(true, Ordering::Relaxed);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::TestDb;
    use std::sync::mpsc;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn shutdown_joins_idle_lanes_and_reports_a_busy_one() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        assert!(worker.shutdown(WAIT));
        let worker = Worker::new(&db.path);
        worker.short(
            |_| {
                thread::sleep(Duration::from_millis(600));
                Ok(())
            },
            |_| {},
        );
        let started = Instant::now();
        assert!(!worker.shutdown(Duration::from_millis(50)));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    /// Mac RunCoordinatorTests.short_runs_run_one_at_a_time_in_order.
    #[test]
    fn short_jobs_run_one_at_a_time_in_order() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let log = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        for n in [3, 1, 2] {
            let (log, tx) = (log.clone(), tx.clone());
            worker.short(
                move |_| {
                    log.lock().unwrap().push(format!("start {n}"));
                    std::thread::sleep(Duration::from_millis(30));
                    log.lock().unwrap().push(format!("end {n}"));
                    Ok(n)
                },
                move |r| tx.send(r.unwrap()).unwrap(),
            );
        }
        let done: Vec<i32> = (0..3).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
        assert_eq!(done, [3, 1, 2]);
        assert_eq!(
            *log.lock().unwrap(),
            ["start 3", "end 3", "start 1", "end 1", "start 2", "end 2"]
        );
    }

    #[test]
    fn short_jobs_run_while_a_long_job_runs() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (release, wait) = mpsc::channel::<()>();
        let (tx, rx) = mpsc::channel();
        let long_tx = tx.clone();
        worker.long(
            move |_, _, _| {
                wait.recv_timeout(WAIT).unwrap();
                Ok("long")
            },
            |_| {},
            move |r| long_tx.send(r.unwrap()).unwrap(),
        );
        worker.short(
            |cx| cx.app.store().feed_count(),
            move |r| {
                tx.send(if r.unwrap() == 11 { "short" } else { "?" })
                    .unwrap()
            },
        );
        assert_eq!(
            rx.recv_timeout(WAIT).unwrap(),
            "short",
            "the long job is still waiting"
        );
        release.send(()).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "long");
    }

    #[test]
    fn a_long_job_s_events_arrive_before_its_end_and_stop_reaches_it() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        let event_tx = Mutex::new(tx.clone());
        let (asked, stopped) = mpsc::channel::<()>();
        worker.long(
            move |_, sink, stop| {
                sink.emit(&Event::Started { total: 2 });
                stopped.recv_timeout(WAIT).unwrap();
                Ok(stop.load(Ordering::Relaxed))
            },
            move |e| event_tx.lock().unwrap().send(format!("{e:?}")).unwrap(),
            move |r| tx.send(format!("done {}", r.unwrap())).unwrap(),
        );
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "Started { total: 2 }");
        worker.stop();
        asked.send(()).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "done true");
    }

    #[test]
    fn a_database_that_cannot_be_opened_fails_the_job_and_the_next_one_still_runs() {
        let worker = Worker::new(Path::new("/nonexistent/dir/r.sqlite"));
        let (tx, rx) = mpsc::channel();
        worker.short(
            |_| Ok(()),
            move |r| tx.send(r.map_err(|f| f.message())).unwrap(),
        );
        assert!(rx.recv_timeout(WAIT).unwrap().is_err());
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        worker.short(
            |_| -> ressac_core::Result<()> { panic!("boom") },
            move |r| tx.send(r.map_err(|f| f.message())).unwrap(),
        );
        worker.short(
            |_| Ok(()),
            move |r| tx2.send(r.map_err(|f| f.message())).unwrap(),
        );
        assert_eq!(
            rx.recv_timeout(WAIT).unwrap(),
            Err("The job stopped unexpectedly.".to_string())
        );
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Ok(()));
    }

    #[test]
    fn a_job_writes_on_its_own_connection_and_the_gui_connection_sees_it() {
        let db = TestDb::new();
        let id = db.entry(1, "2026-10-01T08:00:00.000Z", false);
        let gui = db.store();
        let before = gui.data_version().unwrap();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        worker.short(
            move |cx| cx.app.store().set_read(id, true, chrono::Utc::now()),
            move |r| tx.send(r.is_ok()).unwrap(),
        );
        assert!(rx.recv_timeout(WAIT).unwrap());
        assert_ne!(gui.data_version().unwrap(), before);
        assert!(db.read_at(id).is_some());
    }

    #[test]
    fn stop_then_a_second_long_run_sees_false() {
        let db = TestDb::new();
        let worker = Worker::new(&db.path);
        let (tx, rx) = mpsc::channel();
        let first = tx.clone();
        worker.long(
            |_, _, _| Ok(()),
            |_| {},
            move |r| first.send(r.is_ok()).unwrap(),
        );
        assert!(rx.recv_timeout(WAIT).unwrap());
        worker.stop();
        worker.long(
            |_, _, stop| Ok(stop.load(Ordering::Relaxed)),
            |_| {},
            move |r| tx.send(r.unwrap()).unwrap(),
        );
        assert!(
            !rx.recv_timeout(WAIT).unwrap(),
            "the new run starts with a fresh flag"
        );
    }

    #[test]
    fn a_blocked_short_job_does_not_delay_a_direct_write() {
        let db = TestDb::new();
        let id = db.entry(1, "2026-10-01T08:00:00.000Z", false);
        let worker = Worker::new(&db.path);
        let (release, wait) = mpsc::channel::<()>();
        let (tx, rx) = mpsc::channel();
        let short_tx = tx.clone();
        worker.short(
            move |_| {
                wait.recv_timeout(WAIT).unwrap();
                Ok("short")
            },
            move |r| short_tx.send(r.unwrap()).unwrap(),
        );
        worker.direct(
            move |cx| {
                cx.app
                    .store()
                    .set_read(id, true, chrono::Utc::now())
                    .map(|()| "direct")
            },
            move |r| tx.send(r.unwrap()).unwrap(),
        );
        assert_eq!(
            rx.recv_timeout(WAIT).unwrap(),
            "direct",
            "the short job is still blocked"
        );
        assert!(db.read_at(id).is_some());
        release.send(()).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), "short");
    }
}
