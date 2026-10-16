//! The sources column as a flat list of rows (spec §4, Sources; §6): the smart lists, the
//! folder tree in the app's display order, then the rhythm groups. Pure. The garden's pages
//! join in phase 8.

use ressac_core::model::{CadenceGroup, FeedId, FolderId};
use ressac_core::store::{Source, SourceNode, SourceNodeKind, SourcesSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub enum SidebarRow {
    /// Not selectable: `Feeds`, `Rhythm`.
    Heading(&'static str),
    Smart {
        source: Source,
        title: &'static str,
        unread: usize,
    },
    Folder {
        id: FolderId,
        name: String,
        depth: usize,
        page: Option<u16>,
        unread: usize,
    },
    Feed {
        id: FeedId,
        title: String,
        depth: usize,
        page: Option<u16>,
        unread: usize,
    },
    Rhythm {
        group: CadenceGroup,
        unread: usize,
    },
}

impl SidebarRow {
    /// The list this row opens; `None` for a heading.
    pub fn source(&self) -> Option<Source> {
        match self {
            SidebarRow::Heading(_) => None,
            SidebarRow::Smart { source, .. } => Some(*source),
            SidebarRow::Folder { id, .. } => Some(Source::Folder(*id)),
            SidebarRow::Feed { id, .. } => Some(Source::Feed(*id)),
            SidebarRow::Rhythm { group, .. } => Some(Source::Cadence(*group)),
        }
    }
}

pub fn rows(snapshot: &SourcesSnapshot) -> Vec<SidebarRow> {
    let counts = &snapshot.counts;
    let mut rows = vec![
        SidebarRow::Smart {
            source: Source::Journal,
            title: "Journal",
            unread: counts.journal,
        },
        SidebarRow::Smart {
            source: Source::Unread,
            title: "Unread",
            unread: counts.unread,
        },
        SidebarRow::Smart {
            source: Source::Today,
            title: "Today",
            unread: counts.today,
        },
        SidebarRow::Smart {
            source: Source::Starred,
            title: "Starred",
            unread: counts.starred,
        },
    ];
    if !snapshot.tree.is_empty() {
        rows.push(SidebarRow::Heading("Feeds"));
        push_nodes(&mut rows, &snapshot.tree, 0);
    }
    rows.push(SidebarRow::Heading("Rhythm"));
    rows.extend(CadenceGroup::ALL.iter().map(|&group| SidebarRow::Rhythm {
        group,
        unread: counts.cadence.get(&group).copied().unwrap_or(0),
    }));
    rows
}

fn push_nodes(rows: &mut Vec<SidebarRow>, nodes: &[SourceNode], depth: usize) {
    for node in nodes {
        rows.push(match &node.kind {
            SourceNodeKind::Folder { id, name } => SidebarRow::Folder {
                id: *id,
                name: name.clone(),
                depth,
                page: node.page,
                unread: node.unread,
            },
            SourceNodeKind::Feed { id, title } => SidebarRow::Feed {
                id: *id,
                title: title.clone(),
                depth,
                page: node.page,
                unread: node.unread,
            },
        });
        push_nodes(rows, &node.children, depth + 1);
    }
}

pub fn index_of(rows: &[SidebarRow], source: Source) -> Option<usize> {
    rows.iter().position(|row| row.source() == Some(source))
}

/// The next selectable row `delta` steps away, skipping headings; `from` at either end.
pub fn step(rows: &[SidebarRow], from: usize, delta: isize) -> usize {
    let mut at = from;
    loop {
        match at
            .checked_add_signed(delta)
            .filter(|&next| next < rows.len())
        {
            None => return from,
            Some(next) if rows[next].source().is_some() => return next,
            Some(next) => at = next,
        }
    }
}

#[cfg(test)]
pub(crate) fn sample_snapshot() -> SourcesSnapshot {
    use ressac_core::store::SourceCounts;
    let mut cadence: std::collections::BTreeMap<CadenceGroup, usize> =
        CadenceGroup::ALL.iter().map(|&group| (group, 0)).collect();
    cadence.insert(CadenceGroup::Daily, 3);
    SourcesSnapshot {
        counts: SourceCounts {
            journal: 2,
            unread: 5,
            today: 1,
            starred: 0,
            needs_triage: 4,
            cadence,
        },
        tree: vec![SourceNode {
            kind: SourceNodeKind::Folder {
                id: 1,
                name: "Blogs".into(),
            },
            page: Some(200),
            unread: 3,
            children: vec![SourceNode {
                kind: SourceNodeKind::Feed {
                    id: 7,
                    title: "Daring Fireball".into(),
                },
                page: Some(201),
                unread: 3,
                children: vec![],
            }],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_lists_then_the_tree_then_the_rhythm_groups() {
        let rows = rows(&sample_snapshot());
        assert_eq!(rows.len(), 13);
        assert_eq!(
            rows[0],
            SidebarRow::Smart {
                source: Source::Journal,
                title: "Journal",
                unread: 2
            }
        );
        assert_eq!(
            rows[3],
            SidebarRow::Smart {
                source: Source::Starred,
                title: "Starred",
                unread: 0
            }
        );
        assert_eq!(rows[4], SidebarRow::Heading("Feeds"));
        assert_eq!(
            rows[5],
            SidebarRow::Folder {
                id: 1,
                name: "Blogs".into(),
                depth: 0,
                page: Some(200),
                unread: 3
            }
        );
        assert_eq!(
            rows[6],
            SidebarRow::Feed {
                id: 7,
                title: "Daring Fireball".into(),
                depth: 1,
                page: Some(201),
                unread: 3
            }
        );
        assert_eq!(rows[7], SidebarRow::Heading("Rhythm"));
        assert_eq!(
            rows[8],
            SidebarRow::Rhythm {
                group: CadenceGroup::Daily,
                unread: 3
            }
        );
        assert_eq!(
            rows[12],
            SidebarRow::Rhythm {
                group: CadenceGroup::Silent,
                unread: 0
            }
        );
    }

    #[test]
    fn no_feeds_heading_without_feeds() {
        let mut snapshot = sample_snapshot();
        snapshot.tree.clear();
        let rows = rows(&snapshot);
        assert!(!rows.contains(&SidebarRow::Heading("Feeds")));
        assert_eq!(rows[4], SidebarRow::Heading("Rhythm"));
    }

    #[test]
    fn stepping_skips_headings_and_stops_at_the_ends() {
        let rows = rows(&sample_snapshot());
        assert_eq!(step(&rows, 3, 1), 5);
        assert_eq!(step(&rows, 5, -1), 3);
        assert_eq!(step(&rows, 6, 1), 8);
        assert_eq!(step(&rows, 0, -1), 0);
        assert_eq!(step(&rows, 12, 1), 12);
    }

    #[test]
    fn a_source_is_found_by_its_row() {
        let rows = rows(&sample_snapshot());
        assert_eq!(index_of(&rows, Source::Feed(7)), Some(6));
        assert_eq!(
            index_of(&rows, Source::Cadence(CadenceGroup::Rare)),
            Some(11)
        );
        assert_eq!(index_of(&rows, Source::Feed(99)), None);
    }
}
