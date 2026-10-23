//! The `Journal` QML singleton: where the morning edition stands (the phase machine is
//! `ressac_ui::journal::JournalModel`), its header line, placeholder and end line. It starts
//! the build as a long run through `runs::submit_long`, so the one coordinator keeps its
//! banner and its reloads; the list's rows are `EntriesModel`'s.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use ressac_core::service::{EditionOptions, EditionOutcome, Event};
use ressac_core::store::JournalView;
use ressac_ui::chrono::{Local, Utc};
use ressac_ui::journal::{
    END_LINE, JournalModel, Phase, RETRY, Step, Trigger, edition_failure, header, is_due,
    is_finished, placeholder,
};
use ressac_ui::runs::{RunEnd, RunRequest, read_failure};

use crate::launcher::with_session;
use crate::runs::submit_long;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, phase, READ = phase, NOTIFY = state_changed)]
        #[qproperty(bool, building, READ = building, NOTIFY = state_changed)]
        #[qproperty(QString, placeholder, READ = placeholder, NOTIFY = state_changed)]
        #[qproperty(bool, offers_retry, READ = offers_retry, NOTIFY = state_changed)]
        #[qproperty(QString, header, READ = header, NOTIFY = state_changed)]
        #[qproperty(bool, finished, READ = finished, NOTIFY = state_changed)]
        #[qproperty(QString, end_line, READ = end_line, CONSTANT)]
        #[qproperty(QString, retry_title, READ = retry_title, CONSTANT)]
        type Journal = super::JournalRust;

        fn phase(self: &Journal) -> QString;
        fn building(self: &Journal) -> bool;
        fn placeholder(self: &Journal) -> QString;
        fn offers_retry(self: &Journal) -> bool;
        fn header(self: &Journal) -> QString;
        fn finished(self: &Journal) -> bool;
        fn end_line(self: &Journal) -> QString;
        fn retry_title(self: &Journal) -> QString;

        /// Re-reads the latest edition (the header, the placeholder, the end line).
        #[qinvokable]
        fn reload(self: Pin<&mut Journal>);

        /// Launch and the window becoming active: builds today's edition when it is due
        /// (an automatic trigger waits out a recent failure).
        #[qinvokable]
        fn trigger(self: Pin<&mut Journal>);

        /// The Retry button: builds now, whatever the wait.
        #[qinvokable]
        fn retry(self: Pin<&mut Journal>);

        /// The 60 s timer: did the 04:00 day move, or is a failed build's wait over?
        #[qinvokable]
        fn minute_tick(self: Pin<&mut Journal>);

        /// The phase, the edition or its texts changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut Journal>);

        /// A failed read: for the window's banner.
        #[qsignal]
        fn banner(self: Pin<&mut Journal>, message: &QString, show_log: bool);
    }

    impl cxx_qt::Threading for Journal {}
}

#[derive(Default)]
pub struct JournalRust {
    model: JournalModel,
    /// The latest edition, for the header, the placeholder and the end line.
    view: Option<JournalView>,
}

impl qobject::Journal {
    fn phase(&self) -> QString {
        QString::from(self.rust().model.phase().as_str())
    }

    fn building(&self) -> bool {
        matches!(self.rust().model.phase(), Phase::Building { .. })
    }

    fn placeholder(&self) -> QString {
        let rust = self.rust();
        let text = placeholder(rust.model.phase(), rust.view.as_ref());
        QString::from(text.unwrap_or_default().as_str())
    }

    fn offers_retry(&self) -> bool {
        self.rust().model.phase().offers_retry()
    }

    fn header(&self) -> QString {
        let text = self.rust().view.as_ref().map(|view| header(view, &Local));
        QString::from(text.unwrap_or_default().as_str())
    }

    fn finished(&self) -> bool {
        self.rust().view.as_ref().is_some_and(is_finished)
    }

    fn end_line(&self) -> QString {
        QString::from(END_LINE)
    }

    fn retry_title(&self) -> QString {
        QString::from(RETRY)
    }

    fn reload(mut self: Pin<&mut Self>) {
        match with_session(|session| session.store.journal()) {
            Some(Ok(view)) => self.as_mut().rust_mut().get_mut().view = view,
            Some(Err(error)) => {
                let banner = read_failure(&error);
                self.as_mut()
                    .banner(&QString::from(banner.message.as_str()), banner.show_log);
            }
            None => {}
        }
        self.state_changed();
    }

    fn trigger(self: Pin<&mut Self>) {
        self.begin(Trigger::Automatic);
    }

    fn retry(self: Pin<&mut Self>) {
        self.begin(Trigger::Retry);
    }

    fn minute_tick(self: Pin<&mut Self>) {
        if self.rust().model.minute_check(Utc::now(), &Local) {
            self.begin(Trigger::Automatic);
        }
    }

    /// A trigger fired: the model decides whether to check, the store answers whether the
    /// edition is due, and a due one is built as a long run.
    fn begin(mut self: Pin<&mut Self>, trigger: Trigger) {
        let now = Utc::now();
        if !self
            .as_mut()
            .rust_mut()
            .get_mut()
            .model
            .begin(trigger, now, &Local)
        {
            return;
        }
        let due = with_session(|session| is_due(&session.store, now, &Local)).unwrap_or(Ok(false));
        let step = self.as_mut().rust_mut().get_mut().model.checked(due);
        self.as_mut().state_changed();
        if step != Step::Build {
            return;
        }
        let for_events = self.qt_thread();
        let for_end = self.qt_thread();
        submit_long(
            RunRequest::BuildEdition { rebuild: false },
            |cx, sink, stop| {
                cx.block_on(cx.app.build_edition(
                    EditionOptions::default(),
                    Utc::now(),
                    &Local,
                    sink,
                    stop,
                ))
            },
            edition_failure,
            move |event: &Event| {
                let event = event.clone();
                if for_events
                    .queue(move |journal| journal.edition_event(&event))
                    .is_err()
                {
                    tracing::debug!("the app quit during the edition build");
                }
            },
            move |end: RunEnd<EditionOutcome>| {
                if for_end
                    .queue(move |journal| journal.edition_finished(end))
                    .is_err()
                {
                    tracing::debug!("the app quit before the edition build finished");
                }
            },
        );
    }

    fn edition_event(mut self: Pin<&mut Self>, event: &Event) {
        self.as_mut().rust_mut().get_mut().model.event(event);
        self.state_changed();
    }

    fn edition_finished(mut self: Pin<&mut Self>, end: RunEnd<EditionOutcome>) {
        self.as_mut()
            .rust_mut()
            .get_mut()
            .model
            .finished(end, Utc::now());
        // Emits `state_changed` with the new edition.
        self.reload();
    }
}
