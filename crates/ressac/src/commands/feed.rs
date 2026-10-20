//! `ressac feed cadence`, `theme` and `move`: a feed's rhythm group, its sea and its folder.

use std::io::Write;
use std::process::ExitCode;

use ressac_core::model::{CadenceGroup, FeedId};
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
                .map_err(refusal_error)?;
            if g.json {
                writeln!(out, "{}", cadence_json(&row))?;
            } else {
                writeln!(out, "{}", cadence_text(&row))?;
            }
        }
        FeedCommand::Theme { feed_id, key } => {
            // Reported under the theme's key, whichever way it was typed.
            let resolved = (key != NO_THEME)
                .then(|| app.store().resolve_theme(&key))
                .transpose()
                .map_err(refusal_error)?;
            let theme = resolved.as_ref().map(|t| t.key.as_str());
            app.store()
                .set_feed_theme(feed_id, theme)
                .map_err(refusal_error)?;
            if g.json {
                writeln!(out, "{}", theme_json(feed_id, theme))?;
            } else {
                writeln!(out, "{}", theme_text(feed_id, theme))?;
            }
        }
        FeedCommand::Move { feed_id, folder } => {
            let target = app.store().resolve_folder(&folder).map_err(refusal_error)?;
            let moved = app
                .store()
                .move_feed(feed_id, target)
                .map_err(refusal_error)?;
            let name = match moved.to {
                Some(id) => app
                    .store()
                    .folders()?
                    .into_iter()
                    .find(|f| f.id == id)
                    .map(|f| f.name),
                None => None,
            };
            if g.json {
                writeln!(out, "{}", crate::output::moved_json(&moved))?;
            } else {
                writeln!(
                    out,
                    "{}",
                    crate::output::moved_text(&moved, name.as_deref())
                )?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The key that clears a feed's theme.
const NO_THEME: &str = "none";

/// `#12 theme ai-labs` or `#12 no theme`.
fn theme_text(feed_id: FeedId, theme: Option<&str>) -> String {
    match theme {
        Some(key) => format!("#{feed_id} theme {}", crate::output::printable(key)),
        None => format!("#{feed_id} no theme"),
    }
}

/// The `theme` event: the feed and its theme key, null when cleared.
fn theme_json(feed_id: FeedId, theme: Option<&str>) -> serde_json::Value {
    serde_json::json!({"event": "theme", "feed_id": feed_id, "theme": theme})
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

/// An unknown or buried feed, folder or theme is a usage error (exit 2); anything else
/// stays fatal (exit 1).
fn refusal_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    match e {
        Error::UnknownFeed(_)
        | Error::FeedBuried(_)
        | Error::UnknownFolder(_)
        | Error::UnknownFolderName(_)
        | Error::AmbiguousFolder { .. }
        | Error::UnknownTheme { .. } => {
            usage(crate::output::printable(&e.to_string()).into_owned())
        }
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
    fn theme_lines_name_the_key_or_say_none() {
        assert_eq!(theme_text(12, Some("labs")), "#12 theme labs");
        assert_eq!(theme_text(12, None), "#12 no theme");
        assert_eq!(
            theme_json(12, None),
            json!({"event": "theme", "feed_id": 12, "theme": null})
        );
    }

    #[test]
    fn theme_text_neutralises_controls() {
        assert_eq!(theme_text(1, Some("a\u{1b}[31m")), "#1 theme a\u{FFFD}[31m");
    }

    #[test]
    fn auto_clears_and_the_others_force() {
        assert_eq!(CadenceArg::Auto.group(), None);
        assert_eq!(CadenceArg::Rare.group(), Some(CadenceGroup::Rare));
        assert_eq!(CadenceArg::Silent.group(), Some(CadenceGroup::Silent));
    }

    #[test]
    fn refusals_are_usage_errors_and_others_stay_fatal() {
        use ressac_core::Error;
        for refusal in [
            Error::UnknownFeed(99),
            Error::FeedBuried(2),
            Error::UnknownTheme {
                given: "x".into(),
                keys: vec![],
            },
        ] {
            let text = refusal.to_string();
            let e = refusal_error(refusal);
            assert_eq!(
                e.downcast_ref::<crate::commands::UsageError>()
                    .map(ToString::to_string),
                Some(text)
            );
        }
        let fatal = refusal_error(Error::Invalid("disk".into()));
        assert!(
            fatal
                .downcast_ref::<crate::commands::UsageError>()
                .is_none()
        );
    }
}
