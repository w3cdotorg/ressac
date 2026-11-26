//! Logging set-up shared by every binary: both call [`init`] once at start.
//!
//! A daily-rotated log file in the state directory (`Paths::log_dir`: `~/Library/Logs/Ressac`
//! on macOS, `$XDG_STATE_HOME/ressac/logs` on Linux), mirrored to stderr with `verbose`.

use std::path::Path;

pub use tracing_appender::non_blocking::WorkerGuard;

/// Installs the global subscriber, logging to `dir`; keep the returned guard alive for the
/// whole run (dropping it flushes the file). `None` when the directory cannot be created or
/// written, or a subscriber is already installed: logging is then off, never fatal.
pub fn init(dir: &Path, verbose: bool) -> Option<WorkerGuard> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    std::fs::create_dir_all(dir).ok()?;
    // `rolling::daily` panics when the directory is not writable; a status-bar helper must
    // never print that, so no file logging in that case.
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ressac.log")
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let file_layer = fmt::layer().with_writer(writer).with_ansi(false);
    let stderr_layer = verbose.then(|| fmt::layer().with_writer(std::io::stderr));
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .ok()?;
    Some(guard)
}

/// An address as the log may show it: scheme, host (port) and path. The userinfo, the query and the
/// fragment can carry a secret (`user:pass@host`, `?token=...`), so they are dropped, and `?...`
/// marks a query that was there. Text that is not an http(s) address is cut the same way by hand.
pub fn redact_address(address: &str) -> String {
    let rest_start = address.find("://").map_or(0, |i| i + 3);
    let (head, rest) = address.split_at(rest_start);
    // Userinfo first: up to the last `@` of the authority, even past a `?` or `#` in a password.
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let rest = match rest[..authority_end].rfind('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let marker = if rest[end..].starts_with('?') {
        "?..."
    } else {
        ""
    };
    format!("{head}{}{marker}", &rest[..end])
}

/// Every web address inside a sentence, redacted as [`redact_address`] does; the rest of the
/// text is left as it is. An address runs to the next whitespace, and a query or fragment ends it.
pub fn redact_text(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|word| match find_scheme(word) {
            Some(at) => {
                let body = word[at..].trim_end_matches(char::is_whitespace);
                format!(
                    "{}{}{}",
                    &word[..at],
                    redact_address(body),
                    &word[at + body.len()..]
                )
            }
            None => word.to_string(),
        })
        .collect()
}

/// Where `http://` or `https://` starts in `word`, ASCII case-insensitively.
fn find_scheme(word: &str) -> Option<usize> {
    let bytes = word.as_bytes();
    (0..bytes.len()).find(|&i| {
        let rest = &bytes[i..];
        rest.len() >= 7
            && (rest[..7].eq_ignore_ascii_case(b"http://")
                || (rest.len() >= 8 && rest[..8].eq_ignore_ascii_case(b"https://")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_address_drops_userinfo_query_and_fragment() {
        assert_eq!(
            redact_address("https://user:pass@host.example:8080/feed.xml?token=abc#top"),
            "https://host.example:8080/feed.xml?..."
        );
        assert_eq!(
            redact_address("https://u:p@host.example?token=abc"),
            "https://host.example?..."
        );
        assert_eq!(
            redact_address("https://host.example/a/b.xml"),
            "https://host.example/a/b.xml"
        );
        assert_eq!(
            redact_address("https://host.example/@me/feed"),
            "https://host.example/@me/feed"
        );
        assert_eq!(
            redact_address("host.example/feed#frag"),
            "host.example/feed"
        );
        assert_eq!(
            redact_address("u:p@host.example/feed?k=v"),
            "host.example/feed?..."
        );
        assert_eq!(
            redact_address("https://u:pa?ss@host.example/f"),
            "https://host.example/f"
        );
        assert_eq!(redact_address("u:pa?ss@host/f"), "host/f");
        assert_eq!(redact_address("intranet?token=abc"), "intranet?...");
        assert_eq!(redact_address("u:p@[::1]:8080/feed"), "[::1]:8080/feed");
    }

    #[test]
    fn redact_text_redacts_every_web_address_in_a_sentence() {
        // The query ends the address; the sentence goes on after the word.
        assert_eq!(
            redact_text("Could not reach https://host.example/f?k=v: timed out."),
            "Could not reach https://host.example/f?... timed out."
        );
        assert_eq!(
            redact_text("Could not reach https://u:p@host.example/feed: refused."),
            "Could not reach https://host.example/feed: refused."
        );
        assert_eq!(
            redact_text("from HTTPS://u:p@h.example/x to http://h.example/y?z"),
            "from HTTPS://h.example/x to http://h.example/y?..."
        );
        for plain in [
            "Already followed: xkcd.com.",
            "No folder #3.",
            "Enter a web address.",
            "feed theme 7 ai-labs a@b?c",
            "",
        ] {
            assert_eq!(redact_text(plain), plain);
        }
    }
}
