//! `ressac read`: the reader's event loop. The state decides, this module executes.

pub mod html;
pub mod images;
pub mod jobs;
pub mod journal;
pub mod layout;
pub mod native;
pub mod rhythm;
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
use ressac_core::model::FeedId;
use ressac_core::service::{EditionOptions, NullSink, RefreshOptions, Ressac};
use ressac_core::store::{EntryPage, ListCursor, ListFilter, PAGE_SIZE, Source, Store};

use jobs::{Msg, ProgressSink};
use journal::BUILDING;
use native::{EXTRACT_PROGRESS_LABEL, FETCH_PROGRESS_LABEL};
use rhythm::RhythmInfo;
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
    /// The long job in the slot is the edition build (else a refresh): see `long_job_outcome`.
    edition_running: bool,
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
            edition_running: false,
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

    /// The outcome of the long job that just ended, in its own terms: a failure or an offline
    /// answer of the edition build is the edition's (`EditionFailed`, `EditionOffline`), so that
    /// the Journal's phase never follows a refresh or a quick command. Call it before
    /// `long_job_done`, which may start the next job.
    pub fn long_job_outcome(&self, outcome: JobOutcome) -> JobOutcome {
        match outcome {
            JobOutcome::Failed(sentence) if self.edition_running => {
                JobOutcome::EditionFailed(sentence)
            }
            JobOutcome::Offline if self.edition_running => JobOutcome::EditionOffline,
            other => other,
        }
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
            Command::LoadCadence(feed) => cadence_events(store, feed),
            // The new line at once, then `DataChanged`: this connection's own write does not move
            // `data_version`, and the sources' rhythm counts and a rhythm list must follow.
            Command::SetCadence(feed, group) => app
                .set_cadence(feed, group)
                .map_err(anyhow::Error::from)
                .and_then(|_| cadence_events(store, feed))
                .map(|mut events| {
                    events.push(Event::DataChanged);
                    events
                }),
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
                self.edition_running = true;
                // The edition's day and zone are the injected clock's, not the machine's.
                let now = clock.now;
                let zone = clock.local.timezone();
                jobs::spawn(
                    self.db.clone(),
                    self.tx.clone(),
                    self.stop.clone(),
                    true,
                    move |app, tx, stop| async move {
                        let sink = ProgressSink::new(BUILDING, tx);
                        jobs::edition_outcome(
                            app.build_edition(EditionOptions::default(), now, &zone, &sink, stop)
                                .await,
                        )
                    },
                );
                Ok(Vec::new())
            }
            Command::LoadGarden => garden_events(store, today, None),
            // Nothing changes in the database: no rows or sources to reload.
            Command::Garden(
                command @ (crate::tui::state::Command::None
                | crate::tui::state::Command::Quit
                | crate::tui::state::Command::Open(_)),
            ) => crate::tui::apply_garden_command(app, command)
                .or_else(|e| Ok(Some(busy_or(&e))))
                .map(|message| vec![Event::GardenMessage(message)]),
            Command::Garden(command) => {
                let message = match crate::tui::apply_garden_command(app, command) {
                    Ok(message) => message,
                    Err(e) => Some(busy_or(&e)),
                };
                garden_events(store, today, Some(message))
            }
            // The loop handles `FetchImage` itself (the image store owns those threads).
            Command::FetchImage(_) => Ok(Vec::new()),
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
        self.edition_running = false;
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

/// The rhythm line's data for one feed (`v_cadence` holds the active, diagnosed feeds only: no
/// event for the others).
fn cadence_events(store: &Store, feed: FeedId) -> anyhow::Result<Vec<Event>> {
    Ok(store
        .cadence_details()?
        .iter()
        .find(|detail| detail.feed_id == feed)
        .map(|detail| Event::Cadence(RhythmInfo::from(detail)))
        .into_iter()
        .collect())
}

/// The garden's fresh rows, then (after a garden command) its message, and the sources: Needs
/// triage's count and the rhythm groups change with a decision.
fn garden_events(
    store: &Store,
    today: DateTime<Utc>,
    message: Option<Option<String>>,
) -> anyhow::Result<Vec<Event>> {
    let mut events = vec![Event::GardenRows(ressac_core::garden::load_rows(store)?)];
    if let Some(message) = message {
        events.push(Event::GardenMessage(message));
        events.push(Event::Sources(store.sources(today)?));
    }
    Ok(events)
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
        // After the init, never before it: see `images::detect_picker`.
        let mut images = images::ImageStore::new(images::detect_picker());
        event_loop(
            &mut terminal,
            app,
            db,
            settings_path,
            &mut state,
            &mut images,
        )
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
    images: &mut images::ImageStore,
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
                // Not a long job: its own thread and its own queue, the long-job slot stays free.
                Command::FetchImage(url) => images.fetch(db, url),
                other => {
                    for event in exec.run(other, Clock::system()) {
                        // Its own writes do not move `data_version`: their `DataChanged` is here.
                        pending.extend(state.on_event(event));
                    }
                }
            }
        }
        let mut metrics = None;
        terminal.draw(|frame| metrics = Some(native::render(frame, state, images)))?;
        if let Some(m) = metrics {
            state.set_reader_metrics(m.total_rows, m.viewport_rows);
        }
        // Images the renderer asked for start (four at a time); the ones that ended come back.
        images.start_wanted(db);
        for event in images.drain(db) {
            pending.extend(state.on_event(event));
        }
        // Messages from the jobs.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Event(event) => pending.extend(state.on_event(event)),
                Msg::LongJobDone(outcome) => {
                    let outcome = exec.long_job_outcome(outcome);
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

    /// Everything a long job sends, up to and including its `LongJobDone`.
    fn until_long_job_done(rx: &Receiver<Msg>) -> (Vec<Event>, JobOutcome) {
        let mut events = Vec::new();
        loop {
            match rx
                .recv_timeout(Duration::from_secs(10))
                .expect("the job reports")
            {
                Msg::Event(event) => events.push(event),
                Msg::LongJobDone(outcome) => return (events, outcome),
            }
        }
    }

    #[test]
    fn the_edition_build_runs_as_a_job_and_reports_its_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        assert!(exec.run(Command::BuildEdition, test_clock()).is_empty());
        let (events, outcome) = until_long_job_done(&rx);
        assert_eq!(outcome, JobOutcome::EditionBuilt);
        // Whatever progress it reported is the edition's, under the edition's label.
        assert!(
            events
                .iter()
                .all(|e| matches!(e, Event::Progress { label, .. } if label == journal::BUILDING))
        );
        exec.long_job_done(test_clock());
        // The same morning, by the injected clock's 04:00 rule: not due, and no progress at all.
        let later = Clock {
            now: test_clock().now + chrono::Duration::hours(3),
            local: test_clock().local + chrono::Duration::hours(3),
        };
        exec.run(Command::BuildEdition, later);
        let (events, outcome) = until_long_job_done(&rx);
        assert_eq!(outcome, JobOutcome::EditionNotDue);
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn the_edition_s_day_is_the_injected_clock_s_not_the_machine_s() {
        // 03:30 local on the 17th is still the 16th's edition day: the build at 09:00 on the 16th
        // already covers it, whatever the machine's own clock and zone say.
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        exec.run(Command::BuildEdition, test_clock());
        assert_eq!(until_long_job_done(&rx).1, JobOutcome::EditionBuilt);
        exec.long_job_done(test_clock());
        let night = paris_summer()
            .with_ymd_and_hms(2026, 10, 17, 3, 30, 0)
            .unwrap();
        exec.run(
            Command::BuildEdition,
            Clock {
                now: night.with_timezone(&Utc),
                local: night,
            },
        );
        assert_eq!(until_long_job_done(&rx).1, JobOutcome::EditionNotDue);
        exec.long_job_done(test_clock());
        let morning = paris_summer()
            .with_ymd_and_hms(2026, 10, 17, 4, 0, 0)
            .unwrap();
        exec.run(
            Command::BuildEdition,
            Clock {
                now: morning.with_timezone(&Utc),
                local: morning,
            },
        );
        assert_eq!(until_long_job_done(&rx).1, JobOutcome::EditionBuilt);
    }

    #[test]
    fn only_the_edition_job_s_failures_are_the_editions() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let failed = || JobOutcome::Failed("The job stopped unexpectedly.".into());
        // A refresh: its outcomes stay its own.
        exec.run(Command::Refresh(None), test_clock());
        assert_eq!(exec.long_job_outcome(failed()), failed());
        assert_eq!(
            exec.long_job_outcome(JobOutcome::Offline),
            JobOutcome::Offline
        );
        let _ = until_long_job_done(&rx);
        // The edition: a panic or a database that would not open is its failure.
        exec.long_job_done(test_clock());
        exec.run(Command::BuildEdition, test_clock());
        assert_eq!(
            exec.long_job_outcome(failed()),
            JobOutcome::EditionFailed("The job stopped unexpectedly.".into())
        );
        assert_eq!(
            exec.long_job_outcome(JobOutcome::Offline),
            JobOutcome::EditionOffline
        );
        assert_eq!(
            exec.long_job_outcome(JobOutcome::EditionBuilt),
            JobOutcome::EditionBuilt
        );
        let _ = until_long_job_done(&rx);
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
    fn set_cadence_answers_with_the_new_line_then_a_data_change() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        app.import_opml(
            include_str!("../../../../../tests/fixtures/vienna-like.opml"),
            false,
        )
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        app.diagnose_all(test_clock().now).unwrap();
        let feed = app.store().feeds().unwrap()[0].id;
        let events = exec.run(
            Command::SetCadence(feed, Some(ressac_core::model::CadenceGroup::Daily)),
            test_clock(),
        );
        assert!(
            matches!(events.as_slice(),
                [Event::Cadence(info), Event::DataChanged]
                    if info.feed_id == feed && info.is_override
                        && info.effective == ressac_core::model::CadenceGroup::Daily),
            "{events:?}"
        );
        // Auto again; LoadCadence answers on its own with the same shape.
        let events = exec.run(Command::SetCadence(feed, None), test_clock());
        assert!(
            matches!(events.first(), Some(Event::Cadence(i)) if !i.is_override),
            "{events:?}"
        );
        let events = exec.run(Command::LoadCadence(feed), test_clock());
        assert!(matches!(events.as_slice(), [Event::Cadence(i)] if i.feed_id == feed));
        // A feed outside `v_cadence` has no line.
        assert!(
            exec.run(Command::LoadCadence(9_999), test_clock())
                .is_empty()
        );
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

    #[test]
    fn garden_commands_reload_the_rows_and_report_the_message() {
        use ressac_core::model::Decision;
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        app.import_opml(
            include_str!("../../../../../tests/fixtures/vienna-like.opml"),
            false,
        )
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(Command::LoadGarden, test_clock());
        let [Event::GardenRows(rows)] = events.as_slice() else {
            panic!("{events:?}")
        };
        let feed = rows[0].feed_id;
        let events = exec.run(
            Command::Garden(crate::tui::state::Command::Decide(feed, Decision::Bury)),
            test_clock(),
        );
        assert!(events.iter().any(
            |e| matches!(e, Event::GardenRows(r) if r.iter().any(|row| row.feed_id == feed && row.buried))
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::GardenMessage(None)))
        );
        assert!(
            events.iter().any(|e| matches!(e, Event::Sources(_))),
            "the counts change after a bury"
        );
    }

    #[test]
    fn a_garden_command_that_writes_nothing_reloads_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(
            Command::Garden(crate::tui::state::Command::None),
            test_clock(),
        );
        assert_eq!(events, vec![Event::GardenMessage(None)]);
    }

    #[test]
    fn a_failed_garden_command_is_a_message_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let (app, db) = open_app(&dir);
        let (tx, _rx) = mpsc::channel();
        let mut exec = Executor::new(&app, db, tx);
        let events = exec.run(
            Command::Garden(crate::tui::state::Command::Decide(
                9_999,
                ressac_core::model::Decision::Bury,
            )),
            test_clock(),
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::GardenMessage(Some(_)))),
            "{events:?}"
        );
    }
}
