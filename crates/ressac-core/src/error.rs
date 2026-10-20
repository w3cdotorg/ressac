//! Crate-wide error type. Per-feed fetch failures are data (`model::ErrorKind`), not errors.

use std::path::PathBuf;

use crate::model::{FeedId, FolderId};

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
    #[error("No folder #{0}.")]
    UnknownFolder(FolderId),
    /// `given` matched no theme's key or name; `keys` are the valid keys, in position order.
    #[error("No theme \"{given}\". {}", themes_hint(.keys))]
    UnknownTheme { given: String, keys: Vec<String> },
    #[error("Feed #{0} is buried.")]
    FeedBuried(FeedId),
    /// Phase 2b's refusal; declared here because Phase 2b's five files are taken.
    #[error("An edition was already built this morning.")]
    EditionNotDue,
    /// The address typed to add a feed is not a web address.
    #[error("Enter a web address.")]
    InvalidAddress,
    /// The address (or the first feed a page announces) is not a feed, and the page
    /// announces none. The payload is the address that was tried, for the log.
    #[error("This page announces no feed.")]
    NoFeedFound(String),
    /// The feed is already followed, or buried in the Cemetery.
    #[error("{}", feed_exists_sentence(*.buried, .title))]
    FeedExists {
        id: FeedId,
        title: String,
        buried: bool,
    },
    /// An address to add could not be fetched (network, HTTP status).
    #[error("Could not reach {url}: {reason}.")]
    Unreachable { url: String, reason: String },
    /// An image the terminal reader asked for and will show as a placeholder (spec §7).
    #[error("Image refused: {0}")]
    ImageRefused(String),
}

fn feed_exists_sentence(buried: bool, title: &str) -> String {
    if buried {
        format!("Already in the Cemetery: {title}.")
    } else {
        format!("Already followed: {title}.")
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The tail of the unknown-theme message: the valid keys, or how to get some.
fn themes_hint(keys: &[String]) -> String {
    if keys.is_empty() {
        "No themes yet: import them with ressac chart import.".to_owned()
    } else {
        format!("Themes: {}", keys.join(", "))
    }
}
