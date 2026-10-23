//! The garden pages P800 to P802 as teletext rows (the Mac's `GardenPage`): the entry column
//! (title, progress gauge, status headings and feed rows), the inspector page of the selected
//! feed with its rhythm and theme rows, and the arrow keys' walk. Pure: widths in cells, the
//! clock and the zone come in as arguments. Blue never colours text; Fossil's heading is
//! white on blue.

use std::collections::HashMap;
use std::fmt::Display;

use chrono::{DateTime, TimeZone, Utc};
use ressac_core::model::FeedId;
use ressac_core::store::{AttemptRecord, CadenceDetail};
use unicode_segmentation::UnicodeSegmentation;

use super::{AntiopeColumn, Line, LineRole};
use crate::garden::{
    GardenRow, Inspector, Progress, RHYTHM_CHOICES, chip, empty_message, outcome, progress,
    rhythm_index, rhythm_text, sections, unread_label,
};
use crate::sources::GardenScope;
use crate::teletext::{CellLine, CellSpan, Color, DatePattern, date};

fn wide(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

fn len(text: &str) -> usize {
    text.graphemes(true).count()
}

fn blank_line(id: impl Into<String>, cells: usize) -> Line {
    Line::new(id, LineRole::Blank, CellLine::blank(cells))
}

/// A status in its teletext colour. Fossil's is blue, and blue never colours text, so its
/// heading is reverse video (see [`heading_style`]).
pub fn status_color(status_id: &str) -> Color {
    match status_id {
        "Dead" => Color::Red,
        "Hijacked" => Color::Magenta,
        "Moved" => Color::Cyan,
        "Ailing" => Color::Yellow,
        "Duplicate" => Color::White,
        "Fossil" => Color::Blue,
        "Dormant" | "Alive" => Color::Green,
        _ => Color::White,
    }
}

/// The heading's foreground and background: white on blue for Fossil, else the status colour
/// on black.
pub fn heading_style(status_id: &str) -> (Color, Color) {
    match status_color(status_id) {
        Color::Blue => (Color::White, Color::Blue),
        color => (color, Color::Black),
    }
}

/// The feeds in the order the page lists them: what the arrows walk and the selection
/// follows.
pub fn ordered(visible: &[&GardenRow], scope: GardenScope) -> Vec<FeedId> {
    sections(visible, scope)
        .into_iter()
        .flat_map(|section| section.rows)
        .collect()
}

/// The title row, the progress row (P800 only), then one heading per status in urgency order,
/// each followed by its feeds. `visible` is what the scope lists (`garden::visible_rows`),
/// `unread` the buried feeds' unread counts. Every row is `cells` wide.
pub fn garden_page(
    rows: &[GardenRow],
    visible: &[&GardenRow],
    unread: &HashMap<FeedId, usize>,
    scope: GardenScope,
    selected: Option<FeedId>,
    cells: usize,
) -> Vec<Line> {
    let mut lines = vec![title_line(rows, scope, visible.len(), cells)];
    if scope == GardenScope::NeedsTriage {
        lines.push(progress_line(&progress(rows), cells));
    }
    let listed = sections(visible, scope);
    if listed.is_empty() {
        lines.push(blank_line("blank", cells));
        lines.push(Line::new(
            "empty",
            LineRole::Text,
            CellLine::centred(&empty_message(scope).to_uppercase(), cells, Color::Cyan),
        ));
        return lines;
    }
    let by_id: HashMap<FeedId, &GardenRow> = visible.iter().map(|r| (r.feed_id, *r)).collect();
    for section in listed {
        lines.push(heading_line(section.id, section.rows.len(), cells));
        for id in &section.rows {
            if let Some(row) = by_id.get(id) {
                let count = unread.get(id).copied().unwrap_or(0);
                lines.push(feed_line(row, count, selected == Some(*id), cells));
            }
        }
    }
    lines
}

/// `NEEDS TRIAGE ... 68 LEFT`, `ALL FEEDS ... 186 FEEDS`, `CEMETERY ... 1 FEED`.
pub fn title_line(
    rows: &[GardenRow],
    scope: GardenScope,
    visible_count: usize,
    cells: usize,
) -> Line {
    let count = match scope {
        GardenScope::NeedsTriage => {
            let Progress { decided, needing } = progress(rows);
            format!("{} LEFT", needing.saturating_sub(decided))
        }
        GardenScope::All | GardenScope::Cemetery => {
            if visible_count == 1 {
                "1 FEED".to_string()
            } else {
                format!("{visible_count} FEEDS")
            }
        }
    };
    let count_cells = (len(&count) + 1).min(cells);
    Line::new(
        "title",
        LineRole::Title,
        CellLine::new(vec![
            CellSpan::new(
                &scope.title().to_uppercase(),
                wide(cells - count_cells),
                Color::White,
            ),
            CellSpan::new(&format!(" {count}"), wide(count_cells), Color::Yellow),
        ]),
    )
}

/// P800's mosaic gauge: `▐███░░░ 12/80`, feeds triaged over feeds with a problem status,
/// exactly `cells` wide. Blue is allowed here: the shade is a mosaic.
pub fn progress_line(progress: &Progress, cells: usize) -> Line {
    let done = progress.decided;
    let total = progress.needing;
    let label = format!(" {done}/{total}");
    let label_cells = len(&label).min(cells.saturating_sub(1));
    let bar = cells.saturating_sub(1 + label_cells);
    let filled = (bar * done).checked_div(total).map_or(0, |f| f.min(bar));
    let spans = [
        CellSpan::new("▐", wide(cells.min(1)), Color::Cyan),
        CellSpan::new(&"█".repeat(filled), wide(filled), Color::Green),
        CellSpan::new(&"░".repeat(bar - filled), wide(bar - filled), Color::Blue),
        CellSpan::new(&label, wide(label_cells), Color::Yellow),
    ];
    Line::new(
        "progress",
        LineRole::Progress,
        CellLine::new(spans.into_iter().filter(|s| s.cells() > 0).collect()),
    )
}

/// ` DEAD ... 8`, in the status's teletext colour.
pub fn heading_line(section_id: &str, count: usize, cells: usize) -> Line {
    let (fg, bg) = heading_style(section_id);
    let number = format!(" {count}");
    let number_cells = len(&number).min(cells);
    Line::new(
        format!("heading-{section_id}"),
        LineRole::Heading(section_id.to_string()),
        CellLine::new(vec![
            CellSpan::new(
                &format!(" {}", section_id.to_uppercase()),
                wide(cells - number_cells),
                fg,
            )
            .on(bg),
            CellSpan::new(&number, wide(number_cells), fg).on(bg),
        ]),
    )
}

/// `● Title ... bury`: a yellow dot while the feed needs triage, the title, a buried feed's
/// unread count in yellow (as the index shows counts), then the chip (the proposed action in
/// cyan, or the decision once made in green). Black on white when selected.
pub fn feed_line(row: &GardenRow, unread: usize, selected: bool, cells: usize) -> Line {
    let needs_triage = row.needs_triage();
    let decided = row.decision.is_some() && !needs_triage;
    let chip_text = chip(row).map(|c| format!(" {c}")).unwrap_or_default();
    let count = unread_label(row, unread)
        .map(|_| format!(" {}", unread.min(99_999)))
        .unwrap_or_default();
    let marker_cells = cells.min(2);
    let chip_cells = len(&chip_text).min(cells - marker_cells);
    let count_cells = len(&count).min(cells - marker_cells - chip_cells);
    let mut spans = vec![
        CellSpan::new(
            if needs_triage { "●" } else { "" },
            wide(marker_cells),
            Color::Yellow,
        ),
        CellSpan::new(
            &row.title,
            wide(cells - marker_cells - count_cells - chip_cells),
            Color::White,
        ),
    ];
    if !count.is_empty() {
        spans.push(CellSpan::new(&count, wide(count_cells), Color::Yellow));
    }
    spans.push(CellSpan::new(
        &chip_text,
        wide(chip_cells),
        if decided { Color::Green } else { Color::Cyan },
    ));
    let line = CellLine::new(spans);
    let mut out = Line::new(
        format!("feed-{}", row.feed_id),
        LineRole::Feed(row.feed_id),
        if selected {
            line.reversed(Color::Black, Color::White)
        } else {
            line
        },
    );
    out.selected = selected;
    out
}

/// Greedy word wrap into rows of at most `cells` graphemes; a word longer than a row (a URL)
/// is cut into full rows.
pub fn wrap(text: &str, cells: usize) -> Vec<String> {
    if cells == 0 {
        return Vec::new();
    }
    let mut rows: Vec<String> = Vec::new();
    let mut current = String::new();
    for piece in text.split_whitespace() {
        let mut word = piece.to_string();
        while len(&word) > cells {
            if !current.is_empty() {
                rows.push(std::mem::take(&mut current));
            }
            rows.push(word.graphemes(true).take(cells).collect());
            word = word.graphemes(true).skip(cells).collect();
        }
        if current.is_empty() {
            current = word;
        } else if len(&current) + 1 + len(&word) <= cells {
            current.push(' ');
            current.push_str(&word);
        } else {
            rows.push(std::mem::replace(&mut current, word));
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// What an inspector line builder appends to: numbered ids, `cells` wide.
struct Page {
    lines: Vec<Line>,
    cells: usize,
}

impl Page {
    fn add(&mut self, spans: Vec<CellSpan>, role: LineRole, double_height: bool) {
        let mut line = Line::new(
            format!("inspector-{}", self.lines.len()),
            role,
            CellLine::new(spans),
        );
        line.double_height = double_height;
        self.lines.push(line);
    }

    fn blank(&mut self) {
        let id = format!("inspector-{}", self.lines.len());
        self.lines.push(blank_line(id, self.cells));
    }

    fn heading(&mut self, title: &str, color: Color) {
        let spans = vec![CellSpan::new(
            &title.to_uppercase(),
            wide(self.cells),
            color,
        )];
        self.add(spans, LineRole::Text, false);
    }

    /// `text` wrapped on words; `lead` (a label, a date, a dash) starts the first row and the
    /// following rows are indented by its width.
    fn paragraph(&mut self, text: &str, color: Color, lead: Option<(&str, Color)>) {
        let indent = lead.map_or(0, |(l, _)| len(l).min(self.cells));
        for (index, piece) in wrap(text, self.cells - indent).into_iter().enumerate() {
            let mut spans = Vec::new();
            if let Some((lead, lead_color)) = lead {
                spans.push(CellSpan::new(
                    if index == 0 { lead } else { "" },
                    wide(indent),
                    lead_color,
                ));
            }
            spans.push(CellSpan::new(&piece, wide(self.cells - indent), color));
            self.add(spans, LineRole::Text, false);
        }
    }

    fn labelled(&mut self, label: &str, value: &str) {
        let lead: String = format!("{:<10}", label.to_uppercase())
            .graphemes(true)
            .take(10)
            .collect();
        self.paragraph(value, Color::White, Some((&lead, Color::Cyan)));
    }
}

/// The inspector as a teletext page `cells` wide: name and folder path, the status in double
/// height, the rhythm and theme rows, then evidence, remarks, links, redirects and the fetch
/// log. `cadence` is the selected feed's own; `theme_names` is `garden::theme_names` (empty
/// leaves the Theme rows out); `log` is the feed's fetch log, newest first.
pub fn inspector_page<Tz: TimeZone>(
    inspector: Option<&Inspector>,
    cadence: Option<&CadenceDetail>,
    theme_names: &[String],
    log: &[AttemptRecord],
    cells: usize,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Vec<Line>
where
    Tz::Offset: Display,
{
    let mut page = Page {
        lines: Vec::new(),
        cells,
    };
    if cells == 0 {
        return page.lines;
    }
    let Some(inspector) = inspector else {
        page.blank();
        let id = format!("inspector-{}", page.lines.len());
        page.lines.push(Line::new(
            id,
            LineRole::Text,
            CellLine::centred("NO FEED SELECTED", cells, Color::Cyan),
        ));
        return page.lines;
    };
    page.paragraph(&inspector.title, Color::White, None);
    if !inspector.folder_path.is_empty() {
        page.paragraph(&inspector.folder_path, Color::Cyan, None);
    }
    page.blank();
    let status = inspector.status.as_str();
    let (fg, bg) = heading_style(status);
    page.add(
        vec![CellSpan::new(&format!(" {}", status.to_uppercase()), wide(cells), fg).on(bg)],
        LineRole::Heading(status.to_string()),
        true,
    );
    if inspector.buried {
        page.paragraph(crate::garden::BURIED_NOTE, Color::Yellow, None);
    }
    if !inspector.reason.is_empty() {
        page.paragraph(&inspector.reason, Color::White, None);
    }
    if let Some(cadence) = cadence.filter(|_| !inspector.buried) {
        page.blank();
        page.labelled("Rhythm", &rhythm_text(cadence, now));
        page.lines.extend(cadence_choices(cadence, cells));
    }
    if let Some(current) = inspector.theme.filter(|_| !theme_names.is_empty()) {
        page.blank();
        page.labelled("Theme", theme_names.get(current).map_or("", String::as_str));
        page.lines
            .extend(theme_choices(theme_names, current, cells));
    }
    if !inspector.evidence.is_empty() {
        page.blank();
        page.heading("Evidence", Color::Green);
        for bullet in &inspector.evidence {
            page.paragraph(bullet, Color::White, Some(("- ", Color::Green)));
        }
    }
    if !inspector.remarks.is_empty() {
        page.blank();
        page.heading("Remarks", Color::Yellow);
        for remark in &inspector.remarks {
            page.paragraph(remark, Color::White, Some(("- ", Color::Yellow)));
        }
    }
    page.blank();
    page.heading("Links", Color::Cyan);
    if let Some(proposed) = inspector.proposed.as_deref().filter(|_| !inspector.buried) {
        page.labelled("Proposed", proposed);
    }
    if let Some(decision) = &inspector.decision {
        page.labelled("Decision", decision);
    }
    page.labelled("Feed", &inspector.feed_url);
    if let Some(site) = &inspector.site {
        page.labelled("Site", site);
    }
    if let Some(wayback) = &inspector.wayback {
        page.labelled("Wayback", wayback);
    }
    if let Some(chain) = log
        .first()
        .map(|a| &a.redirect_chain)
        .filter(|c| !c.is_empty())
    {
        page.blank();
        page.heading("Redirects", Color::Magenta);
        for hop in chain {
            page.paragraph(
                &hop.url,
                Color::White,
                Some((&format!("{} ", hop.status), Color::Magenta)),
            );
        }
    }
    page.blank();
    page.heading("Fetch log", Color::Green);
    if log.is_empty() {
        page.paragraph(crate::garden::NOT_FETCHED, Color::White, None);
    }
    for attempt in log {
        let lead = format!("{}  ", date(attempt.at, tz, DatePattern::LogDate));
        page.paragraph(&outcome(attempt), Color::White, Some((&lead, Color::Cyan)));
    }
    page.lines
}

/// One row per rhythm choice (`AUTO`, then the five groups), cyan; the choice in force (Auto
/// unless the feed is overridden) in reverse video, black on cyan. Role `Cadence(index into
/// RHYTHM_CHOICES)`; every row is exactly `cells` wide.
pub fn cadence_choices(cadence: &CadenceDetail, cells: usize) -> Vec<Line> {
    let current = rhythm_index(cadence);
    choice_lines(
        RHYTHM_CHOICES.iter().map(|c| (*c).to_string()),
        current,
        cells,
        "cadence",
        LineRole::Cadence,
    )
}

/// One row per theme (`NONE`, then each theme in capitals), cyan; the current one in reverse
/// video. Role `Theme(index into theme_names)`.
pub fn theme_choices(names: &[String], current: usize, cells: usize) -> Vec<Line> {
    choice_lines(
        names.iter().cloned(),
        current,
        cells,
        "theme",
        LineRole::Theme,
    )
}

fn choice_lines(
    titles: impl Iterator<Item = String>,
    current: usize,
    cells: usize,
    prefix: &str,
    role: fn(usize) -> LineRole,
) -> Vec<Line> {
    titles
        .enumerate()
        .map(|(index, title)| {
            let line = CellLine::new(vec![CellSpan::new(
                &format!(" {}", title.to_uppercase()),
                wide(cells),
                Color::Cyan,
            )]);
            let mut out = Line::new(
                format!("{prefix}-{index}"),
                role(index),
                if index == current {
                    line.reversed(Color::Black, Color::Cyan)
                } else {
                    line
                },
            );
            out.selected = index == current;
            out
        })
        .collect()
}

/// The next (`delta` 1) or previous (-1) feed, stopping at the ends like the native list.
/// Without a selection, the first (or last) feed.
pub fn move_selection(selected: Option<FeedId>, ordered: &[FeedId], delta: i32) -> Option<FeedId> {
    if ordered.is_empty() {
        return selected;
    }
    let Some(index) = selected.and_then(|s| ordered.iter().position(|id| *id == s)) else {
        return if delta >= 0 {
            ordered.first().copied()
        } else {
            ordered.last().copied()
        };
    };
    let target = (index as i64 + i64::from(delta)).clamp(0, ordered.len() as i64 - 1);
    Some(ordered[target as usize])
}

/// The arrows walk the garden list unless the index column is active: its arrows pick pages.
pub fn takes_arrows(column: AntiopeColumn) -> bool {
    column != AntiopeColumn::Index
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use chrono::Utc;
    use ressac_core::model::{Action, CadenceGroup, Decision, Hop, Remark, Status};
    use ressac_core::store::ThemeDef;

    use crate::garden::{theme_names, visible_rows};
    use crate::test_db::{GardenRowExt, at, garden_row};

    const CELLS: usize = 28;

    fn spaces(count: usize) -> String {
        " ".repeat(count)
    }

    fn all_spans(line: &CellLine, f: impl Fn(&CellSpan) -> bool) -> bool {
        line.spans.iter().all(f)
    }

    fn trimmed(line: &Line) -> String {
        line.text().trim_end().to_string()
    }

    fn page(
        rows: &[GardenRow],
        scope: GardenScope,
        retained: &[FeedId],
        selected: Option<FeedId>,
    ) -> Vec<Line> {
        let retained: HashSet<FeedId> = retained.iter().copied().collect();
        let visible = visible_rows(rows, scope, &retained);
        garden_page(rows, &visible, &HashMap::new(), scope, selected, CELLS)
    }

    fn five_rows() -> Vec<GardenRow> {
        vec![
            garden_row(1, "Café 🎉 東京", Some(Status::Dead)).action(Action::Bury),
            garden_row(
                2,
                "A title far longer than the twenty-eight cells of the column",
                Some(Status::Moved),
            )
            .action(Action::FollowRedirect {
                url: "https://m.example.org/feed".into(),
            }),
            garden_row(3, "e\u{301}cole", Some(Status::Fossil)).decided(Decision::Keep),
            garden_row(4, "Gone", Some(Status::Dead))
                .buried()
                .decided(Decision::Bury),
            garden_row(5, "", None),
        ]
    }

    // GardenPageTests

    #[test]
    fn every_garden_line_is_exactly_the_column_width() {
        let rows = five_rows();
        for scope in GardenScope::ALL {
            for selected in [None, Some(1), Some(2)] {
                let lines = page(&rows, scope, &[1, 2, 3, 4], selected);
                assert!(!lines.is_empty());
                for line in &lines {
                    assert_eq!(line.cells.width(), 28, "{scope:?} {}", line.id);
                    assert_eq!(line.text().chars().count(), 28, "{scope:?} {}", line.id);
                    assert!(!line.double_height);
                }
            }
        }
    }

    #[test]
    fn feeds_are_grouped_by_status_in_urgency_order() {
        let statuses = [
            Some(Status::Alive),
            Some(Status::Dead),
            Some(Status::Fossil),
            Some(Status::Duplicate),
            Some(Status::Hijacked),
            Some(Status::Moved),
            Some(Status::Ailing),
            Some(Status::Dormant),
            None,
        ];
        let rows: Vec<GardenRow> = statuses
            .iter()
            .enumerate()
            .map(|(i, s)| garden_row(i as i64 + 1, &format!("Feed {}", i + 1), *s))
            .collect();
        let lines = page(&rows, GardenScope::All, &[], None);
        let heading = |s: &str| LineRole::Heading(s.to_string());
        assert_eq!(
            lines.iter().map(|l| l.role.clone()).collect::<Vec<_>>(),
            [
                LineRole::Title,
                heading("Duplicate"),
                LineRole::Feed(4),
                heading("Hijacked"),
                LineRole::Feed(5),
                heading("Moved"),
                LineRole::Feed(6),
                heading("Dead"),
                LineRole::Feed(2),
                heading("Ailing"),
                LineRole::Feed(7),
                heading("Fossil"),
                LineRole::Feed(3),
                heading("Dormant"),
                LineRole::Feed(8),
                heading("Alive"),
                LineRole::Feed(1),
                heading("Unchecked"),
                LineRole::Feed(9),
            ]
        );
        let visible: Vec<&GardenRow> = rows.iter().collect();
        assert_eq!(
            ordered(&visible, GardenScope::All),
            [4, 5, 6, 2, 7, 3, 8, 1, 9]
        );
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
    }

    #[test]
    fn headings_take_their_teletext_colour_and_fossil_is_reverse_video() {
        let dead = heading_line("Dead", 2, CELLS);
        assert_eq!(dead.text(), format!(" DEAD{}2", spaces(22)));
        assert!(all_spans(&dead.cells, |s| s.fg == Color::Red && s.bg == Color::Black));

        let fossil = heading_line("Fossil", 1, CELLS);
        assert_eq!(fossil.text(), format!(" FOSSIL{}1", spaces(20)));
        assert!(all_spans(&fossil.cells, |s| s.fg == Color::White
            && s.bg == Color::Blue));
        assert_eq!(fossil.action(), "");

        for status in Status::ALL.iter().map(|s| s.as_str()).chain(["Buried"]) {
            let (fg, bg) = heading_style(status);
            assert_ne!(fg, Color::Blue, "{status}: blue never colours text");
            match status_color(status) {
                Color::Blue => assert_eq!((fg, bg), (Color::White, Color::Blue), "{status}"),
                color => assert_eq!((fg, bg), (color, Color::Black), "{status}"),
            }
        }
        assert_eq!(status_color("Hijacked"), Color::Magenta);
        assert_eq!(status_color("Dormant"), Color::Green);
        assert_eq!(status_color("Unchecked"), Color::White);
    }

    #[test]
    fn the_title_row_counts_what_is_left_or_listed() {
        let rows = vec![
            garden_row(1, "A", Some(Status::Dead)),
            garden_row(2, "B", Some(Status::Fossil)).decided(Decision::Keep),
        ];
        let queue = title_line(&rows, GardenScope::NeedsTriage, 2, CELLS);
        assert_eq!(queue.text(), format!("NEEDS TRIAGE{}1 LEFT", spaces(10)));
        assert_eq!(
            queue.cells.spans.last().unwrap(),
            &CellSpan::new(" 1 LEFT", 7, Color::Yellow)
        );
        assert_eq!(
            title_line(&rows, GardenScope::All, 186, CELLS).text(),
            format!("ALL FEEDS{}186 FEEDS", spaces(10))
        );
        assert_eq!(
            title_line(&rows, GardenScope::Cemetery, 1, CELLS).text(),
            format!("CEMETERY{}1 FEED", spaces(14))
        );
    }

    /// The mosaic gauge: `▐`, done cells in green, the rest in blue shade, then done/total,
    /// always exactly 28 cells.
    #[test]
    fn the_progress_row_is_exactly_the_column_width() {
        let cases: [(usize, usize, String); 6] = [
            (
                68,
                80,
                format!("▐{}{} 12/80", "█".repeat(3), "░".repeat(18)),
            ),
            (0, 0, format!("▐{} 0/0", "░".repeat(23))),
            (0, 80, format!("▐{} 80/80", "█".repeat(21))),
            (2, 3, format!("▐{}{} 1/3", "█".repeat(7), "░".repeat(16))),
            (0, 12345, format!("▐{} 12345/12345", "█".repeat(15))),
            (90, 80, format!("▐{} 0/80", "░".repeat(22))),
        ];
        for (left, total, expected) in cases {
            let progress = Progress {
                decided: total.saturating_sub(left),
                needing: total,
            };
            let line = progress_line(&progress, CELLS);
            assert_eq!(line.text(), expected, "{left}/{total}");
            assert_eq!(line.cells.width(), 28, "{left}/{total}");
            assert_eq!(line.role, LineRole::Progress);
        }
        let line = progress_line(
            &Progress {
                decided: 12,
                needing: 80,
            },
            CELLS,
        );
        assert_eq!(
            line.cells.spans,
            [
                CellSpan::new("▐", 1, Color::Cyan),
                CellSpan::new("███", 3, Color::Green),
                CellSpan::new(&"░".repeat(18), 18, Color::Blue),
                CellSpan::new(" 12/80", 6, Color::Yellow),
            ]
        );
        assert!(
            page(&[], GardenScope::All, &[], None)
                .iter()
                .all(|l| l.role != LineRole::Progress),
            "the gauge belongs to P800 only"
        );
        for width in [0, 1, 2, 5, 40] {
            assert_eq!(
                progress_line(
                    &Progress {
                        decided: 1,
                        needing: 3
                    },
                    width
                )
                .cells
                .width(),
                width
            );
        }
    }

    #[test]
    fn feed_rows_show_the_marker_the_title_and_the_chip() {
        let zed = garden_row(1, "Zed", Some(Status::Dead)).action(Action::Bury);
        let waiting = feed_line(&zed, 0, false, CELLS);
        assert_eq!(waiting.text(), format!("● Zed{}bury", spaces(19)));
        assert_eq!(waiting.cells.spans[0], CellSpan::new("●", 2, Color::Yellow));
        assert_eq!(
            waiting.cells.spans.last().unwrap(),
            &CellSpan::new(" bury", 5, Color::Cyan)
        );
        assert!(all_spans(&waiting.cells, |s| s.bg == Color::Black));
        assert!(!waiting.selected);
        assert_eq!(waiting.role, LineRole::Feed(1));
        assert_eq!(waiting.action(), "feed:1");

        let old = garden_row(2, "Old", Some(Status::Fossil)).decided(Decision::Keep);
        let kept = feed_line(&old, 0, false, CELLS);
        assert_eq!(kept.text(), format!("  Old{}kept", spaces(19)));
        assert_eq!(
            kept.cells.spans.last().unwrap(),
            &CellSpan::new(" kept", 5, Color::Green)
        );

        let selected = feed_line(&zed, 0, true, CELLS);
        assert!(selected.selected);
        assert!(all_spans(&selected.cells, |s| s.fg == Color::Black
            && s.bg == Color::White));
        assert_eq!(selected.text(), waiting.text());
    }

    /// A buried feed's unread count sits before its chip, in yellow like the index's counts.
    #[test]
    fn buried_feed_rows_show_their_unread_count() {
        let gone = garden_row(4, "Gone", Some(Status::Dead))
            .buried()
            .decided(Decision::Bury);
        let counted = feed_line(&gone, 12, false, CELLS);
        assert_eq!(counted.text(), format!("  Gone{} 12 buried", spaces(12)));
        assert_eq!(counted.cells.width(), 28);
        assert_eq!(
            counted.cells.spans[2],
            CellSpan::new(" 12", 3, Color::Yellow)
        );
        assert_eq!(
            counted.cells.spans.last().unwrap(),
            &CellSpan::new(" buried", 7, Color::Green)
        );

        let none = feed_line(&gone, 0, false, CELLS);
        assert_eq!(none.text(), format!("  Gone{} buried", spaces(15)));
        assert_eq!(none.cells.spans.len(), 3);
        // An active feed never shows a count.
        let active = garden_row(5, "Live", Some(Status::Alive));
        assert!(!feed_line(&active, 7, false, CELLS).text().contains('7'));
    }

    #[test]
    fn missing_glyphs_are_replaced() {
        let row = garden_row(3, "Café 🎉", Some(Status::Dead)).action(Action::Bury);
        let line = feed_line(&row, 0, true, CELLS);
        assert!(line.text().starts_with("● Café ?"));
        assert_eq!(line.cells.width(), 28);
        assert!(
            line.cells.spans[1]
                .runs
                .iter()
                .any(|r| r.replacement && r.text == "?")
        );
    }

    #[test]
    fn empty_pages_show_one_centred_cyan_line() {
        let queue = page(&[], GardenScope::NeedsTriage, &[], None);
        assert_eq!(
            queue.iter().map(|l| l.role.clone()).collect::<Vec<_>>(),
            [
                LineRole::Title,
                LineRole::Progress,
                LineRole::Blank,
                LineRole::Text
            ]
        );
        assert_eq!(
            queue.last().unwrap().text(),
            format!("{}NOTHING TO TRIAGE{}", spaces(5), spaces(6))
        );
        assert!(all_spans(&queue.last().unwrap().cells, |s| s.fg == Color::Cyan));
        let alive = [garden_row(1, "Alive", Some(Status::Alive))];
        let cemetery = page(&alive, GardenScope::Cemetery, &[], None);
        assert_eq!(
            cemetery.last().unwrap().text(),
            format!("{}THE CEMETERY IS EMPTY{}", spaces(3), spaces(4))
        );
    }

    #[test]
    fn arrows_move_the_selection_and_stop_at_the_ends() {
        let ordered = [1, 2, 3];
        assert_eq!(move_selection(None, &ordered, 1), Some(1));
        assert_eq!(move_selection(None, &ordered, -1), Some(3));
        assert_eq!(move_selection(Some(1), &ordered, 1), Some(2));
        assert_eq!(move_selection(Some(1), &ordered, -1), Some(1));
        assert_eq!(move_selection(Some(3), &ordered, 1), Some(3));
        assert_eq!(move_selection(Some(9), &ordered, 1), Some(1));
        assert_eq!(move_selection(Some(9), &[], 1), Some(9));
        assert!(!takes_arrows(AntiopeColumn::Index));
        assert!(takes_arrows(AntiopeColumn::Entries));
        assert!(takes_arrows(AntiopeColumn::Reader));
    }

    #[test]
    fn the_queue_keeps_decided_feeds_until_the_page_is_left() {
        let rows = vec![
            garden_row(1, "Waiting", Some(Status::Dead)),
            garden_row(2, "Decided", Some(Status::Dead)).decided(Decision::Keep),
        ];
        let kept = page(&rows, GardenScope::NeedsTriage, &[1, 2], None);
        let decided = kept.iter().find(|l| l.role == LineRole::Feed(2)).unwrap();
        assert!(decided.text().starts_with("  Decided"));
        let fresh = page(&rows, GardenScope::NeedsTriage, &[1], None);
        assert!(!fresh.iter().any(|l| l.role == LineRole::Feed(2)));
    }

    // GardenInspectorPageTests

    fn attempt(status: u16, redirects: &[(u16, &str)]) -> AttemptRecord {
        AttemptRecord {
            at: at("2026-10-10T07:00:00Z"),
            http_status: Some(status),
            error: None,
            redirect_chain: redirects
                .iter()
                .map(|(status, url)| Hop {
                    status: *status,
                    url: (*url).into(),
                })
                .collect(),
            content_type: None,
            network_suspect: false,
            not_modified: false,
            entry_count: None,
        }
    }

    fn log() -> Vec<AttemptRecord> {
        vec![attempt(
            410,
            &[(301, "https://www.unphotographable.com/index.rdf")],
        )]
    }

    fn now() -> DateTime<Utc> {
        at("2026-10-16T09:00:00Z")
    }

    fn inspect(
        row: &GardenRow,
        cadence: Option<&CadenceDetail>,
        themes: &[ThemeDef],
        theme_key: Option<&str>,
        log: &[AttemptRecord],
    ) -> Inspector {
        Inspector::of(row, cadence, themes, theme_key, log, now(), &Utc)
    }

    fn unphotographable() -> GardenRow {
        garden_row(7, "Unphotographable", Some(Status::Ailing))
            .folder(Some("Blogs / Photo"))
            .reason("The feed is not valid XML.")
            .evidence(&["Parse error at line 3."])
            .remarks(vec![Remark::HttpOnly])
            .site("http://www.unphotographable.com/")
    }

    #[test]
    fn the_inspector_is_a_teletext_page() {
        let row = unphotographable();
        let log = log();
        let inspector = inspect(&row, None, &[], None, &log);
        let lines = inspector_page(Some(&inspector), None, &[], &log, 40, now(), &Utc);
        assert_eq!(
            lines.iter().map(trimmed).collect::<Vec<_>>(),
            [
                "Unphotographable",
                "Blogs / Photo",
                "",
                " AILING",
                "The feed is not valid XML.",
                "",
                "EVIDENCE",
                "- Parse error at line 3.",
                "",
                "REMARKS",
                "- Only served over http://.",
                "",
                "LINKS",
                "FEED      https://feeds.example.org/7",
                "SITE      http://www.unphotographable.co",
                "          m/",
                "",
                "REDIRECTS",
                "301 https://www.unphotographable.com/ind",
                "    ex.rdf",
                "",
                "FETCH LOG",
                "10 OCT 26 07:00  HTTP 410",
            ]
        );
        for line in &lines {
            assert_eq!(line.cells.width(), 40, "{}: «{}»", line.id, line.text());
        }
        let status = lines.iter().find(|l| l.double_height).unwrap();
        assert_eq!(status.role, LineRole::Heading("Ailing".into()));
        assert!(all_spans(&status.cells, |s| s.fg == Color::Yellow));
        assert_eq!(lines.iter().filter(|l| l.double_height).count(), 1);
        assert_eq!(
            lines[13].cells.spans[0],
            CellSpan::new("FEED      ", 10, Color::Cyan)
        );
        assert_eq!(lines[21].cells.spans[0].fg, Color::Green);
        assert_eq!(
            lines[22].cells.spans[0],
            CellSpan::new("10 OCT 26 07:00  ", 17, Color::Cyan)
        );
        assert_eq!(
            lines[18].cells.spans[0],
            CellSpan::new("301 ", 4, Color::Magenta)
        );
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
    }

    #[test]
    fn a_buried_fossil_shows_how_to_restore_it() {
        let fossil = garden_row(8, "Old blog", Some(Status::Fossil))
            .buried()
            .action(Action::Bury)
            .decided(Decision::Bury);
        let themes = [ThemeDef {
            key: "tech".into(),
            name: "Tech".into(),
            sea: "SEA".into(),
        }];
        let inspector = inspect(&fossil, None, &themes, None, &[]);
        let lines = inspector_page(
            Some(&inspector),
            None,
            &theme_names(&themes),
            &[],
            40,
            now(),
            &Utc,
        );
        let status = lines.iter().find(|l| l.double_height).unwrap();
        assert!(all_spans(&status.cells, |s| s.fg == Color::White
            && s.bg == Color::Blue));
        let texts: Vec<String> = lines.iter().map(trimmed).collect();
        assert!(texts.contains(&"Buried. Press k to restore it.".to_string()));
        assert!(!texts.iter().any(|t| t.starts_with("PROPOSED")));
        assert!(texts.contains(&"DECISION  Bury".to_string()));
        assert!(texts.contains(&"Not fetched yet.".to_string()));
        assert!(!texts.iter().any(|t| t.starts_with("THEME")));
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l.role, LineRole::Theme(_) | LineRole::Cadence(_)))
        );
    }

    #[test]
    fn without_a_selection_the_reader_says_so() {
        let none = inspector_page(None, None, &[], &[], 40, now(), &Utc);
        assert_eq!(
            none.iter().map(Line::text).collect::<Vec<_>>(),
            [
                spaces(40),
                format!("{}NO FEED SELECTED{}", spaces(12), spaces(12))
            ]
        );
        assert!(all_spans(&none[1].cells, |s| s.fg == Color::Cyan));
    }

    /// Spec §5: blue is for separators, backgrounds and mosaics, never for text.
    #[test]
    fn blue_never_colours_text_in_the_garden() {
        let mut rows: Vec<GardenRow> = Status::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| garden_row(i as i64 + 1, s.as_str(), Some(*s)))
            .collect();
        rows.push(garden_row(99, "Buried", Some(Status::Dead)).buried());
        let mut lines = page(&rows, GardenScope::All, &[99], Some(3));
        lines.extend(page(&rows, GardenScope::Cemetery, &[], None));
        for row in &rows {
            let inspector = inspect(row, None, &[], None, &log());
            lines.extend(inspector_page(
                Some(&inspector),
                None,
                &[],
                &log(),
                40,
                now(),
                &Utc,
            ));
        }
        for line in lines.iter().filter(|l| l.role != LineRole::Progress) {
            assert!(
                all_spans(&line.cells, |s| s.fg != Color::Blue),
                "{}: «{}»",
                line.id,
                line.text()
            );
        }
    }

    #[test]
    fn wrap_breaks_on_words_and_splits_long_words() {
        assert_eq!(
            wrap("The quick brown fox jumps", 10),
            ["The quick", "brown fox", "jumps"]
        );
        assert_eq!(wrap("abcdefghijkl xy", 5), ["abcde", "fghij", "kl xy"]);
        assert_eq!(wrap("a\nb", 10), ["a b"]);
        assert!(wrap("", 10).is_empty());
        assert!(wrap("anything", 0).is_empty());
    }

    // AntiopeRhythmTests

    fn detail(group: CadenceGroup, overridden: bool) -> CadenceDetail {
        CadenceDetail {
            feed_id: 7,
            effective_group: group,
            computed_group: CadenceGroup::Weekly,
            is_override: overridden,
            median_interval_days: Some(0.4),
            entries_per_month: 30.0,
            last_entry_at: Some(at("2026-10-16T07:00:00Z")),
            unread: 3,
        }
    }

    #[test]
    fn the_inspector_shows_the_rhythm_under_the_status() {
        let row = unphotographable();
        let cadence = detail(CadenceGroup::Daily, false);
        let inspector = inspect(&row, Some(&cadence), &[], None, &log());
        let lines = inspector_page(
            Some(&inspector),
            Some(&cadence),
            &[],
            &log(),
            60,
            now(),
            &Utc,
        );
        let rhythm = lines
            .iter()
            .position(|l| l.text().starts_with("RHYTHM    "))
            .unwrap();
        assert_eq!(
            trimmed(&lines[rhythm]),
            format!("RHYTHM    {}", rhythm_text(&cadence, now()))
        );
        assert_eq!(
            lines[rhythm + 1..rhythm + 7]
                .iter()
                .map(|l| l.role.clone())
                .collect::<Vec<_>>(),
            (0..6).map(LineRole::Cadence).collect::<Vec<_>>()
        );
        let at_text = |needle: &str| {
            lines
                .iter()
                .position(|l| l.text().contains(needle))
                .unwrap()
        };
        assert!(at_text("The feed is not valid XML.") < rhythm);
        assert!(rhythm < at_text("EVIDENCE"));
        for line in &lines {
            assert_eq!(line.cells.width(), 60, "{}", line.id);
        }
    }

    #[test]
    fn no_rhythm_for_a_buried_feed_or_without_cadence() {
        let row = unphotographable();
        let cadence = detail(CadenceGroup::Daily, false);
        let without = inspector_page(
            Some(&inspect(&row, None, &[], None, &[])),
            None,
            &[],
            &[],
            60,
            now(),
            &Utc,
        );
        assert!(!without.iter().any(|l| l.text().starts_with("RHYTHM")));
        assert!(
            !without
                .iter()
                .any(|l| matches!(l.role, LineRole::Cadence(_)))
        );
        let buried = garden_row(8, "Old", Some(Status::Fossil)).buried();
        let inspector = inspect(&buried, Some(&cadence), &[], None, &[]);
        let lines = inspector_page(Some(&inspector), Some(&cadence), &[], &[], 60, now(), &Utc);
        assert!(!lines.iter().any(|l| l.text().starts_with("RHYTHM")));
        assert!(!lines.iter().any(|l| matches!(l.role, LineRole::Cadence(_))));
    }

    #[test]
    fn the_override_rows_show_the_choice_in_force() {
        let auto = cadence_choices(&detail(CadenceGroup::Daily, false), 40);
        assert_eq!(
            auto.iter()
                .map(|l| l.text().trim_end().to_string())
                .collect::<Vec<_>>(),
            [" AUTO", " DAILY", " WEEKLY", " MONTHLY", " RARE", " SILENT"]
        );
        let reversed =
            |line: &Line| all_spans(&line.cells, |s| s.fg == Color::Black && s.bg == Color::Cyan);
        assert_eq!(
            auto.iter().map(reversed).collect::<Vec<_>>(),
            [true, false, false, false, false, false]
        );
        assert!(auto.iter().all(|l| l.cells.width() == 40));
        let forced = cadence_choices(&detail(CadenceGroup::Daily, true), 40);
        assert_eq!(
            forced.iter().map(reversed).collect::<Vec<_>>(),
            [false, true, false, false, false, false]
        );
        assert_eq!(forced[3].action(), "cadence:3");
        assert_eq!(
            forced.iter().map(|l| l.selected).collect::<Vec<_>>(),
            [false, true, false, false, false, false]
        );
        let ids: HashSet<_> = forced.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn the_theme_rows_show_the_theme_in_force() {
        let names = ["None", "Tech", "Arts"].map(String::from);
        let rows = theme_choices(&names, 2, 30);
        assert_eq!(
            rows.iter()
                .map(|l| l.text().trim_end().to_string())
                .collect::<Vec<_>>(),
            [" NONE", " TECH", " ARTS"]
        );
        assert_eq!(
            rows.iter()
                .map(|l| all_spans(&l.cells, |s| s.bg == Color::Cyan))
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(
            rows.iter().map(Line::action).collect::<Vec<_>>(),
            ["theme:0", "theme:1", "theme:2"]
        );
        assert!(rows.iter().all(|l| l.cells.width() == 30));

        // In the page: after the rhythm rows, before EVIDENCE; none for a buried feed.
        let themes = [
            ThemeDef {
                key: "tech".into(),
                name: "Tech".into(),
                sea: "S".into(),
            },
            ThemeDef {
                key: "arts".into(),
                name: "Arts".into(),
                sea: "S".into(),
            },
        ];
        let row = unphotographable();
        let cadence = detail(CadenceGroup::Weekly, false);
        let inspector = inspect(&row, Some(&cadence), &themes, Some("arts"), &[]);
        let lines = inspector_page(
            Some(&inspector),
            Some(&cadence),
            &theme_names(&themes),
            &[],
            60,
            now(),
            &Utc,
        );
        let position = |f: &dyn Fn(&Line) -> bool| lines.iter().position(f).unwrap();
        let last_cadence = lines
            .iter()
            .rposition(|l| matches!(l.role, LineRole::Cadence(_)))
            .unwrap();
        let first_theme = position(&|l| l.role == LineRole::Theme(0));
        let evidence = position(&|l| l.text().starts_with("EVIDENCE"));
        assert!(last_cadence < first_theme && first_theme < evidence);
        assert_eq!(trimmed(&lines[first_theme - 1]), "THEME     Arts");
        let reversed: Vec<usize> = lines
            .iter()
            .filter_map(|l| match l.role {
                LineRole::Theme(i) if l.selected => Some(i),
                _ => None,
            })
            .collect();
        assert_eq!(reversed, [2]);
        assert_eq!(
            lines
                .iter()
                .filter(|l| matches!(l.role, LineRole::Theme(_)))
                .count(),
            3
        );
        let ids: HashSet<_> = lines.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids.len(), lines.len());
    }

    #[test]
    fn bedstead_draws_the_garden_glyphs() {
        for glyph in ["▐", "█", "░", "●"] {
            assert!(crate::teletext::supports(glyph), "{glyph}");
        }
    }
}
