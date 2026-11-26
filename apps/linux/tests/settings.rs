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

/// A FormLayout centres its content and sizes it by its widest field, and the Preview text was that
/// field: at small sizes it narrowed, the form re-centred and the whole row jumped, under the cursor.
#[test]
fn the_text_size_row_stays_put_whatever_the_size() {
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(Some(&db), &[("RESSAC_RECETTE_WALK", "settings-size")]);
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
    // No tick per point on the Native slider (the Mac shows none: inventory §7.3); Antiope's four sizes are ticked.
    assert!(native.iter().all(|l| field(l, "ticks") < 0), "{native:#?}");
    assert!(
        antiope.iter().all(|l| field(l, "ticks") == 1),
        "{antiope:#?}"
    );
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
