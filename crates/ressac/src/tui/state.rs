//! Garden screen state and key handling. Pure: no terminal, no database.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ressac_core::garden::{GardenRow, Progress, progress, safe_repairs};
use ressac_core::model::{Decision, FeedId, Status};

const PAGE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Queue,
    Detail,
    ConfirmRepairs,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Feeds needing triage (the default view).
    Queue,
    Status(Status),
    Folder(String),
}

impl Filter {
    pub fn label(&self) -> String {
        match self {
            Filter::Queue => "to triage".to_string(),
            Filter::Status(s) => s.as_str().to_string(),
            Filter::Folder(f) => format!("folder {f}"),
        }
    }

    fn matches(&self, row: &GardenRow) -> bool {
        match self {
            Filter::Queue => row.needs_triage(),
            Filter::Status(s) => row.status() == *s,
            Filter::Folder(f) => &row.top_folder == f,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    None,
    Quit,
    Decide(FeedId, Decision),
    Undo,
    ApplySafeRepairs,
    Open(String),
}

pub struct GardenState {
    pub rows: Vec<GardenRow>,
    /// Indexes into `rows` shown by the current filter.
    pub visible: Vec<usize>,
    /// Index into `visible`.
    pub selected: usize,
    pub mode: Mode,
    pub filter: Filter,
    pub message: Option<String>,
    pub ascii: bool,
}

impl GardenState {
    pub fn new(rows: Vec<GardenRow>, ascii: bool) -> Self {
        let mut state = GardenState {
            rows,
            visible: Vec::new(),
            selected: 0,
            mode: Mode::Queue,
            filter: Filter::Queue,
            message: None,
            ascii,
        };
        state.refilter();
        state
    }

    pub fn selected_row(&self) -> Option<&GardenRow> {
        self.visible.get(self.selected).map(|&i| &self.rows[i])
    }

    pub fn progress(&self) -> Progress {
        progress(&self.rows)
    }

    pub fn safe_repair_count(&self) -> usize {
        safe_repairs(&self.rows).len()
    }

    /// Replaces the rows after a database change, keeping the cursor position so the
    /// feed after a just-decided one becomes selected.
    pub fn reload(&mut self, rows: Vec<GardenRow>) {
        self.rows = rows;
        self.refilter();
    }

    fn refilter(&mut self) {
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| self.filter.matches(r))
            .map(|(i, _)| i)
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
    }

    fn filters(&self) -> Vec<Filter> {
        let mut out = vec![Filter::Queue];
        out.extend(
            Status::ALL
                .into_iter()
                .filter(|s| self.rows.iter().any(|r| r.status() == *s))
                .map(Filter::Status),
        );
        let mut folders: Vec<String> = self.rows.iter().map(|r| r.top_folder.clone()).collect();
        folders.sort();
        folders.dedup();
        out.extend(folders.into_iter().map(Filter::Folder));
        out
    }

    fn next_filter(&mut self) {
        let all = self.filters();
        let next = all
            .iter()
            .position(|f| *f == self.filter)
            .map_or(0, |i| (i + 1) % all.len());
        self.filter = all[next].clone();
        self.selected = 0;
        self.refilter();
        self.message = Some(format!("Filter: {}", self.filter.label()));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Command {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Command::Quit;
        }
        self.message = None;
        match self.mode {
            Mode::Help => {
                self.mode = Mode::Queue;
                Command::None
            }
            Mode::ConfirmRepairs => {
                self.mode = Mode::Queue;
                if key.code == KeyCode::Char('y') {
                    Command::ApplySafeRepairs
                } else {
                    Command::None
                }
            }
            Mode::Queue | Mode::Detail => self.on_list_key(key.code),
        }
    }

    fn on_list_key(&mut self, code: KeyCode) -> Command {
        let last = self.visible.len().saturating_sub(1);
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(PAGE),
            KeyCode::PageDown => self.selected = (self.selected + PAGE).min(last),
            KeyCode::Enter => {
                self.mode = if self.mode == Mode::Detail {
                    Mode::Queue
                } else {
                    Mode::Detail
                };
            }
            KeyCode::Esc if self.mode == Mode::Detail => self.mode = Mode::Queue,
            KeyCode::Esc | KeyCode::Char('q') => return Command::Quit,
            KeyCode::Char('k') => return self.decide(Decision::Keep),
            KeyCode::Char('b' | 'r') if self.selected_row().is_some_and(|r| r.buried) => {
                self.message = Some("This feed is buried; press k to restore it first.".into());
            }
            KeyCode::Char('b') => return self.decide(Decision::Bury),
            KeyCode::Char('r') => {
                let repairable = self
                    .selected_row()
                    .map(|r| r.action().is_some_and(|a| a.is_repair()));
                match repairable {
                    Some(true) => return self.decide(Decision::Repair),
                    Some(false) => self.message = Some("Nothing to repair for this feed.".into()),
                    None => {}
                }
            }
            KeyCode::Char('u') => return Command::Undo,
            KeyCode::Char('o') => {
                return self.open(|r| r.html_url.clone(), "This feed has no site address.");
            }
            KeyCode::Char('w') => {
                return self.open(
                    |r| r.wayback_url.clone(),
                    "No Wayback snapshot yet (run `ressac check`).",
                );
            }
            KeyCode::Char('f') => self.next_filter(),
            KeyCode::Char('R') => {
                if self.safe_repair_count() > 0 {
                    self.mode = Mode::ConfirmRepairs;
                } else {
                    self.message = Some("No safe repairs pending.".into());
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
        Command::None
    }

    fn decide(&self, decision: Decision) -> Command {
        self.selected_row()
            .map_or(Command::None, |r| Command::Decide(r.feed_id, decision))
    }

    fn open(&mut self, pick: impl Fn(&GardenRow) -> Option<String>, missing: &str) -> Command {
        match self.selected_row().map(pick) {
            Some(Some(url)) if is_web_url(&url) => Command::Open(url),
            Some(Some(_)) => {
                self.message = Some("Refusing to open a non-web address.".into());
                Command::None
            }
            Some(None) => {
                self.message = Some(missing.to_string());
                Command::None
            }
            None => Command::None,
        }
    }
}

/// Feed and OPML content is untrusted: only http(s) addresses may reach the system opener.
fn is_web_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

#[cfg(test)]
pub(crate) fn sample_rows() -> Vec<GardenRow> {
    use chrono::{DateTime, Utc};
    use ressac_core::model::{Action, Cadence, Diagnosis, Evidence};

    let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
    let row = |id: FeedId,
               title: &str,
               folder: &str,
               status: Status,
               clue: &str,
               action: Option<Action>| GardenRow {
        feed_id: id,
        title: title.into(),
        folder_path: folder.into(),
        top_folder: folder.split(" / ").next().unwrap().into(),
        xml_url: format!("https://feed{id}.example.org/rss"),
        html_url: Some(format!("https://feed{id}.example.org/")),
        wayback_url: None,
        buried: false,
        diagnosis: Some(Diagnosis {
            status,
            reason: format!("{} reason.", status.as_str()),
            evidence: Evidence {
                bullets: vec![format!("{} reason.", status.as_str())],
                clue: clue.into(),
                language: None,
            },
            remarks: vec![],
            action,
            hijack_score: 0.0,
            cadence: Cadence::default(),
        }),
        decision: None,
        recent: vec![],
    };
    let mut biblio = row(
        1,
        "Bibliobsession 2.0",
        "Biblis",
        Status::Hijacked,
        "vie ⚠",
        Some(Action::Bury),
    );
    if let Some(d) = biblio.diagnosis.as_mut() {
        d.reason = "The site now seems to serve unrelated or spam content.".into();
        d.hijack_score = 0.86;
        d.evidence.bullets = vec![
            "Description is in vie (0.97) while the feed is in fra.".into(),
            "Spam vocabulary: \"xóc đĩa\", \"trúng lớn\".".into(),
            "The feed address now returns an HTML page.".into(),
        ];
        d.evidence.language = Some("vie".into());
    }
    biblio.wayback_url =
        Some("http://web.archive.org/web/20210612000000/https://bibliobsession.net/".into());
    biblio.recent = vec![
        (
            at("2019-04-02T10:00:00Z"),
            "Bibliothèques et communs numériques".into(),
        ),
        (
            at("2019-02-11T10:00:00Z"),
            "Le grand débat en bibliothèque ?".into(),
        ),
    ];
    vec![
        biblio,
        row(
            2,
            "Slashdot",
            "Infos",
            Status::Moved,
            "301",
            Some(Action::FollowRedirect {
                url: "https://slashdot.org/rss".into(),
            }),
        ),
        row(
            3,
            "Re/code",
            "Infos",
            Status::Dead,
            "410",
            Some(Action::Bury),
        ),
        row(4, "Heading East", "Blogs", Status::Fossil, "2011", None),
        row(
            5,
            "PhD et maracuja",
            "Blogs / potes",
            Status::Dormant,
            "14 mo",
            None,
        ),
        row(
            6,
            "xkcd.com",
            "Blogs",
            Status::Alive,
            "3 d",
            Some(Action::UpgradeHttps {
                url: "https://xkcd.com/rss.xml".into(),
            }),
        ),
    ]
}

/// `sample_rows()` with Re/code (#3, Dead) buried on 2026-10-09.
#[cfg(test)]
pub(crate) fn sample_rows_with_buried() -> Vec<GardenRow> {
    use ressac_core::store::DecisionRecord;

    let mut rows = sample_rows();
    let recode = rows.iter_mut().find(|r| r.feed_id == 3).unwrap();
    recode.buried = true;
    recode.decision = Some(DecisionRecord {
        id: 1,
        feed_id: 3,
        decision: Decision::Bury,
        status_at_decision: Status::Dead,
        action: None,
        decided_at: "2026-10-09T18:30:00Z".parse().unwrap(),
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state() -> GardenState {
        GardenState::new(sample_rows(), false)
    }

    #[test]
    fn queue_shows_only_feeds_needing_triage() {
        let s = state();
        let ids: Vec<FeedId> = s.visible.iter().map(|&i| s.rows[i].feed_id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert_eq!(s.selected_row().unwrap().feed_id, 1);
    }

    #[test]
    fn decisions_target_the_selected_feed() {
        let mut s = state();
        assert_eq!(
            s.on_key(key(KeyCode::Char('k'))),
            Command::Decide(1, Decision::Keep)
        );
        s.on_key(key(KeyCode::Down));
        assert_eq!(
            s.on_key(key(KeyCode::Char('r'))),
            Command::Decide(2, Decision::Repair)
        );
        s.on_key(key(KeyCode::Down));
        assert_eq!(
            s.on_key(key(KeyCode::Char('b'))),
            Command::Decide(3, Decision::Bury)
        );
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('r'))), Command::None);
        assert_eq!(
            s.message.as_deref(),
            Some("Nothing to repair for this feed.")
        );
    }

    #[test]
    fn buried_feeds_refuse_repair_and_bury_but_keep_restores_them() {
        let mut s = GardenState::new(sample_rows_with_buried(), false);
        for _ in 0..3 {
            s.on_key(key(KeyCode::Char('f')));
        }
        assert_eq!(s.filter, Filter::Status(Status::Dead));
        assert_eq!(s.selected_row().unwrap().feed_id, 3);
        for k in ['r', 'b'] {
            assert_eq!(s.on_key(key(KeyCode::Char(k))), Command::None, "{k}");
            assert_eq!(
                s.message.as_deref(),
                Some("This feed is buried; press k to restore it first."),
                "{k}"
            );
        }
        assert_eq!(
            s.on_key(key(KeyCode::Char('k'))),
            Command::Decide(3, Decision::Keep)
        );
    }

    #[test]
    fn navigation_is_clamped() {
        let mut s = state();
        s.on_key(key(KeyCode::Up));
        assert_eq!(s.selected, 0);
        s.on_key(key(KeyCode::PageDown));
        assert_eq!(s.selected, 3);
    }

    #[test]
    fn open_refuses_non_web_addresses() {
        let mut s = state();
        s.rows[0].html_url = Some("file:///etc/passwd".into());
        assert_eq!(s.on_key(key(KeyCode::Char('o'))), Command::None);
        assert_eq!(
            s.message.as_deref(),
            Some("Refusing to open a non-web address.")
        );
        s.rows[0].wayback_url = Some("x-custom://run".into());
        assert_eq!(s.on_key(key(KeyCode::Char('w'))), Command::None);
        assert_eq!(
            s.message.as_deref(),
            Some("Refusing to open a non-web address.")
        );
        s.rows[0].html_url = Some("https://example.org/".into());
        assert_eq!(
            s.on_key(key(KeyCode::Char('o'))),
            Command::Open("https://example.org/".into())
        );
    }

    #[test]
    fn open_keys_use_site_and_wayback() {
        let mut s = state();
        assert_eq!(
            s.on_key(key(KeyCode::Char('w'))),
            Command::Open(
                "http://web.archive.org/web/20210612000000/https://bibliobsession.net/".into()
            )
        );
        s.on_key(key(KeyCode::Down));
        assert_eq!(s.on_key(key(KeyCode::Char('w'))), Command::None);
        assert!(
            s.message
                .as_deref()
                .unwrap()
                .starts_with("No Wayback snapshot")
        );
        assert_eq!(
            s.on_key(key(KeyCode::Char('o'))),
            Command::Open("https://feed2.example.org/".into())
        );
    }

    #[test]
    fn filters_cycle_through_statuses_and_folders() {
        let mut s = state();
        s.on_key(key(KeyCode::Char('f')));
        assert_eq!(s.filter, Filter::Status(Status::Hijacked));
        for _ in 0..5 {
            s.on_key(key(KeyCode::Char('f')));
        }
        assert_eq!(s.filter, Filter::Status(Status::Alive));
        assert_eq!(s.visible.len(), 1);
        s.on_key(key(KeyCode::Char('f')));
        assert_eq!(s.filter, Filter::Folder("Biblis".into()));
        assert_eq!(s.message.as_deref(), Some("Filter: folder Biblis"));
    }

    #[test]
    fn bulk_repairs_need_confirmation() {
        let mut s = state();
        assert_eq!(s.safe_repair_count(), 2);
        assert_eq!(s.on_key(key(KeyCode::Char('R'))), Command::None);
        assert_eq!(s.mode, Mode::ConfirmRepairs);
        assert_eq!(s.on_key(key(KeyCode::Char('n'))), Command::None);
        assert_eq!(s.mode, Mode::Queue);
        s.on_key(key(KeyCode::Char('R')));
        assert_eq!(s.on_key(key(KeyCode::Char('y'))), Command::ApplySafeRepairs);
    }

    #[test]
    fn detail_help_and_quit() {
        let mut s = state();
        s.on_key(key(KeyCode::Enter));
        assert_eq!(s.mode, Mode::Detail);
        assert_eq!(s.on_key(key(KeyCode::Esc)), Command::None);
        assert_eq!(s.mode, Mode::Queue);
        s.on_key(key(KeyCode::Char('?')));
        assert_eq!(s.mode, Mode::Help);
        s.on_key(key(KeyCode::Char('x')));
        assert_eq!(s.mode, Mode::Queue);
        assert_eq!(
            s.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Command::Quit
        );
        assert_eq!(s.on_key(key(KeyCode::Esc)), Command::Quit);
    }

    #[test]
    fn reload_keeps_the_position_so_the_next_feed_is_selected() {
        let mut s = state();
        s.on_key(key(KeyCode::Down));
        let mut rows = sample_rows();
        rows.retain(|r| r.feed_id != 2);
        s.reload(rows);
        assert_eq!(s.selected_row().unwrap().feed_id, 3);
    }
}
