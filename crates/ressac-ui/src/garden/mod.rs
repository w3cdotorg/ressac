//! The garden's rules, look-agnostic: sections, retention, keys, labels, the rhythm line and
//! the Theme and Rhythm pickers, over the core's `GardenRow`. The Mac's `GardenModel` and
//! `GardenStore`; the KDE window (B5c, B5d) and Antiope's garden pages (B8) draw them.

mod inspector;
mod visit;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use ressac_core::Result;
use ressac_core::model::{Action, CadenceGroup, Decision, ErrorKind, FeedId, Hop, Remark, Status};
use ressac_core::store::{AttemptRecord, CadenceDetail, Store, ThemeDef};

pub use inspector::*;
pub use ressac_core::garden::{GardenRow, Progress, progress, safe_repairs};
pub use visit::GardenVisit;

use crate::keys::qt;
use crate::sanitize::is_web_url;
use crate::sources::{GardenScope, rhythm_title};

/// Fetch-log lines the inspector shows.
pub const LOG_LINES: usize = 20;
pub const SELECT_FIRST: &str = "Select a feed first.";
pub const NO_SAFE_REPAIRS: &str = "No safe repairs pending.";
pub const NOTHING_TO_REPAIR: &str = "Nothing to repair for this feed.";
pub const BURIED_FIRST: &str = "This feed is buried; press k to restore it first.";
pub const NO_SITE: &str = "This feed has no site address.";
pub const NO_WAYBACK: &str = "No Wayback snapshot yet (run `ressac check`).";
pub const NON_WEB: &str = "Refusing to open a non-web address.";
pub const COULD_NOT_READ_GARDEN: &str = "The garden could not be read.";
pub const BURIED_GLYPH: &str = "⚰️";
pub const RHYTHM_CHOICES: [&str; 6] = ["Auto", "Daily", "Weekly", "Monthly", "Rare", "Silent"];

const UNCHECKED_GLYPH: &str = "❔";
const UNCHECKED_ID: &str = "Unchecked";
const BURIED_ID: &str = "Buried";
/// Rows the safe-repair dialog lists before "and N more.".
const PREVIEW_LINES: usize = 8;

pub fn status_glyph(status: Status) -> &'static str {
    match status {
        Status::Duplicate => "👯",
        Status::Hijacked => "🦠",
        Status::Moved => "🚚",
        Status::Dead => "🪦",
        Status::Ailing => "🤒",
        Status::Fossil => "🦴",
        Status::Dormant => "🌙",
        Status::Alive => "🌿",
        Status::Unchecked => UNCHECKED_GLYPH,
    }
}

pub fn rhythm_glyph(group: Option<CadenceGroup>) -> &'static str {
    match group {
        Some(CadenceGroup::Daily) => "⚡",
        Some(CadenceGroup::Weekly) => "📅",
        Some(CadenceGroup::Monthly) => "🌗",
        Some(CadenceGroup::Rare) => "✨",
        Some(CadenceGroup::Silent) => "💤",
        None => UNCHECKED_GLYPH,
    }
}

pub fn empty_message(scope: GardenScope) -> &'static str {
    match scope {
        GardenScope::NeedsTriage => "Nothing to triage",
        GardenScope::All => "No feeds",
        GardenScope::Cemetery => "The cemetery is empty",
    }
}

/// How All Feeds groups its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    #[default]
    Status,
    Rhythm,
}

impl Grouping {
    pub const ALL: [Grouping; 2] = [Grouping::Status, Grouping::Rhythm];

    pub fn title(self) -> &'static str {
        match self {
            Grouping::Status => "Status",
            Grouping::Rhythm => "Rhythm",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Grouping::Status => "status",
            Grouping::Rhythm => "rhythm",
        }
    }

    pub fn parse(text: &str) -> Option<Grouping> {
        Self::ALL.into_iter().find(|g| g.as_str() == text)
    }
}

/// One run of rows under a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub id: &'static str,
    pub glyph: &'static str,
    pub rows: Vec<FeedId>,
}

impl Section {
    /// `"🪦 DEAD · 8"`.
    pub fn header(&self) -> String {
        format!(
            "{} {} · {}",
            self.glyph,
            self.id.to_uppercase(),
            self.rows.len()
        )
    }
}

/// The rows `scope` lists: `retained` feeds (seen needing triage, or decided, during the
/// visit) stay listed after the decision.
pub fn visible_rows<'a>(
    rows: &'a [GardenRow],
    scope: GardenScope,
    retained: &HashSet<FeedId>,
) -> Vec<&'a GardenRow> {
    rows.iter()
        .filter(|r| {
            let kept = retained.contains(&r.feed_id);
            match scope {
                GardenScope::NeedsTriage => r.needs_triage() || kept,
                GardenScope::All => !r.buried || kept,
                GardenScope::Cemetery => r.buried,
            }
        })
        .collect()
}

/// A retained row that no longer belongs to the scope on its own is drawn dimmed.
pub fn is_dimmed(row: &GardenRow, scope: GardenScope, retained: &HashSet<FeedId>) -> bool {
    let kept = retained.contains(&row.feed_id);
    match scope {
        GardenScope::NeedsTriage => !row.needs_triage() && kept,
        GardenScope::All => row.buried && kept,
        GardenScope::Cemetery => false,
    }
}

/// Inside a section: top folder, then title ignoring case.
fn in_section_order(rows: &mut Vec<&GardenRow>) {
    rows.sort_by(|a, b| {
        (&a.top_folder, a.title.to_lowercase(), a.feed_id).cmp(&(
            &b.top_folder,
            b.title.to_lowercase(),
            b.feed_id,
        ))
    });
}

fn ids_in_order(mut rows: Vec<&GardenRow>) -> Vec<FeedId> {
    in_section_order(&mut rows);
    rows.into_iter().map(|r| r.feed_id).collect()
}

/// The Cemetery's one "Buried" section; otherwise a section per status in urgency order
/// (buried rows included), empty ones omitted.
pub fn sections(rows: &[&GardenRow], scope: GardenScope) -> Vec<Section> {
    if scope == GardenScope::Cemetery {
        let buried = ids_in_order(rows.iter().copied().filter(|r| r.buried).collect());
        return if buried.is_empty() {
            vec![]
        } else {
            vec![Section {
                id: BURIED_ID,
                glyph: BURIED_GLYPH,
                rows: buried,
            }]
        };
    }
    Status::ALL
        .into_iter()
        .filter_map(|status| {
            let ids = ids_in_order(
                rows.iter()
                    .copied()
                    .filter(|r| r.status() == status)
                    .collect(),
            );
            (!ids.is_empty()).then(|| Section {
                id: status.as_str(),
                glyph: status_glyph(status),
                rows: ids,
            })
        })
        .collect()
}

/// All Feeds grouped by rhythm: the five groups, then "Unchecked" (no rhythm row).
pub fn rhythm_sections(
    rows: &[&GardenRow],
    cadence: &HashMap<FeedId, CadenceDetail>,
) -> Vec<Section> {
    let group_of = |r: &GardenRow| cadence.get(&r.feed_id).map(|c| c.effective_group);
    CadenceGroup::ALL
        .into_iter()
        .map(Some)
        .chain([None])
        .filter_map(|group| {
            let ids = ids_in_order(
                rows.iter()
                    .copied()
                    .filter(|r| group_of(r) == group)
                    .collect(),
            );
            (!ids.is_empty()).then(|| Section {
                id: group.map_or(UNCHECKED_ID, rhythm_title),
                glyph: rhythm_glyph(group),
                rows: ids,
            })
        })
        .collect()
}

/// `"3 of 4 left"` in the queue, else `"1 feed"` / `"10 feeds"`.
pub fn count_label(scope: GardenScope, rows: &[GardenRow], visible: usize) -> String {
    if scope == GardenScope::NeedsTriage {
        let Progress { decided, needing } = progress(rows);
        return format!("{} of {needing} left", needing - decided);
    }
    match visible {
        1 => "1 feed".into(),
        n => format!("{n} feeds"),
    }
}

/// The dialog's lines: the first eight repairs, then "and N more.".
pub fn safe_repair_preview(rows: &[GardenRow], repairs: &[(FeedId, Action)]) -> String {
    let mut lines: Vec<String> = repairs
        .iter()
        .take(PREVIEW_LINES)
        .map(|(id, action)| {
            let title = rows
                .iter()
                .find(|r| r.feed_id == *id)
                .map_or("", |r| r.title.as_str());
            format!("{title}: {}", action.label())
        })
        .collect();
    if repairs.len() > PREVIEW_LINES {
        lines.push(format!("and {} more.", repairs.len() - PREVIEW_LINES));
    }
    lines.join("\n")
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub fn apply_button_title(count: usize) -> String {
    format!("Apply {}", plural(count, "Safe Repair", "Safe Repairs"))
}

pub fn apply_question(count: usize) -> String {
    format!("Apply {}?", plural(count, "safe repair", "safe repairs"))
}

pub fn apply_confirm_label(count: usize) -> &'static str {
    if count == 1 { "Apply" } else { "Apply All" }
}

/// When the selected feed left the queue with this update, the next feed (wrapping) in
/// `ordered` that still needs triage; otherwise the selection unchanged.
pub fn selection_after_update(
    selected: Option<FeedId>,
    previous: &[GardenRow],
    current: &[GardenRow],
    ordered: &[FeedId],
) -> Option<FeedId> {
    let id = selected?;
    let needs =
        |rows: &[GardenRow], id: FeedId| rows.iter().any(|r| r.feed_id == id && r.needs_triage());
    if !needs(previous, id) || needs(current, id) {
        return selected;
    }
    let Some(at) = ordered.iter().position(|f| *f == id) else {
        return selected;
    };
    (1..ordered.len())
        .map(|step| ordered[(at + step) % ordered.len()])
        .find(|next| needs(current, *next))
        .or(selected)
}

/// What a garden key asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Decide(Decision),
    Undo,
    Open(String),
    ConfirmSafeRepairs,
    Message(&'static str),
}

/// The terminal garden's keys. `modifiers` is Qt's mask (`keys::qt`): a Control, Alt or Meta
/// chord is not the garden's (Ctrl+B never buries).
pub fn command(
    key: &str,
    modifiers: u32,
    row: Option<&GardenRow>,
    safe_repair_count: usize,
) -> Option<Command> {
    if modifiers & (qt::CONTROL | qt::ALT | qt::META) != 0 {
        return None;
    }
    match key {
        "u" => return Some(Command::Undo),
        "R" => {
            return Some(if safe_repair_count == 0 {
                Command::Message(NO_SAFE_REPAIRS)
            } else {
                Command::ConfirmSafeRepairs
            });
        }
        "k" | "r" | "b" | "o" | "w" => {}
        _ => return None,
    }
    let Some(row) = row else {
        return Some(Command::Message(SELECT_FIRST));
    };
    Some(match key {
        "k" => Command::Decide(Decision::Keep),
        "r" | "b" if row.buried => Command::Message(BURIED_FIRST),
        "b" => Command::Decide(Decision::Bury),
        "r" if row.action().is_some_and(Action::is_repair) => Command::Decide(Decision::Repair),
        "r" => Command::Message(NOTHING_TO_REPAIR),
        "o" => open(row.html_url.as_deref(), NO_SITE),
        _ => open(row.wayback_url.as_deref(), NO_WAYBACK),
    })
}

fn open(address: Option<&str>, missing: &'static str) -> Command {
    match address {
        Some(url) if is_web_url(url) => Command::Open(url.into()),
        Some(_) => Command::Message(NON_WEB),
        None => Command::Message(missing),
    }
}

/// The row's chip: its decision once made, else its proposed action, else "wait".
pub fn chip(row: &GardenRow) -> Option<&'static str> {
    if let Some(record) = row.decision.as_ref().filter(|_| !row.needs_triage()) {
        return Some(match record.decision {
            Decision::Keep => "kept",
            Decision::Repair => "repaired",
            Decision::Bury => "buried",
        });
    }
    match row.action() {
        Some(Action::UpgradeHttps { .. }) => Some("https"),
        Some(Action::FollowRedirect { .. }) => Some("follow"),
        Some(Action::AdoptDiscovered { .. }) => Some("adopt"),
        Some(Action::MergeInto { .. }) => Some("merge"),
        Some(Action::Bury) => Some("bury"),
        None => row.status().needs_triage().then_some("wait"),
    }
}

/// `"12 unread"`, buried feeds only (`v_unread_count` covers active feeds).
pub fn unread_label(row: &GardenRow, unread: usize) -> Option<String> {
    (row.buried && unread > 0).then(|| format!("{unread} unread"))
}

/// The folder path, then the reason; an empty part is left out.
pub fn tooltip(row: &GardenRow) -> String {
    let reason = row.diagnosis.as_ref().map_or("", |d| d.reason.as_str());
    [row.folder_path.as_str(), reason]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `"🪦 Dead"`.
pub fn status_line(status: Status) -> String {
    format!("{} {}", status_glyph(status), status.as_str())
}

pub fn remark_sentence(remark: &Remark) -> String {
    match remark {
        Remark::HttpsAvailable => "The https:// address serves the same feed.".into(),
        Remark::HttpOnly => "Only served over http://.".into(),
        Remark::SimilarTitle { feed_id } => {
            format!("A feed in the same folder has a similar title (#{feed_id}).")
        }
        Remark::DateFallback => "Most entries carry no date; Ressac used the fetch date.".into(),
        Remark::Blocked => "The last fetch was blocked (403 or an anti-bot page).".into(),
    }
}

/// A fetch-log line's result: the error's label, else the HTTP status, else "no response".
pub fn outcome(attempt: &AttemptRecord) -> String {
    match (&attempt.error, attempt.http_status) {
        (Some(error), _) => ErrorKind::label(error),
        (None, Some(status)) => format!("HTTP {status}"),
        (None, None) => "no response".into(),
    }
}

/// `"301 → https://…"`.
pub fn redirect_line(hop: &Hop) -> String {
    format!("{} → {}", hop.status, hop.url)
}

/// `"Daily"`, `"Daily*"` when set by hand; `None` without a rhythm row.
pub fn rhythm_label(cadence: Option<&CadenceDetail>) -> Option<String> {
    cadence.map(|c| {
        let title = rhythm_title(c.effective_group);
        if c.is_override {
            format!("{title}*")
        } else {
            title.into()
        }
    })
}

/// `"Daily · median 0.4 d · last entry 2 h ago (set by you)"`.
pub fn rhythm_text(cadence: &CadenceDetail, now: DateTime<Utc>) -> String {
    let mut parts = vec![rhythm_title(cadence.effective_group).to_string()];
    if let Some(median) = cadence.median_interval_days {
        parts.push(if median < 10.0 {
            format!("median {median:.1} d")
        } else {
            format!("median {} d", median.round() as i64)
        });
    }
    if let Some(last) = cadence.last_entry_at {
        parts.push(format!("last entry {}", ago(last, now)));
    }
    let mut text = parts.join(" · ");
    if cadence.is_override {
        text.push_str(" (set by you)");
    }
    text
}

pub fn rhythm_line(cadence: &CadenceDetail, now: DateTime<Utc>) -> String {
    format!("Rhythm: {}", rhythm_text(cadence, now))
}

/// Minutes under an hour, hours under 48, days under 60, then months of 30 days.
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let age = now - then;
    let minutes = age.num_minutes().max(0);
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    let hours = age.num_hours();
    if hours < 48 {
        return format!("{hours} h ago");
    }
    let days = age.num_days();
    if days < 60 {
        format!("{days} d ago")
    } else {
        format!("{} mo ago", days / 30)
    }
}

/// The Rhythm menu's current item: 0 Auto, else 1 + the group's place.
pub fn rhythm_index(cadence: &CadenceDetail) -> usize {
    if !cadence.is_override {
        return 0;
    }
    1 + CadenceGroup::ALL
        .iter()
        .position(|g| *g == cadence.effective_group)
        .unwrap_or(0)
}

/// The menu item `index` asks for: `Some(None)` is Auto.
pub fn rhythm_choice(index: usize) -> Option<Option<CadenceGroup>> {
    match index {
        0 => Some(None),
        n => CadenceGroup::ALL.get(n - 1).map(|g| Some(*g)),
    }
}

/// "None", then the themes by position (the order `Store::themes` returns).
pub fn theme_names(themes: &[ThemeDef]) -> Vec<String> {
    std::iter::once("None".to_string())
        .chain(themes.iter().map(|t| t.name.clone()))
        .collect()
}

/// The Theme menu's current item: 0 (None) when the feed has no theme or an unknown one.
pub fn theme_index(themes: &[ThemeDef], key: Option<&str>) -> usize {
    key.and_then(|k| themes.iter().position(|t| t.key == k))
        .map_or(0, |i| i + 1)
}

/// The menu item `index` asks for: `Some(None)` clears the theme.
pub fn theme_choice(themes: &[ThemeDef], index: usize) -> Option<Option<String>> {
    match index {
        0 => Some(None),
        n => themes.get(n - 1).map(|t| Some(t.key.clone())),
    }
}

/// Everything the garden reads from the database in one pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub rows: Vec<GardenRow>,
    /// Unread entries of buried feeds.
    pub unread: HashMap<FeedId, usize>,
    pub cadence: HashMap<FeedId, CadenceDetail>,
    pub themes: Vec<ThemeDef>,
    pub theme_keys: HashMap<FeedId, String>,
}

pub fn load(store: &Store) -> Result<Snapshot> {
    Ok(Snapshot {
        rows: ressac_core::garden::load_rows(store)?,
        unread: store.buried_unread()?,
        cadence: store
            .cadence_details()?
            .into_iter()
            .map(|c| (c.feed_id, c))
            .collect(),
        themes: store.themes()?,
        theme_keys: store.feed_theme_keys()?,
    })
}

/// A decision run's job: the core's refusals first, then the write.
pub fn decide(store: &Store, feed: FeedId, decision: Decision, now: DateTime<Utc>) -> Result<()> {
    ressac_core::garden::check_decision(store, feed, decision)?;
    ressac_core::garden::decide(store, feed, decision, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::qt;
    use crate::runs::RunRequest;
    use crate::test_db::{TestDb, at};
    use crate::worker::JobFailure;
    use Status::*;
    use chrono::TimeDelta;
    use ressac_core::model::{Cadence, Diagnosis, ErrorKind, Evidence, Hop, Remark};
    use ressac_core::store::{AttemptRecord, DecisionRecord, ThemeDef};

    fn row(id: FeedId, title: &str, status: Option<Status>) -> GardenRow {
        GardenRow {
            feed_id: id,
            title: title.into(),
            folder_path: "Blogs".into(),
            top_folder: "Blogs".into(),
            xml_url: format!("https://feeds.example.org/{id}"),
            html_url: None,
            wayback_url: None,
            buried: false,
            diagnosis: status.map(|status| Diagnosis {
                status,
                reason: String::new(),
                evidence: Evidence::default(),
                remarks: vec![],
                action: None,
                hijack_score: 0.0,
                cadence: Cadence::default(),
            }),
            decision: None,
            recent: vec![],
            attempts: vec![],
        }
    }

    trait RowExt {
        fn folder(self, path: Option<&str>) -> Self;
        fn buried(self) -> Self;
        fn action(self, action: Action) -> Self;
        fn decided(self, decision: Decision) -> Self;
        fn site(self, url: &str) -> Self;
        fn wayback(self, url: &str) -> Self;
    }

    impl RowExt for GardenRow {
        fn folder(mut self, path: Option<&str>) -> Self {
            self.folder_path = path.unwrap_or_default().into();
            self.top_folder = self
                .folder_path
                .split(" / ")
                .next()
                .unwrap_or_default()
                .into();
            self
        }
        fn buried(mut self) -> Self {
            self.buried = true;
            self
        }
        fn action(mut self, action: Action) -> Self {
            self.diagnosis.as_mut().expect("a diagnosed row").action = Some(action);
            self
        }
        fn decided(mut self, decision: Decision) -> Self {
            self.decision = Some(DecisionRecord {
                id: 1,
                feed_id: self.feed_id,
                decision,
                status_at_decision: self.status(),
                action: None,
                decided_at: at("2026-10-10T08:00:00Z"),
            });
            self
        }
        fn site(mut self, url: &str) -> Self {
            self.html_url = Some(url.into());
            self
        }
        fn wayback(mut self, url: &str) -> Self {
            self.wayback_url = Some(url.into());
            self
        }
    }

    fn cadence(
        id: FeedId,
        group: CadenceGroup,
        overridden: bool,
        median: Option<f64>,
        last: Option<DateTime<Utc>>,
    ) -> CadenceDetail {
        CadenceDetail {
            feed_id: id,
            effective_group: group,
            computed_group: group,
            is_override: overridden,
            median_interval_days: median,
            entries_per_month: 0.0,
            last_entry_at: last,
            unread: 0,
        }
    }

    fn follow(url: &str) -> Action {
        Action::FollowRedirect { url: url.into() }
    }

    #[test]
    fn only_buried_feeds_show_their_unread_count() {
        assert_eq!(
            unread_label(&row(4, "Gone", Some(Dead)).buried(), 12).as_deref(),
            Some("12 unread")
        );
        assert_eq!(unread_label(&row(4, "Gone", Some(Dead)).buried(), 0), None);
        assert_eq!(unread_label(&row(1, "Live", Some(Alive)), 3), None);
    }

    #[test]
    fn sections_follow_urgency_then_folder_then_title() {
        let rows = [
            row(1, "Zed", Some(Dead)),
            row(2, "alpha", Some(Dead)),
            row(3, "Mover", Some(Moved)).folder(Some("Infos")),
            row(4, "Fine", Some(Alive)),
            row(5, "Gone", Some(Dead)).buried(),
            row(6, "New", None).folder(None),
            row(7, "Early", Some(Dead)).folder(Some("Biblis / Sub")),
        ];
        let all: Vec<&GardenRow> = rows.iter().collect();
        let s = sections(&all, GardenScope::All);
        assert_eq!(
            s.iter().map(|s| s.id).collect::<Vec<_>>(),
            ["Moved", "Dead", "Alive", "Unchecked"]
        );
        assert_eq!(s[1].rows, [7, 2, 5, 1]);
        assert_eq!(s[1].header(), "🪦 DEAD · 4");
        let buried: Vec<&GardenRow> = rows.iter().filter(|r| r.buried).collect();
        let cemetery = sections(&buried, GardenScope::Cemetery);
        assert_eq!(
            (cemetery.len(), cemetery[0].id, cemetery[0].rows.as_slice()),
            (1, "Buried", &[5][..])
        );
        assert_eq!(cemetery[0].header(), "⚰️ BURIED · 1");
        let queue = sections(&all, GardenScope::NeedsTriage);
        assert_eq!(
            queue.iter().find(|s| s.id == "Dead").unwrap().rows,
            [7, 2, 5, 1]
        );
        assert!(!queue.iter().any(|s| s.id == "Buried"));
    }

    #[test]
    fn the_queue_keeps_feeds_decided_during_the_visit_dimmed() {
        let rows = [
            row(1, "Waiting", Some(Dead)),
            row(2, "Decided", Some(Dead)).decided(Decision::Keep),
            row(3, "Decided last week", Some(Fossil)).decided(Decision::Keep),
        ];
        let ids = |v: Vec<&GardenRow>| v.iter().map(|r| r.feed_id).collect::<Vec<_>>();
        assert_eq!(
            ids(visible_rows(
                &rows,
                GardenScope::NeedsTriage,
                &HashSet::from([1, 2])
            )),
            [1, 2]
        );
        assert_eq!(
            ids(visible_rows(
                &rows,
                GardenScope::NeedsTriage,
                &HashSet::from([1])
            )),
            [1]
        );
        assert!(is_dimmed(
            &rows[1],
            GardenScope::NeedsTriage,
            &HashSet::from([1, 2])
        ));
        assert!(!is_dimmed(
            &rows[0],
            GardenScope::NeedsTriage,
            &HashSet::from([1, 2])
        ));
        assert!(!is_dimmed(
            &rows[1],
            GardenScope::All,
            &HashSet::from([1, 2])
        ));
        let with_buried = [rows[0].clone(), row(4, "Gone", Some(Dead)).buried()];
        assert_eq!(
            ids(visible_rows(
                &with_buried,
                GardenScope::Cemetery,
                &HashSet::new()
            )),
            [4]
        );
    }

    #[test]
    fn all_feeds_keeps_a_feed_buried_during_the_visit_dimmed() {
        let rows = [
            row(1, "Active", Some(Alive)),
            row(2, "Buried now", Some(Dead))
                .buried()
                .decided(Decision::Bury),
            row(3, "Buried before", Some(Dead))
                .buried()
                .decided(Decision::Bury),
        ];
        let ids = |v: Vec<&GardenRow>| v.iter().map(|r| r.feed_id).collect::<Vec<_>>();
        assert_eq!(
            ids(visible_rows(&rows, GardenScope::All, &HashSet::from([2]))),
            [1, 2]
        );
        assert_eq!(
            ids(visible_rows(&rows, GardenScope::All, &HashSet::new())),
            [1]
        );
        assert!(is_dimmed(&rows[1], GardenScope::All, &HashSet::from([2])));
        assert!(!is_dimmed(&rows[0], GardenScope::All, &HashSet::from([2])));
    }

    /// Review Focus 1.
    #[test]
    fn garden_keys_ignore_control_alt_and_meta_chords() {
        let dead = row(1, "Dead", Some(Dead));
        for chord in [qt::CONTROL, qt::ALT, qt::META, qt::CONTROL | qt::SHIFT] {
            for key in ["b", "k", "u", "R", "o"] {
                assert_eq!(
                    command(key, chord, Some(&dead), 1),
                    None,
                    "{key} {chord:#x}"
                );
            }
        }
        assert_eq!(
            command("R", qt::SHIFT, Some(&dead), 1),
            Some(Command::ConfirmSafeRepairs)
        );
        assert_eq!(
            command("b", 0, Some(&dead), 0),
            Some(Command::Decide(Decision::Bury))
        );
    }

    #[test]
    fn progress_counts_like_the_terminal_garden() {
        let rows = [
            row(1, "Dead", Some(Dead)),
            row(2, "Fossil kept", Some(Fossil)).decided(Decision::Keep),
            row(3, "Buried", Some(Dead))
                .buried()
                .decided(Decision::Bury),
            row(4, "Alive", Some(Alive)),
            row(5, "Unchecked", None),
        ];
        assert_eq!(
            count_label(GardenScope::NeedsTriage, &rows, 2),
            "1 of 3 left"
        );
        assert_eq!(count_label(GardenScope::All, &rows, 1), "1 feed");
        assert_eq!(count_label(GardenScope::Cemetery, &rows, 4), "4 feeds");
    }

    #[test]
    fn safe_repairs_are_undecided_active_safe_actions() {
        let https = Action::UpgradeHttps {
            url: "https://a.example.org/feed".into(),
        };
        let rows = vec![
            row(1, "A", Some(Alive)).action(https.clone()),
            row(2, "B", Some(Moved)).action(follow("https://b.example.org/feed")),
            row(3, "C", Some(Moved))
                .action(follow("https://c.example.org/feed"))
                .decided(Decision::Keep),
            row(4, "D", Some(Alive)).action(https.clone()).buried(),
            row(5, "E", Some(Duplicate)).action(Action::MergeInto { feed_id: 1 }),
            row(6, "F", Some(Dead)).action(Action::Bury),
        ];
        let repairs = safe_repairs(&rows);
        assert_eq!(
            repairs.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            safe_repair_preview(&rows, &repairs),
            "A: switch to https://a.example.org/feed\nB: follow redirect to https://b.example.org/feed"
        );
        let many: Vec<GardenRow> = (1..=87)
            .map(|i| row(i, &format!("Feed {i}"), Some(Alive)).action(https.clone()))
            .collect();
        let preview = safe_repair_preview(&many, &safe_repairs(&many));
        assert_eq!(preview.lines().count(), 9);
        assert_eq!(preview.lines().last(), Some("and 79 more."));
        assert_eq!(
            [apply_button_title(1), apply_button_title(87)],
            ["Apply 1 Safe Repair", "Apply 87 Safe Repairs"]
        );
        assert_eq!(
            [apply_question(1), apply_question(87)],
            ["Apply 1 safe repair?", "Apply 87 safe repairs?"]
        );
        assert_eq!(
            [apply_confirm_label(1), apply_confirm_label(87)],
            ["Apply", "Apply All"]
        );
    }

    #[test]
    fn selection_moves_on_when_the_selected_feed_leaves_the_queue() {
        let before = [
            row(1, "A", Some(Dead)),
            row(2, "B", Some(Dead)).decided(Decision::Keep),
            row(3, "C", Some(Dead)),
        ];
        let order = [1, 2, 3];
        let after = [
            before[0].clone().decided(Decision::Keep),
            before[1].clone(),
            before[2].clone(),
        ];
        assert_eq!(
            selection_after_update(Some(1), &before, &after, &order),
            Some(3)
        );
        let last = [
            before[0].clone(),
            before[1].clone(),
            before[2].clone().decided(Decision::Keep),
        ];
        assert_eq!(
            selection_after_update(Some(3), &before, &last, &order),
            Some(1)
        ); // wraps
        assert_eq!(
            selection_after_update(Some(3), &before, &before, &order),
            Some(3)
        );
        let done = before.clone().map(|r| r.decided(Decision::Keep));
        assert_eq!(
            selection_after_update(Some(1), &before, &done, &order),
            Some(1)
        );
        assert_eq!(selection_after_update(None, &before, &after, &order), None);
    }

    /// Review Focus 3.
    #[test]
    fn keys_map_like_the_terminal_garden() {
        let dead = row(1, "Dead", Some(Dead))
            .action(Action::Bury)
            .site("https://dead.example.org/")
            .wayback("http://web.archive.org/web/2021/https://dead.example.org/");
        let fossil = row(2, "Fossil", Some(Fossil))
            .site("javascript:alert(1)")
            .wayback("file:///etc/passwd");
        let buried = row(3, "Buried", Some(Dead))
            .buried()
            .decided(Decision::Bury);
        let moved = row(4, "Moved", Some(Moved)).action(follow("https://m.example.org/feed"));
        let c = |key: &str, r: Option<&GardenRow>, n| command(key, 0, r, n);
        assert_eq!(
            c("k", Some(&dead), 0),
            Some(Command::Decide(Decision::Keep))
        );
        assert_eq!(
            c("b", Some(&dead), 0),
            Some(Command::Decide(Decision::Bury))
        );
        assert_eq!(
            c("r", Some(&moved), 0),
            Some(Command::Decide(Decision::Repair))
        );
        assert_eq!(
            c("r", Some(&dead), 0),
            Some(Command::Message("Nothing to repair for this feed."))
        );
        assert_eq!(
            c("r", Some(&fossil), 0),
            Some(Command::Message("Nothing to repair for this feed."))
        );
        assert_eq!(
            c("k", Some(&buried), 0),
            Some(Command::Decide(Decision::Keep))
        );
        assert_eq!(
            c("b", Some(&buried), 0),
            Some(Command::Message(
                "This feed is buried; press k to restore it first."
            ))
        );
        assert_eq!(
            c("r", Some(&buried), 0),
            Some(Command::Message(
                "This feed is buried; press k to restore it first."
            ))
        );
        assert_eq!(
            c("o", Some(&dead), 0),
            Some(Command::Open("https://dead.example.org/".into()))
        );
        assert_eq!(
            c("w", Some(&dead), 0),
            Some(Command::Open(
                "http://web.archive.org/web/2021/https://dead.example.org/".into()
            ))
        );
        assert_eq!(
            c("o", Some(&fossil), 0),
            Some(Command::Message("Refusing to open a non-web address."))
        );
        assert_eq!(
            c("w", Some(&fossil), 0),
            Some(Command::Message("Refusing to open a non-web address."))
        );
        assert_eq!(
            c("o", Some(&moved), 0),
            Some(Command::Message("This feed has no site address."))
        );
        assert_eq!(
            c("w", Some(&moved), 0),
            Some(Command::Message(
                "No Wayback snapshot yet (run `ressac check`)."
            ))
        );
        assert_eq!(c("u", None, 0), Some(Command::Undo));
        assert_eq!(c("R", None, 3), Some(Command::ConfirmSafeRepairs));
        assert_eq!(
            c("R", None, 0),
            Some(Command::Message("No safe repairs pending."))
        );
        assert_eq!(
            c("k", None, 0),
            Some(Command::Message("Select a feed first."))
        );
        assert_eq!(c("j", Some(&dead), 0), None);
        assert_eq!(
            c("K", Some(&dead), 0),
            None,
            "Caps Lock: not the garden's k"
        );
    }

    #[test]
    fn labels_match_the_core() {
        assert_eq!(
            chip(&row(1, "A", Some(Duplicate)).action(Action::MergeInto { feed_id: 2 })),
            Some("merge")
        );
        assert_eq!(chip(&row(1, "A", Some(Ailing))), Some("wait"));
        assert_eq!(chip(&row(1, "A", Some(Alive))), None);
        assert_eq!(
            chip(
                &row(1, "A", Some(Moved))
                    .action(follow("x"))
                    .decided(Decision::Repair)
            ),
            Some("repaired")
        );
        assert_eq!(
            chip(&row(1, "A", Some(Dead)).buried().decided(Decision::Bury)),
            Some("buried")
        );
        assert_eq!(
            remark_sentence(&Remark::SimilarTitle { feed_id: 12 }),
            "A feed in the same folder has a similar title (#12)."
        );
        assert_eq!(
            remark_sentence(&Remark::HttpsAvailable),
            "The https:// address serves the same feed."
        );
        let attempt = |status: Option<u16>, error: Option<ErrorKind>| AttemptRecord {
            at: at("2026-10-10T07:00:00Z"),
            http_status: status,
            error,
            redirect_chain: vec![],
            content_type: None,
            network_suspect: false,
            not_modified: false,
            entry_count: None,
        };
        assert_eq!(outcome(&attempt(None, Some(ErrorKind::Dns))), "Dns");
        assert_eq!(
            outcome(&attempt(Some(404), Some(ErrorKind::Http(404)))),
            "HTTP 404"
        );
        assert_eq!(
            outcome(&attempt(Some(200), Some(ErrorKind::HtmlInsteadOfFeed))),
            "HTML page instead of a feed"
        );
        assert_eq!(
            outcome(&attempt(Some(403), Some(ErrorKind::Challenge))),
            "anti-bot challenge page"
        );
        assert_eq!(
            outcome(&attempt(Some(200), Some(ErrorKind::TooLarge))),
            "response too large"
        );
        assert_eq!(outcome(&attempt(None, Some(ErrorKind::Timeout))), "Timeout");
        assert_eq!(outcome(&attempt(Some(200), None)), "HTTP 200");
        assert_eq!(outcome(&attempt(None, None)), "no response");
        assert_eq!(
            redirect_line(&Hop {
                status: 301,
                url: "https://e.org/feed".into()
            }),
            "301 → https://e.org/feed"
        );
        assert_eq!(status_line(Dead), "🪦 Dead");
        assert_eq!(
            tooltip(&row(1, "A", Some(Dead)).folder(Some("Blogs / potes"))),
            "Blogs / potes",
            "an empty reason is left out"
        );
        assert_eq!(
            [
                empty_message(GardenScope::NeedsTriage),
                empty_message(GardenScope::All),
                empty_message(GardenScope::Cemetery)
            ],
            ["Nothing to triage", "No feeds", "The cemetery is empty"]
        );
    }

    #[test]
    fn the_rhythm_line_reads_like_the_spec() {
        let now = at("2026-10-16T09:00:00Z");
        let two_hours = Some(now - TimeDelta::hours(2));
        assert_eq!(
            rhythm_line(
                &cadence(1, CadenceGroup::Daily, false, Some(0.4), two_hours),
                now
            ),
            "Rhythm: Daily · median 0.4 d · last entry 2 h ago"
        );
        assert_eq!(
            rhythm_line(
                &cadence(1, CadenceGroup::Daily, true, Some(0.4), two_hours),
                now
            ),
            "Rhythm: Daily · median 0.4 d · last entry 2 h ago (set by you)"
        );
        assert_eq!(
            rhythm_line(
                &cadence(
                    2,
                    CadenceGroup::Monthly,
                    false,
                    Some(12.3),
                    Some(now - TimeDelta::days(3))
                ),
                now
            ),
            "Rhythm: Monthly · median 12 d · last entry 3 d ago"
        );
        assert_eq!(
            rhythm_line(&cadence(3, CadenceGroup::Silent, false, None, None), now),
            "Rhythm: Silent"
        );
        assert_eq!(
            rhythm_text(
                &cadence(1, CadenceGroup::Daily, true, Some(0.4), two_hours),
                now
            ),
            "Daily · median 0.4 d · last entry 2 h ago (set by you)"
        );
    }

    #[test]
    fn ages_count_minutes_hours_days_then_months() {
        let now = at("2026-10-16T09:00:00Z");
        assert_eq!(ago(now - TimeDelta::minutes(45), now), "45 min ago");
        assert_eq!(ago(now - TimeDelta::hours(47), now), "47 h ago");
        assert_eq!(ago(now - TimeDelta::days(59), now), "59 d ago");
        assert_eq!(ago(now - TimeDelta::days(400), now), "13 mo ago");
        assert_eq!(ago(now + TimeDelta::minutes(1), now), "0 min ago"); // a clock ahead of the feed
    }

    #[test]
    fn the_label_marks_a_group_set_by_hand() {
        assert_eq!(
            rhythm_label(Some(&cadence(1, CadenceGroup::Daily, false, None, None))).as_deref(),
            Some("Daily")
        );
        assert_eq!(
            rhythm_label(Some(&cadence(1, CadenceGroup::Rare, true, None, None))).as_deref(),
            Some("Rare*")
        );
        assert_eq!(rhythm_label(None), None);
    }

    #[test]
    fn all_feeds_groups_by_rhythm_then_unchecked() {
        let rows = [
            row(1, "Zeta", Some(Alive)),
            row(2, "Alpha", Some(Dead)),
            row(3, "Mid", Some(Alive)),
            row(4, "New", None),
        ];
        let all: Vec<&GardenRow> = rows.iter().collect();
        let groups = HashMap::from([
            (1, cadence(1, CadenceGroup::Daily, false, None, None)),
            (2, cadence(2, CadenceGroup::Daily, false, None, None)),
            (3, cadence(3, CadenceGroup::Rare, true, None, None)),
        ]);
        let s = rhythm_sections(&all, &groups);
        assert_eq!(
            s.iter().map(|s| s.rows.clone()).collect::<Vec<_>>(),
            [vec![2, 1], vec![3], vec![4]]
        );
        assert_eq!(
            s.iter().map(Section::header).collect::<Vec<_>>(),
            ["⚡ DAILY · 2", "✨ RARE · 1", "❔ UNCHECKED · 1"]
        );
        assert_eq!(Grouping::ALL.map(Grouping::title), ["Status", "Rhythm"]);
    }

    #[test]
    fn the_rhythm_menu_is_auto_then_the_five_groups() {
        assert_eq!(
            RHYTHM_CHOICES,
            ["Auto", "Daily", "Weekly", "Monthly", "Rare", "Silent"]
        );
        assert_eq!(
            rhythm_index(&cadence(1, CadenceGroup::Rare, true, None, None)),
            4
        );
        assert_eq!(
            rhythm_index(&cadence(1, CadenceGroup::Rare, false, None, None)),
            0
        );
        assert_eq!(rhythm_choice(0), Some(None));
        assert_eq!(rhythm_choice(1), Some(Some(CadenceGroup::Daily)));
        assert_eq!(rhythm_choice(5), Some(Some(CadenceGroup::Silent)));
        assert_eq!(rhythm_choice(6), None);
    }

    #[test]
    fn the_theme_menu_is_none_then_the_themes_by_position() {
        let theme = |key: &str, name: &str| ThemeDef {
            key: key.into(),
            name: name.into(),
            sea: format!("SEA OF {name}"),
        };
        let themes = [theme("lit", "Literature"), theme("tech", "Technology")]; // as `Store::themes` orders them
        assert_eq!(theme_names(&themes), ["None", "Literature", "Technology"]);
        assert_eq!(
            [
                theme_index(&themes, Some("tech")),
                theme_index(&themes, None),
                theme_index(&themes, Some("gone"))
            ],
            [2, 0, 0]
        );
        assert_eq!(theme_choice(&themes, 1), Some(Some("lit".to_string())));
        assert_eq!(theme_choice(&themes, 0), Some(None));
        assert_eq!(theme_choice(&themes, 3), None);
    }

    #[test]
    fn the_theme_menu_writes_the_feed_theme() {
        let db = TestDb::full();
        db.execute("INSERT INTO theme (key, name, sea_name, position) VALUES ('tech', 'Technology', 'SEA OF THE MACHINES', 2), \
                ('lit', 'Literature', 'SEA OF LETTERS', 1)");
        let store = db.store();
        store.set_feed_theme(8, Some("lit")).unwrap();
        let snapshot = load(&store).unwrap();
        assert_eq!(
            theme_names(&snapshot.themes),
            ["None", "Literature", "Technology"]
        );
        assert_eq!(snapshot.theme_keys.get(&8).map(String::as_str), Some("lit"));
        store.set_feed_theme(8, None).unwrap();
        assert!(load(&store).unwrap().theme_keys.is_empty());
    }

    /// Review Focus 4.
    #[test]
    fn a_decision_is_checked_before_it_is_recorded() {
        let db = TestDb::full();
        let store = db.store();
        let refused = decide(&store, 4, Decision::Repair, at("2026-10-16T09:00:00Z")).unwrap_err();
        assert_eq!(
            refused.to_string(),
            "Feed #4 is buried; only keep restores it."
        );
        assert!(
            RunRequest::Decide(4, Decision::Repair)
                .failure(&JobFailure::Core(refused))
                .refusal
        );
        decide(&store, 6, Decision::Keep, at("2026-10-16T09:00:00Z")).unwrap();
        let rows = load(&store).unwrap().rows;
        let six = rows.iter().find(|r| r.feed_id == 6).unwrap();
        assert_eq!(
            six.decision.as_ref().map(|d| d.decision),
            Some(Decision::Keep)
        );
        assert!(!six.needs_triage());
    }

    #[test]
    fn load_reads_the_fixture_like_the_mac_app() {
        let db = TestDb::full();
        db.entry(4, "2026-10-09T08:00:00.000Z", false);
        db.entry(4, "2026-10-09T09:00:00.000Z", false);
        db.entry(4, "2026-10-09T10:00:00.000Z", true);
        let s = load(&db.store()).unwrap();
        let by_id = |id| s.rows.iter().find(|r| r.feed_id == id).unwrap();
        assert_eq!(
            (
                by_id(5).status(),
                by_id(5).diagnosis.as_ref().unwrap().evidence.clue.as_str()
            ),
            (Moved, "301")
        );
        assert_eq!(
            by_id(5).action(),
            Some(&Action::FollowRedirect {
                url: "https://girl.example.org/feed".into()
            })
        );
        assert_eq!(
            (by_id(6).status(), by_id(6).action()),
            (Hijacked, Some(&Action::Bury))
        );
        assert_eq!(
            by_id(7).decision.as_ref().map(|d| d.decision),
            Some(Decision::Keep)
        );
        assert_eq!(
            by_id(7).wayback_url.as_deref(),
            Some("http://web.archive.org/web/2019/http://www.unphotographable.com/")
        );
        assert!(by_id(4).buried);
        let mut queue: Vec<FeedId> = s
            .rows
            .iter()
            .filter(|r| r.needs_triage())
            .map(|r| r.feed_id)
            .collect();
        queue.sort();
        assert_eq!(queue, [2, 5, 6]);
        assert_eq!(s.unread, HashMap::from([(4, 2)]), "two unread, one read");
        assert_eq!(
            s.cadence
                .get(&6)
                .map(|c| (c.effective_group, c.is_override)),
            Some((CadenceGroup::Weekly, true))
        );
        assert_eq!(
            unread_label(by_id(4), s.unread[&4]).as_deref(),
            Some("2 unread")
        );
    }
}
