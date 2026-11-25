import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.w3cdotorg.ressac

Item {
    id: page

    objectName: "readingPage"          // the window key filter finds the page by this name

    readonly property int progressInset: 10  // PROGRESS_INSET

    property alias allButton: allButton
    property alias unreadButton: unreadButton
    property alias sourcesPane: sourcesPane
    property alias starButton: starButton
    // The look on screen: Antiope's view over the hidden native panes, or the native panes alone.
    // An empty library is Native whatever the setting: First Run shows in the window's own chrome.
    property string look: SourcesModel.emptyLibrary ? "native" : AppSettings.effectiveAppearance
    readonly property bool antiope: look === "antiope"
    readonly property Item antiopeView: antiopeLoader.item
    // The reader the keys and the toolbar act on: Antiope's own while it is shown.
    readonly property Item activeReader: antiope && antiopeLoader.item ? antiopeLoader.item.reader : readerPane
    readonly property alias reader: readerPane
    readonly property alias garden: gardenPane
    // Native's search field or Antiope's prompt: the menus' letters stay off while typing.
    readonly property bool searchFocused: searchField.activeFocus || (antiopeView !== null && antiopeView.promptFocused === true)
    // The morning edition builds by itself, except in the screenshot walks that do not test it.
    readonly property bool editionTriggers: !Recette.active || Recette.walk === "journal"

    // The last key verb the window filter asked for and whether it acted (the walk reads them).
    property string lastVerb: ""
    property bool lastHandled: false
    // One Space at a time: set while a page-down is in flight, a second Space is spent.
    property bool advancing: false
    property int advanceGeneration: 0

    function focusWithin(item) {
        let f = page.Window.window ? page.Window.window.activeFocusItem : null
        while (f) {
            if (f === item)
                return true
            f = f.parent
        }
        return false
    }

    function focusName() {
        if (searchFocused) return "search"
        if (antiope && antiopeLoader.item && focusWithin(antiopeLoader.item)) return "antiope-" + Antiope.column
        if (focusWithin(sourcesPane)) return "sidebar"
        if (focusWithin(gardenPane)) return "garden"
        if (focusWithin(entriesPane)) return "entries"
        if (focusWithin(readerPane)) return "reader"
        return "other"
    }

    function focusSidebar() { sourcesPane.focusList() }
    function dragFeed(feedKey, targetKey) { return sourcesPane.dragFeed(feedKey, targetKey) }
    function focusGarden() { gardenPane.focusList() }
    function focusSearch() {
        if (antiope && antiopeLoader.item)
            antiopeLoader.item.openPrompt()
        else
            searchField.forceActiveFocus()
    }

    function openInBrowser() {
        if (Reader.link !== "")
            activeReader.openExternally(Reader.link)
    }

    // Space: scroll the page, and at its end move to the next unread entry (marking the one left as read).
    // A held Space only scrolls; a second Space while one is in flight is spent.
    function advance(held) {
        if (advancing)
            return
        advancing = true
        const generation = ++advanceGeneration
        const id = EntriesModel.selectedEntryId
        activeReader.pageDown(id, function (spentOrScrolled) {
            if (generation !== advanceGeneration)
                return
            advancing = false
            if (spentOrScrolled || held || EntriesModel.selectedEntryId !== id)
                return
            EntriesModel.selectNextUnread()
        })
        // A page that never answers must not wedge Space for good.
        advanceGuard.restart()
    }

    // The verb the window filter (cpp/shim.cpp, keys::route) asks for: true when the key was acted on or
    // spent, false to let it through to the focused item. A held one-shot key is spent, as on the Mac;
    // in Antiope its view answers first, and what it leaves (⌫, Esc, m, s…) goes to the Native table.
    function perform(verb) {
        let handled = verb === "spent"
        if (!handled && antiope && antiopeLoader.item)
            handled = antiopeLoader.item.perform(verb)
        if (handled) {
            lastVerb = verb
            lastHandled = true
            return true
        }
        switch (verb) {
        case "nextEntry":
            handled = EntriesModel.canNavigate
            if (handled) EntriesModel.selectNext()
            break
        case "previousEntry":
            handled = EntriesModel.canNavigate
            if (handled) EntriesModel.selectPrevious()
            break
        case "advance":
        case "advanceHeld":
            handled = EntriesModel.canNavigate
            if (handled) advance(verb === "advanceHeld")
            break
        case "toggleRead":
            handled = EntriesModel.canToggle
            if (handled) {
                if (EntriesModel.selectedRead) autoRead.stop()
                EntriesModel.toggleRead(EntriesModel.selectedRow)
            }
            break
        case "toggleStar":
            handled = EntriesModel.canToggle
            if (handled) EntriesModel.toggleStarred(EntriesModel.selectedRow)
            break
        case "toggleReadHeld":
        case "toggleStarHeld":
            handled = EntriesModel.canToggle        // spent: a held key never toggles again
            break
        // The native panes' own moves: never into panes Antiope hides.
        case "focusEntryList":
            if (antiope)
                break
            if (SourcesModel.isGarden) {
                gardenPane.focusList()
            } else {
                EntriesModel.selectFirstIfNone()
                entriesPane.focusList()
            }
            handled = true
            break
        case "focusSidebar":
            if (antiope)
                break
            sourcesPane.focusList()
            handled = true
            break
        }
        lastVerb = verb
        lastHandled = handled
        return handled
    }

    function syncFilter() {
        allButton.checked = !EntriesModel.unreadOnly
        unreadButton.checked = EntriesModel.unreadOnly
    }

    function reloadAll() {
        SourcesModel.reload()
        EntriesModel.reload()
        Reader.reload()
        Journal.reload()
        Garden.reload()
        Chart.noteChange()                       // the Sea Chart's debounced survey (ignored while its window is closed)
        if (antiope)
            Antiope.reload()
    }

    // After own entry writes the list already shows: the counts only (sidebar badges, the Journal's
    // header, the Sea Chart, Antiope's pages), never the entries list (spec §7.1 "Live data").
    function reloadCounts() {
        SourcesModel.reload()
        Journal.reload()
        Chart.noteChange()
        if (antiope)
            Antiope.reload()
    }

    // Coalesces the counts reloads: `EntriesModel.settled` gives the due time (200 ms after the last
    // write, at most 1 s after the first of a burst, `ressac_ui::live::Settle`).
    Timer {
        id: settleTimer
        onTriggered: { EntriesModel.settleFired(); page.reloadCounts() }
    }

    // Back in Native the keyboard goes to the sidebar, never to the destroyed Antiope view, so the
    // letters work at once; an open prompt closes first without taking the focus back (its text
    // stays). Entering Antiope, the view's `onLoaded` focuses its column.
    onAntiopeChanged: {
        if (antiope)
            return
        if (antiopeLoader.item)
            antiopeLoader.item.closePrompt(false, false)
        if (!SourcesModel.emptyLibrary)
            focusSidebar()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        visible: !page.antiope

        QQC2.ToolBar {
            Layout.fillWidth: true

            RowLayout {
                anchors.fill: parent
                spacing: Kirigami.Units.smallSpacing

                Item { Layout.fillWidth: true }

                Kirigami.SearchField {
                    id: searchField
                    objectName: "search"
                    placeholderText: "Search"
                    text: EntriesModel.searchText
                    onTextChanged: EntriesModel.searchText = text
                    Keys.onEscapePressed: text = ""
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                }

                RowLayout {
                    spacing: 0
                    enabled: !SourcesModel.isGarden

                    QQC2.ButtonGroup { id: filterGroup }

                    // `checked` is synced imperatively from the model (syncFilter), never bound: a click and the
                    // group's exclusive uncheck write it directly. A click writes the model through onToggled.
                    QQC2.ToolButton {
                        id: allButton
                        text: "All"
                        checkable: true
                        QQC2.ButtonGroup.group: filterGroup
                        onToggled: if (checked) EntriesModel.unreadOnly = false
                    }

                    QQC2.ToolButton {
                        id: unreadButton
                        text: "Unread"
                        checkable: true
                        QQC2.ButtonGroup.group: filterGroup
                        onToggled: if (checked) EntriesModel.unreadOnly = true
                    }
                }

                QQC2.ToolButton {
                    visible: !Runs.progressVisible
                    icon.name: "view-refresh"
                    text: "Refresh"
                    display: QQC2.AbstractButton.IconOnly
                    enabled: Runs.canRefresh
                    onClicked: Runs.refreshSource(SourcesModel.selectedKey)
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ProgressBar {
                    visible: Runs.progressVisible
                    implicitWidth: Runs.progressWidth
                    indeterminate: Runs.progressFraction < 0
                    value: Math.max(0, Runs.progressFraction)

                    TextMetrics { id: fullMetrics; font: pillLabel.font; text: Runs.progressText }

                    QQC2.Label {
                        id: pillLabel
                        anchors.fill: parent
                        anchors.leftMargin: page.progressInset
                        anchors.rightMargin: page.progressInset
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        text: fullMetrics.width <= Runs.progressWidth - 2 * page.progressInset ? Runs.progressText : Runs.progressCompactText
                    }

                    HoverHandler { id: pillHover }
                    QQC2.ToolTip.text: Runs.progressText
                    QQC2.ToolTip.visible: pillHover.hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolSeparator {}

                QQC2.BusyIndicator {
                    visible: Reader.isExtracting
                    running: visible
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: implicitWidth
                }

                QQC2.ToolButton {
                    text: "Full Article"
                    icon.name: "view-readermode"
                    display: QQC2.AbstractButton.TextBesideIcon
                    checkable: true
                    checked: Reader.isShowingFullText
                    enabled: Reader.canRequestFullText
                    // The state is Reader's: take the click's own toggle back.
                    onClicked: { Reader.requestFullText(); checked = Qt.binding(function () { return Reader.isShowingFullText }) }
                    QQC2.ToolTip.text: Reader.fullTextHelp
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolButton {
                    id: starButton
                    // The list's row, as the menu: an own star toggle shows at once (its optimistic row);
                    // the Reader's detail only refreshes on a full reload.
                    text: EntriesModel.selectedStarred ? "Unstar" : "Star"
                    icon.name: EntriesModel.selectedStarred ? "starred-symbolic" : "non-starred-symbolic"
                    display: QQC2.AbstractButton.IconOnly
                    enabled: Reader.hasEntry && EntriesModel.selectedRow >= 0
                    onClicked: EntriesModel.toggleStarred(EntriesModel.selectedRow)
                    QQC2.ToolTip.text: "Star (S)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolButton {
                    text: "Open in Browser"
                    icon.name: "internet-web-browser"
                    display: QQC2.AbstractButton.IconOnly
                    enabled: Reader.link !== ""
                    onClicked: page.openInBrowser()
                    QQC2.ToolTip.text: "Open in Browser (Ctrl+Return)"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }

                QQC2.ToolButton {
                    text: "Copy Link"
                    icon.name: "edit-copy"
                    display: QQC2.AbstractButton.IconOnly
                    enabled: Reader.link !== ""
                    onClicked: readerPane.copyText(Reader.link)
                    QQC2.ToolTip.text: "Copy Link"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                }
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: Runs.bannerVisible
            text: Runs.bannerMessage
            showCloseButton: false
            actions: [
                Kirigami.Action {
                    text: "Show Log"
                    visible: Runs.bannerShowLog
                    onTriggered: Qt.openUrlExternally("file://" + encodeURI(Runs.logDir))
                },
                Kirigami.Action {
                    text: "Dismiss"
                    icon.name: "window-close"
                    displayHint: Kirigami.DisplayHint.IconOnly
                    onTriggered: Runs.dismissBanner()
                }
            ]
        }

        QQC2.SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal

            SourcesPane {
                id: sourcesPane
                QQC2.SplitView.minimumWidth: 180
                QQC2.SplitView.preferredWidth: 220
                QQC2.SplitView.maximumWidth: 320
            }

            Item {
                // The garden's table and header need more room than the entry list.
                QQC2.SplitView.minimumWidth: SourcesModel.isGarden ? 440 : 260
                QQC2.SplitView.preferredWidth: SourcesModel.isGarden ? 520 : 320
                QQC2.SplitView.maximumWidth: SourcesModel.isGarden ? 760 : 480

                EntriesPane { id: entriesPane; anchors.fill: parent; visible: !SourcesModel.isGarden }
                GardenPane { id: gardenPane; anchors.fill: parent; visible: SourcesModel.isGarden }
            }

            Item {
                QQC2.SplitView.fillWidth: true
                QQC2.SplitView.minimumWidth: 300

                ReaderPane { id: readerPane; anchors.fill: parent; visible: !SourcesModel.isGarden; active: !page.antiope }
                GardenInspector {
                    anchors.fill: parent
                    visible: SourcesModel.isGarden
                    onOpenUrl: (url) => readerPane.openExternally(url)
                }
            }
        }
    }

    // The Antiope look (spec §7.9): made when it is chosen, dropped when Native comes back.
    Loader {
        id: antiopeLoader
        anchors.fill: parent
        active: page.antiope
        source: Qt.resolvedUrl("AntiopeView.qml")
        onLoaded: item.focusColumn()
    }

    Timer { id: advanceGuard; interval: 3000; onTriggered: { page.advanceGeneration++; page.advancing = false } }

    // 1 s on an unread selection marks it read (the selection must stay on it).
    Timer {
        id: autoRead
        interval: EntriesModel.autoReadDelay
        property real entryId: -1
        onTriggered: EntriesModel.autoRead(entryId)
    }

    Connections {
        target: EntriesModel
        function onSelectionChanged() {
            if (EntriesModel.selectedEntryId !== autoRead.entryId) {
                autoRead.entryId = EntriesModel.selectedEntryId
                if (autoRead.entryId >= 0)
                    autoRead.restart()
                else
                    autoRead.stop()
            }
        }
    }

    Timer { interval: Live.pollInterval; repeat: true; running: Live.polling; onTriggered: Live.poll() }
    Timer {
        interval: 60000; repeat: true; running: true
        onTriggered: { Live.minuteTick(); if (page.editionTriggers) Journal.minuteTick() }
    }

    Connections {
        target: Qt.application
        function onStateChanged() {
            if (Qt.application.state === Qt.ApplicationActive && page.editionTriggers)
                Journal.trigger()
        }
    }

    Binding {
        target: Live
        property: "windowActive"
        value: page.Window.window !== null && page.Window.window.active && page.Window.window.visibility !== Window.Minimized
    }

    Connections { target: EntriesModel; function onUnreadOnlyChanged() { page.syncFilter() } }
    Connections { target: Garden; function onOpenUrl(url) { readerPane.openExternally(url) } }
    Connections { target: Reader; function onBanner(m, l) { Runs.showBanner(m, l) } }
    Connections { target: Journal; function onBanner(message, showLog) { Runs.showBanner(message, showLog) } }
    Connections { target: Reader; function onWrote() { page.reloadAll() } }
    Connections { target: Live; function onInvalidated() { page.reloadAll() } }
    Connections { target: Runs; function onDatabaseChanged() { page.reloadAll() } }
    Connections { target: SourcesModel; function onWrote() { page.reloadAll() } }
    Connections { target: EntriesModel; function onWrote() { page.reloadAll() } }
    Connections { target: EntriesModel; function onSettled(dueMs) { settleTimer.interval = Math.max(1, dueMs); settleTimer.restart() } }
    Connections { target: SourcesModel; function onBanner(m, l) { Runs.showBanner(m, l) } }
    Connections { target: EntriesModel; function onBanner(m, l) { Runs.showBanner(m, l) } }
    Connections {
        target: SourcesModel
        function onSelectionChanged() {
            EntriesModel.showSource(SourcesModel.selectedKey)
            Garden.show(SourcesModel.selectedKey)
            if (page.antiope)
                return                       // Antiope's view keeps the keyboard in its column
            if (SourcesModel.isGarden)
                gardenPane.focusList()
            else if (SourcesModel.selectedKey === "journal")
                entriesPane.focusList()      // as on the Mac, the list takes the keyboard in the Journal
        }
    }

    Binding { target: Reader; property: "entryId"; value: EntriesModel.selectedEntryId }
    Binding {
        target: Reader; property: "colorScheme"
        value: Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark ? "dark" : "light"
    }
    Binding { target: Reader; property: "look"; value: page.look }
    Binding { target: Reader; property: "readerFont"; value: AppSettings.readerFont }
    Binding { target: Reader; property: "textSize"; value: page.look === "antiope" ? AppSettings.readerSizeAntiope : AppSettings.readerSize }
    Binding { target: Reader; property: "readerWidth"; value: AppSettings.readerWidth }

    Component.onCompleted: {
        page.syncFilter()
        void Runs.canRefresh     // creates Runs now: the edition's build starts its run through it
        Reader.reload()
        SourcesModel.reload()
        EntriesModel.showSource(SourcesModel.selectedKey)
        Journal.reload()
        if (page.editionTriggers)
            Journal.trigger()
    }
}
