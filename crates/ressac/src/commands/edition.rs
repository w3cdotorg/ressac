//! `ressac edition build | show`: the morning edition from the command line.

use std::io::Write;
use std::process::ExitCode;

use chrono::{DateTime, Local, TimeZone, Utc};
use ressac_core::edition::{Caps, Section};
use ressac_core::service::{EditionOptions, EditionOutcome, EditionReport, Ressac};

use super::{Globals, INTERRUPTED, open_with_feeds, usage};
use crate::cli::EditionCommand;
use crate::output;

/// `ressac edition build` when every fetch failed for network reasons (spec §9).
const OFFLINE: &str = "The network is unreachable: no edition built.";

/// `ressac edition build | show`.
pub async fn edition(
    g: &Globals,
    action: EditionCommand,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let Some(app) = open_with_feeds(&g.db)? else {
        return Ok(ExitCode::SUCCESS);
    };
    match action {
        EditionCommand::Build {
            cap_week,
            cap_wire,
            rebuild,
        } => build(g, &app, cap_week, cap_wire, rebuild, out).await,
        EditionCommand::Show => show(g, &app, out),
    }
}

/// The refusal of a second build the same morning is a usage error (exit 2), whether the
/// service saw it before the refresh (`NotDue`) or inside the write (`EditionNotDue`).
fn edition_error(e: ressac_core::Error) -> anyhow::Error {
    match e {
        ressac_core::Error::EditionNotDue => usage(e.to_string()),
        other => other.into(),
    }
}

async fn build(
    g: &Globals,
    app: &Ressac,
    cap_week: Option<usize>,
    cap_wire: Option<usize>,
    rebuild: bool,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let defaults = Caps::default();
    let opts = EditionOptions {
        caps: Caps {
            week: cap_week.unwrap_or(defaults.week),
            wire: cap_wire.unwrap_or(defaults.wire),
        },
        rebuild,
    };
    let sink = output::sink(g.json);
    let outcome = app
        .build_edition(opts, Utc::now(), &Local, &sink, g.stop.clone())
        .await;
    sink.finish();
    match outcome.map_err(edition_error)? {
        EditionOutcome::NotDue => Err(edition_error(ressac_core::Error::EditionNotDue)),
        EditionOutcome::Offline => Err(usage(OFFLINE)),
        EditionOutcome::Interrupted => {
            eprintln!("{}", output::interrupted_message(sink.not_fetched()));
            Ok(ExitCode::from(INTERRUPTED))
        }
        EditionOutcome::Built(report) => {
            // With --json the sink already printed the `edition` event.
            if !g.json {
                writeln!(out, "{}", output::summary_text(&app.summary()?, g.ascii))?;
                writeln!(out, "{}", built_text(&report))?;
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// The line after `summary_text` once an edition is written.
pub fn built_text(r: &EditionReport) -> String {
    let mut text = format!(
        "Edition #{}: {} (front {}, week {}, wire {})",
        r.id,
        count(r.entries, "entry", "entries"),
        r.front,
        r.week,
        r.wire,
    );
    match r.overflow {
        0 => text.push('.'),
        1 => text.push_str(", 1 more left in its feed."),
        n => text.push_str(&format!(", {n} more left in their feeds.")),
    }
    if r.unreachable > 0 {
        text.push_str(&format!(
            " {} unreachable.",
            count(r.unreachable as usize, "feed", "feeds")
        ));
    }
    text
}

/// The app's section headings (spec §8), shared by `edition show`.
pub fn section_title(section: Section) -> &'static str {
    match section {
        Section::Front => "Front Page",
        Section::Week => "This Week",
        Section::Wire => "The Wire",
    }
}

/// `rank page title`, with `-` for a feed without a page and `(untitled)` for an entry
/// without a title.
pub fn entry_line(rank: u32, page: Option<u16>, title: Option<&str>) -> String {
    let page = page.map_or("-".to_string(), |p| p.to_string());
    format!("{rank} {page} {}", title.unwrap_or("(untitled)"))
}

pub fn overflow_line(hidden: u32, feed_title: &str) -> String {
    format!("+{hidden} more in {feed_title}")
}

/// The Journal's header (spec §8): `Friday 16 October · 43 entries · 12 read`.
pub fn header_line<Tz: TimeZone>(
    built_at: DateTime<Tz>,
    entries: usize,
    read: usize,
    unreachable: u32,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut text = format!(
        "{} · {} · {read} read",
        built_at.format("%A %-d %B"),
        count(entries, "entry", "entries")
    );
    if unreachable > 0 {
        text.push_str(&format!(
            " · {} unreachable",
            count(unreachable as usize, "feed", "feeds")
        ));
    }
    text
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Printed by `edition show` before the first edition (text only; `--json` prints nothing).
const NO_EDITION: &str = "No edition yet.";

/// Printed by `edition show` instead of the sections when the edition is empty.
const NOTHING_NEW: &str = "Nothing new this morning.";

fn show(g: &Globals, app: &Ressac, out: &mut impl Write) -> anyhow::Result<ExitCode> {
    let Some(view) = app.edition_today()? else {
        if !g.json {
            writeln!(out, "{NO_EDITION}")?;
        }
        return Ok(ExitCode::SUCCESS);
    };
    let ed = &view.edition;
    if g.json {
        for e in &view.entries {
            let row = serde_json::json!({
                "event": "edition_entry",
                "edition_id": ed.id,
                "built_at": ed.built_at,
                "window_start": ed.window_start,
                "window_end": ed.window_end,
                "unreachable": ed.unreachable,
                "entry_id": e.entry_id,
                "section": e.section,
                "rank": e.rank,
                "feed_id": e.feed_id,
                "feed_title": e.feed_title,
                "title": e.title,
                "published_at": e.published_at,
                "read_at": e.read_at,
                "starred_at": e.starred_at,
            });
            writeln!(out, "{row}")?;
        }
        for o in &view.overflow {
            let row = serde_json::json!({
                "event": "edition_overflow",
                "edition_id": ed.id,
                "section": o.section,
                "feed_id": o.feed_id,
                "feed_title": o.feed_title,
                "page": o.page,
                "hidden_count": o.hidden_count,
            });
            writeln!(out, "{row}")?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    let pages: std::collections::HashMap<i64, u16> = app
        .pages()?
        .into_iter()
        .filter(|r| r.kind == ressac_core::store::pages::PageKind::Feed)
        .filter_map(|r| r.id.map(|id| (id, r.page)))
        .collect();
    if view.entries.is_empty() {
        writeln!(out, "{NOTHING_NEW}")?;
    }
    for section in [Section::Front, Section::Week, Section::Wire] {
        let mut entries: Vec<_> = view
            .entries
            .iter()
            .filter(|e| e.section == section)
            .collect();
        if entries.is_empty() {
            continue;
        }
        entries.sort_by_key(|e| e.rank);
        writeln!(out, "{}", section_title(section))?;
        for e in entries {
            let page = pages.get(&e.feed_id).copied();
            writeln!(out, "{}", entry_line(e.rank, page, e.title.as_deref()))?;
        }
        let mut overflow: Vec<_> = view
            .overflow
            .iter()
            .filter(|o| o.section == section)
            .collect();
        overflow.sort_by_key(|o| (o.page.is_none(), o.page, o.feed_id));
        for o in overflow {
            writeln!(out, "{}", overflow_line(o.hidden_count, &o.feed_title))?;
        }
    }
    let read = view.entries.iter().filter(|e| e.read_at.is_some()).count();
    let built = ed.built_at.with_timezone(&Local);
    writeln!(
        out,
        "{}",
        header_line(built, view.entries.len(), read, ed.unreachable)
    )?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use ressac_core::edition::Section;
    use ressac_core::service::EditionReport;

    fn report(entries: usize, overflow: u32, unreachable: u32) -> EditionReport {
        EditionReport {
            id: 12,
            entries,
            front: 2,
            week: 7,
            wire: 34,
            overflow,
            unreachable,
        }
    }

    #[test]
    fn built_text_counts_sections_overflow_and_failures() {
        assert_eq!(
            built_text(&report(43, 61, 0)),
            "Edition #12: 43 entries (front 2, week 7, wire 34), 61 more left in their feeds."
        );
        assert_eq!(
            built_text(&report(43, 1, 0)),
            "Edition #12: 43 entries (front 2, week 7, wire 34), 1 more left in its feed."
        );
        assert_eq!(
            built_text(&report(43, 0, 3)),
            "Edition #12: 43 entries (front 2, week 7, wire 34). 3 feeds unreachable."
        );
        assert_eq!(
            built_text(&report(1, 0, 1)),
            "Edition #12: 1 entry (front 2, week 7, wire 34). 1 feed unreachable."
        );
    }

    #[test]
    fn section_titles_are_the_apps_headings() {
        assert_eq!(section_title(Section::Front), "Front Page");
        assert_eq!(section_title(Section::Week), "This Week");
        assert_eq!(section_title(Section::Wire), "The Wire");
    }

    #[test]
    fn entry_and_overflow_lines() {
        assert_eq!(entry_line(3, Some(412), Some("Tides")), "3 412 Tides");
        assert_eq!(entry_line(4, None, None), "4 - (untitled)");
        assert_eq!(overflow_line(12, "Actualitté"), "+12 more in Actualitté");
    }

    #[test]
    fn header_line_names_the_day_counts_and_failures() {
        let built = Local.with_ymd_and_hms(2026, 10, 16, 6, 30, 0).unwrap();
        assert_eq!(
            header_line(built, 43, 12, 0),
            "Friday 16 October · 43 entries · 12 read"
        );
        assert_eq!(
            header_line(built, 1, 0, 1),
            "Friday 16 October · 1 entry · 0 read · 1 feed unreachable"
        );
        assert_eq!(
            header_line(built, 43, 12, 3),
            "Friday 16 October · 43 entries · 12 read · 3 feeds unreachable"
        );
    }

    #[test]
    fn empty_and_missing_edition_texts() {
        assert_eq!(NO_EDITION, "No edition yet.");
        assert_eq!(NOTHING_NEW, "Nothing new this morning.");
    }
}
