//! The launch decisions of the app, as pure functions: where the database is, what to do
//! with what [`Store::probe`] found, and what the command line may say. The app (`apps/linux`)
//! does the I/O and draws the phases; the sentences are the Mac app's.

use ressac_core::Error;
use ressac_core::paths::PathsError;
use ressac_core::store::SchemaState;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const UPDATING: &str = "Updating the database…";
pub const TOO_NEW: &str = "This database was created by a newer Ressac";
pub const NOT_MIGRATED: &str = "The database could not be migrated.";
/// Settings ▸ Data: a chosen database that is newer than this app.
pub const NEWER_DATABASE: &str = "This database was created by a newer Ressac.";
/// Settings ▸ Data: a chosen file that is not a database.
pub const NOT_A_DATABASE: &str = "This file is not a usable Ressac database.";
pub const SCREENSHOT_NEEDS_DB: &str =
    "ressac-kde: --screenshot needs RESSAC_DB (a copy of a fixture database, never the real one)";

/// Where the launch stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPhase {
    Starting,
    Migrating,
    /// The database's schema is newer than this app's.
    TooNew(u32),
    Failed(String),
    Ready,
}

impl LaunchPhase {
    /// The sentence the window shows, if any.
    pub fn message(&self) -> Option<String> {
        match self {
            Self::Starting | Self::Ready => None,
            Self::Migrating => Some(UPDATING.to_string()),
            Self::TooNew(_) => Some(TOO_NEW.to_string()),
            Self::Failed(message) => Some(message.clone()),
        }
    }

    /// Whether the window offers "Use Default Database".
    pub fn offers_default_database(&self) -> bool {
        matches!(self, Self::TooNew(_) | Self::Failed(_))
    }
}

/// What the launch does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchStep {
    /// Open the database and show the app.
    Open,
    /// Create or migrate it on a worker thread, then probe again.
    Migrate,
    Show(LaunchPhase),
}

/// The failure shown when the database cannot be opened.
pub fn cannot_open(db: &Path, error: &Error) -> LaunchPhase {
    LaunchPhase::Failed(format!("Cannot open {}: {error}", db.display()))
}

/// The step after the first probe.
pub fn after_probe(probe: Result<SchemaState, Error>, db: &Path) -> LaunchStep {
    match probe {
        Ok(SchemaState::Current) => LaunchStep::Open,
        Ok(SchemaState::Missing | SchemaState::Older(_)) => LaunchStep::Migrate,
        Ok(SchemaState::Newer(version)) => LaunchStep::Show(LaunchPhase::TooNew(version)),
        Err(error) => LaunchStep::Show(cannot_open(db, &error)),
    }
}

/// The step after migrating and probing again: only a current schema opens.
pub fn after_migration(outcome: Result<SchemaState, Error>, db: &Path) -> LaunchStep {
    match outcome {
        Ok(SchemaState::Current) => LaunchStep::Open,
        Ok(_) => LaunchStep::Show(LaunchPhase::Failed(NOT_MIGRATED.to_string())),
        Err(error) => LaunchStep::Show(cannot_open(db, &error)),
    }
}

/// The database to use: `RESSAC_DB` when not empty, else the stored setting (`~` and `~/…`
/// expanded against `home`; any other spelling as given), else the default location.
/// Without a home, only an explicit path works.
pub fn database_location(
    env_db: Option<&OsStr>,
    stored: Option<&str>,
    home: Option<&Path>,
    default_db: Option<&Path>,
) -> Result<PathBuf, PathsError> {
    if let Some(path) = env_db.filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(stored) = stored.map(str::trim).filter(|path| !path.is_empty()) {
        return match stored.strip_prefix('~') {
            Some("") => home.map(Path::to_path_buf).ok_or(PathsError::NoHome),
            Some(rest) if rest.starts_with('/') => {
                let home = home.ok_or(PathsError::NoHome)?;
                Ok(home.join(rest.trim_start_matches('/')))
            }
            _ => Ok(PathBuf::from(stored)),
        };
    }
    default_db.map(Path::to_path_buf).ok_or(PathsError::NoHome)
}

/// Why a database chosen in Settings ▸ Data is refused, if it is: newer, or unreadable.
pub fn location_problem(probe: &Result<SchemaState, Error>) -> Option<&'static str> {
    match probe {
        Ok(SchemaState::Newer(_)) => Some(NEWER_DATABASE),
        Err(_) => Some(NOT_A_DATABASE),
        Ok(_) => None,
    }
}

/// The command line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    /// The hidden `--screenshot <dir>`: walk the app and save PNGs there.
    pub screenshot: Option<PathBuf>,
}

/// Reads the arguments after the program name. A screenshot run needs `RESSAC_DB` so it
/// can never walk the real database. The error is the message for stderr (exit 2).
pub fn parse_args(
    args: impl IntoIterator<Item = OsString>,
    ressac_db: Option<&OsStr>,
) -> Result<Options, String> {
    let needs_directory = || "ressac-kde: --screenshot needs a directory".to_string();
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let value = if arg == "--screenshot" {
            Some(args.next().ok_or_else(needs_directory)?)
        } else {
            arg.to_str()
                .and_then(|arg| arg.strip_prefix("--screenshot="))
                .map(OsString::from)
        };
        match value {
            Some(dir) if options.screenshot.is_none() => {
                if dir.is_empty() {
                    return Err(needs_directory());
                }
                options.screenshot = Some(PathBuf::from(dir));
            }
            _ => {
                return Err(format!(
                    "ressac-kde: unknown argument {}",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    if options.screenshot.is_some() && ressac_db.is_none_or(OsStr::is_empty) {
        return Err(SCREENSHOT_NEEDS_DB.to_string());
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::store::Store;

    fn invalid(s: &str) -> Error {
        Error::Invalid(s.to_string())
    }
    const DB: &str = "/d/r.sqlite";

    #[test]
    fn the_database_location_follows_ressac_db_then_the_setting_then_the_default() {
        let home = Path::new("/home/u");
        let default = PathBuf::from("/home/u/.local/share/ressac/ressac.sqlite");
        let at = |env: Option<&str>, stored: Option<&str>| {
            database_location(env.map(OsStr::new), stored, Some(home), Some(&default)).unwrap()
        };
        assert_eq!(at(None, None), default);
        assert_eq!(at(None, Some("  ")), default);
        assert_eq!(
            at(None, Some("~/Feeds/other.sqlite")),
            PathBuf::from("/home/u/Feeds/other.sqlite")
        );
        assert_eq!(at(None, Some("~")), PathBuf::from("/home/u"));
        assert_eq!(
            at(None, Some("~bob/r.sqlite")),
            PathBuf::from("~bob/r.sqlite")
        );
        assert_eq!(
            at(None, Some("/data/r.sqlite")),
            PathBuf::from("/data/r.sqlite")
        );
        assert_eq!(
            at(Some("/tmp/copy.sqlite"), Some("~/Feeds/other.sqlite")),
            PathBuf::from("/tmp/copy.sqlite")
        );
        assert_eq!(
            at(Some(""), Some("~/Feeds/other.sqlite")),
            PathBuf::from("/home/u/Feeds/other.sqlite")
        );
        // No HOME: only an explicit path works, as in the CLI.
        assert_eq!(
            database_location(Some(OsStr::new("/tmp/c.sqlite")), None, None, None).unwrap(),
            PathBuf::from("/tmp/c.sqlite")
        );
        assert_eq!(
            database_location(None, Some("/abs/r.sqlite"), None, None).unwrap(),
            PathBuf::from("/abs/r.sqlite")
        );
        assert_eq!(
            database_location(None, Some("~/r.sqlite"), None, None),
            Err(ressac_core::paths::PathsError::NoHome)
        );
        assert_eq!(
            database_location(None, None, None, None),
            Err(ressac_core::paths::PathsError::NoHome)
        );
    }

    #[test]
    fn each_probe_result_leads_to_its_step() {
        let db = Path::new(DB);
        assert_eq!(after_probe(Ok(SchemaState::Current), db), LaunchStep::Open);
        for state in [
            SchemaState::Missing,
            SchemaState::Older(0),
            SchemaState::Older(3),
        ] {
            assert_eq!(after_probe(Ok(state), db), LaunchStep::Migrate, "{state:?}");
        }
        assert_eq!(
            after_probe(Ok(SchemaState::Newer(7)), db),
            LaunchStep::Show(LaunchPhase::TooNew(7))
        );
        assert_eq!(
            after_probe(Err(invalid("file is not a database")), db),
            LaunchStep::Show(LaunchPhase::Failed(
                "Cannot open /d/r.sqlite: file is not a database".into()
            ))
        );
    }

    #[test]
    fn a_migration_must_end_current() {
        let db = Path::new(DB);
        assert_eq!(
            after_migration(Ok(SchemaState::Current), db),
            LaunchStep::Open
        );
        for state in [
            SchemaState::Missing,
            SchemaState::Older(5),
            SchemaState::Newer(9),
        ] {
            assert_eq!(
                after_migration(Ok(state), db),
                LaunchStep::Show(LaunchPhase::Failed(NOT_MIGRATED.into())),
                "{state:?}"
            );
        }
        assert_eq!(
            after_migration(Err(invalid("disk full")), db),
            LaunchStep::Show(LaunchPhase::Failed(
                "Cannot open /d/r.sqlite: disk full".into()
            ))
        );
        assert_eq!(
            cannot_open(db, &invalid("locked")),
            LaunchPhase::Failed("Cannot open /d/r.sqlite: locked".into())
        );
    }

    #[test]
    fn phases_show_the_mac_app_s_sentences() {
        assert_eq!(LaunchPhase::Starting.message(), None);
        assert_eq!(
            LaunchPhase::Migrating.message().as_deref(),
            Some("Updating the database…")
        );
        assert_eq!(
            LaunchPhase::TooNew(7).message().as_deref(),
            Some("This database was created by a newer Ressac")
        );
        assert_eq!(
            LaunchPhase::Failed("x".into()).message().as_deref(),
            Some("x")
        );
        assert_eq!(LaunchPhase::Ready.message(), None);
        let offers: Vec<bool> = [
            LaunchPhase::Starting,
            LaunchPhase::Migrating,
            LaunchPhase::TooNew(7),
            LaunchPhase::Failed("x".into()),
            LaunchPhase::Ready,
        ]
        .iter()
        .map(LaunchPhase::offers_default_database)
        .collect();
        assert_eq!(offers, [false, false, true, true, false]);
    }

    #[test]
    fn a_chosen_database_is_refused_only_when_newer_or_unreadable() {
        for state in [
            SchemaState::Missing,
            SchemaState::Current,
            SchemaState::Older(2),
        ] {
            assert_eq!(location_problem(&Ok(state)), None);
        }
        assert_eq!(
            location_problem(&Ok(SchemaState::Newer(9))),
            Some("This database was created by a newer Ressac.")
        );
        assert_eq!(
            location_problem(&Err(invalid("x"))),
            Some("This file is not a usable Ressac database.")
        );
    }

    #[test]
    fn probing_real_files_drives_the_launch() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sub/ressac.sqlite");
        assert_eq!(after_probe(Store::probe(&db), &db), LaunchStep::Migrate);
        drop(Store::open(&db).unwrap());
        assert_eq!(after_probe(Store::probe(&db), &db), LaunchStep::Open);
        let text = dir.path().join("notes.txt");
        std::fs::write(
            &text,
            "not a database, just words long enough to fill a page header",
        )
        .unwrap();
        match after_probe(Store::probe(&text), &text) {
            LaunchStep::Show(LaunchPhase::Failed(m)) => assert!(
                m.starts_with(&format!("Cannot open {}: ", text.display())),
                "{m}"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_command_line_takes_only_a_hidden_screenshot_flag() {
        let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
        let db = Some(OsStr::new("/tmp/copy.sqlite"));
        assert_eq!(parse_args(args(&[]), None), Ok(Options::default()));
        let shots = Options {
            screenshot: Some(PathBuf::from("/tmp/s")),
        };
        assert_eq!(
            parse_args(args(&["--screenshot", "/tmp/s"]), db),
            Ok(shots.clone())
        );
        assert_eq!(parse_args(args(&["--screenshot=/tmp/s"]), db), Ok(shots));
        assert_eq!(
            parse_args(args(&["--screenshot"]), db),
            Err("ressac-kde: --screenshot needs a directory".into())
        );
        for none in [None, Some(OsStr::new(""))] {
            assert_eq!(
                parse_args(args(&["--screenshot", "/tmp/s"]), none),
                Err(SCREENSHOT_NEEDS_DB.into())
            );
        }
        assert_eq!(
            parse_args(args(&["--help"]), db),
            Err("ressac-kde: unknown argument --help".into())
        );
    }
}
