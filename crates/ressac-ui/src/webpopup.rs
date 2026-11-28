//! Qt WebEngine's `<select>` drop-downs on Wayland (recette fix of the territory editor).
//!
//! Qt WebEngine 6.11 shows a `<select>`'s list in a window of its own, flagged `Qt::Popup` and
//! `Qt::WindowDoesNotAcceptFocus`. On X11 that window never takes the focus. On Wayland it is an
//! `xdg_popup` with a grab, and KWin moves the keyboard focus to it: Qt then focuses the popup
//! window's content item again, which sends the popup's own web view delegate a `FocusOut`
//! (`Qt::OtherFocusReason` or `Qt::ActiveWindowFocusReason`). Qt WebEngine closes a popup whose
//! view loses the focus, so the list opens and closes within about 50 ms (seen on Fedora 45,
//! Plasma 6.8 Wayland, Qt 6.11.2, in a bare `WebEngineView` window as well as in the Sea Chart).
//!
//! The KDE app's application-wide filter (`cpp/shim.cpp`) asks [`drops_focus_out`] about every
//! `FocusOut` sent to an item, and drops the ones this rule names. A popup still closes when an
//! option is picked, on Esc, on a click outside it or when the compositor dismisses it: Qt WebEngine
//! closes it then through its own hide path, not through a focus event.

/// `Qt::PopupFocusReason`: the reason Qt gives a window whose focus went to a popup; Qt WebEngine
/// already ignores it.
pub const POPUP_FOCUS_REASON: i32 = 4;

/// The class name every Qt WebEngine view delegate item starts with (`QMetaObject::className`).
pub const DELEGATE_CLASS: &str = "QtWebEngineCore::RenderWidgetHostViewQtDelegate";

/// Whether a `FocusOut` (`reason`, a `Qt::FocusReason`) sent to an item of class `item_class`
/// is the spurious one: the item is a Qt WebEngine view in a `Qt::Popup` window
/// (`window_is_popup`) that holds the application's focus (`window_focused`), so nothing has
/// taken its focus away.
pub fn drops_focus_out(
    item_class: &str,
    reason: i32,
    window_is_popup: bool,
    window_focused: bool,
) -> bool {
    item_class.starts_with(DELEGATE_CLASS)
        && window_is_popup
        && window_focused
        && reason != POPUP_FOCUS_REASON
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str = "QtWebEngineCore::RenderWidgetHostViewQtDelegateItem";
    const OTHER: i32 = 7; // Qt::OtherFocusReason
    const ACTIVE_WINDOW: i32 = 3; // Qt::ActiveWindowFocusReason

    #[test]
    fn the_popup_view_refocused_in_its_own_focused_window_keeps_its_list() {
        // what the VM trace showed: the popup window took the focus, its view got FocusOut (7)
        assert!(drops_focus_out(ITEM, OTHER, true, true));
        assert!(drops_focus_out(ITEM, ACTIVE_WINDOW, true, true));
    }

    #[test]
    fn a_real_loss_of_focus_still_closes_the_list() {
        // another window took the focus (a click in another app): the popup window is not focused
        assert!(!drops_focus_out(ITEM, ACTIVE_WINDOW, true, false));
        assert!(!drops_focus_out(ITEM, OTHER, true, false));
    }

    #[test]
    fn the_page_view_and_other_items_are_left_alone() {
        // the chart's own view, in its ordinary window: its focus events go through
        assert!(!drops_focus_out(ITEM, OTHER, false, true));
        assert!(!drops_focus_out(ITEM, ACTIVE_WINDOW, false, false));
        // the popup reason is already ignored by Qt WebEngine
        assert!(!drops_focus_out(ITEM, POPUP_FOCUS_REASON, true, true));
        // a Qt Quick control in a popup window (a Kirigami menu) is not a web view
        assert!(!drops_focus_out("QQuickTextField", OTHER, true, true));
        assert!(!drops_focus_out("", OTHER, true, true));
    }
}
