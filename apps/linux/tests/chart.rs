//! The Sea Chart window, offscreen (spec §7.8, §8.3; inventory §6).
#![cfg(target_os = "linux")]

mod common;
use common::*;

/// The facts a `chart` walk step printed after `walk: <tag> ` (`PROBE_SCRIPT`'s line), by key.
fn probe(err: &str, tag: &str) -> std::collections::HashMap<String, String> {
    let prefix = format!("walk: {tag} ");
    let line = err
        .lines()
        .find_map(|l| l.split_once(&prefix).map(|(_, rest)| rest.to_string()))
        .unwrap_or_else(|| panic!("{tag} missing in {err}"));
    let (facts, home) = line.split_once(" home=").unwrap();
    let mut map: std::collections::HashMap<String, String> = facts
        .split(' ')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    map.insert("home".into(), home.to_string());
    map
}
fn number(facts: &std::collections::HashMap<String, String>, key: &str) -> u64 {
    facts[key].parse().unwrap()
}

#[test]
fn the_sea_chart_draws_the_fixture_with_the_stored_prefs_and_answers_the_page() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let db = run.copy_fixture();
    std::fs::create_dir_all(run.home.join("config/ressac")).unwrap();
    std::fs::write(
        run.home.join("config/ressac/ressac-kde.conf"),
        "[General]\nchart.view=themes\nchart.ink=nuit\nchart.animate=true\n",
    )
    .unwrap();
    std::fs::write(
        run.home.join("config/kdeglobals"),
        "[KDE]\nAnimationDurationFactor=0\n",
    )
    .unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../macos/ChartTests/fixture.json"
        ),
        run.home.join("fixture.json"),
    )
    .unwrap();
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "chart")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    assert!(err.contains("walk: 02-open open=true reduce=true"), "{err}");
    assert!(err.contains("walk: 03-wait onScreen=true"), "{err}");
    assert!(err.contains("walk: 11-fixture shown=true"), "{err}");
    let first = probe(&err, "10-first");
    let drawn = probe(&err, "15-drawn");
    assert!(
        number(&first, "colours") > 8,
        "the survey's chart stayed blank: {first:?}"
    );
    assert!(
        number(&drawn, "colours") > 8,
        "the fixture did not draw: {drawn:?}"
    );
    assert_ne!(
        first["print"], drawn["print"],
        "Chart.update did not redraw"
    );
    assert_eq!(
        number(&drawn, "faces"),
        8,
        "every face loaded under the page's CSP: {drawn:?}"
    );
    assert_eq!(
        (drawn["ink"].as_str(), drawn["byTheme"].as_str()),
        ("nuit", "true"),
        "the stored prefs reached Chart.load"
    );
    assert!(
        err.contains("walk: 18-answered selected=feed:8 view=harbour ink=nuit animate=true"),
        "{err}"
    );
    assert!(err.contains("open: https://example.invalid/x"), "{err}");
    assert!(
        !err.contains("open: file:") && !err.contains("open: javascript:"),
        "{err}"
    );
    let home = probe(&err, "23-home");
    assert_eq!(
        home["home"], "object ressac:chart/index.html",
        "the page came back: {home:?}"
    );
    assert!(number(&home, "colours") > 8, "and drew again: {home:?}");
    assert!(
        err.contains("walk: 25-closed open=false onScreen=false window=false"),
        "{err}"
    );
    assert_drawn_min(&run.shots.join("15-drawn.png"), 640, 360);
    let conf = std::fs::read_to_string(run.home.join("config/ressac/ressac-kde.conf")).unwrap();
    assert!(
        conf.contains("chart.view=harbour") && conf.contains("chart.animate=true"),
        "the page's choice, the stored animate kept: {conf}"
    );
    assert!(
        conf.contains("chart.window.width=1200") && conf.contains("chart.window.height=720"),
        "the size is remembered: {conf}"
    );
}

#[test]
fn the_sea_chart_follows_the_database_and_stops_when_closed() {
    let _turn = one_app_at_a_time();
    let run = Run::new();
    let db = run.copy_fixture();
    let out = run.launch_with(
        Some(&db),
        &[("TZ", "UTC"), ("RESSAC_RECETTE_WALK", "chart-live")],
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_clean(&out);
    // The 3 s data_version poll may note the same write once more: counts are compared, not pinned.
    let count = |tag: &str| -> u64 {
        let prefix = format!("walk: {tag} surveys=");
        err.lines()
            .find_map(|l| l.split_once(&prefix))
            .map(|(_, rest)| rest.split(' ').next().unwrap().parse().unwrap())
            .unwrap_or_else(|| panic!("{tag} missing in {err}"))
    };
    assert!(
        err.contains("walk: 06-first surveys=1 phase=ready"),
        "drawn once on open: {err}"
    );
    let second = count("15-second");
    assert!(
        second >= 2,
        "a write redrew the chart within the 2 s window: {err}"
    );
    assert!(count("18-third") > second, "Retry surveys again: {err}");
    assert_eq!(
        count("28-after"),
        count("20-note"),
        "nothing runs once the window is closed: {err}"
    );
    assert!(err.contains("open=false onScreen=false"), "{err}");
}
