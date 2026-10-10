//! Human and JSON output for the CLI commands.

use indicatif::{ProgressBar, ProgressStyle};
use ressac_core::model::Status;
use ressac_core::service::{Event, EventSink, Summary};
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

pub enum Sink {
    /// Progress bar on stderr (hidden automatically when stderr is not a terminal).
    Progress(ProgressBar),
    /// One JSON object per line on stdout.
    Json,
}

pub fn sink(json: bool) -> Sink {
    if json {
        return Sink::Json;
    }
    let bar = ProgressBar::new(0);
    let style =
        ProgressStyle::with_template("Checking {pos}/{len} feeds {bar:20} {elapsed}  {wide_msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar());
    bar.set_style(style);
    Sink::Progress(bar)
}

impl Sink {
    pub fn finish(&self) {
        if let Sink::Progress(bar) = self {
            bar.finish_and_clear();
        }
    }
}

impl EventSink for Sink {
    fn emit(&self, event: &Event) {
        match self {
            Sink::Json => println!("{}", serde_json::to_string(event).unwrap_or_default()),
            Sink::Progress(bar) => match event {
                Event::Started { total } => bar.set_length(*total as u64),
                Event::FeedDone { title, .. } => {
                    bar.inc(1);
                    bar.set_message(title.clone());
                }
                Event::NetworkUnreachable { failed, attempted } => bar.println(format!(
                    "Network looks unreachable ({failed}/{attempted} feeds could not connect) — results not recorded as failures."
                )),
                Event::Probing { what, total } => bar.set_message(format!("probing {what} for {total} feeds…")),
                Event::WaybackDone { .. } | Event::Finished { .. } => {}
            },
        }
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
}
