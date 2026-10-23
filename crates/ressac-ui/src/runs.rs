//! The Mac's `RunCoordinator`, `JournalList` progress texts and direct writes: every request
//! of the app (long and short), the one-long-run rule, progress and banner state, the
//! refusal classes copied from the CLI, the four direct writes and the fetch job's options.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use ressac_core::Error;
use ressac_core::model::{CadenceGroup, Decision, FeedId, FolderId};
use ressac_core::service::{Event, RefreshOptions, check_subset};
use ressac_core::store::{Source, Store};

use crate::sources::Selection;
use crate::worker::{JobFailure, JobResult};

pub const EDITION_LABEL: &str = "Building this morning's edition…";
pub const FETCHING_FEEDS: &str = "Fetching feeds";
pub const FETCHING_FULL_ARTICLES: &str = "Fetching full articles";
pub const BUSY: &str = "The database is busy (is `ressac` running in a terminal?)";
pub const COULD_NOT_SAVE: &str = "Could not save the change: ";
pub const COULD_NOT_READ: &str = "Could not read the database: ";
pub const PROGRESS_INSET: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Long,
    Short,
}

/// Everything the app asks the core to do in the background (inventory §8.2).
#[derive(Debug, Clone, PartialEq)]
pub enum RunRequest {
    // Long
    FetchAll,
    FetchFeeds(Vec<FeedId>),
    FetchFolder(FolderId),
    Import(PathBuf),
    BuildEdition {
        rebuild: bool,
    },
    Add {
        address: String,
        theme: Option<String>,
        folder: Option<FolderId>,
    },
    // Short
    Export {
        path: PathBuf,
        cemetery: bool,
    },
    Extract(i64),
    Decide(FeedId, Decision),
    Undo,
    ApplySafe,
    SetCadence(FeedId, Option<CadenceGroup>),
    SetTheme(FeedId, Option<String>),
    MoveFeed(FeedId, Option<FolderId>),
}

impl RunRequest {
    /// What a refresh does for the current selection.
    pub fn refresh(selection: &Selection) -> RunRequest {
        match selection {
            Selection::Source(Source::Feed(id)) => RunRequest::FetchFeeds(vec![*id]),
            Selection::Source(Source::Folder(id)) => RunRequest::FetchFolder(*id),
            _ => RunRequest::FetchAll,
        }
    }

    pub fn kind(&self) -> RunKind {
        match self {
            RunRequest::FetchAll
            | RunRequest::FetchFeeds(_)
            | RunRequest::FetchFolder(_)
            | RunRequest::Import(_)
            | RunRequest::BuildEdition { .. }
            | RunRequest::Add { .. } => RunKind::Long,
            _ => RunKind::Short,
        }
    }

    /// An extraction failure is stored on the entry and an add failure shows in its sheet.
    pub fn shows_failure_banner(&self) -> bool {
        !matches!(self, RunRequest::Extract(_) | RunRequest::Add { .. })
    }

    /// Whether `error` is one of the CLI's exit-2 refusals for this request: its sentence is
    /// shown as is, without Show Log.
    pub fn is_refusal(&self, error: &Error) -> bool {
        match self {
            RunRequest::Decide(..) | RunRequest::Undo => {
                matches!(error, Error::Invalid(_) | Error::UnknownFeed(_))
            }
            RunRequest::BuildEdition { .. } => matches!(error, Error::EditionNotDue),
            RunRequest::SetCadence(..) | RunRequest::SetTheme(..) | RunRequest::MoveFeed(..) => {
                matches!(
                    error,
                    Error::UnknownFeed(_)
                        | Error::FeedBuried(_)
                        | Error::UnknownFolder(_)
                        | Error::UnknownFolderName(_)
                        | Error::AmbiguousFolder { .. }
                        | Error::UnknownTheme { .. }
                )
            }
            RunRequest::Add { .. } => matches!(
                error,
                Error::InvalidAddress
                    | Error::NoFeedFound(_)
                    | Error::FeedExists { .. }
                    | Error::UnknownTheme { .. }
                    | Error::UnknownFolder(_)
                    | Error::UnknownFolderName(_)
                    | Error::AmbiguousFolder { .. }
                    | Error::UnknownFeed(_)
                    | Error::Invalid(_)
            ),
            _ => false,
        }
    }

    pub fn failure(&self, failure: &JobFailure) -> RunFailure {
        RunFailure {
            message: failure.message(),
            refusal: matches!(failure, JobFailure::Core(e) if self.is_refusal(e)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunProgress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

impl RunProgress {
    fn is_edition(&self) -> bool {
        self.label == EDITION_LABEL
    }

    /// Indeterminate (`None`) until something is done.
    pub fn fraction(&self) -> Option<f64> {
        (self.done > 0 && self.total > 0).then(|| (self.done as f64 / self.total as f64).min(1.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub message: String,
    pub show_log: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    pub message: String,
    pub refusal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Start,
    Refused,
}

/// How a run ended for the one who asked (the Mac's `RunOutcome`).
#[derive(Debug, Clone, PartialEq)]
pub enum RunEnd<T> {
    Done(T),
    /// Turned away at admission: another long run was active.
    Refused,
    Failed(RunFailure),
}

/// One long run at a time; short runs overlap it.
#[derive(Debug, Clone, Default)]
pub struct RunCoordinator {
    long_active: bool,
    progress: Option<RunProgress>,
    banner: Option<Banner>,
    banner_set_during_run: bool,
}

impl RunCoordinator {
    pub fn admit(&mut self, request: &RunRequest) -> Admission {
        if request.kind() == RunKind::Short {
            return Admission::Start;
        }
        if self.long_active {
            tracing::info!("refused {request:?}: a long run is active");
            return Admission::Refused;
        }
        self.long_active = true;
        self.banner_set_during_run = false;
        Admission::Start
    }

    /// Takes a run's event; true when the event changed the database (the caller reloads,
    /// throttled by `live::Invalidation`).
    pub fn event(&mut self, request: &RunRequest, event: &Event) -> bool {
        if request.kind() == RunKind::Long {
            self.track(request, event);
        }
        matches!(
            event,
            Event::FeedDone { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. }
                | Event::Finished { .. }
                | Event::Edition { .. }
        )
    }

    fn track(&mut self, request: &RunRequest, event: &Event) {
        match event {
            Event::Started { total } => {
                let label = if matches!(request, RunRequest::BuildEdition { .. }) {
                    EDITION_LABEL
                } else {
                    FETCHING_FEEDS
                };
                self.progress = Some(RunProgress {
                    done: 0,
                    total: *total,
                    label: label.into(),
                });
            }
            Event::FeedDone { .. } | Event::Extracted { .. } => {
                if let Some(progress) = &mut self.progress {
                    progress.done += 1;
                }
            }
            Event::Probing { what, total } => {
                let label = if what == ressac_core::service::FULL_ARTICLES {
                    FETCHING_FULL_ARTICLES.to_string()
                } else {
                    format!("Checking {what}")
                };
                self.progress = Some(RunProgress {
                    done: 0,
                    total: *total,
                    label,
                });
            }
            Event::NetworkUnreachable { failed, attempted } => {
                self.banner = Some(Banner {
                    message: network_unreachable(*failed, *attempted),
                    show_log: false,
                });
                self.banner_set_during_run = true;
            }
            _ => {}
        }
    }

    pub fn finish(&mut self, request: &RunRequest, outcome: Result<(), &RunFailure>) {
        let long = request.kind() == RunKind::Long;
        if long {
            self.long_active = false;
            self.progress = None;
        }
        match outcome {
            Ok(()) => {
                if !(long && self.banner_set_during_run) {
                    self.banner = None;
                }
            }
            Err(failure) if request.shows_failure_banner() => {
                self.banner = Some(Banner {
                    message: failure.message.clone(),
                    show_log: !failure.refusal,
                });
            }
            Err(_) => {}
        }
    }

    /// `finish` for a job's own result: a job failure, or a value `failure_of` reads as one
    /// (an offline edition), sets the banner as `finish` does; the value goes back as `Done`.
    pub fn conclude<T>(
        &mut self,
        request: &RunRequest,
        result: JobResult<T>,
        failure_of: impl FnOnce(&T) -> Option<RunFailure>,
    ) -> RunEnd<T> {
        match result {
            Err(job) => {
                let failure = request.failure(&job);
                self.finish(request, Err(&failure));
                RunEnd::Failed(failure)
            }
            Ok(value) => {
                let failure = failure_of(&value);
                self.finish(request, failure.as_ref().map_or(Ok(()), Err));
                RunEnd::Done(value)
            }
        }
    }

    pub fn long_run_active(&self) -> bool {
        self.long_active
    }

    pub fn can_refresh(&self) -> bool {
        !self.long_active
    }

    pub fn progress(&self) -> Option<&RunProgress> {
        self.progress.as_ref()
    }

    pub fn banner(&self) -> Option<&Banner> {
        self.banner.as_ref()
    }

    pub fn show_banner(&mut self, banner: Banner) {
        self.banner = Some(banner);
    }

    pub fn dismiss_banner(&mut self) {
        self.banner = None;
    }
}

/// The toolbar's progress text: an edition build counts its feeds.
pub fn progress_text(progress: &RunProgress) -> String {
    if progress.is_edition() && progress.total > 0 {
        format!("{} {}/{}", progress.label, progress.done, progress.total)
    } else {
        progress.label.clone()
    }
}

/// The same for a narrow window.
pub fn compact_progress_text(progress: &RunProgress) -> String {
    if !progress.is_edition() {
        progress.label.clone()
    } else if progress.total > 0 {
        format!("Edition… {}/{}", progress.done, progress.total)
    } else {
        "Edition…".to_string()
    }
}

pub fn progress_width(progress: &RunProgress) -> u32 {
    if progress.is_edition() { 260 } else { 140 }
}

pub fn network_unreachable(failed: usize, attempted: usize) -> String {
    format!("The network looks unreachable: {failed} of {attempted} feeds could not connect.")
}

/// The banner of a failed direct write: the terminal-holds-the-lock sentence, or the cause.
pub fn write_failure(failure: &JobFailure) -> Banner {
    let message = if failure.is_busy() {
        BUSY.to_string()
    } else {
        format!("{COULD_NOT_SAVE}{}", failure.message())
    };
    Banner {
        message,
        show_log: false,
    }
}

pub fn read_failure(error: &Error) -> Banner {
    Banner {
        message: format!("{COULD_NOT_READ}{error}"),
        show_log: true,
    }
}

/// The app's four direct writes; they run as short jobs on the worker's own connection, so a
/// busy wait (a `ressac fetch` in a terminal) never freezes the window.
#[derive(Debug, Clone, PartialEq)]
pub enum DirectWrite {
    SetRead {
        ids: Vec<i64>,
        read: bool,
    },
    SetStarred {
        id: i64,
        starred: bool,
    },
    MarkAllRead {
        source: Source,
        today_start: DateTime<Utc>,
    },
    PreferFullText {
        feed: FeedId,
        on: bool,
    },
}

impl DirectWrite {
    pub fn apply(&self, store: &Store, now: DateTime<Utc>) -> ressac_core::Result<()> {
        match self {
            DirectWrite::SetRead { ids, read } => {
                store.set_read_many(ids, *read, now)?;
            }
            DirectWrite::SetStarred { id, starred } => store.set_starred(*id, *starred, now)?,
            DirectWrite::MarkAllRead {
                source,
                today_start,
            } => {
                store.mark_all_read(*source, *today_start, now)?;
            }
            DirectWrite::PreferFullText { feed, on } => store.set_prefer_full_text(*feed, *on)?,
        }
        Ok(())
    }
}

/// The options of a fetch request, with its subset checked before the network is touched.
pub fn fetch_options(store: &Store, request: &RunRequest) -> ressac_core::Result<RefreshOptions> {
    let only = match request {
        RunRequest::FetchAll => None,
        RunRequest::FetchFeeds(ids) => {
            let checked = check_subset(store, ids, None)?;
            (!ids.is_empty()).then_some(checked)
        }
        RunRequest::FetchFolder(folder) => Some(check_subset(store, &[], Some(*folder))?),
        _ => return Err(Error::Invalid("not a fetch request".into())),
    };
    Ok(RefreshOptions { force: true, only })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entries::EntryList;
    use crate::sources::GardenScope;
    use crate::test_db::*;
    use crate::worker::Worker;
    use ressac_core::Error;
    use ressac_core::store::{ListFilter, ListQuery};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn busy() -> Error {
        Error::Db(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5),
            Some("database is locked".into()),
        ))
    }
    fn failed(e: Error) -> JobFailure {
        JobFailure::Core(e)
    }

    #[test]
    fn long_run_is_refused_while_another_is_active() {
        let mut runs = RunCoordinator::default();
        assert_eq!(runs.admit(&RunRequest::FetchAll), Admission::Start);
        assert!(runs.long_run_active() && !runs.can_refresh());
        assert_eq!(runs.admit(&RunRequest::FetchFolder(1)), Admission::Refused);
        assert_eq!(
            runs.admit(&RunRequest::Decide(1, Decision::Keep)),
            Admission::Start,
            "short runs overlap a long run"
        );
        runs.finish(&RunRequest::FetchAll, Ok(()));
        assert!(!runs.long_run_active() && runs.can_refresh());
        assert_eq!(
            runs.admit(&RunRequest::FetchFeeds(vec![1])),
            Admission::Start
        );
    }

    #[test]
    fn progress_follows_the_events() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        runs.admit(&fetch);
        assert!(!runs.event(&fetch, &Event::Started { total: 2 }));
        assert_eq!(
            runs.progress(),
            Some(&RunProgress {
                done: 0,
                total: 2,
                label: "Fetching feeds".into()
            })
        );
        assert_eq!(
            runs.progress().unwrap().fraction(),
            None,
            "indeterminate until something is done"
        );
        assert!(runs.event(
            &fetch,
            &Event::FeedDone {
                feed_id: 1,
                title: "A".into(),
                error: None
            }
        ));
        assert_eq!(runs.progress().unwrap().fraction(), Some(0.5));
        runs.event(
            &fetch,
            &Event::Probing {
                what: "full articles".into(),
                total: 3,
            },
        );
        assert_eq!(
            runs.progress(),
            Some(&RunProgress {
                done: 0,
                total: 3,
                label: "Fetching full articles".into()
            })
        );
        runs.event(
            &fetch,
            &Event::Probing {
                what: "https".into(),
                total: 4,
            },
        );
        assert_eq!(runs.progress().unwrap().label, "Checking https");
        runs.finish(&fetch, Ok(()));
        assert_eq!(runs.progress(), None);
    }

    #[test]
    fn the_toolbar_progress_counts_feeds_during_an_edition_build_and_shortens_when_narrow() {
        let edition = |done, total| RunProgress {
            done,
            total,
            label: EDITION_LABEL.into(),
        };
        let fetching = RunProgress {
            done: 3,
            total: 10,
            label: "Fetching feeds".into(),
        };
        assert_eq!(
            progress_text(&edition(34, 156)),
            "Building this morning's edition… 34/156"
        );
        assert_eq!(
            progress_text(&edition(0, 0)),
            "Building this morning's edition…"
        );
        assert_eq!(progress_text(&fetching), "Fetching feeds");
        assert_eq!(
            [
                compact_progress_text(&edition(152, 156)),
                compact_progress_text(&edition(0, 0)),
                compact_progress_text(&fetching)
            ],
            ["Edition… 152/156", "Edition…", "Fetching feeds"]
        );
        assert_eq!(
            (
                PROGRESS_INSET,
                progress_width(&edition(1, 2)),
                progress_width(&fetching)
            ),
            (10, 260, 140)
        );
        let mut runs = RunCoordinator::default();
        let build = RunRequest::BuildEdition { rebuild: false };
        runs.admit(&build);
        runs.event(&build, &Event::Started { total: 156 });
        assert_eq!(runs.progress().unwrap().label, EDITION_LABEL);
    }

    #[test]
    fn failure_shows_a_banner_and_the_next_success_clears_it() {
        let mut runs = RunCoordinator::default();
        let undo = RunRequest::Undo;
        let failure = undo.failure(&failed(Error::Io(std::io::Error::other("disk I/O error"))));
        runs.finish(&undo, Err(&failure));
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "I/O error: disk I/O error".into(),
                show_log: true
            })
        );
        runs.finish(&RunRequest::ApplySafe, Ok(()));
        assert_eq!(runs.banner(), None);
    }

    /// The CLI's exit-2 cases show their sentence without Show Log; the same error from a request
    /// the CLI never refuses keeps the log button.
    #[test]
    fn a_refused_run_shows_its_sentence_without_the_log_button() {
        let mut runs = RunCoordinator::default();
        for (request, error, sentence) in [
            (
                RunRequest::Decide(3, Decision::Repair),
                Error::Invalid("Nothing to repair for feed #3.".into()),
                "Nothing to repair for feed #3.",
            ),
            (
                RunRequest::SetCadence(11, Some(CadenceGroup::Daily)),
                Error::FeedBuried(11),
                "Feed #11 is buried.",
            ),
            (
                RunRequest::BuildEdition { rebuild: false },
                Error::EditionNotDue,
                "An edition was already built this morning.",
            ),
        ] {
            runs.finish(&request, Err(&request.failure(&failed(error))));
            assert_eq!(
                runs.banner(),
                Some(&Banner {
                    message: sentence.into(),
                    show_log: false
                }),
                "{request:?}"
            );
        }
        let fetch = RunRequest::FetchFeeds(vec![9]);
        runs.finish(&fetch, Err(&fetch.failure(&failed(Error::UnknownFeed(9)))));
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "No feed #9.".into(),
                show_log: true
            })
        );
    }

    #[test]
    fn only_garden_edition_feed_and_add_runs_can_be_refused() {
        let refusals: Vec<bool> = [
            RunRequest::Decide(9, Decision::Keep),
            RunRequest::Undo,
            RunRequest::BuildEdition { rebuild: true },
            RunRequest::SetCadence(9, None),
            RunRequest::SetTheme(9, None),
            RunRequest::MoveFeed(9, None),
            RunRequest::Add {
                address: "x".into(),
                theme: None,
                folder: None,
            },
            RunRequest::ApplySafe,
            RunRequest::FetchFeeds(vec![9]),
            RunRequest::FetchAll,
            RunRequest::Extract(9),
            RunRequest::Export {
                path: "/tmp/x.opml".into(),
                cemetery: false,
            },
        ]
        .iter()
        .map(|r| r.is_refusal(&Error::UnknownFeed(9)))
        .collect();
        assert_eq!(
            refusals,
            [
                true, true, false, true, true, true, true, false, false, false, false, false
            ]
        );
        assert!(RunRequest::BuildEdition { rebuild: false }.is_refusal(&Error::EditionNotDue));
        assert!(!RunRequest::Decide(9, Decision::Keep).is_refusal(&busy()));
        assert!(
            RunRequest::Add {
                address: "x".into(),
                theme: None,
                folder: None
            }
            .is_refusal(&Error::InvalidAddress)
        );
        assert!(
            !RunRequest::Add {
                address: "x".into(),
                theme: None,
                folder: None
            }
            .is_refusal(&Error::Unreachable {
                url: "u".into(),
                reason: "r".into()
            })
        );
    }

    #[test]
    fn request_kinds_and_banner_rules_follow_the_mac_table() {
        use RunKind::{Long, Short};
        let all = [
            RunRequest::FetchAll,
            RunRequest::FetchFeeds(vec![1]),
            RunRequest::FetchFolder(1),
            RunRequest::Import("/tmp/a.opml".into()),
            RunRequest::BuildEdition { rebuild: false },
            RunRequest::Add {
                address: "x".into(),
                theme: None,
                folder: None,
            },
            RunRequest::Export {
                path: "/tmp/x.opml".into(),
                cemetery: false,
            },
            RunRequest::Extract(1),
            RunRequest::Decide(1, Decision::Bury),
            RunRequest::Undo,
            RunRequest::ApplySafe,
            RunRequest::SetCadence(1, None),
            RunRequest::SetTheme(1, None),
            RunRequest::MoveFeed(1, None),
        ];
        assert_eq!(
            all.iter().map(RunRequest::kind).collect::<Vec<_>>(),
            [
                Long, Long, Long, Long, Long, Long, Short, Short, Short, Short, Short, Short,
                Short, Short
            ]
        );
        assert_eq!(
            all.iter()
                .filter(|r| !r.shows_failure_banner())
                .cloned()
                .collect::<Vec<_>>(),
            [
                RunRequest::Add {
                    address: "x".into(),
                    theme: None,
                    folder: None
                },
                RunRequest::Extract(1)
            ]
        );
    }

    #[test]
    fn refresh_targets_the_current_source() {
        assert_eq!(
            RunRequest::refresh(&Selection::Source(Source::Feed(4))),
            RunRequest::FetchFeeds(vec![4])
        );
        assert_eq!(
            RunRequest::refresh(&Selection::Source(Source::Folder(2))),
            RunRequest::FetchFolder(2)
        );
        for s in [
            Selection::Source(Source::Unread),
            Selection::Source(Source::Journal),
            Selection::Source(Source::Cadence(CadenceGroup::Rare)),
            Selection::Garden(GardenScope::All),
        ] {
            assert_eq!(RunRequest::refresh(&s), RunRequest::FetchAll, "{s:?}");
        }
    }

    #[test]
    fn network_warning_survives_its_own_successful_run_and_extraction_failures_stay_out() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        runs.admit(&fetch);
        runs.event(
            &fetch,
            &Event::NetworkUnreachable {
                failed: 4,
                attempted: 5,
            },
        );
        runs.finish(&fetch, Ok(()));
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "The network looks unreachable: 4 of 5 feeds could not connect.".into(),
                show_log: false
            })
        );
        runs.dismiss_banner();
        let extract = RunRequest::Extract(9);
        runs.finish(
            &extract,
            Err(&extract.failure(&failed(Error::Invalid("HTTP 404.".into())))),
        );
        assert_eq!(runs.banner(), None);
    }

    #[test]
    fn database_changes_are_told_for_the_events_that_write() {
        let mut runs = RunCoordinator::default();
        let fetch = RunRequest::FetchAll;
        let told: Vec<bool> = [
            Event::Started { total: 1 },
            Event::Probing {
                what: "https".into(),
                total: 1,
            },
            Event::WaybackDone { found: 0 },
            Event::FeedDone {
                feed_id: 1,
                title: "A".into(),
                error: None,
            },
            Event::Extracted {
                entry_id: 1,
                ok: true,
                error: None,
            },
            Event::Decided {
                feed_id: 1,
                decision: "keep".into(),
            },
            Event::Undone { feed_id: None },
        ]
        .iter()
        .map(|e| runs.event(&fetch, e))
        .collect();
        assert_eq!(told, [false, false, false, true, true, true, true]);
    }

    #[test]
    fn direct_writes_round_trip_and_failures_name_the_busy_terminal() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let store = db.store();
        let now = at("2026-10-16T09:00:00Z");
        DirectWrite::SetRead {
            ids: ids.clone(),
            read: true,
        }
        .apply(&store, now)
        .unwrap();
        assert_eq!(
            db.read_at(ids[0]).as_deref(),
            Some("2026-10-16T09:00:00.000Z")
        );
        DirectWrite::SetStarred {
            id: ids[1],
            starred: true,
        }
        .apply(&store, now)
        .unwrap();
        assert!(db.starred(ids[1]));
        DirectWrite::SetRead {
            ids: vec![ids[0]],
            read: false,
        }
        .apply(&store, now)
        .unwrap();
        DirectWrite::MarkAllRead {
            source: Source::Feed(1),
            today_start: today(),
        }
        .apply(&store, now)
        .unwrap();
        assert!(db.read_at(ids[0]).is_some());
        DirectWrite::PreferFullText { feed: 1, on: true }
            .apply(&store, now)
            .unwrap();
        assert!(store.prefer_full_text(1).unwrap());
        assert_eq!(
            write_failure(&failed(busy())),
            Banner {
                message: BUSY.into(),
                show_log: false
            }
        );
        assert_eq!(
            BUSY,
            "The database is busy (is `ressac` running in a terminal?)"
        );
        assert_eq!(
            write_failure(&failed(Error::Invalid("disk full".into()))),
            Banner {
                message: "Could not save the change: disk full".into(),
                show_log: false
            }
        );
        assert_eq!(
            read_failure(&Error::Invalid("gone".into())),
            Banner {
                message: "Could not read the database: gone".into(),
                show_log: true
            }
        );
    }

    #[test]
    fn fetch_checks_its_subset_before_the_network() {
        let db = TestDb::new();
        let store = db.store();
        let only = |r| fetch_options(&store, &r).map(|o| (o.force, o.only));
        assert_eq!(only(RunRequest::FetchAll).unwrap(), (true, None));
        assert_eq!(
            only(RunRequest::FetchFeeds(vec![8, 1])).unwrap(),
            (true, Some(vec![8, 1]))
        );
        assert_eq!(
            only(RunRequest::FetchFolder(1)).unwrap(),
            (true, Some(vec![1, 2, 3, 4, 5])),
            "subfolders included"
        );
        assert_eq!(
            only(RunRequest::FetchFeeds(vec![99]))
                .unwrap_err()
                .to_string(),
            "No feed #99."
        );
        assert_eq!(
            only(RunRequest::FetchFolder(99)).unwrap_err().to_string(),
            "No folder #99."
        );
        assert!(only(RunRequest::Undo).is_err());
    }

    /// Review Focus 3 (master plan): `ressac fetch` in Konsole holds the write lock while the app marks
    /// an entry read. The mark shows at once, submitting never waits, the GUI connection keeps reading,
    /// and the write lands when the terminal commits (here after 1.5 s).
    #[test]
    fn a_read_mark_waits_on_the_worker_while_a_terminal_writes_and_the_list_never_blocks() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let gui = db.store();
        let mut list = EntryList::new(ListQuery {
            source: Source::Unread,
            filter: ListFilter::All,
            search: None,
        });
        list.restart(&gui, None, today()).unwrap();
        let worker = Worker::new(&db.path);
        let terminal = db.hold_write_lock(Duration::from_millis(1500));
        let edit = list.set_read_local(&[ids[0]], true);
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let write = DirectWrite::SetRead {
            ids: vec![ids[0]],
            read: true,
        };
        worker.direct(
            move |cx| write.apply(cx.app.store(), chrono::Utc::now()),
            move |r| {
                tx.send((r.map_err(|f| f.message()), start.elapsed()))
                    .unwrap()
            },
        );
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "submitting never waits"
        );
        let read = Instant::now();
        list.reload(&gui, today()).unwrap();
        assert!(
            read.elapsed() < Duration::from_secs(1),
            "the GUI connection reads while the terminal writes"
        );
        assert!(list.rows()[0].is_read, "the optimistic mark holds");
        let (result, waited) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(result, Ok(()));
        assert!(waited >= Duration::from_secs(1), "{waited:?}");
        list.settle(edit);
        terminal.join().unwrap();
        assert!(db.read_at(ids[0]).is_some());
        list.reload(&gui, today()).unwrap();
        assert!(list.rows()[0].is_read);
    }

    /// Review Focus 3: still locked after the 5 s busy timeout → the row reverts and the banner names the terminal.
    #[test]
    fn a_read_mark_still_blocked_after_the_busy_timeout_reverts_with_the_busy_sentence() {
        let db = TestDb::new();
        let ids = db.days(2, 1, |_| false);
        let gui = db.store();
        let mut list = EntryList::new(ListQuery {
            source: Source::Unread,
            filter: ListFilter::All,
            search: None,
        });
        list.restart(&gui, None, today()).unwrap();
        let worker = Worker::new(&db.path);
        let terminal = db.hold_write_lock(Duration::from_millis(6500));
        let edit = list.set_read_local(&[ids[0]], true);
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let write = DirectWrite::SetRead {
            ids: vec![ids[0]],
            read: true,
        };
        worker.direct(
            move |cx| write.apply(cx.app.store(), chrono::Utc::now()),
            move |r| tx.send((r, start.elapsed())).unwrap(),
        );
        list.reload(&gui, today()).unwrap();
        assert!(
            list.rows()[0].is_read,
            "a poll's reload during the wait keeps the mark"
        );
        let (result, waited) = rx.recv_timeout(Duration::from_secs(15)).unwrap();
        let failure = result.unwrap_err();
        assert!(failure.is_busy(), "{}", failure.message());
        assert!(
            waited >= Duration::from_millis(4500),
            "the busy timeout was waited: {waited:?}"
        );
        list.revert(edit);
        assert!(!list.rows()[0].is_read);
        let mut runs = RunCoordinator::default();
        runs.show_banner(write_failure(&failure));
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "The database is busy (is `ressac` running in a terminal?)".into(),
                show_log: false
            })
        );
        terminal.join().unwrap();
        assert_eq!(db.read_at(ids[0]), None);
    }

    #[test]
    fn conclude_hands_a_job_failure_back_and_shows_its_banner() {
        let mut runs = RunCoordinator::default();
        runs.admit(&RunRequest::FetchAll);
        let end = runs.conclude::<()>(&RunRequest::FetchAll, Err(JobFailure::Panicked), |_| None);
        assert_eq!(
            end,
            RunEnd::Failed(RunFailure {
                message: "The job stopped unexpectedly.".into(),
                refusal: false
            })
        );
        assert_eq!(
            runs.banner(),
            Some(&Banner {
                message: "The job stopped unexpectedly.".into(),
                show_log: true
            })
        );
        assert!(!runs.long_run_active());
        runs.admit(&RunRequest::FetchAll);
        assert_eq!(
            runs.conclude(&RunRequest::FetchAll, Ok(()), |_| None),
            RunEnd::Done(())
        );
        assert_eq!(runs.banner(), None);
    }
}
