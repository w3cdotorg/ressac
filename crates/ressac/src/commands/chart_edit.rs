//! `ressac chart land|continent|theme`: each verb builds a `ChartEdit` from the database's
//! current rows (so a command line edit is never stale) and goes through the same path as
//! `ressac chart apply`.

use std::io::Write;
use std::process::ExitCode;

use chrono::Utc;
use ressac_core::model::FeedId;
use ressac_core::store::{ChartEdit, ChartSnapshot, LandRef, LedgerLand, LedgerSea, SeaRef, Store};

use super::chart::commit;
use super::{Globals, usage};
use crate::cli::{ContinentCommand, LandCommand, ThemeCommand};
use crate::output;

/// Opens the database (writable, never created) and the current ledger.
fn current(g: &Globals) -> anyhow::Result<(Store, ChartSnapshot)> {
    let store = Store::open_existing(&g.db)?;
    let snap = store.chart_snapshot(Utc::now())?;
    Ok((store, snap))
}

/// The land `given` names: all digits is first an id, then a name (a land may be called
/// `1984`); anything else a name compared without case. With `territories_only`, a
/// continent is not a match.
fn find_land<'a>(
    snap: &'a ChartSnapshot,
    given: &str,
    territories_only: bool,
) -> anyhow::Result<&'a LedgerLand> {
    let wanted = given.trim();
    let id = (!wanted.is_empty() && wanted.chars().all(|c| c.is_ascii_digit()))
        .then(|| wanted.parse::<i64>().ok())
        .flatten();
    let lowered = wanted.to_lowercase();
    let lands = || {
        snap.ledger
            .lands
            .iter()
            .filter(|l| !territories_only || l.continent.is_none())
    };
    id.and_then(|id| lands().find(|l| l.id == id))
        .or_else(|| lands().find(|l| l.name.trim().to_lowercase() == lowered))
        .ok_or_else(|| usage(output::printable(&format!("No land \"{given}\".")).into_owned()))
}

/// A feed id that names no feed (any state) is said so, not "the chart changed".
fn known_feed(store: &Store, feed: FeedId) -> anyhow::Result<()> {
    no_feed(store.feed(feed).map(|_| ()), feed)
}

/// What a feed lookup's result means here: a missing row is a usage error (exit 2); any
/// other error (I/O, a corrupt file) is the database's own and propagates (exit 1).
fn no_feed(found: ressac_core::Result<()>, feed: FeedId) -> anyhow::Result<()> {
    match found {
        Err(e) if e.is_no_row() => Err(usage(format!("No feed #{feed}."))),
        other => Ok(other?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::UsageError;

    /// T6 Minor: only a missing row is "No feed #n." (exit 2); any other error is the
    /// database's own, propagated (exit 1).
    #[test]
    fn only_a_missing_row_is_no_feed() {
        let missing = no_feed(Err(rusqlite::Error::QueryReturnedNoRows.into()), 7).unwrap_err();
        assert_eq!(
            missing
                .downcast_ref::<UsageError>()
                .map(ToString::to_string),
            Some("No feed #7.".to_owned())
        );
        let io = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
            None,
        );
        let other = no_feed(Err(io.into()), 7).unwrap_err();
        assert!(other.downcast_ref::<UsageError>().is_none(), "{other}");
        assert!(no_feed(Ok(()), 7).is_ok());
    }
}

fn land_ref(land: &LedgerLand) -> LandRef {
    LandRef {
        id: land.id,
        name: land.name.clone(),
    }
}

/// The ledger entry of the theme `given` names (key, else name; see `Store::resolve_theme`).
fn find_sea<'a>(
    store: &Store,
    snap: &'a ChartSnapshot,
    given: &str,
) -> anyhow::Result<&'a LedgerSea> {
    let def = store
        .resolve_theme(given)
        .map_err(super::chart::refusal_error)?;
    snap.ledger
        .seas
        .iter()
        .find(|s| s.key == def.key)
        .ok_or_else(|| usage(output::printable(&format!("No theme \"{given}\".")).into_owned()))
}

fn sea_ref(sea: &LedgerSea) -> SeaRef {
    SeaRef {
        id: sea.id,
        key: sea.key.clone(),
        sea: sea.sea.clone(),
    }
}

pub(super) fn land(
    g: &Globals,
    action: LandCommand,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let (store, snap) = current(g)?;
    let edit = match action {
        LandCommand::New { name, feeds } => {
            for feed in &feeds {
                known_feed(&store, *feed)?;
            }
            ChartEdit::Found { name, feeds }
        }
        LandCommand::Rename { land, name } => ChartEdit::RenameLand {
            land: land_ref(find_land(&snap, &land, false)?),
            name,
        },
        LandCommand::Delete { land } => ChartEdit::DeleteLand {
            land: land_ref(find_land(&snap, &land, false)?),
        },
        LandCommand::Add { land, feed } => {
            known_feed(&store, feed)?;
            ChartEdit::Join {
                feed,
                land: land_ref(find_land(&snap, &land, true)?),
            }
        }
        LandCommand::Remove { land, feed } => {
            known_feed(&store, feed)?;
            ChartEdit::Leave {
                feed,
                land: land_ref(find_land(&snap, &land, true)?),
            }
        }
    };
    commit(g, &store, &edit, out)
}

pub(super) fn continent(
    g: &Globals,
    action: ContinentCommand,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let (store, snap) = current(g)?;
    let edit = match action {
        ContinentCommand::Make { theme, name } => {
            let sea = find_sea(&store, &snap, &theme)?;
            if snap
                .ledger
                .lands
                .iter()
                .any(|l| l.continent.as_deref() == Some(sea.key.as_str()))
            {
                return Err(usage(
                    output::printable(&format!("Theme \"{}\" already has a continent.", sea.key))
                        .into_owned(),
                ));
            }
            ChartEdit::MakeContinent {
                sea: sea_ref(sea),
                name,
            }
        }
        ContinentCommand::Unmake { theme } => {
            let sea = find_sea(&store, &snap, &theme)?;
            let continent = snap
                .ledger
                .lands
                .iter()
                .find(|l| l.continent.as_deref() == Some(sea.key.as_str()))
                .ok_or_else(|| {
                    usage(
                        output::printable(&format!("Theme \"{}\" has no continent.", sea.key))
                            .into_owned(),
                    )
                })?;
            // The page's "Back to Open Water" sends the same op.
            ChartEdit::DeleteLand {
                land: land_ref(continent),
            }
        }
    };
    commit(g, &store, &edit, out)
}

pub(super) fn theme(
    g: &Globals,
    action: ThemeCommand,
    out: &mut impl Write,
) -> anyhow::Result<ExitCode> {
    let (store, snap) = current(g)?;
    let edit = match action {
        ThemeCommand::New { name } => ChartEdit::NewSea { name },
        ThemeCommand::Rename { theme, name } => ChartEdit::RenameSea {
            sea: sea_ref(find_sea(&store, &snap, &theme)?),
            name,
        },
        ThemeCommand::Delete { theme } => ChartEdit::DeleteSea {
            sea: sea_ref(find_sea(&store, &snap, &theme)?),
        },
        ThemeCommand::Move { theme, position } => {
            let sea = find_sea(&store, &snap, &theme)?;
            let n = snap.ledger.seas.len();
            if !(1..=n).contains(&position) {
                return Err(usage(format!(
                    "Position {position} is not between 1 and {n}."
                )));
            }
            let mut seas: Vec<i64> = snap
                .ledger
                .seas
                .iter()
                .map(|s| s.id)
                .filter(|id| *id != sea.id)
                .collect();
            seas.insert(position - 1, sea.id);
            ChartEdit::OrderSeas { seas }
        }
    };
    commit(g, &store, &edit, out)
}
