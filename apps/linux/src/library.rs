//! The `Library` QML singleton: the Add Feed form and its run, the OPML preview / import /
//! export flow and the note under the window. The rules (form, flow, texts, jobs) are
//! `ressac_ui::library`'s; this object translates them to Qt, starts the runs through
//! `runs::submit_long` / `submit_short` and brings their ends back to the GUI thread.

use std::pin::Pin;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QStringList, QVariant};
use ressac_core::clock::today_start;
use ressac_core::paths::Paths;
use ressac_core::service::AddedFeed;
use ressac_core::store::{ImportReport, ThemeDef};
use ressac_ui::chrono::Local;
use ressac_ui::garden::{theme_choice, theme_index, theme_names};
use ressac_ui::library::{
    self, ADD_TITLE, ADDRESS_PLACEHOLDER, AddFeedForm, CANT_IMPORT, CHOOSE_OPML, EXPORT_NAME,
    FEED_MIME, FolderChoice, IMPORT_BLOCKED, IMPORT_NOTE, NO_FEEDS_TEXT, NO_FEEDS_TITLE,
    NOTE_SECONDS, OPML_FILTER, OpmlFlow, OpmlStep, added_note, entry_drag_data, folder_choices,
    import_summary, import_title, local_path,
};
use ressac_ui::runs::{self, RunEnd, RunRequest};
use ressac_ui::worker::JobResult;

use crate::launcher::with_session;
use crate::runs::{submit_long, submit_short};
use crate::shim;

const NO_DATABASE: &str = "The database is not open.";

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, add_open, READ = add_open, NOTIFY = add_changed)]
        #[qproperty(QString, address, READ = address, WRITE = set_address, NOTIFY = add_changed)]
        #[qproperty(i32, theme_index, READ = theme_index, WRITE = set_theme_index, NOTIFY = add_changed)]
        #[qproperty(i32, folder_index, READ = folder_index, WRITE = set_folder_index, NOTIFY = add_changed)]
        #[qproperty(QStringList, theme_names, READ = theme_names, NOTIFY = add_changed)]
        #[qproperty(QStringList, folder_titles, READ = folder_titles, NOTIFY = add_changed)]
        #[qproperty(bool, can_add, READ = can_add, NOTIFY = add_changed)]
        #[qproperty(bool, adding, READ = adding, NOTIFY = add_changed)]
        #[qproperty(QString, add_error, READ = add_error, NOTIFY = add_changed)]
        #[qproperty(bool, add_shows_log, READ = add_shows_log, NOTIFY = add_changed)]
        #[qproperty(QString, note, READ = note, NOTIFY = note_changed)]
        #[qproperty(QString, opml_step, READ = opml_step, NOTIFY = opml_changed)]
        #[qproperty(bool, busy, READ = busy, NOTIFY = opml_changed)]
        #[qproperty(QString, import_title, READ = import_title, NOTIFY = opml_changed)]
        #[qproperty(QStringList, import_lines, READ = import_lines, NOTIFY = opml_changed)]
        #[qproperty(QString, import_note, READ = import_note, NOTIFY = opml_changed)]
        #[qproperty(bool, can_import, READ = can_import, NOTIFY = opml_changed)]
        #[qproperty(QString, failure, READ = failure, NOTIFY = opml_changed)]
        #[qproperty(QString, add_title, READ = add_title, CONSTANT)]
        #[qproperty(QString, address_placeholder, READ = address_placeholder, CONSTANT)]
        #[qproperty(QString, cant_import, READ = cant_import, CONSTANT)]
        #[qproperty(QString, import_blocked, READ = import_blocked, CONSTANT)]
        #[qproperty(QString, no_feeds_title, READ = no_feeds_title, CONSTANT)]
        #[qproperty(QString, no_feeds_text, READ = no_feeds_text, CONSTANT)]
        #[qproperty(QString, choose_opml, READ = choose_opml, CONSTANT)]
        #[qproperty(QString, export_name, READ = export_name, CONSTANT)]
        #[qproperty(QString, opml_filter, READ = opml_filter, CONSTANT)]
        #[qproperty(QString, feed_mime, READ = feed_mime, CONSTANT)]
        #[qproperty(i32, note_seconds, READ = note_seconds, CONSTANT)]
        type Library = super::LibraryRust;

        fn add_open(self: &Library) -> bool;
        fn address(self: &Library) -> QString;
        fn set_address(self: Pin<&mut Library>, value: &QString);
        fn theme_index(self: &Library) -> i32;
        fn set_theme_index(self: Pin<&mut Library>, value: i32);
        fn folder_index(self: &Library) -> i32;
        fn set_folder_index(self: Pin<&mut Library>, value: i32);
        fn theme_names(self: &Library) -> QStringList;
        fn folder_titles(self: &Library) -> QStringList;
        fn can_add(self: &Library) -> bool;
        fn adding(self: &Library) -> bool;
        fn add_error(self: &Library) -> QString;
        fn add_shows_log(self: &Library) -> bool;
        fn note(self: &Library) -> QString;
        fn opml_step(self: &Library) -> QString;
        fn busy(self: &Library) -> bool;
        fn import_title(self: &Library) -> QString;
        fn import_lines(self: &Library) -> QStringList;
        fn import_note(self: &Library) -> QString;
        fn can_import(self: &Library) -> bool;
        fn failure(self: &Library) -> QString;
        fn add_title(self: &Library) -> QString;
        fn address_placeholder(self: &Library) -> QString;
        fn cant_import(self: &Library) -> QString;
        fn import_blocked(self: &Library) -> QString;
        fn no_feeds_title(self: &Library) -> QString;
        fn no_feeds_text(self: &Library) -> QString;
        fn choose_opml(self: &Library) -> QString;
        fn export_name(self: &Library) -> QString;
        fn opml_filter(self: &Library) -> QString;
        fn feed_mime(self: &Library) -> QString;
        fn note_seconds(self: &Library) -> i32;

        /// Ctrl+N: opens the form, the address prefilled from the clipboard when it holds a
        /// web address.
        #[qinvokable]
        fn begin_add(self: Pin<&mut Library>);

        /// Re-reads the Theme and Folder pop-ups (the library changed under an open form).
        #[qinvokable]
        fn reload_choices(self: Pin<&mut Library>);

        /// The Add button: starts the add as a long run (the form stays open meanwhile).
        #[qinvokable]
        fn add(self: Pin<&mut Library>);

        /// Closes the form; a running add continues and its feed still gets its note.
        #[qinvokable]
        fn cancel_add(self: Pin<&mut Library>);

        #[qinvokable]
        fn dismiss_note(self: Pin<&mut Library>);

        /// A drop or a chosen file: previews the first `.opml` among `urls`; whether a
        /// preview started.
        #[qinvokable]
        fn open_urls(self: Pin<&mut Library>, urls: &QStringList) -> bool;

        /// The confirmation's Import button; `blocked`: another long run is active.
        #[qinvokable]
        fn confirm_import(self: Pin<&mut Library>, blocked: bool);

        #[qinvokable]
        fn cancel_import(self: Pin<&mut Library>);

        /// Writes the active feeds as OPML to the `file:` URL; false when it is not one.
        #[qinvokable]
        fn export_to(self: Pin<&mut Library>, url: &QString) -> bool;

        /// The log folder as a `file:` URL ("" when it cannot be made).
        #[qinvokable]
        fn logs_url(self: &Library) -> QString;

        /// The mime data of an entry dragged out ({} when the link is not a web address).
        #[qinvokable]
        fn entry_drag_data(
            self: &Library,
            link: &QString,
            title: &QString,
        ) -> QMap_QString_QVariant;

        #[qsignal]
        fn add_changed(self: Pin<&mut Library>);

        #[qsignal]
        fn note_changed(self: Pin<&mut Library>);

        #[qsignal]
        fn opml_changed(self: Pin<&mut Library>);

        /// A feed was added: `feed:<id>`, the selection key of its sidebar row.
        #[qsignal]
        fn added(self: Pin<&mut Library>, key: &QString);
    }

    impl cxx_qt::Threading for Library {}
}

#[derive(Default)]
pub struct LibraryRust {
    form: Option<AddFeedForm>,
    themes: Vec<ThemeDef>,
    folders: Vec<FolderChoice>,
    note: String,
    flow: OpmlFlow,
    /// The message of an import that failed: the flow is back to idle by then, the dialog shows
    /// it from the run's end.
    import_failure: Option<String>,
}

/// The form's pop-ups from the database; an error keeps the old lists.
fn load_choices(state: &mut LibraryRust) {
    let Some(form) = state.form.as_mut() else {
        return;
    };
    let read = with_session(|session| {
        let themes = session.store.themes()?;
        let sources = session.store.sources(today_start(Local::now()))?;
        Ok::<_, ressac_core::Error>((themes, sources))
    });
    match read {
        Some(Ok((themes, sources))) => {
            state.themes = themes;
            state.folders = folder_choices(&sources.tree);
            form.keep_folder(&state.folders);
        }
        Some(Err(error)) => tracing::warn!("cannot read the add form's choices: {error}"),
        None => {}
    }
}

fn string_list<'a>(items: impl IntoIterator<Item = &'a str>) -> QStringList {
    items.into_iter().map(QString::from).collect()
}

fn index(position: usize) -> i32 {
    i32::try_from(position).unwrap_or(0)
}

impl qobject::Library {
    fn add_open(&self) -> bool {
        self.rust().form.is_some()
    }

    fn address(&self) -> QString {
        QString::from(self.rust().form.as_ref().map_or("", |f| f.address.as_str()))
    }

    fn set_address(mut self: Pin<&mut Self>, value: &QString) {
        let Some(form) = self.as_mut().rust_mut().get_mut().form.as_mut() else {
            return;
        };
        form.address = value.to_string();
        self.add_changed();
    }

    fn theme_index(&self) -> i32 {
        let rust = self.rust();
        rust.form.as_ref().map_or(0, |form| {
            index(theme_index(&rust.themes, form.theme.as_deref()))
        })
    }

    fn set_theme_index(mut self: Pin<&mut Self>, value: i32) {
        let rust = self.as_mut().rust_mut().get_mut();
        let Some(form) = rust.form.as_mut() else {
            return;
        };
        let Some(choice) = usize::try_from(value)
            .ok()
            .and_then(|i| theme_choice(&rust.themes, i))
        else {
            return;
        };
        form.theme = choice;
        self.add_changed();
    }

    fn folder_index(&self) -> i32 {
        let rust = self.rust();
        rust.form.as_ref().map_or(0, |form| {
            index(
                rust.folders
                    .iter()
                    .position(|c| c.folder == form.folder)
                    .unwrap_or(0),
            )
        })
    }

    fn set_folder_index(mut self: Pin<&mut Self>, value: i32) {
        let rust = self.as_mut().rust_mut().get_mut();
        let Some(form) = rust.form.as_mut() else {
            return;
        };
        let Some(choice) = usize::try_from(value)
            .ok()
            .and_then(|i| rust.folders.get(i))
        else {
            return;
        };
        form.folder = choice.folder;
        self.add_changed();
    }

    fn theme_names(&self) -> QStringList {
        let names = theme_names(&self.rust().themes);
        string_list(names.iter().map(String::as_str))
    }

    fn folder_titles(&self) -> QStringList {
        let titles: Vec<String> = self
            .rust()
            .folders
            .iter()
            .map(FolderChoice::title)
            .collect();
        string_list(titles.iter().map(String::as_str))
    }

    fn can_add(&self) -> bool {
        self.rust().form.as_ref().is_some_and(AddFeedForm::can_add)
    }

    fn adding(&self) -> bool {
        self.rust()
            .form
            .as_ref()
            .is_some_and(AddFeedForm::is_running)
    }

    fn add_error(&self) -> QString {
        let error = self.rust().form.as_ref().and_then(AddFeedForm::error);
        QString::from(error.unwrap_or(""))
    }

    fn add_shows_log(&self) -> bool {
        self.rust()
            .form
            .as_ref()
            .is_some_and(AddFeedForm::shows_log)
    }

    fn note(&self) -> QString {
        QString::from(self.rust().note.as_str())
    }

    fn opml_step(&self) -> QString {
        let rust = self.rust();
        if rust.import_failure.is_some() {
            return QString::from("failed");
        }
        QString::from(rust.flow.step().as_str())
    }

    fn busy(&self) -> bool {
        self.rust().flow.is_busy()
    }

    fn import_title(&self) -> QString {
        match self.rust().flow.step() {
            OpmlStep::Confirming(path, _) => QString::from(import_title(path).as_str()),
            _ => QString::default(),
        }
    }

    fn import_lines(&self) -> QStringList {
        match self.rust().flow.step() {
            OpmlStep::Confirming(_, report) => {
                let lines = import_summary(report);
                string_list(lines.iter().map(String::as_str))
            }
            _ => QStringList::default(),
        }
    }

    fn import_note(&self) -> QString {
        match self.rust().flow.step() {
            OpmlStep::Confirming(_, report) if report.feeds_added > 0 => QString::from(IMPORT_NOTE),
            _ => QString::default(),
        }
    }

    fn can_import(&self) -> bool {
        self.rust().flow.can_import(false)
    }

    fn failure(&self) -> QString {
        let rust = self.rust();
        match (&rust.import_failure, rust.flow.step()) {
            (Some(message), _) | (None, OpmlStep::Failed(message)) => {
                QString::from(message.as_str())
            }
            _ => QString::default(),
        }
    }

    fn add_title(&self) -> QString {
        QString::from(ADD_TITLE)
    }

    fn address_placeholder(&self) -> QString {
        QString::from(ADDRESS_PLACEHOLDER)
    }

    fn cant_import(&self) -> QString {
        QString::from(CANT_IMPORT)
    }

    fn import_blocked(&self) -> QString {
        QString::from(IMPORT_BLOCKED)
    }

    fn no_feeds_title(&self) -> QString {
        QString::from(NO_FEEDS_TITLE)
    }

    fn no_feeds_text(&self) -> QString {
        QString::from(NO_FEEDS_TEXT)
    }

    fn choose_opml(&self) -> QString {
        QString::from(CHOOSE_OPML)
    }

    fn export_name(&self) -> QString {
        QString::from(EXPORT_NAME)
    }

    fn opml_filter(&self) -> QString {
        QString::from(OPML_FILTER)
    }

    fn feed_mime(&self) -> QString {
        QString::from(FEED_MIME)
    }

    fn note_seconds(&self) -> i32 {
        i32::try_from(NOTE_SECONDS).unwrap_or(6)
    }

    fn begin_add(mut self: Pin<&mut Self>) {
        let rust = self.as_mut().rust_mut().get_mut();
        rust.form = Some(AddFeedForm::new(Some(&shim::ressac_clipboard_text())));
        load_choices(rust);
        self.add_changed();
    }

    fn reload_choices(mut self: Pin<&mut Self>) {
        let rust = self.as_mut().rust_mut().get_mut();
        if rust.form.is_none() {
            return;
        }
        load_choices(rust);
        self.add_changed();
    }

    fn add(mut self: Pin<&mut Self>) {
        let started = self
            .as_mut()
            .rust_mut()
            .get_mut()
            .form
            .as_mut()
            .and_then(|form| form.start().map(|request| (form.generation(), request)));
        let Some((generation, request)) = started else {
            return;
        };
        let RunRequest::Add {
            address,
            theme,
            folder,
        } = request.clone()
        else {
            return;
        };
        let qt = self.qt_thread();
        submit_long(
            request,
            move |cx, sink, stop| {
                cx.block_on(library::add_feed(
                    cx.app,
                    &address,
                    theme.as_deref(),
                    folder,
                    sink,
                    stop,
                ))
            },
            |_| None,
            |_| {},
            move |end: RunEnd<AddedFeed>| {
                queue_on(&qt, "the add ended", move |library| {
                    library.add_finished(generation, end)
                });
            },
        );
        self.add_changed();
    }

    /// The end of the add started by the form `generation`: only that form takes it; after a
    /// cancel (and maybe a new form) the end is the no-form path (the note and `added`).
    fn add_finished(mut self: Pin<&mut Self>, generation: u64, end: RunEnd<AddedFeed>) {
        let rust = self.as_mut().rust_mut().get_mut();
        let feed = library::settle_add(&mut rust.form, generation, end);
        if let Some(feed) = feed {
            if rust
                .form
                .as_ref()
                .is_some_and(|f| f.generation() == generation)
            {
                rust.form = None;
            }
            rust.note = added_note(&feed);
            self.as_mut().note_changed();
            self.as_mut()
                .added(&QString::from(format!("feed:{}", feed.id).as_str()));
        }
        self.add_changed();
    }

    fn cancel_add(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().form = None;
        self.add_changed();
    }

    fn dismiss_note(mut self: Pin<&mut Self>) {
        if self.rust().note.is_empty() {
            return;
        }
        self.as_mut().rust_mut().get_mut().note.clear();
        self.note_changed();
    }

    fn open_urls(mut self: Pin<&mut Self>, urls: &QStringList) -> bool {
        let urls: Vec<String> = urls.iter().map(QString::to_string).collect();
        let rust = self.as_mut().rust_mut().get_mut();
        let Some(path) = rust.flow.open(&urls) else {
            return false;
        };
        rust.import_failure = None;
        let qt = self.qt_thread();
        let job_path = path.clone();
        let started = with_session(|session| {
            session
                .worker
                .short(move |cx| library::preview(cx.app, &job_path), {
                    let path = path.clone();
                    move |result: JobResult<ImportReport>| {
                        let result = result.map_err(|failure| failure.message());
                        queue_on(&qt, "the preview ended", move |library| {
                            library.preview_done(&path, result);
                        });
                    }
                });
        });
        if started.is_none() {
            self.as_mut()
                .rust_mut()
                .get_mut()
                .flow
                .previewed(&path, Err(NO_DATABASE.into()));
        }
        self.opml_changed();
        true
    }

    fn preview_done(
        mut self: Pin<&mut Self>,
        path: &std::path::Path,
        result: Result<ImportReport, String>,
    ) {
        self.as_mut()
            .rust_mut()
            .get_mut()
            .flow
            .previewed(path, result);
        self.opml_changed();
    }

    fn confirm_import(mut self: Pin<&mut Self>, blocked: bool) {
        let Some(request) = self.as_mut().rust_mut().get_mut().flow.confirm(blocked) else {
            return;
        };
        let RunRequest::Import(path) = request.clone() else {
            return;
        };
        let qt = self.qt_thread();
        submit_long(
            request,
            move |cx, _, _| library::import(cx.app, &path),
            |_| None,
            |_| {},
            move |end: RunEnd<ImportReport>| {
                queue_on(&qt, "the import ended", move |library| {
                    library.import_done(end);
                });
            },
        );
        self.opml_changed();
    }

    /// The import's end: a failure's message is kept from the run's end (the flow goes back to
    /// idle without it); a success goes on to the fetch of every feed.
    fn import_done(mut self: Pin<&mut Self>, end: RunEnd<ImportReport>) {
        let rust = self.as_mut().rust_mut().get_mut();
        let importing = matches!(rust.flow.step(), OpmlStep::Importing(..));
        let next = rust.flow.imported(&end);
        if let (true, RunEnd::Failed(failure)) = (importing, &end) {
            rust.import_failure = Some(failure.message.clone());
        }
        if let Some(request) = next {
            let qt = self.qt_thread();
            submit_long(
                request,
                move |cx, sink, stop| {
                    cx.block_on(runs::fetch(cx.app, &RunRequest::FetchAll, sink, stop))
                },
                |_| None,
                |_| {},
                move |_: RunEnd<()>| {
                    queue_on(&qt, "the fetch after the import ended", |library| {
                        library.fetch_done();
                    });
                },
            );
        }
        self.opml_changed();
    }

    fn fetch_done(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().flow.fetched();
        self.opml_changed();
    }

    fn cancel_import(mut self: Pin<&mut Self>) {
        let rust = self.as_mut().rust_mut().get_mut();
        rust.import_failure = None;
        rust.flow.cancel();
        self.opml_changed();
    }

    fn export_to(self: Pin<&mut Self>, url: &QString) -> bool {
        let Some(path) = local_path(&url.to_string()) else {
            return false;
        };
        submit_short(
            RunRequest::Export {
                path: path.clone(),
                cemetery: false,
            },
            move |cx| library::export(cx.app.store(), &path),
            |_| {},
        );
        true
    }

    fn logs_url(&self) -> QString {
        let dir = Paths::from_env().map(|paths| paths.log_dir());
        let url = dir.ok().and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            url::Url::from_directory_path(&dir).ok()
        });
        QString::from(url.map_or(String::new(), |url| url.to_string()).as_str())
    }

    fn entry_drag_data(&self, link: &QString, title: &QString) -> QMap<QMapPair_QString_QVariant> {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (mime, text) in entry_drag_data(&link.to_string(), &title.to_string()) {
            map.insert(
                QString::from(mime),
                QVariant::from(&QString::from(text.as_str())),
            );
        }
        map
    }
}

/// Queues `f` onto `Library` from another thread; the app quitting is not an error.
fn queue_on(
    qt: &CxxQtThread<qobject::Library>,
    what: &str,
    f: impl FnOnce(Pin<&mut qobject::Library>) + Send + 'static,
) {
    if qt.queue(f).is_err() {
        tracing::debug!("the app quit before {what}");
    }
}
