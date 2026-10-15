//! `ressac garden`: the full-screen garden, or one decision without it.

use std::io::Write;
use std::process::ExitCode;

use chrono::Utc;
use ressac_core::garden;
use ressac_core::model::{Decision, FeedId};
use ressac_core::service::{Event, Ressac};
use ressac_core::store::Store;

use super::{Globals, open_with_feeds, usage};
use crate::cli::GardenCommand;
use crate::{output, tui};

/// `ressac garden` alone opens the TUI; with a subcommand it prints one result line.
pub fn garden(
    g: &Globals,
    action: Option<GardenCommand>,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        None => tui::run(&app, g.ascii)?,
        Some(action) => garden_command(&app, action, g.json, out)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Refuses, as usage errors, the decisions the garden cannot apply: an unknown feed, a
/// buried feed (only keep, which restores it, applies), or a repair with no proposed repair.
fn check_decision(store: &Store, feed_id: FeedId, decision: Decision) -> anyhow::Result<()> {
    let Some(feed) = store.feeds()?.into_iter().find(|f| f.id == feed_id) else {
        return Err(usage(format!("No feed #{feed_id}.")));
    };
    if feed.state == "buried" && decision != Decision::Keep {
        return Err(usage(format!(
            "Feed #{feed_id} is buried; only keep restores it."
        )));
    }
    let repairable = store
        .diagnosis(feed_id)?
        .and_then(|d| d.action)
        .is_some_and(|a| a.is_repair());
    if decision == Decision::Repair && !repairable {
        return Err(usage(format!("Nothing to repair for feed #{feed_id}.")));
    }
    Ok(())
}

/// `ressac garden decide | undo | apply-safe`: the TUI's keys, one result line each.
fn garden_command(
    app: &Ressac,
    action: GardenCommand,
    json: bool,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let store = app.store();
    match action {
        GardenCommand::Decide { feed_id, decision } => {
            let decision = Decision::from(decision);
            check_decision(store, feed_id, decision)?;
            garden::decide(store, feed_id, decision, Utc::now())?;
            let event = Event::Decided {
                feed_id,
                decision: decision.as_str().to_string(),
            };
            output::write_result(out, json, &event, &output::decided_text(feed_id, decision))?;
        }
        GardenCommand::Undo => {
            let feed_id = garden::undo(store)?;
            output::write_result(
                out,
                json,
                &Event::Undone { feed_id },
                &output::undo_text(feed_id),
            )?;
        }
        GardenCommand::ApplySafe => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            let event = Event::Finished {
                summary: app.summary()?,
            };
            output::write_result(
                out,
                json,
                &event,
                &output::apply_safe_text(applied, &skipped),
            )?;
        }
    }
    Ok(())
}
