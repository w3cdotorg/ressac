//! The `Garden` singleton's inspector getters: the selected feed's `Inspector`
//! (`ressac_ui::garden`) as the properties `GardenInspector.qml` binds, and as the teletext
//! page of Antiope's reader column (`antiopeInspector`). Without a selected feed every string
//! is empty and every index −1.

use std::fmt::Display;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use ressac_ui::chrono::{DateTime, Local, TimeZone, Utc};
use ressac_ui::garden::Inspector;
use ressac_ui::pages::encode_rows;
use ressac_ui::pages::garden::inspector_page;

use crate::garden::{GardenRust, qobject::Garden, string_list};
use crate::rows::to_i32;

impl GardenRust {
    /// The selected feed's inspector as a page `antiope_cells` wide: its rhythm, themes and
    /// fetch log as of the last sync.
    pub(crate) fn antiope_inspector_rows<Tz: TimeZone>(
        &self,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Vec<String>
    where
        Tz::Offset: Display,
    {
        let cadence = self
            .visit
            .selected()
            .and_then(|feed| self.visit.snapshot().cadence.get(&feed));
        encode_rows(&inspector_page(
            self.inspector.as_ref(),
            cadence,
            &self.theme_names,
            &self.log,
            usize::try_from(self.antiope_cells).unwrap_or(0),
            now,
            tz,
        ))
    }
}

impl Garden {
    fn inspected(&self) -> Option<&Inspector> {
        self.rust().inspector.as_ref()
    }

    /// A text of the inspector; empty without one.
    fn text(&self, of: impl FnOnce(&Inspector) -> &str) -> QString {
        QString::from(self.inspected().map_or("", of))
    }

    fn texts(&self, of: impl for<'a> FnOnce(&'a Inspector) -> Vec<&'a str>) -> QStringList {
        string_list(self.inspected().map_or_else(Vec::new, of))
    }

    fn flag(&self, of: impl FnOnce(&Inspector) -> bool) -> bool {
        self.inspected().is_some_and(of)
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.inspected().is_some()
    }

    pub(crate) fn title(&self) -> QString {
        self.text(|i| &i.title)
    }

    pub(crate) fn folder_path(&self) -> QString {
        self.text(|i| &i.folder_path)
    }

    pub(crate) fn status_line(&self) -> QString {
        self.text(|i| &i.status_line)
    }

    /// `""` is the secondary colour.
    pub(crate) fn status_color(&self) -> QString {
        self.text(|i| i.status_color)
    }

    pub(crate) fn buried(&self) -> bool {
        self.flag(|i| i.buried)
    }

    pub(crate) fn reason(&self) -> QString {
        self.text(|i| &i.reason)
    }

    /// `""` hides the rhythm row.
    pub(crate) fn rhythm_line(&self) -> QString {
        self.text(|i| i.rhythm.as_ref().map_or("", |r| &r.line))
    }

    /// −1 hides the rhythm row.
    pub(crate) fn rhythm_index(&self) -> i32 {
        self.inspected()
            .and_then(|i| i.rhythm.as_ref())
            .map_or(-1, |r| to_i32(r.index))
    }

    /// The pickers' items, as of the last sync.
    pub(crate) fn theme_names(&self) -> QStringList {
        string_list(self.rust().theme_names.iter().map(String::as_str))
    }

    /// −1 hides the Theme picker.
    pub(crate) fn theme_index(&self) -> i32 {
        self.inspected().and_then(|i| i.theme).map_or(-1, to_i32)
    }

    pub(crate) fn evidence(&self) -> QStringList {
        self.texts(|i| i.evidence.iter().map(String::as_str).collect())
    }

    pub(crate) fn remarks(&self) -> QStringList {
        self.texts(|i| i.remarks.iter().map(String::as_str).collect())
    }

    pub(crate) fn proposed(&self) -> QString {
        self.text(|i| i.proposed.as_deref().unwrap_or(""))
    }

    pub(crate) fn decision(&self) -> QString {
        self.text(|i| i.decision.as_deref().unwrap_or(""))
    }

    pub(crate) fn feed_url(&self) -> QString {
        self.text(|i| &i.feed_url)
    }

    /// `""`: no web address.
    pub(crate) fn site(&self) -> QString {
        self.text(|i| i.site.as_deref().unwrap_or(""))
    }

    /// `""`: no web address.
    pub(crate) fn wayback(&self) -> QString {
        self.text(|i| i.wayback.as_deref().unwrap_or(""))
    }

    pub(crate) fn keep_title(&self) -> QString {
        self.text(|i| i.keep_title)
    }

    pub(crate) fn can_repair(&self) -> bool {
        self.flag(|i| i.can_repair)
    }

    pub(crate) fn can_bury(&self) -> bool {
        self.flag(|i| i.can_bury)
    }

    pub(crate) fn bury_prominent(&self) -> bool {
        self.flag(|i| i.bury_prominent)
    }

    pub(crate) fn redirects(&self) -> QStringList {
        self.texts(|i| i.redirects.iter().map(String::as_str).collect())
    }

    pub(crate) fn log_dates(&self) -> QStringList {
        self.texts(|i| i.log.iter().map(|l| l.at.as_str()).collect())
    }

    pub(crate) fn log_outcomes(&self) -> QStringList {
        self.texts(|i| i.log.iter().map(|l| l.outcome.as_str()).collect())
    }

    pub(crate) fn antiope_cells(&self) -> i32 {
        self.rust().antiope_cells
    }

    pub(crate) fn set_antiope_cells(mut self: Pin<&mut Self>, cells: i32) {
        if self.rust().antiope_cells == cells {
            return;
        }
        self.as_mut().rust_mut().get_mut().antiope_cells = cells;
        self.inspector_changed();
    }

    pub(crate) fn antiope_inspector(&self) -> QStringList {
        let rows = self.rust().antiope_inspector_rows(Utc::now(), &Local);
        string_list(rows.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use ressac_ui::chrono::Utc;

    use crate::garden::tests::{texts, visiting};

    #[test]
    fn the_antiope_inspector_is_the_selected_feeds_page_at_the_bound_width() {
        let now = Utc::now();
        let mut rust = visiting();
        rust.inspector = rust.visit.inspector(&rust.log, now, &Utc);
        assert!(rust.inspector.is_some());
        // Not bound yet: no width, no rows.
        assert!(rust.antiope_inspector_rows(now, &Utc).is_empty());
        rust.antiope_cells = 30;
        let shown = texts(&rust.antiope_inspector_rows(now, &Utc));
        let title = rust.inspector.as_ref().unwrap().title.to_uppercase();
        assert!(
            shown.iter().any(|row| row.to_uppercase().contains(&title)),
            "{shown:?}"
        );
        assert!(
            shown.iter().all(|row| row.chars().count() == 30),
            "{shown:?}"
        );
    }
}
