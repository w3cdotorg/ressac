# Phase 5: Pure diagnosis, cadence and hijack scoring

Read the plan index first. Global Constraints apply. The spec (§7) is the source of truth for the rules.

**Files (4):**
- Create: `crates/ressac-core/src/health/mod.rs` (tests inline)
- Create: `crates/ressac-core/src/health/cadence.rs` (tests inline)
- Create: `crates/ressac-core/src/health/hijack.rs` (tests inline)
- Modify: `crates/ressac-core/src/lib.rs` (add `pub mod health;`)

### Task 5.1: Cadence

**Interfaces:**
- Consumes: `store::EntryRecord`, `model::Cadence`.
- Produces: `ressac_core::health::cadence(entries: &[EntryRecord], now: DateTime<Utc>) -> Cadence`

- [ ] **Step 1: Create** `health/mod.rs` with only the module skeleton (completed in Task 5.3):

```rust
//! Feed diagnosis: pure functions from evidence to a diagnosis. No I/O.

mod cadence;
pub mod hijack;

pub use cadence::cadence;
```

Create `health/hijack.rs` with the single line `//! Hijack scoring (filled in Task 5.2).`. Add `pub mod health;` to `lib.rs`.

- [ ] **Step 2: Write the failing tests** in `health/cadence.rs`

```rust
//! Publication rhythm of a feed, from its stored entries.

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn entry(days_ago: i64, fallback: bool) -> EntryRecord {
        EntryRecord {
            guid: format!("g{days_ago}"),
            url: None,
            title: None,
            summary_html: None,
            published_at: now() - Duration::days(days_ago),
            is_fallback: fallback,
            fetched_at: now(),
        }
    }

    #[test]
    fn empty_feed_has_no_cadence() {
        assert_eq!(cadence(&[], now()), Cadence::default());
    }

    #[test]
    fn computes_last_entry_median_interval_and_monthly_rate() {
        let c = cadence(&[entry(0, false), entry(10, false), entry(30, false), entry(400, false)], now());
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, Some(20.0)); // intervals 10, 20, 370
        assert_eq!(c.entries_per_month, 0.25); // 3 entries in the last 365 days
    }

    #[test]
    fn prefers_real_dates_over_fallback_dates() {
        let c = cadence(&[entry(0, true), entry(800, false)], now());
        assert_eq!(c.last_entry_at, Some(now() - Duration::days(800)));
    }

    #[test]
    fn uses_fallback_dates_when_nothing_else_exists() {
        let c = cadence(&[entry(0, true)], now());
        assert_eq!(c.last_entry_at, Some(now()));
        assert_eq!(c.median_interval_days, None);
    }
}
```

Run: `cargo test -p ressac-core health::cadence`. Expected: compile errors.

- [ ] **Step 3: Implement.** Insert above the tests in `health/cadence.rs`:

```rust
use chrono::{DateTime, Duration, Utc};

use crate::model::Cadence;
use crate::store::EntryRecord;

/// Intervals are measured over at most this many most recent entries.
const INTERVAL_WINDOW: usize = 21;

pub fn cadence(entries: &[EntryRecord], now: DateTime<Utc>) -> Cadence {
    let mut dates: Vec<DateTime<Utc>> = entries.iter().filter(|e| !e.is_fallback).map(|e| e.published_at).collect();
    if dates.is_empty() {
        dates = entries.iter().map(|e| e.published_at).collect();
    }
    dates.sort_unstable_by(|a, b| b.cmp(a));
    let intervals: Vec<f64> = dates
        .iter()
        .take(INTERVAL_WINDOW)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (*w[0] - *w[1]).num_seconds() as f64 / 86_400.0)
        .collect();
    let year_ago = now - Duration::days(365);
    Cadence {
        last_entry_at: dates.first().copied(),
        median_interval_days: median(intervals),
        entries_per_month: dates.iter().filter(|d| **d >= year_ago).count() as f64 / 12.0,
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 { (values[mid - 1] + values[mid]) / 2.0 } else { values[mid] })
}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core health::cadence` → `4 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/ressac-core/src/health crates/ressac-core/src/lib.rs
git commit -m "Feed cadence: last entry, median interval, monthly rate

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5.2: Hijack scoring

**Interfaces:**
- Consumes: `health::FeedEvidence` (defined in Task 5.3; write 5.3's struct first if you work strictly top-down), `opml::clean_description`, `url_key::site_of`, `store::EntryRecord`, `model::ErrorKind`.
- Produces (`ressac_core::health::hijack`):
  - `SPAM_LEXICON: &[&str]`
  - `RECENT_ENTRIES: usize = 5`
  - `struct HijackAssessment { score: f32, bullets: Vec<String>, language: Option<String> }`
  - `has_spam(text: &str) -> bool`
  - `assess(ev: &FeedEvidence) -> HijackAssessment`

To keep this task compilable, add the `FeedEvidence` struct from Task 5.3, Step 3 to `health/mod.rs` now. It is a plain struct.

- [ ] **Step 1: Write the failing tests** in `health/hijack.rs` (replace the placeholder line):

```rust
//! Hijack scoring: signs that a domain now serves unrelated or spam content.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::FeedEvidence;
    use crate::store::{AttemptRecord, EntryRecord};
    use crate::url_key::url_key;
    use chrono::{DateTime, Duration, Utc};

    const VI_SPAM: &str = "Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày. Tham gia ngay để thử thách bản thân.";
    const EN: [&str; 3] = [
        "The library opened a new reading room for children and their parents",
        "Notes from the annual conference on public libraries and open access",
        "Why we still need librarians in the age of search engines and archives",
    ];
    const VI: [&str; 5] = [
        "Hướng dẫn chơi cá cược bóng đá trực tuyến cho người mới bắt đầu",
        "Nhà cái uy tín nhất năm nay với nhiều khuyến mãi hấp dẫn cho thành viên",
        "Cách nhận thưởng khi tham gia các trò chơi trực tuyến mỗi ngày",
        "Kinh nghiệm chọn sân chơi an toàn và minh bạch cho người chơi mới",
        "Tổng hợp các trò chơi được yêu thích nhất trong tháng này tại Việt Nam",
    ];

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn entry(i: i64, title: &str) -> EntryRecord {
        EntryRecord {
            guid: format!("g{i}"),
            url: None,
            title: Some(title.to_string()),
            summary_html: None,
            published_at: now() - Duration::days(i),
            is_fallback: false,
            fetched_at: now(),
        }
    }

    fn ev() -> FeedEvidence {
        FeedEvidence {
            feed_id: 1,
            title: "Bibliobsession 2.0".into(),
            xml_url: "http://www.biblio.example.org/feed/".into(),
            url_key: url_key("http://www.biblio.example.org/feed/"),
            description: None,
            attempts: vec![],
            entries: vec![],
            duplicate_of: None,
            discovered_url: None,
            https_available: None,
            now: now(),
        }
    }

    fn attempt(error: Option<ErrorKind>, chain: Vec<Hop>) -> AttemptRecord {
        AttemptRecord {
            at: now(),
            http_status: Some(200),
            error,
            redirect_chain: chain,
            content_type: None,
            network_suspect: false,
            not_modified: false,
            entry_count: None,
        }
    }

    #[test]
    fn clean_feed_scores_zero() {
        let mut e = ev();
        e.entries = EN.iter().enumerate().map(|(i, t)| entry(i as i64, t)).collect();
        let a = assess(&e);
        assert_eq!(a.score, 0.0);
        assert!(a.bullets.is_empty());
    }

    #[test]
    fn spam_lexicon_alone_is_not_enough() {
        let mut e = ev();
        e.description = Some(VI_SPAM.into());
        let a = assess(&e);
        assert!((a.score - 0.4).abs() < 1e-6, "{}", a.score);
        assert!(a.bullets[0].contains("xóc đĩa"));
    }

    #[test]
    fn spam_plus_html_page_crosses_the_threshold() {
        let mut e = ev();
        e.description = Some(VI_SPAM.into());
        e.attempts = vec![attempt(Some(ErrorKind::HtmlInsteadOfFeed), vec![])];
        assert!(assess(&e).score >= 0.6);
    }

    #[test]
    fn description_language_mismatch_is_reported_with_its_language() {
        let mut e = ev();
        e.description = Some(VI_SPAM.into());
        e.entries = EN.iter().enumerate().map(|(i, t)| entry(i as i64, t)).collect();
        let a = assess(&e);
        assert!((a.score - 0.8).abs() < 1e-6, "{}", a.score);
        assert_eq!(a.language.as_deref(), Some("vie"));
    }

    #[test]
    fn recent_entries_switching_language_is_a_signal() {
        let mut e = ev();
        let mut entries: Vec<EntryRecord> = VI.iter().enumerate().map(|(i, t)| entry(i as i64, t)).collect();
        entries.extend(EN.iter().enumerate().map(|(i, t)| entry(100 + i as i64, t)));
        e.entries = entries;
        let a = assess(&e);
        assert!(a.bullets.iter().any(|b| b.contains("Recent entries")), "{:?}", a.bullets);
        assert!(a.score >= 0.8, "lexicon (cá cược, nhà cái) + language switch: {}", a.score);
    }

    #[test]
    fn cross_domain_redirect_adds_weight_and_score_is_capped() {
        let mut e = ev();
        e.description = Some(VI_SPAM.into());
        e.entries = EN.iter().enumerate().map(|(i, t)| entry(i as i64, t)).collect();
        e.attempts = vec![attempt(
            Some(ErrorKind::HtmlInsteadOfFeed),
            vec![Hop { status: 302, url: "https://casino.example.com/".into() }],
        )];
        assert_eq!(assess(&e).score, 1.0);
    }

    #[test]
    fn has_spam_is_case_insensitive() {
        assert!(has_spam("Best CASINO bonuses"));
        assert!(!has_spam("A blog about libraries"));
    }
}
```

Run: `cargo test -p ressac-core health::hijack`. Expected: compile errors.

- [ ] **Step 2: Implement.** Insert above the tests in `health/hijack.rs`:

```rust
use crate::health::FeedEvidence;
use crate::model::{ErrorKind, Hop};
use crate::opml::clean_description;
use crate::store::EntryRecord;
use crate::url_key::site_of;

/// Lowercase phrases typical of spam that takes over expired blog domains.
pub const SPAM_LEXICON: &[&str] = &[
    "casino", "slot online", "slots", "betting", "sportsbook", "poker online", "payday loan",
    "viagra", "cialis", "escort", "xóc đĩa", "trúng lớn", "nhà cái", "cá cược", "nổ hũ",
    "judi online", "togel",
];

/// Entries considered "recent" for language comparisons.
pub const RECENT_ENTRIES: usize = 5;
/// Shorter texts are not language-detected.
const MIN_DETECT_CHARS: usize = 50;
const MIN_DETECT_CONFIDENCE: f64 = 0.8;

const W_LEXICON: f32 = 0.4;
const W_DESCRIPTION_LANGUAGE: f32 = 0.4;
const W_ENTRIES_LANGUAGE: f32 = 0.4;
const W_CROSS_DOMAIN: f32 = 0.3;
const W_HTML: f32 = 0.3;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HijackAssessment {
    pub score: f32,
    pub bullets: Vec<String>,
    /// ISO 639-3 code of the suspicious description language.
    pub language: Option<String>,
}

pub fn has_spam(text: &str) -> bool {
    !spam_hits(text).is_empty()
}

fn spam_hits(text: &str) -> Vec<&'static str> {
    let lower = text.to_lowercase();
    SPAM_LEXICON.iter().copied().filter(|w| lower.contains(w)).collect()
}

fn detect(text: &str) -> Option<(String, f64)> {
    if text.chars().count() < MIN_DETECT_CHARS {
        return None;
    }
    let info = whatlang::detect(text)?;
    (info.confidence() >= MIN_DETECT_CONFIDENCE).then(|| (info.lang().code().to_string(), info.confidence()))
}

fn entry_text(e: &EntryRecord) -> String {
    let summary = e.summary_html.as_deref().and_then(clean_description).unwrap_or_default();
    format!("{} {summary}", e.title.as_deref().unwrap_or_default())
}

fn joined<'a>(entries: impl Iterator<Item = &'a EntryRecord>) -> String {
    entries.map(entry_text).collect::<Vec<_>>().join(" ")
}

pub fn assess(ev: &FeedEvidence) -> HijackAssessment {
    let mut out = HijackAssessment::default();
    let recent = || ev.entries.iter().take(RECENT_ENTRIES);
    let older = || ev.entries.iter().skip(RECENT_ENTRIES);

    let haystack = format!("{} {} {}", ev.title, ev.description.as_deref().unwrap_or_default(), joined(recent()));
    let hits = spam_hits(&haystack);
    if !hits.is_empty() {
        out.score += W_LEXICON;
        let quoted: Vec<String> = hits.iter().map(|h| format!("\"{h}\"")).collect();
        out.bullets.push(format!("Spam vocabulary: {}.", quoted.join(", ")));
    }

    let has_older = older().next().is_some();
    let reference = if has_older {
        format!("{} {}", ev.title, joined(older()))
    } else {
        format!("{} {}", ev.title, joined(ev.entries.iter()))
    };
    if let (Some((desc_lang, conf)), Some((ref_lang, _))) =
        (ev.description.as_deref().and_then(detect), detect(&reference))
        && desc_lang != ref_lang
    {
        out.score += W_DESCRIPTION_LANGUAGE;
        out.bullets.push(format!("Description is in {desc_lang} ({conf:.2}) while the feed is in {ref_lang}."));
        out.language = Some(desc_lang);
    }

    if has_older
        && let (Some((new_lang, _)), Some((old_lang, _))) = (detect(&joined(recent())), detect(&joined(older())))
        && new_lang != old_lang
    {
        out.score += W_ENTRIES_LANGUAGE;
        out.bullets.push(format!("Recent entries are in {new_lang}, older ones in {old_lang}."));
    }

    if let Some(last) = ev.attempts.iter().find(|a| !a.network_suspect) {
        if let Some(Hop { url, .. }) = last.redirect_chain.last()
            && site_of(url) != site_of(&ev.xml_url)
        {
            out.score += W_CROSS_DOMAIN;
            out.bullets.push(format!("Redirects to another domain: {url}."));
        }
        if last.error == Some(ErrorKind::HtmlInsteadOfFeed) {
            out.score += W_HTML;
            out.bullets.push("The feed address now returns an HTML page.".to_string());
        }
    }
    out.score = out.score.min(1.0);
    out
}
```

The `if let … && …` let-chains are stable in edition 2024 (Rust ≥ 1.88).

- [ ] **Step 3: Run.** `cargo test -p ressac-core health::hijack` → `7 passed`. If whatlang misdetects a fixture sentence, lengthen that sentence; do not loosen `MIN_DETECT_CONFIDENCE`. Log it in `notes.md`.

- [ ] **Step 4: Commit**

```bash
git add crates/ressac-core/src/health
git commit -m "Hijack scoring: spam lexicon, language shifts, cross-domain redirect, HTML page

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5.3: `diagnose`

**Interfaces:**
- Consumes: `cadence`, `hijack::assess`, `store::{AttemptRecord, EntryRecord}`, `model::*`, `url_key::{url_key, https_variant}`.
- Produces (`ressac_core::health`):
  - constants `ALIVE_WITHIN_DAYS`, `FOSSIL_AFTER_DAYS`, `DEAD_FAILURE_STREAK`, `DEAD_STREAK_MIN_DAYS`, `DUPLICATE_OVERLAP`, `HIJACK_THRESHOLD`, `SIMILAR_TITLE_DISTANCE`
  - `struct FeedEvidence { feed_id, title, xml_url, url_key, description: Option<String>, attempts: Vec<AttemptRecord>, entries: Vec<EntryRecord>, duplicate_of: Option<FeedId>, discovered_url: Option<String>, https_available: Option<bool>, now: DateTime<Utc> }`. `description` is already cleaned; `attempts` and `entries` are newest first.
  - `dead_reason(ev: &FeedEvidence) -> Option<(String, String)>`, returning (reason, clue)
  - `diagnose(ev: &FeedEvidence) -> Diagnosis`

- [ ] **Step 1: Write the failing tests.** Append to `health/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Hop;

    fn now() -> DateTime<Utc> {
        "2026-10-10T08:00:00Z".parse().unwrap()
    }

    fn ago(days: i64) -> DateTime<Utc> {
        now() - Duration::days(days)
    }

    fn ok(days: i64) -> AttemptRecord {
        AttemptRecord {
            at: ago(days),
            http_status: Some(200),
            error: None,
            redirect_chain: vec![],
            content_type: Some("application/rss+xml".into()),
            network_suspect: false,
            not_modified: false,
            entry_count: Some(1),
        }
    }

    fn failed(days: i64, kind: ErrorKind) -> AttemptRecord {
        let http_status = match kind {
            ErrorKind::Http(code) => Some(code),
            _ => None,
        };
        AttemptRecord { http_status, error: Some(kind), entry_count: None, ..ok(days) }
    }

    fn entry(days: i64) -> EntryRecord {
        EntryRecord {
            guid: format!("g{days}"),
            url: None,
            title: Some(format!("Entry {days}")),
            summary_html: None,
            published_at: ago(days),
            is_fallback: false,
            fetched_at: now(),
        }
    }

    fn ev(attempts: Vec<AttemptRecord>, entries: Vec<EntryRecord>) -> FeedEvidence {
        FeedEvidence {
            feed_id: 7,
            title: "Blog".into(),
            xml_url: "https://blog.example.org/feed".into(),
            url_key: url_key("https://blog.example.org/feed"),
            description: None,
            attempts,
            entries,
            duplicate_of: None,
            discovered_url: None,
            https_available: None,
            now: now(),
        }
    }

    #[test]
    fn never_fetched_is_unchecked() {
        assert_eq!(diagnose(&ev(vec![], vec![])).status, Status::Unchecked);
    }

    #[test]
    fn age_of_last_entry_decides_alive_dormant_fossil() {
        let cases = [(10, Status::Alive, "10 d"), (200, Status::Dormant, "6 mo"), (1000, Status::Fossil, "2023")];
        for (days, status, clue) in cases {
            let d = diagnose(&ev(vec![ok(0)], vec![entry(days)]));
            assert_eq!(d.status, status, "{days} days");
            assert_eq!(d.evidence.clue, clue);
            assert_eq!(d.action, None);
        }
    }

    #[test]
    fn responding_feed_without_entries_is_a_fossil() {
        let d = diagnose(&ev(vec![ok(0)], vec![]));
        assert_eq!((d.status, d.evidence.clue.as_str()), (Status::Fossil, "empty"));
    }

    #[test]
    fn gone_or_unresolvable_is_dead_at_once() {
        for (kind, clue) in [(ErrorKind::Http(410), "410"), (ErrorKind::Http(404), "404"), (ErrorKind::Dns, "DNS")] {
            let d = diagnose(&ev(vec![failed(0, kind), ok(30)], vec![entry(40)]));
            assert_eq!(d.status, Status::Dead);
            assert_eq!(d.evidence.clue, clue);
            assert_eq!(d.action, Some(Action::Bury));
        }
    }

    #[test]
    fn failure_streak_must_span_fourteen_days() {
        let long: Vec<_> = [0, 5, 10, 15, 20].iter().map(|d| failed(*d, ErrorKind::Timeout)).collect();
        assert_eq!(diagnose(&ev(long, vec![])).status, Status::Dead);
        let short: Vec<_> = [0, 1, 2, 3, 3].iter().map(|d| failed(*d, ErrorKind::Timeout)).collect();
        let d = diagnose(&ev(short, vec![]));
        assert_eq!((d.status, d.evidence.clue.as_str()), (Status::Ailing, "timeout"));
    }

    #[test]
    fn network_suspect_attempts_are_ignored() {
        let mut suspect = failed(0, ErrorKind::Dns);
        suspect.network_suspect = true;
        assert_eq!(diagnose(&ev(vec![suspect, ok(1)], vec![entry(3)])).status, Status::Alive);
    }

    #[test]
    fn permanent_redirect_to_another_feed_is_moved() {
        let mut a = ok(0);
        a.redirect_chain = vec![Hop { status: 301, url: "https://newblog.example.org/rss".into() }];
        let d = diagnose(&ev(vec![a], vec![entry(1)]));
        assert_eq!(d.status, Status::Moved);
        assert_eq!(d.action, Some(Action::FollowRedirect { url: "https://newblog.example.org/rss".into() }));
        assert_eq!(d.evidence.clue, "301");
    }

    #[test]
    fn temporary_redirect_is_not_a_move() {
        let mut a = ok(0);
        a.redirect_chain = vec![Hop { status: 302, url: "https://newblog.example.org/rss".into() }];
        assert_eq!(diagnose(&ev(vec![a], vec![entry(1)])).status, Status::Alive);
    }

    #[test]
    fn http_to_https_redirect_is_a_safe_upgrade_not_a_move() {
        let mut e = ev(vec![], vec![entry(1)]);
        e.xml_url = "http://blog.example.org/feed".into();
        let mut a = ok(0);
        a.redirect_chain = vec![Hop { status: 301, url: "https://blog.example.org/feed".into() }];
        e.attempts = vec![a];
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Alive);
        assert!(d.remarks.contains(&Remark::HttpsAvailable));
        assert!(!d.remarks.contains(&Remark::HttpOnly));
        assert_eq!(d.action, Some(Action::UpgradeHttps { url: "https://blog.example.org/feed".into() }));
    }

    #[test]
    fn probed_https_gives_upgrade_and_unprobed_http_is_remarked() {
        let mut e = ev(vec![ok(0)], vec![entry(1)]);
        e.xml_url = "http://blog.example.org/feed".into();
        assert!(diagnose(&e).remarks.contains(&Remark::HttpOnly));
        e.https_available = Some(true);
        assert_eq!(
            diagnose(&e).action,
            Some(Action::UpgradeHttps { url: "https://blog.example.org/feed".into() })
        );
    }

    #[test]
    fn dead_feed_with_discovered_replacement_is_moved() {
        let mut e = ev(vec![failed(0, ErrorKind::Http(404))], vec![]);
        e.discovered_url = Some("https://blog.example.org/new-feed.xml".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Moved);
        assert_eq!(d.action, Some(Action::AdoptDiscovered { url: "https://blog.example.org/new-feed.xml".into() }));
    }

    #[test]
    fn duplicate_wins_over_everything() {
        let mut e = ev(vec![failed(0, ErrorKind::Http(410))], vec![]);
        e.duplicate_of = Some(41);
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Duplicate);
        assert_eq!(d.evidence.clue, "= #41");
        assert_eq!(d.action, Some(Action::MergeInto { feed_id: 41 }));
    }

    #[test]
    fn hijacked_beats_dead() {
        let mut e = ev(vec![failed(0, ErrorKind::HtmlInsteadOfFeed)], vec![]);
        e.description = Some("Xóc đĩa online là sân chơi mới với cơ hội trúng lớn mỗi ngày.".into());
        let d = diagnose(&e);
        assert_eq!(d.status, Status::Hijacked);
        assert!(d.hijack_score >= HIJACK_THRESHOLD);
        assert_eq!(d.action, Some(Action::Bury));
        assert_eq!(d.evidence.clue, "spam");
    }

    #[test]
    fn server_errors_are_ailing_and_403_is_blocked() {
        assert_eq!(diagnose(&ev(vec![failed(0, ErrorKind::Http(503))], vec![])).status, Status::Ailing);
        let d = diagnose(&ev(vec![failed(0, ErrorKind::Http(403))], vec![]));
        assert_eq!(d.status, Status::Ailing);
        assert!(d.remarks.contains(&Remark::Blocked));
    }

    #[test]
    fn mostly_fallback_dates_are_remarked() {
        let mut e1 = entry(0);
        e1.is_fallback = true;
        let mut e2 = entry(0);
        e2.guid = "other".into();
        e2.is_fallback = true;
        assert!(diagnose(&ev(vec![ok(0)], vec![e1, e2])).remarks.contains(&Remark::DateFallback));
    }
}
```

Run: `cargo test -p ressac-core health::tests`. Expected: compile errors (`diagnose` missing).

- [ ] **Step 2: Implement.** Replace the top of `health/mod.rs` (everything above the tests) with:

```rust
//! Feed diagnosis: pure functions from evidence to a diagnosis. No I/O.

mod cadence;
pub mod hijack;

pub use cadence::cadence;

use chrono::{DateTime, Duration, Utc};

use crate::model::{Action, Cadence, Diagnosis, ErrorKind, Evidence, FeedId, Remark, Status};
use crate::store::{AttemptRecord, EntryRecord};
use crate::url_key::{https_variant, url_key};

pub const ALIVE_WITHIN_DAYS: i64 = 90;
pub const FOSSIL_AFTER_DAYS: i64 = 730;
pub const DEAD_FAILURE_STREAK: usize = 5;
pub const DEAD_STREAK_MIN_DAYS: i64 = 14;
pub const DUPLICATE_OVERLAP: f64 = 0.8;
pub const HIJACK_THRESHOLD: f32 = 0.6;
pub const SIMILAR_TITLE_DISTANCE: usize = 2;

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

struct Verdict {
    status: Status,
    reason: String,
    bullets: Vec<String>,
    clue: String,
    action: Option<Action>,
}

impl Verdict {
    fn new(status: Status, reason: impl Into<String>, clue: impl Into<String>, action: Option<Action>) -> Self {
        let reason = reason.into();
        Verdict { status, bullets: vec![reason.clone()], reason, clue: clue.into(), action }
    }
}

pub fn diagnose(ev: &FeedEvidence) -> Diagnosis {
    let cadence = cadence(&ev.entries, ev.now);
    let hijack = hijack::assess(ev);
    let remarks = remarks(ev);
    let upgrade = remarks
        .contains(&Remark::HttpsAvailable)
        .then(|| Action::UpgradeHttps { url: https_target(ev) });
    let v = verdict(ev, &cadence, &hijack, upgrade);
    let language = (v.status == Status::Hijacked).then(|| hijack.language.clone()).flatten();
    Diagnosis {
        status: v.status,
        reason: v.reason,
        evidence: Evidence { bullets: v.bullets, clue: v.clue, language },
        remarks,
        action: v.action,
        hijack_score: hijack.score,
        cadence,
    }
}

fn last_meaningful(ev: &FeedEvidence) -> Option<&AttemptRecord> {
    ev.attempts.iter().find(|a| !a.network_suspect)
}

fn verdict(ev: &FeedEvidence, cadence: &Cadence, hijack: &hijack::HijackAssessment, upgrade: Option<Action>) -> Verdict {
    if let Some(other) = ev.duplicate_of {
        let mut v = Verdict::new(
            Status::Duplicate,
            format!("Same feed as #{other}."),
            format!("= #{other}"),
            Some(Action::MergeInto { feed_id: other }),
        );
        v.bullets = vec![format!("Its final address or most of its entries match feed #{other}.")];
        return v;
    }
    if hijack.score >= HIJACK_THRESHOLD {
        let clue = hijack.language.as_ref().map_or_else(|| "spam".to_string(), |l| format!("{l} ⚠"));
        let mut v = Verdict::new(
            Status::Hijacked,
            "The site now seems to serve unrelated or spam content.",
            clue,
            Some(Action::Bury),
        );
        v.bullets = hijack.bullets.clone();
        return v;
    }
    let Some(last) = last_meaningful(ev) else {
        return Verdict::new(Status::Unchecked, "Not fetched yet.", "new", None);
    };
    if last.error.is_none()
        && let Some((status, url)) = permanent_redirect(last, &ev.url_key)
    {
        let mut v = Verdict::new(
            Status::Moved,
            "The feed permanently redirects to a new address.",
            status.to_string(),
            Some(Action::FollowRedirect { url: url.clone() }),
        );
        v.bullets.push(format!("{} → {url}", ev.xml_url));
        return v;
    }
    if let Some((reason, clue)) = dead_reason(ev) {
        if let Some(url) = ev.discovered_url.as_ref().filter(|u| url_key(u) != ev.url_key) {
            let mut v = Verdict::new(
                Status::Moved,
                "The feed is gone, but its site now announces another one.",
                "→ new",
                Some(Action::AdoptDiscovered { url: url.clone() }),
            );
            v.bullets = vec![reason, format!("The site links to {url}.")];
            return v;
        }
        return Verdict::new(Status::Dead, reason, clue, Some(Action::Bury));
    }
    if let Some(err) = &last.error {
        return Verdict::new(Status::Ailing, format!("Last fetch failed: {}.", err.label()), err.clue(), upgrade);
    }
    let Some(last_entry) = cadence.last_entry_at else {
        return Verdict::new(Status::Fossil, "The feed responds but has no entries.", "empty", upgrade);
    };
    let days = (ev.now - last_entry).num_days();
    let reason = format!("Last entry on {}.", last_entry.format("%Y-%m-%d"));
    let (status, clue) = if days < ALIVE_WITHIN_DAYS {
        (Status::Alive, format!("{days} d"))
    } else if days < FOSSIL_AFTER_DAYS {
        (Status::Dormant, format!("{} mo", days / 30))
    } else {
        (Status::Fossil, last_entry.format("%Y").to_string())
    };
    let mut v = Verdict::new(status, reason, clue, upgrade);
    v.bullets.push(format!("About {:.1} entries per month over the last year.", cadence.entries_per_month));
    if let Some(median) = cadence.median_interval_days {
        v.bullets.push(format!("Median gap between entries: {median:.0} days."));
    }
    v
}

/// The first permanent hop status and the final URL, when the chain ends on another feed.
fn permanent_redirect(last: &AttemptRecord, own_key: &str) -> Option<(u16, String)> {
    let permanent = last.redirect_chain.iter().find(|h| h.status == 301 || h.status == 308)?;
    let target = &last.redirect_chain.last()?.url;
    (url_key(target) != own_key).then(|| (permanent.status, target.clone()))
}

/// Dead rule (spec §7): 404/410/NXDOMAIN on the last meaningful attempt, or a long
/// enough streak of failures. Returns (reason, clue).
pub fn dead_reason(ev: &FeedEvidence) -> Option<(String, String)> {
    let meaningful: Vec<&AttemptRecord> = ev.attempts.iter().filter(|a| !a.network_suspect).collect();
    let last = meaningful.first()?;
    match &last.error {
        Some(ErrorKind::Http(code @ (404 | 410))) => {
            return Some((format!("The server says the feed is gone (HTTP {code})."), code.to_string()));
        }
        Some(ErrorKind::Dns) => return Some(("The domain no longer resolves.".to_string(), "DNS".to_string())),
        _ => {}
    }
    let streak: Vec<&&AttemptRecord> = meaningful.iter().take_while(|a| a.error.is_some()).collect();
    if streak.len() >= DEAD_FAILURE_STREAK {
        let span = streak[0].at - streak[streak.len() - 1].at;
        if span >= Duration::days(DEAD_STREAK_MIN_DAYS) {
            return Some((
                format!("{} failed fetches in a row over {} days.", streak.len(), span.num_days()),
                format!("{}× fail", streak.len()),
            ));
        }
    }
    None
}

fn remarks(ev: &FeedEvidence) -> Vec<Remark> {
    let mut out = Vec::new();
    let last = last_meaningful(ev);
    if ev.xml_url.starts_with("http://") {
        let redirected_to_https = last
            .filter(|a| a.error.is_none())
            .and_then(|a| a.redirect_chain.last())
            .is_some_and(|h| h.url.starts_with("https://") && url_key(&h.url) == ev.url_key);
        if redirected_to_https || ev.https_available == Some(true) {
            out.push(Remark::HttpsAvailable);
        } else {
            out.push(Remark::HttpOnly);
        }
    }
    if last.is_some_and(|a| a.error == Some(ErrorKind::Http(403))) {
        out.push(Remark::Blocked);
    }
    let fallback = ev.entries.iter().filter(|e| e.is_fallback).count();
    if !ev.entries.is_empty() && fallback * 2 > ev.entries.len() {
        out.push(Remark::DateFallback);
    }
    out
}

fn https_target(ev: &FeedEvidence) -> String {
    last_meaningful(ev)
        .and_then(|a| a.redirect_chain.last())
        .map(|h| h.url.clone())
        .filter(|u| u.starts_with("https://") && url_key(u) == ev.url_key)
        .or_else(|| https_variant(&ev.xml_url))
        .unwrap_or_else(|| ev.xml_url.clone())
}
```

- [ ] **Step 3: Run.** `cargo test -p ressac-core health` → all health tests pass (`4 + 7 + 15`).

- [ ] **Step 4: Verification gate** (fmt, clippy `-D warnings`, test).

- [ ] **Step 5: Commit and push**

```bash
git add crates/ressac-core/src/health
git commit -m "Pure feed diagnosis with statuses, remarks and proposed actions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git -c credential.helper= -c credential.helper='!gh auth git-credential' push
```

**Phase gate:** report progress (the user waived approval between phases on 2026-10-10; continue unless a decision is needed).
