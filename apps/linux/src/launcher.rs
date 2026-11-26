//! The launch of the app (inventory §1.1): resolve the database, probe it, migrate on a worker
//! thread when needed, open it, and tell the window through the `Launcher` QML singleton. The
//! decisions are `ressac_ui::launch`'s.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use ressac_core::Error;
use ressac_core::paths::Paths;
use ressac_core::store::{SchemaState, Store};
use ressac_ui::launch::{
    LaunchPhase, LaunchStep, after_migration, after_probe, cannot_open, database_location,
};
use ressac_ui::settings::{SettingsStore, keys};
use ressac_ui::worker::Worker;

use crate::settings::QSettingsStore;

const NO_HOME: &str = "HOME is not set to an absolute path";

/// The database the GUI thread has open once the launch is ready.
pub(crate) struct Session {
    pub store: Store,
    /// The job threads (runs, direct writes); `worker.db()` is the database path.
    pub worker: Worker,
}

thread_local! {
    // The GUI thread's own connection: a `Store` never crosses threads.
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

fn install_session(session: Session) {
    SESSION.with(|slot| *slot.borrow_mut() = Some(session));
}

/// Ends the session at exit: the long run stopped, the worker's lanes joined (a lane busy
/// beyond `SHUTDOWN_WAIT` is logged), the connection closed.
pub(crate) fn shutdown_session() {
    if let Some(session) = SESSION.with(|slot| slot.borrow_mut().take()) {
        let Session { store, worker } = session;
        worker.shutdown(SHUTDOWN_WAIT);
        drop(store);
    }
}

/// How long the exit waits for the worker's lanes.
const SHUTDOWN_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

fn drop_session() {
    SESSION.with(|slot| *slot.borrow_mut() = None);
}

/// Runs `f` on the open session; `None` before the launch is ready (and off the GUI thread).
pub(crate) fn with_session<R>(f: impl FnOnce(&Session) -> R) -> Option<R> {
    SESSION.with(|slot| slot.borrow().as_ref().map(f))
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[qenum(Launcher)]
    enum Phase {
        Starting,
        Migrating,
        TooNew,
        Failed,
        Ready,
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(Phase, phase, READ = phase, NOTIFY)]
        #[qproperty(QString, message, READ = message, NOTIFY)]
        #[qproperty(QString, database_path, READ = database_path, NOTIFY)]
        type Launcher = super::LauncherRust;

        fn phase(self: &Launcher) -> Phase;
        fn message(self: &Launcher) -> QString;
        fn database_path(self: &Launcher) -> QString;

        /// Once, from `Starting`.
        #[qinvokable]
        fn launch(self: Pin<&mut Launcher>);

        /// "Use Default Database": only when the window offers it.
        #[qinvokable]
        fn relaunch(self: Pin<&mut Launcher>);
    }

    impl cxx_qt::Threading for Launcher {}
}

pub struct LauncherRust {
    phase: LaunchPhase,
    db: Option<PathBuf>,
    /// A migration is running on its worker.
    launching: bool,
}

impl Default for LauncherRust {
    fn default() -> Self {
        LauncherRust {
            phase: LaunchPhase::Starting,
            db: None,
            launching: false,
        }
    }
}

/// The database to launch with: `RESSAC_DB`, the stored setting, the default location.
fn resolve_database() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute());
    let default_db = Paths::from_env().ok().map(|paths| paths.database());
    let stored = QSettingsStore::default_file().get(keys::DATABASE_PATH);
    database_location(
        std::env::var_os("RESSAC_DB").as_deref(),
        stored.as_deref(),
        home.as_deref(),
        default_db.as_deref(),
    )
    .ok()
}

impl qobject::Launcher {
    fn phase(&self) -> qobject::Phase {
        match self.rust().phase {
            LaunchPhase::Starting => qobject::Phase::Starting,
            LaunchPhase::Migrating => qobject::Phase::Migrating,
            LaunchPhase::TooNew(_) => qobject::Phase::TooNew,
            LaunchPhase::Failed(_) => qobject::Phase::Failed,
            LaunchPhase::Ready => qobject::Phase::Ready,
        }
    }

    fn message(&self) -> QString {
        QString::from(self.rust().phase.message().unwrap_or_default().as_str())
    }

    fn database_path(&self) -> QString {
        let db = self.rust().db.as_deref().unwrap_or(Path::new(""));
        QString::from(db.to_string_lossy().as_ref())
    }

    fn set_phase(mut self: Pin<&mut Self>, phase: LaunchPhase) {
        tracing::info!("launch: {phase:?}");
        self.as_mut().rust_mut().get_mut().phase = phase;
        self.as_mut().phase_changed();
        self.as_mut().message_changed();
    }

    fn launch(mut self: Pin<&mut Self>) {
        if self.rust().phase != LaunchPhase::Starting || self.rust().launching {
            return;
        }
        let Some(db) = resolve_database() else {
            self.set_phase(LaunchPhase::Failed(NO_HOME.to_string()));
            return;
        };
        tracing::info!("launch: database {}", db.display());
        self.as_mut().rust_mut().get_mut().db = Some(db.clone());
        self.as_mut().database_path_changed();
        let step = after_probe(Store::probe(&db), &db);
        self.follow(step, &db);
    }

    fn follow(mut self: Pin<&mut Self>, step: LaunchStep, db: &Path) {
        match step {
            LaunchStep::Open => match Store::open_existing(db) {
                Ok(store) => {
                    install_session(Session {
                        store,
                        worker: Worker::new(db),
                    });
                    self.set_phase(LaunchPhase::Ready);
                }
                Err(error) => self.set_phase(cannot_open(db, &error)),
            },
            LaunchStep::Migrate => {
                self.as_mut().rust_mut().get_mut().launching = true;
                self.as_mut().set_phase(LaunchPhase::Migrating);
                let qt = self.qt_thread();
                let db = db.to_path_buf();
                std::thread::spawn(move || {
                    // Its own connection: creates or migrates (the core backs up first), then
                    // probes again.
                    let outcome = Store::open(&db).map(drop).and_then(|()| Store::probe(&db));
                    let queued = qt.queue(move |launcher| launcher.apply_migration(outcome));
                    if queued.is_err() {
                        tracing::debug!("the app quit before the migration finished");
                    }
                });
            }
            LaunchStep::Show(phase) => self.set_phase(phase),
        }
    }

    /// The worker's result, back on the GUI thread.
    fn apply_migration(mut self: Pin<&mut Self>, outcome: Result<SchemaState, Error>) {
        self.as_mut().rust_mut().get_mut().launching = false;
        let Some(db) = self.rust().db.clone() else {
            return;
        };
        let step = after_migration(outcome, &db);
        self.follow(step, &db);
    }

    fn relaunch(mut self: Pin<&mut Self>) {
        if !self.rust().phase.offers_default_database() || self.rust().launching {
            return;
        }
        drop_session();
        self.as_mut().set_phase(LaunchPhase::Starting);
        self.launch();
    }
}
