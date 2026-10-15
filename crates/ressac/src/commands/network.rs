//! The commands that go to the network: `fetch`, `check` and `extract`.

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use ressac_core::model::{FeedId, FolderId};
use ressac_core::service::{ExtractOutcome, NullSink, RefreshOptions};
use ressac_core::store::Store;

use super::{Globals, INTERRUPTED, open_with_feeds, usage};
use crate::output;

/// `ressac extract` on an entry without a link (spec Review Focus 1).
const NO_LINK: &str = "This entry has no link.";

/// After a fetch stopped by Ctrl-C: say how much was left undone, exit 130.
fn interrupted(stop: &AtomicBool, sink: &output::Sink) -> ExitCode {
    if stop.load(Ordering::Relaxed) {
        eprintln!("{}", output::interrupted_message(sink.not_fetched()));
        ExitCode::from(INTERRUPTED)
    } else {
        ExitCode::SUCCESS
    }
}

/// The feeds named by `--feed` or `--folder` (`None`: no restriction). Unknown ids are
/// usage errors. Buried feeds stay in the list, and `refresh` skips them.
fn subset(
    store: &Store,
    feeds: Vec<FeedId>,
    folder: Option<FolderId>,
) -> anyhow::Result<Option<Vec<FeedId>>> {
    if let Some(folder) = folder {
        if !store.folders()?.iter().any(|f| f.id == folder) {
            return Err(usage(format!("No folder #{folder}.")));
        }
        return Ok(Some(store.feeds_in_folder(folder)?));
    }
    if feeds.is_empty() {
        return Ok(None);
    }
    let known: HashSet<FeedId> = store.feeds()?.into_iter().map(|f| f.id).collect();
    if let Some(missing) = feeds.iter().find(|id| !known.contains(id)) {
        return Err(usage(format!("No feed #{missing}.")));
    }
    Ok(Some(feeds))
}

/// `ressac fetch`: every due feed, or only those named by `--feed` or `--folder`.
pub async fn fetch(
    g: &Globals,
    force: bool,
    feeds: Vec<FeedId>,
    folder: Option<FolderId>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let only = subset(app.store(), feeds, folder)?;
    // A manual refresh of some feeds ignores the 30-minute freshness rule (spec §2).
    let opts = RefreshOptions {
        force: force || only.is_some(),
        only,
    };
    let sink = output::sink(g.json);
    let result = app.refresh(opts, &sink, g.stop.clone()).await;
    sink.finish();
    let summary = result?;
    if !g.json {
        writeln!(out, "{}", output::summary_text(&summary, g.ascii))?;
    }
    Ok(interrupted(&g.stop, &sink))
}

/// `ressac check`: a refresh, then the Wayback lookups, then the health report.
pub async fn check(g: &Globals, force: bool, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let sink = output::sink(g.json);
    let refreshed = app
        .refresh(RefreshOptions { force, only: None }, &sink, g.stop.clone())
        .await;
    let refreshed = match refreshed {
        Ok(s) => s,
        Err(e) => {
            sink.finish();
            return Err(e.into());
        }
    };
    let looked_up = app.lookup_wayback(&sink, g.stop.clone()).await;
    sink.finish();
    looked_up?;
    let mut summary = app.summary()?;
    summary.network_unreachable |= refreshed.network_unreachable;
    if g.json {
        writeln!(
            out,
            "{}",
            serde_json::json!({ "event": "report", "summary": summary })
        )?;
    } else {
        writeln!(out, "{}", output::summary_text(&summary, g.ascii))?;
    }
    Ok(interrupted(&g.stop, &sink))
}

/// `ressac extract`: one entry's full article. With `--json` the sink prints the
/// `extracted` line; otherwise the text below says it.
pub async fn extract(g: &Globals, entry_id: i64, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let outcome = if g.json {
        app.extract_entry(entry_id, &output::sink(true)).await?
    } else {
        app.extract_entry(entry_id, &NullSink).await?
    };
    let text = match outcome {
        ExtractOutcome::UnknownEntry => {
            return Err(usage(format!("No entry #{entry_id}.")));
        }
        ExtractOutcome::NoUrl => return Err(usage(NO_LINK)),
        ExtractOutcome::Saved => output::extracted_text(entry_id, None),
        ExtractOutcome::Failed(error) => output::extracted_text(entry_id, Some(&error)),
    };
    if !g.json {
        writeln!(out, "{text}")?;
    }
    Ok(ExitCode::SUCCESS)
}
