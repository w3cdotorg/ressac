//! Background work for the reader: each job opens its own connection on the same database,
//! runs on a current-thread runtime in its own thread, and reports through a channel.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

use ressac_core::service::{EditionOutcome, Event as CoreEvent, EventSink, ExtractOutcome, Ressac};

use super::state::{Event, JobOutcome};

/// What a job sends back to the loop. Images (phase 6) add their own variant.
pub enum Msg {
    Event(Event),
}

/// Turns the core's run events into `Event::Progress` for the top line.
pub struct ProgressSink {
    label: String,
    done: AtomicUsize,
    total: AtomicUsize,
    tx: Sender<Msg>,
}

impl ProgressSink {
    pub fn new(label: &str, tx: Sender<Msg>) -> Self {
        Self {
            label: label.to_owned(),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            tx,
        }
    }

    fn send(&self) {
        let _ = self.tx.send(Msg::Event(Event::Progress {
            label: self.label.clone(),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }));
    }
}

impl EventSink for ProgressSink {
    fn emit(&self, event: &CoreEvent) {
        match event {
            CoreEvent::Started { total } => {
                self.total.store(*total, Ordering::Relaxed);
                self.send();
            }
            CoreEvent::FeedDone { .. } => {
                self.done.fetch_add(1, Ordering::Relaxed);
                self.send();
            }
            _ => {}
        }
    }
}

/// Runs `work` in its own thread with its own `Ressac` on `db`. The outcome becomes
/// `Event::JobDone`; a database that cannot be opened is a `Failed` outcome.
pub fn spawn<F, Fut>(db: PathBuf, tx: Sender<Msg>, stop: Arc<AtomicBool>, work: F)
where
    F: FnOnce(Ressac, Sender<Msg>, Arc<AtomicBool>) -> Fut + Send + 'static,
    Fut: Future<Output = JobOutcome>,
{
    std::thread::spawn(move || {
        let outcome = match (crate::commands::open(&db), tokio_runtime()) {
            (Ok(app), Ok(runtime)) => runtime.block_on(work(app, tx.clone(), stop)),
            (Err(e), _) => JobOutcome::Failed(format!("{e:#}")),
            (_, Err(e)) => JobOutcome::Failed(e.to_string()),
        };
        let _ = tx.send(Msg::Event(Event::JobDone(outcome)));
    });
}

fn tokio_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

pub fn edition_outcome(result: ressac_core::Result<EditionOutcome>) -> JobOutcome {
    match result {
        Ok(EditionOutcome::Built(_)) => JobOutcome::EditionBuilt,
        Ok(EditionOutcome::NotDue) => JobOutcome::EditionNotDue,
        Ok(EditionOutcome::Offline) => JobOutcome::Offline,
        Ok(EditionOutcome::Interrupted) => JobOutcome::Failed("Interrupted.".into()),
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub fn extract_outcome(entry_id: i64, result: ressac_core::Result<ExtractOutcome>) -> JobOutcome {
    let error = match result {
        Ok(ExtractOutcome::Saved) => None,
        Ok(ExtractOutcome::Failed(sentence)) => Some(sentence),
        Ok(ExtractOutcome::NoUrl) => Some("This entry has no link.".into()),
        Ok(ExtractOutcome::UnknownEntry) => Some(format!("No entry #{entry_id}.")),
        Err(e) => Some(e.to_string()),
    };
    JobOutcome::Extracted { entry_id, error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn the_progress_sink_turns_core_events_into_progress() {
        let (tx, rx) = mpsc::channel();
        let sink = ProgressSink::new("Fetching feeds", tx);
        let done = |feed_id| CoreEvent::FeedDone {
            feed_id,
            title: "a".into(),
            error: None,
        };
        sink.emit(&CoreEvent::Started { total: 3 });
        sink.emit(&done(1));
        sink.emit(&done(2));
        let got: Vec<(usize, usize)> = rx
            .try_iter()
            .filter_map(|m| match m {
                Msg::Event(Event::Progress { label, done, total }) if label == "Fetching feeds" => {
                    Some((done, total))
                }
                _ => None,
            })
            .collect();
        assert_eq!(got, vec![(0, 3), (1, 3), (2, 3)]);
    }

    #[test]
    fn a_job_runs_on_its_own_connection_and_reports_its_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("r.sqlite");
        Ressac::open(&db).unwrap();
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        spawn(db.clone(), tx, stop, |app, _tx, _stop| async move {
            assert_eq!(app.store().feed_count().unwrap(), 0);
            JobOutcome::Refreshed
        });
        let msg = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the job reports");
        assert!(matches!(
            msg,
            Msg::Event(Event::JobDone(JobOutcome::Refreshed))
        ));
    }

    #[test]
    fn a_job_that_cannot_open_the_database_reports_a_failure() {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        spawn(
            "/nonexistent/dir/r.sqlite".into(),
            tx,
            stop,
            |_app, _tx, _stop| async move { JobOutcome::Refreshed },
        );
        let msg = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the job reports");
        assert!(matches!(
            msg,
            Msg::Event(Event::JobDone(JobOutcome::Failed(_)))
        ));
    }

    #[test]
    fn edition_outcomes_map_to_job_outcomes() {
        assert_eq!(
            edition_outcome(Ok(EditionOutcome::NotDue)),
            JobOutcome::EditionNotDue
        );
        assert_eq!(
            edition_outcome(Ok(EditionOutcome::Offline)),
            JobOutcome::Offline
        );
        assert_eq!(
            edition_outcome(Ok(EditionOutcome::Interrupted)),
            JobOutcome::Failed("Interrupted.".into())
        );
        assert!(matches!(
            edition_outcome(Err(ressac_core::Error::Invalid("boom".into()))),
            JobOutcome::Failed(m) if m.contains("boom")
        ));
    }

    #[test]
    fn extract_outcomes_map_to_job_outcomes() {
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::Saved)),
            JobOutcome::Extracted {
                entry_id: 7,
                error: None
            }
        );
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::Failed("HTTP 404.".into()))),
            JobOutcome::Extracted {
                entry_id: 7,
                error: Some("HTTP 404.".into())
            }
        );
        assert_eq!(
            extract_outcome(7, Ok(ExtractOutcome::NoUrl)),
            JobOutcome::Extracted {
                entry_id: 7,
                error: Some("This entry has no link.".into())
            }
        );
    }
}
