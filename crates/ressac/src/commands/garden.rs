//! `ressac garden`: the full-screen garden, or one decision without it.

use std::io::Write;
use std::process::ExitCode;

use chrono::Utc;
use ressac_core::garden;
use ressac_core::model::Decision;
use ressac_core::service::{Event, Ressac};

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

/// The refusals of `garden::decide` and `garden::undo` (an address another active feed
/// already uses, a merge target that is not active) and those of `garden::check_decision`
/// (an unknown feed joins them) are usage errors (exit 2); any other error stays fatal
/// (exit 1).
fn decision_error(e: ressac_core::Error) -> anyhow::Error {
    match e {
        ressac_core::Error::Invalid(message) => usage(message),
        e @ ressac_core::Error::UnknownFeed(_) => usage(e.to_string()),
        other => other.into(),
    }
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
            garden::check_decision(store, feed_id, decision).map_err(decision_error)?;
            garden::decide(store, feed_id, decision, Utc::now()).map_err(decision_error)?;
            let event = Event::Decided {
                feed_id,
                decision: decision.as_str().to_string(),
            };
            output::write_result(out, json, &event, &output::decided_text(feed_id, decision))?;
        }
        GardenCommand::Undo => {
            let feed_id = garden::undo(store).map_err(decision_error)?;
            output::write_result(
                out,
                json,
                &Event::Undone { feed_id },
                &output::undo_text(feed_id),
            )?;
        }
        GardenCommand::ApplySafe => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            if json {
                // The `finished` event, plus what this command did: the app reads the
                // summary and ignores the extra keys.
                let mut line = serde_json::to_value(Event::Finished {
                    summary: app.summary()?,
                })?;
                if let Some(object) = line.as_object_mut() {
                    object.insert("applied".into(), applied.into());
                    object.insert("skipped".into(), skipped.into());
                }
                writeln!(out, "{line}")?;
            } else {
                writeln!(out, "{}", output::apply_safe_text(applied, &skipped))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::UsageError;

    #[test]
    fn decision_refusals_are_usage_errors_and_others_stay_fatal() {
        let text = "Feed #2 already uses https://e.org/feed; merge into it instead.";
        let refusal = decision_error(ressac_core::Error::Invalid(text.into()));
        assert_eq!(
            refusal
                .downcast_ref::<UsageError>()
                .map(ToString::to_string),
            Some(text.to_string())
        );
        let fatal = decision_error(ressac_core::Error::Io(std::io::Error::other("disk")));
        assert!(fatal.downcast_ref::<UsageError>().is_none());
        let unknown = decision_error(ressac_core::Error::UnknownFeed(99));
        assert_eq!(
            unknown
                .downcast_ref::<UsageError>()
                .map(ToString::to_string),
            Some("No feed #99.".to_string())
        );
    }
}
