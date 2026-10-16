//! `ressac garden`: the full-screen triage UI.

pub mod reader;
pub mod state;
pub mod ui;

use anyhow::Context;
use chrono::Utc;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ressac_core::garden;
use ressac_core::service::Ressac;

use state::{Command, GardenState};

pub fn run(app: &Ressac, ascii: bool) -> anyhow::Result<()> {
    let mut state = GardenState::new(garden::load_rows(app.store())?, ascii);
    let mut terminal =
        ratatui::try_init().context("cannot start the garden (is this a terminal?)")?;
    let result = event_loop(&mut terminal, app, &mut state);
    ratatui::restore();
    result
}

/// Runs one garden command against the database. `Ok(Some(message))` is a line to show,
/// `Ok(None)` nothing; the caller shows an `Err` as the message too. Shared by `ressac garden`
/// and the reader's garden destinations.
pub fn apply_garden_command(app: &Ressac, command: Command) -> anyhow::Result<Option<String>> {
    let store = app.store();
    match command {
        Command::None | Command::Quit => Ok(None),
        Command::Decide(feed_id, decision) => {
            garden::decide(store, feed_id, decision, Utc::now())?;
            Ok(None)
        }
        Command::Undo => Ok(Some(crate::output::undo_text(garden::undo(store)?))),
        Command::ApplySafeRepairs => {
            let (applied, skipped) = garden::apply_safe_repairs(store, Utc::now())?;
            Ok(Some(crate::output::apply_safe_text(applied, &skipped)))
        }
        Command::Open(url) => {
            open::that(&url)?;
            Ok(None)
        }
    }
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &Ressac,
    state: &mut GardenState,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let command = match state.on_key(key) {
            Command::None => continue,
            Command::Quit => return Ok(()),
            command => command,
        };
        match apply_garden_command(app, command) {
            Ok(message) => {
                state.reload(garden::load_rows(app.store())?);
                state.message = message;
            }
            Err(e) => state.message = Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ressac_core::model::Decision;

    #[test]
    fn the_shared_executor_decides_and_undoes() {
        let dir = tempfile::tempdir().unwrap();
        let app = Ressac::open(&dir.path().join("r.sqlite")).unwrap();
        app.import_opml(
            include_str!("../../../../tests/fixtures/vienna-like.opml"),
            false,
        )
        .unwrap();
        let feed = app.store().feeds().unwrap()[0].id;
        assert_eq!(
            apply_garden_command(&app, Command::Decide(feed, Decision::Bury)).unwrap(),
            None
        );
        assert!(
            app.store()
                .feeds()
                .unwrap()
                .iter()
                .any(|f| f.id == feed && f.state == "buried")
        );
        let message = apply_garden_command(&app, Command::Undo).unwrap();
        assert!(message.is_some());
        assert_eq!(apply_garden_command(&app, Command::None).unwrap(), None);
    }
}
