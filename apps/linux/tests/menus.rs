//! The menu bar is spec §7.2's table (read from the QML file, on every platform).
const MENU: &str = include_str!("../qml/MainMenu.qml");
const GARDEN: &str = include_str!("../qml/GardenPane.qml");
const CHART: &str = include_str!("../qml/SeaChartWindow.qml");

#[test]
fn the_shortcuts_are_the_spec_table() {
    for want in [
        r#"shortcut: "Ctrl+N""#,
        r#"shortcut: "Ctrl+O""#,
        r#"shortcut: "Ctrl+Shift+E""#,
        r#"shortcut: "Ctrl+R""#,
        r#"shortcut: "Ctrl+Shift+R""#,
        r#"sequence: "F5""#,
        "shortcut: StandardKey.Find",
        r#"shortcut: "Ctrl+Alt+U""#,
        "shortcut: StandardKey.ZoomIn",
        "shortcut: StandardKey.ZoomOut",
        r#"shortcut: "Ctrl+0""#,
        r#"shortcut: "Ctrl+Shift+M""#,
        r#"shortcut: "Ctrl+Shift+A""#,
        r#"shortcut: "Ctrl+Return""#,
        r#"shortcut: "Ctrl+Shift+,""#,
        "shortcut: StandardKey.Quit",
        r#"shortcut: "J""#,
        r#"shortcut: "K""#,
        r#"shortcut: "Space""#,
        r#"shortcut: "M""#,
        r#"shortcut: "S""#,
    ] {
        assert!(MENU.contains(want), "{want}");
    }
    for title in [
        "Add Feed…",
        "Import OPML…",
        "Export OPML…",
        "Refresh All Feeds",
        "Settings…",
        "Quit Ressac",
        "Search",
        "Show Unread Only",
        "Bigger Text",
        "Smaller Text",
        "Actual Size",
        "Sea Chart",
        "Next Entry",
        "Previous Entry",
        "Scroll Down or Next Unread",
        "Mark as Read",
        "Mark as Unread",
        "Star",
        "Unstar",
        "Mark All as Read",
        "Full Article",
        "Open in Browser",
    ] {
        assert!(MENU.contains(&format!("\"{title}\"")), "{title}");
    }
}

#[test]
fn menu_mnemonics_never_use_s_g_or_r() {
    let titles: Vec<&str> = MENU
        .lines()
        .filter_map(|l| l.trim().strip_prefix("title: \""))
        .map(|t| t.trim_end_matches('"'))
        .collect();
    assert_eq!(titles, ["&File", "&Edit", "&View", "G&o", "&Article"]);
    for title in titles {
        let mnemonic = title
            .split('&')
            .nth(1)
            .and_then(|r| r.chars().next())
            .unwrap()
            .to_ascii_lowercase();
        assert!(!['s', 'g', 'r'].contains(&mnemonic), "{title}");
    }
}

#[test]
fn the_garden_undo_is_ctrl_z_and_its_tooltips_name_kde_keys() {
    assert!(GARDEN.contains("shortcut: StandardKey.Undo"));
    assert!(
        GARDEN
            .contains("Apply every safe repair: https upgrades and permanent redirects (Shift+R)")
    );
    assert!(
        GARDEN.contains("Undo the last decision, here or in the terminal garden (U or Ctrl+Z)")
    );
}

#[test]
fn the_sea_chart_window_has_no_menu_bar_and_keeps_ctrl_z_and_ctrl_shift_z() {
    // the page's toolbar carries Undo and Redo; the window reserves no menu bar row
    assert!(!CHART.contains("menuBar"));
    assert!(!CHART.contains("MenuBar"));
    assert!(!CHART.contains("QQC2.Action"));
    for (shortcut, page, native) in [
        (
            "sequences: [StandardKey.Undo]",
            "Chart.undo()",
            "WebEngineView.Undo",
        ),
        (
            r#"sequences: ["Ctrl+Shift+Z"]"#,
            "Chart.redo()",
            "WebEngineView.Redo",
        ),
    ] {
        let at = CHART.find(shortcut).unwrap_or_else(|| panic!("{shortcut}"));
        let shortcut_item = &CHART[at..at + CHART[at..].find("\n    }").unwrap()];
        // the page first, then the focused text field's own undo
        let (first, then) = (shortcut_item.find(page), shortcut_item.find(native));
        assert!(
            first.is_some() && then.is_some() && first < then,
            "{shortcut}: {shortcut_item}"
        );
    }
}
