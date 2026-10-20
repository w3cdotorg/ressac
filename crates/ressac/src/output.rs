//! Human and JSON output for the CLI commands.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use indicatif::{ProgressBar, ProgressStyle};
use ressac_core::model::{Decision, FeedId, Status};
use ressac_core::service::{AddedFeed, Event, EventSink, FULL_ARTICLES, Summary};
use ressac_core::store::pages::PageRow;
use ressac_core::store::{ChartImportReport, ChartSnapshot, ImportReport, Moved, ThemeSource};

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
                    bar.set_message(progress_title(title));
                }
                Event::NetworkUnreachable { failed, attempted } => bar.println(format!(
                    "Network looks unreachable ({failed}/{attempted} feeds could not connect) — results not recorded as failures."
                )),
                Event::Probing { what, total } => bar.set_message(probing_message(what, *total)),
                Event::WaybackDone { .. }
                | Event::Finished { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. }
                | Event::Page(_)
                | Event::Edition { .. } => {}
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

/// `ressac add`'s lines: the feed and its page, its theme, and the territory it joined.
pub fn added_text(added: &AddedFeed) -> String {
    let page = added.page.map_or(String::new(), |p| format!(" (page {p})"));
    let mut text = format!("Added #{} {}{page}", added.id, printable(&added.title));
    let theme = match (&added.theme, &added.theme_source) {
        (Some(name), ThemeSource::Sibling { title, .. }) => {
            format!("{} (same site as {})", printable(name), printable(title))
        }
        (Some(name), _) => format!("{} (chosen)", printable(name)),
        (None, _) => "none (Uncharted waters)".to_owned(),
    };
    text.push_str(&format!("\nTheme: {theme}"));
    if let Some(territory) = &added.territory {
        text.push_str(&format!("\nTerritory: {}", printable(territory)));
    }
    text
}

/// The `added` event that ends `ressac --json add`; `like` names the sibling a theme came from.
pub fn added_json(added: &AddedFeed) -> serde_json::Value {
    let like = match &added.theme_source {
        ThemeSource::Sibling { title, .. } => Some(title.as_str()),
        ThemeSource::Chosen | ThemeSource::None => None,
    };
    serde_json::json!({
        "event": "added",
        "id": added.id,
        "title": added.title,
        "xml_url": added.xml_url,
        "page": added.page,
        "theme": added.theme,
        "territory": added.territory,
        "discovered": added.discovered,
        "like": like,
    })
}

/// `ressac feed move`: `Moved #7 Title to Folder (page 120 → 305)`; `folder` is the target's
/// name, `None` for the root.
pub fn moved_text(moved: &Moved, folder: Option<&str>) -> String {
    let place = folder.map_or("the root".into(), printable);
    let title = printable(&moved.title);
    let page = |p: Option<u16>| p.map_or("-".to_owned(), |p| p.to_string());
    if !moved.changed {
        return format!("#{} {title} is already in {place}.", moved.feed);
    }
    format!(
        "Moved #{} {title} to {place} (page {} → {})",
        moved.feed,
        page(moved.old_page),
        page(moved.new_page)
    )
}

/// The `moved` event of `ressac --json feed move`.
pub fn moved_json(moved: &Moved) -> serde_json::Value {
    serde_json::json!({
        "event": "moved",
        "feed_id": moved.feed,
        "title": moved.title,
        "from": moved.from,
        "to": moved.to,
        "old_page": moved.old_page,
        "new_page": moved.new_page,
        "changed": moved.changed,
    })
}

pub fn migrated_text(version: i64) -> String {
    format!("Schema version {version}.")
}

/// Text for a terminal: every control character (ESC, C1 such as U+009B, tab, newline) is
/// replaced by U+FFFD, so a feed or entry title cannot inject escape sequences. Plain text is
/// returned borrowed. JSON output does not need it: serde escapes controls.
pub fn printable(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().any(char::is_control) {
        text.chars()
            .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
            .collect::<String>()
            .into()
    } else {
        text.into()
    }
}

/// A feed title as the progress bar shows it: controls neutralised, like every other terminal line.
fn progress_title(title: &str) -> String {
    printable(title).into_owned()
}

/// One line of `ressac page list` and `ressac page set`: `page kind id title`, with `-`
/// for the id of a reserved page.
pub fn page_text(row: &PageRow) -> String {
    let id = row.id.map_or("-".to_string(), |id| id.to_string());
    format!(
        "{} {} {id} {}",
        row.page,
        row.kind.as_str(),
        printable(&row.title)
    )
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

/// The chart's name for feeds that have no theme.
const UNCHARTED: &str = "UNCHARTED WATERS";

/// `ressac chart` without `--json`: feeds per sea, then the territories.
pub fn chart_text(snap: &ChartSnapshot) -> String {
    let mut out = format!(
        "Sea chart: {} feeds, {} unread this week\n",
        snap.totals.feeds, snap.totals.unread_week
    );
    let mut seas: Vec<(std::borrow::Cow<str>, Option<&str>)> = snap
        .themes
        .iter()
        .map(|t| (printable(&t.sea), Some(t.key.as_str())))
        .collect();
    if snap.feeds.iter().any(|f| f.theme.is_none()) {
        seas.push((UNCHARTED.into(), None));
    }
    for (sea, key) in seas {
        let feeds: Vec<_> = snap
            .feeds
            .iter()
            .filter(|f| f.theme.as_deref() == key)
            .collect();
        let tag = key
            .map(|k| format!(" [{}]", printable(k)))
            .unwrap_or_default();
        out.push_str(&format!("\n{sea}{tag} ({})\n", feeds.len()));
        for f in feeds {
            out.push_str(&format!("  #{} {}\n", f.id, printable(&f.title)));
        }
    }
    if !snap.territories.is_empty() {
        out.push_str("\nTerritories\n");
        for t in &snap.territories {
            let titles: Vec<std::borrow::Cow<str>> = t
                .feeds
                .iter()
                .filter_map(|id| snap.feeds.iter().find(|f| f.id == *id))
                .map(|f| printable(&f.title))
                .collect();
            out.push_str(&format!(
                "  {}: {}\n",
                printable(&t.name),
                titles.join(", ")
            ));
        }
    }
    out
}

/// `ressac chart import`: what the file did, or on a dry run would do.
pub fn chart_import_text(r: &ChartImportReport, dry_run: bool) -> String {
    let mut out = String::new();
    if dry_run {
        out.push_str("Dry run: nothing written.\n");
    }
    out.push_str(&format!(
        "{} {} theme(s), {} feed(s) themed, {} territories.",
        if dry_run { "Would import" } else { "Imported" },
        r.themes,
        r.feeds_themed,
        r.territories
    ));
    if !r.territories_too_small.is_empty() {
        out.push_str(&format!(
            "\n{} territories with fewer than 2 matched feeds, left off the chart: {}.",
            r.territories_too_small.len(),
            r.territories_too_small
                .iter()
                .map(|name| printable(name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if r.unthemed_feeds > 0 {
        out.push_str(&format!(
            "\n{} feed(s) left without a theme.",
            r.unthemed_feeds
        ));
    }
    if !r.unknown_theme_keys.is_empty() {
        out.push_str(&format!(
            "\nUnknown theme keys: {}.",
            r.unknown_theme_keys
                .iter()
                .map(|k| printable(k))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !r.unmatched_urls.is_empty() {
        out.push_str(&format!("\n{} unmatched URL(s):", r.unmatched_urls.len()));
        for url in &r.unmatched_urls {
            out.push_str(&format!("\n  {}", printable(url)));
        }
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
    fn chart_text_neutralises_controls() {
        use ressac_core::model::CadenceGroup;
        use ressac_core::store::{ChartFeed, ChartTerritory, ChartTheme, ChartTotals};
        let evil = "Evil\u{1b}[31m\nTitle";
        let snap = ChartSnapshot {
            version: 1,
            surveyed_at: "2026-10-19T12:00:00Z".into(),
            totals: ChartTotals {
                feeds: 2,
                unread_week: 0,
            },
            themes: vec![ChartTheme {
                key: "t".into(),
                name: "T".into(),
                sea: "SEA\u{1b}[2J".into(),
                count: 2,
            }],
            feeds: [1, 2]
                .map(|id| ChartFeed {
                    id,
                    title: evil.into(),
                    theme: Some("t".into()),
                    folder: None,
                    group: CadenceGroup::Daily,
                    age_days: 0.0,
                    unread_week: 0,
                    entries_week: 0,
                    activity: 0.0,
                    page: None,
                    entries: vec![],
                })
                .into(),
            territories: vec![ChartTerritory {
                name: "COAST\u{1b}[31m\nX".into(),
                feeds: vec![1, 2],
            }],
            continents: vec![],
        };
        let text = chart_text(&snap);
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(text.contains("SEA\u{FFFD}[2J [t] (2)\n"), "{text:?}");
        assert!(
            text.contains("  #1 Evil\u{FFFD}[31m\u{FFFD}Title\n"),
            "{text:?}"
        );
        assert!(
            text.contains("COAST\u{FFFD}[31m\u{FFFD}X: Evil"),
            "{text:?}"
        );
        assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
    }

    #[test]
    fn chart_import_text_says_would_on_a_dry_run_and_names_small_territories() {
        let r = ChartImportReport {
            themes: 2,
            feeds_themed: 3,
            territories: 1,
            territories_too_small: vec!["LONE".into(), "EMPTY".into()],
            ..Default::default()
        };
        let real = chart_import_text(&r, false);
        assert!(
            real.starts_with("Imported 2 theme(s), 3 feed(s) themed, 1 territories."),
            "{real}"
        );
        assert!(
            real.contains(
                "\n2 territories with fewer than 2 matched feeds, left off the chart: LONE, EMPTY."
            ),
            "{real}"
        );
        let dry = chart_import_text(&r, true);
        assert!(
            dry.starts_with(
                "Dry run: nothing written.\nWould import 2 theme(s), 3 feed(s) themed, 1 territories."
            ),
            "{dry}"
        );
    }

    #[test]
    fn chart_import_text_neutralises_controls() {
        let r = ChartImportReport {
            unmatched_urls: vec!["https://x.example/\u{1b}]0;pwn\u{7}".into()],
            unknown_theme_keys: vec!["k\u{1b}[31m".into()],
            territories_too_small: vec!["T\u{1b}[2J".into()],
            ..Default::default()
        };
        let text = chart_import_text(&r, true);
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n'),
            "{text:?}"
        );
        assert!(text.contains("https://x.example/\u{FFFD}]0;pwn\u{FFFD}"));
        assert!(text.contains("k\u{FFFD}[31m"));
        assert!(text.contains("T\u{FFFD}[2J"));
    }

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
            apply_safe_text(
                1,
                &["#7: Feed #2 already uses x; merge into it instead.".into()]
            ),
            "Applied 1 safe repairs. Skipped 1: #7: Feed #2 already uses x; merge into it instead."
        );
    }

    #[test]
    fn extraction_and_migration_texts() {
        assert_eq!(
            extracted_text(5, None),
            "Saved the full article of entry #5."
        );
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

    #[test]
    fn progress_title_neutralises_controls() {
        assert_eq!(progress_title("a\u{1b}[2Jb"), "a\u{FFFD}[2Jb");
        assert_eq!(progress_title("Courrier"), "Courrier");
    }

    #[test]
    fn printable_replaces_controls_and_borrows_plain_text() {
        assert_eq!(
            printable("a\u{1b}[31mb\u{9b}2J\tc\n"),
            "a\u{FFFD}[31mb\u{FFFD}2J\u{FFFD}c\u{FFFD}"
        );
        assert!(matches!(
            printable("Courrier international é"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn page_text_is_page_kind_id_title() {
        use ressac_core::store::pages::PageKind;
        let feed = PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: Some(400),
        };
        assert_eq!(page_text(&feed), "412 feed 12 Courrier international");
        let index = PageRow {
            page: 100,
            kind: PageKind::Reserved,
            id: None,
            title: "Index".into(),
            parent_page: None,
        };
        assert_eq!(page_text(&index), "100 reserved - Index");
    }
}
