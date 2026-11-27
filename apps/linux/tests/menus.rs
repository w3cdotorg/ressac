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
fn the_sea_chart_window_has_the_macs_undo_and_redo_in_an_edit_menu() {
    // the Mac's Undo Chart Edit (⌘Z) and Redo Chart Edit (⇧⌘Z), in an Edit menu as MainMenu builds them
    assert!(CHART.contains("menuBar: QQC2.MenuBar {"));
    assert!(CHART.contains(r#"title: "&Edit""#));
    for (title, shortcut, page, native) in [
        (
            "Undo Chart Edit",
            "shortcut: StandardKey.Undo",
            "Chart.undo()",
            "WebEngineView.Undo",
        ),
        (
            "Redo Chart Edit",
            r#"shortcut: "Ctrl+Shift+Z""#,
            "Chart.redo()",
            "WebEngineView.Redo",
        ),
    ] {
        let at = CHART
            .find(&format!("text: \"{title}\""))
            .unwrap_or_else(|| panic!("{title}"));
        let action = &CHART[at..at + CHART[at..].find("\n    }").unwrap()];
        assert!(action.contains(shortcut), "{title}: {shortcut}");
        // the page first, then the focused text field's own undo
        let (first, then) = (action.find(page), action.find(native));
        assert!(
            first.is_some() && then.is_some() && first < then,
            "{title}: {action}"
        );
    }
    let items: Vec<&str> = CHART
        .lines()
        .filter_map(|l| l.trim().strip_prefix("QQC2.MenuItem { action: "))
        .collect();
    assert_eq!(items, ["undoChartEdit }", "redoChartEdit }"]);
    // one path only: no stray Shortcut left beside the actions
    assert!(!CHART.contains("Shortcut {"));
}
