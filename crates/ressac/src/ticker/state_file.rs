//! The ticker's state on disk (`ticker.json`, next to `terminal.json`) and the lock that
//! serialises the `ressac ticker` processes tmux starts.

use std::ffi::OsString;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::TickerState;

const POLL: Duration = Duration::from_millis(20);

pub fn default_path() -> PathBuf {
    ressac_core::store::default_path().with_file_name("ticker.json")
}

/// The saved state; a missing or unreadable file means a fresh start.
pub fn load(path: &Path) -> TickerState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes `ticker.json.tmp`, then renames it over the state file, so a reader never sees
/// half a file.
pub fn save(path: &Path, state: &TickerState) -> io::Result<()> {
    let tmp = sibling(path, ".tmp");
    std::fs::write(&tmp, serde_json::to_vec(state)?)?;
    std::fs::rename(&tmp, path)
}

/// Takes `ticker.json.lock`, polling every 20 ms for at most `wait`. The lock lasts as long as
/// the returned file. `Ok(None)` when someone else still holds it after the wait; `Err` when
/// the lock file cannot be opened or created, e.g. the directory is missing (this never
/// creates it).
pub fn lock(path: &Path, wait: Duration) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(sibling(path, ".lock"))?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(POLL);
            }
            Err(_) => return Ok(None),
        }
    }
}

/// `path` with `suffix` appended to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TickerState {
        TickerState {
            shown: vec![3, 5],
            last_feed: Some(2),
            current: Some(5),
            slot: Some(1_234),
        }
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticker.json");
        save(&path, &state()).unwrap();
        assert_eq!(load(&path), state());
        assert!(!dir.path().join("ticker.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_means_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticker.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load(&path), TickerState::default());
    }

    #[test]
    fn missing_file_means_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(&dir.path().join("ticker.json")),
            TickerState::default()
        );
    }

    #[test]
    fn lock_is_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticker.json");
        let first = lock(&path, Duration::from_millis(50)).unwrap();
        assert!(first.is_some());
        assert!(lock(&path, Duration::from_millis(50)).unwrap().is_none());
        drop(first);
        assert!(lock(&path, Duration::from_millis(50)).unwrap().is_some());
    }

    #[test]
    fn lock_in_a_missing_directory_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("missing");
        let err = lock(&sub.join("ticker.json"), Duration::from_millis(50)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(!sub.exists());
    }
}
