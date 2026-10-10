//! Crate-wide error type. Per-feed fetch failures are data (`model::ErrorKind`), not errors.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema v{found} is newer than this ressac (knows v{known}); update ressac")]
    SchemaTooNew { found: i64, known: i64 },
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
}

pub type Result<T> = std::result::Result<T, Error>;
