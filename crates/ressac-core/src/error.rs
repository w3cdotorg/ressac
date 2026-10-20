//! Crate-wide error type. Per-feed fetch failures are data (`model::ErrorKind`), not errors.

use std::path::PathBuf;

use crate::model::FeedId;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema v{found} is newer than this ressac (knows v{known}); update ressac")]
    SchemaTooNew { found: i64, known: i64 },
    /// `Store::open_existing*` found no file to open (it never creates one).
    #[error("No database at {}.", .0.display())]
    DatabaseMissing(PathBuf),
    /// `Store::open_existing*` found a schema it will neither migrate nor read.
    #[error("Database schema v{found} does not match this ressac (v{expected}).")]
    SchemaMismatch { found: i64, expected: i64 },
    #[error("cannot read OPML at line {line}, column {column}: {message}")]
    Opml {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("Page {0} is reserved.")]
    PageReserved(u16),
    #[error("Page {0} is not between 110 and 799.")]
    PageOutOfRange(u16),
    #[error("Page {page} is already taken.")]
    PageTaken { page: u16 },
    #[error("No {kind} #{id}.")]
    UnknownPageTarget { kind: &'static str, id: i64 },
    #[error("No feed #{0}.")]
    UnknownFeed(FeedId),
    #[error("No theme \"{0}\".")]
    UnknownTheme(String),
    #[error("Feed #{0} is buried.")]
    FeedBuried(FeedId),
    /// Phase 2b's refusal; declared here because Phase 2b's five files are taken.
    #[error("An edition was already built this morning.")]
    EditionNotDue,
    /// An image the terminal reader asked for and will show as a placeholder (spec §7).
    #[error("Image refused: {0}")]
    ImageRefused(String),
}

pub type Result<T> = std::result::Result<T, Error>;
