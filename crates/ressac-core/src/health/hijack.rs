//! Hijack scoring: signs that a domain now serves unrelated or spam content.

use crate::health::FeedEvidence;
use crate::model::{ErrorKind, Hop};
use crate::opml::clean_description;
use crate::store::EntryRecord;
use crate::url_key::site_of;

/// Lowercase phrases typical of spam that takes over expired blog domains.
pub const SPAM_LEXICON: &[&str] = &[
    "casino",
    "slot online",
    "slots",
    "betting",
    "sportsbook",
    "poker online",
    "payday loan",
    "viagra",
    "cialis",
    "escort",
    "xóc đĩa",
    "trúng lớn",
    "nhà cái",
    "cá cược",
    "nổ hũ",
    "judi online",
    "togel",
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
    SPAM_LEXICON
        .iter()
        .copied()
        .filter(|w| lower.contains(w))
        .collect()
}

fn detect(text: &str) -> Option<(String, f64)> {
    if text.chars().count() < MIN_DETECT_CHARS {
        return None;
    }
    let info = whatlang::detect(text)?;
    (info.confidence() >= MIN_DETECT_CONFIDENCE)
        .then(|| (info.lang().code().to_string(), info.confidence()))
}

fn entry_text(e: &EntryRecord) -> String {
    let summary = e
        .summary_html
        .as_deref()
        .and_then(clean_description)
        .unwrap_or_default();
    format!("{} {summary}", e.title.as_deref().unwrap_or_default())
}

fn joined<'a>(entries: impl Iterator<Item = &'a EntryRecord>) -> String {
    entries.map(entry_text).collect::<Vec<_>>().join(" ")
}

pub fn assess(ev: &FeedEvidence) -> HijackAssessment {
    let mut out = HijackAssessment::default();
    let recent = || ev.entries.iter().take(RECENT_ENTRIES);
    let older = || ev.entries.iter().skip(RECENT_ENTRIES);

    let haystack = format!(
        "{} {} {}",
        ev.title,
        ev.description.as_deref().unwrap_or_default(),
        joined(recent())
    );
    let hits = spam_hits(&haystack);
    if !hits.is_empty() {
        out.score += W_LEXICON;
        let quoted: Vec<String> = hits.iter().map(|h| format!("\"{h}\"")).collect();
        out.bullets
            .push(format!("Spam vocabulary: {}.", quoted.join(", ")));
    }

    let has_older = older().next().is_some();
    let reference = if has_older {
        format!("{} {}", ev.title, joined(older()))
    } else {
        format!("{} {}", ev.title, joined(ev.entries.iter()))
    };
    if let (Some((desc_lang, conf)), Some((ref_lang, _))) = (
        ev.description.as_deref().and_then(detect),
        detect(&reference),
    ) && desc_lang != ref_lang
    {
        out.score += W_DESCRIPTION_LANGUAGE;
        out.bullets.push(format!(
            "Description is in {desc_lang} ({conf:.2}) while the feed is in {ref_lang}."
        ));
        out.language = Some(desc_lang);
    }

    if has_older
        && let (Some((new_lang, _)), Some((old_lang, _))) =
            (detect(&joined(recent())), detect(&joined(older())))
        && new_lang != old_lang
    {
        out.score += W_ENTRIES_LANGUAGE;
        out.bullets.push(format!(
            "Recent entries are in {new_lang}, older ones in {old_lang}."
        ));
    }

    if let Some(last) = ev.attempts.iter().find(|a| !a.network_suspect) {
        if let Some(Hop { url, .. }) = last.redirect_chain.last()
            && site_of(url) != site_of(&ev.xml_url)
        {
            out.score += W_CROSS_DOMAIN;
            out.bullets
                .push(format!("Redirects to another domain: {url}."));
        }
        if last.error == Some(ErrorKind::HtmlInsteadOfFeed) {
            out.score += W_HTML;
            out.bullets
                .push("The feed address now returns an HTML page.".to_string());
        }
    }
    out.score = out.score.min(1.0);
    out
}

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
        e.entries = EN
            .iter()
            .enumerate()
            .map(|(i, t)| entry(i as i64, t))
            .collect();
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
        e.entries = EN
            .iter()
            .enumerate()
            .map(|(i, t)| entry(i as i64, t))
            .collect();
        let a = assess(&e);
        assert!((a.score - 0.8).abs() < 1e-6, "{}", a.score);
        assert_eq!(a.language.as_deref(), Some("vie"));
    }

    #[test]
    fn recent_entries_switching_language_is_a_signal() {
        let mut e = ev();
        let mut entries: Vec<EntryRecord> = VI
            .iter()
            .enumerate()
            .map(|(i, t)| entry(i as i64, t))
            .collect();
        entries.extend(EN.iter().enumerate().map(|(i, t)| entry(100 + i as i64, t)));
        e.entries = entries;
        let a = assess(&e);
        assert!(
            a.bullets.iter().any(|b| b.contains("Recent entries")),
            "{:?}",
            a.bullets
        );
        assert!(
            a.score >= 0.8,
            "lexicon (cá cược, nhà cái) + language switch: {}",
            a.score
        );
    }

    #[test]
    fn cross_domain_redirect_adds_weight_and_score_is_capped() {
        let mut e = ev();
        e.description = Some(VI_SPAM.into());
        e.entries = EN
            .iter()
            .enumerate()
            .map(|(i, t)| entry(i as i64, t))
            .collect();
        e.attempts = vec![attempt(
            Some(ErrorKind::HtmlInsteadOfFeed),
            vec![Hop {
                status: 302,
                url: "https://casino.example.com/".into(),
            }],
        )];
        assert_eq!(assess(&e).score, 1.0);
    }

    #[test]
    fn has_spam_is_case_insensitive() {
        assert!(has_spam("Best CASINO bonuses"));
        assert!(!has_spam("A blog about libraries"));
    }
}
