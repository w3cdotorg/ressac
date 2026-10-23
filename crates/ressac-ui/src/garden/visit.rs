//! The garden's visit: the Mac's `GardenStore`, without the look. Entering a scope starts a
//! visit that remembers the feeds seen in the queue and those decided here, so they stay listed
//! (dimmed) until the scope is left; the selection moves on as feeds leave the queue.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;

use chrono::{DateTime, TimeZone, Utc};
use ressac_core::model::{Action, FeedId};
use ressac_core::store::AttemptRecord;

use super::{
    Command, GardenRow, Grouping, Inspector, Section, Snapshot, command, count_label,
    empty_message, is_dimmed, rhythm_sections, safe_repairs, sections, selection_after_update,
    visible_rows,
};
use crate::sources::GardenScope;

#[derive(Debug, Clone, Default)]
pub struct GardenVisit {
    scope: Option<GardenScope>,
    /// Counts the visits: a run that outlives its visit compares it before touching the next.
    epoch: u64,
    snapshot: Snapshot,
    /// Every feed seen needing triage during the visit.
    seen_in_queue: HashSet<FeedId>,
    /// Feeds decided from this window during the visit.
    decided_this_visit: HashSet<FeedId>,
    /// `seen_in_queue` and `decided_this_visit` together, kept up to date where they change
    /// (the table asks per row).
    retained: HashSet<FeedId>,
    /// Each snapshot row's position, by feed.
    index: HashMap<FeedId, usize>,
    selected: Option<FeedId>,
    grouping: Grouping,
    message: Option<&'static str>,
    confirming: bool,
}

impl GardenVisit {
    /// Another scope starts a visit; the same scope does nothing. The grouping is kept.
    pub fn enter(&mut self, scope: GardenScope) {
        if self.scope == Some(scope) {
            return;
        }
        self.reset();
        self.epoch += 1;
        self.scope = Some(scope);
    }

    pub fn leave(&mut self) {
        self.reset();
        self.epoch += 1;
        self.scope = None;
    }

    /// Identifies the current visit (it changes when another scope is entered or the garden
    /// is left).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    fn reset(&mut self) {
        self.snapshot = Snapshot::default();
        self.seen_in_queue.clear();
        self.decided_this_visit.clear();
        self.retained.clear();
        self.index.clear();
        self.selected = None;
        self.message = None;
        self.confirming = false;
    }

    pub fn scope(&self) -> Option<GardenScope> {
        self.scope
    }

    /// The Mac's `rowsDidChange`: remember the queue, then keep the selection on a feed to triage.
    pub fn update(&mut self, snapshot: Snapshot) {
        let previous = std::mem::replace(&mut self.snapshot, snapshot);
        self.seen_in_queue.extend(
            self.snapshot
                .rows
                .iter()
                .filter(|r| r.needs_triage())
                .map(|r| r.feed_id),
        );
        self.index = self
            .snapshot
            .rows
            .iter()
            .enumerate()
            .map(|(at, r)| (r.feed_id, at))
            .collect();
        self.refresh_retained();
        let ordered = self.ordered();
        if self.scope == Some(GardenScope::NeedsTriage) {
            self.selected = selection_after_update(
                self.selected,
                &previous.rows,
                &self.snapshot.rows,
                &ordered,
            );
        }
        if self.selected.is_none_or(|id| !ordered.contains(&id)) {
            self.selected = ordered
                .iter()
                .copied()
                .find(|id| self.row(*id).is_some_and(GardenRow::needs_triage))
                .or_else(|| ordered.first().copied());
        }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn row(&self, feed: FeedId) -> Option<&GardenRow> {
        self.index
            .get(&feed)
            .and_then(|at| self.snapshot.rows.get(*at))
    }

    fn refresh_retained(&mut self) {
        self.retained = self
            .seen_in_queue
            .union(&self.decided_this_visit)
            .copied()
            .collect();
    }

    pub fn visible(&self) -> Vec<&GardenRow> {
        self.scope.map_or_else(Vec::new, |scope| {
            visible_rows(&self.snapshot.rows, scope, &self.retained)
        })
    }

    /// Rhythm sections in All Feeds grouped by rhythm, status sections otherwise.
    pub fn sections(&self) -> Vec<Section> {
        let Some(scope) = self.scope else {
            return vec![];
        };
        let visible = self.visible();
        if scope == GardenScope::All && self.grouping == Grouping::Rhythm {
            rhythm_sections(&visible, &self.snapshot.cadence)
        } else {
            sections(&visible, scope)
        }
    }

    /// The listed feeds in the order the table shows them.
    pub fn ordered(&self) -> Vec<FeedId> {
        self.sections().into_iter().flat_map(|s| s.rows).collect()
    }

    pub fn is_dimmed(&self, feed: FeedId) -> bool {
        match (self.scope, self.row(feed)) {
            (Some(scope), Some(row)) => is_dimmed(row, scope, &self.retained),
            _ => false,
        }
    }

    pub fn grouping(&self) -> Grouping {
        self.grouping
    }

    pub fn set_grouping(&mut self, grouping: Grouping) {
        self.grouping = grouping;
    }

    pub fn selected(&self) -> Option<FeedId> {
        self.selected
    }

    /// Selects a listed feed; whether the selection changed.
    pub fn select(&mut self, feed: FeedId) -> bool {
        if self.selected == Some(feed) || !self.ordered().contains(&feed) {
            return false;
        }
        self.selected = Some(feed);
        true
    }

    pub fn count_label(&self) -> String {
        self.scope.map_or_else(String::new, |scope| {
            count_label(scope, &self.snapshot.rows, self.visible().len())
        })
    }

    pub fn safe_repairs(&self) -> Vec<(FeedId, Action)> {
        safe_repairs(&self.snapshot.rows)
    }

    /// A key the table received. `None`: not the garden's, the message stays. Otherwise the
    /// message is cleared, a refusal sets it, `R` asks for the confirmation, and the command
    /// comes back for the caller to run (`Decide`, `Undo`, `Open`).
    pub fn key(&mut self, key: &str, modifiers: u32) -> Option<Command> {
        let selected = self.selected.and_then(|id| self.row(id));
        let found = command(key, modifiers, selected, self.safe_repairs().len())?;
        self.message = None;
        match &found {
            Command::Message(text) => self.message = Some(text),
            Command::ConfirmSafeRepairs => self.confirming = true,
            _ => {}
        }
        Some(found)
    }

    /// A decision on `feed` was recorded from this window.
    pub fn decided(&mut self, feed: FeedId) {
        self.decided_this_visit.insert(feed);
        self.refresh_retained();
    }

    /// The last decision was undone (`feed`: the one it concerned, when known).
    pub fn undone(&mut self, feed: Option<FeedId>) {
        if let Some(feed) = feed {
            self.decided_this_visit.remove(&feed);
            self.refresh_retained();
        }
    }

    pub fn message(&self) -> Option<&'static str> {
        self.message
    }

    pub fn set_message(&mut self, message: Option<&'static str>) {
        self.message = message;
    }

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    pub fn set_confirming(&mut self, on: bool) {
        self.confirming = on;
    }

    /// The placeholder when a scope is shown and lists nothing.
    pub fn empty_text(&self) -> Option<&'static str> {
        let scope = self.scope?;
        self.sections()
            .iter()
            .all(|s| s.rows.is_empty())
            .then(|| empty_message(scope))
    }

    /// The inspector of the selected feed; `log` is that feed's fetch log, newest first.
    pub fn inspector<Tz: TimeZone>(
        &self,
        log: &[AttemptRecord],
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Option<Inspector>
    where
        Tz::Offset: Display,
    {
        let row = self.row(self.selected?)?;
        Some(Inspector::of(
            row,
            self.snapshot.cadence.get(&row.feed_id),
            &self.snapshot.themes,
            self.snapshot
                .theme_keys
                .get(&row.feed_id)
                .map(String::as_str),
            log,
            now,
            tz,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{NOTHING_TO_REPAIR, decide, load};
    use crate::keys::qt;
    use crate::test_db::{TestDb, at};
    use ressac_core::model::Decision;
    use ressac_core::store::Store;

    fn fixture() -> (TestDb, Store) {
        let db = TestDb::full();
        let store = db.store();
        (db, store)
    }

    fn visit(scope: GardenScope, store: &Store) -> GardenVisit {
        let mut v = GardenVisit::default();
        v.enter(scope);
        v.update(load(store).unwrap());
        v
    }

    fn headers(v: &GardenVisit) -> Vec<String> {
        v.sections().iter().map(Section::header).collect()
    }

    #[test]
    fn entering_selects_the_first_feed_needing_triage() {
        let (_db, store) = fixture();
        let v = visit(GardenScope::NeedsTriage, &store);
        assert_eq!(v.ordered(), [6, 5, 2]);
        assert_eq!(
            headers(&v),
            ["🦠 HIJACKED · 1", "🚚 MOVED · 1", "🦴 FOSSIL · 1"]
        );
        assert_eq!(v.selected(), Some(6));
        assert_eq!(v.count_label(), "3 of 4 left");
        assert_eq!(
            v.safe_repairs()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            [5]
        );
        let all = visit(GardenScope::All, &store);
        assert_eq!(
            (all.ordered().len(), all.count_label().as_str()),
            (10, "10 feeds")
        );
        assert_eq!(
            all.selected(),
            Some(6),
            "the first feed needing triage, not the first row"
        );
        let cemetery = visit(GardenScope::Cemetery, &store);
        assert_eq!(
            (
                cemetery.ordered(),
                cemetery.selected(),
                cemetery.count_label()
            ),
            (vec![4], Some(4), "1 feed".to_string())
        );
    }

    /// Review Focus 2.
    #[test]
    fn a_feed_decided_here_or_in_a_terminal_stays_listed_dimmed_until_the_scope_is_left() {
        let (_db, store) = fixture();
        let mut v = visit(GardenScope::NeedsTriage, &store);
        decide(&store, 6, Decision::Keep, at("2026-10-16T09:00:00Z")).unwrap(); // here or in `ressac garden`: the same write
        v.update(load(&store).unwrap());
        assert_eq!(v.ordered(), [6, 5, 2]);
        assert!(v.is_dimmed(6) && !v.is_dimmed(5));
        assert_eq!(v.selected(), Some(5));
        assert_eq!(v.count_label(), "2 of 4 left");
        v.enter(GardenScope::All);
        v.enter(GardenScope::NeedsTriage);
        v.update(load(&store).unwrap());
        assert_eq!(v.ordered(), [5, 2]);
    }

    #[test]
    fn all_feeds_keeps_a_feed_buried_during_the_visit() {
        let (_db, store) = fixture();
        let mut v = visit(GardenScope::All, &store);
        v.select(8);
        decide(&store, 8, Decision::Bury, at("2026-10-16T09:00:00Z")).unwrap(); // a healthy feed: never in the queue
        v.decided(8);
        v.update(load(&store).unwrap());
        assert!(v.ordered().contains(&8) && v.is_dimmed(8));
        v.undone(Some(8));
        v.update(load(&store).unwrap());
        assert!(!v.ordered().contains(&8), "buried and no longer remembered");
    }

    #[test]
    fn keys_set_the_message_and_the_confirmation_and_hand_back_runs() {
        let (_db, store) = fixture();
        let mut v = visit(GardenScope::NeedsTriage, &store); // 6 selected: Hijacked, action bury
        assert_eq!(v.key("r", 0), Some(Command::Message(NOTHING_TO_REPAIR)));
        assert_eq!(v.message(), Some("Nothing to repair for this feed."));
        assert_eq!(v.key("j", 0), None);
        assert_eq!(
            v.message(),
            Some("Nothing to repair for this feed."),
            "not the garden's key: the message stays"
        );
        assert_eq!(v.key("R", qt::SHIFT), Some(Command::ConfirmSafeRepairs));
        assert!(v.confirming() && v.message().is_none());
        assert_eq!(v.key("b", qt::CONTROL), None);
        assert_eq!(v.key("k", 0), Some(Command::Decide(Decision::Keep)));
    }

    #[test]
    fn rhythm_grouping_applies_to_all_feeds_only() {
        let (_db, store) = fixture();
        let mut queue = visit(GardenScope::NeedsTriage, &store);
        queue.set_grouping(Grouping::Rhythm);
        assert_eq!(
            headers(&queue),
            ["🦠 HIJACKED · 1", "🚚 MOVED · 1", "🦴 FOSSIL · 1"]
        );
        let mut all = visit(GardenScope::All, &store);
        all.set_grouping(Grouping::Rhythm);
        assert_eq!(
            headers(&all),
            [
                "⚡ DAILY · 1",
                "📅 WEEKLY · 2",
                "🌗 MONTHLY · 1",
                "💤 SILENT · 2",
                "❔ UNCHECKED · 4"
            ]
        );
    }

    #[test]
    fn empty_scopes_say_so() {
        let (db, store) = fixture();
        db.execute("UPDATE feed SET state = 'active' WHERE id = 4");
        let cemetery = visit(GardenScope::Cemetery, &store);
        assert_eq!(
            (cemetery.empty_text(), cemetery.selected()),
            (Some("The cemetery is empty"), None)
        );
        assert_eq!(visit(GardenScope::All, &store).empty_text(), None);
    }

    #[test]
    fn leaving_forgets_the_visit_but_not_the_grouping() {
        let (_db, store) = fixture();
        let mut v = visit(GardenScope::All, &store);
        v.set_grouping(Grouping::Rhythm);
        v.leave();
        assert_eq!(
            (v.scope(), v.ordered(), v.selected(), v.empty_text()),
            (None, vec![], None, None)
        );
        assert!(v.visible().is_empty() && v.count_label().is_empty());
        assert!(v.row(6).is_none(), "the snapshot is gone");
        v.enter(GardenScope::All);
        assert_eq!(
            v.grouping(),
            Grouping::Rhythm,
            "entering keeps the grouping"
        );
        v.update(load(&store).unwrap());
        assert_eq!(headers(&v)[0], "⚡ DAILY · 1");
    }

    #[test]
    fn entering_another_scope_keeps_the_grouping_and_drops_the_memory() {
        let (_db, store) = fixture();
        let mut v = visit(GardenScope::All, &store);
        v.set_grouping(Grouping::Rhythm);
        v.decided(8);
        v.enter(GardenScope::NeedsTriage);
        assert_eq!(v.grouping(), Grouping::Rhythm);
        assert!(!v.is_dimmed(8) && v.selected().is_none());
    }

    #[test]
    fn a_visit_has_an_epoch_that_changes_with_the_scope() {
        let mut v = GardenVisit::default();
        v.enter(GardenScope::NeedsTriage);
        let first = v.epoch();
        v.enter(GardenScope::NeedsTriage);
        assert_eq!(v.epoch(), first, "the same scope is the same visit");
        v.enter(GardenScope::All);
        let second = v.epoch();
        assert_ne!(second, first);
        v.leave();
        assert_ne!(v.epoch(), second);
        v.enter(GardenScope::All);
        assert_ne!(
            v.epoch(),
            second,
            "a new visit of the same scope is another visit"
        );
    }
}
