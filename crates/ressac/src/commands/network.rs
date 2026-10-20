//! The commands that go to the network: `fetch`, `check` and `extract`.

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use ressac_core::model::{FeedId, FolderId};
use ressac_core::service::{ExtractOutcome, NullSink, RefreshOptions};
use ressac_core::store::Store;

use super::{Globals, INTERRUPTED, open, open_with_feeds, usage};
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

/// An address as the log may show it: scheme, host (port) and path. The userinfo, the query and the
/// fragment can carry a secret (`user:pass@host`, `?token=...`), so they are dropped, and `?...`
/// marks a query that was there. Text that is not an http(s) address is cut the same way by hand.
fn redact_address(address: &str) -> String {
    let rest_start = address.find("://").map_or(0, |i| i + 3);
    let (head, rest) = address.split_at(rest_start);
    // Userinfo first: up to the last `@` of the authority, even past a `?` or `#` in a password.
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let rest = match rest[..authority_end].rfind('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let marker = if rest[end..].starts_with('?') {
        "?..."
    } else {
        ""
    };
    format!("{head}{}{marker}", &rest[..end])
}

/// The refusals of `add` (nothing to follow, already followed, unknown theme, private or
/// malformed address) are usage errors (exit 2); a network failure stays fatal (exit 1).
/// Their texts carry feed- or user-supplied strings, so every one goes through `printable`.
fn add_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    if let Error::NoFeedFound(tried) = &e {
        tracing::warn!(address = %redact_address(tried), "no feed found");
    }
    let text = output::printable(&e.to_string()).into_owned();
    match e {
        Error::InvalidAddress
        | Error::NoFeedFound(_)
        | Error::FeedExists { .. }
        | Error::UnknownTheme(_)
        | Error::UnknownFolder(_)
        | Error::UnknownFeed(_)
        | Error::Invalid(_) => usage(text),
        Error::Unreachable { .. } => anyhow::anyhow!(text),
        other => other.into(),
    }
}

/// `ressac add`: one feed, fetched right away. Progress as `fetch` shows it; with `--json`
/// the stream ends with an `added` event.
pub async fn add(
    g: &Globals,
    address: &str,
    theme: Option<&str>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let app = open(&g.db)?;
    let sink = output::sink(g.json);
    let result = app.add_feed(address, theme, &sink, g.stop.clone()).await;
    sink.finish();
    let added = result.map_err(add_error)?;
    if g.json {
        writeln!(out, "{}", output::added_json(&added))?;
    } else {
        writeln!(out, "{}", output::added_text(&added))?;
    }
    Ok(ExitCode::SUCCESS)
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

#[cfg(test)]
mod tests {
    #[test]
    fn redact_address_drops_userinfo_query_and_fragment() {
        use super::redact_address;
        assert_eq!(
            redact_address("https://user:pass@host.example:8080/feed.xml?token=abc#top"),
            "https://host.example:8080/feed.xml?..."
        );
        assert_eq!(
            redact_address("https://u:p@host.example?token=abc"),
            "https://host.example?..."
        );
        assert_eq!(
            redact_address("https://host.example/a/b.xml"),
            "https://host.example/a/b.xml"
        );
        assert_eq!(
            redact_address("host.example/feed#frag"),
            "host.example/feed"
        );
        assert_eq!(
            redact_address("u:p@host.example/feed?k=v"),
            "host.example/feed?..."
        );
        assert_eq!(
            redact_address("https://u:pa?ss@host.example/f"),
            "https://host.example/f"
        );
        assert_eq!(redact_address("u:pa?ss@host/f"), "host/f");
    }

    use super::*;
    use crate::commands::UsageError;
    use ressac_core::Error;

    #[test]
    fn add_refusals_are_usage_errors_and_a_failed_reach_stays_fatal() {
        for refusal in [
            Error::InvalidAddress,
            Error::NoFeedFound("https://e.org/".into()),
            Error::FeedExists {
                id: 1,
                title: "T".into(),
                buried: true,
            },
            Error::UnknownTheme("x".into()),
            Error::UnknownFolder(9),
            Error::UnknownFeed(9),
            Error::Invalid("Ressac only follows public web addresses.".into()),
        ] {
            let text = refusal.to_string();
            let e = add_error(refusal);
            assert_eq!(
                e.downcast_ref::<UsageError>().map(ToString::to_string),
                Some(text)
            );
        }
        let down = add_error(Error::Unreachable {
            url: "https://e.org/\u{1b}[31m".into(),
            reason: "timed out".into(),
        });
        assert!(down.downcast_ref::<UsageError>().is_none());
        assert_eq!(
            down.to_string(),
            "Could not reach https://e.org/\u{FFFD}[31m: timed out."
        );
    }
}
