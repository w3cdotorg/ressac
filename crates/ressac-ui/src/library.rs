//! The library's rules without Qt: the Add Feed form and its note, the folder pop-up, the OPML
//! import flow (preview, confirmation, import, then a fetch of everything), export, the first-run
//! texts, and the drag rules (a feed onto a folder, an entry out as a link).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ressac_core::model::{FeedId, FolderId};
use ressac_core::opml;
use ressac_core::service::{AddedFeed, EventSink, Ressac};
use ressac_core::store::{ImportReport, SourceNode, SourceNodeKind, Store, ThemeSource};
use ressac_core::{Error, Result};

use crate::runs::{RunEnd, RunRequest};
use crate::sanitize::is_web_url;
use crate::sources::{FOLDER_KEY_PREFIX, HEADING_KEY_PREFIX, HEADINGS};

pub const FEED_MIME: &str = "application/x-ressac-feed";
pub const ADD_TITLE: &str = "Add a Feed";
pub const ADDRESS_PLACEHOLDER: &str = "Feed or site address";
pub const NO_THEME: &str = "None";
pub const TOP_LEVEL: &str = "Feeds";
pub const ADD_REFUSED: &str = "A refresh is running. Try again when it ends.";
pub const NOTE_SECONDS: u32 = 6;
pub const CANT_IMPORT: &str = "This OPML file can't be imported";
pub const IMPORT_NOTE: &str = "Ressac fetches every feed right after the import.";
pub const IMPORT_BLOCKED: &str = "A refresh is running; try again when it ends.";
pub const NO_FEEDS_TITLE: &str = "No Feeds Yet";
pub const NO_FEEDS_TEXT: &str = "Drop an OPML file here, or choose one…";
pub const CHOOSE_OPML: &str = "Choose OPML File…";
pub const EXPORT_NAME: &str = "Ressac.opml";
pub const OPML_FILTER: &str = "OPML files (*.opml)";
/// Settings ▸ Data's file dialogs filter.
pub const SQLITE_FILTER: &str = "SQLite databases (*.sqlite)";

const UNCHARTED: &str = "Added to Uncharted waters.";

// Add Feed

/// The clipboard's text when it is, on its own (trimmed, no inner whitespace), a web address.
pub fn clipboard_address(text: Option<&str>) -> Option<String> {
    let text = text?.trim();
    (!text.contains(char::is_whitespace) && is_web_url(text)).then(|| text.to_string())
}

/// The note under the window after an add: where the feed went, and why.
pub fn added_note(added: &AddedFeed) -> String {
    match (&added.theme, &added.theme_source) {
        (Some(theme), ThemeSource::Sibling { title, .. }) => format!("In {theme}, like {title}."),
        (Some(theme), _) => format!("In {theme}."),
        (None, _) => UNCHARTED.to_string(),
    }
}

/// One line of the Folder pop-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChoice {
    pub folder: Option<FolderId>,
    pub name: String,
}

impl FolderChoice {
    /// The bare name: every folder sits at the same level, with no indent (two folders with
    /// one name look the same; Clément chose that over a nested look).
    pub fn title(&self) -> String {
        self.name.clone()
    }
}

/// "Feeds" (the top level), then every folder of `tree` in sidebar order.
pub fn folder_choices(tree: &[SourceNode]) -> Vec<FolderChoice> {
    fn walk(nodes: &[SourceNode], out: &mut Vec<FolderChoice>) {
        for node in nodes {
            if let SourceNodeKind::Folder { id, name } = &node.kind {
                out.push(FolderChoice {
                    folder: Some(*id),
                    name: name.clone(),
                });
                walk(&node.children, out);
            }
        }
    }
    let mut choices = vec![FolderChoice {
        folder: None,
        name: TOP_LEVEL.into(),
    }];
    walk(tree, &mut choices);
    choices
}

/// The Add Feed sheet's state.
#[derive(Debug, Clone, Default)]
pub struct AddFeedForm {
    pub address: String,
    pub theme: Option<String>,
    pub folder: Option<FolderId>,
    running: bool,
    error: Option<String>,
    shows_log: bool,
    generation: u64,
}

/// The number of the next form: an add run carries its form's, so a run that ends after its form
/// was cancelled (and maybe replaced by a new one) cannot touch the new form.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

impl AddFeedForm {
    pub fn new(clipboard: Option<&str>) -> AddFeedForm {
        AddFeedForm {
            address: clipboard_address(clipboard).unwrap_or_default(),
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            ..AddFeedForm::default()
        }
    }

    /// This form's number (unique among the forms of the process).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn can_add(&self) -> bool {
        !self.running && !self.address.trim().is_empty()
    }

    pub fn start(&mut self) -> Option<RunRequest> {
        if !self.can_add() {
            return None;
        }
        self.running = true;
        self.error = None;
        self.shows_log = false;
        Some(RunRequest::Add {
            address: self.address.trim().to_string(),
            theme: self.theme.clone(),
            folder: self.folder,
        })
    }

    /// The run's end: the added feed on success (the sheet closes), else the sentence to show.
    pub fn finished(&mut self, end: RunEnd<AddedFeed>) -> Option<AddedFeed> {
        self.running = false;
        match end {
            RunEnd::Done(added) => {
                self.error = None;
                self.shows_log = false;
                return Some(added);
            }
            RunEnd::Refused => {
                self.error = Some(ADD_REFUSED.into());
                self.shows_log = false;
            }
            RunEnd::Failed(failure) => {
                self.error = Some(failure.message);
                self.shows_log = !failure.refusal;
            }
        }
        None
    }

    /// A folder that is gone from the pop-up resets the choice to the top level.
    pub fn keep_folder(&mut self, choices: &[FolderChoice]) {
        if self.folder.is_some() && !choices.iter().any(|c| c.folder == self.folder) {
            self.folder = None;
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn shows_log(&self) -> bool {
        self.shows_log
    }
}

/// The end of an add run started by the form `generation`: the open form takes it when it is
/// that form; any other case (no form, or a newer one) only keeps the feed it added, so the end
/// shows no error in and never closes a form that did not start it. The added feed, if any,
/// and whether the dialog closes (only the form that started the run, on success).
pub fn settle_add(
    form: &mut Option<AddFeedForm>,
    generation: u64,
    end: RunEnd<AddedFeed>,
) -> AddSettled {
    match form.as_mut().filter(|f| f.generation == generation) {
        Some(own) => {
            let feed = own.finished(end);
            AddSettled {
                closes_form: feed.is_some(),
                feed,
            }
        }
        None => AddSettled {
            feed: match end {
                RunEnd::Done(feed) => Some(feed),
                _ => None,
            },
            closes_form: false,
        },
    }
}

/// What `settle_add` decided.
#[derive(Debug, PartialEq)]
pub struct AddSettled {
    pub feed: Option<AddedFeed>,
    /// The open form started the run and it succeeded: the dialog closes.
    pub closes_form: bool,
}

/// The add job: `Ressac::add_feed`, the folder passed as its id (ids are resolved first).
pub async fn add_feed(
    app: &Ressac,
    address: &str,
    theme: Option<&str>,
    folder: Option<FolderId>,
    sink: &dyn EventSink,
    stop: Arc<AtomicBool>,
) -> Result<AddedFeed> {
    let folder = folder.map(|id| id.to_string());
    app.add_feed(address, theme, folder.as_deref(), sink, stop)
        .await
}

// OPML

fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

pub fn import_title(path: &Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    format!("Import “{name}”?")
}

/// The confirmation's lines, from the dry run.
pub fn import_summary(report: &ImportReport) -> Vec<String> {
    let mut lines = Vec::new();
    if report.feeds_added == 0 {
        lines.push("No new feeds in this file.".to_string());
    } else {
        lines.push(format!(
            "{} in {}.",
            count(report.feeds_added, "feed"),
            count(report.folders_added, "new folder")
        ));
    }
    if !report.merged.is_empty() {
        lines.push(format!(
            "{} inside the file will be merged:",
            count(report.merged.len(), "duplicate")
        ));
        lines.extend(
            report
                .merged
                .iter()
                .map(|(dropped, kept)| format!("“{dropped}” into “{kept}”")),
        );
    }
    if report.skipped_existing > 0 {
        lines.push(format!(
            "{} already in Ressac will be skipped.",
            count(report.skipped_existing, "feed")
        ));
    }
    lines
}

/// The path of a `file:` URL, percent-decoded; anything else is None.
pub fn local_path(url: &str) -> Option<PathBuf> {
    url::Url::parse(url).ok()?.to_file_path().ok()
}

/// The first local file among `urls` whose extension is `opml` (any case).
pub fn first_opml(urls: &[String]) -> Option<PathBuf> {
    urls.iter().filter_map(|url| local_path(url)).find(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("opml"))
    })
}

/// The file's text; a failure reads like the CLI's.
pub fn read_opml(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| Error::Invalid(format!("cannot read {}: {e}", path.display())))
}

/// The dry run: what an import would do, nothing written.
pub fn preview(app: &Ressac, path: &Path) -> Result<ImportReport> {
    app.import_opml(&read_opml(path)?, true)
}

pub fn import(app: &Ressac, path: &Path) -> Result<ImportReport> {
    app.import_opml(&read_opml(path)?, false)
}

/// The active feeds as OPML, written to `path`.
pub fn export(store: &Store, path: &Path) -> Result<()> {
    std::fs::write(path, opml::export(store, false)?)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum OpmlStep {
    #[default]
    Idle,
    Previewing(PathBuf),
    Confirming(PathBuf, ImportReport),
    Importing(PathBuf, ImportReport),
    Failed(String),
}

impl OpmlStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpmlStep::Idle => "idle",
            OpmlStep::Previewing(_) => "previewing",
            OpmlStep::Confirming(..) => "confirming",
            OpmlStep::Importing(..) => "importing",
            OpmlStep::Failed(_) => "failed",
        }
    }
}

/// The Mac's `OPMLFlow`: preview, confirm, import, then the fetch of everything.
#[derive(Debug, Clone, Default)]
pub struct OpmlFlow {
    step: OpmlStep,
}

impl OpmlFlow {
    pub fn step(&self) -> &OpmlStep {
        &self.step
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.step, OpmlStep::Previewing(_) | OpmlStep::Importing(..))
    }

    /// A drop or a chosen file: the path to preview, unless busy or no `.opml` is among `urls`.
    pub fn open(&mut self, urls: &[String]) -> Option<PathBuf> {
        if self.is_busy() {
            return None;
        }
        let path = first_opml(urls)?;
        self.step = OpmlStep::Previewing(path.clone());
        Some(path)
    }

    /// The preview's result, only for the file being previewed.
    pub fn previewed(&mut self, path: &Path, result: std::result::Result<ImportReport, String>) {
        if !matches!(&self.step, OpmlStep::Previewing(current) if current == path) {
            return;
        }
        self.step = match result {
            Ok(report) => OpmlStep::Confirming(path.to_path_buf(), report),
            Err(message) => OpmlStep::Failed(message),
        };
    }

    /// `blocked`: another long run is active.
    pub fn can_import(&self, blocked: bool) -> bool {
        matches!(&self.step, OpmlStep::Confirming(_, report) if report.feeds_added > 0) && !blocked
    }

    pub fn confirm(&mut self, blocked: bool) -> Option<RunRequest> {
        if !self.can_import(blocked) {
            return None;
        }
        let OpmlStep::Confirming(path, report) = std::mem::take(&mut self.step) else {
            unreachable!("can_import checked the step")
        };
        self.step = OpmlStep::Importing(path.clone(), report);
        Some(RunRequest::Import(path))
    }

    /// The import's end: a fetch of everything follows a success (the flow stays busy until
    /// `fetched`); a refusal goes back to the confirmation; a failure ends the flow.
    pub fn imported(&mut self, end: &RunEnd<ImportReport>) -> Option<RunRequest> {
        if !matches!(self.step, OpmlStep::Importing(..)) {
            return None;
        }
        match end {
            RunEnd::Done(_) => Some(RunRequest::FetchAll),
            RunEnd::Refused => {
                if let OpmlStep::Importing(path, report) = std::mem::take(&mut self.step) {
                    self.step = OpmlStep::Confirming(path, report);
                }
                None
            }
            RunEnd::Failed(_) => {
                self.step = OpmlStep::Idle;
                None
            }
        }
    }

    /// The fetch after the import ended.
    pub fn fetched(&mut self) {
        if matches!(self.step, OpmlStep::Importing(..)) {
            self.step = OpmlStep::Idle;
        }
    }

    pub fn cancel(&mut self) {
        if !self.is_busy() {
            self.step = OpmlStep::Idle;
        }
    }
}

// Drag and drop

pub fn feed_payload(feed: FeedId) -> String {
    feed.to_string()
}

/// ASCII digits only, no sign, fits in an id.
pub fn feed_from_payload(text: &str) -> Option<FeedId> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Where a sidebar row accepts a feed: a folder, or the "Feeds" heading (the top level).
pub fn drop_target(key: &str) -> Option<Option<FolderId>> {
    if key.strip_prefix(HEADING_KEY_PREFIX) == Some(HEADINGS[1]) {
        return Some(None);
    }
    key.strip_prefix(FOLDER_KEY_PREFIX)
        .and_then(feed_from_payload)
        .map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedDrop {
    Ignore,
    Move(Option<FolderId>),
}

pub fn decide_drop(current: Option<FolderId>, target: Option<FolderId>) -> FeedDrop {
    if current == target {
        FeedDrop::Ignore
    } else {
        FeedDrop::Move(target)
    }
}

/// Control characters and the Unicode line breaks a drop target may split a payload on.
fn is_break(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// A feed-supplied title on one line: every control or line-break character becomes a space,
/// runs of whitespace collapse, the ends are trimmed.
fn one_line(title: &str) -> String {
    let spaced: String = title
        .chars()
        .map(|c| if is_break(c) { ' ' } else { c })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An entry dragged out: its link as a URL list, as text and as a titled Mozilla URL; nothing
/// when the link is not a web address or carries a control or line-break character (a
/// `text/x-moz-url` is a list of URL and title lines, so neither part may add a line).
pub fn entry_drag_data(link: &str, title: &str) -> Vec<(&'static str, String)> {
    if !is_web_url(link) || link.chars().any(is_break) {
        return Vec::new();
    }
    vec![
        ("text/uri-list", format!("{link}\r\n")),
        ("text/plain", link.to_string()),
        ("text/x-moz-url", format!("{link}\n{}", one_line(title))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::{RunFailure, RunKind};
    use crate::test_db::TestDb;
    use ressac_core::service::{NullSink, Ressac};
    use ressac_core::store::SourceNodeKind;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn added(theme: Option<&str>, like: Option<&str>) -> AddedFeed {
        AddedFeed {
            id: 20,
            title: "Daring Fireball".into(),
            xml_url: "https://df.example/feed.xml".into(),
            page: None,
            theme: theme.map(String::from),
            theme_source: like.map_or(ThemeSource::None, |t| ThemeSource::Sibling {
                id: 9,
                title: t.into(),
            }),
            territory: None,
            folder: None,
            folder_name: None,
            discovered: false,
        }
    }

    fn report(
        feeds: usize,
        folders: usize,
        merged: &[(&str, &str)],
        skipped: usize,
    ) -> ImportReport {
        ImportReport {
            folders_added: folders,
            feeds_added: feeds,
            merged: merged
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            skipped_existing: skipped,
        }
    }

    fn node_folder(id: i64, name: &str, children: Vec<SourceNode>) -> SourceNode {
        SourceNode {
            kind: SourceNodeKind::Folder {
                id,
                name: name.into(),
            },
            page: None,
            unread: 0,
            children,
        }
    }

    fn walk_opml(dir: &Path) -> PathBuf {
        let path = dir.join("walk.opml");
        std::fs::write(
            &path,
            r#"<?xml version="1.0"?>
<opml version="2.0"><head><title>Walk</title></head><body>
<outline text="Walk">
<outline type="rss" text="Alpha" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline type="rss" text="Alpha again" xmlUrl="http://127.0.0.21:9/alpha.xml"/>
<outline type="rss" text="Beta" xmlUrl="http://127.0.0.22:9/beta.xml"/>
</outline>
<outline type="rss" text="Gamma" xmlUrl="http://127.0.0.23:9/gamma.xml"/>
</body></opml>"#,
        )
        .unwrap();
        path
    }

    // AddFeedTests
    #[test]
    fn add_request_carries_the_trimmed_address_theme_and_folder() {
        let mut form = AddFeedForm::new(None);
        form.address = "  df.example \n".into();
        form.theme = Some("tech".into());
        form.folder = Some(7);
        let request = form.start().unwrap();
        assert_eq!(
            request,
            RunRequest::Add {
                address: "df.example".into(),
                theme: Some("tech".into()),
                folder: Some(7)
            }
        );
        assert_eq!(request.kind(), RunKind::Long);
        assert!(!request.shows_failure_banner(), "errors stay in the dialog");
        assert!(request.is_refusal(&ressac_core::Error::FeedExists {
            id: 1,
            title: "Daring Fireball".into(),
            buried: false
        }));
        assert!(!request.is_refusal(&ressac_core::Error::Unreachable {
            url: "https://df.example".into(),
            reason: "timed out".into()
        }));
    }

    #[test]
    fn the_folder_pop_up_lists_the_top_level_then_every_folder_in_sidebar_order() {
        let tree = [
            node_folder(4, "Blogs", vec![]),
            node_folder(
                1,
                "News",
                vec![node_folder(2, "Sub", vec![node_folder(3, "Deep", vec![])])],
            ),
        ];
        let choices = folder_choices(&tree);
        assert_eq!(
            choices.iter().map(|c| c.folder).collect::<Vec<_>>(),
            [None, Some(4), Some(1), Some(2), Some(3)]
        );
        // Every folder at the same level: no indent, whatever its depth in the tree.
        assert_eq!(
            choices.iter().map(FolderChoice::title).collect::<Vec<_>>(),
            ["Feeds", "Blogs", "News", "Sub", "Deep"]
        );
        assert_eq!(
            AddFeedForm::new(None).folder,
            None,
            "the top level is the default"
        );
        assert_eq!(
            folder_choices(&[])
                .iter()
                .map(FolderChoice::title)
                .collect::<Vec<_>>(),
            ["Feeds"]
        );
    }

    #[test]
    fn a_folder_that_disappears_resets_the_choice_to_the_top_level() {
        let mut form = AddFeedForm::new(None);
        let choices = folder_choices(&[node_folder(3, "A", vec![])]);
        form.folder = Some(3);
        form.keep_folder(&choices);
        assert_eq!(form.folder, Some(3));
        form.keep_folder(&choices[..1]);
        assert_eq!(form.folder, None);
    }

    #[test]
    fn an_empty_address_disables_add() {
        let mut form = AddFeedForm::new(None);
        assert!(!form.can_add());
        form.address = "   \n".into();
        assert!(!form.can_add() && form.start().is_none());
        form.address = " example.com ".into();
        assert!(form.can_add());
    }

    #[test]
    fn add_cannot_start_twice() {
        let mut form = AddFeedForm::new(Some("https://example.com/feed.xml"));
        assert!(form.start().is_some());
        assert!(form.is_running() && !form.can_add() && form.start().is_none());
    }

    #[test]
    fn clipboard_prefill_only_for_web_addresses() {
        assert_eq!(
            clipboard_address(Some("  https://example.com/feed.xml\n")).as_deref(),
            Some("https://example.com/feed.xml")
        );
        assert_eq!(
            clipboard_address(Some("http://example.com")).as_deref(),
            Some("http://example.com")
        );
        for refused in [
            Some("example.com"),
            Some("ftp://example.com/x"),
            Some("some words https://example.com"),
            Some("https://"),
            Some(""),
            None,
        ] {
            assert_eq!(clipboard_address(refused), None, "{refused:?}");
        }
        assert_eq!(
            AddFeedForm::new(Some("  https://example.com/feed.xml\n")).address,
            "https://example.com/feed.xml"
        );
        assert_eq!(AddFeedForm::new(Some("not an address")).address, "");
    }

    #[test]
    fn a_refusal_stays_in_the_dialog_without_the_log_button() {
        let mut form = AddFeedForm::new(Some("https://df.example"));
        form.start();
        let refusal = RunFailure {
            message: "Already followed: Daring Fireball.".into(),
            refusal: true,
        };
        assert_eq!(form.finished(RunEnd::Failed(refusal)), None);
        assert_eq!(
            (form.error(), form.shows_log(), form.is_running()),
            (Some("Already followed: Daring Fireball."), false, false)
        );
    }

    #[test]
    fn a_busy_refusal_shows_its_sentence_without_the_log_button() {
        let mut form = AddFeedForm::new(Some("https://df.example"));
        form.start();
        assert_eq!(form.finished(RunEnd::Refused), None);
        assert_eq!(
            (form.error(), form.shows_log()),
            (Some("A refresh is running. Try again when it ends."), false)
        );
    }

    #[test]
    fn a_failure_keeps_the_dialog_and_offers_the_log() {
        let mut form = AddFeedForm::new(Some("https://df.example"));
        form.start();
        let failure = RunFailure {
            message: "Could not reach https://df.example: timed out.".into(),
            refusal: false,
        };
        assert_eq!(form.finished(RunEnd::Failed(failure)), None);
        assert_eq!(
            (form.error(), form.shows_log()),
            (Some("Could not reach https://df.example: timed out."), true)
        );
        assert!(form.start().is_some(), "a new attempt clears the error");
        assert_eq!(form.error(), None);
    }

    #[test]
    fn a_stale_add_never_touches_a_newer_form() {
        let mut old = AddFeedForm::new(Some("https://df.example"));
        let stale = old.generation();
        old.start();
        let mut form = Some(AddFeedForm::new(Some("https://other.example")));
        form.as_mut().unwrap().start();
        // The old run fails: the new form shows no error and keeps running.
        let failure = RunFailure {
            message: "Could not reach https://df.example: timed out.".into(),
            refusal: false,
        };
        let settled = settle_add(&mut form, stale, RunEnd::Failed(failure));
        assert_eq!((settled.feed, settled.closes_form), (None, false));
        let new = form.as_ref().unwrap();
        assert_eq!((new.error(), new.is_running()), (None, true));
        // The old run succeeds: the feed is reported, the new form is neither closed nor reset.
        let settled = settle_add(&mut form, stale, RunEnd::Done(added(None, None)));
        assert_eq!(settled.feed.map(|f| f.id), Some(20));
        assert!(
            !settled.closes_form,
            "a stale run never closes a newer form"
        );
        assert!(form.as_ref().unwrap().is_running());
        // Its own run still settles it.
        let own = form.as_ref().unwrap().generation();
        let own_end = settle_add(&mut form, own, RunEnd::Done(added(None, None)));
        assert!(own_end.feed.is_some() && own_end.closes_form);
        assert!(!form.as_ref().unwrap().is_running());
    }

    #[test]
    fn success_closes_the_dialog_and_reports_the_theme() {
        let mut form = AddFeedForm::new(Some("https://df.example"));
        form.start();
        let done = form
            .finished(RunEnd::Done(added(
                Some("Tech, Apple and geeks"),
                Some("Six Colors"),
            )))
            .unwrap();
        assert_eq!(
            added_note(&done),
            "In Tech, Apple and geeks, like Six Colors."
        );
        assert_eq!((form.error(), form.is_running()), (None, false));
    }

    #[test]
    fn a_finished_add_after_cancel_still_reports() {
        // The bridge drops the form on Cancel; the run's end is still a Done with the feed to select.
        let end: RunEnd<AddedFeed> = RunEnd::Done(added(None, None));
        let RunEnd::Done(feed) = end else {
            unreachable!()
        };
        assert_eq!(added_note(&feed), "Added to Uncharted waters.");
    }

    #[test]
    fn note_wording() {
        assert_eq!(
            added_note(&added(Some("Tech"), Some("B"))),
            "In Tech, like B."
        );
        assert_eq!(added_note(&added(Some("Tech"), None)), "In Tech.");
        assert_eq!(added_note(&added(None, None)), "Added to Uncharted waters.");
        assert_eq!(
            added_note(&added(None, Some("B"))),
            "Added to Uncharted waters."
        );
    }

    #[test]
    fn the_add_job_refuses_private_known_buried_and_blank_addresses() {
        let db = TestDb::new();
        db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
        let app = Ressac::open(&db.path).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let try_add = |address: &str| {
            runtime
                .block_on(add_feed(
                    &app,
                    address,
                    None,
                    None,
                    &NullSink,
                    Arc::new(AtomicBool::new(false)),
                ))
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            try_add("http://127.0.0.1:9/feed"),
            "Ressac only follows public web addresses."
        );
        assert_eq!(
            try_add("https://www.xkcd.com/rss.xml"),
            "Already followed: xkcd.com."
        );
        assert_eq!(
            try_add(
                "http://api.flickr.com/services/feeds/photos_public.gne?id=48609153@N00&lang=en-us&format=rss_200"
            ),
            "Already in the Cemetery: Photos from bouquiniste."
        );
        assert_eq!(try_add(" "), "Enter a web address.");
        let folder_gone = runtime
            .block_on(add_feed(
                &app,
                "http://127.0.0.1:9/feed",
                None,
                Some(99),
                &NullSink,
                Arc::new(AtomicBool::new(false)),
            ))
            .unwrap_err();
        assert_eq!(folder_gone.to_string(), "No folder #99.");
    }

    // FirstRunTests
    #[test]
    fn the_summary_describes_the_dry_run() {
        assert_eq!(
            import_summary(&report(
                186,
                16,
                &[("Daring Fireball (1)", "Daring Fireball")],
                3
            )),
            [
                "186 feeds in 16 new folders.",
                "1 duplicate inside the file will be merged:",
                "“Daring Fireball (1)” into “Daring Fireball”",
                "3 feeds already in Ressac will be skipped.",
            ]
        );
        assert_eq!(
            import_summary(&report(0, 0, &[], 1)),
            [
                "No new feeds in this file.",
                "1 feed already in Ressac will be skipped."
            ]
        );
        assert_eq!(
            import_summary(&report(1, 1, &[], 0)),
            ["1 feed in 1 new folder."]
        );
        assert_eq!(
            import_title(Path::new("/tmp/My Feeds.opml")),
            "Import “My Feeds.opml”?"
        );
    }

    #[test]
    fn the_preview_reads_the_dry_run_counts_and_writes_nothing() {
        let db = TestDb::new();
        let app = Ressac::open(&db.path).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let before = app.store().feed_count().unwrap();
        let got = preview(&app, &walk_opml(dir.path())).unwrap();
        assert_eq!(got, report(3, 1, &[("Alpha again", "Alpha")], 0));
        assert_eq!(app.store().feed_count().unwrap(), before);
        let imported = import(&app, &walk_opml(dir.path())).unwrap();
        assert_eq!(imported.feeds_added, 3);
        assert_eq!(app.store().feed_count().unwrap(), before + 3);
    }

    #[test]
    fn preview_failures_read_like_the_cli() {
        let db = TestDb::new();
        let app = Ressac::open(&db.path).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.opml");
        assert_eq!(
            preview(&app, &missing).unwrap_err().to_string(),
            format!(
                "cannot read {}: No such file or directory (os error 2)",
                missing.display()
            )
        );
        let bad = dir.path().join("bad.opml");
        std::fs::write(&bad, r#"<opml><body><outline text="x"></body>"#).unwrap();
        assert!(
            preview(&app, &bad)
                .unwrap_err()
                .to_string()
                .starts_with("cannot read OPML at line 1, column 31: ")
        );
    }

    #[test]
    fn only_opml_files_are_accepted() {
        assert_eq!(
            first_opml(&[
                "file:///tmp/feeds.xml".into(),
                "https://example.org/a.opml".into()
            ]),
            None
        );
        assert_eq!(
            first_opml(&[
                "file:///tmp/notes.txt".into(),
                "file:///tmp/My%20Feeds.OPML".into()
            ]),
            Some(PathBuf::from("/tmp/My Feeds.OPML"))
        );
        assert_eq!(
            first_opml(&["file:///tmp/My Feeds.opml".into()]),
            Some(PathBuf::from("/tmp/My Feeds.opml"))
        );
        assert_eq!(
            local_path("file:///tmp/a%20b/Ressac.opml"),
            Some(PathBuf::from("/tmp/a b/Ressac.opml"))
        );
        assert_eq!(local_path("ressac:asset/x"), None);
    }

    #[test]
    fn confirming_imports_then_fetches_everything() {
        let mut flow = OpmlFlow::default();
        let path = flow
            .open(&[
                "file:///tmp/notes.txt".into(),
                "file:///tmp/vienna.opml".into(),
            ])
            .unwrap();
        assert_eq!(path, PathBuf::from("/tmp/vienna.opml"));
        assert!(flow.is_busy());
        flow.previewed(&path, Ok(report(186, 16, &[], 3)));
        assert_eq!(flow.step().as_str(), "confirming");
        assert!(flow.can_import(false));
        assert_eq!(flow.confirm(false), Some(RunRequest::Import(path.clone())));
        assert!(flow.is_busy());
        assert_eq!(
            flow.imported(&RunEnd::Done(report(186, 16, &[], 3))),
            Some(RunRequest::FetchAll)
        );
        assert!(
            flow.is_busy(),
            "busy until the fetch ends, as the Mac's spinner"
        );
        flow.fetched();
        assert_eq!(flow.step(), &OpmlStep::Idle);
    }

    #[test]
    fn import_is_blocked_with_a_message_while_a_refresh_runs() {
        let mut flow = OpmlFlow::default();
        let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
        flow.previewed(&path, Ok(report(2, 0, &[], 0)));
        assert!(!flow.can_import(true));
        assert_eq!(flow.confirm(true), None);
        assert_eq!(flow.step().as_str(), "confirming");
        assert_eq!(
            IMPORT_BLOCKED,
            "A refresh is running; try again when it ends."
        );
        assert_eq!(flow.confirm(false), Some(RunRequest::Import(path.clone())));
        assert_eq!(
            flow.imported(&RunEnd::Refused),
            None,
            "turned away anyway: back to the confirmation"
        );
        assert_eq!(flow.step().as_str(), "confirming");
    }

    #[test]
    fn a_file_with_no_new_feeds_cannot_be_imported() {
        let mut flow = OpmlFlow::default();
        let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
        flow.previewed(&path, Ok(report(0, 0, &[], 12)));
        assert!(!flow.can_import(false));
        assert_eq!(flow.confirm(false), None);
    }

    #[test]
    fn a_failed_import_does_not_fetch() {
        let mut flow = OpmlFlow::default();
        let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
        flow.previewed(&path, Ok(report(2, 0, &[], 0)));
        flow.confirm(false);
        assert_eq!(
            flow.imported(&RunEnd::Failed(RunFailure {
                message: "database error: x".into(),
                refusal: false
            })),
            None
        );
        assert_eq!(flow.step(), &OpmlStep::Idle);
    }

    #[test]
    fn preview_failures_are_shown_and_nothing_is_written() {
        let mut flow = OpmlFlow::default();
        let path = flow.open(&["file:///tmp/vienna.opml".into()]).unwrap();
        flow.previewed(&path, Err("invalid OPML".into()));
        assert_eq!(flow.step(), &OpmlStep::Failed("invalid OPML".into()));
        assert!(!flow.is_busy());
        flow.cancel();
        assert_eq!(flow.step(), &OpmlStep::Idle);
    }

    #[test]
    fn a_drop_during_a_preview_is_ignored() {
        let mut flow = OpmlFlow::default();
        let first = flow.open(&["file:///tmp/a.opml".into()]).unwrap();
        assert_eq!(flow.open(&["file:///tmp/b.opml".into()]), None);
        flow.previewed(Path::new("/tmp/b.opml"), Ok(report(9, 0, &[], 0))); // not the one being previewed
        assert_eq!(flow.step(), &OpmlStep::Previewing(first));
        flow.cancel();
        assert!(flow.is_busy(), "a preview under way cannot be cancelled");
    }

    #[test]
    fn export_writes_the_active_feeds() {
        let db = TestDb::new();
        db.execute("UPDATE feed SET state = 'buried' WHERE id = 4");
        let store = db.store();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(EXPORT_NAME);
        export(&store, &target).unwrap();
        let xml = std::fs::read_to_string(&target).unwrap();
        assert!(xml.contains(r#"xmlUrl="https://www.xkcd.com/rss.xml""#));
        assert!(
            !xml.contains("Photos from bouquiniste"),
            "the buried feed is not exported"
        );
        let request = RunRequest::Export {
            path: target,
            cemetery: false,
        };
        assert_eq!(request.kind(), RunKind::Short);
        assert!(export(&store, &dir.path().join("no/such/dir/x.opml")).is_err());
    }

    // MoveFeedTests
    #[test]
    fn move_request_is_short_and_its_refusals_are_sentences() {
        assert_eq!(RunRequest::MoveFeed(7, Some(3)).kind(), RunKind::Short);
        assert!(RunRequest::MoveFeed(7, Some(3)).is_refusal(&ressac_core::Error::UnknownFolder(3)));
    }

    #[test]
    fn drop_on_own_folder_is_ignored() {
        assert_eq!(decide_drop(Some(3), Some(3)), FeedDrop::Ignore);
        assert_eq!(
            decide_drop(None, None),
            FeedDrop::Ignore,
            "a top-level feed dropped on the Feeds heading stays"
        );
    }

    #[test]
    fn drop_maps_a_feed_onto_a_folder() {
        assert_eq!(decide_drop(None, Some(3)), FeedDrop::Move(Some(3)));
        assert_eq!(decide_drop(Some(2), Some(3)), FeedDrop::Move(Some(3)));
        assert_eq!(
            decide_drop(Some(2), None),
            FeedDrop::Move(None),
            "the Feeds heading is the top level"
        );
    }

    #[test]
    fn payload_round_trips_and_foreign_strings_are_ignored() {
        assert_eq!(FEED_MIME, "application/x-ressac-feed");
        assert_eq!(feed_payload(42), "42");
        assert_eq!(feed_from_payload("42"), Some(42));
        for foreign in [
            "",
            "x",
            "1 2",
            "-1",
            "+3",
            " 4",
            "4 ",
            "ressac-feed:4",
            "４２",
            "99999999999999999999",
        ] {
            assert_eq!(feed_from_payload(foreign), None, "{foreign:?}");
        }
    }

    #[test]
    fn drop_targets_are_folders_and_the_feeds_heading() {
        assert_eq!(drop_target("folder:7"), Some(Some(7)));
        assert_eq!(drop_target("heading:Feeds"), Some(None));
        for other in [
            "feed:7",
            "heading:Rhythm",
            "unread",
            "garden:all",
            "folder:",
            "folder:x",
        ] {
            assert_eq!(drop_target(other), None, "{other}");
        }
    }

    #[test]
    fn an_entry_drags_out_as_its_link_and_title() {
        assert_eq!(
            entry_drag_data("https://example.org/a", "A title"),
            [
                ("text/uri-list", "https://example.org/a\r\n".to_string()),
                ("text/plain", "https://example.org/a".to_string()),
                (
                    "text/x-moz-url",
                    "https://example.org/a\nA title".to_string()
                ),
            ]
        );
        assert!(entry_drag_data("javascript:alert(1)", "x").is_empty());
        assert!(entry_drag_data("", "x").is_empty());
    }

    fn moz_url_lines(data: &[(&'static str, String)]) -> Vec<String> {
        let (_, text) = data.iter().find(|(m, _)| *m == "text/x-moz-url").unwrap();
        text.split(['\n', '\r', '\u{2028}', '\u{2029}', '\u{85}'])
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_hostile_title_cannot_smuggle_a_url_into_the_moz_url_list() {
        for title in [
            "a\r\nhttps://evil.example/\nb",
            "a\u{2028}https://evil.example/\u{2029}b",
            "a\u{85}https://evil.example/\tb",
        ] {
            let data = entry_drag_data("https://example.org/a", title);
            assert_eq!(
                moz_url_lines(&data),
                ["https://example.org/a", "a https://evil.example/ b"],
                "{title:?}"
            );
        }
        let data = entry_drag_data("https://example.org/a", "  x \r\n\r\n y  ");
        assert_eq!(moz_url_lines(&data)[1], "x y");
    }

    #[test]
    fn a_link_with_a_control_character_is_not_dragged() {
        for link in [
            "https://example.org/a\nhttps://evil.example/",
            "https://example.org/a\u{2028}b",
            "https://example.org/a\u{85}b",
            "https://example.org/a\u{2029}b",
        ] {
            assert!(entry_drag_data(link, "t").is_empty(), "{link:?}");
        }
    }
}
