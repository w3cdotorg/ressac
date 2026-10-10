//! `ressac garden`: the full-screen triage UI.

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
        let store = app.store();
        let outcome: anyhow::Result<Option<String>> = match state.on_key(key) {
            Command::None => continue,
            Command::Quit => return Ok(()),
            Command::Decide(feed_id, decision) => {
                garden::decide(store, feed_id, decision, Utc::now())
                    .map(|()| None)
                    .map_err(Into::into)
            }
            Command::Undo => garden::undo(store)
                .map(|undone| {
                    Some(undone.map_or("Nothing to undo.".into(), |id| {
                        format!("Undid the last decision (#{id}).")
                    }))
                })
                .map_err(Into::into),
            Command::ApplySafeRepairs => garden::apply_safe_repairs(store, Utc::now())
                .map(|(applied, skipped)| {
                    let mut msg = format!("Applied {applied} safe repairs.");
                    if !skipped.is_empty() {
                        msg.push_str(&format!(
                            " Skipped {}: {}",
                            skipped.len(),
                            skipped.join("; ")
                        ));
                    }
                    Some(msg)
                })
                .map_err(Into::into),
            Command::Open(url) => open::that(&url).map(|()| None).map_err(Into::into),
        };
        match outcome {
            Ok(message) => {
                state.reload(garden::load_rows(store)?);
                state.message = message;
            }
            Err(e) => state.message = Some(e.to_string()),
        }
    }
}
