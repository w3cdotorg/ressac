//! Feed diagnosis: pure functions from evidence to a diagnosis. No I/O.

mod cadence;
pub mod hijack;

pub use cadence::cadence;

use chrono::{DateTime, Utc};

use crate::model::FeedId;
use crate::store::{AttemptRecord, EntryRecord};

/// Everything `diagnose` may look at for one feed. Cross-feed facts (`duplicate_of`)
/// are computed by the caller.
#[derive(Debug, Clone)]
pub struct FeedEvidence {
    pub feed_id: FeedId,
    pub title: String,
    pub xml_url: String,
    pub url_key: String,
    /// Cleaned description: the feed's current one, else the OPML one.
    pub description: Option<String>,
    /// Newest first.
    pub attempts: Vec<AttemptRecord>,
    /// Newest first.
    pub entries: Vec<EntryRecord>,
    pub duplicate_of: Option<FeedId>,
    pub discovered_url: Option<String>,
    pub https_available: Option<bool>,
    pub now: DateTime<Utc>,
}
