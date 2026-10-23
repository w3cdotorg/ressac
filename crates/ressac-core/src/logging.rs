//! Logging set-up shared by every binary: both call [`init`] once at start.
//!
//! A daily-rotated log file in the state directory (`Paths::log_dir`: `~/Library/Logs/Ressac`
//! on macOS, `$XDG_STATE_HOME/ressac/logs` on Linux), mirrored to stderr with `verbose`.

use std::path::Path;

pub use tracing_appender::non_blocking::WorkerGuard;

/// Installs the global subscriber, logging to `dir`; keep the returned guard alive for the
/// whole run (dropping it flushes the file). `None` when the directory cannot be created or
/// written, or a subscriber is already installed: logging is then off, never fatal.
pub fn init(dir: &Path, verbose: bool) -> Option<WorkerGuard> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    std::fs::create_dir_all(dir).ok()?;
    // `rolling::daily` panics when the directory is not writable; a status-bar helper must
    // never print that, so no file logging in that case.
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ressac.log")
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let file_layer = fmt::layer().with_writer(writer).with_ansi(false);
    let stderr_layer = verbose.then(|| fmt::layer().with_writer(std::io::stderr));
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .ok()?;
    Some(guard)
}
