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
    for f in sorted.iter().filter(|f| !f.final_key.is_empty()) {
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
    let a: HashSet<&String> = a.iter().collect();
    let b: HashSet<&String> = b.iter().collect();
    if a.len() < MIN_ENTRIES_FOR_OVERLAP || b.len() < MIN_ENTRIES_FOR_OVERLAP {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / a.len().min(b.len()) as f64
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
    let is_gone = |id: FeedId| {
        matches!(
            diags.get(&id).map(|d| d.status),
            Some(Status::Dead | Status::Fossil)
        )
    };
    let is_alive = |id: FeedId| {
        matches!(
            diags.get(&id).map(|d| d.status),
            Some(Status::Alive | Status::Dormant)
        )
    };
    let mut pairs: Vec<(FeedId, FeedId, String)> = Vec::new();
    for gone in feeds.iter().filter(|f| is_gone(f.feed_id)) {
        let a = normalized(&gone.title);
        if a.chars().count() < MIN_TITLE_CHARS {
            continue;
        }
        let best = feeds
            .iter()
            .filter(|alive| {
                alive.feed_id != gone.feed_id
                    && alive.folder_id == gone.folder_id
                    && is_alive(alive.feed_id)
            })
            .filter_map(|alive| {
                let b = normalized(&alive.title);
                if b.chars().count() < MIN_TITLE_CHARS {
                    return None;
                }
                let d = strsim::levenshtein(&a, &b);
                (d <= SIMILAR_TITLE_DISTANCE).then_some((d, alive))
            })
            .min_by_key(|(d, alive)| (*d, alive.feed_id));
        if let Some((_, alive)) = best {
            pairs.push((gone.feed_id, alive.feed_id, alive.title.clone()));
        }
    }
    for (gone, alive, alive_title) in pairs {
        if let Some(d) = diags.get_mut(&gone) {
            d.remarks.push(Remark::SimilarTitle { feed_id: alive });
            d.action = Some(Action::MergeInto { feed_id: alive });
            d.evidence.bullets.push(format!(
                "Looks like the same site as #{alive} \"{alive_title}\", which is still active."
            ));
        }
        if let Some(d) = diags.get_mut(&alive) {
            let remark = Remark::SimilarTitle { feed_id: gone };
            if !d.remarks.contains(&remark) {
                d.remarks.push(remark);
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

    #[test]
    fn short_alive_title_is_never_compared() {
        let feeds = [
            input(1, Some(1), "Abcd", "a", &[]),
            input(2, Some(1), "Abcdef", "b", &[]),
        ];
        let mut diags = HashMap::from([(1, diag(Status::Alive)), (2, diag(Status::Dead))]);
        apply_similar_titles(&feeds, &mut diags);
        assert!(diags[&2].remarks.is_empty());
        assert!(diags[&1].remarks.is_empty());
    }

    #[test]
    fn dead_feed_picks_one_closest_target() {
        let feeds = [
            input(1, Some(1), "Orange Drenkaaa", "a", &[]),
            input(2, Some(1), "Orange Drenkaa", "b", &[]),
            input(3, Some(1), "Orange Drenka", "c", &[]),
        ];
        let mut diags = HashMap::from([
            (1, diag(Status::Alive)),
            (2, diag(Status::Alive)),
            (3, diag(Status::Dead)),
        ]);
        apply_similar_titles(&feeds, &mut diags);
        assert_eq!(diags[&3].action, Some(Action::MergeInto { feed_id: 2 }));
        assert_eq!(diags[&3].remarks, vec![Remark::SimilarTitle { feed_id: 2 }]);
        assert_eq!(diags[&3].evidence.bullets.len(), 1);
        assert_eq!(diags[&1].remarks, vec![]);
        assert_eq!(diags[&2].remarks, vec![Remark::SimilarTitle { feed_id: 3 }]);
    }

    #[test]
    fn empty_final_keys_are_not_duplicates() {
        let dups = find_duplicates(&[input(1, None, "A", "", &[]), input(2, None, "B", "", &[])]);
        assert!(dups.is_empty());
    }

    #[test]
    fn repeated_guids_cannot_inflate_overlap() {
        let a: Vec<String> = ["1", "1", "1", "1"].iter().map(|s| s.to_string()).collect();
        let b: Vec<String> = ["1", "2", "3"].iter().map(|s| s.to_string()).collect();
        assert!(overlap(&a, &b) <= 1.0);
        assert_eq!(
            overlap(&a, &b),
            0.0,
            "one distinct guid is below the minimum"
        );
    }
}
