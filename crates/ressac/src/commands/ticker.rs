//! `ressac ticker` and `ressac ticker open`: the tmux status-bar ticker.
//!
//! tmux runs both every few seconds or on a click, and shows whatever they print: they exit 0
//! in every case, print nothing but the one line, and log failures instead of reporting them.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use ressac_core::store::{Store, TickerEntry};

use super::Globals;
use crate::ticker::{self, Rng, state_file};

/// How long a call waits for another one to finish before reprinting what is on screen.
const LOCK_WAIT: Duration = Duration::from_millis(200);

/// Prints the entry to show now, or an empty line.
pub fn show(
    g: &Globals,
    state_path: &Path,
    tmux: bool,
    width: usize,
    now: DateTime<Utc>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let text = pick(g, state_path, tmux, width, now).unwrap_or_else(|e| {
        tracing::warn!("ticker: {e:#}");
        String::new()
    });
    writeln!(out, "{text}")?;
    Ok(ExitCode::SUCCESS)
}

fn pick(
    g: &Globals,
    state_path: &Path,
    tmux: bool,
    width: usize,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let guard = state_file::lock(state_path, LOCK_WAIT);
    let store = Store::open_existing_query_only(&g.db)?;
    let pool = store.ticker_pool(now)?;
    let state = state_file::load(state_path);

    if guard.is_none() {
        // Another call is drawing: show what is on screen, and leave the state to it.
        tracing::warn!("ticker: state lock not taken, reprinting the current entry");
        let current = state
            .current
            .and_then(|id| pool.iter().find(|e| e.entry_id == id));
        return Ok(render(current, width, tmux));
    }

    let seed = now.timestamp_nanos_opt().unwrap_or_default() as u64;
    let (entry, state) = ticker::next(&pool, &state, ticker::slot(now), &mut Rng::new(seed));
    if let Err(e) = state_file::save(state_path, &state) {
        tracing::warn!("ticker: cannot save {}: {e}", state_path.display());
    }
    Ok(render(entry.as_ref(), width, tmux))
}

fn render(entry: Option<&TickerEntry>, width: usize, tmux: bool) -> String {
    entry
        .map(|e| ticker::line(e, width, tmux))
        .unwrap_or_default()
}

/// Marks the clicked entry read, then hands its URL to `opener`. Prints nothing.
pub fn open(
    g: &Globals,
    token: &str,
    now: DateTime<Utc>,
    opener: impl FnOnce(&str) -> std::io::Result<()>,
) -> anyhow::Result<ExitCode> {
    if let Err(e) = open_entry(g, token, now, opener) {
        tracing::warn!("ticker open {token}: {e:#}");
    }
    Ok(ExitCode::SUCCESS)
}

fn open_entry(
    g: &Globals,
    token: &str,
    now: DateTime<Utc>,
    opener: impl FnOnce(&str) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    let id = ticker::parse_token(token).context("not a ticker token")?;
    let store = Store::open_existing(&g.db)?;
    let entry = store
        .ticker_entry(id)?
        .with_context(|| format!("entry {id} cannot be opened"))?;
    store.set_read(id, true, now)?;
    opener(&entry.url).with_context(|| format!("cannot open {}", entry.url))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::*;

    const NOW: &str = "2026-10-19T12:00:00Z";

    struct Fixture {
        dir: TempDir,
        g: Globals,
        state: PathBuf,
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// Two feeds, three eligible entries with URLs.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ressac.sqlite");
        drop(Store::open(&db).unwrap());
        let conn = Connection::open(&db).unwrap();
        for id in [1, 2] {
            conn.execute_batch(&format!(
                    "INSERT INTO feed (id, position, title, xml_url, xml_url_original, url_key, state, added_at) \
                     VALUES ({id}, {id}, 'Feed {id}', 'https://f{id}.example.org/feed', \
                     'https://f{id}.example.org/feed', 'https://f{id}.example.org/feed', 'active', \
                     '2026-01-01T00:00:00.000Z')"
                ))
                .unwrap();
        }
        for (id, feed) in [(1, 1), (2, 1), (3, 2)] {
            conn.execute_batch(&format!(
                "INSERT INTO entry (id, feed_id, guid, title, url, published_at, fetched_at) \
                     VALUES ({id}, {feed}, 'g{id}', 'Title {id}', 'https://example.org/{id}', \
                     '2026-10-18T08:00:00.000Z', '2026-10-18T08:00:00.000Z')"
            ))
            .unwrap();
        }
        drop(conn);
        let g = Globals {
            db,
            json: false,
            ascii: false,
            stop: Arc::new(AtomicBool::new(false)),
        };
        let state = dir.path().join("ticker.json");
        Fixture { dir, g, state }
    }

    fn show_at(f: &Fixture, now: &str) -> String {
        let mut out = Vec::new();
        let code = show(&f.g, &f.state, true, 70, at(now), &mut out).unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn two_calls_in_the_same_slot_print_the_same_line() {
        let f = fixture();
        let first = show_at(&f, "2026-10-19T12:00:01Z");
        let second = show_at(&f, "2026-10-19T12:00:09Z");
        assert!(first.contains("#[range=user|rs"), "{first}");
        assert_eq!(first, second);
    }

    #[test]
    fn a_call_in_the_next_slot_prints_another_feed() {
        let f = fixture();
        let first = show_at(&f, "2026-10-19T12:00:01Z");
        let second = show_at(&f, "2026-10-19T12:00:16Z");
        let feed = |line: &str| line.contains("Feed 1");
        assert_ne!(feed(&first), feed(&second), "{first} / {second}");
    }

    #[test]
    fn ticker_reprints_current_when_the_lock_is_held() {
        let f = fixture();
        let first = show_at(&f, "2026-10-19T12:00:01Z");
        let saved = std::fs::read(&f.state).unwrap();
        let _held = state_file::lock(&f.state, Duration::from_millis(50)).unwrap();
        let started = std::time::Instant::now();
        let second = show_at(&f, "2026-10-19T12:00:40Z");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(first, second);
        assert_eq!(std::fs::read(&f.state).unwrap(), saved);
    }

    #[test]
    fn ticker_prints_an_empty_line_on_a_database_error() {
        let f = fixture();
        set_schema_version_3(&f);
        assert_eq!(show_at(&f, NOW), "\n");
        assert!(!f.state.exists());
    }

    #[test]
    fn an_error_leaves_an_existing_state_file_untouched() {
        let f = fixture();
        show_at(&f, "2026-10-19T12:00:01Z");
        let saved = std::fs::read(&f.state).unwrap();
        set_schema_version_3(&f);
        assert_eq!(show_at(&f, "2026-10-19T12:00:20Z"), "\n");
        assert_eq!(std::fs::read(&f.state).unwrap(), saved);
    }

    #[test]
    fn show_in_a_missing_directory_creates_nothing() {
        let mut f = fixture();
        f.state = f.dir.path().join("missing/ticker.json");
        assert_eq!(show_at(&f, NOW), "\n");
        assert!(!f.dir.path().join("missing").exists());
    }

    fn set_schema_version_3(f: &Fixture) {
        Connection::open(&f.g.db)
            .unwrap()
            .execute_batch("UPDATE schema_meta SET version = 3")
            .unwrap();
    }

    fn read_at(f: &Fixture, id: i64) -> Option<String> {
        Connection::open(&f.g.db)
            .unwrap()
            .query_row(
                &format!("SELECT read_at FROM entry WHERE id = {id}"),
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn open_marks_read_then_opens() {
        let f = fixture();
        let opened = RefCell::new(Vec::new());
        let code = open(&f.g, "rs2", at(NOW), |url| {
            opened.borrow_mut().push(url.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        assert_eq!(*opened.borrow(), ["https://example.org/2"]);
        assert!(read_at(&f, 2).is_some());
        assert!(read_at(&f, 1).is_none());
    }

    #[test]
    fn open_with_a_bad_token_changes_nothing() {
        let f = fixture();
        let called = RefCell::new(false);
        let code = open(&f.g, "rsx", at(NOW), |_| {
            *called.borrow_mut() = true;
            Ok(())
        })
        .unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(!*called.borrow());
        assert!(read_at(&f, 1).is_none());
    }

    #[test]
    fn open_still_succeeds_when_the_browser_fails() {
        let f = fixture();
        let code = open(&f.g, "rs1", at(NOW), |_| {
            Err(std::io::Error::other("no browser"))
        })
        .unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(read_at(&f, 1).is_some());
    }

    #[test]
    fn open_on_a_missing_database_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("missing/ressac.sqlite");
        let g = Globals {
            db,
            json: false,
            ascii: false,
            stop: Arc::new(AtomicBool::new(false)),
        };
        let code = open(&g, "rs1", at(NOW), |_| panic!("opened")).unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(!dir.path().join("missing").exists());
    }
}
