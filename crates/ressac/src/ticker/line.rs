//! The ticker line and the click token.

use ressac_core::store::TickerEntry;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Default `--width`, in display columns.
pub const DEFAULT_WIDTH: usize = 70;

const SEPARATOR: &str = " · ";
const ELLIPSIS: char = '…';
/// `rs` plus up to 13 digits: tmux range arguments are limited to 15 bytes.
const TOKEN_MAX_BYTES: usize = 15;

/// Formats `<feed> · <title>` within `width` display columns. With `tmux`, the
/// text is escaped for tmux's format parser and wrapped in a clickable range.
pub fn line(entry: &TickerEntry, width: usize, tmux: bool) -> String {
    let feed = cut(&strip_controls(&entry.feed_title), width / 2);
    let budget = width.saturating_sub(feed.width() + SEPARATOR.width());
    let title = (budget > 0).then(|| cut(&strip_controls(&entry.title), budget));
    if !tmux {
        return match title {
            Some(title) => format!("{feed}{SEPARATOR}{title}"),
            None => feed,
        };
    }
    let mut out = format!(
        "#[range=user|{}]#[bold]{}#[nobold]",
        token(entry.entry_id),
        escape(&feed)
    );
    if let Some(title) = title {
        out.push_str(SEPARATOR);
        out.push_str(&escape(&title));
    }
    out.push_str("#[norange]");
    out
}

/// Whether the title still shows something once control characters and Unicode whitespace
/// (tabs, newlines, NBSP: SQLite's `trim` only strips spaces) are removed.
pub fn has_visible_title(entry: &TickerEntry) -> bool {
    !strip_controls(&entry.title).trim().is_empty()
}

/// The tmux range argument that identifies an entry: `rs{id}`.
pub fn token(entry_id: i64) -> String {
    format!("rs{entry_id}")
}

/// Inverse of [`token`]: `rs` and ASCII digits, at most 15 bytes, id above 0.
pub fn parse_token(token: &str) -> Option<i64> {
    if token.len() > TOKEN_MAX_BYTES {
        return None;
    }
    let digits = token.strip_prefix("rs")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|id| *id > 0)
}

fn strip_controls(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// tmux expands `#[...]`, `#(...)` and `#{...}`; `##` is a literal `#`.
fn escape(text: &str) -> String {
    text.replace('#', "##")
}

/// Cuts `text` to at most `max` display columns, ending with `…` when cut.
fn cut(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    let Some(room) = max.checked_sub(ELLIPSIS.width().unwrap_or(1)) else {
        return String::new();
    };
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > room {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, feed: &str, title: &str) -> TickerEntry {
        TickerEntry {
            entry_id: id,
            feed_id: 1,
            feed_title: feed.to_string(),
            title: title.to_string(),
            url: "https://example.org/a".to_string(),
        }
    }

    /// The line without tmux markup, `##` read as one `#`.
    fn visible(tmux_line: &str) -> String {
        tmux_line
            .replace("#[bold]", "")
            .replace("#[nobold]", "")
            .replace("#[norange]", "")
            .split("#[range=user|")
            .enumerate()
            .map(|(i, part)| {
                if i == 0 {
                    part.to_string()
                } else {
                    part.split_once(']').unwrap().1.to_string()
                }
            })
            .collect::<String>()
            .replace("##", "#")
    }

    #[test]
    fn a_title_blank_after_cleaning_is_not_visible() {
        for blank in ["", "   ", "\t\n", "\u{a0}", "\u{7}\u{200b}\u{2003}"] {
            // U+200B is not whitespace for `trim`: only the ones that are must be rejected.
            let visible = has_visible_title(&entry(1, "Feed", blank));
            assert_eq!(visible, blank.contains('\u{200b}'), "{blank:?}");
        }
        assert!(has_visible_title(&entry(1, "Feed", " a ")));
    }

    #[test]
    fn plain_line() {
        let e = entry(1, "Libération", "Le budget 2027 adopté");
        assert_eq!(line(&e, 70, false), "Libération · Le budget 2027 adopté");
    }

    #[test]
    fn tmux_line_wraps_in_a_range() {
        let e = entry(4521, "Libération", "Le budget 2027 adopté");
        assert_eq!(
            line(&e, 70, true),
            "#[range=user|rs4521]#[bold]Libération#[nobold] · Le budget 2027 adopté#[norange]"
        );
    }

    #[test]
    fn escapes_tmux_formats() {
        let e = entry(7, "Feed", "50#[fg=red] #(rm -rf ~) #{pane_id}");
        let out = line(&e, 70, true);
        assert!(
            out.contains("50##[fg=red] ##(rm -rf ~) ##{pane_id}"),
            "{out}"
        );
        let stripped = out
            .replace("#[range=user|rs7]", "")
            .replace("#[bold]", "")
            .replace("#[nobold]", "")
            .replace("#[norange]", "");
        assert_eq!(stripped.replace("##", "").matches('#').count(), 0, "{out}");
    }

    #[test]
    fn escapes_hostile_feed_title() {
        let e = entry(7, "#(evil)", "t");
        let out = line(&e, 70, true);
        assert!(out.contains("##(evil)"), "{out}");
    }

    #[test]
    fn plain_line_does_not_escape() {
        let e = entry(1, "F", "a #b");
        assert_eq!(line(&e, 70, false), "F · a #b");
    }

    #[test]
    fn controls_are_removed() {
        let e = entry(1, "F", "a\u{1b}[31mb\nc\td");
        assert_eq!(visible(&line(&e, 70, true)), "F · a[31mbcd");
        assert_eq!(line(&e, 70, false), "F · a[31mbcd");
    }

    #[test]
    fn cuts_by_display_width() {
        let e = entry(1, "Feed", &"x".repeat(100));
        let v = visible(&line(&e, 70, true));
        assert_eq!(v.width(), 70);
        assert!(v.ends_with('…'));

        for title in ["日本語のニュース".repeat(10), "🦀 rust ".repeat(30)] {
            let e = entry(1, "Feed", &title);
            let v = visible(&line(&e, 70, true));
            assert!(v.width() <= 70, "{v}");
            assert!(v.ends_with('…'), "{v}");
        }
    }

    #[test]
    fn hash_counts_as_one_column() {
        let e = entry(1, "Feed", &"#".repeat(100));
        let v = visible(&line(&e, 70, true));
        assert_eq!(v.width(), 70);
    }

    #[test]
    fn long_feed_title_is_cut_to_half() {
        let e = entry(1, &"f".repeat(50), "title");
        let v = visible(&line(&e, 70, true));
        let feed = v.split(" · ").next().unwrap();
        assert!(feed.width() <= 35, "{feed}");
        assert!(feed.ends_with('…'));
    }

    #[test]
    fn tiny_width_never_overflows() {
        let e = entry(1, "Feed", "title");
        for width in 0..8 {
            let v = visible(&line(&e, width, true));
            assert!(v.width() <= width, "{width}: {v}");
        }
    }

    #[test]
    fn token_round_trip() {
        assert_eq!(parse_token(&token(4521)), Some(4521));
        for bad in [
            "rs",
            "rs-1",
            "rs0",
            "rx12",
            "rs12a",
            "rs12345678901234",
            "rs+1",
            "",
        ] {
            assert_eq!(parse_token(bad), None, "{bad}");
        }
        assert_eq!(parse_token("rs1234567890123"), Some(1_234_567_890_123));
        assert_eq!(token(9_999_999_999_999).len(), 15);
    }
}
