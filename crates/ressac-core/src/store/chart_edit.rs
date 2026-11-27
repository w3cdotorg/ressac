//! Chart edits: the vocabulary the chart page, both apps and `ressac chart apply` speak, and
//! the patch engine that applies them. Every edit becomes a patch (changes with their exact
//! before and after), checked against the database and applied in one transaction; its
//! inverse is the undo (spec rules 3 and 4). Names and keys follow spec rule 5.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::chart::check_theme_key;
use super::{Store, chart_ops};
use crate::error::{Error, Result};
use crate::model::FeedId;

/// The most bytes of edit JSON any path accepts (page, both bridges, CLI).
pub const MAX_EDIT: usize = 64 * 1024;

/// The most characters (Unicode scalar values) a land or sea name has once trimmed.
const MAX_NAME_CHARS: usize = 40;

/// A land as the page saw it; the core checks every field (rule 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRef {
    pub id: i64,
    pub name: String,
}

/// A sea (a theme) as the page saw it; the core checks every field (rule 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeaRef {
    pub id: i64,
    pub key: String,
    pub sea: String,
}

/// A `theme` row, whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeaRow {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub sea: String,
    pub position: i64,
}

/// A `territory` row, whole: with `continent` (a theme id) it is that theme's continent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRow {
    pub id: i64,
    pub name: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub continent: Option<i64>,
}

/// One change of a patch: a row (or a feed's membership, or several feeds' theme) before and
/// after. `None` before means the row must not exist; `None` after deletes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Sea {
        #[serde(deserialize_with = "Option::deserialize")]
        before: Option<SeaRow>,
        #[serde(deserialize_with = "Option::deserialize")]
        after: Option<SeaRow>,
    },
    Land {
        #[serde(deserialize_with = "Option::deserialize")]
        before: Option<LandRow>,
        #[serde(deserialize_with = "Option::deserialize")]
        after: Option<LandRow>,
    },
    /// `feed`'s position in territory `land`; `None` is no membership.
    Member {
        feed: FeedId,
        land: i64,
        #[serde(deserialize_with = "Option::deserialize")]
        before: Option<i64>,
        #[serde(deserialize_with = "Option::deserialize")]
        after: Option<i64>,
    },
    /// The theme id of each of `feeds`; grouped so a large sea's undo stays small.
    FeedSeas {
        feeds: Vec<FeedId>,
        #[serde(deserialize_with = "Option::deserialize")]
        before: Option<i64>,
        #[serde(deserialize_with = "Option::deserialize")]
        after: Option<i64>,
    },
}

/// An edit of the chart, as the page sends it (the wire table of the plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChartEdit {
    Join {
        feed: FeedId,
        land: LandRef,
    },
    Leave {
        feed: FeedId,
        land: LandRef,
    },
    /// A new land of `feeds`, in chart order.
    Found {
        name: String,
        feeds: Vec<FeedId>,
    },
    RenameLand {
        land: LandRef,
        name: String,
    },
    DeleteLand {
        land: LandRef,
    },
    MakeContinent {
        sea: SeaRef,
        name: String,
    },
    /// `None` is uncharted waters.
    SetSea {
        feed: FeedId,
        #[serde(deserialize_with = "Option::deserialize")]
        from: Option<SeaRef>,
        #[serde(deserialize_with = "Option::deserialize")]
        to: Option<SeaRef>,
    },
    NewSea {
        name: String,
    },
    RenameSea {
        sea: SeaRef,
        name: String,
    },
    /// Every theme id, in the new order.
    OrderSeas {
        seas: Vec<i64>,
    },
    DeleteSea {
        sea: SeaRef,
    },
    Patch {
        label: String,
        changes: Vec<Change>,
    },
}

impl ChartEdit {
    /// The `op` this edit carries on the wire.
    pub fn op(&self) -> &'static str {
        match self {
            ChartEdit::Join { .. } => "join",
            ChartEdit::Leave { .. } => "leave",
            ChartEdit::Found { .. } => "found",
            ChartEdit::RenameLand { .. } => "rename_land",
            ChartEdit::DeleteLand { .. } => "delete_land",
            ChartEdit::MakeContinent { .. } => "make_continent",
            ChartEdit::SetSea { .. } => "set_sea",
            ChartEdit::NewSea { .. } => "new_sea",
            ChartEdit::RenameSea { .. } => "rename_sea",
            ChartEdit::OrderSeas { .. } => "order_seas",
            ChartEdit::DeleteSea { .. } => "delete_sea",
            ChartEdit::Patch { .. } => "patch",
        }
    }
}

/// What an applied edit did: its label (`Join APPLE PRESS`) and the patch that undoes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartEditDone {
    pub label: String,
    /// Always a `ChartEdit::Patch`.
    pub undo: ChartEdit,
}

impl ChartEditDone {
    /// `{"event":"chart_edit","label":…,"undo":…}`: the one shape the CLI prints and both
    /// apps hand to the page.
    pub fn event(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "chart_edit",
            "label": self.label,
            "undo": self.undo,
        })
    }
}

/// The name a land or sea gets from typed text: trimmed (`str::trim`, Unicode `White_Space`),
/// 1 to 40 characters, else `NameLength`.
pub(super) fn clean_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::NameLength);
    }
    Ok(name.to_owned())
}

/// Whether two names are the same name: equal once trimmed and lower-cased.
pub(super) fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// What kind of row a name names: lands and seas each keep their names unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameKind {
    Land,
    Sea,
}

/// `raw` cleaned, refused when it is the same name as any of `others` (the other lands'
/// names, or the other seas' names and sea names).
pub(super) fn checked_name<'a>(
    raw: &str,
    kind: NameKind,
    others: impl IntoIterator<Item = &'a str>,
) -> Result<String> {
    let name = clean_name(raw)?;
    if others.into_iter().any(|other| same_name(other, &name)) {
        return Err(match kind {
            NameKind::Land => Error::LandNameTaken,
            NameKind::Sea => Error::SeaNameTaken,
        });
    }
    Ok(name)
}

/// A new sea's key: `name` lower-cased, runs of non-alphanumerics turned to `-`, trimmed of
/// `-`; empty becomes `sea`; a key that is taken (or `none`) gets a number (`comics-2`).
pub(super) fn derive_key(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut key = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_alphanumeric() {
            key.push(c);
        } else if !key.ends_with('-') {
            key.push('-');
        }
    }
    let key = match key.trim_matches('-') {
        "" => "sea",
        trimmed => trimmed,
    };
    if !taken(key) && check_theme_key(key).is_ok() {
        return key.to_owned();
    }
    (2..)
        .map(|n| format!("{key}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("an unbounded range finds a free key")
}

/// The `theme` row `id`, if any.
pub(super) fn read_sea(conn: &Connection, id: i64) -> Result<Option<SeaRow>> {
    Ok(conn
        .query_row(
            "SELECT id, key, name, sea_name, position FROM theme WHERE id = ?1",
            [id],
            |r| {
                Ok(SeaRow {
                    id: r.get(0)?,
                    key: r.get(1)?,
                    name: r.get(2)?,
                    sea: r.get(3)?,
                    position: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// The `territory` row `id`, if any, and whether it is a plain territory.
pub(super) fn read_land(conn: &Connection, id: i64) -> Result<Option<(LandRow, bool)>> {
    Ok(conn
        .query_row(
            "SELECT id, name, theme_id, kind = 'territory' FROM territory WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    LandRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        continent: r.get(2)?,
                    },
                    r.get(3)?,
                ))
            },
        )
        .optional()?)
}

/// `feed`'s position in territory `land`, if it is a member.
fn read_member(conn: &Connection, feed: FeedId, land: i64) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT position FROM territory_feed WHERE territory_id = ?1 AND feed_id = ?2",
            params![land, feed],
            |r| r.get(0),
        )
        .optional()?)
}

/// `feed`'s theme id; `None` outside, `Some(None)` for an unthemed feed.
pub(super) fn read_feed_sea(conn: &Connection, feed: FeedId) -> Result<Option<Option<i64>>> {
    Ok(conn
        .query_row("SELECT theme_id FROM feed WHERE id = ?1", [feed], |r| {
            r.get(0)
        })
        .optional()?)
}

fn exists(conn: &Connection, sql: &str, id: i64) -> Result<bool> {
    Ok(conn.query_row(sql, [id], |r| r.get::<_, i64>(0))? > 0)
}

/// `refusal` for a SQLite constraint failure (a `UNIQUE` the checks could not see coming),
/// any other error as it is.
fn refuse_constraint(e: rusqlite::Error, refusal: Error) -> Error {
    match e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            refusal
        }
        other => Error::Db(other),
    }
}

/// `ChartChanged` unless `holds`.
fn ensure(holds: bool) -> Result<()> {
    if holds {
        Ok(())
    } else {
        Err(Error::ChartChanged)
    }
}

fn malformed(what: &str) -> Error {
    Error::Invalid(format!("The patch is malformed: {what}."))
}

/// The row id a `before`/`after` pair is about: both must name the same one.
fn pair_id(before: Option<i64>, after: Option<i64>, kind: &str) -> Result<i64> {
    match (before, after) {
        (Some(b), Some(a)) if a != b => Err(malformed(&format!("a {kind} change names two ids"))),
        (Some(id), _) | (None, Some(id)) => Ok(id),
        (None, None) => Err(malformed(&format!(
            "a {kind} change has no before and no after"
        ))),
    }
}

/// Every `before` of `changes` checked against the database, and each row named once.
fn check_befores(conn: &Connection, changes: &[Change]) -> Result<()> {
    use std::collections::HashSet;
    let (mut seas, mut lands, mut members, mut feeds) = (
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    );
    for change in changes {
        match change {
            Change::Sea { before, after } => {
                let id = pair_id(
                    before.as_ref().map(|r| r.id),
                    after.as_ref().map(|r| r.id),
                    "sea",
                )?;
                if !seas.insert(id) {
                    return Err(malformed(&format!("sea {id} changes twice")));
                }
                ensure(read_sea(conn, id)? == *before)?;
            }
            Change::Land { before, after } => {
                let id = pair_id(
                    before.as_ref().map(|r| r.id),
                    after.as_ref().map(|r| r.id),
                    "land",
                )?;
                if !lands.insert(id) {
                    return Err(malformed(&format!("land {id} changes twice")));
                }
                ensure(read_land(conn, id)?.map(|(row, _)| row) == *before)?;
            }
            Change::Member {
                feed,
                land,
                before,
                after,
            } => {
                if before.is_none() && after.is_none() {
                    return Err(malformed("a member change has no before and no after"));
                }
                if !members.insert((*feed, *land)) {
                    return Err(malformed(&format!(
                        "feed {feed} in land {land} changes twice"
                    )));
                }
                ensure(read_member(conn, *feed, *land)? == *before)?;
            }
            Change::FeedSeas {
                feeds: list,
                before,
                ..
            } => {
                for feed in list {
                    if !feeds.insert(*feed) {
                        return Err(malformed(&format!("feed {feed} changes sea twice")));
                    }
                    ensure(read_feed_sea(conn, *feed)? == Some(*before))?;
                }
            }
        }
    }
    Ok(())
}

/// Applies `changes` on `conn` (the caller's transaction). Every `before` is checked first,
/// then the rows are written, and any refusal leaves the caller to roll back.
///
/// A patch is checked only against its `before`s and the schema, plus one structural guard
/// the chart relies on (a continent has no members). The name rules and the one-land-per-feed
/// and one-continent-per-sea rules belong to the op compilers, which build edits from typed
/// input. A patch is never refused for restoring what was there (spec rule 3), even a name
/// or a placement that an import allowed and the editor would not.
pub(super) fn apply_patch(conn: &Connection, changes: &[Change]) -> Result<()> {
    check_befores(conn, changes)?;

    // Sea upserts.
    for change in changes {
        let Change::Sea {
            before,
            after: Some(row),
        } = change
        else {
            continue;
        };
        let sql = if before.is_none() {
            "INSERT INTO theme (id, key, name, sea_name, position) VALUES (?1, ?2, ?3, ?4, ?5)"
        } else {
            "UPDATE theme SET key = ?2, name = ?3, sea_name = ?4, position = ?5 WHERE id = ?1"
        };
        conn.execute(
            sql,
            params![row.id, row.key, row.name, row.sea, row.position],
        )
        .map_err(|e| refuse_constraint(e, Error::ChartChanged))?;
    }
    // Feeds' seas.
    for change in changes {
        let Change::FeedSeas { feeds, after, .. } = change else {
            continue;
        };
        if let Some(theme) = after {
            ensure(exists(
                conn,
                "SELECT count(*) FROM theme WHERE id = ?1",
                *theme,
            )?)?;
        }
        for feed in feeds {
            conn.execute(
                "UPDATE feed SET theme_id = ?1 WHERE id = ?2",
                params![after, feed],
            )?;
        }
    }
    // Member removals.
    for change in changes {
        if let Change::Member {
            feed,
            land,
            after: None,
            ..
        } = change
        {
            conn.execute(
                "DELETE FROM territory_feed WHERE territory_id = ?1 AND feed_id = ?2",
                params![land, feed],
            )?;
        }
    }
    // Land deletions, before the upserts so a new land can take a deleted one's exact name.
    // Never through the schema's cascade: every member must have been removed above.
    for change in changes {
        let Change::Land {
            before: Some(row),
            after: None,
        } = change
        else {
            continue;
        };
        ensure(!exists(
            conn,
            "SELECT count(*) FROM territory_feed WHERE territory_id = ?1",
            row.id,
        )?)?;
        conn.execute("DELETE FROM territory WHERE id = ?1", [row.id])?;
    }
    // Land upserts.
    for change in changes {
        let Change::Land {
            before,
            after: Some(row),
        } = change
        else {
            continue;
        };
        if let Some(theme) = row.continent {
            ensure(exists(
                conn,
                "SELECT count(*) FROM theme WHERE id = ?1",
                theme,
            )?)?;
        }
        let kind = if row.continent.is_some() {
            "continent"
        } else {
            "territory"
        };
        let sql = if before.is_none() {
            "INSERT INTO territory (id, name, kind, theme_id) VALUES (?1, ?2, ?3, ?4)"
        } else {
            "UPDATE territory SET name = ?2, kind = ?3, theme_id = ?4 WHERE id = ?1"
        };
        conn.execute(sql, params![row.id, row.name, kind, row.continent])
            .map_err(|e| refuse_constraint(e, Error::LandNameTaken))?;
    }
    // Member inserts and moves: only a plain territory takes members.
    for change in changes {
        let Change::Member {
            feed,
            land,
            before,
            after: Some(position),
        } = change
        else {
            continue;
        };
        ensure(matches!(read_land(conn, *land)?, Some((_, true))))?;
        if before.is_none() {
            conn.execute(
                "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, ?3)",
                params![land, feed, position],
            )
            .map_err(|e| refuse_constraint(e, Error::ChartChanged))?;
        } else {
            conn.execute(
                "UPDATE territory_feed SET position = ?3 WHERE territory_id = ?1 AND feed_id = ?2",
                params![land, feed, position],
            )?;
        }
    }
    // Sea deletions: no feed (buried ones included) and no continent may point at it.
    for change in changes {
        let Change::Sea {
            before: Some(row),
            after: None,
        } = change
        else {
            continue;
        };
        ensure(!exists(
            conn,
            "SELECT count(*) FROM feed WHERE theme_id = ?1",
            row.id,
        )?)?;
        ensure(!exists(
            conn,
            "SELECT count(*) FROM territory WHERE theme_id = ?1",
            row.id,
        )?)?;
        conn.execute("DELETE FROM theme WHERE id = ?1", [row.id])?;
    }
    // The structural guard: a land written as a continent keeps no member rows.
    for change in changes {
        if let Change::Land {
            after:
                Some(LandRow {
                    id,
                    continent: Some(_),
                    ..
                }),
            ..
        } = change
        {
            ensure(!exists(
                conn,
                "SELECT count(*) FROM territory_feed WHERE territory_id = ?1",
                *id,
            )?)?;
        }
    }
    Ok(())
}

/// The name typed for a land (a territory or a continent): cleaned, and refused when another
/// land has it without case. `me` is the land being renamed (`None` for a new one). For the
/// op compilers, which build edits from typed input.
pub(super) fn land_name(conn: &Connection, raw: &str, me: Option<i64>) -> Result<String> {
    let others = column_except(
        conn,
        "SELECT name FROM territory WHERE id != ?1",
        me.unwrap_or(-1),
    )?;
    checked_name(raw, NameKind::Land, others.iter().map(String::as_str))
}

/// The sea name typed for a theme: cleaned, and refused when another theme has it as its
/// sea name or its name, without case. `me` is the sea being renamed (`None` for a new one).
pub(super) fn sea_name(conn: &Connection, raw: &str, me: Option<i64>) -> Result<String> {
    let others = column_except(
        conn,
        "SELECT name FROM theme WHERE id != ?1 UNION ALL \
         SELECT sea_name FROM theme WHERE id != ?1",
        me.unwrap_or(-1),
    )?;
    checked_name(raw, NameKind::Sea, others.iter().map(String::as_str))
}

/// The first column of `sql`, run with `id`.
fn column_except(conn: &Connection, sql: &str, id: i64) -> Result<Vec<String>> {
    Ok(conn
        .prepare(sql)?
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}

/// The patch that undoes `changes`: each change reversed, in reverse order.
pub(super) fn invert(changes: &[Change]) -> Vec<Change> {
    changes
        .iter()
        .rev()
        .map(|change| match change.clone() {
            Change::Sea { before, after } => Change::Sea {
                before: after,
                after: before,
            },
            Change::Land { before, after } => Change::Land {
                before: after,
                after: before,
            },
            Change::Member {
                feed,
                land,
                before,
                after,
            } => Change::Member {
                feed,
                land,
                before: after,
                after: before,
            },
            Change::FeedSeas {
                feeds,
                before,
                after,
            } => Change::FeedSeas {
                feeds,
                before: after,
                after: before,
            },
        })
        .collect()
}

impl Store {
    /// Applies one chart edit in one transaction and returns its label and its undo.
    pub fn apply_chart_edit(&self, edit: &ChartEdit) -> Result<ChartEditDone> {
        // Compiled inside the write transaction, so what the checks read is what is written
        // over; any refusal drops the transaction and writes nothing.
        let tx = self.write_tx()?;
        let (label, changes) = chart_ops::compile(&tx, edit)?;
        apply_patch(&tx, &changes)?;
        tx.commit()?;
        let undo = ChartEdit::Patch {
            label: label.clone(),
            changes: invert(&changes),
        };
        Ok(ChartEditDone { label, undo })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::library::tests::add_feed;

    // ---- fixtures -------------------------------------------------------------------------

    fn sea_row(id: i64, key: &str, name: &str, sea: &str, position: i64) -> SeaRow {
        SeaRow {
            id,
            key: key.into(),
            name: name.into(),
            sea: sea.into(),
            position,
        }
    }

    fn land_row(id: i64, name: &str, continent: Option<i64>) -> LandRow {
        LandRow {
            id,
            name: name.into(),
            continent,
        }
    }

    fn insert_theme(s: &Store, row: &SeaRow) {
        s.conn()
            .execute(
                "INSERT INTO theme (id, key, name, sea_name, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![row.id, row.key, row.name, row.sea, row.position],
            )
            .unwrap();
    }

    fn insert_land(s: &Store, id: i64, name: &str, members: &[FeedId]) {
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind) VALUES (?1, ?2, 'territory')",
                params![id, name],
            )
            .unwrap();
        for (position, feed) in members.iter().enumerate() {
            s.conn()
                .execute(
                    "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (?1, ?2, ?3)",
                    params![id, feed, position as i64],
                )
                .unwrap();
        }
    }

    fn insert_continent(s: &Store, id: i64, name: &str, theme: i64) {
        s.conn()
            .execute(
                "INSERT INTO territory (id, name, kind, theme_id) VALUES (?1, ?2, 'continent', ?3)",
                params![id, name, theme],
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

    fn sea_a() -> SeaRow {
        sea_row(1, "a", "Name a", "SEA A", 0)
    }

    fn sea_b() -> SeaRow {
        sea_row(2, "b", "Name b", "SEA B", 1)
    }

    /// Six feeds; seas `a` (feeds 1, 2) and `b` (feed 3, and continent 11 `ISLES OF B`);
    /// land 10 `APPLE PRESS` (feeds 1, 2) and land 12 `KITE CLUB` (feeds 4, 5).
    fn chart_store() -> Store {
        let s = Store::open_in_memory().unwrap();
        for id in 1..=6 {
            add_feed(&s, id, None, id, None, "active", "daily");
        }
        insert_theme(&s, &sea_a());
        insert_theme(&s, &sea_b());
        set_theme(&s, 1, Some(1));
        set_theme(&s, 2, Some(1));
        set_theme(&s, 3, Some(2));
        insert_land(&s, 10, "APPLE PRESS", &[1, 2]);
        insert_continent(&s, 11, "ISLES OF B", 2);
        insert_land(&s, 12, "KITE CLUB", &[4, 5]);
        s
    }

    fn count(s: &Store, table: &str) -> i64 {
        s.conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn patch(changes: Vec<Change>) -> ChartEdit {
        ChartEdit::Patch {
            label: "Test".into(),
            changes,
        }
    }

    /// Each feed's theme id, by feed id.
    fn feed_themes(s: &Store) -> Vec<(FeedId, Option<i64>)> {
        s.conn()
            .prepare("SELECT id, theme_id FROM feed ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    /// Applies `changes` and asserts it is refused with `expected` and writes nothing.
    fn assert_refused(s: &Store, changes: Vec<Change>, expected: &str) {
        let before = s.export_chart().unwrap();
        let rows = snapshot_rows(s);
        let themed = feed_themes(s);
        let err = s.apply_chart_edit(&patch(changes)).unwrap_err();
        assert_eq!(err.to_string(), expected, "{err:?}");
        assert_eq!(s.export_chart().unwrap(), before);
        assert_eq!(snapshot_rows(s), rows);
        assert_eq!(feed_themes(s), themed);
    }

    const CHANGED: &str = "The chart changed; nothing was done.";

    // ---- the wire -------------------------------------------------------------------------

    #[test]
    fn edits_round_trip_through_json() {
        let land = LandRef {
            id: 10,
            name: "APPLE PRESS".into(),
        };
        let sea = SeaRef {
            id: 1,
            key: "a".into(),
            sea: "SEA A".into(),
        };
        let land_json = json!({"id": 10, "name": "APPLE PRESS"});
        let sea_json = json!({"id": 1, "key": "a", "sea": "SEA A"});
        let cases: Vec<(ChartEdit, serde_json::Value)> = vec![
            (
                ChartEdit::Join {
                    feed: 3,
                    land: land.clone(),
                },
                json!({"op": "join", "feed": 3, "land": land_json}),
            ),
            (
                ChartEdit::Leave {
                    feed: 1,
                    land: land.clone(),
                },
                json!({"op": "leave", "feed": 1, "land": land_json}),
            ),
            (
                ChartEdit::Found {
                    name: "Kites".into(),
                    feeds: vec![4, 5],
                },
                json!({"op": "found", "name": "Kites", "feeds": [4, 5]}),
            ),
            (
                ChartEdit::RenameLand {
                    land: land.clone(),
                    name: "Pressed".into(),
                },
                json!({"op": "rename_land", "land": land_json, "name": "Pressed"}),
            ),
            (
                ChartEdit::DeleteLand { land: land.clone() },
                json!({"op": "delete_land", "land": land_json}),
            ),
            (
                ChartEdit::MakeContinent {
                    sea: sea.clone(),
                    name: "Mainland".into(),
                },
                json!({"op": "make_continent", "sea": sea_json, "name": "Mainland"}),
            ),
            (
                ChartEdit::SetSea {
                    feed: 3,
                    from: None,
                    to: Some(sea.clone()),
                },
                json!({"op": "set_sea", "feed": 3, "from": null, "to": {"id": 1, "key": "a", "sea": "SEA A"}}),
            ),
            (
                ChartEdit::NewSea {
                    name: "Sea of Kites".into(),
                },
                json!({"op": "new_sea", "name": "Sea of Kites"}),
            ),
            (
                ChartEdit::RenameSea {
                    sea: sea.clone(),
                    name: "SEA OF A".into(),
                },
                json!({"op": "rename_sea", "sea": sea_json, "name": "SEA OF A"}),
            ),
            (
                ChartEdit::OrderSeas { seas: vec![2, 1] },
                json!({"op": "order_seas", "seas": [2, 1]}),
            ),
            (
                ChartEdit::DeleteSea { sea: sea.clone() },
                json!({"op": "delete_sea", "sea": sea_json}),
            ),
            (
                patch(vec![
                    Change::Sea {
                        before: None,
                        after: Some(sea_a()),
                    },
                    Change::Land {
                        before: Some(land_row(10, "APPLE PRESS", None)),
                        after: Some(land_row(10, "ISLES", Some(1))),
                    },
                    Change::Member {
                        feed: 1,
                        land: 10,
                        before: Some(0),
                        after: None,
                    },
                    Change::FeedSeas {
                        feeds: vec![1, 2],
                        before: Some(1),
                        after: None,
                    },
                ]),
                json!({"op": "patch", "label": "Test", "changes": [
                    {"kind": "sea", "before": null,
                     "after": {"id": 1, "key": "a", "name": "Name a", "sea": "SEA A", "position": 0}},
                    {"kind": "land", "before": {"id": 10, "name": "APPLE PRESS", "continent": null},
                     "after": {"id": 10, "name": "ISLES", "continent": 1}},
                    {"kind": "member", "feed": 1, "land": 10, "before": 0, "after": null},
                    {"kind": "feed_seas", "feeds": [1, 2], "before": 1, "after": null},
                ]}),
            ),
        ];
        assert_eq!(cases.len(), 12, "one value per op");
        for (edit, wire) in cases {
            assert_eq!(serde_json::to_value(&edit).unwrap(), wire, "{}", edit.op());
            assert_eq!(edit.op(), wire["op"]);
            let back: ChartEdit = serde_json::from_value(wire).unwrap();
            assert_eq!(back, edit);
        }

        for bad in [
            json!({"op": "teleport", "feed": 3}),
            json!({"op": "join", "feed": 3}),
            json!({"op": "set_sea", "feed": 3, "to": null}),
            json!({"op": "patch", "label": "x", "changes": [{"kind": "member", "feed": 1, "land": 10, "after": 0}]}),
            json!({"op": "patch", "label": "x", "changes": [{"kind": "shore", "before": null, "after": null}]}),
            json!({"feed": 3, "land": {"id": 10, "name": "APPLE PRESS"}}),
        ] {
            assert!(
                serde_json::from_value::<ChartEdit>(bad.clone()).is_err(),
                "{bad}"
            );
        }

        let done = ChartEditDone {
            label: "Join APPLE PRESS".into(),
            undo: patch(vec![]),
        };
        assert_eq!(
            done.event(),
            json!({"event": "chart_edit", "label": "Join APPLE PRESS",
                   "undo": {"op": "patch", "label": "Test", "changes": []}})
        );
    }

    // ---- names and keys -------------------------------------------------------------------

    #[test]
    fn name_cases_from_the_shared_table() {
        let table: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/chart-name-cases.json"
        )))
        .unwrap();
        let cases = table["cases"].as_array().unwrap();
        assert!(cases.len() >= 12, "{}", cases.len());
        for case in cases {
            let label = case["case"].as_str().unwrap();
            let raw = case["raw"].as_str().unwrap();
            let kind = match case["kind"].as_str().unwrap() {
                "land" => NameKind::Land,
                "sea" => NameKind::Sea,
                other => panic!("{label}: kind {other}"),
            };
            let me = case["self"].as_i64();
            let mut others: Vec<&str> = Vec::new();
            for row in case["existing"].as_array().unwrap() {
                if Some(row["id"].as_i64().unwrap()) == me {
                    continue;
                }
                others.push(row["name"].as_str().unwrap());
                if kind == NameKind::Sea {
                    others.push(row["sea"].as_str().unwrap());
                }
            }
            let got = match checked_name(raw, kind, others) {
                Ok(name) => json!({ "name": name }),
                Err(e) => json!({ "error": e.to_string() }),
            };
            assert_eq!(got, case["expect"], "{label}");
        }
    }

    #[test]
    fn clean_name_trims_and_counts_characters() {
        assert_eq!(clean_name("  Apple  ").unwrap(), "Apple");
        for bad in [String::new(), "   ".into(), "é".repeat(41)] {
            assert!(
                matches!(clean_name(&bad), Err(Error::NameLength)),
                "{bad:?}"
            );
        }
        assert_eq!(clean_name(&"é".repeat(40)).unwrap(), "é".repeat(40));
        assert_eq!(
            Error::NameLength.to_string(),
            "A name has 1 to 40 characters."
        );
        assert!(same_name(" Apple Press", "APPLE PRESS "));
        assert!(!same_name("Apple Press", "Apple Presses"));
    }

    #[test]
    fn derive_key_follows_the_spec() {
        let free = |_: &str| false;
        let cases = [
            (derive_key("Sea of Kites!!", free), "sea-of-kites"),
            (derive_key("Comics", |k| k == "comics"), "comics-2"),
            (
                derive_key("Comics", |k| k == "comics" || k == "comics-2"),
                "comics-3",
            ),
            (derive_key("None", free), "none-2"),
            (derive_key("!!!", free), "sea"),
            (derive_key("!!!", |k| k == "sea"), "sea-2"),
            (derive_key("--Été & Mer--", free), "été-mer"),
        ];
        for (got, want) in cases {
            assert_eq!(got, want);
            check_theme_key(&got).unwrap();
        }
    }

    #[test]
    fn names_clash_without_case() {
        // The name rules run where an edit is built from typed input (the op compilers),
        // through `land_name` and `sea_name`; a patch only meets the schema's exact UNIQUE.
        let s = chart_store();
        let c = s.conn();
        let land_taken = "A land with that name already exists.";
        let sea_taken = "A sea with that name already exists.";
        assert_eq!(
            land_name(c, "apple press", None).unwrap_err().to_string(),
            land_taken
        );
        // A continent's name is a land name too.
        assert_eq!(
            land_name(c, " isles of b ", None).unwrap_err().to_string(),
            land_taken
        );
        assert_eq!(
            land_name(c, "  Apple Press ", Some(10)).unwrap(),
            "Apple Press"
        );
        assert_eq!(
            land_name(c, "kite club", Some(10)).unwrap_err().to_string(),
            land_taken
        );
        assert!(matches!(
            land_name(c, &"x".repeat(41), None),
            Err(Error::NameLength)
        ));

        insert_theme(&s, &sea_row(5, "kites", "Kites", "SEA OF KITES", 2));
        assert_eq!(
            sea_name(c, "sea of kites", None).unwrap_err().to_string(),
            sea_taken
        );
        // A theme's name counts as a sea name too.
        c.execute(
            "UPDATE theme SET name = 'Sea of kites', sea_name = 'SEA OF PAPER' WHERE id = 5",
            [],
        )
        .unwrap();
        assert_eq!(
            sea_name(c, "sea of kites", None).unwrap_err().to_string(),
            sea_taken
        );
        assert_eq!(
            sea_name(c, "Sea of Paper", Some(5)).unwrap(),
            "Sea of Paper"
        );
        assert_eq!(
            sea_name(c, "sea a", Some(5)).unwrap_err().to_string(),
            sea_taken
        );

        // A patch meets the schema's exact UNIQUE, refused with the land message.
        assert_refused(
            &s,
            vec![Change::Land {
                before: None,
                after: Some(land_row(20, "APPLE PRESS", None)),
            }],
            land_taken,
        );
        // Renaming a land to another case of its own name passes.
        s.apply_chart_edit(&patch(vec![Change::Land {
            before: Some(land_row(10, "APPLE PRESS", None)),
            after: Some(land_row(10, "Apple Press", None)),
        }]))
        .unwrap();
        let name: String = c
            .query_row("SELECT name FROM territory WHERE id = 10", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "Apple Press");
    }

    #[test]
    fn edit_texts_are_the_global_constraints() {
        assert_eq!(Error::ChartChanged.to_string(), CHANGED);
        assert_eq!(
            Error::LandNameTaken.to_string(),
            "A land with that name already exists."
        );
        assert_eq!(
            Error::SeaNameTaken.to_string(),
            "A sea with that name already exists."
        );
        assert_eq!(
            Error::NewLandNeedsTwo.to_string(),
            "A new land needs at least two feeds."
        );
        assert_eq!(
            Error::PartOfContinent {
                name: "SILICON SHORES".into()
            }
            .to_string(),
            "Part of the continent SILICON SHORES: move it to another sea, or turn the \
             continent back into open water."
        );
    }

    // ---- the patch engine -----------------------------------------------------------------

    /// A valid change listed before each stale one, to show nothing at all is written.
    fn harmless() -> Change {
        Change::Sea {
            before: None,
            after: Some(sea_row(9, "kites", "Kites", "SEA OF KITES", 2)),
        }
    }

    #[test]
    fn stale_refs_are_refused_and_write_nothing() {
        let s = chart_store();
        let feeds_before = count(&s, "territory_feed");
        let stale = [
            // The land was renamed elsewhere.
            Change::Land {
                before: Some(land_row(10, "OLD PRESS", None)),
                after: Some(land_row(10, "NEW PRESS", None)),
            },
            // The land is gone.
            Change::Land {
                before: Some(land_row(30, "GONE", None)),
                after: None,
            },
            // A land with that id exists already.
            Change::Land {
                before: None,
                after: Some(land_row(12, "FRESH", None)),
            },
            // Feed 2 sits at position 1, not 0.
            Change::Member {
                feed: 2,
                land: 10,
                before: Some(0),
                after: None,
            },
            // Feed 3 is in no land.
            Change::Member {
                feed: 3,
                land: 10,
                before: Some(2),
                after: Some(0),
            },
            // Feed 1 is already a member.
            Change::Member {
                feed: 1,
                land: 10,
                before: None,
                after: Some(5),
            },
            // Feed 3 is in sea b, not a.
            Change::FeedSeas {
                feeds: vec![1, 3],
                before: Some(1),
                after: Some(2),
            },
            // No feed 99.
            Change::FeedSeas {
                feeds: vec![99],
                before: None,
                after: Some(1),
            },
            // Sea a's sea name differs.
            Change::Sea {
                before: Some(sea_row(1, "a", "Name a", "SEA OF A", 0)),
                after: None,
            },
            // Sea b's position differs.
            Change::Sea {
                before: Some(sea_row(2, "b", "Name b", "SEA B", 7)),
                after: Some(sea_row(2, "b", "Name b", "SEA BEE", 7)),
            },
            // A theme with that id exists already.
            Change::Sea {
                before: None,
                after: Some(sea_row(1, "z", "Zed", "SEA Z", 5)),
            },
        ];
        for change in stale {
            assert_refused(&s, vec![harmless(), change], CHANGED);
        }
        assert_eq!(count(&s, "territory_feed"), feeds_before);
        assert_eq!(count(&s, "theme"), 2);
    }

    /// Every theme row (by id) and every territory row with its members, as text.
    fn snapshot_rows(s: &Store) -> (Vec<Option<SeaRow>>, Vec<String>) {
        let ids: Vec<i64> = s
            .conn()
            .prepare("SELECT id FROM theme ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        let themes = ids
            .into_iter()
            .map(|id| read_sea(s.conn(), id).unwrap())
            .collect();
        let lands = s
            .conn()
            .prepare(
                "SELECT t.id || ' ' || t.name || ' ' || t.kind || ' ' || ifnull(t.theme_id, '-') \
                 || ' [' || ifnull((SELECT group_concat(feed_id || '@' || position, ',') FROM \
                 (SELECT * FROM territory_feed WHERE territory_id = t.id ORDER BY position)), '') \
                 || ']' FROM territory t ORDER BY t.id",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        (themes, lands)
    }

    #[test]
    fn a_patch_and_its_inverse_restore_the_rows() {
        let s = chart_store();
        let start = s.export_chart().unwrap();
        let start_rows = snapshot_rows(&s);
        let changes = vec![
            // A new sea, with feeds 4 and 5 in it.
            Change::Sea {
                before: None,
                after: Some(sea_row(7, "kites", "Kites", "SEA OF KITES", 2)),
            },
            Change::FeedSeas {
                feeds: vec![4, 5],
                before: None,
                after: Some(7),
            },
            // A new land of feeds 3 and 6.
            Change::Land {
                before: None,
                after: Some(land_row(20, "NEW LAND", None)),
            },
            Change::Member {
                feed: 6,
                land: 20,
                before: None,
                after: Some(0),
            },
            Change::Member {
                feed: 3,
                land: 20,
                before: None,
                after: Some(1),
            },
            // Land 10 deleted with both its members.
            Change::Member {
                feed: 1,
                land: 10,
                before: Some(0),
                after: None,
            },
            Change::Member {
                feed: 2,
                land: 10,
                before: Some(1),
                after: None,
            },
            Change::Land {
                before: Some(land_row(10, "APPLE PRESS", None)),
                after: None,
            },
            // Sea b renamed and moved to the front.
            Change::Sea {
                before: Some(sea_b()),
                after: Some(sea_row(2, "b", "Name b", "SEA OF BEES", -1)),
            },
            // Land 12 renamed, feed 5 moved to the front.
            Change::Land {
                before: Some(land_row(12, "KITE CLUB", None)),
                after: Some(land_row(12, "KITE SOCIETY", None)),
            },
            Change::Member {
                feed: 5,
                land: 12,
                before: Some(1),
                after: Some(-1),
            },
        ];
        let done = s.apply_chart_edit(&patch(changes.clone())).unwrap();
        assert_eq!(done.label, "Test");
        let applied = snapshot_rows(&s);
        assert_ne!(applied, start_rows);
        assert_eq!(
            applied.1,
            [
                "11 ISLES OF B continent 2 []",
                "12 KITE SOCIETY territory - [5@-1,4@0]",
                "20 NEW LAND territory - [6@0,3@1]",
            ]
        );

        let ChartEdit::Patch {
            changes: ref undo, ..
        } = done.undo
        else {
            panic!("the undo is a patch: {:?}", done.undo)
        };
        assert_eq!(undo, &invert(&changes));
        assert_eq!(invert(undo), changes);
        let undone = s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(s.export_chart().unwrap(), start);
        // Land 10 is back with its id and its members' positions.
        assert_eq!(snapshot_rows(&s), start_rows);

        // Redo, then the rows created the first time come back with the same ids.
        s.apply_chart_edit(&undone.undo).unwrap();
        assert_eq!(snapshot_rows(&s), applied);
    }

    #[test]
    fn deleting_a_land_with_unlisted_members_is_refused() {
        let s = chart_store();
        // Feed 2 is not removed: the schema's cascade would drop it silently.
        assert_refused(
            &s,
            vec![
                Change::Member {
                    feed: 1,
                    land: 10,
                    before: Some(0),
                    after: None,
                },
                Change::Land {
                    before: Some(land_row(10, "APPLE PRESS", None)),
                    after: None,
                },
            ],
            CHANGED,
        );
        // Sea a still has feeds 1 and 2.
        assert_refused(
            &s,
            vec![Change::Sea {
                before: Some(sea_a()),
                after: None,
            }],
            CHANGED,
        );
        // Sea b's feed is unthemed, but its continent still points at it.
        assert_refused(
            &s,
            vec![
                Change::FeedSeas {
                    feeds: vec![3],
                    before: Some(2),
                    after: None,
                },
                Change::Sea {
                    before: Some(sea_b()),
                    after: None,
                },
            ],
            CHANGED,
        );
        // A buried feed counts too.
        s.conn()
            .execute(
                "UPDATE feed SET state = 'buried', theme_id = 2 WHERE id = 6",
                [],
            )
            .unwrap();
        assert_refused(
            &s,
            vec![
                Change::FeedSeas {
                    feeds: vec![3],
                    before: Some(2),
                    after: None,
                },
                Change::Land {
                    before: Some(land_row(11, "ISLES OF B", Some(2))),
                    after: None,
                },
                Change::Sea {
                    before: Some(sea_b()),
                    after: None,
                },
            ],
            CHANGED,
        );
        // With every feed and the continent listed, it goes.
        s.apply_chart_edit(&patch(vec![
            Change::FeedSeas {
                feeds: vec![3, 6],
                before: Some(2),
                after: None,
            },
            Change::Land {
                before: Some(land_row(11, "ISLES OF B", Some(2))),
                after: None,
            },
            Change::Sea {
                before: Some(sea_b()),
                after: None,
            },
        ]))
        .unwrap();
        assert_eq!(count(&s, "theme"), 1);
    }

    #[test]
    fn a_continent_takes_no_members() {
        let s = chart_store();
        // No member row into a continent.
        assert_refused(
            &s,
            vec![Change::Member {
                feed: 6,
                land: 11,
                before: None,
                after: Some(0),
            }],
            CHANGED,
        );
        // No territory turned into a continent while it keeps its members.
        assert_refused(
            &s,
            vec![Change::Land {
                before: Some(land_row(10, "APPLE PRESS", None)),
                after: Some(land_row(10, "APPLE PRESS", Some(1))),
            }],
            CHANGED,
        );
        // With its members removed in the same patch, it may become one.
        s.apply_chart_edit(&patch(vec![
            Change::Member {
                feed: 1,
                land: 10,
                before: Some(0),
                after: None,
            },
            Change::Member {
                feed: 2,
                land: 10,
                before: Some(1),
                after: None,
            },
            Change::Land {
                before: Some(land_row(10, "APPLE PRESS", None)),
                after: Some(land_row(10, "APPLE PRESS", Some(1))),
            },
        ]))
        .unwrap();
        assert_eq!(snapshot_rows(&s).1[0], "10 APPLE PRESS continent 1 []");
    }

    #[test]
    fn a_found_takes_the_exact_name_of_a_land_it_dissolves() {
        let s = chart_store();
        // KITE CLUB's islands and feed 6 make a new land under the very same name.
        s.apply_chart_edit(&patch(vec![
            Change::Member {
                feed: 4,
                land: 12,
                before: Some(0),
                after: None,
            },
            Change::Member {
                feed: 5,
                land: 12,
                before: Some(1),
                after: None,
            },
            Change::Land {
                before: Some(land_row(12, "KITE CLUB", None)),
                after: None,
            },
            Change::Land {
                before: None,
                after: Some(land_row(20, "KITE CLUB", None)),
            },
            Change::Member {
                feed: 4,
                land: 20,
                before: None,
                after: Some(0),
            },
            Change::Member {
                feed: 5,
                land: 20,
                before: None,
                after: Some(1),
            },
            Change::Member {
                feed: 6,
                land: 20,
                before: None,
                after: Some(2),
            },
        ]))
        .unwrap();
        assert_eq!(
            snapshot_rows(&s).1,
            [
                "10 APPLE PRESS territory - [1@0,2@1]",
                "11 ISLES OF B continent 2 []",
                "20 KITE CLUB territory - [4@0,5@1,6@2]",
            ]
        );
    }

    /// Applies `changes`, then its undo, and asserts every row is back as it was.
    fn assert_undo_restores(s: &Store, changes: Vec<Change>) {
        let rows = snapshot_rows(s);
        let themed = feed_themes(s);
        let done = s.apply_chart_edit(&patch(changes)).unwrap();
        assert_ne!(snapshot_rows(s), rows);
        s.apply_chart_edit(&done.undo).unwrap();
        assert_eq!(snapshot_rows(s), rows);
        assert_eq!(feed_themes(s), themed);
    }

    #[test]
    fn undo_restores_an_imported_long_name() {
        let s = chart_store();
        let long = "THE VERY LONG NAME OF AN IMPORTED LAND, 45 C";
        assert_eq!(long.chars().count(), 44);
        let long = format!("{long}H");
        s.conn()
            .execute(
                "UPDATE territory SET name = ?1 WHERE id = 10",
                [long.as_str()],
            )
            .unwrap();
        assert_undo_restores(
            &s,
            vec![Change::Land {
                before: Some(land_row(10, &long, None)),
                after: Some(land_row(10, "SHORT", None)),
            }],
        );
        let name: String = s
            .conn()
            .query_row("SELECT name FROM territory WHERE id = 10", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, long);
    }

    #[test]
    fn undo_restores_a_feed_in_two_territories() {
        let s = chart_store();
        // As an import allows: feed 1 in APPLE PRESS and in KITE CLUB.
        s.conn()
            .execute(
                "INSERT INTO territory_feed (territory_id, feed_id, position) VALUES (12, 1, 2)",
                [],
            )
            .unwrap();
        insert_land(&s, 30, "THIRD LAND", &[3, 6]);
        // It joins a third land, leaving both.
        assert_undo_restores(
            &s,
            vec![
                Change::Member {
                    feed: 1,
                    land: 10,
                    before: Some(0),
                    after: None,
                },
                Change::Member {
                    feed: 1,
                    land: 12,
                    before: Some(2),
                    after: None,
                },
                Change::Member {
                    feed: 1,
                    land: 30,
                    before: None,
                    after: Some(2),
                },
            ],
        );
        assert_eq!(
            snapshot_rows(&s).1,
            [
                "10 APPLE PRESS territory - [1@0,2@1]",
                "11 ISLES OF B continent 2 []",
                "12 KITE CLUB territory - [4@0,5@1,1@2]",
                "30 THIRD LAND territory - [3@0,6@1]",
            ]
        );
    }

    #[test]
    fn undo_restores_a_case_twin() {
        let s = chart_store();
        // As an import allows: two lands whose names differ only by case.
        insert_land(&s, 30, "Apple Press", &[3, 6]);
        assert_undo_restores(
            &s,
            vec![Change::Land {
                before: Some(land_row(30, "Apple Press", None)),
                after: Some(land_row(30, "CIDER MILL", None)),
            }],
        );
        // And the delete of one, with its members, undoes too.
        assert_undo_restores(
            &s,
            vec![
                Change::Member {
                    feed: 3,
                    land: 30,
                    before: Some(0),
                    after: None,
                },
                Change::Member {
                    feed: 6,
                    land: 30,
                    before: Some(1),
                    after: None,
                },
                Change::Land {
                    before: Some(land_row(30, "Apple Press", None)),
                    after: None,
                },
            ],
        );
    }
}
