//! Compiling a chart edit into a patch: the op's checks against what the database holds now,
//! the typed names cleaned and checked (spec rule 5), and the changes with their exact
//! before and after. The patch engine (`chart_edit::apply_patch`) never enforces name rules
//! or the one-land and one-continent invariants; the compilers here do, by construction.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};

use super::chart_edit::{
    Change, ChartEdit, LandRef, LandRow, NameKind, SeaRef, SeaRow, checked_name, clean_name,
    derive_key, land_name, read_feed_sea, read_land, read_sea, sea_name,
};
use crate::error::{Error, Result};
use crate::model::FeedId;

/// The label and the changes of `edit`, read against `conn` (the caller's transaction, so
/// what is read is what is written over).
pub(super) fn compile(conn: &Connection, edit: &ChartEdit) -> Result<(String, Vec<Change>)> {
    match edit {
        ChartEdit::NewSea { name } => new_sea(conn, name),
        ChartEdit::RenameSea { sea, name } => rename_sea(conn, sea, name),
        ChartEdit::OrderSeas { seas } => order_seas(conn, seas),
        ChartEdit::DeleteSea { sea } => delete_sea(conn, sea),
        ChartEdit::SetSea { feed, from, to } => set_sea(conn, *feed, from.as_ref(), to.as_ref()),
        ChartEdit::Join { feed, land } => join(conn, *feed, land),
        ChartEdit::Leave { feed, land } => leave(conn, *feed, land),
        ChartEdit::Found { name, feeds } => found(conn, name, feeds),
        ChartEdit::RenameLand { land, name } => rename_land(conn, land, name),
        ChartEdit::DeleteLand { land } => delete_land(conn, land),
        ChartEdit::MakeContinent { sea, name } => make_continent(conn, sea, name),
        ChartEdit::Patch { label, changes } => Ok((label.clone(), changes.clone())),
    }
}

/// The land `land` names (a territory or a continent), or `ChartChanged` when it is gone or
/// renamed; and whether it is a plain territory.
fn checked_land(conn: &Connection, land: &LandRef) -> Result<(LandRow, bool)> {
    match read_land(conn, land.id)? {
        Some((row, plain)) if row.name == land.name => Ok((row, plain)),
        _ => Err(Error::ChartChanged),
    }
}

/// The name of the continent `feed`'s sea has, if any: the feed is one of its islands.
fn continent_of(conn: &Connection, feed: FeedId) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT t.name FROM territory t JOIN feed f ON f.theme_id = t.theme_id \
             WHERE f.id = ?1 ORDER BY t.id LIMIT 1",
            [feed],
            |r| r.get(0),
        )
        .optional()?)
}

/// The member rows of `land` as (feed, position), in position order.
fn members_of(conn: &Connection, land: i64) -> Result<Vec<(FeedId, i64)>> {
    Ok(conn
        .prepare(
            "SELECT feed_id, position FROM territory_feed WHERE territory_id = ?1 \
             ORDER BY position, feed_id",
        )?
        .query_map([land], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

/// The lands `feed` has a member row in.
fn lands_of(conn: &Connection, feed: FeedId) -> Result<Vec<i64>> {
    Ok(conn
        .prepare("SELECT territory_id FROM territory_feed WHERE feed_id = ?1 ORDER BY 1")?
        .query_map([feed], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}

/// The changes that take each (feed, land) pair out of its land. A land left with fewer than
/// two member rows (buried feeds count) is dissolved: its other member rows go too, then the
/// land itself (rule 6).
fn release(conn: &Connection, pairs: &[(FeedId, i64)]) -> Result<Vec<Change>> {
    let mut lands: BTreeMap<i64, Vec<FeedId>> = BTreeMap::new();
    for (feed, land) in pairs {
        lands.entry(*land).or_default().push(*feed);
    }
    let mut changes = Vec::new();
    for (land, leaving) in lands {
        let (row, _) = read_land(conn, land)?.ok_or(Error::ChartChanged)?;
        let members = members_of(conn, land)?;
        let staying = members.iter().filter(|(f, _)| !leaving.contains(f)).count();
        let dissolve = staying < 2;
        for (feed, position) in &members {
            if dissolve || leaving.contains(feed) {
                changes.push(Change::Member {
                    feed: *feed,
                    land,
                    before: Some(*position),
                    after: None,
                });
            }
        }
        if dissolve {
            changes.push(Change::Land {
                before: Some(row),
                after: None,
            });
        }
    }
    Ok(changes)
}

/// The id a new land takes.
fn next_land_id(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT coalesce(max(id), 0) + 1 FROM territory", [], |r| {
            r.get(0)
        })?,
    )
}

/// Refuses a feed that is an island of a continent (rule 2), or one that is gone.
fn ensure_free_island(conn: &Connection, feed: FeedId) -> Result<()> {
    read_feed_sea(conn, feed)?.ok_or(Error::ChartChanged)?;
    match continent_of(conn, feed)? {
        Some(name) => Err(Error::PartOfContinent { name }),
        None => Ok(()),
    }
}

fn join(conn: &Connection, feed: FeedId, land: &LandRef) -> Result<(String, Vec<Change>)> {
    read_feed_sea(conn, feed)?.ok_or(Error::ChartChanged)?;
    let (row, plain) = checked_land(conn, land)?;
    if !plain {
        return Err(Error::ChartChanged);
    }
    ensure_free_island(conn, feed)?;
    let members = members_of(conn, row.id)?;
    if members.iter().any(|(f, _)| *f == feed) {
        return Err(Error::ChartChanged);
    }
    let position = members.iter().map(|(_, p)| *p).max().map_or(0, |p| p + 1);
    // One land per feed: it leaves every territory it was in.
    let old: Vec<_> = lands_of(conn, feed)?
        .into_iter()
        .map(|l| (feed, l))
        .collect();
    let mut changes = release(conn, &old)?;
    changes.push(Change::Member {
        feed,
        land: row.id,
        before: None,
        after: Some(position),
    });
    Ok((format!("Join {}", row.name), changes))
}

fn leave(conn: &Connection, feed: FeedId, land: &LandRef) -> Result<(String, Vec<Change>)> {
    read_feed_sea(conn, feed)?.ok_or(Error::ChartChanged)?;
    let (row, plain) = checked_land(conn, land)?;
    if !plain {
        return Err(Error::ChartChanged);
    }
    ensure_free_island(conn, feed)?;
    if !members_of(conn, row.id)?.iter().any(|(f, _)| *f == feed) {
        return Err(Error::ChartChanged);
    }
    let changes = release(conn, &[(feed, row.id)])?;
    Ok((format!("Leave {}", row.name), changes))
}

fn found(conn: &Connection, name: &str, feeds: &[FeedId]) -> Result<(String, Vec<Change>)> {
    clean_name(name)?;
    if feeds.len() < 2 {
        return Err(Error::NewLandNeedsTwo);
    }
    for (i, feed) in feeds.iter().enumerate() {
        if feeds[..i].contains(feed) {
            return Err(Error::ChartChanged);
        }
        ensure_free_island(conn, *feed)?;
    }
    let mut old = Vec::new();
    for feed in feeds {
        for land in lands_of(conn, *feed)? {
            old.push((*feed, land));
        }
    }
    let mut changes = release(conn, &old)?;
    // The name may be the exact name of a land this found dissolves (deletions run first).
    let dissolved: Vec<i64> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Land {
                before: Some(row),
                after: None,
            } => Some(row.id),
            _ => None,
        })
        .collect();
    let others = conn
        .prepare("SELECT id, name FROM territory")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let name = checked_name(
        name,
        NameKind::Land,
        others
            .iter()
            .filter(|(id, _)| !dissolved.contains(id))
            .map(|(_, n)| n.as_str()),
    )?;
    let id = next_land_id(conn)?;
    changes.push(Change::Land {
        before: None,
        after: Some(LandRow {
            id,
            name: name.clone(),
            continent: None,
        }),
    });
    for (position, feed) in (0_i64..).zip(feeds) {
        changes.push(Change::Member {
            feed: *feed,
            land: id,
            before: None,
            after: Some(position),
        });
    }
    Ok((format!("New Land {name}"), changes))
}

fn rename_land(conn: &Connection, land: &LandRef, name: &str) -> Result<(String, Vec<Change>)> {
    let (before, _) = checked_land(conn, land)?;
    let name = land_name(conn, name, Some(before.id))?;
    let after = LandRow {
        name: name.clone(),
        ..before.clone()
    };
    Ok((
        format!("Rename {name}"),
        vec![Change::Land {
            before: Some(before),
            after: Some(after),
        }],
    ))
}

fn delete_land(conn: &Connection, land: &LandRef) -> Result<(String, Vec<Change>)> {
    let (row, _) = checked_land(conn, land)?;
    let mut changes = Vec::new();
    for (feed, position) in members_of(conn, row.id)? {
        changes.push(Change::Member {
            feed,
            land: row.id,
            before: Some(position),
            after: None,
        });
    }
    let label = format!("Delete {}", row.name);
    changes.push(Change::Land {
        before: Some(row),
        after: None,
    });
    Ok((label, changes))
}

fn make_continent(conn: &Connection, sea: &SeaRef, name: &str) -> Result<(String, Vec<Change>)> {
    let sea = checked_sea(conn, sea)?;
    let has: i64 = conn.query_row(
        "SELECT count(*) FROM territory WHERE theme_id = ?1",
        [sea.id],
        |r| r.get(0),
    )?;
    if has > 0 {
        return Err(Error::ChartChanged);
    }
    let name = land_name(conn, name, None)?;
    let id = next_land_id(conn)?;
    // A continental island never stays in a plain territory (rules 1 and 2): every feed of
    // the sea, buried ones too, leaves its territories, and a land left under two is deleted.
    let feeds: Vec<FeedId> = conn
        .prepare("SELECT id FROM feed WHERE theme_id = ?1 ORDER BY id")?
        .query_map([sea.id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut old = Vec::new();
    for feed in feeds {
        for land in lands_of(conn, feed)? {
            old.push((feed, land));
        }
    }
    let mut changes = release(conn, &old)?;
    changes.push(Change::Land {
        before: None,
        after: Some(LandRow {
            id,
            name: name.clone(),
            continent: Some(sea.id),
        }),
    });
    Ok((format!("Make Continent {name}"), changes))
}

/// The `theme` row `sea` names, or `ChartChanged` when it is gone or differs in any field.
fn checked_sea(conn: &Connection, sea: &SeaRef) -> Result<SeaRow> {
    match read_sea(conn, sea.id)? {
        Some(row) if row.key == sea.key && row.sea == sea.sea => Ok(row),
        _ => Err(Error::ChartChanged),
    }
}

fn new_sea(conn: &Connection, name: &str) -> Result<(String, Vec<Change>)> {
    let name = sea_name(conn, name, None)?;
    let taken_keys = conn
        .prepare("SELECT key FROM theme")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
    let key = derive_key(&name, |k| taken_keys.contains(k));
    let id: i64 = conn.query_row("SELECT coalesce(max(id), 0) + 1 FROM theme", [], |r| {
        r.get(0)
    })?;
    let position: i64 = conn.query_row(
        "SELECT coalesce(max(position) + 1, 0) FROM theme",
        [],
        |r| r.get(0),
    )?;
    let row = SeaRow {
        id,
        key,
        name: name.clone(),
        sea: name.clone(),
        position,
    };
    Ok((
        format!("New Sea {name}"),
        vec![Change::Sea {
            before: None,
            after: Some(row),
        }],
    ))
}

fn rename_sea(conn: &Connection, sea: &SeaRef, name: &str) -> Result<(String, Vec<Change>)> {
    let before = checked_sea(conn, sea)?;
    let name = sea_name(conn, name, Some(before.id))?;
    let after = SeaRow {
        sea: name.clone(),
        ..before.clone()
    };
    Ok((
        format!("Rename {name}"),
        vec![Change::Sea {
            before: Some(before),
            after: Some(after),
        }],
    ))
}

fn order_seas(conn: &Connection, seas: &[i64]) -> Result<(String, Vec<Change>)> {
    let mut stored = conn
        .prepare("SELECT id FROM theme")?
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut given = seas.to_vec();
    stored.sort_unstable();
    given.sort_unstable();
    if stored != given {
        return Err(Error::ChartChanged);
    }
    let mut changes = Vec::new();
    for (position, id) in (0_i64..).zip(seas) {
        let before = read_sea(conn, *id)?.ok_or(Error::ChartChanged)?;
        if before.position != position {
            let after = SeaRow {
                position,
                ..before.clone()
            };
            changes.push(Change::Sea {
                before: Some(before),
                after: Some(after),
            });
        }
    }
    Ok(("Reorder Seas".to_owned(), changes))
}

fn delete_sea(conn: &Connection, sea: &SeaRef) -> Result<(String, Vec<Change>)> {
    let row = checked_sea(conn, sea)?;
    let feeds: Vec<FeedId> = conn
        .prepare("SELECT id FROM feed WHERE theme_id = ?1 ORDER BY id")?
        .query_map([row.id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let continents: Vec<i64> = conn
        .prepare("SELECT id FROM territory WHERE theme_id = ?1 ORDER BY id")?
        .query_map([row.id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut changes = Vec::new();
    if !feeds.is_empty() {
        changes.push(Change::FeedSeas {
            feeds,
            before: Some(row.id),
            after: None,
        });
    }
    for id in continents {
        let (land, _) = read_land(conn, id)?.ok_or(Error::ChartChanged)?;
        changes.push(Change::Land {
            before: Some(land),
            after: None,
        });
    }
    let label = format!("Delete {}", row.sea);
    changes.push(Change::Sea {
        before: Some(row),
        after: None,
    });
    Ok((label, changes))
}

fn set_sea(
    conn: &Connection,
    feed: FeedId,
    from: Option<&SeaRef>,
    to: Option<&SeaRef>,
) -> Result<(String, Vec<Change>)> {
    // `Some(current)`: the feed exists; `current` is its theme id, `None` when unthemed.
    let current = read_feed_sea(conn, feed)?.ok_or(Error::ChartChanged)?;
    let from_id = match from {
        Some(sea) => Some(checked_sea(conn, sea)?.id),
        None => None,
    };
    if current != from_id {
        return Err(Error::ChartChanged);
    }
    let (to_id, label) = match to {
        Some(sea) => {
            let row = checked_sea(conn, sea)?;
            (Some(row.id), format!("Move to {}", row.sea))
        }
        None => (None, "Move to UNCHARTED WATERS".to_owned()),
    };
    if from_id == to_id {
        return Err(Error::ChartChanged);
    }
    let mut changes = vec![Change::FeedSeas {
        feeds: vec![feed],
        before: from_id,
        after: to_id,
    }];
    // Continents follow the sea (rule 2): an island of a continent has no territory, so a
    // feed moved into a sea that has one leaves its plain territories.
    if let Some(to_id) = to_id {
        let continental: i64 = conn.query_row(
            "SELECT count(*) FROM territory WHERE theme_id = ?1",
            [to_id],
            |r| r.get(0),
        )?;
        if continental > 0 {
            let old: Vec<_> = lands_of(conn, feed)?
                .into_iter()
                .map(|l| (feed, l))
                .collect();
            changes.extend(release(conn, &old)?);
        }
    }
    Ok((label, changes))
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::store::Store;
    use crate::store::library::tests::add_feed;

    const CHANGED: &str = "The chart changed; nothing was done.";

    fn sea_ref(s: &Store, key: &str) -> SeaRef {
        s.conn()
            .query_row(
                "SELECT id, key, sea_name FROM theme WHERE key = ?1",
                [key],
                |r| {
                    Ok(SeaRef {
                        id: r.get(0)?,
                        key: r.get(1)?,
                        sea: r.get(2)?,
                    })
                },
            )
            .unwrap()
    }

    fn theme_row(s: &Store, key: &str) -> SeaRow {
        let r = sea_ref(s, key);
        read_sea(s.conn(), r.id).unwrap().unwrap()
    }

    fn insert_sea(s: &Store, id: i64, key: &str, name: &str, sea: &str, position: i64) {
        s.conn()
            .execute(
                "INSERT INTO theme (id, key, name, sea_name, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, key, name, sea, position],
            )
            .unwrap();
    }

    fn set_theme(s: &Store, feed: FeedId, theme: Option<i64>) {
        s.conn()
            .execute(
                "UPDATE feed SET theme_id = ?1 WHERE id = ?2",
                params![theme, feed],
            )
            .unwrap();
    }

    /// Seas `zeta` (1), `alpha` (2) and `empty` (3) in that order; feeds 1 to 6: 1 in zeta,
    /// 2 3 4 in alpha (4 buried), 5 and 6 unthemed; territory `PAIR` (feeds 2 and 3) and
    /// `LONE` (feed 5), and the continent `ALL ALPHA` of alpha.
    fn charted_store() -> Store {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=6 {
            let state = if id == 4 { "buried" } else { "active" };
            add_feed(&s, id, None, id, None, state, "daily");
        }
        insert_sea(&s, 1, "zeta", "Zeta waters", "SEA OF ZETA", 0);
        insert_sea(&s, 2, "alpha", "Alpha waters", "SEA OF ALPHA", 1);
        insert_sea(&s, 3, "empty", "Empty waters", "SEA OF NOTHING", 2);
        set_theme(&s, 1, Some(1));
        for f in [2, 3, 4] {
            set_theme(&s, f, Some(2));
        }
        for (id, name, members) in [(10, "PAIR", vec![2, 3]), (11, "LONE", vec![5])] {
            s.conn()
                .execute(
                    "INSERT INTO territory (id, name, kind) VALUES (?1, ?2, 'territory')",
                    params![id, name],
                )
                .unwrap();
            for (pos, f) in members.iter().enumerate() {
                s.conn()
                    .execute(
                        "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, ?3)",
                        params![id, f, pos as i64],
                    )
                    .unwrap();
            }
        }
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind, theme_id) VALUES (12, 'ALL ALPHA', 'continent', 2)",
                [],
            )
            .unwrap();
        s
    }

    /// Every table the chart edits touch, as sorted rows.
    fn rows(s: &Store) -> Vec<String> {
        let mut out = Vec::new();
        for sql in [
            "SELECT 'theme ' || id || ' ' || key || ' ' || name || ' ' || sea_name || ' ' || position FROM theme ORDER BY id",
            "SELECT 'land ' || id || ' ' || name || ' ' || kind || ' ' || coalesce(theme_id, '-') FROM territory ORDER BY id",
            "SELECT 'member ' || territory_id || ' ' || feed_id || ' ' || position FROM territory_feed ORDER BY 1",
            "SELECT 'feed ' || id || ' ' || coalesce(theme_id, '-') FROM feed ORDER BY id",
        ] {
            let mut stmt = s.conn().prepare(sql).unwrap();
            out.extend(
                stmt.query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .map(|r| r.unwrap()),
            );
        }
        out
    }

    fn keys(s: &Store) -> Vec<String> {
        s.themes().unwrap().into_iter().map(|t| t.key).collect()
    }

    /// `edit` applied, refused with `expected`, nothing written.
    fn assert_refused(s: &Store, edit: &ChartEdit, expected: &str) {
        let before = rows(s);
        let err = s.apply_chart_edit(edit).unwrap_err();
        assert_eq!(err.to_string(), expected, "{err:?}");
        assert_eq!(rows(s), before);
    }

    #[test]
    fn new_sea_takes_a_derived_key_at_the_end() {
        let s = charted_store();
        insert_sea(&s, 4, "comics", "Comics", "COMICS", 3);
        let done = s
            .apply_chart_edit(&ChartEdit::NewSea {
                name: "  Comics!  ".into(),
            })
            .unwrap();
        assert_eq!(done.label, "New Sea Comics!");
        let row = theme_row(&s, "comics-2");
        assert_eq!(
            (row.name.as_str(), row.sea.as_str()),
            ("Comics!", "Comics!")
        );
        assert_eq!(row.position, 4, "last position");
        assert_eq!(keys(&s).last().unwrap(), "comics-2");
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(keys(&s), ["zeta", "alpha", "empty", "comics"]);
    }

    #[test]
    fn new_sea_checks_its_name_and_derives_a_usable_key() {
        let s = charted_store();
        for (raw, err) in [
            ("   ", "A name has 1 to 40 characters."),
            (&"x".repeat(41), "A name has 1 to 40 characters."),
            ("sea of alpha", "A sea with that name already exists."),
            ("ALPHA WATERS", "A sea with that name already exists."),
        ] {
            assert_refused(&s, &ChartEdit::NewSea { name: raw.into() }, err);
        }
        // The key is derived by the spec's rule, not typed: empty, `none` and a leading `-`
        // get numbers; an empty derivation becomes `sea`.
        for (raw, key) in [("None", "none-2"), ("???", "sea"), ("Zeta", "zeta-2")] {
            let done = s
                .apply_chart_edit(&ChartEdit::NewSea { name: raw.into() })
                .unwrap();
            assert!(done.label.starts_with("New Sea "));
            assert!(keys(&s).contains(&key.to_owned()), "{raw} -> {key}");
        }
        let again = s
            .apply_chart_edit(&ChartEdit::NewSea { name: "!!".into() })
            .unwrap();
        assert!(keys(&s).contains(&"sea-2".to_owned()), "{again:?}");
    }

    #[test]
    fn rename_sea_changes_the_sea_name_only() {
        let s = charted_store();
        let alpha = sea_ref(&s, "alpha");
        let done = s
            .apply_chart_edit(&ChartEdit::RenameSea {
                sea: alpha.clone(),
                name: " Alpha Shores ".into(),
            })
            .unwrap();
        assert_eq!(done.label, "Rename Alpha Shores");
        let row = theme_row(&s, "alpha");
        assert_eq!(row.sea, "Alpha Shores");
        assert_eq!(
            (row.name.as_str(), row.id, row.position),
            ("Alpha waters", 2, 1)
        );
        // A case variant of its own name is allowed; another sea's name or sea name is not.
        s.apply_chart_edit(&ChartEdit::RenameSea {
            sea: sea_ref(&s, "alpha"),
            name: "ALPHA SHORES".into(),
        })
        .unwrap();
        for (name, err) in [
            ("sea of zeta", "A sea with that name already exists."),
            ("zeta WATERS", "A sea with that name already exists."),
            ("", "A name has 1 to 40 characters."),
        ] {
            assert_refused(
                &s,
                &ChartEdit::RenameSea {
                    sea: sea_ref(&s, "alpha"),
                    name: name.into(),
                },
                err,
            );
        }
        // The page's undo is stale: the sea was renamed again.
        assert_refused(&s, &done.undo, CHANGED);
        // A stale ref (another sea name) is refused.
        assert_refused(
            &s,
            &ChartEdit::RenameSea {
                sea: alpha,
                name: "Whatever".into(),
            },
            CHANGED,
        );
    }

    #[test]
    fn rename_undo_restores_the_old_sea_name() {
        let s = charted_store();
        let done = s
            .apply_chart_edit(&ChartEdit::RenameSea {
                sea: sea_ref(&s, "zeta"),
                name: "Zed".into(),
            })
            .unwrap();
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(theme_row(&s, "zeta").sea, "SEA OF ZETA");
    }

    #[test]
    fn order_seas_needs_every_id() {
        let s = charted_store();
        let done = s
            .apply_chart_edit(&ChartEdit::OrderSeas {
                seas: vec![3, 1, 2],
            })
            .unwrap();
        assert_eq!(done.label, "Reorder Seas");
        assert_eq!(keys(&s), ["empty", "zeta", "alpha"]);
        for seas in [vec![1, 2], vec![1, 2, 3, 4], vec![1, 1, 2], vec![1, 2, 9]] {
            assert_refused(&s, &ChartEdit::OrderSeas { seas }, CHANGED);
        }
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(keys(&s), ["zeta", "alpha", "empty"]);
    }

    #[test]
    fn deleting_a_sea_removes_its_continent_and_undo_restores_all() {
        let s = charted_store();
        let before = s.export_chart().unwrap().chart;
        let rows_before = rows(&s);
        let done = s
            .apply_chart_edit(&ChartEdit::DeleteSea {
                sea: sea_ref(&s, "alpha"),
            })
            .unwrap();
        assert_eq!(done.label, "Delete SEA OF ALPHA");
        let after = s.export_chart().unwrap().chart;
        let mut expected = before.clone();
        expected.themes.retain(|t| t.key != "alpha");
        expected.feeds.retain(|_, theme| theme != "alpha");
        expected
            .territories
            .retain(|t| t.continent.as_deref() != Some("alpha"));
        assert_eq!(after, expected, "only alpha and its continent are gone");
        assert_eq!(
            after.territories.len(),
            2,
            "PAIR (alpha feeds) and LONE stay as they were"
        );
        assert_eq!(
            s.conn()
                .query_row(
                    "SELECT count(*) FROM feed WHERE theme_id IS NULL",
                    [],
                    |r| { r.get::<_, i64>(0) }
                )
                .unwrap(),
            5,
            "the buried feed is unthemed too"
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(s.export_chart().unwrap().chart, before);
        assert_eq!(
            rows(&s),
            rows_before,
            "ids, positions and members come back"
        );
        let continent: i64 = s
            .conn()
            .query_row(
                "SELECT id FROM territory WHERE name = 'ALL ALPHA'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(continent, 12);
    }

    #[test]
    fn deleting_a_sea_checks_the_ref() {
        let s = charted_store();
        let mut stale = sea_ref(&s, "alpha");
        stale.sea = "SEA OF OLD".into();
        assert_refused(&s, &ChartEdit::DeleteSea { sea: stale }, CHANGED);
        let empty = sea_ref(&s, "empty");
        let done = s
            .apply_chart_edit(&ChartEdit::DeleteSea { sea: empty })
            .unwrap();
        assert_eq!(done.label, "Delete SEA OF NOTHING");
        assert_eq!(keys(&s), ["zeta", "alpha"]);
    }

    #[test]
    fn a_large_sea_undo_fits_the_message_cap() {
        let s = Store::open_in_memory().unwrap();
        insert_sea(&s, 1, "big", "Big", "BIG", 0);
        for id in 1..=2000 {
            add_feed(&s, id, None, id, None, "active", "daily");
        }
        s.conn()
            .execute("UPDATE feed SET theme_id = 1", [])
            .unwrap();
        let done = s
            .apply_chart_edit(&ChartEdit::DeleteSea {
                sea: sea_ref(&s, "big"),
            })
            .unwrap();
        let len = serde_json::to_string(&done.undo).unwrap().len();
        assert!(len < crate::store::MAX_EDIT, "{len} bytes");
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(
            s.feed_theme_keys().unwrap().len(),
            2000,
            "all 2000 are back in the sea"
        );
    }

    #[test]
    fn set_sea_checks_from_and_keeps_lands() {
        let s = charted_store();
        let zeta = sea_ref(&s, "zeta");
        let alpha = sea_ref(&s, "alpha");
        let move_edit = |feed, from: Option<&SeaRef>, to: Option<&SeaRef>| ChartEdit::SetSea {
            feed,
            from: from.cloned(),
            to: to.cloned(),
        };
        // `from` does not match the feed's theme: wrong sea, none while themed, themed while none.
        assert_refused(&s, &move_edit(2, Some(&zeta), Some(&alpha)), CHANGED);
        assert_refused(&s, &move_edit(2, None, Some(&zeta)), CHANGED);
        assert_refused(&s, &move_edit(5, Some(&alpha), Some(&zeta)), CHANGED);
        // `from` equal to `to`, an unknown feed, a gone or renamed `to`.
        assert_refused(&s, &move_edit(2, Some(&alpha), Some(&alpha)), CHANGED);
        assert_refused(&s, &move_edit(5, None, None), CHANGED);
        assert_refused(&s, &move_edit(99, None, Some(&zeta)), CHANGED);
        let mut gone = zeta.clone();
        gone.id = 77;
        assert_refused(&s, &move_edit(2, Some(&alpha), Some(&gone)), CHANGED);
        let mut stale_key = zeta.clone();
        stale_key.key = "old".into();
        assert_refused(&s, &move_edit(2, Some(&alpha), Some(&stale_key)), CHANGED);

        let members_before: Vec<_> = rows(&s)
            .into_iter()
            .filter(|r| r.starts_with("member") || r.starts_with("land"))
            .collect();
        let done = s
            .apply_chart_edit(&move_edit(2, Some(&alpha), Some(&zeta)))
            .unwrap();
        assert_eq!(done.label, "Move to SEA OF ZETA");
        assert_eq!(s.feed_theme_keys().unwrap()[&2], "zeta");
        let to_none = s
            .apply_chart_edit(&move_edit(2, Some(&zeta), None))
            .unwrap();
        assert_eq!(to_none.label, "Move to UNCHARTED WATERS");
        assert!(!s.feed_theme_keys().unwrap().contains_key(&2));
        let members_after: Vec<_> = rows(&s)
            .into_iter()
            .filter(|r| r.starts_with("member") || r.starts_with("land"))
            .collect();
        assert_eq!(
            members_after, members_before,
            "the territory keeps its feed"
        );
        s.apply_chart_edit(&to_none.undo).unwrap();
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(s.feed_theme_keys().unwrap()[&2], "alpha");
    }

    // ---- land edits ------------------------------------------------------------------------

    const PART_OF_ALPHA: &str = "Part of the continent ALL ALPHA: move it to another sea, or turn the continent back into open water.";

    /// Seas `zeta` (1, no continent) and `alpha` (2, continent `ALL ALPHA` 12). Feeds 1 to 6
    /// in zeta (4 buried), 7 and 8 in alpha (islands), 9 unthemed. `PAIR` (10) holds 3, 4 at
    /// 0, 1; `TRIO` (11) holds 1, 2, 6 at 0, 1, 2; feed 5 is in no land.
    fn land_store() -> Store {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=9 {
            let state = if id == 4 { "buried" } else { "active" };
            add_feed(&s, id, None, id, None, state, "daily");
        }
        insert_sea(&s, 1, "zeta", "Zeta waters", "SEA OF ZETA", 0);
        insert_sea(&s, 2, "alpha", "Alpha waters", "SEA OF ALPHA", 1);
        for f in 1..=6 {
            set_theme(&s, f, Some(1));
        }
        for f in [7, 8] {
            set_theme(&s, f, Some(2));
        }
        for (id, name, members) in [(10, "PAIR", vec![3, 4]), (11, "TRIO", vec![1, 2, 6])] {
            s.conn()
                .execute(
                    "INSERT INTO territory (id, name, kind) VALUES (?1, ?2, 'territory')",
                    params![id, name],
                )
                .unwrap();
            for (pos, f) in members.iter().enumerate() {
                s.conn()
                    .execute(
                        "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, ?3)",
                        params![id, f, pos as i64],
                    )
                    .unwrap();
            }
        }
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind, theme_id) VALUES (12, 'ALL ALPHA', 'continent', 2)",
                [],
            )
            .unwrap();
        s
    }

    fn land_ref(s: &Store, name: &str) -> LandRef {
        s.conn()
            .query_row(
                "SELECT id, name FROM territory WHERE name = ?1",
                [name],
                |r| {
                    Ok(LandRef {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .unwrap()
    }

    fn land_rows(s: &Store) -> Vec<String> {
        rows(s)
            .into_iter()
            .filter(|r| r.starts_with("land") || r.starts_with("member"))
            .collect()
    }

    fn join_edit(s: &Store, feed: FeedId, land: &str) -> ChartEdit {
        ChartEdit::Join {
            feed,
            land: land_ref(s, land),
        }
    }

    #[test]
    fn join_moves_a_feed_out_of_its_old_land() {
        let s = land_store();
        let before = rows(&s);
        let done = s.apply_chart_edit(&join_edit(&s, 3, "TRIO")).unwrap();
        assert_eq!(done.label, "Join TRIO");
        let after = land_rows(&s);
        assert!(
            after.contains(&"member 11 3 3".to_owned()),
            "last in TRIO: {after:?}"
        );
        assert!(
            !after
                .iter()
                .any(|r| r.contains("PAIR") || r.starts_with("member 10")),
            "PAIR and both its rows are gone: {after:?}"
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(
            rows(&s),
            before,
            "PAIR is back with its id and both positions"
        );
    }

    #[test]
    fn join_keeps_an_old_land_that_still_has_two_feeds() {
        let s = land_store();
        s.apply_chart_edit(&join_edit(&s, 1, "PAIR")).unwrap();
        let after = land_rows(&s);
        assert!(after.contains(&"member 10 1 2".to_owned()));
        assert!(!after.iter().any(|r| r == "member 11 1 0"));
        assert!(
            after.contains(&"member 11 2 1".to_owned()),
            "TRIO keeps 2 and 6"
        );
        // Refused: the feed is already in it, a gone feed, a renamed land, a continent.
        assert_refused(&s, &join_edit(&s, 1, "PAIR"), CHANGED);
        let trio = land_ref(&s, "TRIO");
        assert_refused(
            &s,
            &ChartEdit::Join {
                feed: 5,
                land: LandRef {
                    name: "OLD".into(),
                    ..trio
                },
            },
            CHANGED,
        );
        assert_refused(
            &s,
            &ChartEdit::Join {
                feed: 99,
                land: land_ref(&s, "PAIR"),
            },
            CHANGED,
        );
        assert_refused(&s, &join_edit(&s, 5, "ALL ALPHA"), CHANGED);
    }

    #[test]
    fn found_takes_a_feed_out_of_every_old_land() {
        // An import may leave a feed in two territories; a join leaves none of them.
        let s = land_store();
        s.conn()
            .execute(
                "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (10, 6, 2)",
                [],
            )
            .unwrap();
        let done = s.apply_chart_edit(&ChartEdit::Found {
            name: "SIX".into(),
            feeds: vec![5, 6],
        });
        done.unwrap();
        let after = land_rows(&s);
        assert!(
            !after
                .iter()
                .any(|r| r.starts_with("member 10 6") || r.starts_with("member 11 6"))
        );
        assert!(after.contains(&"member 13 6 1".to_owned()));
    }

    #[test]
    fn leave_deletes_a_land_left_with_one_feed() {
        let s = land_store();
        let before = rows(&s);
        let done = s
            .apply_chart_edit(&ChartEdit::Leave {
                feed: 1,
                land: land_ref(&s, "TRIO"),
            })
            .unwrap();
        assert_eq!(done.label, "Leave TRIO");
        let after = land_rows(&s);
        assert!(
            after.contains(&"member 11 2 1".to_owned())
                && after.contains(&"member 11 6 2".to_owned())
        );
        assert!(!after.iter().any(|r| r.starts_with("member 11 1")));
        let done2 = s
            .apply_chart_edit(&ChartEdit::Leave {
                feed: 3,
                land: land_ref(&s, "PAIR"),
            })
            .unwrap();
        assert!(
            !land_rows(&s)
                .iter()
                .any(|r| r.contains("PAIR") || r.starts_with("member 10")),
            "buried 4 counted, PAIR deleted"
        );
        s.apply_chart_edit(&done2.undo).unwrap();
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
        // A feed that is not in the land, or a stale land: ChartChanged.
        assert_refused(
            &s,
            &ChartEdit::Leave {
                feed: 5,
                land: land_ref(&s, "TRIO"),
            },
            CHANGED,
        );
        let mut stale = land_ref(&s, "TRIO");
        stale.id = 77;
        assert_refused(
            &s,
            &ChartEdit::Leave {
                feed: 1,
                land: stale,
            },
            CHANGED,
        );
    }

    #[test]
    fn found_keeps_the_given_order_and_empties_old_lands() {
        let s = land_store();
        let before = rows(&s);
        let done = s
            .apply_chart_edit(&ChartEdit::Found {
                name: " NEW ".into(),
                feeds: vec![5, 3, 4],
            })
            .unwrap();
        assert_eq!(done.label, "New Land NEW");
        let after = land_rows(&s);
        for m in ["member 13 5 0", "member 13 3 1", "member 13 4 2"] {
            assert!(after.contains(&m.to_owned()), "{m} in {after:?}");
        }
        assert!(
            !after.iter().any(|r| r.contains("PAIR")),
            "PAIR emptied and deleted"
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
        // TRIO left with one feed is deleted, its last row too; with two it stays.
        s.apply_chart_edit(&ChartEdit::Found {
            name: "AB".into(),
            feeds: vec![1, 2],
        })
        .unwrap();
        assert!(
            !land_rows(&s)
                .iter()
                .any(|r| r.contains("TRIO") || r.starts_with("member 11"))
        );
        let s = land_store();
        s.apply_chart_edit(&ChartEdit::Found {
            name: "AB".into(),
            feeds: vec![1, 5],
        })
        .unwrap();
        assert!(
            land_rows(&s).contains(&"member 11 6 2".to_owned()),
            "TRIO keeps 2 and 6"
        );
        // One feed, a gone feed, a repeated feed.
        assert_refused(
            &s,
            &ChartEdit::Found {
                name: "ONE".into(),
                feeds: vec![5],
            },
            "A new land needs at least two feeds.",
        );
        assert_refused(
            &s,
            &ChartEdit::Found {
                name: "X".into(),
                feeds: vec![5, 99],
            },
            CHANGED,
        );
        assert_refused(
            &s,
            &ChartEdit::Found {
                name: "X".into(),
                feeds: vec![5, 5],
            },
            CHANGED,
        );
    }

    #[test]
    fn found_checks_its_typed_name() {
        let s = land_store();
        for (name, err) in [
            ("  ", "A name has 1 to 40 characters."),
            (&"x".repeat(41), "A name has 1 to 40 characters."),
            ("all alpha", "A land with that name already exists."),
            ("trio", "A land with that name already exists."),
        ] {
            assert_refused(
                &s,
                &ChartEdit::Found {
                    name: name.into(),
                    feeds: vec![5, 9],
                },
                err,
            );
        }
        // The exact name of a land the found dissolves is free again.
        s.apply_chart_edit(&ChartEdit::Found {
            name: "pair".into(),
            feeds: vec![3, 5],
        })
        .unwrap();
        assert!(land_rows(&s).iter().any(|r| r.contains(" pair ")));
    }

    #[test]
    fn continental_feeds_cannot_join_or_leave() {
        let s = land_store();
        for edit in [
            join_edit(&s, 7, "TRIO"),
            ChartEdit::Leave {
                feed: 7,
                land: land_ref(&s, "TRIO"),
            },
            ChartEdit::Found {
                name: "NEW".into(),
                feeds: vec![5, 7],
            },
            ChartEdit::Found {
                name: "NEW".into(),
                feeds: vec![8, 5, 9],
            },
        ] {
            assert_refused(&s, &edit, PART_OF_ALPHA);
        }
    }

    #[test]
    fn make_continent_once_per_sea() {
        let s = land_store();
        let before = rows(&s);
        let zeta = sea_ref(&s, "zeta");
        let done = s
            .apply_chart_edit(&ChartEdit::MakeContinent {
                sea: zeta.clone(),
                name: " Zeta Land ".into(),
            })
            .unwrap();
        assert_eq!(done.label, "Make Continent Zeta Land");
        assert!(land_rows(&s).contains(&"land 13 Zeta Land continent 1".to_owned()));
        assert!(
            !land_rows(&s)
                .iter()
                .any(|r| r.contains("PAIR") || r.contains("TRIO") || r.starts_with("member")),
            "the sea's feeds left their territories, which were dissolved"
        );
        assert_refused(
            &s,
            &ChartEdit::MakeContinent {
                sea: zeta.clone(),
                name: "Second".into(),
            },
            CHANGED,
        );
        assert_refused(
            &s,
            &ChartEdit::MakeContinent {
                sea: sea_ref(&s, "alpha"),
                name: "Second".into(),
            },
            CHANGED,
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before, "undo restores every land and position");
        for (name, err) in [
            ("", "A name has 1 to 40 characters."),
            ("pair", "A land with that name already exists."),
        ] {
            assert_refused(
                &s,
                &ChartEdit::MakeContinent {
                    sea: zeta.clone(),
                    name: name.into(),
                },
                err,
            );
        }
        let mut stale = zeta;
        stale.sea = "OLD".into();
        assert_refused(
            &s,
            &ChartEdit::MakeContinent {
                sea: stale,
                name: "Z".into(),
            },
            CHANGED,
        );
    }

    #[test]
    fn delete_land_handles_both_kinds() {
        let s = land_store();
        let before = rows(&s);
        let done = s
            .apply_chart_edit(&ChartEdit::DeleteLand {
                land: land_ref(&s, "TRIO"),
            })
            .unwrap();
        assert_eq!(done.label, "Delete TRIO");
        assert!(
            !land_rows(&s)
                .iter()
                .any(|r| r.contains("TRIO") || r.starts_with("member 11"))
        );
        assert_eq!(s.export_chart().unwrap().chart.territories.len(), 2);
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
        let done = s
            .apply_chart_edit(&ChartEdit::DeleteLand {
                land: land_ref(&s, "ALL ALPHA"),
            })
            .unwrap();
        assert!(!land_rows(&s).iter().any(|r| r.contains("ALL ALPHA")));
        assert!(
            land_rows(&s).contains(&"member 10 3 0".to_owned()),
            "territories untouched"
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
        let mut stale = land_ref(&s, "PAIR");
        stale.name = "OLD".into();
        assert_refused(&s, &ChartEdit::DeleteLand { land: stale }, CHANGED);
    }

    #[test]
    fn rename_land_to_its_own_name_in_another_case_is_allowed() {
        let s = land_store();
        let done = s
            .apply_chart_edit(&ChartEdit::RenameLand {
                land: land_ref(&s, "TRIO"),
                name: " trio ".into(),
            })
            .unwrap();
        assert_eq!(done.label, "Rename trio");
        assert!(land_rows(&s).contains(&"land 11 trio territory -".to_owned()));
        s.apply_chart_edit(&done.undo).unwrap();
        for (name, err) in [
            ("pair", "A land with that name already exists."),
            ("all ALPHA", "A land with that name already exists."),
            ("", "A name has 1 to 40 characters."),
        ] {
            assert_refused(
                &s,
                &ChartEdit::RenameLand {
                    land: land_ref(&s, "TRIO"),
                    name: name.into(),
                },
                err,
            );
        }
        // A continent renames too, and keeps its sea.
        s.apply_chart_edit(&ChartEdit::RenameLand {
            land: land_ref(&s, "ALL ALPHA"),
            name: "Alphaland".into(),
        })
        .unwrap();
        assert!(land_rows(&s).contains(&"land 12 Alphaland continent 2".to_owned()));
        let stale = LandRef {
            id: 11,
            name: "OLD".into(),
        };
        assert_refused(
            &s,
            &ChartEdit::RenameLand {
                land: stale,
                name: "Z".into(),
            },
            CHANGED,
        );
    }

    #[test]
    fn undo_keeps_an_unrelated_write() {
        let s = land_store();
        let before = rows(&s);
        let done = s.apply_chart_edit(&join_edit(&s, 3, "TRIO")).unwrap();
        s.set_feed_theme(9, Some("zeta")).unwrap();
        add_feed(&s, 20, None, 20, None, "active", "daily");
        s.apply_chart_edit(&done.undo).unwrap();
        let after = rows(&s);
        let mut expected = before;
        expected.push("feed 20 -".into());
        expected.sort();
        expected.retain(|r| r != "feed 9 -");
        expected.push("feed 9 1".into());
        let mut after_sorted = after.clone();
        after_sorted.sort();
        expected.sort();
        assert_eq!(
            after_sorted, expected,
            "membership restored, both writes kept"
        );
    }

    #[test]
    fn undo_after_a_conflicting_write_is_refused() {
        let s = land_store();
        let done = s.apply_chart_edit(&join_edit(&s, 3, "TRIO")).unwrap();
        // Another writer imports a chart that leaves TRIO out.
        let mut file = s.export_chart().unwrap().chart;
        file.territories.retain(|t| t.name != "TRIO");
        s.import_chart(&file, false).unwrap();
        let export = s.export_chart().unwrap().chart;
        let rows_before = rows(&s);
        let err = s.apply_chart_edit(&done.undo).unwrap_err();
        assert_eq!(err.to_string(), CHANGED);
        assert_eq!(s.export_chart().unwrap().chart, export);
        assert_eq!(rows(&s), rows_before);
    }

    #[test]
    fn set_sea_into_a_continent_sea_leaves_the_plain_territories() {
        let s = land_store();
        let zeta = sea_ref(&s, "zeta");
        let alpha = sea_ref(&s, "alpha");
        let before = rows(&s);
        let edit = |feed| ChartEdit::SetSea {
            feed,
            from: Some(zeta.clone()),
            to: Some(alpha.clone()),
        };
        // Feed 3 of PAIR (with buried 4): PAIR is left with one feed and deleted.
        let done = s.apply_chart_edit(&edit(3)).unwrap();
        assert_eq!(done.label, "Move to SEA OF ALPHA");
        assert!(
            !land_rows(&s)
                .iter()
                .any(|r| r.contains("PAIR") || r.starts_with("member 10"))
        );
        assert_eq!(s.feed_theme_keys().unwrap()[&3], "alpha");
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before, "theme, land, id and positions all back");
        // Feed 1 of TRIO: TRIO keeps two feeds.
        let done = s.apply_chart_edit(&edit(1)).unwrap();
        let after = land_rows(&s);
        assert!(
            after.contains(&"member 11 2 1".to_owned())
                && !after.iter().any(|r| r.starts_with("member 11 1"))
        );
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
        // Into open water or a sea with no continent, memberships stay (see the other test).
        let uncharted = ChartEdit::SetSea {
            feed: 3,
            from: Some(zeta),
            to: None,
        };
        s.apply_chart_edit(&uncharted).unwrap();
        assert!(land_rows(&s).contains(&"member 10 3 0".to_owned()));
    }

    #[test]
    fn make_continent_releases_the_seas_feeds_into_islands() {
        let s = land_store();
        // Feed 6 (of TRIO) lives in alpha, so TRIO is left with 1 and 2 when zeta is
        // charted, and then dissolves with them; PAIR (3 and buried 4) dissolves too.
        s.conn()
            .execute("UPDATE feed SET theme_id = 2 WHERE id = 6", [])
            .unwrap();
        // OTHER, a plain land of two unthemed feeds, is not the sea's and must stay.
        add_feed(&s, 10, None, 10, None, "active", "daily");
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind) VALUES (20, 'OTHER', 'territory')",
                [],
            )
            .unwrap();
        for (f, p) in [(9, 0), (10, 1)] {
            s.conn()
                .execute(
                    "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (20, ?1, ?2)",
                    params![f, p],
                )
                .unwrap();
        }
        let before = rows(&s);
        let done = s
            .apply_chart_edit(&ChartEdit::MakeContinent {
                sea: sea_ref(&s, "zeta"),
                name: "Zeta Land".into(),
            })
            .unwrap();
        let after = land_rows(&s);
        let members: Vec<_> = after.iter().filter(|r| r.starts_with("member")).collect();
        assert_eq!(members, ["member 20 10 1", "member 20 9 0"], "{after:?}");
        assert!(after.contains(&"land 21 Zeta Land continent 1".to_owned()));
        let hint = "Part of the continent Zeta Land: move it to another sea, or turn the continent back into open water.";
        // The feeds are continental islands now: join, leave and found name the continent.
        let other = land_ref(&s, "OTHER");
        for edit in [
            ChartEdit::Join {
                feed: 1,
                land: other.clone(),
            },
            ChartEdit::Leave {
                feed: 1,
                land: other,
            },
            ChartEdit::Found {
                name: "NEW".into(),
                feeds: vec![1, 9],
            },
        ] {
            assert_refused(&s, &edit, hint);
        }
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before, "territories, ids and positions all back");
    }

    #[test]
    fn a_land_kept_by_a_buried_feed_survives_a_leave() {
        let s = land_store();
        s.conn()
            .execute("UPDATE feed SET state = 'buried' WHERE id = 6", [])
            .unwrap();
        let before = rows(&s);
        // TRIO = 1, 2 and buried 6: feed 1 leaves, one visible feed and one buried remain.
        let done = s
            .apply_chart_edit(&ChartEdit::Leave {
                feed: 1,
                land: land_ref(&s, "TRIO"),
            })
            .unwrap();
        let after = land_rows(&s);
        assert!(
            after.contains(&"land 11 TRIO territory -".to_owned()),
            "{after:?}"
        );
        assert!(after.contains(&"member 11 2 1".to_owned()));
        assert!(after.contains(&"member 11 6 2".to_owned()));
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(rows(&s), before);
    }

    #[test]
    fn a_patch_compiles_as_it_is() {
        let s = charted_store();
        let row = theme_row(&s, "empty");
        let changes = vec![Change::Sea {
            before: Some(row.clone()),
            after: Some(SeaRow { position: 9, ..row }),
        }];
        let (label, compiled) = compile(
            s.conn(),
            &ChartEdit::Patch {
                label: "Mine".into(),
                changes: changes.clone(),
            },
        )
        .unwrap();
        assert_eq!((label.as_str(), compiled), ("Mine", changes));
    }
}
