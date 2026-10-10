//! Facts that need several feeds at once: duplicates and look-alike titles.

use std::collections::{HashMap, HashSet};

use crate::health::{DUPLICATE_OVERLAP, SIMILAR_TITLE_DISTANCE};
use crate::model::{Action, Diagnosis, FeedId, FolderId, Remark, Status};

/// Entry-overlap duplicates need at least this many entries on both sides.
const MIN_ENTRIES_FOR_OVERLAP: usize = 3;
/// Normalized titles shorter than this are never compared.
const MIN_TITLE_CHARS: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct LinkInput {
    pub feed_id: FeedId,
    pub folder_id: Option<FolderId>,
    pub title: String,
    /// `url_key` of the final URL after redirects (or of `xml_url` when never fetched).
    pub final_key: String,
    /// GUIDs of the 20 newest entries.
    pub guids: Vec<String>,
}

pub fn find_duplicates(feeds: &[LinkInput]) -> HashMap<FeedId, FeedId> {
    let mut sorted: Vec<&LinkInput> = feeds.iter().collect();
    sorted.sort_by_key(|f| f.feed_id);
    let mut dups = HashMap::new();
    let mut by_key: HashMap<&str, FeedId> = HashMap::new();
    for f in &sorted {
        match by_key.get(f.final_key.as_str()) {
            Some(&canonical) => {
                dups.insert(f.feed_id, canonical);
            }
            None => {
                by_key.insert(&f.final_key, f.feed_id);
            }
        }
    }
    for (i, a) in sorted.iter().enumerate() {
        if dups.contains_key(&a.feed_id) {
            continue;
        }
        for b in &sorted[i + 1..] {
            if !dups.contains_key(&b.feed_id) && overlap(&a.guids, &b.guids) >= DUPLICATE_OVERLAP {
                dups.insert(b.feed_id, a.feed_id);
            }
        }
    }
    dups
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.len() < MIN_ENTRIES_FOR_OVERLAP || b.len() < MIN_ENTRIES_FOR_OVERLAP {
        return 0.0;
    }
    let set: HashSet<&String> = a.iter().collect();
    let common = b.iter().filter(|g| set.contains(g)).count();
    common as f64 / a.len().min(b.len()) as f64
}

fn normalized(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Marks Dead/Fossil feeds that look like an Alive/Dormant feed of the same folder
/// (a blog that moved platforms) and proposes merging them into the living one.
pub fn apply_similar_titles(feeds: &[LinkInput], diags: &mut HashMap<FeedId, Diagnosis>) {
    for gone in feeds {
        for alive in feeds {
            if gone.feed_id == alive.feed_id || gone.folder_id != alive.folder_id {
                continue;
            }
            let (a, b) = (normalized(&gone.title), normalized(&alive.title));
            if a.chars().count() < MIN_TITLE_CHARS
                || strsim::levenshtein(&a, &b) > SIMILAR_TITLE_DISTANCE
            {
                continue;
            }
            let gone_status = diags.get(&gone.feed_id).map(|d| d.status);
            let alive_status = diags.get(&alive.feed_id).map(|d| d.status);
            if !matches!(gone_status, Some(Status::Dead | Status::Fossil))
                || !matches!(alive_status, Some(Status::Alive | Status::Dormant))
            {
                continue;
            }
            if let Some(d) = diags.get_mut(&gone.feed_id) {
                d.remarks.push(Remark::SimilarTitle {
                    feed_id: alive.feed_id,
                });
                d.action = Some(Action::MergeInto {
                    feed_id: alive.feed_id,
                });
                d.evidence.bullets.push(format!(
                    "Looks like the same site as #{} \"{}\", which is still active.",
                    alive.feed_id, alive.title
                ));
            }
            if let Some(d) = diags.get_mut(&alive.feed_id) {
                d.remarks.push(Remark::SimilarTitle {
                    feed_id: gone.feed_id,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cadence, Evidence};

    fn input(
        id: FeedId,
        folder: Option<FolderId>,
        title: &str,
        key: &str,
        guids: &[&str],
    ) -> LinkInput {
        LinkInput {
            feed_id: id,
            folder_id: folder,
            title: title.into(),
            final_key: key.into(),
            guids: guids.iter().map(|g| g.to_string()).collect(),
        }
    }

    fn diag(status: Status) -> Diagnosis {
        Diagnosis {
            status,
            reason: String::new(),
            evidence: Evidence::default(),
            remarks: vec![],
            action: (status == Status::Dead).then_some(Action::Bury),
            hijack_score: 0.0,
            cadence: Cadence::default(),
        }
    }

    #[test]
    fn same_final_address_is_a_duplicate_of_the_older_feed() {
        let dups = find_duplicates(&[
            input(9, None, "B", "x.org/feed", &[]),
            input(3, None, "A", "x.org/feed", &[]),
        ]);
        assert_eq!(dups, HashMap::from([(9, 3)]));
    }

    #[test]
    fn mostly_shared_entries_make_a_duplicate() {
        let a = input(1, None, "A", "a.org/rss", &["1", "2", "3", "4", "5"]);
        let b = input(2, None, "B", "b.org/rss", &["1", "2", "3", "4", "z"]);
        let c = input(3, None, "C", "c.org/rss", &["1", "2", "x", "y", "z"]);
        let dups = find_duplicates(&[a, b, c]);
        assert_eq!(dups.get(&2), Some(&1));
        assert_eq!(dups.get(&3), None, "2/5 shared is not a duplicate");
    }

    #[test]
    fn tiny_feeds_are_never_duplicates_by_entries() {
        let dups = find_duplicates(&[
            input(1, None, "A", "a", &["1", "2"]),
            input(2, None, "B", "b", &["1", "2"]),
        ]);
        assert!(dups.is_empty());
    }

    #[test]
    fn dead_look_alike_in_same_folder_is_merged_into_the_living_one() {
        let feeds = [
            input(10, Some(5), "Orange Drenka", "orange.example.org/feed", &[]),
            input(
                11,
                Some(5),
                "Orange-Drenka",
                "orange-drenka.example.net/rss",
                &[],
            ),
        ];
        let mut diags = HashMap::from([(10, diag(Status::Alive)), (11, diag(Status::Dead))]);
        apply_similar_titles(&feeds, &mut diags);
        assert_eq!(diags[&11].action, Some(Action::MergeInto { feed_id: 10 }));
        assert!(
            diags[&11]
                .remarks
                .contains(&Remark::SimilarTitle { feed_id: 10 })
        );
        assert!(
            diags[&10]
                .remarks
                .contains(&Remark::SimilarTitle { feed_id: 11 })
        );
        assert_eq!(diags[&10].action, None);
    }

    #[test]
    fn look_alikes_need_same_folder_long_titles_and_a_dead_side() {
        let other_folder = [
            input(1, Some(1), "Orange Drenka", "a", &[]),
            input(2, Some(2), "Orange-Drenka", "b", &[]),
        ];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Dead))]);
        apply_similar_titles(&other_folder, &mut diags);
        assert!(diags[&2].remarks.is_empty());

        let short = [
            input(1, Some(1), "BBC", "a", &[]),
            input(2, Some(1), "CBS", "b", &[]),
        ];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Dead))]);
        apply_similar_titles(&short, &mut diags);
        assert!(diags[&2].remarks.is_empty());

        let both_alive = [
            input(1, Some(1), "Orange Drenka", "a", &[]),
            input(2, Some(1), "Orange-Drenka", "b", &[]),
        ];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Alive))]);
        apply_similar_titles(&both_alive, &mut diags);
        assert!(diags[&2].remarks.is_empty());
    }
}
