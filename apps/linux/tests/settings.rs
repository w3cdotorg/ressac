//! The Settings window, offscreen (recette fix 2): the Text size row does not move with the size.
#![cfg(target_os = "linux")]

mod common;
use common::*;

fn field(line: &str, key: &str) -> i64 {
    let rest = line
        .split(&format!(" {key}="))
        .nth(1)
        .unwrap_or_else(|| panic!("{key} missing in {line}"));
    rest.split(' ').next().unwrap().parse().unwrap()
}

fn word<'a>(line: &'a str, key: &str) -> &'a str {
    line.split(&format!(" {key}="))
        .nth(1)
        .unwrap_or_else(|| panic!("{key} missing in {line}"))
        .split(' ')
        .next()
        .unwrap()
}

/// A FormLayout centres its content and sizes it by its widest field, and the Preview text was that
/// field: at small sizes it narrowed, the form re-centred and the whole row jumped, under the cursor.
#[test]
fn the_text_size_row_stays_put_whatever_the_size() {
    size_row_at("1", "settings-size");
}

/// The VM runs at scale 2: the fixed-width preview must still wrap inside the page.
#[test]
fn the_text_size_row_stays_put_at_scale_two() {
    size_row_at("2", "settings-size");
}

/// Scale 2 doubles Kirigami's grid unit, so the walk doubles the preview and slider widths (QT_SCALE_FACTOR
/// alone leaves them as they are offscreen): the preview still wraps inside the page.
#[test]
fn the_text_size_row_stays_put_with_double_grid_unit_widths() {
    let _turn = one_app_at_a_time();
    size_row_at("2", "settings-size-x2");
}

/// The walk opens Settings at its minimum width and logs the row at each size.
fn size_row_at(scale: &str, walk: &str) {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(
        Some(&db),
        &[("RESSAC_RECETTE_WALK", walk), ("QT_SCALE_FACTOR", scale)],
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    let native: Vec<&str> = ["03-n12", "05-n20", "07-n28"]
        .iter()
        .map(|t| walk_line(&err, &format!("walk: {t} ")))
        .collect();
    let antiope: Vec<&str> = ["09-a15", "11-a30"]
        .iter()
        .map(|t| walk_line(&err, &format!("walk: {t} ")))
        .collect();
    assert_eq!(
        native.iter().map(|l| field(l, "size")).collect::<Vec<_>>(),
        [12, 20, 28],
        "{err}"
    );
    // With Settings in front and the main window minimised, the list keeps polling: `Live.windowActive`
    // follows the application, as the Mac's `NSApp.isActive` does, not the main window alone.
    let away = walk_line(&err, "walk: 13-away ");
    assert_eq!(word(away, "mainState"), "3", "{away}"); // Window.Minimized
    assert_eq!(word(away, "app"), "true", "{away}");
    assert_eq!(word(away, "windowActive"), "true", "{away}");
    // No tick per point on the Native slider (the Mac's Slider has none in either look: SettingsView.swift).
    assert!(native.iter().all(|l| field(l, "ticks") < 0), "{native:#?}");
    assert!(
        antiope.iter().all(|l| field(l, "ticks") < 0),
        "{antiope:#?}"
    );
    // The whole preview shows at 28 pt (Antiope at 30 pt included), inside the window, never widening it.
    for line in native.iter().chain(&antiope) {
        assert_eq!(word(line, "truncated"), "false", "{line}");
        assert!(
            field(line, "right") <= field(line, "winW"),
            "preview overflows the window: {line}"
        );
        assert_eq!(field(line, "winW"), 540, "the window widened: {line}");
    }
    for group in [&native, &antiope] {
        let xs: Vec<i64> = group.iter().map(|l| field(l, "sliderX")).collect();
        let ws: Vec<i64> = group.iter().map(|l| field(l, "sliderW")).collect();
        assert!(
            xs.windows(2).all(|p| p[0] == p[1]),
            "slider x moved: {xs:?}\n{group:#?}"
        );
        assert!(
            ws.windows(2).all(|p| p[0] == p[1]),
            "slider width changed: {ws:?}\n{group:#?}"
        );
        let pw: Vec<i64> = group.iter().map(|l| field(l, "previewW")).collect();
        assert!(
            pw.windows(2).all(|p| p[0] == p[1]),
            "preview width changed: {pw:?}\n{group:#?}"
        );
    }
}
