//! Human and JSON output for the CLI commands.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use indicatif::{ProgressBar, ProgressStyle};
use ressac_core::model::{Decision, FeedId, Status};
use ressac_core::service::{Event, EventSink, FULL_ARTICLES, Summary};
use ressac_core::store::ImportReport;

/// Order of the `check` summary grid (three per line).
const SUMMARY_ORDER: [Status; 9] = [
    Status::Alive,
    Status::Fossil,
    Status::Moved,
    Status::Dormant,
    Status::Ailing,
    Status::Dead,
    Status::Hijacked,
    Status::Duplicate,
    Status::Unchecked,
];

pub fn glyph(status: Status, ascii: bool) -> &'static str {
    match (status, ascii) {
        (Status::Duplicate, false) => "👯",
        (Status::Hijacked, false) => "🦠",
        (Status::Moved, false) => "🚚",
        (Status::Dead, false) => "🪦",
        (Status::Ailing, false) => "🤒",
        (Status::Fossil, false) => "🦴",
        (Status::Dormant, false) => "🌙",
        (Status::Alive, false) => "🌿",
        (Status::Unchecked, false) => "❔",
        (Status::Duplicate, true) => "=",
        (Status::Hijacked, true) => "!",
        (Status::Moved, true) => ">",
        (Status::Dead, true) => "x",
        (Status::Ailing, true) => "~",
        (Status::Fossil, true) => "#",
        (Status::Dormant, true) => "z",
        (Status::Alive, true) => "+",
        (Status::Unchecked, true) => "?",
    }
}

enum Display {
    /// Progress bar on stderr (hidden automatically when stderr is not a terminal).
    Progress(ProgressBar),
    /// One JSON object per line on stdout.
    Json,
}

/// Shows refresh events and counts fetched feeds, for the interruption notice.
pub struct Sink {
    display: Display,
    started: AtomicUsize,
    done: AtomicUsize,
}

pub fn sink(json: bool) -> Sink {
    if json {
        return Sink::new(Display::Json);
    }
    let bar = ProgressBar::new(0);
    let style =
        ProgressStyle::with_template("Checking {pos}/{len} feeds {bar:20} {elapsed}  {wide_msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar());
    bar.set_style(style);
    Sink::new(Display::Progress(bar))
}

impl Sink {
    fn new(display: Display) -> Self {
        Sink {
            display,
            started: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
        }
    }

    #[cfg(test)]
    pub fn hidden() -> Self {
        Sink::new(Display::Progress(ProgressBar::hidden()))
    }

    pub fn finish(&self) {
        if let Display::Progress(bar) = &self.display {
            bar.finish_and_clear();
        }
    }

    /// Feeds announced by `started` that never reported `feed_done`.
    pub fn not_fetched(&self) -> usize {
        self.started
            .load(Ordering::Relaxed)
            .saturating_sub(self.done.load(Ordering::Relaxed))
    }
}

impl EventSink for Sink {
    fn emit(&self, event: &Event) {
        match event {
            Event::Started { total } => {
                self.started.fetch_add(*total, Ordering::Relaxed);
            }
            Event::FeedDone { .. } => {
                self.done.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        match &self.display {
            Display::Json => {
                // A closed stdout (BrokenPipe) must not panic; the final write reports it.
                let _ = writeln!(std::io::stdout().lock(), "{}", event_json(event));
            }
            Display::Progress(bar) => match event {
                Event::Started { total } => bar.set_length(*total as u64),
                Event::FeedDone { title, .. } => {
                    bar.inc(1);
                    bar.set_message(title.clone());
                }
                Event::NetworkUnreachable { failed, attempted } => bar.println(format!(
                    "Network looks unreachable ({failed}/{attempted} feeds could not connect) — results not recorded as failures."
                )),
                Event::Probing { what, total } => bar.set_message(probing_message(what, *total)),
                Event::WaybackDone { .. }
                | Event::Finished { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. } => {}
            },
        }
    }
}

/// Progress-bar message for a probe phase. The full-article pass counts entries, not feeds.
fn probing_message(what: &str, total: usize) -> String {
    if what == FULL_ARTICLES {
        format!("fetching {total} full articles…")
    } else {
        format!("probing {what} for {total} feeds…")
    }
}

fn event_json(event: &Event) -> String {
    serde_json::to_string(event).unwrap_or_default()
}

/// Writes a one-shot command's result: the event as one JSON line with `--json`, else `text`.
pub fn write_result(
    out: &mut impl Write,
    json: bool,
    event: &Event,
    text: &str,
) -> std::io::Result<()> {
    if json {
        writeln!(out, "{}", event_json(event))
    } else {
        writeln!(out, "{text}")
    }
}

pub fn decided_text(feed_id: FeedId, decision: Decision) -> String {
    let verb = match decision {
        Decision::Keep => "Kept",
        Decision::Repair => "Repaired",
        Decision::Bury => "Buried",
    };
    format!("{verb} feed #{feed_id}.")
}

/// Also shown by the full-screen garden.
pub fn undo_text(undone: Option<FeedId>) -> String {
    undone.map_or("Nothing to undo.".into(), |id| {
        format!("Undid the last decision (#{id}).")
    })
}

/// Also shown by the full-screen garden.
pub fn apply_safe_text(applied: usize, skipped: &[String]) -> String {
    let mut msg = format!("Applied {applied} safe repairs.");
    if !skipped.is_empty() {
        msg.push_str(&format!(
            " Skipped {}: {}",
            skipped.len(),
            skipped.join("; ")
        ));
    }
    msg
}

/// `error` is the stored failure sentence; the wording matches the app's inline notice.
pub fn extracted_text(entry_id: i64, error: Option<&str>) -> String {
    match error {
        None => format!("Saved the full article of entry #{entry_id}."),
        Some(e) => format!("Full article unavailable: {e}"),
    }
}

pub fn migrated_text(version: i64) -> String {
    format!("Schema version {version}.")
}

/// Printed on stderr after a run stopped by Ctrl-C.
pub fn interrupted_message(not_fetched: usize) -> String {
    if not_fetched == 1 {
        "Interrupted: 1 feed was not fetched.".to_string()
    } else {
        format!("Interrupted: {not_fetched} feeds were not fetched.")
    }
}

pub fn import_text(r: &ImportReport, dry_run: bool) -> String {
    let mut out = String::new();
    if dry_run {
        out.push_str("Dry run: nothing written.\n");
    }
    out.push_str(&format!(
        "Imported {} feeds in {} new folders.",
        r.feeds_added, r.folders_added
    ));
    if !r.merged.is_empty() {
        out.push_str(&format!("\nMerged {} duplicate(s):", r.merged.len()));
        for (dropped, kept) in &r.merged {
            out.push_str(&format!("\n  \"{dropped}\" → \"{kept}\""));
        }
    }
    if r.skipped_existing > 0 {
        out.push_str(&format!(
            "\n{} feed(s) already present.",
            r.skipped_existing
        ));
    }
    out
}

pub fn summary_text(s: &Summary, ascii: bool) -> String {
    let cells: Vec<String> = SUMMARY_ORDER
        .iter()
        .filter(|st| **st != Status::Unchecked || s.count(**st) > 0)
        .map(|st| {
            format!(
                "{} {:<10}{:>4}",
                glyph(*st, ascii),
                st.as_str(),
                s.count(*st)
            )
        })
        .collect();
    let mut out = String::new();
    for row in cells.chunks(3) {
        out.push_str("  ");
        out.push_str(&row.join("    "));
        out.push('\n');
    }
    if s.network_unreachable {
        out.push_str("\n  Network looked unreachable: failures were not counted.\n");
    }
    if s.safe_repairs() > 0 {
        out.push_str(&format!(
            "\n  {} safe repairs available ({} × https, {} × redirect)\n",
            s.safe_repairs(),
            s.https_repairs,
            s.redirect_repairs
        ));
    }
    out.push_str("  → run `ressac garden` to triage");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn glyphs_have_an_ascii_fallback() {
        assert_eq!(glyph(Status::Dead, false), "🪦");
        assert_eq!(glyph(Status::Dead, true), "x");
        for s in Status::ALL {
            assert_eq!(glyph(s, true).chars().count(), 1);
        }
    }

    #[test]
    fn sink_counts_feeds_left_unfetched() {
        let sink = Sink::hidden();
        assert_eq!(sink.not_fetched(), 0);
        sink.emit(&Event::Started { total: 5 });
        for feed_id in [1, 2] {
            sink.emit(&Event::FeedDone {
                feed_id,
                title: "T".into(),
                error: None,
            });
        }
        assert_eq!(sink.not_fetched(), 3);
    }

    #[test]
    fn interrupted_message_counts_unfetched_feeds() {
        assert_eq!(
            interrupted_message(3),
            "Interrupted: 3 feeds were not fetched."
        );
        assert_eq!(
            interrupted_message(1),
            "Interrupted: 1 feed was not fetched."
        );
        assert_eq!(
            interrupted_message(0),
            "Interrupted: 0 feeds were not fetched."
        );
    }

    #[test]
    fn import_text_mentions_counts_merges_and_dry_run() {
        let report = ImportReport {
            folders_added: 7,
            feeds_added: 11,
            merged: vec![("Daring Fireball (1)".into(), "Daring Fireball".into())],
            skipped_existing: 0,
        };
        let text = import_text(&report, true);
        assert!(text.starts_with("Dry run: nothing written."));
        assert!(text.contains("Imported 11 feeds in 7 new folders."));
        assert!(text.contains("\"Daring Fireball (1)\" → \"Daring Fireball\""));
    }

    #[test]
    fn summary_text_lists_statuses_repairs_and_next_step() {
        let summary = Summary {
            total: 6,
            counts: BTreeMap::from([("Alive".into(), 3), ("Dead".into(), 2), ("Moved".into(), 1)]),
            https_repairs: 2,
            redirect_repairs: 1,
            network_unreachable: false,
        };
        let text = summary_text(&summary, true);
        assert!(text.contains("+ Alive"), "{text}");
        assert!(text.contains("x Dead"), "{text}");
        assert!(
            text.contains("3 safe repairs available (2 × https, 1 × redirect)"),
            "{text}"
        );
        assert!(text.contains("run `ressac garden` to triage"), "{text}");
        assert!(!text.contains("Unchecked"), "zero Unchecked is hidden");
    }

    #[test]
    fn probing_message_names_full_articles_by_count() {
        assert_eq!(probing_message("https", 3), "probing https for 3 feeds…");
        assert_eq!(
            probing_message(ressac_core::service::FULL_ARTICLES, 20),
            "fetching 20 full articles…"
        );
    }

    #[test]
    fn garden_texts_are_shared_with_the_tui() {
        assert_eq!(decided_text(3, Decision::Keep), "Kept feed #3.");
        assert_eq!(decided_text(3, Decision::Repair), "Repaired feed #3.");
        assert_eq!(decided_text(3, Decision::Bury), "Buried feed #3.");
        assert_eq!(undo_text(Some(4)), "Undid the last decision (#4).");
        assert_eq!(undo_text(None), "Nothing to undo.");
        assert_eq!(apply_safe_text(2, &[]), "Applied 2 safe repairs.");
        assert_eq!(
            apply_safe_text(1, &["#7: Feed #2 already uses x; merge into it instead.".into()]),
            "Applied 1 safe repairs. Skipped 1: #7: Feed #2 already uses x; merge into it instead."
        );
    }

    #[test]
    fn extraction_and_migration_texts() {
        assert_eq!(extracted_text(5, None), "Saved the full article of entry #5.");
        assert_eq!(
            extracted_text(5, Some("HTTP 404.")),
            "Full article unavailable: HTTP 404."
        );
        assert_eq!(migrated_text(2), "Schema version 2.");
    }

    #[test]
    fn write_result_prints_the_event_or_the_text() {
        let event = Event::Undone { feed_id: Some(4) };
        let mut json = Vec::new();
        write_result(&mut json, true, &event, "ignored").unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"event\":\"undone\",\"feed_id\":4}\n"
        );
        let mut text = Vec::new();
        write_result(&mut text, false, &event, "Undid the last decision (#4).").unwrap();
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "Undid the last decision (#4).\n"
        );
    }
}
