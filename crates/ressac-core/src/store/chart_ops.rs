//! Compiling a chart edit into a patch: the op's checks against what the database holds now,
//! the typed names cleaned and checked (spec rule 5), and the changes with their exact
//! before and after. The patch engine (`chart_edit::apply_patch`) never enforces name rules
//! or the one-land and one-continent invariants; the compilers here do, by construction.

use rusqlite::Connection;

use super::chart_edit::{
    Change, ChartEdit, SeaRef, SeaRow, derive_key, read_feed_sea, read_land, read_sea, sea_name,
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
        ChartEdit::Patch { label, changes } => Ok((label.clone(), changes.clone())),
        other => Err(Error::Invalid(format!(
            "{} is not implemented yet",
            other.op()
        ))),
    }
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
    Ok((
        label,
        vec![Change::FeedSeas {
            feeds: vec![feed],
            before: from_id,
            after: to_id,
        }],
    ))
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
        s.apply_chart_edit(&done.undo).unwrap_err(); // the page's undo is stale: renamed again
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
