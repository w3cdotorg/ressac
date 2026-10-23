import QtQuick
import QtQuick.Controls as QQC2
import org.w3cdotorg.ressac

// The menu bar and the shortcuts table of spec §7.2. `page` is the ReadingPage (null until the database is
// ready: every item but Quit is then disabled). Mnemonics are never S, G or R (they would clash with the
// single-key shortcuts); the window's key filter (cpp/shim.cpp) takes letters and Space before any shortcut
// here, and `letters` switches them off while the search field is typing.
QQC2.MenuBar {
    id: bar

    property var page: null
    readonly property bool ready: page !== null
    readonly property bool letters: ready && !page.searchFocused

    readonly property alias addFeed: addFeed
    readonly property alias importOpml: importOpml
    readonly property alias exportOpml: exportOpml
    readonly property alias refresh: refresh
    readonly property alias refreshAll: refreshAll
    readonly property alias settings: settings
    readonly property alias quit: quit
    readonly property alias search: search
    readonly property alias unreadOnly: unreadOnly
    readonly property alias biggerText: biggerText
    readonly property alias smallerText: smallerText
    readonly property alias actualSize: actualSize
    readonly property alias seaChart: seaChart
    readonly property alias nextEntry: nextEntry
    readonly property alias previousEntry: previousEntry
    readonly property alias advance: advance
    readonly property alias toggleRead: toggleRead
    readonly property alias toggleStar: toggleStar
    readonly property alias markAllRead: markAllRead
    readonly property alias fullArticle: fullArticle
    readonly property alias openInBrowser: openInBrowser

    function menuFor(name) {
        return { file: fileMenu, edit: editMenu, view: viewMenu, go: goMenu, article: articleMenu }[name]
    }

    function openMenu(name) {
        const menu = menuFor(name)
        const index = [fileMenu, editMenu, viewMenu, goMenu, articleMenu].indexOf(menu)
        if (menu === undefined || index < 0)
            return
        closeMenus()
        const item = bar.itemAt(index)
        menu.popup(item, 0, item.height)
    }

    function closeMenus() {
        for (const menu of [fileMenu, editMenu, viewMenu, goMenu, articleMenu])
            menu.close()
    }

    QQC2.Action { id: addFeed; text: "Add Feed…"; shortcut: "Ctrl+N"; enabled: false }                       // B6a
    QQC2.Action { id: importOpml; text: "Import OPML…"; shortcut: "Ctrl+O"; enabled: false }                // B6a
    QQC2.Action { id: exportOpml; text: "Export OPML…"; shortcut: "Ctrl+Shift+E"; enabled: false }          // B6a
    QQC2.Action {
        id: refresh
        text: "Refresh"
        shortcut: "Ctrl+R"
        enabled: bar.ready && Runs.canRefresh
        onTriggered: Runs.refreshSource(SourcesModel.selectedKey)
    }
    QQC2.Action {
        id: refreshAll
        text: "Refresh All Feeds"
        shortcut: "Ctrl+Shift+R"
        enabled: bar.ready && Runs.canRefresh
        onTriggered: Runs.refreshAll()
    }
    QQC2.Action { id: settings; text: "Settings…"; shortcut: "Ctrl+Shift+,"; enabled: false }               // B6b
    QQC2.Action { id: quit; text: "Quit Ressac"; shortcut: StandardKey.Quit; enabled: true; onTriggered: Qt.quit() }

    QQC2.Action {
        id: search
        text: "Search"
        shortcut: StandardKey.Find
        enabled: bar.ready
        onTriggered: bar.page.focusSearch()
    }

    QQC2.Action {
        id: unreadOnly
        text: "Show Unread Only"
        shortcut: "Ctrl+Alt+U"
        checkable: true
        checked: EntriesModel.unreadOnly
        enabled: bar.ready && !SourcesModel.isGarden
        onTriggered: {
            EntriesModel.unreadOnly = checked
            checked = Qt.binding(function () { return EntriesModel.unreadOnly })   // the model owns the state
        }
    }
    QQC2.Action {
        id: biggerText
        text: "Bigger Text"
        shortcut: StandardKey.ZoomIn
        enabled: bar.ready
        onTriggered: AppSettings.resize(bar.page.look, "bigger")
    }
    QQC2.Action {
        id: smallerText
        text: "Smaller Text"
        shortcut: StandardKey.ZoomOut
        enabled: bar.ready
        onTriggered: AppSettings.resize(bar.page.look, "smaller")
    }
    QQC2.Action {
        id: actualSize
        text: "Actual Size"
        shortcut: "Ctrl+0"
        enabled: bar.ready
        onTriggered: AppSettings.resize(bar.page.look, "actualSize")
    }
    QQC2.Action { id: seaChart; text: "Sea Chart"; shortcut: "Ctrl+Shift+M"; enabled: false }              // B7

    QQC2.Action {
        id: nextEntry
        text: "Next Entry"
        shortcut: "J"
        enabled: bar.letters && EntriesModel.canNavigate
        onTriggered: bar.page.perform("nextEntry")
    }
    QQC2.Action {
        id: previousEntry
        text: "Previous Entry"
        shortcut: "K"
        enabled: bar.letters && EntriesModel.canNavigate
        onTriggered: bar.page.perform("previousEntry")
    }
    QQC2.Action {
        id: advance
        text: "Scroll Down or Next Unread"
        shortcut: "Space"
        enabled: bar.letters && EntriesModel.canNavigate
        onTriggered: bar.page.perform("advance")
    }

    QQC2.Action {
        id: toggleRead
        text: EntriesModel.selectedRead ? "Mark as Unread" : "Mark as Read"
        shortcut: "M"
        enabled: bar.letters && EntriesModel.canToggle
        onTriggered: bar.page.perform("toggleRead")
    }
    QQC2.Action {
        id: toggleStar
        text: EntriesModel.selectedStarred ? "Unstar" : "Star"
        shortcut: "S"
        enabled: bar.letters && EntriesModel.canToggle
        onTriggered: bar.page.perform("toggleStar")
    }
    QQC2.Action {
        id: markAllRead
        text: "Mark All as Read"
        shortcut: "Ctrl+Shift+A"
        enabled: bar.ready && EntriesModel.canMarkAllRead
        onTriggered: EntriesModel.markAllRead()
    }
    QQC2.Action {
        id: fullArticle
        text: "Full Article"
        checkable: true
        checked: Reader.isShowingFullText
        enabled: Reader.canRequestFullText
        onTriggered: {
            Reader.requestFullText()
            checked = Qt.binding(function () { return Reader.isShowingFullText })   // Reader owns the state
        }
    }
    QQC2.Action {
        id: openInBrowser
        text: "Open in Browser"
        shortcut: "Ctrl+Return"
        enabled: Reader.link !== ""
        onTriggered: bar.page.openInBrowser()
    }

    Shortcut { sequence: "F5"; enabled: refresh.enabled; onActivated: refresh.trigger() }

    QQC2.Menu {
        id: fileMenu
        title: "&File"
        QQC2.MenuItem { action: addFeed }
        QQC2.MenuItem { action: importOpml }
        QQC2.MenuItem { action: exportOpml }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { action: refresh }
        QQC2.MenuItem { action: refreshAll }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { action: settings }
        QQC2.MenuItem { action: quit }
    }

    QQC2.Menu {
        id: editMenu
        title: "&Edit"
        QQC2.MenuItem { action: search }
    }

    QQC2.Menu {
        id: viewMenu
        title: "&View"
        QQC2.MenuItem { action: unreadOnly }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { action: biggerText }
        QQC2.MenuItem { action: smallerText }
        QQC2.MenuItem { action: actualSize }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { action: seaChart }
    }

    QQC2.Menu {
        id: goMenu
        title: "G&o"
        QQC2.MenuItem { action: nextEntry }
        QQC2.MenuItem { action: previousEntry }
        QQC2.MenuItem { action: advance }
    }

    QQC2.Menu {
        id: articleMenu
        title: "&Article"
        QQC2.MenuItem { action: toggleRead }
        QQC2.MenuItem { action: toggleStar }
        QQC2.MenuItem { action: markAllRead }
        QQC2.MenuSeparator {}
        QQC2.MenuItem { action: fullArticle }
        QQC2.MenuItem { action: openInBrowser }
    }
}
