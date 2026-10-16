//! `ressac feed cadence`: force a feed's rhythm group, or give it back to the core.

use std::io::Write;
use std::process::ExitCode;

use ressac_core::model::CadenceGroup;
use ressac_core::store::CadenceRow;

use super::{Globals, open_with_feeds, usage};
use crate::cli::FeedCommand;

/// `ressac feed cadence ID GROUP`: one line, or one `cadence` event.
pub fn feed(g: &Globals, action: FeedCommand, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        FeedCommand::Cadence { feed_id, group } => {
            let row = app
                .set_cadence(feed_id, group.group())
                .map_err(cadence_error)?;
            if g.json {
                writeln!(out, "{}", cadence_json(&row))?;
            } else {
                writeln!(out, "{}", cadence_text(&row))?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The sidebar's names for the five groups (spec §8).
pub fn group_title(group: CadenceGroup) -> &'static str {
    match group {
        CadenceGroup::Daily => "Daily",
        CadenceGroup::Weekly => "Weekly",
        CadenceGroup::Monthly => "Monthly",
        CadenceGroup::Rare => "Rare",
        CadenceGroup::Silent => "Silent",
    }
}

/// `#12 Daily (set by you)` or `#12 Weekly (auto)`.
pub fn cadence_text(row: &CadenceRow) -> String {
    let who = if row.is_override {
        "set by you"
    } else {
        "auto"
    };
    format!(
        "#{} {} ({who})",
        row.feed_id,
        group_title(row.effective_group)
    )
}

/// The `cadence` event: the effective group, the computed one, and whether it is forced.
pub fn cadence_json(row: &CadenceRow) -> serde_json::Value {
    serde_json::json!({
        "event": "cadence",
        "feed_id": row.feed_id,
        "group": row.effective_group.as_str(),
        "computed": row.computed_group.as_str(),
        "override": row.is_override,
    })
}

/// An unknown or buried feed is a usage error (exit 2); anything else stays fatal (exit 1).
fn cadence_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    match e {
        Error::UnknownFeed(_) | Error::FeedBuried(_) => usage(e.to_string()),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CadenceArg;
    use serde_json::json;

    fn row(effective: CadenceGroup, computed: CadenceGroup, is_override: bool) -> CadenceRow {
        CadenceRow {
            feed_id: 12,
            effective_group: effective,
            computed_group: computed,
            is_override,
        }
    }

    #[test]
    fn cadence_text_names_the_group_and_who_chose_it() {
        assert_eq!(
            cadence_text(&row(CadenceGroup::Daily, CadenceGroup::Weekly, true)),
            "#12 Daily (set by you)"
        );
        assert_eq!(
            cadence_text(&row(CadenceGroup::Weekly, CadenceGroup::Weekly, false)),
            "#12 Weekly (auto)"
        );
    }

    #[test]
    fn cadence_json_is_the_cadence_event() {
        assert_eq!(
            cadence_json(&row(CadenceGroup::Daily, CadenceGroup::Weekly, true)),
            json!({"event": "cadence", "feed_id": 12, "group": "daily",
                   "computed": "weekly", "override": true})
        );
    }

    #[test]
    fn group_titles_are_the_sidebar_names() {
        let titles: Vec<&str> = CadenceGroup::ALL.iter().map(|g| group_title(*g)).collect();
        assert_eq!(titles, ["Daily", "Weekly", "Monthly", "Rare", "Silent"]);
    }

    #[test]
    fn auto_clears_and_the_others_force() {
        assert_eq!(CadenceArg::Auto.group(), None);
        assert_eq!(CadenceArg::Rare.group(), Some(CadenceGroup::Rare));
        assert_eq!(CadenceArg::Silent.group(), Some(CadenceGroup::Silent));
    }

    #[test]
    fn cadence_refusals_are_usage_errors_and_others_stay_fatal() {
        use ressac_core::Error;
        for refusal in [Error::UnknownFeed(99), Error::FeedBuried(2)] {
            let text = refusal.to_string();
            let e = cadence_error(refusal);
            assert_eq!(
                e.downcast_ref::<crate::commands::UsageError>()
                    .map(ToString::to_string),
                Some(text)
            );
        }
        let fatal = cadence_error(Error::Invalid("disk".into()));
        assert!(
            fatal
                .downcast_ref::<crate::commands::UsageError>()
                .is_none()
        );
    }
}
