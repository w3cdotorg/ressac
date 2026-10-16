//! `ressac read`: the reader's event loop. The state decides, this module executes.

pub mod html;
pub mod jobs;
pub mod layout;
pub mod native;
pub mod settings;
pub mod sidebar;
pub mod state;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyEventKind};
use ressac_core::service::{EditionOptions, NullSink, RefreshOptions, Ressac};
use ressac_core::store::{EntryPage, ListCursor, ListFilter, PAGE_SIZE, Source, Store};

use jobs::{Msg, ProgressSink};
use native::{EDITION_PROGRESS_LABEL, EXTRACT_PROGRESS_LABEL, FETCH_PROGRESS_LABEL};
use settings::TerminalSettings;
use state::{Command, Event, JobOutcome, ReaderState};

const IDLE_POLL: Duration = Duration::from_secs(2);
const INPUT_WAIT: Duration = Duration::from_millis(100);
const TICK: Duration = Duration::from_secs(1);

/// Local midnight of the day `now` belongs to, in UTC (the app's "Today").
pub fn today_start<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Utc> {
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).expect("00:00 exists");
    let zone = now.timezone();
    zone.from_local_datetime(&midnight)
        .earliest()
        // A zone that skips midnight (DST at 00:00): the day starts at the first valid hour.
        .or_else(|| {
            zone.from_local_datetime(&(midnight + chrono::Duration::hours(1)))
                .earliest()
        })
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc))
}

/// The two clocks a command needs: `now` for writes, `local` for "Today".
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub now: DateTime<Utc>,
    pub local: DateTime<FixedOffset>,
}

impl Clock {
    fn system() -> Self {
        let local = Local::now().fixed_offset();
        Self {
            now: local.with_timezone(&Utc),
            local,
        }
    }

    fn tick(self) -> Event {
        Event::Tick {
            now: self.now,
            local: self.local,
        }
    }
}

/// `PRAGMA data_version` polling: after two seconds without a key, a moved version is a reload.
pub struct Poll {
    last_key: Instant,
    last_check: Instant,
    version: i64,
}

impl Poll {
    pub fn new(now: Instant, version: i64) -> Self {
        Self {
            last_key: now,
            last_check: now,
            version,
        }
    }

    pub fn keyed(&mut self, now: Instant) {
        self.last_key = now;
    }

    /// Whether a check is worth its query: idle for two seconds, and two since the last check.
    pub fn due(&self, now: Instant) -> bool {
        now.duration_since(self.last_key) >= IDLE_POLL
            && now.duration_since(self.last_check) >= IDLE_POLL
    }

    /// A version that could not be read: try again at the next due time.
    pub fn failed(&mut self, now: Instant) {
        self.last_check = now;
    }

    pub fn check(&mut self, now: Instant, version: i64) -> Option<Event> {
        if !self.due(now) {
            return None;
        }
        self.last_check = now;
        if version == self.version {
            return None;
        }
        self.version = version;
        Some(Event::DataChanged)
    }
}

/// Runs the state's commands: quick reads and writes on the loop's connection, long work in jobs.
pub struct Executor<'a> {
    app: &'a Ressac,
    db: PathBuf,
    tx: Sender<Msg>,
    stop: Arc<AtomicBool>,
    long_running: bool,
    queue: VecDeque<Command>,
}

impl<'a> Executor<'a> {
    pub fn new(app: &'a Ressac, db: PathBuf, tx: Sender<Msg>) -> Self {
        Self {
            app,
            db,
            tx,
            stop: Arc::new(AtomicBool::new(false)),
            long_running: false,
            queue: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    #[cfg(test)]
    pub fn long_job_running(&self) -> bool {
        self.long_running
    }

    /// Called when a long job reports `JobDone`: the next queued one starts.
    pub fn long_job_done(&mut self, clock: Clock) -> Vec<Event> {
        self.long_running = false;
        match self.queue.pop_front() {
            Some(next) => self.run(next, clock),
            None => Vec::new(),
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Runs one command; quick ones answer with events at once, jobs answer later on the channel.
    /// A `LoadEntries` or `Search` is always answered: with its `Event::Entries` (the `request`
    /// echoed), or on failure with `JobDone(Failed)`, which also frees the state's next-page latch.
    pub fn run(&mut self, command: Command, clock: Clock) -> Vec<Event> {
        // Copy the `&'a Ressac` first: a store borrowed through `&self` would block `self.refresh`.
        let app = self.app;
        let store = app.store();
        let today = today_start(clock.local);
        let result: anyhow::Result<Vec<Event>> = match command {
            Command::LoadSources => store
                .sources(today)
                .map(|s| vec![Event::Sources(s)])
                .map_err(Into::into),
            Command::LoadEntries {
                source,
                filter,
                after,
                request,
            } => store
                .entries(source, filter, after.as_ref(), today)
                .map(|page| {
                    vec![Event::Entries {
                        source,
                        page,
                        appended: after.is_some(),
                        request,
                    }]
                })
                .map_err(Into::into),
            Command::LoadJournal => store
                .journal()
                .map(|j| vec![Event::Journal(j)])
                .map_err(Into::into),
            Command::Search {
                source,
                text,
                filter,
                after,
                request,
            } => {
                let appended = after.is_some();
                search(store, source, &text, filter, after, today).map(|page| {
                    vec![Event::Entries {
                        source,
                        page,
                        appended,
                        request,
                    }]
                })
            }
            Command::LoadEntry(id) => store
                .entry_detail(id)
                .map(|d| d.map(|d| Event::Entry(Box::new(d))).into_iter().collect())
                .map_err(Into::into),
            Command::SetRead(id, read) => store
                .set_read(id, read, clock.now)
                .map(|()| vec![Event::DataChanged])
                .map_err(Into::into),
            Command::SetStarred(id, on) => store
                .set_starred(id, on, clock.now)
                .map(|()| vec![Event::DataChanged])
                .map_err(Into::into),
            Command::MarkAllRead(source) => store
                .mark_all_read(source, today, clock.now)
                .map(|_| vec![Event::DataChanged])
                .map_err(Into::into),
            Command::SetCadence(feed, group) => app
                .set_cadence(feed, group)
                .map(|_| vec![Event::DataChanged])
                .map_err(Into::into),
            Command::OpenUrl(url) => open::that(&url).map(|()| Vec::new()).map_err(Into::into),
            Command::Extract(id) => {
                let _ = self.tx.send(Msg::Event(Event::Progress {
                    label: EXTRACT_PROGRESS_LABEL.into(),
                    done: 0,
                    total: 1,
                }));
                jobs::spawn(
                    self.db.clone(),
                    self.tx.clone(),
                    self.stop.clone(),
                    false,
                    move |app, _tx, _stop| async move {
                        jobs::extract_outcome(id, app.extract_entry(id, &NullSink).await)
                    },
                );
                Ok(Vec::new())
            }
            Command::Refresh(_) | Command::BuildEdition if self.long_running => {
                // The same request twice while one runs is one request.
                if !self.queue.contains(&command) {
                    self.queue.push_back(command);
                }
                Ok(Vec::new())
            }
            Command::Refresh(source) => self.refresh(source),
            Command::BuildEdition => {
                self.long_running = true;
                let now = clock.now;
                jobs::spawn(
                    self.db.clone(),
                    self.tx.clone(),
                    self.stop.clone(),
                    true,
                    move |app, tx, stop| async move {
                        let sink = ProgressSink::new(EDITION_PROGRESS_LABEL, tx);
                        jobs::edition_outcome(
                            app.build_edition(EditionOptions::default(), now, &Local, &sink, stop)
                                .await,
                        )
                    },
                );
                Ok(Vec::new())
            }
            // Phase 6 fetches images; phase 8 runs the garden's commands: nothing to do yet.
            Command::FetchImage(_) | Command::Garden(_) => Ok(Vec::new()),
            // Both are handled by the loop itself (it owns the settings path and the exit).
            Command::SaveSettings | Command::Quit => Ok(Vec::new()),
        };
        result.unwrap_or_else(|e| vec![Event::JobDone(JobOutcome::Failed(busy_or(&e)))])
    }

    fn refresh(&mut self, source: Option<Source>) -> anyhow::Result<Vec<Event>> {
        let only = match source {
            Some(Source::Feed(id)) => Some(vec![id]),
            Some(Source::Folder(id)) => Some(self.app.store().feeds_in_folder(id)?),
            _ => None,
        };
        self.long_running = true;
        jobs::spawn(
            self.db.clone(),
            self.tx.clone(),
            self.stop.clone(),
            true,
            move |app, tx, stop| async move {
                let sink = ProgressSink::new(FETCH_PROGRESS_LABEL, tx);
                match app
                    .refresh(RefreshOptions { force: true, only }, &sink, stop)
                    .await
                {
                    Ok(summary) if summary.network_unreachable => JobOutcome::Offline,
                    Ok(_) => JobOutcome::Refreshed,
                    Err(e) => JobOutcome::Failed(jobs::sentence(e.to_string())),
                }
            },
        );
        Ok(Vec::new())
    }
}

/// A search page; under `Unread` only the unread hits, topped up from the following pages so
/// that a page of hits is not left short while more remain.
fn search(
    store: &Store,
    source: Source,
    text: &str,
    filter: ListFilter,
    after: Option<ListCursor>,
    today: DateTime<Utc>,
) -> anyhow::Result<EntryPage> {
    let first = store.search_entries(source, text, after.as_ref(), today)?;
    if filter != ListFilter::Unread {
        return Ok(first);
    }
    unread_hits(first, |cursor| {
        Ok(store.search_entries(source, text, Some(cursor), today)?)
    })
}

/// `Store::search_entries` ignores the read state (as the app's search does); the reader's
/// Unread filter keeps the unread hits, and the count of the list becomes the unread count.
fn unread_hits(
    mut page: EntryPage,
    mut more: impl FnMut(&ListCursor) -> anyhow::Result<EntryPage>,
) -> anyhow::Result<EntryPage> {
    let mut rows = Vec::new();
    loop {
        rows.extend(page.rows.drain(..).filter(|row| !row.is_read));
        match page.next.clone() {
            Some(cursor) if rows.len() < PAGE_SIZE => page = more(&cursor)?,
            _ => break,
        }
    }
    page.total = page.unread;
    page.rows = rows;
    Ok(page)
}

fn busy_or(e: &anyhow::Error) -> String {
    jobs::sentence(format!("{e:#}"))
}

/// Opens the reader on `app` (already opened by `commands::read`) until `q`.
pub fn run(app: &Ressac, db: &Path, settings_path: &Path) -> anyhow::Result<()> {
    let settings = TerminalSettings::load(settings_path);
    let original: Arc<PanicHook> = Arc::new(std::panic::take_hook());
    forward_panics_to(&original);
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            forward_panics_to(&original);
            return Err(e).context("cannot start the reader (is this a terminal?)");
        }
    };
    // `try_init` wrapped our forwarder in a hook that restores the terminal on whichever
    // thread panics. A job thread must not tear the screen down under the loop: only a panic
    // of this thread restores it; the others are logged (a message on the screen would garble it).
    let restoring: Arc<PanicHook> = Arc::new(std::panic::take_hook());
    let loop_thread = std::thread::current().id();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == loop_thread {
            restoring(info);
        } else {
            tracing::error!("a background thread panicked: {info}");
        }
    }));
    // Everything fallible after the init sits in here: `restore` always runs.
    let result = (|| {
        let size = terminal.size()?;
        let mut state = ReaderState::new(settings, size.width, size.height);
        event_loop(&mut terminal, app, db, settings_path, &mut state)
    })();
    ratatui::restore();
    forward_panics_to(&original);
    result
}

type PanicHook = dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static;

fn forward_panics_to(hook: &Arc<PanicHook>) {
    let hook = Arc::clone(hook);
    std::panic::set_hook(Box::new(move |info| hook(info)));
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &Ressac,
    db: &Path,
    settings_path: &Path,
    state: &mut ReaderState,
) -> anyhow::Result<()> {
    let (tx, rx): (Sender<Msg>, Receiver<Msg>) = mpsc::channel();
    let mut exec = Executor::new(app, db.to_path_buf(), tx);
    let mut poll = Poll::new(Instant::now(), app.store().data_version()?);
    let mut last_tick = Instant::now();
    let clock = Clock::system();
    let mut pending: VecDeque<Command> = state.start(clock.now, clock.local).into();
    loop {
        // Execute everything the state asked for; quick answers go straight back to it.
        while let Some(command) = pending.pop_front() {
            match command {
                Command::Quit => {
                    exec.stop();
                    return Ok(());
                }
                Command::SaveSettings => {
                    // Never blocks reading: an unwritable file only forgets the choice.
                    let _ = state.settings.save(settings_path);
                }
                other => {
                    for event in exec.run(other, Clock::system()) {
                        // Its own writes do not move `data_version`: their `DataChanged` is here.
                        pending.extend(state.on_event(event));
                    }
                }
            }
        }
        let mut metrics = None;
        terminal.draw(|frame| metrics = Some(native::render(frame, state)))?;
        if let Some(m) = metrics {
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
        }
        // Messages from the jobs.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Event(event) => pending.extend(state.on_event(event)),
                Msg::LongJobDone(outcome) => {
                    pending.extend(state.on_event(Event::JobDone(outcome)));
                    for e in exec.long_job_done(Clock::system()) {
                        pending.extend(state.on_event(e));
                    }
                }
            }
        }
        // Keys and resizes.
        if event::poll(INPUT_WAIT)? {
            match event::read()? {
                TermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    poll.keyed(Instant::now());
                    pending.extend(state.on_key(key));
                }
                TermEvent::Resize(width, height) => {
                    pending.extend(state.on_event(Event::Resize { width, height }));
                }
                _ => {}
            }
        }
        // The clock, and what Ressac.app or a job wrote.
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            pending.extend(state.on_event(Clock::system().tick()));
        }
        let now = Instant::now();
        if poll.due(now) {
            match app.store().data_version() {
                Ok(version) => pending.extend(
                    poll.check(now, version)
                        .map(|e| state.on_event(e))
                        .into_iter()
                        .flatten(),
                ),
                Err(e) => {
                    // Transient (a busy database): the reader goes on and asks again.
                    tracing::warn!("cannot read data_version: {e}");
                    poll.failed(now);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Utc};
    use ressac_core::store::EntryRow;
    use std::time::{Duration, Instant};

    fn paris_summer() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn test_clock() -> Clock {
        let local = paris_summer()
            .with_ymd_and_hms(2026, 10, 16, 9, 0, 0)
            .unwrap();
        Clock {
            now: local.with_timezone(&Utc),
            local,
        }
    }

    fn open_app(dir: &tempfile::TempDir) -> (Ressac, PathBuf) {
        let db = dir.path().join("r.sqlite");
        (Ressac::open(&db).unwrap(), db)
    }

    fn row(id: i64, read: bool) -> EntryRow {
        EntryRow {
            entry_id: id,
            feed_id: 1,
            feed_title: "Feed".into(),
            title: Some(format!("Entry {id}")),
            author: None,
            url: None,
            published_at: "2026-10-16T08:00:00Z".parse().unwrap(),
            is_read: read,
            is_starred: false,
            has_fulltext: false,
        }
    }

    fn cursor(id: i64) -> ListCursor {
        ListCursor {
            published_at: "2026-10-15T08:00:00.000Z".into(),
            entry_id: id,
        }
    }

    #[test]
    fn today_starts_at_local_midnight() {
        let now = paris_summer()
            .with_ymd_and_hms(2026, 10, 16, 0, 30, 0)
            .unwrap();
        assert_eq!(
            today_start(now),
            Utc.with_ymd_and_hms(2026, 10, 15, 22, 0, 0).unwrap()
        );
        let evening = paris_summer()
            .with_ymd_and_hms(2026, 10, 16, 23, 59, 0)
            .unwrap();
        assert_eq!(
            today_start(evening),
            Utc.with_ymd_and_hms(2026, 10, 15, 22, 0, 0).unwrap()
        );
    }

    #[test]
    fn polling_waits_two_idle_seconds() {
        let start = Instant::now();
        let mut poll = Poll::new(start, 5);
        assert_eq!(poll.check(start + Duration::from_millis(1900), 6), None);
        assert_eq!(
            poll.check(start + Duration::from_secs(2), 6),
            Some(Event::DataChanged)
        );
        assert_eq!(
            poll.check(start + Duration::from_secs(4), 6),
            None,
            "unchanged version"
        );
        poll.keyed(start + Duration::from_secs(5));
        assert_eq!(
            poll.check(start + Duration::from_secs(6), 7),
            None,
            "a key resets the idle timer"
        );
        assert_eq!(
            poll.check(start + Duration::from_secs(7), 7),
            Some(Event::DataChanged)
        );
    }

    #[test]
    fn data_version_moves_when_another_connection_writes() {
        // Review Focus 2, loop side: what Ressac.app's writes look like to the reader.
        let dir = tempfile::tempdir().unwrap();
        let (reader, db) = open_app(&dir);
        let before = reader.store().data_version().unwrap();
        let other = Ressac::open(&db).unwrap();
        other
            .import_opml(
                r#"<opml version="2.0"><body><outline text="F" type="rss" xmlUrl="https://f.test/rss"/></body></opml>"#,
                false,
            )
            .unwrap();
        assert_ne!(reader.store().data_version().unwrap(), before);
    }

    #[test]
    fn own_writes_are_followed_by_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(Command::MarkAllRead(Source::Unread), test_clock());
        assert!(events.contains(&Event::DataChanged), "{events:?}");
    }

    #[test]
    fn long_jobs_queue_behind_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        exec.run(Command::Refresh(None), test_clock());
        exec.run(Command::BuildEdition, test_clock());
        exec.run(Command::BuildEdition, test_clock());
        exec.run(Command::Refresh(None), test_clock());
        assert_eq!(
            exec.queued(),
            2,
            "the edition and a refresh wait, once each"
        );
        exec.run(Command::Refresh(None), test_clock());
        assert_eq!(exec.queued(), 2);
        assert!(exec.long_job_running());
    }

    #[test]
    fn a_finished_long_job_starts_the_next_and_otherwise_frees_the_slot() {
        // `LongJobDone` (also sent after a panic, see jobs tests) is what calls `long_job_done`.
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        exec.run(Command::Refresh(None), test_clock());
        exec.run(Command::BuildEdition, test_clock());
        exec.long_job_done(test_clock());
        assert_eq!(exec.queued(), 0);
        assert!(exec.long_job_running(), "the edition started");
        exec.long_job_done(test_clock());
        assert!(!exec.long_job_running());
    }

    #[test]
    fn loading_a_list_answers_with_its_rows_and_echoes_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(
            Command::LoadEntries {
                source: Source::Unread,
                filter: ListFilter::All,
                after: None,
                request: 7,
            },
            test_clock(),
        );
        assert!(matches!(
            events.as_slice(),
            [Event::Entries {
                source: Source::Unread,
                appended: false,
                request: 7,
                ..
            }]
        ));
    }

    #[test]
    fn a_search_is_answered_too_with_its_request_and_its_paging() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(
            Command::Search {
                source: Source::Today,
                text: "rust".into(),
                filter: ListFilter::Unread,
                after: Some(cursor(3)),
                request: 4,
            },
            test_clock(),
        );
        assert!(matches!(
            events.as_slice(),
            [Event::Entries {
                source: Source::Today,
                appended: true,
                request: 4,
                ..
            }]
        ));
    }

    #[test]
    fn a_failure_is_answered_with_a_sentence_and_frees_the_next_page_latch() {
        // The executor answers an error with `JobDone(Failed)`; the state clears `loading_more`
        // on it (state test `a_failed_next_page_can_be_asked_again`).
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(Command::SetCadence(9_999, None), test_clock());
        assert!(
            matches!(events.as_slice(), [Event::JobDone(JobOutcome::Failed(_))]),
            "{events:?}"
        );
        assert_eq!(
            busy_or(&anyhow::anyhow!("database is locked")),
            "The database is busy (is Ressac.app writing?)"
        );
        assert_eq!(busy_or(&anyhow::anyhow!("boom")), "boom");
    }

    #[test]
    fn the_unread_filter_keeps_unread_hits_and_tops_up_short_pages() {
        let first = EntryPage {
            rows: vec![row(1, true), row(2, false)],
            total: 4,
            unread: 3,
            next: Some(cursor(2)),
        };
        let mut asked = Vec::new();
        let page = unread_hits(first, |after| {
            asked.push(after.entry_id);
            Ok(EntryPage {
                rows: vec![row(3, false), row(4, true)],
                total: 4,
                unread: 3,
                next: None,
            })
        })
        .unwrap();
        let ids: Vec<i64> = page.rows.iter().map(|r| r.entry_id).collect();
        assert_eq!(ids, [2, 3]);
        assert_eq!((page.total, page.unread), (3, 3));
        assert_eq!(page.next, None);
        assert_eq!(asked, [2]);
    }
}
